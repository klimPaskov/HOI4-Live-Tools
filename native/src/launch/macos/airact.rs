//! Air orders with ordinary input on macOS (089EA519): rebase a wing, found a wing from the
//! reserve, disband, hold, and read a strategic region's air presence. All of them go through the
//! strategic air map (F3): wings are selected through their row of the Air Forces Overview (or a
//! base icon), a base is found with the Find view and its icon read from the game's own map icon
//! objects (`airui::base_icons`), orders are the game's right clicks, the N shortcut of the base
//! panel and the toolbar buttons. Memory (the own strategic air object: wings, bases, the
//! transfer destination at wing +0x44) verifies every step; nothing is retried after possible input.
//!
//! Routes mapped and verified live on 26 October 2026 (Germany, 1 January 1936, 1920x1080):
//! * left click on a base circle: selects the base's wings and opens the base panel (header with the
//!   base name, capacity and the N / quick-deploy / close buttons);
//! * N with a base panel open and aircraft in reserve: window "Create new Air Wing in <base>": a
//!   click on a reserve row moves up to 100 planes of that variant into the wing, Ok founds it;
//! * wing selected, right click on another base's circle: the wing's status reads "To <base>",
//!   a dashed flight line appears, wing +0x44 holds the destination base id;
//! * toolbar delete (`btn_delete`) opens the "Disband Air Wing" confirmation (OK returns all
//!   equipment to reserve); toolbar hold (`btn_hold`, shortcut H) stops the missions.
use super::{
    air,
    airread::{self, Base, Wing},
    airui, fleetmem, mapdata,
    settle::Verdict,
    ui::{accept_command, clear_notices, find_state, read, settled, wait},
};
use crate::{
    error::{Error, Result},
    platform::{
        macos_input::{Key, MacKeys},
        Process,
    },
};
use serde_json::{json, Value};
use std::{thread, time::Duration};

const GAME: (u32, u32) = airui::GAME;
const F1: u16 = 122;
const F3: u16 = 99;

