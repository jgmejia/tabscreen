# tabscreen wire protocol (v1)

This is the application-level framing used over the raw AOA bulk pipe
(`CLAUDE.md`'s No-Network Decision — no IP, no sockets, just USB bulk
transfers). It exists because USB transfer boundaries can't be trusted as
message boundaries: large payloads (encoded video frames) may span multiple
underlying transfers, so every message is self-describing.

Implemented independently on both ends: `host/` (Rust, per
[ADR-0001](adr/0001-host-daemon-language.md)) and the Android client
(Kotlin, per [ADR-0002](adr/0002-android-client-language.md)). Keep both
implementations in sync with this document — it's the shared contract.

## Frame format

```
+----------------+----------+------------------------+
| length (u32LE) | channel  | payload (`length` bytes) |
|    4 bytes     | 1 byte   |                        |
+----------------+----------+------------------------+
```

- `length`: payload length in bytes, **not** including the 5-byte header.
  Max accepted value: 4 MiB (`MAX_FRAME_LEN`) — a defensive ceiling against
  a corrupt/malicious length field causing runaway allocation; real video
  frames are expected far below this.
- `channel`: single byte, see below.
- `payload`: exactly `length` bytes, meaning defined per-channel.

Multi-byte integers are little-endian throughout.

## Channels

| ID | Name | Purpose | Payload defined in |
|---|---|---|---|
| 0 | `CONTROL` | Handshake, capability/mode negotiation, connection lifecycle | This document (below) + Phase 5 additions |
| 1 | `VIDEO` | Encoded video frames | Phase 3 |
| 2 | `INPUT` | Touch/stylus input events | Phase 4 |

Unknown channel IDs should be logged and the frame discarded, not treated
as fatal — forward compatibility for future channels without a hard
version bump.

## CONTROL channel messages (Phase 1 scope)

Each `CONTROL` payload starts with a 1-byte message type, followed by a
type-specific body. This is intentionally minimal for Phase 1 — just enough
to prove a real structured round trip over the frame format, replacing
Phase 0's raw-byte echo spike. Later phases add message types here (Phase 5
mode negotiation, connection-quality signaling, etc.) rather than inventing
a new channel.

| Type | Name | Direction | Body |
|---|---|---|---|
| 0x01 | `HELLO` | host → client | `protocol_version: u8` |
| 0x02 | `HELLO_ACK` | client → host | `protocol_version: u8` |

Sequence: once the AOA bulk pipe is open (host handshake complete, Android
app has claimed the accessory), the host sends `HELLO` with
`protocol_version = 1`. The client responds `HELLO_ACK` with the version it
supports. A version mismatch is logged, not treated as fatal in v1 (only
one version exists) — future versions should define real negotiation
behavior here rather than assuming this stays trivial forever.

## VIDEO channel messages (Phase 3 scope)

Host → client only. Unlike `CONTROL`/`INPUT`, there's no message-type byte
here — every `VIDEO` payload is exactly **one Annex-B H.264 NAL unit**,
start code included (either the 3-byte `00 00 01` or 4-byte `00 00 00 01`
form, whichever the encoder emitted), in encode order: parameter sets
(SPS/PPS) first, then each frame's slice NAL(s). One NAL unit per frame
keeps the client's decoder feed aligned to NAL boundaries without needing
to buffer/re-split an arbitrary byte stream itself — `MediaCodec` on
Android is fed one `queueInputBuffer` call per received `VIDEO` frame.

Host-side capture target: until Phase 2 lands, there's no dummy RandR
output to target, so v1 captures the **current default X11 screen**
instead (`host/src/video.rs`) — the same placeholder-target approach
`host/src/input.rs` already uses for pointer injection, for the same
reason (ADR 0006). Encoding is hardware-accelerated H.264 via VAAPI on the
Intel iGPU, matching the codec baseline in
[ADR-0003](adr/0003-video-codec-baseline.md) and the empirical VAAPI
validation in [ADR-0004](adr/0004-target-scope-v1.md). v1 shells out to
`ffmpeg` (`-f x11grab` capture piped into `-c:v h264_vaapi` encode, raw
Annex-B `-f h264` output) rather than reimplementing X11 capture and VAAPI
encoding directly in Rust — Phase 0 already validated this exact pipeline
shape empirically against this hardware, so re-deriving it in raw
Rust/libva bindings would be substantial, avoidable engineering for v1.

## INPUT channel messages (Phase 4 scope)

Client → host only. Each `INPUT` payload starts with a 1-byte message type,
same pattern as `CONTROL`, for the same forward-compatibility reason (room
for e.g. key/scroll events later without a new channel).

| Type | Name | Body |
|---|---|---|
| 0x01 | `TOUCH` | `action: u8, pointer_id: u8, x: f32LE, y: f32LE, pressure: f32LE` (15 bytes) |

- `action`: `0` = down, `1` = move, `2` = up, `3` = cancel (mirrors Android's
  `MotionEvent` action set, minus the multi-pointer index/mask bits — v1
  tracks one active pointer per `pointer_id`).
- `pointer_id`: reserved for multi-touch; v1's client only ever sends `0`
  (the primary pointer), but the field exists now so a multi-touch client
  update doesn't need a wire-format change later.
- `x`, `y`: **normalized 0.0-1.0** relative to the tablet's reported screen
  dimensions, not raw pixels. This is deliberate: the host maps normalized
  coordinates onto whatever the target output's actual pixel rect is at
  injection time, so the client never needs to know the host's screen
  resolution, and the mapping keeps working across arandr
  position/rotation/resolution changes (Phase 5) without a protocol change.
- `pressure`: normalized 0.0-1.0. Plain touch reports `1.0`; a stylus (if
  the tablet supports one) reports its real pressure value. `0.0` on `up`/
  `cancel`.

Host-side injection target: until Phase 2 lands, there's no dummy output to
map onto, so v1's host injects against the current default screen instead
(see `host/src/input.rs`) — the normalized-coordinate design means this
becomes "target the dummy output's rect instead of the default screen's"
with no wire-format change once Phase 2 unblocks.

## Design notes / consequences

- No checksum/CRC on top of the frame: USB bulk transfers already have
  hardware-level CRC and retry at the link layer, so an application-level
  checksum would be redundant for corruption detection. This does **not**
  cover application-level bugs (e.g. a truncated write) — the length prefix
  catches truncation by making a short read detectable (parser should
  treat a closed stream mid-frame as a connection error, not silently
  return a partial message).
- No sequence numbers in v1: the transport is a single ordered bulk pipe,
  not a network link with reordering/loss, so frames arrive in send order.
  If multiplexing ever needs interleaving guarantees beyond "one frame
  fully before the next," revisit this.
- Endianness/format choices favor simplicity over compactness — this
  protocol runs over USB with confirmed comfortable throughput headroom
  (Phase 0: 20.7 MB/s vs. ~25 Mbps needed), so a few extra header bytes
  are not worth optimizing away.
