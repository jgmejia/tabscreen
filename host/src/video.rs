//! Screen capture + VAAPI H.264 encode via a spawned `ffmpeg` subprocess.
//!
//! Reimplementing X11 capture and VAAPI encoding directly in Rust would be
//! a large undertaking; Phase 0 already validated this exact ffmpeg
//! pipeline (x11grab + VAAPI h264_vaapi) empirically against this hardware,
//! so v1 shells out to it rather than reinventing it.
//!
//! Captures the tabscreen `vkms` output's exact rect (see `display.rs`,
//! ADR 0021) rather than the whole X11 screen -- `main.rs` activates that
//! output and passes its geometry in here per connection.

use std::io::Read;
use std::process::{Child, Command, Stdio};
use std::sync::mpsc::{self, Receiver};

use crate::display::OutputRect;

const VAAPI_RENDER_NODE: &str = "/dev/dri/renderD128"; // confirmed the Intel iGPU in Phase 0
const BITRATE: &str = "8M";
const FRAMERATE: &str = "30";
const GOP: &str = "60";

/// Empirically-confirmed pixel-count ceiling for the target HONOR Pad
/// X8a's AVC hardware decoder. Its vendor-declared capability XML claims
/// a far higher ceiling (max size 4096x2176, block-count up to 34816),
/// but real decode of the tabscreen output's native 1200x1920
/// (2,304,000px) faults hard -- confirmed on *both* the low_latency and
/// plain hardware decoder variants, and confirmed independent of
/// orientation (1920x1200, same pixel count swapped to landscape, fails
/// identically) and independent of the MediaCodec `configure()` hint
/// (matching the real dimensions exactly just moves the failure from
/// configure()-time to first-real-frame-time). 1920x1080 (2,073,600px --
/// this device's declared performance-point-1920x1080 entry) decodes
/// cleanly; the real enforced ceiling sits somewhere between the two.
/// This constant stays safely under it, aspect ratio preserved via
/// uniform scaling rather than width-only clamping (an earlier version
/// of this constant capped width alone, which doesn't hold: the failure
/// is pixel-count-driven, not width-driven).
const MAX_ENCODE_PIXELS: u64 = 1088 * 1744;

pub struct VideoStream {
    child: Child,
    rx: Receiver<Vec<u8>>,
}

impl VideoStream {
    /// Blocks until a NAL unit is available, or returns `None` once the
    /// encoder's read thread has ended (e.g. ffmpeg exited on its own).
    pub fn recv(&self) -> Option<Vec<u8>> {
        self.rx.recv().ok()
    }

    /// Stops the ffmpeg process. Call once nothing will read from this
    /// stream again (e.g. the connection it was being sent over ended).
    pub fn stop(mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// Spawns the capture+encode pipeline and returns a [`VideoStream`] that
/// yields one Annex-B NAL unit (start code included) per [`VideoStream::recv`]
/// call, in encode order. Captures exactly `rect` (the activated tabscreen
/// output's geometry), not the whole X11 screen.
pub fn spawn_encoder(rect: &OutputRect) -> anyhow::Result<VideoStream> {
    let (width, height) = (rect.width, rect.height);
    let display = std::env::var("DISPLAY").unwrap_or_else(|_| ":0".to_string());

    // Even-dimensioned encode target (yuv420p/nv12 requires it), scaled
    // uniformly (aspect ratio preserved) if needed to stay under
    // MAX_ENCODE_PIXELS.
    let area = width as u64 * height as u64;
    let (encode_width, encode_height) = if area > MAX_ENCODE_PIXELS {
        let scale = (MAX_ENCODE_PIXELS as f64 / area as f64).sqrt();
        (
            (((width as f64 * scale) as u32) & !1).max(2),
            (((height as f64 * scale) as u32) & !1).max(2),
        )
    } else {
        (width as u32 & !1, height as u32 & !1)
    };
    log::info!(
        "starting video capture: {display}+{},{} {width}x{height} -> scale {encode_width}x{encode_height} -> VAAPI h264 @ {BITRATE}",
        rect.x, rect.y
    );

    let mut child = Command::new("ffmpeg")
        .args([
            "-loglevel", "warning",
            "-f", "x11grab",
            "-framerate", FRAMERATE,
            "-video_size", &format!("{width}x{height}"),
            "-i", &format!("{display}+{},{}", rect.x, rect.y),
            "-vaapi_device", VAAPI_RENDER_NODE,
            "-vf", &format!("scale={encode_width}:{encode_height},format=nv12,hwupload"),
            "-c:v", "h264_vaapi",
            "-bf", "0",
            "-g", GOP,
            "-b:v", BITRATE,
            // The default SEI messages (identifier/timing/recovery_point)
            // produce one standalone SEI NAL per frame; confirmed against
            // the target tablet's low_latency hardware decoder that
            // feeding those as their own buffer eventually crashes it
            // (IllegalStateException after a handful of frames) even
            // though the bitstream itself is fully spec-valid (verified
            // via a clean ffmpeg -f null software-decode round trip).
            "-sei", "0",
            "-f", "h264",
            "-",
        ])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()
        .map_err(|e| anyhow::anyhow!("failed to spawn ffmpeg: {e}"))?;

    let stdout = child.stdout.take().expect("stdout was piped");
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        if let Err(e) = read_nal_units(stdout, &tx) {
            log::error!("video encoder read loop ended: {e:#}");
        }
    });
    Ok(VideoStream { child, rx })
}

