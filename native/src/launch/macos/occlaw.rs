//! Default occupation law (the "Territory Management" law of the Occupied Territories view),
//! set the way a player does it: an occupied or resisting state's view, its occupation-law
//! button (opens the Occupied Territories view), the view's own law button (opens the
//! Default Law list), then the law's row. Verified from the country's occupation law in memory
//! (`CCountryOccupationStatus +0x108`, the key `hoi4_occupation` reports).
//!
//! The Default Law list is not reachable from the interface manager; its rows are clicked by
//! their fixed place at 1920x1080 and GUI scale 1 (mapped live 5 November 2026: the list's
//! first row at y 187, rows 38 apart, in the game's order below). The readback decides.
use super::{
    stateslots,
    ui::{clear_notices, read, wait},
};
use crate::{
    error::{Error, Result},
    game, gui_tree,
    memory::Reader,
    platform::{
        game_input::{GameInput, Key},
        Process,
    },
    xlayout::{fld, Class},
};
use serde_json::{json, Value};

const GAME: (u32, u32) = super::panelkit::GAME;
/// Laws in the order of the Default Law list (key, shown name).
pub(super) const LAWS: [(&str, &str); 9] = [
    ("no_garrison", "No Garrison"),
    ("foreign_civilian_oversight", "Civilian Oversight"),
    ("local_police_force_garrison", "Local Police Force"),
    ("secret_police_oversight", "Secret Police"),
    ("military_governor_occupation", "Military Governor"),
    ("martial_law_occupation", "Martial Law"),
    ("forced_labor_occupation", "Forced Labor"),
    ("harsh_quotas_occupation", "Harsh Quotas"),
    ("brutally_oppressive_occupation", "Brutal Oppression"),
];
const ROW_X: f64 = 700.0;
const FIRST_ROW_Y: f64 = 187.0;
const ROW_STEP: f64 = 37.8;

const IDLER: Class = Class::new("CInGameIdler", 0x35089d8);
const MANAGER: Class = Class::new("idler.manager", 0);
const OCCUPATION_VIEW: Class = Class::new("occupation.view", 0);
const IDLER_MANAGER: u64 = 0x670;
const VIEW_SLOT: u64 = 0x120;
const VIEW_ROOT: u64 = 0x490;

/// The shown `select_law_button` of the Occupied Territories view's header, when the view is
/// on screen.
fn header_law_button(r: &Reader<'_>, image: u64) -> Result<Option<gui_tree::Rect>> {
    let abi = r.string_abi();
    let idler = r.u64(image + game::layout(r).idler_global)?;
    let manager = r.u64(idler + fld(abi, &IDLER, IDLER_MANAGER)?)?;
    let view = r.u64(manager + fld(abi, &MANAGER, VIEW_SLOT)?)?;
    if view == 0 {
        return Ok(None);
    }
    let Some(root) = gui_tree::container(
        r,
        r.u64(view + fld(abi, &OCCUPATION_VIEW, VIEW_ROOT)?)?,
        image,
    )?
    else {
        return Ok(None);
    };
    let Some(header) = super::panelkit::find(r, image, root, "header", 1)? else {
        return Ok(None);
    };
    let Some(button) = super::panelkit::find(r, image, header, "select_law_button", 1)? else {
        return Ok(None);
    };
    if !gui_tree::shown(r, button)? {
        return Ok(None);
    }
    match gui_tree::rect(r, image, button, GAME) {
        Ok(rect) if rect.x >= 0 => Ok(Some(rect)),
        _ => Ok(None),
    }
}

