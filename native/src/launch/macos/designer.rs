//! Division Designer screen on macOS: memory reads of the open designer (working copy
//! layout, slot grids, selection window, template dropdown, buttons) and the ordinary
//! clicks that drive it. Nothing here writes game memory: every change is a click, a
//! shift-click or a key, and every step is verified from a fresh read.
//!
//! Mapped live on 26 October 2026 (Mach-O 089EA519, Germany 1938, 1920x1080):
//! * `CDivisionDesignerView` is interface manager (+0x670) slot +0x250; its window root is
//!   at view +0x68 and its working copy of the template at `*(*(view + 0x2110) + 8)`, a
//!   `CDivisionTemplateData` (name +0x8, regiment grid +0x40, support +0x60, priority +0x17c);
//!   the same data object is embedded at +0x18 of every own `CReferencedDivisionTemplate`.
//! * The regiment grid lists slot items column by column (5 rows per column, locked slots
//!   included); a slot is an `unit_icon` button (filled or locked) or an `invisible_icon`
//!   button (empty slot that accepts a battalion); the support grid is one column.
//! * Clicking a slot opens the selection window `designerregimentswindow`: first the group
//!   list (`CDesignerSubUnitTypeItem`: Infantry, Mobile, Armored, ... battalions), then the
//!   unlocked sub-units of one group (`CDesignerSubUnitItem`, definition at item +0x3f8). Support
//!   slots list the unlocked companies directly. The window's Back button (Return) goes up one
//!   level and closes the window at the top. Shift+click on a filled slot removes it (the column
//!   closes up); the game stores the draft as the player edits it.
use super::{
    deployment, special_projects, ui,
    ui::{read, settled, wait},
    units::{self, Layout5},
};
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
use std::time::{Duration, Instant};

pub(super) const GAME: (u32, u32) = (1920, 1080);
const MANAGER: u64 = 0x670;
const VIEW: u64 = 0x250;
const VIEW_ROOT: u64 = 0x68;
const DRAFT_HOLDER: u64 = 0x2110;
const DRAFT_DATA: u64 = 8;
const GRID_ITEMS: u64 = 0x190;
const ITEM_CONTAINER: u64 = 0x10;
const TEXT: u64 = 0x100;
const BUTTON_TEXT: u64 = 0x158;
const UNIT_ITEM_DEFINITION: u64 = 0x3f8;
// CDropDownBox members (the template dropdown of the designer).
const DD_EXPAND: u64 = 0x880;
const DD_WINDOW: u64 = 0x888;
/// Slot pitch of the grids (`slotsize` in divisiondesignerview.gui).
const REGIMENT_PITCH: (i32, i32) = (102, 46);
const SUPPORT_PITCH: i32 = 35;

