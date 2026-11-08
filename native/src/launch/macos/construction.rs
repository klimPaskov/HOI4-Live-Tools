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
        game_input::{GameInput, Key},
        Process,
    },
    xlayout::{fld, img, Class},
};
use std::{thread, time::Duration};

#[derive(Clone, Copy)]
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
    view_root: 0x490,
    grid_items: 0x190,
    item_vtable: 0x3647398,
    item_container: 0x10,
    item_template: 0x20,
    template_key: 0x228,
    selected_template: 0x3b8,
    selection_mode: 0x3c0,
};
/// Owners of the fields above (macOS 1.19.4.1 RTTI names and vtables; 0 where
/// none was reviewed).
const IDLER: Class = Class::new("CInGameIdler", 0x35089d8);
const MANAGER: Class = Class::new("idler.manager", 0);
const VIEW: Class = Class::new("construction.view", 0);
const GRID: Class = Class::new("CStandardGridBox", 0x36cca80);
const GRID_ITEM: Class = Class::new("grid.item", 0);
const ITEM: Class = Class::new("CCountryConstructionViewBuildingItem", 0x3647398);
const TEMPLATE: Class = Class::new("CBuildingTemplate", 0x352fa88);
impl Ui {
    /// The macOS keys translated for this build (identity on macOS).
    fn resolve(&self, abi: StringAbi) -> Result<Self> {
        Ok(Self {
            manager: fld(abi, &IDLER, self.manager)?,
            view: fld(abi, &MANAGER, self.view)?,
            view_root: fld(abi, &VIEW, self.view_root)?,
            grid_items: fld(abi, &GRID, self.grid_items)?,
            item_vtable: img(abi, self.item_vtable)?,
            item_container: fld(abi, &GRID_ITEM, self.item_container)?,
            item_template: fld(abi, &ITEM, self.item_template)?,
            template_key: fld(abi, &TEMPLATE, self.template_key)?,
            selected_template: fld(abi, &MANAGER, self.selected_template)?,
            selection_mode: fld(abi, &MANAGER, self.selection_mode)?,
        })
    }
}
const GAME: (u32, u32) = (1920, 1080);
const GRIDS: [(&str, &str); 4] = [
    ("possible_state_constructions", "non_shared_item_grid"),
    ("possible_shared_constructions", "shared_item_grid"),
    ("possible_province_constructions", "province_item_grid"),
    ("possible_facility_constructions", "facility_item_grid"),
];

fn ui(r: &Reader<'_>) -> Result<Ui> {
    MACOS.resolve(r.string_abi())
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
    let observed = loop {
        match construction::target_snapshot::observe(process, key, state) {
            Err(e) if e.code == "torn_read" && attempt < 8 => {
                attempt += 1;
                thread::sleep(Duration::from_millis(10 * attempt));
            }
            result => break result?,
        }
    };
    if !observed.summary.complete {
        return Err(Error::new(
            "construction_target_unavailable",
            "Complete own construction queue evidence required",
        ));
    }
    Ok(observed.summary.in_state)
}
pub(super) fn leave(process: &dyn Process, keys: &dyn GameInput) {
    // Switch placement mode off: Escape closes the selection, then the
    // panel; each Escape is given time to act so none reaches the game menu.
    super::ui::dismiss(process, keys, |r, i| {
        Ok(selection(r, i)?.0 == 0 && !open(r, i)?)
    });
}

/// Append what the construction manager selected instead (mode and building) to a
/// selection failure, so an intermittent refusal explains itself.
fn with_selection(process: &dyn Process, mut e: Error) -> Error {
    if let Ok((mode, key)) = read(process, selection) {
        let open = read(process, open).unwrap_or(false);
        e.message = format!(
            "{} [observed afterwards: panel open {open}, selection mode {mode}, selected {:?}]",
            e.message, key
        );
    }
    e
}
/// The building button click was sent but the manager's selection never named
/// the building. Other codes (ownership, lease, torn reads) keep their cause.
fn select_failed(error: Error, key: &str) -> Error {
    if error.code != "input_readback_failed" {
        return error;
    }
    Error::new(
        "building_select_failed",
        format!(
            "Observed: after one click on the {key} button the construction manager's selection did not \
             name {key} within 3 s; nothing was placed and no map click was sent. Not inferred: whether \
             the click missed, a pop-up took it, or the game refused the selection. Re-read hoi4_events \
             and the construction queue before trying again; do not assume the state has no free slot."
        ),
    )
}
/// One centre click was sent with the building selected, yet the own queue in
/// the state did not grow within 3 s. Memory alone cannot tell which cause.
fn no_slot_or_not_allowed(key: &str, state: i32) -> Error {
    Error::new(
        "no_free_slot_or_not_allowed",
        format!(
            "Observed: with {key} selected and state {state} centred, one centre click did not raise the \
             queued {key} in state {state} within 3 s. Most likely the state has no free building slot or \
             {key} is not allowed there, but a click that missed the state or a pop-up is not excluded, and \
             nothing was retried. Check hoi4_state_slots for state {state} (or choose another state) before \
             trying again; do not repeat the same call."
        ),
    )
}