pub(super) fn tool() -> Value {
    let keys: Vec<&str> = LAWS.iter().map(|(k, _)| *k).collect();
    json!({"name":"hoi4_occupation_law","description":format!("Set the DEFAULT occupation law for all occupied territories (the Occupied Territories view's Territory Management law) with ordinary input: an occupied or resisting state's view, its occupation-law button, the default-law button, the law's row. Verified from the country's occupation law in memory (hoi4_occupation player_occupation_law). Laws, mild to harsh: {}. Harsher laws cut resistance and raise the garrison need; milder ones raise compliance slowly and need fewer garrison troops. A law the game disables for you (ideology, DLC) is not applied: the readback refuses it with action_not_applied. `via_state` is an occupied or resisting state id (default: the first one hoi4_occupation lists).", keys.join(", ")),"inputSchema":{"type":"object","properties":{"law":{"type":"string","enum":keys},"via_state":{"type":"integer","minimum":1,"maximum":4095},"request_id":{"type":"string"},"lease_token":{"type":"string"}},"required":["law","request_id","lease_token"],"additionalProperties":false}})
}

/// Open the Default Law list and click `law`'s row. The caller verifies the law.
pub(super) fn set_default(
    process: &dyn Process,
    keys: &dyn GameInput,
    state: i32,
    law: &str,
) -> Result<()> {
    let row = LAWS
        .iter()
        .position(|(k, _)| *k == law)
        .ok_or_else(|| Error::new("invalid_arguments", "Unknown occupation law"))?;
    if read(process, header_law_button)?.is_none() {
        // The game's resistance alert opens the view of a resisting state with its law button
        // shown; a state opened from the map does not always show it.
        match read(process, |r, i| super::alerts::rect_of(r, i, "resistance"))? {
            Some([x, y, w, h]) => {
                clear_notices(process, keys)?;
                keys.click(GAME, (x + w / 2) as f64, (y + h / 2) as f64)?;
                std::thread::sleep(std::time::Duration::from_millis(900));
            }
            None => {
                stateslots::open_state_view(process, keys, state)?;
            }
        }
        let mut button = None;
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(3);
        while button.is_none() && std::time::Instant::now() < deadline {
            button = read(process, |r, i| {
                Ok(
                    match stateslots::state_view_widget(r, i, "select_law_button")? {
                        Some(w) => gui_tree::rect(r, i, w, GAME).ok().filter(|x| x.x >= 0),
                        None => None,
                    },
                )
            })?;
            if button.is_none() {
                std::thread::sleep(std::time::Duration::from_millis(150));
            }
        }
        let Some(button) = button else {
            let shown = read(process, stateslots::state_view_name).ok().flatten();
            return Err(Error::new(
                "invalid_target",
                format!("State {state} shows no occupation law (not an occupied or resisting state?); state view shows {shown:?}"),
            ));
        };
        // Let the slide-in finish before clicking.
        std::thread::sleep(std::time::Duration::from_millis(300));
        let button = read(process, |r, i| {
            let w = stateslots::state_view_widget(r, i, "select_law_button")?
                .ok_or_else(|| Error::new("input_readback_failed", "State view closed"))?;
            gui_tree::rect(r, i, w, GAME)
        })
        .unwrap_or(button);
        let (x, y) = button.center();
        keys.click(GAME, x, y)?;
        wait(process, "occupied territories view", |r, i| {
            Ok(header_law_button(r, i)?.is_some())
        })?;
        std::thread::sleep(std::time::Duration::from_millis(400));
    }
    let header = read(process, header_law_button)?
        .ok_or_else(|| Error::new("input_readback_failed", "Occupied territories view closed"))?;
    let (x, y) = header.center();
    keys.click(GAME, x, y)?;
    std::thread::sleep(std::time::Duration::from_millis(600));
    keys.click(GAME, ROW_X, FIRST_ROW_Y + ROW_STEP * row as f64)?;
    std::thread::sleep(std::time::Duration::from_millis(400));
    Ok(())
}

/// Close the Occupied Territories view (Escape closes it with its list).
pub(super) fn close(process: &dyn Process, keys: &dyn GameInput) {
    if read(process, header_law_button).ok().flatten().is_some() {
        let _ = keys.press(Key::Escape);
    }
    let _ = clear_notices(process, keys);
}
