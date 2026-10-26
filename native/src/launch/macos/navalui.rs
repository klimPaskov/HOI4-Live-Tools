//! Naval UI reads on macOS (089EA519): the Strategic Navy map mode (F2), the fleet cards of the
//! bottom bar, the fleet panel (`CNaviesView`) with its task force entries, the mission bar and
//! the leader window. Memory reads only; every rectangle comes from the laid-out sprites and is
//! a click target, never an authorization.
//!
//! Mapped live on 26 October 2026 (Germany, 15 October 1938, 1920x1080):
//! * interface manager slot +0x1c8 `CFleetsBottomBar`: window `navy_leader_window` +0x850, its
//!   `centering_box` holds `new_fleet_window` (the "+" card) and `fleets_box` (one card per fleet,
//!   the reserve fleet last); slot +0x1e0 `CNaviesView`: `naviesview_window` +0x6c70 (fleet panel;
//!   grid `detailed_fleet_box` holds the fleet header), `action_bar_window` +0x6c80 (missions),
//!   `target_bar_window` +0x6c88, `tool_bar_window` +0x6e40; the displayed task force entries are
//!   the Clausewitz array at view +0x30 (`CTaskForceViewEntry`, whose window pointer is entry
//!   +0x2390).
use super::ui::read;
use crate::{
    error::{Error, Result},
    game,
    gui_tree::{self, Rect},
    memory::{Reader, StringAbi},
    platform::Process,
};
use serde_json::{json, Map, Value};

pub(super) const GAME: (u32, u32) = (1920, 1080);
const MANAGER: u64 = 0x670;
pub(super) const SLOT_BOTTOM: u64 = 0x1c8;
pub(super) const SLOT_NAVIES: u64 = 0x1e0;
const BOTTOM_ROOT: u64 = 0x850;
const NV_WINDOW: u64 = 0x6c70;
const NV_ACTION: u64 = 0x6c80;
/// `CNavyLeaderWindow` member of the fleet view and its root window `navyleaderwindow`.
const NV_LEADER_WINDOW: u64 = 0x4218;
const LEADER_ROOT: u64 = 0x4b8;
const NV_TARGET: u64 = 0x6c88;
const NV_TOOLBAR: u64 = 0x6e40;
const NV_ENTRIES: u64 = 0x30;
const ENTRY_WINDOW: u64 = 0x2390;
/// Task forces the panel shows now (`CTaskForce`, name at +0x258): all of the fleet's while none is
/// selected, only the selected one afterwards. The shown entry windows are re-bound to them in the
/// same order (selecting the second task force showed it in the first window, 26 October 2026).
const NV_TASK_FORCES: u64 = 0x6e28;
const TF_NAME: u64 = 0x258;

const GRID_ITEMS: u64 = 0x190;
const ITEM_CONTAINER: u64 = 0x10;
const TEXT: u64 = 0x100;
const BUTTON_TEXT: u64 = 0x158;

