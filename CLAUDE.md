# tabscreen

## Project Purpose

tabscreen is a software product that lets an Android tablet function as a second
monitor for a computer, connected via the tablet's USB-C port.

## Key Technical Constraint

Android tablets' USB-C ports support DisplayPort Alt Mode only as a *source*
(driving an external monitor), essentially never as a *sink* (receiving video
in). There is no plug-and-play hardware path to make a tablet act as a
monitor. USB in this product is therefore used as a data transport, not as a
raw video signal — tabscreen requires a host-side daemon plus an Android
client app that decodes a software video stream.

## No-Network Decision

**tabscreen does not use TCP/IP for the tablet link, by design.** The tablet
must behave like a real hardware peripheral (plug in, work immediately) —
not like a network endpoint that needs an interface, address, or socket.

This rules out RNDIS/USB tethering and `adb reverse`, since both establish an
IP-based link (a virtual network interface or forwarded TCP ports) even
though the cable itself is USB. It also rules out any Wi-Fi-based transport
outright. Instead the transport must be raw USB, using **AOA (Android Open
Accessory Protocol)**: the tablet enumerates as a USB accessory and
communicates over plain USB bulk endpoints, with no IP stack involved at any
layer.

### Consequences

**Wins:**
- *Feels like a real peripheral*: the host reacts to a USB plug/unplug event
  (e.g. a udev rule) the same way it would for a physical monitor — no
  network discovery, pairing codes, or handshake delay.
- No dependency on USB debugging / ADB being enabled on the tablet, and no
  developer-mode friction for end users.
- No open TCP/IP listening socket, no ARP/DHCP, no exposure to LAN-based
  attack surface or Wi-Fi/firewall interference that plagues network-based
  screen-sharing apps.
- Video, input, and control data can be multiplexed over one USB link with
  simple length-prefixed framing — no network protocol overhead (IP/TCP
  headers, handshakes, retransmission timers).

**Costs:**
- Significantly more engineering than reusing `adb reverse` or RNDIS: no
  off-the-shelf tooling assumes a non-IP USB transport, so tabscreen must
  implement its own framing, backpressure, and reconnect logic directly over
  AOA bulk transfers.
- AOA throughput is historically lower than a native RNDIS/USB-tethering
  link, which caps achievable resolution/frame rate more than an IP-based
  transport would — video encoding parameters need to be tuned against this
  ceiling rather than assumed away.
- On the host side, this means writing directly against USB (e.g. `libusb`
  on Linux) instead of talking to a normal network socket — and on any
  future Windows port, an unsigned custom USB interface needs a signed
  WinUSB-class driver, unlike RNDIS which uses an inbox driver.
- No standard network debugging tools (Wireshark on a virtual NIC, `nc`,
  etc.) apply; USB-level tooling (`usbmon`, `Wireshark` with USB capture) is
  needed instead.
- Rules out any future "same protocol over Wi-Fi as a fallback" option —
  a Wi-Fi transport would need an entirely separate implementation, not a
  reuse of the same wire protocol.

## Architecture Direction (chosen)

Custom video-streaming pipeline, matching what makes commercial tools (Duet
Display, Splashtop Wired XDisplay, SuperDisplay) actually good:

