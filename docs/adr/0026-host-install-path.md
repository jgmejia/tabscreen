# 26. Host install path: udev rule + systemd user service + installer script

Date: 2026-08-20

## Status

Accepted. Live-validated end-to-end.

## Context

Through this entire project, the host daemon has only ever been run
manually (`sudo ./target/debug/host` in a terminal, or, once ADR 0025
established `sudo` isn't actually required, `./target/debug/host`
directly). This directly conflicts with `CLAUDE.md`'s stated goal that
tabscreen should "feel like a real peripheral" — plug in, work
immediately — not require a terminal command every session.

## Decision

Three artifacts, tied together by an installer script:

- **`host/resources/udev/69-tabscreen-aoa.rules`**: grants non-root USB
  access to the tablet at both stages of the AOA handshake — vendor
  `339b` (HONOR) before the handshake, vendor `18d1` product `2d00`/
  `2d01` (Google, accessory / accessory+ADB) after — mirroring
  `aoa.rs`'s own `TARGET_VENDOR_ID`/`GOOGLE_VENDOR_ID`/`ACCESSORY_PIDS`
  constants exactly, so the rule can't silently drift out of sync with
  what the code actually matches. Uses `TAG+="uaccess"` (systemd-logind
  seat ownership, the modern mechanism) with `GROUP="plugdev"`/`MODE`
  as a fallback.

  **Explicitly not** a udev-triggered spawn-on-plug-in rule, despite
  that being what the original Phase 6 checklist item described. The
  daemon already has its own internal wait-for-device loop
  (`aoa::wait_for_candidate_device`, up to 1hr) and full reconnect
  logic (Phase 1) built for a long-running process, not a fresh
  process per USB event — a udev-spawn model would mean either
  reworking the daemon to exit cleanly per-disconnect (losing the
  existing reconnect logic's value) or risking double-spawn races
  between udev and an already-running instance. A continuously-running
  service is the better fit for the architecture that already exists.

- **`host/resources/systemd/tabscreen.service.template`**: a systemd
  *user* service (not system-wide) — the daemon needs the desktop
  session's X11 `DISPLAY`/`XAUTHORITY`, which a system service wouldn't
  have. Templated rather than static because a user service's own
  environment doesn't reliably inherit `DISPLAY`/`XAUTHORITY` from the
  graphical session on every distro/session-manager combination;
  `install.sh` substitutes real values from the installing shell's own
  environment at install time instead of hoping systemd's environment
  import already works.

- **`host/install.sh`** / **`host/uninstall.sh`**: release build, binary
  to `~/.local/bin/tabscreen-host`, the udev rule (sudo), the `vkms`
  module-load config (sudo, idempotent — reuses ADR 0021's boot-time
  setup if already present rather than duplicating it), and the
  systemd service (`enable --now`). Refuses to run as root (it calls
  `sudo` itself only for the specific steps that need it) and refuses
  to run without `DISPLAY`/`XAUTHORITY` already set (catches the "ran
  from a bare SSH session/TTY" mistake early with a clear message
  instead of a confusing later failure). `uninstall.sh` reverses every
  step.

## Consequences

- **Live-validated by the user directly** (not this session — genuinely
  can't run an installer needing interactive `sudo`): ran `./install.sh`
  from a real terminal, output showed a clean build, the udev rule
  install, the `vkms` check, and the systemd service coming up
  `active`/`enabled` within milliseconds. Confirmed via `journalctl
  --user -u tabscreen.service` and a live tablet connection immediately
  afterward that the service-managed daemon behaves identically to every
  manual run this session — correct activation
  (`1920x1200+6400+0 (Right)`), video decode confirmed on the tablet
  (`released>0 errors=0`), same as ADR 0022/0023's validation.
- One non-issue surfaced during validation, already documented (Phase 1):
  the Android app needed a fresh relaunch to pick up the connection,
  since it had been sitting idle from an earlier manual test when the
  service started. Not a regression or new bug — this is the
  already-known `ACTION_USB_ACCESSORY_ATTACHED`-unreliable-on-a-
  long-running-app-instance finding, unrelated to the install path
  itself. Worth keeping in mind for Phase 7/8's setup guide: first
  real-world use should be from a freshly-launched app, not one left
  running from development.
- The daemon is now `enabled`, meaning it starts automatically on future
  logins without any manual step — the actual "feels like a real
  peripheral" goal, for the host side at least. The tablet side (the
  Android client) is still a manually-launched, manually-installed debug
  APK — that's the remaining, not-yet-started Phase 6 item (Android app
  packaging for sideload distribution).
- `install.sh` was deliberately never run by this session itself — it
  needs the user's interactive `sudo` password, which this environment
  can't supply. This is consistent with this whole project's established
  posture toward system-level changes (`CLAUDE.md`'s installation note):
  written, syntax-checked, and its non-privileged parts (the release
  build, the template substitution) verified directly, but the actual
  privileged installation was always going to be the user's own action.
