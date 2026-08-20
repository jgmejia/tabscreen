# 13. `ConnectedMonitor` conflicts with "off by default" at the desktop layer, not just the driver

Date: 2026-08-19

## Status

Accepted — identifies a more fundamental problem than 0012's greeter-primary
hypothesis. Supersedes 0012's proposed fix (NULL-default `MetaModes`
combo), which was tried and failed for a different, more informative reason.

## Context

0012 proposed keeping the confirmed-working `Device`+`Screen` config but
defaulting `DFP-1` to inactive via a `NULL` `MetaModes` combo, switching to
an active combo later via plain `xrandr`:

```
Option "MetaModes" "DFP-4: nvidia-auto-select +2560+0, DFP-1: NULL; DFP-4: nvidia-auto-select +2560+0, DFP-1: 1200x1920 +6400+0"
```

Applied via full reboot. Same symptom as 0012: blank/failed-looking login,
recovered via VT-switch + `rm` of the config file. Diagnosis from
`journalctl -b 0` (logs from before the fix, same boot):

1. **The `NULL` combo was rejected by the driver**, not silently ignored:
   ```
   (WW) NVIDIA(0):     "DFP-4:nvidia-auto-select+2560+0,DFP-1:NULL"; removing.
   (II) NVIDIA(0): Validated MetaModes:
       "DFP-4:nvidia-auto-select+2560+0,DFP-1:1200x1920+6400+0"
   ```
   Only the active combo survived validation, so `DFP-1` was live from the
   very start of the boot — the NULL-default trick did not achieve its
   goal. The named-device-prefixed `NULL` syntax (`DFP-1: NULL`) may simply
   not be valid the way the docs' positional example (`"1600x1200, NULL"`)
   is — not confirmed either way, but the practical result is the same:
   this technique doesn't produce an inactive-by-default output on this
   driver.
2. **The same external re-positioning seen in 0012 recurred, at a
   different, worse position:**
   ```
   (II) NVIDIA(0): Setting mode "NULL"
   (II) NVIDIA(0): Setting mode "DP-1: nvidia-auto-select @1200x1920 +0+0..."
   ```
   Something outside our static config actively turned the device off then
   immediately back on, this time at `+0+0` (0012's attempt saw `+0+0` too,
   after a different sequence) rather than our declared `+6400+0`. This
   confirms the repositioning is not an artifact of our specific config
   content — it happens regardless, and lands at whatever default position
   its own logic picks, not ours.
3. **Read `/usr/share/gdm/generate-config`** (the `ExecStartPre` step visible
   in `systemctl status gdm`) directly, as a zero-risk way to rule it in or
   out: it only compiles `dconf` greeter defaults and signals
   `dconf-service`. Not the source of the repositioning.

## Decision

**The real problem is architectural, not a config syntax bug.** `Option
"ConnectedMonitor"` tells the driver — and therefore the entire desktop
session built on top of it (most plausibly `mutter`, running the greeter's
compositor and its own monitor-layout auto-configuration) — that a real
monitor is physically present. A real, connected monitor being
automatically turned on and included in the desktop layout is *correct,
expected* desktop behavior, not a bug to route around. The problem is that
`ConnectedMonitor` only has one signal to give: "this exists" — there's no
way to simultaneously say "this exists, but don't turn it on yet," and the
whole desktop stack (not just Xorg/NVIDIA) reasonably acts on that signal.

This means 0008 through 0012's whole framing — "pre-declare a
permanently-present connector, toggle it on/off later like a real
monitor being plugged in and out" — runs into a wall that has nothing to do
with which specific Xorg option or MetaModes syntax is used. Any mechanism
that makes the driver report the port as *connected* at boot time will hit
the same desktop-level auto-activation behavior.

Not yet attempted, real candidates for a genuine fix:

- **Seed a `monitors.xml`** for the `gdm` user (and/or the real user) that
  explicitly configures only `eDP-1`/`HDMI-1-0` as active, hoping mutter
  respects a saved config over its own auto-layout heuristic instead of
  auto-activating every connected output. Earlier logs already show mutter
  looks for a saved session file and warns (non-fatally) when none exists —
  suggesting this mechanism exists but isn't currently populated.
  Uncertain whether a saved config can express "don't activate this
  connected output" rather than just "this is how the active outputs are
  arranged."
- **Reconsider whether "connected" needs to be a boot-time-static fact at
  all.** The deeper fix may be that `ConnectedMonitor`/`CustomEDID` should
  only be applied by the host daemon *at the moment the tablet actually
  connects* — i.e., a genuinely dynamic/hotplug mechanism — rather than
  pre-declared at boot and toggled with `--off`/`--auto`. This was exactly
  what 0008 was trying to avoid needing (see its whole premise: reuse an
  already-present connector rather than need true hotplug), but the
  evidence now suggests that avoidance doesn't hold once the desktop layer
  is accounted for, not just the driver layer.
- **Accept a different tradeoff for v1**: let the virtual output be active
  from boot alongside the real monitors (not "off until tablet connects"),
  and have the host daemon only control its *content*/positioning rather
  than its connected state. This would technically satisfy `CLAUDE.md`'s
  core requirement (a real, arandr-manageable RandR output) without solving
  the toggle problem — a real scope reduction, not a technical fix, and
  would need explicit buy-in before treating it as Phase 2's actual
  target.

## Consequences

- No lasting damage: recovered the same way as 0012, current session
  independently re-verified healthy (`eDP-1`/`HDMI-1-0` both connected,
  `gdm.service` active).
- This is the fifth live-hardware Xorg-config attempt this session, the
  second producing a failed/blank greeter requiring VT-switch recovery.
  The pattern is now: every attempt that gets the driver to show the
  device "connected" also gets it auto-activated and mispositioned by
  something above the driver, regardless of what our static config says.
  Continuing to iterate `MetaModes` syntax against this specific symptom is
  very unlikely to converge — the next step should address the
  architectural question (can "connected but not yet on" be expressed at
  all, to mutter and not just to the driver) before any further live
  attempt, not another live attempt itself.
- Given the escalating attempt count and repeated recovery cycles, this is
  a reasonable point to pause live-hardware iteration and decide
  deliberately with the user how to proceed, rather than trying a sixth
  variant blind.
