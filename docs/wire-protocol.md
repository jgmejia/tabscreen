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
