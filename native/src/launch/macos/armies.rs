//! Own armies (orders groups) and creating one from the divisions around a
//! state: the game's Find view centres the state, a box selection picks the
//! own divisions there, and the new-army card creates the army. Verified by a
//! new orders group in memory holding the selected divisions.
use super::ui::{clear_notices, find_state, read, settled, wait};
use crate::{
    error::{Error, Result},
    game,
    gui_tree::{self, Rect},
    memory::Reader,
    platform::{
        game_input::{Capture, GameInput, Key},
        Process,
    },
    xlayout::{self, fld, img},
};
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Copy)]
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
    /// Division movement target province id (the field `heading_for` uses).
    heading_for: u64,
    /// CContainerWindow's embedded container.
    container_base: u64,
}
/// macOS Mach-O 089EA519, mapped live on 18 October 2026 (Germany test run):
/// a box selection set the idler's selection size (+0x540) to 7 and the
/// new-army card put those 7 divisions into one orders group ("Army 1",
/// division +0xc0, name +0x160).
const MACOS: Ui = Ui {
    country_divisions: 0x250,
    division_vtable: 0x34ace60,
    division_group: 0xc0,
    division_handle: 0x18,
    group_name: 0x160,
    selection_size: 0x540,
    manager: 0x670,
    leader_view_vtable: 0x3670a20,
    leader_view: 0x1c0,
    leader_portraits: 0x38,
    portrait_window: 0x28,
    portrait_group: 0x30,
    group_handle: 0x8,
    group_orders: 0x98,
    division_location: 0x1f0,
    province_id: 0x9c,
    order_vtable: 0x3512bf0,
    // Drawing an offensive line added one COrderInstance under each list.
    order_children: [0x1c8, 0x2a0],
    // Commander list of the army panel: CArmiesView (+0x1d8) holds the
    // CArmyLeaderWindow (+0x480, root +0x518 in 1.19.4; +0x420/+0x4b8 in 1.19.3); choosing a general set the
    // group's leader (+0x88, name +0x38).
    armies_view: 0x1d8,
    leader_window: 0x480,
    leader_window_root: 0x518,
    grid_items: 0x190,
    item_container: 0x10,
    text: 0x100,
    group_leader: 0x88,
    leader_name: 0x38,
    container_vtable: 0x36ba978,
    // An army's CArmyGroup (name +0x160, leader +0x88 like an army); its
    // card's group handle is the army group's +0x8. Mapped live on
    // 18 October 2026 after grouping Army 7 and Army 8.
    army_group: 0x1b0,
    heading_for: 0x2ac,
    container_base: 0x30,
};
/// Owners of the fields above (macOS 1.19.4.1 RTTI names and vtables; 0 where
/// the owning class has no reviewed vtable).
const COUNTRY: xlayout::Class = xlayout::Class::new("CCountry", 0x357e388);
const DIVISION: xlayout::Class = xlayout::Class::new("CArmy", 0x34ace60);
const GROUP: xlayout::Class = xlayout::Class::new("COrdersGroup", 0x3516b78);
const LEADER: xlayout::Class = xlayout::Class::new("unit.leader", 0);
const IDLER: xlayout::Class = xlayout::Class::new("CInGameIdler", 0x35089d8);
const IFACE_MANAGER: xlayout::Class = xlayout::Class::new("idler.manager", 0);
const LEADER_VIEW: xlayout::Class = xlayout::Class::new("CLeaderGroupView", 0x3670a20);
const PORTRAIT: xlayout::Class = xlayout::Class::new("leader_group.portrait", 0);
const ORDER: xlayout::Class = xlayout::Class::new("COrderInstance", 0x3512bf0);
const ARMIES_VIEW: xlayout::Class = xlayout::Class::new("CArmiesView", 0x363d530);
const LEADER_WINDOW: xlayout::Class = xlayout::Class::new("CArmyLeaderWindow", 0);
const GRID: xlayout::Class = xlayout::Class::new("CStandardGridBox", 0x36cca80);
const GRID_ITEM: xlayout::Class = xlayout::Class::new("grid.item", 0);
const TEXT_BOX: xlayout::Class = xlayout::Class::new("gui.text", 0);
const CONTAINER: xlayout::Class = xlayout::Class::new("CContainerWindow", 0x36ba978);
/// The army panel's commander portrait ("Click to assign") while an army is
/// selected, and the commander list's visible row band; portraits are drawn
/// 90 px tall from the bottom of their texture rectangle.
const COMMANDER_SLOT: (f64, f64) = (33.0, 160.0);
const LEADER_ROWS: (i32, i32) = (140, 625);
const PORTRAIT_DRAWN: i32 = 90;
/// Command power for promoting a general to field marshal (00_defines.lua, 1.19.4.1).
const PROMOTE_LEADER_CP_COST: f64 = 40.0;
const GAME: (u32, u32) = (1920, 1080);

/// The layout for this build: the macOS values resolved through the per-build
/// tables (unchanged on macOS).
fn ui(r: &Reader<'_>) -> Result<Ui> {
    let abi = r.string_abi();
    let m = &MACOS;
    let f = |class: &xlayout::Class, mac: u64| fld(abi, class, mac);
    Ok(Ui {
        country_divisions: f(&COUNTRY, m.country_divisions)?,
        division_vtable: img(abi, m.division_vtable)?,
        division_group: f(&DIVISION, m.division_group)?,
        division_handle: f(&DIVISION, m.division_handle)?,
        group_name: f(&GROUP, m.group_name)?,
        selection_size: f(&IDLER, m.selection_size)?,
        manager: f(&IDLER, m.manager)?,
        leader_view_vtable: img(abi, m.leader_view_vtable)?,
        leader_view: f(&IFACE_MANAGER, m.leader_view)?,
        leader_portraits: f(&LEADER_VIEW, m.leader_portraits)?,
        portrait_window: f(&PORTRAIT, m.portrait_window)?,
        portrait_group: f(&PORTRAIT, m.portrait_group)?,
        group_handle: f(&GROUP, m.group_handle)?,
        group_orders: f(&GROUP, m.group_orders)?,
        division_location: f(&DIVISION, m.division_location)?,
        // Province id: the per-build province layout.
        province_id: crate::construction::layout(r).province_id,
        order_vtable: img(abi, m.order_vtable)?,
        order_children: [
            f(&ORDER, m.order_children[0])?,
            f(&ORDER, m.order_children[1])?,
        ],
        armies_view: f(&IFACE_MANAGER, m.armies_view)?,
        leader_window: f(&ARMIES_VIEW, m.leader_window)?,
        leader_window_root: f(&LEADER_WINDOW, m.leader_window_root)?,
        grid_items: f(&GRID, m.grid_items)?,
        item_container: f(&GRID_ITEM, m.item_container)?,
        text: f(&TEXT_BOX, m.text)?,
        group_leader: f(&GROUP, m.group_leader)?,
        leader_name: f(&LEADER, m.leader_name)?,
        container_vtable: img(abi, m.container_vtable)?,
        army_group: f(&GROUP, m.army_group)?,
        heading_for: f(&DIVISION, m.heading_for)?,
        container_base: f(&CONTAINER, m.container_base)?,
    })
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
/// division id -> province id where it stands (None while moving or unknown).
pub(super) fn division_provinces(
    r: &Reader<'_>,
    image: u64,
) -> Result<BTreeMap<String, Option<i32>>> {
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
        let province = if location != 0 && r.u64(location)? == image + l.province_vtable {
            Some(r.i32(location + u.province_id)?)
        } else {
            None
        };
        out.insert(id, province);
    }
    Ok(out)
}
/// Screen rectangle: top-left and bottom-right corners.
type ScreenBox = ((f64, f64), (f64, f64));

