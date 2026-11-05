//! Laws, advisors and designers with ordinary input: politics panel (Q), the
//! category slot, then the idea in the selection window. Success is the slot
//! holding the idea in memory. A listing mode opens the slot and returns its
//! options (with the game's enabled state) without choosing anything.
use super::ui::{accept_command, clear_notices, decline_stale, dismiss, read, settled, wait};
use crate::{
    error::{Error, Result},
    game,
    gui_tree::{self, Rect},
    memory::Reader,
    platform::{game_input::GameInput, Process},
    xlayout::{fld, img, Class},
};
use serde_json::{json, Value};

struct Ui {
    manager: u64,
    view: u64,
    view_root: u64,
    grid_items: u64,
    grid_vtable: u64,
    category_vtable: u64,
    category_container: u64,
    category_kind: u64,
    kind_key: u64,
    slot_vtable: u64,
    slot_idea: u64,
    slot_button: u64,
    idea_key: u64,
    window: u64,
    window_root: u64,
    entry_vtable: u64,
    entry_container: u64,
    entry_idea: u64,
    advisor_slot_vtable: u64,
    advisor_button: u64,
    advisor_character: u64,
    entry_advisor_button: u64,
    entry_advisor_character: u64,
    character_key: u64,
    gui_from_idler: u64,
    root_windows: u64,
    character_entry_vtable: u64,
    container_base: u64,
}
/// macOS Mach-O 089EA519, mapped live on 18 October 2026 (Germany test run):
/// category entries and their slots (group, current idea, button), the
/// selection window embedded in the politics view (+0x5d50), and its entries
/// (idea +0x30) whose idea_entry_bg buttons the game disables (state 3).
const MACOS: Ui = Ui {
    manager: 0x670,
    view: 0x128,
    view_root: 0x490,
    grid_items: 0x190,
    grid_vtable: 0x36cca80,
    category_vtable: 0x367f210,
    category_container: 0x10,
    category_kind: 0x30,
    kind_key: 0x10,
    // Slots are stored as their secondary base (primary + 8).
    slot_vtable: 0x367efa0,
    slot_idea: 0x20,
    slot_button: 0x40,
    idea_key: 0x18,
    window: 0x6630,
    window_root: 0x8,
    entry_vtable: 0x367f050,
    entry_container: 0x10,
    entry_idea: 0x30,
    advisor_slot_vtable: 0x36a4d00,
    advisor_button: 0x28,
    advisor_character: 0x490,
    entry_advisor_button: 0x28,
    entry_advisor_character: 0x490,
    character_key: 0x38,
    gui_from_idler: 0x4c0,
    root_windows: 0x140,
    character_entry_vtable: 0x36a40e8,
    container_base: 0x30,
};
/// Owners of the fields above (macOS 1.19.4.1 RTTI names and vtables). The
/// interface manager, the selection window object and the idea-group record
/// have no reviewed class; their Linux offsets need an anchor function.
const IDLER: Class = Class::new("CInGameIdler", 0x35089d8);
const MANAGER: Class = Class::new("idler.manager", 0);
const GAME_GUI: Class = Class::new("idler.gui", 0);
const VIEW: Class = Class::new("CCountryPoliticsView", 0x36511f0);
const SELECTION: Class = Class::new("politics.selection_window", 0);
const GRID: Class = Class::new("CStandardGridBox", 0x36cca80);
const CATEGORY: Class = Class::new("CPoliticsIdeaCategoryEntry", 0x367f210);
const KIND: Class = Class::new("idea.category", 0);
const SLOT: Class = Class::new("CPoliticalIdeaItem", 0x367efa0);
const IDEA: Class = Class::new("CIdea", 0x35882b8);
const ENTRY: Class = Class::new("CPoliticalSelectableIdeaItem", 0x367f050);
const ADVISOR_SLOT: Class = Class::new("CCommandStructureLeaderView", 0x36a4d00);
const CHARACTER_ENTRY: Class = Class::new("CCountryCharacterSelectionItem", 0x36a40e8);
const CHARACTER: Class = Class::new("CCharacter", 0x3530150);
const CONTAINER: Class = Class::new("CContainerWindow", 0x36ba978);
const GAME: (u32, u32) = (1920, 1080);

