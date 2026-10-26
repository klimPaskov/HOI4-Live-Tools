//! Naval orders with ordinary input on macOS (089EA519). Everything an expert does with the
//! navy through the Strategic Navy map mode (F2): a fleet card selects a fleet, the fleet panel's
//! entry button selects one task force, a mission button gives it its mission, a right click on a
//! sea region adds the region to the fleet's area of operation, the red remove button plus a right
//! click takes one away again. Fresh memory reads (task force mission code, fleet region list)
//! verify every step; nothing is ever retried after possible input.
//!
//! Routes mapped and verified live on 26 October 2026 (Germany, 15 October 1938, 1920x1080):
//! * F2 shows the fleet bar (`navy_leader_window`); F1 restores the default map mode;
//! * a fleet card (`portrait_frame`) opens the fleet panel; a task force entry's `select_btn`
//!   (label "Select" -> "Back") narrows the mission bar to that task force ("Missions - <name>");
//! * a mission button alone sets the task force's mission (patrol, strike force, convoy
//!   raiding, convoy escort, training, invasion support; hold = go to the closest port); the mission
//!   code at task force +0x36c read 1, 2, 3, 4, 7, 9 and 0 after the six clicks;
//! * a right click on a sea region with the fleet selected adds it to the fleet area (fleet +0x40,
//!   panel "Active in Regions"); `btn_remove_regions` (state 2 while armed, disarmed by the next
//!   right click) plus a right click removes it;
//! * a left click on a sea region (nothing selected) opens the region view whose `region_name`
//!   names the region; this is how a region is found on screen before anything is ordered.
use super::{
    naval::{self, Fleet},
    navalui::{self as nui, GAME},
    ui::{clear_notices, find_state, read, settled, wait},
};
use crate::{
    memory::Reader,
    economy,
    error::{Error, Result},
    platform::{macos_input::MacKeys, Process},
};
use serde_json::{json, Value};
use std::{thread, time::Duration};

/// (tool name, mission button, mission code, needs a fleet area of operation).
pub(super) const MISSIONS: [(&str, &str, i32, bool); 9] = [
    ("hold", "btn_mission0", 0, false),
    ("patrol", "btn_mission1", 1, true),
    ("strike_force", "btn_mission2", 2, true),
    ("convoy_raiding", "btn_mission3", 3, true),
    ("convoy_escort", "btn_mission4", 4, true),
    ("mine_laying", "btn_mission5", 5, true),
    ("mine_sweeping", "btn_mission6", 6, true),
    ("training", "btn_mission7", 7, true),
    ("invasion_support", "btn_mission9", 9, true),
];
const F1: u16 = 122;
const F2: u16 = 120;

pub(super) fn own_fleets(process: &dyn Process) -> Result<Vec<Fleet>> {
    read(process, |r, image| {
        super::fleetmem::require_libcxx(r)?;
        let (country, _) = economy::own_production(r, image)?;
        naval::fleets(r, image, country)
    })
}

fn same_name(a: &str, b: &str) -> bool {
    a.trim().eq_ignore_ascii_case(b.trim())
}

