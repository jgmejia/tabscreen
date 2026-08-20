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

## Phase 2 — Virtual Display (X11 RandR) — Unblocked: `vkms` works cleanly (2026-08-19)

**Breakthrough (2026-08-19):** `vkms` (in-tree, no build/compatibility
risk, loaded via `/etc/modules-load.d/vkms.conf` at boot) created a real,
connected, correctly-positioned RandR output (`Virtual-2-1`) on the first
attempt, with **neither real monitor disturbed and no wrong-primary bug**
— the exact two failure modes that consumed the entire `ConnectedMonitor`
investigation below. Full writeup in
[docs/adr/0021-vkms-works-cleanly-first-try.md](docs/adr/0021-vkms-works-cleanly-first-try.md).
This works because `vkms` never touches Intel's or NVIDIA's own detection
state — it's a genuinely separate virtual GPU, unlike `ConnectedMonitor`'s
override-on-a-real-GPU approach. **Phase 2 is unblocked.** Remaining work:
add a custom mode matching the tablet's real panel via `xrandr --newmode`/
`--addmode`, confirm on/off toggling, verify greeter behavior and arandr
positioning, then point Phase 3/4's placeholder targets at this real
output. See updated checklist below.

**Everything below is preserved history of how the (much harder than
expected) path to this was found — not further action items unless
`vkms` turns out to have a blocking limitation.**

**Conclusive finding (2026-08-19):** `ConnectedMonitor "DFP-1"` doesn't
just cause an unreliable toggle or a greeter-selection problem — it
**breaks real hardware detection for the actual external monitor**,
confirmed across all 9 boots tonight with a causal proof, not a
correlation: the "recovery" moment in every boot happened only *after*
the config file had already been removed and `gdm` restarted, never from
the same Xorg session healing itself. Full writeup in
[docs/adr/0019-connectedmonitor-breaks-real-monitor-detection.md](docs/adr/0019-connectedmonitor-breaks-real-monitor-detection.md).
**This rules out the entire `ConnectedMonitor`/`CustomEDID` approach
(ADRs 0010–0018), not just this session's specific tests** — both
"dynamic" (option 1) and "always-on" (option 2) would mean the real
external monitor stops working for as long as the config is active, which
is a hard non-starter, not a UX tradeoff. Real remaining paths:
reconsider `evdi`/`vkms` (deprioritized in 0006/0008, but their failure
mode is different from what just ruled out `ConnectedMonitor`); move
further experimentation to a VM or second machine (ADR 0006's
long-deferred suggestion, now more clearly warranted given tonight's
impact on the primary daily-use machine); or check whether a USB-C
dock/adapter could give Intel a genuinely idle connector (0009 found none
built into this laptop). Confirmed fully recovered: both monitors active,
greeter correctly on `eDP-1`, after removing the config.

**Deeper look at the USB-C dock option (2026-08-19):** checked this
machine's actual USB topology rather than relying on the spec sheet alone.
Both Keychron peripherals (physically in USB-C-shaped ports right now)
enumerate via the ordinary 480Mbps USB 2.0 hub, not the dedicated 20Gbps
USB-C/SuperSpeed controller, which currently shows zero connected devices;
`/sys/class/typec/` is empty even with the relevant kernel modules loaded.
Lenovo's spec sheet does list USB-C 3.2 Gen 2 with DisplayPort 1.4, and
the port is physically marked USB-PD, so this isn't fully ruled out — but
priority is lowered pending further investigation; not pursuing purchases
based on this until revisited.

**Researched `evdi` properly (2026-08-19) — promising, and architecturally
avoids 0019's failure mode.** Full writeup in
[docs/adr/0020-evdi-research-promising-candidate.md](docs/adr/0020-evdi-research-promising-candidate.md).
Key points: `evdi` is a genuine separate DRM device driven by Xorg's
generic `modesetting` driver — never touches NVIDIA's driver, so it can't
reproduce 0019's "breaks real monitor detection" bug. Confirmed no real
DisplayLink dock or proprietary daemon needed. Its own documented
`modprobe.d` config includes a `softdep` specifically to stop the
compositor from selecting it as primary — the exact bug class that ate
hours of tonight's NVIDIA testing is something `evdi`'s maintainers
already solved. The previously-flagged freeze risk (loading the module
into an already-running Xorg) is avoidable by loading it at boot via
`/etc/modules` with `initial_device_count` set, so Xorg sees the virtual
device from its very first startup rather than a live dynamic add. All
build prerequisites (kernel headers, `dkms`, `evdi-dkms` package) already
present/available on this machine. `vkms` ruled out separately — testing
tool only, doesn't reliably expose a real capturable framebuffer. **Not
yet attempted live** — see ADR 0020's proposed plan.