// ---- memory ------------------------------------------------------------------------------------
pub(super) fn own(process: &dyn Process) -> Result<(Vec<Base>, Vec<Wing>)> {
    read(process, |r, image| {
        fleetmem::require_libcxx(r)?;
        let own = fleetmem::own_strategic(r, image, fleetmem::GS_AIR_MANAGER, fleetmem::AIR_BY_COUNTRY)?;
        airread::bases_and_wings(r, image, own)
    })
}
/// One own wing by exact name, straight from memory.
pub(super) fn own_wing(r: &crate::memory::Reader<'_>, image: u64, name: &str) -> Result<Option<Wing>> {
    fleetmem::require_libcxx(r)?;
    let own = fleetmem::own_strategic(r, image, fleetmem::GS_AIR_MANAGER, fleetmem::AIR_BY_COUNTRY)?;
    Ok(airread::bases_and_wings(r, image, own)?
        .1
        .into_iter()
        .find(|w| w.name == name))
}
fn same(a: &str, b: &str) -> bool {
    a.trim().eq_ignore_ascii_case(b.trim())
}
fn find_wing<'a>(wings: &'a [Wing], name: &str) -> Result<&'a Wing> {
    let hits: Vec<&Wing> = wings.iter().filter(|w| same(&w.name, name)).collect();
    match hits.len() {
        0 => Err(Error::new(
            "wing_unknown",
            format!(
                "No own air wing named {name:?}; own wings: {}",
                wings.iter().map(|w| w.name.as_str()).collect::<Vec<_>>().join(" | ")
            ),
        )),
        1 => Ok(hits[0]),
        _ => Err(Error::new(
            "wing_ambiguous",
            "Several own air wings carry this name",
        )),
    }
}
fn base_for_state(bases: &[Base], state: i32) -> Result<&Base> {
    bases.iter().find(|b| b.state == Some(state)).ok_or_else(|| {
        Error::new(
            "air_base_unknown",
            format!(
                "No own air base in state {state}; own air base states: {}",
                bases
                    .iter()
                    .filter_map(|b| b.state)
                    .map(|s| s.to_string())
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
        )
    })
}
/// Aircraft in the base plus wings flying to it.
fn base_load(base: &Base, wings: &[Wing]) -> i32 {
    wings
        .iter()
        .filter(|w| w.base_id == base.id || w.transfer_base_id == base.id)
        .map(|w| w.planes)
        .sum()
}

// ---- planning (pure, before any input) ---------------------------------------------------------
#[derive(Clone, Debug, PartialEq)]
pub(super) struct WingSnap {
    pub name: String,
    pub planes: i32,
    pub base_id: i32,
    pub base_state: Option<i32>,
    pub transfer_base_id: i32,
    pub mission_mask: i32,
    pub region_id: i32,
}
pub(super) fn snap(w: &Wing) -> WingSnap {
    WingSnap {
        name: w.name.clone(),
        planes: w.planes,
        base_id: w.base_id,
        base_state: w.base_state,
        transfer_base_id: w.transfer_base_id,
        mission_mask: w.mission_mask,
        region_id: w.region_id,
    }
}
#[derive(Clone, Debug, PartialEq)]
pub(super) struct RebasePlan {
    pub wing: String,
    pub planes: i32,
    pub state: i32,
    pub to_base_id: i32,
    pub from_state: Option<i32>,
    pub before: WingSnap,
}
pub(super) fn plan_rebase(
    bases: &[Base],
    wings: &[Wing],
    wing: &str,
    state: i32,
) -> Result<RebasePlan> {
    let w = find_wing(wings, wing)?;
    let target = base_for_state(bases, state)?;
    if w.base_id == target.id && w.transfer_base_id == 0 {
        return Err(Error::new(
            "nothing_to_change",
            format!("{} is already based in state {state}", w.name),
        ));
    }
    if w.transfer_base_id == target.id {
        return Err(Error::new(
            "nothing_to_change",
            format!("{} is already flying to state {state}", w.name),
        ));
    }
    let others: i32 = wings
        .iter()
        .filter(|o| o.name != w.name && (o.base_id == target.id || o.transfer_base_id == target.id))
        .map(|o| o.planes)
        .sum();
    if target.capacity - others < w.planes {
        return Err(Error::new(
            "air_base_full",
            format!(
                "The air base in state {state} holds {others} of {} aircraft; {} needs {}",
                target.capacity, w.name, w.planes
            ),
        ));
    }
    Ok(RebasePlan {
        wing: w.name.clone(),
        planes: w.planes,
        state,
        to_base_id: target.id,
        from_state: w.base_state,
        before: snap(w),
    })
}
/// The wing's base or transfer destination changes only by this action.
pub(super) fn settle_rebase(before: &WingSnap, now: Option<&WingSnap>, to_base_id: i32) -> Verdict {
    match now {
        Some(n) if n.base_id == to_base_id || n.transfer_base_id == to_base_id => Verdict::Reached,
        Some(n) if n.base_id == before.base_id && n.transfer_base_id == before.transfer_base_id => {
            Verdict::Unchanged
        }
        Some(_) => Verdict::Diverged,
        None => Verdict::Unknown,
    }
}
#[derive(Clone, Debug, PartialEq)]
pub(super) struct CreatePlan {
    pub state: i32,
    pub base_id: i32,
    pub aircraft: String,
    pub names_before: Vec<String>,
}
pub(super) fn plan_create(bases: &[Base], wings: &[Wing], state: i32, aircraft: &str) -> Result<CreatePlan> {
    let base = base_for_state(bases, state)?;
    if base.capacity - base_load(base, wings) <= 0 {
        return Err(Error::new(
            "air_base_full",
            format!("The air base in state {state} has no free capacity"),
        ));
    }
    Ok(CreatePlan {
        state,
        base_id: base.id,
        aircraft: aircraft.trim().to_string(),
        names_before: wings
            .iter()
            .filter(|w| w.base_id == base.id)
            .map(|w| w.name.clone())
            .collect(),
    })
}
/// A founded wing is a name now present at the base that was not before.
pub(super) fn settle_create(plan: &CreatePlan, wings: &[Wing]) -> (Verdict, Vec<String>) {
    let now: Vec<String> = wings
        .iter()
        .filter(|w| w.base_id == plan.base_id)
        .map(|w| w.name.clone())
        .collect();
    let added: Vec<String> = now
        .iter()
        .filter(|n| !plan.names_before.contains(n))
        .cloned()
        .collect();
    let verdict = if added.len() == 1 {
        Verdict::Reached
    } else if added.is_empty() && now.len() == plan.names_before.len() {
        Verdict::Unchanged
    } else {
        Verdict::Diverged
    };
    (verdict, added)
}
/// The reserve row asked for: variant name or class name, case-insensitive; one row only.
pub(super) fn pick_entry<'a>(entries: &'a [airui::Entry], aircraft: &str) -> Result<&'a airui::Entry> {
    let exact: Vec<&airui::Entry> = entries
        .iter()
        .filter(|e| same(&e.variant, aircraft) || same(&e.class, aircraft))
        .collect();
    let hits = if exact.is_empty() {
        let want = aircraft.trim().to_lowercase();
        entries
            .iter()
            .filter(|e| e.variant.to_lowercase().contains(&want) || e.class.to_lowercase().contains(&want))
            .collect()
    } else {
        exact
    };
    match hits.len() {
        1 => Ok(hits[0]),
        0 => Err(Error::new(
            "aircraft_not_in_reserve",
            format!(
                "No reserve aircraft matches {aircraft:?}; in reserve: {}",
                if entries.is_empty() {
                    "none".to_string()
                } else {
                    entries
                        .iter()
                        .map(|e| format!("{} ({}) x{}", e.variant, e.class, e.count))
                        .collect::<Vec<_>>()
                        .join(" | ")
                }
            ),
        )),
        _ => Err(Error::new(
            "aircraft_ambiguous",
            "Several reserve rows match; name the variant exactly",
        )),
    }
}
pub(super) fn plan_disband(wings: &[Wing], wing: &str) -> Result<WingSnap> {
    Ok(snap(find_wing(wings, wing)?))
}
pub(super) fn settle_disband(before: &WingSnap, wings: &[Wing]) -> Verdict {
    let same_name: Vec<&Wing> = wings.iter().filter(|w| w.name == before.name).collect();
    match same_name.len() {
        0 => Verdict::Reached,
        1 if same_name[0].planes == before.planes => Verdict::Unchanged,
        _ => Verdict::Diverged,
    }
}
#[derive(Clone, Debug, PartialEq)]
pub(super) struct SplitPlan {
    pub wing: String,
    pub planes: i32,
    pub base_id: i32,
    pub names_before: Vec<String>,
}
pub(super) fn plan_split(wings: &[Wing], name: &str) -> Result<SplitPlan> {
    let w = find_wing(wings, name)?;
    if w.transfer_base_id != 0 {
        return Err(Error::new("wing_in_transfer", format!("{} is flying a transfer", w.name)));
    }
    if w.planes < 2 {
        return Err(Error::new("nothing_to_change", format!("{} has too few aircraft to split", w.name)));
    }
    Ok(SplitPlan {
        wing: w.name.clone(),
        planes: w.planes,
        base_id: w.base_id,
        names_before: wings.iter().filter(|o| o.base_id == w.base_id).map(|o| o.name.clone()).collect(),
    })
}
/// A split adds exactly one wing at the base and takes planes from the original.
pub(super) fn settle_split(plan: &SplitPlan, wings: &[Wing]) -> (Verdict, Vec<String>) {
    let at_base: Vec<&Wing> = wings.iter().filter(|w| w.base_id == plan.base_id).collect();
    let added: Vec<String> = at_base
        .iter()
        .filter(|w| !plan.names_before.contains(&w.name))
        .map(|w| w.name.clone())
        .collect();
    let original = at_base.iter().find(|w| w.name == plan.wing).map(|w| w.planes);
    let verdict = match (added.len(), original) {
        (1, Some(p)) if p < plan.planes => Verdict::Reached,
        (0, Some(p)) if p == plan.planes => Verdict::Unchanged,
        _ => Verdict::Diverged,
    };
    (verdict, added)
}
#[derive(Clone, Debug, PartialEq)]
pub(super) struct MergePlan {
    pub wing: String,
    /// Every wing of the group (aircraft type at the base), the first wing included.
    pub group: Vec<String>,
    pub base_id: i32,
    pub state: Option<i32>,
    pub total: i32,
    /// Aircraft count of each group member sums to `total`; used to find the group's map stack item.
    pub planes: Vec<i32>,
}
/// Aircraft archetype of an equipment key (`tac_bomber_equipment_1` -> `tac_bomber`): the game's wing
/// type, the grouping of the map stack and of Merge (variants of one archetype group together;
/// seen live: Kampfgeschwader 152/155 and a split half shared one stack item of 160 aircraft).
pub(super) fn archetype(equipment: &str) -> &str {
    equipment.split("_equipment").next().unwrap_or(equipment)
}
/// The game's map stack and Merge button work on all wings of one aircraft type at a base, so a
/// merge joins `wing` with every other wing of its type there; `with` names the one expected partner
/// and refuses a larger group before any input.
pub(super) fn plan_merge(wings: &[Wing], a: &str, with: Option<&str>) -> Result<MergePlan> {
    let wa = find_wing(wings, a)?;
    let group: Vec<&Wing> = wings
        .iter()
        .filter(|w| w.base_id == wa.base_id && archetype(&w.equipment) == archetype(&wa.equipment))
        .collect();
    if let Some(b) = with {
        let wb = find_wing(wings, b)?;
        if wa.name == wb.name {
            return Err(Error::new("invalid_arguments", "Name two different wings"));
        }
        if wa.base_id != wb.base_id {
            return Err(Error::new(
                "merge_different_base",
                format!("{} and {} are not at the same air base", wa.name, wb.name),
            ));
        }
        if archetype(&wa.equipment) != archetype(&wb.equipment) {
            return Err(Error::new(
                "merge_different_type",
                format!("{} flies {} and {} flies {}; only wings of one aircraft type merge", wa.name, archetype(&wa.equipment), wb.name, archetype(&wb.equipment)),
            ));
        }
    }
    if group.len() < 2 {
        return Err(Error::new(
            "no_merge_partner",
            format!("{} is the only {} wing at its air base", wa.name, archetype(&wa.equipment)),
        ));
    }
    if group.iter().any(|w| w.transfer_base_id != 0) {
        return Err(Error::new("wing_in_transfer", "A wing in transfer cannot be merged"));
    }
    if let Some(b) = with {
        if group.len() != 2 {
            return Err(Error::new(
                "merge_group_larger",
                format!(
                    "The game merges all wings of one type at a base at once: {} wings fly {} there ({}); omit `with` to merge them all",
                    group.len(),
                    archetype(&wa.equipment),
                    group.iter().map(|w| w.name.as_str()).collect::<Vec<_>>().join(" | ")
                ),
            ));
        }
        let _ = b;
    }
    Ok(MergePlan {
        wing: wa.name.clone(),
        group: group.iter().map(|w| w.name.clone()).collect(),
        base_id: wa.base_id,
        state: wa.base_state,
        total: group.iter().map(|w| w.planes).sum(),
        planes: group.iter().map(|w| w.planes).collect(),
    })
}
/// Fewer wings of the group are left and together they hold all the aircraft. A wing holds at most
/// 100 aircraft: merging 40 + 80 + 40 bombers left wings of 100 and 60 (seen live).
pub(super) fn settle_merge(plan: &MergePlan, wings: &[Wing]) -> Verdict {
    let left: Vec<&Wing> = wings.iter().filter(|w| plan.group.contains(&w.name)).collect();
    let held: i32 = left.iter().map(|w| w.planes).sum();
    let unchanged = plan.group.iter().zip(&plan.planes).all(|(n, p)| {
        wings.iter().find(|w| w.name == *n).is_some_and(|w| w.planes == *p)
    });
    if !left.is_empty() && left.len() < plan.group.len() && held == plan.total {
        Verdict::Reached
    } else if unchanged {
        Verdict::Unchanged
    } else {
        Verdict::Diverged
    }
}
pub(super) fn plan_hold(wings: &[Wing], wing: &str) -> Result<WingSnap> {
    let w = find_wing(wings, wing)?;
    if w.region_id <= 0 {
        return Err(Error::new(
            "nothing_to_change",
            format!("{} is standing by (no assigned region)", w.name),
        ));
    }
    Ok(snap(w))
}
pub(super) fn settle_hold(before: &WingSnap, now: Option<&WingSnap>) -> Verdict {
    match now {
        Some(n) if n.region_id <= 0 => Verdict::Reached,
        Some(n) if n.region_id == before.region_id => Verdict::Unchanged,
        Some(_) => Verdict::Diverged,
        None => Verdict::Unknown,
    }
}
/// A wing's mission and region change only by this action.
pub(super) fn settle_mission(before: &WingSnap, now: Option<&WingSnap>, mask: i32, region: Option<i32>) -> Verdict {
    match now {
        Some(n) if n.mission_mask & mask == mask && region.is_none_or(|r| n.region_id == r) => Verdict::Reached,
        Some(n) if n.mission_mask == before.mission_mask && n.region_id == before.region_id => Verdict::Unchanged,
        Some(_) => Verdict::Diverged,
        None => Verdict::Unknown,
    }
}
// ---- geometry and ranges -----------------------------------------------------------------------
fn game_dir(process: &dyn Process) -> Option<std::path::PathBuf> {
    process.identity().and_then(|i| mapdata::game_dir(&i.exe).ok())
}
/// Base position (x, z) of a state from the game's building positions.
pub(super) fn base_position(process: &dyn Process, state: i32) -> Result<(f64, f64)> {
    let dir = game_dir(process).ok_or_else(|| Error::new("map_data_unavailable", "Game install not found"))?;
    mapdata::air_base_positions(&dir)?
        .get(&state)
        .copied()
        .ok_or_else(|| Error::new("air_base_unknown", format!("No air base position for state {state}")))
}
// ---- driving the air UI -------------------------------------------------------------------------
fn click(keys: &MacKeys, x: f64, y: f64) -> Result<()> {
    keys.click(GAME, x, y).map(|_| ())
}
fn pause(ms: u64) {
    thread::sleep(Duration::from_millis(ms));
}
fn icons_shown(r: &crate::memory::Reader<'_>, image: u64) -> Result<bool> {
    Ok(!airui::base_icons(r, image)?.is_empty())
}
/// Make sure the strategic air map (F3) is on; returns whether it was switched on here.
fn enter_air_mode(process: &dyn Process, keys: &MacKeys) -> Result<bool> {
    if read(process, icons_shown)? {
        return Ok(false);
    }
    keys.press_code(F3)?;
    wait(process, "strategic air map mode", icons_shown)?;
    pause(300);
    Ok(true)
}
fn leave_air_mode(process: &dyn Process, keys: &MacKeys, entered: bool) {
    if entered {
        let _ = keys.press_code(F1);
        let _ = wait(process, "default map mode", |r, i| Ok(!icons_shown(r, i)?));
    }
}
/// Tidy: close the wing window and the selection toolbar (best effort).
fn release(process: &dyn Process, keys: &MacKeys, entered: bool) {
    if read(process, airui::reorg_open).unwrap_or(false) {
        if let Ok(rect) = settled(process, "wing window close", airui::reorg_close) {
            let (x, y) = rect.center();
            let _ = click(keys, x, y);
            let _ = wait(process, "wing window closed", |r, i| Ok(!airui::reorg_open(r, i)?));
        }
    }
    let _ = air::deselect(process, keys);
    leave_air_mode(process, keys, entered);
}
/// Centre the camera on `state` and return the screen point of its air base circle.
fn base_point(process: &dyn Process, keys: &MacKeys, state: i32) -> Result<(f64, f64)> {
    let pos = base_position(process, state)?;
    find_state(process, keys, state)?;
    for step in 0..8 {
        let found = settled(process, "air base icon", |r, i| {
            Ok(airui::icon_at(&airui::base_icons(r, i)?, pos).cloned())
        })?;
        if let Some(icon) = found {
            let p = icon.center();
            if airui::clickable(p) {
                return Ok(p);
            }
        }
        // The base lies off the usable map area: zoom out one notch around the screen centre.
        let _ = step;
        keys.scroll(GAME, (960.0, 540.0), -240)?;
        pause(450);
    }
    Err(Error::new(
        "base_icon_unavailable",
        format!("The air base icon of state {state} could not be brought on screen"),
    ))
}
/// The selection is exactly this wing (read from the selection view's own array).
fn only_selected(process: &dyn Process, name: &str) -> Result<bool> {
    Ok(read(process, airui::selected_wings)? == [name.to_string()])
}

