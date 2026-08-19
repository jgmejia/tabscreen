//! Wire protocol codec — see docs/wire-protocol.md for the spec this implements.
//! Keep this in sync with the Android client's equivalent parser.

use std::collections::VecDeque;

pub const MAX_FRAME_LEN: u32 = 4 * 1024 * 1024;
const HEADER_LEN: usize = 5;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Channel {
    Control,
    Video,
    Input,
    Unknown(u8),
}

impl Channel {
    fn from_byte(b: u8) -> Self {
        match b {
            0 => Channel::Control,
            1 => Channel::Video,
            2 => Channel::Input,
            other => Channel::Unknown(other),
        }
    }

    fn to_byte(self) -> u8 {
        match self {
            Channel::Control => 0,
            Channel::Video => 1,
            Channel::Input => 2,
            Channel::Unknown(b) => b,
        }
    }
}

#[derive(Debug, Clone)]
pub struct Frame {
    pub channel: Channel,
    pub payload: Vec<u8>,
}

/// Encodes a single frame ready to write to the bulk OUT endpoint.
pub fn encode_frame(channel: Channel, payload: &[u8]) -> anyhow::Result<Vec<u8>> {
    if payload.len() as u64 > MAX_FRAME_LEN as u64 {
        anyhow::bail!("payload of {} bytes exceeds MAX_FRAME_LEN", payload.len());
    }
    let mut out = Vec::with_capacity(HEADER_LEN + payload.len());
    out.extend_from_slice(&(payload.len() as u32).to_le_bytes());
    out.push(channel.to_byte());
    out.extend_from_slice(payload);
    Ok(out)
}

/// Incremental parser: bulk reads don't align with frame boundaries (a
/// single read may contain a partial frame, multiple frames, or a frame
/// spanning several reads), so bytes are fed in as they arrive and frames
/// are pulled out as they become complete.
#[derive(Default)]
pub struct FrameReader {
    buf: VecDeque<u8>,
}

impl FrameReader {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn feed(&mut self, data: &[u8]) {
        self.buf.extend(data.iter().copied());
    }

    /// Returns the next complete frame if one is fully buffered, consuming
    /// its bytes. Returns Ok(None) if more data is needed. Returns Err on a
    /// malformed length (defensive against a corrupt/hostile length field).
    pub fn try_read_frame(&mut self) -> anyhow::Result<Option<Frame>> {
        if self.buf.len() < HEADER_LEN {
            return Ok(None);
        }

        let len_bytes: Vec<u8> = self.buf.iter().take(4).copied().collect();
        let len = u32::from_le_bytes(len_bytes.try_into().unwrap());
        if len > MAX_FRAME_LEN {
            anyhow::bail!("frame length {} exceeds MAX_FRAME_LEN {}", len, MAX_FRAME_LEN);
        }

        let total_len = HEADER_LEN + len as usize;
        if self.buf.len() < total_len {
            return Ok(None);
        }

        let channel_byte = self.buf[4];
        let payload: Vec<u8> = self.buf.iter().skip(HEADER_LEN).take(len as usize).copied().collect();

        for _ in 0..total_len {
            self.buf.pop_front();
        }

        Ok(Some(Frame {
            channel: Channel::from_byte(channel_byte),
            payload,
        }))
    }
}

pub mod control {
    pub const HELLO: u8 = 0x01;
    pub const HELLO_ACK: u8 = 0x02;

    pub fn encode_hello(protocol_version: u8) -> Vec<u8> {
        vec![HELLO, protocol_version]
    }

    // The host only sends HELLO and receives HELLO_ACK; this encoder is
    // the client-side half of the pair, kept here for protocol symmetry
    // and exercised by tests. The Kotlin client has its own equivalent.
    #[allow(dead_code)]
    pub fn encode_hello_ack(protocol_version: u8) -> Vec<u8> {
        vec![HELLO_ACK, protocol_version]
    }
}

pub mod input {
    pub const TOUCH: u8 = 0x01;
    const TOUCH_LEN: usize = 15;

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub enum TouchAction {
        Down,
        Move,
        Up,
        Cancel,
    }

    impl TouchAction {
        fn from_byte(b: u8) -> Option<Self> {
            match b {
                0 => Some(TouchAction::Down),
                1 => Some(TouchAction::Move),
                2 => Some(TouchAction::Up),
                3 => Some(TouchAction::Cancel),
                _ => None,
            }
        }

        // Only the client (Kotlin) encodes touch events at runtime; this
        // half exists for wire-format symmetry and is exercised by tests.
        #[allow(dead_code)]
        fn to_byte(self) -> u8 {
            match self {
                TouchAction::Down => 0,
                TouchAction::Move => 1,
                TouchAction::Up => 2,
                TouchAction::Cancel => 3,
            }
        }
    }

    #[derive(Debug, Clone, Copy, PartialEq)]
    pub struct TouchEvent {
        pub action: TouchAction,
        pub pointer_id: u8,
        /// Normalized 0.0-1.0 relative to the tablet's reported screen size.
        pub x: f32,
        /// Normalized 0.0-1.0 relative to the tablet's reported screen size.
        pub y: f32,
        /// Normalized 0.0-1.0; 1.0 for plain touch, real value for stylus.
        pub pressure: f32,
    }

