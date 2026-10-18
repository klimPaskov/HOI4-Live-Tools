//! Start one own national focus with ordinary input: politics panel (Q), its
//! national-focus button, panning the tree until the focus is in view, the
//! focus symbol, then the detail window's Start button. Targets come from the
//! live GUI tree (zoom-aware), each stage waits for its memory change, and
//! success is the own focus progress naming the focus as active.
use super::ui::{clear_notices, dismiss, read, settled, wait};
use crate::{
    error::{Error, Result},
    game,
    gui_tree::{self, Rect},
    memory::{Reader, StringAbi},
    own_focus,
    platform::{
        macos_input::{Key, MacKeys},
        Process,
    },
};
use serde_json::Value;
use std::{
    collections::BTreeSet,
    thread,
    time::{Duration, Instant},
};

/// Focus UI joins of one inspected build.
struct Ui {
    manager: u64,
    politics_view: u64,
    focus_view: u64,
    view_root: u64,
    displayed_country: u64,
    items: u64,
    item_vtable: u64,
    item_focus: u64,
    item_container: u64,
    focus_key: u64,
    zoom: u64,
    panels: u64,
    panel_vtable: u64,
    panel_window: u64,
    panel_selected: u64,
}
/// macOS Mach-O 089EA519, mapped live on 18 October 2026 (Germany test run).
/// Panel joins (+0xb90 idler, +0xba0 window, +0xba8 selected) and the view's
/// panel list (+0x1938) and displayed country (+0x1990) are the Linux
/// reader's offsets translated by aligned methods and confirmed live; the zoom
/// float matched a measured node-spacing ratio, and drag panning is 1:1.
const MACOS: Ui = Ui {
    manager: 0x670,
    politics_view: 0x128,
    focus_view: 0x130,
    view_root: 0x430,
    displayed_country: 0x1990,
    items: 0x18f0,
    item_vtable: 0x3405b78,
    item_focus: 0x20,
    item_container: 0x10,
    focus_key: 0x18,
    zoom: 0x19b0,
    panels: 0x1938,
    panel_vtable: 0x3405158,
    panel_window: 0xba0,
    panel_selected: 0xba8,
};
const GAME: (u32, u32) = (1920, 1080);
const START_WAIT: Duration = Duration::from_secs(3);
/// Tree area clear of the title bar, filter buttons and the bottom scrollbar.
const SAFE: (f64, f64, f64, f64) = (260.0, 200.0, 1860.0, 1000.0);

