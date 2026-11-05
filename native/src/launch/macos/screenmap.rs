//! World map point -> game screen point from the live camera (macOS 1.19.4.1).
//! The camera object (idler +0x718) holds the inverse projection at +0x110 (column-major;
//! [0] and [5] are tan(fov/2) horizontally and vertically) and the camera-to-world transform
//! at +0xd0 (rotation columns, translation = eye), the same blocks the construction ray uses.
//! Read live 2 November 2026: eye (3104, 50, 1577), look-at (3104, 9.5, 1607), fov y ~45 deg.
use crate::{
    error::{Error, Result},
    game,
    memory::Reader,
    xlayout::{self, fld},
};

const GAME: (f64, f64) = (1920.0, 1080.0);
/// Height of the map plane used for province points (the camera look-at height).
const GROUND: f64 = 9.5;
/// Owners of the camera fields (macOS 1.19.4.1; no reviewed camera vtable).
const IDLER: xlayout::Class = xlayout::Class::new("CInGameIdler", 0x35089d8);
const CAMERA: xlayout::Class = xlayout::Class::new("idler.camera", 0);

fn floats<const N: usize>(r: &Reader<'_>, at: u64) -> Result<[f64; N]> {
    let bytes = r.read(at, 4 * N)?;
    let mut out = [0.0; N];
    for (k, c) in bytes.chunks_exact(4).enumerate() {
        let v = f32::from_le_bytes(c.try_into().unwrap());
        if !v.is_finite() || v.abs() > 1.0e6 {
            return Err(Error::new(
                "camera_unavailable",
                "Camera matrices not finite",
            ));
        }
        out[k] = f64::from(v);
    }
    Ok(out)
}

/// Screen point (game pixels) of map point (x, z), or None behind the camera.
pub(super) fn to_screen(r: &Reader<'_>, image: u64, x: f64, z: f64) -> Result<Option<(f64, f64)>> {
    let idler = r.u64(image + game::layout(r).idler_global)?;
    let abi = r.string_abi();
    let camera = r.u64(idler + fld(abi, &IDLER, 0x718)?)?;
    let proj: [f64; 16] = floats(r, camera + fld(abi, &CAMERA, 0x110)?)?;
    let view: [f64; 16] = floats(r, camera + fld(abi, &CAMERA, 0xd0)?)?;
    Ok(project(&proj, &view, x, z))
}

/// Camera eye height above the map (larger = zoomed out).
pub(super) fn camera_height(r: &Reader<'_>, image: u64) -> Result<f64> {
    let idler = r.u64(image + game::layout(r).idler_global)?;
    let abi = r.string_abi();
    let camera = r.u64(idler + fld(abi, &IDLER, 0x718)?)?;
    let view: [f64; 16] = floats(r, camera + fld(abi, &CAMERA, 0xd0)?)?;
    Ok(view[13])
}

/// Zoomed in closer than this, the camera is brought back out after a map action.
const CLOSE: f64 = 500.0;
/// Strategic view height (the game starts at 800; 50 is fully in, 3000 fully out).
const STRATEGIC: f64 = 750.0;
/// Scroll the map camera back out to the strategic height when an action left it close in.
pub(super) fn restore_zoom(
    process: &dyn crate::platform::Process,
    keys: &dyn crate::platform::game_input::GameInput,
) -> Result<()> {
    let height = |p: &dyn crate::platform::Process| super::ui::read(p, camera_height);
    let h = height(process)?;
    if h >= CLOSE {
        return Ok(());
    }
    // One swoop: a single quick flick of wheel events that the game eases into one camera
    // movement (one event zooms about 10%, measured live 5 November 2026), then at most one
    // short correction.
    let events = |from: f64| {
        ((STRATEGIC / from.max(1.0)).ln() / 1.1f64.ln())
            .ceil()
            .clamp(1.0, 40.0) as u32
    };
    keys.scroll_burst(super::panelkit::GAME, (960.0, 540.0), -120, events(h))?;
    let now = height(process)?;
    if now < h - 1.0 {
        // This input backend scrolls the other way: undo and swoop the right way.
        keys.scroll_burst(
            super::panelkit::GAME,
            (960.0, 540.0),
            120,
            events(now) + events(h),
        )?;
    } else if now < STRATEGIC * 0.8 {
        keys.scroll_burst(super::panelkit::GAME, (960.0, 540.0), -120, events(now))?;
    }
    Ok(())
}

pub(super) fn project(proj: &[f64; 16], view: &[f64; 16], x: f64, z: f64) -> Option<(f64, f64)> {
    let (tx, ty) = (proj[0], proj[5]);
    if tx <= 0.0 || ty <= 0.0 {
        return None;
    }
    let v = [x - view[12], GROUND - view[13], z - view[14]];
    let col = |k: usize| [view[4 * k], view[4 * k + 1], view[4 * k + 2]];
    let dot = |a: [f64; 3]| a[0] * v[0] + a[1] * v[1] + a[2] * v[2];
    let (cx, cy, cz) = (dot(col(0)), dot(col(1)), dot(col(2)));
    if cz <= 0.01 {
        return None;
    }
    let nx = cx / cz / tx;
    let ny = cy / cz / ty;
    Some(((nx + 1.0) * GAME.0 / 2.0, (1.0 - ny) * GAME.1 / 2.0))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn the_look_at_point_projects_to_the_screen_centre() {
        let proj = [
            0.736, 0.0, 0.0, 0.0, 0.0, 0.414, 0.0, 0.0, 0.0, 0.0, 0.0, -0.1997, 0.0, 0.0, 1.0, 0.2,
        ];
        let view = [
            1.0, 0.0, 0.0, 0.0, 0.0, 0.5952, 0.8036, 0.0, 0.0, -0.8036, 0.5952, 0.0, 3103.888,
            50.0, 1576.98, 1.0,
        ];
        let (x, y) = project(&proj, &view, 3104.0, 1607.0).unwrap();
        assert!(
            (x - 960.0).abs() < 5.0 && (y - 540.0).abs() < 5.0,
            "{x} {y}"
        );
        // East (larger x) is to the right; north (larger z) is up the screen.
        assert!(project(&proj, &view, 3114.0, 1607.0).unwrap().0 > x);
        assert!(project(&proj, &view, 3104.0, 1617.0).unwrap().1 < y);
    }
}
