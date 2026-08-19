# tabscreen — Project Tasks

Task breakdown derived from the decisions in `CLAUDE.md`. Locked-in
decisions are not re-litigated here: no TCP/IP transport (raw USB/AOA only),
custom video-streaming pipeline (not VNC), and the virtual display must be a
real X11 RandR output manageable via arandr/xrandr (X11/Xorg only, not
Wayland).

## Open Decisions (resolve before/during Phase 0)

- [x] Host daemon implementation language: **Rust** — see
      [docs/adr/0001-host-daemon-language.md](docs/adr/0001-host-daemon-language.md).
- [x] Android client language: **Kotlin** — see
      [docs/adr/0002-android-client-language.md](docs/adr/0002-android-client-language.md).
- [x] Video codec baseline: **H.264**, upgrade path is **AV1** (not H.265,
      licensing) — see
      [docs/adr/0003-video-codec-baseline.md](docs/adr/0003-video-codec-baseline.md).
- [x] Target scope for v1: **this machine only** — Ubuntu 24.04 LTS, GNOME
      on X11, Intel Raptor Lake iGPU (VAAPI) — see
      [docs/adr/0004-target-scope-v1.md](docs/adr/0004-target-scope-v1.md).

## Phase 0 — Feasibility Spikes ✅ Complete (2026-08-18)

De-risked the two biggest unknowns from `CLAUDE.md` before committing to
full implementation. All 9 items resolved empirically against the real
target hardware (this machine + HONOR Pad X8a) — see individual items below
and [docs/adr/0004-target-scope-v1.md](docs/adr/0004-target-scope-v1.md)
for full detail. Headline results: AOA transport works end-to-end with
negligible latency (<1ms typical) and comfortable throughput headroom
(20.7 MB/s vs. ~25 Mbps needed); the dummy-output/RandR approach works with
some refinements; AV1 is ruled out on this pairing entirely, H.264 is
solidly validated on both ends. The minimal Android spike app lives at
`spikes/aoa-hello-android/`.

