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
        game_input::{GameInput, Key},
        Process,
    },
    production_lines::{self, OwnLine},
    xlayout::{fld, img, Class},
};

/// Production UI joins of one inspected build.
#[derive(Clone, Copy)]
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
    view_root: 0x490,
    military_row_vtable: 0x3686b08,
    naval_row_vtable: 0x3686c50,
    row_handle: 0x898,
    row_container: 0x10,
    grid_items: 0x190,
    selection_military: 0x1f10,
    selection_root: 0xcf8,
    selection_row_variant: 0xd50,
    variant_type: 0x3e0,
    type_key: 0x10,
};
/// Owners of the fields above (macOS 1.19.4.1 RTTI names and vtables; 0 where
/// none was reviewed).
const IDLER: Class = Class::new("CInGameIdler", 0x35089d8);
const MANAGER: Class = Class::new("idler.manager", 0);
const VIEW: Class = Class::new("production.view", 0);
const ROW: Class = Class::new("CProductionLineMilitaryItem", 0x3686b08);
const GRID: Class = Class::new("CStandardGridBox", 0x36cca80);
const GRID_ITEM: Class = Class::new("grid.item", 0);
const SELECTION: Class = Class::new("production.selection.window", 0);
const SELECTION_ROW: Class = Class::new("production.selection.row", 0);
const VARIANT: Class = Class::new("CEquipmentVariant", 0x357d1b8);
const EQUIPMENT_TYPE: Class = Class::new("CEquipmentType", 0x357bad8);
/// Design id of an equipment variant.
const VARIANT_ID: u64 = 0xc;
impl Ui {
    /// The macOS keys translated for this build (identity on macOS).
    fn resolve(&self, abi: StringAbi) -> Result<Self> {
        Ok(Self {
            manager: fld(abi, &IDLER, self.manager)?,
            view: fld(abi, &MANAGER, self.view)?,
            view_root: fld(abi, &VIEW, self.view_root)?,
            military_row_vtable: img(abi, self.military_row_vtable)?,
            naval_row_vtable: img(abi, self.naval_row_vtable)?,
            row_handle: fld(abi, &ROW, self.row_handle)?,
            row_container: fld(abi, &GRID_ITEM, self.row_container)?,
            grid_items: fld(abi, &GRID, self.grid_items)?,
            selection_military: fld(abi, &VIEW, self.selection_military)?,
            selection_root: fld(abi, &SELECTION, self.selection_root)?,
            selection_row_variant: fld(abi, &SELECTION_ROW, self.selection_row_variant)?,
            variant_type: fld(abi, &VARIANT, self.variant_type)?,
            type_key: fld(abi, &EQUIPMENT_TYPE, self.type_key)?,
        })
    }
}
const GAME: (u32, u32) = (1920, 1080);
/// Production line list area (rows fully inside are clickable). At 1920x1080 the list runs past the
/// bottom of the screen and cannot scroll its last row higher than about 1050-1066 for that row's
/// priority arrow (Germany, 16 Nov 2026: 21 lines, to:top of the last line refused "could not be
/// scrolled into view" with a bottom of 1040), so the bottom is the screen's edge.
const LIST: (i32, i32) = (360, 1078);
const CATEGORIES: [&str; 3] = ["infantry_button", "tank_button", "aircraft_button"];
/// Ships and convoys: the Production panel's naval category button opens the ship selection
/// window (view +0x1cd8), whose rows `design_ui::rows` reads.
const NAVAL_CATEGORIES: [&str; 1] = ["naval_button"];
/// Whether an equipment key is a ship hull or the convoy (the naval category lists those).
pub(super) fn is_naval_key(key: &str) -> bool {
    key.starts_with("ship_hull") || key.starts_with("convoy")
}
/// The naval selection row for `key`: its equipment button rectangle, if listed and buildable.
fn naval_row(r: &Reader<'_>, image: u64, key: &str, design: Option<i32>) -> Result<Option<Rect>> {
    for row in super::design_ui::rows(r, image)? {
        if !row.shown || row.key != key || row.invalid || row.obsolete {
            continue;
        }
        if design.is_some_and(|id| row.id != id) {
            continue;
        }
        if let Some(button) = row.equipment_button {
            return Ok(Some(button));
        }
    }
    Ok(None)
}
fn naval_offered(r: &Reader<'_>, image: u64) -> Result<Vec<String>> {
    let mut out = Vec::new();
    for row in super::design_ui::rows(r, image)? {
        if row.shown && !row.invalid && !row.obsolete && !out.contains(&row.key) {
            out.push(row.key);
        }
    }
    Ok(out)
}

