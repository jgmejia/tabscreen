# 33. Automatic recovery from a dead video encoder or a drifted output position

Date: 2026-08-20

## Status

Accepted. Live-validated by reproducing the exact real-world failure from
ADR 0032 and confirming automatic recovery.

## Context

ADR 0032 root-caused a real incident: deactivating a real monitor
(`HDMI-1-0`) via arandr mid-connection shifted the tabscreen `Virtual-2-1`
output's position (mutter's known "reactive relayout" behavior, first
seen in ADR 0024). `video.rs`'s `ffmpeg` capture was still reading its
*old* fixed offset, which fell outside the new (shrunk) screen bounds,
producing a real X `BadMatch` error and killing the encoder process.
Nothing detected this: `video_writer_loop` ran in its own thread and,
on the encoder dying, just logged a warning and quietly returned — the
main connection loop kept running (still injecting input) with no video
for the rest of that connection cycle, indefinitely, until something
else happened to end it. The only fix at the time was a manual
`systemctl --user restart tabscreen.service`.

## Decision

Added a periodic in-connection health check (`main.rs`'s
`check_video_health`, called every `HEALTH_CHECK_INTERVAL` = 5s from the
main INPUT-reading loop) that detects both failure modes and forces a
reconnect by returning `Err` from `run_one_connection` — reusing the
*existing* outer retry loop and `display::activate()`'s already-correct
"recompute position from current layout" logic, rather than writing new
incremental repositioning logic:

1. **Dead encoder**: `video_writer_loop` now takes a shared
   `Arc<AtomicBool>` (`video_alive`) and sets it `false` on either exit
   path (`ffmpeg` exiting or a USB write failing) before returning. The
   health check bails if it's `false`.
2. **Drifted output**: new `ActiveOutput::current_rect()` in
   `display.rs` re-queries the tabscreen output's live CRTC geometry
   directly from X11. The health check bails if it no longer matches
   the `OutputRect` captured at activation time, or if the output has
   no active CRTC at all (turned off outside the daemon).

`READ_TIMEOUT` (bounding how long a single `read_bulk` call blocks) was
also shortened from 30s to 3s, so the health check still gets a chance
to run promptly even when the tablet sends no touch input at all —
decoupled from the 5s check interval itself via a separate `Instant`
timer, so a burst of touch events doesn't turn the check into an X11
round-trip on every single read.

Since the AOA device stays in accessory mode across this kind of
reconnect (no physical unplug happened), `run_one_connection`'s existing
"device already in accessory mode, skipping handshake" fast path applies
— a forced reconnect is cheap, not a full USB re-enumeration.

## Consequences

**Live-validated by reproducing the actual bug, not just a synthetic
test:** repositioned `Virtual-2-1` directly via `xrandr --pos 0x0` while
the daemon was running against a live tablet connection. Full recovery
sequence observed in the daemon's own log:

- `ffmpeg` hit the identical `BadMatch`/`Continuing without shared
  memory`/`Permission denied` failure as the original incident and
  exited.
- The health check caught it 3s later (`"video encoder died mid-
  connection, forcing reconnect to recover"`).
- Reconnect skipped the AOA handshake (device still in accessory mode),
  completed `HELLO`/`HELLO_ACK`/`DISPLAY_INFO`, and re-activated
  `Virtual-2-1` at a freshly-computed correct position (`+3840+0`,
  matching the layout live at that moment) — all automatic.
- Total recovery time: ~6 seconds, no manual intervention.

A second live test (toggling `HDMI-1-0` off again afterward, restoring
the user's own prior layout choice) caused **no** false-positive
reconnect — `Virtual-2-1`'s position happened not to drift that time,
and the health check correctly did nothing. Confirms the check isn't
trigger-happy on every RandR event, only on an actual mismatch.

- This does not fix mutter's underlying "reactive relayout" behavior
  itself (still not understood at the root, per ADR 0024/0032) — it
  makes the daemon resilient *to* it instead, which is a better fit for
  this project's own priority (a working state over a complete one) and
  doesn't require understanding or controlling GNOME/mutter internals
  the project has no first-party visibility into.
- A drifted-but-still-reconnected output can end up positioned
  suboptimally (e.g. leaving a dead gap in the X screen where a
  deactivated real monitor used to be) until the *next* connection
  cycle recomputes it fresh — cosmetic, not functional; confirmed the
  video capture itself still works correctly at the interim position.
- The user-visible effect of a recovery cycle is the same brief
  reconnect blip already normal for this daemon (input pauses for ~1-2s,
  the Android client's status overlay reappears momentarily per its own
  hide/show logic, then the video resumes) — not a new failure mode to
  document separately, just this existing behavior now triggered
  automatically instead of requiring a manual restart.
- Closes the specific, reproduced half of Phase 7's "graceful handling
  of encode/decode errors and dropped frames" item — the encoder-death
  and output-drift cases are now self-healing. Per-frame decode error
  handling on the Android side and other dropped-frame scenarios are
  still open.