**Status as of 2026-08-19 (corrected after an initial overclaim — see ADR
0016):** the blind-login test (proposed after ADR 0015's greeter/session
reframing) confirmed via Xorg's own log that a real logged-in session
correctly keeps the virtual tablet output disconnected at the driver level
while both real monitors configure normally. **That is not the same as
confirming the desktop was actually usable.** `gnome-shell`'s own log
(checked after an initial, premature "resolved" claim was made and then
retracted) shows rendering warnings followed by a 6-minute-40-second gap
before the desktop became interactive — consistent with the user's direct
report of "no luck," not with a clean success. Full writeup, including the
correction, in
[docs/adr/0016-blind-login-driver-ok-desktop-usability-unconfirmed.md](docs/adr/0016-blind-login-driver-ok-desktop-usability-unconfirmed.md).
**Phase 2 remains blocked.** Next step: retest with the user directly
observing and reporting in real time whether the desktop becomes usable,
rather than reconstructing the answer from logs afterward — see ADR 0016's
Decision.

**Status as of 2026-08-19 (corrected after an initial overclaim — see ADR
0016):** the blind-login test (proposed after ADR 0015's greeter/session
reframing) confirmed via Xorg's own log that a real logged-in session
correctly keeps the virtual tablet output disconnected at the driver level
while both real monitors configure normally. **That is not the same as
confirming the desktop was actually usable.** `gnome-shell`'s own log
(checked after an initial, premature "resolved" claim was made and then
retracted) shows rendering warnings followed by a 6-minute-40-second gap
before the desktop became interactive — consistent with the user's direct
report of "no luck," not with a clean success. Full writeup, including the
correction, in
[docs/adr/0016-blind-login-driver-ok-desktop-usability-unconfirmed.md](docs/adr/0016-blind-login-driver-ok-desktop-usability-unconfirmed.md).
**Phase 2 remains blocked.** Next step: retest with the user directly
observing and reporting in real time whether the desktop becomes usable,
rather than reconstructing the answer from logs afterward — see ADR 0016's
Decision.

**Retested (2026-08-19), user watched directly.** Initially misread as
"non-deterministic within one boot" by comparing two logins — corrected
after finding the second login happened *after* the user had already
removed the Xorg config via their fallback (VT-switch, `sudo rm`,
`systemctl restart gdm`) — that comparison was invalid, not a real
same-config discrepancy. Full correction in
[docs/adr/0017-nondeterminism-claim-was-analysis-error.md](docs/adr/0017-nondeterminism-claim-was-analysis-error.md).
**The corrected, honest picture:** across all boots today, exactly two
genuine trials had the config actually active — 0016's session (`DFP-1`
correctly disconnected, but a 6m40s stall before the desktop responded)
and today's session 1 (`DFP-1` wrongly connected — the same broken pattern
as every greeter failure — but fast rendering). One for two, two different
failure modes, neither clean.