// ---- planning (pure, before any input) -------------------------------------------------------
#[derive(Clone, Debug, PartialEq)]
pub(super) struct MissionPlan {
    pub fleet: String,
    pub task_force: String,
    pub mission: &'static str,
    pub button: &'static str,
    pub code: i32,
    pub before_code: i32,
    /// Fleet names in memory (= fleet card) order.
    pub order: Vec<String>,
    /// The fleet's task force names in panel order.
    pub fleet_task_forces: Vec<String>,
}
pub(super) fn find_task_force<'a>(
    fleets: &'a [Fleet],
    name: &str,
) -> Result<(usize, &'a Fleet, &'a naval::TaskForce)> {
    let mut hits = Vec::new();
    for (i, f) in fleets.iter().enumerate() {
        for t in &f.task_forces {
            if same_name(&t.name, name) {
                hits.push((i, f, t));
            }
        }
    }
    match hits.len() {
        0 => Err(Error::new(
            "task_force_unknown",
            format!(
                "No own task force named {name:?}; own task forces: {}",
                fleets
                    .iter()
                    .flat_map(|f| f.task_forces.iter().map(|t| t.name.as_str()))
                    .collect::<Vec<_>>()
                    .join(" | ")
            ),
        )),
        1 => Ok(hits.remove(0)),
        _ => Err(Error::new(
            "task_force_ambiguous",
            "Several own task forces carry this name",
        )),
    }
}
pub(super) fn plan_mission(fleets: &[Fleet], task_force: &str, mission: &str) -> Result<MissionPlan> {
    let Some(&(name, button, code, needs_area)) = MISSIONS.iter().find(|m| m.0 == mission) else {
        return Err(Error::new(
            "invalid_arguments",
            format!(
                "mission must be one of {}",
                MISSIONS.iter().map(|m| m.0).collect::<Vec<_>>().join(", ")
            ),
        ));
    };
    let (index, fleet, tf) = find_task_force(fleets, task_force)?;
    if index + 1 == fleets.len() && fleets.len() > 1 && tf.mission_code == 8 {
        return Err(Error::new(
            "reserve_fleet",
            "The reserve fleet's task force holds ships without orders; move ships into a real fleet first",
        ));
    }
    if tf.mission_code == code && code != 0 {
        return Err(Error::new(
            "nothing_to_change",
            format!("{} already has the {name} mission", tf.name),
        ));
    }
    if name == "hold" && tf.mission_code == 0 {
        return Err(Error::new(
            "nothing_to_change",
            format!("{} already holds (no mission)", tf.name),
        ));
    }
    if needs_area && fleet.regions.is_empty() {
        return Err(Error::new(
            "no_assigned_region",
            format!(
                "Fleet {:?} has no sea region of operation; add one with hoi4_fleet_region first",
                fleet.name
            ),
        ));
    }
    Ok(MissionPlan {
        fleet: fleet.name.clone(),
        task_force: tf.name.clone(),
        mission: name,
        button,
        code,
        before_code: tf.mission_code,
        order: fleets.iter().map(|f| f.name.clone()).collect(),
        fleet_task_forces: fleet.task_forces.iter().map(|t| t.name.clone()).collect(),
    })
}
#[derive(Clone, Debug, PartialEq)]
pub(super) struct RegionPlan {
    pub fleet: String,
    pub region: String,
    pub state: i32,
    pub remove: bool,
    pub order: Vec<String>,
}
pub(super) fn plan_region(
    fleets: &[Fleet],
    fleet: &str,
    region: &str,
    state: i32,
    remove: bool,
) -> Result<RegionPlan> {
    let Some(f) = fleets.iter().find(|f| same_name(&f.name, fleet)) else {
        return Err(Error::new(
            "fleet_unknown",
            format!(
                "No own fleet named {fleet:?}; own fleets: {}",
                fleets.iter().map(|f| f.name.trim()).collect::<Vec<_>>().join(" | ")
            ),
        ));
    };
    if fleets.last().is_some_and(|l| std::ptr::eq(l, f)) && fleets.len() > 1 && f.task_forces.iter().all(|t| t.mission_code == 8) {
        return Err(Error::new(
            "reserve_fleet",
            "The reserve fleet takes no sea region",
        ));
    }
    let has = f.regions.iter().any(|(_, n)| same_name(n, region));
    if remove && !has {
        return Err(Error::new(
            "region_not_assigned",
            format!(
                "Fleet {:?} does not operate in {region:?}; its regions: {}",
                f.name,
                f.regions.iter().map(|r| r.1.as_str()).collect::<Vec<_>>().join(" | ")
            ),
        ));
    }
    if !remove && has {
        return Err(Error::new(
            "already_assigned",
            format!("Fleet {:?} already operates in {region:?}", f.name),
        ));
    }
    Ok(RegionPlan {
        fleet: f.name.clone(),
        region: region.trim().to_string(),
        state,
        remove,
        order: fleets.iter().map(|f| f.name.clone()).collect(),
    })
}

// ---- settlement (pure) -----------------------------------------------------------------------
pub(super) fn settle_mission(before: i32, now: Option<i32>, target: i32) -> super::settle::Verdict {
    use super::settle::Verdict;
    match now {
        Some(c) if c == target => Verdict::Reached,
        Some(c) if c == before => Verdict::Unchanged,
        Some(_) => Verdict::Diverged,
        None => Verdict::Unknown,
    }
}
/// A fleet's area changes only by this action: the region present (add) or absent (remove).
pub(super) fn settle_region(had: bool, has: Option<bool>, want_present: bool) -> super::settle::Verdict {
    use super::settle::Verdict;
    match has {
        Some(h) if h == want_present => Verdict::Reached,
        Some(h) if h == had => Verdict::Unchanged,
        Some(_) => Verdict::Diverged,
        None => Verdict::Unknown,
    }
}
pub(super) fn mission_code_now(process: &dyn Process, task_force: &str) -> Result<Option<i32>> {
    Ok(own_fleets(process)?
        .iter()
        .flat_map(|f| f.task_forces.iter())
        .find(|t| same_name(&t.name, task_force))
        .map(|t| t.mission_code))
}
pub(super) fn region_present_now(process: &dyn Process, fleet: &str, region: &str) -> Result<Option<bool>> {
    Ok(own_fleets(process)?
        .iter()
        .find(|f| same_name(&f.name, fleet))
        .map(|f| f.regions.iter().any(|(_, n)| same_name(n, region))))
}

