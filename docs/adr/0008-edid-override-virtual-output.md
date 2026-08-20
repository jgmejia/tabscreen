# 8. EDID firmware override on an unused connector, replacing xf86-video-dummy

Date: 2026-08-19

## Status

Proposed — supersedes the `xf86-video-dummy` technical path from
[0006](0006-dummy-driver-multi-gpu-binding-failure.md)'s finding 1. Not yet
validated against live hardware (requires a reboot); validation is Phase 2's
next concrete step.

## Context

0006 left Phase 2 blocked with three unexplored options: (a) a newer/patched
`xf86-video-dummy` build, (b) an alternative virtual-KMS driver (`evdi` named
explicitly, plus the in-tree `vkms`), or (c) an Xorg config idiom none of the
four tested shapes hit. This session researched (a)/(b) before writing any
more Xorg config.

**`evdi` and `vkms`, checked against this machine:**

- Both are available with no build step: `vkms` is in-tree for the running
  kernel (`7.0.0-29-generic`) and already present as a loadable module;
  `evdi-dkms` is packaged in Ubuntu 24.04's `universe` repo
  (`1.14.2+dfsg-1ubuntu1.4`).
- Both are genuine separate DRM/KMS devices, architecturally more promising
  than `xf86-video-dummy` for coexistence — Xorg's own dummy-driver probe
  logs "falling back to old probe method for dummy," meaning it isn't
  platform/udev-probed the way a real KMS device (or `evdi`/`vkms`) is,
  which is plausibly *why* 0006's `Inactive`/`GPUScreen` binding attempts
  failed specifically for `dummy`.
- However, both real-world writeups found on `evdi` (a DisplayLink-focused
  virtual-display guide and a personal blog walkthrough) independently
  report the same failure class as
  [0005](0005-live-session-dummy-output-hazard.md)'s incident: **loading the
  kernel module while Xorg is already running freezes the X session**,
  requiring either a reboot or loading the module before Xorg starts (via
  `/etc/modules-load.d/`). `evdi` is also reported to lose GPU acceleration
  on its virtual output. `vkms` was not tested directly this session, but
  shares evdi's basic shape (new DRM device introduced at runtime) closely
  enough that the same live-session risk should be assumed absent evidence
  otherwise.
- Given 0005's incident already burned a live-session freeze once, neither
  `evdi` nor `vkms` was pursued further as the primary path.

**Third option found, not anticipated by 0006: EDID firmware override on an
already-live GPU's unused connector.** Two independent real-world writeups
describe using the `drm.edid_firmware=<connector>:<edid-file>` kernel boot
parameter to make the kernel report a chosen, otherwise-disconnected
connector as "connected," using a supplied EDID binary — no new driver, no
new `Screen`, no `GPUScreen`/`Inactive` binding. It's the exact same screen
and driver already running the live desktop, with one more RandR output
added to it. This sidesteps 0006's actual failure mode entirely, since that
failure was specifically about binding a *second* driver/screen alongside
the real GPU's screen — there is no second screen here.

Checked against this machine (`xrandr --query`, live session): alongside
the active `eDP-1` (laptop panel, 2560×1600) and `HDMI-1-0` (external
monitor, 3840×2160, currently connected), there are **five disconnected
DisplayPort connectors on the same Intel screen — `DP-1-0` through
`DP-1-4`** — idle and available to claim as "the tablet's port." The iGPU
is confirmed via `lspci` to be Intel Raptor Lake (UHD Graphics,
`8086:a788`), which supports up to 4 simultaneous displays; 2 are in use
(`eDP-1`, `HDMI-1-0`), leaving headroom for a third (the virtual tablet
output) without displacing either existing display. The NVIDIA GPU
(`AD106M`/RTX 4070 Max-Q) is uninvolved — this stays entirely on the Intel
screen already driving the session.

No real cable or monitor is ever needed on the claimed port: tabscreen
doesn't send real video signal out of it. The RandR output backed by that
connector is only a software capture target (`x11grab`, per Phase 3) —
nothing electrical needs to leave the physical port, so an unwired or
unused connector is exactly as usable as a wired one.

## Decision

Adopt EDID firmware override as Phase 2's virtual-display mechanism,
replacing the `xf86-video-dummy`-based plan:

- Claim one of the Intel screen's idle connectors — `DP-1-0` — as the
  tablet's permanent virtual output. Pre-provisioning one fixed connector
  (rather than dynamically creating one) matches the installer approach
  already planned in TASKS.md (pre-declare, toggle on connect/disconnect),
  so this isn't a new constraint, just a different mechanism for satisfying
  it.
- Craft an EDID binary from the tablet's real, already-measured panel spec
  (HONOR Pad X8a: 1200×1920, supporting 60Hz and 90Hz — Phase 0), so the
  connector's native mode list matches the real device from boot, rather
  than depending on runtime `--newmode`/`--addmode` calls. This satisfies
  `CLAUDE.md`'s mode-matching requirement more directly than the original
  dummy-driver plan did.
- Install the EDID file where initramfs picks it up and set
  `drm.edid_firmware=DP-1-0:edid/tabscreen-honor-pad-x8a.bin` as a kernel
  boot parameter (GRUB), then reboot once to apply. This is a one-time
  install-time step, not something the host daemon does per-connect.
- Do not pursue `evdi` or `vkms` further unless this approach fails
  validation — both carry a live-session freeze risk this approach avoids
  entirely by not loading any new driver into the running session.

## Consequences

- **Requires one reboot to validate and to install**, unlike the
  live-session-Xorg-config approach 0005/0006 tested — but carries no
  freeze risk during the change itself, since nothing is reloaded into the
  running X server; the new connector state is only visible after the
  reboot completes.
- **Permanently claims `DP-1-0`** as tabscreen's connector on this machine.
  If a real monitor is ever plugged into that specific physical port, it
  would conflict with the forced EDID; the installer should pick, or let
  the user pick, a connector unlikely to be used for a real display.
- **Not yet empirically validated.** Open items before Phase 2's other
  tasks can proceed:
  - Generate a real EDID binary encoding 1200×1920 @ 60/90Hz (tooling:
    `edid-generator`, `wxedid`, or hand-built via `edid-decode`'s inverse).
  - Add the kernel boot parameter, update initramfs, reboot, and confirm
    `xrandr --query` shows `DP-1-0` connected with the expected modes,
    coexisting with `eDP-1` and `HDMI-1-0` still active.
  - Confirm `xrandr --output DP-1-0 --off` / `--auto` behave as needed for
    the "off until tablet connects" workflow — EDID override should make
    the connector report *connected*, not force it *on*, but this needs to
    be confirmed empirically rather than assumed.
  - Confirm arandr shows the output and allows drag/position/rotate, same
    as the original Phase 2 acceptance criterion.
- Once validated, the TASKS.md Phase 2 checklist items about writing the
  installer's Xorg config snippet and deriving `BusID` from `lspci` no
  longer apply — there's no Xorg config snippet or `BusID` involved in this
  approach. The installer step becomes: generate the EDID file, write it
  to the initramfs-visible location, add the GRUB parameter, and prompt for
  or automate the one-time reboot.
- 0005's live-session hazard finding still stands as general guidance
  (never apply untested Xorg/display config changes directly to a running
  session) even though this specific approach doesn't touch Xorg config at
  all.
