//! X11 pointer injection via the XTEST extension.
//!
//! Injects against the tabscreen `vkms` output's exact rect (see
//! `display.rs`, ADR 0021), not the whole root window -- the wire
//! protocol's normalized 0.0-1.0 coordinates (docs/wire-protocol.md) mean
//! this needed no protocol change, just an offset/scale computed from a
//! different (x, y, width, height) source here.

use crate::display::OutputRect;
use crate::protocol::input::{TouchAction, TouchEvent};
use x11rb::connection::Connection;
use x11rb::protocol::xproto::{BUTTON_PRESS_EVENT, BUTTON_RELEASE_EVENT, MOTION_NOTIFY_EVENT};
use x11rb::protocol::xtest::ConnectionExt as _;
use x11rb::rust_connection::RustConnection;

const LEFT_BUTTON: u8 = 1;
/// XTestFakeInput's `time` field: 0 means "use the current server time" —
/// there's no meaningful client-side timestamp to send instead.
const CURRENT_TIME: u32 = 0;

pub struct Injector<'a> {
    conn: &'a RustConnection,
    root: u32,
    rect: OutputRect,
}

impl<'a> Injector<'a> {
    /// Borrows the shared X connection and targets `rect` (the activated
    /// tabscreen output's geometry) for all injected events.
    pub fn new(conn: &'a RustConnection, root: u32, rect: OutputRect) -> Self {
        Self { conn, root, rect }
    }

    pub fn inject(&self, ev: TouchEvent) -> anyhow::Result<()> {
        let x = self.rect.x + (ev.x.clamp(0.0, 1.0) * self.rect.width as f32) as i16;
        let y = self.rect.y + (ev.y.clamp(0.0, 1.0) * self.rect.height as f32) as i16;

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
