---
name: Bug report
about: Something isn't working
title: ""
labels: bug
assignees: ""
---

**Describe the bug**
A clear description of what went wrong.

**Environment (please complete)**

- Host distro/version:
- Desktop session: X11 or Wayland? (tabscreen only supports X11 — see `CLAUDE.md`)
- GPU + driver (e.g. NVIDIA 550, Intel iGPU, AMD):
- Tablet model:
- tabscreen version/commit:

**Steps to reproduce**
1.
2.
3.

**Expected behavior**
What you expected to happen instead.

**Logs**
Relevant output from `journalctl --user -u tabscreen.service` (host) and/or
on-device status (Android — `logcat` is known-unreliable on some builds,
see `README.md`'s Troubleshooting section).

**Additional context**
Anything else relevant — e.g. whether you were reconfiguring monitors in
arandr when it happened (see `docs/adr/0034`), multi-GPU setup, etc.
