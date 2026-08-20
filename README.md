# tabscreen

Use an Android tablet as a second monitor for a Linux desktop, connected
over USB — no Wi-Fi, no network stack, no `adb`. The tablet enumerates as
a real USB accessory and shows up as a genuine X11/RandR output you can
manage with `arandr` like any physical monitor.

Why it works this way (and why it's harder than it sounds) is documented
in [`CLAUDE.md`](CLAUDE.md). This file is the practical "how do I run it"
guide; [`TASKS.md`](TASKS.md) tracks project status in detail, and
[`docs/adr/`](docs/adr/) has the full decision history for anything
surprising you hit along the way.

## Current status

**v1, targeting one specific device pairing**: this laptop (Intel iGPU,
X11/GNOME) and one specific tablet, the HONOR Pad X8a. It works, but it
hasn't been generalized beyond that pairing — see
[Known limitations](#known-limitations) below before assuming it'll work
with different hardware out of the box.

## How it works, briefly

1. **Host** creates a virtual monitor as a real X11 RandR output (via the
   in-tree `vkms` kernel driver), sized to match the tablet's actual
   screen.
2. **Host** captures that output and encodes it to H.264 with hardware
   acceleration (VAAPI).
3. The encoded stream, plus touch input going the other way, travels over
   a raw USB connection to the tablet using AOA (Android Open Accessory
   Protocol) — the same category of USB mode used by car head units, not
   a network protocol.
4. **Tablet** decodes and renders fullscreen, and sends touch events back.

## Prerequisites

**Host** (tested on Ubuntu 24.04):

- A GPU with VAAPI support (this project only validated Intel iGPU)
- `vkms` support in your kernel (in-tree on any reasonably recent Linux
  kernel; check `modinfo vkms`)
- Rust (edition 2024 — a recent stable toolchain)
- `ffmpeg`, `x11-xserver-utils` (for `xrandr`), `xcvt` (for `cvt`)

```
sudo apt install ffmpeg x11-xserver-utils xcvt
```

**Tablet**: an Android device supporting AOA (most do). Only ever tested
against a HONOR Pad X8a — see [Known limitations](#known-limitations).

**Building the Android client**: JDK 17, Android SDK (`compileSdk` 34,
`minSdk` 26). [Gradle](https://gradle.org/) 8.9 — no wrapper is checked
in, so either install it yourself or point an existing Gradle install at
`android/`.

## Installing

### Host

```
cd host
./install.sh
```

This builds a release binary, installs a udev rule (non-root USB access
to the tablet), makes sure `vkms` loads at boot, and registers/starts a
systemd **user** service that runs the daemon automatically from then on
— no more manually running a binary in a terminal. It'll ask for your
`sudo` password for the udev rule and the `vkms` boot config; everything
else runs as your normal user.

Run it from a real logged-in graphical session (a terminal opened from
your desktop), not a bare SSH session or a TTY — it needs your session's
`DISPLAY`/`XAUTHORITY` to bake into the service.

To remove everything it installed: `./host/uninstall.sh`.

### Android client

No Play Store distribution — sideload the APK:

```
cd android
gradle assembleRelease
adb install app/build/outputs/apk/release/app-release.apk
```

(`assembleRelease` needs a signing key — see `keystore.properties` /
`build.gradle.kts`'s `signingConfigs` block. If you're building your own
release for the first time, generate one:

```
cd android
keytool -genkeypair -v -keystore tabscreen-release.keystore -alias tabscreen \
  -keyalg RSA -keysize 2048 -validity 10000
cat > keystore.properties << EOF
storeFile=tabscreen-release.keystore
storePassword=<the password you just set>
keyAlias=tabscreen
keyPassword=<same password -- PKCS12 keystores don't support separate store/key passwords>
EOF
```

**Back this keystore up somewhere durable and never commit it.** It's
the app's permanent identity — losing it means any future update can't
be installed over an existing copy without a full uninstall first.)

If you don't want to build it yourself, `gradle assembleDebug` produces
a debug-signed APK that works identically for a single-device setup —
just skip the signing config entirely.

Without a computer to `adb install` from, transfer the APK to the
tablet by any means (email, cloud storage, USB file transfer) and tap it
in a file manager — Android will prompt to enable "install from unknown
sources" for that source app the first time.

## Using it

1. Unlock the tablet, then plug it into the host's USB-C port.
2. **For the very first connection after installing**, let Android
   auto-launch the app from the USB-accessory prompt rather than opening
   it manually from the home screen — confirmed live that manually
   launching the app *before* it has ever been granted USB permission
   hits a crash in the permission-request fallback path (see
   [Troubleshooting](#troubleshooting)). If it doesn't auto-launch
   (MagicOS's auto-launch is known to be unreliable), unplug and replug
   rather than opening it by hand. Once permission has been granted once
   for this app install, opening it manually is fine from then on.
3. The tablet should show your host's desktop, and a new output (named
   something like `Virtual-2-1`) should appear in `arandr`/`xrandr`,
   positioned to the right of your existing monitors. Drag it around in
   `arandr` like any other monitor if you want a different position —
   left-side placement is not currently supported by the daemon itself
   (see [Known limitations](#known-limitations)).
4. Touch the tablet's screen to control the host's mouse.
5. Unplug when done — the virtual output deactivates automatically.

## Troubleshooting

- **App crashes immediately on launch, every time** (only seen so far
  when manually opening/reopening the app *before* it has ever been
  granted USB permission — e.g. testing with `adb shell am start`
  instead of a real accessory connection): unplug and replug the tablet
  instead of opening the app by hand. The crash is in the manual
  permission-request fallback path
  (`MainActivity.requestOrOpen()`/`UsbManager.requestPermission()`); the
  normal accessory-attach flow doesn't go through it and isn't affected.
  Once permission's been granted once, manually reopening the app is
  fine from then on.
- **Tablet shows "Connected. Waiting for HELLO..." and never proceeds**:
  the most common issue by far, once permission's already been granted.
  Force-stop and relaunch the Android app. Android's accessory-attached
  auto-launch isn't reliable on all devices, so the app sometimes needs
  a manual nudge to notice an already-active connection.
- **Still stuck after relaunching the app**: physically unplug and
  replug the USB cable. This forces a full USB re-enumeration and clears
  almost any stuck state on either end.
- **Check the daemon's logs**:
  `journalctl --user -u tabscreen.service -f`
- **Video looks fine but touch doesn't work, or vice versa**: check the
  log above for errors on the specific channel; the two are independent
  pipelines and can fail separately.

## Known limitations

- **Single hardware pairing validated.** Real hardware quirks specific
  to this laptop and this tablet are baked into the current defaults —
  see `docs/adr/` for details (especially
  [0023](docs/adr/0023-decoder-pixel-count-ceiling.md) and
  [0027](docs/adr/0027-narrowed-decoder-pixel-ceiling.md) on a real
  hardware decoder pixel-count ceiling well below what the tablet's own
  vendor driver claims).
- **X11 only, not Wayland** — `arandr`/`xrandr` need X11's RandR
  extension, which Wayland doesn't expose (see `CLAUDE.md`).
- **Right-side placement only.** Left-side placement exists in the code
  but is disabled — it hit a GNOME/mutter display-management interaction
  that needs more work before it's safe to enable by default (see
  [ADR 0024](docs/adr/0024-left-placement-tried-and-reverted.md)). You
  can always reposition manually in `arandr` after connecting.
- **One tablet at a time.** Multi-tablet support isn't implemented.
- **90Hz isn't requested**, even on tablets that support it — the client
  reports whatever refresh rate is already active rather than switching
  to a higher one (see [ADR 0028](docs/adr/0028-phase5-mode-negotiation.md)).
- **GDM greeter behavior with `vkms` active is unverified** — should be
  fine (this failure mode was NVIDIA-`ConnectedMonitor`-specific, not a
  `vkms` property), but hasn't actually been tested.