// ---- driving the navy UI ---------------------------------------------------------------------
fn click_at(keys: &MacKeys, x: f64, y: f64) -> Result<()> {
    keys.click(GAME, x, y).map(|_| ())
}
fn pause(ms: u64) {
    thread::sleep(Duration::from_millis(ms));
}
/// Switch to the Strategic Navy map mode.
pub(super) fn enter_navy_mode(process: &dyn Process, keys: &MacKeys) -> Result<()> {
    if !read(process, nui::navy_mode)? {
        keys.press_code(F2)?;
        wait(process, "strategic navy map mode", nui::navy_mode)?;
        pause(300);
    }
    Ok(())
}
/// Back to the default map mode.
pub(super) fn leave_navy_mode(process: &dyn Process, keys: &MacKeys) -> Result<()> {
    if read(process, nui::navy_mode)? {
        keys.press_code(F1)?;
        wait(process, "default map mode", |r, i| Ok(!nui::navy_mode(r, i)?))?;
    }
    Ok(())
}
fn close_region_view(process: &dyn Process, keys: &MacKeys) -> Result<()> {
    for attempt in 0..2 {
        if read(process, nui::region_view)?.is_none() {
            return Ok(());
        }
        // The view slides in; click its close button only where it rests.
        let (_, close) = settled(process, "region view", |r, i| {
            nui::region_view(r, i)?.ok_or_else(|| {
                Error::new("gui_geometry_unavailable", "Region view closed meanwhile")
            })
        })?;
        let (x, y) = close.center();
        click_at(keys, x, y)?;
        match wait(process, "region view closed", |r, i| Ok(nui::region_view(r, i)?.is_none())) {
            Ok(()) => return Ok(()),
            Err(error) if attempt == 1 => return Err(error),
            Err(_) => pause(300),
        }
    }
    Ok(())
}
/// Deselect every task force and close the fleet panel.
fn close_panel(process: &dyn Process, keys: &MacKeys) -> Result<()> {
    for _ in 0..8 {
        if !read(process, nui::panel_open)? {
            return Ok(());
        }
        let entries = read(process, nui::entries)?;
        if let Some(selected) = entries.iter().find(|e| e.selected) {
            click_at(keys, f64::from(selected.select.x) + 30.0, f64::from(selected.select.y + selected.select.h / 2))?;
            pause(500);
            continue;
        }
        let close = settled(process, "fleet panel close", |r, i| nui::header_rect(r, i, "btn_close"))?;
        let (x, y) = close.center();
        click_at(keys, x, y)?;
        let _ = wait(process, "fleet panel closed", |r, i| Ok(!nui::panel_open(r, i)?));
    }
    if read(process, nui::panel_open)? {
        return Err(Error::new("ui_blocked", "The fleet panel did not close"));
    }
    Ok(())
}
/// Leave the navy UI tidy (best effort; errors after the real work must not hide its result).
pub(super) fn release(process: &dyn Process, keys: &MacKeys) {
    let _ = close_region_view(process, keys);
    let _ = close_panel(process, keys);
    let _ = leave_navy_mode(process, keys);
}
fn select_fleet(process: &dyn Process, keys: &MacKeys, name: &str, order: &[String]) -> Result<()> {
    if read(process, nui::panel_fleet)?.is_some_and(|n| same_name(&n, name)) {
        return Ok(());
    }
    close_region_view(process, keys)?;
    let cards = settled(process, "fleet cards", nui::cards)?;
    let wanted = order.iter().position(|n| same_name(n, name)).ok_or_else(|| {
        Error::new("fleet_unknown", "The fleet is not among the own fleets")
    })?;
    // The card order is the fleet order; a different order is detected by the panel title and the
    // other cards are tried once each (a card click only selects, it changes nothing).
    let mut candidates: Vec<usize> = vec![wanted];
    candidates.extend((0..cards.len()).filter(|i| *i != wanted));
    for index in candidates {
        let Some(card) = cards.get(index) else {
            continue;
        };
        let x = f64::from(card.frame.x) + f64::from(card.frame.w) / 2.0;
        let y = (f64::from(card.frame.y) + f64::from(card.frame.h) / 2.0).min(1040.0);
        click_at(keys, x, y)?;
        let ok = wait(process, "fleet panel", |r, i| {
            Ok(nui::panel_fleet(r, i)?.is_some())
        });
        if ok.is_ok() {
            pause(250);
            if read(process, nui::panel_fleet)?.is_some_and(|n| same_name(&n, name)) {
                return Ok(());
            }
        }
    }
    Err(Error::new(
        "input_readback_failed",
        "No fleet card opened the wanted fleet panel",
    ))
}
fn select_task_force(process: &dyn Process, keys: &MacKeys, name: &str) -> Result<()> {
    for _ in 0..14 {
        let entries = settled(process, "task force entries", nui::entries)?;
        // A selected task force is the only one shown; deselect it before selecting another.
        if let Some(other) = entries.iter().find(|e| e.selected && !same_name(&e.name, name)) {
            click_at(
                keys,
                f64::from(other.select.x) + 30.0,
                f64::from(other.select.y + other.select.h / 2),
            )?;
            pause(600);
            continue;
        }
        let Some(entry) = entries.iter().find(|e| same_name(&e.name, name)) else {
            return Err(Error::new(
                "task_force_unknown",
                "The task force is not shown in this fleet's panel",
            ));
        };
        if entry.selected {
            return Ok(());
        }
        let (top, bottom) = (250, 1000);
        if entry.select.y < top || entry.select.y + entry.select.h > bottom {
            let pixels = if entry.select.y < top { 240 } else { -240 };
            keys.scroll(GAME, (200.0, 600.0), pixels)?;
            pause(250);
            continue;
        }
        click_at(
            keys,
            f64::from(entry.select.x) + 30.0,
            f64::from(entry.select.y + entry.select.h / 2),
        )?;
        let want = name.to_string();
        wait(process, "task force selected", move |r, i| {
            Ok(nui::entries(r, i)?.iter().any(|e| same_name(&e.name, &want) && e.selected))
        })?;
        pause(250);
        return Ok(());
    }
    Err(Error::new(
        "gui_target_unavailable",
        "The task force's select button could not be scrolled into view",
    ))
}

