# 27. Narrowed the decoder's pixel-count ceiling via live binary search

Date: 2026-08-20

## Status

Accepted.

## Context

ADR 0023 found a real hardware pixel-count ceiling on the target
tablet's AVC decoder and picked a conservative working value (1088x1744,
1,897,472px) to unblock the pipeline, without narrowing exactly where
the real ceiling sits — known-good was 1,897,472px, known-bad was
2,304,000px, a gap wide enough to be leaving real sharpness on the
table (82.4% of the tabscreen output's native 2,304,000px area).

## Decision

Ran a live binary search directly against the real tablet, using the
now-installed systemd service (ADR 0026) and `adb shell uiautomator
dump` (faster and more reliable than a screenshot — pulls the on-screen
status `TextView`'s text directly, since this device's `logcat` is
already known unreliable per Phase 0) to read `released=`/`errors=`
after each step. Each probe changed `MAX_ENCODE_PIXELS`, rebuilt and
redeployed the host binary directly to `~/.local/bin/tabscreen-host`
(no `sudo` needed for this — only the original `install.sh` run needed
it, for the udev rule and module-load config), restarted the systemd
service, read the real computed encode dimensions from its log, matched
the Android client's `HINT_WIDTH`/`HINT_HEIGHT` to that exact value, and
rebuilt/reinstalled the APK before checking pass/fail.

Six probes narrowed the range from the original 406,528px gap down to
under 6,000px:

| Probe | Dimensions | Area (px) | Result |
|---|---|---|---|
| (baseline) | 1088x1744 | 1,897,472 | known good (ADR 0023) |
| 1 | 1832x1144 | 2,095,808 | fails (`errorCode=1100`) |
| 2 | 1786x1116 | 1,993,176 | **works** |
| 3 | 1808x1130 | 2,043,040 | **works** |
| 4 | 1818x1136 | 2,065,248 | **works** |
| 5 | 1824x1140 | 2,079,360 | fails |
| 6 | 1820x1138 | 2,071,160 | fails (reconfirmed after a one-off app instability muddied the first read) |

Final value: **1816x1136 (2,063,776px)** — the real value
`MAX_ENCODE_PIXELS = 1818 * 1136` reconstructs through the existing
uniform-scaling algorithm (1818's exact area target doesn't divide
perfectly back into a 1920:1200-aspect width/height pair, so the actual
output is 1816, not 1818 — still safely inside the confirmed-good
range since it has *fewer* pixels than the 1818x1136 probe that passed).

One probe (#6, retried) surfaced a one-off Android app crash unrelated
to the actual pass/fail signal — the app's process died mid-test (no
crash detail available, `logcat` unreliable as always on this device)
independent of the decoder question, requiring a `force-stop`+relaunch
before the real (consistent, reproducible) `errorCode=1100` result for
that dimension could be confirmed. Not investigated further since it
didn't recur and isn't this ADR's concern.

## Consequences

- Real encode resolution improved from 1,897,472px to 2,063,776px
  (+8.8% area, ≈+4.3% linear per dimension) — now at 89.6% of the
  tabscreen output's native 2,304,000px area, up from 82.4%.
- The remaining ~6,000px gap between confirmed-good and confirmed-bad
  was judged not worth further narrowing — diminishing returns (a
  further halving would only recover another ~0.15% of area) against
  the cost of more live test cycles.
- Corrects an imprecise claim in ADR 0023: that ADR's mention of
  "1920x1080 (2,073,600px) decodes cleanly" was only ever verified as a
  `MediaCodec.configure()` *hint* accepted without error, using the
  *old* placeholder pipeline's actual encoded content (1280x800,
  1,024,000px) — never verified with real 1920x1080 content actually
  flowing through decode. Given this session found genuine failure
  starting at 2,071,160px, well *below* 1920x1080's 2,073,600px, that
  older claim should not be trusted as evidence 1920x1080 itself would
  actually decode real content correctly.
- `adb shell uiautomator dump` (over a screenshot) is a faster, more
  direct way to check this app's on-screen status going forward —
  worth defaulting to it for any future live Android UI-state check on
  this project, reserving screenshots for when the actual visual
  rendering (not just status text) needs inspecting.