pub(super) fn libcxx(r: &Reader<'_>) -> Result<()> {
    match r.string_abi() {
        StringAbi::LibCxx => Ok(()),
        StringAbi::LibStdCxx => Err(Error::new(
            "capability_unavailable",
            "Naval input is reviewed for the macOS build only",
        )),
    }
}
pub(super) fn view(r: &Reader<'_>, image: u64, slot: u64) -> Result<u64> {
    libcxx(r)?;
    let idler = r.u64(image + game::layout(r).idler_global)?;
    let view = r.u64(r.u64(idler + MANAGER)? + slot)?;
    if view == 0 {
        return Err(Error::new("invalid_gui", "Naval view unavailable"));
    }
    Ok(view)
}
fn root_at(r: &Reader<'_>, image: u64, view: u64, field: u64) -> Result<u64> {
    gui_tree::container(r, r.u64(view + field)?, image)?
        .ok_or_else(|| Error::new("invalid_gui", "Naval window unavailable"))
}
/// Named windows of the fleet panel view.
pub(super) fn panel(r: &Reader<'_>, image: u64) -> Result<u64> {
    root_at(r, image, view(r, image, SLOT_NAVIES)?, NV_WINDOW)
}
pub(super) fn leader_root(r: &Reader<'_>, image: u64) -> Result<u64> {
    let window = r.u64(view(r, image, SLOT_NAVIES)? + NV_LEADER_WINDOW)?;
    gui_tree::container(r, r.u64(window + LEADER_ROOT)?, image)?
        .ok_or_else(|| Error::new("invalid_gui", "Navy leader window unavailable"))
}
pub(super) fn action_bar(r: &Reader<'_>, image: u64) -> Result<u64> {
    root_at(r, image, view(r, image, SLOT_NAVIES)?, NV_ACTION)
}
pub(super) fn target_bar(r: &Reader<'_>, image: u64) -> Result<u64> {
    root_at(r, image, view(r, image, SLOT_NAVIES)?, NV_TARGET)
}
pub(super) fn tool_bar(r: &Reader<'_>, image: u64) -> Result<u64> {
    root_at(r, image, view(r, image, SLOT_NAVIES)?, NV_TOOLBAR)
}
pub(super) fn bottom_bar(r: &Reader<'_>, image: u64) -> Result<u64> {
    root_at(r, image, view(r, image, SLOT_BOTTOM)?, BOTTOM_ROOT)
}
/// Windows of the displayed task force entries, in panel order.
pub(super) fn entry_windows(r: &Reader<'_>, image: u64) -> Result<Vec<u64>> {
    let v = view(r, image, SLOT_NAVIES)?;
    let (_, data, count) = r.array_header(v + NV_ENTRIES)?;
    if count > 64 {
        return Err(Error::new("invalid_array", "Task force entries exceed bound"));
    }
    let mut out = Vec::new();
    for i in 0..count as u64 {
        let entry = r.u64(data + i * 8)?;
        if let Some(w) = gui_tree::container(r, r.u64(entry + ENTRY_WINDOW)?, image)? {
            out.push(w);
        }
    }
    Ok(out)
}
pub(super) fn grid_items(r: &Reader<'_>, grid: u64) -> Result<Vec<u64>> {
    let (_, data, count) = r.array_header(grid + GRID_ITEMS)?;
    if count > 256 {
        return Err(Error::new("invalid_gui", "Grid item list exceeds bound"));
    }
    (0..count as u64).map(|i| r.u64(data + i * 8)).collect()
}
pub(super) fn text_at(r: &Reader<'_>, widget: u64) -> Result<String> {
    Ok(super::special_projects::plain(&r.string_bounded(widget + TEXT, 256)?))
}
pub(super) fn label_at(r: &Reader<'_>, button: u64) -> Result<String> {
    Ok(super::special_projects::plain(&r.string_bounded(button + BUTTON_TEXT, 160)?))
}
pub(super) fn child_of(r: &Reader<'_>, image: u64, parent: u64, name: &str) -> Result<u64> {
    gui_tree::child(r, image, parent, name)?
        .ok_or_else(|| Error::new("gui_target_unavailable", format!("{name} not found")))
}
pub(super) fn window_of(r: &Reader<'_>, image: u64, parent: u64, name: &str) -> Result<u64> {
    gui_tree::container(r, child_of(r, image, parent, name)?, image)?
        .ok_or_else(|| Error::new("invalid_gui", format!("{name} is not a window")))
}