/// Incrementally splits ffmpeg's Annex-B byte stream on start codes and
/// sends each complete NAL unit (start code included) as it's found.
fn read_nal_units(mut stdout: impl Read, tx: &mpsc::Sender<Vec<u8>>) -> anyhow::Result<()> {
    let mut buf = Vec::new();
    let mut chunk = [0u8; 65536];
    loop {
        let n = stdout.read(&mut chunk)?;
        if n == 0 {
            return Ok(()); // ffmpeg exited
        }
        buf.extend_from_slice(&chunk[..n]);

        let starts = find_start_codes(&buf);
        if starts.len() < 2 {
            continue; // need at least two boundaries to emit one complete NAL
        }
        for w in starts.windows(2) {
            let nal = buf[w[0]..w[1]].to_vec();
            if tx.send(nal).is_err() {
                return Ok(()); // receiver gone, connection closed
            }
        }
        let tail_from = *starts.last().unwrap();
        buf.drain(..tail_from);
    }
}

/// Byte offset of every Annex-B start code in `buf` — both the 3-byte
/// `00 00 01` and 4-byte `00 00 00 01` forms (the offset returned includes
/// the extra leading zero for the 4-byte form).
fn find_start_codes(buf: &[u8]) -> Vec<usize> {
    let mut out = Vec::new();
    let mut i = 0;
    while i + 3 <= buf.len() {
        if buf[i] == 0 && buf[i + 1] == 0 && buf[i + 2] == 1 {
            let start = if i > 0 && buf[i - 1] == 0 { i - 1 } else { i };
            out.push(start);
            i += 3;
        } else {
            i += 1;
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splits_annex_b_stream_on_start_codes() {
        let mut stream = Vec::new();
        stream.extend_from_slice(&[0, 0, 0, 1]); // 4-byte start code
        stream.extend_from_slice(&[0xAA, 0xBB]);
        stream.extend_from_slice(&[0, 0, 1]); // 3-byte start code
        stream.extend_from_slice(&[0xCC, 0xDD, 0xEE]);
        stream.extend_from_slice(&[0, 0, 0, 1]);
        stream.extend_from_slice(&[0xFF]);

        let (tx, rx) = mpsc::channel();
        read_nal_units(&stream[..], &tx).unwrap();
        drop(tx);

        let nals: Vec<Vec<u8>> = rx.iter().collect();
        assert_eq!(nals, vec![
            vec![0, 0, 0, 1, 0xAA, 0xBB],
            vec![0, 0, 1, 0xCC, 0xDD, 0xEE],
        ]);
    }

    #[test]
    fn holds_a_trailing_partial_nal_until_more_data_arrives() {
        // Only one start code total -> no complete NAL to emit yet.
        let stream = [0u8, 0, 0, 1, 0xAA, 0xBB];
        let (tx, rx) = mpsc::channel();
        read_nal_units(&stream[..], &tx).unwrap();
        drop(tx);
        assert!(rx.iter().collect::<Vec<_>>().is_empty());
    }
}
