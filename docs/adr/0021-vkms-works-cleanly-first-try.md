# 21. `vkms` works cleanly, first try — real breakthrough after the `ConnectedMonitor` dead end

Date: 2026-08-19

## Status

Accepted. This is the actual unblock for Phase 2, after ADRs 0005–0019
exhausted the `xf86-video-dummy` and NVIDIA `ConnectedMonitor` approaches.

## Context

Following the reframe in this session's discussion of "build our own
evdi" (see conversation, not a prior ADR): the reason `evdi`/a custom
driver looked necessary was an assumption that the virtual output needs to
expose a real, directly-readable framebuffer. It doesn't — `host/src/video.rs`
already captures via `ffmpeg -x11grab` against the *composited desktop
region*, independent of which driver backs that region. This meant `vkms`
(dismissed earlier only because *its own* internal test-harness capture
feature is CRC-only) was worth retrying on its own terms: does `mutter`
treat a `vkms`-backed output as a normal, positionable RandR output at
all? That's all that was ever actually required.

Loaded `vkms` via `/etc/modules-load.d/vkms.conf` (boot-time, before Xorg
starts — same safe pattern used throughout this session, avoiding any
live-session hotplug risk). No module parameters needed; `vkms`'s default
device creation was sufficient. Result, from a real logged-in session,
first attempt:

```
eDP-1 connected primary 2560x1600+0+0 ...      (unchanged)
Virtual-2-1 connected 1024x768+6400+0 ...      (new, vkms-backed)
HDMI-1-0 connected 3840x2160+2560+0 ...        (unchanged)
```

- `Virtual-2-1` (the `vkms` connector, `/sys/class/drm/card1-Virtual-1`)
  shows up **connected**, with a real mode list (multiple standard
  resolutions, `1024x768` as default/preferred), cleanly positioned to the
  right of the existing desktop (`+6400+0`, matching the combined width of
  `eDP-1`+`HDMI-1-0`), automatically — no manual positioning needed.
- **Neither real monitor was disturbed** — both show their exact normal
  mode lists and current modes, unlike every single `ConnectedMonitor`
  attempt tonight.
- **`Virtual-2-1` is not marked primary** — only `eDP-1` is. The
  wrong-primary-selection bug that consumed hours of NVIDIA testing did
  not occur.
- `xrandr --listproviders` shows `vkms`'s provider already merged with the
  same `modesetting` provider driving `eDP-1` — the RandR 1.4 provider
  merge that caused so much trouble to reason about for NVIDIA happened
  automatically and correctly here.
- `vkms` also exposes a `Writeback-1` connector (a distinct DRM connector
  type for capturing composited output directly) — not needed given
  `x11grab` already works, but worth knowing it exists if `x11grab`
  latency ever becomes a concern later.

This worked because `vkms` is a real, generic, `modesetting`-driver-backed
KMS device — architecturally the same category as `evdi` (separate virtual
GPU, doesn't touch NVIDIA/Intel's own detection state) but with none of
`evdi`'s kernel-version fragility, since it's in-tree and ships with the
kernel itself.

## Decision

Adopt `vkms` as Phase 2's virtual-display mechanism. Remaining work before
calling Phase 2 done:

- Add a custom mode matching the tablet's real panel (1200×1920, 60/90Hz)
  via `xrandr --newmode`/`--addmode` — standard, driver-agnostic RandR
  mechanism, not dependent on `vkms` having its own EDID-injection support.
- Confirm the output can be toggled off/on (`xrandr --output Virtual-2-1
  --off` / `--auto` or equivalent) for the "off until tablet connects"
  workflow — untested so far tonight, this session only checked the
  always-on default state.
- Confirm behavior at the GDM greeter, not just a logged-in session (not
  yet tested for `vkms` specifically — the NVIDIA saga's greeter problems
  don't obviously apply here since the failure mode was NVIDIA-specific,
  but should be verified rather than assumed).
- Verify in arandr: output appears, can be dragged/positioned.
- Reuse the existing, already-validated EDID
  (`host/resources/edid/tabscreen-honor-pad-x8a.bin`) if `vkms` turns out
  to support EDID injection via its configfs interface (present in newer
  kernels) — otherwise the `--newmode` approach above is sufficient on its
  own.
- Once confirmed: point Phase 3's video capture and Phase 4's input
  injection at this real output instead of their current placeholder
  targets.

## Consequences

- This resolves Phase 2's blocker after 21 ADR entries (0005–0021) spent
  on `xf86-video-dummy` and NVIDIA-specific approaches. The eventual
  answer was simpler than anything attempted: a driver that never touches
  a real GPU's own detection state at all.
- Worth noting for the installer/Phase 6 work: `vkms` loading via
  `/etc/modules-load.d/` is a clean, minimal, standard mechanism — no DKMS
  build step, no kernel-version-specific patching, unlike the `evdi` path
  that was seriously considered earlier tonight.
- Still open: whether `vkms`'s default connector naming (`Virtual-2-1`)
  and positioning stay stable across reboots, and how multiple tablets
  (Phase 7's open question) would be handled — `vkms` only creates one
  default device without additional configfs-based configuration.