/// Screen box around the projected positions of `ids` (after the camera is placed), with a
/// margin, when it fits the map area between the panels.
fn projected_box(process: &dyn Process, ids: &[String]) -> Result<Option<ScreenBox>> {
    let exe = &process
        .identity()
        .ok_or_else(|| Error::new("map_data_unavailable", "Game identity unavailable"))?
        .exe;
    let dir = super::mapdata::game_dir(exe)?;
    let positions = super::mapdata::positions(&dir)?;
    read(process, |r, i| {
        let provinces = division_provinces(r, i)?;
        let mut pts = Vec::new();
        for id in ids {
            let Some(Some(p)) = provinces.get(id) else {
                return Ok(None);
            };
            let Some((x, z)) = positions.get(p).copied() else {
                return Ok(None);
            };
            let Some(s) = super::screenmap::to_screen(r, i, x, z)? else {
                return Ok(None);
            };
            pts.push(s);
        }
        if pts.is_empty() {
            return Ok(None);
        }
        let (mut x0, mut y0, mut x1, mut y1) = (f64::MAX, f64::MAX, f64::MIN, f64::MIN);
        for (x, y) in pts {
            x0 = x0.min(x);
            y0 = y0.min(y);
            x1 = x1.max(x);
            y1 = y1.max(y);
        }
        let (x0, y0, x1, y1) = (x0 - 45.0, y0 - 45.0, x1 + 45.0, y1 + 30.0);
        Ok((x0 > 460.0 && y0 > 110.0 && x1 < 1540.0 && y1 < 830.0).then_some(((x0, y0), (x1, y1))))
    })
}
/// Select own divisions by clicking their unit counters: a click on a province's counter selects
/// every own division standing there, Shift+click adds the next province. Only provinces whose
/// own divisions are all in `wanted` are clicked, so the selection can never take divisions of
/// another army. Returns how many divisions the game's selection holds when it holds only wanted
/// divisions (read back from the selection list), else None and the caller falls back to boxes.
/// Exact where boxes miss divisions spread over a large state (seen live 6 Nov 2026: boxes took
/// 5 and then 3 of a state's 10 unassigned divisions).
fn counter_select(
    process: &dyn Process,
    keys: &dyn GameInput,
    wanted: &[String],
) -> Result<Option<usize>> {
    counter_select_with(process, keys, wanted, &[])
}
/// As `counter_select`; provinces may also hold divisions of `also_ok` (the receiving army's
/// own divisions: assigning them to their own army changes nothing).
fn counter_select_with(
    process: &dyn Process,
    keys: &dyn GameInput,
    wanted: &[String],
    also_ok: &[String],
) -> Result<Option<usize>> {
    if wanted.is_empty() {
        return Ok(None);
    }
    let Some(identity) = process.identity() else {
        return Ok(None);
    };
    let Ok(dir) = super::mapdata::game_dir(&identity.exe) else {
        return Ok(None);
    };
    let Ok(positions) = super::mapdata::positions(&dir) else {
        return Ok(None);
    };
    let wanted_set: BTreeSet<&str> = wanted.iter().map(String::as_str).collect();
    // Large states put some provinces off screen around the state's centre: pan the camera with
    // the arrow keys toward the provinces still off screen (projected again after every tap), at
    // most 14 taps. Seen live 6 Nov 2026: Italian Somaliland's unassigned divisions projected
    // at (-700, 1466) with the state centred.
    let mut taps = 0;
    let mut zooms = 0;
    let points = loop {
        let (points, missing) = read(process, |r, i| {
            let provinces = division_provinces(r, i)?;
            let mut clean: BTreeMap<i32, bool> = BTreeMap::new();
            let mut has_wanted: BTreeSet<i32> = BTreeSet::new();
            for (id, province) in &provinces {
                if let Some(p) = province {
                    let is_wanted = wanted_set.contains(id.as_str());
                    if is_wanted {
                        has_wanted.insert(*p);
                    }
                    let ok = is_wanted || also_ok.iter().any(|a| a == id);
                    let entry = clean.entry(*p).or_insert(true);
                    *entry &= ok;
                }
            }
            clean.retain(|p, _| has_wanted.contains(p));
            let mut pts = Vec::new();
            let mut missing = Vec::new();
            for (p, ok) in clean {
                if !ok {
                    continue;
                }
                let Some(&(x, z)) = positions.get(&p) else {
                    continue;
                };
                match super::screenmap::to_screen(r, i, x, z)? {
                    Some(s) if (470.0..1700.0).contains(&s.0) && (120.0..900.0).contains(&s.1) => {
                        pts.push(s)
                    }
                    Some(s) => missing.push(s),
                    None => {}
                }
            }
            Ok((pts, missing))
        })?;
        let Some(&(mx, my)) = missing.first() else {
            break points;
        };
        if taps >= 14 {
            break points;
        }
        // Some provinces off screen while others show: one wheel step out (at most two) brings
        // a whole large state on screen in one pass, instead of picking it up over several
        // follow-up assigns (Eritrea took 82 s, live 6 Nov 2026).
        if !points.is_empty() && zooms < 2 {
            keys.scroll(GAME, (960.0, 540.0), -120)?;
            zooms += 1;
            std::thread::sleep(std::time::Duration::from_millis(400));
            continue;
        }
        use crate::platform::game_input::vk;
        if mx < 470.0 {
            keys.press_code(vk::LEFT)?;
        } else if mx >= 1700.0 {
            keys.press_code(vk::RIGHT)?;
        }
        if my < 120.0 {
            keys.press_code(vk::UP)?;
        } else if my >= 900.0 {
            keys.press_code(vk::DOWN)?;
        }
        taps += 1;
        std::thread::sleep(std::time::Duration::from_millis(250));
    };
    if points.is_empty() {
        return Ok(None);
    }
    for (k, (x, y)) in points.iter().enumerate() {
        if k == 0 {
            keys.click(GAME, *x, *y)?;
        } else {
            keys.shift_click(GAME, *x, *y)?;
        }
        std::thread::sleep(std::time::Duration::from_millis(150));
    }
    std::thread::sleep(std::time::Duration::from_millis(250));
    read(process, |r, i| {
        Ok(selected_divisions(r, i)?.and_then(|ids| {
            (ids.iter().any(|id| wanted_set.contains(id.as_str()))
                && ids
                    .iter()
                    .all(|id| wanted_set.contains(id.as_str()) || also_ok.iter().any(|a| a == id)))
            .then_some(ids.len())
        }))
    })
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
                    "states":standing,"main_state":main_state,
                    "motorization":(r.i32(group + fld(r.string_abi(), &GROUP, MOTORIZATION)?)? as u32) & 0xff}),
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
    /// Divisions of other named armies standing in the state.
    pub others_here: usize,
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
            others_here: 0,
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
        others_here: armies
            .iter()
            .filter(|a| a.name != army.name)
            .map(|a| a.here)
            .sum(),
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
pub(super) fn assign(process: &dyn Process, keys: &dyn GameInput, plan: &AssignPlan) -> Result<()> {
    let army = match &plan.mode {
        AssignMode::CreateNew => return create(process, keys, plan.state, false),
        AssignMode::Absorb { army } => army,
    };
    clear_notices(process, keys)?;
    default_map_mode(keys)?;
    find_state(process, keys, plan.state)?;
    let expected = plan.unassigned + plan.target_here;
    // Divisions stand anywhere in the state, so try the usual box first and
    // then wider ones; the card is only right-clicked when the selection holds
    // exactly the expected divisions (a new drag replaces the selection).
    let mut sizes = Vec::new();
    let mut exact = false;
    let mut partial = None;
    // Exact first: click the counters of provinces holding only unassigned divisions.
    if plan.unassigned > 0 {
        let view = armies(process)?;
        let own: Vec<String> = view["armies"]
            .as_array()
            .and_then(|a| a.iter().find(|x| x["army"] == json!(army)))
            .and_then(|a| a["divisions"].as_array())
            .map(|d| {
                d.iter()
                    .filter_map(|v| v.as_str().map(str::to_string))
                    .collect()
            })
            .unwrap_or_default();
        if let Some(n) =
            counter_select_with(process, keys, &unassigned_ids(&view, plan.state), &own)?
        {
            sizes.push(n);
            exact = true;
        }
    }
    // Large states (Eritrea, Hararghe) spread divisions beyond the boxes around the centre:
    // a second pass zooms the map out two wheel steps around the state's centre.
    for pass in 0..2 {
        if exact {
            break;
        }
        if pass == 1 {
            find_state(process, keys, plan.state)?;
            // Both wheel steps in one flick: one smooth zoom-out.
            keys.scroll_burst(GAME, (960.0, 540.0), -240, 2)?;
            std::thread::sleep(std::time::Duration::from_millis(500));
        }
        for (k, (from, to)) in ASSIGN_BOXES.into_iter().enumerate() {
            keys.drag(GAME, from, to)?;
            std::thread::sleep(std::time::Duration::from_millis(350));
            let size = read(process, selection_size)? as usize;
            sizes.push(size);
            if size == expected {
                exact = true;
                break;
            }
            // A selection read back as only unassigned divisions is always safe to assign: the
            // right-click moves nothing out of another army (seen live 6 Nov 2026: boxes held
            // exactly the 2 unassigned divisions beside the army's 4 and were refused).
            if size > 0 && read(process, selection_unassigned_only).unwrap_or(false) {
                partial = Some((k + pass * ASSIGN_BOXES.len(), size));
                exact = true;
                break;
            }
            // No other army stands in the state: a box holding the target's divisions and part of
            // the unassigned ones only folds those (settlement checks that only pool divisions
            // moved); take it at once, since the drags can pan the map and a later re-drag of the
            // same box picked other divisions (seen live 1 November 2026).
            if plan.others_here == 0 && size > plan.target_here && size < expected {
                partial = Some((k + pass * ASSIGN_BOXES.len(), size));
                exact = true;
                break;
            }
        }
    }
    if !exact {
        // Third pass: a box drawn around the projected positions of the pool and the target's
        // divisions here (map positions through the live camera).
        let view = armies(process)?;
        let mut ids = unassigned_ids(&view, plan.state);
        if let Some(a) = view["armies"]
            .as_array()
            .and_then(|a| a.iter().find(|x| x["army"] == json!(army)))
        {
            let here = read(process, division_states)?;
            for d in a["divisions"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(|d| d.as_str())
            {
                if here.get(d).copied().flatten() == Some(plan.state) {
                    ids.push(d.to_string());
                }
            }
        }
        find_state(process, keys, plan.state)?;
        std::thread::sleep(std::time::Duration::from_millis(400));
        if let Some((from, to)) = projected_box(process, &ids)? {
            keys.drag(GAME, from, to)?;
            std::thread::sleep(std::time::Duration::from_millis(400));
            let size = read(process, selection_size)? as usize;
            sizes.push(size);
            exact = size == expected
                || (plan.others_here == 0 && size > plan.target_here && size < expected);
        }
    }
    if !exact {
        return Err(Error::new(
            "selection_mismatch",
            format!(
                "The box selections held {sizes:?} divisions but {expected} were expected (unassigned {} plus {army}'s \
                 {} here), so they took divisions of another army or missed some. Nothing was assigned; the \
                 right-click on the card was not sent (other armies here: {}, partial box: {:?}).",
                plan.unassigned, plan.target_here, plan.others_here, partial
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
/// Ids of the selected own divisions, or None when the selection holds anything else.
/// The selection is a linked list in the idler: head at `selection_size - 0x10`, each node
/// {object, prev, next}; mapped live 5 November 2026 (two selected divisions; a province
/// selection reads CProvince objects instead).
fn selected_divisions(r: &Reader<'_>, image: u64) -> Result<Option<Vec<String>>> {
    let u = ui(r)?;
    let idler = r.u64(image + game::layout(r).idler_global)?;
    let count = selection_size(r, image)?;
    let mut node = r.u64(idler + u.selection_size - 0x10)?;
    let mut out = Vec::new();
    for _ in 0..count.min(512) {
        if node == 0 {
            return Ok(None);
        }
        let object = r.u64(node)?;
        if object == 0 || r.u64(object)? != image + u.division_vtable {
            return Ok(None);
        }
        let handle = r.read(object + u.division_handle, 8)?;
        out.push(format!(
            "{:08x}:{:08x}",
            u32::from_le_bytes(handle[..4].try_into().unwrap()),
            u32::from_le_bytes(handle[4..].try_into().unwrap())
        ));
        node = r.u64(node + 0x10)?;
    }
    Ok((out.len() as u64 == count).then_some(out))
}
/// Every selected object is an own division that belongs to no army.
fn selection_unassigned_only(r: &Reader<'_>, image: u64) -> Result<bool> {
    let Some(ids) = selected_divisions(r, image)? else {
        return Ok(false);
    };
    let groups = membership(r, image)?;
    Ok(!ids.is_empty() && ids.iter().all(|id| groups.get(id) == Some(&0)))
}
pub(super) fn selection_size(r: &Reader<'_>, image: u64) -> Result<u64> {
    let idler = r.u64(image + game::layout(r).idler_global)?;
    // A 32-bit count: in 1.19.4 the next four bytes are no longer zero (a selection of 26 read
    // as 0x100_0000_001a on 1 Nov 2026), so the high half is never part of the size.
    Ok(r.u64(idler + ui(r)?.selection_size)? & 0xffff_ffff)
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

/// Create an army from own divisions around state `state`, then fold the state's
/// remaining unassigned divisions into it: one box rarely holds all of a large state's
/// divisions (seen live 5 November 2026: Eritrea needed one create and four assigns).
pub(super) fn create(
    process: &dyn Process,
    keys: &dyn GameInput,
    state: i32,
    absorb: bool,
) -> Result<()> {
    let names_before: BTreeSet<String> = armies(process)
        .ok()
        .map(|view| army_names(&view))
        .unwrap_or_default();
    let t0 = std::time::Instant::now();
    create_once(process, keys, state, absorb)?;
    eprintln!("army_create timing: create_once {:?}", t0.elapsed());
    if absorb {
        return Ok(());
    }
    // Best effort: the army exists; a fold that cannot be done safely leaves the rest
    // unassigned for hoi4_army_assign, never fails the create.
    for _ in 0..6 {
        let Ok(view) = armies(process) else { break };
        let Some(name) = army_names(&view).difference(&names_before).next().cloned() else {
            break;
        };
        let plan = match plan_assign(&view, state, Some(&name), 24) {
            Ok(plan) => plan,
            Err(e) => {
                eprintln!("army_create fold: plan_assign {}: {}", e.code, e.message);
                break;
            }
        };
        if plan.unassigned == 0 || plan.take == 0 {
            break;
        }
        let t1 = std::time::Instant::now();
        let res = assign(process, keys, &plan);
        eprintln!(
            "army_create timing: fold assign {:?} -> {:?}",
            t1.elapsed(),
            res.as_ref().map(|_| ()).map_err(|e| e.code)
        );
        if let Err(e) = res {
            eprintln!("army_create fold: assign {}: {}", e.code, e.message);
            break;
        }
    }
    Ok(())
}
fn army_names(view: &Value) -> BTreeSet<String> {
    view["armies"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|a| a["army"].as_str().map(str::to_string))
        .collect()
}
fn create_once(
    process: &dyn Process,
    keys: &dyn GameInput,
    state: i32,
    absorb: bool,
) -> Result<()> {
    clear_notices(process, keys)?;
    // The selection box stays clear of the left-hand panels; Escape is not used
    // because on a free map it opens the game menu.
    default_map_mode(keys)?;
    find_state(process, keys, state)?;
    // Unassigned divisions can stand anywhere in the state: when the usual box
    // does not hold exactly them, try wider boxes (a new drag replaces the
    // selection) and keep the first exact one; otherwise fall back to the usual
    // box and the old outcome handling below.
    let view_before = armies(process).ok();
    let unassigned_here = view_before
        .as_ref()
        .map(|view| unassigned_ids(view, state).len())
        .filter(|n| *n > 0);
    // Divisions of named armies standing in this state: a box here could take them.
    let armies_here: usize = view_before
        .as_ref()
        .map(|view| {
            view["armies"]
                .as_array()
                .map(|a| {
                    a.iter()
                        .map(|x| x["states"][state.to_string()].as_u64().unwrap_or(0) as usize)
                        .sum()
                })
                .unwrap_or(0)
        })
        .unwrap_or(0);
    let by_counters = match (&view_before, absorb) {
        (Some(view), false) => counter_select(process, keys, &unassigned_ids(view, state))?,
        _ => None,
    };
    if by_counters.is_none() {
        keys.drag(GAME, ASSIGN_BOXES[0].0, ASSIGN_BOXES[0].1)?;
        wait(process, "select divisions", |r, i| {
            Ok(selection_size(r, i)? > 0)
        })?;
    }
    if let Some(expected) = unassigned_here.filter(|_| !absorb && by_counters.is_none()) {
        if read(process, selection_size)? as usize != expected {
            for (from, to) in ASSIGN_BOXES.iter().skip(1) {
                keys.drag(GAME, *from, *to)?;
                std::thread::sleep(std::time::Duration::from_millis(350));
                if read(process, selection_size)? as usize == expected {
                    break;
                }
            }
            let mut held = read(process, selection_size)? as usize;
            if held != expected && !(armies_here == 0 && held > 0 && held <= expected) {
                // A box around the projected positions of the state's unassigned divisions.
                if let Some(view) = view_before.as_ref() {
                    let ids = unassigned_ids(view, state);
                    find_state(process, keys, state)?;
                    std::thread::sleep(std::time::Duration::from_millis(400));
                    if let Some((from, to)) = projected_box(process, &ids)? {
                        keys.drag(GAME, from, to)?;
                        std::thread::sleep(std::time::Duration::from_millis(400));
                        held = read(process, selection_size)? as usize;
                    }
                }
            }
            // A part of a large state's unassigned divisions is fine when no army stands
            // there (seen live: a box held 5 of Eritrea's 18 with no army in the state).
            // With armies here too, a partial box is fine when the selection read back holds
            // only unassigned divisions (Ethiopia run 10: 11 divisions spread over a state
            // stayed outside every army because no box held all of them).
            let partial_ok = held > 0
                && held <= expected
                && (armies_here == 0 || read(process, selection_unassigned_only).unwrap_or(false));
            if held != expected && !partial_ok {
                // No box holds exactly the unassigned divisions here: a new army would also
                // take divisions of neighbouring armies (seen live 1 November 2026: a create
                // pulled 7 divisions out of two armies). Refuse instead of guessing.
                return Err(Error::new(
                    "selection_mismatch",
                    format!(
                        "No selection box held exactly the {expected} unassigned divisions of state \
                         {state} (last box: {held}); nothing was created. Deploy new divisions to a \
                         state without other armies, or fold them with hoi4_army_assign"
                    ),
                ));
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
    // armies of 24 and 19 divisions into one of 52). Either the dialog or the new army shows
    // within 500 ms; stop waiting at the first of them.
    let created = |r: &Reader<'_>, i: u64| -> Result<bool> {
        Ok(membership(r, i)?
            .values()
            .any(|g| *g != 0 && !groups_before.contains(g)))
    };
    let deadline = std::time::Instant::now() + std::time::Duration::from_millis(500);
    while std::time::Instant::now() < deadline
        && !read(process, |r, i| {
            Ok(super::ui::reassign_open(r, i)? || created(r, i)?)
        })?
    {
        std::thread::sleep(std::time::Duration::from_millis(30));
    }
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

/// Probe-only: hex words of an own army object (`NAME` or `NAME:LEN`).
pub(super) fn debug_bytes(process: &dyn Process, spec: &str) -> Result<Value> {
    let (army, len) = match spec.rsplit_once(':') {
        Some((a, n)) if n.parse::<usize>().is_ok() => (a, n.parse::<usize>().unwrap()),
        _ => (spec, 0x400),
    };
    read(process, |r, i| {
        let group = group_named(r, i, army)?;
        let bytes = r.read(group, len.min(0x2000))?;
        let words: Vec<String> = bytes
            .chunks(8)
            .map(|c| {
                let mut w = [0u8; 8];
                w[..c.len()].copy_from_slice(c);
                format!("{:016x}", u64::from_le_bytes(w))
            })
            .collect();
        Ok(json!({"army": army, "words": words}))
    })
}
/// Supply-hub motorization of an army (0 none, 1 partial, 2 full): hubs it draws from use
/// trucks to reach farther. Field mapped live on 5 November 2026 (army +0x38 cycled 0->1->2->0
/// with each click on the badge's motorization button).
const MOTORIZATION: u64 = 0x38;
/// The selected army's badge: CArmiesView (interface manager +0x1d8) keeps the badges of the
/// selection in an array at +0x488; a badge holds its root at +0x20 and the army's group
/// handle at +0x1f8 (mapped live 5 November 2026).
const ARMIES_VIEW_SLOT: u64 = 0x1d8;
const BADGES: u64 = 0x488;
const BADGE_ROOT: u64 = 0x20;
const BADGE_HANDLE: u64 = 0x1f8;
const BADGE_VTABLE: u64 = 0x363d598;
const BADGE: xlayout::Class = xlayout::Class::new("CArmyBadgeView", 0x363d598);

pub(super) fn motorization(r: &Reader<'_>, image: u64, army: &str) -> Result<u32> {
    let group = group_named(r, image, army)?;
    Ok((r.i32(group + fld(r.string_abi(), &GROUP, MOTORIZATION)?)? as u32) & 0xff)
}

/// The motorization button on the badge of `army`, which must be the only selected army.
fn motorization_button(r: &Reader<'_>, image: u64, army: &str) -> Result<Rect> {
    let abi = r.string_abi();
    let u = ui(r)?;
    let handle = r.u64(group_named(r, image, army)? + u.group_handle)?;
    let idler = r.u64(image + game::layout(r).idler_global)?;
    let manager = r.u64(idler + u.manager)?;
    let view = r.u64(manager + fld(abi, &IFACE_MANAGER, ARMIES_VIEW_SLOT)?)?;
    if r.u64(view)? != image + img(abi, ARMIES_VIEW.vtable)? {
        return Err(Error::new("invalid_gui", "Exact armies view required"));
    }
    let (_, data, count) = r.array_header(view + fld(abi, &ARMIES_VIEW, BADGES)?)?;
    for k in 0..count.min(8) as u64 {
        let badge = r.u64(data + k * 8)?;
        if r.u64(badge)? != image + img(abi, BADGE_VTABLE)?
            || r.u64(badge + fld(abi, &BADGE, BADGE_HANDLE)?)? != handle
        {
            continue;
        }
        let root = gui_tree::container(r, r.u64(badge + fld(abi, &BADGE, BADGE_ROOT)?)?, image)?
            .ok_or_else(|| Error::new("invalid_gui", "Army badge window required"))?;
        let button = super::panelkit::find(r, image, root, "army_motorization_priority_button", 6)?
            .ok_or_else(|| Error::new("gui_target_unavailable", "Motorization button not found"))?;
        if !gui_tree::shown(r, button)? {
            return Err(Error::new(
                "gui_target_unavailable",
                "Motorization button not shown",
            ));
        }
        return gui_tree::rect(r, image, button, GAME);
    }
    Err(Error::new(
        "gui_target_unavailable",
        format!("No badge for {army} is shown after selecting it"),
    ))
}

/// Set the supply motorization level of `army` with the badge's ordinary toggle button
/// (each click advances 0 -> 1 -> 2 -> 0); every click is verified from memory.
pub(super) fn set_motorization(
    process: &dyn Process,
    keys: &dyn GameInput,
    army: &str,
    level: u32,
) -> Result<()> {
    let now = read(process, |r, i| motorization(r, i, army))?;
    if now == level {
        return Ok(());
    }
    clear_notices(process, keys)?;
    select_army(process, keys, army)?;
    let clicks = (level + 3 - now) % 3;
    for step in 1..=clicks {
        let button = read(process, |r, i| motorization_button(r, i, army))?;
        let (x, y) = button.center();
        keys.click(GAME, x, y)?;
        let want = (now + step) % 3;
        wait(process, "motorization level", |r, i| {
            Ok(motorization(r, i, army)? == want)
        })?;
    }
    Ok(())
}

/// The army panel's division rows: CArmiesView keeps two CArmyDivisionListView objects
/// (+0x4a0, +0x4a8); each keeps its CArmyDivisionView rows in an array at +0xe0, and a row
/// names its division by the id words at +0x10/+0x14 (mapped live 5 November 2026).
const DIVISION_LISTS: [u64; 2] = [0x4a0, 0x4a8];
/// The division list view's GUI root; the armies view keeps two list views and only the
/// one whose root is visible is on screen (the other keeps stale rows).
const LIST_ROOT: u64 = 0x40;
fn list_shown(r: &Reader<'_>, list: u64) -> Result<bool> {
    let root = r.u64(list + fld(r.string_abi(), &DIVISION_LIST, LIST_ROOT)?)?;
    Ok(root != 0 && gui_tree::visible(r, root)?)
}
/// A shown widget of the on-screen division list view (e.g. its `btn_delete`).
fn list_widget(r: &Reader<'_>, image: u64, name: &str) -> Result<Option<Rect>> {
    let abi = r.string_abi();
    let u = ui(r)?;
    let idler = r.u64(image + game::layout(r).idler_global)?;
    let view = r.u64(r.u64(idler + u.manager)? + fld(abi, &IFACE_MANAGER, ARMIES_VIEW_SLOT)?)?;
    for list_at in DIVISION_LISTS {
        let list = r.u64(view + fld(abi, &ARMIES_VIEW, list_at)?)?;
        if list == 0 || !list_shown(r, list)? {
            continue;
        }
        let root = r.u64(list + fld(abi, &DIVISION_LIST, LIST_ROOT)?)?;
        let Some(root) = gui_tree::container(r, root, image)? else {
            continue;
        };
        if let Some(w) = super::panelkit::find(r, image, root, name, 4)? {
            if gui_tree::shown(r, w)? {
                return gui_tree::rect(r, image, w, GAME).map(Some);
            }
        }
    }
    Ok(None)
}
const LIST_ROWS: u64 = 0xe0;
const ROW_TYPE_WORD: u64 = 0x10;
const ROW_VTABLE: u64 = 0x363d658;
const DIVISION_LIST: xlayout::Class = xlayout::Class::new("CArmyDivisionListView", 0x363d5c0);
const DIVISION_ROW: xlayout::Class = xlayout::Class::new("CArmyDivisionView", 0x363d658);
const ROW_ROOTS: [u64; 6] = [0x20, 0x28, 0x30, 0x8, 0x10, 0x18];

/// Screen rectangle of the shown panel row of division `id` ("type:token" as hoi4_armies).
pub(super) fn division_row(r: &Reader<'_>, image: u64, id: &str) -> Result<Rect> {
    let abi = r.string_abi();
    let u = ui(r)?;
    let (t, k) = id
        .split_once(':')
        .and_then(|(a, b)| {
            Some((
                u32::from_str_radix(a, 16).ok()?,
                u32::from_str_radix(b, 16).ok()?,
            ))
        })
        .ok_or_else(|| Error::new("invalid_arguments", "division id must be TYPE:TOKEN (hex)"))?;
    let idler = r.u64(image + game::layout(r).idler_global)?;
    let view = r.u64(r.u64(idler + u.manager)? + fld(abi, &IFACE_MANAGER, ARMIES_VIEW_SLOT)?)?;
    let row_vt = image + img(abi, ROW_VTABLE)?;
    let word = fld(abi, &DIVISION_ROW, ROW_TYPE_WORD)?;
    for list_at in DIVISION_LISTS {
        let list = r.u64(view + fld(abi, &ARMIES_VIEW, list_at)?)?;
        if list == 0 || !list_shown(r, list)? {
            continue;
        }
        let (_, data, count) = r.array_header(list + fld(abi, &DIVISION_LIST, LIST_ROWS)?)?;
        for n in 0..count.min(64) as u64 {
            let row = r.u64(data + n * 8)?;
            if r.u64(row)? != row_vt
                || r.i32(row + word)? as u32 != t
                || r.i32(row + word + 4)? as u32 != k
            {
                continue;
            }
            for at in ROW_ROOTS {
                let Ok(p) = r.u64(row + fld(abi, &DIVISION_ROW, at)?) else {
                    continue;
                };
                if let Ok(Some(root)) = gui_tree::container(r, p, image) {
                    if gui_tree::visible(r, root)? {
                        // The row's background button is its click target (army_division_view).
                        if let Some(bg) = super::panelkit::find(r, image, root, "btn_bg", 1)? {
                            if gui_tree::shown(r, bg)? {
                                return gui_tree::rect(r, image, bg, GAME);
                            }
                        }
                    }
                }
            }
        }
    }
    Err(Error::new(
        "gui_target_unavailable",
        format!("Division {id} has no shown row in the army panel"),
    ))
}
/// Move the chosen divisions of `army` into `into` (an own army) or, without it, into one
/// new army, one division at a time with the game's controls: the army's card opens its
/// division list, a click on a division's row selects that division alone, then the new-army
/// card (first one) or a right click on the target's card (the rest) moves it. Each move is
/// verified from division membership. Returns the receiving army's name.
pub(super) fn detach(
    process: &dyn Process,
    keys: &dyn GameInput,
    army: &str,
    ids: &[String],
    into: Option<&str>,
) -> Result<String> {
    let source = read(process, |r, i| group_named(r, i, army))?;
    let members = read(process, membership)?;
    for id in ids {
        if members.get(id) != Some(&source) {
            return Err(Error::new(
                "invalid_target",
                format!("Division {id} is not in {army} (ids as hoi4_armies lists them)"),
            ));
        }
    }
    if ids.len() >= members.values().filter(|g| **g == source).count() {
        return Err(Error::new(
            "invalid_target",
            "That would empty the army; use hoi4_army_merge to move a whole army",
        ));
    }
    let mut dest: Option<String> = into.map(str::to_string);
    for id in ids {
        clear_notices(process, keys)?;
        select_army(process, keys, army)?;
        let row = settled(process, "division row", |r, i| division_row(r, i, id))?;
        let (x, y) = row.center();
        keys.click(GAME, x, y)?;
        wait(process, "division selected", |r, i| {
            Ok(selection_size(r, i)? & 0xffff_ffff == 1)
        })?;
        match &dest {
            Some(name) => {
                let target = read(process, |r, i| group_named(r, i, name))?;
                let card = read(process, |r, i| card_rect(r, i, name, "portrait_btn"))?;
                let (cx, cy) = card.center();
                keys.right_click(GAME, cx, cy)?;
                wait(process, "division moved", |r, i| {
                    Ok(membership(r, i)?.get(id) == Some(&target))
                })?;
            }
            None => {
                let groups_before: BTreeSet<u64> =
                    read(process, membership)?.values().copied().collect();
                let card = settled(process, "new army card", |r, i| new_card(r, i, false))?;
                let (cx, cy) = card.center();
                keys.click(GAME, cx, cy)?;
                wait(process, "new army", |r, i| {
                    Ok(membership(r, i)?
                        .get(id)
                        .is_some_and(|g| *g != 0 && !groups_before.contains(g)))
                })?;
                let name = read(process, |r, i| {
                    let g = membership(r, i)?[id];
                    army_name(r, g)
                })?;
                dest = Some(name);
            }
        }
    }
    dest.ok_or_else(|| Error::new("invalid_arguments", "No division given"))
}
/// Disband own divisions of one army with the division list's delete button (the game returns
/// their equipment and manpower to the pool). A whole army is disbanded in one press with the
/// army selected; otherwise each division is selected from its row first. Each division is
/// verified gone from the own divisions.
pub(super) fn disband(
    process: &dyn Process,
    keys: &dyn GameInput,
    army: &str,
    ids: &[String],
) -> Result<()> {
    let source = read(process, |r, i| group_named(r, i, army))?;
    let members = read(process, membership)?;
    for id in ids {
        if members.get(id) != Some(&source) {
            return Err(Error::new(
                "invalid_target",
                format!("Division {id} is not in {army} (ids as hoi4_armies lists them)"),
            ));
        }
    }
    let whole = ids.len() >= members.values().filter(|g| **g == source).count();
    let press_delete = |gone: &dyn Fn(&BTreeMap<String, u64>) -> bool| -> Result<()> {
        let button = settled(process, "division list delete button", |r, i| {
            list_widget(r, i, "btn_delete")?
                .ok_or_else(|| Error::new("gui_target_unavailable", "No shown delete button"))
        })?;
        let (x, y) = button.center();
        keys.click(GAME, x, y)?;
        let deadline = std::time::Instant::now() + std::time::Duration::from_millis(900);
        while std::time::Instant::now() < deadline {
            if gone(&read(process, membership)?) {
                return Ok(());
            }
            std::thread::sleep(std::time::Duration::from_millis(100));
        }
        // A confirmation dialog: accept it.
        keys.press(Key::Return)?;
        wait(process, "divisions disbanded", |r, i| {
            Ok(gone(&membership(r, i)?))
        })
    };
    if whole {
        clear_notices(process, keys)?;
        select_army(process, keys, army)?;
        return press_delete(&|m| ids.iter().all(|d| !m.contains_key(d)));
    }
    for id in ids {
        clear_notices(process, keys)?;
        select_army(process, keys, army)?;
        let row = settled(process, "division row", |r, i| division_row(r, i, id))?;
        let (x, y) = row.center();
        keys.click(GAME, x, y)?;
        wait(process, "division selected", |r, i| {
            Ok(selection_size(r, i)? & 0xffff_ffff == 1)
        })?;
        press_delete(&|m| !m.contains_key(id))?;
    }
    Ok(())
}
/// Probe-only: rows of every division of `army` (select the army first).
pub(super) fn debug_rows(process: &dyn Process, army: &str) -> Result<Value> {
    read(process, |r, i| {
        let group = group_named(r, i, army)?;
        let ids: Vec<String> = membership(r, i)?
            .into_iter()
            .filter(|(_, g)| *g == group)
            .map(|(d, _)| d)
            .collect();
        Ok(json!(ids
            .iter()
            .map(|id| json!({"id": id, "row": division_row(r, i, id).map(|x| [x.x, x.y, x.w, x.h]).map_err(|e| e.message)}))
            .collect::<Vec<_>>()))
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
pub(super) fn select_army(process: &dyn Process, keys: &dyn GameInput, army: &str) -> Result<Rect> {
    let card = select_army_quick(process, keys, army)?;
    // The army panel slides in after the selection; callers click its division rows next.
    std::thread::sleep(std::time::Duration::from_millis(300));
    Ok(card)
}
/// `select_army` without the wait for the army panel, for orders that go to the map next.
pub(super) fn select_army_quick(
    process: &dyn Process,
    keys: &dyn GameInput,
    army: &str,
) -> Result<Rect> {
    let size = read(process, |r, i| {
        let group = group_named(r, i, army)?;
        Ok(membership(r, i)?.values().filter(|g| **g == group).count() as u64)
    })?;
    // The card strip re-lays out when an army is added or removed (a detach creating a new army
    // moved Army 1's card and the next click missed it, live 6 Nov 2026): wait for a stable
    // rectangle, and click once more on a fresh read if the selection does not follow.
    let mut last = None;
    for _ in 0..2 {
        let card = settled(process, "army card", |r, i| {
            card_rect(r, i, army, "portrait_btn")
        })?;
        let (x, y) = card.center();
        keys.click(GAME, x, y)?;
        match wait(process, "select army", |r, i| {
            Ok(selection_size(r, i)? & 0xffff_ffff == size)
        }) {
            Ok(()) => return Ok(card),
            Err(e) => last = Some(e),
        }
        std::thread::sleep(std::time::Duration::from_millis(400));
    }
    Err(last.unwrap())
}
/// Move every division of army `source` into army `target` with the game's own controls:
/// a left click on the source card selects exactly its divisions, a right click on the
/// target card transfers the selection. Verified by membership: all of the source's
/// divisions now belong to the target's group.
pub(super) fn merge(
    process: &dyn Process,
    keys: &dyn GameInput,
    source: &str,
    target: &str,
) -> Result<()> {
    clear_notices(process, keys)?;
    let (moving, target_group) = read(process, |r, i| {
        let from = group_named(r, i, source)?;
        let to = group_named(r, i, target)?;
        let moving: Vec<String> = membership(r, i)?
            .into_iter()
            .filter(|(_, g)| *g == from)
            .map(|(d, _)| d)
            .collect();
        Ok((moving, to))
    })?;
    if moving.is_empty() {
        return Err(Error::new(
            "no_own_divisions",
            format!("{source} holds no division"),
        ));
    }
    if moving.len() > PANEL_ROWS {
        return Err(Error::new(
            "invalid_target",
            format!("{source} has more than {PANEL_ROWS} divisions (the rows visible without scrolling); split it first"),
        ));
    }
    // A selected army (its card) is not transferred by a right click; its divisions are:
    // select them in the army panel (first row, Shift on the last), as hoi4_army_split does.
    select_army(process, keys, source)?;
    keys.click(GAME, PANEL_ROW.0, PANEL_ROW.1)?;
    wait(process, "first division selected", |r, i| {
        Ok(selection_size(r, i)? == 1)
    })?;
    if moving.len() > 1 {
        keys.shift_click(
            GAME,
            PANEL_ROW.0,
            PANEL_ROW.1 + PANEL_PITCH * (moving.len() - 1) as f64,
        )?;
    }
    wait(process, "divisions selected", |r, i| {
        Ok(selection_size(r, i)? == moving.len() as u64)
    })?;
    let card = read(process, |r, i| card_rect(r, i, target, "portrait_btn"))?;
    let (x, y) = card.center();
    keys.right_click(GAME, x, y)?;
    // Emptying the source army asks "Reassign Armies" (it will be removed): accept it.
    super::ui::accept_command(process, keys)?;
    wait(process, "divisions merged", |r, i| {
        let now = membership(r, i)?;
        Ok(moving.iter().all(|d| now.get(d) == Some(&target_group)))
    })
    .map_err(|_| {
        Error::new(
            "input_readback_failed",
            "The right-click on the target army card did not move every division within 3 s; re-read hoi4_armies",
        )
    })
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
    highlighted_near(plain, drawing, skip, count, (960.0, 540.0), 7)
}
/// Like `highlighted`, nearest `anchor`, with `patch`x`patch` solid patches (3 finds the
/// thin dashed front lines drawn along borders).
fn highlighted_near(
    plain: &Capture,
    drawing: &Capture,
    skip: &dyn Fn(f64, f64) -> bool,
    count: usize,
    anchor: (f64, f64),
    patch: i32,
) -> Vec<(f64, f64)> {
    // Front-line drawing tints the whole map dark blue, so "changed" alone matches almost
    // everything (seen live 5 November 2026: no front found around Bale). The valid fronts
    // are bright dashes in the army's colour (e.g. (161,194,112)); the tinted map stays dark
    // and blue-dominant (about (30,55,90)).
    let changed = |x: f64, y: f64| match (plain.rgb(x, y), drawing.rgb(x, y)) {
        (Some(a), Some(b)) => {
            let d = |p: u8, q: u8| (i32::from(p) - i32::from(q)).abs();
            let bright = b.0.max(b.1) >= 130;
            let not_blue = b.2 < b.0.max(b.1);
            d(a.0, b.0) + d(a.1, b.1) + d(a.2, b.2) > 90 && bright && not_blue
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
                && (0..patch)
                    .all(|dy| (0..patch).all(|dx| changed(x + f64::from(dx), y + f64::from(dy))))
            {
                let d = (x - anchor.0).powi(2) + (y - anchor.1).powi(2);
                let h = f64::from(patch / 2);
                found.push((d, x + h, y + h));
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
            // An own state the enemy occupies is a valid front target (seen live: Ethiopia).
            && r.i32(state + l.state_controller)? == tag
        {
            out.push(r.i32(state + l.state_id)?);
        }
    }
    Ok(out)
}
/// Map position of the target state's province nearest the army's divisions.
/// Own-controlled state ids (controller is the player, whoever owns them).
fn controlled_states(r: &Reader<'_>, image: u64) -> Result<Vec<i32>> {
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
            && r.i32(state + l.state_controller)? == tag
        {
            out.push(r.i32(state + l.state_id)?);
        }
    }
    Ok(out)
}
/// Map point on the border between an own-controlled state and the target state, nearest the
/// army: midpoint of the closest province pair across that border (fronts can only be drawn on
/// the highlighted borders, 1.19.4.1).
fn front_border_point(process: &dyn Process, army: &str, state: i32) -> Result<Option<(f64, f64)>> {
    let exe = &process
        .identity()
        .ok_or_else(|| Error::new("map_data_unavailable", "Game identity unavailable"))?
        .exe;
    let dir = super::mapdata::game_dir(exe)?;
    let positions = super::mapdata::positions(&dir)?;
    let map = super::front_plan::static_map(&dir)?;
    let mine = read(process, controlled_states)?;
    let army_at = read(process, |r, i| army_provinces(r, i, army))?;
    let pts: Vec<(f64, f64)> = army_at
        .iter()
        .filter_map(|p| positions.get(p).copied())
        .collect();
    let centre = if pts.is_empty() {
        None
    } else {
        Some((
            pts.iter().map(|p| p.0).sum::<f64>() / pts.len() as f64,
            pts.iter().map(|p| p.1).sum::<f64>() / pts.len() as f64,
        ))
    };
    let target: Vec<(f64, f64)> = super::mapdata::state_provinces(&dir, state)?
        .iter()
        .filter_map(|p| positions.get(p).copied())
        .collect();
    let neighbours = map.adj.get(&state).cloned().unwrap_or_default();
    let mut best: Option<(f64, (f64, f64))> = None;
    for n in neighbours.iter().filter(|n| mine.contains(n)) {
        for p in super::mapdata::state_provinces(&dir, *n)?
            .iter()
            .filter_map(|p| positions.get(p).copied())
        {
            for q in &target {
                let gap = (p.0 - q.0).hypot(p.1 - q.1);
                let near_army = centre.map(|c| (q.0 - c.0).hypot(q.1 - c.1)).unwrap_or(0.0);
                let score = gap + 0.3 * near_army;
                if best.is_none_or(|b| score < b.0) {
                    best = Some((score, ((p.0 + q.0) / 2.0, (p.1 + q.1) / 2.0)));
                }
            }
        }
    }
    Ok(best.map(|b| b.1))
}
#[allow(dead_code)]
fn front_point_province(
    process: &dyn Process,
    army: &str,
    state: i32,
) -> Result<Option<(f64, f64)>> {
    let exe = &process
        .identity()
        .ok_or_else(|| Error::new("map_data_unavailable", "Game identity unavailable"))?
        .exe;
    let dir = super::mapdata::game_dir(exe)?;
    let positions = super::mapdata::positions(&dir)?;
    let mine = read(process, |r, i| army_provinces(r, i, army))?;
    let pts: Vec<(f64, f64)> = mine
        .iter()
        .filter_map(|p| positions.get(p).copied())
        .collect();
    if pts.is_empty() {
        return Ok(None);
    }
    let cx = pts.iter().map(|p| p.0).sum::<f64>() / pts.len() as f64;
    let cz = pts.iter().map(|p| p.1).sum::<f64>() / pts.len() as f64;
    Ok(super::mapdata::state_provinces(&dir, state)?
        .iter()
        .filter_map(|p| positions.get(p).copied())
        .min_by(|a, b| {
            let da = (a.0 - cx).powi(2) + (a.1 - cz).powi(2);
            let db = (b.0 - cx).powi(2) + (b.1 - cz).powi(2);
            da.total_cmp(&db)
        }))
}
pub(super) fn front(
    process: &dyn Process,
    keys: &dyn GameInput,
    army: &str,
    state: i32,
) -> Result<()> {
    let t0 = std::time::Instant::now();
    clear_notices(process, keys)?;
    super::ui::trace("front: notices", t0);
    let own = read(process, own_states)?;
    if own.contains(&state) {
        return Err(Error::new(
            "invalid_target",
            "This state is own and own-controlled; choose a state the enemy owns or occupies",
        ));
    }
    let before = read(process, |r, i| orders(r, i, army))?;
    let card = select_army_quick(process, keys, army)?;
    super::ui::trace("front: selected", t0);
    // Centre on the target state's province nearest the army: valid fronts are highlighted
    // only along the army's own line, which the state's centre can be far from (seen live:
    // an army in Bale facing Hararghe).
    let near = front_border_point(process, army, state)
        .ok()
        .flatten()
        .or_else(|| front_point_province(process, army, state).ok().flatten());
    // The Find view is only needed when the border point is not already in the usable area.
    let visible = super::screenmap::aim_height_ok(process)
        && near.is_some_and(|(x, z)| {
            read(process, |r, i| super::screenmap::to_screen(r, i, x, z))
                .ok()
                .flatten()
                .is_some_and(|(sx, sy)| {
                    (520.0..1500.0).contains(&sx) && (160.0..800.0).contains(&sy)
                })
        });
    super::ui::trace("front: border point", t0);
    if !visible {
        find_state(process, keys, state)?;
    }
    super::ui::trace("front: find", t0);
    if let Some((x, z)) = near {
        // Zoom out (at most four wheel steps, as before) until the point lies in the usable
        // area: the steps it needs, sized from its screen offset, go in one smooth flick.
        let mut left = 4;
        for _ in 0..2 {
            let events = match read(process, |r, i| super::screenmap::to_screen(r, i, x, z))? {
                Some(p) => super::glide::zoom_out_events(p, (520.0, 160.0, 1500.0, 800.0), left),
                None => left.min(2),
            };
            if events == 0 {
                break;
            }
            keys.scroll_burst(GAME, (960.0, 540.0), -240, events)?;
            std::thread::sleep(std::time::Duration::from_millis(80));
            super::screenmap::wait_camera_still(process, 600);
            left -= events;
            if left == 0 {
                break;
            }
        }
    }
    // The fronts this army can take are highlighted only in front-line
    // drawing, in a colour that varies by army and lighting; compare one
    // capture before Z with one after it, at the same camera position.
    super::ui::trace("front: zoom", t0);
    let plain = keys.capture(GAME)?;
    super::ui::trace("front: capture 1", t0);
    keys.press(Key::letter('z')?)?;
    // The card's tooltip may change under the resting cursor.
    let tooltip =
        |x: f64, y: f64| x > f64::from(card.x) - 60.0 && x < f64::from(card.x) + 460.0 && y > 680.0;
    // Capture until the highlighted fronts are drawn (about 100-300 ms after Z) instead of
    // a fixed 300 ms.
    let mut drawing = keys.capture(GAME)?;
    let mut candidates = highlighted(&plain, &drawing, &tooltip, 4);
    for _ in 0..6 {
        if !candidates.is_empty() {
            // One more frame: the highlight fades in, and a later frame holds all of it.
            std::thread::sleep(std::time::Duration::from_millis(60));
            drawing = keys.capture(GAME)?;
            candidates = highlighted(&plain, &drawing, &tooltip, 4);
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(50));
        drawing = keys.capture(GAME)?;
        candidates = highlighted(&plain, &drawing, &tooltip, 4);
    }
    super::ui::trace("front: capture 2", t0);
    // Highlighted border pixels nearest the projected border point first (thin dashed lines).
    if let Some((x, z)) = near {
        if let Some((sx, sy)) = read(process, |r, i| super::screenmap::to_screen(r, i, x, z))? {
            let mut nearby: Vec<(f64, f64)> =
                highlighted_near(&plain, &drawing, &tooltip, 3, (sx, sy), 3)
                    .into_iter()
                    .filter(|(x, y)| (x - sx).hypot(y - sy) < 120.0)
                    .collect();
            nearby.append(&mut candidates);
            candidates = nearby;
        }
    }
    super::ui::trace("front: candidates", t0);
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
pub(super) fn execute(
    process: &dyn Process,
    keys: &dyn GameInput,
    army: &str,
    run: bool,
) -> Result<()> {
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
    .map_err(|e| {
        if !run {
            return e;
        }
        Error::new(
            "plan_not_executable",
            "The Execute click did not start the plan. For a naval invasion the game waits until \
             the invasion has prepared (days shown on the plan button) and needs naval dominance in \
             every sea region on its path (fleets on patrol or strike force there: \
             hoi4_fleet_region + hoi4_fleet_mission); execute again once both hold. Other plans: \
             re-read hoi4_armies (battle_plan_orders) before retrying",
        )
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
    keys: &dyn GameInput,
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
    // The target state's centre where the camera already is; the Find view (it centres the
    // state on screen) when it is not in the usable area, or when a drag there drew nothing.
    let projected = super::screenmap::aim_height_ok(process)
        .then(|| state_screen_centre(process, state).ok().flatten())
        .flatten()
        .filter(|(x, y)| (520.0..1500.0).contains(x) && (160.0..800.0).contains(y));
    let draw = |(cx, cy): (f64, f64)| -> Result<()> {
        keys.press(Key::letter('x')?)?;
        std::thread::sleep(std::time::Duration::from_millis(300));
        let clamp = |x: f64, y: f64| (x.clamp(320.0, 1880.0), y.clamp(120.0, 880.0));
        let from = clamp(cx + dx * 260.0, cy + dy * 260.0);
        let to = clamp(cx - dx * 40.0, cy - dy * 40.0);
        keys.right_drag(GAME, from, to)?;
        wait(process, "offensive line placed", |r, i| {
            Ok(plan_nodes(r, i, army)? > before)
        })
    };
    if let Some(centre) = projected {
        if draw(centre).is_ok() {
            return Ok(());
        }
        let _ = keys.press(Key::Escape);
        if read(process, |r, i| plan_nodes(r, i, army))? > before {
            return Ok(());
        }
        let (x, y) = card.center();
        keys.click(GAME, x, y)?;
        wait(process, "select army", |r, i| Ok(selection_size(r, i)? > 0))?;
    }
    find_state(process, keys, state)?;
    draw((960.0, 540.0))
}
/// Divisions of `army` whose movement target (province id at +0x2ac of the
/// division, mapped live on 24 July 1941 by diffing a right-click move order)
/// lies in the provinces `wanted`, and the army's division count.
/// Each division's current heading (province id, 0 when standing) for army `army`.
pub(super) fn headings(r: &Reader<'_>, image: u64, army: &str) -> Result<Vec<i32>> {
    let u = ui(r)?;
    let group = group_named(r, image, army)?;
    let country = own_country(r, image)?;
    let (_, data, count) = r.array_header(country + u.country_divisions)?;
    let mut out = Vec::new();
    for index in 0..count.min(4096) as u64 {
        let division = r.u64(data + index * 8)?;
        if r.u64(division + u.division_group)? == group {
            out.push(r.i32(division + u.heading_for)?);
        }
    }
    Ok(out)
}
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
        if wanted.contains(&r.i32(division + u.heading_for)?) {
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
        if wanted.contains(&r.i32(division + u.heading_for)?) {
            heading += 1;
        }
    }
    Ok((heading, listed))
}
/// Box-select the own divisions standing in `state` (Find view, then the usual boxes) until the
/// selection holds exactly `expected` divisions; refuses with `selection_mismatch` otherwise.
/// Back to the default map mode (F1, mapmodes_interface.gui): in the air map mode left by the
/// air tools the division counters are hidden and a box selection picks nothing.
fn default_map_mode(keys: &dyn GameInput) -> Result<()> {
    keys.press_code(crate::platform::game_input::vk::F1)?;
    std::thread::sleep(std::time::Duration::from_millis(250));
    Ok(())
}
pub(super) fn box_select_exact(
    process: &dyn Process,
    keys: &dyn GameInput,
    state: i32,
    expected: usize,
) -> Result<()> {
    default_map_mode(keys)?;
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
pub(super) fn move_to(
    process: &dyn Process,
    keys: &dyn GameInput,
    army: &str,
    state: i32,
    province: Option<i32>,
) -> Result<()> {
    clear_notices(process, keys)?;
    let card = read(process, |r, i| card_rect(r, i, army, "portrait_btn"))?;
    let (x, y) = card.center();
    keys.click(GAME, x, y)?;
    wait(process, "select army", |r, i| Ok(selection_size(r, i)? > 0))?;
    let before = read(process, |r, i| headings(r, i, army))?;
    // The state already on screen (the usual case for a front-line army): right-click one of
    // its projected provinces where the camera is. The Find view (open, type, glide, about
    // 1.5 s) only when none shows, and the camera stays where the viewer follows the units.
    let visible = target_points(process, state, province).unwrap_or_default();
    if !visible.is_empty() && order_at(process, keys, army, &before, visible, false)? {
        return Ok(());
    }
    find_state(process, keys, state)?;
    // Map icons at the state's centre (air base, port, victory point) take a right click
    // for themselves (seen live 10 Dec 2026: Tripoli's air base); try points around the
    // centre until the divisions get a new heading.
    const POINTS: [(f64, f64); 5] = [
        (960.0, 540.0),
        (900.0, 600.0),
        (1020.0, 600.0),
        (900.0, 480.0),
        (1020.0, 480.0),
    ];
    // The state's own land provinces projected through the camera first: far out, the points
    // around the centre fell on Tripoli's air base icon or the sea and an overseas move never
    // started (live 6 Nov 2026). Points on the centre icon itself are skipped.
    let mut points: Vec<(f64, f64)> = target_points(process, state, province).unwrap_or_default();
    let projected = !points.is_empty();
    // Points around the centre stand in for the state; a named province is only its own point.
    if province.is_none() {
        points.extend(POINTS);
    }
    order_at(process, keys, army, &before, points, projected)?;
    Ok(())
}
/// Right-click `points` in turn until the army's headings change (true). Divisions waiting in
/// their ports show no new heading for a sea transfer: with `projected` points, one click on a
/// land province of the target is the order, repeating it only re-issues it.
fn order_at(
    process: &dyn Process,
    keys: &dyn GameInput,
    army: &str,
    before: &[i32],
    points: Vec<(f64, f64)>,
    projected: bool,
) -> Result<bool> {
    let in_ports = projected && {
        let ports = read(process, super::naval::naval_base_provinces).unwrap_or_default();
        let at = read(process, division_provinces).unwrap_or_default();
        let ids = read(process, |r, i| army_division_ids(r, i, army)).unwrap_or_default();
        !ids.is_empty()
            && ids.iter().all(|id| {
                at.get(id)
                    .copied()
                    .flatten()
                    .is_some_and(|p| ports.contains(&p))
            })
    };
    for (n, (x, y)) in points.into_iter().enumerate() {
        if in_ports && n == 1 {
            return Ok(true);
        }
        keys.right_click(GAME, x, y)?;
        let deadline = std::time::Instant::now() + std::time::Duration::from_millis(700);
        while std::time::Instant::now() < deadline {
            if read(process, |r, i| headings(r, i, army))? != before {
                return Ok(true);
            }
            std::thread::sleep(std::time::Duration::from_millis(40));
        }
    }
    Ok(false)
}
/// Screen point of the mean of `state`'s land province positions (map files, live camera).
pub(super) fn state_screen_centre(process: &dyn Process, state: i32) -> Result<Option<(f64, f64)>> {
    let Some(identity) = process.identity() else {
        return Ok(None);
    };
    let dir = super::mapdata::game_dir(&identity.exe)?;
    let positions = super::mapdata::positions(&dir)?;
    let provinces = super::mapdata::state_provinces(&dir, state)?;
    let pts: Vec<(f64, f64)> = provinces
        .iter()
        .filter_map(|p| positions.get(p).copied())
        .collect();
    if pts.is_empty() {
        return Ok(None);
    }
    let n = pts.len() as f64;
    let (x, z) = pts
        .iter()
        .fold((0.0, 0.0), |a, p| (a.0 + p.0 / n, a.1 + p.1 / n));
    read(process, |r, i| super::screenmap::to_screen(r, i, x, z))
}
/// Screen points of the land provinces of `state` (map files, live camera), nearest the screen
/// centre first, skipping the centre itself where the state's map icons sit; at most eight.
fn target_points(
    process: &dyn Process,
    state: i32,
    province: Option<i32>,
) -> Result<Vec<(f64, f64)>> {
    let Some(identity) = process.identity() else {
        return Ok(Vec::new());
    };
    let dir = super::mapdata::game_dir(&identity.exe)?;
    let positions = super::mapdata::positions(&dir)?;
    let provinces = match province {
        Some(p) => vec![p],
        None => super::mapdata::state_provinces(&dir, state)?,
    };
    let mut pts = read(process, |r, i| {
        let mut out = Vec::new();
        for p in &provinces {
            if let Some(&(x, z)) = positions.get(p) {
                if let Some(s) = super::screenmap::to_screen(r, i, x, z)? {
                    // The state's centre carries its map icons; one named province is clicked
                    // wherever it is.
                    let near_centre = province.is_none()
                        && (s.0 - 960.0).abs() < 28.0
                        && (s.1 - 540.0).abs() < 28.0;
                    if (470.0..1700.0).contains(&s.0)
                        && (120.0..880.0).contains(&s.1)
                        && !near_centre
                    {
                        out.push(s);
                    }
                }
            }
        }
        Ok(out)
    })?;
    pts.sort_by(|a, b| {
        let d = |p: &(f64, f64)| (p.0 - 960.0).powi(2) + (p.1 - 540.0).powi(2);
        d(a).total_cmp(&d(b))
    });
    pts.truncate(8);
    Ok(pts)
}
/// Ids of the divisions of own army `army`.
pub(super) fn army_division_ids(r: &Reader<'_>, image: u64, army: &str) -> Result<Vec<String>> {
    let group = group_named(r, image, army)?;
    Ok(membership(r, image)?
        .into_iter()
        .filter(|(_, g)| *g == group)
        .map(|(id, _)| id)
        .collect())
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
    gui_tree::container(r, root + u.container_base, image)?
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
            entry + u.container_base
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
            // 1.19.4.1: the leader button's sprite is 210 px tall, so the Promote button is
            // placed from the row's bottom (centre 402 px right, 22 px up; measured live on
            // 1 November 2026 against the drawn list).
            let bottom = rect.y + rect.h;
            let promote = promote_shown.then_some(Rect {
                x: rect.x + 353,
                y: bottom - 36,
                w: 98,
                h: 28,
            });
            out.push((name, rect, promote));
        }
    }
    Ok(out)
}

/// Whether the named general's Promote button is shown and enabled (None: no such row or
/// no Promote button). The game disables it without enough command power.
pub(super) fn promote_enabled(r: &Reader<'_>, image: u64, general: &str) -> Result<Option<bool>> {
    let u = ui(r)?;
    let root = leader_root(r, image)?;
    let wanted = general.to_lowercase();
    for child in gui_tree::children(r, root)? {
        let Some(inner) = gui_tree::container(r, child, image)? else {
            continue;
        };
        let Some(grid) = gui_tree::child(r, image, inner, "leaders_grid")? else {
            continue;
        };
        let (_, data, count) = r.array_header(grid + u.grid_items)?;
        for index in 0..count.min(256) as u64 {
            let item = r.u64(data + index * 8)?;
            let entry = r.u64(item + u.item_container)?;
            let base = if r.u64(entry)? == image + u.container_vtable {
                entry + u.container_base
            } else {
                entry
            };
            let Some(frame) = gui_tree::container(r, base, image)? else {
                continue;
            };
            let Some(name) = gui_tree::child(r, image, frame, "name")? else {
                continue;
            };
            if r.string_bounded(name + u.text, 128)?.to_lowercase() != wanted {
                continue;
            }
            return Ok(match gui_tree::child(r, image, frame, "promote_btn")? {
                Some(b) if gui_tree::shown(r, b)? => Some(!gui_tree::disabled(r, b)?),
                _ => None,
            });
        }
    }
    Ok(None)
}

/// Assign general `general` to `army` from the army panel's commander list.
/// Verified when the army's commander is that general.
pub(super) fn assign_commander(
    process: &dyn Process,
    keys: &dyn GameInput,
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
    keys: &dyn GameInput,
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
    let Some(rect) = settled(process, "commander list", find)? else {
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
    // The drawn portrait (the row's lower part) glides into the list band in one movement.
    let drawn = |rect: &Rect| {
        let bottom = f64::from(rect.y + rect.h);
        (bottom - f64::from(PORTRAIT_DRAWN), bottom)
    };
    let outcome = super::glide::glide(
        process,
        keys,
        &super::glide::Glide {
            at: (960.0, 560.0),
            pixels: 240,
            band: (f64::from(band.0), f64::from(band.1)),
            search: 0,
        },
        || {
            // The list can flicker shut while the game rebuilds it; give it a moment,
            // then open it again from the commander slot.
            for attempt in 0..8 {
                if read(process, |r, i| Ok(leader_list_open(r, i).unwrap_or(false)))? {
                    return Ok(());
                }
                std::thread::sleep(std::time::Duration::from_millis(250));
                if attempt == 4 {
                    keys.click(GAME, COMMANDER_SLOT.0, COMMANDER_SLOT.1)?;
                }
            }
            Err(Error::new(
                "input_readback_failed",
                format!("Commander list closed while bringing the row into view (row {rect:?}, list band {band:?})"),
            ))
        },
        |r, i| Ok(find(r, i)?.map(|x| drawn(&x))),
    )?;
    // The first or last row cannot scroll further: it is kept when its middle is shown.
    if outcome == super::glide::Glided::Unknown {
        return Err(Error::new(
            "gui_target_unavailable",
            "General left the list",
        ));
    }
    let rect = settled(process, "general row", |r, i| {
        find(r, i)?.ok_or_else(|| Error::new("gui_target_unavailable", "General left the list"))
    })?;
    let bottom = rect.y + rect.h;
    let middle = bottom - PORTRAIT_DRAWN / 2;
    if middle < band.0 || middle > band.1 {
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
        // The game keeps Promote enabled without enough command power (the tooltip turns
        // red); PROMOTE_LEADER_CP_COST is 40 in 00_defines.lua (1.19.4.1).
        let power = read(process, super::country_status::command_power)?;
        if read(process, |r, i| promote_enabled(r, i, general))?.is_some()
            && power.is_some_and(|cp| cp < PROMOTE_LEADER_CP_COST)
        {
            super::ui::dismiss(process, keys, |r, i| Ok(!leader_list_open(r, i)?));
            return Err(Error::new(
                "insufficient_command_power",
                format!(
                    "Promoting {general} to field marshal costs {PROMOTE_LEADER_CP_COST} command power and \
                     {} are held; nothing was changed",
                    power.unwrap_or_default()
                ),
            ));
        }
        if read(process, |r, i| promote_enabled(r, i, general))? == Some(false) {
            super::ui::dismiss(process, keys, |r, i| Ok(!leader_list_open(r, i)?));
            return Err(Error::new(
                "promotion_unavailable",
                format!(
                    "{general} must be promoted to field marshal to lead an army group, and the game \
                     disables Promote (not enough command power); nothing was changed"
                ),
            ));
        }
        if let Some(button) = read(process, promote_of)? {
            let (x, y) = button.center();
            keys.click(GAME, x, y)?;
            super::ui::accept_command(process, keys)?;
            // A closed list hides the button too: only an open list without it counts.
            wait(process, "general promoted", |r, i| {
                Ok(leader_list_open(r, i).unwrap_or(false) && promote_of(r, i)?.is_none())
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
        return Err(Error::new(
            "input_readback_failed",
            format!("Commander list closed before the click (group {is_group}, row {rect:?})"),
        ));
    }
    keys.click(
        GAME,
        f64::from(rect.x) + 34.0,
        f64::from(bottom - PORTRAIT_DRAWN / 2),
    )?;
    // Moving a commander who leads another army asks for confirmation; without one the
    // assignment shows at once.
    let assigned = |r: &Reader<'_>, i: u64| {
        Ok(commander(r, i, army)?.is_some_and(|c| c.to_lowercase() == wanted))
    };
    super::ui::accept_command_until(process, keys, assigned)?;
    wait(process, "commander assigned", assigned)
}

/// Group own armies `names` into a new army group: the first army's card,
/// the others' cards with Shift, then the new-army-group card. Verified when
/// all of them belong to one army group that did not exist before.
pub(super) fn create_group(
    process: &dyn Process,
    keys: &dyn GameInput,
    names: &[String],
) -> Result<()> {
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
pub(super) fn split(
    process: &dyn Process,
    keys: &dyn GameInput,
    army: &str,
    count: usize,
) -> Result<()> {
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
        assert_eq!(
            plan.mode,
            AssignMode::Absorb {
                army: "Army 2".into()
            }
        );
        assert_eq!(
            (plan.take, plan.overflow, plan.army_after),
            (6, 0, Some(16))
        );
        let plan = plan_assign(&assign_view(), 64, Some("Army 1"), ARMY_CAP).unwrap();
        assert_eq!(
            (plan.take, plan.overflow, plan.army_after),
            (5, 1, Some(24))
        );
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
        assert_eq!(
            plan_assign(&v, 99, None, 24).unwrap_err().code,
            "no_unassigned_here"
        );
        assert_eq!(
            plan_assign(&v, 64, Some("Army 9"), 24).unwrap_err().code,
            "no_such_army"
        );
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
