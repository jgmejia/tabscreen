# 25. Graceful shutdown on Ctrl+C/SIGTERM, and closing the "mystery flicker" from ADR 0024

Date: 2026-08-20

## Status

Accepted.

## Context

After reverting to `Placement::Right` (ADR 0024), live-testing the
landscape switch (right after this session's questions 2-5) surfaced a
new, alarming symptom: polling `xrandr --query` between checks showed
`Virtual-2-1` in a state that should have been *impossible* given the
current code -- active with mode `1200x1920_60.00` (portrait) and with
`eDP-1`/`HDMI-1-0` shifted, exactly matching the *old*, already-reverted
`Placement::Left` behavior. `TABLET_WIDTH`/`HEIGHT` are hardcoded to
`1920`/`1200` now, and `PLACEMENT` to `Right`, which never touches the
real outputs at all -- there was no code path left that could produce
what was observed. Immediately stabilized the desktop (`xrandr --off`
plus restoring positions) and stopped the daemon rather than keep
live-testing against an unexplained symptom.

**The real gap, identified once the user Ctrl+C'd the daemon:** the
process had no signal handler. `SIGINT` (Ctrl+C) aborts a Rust process
immediately by default -- `ActiveOutput`'s `Drop` impl, which is
responsible for turning the tabscreen output back off and restoring any
shifted outputs, never runs. Whatever state existed at the exact moment
of Ctrl+C is what's left behind, permanently, until something else
(a human running `xrandr`, or the next `heal_layout_gap` check at the
*next* daemon startup) cleans it up.

This reframed the "mystery" entirely: this session had been repeatedly
Ctrl+C'ing the daemon and restarting it throughout troubleshooting
(reinstalling the Android app, testing the Left-placement bug, etc.).
Each of those Ctrl+C's froze whatever transient mid-connection-cycle
state existed at that instant -- and the *next* daemon startup's first
`xrandr --query` (run by this session for diagnosis) would see that
frozen leftover, not a fresh, deterministic state. The two "impossible"
snapshots weren't a new code bug; they were stale artifacts from
*previous* ungraceful kills, observed *after* a fresh restart, before
that fresh instance's own `activate()` had run yet or overwritten them.

**Confirmed by finally getting real visibility into the daemon's own
logs**, rather than inferring everything from `xrandr` polling snapshots
(this session's practice up to this point, forced by the daemon running
in the user's own terminal rather than somewhere this session could
capture output from): running `./target/debug/host` directly (turned out
not to need `sudo` at all in this environment -- the AOA device was
already claimed in accessory mode from the user's prior session, so no
fresh USB permission was needed) with output redirected to a log file
showed `activated virtual output Virtual-2-1: 1920x1200+6400+0 (Right)`
immediately and correctly, and a 45-second, 9-sample polling loop against
that same run showed **zero flicker, zero deviation** -- rock solid at
the correct geometry the entire time. The code was never wrong.

## Decision

Added a Ctrl+C/SIGTERM/SIGHUP handler (`ctrlc` crate, `termination`
feature -- confirmed live that the crate's default build only covers
`SIGINT`, and `timeout`'s kill signal (`SIGTERM`) reproduced the same
frozen-state problem until this feature was enabled) that calls a new
`display::deactivate_any_active()`: rather than requiring a handle to
whatever `ActiveOutput` might currently exist (awkward to reach from a
separate signal-handling thread), it just queries RandR directly for any
currently-active `Virtual-*` output and turns it off, unconditionally.
Opens its own short-lived X11 connection rather than sharing the main
one, since a signal can land at an arbitrary point in the program,
including before the main connection is even established.

Live-confirmed both signals now clean up correctly: `kill -INT` and
`timeout`'s `SIGTERM` both now log `interrupted, cleaning up before
exit...` followed by `cleanup on exit: deactivating active tabscreen
output Virtual-2-1`, and leave `Virtual-2-1` fully disconnected
afterward -- matching a clean, intentional shutdown instead of an
abandoned mid-session freeze.

## Consequences

- This closes out the real, if narrow, root cause: not a RandR
  positioning bug, not GNOME/mutter interference beyond what ADR 0024
  already found, but simply that this daemon had never been able to shut
  down cleanly under any signal, and this session's own repeated
  Ctrl+C-driven iteration was what generated the confusing snapshots.
- Directly relevant to Phase 6: a systemd service stops units via
  `SIGTERM` by default, which is now handled -- this would have been a
  real bug in production, not just a live-testing artifact, if it had
  shipped without this fix.
- **Running the daemon directly (piped to a log file) rather than via the
  user's interactive `sudo` terminal is a much better default debugging
  approach going forward** when live testing is needed -- it gives this
  session direct, real-time visibility into `log::info!`/`log::warn!`
  output instead of inferring state purely from external `xrandr`
  polling, which this incident showed can be actively misleading (stale
  frozen state looking identical to a live bug). Confirmed this
  environment doesn't actually need `sudo` for the USB layer once the
  AOA device is already in accessory mode from a prior session -- worth
  trying without `sudo` first in future sessions rather than assuming it's
  required.
