//! Train own divisions with ordinary input: deployment panel (U) and the
//! template's Train button, once per division. Each click must add one
//! training line for that template to the own deployment queue in memory.
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

struct Ui {
    manager: u64,
    view: u64,
    view_root: u64,
    template_window: u64,
    window_rows: u64,
    row_container: u64,
    country_templates: u64,
    template_name: u64,
    country_deployment: u64,
    conveyors: u64,
    conveyor_template: u64,
    location_label: u64,
    grid_items: u64,
    item_container: u64,
}
/// macOS Mach-O 089EA519, mapped live on 18 October 2026 (Germany test run):
/// the template window's rows (+0xb90) follow the country's template list
/// (+0x180, name +0x20) one to one, and every Train click added one
/// conveyor (+0x50 of the deployment status, template +0x18).
const MACOS: Ui = Ui {
    manager: 0x670,
    view: 0x138,
    view_root: 0x430,
    template_window: 0x438,
    window_rows: 0xb90,
    row_container: 0x10,
    country_templates: 0x180,
    template_name: 0x20,
    country_deployment: 0xd18,
    conveyors: 0x50,
    conveyor_template: 0x18,
    // Label of a training line's location button: a line without a location
    // never deploys its trained division. Mapped live on 18 October 2026.
    location_label: 0x158,
    grid_items: 0x190,
    item_container: 0x10,
};
/// Visible band of the deployment list.
const LIST: (i32, i32) = (360, 1050);
const GAME: (u32, u32) = (1920, 1080);

