//! Open event windows and answering them with ordinary clicks. Option
//! entries have no laid-out sprite, so their rectangles come from the live
//! event picture's rectangle and the live local positions of the picture and
//! the options grid (the engine lays the grid out from the window's .gui).
//! Only the newest window (drawn on top) is answered; verified when exactly
//! that event window closes.
use super::ui::{clear_notices, read, settled, wait};
use crate::{
    error::{Error, Result},
    game,
    gui_tree::{self, Rect},
    memory::{Reader, StringAbi},
    platform::{macos_input::MacKeys, Process},
};
use serde_json::{json, Value};

struct Ui {
    gui: u64,
    popups: u64,
    /// Popup-list entries of event windows point at this secondary vtable.
    event_window_vtable: u64,
    root: u64,
    event: u64,
    event_id: u64,
    container_vtable: u64,
    container_base: u64,
    grid_items: u64,
    item_container: u64,
    grid_position: u64,
    grid_slot: u64,
    icon_position: u64,
    text: u64,
}
/// macOS Mach-O 089EA519, mapped live on 18 October 2026 from the event
/// germany_mefo_bills.3: the options grid's local position (215, -1) and slot
/// (300, 47) and the picture's local position (5, 5) match eventwindow.gui,
/// and the computed option centre lies on the drawn option.
const MACOS: Ui = Ui {
    gui: 0x4c0,
    popups: 0x9e0,
    event_window_vtable: 0x33db0d8,
    root: 0x28,
    event: 0x40,
    event_id: 0x18,
    container_vtable: 0x34ac0b0,
    container_base: 0x30,
    grid_items: 0x190,
    item_container: 0x10,
    grid_position: 0x70,
    grid_slot: 0xd4,
    icon_position: 0xf4,
    text: 0x100,
};
const GAME: (u32, u32) = (1920, 1080);

