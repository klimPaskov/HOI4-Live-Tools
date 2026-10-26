//! Own equipment production with ordinary input: open production (Y), add a
//! line through the category button and the equipment selection window, and
//! set a line's factories with its +/- buttons. Rows are joined to lines by
//! the line handle, every click target comes from the live GUI, and every
//! step is verified from the own line collection in memory.
use super::ui::{clear_notices, read, settled, wait};
use crate::{
    error::{Error, Result},
    game,
    gui_tree::{self, Rect},
    memory::{Reader, StringAbi},
    platform::{
        macos_input::{Key, MacKeys},
        Process,
    },
    production_lines::{self, OwnLine},
};

/// Production UI joins of one inspected build.
struct Ui {
    manager: u64,
    view: u64,
    view_root: u64,
    military_row_vtable: u64,
    naval_row_vtable: u64,
    row_handle: u64,
    row_container: u64,
    grid_items: u64,
    selection_military: u64,
    selection_root: u64,
    selection_row_variant: u64,
    variant_type: u64,
    type_key: u64,
}
/// macOS Mach-O 089EA519, mapped live on 18 October 2026 (Germany test run):
/// row handle +0x7d8 matched every own line handle, selection rows +0xb98
/// held the six listed designs and row +0xc30 their variants.
const MACOS: Ui = Ui {
    manager: 0x670,
    view: 0xd8,
    view_root: 0x430,
    military_row_vtable: 0x3478e78,
    naval_row_vtable: 0x3478fc0,
    row_handle: 0x7d8,
    row_container: 0x10,
    grid_items: 0x190,
    selection_military: 0x1cd0,
    selection_root: 0xbd8,
    selection_row_variant: 0xc30,
    variant_type: 0x3e0,
    type_key: 0x10,
};
const GAME: (u32, u32) = (1920, 1080);
/// Production line list area (rows fully inside are clickable).
const LIST: (i32, i32) = (360, 1040);
const CATEGORIES: [&str; 3] = ["infantry_button", "tank_button", "aircraft_button"];

