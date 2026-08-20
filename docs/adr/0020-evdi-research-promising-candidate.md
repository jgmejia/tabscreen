# 20. `evdi` research: promising, architecturally avoids 0019's failure mode

Date: 2026-08-19

## Status

Proposed — research only, nothing installed or attempted live yet.

## Context

0019 conclusively ruled out `ConnectedMonitor`/`CustomEDID` (breaks real
external monitor detection on NVIDIA, confirmed causally across 9 boots).
Revisited `evdi`, deprioritized back in 0006/0008 for a live-session-freeze
risk found in community reports, to see whether that risk is avoidable and
whether the mechanism sidesteps 0019's specific failure mode.

**Why `evdi` is architecturally different from what just failed:**

- It's a genuine, separate DRM/KMS device (`/sys/devices/evdi`), not an
  override on an existing GPU's own detection state. It cannot break
  NVIDIA's real monitor detection because it never touches NVIDIA's driver
  at all.
- It's driven by Xorg's **generic** `modesetting` driver
  (`Option "AccelMethod" "none"`, `MatchDriver "evdi"`) — the same driver
  already working reliably for Intel's `eDP-1` all evening. No NVIDIA
  proprietary config involved.
- Confirmed (DisplayLink's own module README): **no real DisplayLink dock
  or proprietary userspace daemon required** — virtual displays can be
  created directly via a kernel sysfs interface (`echo 1 >
  /sys/devices/evdi/add`), independent of any real hardware.

**Directly addresses the "wrong primary" failure class that consumed
tonight:** the documented `modprobe.d` config includes
```
options evdi initial_device_count=4
softdep evdi pre: drm_kms_helper i915
```
The `softdep` line explicitly exists so "the compositor will not attempt
to use evdi driver as primary gpu" — i.e., the exact bug class 0012/0013
spent hours on (a fake output winning the primary/render-target slot) is
something `evdi`'s own maintainers already identified and built a
documented fix for, rather than something we'd be discovering and working
around ourselves.

**Also addresses the freeze risk that deprioritized this option originally:**
that risk (confirmed in 0006's earlier research, from community reports)
was specifically about loading the kernel module *while Xorg is already
running* and dynamically adding a device to a live X server's static
device list. The `initial_device_count` module parameter creates the
virtual devices at **module load time** — if `evdi` is loaded via
`/etc/modules` (boot-time, before Xorg ever starts, same pattern already
used successfully for other kernel modules on this system), Xorg would see
the devices as already present from its very first startup, never
triggering the "dynamically add to a live server" scenario that caused the
freeze in those reports.

**`vkms` ruled out as a serious candidate**, separately: it's explicitly
documented as experimental/testing-only, and in some configurations
doesn't expose a real readable framebuffer at all (just computes a CRC per
frame for driver-validation purposes) — not suitable for actually
capturing and streaming real desktop content, which is the whole point.

**Prerequisites confirmed ready on this machine:** kernel headers for the
running kernel (`7.0.0-29-generic`) are installed, `dkms` is installed,
and `evdi-dkms` (`1.14.2+dfsg-1ubuntu1.4`) is available directly from
Ubuntu's `universe` repo — no manual build from source needed.

## Decision

Not yet decided whether to attempt live. Proposed plan, for review before
any live step:

1. Install `evdi-dkms` via `apt` (builds and loads cleanly via the
   standard Ubuntu package, no manual DKMS work).
2. Configure `/etc/modprobe.d/evdi.conf` with `options evdi
   initial_device_count=1` (only need one virtual output) and the
   `softdep` line adjusted for this system's actual primary GPU situation
   — needs checking whether `i915` alone is right given NVIDIA, not Intel,
   is what's been acting as the "primary"/boot screen all evening, or
   whether both `i915` and `nvidia` need listing.
3. Add `evdi` to `/etc/modules` so it loads at boot, before Xorg —
   avoiding the live-session freeze scenario entirely per the reasoning
   above.
4. Add the Xorg `modesetting`/`MatchDriver "evdi"` config fragment.
5. Reboot, verify via `xrandr --listproviders` and `xrandr --query`
   (from a working session, not just the greeter) that a new evdi-backed
   output appears, is not selected as primary, and doesn't disturb
   `eDP-1`/`HDMI-1-0`.
6. Only then test the EDID injection (reusing the existing, validated
   `host/resources/edid/tabscreen-honor-pad-x8a.bin`) and the on/off
   toggle behavior needed for Phase 2.

## Consequences

- This is a meaningfully different risk profile than the NVIDIA-based
  attempts: a new kernel module needs installing (a real, if standard,
  system change) and reused on this laptop for the first time — worth the
  same care as prior live attempts (backup plan, careful reboot protocol)
  even though the specific failure modes that plagued 0010–0019 shouldn't
  apply here.
- Still need to independently confirm the `softdep` ordering is correct
  for an Intel+NVIDIA hybrid system (the documented example assumes `i915`
  is the primary GPU; this system's "primary"/boot screen has actually
  been NVIDIA all evening) — not yet researched.
- If this works, it would also resolve the toggle problem more cleanly
  than anything tried with NVIDIA: sysfs `add`/`remove` for the virtual
  device is a genuinely dynamic, live mechanism (unlike `ConnectedMonitor`,
  which required a static config read once at Xorg startup) — worth
  testing whether `evdi` devices can be added/removed live, without an
  Xorg restart, which would be a real improvement over every option
  considered in 0018.
