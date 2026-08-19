# 4. Target scope for v1: this machine + HONOR Pad X8a only

Date: 2026-08-18

## Status

Accepted

## Context

`TASKS.md`'s Phase 6 (packaging) and the [X11 RandR Integration
Requirement](../../CLAUDE.md#x11-randr-integration-requirement) in
`CLAUDE.md` both depend on knowing how broad v1's distro/DE support needs to
be — a personal tool and a "works on GNOME/KDE/XFCE across distros" product
call for very different amounts of installer/config-generalization work.

The actual development machine is Ubuntu 24.04.4 LTS ("noble"), currently
running a GNOME (Mutter) session on X11 (confirmed via
`XDG_SESSION_TYPE=x11`). It's a hybrid-GPU laptop: an Intel Raptor Lake-S
iGPU (`i915`/`xe` driver) currently driving the X11 session per `glxinfo`,
plus an Nvidia RTX 4070 Max-Q dGPU (`nvidia` driver) available but not
driving the display.

The target tablet is a **HONOR Pad X8a**: Snapdragon 680 4G chipset, 11.0″
display, 4GB RAM, USB-C (USB 2.0, OTG), Android 14 with MagicOS 8 (Honor's
OEM skin). Scope for v1 is this specific host+tablet pairing, not "some
Android tablet" in the abstract — both sides are now concrete enough that
Phase 0's remaining spikes should be validated against these exact devices.

## Decision

**v1 targets this machine + this tablet only: Ubuntu 24.04 LTS, GNOME on
X11, Intel Raptor Lake iGPU, paired with a HONOR Pad X8a.** No cross-distro,
cross-DE, or cross-device generalization work in v1.

- Installer/config (Phase 6) can hardcode Ubuntu-specific assumptions:
  `apt` packaging only, `/etc/X11/xorg.conf.d/` as the dummy-driver config
  location, systemd (Ubuntu's init system) for the host daemon service, and
  package versions matching 24.04's repos — no distro-detection logic, no
  packaging for other package managers.
- Hardware video encode targets the **Intel Quick Sync (VAAPI) path**, not
  Nvidia NVENC, because the X11 session — and therefore the dummy RandR
  output's framebuffer — is rendered on the Intel iGPU. Encoding via NVENC
  instead would mean copying frames across PCIe to the discrete GPU for no
  benefit, since nothing else in the pipeline needs the dGPU. This also
  means Phase 0's feasibility spikes should validate VAAPI specifically on
  this Raptor Lake iGPU, not encode hardware in the abstract.
- **Correction (superseded by empirical test below):** this ADR originally
  claimed Raptor Lake's Quick Sync engine has hardware AV1 encode. It does
  not — see the confirmed VAAPI results below. Left here so the reasoning
  trail is visible rather than silently rewritten.
- Phase 6's "common X11 desktop environments broadly" work (generalized
  installer, testing across KDE/XFCE/other distros) is explicitly deferred,
  not designed for preemptively. If tabscreen is ever opened up beyond
  personal use, that generalization is a deliberate future decision — it
  should get its own ADR when it happens, not be half-built now on
  speculation.

### Confirmed: AOA works on this tablet (empirical, 2026-08-18)

With the tablet connected via its stock USB-C cable in its default
MTP-connected state — no developer options, no USB debugging, no
"connect as" mode change — a host-side probe using `libusb`/`pyusb`
confirmed the full AOA handshake against the real device (VID/PID
`339b:107d`, product string `NDL-W09`):

1. `GET_PROTOCOL` (control transfer, `bmRequestType=0xC0`, `bRequest=51`)
   returned **protocol version 2** — AOA is live at the kernel/gadget level,
   not just claimed by the spec sheet.
2. Sending the six `SEND_STRING` identification strings followed by
   `START_ACCESSORY` (`bRequest=53`) caused the tablet to cleanly disconnect
   and re-enumerate as `18d1:2d00` (Google's accessory-mode ID, no-ADB
   variant) — the full handshake completes, not just the version query.
3. The re-enumerated device exposes exactly one vendor-specific interface
   with two bulk endpoints (EP1 IN/OUT, 512-byte packets — USB 2.0
   High-Speed bulk) — a clean, textbook AOA accessory interface with no
   visible MagicOS-specific alteration at this layer.
4. A bulk `OUT` write to that endpoint timed out, as expected — nothing on
   the Android side is listening yet, since no app has claimed the
   accessory. This is not a failure; it marks where empirical validation
   stops and real Phase 1 app work (the `UsbAccessory` reader) begins.

This resolves the biggest Phase 0 unknown for this specific pairing:
**MagicOS 8 does not appear to interfere with standard AOA behavior**, and
no special tablet-side configuration is required for the handshake to work
— consistent with the "feels like a real peripheral" goal in `CLAUDE.md`'s
No-Network Decision.

### Confirmed: VAAPI hardware encode on the Intel iGPU (empirical, 2026-08-18)

`/dev/dri/renderD128` (the lower-numbered, default render node) maps to the
Intel PCI device (`0000:00:02.0`, vendor `0x8086`); `renderD129` maps to the
Nvidia dGPU (`0000:01:00.0`, vendor `0x10de`). This means code that opens
the default/first render node already lands on Intel with no hybrid-GPU
selection logic required — confirms the assumption in this ADR's Decision
section.

Actual `ffmpeg`+VAAPI encode tests against `renderD128`:

| Codec | Result |
|---|---|
| H.264 | **Works.** Hardware encode confirmed, 30 frames @ 16.4x realtime. |
| H.265/HEVC | **Works.** Hardware encode confirmed, 30 frames @ 10.7x realtime. Confirms [0003](0003-video-codec-baseline.md)'s H.265 exclusion was a licensing choice, not a hardware limitation — the capability is there. |
| AV1 | **Fails.** `No usable encoding entrypoint found for profile VAProfileAV1Profile0`. |

The AV1 failure corrects the mistaken claim earlier in this ADR. Raptor
Lake's iGPU is Gen12/Xe-LP generation — it has AV1 *decode* but not AV1
*encode*; Intel's AV1 hardware encode block first appears with Meteor
Lake's Xe-LPG and with Arc discrete GPUs, both newer than what's in this
machine. This is a direct correction to [0003](0003-video-codec-baseline.md),
which also mis-stated the generation cutoff as "12th-gen+" — that ADR needs
updating too.

**Practical consequence:** on this specific host, the AV1 upgrade path from
[0003](0003-video-codec-baseline.md) is not available via VAAPI at all. The
only AV1 hardware encode on this machine would be through NVENC on the RTX
4070 dGPU — which reintroduces exactly the cross-GPU frame-copy cost this
ADR's Decision section ruled out. So for this machine, the realistic
options if H.264 throughput proves insufficient are: (a) tune H.264
bitrate/resolution harder before reaching for a codec change, or (b)
explicitly accept the cross-GPU copy cost and use NVENC AV1 despite the
tradeoff. Neither is free; this needs a decision when/if it's actually
reached, not now.

### Confirmed: tablet codec hardware support (empirical, 2026-08-18)

With USB debugging enabled and `adb` authorized, the tablet's actual
vendor-declared codec list (`/vendor/etc/media_codecs_khaje_v0.xml` /
`media_codecs_bengal_v1.xml` — Qualcomm's internal codenames for this SD680
platform) was read directly rather than inferred from spec sheets:

| Codec | Decoder | Encoder |
|---|---|---|
| H.264/AVC | **Hardware** (`c2.qti.avc.decoder`, plus a dedicated `.low_latency` variant) | Hardware (`c2.qti.avc.encoder`) |
| H.265/HEVC | **Hardware** (`c2.qti.hevc.decoder`, plus `.low_latency` variant) | Hardware (`c2.qti.hevc.encoder`) |
| AV1 | **Software only** (`c2.android.av1.decoder`, `variant="slow-cpu"` — Android's generic CPU fallback, no `c2.qti.*` hardware entry exists) | Not present at all |

This **definitively closes** the AV1 question the spec sheet only made
"doubtful": the Snapdragon 680 has no hardware AV1 decode, only a
CPU-software fallback explicitly tagged slow — unusable for real-time
streaming. Combined with the confirmed VAAPI encode gap above, **AV1 is not
viable on this host+tablet pairing on either end**, full stop, not just
"suspect." [0003](0003-video-codec-baseline.md)'s AV1 upgrade path is
theoretical for this project until either piece of hardware changes.

The H.264 baseline is strongly validated by this same data: genuine
Qualcomm hardware decode, and — notably — a **dedicated low-latency
decoder variant** (`c2.qti.avc.decoder.low_latency`), which is exactly what
a real-time peripheral-like display needs and isn't something to take for
granted on a budget chipset.

Also captured while ADB was available (useful for
[Phase 5](../../TASKS.md#phase-5--mode-negotiation)): actual panel is
**1200×1920, up to 90Hz** (`supportedRefreshRates [90.0, 60.000004]`,
currently running at 60Hz by default) — better than the spec sheet's bare
"11.0″ display" suggested. Also, the tablet is now running **Android 16
(SDK 36)**, not the Android 14 it originally shipped with per the spec
sheet — it's received OS updates since its 2024 launch. `CLAUDE.md` and
future ADRs should treat "Android 14/MagicOS 8" as the launch spec, not the
current running version.

### Confirmed: AOA throughput and latency (empirical, 2026-08-18)

With the minimal `spikes/aoa-hello-android/` app installed and running
(open accessory, echo payloads ≤64 bytes, count/log larger ones), the host
ran real measurements against the live bulk endpoints rather than the
estimated USB 2.0 ceiling this ADR originally cited:

- **Throughput: 20.70 MB/s (165.6 Mbps)** sustained, one-way host→tablet,
  16KB chunks, ~30MB total. Lower than the naive 30-35 MB/s USB 2.0
  estimate (AOA framing/protocol overhead accounts for the gap), but not a
  practical constraint: a 1200×1920 H.264 stream at good quality typically
  needs well under 25 Mbps, leaving roughly 6-10x headroom. **The
  throughput risk flagged throughout this ADR does not bind in practice.**
- **Latency: 0.21-3.17ms round-trip** (avg 0.70ms, 20 samples, 32-byte
  echo payloads). Negligible — end-to-end latency will be dominated by
  video encode/decode, not the USB/AOA transport itself.

**Real finding along the way, worth its own note:** the accessory-attached
intent-filter did not reliably bring the app to the foreground on MagicOS,
even though the accessory permission was silently granted at the system
level — the app had to be launched manually and fall back to
`UsbManager.accessoryList`/`requestPermission()`. Phase 1's real Android
client should include that fallback defensively, not assume auto-launch
works. Logcat visibility for the app was also unreliable on this device;
don't design Phase 7's field debugging around logcat alone.

## Consequences

- Faster path through Phases 1–5: no time spent generalizing config or
  abstracting over desktop environments/distros that aren't being tested
  anyway.
- The GPU-hybrid nature of this machine needs a small guard in Phase 0/3:
  confirm VAAPI enumerates the Intel device (not the Nvidia one) by default
  in whatever encode library is used, since hybrid-GPU Linux systems can
  sometimes expose both `/dev/dri/renderD*` nodes and pick the wrong one.
- Anything hardcoded per this ADR (paths, `apt`, systemd, VAAPI-on-Intel)
  is a known, intentional source of rework if the target scope broadens
  later — tracked here so that future generalization work knows exactly
  what it's undoing.
