mod aoa;
mod display;
mod input;
mod protocol;
mod video;

use protocol::{control, Channel, Frame, FrameReader};
use rusb::{DeviceHandle, GlobalContext};
use std::sync::Arc;
use std::time::Duration;
use x11rb::connection::Connection;
use x11rb::rust_connection::RustConnection;

/// Fallback tablet mode (Phase 5): used only if the client doesn't
/// complete the `DISPLAY_INFO` handshake within `HELLO_ACK_TIMEOUT` -- an
/// old/non-negotiating client, or a malformed message. Otherwise the real
/// negotiated values from the client's own `DISPLAY_INFO` (see
/// `wait_for_display_info`) are used instead. Value is the HONOR Pad
/// X8a's confirmed real panel spec (Phase 0), landscape (the user runs it
/// rotated 90° from native -- `android:screenOrientation="landscape"` in
/// the manifest; the tablet's own rotation lock must be set to match, not
/// auto-rotate, or the orientation mismatch letterboxes the surface, see
/// ADR 0023).
const FALLBACK_TABLET_WIDTH: u32 = 1920;
const FALLBACK_TABLET_HEIGHT: u32 = 1200;
const FALLBACK_TABLET_REFRESH_HZ: u32 = 60;

/// How long to wait for the client's `DISPLAY_INFO` (docs/wire-protocol.md)
/// after `HELLO_ACK` before giving up and using the fallback above. AOA
/// bulk transfer is confirmed fast (Phase 0: sub-millisecond typical), so
/// this is generous slack for a slow client, not a tuned-tight budget.
const HELLO_ACK_TIMEOUT: Duration = Duration::from_secs(5);

/// `Placement::Left` was tried live (2026-08-20) and reverted: shifting
/// real outputs via sequential per-output `xrandr --pos` calls hit GNOME/
/// mutter reactively re-laying-out *all* outputs between each call,
/// producing an unpredictable final layout (twice, on the same session --
/// see the conversation, not yet written up as an ADR). A single atomic
/// `xrandr` command covering every `--output`/`--pos` pair together
/// looked like it might fix it (confirmed only for a manual restore, not
/// yet tried for the actual shift), but wasn't retested live after two
/// live disruptions in a row. `Right` never touches the real outputs at
/// all and has caused zero issues -- stick with it unless that atomic-
/// command fix is actually implemented and carefully retested.
const PLACEMENT: display::Placement = display::Placement::Right;

fn main() {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();

    // Ctrl+C (SIGINT) or a plain `kill`/`timeout`/systemd stop (SIGTERM)
    // otherwise abort the process before ActiveOutput::drop gets a chance
    // to run its own cleanup -- confirmed live (2026-08-20) that Ctrl+C
    // without this left the tablet's RandR output frozen active, needing
    // a manual `xrandr --off` to recover; separately confirmed `timeout`
    // sends SIGTERM, not SIGINT, and left the same mess since ctrlc's
    // default build only covers SIGINT -- the `termination` feature
    // (Cargo.toml) is what extends this handler to SIGTERM/SIGHUP too,
    // which matters once Phase 6 wraps this in a systemd service (whose
    // stop mechanism is SIGTERM). Opens its own short-lived X11
    // connection rather than sharing the main one, since the handler
    // runs on its own thread at an arbitrary point in the program and a
    // signal can arrive mid-connection-setup, before `x11` below is even
    // established.
    ctrlc::set_handler(|| {
        log::info!("interrupted, cleaning up before exit...");
        if let Ok((conn, screen_num)) = x11rb::connect(None) {
            let root = conn.setup().roots[screen_num].root;
            display::deactivate_any_active(&conn, root);
        }
        std::process::exit(130);
    })
    .expect("failed to install Ctrl+C handler");

    let x11 = match x11rb::connect(None) {
        Ok((conn, screen_num)) => {
            let root = conn.setup().roots[screen_num].root;
            display::heal_layout_gap(&conn, root);
            Some((conn, root))
        }
        Err(e) => {
            log::warn!("X11 connection unavailable, virtual display/input support disabled: {e:#}");
            None
        }
    };

    loop {
        if let Err(e) = run_one_connection(x11.as_ref()) {
            log::error!("connection cycle ended: {e:#}");
        }
        log::info!("retrying in 2s...");
        std::thread::sleep(Duration::from_secs(2));
    }
}

