//! Air wings and their missions with ordinary input. The Air Forces
//! Overview (L) lists own wings with base, region and active mission icons;
//! a wing is chosen by its row, a mission by its toolbar button (identified
//! by the icon frame, which is the engine's mission index + 1), and a region
//! by a right click on a state the Find view has centred. Verified from the
//! overview: the wing shows a region and the mission's icon.
use super::ui::{clear_notices, find_state, read, settled, wait};
use crate::{
    error::{Error, Result},
    game,
    gui_tree::{self, Rect},
    memory::{Reader, StringAbi},
    platform::{
        macos_input::{Key, MacKeys},
        Process,
    },
};
use serde_json::{json, Value};

struct Ui {
    manager: u64,
    overview: u64,
    overview_root: u64,
    selection: u64,
    selection_root: u64,
    container_vtable: u64,
    container_base: u64,
    grid_items: u64,
    item_container: u64,
    text: u64,
    icon_sprite: u64,
    sprite_frame: u64,
}
/// macOS Mach-O 089EA519, mapped live on 18 October 2026 (Germany test run):
/// the military overview (+0x158) shows air wings in air_units/item_grid;
/// the air selection view (+0x200) holds missions_grid and the toolbar; a
/// fighter wing given Air Superiority over Brandenburg showed "Eastern
/// Germany" and frame 1.
const MACOS: Ui = Ui {
    manager: 0x670,
    overview: 0x158,
    overview_root: 0x430,
    selection: 0x200,
    selection_root: 0x28,
    container_vtable: 0x34ac0b0,
    container_base: 0x30,
    grid_items: 0x190,
    item_container: 0x10,
    text: 0x100,
    icon_sprite: 0xd0,
    sprite_frame: 0x34,
};
const GAME: (u32, u32) = (1920, 1080);
/// Visible band of the overview's wing rows.
const ROWS: (i32, i32) = (380, 940);

/// Mission names and their icon frames (AIR_MISSION_SPOTTING_FACTORS order + 1).
pub(super) const MISSIONS: [(&str, i32); 14] = [
    ("air_superiority", 1),
    ("cas", 2),
    ("interception", 3),
    ("strategic_bombing", 4),
    ("naval_strike", 5),
    ("kamikaze", 8),
    ("port_strike", 9),
    ("logistics_strike", 10),
    ("air_supply", 11),
    ("training", 12),
    ("naval_mines", 13),
    ("naval_minesweeping", 14),
    ("recon", 15),
    ("naval_patrol", 16),
];
fn mission_name(frame: i32) -> Option<&'static str> {
    MISSIONS.iter().find(|(_, f)| *f == frame).map(|(n, _)| *n)
}

