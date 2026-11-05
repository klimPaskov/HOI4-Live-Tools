//! The game's own alert icons (top bar): what a human player sees as the row of warnings.
//! Read-only. Each alert window under the top bar root (interface manager +0x188, root +0x68)
//! is a `global_alerticon_window` whose `icon` button draws one frame of
//! GFX_global_alert_icons (70 frames); the button's sprite holds the frame plus one at +0x34
//! (mapped live 5 November 2026: every label below was read from the game tooltip of its icon).
use super::ui::read;
use crate::{
    error::{Error, Result},
    game, gui_tree,
    memory::Reader,
    platform::Process,
    xlayout::{fld, Class},
};
use serde_json::{json, Value};

const TOPBAR_SLOT: u64 = 0x188;
const TOPBAR_ROOT: u64 = 0x68;
const BUTTON_SPRITE: u64 = 0xe0;
const SPRITE_FRAME: u64 = 0x34;
const IDLER_MANAGER: u64 = 0x670;
const SPRITE_X: u64 = 0xb0;
const IDLER: Class = Class::new("CInGameIdler", 0x35089d8);
const MANAGER: Class = Class::new("idler.manager", 0);
const TOPBAR: Class = Class::new("CTopBar", 0x3697918);
const BUTTON: Class = Class::new("CButtonStandard", 0x36b9738);
const SPRITE: Class = Class::new("CSprite", 0x36cc528);

/// Name of a frame of GFX_global_alert_icons where the icon identifies the alert, with what
/// to do about it. Unlisted frames are reported by their frame number only.
pub(super) fn label(frame: u32) -> Option<(&'static str, &'static str)> {
    Some(match frame {
        1 => ("no_national_focus", "no focus is running: hoi4_focus_start"),
        2 => ("at_war", "we are at war"),
        7 => ("free_dockyards", "idle dockyards: naval production lines"),
        8 => (
            "missing_equipment_production",
            "logistics requests equipment nothing produces: hoi4_production_add",
        ),
        9 => (
            "modify_government",
            "a law, advisor or idea can be picked: hoi4_politics_options",
        ),
        10 => (
            "unfulfilled_import_requests",
            "an import request is not filled: hoi4_trade / hoi4_auto_import",
        ),
        21 => (
            "air_wings_without_mission",
            "air wings have no mission or region: hoi4_air_mission",
        ),
        26 => (
            "no_divisions_in_training",
            "nothing is training: hoi4_train",
        ),
        30 => (
            "resistance",
            "resistance is high in occupied states: hoi4_occupation",
        ),
        39 => (
            "unassigned_divisions",
            "divisions are in no army: hoi4_army_assign / hoi4_army_create",
        ),
        42 => (
            "decisions_available",
            "decisions can be taken: hoi4_decisions_list",
        ),
        43 => (
            "events_or_decisions_timeout",
            "events or decisions are about to time out: hoi4_events / hoi4_decisions_list",
        ),
        51 => (
            "not_enough_garrison",
            "not enough manpower or equipment for garrisons",
        ),
        53 => (
            "doctrines_available",
            "a doctrine can be unlocked: hoi4_doctrines",
        ),
        54 => (
            "modify_officer_corps",
            "a high command or military advisor can be picked: hoi4_politics_options",
        ),
        57 => (
            "mio_traits",
            "military industrial organisation traits can be unlocked: hoi4_mios",
        ),
        67 => (
            "admiral_assignable",
            "an admiral can be assigned to a naval headquarters: hoi4_navy_admirals",
        ),
        _ => return None,
    })
}

pub(super) fn tool() -> Value {
    json!({"name":"hoi4_alerts","description":"Read-only: the game's own alert icons in the top bar, left to right (what a human sees as the warning row): each with its icon frame and, where the icon identifies it, the alert name and what to do (e.g. no_national_focus, free_dockyards, missing_equipment_production, modify_government, air_wings_without_mission, no_divisions_in_training, unassigned_divisions, decisions_available, doctrines_available, resistance), plus its screen rect (\"at\"); hovering an icon shows the game tooltip. Use it every tick: every alert is work a strong player would not leave undone. No input, no lease.","inputSchema":{"type":"object","properties":{},"additionalProperties":false}})
}

pub(super) fn alerts(process: &dyn Process) -> Result<Value> {
    read(process, |r, i| {
        list(r, i).map(|rows| json!({"alerts": rows, "count": rows.len()}))
    })
}

fn list(r: &Reader<'_>, image: u64) -> Result<Vec<Value>> {
    let abi = r.string_abi();
    let idler = r.u64(image + game::layout(r).idler_global)?;
    let manager = r.u64(idler + fld(abi, &IDLER, IDLER_MANAGER)?)?;
    let topbar = r.u64(manager + fld(abi, &MANAGER, TOPBAR_SLOT)?)?;
    let root = gui_tree::container(r, r.u64(topbar + fld(abi, &TOPBAR, TOPBAR_ROOT)?)?, image)?
        .ok_or_else(|| Error::new("invalid_gui", "Top bar root unavailable"))?;
    let mut found: Vec<(f32, u32, Option<[i32; 4]>)> = Vec::new();
    let mut stack = vec![(root, 0u32)];
    while let Some((object, depth)) = stack.pop() {
        for child in gui_tree::children(r, object)? {
            if gui_tree::name(r, child, image)?.as_deref() == Some("global_alerticon_window") {
                let Some(window) = gui_tree::container(r, child, image)? else {
                    continue;
                };
                let Some(icon) = gui_tree::child(r, image, window, "icon")? else {
                    continue;
                };
                if !gui_tree::shown(r, icon)? {
                    continue;
                }
                let sprite = r.u64(icon + fld(abi, &BUTTON, BUTTON_SPRITE)?)?;
                let stored = r.i32(sprite + fld(abi, &SPRITE, SPRITE_FRAME)?)?;
                if !(1..=70).contains(&stored) {
                    continue;
                }
                let x = f32::from_le_bytes(
                    r.read(sprite + fld(abi, &SPRITE, SPRITE_X)?, 4)?
                        .try_into()
                        .unwrap(),
                );
                let at = gui_tree::rect(r, image, icon, super::panelkit::GAME)
                    .ok()
                    .map(|g| [g.x, g.y, g.w, g.h]);
                found.push((x, (stored - 1) as u32, at));
            } else if depth < 6 {
                if let Some(inner) = gui_tree::container(r, child, image)? {
                    stack.push((inner, depth + 1));
                }
            }
        }
    }
    found.sort_by(|a, b| a.0.total_cmp(&b.0));
    Ok(found
        .into_iter()
        .map(|(_, frame, at)| match label(frame) {
            Some((name, todo)) => json!({"frame": frame, "alert": name, "todo": todo, "at": at}),
            None => json!({"frame": frame, "alert": null, "todo": "unlabelled alert: hover it in game or check every domain", "at": at}),
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn labels_cover_the_common_alerts() {
        for f in [1, 7, 8, 9, 21, 26, 39, 42, 53] {
            assert!(label(f).is_some(), "{f}");
        }
        assert!(label(69).is_none());
    }
}