fn run_one_connection(x11: Option<&(RustConnection, u32)>) -> anyhow::Result<()> {
    // A prior connection may have ended without a physical unplug (app
    // crash, permission revoked, host-side error) — in that case the
    // device is still sitting in accessory mode and there's no need (or
    // way) to redo the handshake from scratch.
    let accessory_device = match aoa::find_existing_accessory_device()? {
        Some(device) => {
            log::info!("device already in accessory mode, skipping handshake");
            device
        }
        None => {
            log::info!("waiting for tablet (vendor id match)...");
            let candidate = aoa::wait_for_candidate_device(Duration::from_secs(3600))?;
            log::info!("found candidate device, starting AOA handshake");
            aoa::start_accessory_mode(&candidate, &aoa::TABSCREEN_IDENTIFICATION)?;

            log::info!("waiting for accessory-mode re-enumeration...");
            aoa::wait_for_accessory_device(Duration::from_secs(10))?
        }
    };
    let conn = aoa::open_accessory(accessory_device)?;
    log::info!(
        "accessory open: interface {} ep_in={:#04x} ep_out={:#04x}",
        conn.interface,
        conn.ep_in,
        conn.ep_out
    );

    let handle = Arc::new(conn.handle);
    let ep_in = conn.ep_in;
    let ep_out = conn.ep_out;

    let hello = protocol::encode_frame(Channel::Control, &control::encode_hello(1))?;
    handle.write_bulk(ep_out, &hello, Duration::from_secs(10))?;
    log::info!("sent HELLO");

    // Shared with the main read loop below -- wait_for_display_info only
    // consumes CONTROL frames it recognizes (HELLO_ACK, DISPLAY_INFO) and
    // forwards anything else to handle_frame, so nothing arriving early is
    // lost or double-processed once the main loop takes over the same
    // reader/buf.
    let mut reader = FrameReader::new();
    let mut buf = [0u8; 16384];

    let (tablet_width, tablet_height, tablet_refresh_hz) =
        match wait_for_display_info(&handle, ep_in, &mut reader, &mut buf, HELLO_ACK_TIMEOUT)? {
            Some(info) => {
                log::info!(
                    "negotiated display: {}x{}@{}Hz",
                    info.width, info.height, info.refresh_hz
                );
                (info.width as u32, info.height as u32, (info.refresh_hz as u32).max(1))
            }
            None => {
                log::warn!(
                    "no DISPLAY_INFO within {HELLO_ACK_TIMEOUT:?}, falling back to \
                     {FALLBACK_TABLET_WIDTH}x{FALLBACK_TABLET_HEIGHT}@{FALLBACK_TABLET_REFRESH_HZ}Hz"
                );
                (FALLBACK_TABLET_WIDTH, FALLBACK_TABLET_HEIGHT, FALLBACK_TABLET_REFRESH_HZ)
            }
        };

    // Activated for the lifetime of this connection cycle only -- dropping
    // `active_output` (any exit path: error, unplug, normal return) turns
    // the tabscreen output back off via its Drop impl.
    let active_output = match x11 {
        Some((conn, root)) => {
            match display::activate(conn, *root, tablet_width, tablet_height, tablet_refresh_hz, PLACEMENT) {
                Ok(a) => Some(a),
                Err(e) => {
                    log::warn!("virtual display activation failed, video/input disabled for this connection: {e:#}");
                    None
                }
            }
        }
        None => None,
    };

    let injector = match (x11, &active_output) {
        (Some((conn, root)), Some(active)) => Some(input::Injector::new(conn, *root, active.rect)),
        _ => None,
    };
    if injector.is_none() {
        log::warn!("X11 XTEST injector unavailable, INPUT frames will be dropped");
    }

    match &active_output {
        Some(active) => match video::spawn_encoder(&active.rect) {
            Ok(stream) => {
                let video_handle = handle.clone();
                // Self-terminating: exits (and stops ffmpeg) once a write fails,
                // which happens naturally once this connection ends — not
                // joined here, so a brief overlap with the next connection
                // cycle's own video thread is possible but harmless.
                std::thread::spawn(move || video_writer_loop(video_handle, ep_out, stream));
            }
            Err(e) => log::warn!("video capture/encode unavailable, streaming disabled: {e:#}"),
        },
        None => log::warn!("no active virtual display, video streaming disabled"),
    }

    loop {
        match handle.read_bulk(ep_in, &mut buf, Duration::from_secs(30)) {
            Ok(n) => {
                reader.feed(&buf[..n]);
                while let Some(frame) = reader.try_read_frame()? {
                    handle_frame(frame, injector.as_ref());
                }
            }
            Err(rusb::Error::Timeout) => continue,
            Err(e) => return Err(e.into()),
        }
    }
}