/// Rebase one wing: select it, right click the target base's circle.
pub(super) fn rebase(process: &dyn Process, keys: &MacKeys, plan: &RebasePlan) -> Result<Value> {
    clear_notices(process, keys)?;
    let entered = enter_air_mode(process, keys)?;
    let result = (|| -> Result<Value> {
        air::select_wing(process, keys, &plan.wing)?;
        let (x, y) = base_point(process, keys, plan.state)?;
        if !only_selected(process, &plan.wing)? {
            return Err(Error::new(
                "selection_lost",
                "The wing selection changed while the base was brought on screen; nothing was ordered",
            ));
        }
        keys.right_click(GAME, x, y)?;
        let (name, to) = (plan.wing.clone(), plan.to_base_id);
        wait(process, "rebase accepted", move |r, i| {
            fleetmem::require_libcxx(r)?;
            let own = fleetmem::own_strategic(r, i, fleetmem::GS_AIR_MANAGER, fleetmem::AIR_BY_COUNTRY)?;
            let (_, wings) = airread::bases_and_wings(r, i, own)?;
            Ok(wings
                .iter()
                .any(|w| w.name == name && (w.base_id == to || w.transfer_base_id == to)))
        })
        .map_err(|_| {
            Error::new(
                "rebase_refused",
                "The wing did not start a transfer (the game refused the base: it is full or the wing cannot move)",
            )
        })?;
        Ok(json!({"wing":plan.wing,"to_state":plan.state,"planes":plan.planes,"from_state":plan.from_state}))
    })();
    release(process, keys, entered);
    result
}

