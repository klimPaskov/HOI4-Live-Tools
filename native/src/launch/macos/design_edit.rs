//! Equipment design management with ordinary input: read what a designer offers, create a new
//! variant (tank/plane upgrades or ship modules), decommission a variant and retarget a
//! production line, all through the Production panel and the game's own designer windows.
//!
//! Every request is validated from memory before any input (own designs, module slot tables,
//! names); every click is verified against the designer's working copy; the saved result is
//! verified against the own design list (`hoi4_designs` source). The designers spend the
//! matching branch experience (army, air, navy) when a variant is saved; it never costs
//! political power.
use super::{
    design_ui::{self as dui, EntryKind, Kind},
    designer,
    designs::{self, Design},
    fleetmem,
    settle::Verdict,
    ui::{read, settled, wait},
};
use crate::{
    error::{Error, Result},
    gui_tree::Rect,
    platform::{
        macos_input::{Key, MacKeys},
        Process,
    },
};
use serde_json::{json, Map, Value};
use std::{collections::BTreeMap, thread, time::Duration};

/// Longest design name accepted (the game's boxes take more; the tool stays short).
pub(super) const MAX_NAME: usize = 40;
/// Upper bound of any upgrade level asked for.
pub(super) const MAX_LEVEL: i64 = 20;

pub(super) fn options_tool() -> Value {
    json!({"name":"hoi4_design_options","description":"What the equipment designers offer, read from the game's own designer windows (Production panel, equipment selection list, the row's Create variant button; nothing is saved and no experience is spent, the window is closed again). Without `base`: the own designs of that kind that can be a base (design_id, type, name, designer: upgrade or modules) and the held army/air/navy experience, read from memory with no input. With `base` (design_id, or an exact design name or equipment type key): the designer opens on it. Tank/plane without modules: the upgrade rows (key, label, level, experience per level measured by one step up and down, step limit) and the base stats. Ship: every module slot (locked or open, fixed or custom) with its current module and, per slot, the categories and unlocked modules the selector offers (module key, game name, experience cost text). `slot` limits the scan to one slot.","inputSchema":{"type":"object","properties":{"kind":{"type":"string","enum":["tank","plane","ship","other"]},"base":{"type":["integer","string"]},"slot":{"type":"string","maxLength":64},"measure_costs":{"type":"boolean"},"lease_token":{"type":"string"}},"required":["kind","lease_token"],"additionalProperties":false}})
}
pub(super) fn create_tool() -> Value {
    json!({"name":"hoi4_design_create","description":"Create a new own equipment variant through the game's designer: Production panel, equipment selection list, the base design's Create variant button, then the designer's own controls, name box and Save. `kind` tank|plane|other (equipment without module slots) takes `upgrades` {row: level} (the designer's upgrade rows, e.g. armor, reliability, gun, engine, range; absolute levels 0..20, listed by hoi4_design_options); `kind` ship takes `modules` {slot: module_key or null to empty the slot} (slot names and module keys as in hoi4_designs detail/hoi4_design_options; only the slots named are changed, the rest keep the base design's modules). `base` is a design id or an exact design name/type key. Saving a custom (named) ship design uses the designer's Save As New so the original design and its production lines stay unchanged. Spends army/air/navy experience read from the window and returned. Refused before any input: kind_mismatch, base_unknown/base_ambiguous (candidates listed), design_not_offered, slot_unknown (slots listed), name_taken, name_not_typable, nothing_to_change, designer_unsupported. Found while building (nothing saved, the draft is dropped): upgrade_unknown, upgrade_level_max, slot_locked, module_unavailable (unlocked modules listed), insufficient_experience. Verified when exactly one new own design with the requested name and exactly the requested modules (or the designer's computed stats) is listed by hoi4_designs; the receipt carries that row. Use the returned design_id with hoi4_production_add `design`.","inputSchema":{"type":"object","properties":{"kind":{"type":"string","enum":["tank","plane","ship","other"]},"base":{"type":["integer","string"]},"upgrades":{"type":"object","additionalProperties":{"type":"integer","minimum":0,"maximum":20}},"modules":{"type":"object","additionalProperties":{"type":["string","null"],"maxLength":96}},"name":{"type":"string","maxLength":40},"request_id":{"type":"string"},"lease_token":{"type":"string"}},"required":["kind","base","request_id","lease_token"],"additionalProperties":false}})
}
pub(super) fn obsolete_tool() -> Value {
    json!({"name":"hoi4_design_obsolete","description":"Decommission (obsolete: true, default) or recommission (obsolete: false) an own equipment variant with the Production selection list's Decommission/Recommission button: a decommissioned variant is marked outdated equipment and hidden from the lists unless Show outdated equipment is ticked (the tool ticks and restores that filter itself when it must reach a hidden row). `design` is the design id from hoi4_designs. Refused before input: design_unknown, already_obsolete, not_obsolete. Verified from the design's obsolete flag in memory (hoi4_designs `obsolete`).","inputSchema":{"type":"object","properties":{"design":{"type":"integer","minimum":1},"obsolete":{"type":"boolean"},"request_id":{"type":"string"},"lease_token":{"type":"string"}},"required":["design","request_id","lease_token"],"additionalProperties":false}})
}
pub(super) fn set_production_tool() -> Value {
    json!({"name":"hoi4_design_set_production","description":"Change which design an existing own production line builds: click the line's equipment picture, then the wanted design in the selection list (ordinary UI, same category). The game warns that progress of the line is lost; pass accept_progress_loss: true to confirm. `line` is the line handle from hoi4_production_lines/hoi4_designs production_lines. Refused before input: line_unknown, design_unknown, design_wrong_type (a line only switches inside its equipment family as the game lists it), already_building, progress_loss_unconfirmed. Verified when the design's production_lines contain the line handle.","inputSchema":{"type":"object","properties":{"line":{"type":"integer","minimum":1},"design":{"type":"integer","minimum":1},"accept_progress_loss":{"type":"boolean"},"request_id":{"type":"string"},"lease_token":{"type":"string"}},"required":["line","design","request_id","lease_token"],"additionalProperties":false}})
}

// ---- arguments -------------------------------------------------------------------------------
pub(super) fn kind_arg(args: &Map<String, Value>) -> Result<&'static str> {
    match args.get("kind").and_then(Value::as_str) {
        Some("tank") => Ok("tank"),
        Some("plane") => Ok("plane"),
        Some("ship") => Ok("ship"),
        Some("other") => Ok("other"),
        _ => Err(Error::new("invalid_arguments", "kind must be tank, plane, ship or other")),
    }
}
#[derive(Clone, Debug, PartialEq)]
pub(super) enum BaseRef {
    Id(i32),
    Name(String),
}
pub(super) fn base_arg(v: &Value) -> Result<BaseRef> {
    if let Some(id) = v.as_i64() {
        return i32::try_from(id)
            .ok()
            .filter(|i| *i > 0)
            .map(BaseRef::Id)
            .ok_or_else(|| Error::new("invalid_arguments", "base id out of range"));
    }
    match v.as_str() {
        Some(s) if !s.is_empty() && s.len() <= 96 && !s.chars().any(char::is_control) => {
            Ok(BaseRef::Name(s.to_string()))
        }
        _ => Err(Error::new("invalid_arguments", "base must be a design id or a name")),
    }
}
/// `[a-z0-9_ ]{1,40}` upgrade row keys with levels 0..=MAX_LEVEL.
pub(super) fn upgrades_arg(v: &Value) -> Result<BTreeMap<String, i64>> {
    let o = v
        .as_object()
        .filter(|o| o.len() <= 12)
        .ok_or_else(|| Error::new("invalid_arguments", "upgrades must be an object of row: level"))?;
    let mut out = BTreeMap::new();
    for (k, v) in o {
        let level = v
            .as_i64()
            .filter(|l| (0..=MAX_LEVEL).contains(l))
            .ok_or_else(|| Error::new("invalid_arguments", format!("level of {k} must be 0..{MAX_LEVEL}")))?;
        if k.is_empty() || k.len() > 40 || !k.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b' ' || b == b'-') {
            return Err(Error::new("invalid_arguments", format!("Invalid upgrade key {k:?}")));
        }
        out.insert(norm_key(k), level);
    }
    Ok(out)
}
/// slot -> module key (None empties the slot).
pub(super) fn modules_arg(v: &Value) -> Result<BTreeMap<String, Option<String>>> {
    let o = v
        .as_object()
        .filter(|o| o.len() <= 16)
        .ok_or_else(|| Error::new("invalid_arguments", "modules must be an object of slot: module"))?;
    let mut out = BTreeMap::new();
    for (slot, m) in o {
        if slot.is_empty() || slot.len() > 64 || !slot.bytes().all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_') {
            return Err(Error::new("invalid_arguments", format!("Invalid slot name {slot:?}")));
        }
        let module = match m {
            Value::Null => None,
            Value::String(s) if s.is_empty() || s == "none" || s == "empty" => None,
            Value::String(s) => {
                if s.len() > 96 || !s.bytes().all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_') {
                    return Err(Error::new("invalid_arguments", format!("Invalid module key {s:?}")));
                }
                Some(s.clone())
            }
            _ => return Err(Error::new("invalid_arguments", "a module is a key string or null")),
        };
        out.insert(slot.clone(), module);
    }
    Ok(out)
}
pub(super) fn name_arg(v: Option<&Value>) -> Result<Option<String>> {
    let Some(v) = v else { return Ok(None) };
    let s = v
        .as_str()
        .ok_or_else(|| Error::new("invalid_arguments", "name must be a string"))?;
    check_name(s)?;
    Ok(Some(s.to_string()))
}
pub(super) fn check_name(s: &str) -> Result<()> {
    if s.trim() != s || s.is_empty() || s.chars().count() > MAX_NAME {
        return Err(Error::new(
            "invalid_arguments",
            format!("Design names are 1..{MAX_NAME} characters without leading or trailing space"),
        ));
    }
    if !designer::typable(s) {
        return Err(Error::new(
            "name_not_typable",
            "Design names may use letters, digits, space and - _ . , / ' ( ) only",
        ));
    }
    Ok(())
}
fn norm_key(k: &str) -> String {
    k.trim().to_lowercase().replace([' ', '-'], "_")
}