/// Reads CONTROL frames until the client's `DISPLAY_INFO` (Phase 5,
/// docs/wire-protocol.md) arrives or `timeout` elapses, returning `Ok(None)`
/// on a clean timeout (not an error -- the caller falls back to a default
/// mode) or `Err` on a real connection failure. Shares `reader`/`buf` with
/// the caller's own main loop so a read that grabs bytes belonging to a
/// later frame (e.g. an early `INPUT` frame arriving in the same bulk
/// transfer as `DISPLAY_INFO`) isn't lost or replayed once the caller
/// takes over.
fn wait_for_display_info(
    handle: &DeviceHandle<GlobalContext>,
    ep_in: u8,
    reader: &mut FrameReader,
    buf: &mut [u8],
    timeout: Duration,
) -> anyhow::Result<Option<control::DisplayInfo>> {
    let deadline = std::time::Instant::now() + timeout;
    loop {
        let remaining = deadline.saturating_duration_since(std::time::Instant::now());
        if remaining.is_zero() {
            return Ok(None);
        }
        match handle.read_bulk(ep_in, buf, remaining) {
            Ok(n) => {
                reader.feed(&buf[..n]);
                while let Some(frame) = reader.try_read_frame()? {
                    if frame.channel == Channel::Control {
                        if frame.payload.first() == Some(&control::HELLO_ACK) {
                            let version = frame.payload.get(1).copied().unwrap_or(0);
                            log::info!("received HELLO_ACK, client protocol_version={version}");
                            continue;
                        }
                        if let Ok(info) = control::decode_display_info(&frame.payload) {
                            return Ok(Some(info));
                        }
                    }
                    handle_frame(frame, None);
                }
            }
            Err(rusb::Error::Timeout) => continue,
            Err(e) => return Err(e.into()),
        }
    }
}

fn video_writer_loop(handle: Arc<DeviceHandle<GlobalContext>>, ep_out: u8, stream: video::VideoStream) {
    loop {
        let Some(nal) = stream.recv() else {
            log::warn!("video encoder ended (ffmpeg exited)");
            stream.stop();
            return;
        };
        let frame = match protocol::encode_frame(Channel::Video, &nal) {
            Ok(f) => f,
            Err(e) => {
                log::warn!("failed to encode VIDEO frame: {e:#}");
                continue;
            }
        };
        if let Err(e) = handle.write_bulk(ep_out, &frame, Duration::from_secs(5)) {
            log::warn!("video write failed, stopping encoder: {e:#}");
            stream.stop();
            return;
        }
    }
}

fn handle_frame(frame: Frame, injector: Option<&input::Injector<'_>>) {
    match frame.channel {
        Channel::Control => {
            if frame.payload.first() == Some(&control::HELLO_ACK) {
                let version = frame.payload.get(1).copied().unwrap_or(0);
                log::info!("received HELLO_ACK, client protocol_version={version}");
            } else if control::decode_display_info(&frame.payload).is_ok() {
                // Already consumed by wait_for_display_info in the normal
                // case; only reachable if the client sends a second one
                // mid-connection, which nothing currently does or expects.
                log::warn!("ignoring unexpected mid-connection DISPLAY_INFO");
            } else {
                log::warn!("unexpected control payload: {:?}", frame.payload);
            }
        }
        Channel::Input => match protocol::input::decode_touch(&frame.payload) {
            Ok(ev) => {
                let Some(injector) = injector else {
                    log::warn!("dropping touch event, no injector available: {ev:?}");
                    return;
                };
                if let Err(e) = injector.inject(ev) {
                    log::error!("failed to inject touch event {ev:?}: {e:#}");
                }
            }
            Err(e) => log::warn!("malformed INPUT payload: {e:#}"),
        },
        other => log::warn!(
            "unexpected channel: {other:?} ({} bytes)",
            frame.payload.len()
        ),
    }
}