/// Give the task force its mission with the game's mission button.
pub(super) fn set_mission(process: &dyn Process, keys: &MacKeys, plan: &MissionPlan) -> Result<Value> {
    clear_notices(process, keys)?;
    enter_navy_mode(process, keys)?;
    let result = (|| -> Result<Value> {
        close_region_view(process, keys)?;
        select_fleet(process, keys, &plan.fleet, &plan.order)?;
        select_task_force(process, keys, &plan.task_force)?;
        let (rect, state) = settled(process, "mission button", |r, i| nui::bar_button(r, i, plan.button))?;
        if state == 3 {
            return Err(Error::new(
                "mission_unavailable",
                format!("The game disables the {} mission button for this task force", plan.mission),
            ));
        }
        let (x, y) = rect.center();
        click_at(keys, x, y)?;
        let tf = plan.task_force.clone();
        let code = plan.code;
        wait(process, "mission set", move |r, i| {
            super::fleetmem::require_libcxx(r)?;
            let (country, _) = economy::own_production(r, i)?;
            Ok(naval::fleets(r, i, country)?
                .iter()
                .flat_map(|f| f.task_forces.iter())
                .any(|t| same_name(&t.name, &tf) && t.mission_code == code))
        })
        .map_err(|_| {
            Error::new(
                "input_readback_failed",
                "The task force's mission did not change (the game may have refused it)",
            )
        })?;
        Ok(json!({"task_force":plan.task_force,"fleet":plan.fleet,"mission":plan.mission,"code":plan.code}))
    })();
    release(process, keys);
    result
}

// ---- finding a sea region on screen ----------------------------------------------------------
/// Screen points to probe, nearest to the screen centre first, avoiding the panels, the top bar,
/// the fleet bar and the right-hand controls. Pure and deterministic.
pub(super) fn probe_points() -> Vec<(f64, f64)> {
    let mut points: Vec<(f64, f64)> = Vec::new();
    let mut y: f64 = 150.0;
    while y <= 880.0 {
        let mut x: f64 = 360.0;
        while x <= 1700.0 {
            let in_theatre = x > 1450.0 && y < 330.0;
            let in_mission_bar = (640.0..=1200.0).contains(&x) && y > 800.0;
            let in_controls = x > 1780.0;
            if !(in_theatre || in_mission_bar || in_controls) {
                points.push((x, y));
            }
            x += 170.0;
        }
        y += 130.0;
    }
    points.sort_by(|a, b| {
        let da = (a.0 - 960.0).powi(2) + (a.1 - 500.0).powi(2);
        let db = (b.0 - 960.0).powi(2) + (b.1 - 500.0).powi(2);
        da.partial_cmp(&db).unwrap()
    });
    points
}
pub(super) const PROBE_LIMIT: usize = 36;
/// Centre on `state` and left-click screen points until the region view names `region`.
/// Left clicks only open and close views: nothing is ordered here.
pub(super) fn find_region_point(
    process: &dyn Process,
    keys: &MacKeys,
    state: i32,
    region: &str,
) -> Result<((f64, f64), Vec<String>)> {
    close_panel(process, keys)?;
    close_region_view(process, keys)?;
    find_state(process, keys, state)?;
    let mut seen: Vec<String> = Vec::new();
    for (x, y) in probe_points().into_iter().take(PROBE_LIMIT) {
        click_at(keys, x, y)?;
        // The view opens within a moment for a sea region; land opens the state view instead.
        let mut found = None;
        for _ in 0..16 {
            pause(70);
            if let Some(view) = read(process, nui::region_view)? {
                found = Some(view);
                break;
            }
        }
        match found {
            Some((name, _)) => {
                if !seen.iter().any(|s| same_name(s, &name)) {
                    seen.push(name.clone());
                }
                close_region_view(process, keys)?;
                if same_name(&name, region) {
                    return Ok(((x, y), seen));
                }
            }
            None => {
                super::ui::close_state_view(process, keys)?;
            }
        }
    }
    Err(Error::new(
        "region_not_found",
        format!(
            "No sea region named {region:?} within reach of state {state}; regions seen: {}. Name a coastal state next to that sea",
            if seen.is_empty() { "none".to_string() } else { seen.join(" | ") }
        ),
    ))
}
/// Add `plan.region` to the fleet's area (or remove it) with a right click.
pub(super) fn region_op(process: &dyn Process, keys: &MacKeys, plan: &RegionPlan) -> Result<Value> {
    clear_notices(process, keys)?;
    enter_navy_mode(process, keys)?;
    let result = (|| -> Result<Value> {
        let ((x, y), seen) = find_region_point(process, keys, plan.state, &plan.region)?;
        select_fleet(process, keys, &plan.fleet, &plan.order)?;
        if plan.remove {
            let (rect, state) = settled(process, "remove regions button", |r, i| {
                nui::bar_button(r, i, "btn_remove_regions")
            })?;
            if state == 3 {
                return Err(Error::new(
                    "mission_unavailable",
                    "The game disables the remove region button (the fleet has no region)",
                ));
            }
            let (bx, by) = rect.center();
            click_at(keys, bx, by)?;
            wait(process, "remove mode", |r, i| {
                Ok(nui::bar_button(r, i, "btn_remove_regions")?.1 == 2)
            })?;
        }
        keys.right_click(GAME, x, y)?;
        let fleet = plan.fleet.clone();
        let region = plan.region.clone();
        let remove = plan.remove;
        wait(process, "fleet area changed", move |r, i| {
            super::fleetmem::require_libcxx(r)?;
            let (country, _) = economy::own_production(r, i)?;
            Ok(naval::fleets(r, i, country)?
                .iter()
                .find(|f| same_name(&f.name, &fleet))
                .is_some_and(|f| f.regions.iter().any(|(_, n)| same_name(n, &region)) != remove))
        })
        .map_err(|_| {
            Error::new(
                "input_readback_failed",
                "The fleet's area did not change (the game may have refused the region)",
            )
        })?;
        Ok(json!({"fleet":plan.fleet,"region":plan.region,"removed":plan.remove,"regions_seen_while_aiming":seen}))
    })();
    release(process, keys);
    result
}