fn ui(r: &Reader<'_>) -> Result<Ui> {
    MACOS.resolve(r.string_abi())
}
fn view(r: &Reader<'_>, image: u64) -> Result<u64> {
    let idler = r.u64(image + game::layout(r).idler_global)?;
    let u = ui(r)?;
    r.u64(r.u64(idler + u.manager)? + u.view)
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
    let variant_id = fld(r.string_abi(), &VARIANT, VARIANT_ID)?;
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
        if design.is_some_and(|id| r.i32(variant + variant_id).ok() != Some(id)) {
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

pub(super) fn ensure_open(process: &dyn Process, keys: &dyn GameInput) -> Result<()> {
    if !read(process, open)? {
        keys.press(Key::letter('y')?)?;
        wait(process, "open production", open)?;
    }
    Ok(())
}
fn click(process: &dyn Process, keys: &dyn GameInput, rect: Rect) -> Result<()> {
    clear_notices(process, keys)?;
    let (x, y) = rect.center();
    keys.click(GAME, x, y).map(|_| ())
}

/// Bring the row's +/- buttons inside the list area by scrolling the list.
pub(super) fn reveal(process: &dyn Process, keys: &dyn GameInput, handle: u64) -> Result<()> {
    reveal_named(process, keys, handle, "factories_add_1x", i32::MAX)
}
/// Bring the named row button of the line inside the list area and return its rectangle.
pub(super) fn reveal_named(
    process: &dyn Process,
    keys: &dyn GameInput,
    handle: u64,
    name: &str,
    min_visible: i32,
) -> Result<()> {
    let closed = || {
        Error::new(
            "input_readback_failed",
            "Production panel closed while adjusting a line",
        )
    };
    // Never scroll unless the list is on screen; the wheel zooms the map.
    if !read(process, open)? {
        return Err(closed());
    }
    // One glide (a row not laid out yet is searched downwards one event at a time).
    let glide_once = || {
        super::glide::glide(
            process,
            keys,
            &super::glide::Glide {
                at: (260.0, 700.0),
                pixels: 240,
                band: (f64::from(LIST.0), f64::from(LIST.1)),
                search: -1,
            },
            || {
                if read(process, open)? {
                    Ok(())
                } else {
                    Err(closed())
                }
            },
            |r, i| {
                Ok(button(r, i, handle, name)?.map(|b| {
                    let top = f64::from(b.y);
                    (top, top + f64::from(b.h.min(min_visible)))
                }))
            },
        )
    };
    let mut outcome = glide_once()?;
    if !matches!(outcome, super::glide::Glided::InBand(_)) {
        // A row read far outside the list that a wheel event does not move (pp2, 17 Nov 2026:
        // "Stuck(-523)" after the panel was reopened from another view, the game running): the
        // rows keep the place of an older scroll until the list is drawn again. One burst to
        // the list's top redraws it, then the glide is tried once more.
        keys.scroll_burst(GAME, (260.0, 700.0), 240, 60)?;
        std::thread::sleep(std::time::Duration::from_millis(700));
        outcome = glide_once()?;
    }
    if matches!(outcome, super::glide::Glided::InBand(_)) {
        return Ok(());
    }
    Err(Error::new(
        "gui_target_unavailable",
        format!("Production line row could not be scrolled into view ({outcome:?})"),
    ))
}

/// Line handles in the order the production view shows them (its `item_grid`): the priority
/// order, highest first (a line higher in the list gets resources and factories first).
pub(super) fn shown_order(r: &Reader<'_>, image: u64) -> Result<Vec<u64>> {
    let u = ui(r)?;
    let lines = window(r, image, military_tab(r, image)?, "production_lines")?;
    let grid = gui_tree::child(r, image, lines, "item_grid")?
        .ok_or_else(|| Error::new("gui_target_unavailable", "Line grid not found"))?;
    let (_, data, count) = r.array_header(grid + u.grid_items)?;
    let mut out = Vec::new();
    for index in 0..count.min(2048) as u64 {
        let item = r.u64(data + index * 8)?;
        let kind = r.u64(item)?;
        if kind != image + u.military_row_vtable && kind != image + u.naval_row_vtable {
            continue;
        }
        out.push(r.u64(item + u.row_handle)?);
    }
    Ok(out)
}

/// Where a line goes: `Top`/`Bottom` (the arrow with Shift: highest or lowest priority) or one
/// place `Up`/`Down` (the arrow alone).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Move {
    Top,
    Up,
    Down,
    Bottom,
}
impl Move {
    pub(super) fn parse(s: &str) -> Option<Self> {
        Some(match s {
            "top" => Self::Top,
            "up" => Self::Up,
            "down" => Self::Down,
            "bottom" => Self::Bottom,
            _ => return None,
        })
    }
    /// The place the line should show at after the move, from its place `at` among `count`.
    pub(super) fn target(self, at: usize, count: usize) -> usize {
        match self {
            Self::Top => 0,
            Self::Up => at.saturating_sub(1),
            Self::Down => (at + 1).min(count.saturating_sub(1)),
            Self::Bottom => count.saturating_sub(1),
        }
    }
}

/// Move the line `handle` in the production view's priority order with its row's arrows
/// (increase_priority / decrease_priority; Shift+click goes to the top or the bottom). Verified by
/// the line's place in the view's list. A line already where it is asked to go is left alone.
pub(super) fn prioritize(
    process: &dyn Process,
    keys: &dyn GameInput,
    handle: u64,
    to: Move,
) -> Result<()> {
    ensure_open(process, keys)?;
    let place = |r: &Reader<'_>, i: u64| -> Result<(usize, usize)> {
        let order = shown_order(r, i)?;
        let at = order.iter().position(|h| *h == handle).ok_or_else(|| {
            Error::new(
                "input_readback_failed",
                "The line is not in the production list",
            )
        })?;
        Ok((at, order.len()))
    };
    let (at, count) = settled(process, "production list", place)?;
    let target = to.target(at, count);
    if at == target {
        return Ok(());
    }
    let name = if target < at {
        "increase_priority"
    } else {
        "decrease_priority"
    };
    reveal_named(process, keys, handle, name, 20)?;
    let rect = settled(process, "priority arrow", |r, i| {
        button(r, i, handle, name)?
            .ok_or_else(|| Error::new("gui_target_unavailable", "Priority arrow not found"))
    })?;
    clear_notices(process, keys)?;
    let (x, y) = rect.center();
    if matches!(to, Move::Top | Move::Bottom) {
        keys.shift_click(GAME, x, y)?;
        // The Shift+click left the line in place on 1.19.4 (pm1 24 -> 24, pr1 3 -> 3, 17 Nov
        // 2026): step it with single clicks on the same arrow, one place per click.
        if wait(process, "line moved", |r, i| Ok(place(r, i)?.0 != at)).is_err() {
            for _ in 0..at.abs_diff(target).min(64) {
                let now = read(process, place)?.0;
                if now == target {
                    break;
                }
                reveal_named(process, keys, handle, name, 20)?;
                let rect = settled(process, "priority arrow", |r, i| {
                    button(r, i, handle, name)?.ok_or_else(|| {
                        Error::new("gui_target_unavailable", "Priority arrow not found")
                    })
                })?;
                let (x, y) = rect.center();
                keys.click(GAME, x, y)?;
                let _ = wait(process, "line stepped", |r, i| Ok(place(r, i)?.0 != now));
            }
        }
    } else {
        keys.click(GAME, x, y)?;
    }
    wait(process, "line moved", |r, i| Ok(place(r, i)?.0 == target)).map_err(|_| {
        let now = read(process, place).map(|p| p.0).ok();
        Error::new(
            "action_not_applied",
            format!(
                "The {name} click left the line at place {now:?} of {count} (asked: {target}, was {at})"
            ),
        )
    })
}

