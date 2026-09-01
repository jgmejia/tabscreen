# Contributing to tabscreen

Thanks for your interest in tabscreen. Before diving in, please read
[`CLAUDE.md`](CLAUDE.md) — it documents the hard constraints this project
is built around (no network transport, real X11 RandR output, not a
window) and *why* they exist. Contributions that reintroduce something
those constraints already ruled out (RNDIS/`adb reverse`, an app-level
mirror window, VNC-style framebuffer diffing) will be redirected there
first.

[`TASKS.md`](TASKS.md) tracks current status phase by phase, and
[`docs/adr/`](docs/adr/) has the full decision history, including several
approaches that were tried and reverted — worth checking before
re-proposing one of them.

## Current state

tabscreen is a v1 validated against exactly one hardware pairing (see
`README.md`'s "Known limitations"). The most valuable contributions right
now are:

- Testing against different tablets/GPUs and reporting what breaks
- Reliability work tracked in `TASKS.md`'s Phase 7 (decode-error handling,
  session logging, multi-tablet support)
- Packaging/install-path improvements for distros other than the one this
  was built on

## Reporting bugs

Open an issue using the bug report template. Include your GPU (and
driver: NVIDIA/Intel/AMD), tablet model, and Linux/X11 distro version —
this project's failure modes have consistently turned out to be
GPU/driver-topology-specific (see `docs/adr/0019`, `0021`, `0034`), so
that context matters more here than in most projects.

## Submitting changes

1. Open an issue first for anything beyond a small fix, so we can agree
   on approach before you invest time — especially for anything touching
   the transport layer or the RandR integration, where a design that
   looks simpler often reintroduces a constraint this project already
   worked around.
2. Keep the host (Rust) and Android client changes in separate commits
   where practical.
3. For a change that reverses or revises a prior decision, add a new ADR
   under `docs/adr/` rather than editing history — this project's own
   history (see ADR 0031 reverting ADR 0030) is a record of what was
   tried, not just what's currently true.
4. Run `cargo test` (host) and the Android unit tests before opening a
   PR.

## Development setup

See `README.md`'s Prerequisites and Installing sections — there's no
separate dev-only setup.
