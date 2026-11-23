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
        game_input::{GameInput, Key},
        Process,
    },
};
use serde_json::{json, Value};
use std::time::Duration;

const GAME: (u32, u32) = airui::GAME;
const F1: u16 = 122;
const F3: u16 = 99;

// ---- memory ------------------------------------------------------------------------------------
pub(super) fn own(process: &dyn Process) -> Result<(Vec<Base>, Vec<Wing>)> {
    read(process, |r, image| {
        let own =
            fleetmem::own_strategic(r, image, fleetmem::GS_AIR_MANAGER, fleetmem::AIR_BY_COUNTRY)?;
        airread::bases_and_wings(r, image, own)
    })
}
/// One own wing by exact name, straight from memory.
pub(super) fn own_wing(
    r: &crate::memory::Reader<'_>,
    image: u64,
    name: &str,
) -> Result<Option<Wing>> {
    let own =
        fleetmem::own_strategic(r, image, fleetmem::GS_AIR_MANAGER, fleetmem::AIR_BY_COUNTRY)?;
    Ok(airread::bases_and_wings(r, image, own)?
        .1
        .into_iter()
        .find(|w| w.name == name))
}
/// Probe only (macprobe `dbg_wing:NAME[:LEN]`): the own wing's address and raw words, for
/// mapping fields such as the army a wing is attached to.
pub(super) fn debug_wing(process: &dyn Process, spec: &str) -> Result<Value> {
    let (name, len) = match spec.rsplit_once(':') {
        Some((a, n)) if n.parse::<usize>().is_ok() => (a, n.parse::<usize>().unwrap()),
        _ => (spec, 0xa00),
    };
    read(process, |r, i| {
        let w = own_wing(r, i, name)?
            .ok_or_else(|| Error::new("wing_unknown", "No own wing with this name"))?;
        let bytes = r.read(w.address, len.min(0x2000))?;
        let words: Vec<String> = bytes
            .chunks(8)
            .map(|c| {
                let mut x = [0u8; 8];
                x[..c.len()].copy_from_slice(c);
                format!("{:016x}", u64::from_le_bytes(x))
            })
            .collect();
        Ok(json!({"wing": name, "address": format!("{:#x}", w.address),
            "mission_mask": w.mission_mask, "region_id": w.region_id, "words": words}))
    })
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
                wings
                    .iter()
                    .map(|w| w.name.as_str())
                    .collect::<Vec<_>>()
                    .join(" | ")
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
    bases
        .iter()
        .find(|b| b.state == Some(state))
        .ok_or_else(|| {
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
pub(super) fn plan_create(
    bases: &[Base],
    wings: &[Wing],
    state: i32,
    aircraft: &str,
) -> Result<CreatePlan> {
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
pub(super) fn pick_entry<'a>(
    entries: &'a [airui::Entry],
    aircraft: &str,
) -> Result<&'a airui::Entry> {
    let exact: Vec<&airui::Entry> = entries
        .iter()
        .filter(|e| same(&e.variant, aircraft) || same(&e.class, aircraft))
        .collect();
    let hits = if exact.is_empty() {
        let want = aircraft.trim().to_lowercase();
        entries
            .iter()
            .filter(|e| {
                e.variant.to_lowercase().contains(&want) || e.class.to_lowercase().contains(&want)
            })
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
        return Err(Error::new(
            "wing_in_transfer",
            format!("{} is flying a transfer", w.name),
        ));
    }
    if w.planes < 2 {
        return Err(Error::new(
            "nothing_to_change",
            format!("{} has too few aircraft to split", w.name),
        ));
    }
    Ok(SplitPlan {
        wing: w.name.clone(),
        planes: w.planes,
        base_id: w.base_id,
        names_before: wings
            .iter()
            .filter(|o| o.base_id == w.base_id)
            .map(|o| o.name.clone())
            .collect(),
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
    let original = at_base
        .iter()
        .find(|w| w.name == plan.wing)
        .map(|w| w.planes);
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
                format!(
                    "{} flies {} and {} flies {}; only wings of one aircraft type merge",
                    wa.name,
                    archetype(&wa.equipment),
                    wb.name,
                    archetype(&wb.equipment)
                ),
            ));
        }
    }
    if group.len() < 2 {
        return Err(Error::new(
            "no_merge_partner",
            format!(
                "{} is the only {} wing at its air base",
                wa.name,
                archetype(&wa.equipment)
            ),
        ));
    }
    if group.iter().any(|w| w.transfer_base_id != 0) {
        return Err(Error::new(
            "wing_in_transfer",
            "A wing in transfer cannot be merged",
        ));
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
    let left: Vec<&Wing> = wings
        .iter()
        .filter(|w| plan.group.contains(&w.name))
        .collect();
    let held: i32 = left.iter().map(|w| w.planes).sum();
    let unchanged = plan.group.iter().zip(&plan.planes).all(|(n, p)| {
        wings
            .iter()
            .find(|w| w.name == *n)
            .is_some_and(|w| w.planes == *p)
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
pub(super) fn settle_mission(
    before: &WingSnap,
    now: Option<&WingSnap>,
    mask: i32,
    region: Option<i32>,
) -> Verdict {
    match now {
        Some(n) if n.mission_mask & mask == mask && region.is_none_or(|r| n.region_id == r) => {
            Verdict::Reached
        }
        Some(n) if n.mission_mask == before.mission_mask && n.region_id == before.region_id => {
            Verdict::Unchanged
        }
        Some(_) => Verdict::Diverged,
        None => Verdict::Unknown,
    }
}
// ---- geometry and ranges -----------------------------------------------------------------------
fn game_dir(process: &dyn Process) -> Option<std::path::PathBuf> {
    process
        .identity()
        .and_then(|i| mapdata::game_dir(&i.exe).ok())
}
/// Base position (x, z) of a state from the game's building positions.
pub(super) fn base_position(process: &dyn Process, state: i32) -> Result<(f64, f64)> {
    let dir = game_dir(process)
        .ok_or_else(|| Error::new("map_data_unavailable", "Game install not found"))?;
    mapdata::air_base_positions(&dir)?
        .get(&state)
        .copied()
        .ok_or_else(|| {
            Error::new(
                "air_base_unknown",
                format!("No air base position for state {state}"),
            )
        })
}
// ---- driving the air UI -------------------------------------------------------------------------
fn click(keys: &dyn GameInput, x: f64, y: f64) -> Result<()> {
    keys.click(GAME, x, y).map(|_| ())
}
#[track_caller]
fn pause(ms: u64) {
    crate::phase::sleep(Duration::from_millis(ms));
}
fn icons_shown(r: &crate::memory::Reader<'_>, image: u64) -> Result<bool> {
    Ok(airui::base_icons(r, image)?.iter().any(|i| i.strategic))
}
/// Make sure the strategic air map (F3) is on; returns whether it was switched on here.
/// Air mode is seen through the air base icons, which the game draws only near the camera
/// and not when it is far out; the Find view to an own base (1.5 s) is the last resort.
fn enter_air_mode(process: &dyn Process, keys: &dyn GameInput) -> Result<bool> {
    if read(process, icons_shown)? {
        return Ok(false);
    }
    // Far out, no icons are drawn: come in to the strategic height first (one swoop).
    if read(process, super::screenmap::camera_height).is_ok_and(|h| h > 825.0) {
        let _ = super::screenmap::zoom_in_to_strategic(process, keys);
        if read(process, icons_shown)? {
            return Ok(false);
        }
    }
    keys.press_code(F3)?;
    if wait(process, "strategic air map mode", icons_shown).is_ok() {
        pause(300);
        return Ok(true);
    }
    // No icons near the camera (a camera left over the Sahara, live 6 Nov 2026), or F3 toggled
    // an air mode that was already on (no icons drawn) off: centre an own air base state.
    if let Some(state) = own(process)
        .ok()
        .and_then(|(bases, _)| bases.iter().find_map(|b| b.state))
    {
        let _ = super::ui::find_state(process, keys, state);
        pause(350);
        if read(process, icons_shown)? {
            return Ok(true);
        }
    }
    let _ = super::screenmap::zoom_in_to_strategic(process, keys);
    if read(process, icons_shown)? {
        return Ok(true);
    }
    keys.press_code(F3)?;
    if wait(process, "strategic air map mode", icons_shown).is_err() {
        // Far out, the game draws no air base icons: come in to the strategic height.
        super::screenmap::zoom_in_to_strategic(process, keys)?;
        if wait(process, "strategic air map mode", icons_shown).is_err() {
            // F3 toggles: when air mode was already on (far out, no icons drawn), the press
            // above switched it off (seen live 6 Nov 2026 after the Air Forces Overview). Press
            // it once more at the strategic height.
            keys.press_code(F3)?;
            wait(process, "strategic air map mode", icons_shown)?;
        }
    }
    pause(300);
    Ok(true)
}
fn leave_air_mode(process: &dyn Process, keys: &dyn GameInput, entered: bool) {
    if entered {
        let _ = keys.press_code(F1);
        let _ = wait(process, "default map mode", |r, i| Ok(!icons_shown(r, i)?));
    }
}
/// Tidy: close the wing window and the selection toolbar (best effort).
fn release(process: &dyn Process, keys: &dyn GameInput, entered: bool) {
    if read(process, airui::reorg_open).unwrap_or(false) {
        if let Ok(rect) = settled(process, "wing window close", airui::reorg_close) {
            let (x, y) = rect.center();
            let _ = click(keys, x, y);
            let _ = wait(process, "wing window closed", |r, i| {
                Ok(!airui::reorg_open(r, i)?)
            });
        }
    }
    let _ = air::deselect(process, keys);
    leave_air_mode(process, keys, entered);
}
/// Centre the camera on `state` and return the screen point of its air base circle.
fn base_point(process: &dyn Process, keys: &dyn GameInput, state: i32) -> Result<(f64, f64)> {
    let pos = base_position(process, state)?;
    // already on screen (the wing before went to the same base): no Find view, no camera travel
    super::screenmap::wait_camera_still(process, 400);
    if let Some(p) = read(process, |r, i| {
        Ok(airui::icon_at(&airui::base_icons(r, i)?, pos).map(|icon| icon.center()))
    })?
    .filter(|p| airui::clickable(*p))
    {
        return Ok(p);
    }
    find_state(process, keys, state)?;
    // At most eight zoom-out wheel steps in all, as before, but sized and sent as one flick.
    let mut left: u32 = 8;
    loop {
        let found = settled(process, "air base icon", |r, i| {
            Ok(airui::icon_at(&airui::base_icons(r, i)?, pos).map(|icon| icon.center()))
        })?;
        if let Some(p) = found.filter(|p| airui::clickable(*p)) {
            return Ok(p);
        }
        if left == 0 {
            break;
        }
        // No icon drawn: where the base projects on screen.
        let at = found.or_else(|| {
            read(process, |r, i| {
                super::screenmap::to_screen(r, i, pos.0, pos.1)
            })
            .ok()
            .flatten()
        });
        // The base lies off the usable map area: zoom out around the screen centre by the
        // steps its offset needs (one when it sits in a toolbar corner or is not projected).
        let events = at
            .map(|p| super::glide::zoom_out_events(p, (660.0, 160.0, 1740.0, 960.0), left))
            .unwrap_or(0)
            .clamp(1, left);
        keys.scroll_burst(GAME, (960.0, 540.0), -240, events)?;
        left -= events;
        pause(450);
    }
    // Last try from a known height: the strategic zoom, centred again (the zoom-out search
    // missed Eritrea's base once and found it on the next call, live 6 Nov 2026).
    let _ = super::screenmap::zoom_in_to_strategic(process, keys);
    find_state(process, keys, state)?;
    pause(450);
    if let Some(p) = settled(process, "air base icon", |r, i| {
        Ok(airui::icon_at(&airui::base_icons(r, i)?, pos).map(|icon| icon.center()))
    })?
    .filter(|p| airui::clickable(*p))
    {
        return Ok(p);
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
pub(super) fn rebase(
    process: &dyn Process,
    keys: &dyn GameInput,
    plan: &RebasePlan,
) -> Result<Value> {
    clear_notices(process, keys)?;
    let entered = enter_air_mode(process, keys)?;
    let result = rebase_one(process, keys, plan);
    release(process, keys, entered);
    result
}
/// The wing selected, the base's circle right-clicked, the transfer read back (in air mode).
fn rebase_one(process: &dyn Process, keys: &dyn GameInput, plan: &RebasePlan) -> Result<Value> {
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
    Ok(
        json!({"wing":plan.wing,"to_state":plan.state,"planes":plan.planes,"from_state":plan.from_state}),
    )
}
/// Several rebases in one call: one air mode, the camera left on a base the next wing goes to
/// too (base_point finds its circle on screen without the Find view). Returns each wing's
/// outcome (None = its transfer started).
pub(super) fn rebase_many(
    process: &dyn Process,
    keys: &dyn GameInput,
    plans: &[RebasePlan],
) -> Result<Vec<(String, Option<Error>)>> {
    clear_notices(process, keys)?;
    let entered = enter_air_mode(process, keys)?;
    let mut out = Vec::new();
    for plan in plans {
        let result = rebase_one(process, keys, plan);
        if read(process, airui::reorg_open).unwrap_or(false) {
            release(process, keys, false);
        } else {
            let _ = air::deselect(process, keys);
        }
        if let Err(e) = &result {
            if matches!(
                e.code,
                "system_dialog_open" | "input_target_not_frontmost" | "action_deadline"
            ) {
                leave_air_mode(process, keys, entered);
                return Err(result.unwrap_err());
            }
        }
        out.push((plan.wing.clone(), result.err()));
    }
    leave_air_mode(process, keys, entered);
    Ok(out)
}

// ---- bulk: a base click selects every wing at the base -----------------------------------------
// In the strategic air map a left click on an air base circle selects every wing stationed there
// (the selection view lists them: airselectionview.gui airbase_selection_item,
// airwing_selection_item); the mission toolbar then gives the mission to every selected wing and
// one right click on the region (or on another base's circle) orders them all. A base is used
// only when every wing at it is asked for (the selection is then exactly the wanted wings, read
// back from the selection view's own array); wings at mixed bases, and any wing the bulk order
// did not reach, take the single path (the Air Forces Overview row) in the same call.

/// The wings at one air base that one base click orders.
#[derive(Clone, Debug, PartialEq)]
pub(super) struct BaseGroup {
    pub state: i32,
    /// every wing stationed at the base (the selection a click on its circle makes)
    pub wings: Vec<String>,
}
/// The bases whose wings are all asked for and at least one still pending, fewest moves first
/// (most pending wings first); `rows` = (wing, base state) of every own wing; `skip` = base
/// states not to use (the target base of a rebase, bases already tried). Pure.
pub(super) fn clean_bases(
    asked: &[String],
    pending: &[String],
    rows: &[(String, Option<i32>)],
    skip: &[i32],
) -> Vec<BaseGroup> {
    let mut states: Vec<i32> = rows
        .iter()
        .filter(|(n, _)| pending.contains(n))
        .filter_map(|(_, s)| *s)
        .filter(|s| !skip.contains(s))
        .collect();
    states.sort_unstable();
    states.dedup();
    let mut out: Vec<(usize, BaseGroup)> = states
        .into_iter()
        .filter_map(|state| {
            let wings: Vec<String> = rows
                .iter()
                .filter(|(_, s)| *s == Some(state))
                .map(|(n, _)| n.clone())
                .collect();
            let clean = wings.iter().all(|w| asked.contains(w));
            let waiting = wings.iter().filter(|w| pending.contains(*w)).count();
            (clean && waiting > 1).then_some((waiting, BaseGroup { state, wings }))
        })
        .collect();
    out.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.state.cmp(&b.1.state)));
    out.into_iter().map(|(_, g)| g).collect()
}
/// The same wings, any order. Pure.
pub(super) fn same_wings(a: &[String], b: &[String]) -> bool {
    let mut a: Vec<&String> = a.iter().collect();
    let mut b: Vec<&String> = b.iter().collect();
    a.sort();
    a.dedup();
    b.sort();
    b.dedup();
    a == b
}
/// Select every wing at `group`'s base with one click on its circle; the selection read back
/// must be exactly the group's wings.
fn select_base(process: &dyn Process, keys: &dyn GameInput, group: &BaseGroup) -> Result<()> {
    air::deselect(process, keys)?;
    let (x, y) = base_point(process, keys, group.state)?;
    click(keys, x, y)?;
    wait(process, "air base selected", air::selection_open)?;
    let wanted = group.wings.clone();
    if wait(process, "base wings selected", move |r, i| {
        Ok(same_wings(&airui::selected_wings(r, i)?, &wanted))
    })
    .is_err()
    {
        let seen = read(process, airui::selected_wings).unwrap_or_default();
        let _ = air::deselect(process, keys);
        return Err(Error::new(
            "selection_not_the_base",
            format!(
                "The base click selected {} wing(s), not the base's {} ({})",
                seen.len(),
                group.wings.len(),
                seen.join(", ")
            ),
        ));
    }
    Ok(())
}
/// The selected wings (the group) given `mission` over `region` (the state's): one mission
/// button click for all (a second when the first toggled a mixed selection off), one right click.
fn order_base_mission(
    process: &dyn Process,
    keys: &dyn GameInput,
    group: &BaseGroup,
    mission: &str,
    state: i32,
    named: Option<i32>,
) -> Result<()> {
    let frame = air::mission_frame(mission)
        .ok_or_else(|| Error::new("invalid_arguments", "Unknown mission"))?;
    let bit = 1 << (frame - 1);
    let regionless = mission == "training";
    let (dir, region) = air::target_region(process, state, named);
    select_base(process, keys, group)?;
    let names = group.wings.clone();
    let all_bit = {
        let names = names.clone();
        move |r: &crate::memory::Reader<'_>, i: u64| -> Result<bool> {
            let own = fleetmem::own_strategic(r, i, fleetmem::GS_AIR_MANAGER, fleetmem::AIR_BY_COUNTRY)?;
            let (_, wings) = airread::bases_and_wings(r, i, own)?;
            Ok(names.iter().all(|n| {
                wings
                    .iter()
                    .any(|w| &w.name == n && w.mission_mask & bit != 0)
            }))
        }
    };
    let buttons = settled(process, "mission buttons", air::mission_buttons)?;
    let Some((_, button, enabled, _)) = buttons.iter().find(|(f, _, _, _)| *f == frame).cloned()
    else {
        let _ = air::deselect(process, keys);
        return Err(Error::new(
            "invalid_target",
            "Mission not offered for the base's wings",
        ));
    };
    if !enabled {
        let _ = air::deselect(process, keys);
        return Err(Error::new(
            "invalid_target",
            format!("The game disables {mission} for the base's wings"),
        ));
    }
    let (bx, by) = button.center();
    for _ in 0..2 {
        if read(process, &all_bit)? {
            break;
        }
        click(keys, bx, by)?;
        let _ = wait(process, "mission chosen for the base", &all_bit);
    }
    if !read(process, &all_bit)? {
        let _ = air::deselect(process, keys);
        return Err(Error::new(
            "action_not_applied",
            "The mission button did not give every wing of the base the mission",
        ));
    }
    if regionless {
        let _ = air::deselect(process, keys);
        return Ok(());
    }
    let targets = air::region_targets(process, keys, dir.as_deref(), state, region)?;
    super::screenmap::wait_camera_still(process, 600);
    let assigned = {
        let names = names.clone();
        move |r: &crate::memory::Reader<'_>, i: u64| -> Result<bool> {
            let own = fleetmem::own_strategic(r, i, fleetmem::GS_AIR_MANAGER, fleetmem::AIR_BY_COUNTRY)?;
            let (_, wings) = airread::bases_and_wings(r, i, own)?;
            Ok(names.iter().all(|n| {
                wings.iter().any(|w| {
                    &w.name == n
                        && w.mission_mask & bit != 0
                        && region.map_or(w.region_id > 0, |want| w.region_id == want)
                })
            }))
        }
    };
    let mut done = false;
    for target in targets {
        keys.right_click(GAME, target.0, target.1)?;
        let deadline = std::time::Instant::now() + Duration::from_millis(1200);
        while std::time::Instant::now() < deadline {
            if read(process, &assigned)? {
                done = true;
                break;
            }
            crate::phase::sleep(Duration::from_millis(40));
        }
        if done {
            break;
        }
    }
    let _ = air::deselect(process, keys);
    if done {
        Ok(())
    } else {
        Err(Error::new(
            "input_readback_failed",
            "Not every wing of the base took the region",
        ))
    }
}
/// A refusal that ends the whole call (no input can follow it).
fn fatal(e: &Error) -> bool {
    matches!(
        e.code,
        "system_dialog_open" | "input_target_not_frontmost" | "action_deadline"
    )
}
/// The bulk mission (bulk: true): the bases whose every wing is asked take one base click, one
/// mission click and one region click each; the wings left (mixed bases, any the bulk order did
/// not reach) take the single path. Returns each wing's outcome (None = it flies the mission
/// over the region) and what the bulk orders did (base state, wings, ok or why).
pub(super) fn missions_by_base(
    process: &dyn Process,
    keys: &dyn GameInput,
    wings: &[String],
    mission: &str,
    state: i32,
    region: Option<i32>,
) -> Result<(Vec<(String, Option<Error>)>, Vec<Value>)> {
    clear_notices(process, keys)?;
    let entered = enter_air_mode(process, keys)?;
    let frame = air::mission_frame(mission)
        .ok_or_else(|| Error::new("invalid_arguments", "Unknown mission"))?;
    let bit = 1 << (frame - 1);
    let (_, want_region) = air::target_region(process, state, region);
    let flying = |w: &Wing| {
        w.mission_mask & bit != 0
            && (mission == "training"
                || want_region.map_or(w.region_id > 0, |r| w.region_id == r))
    };
    let result = (|| -> Result<(Vec<(String, Option<Error>)>, Vec<Value>)> {
        let mut bulk = Vec::new();
        let mut tried: Vec<i32> = Vec::new();
        for _ in 0..12 {
            let (_, all) = own(process)?;
            let pending: Vec<String> = wings
                .iter()
                .filter(|n| all.iter().any(|w| &w.name == *n && !flying(w)))
                .cloned()
                .collect();
            let rows: Vec<(String, Option<i32>)> =
                all.iter().map(|w| (w.name.clone(), w.base_state)).collect();
            let Some(group) = clean_bases(wings, &pending, &rows, &tried).into_iter().next()
            else {
                break;
            };
            tried.push(group.state);
            match order_base_mission(process, keys, &group, mission, state, region) {
                Ok(()) => bulk.push(json!({"base_state": group.state, "wings": group.wings, "ok": true})),
                Err(e) if fatal(&e) => return Err(e),
                Err(e) => bulk.push(json!({"base_state": group.state, "wings": group.wings,
                    "ok": false, "code": e.code, "message": e.message})),
            }
        }
        let mut out = Vec::new();
        for name in wings {
            let (_, all) = own(process)?;
            if all.iter().any(|w| &w.name == name && flying(w)) {
                out.push((name.clone(), None));
                continue;
            }
            let result = air::assign(process, keys, name, mission, state, region);
            if let Err(e) = &result {
                if fatal(e) {
                    return Err(result.unwrap_err());
                }
            }
            out.push((name.clone(), result.err()));
        }
        Ok((out, bulk))
    })();
    leave_air_mode(process, keys, entered);
    result
}
/// The bulk rebase (bulk: true): the bases whose every wing is asked take one base click and one
/// right click on the target base's circle each; the wings left take the single path. Returns
/// each wing's outcome (None = its transfer started) and what the bulk orders did.
pub(super) fn rebase_by_base(
    process: &dyn Process,
    keys: &dyn GameInput,
    plans: &[RebasePlan],
) -> Result<(Vec<(String, Option<Error>)>, Vec<Value>)> {
    let Some(first) = plans.first() else {
        return Ok((Vec::new(), Vec::new()));
    };
    let (to_state, to) = (first.state, first.to_base_id);
    let asked: Vec<String> = plans.iter().map(|p| p.wing.clone()).collect();
    let moving = |w: &Wing| w.base_id == to || w.transfer_base_id == to;
    clear_notices(process, keys)?;
    let entered = enter_air_mode(process, keys)?;
    let result = (|| -> Result<(Vec<(String, Option<Error>)>, Vec<Value>)> {
        let mut bulk = Vec::new();
        let mut tried: Vec<i32> = vec![to_state];
        for _ in 0..12 {
            let (_, all) = own(process)?;
            let pending: Vec<String> = asked
                .iter()
                .filter(|n| all.iter().any(|w| &w.name == *n && !moving(w)))
                .cloned()
                .collect();
            let rows: Vec<(String, Option<i32>)> =
                all.iter().map(|w| (w.name.clone(), w.base_state)).collect();
            let Some(group) = clean_bases(&asked, &pending, &rows, &tried).into_iter().next()
            else {
                break;
            };
            tried.push(group.state);
            let ordered = (|| -> Result<()> {
                select_base(process, keys, &group)?;
                let (x, y) = base_point(process, keys, to_state)?;
                let wanted = group.wings.clone();
                if !same_wings(&read(process, airui::selected_wings)?, &wanted) {
                    return Err(Error::new(
                        "selection_lost",
                        "The base selection changed while the target base was brought on screen",
                    ));
                }
                keys.right_click(GAME, x, y)?;
                let names = group.wings.clone();
                wait(process, "base rebase accepted", move |r, i| {
                    let own = fleetmem::own_strategic(r, i, fleetmem::GS_AIR_MANAGER, fleetmem::AIR_BY_COUNTRY)?;
                    let (_, wings) = airread::bases_and_wings(r, i, own)?;
                    Ok(names.iter().all(|n| {
                        wings.iter().any(|w| {
                            &w.name == n && (w.base_id == to || w.transfer_base_id == to)
                        })
                    }))
                })
                .map_err(|_| {
                    Error::new(
                        "rebase_refused",
                        "Not every wing of the base started a transfer",
                    )
                })
            })();
            let _ = air::deselect(process, keys);
            match ordered {
                Ok(()) => bulk.push(json!({"base_state": group.state, "wings": group.wings, "ok": true})),
                Err(e) if fatal(&e) => return Err(e),
                Err(e) => bulk.push(json!({"base_state": group.state, "wings": group.wings,
                    "ok": false, "code": e.code, "message": e.message})),
            }
        }
        let mut out = Vec::new();
        for plan in plans {
            let (_, all) = own(process)?;
            if all.iter().any(|w| w.name == plan.wing && moving(w)) {
                out.push((plan.wing.clone(), None));
                continue;
            }
            let result = rebase_one(process, keys, plan);
            let _ = air::deselect(process, keys);
            if let Err(e) = &result {
                if fatal(e) {
                    return Err(result.unwrap_err());
                }
            }
            out.push((plan.wing.clone(), result.err()));
        }
        Ok((out, bulk))
    })();
    leave_air_mode(process, keys, entered);
    result
}

/// Select the base (left click on its circle), N, pick the reserve row, Ok.
pub(super) fn create_wing(
    process: &dyn Process,
    keys: &dyn GameInput,
    plan: &CreatePlan,
) -> Result<Value> {
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
        let reserve = settled(process, "reserve rows", |r, i| {
            airui::reorg_entries(r, i, "list_left")
        })?;
        let mut entry = pick_entry(&reserve, &plan.aircraft)?.clone();
        // A row scrolled out of the list takes no click (st2, 17 Nov 2026: four reserve rows of a
        // 1939 Germany showed, the Bf 110 lay below). The list's band: from the first row as the
        // window opens (scrolled to the top) down to the Ok button below the list (the grid itself
        // has no reviewed renderer to measure). Wheel over the list until the row fits the band.
        let top = reserve.iter().map(|e| e.bg.y).min().unwrap_or(0);
        let bottom = settled(process, "wing ok", airui::reorg_confirm)?.0.y - 8;
        let fits = |e: &airui::Entry| e.bg.y >= top - 2 && e.bg.y + e.bg.h <= bottom;
        let at = (entry.bg.center().0, f64::from(top + bottom) / 2.0);
        for _ in 0..40 {
            if bottom <= top || fits(&entry) {
                break;
            }
            keys.scroll(
                GAME,
                at,
                if entry.bg.y + entry.bg.h > bottom {
                    -120
                } else {
                    120
                },
            )?;
            pause(200);
            let rows = settled(process, "reserve rows", |r, i| {
                airui::reorg_entries(r, i, "list_left")
            })?;
            entry = pick_entry(&rows, &plan.aircraft)?.clone();
        }
        if bottom > top && !fits(&entry) {
            return Err(Error::new(
                "gui_target_unavailable",
                format!(
                    "The reserve row {} stayed outside the new wing window's list after scrolling",
                    entry.variant
                ),
            ));
        }
        let (cx, cy) = entry.bg.center();
        click(keys, cx, cy)?;
        wait(process, "planes moved into the wing", |r, i| {
            Ok(airui::reorg_planes(r, i)? > 0)
        })?;
        let planes = read(process, airui::reorg_planes)?;
        let (rect, state) = settled(process, "wing ok", airui::reorg_confirm)?;
        if state == 3 {
            return Err(Error::new(
                "create_unavailable",
                "The game disables Ok for this wing (manpower?)",
            ));
        }
        let (ox, oy) = rect.center();
        click(keys, ox, oy)?;
        wait(process, "wing founded", |r, i| {
            Ok(!airui::reorg_open(r, i)?)
        })?;
        Ok(
            json!({"state":plan.state,"variant":entry.variant,"class":entry.class,"planes_moved":planes,
            "reserve_before":entry.count}),
        )
    })();
    release(process, keys, entered);
    result
}