/// Select the base (left click on its circle), N, pick the reserve row, Ok.
pub(super) fn create_wing(process: &dyn Process, keys: &MacKeys, plan: &CreatePlan) -> Result<Value> {
    clear_notices(process, keys)?;
    let entered = enter_air_mode(process, keys)?;
    let result = (|| -> Result<Value> {
        let _ = air::deselect(process, keys);
        let (x, y) = base_point(process, keys, plan.state)?;
        click(keys, x, y)?;
        wait(process, "air base selected", air::selection_open)?;
        pause(300);
        keys.press(Key::letter('n')?)?;
        if wait(process, "wing window", airui::reorg_open).is_err() {
            return Err(Error::new(
                "no_reserve_aircraft",
                "The new air wing window did not open: the game refuses a wing when no aircraft are in reserve (produce or disband first)",
            ));
        }
        pause(300);
        let reserve = settled(process, "reserve rows", |r, i| airui::reorg_entries(r, i, "list_left"))?;
        let entry = pick_entry(&reserve, &plan.aircraft)?.clone();
        let (cx, cy) = entry.bg.center();
        click(keys, cx, cy)?;
        wait(process, "planes moved into the wing", |r, i| Ok(airui::reorg_planes(r, i)? > 0))?;
        let planes = read(process, airui::reorg_planes)?;
        let (rect, state) = settled(process, "wing ok", airui::reorg_confirm)?;
        if state == 3 {
            return Err(Error::new("create_unavailable", "The game disables Ok for this wing (manpower?)"));
        }
        let (ox, oy) = rect.center();
        click(keys, ox, oy)?;
        wait(process, "wing founded", |r, i| Ok(!airui::reorg_open(r, i)?))?;
        Ok(json!({"state":plan.state,"variant":entry.variant,"class":entry.class,"planes_moved":planes,
            "reserve_before":entry.count}))
    })();
    release(process, keys, entered);
    result
}

/// `air::assign` inside the strategic air map (switched on and back when it was off).
pub(super) fn mission(process: &dyn Process, keys: &MacKeys, wing: &str, mission: &str, state: i32) -> Result<()> {
    clear_notices(process, keys)?;
    let entered = enter_air_mode(process, keys)?;
    let result = air::assign(process, keys, wing, mission, state);
    leave_air_mode(process, keys, entered);
    result
}

/// Select the wing, toolbar delete, confirm.
pub(super) fn disband(process: &dyn Process, keys: &MacKeys, wing: &str) -> Result<Value> {
    clear_notices(process, keys)?;
    let entered = enter_air_mode(process, keys)?;
    let result = (|| -> Result<Value> {
        air::select_wing(process, keys, wing)?;
        if !only_selected(process, wing)? {
            return Err(Error::new("selection_lost", "The selection is not exactly this wing; nothing was disbanded"));
        }
        let (rect, state) = settled(process, "disband button", |r, i| airui::toolbar(r, i, "btn_delete"))?;
        if state == 3 {
            return Err(Error::new("disband_unavailable", "The game disables Disband for this wing"));
        }
        let (x, y) = rect.center();
        click(keys, x, y)?;
        if !accept_command(process, keys)? {
            return Err(Error::new("input_readback_failed", "The disband confirmation did not appear"));
        }
        let name = wing.to_string();
        wait(process, "wing disbanded", move |r, i| {
            fleetmem::require_libcxx(r)?;
            let own = fleetmem::own_strategic(r, i, fleetmem::GS_AIR_MANAGER, fleetmem::AIR_BY_COUNTRY)?;
            let (_, wings) = airread::bases_and_wings(r, i, own)?;
            Ok(!wings.iter().any(|w| w.name == name))
        })?;
        Ok(json!({"wing":wing,"returned_to_reserve":true}))
    })();
    release(process, keys, entered);
    result
}