/// Queue `count` more of building `key` in own state `state`.
/// Buildings placed on one province (the panel's province grid): a click must land on a
/// province the game allows, so the state centre is not enough.
pub(super) const PROVINCE_BUILDINGS: [&str; 4] =
    ["supply_node", "naval_base", "bunker", "coastal_bunker"];

/// Screen points to click for a building: the given province, or every province of the state
/// that projects on screen (nearest the centre first). State buildings fall back to the screen
/// centre when the map data is unavailable. A state's centre can lie outside the state (long
/// coastal states such as Eritrea: an infrastructure click there was refused live, 6 Nov 2026),
/// while a click on any of its own provinces places a state building in it.
fn aim_points(
    process: &dyn Process,
    key: &str,
    state: i32,
    province: Option<i32>,
) -> Result<Vec<(f64, f64)>> {
    let wants_province = province.is_some() || PROVINCE_BUILDINGS.contains(&key);
    if !wants_province {
        return Ok(match province_points(process, state, None) {
            Ok(points) if !points.is_empty() => points,
            _ => vec![(960.0, 540.0)],
        });
    }
    province_points(process, state, province)
}

fn province_points(
    process: &dyn Process,
    state: i32,
    province: Option<i32>,
) -> Result<Vec<(f64, f64)>> {
    let dir = process
        .identity()
        .and_then(|i| super::mapdata::game_dir(&i.exe).ok())
        .ok_or_else(|| Error::new("map_data_unavailable", "Game install not found"))?;
    let positions = super::mapdata::positions(&dir)?;
    let provinces = match province {
        Some(p) => vec![p],
        None => super::mapdata::state_provinces(&dir, state)?,
    };
    let mut points: Vec<(f64, f64)> = Vec::new();
    for p in provinces {
        let Some(&(x, z)) = positions.get(&p) else {
            continue;
        };
        if let Some(point) = read(process, |r, i| super::screenmap::to_screen(r, i, x, z))? {
            // keep clear of the top bar, the panels on the left and the map controls
            if (380.0..1800.0).contains(&point.0) && (90.0..1000.0).contains(&point.1) {
                points.push(point);
            }
        }
    }
    if points.is_empty() {
        return Err(Error::new(
            "invalid_target",
            match province {
                Some(p) => format!("Province {p} is not on screen after centring state {state} (is it in that state?)"),
                None => format!("No province of state {state} projects on screen"),
            },
        ));
    }
    // nearest to the screen centre first
    points.sort_by(|a, b| {
        let d = |p: &(f64, f64)| (p.0 - 960.0).powi(2) + (p.1 - 540.0).powi(2);
        d(a).total_cmp(&d(b))
    });
    points.truncate(10);
    Ok(points)
}

/// Open the construction panel and select building `key` (also railway mode for `rail_way`),
/// verified from the construction manager's selection.
pub(super) fn select_building(
    process: &dyn Process,
    keys: &dyn GameInput,
    key: &str,
) -> Result<()> {
    if !read(process, open)? {
        keys.press(Key::letter('t')?)?;
        wait(process, "open construction", open)?;
    }
    let deadline = std::time::Instant::now() + Duration::from_secs(2);
    while read(process, |r, i| button(r, i, key))?.is_none() {
        if std::time::Instant::now() >= deadline {
            return Err(Error::new(
                "invalid_target",
                "This building is not offered in the construction panel",
            ));
        }
        thread::sleep(Duration::from_millis(100));
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
    })
    .map_err(|e| with_selection(process, select_failed(e, key)))
}

