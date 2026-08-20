# 9. `drm.edid_firmware=` has no valid target on this machine

Date: 2026-08-19

## Status

Accepted — records why [0008](0008-edid-override-virtual-output.md)'s
approach failed empirically and what's known about the alternative.
0008's connector-naming and GPU-vendor assumptions are superseded by the
findings below; its core insight (avoid adding a second driver/screen,
reuse a connector on a GPU already driving the session) still stands as
the right shape of solution — it just needs a different mechanism on this
specific hardware.

## Context

Following 0008's plan: generated and `edid-decode`-validated an EDID binary
for the HONOR Pad X8a (1200×1920, 60/90Hz), installed it at
`/lib/firmware/edid/tabscreen-honor-pad-x8a.bin`, added
`drm.edid_firmware=DP-1-0:edid/tabscreen-honor-pad-x8a.bin` to
`GRUB_CMDLINE_LINUX_DEFAULT` (backing up `/etc/default/grub` first, and
switching `GRUB_TIMEOUT_STYLE`/`GRUB_TIMEOUT` from hidden/0 to menu/5s so
the GRUB-menu escape hatch would actually be usable), and rebooted.

Result: `eDP-1` and `HDMI-1-0` came up fine (no session damage — confirms
this mechanism, unlike 0005/0006's Xorg config edits, really doesn't risk
the live session). But `DP-1-0` stayed `disconnected` in `xrandr`. Diagnosis:

1. **`/sys/module/drm/parameters/edid_firmware` showed the parameter was
   received and parsed correctly** (`DP-1-0:edid/tabscreen-honor-pad-x8a.bin`)
   — ruling out a parameter-name or parsing problem.
2. **`DP-1-0` is not a real kernel DRM connector name at all.** `/sys/class/drm/`
   shows the actual connectors are `DP-1`, `DP-2`, `eDP-2`, `HDMI-A-1` (all on
   `card0`) and `eDP-1` (on `card1`) — no `-0`/`-1`-style suffix. That suffix
   is Xorg's own RandR output naming, layered on top of the kernel's real
   connector names, not the same string.
3. **0008's GPU-vendor assumption was backwards.** `card0` (the one with the
   idle `DP-1`/`DP-2` connectors 0008 was targeting) is **NVIDIA**
   (`PCI_ID=10DE:2860`); `card1` is **Intel** (`PCI_ID=8086:A788`). Intel's
   `card1` exposes exactly one connector, `eDP-1` — the laptop panel,
   already in use. **Intel has no idle connector on this machine at all.**
4. **NVIDIA's proprietary driver doesn't implement the generic
   `drm.edid_firmware=` mechanism regardless of naming.** That mechanism
   (`drm_load_edid_firmware()`) is part of the open `drm_kms_helper` code
   path used by drivers like `i915`, `amdgpu`, `nouveau` — not by the
   proprietary `nvidia`/`nvidia-modeset`/`nvidia-drm` stack, which does its
   own separate EDID/display management. Confirmed in `dmesg`:
   `nvidia-modeset: WARNING: GPU:0: Unable to read EDID for display device
   DP-4` — a *third* naming scheme, internal to NVIDIA's driver, distinct
   from both the kernel DRM names and Xorg's RandR names.

Net: the GPU that supports this kernel parameter (Intel) has no spare
connector; the GPU with spare connectors (NVIDIA) doesn't support this
kernel parameter. There is no valid `<connector>` value that makes 0008's
approach work on this hardware.

Change reverted cleanly: restored the backed-up `/etc/default/grub`,
re-ran `update-grub`, confirmed `diff` shows the file identical to the
pre-change backup. Takes effect next boot; no further reboot forced yet.

## Decision

Do not pursue `drm.edid_firmware=` further on this machine — there is no
connector it can bind to. The next candidate, not yet attempted, is
NVIDIA's own native equivalent: `Option "ConnectedMonitor"` +
`Option "CustomEDID"` in an Xorg `Device` section for the `nvidia` driver,
the standard mechanism NVIDIA users rely on for headless/virtual displays
without a physical dummy plug. This targets the same idle `DP-1`/`DP-2`
connectors 0008 already confirmed are free, just via NVIDIA's own
driver-level path instead of the generic kernel one.

This has not been attempted yet. Confirmed so far: there is currently **no
existing Xorg config file for `nvidia` at all** (`grep -rl nvidia
/etc/X11/xorg.conf.d/ /etc/X11/xorg.conf` returns nothing) — this system's
dual-GPU setup, including the already-working `eDP-1` + `HDMI-1-0` merge, is
fully autodetected with zero static config. Adding `ConnectedMonitor`/
`CustomEDID` means introducing an Xorg config file where none exists today.

## Consequences

- This is a materially different risk shape than 0005/0006's dummy-driver
  attempts (adding a competing driver/screen) or 0008's kernel-parameter
  approach (doesn't touch Xorg at all, confirmed safe to the live session
  even when it silently didn't work) — it's Xorg config again, but scoped
  to *adding options to a GPU already driving part of the live session*
  rather than introducing a second driver/screen. Plausibly lower risk than
  0005's incident, but unproven, and should not be assumed safe without the
  same care: validate before it ever lands in `/etc/X11/xorg.conf.d/` live.
- Before attempting the Xorg-config route, worth a deliberate go/no-go
  check-in given the live-session risk category, rather than proceeding
  straight from this finding into another config edit.
- `host/resources/edid/tabscreen-honor-pad-x8a.bin` and its generator
  (`generate_edid.py`) remain valid and reusable — the EDID content itself
  was never in question, only the mechanism for making a connector report
  it. Whatever mechanism is ultimately used (NVIDIA `CustomEDID`, or
  something else), it can point at the same binary.
- The `GRUB_TIMEOUT`/`GRUB_TIMEOUT_STYLE` visibility fix from 0008 was
  reverted along with the EDID parameter (same backup file covered both).
  If a future attempt wants the GRUB menu visible again as a safety net,
  that needs re-applying — it's not implied by anything currently in place.
