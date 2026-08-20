# 17. Correction: "session 2" never had the config active — real picture is one clean repro of the old bug

Date: 2026-08-19

## Status

**Revised.** The first version of this ADR claimed the outcome was
non-deterministic *within the same boot, same config* — comparing two
login sessions where one showed `DFP-1` connected and the other showed it
disconnected. That comparison was invalid: the "disconnected" session's
Xorg log has **zero** `ConnectedMonitor`/`CustomEDID` option lines, proving
the config had already been removed before that login happened. This ADR
corrects the record. See "What this entry got wrong" below — this is the
second time in this investigation an ADR asserted something from
incomplete log analysis (0016 did the same with "the session worked");
that pattern itself is worth naming plainly rather than repeating quietly.

## What this entry got wrong

Original claim: two real logins in the same boot, `18:39:57` (session 1)
and `18:54:50` (session 2), showed opposite `DFP-1` states with "no config
changes between them." The second half of that claim is false. Checking
`sudo` history in the same boot:

```
18:54:37 - sudo rm /etc/X11/xorg.conf.d/90-tabscreen-nvidia-edid.conf  (from tty3)
18:54:40 - sudo systemctl restart gdm
18:54:41 - new greeter Xorg starts (config already gone)
18:54:50 - session 2's login succeeds
```

The user had already switched to a text console (`tty3`, confirmed by a
`login[6018]` PAM session at `18:54:20`), removed the Xorg config, and
restarted GDM — all *before* session 2's login. Session 2's Xorg log
confirms this: no `ConnectedMonitor`/`CustomEDID` option lines at all.
`DFP-1` showed disconnected in session 2 for the mundane reason that
nothing was overriding its detection anymore — not because `monitors.xml`
correctly disabled it. This was a real analysis error: the timeline should
have been checked before drawing a conclusion from the comparison.

## Context, corrected

Removing the invalid comparison, there is exactly **one genuine trial from
this boot** with the config actually active: session 1 (`18:39:57`, Xorg
PID `3663`). Its result: `(--) NVIDIA(GPU-0): TBS TabScreen (DFP-1):
connected` — the same broken pattern seen in every prior greeter failure
(0012, 0013, 0015). `gnome-shell` for that session did start fast
("Launching DING process" the same second as shell startup) — but with
`DFP-1` wrongly active, this session likely reproduces the same
wrong-output-rendering problem 0012/0013 diagnosed, not a new issue.

That makes the full genuine-trial count, across all boots today, **two**:
- 0016's original session (boot `-1`, `17:53`): `DFP-1` correctly
  disconnected, but `gnome-shell` stalled 6m40s before becoming
  interactive.
- Today's session 1 (boot `0`, `18:39:57`): `DFP-1` wrongly connected,
  `gnome-shell` started fast.

Two trials, two different failure modes, neither one fully clean. This is
a smaller and more mundane finding than "non-deterministic" — it's
consistent with the `monitors.xml` `<disabled>` mechanism simply not
reliably working (matching its track record: it never definitively
succeeded even once with independent confirmation), rather than some
exotic race condition. The apparent "success" in 0016 may itself have been
a fluke, or `monitors.xml` may only intermittently get read before Xorg's
own `ConnectedMonitor` detection locks in — genuinely not yet known, but
not established as a *race* either; that was over-interpretation of a
comparison that turned out to be invalid.

## Decision

- Retract the "non-determinism within a boot" framing. It's not supported.
- What's actually established: `monitors.xml`'s `<disabled>` entry has
  produced the correct driver-level state **once** (0016) and the broken
  state **once** (today), across two genuine trials. That's too little
  data to characterize as anything more precise than "unreliable."
- Before any further live attempt, this needs a cleaner experimental
  protocol: multiple reboots, config verified present via the Xorg log
  each time (not inferred), and no fallback actions taken between the
  observation and the log check — to avoid another false comparison like
  this one.
- Process note for future ADRs in this series: always check whether a
  fallback/recovery action (file deletion, service restart) occurred
  between two observations before comparing them. This is the second
  overclaim this investigation has produced from skipping that check.

## Consequences

- Phase 2 remains blocked. The honest state: the `ConnectedMonitor`/
  `CustomEDID`/`MetaModes` Xorg-level mechanism is confirmed capable of
  correctly excluding `DFP-1` at the driver level (0012 first showed this,
  0016 confirmed it again) — but `monitors.xml`'s `<disabled>` entry, meant
  to make that the *default* state, has an unreliable track record (1 for
  2) and gnome-shell's startup delay is unexplained and highly variable
  (near-instant to 6m40s) independent of that.
- Given the volume of live-hardware attempts today and two consecutive
  analysis corrections, the next step should not be another blind reboot.
  It should be deciding, with the user, whether to invest in a cleaner
  test protocol, revisit 0013's alternative fixes, or pause here.
