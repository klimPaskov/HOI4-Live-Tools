//! Own armies (orders groups) and creating one from the divisions around a
//! state: the game's Find view centres the state, a box selection picks the
//! own divisions there, and the new-army card creates the army. Verified by a
//! new orders group in memory holding the selected divisions.
use super::ui::{clear_notices, find_state, read, settled, wait};
use crate::{
    error::{Error, Result},
    game,
    gui_tree::{self, Rect},
    memory::{Reader, StringAbi},
    platform::{
        macos_input::{Capture, Key, MacKeys},
        Process,
    },
};
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet};

struct Ui {
    country_divisions: u64,
    division_vtable: u64,
    division_group: u64,
    division_handle: u64,
    group_name: u64,
    selection_size: u64,
    manager: u64,
    leader_view_vtable: u64,
    leader_view: u64,
    leader_portraits: u64,
    portrait_window: u64,
    portrait_group: u64,
    group_handle: u64,
    group_orders: u64,
    division_location: u64,
    province_id: u64,
    order_vtable: u64,
    order_children: [u64; 2],
    armies_view: u64,
    leader_window: u64,
    leader_window_root: u64,
    grid_items: u64,
    item_container: u64,
    text: u64,
    group_leader: u64,
    leader_name: u64,
    container_vtable: u64,
    army_group: u64,
}
/// macOS Mach-O 089EA519, mapped live on 18 October 2026 (Germany test run):
/// a box selection set the idler's selection size (+0x540) to 7 and the
/// new-army card put those 7 divisions into one orders group ("Army 1",
/// division +0xc0, name +0x160).
const MACOS: Ui = Ui {
    country_divisions: 0x250,
    division_vtable: 0x32af5a8,
    division_group: 0xc0,
    division_handle: 0x18,
    group_name: 0x160,
    selection_size: 0x540,
    manager: 0x670,
    leader_view_vtable: 0x3463220,
    leader_view: 0x1c0,
    leader_portraits: 0x38,
    portrait_window: 0x28,
    portrait_group: 0x30,
    group_handle: 0x8,
    group_orders: 0x98,
    division_location: 0x1f0,
    province_id: 0x9c,
    order_vtable: 0x3313328,
    // Drawing an offensive line added one COrderInstance under each list.
    order_children: [0x1c8, 0x2a0],
    // Commander list of the army panel: CArmiesView (+0x1d8) holds the
    // CArmyLeaderWindow (+0x420, root +0x4b8); choosing a general set the
    // group's leader (+0x88, name +0x38).
    armies_view: 0x1d8,
    leader_window: 0x420,
    leader_window_root: 0x4b8,
    grid_items: 0x190,
    item_container: 0x10,
    text: 0x100,
    group_leader: 0x88,
    leader_name: 0x38,
    container_vtable: 0x34ac0b0,
    // An army's CArmyGroup (name +0x160, leader +0x88 like an army); its
    // card's group handle is the army group's +0x8. Mapped live on
    // 18 October 2026 after grouping Army 7 and Army 8.
    army_group: 0x1b0,
};
/// The army panel's commander portrait ("Click to assign") while an army is
/// selected, and the commander list's visible row band; portraits are drawn
/// 90 px tall from the bottom of their texture rectangle.
const COMMANDER_SLOT: (f64, f64) = (33.0, 160.0);
const LEADER_ROWS: (i32, i32) = (140, 625);
const PORTRAIT_DRAWN: i32 = 90;
const GAME: (u32, u32) = (1920, 1080);

