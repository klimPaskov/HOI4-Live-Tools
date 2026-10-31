//! Equipment designers on macOS (089EA519): memory reads of the open designer windows and the
//! ordinary clicks that open them from the Production panel.
//!
//! Mapped live on 26 October 2026 (Germany, 15 October 1938, 1920x1080):
//! * The Production panel's equipment selection windows (`CProductionEquipmentWindow`, view
//!   +0x1cd0 for army and air equipment, +0x1cd8 for ships) list one row per design; a row's
//!   `variant` button is the game's "Create variant" button, its `obsolete` button the
//!   "Decommission" button (tooltips read on screen). Rows: military array +0xb98, ship array
//!   +0xbb0, variant at row +0xc30, row window at row +0x10.
//! * Equipment without module slots (tanks, planes without airframe modules) opens the
//!   *upgrade designer* (`CEquipmentUpgradeDesignerWindow`, embedded in each selection window at
//!   +0xbe0; root window +0x1540, view +0x1548 whose draft variant is +0x7e8 and base variant
//!   +0x7e0): upgrade rows with `decrease_level`/`increase_level` buttons and a level text, a
//!   name edit box (text +0x4d8), Save with the experience cost.
//! * Hulls and module-based chassis open `CEquipmentDesignerView` (interface manager slots
//!   +0x258 ship, +0x260 tank, +0x268 plane; root window at view +0xbbb0, draft variant at
//!   view +0x138): module slot views (arrays +0xedd0 custom, +0xede8 fixed; slot definition
//!   +0xb0, button +0x4a0), a two level module selector (categories, then modules whose entries
//!   carry the module at +0x818), a name edit box (text +0x3d8) and Save.
//!
//! Nothing here writes memory; every change is a click or a key.
use super::{
    fleetmem, special_projects,
    ui::{read, settled, wait},
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
use serde_json::{json, Map, Value};
use std::time::{Duration, Instant};

pub(super) const GAME: (u32, u32) = (1920, 1080);
const MANAGER: u64 = 0x670;
const PRODUCTION_VIEW: u64 = 0xd8;
const SELECTION_ARMY: u64 = 0x1f10;
const SELECTION_NAVY: u64 = 0x1f18;
const SEL_ROOT: u64 = 0xcf8;
const SEL_UDW: u64 = 0xd00;
const ROW_CONTAINER: u64 = 0x10;
const ROW_VARIANT: u64 = 0xd50;
const UDW_ROOT: u64 = 0x1720;
const UDW_VIEW: u64 = 0x1728;
// 1.19.4: +0xc0 (live: CEquipmentUpgradeDesignerView +0x8a0 holds base design 1319 after its
// designer opened, +0x8a8 the draft variant (id 0); 0x7e0/0x7e8/0x7f0 in 1.19.3).
const UDV_BASE: u64 = 0x8a0;
const UDV_DRAFT: u64 = 0x8a8;
const UDV_COST: u64 = 0x8b0;
// 1.19.4: 0x4d8 -> 0x3c8 (live: `variant_name`, now a plain CEditBox, shows "Bf 109" at +0x3c8;
// +0x4e8 stays empty).
const UDW_EDIT_TEXT: u64 = 0x3c8;
const EDV_SHIP: u64 = 0x258;
const EDV_TANK: u64 = 0x260;
const EDV_PLANE: u64 = 0x268;
// 1.19.4: root window 0xbbb0 -> 0xc730 (live: CEquipmentDesignerView +0xc730 is `countryequipmentdesignerview` in all three slots).
const EDV_ROOT: u64 = 0xc730;
// 1.19.4: 0x138 -> 0x178 (live: the open ship designer's draft CEquipmentVariant, id 0).
const EDV_DRAFT: u64 = 0x178;
// 1.19.4: slot arrays +0xee0 (static: neighbours 0xed88/0xef10 of CEquipmentDesignerView both
// moved +0xee0; live: both arrays hold 9 CEquipmentModuleSlotView on a destroyer hull).
const EDV_SLOTS_CUSTOM: u64 = 0xfcb0;
const EDV_SLOTS_FIXED: u64 = 0xfcc8;
// 1.19.4: the name box (CEditBox, view +0xd478, 0xc7d8 in 1.19.3) keeps its text at +0x3c8
// (live: "Basic Destroyer" with the ship designer open on design 1338).
const EDV_EDIT_BOX: u64 = 0xd478;
const EDV_EDIT_TEXT: u64 = 0x3c8;
const SLOT_DEF: u64 = 0xb0;
// 1.19.4: slot view button 0x4a0 -> 0x500 (static +0x60; live: CButtonStandard at +0x500).
const SLOT_BUTTON: u64 = 0x500;
const SLOT_ID: u64 = 0x8;
const SLOT_NAME: u64 = 0x28;
// 1.19.4: 0x818 -> 0x8d8 (live: CEquipmentModuleEntry +0x8d8 is the CEquipmentModule, keys
// light_ship_engine_1/2 at +0x10).
const ENTRY_MODULE: u64 = 0x8d8;
const MODULE_KEY: u64 = 0x10;
const GRID_ITEMS: u64 = 0x190;
const ITEM_CONTAINER: u64 = 0x10;
const TEXT: u64 = 0x100;
// 1.19.4: CButtonStandard text 0x158 -> 0x160 (same move as navalui BUTTON_TEXT).
const BUTTON_TEXT: u64 = 0x160;
const VT_ENTRY_REMOVAL: u64 = 0x0;

/// Variant stat scale and the fleetmem decoders are shared with `hoi4_designs`.
fn libcxx(r: &Reader<'_>) -> Result<()> {
    match r.string_abi() {
        StringAbi::LibCxx => Ok(()),
        StringAbi::LibStdCxx => Err(Error::new(
            "capability_unavailable",
            "The equipment designers are reviewed for the macOS build only",
        )),
    }
}
fn manager(r: &Reader<'_>, image: u64) -> Result<u64> {
    libcxx(r)?;
    let idler = r.u64(image + game::layout(r).idler_global)?;
    r.u64(idler + MANAGER)
}
fn production_view(r: &Reader<'_>, image: u64) -> Result<u64> {
    r.u64(manager(r, image)? + PRODUCTION_VIEW)
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
fn text_at(r: &Reader<'_>, widget: u64) -> Result<String> {
    Ok(special_projects::plain(&r.string_bounded(widget + TEXT, 256)?))
}
fn text_of(r: &Reader<'_>, image: u64, parent: u64, name: &str) -> Result<String> {
    text_at(r, named(r, image, parent, name)?)
}
fn label_of(r: &Reader<'_>, button: u64) -> Result<String> {
    Ok(special_projects::plain(&r.string_bounded(button + BUTTON_TEXT, 160)?))
}
fn items(r: &Reader<'_>, grid: u64) -> Result<Vec<u64>> {
    let (_, data, count) = r.array_header(grid + GRID_ITEMS)?;
    if count > 128 {
        return Err(Error::new("invalid_gui", "Grid item list exceeds bound"));
    }
    if count == 0 {
        return Ok(Vec::new());
    }
    Ok(r.read(data, count * 8)?
        .chunks_exact(8)
        .map(|b| u64::from_le_bytes(b.try_into().unwrap()))
        .collect())
}
/// First integer in a text ("Save 6" -> 6, "air_experience  50" -> 50).
pub(super) fn first_number(text: &str) -> Option<i64> {
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

// ---- production selection rows --------------------------------------------------------------
#[derive(Clone, Debug, PartialEq)]
pub(super) struct Row {
    pub address: u64,
    pub window: u64,
    pub id: i32,
    pub key: String,
    pub name: String,
    /// The row window is shown (not scrolled away by a filter).
    pub shown: bool,
    pub variant_button: Option<Rect>,
    pub obsolete_button: Option<Rect>,
    pub equipment_button: Option<Rect>,
    pub obsolete_state: i32,
    /// The design's own obsolete flag (variant +0x414).
    pub obsolete: bool,
    /// The row's equipment button is disabled (state 3: the design lacks required modules, e.g.
    /// a battery, and the game refuses to build it; tooltip "Design lacks core or more required
    /// modules", seen live on a default hull design).
    pub invalid: bool,
}
/// The selection window (army/air or ship) whose root is shown, with its rows.
pub(super) fn selection_window(r: &Reader<'_>, image: u64) -> Result<Option<u64>> {
    let view = production_view(r, image)?;
    for offset in [SELECTION_ARMY, SELECTION_NAVY] {
        let window = r.u64(view + offset)?;
        if window == 0 {
            continue;
        }
        let Some(root) = gui_tree::container(r, r.u64(window + SEL_ROOT)?, image)? else {
            continue;
        };
        if gui_tree::visible(r, root)? {
            return Ok(Some(window));
        }
    }
    Ok(None)
}
pub(super) fn selection_open(r: &Reader<'_>, image: u64) -> Result<bool> {
    Ok(selection_window(r, image)?.is_some())
}
fn button_rect(r: &Reader<'_>, image: u64, frame: u64, name: &str) -> Result<(Option<Rect>, i32)> {
    let Some(b) = gui_tree::child(r, image, frame, name)? else {
        return Ok((None, 0));
    };
    if !gui_tree::shown(r, b)? {
        return Ok((None, 0));
    }
    let state = gui_tree::button_state(r, b)?;
    Ok((Some(gui_tree::rect_unclipped(r, image, b, GAME)?), state))
}
/// The rows the open selection list actually displays, in list order: the items of its
/// `equipments_grid`. (The window also keeps a pool of earlier rows whose widgets still report
/// stale rectangles; clicking those hits whatever row now stands there.)
pub(super) fn rows(r: &Reader<'_>, image: u64) -> Result<Vec<Row>> {
    let Some(window) = selection_window(r, image)? else {
        return Ok(Vec::new());
    };
    let root = gui_tree::container(r, r.u64(window + SEL_ROOT)?, image)?
        .ok_or_else(|| Error::new("invalid_gui", "Selection window root unavailable"))?;
    let list = named_frame(r, image, root, "equipments")?;
    let grid = named(r, image, list, "equipments_grid")?;
    let mut out = Vec::new();
    for row in items(r, grid)? {
        let variant = r.u64(row + ROW_VARIANT)?;
        if variant == 0 || !fleetmem::is_object(r, variant, fleetmem::VARIANT_VTABLE, image)? {
            continue;
        }
        let v = fleetmem::variant(r, image, variant)?;
        let Some(frame) = gui_tree::container(r, r.u64(row + ROW_CONTAINER)?, image)? else {
            continue;
        };
        let shown = gui_tree::visible(r, frame)?;
        let (variant_button, _) = button_rect(r, image, frame, "variant")?;
        let (obsolete_button, obsolete_state) = button_rect(r, image, frame, "obsolete")?;
        let (equipment_button, equipment_state) = button_rect(r, image, frame, "equipment_button")?;
        // The game disables the picture button of a design it will not build (state 3).
        let invalid = equipment_state == 3;
        out.push(Row {
            address: variant,
            window,
            id: v.id,
            key: v.type_key,
            name: v.name,
            shown,
            variant_button,
            obsolete_button,
            equipment_button,
            obsolete_state,
            obsolete: v.obsolete,
            invalid,
        });
    }
    Ok(out)
}

// ---- which designer is open -----------------------------------------------------------------
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Kind {
    /// Upgrade designer ("Create Variant" window: levels per upgrade row).
    Upgrade,
    Ship,
    Tank,
    Plane,
}
impl Kind {
    pub(super) fn name(self) -> &'static str {
        match self {
            Kind::Upgrade => "upgrade",
            Kind::Ship => "ship_modules",
            Kind::Tank => "tank_modules",
            Kind::Plane => "plane_modules",
        }
    }
}
fn udw_roots(r: &Reader<'_>, image: u64) -> Result<Vec<(u64, u64)>> {
    let view = production_view(r, image)?;
    let mut out = Vec::new();
    for offset in [SELECTION_ARMY, SELECTION_NAVY] {
        let window = r.u64(view + offset)?;
        if window == 0 {
            continue;
        }
        let udw = r.u64(window + SEL_UDW)?;
        if udw == 0 {
            continue;
        }
        if let Some(root) = gui_tree::container(r, r.u64(udw + UDW_ROOT)?, image)? {
            out.push((udw, root));
        }
    }
    Ok(out)
}
fn edv(r: &Reader<'_>, image: u64, slot: u64) -> Result<(u64, u64)> {
    let view = r.u64(manager(r, image)? + slot)?;
    r.object_type(view, "CEquipmentDesignerView")?;
    let root = gui_tree::container(r, r.u64(view + EDV_ROOT)?, image)?
        .ok_or_else(|| Error::new("invalid_gui", "Equipment designer window unavailable"))?;
    Ok((view, root))
}
/// The shown designer: kind, its view/window object and its root window.
pub(super) fn open_designer(r: &Reader<'_>, image: u64) -> Result<Option<(Kind, u64, u64)>> {
    for (udw, root) in udw_roots(r, image)? {
        if gui_tree::visible(r, root)? {
            return Ok(Some((Kind::Upgrade, udw, root)));
        }
    }
    for (slot, kind) in [(EDV_SHIP, Kind::Ship), (EDV_TANK, Kind::Tank), (EDV_PLANE, Kind::Plane)] {
        let (view, root) = edv(r, image, slot)?;
        if gui_tree::visible(r, root)? {
            return Ok(Some((kind, view, root)));
        }
    }
    Ok(None)
}
pub(super) fn designer_open(r: &Reader<'_>, image: u64) -> Result<bool> {
    Ok(open_designer(r, image)?.is_some())
}

// ---- the upgrade designer --------------------------------------------------------------------
#[derive(Clone, Debug, PartialEq)]
pub(super) struct UpgradeRow {
    /// GUI window name, e.g. `tank_gun_upgrade`.
    pub window: String,
    pub label: String,
    pub level: i64,
    pub decrease: Rect,
    pub increase: Rect,
    pub decrease_disabled: bool,
    pub increase_disabled: bool,
}
#[derive(Clone, Debug, PartialEq)]
pub(super) struct UpgradeState {
    pub title: String,
    pub name: String,
    pub experience: Option<i64>,
    pub save_label: String,
    pub save_cost: Option<i64>,
    pub save_enabled: bool,
    pub save: Rect,
    pub close: Rect,
    pub rows: Vec<UpgradeRow>,
    pub draft: u64,
    pub base: u64,
    pub total_levels: i64,
    pub stats: Map<String, Value>,
    pub base_stats: Map<String, Value>,
}
pub(super) fn upgrade_state(r: &Reader<'_>, image: u64) -> Result<UpgradeState> {
    let Some((Kind::Upgrade, udw, root)) = open_designer(r, image)? else {
        return Err(Error::new("designer_closed", "The Create Variant window is not open"));
    };
    let active = named_frame(r, image, root, "active_equipment_view")?;
    let mut rows = Vec::new();
    for view in gui_tree::children(r, active)? {
        let Some(view) = gui_tree::container(r, view, image)? else { continue };
        if !gui_tree::visible(r, view)? {
            continue;
        }
        for child in gui_tree::children(r, view)? {
            let Some(name) = gui_tree::name(r, child, image)? else { continue };
            if !name.ends_with("_upgrade") {
                continue;
            }
            let Some(frame) = gui_tree::container(r, child, image)? else { continue };
            if !gui_tree::visible(r, frame)? {
                continue;
            }
            let dec = named(r, image, frame, "decrease_level")?;
            let inc = named(r, image, frame, "increase_level")?;
            rows.push(UpgradeRow {
                window: name,
                label: text_of(r, image, frame, "label")?,
                level: first_number(&text_of(r, image, frame, "level")?).unwrap_or(0),
                decrease: gui_tree::rect(r, image, dec, GAME)?,
                increase: gui_tree::rect(r, image, inc, GAME)?,
                decrease_disabled: gui_tree::disabled(r, dec)?,
                increase_disabled: gui_tree::disabled(r, inc)?,
            });
        }
    }
    let save = named(r, image, root, "save_button")?;
    let save_label = label_of(r, save)?;
    let edit = named(r, image, root, "variant_name")?;
    let view = r.u64(udw + UDW_VIEW)?;
    let draft = r.u64(view + UDV_DRAFT)?;
    let base = r.u64(view + UDV_BASE)?;
    fleetmem::expect(r, draft, fleetmem::VARIANT_VTABLE, image, "Draft variant")?;
    fleetmem::expect(r, base, fleetmem::VARIANT_VTABLE, image, "Base variant")?;
    let _ = UDV_COST;
    Ok(UpgradeState {
        title: text_of(r, image, root, "equipment_variant_label").unwrap_or_default(),
        name: r.string_bounded(edit + UDW_EDIT_TEXT, 160)?,
        experience: first_number(&text_of(r, image, root, "experience")?),
        save_cost: first_number(save_label.trim_start_matches(|c: char| !c.is_ascii_digit())),
        save_label,
        save_enabled: !gui_tree::disabled(r, save)?,
        save: gui_tree::rect(r, image, save, GAME)?,
        close: gui_tree::rect(r, image, named(r, image, root, "close_button")?, GAME)?,
        total_levels: rows.iter().map(|x| x.level).sum(),
        rows,
        draft,
        base,
        stats: fleetmem::stats(r, draft)?,
        base_stats: fleetmem::stats(r, base)?,
    })
}

// ---- the module designer (ship) ----------------------------------------------------------------
#[derive(Clone, Debug, PartialEq)]
pub(super) struct SlotView {
    /// None for a locked slot (the view exists but has no definition).
    pub name: Option<String>,
    pub id: i32,
    pub fixed: bool,
    pub button: Option<Rect>,
}
#[derive(Clone, Debug, PartialEq)]
pub(super) struct EntryView {
    pub kind: EntryKind,
    pub name: String,
    pub cost: String,
    pub module: Option<String>,
    pub click: Rect,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum EntryKind {
    Removal,
    Category,
    Module,
}
#[derive(Clone, Debug, PartialEq)]
pub(super) struct SelectorView {
    pub slot_title: String,
    pub back: Rect,
    pub entries: Vec<EntryView>,
}
#[derive(Clone, Debug, PartialEq)]
pub(super) struct ModuleState {
    pub kind: Kind,
    pub name: String,
    pub experience: Option<i64>,
    pub save_label: String,
    pub save_cost: Option<i64>,
    pub save_enabled: bool,
    pub save: Rect,
    pub close: Rect,
    pub reset: Rect,
    pub duplicate: Rect,
    pub duplicate_state: i32,
    /// Click point inside the name edit box (offset from the Close button; the window has a fixed size).
    pub name_point: (f64, f64),
    pub refit_cost: String,
    pub production_cost: String,
    pub draft: u64,
    pub type_key: String,
    pub modules: Vec<(String, String)>,
    pub slots: Vec<SlotView>,
    pub selector: Option<SelectorView>,
    pub stats: Map<String, Value>,
}
fn entry_kind(r: &Reader<'_>, image: u64, item: u64) -> Result<Option<EntryKind>> {
    let _ = VT_ENTRY_REMOVAL;
    let name = r.u64(item).ok().and_then(|vt| rtti_name(r, vt, image));
    Ok(match name.as_deref() {
        Some(n) if n.contains("ModuleRemovalEntry") => Some(EntryKind::Removal),
        Some(n) if n.contains("ModuleCategoryEntry") => Some(EntryKind::Category),
        Some(n) if n.contains("EquipmentModuleEntry") => Some(EntryKind::Module),
        _ => None,
    })
}
/// RTTI class name of an object from its vtable (typeinfo at vtable-8, name at typeinfo+8).
fn rtti_name(r: &Reader<'_>, vtable: u64, image: u64) -> Option<String> {
    if !(image..image + 0x400_0000).contains(&vtable) {
        return None;
    }
    let info = r.u64(vtable - 8).ok()?;
    if !(image..image + 0x400_0000).contains(&info) {
        return None;
    }
    let name = r.u64(info + 8).ok()? & 0x7fff_ffff_ffff_ffff;
    if !(image..image + 0x400_0000).contains(&name) {
        return None;
    }
    let bytes = r.read(name, 64).ok()?;
    let end = bytes.iter().position(|b| *b == 0)?;
    String::from_utf8(bytes[..end].to_vec()).ok()
}
fn selector(r: &Reader<'_>, image: u64, root: u64) -> Result<Option<SelectorView>> {
    let window = named_frame(r, image, root, "module_selector_window")?;
    if !gui_tree::visible(r, window)? {
        return Ok(None);
    }
    let grid = named(r, image, window, "gb_modules")?;
    let mut entries = Vec::new();
    for item in items(r, grid)? {
        let Some(kind) = entry_kind(r, image, item)? else { continue };
        let Some(frame) = gui_tree::container(r, r.u64(item + ITEM_CONTAINER)?, image)? else { continue };
        if !gui_tree::visible(r, frame)? {
            continue;
        }
        let name_box = named(r, image, frame, "name")?;
        let cost = gui_tree::child(r, image, frame, "cost")?
            .map(|c| text_at(r, c))
            .transpose()?
            .unwrap_or_default();
        let module = if kind == EntryKind::Module {
            let m = r.u64(item + ENTRY_MODULE)?;
            if m == 0 {
                None
            } else {
                Some(r.string_bounded(m + MODULE_KEY, 128)?)
            }
        } else {
            None
        };
        entries.push(EntryView {
            kind,
            name: text_at(r, name_box)?,
            cost,
            module,
            click: gui_tree::rect(r, image, named(r, image, frame, "equipment_icon")?, GAME)?,
        });
    }
    Ok(Some(SelectorView {
        slot_title: text_of(r, image, window, "slot_name")?,
        back: gui_tree::rect(r, image, named(r, image, window, "btn_back")?, GAME)?,
        entries,
    }))
}
pub(super) fn module_state(r: &Reader<'_>, image: u64) -> Result<ModuleState> {
    let (kind, view, root) = match open_designer(r, image)? {
        Some((k @ (Kind::Ship | Kind::Tank | Kind::Plane), v, root)) => (k, v, root),
        _ => return Err(Error::new("designer_closed", "No module designer is open")),
    };
    let draft = r.u64(view + EDV_DRAFT)?;
    fleetmem::expect(r, draft, fleetmem::VARIANT_VTABLE, image, "Draft variant")?;
    let v = fleetmem::variant(r, image, draft)?;
    // Slot views of a previous hull can linger in the arrays until the window is rebuilt; only
    // slots of the draft's own equipment type count.
    let table = fleetmem::type_slots(r, draft)?;
    let mut slots = Vec::new();
    for (array, fixed) in [(EDV_SLOTS_CUSTOM, false), (EDV_SLOTS_FIXED, true)] {
        let (_, data, count) = r.array_header(view + array)?;
        for index in 0..count.min(16) as u64 {
            let slot_view = r.u64(data + index * 8)?;
            let def = r.u64(slot_view + SLOT_DEF)?;
            let button = r.u64(slot_view + SLOT_BUTTON)?;
            let (name, id) = if def == 0 {
                (None, 0)
            } else {
                let name = r.string_bounded(def + SLOT_NAME, 128)?;
                if !table.iter().any(|(_, n)| *n == name) {
                    continue;
                }
                (Some(name), r.i32(def + SLOT_ID)?)
            };
            let rect = if name.is_some() && button != 0 && gui_tree::shown(r, button)? {
                gui_tree::rect(r, image, button, GAME).ok()
            } else {
                None
            };
            slots.push(SlotView {
                name,
                id,
                fixed,
                button: rect,
            });
        }
    }
    let save = named(r, image, root, "save_button")?;
    let save_label = label_of(r, save)?;
    let dup = named(r, image, root, "duplicate_button")?;
    let edit = named(r, image, root, "equipment_names")?;
    let edit_box = r.u64(view + EDV_EDIT_BOX)?;
    let _ = edit;
    Ok(ModuleState {
        kind,
        name: r.string_bounded(edit_box + EDV_EDIT_TEXT, 160)?,
        experience: first_number(&text_of(r, image, root, "experience")?),
        save_cost: first_number(save_label.trim_start_matches(|c: char| !c.is_ascii_digit())),
        save_label,
        save_enabled: !gui_tree::disabled(r, save)?,
        save: gui_tree::rect(r, image, save, GAME)?,
        close: gui_tree::rect(r, image, named(r, image, root, "close_button")?, GAME)?,
        reset: gui_tree::rect(r, image, named(r, image, root, "reset_button")?, GAME)?,
        duplicate: gui_tree::rect(r, image, dup, GAME)?,
        duplicate_state: gui_tree::button_state(r, dup)?,
        name_point: {
            let (x, y) = gui_tree::rect(r, image, named(r, image, root, "close_button")?, GAME)?.center();
            (x - 963.0, y + 42.0)
        },
        refit_cost: text_of(r, image, root, "refit_cost").unwrap_or_default(),
        production_cost: text_of(r, image, root, "production_value").unwrap_or_default(),
        draft,
        type_key: v.type_key,
        modules: fleetmem::modules(r, draft)?,
        slots,
        selector: selector(r, image, root)?,
        stats: fleetmem::stats(r, draft)?,
    })
}

// ---- driving the panel -------------------------------------------------------------------------
fn click(keys: &MacKeys, rect: Rect) -> Result<()> {
    let (x, y) = rect.center();
    keys.click(GAME, x, y).map(|_| ())
}
pub(super) fn click_rect(keys: &MacKeys, rect: Rect) -> Result<()> {
    click(keys, rect)
}
/// Rows fully inside the visible list (the ship list sits below a class filter header).
pub(super) fn in_list(rect: &Rect, category: &str) -> bool {
    let top = if category == "naval_button" { 250 } else { 170 };
    rect.y >= top && rect.y + rect.h <= 1040
}

/// Category button order that can list a design of this coarse kind.
pub(super) fn categories_for(kind: &str) -> &'static [&'static str] {
    match kind {
        "ship" => &["naval_button"],
        "plane" => &["aircraft_button"],
        "tank" => &["tank_button", "infantry_button"],
        _ => &["infantry_button", "tank_button"],
    }
}
/// Row of the design with this id in the open selection list.
fn row_of(r: &Reader<'_>, image: u64, id: i32) -> Result<Option<Row>> {
    Ok(rows(r, image)?.into_iter().find(|x| x.id == id && x.shown))
}
/// What `locate` found: the category button whose list now shows the row in view.
pub(super) struct Located {
    pub category: &'static str,
    /// "Show outdated equipment" is ticked in the open list (the game keeps the filter between
    /// openings, and it changes which design the line-add list offers first): callers untick it
    /// with `untick_outdated` before they leave.
    pub outdated_ticked: bool,
    /// The filter was ticked by this call to reach a hidden row.
    pub ticked_by_us: bool,
}
// 1.19.4: CButtonStandard fields past 0xb8 +8 (live: an unticked box reads 1 at +0x1c0).
const CHECKBOX_STATE: u64 = 0x1c0;
fn outdated_checkbox_button(r: &Reader<'_>, image: u64) -> Result<u64> {
    let Some(window) = selection_window(r, image)? else {
        return Err(Error::new("gui_target_unavailable", "No selection list is open"));
    };
    let root = gui_tree::container(r, r.u64(window + SEL_ROOT)?, image)?
        .ok_or_else(|| Error::new("invalid_gui", "Selection window root unavailable"))?;
    named(r, image, root, "checkbox")
}
fn outdated_checkbox(r: &Reader<'_>, image: u64) -> Result<Rect> {
    gui_tree::rect(r, image, outdated_checkbox_button(r, image)?, GAME)
}
/// The filter's checked state (button +0x1b8: 2 ticked, 1 empty; flipped by clicks on 26 Oct 2026).
pub(super) fn outdated_ticked(r: &Reader<'_>, image: u64) -> Result<bool> {
    Ok(r.i32(outdated_checkbox_button(r, image)? + CHECKBOX_STATE)? == 2)
}
fn set_outdated(process: &dyn Process, keys: &MacKeys, want: bool) -> Result<()> {
    if read(process, outdated_ticked)? == want {
        return Ok(());
    }
    // 1.19.4: the selection list slides in from the left; right after it opens the box's sprite
    // still sits ~370 px left of where it is drawn (read live 31 October 2026: x 322 vs 679), and
    // a click there misses. Let the slide finish before the rectangle is read.
    std::thread::sleep(Duration::from_millis(600));
    let rect = settled(process, "outdated box", outdated_checkbox)?;
    click(keys, rect)?;
    wait(process, "outdated filter", |r, i| Ok(outdated_ticked(r, i)? == want))?;
    let _ = settled(process, "outdated rows", |r, i| {
        Ok(rows(r, i)?.iter().map(|x| x.id).collect::<Vec<_>>())
    });
    Ok(())
}
/// Untick "Show outdated equipment" again (the open list must be the one it was ticked in).
pub(super) fn untick_outdated(process: &dyn Process, keys: &MacKeys) -> Result<()> {
    set_outdated(process, keys, false)
}
/// Open the Production panel's equipment list that shows the design with this id and scroll
/// its row into view. Designs marked outdated stay hidden unless "Show outdated equipment" is
/// ticked; `show_outdated` allows the call to tick it (reported in the result).
pub(super) fn locate(
    process: &dyn Process,
    keys: &MacKeys,
    id: i32,
    kind: &str,
    show_outdated: bool,
) -> Result<Located> {
    super::ui::clear_notices(process, keys)?;
    super::production::ensure_open(process, keys)?;
    if read(process, designer_open)? {
        close(process, keys)?;
    }
    let mut seen: Vec<String> = Vec::new();
    let mut debug: Vec<String> = Vec::new();
    for category in categories_for(kind) {
        if read(process, selection_open)? {
            keys.press(Key::Escape)?;
            wait(process, "close selection", |r, i| Ok(!selection_open(r, i)?))?;
            super::production::ensure_open(process, keys)?;
        }
        let rect = settled(process, "category", |r, i| {
            super::production::category_rect(r, i, category)
        })?;
        click(keys, rect)?;
        wait(process, "open selection", selection_open)?;
        // The list slides in; wait until its rows report their buttons.
        let _ = wait(process, "selection rows", |r, i| {
            Ok(rows(r, i)?.iter().any(|x| x.shown && x.variant_button.is_some()))
        });
        let _ = settled(process, "selection rows", |r, i| {
            Ok(rows(r, i)?
                .iter()
                .map(|x| (x.id, x.shown, x.variant_button))
                .collect::<Vec<_>>())
        });
        let mut ticked_by_us = false;
        if show_outdated
            && read(process, |r, i| row_of(r, i, id))?.is_none()
            && !read(process, outdated_ticked)?
        {
            set_outdated(process, keys, true)?;
            ticked_by_us = true;
        }
        let mut last_y: Option<i32> = None;
        for _ in 0..40 {
            let list = read(process, rows)?;
            for r in &list {
                if r.shown && !seen.contains(&r.key) {
                    seen.push(r.key.clone());
                }
            }
            let Some(row) = list.iter().find(|x| x.id == id && x.shown) else {
                debug.push(format!("{category}: row {id} not listed"));
                break;
            };
            let Some(button) = row.variant_button.or(row.obsolete_button) else {
                debug.push(format!("{category}: row {id} has no buttons"));
                break;
            };
            if in_list(&button, category) {
                return Ok(Located {
                    category,
                    outdated_ticked: read(process, outdated_ticked)?,
                    ticked_by_us,
                });
            }
            if last_y == Some(button.y) {
                debug.push(format!("{category}: row {id} cannot be scrolled into view"));
                break;
            }
            last_y = Some(button.y);
            let pixels = if button.y < 170 { 240 } else { -240 };
            keys.scroll(GAME, (780.0, 700.0), pixels)?;
            std::thread::sleep(Duration::from_millis(250));
        }
        if ticked_by_us {
            // Wrong list: put the filter back before trying the next category.
            set_outdated(process, keys, false)?;
        }
    }
    if read(process, selection_open)? {
        keys.press(Key::Escape)?;
    }
    Err(Error::new(
        "design_not_offered",
        format!(
            "The Production panel lists no design {id}; offered types: {} ({})",
            seen.join(", "),
            debug.join("; ")
        ),
    ))
}
/// Open the designer of the design with this id: `locate`, then the row's Create variant button.
pub(super) fn open_for(
    process: &dyn Process,
    keys: &MacKeys,
    id: i32,
    kind: &str,
) -> Result<Kind> {
    // A base that became outdated (older version of a newer design) is only listed when the
    // outdated filter is ticked; `leave` puts the filter back.
    locate(process, keys, id, kind, true)?;
    let b = settled(process, "variant button", |r, i| {
        row_of(r, i, id)?
            .and_then(|x| x.variant_button)
            .ok_or_else(|| Error::new("gui_target_unavailable", "Create variant button not found"))
    })?;
    click(keys, b)?;
    wait(process, "designer opens", designer_open)?;
    let opened = read(process, open_designer)?
        .map(|(k, _, _)| k)
        .ok_or_else(|| Error::new("input_readback_failed", "Designer did not open"))?;
    if opened != Kind::Upgrade {
        // The slot views are rebuilt while the window fades in.
        let _ = settled(process, "designer layout", |r, i| {
            let s = module_state(r, i)?;
            Ok((s.type_key, s.slots, s.modules))
        });
    }
    Ok(opened)
}
/// Click the row's Decommission/Recommission button (the row must be located). The click is
/// refused at the last moment when the row's button is no longer where it was read, and the
/// flags of every other listed design must be unchanged afterwards.
pub(super) fn click_obsolete(process: &dyn Process, keys: &MacKeys, id: i32, want: bool) -> Result<()> {
    let b = settled(process, "obsolete button", |r, i| {
        row_of(r, i, id)?
            .and_then(|x| x.obsolete_button)
            .ok_or_else(|| Error::new("gui_target_unavailable", "Decommission button not found"))
    })?;
    let others: Vec<(i32, bool)> = read(process, rows)?
        .iter()
        .filter(|x| x.id != id)
        .map(|x| (x.id, x.obsolete))
        .collect();
    let (x, y) = b.center();
    keys.click_checked(GAME, x, y, &|| {
        let now = read(process, |r, i| row_of(r, i, id))?
            .and_then(|x| x.obsolete_button);
        if now == Some(b) {
            Ok(())
        } else {
            Err(Error::new(
                "gui_target_unsettled",
                "The row moved before the click; nothing was clicked",
            ))
        }
    })?;
    wait(process, "obsolete flag", |r, i| {
        Ok(row_of(r, i, id)?.is_none_or(|x| x.obsolete == want))
    })?;
    let after = read(process, rows)?;
    for (other, flag) in others {
        if after.iter().any(|x| x.id == other && x.obsolete != flag) {
            return Err(Error::new(
                "input_readback_failed",
                format!("Design {other} changed its obsolete flag too; check hoi4_designs and repair it with hoi4_design_obsolete"),
            ));
        }
    }
    Ok(())
}
/// The equipment picture of a design row (used to pick it in the line change list).
pub(super) fn row_equipment_button(r: &Reader<'_>, image: u64, id: i32) -> Result<Option<Rect>> {
    Ok(row_of(r, image, id)?.and_then(|x| x.equipment_button))
}

/// Close the designer (Escape drops the draft, nothing is saved), then the selection list.
pub(super) fn close(process: &dyn Process, keys: &MacKeys) -> Result<()> {
    for _ in 0..4 {
        if !read(process, designer_open)? {
            break;
        }
        // A module selector open inside the ship designer is closed by Back first.
        keys.press(Key::Escape)?;
        let deadline = Instant::now() + Duration::from_secs(2);
        while Instant::now() < deadline && read(process, designer_open)? {
            std::thread::sleep(Duration::from_millis(60));
        }
    }
    if read(process, designer_open)? {
        return Err(Error::new("input_readback_failed", "The designer did not close"));
    }
    Ok(())
}
/// Close the designer, the selection list and the Production panel if they are open.
pub(super) fn leave(process: &dyn Process, keys: &MacKeys) {
    let _ = close(process, keys);
    for _ in 0..2 {
        if read(process, selection_open).unwrap_or(false) {
            if read(process, outdated_ticked).unwrap_or(false) {
                let _ = untick_outdated(process, keys);
            }
            let _ = keys.press(Key::Escape);
            let _ = wait(process, "close selection", |r, i| Ok(!selection_open(r, i)?));
        }
    }
    if read(process, super::production::open).unwrap_or(false) {
        let _ = keys.press(Key::Escape);
        let _ = wait(process, "close production", |r, i| Ok(!super::production::open(r, i)?));
    }
}

#[allow(dead_code)]
pub(super) fn state_json(process: &dyn Process) -> Result<Value> {
    read(process, |r, i| {
        Ok(match open_designer(r, i)? {
            Some((Kind::Upgrade, _, _)) => {
                let s = upgrade_state(r, i)?;
                json!({"designer":"upgrade","name":s.name,"experience":s.experience,"save":s.save_label,"rows":s.rows.iter().map(|x| json!({"window":x.window,"label":x.label,"level":x.level})).collect::<Vec<_>>(),"stats":s.stats})
            }
            Some((k, _, _)) => {
                let s = module_state(r, i)?;
                json!({"designer":k.name(),"name":s.name,"experience":s.experience,"save":s.save_label,"type":s.type_key,"modules":s.modules,"slots":s.slots.len()})
            }
            None => json!({"designer":null}),
        })
    })
}