// ---- fleet panel, cards, entries, mission bar, region view -------------------------------------
const REGION_VIEW_SLOT: u64 = 0xd0;
const REGION_ROOT: u64 = 0x430;
/// Edit boxes (`fleet_name`, `unit_name`) keep their text at +0x3b8 (read live on 26 October 2026).
const EDIT_TEXT: u64 = 0x3b8;
pub(super) fn edit_text(r: &Reader<'_>, widget: u64) -> Result<String> {
    Ok(super::special_projects::plain(&r.string_bounded(widget + EDIT_TEXT, 160)?))
}
/// Whether the Strategic Navy map mode is on (its fleet bar is shown only there).
pub(super) fn navy_mode(r: &Reader<'_>, image: u64) -> Result<bool> {
    gui_tree::visible(r, bottom_bar(r, image)?)
}
pub(super) fn panel_open(r: &Reader<'_>, image: u64) -> Result<bool> {
    gui_tree::visible(r, panel(r, image)?)
}
/// The fleet header window of the open fleet panel.
pub(super) fn header(r: &Reader<'_>, image: u64) -> Result<Option<u64>> {
    let root = panel(r, image)?;
    if !gui_tree::visible(r, root)? {
        return Ok(None);
    }
    let list = window_of(r, image, root, "detailed_fleet_list")?;
    let grid = child_of(r, image, list, "detailed_fleet_box")?;
    let Some(item) = grid_items(r, grid)?.first().copied() else {
        return Ok(None);
    };
    gui_tree::container(r, r.u64(item + ITEM_CONTAINER)?, image)
}
/// Name of the fleet the open panel shows.
pub(super) fn panel_fleet(r: &Reader<'_>, image: u64) -> Result<Option<String>> {
    let Some(header) = header(r, image)? else {
        return Ok(None);
    };
    let name = child_of(r, image, header, "fleet_name")?;
    Ok(Some(edit_text(r, name)?))
}
/// Screen rectangle of a header widget.
pub(super) fn header_rect(r: &Reader<'_>, image: u64, name: &str) -> Result<Rect> {
    let header = header(r, image)?
        .ok_or_else(|| Error::new("gui_target_unavailable", "Fleet panel is not open"))?;
    gui_tree::rect(r, image, child_of(r, image, header, name)?, GAME)
}
#[derive(Clone, Debug, PartialEq)]
pub(super) struct Card {
    pub frame: Rect,
    pub reserve: bool,
    pub task_forces: i32,
}
/// Fleet cards of the bottom bar, in bar order (own fleets, then the reserve fleet).
pub(super) fn cards(r: &Reader<'_>, image: u64) -> Result<Vec<Card>> {
    let bar = bottom_bar(r, image)?;
    let centering = window_of(r, image, bar, "centering_box")?;
    let grid = child_of(r, image, centering, "fleets_box")?;
    let mut out = Vec::new();
    for item in grid_items(r, grid)? {
        let Some(window) = gui_tree::container(r, r.u64(item + ITEM_CONTAINER)?, image)? else {
            continue;
        };
        let frame = child_of(r, image, window, "portrait_frame")?;
        let count = child_of(r, image, window, "taskforce_count")?;
        let reserve = gui_tree::child(r, image, window, "btn_disband_navy")?.is_none();
        out.push(Card {
            frame: gui_tree::rect_unclipped(r, image, frame, GAME)?,
            reserve,
            task_forces: text_at(r, count)?.trim().parse().unwrap_or(0),
        });
    }
    Ok(out)
}
#[derive(Clone, Debug, PartialEq)]
pub(super) struct Entry {
    pub name: String,
    pub select: Rect,
    pub selected: bool,
    pub y: i32,
}
/// Task force entries of the open panel with their select button state.
pub(super) fn entries(r: &Reader<'_>, image: u64) -> Result<Vec<Entry>> {
    let v = view(r, image, SLOT_NAVIES)?;
    let (_, tf_data, tf_count) = r.array_header(v + NV_TASK_FORCES)?;
    if tf_count > 64 {
        return Err(Error::new("invalid_array", "Task force list exceeds bound"));
    }
    let mut shown = Vec::new();
    for window in entry_windows(r, image)? {
        if gui_tree::visible(r, window)? {
            shown.push(window);
        }
    }
    if shown.len() != tf_count {
        return Err(Error::new("invalid_gui", "Task force list and shown entries disagree"));
    }
    let mut out = Vec::new();
    for (index, window) in shown.into_iter().enumerate() {
        let tf = r.u64(tf_data + 8 * index as u64)?;
        let name = r.string_bounded(tf + TF_NAME, 128)?;
        let select = child_of(r, image, window, "select_btn")?;
        let rect = gui_tree::rect_unclipped(r, image, select, GAME)?;
        let label = label_at(r, select)?;
        let top = gui_tree::rect_unclipped(r, image, child_of(r, image, window, "background_sprite")?, GAME)?;
        out.push(Entry {
            name,
            select: rect,
            selected: label.eq_ignore_ascii_case("back"),
            y: top.y,
        });
    }
    Ok(out)
}
/// Mission bar button: (rectangle, raw state: 0 normal, 2 armed, 3 disabled).
pub(super) fn bar_button(r: &Reader<'_>, image: u64, name: &str) -> Result<(Rect, i32)> {
    let bar = action_bar(r, image)?;
    let button = child_of(r, image, bar, name)?;
    if !gui_tree::shown(r, button)? {
        return Err(Error::new("gui_target_unavailable", format!("{name} is not shown")));
    }
    Ok((
        gui_tree::rect(r, image, button, GAME)?,
        gui_tree::button_state(r, button)?,
    ))
}
/// The naval region view (opened by a left click on a sea region): (name, close button).
pub(super) fn region_view(r: &Reader<'_>, image: u64) -> Result<Option<(String, Rect)>> {
    let view = view(r, image, REGION_VIEW_SLOT)?;
    let Some(root) = gui_tree::container(r, r.u64(view + REGION_ROOT)?, image)? else {
        return Ok(None);
    };
    if !gui_tree::visible(r, root)? {
        return Ok(None);
    }
    let name = text_at(r, child_of(r, image, root, "region_name")?)?;
    let close = gui_tree::rect(r, image, child_of(r, image, root, "close_button")?, GAME)?;
    Ok(Some((name, close)))
}

