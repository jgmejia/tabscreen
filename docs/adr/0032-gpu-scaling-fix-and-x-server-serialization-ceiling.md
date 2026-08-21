# 32. GPU-accelerated scaling cuts ffmpeg CPU ~4.5x; remaining lag is the X server's own request-serialization cost, not a missing MIT-SHM fast path

Date: 2026-08-20

## Status

Accepted.

## Context

After ADR 0031 reverted `FRAMERATE` from 90 back down through 60 to 30,
live use still showed the user real, direct symptoms: sporadic visual
corruption on the *real* monitors (not just the tablet — captured in
`glitches.png`) and system-wide keyboard input lag while typing in a
plain terminal. `ffmpeg` measured at 158% CPU sustained at 60fps, later
99% even at 30fps — both far above what Phase 0 had established VAAPI
encode itself costs (16.4x realtime, i.e. cheap).

**Root cause 1 — CPU-side scaling.** `spawn_encoder()`'s `-vf` chain was
`scale=W:H,format=nv12,hwupload`: every captured frame was scaled and
color-converted on the CPU *before* being uploaded to the GPU for
encode. A controlled standalone A/B test (two separate `ffmpeg`
background processes, not the daemon) isolated this precisely: the old
CPU-scale chain measured 69.0% CPU, an otherwise-identical chain that
uploads the raw frame first and scales on the GPU
(`hwupload,scale_vaapi=w=W:h=H:format=nv12`) measured 15.2% CPU — a
~4.5x reduction for identical output. Deployed live: `ffmpeg` dropped
from 99% to **17.0% CPU**, decode stayed clean
(`released=90773 errors=0`). The visual glitches on the real monitors
disappeared entirely after this fix and have not recurred — consistent
with the working theory that they were caused by the compositor being
CPU/memory-bandwidth-starved under `ffmpeg`'s old scaling load, not a
genuine GPU/driver fault (dmesg and thermal sensors were clean
throughout, ruling that out separately).

**The keyboard lag did not improve** ("hmmm, not a real improvement,
I can see glitches but, in this case, they are less than in our
previous setup" — before this GPU-scaling fix even landed, referring
only to the framerate cut; after this fix, "It feels the same, however
I don't see anymore glitches."). This pointed at a second, independent
bottleneck.

**Investigated — RAM/swap ruled out.** `free -h` showed 13Gi available
out of 31Gi total; `swapon --show` showed 908Ki used out of 8.0Gi swap.
Not swapping at all; not a factor.

**Investigated — root cause 2, confirmed live: `Xorg`'s own main
event-processing thread.** `top -H -b -n1 -p <xorg-pid>` showed Xorg's
main thread at 40-50% CPU, independent of `ffmpeg`'s own CPU usage
(unaffected by the scaling fix above). Xorg is single-threaded for
input/request processing (verified via the same `top -H` view — one
thread carries this load, unlike `ffmpeg`'s multi-threaded encode).
32 physical cores don't help here: this is a serialization bottleneck
on one thread's request-processing loop, not a total-CPU-capacity
problem — directly addressing the user's own correct pushback ("why
would 158% CPU affect my computer... we have 32 cores").

**Checked whether this was actually the *slow* `XGetImage` capture path
(no MIT-SHM) rather than genuine per-request overhead** — this was
ADR 0031's still-open question. Confirmed **MIT-SHM is active, not the
problem**:
- `xdpyinfo -ext all` shows the X server advertises `MIT-SHM version
  1.2`, shared pixmaps supported.
- `ffmpeg`'s own `/proc/<pid>/maps` shows 3 attached SysV shared-memory
  segments (`/SYSV00000000 (deleted)`, the standard anonymous-key
  pattern for X shm), each 9,216,064 bytes — matching
  `1920×1200×4` bytes (the exact capture rect's BGRA frame size)
  almost exactly, with `libxcb-shm.so.0` actually mapped into the
  process, not just linked. Three buffers matches `xcbgrab`'s expected
  triple-buffering pattern.

So the fast path was already in use the whole time. The X server's
per-request cost is inherent to servicing one `shm_get_image` request
per captured frame — tied to **capture frequency**, not to any missing
optimization.

## Decision

1. Keep the `scale_vaapi` GPU-scaling fix — clear win, no downside
   found, already deployed and verified stable.
2. Keep `FRAMERATE` at 30 (not revisit raising it) — the user explicitly
   chose to hold here once it was clear the remaining lag is tied to
   request frequency, not a fixable inefficiency, and a further empirical
   framerate-vs-lag test was explicitly declined ("No, let's keep it at
   30.").
3. Do not pursue a non-X11-protocol capture rewrite (e.g. reading the
   `vkms` framebuffer directly via DRM/KMS) at this time — a real
   architecture change, not a tuning knob, and out of scope for the
   "working state ASAP" priority the user set earlier this session.

## Consequences

- `ffmpeg`'s own CPU footprint is now proportionate to its real workload
  (~17% at 30fps) rather than dominated by avoidable CPU-side image
  processing — a genuine, unconditional improvement (lower system-wide
  load, less battery/thermal impact) independent of the lag question.
- The visual glitching on real monitors is resolved as a side effect of
  reducing CPU/memory-bandwidth contention, without directly targeting
  it.
- The remaining keyboard-lag sensation is now understood, not just
  reduced: it's Xorg's own single-threaded request-processing cost,
  bounded by capture frequency (30 requests/sec), and is not fixable by
  more CPU cores, GPU offload, or MIT-SHM tuning — those levers are all
  now exhausted. The only remaining levers are (a) lowering capture
  frequency further (direct, but reduces stream smoothness, and was
  explicitly declined for now) or (b) a genuinely different capture
  mechanism that doesn't route through X11's request protocol at all
  (a real rewrite, not attempted).
- `docs/adr/0031`'s still-open MIT-SHM question is now closed: MIT-SHM
  is confirmed active. `x11grab`'s real sustained capture-rate ceiling
  on this hardware is still not separately measured, but is no longer
  the live concern — the ceiling in practice is Xorg's request-servicing
  cost, already observed directly.
