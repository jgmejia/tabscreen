# 31. Reverted 90fps: x11grab can't sustain it, causing the video to silently freeze

Date: 2026-08-20

## Status

Accepted.

## Context

ADR 0030 raised `FRAMERATE` to 90, live-validated with a short (~25s)
test showing clean, sustained decode. Longer real use surfaced a
different failure: the video stream would freeze on a stale frame while
the Android client's own `released`/`errors` counters kept climbing with
zero errors — misleadingly looking like a client-side rendering bug.

Diagnosed by checking the daemon's log rather than trusting the client's
counters: `ffmpeg` was repeatedly logging `frame duplication too large,
skipping`. `x11grab` cannot actually sustain real captures at 90fps on
this hardware (most likely its own polling/`XGetImage` overhead, not a
VAAPI or decoder limit -- not confirmed either way). When the real
capture rate falls short of the requested output framerate, `ffmpeg`
pads the difference with duplicate frames; past some internal threshold
it gives up and stops emitting real output entirely, while the h264
stream's own timestamps/NAL flow can still let the *client* believe
it's decoding successfully (the frames it does receive decode fine --
there just stop being new ones).

This also explains an earlier, seemingly separate live incident this
session ("no output, only the debug text") that was worked around at
the time by just relaunching the Android app -- almost certainly the
same underlying cause, not the stale-`Surface` theory assumed then. A
fresh `ffmpeg` process (new connection cycle) buys some time before
duplication pressure builds back up again, so a relaunch "fixing" it
was a coincidence of timing, not a real fix.

## Decision

Reverted `FRAMERATE` from 90 back to 60. Live-validated: real, live
(non-frozen) video confirmed via direct visual comparison of the actual
host screen content against what the tablet displays (not just the
client's self-reported counters, learning from how those counters
misled the initial diagnosis), sustained for a full minute with no
`too large, skipping` failures (only the milder, expected `More than
1000 frames duplicated` notice, which does not by itself indicate a
failure state).

## Consequences

- 90fps is not currently usable, contradicting ADR 0030's conclusion --
  that ADR's validation window (~25s) was too short to surface this;
  worth remembering that a short clean test is not sufficient evidence
  of sustained stability for this pipeline, especially anything to do
  with `x11grab`'s own capture throughput.
- The client's `released`/`errors` counters are **not** a reliable
  signal of whether real, live video is actually reaching the tablet --
  they only reflect the decoder successfully processing whatever bytes
  it receives, and say nothing about whether the *host* is still
  sending genuinely new frames. Any future live check of "is video
  actually working" should compare real host content against what's on
  the tablet directly (a screenshot pair, as used to diagnose this),
  not just read the on-screen counters.
- Real fix not yet found: whether `x11grab` is actually using MIT-SHM
  (`-f x11grab`'s fast path) hasn't been confirmed, nor has this
  hardware's actual sustained capture rate been measured. 60fps
  currently works but is itself unconfirmed for *arbitrarily* long
  sessions -- only tested for about a minute here. Worth a proper,
  longer soak test before trusting it fully, and worth investigating
  whether an explicit `-show_region`/shared-memory flag or a different
  capture approach could sustain a genuinely higher rate if that's ever
  revisited.
