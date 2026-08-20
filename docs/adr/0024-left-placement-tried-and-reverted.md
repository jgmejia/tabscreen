# 24. Left-side placement tried live, reverted after two layout disruptions

Date: 2026-08-20

## Status

Accepted (reverted to `Placement::Right`, the pre-existing behavior).
`Placement::Left` stays in `display.rs` as documented, known-broken code,
not deleted — see Decision.

## Context

User preference: place the tabscreen output to the *left* of the existing
monitors rather than the right. Confirmed live that X11/RandR's screen
origin can't go negative — `xrandr --output Virtual-2-1 --pos -1200x0`
silently clamps to `0x0`, overlapping whatever's already there rather
than sitting left of it. The only way to actually achieve left placement
is to shift the real outputs (`eDP-1`, `HDMI-1-0`) right by the tabscreen
output's width first, then place the tabscreen output at the resulting
leftmost `x`.

Implemented in `display.rs`: `Placement::Left` queries every active real
output's current position, issues one `xrandr --output <name> --pos
<x>x<y>` call *per output* to shift each right, then activates the
tabscreen output at the freed-up leftmost position. `ActiveOutput::Drop`
restores each shifted output to its recorded original position before
turning the tabscreen output off.

**Live-tested in isolation first** (a throwaway `#[ignore]`d test, same
pattern as ADR 0022): activated with `Placement::Left`, logged
`shifted: [("eDP-1", 0, 0), ("HDMI-1-0", 2560, 0)]`, waited 3s, dropped,
confirmed via `xrandr --query` afterward that both outputs were back at
their original positions. **This test had a blind spot, found only after
the fact**: it checked state *before* and *after*, never *during* --
so it never actually confirmed `eDP-1` moved while the placement was
active, only that it ended up back where it started. Since it started
and ended at the same place either way, a `eDP-1` move that silently no-
opped the whole time would have looked identical to a real, working
shift-and-restore in this test's output.

**Live-tested via the real daemon (user's session):** broke visibly.
Actual resulting state: `eDP-1` stayed at `2560x1600+0+0` (never moved),
`Virtual-2-1` activated at `1920x1200+0+0` (directly overlapping
`eDP-1`), `HDMI-1-0` shifted correctly to `+4480+0`. Symptoms reported by
the user: mouse pointer confined to `eDP-1`'s region, the tablet showing
`eDP-1`'s content instead of its own assigned region, and degraded
keyboard input latency in an unrelated terminal -- the last one traced to
`ffmpeg` still capturing at ~94% CPU against the now-meaningless overlap
region, not a direct RandR effect.

**Recovery:** immediate, live, non-destructive -- `xrandr --output
Virtual-2-1 --off` plus restoring `HDMI-1-0`'s position, then the user
`Ctrl+C`'d the daemon to stop the runaway `ffmpeg` process. Full desktop
function confirmed back to normal within a couple of commands, no reboot
or session restart needed (unlike ADR 0005's incident).

**Root-caused afterward**, isolated and safe (no tablet/daemon involved,
just direct `xrandr` calls against the live desktop with immediate
before/during/after checks each time):

1. `xrandr --output eDP-1 --pos 1920x0` alone: **zero effect**, instantly
   -- `eDP-1` never moved, confirmed with no delay between the command and
   the check (rules out "mutter reverts it after a moment").
2. `xrandr --verbose --output eDP-1 --pos 1920x0`: the verbose per-CRTC
   dump showed `eDP-1` at `+1280+0` -- a third, different, unrequested
   position -- inside what looks like a single GPU provider's own local
   coordinate space (this machine has a hybrid Intel+NVIDIA setup with
   RandR provider offload, extensively documented earlier in this
   project's ADR history), not the final merged multi-provider layout.
   The real `xrandr --query` afterward showed `eDP-1` at `+1280+0` *and*
   `HDMI-1-0` moved on its own from `+2560+0` to `+0+0` -- a single
   command targeting only `eDP-1` visibly moved a *different* output too.
3. Conclusion: this isn't "the primary output specifically refuses to
   move" (which would be simple to work around). It's that **GNOME/
   mutter reactively re-lays-out the whole multi-monitor arrangement in
   response to any single `xrandr --pos` change**, and issuing the shifts
   as *separate sequential* `xrandr` invocations (one per output, as
   `Placement::Left`'s implementation does) lets mutter's reactive
   relayout race with and override each intermediate step, landing on an
   unpredictable final layout rather than the simple shift intended.
4. A single **atomic** `xrandr` command covering every `--output`/`--pos`
   pair together (`xrandr --output eDP-1 --pos 0x0 --output HDMI-1-0
   --pos 2560x0`, one invocation) restored both outputs to their correct
   positions cleanly and immediately when used for recovery. This is
   suggestive that the real fix is combining `Placement::Left`'s
   per-output shift calls into one atomic command instead of a loop of
   separate ones -- **but this was only confirmed for a restore back to
   known-good positions, never actually tried for the original shift**,
   given two live disruptions in a row on the user's actual working
   session was enough reason to stop and switch approaches rather than
   attempt a third live test immediately.

## Decision

Reverted `main.rs`'s `PLACEMENT` constant to `Placement::Right` --
proven, has never caused an issue, and (unlike `Left`) never touches
`eDP-1`/`HDMI-1-0` at all, so it has no equivalent failure mode to begin
with. The user's fallback plan (manually repositioning the tabscreen
output further via arandr after it activates on the right) covers their
actual placement preference without needing `Left` to work.

`Placement::Left`'s implementation is **kept in `display.rs`, not
deleted** -- it's a real, mostly-working mechanism (the shift-tracking
and `Drop`-based restore logic are sound; only the sequential-command
ordering is suspect) and the atomic-command fix is a concrete, testable
next step if this is revisited. `main.rs`'s `PLACEMENT` constant comment
documents this status directly so a future session doesn't have to
rediscover it from git history.

## Consequences

- `Placement::Left` must not be flipped back on without first changing
  `activate()` to issue one combined `xrandr` invocation for all the
  shifted outputs (and the tabscreen output's own positioning) instead
  of the current per-output loop, and retesting live with the same
  before/during/after rigor this investigation used -- not just
  before/after, which is what let the original isolated test's blind
  spot through undetected.
- This is at least the second time in this project that verifying a
  live-desktop RandR change only before/after (not during) missed a real
  bug -- worth remembering as a standing testing discipline for any
  future live X11 experiment, not specific to placement.
- The `ffmpeg`-CPU-while-overlapping side effect is a real, if narrow,
  finding on its own: `video.rs`'s encoder has no awareness of whether
  its capture region is actually meaningful (e.g., the tabscreen output
  having been turned off out from under it via an out-of-band `xrandr`
  call, as happened here) and will happily keep encoding garbage/
  irrelevant content at full CPU cost. Not fixed here -- the daemon was
  stopped instead -- but worth a defensive check in a future pass if
  this class of external-interference scenario seems likely to recur.
- No damage to the live session beyond the temporary degraded input
  latency during the ~1-2 minutes the layout was broken; fully recovered
  without a restart, unlike ADR 0005's incident.
