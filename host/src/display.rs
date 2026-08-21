//! Locates and (de)activates the tabscreen virtual RandR output over
//! `vkms` (ADR 0021), so `video.rs`/`input.rs` can target its real
//! geometry instead of the whole X11 screen.
//!
//! Mode registration/positioning is done by shelling out to `xrandr`
//! (`--newmode`/`--addmode`/`--pos`) rather than hand-rolling `RRCreateMode`
//! over the wire — this is the exact command sequence validated live
//! against the real hardware (ADR 0021), and matches this codebase's
//! existing pattern of shelling out to established X11/video tooling
//! (see `video.rs`'s `ffmpeg` use) rather than reimplementing it.

use std::process::Command;

use x11rb::protocol::randr::{ConnectionExt as _, GetScreenResourcesCurrentReply};
use x11rb::rust_connection::RustConnection;

/// `vkms`'s connector is always named `Virtual-*`; matched by prefix
/// rather than the exact `Virtual-2-1` seen in live testing, since the
/// numeric suffix depends on DRM card enumeration order and isn't
/// guaranteed stable across reboots (open question noted in ADR 0021).
const OUTPUT_NAME_PREFIX: &str = "Virtual-";

#[derive(Debug, Clone, Copy)]
pub struct OutputRect {
    pub x: i16,
    pub y: i16,
    pub width: u16,
    pub height: u16,
}

/// Where to position the tabscreen output relative to the real outputs
/// already active. `Left` requires shifting the real outputs right by
/// the tabscreen output's width first — X11/RandR's screen origin can't
/// go negative (confirmed live: `xrandr --pos -1200x0` silently clamps to
/// `0x0`, overlapping whatever's already there instead of sitting left of
/// it), so there's no way to give the tabscreen output a negative `x`
/// without moving something else to make room.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Placement {
    Left,
    Right,
}

/// Handle to an activated virtual output. Deactivates it (`xrandr --off`)
/// on drop, best-effort — so a connection cycle ending for any reason
/// (error, tablet unplug, normal shutdown) always leaves the output off
/// rather than stuck on from a stale prior session. For `Placement::Left`,
/// also restores every real output it shifted back to its original
/// position first — see `shifted`.
pub struct ActiveOutput {
    name: String,
    pub rect: OutputRect,
    /// Real outputs moved to make room for `Placement::Left`, as
    /// `(name, original_x, original_y)` — empty for `Placement::Right`,
    /// which never moves anything else.
    shifted: Vec<(String, i32, i32)>,
}

impl ActiveOutput {
    /// Queries this output's current live geometry directly from X11, for
    /// drift detection against `self.rect`. A RandR change made outside
    /// this daemon (e.g. the user toggling a real monitor via arandr)
    /// moves or turns off outputs without going through `activate()`/
    /// `Drop` at all -- confirmed live (2026-08-20): deactivating a real
    /// monitor shifted this output to a new position, leaving `video.rs`'s
    /// capture still reading the stale old offset until `ffmpeg` itself
    /// hard-failed. Returns `None` if the output currently has no active
    /// CRTC (turned off, or the mode was otherwise cleared).
    pub fn current_rect(&self, conn: &RustConnection, root: u32) -> anyhow::Result<Option<OutputRect>> {
        let resources = conn.randr_get_screen_resources_current(root)?.reply()?;
        for &output in &resources.outputs {
            let info = conn.randr_get_output_info(output, resources.config_timestamp)?.reply()?;
            if info.name != self.name.as_bytes() || info.crtc == 0 {
                continue;
            }
            let crtc = conn.randr_get_crtc_info(info.crtc, resources.config_timestamp)?.reply()?;
            if crtc.mode == 0 {
                return Ok(None);
            }
            return Ok(Some(OutputRect { x: crtc.x, y: crtc.y, width: crtc.width, height: crtc.height }));
        }
        Ok(None)
    }
}

impl Drop for ActiveOutput {
    fn drop(&mut self) {
        for (name, x, y) in &self.shifted {
            if let Err(e) = run_xrandr([
                "--output".to_string(),
                name.clone(),
                "--pos".to_string(),
                format!("{x}x{y}"),
            ]) {
                log::warn!("failed to restore output {name} to {x}x{y}: {e:#}");
            }
        }
        if let Err(e) = deactivate(&self.name) {
            log::warn!("failed to deactivate virtual output {}: {e:#}", self.name);
        }
    }
}

/// A real (non-tabscreen), currently-active output's geometry.
struct RealOutput {
    name: String,
    x: i32,
    y: i32,
    width: i32,
}

