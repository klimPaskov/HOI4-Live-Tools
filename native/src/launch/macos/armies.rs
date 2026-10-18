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
};
/// The army panel's commander portrait ("Click to assign") while an army is
/// selected, and the commander list's visible row band; portraits are drawn
/// 90 px tall from the bottom of their texture rectangle.
const COMMANDER_SLOT: (f64, f64) = (33.0, 160.0);
const LEADER_ROWS: (i32, i32) = (320, 810);
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
fn division_states(r: &Reader<'_>, image: u64) -> Result<BTreeMap<String, Option<i32>>> {
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
        let armies = groups
            .into_iter()
            .map(|(group, ids)| {
                let name = army_name(r, group)?;
                let leader = r.u64(group + ui(r)?.group_leader)?;
                let commander = if leader == 0 {
                    None
                } else {
                    Some(r.string_bounded(leader + ui(r)?.leader_name, 128)?)
                };
                let orders = r.array_header(group + ui(r)?.group_orders)?.2;
                let executing = executing(r, image, &name).ok();
                Ok(
                    json!({"army":name,"commander":commander,"battle_plan_orders":orders,
                    "plan_executing":executing,"division_count":ids.len(),"divisions":ids}),
                )
            })
            .collect::<Result<Vec<_>>>()?;
        let places = division_states(r, image)?;
        let mut by_state: BTreeMap<i32, usize> = BTreeMap::new();
        for id in &unassigned {
            if let Some(Some(state)) = places.get(id) {
                *by_state.entry(*state).or_default() += 1;
            }
        }
        let by_state: Vec<Value> = by_state
            .into_iter()
            .map(|(state, n)| json!({"state_id":state,"divisions":n}))
            .collect();
        Ok(json!({"armies":armies,"unassigned":unassigned,"unassigned_by_state":by_state}))
    })
}
fn selection_size(r: &Reader<'_>, image: u64) -> Result<u64> {
    let idler = r.u64(image + game::layout(r).idler_global)?;
    r.u64(idler + ui(r)?.selection_size)
}
/// The new-army card: the rightmost portrait button of the army card bar.
fn new_army_card(r: &Reader<'_>, image: u64) -> Result<Rect> {
    let u = ui(r)?;
    let idler = r.u64(image + game::layout(r).idler_global)?;
    let bar = r.u64(r.u64(idler + u.manager)? + u.leader_view)?;
    if r.u64(bar)? != image + u.leader_view_vtable {
        return Err(Error::new("invalid_gui", "Exact army card bar required"));
    }
    let (_, data, count) = r.array_header(bar + u.leader_portraits)?;
    let mut best: Option<Rect> = None;
    for index in 0..count.min(64) as u64 {
        let portrait = r.u64(data + index * 8)?;
        let Some(window) = gui_tree::container(r, r.u64(portrait + u.portrait_window)?, image)?
        else {
            continue;
        };
        if let Some(button) = gui_tree::child(r, image, window, "portrait_btn")? {
            if let Ok(rect) = gui_tree::rect(r, image, button, GAME) {
                if best.is_none_or(|b| rect.x > b.x) {
                    best = Some(rect);
                }
            }
        }
    }
    best.ok_or_else(|| Error::new("gui_target_unavailable", "New army card not found"))
}

/// Create an army from own divisions around state `state`.
pub(super) fn create(process: &dyn Process, keys: &MacKeys, state: i32) -> Result<()> {
    clear_notices(process, keys)?;
    // The selection box stays clear of the left-hand panels; Escape is not used
    // because on a free map it opens the game menu.
    find_state(process, keys, state)?;
    keys.drag(GAME, (560.0, 260.0), (1360.0, 860.0))?;
    wait(process, "select divisions", |r, i| {
        Ok(selection_size(r, i)? > 0)
    })?;
    let before = read(process, membership)?;
    let groups_before: BTreeSet<u64> = before.values().copied().collect();
    let card = read(process, new_army_card)?;
    clear_notices(process, keys)?;
    let (x, y) = card.center();
    keys.click(GAME, x, y)?;
    wait(process, "army created", |r, i| {
        Ok(membership(r, i)?
            .values()
            .any(|g| *g != 0 && !groups_before.contains(g)))
    })
    .map_err(|_| {
        Error::new(
            "input_readback_failed",
            "No new army: the divisions around this state may already belong to an army \
             (see hoi4_armies), or none are there",
        )
    })
}

/// The own orders group named `army` (names must be unique).
fn group_named(r: &Reader<'_>, image: u64, army: &str) -> Result<u64> {
    let groups: BTreeSet<u64> = membership(r, image)?
        .into_values()
        .filter(|g| *g != 0)
        .collect();
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
fn select_army(process: &dyn Process, keys: &MacKeys, army: &str) -> Result<Rect> {
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
fn leader_list_open(r: &Reader<'_>, image: u64) -> Result<bool> {
    gui_tree::visible(r, leader_root(r, image)?)
}
/// (general name, portrait rectangle) for each row of the commander list.
fn leader_rows(r: &Reader<'_>, image: u64) -> Result<Vec<(String, Rect)>> {
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
        if let Ok(rect) = gui_tree::rect_unclipped(r, image, button, GAME) {
            out.push((name, rect));
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
    clear_notices(process, keys)?;
    if read(process, |r, i| commander(r, i, army))?.is_some_and(|c| c.eq_ignore_ascii_case(general))
    {
        return Ok(());
    }
    let card = read(process, |r, i| card_rect(r, i, army, "portrait_btn"))?;
    let (x, y) = card.center();
    keys.click(GAME, x, y)?;
    wait(process, "select army", |r, i| Ok(selection_size(r, i)? > 0))?;
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
        return Err(Error::new(
            "invalid_target",
            format!("No such general in the list: {}", names.join(", ")),
        ));
    };
    for _ in 0..40 {
        let bottom = rect.y + rect.h;
        let top = bottom - PORTRAIT_DRAWN;
        if top >= LEADER_ROWS.0 && bottom <= LEADER_ROWS.1 {
            break;
        }
        if !read(process, leader_list_open)? {
            return Err(Error::new("input_readback_failed", "Commander list closed"));
        }
        let pixels = if top < LEADER_ROWS.0 { 240 } else { -240 };
        keys.scroll(GAME, (960.0, 560.0), pixels)?;
        rect = read(process, find)?
            .ok_or_else(|| Error::new("gui_target_unavailable", "General left the list"))?;
    }
    let rect = settled(process, "general row", |r, i| {
        find(r, i)?.ok_or_else(|| Error::new("gui_target_unavailable", "General left the list"))
    })?;
    let bottom = rect.y + rect.h;
    if bottom - PORTRAIT_DRAWN < LEADER_ROWS.0 || bottom > LEADER_ROWS.1 {
        return Err(Error::new(
            "gui_target_unavailable",
            "General row not in view",
        ));
    }
    keys.click(
        GAME,
        f64::from(rect.x) + 34.0,
        f64::from(bottom - PORTRAIT_DRAWN / 2),
    )?;
    wait(process, "commander assigned", |r, i| {
        Ok(commander(r, i, army)?.is_some_and(|c| c.to_lowercase() == wanted))
    })
}