pub(super) fn add(
    process: &dyn Process,
    keys: &dyn GameInput,
    key: &str,
    state: i32,
    count: i64,
    province: Option<i32>,
) -> Result<()> {
    if key == "rail_way" || key == "railway" {
        return Err(Error::new(
            "use_railway_tool",
            "Railways run between provinces, not in one state: use hoi4_railway_build \
             (from_state/from_province to to_state/to_province). Nothing was clicked.",
        ));
    }
    clear_notices(process, keys)?;
    let start = queued(process, key, state)?;
    if !read(process, open)? {
        keys.press(Key::letter('t')?)?;
        wait(process, "open construction", open)?;
    }
    // The panel fills its building grids a moment after it opens; an empty read right after
    // opening is not a refusal (seen live 1 Nov 2026), so wait up to 2 s for the button.
    let deadline = std::time::Instant::now() + Duration::from_secs(2);
    loop {
        if read(process, |r, i| button(r, i, key))?.is_some() {
            break;
        }
        if std::time::Instant::now() >= deadline {
            return Err(Error::new(
                "invalid_target",
                "This building is not offered in the construction panel",
            ));
        }
        thread::sleep(Duration::from_millis(100));
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
    })
    .map_err(|e| with_selection(process, select_failed(e, key)))?;
    let result = (|| {
        // The state already in the open map (right of the construction panel) at aiming
        // height: click where the camera is. Otherwise the Find view centres it (it waits for
        // the camera to come to rest).
        let open_map =
            |p: &(f64, f64)| (740.0..1700.0).contains(&p.0) && (120.0..900.0).contains(&p.1);
        let visible: Vec<(f64, f64)> = if super::screenmap::aim_height_ok(process) {
            province_points(process, state, province)
                .map(|v| v.into_iter().filter(open_map).collect())
                .unwrap_or_default()
        } else {
            Vec::new()
        };
        let points = if visible.is_empty() {
            find_state(process, keys, state)?;
            aim_points(process, key, state, province)?
        } else {
            visible
        };
        let mut point = 0;
        for click in 0..count.min(20) {
            if click > 0 {
                // A click right after a placement was ignored by 1.19.4 (gate1194, 1 Nov 2026):
                // the queue never grew while a fresh call on the same state worked.
                thread::sleep(Duration::from_millis(600));
            }
            if read(process, selection)?.1.as_deref() != Some(key) {
                return Err(Error::new(
                    "input_readback_failed",
                    "Placement mode ended before all clicks",
                ));
            }
            let before = queued(process, key, state)?;
            // Province buildings: try the next province when a click is refused.
            loop {
                super::ui::dismiss_notices(process, keys)?;
                let (x, y) = points[point];
                keys.click(GAME, x, y)?;
                let wait_for = if points.len() > 1 { 1200 } else { 3000 };
                let deadline = std::time::Instant::now() + Duration::from_millis(wait_for);
                let grew = loop {
                    if queued(process, key, state)? > before {
                        break true;
                    }
                    if std::time::Instant::now() >= deadline {
                        break false;
                    }
                    thread::sleep(Duration::from_millis(40));
                };
                if grew {
                    break;
                }
                point += 1;
                if point >= points.len() || read(process, selection)?.1.as_deref() != Some(key) {
                    return Err(no_slot_or_not_allowed(key, state));
                }
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

/// Shown widgets named `name` anywhere in the open construction view (child windows and grid
/// items), top to bottom.
fn row_buttons(r: &Reader<'_>, image: u64, name: &str) -> Result<Vec<Rect>> {
    fn walk(
        r: &Reader<'_>,
        image: u64,
        object: u64,
        name: &str,
        depth: u32,
        out: &mut Vec<Rect>,
    ) -> Result<()> {
        for child in gui_tree::children(r, object)? {
            if gui_tree::name(r, child, image)?.as_deref() == Some(name)
                && gui_tree::shown(r, child)?
            {
                let rect = gui_tree::rect(r, image, child, GAME)?;
                if rect.w > 0 && rect.h > 0 {
                    out.push(rect);
                }
            }
            if depth == 0 {
                continue;
            }
            if let Some(inner) = gui_tree::container(r, child, image)? {
                walk(r, image, inner, name, depth - 1, out)?;
            }
            if let Some(frames) = super::panelkit::grid_frames(r, image, child)? {
                for frame in frames {
                    walk(r, image, frame, name, depth - 1, out)?;
                }
            }
        }
        Ok(())
    }
    let mut out = Vec::new();
    walk(r, image, root(r, image)?, name, 10, &mut out)?;
    out.sort_by_key(|rect| (rect.y, rect.x));
    out.dedup();
    Ok(out)
}

/// Handles of the own construction lines in queue order.
fn queue_handles(process: &dyn Process) -> Result<Vec<String>> {
    let page = construction::page(process, 0, 128)?;
    Ok(page["lines"]
        .as_array()
        .map(|rows| {
            rows.iter()
                .filter_map(|l| l["line_handle"].as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default())
}

/// Move construction line `line` (0-based queue index) to the top, one up, one down, to the
/// bottom, or remove it, with the row's own buttons (countryconstructionsview.gui:
/// increase_priority, decrease_priority, close_production_line). Verified by the line's new
/// position in the own queue (or its absence after a removal).
pub(super) fn reorder(
    process: &dyn Process,
    keys: &dyn GameInput,
    line: usize,
    action: &str,
) -> Result<serde_json::Value> {
    let button = match action {
        "top" | "up" => "increase_priority",
        "down" | "bottom" => "decrease_priority",
        "remove" => "close_production_line",
        _ => {
            return Err(Error::new(
                "invalid_arguments",
                "action must be top, up, down, bottom or remove",
            ))
        }
    };
    let before = queue_handles(process)?;
    let n = before.len();
    let handle = before.get(line).cloned().ok_or_else(|| {
        Error::new(
            "invalid_target",
            format!("No construction line {line}: the queue has {n} (hoi4_construction_queue lists them in order)"),
        )
    })?;
    let wanted = match action {
        "top" => Some(0),
        "up" => Some(line.saturating_sub(1)),
        "down" => Some((line + 1).min(n - 1)),
        "bottom" => Some(n - 1),
        _ => None,
    };
    if wanted == Some(line) {
        return Ok(
            serde_json::json!({"line_handle": handle, "position": line, "unchanged": "already there"}),
        );
    }
    clear_notices(process, keys)?;
    if !read(process, open)? {
        keys.press(Key::letter('t')?)?;
        wait(process, "open construction", open)?;
    }
    let result = (|| -> Result<serde_json::Value> {
        let rows = settled(process, "construction rows", |r, i| {
            row_buttons(r, i, button)
        })?;
        if rows.len() != n {
            return Err(Error::new(
                "gui_target_unavailable",
                format!("The construction panel shows {} {button} buttons for {n} queue lines; nothing was clicked", rows.len()),
            ));
        }
        let (x, y) = rows[line].center();
        let position = |process: &dyn Process| -> Result<Option<usize>> {
            Ok(queue_handles(process)?.iter().position(|h| *h == handle))
        };
        match action {
            "remove" => {
                keys.click(GAME, x, y)?;
                if !super::ui::accept_command(process, keys)? {
                    // no confirmation pop-up was found: the removal may already be done
                }
                wait(process, "line removed", |_, _| {
                    Ok(position(process)?.is_none())
                })?;
                Ok(serde_json::json!({"line_handle": handle, "removed": true, "lines_before": n}))
            }
            _ => {
                // Shift moves a line to the top or bottom in one click; without effect the
                // single steps follow.
                if matches!(action, "top" | "bottom") {
                    keys.shift_click(GAME, x, y)?;
                } else {
                    keys.click(GAME, x, y)?;
                }
                let target = wanted.unwrap_or(line);
                let moved = wait(process, "line moved", |_, _| {
                    Ok(position(process)? == Some(target))
                });
                if moved.is_err() && matches!(action, "top" | "bottom") {
                    for _ in 0..n {
                        let Some(at) = position(process)? else { break };
                        if at == target {
                            break;
                        }
                        let rows = read(process, |r, i| row_buttons(r, i, button))?;
                        let Some(rect) = rows.get(at) else { break };
                        let (x, y) = rect.center();
                        keys.click(GAME, x, y)?;
                        thread::sleep(Duration::from_millis(400));
                    }
                }
                let now = position(process)?;
                if now != Some(target) {
                    return Err(Error::new(
                        "input_readback_failed",
                        format!(
                            "Line {handle} is at {now:?} after the {button} click, not at {target}"
                        ),
                    ));
                }
                Ok(serde_json::json!({"line_handle": handle, "from": line, "to": target}))
            }
        }
    })();
    leave(process, keys);
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn select_readback_timeout_is_typed_and_says_nothing_was_placed() {
        let e = select_failed(
            Error::new(
                "input_readback_failed",
                "select building: expected game change not observed",
            ),
            "arms_factory",
        );
        assert_eq!(e.code, "building_select_failed");
        assert!(e.message.contains("arms_factory"));
        assert!(e.message.contains("no map click was sent"));
        assert!(e.message.contains("do not assume"));
    }
    #[test]
    fn other_select_failures_keep_their_cause() {
        for code in ["ownership_changed", "torn_read", "input_lease_required"] {
            assert_eq!(select_failed(Error::new(code, "x"), "dockyard").code, code);
        }
    }
    #[test]
    fn centre_click_without_growth_is_typed_with_the_limits_of_the_evidence() {
        let e = no_slot_or_not_allowed("industrial_complex", 64);
        assert_eq!(e.code, "no_free_slot_or_not_allowed");
        assert!(e.message.contains("state 64"));
        assert!(e.message.contains("hoi4_state_slots"));
        assert!(e.message.contains("not excluded"));
        assert!(e.message.contains("nothing was retried"));
    }
}