- [x] Confirm target tablet (HONOR Pad X8a) supports USB accessory (AOA)
      mode in practice: **confirmed empirically** — `GET_PROTOCOL` returns
      version 2, and a full `SEND_STRING`+`START_ACCESSORY` handshake
      re-enumerates the device as `18d1:2d00` with a clean, standard
      2-bulk-endpoint accessory interface. No developer options or USB
      debugging needed on the tablet. See
      [docs/adr/0004-target-scope-v1.md](docs/adr/0004-target-scope-v1.md#confirmed-aoa-works-on-this-tablet-empirical-2026-08-18).
- [x] Spike: raw AOA "hello world" round trip: **confirmed working
      end-to-end.** Built a minimal Kotlin app
      (`spikes/aoa-hello-android/`) that opens the accessory and echoes
      small payloads. Real finding: the accessory-attached intent-filter
      auto-launch **did not bring the app to the foreground** on MagicOS
      (no evidence in any logcat buffer, despite the permission being
      silently granted at the system level) — worked around it with an
      explicit `accessoryList`/`requestPermission` fallback path, which
      Phase 1's real app should include defensively rather than relying on
      auto-launch alone. Also: logcat visibility for this app's own tag was
      unreliable on this device — don't design Phase 7's field-debugging
      logging around logcat alone; prefer on-device UI/file-based logging
      as a fallback.
- [x] Measure actual sustained AOA bulk-transfer throughput on target
      hardware: **measured 20.70 MB/s (165.6 Mbps)**, one-way host→tablet,
      16KB chunks. Lower than the naive USB 2.0 ceiling estimate
      (30-35 MB/s) but not a practical constraint — a 1200×1920 H.264
      stream at good quality needs well under 25 Mbps, leaving roughly
      6-10x headroom. The throughput risk flagged throughout Phase 0 turns
      out not to bind. See
      [docs/adr/0004-target-scope-v1.md](docs/adr/0004-target-scope-v1.md).
- [x] Measure one-way/round-trip latency separately from throughput:
      **measured 0.21-3.17ms round-trip** (avg 0.70ms, 20 samples,
      32-byte echo). Negligible — end-to-end latency will be dominated by
      video encode/decode, not the USB/AOA transport.
- [x] Check what Rust crate ecosystem exists for the AOA handshake
      specifically: **confirmed there is none.** crates.io search turned up
      only generic USB host crates (`rusb`, `usb-host`, `crab-usb`) and
      unrelated Android protocol crates (`android-auto`, `adb_client`,
      `android-usbser`) — no dedicated AOA implementation. The handshake
      must be hand-rolled on top of `rusb`'s raw control-transfer API,
      following the exact sequence validated by the Python/`pyusb` probe
      above (`GET_PROTOCOL` → `SEND_STRING` ×6 → `START_ACCESSORY`).
- [x] Verify AV1 hardware decode support on the Snapdragon 680:
      **confirmed absent.** Read the tablet's actual vendor codec XML via
      `adb` — only `c2.android.av1.decoder` (`variant="slow-cpu"`, Android's
      generic software fallback) exists; no `c2.qti.*` hardware AV1 decoder.
      Combined with the confirmed VAAPI AV1 encode gap on the host, **AV1 is
      not viable on this pairing at all.** H.264/H.265 both confirmed as
      genuine Qualcomm hardware codecs, including a
      `c2.qti.avc.decoder.low_latency` variant. See
      [docs/adr/0004-target-scope-v1.md](docs/adr/0004-target-scope-v1.md#confirmed-tablet-codec-hardware-support-empirical-2026-08-18).
      Also captured while ADB was available: actual panel is **1200×1920,
      up to 90Hz** (running at 60Hz by default), and the tablet is now on
      **Android 16 (SDK 36)**, having been updated since its Android
      14/MagicOS 8 launch spec — relevant to Phase 5's mode negotiation.
- [x] Spike: `xf86-video-dummy` output declared in Xorg config, confirm it
      shows up in `xrandr --query` and can be toggled/repositioned at
      runtime: **confirmed, with a refinement.** Tested via an isolated
      second Xorg instance (`Xorg :2`, background VT, `sudo` required —
      `Xorg.wrap`'s console-user check rejects even a real terminal's PTY,
      only root or a genuine VT-attached session passes). Findings:
      - A single `Device`/`Screen` config section produces **16 outputs**
        (`DUMMY0`-`DUMMY15`) for free, not just one — good news for
        multi-tablet headroom (Phase 7's open question).
      - Only `DUMMY0` is active by default; `DUMMY1`-`15` start
        `disconnected`, matching the "off until a tablet connects" workflow
        already planned.
      - The config file's `Modes "1920x1200" ...` list was **not**
        respected — `DUMMY0` showed its own built-in mode list instead.
        Confirms Phase 5's runtime `--newmode`/`--addmode` approach is
        necessary, not optional.
      - `xrandr --output DUMMY1 --auto --right-of DUMMY0` did **not**
        activate `DUMMY1` (stayed disconnected) — `--auto` has nothing to
        select without a mode already registered via `--newmode`/
        `--addmode` first. This means every non-`DUMMY0` output needs an
        explicit mode registered before first use, not just when matching
        the tablet's exact resolution.
      - Max resolution ceiling is 32767×32767 — no concern for fitting any
        real tablet panel size.
      - arandr itself wasn't opened against the test server (no WM was
        running on `:2`) — not re-verified, but arandr is a thin GUI over
        the same RandR calls tested here, so this is treated as sufficient.
- [x] Spike: confirm a single RandR output/CRTC's framebuffer can be
      captured in isolation (not just the whole X screen cropped):
      **confirmed** — `ffmpeg -f x11grab -video_size WxH -i "$DISPLAY+X,Y"`
      against the real `eDP-1` output's exact geometry (2560x1600+0+0)
      produced a correctly-sized, correctly-content frame, not the full
      6400x2160 virtual screen. Test file deleted after verification (it
      contained live screen content).
- [x] Confirm VAAPI enumerates the Intel iGPU (not the Nvidia dGPU) by
      default on this hybrid-GPU laptop: **confirmed** — `/dev/dri/renderD128`
      (the default/lowest-numbered node) maps to the Intel PCI device,
      `renderD129` to Nvidia. Real `ffmpeg`+VAAPI hardware encode against
      `renderD128` confirmed working for **H.264** (16.4x realtime) and
      **H.265** (10.7x realtime), but **AV1 encode fails**
      (`No usable encoding entrypoint found for profile VAProfileAV1Profile0`)
      — this corrects a wrong claim in both
      [docs/adr/0003-video-codec-baseline.md](docs/adr/0003-video-codec-baseline.md)
      and
      [docs/adr/0004-target-scope-v1.md](docs/adr/0004-target-scope-v1.md#confirmed-vaapi-hardware-encode-on-the-intel-igpu-empirical-2026-08-18)
      that Raptor Lake has AV1 hardware encode — it doesn't; that arrived
      later with Meteor Lake/Arc.

## Phase 1 — USB Transport (AOA) ✅ Complete (2026-08-19)

Validated end-to-end against the real host + tablet: real Rust daemon
(`host/`) and real Kotlin client (`android/`) completing a full AOA
handshake, `CONTROL` `HELLO`/`HELLO_ACK` exchange, and — critically —
automatic recovery from both a software-only disconnect and a genuine
physical unplug/replug, with no manual intervention beyond the tablet's own
unlock/USB-mode prompts.

- [x] Define wire protocol: length-prefixed frames, channel IDs for
      video / input / control messages. See
      [docs/wire-protocol.md](docs/wire-protocol.md); implemented
      identically (with matching unit tests) in `host/src/protocol.rs` and
      `android/app/.../protocol/FrameProtocol.kt`.
- [x] Host: implement AOA host-side handshake (accessory mode negotiation)
      and bulk endpoint read/write. `host/src/aoa.rs`. Real finding: Rust
      issues the GET_PROTOCOL/SEND_STRING×6/START_ACCESSORY control
      transfers essentially back-to-back, far faster than the Python probe
      (which had interpreter overhead between calls) — without a ~100ms
      delay between each control transfer and after the final one, the
      device reliably fails to re-enumerate. This isn't documented anywhere
      in the AOA spec; it was found empirically. The delays are now baked
      into `start_accessory_mode`.
- [x] Android: implement `UsbAccessory` registration (accessory filter,
      intent-filter, permission handling) and bulk endpoint read/write.
      `android/app/.../MainActivity.kt`. Real finding, more significant
      than the Phase 0 note anticipated: **`ACTION_USB_ACCESSORY_ATTACHED`
      does not reliably reach even a dynamically-registered runtime
      receiver on an already-running app instance on this device/OS** —
      only a fresh Activity launch's `accessoryList` check picked it up
      reliably. The working fix is active polling of `accessoryList` (see
      `startPolling()`) as the primary reconnect mechanism, with the
      broadcast receiver kept only as a possibly-faster opportunistic path
      on devices where it does work.
- [x] Connect/disconnect detection on both ends; clean teardown. Host
      detects physical disconnect via a `read_bulk` `NoDevice` error and
      loops back to reconnect automatically (`main.rs`'s retry loop).
      Android detects it via the `ACTION_USB_ACCESSORY_DETACHED` broadcast
      (confirmed reliable, unlike ATTACHED) plus an `IOException`/EOF on
      the read stream as a backup signal; both trigger `closeConnection()`
      + `startPolling()`. AOA re-enumeration (device disappears and
      reappears as `18d1:2d00`/`2d01`) is handled as a first-class state
      transition via `find_existing_accessory_device()`, not an edge case —
      a reconnect after a *non-physical* disconnect (e.g. the host process
      restarting) correctly skips redoing the handshake since the device is
      already in accessory mode.
- [x] Basic reconnect logic (tablet unplugged/replugged mid-session):
      validated live, twice — once via killing/restarting the host process
      with no physical replug (skips handshake, reconnects instantly), and
      once via a genuine physical unplug/replug while the daemon was
      running continuously (full handshake redone automatically, `HELLO`/
      `HELLO_ACK` completed with no manual steps on either side).

### Other real findings worth carrying into later phases

- **Repeated rapid failed handshake attempts can leave the device in a
  confusing state** where several consecutive retries fail even though the
  code is correct — recovery sometimes needed a genuine physical replug to
  clear, not just more software retries. The 2s backoff between retry
  cycles already in `main.rs` seems sufficient in practice, but this is
  worth watching if flakiness resurfaces.
- **MagicOS requires unlocking the device (PIN/password) on most
  reconnects**, plus its normal "how do you want to connect" (Photos/Files/
  Charge-only) prompt, before the accessory permission flow can proceed.
  This is a real, unavoidable OS-level security gate that works against
  the "feels like a real peripheral" goal from `CLAUDE.md` — not fixable by
  tabscreen, but belongs in Phase 8's setup guide as an expected step, not
  a bug.
- **Multiple installed apps matching the same `accessory_filter.xml`
  identification strings trigger an app-chooser dialog.** Only one app
  should ever be installed with tabscreen's identification strings at a
  time; the Phase 0 spike app (`ai.tabscreen.aoahello`) was uninstalled for
  exactly this reason once `android/`'s real client existed.
- The Android client's reconnect story now depends on the Activity staying
  alive (for its poll thread) or the OS relaunching it — neither is
  guaranteed if MagicOS kills a backgrounded/minimized app. A persistent
  foreground `Service` doing the polling/connection work, rather than the
  `Activity`, would be more robust — worth considering before Phase 6
  packaging, not required for Phase 1's scope.

## Phase 2 — Virtual Display (X11 RandR)

**Incident (2026-08-18):** an early attempt applied a bare dummy-driver Xorg
snippet directly to the live GDM/GNOME session (not the isolated `Xorg :2`
spike instance) and froze the desktop hard enough to require a blind
`sudo rm` of the file and `sudo shutdown now` to recover. See
[docs/adr/0005-live-session-dummy-output-hazard.md](docs/adr/0005-live-session-dummy-output-hazard.md).

**Blocker (2026-08-19):** four systematic follow-up tests against an
isolated `Xorg :2` secondary instance (never the live session) all failed to
get `xf86-video-dummy` to coexist with a real GPU in one Xorg config —
including the corrected RandR 1.4 `Inactive`-GPU pattern that 0005 proposed.
The dummy screen is dropped (`Screen 1 deleted because of no matching config
section`) regardless of which real GPU it's paired with or which
`ServerLayout` shape is used; this now looks like a real
driver/Xorg-version compatibility gap (Ubuntu 24.04's `xserver-xorg-core`
1.21.1.11), not a config mistake. Separately: both of this machine's real
GPUs (Intel *and* NVIDIA) are already claimed for modesetting authority by
the live GDM session, so there is no real GPU left that's safe to test an
active competing `Screen` against from a second process — the
`Xorg :2`-on-a-spare-VT isolation technique used since Phase 0 can no longer
validate the actual merge target on this hardware. Full details, all four
test logs' findings, and next-step options in
[docs/adr/0006-dummy-driver-multi-gpu-binding-failure.md](docs/adr/0006-dummy-driver-multi-gpu-binding-failure.md).
**Phase 2 is blocked on resolving this before any further live-hardware
Xorg testing or installer work.**

- [ ] Research whether a newer/patched `xf86-video-dummy` build, or an
      alternative virtual-display driver (e.g. `evdi`, a purpose-built
      virtual KMS driver), supports binding as a secondary/offload GPU
      screen — ADR 0006's finding 1.
- [ ] Decide on a validation strategy that doesn't require contending with
      this machine's already-claimed real GPUs (e.g. a VM, or a different
      test machine) — ADR 0006's finding 2.
- [ ] Once a working driver/approach is found: re-validate the combined
      config shape before it ever touches `/etc/X11/xorg.conf.d/` on the
      live session.
- [ ] Write installer step that adds the validated `xf86-video-dummy` Xorg
      config snippet — confirmed one `Device` section yields 16 outputs
      (`DUMMY0`-`DUMMY15`) for free when used standalone, so no need to
      hand-declare more than one (see Phase 0 findings). Installer should
      derive the real GPU's `BusID` from `lspci` at install time rather than
      hardcoding it.
- [ ] Host daemon: on tablet connect, run `xrandr --newmode <name> <modeline>`
      + `--addmode DUMMYn <name>` **before** `--mode`/`--auto` — confirmed
      `--auto` alone does not activate a non-`DUMMY0` output, since it has
      no mode to select without one being registered first. Then
      `--output DUMMYn --mode <name> --pos ...`; on disconnect, `--off`.
- [ ] Verify in arandr: output appears, can be dragged/positioned/rotated,
      and disappears cleanly on tablet disconnect.
- [ ] Handle the "no free dummy output available" case (e.g. second tablet,
      or dummy outputs not pre-provisioned) with a clear error.

## Phase 3 — Video Pipeline

- [ ] Host: capture the dummy output's framebuffer (X11 capture of that
      specific RandR output/CRTC, not the whole desktop).
- [ ] Host: hardware-accelerated H.264 encode of the captured frames via
      VAAPI on the Intel Quick Sync iGPU.
- [ ] Host: send encoded frames over the Phase 1 transport's video channel.
- [ ] Android: decode via `MediaCodec` (hardware decoder — confirmed
      `c2.qti.avc.decoder.low_latency` exists on the target tablet, prefer
      it explicitly over the default variant), render fullscreen
      (`SurfaceView`/`TextureView`).
- [ ] Static-image end-to-end test: single frame captured → encoded → sent
      → decoded → displayed.
- [ ] Continuous streaming: measure latency and achieved frame rate against
      the Phase 0 AOA throughput ceiling; tune encode bitrate/resolution.

## Phase 4 — Input Injection

Not blocked by Phase 2 — built and unit-tested (2026-08-19), but **not yet
validated against the real tablet** (unlike Phase 1, which was validated
live) since that requires a working end-to-end session. Re-verify against
real hardware once Phase 2/3 exist, or by wiring up a throwaway test client.

- [x] Define input event schema: `TOUCH` message on the `INPUT` channel
      (not `CONTROL` — that channel was already reserved for this in
      `docs/wire-protocol.md`'s Phase 1 channel table). `action` (down/move/
      up/cancel), `pointer_id` (multi-touch-ready, v1 client only sends 0),
      normalized `x`/`y` (0.0-1.0, not raw pixels — decouples the client
      from the host's screen resolution and survives arandr repositioning
      with no protocol change), and `pressure` (0.0-1.0, stylus-ready). See
      `docs/wire-protocol.md`'s new INPUT section.
- [x] Android: `MainActivity` captures touch via a whole-window
      `setOnTouchListener` (no `SurfaceView` yet — Phase 3), normalizes by
      window size, sends `TOUCH` frames over a newly-synchronized shared
      output stream (previously accessed only from the reader thread).
- [x] Host: `host/src/input.rs`'s `Injector` connects to the X server via
      `x11rb`'s XTEST extension (`XTestFakeInput`: `MotionNotify` for
      move, `ButtonPress`/`ButtonRelease` for down/up, button 1) and maps
      normalized coordinates onto the **current default screen** — there's
      no dummy output to target yet (Phase 2 blocked), so this is a
      placeholder target; the design note in `docs/wire-protocol.md`
      explains why swapping in the dummy output's rect later needs no wire
      change, just a different (width, height) source in `Injector`.
- [x] Verify the host-side XTEST injection actually moves the real pointer
      (independent of the tablet/USB path): a throwaway `#[ignore]`d test
      called `Injector::inject` directly and confirmed via `xdotool
      getmouselocation` that the cursor moved to the exact expected pixel
      (screen center, `1280,800` on this `2560x1600` display) — deleted
      after confirming, not part of the shipped code.
- [ ] Verify the *full* path end-to-end: touches from the real tablet,
      over USB, actually move/click the host's pointer. Needs the tablet
      connected and a live session (not required for the above, which only
      exercised the host's X11 half).
- [ ] Verify coordinate mapping is correct across output position/rotation
      changes made via arandr — blocked on Phase 2 (no dummy output to
      reposition yet).

## Phase 5 — Mode Negotiation

- [ ] Android: report tablet panel resolution + refresh rate during the
      Phase 1 handshake. Confirmed target values for the HONOR Pad X8a:
      1200×1920 native, supports 60Hz (default) and 90Hz — decide whether
      to request 90Hz explicitly given the USB 2.0/AOA throughput ceiling
      from Phase 0, or stay at 60Hz to leave more bandwidth for video
      quality. Don't assume 90Hz is free.
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
      per `CLAUDE.md`, since network tools don't apply). On the Android
      side, don't rely on `logcat` alone — confirmed unreliable for this
      app on the target tablet's MagicOS build during Phase 0; prefer
      on-device UI status and/or file-based logging as the primary
      mechanism, with `logcat` as a bonus when it happens to work.
- [ ] Multiple-tablet support decision: in scope for v1 or explicitly
      deferred?
- [ ] Battery/thermal impact check on the tablet during extended use.

## Phase 8 — Documentation & Release

- [ ] User-facing setup guide (install steps, enabling accessory mode,
      first-connect flow).
- [ ] Update `CLAUDE.md` with any architecture decisions finalized during
      implementation that diverge from the current plan.
- [ ] Versioned release/tag of host daemon + Android client.
