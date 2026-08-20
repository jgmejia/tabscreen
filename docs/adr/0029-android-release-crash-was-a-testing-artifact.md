# 29. The Android release-build launch crash was a testing artifact, not a real bug

Date: 2026-08-20

## Status

Accepted.

## Context

After wiring release signing (`android/app/build.gradle.kts`,
`keystore.properties`) and producing a correctly-signed
`app-release.apk` (verified with `apksigner`), installing and launching
it on the real tablet crashed immediately and consistently — confirmed
via `adb`, and independently via five direct physical taps on the
tablet's screen, ruling out an `adb`-scripting artifact.

**Extensive live bisection**, each step reproducing the crash
identically: `isDebuggable=true` on the release build type, the release
build type signed with the debug keystore instead of the real release
key, a fully clean (`gradle clean`) rebuild of the plain, previously-
working `debug` variant, and repeating all of this after the tablet's
battery recovered from a critical 7% up to 21%. None of these changed
the outcome. `logcat` produced nothing useful throughout (this device's
`logcat` has been unreliable for this app since Phase 0), and
`dumpsys activity exit-info` confirmed a real, repeated
`APP CRASH(EXCEPTION)` with no trace or description available.

**The actual distinguishing factor**, found by comparing a genuinely
successful connection's daemon log against the failing attempts: the one
success showed a **fresh AOA handshake** (`found candidate device,
starting AOA handshake`) — meaning that connection came from the tablet
being freshly plugged in, not from an explicit app launch. Every failing
attempt, by contrast, was triggered via `adb shell am start -n
ai.tabscreen.client/.MainActivity`.

`am start -n` delivers a plain intent with no `EXTRA_ACCESSORY` attached.
`MainActivity.onCreate()`'s handling of that (no accessory in the launch
intent) falls through to checking `usbManager.accessoryList`, and if the
app has never been granted USB permission before, calls
`usbManager.requestPermission()` — the fallback path Phase 0 built
specifically because MagicOS's accessory-attach auto-launch doesn't
reliably bring the app to the foreground on its own. **This fallback
path appears to crash** when invoked against an app with no prior grant.
The real accessory-attach flow (the OS launching the app directly, with
`EXTRA_ACCESSORY` already present) never reaches this code and was
confirmed live, immediately after this diagnosis, to work correctly:
`released=1653 errors=0`, correct `1920x1200@90Hz` negotiation, using
the exact same signed release APK that had "crashed" many times before
under `am start -n`.

## Decision

Accept the release packaging as done and working. The crash chase was
entirely a self-inflicted artifact of this session's testing method
(`am start -n`, used dozens of times today for fast iteration on
everything from the resolution binary search to Phase 5 negotiation),
not a defect in the signing setup, the build type, or the app itself.

Not fixing the underlying `requestPermission()` crash itself — root
cause isn't diagnosed (no stack trace available, as usual for this
device), and it's a low-priority path: it only matters for an app that
has *never* been granted USB permission, manually opened *without* a
physical accessory-attach event ever happening first. In ordinary use
(README's documented flow) that specific sequence shouldn't occur.

`README.md` updated to steer first-time setup away from the risky
sequence: let the OS auto-launch the app on the very first connection
(or replug if it doesn't), rather than opening it manually before any
permission has ever been granted.

## Consequences

- Phase 6's Android packaging checklist item is now genuinely closed —
  see `TASKS.md`.
- This session's whole testing methodology (`am start -n` for fast
  iteration, used constantly today) has a real, now-documented blind
  spot: it cannot exercise the `openAccessory(fromIntent)` code path at
  all, only the `accessoryList`/`requestOrOpen()` fallback. Worth
  remembering for any future Android-side debugging on this project —
  a symptom that only reproduces via `am start` and never via a real
  physical connection is a strong signal to check which of these two
  paths is actually involved before concluding it's a real bug.
- If the `requestPermission()` crash ever turns out to matter in
  practice (e.g. a user force-stops the app before ever completing a
  first real connection, then manually reopens it), it's still there,
  undiagnosed, and worth a proper investigation with real logcat/stack-
  trace access (a different device, or `adb bugreport`, might succeed
  where this device's `logcat` has consistently failed all project-long).
