# 11. Bare NVIDIA `Device` fragment fails: no effect, plus a login-time crash

Date: 2026-08-19

## Status

Accepted — records the empirical result of 0010's proposed first attempt.
Does not rule out `ConnectedMonitor`/`CustomEDID` entirely, but rules out
the *bare-fragment* injection technique 0010 assumed would work by analogy
to `Coolbits`.

## Context

Applied 0010's proposed config exactly as written, to
`/etc/X11/xorg.conf.d/90-tabscreen-nvidia-edid.conf`:

```
Section "Device"
    Identifier  "TabScreen NVIDIA"
    Driver      "nvidia"
    Option      "ConnectedMonitor" "DFP-1"
    Option      "CustomEDID" "DFP-1:/lib/firmware/edid/tabscreen-honor-pad-x8a.bin"
EndSection
```

Applied via full reboot (not a live `systemctl restart gdm`), per 0010's own
stated preference given 0005's history. Result, reconstructed from
`journalctl -b -1` after the fact (the user recovered via VT-switch before
this diagnosis happened):

1. **The option never took effect.** Both Xorg instances that boot (the
   greeter's and the post-crash recovery one GDM spawned automatically,
   see below) logged `DFP-1: disconnected` repeatedly throughout their
   runtime — `ConnectedMonitor "DFP-1"` did not change what NVIDIA reported
   as connected.
2. **Xorg crashed at the exact moment of login (greeter → session
   handoff).** The first Xorg process that boot (PID 2236) logged, within
   6 seconds of the user's password being accepted:
   `(EE) modeset(0): Failed to set CTM property: -13`,
   `(EE) modeset(0): failed to set mode: No such file or directory`, then
   `(WW) NVIDIA(G0): Failed to set the display configuration` with
   `Setting a mode on head 0/1/2/3 failed: Insufficient permissions` — all
   four heads, not just the targeted `DFP-1`, and on **both** drivers
   (NVIDIA and Intel's `modeset(0)`) simultaneously. Earlier in the same
   process's log: `xf86EnableIO: failed to enable I/O ports... Operation
   not permitted` and `(EE) open /dev/fb0: Permission denied`.
3. **GDM self-healed automatically**, spawning a second Xorg process
   (PID 3652) about one second after the first crashed, which ran
   successfully for the rest of that boot (51 minutes, until the user's
   own reboot) — no forced shutdown was needed, unlike 0005's incident.
   That second process *also* logged `DFP-1: disconnected` throughout,
   consistent with finding 1.
4. The user didn't wait to observe GDM's self-heal — reasonably, given
   0005's history — and recovered manually via VT-switch (`Ctrl+Alt+F3`),
   deleting the config file. Current session (post-cleanup, post-reboot)
   confirmed fully healthy: `eDP-1` and `HDMI-1-0` both connected and
   working normally, `/etc/X11/xorg.conf.d/` empty.

**On causation:** the crash pattern (every head failing at once, across two
unrelated drivers, all "Insufficient permissions") looks more consistent
with a DRM-master contention/handoff race during the greeter-to-session
transition than with something specific to the `DFP-1`/`CustomEDID`
content — a real, if less dramatic, monitor-mode-setting error would more
plausibly be scoped to the one display device being reconfigured, not every
head on every driver simultaneously. That said, the fragment being present
is the only variable that changed relative to every other clean boot this
session, so it can't be dismissed as pure coincidence either — the honest
position is "correlated, causation not proven."

## Decision

- Do not retry `DFP-3`/`DFP-5` with the same bare-fragment technique as-is
  — finding 1 (the option had no effect at all) would very plausibly repeat
  identically regardless of which connector name is used, since the
  fragment doesn't appear to be binding to the right place at all. Trying a
  different connector name doesn't address why the option didn't apply.
- The likely fix, not yet attempted: match the *shape* of the one precedent
  that's actually confirmed working (0010's kovasky.me reference) more
  closely — that setup used a full `Device` + `Screen` (with explicit
  `metamodes` covering both the real and virtual outputs together), not a
  bare `Device`-only fragment. `ConnectedMonitor`/`CustomEDID` may need to
  be paired with an explicit `Screen`/`ServerLayout` that fully describes
  both `DFP-4` (the real monitor) and the target virtual connector together,
  rather than assuming autoconfig will merge a partial fragment the way it
  does for simpler global options like `Coolbits`.
- Given this is now three failed live-hardware attempts across three
  different mechanisms (0005/0006's dummy driver, 0008/0009's kernel EDID
  parameter, 0010/0011's bare NVIDIA fragment) — one of which caused a real
  crash — the next attempt should not proceed without deliberately
  reconsidering the approach first, not just iterating connector names.

## Consequences

- No lasting damage: confirmed the live session is fully healthy post-
  recovery, both real outputs (`eDP-1`, `HDMI-1-0`) working normally.
- `ConnectedMonitor`/`CustomEDID` are not ruled out as a mechanism — only
  the specific bare-fragment injection shape 0010 proposed is ruled out.
  The EDID file itself (`host/resources/edid/tabscreen-honor-pad-x8a.bin`)
  remains valid and reusable in any future attempt.
- This crash, even though self-healing, is a second data point (after
  0005) that Xorg-config changes on this machine's login path carry real
  risk beyond "it just doesn't work" — worth treating any future live
  attempt with at least the same caution as 0005/0008/0010 required, and
  worth explicitly deciding with the user whether to keep iterating on this
  machine's live session or find a different validation environment (a
  question 0006 already raised and deferred).
