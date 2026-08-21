# 34. A full X session crash during manual arandr reconfiguration recovered fully automatically

Date: 2026-08-20

## Status

Accepted.

## Context

While manually rearranging monitors in arandr (real-time, interactive
use — not automated by the daemon), the entire X session crashed and
GDM rebuilt it from scratch: a brand new `Xorg` process, a brand new
`gnome-shell`/`gnome-session`/`dbus-daemon` generation, and (because
Ubuntu ties the `systemd --user` instance's lifecycle to the login
session) a brand new `systemd --user` manager — which killed and then
auto-restarted `tabscreen.service` as a side effect, independent of
anything this project's own code did.

**Root cause of the crash itself**, found in the system journal (not
`tabscreen.service`'s own log): a fatal X protocol error just before
the session died --

```
X Error of failed request:  BadValue (integer parameter out of range for operation)
  Major opcode of failed request:  140 (RANDR)
  Minor opcode of failed request:  35 (RRSetProviderOutputSource)
  Value in failed request:  0x43
```

`RRSetProviderOutputSource` is a multi-GPU/PRIME output-routing RandR
call, not something a plain `--pos`/`--mode` arandr change issues
directly — the far more likely source is mutter's own automatic
monitor-reconfiguration logic (the same "reactive relayout" behavior
first seen in ADR 0024) reacting to the live arandr change by trying to
route an output through a GPU *provider*, and choosing an invalid
target. `vkms` is a genuinely separate RandR *provider* (a real virtual
GPU, not just another output on an existing one — ADR 0021's whole
reason it works safely for the base case), and this is the first
observed case where that separateness causes a problem rather than
preventing one: NVIDIA's own provider-based PRIME/output-offload code
path, when mutter invokes it during reconfiguration, doesn't appear to
handle a third (`vkms`) provider gracefully in every situation.

This is a genuinely different failure mode from ADR 0032/0033's output
*position* drift — this is the X server itself dying, not just the
tabscreen output moving.

## What actually happened, reconstructed from both logs

1. The daemon's own drift-check (ADR 0033, deployed minutes earlier)
   caught the first visible symptom: `Virtual-2-1` had drifted to
   `6400,0,1024x768` -- `1024x768` is `vkms`'s own internal fallback
   mode (Phase 0's spike notes on `DUMMY0`'s built-in mode list), a
   sign the RandR state was already going through a corrupted
   transition moments before the crash. It correctly forced a
   reconnect.
2. Before that reconnect could complete, `systemd --user` itself
   stopped `tabscreen.service` — the old session (and everything tied
   to it) was being torn down by GDM at that same moment.
3. The *new* session's `systemd --user` auto-started `tabscreen.service`
   fresh (already `enable`d, Phase 6) a few seconds later. It connected
   to the tablet fine (still in AOA accessory mode) but hit the same
   `BadMatch`/`Permission denied` capture failure activating against
   the brand-new X session — plausibly a timing race with GNOME's own
   post-crash monitor auto-configuration still settling.
4. ADR 0033's encoder-death check caught *that* failure too and forced
   one more reconnect.
5. That reconnect succeeded cleanly. Confirmed stable (no further
   errors, `ffmpeg` healthy) for several minutes afterward with zero
   manual intervention from either the user or this session.

## Decision

Do not attempt to fix `RRSetProviderOutputSource`/mutter's provider
handling itself — this is Xorg/mutter/NVIDIA-driver internal behavior,
well outside this project's own code, and not something reproducible
or debuggable from here (no core dump, no reliable repro sequence
identified, only correlated with "was reconfiguring monitors in
arandr"). Consistent with this project's standing priority (a working
state over a complete one): the practical mitigation is already in
place and just proved itself under a real, more severe version of the
failure it was built for.

## Consequences

- No code change needed from this incident — ADR 0033's health check
  and Phase 6's existing systemd auto-restart, working together (not
  by original design, but compatibly), turned out sufficient to recover
  from a strictly worse failure than either was individually built for.
- This is a real, if apparently rare, risk specific to this machine's
  three-provider GPU topology (NVIDIA + Intel + `vkms`) that arandr
  users should know about — added to `README.md`'s Known limitations:
  manually reconfiguring monitors while tabscreen is connected carries
  a small risk of crashing the whole X session, but recovery is now
  fully automatic.
- No data was lost beyond what any GNOME session crash already costs
  (open windows/layout from before the crash) — not a tabscreen-specific
  cost, and not something this project can mitigate.
- Worth keeping in mind for any future investigation of mutter's
  "reactive relayout" behavior (still not root-caused, per ADR 0024):
  this incident shows it can escalate past just repositioning outputs
  into a fatal provider-routing call, at least on this particular
  three-provider GPU topology.