fn ui(r: &Reader<'_>) -> Result<&'static Ui> {
    match r.string_abi() {
        StringAbi::LibCxx => Ok(&MACOS),
        StringAbi::LibStdCxx => Err(Error::new(
            "capability_unavailable",
            "Focus input is not reviewed for this build",
        )),
    }
}
fn view(r: &Reader<'_>, image: u64, slot: u64) -> Result<u64> {
    let idler = r.u64(image + game::layout(r).idler_global)?;
    r.u64(r.u64(idler + ui(r)?.manager)? + slot)
}
fn root(r: &Reader<'_>, image: u64, slot: u64) -> Result<u64> {
    gui_tree::container(r, r.u64(view(r, image, slot)? + ui(r)?.view_root)?, image)?
        .ok_or_else(|| Error::new("invalid_gui", "View root unavailable"))
}
fn open(r: &Reader<'_>, image: u64, slot: u64) -> Result<bool> {
    gui_tree::visible(r, root(r, image, slot)?)
}
fn focus_button(r: &Reader<'_>, image: u64) -> Result<Rect> {
    let politics = root(r, image, ui(r)?.politics_view)?;
    let goal = gui_tree::child(r, image, politics, "active_goal")?
        .and_then(|c| gui_tree::container(r, c, image).transpose())
        .transpose()?
        .ok_or_else(|| Error::new("gui_target_unavailable", "Politics focus area not found"))?;
    let button = gui_tree::child(r, image, goal, "add_national_goal_button")?
        .ok_or_else(|| Error::new("gui_target_unavailable", "Focus button not found"))?;
    gui_tree::rect(r, image, button, GAME)
}
/// The focus view's symbol rectangle for `key` (unclipped, zoom-aware), and
/// every symbol/background rectangle (to find empty background for panning).
fn symbols(r: &Reader<'_>, image: u64, key: &str) -> Result<(Option<Rect>, Vec<Rect>)> {
    let u = ui(r)?;
    let focus = view(r, image, u.focus_view)?;
    let zoom = f32::from_le_bytes(r.read(focus + u.zoom, 4)?.try_into().unwrap());
    let (_, data, count) = r.array_header(focus + u.items)?;
    if count > 4096 {
        return Err(Error::new("invalid_gui", "Focus item list exceeds bound"));
    }
    let mut target = None;
    let mut all = Vec::new();
    for index in 0..count as u64 {
        let item = r.u64(data + index * 8)?;
        if r.u64(item)? != image + u.item_vtable {
            return Err(Error::new("invalid_gui", "Exact focus tree item required"));
        }
        let Some(node) = gui_tree::container(r, r.u64(item + u.item_container)?, image)? else {
            continue;
        };
        let mut symbol = None;
        for name in ["symbol", "bg"] {
            if let Some(widget) = gui_tree::child(r, image, node, name)? {
                if let Ok(rect) = gui_tree::rect_zoomed(r, image, widget, GAME, zoom) {
                    all.push(rect);
                    if name == "symbol" {
                        symbol = Some(rect);
                    }
                }
            }
        }
        let focus_object = r.u64(item + u.item_focus)?;
        if focus_object != 0 && r.string_bounded(focus_object + u.focus_key, 256)? == key {
            target = symbol;
        }
    }
    Ok((target, all))
}
fn inside(rect: &Rect) -> bool {
    let (x, y) = rect.center();
    (SAFE.0..SAFE.2).contains(&x) && (SAFE.1..SAFE.3).contains(&y)
}
/// A tree-area point at least 24 px from every focus widget.
fn background(all: &[Rect]) -> Option<(f64, f64)> {
    let clear = |x: f64, y: f64| {
        all.iter().all(|r| {
            x < f64::from(r.x) - 24.0
                || x > f64::from(r.x + r.w) + 24.0
                || y < f64::from(r.y) - 24.0
                || y > f64::from(r.y + r.h) + 24.0
        })
    };
    let mut y = 600.0;
    while y < 960.0 {
        let mut x = 960.0;
        while x < 1700.0 {
            for px in [x, 1920.0 - x] {
                if clear(px, y) {
                    return Some((px, y));
                }
            }
            x += 37.0;
        }
        y += 29.0;
    }
    None
}
/// Visible detail window for `key` and its Start button rectangle.
fn detail(r: &Reader<'_>, image: u64, key: &str) -> Result<Option<(Rect, bool)>> {
    let u = ui(r)?;
    let focus = view(r, image, u.focus_view)?;
    let (_, data, count) = r.array_header(focus + u.panels)?;
    for index in 0..count.min(8) as u64 {
        let panel = r.u64(data + index * 8)?;
        if r.u64(panel)? != image + u.panel_vtable {
            return Err(Error::new(
                "invalid_gui",
                "Exact focus detail panel required",
            ));
        }
        let selected = r.u64(panel + u.panel_selected)?;
        if selected == 0 || r.string_bounded(selected + u.focus_key, 256)? != key {
            continue;
        }
        let Some(window) = gui_tree::container(r, r.u64(panel + u.panel_window)?, image)? else {
            continue;
        };
        if !gui_tree::visible(r, window)? {
            continue;
        }
        if let Some(start) = gui_tree::child(r, image, window, "start")? {
            return Ok(Some((
                gui_tree::rect(r, image, start, GAME)?,
                gui_tree::disabled(r, start)?,
            )));
        }
    }
    Ok(None)
}

pub(super) struct Progress {
    pub active: Option<String>,
    pub completed: BTreeSet<String>,
}
pub(super) fn progress(process: &dyn Process) -> Result<Progress> {
    let mut completed = BTreeSet::new();
    let mut offset = 0;
    loop {
        let page: Value = own_focus::completion_snapshot(process, offset, 100)?;
        for key in page["completed"].as_array().into_iter().flatten() {
            if let Some(key) = key.as_str() {
                completed.insert(key.to_string());
            }
        }
        match page["next_offset"].as_u64() {
            Some(next) if next as usize > offset => offset = next as usize,
            _ => {
                return Ok(Progress {
                    active: page["active_key"].as_str().map(String::from),
                    completed,
                })
            }
        }
    }
}
/// Refusals that need no input.
pub(super) fn check(before: &Progress, key: &str) -> Result<()> {
    if before.completed.contains(key) {
        return Err(Error::new("invalid_target", "Focus already completed"));
    }
    match &before.active {
        Some(active) if active == key => Err(Error::new("invalid_target", "Focus already active")),
        Some(_) => Err(Error::new(
            "invalid_target",
            "Another focus is active; it must finish or be cancelled first",
        )),
        None => Ok(()),
    }
}

/// Ordinary input sequence. The caller has journaled intent and holds the lease.
pub(super) fn start(process: &dyn Process, keys: &MacKeys, key: &str) -> Result<()> {
    let u = &MACOS;
    let click = |rect: Rect| {
        clear_notices(process, keys)?;
        let (x, y) = rect.center();
        keys.click(GAME, x, y).map(|_| ())
    };
    clear_notices(process, keys)?;
    if !read(process, |r, i| open(r, i, u.focus_view))? {
        if !read(process, |r, i| open(r, i, u.politics_view))? {
            keys.press(Key::letter('q')?)?;
            wait(process, "open politics", |r, i| open(r, i, u.politics_view))?;
        }
        click(settled(process, "focus button", focus_button)?)?;
        wait(process, "open focus tree", |r, i| open(r, i, u.focus_view))?;
    }
    // The tree must show the own country before any node is trusted.
    let own = read(process, |r, i| {
        let focus = view(r, i, u.focus_view)?;
        let shown = r.u64(focus + u.displayed_country)?;
        let g = game::layout(r);
        let gs = r.u64(i + g.gs_global)?;
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
        Ok(shown == 0 || shown == country)
    })?;
    if !own {
        return Err(Error::new(
            "foreign_focus_scope",
            "Focus view shows another country",
        ));
    }
    // The tree's node widgets are laid out shortly after the view opens.
    wait(process, "focus tree laid out", |r, i| {
        Ok(symbols(r, i, key)?.0.is_some())
    })
    .map_err(|_| {
        Error::new(
            "gui_target_unavailable",
            "Focus has no node in the own tree",
        )
    })?;
    let mut symbol = None;
    for _ in 0..12 {
        let (target, all) = read(process, |r, i| symbols(r, i, key))?;
        let rect = target.ok_or_else(|| {
            Error::new(
                "gui_target_unavailable",
                "Focus has no node in the own tree",
            )
        })?;
        if inside(&rect) {
            symbol = Some(rect);
            break;
        }
        let (cx, cy) = rect.center();
        let from = background(&all)
            .ok_or_else(|| Error::new("gui_target_unavailable", "No empty tree area to pan"))?;
        // Move the node towards the tree centre, at most 700 px per drag.
        let dx = (960.0 - cx).clamp(-700.0, 700.0);
        let dy = (600.0 - cy).clamp(-350.0, 350.0);
        let to = (
            (from.0 + dx).clamp(SAFE.0, SAFE.2),
            (from.1 + dy).clamp(SAFE.1, SAFE.3),
        );
        keys.drag(GAME, from, to)?;
        thread::sleep(Duration::from_millis(150));
    }
    let symbol = symbol.ok_or_else(|| {
        Error::new(
            "gui_target_unavailable",
            "Focus could not be panned into view",
        )
    })?;
    let _ = symbol;
    // Panning can coast; click only a node that has stopped inside the tree area.
    let symbol = settled(process, "focus node", |r, i| {
        symbols(r, i, key)?
            .0
            .ok_or_else(|| Error::new("gui_target_unavailable", "Focus node disappeared"))
    })?;
    if !inside(&symbol) {
        return Err(Error::new(
            "gui_target_unavailable",
            "Focus node drifted out of the tree area",
        ));
    }
    click(symbol)?;
    wait(process, "open focus detail", |r, i| {
        Ok(detail(r, i, key)?.is_some())
    })?;
    let (button, disabled) = settled(process, "start button", |r, i| {
        detail(r, i, key)?.ok_or_else(|| Error::new("input_readback_failed", "Focus detail closed"))
    })?;
    if disabled {
        dismiss(process, keys, |r, i| Ok(detail(r, i, key)?.is_none()));
        return Err(Error::new(
            "invalid_target",
            "The game disables Start for this focus (requirements unmet or exclusive)",
        ));
    }
    click(button)?;
    let deadline = Instant::now() + START_WAIT;
    loop {
        if progress(process)?.active.as_deref() == Some(key) {
            return Ok(());
        }
        if Instant::now() >= deadline {
            return Err(Error::new(
                "input_readback_failed",
                "start focus: the focus did not become active",
            ));
        }
        thread::sleep(Duration::from_millis(30));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn refusals_need_no_input() {
        let p = |active: Option<&str>| Progress {
            active: active.map(String::from),
            completed: ["GER_a".to_string()].into(),
        };
        assert!(check(&p(None), "GER_b").is_ok());
        assert_eq!(check(&p(None), "GER_a").unwrap_err().code, "invalid_target");
        assert_eq!(
            check(&p(Some("GER_b")), "GER_b").unwrap_err().code,
            "invalid_target"
        );
        assert_eq!(
            check(&p(Some("GER_c")), "GER_b").unwrap_err().code,
            "invalid_target"
        );
    }
    #[test]
    fn background_avoids_every_widget() {
        let busy = vec![Rect {
            x: 900,
            y: 560,
            w: 200,
            h: 100,
        }];
        let (x, y) = background(&busy).unwrap();
        assert!(!(876.0..=1124.0).contains(&x) || !(536.0..=684.0).contains(&y));
        assert!(inside(&Rect {
            x: 950,
            y: 590,
            w: 20,
            h: 20
        }));
        assert!(!inside(&Rect {
            x: 4000,
            y: 590,
            w: 20,
            h: 20
        }));
    }
}