/// Select the wing, toolbar split (halves it; the game keeps both halves selected).
pub(super) fn split(process: &dyn Process, keys: &MacKeys, plan: &SplitPlan) -> Result<Value> {
    clear_notices(process, keys)?;
    let entered = enter_air_mode(process, keys)?;
    let result = (|| -> Result<Value> {
        air::select_wing(process, keys, &plan.wing)?;
        if !only_selected(process, &plan.wing)? {
            return Err(Error::new("selection_lost", "The selection is not exactly this wing; nothing was split"));
        }
        let (rect, state) = settled(process, "split button", |r, i| airui::toolbar(r, i, "split_wing_btn"))?;
        if state == 3 {
            return Err(Error::new("split_unavailable", "The game disables Split for this wing"));
        }
        let (x, y) = rect.center();
        click(keys, x, y)?;
        let (name, base, before) = (plan.wing.clone(), plan.base_id, plan.planes);
        wait(process, "wing split", move |r, i| {
            fleetmem::require_libcxx(r)?;
            let own = fleetmem::own_strategic(r, i, fleetmem::GS_AIR_MANAGER, fleetmem::AIR_BY_COUNTRY)?;
            let (_, wings) = airread::bases_and_wings(r, i, own)?;
            Ok(wings.iter().any(|w| w.name == name && w.planes < before)
                && wings.iter().filter(|w| w.base_id == base).count() > 1)
        })?;
        Ok(json!({"wing":plan.wing,"planes_before":plan.planes}))
    })();
    release(process, keys, entered);
    result
}

/// Select `wing` through the overview, then the base's map stack item of its aircraft type (a Shift
/// click adds the whole type group, which includes the wing), check with the selection array that
/// exactly the group is selected and press Merge.
pub(super) fn merge(process: &dyn Process, keys: &MacKeys, plan: &MergePlan) -> Result<Value> {
    clear_notices(process, keys)?;
    let entered = enter_air_mode(process, keys)?;
    let result = (|| -> Result<Value> {
        let state = plan.state.ok_or_else(|| Error::new("air_base_unknown", "The wings' air base has no state"))?;
        let pos = base_position(process, state)?;
        let mut group = plan.group.clone();
        group.sort();
        let is_group = |names: &[String]| {
            let mut n = names.to_vec();
            n.sort();
            n == group
        };
        air::select_wing(process, keys, &plan.wing)?;
        if !only_selected(process, &plan.wing)? {
            return Err(Error::new("selection_lost", "The selection is not exactly the first wing; nothing was merged"));
        }
        base_point(process, keys, state)?;
        let items = settled(process, "wing stack", |r, i| {
            airui::stack_at(r, i, pos)?
                .ok_or_else(|| Error::new("gui_target_unavailable", "The base's wing stack is not shown"))
        })?;
        // The stack item showing the group's aircraft total is the likely one.
        let mut tries: Vec<usize> = (0..items.len()).filter(|i| items[*i].planes == plan.total).collect();
        tries.extend((0..items.len()).filter(|i| items[*i].planes != plan.total));
        let mut joined = false;
        for index in tries {
            let (x, y) = items[index].rect.center();
            keys.shift_click(GAME, x, y)?;
            pause(500);
            let names = read(process, airui::selected_wings)?;
            if is_group(&names) {
                joined = true;
                break;
            }
            // A wrong group joined: start the selection again from the first wing.
            air::select_wing(process, keys, &plan.wing)?;
            base_point(process, keys, state)?;
            if !only_selected(process, &plan.wing)? {
                break;
            }
        }
        if !joined {
            return Err(Error::new(
                "selection_lost",
                "The wings of this type could not be selected together; nothing was merged",
            ));
        }
        let (rect, state_code) = settled(process, "merge button", |r, i| airui::toolbar(r, i, "merge_wings_btn"))?;
        if state_code == 3 {
            return Err(Error::new("merge_unavailable", "The game disables Merge for these wings"));
        }
        let (x, y) = rect.center();
        click(keys, x, y)?;
        let (names, total) = (plan.group.clone(), plan.total);
        wait(process, "wings merged", move |r, i| {
            fleetmem::require_libcxx(r)?;
            let own = fleetmem::own_strategic(r, i, fleetmem::GS_AIR_MANAGER, fleetmem::AIR_BY_COUNTRY)?;
            let (_, wings) = airread::bases_and_wings(r, i, own)?;
            let kept: Vec<&Wing> = wings.iter().filter(|w| names.contains(&w.name)).collect();
            Ok(!kept.is_empty()
                && kept.len() < names.len()
                && kept.iter().map(|w| w.planes).sum::<i32>() == total)
        })?;
        Ok(json!({"merged":plan.group,"planes":plan.total}))
    })();
    release(process, keys, entered);
    result
}

/// Select the wing, toolbar hold (stops its missions; the wing stays at its base).
pub(super) fn hold(process: &dyn Process, keys: &MacKeys, wing: &str) -> Result<Value> {
    clear_notices(process, keys)?;
    let entered = enter_air_mode(process, keys)?;
    let result = (|| -> Result<Value> {
        air::select_wing(process, keys, wing)?;
        if !only_selected(process, wing)? {
            return Err(Error::new("selection_lost", "The selection is not exactly this wing; nothing was held"));
        }
        let (rect, state) = settled(process, "hold button", |r, i| airui::toolbar(r, i, "btn_hold"))?;
        if state == 3 {
            return Err(Error::new("hold_unavailable", "The game disables Hold for this wing"));
        }
        let (x, y) = rect.center();
        click(keys, x, y)?;
        let name = wing.to_string();
        wait(process, "wing held", move |r, i| {
            fleetmem::require_libcxx(r)?;
            let own = fleetmem::own_strategic(r, i, fleetmem::GS_AIR_MANAGER, fleetmem::AIR_BY_COUNTRY)?;
            let (_, wings) = airread::bases_and_wings(r, i, own)?;
            Ok(wings.iter().any(|w| w.name == name && w.region_id <= 0))
        })?;
        Ok(json!({"wing":wing,"held":true}))
    })();
    release(process, keys, entered);
    result
}