fn ui(r: &Reader<'_>) -> Result<&'static Ui> {
    match r.string_abi() {
        StringAbi::LibCxx => Ok(&MACOS),
        StringAbi::LibStdCxx => Err(Error::new(
            "capability_unavailable",
            "Army input is not reviewed for this build",
        )),
    }
}
fn own_country(r: &Reader<'_>, image: u64) -> Result<u64> {
    let g = game::layout(r);
    let gs = r.u64(image + g.gs_global)?;
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
    if r.u64(country)? != image + g.country_vtable {
        return Err(Error::new("invalid_type", "Own country required"));
    }
    Ok(country)
}
/// division id -> army (orders group pointer or 0).
fn membership(r: &Reader<'_>, image: u64) -> Result<BTreeMap<String, u64>> {
    let u = ui(r)?;
    let country = own_country(r, image)?;
    let (_, data, count) = r.array_header(country + u.country_divisions)?;
    let mut out = BTreeMap::new();
    for index in 0..count.min(4096) as u64 {
        let division = r.u64(data + index * 8)?;
        if r.u64(division)? != image + u.division_vtable {
            return Err(Error::new(
                "invalid_army_type",
                "Exact own division required",
            ));
        }
        let handle = r.read(division + u.division_handle, 8)?;
        let id = format!(
            "{:08x}:{:08x}",
            u32::from_le_bytes(handle[..4].try_into().unwrap()),
            u32::from_le_bytes(handle[4..].try_into().unwrap())
        );
        out.insert(id, r.u64(division + u.division_group)?);
    }
    Ok(out)
}
/// division id -> state id where it stands (None while moving or unknown).
pub(super) fn division_states(r: &Reader<'_>, image: u64) -> Result<BTreeMap<String, Option<i32>>> {
    let u = ui(r)?;
    let l = crate::construction::layout(r);
    let country = own_country(r, image)?;
    let (_, data, count) = r.array_header(country + u.country_divisions)?;
    let mut out = BTreeMap::new();
    for index in 0..count.min(4096) as u64 {
        let division = r.u64(data + index * 8)?;
        let handle = r.read(division + u.division_handle, 8)?;
        let id = format!(
            "{:08x}:{:08x}",
            u32::from_le_bytes(handle[..4].try_into().unwrap()),
            u32::from_le_bytes(handle[4..].try_into().unwrap())
        );
        let location = r.u64(division + u.division_location)?;
        let state = if location != 0 && r.u64(location)? == image + l.province_vtable {
            let state = r.u64(location + l.province_state)?;
            if state != 0 && r.u64(state)? == image + l.state_vtable {
                Some(r.i32(state + l.state_id)?)
            } else {
                None
            }
        } else {
            None
        };
        out.insert(id, state);
    }
    Ok(out)
}
fn army_name(r: &Reader<'_>, group: u64) -> Result<String> {
    r.string_bounded(group + ui(r)?.group_name, 128)
}
/// Own armies with their division ids, plus unassigned division ids.
pub(super) fn armies(process: &dyn Process) -> Result<Value> {
    read(process, |r, image| {
        let members = membership(r, image)?;
        let mut groups: BTreeMap<u64, Vec<String>> = BTreeMap::new();
        let mut unassigned = Vec::new();
        for (id, group) in members {
            if group == 0 {
                unassigned.push(id);
            } else {
                groups.entry(group).or_default().push(id);
            }
        }
        let places = division_states(r, image)?;
        let armies = groups
            .into_iter()
            .map(|(group, ids)| {
                let mut standing: BTreeMap<i32, usize> = BTreeMap::new();
                for id in &ids {
                    if let Some(Some(state)) = places.get(id) {
                        *standing.entry(*state).or_default() += 1;
                    }
                }
                let main_state = standing.iter().max_by_key(|(_, n)| **n).map(|(s, _)| *s);
                let standing: serde_json::Map<String, Value> = standing
                    .into_iter()
                    .map(|(s, n)| (s.to_string(), json!(n)))
                    .collect();
                let name = army_name(r, group)?;
                let leader = r.u64(group + ui(r)?.group_leader)?;
                let commander = if leader == 0 {
                    None
                } else {
                    Some(r.string_bounded(leader + ui(r)?.leader_name, 128)?)
                };
                let orders = r.array_header(group + ui(r)?.group_orders)?.2;
                let executing = executing(r, image, &name).ok();
                let parent = r.u64(group + ui(r)?.army_group)?;
                let army_group = if parent == 0 {
                    None
                } else {
                    Some(army_name(r, parent)?)
                };
                Ok(
                    json!({"army":name,"army_group":army_group,"commander":commander,"battle_plan_orders":orders,
                    "plan_executing":executing,"division_count":ids.len(),"divisions":ids,
                    "states":standing,"main_state":main_state}),
                )
            })
            .collect::<Result<Vec<_>>>()?;
        let mut by_state: BTreeMap<i32, Vec<&String>> = BTreeMap::new();
        for id in &unassigned {
            if let Some(Some(state)) = places.get(id) {
                by_state.entry(*state).or_default().push(id);
            }
        }
        let by_state: Vec<Value> = by_state
            .into_iter()
            .map(|(state, ids)| json!({"state_id":state,"divisions":ids.len(),"division_ids":ids}))
            .collect();
        let groups = army_groups(r, image)?
            .into_iter()
            .map(|g| {
                let leader = r.u64(g + ui(r)?.group_leader)?;
                let marshal = if leader == 0 {
                    None
                } else {
                    Some(r.string_bounded(leader + ui(r)?.leader_name, 128)?)
                };
                Ok(json!({"army_group":army_name(r, g)?,"field_marshal":marshal}))
            })
            .collect::<Result<Vec<_>>>()?;
        Ok(
            json!({"army_groups":groups,"armies":armies,"unassigned":unassigned,
            "unassigned_by_state":by_state}),
        )
    })
}
/// Most divisions the planner puts into one army.
pub(super) const ARMY_CAP: usize = 24;

/// What `hoi4_army_assign` does for the unassigned divisions in one state.
#[derive(Debug, PartialEq)]
pub(super) enum AssignMode {
    /// Fold them into the standing army `army` without disturbing the others.
    Absorb { army: String },
    /// No army stands there: the ordinary new-army path creates one.
    CreateNew,
}
#[derive(Debug, PartialEq)]
pub(super) struct AssignPlan {
    pub state: i32,
    pub mode: AssignMode,
    /// Unassigned divisions standing in the state.
    pub unassigned: usize,
    /// Divisions the action is expected to move (all of them unless the army is nearly full).
    pub take: usize,
    /// Unassigned divisions left over because the army reached the cap.
    pub overflow: usize,
    /// Divisions of the receiving army afterwards (None for a new army).
    pub army_after: Option<usize>,
    /// A new army larger than the cap needs this many divisions split off afterwards.
    pub then_split: usize,
    /// Divisions of the receiving army standing in the state (0 for a new army).
    pub target_here: usize,
}
impl AssignPlan {
    pub(super) fn to_json(&self) -> Value {
        let (mode, army) = match &self.mode {
            AssignMode::Absorb { army } => ("absorb", json!(army)),
            AssignMode::CreateNew => ("create_new", Value::Null),
        };
        json!({"state_id":self.state,"mode":mode,"army":army,"unassigned":self.unassigned,
            "take":self.take,"overflow":self.overflow,"army_after":self.army_after,
            "then_split":self.then_split,"army_cap":ARMY_CAP,
            "target_here":self.target_here,"executable": true,"unmapped_control": false})
    }
}

/// Decide, from a `hoi4_armies` view alone, where the unassigned divisions in
/// `state` go: into `target` (or, without one, the army standing there with
/// the most room under `cap`), or into a new army when none stands there.
/// Reads nothing and presses nothing; every refusal happens before any input.
pub(super) fn plan_assign(
    view: &Value,
    state: i32,
    target: Option<&str>,
    cap: usize,
) -> Result<AssignPlan> {
    let key = state.to_string();
    let unassigned = view["unassigned_by_state"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|row| row["state_id"].as_i64() == Some(state as i64))
        .filter_map(|row| row["divisions"].as_u64())
        .sum::<u64>() as usize;
    if unassigned == 0 {
        return Err(Error::new(
            "no_unassigned_here",
            format!(
                "No unassigned division stands in state {state} (hoi4_armies unassigned_by_state); \
                 divisions on the move are not counted. Nothing was clicked."
            ),
        ));
    }
    struct Standing {
        name: String,
        here: usize,
        total: usize,
    }
    let armies: Vec<Standing> = view["armies"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|a| {
            Some(Standing {
                name: a["army"].as_str()?.to_string(),
                here: a["states"][&key].as_u64().unwrap_or(0) as usize,
                total: a["division_count"]
                    .as_u64()
                    .map(|n| n as usize)
                    .or_else(|| a["divisions"].as_array().map(Vec::len))?,
            })
        })
        .collect();
    let chosen = match target {
        Some(name) => {
            let army = armies.iter().find(|a| a.name == name).ok_or_else(|| {
                Error::new(
                    "no_such_army",
                    format!("No own army is named {name}. Nothing was clicked."),
                )
            })?;
            if army.here == 0 {
                return Err(Error::new(
                    "target_not_in_state",
                    format!(
                        "{name} has no division in state {state}; absorbing is only planned for an army \
                         standing where the unassigned divisions are. Nothing was clicked."
                    ),
                ));
            }
            Some(army)
        }
        None => armies.iter().filter(|a| a.here > 0).max_by(|a, b| {
            let room = |x: &Standing| cap.saturating_sub(x.total);
            room(a)
                .cmp(&room(b))
                .then(a.here.cmp(&b.here))
                .then(b.name.cmp(&a.name))
        }),
    };
    let Some(army) = chosen else {
        return Ok(AssignPlan {
            state,
            mode: AssignMode::CreateNew,
            unassigned,
            take: unassigned,
            overflow: 0,
            army_after: None,
            then_split: unassigned.saturating_sub(cap),
            target_here: 0,
        });
    };
    let room = cap.saturating_sub(army.total);
    if room == 0 {
        return Err(Error::new(
            "army_full",
            format!(
                "{} already holds {} divisions (cap {cap}); {unassigned} unassigned division(s) wait in \
                 state {state}. Split that army with hoi4_army_split first or name another army. Nothing \
                 was clicked.",
                army.name, army.total
            ),
        ));
    }
    let take = unassigned.min(room);
    Ok(AssignPlan {
        state,
        mode: AssignMode::Absorb {
            army: army.name.clone(),
        },
        unassigned,
        take,
        overflow: unassigned - take,
        army_after: Some(army.total + take),
        then_split: 0,
        target_here: army.here,
    })
}

/// The unassigned division ids of `state` from the same view (settlement pool).
pub(super) fn unassigned_ids(view: &Value, state: i32) -> Vec<String> {
    view["unassigned_by_state"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|row| row["state_id"].as_i64() == Some(state as i64))
        .flat_map(|row| row["division_ids"].as_array().into_iter().flatten())
        .filter_map(|id| id.as_str().map(String::from))
        .collect()
}

/// Refusal decided before any input (and before the action is journaled).
pub(super) fn assign_preflight(_plan: &AssignPlan) -> Result<()> {
    Ok(())
}
/// Carry out a plan. A new army uses the ordinary new-army card. Absorbing is
/// the game's own affordance, found live: with the unassigned divisions
/// selected, a RIGHT-click on the standing army's card assigns the selection
/// ("Click to select / Right-click to assign the selected Divisions").
/// The box selection also takes divisions of other armies standing nearby, and
/// the right-click would move those too, so the selection size must equal the
/// unassigned plus the target army's divisions in this state; otherwise nothing
/// is clicked on the card.
/// Selection boxes (game points) for gathering a state's divisions, usual size first.
const ASSIGN_BOXES: [((f64, f64), (f64, f64)); 7] = [
    ((560.0, 260.0), (1360.0, 860.0)),
    ((560.0, 260.0), (1400.0, 980.0)),
    ((560.0, 220.0), (1480.0, 980.0)),
    ((480.0, 260.0), (1400.0, 980.0)),
    ((480.0, 220.0), (1560.0, 960.0)),
    ((420.0, 190.0), (1640.0, 980.0)),
    ((640.0, 300.0), (1300.0, 980.0)),
];
pub(super) fn assign(process: &dyn Process, keys: &MacKeys, plan: &AssignPlan) -> Result<()> {
    let army = match &plan.mode {
        AssignMode::CreateNew => return create(process, keys, plan.state, false),
        AssignMode::Absorb { army } => army,
    };
    clear_notices(process, keys)?;
    find_state(process, keys, plan.state)?;
    let expected = plan.unassigned + plan.target_here;
    // Divisions stand anywhere in the state, so try the usual box first and
    // then wider ones; the card is only right-clicked when the selection holds
    // exactly the expected divisions (a new drag replaces the selection).
    let mut sizes = Vec::new();
    let mut exact = false;
    for (from, to) in ASSIGN_BOXES {
        keys.drag(GAME, from, to)?;
        std::thread::sleep(std::time::Duration::from_millis(350));
        let size = read(process, selection_size)? as usize;
        sizes.push(size);
        if size == expected {
            exact = true;
            break;
        }
    }
    if !exact {
        return Err(Error::new(
            "selection_mismatch",
            format!(
                "The box selections held {sizes:?} divisions but {expected} were expected (unassigned {} plus {army}'s \
                 {} here), so they also took divisions of another army or missed some. Nothing was assigned; the \
                 right-click on the card was not sent.",
                plan.unassigned, plan.target_here
            ),
        ));
    }
    let before = read(process, membership)?;
    let card = read(process, |r, i| card_rect(r, i, army, "portrait_btn"))?;
    clear_notices(process, keys)?;
    let (x, y) = card.center();
    keys.right_click(GAME, x, y)?;
    wait(process, "divisions assigned", |r, i| {
        let now = membership(r, i)?;
        Ok(now != before)
    })
    .map_err(|_| {
        Error::new(
            "input_readback_failed",
            "The right-click on the army card changed no division membership within 3 s; re-read hoi4_armies before trying again",
        )
    })
}

/// Refusals for `hoi4_army_create` that need no input, decided from the
/// `armies()` view: the box selection only gathers own divisions standing
/// around the state, and one that takes every division of an existing army
/// would delete it. Divisions on the move have no state and are not counted,
/// so this refuses only on positive evidence of where divisions stand.
pub(super) fn check_create(view: &Value, state: i32, absorb: bool) -> Result<()> {
    let key = state.to_string();
    let unassigned: u64 = view["unassigned_by_state"]
        .as_array()
        .map(|rows| {
            rows.iter()
                .filter(|row| row["state_id"].as_i64() == Some(state as i64))
                .filter_map(|row| row["divisions"].as_u64())
                .sum()
        })
        .unwrap_or(0);
    let mut standing: Vec<(String, u64)> = Vec::new();
    for army in view["armies"].as_array().into_iter().flatten() {
        if let (Some(name), Some(n)) = (army["army"].as_str(), army["states"][&key].as_u64()) {
            if n > 0 {
                standing.push((name.to_string(), n));
            }
        }
    }
    if unassigned > 0 || (absorb && !standing.is_empty()) {
        return Ok(());
    }
    if standing.is_empty() {
        return Err(Error::new(
            "no_own_divisions",
            format!(
                "No own division stands in state {state} (hoi4_armies lists none there, assigned or \
                 not); divisions on the move are not counted. Nothing was clicked. Pick a state from \
                 hoi4_armies (unassigned_by_state or an army's states) or wait for the divisions to arrive."
            ),
        ));
    }
    let named: Vec<String> = standing
        .iter()
        .map(|(name, n)| format!("{name} ({n})"))
        .collect();
    Err(Error::new(
        "already_in_army",
        format!(
            "All own divisions in state {state} already belong to: {}; no unassigned division stands \
             there, so a new army could only be made by deleting that army. Nothing was clicked. \
             Use absorb_existing: true to fold them into a new army, or hoi4_army_split on that army.",
            named.join(", ")
        ),
    ))
}
pub(super) fn selection_size(r: &Reader<'_>, image: u64) -> Result<u64> {
    let idler = r.u64(image + game::layout(r).idler_global)?;
    r.u64(idler + ui(r)?.selection_size)
}
/// The new-army card (rightmost portrait button of the card bar) or, with
/// `leftmost`, the new-army-group card.
fn new_card(r: &Reader<'_>, image: u64, leftmost: bool) -> Result<Rect> {
    let u = ui(r)?;
    let idler = r.u64(image + game::layout(r).idler_global)?;
    let bar = r.u64(r.u64(idler + u.manager)? + u.leader_view)?;
    if r.u64(bar)? != image + u.leader_view_vtable {
        return Err(Error::new("invalid_gui", "Exact army card bar required"));
    }
    let (_, data, count) = r.array_header(bar + u.leader_portraits)?;
    // The two "new" cards have no group handle: the first in the bar's list
    // creates an army group, the last an army. Their screen position changes
    // when the bar wraps into rows, so the list order decides.
    let mut empty = Vec::new();
    for index in 0..count.min(64) as u64 {
        let portrait = r.u64(data + index * 8)?;
        if r.u64(portrait + u.portrait_group)? == 0 {
            empty.push(portrait);
        }
    }
    let portrait = if leftmost {
        empty.first()
    } else {
        empty.last()
    }
    .copied()
    .ok_or_else(|| Error::new("gui_target_unavailable", "New army card not found"))?;
    let window = gui_tree::container(r, r.u64(portrait + u.portrait_window)?, image)?
        .ok_or_else(|| Error::new("invalid_gui", "New army card window required"))?;
    let button = gui_tree::child(r, image, window, "portrait_btn")?
        .ok_or_else(|| Error::new("gui_target_unavailable", "New army card not found"))?;
    gui_tree::rect(r, image, button, GAME)
}

/// Create an army from own divisions around state `state`.
pub(super) fn create(
    process: &dyn Process,
    keys: &MacKeys,
    state: i32,
    absorb: bool,
) -> Result<()> {
    clear_notices(process, keys)?;
    // The selection box stays clear of the left-hand panels; Escape is not used
    // because on a free map it opens the game menu.
    find_state(process, keys, state)?;
    // Unassigned divisions can stand anywhere in the state: when the usual box
    // does not hold exactly them, try wider boxes (a new drag replaces the
    // selection) and keep the first exact one; otherwise fall back to the usual
    // box and the old outcome handling below.
    let unassigned_here = armies(process)
        .ok()
        .map(|view| unassigned_ids(&view, state).len())
        .filter(|n| *n > 0);
    keys.drag(GAME, ASSIGN_BOXES[0].0, ASSIGN_BOXES[0].1)?;
    wait(process, "select divisions", |r, i| {
        Ok(selection_size(r, i)? > 0)
    })?;
    if let Some(expected) = unassigned_here.filter(|_| !absorb) {
        if read(process, selection_size)? as usize != expected {
            for (from, to) in ASSIGN_BOXES.iter().skip(1) {
                keys.drag(GAME, *from, *to)?;
                std::thread::sleep(std::time::Duration::from_millis(350));
                if read(process, selection_size)? as usize == expected {
                    break;
                }
            }
            if read(process, selection_size)? as usize != expected {
                keys.drag(GAME, ASSIGN_BOXES[0].0, ASSIGN_BOXES[0].1)?;
                wait(process, "select divisions", |r, i| {
                    Ok(selection_size(r, i)? > 0)
                })?;
            }
        }
    }
    let before = read(process, membership)?;
    let groups_before: BTreeSet<u64> = before.values().copied().collect();
    let card = read(process, |r, i| new_card(r, i, false))?;
    clear_notices(process, keys)?;
    let (x, y) = card.center();
    keys.click(GAME, x, y)?;
    // "Reassign Armies" appears when the box selection took every division
    // of an existing army, which would be deleted with its orders. Never
    // accept: decline and say what to do instead (accepting once merged two
    // armies of 24 and 19 divisions into one of 52).
    std::thread::sleep(std::time::Duration::from_millis(500));
    if read(process, super::ui::reassign_open)? {
        if absorb {
            // The caller asked for the armies standing here to be folded in.
            super::ui::accept_command(process, keys)?;
            return wait(process, "army created", |r, i| {
                Ok(membership(r, i)?
                    .values()
                    .any(|g| *g != 0 && !groups_before.contains(g)))
            });
        }
        super::ui::decline_stale(process, keys)?;
        return Err(Error::new(
            "invalid_target",
            "Divisions of an existing army stand here, so a new army would delete it: deploy new divisions \
             to another state (hoi4_train deploy_state_id) or use hoi4_army_split on that army",
        ));
    }
    wait(process, "army created", |r, i| {
        Ok(membership(r, i)?
            .values()
            .any(|g| *g != 0 && !groups_before.contains(g)))
    })
    .map_err(|_| {
        Error::new(
            "input_readback_failed",
            "No new army appeared within 3 s of the new-army card click. The pre-input check found \
             own divisions standing in this state, so this is not proof of any single cause: \
             re-read hoi4_armies before trying again (the divisions may have moved or been assigned)",
        )
    })
}

/// Own army groups (from the own armies).
fn army_groups(r: &Reader<'_>, image: u64) -> Result<BTreeSet<u64>> {
    let u = ui(r)?;
    let mut out = BTreeSet::new();
    for group in membership(r, image)?.into_values().filter(|g| *g != 0) {
        let parent = r.u64(group + u.army_group)?;
        if parent != 0 {
            out.insert(parent);
        }
    }
    Ok(out)
}
/// The own army or army group named `army` (names must be unique).
fn group_named(r: &Reader<'_>, image: u64, army: &str) -> Result<u64> {
    let mut groups: BTreeSet<u64> = membership(r, image)?
        .into_values()
        .filter(|g| *g != 0)
        .collect();
    groups.extend(army_groups(r, image)?);
    let mut found = None;
    for group in groups {
        if army_name(r, group)? == army {
            if found.is_some() {
                return Err(Error::new(
                    "invalid_target",
                    "More than one own army has this name",
                ));
            }
            found = Some(group);
        }
    }
    found.ok_or_else(|| Error::new("invalid_target", "No own army with this name"))
}
/// Battle-plan orders assigned to an army.
pub(super) fn orders(r: &Reader<'_>, image: u64, army: &str) -> Result<usize> {
    let group = group_named(r, image, army)?;
    Ok(r.array_header(group + ui(r)?.group_orders)?.2)
}
/// Widget `name` in the card of army `army` (the card's portrait window).
fn card_widget(r: &Reader<'_>, image: u64, army: &str, name: &str) -> Result<u64> {
    let u = ui(r)?;
    let group = group_named(r, image, army)?;
    let handle = r.u64(group + u.group_handle)?;
    let idler = r.u64(image + game::layout(r).idler_global)?;
    let bar = r.u64(r.u64(idler + u.manager)? + u.leader_view)?;
    if r.u64(bar)? != image + u.leader_view_vtable {
        return Err(Error::new("invalid_gui", "Exact army card bar required"));
    }
    let (_, data, count) = r.array_header(bar + u.leader_portraits)?;
    for index in 0..count.min(64) as u64 {
        let portrait = r.u64(data + index * 8)?;
        if r.u64(portrait + u.portrait_group)? != handle {
            continue;
        }
        let window = gui_tree::container(r, r.u64(portrait + u.portrait_window)?, image)?
            .ok_or_else(|| Error::new("invalid_gui", "Army card window required"))?;
        return gui_tree::child(r, image, window, name)?
            .ok_or_else(|| Error::new("gui_target_unavailable", "Army card widget not found"));
    }
    Err(Error::new("gui_target_unavailable", "Army card not shown"))
}
fn card_rect(r: &Reader<'_>, image: u64, army: &str, name: &str) -> Result<Rect> {
    gui_tree::rect(r, image, card_widget(r, image, army, name)?, GAME)
}

/// Select exactly `army` with its card: the selection must hold all of its
/// divisions, not a selection left over from before.
pub(super) fn select_army(process: &dyn Process, keys: &MacKeys, army: &str) -> Result<Rect> {
    let size = read(process, |r, i| {
        let group = group_named(r, i, army)?;
        Ok(membership(r, i)?.values().filter(|g| **g == group).count() as u64)
    })?;
    let card = read(process, |r, i| card_rect(r, i, army, "portrait_btn"))?;
    let (x, y) = card.center();
    keys.click(GAME, x, y)?;
    wait(process, "select army", |r, i| {
        Ok(selection_size(r, i)? & 0xffff_ffff == size)
    })?;
    std::thread::sleep(std::time::Duration::from_millis(300));
    Ok(card)
}
/// Screen areas covered by the top bar, the selected army's panel, the
/// battle-plan toolbar and the army card bar.
fn covered(x: f64, y: f64) -> bool {
    // The army panel lists divisions as green bars and grows with the
    // army (to y 930 for 43 divisions), so the whole left strip is out.
    y < 90.0 || x < 450.0 || y > 830.0 || (x > 1560.0 && y < 300.0)
}
/// Up to `count` distinct solid 7x7 patches that front-line drawing changed
/// (strongly, in every pixel), nearest the screen centre.
fn highlighted(
    plain: &Capture,
    drawing: &Capture,
    skip: &dyn Fn(f64, f64) -> bool,
    count: usize,
) -> Vec<(f64, f64)> {
    let changed = |x: f64, y: f64| match (plain.rgb(x, y), drawing.rgb(x, y)) {
        (Some(a), Some(b)) => {
            let d = |p: u8, q: u8| (i32::from(p) - i32::from(q)).abs();
            d(a.0, b.0) + d(a.1, b.1) + d(a.2, b.2) > 90
        }
        _ => false,
    };
    let mut found: Vec<(f64, f64, f64)> = Vec::new();
    let mut y = 0.0;
    while y < 1072.0 {
        let mut x = 0.0;
        while x < 1912.0 {
            if !covered(x, y)
                && !skip(x, y)
                && changed(x, y)
                && (0..7).all(|dy| (0..7).all(|dx| changed(x + f64::from(dx), y + f64::from(dy))))
            {
                let d = (x - 960.0).powi(2) + (y - 540.0).powi(2);
                found.push((d, x + 3.0, y + 3.0));
            }
            x += 3.0;
        }
        y += 3.0;
    }
    found.sort_by(|a, b| a.0.total_cmp(&b.0));
    let mut out: Vec<(f64, f64)> = Vec::new();
    for (_, x, y) in found {
        if out.iter().all(|(a, b)| (a - x).hypot(b - y) > 40.0) {
            out.push((x, y));
            if out.len() == count {
                break;
            }
        }
    }
    out
}
/// Give `army` a front line against the owner of state `state`: the army
/// card selects the army, Z enters front-line drawing, the Find view centres
/// the state and one click inside it places the front along that country's
/// border. Verified by one more battle-plan order on the army.
/// Own state ids (owner is the player).
fn own_states(r: &Reader<'_>, image: u64) -> Result<Vec<i32>> {
    let g = game::layout(r);
    let l = crate::construction::layout(r);
    let gs = r.u64(image + g.gs_global)?;
    let tag = r.i32(gs + g.player_tag)?;
    let (_, data, count) = r.array_header(gs + l.gs_states)?;
    let mut out = Vec::new();
    for index in 1..count.min(4096) as u64 {
        let state = r.u64(data + index * 8)?;
        if state != 0
            && r.u64(state)? == image + l.state_vtable
            && r.i32(state + l.state_owner)? == tag
        {
            out.push(r.i32(state + l.state_id)?);
        }
    }
    Ok(out)
}
pub(super) fn front(process: &dyn Process, keys: &MacKeys, army: &str, state: i32) -> Result<()> {
    clear_notices(process, keys)?;
    let own = read(process, own_states)?;
    if own.contains(&state) {
        return Err(Error::new(
            "invalid_target",
            "This state is own; choose a state of the country to face",
        ));
    }
    let before = read(process, |r, i| orders(r, i, army))?;
    let card = select_army(process, keys, army)?;
    find_state(process, keys, state)?;
    std::thread::sleep(std::time::Duration::from_millis(500));
    // The fronts this army can take are highlighted only in front-line
    // drawing, in a colour that varies by army and lighting; compare one
    // capture before Z with one after it, at the same camera position.
    let plain = keys.capture(GAME)?;
    keys.press(Key::letter('z')?)?;
    std::thread::sleep(std::time::Duration::from_millis(600));
    let drawing = keys.capture(GAME)?;
    // The card's tooltip may change under the resting cursor.
    let tooltip =
        |x: f64, y: f64| x > f64::from(card.x) - 60.0 && x < f64::from(card.x) + 460.0 && y > 680.0;
    let candidates = highlighted(&plain, &drawing, &tooltip, 4);
    for (x, y) in candidates {
        keys.click(GAME, x, y)?;
        let deadline = std::time::Instant::now() + std::time::Duration::from_millis(600);
        while std::time::Instant::now() < deadline {
            if read(process, |r, i| orders(r, i, army))? > before {
                return Ok(());
            }
            std::thread::sleep(std::time::Duration::from_millis(40));
        }
    }
    // Leave front-line drawing without touching the map.
    let _ = keys.press(Key::Escape);
    Err(Error::new(
        "input_readback_failed",
        "No front line was placed: no highlighted front toward this country near the state \
         (the border may already be fully covered by other fronts)",
    ))
}

/// Whether the army's battle plan is executing (the card's execute animation).
pub(super) fn executing(r: &Reader<'_>, image: u64, army: &str) -> Result<bool> {
    gui_tree::shown(r, card_widget(r, image, army, "btn_execute_anim")?)
}
/// Execute (`run`) or stop the battle plan of `army` with the card's play or
/// stop button. Verified by the card's execute animation turning on or off.
pub(super) fn execute(process: &dyn Process, keys: &MacKeys, army: &str, run: bool) -> Result<()> {
    clear_notices(process, keys)?;
    if read(process, |r, i| orders(r, i, army))? == 0 {
        return Err(Error::new(
            "invalid_target",
            "This army has no battle plan; draw a front line first",
        ));
    }
    if read(process, |r, i| executing(r, i, army))? == run {
        return Ok(());
    }
    let name = if run { "btn_execute" } else { "btn_stop" };
    let rect = settled(process, "plan button", |r, i| {
        let button = card_widget(r, i, army, name)?;
        if !gui_tree::shown(r, button)? || gui_tree::disabled(r, button)? {
            return Err(Error::new(
                "gui_target_unavailable",
                "The game does not offer this plan button now",
            ));
        }
        gui_tree::rect(r, i, button, GAME)
    })?;
    let (x, y) = rect.center();
    keys.click(GAME, x, y)?;
    wait(process, "plan state changed", |r, i| {
        Ok(executing(r, i, army)? == run)
    })
}

/// Province IDs where the divisions of `army` stand.
fn army_provinces(r: &Reader<'_>, image: u64, army: &str) -> Result<Vec<i32>> {
    let u = ui(r)?;
    let group = group_named(r, image, army)?;
    let country = own_country(r, image)?;
    let province_vtable = image + crate::construction::layout(r).province_vtable;
    let (_, data, count) = r.array_header(country + u.country_divisions)?;
    let mut out = Vec::new();
    for index in 0..count.min(4096) as u64 {
        let division = r.u64(data + index * 8)?;
        if r.u64(division + u.division_group)? != group {
            continue;
        }
        let location = r.u64(division + u.division_location)?;
        if location != 0 && r.u64(location)? == province_vtable {
            out.push(r.i32(location + u.province_id)?);
        }
    }
    Ok(out)
}
/// All battle-plan order nodes of `army` (top orders and their sub-orders).
pub(super) fn plan_nodes(r: &Reader<'_>, image: u64, army: &str) -> Result<usize> {
    let u = ui(r)?;
    let group = group_named(r, image, army)?;
    let mut stack = vec![];
    let (_, data, count) = r.array_header(group + u.group_orders)?;
    for index in 0..count.min(64) as u64 {
        stack.push((r.u64(data + index * 8)?, 0));
    }
    let mut seen = 0;
    while let Some((order, depth)) = stack.pop() {
        if r.u64(order)? != image + u.order_vtable {
            return Err(Error::new(
                "invalid_army_type",
                "Exact battle-plan order required",
            ));
        }
        seen += 1;
        if seen > 512 {
            return Err(Error::new("invalid_array", "Battle plan exceeds bound"));
        }
        if depth < 6 {
            for list in u.order_children {
                let (_, data, count) = r.array_header(order + list)?;
                for index in 0..count.min(64) as u64 {
                    stack.push((r.u64(data + index * 8)?, depth + 1));
                }
            }
        }
    }
    Ok(seen)
}

/// Draw an offensive line for `army` (which must already hold a front line)
/// into state `state`: the army card selects the army, X enters offensive-line
/// drawing, the Find view centres the state and one right-button drag runs
/// from the army's side to the state; the game joins it to the front.
/// Verified by new battle-plan order nodes on the army.
pub(super) fn offensive(
    process: &dyn Process,
    keys: &MacKeys,
    game_dir: &std::path::Path,
    army: &str,
    state: i32,
) -> Result<()> {
    clear_notices(process, keys)?;
    if read(process, |r, i| orders(r, i, army))? == 0 {
        return Err(Error::new(
            "invalid_target",
            "This army has no front line; draw one first",
        ));
    }
    let provinces = read(process, |r, i| army_provinces(r, i, army))?;
    let (dx, dy) = super::mapdata::direction_to(game_dir, state, &provinces)?;
    let before = read(process, |r, i| plan_nodes(r, i, army))?;
    let card = read(process, |r, i| card_rect(r, i, army, "portrait_btn"))?;
    let (x, y) = card.center();
    keys.click(GAME, x, y)?;
    wait(process, "select army", |r, i| Ok(selection_size(r, i)? > 0))?;
    find_state(process, keys, state)?;
    keys.press(Key::letter('x')?)?;
    std::thread::sleep(std::time::Duration::from_millis(300));
    let clamp = |x: f64, y: f64| (x.clamp(320.0, 1880.0), y.clamp(120.0, 880.0));
    let from = clamp(960.0 + dx * 260.0, 540.0 + dy * 260.0);
    let to = clamp(960.0 - dx * 40.0, 540.0 - dy * 40.0);
    keys.right_drag(GAME, from, to)?;
    wait(process, "offensive line placed", |r, i| {
        Ok(plan_nodes(r, i, army)? > before)
    })
}
/// Divisions of `army` whose movement target (province id at +0x2ac of the
/// division, mapped live on 24 July 1941 by diffing a right-click move order)
/// lies in the provinces `wanted`, and the army's division count.
pub(super) fn heading_for(
    r: &Reader<'_>,
    image: u64,
    army: &str,
    wanted: &[i32],
) -> Result<(usize, usize)> {
    let u = ui(r)?;
    let group = group_named(r, image, army)?;
    let country = own_country(r, image)?;
    let (_, data, count) = r.array_header(country + u.country_divisions)?;
    let (mut heading, mut total) = (0, 0);
    for index in 0..count.min(4096) as u64 {
        let division = r.u64(data + index * 8)?;
        if r.u64(division + u.division_group)? != group {
            continue;
        }
        total += 1;
        if wanted.contains(&r.i32(division + 0x2ac)?) {
            heading += 1;
        }
    }
    Ok((heading, total))
}
/// Own divisions (by id) whose movement target province (division +0x2ac, the field `heading_for`
/// uses) lies in `wanted`: (heading, listed). Divisions not found are not counted as listed.
pub(super) fn divisions_heading_for(
    r: &Reader<'_>,
    image: u64,
    ids: &[String],
    wanted: &[i32],
) -> Result<(usize, usize)> {
    let u = ui(r)?;
    let country = own_country(r, image)?;
    let (_, data, count) = r.array_header(country + u.country_divisions)?;
    let (mut heading, mut listed) = (0, 0);
    for index in 0..count.min(4096) as u64 {
        let division = r.u64(data + index * 8)?;
        let handle = r.read(division + u.division_handle, 8)?;
        let id = format!(
            "{:08x}:{:08x}",
            u32::from_le_bytes(handle[..4].try_into().unwrap()),
            u32::from_le_bytes(handle[4..].try_into().unwrap())
        );
        if !ids.contains(&id) {
            continue;
        }
        listed += 1;
        if wanted.contains(&r.i32(division + 0x2ac)?) {
            heading += 1;
        }
    }
    Ok((heading, listed))
}
/// Box-select the own divisions standing in `state` (Find view, then the usual boxes) until the
/// selection holds exactly `expected` divisions; refuses with `selection_mismatch` otherwise.
pub(super) fn box_select_exact(
    process: &dyn Process,
    keys: &MacKeys,
    state: i32,
    expected: usize,
) -> Result<()> {
    find_state(process, keys, state)?;
    let mut sizes = Vec::new();
    for (from, to) in ASSIGN_BOXES {
        keys.drag(GAME, from, to)?;
        std::thread::sleep(std::time::Duration::from_millis(350));
        let size = (read(process, selection_size)? & 0xffff_ffff) as usize;
        sizes.push(size);
        if size == expected {
            return Ok(());
        }
    }
    Err(Error::new(
        "selection_mismatch",
        format!("The box selections held {sizes:?} divisions but exactly {expected} were wanted; nothing was ordered"),
    ))
}
/// Order `army` to move to the centre of state `state` (select the army by
/// its card, Find view centres the state, one right click). A plain move order
/// attacks whatever stands in the target province; battle plans are replaced
/// by manual control for these divisions.
pub(super) fn move_to(process: &dyn Process, keys: &MacKeys, army: &str, state: i32) -> Result<()> {
    clear_notices(process, keys)?;
    let card = read(process, |r, i| card_rect(r, i, army, "portrait_btn"))?;
    let (x, y) = card.center();
    keys.click(GAME, x, y)?;
    wait(process, "select army", |r, i| Ok(selection_size(r, i)? > 0))?;
    find_state(process, keys, state)?;
    keys.right_click(GAME, 960.0, 540.0)?;
    std::thread::sleep(std::time::Duration::from_millis(400));
    Ok(())
}
/// Name of the commander of `army`, if any.
pub(super) fn commander(r: &Reader<'_>, image: u64, army: &str) -> Result<Option<String>> {
    let u = ui(r)?;
    let leader = r.u64(group_named(r, image, army)? + u.group_leader)?;
    if leader == 0 {
        return Ok(None);
    }
    Ok(Some(r.string_bounded(leader + u.leader_name, 128)?))
}
fn leader_root(r: &Reader<'_>, image: u64) -> Result<u64> {
    let u = ui(r)?;
    let idler = r.u64(image + game::layout(r).idler_global)?;
    let view = r.u64(r.u64(idler + u.manager)? + u.armies_view)?;
    let window = r.u64(view + u.leader_window)?;
    let root = r.u64(window + u.leader_window_root)?;
    if r.u64(root)? != image + u.container_vtable {
        return Err(Error::new("invalid_gui", "Commander list window required"));
    }
    gui_tree::container(r, root + 0x30, image)?
        .ok_or_else(|| Error::new("invalid_gui", "Commander list window required"))
}
pub(super) fn leader_list_open(r: &Reader<'_>, image: u64) -> Result<bool> {
    gui_tree::visible(r, leader_root(r, image)?)
}
/// Visible band (top, bottom) of the commander list's rows in game pixels:
/// from the bottom of the sort header to the top of the Unassign button. The
/// window sits at different heights depending on the panels open beside it.
fn leader_band(r: &Reader<'_>, image: u64) -> (i32, i32) {
    let found = (|| -> Result<(i32, i32)> {
        let root = leader_root(r, image)?;
        let header = gui_tree::child(r, image, root, "sort_name")?
            .ok_or_else(|| Error::new("gui_target_unavailable", "Sort header not found"))?;
        let footer = gui_tree::child(r, image, root, "no_leader_button")?
            .ok_or_else(|| Error::new("gui_target_unavailable", "Unassign button not found"))?;
        let header = gui_tree::rect_unclipped(r, image, header, GAME)?;
        let footer = gui_tree::rect_unclipped(r, image, footer, GAME)?;
        Ok((header.y + header.h, footer.y))
    })();
    found.unwrap_or(LEADER_ROWS)
}
/// (general name, portrait rectangle) for each row of the commander list.
fn leader_rows(r: &Reader<'_>, image: u64) -> Result<Vec<(String, Rect)>> {
    Ok(leader_entries(r, image)?
        .into_iter()
        .map(|(n, rect, _)| (n, rect))
        .collect())
}
/// Like `leader_rows`, plus the row's Promote button while it is shown (a
/// general who is not yet a field marshal).
pub(super) fn leader_entries(
    r: &Reader<'_>,
    image: u64,
) -> Result<Vec<(String, Rect, Option<Rect>)>> {
    let u = ui(r)?;
    let root = leader_root(r, image)?;
    let mut grid = None;
    for child in gui_tree::children(r, root)? {
        if let Some(inner) = gui_tree::container(r, child, image)? {
            if let Some(found) = gui_tree::child(r, image, inner, "leaders_grid")? {
                grid = Some(found);
            }
        }
    }
    let grid =
        grid.ok_or_else(|| Error::new("gui_target_unavailable", "Commander list not found"))?;
    let (_, data, count) = r.array_header(grid + u.grid_items)?;
    let mut out = Vec::new();
    for index in 0..count.min(256) as u64 {
        let item = r.u64(data + index * 8)?;
        let entry = r.u64(item + u.item_container)?;
        let base = if r.u64(entry)? == image + u.container_vtable {
            entry + 0x30
        } else {
            entry
        };
        let Some(frame) = gui_tree::container(r, base, image)? else {
            continue;
        };
        let (Some(name), Some(button)) = (
            gui_tree::child(r, image, frame, "name")?,
            gui_tree::child(r, image, frame, "leader_button")?,
        ) else {
            continue;
        };
        let name = r.string_bounded(name + u.text, 128)?;
        // The button's own sprite rectangle is scaled oddly; it sits at a fixed
        // offset from the portrait in unitleaderwindow.gui (414,56 against 56,5).
        let promote_shown = match gui_tree::child(r, image, frame, "promote_btn")? {
            Some(b) => gui_tree::shown(r, b)?,
            None => false,
        };
        if let Ok(rect) = gui_tree::rect_unclipped(r, image, button, GAME) {
            let promote = promote_shown.then_some(Rect {
                x: rect.x + 358,
                y: rect.y + 51,
                w: 98,
                h: 28,
            });
            out.push((name, rect, promote));
        }
    }
    Ok(out)
}

/// Assign general `general` to `army` from the army panel's commander list.
/// Verified when the army's commander is that general.
pub(super) fn assign_commander(
    process: &dyn Process,
    keys: &MacKeys,
    army: &str,
    general: &str,
) -> Result<()> {
    // The list sometimes closes under the pointer while rows scroll; each retry
    // starts again from the army's card.
    let mut last = None;
    for _ in 0..3 {
        match assign_commander_once(process, keys, army, general, true) {
            Ok(()) => return Ok(()),
            Err(e) if matches!(e.code, "input_readback_failed" | "gui_target_unavailable") => {
                last = Some(e);
                super::ui::dismiss(process, keys, |r, i| {
                    Ok(!leader_list_open(r, i).unwrap_or(false))
                });
            }
            Err(e) => return Err(e),
        }
    }
    Err(last.unwrap_or_else(|| Error::new("input_readback_failed", "Commander not assigned")))
}
fn assign_commander_once(
    process: &dyn Process,
    keys: &MacKeys,
    army: &str,
    general: &str,
    allow_promote: bool,
) -> Result<()> {
    clear_notices(process, keys)?;
    if read(process, |r, i| commander(r, i, army))?.is_some_and(|c| c.eq_ignore_ascii_case(general))
    {
        return Ok(());
    }
    // An army or an army group is selected by its own card; the group's
    // panel then offers its field marshal slot at the same place.
    let card = settled(process, "card", |r, i| {
        card_rect(r, i, army, "portrait_btn")
    })?;
    let (x, y) = card.center();
    keys.click(GAME, x, y)?;
    wait(process, "select", |r, i| {
        Ok(selection_size(r, i)? & 0xffff_ffff > 0)
    })?;
    std::thread::sleep(std::time::Duration::from_millis(250));
    keys.click(GAME, COMMANDER_SLOT.0, COMMANDER_SLOT.1)?;
    wait(process, "commander list open", leader_list_open)?;
    let wanted = general.to_lowercase();
    let find = |r: &Reader<'_>, i: u64| -> Result<Option<Rect>> {
        Ok(leader_rows(r, i)?
            .into_iter()
            .find(|(n, _)| n.to_lowercase() == wanted)
            .map(|(_, rect)| rect))
    };
    let Some(mut rect) = settled(process, "commander list", find)? else {
        let names: Vec<String> = read(process, leader_rows)?
            .into_iter()
            .map(|(n, _)| n)
            .collect();
        super::ui::dismiss(process, keys, |r, i| Ok(!leader_list_open(r, i)?));
        let close = super::commanders::closest_names(general, &names, 5);
        return Err(Error::new(
            "unknown_commander",
            format!(
                "No such general in the list. Closest: {}. All listed: {}",
                close.join(", "),
                names.join(", ")
            ),
        ));
    };
    let band = read(process, |r, i| Ok(leader_band(r, i)))?;
    for _ in 0..40 {
        let bottom = rect.y + rect.h;
        let top = bottom - PORTRAIT_DRAWN;
        if top >= band.0 && bottom <= band.1 {
            break;
        }
        // The list can flicker shut while the game rebuilds it; give it a moment,
        // then open it again from the commander slot.
        let mut open = false;
        for attempt in 0..8 {
            if read(process, |r, i| Ok(leader_list_open(r, i).unwrap_or(false)))? {
                open = true;
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(250));
            if attempt == 4 {
                keys.click(GAME, COMMANDER_SLOT.0, COMMANDER_SLOT.1)?;
            }
        }
        if !open {
            return Err(Error::new("input_readback_failed", "Commander list closed"));
        }
        let pixels = if top < band.0 { 240 } else { -240 };
        keys.scroll(GAME, (960.0, 560.0), pixels)?;
        rect = read(process, find)?
            .ok_or_else(|| Error::new("gui_target_unavailable", "General left the list"))?;
    }
    let rect = settled(process, "general row", |r, i| {
        find(r, i)?.ok_or_else(|| Error::new("gui_target_unavailable", "General left the list"))
    })?;
    let bottom = rect.y + rect.h;
    if bottom - PORTRAIT_DRAWN < band.0 || bottom > band.1 {
        return Err(Error::new(
            "gui_target_unavailable",
            format!("General row not in view: {rect:?}"),
        ));
    }
    // Only field marshals can lead an army group: promote a plain general first.
    let is_group = read(process, |r, i| {
        Ok(army_groups(r, i)?.contains(&group_named(r, i, army)?))
    })?;
    let promote_of = |r: &Reader<'_>, i: u64| -> Result<Option<Rect>> {
        Ok(leader_entries(r, i)?
            .into_iter()
            .find(|(n, _, _)| n.to_lowercase() == wanted)
            .and_then(|(_, _, p)| p))
    };
    if is_group && allow_promote {
        if let Some(button) = read(process, promote_of)? {
            let (x, y) = button.center();
            keys.click(GAME, x, y)?;
            super::ui::accept_command(process, keys)?;
            wait(process, "general promoted", |r, i| {
                Ok(promote_of(r, i)?.is_none())
            })?;
            std::thread::sleep(std::time::Duration::from_millis(400));
            // The game closes the list on promotion: open it again.
            return assign_commander_once(process, keys, army, general, false);
        }
    }
    let rect = settled(process, "general row", |r, i| {
        find(r, i)?.ok_or_else(|| Error::new("gui_target_unavailable", "General left the list"))
    })?;
    let bottom = rect.y + rect.h;
    if !read(process, |r, i| Ok(leader_list_open(r, i).unwrap_or(false)))? {
        return Err(Error::new("input_readback_failed", "Commander list closed"));
    }
    keys.click(
        GAME,
        f64::from(rect.x) + 34.0,
        f64::from(bottom - PORTRAIT_DRAWN / 2),
    )?;
    // Moving a commander who leads another army asks for confirmation.
    super::ui::accept_command(process, keys)?;
    wait(process, "commander assigned", |r, i| {
        Ok(commander(r, i, army)?.is_some_and(|c| c.to_lowercase() == wanted))
    })
}