/// The layout for this build (macOS values translated through xlayout).
fn ui(r: &Reader<'_>) -> Result<Ui> {
    let abi = r.string_abi();
    let m = &MACOS;
    Ok(Ui {
        manager: fld(abi, &IDLER, m.manager)?,
        view: fld(abi, &MANAGER, m.view)?,
        view_root: fld(abi, &VIEW, m.view_root)?,
        grid_items: fld(abi, &GRID, m.grid_items)?,
        grid_vtable: img(abi, m.grid_vtable)?,
        category_vtable: img(abi, m.category_vtable)?,
        category_container: fld(abi, &CATEGORY, m.category_container)?,
        category_kind: fld(abi, &CATEGORY, m.category_kind)?,
        kind_key: fld(abi, &KIND, m.kind_key)?,
        slot_vtable: img(abi, m.slot_vtable)?,
        slot_idea: fld(abi, &SLOT, m.slot_idea)?,
        slot_button: fld(abi, &SLOT, m.slot_button)?,
        idea_key: fld(abi, &IDEA, m.idea_key)?,
        window: fld(abi, &VIEW, m.window)?,
        window_root: fld(abi, &SELECTION, m.window_root)?,
        entry_vtable: img(abi, m.entry_vtable)?,
        entry_container: fld(abi, &ENTRY, m.entry_container)?,
        entry_idea: fld(abi, &ENTRY, m.entry_idea)?,
        advisor_slot_vtable: img(abi, m.advisor_slot_vtable)?,
        advisor_button: fld(abi, &ADVISOR_SLOT, m.advisor_button)?,
        advisor_character: fld(abi, &ADVISOR_SLOT, m.advisor_character)?,
        entry_advisor_button: fld(abi, &CHARACTER_ENTRY, m.entry_advisor_button)?,
        entry_advisor_character: fld(abi, &CHARACTER_ENTRY, m.entry_advisor_character)?,
        character_key: fld(abi, &CHARACTER, m.character_key)?,
        gui_from_idler: fld(abi, &IDLER, m.gui_from_idler)?,
        root_windows: fld(abi, &GAME_GUI, m.root_windows)?,
        character_entry_vtable: img(abi, m.character_entry_vtable)?,
        container_base: fld(abi, &CONTAINER, m.container_base)?,
    })
}
fn view(r: &Reader<'_>, image: u64) -> Result<u64> {
    let idler = r.u64(image + game::layout(r).idler_global)?;
    let u = ui(r)?;
    r.u64(r.u64(idler + u.manager)? + u.view)
}
fn open(r: &Reader<'_>, image: u64) -> Result<bool> {
    let u = ui(r)?;
    let root = gui_tree::container(r, r.u64(view(r, image)? + u.view_root)?, image)?
        .ok_or_else(|| Error::new("invalid_gui", "Politics view root unavailable"))?;
    gui_tree::visible(r, root)
}
fn items(r: &Reader<'_>, grid: u64) -> Result<Vec<u64>> {
    let (_, data, count) = r.array_header(grid + ui(r)?.grid_items)?;
    let bytes = if count == 0 {
        Vec::new()
    } else {
        r.read(data, count.min(256) * 8)?
    };
    Ok(bytes
        .chunks_exact(8)
        .map(|b| u64::from_le_bytes(b.try_into().unwrap()))
        .collect())
}
/// Every grid box (depth-first) under a container.
fn grids(r: &Reader<'_>, image: u64, object: u64, depth: u32, out: &mut Vec<u64>) -> Result<()> {
    let grid = image + ui(r)?.grid_vtable;
    for child in gui_tree::children(r, object)? {
        if r.u64(child)? == grid {
            out.push(child);
        } else if depth < 6 {
            if let Some(inner) = gui_tree::container(r, child, image)? {
                grids(r, image, inner, depth + 1, out)?;
            }
        }
    }
    Ok(())
}
fn grid_in(r: &Reader<'_>, image: u64, object: u64, depth: u32) -> Result<Option<u64>> {
    let mut out = Vec::new();
    grids(r, image, object, depth, &mut out)?;
    Ok(out.first().copied())
}
fn idea_key(r: &Reader<'_>, idea: u64) -> Result<Option<String>> {
    if idea == 0 {
        return Ok(None);
    }
    let key = r.string_bounded(idea + ui(r)?.idea_key, 128)?;
    Ok((!key.is_empty() && key != "null").then_some(key))
}
#[derive(Clone, Copy, PartialEq, Eq)]
enum Kind {
    Idea,
    Advisor,
}
fn character_key(r: &Reader<'_>, character: u64) -> Result<Option<String>> {
    if character == 0 {
        return Ok(None);
    }
    let key = r.string_bounded(character + ui(r)?.character_key, 128)?;
    Ok((!key.is_empty()).then_some(key))
}
/// (current idea or advisor, button widget, kind) of slot `slot` in `category`.
fn slot(
    r: &Reader<'_>,
    image: u64,
    category: &str,
    slot: usize,
) -> Result<(Option<String>, u64, Kind)> {
    let u = ui(r)?;
    let root = gui_tree::container(r, r.u64(view(r, image)? + u.view_root)?, image)?
        .ok_or_else(|| Error::new("invalid_gui", "Politics view root unavailable"))?;
    let ideas = gui_tree::child(r, image, root, "ideas")?
        .and_then(|c| gui_tree::container(r, c, image).transpose())
        .transpose()?
        .ok_or_else(|| Error::new("gui_target_unavailable", "Ideas area not found"))?;
    let grid = grid_in(r, image, ideas, 0)?
        .ok_or_else(|| Error::new("gui_target_unavailable", "Idea categories not found"))?;
    for entry in items(r, grid)? {
        if r.u64(entry)? != image + u.category_vtable {
            continue;
        }
        let kind = r.u64(entry + u.category_kind)?;
        if kind == 0 || r.string_bounded(kind + u.kind_key, 64)? != category {
            continue;
        }
        let frame = gui_tree::container(r, r.u64(entry + u.category_container)?, image)?
            .ok_or_else(|| Error::new("invalid_gui", "Category window required"))?;
        let slots = gui_tree::child(r, image, frame, "ideas_grid")?
            .ok_or_else(|| Error::new("gui_target_unavailable", "Slot grid not found"))?;
        let item = *items(r, slots)?
            .get(slot)
            .ok_or_else(|| Error::new("invalid_arguments", "Slot outside this category"))?;
        if r.u64(item)? == image + u.advisor_slot_vtable {
            let current = character_key(r, r.u64(item + u.advisor_character)?)?;
            return Ok((current, r.u64(item + u.advisor_button)?, Kind::Advisor));
        }
        // Arms Against Tyranny: the research & production designers became Military
        // Industrial Organizations; their slots are COpenRosterButtonForPoliticalScreen.
        let vt = r.u64(item)?.wrapping_sub(image);
        let abi = r.string_abi();
        if vt == crate::xlayout::img(abi, 0x362e210)? || vt == crate::xlayout::img(abi, 0x362e238)?
        {
            return Err(Error::new(
                "use_mio_tools",
                "This slot holds a Military Industrial Organization (Arms Against Tyranny), not an \
                 advisor: read them with hoi4_mios and assign or change them with hoi4_mio_* tools \
                 (hoi4_mio_design_team, hoi4_mio_policy, hoi4_mio_trait). Nothing was clicked.",
            ));
        }
        if r.u64(item)? != image + u.slot_vtable {
            return Err(Error::new(
                "invalid_gui",
                format!(
                    "Unknown politics slot class (vtable +{:#x})",
                    r.u64(item)?.wrapping_sub(image)
                ),
            ));
        }
        let current = idea_key(r, r.u64(item + u.slot_idea)?)?;
        let button = r.u64(item + u.slot_button)?;
        if button == 0 {
            return Err(Error::new(
                "invalid_target",
                "This slot is not available yet",
            ));
        }
        return Ok((current, button, Kind::Idea));
    }
    Err(Error::new("invalid_arguments", "Unknown idea category"))
}
fn window_root(r: &Reader<'_>, image: u64) -> Result<u64> {
    let u = ui(r)?;
    let window = r.u64(view(r, image)? + u.window)?;
    gui_tree::container(r, r.u64(window + u.window_root)?, image)?
        .ok_or_else(|| Error::new("invalid_gui", "Selection window root unavailable"))
}
/// The shown character (advisor) selection window, if any.
fn character_window(r: &Reader<'_>, image: u64) -> Result<Option<u64>> {
    let u = ui(r)?;
    let idler = r.u64(image + game::layout(r).idler_global)?;
    let gui = r.u64(idler + u.gui_from_idler)?;
    let (_, data, count) = r.array_header(gui + u.root_windows)?;
    for index in 0..count.min(1024) as u64 {
        // The root-window list holds container objects (1.19.4, checked live 1 Nov 2026);
        // the widget name sits behind the GUI base at object + 0x30.
        let object = r.u64(data + index * 8)?;
        if !(0x10000..=0x7fff_ffff_0000).contains(&object) {
            continue;
        }
        let Some(window) = gui_tree::container(r, object, image)? else {
            continue;
        };
        if gui_tree::name(r, window + u.container_base, image)?.as_deref()
            != Some("command_character_view_window")
        {
            continue;
        }
        {
            if gui_tree::visible(r, window)? {
                return Ok(Some(window));
            }
        }
    }
    Ok(None)
}
fn window_open(r: &Reader<'_>, image: u64) -> Result<bool> {
    Ok(gui_tree::visible(r, window_root(r, image)?)? || character_window(r, image)?.is_some())
}
fn advisor_options(r: &Reader<'_>, image: u64) -> Result<Vec<(String, bool, Rect)>> {
    let u = ui(r)?;
    let Some(window) = character_window(r, image)? else {
        return Ok(Vec::new());
    };
    let mut all = Vec::new();
    grids(r, image, window, 0, &mut all)?;
    let mut out = Vec::new();
    for entry in all
        .into_iter()
        .map(|g| items(r, g))
        .collect::<Result<Vec<_>>>()?
        .concat()
    {
        if r.u64(entry)? != image + u.character_entry_vtable {
            continue;
        }
        let Some(key) = character_key(r, r.u64(entry + u.entry_advisor_character)?)? else {
            continue;
        };
        let button = r.u64(entry + u.entry_advisor_button)?;
        if let Ok(rect) = gui_tree::rect(r, image, button, GAME) {
            out.push((key, !gui_tree::disabled(r, button)?, rect));
        }
    }
    Ok(out)
}
/// Options of the open selection window: (idea key, enabled, rectangle).
fn options(r: &Reader<'_>, image: u64) -> Result<Vec<(String, bool, Rect)>> {
    let u = ui(r)?;
    if character_window(r, image)?.is_some() {
        return advisor_options(r, image);
    }
    let root = window_root(r, image)?;
    let mut all = Vec::new();
    grids(r, image, root, 0, &mut all)?;
    let mut out = Vec::new();
    for entry in all
        .into_iter()
        .map(|g| items(r, g))
        .collect::<Result<Vec<_>>>()?
        .concat()
    {
        if r.u64(entry)? != image + u.entry_vtable {
            continue;
        }
        let Some(key) = idea_key(r, r.u64(entry + u.entry_idea)?)? else {
            continue;
        };
        let frame = gui_tree::container(r, r.u64(entry + u.entry_container)?, image)?
            .ok_or_else(|| Error::new("invalid_gui", "Option window required"))?;
        if let Some(button) = gui_tree::child(r, image, frame, "idea_entry_bg")? {
            if let Ok(rect) = gui_tree::rect(r, image, button, GAME) {
                out.push((key, !gui_tree::disabled(r, button)?, rect));
            }
        }
    }
    Ok(out)
}

