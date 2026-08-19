//! Screen capture + VAAPI H.264 encode via a spawned `ffmpeg` subprocess.
//!
//! Reimplementing X11 capture and VAAPI encoding directly in Rust would be
//! a large undertaking; Phase 0 already validated this exact ffmpeg
//! pipeline (x11grab + VAAPI h264_vaapi) empirically against this hardware,
//! so v1 shells out to it rather than reinventing it.
//!
//! Captures the *current default screen* until Phase 2 lands a dummy
//! output to target instead (see docs/wire-protocol.md's VIDEO section and
//! ADR 0006) — swapping the target later is a command-line change here,
//! not a wire-format change.

use std::io::Read;
use std::process::{Child, Command, Stdio};
use std::sync::mpsc::{self, Receiver};

use x11rb::connection::Connection;

const VAAPI_RENDER_NODE: &str = "/dev/dri/renderD128"; // confirmed the Intel iGPU in Phase 0
const BITRATE: &str = "8M";
const FRAMERATE: &str = "30";
const GOP: &str = "60";

/// Ceiling on the encoded width, aspect ratio preserved. The host's own
/// screen (captured as today's Phase-2-blocked stand-in target -- see
/// docs/wire-protocol.md's VIDEO section) can easily exceed what a budget
/// tablet's hardware decoder actually supports: confirmed against the
/// target HONOR Pad X8a that its low-latency AVC decoder variant reports a
/// hard ~1920px width ceiling, and feeding it a larger real SPS (e.g. this
/// dev machine's native 2560x1600) doesn't gracefully downscale -- it
/// faults the codec outright after a handful of frames. Once Phase 2
/// lands, capture targets the dummy output sized to the tablet's own
/// panel instead, which won't need this scale-down at all.
const MAX_ENCODE_WIDTH: u32 = 1280;

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
/// call, in encode order.
pub fn spawn_encoder() -> anyhow::Result<VideoStream> {
    let (width, height) = query_screen_size()?;
    let display = std::env::var("DISPLAY").unwrap_or_else(|_| ":0".to_string());

    // Even-dimensioned encode target (yuv420p/nv12 requires it), capped to
    // MAX_ENCODE_WIDTH with aspect ratio preserved.
    let (encode_width, encode_height) = if width as u32 > MAX_ENCODE_WIDTH {
        let scaled_height = (height as u64 * MAX_ENCODE_WIDTH as u64 / width as u64) as u32;
        (MAX_ENCODE_WIDTH & !1, scaled_height & !1)
    } else {
        (width as u32 & !1, height as u32 & !1)
    };
    log::info!(
        "starting video capture: {display} {width}x{height} -> scale {encode_width}x{encode_height} -> VAAPI h264 @ {BITRATE}"
    );

    let mut child = Command::new("ffmpeg")
        .args([
            "-loglevel", "warning",
            "-f", "x11grab",
            "-framerate", FRAMERATE,
            "-video_size", &format!("{width}x{height}"),
            "-i", &format!("{display}+0,0"),
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

fn query_screen_size() -> anyhow::Result<(u16, u16)> {
    let (conn, screen_num) = x11rb::connect(None)?;
    let screen = &conn.setup().roots[screen_num];
    Ok((screen.width_in_pixels, screen.height_in_pixels))
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