/// Group own armies `names` into a new army group: the first army's card,
/// the others' cards with Shift, then the new-army-group card. Verified when
/// all of them belong to one army group that did not exist before.
pub(super) fn create_group(process: &dyn Process, keys: &MacKeys, names: &[String]) -> Result<()> {
    clear_notices(process, keys)?;
    let u = &MACOS;
    let before = read(process, army_groups)?;
    let mut size = 0;
    for (index, name) in names.iter().enumerate() {
        let count = read(process, |r, i| {
            let group = group_named(r, i, name)?;
            Ok(membership(r, i)?.values().filter(|g| **g == group).count() as u64)
        })?;
        size += count;
        let card = settled(process, "army card", |r, i| {
            card_rect(r, i, name, "portrait_btn")
        })?;
        let (x, y) = card.center();
        if index == 0 {
            keys.click(GAME, x, y)?;
        } else {
            keys.shift_click(GAME, x, y)?;
        }
        wait(process, "army selected", |r, i| {
            Ok(selection_size(r, i)? & 0xffff_ffff == size)
        })?;
    }
    let card = settled(process, "new army group card", |r, i| new_card(r, i, true))?;
    let (x, y) = card.center();
    keys.click(GAME, x, y)?;
    wait(process, "army group created", |r, i| {
        let parents: BTreeSet<u64> = names
            .iter()
            .map(|n| r.u64(group_named(r, i, n)? + u.army_group))
            .collect::<Result<_>>()?;
        Ok(parents.len() == 1 && parents.iter().all(|p| *p != 0 && !before.contains(p)))
    })
}

