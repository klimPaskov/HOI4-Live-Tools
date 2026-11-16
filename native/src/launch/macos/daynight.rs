//! hoi4_map_daynight: the map's day/night shading on or off. The game starts every session with
//! it on and does not save it; at night the map is dark and units, fronts and state colours are
//! hard to read. The toggle sits in the anchored map controls at the bottom right
//! (interface/mapmodes_interface.gui `day_night_toggle`, live at 1857,1062 of a 1920x1080 game
//! since the w-runs). The state is read back from the map's brightness: shading off brightens a
//! map area in night; in daylight nothing changes and the result says so.
use crate::{
    error::{Error, Result},
    platform::game_input::{Capture, GameInput},
};
use serde_json::{json, Value};

pub(super) const TOOL: &str = "hoi4_map_daynight";
const GAME: (u32, u32) = (1920, 1080);
/// The toggle, from the bottom-right corner of the game area.
const TOGGLE_FROM_CORNER: (f64, f64) = (63.0, 18.0);
/// A change of the map's mean brightness (0..255) that counts as visible.
const VISIBLE: f64 = 1.0;

pub(super) fn tool() -> Value {
    json!({"name": TOOL, "description": "Turn the map's day/night shading off (on:false, the default) or on: the game starts every session with it on, and at night the map is dark, so units, fronts and state colours are hard to read. One click on the toggle in the bottom-right map controls; the result is read back from the map's brightness (shading off brightens a map in night). In daylight nothing changes on screen: the result says no_visible_change, and the call is best made when the map shows night. A view change only.", "inputSchema": {"type": "object", "properties": {"on": {"type": "boolean"}, "lease_token": {"type": "string"}}, "required": ["lease_token"], "additionalProperties": false}})
}

/// Mean brightness of the map area (a grid of samples clear of the top bar, the outliner and
/// the bottom panels).
pub(super) fn brightness(c: &Capture) -> f64 {
    let (mut sum, mut n) = (0.0, 0.0);
    let mut y = 160.0;
    while y < 900.0 {
        let mut x = 300.0;
        while x < 1600.0 {
            if let Some((r, g, b)) = c.rgb(x, y) {
                sum += (r as f64 + g as f64 + b as f64) / 3.0;
                n += 1.0;
            }
            x += 25.0;
        }
        y += 25.0;
    }
    if n > 0.0 {
        sum / n
    } else {
        0.0
    }
}

/// What one click did, from the brightness before and after, for the state wanted.
/// (state, click back): shading `off` wanted and the map got darker means it was off already.
pub(super) fn judge(want_on: bool, before: f64, after: f64) -> (&'static str, bool) {
    let change = after - before;
    if change.abs() <= VISIBLE {
        return ("no_visible_change", false);
    }
    // brighter after the click: shading went off
    let now_off = change > 0.0;
    if now_off != want_on {
        (if want_on { "on" } else { "off" }, false)
    } else {
        (if want_on { "was_on" } else { "was_off" }, true)
    }
}

pub(super) fn run(keys: &dyn GameInput, want_on: bool) -> Result<Value> {
    keys.preflight()?;
    let at = (
        GAME.0 as f64 - TOGGLE_FROM_CORNER.0,
        GAME.1 as f64 - TOGGLE_FROM_CORNER.1,
    );
    let before = brightness(&keys.capture(GAME)?);
    keys.click(GAME, at.0, at.1)?;
    std::thread::sleep(std::time::Duration::from_millis(1200));
    let after = brightness(&keys.capture(GAME)?);
    let (state, back) = judge(want_on, before, after);
    if back {
        keys.click(GAME, at.0, at.1)?;
        std::thread::sleep(std::time::Duration::from_millis(600));
    }
    let round = |v: f64| (v * 10.0).round() / 10.0;
    if before == 0.0 && after == 0.0 {
        return Err(Error::new(
            "input_readback_failed",
            "The map capture held no pixels; the shading state is unknown",
        ));
    }
    Ok(
        json!({"day_night": state, "wanted": if want_on {"on"} else {"off"},
        "clicked_back": back, "brightness_before": round(before), "brightness_after": round(after),
        "note": if state == "no_visible_change" {
            "the shown map is in daylight, so the shading change is not visible: the toggle was clicked once; call again when the map shows night to confirm"
        } else { "" }}),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_click_is_judged_by_how_the_map_brightness_moved() {
        assert_eq!(judge(false, 40.0, 70.0), ("off", false));
        assert_eq!(judge(false, 70.0, 40.0), ("was_off", true));
        assert_eq!(judge(true, 70.0, 40.0), ("on", false));
        assert_eq!(judge(true, 40.0, 70.0), ("was_on", true));
        assert_eq!(judge(false, 80.0, 80.5), ("no_visible_change", false));
    }

    #[test]
    fn brightness_is_the_mean_of_the_map_samples() {
        let c = Capture {
            width: 1920,
            height: 1080,
            stride: 1920 * 4,
            scale: 1.0,
            data: vec![90; 1920 * 1080 * 4],
        };
        assert!((brightness(&c) - 90.0).abs() < 1e-9);
    }
}
