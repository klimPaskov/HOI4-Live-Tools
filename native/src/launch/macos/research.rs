//! Start one own technology in one empty research slot with ordinary input:
//! research panel (W), the slot, the technology's folder tab, its tile, then
//! the detail window's Research button. Every click target comes from the live
//! GUI tree of the same fresh read, and every stage waits for the matching
//! memory change. Success is the slot holding the technology in memory.
use super::ui::{clear_notices, dismiss, read, settled, wait};
use crate::{
    error::{Error, Result},
    game,
    gui_tree::{self, Rect},
    memory::{Reader, StringAbi},
    platform::{
        macos_input::{Key, MacKeys},
        Process,
    },
    research,
};
use std::collections::BTreeSet;

/// Research UI joins of one inspected build.
struct Ui {
    manager: u64,
    technology_view: u64,
    tree_view: u64,
    view_root: u64,
    grid_vtable: u64,
    grid_items: u64,
    slot_item_vtable: u64,
    item_container: u64,
    tech_item_vtable: u64,
    tech_item_technology: u64,
    technology_template: u64,
    template_key: u64,
    info_windows: u64,
    info_vtable: u64,
    info_container: u64,
    info_technology: u64,
    info_research: u64,
}
/// macOS Mach-O 089EA519, mapped live on 18 October 2026 (Germany test run):
/// view slots and roots, grid items, slot/tech items, and the info window,
/// whose idler/tree/container/technology joins match the Linux reader's
/// translated offsets.
const MACOS: Ui = Ui {
    manager: 0x670,
    technology_view: 0xf0,
    tree_view: 0xf8,
    view_root: 0x430,
    grid_vtable: 0x34be068,
    grid_items: 0x190,
    slot_item_vtable: 0x34276f8,
    item_container: 0x10,
    tech_item_vtable: 0x3427200,
    tech_item_technology: 0xb8,
    technology_template: 0x160,
    template_key: 0x8,
    info_windows: 0x1418,
    info_vtable: 0x3425f40,
    info_container: 0xfa8,
    info_technology: 0xfb8,
    info_research: 0xfd0,
};
const GAME: (u32, u32) = (1920, 1080);

