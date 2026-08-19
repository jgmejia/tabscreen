# 3. Video codec baseline: H.264, with AV1 (not H.265) as the upgrade path

Date: 2026-08-18

## Status

Accepted

## Context

tabscreen's transport is raw USB/AOA with no TCP/IP (see `CLAUDE.md`'s
No-Network Decision), and Phase 0 of `TASKS.md` explicitly flags AOA
throughput as an unmeasured, likely-lower-than-RNDIS ceiling. Codec choice
has to balance three things: hardware encode coverage on the Linux host,
hardware decode coverage across Android tablets, and compression efficiency
against that throughput ceiling — plus, since tabscreen's eventual license/
distribution model isn't decided yet, codec patent-licensing exposure.

## Decision

**Baseline on H.264** for the initial working pipeline (Phases 3–5 in
`TASKS.md`). Do not stack an unproven codec on top of the other unresolved
unknowns (AOA throughput, dummy-driver behavior) while first getting the
pipeline working end-to-end.

**When compression efficiency becomes necessary** (i.e. Phase 0's throughput
measurement shows H.264 can't hit an acceptable resolution/frame-rate
combination), **upgrade to AV1, not H.265**.

### Why H.264 as the baseline, independent of licensing

- Hardware H.264 decode has existed on essentially every Android device
  since ~2011, and hardware H.264 encode has existed on essentially every
  Linux-capable GPU since ~2011–2012 (Intel Quick Sync from Sandy Bridge,
  AMD VCE, Nvidia NVENC from Kepler). It's about as safe a hardware-support
  bet as exists on either end of the pipeline.
- Low-latency tuning for H.264 is extremely well-trodden — this is why
  Moonlight/Sunshine, Steam Link, and Parsec all default to it for
  interactive streaming. Not because other codecs can't do low latency, but
  because H.264's real-time latency behavior is the most consistently
  implemented across vendor hardware, which matters given `TASKS.md`'s AOA
  throughput ceiling is still unmeasured — H.264 keeps that one variable
  well-understood while other unknowns get resolved.

### Why AV1, not H.265, as the upgrade path — and why this isn't a "safe vs. risky codec" framing

It would be inaccurate to frame this as "H.264 has no licensing cost, H.265
does" — both codecs carry patent licensing; H.264's MPEG LA pool has just
been running since 2003 and is well-settled. What actually distinguishes
H.265 is *fragmentation*: instead of one pool there are several overlapping/
competing ones (MPEG LA, Access Advance, Velos Media) plus unpooled holders,
and they don't always agree with each other. That fragmentation — not the
mere existence of licensing — is the documented reason HEVC adoption stalled
industry-wide, and is literally why AOMedia formed AV1 in the first place.

It's also worth being explicit that tabscreen doesn't ship its own
encoder/decoder — it calls the OS/silicon-provided one (VAAPI on the host,
`MediaCodec` on the tablet). The patent obligation for *implementing* a
codec falls mainly on whoever ships the actual encode/decode silicon or
software (GPU vendors, Google/OEMs), who've presumably cleared that for both
codecs already. So tabscreen's direct legal exposure from merely *calling* a
hardware HEVC path is probably low regardless — but that reasoning applies
equally to H.264, so it doesn't distinguish the two on its own.

The actual reason to prefer AV1 over H.265 as the upgrade path: there is no
upside to accepting HEVC's licensing ambiguity when AV1 delivers the same
compression-efficiency improvement over H.264 with a clean, unambiguous
royalty-free story, for close to zero extra cost. If AV1 didn't exist,
HEVC's fragmentation would be a real but probably-tolerable risk worth
accepting for the compression gain. Since AV1 does exist, taking on HEVC's
ambiguity buys nothing. AV1's actual downside is weaker host-side hardware
*encode* coverage today — **corrected, see empirical note below**: Intel
AV1 hardware encode requires Meteor Lake's Xe-LPG or Arc discrete GPUs, not
simply "12th-gen+" as originally written here; AMD RDNA3+ and Nvidia
40-series+ remain accurate. This is a real, practical cost, unlike the
licensing question.

### Correction from empirical testing (2026-08-18)

[0004](0004-target-scope-v1.md) tested actual VAAPI encode against this
project's real reference hardware (Intel Raptor Lake-S iGPU): H.264 and
H.265 hardware encode both work, but **AV1 hardware encode fails outright**
(`No usable encoding entrypoint found for profile VAProfileAV1Profile0`).
Raptor Lake's iGPU is Gen12/Xe-LP generation — it has AV1 *decode* but not
*encode*; this ADR's original "12th-gen+" framing conflated the two and was
wrong.

**Update, tablet side confirmed too:** reading the Snapdragon 680's actual
vendor codec declarations (via `adb`, see
[0004](0004-target-scope-v1.md#confirmed-tablet-codec-hardware-support-empirical-2026-08-18))
shows only a software CPU fallback AV1 decoder (`c2.android.av1.decoder`,
`variant="slow-cpu"`) — no Qualcomm hardware AV1 decoder exists on this
chipset. This is no longer a "doubtful, needs verification" risk — it's
confirmed. **AV1 is not viable on tabscreen's v1 host+tablet pairing on
either end.** See [0004](0004-target-scope-v1.md) for the realistic
fallback options (harder H.264 tuning, or accepting a cross-GPU NVENC copy
cost) if H.264 throughput ever proves insufficient. This ADR's codec
*preference ordering* (H.264 baseline, AV1 over H.265 if an upgrade is ever
needed and the hardware changes) still holds as a principle — what's now
closed off is that the upgrade is available on *this* hardware today.

The same tablet-side data confirms the H.264 baseline strongly: genuine
Qualcomm hardware H.264 decode, including a dedicated
`c2.qti.avc.decoder.low_latency` variant — real hardware support for
exactly the low-latency use case this ADR argued H.264 was safest for.

## Consequences

- Phase 0's AOA throughput measurement is now the explicit trigger for
  deciding whether the AV1 upgrade path is needed — that data should be
  captured and referenced when making the call, not decided speculatively.
- H.265 is intentionally excluded from tabscreen's roadmap; if it's
  reconsidered later, that reversal should get its own ADR explaining what
  changed (e.g. a licensing arrangement, or AV1 hardware encode remaining
  inadequate on target host hardware).
- The host encode pipeline ([0001](0001-host-daemon-language.md)'s
  `ffmpeg`/VAAPI FFI layer) should be structured so swapping the codec later
  doesn't require restructuring the capture/transport code around it.
