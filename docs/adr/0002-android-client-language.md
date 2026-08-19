# 2. Android client language: Kotlin

Date: 2026-08-18

## Status

Accepted

## Context

The Android client registers as a USB accessory (`UsbManager`/
`UsbAccessory`), exchanges frames over the AOA link from
[0001](0001-host-daemon-language.md)'s wire protocol, decodes video via
`MediaCodec`, renders fullscreen, and captures touch/stylus input to send
back to the host. All of the OS-level APIs it depends on — `UsbManager`,
`MediaCodec`, `Surface`/`SurfaceView` — are first-class Java/Kotlin
framework APIs, not APIs that require native (NDK) access to use correctly.

## Decision

The Android client will be written in **Kotlin**.

- It's Google's default for new Android development, with the framework
  APIs the client depends on (`UsbManager`, `MediaCodec`) designed
  Java/Kotlin-first.
- Kotlin coroutines fit this client's concurrency shape well: a USB
  accessory read loop, async `MediaCodec` decode callbacks, and a
  touch-input capture/send loop all running concurrently. Structured
  concurrency avoids the callback-nesting that the equivalent Java
  (`AsyncTask`/manual `Thread`) code would produce.
- No NDK/C++ layer is needed for the performance-critical decode/render
  path: `MediaCodec` writing directly to a `Surface` already hands frames to
  hardware without the app touching pixel buffers, so there's no hot loop
  that native code would meaningfully speed up. If profiling later reveals
  otherwise (unlikely), a narrow NDK module can be added without discarding
  the Kotlin app shell.
- If a shared wire-protocol codec module is built per
  [0001](0001-host-daemon-language.md)'s consequences (Rust, cross-compiled
  to `aarch64-linux-android`), it binds into the Kotlin app via JNI — this
  is a normal, well-supported pattern and doesn't push the client toward
  Java or full native implementation.

Java and Flutter/Dart/React Native were considered and rejected quickly:
Java has no advantage over Kotlin for a new codebase (more verbose, no
null-safety, no coroutines). Flutter/React Native solve a problem tabscreen
doesn't have (multi-platform UI) — it targets Android only — and neither has
cross-platform bindings for `UsbManager`/`UsbAccessory` or `MediaCodec`, so
either would still need a native Kotlin (or Rust) plugin underneath to do
the actual work, adding a framework layer for no benefit.

### Why not a full-Rust client, given the host is already Rust ([0001](0001-host-daemon-language.md))

This is the alternative worth taking seriously, not dismissing — "one
language for the whole project" is a legitimate argument, and it's
technically viable: Android exposes a *native* MediaCodec API (`AMediaCodec`
via NDK) that decodes directly to an `ANativeWindow`, so the decode/render
hot path can be done in pure native code with zero JNI in the loop. This
isn't a Kotlin-exclusive capability, and frameworks like `android-activity`
make a NativeActivity-based Rust app real, not exotic in principle.

What decides it: **`UsbManager`/`UsbAccessory` — the actual AOA API
tabscreen's transport depends on — has no NDK-native equivalent.**
Accessory file descriptors can only be obtained via
`UsbManager.openAccessory()`, a Java framework method; so does the
accessory intent-filter registration, the permission dialog, and the
Activity lifecycle. A full-Rust client doesn't escape the JNI boundary — it
only relocates it, from "Kotlin app calls into a small Rust `.so` for the
shared protocol codec" to "Rust app calls out to Java framework methods for
USB/lifecycle/permissions via JNI shims." Either way, a JNI boundary exists;
the only question is which side is the "host."

Given that the JNI tax is unavoidable either direction, the tiebreaker is
ecosystem maturity — and it isn't close. `UsbAccessory`/AOA is already a
niche, thinly-documented corner of Android compared to, say, Bluetooth or
USB host mode, and what community knowledge exists (Stack Overflow, sample
code, blog posts) is almost entirely Java/Kotlin. NDK-native Android apps
via `android-activity` are a path walked mostly by game developers
(`winit`/`bevy`), not hardware-accessory apps — going there compounds
"already debugging an obscure USB mode" with "and doing it off the
platform's well-trodden path," for no offsetting performance gain, since
Kotlin's `MediaCodec`-to-`Surface` path is already zero-copy. The
language-uniformity argument is real, but it's outweighed by giving up
ecosystem maturity in exactly the area (AOA) where tabscreen will need it
most.

## Consequences

- Contributors need Kotlin + Android framework familiarity (`UsbManager`
  accessory APIs, `MediaCodec`, coroutines).
- The client stays a normal Android Studio/Gradle project; only the shared
  protocol module (if built) introduces NDK/JNI build complexity, and only
  for that one piece.
- If Android ever adds an NDK-native `UsbAccessory` API, or the AOA
  community/tooling landscape shifts meaningfully toward native Rust, this
  decision should be revisited explicitly with its own ADR rather than
  drifting — the reasoning above depends on the current state of the
  platform, not a permanent property of it.
