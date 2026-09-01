---
name: Feature request
about: Suggest an idea for tabscreen
title: ""
labels: enhancement
assignees: ""
---

**What problem does this solve?**
A clear description of the use case or limitation you're hitting.

**Proposed solution**
What you'd like to see happen.

**Have you checked `CLAUDE.md` and `docs/adr/`?**
tabscreen has several hard architectural constraints (no network
transport, real X11 RandR output, hardware video encoding) and a long
list of approaches already tried and reverted. If your proposal touches
the transport layer, the virtual display mechanism, or the video
pipeline, please note which ADRs you've reviewed.

**Alternatives considered**
Any other approaches you thought about and why you didn't propose them.