/// Adjust the line's requested factories to `target` with single +/- clicks.
fn adjust(process: &dyn Process, keys: &dyn GameInput, handle: u64, target: i32) -> Result<()> {
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
    keys: &dyn GameInput,
    key: &str,
    factories: i32,
    design: Option<i32>,
) -> Result<()> {
    clear_notices(process, keys)?;
    ensure_open(process, keys)?;
    let naval = is_naval_key(key);
    let categories: &[&str] = if naval {
        &NAVAL_CATEGORIES
    } else {
        &CATEGORIES
    };
    let before: Vec<u64> = lines(process)?.iter().map(|l| l.handle).collect();
    let mut chosen = None;
    let mut seen: Vec<String> = Vec::new();
    for category in categories {
        if read(process, super::design_ui::selection_open)? {
            keys.press(Key::Escape)?;
            wait(process, "close selection", |r, i| {
                Ok(!super::design_ui::selection_open(r, i)?)
            })?;
            ensure_open(process, keys)?;
        }
        let rect = settled(process, "category", |r, i| category_rect(r, i, category))?;
        click(process, keys, rect)?;
        wait(process, "open selection", super::design_ui::selection_open)?;
        let found = if naval {
            // The list slides in; wait until its rows report their buttons.
            let _ = wait(process, "selection rows", |r, i| {
                Ok(super::design_ui::rows(r, i)?
                    .iter()
                    .any(|x| x.shown && x.equipment_button.is_some()))
            });
            // The ship list is long: scroll the wanted row into the visible band.
            // The first row (the convoy) sits right under the class filter header.
            let outcome = super::glide::glide(
                process,
                keys,
                &super::glide::Glide {
                    at: (780.0, 700.0),
                    pixels: 240,
                    band: (230.0, 1040.0),
                    search: 0,
                },
                || {
                    if read(process, super::design_ui::selection_open)? {
                        Ok(())
                    } else {
                        Err(Error::new(
                            "input_readback_failed",
                            "The equipment selection list closed",
                        ))
                    }
                },
                |r, i| Ok(naval_row(r, i, key, design)?.map(|x| super::glide::span_y(&x))),
            )?;
            match outcome {
                super::glide::Glided::InBand(_) => settled(process, "equipment row", |r, i| {
                    naval_row(r, i, key, design)
                })?,
                _ => None,
            }
        } else {
            settled(process, "equipment row", |r, i| {
                selection_row(r, i, key, design)
            })?
        };
        if let Some(rect) = found {
            chosen = Some(rect);
            break;
        }
        let listed = if naval {
            read(process, naval_offered)
        } else {
            read(process, offered)
        };
        for key in listed.unwrap_or_default() {
            if !seen.contains(&key) {
                seen.push(key);
            }
        }
    }
    let Some(rect) = chosen else {
        if read(process, super::design_ui::selection_open)? {
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
    // With Arms Against Tyranny a plain click opens the Military Industrial Organization
    // chooser; Shift+click is the game's own "produce without a Military Industrial
    // Organization" and adds the line directly (verified 1 November 2026).
    clear_notices(process, keys)?;
    let (x, y) = rect.center();
    keys.shift_click(GAME, x, y)?;
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
        Ok(!super::design_ui::selection_open(r, i)?)
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
    keys: &dyn GameInput,
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
    if factories == 0 {
        // Zero factories removes the line with its close_production_line button
        // (countryproductionlineview.gui); verified when the line handle is gone.
        reveal_named(
            process,
            keys,
            line.handle,
            "close_production_line",
            i32::MAX,
        )?;
        let rect = settled(process, "remove button", |r, i| {
            button(r, i, line.handle, "close_production_line")?
                .ok_or_else(|| Error::new("gui_target_unavailable", "Remove button not found"))
        })?;
        click(process, keys, rect)?;
        // "Cancel Production Line" asks for confirmation; Return accepts it (1.19.4.1).
        if !super::ui::accept_command(process, keys)? {
            std::thread::sleep(std::time::Duration::from_millis(300));
            keys.press(Key::Return)?;
        }
        return wait(process, "line removed", |r, base| {
            Ok(!production_lines::own_lines(r, base)?
                .iter()
                .any(|l| l.handle == line.handle))
        });
    }
    adjust(process, keys, line.handle, factories)
}
