# 7. MediaCodec low-latency decoder constraints (empirical, HONOR Pad X8a)

Date: 2026-08-19

## Status

Accepted

## Context

Phase 3's Android decode side targets `c2.qti.avc.decoder.low_latency`
specifically — TASKS.md called this out from Phase 0's codec survey, which
confirmed the variant exists on this tablet and should be preferred over
whatever the platform picks by default. Getting a real end-to-end stream
working against it (host capture → VAAPI encode → USB → decode → render)
took five separate failure modes, each initially indistinguishable from the
others because `logcat` is confirmed unreliable on this device/OS (Phase 0)
— diagnosis had to go through an on-screen status counter instead
(`MainActivity`'s `refreshStatus()`), not log inspection.

Empirical findings, in the order they were hit and fixed:

1. **`MediaFormat.createVideoFormat` width/height is a hard ceiling for
   this decoder, not just a sizing hint.** An initial defensive 4K
   (`3840x2160`) guess made `MediaCodec.configure()` throw
   `IllegalArgumentException` outright — this decoder variant's real
   capability is far below that.
2. **Independently clamping width and height to their own reported maxima
   produces an invalid pairing.** Querying
   `CodecCapabilities.videoCapabilities` and clamping width/height each to
   `.supportedWidths.upper`/`.supportedHeights.upper` independently (both
   happened to report `1920`) produced a `1920x1920` square configure
   attempt — `CodecException errorCode=1100`. `VideoCapabilities` exposes
   coupled ranges (`getSupportedHeightsFor(width)`) for a reason; two
   independent uppers aren't guaranteed to be a jointly valid pair. Rather
   than chase the joint-query API for what's ultimately just a sizing
   hint, this landed on a fixed, universally-safe `1920x1080`.
3. **Default VAAPI SEI messages (identifier/timing/recovery_point) produce
   one standalone SEI NAL per frame that this decoder doesn't tolerate
   well long-term.** Confirmed the raw bitstream was fully spec-valid
   first (clean `ffmpeg -f null` software-decode round trip with zero
   errors) before concluding this was a decoder-strictness issue rather
   than a malformed stream; `-sei 0` on the `h264_vaapi` encoder removes
   them. On its own this delayed but didn't prevent the eventual crash
   (see #4) — worth keeping regardless since it's a real spec-compliant
   simplification with no downside.
4. **The actual root cause: polling `dequeueOutputBuffer` with a 0ms
   timeout immediately after each `queueInputBuffer` call almost never has
   a ready buffer, and this decoder's low-latency variant has a minimal
   internal pipeline depth that doesn't tolerate output staying undrained
   for long** — it eventually faults the whole codec (every later call
   throws `IllegalStateException: Invalid to call at Released state`, not
   a recoverable per-call error). The fix: a dedicated thread blocking on
   `dequeueOutputBuffer` with a real timeout (10s), decoupled entirely
   from however fast input NALs arrive, draining continuously for the
   codec's lifetime.
5. **After the above, `released` output count stayed at zero and
   `errors` climbed anyway — because the *actual* incoming content
   exceeded this decoder's true capability.** The 1920px-width ceiling
   from finding #2 was real, not just a hint-clamping artifact — and
   Phase 3's host-side capture target is currently the *host's own
   screen* (2560×1600 on this dev machine, since Phase 2's dummy output
   doesn't exist yet — see [ADR 0006](0006-dummy-driver-multi-gpu-binding-failure.md)),
   which exceeds it. Fixed by scaling the host's encode output down to a
   1280px-wide ceiling (`host/src/video.rs`'s `MAX_ENCODE_WIDTH`),
   aspect-ratio preserved.

With all five addressed: 1680 frames received, 1623 output buffers
released, zero errors over a sustained ~28s live test, with the tablet
visibly mirroring the host's desktop in real time.

## Decision

- Keep targeting `c2.qti.avc.decoder.low_latency` explicitly
  (`VideoDecoder.selectDecoder()`), but treat its capability report as a
  hard constraint, not a hint: never configure or feed it content wider
  than what's cheaply known to be safe.
- Keep the dedicated output-draining thread as the standing pattern for
  any future MediaCodec consumer in this codebase — polling output
  inline with input on this hardware is not just slower, it's unsafe.
- `host/src/video.rs` caps its encode width (`MAX_ENCODE_WIDTH = 1280`)
  as a stand-in for Phase 2's eventual dummy output, which will be sized
  to the tablet's own panel (1200×1920) and comfortably fit without this
  scale-down being necessary going forward.
- When `logcat` is unreliable on a device (confirmed for this one), build
  the on-screen status counter first, before trying to interpret
  intermittent/silent failures — five rebuild-install-relaunch cycles
  were needed here specifically because the *first* two attempts had no
  visible error signal at all (a black screen with no diagnostic text).

## Consequences

- Phase 3's Android decode path is no longer a placeholder: it's a real,
  validated MediaCodec consumer with documented constraints future phases
  (5's mode negotiation, 7's reliability work) need to respect rather than
  rediscover.
- The 1280px encode cap is currently host-screen-driven and will need
  revisiting once Phase 2 unblocks and the real capture target becomes
  the tablet's own panel resolution — at that point `MAX_ENCODE_WIDTH`
  should no longer bind in practice, but the clamp itself is cheap
  insurance worth keeping.
- This decoder's low internal pipeline tolerance (finding #4) is a
  reliability-relevant fact for Phase 7: any future change to the
  Android-side decode loop must preserve continuous, decoupled output
  draining — regressing to inline polling would silently reintroduce
  this exact crash under real streaming load.
