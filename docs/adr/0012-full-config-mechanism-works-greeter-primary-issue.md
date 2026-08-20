# 12. Full `Device`+`Screen` config works at the driver level; greeter picked the wrong primary output

Date: 2026-08-19

## Status

Accepted — records a genuine partial success. Unlike 0005/0009/0011, this
attempt did **not** fail at the mechanism level; it failed at a
GDM-greeter-display-selection level that's plausibly irrelevant to the real
Phase 2 design.

## Context

Applied 0011's proposed fix: a full `Device` + `Screen` section (not a bare
`Device`-only fragment), with `ConnectedMonitor "DFP-1"`, `CustomEDID
"DFP-1:/lib/firmware/edid/tabscreen-honor-pad-x8a.bin"`, and an explicit
`MetaModes` string preserving the real monitor's exact current mode
(`DFP-4: nvidia-auto-select +2560+0`, matching what the live session was
already using) while adding the virtual tablet output
(`DFP-1: 1200x1920 +6400+0`). Applied via full reboot.

Result: the user reported "failed gdm start," recovered via VT-switch
(`rm` the config file) without a full second reboot — `systemctl restart
gdm` (or GDM's own crash-restart) brought the session back after that.
Current session confirmed healthy afterward.

**Diagnosis from `journalctl -b 0` (the same boot the failure happened in,
since no second reboot occurred — logs from before the fix are still
present):**

1. **The core mechanism worked.** Unlike 0011's bare fragment, this attempt
   produced:
   ```
   (--) NVIDIA(GPU-0): TBS TabScreen (DFP-1): connected
   (--) NVIDIA(GPU-0): TBS TabScreen (DFP-1): Internal DisplayPort
   (--) NVIDIA(GPU-0): TBS TabScreen (DFP-1): 2670.0 MHz maximum pixel clock
   (II) NVIDIA(0): Validated MetaModes:
   (II) NVIDIA(0):     "DFP-4:nvidia-auto-select+2560+0,DFP-1:1200x1920+6400+0"
   (II) NVIDIA(0): Setting mode "DFP-4:nvidia-auto-select+2560+0,DFP-1:1200x1920+6400+0"
   ```
   "TBS TabScreen" is the manufacturer/product name encoded into the EDID in
   [0008](0008-edid-override-virtual-output.md) — confirms the driver
   genuinely parsed and used our EDID file, reported the device connected,
   and validated+set the combined mode including `DFP-1` at 1200×1920. This
   is the first attempt (of four) where the mechanism itself worked as
   intended.
2. **No crash found anywhere in the logs.** `gnome-shell` (the greeter's
   compositor) started normally (`Running GNOME Shell... as a X11 window and
   compositing manager`), registered with GDM, and never logged an exit,
   crash, or fatal error in the ~74-second window between the config taking
   effect and the user's manual recovery. The only repeated log lines in
   that window (`XIChangeProperty failed on device 20... X error 8`) are
   unrelated libinput/touchpad property warnings, not new to this attempt.
3. **One later log line hints at the actual problem:**
   ```
   (II) NVIDIA(0): Setting mode "DP-1: nvidia-auto-select @1200x1920 +0+0
   {ViewPortIn=1200x1920, ViewPortOut=1200x1920+0+0}"
   ```
   — a *second*, separate mode-set call, using `DP-1` (the connector-based
   alias for the same device, see Appendix C) at `+0+0` — contradicting the
   `+6400+0` position from our declared `MetaModes`. This looks like
   something in the greeter/session startup path (most likely GNOME's own
   default-primary-output selection, run by `gnome-shell` or an
   RandR-calling helper after Xorg finished starting) independently
   repositioned or reselected the virtual output — plausibly *as the
   primary/only display it chose to render the greeter onto*, since nothing
   in the log shows a crash but the user saw nothing.
4. **Best available explanation:** the greeter likely rendered itself onto
   `DFP-1` (nothing physically connected — hence invisible to the user)
   rather than the real monitors, most plausibly because some
   default-primary heuristic favored the lowest-numbered/newly-added output.
   This is a display-selection/UX problem, not a driver or mechanism
   failure.

## Decision

- **The EDID + `ConnectedMonitor` + `CustomEDID` + explicit `MetaModes`
  mechanism is confirmed working at the NVIDIA driver level** — this is
  real progress after three prior failures (0006, 0009, 0011). Do not
  abandon this general approach.
- **Do not chase the greeter-primary-selection problem directly** — it's
  plausibly irrelevant to what Phase 2 actually needs. The real design
  (TASKS.md) was never "declare the tablet output permanently live in
  boot-time `MetaModes`" — it's "off until the tablet connects," toggled
  at runtime via `xrandr --output ... --auto`/`--off` by the host daemon.
  This test forced the virtual output permanently into the boot-time
  config and through the greeter, which is a harsher and more
  production-irrelevant test than what tabscreen actually needs.
- **Proposed next test, not yet attempted:** keep the same `Device`+`Screen`
  config (since it's confirmed to work), but immediately after Xorg starts
  — before any greeter render — turn the virtual output off
  (`xrandr --output DFP-1 --off`, run via a systemd unit or udev hook
  ordered right after the display manager starts, not manually after
  login). This is much closer to the actual target workflow (output exists
  and is EDID-aware, but inactive until explicitly turned on) and would
  very plausibly sidestep the greeter-primary problem entirely, since an
  `--off` output can't be selected as a rendering target.

## Consequences

- No lasting damage: confirmed current session fully healthy, `eDP-1` and
  `HDMI-1-0` unaffected, config file removed.
- This is the strongest positive signal so far for Phase 2 — the remaining
  work looks like sequencing/timing (when the output gets toggled on/off
  relative to Xorg/GDM startup), not a fundamental mechanism problem.
- Still worth deciding, before the next live attempt, whether to test the
  off-by-default sequencing live on this machine again (four attempts in,
  one causing a real if self-healing crash, this one a ~74s greeter hang)
  or to first prototype the systemd/udev sequencing offline before touching
  the live session a fifth time.