fn ui(r: &Reader<'_>) -> Result<&'static Ui> {
    match r.string_abi() {
        StringAbi::LibCxx => Ok(&MACOS),
        StringAbi::LibStdCxx => Err(Error::new(
            "capability_unavailable",
            "Production input is not reviewed for this build",
        )),
    }
}
fn view(r: &Reader<'_>, image: u64) -> Result<u64> {
    let idler = r.u64(image + game::layout(r).idler_global)?;
    r.u64(r.u64(idler + ui(r)?.manager)? + ui(r)?.view)
}
fn root(r: &Reader<'_>, image: u64) -> Result<u64> {
    gui_tree::container(r, r.u64(view(r, image)? + ui(r)?.view_root)?, image)?
        .ok_or_else(|| Error::new("invalid_gui", "Production view root unavailable"))
}
pub(super) fn open(r: &Reader<'_>, image: u64) -> Result<bool> {
    gui_tree::visible(r, root(r, image)?)
}
fn window(r: &Reader<'_>, image: u64, object: u64, name: &str) -> Result<u64> {
    let child = gui_tree::child(r, image, object, name)?
        .ok_or_else(|| Error::new("gui_target_unavailable", format!("{name} not found")))?;
    gui_tree::container(r, child, image)?
        .ok_or_else(|| Error::new("invalid_gui", format!("{name} is not a window")))
}
fn military_tab(r: &Reader<'_>, image: u64) -> Result<u64> {
    window(r, image, root(r, image)?, "military_tab")
}
pub(super) fn category_rect(r: &Reader<'_>, image: u64, name: &str) -> Result<Rect> {
    let top = window(r, image, military_tab(r, image)?, "production_win_top")?;
    let button = gui_tree::child(r, image, top, name)?
        .ok_or_else(|| Error::new("gui_target_unavailable", "Category button not found"))?;
    gui_tree::rect(r, image, button, GAME)
}
fn selection(r: &Reader<'_>, image: u64) -> Result<(u64, u64)> {
    let u = ui(r)?;
    let window = r.u64(view(r, image)? + u.selection_military)?;
    let root = gui_tree::container(r, r.u64(window + u.selection_root)?, image)?
        .ok_or_else(|| Error::new("invalid_gui", "Selection window root unavailable"))?;
    Ok((window, root))
}
pub(super) fn selection_open(r: &Reader<'_>, image: u64) -> Result<bool> {
    gui_tree::visible(r, selection(r, image)?.1)
}
/// The rows the open selection list displays, in list order (the items of its grid). The
/// window's own row array is a pool that also holds rows of other lists with stale rectangles.
fn displayed_rows(r: &Reader<'_>, image: u64, root: u64) -> Result<Vec<u64>> {
    let u = ui(r)?;
    let list = window(r, image, root, "equipments")?;
    let grid = gui_tree::child(r, image, list, "equipments_grid")?
        .ok_or_else(|| Error::new("gui_target_unavailable", "Selection grid not found"))?;
    let (_, data, count) = r.array_header(grid + u.grid_items)?;
    if count > 256 {
        return Err(Error::new("invalid_gui", "Selection grid exceeds bound"));
    }
    (0..count as u64).map(|i| r.u64(data + i * 8)).collect()
}
/// The selection row for `key`: its equipment button rectangle, if listed.
fn selection_row(
    r: &Reader<'_>,
    image: u64,
    key: &str,
    design: Option<i32>,
) -> Result<Option<Rect>> {
    let u = ui(r)?;
    let (_, root) = selection(r, image)?;
    for row in displayed_rows(r, image, root)? {
        let variant = r.u64(row + u.selection_row_variant)?;
        if variant == 0 {
            continue;
        }
        let kind = r.u64(variant + u.variant_type)?;
        if kind == 0 || r.string_bounded(kind + u.type_key, 128)? != key {
            continue;
        }
        if design.is_some_and(|id| r.i32(variant + 0xc).ok() != Some(id)) {
            continue;
        }
        let Some(frame) = gui_tree::container(r, r.u64(row + u.row_container)?, image)? else {
            continue;
        };
        if let Some(button) = gui_tree::child(r, image, frame, "equipment_button")? {
            // A design the game will not build has a disabled button (state 3): never offered.
            if gui_tree::disabled(r, button)? {
                continue;
            }
            return Ok(Some(gui_tree::rect(r, image, button, GAME)?));
        }
    }
    Ok(None)
}
/// Equipment keys of the designs the open selection offers.
fn offered(r: &Reader<'_>, image: u64) -> Result<Vec<String>> {
    let u = ui(r)?;
    let (_, root) = selection(r, image)?;
    let mut out = Vec::new();
    for row in displayed_rows(r, image, root)? {
        let variant = r.u64(row + u.selection_row_variant)?;
        if variant == 0 {
            continue;
        }
        let kind = r.u64(variant + u.variant_type)?;
        if kind != 0 {
            let key = r.string_bounded(kind + u.type_key, 128)?;
            if !out.contains(&key) {
                out.push(key);
            }
        }
    }
    Ok(out)
}
/// Row window of the own line with this handle, if materialized.
pub(super) fn row(r: &Reader<'_>, image: u64, handle: u64) -> Result<Option<u64>> {
    let u = ui(r)?;
    let lines = window(r, image, military_tab(r, image)?, "production_lines")?;
    let grid = gui_tree::child(r, image, lines, "item_grid")?
        .ok_or_else(|| Error::new("gui_target_unavailable", "Line grid not found"))?;
    let (_, data, count) = r.array_header(grid + u.grid_items)?;
    for index in 0..count.min(2048) as u64 {
        let item = r.u64(data + index * 8)?;
        let kind = r.u64(item)?;
        if kind != image + u.military_row_vtable && kind != image + u.naval_row_vtable {
            continue;
        }
        if r.u64(item + u.row_handle)? == handle {
            return gui_tree::container(r, r.u64(item + u.row_container)?, image);
        }
    }
    Ok(None)
}
pub(super) fn button(r: &Reader<'_>, image: u64, handle: u64, name: &str) -> Result<Option<Rect>> {
    let Some(frame) = row(r, image, handle)? else {
        return Ok(None);
    };
    let Some(button) = gui_tree::child(r, image, frame, name)? else {
        return Ok(None);
    };
    Ok(Some(gui_tree::rect_unclipped(r, image, button, GAME)?))
}

pub(super) fn lines(process: &dyn Process) -> Result<Vec<OwnLine>> {
    read(process, production_lines::own_lines)
}

pub(super) fn ensure_open(process: &dyn Process, keys: &MacKeys) -> Result<()> {
    if !read(process, open)? {
        keys.press(Key::letter('y')?)?;
        wait(process, "open production", open)?;
    }
    Ok(())
}
fn click(process: &dyn Process, keys: &MacKeys, rect: Rect) -> Result<()> {
    clear_notices(process, keys)?;
    let (x, y) = rect.center();
    keys.click(GAME, x, y).map(|_| ())
}

/// Bring the row's +/- buttons inside the list area by scrolling the list.
pub(super) fn reveal(process: &dyn Process, keys: &MacKeys, handle: u64) -> Result<()> {
    reveal_named(process, keys, handle, "factories_add_1x", i32::MAX)
}
/// Bring the named row button of the line inside the list area and return its rectangle.
pub(super) fn reveal_named(
    process: &dyn Process,
    keys: &MacKeys,
    handle: u64,
    name: &str,
    min_visible: i32,
) -> Result<()> {
    for _ in 0..40 {
        // Never scroll unless the list is on screen; the wheel zooms the map.
        if !read(process, open)? {
            return Err(Error::new(
                "input_readback_failed",
                "Production panel closed while adjusting a line",
            ));
        }
        let rect = read(process, |r, i| button(r, i, handle, name))?;
        match rect {
            Some(r) if r.y >= LIST.0 && r.y + r.h.min(min_visible) <= LIST.1 => return Ok(()),
            Some(r) => {
                let pixels = if r.y < LIST.0 { 240 } else { -240 };
                keys.scroll(GAME, (260.0, 700.0), pixels)?;
            }
            None => keys.scroll(GAME, (260.0, 700.0), -240)?,
        }
    }
    Err(Error::new(
        "gui_target_unavailable",
        "Production line row could not be scrolled into view",
    ))
}

/// Adjust the line's requested factories to `target` with single +/- clicks.
fn adjust(process: &dyn Process, keys: &MacKeys, handle: u64, target: i32) -> Result<()> {
    for _ in 0..120 {
        let current = lines(process)?
            .into_iter()
            .find(|l| l.handle == handle)
            .ok_or_else(|| Error::new("input_readback_failed", "Production line disappeared"))?
            .requested;
        if current == target {
            return Ok(());
        }
        reveal(process, keys, handle)?;
        let name = if current < target {
            "factories_add_1x"
        } else {
            "factories_sub_1x"
        };
        let rect = settled(process, "factory button", |r, i| {
            button(r, i, handle, name)?
                .ok_or_else(|| Error::new("gui_target_unavailable", "Factory button not found"))
        })?;
        click(process, keys, rect)?;
        wait(process, "factory change", |r, base| {
            Ok(production_lines::own_lines(r, base)?
                .iter()
                .find(|l| l.handle == handle)
                .is_none_or(|l| l.requested != current))
        })?;
    }
    Err(Error::new(
        "input_readback_failed",
        "Factory target not reached within the click bound",
    ))
}

/// Add one line for base equipment `key` and set it to `factories`.
pub(super) fn add(
    process: &dyn Process,
    keys: &MacKeys,
    key: &str,
    factories: i32,
    design: Option<i32>,
) -> Result<()> {
    clear_notices(process, keys)?;
    ensure_open(process, keys)?;
    let before: Vec<u64> = lines(process)?.iter().map(|l| l.handle).collect();
    let mut chosen = None;
    let mut seen: Vec<String> = Vec::new();
    for category in CATEGORIES {
        if read(process, selection_open)? {
            keys.press(Key::Escape)?;
            wait(process, "close selection", |r, i| {
                Ok(!selection_open(r, i)?)
            })?;
            ensure_open(process, keys)?;
        }
        let rect = settled(process, "category", |r, i| category_rect(r, i, category))?;
        click(process, keys, rect)?;
        wait(process, "open selection", selection_open)?;
        let found = settled(process, "equipment row", |r, i| selection_row(r, i, key, design))?;
        if let Some(rect) = found {
            chosen = Some(rect);
            break;
        }
        for key in read(process, offered).unwrap_or_default() {
            if !seen.contains(&key) {
                seen.push(key);
            }
        }
    }
    let Some(rect) = chosen else {
        if read(process, selection_open)? {
            keys.press(Key::Escape)?;
        }
        return Err(Error::new(
            "invalid_target",
            format!(
                "No producible design of this equipment is offered; offered: {}",
                seen.join(", ")
            ),
        ));
    };
    click(process, keys, rect)?;
    wait(process, "add line", |r, base| {
        Ok(production_lines::own_lines(r, base)?
            .iter()
            .any(|l| !before.contains(&l.handle) && l.key.as_deref() == Some(key)))
    })?;
    let handle = lines(process)?
        .into_iter()
        .find(|l| !before.contains(&l.handle) && l.key.as_deref() == Some(key))
        .map(|l| l.handle)
        .ok_or_else(|| Error::new("input_readback_failed", "New line not found"))?;
    // Choosing a design closes the selection window by itself.
    wait(process, "selection closes", |r, i| {
        Ok(!selection_open(r, i)?)
    })?;
    if !read(process, open)? {
        return Err(Error::new(
            "input_readback_failed",
            "Production panel closed after adding the line",
        ));
    }
    adjust(process, keys, handle, factories)
}

/// Set an existing line (by collection index and its equipment key) to `factories`.
pub(super) fn set(
    process: &dyn Process,
    keys: &MacKeys,
    index: usize,
    key: &str,
    factories: i32,
) -> Result<()> {
    clear_notices(process, keys)?;
    let line = lines(process)?
        .get(index)
        .cloned()
        .filter(|l| l.key.as_deref() == Some(key))
        .ok_or_else(|| {
            Error::new(
                "invalid_target",
                "No own line with this index and equipment",
            )
        })?;
    ensure_open(process, keys)?;
    if read(process, selection_open)? {
        keys.press(Key::Escape)?;
        wait(process, "close selection", |r, i| {
            Ok(!selection_open(r, i)?)
        })?;
        ensure_open(process, keys)?;
    }
    adjust(process, keys, line.handle, factories)
}
