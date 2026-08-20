# 14. `monitors.xml`'s `<disabled>` mechanism: a real candidate, not yet tried

Date: 2026-08-19

## Status

Proposed — research only, no live changes made. Answers the question 0013
left open: can mutter be told "this monitor exists but keep it off,"
separately from what the driver reports as connected.

## Context

0013 found the real problem isn't Xorg/NVIDIA config syntax — it's that
`ConnectedMonitor` tells the whole desktop stack a monitor is present, and
mutter (running the greeter's compositor) reasonably auto-activates
anything it's told is connected. 0013 proposed checking whether mutter's
saved-configuration mechanism (`monitors.xml`) can express "connected but
not activated," since earlier logs already showed mutter looks for one
(`Failed to parse saved session file: ... No such file or directory`,
non-fatal) — suggesting the mechanism exists but isn't populated.

Read mutter's own config-store source directly
(`src/backends/meta-monitor-config-store.c`, official GNOME repo) rather
than relying on secondary writeups, since this is exactly the kind of
claim 0013's "not confirmed either way" needs resolved before another live
attempt.

**Confirmed from source:**

- `<configuration>` in `monitors.xml` recognizes (among others) two
  sibling sections to the normal `<logicalmonitor>` list: **`<disabled>`**
  and `<forlease>`. `<disabled>` is parsed as a list of `<monitorspec>`
  entries — this is a first-class, purpose-built mechanism for "this
  monitor is known, but should not be activated," distinct from simply
  omitting a monitor from the config entirely.
- A `<monitorspec>` identifies a monitor by four child elements:
  `<connector>`, `<vendor>`, `<product>`, `<serial>` — all four required,
  validated via `meta_verify_monitor_spec()`. For our EDID
  (`host/resources/edid/tabscreen-honor-pad-x8a.bin`), this would be
  `connector = DP-1` (the connector-based alias for `DFP-1`, per Appendix
  C of the NVIDIA README — either alias should work, but `monitors.xml`
  conventionally uses the connector-based form), `vendor = TBS`,
  `product`/`serial` matching what `generate_edid.py` encoded (currently
  both `1` — worth double-checking mutter/EDID-parsing renders these as
  strings the same way before relying on an exact match).

**Not yet confirmed, and load-bearing before trying this live:**

- Whether the config-matching logic (`meta_verify_monitors_config` /
  `meta_monitors_config_key_equal`, not present in the file read) requires
  a saved config to account for *every* currently-connected monitor
  (active or disabled) to be considered valid, or accepts a partial
  match. If it requires completeness and our `vendor`/`product`/`serial`
  values don't exactly match what mutter reads back from the live EDID,
  the saved config would fail validation and mutter would fall back to
  auto-configuration — which is the exact behavior we're trying to avoid.
- **Whether GDM's greeter reads a per-session `monitors.xml` the same way
  a real user session does, and from where.** Our actual failures (0012,
  0013) happen at the *greeter* stage, running as the `gdm` system user,
  not the real user's session. The one hint so far: an earlier log showed
  mutter looking for
  `/var/lib/gdm3/.config/mutter/sessions/<id>.ms` — a *session* file, not
  necessarily `monitors.xml` itself, and its exact relationship to
  `monitors.xml`'s `<disabled>` mechanism isn't confirmed. The official
  doc (`doc/monitor-configuration.md`) states system-level config defaults
  to `/etc/xdg/monitors.xml` and user-level to `~/.config/monitors.xml`,
  but doesn't document a GDM-specific path or confirm the greeter honors
  either location the same way a logged-in session does.

## Decision

Not yet decided whether to attempt this live. Two concrete unknowns to
resolve before doing so, in order of how load-bearing they are:

1. Does the greeter (as the `gdm` user) actually consult a `monitors.xml`
   at startup, and if so, which path — `/etc/xdg/monitors.xml` (system,
   would also affect the real user's own session) or something under
   `/var/lib/gdm3/.config/` specific to the `gdm` user? This determines
   whether seeding a file is even structurally capable of affecting the
   greeter at all, versus only affecting the post-login session (which
   didn't fail in 0012/0013 — only the greeter did).
2. Does an incomplete/mismatched saved config fail closed (fall back to
   current, connected-things-get-activated auto-config — the status quo
   problem) or fail some other way? If matching is strict and our EDID's
   `vendor`/`product`/`serial` need to exactly match what mutter reads
   back, this needs verifying (e.g. by reading what a real captured EDID's
   values look like once round-tripped through mutter, if that's
   inspectable without a live attempt).

## Consequences

- This is a substantively different, and plausibly much lower-risk,
  category of change than every previous attempt: `monitors.xml` is a
  plain per-user/per-system config file, not Xorg boot config — mutter is
  documented to fall back gracefully to auto-configuration when a saved
  config doesn't validate, rather than crash or hang. If true, a bad
  attempt here should look like "no improvement" rather than another
  failed/blank greeter requiring VT-switch recovery. This is not yet
  independently confirmed (no source read on the fallback path itself),
  so should still be treated with the same care as prior attempts until
  proven safe in practice.
- If the greeter-specific path question resolves unfavorably (greeter
  doesn't read a seedable `monitors.xml` at all), this whole approach is a
  dead end for the *greeter* problem specifically, though it might still be
  useful for the real user's own post-login session configuration later.
- Existing EDID identity (`vendor "TBS"`, product name "TabScreen") from
  0008 remains reusable regardless of outcome here.