/// `air::assign` inside the strategic air map (switched on and back when it was off).
pub(super) fn mission(
    process: &dyn Process,
    keys: &dyn GameInput,
    wing: &str,
    mission: &str,
    state: i32,
    region: Option<i32>,
) -> Result<()> {
    clear_notices(process, keys)?;
    let entered = enter_air_mode(process, keys)?;
    let result = air::assign(process, keys, wing, mission, state, region);
    leave_air_mode(process, keys, entered);
    result
}

/// `air::assign_many` inside the strategic air map (one air mode for every wing listed).
pub(super) fn missions(
    process: &dyn Process,
    keys: &dyn GameInput,
    wings: &[String],
    mission: &str,
    state: i32,
    region: Option<i32>,
) -> Result<Vec<(String, Option<Error>)>> {
    clear_notices(process, keys)?;
    let entered = enter_air_mode(process, keys)?;
    let result = air::assign_many(process, keys, wings, mission, state, region);
    leave_air_mode(process, keys, entered);
    result
}

/// Select the wing, toolbar delete, confirm.
pub(super) fn disband(process: &dyn Process, keys: &dyn GameInput, wing: &str) -> Result<Value> {
    clear_notices(process, keys)?;
    let entered = enter_air_mode(process, keys)?;
    let result = (|| -> Result<Value> {
        air::select_wing(process, keys, wing)?;
        if !only_selected(process, wing)? {
            return Err(Error::new(
                "selection_lost",
                "The selection is not exactly this wing; nothing was disbanded",
            ));
        }
        let (rect, state) = settled(process, "disband button", |r, i| {
            airui::toolbar(r, i, "btn_delete")
        })?;
        if state == 3 {
            return Err(Error::new(
                "disband_unavailable",
                "The game disables Disband for this wing",
            ));
        }
        let (x, y) = rect.center();
        click(keys, x, y)?;
        if !accept_command(process, keys)? {
            return Err(Error::new(
                "input_readback_failed",
                "The disband confirmation did not appear",
            ));
        }
        let name = wing.to_string();
        wait(process, "wing disbanded", move |r, i| {
            let own =
                fleetmem::own_strategic(r, i, fleetmem::GS_AIR_MANAGER, fleetmem::AIR_BY_COUNTRY)?;
            let (_, wings) = airread::bases_and_wings(r, i, own)?;
            Ok(!wings.iter().any(|w| w.name == name))
        })?;
        Ok(json!({"wing":wing,"returned_to_reserve":true}))
    })();
    release(process, keys, entered);
    result
}