/// Finds the tabscreen `Virtual-*` output, registers a custom mode for
/// `width`x`height`@`refresh_hz` (CVT timings, via the `cvt` tool) if one
/// doesn't already exist, and positions it per `placement` relative to
/// whatever real outputs are currently active — not a hardcoded position,
/// since that depends on this machine's own monitor layout.
pub fn activate(
    conn: &RustConnection,
    root: u32,
    width: u32,
    height: u32,
    refresh_hz: u32,
    placement: Placement,
) -> anyhow::Result<ActiveOutput> {
    let output_name = find_output_name(conn, root)?;
    let resources = conn.randr_get_screen_resources_current(root)?.reply()?;

    let mode_name = format!("{width}x{height}_{refresh_hz}.00");
    if !mode_name_exists(&resources, &mode_name) {
        let params = cvt_modeline_params(width, height, refresh_hz)?;
        run_xrandr(
            std::iter::once("--newmode".to_string())
                .chain(std::iter::once(mode_name.clone()))
                .chain(params),
        )?;
    }

    let others = active_outputs_excluding(conn, &resources, &output_name)?;

    let (x, y, shifted) = match placement {
        Placement::Right => {
            let rightmost = others.iter().map(|o| o.x + o.width).max().unwrap_or(0);
            (rightmost, 0, Vec::new())
        }
        Placement::Left => {
            let leftmost = others.iter().map(|o| o.x).min().unwrap_or(0);
            for o in &others {
                run_xrandr([
                    "--output".to_string(),
                    o.name.clone(),
                    "--pos".to_string(),
                    format!("{}x{}", o.x + width as i32, o.y),
                ])?;
            }
            let shifted = others.into_iter().map(|o| (o.name, o.x, o.y)).collect();
            (leftmost, 0, shifted)
        }
    };

    run_xrandr(["--addmode".to_string(), output_name.clone(), mode_name.clone()])?;
    run_xrandr([
        "--output".to_string(),
        output_name.clone(),
        "--mode".to_string(),
        mode_name,
        "--pos".to_string(),
        format!("{x}x{y}"),
    ])?;

    log::info!(
        "activated virtual output {output_name}: {width}x{height}+{x}+{y} ({placement:?})"
    );
    Ok(ActiveOutput {
        name: output_name,
        rect: OutputRect { x: x as i16, y: y as i16, width: width as u16, height: height as u16 },
        shifted,
    })
}

/// Startup self-heal: if no tabscreen output is currently active but the
/// real outputs aren't flush against `x=0`, a prior `Placement::Left`
/// shift was left applied by something that didn't go through
/// `ActiveOutput`'s clean `Drop` path (a crash, `kill -9`, power loss).
/// Closes the gap. Best-effort — logs and moves on rather than blocking
/// startup on a RandR error.
pub fn heal_layout_gap(conn: &RustConnection, root: u32) {
    if let Err(e) = try_heal_layout_gap(conn, root) {
        log::warn!("layout gap self-heal check failed: {e:#}");
    }
}

fn try_heal_layout_gap(conn: &RustConnection, root: u32) -> anyhow::Result<()> {
    let resources = conn.randr_get_screen_resources_current(root)?.reply()?;

    // A currently-active Virtual-* output means a placement is
    // legitimately in effect right now -- don't touch anything.
    for &output in &resources.outputs {
        let info = conn.randr_get_output_info(output, resources.config_timestamp)?.reply()?;
        let name = String::from_utf8_lossy(&info.name);
        if !name.starts_with(OUTPUT_NAME_PREFIX) || info.crtc == 0 {
            continue;
        }
        let crtc = conn.randr_get_crtc_info(info.crtc, resources.config_timestamp)?.reply()?;
        if crtc.mode != 0 {
            return Ok(());
        }
    }

    let others = active_outputs_excluding(conn, &resources, "")?;
    let leftmost = others.iter().map(|o| o.x).min().unwrap_or(0);
    if leftmost <= 0 {
        return Ok(());
    }
    log::warn!(
        "found a {leftmost}px gap at the left of the screen with no active tabscreen \
         output -- closing it (likely left over from an abnormal exit while a left-side \
         placement was applied)"
    );
    for o in &others {
        run_xrandr([
            "--output".to_string(),
            o.name.clone(),
            "--pos".to_string(),
            format!("{}x{}", o.x - leftmost, o.y),
        ])?;
    }
    Ok(())
}

fn deactivate(output_name: &str) -> anyhow::Result<()> {
    run_xrandr(["--output".to_string(), output_name.to_string(), "--off".to_string()])
}

/// Best-effort: turns off any currently-active `Virtual-*` output,
/// regardless of whether an `ActiveOutput` handle is tracking it. A
/// signal (SIGINT/SIGTERM, e.g. Ctrl+C) aborts execution before
/// `ActiveOutput::drop` gets a chance to run, so `main.rs`'s signal
/// handler calls this as the fallback that leaves a clean X11 state
/// behind instead of a frozen mid-session one -- confirmed live
/// (2026-08-20) that Ctrl+C without this left the tablet output active,
/// requiring manual `xrandr --off` to recover.
pub fn deactivate_any_active(conn: &RustConnection, root: u32) {
    if let Err(e) = try_deactivate_any_active(conn, root) {
        log::warn!("cleanup on exit: failed to check/deactivate tabscreen output: {e:#}");
    }
}