    // Only the client (Kotlin) encodes touch events at runtime; this half
    // exists for wire-format symmetry and is exercised by tests.
    #[allow(dead_code)]
    pub fn encode_touch(ev: TouchEvent) -> Vec<u8> {
        let mut out = Vec::with_capacity(TOUCH_LEN);
        out.push(TOUCH);
        out.push(ev.action.to_byte());
        out.push(ev.pointer_id);
        out.extend_from_slice(&ev.x.to_le_bytes());
        out.extend_from_slice(&ev.y.to_le_bytes());
        out.extend_from_slice(&ev.pressure.to_le_bytes());
        out
    }

    pub fn decode_touch(payload: &[u8]) -> anyhow::Result<TouchEvent> {
        if payload.len() < TOUCH_LEN || payload[0] != TOUCH {
            anyhow::bail!("malformed TOUCH payload: {:?}", payload);
        }
        let action = TouchAction::from_byte(payload[1])
            .ok_or_else(|| anyhow::anyhow!("unknown touch action byte {}", payload[1]))?;
        Ok(TouchEvent {
            action,
            pointer_id: payload[2],
            x: f32::from_le_bytes(payload[3..7].try_into().unwrap()),
            y: f32::from_le_bytes(payload[7..11].try_into().unwrap()),
            pressure: f32::from_le_bytes(payload[11..15].try_into().unwrap()),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_a_single_frame() {
        let encoded = encode_frame(Channel::Control, &control::encode_hello(1)).unwrap();
        let mut reader = FrameReader::new();
        reader.feed(&encoded);
        let frame = reader.try_read_frame().unwrap().unwrap();
        assert_eq!(frame.channel, Channel::Control);
        assert_eq!(frame.payload, control::encode_hello(1));
        assert!(reader.try_read_frame().unwrap().is_none());
    }

    #[test]
    fn handles_a_frame_split_across_multiple_feeds() {
        let encoded = encode_frame(Channel::Video, &vec![0xAB; 1000]).unwrap();
        let mut reader = FrameReader::new();
        reader.feed(&encoded[..3]);
        assert!(reader.try_read_frame().unwrap().is_none());
        reader.feed(&encoded[3..500]);
        assert!(reader.try_read_frame().unwrap().is_none());
        reader.feed(&encoded[500..]);
        let frame = reader.try_read_frame().unwrap().unwrap();
        assert_eq!(frame.channel, Channel::Video);
        assert_eq!(frame.payload.len(), 1000);
    }

    #[test]
    fn handles_multiple_frames_in_one_feed() {
        let a = encode_frame(Channel::Control, &[1, 2, 3]).unwrap();
        let b = encode_frame(Channel::Input, &[4, 5]).unwrap();
        let mut combined = a.clone();
        combined.extend_from_slice(&b);

        let mut reader = FrameReader::new();
        reader.feed(&combined);
        let f1 = reader.try_read_frame().unwrap().unwrap();
        let f2 = reader.try_read_frame().unwrap().unwrap();
        assert_eq!(f1.payload, vec![1, 2, 3]);
        assert_eq!(f2.payload, vec![4, 5]);
    }

    #[test]
    fn rejects_a_frame_length_exceeding_the_cap() {
        let mut reader = FrameReader::new();
        let mut bogus_header = (MAX_FRAME_LEN + 1).to_le_bytes().to_vec();
        bogus_header.push(0);
        reader.feed(&bogus_header);
        assert!(reader.try_read_frame().is_err());
    }

    #[test]
    fn recognizes_a_hello_ack_reply() {
        let encoded = encode_frame(Channel::Control, &control::encode_hello_ack(1)).unwrap();
        let mut reader = FrameReader::new();
        reader.feed(&encoded);
        let frame = reader.try_read_frame().unwrap().unwrap();
        assert_eq!(frame.payload[0], control::HELLO_ACK);
        assert_eq!(frame.payload[1], 1);
    }

    #[test]
    fn round_trips_a_touch_event() {
        let ev = input::TouchEvent {
            action: input::TouchAction::Move,
            pointer_id: 0,
            x: 0.25,
            y: 0.75,
            pressure: 0.5,
        };
        let encoded = input::encode_touch(ev);
        let decoded = input::decode_touch(&encoded).unwrap();
        assert_eq!(decoded, ev);
    }

    #[test]
    fn rejects_a_truncated_touch_payload() {
        let ev = input::TouchEvent {
            action: input::TouchAction::Down,
            pointer_id: 0,
            x: 0.0,
            y: 0.0,
            pressure: 1.0,
        };
        let encoded = input::encode_touch(ev);
        assert!(input::decode_touch(&encoded[..encoded.len() - 1]).is_err());
    }

    #[test]
    fn rejects_an_unknown_touch_action_byte() {
        let mut encoded = input::encode_touch(input::TouchEvent {
            action: input::TouchAction::Down,
            pointer_id: 0,
            x: 0.0,
            y: 0.0,
            pressure: 1.0,
        });
        encoded[1] = 0xEE;
        assert!(input::decode_touch(&encoded).is_err());
    }

    #[test]
    fn unknown_channel_byte_round_trips_as_unknown() {
        let mut reader = FrameReader::new();
        reader.feed(&5u32.to_le_bytes());
        reader.feed(&[99]);
        reader.feed(&[1, 2, 3, 4, 5]);
        let frame = reader.try_read_frame().unwrap().unwrap();
        assert_eq!(frame.channel, Channel::Unknown(99));
    }
}
