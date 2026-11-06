//! Train own divisions with ordinary input: deployment panel (U) and the
//! template's Train button, once per division. Each click must add one
//! training line for that template to the own deployment queue in memory.
use super::glide::{glide, span_y, Glide, Glided, Span};
use super::ui::{clear_notices, find_state, read, settled, wait};
use crate::{
    error::{Error, Result},
    game,
    gui_tree::{self, Rect},
    memory::Reader,
    platform::{
        game_input::{GameInput, Key},
        Process,
    },
    xlayout::{fld, Class},
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
    template_root: u64,
    text: u64,
    conveyor_amount: u64,
    conveyor_lines: u64,
}
/// macOS Mach-O 089EA519, mapped live on 18 October 2026 (Germany test run):
/// the template window's rows (+0xb90) follow the country's template list
/// (+0x180, name +0x20) one to one, and every Train click added one
/// conveyor (+0x50 of the deployment status, template +0x18).
const MACOS: Ui = Ui {
    manager: 0x670,
    view: 0x138,
    view_root: 0x490,
    template_window: 0x498,
    window_rows: 0xcb0,
    row_container: 0x10,
    country_templates: 0x180,
    template_name: 0x20,
    country_deployment: 0xd18,
    conveyors: 0x50,
    conveyor_template: 0x18,
    // Label of a training line's location button: a line without a location
    // never deploys its trained division. Mapped live on 18 October 2026.
    location_label: 0x160, // 1.19.4: CButtonStandard text +8 (was 0x158)
    grid_items: 0x190,
    item_container: 0x10,
    template_root: 0xd20,
    text: 0x100,
    conveyor_amount: CONVEYOR_AMOUNT,
    conveyor_lines: CONVEYOR_LINES,
};
/// Owners of the fields above (macOS 1.19.4.1 RTTI names and vtables). The
/// interface manager, the template window and its rows, division templates,
/// grid items and text widgets have no reviewed class yet; their Linux offsets
/// need an anchor function. The deployment view, status and conveyor classes
/// are named after the objects the comments describe.
const IDLER: Class = Class::new("CInGameIdler", 0x35089d8);
const MANAGER: Class = Class::new("idler.manager", 0);
const VIEW: Class = Class::new("CCountryDeploymentView", 0x364a878);
const TEMPLATE_WINDOW: Class = Class::new("deployment.template_window", 0);
const TEMPLATE_ROW: Class = Class::new("deployment.template_row", 0);
const COUNTRY: Class = Class::new("CCountry", 0x357e388);
const TEMPLATE: Class = Class::new("division_template", 0);
const STATUS: Class = Class::new("CDeploymentStatus", 0x358d850);
const CONVEYOR: Class = Class::new("CMilitaryDeploymentConveyor", 0x358dff8);
const BUTTON: Class = Class::new("gui.button", 0);
const GRID: Class = Class::new("CStandardGridBox", 0x36cca80);
const GRID_ITEM: Class = Class::new("grid.item", 0);
const TEXT_BOX: Class = Class::new("gui.text", 0);
/// Visible band of the deployment list.
const LIST: (i32, i32) = (360, 1050);
const GAME: (u32, u32) = (1920, 1080);

