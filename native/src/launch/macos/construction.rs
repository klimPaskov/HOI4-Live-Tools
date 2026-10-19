//! Queue own state buildings with ordinary input: construction panel (T), the
//! building's button, the game's own Find view (F, the state ID, Return) to
//! centre the camera on the state, then clicks at the screen centre. Success
//! is the own construction queue holding the requested extra amount of that
//! building in that state; placement mode is switched off again afterwards.
use super::ui::{clear_notices, find_state, read, settled, wait};
use crate::{
    construction,
    error::{Error, Result},
    game,
    gui_tree::{self, Rect},
    memory::{Reader, StringAbi},
    platform::{
        macos_input::{Key, MacKeys},
        Process,
    },
};
use std::{thread, time::Duration};

struct Ui {
    manager: u64,
    view: u64,
    view_root: u64,
    grid_items: u64,
    item_vtable: u64,
    item_container: u64,
    item_template: u64,
    template_key: u64,
    selected_template: u64,
    selection_mode: u64,
}
/// macOS Mach-O 089EA519, mapped live on 18 October 2026 (Germany test run):
/// choice items and their templates, the manager's selection (template
/// +0x3b8, mode +0x3c0, both 0x30 below Linux) by a select/Escape diff, and
/// the Find view's idler/window joins by aligned methods and a live scan.
const MACOS: Ui = Ui {
    manager: 0x670,
    view: 0x150,
    view_root: 0x430,
    grid_items: 0x190,
    item_vtable: 0x343a9b0,
    item_container: 0x10,
    item_template: 0x20,
    template_key: 0x228,
    selected_template: 0x3b8,
    selection_mode: 0x3c0,
};
const GAME: (u32, u32) = (1920, 1080);
const GRIDS: [(&str, &str); 4] = [
    ("possible_state_constructions", "non_shared_item_grid"),
    ("possible_shared_constructions", "shared_item_grid"),
    ("possible_province_constructions", "province_item_grid"),
    ("possible_facility_constructions", "facility_item_grid"),
];

