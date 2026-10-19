//! Open event windows and answering them with ordinary clicks. Option
//! entries have no laid-out sprite, so their rectangles come from the live
//! event picture's rectangle and the live local positions of the picture and
//! the options grid (the engine lays the grid out from the window's .gui).
//! Only the newest window (drawn on top) is answered; verified when exactly
//! that event window closes.
use super::ui::{close_menu, read, settled, wait};
use crate::{
    error::{Error, Result},
    game,
    gui_tree::{self, Rect},
    memory::{Reader, StringAbi},
    platform::{
        macos_input::{Capture, MacKeys},
        Process,
    },
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
    /// News windows: options are found on screen below this point within
    /// this column (centre x, half width, top y).
    scan: Option<(f64, f64, f64)>,
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
        let (centres, scan) = option_centres(r, image, window, grid, &kind, options.len())?;
        out.push(Open {
            event,
            id,
            title,
            options,
            centres,
            scan,
        });
    }
    Ok(out)
}
type Centres = (Option<Vec<(f64, f64)>>, Option<(f64, f64, f64)>);
/// Option centres from the live picture rectangle and local layout (the
/// plain window keeps picture and options in bottom_Window). The news window
/// moves its options below a description of variable height, so it yields
/// a screen column to search instead. Unreviewed variants yield neither.
fn option_centres(
    r: &Reader<'_>,
    image: u64,
    window: u64,
    grid: u64,
    kind: &str,
    count: usize,
) -> Result<Centres> {
    let u = ui(r)?;
    if kind != "EventWindow" && kind != "EventWindow_News" {
        return Ok((None, None));
    }
    let Some(picture) = find(r, image, window, "event_picture", 0)? else {
        return Ok((None, None));
    };
    let Rect { x, y, h, .. } = gui_tree::rect(r, image, picture, GAME)?;
    let (px, py) = pair(r, picture + u.icon_position)?;
    let (gx, gy) = pair(r, grid + u.grid_position)?;
    let (sw, sh) = pair(r, grid + u.grid_slot)?;
    if !(1..=800).contains(&sw) || !(1..=200).contains(&sh) || px.abs() > 800 || gx.abs() > 800 {
        return Err(Error::new(
            "invalid_gui_geometry",
            "Bounded event layout required",
        ));
    }
    let left = x - px + gx;
    if kind == "EventWindow_News" {
        let centre = f64::from(left) + f64::from(sw) / 2.0;
        return Ok((
            None,
            Some((centre, f64::from(sw) / 2.0 - 30.0, f64::from(y + h + 20))),
        ));
    }
    let top = y - py + gy;
    Ok((
        Some(
            (0..count)
                .map(|i| {
                    (
                        f64::from(left) + f64::from(sw) / 2.0,
                        f64::from(top) + f64::from(sh) * (i as f64 + 0.5),
                    )
                })
                .collect(),
        ),
        None,
    ))
}
/// Dark option bars (rows mostly darker than the newspaper) in a column of
/// the capture: (top, bottom) of each band 20..60 px tall, top to bottom.
fn dark_bands(capture: &Capture, centre: f64, half: f64, from: f64) -> Vec<(f64, f64)> {
    let dark = |y: f64| {
        let mut n = 0;
        let mut hits = 0;
        let mut x = centre - half;
        while x <= centre + half {
            if let Some((r, g, b)) = capture.rgb(x, y) {
                n += 1;
                if (u32::from(r) + u32::from(g) + u32::from(b)) < 270 {
                    hits += 1;
                }
            }
            x += 10.0;
        }
        n > 0 && hits * 10 >= n * 6
    };
    let mut bands = Vec::new();
    let mut start: Option<f64> = None;
    let mut last = from;
    let mut y = from;
    while y < 1060.0 {
        if dark(y) {
            if start.is_none() {
                start = Some(y);
            }
            last = y;
        } else if let Some(s) = start {
            if y - last > 4.0 {
                if (20.0..=60.0).contains(&(last - s)) {
                    bands.push((s, last));
                }
                start = None;
            }
        }
        y += 2.0;
    }
    bands
}