// ---- admirals --------------------------------------------------------------------------------
#[derive(Clone, Debug, PartialEq)]
pub(super) struct AdmiralPlan {
    pub fleet: String,
    /// None: unassign.
    pub admiral: Option<String>,
    pub order: Vec<String>,
}
pub(super) fn plan_admiral(fleets: &[Fleet], fleet: &str, admiral: Option<&str>) -> Result<AdmiralPlan> {
    let Some(f) = fleets.iter().find(|f| same_name(&f.name, fleet)) else {
        return Err(Error::new(
            "fleet_unknown",
            format!(
                "No own fleet named {fleet:?}; own fleets: {}",
                fleets.iter().map(|f| f.name.trim()).collect::<Vec<_>>().join(" | ")
            ),
        ));
    };
    if fleets.len() > 1 && fleets.last().is_some_and(|l| std::ptr::eq(l, f)) && f.task_forces.iter().all(|t| t.mission_code == 8) {
        return Err(Error::new("reserve_fleet", "The reserve fleet has no admiral slot"));
    }
    match (&f.admiral, admiral) {
        (Some(have), Some(want)) if same_name(have, want) => Err(Error::new(
            "nothing_to_change",
            format!("{} already leads fleet {:?}", have, f.name),
        )),
        (None, None) => Err(Error::new(
            "nothing_to_change",
            format!("Fleet {:?} has no admiral to unassign", f.name),
        )),
        _ => Ok(AdmiralPlan {
            fleet: f.name.clone(),
            admiral: admiral.map(|a| a.trim().to_string()),
            order: fleets.iter().map(|f| f.name.clone()).collect(),
        }),
    }
}
pub(super) fn admiral_now(process: &dyn Process, fleet: &str) -> Result<Option<Option<String>>> {
    Ok(own_fleets(process)?
        .iter()
        .find(|f| same_name(&f.name, fleet))
        .map(|f| f.admiral.clone()))
}
pub(super) fn settle_admiral(
    before: &Option<String>,
    now: Option<&Option<String>>,
    want: &Option<String>,
) -> super::settle::Verdict {
    use super::settle::Verdict;
    let eq = |a: &Option<String>, b: &Option<String>| match (a, b) {
        (Some(x), Some(y)) => same_name(x, y),
        (None, None) => true,
        _ => false,
    };
    match now {
        Some(n) if eq(n, want) => Verdict::Reached,
        Some(n) if eq(n, before) => Verdict::Unchanged,
        Some(_) => Verdict::Diverged,
        None => Verdict::Unknown,
    }
}
/// Open the fleet panel's commander list and read it (nothing is assigned).
fn open_leader_list(process: &dyn Process, keys: &MacKeys) -> Result<()> {
    // The portrait slot of the header ("Click to assign.") is the frame of the portrait.
    let frame = settled(process, "commander slot", |r, i| {
        let header = nui::header(r, i)?
            .ok_or_else(|| Error::new("gui_target_unavailable", "Fleet panel is not open"))?;
        crate::gui_tree::rect(r, i, nui::child_of(r, i, header, "naval_unit_leader_frame")?, GAME)
    })?;
    let (x, y) = frame.center();
    click_at(keys, x, y)?;
    wait(process, "commander list open", nui::leader_open)?;
    pause(400);
    Ok(())
}
fn close_leader_list(process: &dyn Process, keys: &MacKeys) {
    if read(process, |r, i| Ok(nui::leader_open(r, i).unwrap_or(false))).unwrap_or(false) {
        if let Ok(rect) = settled(process, "commander close", |r, i| nui::leader_button(r, i, "close_button")) {
            let (x, y) = rect.center();
            let _ = click_at(keys, x, y);
            let _ = wait(process, "commander list closed", |r, i| Ok(!nui::leader_open(r, i)?));
        }
    }
}
/// Names and skills of the admirals the list offers for this fleet (assigns nothing).
pub(super) fn list_admirals(process: &dyn Process, keys: &MacKeys, fleet: &str, order: &[String]) -> Result<Vec<nui::Admiral>> {
    clear_notices(process, keys)?;
    enter_navy_mode(process, keys)?;
    let result = (|| -> Result<Vec<nui::Admiral>> {
        close_region_view(process, keys)?;
        select_fleet(process, keys, fleet, order)?;
        open_leader_list(process, keys)?;
        let rows = settled(process, "admiral list", nui::leader_rows)?;
        close_leader_list(process, keys);
        Ok(rows)
    })();
    close_leader_list(process, keys);
    release(process, keys);
    result
}
/// Put `plan.admiral` in command of the fleet (or unassign) from the fleet panel's list.
pub(super) fn assign_admiral(process: &dyn Process, keys: &MacKeys, plan: &AdmiralPlan) -> Result<Value> {
    clear_notices(process, keys)?;
    enter_navy_mode(process, keys)?;
    let result = (|| -> Result<Value> {
        close_region_view(process, keys)?;
        select_fleet(process, keys, &plan.fleet, &plan.order)?;
        open_leader_list(process, keys)?;
        let Some(wanted) = plan.admiral.as_deref() else {
            let rect = settled(process, "unassign", |r, i| nui::leader_button(r, i, "no_leader_button"))?;
            let (x, y) = rect.center();
            click_at(keys, x, y)?;
            super::ui::accept_command(process, keys)?;
            let fleet = plan.fleet.clone();
            wait(process, "admiral unassigned", move |r, i| {
                super::fleetmem::require_libcxx(r)?;
                let (country, _) = economy::own_production(r, i)?;
                Ok(naval::fleets(r, i, country)?
                    .iter()
                    .any(|f| same_name(&f.name, &fleet) && f.admiral.is_none()))
            })?;
            return Ok(json!({"fleet":plan.fleet,"admiral":null}));
        };
        let find = |r: &Reader<'_>, i: u64| -> Result<Option<nui::Admiral>> {
            Ok(nui::leader_rows(r, i)?.into_iter().find(|a| same_name(&a.name, wanted)))
        };
        let Some(mut row) = settled(process, "admiral list", find)? else {
            let names: Vec<String> = read(process, nui::leader_rows)?.into_iter().map(|a| a.name).collect();
            return Err(Error::new(
                "unknown_commander",
                format!("No such admiral in the list. Listed: {}", names.join(", ")),
            ));
        };
        let band = read(process, nui::leader_band)?;
        for _ in 0..40 {
            let bottom = row.icon.y + row.icon.h;
            let top = bottom - nui::PORTRAIT_DRAWN;
            if top >= band.0 && bottom <= band.1 {
                break;
            }
            if !read(process, |r, i| Ok(nui::leader_open(r, i).unwrap_or(false)))? {
                return Err(Error::new("input_readback_failed", "Commander list closed"));
            }
            keys.scroll(GAME, (960.0, 560.0), if top < band.0 { 240 } else { -240 })?;
            pause(250);
            row = read(process, find)?
                .ok_or_else(|| Error::new("gui_target_unavailable", "The admiral left the list"))?;
        }
        let row = settled(process, "admiral row", |r, i| {
            find(r, i)?.ok_or_else(|| Error::new("gui_target_unavailable", "The admiral left the list"))
        })?;
        let bottom = row.icon.y + row.icon.h;
        if bottom - nui::PORTRAIT_DRAWN < band.0 || bottom > band.1 {
            return Err(Error::new(
                "gui_target_unavailable",
                format!("Admiral row not in view: {:?}", row.icon),
            ));
        }
        click_at(
            keys,
            f64::from(row.icon.x) + 34.0,
            f64::from(bottom - nui::PORTRAIT_DRAWN / 2),
        )?;
        // Moving an admiral who leads another fleet asks for confirmation.
        super::ui::accept_command(process, keys)?;
        let fleet = plan.fleet.clone();
        let want = wanted.to_string();
        wait(process, "admiral assigned", move |r, i| {
            super::fleetmem::require_libcxx(r)?;
            let (country, _) = economy::own_production(r, i)?;
            Ok(naval::fleets(r, i, country)?
                .iter()
                .any(|f| same_name(&f.name, &fleet) && f.admiral.as_deref().is_some_and(|a| same_name(a, &want))))
        })?;
        Ok(json!({"fleet":plan.fleet,"admiral":wanted}))
    })();
    close_leader_list(process, keys);
    release(process, keys);
    result
}