/// Open the strategic air region window of the region around `state` and read it (orders nothing).
pub(super) fn probe_region(process: &dyn Process, keys: &MacKeys, state: i32) -> Result<Value> {
    clear_notices(process, keys)?;
    let entered = enter_air_mode(process, keys)?;
    let result = (|| -> Result<Value> {
        let _ = air::deselect(process, keys);
        find_state(process, keys, state)?;
        pause(300);
        click(keys, 960.0, 540.0)?;
        let view = {
            wait(process, "air region window", |r, i| Ok(airui::region_view(r, i)?.is_some()))?;
            pause(300);
            settled(process, "air region window", |r, i| {
                airui::region_view(r, i)?
                    .ok_or_else(|| Error::new("gui_geometry_unavailable", "Region window closed"))
            })?
        };
        let json = region_json(&view);
        if let Ok(rect) = read(process, |r, i| {
            airui::region_view(r, i)?
                .map(|v| v.close)
                .ok_or_else(|| Error::new("gui_target_unavailable", "closed"))
        }) {
            let (x, y) = rect.center();
            let _ = click(keys, x, y);
            let _ = wait(process, "region window closed", |r, i| Ok(airui::region_view(r, i)?.is_none()));
        }
        Ok(json)
    })();
    leave_air_mode(process, keys, entered);
    result
}
pub(super) fn region_json(v: &airui::RegionView) -> Value {
    let kinds = ["fighters", "bombers", "close_air_support", "anti_air_guns"];
    let row = |counts: &[i32; 4]| {
        let mut m = serde_json::Map::new();
        for (k, c) in kinds.iter().zip(counts) {
            m.insert((*k).to_string(), json!(c));
        }
        Value::Object(m)
    };
    json!({"region":v.name,"our_active":row(&v.ours),"enemy_active":row(&v.theirs),
        "our_detection":v.our_detection,"our_efficiency":v.our_efficiency,
        "enemy_detection":v.enemy_detection,"enemy_efficiency":v.enemy_efficiency,
        "note":"As the player's air map shows it (counts of currently active aircraft by type in the region)"})
}

/// `hoi4_air_regions`: own wings and air bases grouped by strategic air region (memory and game
/// files only; the enemy's presence is what `hoi4_air_region_probe` reads from the air map).
pub(super) fn regions(process: &dyn Process) -> Result<Value> {
    use std::collections::BTreeMap;
    let (bases, wings) = own(process)?;
    let names = game_dir(process).map(|d| airread::region_names(&d)).unwrap_or_default();
    let mut rows: BTreeMap<i32, (Vec<Value>, Vec<Value>)> = BTreeMap::new();
    for w in &wings {
        if w.region_id > 0 {
            rows.entry(w.region_id).or_default().0.push(json!({"wing":w.name,"aircraft":w.planes,
                "aircraft_type":w.equipment,"missions":airread::missions_in(w.mission_mask),
                "air_base_state":w.base_state}));
        }
    }
    for b in &bases {
        let used: i32 = wings.iter().filter(|w| w.base_id == b.id).map(|w| w.planes).sum();
        let region = b.region_id;
        if let Some(region) = region {
            rows.entry(region).or_default().1.push(json!({"state_id":b.state,"capacity":b.capacity,
                "aircraft_stationed":used,"free":b.capacity - used,
                "wings":wings.iter().filter(|w| w.base_id == b.id).map(|w| w.name.as_str()).collect::<Vec<_>>()}));
        }
    }
    let idle: Vec<&str> = wings.iter().filter(|w| w.region_id <= 0).map(|w| w.name.as_str()).collect();
    Ok(json!({
        "regions":rows.into_iter().map(|(id, (w, b))| json!({"region_id":id,"name":names.get(&id),
            "wings_assigned":w.len(),"aircraft_assigned":w.iter().map(|x| x["aircraft"].as_i64().unwrap_or(0)).sum::<i64>(),
            "wings":w,"air_bases_here":b})).collect::<Vec<_>>(),
        "wings_without_region":idle,
        "enemy_air":"Not in memory: hoi4_air_region_probe {state_id} opens the air map's region window and reads what it shows (active aircraft by type, detection and efficiency)",
        "validated":{"wings_by_region":"wing +0xb8 points at the CStrategicRegion; its file id (+0x40) matched the overview's Area column for Western and Eastern Germany","bases_by_region":"the base province's region pointer (+0xc8), file id +0x40"}
    }))
}

// ---- region arguments ---------------------------------------------------------------------------
/// A region id or display name (case-insensitive, unique substring allowed) -> region id.
pub(super) fn pick_region(names: &std::collections::BTreeMap<i32, String>, wanted: &str) -> Result<i32> {
    let wanted = wanted.trim();
    if let Ok(id) = wanted.parse::<i32>() {
        if names.is_empty() || names.contains_key(&id) {
            return Ok(id);
        }
    } else {
        let exact: Vec<i32> = names.iter().filter(|(_, n)| same(n, wanted)).map(|(i, _)| *i).collect();
        if exact.len() == 1 {
            return Ok(exact[0]);
        }
        let lower = wanted.to_lowercase();
        let part: Vec<i32> = names
            .iter()
            .filter(|(_, n)| n.to_lowercase().contains(&lower))
            .map(|(i, _)| *i)
            .collect();
        if part.len() == 1 {
            return Ok(part[0]);
        }
    }
    Err(Error::new(
        "region_unknown",
        format!(
            "No unique strategic air region {wanted:?}; examples: {}",
            names.values().take(8).cloned().collect::<Vec<_>>().join(" | ")
        ),
    ))
}
/// The state inside `region` with the largest share of its provinces there (ties: more provinces,
/// lower id).
pub(super) fn best_state(map: &mapdata::StateRegions, region: i32) -> Option<i32> {
    map.iter()
        .filter(|(_, (r, _, _))| *r == region)
        .max_by(|a, b| {
            let share = |v: &(i32, usize, usize)| v.1 as f64 / v.2.max(1) as f64;
            share(a.1)
                .partial_cmp(&share(b.1))
                .unwrap()
                .then(a.1 .1.cmp(&b.1 .1))
                .then(b.0.cmp(a.0))
        })
        .map(|(s, _)| *s)
}
/// The state to centre on and the region to verify afterwards (when `region` was named).
pub(super) fn resolve_region(
    process: &dyn Process,
    state: Option<i32>,
    region: Option<String>,
) -> Result<(i32, Option<i32>)> {
    let Some(text) = region else {
        return state
            .map(|s| (s, None))
            .ok_or_else(|| Error::new("invalid_arguments", "Give state_id or region"));
    };
    let dir = game_dir(process)
        .ok_or_else(|| Error::new("map_data_unavailable", "Game install not found; give state_id only"))?;
    let id = pick_region(&airread::region_names(&dir), &text)?;
    let states = mapdata::state_regions(&dir)?;
    match state {
        Some(s) => match states.get(&s) {
            Some((r, _, _)) if *r != id => Err(Error::new(
                "region_state_mismatch",
                format!("State {s} lies mostly in region {r}, not {id} ({text})"),
            )),
            _ => Ok((s, Some(id))),
        },
        None => best_state(&states, id)
            .map(|s| (s, Some(id)))
            .ok_or_else(|| Error::new("region_unknown", format!("No state lies in region {text:?}"))),
    }
}

