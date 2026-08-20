# 19. `ConnectedMonitor` reliably breaks real external monitor detection — not viable at all

Date: 2026-08-19

## Status

Accepted. Supersedes the entire `ConnectedMonitor`/`CustomEDID` line of
investigation (0010 through 0018) with a decisive, causally-confirmed
finding: it's not a toggle-reliability or greeter-selection problem. The
mechanism breaks real hardware detection for the actual external monitor,
every time, for as long as it's active. Neither option 1 (dynamic
application) nor option 2 (always-on) from 0018's decision survives this.

## Context

While investigating option 2 live, the user reported something previously
under-examined: **during every single test tonight, neither monitor showed
anything at all** — not just the wrong one being selected, but a total
blank on both `eDP-1` and the external LG monitor. That prompted checking
whether the real monitor (`DFP-4`) was even being detected at the driver
level, independent of anything to do with primary-selection or
`monitors.xml`.

**Live check, mid-test:** queried the greeter's own X display directly
(`DISPLAY=:0`, `gdm`'s `Xauthority`, from a TTY). Result: `DP-1` (the fake
tablet output) was the *only* connected output, screen sized exactly to
its resolution (`1200x1920`) — `eDP-1` wasn't even part of this screen
(different GPU), and the real monitor's connector (`HDMI-0`/`DFP-4`)
showed disconnected. Repeated the query several times, 10 seconds apart:
identical, stable result — not a transient timing blip.

**Cross-checked against this boot's own log:** `DFP-4` showed the generic,
no-EDID-read default (`165.0 MHz maximum pixel clock`, `Internal TMDS`)
throughout the entire greeter session — never once transitioning to
`connected` with the real monitor's actual identity
(`LG Electronics LG HDR 4K`, `600.0 MHz`).

**Checked every other boot from tonight (9 total) for the same signature.**
100% consistent: every boot with `Option "ConnectedMonitor" "DFP-1"`
active shows `DFP-4` stuck at the generic undetected state on the first
Xorg process. The real monitor's identity only ever appears **later, under
a different PID** — and checking the specific case (boot `-1`, PID `6522`):
that PID's own log has **zero** `ConnectedMonitor` lines, and `journalctl`
confirms the config file was `rm`'d and `gdm` was restarted *before* that
PID started. The "recovery" was never the same Xorg session healing
itself — it was always a fresh Xorg instance starting *after* the config
had already been removed.

**Conclusion, not inference: `ConnectedMonitor "DFP-1"` prevents the real
external monitor from being detected at all, for the entire lifetime of
any Xorg process where it's set.** This is why every test tonight showed
both monitors blank — the real monitor's EDID/DDC detection was genuinely
broken, not just misconfigured or wrongly targeted.

## Decision

**Abandon the `ConnectedMonitor`/`CustomEDID` static-override approach
entirely on this driver (580.173.02) and hardware combination.** This
isn't a UX/reliability problem to work around — it's a hard incompatibility
with keeping the real external monitor functional. Neither remaining
option from 0018 survives:

- **Option 1 (dynamic, apply only at tablet-connect time)** would mean the
  real external monitor goes dark for the entire duration the tablet is
  connected — not an acceptable tradeoff, arguably worse than the
  always-on option since it's an active regression during real use rather
  than a permanent one you could plan around.
- **Option 2 (always-on from boot)** means the real monitor never works
  at all, on any boot, for the lifetime of the session — a complete
  non-starter, far worse than the "extra ghost monitor" cost originally
  described.

This closes out ADRs 0010 through 0018's whole approach, not just this
session's specific tests within it.

Real remaining paths, revisiting the options ADR 0009 and 0013 already
raised but deprioritized:

1. **`evdi`/`vkms`** (deprioritized in 0006/0008 for live-session-freeze
   risk when loading the kernel module) — these create genuine, separate
   DRM devices rather than overriding detection state on the GPU that's
   also driving a real monitor. Worth reconsidering now that the
   alternative (this entry) has proven to have its own severe, different
   failure mode.
2. **A different validation environment** (VM, second machine) — ADR
   0006 raised this and it was deferred repeatedly. Given tonight's
   investigation caused the real external monitor to be non-functional
   for extended periods across many reboots, on the primary daily-use
   machine, the cost of continuing to experiment here is more concrete
   than it was when this was first deferred.
3. Reconsider whether Intel ever has a viable idle connector (0009 found
   none on this laptop's built-in ports) — e.g. via a USB-C dock or
   external adapter that would route through Intel rather than NVIDIA,
   not previously considered.

## Consequences

- Confirmed fully recovered: both monitors active, greeter correctly on
  `eDP-1`, after removing the config and restarting `gdm`.
- This finding should have been checked much earlier — the "both monitors
  blank during every test" detail was mentioned by the user only after
  several ADRs had already investigated narrower theories (greeter
  primary-selection, `monitors.xml` reliability, non-determinism). Worth
  noting for future sessions: when live-testing produces "nothing visible
  at all," check literally every output's detection state directly, not
  just the one being deliberately modified — a total blackout is a
  different and more informative symptom than a partial one, and
  shouldn't be assumed to reduce to the same cause.
- Phase 2 remains blocked, now on a clearer footing: the specific
  mechanism explored across an entire session (ADRs 0008–0019) is ruled
  out conclusively, not just unreliable. The next real decision is among
  the three paths above, likely starting with `evdi`/`vkms` research or a
  VM-based validation environment rather than further live attempts on
  this machine's primary NVIDIA GPU.