fn try_deactivate_any_active(conn: &RustConnection, root: u32) -> anyhow::Result<()> {
    let resources = conn.randr_get_screen_resources_current(root)?.reply()?;
    for &output in &resources.outputs {
        let info = conn
            .randr_get_output_info(output, resources.config_timestamp)?
            .reply()?;
        let name = String::from_utf8_lossy(&info.name);
        if !name.starts_with(OUTPUT_NAME_PREFIX) || info.crtc == 0 {
            continue;
        }
        let crtc = conn
            .randr_get_crtc_info(info.crtc, resources.config_timestamp)?
            .reply()?;
        if crtc.mode != 0 {
            log::info!("cleanup on exit: deactivating active tabscreen output {name}");
            deactivate(&name)?;
        }
    }
    Ok(())
}

fn run_xrandr(args: impl IntoIterator<Item = String>) -> anyhow::Result<()> {
    let args: Vec<String> = args.into_iter().collect();
    let status = Command::new("xrandr")
        .args(&args)
        .status()
        .map_err(|e| anyhow::anyhow!("failed to spawn xrandr {args:?}: {e}"))?;
    if !status.success() {
        anyhow::bail!("xrandr {args:?} failed: {status}");
    }
    Ok(())
}

fn find_output_name(conn: &RustConnection, root: u32) -> anyhow::Result<String> {
    let resources = conn.randr_get_screen_resources_current(root)?.reply()?;
    for &output in &resources.outputs {
        let info = conn
            .randr_get_output_info(output, resources.config_timestamp)?
            .reply()?;
        let name = String::from_utf8_lossy(&info.name);
        if name.starts_with(OUTPUT_NAME_PREFIX) {
            return Ok(name.into_owned());
        }
    }
    anyhow::bail!(
        "no {OUTPUT_NAME_PREFIX}* RandR output found -- is vkms loaded \
         (/etc/modules-load.d/vkms.conf)?"
    )
}

/// Every currently-active real (non-tabscreen) output's geometry,
/// excluding `exclude_name`.
fn active_outputs_excluding(
    conn: &RustConnection,
    resources: &GetScreenResourcesCurrentReply,
    exclude_name: &str,
) -> anyhow::Result<Vec<RealOutput>> {
    let mut out = Vec::new();
    for &output in &resources.outputs {
        let info = conn
            .randr_get_output_info(output, resources.config_timestamp)?
            .reply()?;
        if info.name == exclude_name.as_bytes() || info.crtc == 0 {
            continue;
        }
        let crtc = conn
            .randr_get_crtc_info(info.crtc, resources.config_timestamp)?
            .reply()?;
        if crtc.mode == 0 {
            continue;
        }
        out.push(RealOutput {
            name: String::from_utf8_lossy(&info.name).into_owned(),
            x: crtc.x as i32,
            y: crtc.y as i32,
            width: crtc.width as i32,
        });
    }
    Ok(out)
}

fn mode_name_exists(resources: &GetScreenResourcesCurrentReply, target: &str) -> bool {
    let mut offset = 0usize;
    for mode in &resources.modes {
        let len = mode.name_len as usize;
        let Some(name_bytes) = resources.names.get(offset..offset + len) else {
            break;
        };
        if name_bytes == target.as_bytes() {
            return true;
        }
        offset += len;
    }
    false
}

/// Runs `cvt width height refresh_hz` and returns the `Modeline` line's
/// parameters (everything after the mode name) ready to pass straight to
/// `xrandr --newmode <name> <these params>`.
fn cvt_modeline_params(width: u32, height: u32, refresh_hz: u32) -> anyhow::Result<Vec<String>> {
    let output = Command::new("cvt")
        .args([width.to_string(), height.to_string(), refresh_hz.to_string()])
        .output()
        .map_err(|e| anyhow::anyhow!("failed to run cvt: {e}"))?;
    let stdout = String::from_utf8_lossy(&output.stdout);
    let line = stdout
        .lines()
        .find(|l| l.trim_start().starts_with("Modeline"))
        .ok_or_else(|| anyhow::anyhow!("cvt produced no Modeline line: {stdout}"))?;

    // `Modeline "1200x1920_60.00"  196.50  1200 1296 1424 1648  1920 1923 1933 1989 -hsync +vsync`
    let rest = line
        .trim_start()
        .strip_prefix("Modeline")
        .unwrap()
        .trim_start();
    let rest = rest
        .strip_prefix('"')
        .ok_or_else(|| anyhow::anyhow!("unexpected cvt Modeline format: {line}"))?;
    let name_end = rest
        .find('"')
        .ok_or_else(|| anyhow::anyhow!("unexpected cvt Modeline format: {line}"))?;
    Ok(rest[name_end + 1..].split_whitespace().map(String::from).collect())
}