**Retested once more (2026-08-19) with a tightened protocol — config
presence confirmed via the Xorg log immediately, user watched in real
time.** Result: user watched a genuinely blank, unresponsive screen for
2.5 minutes; the log (checked right after, config confirmed active)
showed the same broken re-activation pattern as every greeter failure.
This is a third genuine, protocol-verified trial, and it's broken. Full
conclusion in
[docs/adr/0018-monitorsxml-disabled-mechanism-unreliable.md](docs/adr/0018-monitorsxml-disabled-mechanism-unreliable.md).
**Conclusion: `monitors.xml`'s `<disabled>` mechanism is unreliable (2 of
3 genuine trials broken, and the 1 "correct" trial never confirmed
usable), not a race condition worth debugging further.** This is roughly
the ninth live-hardware attempt this session. **Phase 2 remains blocked.**
Three real options going forward (ADR 0018's Decision): make
`ConnectedMonitor` genuinely dynamic (applied only at actual tablet-connect
time, not pre-declared at boot); accept a scope reduction (virtual output
active from boot, not toggled); or move further experimentation to a
different validation environment (ADR 0006's still-open suggestion).
Recommend deciding among these with the user before further live attempts
on this machine.

**History below, preserved for context on how this was reached:**

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

**Attempted and failed empirically (2026-08-19):** researched `evdi`/`vkms`
as ADR 0006 suggested, but both share a live-session-freeze risk when their
kernel module loads while Xorg is already running (the same failure class as
0005's incident) — real-world reports confirm this for `evdi`. Instead found
and tried a third approach that avoids the multi-driver/multi-screen problem
entirely: **EDID firmware override on an already-idle connector**
(`drm.edid_firmware=DP-1-0:<edid-file>` kernel boot parameter), rather than
adding a second driver/screen at all — full plan in
[docs/adr/0008-edid-override-virtual-output.md](docs/adr/0008-edid-override-virtual-output.md).
Generated and `edid-decode`-validated an EDID for the tablet, installed the
GRUB parameter (with a proper backup/rollback plan), rebooted: **no session
damage, but `DP-1-0` never came up connected.** Root cause (full diagnosis in
[docs/adr/0009-edid-firmware-wrong-gpu-driver.md](docs/adr/0009-edid-firmware-wrong-gpu-driver.md)):
`DP-1-0` is Xorg's RandR name, not a real kernel connector name; the idle
connectors (`DP-1`/`DP-2`) actually sit on the **NVIDIA** GPU (0008 had the
GPU vendor backwards), and NVIDIA's proprietary driver doesn't implement the
generic kernel `drm.edid_firmware=` mechanism at all — while **Intel** (the
GPU that *does* support it) has zero idle connectors on this machine, only
the already-in-use laptop panel. Change fully reverted (`/etc/default/grub`
restored from backup, confirmed identical via `diff`, `update-grub` re-run).
**Phase 2 is blocked again.**

**Researched (2026-08-19), not yet attempted:** NVIDIA's own native
`Option "ConnectedMonitor"`/`Option "CustomEDID"` Xorg `Device` options —
pulled exact semantics from the NVIDIA driver README shipped locally on
this machine, plus a real-world Ubuntu/Xorg/NVIDIA/GDM precedent that kept
its real monitor working while adding a virtual one the same way. Checked
this machine's actual `journalctl` Xorg log for real candidate connectors on
the NVIDIA GPU: of six enumerated (`DFP-0`..`DFP-5`), two are TMDS-class
(165 MHz max pixel clock — too low for our 196.5/302.75 MHz modes), one
(`DFP-4`) is the real connected monitor, leaving `DFP-1`/`DFP-3`/`DFP-5`
(DisplayPort-class, 2670 MHz max) as real candidates. Full writeup, exact
proposed config fragment, and open questions (fragment scoping without an
existing base Xorg config, restart-vs-reboot) in
[docs/adr/0010-nvidia-customedid-connectedmonitor.md](docs/adr/0010-nvidia-customedid-connectedmonitor.md).
**Attempted and failed (2026-08-19):** applied 0010's exact proposed
fragment (bare `Device` section, `ConnectedMonitor "DFP-1"` +
`CustomEDID "DFP-1:..."`, no `Screen`/`ServerLayout`) via a full reboot.
Two problems, diagnosed after the fact from `journalctl` (full detail in
[docs/adr/0011-customedid-bare-fragment-failed.md](docs/adr/0011-customedid-bare-fragment-failed.md)):
**(1) the option had no effect** — `DFP-1` logged `disconnected` throughout,
in both Xorg instances that boot; **(2) Xorg crashed at the exact moment of
login** (`Insufficient permissions` setting a mode on all four heads, plus
Intel's separate `modeset(0)` driver failing too, at the same instant — a
pattern more consistent with a DRM-master handoff race than something
`DFP-1`-specific, though correlation with the new fragment can't be ruled
out either). GDM self-healed automatically within ~1 second (unlike 0005,
no forced shutdown needed), but the user reasonably recovered manually via
VT-switch before observing that. Session confirmed fully healthy afterward.
**Do not retry with `DFP-3`/`DFP-5` using the same bare-fragment shape** —
since the option didn't bind at all, a different connector name would very
plausibly fail identically. Next candidate: match the actually-confirmed-
working precedent's shape more closely (full `Device` + `Screen` with
explicit `metamodes`, not a bare fragment) — not yet attempted. **Phase 2
remains blocked**, and given this is the third failed live-hardware attempt
across three mechanisms (one causing a real if self-healing crash), the
next attempt should not proceed without deliberately reconsidering the
approach first, not just iterating connector names — see ADR 0011's
Decision.

**Attempted (2026-08-19): the mechanism itself now works.** Applied a full
`Device` + `Screen` config (`ConnectedMonitor`/`CustomEDID` plus an explicit
`MetaModes` string preserving the real monitor's exact current mode and
adding the virtual tablet output). Result, diagnosed from `journalctl` in
[docs/adr/0012-full-config-mechanism-works-greeter-primary-issue.md](docs/adr/0012-full-config-mechanism-works-greeter-primary-issue.md):
**the driver correctly showed "TBS TabScreen (DFP-1): connected" and set
the mode to 1200×1920** — the EDID/`ConnectedMonitor`/`CustomEDID`/
`MetaModes` recipe is confirmed correct, the first attempt (of four) where
the underlying mechanism worked. The user still saw "gdm failed to start"
(~74s, recovered via VT-switch, no crash found in logs, session fully
healthy afterward) — best explanation is the login *greeter* rendered
itself onto the new invisible virtual output rather than the real monitor,
a display-selection/UX problem, not a mechanism failure. Also a reframe:
this test forced the virtual output permanently live in boot-time
`MetaModes`, which is a harsher test than Phase 2 actually needs (the real
design is "off until tablet connects," toggled at runtime) — the proposed
next test keeps this same config but turns the output off immediately
after Xorg starts, before any greeter render, which is both closer to
production behavior and likely sidesteps the greeter problem entirely.

**Attempted (2026-08-19), and found a deeper problem:** tried the
NULL-default `MetaModes` combo from the item above
(`DFP-1: NULL` as the first/default combo). Failed differently and more
informatively — full diagnosis in
[docs/adr/0013-connectedmonitor-desktop-autoactivation-conflict.md](docs/adr/0013-connectedmonitor-desktop-autoactivation-conflict.md):
**the driver rejected the NULL combo outright** (`"...DFP-1:NULL"; removing`),
so `DFP-1` was live from boot regardless; and **something above Xorg
(most plausibly `mutter`'s greeter auto-layout) actively re-activated and
repositioned the output** even when the config tried to keep it off. This
points to an architectural conflict, not a syntax bug:
`ConnectedMonitor` tells the *entire desktop stack*, not just the driver,
"this is really plugged in" — and a real connected monitor being
auto-activated is correct desktop behavior, which is fundamentally in
tension with "off until the tablet connects." Every attempt so far that
gets the driver to report the port connected also gets it auto-activated
by something above the driver, regardless of what the static config says.
**Do not keep iterating `MetaModes` syntax against this** — see ADR 0013's
Decision for the real candidate fixes (seeding a `monitors.xml` that
excludes the output; making `ConnectedMonitor` genuinely dynamic —
applied only at actual tablet-connect time rather than pre-declared at
boot; or accepting a scope reduction where the output is active from boot
rather than toggled). This is the fifth live-hardware attempt this
session (second producing a failed/blank greeter) — reconsider strategy
with the user before a sixth.

**Researched (2026-08-19), not yet attempted:** read mutter's own
`meta-monitor-config-store.c` source directly to check whether
`monitors.xml` can express "known monitor, keep it off." Confirmed:
**it has a purpose-built `<disabled>` section with `<monitorspec>`
entries** (`connector`/`vendor`/`product`/`serial`) — exactly this
semantic, not something to hack around. Full writeup and two open
unknowns (does GDM's greeter even read a per-session `monitors.xml`;
does config-matching need an exact/complete match against every
connected monitor) in
[docs/adr/0014-monitorsxml-disabled-mechanism-research.md](docs/adr/0014-monitorsxml-disabled-mechanism-research.md).
Plausibly much lower-risk than prior attempts (a plain config file, not
boot-critical Xorg config; mutter is documented to fall back gracefully
rather than crash on a bad config) but not yet proven. Not yet attempted
live.

**Attempted (2026-08-19): `monitors.xml` didn't stop it either — and a
strategic reframing.** Seeded `monitors.xml` at both plausible locations
with exact real monitor identities (queried live via
`org.gnome.Mutter.DisplayConfig.GetCurrentState`, removing prior guesswork)
plus a `<disabled>` entry for the virtual output. Same failure signature as
every prior attempt — full diagnosis in
[docs/adr/0015-connectedmonitor-desktop-autoactivation-conflict-2.md](docs/adr/0015-connectedmonitor-desktop-autoactivation-conflict-2.md).
**Key reframing: every failure so far has been at the GDM greeter stage,
which is not what Phase 2 actually needs** — the host daemon that toggles
the virtual output only ever runs during an already-logged-in session,
never the greeter. GDM's greeter plausibly has its own independent
monitor policy that doesn't respect `monitors.xml` the way a real session
does (reasonably, for security reasons). This is directly testable: prior
diagnosis (ADR 0011) showed login can succeed blind (typing
username/password without seeing the greeter) even when the screen looks
broken. **Not yet attempted:** logging in blind through the blank greeter
to check whether the real session (not the greeter) correctly keeps
`DFP-1` off by default. This is the sixth live-hardware attempt this
session — paused here to confirm direction with the user before a seventh.

**Attempted (2026-08-19), mixed result — corrected after an initial
overclaim:** logged in blind through the broken greeter. The user reported
"no luck" and fell back. Xorg's own log shows `DFP-1: disconnected` held
consistently for ~13 minutes with both real monitors configured normally —
but checking `gnome-shell`'s own log (done only after an initial premature
"it worked" claim was made and then retracted) shows rendering warnings
followed by a 6-minute-40-second gap before the desktop became interactive,
consistent with the user's report, not a clean success. Full account
including the correction in
[docs/adr/0016-blind-login-driver-ok-desktop-usability-unconfirmed.md](docs/adr/0016-blind-login-driver-ok-desktop-usability-unconfirmed.md).
**Driver-level config confirmed correct; desktop usability NOT confirmed —
Phase 2 remains blocked.** Next step: retest with the user directly
watching and reporting in real time. The greeter itself is separately
cosmetically broken with this config active — out of Phase 2's critical
path regardless, tracked as Phase 7/8 polish.

- [x] Research whether a newer/patched `xf86-video-dummy` build, or an
      alternative virtual-display driver (e.g. `evdi`, a purpose-built
      virtual KMS driver), supports binding as a secondary/offload GPU
      screen — ADR 0006's finding 1. Resolved by adopting the EDID-override
      approach instead (ADR 0008), which sidesteps the binding problem
      rather than solving it for `dummy`/`evdi`/`vkms`.
- [x] Generate an EDID binary encoding the HONOR Pad X8a's real panel spec
      (1200×1920, 60/90Hz) — see ADR 0008.
      `host/resources/edid/generate_edid.py` builds it programmatically
      (CVT timings from `cvt 1200 1920 60`/`90`, sRGB chromaticity, digital
      DisplayPort interface, "TabScreen" product name descriptor); output
      at `host/resources/edid/tabscreen-honor-pad-x8a.bin`, validated clean
      with `edid-decode` (correct 1200x1920 DTDs at 59.95/89.89 Hz, valid
      checksum, no warnings).
- [x] ~~Add the `drm.edid_firmware=DP-1-0:<file>` kernel boot parameter...~~
      **Tried, failed empirically, reverted — see ADR 0009.** Not a viable
      path on this hardware (wrong GPU/driver combination); superseded by
      the NVIDIA-native item below.
- [x] ~~Try NVIDIA's native `Option "ConnectedMonitor" "DFP-1"` + bare
      `Option "CustomEDID"` fragment...~~ **Tried, failed (no effect on
      DFP-1's connected state, plus a self-healing login-time crash) — see
      ADR 0011.** Superseded by the full-`Device`+`Screen` item below.
- [x] ~~Try `ConnectedMonitor`/`CustomEDID` again, shaped like the
      confirmed-working precedent...~~ **Tried — the mechanism works**
      (`TBS TabScreen (DFP-1): connected`, mode set to 1200×1920 correctly)
      **but the login greeter rendered onto the invisible virtual output
      instead of the real monitor** — see ADR 0012. Superseded by the
      off-by-default sequencing item below.
- [x] ~~Test the same confirmed-working `Device`+`Screen` config, but with
      the virtual output turned off...~~ Superseded — `monitors.xml`'s
      `<disabled>` mechanism (ADR 0014) is the right lever, not Xorg-level
      sequencing.
- [ ] **Confirm, with the user directly watching and reporting in real
      time (not reconstructed from logs afterward), that the desktop
      actually becomes usable after login with this config active** — the
      one unconfirmed and highest-priority item blocking Phase 2. Xorg's
      own log confirms the driver-level output state is correct
      (`DFP-1`/`DP-1` disconnected, real monitors fine); `gnome-shell`'s
      log shows a ~6m40s gap before the desktop became interactive in the
      one attempt made so far, which needs explaining (or fixing) before
      this can be marked done. See ADR 0016.
- [ ] Once desktop usability is actually confirmed: the same check via a
      live `xrandr --query` run interactively from within a session, and
      confirmation of exactly which `monitors.xml` location (`/etc/xdg/`
      vs `/var/lib/gdm3/.config/`) is the one a real user session actually
      reads — both are currently seeded, not yet isolated.
- [x] ~~Confirm the output can be toggled on via NVIDIA MetaModes
      switching...~~ Superseded — abandoned the entire `ConnectedMonitor`
      approach (ADR 0019). See the `vkms`-based checklist below instead.

### Current Phase 2 checklist (`vkms`-based, 2026-08-19)

- [x] Load `vkms` at boot via `/etc/modules-load.d/vkms.conf`; confirmed a
      real, connected, correctly-positioned output (`Virtual-2-1`)
      appears, with neither real monitor disturbed and no wrong-primary
      bug — see ADR 0021.
- [x] Add a custom mode matching the tablet's real panel via `xrandr
      --newmode`/`--addmode` against `Virtual-2-1`, confirmed live:
      `Virtual-2-1 connected 1200x1920+6400+0`, mode `1200x1920_60.00`
      current, both real monitors unaffected. (60Hz only so far; 90Hz mode
      not yet added.)
- [x] Confirmed live: `xrandr --output Virtual-2-1 --off` cleanly
      deactivates it (stays "connected", no active mode/geometry), real
      monitors unaffected. Reactivating needs **both** `--mode` and
      `--pos` explicitly (`--mode` alone resets position to `+0+0`,
      overlapping `eDP-1` — caught live, not a bug, just `xrandr`
      behavior after `--off` clears layout). Confirmed working:
      `xrandr --output Virtual-2-1 --mode "1200x1920_60.00" --pos 6400x0`.
      Host daemon implementation must always pass `--pos` on activate, not
      rely on remembered position.
- [ ] Confirm behavior at the GDM greeter specifically (not yet tested for
      `vkms`) — the NVIDIA saga's greeter problems were NVIDIA-specific,
      shouldn't apply here, but verify rather than assume.
- [x] Verified in arandr: all three monitors shown correctly
      (`eDP-1`/`HDMI-1-0`/`Virtual-2-1`, left-to-right, top-aligned),
      `Virtual-2-1` correctly rendered in its true portrait orientation
      (1200 wide × 1920 tall). Satisfies `CLAUDE.md`'s core requirement —
      a real, arandr-manageable RandR output.
- [x] Checked `vkms`'s configfs interface: it exists on this kernel
      (`/sys/kernel/config/vkms`, confirmed present) but is empty — the
      working `Virtual-2-1` comes from the module's own default-device
      creation, not configfs. Using it for EDID injection would mean
      building a whole device from scratch (planes/crtcs/encoders/
      connectors wired via symlinks), real complexity for no benefit
      since `--newmode` already fully works. Not pursued; only worth
      revisiting if Phase 7's multi-tablet support ends up needing
      configfs-based device creation anyway (that's the actual reason to
      touch it, not EDID).
- [x] Write installer step: `/etc/modules-load.d/vkms.conf` plus whatever
      mode/EDID setup is confirmed above — no DKMS build step needed,
      much simpler than the `evdi` install path would have been.
      `host/src/display.rs`'s `activate()` handles mode
      registration/positioning at runtime via `xrandr`; the only static
      installer piece needed is the one-line `vkms` module-load config
      (`echo vkms | sudo tee /etc/modules-load.d/vkms.conf`) — captured
      as a real installer script still TODO in Phase 6, not done here.
- [x] Host daemon: on tablet connect, activate the output with the
      tablet's mode and position; on disconnect, deactivate it. Implemented
      in `host/src/display.rs` (`activate()`/`ActiveOutput`'s `Drop` impl)
      and wired into `host/src/main.rs`'s `run_one_connection()` — the
      output is activated right after the AOA `HELLO` and deactivated
      automatically (via `Drop`) whenever that connection cycle ends, for
      any reason (unplug, error, or normal return). Position is computed
      dynamically each time (rightmost edge of currently-active real
      outputs), not hardcoded to this dev machine's `+6400+0` layout.
      Live-validated in isolation against the real X session (a throwaway
      `#[ignore]`d test, deleted after confirming, same pattern used for
      Phase 4's XTEST validation): `activate()` correctly computed
      `+6400+0` and set `1200x1920`, `drop()` cleanly turned it back off.
      **Now also validated with a real tablet physically connected**,
      multiple times (2026-08-20) — full pipeline confirmed working
      end-to-end (video decoding, `released>0 errors=0`; see ADR 0023).
      Contrary to the original note here: `sudo` turned out not to be
      required in this environment once the AOA device is already in
      accessory mode from a prior session — worth trying without it
      first. Also fixed (ADR 0025): the daemon had no signal handler, so
      Ctrl+C/`SIGTERM` aborted it without running `ActiveOutput::drop`,
      leaving the tabscreen output frozen active — a `ctrlc`-based
      handler (with the `termination` feature, for `SIGTERM`/`SIGHUP` too
      — directly relevant once Phase 6's systemd service stops the daemon
      via `SIGTERM`) now runs the same cleanup on any of these signals.
- [x] `host/src/video.rs` and `host/src/input.rs` now target the activated
      output's real rect (`display::OutputRect`) instead of the whole X11
      screen / root window — `video.rs`'s x11grab offset and
      `input.rs`'s `Injector` coordinate mapping both use it. Graceful
      degradation preserved if X11 is entirely unavailable (existing
      pattern: warn, drop INPUT frames / disable streaming) but no longer
      falls back to whole-screen capture once X11 is available — that
      placeholder is what this item retires.
- [ ] Handle the "no free output available" case (e.g. a second tablet)
      with a clear error — `vkms` only creates one default device without
      additional configfs-based configuration; multi-tablet support
      (Phase 7's open question) needs revisiting against this constraint.
      `display::activate()` currently just picks the first `Virtual-*`
      output found; with only one such output ever present today, a
      second tablet would silently collide onto the same output rather
      than erroring clearly — worth an explicit check before Phase 7.

## Phase 3 — Video Pipeline ✅ Validated live against the real tabscreen output (2026-08-20)

Originally captured the host's own current screen as a stand-in target
(same placeholder approach `host/src/input.rs` used for Phase 4, per
[ADR 0006](docs/adr/0006-dummy-driver-multi-gpu-binding-failure.md)).
Getting the Android decode side working against the real tablet took five
separate, initially-indistinguishable failure modes to work through —
full writeup in
[docs/adr/0007-mediacodec-low-latency-decoder-constraints.md](docs/adr/0007-mediacodec-low-latency-decoder-constraints.md).

**Update (2026-08-20):** once Phase 2 landed, pointing this pipeline at
the real `vkms` output's native 1200×1920 geometry (ADR 0022) exposed a
sixth failure mode: the target decoder has a real pixel-count ceiling
well below its declared capability XML, and hard-fails on genuine
1200×1920 content regardless of orientation or decode hint. Full
investigation and fix (encode output scaled down to 1088×1742, aspect
preserved) in
[docs/adr/0023-decoder-pixel-count-ceiling.md](docs/adr/0023-decoder-pixel-count-ceiling.md).
**Validated live end-to-end against the real HONOR Pad X8a with the real
tabscreen output as the capture target** (not the placeholder): 1560
frames received, 1507 released, zero errors, the host's actual desktop
content rendering correctly on the tablet.

- [x] Host: hardware-accelerated H.264 encode via VAAPI on the Intel iGPU.
      `host/src/video.rs` shells out to `ffmpeg` (x11grab capture +
      `h264_vaapi` encode, `-bf 0`/`-sei 0` tuned per ADR 0007) rather than
      reimplementing X11 capture and VAAPI encoding directly in Rust —
      Phase 0 already validated this exact pipeline shape empirically.
      Encode output is scaled to stay under `MAX_ENCODE_PIXELS` (aspect
      ratio preserved) — superseded the original width-only
      `MAX_ENCODE_WIDTH` cap once ADR 0023 found the real constraint is
      pixel-count-driven, not width-driven.
- [x] Host: send encoded frames over the Phase 1 transport's video channel
      — one Annex-B NAL unit per `VIDEO` frame, defined in
      `docs/wire-protocol.md`'s new VIDEO section.
- [x] Android: decode via `MediaCodec`, explicitly selecting
      `c2.qti.avc.decoder.low_latency` over the platform default
      (`VideoDecoder.selectDecoder()`), render fullscreen via
      `SurfaceView`. Required a dedicated output-draining thread decoupled
      from input feed cadence — inline polling starves this decoder's
      minimal internal pipeline and silently faults it under real load
      (ADR 0007 finding 4); this is now the standing pattern for any
      future `MediaCodec` consumer in this codebase.
- [x] Static-image + continuous streaming end-to-end test: validated live
      against the real HONOR Pad X8a (see above) — a single static frame
      wasn't tested in isolation, but continuous streaming succeeding is a
      strictly harder bar.
- [ ] Rigorous latency measurement (e.g. timestamped round trip) and
      formal bitrate/resolution tuning against the Phase 0 AOA throughput
      ceiling — not done yet; current settings (8Mbps, 30fps target,
      1088x1742 real encode output post-ADR-0023 downscale) were
      validated for stability, not tuned for quality or measured for
      latency.
- [x] Capture the tabscreen output's framebuffer specifically (not the
      host's whole current screen) — done via `display.rs`/ADR 0022, and
      live-validated end-to-end per this phase's header above.
- [x] Narrow the real decoder pixel-count ceiling found in ADR 0023 —
      done via a live binary search (ADR 0027): confirmed good at
      2,065,248px, confirmed bad at 2,071,160px, under 6,000px apart.
      Real encode resolution now 1816x1136 (2,063,776px, 89.6% of the
      tabscreen output's native area), up from the original conservative
      1088x1744 (82.4%) — `host/src/video.rs`'s `MAX_ENCODE_PIXELS` and
      the Android client's matching `HINT_WIDTH`/`HINT_HEIGHT` both
      updated. Live-validated sustained (3,480 frames received, 3,262
      released, 0 errors).

## Phase 4 — Input Injection ✅ Complete (2026-08-19)

Not blocked by Phase 2 — built, unit-tested, and validated live against the
real HONOR Pad X8a.

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
- [x] Verify the *full* path end-to-end against the real tablet
      (2026-08-19): connected the HONOR Pad X8a, host auto-completed the
      AOA handshake and CONTROL `HELLO`/`HELLO_ACK` with no manual steps
      (Phase 1's reconnect logic already handled this). Dragging a finger
      on the tablet moved the host's real cursor in real time (watched via
      `xdotool getmouselocation` — coordinates tracked the drag). A tap
      produced a genuine `ButtonPress`/`ButtonRelease` pair on the XTEST
      virtual pointer device, confirmed via `xinput test-xi2 --root`
      (cross-checked the detection method against a self-generated
      `xdotool click` first, so the absence of an event would have meant
      something). No errors in the host log throughout.
- [ ] Verify coordinate mapping is correct across output position/rotation
      changes made via arandr — no longer blocked (`host/src/input.rs`'s
      `Injector` now maps into the real tabscreen output's rect, offset
      included, per ADR 0022). Spot-checked via `adb shell input tap`
      against the real tablet (2026-08-20): a center tap landed in the
      right output (`x` within `Virtual-2-1`'s 6400-7600 range) but not
      pixel-exact, and repeating the identical tap gave a different
      result each time (~70px variance); a corner tap (100,100) landed
      **outside** the output entirely (`x=1363`, within `eDP-1`'s range).
      Inconclusive, not a confirmed bug in `Injector` itself — this
      project's own Phase 4 validation originally used real physical
      finger drags specifically because `adb`-synthetic touch isn't a
      reliable stand-in (screen insets/cutouts can distort the synthetic
      event's coordinate space in ways a real touch wouldn't hit the same
      way). Needs a real-finger re-check, not another `adb` attempt.

Phase 4 is otherwise complete; the remaining item just needs a live
re-check with real touch now that Phase 2 is unblocked.

## Phase 5 — Mode Negotiation ✅ Complete (2026-08-20)

Full writeup, including a real rotation-adjustment bug found and fixed
during live validation, in
[docs/adr/0028-phase5-mode-negotiation.md](docs/adr/0028-phase5-mode-negotiation.md).

- [x] Android: report tablet panel resolution + refresh rate during the
      Phase 1 handshake. New `DISPLAY_INFO` CONTROL message
      (`docs/wire-protocol.md`), sent right after `HELLO_ACK`. **Refresh
      rate is reported, not requested** — the client doesn't switch
      Android's display mode to 90Hz (confirmed available, Phase 0), it
      just reports whatever's currently active (defaults to 60Hz) —
      deliberately conservative per this item's own original note, given
      ADR 0023/0027 already found this hardware's real limits don't
      reliably match what's declared/assumed. Actively requesting 90Hz
      is explicitly left as a separate, not-yet-attempted future step.
- [x] Host: register a matching `xrandr` mode from the negotiated values
      instead of a hardcoded resolution — `main.rs`'s
      `wait_for_display_info` waits (5s timeout, then falls back to the
      old hardcoded constants, renamed `FALLBACK_TABLET_*`) for
      `DISPLAY_INFO` before calling `display::activate()`.
      Live-validated: host log shows `negotiated display:
      1920x1200@60Hz`, matching this session's manually-tuned constants
      exactly, and real decode confirmed on the tablet
      (`released=2317 errors=0`).
- [x] Handle tablet rotation — resolved as out-of-scope for v1 by
      design, not left unresolved: the Android client is locked to
      landscape (`AndroidManifest.xml`, this session's earlier UI work),
      so there's no runtime rotation to handle. `DISPLAY_INFO` reports
      that fixed orientation's dimensions once, at connection time. A
      real bug was found and fixed here during live testing:
      `Display.Mode.physicalWidth`/`physicalHeight` are **not**
      rotation-adjusted (report the panel's native portrait shape even
      while actually rendering landscape) — `sendDisplayInfo()` now
      checks `Display.getRotation()` and swaps width/height when it's
      90°/270° from natural.

## Phase 6 — Packaging & Installation ✅ Host install path complete (2026-08-20)

- [x] udev rule granting non-root USB access to the AOA device:
      `host/resources/udev/69-tabscreen-aoa.rules`, matching both stages
      of the handshake (vendor `339b`/HONOR pre-handshake, `18d1`/Google
      product `2d00`/`2d01` post-handshake, mirroring `aoa.rs`'s own
      constants exactly). Uses `TAG+="uaccess"` (systemd-logind seat
      ownership) with `GROUP="plugdev"` as a fallback. **Not** a
      udev-triggered spawn-on-plug-in rule — superseded by the systemd
      service below (see its item for why that fits this daemon's
      existing design better).
- [x] systemd (user) service for the host daemon:
      `host/resources/systemd/tabscreen.service.template`. A
      continuously-running user service, not a udev-triggered one-shot —
      the daemon already has its own internal wait/retry loop
      (`aoa::wait_for_candidate_device`, up to 1hr) and reconnect logic
      (Phase 1), so a long-running service that's just always ready
      matches its existing architecture far better than spawning a fresh
      process per USB plug event would (which would need reworking the
      daemon to exit cleanly per-disconnect and would risk double-spawn
      races with udev). Templated (`__DISPLAY__`/`__XAUTHORITY__`/
      `__BIN_PATH__`) since a systemd user service's environment doesn't
      reliably inherit these on every distro/session-manager combo;
      `install.sh` fills them in from the installing shell's own
      environment at install time.
- [x] Installer script: `host/install.sh` (release build, binary to
      `~/.local/bin/tabscreen-host`, udev rule, `vkms` module-load config
      — idempotent, reuses the boot-time config from ADR 0021 if already
      present — and the systemd service, `enable --now`) and
      `host/uninstall.sh` (reverses all of it).
      **Live-validated end-to-end (2026-08-20)**: user ran `install.sh`
      directly, service came up `active`/`enabled` immediately, correctly
      found the tablet already in accessory mode and connected. Confirmed
      the daemon no longer needs a manually-run terminal command at
      all — full pipeline (activation, video decode, `released>0
      errors=0`) working through the systemd-managed process, same as
      every manual run this session. One expected hiccup, not a bug: the
      Android app needed a fresh relaunch to pick up the connection after
      sitting idle from a prior manual test — the same already-documented
      Phase 1 finding (`ACTION_USB_ACCESSORY_ATTACHED` unreliable on an
      already-running app instance), not new to this install path.
- [ ] Android app packaging for sideload distribution (no Play Store
      dependency assumed initially). **In progress (2026-08-20):**
      release signing is implemented and working —
      `android/app/build.gradle.kts` reads a `keystore.properties`
      (gitignored, generated live and verified with `apksigner`) and
      `gradle assembleRelease` produces a correctly-signed APK. **Not yet
      closed out**: the signed release build crashes on launch on the
      real tablet, reproduced consistently (including via direct
      physical taps, not just `adb`). A live bisection ruled out the
      obvious suspects — identical crash with `isDebuggable=true`, with
      the release build signed by the *debug* keystore instead, and even
      a from-scratch clean rebuild of the plain debug variant that had
      been working correctly for hours earlier the same session. The
      tablet's battery was critically low (down to 7%, draining even
      while nominally "charging") during the failed attempts, which is
      the leading suspect — Android aggressively kills newly-launching
      apps under critical battery pressure, which would explain a crash
      uncorrelated with which specific APK is installed. Paused pending
      the tablet actually charging back up on a proper power source (not
      through the host laptop's USB port) before retrying.

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

- [x] User-facing setup guide (install steps, enabling accessory mode,
      first-connect flow). `README.md` (new, project had none before) —
      prerequisites, host install (`install.sh`), Android sideload
      (including how to generate your own release keystore), usage, a
      troubleshooting section covering the two recurring live-testing
      papercuts this project kept hitting (app needs a fresh relaunch;
      physical replug clears a stuck USB state), and an honest
      "Known limitations" section (single validated hardware pairing,
      X11-only, right-side placement only, one tablet at a time, 90Hz
      not requested, GDM greeter unverified).
- [ ] Update `CLAUDE.md` with any architecture decisions finalized during
      implementation that diverge from the current plan.
- [ ] Versioned release/tag of host daemon + Android client.
