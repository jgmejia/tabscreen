# 35. Readability investigation: fps-for-resolution, bitrate/CQP, upscale isolation, and chroma siting all ruled out

Date: 2026-09-01

## Status

Accepted.

## Context

User feedback: the tablet's rendered resolution (1816x1136, ADR 0023/0027's
decoder pixel-count ceiling) isn't native, and small UI text (VS Code tab
titles, breadcrumbs) reads poorly -- specifically, glyph strokes on tiny
text show a one-sided brightness artifact (the upper part of each letter
rendering brighter than the rest). Starting hypothesis: `FRAMERATE` was
lowered after the pixel ceiling was established (ADR 0032, 30fps, for an
unrelated Xorg-serialization reason), so maybe the ceiling itself was never
re-tested at a lower framerate and could be fps-dependent.

## Investigation

**1. fps-for-resolution trade -- disproven.** Live-tested native
1920x1200 (no downscale) at 15fps against the real tablet (same
rebuild/redeploy/`adb`-verify methodology as ADR 0027): `MediaCodec`
`configure()` failed immediately with `CodecException errorCode=1100`,
identical to ADR 0023's original finding at 30fps. This is conclusive, not
just another negative data point: `MediaCodec.configure()` for a decoder
takes no framerate parameter, so a configure()-stage rejection cannot
depend on fps by construction. The ~2.06M px ceiling (1816x1136, already
binary-searched to ~0.3% precision in ADR 0027) is a pure spatial
buffer-size limit -- lowering framerate can never unlock more resolution
on this decoder. Reverted to the known-good `MAX_ENCODE_PIXELS`/`FRAMERATE`
values after confirming.

*Side incident during this probe*: relaunching the client via
`adb shell monkey -c android.intent.category.LAUNCHER` produced a real
black-screen decode failure that turned out to be an artifact of that
launch method (no `EXTRA_ACCESSORY` in the intent, forcing the same
untested fallback permission path ADR 0029 already flagged), not a second
resolution-related bug. Physically relaunching from the tablet's home
screen is the reliable way to test client changes; `adb`-driven launches
are not equivalent to the real-world attach path.

**2. Bitrate / rate-control mode -- no visible improvement.** The existing
encoder args set only `-b:v` with no explicit `-rc_mode`, meaning the
encoder auto-selects a rate-control mode that targets an average bitrate
rather than a fixed per-block quality -- a real theoretical difference for
text-heavy content, since a bitrate target can still blur a hard
(text-dense) frame to stay on budget. Tested raising `-b:v` to 24M (still
targeted) and separately switching to `-rc_mode CQP -qp 18` (unbounded,
quality-fixed) -- affordable given Phase 0's confirmed USB headroom (20.7
MB/s vs ~1-3 MB/s actually used). Both live-tested cleanly (no decoder
errors) but the user's direct read on the physical tablet: no
perceptible improvement on the specific small-text failure case.

**3. Fullscreen upscale-stretch -- ruled out as the mechanism.**
Normal operation lets `SurfaceFlinger` stretch the decoded 1816x1136
buffer to fill the tablet's full 1920x1200 panel, adding a second resample
pass on top of the encoder's own downscale. Temporarily forced the
`SurfaceView` to a fixed 1816x1136 `FrameLayout.LayoutParams` (Gravity.CENTER)
so the decoded frame renders 1:1, no extra stretch. The artifact was
still visible identically -- it's baked in before the stretch, not caused
by it. Reverted (`MainActivity.kt`'s `SurfaceView` back to filling its
parent).

**4. Chroma siting -- no improvement, and reasoned to be the wrong
mechanism in hindsight.** Tried `scale_vaapi`'s `out_chroma_location=left`
explicitly, on the theory that a chroma-siting mismatch between the
encoder's assumption and the tablet decoder's NV12 4:2:0 reconstruction
could produce a one-sided edge artifact. No visible change. In hindsight
this was never a strong fit for the reported symptom: chroma siting only
affects color-plane alignment, and the user's description (brightness --
a luma phenomenon) was never a chroma-channel effect to begin with.
Reverted (`host/src/video.rs`'s `-vf` chain back to no explicit
`out_chroma_location`).

**5. Subpixel (RGB-striped) font antialiasing -- identified as a likely
real contributor, but the user declined to test it.** Confirmed this host
currently has it active system-wide (`Xft.rgba: rgb`,
`org.gnome.desktop.interface font-antialiasing: 'rgba'`). Subpixel AA
encodes sub-character position into per-channel color fringing, on the
assumption the image reaches a physically matching LCD subpixel layout
completely undisturbed -- an assumption this pipeline violates twice
(the GPU downscale blends fringe pixels across neighbors; NV12 4:2:0
chroma subsampling then discards half the color resolution each
direction) and a third time trivially (the tablet's panel has its own,
unrelated subpixel geometry). Not tested because disabling it is a
desktop-wide GNOME setting, not scoped to the tabscreen output alone --
it would change font rendering on the user's real monitors too, and the
user explicitly declined that trade-off.

## Decision

No code or config changes retained from this investigation --
`host/src/video.rs` and the Android client are both back to exactly the
settings in place before this session (`FRAMERATE=30`,
`MAX_ENCODE_PIXELS=1818*1136`, `BITRATE=8M`, default `rc_mode`, default
`scale_vaapi` chroma siting, `SurfaceView` filling its parent). Verified
via `git diff --stat` showing no outstanding changes in `host/`/`android/`
and a clean reconnect log after the final revert.

## Consequences

- The fps-vs-resolution question raised at the start of this session is
  now closed with a structural (not just empirical) argument: framerate
  cannot affect a decoder's `configure()`-stage pixel-count ceiling.
  ADR 0023/0027's 1816x1136 remains the real, final ceiling on this
  hardware for H.264.
- Four independent, live-tested levers (fps, bitrate/CQP, upscale
  isolation, chroma siting) are now ruled out as fixes for the specific
  small-UI-text brightness artifact. The likely real mechanism is
  aliasing from downscaling hinted/anti-aliased glyphs by a non-integer
  ratio (1920->1816, ~5.4% linear) at the GPU scaler -- not something
  tunable via encoder parameters, since it happens before the encoder
  ever sees the frame.
- Two options remain untried, both bigger asks than a config flag:
  (a) raise DPI/font scale on the host for the `Virtual-2-1` output
  specifically (a real trade-off -- more legible per character, less
  on-screen content -- but does not require the subpixel-AA change the
  user declined), or (b) a different codec/decoder hardware block
  entirely (e.g. HEVC via `hevc_vaapi` + an on-device HEVC decoder, if
  one exists) -- untested, and a bigger lift than this session's
  probes (wire-protocol/negotiation changes, not a one-line constant).
- Confirmed a real, separate gotcha worth remembering for future live
  Android testing: `adb shell monkey -c android.intent.category.LAUNCHER`
  is not equivalent to a real launch (per ADR 0029, it skips
  `EXTRA_ACCESSORY` and forces the untested fallback permission path) --
  physically relaunching from the tablet is the reliable method when
  verifying a client change actually works, not just that the client
  process didn't crash.