// ---- the admiral list -------------------------------------------------------------------------
/// One row of the "Select Commander" list: name, skill level and the portrait rectangle (the
/// portrait is drawn 90 px tall from the bottom of its texture rectangle, as in the army list).
#[derive(Clone, Debug, PartialEq)]
pub(super) struct Admiral {
    pub name: String,
    pub skill: String,
    pub icon: Rect,
}
pub(super) fn leader_open(r: &Reader<'_>, image: u64) -> Result<bool> {
    gui_tree::visible(r, leader_root(r, image)?)
}
pub(super) fn leader_rows(r: &Reader<'_>, image: u64) -> Result<Vec<Admiral>> {
    let root = leader_root(r, image)?;
    let list = window_of(r, image, root, "leaders")?;
    let grid = child_of(r, image, list, "leaders_grid")?;
    let mut out = Vec::new();
    for item in grid_items(r, grid)?.into_iter().take(256) {
        let Some(frame) = gui_tree::container(r, r.u64(item + ITEM_CONTAINER)?, image)? else {
            continue;
        };
        let (Some(name), Some(icon)) = (
            gui_tree::child(r, image, frame, "name")?,
            gui_tree::child(r, image, frame, "leader_icon")?,
        ) else {
            continue;
        };
        let skill = match gui_tree::child(r, image, frame, "skill")? {
            Some(s) => text_at(r, s)?,
            None => String::new(),
        };
        if let Ok(rect) = gui_tree::rect_unclipped(r, image, icon, GAME) {
            out.push(Admiral {
                name: text_at(r, name)?,
                skill,
                icon: rect,
            });
        }
    }
    Ok(out)
}
/// Visible band (top, bottom) of the list rows: from the sort header to the Unassign button.
pub(super) fn leader_band(r: &Reader<'_>, image: u64) -> Result<(i32, i32)> {
    let root = leader_root(r, image)?;
    let header = gui_tree::rect_unclipped(r, image, child_of(r, image, root, "sort_name")?, GAME)?;
    let footer = gui_tree::rect_unclipped(r, image, child_of(r, image, root, "no_leader_button")?, GAME)?;
    Ok((header.y + header.h, footer.y))
}
pub(super) fn leader_button(r: &Reader<'_>, image: u64, name: &str) -> Result<Rect> {
    let root = leader_root(r, image)?;
    gui_tree::rect(r, image, child_of(r, image, root, name)?, GAME)
}
pub(super) const PORTRAIT_DRAWN: i32 = 90;

/// The bottom bar's "+" card (new fleet from the selected task forces): (rectangle, state; 3 disabled).
pub(super) fn new_fleet_card(r: &Reader<'_>, image: u64) -> Result<(Rect, i32)> {
    let bar = bottom_bar(r, image)?;
    let centering = window_of(r, image, bar, "centering_box")?;
    let card = window_of(r, image, centering, "new_fleet_window")?;
    let button = child_of(r, image, card, "portrait_btn")?;
    // The plain button reads disabled (3) at all times; the highlighted twin is shown only while
    // task forces are selected, which is when the card works (seen live 26 October 2026).
    let lit = gui_tree::shown(r, child_of(r, image, card, "portrait_highlighted_btn")?)?;
    Ok((gui_tree::rect(r, image, button, GAME)?, if lit { 0 } else { 3 }))
}

