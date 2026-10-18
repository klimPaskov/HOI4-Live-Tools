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
};
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
                Ok(json!({"army":army_name(r, group)?,"division_count":ids.len(),"divisions":ids}))
            })
            .collect::<Result<Vec<_>>>()?;
        Ok(json!({"armies":armies,"unassigned":unassigned}))
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

/// Areas covered by the top bar, the left-hand panels and the card bar.
fn map_area(x: f64, y: f64) -> bool {
    (90.0..=900.0).contains(&y) && !(x < 300.0 && y < 340.0)
}
/// Front-line drawing mode marks the borders an order can follow with bright
/// green dashes; the target is the solid dash patch nearest the screen
/// centre, where the Find view has put the enemy state.
fn nearest_dash(capture: &Capture) -> Option<(f64, f64)> {
    let green = |x: f64, y: f64| {
        capture.rgb(x, y).is_some_and(|(r, g, b)| {
            g > 150 && i32::from(g) > i32::from(r) + 25 && i32::from(g) > i32::from(b) + 60
        })
    };
    let (w, h) = (
        capture.width as f64 / capture.scale,
        capture.height as f64 / capture.scale,
    );
    let mut best: Option<(f64, f64, f64)> = None;
    let mut y = 0.0;
    while y + 2.0 < h {
        let mut x = 0.0;
        while x + 2.0 < w {
            if map_area(x, y)
                && green(x, y)
                && (0..3).all(|dy| (0..3).all(|dx| green(x + f64::from(dx), y + f64::from(dy))))
            {
                let d = (x - 960.0).powi(2) + (y - 540.0).powi(2);
                if best.is_none_or(|b| d < b.0) {
                    best = Some((d, x + 1.0, y + 1.0));
                }
            }
            x += 1.0;
        }
        y += 1.0;
    }
    best.map(|b| (b.1, b.2))
}

/// Draw a front line for `army` along the border nearest state `state`: the
/// army card selects the army, Z enters front-line drawing, the Find view
/// centres the state and one click on the nearest highlighted border places
/// the order. Verified by one more battle-plan order on the army.
pub(super) fn front(process: &dyn Process, keys: &MacKeys, army: &str, state: i32) -> Result<()> {
    clear_notices(process, keys)?;
    let before = read(process, |r, i| orders(r, i, army))?;
    let card = read(process, |r, i| card_rect(r, i, army, "portrait_btn"))?;
    let (x, y) = card.center();
    keys.click(GAME, x, y)?;
    wait(process, "select army", |r, i| Ok(selection_size(r, i)? > 0))?;
    keys.press(Key::letter('z')?)?;
    std::thread::sleep(std::time::Duration::from_millis(200));
    find_state(process, keys, state)?;
    std::thread::sleep(std::time::Duration::from_millis(400));
    let capture = keys.capture(GAME)?;
    let Some((x, y)) = nearest_dash(&capture) else {
        return Err(Error::new(
            "gui_target_unavailable",
            "No front this army can follow is highlighted near the state",
        ));
    };
    keys.click(GAME, x, y)?;
    wait(process, "front line placed", |r, i| {
        Ok(orders(r, i, army)? > before)
    })
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

#[cfg(test)]
mod tests {
    use super::*;
    fn capture(green: &[(usize, usize)]) -> Capture {
        let (width, height) = (1920, 1080);
        let mut data = vec![40u8; width * height * 4];
        for &(x0, y0) in green {
            for y in y0..y0 + 4 {
                for x in x0..x0 + 4 {
                    let o = (y * width + x) * 4;
                    data[o..o + 3].copy_from_slice(&[60, 220, 90]);
                }
            }
        }
        Capture {
            width,
            height,
            stride: width * 4,
            scale: 1.0,
            data,
        }
    }
    #[test]
    fn nearest_dash_prefers_the_centre_and_skips_ui() {
        assert_eq!(nearest_dash(&capture(&[])), None);
        // Top bar, left panel and card bar are never targets.
        assert_eq!(
            nearest_dash(&capture(&[(960, 40), (100, 200), (960, 950)])),
            None
        );
        let found = nearest_dash(&capture(&[(500, 500), (1000, 560)])).unwrap();
        assert_eq!(found, (1001.0, 561.0));
    }
}