/// `hoi4_naval_production_options`: the ship and convoy designs the Production panel can build
/// (not decommissioned) and the own naval lines. Memory only.
pub(super) fn production_options(process: &dyn Process) -> Result<Value> {
    let all = super::designs::read_all(process)?;
    let lines = super::production::lines(process)?;
    let buildable: Vec<Value> = all
        .iter()
        .filter(|d| super::designs::kind_of(&d.variant.type_key) == "ship" && !d.variant.obsolete)
        .map(|d| {
            json!({"design_id":d.variant.id,"name":d.variant.name,"class_suffix":d.variant.class_suffix,
                "equipment":d.variant.type_key,"manpower":d.variant.manpower,
                "lines":d.lines.iter().map(|(h, n)| json!({"line_handle":h,"requested_factories":n})).collect::<Vec<_>>()})
        })
        .collect();
    let naval_lines: Vec<Value> = lines
        .iter()
        .enumerate()
        .filter(|(_, l)| l.naval)
        .map(|(i, l)| json!({"line_index":i,"line_handle":l.handle,"equipment":l.key,"requested_factories":l.requested}))
        .collect();
    Ok(json!({"designs":buildable,"naval_lines":naval_lines,
        "how":"hoi4_production_add {equipment: <the design's equipment key, e.g. ship_hull_light_2 or convoy_1>, design?: <design_id>, factories}; hoi4_production_set {line: <line_index>, equipment, factories}; hoi4_design_create makes new ship variants",
        "note":"The game's naval factories are the dockyards (hoi4_economy factory_trigger_count.naval); a ship line's factories are dockyards. Whether the game offers a design depends on its technology; designs the game flags invalid (no battery/core module) are not listed as buildable by the Production panel."}))
}

