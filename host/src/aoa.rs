//! Android Open Accessory (AOA) handshake and bulk transport.
//!
//! Hand-rolled on top of rusb's raw control-transfer API — confirmed in
//! Phase 0 (see TASKS.md) that no maintained Rust crate implements AOA.
//! The control-transfer sequence here mirrors the Python/pyusb probe that
//! validated this against the real HONOR Pad X8a in Phase 0.

use rusb::{Device, DeviceHandle, GlobalContext};
use std::time::{Duration, Instant};

// Per ADR-0004: v1 targets this specific tablet. Broadening to "any AOA
// device" is a deliberate future decision, not an oversight — see that ADR.
const TARGET_VENDOR_ID: u16 = 0x339b; // HONOR

const GOOGLE_VENDOR_ID: u16 = 0x18d1;
const ACCESSORY_PIDS: [u16; 2] = [0x2d00, 0x2d01]; // accessory, accessory+adb

const REQ_GET_PROTOCOL: u8 = 51;
const REQ_SEND_STRING: u8 = 52;
const REQ_START_ACCESSORY: u8 = 53;

const CTRL_IN_VENDOR_DEVICE: u8 = 0xC0;
const CTRL_OUT_VENDOR_DEVICE: u8 = 0x40;

pub struct AccessoryIdentification {
    pub manufacturer: &'static str,
    pub model: &'static str,
    pub description: &'static str,
    pub version: &'static str,
    pub uri: &'static str,
    pub serial: &'static str,
}

pub const TABSCREEN_IDENTIFICATION: AccessoryIdentification = AccessoryIdentification {
    manufacturer: "tabscreen",
    model: "tabscreen-host",
    description: "tabscreen host daemon",
    version: "0.1",
    uri: "https://example.invalid/tabscreen",
    serial: "0000000000000001",
};

pub struct AccessoryConnection {
    pub handle: DeviceHandle<GlobalContext>,
    pub interface: u8,
    pub ep_in: u8,
    pub ep_out: u8,
}

/// Polls for the tablet in its normal (non-accessory) mode. Used both on
/// startup and after every reconnect cycle, since a physical unplug/replug
/// always reverts the tablet to normal mode — the AOA handshake has to run
/// again each time, it's not a one-shot setup.
pub fn wait_for_candidate_device(timeout: Duration) -> anyhow::Result<Device<GlobalContext>> {
    let deadline = Instant::now() + timeout;
    loop {
        for device in rusb::devices()?.iter() {
            let desc = device.device_descriptor()?;
            if desc.vendor_id() == TARGET_VENDOR_ID {
                return Ok(device);
            }
        }
        if Instant::now() >= deadline {
            anyhow::bail!("timed out waiting for candidate device (vendor {:#06x})", TARGET_VENDOR_ID);
        }
        std::thread::sleep(Duration::from_millis(200));
    }
}

/// Drives an already-found candidate device into accessory mode. Returns
/// once START_ACCESSORY has been sent — the caller must then wait for
/// re-enumeration (the device disconnects and reappears under Google's
/// vendor ID) via [`wait_for_accessory_device`].
pub fn start_accessory_mode(device: &Device<GlobalContext>, ident: &AccessoryIdentification) -> anyhow::Result<()> {
    let handle = device.open()?;
    log::info!("opened candidate device, probing AOA protocol version");

    let mut buf = [0u8; 2];
    let n = handle.read_control(CTRL_IN_VENDOR_DEVICE, REQ_GET_PROTOCOL, 0, 0, &mut buf, Duration::from_secs(3))?;
    if n != 2 {
        anyhow::bail!("GET_PROTOCOL returned unexpected length {}", n);
    }
    let version = u16::from_le_bytes(buf);
    log::info!("AOA protocol version: {}", version);
    if version == 0 {
        anyhow::bail!("device does not support AOA (protocol version 0)");
    }
    std::thread::sleep(Duration::from_millis(100));

    for (index, s) in [
        ident.manufacturer,
        ident.model,
        ident.description,
        ident.version,
        ident.uri,
        ident.serial,
    ]
    .iter()
    .enumerate()
    {
        let mut data = s.as_bytes().to_vec();
        data.push(0);
        handle.write_control(CTRL_OUT_VENDOR_DEVICE, REQ_SEND_STRING, 0, index as u16, &data, Duration::from_secs(3))?;
        // Diagnostic: give the gadget driver time to process each command.
        // Rust issues these back-to-back far faster than the Python probe's
        // interpreter overhead did, which worked reliably.
        std::thread::sleep(Duration::from_millis(100));
    }

    handle.write_control(CTRL_OUT_VENDOR_DEVICE, REQ_START_ACCESSORY, 0, 0, &[], Duration::from_secs(3))?;
    log::info!("START_ACCESSORY sent; device should re-enumerate as {:#06x}:2d00/2d01", GOOGLE_VENDOR_ID);
    std::thread::sleep(Duration::from_millis(500));
    Ok(())
}

