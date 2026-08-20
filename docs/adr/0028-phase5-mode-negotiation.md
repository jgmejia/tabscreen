# 28. Phase 5: real mode negotiation over the wire, replacing hardcoded constants

Date: 2026-08-20

## Status

Accepted. Live-validated end-to-end.

## Context

Since Phase 2, `host/src/main.rs` activated the tabscreen output using
hardcoded `TABLET_WIDTH`/`TABLET_HEIGHT`/`TABLET_REFRESH_HZ` constants
(the HONOR Pad X8a's confirmed real panel spec from Phase 0) rather than
anything the tablet actually reported. Phase 5's job was to replace that
assumption with real negotiation over the wire.

## Decision

Added `DISPLAY_INFO` (type `0x03`) to the `CONTROL` channel
(`docs/wire-protocol.md`): `width: u16LE, height: u16LE, refresh_hz: u8`,
sent client → host immediately after `HELLO_ACK`. The host
(`wait_for_display_info` in `main.rs`) reads CONTROL frames for up to
`HELLO_ACK_TIMEOUT` (5s) looking for it before activating the display;
a timeout falls back to the old hardcoded constants (renamed
`FALLBACK_TABLET_*`) rather than failing the connection outright, so an
old or misbehaving client doesn't hard-break.

Two of the three sub-decisions `TASKS.md` originally flagged as open
turned out to already be settled by earlier work this session, not new
decisions to make here:

- **Refresh rate**: `DISPLAY_INFO` reports the display's *current*
  refresh rate rather than actively requesting a higher one. The client
  doesn't switch Android's display mode to 90Hz (confirmed available on
  this tablet, Phase 0) — it just reports whatever's already active,
  which defaults to 60Hz. Deliberately conservative, per `TASKS.md`'s own
  "don't assume 90Hz is free" note: ADR 0023/0027 already showed this
  hardware's real limits don't reliably match what's declared/assumed,
  and actively requesting 90Hz deserves its own live validation, not a
  default bundled into the negotiation mechanism.
- **Rotation**: out of scope for v1 by design, not an open question —
  this session's earlier UI work locked the Android client to landscape
  (`AndroidManifest.xml`'s `screenOrientation`), specifically so there's
  no runtime orientation switch to handle. `DISPLAY_INFO` reports that
  fixed orientation's dimensions once, at connection time.

**Real bug found and fixed during live validation**: `Display.Mode`'s
`physicalWidth`/`physicalHeight` are **not** rotation-adjusted — they
report the panel's fixed native orientation (1200x1920, portrait) even
while the activity is locked to and actually rendering landscape.
Confirmed live: the first version of `sendDisplayInfo()` reported
`1200x1920`, which the host dutifully activated as a *portrait* RandR
output while a *landscape*-shaped video stream was actually being
encoded into it — decode still nominally "succeeded" (the resolution
happened to survive the pixel-count ceiling either way, same total area)
but would have rendered visibly wrong (rotated 90°). Fixed by checking
`Display.getRotation()` and swapping width/height whenever it's 90°/270°
from natural, giving the actual current effective shape as
`docs/wire-protocol.md` already specified it should be.

## Consequences

- Live-validated end-to-end after the rotation fix: host log shows
  `negotiated display: 1920x1200@60Hz`, activation at the same
  `1920x1200+6400+0` geometry this session's manual constants already
  used, and the tablet confirms real decode (`released=2317 errors=0`).
  The negotiated path and the old hardcoded path now agree exactly on
  this specific tablet, which is expected — Phase 0's constants were
  correct — but the mechanism generating them is now real, not assumed.
- Encountered (twice) the same already-documented "app needs a fresh
  relaunch to pick up a new connection" issue (Phase 1 finding) while
  iterating live — not a Phase 5 regression, just the same known
  papercut recurring during rapid test cycles.
- `wait_for_display_info` shares its `FrameReader`/buffer with the main
  connection loop rather than using a separate one, so any bytes read
  past `DISPLAY_INFO` in the same bulk transfer (e.g. an early `INPUT`
  frame arriving in the same read as the handshake) aren't lost or
  double-processed once the main loop takes over.
- Still open, deliberately not attempted here: actually requesting 90Hz
  (would need Android-side display mode switching code, not just
  reporting) and any real multi-resolution/multi-device negotiation
  (v1 has exactly one target device).