/// The layout for this build (macOS values translated through xlayout).
fn ui(r: &Reader<'_>) -> Result<Ui> {
    let abi = r.string_abi();
    let m = &MACOS;
    Ok(Ui {
        manager: fld(abi, &IDLER, m.manager)?,
        view: fld(abi, &MANAGER, m.view)?,
        view_root: fld(abi, &VIEW, m.view_root)?,
        template_window: fld(abi, &VIEW, m.template_window)?,
        window_rows: fld(abi, &TEMPLATE_WINDOW, m.window_rows)?,
        row_container: fld(abi, &TEMPLATE_ROW, m.row_container)?,
        country_templates: fld(abi, &COUNTRY, m.country_templates)?,
        template_name: fld(abi, &TEMPLATE, m.template_name)?,
        country_deployment: fld(abi, &COUNTRY, m.country_deployment)?,
        conveyors: fld(abi, &STATUS, m.conveyors)?,
        conveyor_template: fld(abi, &CONVEYOR, m.conveyor_template)?,
        location_label: fld(abi, &BUTTON, m.location_label)?,
        grid_items: fld(abi, &GRID, m.grid_items)?,
        item_container: fld(abi, &GRID_ITEM, m.item_container)?,
        template_root: fld(abi, &TEMPLATE_WINDOW, m.template_root)?,
        text: fld(abi, &TEXT_BOX, m.text)?,
        conveyor_amount: fld(abi, &CONVEYOR, m.conveyor_amount)?,
        conveyor_lines: fld(abi, &CONVEYOR, m.conveyor_lines)?,
    })
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
            // Parallel training lines of this queue (1.19.4.1 conveyor +0x70).
            count += r.i32(line + u.conveyor_lines)?.clamp(1, 1024) as usize;
        }
    }
    Ok(count)
}
fn view(r: &Reader<'_>, image: u64) -> Result<u64> {
    let idler = r.u64(image + game::layout(r).idler_global)?;
    let u = ui(r)?;
    r.u64(r.u64(idler + u.manager)? + u.view)
}
fn open(r: &Reader<'_>, image: u64) -> Result<bool> {
    let root = gui_tree::container(r, r.u64(view(r, image)? + ui(r)?.view_root)?, image)?
        .ok_or_else(|| Error::new("invalid_gui", "Deployment view root unavailable"))?;
    gui_tree::visible(r, root)
}
/// Row button `button` of the template row `name` and whether the game disables it.
pub(super) fn row_button(
    r: &Reader<'_>,
    image: u64,
    name: &str,
    button: &str,
) -> Result<(Rect, bool)> {
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
        let mut shown = Vec::new();
        for &candidate in &rows {
            let Some(frame) = gui_tree::container(r, r.u64(candidate + u.row_container)?, image)?
            else {
                continue;
            };
            let Some(label) = gui_tree::child(r, image, frame, "name")? else {
                continue;
            };
            let drawn = r.string_bounded(label + u.text, 128)?.trim().to_string();
            if drawn == name {
                found = Some(candidate);
                break;
            }
            shown.push(drawn);
        }
        found.ok_or_else(|| {
            Error::new(
                "template_not_trainable",
                format!(
                    "Template {name} is decommissioned (hidden from the training list). Trainable \
                     templates: {}. Nothing was queued.",
                    shown.join(", ")
                ),
            )
        })?
    };
    let frame = gui_tree::container(r, r.u64(row + u.row_container)?, image)?
        .ok_or_else(|| Error::new("invalid_gui", "Template row window required"))?;
    let widget = gui_tree::child(r, image, frame, button)?
        .ok_or_else(|| Error::new("gui_target_unavailable", format!("{button} not found")))?;
    Ok((
        gui_tree::rect(r, image, widget, GAME)?,
        gui_tree::disabled(r, widget)?,
    ))
}
/// Train button of the row for template `name`.
fn train_button(r: &Reader<'_>, image: u64, name: &str) -> Result<Rect> {
    let (rect, disabled) = row_button(r, image, name, "add_template_button")?;
    if disabled {
        return Err(Error::new(
            "invalid_target",
            "The game disables training this template",
        ));
    }
    Ok(rect)
}
/// The panel's own Division Designer button (container +0xc00 of the template window).
pub(super) fn designer_button(r: &Reader<'_>, image: u64) -> Result<Rect> {
    let u = ui(r)?;
    let window = r.u64(view(r, image)? + u.template_window)?;
    let root = gui_tree::container(r, r.u64(window + u.template_root)?, image)?
        .ok_or_else(|| Error::new("invalid_gui", "Template window root unavailable"))?;
    let button = gui_tree::child(r, image, root, "open_designer_button")?.ok_or_else(|| {
        Error::new(
            "gui_target_unavailable",
            "Division Designer button not found",
        )
    })?;
    gui_tree::rect(r, image, button, GAME)
}
/// Open the Recruit & Deploy panel (U) if it is closed.
pub(super) fn ensure_open(process: &dyn Process, keys: &dyn GameInput) -> Result<()> {
    if !read(process, open)? {
        keys.press(Key::letter('u')?)?;
        wait(process, "open deployment", open)?;
        // The panel slides in over about half a second and two reads in one frame look
        // "settled" in mid-slide (seen live: a click aimed at Edit landed on Train).
        std::thread::sleep(std::time::Duration::from_millis(1100));
    }
    Ok(())
}