// ---- pure planning ------------------------------------------------------------------------------
/// The own design a request names, with the kind check.
pub(super) fn resolve_base<'a>(all: &'a [Design], kind: &str, base: &BaseRef) -> Result<&'a Design> {
    let of_kind = |d: &&Design| designs::kind_of(&d.variant.type_key) == kind;
    let hit: Vec<&Design> = match base {
        BaseRef::Id(id) => all.iter().filter(|d| d.variant.id == *id).collect(),
        BaseRef::Name(n) => {
            let lower = n.to_lowercase();
            let by_name: Vec<&Design> = all
                .iter()
                .filter(|d| !d.variant.name.is_empty() && d.variant.name.to_lowercase() == lower)
                .collect();
            if by_name.is_empty() {
                all.iter()
                    .filter(|d| d.variant.parent_id.is_none() && d.variant.type_key.to_lowercase() == lower)
                    .collect()
            } else {
                by_name
            }
        }
    };
    if hit.len() > 1 {
        let ids: Vec<String> = hit.iter().map(|d| d.variant.id.to_string()).collect();
        return Err(Error::new(
            "base_ambiguous",
            format!("{} own designs match; give one design id: {}", hit.len(), ids.join(", ")),
        ));
    }
    let Some(found) = hit.first().copied() else {
        let names: Vec<String> = all
            .iter()
            .filter(|d| of_kind(d))
            .map(|d| format!("{} ({})", d.variant.id, designs::display(&d.variant)))
            .collect();
        return Err(Error::new(
            "base_unknown",
            format!("No own {kind} design matches {base:?}; designs: {}", names.join(", ")),
        ));
    };
    let actual = designs::kind_of(&found.variant.type_key);
    if actual != kind {
        return Err(Error::new(
            "kind_mismatch",
            format!("Design {} is a {actual} design ({}), not {kind}", found.variant.id, found.variant.type_key),
        ));
    }
    Ok(found)
}
/// Module map the saved design must carry: the base's modules with the overrides applied.
pub(super) fn expected_modules(
    base: &[(String, String)],
    overrides: &BTreeMap<String, Option<String>>,
) -> BTreeMap<String, String> {
    let mut out: BTreeMap<String, String> = base
        .iter()
        .filter(|(_, m)| !m.is_empty())
        .cloned()
        .collect();
    for (slot, m) in overrides {
        match m {
            Some(m) => {
                out.insert(slot.clone(), m.clone());
            }
            None => {
                out.remove(slot);
            }
        }
    }
    out
}
pub(super) fn module_map(list: &[(String, String)]) -> BTreeMap<String, String> {
    list.iter().filter(|(_, m)| !m.is_empty()).cloned().collect()
}
pub(super) fn name_taken(all: &[Design], type_key: &str, name: &str) -> Result<()> {
    if all.iter().any(|d| {
        d.variant.type_key == type_key && d.variant.name.eq_ignore_ascii_case(name)
    }) {
        return Err(Error::new(
            "name_taken",
            format!("An own {type_key} design is already called {name:?}"),
        ));
    }
    Ok(())
}
/// What the saved design must look like.
#[derive(Clone, Debug, PartialEq)]
pub(super) struct Want {
    pub type_key: String,
    pub name: String,
    pub parent: i32,
    /// Ship/module designs: the exact module map.
    pub modules: Option<BTreeMap<String, String>>,
    /// Upgrade designs: the sum of the upgrade levels.
    pub upgrade_total: Option<i64>,
}
/// Settlement of a design save: designs that appeared since `before_ids` are the only effect.
pub(super) fn settle_create(before_ids: &[i32], now: &[Design], want: &Want) -> Verdict {
    // Only designs derived from the base of this equipment type can be this action's effect.
    let fresh: Vec<&Design> = now
        .iter()
        .filter(|d| {
            !before_ids.contains(&d.variant.id)
                && d.variant.type_key == want.type_key
                && d.variant.parent_id == Some(want.parent)
        })
        .collect();
    if fresh.is_empty() {
        return Verdict::Unchanged;
    }
    let exact: Vec<&&Design> = fresh.iter().filter(|d| matches_want(d, want)).collect();
    if fresh.len() == 1 && exact.len() == 1 {
        Verdict::Reached
    } else {
        Verdict::Diverged
    }
}
pub(super) fn matches_want(d: &Design, want: &Want) -> bool {
    d.variant.type_key == want.type_key
        && d.variant.name == want.name
        && d.variant.parent_id == Some(want.parent)
        && match &want.modules {
            Some(m) => &module_map(&d.modules) == m,
            None => true,
        }
        && match want.upgrade_total {
            Some(t) => d.upgrades.iter().map(|(_, l)| i64::from(*l)).sum::<i64>() == t,
            None => true,
        }
}
/// Retried request: only an exactly matching design settles it (nothing is compared otherwise).
pub(super) fn settle_retry(now: &[Design], want: &Want) -> Verdict {
    let hits = now.iter().filter(|d| matches_want(d, want)).count();
    match hits {
        1 => Verdict::Reached,
        0 => Verdict::Unchanged,
        _ => Verdict::Diverged,
    }
}
/// Upgrade row for a key: the window name without `_upgrade`, its short form (after the first
/// underscore) or the label with underscores.
pub(super) fn find_upgrade<'a>(rows: &'a [dui::UpgradeRow], key: &str) -> Option<&'a dui::UpgradeRow> {
    let key = norm_key(key);
    rows.iter().find(|r| upgrade_keys(r).contains(&key))
}
pub(super) fn upgrade_keys(r: &dui::UpgradeRow) -> Vec<String> {
    let full = r.window.trim_end_matches("_upgrade").to_string();
    let mut keys = vec![full.clone()];
    if let Some((_, rest)) = full.split_once('_') {
        keys.push(rest.to_string());
    }
    keys.push(norm_key(&r.label));
    keys
}
pub(super) fn upgrade_short(r: &dui::UpgradeRow) -> String {
    let full = r.window.trim_end_matches("_upgrade");
    full.split_once('_').map_or(full, |(_, rest)| rest).to_string()
}

// ---- reads for the pre-checks ------------------------------------------------------------------
pub(super) struct Catalogue {
    pub designs: Vec<Design>,
    /// design id -> module slot table of its type (empty without modules)
    pub slots: BTreeMap<i32, Vec<(i32, String)>>,
}
pub(super) fn catalogue(process: &dyn Process) -> Result<Catalogue> {
    read(process, |r, image| {
        let (_, production) = crate::economy::own_production(r, image)?;
        let all = designs::designs(r, image, production)?;
        let mut slots = BTreeMap::new();
        for d in &all {
            slots.insert(d.variant.id, fleetmem::type_slots(r, d.variant.address)?);
        }
        Ok(Catalogue { designs: all, slots })
    })
}
fn designer_kind(c: &Catalogue, d: &Design) -> &'static str {
    if c.slots.get(&d.variant.id).is_some_and(|s| !s.is_empty()) {
        "modules"
    } else {
        "upgrade"
    }
}
/// Branch experience held (army, air, navy) from the top-bar designer texts is not available
/// without a window; the designers report their own.
fn experience_label(kind: &str) -> &'static str {
    match kind {
        "plane" => "air",
        "ship" => "navy",
        _ => "army",
    }
}

