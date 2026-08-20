# 23. Real decode of the tabscreen output's native resolution hits a hardware pixel-count ceiling

Date: 2026-08-20

## Status

Accepted.

## Context

Following ADR 0022's video/input wiring, the first live test against the
real HONOR Pad X8a (tablet connected, real host desktop content, real
`vkms`-backed `Virtual-2-1` output at its true 1200×1920 geometry) failed
completely: the on-device status UI showed `released=0`,
`errors≈frames_received`, and `last=feed: IllegalStateException: Invalid
to call at Released state; only valid in executing state` — the hardware
decoder faulted on essentially the first real frame, every time, and
never successfully decoded a single one.

Ruled out, in order, each with direct evidence (not just plausible
theory), thanks to `adb` access to the tablet (it exposes ADB alongside
AOA accessory mode simultaneously on this device, letting this session
read on-device state and rebuild/install the client directly rather than
relying on the user to relay error text by hand):

1. **H.264 SPS level** — VAAPI auto-selected level 5.0 for this
   resolution/framerate; forcing level 4.1 (`-level 41`) produced an
   identical failure. Also realized forcing 4.1 was directionally wrong
   anyway: 1200×1920@30fps needs ~270,000 macroblocks/sec, which exceeds
   level 4.1's spec ceiling (245,760) — level 5.0 was the *correct*
   auto-selection, not the bug. Reverted.
2. **Activity orientation / letterboxing** — `AndroidManifest.xml` had no
   `screenOrientation` lock, and the tablet's system rotation was
   currently landscape-locked, so the app was letterboxed into a
   711×1200 window inside a 1920×1200 framebuffer while receiving a
   genuine 1200×1920 portrait stream. Fixed
   (`android:screenOrientation="portrait"`, confirmed via `dumpsys
   activity` that the app now gets a true 1200×1920 surface) — a real,
   necessary fix, but the decode crash persisted identically afterward,
   proving it wasn't the (sole) cause.
3. **Decoder variant** (`c2.qti.avc.decoder.low_latency` vs. the plain
   `c2.qti.avc.decoder`) — swapped `VideoDecoder.selectDecoder()` to force
   the plain hardware decoder. Identical failure. Reverted.
4. **`MediaFormat` `configure()` hint dimensions** — the existing hint
   (1920×1080, a leftover from the old whole-screen-placeholder pipeline)
   is smaller in total pixels (2,073,600) than the real stream
   (1200×1920 = 2,304,000). Tried setting the hint to the *exact* real
   dimensions:
   - `1200x1920` → **`configure()` itself threw
     `CodecException errorCode=1100`** — a hard, immediate rejection, not
     a later crash.
   - `1920x1200` (same pixel count, swapped to landscape) → **also
     `errorCode=1100`.** This is the key result: identical failure in
     both orientations at the same pixel count rules out orientation/
     aspect-ratio as the mechanism and points at total pixel count.
   - `1088x1744` (2,304,000px scaled down to ~1,897,472px, same ~5:8
     aspect) → **`configure()` succeeded.** But real decode (the host was
     still sending genuine 1200×1920 content) still crashed with the
     original signature — proving the *hint* was never the real
     bottleneck; the **real content's pixel count** is.

## Decision

Accept that this specific hardware decoder (both the `low_latency` and
plain variants) has a real, enforced pixel-count ceiling well below what
its own vendor-declared capability XML claims (`/vendor/etc/
media_codecs.xml` declares `size` up to `4096x2176` and `block-count` up
to `34816` — both comfortably cover 1200×1920 on paper). The actual
enforced ceiling sits somewhere between 1920×1080 (2,073,600px, decodes
cleanly, and not coincidentally exactly this device's declared
`performance-point-1920x1080` entry) and 1200×1920/1920×1200
(2,304,000px, hard failure either orientation).

Fixed by scaling the real encode output down, not just the configure()
hint:

- `host/src/video.rs`: replaced the old width-only `MAX_ENCODE_WIDTH`
  cap (1280px — a Phase 0-era guess that happened to never trigger for
  this resolution, and wouldn't have helped here since 1200px width is
  already under it) with `MAX_ENCODE_PIXELS = 1088 * 1744`, and scale
  *uniformly* (both dimensions, aspect ratio preserved via
  `sqrt(cap/area)`) rather than clamping width alone — this session's
  evidence shows the constraint is pixel-count-driven, not width-driven.
  Real computed output for 1200×1920 input: **1088×1742**.
- `android/.../VideoDecoder.kt`: `HINT_WIDTH`/`HINT_HEIGHT` updated to
  `1088`/`1742` to match the real encode output exactly (no longer a
  generic guess — Phase 2 gives a fixed, known real target now).

Live-validated end-to-end against the real tablet after this fix:
`released=1507 errors=0` — the host's actual desktop content rendering
correctly on the tablet, sustained.

## Consequences

- The tabscreen output now effectively renders at ~1088×1742 instead of
  the tablet panel's true 1200×1920 — a ~9% linear downscale, then
  upscaled back to fill the physical Surface by normal Android
  compositor stretching. Some sharpness loss versus a pixel-perfect
  1:1 mapping; acceptable for now, but worth reducing if a tighter real
  ceiling can be established (this session confirmed a working
  upper-bound of 1088×1742 and a failing lower-bound of 1200×1920/
  1920×1200 — the exact boundary between them was not narrowed further).
- This ceiling is almost certainly independent of Phase 5's mode
  negotiation work — whatever resolution gets negotiated in the future,
  `MAX_ENCODE_PIXELS` will still need to cap it, and the client's hint
  will still need to track the real encoded output rather than the
  negotiated/panel resolution.
- Vendor-declared `media_codecs.xml` capability limits are confirmed
  unreliable as a ceiling on this device (this is at least the second
  such finding on this tablet's decoder — see ADR 0007 for the earlier
  SEI-NAL-crashes-despite-spec-valid-bitstream finding). Treat any future
  capability question about this decoder as something to verify live,
  not read off the XML.
- `adb` access alongside AOA accessory mode (confirmed working
  simultaneously on this device) was what made this entire investigation
  tractable in one session — able to read on-device error state, take
  screenshots, and rebuild/reinstall the client directly, without relying
  on the user to manually relay error text or physically interact beyond
  the couple of USB replugs actually needed for kernel-level USB state
  recovery. Worth remembering as the default debugging approach for this
  project's Android side going forward.
