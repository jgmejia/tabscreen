# 36. Bumped encode resolution to 1818x1136 and added a user-facing native/fill display-mode toggle

Date: 2026-09-01

## Status

Accepted.

## Context

ADR 0035 closed out the readability investigation without a fix: the
small-UI-text brightness artifact survived bitrate/CQP changes, upscale
isolation, and chroma-siting changes, and was diagnosed as aliasing from
downscaling hinted/anti-aliased glyphs by a non-integer ratio -- not
something an encoder setting can fix. Two things came out of manually
comparing the decoded frame at its true size (no fullscreen stretch)
against the previous deployed value, though:

1. The previously-deployed constant, `MAX_ENCODE_PIXELS = 1818 * 1136`,
   does not actually reconstruct to 1818x1136 through the uniform-scale
   formula (`sqrt` then truncate rounds down) -- it reconstructs to
   1816x1136, per ADR 0027's own note. The real confirmed-good, unused
   2px-per-dimension of headroom was sitting on the table.
2. Viewing the decoded frame at its true size (rather than fullscreen-
   stretched) is itself useful for judging real sharpness, independent of
   which exact resolution is deployed -- the fullscreen fill mode adds a
   second resample pass on top of the encoder's own downscale.

## Decision

**Resolution**: fixed the rounding gap so the deployed resolution is
genuinely 1818x1136, ADR 0027's exact largest confirmed-good binary-search
probe -- `MAX_ENCODE_PIXELS` is now `1819 * 1137` (the smallest area that
survives the same truncation and lands back on 1818x1136), not the
literal-but-misleading `1818 * 1136`. `android/.../VideoDecoder.kt`'s
`HINT_WIDTH` updated to match (1818, `HINT_HEIGHT` unchanged at 1136) --
this hint has to track the real encode output exactly, per its own
existing comment.

**Display mode**: added a user-facing choice between the previous
default (`FILL` -- `SurfaceView` stretched to fill the panel, one extra
resample pass on top of the decode) and a new `NATIVE` mode (`SurfaceView`
sized to the real decoded output, centered, no stretch -- at the cost of a
visible border/frame around the image). Implementation notes:

- `VideoDecoder` now handles `MediaCodec.INFO_OUTPUT_FORMAT_CHANGED` in
  its output-drain loop (previously silently ignored) and reports the
  real decoded width/height back to `MainActivity` via a constructor
  callback, rather than `MainActivity` hardcoding the encode dimensions a
  third time (they'd already existed in `host/src/video.rs` and the
  decoder hint) -- avoids exactly the kind of silent-drift risk
  `VideoDecoder.kt`'s own `HINT_WIDTH` comment already warns about.
- New `Settings` object (`SharedPreferences`-backed) persists
  `DisplayMode` and a frame color (`Int` ARGB) locally on the tablet --
  purely a client-side rendering preference, nothing negotiated with or
  known to the host.
- **Settings access**: this app forwards every touch on its window to the
  host as remote mouse/touch input (that's its whole purpose as a second
  monitor), so no touch gesture can double as a local menu trigger without
  stealing it from the remote session. Used a long-press (800ms) on the
  hardware Volume Down button instead -- not otherwise used by this app.
  A short press still behaves as an ordinary volume-down (not consumed);
  only a press held past the threshold opens the dialog and suppresses
  that press's release.
- **Settings dialog**: built programmatically (matching this app's
  existing no-XML-layout style) -- a two-way `RadioGroup` (Fit to screen /
  Native pixels) and a row of frame-color preset swatches (black, dark
  gray, white, plus four Solarized Dark base tones -- base03/02/01/0 --
  for users of that editor/terminal theme). Each swatch is a
  `GradientDrawable` with an explicit border (otherwise a preset close to
  the dialog's own background, e.g. white, is indistinguishable from the
  gap between swatches) and the currently-selected swatch gets a thicker
  cyan ring, updated live on tap -- added after live testing surfaced both
  as real usability gaps, not designed in up front.

## Consequences

- Tablet output is now 1818x1136 instead of 1816x1136 -- a marginal
  (~0.2%) area increase, already confirmed working via ADR 0027's own
  binary search; no new live-decode risk introduced.
- The `NATIVE` display mode gives a way to inspect the real decode
  quality directly (as this session did manually, repeatedly, via a
  temporary code change each time) as a permanent, no-rebuild-required
  toggle -- useful for any future readability work, not just this
  session's.
- Settings are per-device (`SharedPreferences`), not synced with or
  known to the host -- consistent with this project's client/host split,
  where the host has no visibility into how its output is being rendered
  on the tablet.
- The Volume Down long-press means this app fully owns that button while
  in the foreground; a user who wants ordinary system volume-down
  behavior gets it as long as they don't hold it past ~800ms, which is
  an acceptable trade-off for this user's own daily-use tool.