fn ui(r: &Reader<'_>) -> Result<&'static Ui> {
    match r.string_abi() {
        StringAbi::LibCxx => Ok(&MACOS),
        StringAbi::LibStdCxx => Err(Error::new(
            "capability_unavailable",
            "Deployment input is not reviewed for this build",
        )),
    }
}
fn own_country(r: &Reader<'_>, image: u64) -> Result<u64> {
    let g = game::layout(r);
    let gs = r.u64(image + g.gs_global)?;
    let tag = r.i32(gs + g.player_tag)?;
    let index = i32::from_le_bytes(
        r.array_entry(gs + g.tag_country_index, tag as usize, 4)?
            .try_into()
            .unwrap(),
    );
    let country = u64::from_le_bytes(
        r.array_entry(gs + g.countries, index as usize, 8)?
            .try_into()
            .unwrap(),
    );
    if r.u64(country)? != image + g.country_vtable {
        return Err(Error::new("invalid_type", "Own country required"));
    }
    Ok(country)
}
fn pointers(r: &Reader<'_>, header: u64, bound: usize) -> Result<Vec<u64>> {
    let (_, data, count) = r.array_header(header)?;
    if count > bound {
        return Err(Error::new("invalid_array", "List exceeds bound"));
    }
    let bytes = if count == 0 {
        Vec::new()
    } else {
        r.read(data, count * 8)?
    };
    Ok(bytes
        .chunks_exact(8)
        .map(|b| u64::from_le_bytes(b.try_into().unwrap()))
        .collect())
}
/// Own division template names in list order.
pub(super) fn templates(r: &Reader<'_>, image: u64) -> Result<Vec<String>> {
    let u = ui(r)?;
    let country = own_country(r, image)?;
    pointers(r, country + u.country_templates, 256)?
        .into_iter()
        .map(|t| r.string_bounded(t + u.template_name, 128))
        .collect()
}
/// Training lines per template name in the own deployment queue.
pub(super) fn queued(r: &Reader<'_>, image: u64, name: &str) -> Result<usize> {
    let u = ui(r)?;
    let country = own_country(r, image)?;
    let status = r.u64(country + u.country_deployment)?;
    let mut count = 0;
    for line in pointers(r, status + u.conveyors, 1024)? {
        let template = r.u64(line + u.conveyor_template)?;
        if template != 0 && r.string_bounded(template + u.template_name, 128)? == name {
            count += 1;
        }
    }
    Ok(count)
}
fn view(r: &Reader<'_>, image: u64) -> Result<u64> {
    let idler = r.u64(image + game::layout(r).idler_global)?;
    r.u64(r.u64(idler + ui(r)?.manager)? + ui(r)?.view)
}
fn open(r: &Reader<'_>, image: u64) -> Result<bool> {
    let root = gui_tree::container(r, r.u64(view(r, image)? + ui(r)?.view_root)?, image)?
        .ok_or_else(|| Error::new("invalid_gui", "Deployment view root unavailable"))?;
    gui_tree::visible(r, root)
}
/// Train button of the row for template `name`.
fn train_button(r: &Reader<'_>, image: u64, name: &str) -> Result<Rect> {
    let u = ui(r)?;
    let names = templates(r, image)?;
    let window = r.u64(view(r, image)? + u.template_window)?;
    let rows = pointers(r, window + u.window_rows, 256)?;
    if !names.iter().any(|n| n == name) {
        return Err(Error::new(
            "invalid_target",
            "No own division template with this name",
        ));
    }
    // Rows follow the template list one to one unless the game hides
    // decommissioned templates; then each row is found by its drawn name.
    let row = if rows.len() == names.len() {
        rows[names.iter().position(|n| n == name).unwrap()]
    } else {
        let mut found = None;
        for &candidate in &rows {
            let Some(frame) = gui_tree::container(r, r.u64(candidate + u.row_container)?, image)?
            else {
                continue;
            };
            let Some(label) = gui_tree::child(r, image, frame, "name")? else {
                continue;
            };
            if r.string_bounded(label + 0x100, 128)?.trim() == name {
                found = Some(candidate);
                break;
            }
        }
        found.ok_or_else(|| {
            Error::new(
                "gui_target_unavailable",
                "Template row not shown (decommissioned templates are hidden)",
            )
        })?
    };
    let frame = gui_tree::container(r, r.u64(row + u.row_container)?, image)?
        .ok_or_else(|| Error::new("invalid_gui", "Template row window required"))?;
    let button = gui_tree::child(r, image, frame, "add_template_button")?
        .ok_or_else(|| Error::new("gui_target_unavailable", "Train button not found"))?;
    if gui_tree::disabled(r, button)? {
        return Err(Error::new(
            "invalid_target",
            "The game disables training this template",
        ));
    }
    gui_tree::rect(r, image, button, GAME)
}

/// Close the Recruit & Deploy panel when an earlier action left it open: it covers
/// the left half of the map, so a later box selection or map click would miss.
/// Best effort (the panel toggles with the same ordinary key that opened it).
pub(super) fn leave(process: &dyn Process, keys: &MacKeys) {
    if !read(process, open).unwrap_or(false) {
        return;
    }
    if keys.press(Key::letter('u').unwrap_or(Key::Escape)).is_err() {
        return;
    }
    let _ = wait(process, "close deployment", |r, i| Ok(!open(r, i)?));
}

/// Queue `count` divisions of template `name`; the caller has journaled intent.
pub(super) fn train(process: &dyn Process, keys: &MacKeys, name: &str, count: usize) -> Result<()> {
    clear_notices(process, keys)?;
    if !read(process, open)? {
        keys.press(Key::letter('u')?)?;
        wait(process, "open deployment", open)?;
    }
    let start = read(process, |r, i| queued(r, i, name))?;
    for done in 0..count {
        let rect = settled(process, "train button", |r, i| train_button(r, i, name))?;
        clear_notices(process, keys)?;
        let (x, y) = rect.center();
        keys.click(GAME, x, y)?;
        wait(process, "training line added", |r, i| {
            Ok(queued(r, i, name)? > start + done)
        })?;
    }
    Ok(())
}

/// Location labels and buttons of the training lines, in list order. The
/// conveyor's own province field is only a pending choice the game clears
/// once applied, so the drawn label is the reliable state: a state name, or
/// "No location set".
fn location_rows(r: &Reader<'_>, image: u64) -> Result<Vec<(bool, Option<Rect>)>> {
    let u = ui(r)?;
    let root = gui_tree::container(r, r.u64(view(r, image)? + u.view_root)?, image)?
        .ok_or_else(|| Error::new("invalid_gui", "Deployment view root unavailable"))?;
    let grid = find(r, image, root, "deployments_list_gridbox", 0)?
        .ok_or_else(|| Error::new("gui_target_unavailable", "Deployment list not found"))?;
    let items = pointers(r, grid + u.grid_items, 1024)?;
    let mut out = Vec::new();
    for item in items {
        let kind =
            crate::research_ui::primary_type_encoding(r, image, r.u64(item)?).unwrap_or_default();
        if kind != "31CMilitaryDeploymentConveyorView" {
            continue;
        }
        let Some(frame) = gui_tree::container(r, r.u64(item + u.item_container)?, image)? else {
            continue;
        };
        if let Some(button) = gui_tree::child(r, image, frame, "deploy_location_button")? {
            let label = r.string_bounded(button + u.location_label, 128)?;
            let located = !label.is_empty() && !label.contains("No location set");
            out.push((
                located,
                gui_tree::rect_unclipped(r, image, button, GAME).ok(),
            ));
        }
    }
    Ok(out)
}
/// Training lines without a deployment location.
pub(super) fn unlocated(r: &Reader<'_>, image: u64) -> Result<usize> {
    Ok(location_rows(r, image)?.iter().filter(|(l, _)| !l).count())
}
/// Location buttons of training lines without a location, in list order.
fn location_buttons(r: &Reader<'_>, image: u64) -> Result<Vec<Rect>> {
    Ok(location_rows(r, image)?
        .into_iter()
        .filter(|(l, _)| !l)
        .filter_map(|(_, rect)| rect)
        .collect())
}
fn find(r: &Reader<'_>, image: u64, object: u64, name: &str, depth: u32) -> Result<Option<u64>> {
    for child in gui_tree::children(r, object)? {
        if gui_tree::name(r, child, image)?.as_deref() == Some(name) {
            return Ok(Some(child));
        }
        if depth < 5 {
            if let Some(inner) = gui_tree::container(r, child, image)? {
                if let Some(found) = find(r, image, inner, name, depth + 1)? {
                    return Ok(Some(found));
                }
            }
        }
    }
    Ok(None)
}

/// Give every training line without a location the own state `state`: the
/// Find view centres the state, then each line's location button and one
/// click on the state. Verified by one fewer unlocated line per step.
pub(super) fn set_locations(process: &dyn Process, keys: &MacKeys, state: i32) -> Result<usize> {
    clear_notices(process, keys)?;
    let total = read(process, unlocated)?;
    if total == 0 {
        return Ok(0);
    }
    find_state(process, keys, state)?;
    if !read(process, open)? {
        keys.press(Key::letter('u')?)?;
        wait(process, "open deployment", open)?;
    }
    let mut done = 0;
    for _ in 0..total.min(64) {
        let before = read(process, unlocated)?;
        if before == 0 {
            break;
        }
        // Scroll the first unlocated line's button into the list area.
        let mut rect = None;
        for _ in 0..40 {
            let found = read(process, location_buttons)?.first().copied();
            match found {
                Some(r) if r.y >= LIST.0 && r.y + r.h <= LIST.1 => {
                    rect = Some(r);
                    break;
                }
                Some(r) => {
                    let pixels = if r.y < LIST.0 { 240 } else { -240 };
                    keys.scroll(GAME, (180.0, 700.0), pixels)?;
                }
                None => break,
            }
        }
        if rect.is_none() {
            return Err(Error::new(
                "gui_target_unavailable",
                "Training line location button not in view",
            ));
        }
        // The panel slides in and the list scrolls smoothly; click a resting target.
        let rect = settled(process, "location button", |r, i| {
            location_buttons(r, i)?
                .first()
                .copied()
                .filter(|r| r.y >= LIST.0 && r.y + r.h <= LIST.1)
                .ok_or_else(|| Error::new("gui_target_unavailable", "Location button moved"))
        })?;
        // The text sprite is wider (446 px) than the drawn location field
        // (about 230 px from its left edge); click the field's middle.
        let (x, y) = (f64::from(rect.x) + 115.0, rect.center().1);
        keys.click(GAME, x, y)?;
        std::thread::sleep(std::time::Duration::from_millis(300));
        keys.click(GAME, 960.0, 540.0)?;
        wait(process, "deployment location set", |r, i| {
            Ok(unlocated(r, i)? < before)
        })
        .map_err(|_| {
            Error::new(
                "input_readback_failed",
                "No location was set (the state must be own and able to receive divisions)",
            )
        })?;
        done += 1;
    }
    Ok(done)
}
