# tabscreen — Project Tasks

Task breakdown derived from the decisions in `CLAUDE.md`. Locked-in
decisions are not re-litigated here: no TCP/IP transport (raw USB/AOA only),
custom video-streaming pipeline (not VNC), and the virtual display must be a
real X11 RandR output manageable via arandr/xrandr (X11/Xorg only, not
Wayland).

## Open Decisions (resolve before/during Phase 0)

- [ ] Host daemon implementation language (needs good `libusb` + hardware
      video encode bindings — e.g. Rust or C/C++ are the natural fits).
- [ ] Android client language (Kotlin assumed, not yet confirmed).
- [ ] Video codec baseline: H.264 first (broadest hardware decoder support
      on Android), evaluate H.265/AV1 later.
- [ ] Target distros/desktop environments for v1 (must be X11 sessions —
      pick 1-2 to develop against first, e.g. a plain Xorg + a common WM).

## Phase 0 — Feasibility Spikes

De-risk the two biggest unknowns from `CLAUDE.md` before committing to full
implementation.

- [ ] Spike: raw AOA "hello world" — minimal host tool + minimal Android app
      exchange bytes over USB accessory mode, no video involved.
- [ ] Measure actual sustained AOA bulk-transfer throughput on target
      hardware; use this to bound realistic resolution/frame-rate targets.
- [ ] Spike: `xf86-video-dummy` output declared in Xorg config, confirm it
      shows up in `xrandr --query` and arandr, and can be toggled
      on/off/repositioned via `xrandr` calls at runtime.
- [ ] Confirm target Android tablet(s) support USB accessory (AOA) mode in
      practice (varies by OEM/Android version).

## Phase 1 — USB Transport (AOA)

- [ ] Define wire protocol: length-prefixed frames, channel IDs for
      video / input / control messages.
- [ ] Host: implement AOA host-side handshake (accessory mode negotiation)
      and bulk endpoint read/write.
- [ ] Android: implement `UsbAccessory` registration (accessory filter,
      intent-filter, permission handling) and bulk endpoint read/write.
- [ ] Connect/disconnect detection on both ends; clean teardown.
- [ ] Basic reconnect logic (tablet unplugged/replugged mid-session).

## Phase 2 — Virtual Display (X11 RandR)

- [ ] Write installer step that adds a `xf86-video-dummy` Xorg config
      snippet (one or more pre-declared, initially-off dummy outputs).
- [ ] Host daemon: on tablet connect, run the `xrandr --output DUMMYn
      --mode ... --pos ... --auto` sequence; on disconnect, `--off`.
- [ ] Verify in arandr: output appears, can be dragged/positioned/rotated,
      and disappears cleanly on tablet disconnect.
- [ ] Handle the "no free dummy output available" case (e.g. second tablet,
      or dummy outputs not pre-provisioned) with a clear error.

## Phase 3 — Video Pipeline

- [ ] Host: capture the dummy output's framebuffer (X11 capture of that
      specific RandR output/CRTC, not the whole desktop).
- [ ] Host: hardware-accelerated H.264 encode of the captured frames.
- [ ] Host: send encoded frames over the Phase 1 transport's video channel.
- [ ] Android: decode via `MediaCodec` (hardware decoder), render fullscreen
      (`SurfaceView`/`TextureView`).
- [ ] Static-image end-to-end test: single frame captured → encoded → sent
      → decoded → displayed.
- [ ] Continuous streaming: measure latency and achieved frame rate against
      the Phase 0 AOA throughput ceiling; tune encode bitrate/resolution.

## Phase 4 — Input Injection

- [ ] Define input event schema (touch down/move/up, coordinates, pressure
      if stylus-capable) on the control channel.
- [ ] Android: capture touch/stylus events, send over USB.
- [ ] Host: inject received events as pointer input against the dummy
      output's coordinate space (e.g. via `uinput` or `XTestFakeMotionEvent`).
- [ ] Verify coordinate mapping is correct across output position/rotation
      changes made via arandr.

## Phase 5 — Mode Negotiation

- [ ] Android: report tablet panel resolution + refresh rate during the
      Phase 1 handshake.
- [ ] Host: register a matching `xrandr` mode (`--newmode`/`--addmode`)
      instead of a hardcoded dummy resolution.
- [ ] Handle tablet rotation (portrait/landscape) — decide whether this
      remaps the X11 output's mode or is handled client-side only.

## Phase 6 — Packaging & Installation

- [ ] udev rule(s) granting non-root USB access to the AOA device and
      triggering the host daemon on plug-in.
- [ ] systemd (user) service definition for the host daemon.
- [ ] Installer script: Xorg config snippet + udev rule + service
      registration, with a clean uninstall path.
- [ ] Android app packaging for sideload distribution (no Play Store
      dependency assumed initially).

## Phase 7 — Reliability & Polish

- [ ] Graceful handling of encode/decode errors and dropped frames.
- [ ] Session logging on both ends for field debugging (USB-level tools
      per `CLAUDE.md`, since network tools don't apply).
- [ ] Multiple-tablet support decision: in scope for v1 or explicitly
      deferred?
- [ ] Battery/thermal impact check on the tablet during extended use.

## Phase 8 — Documentation & Release

- [ ] User-facing setup guide (install steps, enabling accessory mode,
      first-connect flow).
- [ ] Update `CLAUDE.md` with any architecture decisions finalized during
      implementation that diverge from the current plan.
- [ ] Versioned release/tag of host daemon + Android client.