pub(super) fn current(
    process: &dyn Process,
    category: &str,
    index: usize,
) -> Result<Option<String>> {
    read(process, |r, i| Ok(slot(r, i, category, index)?.0))
}

/// The slot of `category` that holds `idea`, if any (advisors land in the first free slot).
pub(super) fn holder(process: &dyn Process, category: &str, idea: &str) -> Result<Option<usize>> {
    read(process, |r, i| {
        for index in 0..8 {
            match slot(r, i, category, index) {
                Ok((current, _, _)) if current.as_deref() == Some(idea) => return Ok(Some(index)),
                Ok(_) => {}
                Err(e) if e.code == "invalid_arguments" => break,
                Err(e) => return Err(e),
            }
        }
        Ok(None)
    })
}

fn open_slot(
    process: &dyn Process,
    keys: &dyn GameInput,
    category: &str,
    index: usize,
) -> Result<()> {
    decline_stale(process, keys)?;
    clear_notices(process, keys)?;
    if !read(process, open)? {
        super::ui::open_politics(process, keys, open)?;
    }
    let rect = settled(process, "idea slot", |r, i| {
        gui_tree::rect(r, i, slot(r, i, category, index)?.1, GAME)
    })?;
    clear_notices(process, keys)?;
    let (x, y) = rect.center();
    keys.click(GAME, x, y)?;
    wait(process, "open idea selection", window_open)
}

