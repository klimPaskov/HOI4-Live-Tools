//! Train own divisions with ordinary input: deployment panel (U) and the
//! template's Train button, once per division. Each click must add one
//! training line for that template to the own deployment queue in memory.
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
};
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
    if rows.len() != names.len() {
        return Err(Error::new(
            "gui_target_unavailable",
            "Template rows differ from the own template list (obsolete templates shown or hidden)",
        ));
    }
    let index = names
        .iter()
        .position(|n| n == name)
        .ok_or_else(|| Error::new("invalid_target", "No own division template with this name"))?;
    let frame = gui_tree::container(r, r.u64(rows[index] + u.row_container)?, image)?
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