/// Select the wing, toolbar split (halves it; the game keeps both halves selected).
pub(super) fn split(
    process: &dyn Process,
    keys: &dyn GameInput,
    plan: &SplitPlan,
) -> Result<Value> {
    clear_notices(process, keys)?;
    let entered = enter_air_mode(process, keys)?;
    let result = (|| -> Result<Value> {
        air::select_wing(process, keys, &plan.wing)?;
        if !only_selected(process, &plan.wing)? {
            return Err(Error::new(
                "selection_lost",
                "The selection is not exactly this wing; nothing was split",
            ));
        }
        let (rect, state) = settled(process, "split button", |r, i| {
            airui::toolbar(r, i, "split_wing_btn")
        })?;
        if state == 3 {
            return Err(Error::new(
                "split_unavailable",
                "The game disables Split for this wing",
            ));
        }
        let (x, y) = rect.center();
        click(keys, x, y)?;
        let (name, base, before) = (plan.wing.clone(), plan.base_id, plan.planes);
        wait(process, "wing split", move |r, i| {
            let own =
                fleetmem::own_strategic(r, i, fleetmem::GS_AIR_MANAGER, fleetmem::AIR_BY_COUNTRY)?;
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
pub(super) fn merge(
    process: &dyn Process,
    keys: &dyn GameInput,
    plan: &MergePlan,
) -> Result<Value> {
    clear_notices(process, keys)?;
    let entered = enter_air_mode(process, keys)?;
    let result = (|| -> Result<Value> {
        let state = plan
            .state
            .ok_or_else(|| Error::new("air_base_unknown", "The wings' air base has no state"))?;
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
            return Err(Error::new(
                "selection_lost",
                "The selection is not exactly the first wing; nothing was merged",
            ));
        }
        base_point(process, keys, state)?;
        let items = settled(process, "wing stack", |r, i| {
            airui::stack_at(r, i, pos)?.ok_or_else(|| {
                Error::new(
                    "gui_target_unavailable",
                    "The base's wing stack is not shown",
                )
            })
        })?;
        // The stack item showing the group's aircraft total is the likely one.
        let mut tries: Vec<usize> = (0..items.len())
            .filter(|i| items[*i].planes == plan.total)
            .collect();
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
        let (rect, state_code) = settled(process, "merge button", |r, i| {
            airui::toolbar(r, i, "merge_wings_btn")
        })?;
        if state_code == 3 {
            return Err(Error::new(
                "merge_unavailable",
                "The game disables Merge for these wings",
            ));
        }
        let (x, y) = rect.center();
        click(keys, x, y)?;
        let (names, total) = (plan.group.clone(), plan.total);
        wait(process, "wings merged", move |r, i| {
            let own =
                fleetmem::own_strategic(r, i, fleetmem::GS_AIR_MANAGER, fleetmem::AIR_BY_COUNTRY)?;
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
pub(super) fn hold(process: &dyn Process, keys: &dyn GameInput, wing: &str) -> Result<Value> {
    clear_notices(process, keys)?;
    let entered = enter_air_mode(process, keys)?;
    let result = (|| -> Result<Value> {
        air::select_wing(process, keys, wing)?;
        if !only_selected(process, wing)? {
            return Err(Error::new(
                "selection_lost",
                "The selection is not exactly this wing; nothing was held",
            ));
        }
        let (rect, state) = settled(process, "hold button", |r, i| {
            airui::toolbar(r, i, "btn_hold")
        })?;
        if state == 3 {
            return Err(Error::new(
                "hold_unavailable",
                "The game disables Hold for this wing",
            ));
        }
        let (x, y) = rect.center();
        click(keys, x, y)?;
        let name = wing.to_string();
        wait(process, "wing held", move |r, i| {
            let own =
                fleetmem::own_strategic(r, i, fleetmem::GS_AIR_MANAGER, fleetmem::AIR_BY_COUNTRY)?;
            let (_, wings) = airread::bases_and_wings(r, i, own)?;
            Ok(wings.iter().any(|w| w.name == name && w.region_id <= 0))
        })?;
        Ok(json!({"wing":wing,"held":true}))
    })();
    release(process, keys, entered);
    result
}

/// Open the strategic air region window of the region around `state` and read it (orders nothing).
pub(super) fn probe_region(
    process: &dyn Process,
    keys: &dyn GameInput,
    state: i32,
) -> Result<Value> {
    clear_notices(process, keys)?;
    let entered = enter_air_mode(process, keys)?;
    let result = (|| -> Result<Value> {
        let _ = air::deselect(process, keys);
        find_state(process, keys, state)?;
        pause(300);
        // Below the centre: the camera centres on the state, where a capital's victory point
        // star takes the click and no region window opens (Berlin, state 64, Linux live).
        click(keys, 960.0, 620.0)?;
        let view = {
            wait(process, "air region window", |r, i| {
                Ok(airui::region_view(r, i)?.is_some())
            })?;
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
            let _ = wait(process, "region window closed", |r, i| {
                Ok(airui::region_view(r, i)?.is_none())
            });
        }
        Ok(json)
    })();
    leave_air_mode(process, keys, entered);
    result
}
/// The enemy's share of the region's air superiority as the window lets a player judge it:
/// active fighters times the efficiency shown, each side; None with no fighter on either side.
pub(super) fn superiority_estimate(v: &airui::RegionView) -> Option<f64> {
    let pct = |t: &str| {
        t.trim()
            .trim_end_matches('%')
            .trim()
            .parse::<f64>()
            .ok()
            .map_or(1.0, |p| p / 100.0)
    };
    let ours = f64::from(v.ours[0]) * pct(&v.our_efficiency);
    let theirs = f64::from(v.theirs[0]) * pct(&v.enemy_efficiency);
    (ours + theirs > 0.0).then(|| ((theirs / (ours + theirs)) * 1000.0).round() / 1000.0)
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
        "enemy_air_superiority_estimate":superiority_estimate(v),
        "estimate_basis":"active fighters x efficiency shown, each side; the game's own figure is not located in memory",
        "note":"As the player's air map shows it (counts of currently active aircraft by type in the region)"})
}

/// Air superiority per strategic region is what the air map mode colours each region by (red
/// under 40% own share, contested 40-60%, own over 60%): each side's superiority comes from the
/// air superiority stat of its wings on air superiority missions in the region (anti-air buildings
/// take NDefines.NBuildings.ANTI_AIR_SUPERIORITY_MULT per level off the enemy's), the region map
/// scale is NAir.AIR_REGION_SUPERIORITY_PIXEL_SCALE. Enemy air superiority slows
/// (NMilitary.ENEMY_AIR_SUPERIORITY_SPEED_IMPACT) and weakens (ENEMY_AIR_SUPERIORITY_IMPACT) land
/// units under it, and paradrops need NCountry.PARADROP_AIR_SUPERIORITY_RATIO. The value is not located in memory yet: `regions[]` carries
/// `enemy_air_superiority` null with this reason until the read is built and compared with the air
/// map live (gap register air.enemy_superiority_read); the plan rule `manage.enemy_air_superiority`
/// (plan_military.rs) reads that field and acts on nothing while it is null.
const ENEMY_SUPERIORITY_NOT_MAPPED: &str =
    "not_mapped: the enemy's air superiority per region is not located in memory yet";

/// `hoi4_air_regions`: own wings and air bases grouped by strategic air region (memory and game
/// files only; the enemy's presence is what `hoi4_air_region_probe` reads from the air map).
pub(super) fn regions(process: &dyn Process) -> Result<Value> {
    use std::collections::BTreeMap;
    let (bases, wings) = own(process)?;
    let names = game_dir(process)
        .map(|d| airread::region_names(&d))
        .unwrap_or_default();
    let mut rows: BTreeMap<i32, (Vec<Value>, Vec<Value>)> = BTreeMap::new();
    for w in &wings {
        if w.region_id > 0 {
            rows.entry(w.region_id)
                .or_default()
                .0
                .push(json!({"wing":w.name,"aircraft":w.planes,
                "aircraft_type":w.equipment,"missions":airread::missions_in(w.mission_mask),
                "air_base_state":w.base_state}));
        }
    }
    for b in &bases {
        let used: i32 = wings
            .iter()
            .filter(|w| w.base_id == b.id)
            .map(|w| w.planes)
            .sum();
        let region = b.region_id;
        if let Some(region) = region {
            rows.entry(region).or_default().1.push(json!({"state_id":b.state,"capacity":b.capacity,
                "aircraft_stationed":used,"free":b.capacity - used,
                "wings":wings.iter().filter(|w| w.base_id == b.id).map(|w| w.name.as_str()).collect::<Vec<_>>()}));
        }
    }
    let idle: Vec<&str> = wings
        .iter()
        .filter(|w| w.region_id <= 0)
        .map(|w| w.name.as_str())
        .collect();
    Ok(json!({
        "regions":rows.into_iter().map(|(id, (w, b))| json!({"region_id":id,"name":names.get(&id),
            "wings_assigned":w.len(),"aircraft_assigned":w.iter().map(|x| x["aircraft"].as_i64().unwrap_or(0)).sum::<i64>(),
            "wings":w,"air_bases_here":b,
            "enemy_air_superiority":null,"enemy_air_superiority_reason":ENEMY_SUPERIORITY_NOT_MAPPED})).collect::<Vec<_>>(),
        "wings_without_region":idle,
        "enemy_air":"Not in memory: hoi4_air_region_probe {state_id} opens the air map's region window and reads what it shows (active aircraft by type, detection and efficiency)",
        "validated":{"wings_by_region":"wing +0xb8 points at the CStrategicRegion; its file id (+0x40) matched the overview's Area column for Western and Eastern Germany","bases_by_region":"the base province's region pointer (+0xc8), file id +0x40"}
    }))
}

// ---- region arguments ---------------------------------------------------------------------------
/// A region id or display name (case-insensitive, unique substring allowed) -> region id.
pub(super) fn pick_region(
    names: &std::collections::BTreeMap<i32, String>,
    wanted: &str,
) -> Result<i32> {
    let wanted = wanted.trim();
    if let Ok(id) = wanted.parse::<i32>() {
        if names.is_empty() || names.contains_key(&id) {
            return Ok(id);
        }
    } else {
        let exact: Vec<i32> = names
            .iter()
            .filter(|(_, n)| same(n, wanted))
            .map(|(i, _)| *i)
            .collect();
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
            "No unique strategic region {wanted:?} in this game's map files; {}",
            {
                // Names sharing a word with the request first (the full list is long).
                let words: Vec<String> = wanted
                    .to_lowercase()
                    .split_whitespace()
                    .filter(|w| w.len() > 3)
                    .map(str::to_string)
                    .collect();
                let close: Vec<String> = names
                    .values()
                    .filter(|n| {
                        let n = n.to_lowercase();
                        words.iter().any(|w| n.contains(w.as_str()))
                    })
                    .take(12)
                    .cloned()
                    .collect();
                if close.is_empty() {
                    format!(
                        "examples: {}",
                        names
                            .values()
                            .take(8)
                            .cloned()
                            .collect::<Vec<_>>()
                            .join(" | ")
                    )
                } else {
                    format!("close names: {}", close.join(" | "))
                }
            }
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
/// A region with no state in it (a sea): the state whose centre lies nearest the region's
/// provinces' centre, for the Find view to centre on before the region's own provinces are
/// clicked (air::region_points falls back to them).
pub(super) fn nearest_state(dir: &std::path::Path, region: i32) -> Option<i32> {
    let regions = mapdata::province_regions(dir).ok()?;
    let positions = mapdata::positions(dir).ok()?;
    let pts: Vec<(f64, f64)> = regions
        .iter()
        .filter(|(_, r)| **r == region)
        .filter_map(|(p, _)| positions.get(p).copied())
        .collect();
    if pts.is_empty() {
        return None;
    }
    let n = pts.len() as f64;
    let mid = pts
        .iter()
        .fold((0.0, 0.0), |a, p| (a.0 + p.0 / n, a.1 + p.1 / n));
    // the region province nearest its own centre (a bay's centre can lie on land)
    let anchor = pts
        .iter()
        .min_by(|a, b| {
            let d = |p: &&(f64, f64)| (p.0 - mid.0).powi(2) + (p.1 - mid.1).powi(2);
            d(a).total_cmp(&d(b))
        })
        .copied()?;
    mapdata::state_centers(dir)
        .ok()?
        .iter()
        .min_by(|a, b| {
            let d = |c: &(f64, f64)| (c.0 - anchor.0).powi(2) + (c.1 - anchor.1).powi(2);
            d(a.1).total_cmp(&d(b.1)).then(a.0.cmp(b.0))
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
        // A state alone: its main strategic region (most of its provinces), so the readback
        // checks the region too (a click outside the wing's range used to pass as done).
        let s = state.ok_or_else(|| Error::new("invalid_arguments", "Give state_id or region"))?;
        let main = game_dir(process)
            .and_then(|dir| mapdata::state_regions(&dir).ok())
            .and_then(|m| m.get(&s).map(|v| v.0));
        return Ok((s, main));
    };
    let dir = game_dir(process).ok_or_else(|| {
        Error::new(
            "map_data_unavailable",
            "Game install not found; give state_id only",
        )
    })?;
    // By id or map file name first, then by the name the game shows for the region.
    let id = pick_region(&airread::region_names(&dir), &text).or_else(|by_file| {
        pick_region(&airread::region_display_names(&dir), &text).map_err(|_| by_file)
    })?;
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
            .or_else(|| nearest_state(&dir, id))
            .map(|s| (s, Some(id)))
            .ok_or_else(|| {
                Error::new(
                    "region_unknown",
                    format!("No state lies in region {text:?}"),
                )
            }),
    }
}

// ---- arguments ----------------------------------------------------------------------------------
pub(super) fn text_arg(
    args: &serde_json::Map<String, Value>,
    key: &str,
    max: usize,
) -> Result<String> {
    let v = args
        .get(key)
        .and_then(Value::as_str)
        .ok_or_else(|| Error::new("invalid_arguments", format!("{key} must be a string")))?;
    if v.trim().is_empty() || v.len() > max || v.chars().any(char::is_control) {
        return Err(Error::new("invalid_arguments", format!("Invalid {key}")));
    }
    Ok(v.to_string())
}
/// A strategic region given as its name, its map file name or its id; an id may come as a
/// number (players sent `region: 5` 30 times in the runs of 11-19 Nov and were refused).
pub(super) fn region_arg(args: &serde_json::Map<String, Value>) -> Result<String> {
    match args.get("region") {
        Some(Value::Number(n)) => n
            .as_u64()
            .filter(|id| (1..=4095).contains(id))
            .map(|id| id.to_string())
            .ok_or_else(|| Error::new("invalid_arguments", "Invalid region")),
        _ => text_arg(args, "region", 96),
    }
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

    fn names(v: &[&str]) -> Vec<String> {
        v.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn a_base_click_is_used_only_where_every_wing_of_the_base_is_asked() {
        // Germany May 1940 shape: fighters at 57 (all asked), a mixed base 64 (a CAS wing
        // there is not asked), one fighter alone at 66
        let rows: Vec<(String, Option<i32>)> = vec![
            ("JG 1".into(), Some(57)),
            ("JG 2".into(), Some(57)),
            ("JG 3".into(), Some(57)),
            ("JG 26".into(), Some(64)),
            ("StG 1".into(), Some(64)),
            ("JG 27".into(), Some(66)),
            ("JG 51".into(), Some(66)),
            ("JG 52".into(), None),
        ];
        let asked = names(&["JG 1", "JG 2", "JG 3", "JG 26", "JG 27", "JG 51", "JG 52"]);
        let groups = clean_bases(&asked, &asked, &rows, &[]);
        assert_eq!(
            groups,
            vec![
                BaseGroup { state: 57, wings: names(&["JG 1", "JG 2", "JG 3"]) },
                BaseGroup { state: 66, wings: names(&["JG 27", "JG 51"]) },
            ],
            "most pending wings first; the mixed base 64 and the wing with no base are left to the single path"
        );
        // a base whose wings already fly the order (not pending) is not clicked again
        let pending = names(&["JG 27", "JG 51", "JG 26"]);
        assert_eq!(clean_bases(&asked, &pending, &rows, &[]).len(), 1);
        // a base tried already (or the rebase target) is skipped; one pending wing is not a batch
        assert!(clean_bases(&asked, &pending, &rows, &[66]).is_empty());
        assert!(clean_bases(&asked, &names(&["JG 1"]), &rows, &[]).is_empty());
    }

    #[test]
    fn the_selection_read_back_compares_the_wings_in_any_order() {
        assert!(same_wings(&names(&["b", "a"]), &names(&["a", "b"])));
        assert!(!same_wings(&names(&["a"]), &names(&["a", "b"])));
        assert!(!same_wings(&names(&["a", "c"]), &names(&["a", "b"])));
    }

    #[test]
    fn a_region_id_may_come_as_a_number() {
        let args = |v: Value| v.as_object().unwrap().clone();
        assert_eq!(
            region_arg(&args(serde_json::json!({"region": 5}))).unwrap(),
            "5"
        );
        assert_eq!(
            region_arg(&args(serde_json::json!({"region": "Western Poland"}))).unwrap(),
            "Western Poland"
        );
        assert!(region_arg(&args(serde_json::json!({"region": 0}))).is_err());
        assert!(region_arg(&args(serde_json::json!({"region": -3}))).is_err());
    }

    fn base(id: i32, state: i32, capacity: i32) -> Base {
        Base {
            id,
            state: Some(state),
            province: 1,
            capacity,
            region_id: None,
        }
    }
    fn wing(name: &str, planes: i32, base_id: i32, state: i32) -> Wing {
        Wing {
            address: 0,
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
        assert_eq!(
            plan_rebase(&b, &w, "Nope", 66).unwrap_err().code,
            "wing_unknown"
        );
        assert_eq!(
            plan_rebase(&b, &w, "Jagdgeschwader 132", 99)
                .unwrap_err()
                .code,
            "air_base_unknown"
        );
        assert_eq!(
            plan_rebase(&b, &w, "Jagdgeschwader 132", 64)
                .unwrap_err()
                .code,
            "nothing_to_change"
        );
        // 80 aircraft do not fit the 400-capacity base that already holds 40 and gets 300 incoming.
        let mut w2 = w.clone();
        w2.push(wing("Incoming", 300, 10, 66));
        w2.last_mut().unwrap().transfer_base_id = 3;
        assert_eq!(
            plan_rebase(&b, &w2, "Jagdgeschwader 132", 52)
                .unwrap_err()
                .code,
            "air_base_full"
        );
        let mut flying = w.clone();
        flying[0].transfer_base_id = 10;
        assert_eq!(
            plan_rebase(&b, &flying, "Jagdgeschwader 132", 66)
                .unwrap_err()
                .code,
            "nothing_to_change"
        );
        let mut dup = w.clone();
        dup.push(wing("Jagdgeschwader 132", 1, 9, 64));
        assert_eq!(
            plan_rebase(&b, &dup, "Jagdgeschwader 132", 66)
                .unwrap_err()
                .code,
            "wing_ambiguous"
        );
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
        assert_eq!(
            plan_create(&b, &w, 99, "x").unwrap_err().code,
            "air_base_unknown"
        );
        let mut full = w.clone();
        full.push(wing("Fill", 1040, 3, 52));
        assert_eq!(
            plan_create(&b, &full, 52, "x").unwrap_err().code,
            "air_base_full"
        );
        let mut after = w.clone();
        assert_eq!(settle_create(&p, &after), (Verdict::Unchanged, vec![]));
        after.push(wing("KG 3 'Blitz'", 40, 9, 64));
        assert_eq!(
            settle_create(&p, &after),
            (Verdict::Reached, vec!["KG 3 'Blitz'".to_string()])
        );
        after.push(wing("Another", 40, 9, 64));
        assert_eq!(settle_create(&p, &after).0, Verdict::Diverged);
    }
    #[test]
    fn reserve_rows_are_matched_by_variant_or_class() {
        let rect = crate::gui_tree::Rect {
            x: 0,
            y: 0,
            w: 10,
            h: 10,
        };
        let e = |v: &str, c: &str, n: i32| airui::Entry {
            variant: v.into(),
            class: c.into(),
            count: n,
            bg: rect,
        };
        let rows = vec![
            e("Ju 86", "Inter-War Bomber", 40),
            e("He 51", "Inter-War Fighter", 120),
        ];
        assert_eq!(pick_entry(&rows, "ju 86").unwrap().count, 40);
        assert_eq!(
            pick_entry(&rows, "Inter-War Fighter").unwrap().variant,
            "He 51"
        );
        assert_eq!(pick_entry(&rows, "fighter").unwrap().variant, "He 51");
        assert_eq!(
            pick_entry(&rows, "Inter-War").unwrap_err().code,
            "aircraft_ambiguous"
        );
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
        assert_eq!(
            plan_split(&flying, "Jagdgeschwader 132").unwrap_err().code,
            "wing_in_transfer"
        );
        let mut tiny = w.clone();
        tiny[0].planes = 1;
        assert_eq!(
            plan_split(&tiny, "Jagdgeschwader 132").unwrap_err().code,
            "nothing_to_change"
        );
        let mut after = w.clone();
        assert_eq!(settle_split(&p, &after), (Verdict::Unchanged, vec![]));
        after[0].planes = 40;
        after.push(wing("Jagdgeschwader 134", 40, 9, 64));
        assert_eq!(
            settle_split(&p, &after),
            (Verdict::Reached, vec!["Jagdgeschwader 134".to_string()])
        );
        // Merge: the game joins every wing of one type at a base.
        let m = plan_merge(&w, "Jagdgeschwader 132", Some("Jagdgeschwader 133")).unwrap();
        assert_eq!((m.total, m.group.len()), (120, 2));
        assert_eq!(
            plan_merge(&w, "Jagdgeschwader 132", None)
                .unwrap()
                .group
                .len(),
            2
        );
        assert_eq!(
            plan_merge(&w, "Jagdgeschwader 132", Some("Jagdgeschwader 132"))
                .unwrap_err()
                .code,
            "invalid_arguments"
        );
        assert_eq!(
            plan_merge(&w, "Jagdgeschwader 132", Some("Kampfgeschwader 253"))
                .unwrap_err()
                .code,
            "merge_different_base"
        );
        assert_eq!(
            plan_merge(&w, "Kampfgeschwader 253", None)
                .unwrap_err()
                .code,
            "no_merge_partner"
        );
        let other = w.clone();
        assert_eq!(
            plan_merge(&other, "Jagdgeschwader 132", Some("KG 1 'Hindenburg'"))
                .unwrap_err()
                .code,
            "merge_different_type"
        );
        assert_eq!(archetype("tac_bomber_equipment_1"), "tac_bomber");
        assert_eq!(archetype("CAS_equipment_1"), "CAS");
        let mut three = w.clone();
        three.push(wing("Jagdgeschwader 135", 20, 9, 64));
        assert_eq!(
            plan_merge(&three, "Jagdgeschwader 132", Some("Jagdgeschwader 133"))
                .unwrap_err()
                .code,
            "merge_group_larger"
        );
        assert_eq!(
            plan_merge(&three, "Jagdgeschwader 132", None)
                .unwrap()
                .total,
            140
        );
        let mut flying = w.clone();
        flying[4].transfer_base_id = 3;
        assert_eq!(
            plan_merge(&flying, "Jagdgeschwader 132", None)
                .unwrap_err()
                .code,
            "wing_in_transfer"
        );
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
        cap.iter_mut()
            .find(|x| x.name == "Jagdgeschwader 132")
            .unwrap()
            .planes = 100;
        cap.iter_mut()
            .find(|x| x.name == "Kampfgeschwader 155")
            .unwrap()
            .planes = 60;
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
        assert_eq!(
            pick_region(&names, "Germany").unwrap_err().code,
            "region_unknown"
        );
        assert_eq!(
            pick_region(&names, "99").unwrap_err().code,
            "region_unknown"
        );
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
        assert_eq!(
            plan_hold(&w, "Jagdgeschwader 132").unwrap_err().code,
            "nothing_to_change"
        );
        w[0].mission_mask = 1;
        w[0].region_id = 7;
        let b = plan_hold(&w, "Jagdgeschwader 132").unwrap();
        assert_eq!(settle_hold(&b, Some(&snap(&w[0]))), Verdict::Unchanged);
        let mut held = snap(&w[0]);
        held.mission_mask = 0;
        held.region_id = 0;
        assert_eq!(settle_hold(&b, Some(&held)), Verdict::Reached);
        let nothing = snap(&fixture().1[0]);
        assert_eq!(
            settle_mission(&nothing, Some(&b), 1, Some(7)),
            Verdict::Reached
        );
        assert_eq!(
            settle_mission(&nothing, Some(&b), 1, Some(8)),
            Verdict::Diverged
        );
        assert_eq!(
            settle_mission(&nothing, Some(&nothing), 1, None),
            Verdict::Unchanged
        );
        assert_eq!(settle_mission(&nothing, None, 1, None), Verdict::Unknown);
    }
}