/// Options offered for a slot (opens and closes the selection window).
pub(super) fn list(
    process: &dyn Process,
    keys: &dyn GameInput,
    category: &str,
    index: usize,
) -> Result<Value> {
    open_slot(process, keys, category, index)?;
    let found = settled(process, "options", |r, i| {
        Ok(options(r, i)?
            .into_iter()
            .map(|(k, e, _)| (k, e))
            .collect::<Vec<_>>())
    })?;
    dismiss(process, keys, |r, i| Ok(!window_open(r, i)?));
    Ok(json!(found
        .into_iter()
        .map(|(key, enabled)| json!({"idea":key,"enabled":enabled}))
        .collect::<Vec<_>>()))
}

/// Choose `idea` for the slot; the caller has journaled intent.
pub(super) fn choose(
    process: &dyn Process,
    keys: &dyn GameInput,
    category: &str,
    index: usize,
    idea: &str,
) -> Result<()> {
    open_slot(process, keys, category, index)?;
    let found = settled(process, "options", |r, i| {
        Ok(options(r, i)?.into_iter().find(|(k, _, _)| k == idea))
    })?;
    let Some((_, enabled, rect)) = found else {
        dismiss(process, keys, |r, i| Ok(!window_open(r, i)?));
        return Err(Error::new(
            "invalid_target",
            "This idea is not offered for the slot",
        ));
    };
    if !enabled {
        dismiss(process, keys, |r, i| Ok(!window_open(r, i)?));
        return Err(Error::new(
            "invalid_target",
            "The game disables this idea (conditions or political power)",
        ));
    }
    clear_notices(process, keys)?;
    let (x, y) = rect.center();
    keys.click(GAME, x, y)?;
    accept_command(process, keys)?;
    // An advisor goes to the category's first free advisor slot, not always the one clicked
    // (gates 1 and 2, 1 Nov 2026), so the pick counts when any slot of the category holds it.
    wait(process, "idea taken", |r, i| {
        if slot(r, i, category, index)?.0.as_deref() == Some(idea) {
            return Ok(true);
        }
        for other in 0..8 {
            match slot(r, i, category, other) {
                Ok((current, _, _)) if current.as_deref() == Some(idea) => return Ok(true),
                Ok(_) => {}
                Err(e) if e.code == "invalid_arguments" => break,
                Err(e) => return Err(e),
            }
        }
        Ok(false)
    })?;
    dismiss(process, keys, |r, i| Ok(!window_open(r, i)?));
    Ok(())
}