// ---- new fleet -------------------------------------------------------------------------------
#[derive(Clone, Debug, PartialEq)]
pub(super) struct CreatePlan {
    pub task_force: String,
    pub from_fleet: String,
    pub fleets_before: usize,
    pub order: Vec<String>,
}
pub(super) fn plan_create(fleets: &[Fleet], task_force: &str) -> Result<CreatePlan> {
    let (index, fleet, tf) = find_task_force(fleets, task_force)?;
    if fleets.len() > 1 && index + 1 == fleets.len() && tf.mission_code == 8 {
        return Err(Error::new("reserve_fleet", "A reserve task force cannot found a fleet; it holds unassigned ships"));
    }
    if fleet.task_forces.len() < 2 {
        return Err(Error::new(
            "nothing_to_change",
            format!("{} is already alone in fleet {:?}", tf.name, fleet.name.trim()),
        ));
    }
    if fleets.len() >= 40 {
        return Err(Error::new("too_many_fleets", "The game's fleet bar holds too many fleets already"));
    }
    Ok(CreatePlan {
        task_force: tf.name.clone(),
        from_fleet: fleet.name.clone(),
        fleets_before: fleets.len(),
        order: fleets.iter().map(|f| f.name.clone()).collect(),
    })
}
/// A new fleet holds exactly the named task force, the old fleet lost it, the fleet count grew by one.
pub(super) fn settle_create(fleets: &[Fleet], plan: &CreatePlan) -> super::settle::Verdict {
    use super::settle::Verdict;
    let holder = fleets.iter().find(|f| f.task_forces.iter().any(|t| same_name(&t.name, &plan.task_force)));
    match holder {
        Some(h) if !same_name(&h.name, &plan.from_fleet) && h.task_forces.len() == 1 && fleets.len() == plan.fleets_before + 1 => Verdict::Reached,
        Some(h) if same_name(&h.name, &plan.from_fleet) && fleets.len() == plan.fleets_before => Verdict::Unchanged,
        Some(_) => Verdict::Diverged,
        None => Verdict::Unknown,
    }
}
/// Select the task force, then the bottom bar's "+" card founds a new fleet from it.
pub(super) fn create_fleet(process: &dyn Process, keys: &MacKeys, plan: &CreatePlan) -> Result<Value> {
    clear_notices(process, keys)?;
    enter_navy_mode(process, keys)?;
    let result = (|| -> Result<Value> {
        close_region_view(process, keys)?;
        select_fleet(process, keys, &plan.from_fleet, &plan.order)?;
        select_task_force(process, keys, &plan.task_force)?;
        let (rect, state) = settled(process, "new fleet card", nui::new_fleet_card)?;
        if state == 3 {
            return Err(Error::new("mission_unavailable", "The game disables the new fleet card"));
        }
        let (x, y) = rect.center();
        click_at(keys, x, y)?;
        let before = plan.fleets_before;
        let tf = plan.task_force.clone();
        let from = plan.from_fleet.clone();
        wait(process, "new fleet", move |r, i| {
            super::fleetmem::require_libcxx(r)?;
            let (country, _) = economy::own_production(r, i)?;
            let fleets = naval::fleets(r, i, country)?;
            Ok(fleets.len() == before + 1
                && fleets.iter().any(|f| !same_name(&f.name, &from) && f.task_forces.len() == 1 && same_name(&f.task_forces[0].name, &tf)))
        })?;
        let now = own_fleets(process)?;
        let name = now
            .iter()
            .find(|f| f.task_forces.iter().any(|t| same_name(&t.name, &plan.task_force)))
            .map(|f| f.name.trim().to_string());
        Ok(json!({"task_force":plan.task_force,"new_fleet":name,"from_fleet":plan.from_fleet.trim()}))
    })();
    release(process, keys);
    result
}

#[cfg(test)]
mod tests {
    use super::super::naval::{Fleet, TaskForce};
    use super::super::settle::Verdict;
    use super::*;