// ---- keyboard text ---------------------------------------------------------------------------------
/// Replace the text of an edit box: click into it, Backspace everything (the first Backspace
/// proves the keyboard focus), then type the new name one key at a time, each verified. A
/// space is typed only after focus is proven, since a stray Space would toggle the pause.
pub(super) fn type_name(
    process: &dyn Process,
    keys: &MacKeys,
    point: (f64, f64),
    name: &str,
    current: &dyn Fn(&crate::memory::Reader<'_>, u64) -> Result<String>,
) -> Result<()> {
    if !designer::typable(name) {
        return Err(Error::new("name_not_typable", "Design names may use letters, digits, space and - _ . , / ' ( )"));
    }
    let text = || read(process, |r, i| current(r, i));
    let mut have = text()?;
    if have == name {
        return Ok(());
    }
    keys.click(dui::GAME, point.0, point.1)?;
    thread::sleep(Duration::from_millis(250));
    keys.press_code(119)?; // End
    let mut length = have.chars().count();
    if length > 0 {
        keys.press_code(51)?;
        length -= 1;
        wait(process, "name box focused", |r, i| Ok(current(r, i)?.chars().count() == length))
            .map_err(|_| Error::new("input_readback_failed", "The name box did not take the keyboard focus"))?;
        have = text()?;
    }
    while length > 0 {
        keys.press_code(51)?;
        length -= 1;
        wait(process, "name erased", |r, i| Ok(current(r, i)?.chars().count() == length))?;
    }
    let _ = have;
    let mut typed = String::new();
    for c in name.chars() {
        let (code, shifted) = designer::key_for(c)
            .ok_or_else(|| Error::new("name_not_typable", "key"))?;
        if shifted {
            keys.press_shifted(code)?;
        } else {
            keys.press_code(code)?;
        }
        typed.push(c);
        wait(process, "name key", |r, i| Ok(current(r, i)? == typed))?;
    }
    Ok(())
}

// ---- upgrade designer ------------------------------------------------------------------------------------
fn upgrade(r: &crate::memory::Reader<'_>, i: u64) -> Result<dui::UpgradeState> {
    dui::upgrade_state(r, i)
}
fn upgrade_name_point(s: &dui::UpgradeState) -> (f64, f64) {
    // The name edit box sits left of the Close button at a fixed offset of the fixed-size window.
    let (x, y) = s.close.center();
    (x - 250.0, y + 36.0)
}
/// Step one upgrade row to `target` with single clicks, each verified from the level text.
fn set_level(process: &dyn Process, keys: &MacKeys, window: &str, target: i64) -> Result<()> {
    for _ in 0..(MAX_LEVEL * 2 + 4) {
        let s = read(process, upgrade)?;
        let row = s
            .rows
            .iter()
            .find(|r| r.window == window)
            .ok_or_else(|| Error::new("upgrade_unknown", format!("{window} is not shown")))?
            .clone();
        if row.level == target {
            return Ok(());
        }
        let up = row.level < target;
        if (up && row.increase_disabled) || (!up && row.decrease_disabled) {
            return Err(Error::new(
                "upgrade_level_max",
                format!("{} stops at level {} (wanted {target}); nothing was saved", row.label, row.level),
            ));
        }
        let rect = settled(process, "upgrade step", |r, i| {
            upgrade(r, i)?
                .rows
                .iter()
                .find(|x| x.window == window)
                .map(|x| if up { x.increase } else { x.decrease })
                .ok_or_else(|| Error::new("gui_target_unavailable", "Upgrade row not shown"))
        })?;
        dui::click_rect(keys, rect)?;
        let before = row.level;
        wait(process, "upgrade level", |r, i| {
            Ok(upgrade(r, i)?
                .rows
                .iter()
                .find(|x| x.window == window)
                .is_some_and(|x| x.level != before))
        })?;
    }
    Err(Error::new("input_readback_failed", "Upgrade level not reached within the click bound"))
}

// ---- ship (module) designer -------------------------------------------------------------------------------------
fn module(r: &crate::memory::Reader<'_>, i: u64) -> Result<dui::ModuleState> {
    dui::module_state(r, i)
}
fn slot_button(s: &dui::ModuleState, slot: &str) -> Result<Rect> {
    let view = s
        .slots
        .iter()
        .find(|v| v.name.as_deref() == Some(slot))
        .ok_or_else(|| Error::new("slot_unknown", format!("{slot} is not a slot of this design")))?;
    view.button.ok_or_else(|| {
        Error::new("slot_locked", format!("{slot} is locked in this designer (not unlocked yet); nothing was saved"))
    })
}
fn draft_module(s: &dui::ModuleState, slot: &str) -> Option<String> {
    s.modules
        .iter()
        .find(|(n, m)| n == slot && !m.is_empty())
        .map(|(_, m)| m.clone())
}
/// Back out of the module selector to the main window.
fn selector_close(process: &dyn Process, keys: &MacKeys) -> Result<()> {
    for _ in 0..3 {
        let s = read(process, module)?;
        let Some(sel) = s.selector else { return Ok(()) };
        dui::click_rect(keys, sel.back)?;
        let _ = wait(process, "selector back", |r, i| {
            Ok(module(r, i)?.selector.as_ref() != Some(&sel))
        });
    }
    wait(process, "selector closed", |r, i| Ok(module(r, i)?.selector.is_none()))
}
/// Modules the selector lists for `slot`, by category: opens the slot, clicks each category,
/// reads, backs out. Leaves the selector closed.
pub(super) fn scan_slot(process: &dyn Process, keys: &MacKeys, slot: &str) -> Result<Vec<Value>> {
    let s = read(process, module)?;
    let button = slot_button(&s, slot)?;
    dui::click_rect(keys, button)?;
    wait(process, "module selector", |r, i| Ok(module(r, i)?.selector.is_some()))?;
    let first = read(process, module)?.selector.unwrap();
    let mut out = Vec::new();
    let categories: Vec<_> = first
        .entries
        .iter()
        .filter(|e| e.kind == EntryKind::Category)
        .map(|e| e.name.clone())
        .collect();
    // A slot whose list is already module level (single category) reports it directly.
    let direct: Vec<Value> = first
        .entries
        .iter()
        .filter(|e| e.kind == EntryKind::Module)
        .map(entry_json)
        .collect();
    if !direct.is_empty() {
        out.push(json!({"category":null,"modules":direct}));
    }
    for name in categories {
        let sel = read(process, module)?.selector;
        let Some(sel) = sel else { break };
        let Some(entry) = sel.entries.iter().find(|e| e.kind == EntryKind::Category && e.name == name) else {
            continue;
        };
        dui::click_rect(keys, entry.click)?;
        wait(process, "module list", |r, i| {
            Ok(module(r, i)?
                .selector
                .is_some_and(|s| s.entries.iter().any(|e| e.kind == EntryKind::Module)))
        })?;
        let listed = read(process, module)?.selector.unwrap();
        let mods: Vec<Value> = listed
            .entries
            .iter()
            .filter(|e| e.kind == EntryKind::Module)
            .map(entry_json)
            .collect();
        out.push(json!({"category":name,"modules":mods}));
        dui::click_rect(keys, listed.back)?;
        wait(process, "category list", |r, i| {
            Ok(module(r, i)?
                .selector
                .is_some_and(|s| s.entries.iter().any(|e| e.kind == EntryKind::Category)))
        })?;
    }
    selector_close(process, keys)?;
    Ok(out)
}
fn entry_json(e: &dui::EntryView) -> Value {
    json!({"module":e.module,"name":e.name,"cost":e.cost})
}
/// Put `module` into `slot` (None empties it) through the selector, verified from the draft.
fn set_slot(process: &dyn Process, keys: &MacKeys, slot: &str, module_key: Option<&str>) -> Result<()> {
    let s = read(process, module)?;
    let current = draft_module(&s, slot);
    if current.as_deref() == module_key {
        return Ok(());
    }
    let button = slot_button(&s, slot)?;
    dui::click_rect(keys, button)?;
    wait(process, "module selector", |r, i| Ok(module(r, i)?.selector.is_some()))?;
    let result = (|| -> Result<()> {
        let first = read(process, module)?.selector.unwrap();
        let Some(want) = module_key else {
            let removal = first
                .entries
                .iter()
                .find(|e| e.kind == EntryKind::Removal)
                .ok_or_else(|| Error::new("module_unavailable", format!("{slot} offers no Remove entry (it holds no module)")))?;
            dui::click_rect(keys, removal.click)?;
            wait(process, "module removed", |r, i| {
                let s = module(r, i)?;
                Ok(s.selector.is_none() && draft_module(&s, slot).is_none())
            })?;
            return Ok(());
        };
        let mut listed: Vec<String> = Vec::new();
        for e in &first.entries {
            if e.kind == EntryKind::Module {
                if let Some(m) = &e.module {
                    listed.push(m.clone());
                }
            }
        }
        if let Some(entry) = first
            .entries
            .iter()
            .find(|e| e.kind == EntryKind::Module && e.module.as_deref() == Some(want))
        {
            dui::click_rect(keys, entry.click)?;
            return finish_pick(process, slot, want);
        }
        let categories: Vec<String> = first
            .entries
            .iter()
            .filter(|e| e.kind == EntryKind::Category)
            .map(|e| e.name.clone())
            .collect();
        for name in categories {
            let sel = read(process, module)?.selector;
            let Some(sel) = sel else { break };
            let Some(entry) = sel.entries.iter().find(|e| e.kind == EntryKind::Category && e.name == name) else {
                continue;
            };
            dui::click_rect(keys, entry.click)?;
            wait(process, "module list", |r, i| {
                Ok(module(r, i)?
                    .selector
                    .is_some_and(|s| s.entries.iter().any(|e| e.kind == EntryKind::Module)))
            })?;
            let listed_now = read(process, module)?.selector.unwrap();
            for e in listed_now.entries.iter().filter(|e| e.kind == EntryKind::Module) {
                if let Some(m) = &e.module {
                    listed.push(m.clone());
                }
            }
            if let Some(entry) = listed_now
                .entries
                .iter()
                .find(|e| e.kind == EntryKind::Module && e.module.as_deref() == Some(want))
            {
                dui::click_rect(keys, entry.click)?;
                return finish_pick(process, slot, want);
            }
            dui::click_rect(keys, listed_now.back)?;
            wait(process, "category list", |r, i| {
                Ok(module(r, i)?
                    .selector
                    .is_some_and(|s| s.entries.iter().any(|e| e.kind == EntryKind::Category)))
            })?;
        }
        Err(Error::new(
            "module_unavailable",
            format!(
                "{want} is not offered for {slot}; unlocked modules there: {}; nothing was saved",
                if listed.is_empty() { "none".to_string() } else { listed.join(", ") }
            ),
        ))
    })();
    if result.is_err() {
        let _ = selector_close(process, keys);
    }
    result
}
fn finish_pick(process: &dyn Process, slot: &str, want: &str) -> Result<()> {
    wait(process, "module chosen", |r, i| {
        let s = module(r, i)?;
        Ok(s.selector.is_none() && draft_module(&s, slot).as_deref() == Some(want))
    })
}

// ---- options ------------------------------------------------------------------------------------------------------------
pub(super) fn options_listing(process: &dyn Process, kind: &str) -> Result<Value> {
    let c = catalogue(process)?;
    let rows: Vec<Value> = c
        .designs
        .iter()
        .filter(|d| designs::kind_of(&d.variant.type_key) == kind)
        .map(|d| {
            json!({"design_id":d.variant.id,"type":d.variant.type_key,
                "name":(!d.variant.name.is_empty()).then_some(&d.variant.name),
                "default_design":d.variant.parent_id.is_none(),
                "designer":designer_kind(&c, d),
                "production_lines":d.lines.len()})
        })
        .collect();
    Ok(json!({"kind":kind,"bases":rows,"experience_account":experience_label(kind),
        "note":"Pass one design_id or name as `base` to open its designer and read the upgrade rows or module slots."}))
}
pub(super) fn options(
    process: &dyn Process,
    keys: &MacKeys,
    kind: &str,
    base: &BaseRef,
    slot: Option<&str>,
    measure: bool,
) -> Result<Value> {
    let c = catalogue(process)?;
    let design = resolve_base(&c.designs, kind, base)?;
    let id = design.variant.id;
    let has_modules = designer_kind(&c, design) == "modules";
    let result = options_run(process, keys, kind, design, id, has_modules, slot, measure);
    dui::leave(process, keys);
    result
}
#[allow(clippy::too_many_arguments)]
fn options_run(
    process: &dyn Process,
    keys: &MacKeys,
    kind: &str,
    design: &Design,
    id: i32,
    has_modules: bool,
    slot: Option<&str>,
    measure: bool,
) -> Result<Value> {
    let opened = dui::open_for(process, keys, id, kind)?;
    if has_modules != (opened != Kind::Upgrade) {
        return Err(Error::new(
            "designer_unsupported",
            format!("The {} designer opened for design {id}", opened.name()),
        ));
    }
    let base = json!({"design_id":id,"type":design.variant.type_key,"name":(!design.variant.name.is_empty()).then_some(&design.variant.name)});
    if opened == Kind::Upgrade {
        let s = read(process, upgrade)?;
        let mut rows = Vec::new();
        for row in &s.rows {
            let mut cost_per_level = Value::Null;
            let mut max_level = Value::Null;
            if measure && !row.increase_disabled {
                dui::click_rect(keys, row.increase)?;
                let w = row.window.clone();
                wait(process, "level up", |r, i| {
                    Ok(upgrade(r, i)?.rows.iter().any(|x| x.window == w && x.level == row.level + 1))
                })?;
                let up = read(process, upgrade)?;
                cost_per_level = json!(up.save_cost);
                let down = up
                    .rows
                    .iter()
                    .find(|x| x.window == row.window)
                    .map(|x| x.decrease)
                    .unwrap_or(row.decrease);
                dui::click_rect(keys, down)?;
                wait(process, "level down", |r, i| {
                    Ok(upgrade(r, i)?.rows.iter().any(|x| x.window == w && x.level == row.level))
                })?;
            } else if row.increase_disabled {
                max_level = json!(row.level);
            }
            rows.push(json!({"key":upgrade_short(row),"window":row.window,"label":row.label,"level":row.level,
                "experience_per_level":cost_per_level,"at_limit":max_level != Value::Null,
                "keys":upgrade_keys(row)}));
        }
        let s = read(process, upgrade)?;
        return Ok(json!({"designer":"upgrade","title":s.title,"base":base,"default_name":s.name,
            "experience":s.experience,"experience_account":experience_label(kind),
            "upgrades":rows,"stats":s.stats,
            "route":"Y (Production) -> category button -> the row's Create variant button; + / - on each upgrade row; name box; Save"}));
    }
    let s = read(process, module)?;
    let mut slots = Vec::new();
    for view in &s.slots {
        let Some(name) = &view.name else {
            slots.push(json!({"slot":null,"locked":true,"fixed":view.fixed}));
            continue;
        };
        let current = draft_module(&s, name);
        let mut entry = json!({"slot":name,"fixed":view.fixed,"locked":view.button.is_none(),"current":current});
        if view.button.is_some() && slot.is_none_or(|w| w == name) {
            entry["categories"] = json!(scan_slot(process, keys, name)?);
        }
        slots.push(entry);
    }
    let s = read(process, module)?;
    Ok(json!({"designer":s.kind.name(),"base":base,"default_name":s.name,
        "experience":s.experience,"experience_account":experience_label(kind),
        "slots":slots,"stats":s.stats,"refit_cost":s.refit_cost,"production_cost":s.production_cost,
        "route":"Y (Production) -> Ships button -> the row's Create variant button; slot button -> category -> module; name box; Save",
        "save_as_new":{"button":"duplicate_button","state":s.duplicate_state}}))
}

// ---- create --------------------------------------------------------------------------------------------------------------
#[derive(Debug)]
pub(super) struct Input {
    pub kind: &'static str,
    pub base: BaseRef,
    pub upgrades: BTreeMap<String, i64>,
    pub modules: BTreeMap<String, Option<String>>,
    pub name: Option<String>,
}
#[derive(Debug)]
pub(super) struct Planned {
    pub kind: &'static str,
    pub base_id: i32,
    pub base_key: String,
    pub custom_base: bool,
    pub modules_designer: bool,
    pub want: Want,
    pub before_ids: Vec<i32>,
    pub upgrades: BTreeMap<String, i64>,
    pub overrides: BTreeMap<String, Option<String>>,
    pub base_modules: BTreeMap<String, String>,
}
/// `hoi4_production_add design`: the design must exist, be of that equipment type and be listed.
pub(super) fn check_production_design(all: &[Design], id: i32, key: &str) -> Result<()> {
    let Some(d) = all.iter().find(|d| d.variant.id == id) else {
        return Err(Error::new("design_unknown", format!("No own design {id}")));
    };
    if d.variant.type_key != key {
        return Err(Error::new(
            "design_wrong_equipment",
            format!("Design {id} is a {} design, not {key}", d.variant.type_key),
        ));
    }
    if d.variant.obsolete {
        return Err(Error::new(
            "design_obsolete",
            format!("Design {id} is decommissioned; recommission it with hoi4_design_obsolete first"),
        ));
    }
    Ok(())
}
/// Deterministic default name: the base's name stem and four characters of the request id, so a
/// retried request settles on the same name.
pub(super) fn default_name(stem: &str, request_id: &str) -> String {
    let mut h: u32 = 2166136261;
    for b in request_id.bytes() {
        h = (h ^ u32::from(b)).wrapping_mul(16777619);
    }
    const DIGITS: &[u8] = b"ABCDEFGHJKLMNPQRSTUVWXYZ23456789";
    let tag: String = (0..4).map(|i| DIGITS[((h >> (5 * i)) & 31) as usize] as char).collect();
    let stem: String = stem.chars().take(MAX_NAME - 5).collect();
    format!("{stem} {tag}")
}
/// Every refusal that needs no input, in order. A retried request skips the name-taken check
/// (its own design may already exist) and only rebuilds what it must settle on.
pub(super) fn plan(input: &Input, c: &Catalogue, request_id: &str, retry: bool) -> Result<Planned> {
    let base = resolve_base(&c.designs, input.kind, &input.base)?;
    let modules_designer = designer_kind(c, base) == "modules";
    let base_slots = c.slots.get(&base.variant.id).cloned().unwrap_or_default();
    if modules_designer {
        if !input.upgrades.is_empty() {
            return Err(Error::new(
                "invalid_arguments",
                "This design has module slots: pass `modules`, not `upgrades`",
            ));
        }
        if input.modules.is_empty() {
            return Err(Error::new("nothing_to_change", "Name the slots to change in `modules`"));
        }
        let names: Vec<&str> = base_slots.iter().map(|(_, n)| n.as_str()).collect();
        for slot in input.modules.keys() {
            if !names.contains(&slot.as_str()) {
                return Err(Error::new(
                    "slot_unknown",
                    format!("{slot} is not a slot of {}; slots: {}", base.variant.type_key, names.join(", ")),
                ));
            }
        }
    } else {
        if !input.modules.is_empty() {
            return Err(Error::new(
                "invalid_arguments",
                "This design has no module slots: pass `upgrades`, not `modules`",
            ));
        }
        if input.upgrades.values().all(|l| *l == 0) {
            return Err(Error::new("nothing_to_change", "Name at least one upgrade row with a level above 0"));
        }
    }
    let expected = modules_designer.then(|| expected_modules(&base.modules, &input.modules));
    if let Some(e) = &expected {
        if *e == module_map(&base.modules) && input.name.is_none() {
            return Err(Error::new("nothing_to_change", "The modules already are exactly those of the base design"));
        }
        if *e == module_map(&base.modules) {
            return Err(Error::new("nothing_to_change", "The modules already are exactly those of the base design; a renamed copy is not created"));
        }
    }
    let stem = if base.variant.name.is_empty() {
        base.variant.type_key.clone()
    } else {
        base.variant.name.clone()
    };
    let name = match &input.name {
        Some(n) => n.clone(),
        None => default_name(&stem, request_id),
    };
    check_name(&name)?;
    if !retry {
        name_taken(&c.designs, &base.variant.type_key, &name)?;
    }
    Ok(Planned {
        kind: input.kind,
        base_id: base.variant.id,
        base_key: base.variant.type_key.clone(),
        custom_base: base.variant.parent_id.is_some(),
        modules_designer,
        want: Want {
            type_key: base.variant.type_key.clone(),
            name,
            parent: base.variant.id,
            modules: expected,
            upgrade_total: (!modules_designer).then(|| input.upgrades.values().sum()),
        },
        before_ids: c.designs.iter().map(|d| d.variant.id).collect(),
        upgrades: input.upgrades.clone(),
        overrides: input.modules.clone(),
        base_modules: module_map(&base.modules),
    })
}

/// Drive the designer to the planned design and Save. Cleans the screen up on every path.
pub(super) fn apply(process: &dyn Process, keys: &MacKeys, plan: &Planned, kind: &str) -> Result<Value> {
    let result = apply_inner(process, keys, plan, kind);
    dui::leave(process, keys);
    result
}
fn mismatch(what: &str, detail: String) -> Error {
    Error::new("input_readback_failed", format!("{what}: {detail}"))
}
fn apply_inner(process: &dyn Process, keys: &MacKeys, plan: &Planned, kind: &str) -> Result<Value> {
    let opened = dui::open_for(process, keys, plan.base_id, kind)?;
    if plan.modules_designer != (opened != Kind::Upgrade) {
        return Err(Error::new(
            "designer_unsupported",
            format!("The {} designer opened for design {}; only the upgrade designer and the ship module designer are driven", opened.name(), plan.base_id),
        ));
    }
    if opened == Kind::Tank || opened == Kind::Plane {
        return Err(Error::new(
            "designer_unsupported",
            "The tank/plane module designer is not driven yet (not validated); nothing was saved",
        ));
    }
    if opened == Kind::Upgrade {
        apply_upgrade(process, keys, plan)
    } else {
        apply_ship(process, keys, plan)
    }
}
fn apply_upgrade(process: &dyn Process, keys: &MacKeys, plan: &Planned) -> Result<Value> {
    let s = read(process, upgrade)?;
    let base_id = read(process, |r, i| {
        let s = upgrade(r, i)?;
        fleetmem::variant(r, i, s.base).map(|v| v.id)
    })?;
    if base_id != plan.base_id {
        return Err(mismatch("designer base", format!("opened design {base_id} instead of {}", plan.base_id)));
    }
    if s.rows.iter().any(|r| r.level != 0) {
        return Err(mismatch("designer start", "the upgrade levels do not start at 0".into()));
    }
    // Resolve every requested row first: unknown keys drop the draft untouched.
    let mut targets: Vec<(String, i64)> = Vec::new();
    for (key, level) in &plan.upgrades {
        let row = find_upgrade(&s.rows, key).ok_or_else(|| {
            Error::new(
                "upgrade_unknown",
                format!(
                    "No upgrade row {key:?} in this designer; rows: {}; nothing was saved",
                    s.rows.iter().map(upgrade_short).collect::<Vec<_>>().join(", ")
                ),
            )
        })?;
        targets.push((row.window.clone(), *level));
    }
    let experience_before = s.experience;
    for (window, level) in &targets {
        set_level(process, keys, window, *level)?;
    }
    let s = read(process, upgrade)?;
    for (window, level) in &targets {
        let have = s.rows.iter().find(|r| &r.window == window).map(|r| r.level);
        if have != Some(*level) {
            return Err(mismatch("upgrade levels", format!("{window} is {have:?}, wanted {level}")));
        }
    }
    type_name(process, keys, upgrade_name_point(&s), &plan.want.name, &|r, i| Ok(upgrade(r, i)?.name))?;
    let s = read(process, upgrade)?;
    if s.name != plan.want.name {
        return Err(mismatch("design name", format!("{:?} instead of {:?}", s.name, plan.want.name)));
    }
    check_affordable(s.save_cost, s.experience, s.save_enabled, experience_label(plan.kind))?;
    let stats = s.stats.clone();
    let cost = s.save_cost;
    dui::click_rect(keys, s.save)?;
    let _ = wait(process, "design saved", |r, i| {
        Ok(!dui::designer_open(r, i)? || designs_have(r, i, &plan.want))
    });
    Ok(json!({"designer":"upgrade","experience_cost":cost,"experience_before":experience_before,
        "levels":targets.iter().map(|(w, l)| json!({"row":w,"level":l})).collect::<Vec<_>>(),
        "stats_planned":stats,"base_stats":s.base_stats,"name":plan.want.name}))
}
fn designs_have(r: &crate::memory::Reader<'_>, image: u64, want: &Want) -> bool {
    let Ok((_, production)) = crate::economy::own_production(r, image) else { return false };
    designs::designs(r, image, production)
        .map(|all| all.iter().any(|d| matches_want(d, want) && !all.is_empty()))
        .unwrap_or(false)
}
fn check_affordable(cost: Option<i64>, have: Option<i64>, enabled: bool, account: &str) -> Result<()> {
    if let (Some(c), Some(h)) = (cost, have) {
        if c > h {
            return Err(Error::new(
                "insufficient_experience",
                format!("Saving costs {c} {account} experience but only {h} are held; nothing was saved"),
            ));
        }
    }
    if !enabled {
        return Err(Error::new(
            "insufficient_experience",
            "The designer disables Save (not affordable or nothing changed); nothing was saved",
        ));
    }
    Ok(())
}
fn apply_ship(process: &dyn Process, keys: &MacKeys, plan: &Planned) -> Result<Value> {
    let s = read(process, module)?;
    if s.type_key != plan.base_key {
        return Err(mismatch("designer hull", format!("{} instead of {}", s.type_key, plan.base_key)));
    }
    let start = module_map(&s.modules);
    if start != plan.base_modules {
        return Err(mismatch(
            "designer start",
            format!("the designer opened with {start:?} instead of the base design's {:?}", plan.base_modules),
        ));
    }
    let experience_before = s.experience;
    // Removals first so the freed slots never hold a module the selector would refuse.
    let mut order: Vec<(&String, &Option<String>)> = plan.overrides.iter().collect();
    order.sort_by_key(|(_, m)| m.is_some());
    for (slot, m) in order {
        set_slot(process, keys, slot, m.as_deref())?;
    }
    let s = read(process, module)?;
    let want_modules = plan.want.modules.clone().unwrap_or_default();
    if module_map(&s.modules) != want_modules {
        return Err(mismatch(
            "modules",
            format!("the draft holds {:?} instead of {:?}", module_map(&s.modules), want_modules),
        ));
    }
    // A custom design is saved as a new design (the original and its lines stay unchanged).
    if plan.custom_base && s.duplicate_state == 0 {
        dui::click_rect(keys, s.duplicate)?;
        wait(process, "save as new", |r, i| Ok(module(r, i)?.duplicate_state != 0))?;
    }
    type_name(process, keys, s.name_point, &plan.want.name, &|r, i| Ok(module(r, i)?.name))?;
    let s = read(process, module)?;
    if s.name != plan.want.name {
        return Err(mismatch("design name", format!("{:?} instead of {:?}", s.name, plan.want.name)));
    }
    check_affordable(s.save_cost, s.experience, s.save_enabled, experience_label("ship"))?;
    let cost = s.save_cost;
    let planned_stats = s.stats.clone();
    dui::click_rect(keys, s.save)?;
    let _ = wait(process, "design saved", |r, i| {
        Ok(!dui::designer_open(r, i)? || designs_have(r, i, &plan.want))
    });
    Ok(json!({"designer":"ship_modules","experience_cost":cost,"experience_before":experience_before,
        "modules_before":start,"modules_planned":want_modules,"stats_planned":planned_stats,
        "refit_cost":s.refit_cost,"production_cost":s.production_cost,"name":plan.want.name,
        "save_as_new":plan.custom_base}))
}
pub(super) fn design_row(all: &[Design], id: i32) -> Value {
    designs::rows_json(all, |d| d.variant.id == id)
        .into_iter()
        .next()
        .unwrap_or(Value::Null)
}

// ---- decommission / recommission -------------------------------------------------------------------------------------
pub(super) fn plan_obsolete(all: &[Design], id: i32, want: bool) -> Result<&Design> {
    let Some(d) = all.iter().find(|d| d.variant.id == id) else {
        let ids: Vec<String> = all.iter().filter(|d| !d.variant.name.is_empty()).map(|d| format!("{} ({})", d.variant.id, d.variant.name)).collect();
        return Err(Error::new(
            "design_unknown",
            format!("No own design {id}; named designs: {}", ids.join(", ")),
        ));
    };
    if d.variant.obsolete == want {
        return Err(Error::new(
            if want { "already_obsolete" } else { "not_obsolete" },
            format!(
                "Design {id} is {} already",
                if want { "decommissioned" } else { "up to date (not decommissioned)" }
            ),
        ));
    }
    Ok(d)
}
pub(super) fn settle_obsolete(before: bool, now: Option<bool>, want: bool) -> Verdict {
    match now {
        Some(n) if n == want => Verdict::Reached,
        Some(n) if n == before => Verdict::Unchanged,
        Some(_) => Verdict::Diverged,
        None => Verdict::Unknown,
    }
}
pub(super) fn obsolete_flag(process: &dyn Process, id: i32) -> Result<Option<bool>> {
    Ok(designs::read_all(process)?
        .iter()
        .find(|d| d.variant.id == id)
        .map(|d| d.variant.obsolete))
}
pub(super) fn obsolete_apply(process: &dyn Process, keys: &MacKeys, d: &Design, want: bool) -> Result<Value> {
    let result = (|| -> Result<Value> {
        let kind = designs::kind_of(&d.variant.type_key);
        let id = d.variant.id;
        // A recommission needs the hidden (outdated) row: the list's filter is ticked and put back.
        let located = dui::locate(process, keys, id, kind, !want)?;
        dui::click_obsolete(process, keys, id, want)?;
        wait(process, "obsolete flag", |r, _| {
            Ok((r.i32(d.variant.address + fleetmem::VARIANT_OBSOLETE)? != 0) == want)
        })?;
        if located.outdated_ticked {
            dui::untick_outdated(process, keys)?;
        }
        Ok(json!({"design_id":id,"obsolete":want,"list":located.category,
            "outdated_filter_ticked_by_tool":located.ticked_by_us,
            "outdated_filter_restored_to_unticked":located.outdated_ticked,
            "route":"Y (Production) -> category button -> the row's Decommission/Recommission button"}))
    })();
    dui::leave(process, keys);
    result
}

// ---- retarget a production line -------------------------------------------------------------------------------------
#[derive(Debug, Clone)]
pub(super) struct LineChange {
    pub handle: u64,
    pub design: i32,
    pub from_design: Option<i32>,
    pub naval: bool,
}
pub(super) fn plan_set_production(
    lines: &[crate::production_lines::OwnLine],
    all: &[Design],
    handle: u64,
    design: i32,
    accept_progress_loss: bool,
) -> Result<LineChange> {
    let line = lines.iter().find(|l| l.handle == handle).ok_or_else(|| {
        Error::new(
            "line_unknown",
            format!(
                "No own production line {handle}; lines: {}",
                lines.iter().map(|l| l.handle.to_string()).collect::<Vec<_>>().join(", ")
            ),
        )
    })?;
    let target = all
        .iter()
        .find(|d| d.variant.id == design)
        .ok_or_else(|| Error::new("design_unknown", format!("No own design {design}")))?;
    let current = all
        .iter()
        .find(|d| d.lines.iter().any(|(h, _)| *h == handle));
    if current.is_some_and(|c| c.variant.id == design) {
        return Err(Error::new(
            "already_building",
            format!("Line {handle} already builds design {design}"),
        ));
    }
    let target_naval = designs::kind_of(&target.variant.type_key) == "ship";
    if target_naval != line.naval {
        return Err(Error::new(
            "design_wrong_type",
            format!(
                "Line {handle} is a {} line; design {design} ({}) is not {} equipment",
                if line.naval { "naval" } else { "military" },
                target.variant.type_key,
                if line.naval { "naval" } else { "military" }
            ),
        ));
    }
    if target.variant.obsolete {
        return Err(Error::new(
            "design_obsolete",
            format!("Design {design} is decommissioned and not offered; recommission it first"),
        ));
    }
    if !accept_progress_loss {
        return Err(Error::new(
            "progress_loss_unconfirmed",
            "The game warns that a line change loses the line's progress; pass accept_progress_loss: true to do it anyway",
        ));
    }
    Ok(LineChange {
        handle,
        design,
        from_design: current.map(|c| c.variant.id),
        naval: line.naval,
    })
}
pub(super) fn settle_set_production(all: &[Design], change: &LineChange) -> Verdict {
    let has = |id: i32| {
        all.iter()
            .find(|d| d.variant.id == id)
            .is_some_and(|d| d.lines.iter().any(|(h, _)| *h == change.handle))
    };
    if has(change.design) {
        Verdict::Reached
    } else if change.from_design.is_none_or(has) {
        Verdict::Unchanged
    } else {
        Verdict::Unknown
    }
}
pub(super) fn set_production_apply(process: &dyn Process, keys: &MacKeys, change: &LineChange) -> Result<Value> {
    let result = (|| -> Result<Value> {
        super::ui::clear_notices(process, keys)?;
        super::production::ensure_open(process, keys)?;
        if read(process, dui::selection_open)? {
            keys.press(Key::Escape)?;
            wait(process, "close selection", |r, i| Ok(!dui::selection_open(r, i)?))?;
        }
        super::production::reveal_named(process, keys, change.handle, "equipment_button", 40)?;
        let rect = settled(process, "line equipment button", |r, i| {
            super::production::button(r, i, change.handle, "equipment_button")?
                .ok_or_else(|| Error::new("gui_target_unavailable", "Line equipment button not found"))
        })?;
        // The last line of the list may sit under the panel's lower edge: click its visible top.
        dui::click_rect(keys, Rect { h: rect.h.min(40), ..rect })?;
        wait(process, "line change list", dui::selection_open)?;
        let _ = settled(process, "line change rows", |r, i| {
            Ok(dui::rows(r, i)?.iter().map(|x| (x.id, x.shown)).collect::<Vec<_>>())
        });
        let mut last_y: Option<i32> = None;
        for _ in 0..40 {
            let list = read(process, dui::rows)?;
            let Some(row) = list.iter().find(|x| x.id == change.design && x.shown) else {
                let offered: Vec<String> = list.iter().filter(|x| x.shown).map(|x| x.id.to_string()).collect();
                return Err(Error::new(
                    "design_not_selectable",
                    format!(
                        "The game's line change list does not offer design {}; it offers: {}; nothing was changed",
                        change.design,
                        offered.join(", ")
                    ),
                ));
            };
            if row.invalid {
                return Err(Error::new(
                    "design_invalid",
                    format!("The game marks design {} invalid (it lacks required modules, e.g. a battery) and does not let a line build it; nothing was changed", change.design),
                ));
            }
            let Some(button) = row.equipment_button else {
                return Err(Error::new("gui_target_unavailable", "The design row has no equipment button"));
            };
            if dui::in_list(&button, if change.naval { "naval_button" } else { "tank_button" }) {
                let b = settled(process, "design row", |r, i| {
                    dui::row_equipment_button(r, i, change.design)?
                        .ok_or_else(|| Error::new("gui_target_unavailable", "Design row not found"))
                })?;
                dui::click_rect(keys, b)?;
                let _ = super::ui::accept_command(process, keys)?;
                wait(process, "line retargeted", |r, i| {
                    let (_, production) = crate::economy::own_production(r, i)?;
                    let all = designs::designs(r, i, production)?;
                    Ok(settle_set_production(&all, change) == Verdict::Reached)
                })?;
                return Ok(json!({"line":change.handle,"design":change.design,"from_design":change.from_design,
                    "route":"Y (Production) -> the line's equipment picture -> the design's row in the list that opens"}));
            }
            if last_y == Some(button.y) {
                return Err(Error::new("gui_target_unavailable", "The design row cannot be scrolled into view"));
            }
            last_y = Some(button.y);
            keys.scroll(dui::GAME, (780.0, 700.0), if button.y < 170 { 240 } else { -240 })?;
            thread::sleep(Duration::from_millis(250));
        }
        Err(Error::new("gui_target_unavailable", "The design row did not scroll into view"))
    })();
    dui::leave(process, keys);
    result
}

#[allow(dead_code)]
fn unused(_: Key) {}

#[cfg(test)]
mod tests {
    use super::*;
    fn d(id: i32, key: &str, name: &str, parent: Option<i32>, modules: &[(&str, &str)]) -> Design {
        Design {
            variant: fleetmem::Variant {
                address: id as u64,
                id,
                name: name.into(),
                class_suffix: String::new(),
                type_key: key.into(),
                parent_id: parent,
                manpower: 0,
                max_strength: 0.0,
                obsolete: false,
            },
            stats: Map::new(),
            modules: modules.iter().map(|(a, b)| (a.to_string(), b.to_string())).collect(),
            upgrades: vec![],
            lines: vec![],
        }
    }
    fn all() -> Vec<Design> {
        vec![
            d(1340, "ship_hull_cruiser_2", "", None, &[("a_slot", "m1"), ("b_slot", "m2")]),
            d(1356, "ship_hull_cruiser_2", "Leipzig", Some(1340), &[("a_slot", "m1"), ("b_slot", "m3")]),
            d(1357, "ship_hull_cruiser_2", "Hipper", Some(1340), &[]),
            d(1335, "light_tank_equipment_2", "", None, &[]),
            d(1319, "fighter_equipment_1", "", None, &[]),
        ]
    }
    #[test]
    fn arguments_are_parsed_and_typed() {
        assert_eq!(base_arg(&json!(5)).unwrap(), BaseRef::Id(5));
        assert_eq!(base_arg(&json!("Leipzig")).unwrap(), BaseRef::Name("Leipzig".into()));
        assert!(base_arg(&json!(0)).is_err());
        assert!(base_arg(&json!("")).is_err());
        assert!(base_arg(&json!(true)).is_err());
        let u = upgrades_arg(&json!({"Main Gun": 2, "engine": 0})).unwrap();
        assert_eq!(u.get("main_gun"), Some(&2));
        assert!(upgrades_arg(&json!({"gun": 99})).is_err());
        assert!(upgrades_arg(&json!({"gun": -1})).is_err());
        assert!(upgrades_arg(&json!({"gu%n": 1})).is_err());
        let m = modules_arg(&json!({"front_1_custom_slot":"ship_anti_air_1","mid_1_custom_slot":null,"x_slot":"none"})).unwrap();
        assert_eq!(m["front_1_custom_slot"], Some("ship_anti_air_1".into()));
        assert_eq!(m["mid_1_custom_slot"], None);
        assert_eq!(m["x_slot"], None);
        assert!(modules_arg(&json!({"Front":"x"})).is_err());
        assert!(modules_arg(&json!({"a":"Bad Key"})).is_err());
        assert!(modules_arg(&json!({"a":5})).is_err());
        assert!(check_name("Zz Test").is_ok());
        assert_eq!(check_name("Bad%").unwrap_err().code, "name_not_typable");
        assert!(check_name(" lead").is_err());
        assert!(check_name(&"x".repeat(41)).is_err());
    }
    #[test]
    fn bases_resolve_by_id_name_or_type() {
        let a = all();
        assert_eq!(resolve_base(&a, "ship", &BaseRef::Id(1356)).unwrap().variant.name, "Leipzig");
        assert_eq!(resolve_base(&a, "ship", &BaseRef::Name("leipzig".into())).unwrap().variant.id, 1356);
        // a type key names the default design only
        assert_eq!(resolve_base(&a, "ship", &BaseRef::Name("ship_hull_cruiser_2".into())).unwrap().variant.id, 1340);
        assert_eq!(resolve_base(&a, "tank", &BaseRef::Name("light_tank_equipment_2".into())).unwrap().variant.id, 1335);
        assert_eq!(resolve_base(&a, "plane", &BaseRef::Id(1356)).unwrap_err().code, "kind_mismatch");
        let e = resolve_base(&a, "ship", &BaseRef::Name("nope".into())).unwrap_err();
        assert_eq!(e.code, "base_unknown");
        assert!(e.message.contains("1356"));
        let mut b = all();
        b.push(d(9, "ship_hull_cruiser_2", "Leipzig", Some(1340), &[]));
        assert_eq!(resolve_base(&b, "ship", &BaseRef::Name("Leipzig".into())).unwrap_err().code, "base_ambiguous");
    }
    fn catalogue_of(all: Vec<Design>) -> Catalogue {
        let mut slots = BTreeMap::new();
        for x in &all {
            if x.variant.type_key.starts_with("ship_hull") {
                slots.insert(x.variant.id, vec![(1, "a_slot".to_string()), (2, "b_slot".to_string()), (3, "c_slot".to_string())]);
            } else {
                slots.insert(x.variant.id, vec![]);
            }
        }
        Catalogue { designs: all, slots }
    }
    #[test]
    fn requests_are_refused_before_input() {
        let c = catalogue_of(all());
        let mk = |kind: &'static str, base: BaseRef, up: &[(&str, i64)], mo: &[(&str, Option<&str>)], name: Option<&str>| Input {
            kind,
            base,
            upgrades: up.iter().map(|(k, v)| (k.to_string(), *v)).collect(),
            modules: mo.iter().map(|(k, v)| (k.to_string(), v.map(str::to_string))).collect(),
            name: name.map(str::to_string),
        };
        // ship: unknown slot lists the slots
        let plan = |i: &Input, c: &Catalogue| super::plan(i, c, "req1", false);
        let e = plan(&mk("ship", BaseRef::Id(1340), &[], &[("zz_slot", Some("m9"))], Some("N")), &c).unwrap_err();
        assert_eq!(e.code, "slot_unknown");
        assert!(e.message.contains("a_slot, b_slot, c_slot"));
        // ship: unchanged modules
        let e = plan(&mk("ship", BaseRef::Id(1340), &[], &[("a_slot", Some("m1"))], Some("N")), &c).unwrap_err();
        assert_eq!(e.code, "nothing_to_change");
        // ship: upgrades on a module design / modules on an upgrade design
        assert_eq!(plan(&mk("ship", BaseRef::Id(1340), &[("gun", 1)], &[], None), &c).unwrap_err().code, "invalid_arguments");
        assert_eq!(plan(&mk("tank", BaseRef::Id(1335), &[], &[("a", None)], None), &c).unwrap_err().code, "invalid_arguments");
        // tank: nothing to change when every level is 0 / none given
        assert_eq!(plan(&mk("tank", BaseRef::Id(1335), &[("gun", 0)], &[], None), &c).unwrap_err().code, "nothing_to_change");
        assert_eq!(plan(&mk("tank", BaseRef::Id(1335), &[], &[], None), &c).unwrap_err().code, "nothing_to_change");
        // name taken per equipment type
        let e = plan(&mk("ship", BaseRef::Id(1340), &[], &[("a_slot", Some("m9"))], Some("leipzig")), &c).unwrap_err();
        assert_eq!(e.code, "name_taken");
        assert_eq!(plan(&mk("ship", BaseRef::Id(1340), &[], &[("a_slot", Some("m9"))], Some("Ok1")), &c).unwrap().want.name, "Ok1");
        // default name is unique and typable
        let p = plan(&mk("tank", BaseRef::Id(1335), &[("gun", 1)], &[], None), &c).unwrap();
        assert_eq!(p.want.name, default_name("light_tank_equipment_2", "req1"));
        assert_eq!(p.want.name.chars().count(), "light_tank_equipment_2".len() + 5);
        assert_ne!(default_name("x", "req1"), default_name("x", "req2"));
        assert!(!p.modules_designer);
        assert_eq!(p.want.parent, 1335);
        // expected modules of a ship
        let p = plan(&mk("ship", BaseRef::Id(1356), &[], &[("a_slot", None), ("c_slot", Some("m7"))], Some("Zz")), &c).unwrap();
        assert!(p.custom_base);
        let m = p.want.modules.unwrap();
        assert_eq!(m.get("a_slot"), None);
        assert_eq!(m.get("b_slot").map(String::as_str), Some("m3"));
        assert_eq!(m.get("c_slot").map(String::as_str), Some("m7"));
    }
    #[test]
    fn creation_settlement_compares_only_new_designs() {
        let before = vec![1340, 1356, 1357, 1335, 1319];
        let mut want = Want {
            type_key: "ship_hull_cruiser_2".into(),
            name: "Zz".into(),
            parent: 1340,
            modules: Some([("a_slot".to_string(), "m1".to_string())].into_iter().collect()),
            upgrade_total: None,
        };
        let mut now = all();
        // no new design: unchanged (other designs may change freely)
        assert_eq!(settle_create(&before, &now, &want), Verdict::Unchanged);
        now.push(d(2000, "ship_hull_cruiser_2", "Zz", Some(1340), &[("a_slot", "m1")]));
        assert_eq!(settle_create(&before, &now, &want), Verdict::Reached);
        // a new design with other modules is a recorded divergence
        want.modules = Some([("a_slot".to_string(), "m2".to_string())].into_iter().collect());
        assert_eq!(settle_create(&before, &now, &want), Verdict::Diverged);
        // a second derived design of the same base next to the exact one is also diverged
        want.modules = Some([("a_slot".to_string(), "m1".to_string())].into_iter().collect());
        now.push(d(2001, "ship_hull_cruiser_2", "Other", Some(1340), &[("a_slot", "m1")]));
        assert_eq!(settle_create(&before, &now, &want), Verdict::Diverged);
        // designs of other equipment appearing meanwhile are not this action's business
        now.pop();
        now.push(d(2002, "light_tank_equipment_2", "Elsewhere", Some(1335), &[]));
        assert_eq!(settle_create(&before, &now, &want), Verdict::Reached);
        // retry: exact match only
        assert_eq!(settle_retry(&now, &want), Verdict::Reached);
        want.name = "Missing".into();
        assert_eq!(settle_retry(&now, &want), Verdict::Unchanged);
        // upgrade designs match by type and name only
        let w2 = Want { type_key: "light_tank_equipment_2".into(), name: "Elsewhere".into(), parent: 1335, modules: None, upgrade_total: Some(0) };
        assert_eq!(settle_retry(&now, &w2), Verdict::Reached);
    }
    fn row(window: &str, label: &str, level: i64) -> dui::UpgradeRow {
        let r = Rect { x: 0, y: 0, w: 1, h: 1 };
        dui::UpgradeRow {
            window: window.into(),
            label: label.into(),
            level,
            decrease: r,
            increase: r,
            decrease_disabled: false,
            increase_disabled: false,
        }
    }
    #[test]
    fn upgrade_rows_match_by_window_short_or_label() {
        let rows = vec![row("tank_gun_upgrade", "Main Gun", 0), row("tank_armor_upgrade", "Armor", 0), row("plane_engine_upgrade", "Engine", 0)];
        assert_eq!(find_upgrade(&rows, "tank_gun").unwrap().label, "Main Gun");
        assert_eq!(find_upgrade(&rows, "gun").unwrap().label, "Main Gun");
        assert_eq!(find_upgrade(&rows, "main_gun").unwrap().label, "Main Gun");
        assert_eq!(find_upgrade(&rows, "Main Gun").unwrap().label, "Main Gun");
        assert_eq!(find_upgrade(&rows, "engine").unwrap().label, "Engine");
        assert!(find_upgrade(&rows, "reliability").is_none());
        assert_eq!(upgrade_short(&rows[0]), "gun");
    }
    #[test]
    fn expected_modules_apply_overrides() {
        let base = vec![("a".to_string(), "m1".to_string()), ("b".to_string(), "m2".to_string()), ("c".to_string(), String::new())];
        let mut o = BTreeMap::new();
        o.insert("a".to_string(), None);
        o.insert("d".to_string(), Some("m4".to_string()));
        let e = expected_modules(&base, &o);
        assert_eq!(e.len(), 2);
        assert_eq!(e["b"], "m2");
        assert_eq!(e["d"], "m4");
    }

    #[test]
    fn obsolete_and_line_change_plans_are_typed() {
        let mut a = all();
        a[1].variant.obsolete = true;
        assert_eq!(plan_obsolete(&a, 9999, true).unwrap_err().code, "design_unknown");
        assert_eq!(plan_obsolete(&a, 1356, true).unwrap_err().code, "already_obsolete");
        assert_eq!(plan_obsolete(&a, 1340, false).unwrap_err().code, "not_obsolete");
        assert!(plan_obsolete(&a, 1340, true).is_ok());
        assert!(plan_obsolete(&a, 1356, false).is_ok());
        // settlement compares only the one flag
        assert_eq!(settle_obsolete(false, Some(true), true), Verdict::Reached);
        assert_eq!(settle_obsolete(false, Some(false), true), Verdict::Unchanged);
        assert_eq!(settle_obsolete(true, Some(false), true), Verdict::Diverged);
        assert_eq!(settle_obsolete(false, None, true), Verdict::Unknown);
    }
    #[test]
    fn line_change_refusals_come_before_input() {
        use crate::production_lines::OwnLine;
        let mut a = all();
        a[3].lines = vec![(77, 1)]; // design 1335 builds line 77
        let lines = vec![
            OwnLine { handle: 77, naval: false, key: Some("light_tank_equipment_2".into()), requested: 1 },
            OwnLine { handle: 88, naval: true, key: Some("ship_hull_cruiser_2".into()), requested: 1 },
        ];
        let plan = |h, d, ok| plan_set_production(&lines, &a, h, d, ok);
        assert_eq!(plan(5, 1319, true).unwrap_err().code, "line_unknown");
        assert_eq!(plan(77, 5555, true).unwrap_err().code, "design_unknown");
        assert_eq!(plan(77, 1335, true).unwrap_err().code, "already_building");
        assert_eq!(plan(77, 1340, true).unwrap_err().code, "design_wrong_type");
        assert_eq!(plan(88, 1319, true).unwrap_err().code, "design_wrong_type");
        assert_eq!(plan(77, 1319, false).unwrap_err().code, "progress_loss_unconfirmed");
        let c = plan(77, 1319, true).unwrap();
        assert_eq!((c.handle, c.design, c.from_design, c.naval), (77, 1319, Some(1335), false));
        let mut b = a.clone();
        b[4].variant.obsolete = true;
        assert_eq!(plan_set_production(&lines, &b, 77, 1319, true).unwrap_err().code, "design_obsolete");
        // settlement: reached when the target design lists the line, unchanged while the old one does
        assert_eq!(settle_set_production(&a, &c), Verdict::Unchanged);
        let mut moved = a.clone();
        moved[3].lines.clear();
        moved[4].lines = vec![(77, 1)];
        assert_eq!(settle_set_production(&moved, &c), Verdict::Reached);
        moved[4].lines.clear();
        assert_eq!(settle_set_production(&moved, &c), Verdict::Unknown);
    }
    #[test]
    fn production_add_design_is_checked() {
        let mut a = all();
        assert_eq!(check_production_design(&a, 9, "x").unwrap_err().code, "design_unknown");
        assert_eq!(check_production_design(&a, 1335, "fighter_equipment_1").unwrap_err().code, "design_wrong_equipment");
        assert!(check_production_design(&a, 1335, "light_tank_equipment_2").is_ok());
        a[3].variant.obsolete = true;
        assert_eq!(check_production_design(&a, 1335, "light_tank_equipment_2").unwrap_err().code, "design_obsolete");
    }
    #[test]
    fn default_names_are_stable_per_request_and_typable() {
        let n = default_name("light_tank_equipment_2", "req-1");
        assert_eq!(n, default_name("light_tank_equipment_2", "req-1"));
        assert_ne!(n, default_name("light_tank_equipment_2", "req-2"));
        assert!(check_name(&n).is_ok());
        let long = default_name(&"y".repeat(60), "r");
        assert!(long.chars().count() <= MAX_NAME);
        assert!(check_name(&long).is_ok());
    }
}
