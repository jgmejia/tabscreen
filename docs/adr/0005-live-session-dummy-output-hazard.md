# 5. Live-session xf86-video-dummy config caused a full desktop freeze

Date: 2026-08-19

## Status

Accepted

## Context

An earlier attempt at Phase 2 wrote a real Xorg config snippet
(`/etc/X11/xorg.conf.d/20-tabscreen-dummy.conf`) directly onto this
machine's live GDM/GNOME session — not the isolated `Xorg :2` spike instance
used to validate the dummy driver in Phase 0
([0004](0004-target-scope-v1.md)). Applying it froze the desktop hard enough
that recovery required `sudo rm`-ing the file blind (no working session to
inspect it in) and `sudo shutdown now`, followed by several very short,
unstable boot cycles before the machine came back up normally. The offending
file no longer exists, so its exact contents aren't recoverable — but the
shell history and journalctl boot log (three sub-3-minute boots right after
the incident) confirm the sequence of events.

This machine's actual Xorg config today has **zero explicit `Device`/
`Screen`/`ServerLayout` sections** — confirmed via
`~/.local/share/xorg/Xorg.1.log.old`, which shows both the Intel iGPU
(`modesetting` driver, auto-probed via DRM/udev, "Default Screen Section")
and the Nvidia dGPU (`NVIDIA(G0)` GPU-provider) being fully autoconfigured.
The only files under `/usr/share/X11/xorg.conf.d/` touching GPU config are
small `OutputClass`/`ServerLayout`-option snippets (`10-nvidia.conf`), not
full explicit device declarations. This is the standard, fragile-if-touched
setup for an Nvidia Prime "on-demand" hybrid-GPU laptop.

Web research into this class of problem (see Sources) confirms it's a known
failure mode, not a one-off: on a Prime/Optimus hybrid-GPU system, dropping
*any* explicit `Device` section into `xorg.conf.d` disrupts the
autoconfiguration path Xorg otherwise uses to correctly claim the Intel and
Nvidia GPUs, and doing it safely requires an explicit `ServerLayout` that
correctly lists every screen (real + dummy) and marks GPU roles — omitting
that (which is the most likely mistake in the deleted file, though it can't
be confirmed) is a well-documented cause of exactly this symptom: a hung/
black desktop that needs a hard recovery, not a clean X error.

Phase 0's dummy-driver spike never exercised this interaction: it ran a
fully separate, minimal `Xorg :2` instance on a spare VT with nothing else
configured, so it never had to coexist with the live session's hybrid-GPU
autoconfig. That spike's success does not by itself validate a config meant
to merge into the live GDM instance.

## Decision

Phase 2's real installer snippet must not repeat a bare `Device "dummy"`
drop-in. Instead:

- Pin the Intel GPU's `Device` section to its actual `BusID` (`PCI:0:2:0`
  per `lspci`, i.e. `00:02.0`) rather than leaving it auto-probed, once any
  other explicit `Device` exists in the same config tree.
- Add the dummy `Device` section with no `BusID` (it's virtual, not a real
  PCI device).
- Add explicit `Screen` sections for both, and an explicit `ServerLayout`
  that lists the Intel screen as primary and the dummy screen(s) alongside
  it (`Screen 1 "DummyScreen" RightOf "IntelScreen"`), while preserving the
  existing `Option "AllowNVIDIAGPUScreens"` so Nvidia Prime offload keeps
  working as a GPU provider (it doesn't need its own `Screen` entry).
- Re-validate this *combined* shape (explicit Intel + dummy + ServerLayout,
  not dummy alone) against an isolated `Xorg :2`-style secondary instance
  before it ever touches `/etc/X11/xorg.conf.d/` on the live session again.
- Before any live-session test, keep an independent recovery path open
  (e.g. an active SSH session, or a plain-TTY login on another VT) so a bad
  config can be reverted (`mv`/`rm` + `systemctl restart gdm` or reboot)
  without a blind `sudo shutdown now`.

## Consequences

- Phase 2 gains a real prerequisite step — proving the merged (real GPU +
  dummy) config shape is safe in isolation — before the "verify in arandr"
  checklist item that was already planned.
- The installer's Xorg snippet is now known to require host-specific values
  (the Intel `BusID`) rather than being a fully generic drop-in; the
  installer step should derive this from `lspci` at install time rather
  than hardcoding it, since v1's scope is this machine only
  ([0004](0004-target-scope-v1.md)) but a hardcoded BusID would silently
  break on any other hybrid-GPU layout.
- No physical hardware damage occurred, but this is now documented as a
  real operational risk to respect, not just a theoretical one — repeat
  testing on the live session should not happen without a recovery path
  open.

## Sources

- [ServerLayout ignored: multi-GPU NVIDIA xorg.conf — Manjaro Linux Forum](https://forum.manjaro.org/t/serverlayout-ignored-multi-gpu-nvidia-xorg-conf/59168)
- [\[SOLVED\] Nvidia GPU not used by X server with Dual GPU config — Gentoo Forums](https://forums.gentoo.org/viewtopic.php?t=1167414)
- [\[SOLVED\] Black screen, nvidia optimus, nvidia-drivers-361.28 — Gentoo Forums](https://forums.gentoo.org/viewtopic-t-1038828.html)