1. **Host**: create a virtual display as a real X11 RandR output (Linux
   `vkms`, the in-tree Virtual Kernel Mode Setting driver — not an
   app-level window, and not `xf86-video-dummy`; see
   [X11 RandR Integration Requirement](#x11-randr-integration-requirement)),
   capture its framebuffer.
2. **Encode**: hardware-accelerated H.264/H.265 for low latency and full
   frame rate — the reason a VNC-based approach was rejected (VNC's
   framebuffer-diffing performs poorly for video/motion/scrolling).
3. **Transport**: stream over raw USB via AOA (Android Open Accessory
   Protocol) — no RNDIS/USB tethering and no IP stack. See
   [No-Network Decision](#no-network-decision).
4. **Client (Android)**: decode the stream, render fullscreen, capture
   touch/stylus input and send it back to the host as injected mouse/touch
   events.

This was chosen over a simpler VNC-based approach (dummy output + x11vnc +
`adb reverse`/USB tethering + VNC client) because good motion/video
performance is the actual selling point of this product, not just static
screen mirroring.

## X11 RandR Integration Requirement

**The tablet must appear as a real xrandr output, manageable through arandr.**
Users need standard multi-monitor behavior: dragging windows onto it,
positioning/rotating it via arandr's GUI (or raw `xrandr` calls), setting its
mode/resolution, and enabling/disabling it — exactly as they would for a
physical monitor. This is not just a UX nicety; it's what distinguishes the
virtual display from an ordinary borderless window that happens to show
mirrored content.

This rules out implementing the "virtual display" as a top-level application
window (borderless, sized to the tablet, streamed) — a window is not an
addressable RandR output, so window managers won't let users snap/drag other
windows onto it the way they can onto a real monitor, and arandr would have
nothing to show for it. The virtual display has to be a genuine output on
the RandR tree.

**Implementation note (revised after live hardware investigation, see
`docs/adr/0006` through `0021` for the full trail):** the originally
planned mechanism, `xf86-video-dummy`, turned out not to work on this
hardware at all — it couldn't coexist with a real GPU already driving
the live session's real monitors (`docs/adr/0006`), and neither could
several NVIDIA-native workarounds tried afterward (they conclusively
broke real external-monitor detection, `docs/adr/0019`). The actual
mechanism is **`vkms`** (Virtual Kernel Mode Setting), an in-tree Linux
kernel driver: a genuinely separate virtual GPU that never touches a
real GPU's own detection/output state, so it can't cause the failures
`xf86-video-dummy` and the NVIDIA approaches did (`docs/adr/0021`). The
architectural requirement below — a real RandR output, not a window —
is unchanged; only the driver providing it is different from the
original plan.

### Consequences

- **X11/Xorg only, not Wayland.** arandr/xrandr talk to the X11 RandR
  extension; Wayland compositors don't expose it (they use separate,
  compositor-specific protocols like `wlr-output-management`, which arandr
  cannot drive at all). This scopes tabscreen's host support to X11 sessions
  for the foreseeable future — a real limitation given several distros now
  default to Wayland.
- **`vkms` *is* genuinely hot-pluggable, unlike the originally planned
  `xf86-video-dummy`.** No Xorg config snippet, no X server restart: the
  kernel module is loaded once at boot (`/etc/modules-load.d/vkms.conf`),
  and the host daemon does `xrandr --newmode`/`--addmode` (registering a
  mode matching the tablet's real panel) plus `--output ... --mode ...
  --pos ...` on tablet connect, and `--output ... --off` on disconnect —
  all live, ordinary RandR calls, no session disruption. This turned out
  simpler than the original plan anticipated, not harder.
- **Installation still needs elevated privileges, but much less than
  originally planned.** The only system-wide, root-requiring step is the
  one-line `vkms` boot-time module config — no Xorg config snippet to
  write, no display-manager/session restart. Windows (IddCx) and macOS
  both have supported APIs for creating a virtual display dynamically
  without touching system-wide config at all; Linux/X11 still has no
  equivalent, but the gap is narrower than `xf86-video-dummy` would have
  required.
- **Mode matching is now a hard requirement, not an optimization.** Because
  arandr shows the output's actual mode/resolution, tabscreen must query the
  tablet's real panel resolution and refresh rate (over the AOA link) and
  register a matching `xrandr` mode (`--newmode`/`--addmode`), rather than
  hardcoding one dummy resolution — a mismatch would be visible/awkward in
  arandr itself, not just a quality issue in the video stream.
- **Standard WM/compositor behavior comes for free once the output exists**:
  fullscreening an app onto the tablet, per-output DPI/scaling (if the WM
  supports it), and drag-to-move between monitors all work exactly like any
  other multi-monitor setup, with no tabscreen-specific window handling
  required.

## Prior Art / Alternatives

**Commercial (closed source, video-codec based):**
- Duet Display
- Splashtop Wired XDisplay
- SuperDisplay
- spacedesk
- AirDroid Cast
- Samsung Second Screen (Galaxy ecosystem only)

**Open source (Linux-focused, VNC-based):**
- [Prayag2/android-as-monitor-linux](https://github.com/Prayag2/android-as-monitor-linux) —
  Python script, creates a virtual monitor + VNC server, connects over Wi-Fi
  or USB tethering.
- [ChipCE/Smartphone-tablet-as-second-monitor-for-linux](https://github.com/ChipCE/Smartphone-tablet-as-second-monitor-for-linux) —
  CLI tool creating a virtual monitor for use with any VNC-compatible client.
- [luisfemd/duaekrano](https://github.com/luisfemd/duaekrano) — script using
  reverse USB tethering to set up a tablet as a second monitor.

**Related but different use case:**
- [Genymobile/scrcpy](https://github.com/Genymobile/scrcpy) — mirrors an
  Android device's screen *to* a computer (opposite direction of tabscreen),
  but is the reference implementation worth studying for its low-latency
  H.264 streaming and input-injection protocol design. Note: scrcpy's USB
  mode is still `adb`-over-USB, i.e. TCP/IP-over-USB, not a fit for the
  no-network constraint below.

**Note on transport:** every alternative surveyed above — commercial and
open source alike — moves data over TCP/IP-over-USB (RNDIS tethering or
`adb reverse`), not raw USB. tabscreen's no-network requirement (see below)
means none of them can be used as a transport reference; AOA-based USB
accessory communication has to be built from scratch.