/// Polls for the re-enumerated accessory-mode device, since AOA
/// re-enumeration is a first-class state transition (confirmed in Phase 0),
/// not something that can be waited on via a single device handle.
pub fn wait_for_accessory_device(timeout: Duration) -> anyhow::Result<Device<GlobalContext>> {
    let deadline = Instant::now() + timeout;
    loop {
        if let Some(device) = find_existing_accessory_device()? {
            return Ok(device);
        }
        if Instant::now() >= deadline {
            anyhow::bail!("timed out waiting for accessory-mode re-enumeration");
        }
        std::thread::sleep(Duration::from_millis(200));
    }
}

/// Single, non-blocking check for a device already sitting in accessory
/// mode. Needed because a connection can end without a physical
/// unplug/replug (app crash, permission revoked, host-side error) — in
/// that case the device stays in accessory mode rather than reverting to
/// its normal vendor ID, so the reconnect loop must not assume it always
/// has to redo the full handshake from [`wait_for_candidate_device`].
pub fn find_existing_accessory_device() -> anyhow::Result<Option<Device<GlobalContext>>> {
    for device in rusb::devices()?.iter() {
        let desc = device.device_descriptor()?;
        if desc.vendor_id() == GOOGLE_VENDOR_ID && ACCESSORY_PIDS.contains(&desc.product_id()) {
            return Ok(Some(device));
        }
    }
    Ok(None)
}

/// Opens the re-enumerated device and claims its single vendor-specific
/// accessory interface, returning the bulk IN/OUT endpoint addresses found
/// on it rather than assuming fixed values — confirmed in Phase 0 that the
/// accessory interface is always interface 0 with exactly 2 bulk endpoints,
/// but reading the descriptor is cheap and avoids silent breakage if that
/// ever changes.
pub fn open_accessory(device: Device<GlobalContext>) -> anyhow::Result<AccessoryConnection> {
    let config = device.active_config_descriptor()?;
    for interface in config.interfaces() {
        for descriptor in interface.descriptors() {
            if descriptor.class_code() != 0xff {
                continue; // not vendor-specific
            }
            let bulk_endpoints: Vec<_> = descriptor
                .endpoint_descriptors()
                .filter(|e| e.transfer_type() == rusb::TransferType::Bulk)
                .collect();
            let ep_in = bulk_endpoints.iter().find(|e| e.direction() == rusb::Direction::In);
            let ep_out = bulk_endpoints.iter().find(|e| e.direction() == rusb::Direction::Out);
            if let (Some(ep_in), Some(ep_out)) = (ep_in, ep_out) {
                let handle = device.open()?;
                handle.claim_interface(interface.number())?;
                return Ok(AccessoryConnection {
                    handle,
                    interface: interface.number(),
                    ep_in: ep_in.address(),
                    ep_out: ep_out.address(),
                });
            }
        }
    }
    anyhow::bail!("no vendor-specific interface with bulk IN+OUT endpoints found")
}
