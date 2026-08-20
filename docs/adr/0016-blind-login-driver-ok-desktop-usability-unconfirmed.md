# 16. Xorg-level config confirmed correct; desktop-level usability NOT confirmed

Date: 2026-08-19

## Status

**Revised, downgraded from an earlier "Accepted" claim in this same entry.**
The first version of this ADR claimed the blind-login test fully resolved
0013/0015's conflict, based on Xorg's own driver log showing `DFP-1`
correctly disconnected for 13 minutes. That claim was premature — see
"What this entry got wrong" below. Xorg-level configuration is confirmed
correct; whether the user actually had a usable, visible desktop during
that session is genuinely unconfirmed, and the user's own direct report
("no luck") should carry more weight than a log inference. Do not treat
Phase 2 as unblocked on the strength of this entry alone.

## What this entry got wrong

The original version of this ADR reported the blind-login session (`17:53`
to `18:06`, boot `-1`) as a confirmed success, citing:
- Xorg's own log (`gdm-x-session[4630]`) showing `DFP-1: disconnected`
  consistently for 13 minutes, both real monitors configured correctly.
- A clean process exit at the end.

This is real and still true — but it only proves the **driver-level**
configuration was correct, not that the **desktop was actually usable**.
Checking `gnome-shell[4860]`'s own log (the real compositor, not Xorg)
revealed a much messier picture:
- Clutter rendering assertion warnings within the first 10 seconds
  (`tried to allocate a size of -2147483648.00 x -2147483648.00`).
- **A 6-minute-40-second gap with no gnome-shell log activity at all**,
  before "Launching DING process" (desktop icons — normally happens within
  seconds of login, not minutes) finally appears at `17:59:55`.
- Broader journal activity *during* that gap includes real signs of life
  (a `terminator` terminal process launched ~20s after login; a `pkexec`
  action run by the user at `17:54:09`) — but this doesn't resolve the
  ambiguity, because it's not established whose activity this was. Notably,
  the AI assistant's own shell/tool access to this machine remained
  continuously working across every reboot this session, suggesting that
  access path may be independent of whether the human's own GDM/GNOME
  session was visibly rendering anything — so process activity in the logs
  cannot be assumed to reflect what the human actually saw on screen.

**The user's direct, first-hand report was "tried multiple times logging in
to no luck, then fallback."** That is a real, trustworthy signal that was
initially overridden by a log-based inference — the wrong call. A
6+ minute stall before the desktop becomes interactive is entirely
consistent with what the user described, not with a clean success.

## Context

0015 proposed logging in blind through the broken greeter (rather than
falling back to VT-switch immediately) to check whether a real, logged-in
session — as opposed to the greeter — correctly respects the `<disabled>`
`monitors.xml` entry for the virtual output. The user tried this and
reported "no luck" before falling back.

**What `journalctl -b -1` actually establishes, and no more:**

- `17:53:03` — PAM authentication succeeded, a session opened for the real
  user (`gdm-password[4587]`).
- A new Xorg process (`gdm-x-session[4630]`) started for that session and,
  at the driver level, correctly kept `DFP-1` disconnected
  (`(II) NVIDIA(G0): Setting mode "NULL"`) while configuring `eDP-1` and
  `HDMI-1-0` normally. This state persisted for ~13 minutes, ending in a
  clean process exit — not a crash.
- Separately, `gnome-shell[4860]` (the actual desktop compositor for that
  session) logged rendering assertion warnings within 10 seconds, then had
  no further log output for 6 minutes 40 seconds before desktop icons
  ("DING") finally launched — a strong signal the desktop was not
  promptly interactive, consistent with the user's own report.
- Broader activity during that gap (a `terminator` process launch, a
  `pkexec` action) doesn't resolve whether the user could actually see and
  use the desktop, for the reasons in "What this entry got wrong" above.

**Net: the Xorg/NVIDIA driver-level configuration (the specific thing 0008
through 0015 were trying to get right) is confirmed correct. Whether the
GNOME desktop built on top of it was actually usable during this session
is not established either way.** This entry does not resolve 0013/0015's
conflict; it narrows what's still unknown.

## Decision

Do not mark Phase 2's mechanism as confirmed. Before any further claim of
resolution:

- Retest with the user directly observing and reporting, in real time,
  whether the desktop becomes visible and usable after blind login —
  rather than reconstructing the answer from logs after the fact.
- If it's usable but slow (matching the 6m40s gap seen here), investigate
  why `gnome-shell` stalls that long before rendering the desktop with
  this config active — that stall is itself a real problem worth
  understanding even if the underlying output state is correct, since a
  6+ minute hang on every login is not acceptable regardless of what
  Xorg's driver log says.
- If it's genuinely not usable at all, 0013's architectural conflict is
  still open and the `monitors.xml` approach (0014) has not actually
  fixed it — back to considering 0013's other candidate fixes (genuinely
  dynamic `ConnectedMonitor` application at tablet-connect time, or the
  scope-reduction option).

## Consequences

- The Xorg-level config (`ConnectedMonitor`/`CustomEDID`/`MetaModes`) and
  the `monitors.xml` `<disabled>` entry remain good, reusable building
  blocks — nothing here invalidates that work. What's unconfirmed is
  specifically whether they add up to a usable desktop experience.
- This session has now had seven live-hardware attempts (0006, 0008, 0011,
  0012, 0013, 0015, and this one), several requiring recovery. Given the
  pattern of this entry's own premature conclusion, future ADRs in this
  series should distinguish explicitly between "driver/Xorg-level log
  confirms X" and "the user directly observed X" — they are not
  interchangeable evidence, and this entry conflated them once already.
- Do not update `TASKS.md` to mark Phase 2's core mechanism as resolved
  based on this entry.
