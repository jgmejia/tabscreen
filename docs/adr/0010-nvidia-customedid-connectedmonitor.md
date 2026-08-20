# 10. NVIDIA `ConnectedMonitor`/`CustomEDID` as the virtual-output mechanism

Date: 2026-08-19

## Status

Proposed — research only, not yet applied to any Xorg config or tested
against live hardware. Follows [0009](0009-edid-firmware-wrong-gpu-driver.md),
which ruled out the generic kernel `drm.edid_firmware=` mechanism on this
machine because the idle connectors sit on the NVIDIA GPU, whose proprietary
driver doesn't implement that generic path.

## Context

0009 left one candidate unexplored: NVIDIA's own native equivalent of EDID
override, exposed as Xorg `Device`-section options rather than a kernel
parameter. Researched two sources: the official NVIDIA driver README
(shipped locally at `/usr/share/doc/nvidia-driver-580/README.txt.gz` on this
machine — authoritative, not a web summary) and a real-world precedent blog
post doing the same thing for the same rough purpose (a virtual display for
game streaming, on Ubuntu/Xorg/NVIDIA/GDM, real monitor staying functional
throughout).

**What the options do, per the local README:**

- `Option "ConnectedMonitor" "<name>"` — overrides what the driver detects as
  plugged in. Valid names are `CRT`/`DFP`, optionally with a numeric suffix
  (`DFP-1`) to target a specific connector. The README explicitly says to
  combine this with `CustomEDID` "to provide the simulated display device
  with an EDID" — this is the documented intended use for exactly our case.
- `Option "CustomEDID" "<name>:<file>"` — feeds a real EDID binary file to a
  named display device, semicolon-separated for multiple. Explicitly
  supports reusing an existing generated EDID file (the README's own example
  references one from `nvidia-settings`, but any valid EDID binary works —
  including the one already built and `edid-decode`-validated in
  [0008](0008-edid-override-virtual-output.md),
  `host/resources/edid/tabscreen-honor-pad-x8a.bin`).
- **Caution from the README, verbatim:** "Specifying an EDID that doesn't
  exactly match your display may damage your hardware, as it allows the
  driver to specify timings beyond the capabilities of your display." This
  doesn't apply to our case — nothing is ever physically plugged into the
  target port — but is worth carrying forward as a reminder for anyone
  reading this later: this technique is only safe to use on a port with
  nothing real connected to it.
- **Both options, when set, apply to every X screen on that GPU** — i.e.
  this is additive to whatever's already configured for the GPU, not a
  wholesale replacement, which is the load-bearing difference from 0006's
  dummy-driver failure (that failure came from declaring a competing
  `Screen`, not from adding options to an existing one).
- A third naming scheme is confirmed to exist, on top of the two 0009 already
  found (kernel DRM names, Xorg RandR names): NVIDIA's own `DFP-N`/`CRT-N`
  identifiers are yet a different namespace again from both of those, *and*
  from the `DP-4` name seen in 0009's `nvidia-modeset` dmesg warning — that
  warning used a legacy/internal name distinct from the `DFP-N` names Xorg
  itself logs. The README's own advice is followed here: "check your X log
  to determine which display device identifiers are valid for your
  particular GPU," rather than guessing.

**This machine's real candidates, from `journalctl -b 0 _COMM=Xorg`:**

NVIDIA `GPU-0` enumerates six display devices. `DFP-4` is the real, currently
connected monitor (LG HDR 4K, marked `(boot)`). Of the remaining five:

| Device | Type | Max pixel clock | Usable? |
|---|---|---|---|
| `DFP-0` | Internal TMDS | 165 MHz | No — below our modes' 196.5/302.75 MHz |
| `DFP-1` | Internal DisplayPort | 2670 MHz | Yes |
| `DFP-2` | Internal TMDS | 165 MHz | No |
| `DFP-3` | Internal DisplayPort | 2670 MHz | Yes |
| `DFP-5` | Internal DisplayPort | 2670 MHz | Yes |

The two TMDS-class devices are eliminated outright: their 165 MHz ceiling is
below even our 60Hz mode's 196.5 MHz pixel clock, let alone the 90Hz mode's
302.75 MHz. `DFP-1`, `DFP-3`, `DFP-5` are the real shortlist.