fn ui(r: &Reader<'_>) -> Result<&'static Ui> {
    match r.string_abi() {
        StringAbi::LibCxx => Ok(&MACOS),
        StringAbi::LibStdCxx => Err(Error::new(
            "capability_unavailable",
            "Air input is not reviewed for this build",
        )),
    }
}
pub(super) fn view_root(r: &Reader<'_>, image: u64, slot: u64, root: u64) -> Result<u64> {
    let u = ui(r)?;
    let idler = r.u64(image + game::layout(r).idler_global)?;
    let view = r.u64(r.u64(idler + u.manager)? + slot)?;
    let object = r.u64(view + root)?;
    if r.u64(object)? != image + u.container_vtable {
        return Err(Error::new("invalid_gui", "Air view root required"));
    }
    gui_tree::container(r, object + u.container_base, image)?
        .ok_or_else(|| Error::new("invalid_gui", "Air view root required"))
}
pub(super) fn named_child(r: &Reader<'_>, image: u64, object: u64, name: &str) -> Result<Option<u64>> {
    for child in gui_tree::children(r, object)? {
        if gui_tree::name(r, child, image)?.as_deref() == Some(name) {
            return gui_tree::container(r, child, image);
        }
    }
    Ok(None)
}
pub(super) fn text(r: &Reader<'_>, base: u64) -> Result<String> {
    r.string_bounded(base + ui(r)?.text, 128)
}
pub(super) fn frame(r: &Reader<'_>, icon: u64) -> Result<i32> {
    let u = ui(r)?;
    r.i32(r.u64(icon + u.icon_sprite)? + u.sprite_frame)
}
pub(super) fn entry_frame(r: &Reader<'_>, image: u64, item: u64) -> Result<Option<u64>> {
    let u = ui(r)?;
    let entry = r.u64(item + u.item_container)?;
    let base = if r.u64(entry)? == image + u.container_vtable {
        entry + u.container_base
    } else {
        entry
    };
    gui_tree::container(r, base, image)
}

/// Whether the Air Forces Overview list is shown.
pub(super) fn overview_open(r: &Reader<'_>, image: u64) -> Result<bool> {
    let u = ui(r)?;
    let root = view_root(r, image, u.overview, u.overview_root)?;
    if !gui_tree::visible(r, root)? {
        return Ok(false);
    }
    match named_child(r, image, root, "air_units")? {
        Some(units) => gui_tree::visible(r, units),
        None => Ok(false),
    }
}
#[derive(Clone, Debug, PartialEq)]
pub(super) struct Wing {
    pub name: String,
    pub base: String,
    pub region: String,
    pub missions: Vec<i32>,
    pub rect: Option<Rect>,
}
pub(super) fn wings(r: &Reader<'_>, image: u64) -> Result<Vec<Wing>> {
    let u = ui(r)?;
    let root = view_root(r, image, u.overview, u.overview_root)?;
    let units = named_child(r, image, root, "air_units")?
        .ok_or_else(|| Error::new("gui_target_unavailable", "Air wing list not found"))?;
    let grid = gui_tree::child(r, image, units, "item_grid")?
        .ok_or_else(|| Error::new("gui_target_unavailable", "Air wing list not found"))?;
    let (_, data, count) = r.array_header(grid + u.grid_items)?;
    let mut out = Vec::new();
    for index in 0..count.min(512) as u64 {
        let Some(row) = entry_frame(r, image, r.u64(data + index * 8)?)? else {
            continue;
        };
        let mut wing = Wing {
            name: String::new(),
            base: String::new(),
            region: String::new(),
            missions: Vec::new(),
            rect: None,
        };
        for child in gui_tree::children(r, row)? {
            match gui_tree::name(r, child, image)?.as_deref() {
                Some("unit_name") => wing.name = text(r, child)?,
                Some("base") => wing.base = text(r, child)?,
                Some("assigned_region") => wing.region = text(r, child)?,
                Some("btn_bg") => wing.rect = gui_tree::rect_unclipped(r, image, child, GAME).ok(),
                Some(n) if n.starts_with("mission_icon_") && gui_tree::shown(r, child)? => {
                    wing.missions.push(frame(r, child)?)
                }
                _ => {}
            }
        }
        out.push(wing);
    }
    Ok(out)
}
/// The air selection view's root window.
pub(super) fn selection_root(r: &Reader<'_>, image: u64) -> Result<u64> {
    let u = ui(r)?;
    view_root(r, image, u.selection, u.selection_root)
}
/// Whether wings are selected (the air selection toolbar is shown).
pub(super) fn selection_open(r: &Reader<'_>, image: u64) -> Result<bool> {
    let u = ui(r)?;
    gui_tree::visible(r, view_root(r, image, u.selection, u.selection_root)?)
}
pub(super) fn toolbar_button(r: &Reader<'_>, image: u64, name: &str) -> Result<Rect> {
    let u = ui(r)?;
    let root = view_root(r, image, u.selection, u.selection_root)?;
    let bar = named_child(r, image, root, "air_wing_toolbar")?
        .ok_or_else(|| Error::new("gui_target_unavailable", "Air toolbar not found"))?;
    let button = gui_tree::child(r, image, bar, name)?
        .ok_or_else(|| Error::new("gui_target_unavailable", "Air toolbar button not found"))?;
    gui_tree::rect(r, image, button, GAME)
}
/// (icon frame, rectangle, enabled, toggled on) of the mission buttons.
pub(super) fn mission_buttons(r: &Reader<'_>, image: u64) -> Result<Vec<(i32, Rect, bool, bool)>> {
    let u = ui(r)?;
    let root = view_root(r, image, u.selection, u.selection_root)?;
    let bar = named_child(r, image, root, "mission_toolbar")?
        .ok_or_else(|| Error::new("gui_target_unavailable", "Mission toolbar not found"))?;
    let grid = gui_tree::child(r, image, bar, "missions_grid")?
        .ok_or_else(|| Error::new("gui_target_unavailable", "Mission toolbar not found"))?;
    let (_, data, count) = r.array_header(grid + u.grid_items)?;
    let mut out = Vec::new();
    for index in 0..count.min(32) as u64 {
        let Some(entry) = entry_frame(r, image, r.u64(data + index * 8)?)? else {
            continue;
        };
        let (Some(button), Some(icon)) = (
            gui_tree::child(r, image, entry, "btn")?,
            gui_tree::child(r, image, entry, "icon")?,
        ) else {
            continue;
        };
        let state = gui_tree::button_state(r, button)?;
        out.push((
            frame(r, icon)?,
            gui_tree::rect(r, image, button, GAME)?,
            state != 3,
            state == 2,
        ));
    }
    Ok(out)
}
pub(super) fn deselect(process: &dyn Process, keys: &MacKeys) -> Result<()> {
    if read(process, selection_open)? {
        let rect = settled(process, "air toolbar", |r, i| {
            toolbar_button(r, i, "btn_close")
        })?;
        let (x, y) = rect.center();
        keys.click(GAME, x, y)?;
        wait(process, "air wings deselected", |r, i| {
            Ok(!selection_open(r, i)?)
        })?;
    }
    Ok(())
}
pub(super) fn open_overview(process: &dyn Process, keys: &MacKeys) -> Result<()> {
    clear_notices(process, keys)?;
    deselect(process, keys)?;
    if !read(process, overview_open)? {
        keys.press(Key::letter('l')?)?;
        wait(process, "air overview open", overview_open)?;
    }
    Ok(())
}
fn wing_json(w: &Wing) -> Value {
    json!({"wing":w.name,"air_base":w.base,"region":(!w.region.is_empty()).then_some(&w.region),
        "missions":w.missions.iter().map(|f| mission_name(*f).unwrap_or("other")).collect::<Vec<_>>()})
}

/// Own air wings with base, region and missions (opens the air overview).
pub(super) fn list(process: &dyn Process, keys: &MacKeys) -> Result<Value> {
    open_overview(process, keys)?;
    let found = settled(process, "air wings", wings)?;
    Ok(
        json!({"wings":found.iter().map(wing_json).collect::<Vec<_>>(),
        "missions":MISSIONS.iter().map(|(n, _)| *n).collect::<Vec<_>>()}),
    )
}
/// Select the named wing through its row in the Air Forces Overview (opens it, scrolls the row into
/// view, one click); returns the row as it was before the click.
pub(super) fn select_wing(process: &dyn Process, keys: &MacKeys, name: &str) -> Result<Option<Wing>> {
    open_overview(process, keys)?;
    let find = |r: &Reader<'_>, i: u64| -> Result<Option<Rect>> {
        Ok(wings(r, i)?
            .into_iter()
            .find(|w| w.name == name)
            .and_then(|w| w.rect))
    };
    let mut rect = settled(process, "air wing row", find)?
        .ok_or_else(|| Error::new("invalid_target", "No own air wing with this name"))?;
    for _ in 0..40 {
        if rect.y >= ROWS.0 && rect.y + rect.h <= ROWS.1 {
            break;
        }
        if !read(process, overview_open)? {
            return Err(Error::new("input_readback_failed", "Air overview closed"));
        }
        let pixels = if rect.y < ROWS.0 { 240 } else { -240 };
        keys.scroll(GAME, (260.0, 650.0), pixels)?;
        rect = read(process, find)?
            .ok_or_else(|| Error::new("gui_target_unavailable", "Air wing left the list"))?;
    }
    let rect = settled(process, "air wing row", |r, i| {
        find(r, i)?
            .filter(|r| r.y >= ROWS.0 && r.y + r.h <= ROWS.1)
            .ok_or_else(|| Error::new("gui_target_unavailable", "Air wing row not in view"))
    })?;
    let before = read(process, wings)?.into_iter().find(|w| w.name == name);
    let (x, y) = rect.center();
    keys.click(GAME, x.min(330.0), y)?;
    wait(process, "air wing selected", selection_open)?;
    std::thread::sleep(std::time::Duration::from_millis(250));
    Ok(before)
}