/// Close the Recruit & Deploy panel when an earlier action left it open: it covers
/// the left half of the map, so a later box selection or map click would miss.
/// Best effort (the panel toggles with the same ordinary key that opened it).
pub(super) fn leave(process: &dyn Process, keys: &dyn GameInput) {
    if !read(process, open).unwrap_or(false) {
        return;
    }
    if keys.press(Key::letter('u').unwrap_or(Key::Escape)).is_err() {
        return;
    }
    let _ = wait(process, "close deployment", |r, i| Ok(!open(r, i)?));
}

/// Conveyor fields (1.19.4.1, mapped live 2 November 2026): series amount per line at +0x38
/// (0 = infinite: each line trains again forever), parallel lines at +0x70.
const CONVEYOR_AMOUNT: u64 = 0x38;
const CONVEYOR_LINES: u64 = 0x70;
/// Own conveyors in list order: (pointer, template name, amount, lines).
fn conveyors(r: &Reader<'_>, image: u64) -> Result<Vec<(u64, String, i32, i32)>> {
    let u = ui(r)?;
    let country = own_country(r, image)?;
    let status = r.u64(country + u.country_deployment)?;
    let mut out = Vec::new();
    for line in pointers(r, status + u.conveyors, 1024)? {
        let template = r.u64(line + u.conveyor_template)?;
        let name = if template != 0 {
            r.string_bounded(template + u.template_name, 128)?
        } else {
            String::new()
        };
        out.push((
            line,
            name,
            r.i32(line + u.conveyor_amount)?,
            r.i32(line + u.conveyor_lines)?,
        ));
    }
    Ok(out)
}
/// Button `name` of the `index`-th conveyor view (same order as the conveyor list).
fn conveyor_button(r: &Reader<'_>, image: u64, index: usize, name: &str) -> Result<Option<Rect>> {
    let u = ui(r)?;
    let root = gui_tree::container(r, r.u64(view(r, image)? + u.view_root)?, image)?
        .ok_or_else(|| Error::new("invalid_gui", "Deployment view root unavailable"))?;
    let grid = find(r, image, root, "deployments_list_gridbox", 0)?
        .ok_or_else(|| Error::new("gui_target_unavailable", "Deployment list not found"))?;
    let mut k = 0;
    for item in pointers(r, grid + u.grid_items, 1024)? {
        let kind =
            crate::research_ui::primary_type_encoding(r, image, r.u64(item)?).unwrap_or_default();
        if kind != "31CMilitaryDeploymentConveyorView" {
            continue;
        }
        if k == index {
            let Some(frame) = gui_tree::container(r, r.u64(item + u.item_container)?, image)?
            else {
                return Ok(None);
            };
            return Ok(match gui_tree::child(r, image, frame, name)? {
                Some(b) => gui_tree::rect_unclipped(r, image, b, GAME).ok(),
                None => None,
            });
        }
        k += 1;
    }
    Ok(None)
}
/// Glide the deployment list (only while the panel is open) until the measured span lies in the
/// list area, in one smooth movement.
fn glide_list(
    process: &dyn Process,
    keys: &dyn GameInput,
    position: impl Fn(&Reader<'_>, u64) -> Result<Option<Span>>,
) -> Result<Glided> {
    glide(
        process,
        keys,
        &Glide {
            at: (180.0, 700.0),
            pixels: 240,
            band: (f64::from(LIST.0), f64::from(LIST.1)),
            search: 0,
        },
        || {
            if read(process, open)? {
                Ok(())
            } else {
                Err(Error::new(
                    "input_readback_failed",
                    "Recruit & Deploy panel closed",
                ))
            }
        },
        position,
    )
}
/// Click button `name` of conveyor `index` once it rests inside the list area (scrolling it there).
fn click_conveyor(
    process: &dyn Process,
    keys: &dyn GameInput,
    index: usize,
    name: &str,
) -> Result<()> {
    // Long queue lists (seen live: 15 queues) glide the button in with one burst.
    let outcome = glide_list(process, keys, |r, i| {
        Ok(conveyor_button(r, i, index, name)?.map(|b| {
            let top = f64::from(b.y);
            (top, top + f64::from(b.h.min(40)))
        }))
    })?;
    if outcome == Glided::Unknown {
        return Err(Error::new(
            "gui_target_unavailable",
            format!("Training queue {name} button not found"),
        ));
    }
    let rect = settled(process, "queue button", |r, i| {
        conveyor_button(r, i, index, name)?
            .filter(|r| r.y >= LIST.0 && r.y + r.h.min(40) <= LIST.1)
            .ok_or_else(|| {
                Error::new(
                    "gui_target_unavailable",
                    "Training queue button not in view",
                )
            })
    })?;
    clear_notices(process, keys)?;
    let (mut x, y) = rect.center();
    // Text buttons (Add Unit) report their wide text sprite; click near the left edge.
    if rect.w > 150 {
        x = f64::from(rect.x) + 40.0;
    }
    keys.click(GAME, x, y)?;
    Ok(())
}

/// Queue `count` divisions of template `name` in ONE training queue: the template's Train
/// button creates the queue (one line, series amount infinite), its + sets the amount to one
/// division per line (an infinite line trains forever), then Add Unit adds the other lines.
/// Each step is verified on the queue's own fields. The caller has journaled intent.
pub(super) fn train(
    process: &dyn Process,
    keys: &dyn GameInput,
    name: &str,
    count: usize,
) -> Result<()> {
    clear_notices(process, keys)?;
    if !read(process, open)? {
        keys.press(Key::letter('u')?)?;
        wait(process, "open deployment", open)?;
        std::thread::sleep(std::time::Duration::from_millis(1100));
    }
    let before: Vec<u64> = read(process, conveyors)?.into_iter().map(|c| c.0).collect();
    let rect = settled(process, "train button", |r, i| train_button(r, i, name))?;
    clear_notices(process, keys)?;
    let (x, y) = rect.center();
    keys.click(GAME, x, y)?;
    wait(process, "training queue added", |r, i| {
        Ok(conveyors(r, i)?
            .iter()
            .any(|c| !before.contains(&c.0) && c.1 == name))
    })?;
    let find_new = |r: &Reader<'_>, i: u64| -> Result<(usize, u64, i32, i32)> {
        conveyors(r, i)?
            .into_iter()
            .enumerate()
            .find(|(_, c)| !before.contains(&c.0) && c.1 == name)
            .map(|(k, c)| (k, c.0, c.2, c.3))
            .ok_or_else(|| Error::new("input_readback_failed", "New training queue left the list"))
    };
    let (index, line, amount, _) = read(process, find_new)?;
    if amount == 0 {
        click_conveyor(process, keys, index, "increase_amount")?;
        wait(process, "series amount one", |r, i| {
            Ok(r.i32(line + ui(r)?.conveyor_amount)? == 1 && find_new(r, i)?.0 == index)
        })
        .map_err(|_| {
            Error::new(
                "input_readback_failed",
                "The new training queue's amount stayed infinite; set it with its + button",
            )
        })?;
    }
    for done in 1..count {
        let (index, _, _, _) = read(process, find_new)?;
        click_conveyor(process, keys, index, "add_button")?;
        wait(process, "training line added", |r, i| {
            Ok(find_new(r, i)?.3 as usize > done)
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
pub(super) fn set_locations(
    process: &dyn Process,
    keys: &dyn GameInput,
    state: i32,
) -> Result<usize> {
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
        let outcome = glide_list(process, keys, |r, i| {
            Ok(location_buttons(r, i)?.first().map(span_y))
        })?;
        if !matches!(outcome, Glided::InBand(_)) {
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
                "No location was set: the game does not let these divisions deploy in that state (shown red on the deployment map, e.g. a colony or non-core state). Deploy in a core state (green), then move them by sea or land",
            )
        })?;
        done += 1;
    }
    Ok(done)
}