/// Whether the peace conference window is open (interface manager +0x240
/// holds CPeaceConferenceWindow, whose window is at +0x3da0; macOS 089EA519,
/// found live on 18 October 2026 while closed).
fn peace_conference_open(r: &Reader<'_>, image: u64) -> Result<bool> {
    ui(r)?;
    let idler = r.u64(image + game::layout(r).idler_global)?;
    let view = r.u64(r.u64(idler + 0x670)? + 0x240)?;
    let window = r.u64(view + 0x3da0)?;
    match gui_tree::container(r, base_of(r, image, window)?, image)? {
        Some(object) => gui_tree::visible(r, object),
        None => Ok(false),
    }
}
/// Open events in list order; the last one is drawn on top.
pub(super) fn list(process: &dyn Process) -> Result<Value> {
    let open = read(process, open_events)?;
    let peace = read(process, peace_conference_open).unwrap_or(false);
    let last = open.len().saturating_sub(1);
    Ok(
        json!({"peace_conference_open":peace,"events":open.iter().enumerate().map(|(i, e)| json!({
        "event":e.id,"title":e.title,"options":e.options,"on_top":i == last,
        "answerable":e.centres.is_some() || e.scan.is_some()})).collect::<Vec<_>>()}),
    )
}
/// Ids of the open events, in list order.
pub(super) fn open_ids(r: &Reader<'_>, image: u64) -> Result<Vec<String>> {
    Ok(open_events(r, image)?.into_iter().map(|e| e.id).collect())
}
/// Whether event `id` is open.
pub(super) fn is_open(process: &dyn Process, id: &str) -> Result<bool> {
    Ok(read(process, open_events)?.iter().any(|e| e.id == id))
}

/// Choose option `option` (0-based) of the open event `id`.
pub(super) fn choose(process: &dyn Process, keys: &MacKeys, id: &str, option: usize) -> Result<()> {
    close_menu(process, keys)?;
    // A notice (a finished focus) can be drawn over the event window.
    super::ui::dismiss_notices(process, keys)?;
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
    let centres = match (&top.centres, top.scan) {
        (Some(c), _) => c.clone(),
        (None, Some((centre, half, from))) => {
            // The window fades in; capture again until its bars are drawn.
            let mut bands = Vec::new();
            for _ in 0..6 {
                bands = dark_bands(&keys.capture(GAME)?, centre, half, from);
                if bands.len() >= top.options.len() {
                    break;
                }
                std::thread::sleep(std::time::Duration::from_millis(250));
            }
            if bands.len() < top.options.len() {
                return Err(Error::new(
                    "gui_target_unavailable",
                    "News event options not found on screen",
                ));
            }
            bands.iter().map(|(a, b)| (centre, (a + b) / 2.0)).collect()
        }
        (None, None) => {
            return Err(Error::new(
                "capability_unavailable",
                "This event window layout is not reviewed",
            ))
        }
    };
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

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn news_option_bars_are_found_below_the_text() {
        let (width, height) = (1920usize, 1080usize);
        let mut data = vec![200u8; width * height * 4];
        let mut paint = |y0: usize, y1: usize, x0: usize, x1: usize| {
            for y in y0..y1 {
                for x in x0..x1 {
                    let o = (y * width + x) * 4;
                    data[o..o + 3].copy_from_slice(&[30, 30, 30]);
                }
            }
        };
        paint(612, 614, 790, 820); // a line of text
        paint(738, 777, 760, 1110); // the option bar
        paint(797, 800, 700, 1200); // the window's bottom edge
        let capture = Capture {
            width,
            height,
            stride: width * 4,
            scale: 1.0,
            data,
        };
        let bands = dark_bands(&capture, 908.0, 120.0, 494.0);
        assert_eq!(bands.len(), 1);
        assert!((bands[0].0 - 738.0).abs() <= 2.0 && (bands[0].1 - 776.0).abs() <= 2.0);
    }
}
