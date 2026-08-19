# 1. Host daemon language: Rust

Date: 2026-08-18

## Status

Accepted

## Context

The host daemon is the Linux-side component of tabscreen: it negotiates the
AOA (Android Open Accessory) USB link, captures the virtual display's
framebuffer, hardware-encodes video, drives the `xf86-video-dummy` output
through `xrandr`, and injects input events received from the tablet. It sits
directly on `libusb`, VAAPI/`ffmpeg`, and X11/XRandR — all C APIs — in a
latency-sensitive path, and parses data arriving over USB from the tablet.

The initial instinct was that, since `libusb` (and the other core
dependencies) are native C libraries, C might be the "path of least
resistance" language for the daemon, avoiding an FFI layer entirely. This
ADR records why that instinct doesn't hold up, and why Rust was chosen
instead.

## Decision

The host daemon will be written in **Rust**, using `rusb` (over
`libusb1-sys`) for USB, `x11rb` for X11/RandR control, and FFI to
`ffmpeg`/VAAPI for hardware video encode.

### Why not C, despite `libusb` being a C API

- **FFI overhead is negligible here.** Every operation the daemon performs
  through these libraries — USB bulk transfers, VAAPI encode calls, X11/
  RandR requests — already crosses a kernel syscall or driver boundary
  (microseconds-to-milliseconds). An additional Rust↔C shim call is noise
  against that cost. FFI overhead only matters when it's comparable to the
  cost of the operation itself, which isn't the case for any of tabscreen's
  hot paths.
- **The bindings are mature, not toy wrappers.** `rusb`/`libusb1-sys` and
  `x11rb` are established, actively maintained bindings used in real
  production tools, not thin experimental shims.
- **The daemon parses untrusted-ish input by nature.** The Phase 1 wire
  protocol (length-prefixed frames, input events — see `TASKS.md`) arrives
  over USB from the tablet: a compromised, buggy, or simply flaky client can
  send malformed frames. Parsing externally-supplied, variable-length binary
  data is exactly the class of code that historically produces
  buffer-overrun/use-after-free bugs in C (see the CVE history of USB
  gadget stacks and media parsers). The daemon also runs with real USB
  device permissions granted via udev (see `CLAUDE.md`'s No-Network
  Decision). Rust's memory safety is a substantive risk reduction here, not
  a stylistic preference.
- **Cross-platform code sharing doesn't favor C either.** The USB transport
  itself can't be shared between host and tablet regardless of language —
  the host negotiates AOA via `libusb` (host/PC role) while the tablet uses
  Android's `UsbManager`/`UsbAccessory` Java/Kotlin framework (accessory/
  device role); these are unavoidably different APIs. What *can* be shared
  is the wire-protocol codec (framing/serialization). Rust cross-compiles
  cleanly to `aarch64-linux-android` and binds into Kotlin via JNI (the
  `jni` crate, or `uniffi-rs`) about as well as C++ does — so C++'s usual
  "one shared native module for both platforms" advantage isn't actually
  unique to C++ for this project.

### Where C/C++ would have genuinely won (and didn't apply here)

- Wanting a single toolchain with zero FFI anywhere.
- Leaning heavily on a C++-first SDK with no clean C or Rust bindings (e.g.
  a vendor hardware-encode SDK). Not the case: `ffmpeg`/VAAPI expose solid C
  APIs that Rust consumes as well as C++ does.

Go was also considered (simpler concurrency model, decent `libusb` bindings
via `gousb`) but was set aside: video encode would still mean wrapping
`ffmpeg` via cgo regardless, so it doesn't avoid FFI either, and its GC can
introduce latency jitter in a latency-sensitive daemon.

## Consequences

- The daemon's core dependencies (`rusb`, `x11rb`, `ffmpeg`/VAAPI FFI) need
  to be validated early — this is exactly what Phase 0's feasibility spikes
  in `TASKS.md` are for.
- Any future shared wire-protocol module between the host daemon and the
  Android client can be written once in Rust and cross-compiled to
  `aarch64-linux-android`, bound into Kotlin via JNI — this is now the
  default plan for the Phase 1 protocol codec rather than a maybe.
- Contributors need Rust familiarity, including `unsafe` code at the FFI
  boundaries with `libusb`/`ffmpeg`. Those boundaries should be kept as
  small and well-isolated as practical, since safety guarantees don't
  extend across an FFI call.