fn ui(r: &Reader<'_>) -> Result<&'static Ui> {
    match r.string_abi() {
        StringAbi::LibCxx => Ok(&MACOS),
        StringAbi::LibStdCxx => Err(Error::new(
            "capability_unavailable",
            "Construction input is not reviewed for this build",
        )),
    }
}
fn manager(r: &Reader<'_>, image: u64) -> Result<u64> {
    let idler = r.u64(image + game::layout(r).idler_global)?;
    r.u64(idler + ui(r)?.manager)
}
fn root(r: &Reader<'_>, image: u64) -> Result<u64> {
    let u = ui(r)?;
    let view = r.u64(manager(r, image)? + u.view)?;
    gui_tree::container(r, r.u64(view + u.view_root)?, image)?
        .ok_or_else(|| Error::new("invalid_gui", "Construction view root unavailable"))
}
fn open(r: &Reader<'_>, image: u64) -> Result<bool> {
    gui_tree::visible(r, root(r, image)?)
}
/// (mode, selected building key) of the construction selection.
fn selection(r: &Reader<'_>, image: u64) -> Result<(i32, Option<String>)> {
    let u = ui(r)?;
    let m = manager(r, image)?;
    let mode = r.i32(m + u.selection_mode)?;
    let selected = r.u64(m + u.selected_template)?;
    let key = if mode != 0 && selected != 0 {
        Some(r.string_bounded(selected + u.template_key, 128)?)
    } else {
        None
    };
    Ok((mode, key))
}
fn button(r: &Reader<'_>, image: u64, key: &str) -> Result<Option<Rect>> {
    let u = ui(r)?;
    let view = root(r, image)?;
    let possible = gui_tree::child(r, image, view, "possible_constructions")?
        .and_then(|c| gui_tree::container(r, c, image).transpose())
        .transpose()?
        .ok_or_else(|| Error::new("gui_target_unavailable", "Building choices not found"))?;
    for (section, grid) in GRIDS {
        let Some(section) = gui_tree::child(r, image, possible, section)?
            .and_then(|c| gui_tree::container(r, c, image).transpose())
            .transpose()?
        else {
            continue;
        };
        let Some(grid) = gui_tree::child(r, image, section, grid)? else {
            continue;
        };
        let (_, data, count) = r.array_header(grid + u.grid_items)?;
        for index in 0..count.min(64) as u64 {
            let item = r.u64(data + index * 8)?;
            if r.u64(item)? != image + u.item_vtable {
                continue;
            }
            let template = r.u64(item + u.item_template)?;
            if template == 0 || r.string_bounded(template + u.template_key, 128)? != key {
                continue;
            }
            let frame = gui_tree::container(r, r.u64(item + u.item_container)?, image)?
                .ok_or_else(|| Error::new("invalid_gui", "Choice window required"))?;
            if let Some(start) = gui_tree::child(r, image, frame, "start_construction_button")? {
                return Ok(Some(gui_tree::rect(r, image, start, GAME)?));
            }
        }
    }
    Ok(None)
}

/// Queued amount of `key` in state `state` (own located building lines).
pub(super) fn queued(process: &dyn Process, key: &str, state: i32) -> Result<i64> {
    // The queue is rewritten on game days; retry a torn snapshot.
    let mut attempt = 0;
    let value = loop {
        match construction::page_with_location(process, 0, 128, true) {
            Err(e) if e.code == "torn_read" && attempt < 8 => {
                attempt += 1;
                thread::sleep(Duration::from_millis(10 * attempt));
            }
            result => break result?,
        }
    };
    Ok(value["lines"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|l| {
            l["building"]["building_id"].as_str() == Some(key)
                && l["location"]["state_id"].as_i64() == Some(i64::from(state))
        })
        .map(|l| l["amount_raw_signed"].as_i64().unwrap_or(0))
        .sum())
}
pub(super) fn leave(process: &dyn Process, keys: &MacKeys) {
    // Switch placement mode off: Escape closes the selection, then the
    // panel; each Escape is given time to act so none reaches the game menu.
    super::ui::dismiss(process, keys, |r, i| {
        Ok(selection(r, i)?.0 == 0 && !open(r, i)?)
    });
}

/// Queue `count` more of building `key` in own state `state`.
pub(super) fn add(
    process: &dyn Process,
    keys: &MacKeys,
    key: &str,
    state: i32,
    count: i64,
) -> Result<()> {
    clear_notices(process, keys)?;
    let start = queued(process, key, state)?;
    if !read(process, open)? {
        keys.press(Key::letter('t')?)?;
        wait(process, "open construction", open)?;
    }
    let rect = settled(process, "building button", |r, i| {
        button(r, i, key)?.ok_or_else(|| {
            Error::new(
                "invalid_target",
                "This building is not offered in the construction panel",
            )
        })
    })?;
    super::ui::dismiss_notices(process, keys)?;
    let (x, y) = rect.center();
    keys.click(GAME, x, y)?;
    wait(process, "select building", |r, i| {
        Ok(selection(r, i)?.1.as_deref() == Some(key))
    })?;
    let result = (|| {
        find_state(process, keys, state)?;
        for _ in 0..count.min(20) {
            if read(process, selection)?.1.as_deref() != Some(key) {
                return Err(Error::new(
                    "input_readback_failed",
                    "Placement mode ended before all clicks",
                ));
            }
            let before = queued(process, key, state)?;
            super::ui::dismiss_notices(process, keys)?;
            keys.click(GAME, 960.0, 540.0)?;
            let deadline = std::time::Instant::now() + Duration::from_secs(3);
            loop {
                if queued(process, key, state)? > before {
                    break;
                }
                if std::time::Instant::now() >= deadline {
                    return Err(Error::new(
                        "input_readback_failed",
                        "The centre click queued nothing more in this state (no free building slot, or the building is not allowed there)",
                    ));
                }
                thread::sleep(Duration::from_millis(40));
            }
            if queued(process, key, state)? >= start + count {
                return Ok(());
            }
        }
        Err(Error::new(
            "input_readback_failed",
            "Requested amount not reached",
        ))
    })();
    leave(process, keys);
    result
}
