mod aoa;
mod protocol;

use protocol::{control, Channel, Frame, FrameReader};
use std::time::Duration;

fn main() {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();
    loop {
        if let Err(e) = run_one_connection() {
            log::error!("connection cycle ended: {e:#}");
        }
        log::info!("retrying in 2s...");
        std::thread::sleep(Duration::from_secs(2));
    }
}

fn run_one_connection() -> anyhow::Result<()> {
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

    let hello = protocol::encode_frame(Channel::Control, &control::encode_hello(1))?;
    conn.handle.write_bulk(conn.ep_out, &hello, Duration::from_secs(10))?;
    log::info!("sent HELLO");

    let mut reader = FrameReader::new();
    let mut buf = [0u8; 16384];
    loop {
        match conn.handle.read_bulk(conn.ep_in, &mut buf, Duration::from_secs(30)) {
            Ok(n) => {
                reader.feed(&buf[..n]);
                while let Some(frame) = reader.try_read_frame()? {
                    handle_frame(frame);
                }
            }
            Err(rusb::Error::Timeout) => continue,
            Err(e) => return Err(e.into()),
        }
    }
}

fn handle_frame(frame: Frame) {
    match frame.channel {
        Channel::Control => {
            if frame.payload.first() == Some(&control::HELLO_ACK) {
                let version = frame.payload.get(1).copied().unwrap_or(0);
                log::info!("received HELLO_ACK, client protocol_version={version}");
            } else {
                log::warn!("unexpected control payload: {:?}", frame.payload);
            }
        }
        other => log::warn!(
            "unexpected channel in Phase 1: {other:?} ({} bytes)",
            frame.payload.len()
        ),
    }
}
