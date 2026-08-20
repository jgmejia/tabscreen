# 22. Video capture and input injection target the real `vkms` output; host daemon owns its lifecycle

Date: 2026-08-20

## Status

Accepted.

## Context

ADR 0021 confirmed `vkms` gives a real, correctly-positioned RandR output.
Phase 3 (`host/src/video.rs`) and Phase 4 (`host/src/input.rs`) had shipped
against a placeholder in the meantime — the whole X11 screen / root window
— explicitly noted in both files' doc comments as temporary pending Phase
2. With Phase 2 unblocked, this is the integration work to retire that
placeholder.

## Decision

Added `host/src/display.rs`, a new module responsible for the tabscreen
output's full runtime lifecycle:

- **Finds** the output by name prefix (`Virtual-*`), not the exact
  `Virtual-2-1` seen in ADR 0021's testing — the numeric suffix depends on
  DRM card enumeration order and isn't guaranteed stable across reboots.
- **Activates** it: registers a custom mode via `xrandr --newmode`/
  `--addmode` (CVT timings, computed by shelling out to `cvt` — reusing
  established tooling rather than hand-rolling CVT math in Rust) if the
  mode doesn't already exist, then positions it via `xrandr --output ...
  --mode ... --pos ...`. Position is **computed dynamically** as the
  rightmost edge of whatever real outputs are currently active, not
  hardcoded to this dev machine's `+6400+0` layout from live testing —
  that number is an artifact of this laptop's specific eDP-1+HDMI-1-0
  layout, not a portable constant.
- **Deactivates** it (`xrandr --output ... --off`) via a `Drop` impl on the
  `ActiveOutput` handle `activate()` returns — so the output reliably goes
  back off when a connection cycle ends for *any* reason (tablet unplug,
  error, normal return), without every exit path needing to remember to
  call a cleanup function explicitly.

`host/src/main.rs`'s `run_one_connection()` now calls `display::activate()`
right after the AOA `HELLO` exchange, and passes the resulting
`OutputRect` into both `video::spawn_encoder(&rect)` (x11grab capture
offset/size) and `input::Injector::new(conn, root, rect)` (touch coordinate
mapping — normalized `0.0-1.0` scaled and offset into the rect, same wire
format as before, no protocol change). `Injector` changed from owning its
own X connection (opened once at daemon startup) to borrowing a connection
opened once in `main()` and shared across reconnects — needed since
`display::activate()` and `Injector` both need the same `RandR`/`XTEST`
connection now, and `RustConnection` isn't `Clone`.

Mode registration turned out **not idempotent**: re-running `xrandr
--newmode` with a mode name that already exists fails (`BadName`,
`RRCreateMode`) — confirmed live. `activate()` checks the screen
resources' existing mode list first (parsing `GetScreenResourcesCurrent`'s
packed `names` buffer against each `ModeInfo`'s `name_len`) and only calls
`--newmode` if the name isn't already registered. `--addmode` and the
`--mode`/`--pos` activation call, by contrast, are safely idempotent —
confirmed by running the full sequence twice in a row live.

Checked `vkms`'s configfs interface (`/sys/kernel/config/vkms`) as a
possible path to reuse the already-validated
`host/resources/edid/tabscreen-honor-pad-x8a.bin` instead of `--newmode`:
it exists on this kernel but is empty — the working `Virtual-2-1` comes
from the module's own default-device creation, not configfs. Using it
would mean building an entire device from scratch (planes/crtcs/encoders/
connectors wired via configfs symlinks) for no real benefit over
`--newmode`, which already fully works. Not pursued; only worth
revisiting if Phase 7's multi-tablet support needs configfs-based device
creation anyway.

## Consequences

- Live-validated in isolation (a throwaway `#[ignore]`d test against the
  real X session, deleted after confirming — same pattern used for Phase
  4's original XTEST validation): `activate()` correctly computed
  `+6400+0` from the currently-active real outputs and set `1200x1920`;
  dropping the returned handle cleanly turned the output back off.
- **Not yet validated with a real tablet physically connected.** This
  environment has no udev rule granting non-root USB access to the AOA
  device (Phase 6 TODO, not yet written) and no interactive `sudo`
  available to this session — the full path (AOA connect → `HELLO` →
  activate → capture/inject against the real rect → deactivate on
  unplug/disconnect) needs the user to run `sudo ./target/debug/host` with
  the tablet plugged in, matching how every other privileged/hardware step
  this session has been done.
- `display::activate()` currently just returns the first `Virtual-*`
  output found — with only one such output ever present today this is
  fine, but a second tablet (Phase 7's still-open multi-tablet question)
  would silently collide onto the same output rather than erroring
  clearly. Flagged in `TASKS.md`, not fixed here — out of scope until
  Phase 7 actually needs multiple simultaneous outputs.
- `TABLET_WIDTH`/`TABLET_HEIGHT`/`TABLET_REFRESH_HZ` in `main.rs` are a
  hardcoded placeholder (the HONOR Pad X8a's confirmed real panel spec, per
  Phase 0) until Phase 5 actually negotiates the connected tablet's real
  mode over the wire instead of assuming this one.
- GDM greeter behavior with `vkms` active is still unverified (unchanged
  from ADR 0021 — this work didn't touch that question).
