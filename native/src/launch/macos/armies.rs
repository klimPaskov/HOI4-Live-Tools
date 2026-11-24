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

pub(super) mod exercise;
pub(super) mod mode;
pub(super) mod orders;
mod select;

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
    /// The two order lists a fallback stroke fills (one order in each), where the build maps them.
    group_fallback_lists: [Option<u64>; 2],
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
    // A fallback stroke left +0x98 empty and added one COrderInstance to each of these lists
    // (live 16 Nov 2026, 1.19.4.1, two armies; the line was drawn on the map); an area defence
    // order is counted there too (fb2: the card showed the order, +0x98 stayed empty).
    group_fallback_lists: [Some(0xb0), Some(0xc8)],
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
pub(super) const COMMANDER_SLOT: (f64, f64) = (33.0, 160.0);
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
        // Unmapped in a build: those orders go uncounted, the other army tools keep working.
        group_fallback_lists: m
            .group_fallback_lists
            .map(|o| o.and_then(|o| f(&GROUP, o).ok())),
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
/// Planning bonus of a division (CArmy +0x468, i64 1/100000; 0.3 = 30%). The game's
/// `min_planning` trigger (0x1018286e0) divides it by the division's cap (computed by 0x100091800
/// from PLANNING_MAX, generals, technology and the HQ distance; not stored). Read live on
/// 9 November 2026: 0 on all 35 Italian divisions, none of whose armies held a front or plan.
const DIVISION_PLANNING: u64 = 0x468;
/// division id -> planning bonus (0..1).
fn planning(r: &Reader<'_>, image: u64) -> Result<BTreeMap<String, f64>> {
    let u = ui(r)?;
    let at = fld(r.string_abi(), &DIVISION, DIVISION_PLANNING)?;
    let country = own_country(r, image)?;
    let (_, data, count) = r.array_header(country + u.country_divisions)?;
    let mut out = BTreeMap::new();
    for index in 0..count.min(4096) as u64 {
        let division = r.u64(data + index * 8)?;
        if r.u64(division)? != image + u.division_vtable {
            continue;
        }
        let handle = r.read(division + u.division_handle, 8)?;
        let id = format!(
            "{:08x}:{:08x}",
            u32::from_le_bytes(handle[..4].try_into().unwrap()),
            u32::from_le_bytes(handle[4..].try_into().unwrap())
        );
        let raw = i64::from_le_bytes(r.read(division + at, 8)?.try_into().unwrap());
        if (0..=10_000_000).contains(&raw) {
            out.insert(id, raw as f64 / 100_000.0);
        }
    }
    Ok(out)
}
/// Planning of an army's divisions: {mean, min, max} (fractions, 0.3 = 30%).
fn planning_of(planning: &BTreeMap<String, f64>, ids: &[String]) -> Value {
    let values: Vec<f64> = ids
        .iter()
        .filter_map(|id| planning.get(id).copied())
        .collect();
    if values.is_empty() {
        return Value::Null;
    }
    let r3 = |v: f64| (v * 1000.0).round() / 1000.0;
    let mean = values.iter().sum::<f64>() / values.len() as f64;
    let min = values.iter().cloned().fold(f64::INFINITY, f64::min);
    let max = values.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
    json!({"mean": r3(mean), "min": r3(min), "max": r3(max)})
}
/// division id -> state id where it stands (None while moving or unknown).
pub(super) fn division_states(r: &Reader<'_>, image: u64) -> Result<BTreeMap<String, Option<i32>>> {
    let country = own_country(r, image)?;
    division_states_of(r, image, country)
}
/// division id -> state id of another country's divisions (the same CCountry division list the
/// own read walks; the caller passes a country pointer of the game state's country table).
pub(super) fn division_states_of(
    r: &Reader<'_>,
    image: u64,
    country: u64,
) -> Result<BTreeMap<String, Option<i32>>> {
    let u = ui(r)?;
    let l = crate::construction::layout(r);
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
/// division id -> the province it marches on (its movement target; 0 or its own province when
/// it has no march order).
pub(super) fn division_headings(r: &Reader<'_>, image: u64) -> Result<BTreeMap<String, i32>> {
    let u = ui(r)?;
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
        out.insert(id, r.i32(division + u.heading_for)?);
    }
    Ok(out)
}
/// How many of the listed divisions march on a province other than the one they stand in.
pub(super) fn marching(
    headings: &BTreeMap<String, i32>,
    at: &BTreeMap<String, Option<i32>>,
    ids: &[String],
) -> usize {
    ids.iter()
        .filter(|id| {
            let heading = headings.get(*id).copied().unwrap_or(0);
            heading > 0 && at.get(*id).copied().flatten() != Some(heading)
        })
        .count()
}
/// Halt `army`: its card selects its divisions and the unit panel's halt control (btn_hold,
/// shortcut H, "Halt all selected units") cancels every march and attack order they hold. The
/// divisions stay where they stand; a front line they are assigned to keeps them. Returns
/// whether the shown button was clicked (else its shortcut was pressed). The game keeps each
/// division's last heading after a halt, so the stop cannot be read from the headings.
pub(super) fn halt(process: &dyn Process, keys: &dyn GameInput, army: &str) -> Result<bool> {
    clear_notices(process, keys)?;
    // The halt control lives in the unit panel, which slides in after the selection: the
    // shortcut does nothing before it is shown (live 15 Nov 2026: the key right after the
    // card click left every division marching).
    select_army(process, keys, army)?;
    // The button itself when the division list shows it, its shortcut otherwise.
    let clicked = match settled(process, "halt button", |r, i| {
        list_widget(r, i, "btn_hold")?
            .ok_or_else(|| Error::new("gui_target_unavailable", "No shown halt button"))
    }) {
        Ok(button) => {
            let (x, y) = button.center();
            keys.click(GAME, x, y)?;
            true
        }
        Err(_) => {
            keys.press(Key::letter('h')?)?;
            false
        }
    };
    crate::phase::sleep(std::time::Duration::from_millis(300));
    clear_selection(process, keys)?;
    Ok(clicked)
}
/// `threat` per army in an `armies` view: the land provinces bordering the army's provinces
/// (outside them) and how many another country holds. `encirclement_risk` from 70% on.
pub(super) fn add_threats(
    process: &dyn Process,
    dir: &std::path::Path,
    view: &mut Value,
) -> Result<()> {
    let neighbours = super::wars::province_neighbours(dir)?;
    let at = read(process, division_provinces)?;
    let own = super::ui::retry_torn(|| crate::economy::snapshot(process))
        .ok()
        .and_then(|e| e["player"].as_str().map(str::to_string));
    let Some(armies) = view["armies"].as_array_mut() else {
        return Ok(());
    };
    for army in armies.iter_mut() {
        let provinces: BTreeSet<i32> = army["divisions"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|d| d.as_str())
            .filter_map(|d| at.get(d).copied().flatten())
            .collect();
        if provinces.is_empty() {
            continue;
        }
        let ring: Vec<i32> = provinces
            .iter()
            .flat_map(|p| neighbours.get(p).cloned().unwrap_or_default())
            .filter(|p| !provinces.contains(p))
            .collect::<BTreeSet<i32>>()
            .into_iter()
            .collect();
        let held = read(process, |r, i| {
            super::states::province_controllers(r, i, &ring)
        })?;
        let Some(me) = own.as_ref() else { continue };
        let hostile = held.iter().filter(|(_, c)| c != me).count();
        let share = if held.is_empty() {
            0.0
        } else {
            hostile as f64 / held.len() as f64
        };
        army["threat"] = json!({
            "neighbour_provinces": held.len(),
            "held_by_others": hostile,
            "share": (share * 100.0).round() / 100.0,
            "encirclement_risk": share >= 0.7,
        });
    }
    Ok(())
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
    let heights = super::mapdata::unit_heights(&dir)?;
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
            let y = heights.get(p).copied().unwrap_or(super::screenmap::GROUND);
            let Some(s) = super::screenmap::to_screen_at(r, i, x, y, z)? else {
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
    let heights = super::mapdata::unit_heights(&dir).unwrap_or_default();
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
                let y = heights.get(&p).copied().unwrap_or(super::screenmap::GROUND);
                match super::screenmap::to_screen_at(r, i, x, y, z)? {
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
            crate::phase::sleep(std::time::Duration::from_millis(400));
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
        crate::phase::sleep(std::time::Duration::from_millis(250));
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
        crate::phase::sleep(std::time::Duration::from_millis(150));
    }
    crate::phase::sleep(std::time::Duration::from_millis(250));
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
pub(super) fn army_name(r: &Reader<'_>, group: u64) -> Result<String> {
    r.string_bounded(group + ui(r)?.group_name, 128)
}
/// Own armies with their division ids, plus unassigned division ids.
pub(super) fn armies(process: &dyn Process) -> Result<Value> {
    let limits = super::command_limit::limits(process);
    read(process, |r, image| {
        // each commander's traits for the command limits (divisions an army commander and armies a
        // field marshal lead without penalty); unreadable leaders get the base limits
        let traits: BTreeMap<String, Vec<String>> = super::units::leaders(r, image)
            .map(|ls| ls.into_iter().map(|l| (l.name, l.traits)).collect())
            .unwrap_or_default();
        let limit_of = |leader: Option<&String>, marshal: bool| -> Option<i64> {
            let l = limits.as_ref()?;
            let t = leader
                .and_then(|n| traits.get(n))
                .cloned()
                .unwrap_or_default();
            Some(if marshal {
                l.armies(&t)
            } else {
                l.divisions(&t)
            })
        };
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
        let planning = planning(r, image).unwrap_or_default();
        let provinces = division_provinces(r, image).unwrap_or_default();
        // divisions per province of each army and army group, for the order read-back
        let mut per_group: BTreeMap<u64, BTreeMap<i32, usize>> = BTreeMap::new();
        for (group, ids) in &groups {
            let parent = r.u64(*group + ui(r)?.army_group)?;
            for id in ids {
                if let Some(Some(p)) = provinces.get(id) {
                    *per_group.entry(*group).or_default().entry(*p).or_default() += 1;
                    if parent != 0 {
                        *per_group.entry(parent).or_default().entry(*p).or_default() += 1;
                    }
                }
            }
        }
        let plan_of = |group: u64| -> Value {
            let empty = BTreeMap::new();
            orders::rows(r, image, group, per_group.get(&group).unwrap_or(&empty))
                .map(Value::from)
                .unwrap_or(Value::Null)
        };
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
                // front, offensive and other plan orders, then the fallback and area defence
                // orders the game keeps in two lists of their own (fb2, 16 Nov 2026: an area
                // order left +0x98 empty): an army holding only those still has orders
                let u = ui(r)?;
                let lines = r.array_header(group + u.group_orders)?.2;
                let mut defensive = 0;
                for list in u.group_fallback_lists.into_iter().flatten() {
                    defensive += r.array_header(group + list)?.2;
                }
                let orders = lines + defensive;
                let executing = executing(r, image, &name).ok();
                let exercise = exercise::state(r, image, &name).ok();
                let parent = r.u64(group + ui(r)?.army_group)?;
                let army_group = if parent == 0 {
                    None
                } else {
                    Some(army_name(r, parent)?)
                };
                Ok(
                    json!({"army":name,"army_group":army_group,"commander":commander,"battle_plan_orders":orders,
                    "defensive_orders":defensive,
                    "plan_executing":executing,"exercising":exercise.map(|e| e.exercising),
                    "exercise_stop_at_max":exercise.map(|e| e.stop_at_max),"planning":planning_of(&planning, &ids),
                    "division_count":ids.len(),"division_limit":limit_of(commander.as_ref(), false),
                    "divisions":ids,"plan":plan_of(group),
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
        // Which armies stand in the same province as an unassigned division: a click or a box on
        // such a stack takes that army's divisions too, so only that army can receive it.
        let mut groups_at: BTreeMap<i32, BTreeSet<u64>> = BTreeMap::new();
        for (id, group) in membership(r, image)? {
            if group != 0 {
                if let Some(Some(p)) = provinces.get(&id) {
                    groups_at.entry(*p).or_default().insert(group);
                }
            }
        }
        let mut names: BTreeMap<u64, String> = BTreeMap::new();
        let mut by_state_rows = Vec::with_capacity(by_state.len());
        for (state, ids) in by_state {
            let mut shared: BTreeMap<String, usize> = BTreeMap::new();
            for id in &ids {
                let Some(Some(p)) = provinces.get(*id) else {
                    continue;
                };
                for g in groups_at.get(p).into_iter().flatten() {
                    if !names.contains_key(g) {
                        names.insert(*g, army_name(r, *g)?);
                    }
                    *shared.entry(names[g].clone()).or_default() += 1;
                }
            }
            by_state_rows.push(
                json!({"state_id":state,"divisions":ids.len(),"division_ids":ids,
                "shared_with":shared}),
            );
        }
        let by_state = by_state_rows;
        let groups = army_groups(r, image)?
            .into_iter()
            .map(|g| {
                let leader = r.u64(g + ui(r)?.group_leader)?;
                let marshal = if leader == 0 {
                    None
                } else {
                    Some(r.string_bounded(leader + ui(r)?.leader_name, 128)?)
                };
                Ok(
                    json!({"army_group":army_name(r, g)?,"field_marshal":marshal,
                    "army_limit":limit_of(marshal.as_ref(), true),"plan":plan_of(g)}),
                )
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
        // An army sharing provinces with the unassigned divisions comes first: a stack holding
        // them and its divisions can only go to that army (another army's divisions in the
        // selection refuse the right-click), so any other target leaves them unassigned for good
        // (n1, 16 Nov 2026: six Eritrea divisions stayed unassigned 230 days).
        None => armies.iter().filter(|a| a.here > 0).max_by(|a, b| {
            let room = |x: &Standing| cap.saturating_sub(x.total);
            let shares = |x: &Standing| shared_with(view, state, &x.name);
            (room(a) > 0)
                .cmp(&(room(b) > 0))
                .then(shares(a).cmp(&shares(b)))
                .then(room(a).cmp(&room(b)))
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

/// How many of `state`'s unassigned divisions stand in a province where `army` stands too
/// (`hoi4_armies` unassigned_by_state shared_with).
pub(super) fn shared_with(view: &Value, state: i32, army: &str) -> usize {
    view["unassigned_by_state"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|row| row["state_id"].as_i64() == Some(state as i64))
        .filter_map(|row| row["shared_with"][army].as_u64())
        .sum::<u64>() as usize
}

/// The armies `state`'s unassigned divisions share provinces with, as "name (n)", most first.
pub(super) fn sharing_armies(view: &Value, state: i32) -> Vec<String> {
    let mut rows: Vec<(String, u64)> = view["unassigned_by_state"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|row| row["state_id"].as_i64() == Some(state as i64))
        .filter_map(|row| row["shared_with"].as_object())
        .flat_map(|m| m.iter().map(|(k, v)| (k.clone(), v.as_u64().unwrap_or(0))))
        .collect();
    rows.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
    rows.into_iter()
        .map(|(k, n)| format!("{k} ({n})"))
        .collect()
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
/// Selection boxes (game points) for gathering a state's divisions, usual size first (the older
/// path, without map data).
const ASSIGN_BOXES: [((f64, f64), (f64, f64)); 7] = [
    ((560.0, 260.0), (1360.0, 860.0)),
    ((560.0, 260.0), (1400.0, 980.0)),
    ((560.0, 220.0), (1480.0, 980.0)),
    ((480.0, 260.0), (1400.0, 980.0)),
    ((480.0, 220.0), (1560.0, 960.0)),
    ((420.0, 190.0), (1640.0, 980.0)),
    ((640.0, 300.0), (1300.0, 980.0)),
];
/// Carry out a plan. A new army uses the new-army path (`create`). Absorbing is the game's own
/// affordance, found live: with the unassigned divisions selected, a RIGHT-click on the standing
/// army's card assigns the selection ("Click to select / Right-click to assign the selected
/// Divisions"). The right-click moves the whole selection, so it is sent only when the selection
/// read back holds nothing but this state's unassigned divisions and the army's own; the one
/// selection is `select`'s (camera framed once, one box or one click round).
pub(super) fn assign(process: &dyn Process, keys: &dyn GameInput, plan: &AssignPlan) -> Result<()> {
    let army = match &plan.mode {
        AssignMode::CreateNew => return create(process, keys, plan.state, false),
        AssignMode::Absorb { army } => army,
    };
    let t0 = std::time::Instant::now();
    let view = armies(process)?;
    let pool = unassigned_ids(&view, plan.state);
    // The receiving army's own divisions may come along: assigning them to it changes nothing.
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
    clear_notices(process, keys)?;
    let picked = match select::select(process, keys, plan.state, &pool, &own) {
        Ok(picked) => picked,
        Err(e) if matches!(e.code, "map_data_unavailable" | "camera_unavailable") => {
            return assign_boxes(process, keys, plan, army);
        }
        Err(e) => return Err(sharing_hint(e, &view, plan.state, army)),
    };
    // The settlement allows only this state's unassigned divisions to join.
    if !picked.verdict.loose.is_empty() {
        return Err(Error::new(
            "selection_mismatch",
            format!(
                "The {} in state {} also took {} unassigned division(s) standing in another state; \
                 the army card was not right-clicked, nothing was assigned.",
                picked.how,
                plan.state,
                picked.verdict.loose.len()
            ),
        ));
    }
    let before = read(process, membership)?;
    right_click_card(process, keys, army, &before)?;
    eprintln!(
        "army_assign timing: state {} {} (find {}, zoom {}) {:?}: {} of {} into {army}",
        plan.state,
        picked.how,
        picked.found,
        picked.zoom_events,
        t0.elapsed(),
        picked.verdict.taken.len(),
        pool.len()
    );
    Ok(())
}
/// A refused selection names the armies the unassigned divisions share provinces with: only
/// they can receive those stacks (`army` names the one that was tried).
pub(super) fn sharing_hint(e: Error, view: &Value, state: i32, army: &str) -> Error {
    if !matches!(e.code, "selection_unavailable" | "selection_mismatch") {
        return e;
    }
    let others: Vec<String> = sharing_armies(view, state)
        .into_iter()
        .filter(|a| !a.starts_with(&format!("{army} (")))
        .collect();
    if others.is_empty() {
        return e;
    }
    Error::new(
        e.code,
        format!(
            "{} Unassigned divisions of state {state} share provinces with {}: call \
             hoi4_army_assign with that army (or without an army, which picks it).",
            e.message,
            others.join(", ")
        ),
    )
}
/// Right-click `army`'s card with the selection standing (the game's "Right-click to assign the
/// selected Divisions"), verified by a membership change.
fn right_click_card(
    process: &dyn Process,
    keys: &dyn GameInput,
    army: &str,
    before: &BTreeMap<String, u64>,
) -> Result<()> {
    let card = read(process, |r, i| card_rect(r, i, army, "portrait_btn"))?;
    clear_notices(process, keys)?;
    let (x, y) = card.center();
    keys.right_click(GAME, x, y)?;
    wait(process, "divisions assigned", |r, i| {
        Ok(&membership(r, i)? != before)
    })
    .map_err(|_| {
        Error::new(
            "input_readback_failed",
            "The right-click on the army card changed no division membership within 3 s; re-read hoi4_armies before trying again",
        )
    })
}
/// The older absorb path (no map data): Find view, counter clicks, then box sweeps.
fn assign_boxes(
    process: &dyn Process,
    keys: &dyn GameInput,
    plan: &AssignPlan,
    army: &str,
) -> Result<()> {
    clear_notices(process, keys)?;
    default_map_mode(process, keys)?;
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
            crate::phase::sleep(std::time::Duration::from_millis(500));
        }
        for (k, (from, to)) in ASSIGN_BOXES.into_iter().enumerate() {
            keys.drag(GAME, from, to)?;
            crate::phase::sleep(std::time::Duration::from_millis(350));
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
        crate::phase::sleep(std::time::Duration::from_millis(400));
        if let Some((from, to)) = projected_box(process, &ids)? {
            keys.drag(GAME, from, to)?;
            crate::phase::sleep(std::time::Duration::from_millis(400));
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
    right_click_card(process, keys, army, &before)
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
/// A selection left by the last order keeps its panels over the map: the unit list at the top
/// left and the battle-plan bar across the bottom of the map area, where the lower corner of a
/// box (or a click on a low counter) lands on the bar and selects nothing (14 Nov 2026: every
/// army made after the first one in a session failed this way). Escape clears a selection; it is
/// pressed only while something is selected, since with nothing selected it opens the menu.
pub(super) fn clear_selection(process: &dyn Process, keys: &dyn GameInput) -> Result<()> {
    if read(process, selection_size).unwrap_or(0) == 0 {
        return Ok(());
    }
    keys.press_code(crate::platform::game_input::vk::ESCAPE)?;
    let pressed = std::time::Instant::now();
    while pressed.elapsed() < std::time::Duration::from_millis(600) {
        crate::phase::sleep(std::time::Duration::from_millis(40));
        if read(process, selection_size).unwrap_or(0) == 0 {
            break;
        }
    }
    super::ui::trace("selection cleared", pressed);
    Ok(())
}

/// The type of every selected object: its vtable, relative to the image (own divisions carry
/// one known value; anything else names what a selection took instead: a fleet, an air wing).
pub(super) fn selection_types(r: &Reader<'_>, image: u64) -> Result<Vec<String>> {
    let u = ui(r)?;
    let idler = r.u64(image + game::layout(r).idler_global)?;
    let count = selection_size(r, image)?;
    let mut node = r.u64(idler + u.selection_size - 0x10)?;
    let mut out = Vec::new();
    for _ in 0..count.min(16) {
        if node == 0 {
            break;
        }
        let object = r.u64(node)?;
        if object == 0 {
            break;
        }
        let vtable = r.u64(object)?.wrapping_sub(image);
        out.push(if vtable == u.division_vtable {
            "division".to_string()
        } else {
            format!("{vtable:#x}")
        });
        node = r.u64(node + 0x10)?;
    }
    Ok(out)
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

/// Create one army from the unassigned own divisions standing in state `state`: one selection
/// takes all of them (`select`: the camera framed once, one box or one click round, read back
/// from memory, at most one Shift+click correction), then one click on the new-army card. The
/// selection never holds a division of another army: then nothing is clicked. Without map data
/// (or with `absorb`, which folds the armies standing there) the older box path runs.
pub(super) fn create(
    process: &dyn Process,
    keys: &dyn GameInput,
    state: i32,
    absorb: bool,
) -> Result<()> {
    let t0 = std::time::Instant::now();
    let pool = if absorb {
        Vec::new()
    } else {
        unassigned_ids(&armies(process)?, state)
    };
    if pool.is_empty() {
        let out = create_once(process, keys, state, absorb);
        eprintln!("army_create timing: box path {:?}", t0.elapsed());
        return out;
    }
    clear_notices(process, keys)?;
    let picked = match select::select(process, keys, state, &pool, &[]) {
        Ok(picked) => picked,
        Err(e) if matches!(e.code, "map_data_unavailable" | "camera_unavailable") => {
            let out = create_once(process, keys, state, absorb);
            eprintln!(
                "army_create timing: box path ({}) {:?}",
                e.code,
                t0.elapsed()
            );
            return out;
        }
        Err(e) => return Err(e),
    };
    let selected = t0.elapsed();
    let before = read(process, membership)?;
    click_new_army(process, keys, &before, false)?;
    let after = read(process, membership)?;
    let groups_before: BTreeSet<u64> = before.values().copied().collect();
    let joined = after
        .iter()
        .filter(|(_, g)| **g != 0 && !groups_before.contains(*g))
        .count();
    let left = pool.iter().filter(|d| after.get(*d) == Some(&0)).count();
    eprintln!(
        "army_create timing: state {state} {} (find {}, zoom {}) selected {selected:?}, created {:?}: \
         {joined} in the new army, {left} of {} left unassigned",
        picked.how,
        picked.found,
        picked.zoom_events,
        t0.elapsed(),
        pool.len()
    );
    Ok(())
}
/// Click the new-army card with the selection standing and wait for the new army. "Reassign
/// Armies" (the selection took every division of an existing army, which would be deleted) is
/// accepted only with `absorb`, else declined.
fn click_new_army(
    process: &dyn Process,
    keys: &dyn GameInput,
    before: &BTreeMap<String, u64>,
    absorb: bool,
) -> Result<()> {
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
        crate::phase::sleep(std::time::Duration::from_millis(30));
    }
    if read(process, super::ui::reassign_open)? {
        if absorb {
            // The caller asked for the armies standing here to be folded in.
            super::ui::accept_command(process, keys)?;
            return wait(process, "army created", created);
        }
        super::ui::decline_stale(process, keys)?;
        return Err(Error::new(
            "invalid_target",
            "Divisions of an existing army stand here, so a new army would delete it: deploy new divisions \
             to another state (hoi4_train deploy_state_id) or use hoi4_army_split on that army",
        ));
    }
    wait(process, "army created", created).map_err(|_| {
        Error::new(
            "input_readback_failed",
            "No new army appeared within 3 s of the new-army card click. The pre-input check found \
             own divisions standing in this state, so this is not proof of any single cause: \
             re-read hoi4_armies before trying again (the divisions may have moved or been assigned)",
        )
    })
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
    default_map_mode(process, keys)?;
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
                crate::phase::sleep(std::time::Duration::from_millis(350));
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
                    crate::phase::sleep(std::time::Duration::from_millis(400));
                    if let Some((from, to)) = projected_box(process, &ids)? {
                        keys.drag(GAME, from, to)?;
                        crate::phase::sleep(std::time::Duration::from_millis(400));
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
    click_new_army(process, keys, &before, absorb)
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
        Ok(json!({"army": army, "address": format!("{group:#x}"), "words": words}))
    })
}
/// Probe only (macprobe `dbg_card:ARMY`): the widget tree of the army's card window (names,
/// rectangles, shown flags, to depth 4), for mapping controls such as the air-wing attachment.
/// Probe only: raw words of the army object (`len` bytes) with its address.
pub(super) fn debug_army_words(
    process: &dyn Process,
    army: &str,
    len: usize,
) -> Result<(u64, Vec<u64>)> {
    read(process, |r, i| {
        let group = group_named(r, i, army)?;
        let bytes = r.read(group, len.min(0x4000))?;
        Ok((
            group,
            bytes
                .chunks(8)
                .map(|c| {
                    let mut w = [0u8; 8];
                    w[..c.len()].copy_from_slice(c);
                    u64::from_le_bytes(w)
                })
                .collect(),
        ))
    })
}
pub(super) fn debug_card(process: &dyn Process, army: &str) -> Result<Value> {
    read(process, |r, i| {
        let portrait = card_widget(r, i, army, "portrait_btn")?;
        let _ = portrait;
        let u = ui(r)?;
        let group = group_named(r, i, army)?;
        let handle = r.u64(group + u.group_handle)?;
        let idler = r.u64(i + game::layout(r).idler_global)?;
        let bar = r.u64(r.u64(idler + u.manager)? + u.leader_view)?;
        let (_, data, count) = r.array_header(bar + u.leader_portraits)?;
        fn walk(r: &Reader<'_>, i: u64, o: u64, depth: usize, out: &mut Vec<Value>) {
            for c in gui_tree::children(r, o)
                .unwrap_or_default()
                .into_iter()
                .take(40)
            {
                let name = gui_tree::name(r, c, i).ok().flatten();
                let rect = gui_tree::rect_unclipped(r, i, c, GAME)
                    .ok()
                    .map(|x| [x.x, x.y, x.w, x.h]);
                let shown = gui_tree::shown(r, c).ok();
                out.push(
                    json!({"d": depth, "name": name, "rect": rect, "shown": shown,
                    "at": format!("{c:#x}")}),
                );
                if depth < 4 {
                    if let Ok(Some(inner)) = gui_tree::container(r, c, i) {
                        walk(r, i, inner, depth + 1, out);
                    }
                }
            }
        }
        let mut out = Vec::new();
        for index in 0..count.min(64) as u64 {
            let p = r.u64(data + index * 8)?;
            if r.u64(p + u.portrait_group)? != handle {
                continue;
            }
            if let Some(window) = gui_tree::container(r, r.u64(p + u.portrait_window)?, i)? {
                walk(r, i, window, 0, &mut out);
            }
        }
        Ok(json!({"army": army, "widgets": out}))
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

/// Widget `name` on the badge of `army` (army_badge_view, unitview.gui), which must be the only
/// selected army.
fn badge_widget(r: &Reader<'_>, image: u64, army: &str, name: &str) -> Result<u64> {
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
        let widget = super::panelkit::find(r, image, root, name, 6)?.ok_or_else(|| {
            Error::new("gui_target_unavailable", format!("Badge {name} not found"))
        })?;
        if !gui_tree::shown(r, widget)? {
            return Err(Error::new(
                "gui_target_unavailable",
                format!("Badge {name} not shown"),
            ));
        }
        return Ok(widget);
    }
    Err(Error::new(
        "gui_target_unavailable",
        format!("No badge for {army} is shown after selecting it"),
    ))
}
/// The motorization button on the badge of `army`, which must be the only selected army.
fn motorization_button(r: &Reader<'_>, image: u64, army: &str) -> Result<Rect> {
    let button = badge_widget(r, image, army, "army_motorization_priority_button")?;
    gui_tree::rect(r, image, button, GAME)
}

/// The centre of the badge's name box. An edit box has no reviewed renderer, so it is placed from
/// a sibling the badge draws with a sprite: army_badge_view (unitview.gui) puts `army_bg` at
/// (0, 0), `army_item_bg` at (15, 0) and `group_name` at (66, 10), 135 x 23 px.
fn name_box_point(r: &Reader<'_>, image: u64, army: &str) -> Result<(f64, f64)> {
    let mut last = None;
    for (sibling, dx, dy) in [("army_bg", 0.0, 0.0), ("army_item_bg", 15.0, 0.0)] {
        let rect =
            badge_widget(r, image, army, sibling).and_then(|w| gui_tree::rect(r, image, w, GAME));
        match rect {
            Ok(rect) => {
                return Ok((
                    f64::from(rect.x) - dx + 66.0 + 135.0 / 2.0,
                    f64::from(rect.y) - dy + 10.0 + 23.0 / 2.0,
                ))
            }
            Err(e) => last = Some(e),
        }
    }
    Err(last.unwrap_or_else(|| Error::new("gui_target_unavailable", "Army name box not placed")))
}
/// Most characters of an army name: the game's name box keeps 20 (s6u, 24 Nov 2026: 23 typed, "Reserve Armee Ostpre"
/// kept, the read-back failed), so a longer name is refused before any input on both paths.
pub(super) const NAME_MAX: usize = 20;
/// Why `name` cannot become the name of `army` (checked before any input): not typable, too
/// long, or the name of another own army or army group.
pub(super) fn rename_refusal(view: &Value, army: &str, name: &str) -> Option<Error> {
    if !super::designer::typable(name) || name.trim() != name || name.chars().count() > NAME_MAX {
        return Some(Error::new(
            "name_not_typable",
            format!(
                "Army names may use letters (with ä ö ü ß), digits, space and - _ . , / ' ( ), at most {NAME_MAX} \
                 characters, no leading or trailing space. Nothing was typed."
            ),
        ));
    }
    let taken = view["armies"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|a| a["army"].as_str())
        .chain(
            view["army_groups"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(|g| g["army_group"].as_str()),
        )
        .any(|n| n == name && n != army);
    taken.then(|| {
        Error::new(
            "name_taken",
            format!("Another own army or army group is already named {name:?}. Nothing was typed."),
        )
    })
}
/// Rename `army` with the game's own control: its card selects it, the badge's name box
/// (`group_name`) takes a click, the old text is erased and the new name typed one verified key
/// at a time (design_edit::type_name: a letter is typed only once the box has proven the
/// keyboard focus), then Return. Verified when the army's orders group carries the new name.
pub(super) fn rename(
    process: &dyn Process,
    keys: &dyn GameInput,
    army: &str,
    name: &str,
) -> Result<()> {
    let group = read(process, |r, i| group_named(r, i, army))?;
    clear_notices(process, keys)?;
    select_army(process, keys, army)?;
    let (widget, point) = settled(process, "army name box", |r, i| {
        Ok((
            badge_widget(r, i, army, "group_name")?,
            name_box_point(r, i, army)?,
        ))
    })?;
    // The box's own buffer, spaces kept; it must hold the army's name before anything is typed,
    // so the first erased character proves the keyboard focus (type_name).
    let text = |r: &Reader<'_>, _: u64| super::navalui::edit_text_raw(r, widget);
    // An army the game named itself ("Army 3") keeps its edit buffer empty and draws its name
    // from the box's shown-text object until the box is used (as a fleet founded in game does:
    // navalui::edit_text). g17i (18 Nov 2026): 26 renames of "Army 1"/"Army 3" refused on the
    // empty buffer. The buffer fills a moment after the selection or after a click on the box;
    // nothing is typed until it holds the army's name, so the first erased key still proves
    // the keyboard focus.
    let mut shown = wait(process, "army name box filled", |r, i| {
        Ok(text(r, i)? == army)
    })
    .map(|_| army.to_string())
    .unwrap_or_else(|_| read(process, text).unwrap_or_default());
    if shown.is_empty()
        && read(process, |r, i| super::navalui::edit_text(r, i, widget)).unwrap_or_default() == army
    {
        keys.click(GAME, point.0, point.1)?;
        shown = wait(process, "army name box filled", |r, i| {
            Ok(text(r, i)? == army)
        })
        .map(|_| army.to_string())
        .unwrap_or_else(|_| read(process, text).unwrap_or_default());
    }
    if shown != army {
        return Err(Error::new(
            "gui_target_unavailable",
            format!("The badge name box reads {shown:?}, not {army:?}; nothing was typed"),
        ));
    }
    super::design_edit::type_name(process, keys, point, name, &text)?;
    keys.press(crate::platform::game_input::Key::Return)?;
    wait(process, "army renamed", |r, _| {
        Ok(army_name(r, group)? == name)
    })
    .map_err(|_| {
        Error::new(
            "input_readback_failed",
            format!(
                "The name box reads {:?} but the army kept another name within 3 s",
                read(process, text).unwrap_or_default()
            ),
        )
    })
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

/// The shown background rectangle of one panel row (None while the row is scrolled away).
fn row_shown_rect(r: &Reader<'_>, image: u64, row: u64) -> Result<Option<Rect>> {
    let abi = r.string_abi();
    for at in ROW_ROOTS {
        let Ok(p) = r.u64(row + fld(abi, &DIVISION_ROW, at)?) else {
            continue;
        };
        if let Ok(Some(root)) = gui_tree::container(r, p, image) {
            if gui_tree::visible(r, root)? {
                if let Some(bg) = super::panelkit::find(r, image, root, "btn_bg", 1)? {
                    if gui_tree::shown(r, bg)? {
                        return gui_tree::rect(r, image, bg, GAME).map(Some);
                    }
                }
            }
        }
    }
    Ok(None)
}

/// What the shown division list draws, for scrolling to division `id`.
struct RowView {
    /// index of the division's row object, None when the list keeps none for it
    target: Option<usize>,
    /// index of the first shown row
    first: usize,
    /// the first shown row's rectangle (the wheel's point)
    rect: Rect,
    /// the division id words of the first shown row (an unchanged top after a wheel notch is
    /// the list's end)
    top: (u32, u32),
}

/// Where the panel must scroll to show division `id`. None when the list shows no row at all.
/// The list keeps row objects only for the rows it draws (rp1, 19 Nov 2026: Armee Süd's 21st
/// division had none, its detach failed in 1.3 s without one wheel notch and moved at once
/// when the army had 20): the target is then None and the caller sweeps the list.
fn row_scroll_hint(r: &Reader<'_>, image: u64, id: &str) -> Result<Option<RowView>> {
    let abi = r.string_abi();
    let u = ui(r)?;
    let Some((t, k)) = id.split_once(':').and_then(|(a, b)| {
        Some((
            u32::from_str_radix(a, 16).ok()?,
            u32::from_str_radix(b, 16).ok()?,
        ))
    }) else {
        return Ok(None);
    };
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
        let mut target = None;
        let mut first_shown: Option<(usize, Rect, (u32, u32))> = None;
        for n in 0..count.min(256) {
            let row = r.u64(data + n as u64 * 8)?;
            if r.u64(row)? != row_vt {
                continue;
            }
            let words = (r.i32(row + word)? as u32, r.i32(row + word + 4)? as u32);
            if words == (t, k) {
                target = Some(n);
            }
            if first_shown.is_none() {
                if let Some(rect) = row_shown_rect(r, image, row)? {
                    first_shown = Some((n, rect, words));
                }
            }
        }
        if let Some((first, rect, top)) = first_shown {
            return Ok(Some(RowView {
                target,
                first,
                rect,
                top,
            }));
        }
    }
    Ok(None)
}

/// The shown row of division `id`, the army panel's list scrolled with the wheel until it is
/// drawn: a 24-division army shows only its first rows (g17i, 18 Nov 2026: 27 detaches refused
/// "no shown row in the army panel"). One wheel notch at a time towards the row, each read back.
fn division_row_scrolled(process: &dyn Process, keys: &dyn GameInput, id: &str) -> Result<Rect> {
    let mut last = None;
    // without a row object for the division: down to the list's end, then up to its top
    let mut down = true;
    let mut top = None;
    for _ in 0..64 {
        match settled(process, "division row", |r, i| division_row(r, i, id)) {
            Ok(rect) => return Ok(rect),
            Err(e) if e.code == "gui_target_unavailable" => last = Some(e),
            Err(e) => return Err(e),
        }
        let Some(view) = read(process, |r, i| row_scroll_hint(r, i, id))? else {
            break;
        };
        let delta = match view.target {
            Some(want) => {
                if want > view.first {
                    -120
                } else {
                    120
                }
            }
            None => {
                if top == Some(view.top) {
                    if !down {
                        break;
                    }
                    down = false;
                }
                top = Some(view.top);
                if down {
                    -120
                } else {
                    120
                }
            }
        };
        let (x, y) = view.rect.center();
        keys.scroll(GAME, (x, y), delta)?;
        crate::phase::sleep(std::time::Duration::from_millis(120));
    }
    Err(last.unwrap_or_else(|| {
        Error::new(
            "gui_target_unavailable",
            format!("Division {id} has no shown row in the army panel"),
        )
    }))
}
/// Which modified click toggles a division row in the selection in this game: 0 not yet known,
/// 1 Control, 2 Command, 3 neither (the rows are then ordered one division at a time).
static ROW_TOGGLE: std::sync::atomic::AtomicU8 = std::sync::atomic::AtomicU8::new(0);
/// Toggle clicks that found no working way in a row while the way was unknown: three of them
/// settle "neither" (one miss is a click the game lost, rs2 19 Nov 2026, not the game's rule).
static ROW_TOGGLE_MISSES: std::sync::atomic::AtomicU8 = std::sync::atomic::AtomicU8::new(0);
/// One toggle click on division `id`'s row, read back as the selection reaching `want`
/// divisions. false when the row is not shown or the selection did not follow.
fn toggle_row(process: &dyn Process, keys: &dyn GameInput, id: &str, want: u64) -> Result<bool> {
    use std::sync::atomic::Ordering;
    let Ok(row) = division_row_scrolled(process, keys, id) else {
        return Ok(false);
    };
    let (x, y) = row.center();
    let before = read(process, selection_size)?;
    let known = ROW_TOGGLE.load(Ordering::Relaxed);
    let tries: &[u8] = match known {
        1 => &[1],
        2 => &[2],
        _ => &[1, 2],
    };
    for &how in tries {
        let sent = if how == 1 {
            keys.ctrl_click(GAME, x, y)
        } else {
            keys.cmd_click(GAME, x, y)
        };
        if sent.is_err() {
            continue;
        }
        let bound = std::time::Duration::from_millis(700);
        if super::ui::wait_up_to(process, "division toggled", bound, |r, i| {
            Ok(selection_size(r, i)? == want)
        })
        .is_ok()
        {
            ROW_TOGGLE.store(how, Ordering::Relaxed);
            ROW_TOGGLE_MISSES.store(0, Ordering::Relaxed);
            return Ok(true);
        }
        // the click changed the selection some other way: no second way on top of it
        if read(process, selection_size)? != before {
            return Ok(false);
        }
    }
    if known == 0 && ROW_TOGGLE_MISSES.fetch_add(1, Ordering::Relaxed) + 1 >= 3 {
        ROW_TOGGLE.store(3, Ordering::Relaxed);
    }
    Ok(false)
}
/// Select divisions `ids` of `army` in its division list with the game's own controls (its
/// tooltips: "Click to select Division", "Ctrl+Click to toggle selection") and return the ones
/// selected now, read from the game's selection: all of `ids`, a part of them (the caller orders
/// that part at once and the rest one at a time), or none (the game took no multi-selection).
/// Few left out: the army's card selects all, a toggle takes each other division out. Else:
/// a click on the first row (that division alone; the list stays), a toggle on each other row.
/// One selection then one order replaces a re-selection, a row click and a camera trip per
/// division (rd6, 19 Nov 2026: 14 division-list moves at 13.3 s each, a third of the war).
fn select_rows(
    process: &dyn Process,
    keys: &dyn GameInput,
    army: &str,
    ids: &[String],
) -> Result<Vec<String>> {
    use std::sync::atomic::Ordering;
    if ids.len() < 2 || ROW_TOGGLE.load(Ordering::Relaxed) == 3 {
        return Ok(Vec::new());
    }
    let members = read(process, |r, i| army_division_ids(r, i, army))?;
    let out: Vec<String> = members
        .iter()
        .filter(|m| !ids.contains(m))
        .cloned()
        .collect();
    let chosen = |process: &dyn Process| -> Result<Option<Vec<String>>> {
        read(process, selected_divisions)
    };
    clear_notices(process, keys)?;
    if out.len() + 1 < ids.len() {
        select_army(process, keys, army)?;
        let mut size = read(process, selection_size)?;
        let mut whole = true;
        for id in &out {
            if !toggle_row(process, keys, id, size.saturating_sub(1))? {
                whole = false;
                break;
            }
            size -= 1;
        }
        if whole {
            if let Some(picked) = chosen(process)? {
                if picked.iter().all(|p| ids.contains(p)) {
                    return Ok(picked);
                }
            }
        }
    }
    select_army(process, keys, army)?;
    let Ok(first) = division_row_scrolled(process, keys, &ids[0]) else {
        return Ok(Vec::new());
    };
    let (x, y) = first.center();
    keys.click(GAME, x, y)?;
    let bound = std::time::Duration::from_millis(900);
    if super::ui::wait_up_to(process, "first division selected", bound, |r, i| {
        Ok(selection_size(r, i)? == 1)
    })
    .is_err()
    {
        return Ok(Vec::new());
    }
    for (size, id) in (1u64..).zip(&ids[1..]) {
        if !toggle_row(process, keys, id, size + 1)? {
            break;
        }
    }
    let picked = chosen(process)?.unwrap_or_default();
    Ok(if picked.iter().all(|p| ids.contains(p)) {
        picked
    } else {
        Vec::new()
    })
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
    // the chosen rows in one selection, then one card click moves them together (the rest,
    // if the game took only a part, one at a time below)
    let picked = select_rows(process, keys, army, ids)?;
    if picked.len() >= 2 {
        let ids: &[String] = &picked;
        let moved = match &dest {
            Some(name) => {
                let target = read(process, |r, i| group_named(r, i, name))?;
                let card = read(process, |r, i| card_rect(r, i, name, "portrait_btn"))?;
                let (cx, cy) = card.center();
                keys.right_click(GAME, cx, cy)?;
                wait(process, "divisions moved", |r, i| {
                    let m = membership(r, i)?;
                    Ok(ids.iter().all(|id| m.get(id) == Some(&target)))
                })
                .is_ok()
            }
            None => {
                let groups_before: BTreeSet<u64> =
                    read(process, membership)?.values().copied().collect();
                let card = settled(process, "new army card", |r, i| new_card(r, i, false))?;
                let (cx, cy) = card.center();
                keys.click(GAME, cx, cy)?;
                let fresh = wait(process, "new army", |r, i| {
                    let m = membership(r, i)?;
                    let g = m.get(&ids[0]).copied().unwrap_or(0);
                    Ok(g != 0
                        && !groups_before.contains(&g)
                        && ids.iter().all(|id| m.get(id) == Some(&g)))
                })
                .is_ok();
                if fresh {
                    dest = Some(read(process, |r, i| {
                        let g = membership(r, i)?[&ids[0]];
                        army_name(r, g)
                    })?);
                }
                fresh
            }
        };
        let _ = moved;
    }
    let members_now = read(process, membership)?;
    let ids: Vec<String> = ids
        .iter()
        .filter(|id| members_now.get(*id) == Some(&source))
        .cloned()
        .collect();
    if ids.is_empty() {
        return dest.ok_or_else(|| Error::new("invalid_arguments", "No division given"));
    }
    for id in &ids {
        clear_notices(process, keys)?;
        select_army(process, keys, army)?;
        let row = division_row_scrolled(process, keys, id)?;
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
            crate::phase::sleep(std::time::Duration::from_millis(100));
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
        let row = division_row_scrolled(process, keys, id)?;
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
/// The army (or army group) of this name exists among the own formations (a refusal before input).
/// The army object's address (for read-back diffs).
pub(super) fn army_address(process: &dyn Process, army: &str) -> Result<u64> {
    read(process, |r, i| group_named(r, i, army))
}
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
pub(super) fn card_widget(r: &Reader<'_>, image: u64, army: &str, name: &str) -> Result<u64> {
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
/// Cards in the bar that belong to no own army: army group cards (the new-army cards have no
/// handle). The parent's own card first when its handle is known, then the others.
fn group_cards(r: &Reader<'_>, image: u64, army: &str) -> Result<Vec<Rect>> {
    let u = ui(r)?;
    let group = group_named(r, image, army)?;
    let parent = r.u64(group + u.army_group)?;
    if parent == 0 {
        return Ok(Vec::new());
    }
    let parent_handle = r.u64(parent + u.group_handle).unwrap_or(0);
    let mut armies = BTreeSet::new();
    for g in membership(r, image)?.into_values().filter(|g| *g != 0) {
        armies.insert(r.u64(g + u.group_handle)?);
    }
    let idler = r.u64(image + game::layout(r).idler_global)?;
    let bar = r.u64(r.u64(idler + u.manager)? + u.leader_view)?;
    if r.u64(bar)? != image + u.leader_view_vtable {
        return Err(Error::new("invalid_gui", "Exact army card bar required"));
    }
    let (_, data, count) = r.array_header(bar + u.leader_portraits)?;
    let mut cards = Vec::new();
    for index in 0..count.min(64) as u64 {
        let portrait = r.u64(data + index * 8)?;
        let handle = r.u64(portrait + u.portrait_group)?;
        if handle == 0 || armies.contains(&handle) {
            continue;
        }
        let Some(window) = gui_tree::container(r, r.u64(portrait + u.portrait_window)?, image)?
        else {
            continue;
        };
        let Some(button) = gui_tree::child(r, image, window, "portrait_btn")? else {
            continue;
        };
        let Ok(rect) = gui_tree::rect(r, image, button, GAME) else {
            continue;
        };
        cards.push((handle, rect));
    }
    Ok(group_cards_in_order(cards, &armies, parent_handle))
}
/// Army group cards to try, the parent's first: every card that is not an own army's and not a
/// new-army card (handle 0), in bar order.
fn group_cards_in_order(cards: Vec<(u64, Rect)>, armies: &BTreeSet<u64>, parent: u64) -> Vec<Rect> {
    let (mut first, rest): (Vec<_>, Vec<_>) = cards
        .into_iter()
        .filter(|(h, _)| *h != 0 && !armies.contains(h))
        .partition(|(h, _)| *h == parent);
    first.extend(rest);
    first.into_iter().map(|(_, r)| r).collect()
}
/// The card of `army`, shown first when it is folded into its army group's card: the bar shows
/// an army group as one card and its armies' cards only while the group is selected (the AI's
/// 1939 Germany: fifteen armies in groups, every front and move order failed with "Army card not
/// shown", war bed 18 Nov 2026). One click on the group card, then the army's card is read again.
pub(super) fn shown_card(process: &dyn Process, keys: &dyn GameInput, army: &str) -> Result<Rect> {
    let first = read(process, |r, i| card_rect(r, i, army, "portrait_btn"));
    match first {
        Err(e) if e.message == "Army card not shown" => {
            let groups = read(process, |r, i| group_cards(r, i, army))?;
            for rect in groups.into_iter().take(4) {
                let (x, y) = rect.center();
                keys.click(GAME, x, y)?;
                let deadline = std::time::Instant::now() + std::time::Duration::from_millis(1200);
                while std::time::Instant::now() < deadline {
                    if let Ok(card) = read(process, |r, i| card_rect(r, i, army, "portrait_btn")) {
                        return Ok(card);
                    }
                    crate::phase::sleep(std::time::Duration::from_millis(100));
                }
            }
            Err(e)
        }
        other => other,
    }
}

/// Select exactly `army` with its card: the selection must hold all of its
/// divisions, not a selection left over from before.
pub(super) fn select_army(process: &dyn Process, keys: &dyn GameInput, army: &str) -> Result<Rect> {
    let card = select_army_quick(process, keys, army)?;
    // The army panel slides in after the selection; callers click its division rows next.
    crate::phase::sleep(std::time::Duration::from_millis(300));
    Ok(card)
}
/// `select_army` without the wait for the army panel, for orders that go to the map next.
pub(super) fn select_army_quick(
    process: &dyn Process,
    keys: &dyn GameInput,
    army: &str,
) -> Result<Rect> {
    // An army group's card selects the divisions of all its armies (a division belongs to an
    // army, and the army to the group).
    let size = read(process, |r, i| {
        let group = group_named(r, i, army)?;
        let parent = ui(r)?.army_group;
        let mut count = 0u64;
        for g in membership(r, i)?.values() {
            if *g == group || (*g != 0 && r.u64(*g + parent)? == group) {
                count += 1;
            }
        }
        Ok(count)
    })?;
    // The card strip re-lays out when an army is added or removed (a detach creating a new army
    // moved Army 1's card and the next click missed it, live 6 Nov 2026): wait for a stable
    // rectangle, and click once more on a fresh read if the selection does not follow.
    shown_card(process, keys, army)?;
    let mut last = None;
    for attempt in 0..2 {
        let card = settled(process, "army card", |r, i| {
            card_rect(r, i, army, "portrait_btn")
        })?;
        let (x, y) = card.center();
        keys.click(GAME, x, y)?;
        // the first click is tried again on a fresh card read: it need not wait the whole stage
        // (rd3: a click lost to the card strip's re-layout after a detach waited 3 s, 108 polls)
        let bound = if attempt == 0 {
            std::time::Duration::from_millis(900)
        } else {
            super::ui::STAGE_WAIT
        };
        match super::ui::wait_up_to(process, "select army", bound, |r, i| {
            Ok(selection_size(r, i)? & 0xffff_ffff == size)
        }) {
            Ok(()) => return Ok(card),
            Err(e) => last = Some(e),
        }
        crate::phase::sleep(std::time::Duration::from_millis(400));
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
/// Every unassigned own division into one army wherever the divisions stand, with the game's own
/// controls: Shift+click on the unassigned-divisions alert selects all of them (the alert's own
/// tooltip), then a right click on the target army's card assigns them, or a click on the
/// new-army card makes one army of them. Nothing walks states or moves the camera over the map,
/// so it costs the same for 3 divisions or 300. Returns (divisions moved, target army).
pub(super) fn gather(
    process: &dyn Process,
    keys: &dyn GameInput,
    target: Option<&str>,
    max: usize,
) -> Result<(usize, String)> {
    let view = armies(process)?;
    let pool: Vec<String> = view["unassigned"]
        .as_array()
        .map(|a| {
            a.iter()
                .filter_map(|d| d.as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default();
    if pool.is_empty() {
        return Err(Error::new(
            "no_unassigned",
            "No own division is outside an army",
        ));
    }
    if let Some(name) = target {
        let row = view["armies"]
            .as_array()
            .and_then(|a| a.iter().find(|x| x["army"].as_str() == Some(name)))
            .ok_or_else(|| Error::new("no_such_army", format!("No own army named {name:?}")))?;
        let held = row["division_count"].as_u64().unwrap_or(0) as usize;
        if held + pool.len() > max {
            return Err(Error::new(
                "army_full",
                format!(
                    "{name} holds {held}; {} unassigned divisions would make {} (over {max}): name another army or use new_army",
                    pool.len(),
                    held + pool.len()
                ),
            ));
        }
    }
    let at = read(process, |r, i| super::alerts::rect_of(r, i, "unassigned_divisions"))?
        .ok_or_else(|| {
            Error::new(
                "gui_target_unavailable",
                "The unassigned-divisions alert is not shown (the game raises it some days after a division is left outside an army)",
            )
        })?;
    clear_notices(process, keys)?;
    clear_selection(process, keys)?;
    let (x, y) = ((at[0] + at[2] / 2) as f64, (at[1] + at[3] / 2) as f64);
    keys.shift_click(GAME, x, y)?;
    wait(process, "unassigned divisions selected", |r, i| {
        Ok(selection_size(r, i)? > 0 && selection_unassigned_only(r, i)?)
    })
    .map_err(|_| {
        Error::new(
            "input_readback_failed",
            "Shift+click on the unassigned-divisions alert selected no unassigned division within 3 s",
        )
    })?;
    let selected = read(process, selected_divisions)?.unwrap_or_default();
    if let Some(name) = target {
        if selected.len()
            + read(process, |r, i| {
                let g = group_named(r, i, name)?;
                Ok(membership(r, i)?.values().filter(|v| **v == g).count())
            })?
            > max
        {
            clear_selection(process, keys)?;
            return Err(Error::new(
                "army_full",
                format!(
                    "{} divisions were selected; {name} has no room for all of them",
                    selected.len()
                ),
            ));
        }
    }
    let before = read(process, membership)?;
    let name = match target {
        Some(name) => {
            right_click_card(process, keys, name, &before)?;
            name.to_string()
        }
        None => {
            click_new_army(process, keys, &before, false)?;
            let after = read(process, membership)?;
            let groups_before: BTreeSet<u64> = before.values().copied().collect();
            let group = after
                .values()
                .copied()
                .find(|g| *g != 0 && !groups_before.contains(g))
                .ok_or_else(|| Error::new("input_readback_failed", "No new army appeared"))?;
            read(process, |r, _| army_name(r, group))
                .unwrap_or_else(|_| format!("group {group:#x}"))
        }
    };
    let after = read(process, membership)?;
    let moved = selected
        .iter()
        .filter(|d| after.get(*d).is_some_and(|g| *g != 0))
        .count();
    clear_selection(process, keys)?;
    Ok((moved, name))
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
/// A map point with its height: (x, height, z).
type MapPoint = (f64, f64, f64);

fn front_border_point(process: &dyn Process, army: &str, state: i32) -> Result<Option<MapPoint>> {
    let exe = &process
        .identity()
        .ok_or_else(|| Error::new("map_data_unavailable", "Game identity unavailable"))?
        .exe;
    let dir = super::mapdata::game_dir(exe)?;
    let positions = super::mapdata::positions(&dir)?;
    let heights = super::mapdata::unit_heights(&dir)?;
    let height = |p: &i32| heights.get(p).copied().unwrap_or(super::screenmap::GROUND);
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
    // the border lies between the two provinces' unit positions, at about their mean height (on
    // the Ethiopian highlands the flat-map point fell tens of pixels off the drawn border)
    let target: Vec<((f64, f64), f64)> = super::mapdata::state_provinces(&dir, state)?
        .iter()
        .filter_map(|p| Some((positions.get(p).copied()?, height(p))))
        .collect();
    let neighbours = map.adj.get(&state).cloned().unwrap_or_default();
    let mut best: Option<(f64, MapPoint)> = None;
    for n in neighbours.iter().filter(|n| mine.contains(n)) {
        for (p, hp) in super::mapdata::state_provinces(&dir, *n)?
            .iter()
            .filter_map(|p| Some((positions.get(p).copied()?, height(p))))
        {
            for (q, hq) in &target {
                let gap = (p.0 - q.0).hypot(p.1 - q.1);
                let near_army = centre.map(|c| (q.0 - c.0).hypot(q.1 - c.1)).unwrap_or(0.0);
                let score = gap + 0.3 * near_army;
                if best.is_none_or(|b| score < b.0) {
                    best = Some((
                        score,
                        ((p.0 + q.0) / 2.0, (hp + hq) / 2.0, (p.1 + q.1) / 2.0),
                    ));
                }
            }
        }
    }
    Ok(best.map(|b| b.1))
}
/// The two ends of the border between own state `own` and state `target` (unit positions of the
/// province pairs that face each other, at their mean height): a segment front is dragged from one
/// to the other. None when the two states do not touch.
///
/// Several own states (`from_states`): the border between them and the land of the target
/// state's controller (every state of that controller touching one of them), so two army groups
/// each draw their own piece of one long border.
fn segment_ends(process: &dyn Process, owns: &[i32], target: i32) -> Result<Option<Vec<MapPoint>>> {
    let exe = &process
        .identity()
        .ok_or_else(|| Error::new("map_data_unavailable", "Game identity unavailable"))?
        .exe;
    let dir = super::mapdata::game_dir(exe)?;
    let positions = super::mapdata::positions(&dir)?;
    let heights = super::mapdata::unit_heights(&dir)?;
    let at = |p: &i32| -> Option<((f64, f64), f64)> {
        Some((
            positions.get(p).copied()?,
            heights.get(p).copied().unwrap_or(super::screenmap::GROUND),
        ))
    };
    let mut mine = Vec::new();
    for own in owns {
        mine.extend(
            super::mapdata::state_provinces(&dir, *own)?
                .iter()
                .filter_map(at),
        );
    }
    let target_provinces = super::mapdata::state_provinces(&dir, target)?;
    let theirs_provinces = if owns.len() <= 1 {
        target_provinces
    } else {
        let map = super::front_plan::static_map(&dir)?;
        let near = facing_states(&map.adj, owns, target);
        let mut provinces = Vec::new();
        for s in &near {
            provinces.extend(super::mapdata::state_provinces(&dir, *s)?);
        }
        let controllers = read(process, |r, i| {
            super::states::province_controllers(r, i, &provinces)
        })?;
        let enemy = target_provinces
            .first()
            .and_then(|p| controllers.iter().find(|(q, _)| q == p))
            .map(|(_, tag)| tag.clone());
        controllers
            .into_iter()
            .filter(|(_, tag)| Some(tag) == enemy.as_ref())
            .map(|(p, _)| p)
            .collect()
    };
    let theirs: Vec<_> = theirs_provinces.iter().filter_map(at).collect();
    Ok(segment_path(&mine, &theirs))
}

/// The states touching any of `owns` (not one of them), with `target` first.
pub(super) fn facing_states(
    adj: &BTreeMap<i32, BTreeSet<i32>>,
    owns: &[i32],
    target: i32,
) -> Vec<i32> {
    let mut out = vec![target];
    for own in owns {
        for s in adj.get(own).into_iter().flatten() {
            if !owns.contains(s) && !out.contains(s) {
                out.push(*s);
            }
        }
    }
    out
}

/// `from_states` of hoi4_army_front: 2-12 distinct own state ids, or None when absent.
pub(super) fn segment_states(v: Option<&Value>) -> Result<Option<Vec<i32>>> {
    let Some(v) = v.filter(|v| !v.is_null()) else {
        return Ok(None);
    };
    let bad = || {
        Error::new(
            "invalid_arguments",
            "from_states must be 2-12 distinct state ids 1..4095",
        )
    };
    let items = v.as_array().ok_or_else(bad)?;
    let mut out = Vec::new();
    for item in items {
        let s = item
            .as_i64()
            .filter(|s| (1..4096).contains(s))
            .ok_or_else(bad)? as i32;
        if out.contains(&s) {
            return Err(bad());
        }
        out.push(s);
    }
    if !(2..=12).contains(&out.len()) {
        return Err(bad());
    }
    Ok(Some(out))
}

/// The border points of `segment_from_points` in order from one end to the other (by their
/// position along the line between the two ends): a drag through them follows a bending border
/// instead of cutting across own land.
pub(super) fn segment_path(
    mine: &[((f64, f64), f64)],
    theirs: &[((f64, f64), f64)],
) -> Option<Vec<MapPoint>> {
    let (a, b) = segment_from_points(mine, theirs)?;
    let pairs: Vec<(f64, MapPoint)> = mine
        .iter()
        .filter_map(|(p, hp)| {
            let (q, hq) = theirs.iter().min_by(|x, y| {
                let dx = (x.0 .0 - p.0).hypot(x.0 .1 - p.1);
                let dy = (y.0 .0 - p.0).hypot(y.0 .1 - p.1);
                dx.total_cmp(&dy)
            })?;
            let gap = (q.0 - p.0).hypot(q.1 - p.1);
            Some((gap, ((p.0 + q.0) / 2.0, (hp + hq) / 2.0, (p.1 + q.1) / 2.0)))
        })
        .collect();
    let closest = pairs.iter().map(|p| p.0).fold(f64::INFINITY, f64::min);
    let (ax, az) = (b.0 - a.0, b.2 - a.2);
    let len2 = (ax * ax + az * az).max(1e-9);
    let mut pts: Vec<(f64, MapPoint)> = pairs
        .into_iter()
        .filter(|p| p.0 <= closest * 2.0 + 1.0)
        .map(|(_, m)| (((m.0 - a.0) * ax + (m.2 - a.2) * az) / len2, m))
        .collect();
    pts.sort_by(|x, y| x.0.total_cmp(&y.0));
    pts.dedup_by(|x, y| (x.1 .0 - y.1 .0).hypot(x.1 .2 - y.1 .2) < 1.0);
    let mut path: Vec<MapPoint> = pts.into_iter().map(|p| p.1).collect();
    if path.len() < 2 {
        path = vec![a, b];
    }
    Some(path)
}

/// The border midpoints of the facing province pairs (each own province with its nearest target
/// province, kept within twice the closest pair's gap) and the two that lie farthest apart.
pub(super) fn segment_from_points(
    mine: &[((f64, f64), f64)],
    theirs: &[((f64, f64), f64)],
) -> Option<(MapPoint, MapPoint)> {
    let pairs: Vec<(f64, MapPoint)> = mine
        .iter()
        .filter_map(|(p, hp)| {
            let (q, hq) = theirs.iter().min_by(|a, b| {
                let da = (a.0 .0 - p.0).hypot(a.0 .1 - p.1);
                let db = (b.0 .0 - p.0).hypot(b.0 .1 - p.1);
                da.total_cmp(&db)
            })?;
            let gap = (q.0 - p.0).hypot(q.1 - p.1);
            Some((gap, ((p.0 + q.0) / 2.0, (hp + hq) / 2.0, (p.1 + q.1) / 2.0)))
        })
        .collect();
    let closest = pairs.iter().map(|p| p.0).fold(f64::INFINITY, f64::min);
    if !closest.is_finite() {
        return None;
    }
    let border: Vec<MapPoint> = pairs
        .iter()
        .filter(|p| p.0 <= closest * 2.0 + 1.0)
        .map(|p| p.1)
        .collect();
    let mut best: Option<(f64, MapPoint, MapPoint)> = None;
    for a in &border {
        for b in &border {
            let d = (a.0 - b.0).hypot(a.2 - b.2);
            if best.is_none_or(|x| d > x.0) {
                best = Some((d, *a, *b));
            }
        }
    }
    best.map(|(_, a, b)| (a, b))
}

#[allow(dead_code)]
fn front_point_province(process: &dyn Process, army: &str, state: i32) -> Result<Option<MapPoint>> {
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
    let heights = super::mapdata::unit_heights(&dir)?;
    Ok(super::mapdata::state_provinces(&dir, state)?
        .iter()
        .filter_map(|p| {
            let (x, z) = positions.get(p).copied()?;
            let y = heights.get(p).copied().unwrap_or(super::screenmap::GROUND);
            Some((x, y, z))
        })
        .min_by(|a, b| {
            let da = (a.0 - cx).powi(2) + (a.2 - cz).powi(2);
            let db = (b.0 - cx).powi(2) + (b.2 - cz).powi(2);
            da.total_cmp(&db)
        }))
}
/// Arrow-key taps the front's border point may take to reach the middle of the map area.
const FRONT_PAN_TAPS: usize = 8;

pub(super) fn front(
    process: &dyn Process,
    keys: &dyn GameInput,
    army: &str,
    state: i32,
    segment: Option<&[i32]>,
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
    // The highlighted fronts are drawn for the army whose panel has opened: keep the panel wait
    // (with the quick selection no front was ever highlighted, live 8 Nov 2026).
    let card = select_army(process, keys, army)?;
    super::ui::trace("front: selected", t0);
    // Centre on the target state's province nearest the army: valid fronts are highlighted
    // only along the army's own line, which the state's centre can be far from (seen live:
    // an army in Bale facing Hararghe).
    // A segment front (`segment` = the own state): the border between that state and the
    // target only, dragged from end to end; the camera centres its middle.
    let ends = match segment {
        Some(own) => Some(segment_ends(process, own, state)?.ok_or_else(|| {
            Error::new(
                "invalid_target",
                format!("Own state(s) {own:?} do not border state {state}: no segment to draw"),
            )
        })?),
        None => None,
    };
    let near = match ends.as_deref() {
        Some([a, .., b]) => Some(((a.0 + b.0) / 2.0, (a.1 + b.1) / 2.0, (a.2 + b.2) / 2.0)),
        Some(_) => None,
        None => front_border_point(process, army, state)
            .ok()
            .flatten()
            .or_else(|| front_point_province(process, army, state).ok().flatten()),
    };
    // The Find view is only needed when the border point is not already in the usable area.
    let visible = super::screenmap::front_height_ok(process)
        && near.is_some_and(|(x, y, z)| {
            read(process, |r, i| {
                super::screenmap::to_screen_at(r, i, x, y, z)
            })
            .ok()
            .flatten()
            .is_some_and(|(sx, sy)| (520.0..1500.0).contains(&sx) && (160.0..800.0).contains(&sy))
        });
    super::ui::trace("front: border point", t0);
    if !visible {
        find_state(process, keys, state)?;
    }
    super::ui::trace("front: find", t0);
    if let Some((x, y, z)) = near {
        // Zoom out (at most four wheel steps, as before) until the point lies in the usable
        // area: the steps it needs, sized from its screen offset, go in one smooth flick.
        let mut left = 4;
        for _ in 0..2 {
            let events = match read(process, |r, i| {
                super::screenmap::to_screen_at(r, i, x, y, z)
            })? {
                Some(p) => super::glide::zoom_out_events(p, (520.0, 160.0, 1500.0, 800.0), left),
                None => left.min(2),
            };
            if events == 0 {
                break;
            }
            keys.scroll_burst(GAME, (960.0, 540.0), -240, events)?;
            crate::phase::sleep(std::time::Duration::from_millis(80));
            super::screenmap::wait_camera_still(process, 600);
            left -= events;
            if left == 0 {
                break;
            }
        }
    }
    // The Find view centres the target state, and the border the army stands on can run along
    // the edge of the usable area, where the battle-plan bar, the army cards and the tooltip
    // cover its highlight while another border's highlight (a neutral neighbour's) is in full
    // view (live 14 Nov 2026: fronts toward Hararghe from Somaliland failed two times in
    // three). The arrow keys bring the border point into the middle of the map area.
    if let Some((x, y, z)) = near {
        use crate::platform::game_input::vk;
        let (x0, x1, y0, y1) = (640.0, 1280.0, 260.0, 640.0);
        let (mx, my) = ((x0 + x1) / 2.0, (y0 + y1) / 2.0);
        for _ in 0..FRONT_PAN_TAPS {
            let Some((sx, sy)) = read(process, |r, i| {
                super::screenmap::to_screen_at(r, i, x, y, z)
            })?
            else {
                break;
            };
            if (x0..x1).contains(&sx) && (y0..y1).contains(&sy) {
                break;
            }
            let (dx, dy) = ((sx - mx) / (x1 - x0), (sy - my) / (y1 - y0));
            let key = if dx.abs() >= dy.abs() {
                if dx < 0.0 {
                    vk::LEFT
                } else {
                    vk::RIGHT
                }
            } else if dy < 0.0 {
                vk::UP
            } else {
                vk::DOWN
            };
            keys.press_code(key)?;
            crate::phase::sleep(std::time::Duration::from_millis(160));
            super::screenmap::wait_camera_still(process, 600);
        }
        super::ui::trace_text(&format!(
            "front: border point on screen at {:?}",
            read(process, |r, i| super::screenmap::to_screen_at(
                r, i, x, y, z
            ))?
        ));
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
            crate::phase::sleep(std::time::Duration::from_millis(60));
            drawing = keys.capture(GAME)?;
            candidates = highlighted(&plain, &drawing, &tooltip, 4);
            break;
        }
        crate::phase::sleep(std::time::Duration::from_millis(50));
        drawing = keys.capture(GAME)?;
        candidates = highlighted(&plain, &drawing, &tooltip, 4);
    }
    // Front drawing already on before the first capture (left by an earlier attempt): the
    // highlight is in the "plain" capture and Z switched the mode off (live 8 Nov 2026: the
    // dashed front was on screen and nothing was found). Switch it back on and use those points.
    if candidates.is_empty() {
        let reversed = highlighted(&drawing, &plain, &tooltip, 4);
        if !reversed.is_empty() {
            keys.press(Key::letter('z')?)?;
            crate::phase::sleep(std::time::Duration::from_millis(250));
            drawing = keys.capture(GAME)?;
            candidates = highlighted(&plain, &drawing, &tooltip, 4);
            if candidates.is_empty() {
                candidates = reversed;
            }
        }
    }
    super::ui::trace("front: capture 2", t0);
    if let Some(dir) = std::env::var_os("HOI4_DUMP_FRONT") {
        // Debug: both captures as PPM (game pixels, every other pixel).
        for (name, cap) in [("plain", &plain), ("drawing", &drawing)] {
            let mut out = b"P6 960 540 255\n".to_vec();
            for y in 0..540 {
                for x in 0..960 {
                    let (r, g, b) = cap.rgb(x as f64 * 2.0, y as f64 * 2.0).unwrap_or((0, 0, 0));
                    out.extend_from_slice(&[r, g, b]);
                }
            }
            let _ = std::fs::write(
                std::path::Path::new(&dir).join(format!("front-{name}.ppm")),
                out,
            );
        }
    }
    // Highlighted border pixels nearest the projected border point first (thin dashed lines).
    if let Some((x, y, z)) = near {
        if let Some((sx, sy)) = read(process, |r, i| {
            super::screenmap::to_screen_at(r, i, x, y, z)
        })? {
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
    if let Some(path) = ends {
        // Drag along the border through every facing province pair, button held (right, then the
        // left one): the game draws the front over the dragged stretch only. A straight drag
        // between the two ends cut across own land on a bending border and the game took the
        // whole border (Silesia-Lodz, 18 Nov 2026).
        let screen: Vec<Option<(f64, f64)>> = read(process, |r, i| {
            path.iter()
                .map(|p| super::screenmap::to_screen_at(r, i, p.0, p.1, p.2))
                .collect::<Result<Vec<_>>>()
        })?;
        let inside =
            |p: &(f64, f64)| (300.0..1700.0).contains(&p.0) && (120.0..860.0).contains(&p.1);
        let mut pts: Vec<(f64, f64)> = Vec::new();
        for p in screen {
            let Some(p) = p.filter(inside) else {
                let _ = keys.press(Key::Escape);
                return Err(Error::new(
                    "input_geometry_unverified",
                    format!("The segment {path:?} does not fit the map area at once"),
                ));
            };
            if pts
                .last()
                .is_none_or(|q: &(f64, f64)| (q.0 - p.0).hypot(q.1 - p.1) >= 6.0)
            {
                pts.push(p);
            }
        }
        // Each point onto the highlighted border itself (the province midpoints lie beside the
        // dashed line; a press off the line made the release a plain click and the game took the
        // whole border, airdbg4 18 Nov 2026). Points with no highlight within reach are dropped.
        let snapped: Vec<(f64, f64)> = pts
            .iter()
            .filter_map(|p| {
                highlighted_near(&plain, &drawing, &tooltip, 1, *p, 3)
                    .into_iter()
                    .next()
                    .filter(|q| (q.0 - p.0).hypot(q.1 - p.1) < 45.0)
            })
            .collect();
        let mut pts: Vec<(f64, f64)> = Vec::new();
        for p in snapped {
            if pts
                .last()
                .is_none_or(|q: &(f64, f64)| (q.0 - p.0).hypot(q.1 - p.1) >= 6.0)
            {
                pts.push(p);
            }
        }
        if pts.len() < 2 {
            let _ = keys.press(Key::Escape);
            return Err(Error::new(
                "input_geometry_unverified",
                "The segment's border is not highlighted at two points or more at this zoom",
            ));
        }
        super::ui::trace_text(&format!("front: segment drag through {pts:?}"));
        // the right button first: the game's own words for a shorter front are "Right-Click and
        // drag the mouse on one of the borders ... to set the front in a shorter interval"; a
        // left press on the border is a Click, which "set[s] the assigned front along entire
        // border" (o1i-o1n, qs1: every armour front took the whole border and its divisions stood
        // one a province)
        for right in [true, false] {
            if pts.len() == 2 {
                if right {
                    keys.right_drag(GAME, pts[0], pts[1])?;
                } else {
                    keys.drag(GAME, pts[0], pts[1])?;
                }
            } else if right {
                keys.right_drag_path(GAME, &pts)?;
            } else {
                keys.drag_path(GAME, &pts)?;
            }
            let deadline = std::time::Instant::now() + std::time::Duration::from_millis(800);
            while std::time::Instant::now() < deadline {
                if read(process, |r, i| orders(r, i, army))? > before {
                    let _ = keys.press(Key::Escape);
                    return Ok(());
                }
                crate::phase::sleep(std::time::Duration::from_millis(40));
            }
        }
        let _ = keys.press(Key::Escape);
        return Err(Error::new(
            "input_readback_failed",
            "No segment front was placed by a drag along the border",
        ));
    }
    // An army group's front goes on with Shift+Click: the game's tooltip, "Shift+Click on one of
    // the borders against another country to set the assigned front without sub-army fronts
    // along entire border". One group front the field marshal divides among his armies, not a
    // front per army besides it (the owner's rule, 19 Nov 2026: few front lines, the field
    // marshal's front always by Shift+Click).
    let group_front = read(process, |r, i| {
        Ok(army_groups(r, i)?.contains(&group_named(r, i, army)?))
    })
    .unwrap_or(false);
    for (x, y) in candidates {
        if group_front {
            keys.shift_click(GAME, x, y)?;
        } else {
            keys.click(GAME, x, y)?;
        }
        let deadline = std::time::Instant::now() + std::time::Duration::from_millis(600);
        while std::time::Instant::now() < deadline {
            if read(process, |r, i| orders(r, i, army))? > before {
                return Ok(());
            }
            crate::phase::sleep(std::time::Duration::from_millis(40));
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
    shown_card(process, keys, army)?;
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
        // A division of an army in the army group `army` stands with the group.
        let own = r.u64(division + u.division_group)?;
        if own != group && (own == 0 || r.u64(own + u.army_group)? != group) {
            continue;
        }
        let location = r.u64(division + u.division_location)?;
        if location != 0 && r.u64(location)? == province_vtable {
            out.push(r.i32(location + u.province_id)?);
        }
    }
    Ok(out)
}
/// All battle-plan order nodes of `army` (top orders, fallback orders and their sub-orders).
pub(super) fn plan_nodes(r: &Reader<'_>, image: u64, army: &str) -> Result<usize> {
    let u = ui(r)?;
    let group = group_named(r, image, army)?;
    let mut stack = vec![];
    let lists = std::iter::once(u.group_orders).chain(u.group_fallback_lists.into_iter().flatten());
    for list in lists {
        let (_, data, count) = r.array_header(group + list)?;
        for index in 0..count.min(64) as u64 {
            stack.push((r.u64(data + index * 8)?, 0));
        }
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

/// What is left of `army`'s battle plan, by kind ("front x1, offensive x1"; empty when none).
pub(super) fn orders_left(r: &Reader<'_>, image: u64, army: &str) -> Result<String> {
    let group = group_named(r, image, army)?;
    Ok(orders::summary(&orders::group_orders(r, image, group)?))
}

/// Every order id of `army` (fronts, their arrows and targets, defensive lists).
pub(super) fn order_ids(r: &Reader<'_>, image: u64, army: &str) -> Result<Vec<i32>> {
    let group = group_named(r, image, army)?;
    let mut out = Vec::new();
    let mut stack: Vec<orders::OrderNode> = orders::group_orders(r, image, group)?
        .into_iter()
        .map(|(_, n)| n)
        .collect();
    while let Some(n) = stack.pop() {
        out.push(n.id);
        stack.extend(n.children);
    }
    Ok(out)
}

/// The order `id` of `army` and the front it hangs on (None when the army has no such order).
fn find_order(
    r: &Reader<'_>,
    image: u64,
    army: &str,
    id: i32,
) -> Result<Option<(orders::OrderNode, Option<i32>)>> {
    let group = group_named(r, image, army)?;
    for (_, top) in orders::group_orders(r, image, group)? {
        if top.id == id {
            return Ok(Some((top, None)));
        }
        for c in &top.children {
            if c.id == id || c.children.iter().any(|t| t.id == id) {
                return Ok(Some((c.clone(), Some(top.id))));
            }
        }
    }
    Ok(None)
}

/// Where to click an order: the map point (x, z), its province, and the order's two ends.
type OrderPoint = ((f64, f64), i32, (f64, f64), (f64, f64));

/// The point to click on an order: the middle of its arrow (the arrow's provinces in drawing
/// order), else the middle of its target line (map x, z) and the province there.
pub(super) fn order_click_point(
    node: &orders::OrderNode,
    pos: &dyn Fn(i32) -> Option<(f64, f64)>,
) -> Option<OrderPoint> {
    let pick = |provinces: &[i32]| -> Option<OrderPoint> {
        let pts: Vec<(i32, (f64, f64))> = provinces
            .iter()
            .filter_map(|p| pos(*p).map(|xz| (*p, xz)))
            .collect();
        let (p, mid) = *pts.get(pts.len() / 2)?;
        Some((mid, p, pts.first()?.1, pts.last()?.1))
    };
    pick(&node.provinces).or_else(|| {
        node.children
            .iter()
            .find(|t| t.kind == 2)
            .and_then(|t| pick(&t.provinces))
    })
}

/// Delete ONE battle-plan order of `army` (an offensive arrow, by its read-back id) the way a
/// player does: the bar's delete tool (left click), then a click on the arrow's middle on the
/// map. The front and every other order stay (the planning bonus of the front's divisions with
/// them). Verified by the id gone; the caller reads what else is left.
pub(super) fn delete_order(
    process: &dyn Process,
    keys: &dyn GameInput,
    game_dir: &std::path::Path,
    army: &str,
    id: i32,
) -> Result<Value> {
    clear_notices(process, keys)?;
    let Some((node, front)) = read(process, |r, i| find_order(r, i, army, id))? else {
        return Err(Error::new(
            "invalid_target",
            format!("{army} has no battle-plan order {id}"),
        ));
    };
    let pos = super::mapdata::positions(game_dir)?;
    let Some((mid, province, a, b)) = order_click_point(&node, &|p| pos.get(&p).copied()) else {
        return Err(Error::new(
            "invalid_target",
            format!("order {id} of {army} has no province on the map to click"),
        ));
    };
    let state = read(process, |r, i| {
        Ok(orders::state_of_province(r, i, province))
    })?;
    let area = (420.0, 140.0, 1700.0, 880.0);
    let framed = super::screenmap::frame_two(process, keys, a, b, area, || match state {
        Some(s) => find_state(process, keys, s),
        None => Ok(()),
    })?;
    let inside =
        |p: &(f64, f64)| (area.0..area.2).contains(&p.0) && (area.1..area.3).contains(&p.1);
    let at = match framed {
        None => None,
        Some(_) => read(process, |r, i| {
            super::screenmap::to_screen(r, i, mid.0, mid.1)
        })?
        .filter(inside),
    };
    let Some(at) = at else {
        let _ = keys.press(Key::Escape);
        return Err(Error::new(
            "input_geometry_unverified",
            format!("the middle of order {id} (province {province}) is not on the screen"),
        ));
    };
    let button = mode::delete_tool(process, keys, army)?;
    keys.click(GAME, at.0, at.1)?;
    // the game asks "Are you sure you want to DELETE ... order?" (CONFIRM_DELETE_ORDER_TEXT)
    let _ = super::ui::accept_command_until(process, keys, |r, i| {
        Ok(!order_ids(r, i, army)?.contains(&id))
    });
    let gone = wait(process, "order deleted", |r, i| {
        Ok(!order_ids(r, i, army)?.contains(&id))
    });
    let _ = keys.press(Key::Escape);
    let _ = clear_selection(process, keys);
    let left = read(process, |r, i| order_ids(r, i, army)).unwrap_or_default();
    let detail = json!({"army": army, "order": id, "front": front, "province": province,
        "clicked": [at.0, at.1], "delete_button": [button.x, button.y, button.w, button.h],
        "front_kept": front.map(|f| left.contains(&f)), "orders_left": left.len()});
    gone.map(|_| detail.clone()).map_err(|_| {
        Error::new(
            "input_readback_failed",
            format!("the delete tool clicked on order {id} of {army} at {at:?} (province {province}) and the order stayed: {detail}"),
        )
    })
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
    through: Option<i32>,
    spearhead: bool,
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
    let card = shown_card(process, keys, army)?;
    let (x, y) = card.center();
    keys.click(GAME, x, y)?;
    wait(process, "select army", |r, i| Ok(selection_size(r, i)? > 0))?;
    let draw_line = |from: (f64, f64), to: (f64, f64)| -> Result<()> {
        line_tool(process, keys, spearhead)?;
        keys.right_drag(GAME, from, to)?;
        wait(process, "offensive line placed", |r, i| {
            Ok(plan_nodes(r, i, army)? > before)
        })
    };
    let reselect = || -> Result<()> {
        let _ = keys.press(Key::Escape);
        let (x, y) = card.center();
        keys.click(GAME, x, y)?;
        wait(process, "select army", |r, i| Ok(selection_size(r, i)? > 0))
    };
    // As a player draws it: from the army to the target. The game takes an offensive line only
    // from the army's front, and the old arrow around the target's centre missed it whenever the
    // front lay farther away on screen (w22: every offensive from Eritrea toward Wello failed).
    let pos = super::mapdata::positions(game_dir)?;
    let mean = |ids: &[i32]| -> Option<(f64, f64)> {
        let pts: Vec<(f64, f64)> = ids.iter().filter_map(|p| pos.get(p).copied()).collect();
        (!pts.is_empty()).then(|| {
            let n = pts.len() as f64;
            pts.iter()
                .fold((0.0, 0.0), |a, p| (a.0 + p.0 / n, a.1 + p.1 / n))
        })
    };
    let target_provinces = super::mapdata::state_provinces(game_dir, state)?;
    // Every stroke tried, for the failure message: a line that was not placed says where it was
    // drawn (live 15 Nov 2026: the tool gave up toward Begemder without a word on its strokes).
    let mut tried: Vec<Stroke> = Vec::new();
    let failed =
        |tried: &[Stroke], cause: Option<Error>| offensive_failure(state, through, tried, cause);
    // First choice: the line across the target, parallel to the army's front and as wide as the
    // state, at the depth of the objective (`through`, else the middle of the state), so the
    // whole front advances to it. A stroke from the army to the target's centre is a line along
    // the advance: the army files down one corridor of provinces and stops at the centre with
    // the enemy on both flanks. A line on the far side of a large state is no better: the game
    // joins it to the front with one long arrow round the state (the Ogaden, live 15 Nov 2026).
    let own: Vec<(f64, f64)> = provinces
        .iter()
        .filter_map(|p| pos.get(p).copied())
        .collect();
    let theirs: Vec<(f64, f64)> = target_provinces
        .iter()
        .filter_map(|p| pos.get(p).copied())
        .collect();
    let pivot = through.and_then(|p| pos.get(&p).copied());
    // a spearhead is painted along the thrust (the game takes its provinces as painted): from
    // the army to the objective (`through`, else the state's centre), never across the state
    let strokes = if spearhead {
        // from the army's own front where it lies nearest the objective (the army's mean
        // position lay inland, half-way along a march, and the stroke crossed the border far from
        // the thrust: qs1, 19 Nov 2026), else from the army
        let goal = pivot.or_else(|| mean(&target_provinces));
        let front: Vec<(f64, f64)> = read(process, |r, i| {
            let group = group_named(r, i, army)?;
            Ok(orders::group_orders(r, i, group)?
                .into_iter()
                .filter(|(list, o)| *list == orders::List::Top && o.kind == 2)
                .flat_map(|(_, o)| o.provinces)
                .collect::<Vec<i32>>())
        })
        .unwrap_or_default()
        .iter()
        .filter_map(|p| pos.get(p).copied())
        .collect();
        let start = goal
            .and_then(|g| {
                front.iter().copied().min_by(|a, b| {
                    let d = |p: &(f64, f64)| (p.0 - g.0).powi(2) + (p.1 - g.1).powi(2);
                    d(a).total_cmp(&d(b))
                })
            })
            .or_else(|| mean(&provinces));
        vec![("spearhead from the front to the objective", start.zip(goal))]
    } else {
        vec![
            ("across the state", offensive_span(&own, &theirs, pivot)),
            (
                "from the army to the state's centre",
                mean(&provinces).zip(mean(&target_provinces)),
            ),
        ]
    };
    for (kind, ends) in strokes {
        let Some((a, b)) = ends else {
            tried.push(Stroke {
                kind,
                map: None,
                screen: None,
                outcome: format!(
                    "no such stroke from {} army and {} state province positions",
                    own.len(),
                    theirs.len()
                ),
            });
            continue;
        };
        let framed =
            super::screenmap::frame_two(process, keys, a, b, (420.0, 140.0, 1700.0, 880.0), || {
                find_state(process, keys, state)
            })
            .map_err(|e| failed(&tried, Some(e)))?;
        let Some((from, to)) = framed else {
            tried.push(Stroke {
                kind,
                map: Some((a, b)),
                screen: None,
                outcome: "its ends did not fit the map area together".into(),
            });
            continue;
        };
        let drawn = draw_line(from, to);
        if drawn.is_ok() || read(process, |r, i| plan_nodes(r, i, army))? > before {
            return Ok(());
        }
        tried.push(Stroke {
            kind,
            map: Some((a, b)),
            screen: Some((from, to)),
            outcome: "drawn, no new battle plan order".into(),
        });
        reselect().map_err(|e| failed(&tried, Some(e)))?;
    }
    // Fallback, the earlier stroke: the target centred, a short arrow from the army's side.
    find_state(process, keys, state).map_err(|e| failed(&tried, Some(e)))?;
    let clamp = |x: f64, y: f64| (x.clamp(320.0, 1880.0), y.clamp(120.0, 880.0));
    let (from, to) = (
        clamp(960.0 + dx * 260.0, 540.0 + dy * 260.0),
        clamp(960.0 - dx * 40.0, 540.0 - dy * 40.0),
    );
    if draw_line(from, to).is_ok() {
        return Ok(());
    }
    tried.push(Stroke {
        kind: "short arrow at the centred state",
        map: None,
        screen: Some((from, to)),
        outcome: "drawn, no new battle plan order".into(),
    });
    Err(failed(&tried, None))
}
/// Draw one offensive line for `army` across several target states side by side (a deep line
/// across the enemy country, as the game's own AI draws it: its Poland line of 1 September 1939
/// runs 22 provinces through eight states): X, then one right-button drag through the states'
/// whole width at their middle depth, perpendicular to the way from the army (see
/// `defense_span`). Verified by a new battle-plan order; the receipt reads the line back.
pub(super) fn offensive_across(
    process: &dyn Process,
    keys: &dyn GameInput,
    game_dir: &std::path::Path,
    army: &str,
    states: &[i32],
    spearhead: bool,
) -> Result<()> {
    clear_notices(process, keys)?;
    if read(process, |r, i| orders(r, i, army))? == 0 {
        return Err(Error::new(
            "invalid_target",
            "This army has no front line; draw one first",
        ));
    }
    let pos = super::mapdata::positions(game_dir)?;
    let mut centres = Vec::new();
    for state in states {
        let pts: Vec<(f64, f64)> = super::mapdata::state_provinces(game_dir, *state)?
            .iter()
            .filter_map(|p| pos.get(p).copied())
            .collect();
        if let Some(c) = across_centre(&pts) {
            centres.push(c);
        }
    }
    if spearhead {
        // a spearhead path starts at the army and runs through the states in the order given
        let own: Vec<(f64, f64)> = read(process, |r, i| army_provinces(r, i, army))?
            .iter()
            .filter_map(|p| pos.get(p).copied())
            .collect();
        if let Some(start) = across_centre(&own) {
            centres.insert(0, start);
        }
    }
    let Some((a, b)) = farthest_pair(&centres) else {
        return Err(Error::new(
            "invalid_target",
            format!("The states {states:?} give no line to draw (fewer than two map positions)"),
        ));
    };
    let before = read(process, |r, i| plan_nodes(r, i, army))?;
    let first = states.first().copied().unwrap_or(0);
    let area = (420.0, 140.0, 1700.0, 880.0);
    select_for_orders(process, keys, army)?;
    let framed = super::screenmap::frame_two(process, keys, a, b, area, || {
        find_state(process, keys, first)
    })?;
    let inside =
        |p: &(f64, f64)| (area.0..area.2).contains(&p.0) && (area.1..area.3).contains(&p.1);
    let screen: Option<Vec<(f64, f64)>> = match framed {
        None => None,
        Some(_) => read(process, |r, i| {
            centres
                .iter()
                .map(|c| super::screenmap::to_screen(r, i, c.0, c.1))
                .collect::<Result<Option<Vec<_>>>>()
        })?
        .filter(|pts| pts.iter().all(inside)),
    };
    let Some(path) = screen else {
        let _ = keys.press(Key::Escape);
        return Err(Error::new(
            "input_geometry_unverified",
            format!(
                "The offensive line through states {states:?} does not fit the screen at once; \
                 give fewer or nearer states"
            ),
        ));
    };
    select_for_orders(process, keys, army)?;
    line_tool(process, keys, spearhead)?;
    keys.right_drag_path(GAME, &path)?;
    let placed = wait(process, "offensive line placed", |r, i| {
        Ok(plan_nodes(r, i, army)? > before)
    });
    let _ = keys.press(Key::Escape);
    placed.map_err(|_| {
        Error::new(
            "input_readback_failed",
            format!(
                "An offensive stroke through states {states:?} ({} screen points from {:?} to \
                 {:?}) placed no new battle-plan order for {army}",
                path.len(),
                path.first(),
                path.last()
            ),
        )
    })
}
/// Enter the drawing of an attack line for the selected army: X for an offensive line, the
/// battle-plan bar's spearhead button (`btn_order_draw_blitz`, interface/battleplantools.gui;
/// its shortcut is Shift+X) for a spearhead. The game's tooltip: "Compared to Offensive Line a
/// Spearhead will aggressively try to take all provinces as they were there when the order was
/// first painted without adjusting to changes. Prefer Spearheads when you need to do
/// overlapping encirclements, cutting off the enemy or for more controlled smaller thrusts".
fn line_tool(process: &dyn Process, keys: &dyn GameInput, spearhead: bool) -> Result<()> {
    if spearhead {
        let (rect, state) = settled(process, "spearhead button", |r, i| {
            super::invasion::plan_button(r, i, "btn_order_draw_blitz")
        })?;
        if state == 3 {
            return Err(Error::new(
                "button_disabled",
                "The game disables the spearhead order for this army (it needs a front line)",
            ));
        }
        super::kit::click(keys, rect)?;
    } else {
        keys.press(Key::letter('x')?)?;
    }
    crate::phase::sleep(std::time::Duration::from_millis(300));
    Ok(())
}
/// The centre of a state's province positions (None for none).
fn across_centre(pts: &[(f64, f64)]) -> Option<(f64, f64)> {
    (!pts.is_empty()).then(|| {
        let n = pts.len() as f64;
        pts.iter()
            .fold((0.0, 0.0), |a, p| (a.0 + p.0 / n, a.1 + p.1 / n))
    })
}
/// The two points farthest apart (None for fewer than two distinct points).
pub(super) fn farthest_pair(pts: &[(f64, f64)]) -> Option<((f64, f64), (f64, f64))> {
    type Pair = ((f64, f64), (f64, f64));
    let mut best: Option<(f64, Pair)> = None;
    for (i, a) in pts.iter().enumerate() {
        for b in &pts[i + 1..] {
            let d = (a.0 - b.0).powi(2) + (a.1 - b.1).powi(2);
            if d > 1e-9 && best.is_none_or(|(x, _)| d > x) {
                best = Some((d, (*a, *b)));
            }
        }
    }
    best.map(|(_, pair)| pair)
}

/// One stroke `offensive` tried: its ends on the map (unit positions, x and z) and on the screen
/// (game pixels) where it got that far, and how it ended.
struct Stroke {
    kind: &'static str,
    map: Option<((f64, f64), (f64, f64))>,
    screen: Option<((f64, f64), (f64, f64))>,
    outcome: String,
}
/// The failure of an offensive line with every stroke tried. `cause` is the error that ended
/// the attempts early (its code is kept); without one all strokes were drawn and none placed
/// a line.
fn offensive_failure(
    state: i32,
    through: Option<i32>,
    tried: &[Stroke],
    cause: Option<Error>,
) -> Error {
    let ends = |e: &((f64, f64), (f64, f64))| {
        format!(
            "({:.0},{:.0})-({:.0},{:.0})",
            e.0 .0, e.0 .1, e.1 .0, e.1 .1
        )
    };
    let strokes: Vec<String> = tried
        .iter()
        .enumerate()
        .map(|(n, s)| {
            let mut line = format!("{} {}", n + 1, s.kind);
            if let Some(map) = &s.map {
                line.push_str(&format!(" map {}", ends(map)));
            }
            if let Some(screen) = &s.screen {
                line.push_str(&format!(" screen {}", ends(screen)));
            }
            format!("{line}: {}", s.outcome)
        })
        .collect();
    let depth = through.map_or_else(
        || "the middle of the state".to_string(),
        |p| format!("province {p}"),
    );
    let tried_text = if strokes.is_empty() {
        "no stroke was drawn".to_string()
    } else {
        format!("strokes tried: {}", strokes.join("; "))
    };
    match cause {
        Some(e) => Error::new(
            e.code,
            format!(
                "{} (offensive line into state {state} at the depth of {depth}; {tried_text})",
                e.message
            ),
        ),
        None => Error::new(
            "input_readback_failed",
            format!(
                "offensive line placed: expected game change not observed. No offensive line \
                 into state {state} at the depth of {depth} was taken by the game; {tried_text}"
            ),
        ),
    }
}
/// The two ends of an offensive line across a target, parallel to the army's front: of the
/// third of the target's provinces nearest in depth to `pivot` (the objective; without one, the
/// target's centre), the two farthest apart sideways (map points; `own` are the army's province
/// positions, `target` the target's). The game extends the line to the edges of the enemy land
/// whatever its length, so only its depth is chosen here. None when the target has no width
/// seen from the army (one province, or all in a row along the advance).
pub(super) fn offensive_span(
    own: &[(f64, f64)],
    target: &[(f64, f64)],
    pivot: Option<(f64, f64)>,
) -> Option<((f64, f64), (f64, f64))> {
    let mean = |pts: &[(f64, f64)]| -> Option<(f64, f64)> {
        (!pts.is_empty()).then(|| {
            let n = pts.len() as f64;
            pts.iter()
                .fold((0.0, 0.0), |a, p| (a.0 + p.0 / n, a.1 + p.1 / n))
        })
    };
    let (from, to) = (mean(own)?, mean(target)?);
    let (dx, dy) = (to.0 - from.0, to.1 - from.1);
    let len = (dx * dx + dy * dy).sqrt();
    if len < 1e-6 {
        return None;
    }
    let (dx, dy) = (dx / len, dy / len);
    let depth = |p: &(f64, f64)| (p.0 - to.0) * dx + (p.1 - to.1) * dy;
    let side = |p: &(f64, f64)| -(p.0 - to.0) * dy + (p.1 - to.1) * dx;
    let at = pivot.as_ref().map_or(0.0, depth);
    let mut band: Vec<&(f64, f64)> = target.iter().collect();
    band.sort_by(|x, y| (depth(x) - at).abs().total_cmp(&(depth(y) - at).abs()));
    // the third nearest in depth, and every province as near as the last of them
    let last = (target.len() / 3).max(3).min(band.len()) - 1;
    let reach = (depth(band[last]) - at).abs() + 1e-6;
    band.retain(|p| (depth(p) - at).abs() <= reach);
    let a = band
        .iter()
        .copied()
        .max_by(|x, y| side(x).total_cmp(&side(y)))?;
    let b = band
        .iter()
        .copied()
        .min_by(|x, y| side(x).total_cmp(&side(y)))?;
    // narrower than a tenth of the way there: no line across to speak of
    (side(a) - side(b) > (len * 0.1).max(1.0)).then_some((*a, *b))
}
/// The stroke of a defensive line through the provinces `line` (map positions) for an army
/// standing at `army`: across the line's whole width, at its middle depth, perpendicular to the
/// way from the line to the army (the army's front lies ahead of a fallback line). With the army
/// inside the line (no way to speak of) the line's longest spread is used. None for fewer than
/// two distinct positions.
pub(super) fn defense_span(
    army: &[(f64, f64)],
    line: &[(f64, f64)],
) -> Option<((f64, f64), (f64, f64))> {
    if line.len() < 2 {
        return None;
    }
    let n = line.len() as f64;
    let c = line
        .iter()
        .fold((0.0, 0.0), |a, p| (a.0 + p.0 / n, a.1 + p.1 / n));
    let toward = (!army.is_empty()).then(|| {
        let m = army.len() as f64;
        let a = army
            .iter()
            .fold((0.0, 0.0), |s, p| (s.0 + p.0 / m, s.1 + p.1 / m));
        (a.0 - c.0, a.1 - c.1)
    });
    let spread = line
        .iter()
        .map(|p| ((p.0 - c.0).powi(2) + (p.1 - c.1).powi(2)).sqrt())
        .fold(0.0, f64::max);
    // the side axis: perpendicular to the army's side, else the main axis of the points
    let axis = match toward {
        Some((dx, dy)) if (dx * dx + dy * dy).sqrt() > spread * 0.25 => {
            let len = (dx * dx + dy * dy).sqrt();
            (-dy / len, dx / len)
        }
        _ => {
            let (mut sxx, mut syy, mut sxy) = (0.0, 0.0, 0.0);
            for p in line {
                let (x, y) = (p.0 - c.0, p.1 - c.1);
                sxx += x * x;
                syy += y * y;
                sxy += x * y;
            }
            let angle = 0.5 * (2.0 * sxy).atan2(sxx - syy);
            (angle.cos(), angle.sin())
        }
    };
    let side = |p: &(f64, f64)| (p.0 - c.0) * axis.0 + (p.1 - c.1) * axis.1;
    let lo = line.iter().map(side).fold(f64::MAX, f64::min);
    let hi = line.iter().map(side).fold(f64::MIN, f64::max);
    (hi - lo > 1e-6).then_some((
        (c.0 + axis.0 * lo, c.1 + axis.1 * lo),
        (c.0 + axis.0 * hi, c.1 + axis.1 * hi),
    ))
}

/// Select `army` with its card (the battle-plan toolbar shows) and return the card's centre.
fn select_for_orders(process: &dyn Process, keys: &dyn GameInput, army: &str) -> Result<()> {
    clear_notices(process, keys)?;
    let card = shown_card(process, keys, army)?;
    let (x, y) = card.center();
    keys.click(GAME, x, y)?;
    wait(process, "select army", |r, i| Ok(selection_size(r, i)? > 0))
}

/// Draw a fallback line for `army` through `states` (battle-plan toolbar fallback, key C): one
/// right-button stroke across the states at their middle depth, perpendicular to the way to the
/// army's front (see `defense_span`). Verified by new battle-plan orders on the army; the army
/// moves to the line when its plan is executed.
pub(super) fn fallback(
    process: &dyn Process,
    keys: &dyn GameInput,
    game_dir: &std::path::Path,
    army: &str,
    states: &[i32],
) -> Result<()> {
    let pos = super::mapdata::positions(game_dir)?;
    let mut line = Vec::new();
    for state in states {
        for p in super::mapdata::state_provinces(game_dir, *state)? {
            if let Some(xz) = pos.get(&p) {
                line.push(*xz);
            }
        }
    }
    let at: Vec<(f64, f64)> = read(process, |r, i| army_provinces(r, i, army))?
        .iter()
        .filter_map(|p| pos.get(p).copied())
        .collect();
    let Some((a, b)) = defense_span(&at, &line) else {
        return Err(Error::new(
            "invalid_target",
            format!("The states {states:?} give no line to draw (fewer than two map positions)"),
        ));
    };
    let before = read(process, |r, i| plan_nodes(r, i, army))?;
    select_for_orders(process, keys, army)?;
    let first = states.first().copied().unwrap_or(0);
    let framed =
        super::screenmap::frame_two(process, keys, a, b, (420.0, 140.0, 1700.0, 880.0), || {
            find_state(process, keys, first)
        })?;
    let Some((from, to)) = framed else {
        let _ = keys.press(Key::Escape);
        return Err(Error::new(
            "input_geometry_unverified",
            format!(
                "The fallback line's ends ({a:?} to {b:?} on the map) do not fit the screen together; \
                 give fewer or nearer states"
            ),
        ));
    };
    select_for_orders(process, keys, army)?;
    keys.press(Key::letter('c')?)?;
    crate::phase::sleep(std::time::Duration::from_millis(300));
    keys.right_drag(GAME, from, to)?;
    let placed = wait(process, "fallback line placed", |r, i| {
        Ok(plan_nodes(r, i, army)? > before)
    });
    let _ = keys.press(Key::Escape);
    placed.map_err(|_| {
        Error::new(
            "input_readback_failed",
            format!(
                "A fallback stroke from {from:?} to {to:?} (screen) placed no new battle-plan order \
                 for {army}"
            ),
        )
    })
}

/// Give `army` an area defence order over `states` (battle-plan toolbar area defence, key V):
/// one LEFT click on each state's centre (Find view first): the game's own help reads "Click on
/// states to add/remove them from the army's order", where the line tools say "Right-Click and
/// drag" (a right-click with the tool, fb1 16 Nov 2026, placed nothing on two armies). Verified by
/// new battle-plan orders on the army. The order's settings window is left at the game's defaults.
pub(super) fn area_defense(
    process: &dyn Process,
    keys: &dyn GameInput,
    game_dir: &std::path::Path,
    army: &str,
    states: &[i32],
) -> Result<()> {
    let pos = super::mapdata::positions(game_dir)?;
    let before = read(process, |r, i| plan_nodes(r, i, army))?;
    // Provinces where own divisions stand: a click there lands on a unit counter and selects
    // those divisions instead of adding the state (e5/e6, 16 Nov 2026: Army of Wello's area order
    // over 843 and 271 never placed, five tries a run; both states' central provinces hold
    // stacks, Dessie and Addis Ababa).
    let occupied: std::collections::HashSet<i32> = read(process, |r, i| {
        Ok(division_provinces(r, i)?.into_values().flatten().collect())
    })
    .unwrap_or_default();
    let mut clicked = 0;
    for state in states {
        let all: Vec<i32> = super::mapdata::state_provinces(game_dir, *state)?;
        let free: Vec<i32> = all
            .iter()
            .copied()
            .filter(|p| !occupied.contains(p))
            .collect();
        let pts: Vec<(f64, f64)> = if free.is_empty() { &all } else { &free }
            .iter()
            .filter_map(|p| pos.get(p).copied())
            .collect();
        if pts.is_empty() {
            continue;
        }
        let n = pts.len() as f64;
        let c = pts
            .iter()
            .fold((0.0, 0.0), |a, p| (a.0 + p.0 / n, a.1 + p.1 / n));
        // the province nearest the centre, so the click lands inside the state
        let inner = pts
            .iter()
            .copied()
            .min_by(|x, y| {
                let d = |p: &(f64, f64)| (p.0 - c.0).powi(2) + (p.1 - c.1).powi(2);
                d(x).total_cmp(&d(y))
            })
            .unwrap_or(c);
        find_state(process, keys, *state)?;
        let Some((x, y)) = read(process, |r, i| {
            super::screenmap::to_screen(r, i, inner.0, inner.1)
        })?
        else {
            continue;
        };
        if !(420.0..1700.0).contains(&x) || !(140.0..880.0).contains(&y) {
            continue;
        }
        select_for_orders(process, keys, army)?;
        keys.press(Key::letter('v')?)?;
        crate::phase::sleep(std::time::Duration::from_millis(300));
        // a click toggles the state: a second click on a state already in the order removes it
        keys.click(GAME, x, y)?;
        crate::phase::sleep(std::time::Duration::from_millis(250));
        clicked += 1;
    }
    let _ = keys.press(Key::Escape);
    if clicked == 0 {
        return Err(Error::new(
            "input_geometry_unverified",
            format!("No state of {states:?} could be put on screen for the area order"),
        ));
    }
    wait(process, "area defence placed", |r, i| {
        Ok(plan_nodes(r, i, army)? > before)
    })
    .map_err(|_| {
        Error::new(
            "input_readback_failed",
            format!("{clicked} area-defence click(s) placed no new battle-plan order for {army}"),
        )
    })
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
fn default_map_mode(process: &dyn Process, keys: &dyn GameInput) -> Result<()> {
    super::ui::leave_strategic_modes(process, keys).map(|_| ())
}
pub(super) fn box_select_exact(
    process: &dyn Process,
    keys: &dyn GameInput,
    state: i32,
    expected: usize,
) -> Result<()> {
    default_map_mode(process, keys)?;
    find_state(process, keys, state)?;
    let mut sizes = Vec::new();
    for (from, to) in ASSIGN_BOXES {
        keys.drag(GAME, from, to)?;
        crate::phase::sleep(std::time::Duration::from_millis(350));
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
    let card = shown_card(process, keys, army)?;
    let (x, y) = card.center();
    keys.click(GAME, x, y)?;
    wait(process, "select army", |r, i| Ok(selection_size(r, i)? > 0))?;
    let before = read(process, |r, i| headings(r, i, army))?;
    // Every division already marches on the named province: the order stands, and a second
    // click would only read as an order that changed nothing.
    if let Some(p) = province {
        if !before.is_empty() && before.iter().all(|h| *h == p) {
            return Ok(());
        }
    }
    // The state already on screen (the usual case for a front-line army): right-click one of
    // its projected provinces where the camera is. The Find view (open, type, glide, about
    // 1.5 s) only when none shows, and the camera stays where the viewer follows the units.
    // Far out (above the aiming height) a whole state is a few pixels under its own map icons:
    // no click on it gives an order (live 15 Nov 2026: a sea move to Eritrea from the world view
    // showed the route on hover and never took).
    let close = super::screenmap::aim_height_ok(process);
    let visible = with_icon_offsets(
        target_points(process, state, province).unwrap_or_default(),
        province,
    );
    if close
        && !visible.is_empty()
        && order_at(process, keys, army, &before, visible, false, province)?
    {
        return Ok(());
    }
    find_state(process, keys, state)?;
    // The Find view centres the state but keeps the height: bring a far camera in first.
    if !super::screenmap::aim_height_ok(process) {
        super::screenmap::zoom_in_to_strategic(process, keys)?;
        super::screenmap::wait_camera_still(process, 700);
    }
    // A state order goes to the state's port first: inland points near a front lie under the
    // drawn front lines and battle-plan arrows, which take the right click for themselves (live
    // 15 Nov 2026: a sea move to Eritrea clicked North Guard's offensive arrow every time), and
    // a port is where a sea transfer lands. Any heading change counts (a sea move first heads
    // for the embarkation port).
    if province.is_none() {
        if let Some(port) = state_port(process, state) {
            let points = twice(with_icon_offsets(
                target_points(process, state, Some(port)).unwrap_or_default(),
                Some(port),
            ));
            if !points.is_empty() && order_at(process, keys, army, &before, points, false, None)? {
                return Ok(());
            }
        }
    }
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
    } else if points.is_empty() {
        // A named province of a large state lies off screen at the Find view's zoom (Dire Dawa
        // at the far edge of Hararghe, live 15 Nov 2026: no click was sent and the order read
        // as not applied): zoom out until it shows.
        if let Some(at) = province_position(process, province) {
            let area = (470.0, 120.0, 1700.0, 880.0);
            if let Some((point, _)) =
                super::screenmap::frame_two(process, keys, at, at, area, || Ok(()))?
            {
                points.push(point);
            }
        }
    }
    let points = with_icon_offsets(points, province);
    let points = if province.is_none() {
        twice(points)
    } else {
        points
    };
    if order_at(process, keys, army, &before, points, projected, province)? {
        return Ok(());
    }
    // A named province that took no order is an error with its reason, not a silent success
    // the caller finds out about from the divisions standing still.
    match province {
        Some(p) => Err(Error::new(
            "input_readback_failed",
            format!("No division of the army took a march order to province {p} (not on screen, or its map icons took every click)"),
        )),
        None => Ok(()),
    }
}
/// Each point twice: the game plans a long route (a sea transfer) while the cursor rests on the
/// target and takes a right click only once the route shows; a click that arrives with the
/// cursor is dropped (live 15 Nov 2026: the hover read "first division arrives in 19 days" and
/// no order took), the second click on the same point lands on the planned route.
fn twice(points: Vec<(f64, f64)>) -> Vec<(f64, f64)> {
    points.into_iter().flat_map(|p| [p, p]).collect()
}
/// The province of a naval base in `state`, when the state has one.
fn state_port(process: &dyn Process, state: i32) -> Option<i32> {
    let identity = process.identity()?;
    let dir = super::mapdata::game_dir(&identity.exe).ok()?;
    let provinces = super::mapdata::state_provinces(&dir, state).ok()?;
    let ports = read(process, super::naval::naval_base_provinces).ok()?;
    ports.into_iter().find(|p| provinces.contains(p))
}
/// Map position of a named province (map files), when it has one.
fn province_position(process: &dyn Process, province: Option<i32>) -> Option<(f64, f64)> {
    let identity = process.identity()?;
    let dir = super::mapdata::game_dir(&identity.exe).ok()?;
    let positions = super::mapdata::positions(&dir).ok()?;
    positions.get(&province?).copied()
}
/// A named province has one map point, and its city, victory point, hub or port icon can sit on
/// it and take the right click for itself: after the point, three points beside it.
fn with_icon_offsets(points: Vec<(f64, f64)>, province: Option<i32>) -> Vec<(f64, f64)> {
    if province.is_none() {
        return points;
    }
    let mut out = points.clone();
    for (x, y) in points {
        out.extend([(x + 14.0, y + 12.0), (x - 14.0, y + 12.0), (x, y - 16.0)]);
    }
    out
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
    want: Option<i32>,
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
            let now = read(process, |r, i| headings(r, i, army))?;
            // A named province: the order counts when divisions march on it. A point beside its
            // icon can fall in the next province; the next point then corrects the order.
            let took = match want {
                Some(p) => now.contains(&p),
                None => now != before,
            };
            if took {
                return Ok(true);
            }
            crate::phase::sleep(std::time::Duration::from_millis(40));
        }
    }
    Ok(false)
}
/// Order some divisions of `army` (ids as hoi4_armies lists them) to march on `province` of
/// `state`, one division at a time with the game's controls: the army's card opens its division
/// list, a click on a division's row selects that division alone (as for a detach), then one
/// right click on the province. Each order counts when that division marches on the province;
/// the others of the army keep their orders. A division already marching on it gets no input.
/// A division that takes no order (live 15 Nov 2026: one division refused every order for weeks
/// and held its whole army back) is passed over; an error only when none took the order.
pub(super) fn move_divisions(
    process: &dyn Process,
    keys: &dyn GameInput,
    army: &str,
    ids: &[String],
    state: i32,
    province: i32,
) -> Result<()> {
    let mut refused: Vec<&str> = Vec::new();
    // all of them in one selection and one right click when the game takes it; whoever did not
    // take the order is then ordered alone below
    let heading = read(process, division_headings)?;
    let todo: Vec<String> = ids
        .iter()
        .filter(|id| heading.get(*id) != Some(&province))
        .cloned()
        .collect();
    let picked = if todo.len() >= 2 {
        select_rows(process, keys, army, &todo)?
    } else {
        Vec::new()
    };
    if picked.len() >= 2 {
        let todo = &picked;
        let mut points = target_points(process, state, Some(province)).unwrap_or_default();
        if points.is_empty() {
            find_state(process, keys, state)?;
            points = target_points(process, state, Some(province)).unwrap_or_default();
        }
        for (x, y) in with_icon_offsets(points, Some(province)) {
            keys.right_click(GAME, x, y)?;
            let deadline = std::time::Instant::now() + std::time::Duration::from_millis(700);
            let mut any = false;
            while std::time::Instant::now() < deadline {
                let now = read(process, division_headings)?;
                let took = todo
                    .iter()
                    .filter(|id| now.get(*id) == Some(&province))
                    .count();
                if took == todo.len() {
                    any = true;
                    break;
                }
                if took > 0 && !any {
                    // the order landed: the rest of the selection follows within moments
                    any = true;
                }
                crate::phase::sleep(std::time::Duration::from_millis(40));
            }
            if any {
                break;
            }
        }
    }
    for id in ids {
        if read(process, division_headings)?.get(id) == Some(&province) {
            continue;
        }
        clear_notices(process, keys)?;
        select_army(process, keys, army)?;
        let Ok(row) = division_row_scrolled(process, keys, id) else {
            refused.push(id);
            continue;
        };
        let (x, y) = row.center();
        keys.click(GAME, x, y)?;
        wait(process, "division selected", |r, i| {
            Ok(selection_size(r, i)? & 0xffff_ffff == 1)
        })?;
        let mut points = target_points(process, state, Some(province)).unwrap_or_default();
        if points.is_empty() {
            find_state(process, keys, state)?;
            points = target_points(process, state, Some(province)).unwrap_or_default();
        }
        if points.is_empty() {
            if let Some(at) = province_position(process, Some(province)) {
                let area = (470.0, 120.0, 1700.0, 880.0);
                if let Some((point, _)) =
                    super::screenmap::frame_two(process, keys, at, at, area, || Ok(()))?
                {
                    points.push(point);
                }
            }
        }
        let mut took = false;
        for (x, y) in with_icon_offsets(points, Some(province)) {
            keys.right_click(GAME, x, y)?;
            let deadline = std::time::Instant::now() + std::time::Duration::from_millis(700);
            while std::time::Instant::now() < deadline {
                if read(process, division_headings)?.get(id) == Some(&province) {
                    took = true;
                    break;
                }
                crate::phase::sleep(std::time::Duration::from_millis(40));
            }
            if took {
                break;
            }
        }
        if !took {
            refused.push(id);
        }
    }
    if !ids.is_empty() && refused.len() == ids.len() {
        return Err(Error::new(
            "input_readback_failed",
            format!("No division took a march order to province {province} (not on screen, or its map icons took every click): {}", refused.join(", ")),
        ));
    }
    Ok(())
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
pub(super) fn leader_band(r: &Reader<'_>, image: u64) -> (i32, i32) {
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
    shown_card(process, keys, army)?;
    let card = settled(process, "card", |r, i| {
        card_rect(r, i, army, "portrait_btn")
    })?;
    let (x, y) = card.center();
    keys.click(GAME, x, y)?;
    wait(process, "select", |r, i| {
        Ok(selection_size(r, i)? & 0xffff_ffff > 0)
    })?;
    crate::phase::sleep(std::time::Duration::from_millis(250));
    keys.click(GAME, COMMANDER_SLOT.0, COMMANDER_SLOT.1)?;
    wait(process, "commander list open", leader_list_open)?;
    let wanted = general.to_lowercase();
    let find = |r: &Reader<'_>, i: u64| -> Result<Option<Rect>> {
        Ok(leader_rows(r, i)?
            .into_iter()
            .find(|(n, _)| n.to_lowercase() == wanted)
            .map(|(_, rect)| rect))
    };
    let mut found = settled(process, "commander list", find)?;
    // The list draws only the rows in its band (pm1/pm2, 17 Nov 2026: Guderian and Hoth, both
    // recruited, were "not listed" among the first ten): wheel it down a page at a time and
    // read the drawn rows again until the general shows or a page brings no new name.
    let mut names: Vec<String> = Vec::new();
    if found.is_none() {
        for _ in 0..12 {
            let rows = read(process, leader_rows)?;
            let fresh: Vec<String> = rows
                .iter()
                .map(|(n, _)| n.clone())
                .filter(|n| !names.contains(n))
                .collect();
            if fresh.is_empty() {
                break;
            }
            names.extend(fresh);
            let Some((_, first)) = rows.first() else {
                break;
            };
            let band = read(process, |r, i| Ok(leader_band(r, i)))?;
            let at = (
                f64::from(first.x + first.w / 2),
                f64::from(band.0 + band.1) / 2.0,
            );
            keys.scroll(GAME, at, -f64::from(band.1 - band.0).max(120.0) as i32)?;
            crate::phase::sleep(std::time::Duration::from_millis(300));
            found = settled(process, "commander list", find)?;
            if found.is_some() {
                break;
            }
        }
    }
    let Some(rect) = found else {
        let names: Vec<String> = if names.is_empty() {
            read(process, leader_rows)?
                .into_iter()
                .map(|(n, _)| n)
                .collect()
        } else {
            names
        };
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
                crate::phase::sleep(std::time::Duration::from_millis(250));
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
            crate::phase::sleep(std::time::Duration::from_millis(400));
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
    #[test]
    fn several_own_states_face_every_state_beside_them_the_target_first() {
        let mut adj: BTreeMap<i32, BTreeSet<i32>> = BTreeMap::new();
        adj.insert(1, BTreeSet::from([2, 10, 11]));
        adj.insert(2, BTreeSet::from([1, 11, 12]));
        assert_eq!(facing_states(&adj, &[1, 2], 12), [12, 10, 11]);
        assert_eq!(
            segment_states(Some(&json!([1, 2]))).unwrap(),
            Some(vec![1, 2])
        );
        assert_eq!(segment_states(None).unwrap(), None);
        for bad in [json!([1]), json!([1, 1]), json!([0, 2]), json!("1,2")] {
            assert!(segment_states(Some(&bad)).is_err(), "{bad}");
        }
    }

    #[test]
    fn a_segment_runs_between_the_farthest_facing_province_pairs() {
        // own provinces along x = 0, the target's along x = 10; one own province far inland
        let mine = [
            ((0.0, 0.0), 1.0),
            ((0.0, 10.0), 1.0),
            ((0.0, 20.0), 1.0),
            ((-40.0, 10.0), 1.0),
        ];
        let theirs = [((10.0, 0.0), 3.0), ((10.0, 10.0), 3.0), ((10.0, 20.0), 3.0)];
        let (a, b) = super::segment_from_points(&mine, &theirs).unwrap();
        let mut ends = [a, b];
        ends.sort_by(|x, y| x.2.total_cmp(&y.2));
        assert_eq!(ends, [(5.0, 2.0, 0.0), (5.0, 2.0, 20.0)]);
        assert!(super::segment_from_points(&[], &theirs).is_none());
    }

    #[test]
    fn a_folded_army_tries_its_own_group_card_first_and_never_an_army_or_new_card() {
        let rect = |x: i32| Rect {
            x,
            y: 0,
            w: 10,
            h: 10,
        };
        let armies: BTreeSet<u64> = [11, 12].into_iter().collect();
        // bar order: new-army card, army 11, group 7, group 9 (the parent), army 12
        let cards = vec![
            (0, rect(0)),
            (11, rect(1)),
            (7, rect(2)),
            (9, rect(3)),
            (12, rect(4)),
        ];
        let order = super::group_cards_in_order(cards, &armies, 9);
        assert_eq!(order.iter().map(|r| r.x).collect::<Vec<_>>(), [3, 2]);
    }
    use super::*;

    #[test]
    fn an_offensive_line_runs_across_the_target_at_the_depth_of_its_objective() {
        // the army stands south of a target three provinces wide and three deep
        let own = [(0.0, 0.0), (10.0, 0.0), (20.0, 0.0)];
        let mut target = Vec::new();
        for y in [30.0, 50.0, 70.0] {
            for x in [0.0, 10.0, 20.0] {
                target.push((x, y));
            }
        }
        // without an objective: across the middle row, side to side
        let (a, b) = super::offensive_span(&own, &target, None).unwrap();
        assert_eq!((a.1, b.1), (50.0, 50.0));
        assert_eq!((a.0 - b.0).abs(), 20.0);
        // an objective in the near row: the line runs along that row
        let (a, b) = super::offensive_span(&own, &target, Some((10.0, 30.0))).unwrap();
        assert_eq!((a.1, b.1), (30.0, 30.0));
        assert_eq!((a.0 - b.0).abs(), 20.0);
        // provinces as near in depth as the last of the band all belong to it
        let wide: Vec<(f64, f64)> = (0..9).map(|i| (f64::from(i) * 10.0, 40.0)).collect();
        let (a, b) = super::offensive_span(&[(40.0, 0.0)], &wide, Some((40.0, 40.0))).unwrap();
        assert_eq!((a.0 - b.0).abs(), 80.0);
        // and one in the far row
        let (a, b) = super::offensive_span(&own, &target, Some((0.0, 70.0))).unwrap();
        assert_eq!((a.1, b.1), (70.0, 70.0));
    }

    #[test]
    fn the_delete_tool_clicks_the_middle_of_the_arrow_else_of_its_target_line() {
        let pos = |p: i32| (p != 99).then(|| (f64::from(p), 2.0 * f64::from(p)));
        let line = |provinces: Vec<i32>| orders::OrderNode {
            kind: 2,
            provinces,
            ..Default::default()
        };
        let arrow = orders::OrderNode {
            kind: 1,
            id: 41,
            provinces: vec![10, 11, 99, 12, 13],
            children: vec![line(vec![20, 21, 22])],
            ..Default::default()
        };
        // the provinces with a map position: 10 11 12 13, the middle one 12
        let (at, province, a, b) = order_click_point(&arrow, &pos).unwrap();
        assert_eq!((at, province), ((12.0, 24.0), 12));
        assert_eq!((a, b), ((10.0, 20.0), (13.0, 26.0)));
        // an arrow with no province on the map: the middle of its target line
        let bare = orders::OrderNode {
            provinces: vec![99],
            ..arrow.clone()
        };
        assert_eq!(order_click_point(&bare, &pos).unwrap().1, 21);
        let none = orders::OrderNode {
            children: vec![],
            ..bare
        };
        assert!(order_click_point(&none, &pos).is_none());
    }

    #[test]
    fn a_failed_offensive_line_names_every_stroke_it_tried() {
        let tried = [
            Stroke {
                kind: "across the state",
                map: Some(((3400.4, 1210.0), (3460.0, 1190.6))),
                screen: Some(((812.0, 400.0), (1104.5, 470.0))),
                outcome: "drawn, no new battle plan order".into(),
            },
            Stroke {
                kind: "from the army to the state's centre",
                map: Some(((3420.0, 1260.0), (3431.0, 1201.0))),
                screen: None,
                outcome: "its ends did not fit the map area together".into(),
            },
            Stroke {
                kind: "short arrow at the centred state",
                map: None,
                screen: Some(((960.0, 800.0), (960.0, 500.0))),
                outcome: "drawn, no new battle plan order".into(),
            },
        ];
        let e = offensive_failure(841, Some(4050), &tried, None);
        assert_eq!(e.code, "input_readback_failed");
        assert_eq!(
            e.message,
            "offensive line placed: expected game change not observed. No offensive line into \
             state 841 at the depth of province 4050 was taken by the game; strokes tried: \
             1 across the state map (3400,1210)-(3460,1191) screen (812,400)-(1104,470): drawn, \
             no new battle plan order; 2 from the army to the state's centre map \
             (3420,1260)-(3431,1201): its ends did not fit the map area together; 3 short arrow \
             at the centred state screen (960,800)-(960,500): drawn, no new battle plan order"
        );
        // an error that ended the attempts keeps its code and gains the strokes so far
        let e = offensive_failure(
            841,
            None,
            &tried[..1],
            Some(Error::new(
                "gui_target_unavailable",
                "Find view did not open",
            )),
        );
        assert_eq!(e.code, "gui_target_unavailable");
        assert!(
            e.message.starts_with(
                "Find view did not open (offensive line into state 841 at the depth of the \
                 middle of the state; strokes tried: 1 across the state map"
            ),
            "{}",
            e.message
        );
        let e = offensive_failure(841, None, &[], Some(Error::new("torn_read", "x")));
        assert!(e.message.ends_with("no stroke was drawn)"), "{}", e.message);
    }

    #[test]
    fn the_deep_line_is_framed_by_its_two_farthest_state_centres() {
        let pts = [(0.0, 0.0), (5.0, 1.0), (1.0, 9.0), (2.0, 4.0)];
        assert_eq!(farthest_pair(&pts), Some(((0.0, 0.0), (1.0, 9.0))));
        assert_eq!(farthest_pair(&[(1.0, 1.0), (1.0, 1.0)]), None);
        assert_eq!(across_centre(&[(0.0, 0.0), (2.0, 4.0)]), Some((1.0, 2.0)));
    }

    #[test]
    fn a_target_without_width_has_no_line_across() {
        let own = [(0.0, 0.0)];
        assert!(super::offensive_span(&own, &[(0.0, 30.0)], None).is_none());
        // provinces in a row along the advance
        assert!(
            super::offensive_span(&own, &[(0.0, 30.0), (0.0, 50.0), (0.0, 70.0)], None).is_none()
        );
        assert!(super::offensive_span(&[], &[(0.0, 30.0), (9.0, 30.0)], None).is_none());
        // the army standing on the target's centre has no direction
        assert!(super::offensive_span(&[(5.0, 5.0)], &[(0.0, 5.0), (10.0, 5.0)], None).is_none());
    }

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
    fn a_defensive_line_runs_across_the_states_facing_the_army() {
        // the army stands north (smaller z is up the screen? only the geometry matters here)
        let line = [(0.0, 0.0), (10.0, 1.0), (20.0, 0.0), (10.0, -1.0)];
        let (a, b) = defense_span(&[(10.0, 50.0)], &line).unwrap();
        // perpendicular to the way north: the stroke runs east-west across the whole width
        assert!((a.1 - b.1).abs() < 1e-6, "{a:?} {b:?}");
        assert!(((a.0 - b.0).abs() - 20.0).abs() < 1e-6);
        // the army inside the line: the main spread of the points
        let (a, b) = defense_span(&[(10.0, 0.0)], &line).unwrap();
        assert!(((a.0 - b.0).abs() - 20.0).abs() < 1e-6, "{a:?} {b:?}");
        assert!(defense_span(&[], &[(1.0, 1.0)]).is_none());
    }
    #[test]
    fn unassigned_divisions_sharing_a_province_go_to_the_army_standing_with_them() {
        // n1, 16 Nov 2026: six Eritrea divisions shared provinces with Army 1 while the plan sent
        // them to Army 2 (more room); every selection was refused for 230 days.
        let mut v = assign_view();
        v["unassigned_by_state"][0]["shared_with"] = json!({"Army 1": 4});
        let plan = plan_assign(&v, 64, None, ARMY_CAP).unwrap();
        assert_eq!(
            plan.mode,
            AssignMode::Absorb {
                army: "Army 1".into()
            }
        );
        assert_eq!((plan.take, plan.overflow), (5, 1));
        // a full sharing army does not block the others
        v["armies"][0]["division_count"] = json!(24);
        let plan = plan_assign(&v, 64, None, ARMY_CAP).unwrap();
        assert_eq!(
            plan.mode,
            AssignMode::Absorb {
                army: "Army 2".into()
            }
        );
        assert_eq!(shared_with(&v, 64, "Army 1"), 4);
        assert_eq!(shared_with(&v, 64, "Army 2"), 0);
    }
    #[test]
    fn a_refused_selection_names_the_armies_sharing_the_provinces() {
        let mut v = assign_view();
        v["unassigned_by_state"][0]["shared_with"] = json!({"Army 1": 4, "Army 2": 1});
        let e = sharing_hint(
            Error::new("selection_unavailable", "None of the 6 wanted divisions."),
            &v,
            64,
            "Army 2",
        );
        assert_eq!(e.code, "selection_unavailable");
        assert!(
            e.message.contains("share provinces with Army 1 (4)"),
            "{}",
            e.message
        );
        assert!(!e.message.contains("Army 2 (1)"));
        let e = sharing_hint(Error::new("army_full", "x"), &v, 64, "Army 2");
        assert_eq!(e.message, "x");
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
    #[test]
    fn umlauts_are_dead_keys_and_match_by_length() {
        use super::super::designer::{dead_key_for, typable, typed_matches};
        assert!(typable("Armee Süd"));
        assert!(!typable("Étna"));
        assert_eq!(dead_key_for('ü'), Some((32, Some((32, false)))));
        assert_eq!(dead_key_for('Ö'), Some((32, Some((31, true)))));
        assert_eq!(dead_key_for('ß'), Some((1, None)));
        assert!(typed_matches("Armee Süd", "Armee Süd"));
        assert!(typed_matches("Armee S\u{fffd}d", "Armee Süd"));
        assert!(!typed_matches("Armee Sxd", "Armee Sud"));
        assert!(!typed_matches("Armee S", "Armee Sü"));
    }

    #[test]
    fn a_rename_is_refused_before_input_for_untypable_long_or_taken_names() {
        let view = json!({"armies": [{"army": "Army 1"}, {"army": "Army 2"}],
            "army_groups": [{"army_group": "Army Group 1"}]});
        assert!(rename_refusal(&view, "Army 1", "North_strike").is_none());
        // umlauts and the sharp s are typed through the layout's dead keys
        assert!(rename_refusal(&view, "Army 1", "Armee Süd").is_none());
        assert!(rename_refusal(&view, "Army 1", "Ätna Straße").is_none());
        // its own name again is no refusal (nothing to type)
        assert!(rename_refusal(&view, "Army 1", "Army 1").is_none());
        for bad in [
            "",
            " North",
            "North ",
            "Étna",
            "North;strike",
            "abcdefghijklmnopqrstuvwxy",
        ] {
            assert_eq!(
                rename_refusal(&view, "Army 1", bad).map(|e| e.code),
                Some("name_not_typable"),
                "{bad:?}"
            );
        }
        for taken in ["Army 2", "Army Group 1"] {
            assert_eq!(
                rename_refusal(&view, "Army 1", taken).map(|e| e.code),
                Some("name_taken")
            );
        }
    }
}
