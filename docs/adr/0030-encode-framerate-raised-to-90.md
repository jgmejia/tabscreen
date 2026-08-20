# 30. Encode framerate raised from a hardcoded 30fps to 90fps

Date: 2026-08-20

## Status

Accepted. Live-validated, sustained.

## Context

The user noticed real latency/sluggishness while using tabscreen and
asked why. `host/src/video.rs`'s `FRAMERATE` constant had been hardcoded
to `"30"` since early Phase 3 testing and never revisited — including
after Phase 5 landed real mode negotiation and the tablet started
reporting its actual refresh rate (60Hz or 90Hz depending on its current
state). Every screen update, including the visual feedback for the
user's own touch input, was capped to a 33ms-spaced frame cadence
regardless of what the panel could actually display — a real,
measurable contributor to the perceived lag, on top of the inherent
capture→encode→transmit→decode→render round trip this architecture
has compared to a real display signal (see the conversation for the
full explanation given before this change).

## Decision

Raised `FRAMERATE` incrementally, live-testing each step rather than
jumping straight to 90 — matching this project's standing practice of
verifying hardware behavior rather than trusting a declared spec (the
same discipline that found the real pixel-count ceiling in ADR 0023/
0027):

- **30 → 60**: confirmed clean immediately (`released=27780 errors=0`).
- **60 → 90**: `ffmpeg` itself warned `Stream will not conform to any
  level: using level 6.2` — 6.2 is the *highest* defined H.264 level,
  meaning the encoder couldn't find any standard level whose declared
  limits the resulting stream actually satisfies, and fell back to
  declaring the highest one as a best effort. This looked like a strong
  signal the real decoder (already proven stricter than its own
  declared capability XML, ADR 0023) would reject it. **It didn't** —
  confirmed live and sustained (`released` climbed cleanly from 36,568
  to 43,691 over the following ~25 seconds, zero errors throughout).

Final value: `FRAMERATE = "90"`, matching the tablet's peak negotiated
refresh rate exactly.

## Consequences

- This is at least the second time this session that a *stricter*
  H.264-spec-conformance signal (the encoder's own level warning here;
  ADR 0023's `configure()` `errorCode=1100` rejections earlier) turned
  out not to predict real decode behavior in the same direction both
  times — ADR 0023 found the decoder *more* restrictive than its
  declared capability, while this finding shows it's *more permissive*
  than strict H.264 level conformance would suggest. Net lesson
  unchanged from before: this hardware's real behavior has to be
  checked live, in both directions, not inferred from any written spec
  (vendor capability XML or the H.264 standard's own level tables).
- `BITRATE` (`8M`) is a target bitrate independent of framerate, so this
  change doesn't increase USB bandwidth needs — Phase 0's confirmed
  throughput headroom (20.7 MB/s vs. ~1MB/s needed at 8Mbps) is
  unaffected.
- `GOP` (`60`) wasn't scaled with the framerate change. At the old
  30fps this meant a keyframe every 2s; at 90fps it's now every ~0.67s.
  Not a correctness issue (more frequent keyframes trade a little
  encode efficiency for faster error recovery, still within the same
  target bitrate), but worth revisiting if bitrate efficiency ever
  becomes a concern.
- Rigorous end-to-end latency measurement is still not done (a
  standing Phase 3 TODO) — this change is a reasoned, live-validated
  improvement to a known contributor, not a measured before/after
  latency number.
