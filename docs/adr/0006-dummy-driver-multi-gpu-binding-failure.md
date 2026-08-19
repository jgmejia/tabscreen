# 6. `xf86-video-dummy` cannot bind as a secondary/`Inactive` GPU screen on this Xorg version

Date: 2026-08-19

## Status

Accepted — supersedes the specific config shape proposed in
[0005](0005-live-session-dummy-output-hazard.md)'s Decision section (two
explicit `Screen`s + `ServerLayout`), which testing below showed does not
work as written. The underlying hazard documented in 0005 (live-session risk)
still stands and is reinforced further here.

## Context

Following 0005's incident, this session ran four systematic tests against an
isolated secondary `Xorg :2` instance (`vt9`, `-novtswitch`, never the live
GDM session directly) to find a working merged config before touching
`/etc/X11/xorg.conf.d/` again. Xorg version throughout: `1.21.1.11`
(`xorg-server 2:21.1.12-1ubuntu1.5`, Ubuntu 24.04's stock package).

1. **Two full `Screen`s (`IntelScreen` + `DummyScreen`) under one
   `ServerLayout`, Intel `Device` BusID-pinned (`PCI:0:2:0`).** This is the
   old "Zaphod mode" model (separate desktops per screen) — the wrong
   conceptual fit for a unified drag-between-monitors desktop regardless of
   outcome. Result: `(EE) Screen 1 deleted because of no matching config
   section` — the dummy screen was dropped — and Xorg's `AutoBindGPU`
   default auto-attached the NVIDIA GPU as an offload `GPUDevice` instead,
   which then crashed (`Failed to create pixmap` / `failed to create screen
   resources`, a nouveau-vs-proprietary-nvidia-driver conflict). This is
   likely close to what the original deleted `20-tabscreen-dummy.conf` did,
   and explains 0005's freeze precisely: the real Intel screen driving the
   live desktop would be *replaced*, not augmented, yanking GNOME
   Shell/Mutter's rendering backend out from under a running session — far
   more severe than a simple black screen on an idle secondary instance.
2. **Only the dummy `Device`/`Screen` declared, no `ServerLayout`, no real
   GPU section at all, `AutoAddGPU`/`AutoBindGPU` off.** Worked cleanly —
   `DUMMY0`-`DUMMY15` all present, no errors — reproducing Phase 0's
   original spike result. But confirms declaring *any* explicit `Screen`
   suppresses Xorg's autoprobe of the real GPU screen entirely; it doesn't
   merge. This is a dead end for coexistence, not a step toward it.
3. **RandR 1.4 provider pattern**: Intel as active `Screen 0`
   (BusID-pinned), dummy declared `Inactive` in `ServerLayout` (the
   documented NVIDIA Optimus pattern for a secondary GPU's outputs to join
   the primary screen — see
   [NVIDIA's own RandR 1.4 README](https://download.nvidia.com/XFree86/Linux-x86_64/396.51/README/randr14.html)).
   First attempt had a syntax bug (`Inactive` takes a `Device` identifier,
   not a `Screen` identifier — easy to miss since NVIDIA's own example names
   both sections identically). Fixed and retried: same
   `(EE) Screen 1 deleted because of no matching config section` as
   attempt 1 — the dummy screen still never binds, `Inactive` or not.
   Separately, this test also proved that pinning the *same physical* Intel
   GPU (`BusID PCI:0:2:0`) from a second Xorg process is unsafe even with
   `-novtswitch`: focus slipped to `vt9` anyway (black screen, recovered via
   Ctrl+Alt+F7), and on switching back the second instance crashed with
   `(EE) NVIDIA... modeset(0): drmSetMaster failed: Device or resource busy`
   — real DRM-master contention between the two processes, not a
   theoretical risk.
4. **Same `Inactive`-GPU pattern, but with NVIDIA as the active `Screen 0`**
   instead of Intel, on the theory that NVIDIA is idle on this "on-demand"
   Prime laptop and safe to test against. Same dummy-binding failure
   (`Screen 1 deleted...`) recurred — proving the failure is independent of
   which real GPU is paired with the dummy driver, i.e. it's a
   dummy-driver/Xorg-version issue, not a config or GPU-choice mistake.
   It also disproved the "NVIDIA is idle" assumption: the instance crashed
   with `(EE) NVIDIA(GPU-0): Failed to acquire modesetting permission` when
   focus returned to `vt9` — the live GDM session already holds modesetting
   authority on *both* GPUs (standard for Prime "on-demand": NVIDIA is kept
   ready to drive an externally-plugged monitor without restarting X).

`-novtswitch` failed to hold focus on `vt9` in 3 of the 4 test runs (only
test 2, the successful dummy-only case, didn't trigger a VT switch) — it is
not a reliable isolation guarantee on this system, contrary to what Phase 0's
single successful spike run implied.

## Decision

Two conclusions, both load-bearing for how Phase 2 proceeds:

1. **`xf86-video-dummy` (as packaged in Ubuntu 24.04's `xserver-xorg-core`
   1.21.1.11) cannot be bound as a secondary/offload/`Inactive` GPU screen
   alongside a real GPU in one Xorg config, via any of the three tested
   config shapes.** It works perfectly as the *only* declared screen ("Falling
   back to old probe method for dummy" is a real warning, not just noise —
   dummy isn't a platform/udev-probed driver, and Xorg's modern
   multi-GPU/`GPUScreen`/`Inactive`-binding logic appears to require that).
   This is a driver/Xorg-version compatibility gap, not something fixable by
   further config changes alone — before writing more Xorg config, research
   whether (a) a newer/patched `xf86-video-dummy` build exists that supports
   this, (b) another virtual-output driver (e.g. a `dummy`-like DRM/KMS
   driver, `evdi`, or a purpose-built virtual-display kernel driver) handles
   multi-GPU binding correctly, or (c) there's a known Xorg config idiom for
   this specific driver that none of the above three attempts hit.
2. **This machine has no real GPU that's safe to test an active competing
   `Screen` against from a second process** — both Intel and NVIDIA are
   already claimed for modesetting authority by the live GDM session. Any
   further "does dummy+real-GPU work" testing either has to happen inside
   the live session directly (the exact risk 0005 already burned once) or
   needs a different isolation mechanism entirely (e.g. a VM, or a machine
   not currently running a hybrid-GPU desktop session).

## Considered alternative: switching `prime-select` mode

Considered switching `prime-select` (currently `on-demand`) to `nvidia`
mode, which would fully free the Intel GPU from GDM's modesetting claim and
allow a contention-free test of the actual target pairing (Intel + dummy).
Rejected for now: it requires logging out of the live GNOME session (closing
all open apps) both to switch and to switch back, and — critically — the
`Screen 1 deleted because of no matching config section` failure occurred at
Xorg's config-parsing stage in both GPU tests above, *before* either GPU's
modesetting-permission conflict surfaced, meaning the dummy-binding failure
already looks independent of GPU contention. The logout cost isn't
justified without reason to expect a different outcome. Worth revisiting
only if finding 1's research (driver/version fix) stalls and a
contention-free sandbox becomes worth the disruption for further config
experimentation.

## Consequences

- Phase 2 cannot proceed on the currently-assumed technical path (bare
  `xf86-video-dummy` merged into the live session's Xorg config) without
  first resolving finding 1 above. This blocks the "write installer step"
  and "verify in arandr" TASKS.md items, not just the live-session test.
- The `Xorg :2`-on-a-spare-VT isolation technique, relied on since Phase 0,
  is confirmed to only be safe for testing a dummy-only config with no real
  GPU involved — it cannot be used to validate the actual merge target on
  this hardware. A different validation strategy is needed before more
  config changes are tried against real hardware.
- No further live-hardware Xorg testing should happen without first
  answering finding 1 (is there a driver/version that supports this at
  all) — otherwise further attempts are just re-running the same failure
  against progressively more of the live session's territory.