// ---- arguments ----------------------------------------------------------------------------------
pub(super) fn text_arg(args: &serde_json::Map<String, Value>, key: &str, max: usize) -> Result<String> {
    let v = args
        .get(key)
        .and_then(Value::as_str)
        .ok_or_else(|| Error::new("invalid_arguments", format!("{key} must be a string")))?;
    if v.trim().is_empty() || v.len() > max || v.chars().any(char::is_control) {
        return Err(Error::new("invalid_arguments", format!("Invalid {key}")));
    }
    Ok(v.to_string())
}
pub(super) fn state_arg(args: &serde_json::Map<String, Value>, key: &str) -> Result<i32> {
    args.get(key)
        .and_then(Value::as_i64)
        .filter(|v| (1..4096).contains(v))
        .map(|v| v as i32)
        .ok_or_else(|| Error::new("invalid_arguments", format!("{key} must be 1..4095")))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn base(id: i32, state: i32, capacity: i32) -> Base {
        Base { id, state: Some(state), province: 1, capacity, region_id: None }
    }
    fn wing(name: &str, planes: i32, base_id: i32, state: i32) -> Wing {
        Wing {
            name: name.into(),
            unit: "light_plane".into(),
            equipment: "fighter_equipment_0".into(),
            variant: String::new(),
            planes,
            manpower: planes * 22,
            mission_mask: 0,
            region_id: 0,
            region_name: None,
            base_id,
            base_state: Some(state),
            transfer_base_id: 0,
        }
    }
    fn fixture() -> (Vec<Base>, Vec<Wing>) {
        let bomber = |mut w: Wing| {
            w.equipment = "tac_bomber_equipment_0".into();
            w
        };
        (
            vec![base(9, 64, 1200), base(10, 66, 1200), base(3, 52, 400)],
            vec![
                wing("Jagdgeschwader 132", 80, 9, 64),
                bomber(wing("KG 1 'Hindenburg'", 80, 9, 64)),
                bomber(wing("Kampfgeschwader 253", 60, 10, 66)),
                wing("Jagdgeschwader 136", 40, 3, 52),
            ],
        )
    }
    #[test]
    fn rebase_refusals_come_before_input() {
        let (b, w) = fixture();
        let p = plan_rebase(&b, &w, "jagdgeschwader 132", 66).unwrap();
        assert_eq!((p.to_base_id, p.planes, p.from_state), (10, 80, Some(64)));
        assert_eq!(plan_rebase(&b, &w, "Nope", 66).unwrap_err().code, "wing_unknown");
        assert_eq!(plan_rebase(&b, &w, "Jagdgeschwader 132", 99).unwrap_err().code, "air_base_unknown");
        assert_eq!(plan_rebase(&b, &w, "Jagdgeschwader 132", 64).unwrap_err().code, "nothing_to_change");
        // 80 aircraft do not fit the 400-capacity base that already holds 40 and gets 300 incoming.
        let mut w2 = w.clone();
        w2.push(wing("Incoming", 300, 10, 66));
        w2.last_mut().unwrap().transfer_base_id = 3;
        assert_eq!(plan_rebase(&b, &w2, "Jagdgeschwader 132", 52).unwrap_err().code, "air_base_full");
        let mut flying = w.clone();
        flying[0].transfer_base_id = 10;
        assert_eq!(plan_rebase(&b, &flying, "Jagdgeschwader 132", 66).unwrap_err().code, "nothing_to_change");
        let mut dup = w.clone();
        dup.push(wing("Jagdgeschwader 132", 1, 9, 64));
        assert_eq!(plan_rebase(&b, &dup, "Jagdgeschwader 132", 66).unwrap_err().code, "wing_ambiguous");
    }
    #[test]
    fn rebase_settlement_compares_only_base_and_destination() {
        let (b, w) = fixture();
        let p = plan_rebase(&b, &w, "Jagdgeschwader 132", 66).unwrap();
        let mut now = p.before.clone();
        assert_eq!(settle_rebase(&p.before, Some(&now), 10), Verdict::Unchanged);
        now.planes = 79; // attrition elsewhere does not matter
        now.mission_mask = 4;
        assert_eq!(settle_rebase(&p.before, Some(&now), 10), Verdict::Unchanged);
        now.transfer_base_id = 10;
        assert_eq!(settle_rebase(&p.before, Some(&now), 10), Verdict::Reached);
        now.transfer_base_id = 3;
        assert_eq!(settle_rebase(&p.before, Some(&now), 10), Verdict::Diverged);
        now.transfer_base_id = 0;
        now.base_id = 10;
        assert_eq!(settle_rebase(&p.before, Some(&now), 10), Verdict::Reached);
        assert_eq!(settle_rebase(&p.before, None, 10), Verdict::Unknown);
    }
    #[test]
    fn create_plans_and_settlement() {
        let (b, w) = fixture();
        let p = plan_create(&b, &w, 64, "Ju 86").unwrap();
        assert_eq!((p.base_id, p.names_before.len()), (9, 2));
        assert_eq!(plan_create(&b, &w, 99, "x").unwrap_err().code, "air_base_unknown");
        let mut full = w.clone();
        full.push(wing("Fill", 1040, 3, 52));
        assert_eq!(plan_create(&b, &full, 52, "x").unwrap_err().code, "air_base_full");
        let mut after = w.clone();
        assert_eq!(settle_create(&p, &after), (Verdict::Unchanged, vec![]));
        after.push(wing("KG 3 'Blitz'", 40, 9, 64));
        assert_eq!(settle_create(&p, &after), (Verdict::Reached, vec!["KG 3 'Blitz'".to_string()]));
        after.push(wing("Another", 40, 9, 64));
        assert_eq!(settle_create(&p, &after).0, Verdict::Diverged);
    }
    #[test]
    fn reserve_rows_are_matched_by_variant_or_class() {
        let rect = crate::gui_tree::Rect { x: 0, y: 0, w: 10, h: 10 };
        let e = |v: &str, c: &str, n: i32| airui::Entry { variant: v.into(), class: c.into(), count: n, bg: rect };
        let rows = vec![e("Ju 86", "Inter-War Bomber", 40), e("He 51", "Inter-War Fighter", 120)];
        assert_eq!(pick_entry(&rows, "ju 86").unwrap().count, 40);
        assert_eq!(pick_entry(&rows, "Inter-War Fighter").unwrap().variant, "He 51");
        assert_eq!(pick_entry(&rows, "fighter").unwrap().variant, "He 51");
        assert_eq!(pick_entry(&rows, "Inter-War").unwrap_err().code, "aircraft_ambiguous");
        let err = pick_entry(&rows, "Bf 109").unwrap_err();
        assert_eq!(err.code, "aircraft_not_in_reserve");
        assert!(err.message.contains("Ju 86"));
        assert!(pick_entry(&[], "x").unwrap_err().message.contains("none"));
    }
    #[test]
    fn split_and_merge_plans() {
        let (_, mut w) = fixture();
        w.push(wing("Jagdgeschwader 133", 40, 9, 64));
        let p = plan_split(&w, "jagdgeschwader 132").unwrap();
        assert_eq!((p.planes, p.names_before.len()), (80, 3));
        assert_eq!(plan_split(&w, "Nope").unwrap_err().code, "wing_unknown");
        let mut flying = w.clone();
        flying[0].transfer_base_id = 10;
        assert_eq!(plan_split(&flying, "Jagdgeschwader 132").unwrap_err().code, "wing_in_transfer");
        let mut tiny = w.clone();
        tiny[0].planes = 1;
        assert_eq!(plan_split(&tiny, "Jagdgeschwader 132").unwrap_err().code, "nothing_to_change");
        let mut after = w.clone();
        assert_eq!(settle_split(&p, &after), (Verdict::Unchanged, vec![]));
        after[0].planes = 40;
        after.push(wing("Jagdgeschwader 134", 40, 9, 64));
        assert_eq!(settle_split(&p, &after), (Verdict::Reached, vec!["Jagdgeschwader 134".to_string()]));
        // Merge: the game joins every wing of one type at a base.
        let m = plan_merge(&w, "Jagdgeschwader 132", Some("Jagdgeschwader 133")).unwrap();
        assert_eq!((m.total, m.group.len()), (120, 2));
        assert_eq!(plan_merge(&w, "Jagdgeschwader 132", None).unwrap().group.len(), 2);
        assert_eq!(plan_merge(&w, "Jagdgeschwader 132", Some("Jagdgeschwader 132")).unwrap_err().code, "invalid_arguments");
        assert_eq!(plan_merge(&w, "Jagdgeschwader 132", Some("Kampfgeschwader 253")).unwrap_err().code, "merge_different_base");
        assert_eq!(plan_merge(&w, "Kampfgeschwader 253", None).unwrap_err().code, "no_merge_partner");
        let other = w.clone();
        assert_eq!(plan_merge(&other, "Jagdgeschwader 132", Some("KG 1 'Hindenburg'")).unwrap_err().code, "merge_different_type");
        assert_eq!(archetype("tac_bomber_equipment_1"), "tac_bomber");
        assert_eq!(archetype("CAS_equipment_1"), "CAS");
        let mut three = w.clone();
        three.push(wing("Jagdgeschwader 135", 20, 9, 64));
        assert_eq!(plan_merge(&three, "Jagdgeschwader 132", Some("Jagdgeschwader 133")).unwrap_err().code, "merge_group_larger");
        assert_eq!(plan_merge(&three, "Jagdgeschwader 132", None).unwrap().total, 140);
        let mut flying = w.clone();
        flying[4].transfer_base_id = 3;
        assert_eq!(plan_merge(&flying, "Jagdgeschwader 132", None).unwrap_err().code, "wing_in_transfer");
        let mut merged = w.clone();
        merged.retain(|x| x.name != "Jagdgeschwader 133");
        merged[0].planes = 120;
        assert_eq!(settle_merge(&m, &merged), Verdict::Reached);
        assert_eq!(settle_merge(&m, &w), Verdict::Unchanged);
        merged[0].planes = 100;
        assert_eq!(settle_merge(&m, &merged), Verdict::Diverged);
        // Three wings of 40, 80 and 40 end as 100 and 60 (the game's wing size cap).
        let mut cap = fixture().1;
        cap.push(wing("Kampfgeschwader 155", 80, 9, 64));
        cap.push(wing("KG 3 'Blitz'", 40, 9, 64));
        cap.retain(|x| x.name != "KG 1 'Hindenburg'");
        cap[0].planes = 40;
        let three_plan = plan_merge(&cap, "Jagdgeschwader 132", None).unwrap();
        assert_eq!(three_plan.total, 160);
        cap.retain(|x| x.name != "KG 3 'Blitz'");
        cap.iter_mut().find(|x| x.name == "Jagdgeschwader 132").unwrap().planes = 100;
        cap.iter_mut().find(|x| x.name == "Kampfgeschwader 155").unwrap().planes = 60;
        assert_eq!(settle_merge(&three_plan, &cap), Verdict::Reached);
    }
    #[test]
    fn regions_resolve_by_id_name_and_best_state() {
        let mut names = std::collections::BTreeMap::new();
        names.insert(7, "Eastern Germany".to_string());
        names.insert(8, "Western Germany".to_string());
        names.insert(9, "Baltic Sea".to_string());
        assert_eq!(pick_region(&names, "7").unwrap(), 7);
        assert_eq!(pick_region(&names, "eastern germany").unwrap(), 7);
        assert_eq!(pick_region(&names, "Baltic").unwrap(), 9);
        assert_eq!(pick_region(&names, "Germany").unwrap_err().code, "region_unknown");
        assert_eq!(pick_region(&names, "99").unwrap_err().code, "region_unknown");
        let mut map = std::collections::HashMap::new();
        map.insert(64, (7, 5, 5));
        map.insert(65, (7, 2, 4));
        map.insert(66, (8, 3, 3));
        assert_eq!(best_state(&map, 7), Some(64));
        assert_eq!(best_state(&map, 8), Some(66));
        assert_eq!(best_state(&map, 9), None);
    }
    #[test]
    fn disband_hold_and_mission_settlement() {
        let (_, mut w) = fixture();
        let before = plan_disband(&w, "Kampfgeschwader 253").unwrap();
        assert_eq!(plan_disband(&w, "Nope").unwrap_err().code, "wing_unknown");
        assert_eq!(settle_disband(&before, &w), Verdict::Unchanged);
        w.retain(|x| x.name != "Kampfgeschwader 253");
        assert_eq!(settle_disband(&before, &w), Verdict::Reached);
        let (_, mut w) = fixture();
        assert_eq!(plan_hold(&w, "Jagdgeschwader 132").unwrap_err().code, "nothing_to_change");
        w[0].mission_mask = 1;
        w[0].region_id = 7;
        let b = plan_hold(&w, "Jagdgeschwader 132").unwrap();
        assert_eq!(settle_hold(&b, Some(&snap(&w[0]))), Verdict::Unchanged);
        let mut held = snap(&w[0]);
        held.mission_mask = 0;
        held.region_id = 0;
        assert_eq!(settle_hold(&b, Some(&held)), Verdict::Reached);
        let nothing = snap(&fixture().1[0]);
        assert_eq!(settle_mission(&nothing, Some(&b), 1, Some(7)), Verdict::Reached);
        assert_eq!(settle_mission(&nothing, Some(&b), 1, Some(8)), Verdict::Diverged);
        assert_eq!(settle_mission(&nothing, Some(&nothing), 1, None), Verdict::Unchanged);
        assert_eq!(settle_mission(&nothing, None, 1, None), Verdict::Unknown);
    }
}