/// Give wing `name` mission `mission` over the region containing `state`.
pub(super) fn assign(
    process: &dyn Process,
    keys: &MacKeys,
    name: &str,
    mission: &str,
    state: i32,
) -> Result<()> {
    let wanted = MISSIONS
        .iter()
        .find(|(n, _)| *n == mission)
        .map(|(_, f)| *f)
        .ok_or_else(|| Error::new("invalid_arguments", "Unknown mission"))?;
    select_wing(process, keys, name)?;
    let buttons = settled(process, "mission buttons", mission_buttons)?;
    let (_, button, enabled, _) = buttons
        .into_iter()
        .find(|(f, _, _, _)| *f == wanted)
        .ok_or_else(|| Error::new("invalid_target", "Mission not offered for this wing"))?;
    if !enabled {
        let _ = deselect(process, keys);
        return Err(Error::new(
            "invalid_target",
            "The game disables this mission for this wing",
        ));
    }
    // A mission button flips the wing's mission bit at once (a held wing keeps its last bit, and the
    // toolbar then shows the button off although the bit is set: clicking it again would clear the bit
    // and the right click would assign a region with no mission, seen live on JG 136 and KG 1,
    // 26 October 2026). So the bit in memory decides whether to click, not the button's look.
    let bit = 1 << (wanted - 1);
    let has_bit = |r: &Reader<'_>, i: u64| -> Result<bool> {
        Ok(super::airact::own_wing(r, i, name)?.is_some_and(|w| w.mission_mask & bit != 0))
    };
    let (x, y) = button.center();
    if !read(process, has_bit)? {
        keys.click(GAME, x, y)?;
        if let Err(error) = wait(process, "mission chosen", has_bit) {
            let _ = deselect(process, keys);
            return Err(error);
        }
    }
    find_state(process, keys, state)?;
    if !read(process, has_bit)? {
        let _ = deselect(process, keys);
        return Err(Error::new(
            "input_readback_failed",
            "The mission choice was lost before the region click",
        ));
    }
    let area = |w: &super::airread::Wing| (w.mission_mask, w.region_id);
    let mem_before = read(process, |r, i| super::airact::own_wing(r, i, name))?.map(|w| area(&w));
    // The camera glides to the state after the Find; give it time to rest before the click.
    std::thread::sleep(std::time::Duration::from_millis(900));
    keys.right_click(GAME, 960.0, 540.0)?;
    std::thread::sleep(std::time::Duration::from_millis(300));
    let result = wait(process, "air mission assigned", |r, i| {
        Ok(super::airact::own_wing(r, i, name)?.is_some_and(|w| {
            w.mission_mask & bit != 0
                && w.region_id > 0
                && mem_before.is_none_or(|b| b != area(&w))
        }))
    })
    .map_err(|_| {
        Error::new(
            "input_readback_failed",
            "The wing's mission or region did not change (the region is out of the wing's range, or the wing already has it)",
        )
    });
    let _ = deselect(process, keys);
    result
}