/// First division row of the selected army's panel (game pixels), the row
/// pitch and the rows visible before the list scrolls.
const PANEL_ROW: (f64, f64) = (200.0, 252.0);
const PANEL_PITCH: f64 = 33.0;
const PANEL_ROWS: usize = 19;

/// Split the first `count` divisions of `army` (list order) into a new army:
/// the army's card, the first row, Shift on row `count`, then the new-army
/// card. Verified by a new army holding exactly those divisions.
pub(super) fn split(process: &dyn Process, keys: &MacKeys, army: &str, count: usize) -> Result<()> {
    if count == 0 || count > PANEL_ROWS {
        return Err(Error::new(
            "invalid_arguments",
            format!("count must be 1..{PANEL_ROWS} (the rows visible without scrolling)"),
        ));
    }
    clear_notices(process, keys)?;
    let total = read(process, |r, i| {
        let group = group_named(r, i, army)?;
        Ok(membership(r, i)?.values().filter(|g| **g == group).count())
    })?;
    if count >= total {
        return Err(Error::new(
            "invalid_target",
            "The army has no more divisions than the split would take",
        ));
    }
    // The game may rebuild the source army's group object, so the new army
    // is recognised by its name and size, not by its pointer.
    let names_before: BTreeSet<String> = read(process, |r, i| {
        membership(r, i)?
            .values()
            .filter(|g| **g != 0)
            .map(|g| army_name(r, *g))
            .collect()
    })?;
    select_army(process, keys, army)?;
    keys.click(GAME, PANEL_ROW.0, PANEL_ROW.1)?;
    wait(process, "first division selected", |r, i| {
        Ok(selection_size(r, i)? & 0xffff_ffff == 1)
    })?;
    if count > 1 {
        keys.shift_click(
            GAME,
            PANEL_ROW.0,
            PANEL_ROW.1 + PANEL_PITCH * (count - 1) as f64,
        )?;
    }
    wait(process, "divisions selected", |r, i| {
        Ok(selection_size(r, i)? & 0xffff_ffff == count as u64)
    })?;
    let card = settled(process, "new army card", |r, i| new_card(r, i, false))?;
    let (x, y) = card.center();
    keys.click(GAME, x, y)?;
    wait(process, "army created", |r, i| {
        let now = membership(r, i)?;
        let mut sizes: BTreeMap<u64, usize> = BTreeMap::new();
        for g in now.values().filter(|g| **g != 0) {
            *sizes.entry(*g).or_default() += 1;
        }
        for (g, n) in sizes {
            if n == count && !names_before.contains(&army_name(r, g)?) {
                return Ok(true);
            }
        }
        Ok(false)
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    fn view() -> Value {
        json!({
            "unassigned_by_state": [{"state_id": 64, "divisions": 3}],
            "armies": [
                {"army": "1st Army", "states": {"10": 4, "64": 2}},
                {"army": "2nd Army", "states": {"10": 6}},
            ],
        })
    }
    #[test]
    fn unassigned_divisions_in_the_state_allow_the_box_selection() {
        assert!(check_create(&view(), 64, false).is_ok());
    }
    #[test]
    fn divisions_all_in_armies_are_refused_before_input_naming_the_armies() {
        let e = check_create(&view(), 10, false).unwrap_err();
        assert_eq!(e.code, "already_in_army");
        assert!(e.message.contains("1st Army (4)") && e.message.contains("2nd Army (6)"));
        assert!(e.message.contains("absorb_existing") && e.message.contains("hoi4_army_split"));
        assert!(e.message.contains("Nothing was clicked"));
    }
    fn assign_view() -> Value {
        json!({
            "unassigned_by_state": [
                {"state_id": 64, "divisions": 6, "division_ids": ["a","b","c","d","e","f"]},
                {"state_id": 66, "divisions": 3, "division_ids": ["g","h","i"]},
                {"state_id": 70, "divisions": 30, "division_ids": []}],
            "armies": [
                {"army": "Army 1", "division_count": 19, "states": {"64": 19}},
                {"army": "Army 2", "division_count": 10, "states": {"64": 4, "66": 6}},
                {"army": "Army 3", "division_count": 24, "states": {"70": 24}}],
        })
    }
    #[test]
    fn unassigned_divisions_go_to_the_standing_army_with_the_most_room() {
        let plan = plan_assign(&assign_view(), 64, None, ARMY_CAP).unwrap();
        // Army 1 has room for 5 of the 6, Army 2 for 14: Army 2 takes all six
        assert_eq!(plan.mode, AssignMode::Absorb { army: "Army 2".into() });
        assert_eq!((plan.take, plan.overflow, plan.army_after), (6, 0, Some(16)));
        let plan = plan_assign(&assign_view(), 64, Some("Army 1"), ARMY_CAP).unwrap();
        assert_eq!((plan.take, plan.overflow, plan.army_after), (5, 1, Some(24)));
        assert_eq!(plan.to_json()["unmapped_control"], json!(false));
        assert_eq!(plan.target_here, 19);
    }
    #[test]
    fn without_a_standing_army_a_new_one_is_planned_and_oversize_is_split_after() {
        let view = json!({"unassigned_by_state": [{"state_id": 5, "divisions": 30}], "armies": []});
        let plan = plan_assign(&view, 5, None, ARMY_CAP).unwrap();
        assert_eq!(plan.mode, AssignMode::CreateNew);
        assert_eq!((plan.take, plan.then_split), (30, 6));
        assert!(assign_preflight(&plan).is_ok());
    }
    #[test]
    fn assign_refuses_before_input_with_typed_errors() {
        let v = assign_view();
        assert_eq!(plan_assign(&v, 99, None, 24).unwrap_err().code, "no_unassigned_here");
        assert_eq!(plan_assign(&v, 64, Some("Army 9"), 24).unwrap_err().code, "no_such_army");
        assert_eq!(
            plan_assign(&v, 66, Some("Army 1"), 24).unwrap_err().code,
            "target_not_in_state"
        );
        // a state where the only standing army is full
        assert_eq!(plan_assign(&v, 70, None, 24).unwrap_err().code, "army_full");
        let plan = plan_assign(&v, 66, None, 24).unwrap();
        // absorbing into a standing army is executable (right-click on the army card)
        assert!(assign_preflight(&plan).is_ok());
        assert_eq!(plan.target_here, 6);
    }
    #[test]
    fn the_settlement_pool_is_the_unassigned_ids_of_that_state() {
        assert_eq!(unassigned_ids(&assign_view(), 66), vec!["g", "h", "i"]);
        assert!(unassigned_ids(&assign_view(), 1).is_empty());
    }
    #[test]
    fn absorb_existing_may_fold_the_armies_standing_there() {
        assert!(check_create(&view(), 10, true).is_ok());
    }
    #[test]
    fn a_state_without_own_divisions_is_refused_even_when_absorbing() {
        for absorb in [false, true] {
            let e = check_create(&view(), 99, absorb).unwrap_err();
            assert_eq!(e.code, "no_own_divisions");
            assert!(e.message.contains("state 99"));
        }
        let empty = json!({"unassigned_by_state": [], "armies": []});
        assert_eq!(
            check_create(&empty, 1, false).unwrap_err().code,
            "no_own_divisions"
        );
    }
}