fn ui(r: &Reader<'_>) -> Result<&'static Ui> {
    match r.string_abi() {
        StringAbi::LibCxx => Ok(&MACOS),
        StringAbi::LibStdCxx => Err(Error::new(
            "capability_unavailable",
            "Event input is not reviewed for this build",
        )),
    }
}
/// GUI-object base of a container given its object pointer.
fn base_of(r: &Reader<'_>, image: u64, object: u64) -> Result<u64> {
    let u = ui(r)?;
    if r.u64(object)? != image + u.container_vtable {
        return Err(Error::new("invalid_gui", "Event container required"));
    }
    Ok(object + u.container_base)
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
fn pair(r: &Reader<'_>, address: u64) -> Result<(i32, i32)> {
    Ok((r.i32(address)?, r.i32(address + 4)?))
}

#[derive(Clone, Debug, PartialEq)]
struct Open {
    event: u64,
    id: String,
    title: String,
    options: Vec<String>,
    centres: Option<Vec<(f64, f64)>>,
}

fn open_events(r: &Reader<'_>, image: u64) -> Result<Vec<Open>> {
    let u = ui(r)?;
    let idler = r.u64(image + game::layout(r).idler_global)?;
    let gui = r.u64(idler + u.gui)?;
    let (_, data, count) = r.array_header(gui + u.popups)?;
    let mut out = Vec::new();
    for index in 0..count.min(32) as u64 {
        let entry = r.u64(data + index * 8)?;
        if r.u64(entry)? != image + u.event_window_vtable {
            continue;
        }
        let root = r.u64(entry + u.root)?;
        let Some(window) = gui_tree::container(r, base_of(r, image, root)?, image)? else {
            continue;
        };
        if !gui_tree::visible(r, window)? {
            continue;
        }
        let event = r.u64(entry + u.event)?;
        let id = r.string_bounded(event + u.event_id, 128)?;
        let kind = gui_tree::name(r, base_of(r, image, root)?, image)?.unwrap_or_default();
        let title = match find(r, image, window, "Title", 0)? {
            Some(t) => r.string_bounded(t + u.text, 256).unwrap_or_default(),
            None => String::new(),
        };
        let grid = find(r, image, window, "options_grid", 0)?
            .ok_or_else(|| Error::new("gui_target_unavailable", "Event options not found"))?;
        let (_, items, n) = r.array_header(grid + u.grid_items)?;
        let mut options = Vec::new();
        for i in 0..n.min(8) as u64 {
            let item = r.u64(items + i * 8)?;
            let entry = r.u64(item + u.item_container)?;
            let frame = gui_tree::container(r, base_of(r, image, entry)?, image)?
                .ok_or_else(|| Error::new("invalid_gui", "Event option entry required"))?;
            let text = match gui_tree::child(r, image, frame, "Name")? {
                Some(t) => r.string_bounded(t + u.text, 256).unwrap_or_default(),
                None => String::new(),
            };
            options.push(text);
        }
        let centres = option_centres(r, image, window, grid, &kind, options.len())?;
        out.push(Open {
            event,
            id,
            title,
            options,
            centres,
        });
    }
    Ok(out)
}
/// Option centres from the live picture rectangle and local layout; `None`
/// for a window variant whose layout is not reviewed.
fn option_centres(
    r: &Reader<'_>,
    image: u64,
    window: u64,
    grid: u64,
    kind: &str,
    count: usize,
) -> Result<Option<Vec<(f64, f64)>>> {
    let u = ui(r)?;
    // Offset of the grid's parent from the picture's parent: the plain event
    // window keeps both in bottom_Window; the news window puts the picture
    // in top_Window (0, 0) and the grid in bottom_Window (0, 221).
    let parents = match kind {
        "EventWindow" => (0, 0),
        "EventWindow_News" => (0, 221),
        _ => return Ok(None),
    };
    let Some(picture) = find(r, image, window, "event_picture", 0)? else {
        return Ok(None);
    };
    let Rect { x, y, .. } = gui_tree::rect(r, image, picture, GAME)?;
    let (px, py) = pair(r, picture + u.icon_position)?;
    let (gx, gy) = pair(r, grid + u.grid_position)?;
    let (sw, sh) = pair(r, grid + u.grid_slot)?;
    if !(1..=800).contains(&sw) || !(1..=200).contains(&sh) || px.abs() > 800 || gx.abs() > 800 {
        return Err(Error::new(
            "invalid_gui_geometry",
            "Bounded event layout required",
        ));
    }
    let left = x - px + parents.0 + gx;
    let top = y - py + parents.1 + gy;
    Ok(Some(
        (0..count)
            .map(|i| {
                (
                    f64::from(left) + f64::from(sw) / 2.0,
                    f64::from(top) + f64::from(sh) * (i as f64 + 0.5),
                )
            })
            .collect(),
    ))
}

/// Open events in list order; the last one is drawn on top.
pub(super) fn list(process: &dyn Process) -> Result<Value> {
    let open = read(process, open_events)?;
    let last = open.len().saturating_sub(1);
    Ok(json!({"events":open.iter().enumerate().map(|(i, e)| json!({
        "event":e.id,"title":e.title,"options":e.options,"on_top":i == last,
        "answerable":e.centres.is_some()})).collect::<Vec<_>>()}))
}
/// Whether event `id` is open.
pub(super) fn is_open(process: &dyn Process, id: &str) -> Result<bool> {
    Ok(read(process, open_events)?.iter().any(|e| e.id == id))
}

/// Choose option `option` (0-based) of the open event `id`.
pub(super) fn choose(process: &dyn Process, keys: &MacKeys, id: &str, option: usize) -> Result<()> {
    clear_notices(process, keys)?;
    let open = settled(process, "event window", open_events)?;
    let Some(top) = open.last() else {
        return Err(Error::new("invalid_target", "No event window is open"));
    };
    if top.id != id {
        return Err(Error::new(
            "invalid_target",
            format!("Answer the event on top first: {}", top.id),
        ));
    }
    let centres = top.centres.clone().ok_or_else(|| {
        Error::new(
            "capability_unavailable",
            "This event window layout is not reviewed",
        )
    })?;
    let &(x, y) = centres
        .get(option)
        .ok_or_else(|| Error::new("invalid_arguments", "No such event option"))?;
    let event = top.event;
    let others: Vec<u64> = open[..open.len() - 1].iter().map(|e| e.event).collect();
    keys.click(GAME, x, y)?;
    wait(process, "event answered", |r, i| {
        let now = open_events(r, i)?;
        if others.iter().any(|o| !now.iter().any(|e| e.event == *o)) {
            return Err(Error::new(
                "input_readback_failed",
                "A different event window closed",
            ));
        }
        Ok(!now.iter().any(|e| e.event == event))
    })
}
