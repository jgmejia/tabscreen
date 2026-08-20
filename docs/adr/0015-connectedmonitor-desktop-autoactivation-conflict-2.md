# 15. `monitors.xml` didn't stop it either; reframing away from the greeter

Date: 2026-08-19

## Status

Accepted — sixth live-hardware attempt this session. Records both the
`monitors.xml` result and a strategic reframing that changes what's actually
worth testing next.

## Context

Applied 0014's proposed test: the same confirmed-working `Device`+`Screen`
Xorg config from 0012, plus a `monitors.xml` seeded at both plausible
locations (`/etc/xdg/monitors.xml` and `/var/lib/gdm3/.config/monitors.xml`,
the latter owned `gdm:gdm`), with `eDP-1`/`HDMI-1-0`'s exact real identities
(queried live via `org.gnome.Mutter.DisplayConfig.GetCurrentState` — vendor
`BOE`/`GSM`, product `0x0b8b`/`LG HDR 4K`, serial `0x00000001`/
`0x0007311f` — removing the guesswork 0014 flagged) and `DP-1`/`TBS`/
`TabScreen`/`0x00000001` in a `<disabled>` block for the virtual output.

Result: same symptom as 0012/0013 — blank/failed greeter, recovered via
VT-switch (this time the user only removed the Xorg config file, correctly
reasoning the `monitors.xml` files are inert without it — confirmed
correct: nothing makes `DP-1` exist for the `<disabled>` entry to match
against without `ConnectedMonitor`/`CustomEDID`).

**Diagnosis from `journalctl -b 0`:** identical failure signature to every
prior attempt —
```
(II) NVIDIA(0): Setting mode "NULL"
(II) NVIDIA(0): Setting mode "DP-1: nvidia-auto-select @1200x1920 +0+0..."
```
`monitors.xml`'s `<disabled>` entry had no visible effect.

**New finding: timing rules out the "happens before mutter" theory.**
`gnome-shell[2393]: Running GNOME Shell (using mutter 46.2) as a X11 window
and compositing manager` is logged at the same second (`17:52:02`) as the
re-activation sequence. This is consistent with the re-activation being
mutter's own action (contradicting a hypothesis this session briefly
considered — that it might be a generic pre-mutter Xorg/RandR auto-config
step) — though not proven with certainty at 1-second log granularity.

**Root cause still not confirmed among the live options:**
- The seeded config's exact values (rate precision, connector-name form)
  might not match closely enough for mutter's validation, causing total
  fallback to auto-config.
- Or, more likely given the *consistency* of this exact failure across six
  different config variants: **GDM's greeter may simply not consult
  `monitors.xml` (or not the same way a real user session does) at all** —
  plausibly by design, since a stale or tampered saved config being able to
  affect the trusted pre-login screen is a real thing a display manager
  would reasonably want to avoid.

## Decision

**Reframe the actual goal before attempting a seventh live variant.**
Everything tried so far (0006 through this entry) has targeted getting the
*greeter* to behave — but the greeter's behavior is not what Phase 2
actually needs. The host daemon that will toggle the virtual output on
tablet-connect only ever runs during an already-logged-in session, never
during the greeter. If the *logged-in session* correctly respects
`monitors.xml`'s `<disabled>` entry (via the real user's own
`~/.config/monitors.xml`, not the greeter's), the greeter rendering onto
the wrong output for a few seconds during boot is a cosmetic annoyance, not
a blocker — and might not even need fixing at all.

This is also directly testable without further guessing: 0011's diagnosis
showed a prior attempt's login *actually succeeded* (password accepted,
session opened) despite the greeter appearing broken/blank — keyboard
input still reaches the login prompt even when nothing is visibly
rendered. So the next test doesn't need a new config variant; it needs a
different reaction to the same blank screen: **type the username and
password blind, the way 0011 showed already works, instead of immediately
falling back to VT-switch recovery** — to reach the real session and check
whether `DFP-1` behaves correctly there, independent of whatever the
greeter does.

Not yet attempted: this blind-login approach, and (only if warranted after
that) whether a real user's own `~/.config/monitors.xml` is needed in
addition to the system-level one already in place.

## Consequences

- Given six live attempts this session, several requiring recovery, this
  is a natural point to pause and confirm direction with the user before a
  seventh — specifically, whether they're willing to try blind-login
  through a blank greeter (a different kind of ask than "apply this config
  and reboot") as the next diagnostic step.
- If the logged-in session turns out to handle this correctly, the
  greeter's cosmetic misbehavior becomes a real but much lower-priority
  Phase 7/8 polish item, not a Phase 2 blocker — a significant scope
  clarification if confirmed.
- `monitors.xml` at both seeded locations and the Xorg config file (already
  removed by the user) remain as they were left; no new cleanup needed
  beyond what the user already did.