fn ui(r: &Reader<'_>) -> Result<&'static Ui> {
    match r.string_abi() {
        StringAbi::LibCxx => Ok(&MACOS),
        StringAbi::LibStdCxx => Err(Error::new(
            "capability_unavailable",
            "Research input is not reviewed for this build",
        )),
    }
}

fn root(r: &Reader<'_>, image: u64, view: u64) -> Result<u64> {
    let u = ui(r)?;
    let idler = r.u64(image + game::layout(r).idler_global)?;
    let manager = r.u64(idler + u.manager)?;
    let view = r.u64(manager + view)?;
    gui_tree::container(r, r.u64(view + u.view_root)?, image)?
        .ok_or_else(|| Error::new("invalid_gui", "Research view root unavailable"))
}
fn open(r: &Reader<'_>, image: u64, view: u64) -> Result<bool> {
    gui_tree::visible(r, root(r, image, view)?)
}
fn named(r: &Reader<'_>, image: u64, object: u64, name: &str) -> Result<u64> {
    let child = gui_tree::child(r, image, object, name)?
        .ok_or_else(|| Error::new("gui_target_unavailable", format!("{name} not found")))?;
    gui_tree::container(r, child, image)?
        .ok_or_else(|| Error::new("invalid_gui", format!("{name} is not a window")))
}
fn items(r: &Reader<'_>, grid: u64) -> Result<Vec<u64>> {
    let (_, data, count) = r.array_header(grid + ui(r)?.grid_items)?;
    if count > 4096 {
        return Err(Error::new("invalid_gui", "Grid item list exceeds bound"));
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

/// Click target of an own research slot row.
fn slot_rect(r: &Reader<'_>, image: u64, slot: usize) -> Result<Rect> {
    let u = ui(r)?;
    let panel = root(r, image, u.technology_view)?;
    let slots = named(r, image, panel, "research_slots")?;
    let window = named(r, image, slots, "slots_window")?;
    let grid = gui_tree::child(r, image, window, "research_slots_grid")?
        .ok_or_else(|| Error::new("gui_target_unavailable", "Slot grid not found"))?;
    if r.u64(grid)? != image + u.grid_vtable {
        return Err(Error::new("invalid_gui", "Exact slot grid required"));
    }
    let item = *items(r, grid)?
        .get(slot)
        .ok_or_else(|| Error::new("invalid_arguments", "Slot outside the own slot grid"))?;
    if r.u64(item)? != image + u.slot_item_vtable {
        return Err(Error::new(
            "invalid_gui",
            "Exact research slot item required",
        ));
    }
    let row = gui_tree::container(r, r.u64(item + u.item_container)?, image)?
        .ok_or_else(|| Error::new("invalid_gui", "Slot row window required"))?;
    let glow = gui_tree::child(r, image, row, "empty_research_slot_glow")?
        .ok_or_else(|| Error::new("gui_target_unavailable", "Empty slot row not found"))?;
    gui_tree::rect(r, image, glow, GAME)
}

/// Tile of `key` in the tree: (top-level folder name, folder window, tile icon).
fn tiles(r: &Reader<'_>, image: u64, key: &str) -> Result<Vec<(String, u64, u64)>> {
    let u = ui(r)?;
    let tree = root(r, image, u.tree_view)?;
    let mut found = Vec::new();
    for folder in gui_tree::children(r, tree)? {
        let Some(name) = gui_tree::name(r, folder, image)? else {
            continue;
        };
        if name == "folder_tabs" {
            continue;
        }
        let Some(object) = gui_tree::container(r, folder, image)? else {
            continue;
        };
        let mut stack = vec![(object, 0)];
        while let Some((window, depth)) = stack.pop() {
            for child in gui_tree::children(r, window)? {
                if let Some(inner) = gui_tree::container(r, child, image)? {
                    if depth < 8 {
                        stack.push((inner, depth + 1));
                    }
                    continue;
                }
                if r.u64(child)? != image + u.grid_vtable {
                    continue;
                }
                for item in items(r, child)? {
                    if r.u64(item)? != image + u.tech_item_vtable {
                        continue;
                    }
                    let technology = r.u64(item + u.tech_item_technology)?;
                    let template = r.u64(technology + u.technology_template)?;
                    if r.string_bounded(template + u.template_key, 256)? != key {
                        continue;
                    }
                    let tile = gui_tree::container(r, r.u64(item + u.item_container)?, image)?
                        .ok_or_else(|| Error::new("invalid_gui", "Tile window required"))?;
                    if let Some(icon) = gui_tree::child(r, image, tile, "Icon")? {
                        found.push((name.clone(), object, icon));
                    }
                }
            }
        }
    }
    Ok(found)
}
fn tab_shown(r: &Reader<'_>, image: u64, folder: &str) -> Result<bool> {
    let tree = root(r, image, ui(r)?.tree_view)?;
    let tabs = named(r, image, tree, "folder_tabs")?;
    match gui_tree::child(r, image, tabs, &format!("{folder}_tab"))? {
        Some(tab) => gui_tree::shown(r, tab),
        None => Ok(false),
    }
}
fn tab_rect(r: &Reader<'_>, image: u64, folder: &str) -> Result<Rect> {
    let tree = root(r, image, ui(r)?.tree_view)?;
    let tabs = named(r, image, tree, "folder_tabs")?;
    let tab = gui_tree::child(r, image, tabs, &format!("{folder}_tab"))?
        .ok_or_else(|| Error::new("gui_target_unavailable", "Folder tab not found"))?;
    gui_tree::rect(r, image, tab, GAME)
}
/// Visible info window for `key` and its Research button rectangle.
fn info(r: &Reader<'_>, image: u64, key: &str) -> Result<Option<(Rect, bool)>> {
    let u = ui(r)?;
    let idler = r.u64(image + game::layout(r).idler_global)?;
    let view = r.u64(r.u64(idler + u.manager)? + u.tree_view)?;
    let (_, data, count) = r.array_header(view + u.info_windows)?;
    for index in 0..count.min(8) {
        let window = r.u64(data + index as u64 * 8)?;
        if r.u64(window)? != image + u.info_vtable {
            return Err(Error::new(
                "invalid_gui",
                "Exact technology info window required",
            ));
        }
        let Some(frame) = gui_tree::container(r, r.u64(window + u.info_container)?, image)? else {
            continue;
        };
        let technology = r.u64(window + u.info_technology)?;
        if technology == 0 || !gui_tree::visible(r, frame)? {
            continue;
        }
        let template = r.u64(technology + u.technology_template)?;
        if r.string_bounded(template + u.template_key, 256)? == key {
            let button = r.u64(window + u.info_research)?;
            return Ok(Some((
                gui_tree::rect(r, image, button, GAME)?,
                gui_tree::disabled(r, button)?,
            )));
        }
    }
    Ok(None)
}

pub(super) struct Assignments {
    pub slots: Vec<Option<String>>,
    pub completed: BTreeSet<String>,
}
/// How an unresolved `research_start` settles from two own-slot snapshots.
#[derive(Debug, PartialEq)]
pub(super) enum Settled {
    /// The target slot holds the technology.
    Reached,
    /// The only effect this action can have (putting `key` into some slot) is
    /// absent: the target slot is empty and no other slot newly holds `key`.
    /// Other slots may have changed on their own (research finishing at speed).
    Unchanged,
    Unknown,
}
pub(super) fn settle(
    before: &[Option<String>],
    now: &[Option<String>],
    slot: usize,
    key: &str,
) -> Settled {
    let target = now.get(slot).cloned().flatten();
    if target.as_deref() == Some(key) {
        return Settled::Reached;
    }
    let newly_elsewhere = now.iter().enumerate().any(|(i, s)| {
        i != slot
            && s.as_deref() == Some(key)
            && before.get(i).cloned().flatten().as_deref() != Some(key)
    });
    if target.is_none() && before.get(slot).cloned().flatten().is_none() && !newly_elsewhere {
        Settled::Unchanged
    } else {
        Settled::Unknown
    }
}
pub(super) fn assignments(process: &dyn Process) -> Result<Assignments> {
    read(process, |r, base| {
        let (slots, completed) = research::assignments(r, base)?;
        Ok(Assignments { slots, completed })
    })
}

/// Refusals that need no input: unknown/finished/assigned technology, busy slot.
pub(super) fn check(before: &Assignments, key: &str, slot: usize) -> Result<()> {
    if before.completed.contains(key) {
        return Err(Error::new(
            "already_researched",
            format!(
                "Technology {key} is already researched (own completed set); nothing was clicked. \
                 Choose a technology that is not researched yet."
            ),
        ));
    }
    if before.slots.iter().any(|s| s.as_deref() == Some(key)) {
        return Err(Error::new(
            "invalid_target",
            "Technology already being researched",
        ));
    }
    match before.slots.get(slot) {
        None => Err(Error::new("invalid_arguments", "Slot outside own slots")),
        Some(Some(_)) => Err(Error::new("invalid_target", "Research slot is busy")),
        Some(None) => Ok(()),
    }
}

/// Ordinary input sequence. The caller has journaled intent and holds the lease.
pub(super) fn start(process: &dyn Process, keys: &MacKeys, key: &str, slot: usize) -> Result<()> {
    let image = process.image_base();
    let u = &MACOS;
    let click = |rect: Rect| {
        clear_notices(process, keys)?;
        let (x, y) = rect.center();
        keys.click(GAME, x, y).map(|_| ())
    };
    clear_notices(process, keys)?;
    // Escape closes an open detail window before the tree itself.
    for attempt in 0..3 {
        if !read(process, |r, i| open(r, i, ui(r)?.tree_view))? {
            break;
        }
        keys.press(Key::Escape)?;
        let closed = wait(process, "close tree", |r, i| Ok(!open(r, i, u.tree_view)?));
        if closed.is_err() && attempt == 2 {
            return closed;
        }
    }
    if !read(process, |r, i| open(r, i, ui(r)?.technology_view))? {
        keys.press(Key::letter('w')?)?;
        wait(process, "open research panel", |r, i| {
            open(r, i, u.technology_view)
        })?;
    }
    click(settled(process, "slot", |r, _| slot_rect(r, image, slot))?)?;
    wait(process, "open tree", |r, i| open(r, i, u.tree_view))?;
    let matches = read(process, |r, i| tiles(r, i, key))?;
    if matches.is_empty() {
        return Err(Error::new(
            "gui_target_unavailable",
            "Technology has no tile in the own tree",
        ));
    }
    let shown = read(process, |r, _| {
        for (_, folder, icon) in &matches {
            if gui_tree::visible(r, *folder)? {
                return Ok(Some(*icon));
            }
        }
        Ok(None)
    })?;
    let icon = match shown {
        Some(icon) => icon,
        None => {
            // Only a shown tab belongs to the active variant (DLC variants hide theirs).
            let (name, folder, icon) = matches
                .iter()
                .find(|(name, _, _)| read(process, |r, i| tab_shown(r, i, name)).unwrap_or(false))
                .cloned()
                .ok_or_else(|| {
                    Error::new(
                        "invalid_target",
                        "The technology's folder is not active in this game (inactive DLC variant)",
                    )
                })?;
            click(settled(process, "folder tab", |r, i| {
                tab_rect(r, i, &name)
            })?)?;
            wait(process, "open folder", |r, _| gui_tree::visible(r, folder))?;
            icon
        }
    };
    // Later technologies sit below the visible part of a folder; scroll the
    // tree (only while it is open) until the tile is inside the tree area.
    let in_view = |t: &Rect| t.y >= 140 && t.y + t.h <= 1040;
    for _ in 0..30 {
        let tile = read(process, |r, i| gui_tree::rect_unclipped(r, i, icon, GAME))?;
        if in_view(&tile) {
            break;
        }
        if !read(process, |r, i| open(r, i, u.tree_view))? {
            return Err(Error::new(
                "input_readback_failed",
                "Technology tree closed",
            ));
        }
        let pixels = if tile.y < 140 { 240 } else { -240 };
        keys.scroll(GAME, (1700.0, 600.0), pixels)?;
    }
    let tile = settled(process, "tile", |r, i| {
        gui_tree::rect_unclipped(r, i, icon, GAME)
    })?;
    if !in_view(&tile) {
        return Err(Error::new(
            "gui_target_unavailable",
            "Technology tile could not be scrolled into the tree area",
        ));
    }
    click(tile)?;
    wait(process, "open technology detail", |r, i| {
        Ok(info(r, i, key)?.is_some())
    })?;
    let (button, disabled) = settled(process, "research button", |r, i| {
        info(r, i, key)?
            .ok_or_else(|| Error::new("input_readback_failed", "Technology detail closed"))
    })?;
    if disabled {
        dismiss(process, keys, |r, i| Ok(!open(r, i, u.tree_view)?));
        return Err(Error::new(
            "invalid_target",
            "The game disables Research for this technology (missing prerequisite or exclusive choice)",
        ));
    }
    click(button)?;
    wait(process, "start research", |r, base| {
        let (slots, _) = research::assignments(r, base)?;
        Ok(slots.get(slot).and_then(|s| s.as_deref()) == Some(key))
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    fn own(slots: &[Option<&str>]) -> Vec<Option<String>> {
        slots.iter().map(|s| s.map(String::from)).collect()
    }
    #[test]
    fn unresolved_start_settles_unchanged_even_when_other_slots_finish() {
        let before = own(&[Some("a"), Some("b"), None, None]);
        let now = own(&[None, Some("b2"), None, None]);
        assert_eq!(settle(&before, &now, 3, "oil"), Settled::Unchanged);
        assert_eq!(
            settle(
                &before,
                &own(&[Some("a"), Some("b"), None, Some("oil")]),
                3,
                "oil"
            ),
            Settled::Reached
        );
        // started in the wrong slot: cannot prove nothing happened
        assert_eq!(
            settle(
                &before,
                &own(&[Some("a"), Some("b"), Some("oil"), None]),
                3,
                "oil"
            ),
            Settled::Unknown
        );
        // target slot taken by something else
        assert_eq!(
            settle(
                &before,
                &own(&[Some("a"), Some("b"), None, Some("x")]),
                3,
                "oil"
            ),
            Settled::Unknown
        );
    }
    fn state(slots: &[Option<&str>], done: &[&str]) -> Assignments {
        Assignments {
            slots: slots.iter().map(|s| s.map(String::from)).collect(),
            completed: done.iter().map(|s| s.to_string()).collect(),
        }
    }
    #[test]
    fn refusals_need_no_input() {
        let s = state(&[None, Some("radio")], &["infantry_weapons"]);
        assert!(check(&s, "support_weapons", 0).is_ok());
        let done = check(&s, "infantry_weapons", 0).unwrap_err();
        assert_eq!(done.code, "already_researched");
        assert!(done.message.contains("infantry_weapons"));
        assert_eq!(check(&s, "radio", 0).unwrap_err().code, "invalid_target");
        assert_eq!(
            check(&s, "support_weapons", 1).unwrap_err().code,
            "invalid_target"
        );
        assert_eq!(
            check(&s, "support_weapons", 2).unwrap_err().code,
            "invalid_arguments"
        );
    }
}
