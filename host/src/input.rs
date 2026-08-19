//! X11 pointer injection via the XTEST extension.
//!
//! Until Phase 2 lands there's no dummy RandR output to target, so this
//! injects against the current default screen's root window instead — the
//! wire protocol's normalized 0.0-1.0 coordinates (docs/wire-protocol.md)
//! mean swapping the target rect for the dummy output later needs no
//! protocol change, just a different (width, height) here.

use crate::protocol::input::{TouchAction, TouchEvent};
use x11rb::connection::Connection;
use x11rb::protocol::xproto::{BUTTON_PRESS_EVENT, BUTTON_RELEASE_EVENT, MOTION_NOTIFY_EVENT};
use x11rb::protocol::xtest::ConnectionExt as _;
use x11rb::rust_connection::RustConnection;

const LEFT_BUTTON: u8 = 1;
/// XTestFakeInput's `time` field: 0 means "use the current server time" —
/// there's no meaningful client-side timestamp to send instead.
const CURRENT_TIME: u32 = 0;

pub struct Injector {
    conn: RustConnection,
    root: u32,
    width: u16,
    height: u16,
}

impl Injector {
    /// Connects to the X server named by `$DISPLAY` and captures the
    /// default screen's root window and pixel size once at startup.
    pub fn connect() -> anyhow::Result<Self> {
        let (conn, screen_num) = x11rb::connect(None)?;
        let screen = &conn.setup().roots[screen_num];
        let root = screen.root;
        let width = screen.width_in_pixels;
        let height = screen.height_in_pixels;
        Ok(Self { conn, root, width, height })
    }

    pub fn inject(&self, ev: TouchEvent) -> anyhow::Result<()> {
        let x = (ev.x.clamp(0.0, 1.0) * self.width as f32) as i16;
        let y = (ev.y.clamp(0.0, 1.0) * self.height as f32) as i16;

        self.conn
            .xtest_fake_input(MOTION_NOTIFY_EVENT, 0, CURRENT_TIME, self.root, x, y, 0)?
            .check()?;

        match ev.action {
            TouchAction::Down => {
                self.conn
                    .xtest_fake_input(BUTTON_PRESS_EVENT, LEFT_BUTTON, CURRENT_TIME, self.root, x, y, 0)?
                    .check()?;
            }
            TouchAction::Up | TouchAction::Cancel => {
                self.conn
                    .xtest_fake_input(BUTTON_RELEASE_EVENT, LEFT_BUTTON, CURRENT_TIME, self.root, x, y, 0)?
                    .check()?;
            }
            TouchAction::Move => {}
        }

        self.conn.flush()?;
        Ok(())
    }
}