// ---- debug dump ------------------------------------------------------------------------------
fn printable(s: &str) -> bool {
    !s.is_empty() && s.chars().all(|c| !c.is_control() || c == '\u{11}' || c == '\u{12}' || c == '\u{13}')
}
fn dump_widget(r: &Reader<'_>, image: u64, pointer: u64, depth: usize) -> Result<Value> {
    let object = gui_tree::container(r, pointer, image)?;
    let base = match object {
        Some(o) => o + 0x30,
        None => pointer,
    };
    let name = gui_tree::name(r, base, image)?;
    let class = r.u64(base)?.wrapping_sub(image);
    let shown = gui_tree::shown(r, base).unwrap_or(false);
    let rect = gui_tree::rect_unclipped(r, image, base, GAME)
        .ok()
        .map(|x| json!([x.x, x.y, x.w, x.h]));
    let mut v = json!({"name":name,"vt":format!("{class:#x}"),"shown":shown,"rect":rect});
    if let Some(c) = object {
        v["visible"] = json!(gui_tree::visible(r, c)?);
        if depth > 0 {
            let mut kids = Vec::new();
            for child in gui_tree::children(r, c)? {
                kids.push(dump_widget(r, image, child, depth - 1)?);
            }
            v["children"] = Value::Array(kids);
        }
    } else {
        if let Ok(t) = text_at(r, base) {
            if printable(&t) && t.len() < 200 {
                v["text"] = json!(t);
            }
        }
        if let Ok(t) = label_at(r, base) {
            if printable(&t) && t.len() < 200 {
                v["label"] = json!(t);
            }
        }
        if let Ok(s) = gui_tree::button_state(r, base) {
            v["state"] = json!(s);
        }
    }
    Ok(v)
}
/// Debug: dump the tree below a named root.
pub(super) fn dump(process: &dyn Process, args: &Map<String, Value>) -> Result<Value> {
    let which = args.get("root").and_then(Value::as_str).unwrap_or("panel");
    let depth = args.get("depth").and_then(Value::as_u64).unwrap_or(2).min(8) as usize;
    let index = args.get("index").and_then(Value::as_u64).unwrap_or(0) as usize;
    let path: Vec<String> = args
        .get("path")
        .and_then(Value::as_str)
        .map(|p| p.split('/').filter(|s| !s.is_empty()).map(str::to_string).collect())
        .unwrap_or_default();
    read(process, |r, image| {
        let mut base = match which {
            "panel" => panel(r, image)?,
            "action" => action_bar(r, image)?,
            "leaders" => leader_root(r, image)?,
            "target" => target_bar(r, image)?,
            "toolbar" => tool_bar(r, image)?,
            "bottom" => bottom_bar(r, image)?,
            "entry" => *entry_windows(r, image)?
                .get(index)
                .ok_or_else(|| Error::new("invalid_target", "No such entry"))?,
            _ => return Err(Error::new("invalid_arguments", "Unknown root")),
        };
        for step in &path {
            let object = gui_tree::container(r, base, image)?.unwrap_or(base);
            base = gui_tree::child(r, image, object, step)?
                .ok_or_else(|| Error::new("gui_target_unavailable", format!("{step} not found")))?;
        }
        if args.get("items").and_then(Value::as_bool).unwrap_or(false) {
            let mut out = Vec::new();
            for item in grid_items(r, base)? {
                let frame = r.u64(item + ITEM_CONTAINER)?;
                out.push(dump_widget(r, image, frame_base(r, image, frame)?, depth)?);
            }
            return Ok(Value::Array(out));
        }
        dump_widget(r, image, base, depth)
    })
}
/// GUI-object base of a grid item's window pointer (stored as the object or as its base).
pub(super) fn frame_base(r: &Reader<'_>, image: u64, frame: u64) -> Result<u64> {
    match gui_tree::container(r, frame, image)? {
        Some(object) => Ok(object),
        None => Ok(frame),
    }
}
