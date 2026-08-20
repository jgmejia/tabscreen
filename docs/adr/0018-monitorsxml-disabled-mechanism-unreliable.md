# 18. `monitors.xml`'s `<disabled>` mechanism is unreliable, not a race to fix

Date: 2026-08-19

## Status

Accepted. Consolidates three genuine, protocol-verified trials (config
presence confirmed via the Xorg log at the time of each observation, per
0017's corrected protocol) into a clear conclusion: this mechanism does not
reliably keep the virtual output off by default.

## Context

Following 0017's correction (the earlier "non-determinism" claim was
invalid — it compared a session with the config active against one
without), retested once more with a tightened protocol: confirm the Xorg
log shows `ConnectedMonitor`/`CustomEDID` active for the specific session
being evaluated, *before* drawing any conclusion, and have the user
directly watch and report in real time rather than reconstructing after
the fact.

**Result:** user logged in once (`19:18:30`ish, confirmed PAM session at
`19:18:37`), watched a blank screen with an unresponsive cursor on `eDP-1`
for a full 2.5 minutes without touching anything, then fell back. Checked
immediately: PID `3679`'s Xorg log confirms the config was genuinely
active, and shows the same broken pattern seen in every greeter failure
(0012, 0013, 0015) and today's earlier session:
```
(II) NVIDIA(0): Setting mode "DFP-4:nvidia-auto-select+2560+0,DFP-1:1200x1920+6400+0"
(II) NVIDIA(0): Setting mode "NULL"
(II) NVIDIA(0): Setting mode "DP-1: nvidia-auto-select @1200x1920 +0+0..."
```

This is a clean, unambiguous data point: config confirmed active, user
directly watched, genuinely broken. Combined with the two prior genuine
trials (0016's session — correct state, severe stall; today's earlier
session — wrong state, not directly watched by the user in real time):

| Trial | Config confirmed active | `DFP-1` state | Outcome |
|---|---|---|---|
| 0016 | Yes | Correct (off) | 6m40s stall, usability never confirmed |
| Today, session 1 | Yes | Wrong (on) | Broken, not directly observed |
| Today, this trial | Yes | Wrong (on) | Broken, **directly observed by user** |

Two of three genuine trials show the same broken re-activation sequence.
The one case where the state was correct still never produced a confirmed
usable desktop. **No trial across this entire investigation has produced
both a correct state and confirmed usability.**

## Decision

Stop treating this as a race condition or a nearly-working mechanism that
needs one more tweak. The evidence across three independent trials points
to `monitors.xml`'s `<disabled>` entry simply not reliably preventing
`ConnectedMonitor`-driven auto-activation, at least not in the shape
applied here (system-level `/etc/xdg/monitors.xml` plus a `gdm`-owned copy
at `/var/lib/gdm3/.config/monitors.xml`). Further blind retries of this
same shape are not a good use of more live-hardware attempts.

Real options going forward, per 0013's original candidate list (now with
much more evidence behind ruling out "just needs the right config"):

1. **Make `ConnectedMonitor` genuinely dynamic** — applied by the host
   daemon only at the moment the tablet actually connects (writing the
   Xorg config fragment and triggering an Xorg/GDM restart at that exact
   moment), rather than pre-declared at boot and expected to stay off.
   This sidesteps the whole "keep it off by default" problem by never
   having it in a state where the driver claims it's connected unless the
   tablet genuinely is. Cost: requires an Xorg/GDM restart on every
   tablet connect/disconnect, which is disruptive (kicks the user's
   session) unless there's a way to apply it without a full restart —
   not yet researched.
2. **Scope reduction**: accept the virtual output being active from boot
   (the "broken" state is only broken because nothing is meant to render
   there yet — if real content were always intended to be shown there,
   "always on" might be acceptable). This technically satisfies
   `CLAUDE.md`'s "real RandR output" requirement without solving the
   toggle problem. Would need explicit product-scope buy-in.
3. **A different validation environment** — ADR 0006 raised this and it
   was deferred; given how much live-hardware iteration this specific
   machine has absorbed (roughly 9 attempts today across multiple
   mechanisms), a VM or secondary machine for further experimentation may
   now be worth the setup cost it was deferred over earlier.

## Consequences

- Phase 2 remains blocked. This entry closes out the `monitors.xml`
  `<disabled>` investigation (0014 through 0018) with a negative but clear
  result, rather than leaving it open-ended.
- The underlying driver-level mechanism (`ConnectedMonitor`/`CustomEDID`/
  `MetaModes` correctly creating and, when it works, correctly hiding a
  real RandR output) is still validated as fundamentally sound — 0012 and
  0016 both showed it *can* produce the correct state. What's unreliable
  is specifically the "stay off by default" part via a static saved
  config.
- Recommend pausing further live-hardware attempts on this machine until
  a decision is made among the three options above, rather than continuing
  to iterate the same shape of fix.