fn libcxx(r: &Reader<'_>) -> Result<()> {
    match r.string_abi() {
        StringAbi::LibCxx => Ok(()),
        StringAbi::LibStdCxx => Err(Error::new(
            "capability_unavailable",
            "The Division Designer is not reviewed for this build",
        )),
    }
}
fn view(r: &Reader<'_>, image: u64) -> Result<u64> {
    libcxx(r)?;
    let idler = r.u64(image + game::layout(r).idler_global)?;
    let view = r.u64(r.u64(idler + MANAGER)? + VIEW)?;
    r.object_type(view, "CDivisionDesignerView")?;
    Ok(view)
}
fn root(r: &Reader<'_>, image: u64) -> Result<u64> {
    let view = view(r, image)?;
    gui_tree::container(r, r.u64(view + VIEW_ROOT)?, image)?
        .ok_or_else(|| Error::new("invalid_gui", "Division Designer window unavailable"))
}
fn named(r: &Reader<'_>, image: u64, parent: u64, name: &str) -> Result<u64> {
    gui_tree::child(r, image, parent, name)?
        .ok_or_else(|| Error::new("gui_target_unavailable", format!("{name} not found")))
}
fn named_frame(r: &Reader<'_>, image: u64, parent: u64, name: &str) -> Result<u64> {
    let child = named(r, image, parent, name)?;
    gui_tree::container(r, child, image)?
        .ok_or_else(|| Error::new("invalid_gui", format!("{name} is not a window")))
}
fn items(r: &Reader<'_>, grid: u64) -> Result<Vec<u64>> {
    let (_, data, count) = r.array_header(grid + GRID_ITEMS)?;
    if count > 256 {
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
fn item_frame(r: &Reader<'_>, image: u64, item: u64) -> Result<u64> {
    gui_tree::container(r, r.u64(item + ITEM_CONTAINER)?, image)?
        .ok_or_else(|| Error::new("invalid_gui", "Grid item window required"))
}
fn text_of(r: &Reader<'_>, image: u64, parent: u64, name: &str) -> Result<String> {
    let t = named(r, image, parent, name)?;
    Ok(special_projects::plain(&r.string_bounded(t + TEXT, 256)?))
}
fn button_label(r: &Reader<'_>, button: u64) -> Result<String> {
    Ok(special_projects::plain(
        &r.string_bounded(button + BUTTON_TEXT, 128)?,
    ))
}

/// The designer window is shown.
pub(super) fn is_open(r: &Reader<'_>, image: u64) -> Result<bool> {
    gui_tree::visible(r, root(r, image)?)
}
/// Non-HQ templates only: the army HQ designer has another layout.
fn non_hq_view(r: &Reader<'_>, image: u64) -> Result<u64> {
    let v = named_frame(r, image, root(r, image)?, "non_hq_view")?;
    if !gui_tree::visible(r, v)? {
        return Err(Error::new(
            "hq_template_unsupported",
            "The designer shows an army HQ template; only line templates are supported",
        ));
    }
    Ok(v)
}

/// The designer's working copy of the template.
pub(super) fn draft(r: &Reader<'_>, image: u64) -> Result<Layout5> {
    let view = view(r, image)?;
    let holder = r.u64(view + DRAFT_HOLDER)?;
    if !(0x10000..=0x7fff_ffff_0000).contains(&holder) {
        return Err(Error::new("invalid_gui", "Designer working copy unavailable"));
    }
    units::template_data(r, image, r.u64(holder + DRAFT_DATA)?)
}

// ---- slots -----------------------------------------------------------------------
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Grid {
    Regiments,
    Support,
}
impl Grid {
    fn name(self) -> &'static str {
        match self {
            Self::Regiments => "regiments_grid",
            Self::Support => "support_grid",
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum SlotState {
    Filled,
    /// Empty and accepting a unit.
    Add,
    Locked,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct Slot {
    pub col: usize,
    pub row: usize,
    pub state: SlotState,
    pub rect: Rect,
}
/// Slots of one grid with their column/row and the click rectangle.
pub(super) fn slots(r: &Reader<'_>, image: u64, grid: Grid, draft: &Layout5) -> Result<Vec<Slot>> {
    let subunits = named_frame(r, image, non_hq_view_parent(r, image)?, "subunits")?;
    let grid_widget = named(r, image, subunits, grid.name())?;
    let mut raw = Vec::new();
    for item in items(r, grid_widget)? {
        let frame = item_frame(r, image, item)?;
        let unit = named(r, image, frame, "unit_icon")?;
        let add = named(r, image, frame, "invisible_icon")?;
        let pos = gui_tree::rect_unclipped(r, image, unit, GAME)?;
        let add_shown = gui_tree::shown(r, add)?;
        let unit_shown = gui_tree::shown(r, unit)?;
        let click = if add_shown {
            gui_tree::rect_unclipped(r, image, add, GAME)?
        } else {
            pos
        };
        raw.push((pos, add_shown, unit_shown, click));
    }
    let (x0, y0) = (
        raw.iter().map(|s| s.0.x).min().unwrap_or(0),
        raw.iter().map(|s| s.0.y).min().unwrap_or(0),
    );
    let mut out = Vec::new();
    for (pos, add_shown, unit_shown, click) in raw {
        let (col, row) = match grid {
            Grid::Regiments => (
                ((pos.x - x0 + REGIMENT_PITCH.0 / 2) / REGIMENT_PITCH.0) as usize,
                ((pos.y - y0 + REGIMENT_PITCH.1 / 2) / REGIMENT_PITCH.1) as usize,
            ),
            Grid::Support => (0, ((pos.y - y0 + SUPPORT_PITCH / 2) / SUPPORT_PITCH) as usize),
        };
        let filled = match grid {
            Grid::Regiments => draft.columns.get(col).is_some_and(|c| row < c.len()),
            Grid::Support => row < draft.support.len(),
        };
        let state = if filled && unit_shown {
            SlotState::Filled
        } else if add_shown && !filled {
            SlotState::Add
        } else {
            SlotState::Locked
        };
        out.push(Slot {
            col,
            row,
            state,
            rect: click,
        });
    }
    Ok(out)
}
fn non_hq_view_parent(r: &Reader<'_>, image: u64) -> Result<u64> {
    non_hq_view(r, image)?;
    root(r, image)
}

// ---- selection window ------------------------------------------------------------
#[derive(Clone, Debug, PartialEq)]
pub(super) enum EntryKind {
    Group,
    Unit(String),
    /// The "Remove" entry of a filled slot's list (costs experience like any change).
    Remove,
}
#[derive(Clone, Debug, PartialEq)]
pub(super) struct Entry {
    pub kind: EntryKind,
    pub label: String,
    pub cost_text: String,
    /// First number of the cost line (army experience), if any.
    pub cost: Option<i64>,
    pub rect: Rect,
}
#[derive(Clone, Debug, PartialEq)]
pub(super) struct Selection {
    pub entries: Vec<Entry>,
    pub back: Rect,
}
fn first_number(text: &str) -> Option<i64> {
    let mut digits = String::new();
    for c in text.chars() {
        if c.is_ascii_digit() {
            digits.push(c);
        } else if !digits.is_empty() {
            break;
        }
    }
    digits.parse().ok()
}
fn selection_window(r: &Reader<'_>, image: u64) -> Result<u64> {
    named_frame(r, image, root(r, image)?, "designerregimentswindow")
}
pub(super) fn selection_open(r: &Reader<'_>, image: u64) -> Result<bool> {
    gui_tree::visible(r, selection_window(r, image)?)
}
pub(super) fn selection(r: &Reader<'_>, image: u64) -> Result<Option<Selection>> {
    let window = selection_window(r, image)?;
    if !gui_tree::visible(r, window)? {
        return Ok(None);
    }
    let back = gui_tree::rect(r, image, named(r, image, window, "cancel_button")?, GAME)?;
    let grid = named(r, image, window, "designer_regiments_grid")?;
    let mut entries = Vec::new();
    for item in items(r, grid)? {
        let frame = item_frame(r, image, item)?;
        let label = text_of(r, image, frame, "name")?;
        let cost_text = text_of(r, image, frame, "cost").unwrap_or_default();
        let icon = named(r, image, frame, "subunit_icon")?;
        let rect = gui_tree::rect(r, image, icon, GAME)?;
        let kind = if r.object_type(item, "CDesignerSubUnitItem").is_ok() {
            let def = r.u64(item + UNIT_ITEM_DEFINITION)?;
            match units::key_at(r, image, def)? {
                Some(key) => EntryKind::Unit(key),
                None => EntryKind::Remove,
            }
        } else if r.object_type(item, "CDesignerSubUnitTypeItem").is_ok() {
            EntryKind::Group
        } else {
            continue;
        };
        // The cost line of a group entry is hidden when it is free; only a shown one counts.
        let cost = first_number(&cost_text);
        entries.push(Entry {
            kind,
            label,
            cost_text,
            cost,
            rect,
        });
    }
    Ok(Some(Selection { entries, back }))
}

// ---- dropdown, buttons, texts ----------------------------------------------------
#[derive(Clone, Debug, PartialEq)]
pub(super) struct DropItem {
    pub name: String,
    pub is_new: bool,
    pub rect: Rect,
}
#[derive(Clone, Debug, PartialEq)]
pub(super) struct Dropdown {
    pub expand: Rect,
    pub expanded: bool,
    pub items: Vec<DropItem>,
}
fn dropdown_widget(r: &Reader<'_>, image: u64) -> Result<u64> {
    named(r, image, non_hq_view(r, image)?, "divisions")
}
pub(super) fn dropdown(r: &Reader<'_>, image: u64) -> Result<Dropdown> {
    let dd = dropdown_widget(r, image)?;
    let expand = gui_tree::rect(r, image, r.u64(dd + DD_EXPAND)?, GAME)?;
    let window = gui_tree::container(r, r.u64(dd + DD_WINDOW)?, image)?
        .ok_or_else(|| Error::new("invalid_gui", "Template list window unavailable"))?;
    let expanded = gui_tree::visible(r, window)?;
    let grid = named(r, image, window, "divisions_grid")?;
    let mut out = Vec::new();
    for item in items(r, grid)? {
        let frame = item_frame(r, image, item)?;
        let name = text_of(r, image, frame, "name")?;
        let icon = named(r, image, frame, "division")?;
        let rect = gui_tree::rect_unclipped(r, image, icon, GAME)?;
        out.push(DropItem {
            name,
            is_new: r.object_type(item, "CDesignerNewTemplateItem").is_ok(),
            rect,
        });
    }
    Ok(Dropdown {
        expand,
        expanded,
        items: out,
    })
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct Button {
    pub rect: Rect,
    pub disabled: bool,
}
fn button(r: &Reader<'_>, image: u64, parent: u64, name: &str) -> Result<Button> {
    let b = named(r, image, parent, name)?;
    Ok(Button {
        rect: gui_tree::rect(r, image, b, GAME)?,
        disabled: gui_tree::disabled(r, b)?,
    })
}
pub(super) fn save_button(r: &Reader<'_>, image: u64) -> Result<Button> {
    button(r, image, root(r, image)?, "save_button")
}
pub(super) fn close_button(r: &Reader<'_>, image: u64) -> Result<Button> {
    button(r, image, root(r, image)?, "close_button")
}
/// Priority button 0 reserve, 1 regular, 2 elite.
pub(super) fn priority_button(r: &Reader<'_>, image: u64, p: i32) -> Result<Button> {
    let name = format!("priority_button_{}", p + 1);
    button(r, image, non_hq_view(r, image)?, &name)
}
/// Designer texts: (save label, army experience held, estimated production cost line,
/// combat width).
#[derive(Clone, Debug, PartialEq)]
pub(super) struct Texts {
    pub save_label: String,
    pub save_cost: Option<i64>,
    pub save_enabled: bool,
    pub experience: Option<i64>,
    pub production: String,
    pub width: Option<i64>,
}
pub(super) fn texts(r: &Reader<'_>, image: u64) -> Result<Texts> {
    let root = root(r, image)?;
    let save = named(r, image, root, "save_button")?;
    let save_label = button_label(r, save)?;
    let production = text_of(r, image, root, "production_value").unwrap_or_default();
    let experience = text_of(r, image, root, "experience")?;
    let width = {
        let f = named_frame(r, image, root, "combat_width_view")?;
        text_of(r, image, f, "combat_width_text")?
    };
    Ok(Texts {
        save_cost: first_number(save_label.trim_start_matches(|c: char| !c.is_ascii_digit())),
        save_label,
        save_enabled: !gui_tree::disabled(r, save)?,
        experience: first_number(&experience),
        production,
        width: first_number(&width),
    })
}

// ---- opening and closing -----------------------------------------------------------
fn click(keys: &MacKeys, rect: Rect) -> Result<()> {
    let (x, y) = rect.center();
    keys.click(GAME, x, y)?;
    Ok(())
}
fn open_settled(process: &dyn Process, keys: &MacKeys) -> Result<()> {
    wait(process, "designer window", is_open)?;
    // The window fades in; wait for its working copy and dropdown to answer.
    settled(process, "designer geometry", |r, i| {
        Ok((dropdown(r, i)?.expand, save_button(r, i)?.rect))
    })?;
    let _ = keys;
    Ok(())
}
/// Open the designer with the panel's Division Designer button (the first template shown).
pub(super) fn open_new_designer(process: &dyn Process, keys: &MacKeys) -> Result<()> {
    ui::clear_notices(process, keys)?;
    if read(process, is_open)? {
        return Ok(());
    }
    deployment::ensure_open(process, keys)?;
    let rect = settled(process, "designer button", deployment::designer_button)?;
    click(keys, rect)?;
    open_settled(process, keys)
}
/// Open the designer on an existing template with its Edit button.
pub(super) fn open_template(process: &dyn Process, keys: &MacKeys, name: &str) -> Result<()> {
    ui::clear_notices(process, keys)?;
    if read(process, is_open)? {
        select_template(process, keys, name)?;
        return Ok(());
    }
    deployment::ensure_open(process, keys)?;
    let rect = settled(process, "edit button", |r, i| {
        deployment::row_button(r, i, name, "edit_button").map(|(rect, _)| rect)
    })?;
    click(keys, rect)?;
    open_settled(process, keys)?;
    wait(process, "template loaded", |r, i| {
        Ok(draft(r, i)?.name == name)
    })
}
/// Pick a template (or "Create Empty") in the designer's dropdown.
pub(super) fn select_template(process: &dyn Process, keys: &MacKeys, name: &str) -> Result<()> {
    let create = name.is_empty();
    let current = read(process, draft)?;
    if !create && current.name == name {
        return Ok(());
    }
    let dd = read(process, dropdown)?;
    if !dd.expanded {
        click(keys, dd.expand)?;
        wait(process, "template list open", |r, i| Ok(dropdown(r, i)?.expanded))?;
    }
    let target = settled(process, "template list", |r, i| {
        let d = dropdown(r, i)?;
        d.items
            .iter()
            .find(|it| if create { it.is_new } else { it.name == name })
            .map(|it| it.rect)
            .ok_or_else(|| Error::new("template_unknown", "Template not in the designer list"))
    })?;
    click(keys, target)?;
    wait(process, "template selected", |r, i| {
        let d = draft(r, i)?;
        Ok(if create {
            d.columns.is_empty() && d.support.is_empty()
        } else {
            d.name == name
        } && !dropdown(r, i)?.expanded)
    })
}
/// A fresh empty draft ("Create Empty").
pub(super) fn create_empty(process: &dyn Process, keys: &MacKeys) -> Result<()> {
    select_template(process, keys, "")
}
/// Close the designer (its close button); unsaved changes are discarded by the game.
pub(super) fn close(process: &dyn Process, keys: &MacKeys) -> Result<()> {
    if !read(process, is_open)? {
        return Ok(());
    }
    if read(process, selection_open)? {
        keys.press(Key::Return)?;
        let _ = wait(process, "selection closes", |r, i| Ok(!selection_open(r, i)?));
    }
    // The two-level selection may need a second Back.
    if read(process, selection_open)? {
        keys.press(Key::Return)?;
        wait(process, "selection closes", |r, i| Ok(!selection_open(r, i)?))?;
    }
    let b = settled(process, "close button", |r, i| Ok(close_button(r, i)?.rect))?;
    click(keys, b)?;
    wait(process, "designer closes", |r, i| Ok(!is_open(r, i)?))
}
/// Close the designer if it was left open, then the Recruit & Deploy panel.
pub(super) fn leave(process: &dyn Process, keys: &MacKeys) {
    let _ = close(process, keys);
    deployment::leave(process, keys);
}

// ---- editing --------------------------------------------------------------------
/// Sub-unit keys the selection window offers, per group, found by opening the window.
fn selection_state(process: &dyn Process) -> Result<Option<Selection>> {
    read(process, selection)
}
/// Go back one level in the selection window.
fn back(keys: &MacKeys, sel: &Selection) -> Result<()> {
    click(keys, sel.back)
}
fn units_listed(sel: &Selection) -> bool {
    sel.entries
        .iter()
        .any(|e| matches!(e.kind, EntryKind::Unit(_) | EntryKind::Remove))
}
fn wait_selection(process: &dyn Process, units_level: bool) -> Result<Selection> {
    let deadline = Instant::now() + Duration::from_secs(3);
    loop {
        if let Some(sel) = selection_state(process)? {
            if !sel.entries.is_empty() && units_listed(&sel) == units_level {
                return settled(process, "selection list", |r, i| {
                    selection(r, i)?.ok_or_else(|| Error::new("gui_target_unavailable", "closed"))
                });
            }
        }
        if Instant::now() >= deadline {
            return Err(Error::new(
                "input_readback_failed",
                "The selection window did not show the expected list",
            ));
        }
        std::thread::sleep(Duration::from_millis(40));
    }
}
/// Which group title a battalion key is listed under first (a hint only; the window decides).
pub(super) fn group_hint(key: &str) -> &'static str {
    match key {
        "infantry" | "mountaineers" | "marine" | "paratrooper" | "bicycle_battalion" | "camelry"
        | "cavalry_infantry" | "special_forces" | "gurkha" | "militia" => "Infantry Battalions",
        "cavalry" | "motorized" | "mechanized" | "armored_car" | "amphibious_mechanized"
        | "bus" => "Mobile Battalions",
        k if k.ends_with("_armor") || k == "amphibious_armor" => "Armored Battalions",
        k if k.starts_with("mot_") => "Mobile Combat Support Battalions",
        k if k.contains("_sp_") || k.contains("tank_destroyer") => {
            "Armored Combat Support Battalions"
        }
        _ => "Combat Support Battalions",
    }
}
/// Open a slot and choose the sub-unit `key` there. Returns the experience cost shown for the
/// entry. The caller verifies the working copy afterwards.
///
/// An empty slot lists the groups first, then the unlocked units of the chosen group; a filled
/// slot lists the units of its own group directly, with a "Remove" entry on top. A unit of
/// another group replaces the old one in two steps (Remove, then add).
pub(super) fn choose(
    process: &dyn Process,
    keys: &MacKeys,
    grid: Grid,
    col: usize,
    row: usize,
    key: &str,
) -> Result<Option<i64>> {
    let draft0 = read(process, draft)?;
    let slot = settled(process, "slot", |r, i| {
        let d = draft(r, i)?;
        slots(r, i, grid, &d)?
            .into_iter()
            .find(|s| s.col == col && s.row == row)
            .ok_or_else(|| Error::new("slot_locked", "That slot is not shown"))
    })?;
    if slot.state == SlotState::Locked {
        return Err(Error::new(
            "slot_locked",
            format!(
                "{} slot column {col} row {row} is locked in this designer",
                match grid {
                    Grid::Regiments => "Regiment",
                    Grid::Support => "Support",
                }
            ),
        ));
    }
    click(keys, slot.rect)?;
    wait(process, "selection window", selection_open)?;
    let mut sel = wait_selection_any(process)?;
    let close_window = |keys: &MacKeys| {
        let _ = keys.press(Key::Return);
        let _ = wait(process, "selection closes", |r, i| Ok(!selection_open(r, i)?));
    };
    let wanted = EntryKind::Unit(key.to_string());
    if units_listed(&sel) && !sel.entries.iter().any(|e| e.kind == wanted) {
        // A filled regiment slot whose list holds only its own group.
        if slot.state == SlotState::Filled && grid == Grid::Regiments {
            let remove = sel.entries.iter().find(|e| e.kind == EntryKind::Remove).cloned();
            if let Some(entry) = remove {
                click(keys, entry.rect)?;
                wait(process, "unit removed", |r, i| {
                    Ok(draft(r, i)? != draft0 && !selection_open(r, i)?)
                })?;
                // The slot is empty now; add the unit through the group list.
                return choose(process, keys, grid, col, row, key);
            }
        }
        close_window(keys);
        return Err(Error::new(
            "unit_not_unlocked",
            format!("{key} is not offered by the designer (not unlocked yet or already used)"),
        ));
    }
    if !units_listed(&sel) {
        // Group list: try the hinted group first, then the others.
        let hint = group_hint(key);
        let mut order: Vec<usize> = (0..sel.entries.len()).collect();
        order.sort_by_key(|i| sel.entries[*i].label != hint);
        let mut found = false;
        for index in order {
            if !units_listed(&sel) {
                let entry = sel.entries[index].clone();
                click(keys, entry.rect)?;
                sel = wait_selection(process, true)?;
            }
            if sel.entries.iter().any(|e| e.kind == wanted) {
                found = true;
                break;
            }
            // Not in this group: back to the group list for the next one.
            back(keys, &sel)?;
            sel = wait_selection(process, false)?;
        }
        if !found {
            close_window(keys);
            return Err(Error::new(
                "unit_not_unlocked",
                format!("{key} is not offered by the designer (not unlocked yet)"),
            ));
        }
    }
    let entry = sel
        .entries
        .iter()
        .find(|e| e.kind == wanted)
        .cloned()
        .ok_or_else(|| Error::new("invalid_gui", "The chosen unit vanished from the list"))?;
    click(keys, entry.rect)?;
    wait(process, "unit chosen", |r, i| {
        Ok(draft(r, i)? != draft0 && !selection_open(r, i)?)
    })?;
    Ok(entry.cost)
}
fn wait_selection_any(process: &dyn Process) -> Result<Selection> {
    let deadline = Instant::now() + Duration::from_secs(3);
    loop {
        if let Some(sel) = selection_state(process)? {
            if !sel.entries.is_empty() {
                return settled(process, "selection list", |r, i| {
                    selection(r, i)?.ok_or_else(|| Error::new("gui_target_unavailable", "closed"))
                });
            }
        }
        if Instant::now() >= deadline {
            return Err(Error::new(
                "input_readback_failed",
                "The selection window listed nothing",
            ));
        }
        std::thread::sleep(Duration::from_millis(40));
    }
}
/// Shift+click a filled slot: the game removes the unit and closes the column up.
pub(super) fn remove(
    process: &dyn Process,
    keys: &MacKeys,
    grid: Grid,
    col: usize,
    row: usize,
) -> Result<()> {
    let before = read(process, draft)?;
    let slot = settled(process, "slot", |r, i| {
        let d = draft(r, i)?;
        slots(r, i, grid, &d)?
            .into_iter()
            .find(|s| s.col == col && s.row == row && s.state == SlotState::Filled)
            .ok_or_else(|| Error::new("slot_empty", "No unit in that slot"))
    })?;
    let (x, y) = slot.rect.center();
    keys.shift_click(GAME, x, y)?;
    wait(process, "unit removed", |r, i| Ok(draft(r, i)? != before))
}

// ---- name and priority ------------------------------------------------------------------
/// ANSI key code and shift flag of one name character.
pub(super) fn key_for(c: char) -> Option<(u16, bool)> {
    const LETTERS: [u16; 26] = [
        0, 11, 8, 2, 14, 3, 5, 4, 34, 38, 40, 37, 46, 45, 31, 35, 12, 15, 1, 17, 32, 9, 13, 7, 16,
        6,
    ];
    const DIGITS: [u16; 10] = [29, 18, 19, 20, 21, 23, 22, 26, 28, 25];
    Some(match c {
        'a'..='z' => (LETTERS[(c as u8 - b'a') as usize], false),
        'A'..='Z' => (LETTERS[(c as u8 - b'A') as usize], true),
        '0'..='9' => (DIGITS[(c as u8 - b'0') as usize], false),
        ' ' => (49, false),
        '-' => (27, false),
        '_' => (27, true),
        '.' => (47, false),
        ',' => (43, false),
        '/' => (44, false),
        '\'' => (39, false),
        '(' => (DIGITS[9], true),
        ')' => (DIGITS[0], true),
        _ => return None,
    })
}
/// Whether every character of `name` can be typed.
pub(super) fn typable(name: &str) -> bool {
    !name.is_empty() && name.chars().all(|c| key_for(c).is_some())
}
/// Replace the template name: click into the name box, Backspace away the old text (the first
/// Backspace proves the box has the keyboard focus), then type the new name one key at a time,
/// each verified against the working copy. A space is typed only after focus is proven, since
/// a stray Space would toggle the game's pause.
pub(super) fn set_name(process: &dyn Process, keys: &MacKeys, name: &str) -> Result<()> {
    if !typable(name) {
        return Err(Error::new(
            "name_not_typable",
            "Template names may use letters, digits, space and - _ . , / ' ( )",
        ));
    }
    let current = read(process, draft)?.name;
    if current == name {
        return Ok(());
    }
    let dd = settled(process, "name box", |r, i| Ok(dropdown(r, i)?.expand))?;
    let (x, y) = dd.center();
    // The name box is the left part of the dropdown; the expand arrow sits at its right end.
    keys.click(GAME, x - 110.0, y)?;
    std::thread::sleep(Duration::from_millis(250));
    keys.press_code(119)?; // End
    let mut length = current.chars().count();
    if length > 0 {
        keys.press_code(51)?;
        length -= 1;
        wait(process, "name box focused", |r, i| {
            Ok(draft(r, i)?.name.chars().count() == length)
        })
        .map_err(|_| {
            Error::new(
                "input_readback_failed",
                "The name box did not take the keyboard focus",
            )
        })?;
    }
    while length > 0 {
        keys.press_code(51)?;
        length -= 1;
        wait(process, "name erased", |r, i| {
            Ok(draft(r, i)?.name.chars().count() == length)
        })?;
    }
    let mut typed = String::new();
    for c in name.chars() {
        let (code, shifted) = key_for(c).ok_or_else(|| Error::new("name_not_typable", "key"))?;
        if shifted {
            keys.press_shifted(code)?;
        } else {
            keys.press_code(code)?;
        }
        typed.push(c);
        wait(process, "name key", |r, i| Ok(draft(r, i)?.name == typed))?;
    }
    Ok(())
}
pub(super) fn set_priority(process: &dyn Process, keys: &MacKeys, priority: i32) -> Result<()> {
    if read(process, draft)?.priority == priority {
        return Ok(());
    }
    let b = settled(process, "priority button", |r, i| {
        Ok(priority_button(r, i, priority)?.rect)
    })?;
    click(keys, b)?;
    wait(process, "priority set", |r, i| {
        Ok(draft(r, i)?.priority == priority)
    })
}

// ---- save and reset ------------------------------------------------------------------------
/// Click Save/Update once and wait for the own template list to hold the draft's layout.
pub(super) fn save(process: &dyn Process, keys: &MacKeys, expect: &Layout5) -> Result<()> {
    let b = settled(process, "save button", |r, i| Ok(save_button(r, i)?.rect))?;
    click(keys, b)?;
    let deadline = Instant::now() + Duration::from_secs(4);
    loop {
        let list = read(process, units::template_layouts)?;
        if list.iter().any(|l| l == expect) {
            return Ok(());
        }
        if ui::decline_stale(process, keys).is_err() {
            return Err(Error::new("input_readback_failed", "Save left a dialog open"));
        }
        if Instant::now() >= deadline {
            return Err(Error::new(
                "input_readback_failed",
                "Save did not change the own template list as requested",
            ));
        }
        std::thread::sleep(Duration::from_millis(60));
    }
}

// ---- listing what the designer offers ---------------------------------------------------------
#[derive(Clone, Debug, PartialEq)]
pub(super) struct ListedUnit {
    pub key: String,
    pub label: String,
    pub group: String,
    pub cost: Option<i64>,
}
#[derive(Clone, Debug, PartialEq)]
pub(super) struct Listing {
    pub units: Vec<ListedUnit>,
    /// (group title, experience for a new column)
    pub groups: Vec<(String, Option<i64>)>,
}
/// Open the first empty slot of a grid and read what its selection window offers; closes the
/// window again without choosing.
pub(super) fn list_group_units(process: &dyn Process, keys: &MacKeys, grid: Grid) -> Result<Listing> {
    let slot = settled(process, "empty slot", |r, i| {
        let d = draft(r, i)?;
        slots(r, i, grid, &d)?
            .into_iter()
            .find(|s| s.state == SlotState::Add)
            .ok_or_else(|| Error::new("slot_locked", "No empty slot to open the list from"))
    })?;
    click(keys, slot.rect)?;
    wait(process, "selection window", selection_open)?;
    let mut sel = wait_selection_any(process)?;
    let mut out = Listing {
        units: Vec::new(),
        groups: Vec::new(),
    };
    if units_listed(&sel) {
        for e in &sel.entries {
            if let EntryKind::Unit(key) = &e.kind {
                out.units.push(ListedUnit {
                    key: key.clone(),
                    label: e.label.clone(),
                    group: "support".to_string(),
                    cost: e.cost,
                });
            }
        }
    } else {
        let groups: Vec<Entry> = sel.entries.clone();
        for g in groups {
            out.groups.push((g.label.clone(), g.cost));
            click(keys, g.rect)?;
            sel = wait_selection(process, true)?;
            for e in &sel.entries {
                if let EntryKind::Unit(key) = &e.kind {
                    out.units.push(ListedUnit {
                        key: key.clone(),
                        label: e.label.clone(),
                        group: g.label.clone(),
                        cost: e.cost,
                    });
                }
            }
            back(keys, &sel)?;
            sel = wait_selection(process, false)?;
        }
        let _ = &sel;
    }
    keys.press(Key::Return)?;
    wait(process, "selection closes", |r, i| Ok(!selection_open(r, i)?))?;
    Ok(out)
}