Also confirmed independently in the same log: `eDP-1` (the laptop panel) is
logged by a separate `modeset(0)` driver line, not an `NVIDIA(...)` line —
reconfirming 0009's finding that it's Intel/generic-`modesetting`-owned, a
different Xorg screen entirely from NVIDIA's.

**One real-world caveat, from the precedent blog post
(kovasky.me's Sunshine virtual-display writeup, same rough goal on
Ubuntu/Xorg/NVIDIA/GDM):** even among connectors the driver enumerates,
*"only certain DisplayPort outputs were usable... some GPU ports couldn't
support virtual monitors despite being enumerated."* That post also
confirmed the core coexistence claim we need: its real physical monitor
kept working throughout, with the virtual output added alongside it via
`metamodes` listing both together — this is the same GPU/driver being
extended, not a second one being introduced, which is why it didn't hit
0006's failure mode. That post used explicit `Modes`/`metamodes` instead of
a real `CustomEDID` file, so it doesn't independently confirm `CustomEDID`
specifically works cleanly — only that this general class of NVIDIA
`Device`-option customization is safe to add alongside a live real display.

## Decision

Propose, for the next live attempt: an Xorg config fragment (not yet
written) adding to the NVIDIA GPU's `Device` section:

```
Option "ConnectedMonitor" "DFP-1"
Option "CustomEDID" "DFP-1:/lib/firmware/edid/tabscreen-honor-pad-x8a.bin"
```

reusing the EDID file already generated and validated in 0008 — no new EDID
work needed, only a new place for it to be read from. If `DFP-1` doesn't
work per the precedent's "not every enumerated port actually works" caveat,
`DFP-3` and `DFP-5` are the fallback candidates, in that order (arbitrary
tie-break, no evidence yet distinguishing them).

Not yet decided or attempted:

- **How to inject this without an existing Xorg config file to extend.**
  Confirmed in 0009: no `/etc/X11/xorg.conf.d/*nvidia*` (or any nvidia
  config) exists today. A new fragment file needs to correctly scope to the
  NVIDIA `Device` section without an explicit `BusID`/`Screen`/
  `ServerLayout` accidentally suppressing GDM's own autodetection the way
  0006 found bare `Screen` declarations do for `xf86-video-dummy` — this
  needs to be confirmed specifically for the `nvidia` driver, not assumed
  safe by analogy.
- Whether applying it requires only a GDM/Xorg restart or a full reboot —
  the precedent blog used a full reboot; not yet confirmed whether that's a
  hard requirement or just what that author did.
- The toggle-on-connect / toggle-off-on-disconnect behavior needed for
  Phase 2's "off until tablet connects" workflow — `ConnectedMonitor`/
  `CustomEDID` make the port permanently report connected once the X
  server starts with that config, same basic shape as 0008's kernel-level
  approach; whether `xrandr --output ... --off` cleanly hides it the same
  way still needs empirical confirmation.

## Consequences

- This is Xorg config again — the same risk category as 0005's incident —
  but a different shape of change than either 0005/0006 (introducing a
  competing driver/screen) or 0008 (a kernel parameter, confirmed safe to
  the live session even when ineffective). It extends the *same* driver
  already successfully driving part of the live session, which is why the
  precedent's real monitor survived, but this hasn't been validated on
  *this* machine and shouldn't be assumed safe by analogy alone.
- Before writing anything to `/etc/X11/xorg.conf.d/`, the open items above
  (fragment scoping without an existing base config, restart-vs-reboot
  requirement) need answers — not necessarily full pre-validation in
  isolation (0006 already showed the `Xorg :2` isolation technique can't
  validate a real multi-GPU-adjacent config on this hardware), but at least
  a considered plan for how to back out cleanly if `DFP-1` doesn't come up
  as expected, analogous to 0008's GRUB-backup/GRUB-menu-visibility plan.
- If this works, Phase 2 unblocks via a mechanism that's arguably a better
  fit than the original `xf86-video-dummy` plan ever was: no installer-time
  Xorg driver package, no `BusID` derivation, just one config fragment plus
  the already-built EDID file.