    fn tf(name: &str, code: i32) -> TaskForce {
        TaskForce {
            name: name.into(),
            place: None,
            mission_code: code,
            ships: Vec::new(),
        }
    }
    fn fleets() -> Vec<Fleet> {
        vec![
            Fleet {
                name: "Kriegsmarine".into(),
                home_port: None,
                admiral: None,
                regions: vec![("STRATEGICREGION_173".into(), "Eastern North Sea".into())],
                task_forces: vec![tf("Hochseeflotte", 0), tf("Ostseeflotte", 1)],
            },
            Fleet {
                name: "U-Flotte ".into(),
                home_port: None,
                admiral: None,
                regions: Vec::new(),
                task_forces: vec![tf("I. U-Boots", 0)],
            },
            Fleet {
                name: "Reserve".into(),
                home_port: None,
                admiral: None,
                regions: Vec::new(),
                task_forces: vec![tf("Reserve fleet 1", 8)],
            },
        ]
    }
    #[test]
    fn mission_plans_refuse_before_input() {
        let f = fleets();
        let p = plan_mission(&f, "hochseeflotte", "strike_force").unwrap();
        assert_eq!((p.code, p.button, p.before_code), (2, "btn_mission2", 0));
        assert_eq!(p.order[1].trim(), "U-Flotte");
        assert_eq!(plan_mission(&f, "Nope", "patrol").unwrap_err().code, "task_force_unknown");
        assert_eq!(plan_mission(&f, "Hochseeflotte", "ram").unwrap_err().code, "invalid_arguments");
        assert_eq!(plan_mission(&f, "Ostseeflotte", "patrol").unwrap_err().code, "nothing_to_change");
        assert_eq!(plan_mission(&f, "Hochseeflotte", "hold").unwrap_err().code, "nothing_to_change");
        assert_eq!(plan_mission(&f, "I. U-Boots", "convoy_raiding").unwrap_err().code, "no_assigned_region");
        assert_eq!(plan_mission(&f, "Reserve fleet 1", "patrol").unwrap_err().code, "reserve_fleet");
        // Hold needs no area and works for a task force that has a mission.
        assert_eq!(plan_mission(&f, "Ostseeflotte", "hold").unwrap().code, 0);
    }
    #[test]
    fn create_plans_and_settlement() {
        let f = fleets();
        let p = plan_create(&f, "Ostseeflotte").unwrap();
        assert_eq!((p.fleets_before, p.from_fleet.as_str()), (3, "Kriegsmarine"));
        assert_eq!(plan_create(&f, "I. U-Boots").unwrap_err().code, "nothing_to_change");
        assert_eq!(plan_create(&f, "Reserve fleet 1").unwrap_err().code, "reserve_fleet");
        assert_eq!(plan_create(&f, "Nope").unwrap_err().code, "task_force_unknown");
        let mut after = fleets();
        let moved = after[0].task_forces.remove(1);
        after.push(Fleet { name: "Flotta 2".into(), home_port: None, admiral: None, regions: Vec::new(), task_forces: vec![moved] });
        assert_eq!(settle_create(&after, &p), Verdict::Reached);
        assert_eq!(settle_create(&f, &p), Verdict::Unchanged);
    }
    #[test]
    fn region_plans() {
        let f = fleets();
        let p = plan_region(&f, "kriegsmarine", "Danish Belts", 58, false).unwrap();
        assert_eq!((p.state, p.remove), (58, false));
        assert_eq!(
            plan_region(&f, "Kriegsmarine", "eastern north sea", 56, false).unwrap_err().code,
            "already_assigned"
        );
        assert_eq!(
            plan_region(&f, "Kriegsmarine", "Danish Belts", 58, true).unwrap_err().code,
            "region_not_assigned"
        );
        assert!(plan_region(&f, "Kriegsmarine", "Eastern North Sea", 56, true).unwrap().remove);
        assert_eq!(plan_region(&f, "U-Flotte", "X", 1, false).unwrap().fleet, "U-Flotte ");
        assert_eq!(plan_region(&f, "Nope", "X", 1, false).unwrap_err().code, "fleet_unknown");
        assert_eq!(plan_region(&f, "Reserve", "X", 1, false).unwrap_err().code, "reserve_fleet");
    }
    #[test]
    fn settlement_compares_only_the_ordered_field() {
        assert_eq!(settle_mission(0, Some(1), 1), Verdict::Reached);
        assert_eq!(settle_mission(0, Some(0), 1), Verdict::Unchanged);
        assert_eq!(settle_mission(0, Some(2), 1), Verdict::Diverged);
        assert_eq!(settle_mission(0, None, 1), Verdict::Unknown);
        assert_eq!(settle_region(false, Some(true), true), Verdict::Reached);
        assert_eq!(settle_region(false, Some(false), true), Verdict::Unchanged);
        assert_eq!(settle_region(true, Some(false), false), Verdict::Reached);
        assert_eq!(settle_region(true, Some(true), false), Verdict::Unchanged);
        assert_eq!(settle_region(true, None, false), Verdict::Unknown);
    }
    #[test]
    fn probe_points_avoid_panels_and_start_near_the_centre() {
        let points = probe_points();
        assert!(points.len() > PROBE_LIMIT);
        let first = points[0];
        assert!((first.0 - 960.0).abs() < 200.0 && (first.1 - 500.0).abs() < 200.0);
        assert!(points.iter().all(|p| p.0 >= 360.0 && p.0 <= 1780.0 && p.1 >= 150.0 && p.1 <= 880.0));
        assert!(!points.iter().any(|p| p.0 > 1450.0 && p.1 < 330.0));
        assert!(!points.iter().any(|p| (640.0..=1200.0).contains(&p.0) && p.1 > 800.0));
    }
}
