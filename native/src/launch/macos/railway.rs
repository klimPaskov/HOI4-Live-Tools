//! `hoi4_railway_build`: queue one railway with the construction panel's railway mode.
//!
//! The game's own route (railway mode tooltip, 1.19.4.1, read live 6 Nov 2026): Shift+Click on a
//! node (a supply hub, a naval base or the capital) and click the target province: the game builds
//! the shortest path, re-using existing railways. Start and target come from the game's (or a
//! mod's) map files: `map/supply_nodes.txt`, the state history's naval bases and
//! `map/unitstacks.txt` positions, projected through the live camera. Verified by the own
//! construction queue holding more lines than before (railway lines carry a path, not a state
//! building, so the line count is the evidence).
use super::mapdata;
use super::ui::{clear_notices, find_state, read};
use crate::{
    error::{Error, Result},
    platform::{game_input::GameInput, Process},
};
use serde_json::{json, Value};
use std::collections::{HashMap, HashSet, VecDeque};
use std::path::Path;
use std::thread;
use std::time::{Duration, Instant};

const GAME: (u32, u32) = (1920, 1080);
/// Left edge of the open map while the construction panel is shown (game points).
const MAP_LEFT: f64 = 740.0;

pub(super) fn tool() -> Value {
    json!({"name":"hoi4_railway_build","description":"Queue ONE railway with the game's railway construction mode: Shift+Click on a start node (a supply hub, naval base or the capital; the game builds the shortest path re-using existing railways) and a click on the target province. Give the start as `from_province` or `from_state` (its supply hub or naval base from the map files) and the target as `to_province` or `to_state` (its supply hub, else its largest victory point). Both ends are checked live before any input: the start must be an own railway node now (a supply hub or naval base read from the game, or the capital) and both ends own-controlled. `from_nearest_node:true` replaces a missing start, or one that is no own node, with the own railway node the network already feeds (capital, port, hub with a railway) nearest to the target over own-controlled land (the game re-uses existing railways, so the line is the same); the result says start_replaced. `to_nearest_node:true` makes the target the own railway node nearest to the given province (a railway feeds a hub, the hub feeds the provinces around it; a railway to a plain province only helps as part of the way to a hub); the result says target_node_for_province. The receipt is journaled under request_id (Verified when the own construction queue holds one more line or one more railway level; a retry of the same id settles, it never clicks again). Typed refusals before input: not_a_railway_node and end_not_controlled (both name the nearest own node), no_node_reachable (no own node reaches the target over own land: use a port, trucks or a hub), no_node_in_state, province_not_in_state, unknown_state. railway_not_queued after the clicks means the game refused the path (often: it already stands or is queued at its highest level). Railways cost 170 per level (+130 per existing level) and are the way to carry supply from the capital to far fronts.","inputSchema":{"type":"object","properties":{"from_province":{"type":"integer","minimum":1,"maximum":99999},"from_state":{"type":"integer","minimum":1,"maximum":4095},"to_province":{"type":"integer","minimum":1,"maximum":99999},"to_state":{"type":"integer","minimum":1,"maximum":4095},"from_nearest_node":{"type":"boolean"},"to_nearest_node":{"type":"boolean"},"request_id":{"type":"string"},"lease_token":{"type":"string"}},"required":["request_id","lease_token"],"additionalProperties":false}})
}

/// Supply hub provinces listed in `map/supply_nodes.txt` (`level province` per line).
pub(super) fn parse_supply_nodes(text: &str) -> Vec<i32> {
    text.lines()
        .filter_map(|l| {
            let mut it = l.split_whitespace();
            let _level = it.next()?;
            it.next()?.parse::<i32>().ok()
        })
        .collect()
}
/// Provinces holding a naval base in a state history file (`PROVINCE = { naval_base = N }`).
pub(super) fn parse_naval_base_provinces(text: &str) -> Vec<i32> {
    let mut out = Vec::new();
    let mut last_number: Option<i32> = None;
    for token in text
        .split(|c: char| c.is_whitespace() || c == '=' || c == '{' || c == '}')
        .filter(|t| !t.is_empty())
    {
        if token == "naval_base" {
            if let Some(p) = last_number.take() {
                if p > 0 && !out.contains(&p) {
                    out.push(p);
                }
            }
        } else if let Ok(n) = token.parse::<i32>() {
            last_number = Some(n);
        } else {
            last_number = None;
        }
    }
    out
}
/// Victory point provinces of a state history file, largest value first.
pub(super) fn parse_victory_points(text: &str) -> Vec<(i32, f64)> {
    let mut out = Vec::new();
    let mut rest = text;
    while let Some(at) = rest.find("victory_points") {
        rest = &rest[at + "victory_points".len()..];
        let Some(open) = rest.find('{') else { break };
        let Some(close) = rest[open..].find('}') else {
            break;
        };
        let inner = &rest[open + 1..open + close];
        // Comments (`#Massaua`) run to the end of their line.
        let nums: Vec<&str> = inner
            .lines()
            .map(|l| l.split('#').next().unwrap_or(""))
            .flat_map(str::split_whitespace)
            .collect();
        if nums.len() >= 2 {
            if let (Ok(p), Ok(v)) = (nums[0].parse::<i32>(), nums[1].parse::<f64>()) {
                out.push((p, v));
            }
        }
        rest = &rest[open + close..];
    }
    out.sort_by(|a, b| b.1.total_cmp(&a.1));
    out
}

fn state_text(dir: &Path, state: i32) -> Result<String> {
    let prefix = format!("{state}-");
    let entries = std::fs::read_dir(dir.join("history/states"))
        .map_err(|_| Error::new("map_data_unavailable", "State history unreadable"))?;
    for entry in entries.flatten() {
        if entry.file_name().to_string_lossy().starts_with(&prefix) {
            return std::fs::read_to_string(entry.path())
                .map_err(|_| Error::new("map_data_unavailable", "State history unreadable"));
        }
    }
    Err(Error::new(
        "unknown_state",
        format!("No state {state} in the map files"),
    ))
}

/// Start node of a state: a supply hub listed for one of its provinces, else a naval base.
fn node_of_state(dir: &Path, state: i32) -> Result<i32> {
    let provinces = mapdata::state_provinces(dir, state)?;
    let hubs = std::fs::read_to_string(dir.join("map/supply_nodes.txt"))
        .map(|t| parse_supply_nodes(&t))
        .unwrap_or_default();
    if let Some(p) = provinces.iter().find(|p| hubs.contains(p)) {
        return Ok(*p);
    }
    if let Some(p) = parse_naval_base_provinces(&state_text(dir, state)?).first() {
        return Ok(*p);
    }
    Err(Error::new(
        "no_node_in_state",
        format!(
            "State {state} has no supply hub or naval base in the map files to start a railway from. \
             Start from a state that has one (a port or an older hub further back, e.g. the colony's \
             port): the game builds the shortest path and re-uses existing railways, so a railway from \
             the port to the front state goes through this one. Or pass from_province (a hub built \
             during the game) or build a supply hub here first (hoi4_construction_add supply_node)"
        ),
    ))
}
/// Target province of a state: its supply hub, else its largest victory point, else the first.
fn target_of_state(dir: &Path, state: i32) -> Result<i32> {
    let provinces = mapdata::state_provinces(dir, state)?;
    let hubs = std::fs::read_to_string(dir.join("map/supply_nodes.txt"))
        .map(|t| parse_supply_nodes(&t))
        .unwrap_or_default();
    if let Some(p) = provinces.iter().find(|p| hubs.contains(p)) {
        return Ok(*p);
    }
    let text = state_text(dir, state)?;
    if let Some((p, _)) = parse_victory_points(&text)
        .into_iter()
        .find(|(p, _)| provinces.contains(p))
    {
        return Ok(p);
    }
    provinces
        .first()
        .copied()
        .ok_or_else(|| Error::new("unknown_state", format!("State {state} lists no provinces")))
}
fn state_of(dir: &Path, province: i32) -> Result<i32> {
    super::wars::province_states(dir)?
        .get(&province)
        .copied()
        .ok_or_else(|| {
            Error::new(
                "province_not_in_state",
                format!("Province {province} belongs to no state in the map files"),
            )
        })
}
/// (province, its state) for one end from the arguments.
fn end(dir: &Path, province: Option<i32>, state: Option<i32>, start: bool) -> Result<(i32, i32)> {
    match (province, state) {
        (Some(p), Some(s)) => {
            if !mapdata::state_provinces(dir, s)?.contains(&p) {
                return Err(Error::new(
                    "province_not_in_state",
                    format!("Province {p} is not in state {s}"),
                ));
            }
            Ok((p, s))
        }
        (Some(p), None) => Ok((p, state_of(dir, p)?)),
        (None, Some(s)) => Ok((
            if start {
                node_of_state(dir, s)?
            } else {
                target_of_state(dir, s)?
            },
            s,
        )),
        (None, None) => Err(Error::new(
            "invalid_arguments",
            if start {
                "Give the start as from_province or from_state"
            } else {
                "Give the target as to_province or to_state"
            },
        )),
    }
}

/// Pan the camera with the arrow keys until `province` projects on screen (at most 24 taps).
fn pan_to(
    process: &dyn Process,
    keys: &dyn GameInput,
    dir: &Path,
    province: i32,
) -> Result<(f64, f64)> {
    use crate::platform::game_input::vk;
    let positions = mapdata::positions(dir)?;
    let &(x, z) = positions.get(&province).ok_or_else(|| {
        Error::new(
            "map_data_unavailable",
            format!("No map position for province {province}"),
        )
    })?;
    // A far target (Eritrea to Wello, live 6 Nov 2026) is out of reach of a few taps: the
    // wheel zooms out first (up to three steps; zooming keeps the railway in progress), and a
    // point behind the camera means the target lies below the screen.
    let mut zooms = 0;
    for _ in 0..48 {
        let p = read(process, |r, i| super::screenmap::to_screen(r, i, x, z))?;
        let Some((sx, sy)) = p else {
            keys.press_code(vk::DOWN)?;
            thread::sleep(Duration::from_millis(220));
            continue;
        };
        if (MAP_LEFT..1700.0).contains(&sx) && (120.0..900.0).contains(&sy) {
            return Ok((sx, sy));
        }
        let far = (sx - 960.0).abs() > 1400.0 || (sy - 540.0).abs() > 900.0;
        if far && zooms < 3 {
            keys.scroll(GAME, (1100.0, 540.0), -120)?;
            zooms += 1;
            thread::sleep(Duration::from_millis(400));
            continue;
        }
        if sx < MAP_LEFT {
            keys.press_code(vk::LEFT)?;
        } else if sx >= 1700.0 {
            keys.press_code(vk::RIGHT)?;
        }
        if sy < 120.0 {
            keys.press_code(vk::UP)?;
        } else if sy >= 900.0 {
            keys.press_code(vk::DOWN)?;
        }
        thread::sleep(Duration::from_millis(220));
    }
    Err(Error::new(
        "gui_target_unavailable",
        format!("Province {province} could not be brought on screen with the arrow keys"),
    ))
}
/// A screen point (game pixels).
type Point = (f64, f64);
/// Screen points of both railway ends once both lie in the open map, bringing them there with
/// at most one Find (of the start state) and one zoom-out burst; None when they still do not
/// fit (the caller pans with the arrow keys).
fn aim_both(
    process: &dyn Process,
    keys: &dyn GameInput,
    dir: &Path,
    from_state: i32,
    from: i32,
    to: i32,
) -> Result<Option<(Point, Point)>> {
    let positions = mapdata::positions(dir)?;
    let (Some(&a), Some(&b)) = (positions.get(&from), positions.get(&to)) else {
        return Ok(None);
    };
    let area = (MAP_LEFT, 120.0, 1700.0, 900.0);
    let inside = |p: (f64, f64)| (area.0..area.2).contains(&p.0) && (area.1..area.3).contains(&p.1);
    let project = || -> Result<(Option<Point>, Option<Point>)> {
        read(process, |r, i| {
            Ok((
                super::screenmap::to_screen(r, i, a.0, a.1)?,
                super::screenmap::to_screen(r, i, b.0, b.1)?,
            ))
        })
    };
    let both = |pa: Option<(f64, f64)>, pb: Option<(f64, f64)>| match (pa, pb) {
        (Some(x), Some(y)) if inside(x) && inside(y) => Some((x, y)),
        _ => None,
    };
    let (pa, pb) = project()?;
    if super::screenmap::aim_height_ok(process) {
        if let Some(found) = both(pa, pb) {
            return Ok(Some(found));
        }
    }
    find_state(process, keys, from_state)?;
    let mut left = 24;
    for _ in 0..2 {
        let (pa, pb) = project()?;
        if let Some(found) = both(pa, pb) {
            return Ok(Some(found));
        }
        let need = |p: Option<(f64, f64)>| match p {
            Some(p) => super::glide::zoom_out_events(p, area, left),
            None => left.min(6),
        };
        let events = need(pa).max(need(pb));
        if events == 0 || left == 0 {
            break;
        }
        keys.scroll_burst(GAME, (960.0, 540.0), -240, events)?;
        thread::sleep(Duration::from_millis(80));
        super::screenmap::wait_camera_still(process, 700);
        left = left.saturating_sub(events);
    }
    let (pa, pb) = project()?;
    Ok(both(pa, pb))
}
/// Queue lines plus the levels queued on railway lines: a railway along a path already in the
/// queue raises that line's level count instead of adding a line (live 6 Nov 2026: four
/// Asmara-Tigray orders made one line of 4 levels, and the line count never moved).
fn queue_lines(process: &dyn Process) -> Result<i64> {
    // The queue is rewritten on game days; a torn snapshot is read again.
    let mut attempt = 0;
    loop {
        match queue_work(process) {
            Err(e) if e.code == "torn_read" && attempt < 8 => {
                attempt += 1;
                thread::sleep(Duration::from_millis(15 * attempt));
            }
            result => return result,
        }
    }
}
fn queue_work(process: &dyn Process) -> Result<i64> {
    let lines = crate::economy::snapshot(process)?["construction_projects_queued"]
        .as_i64()
        .ok_or_else(|| Error::new("construction_target_unavailable", "Queue lines unreadable"))?;
    let page = crate::construction::page(process, 0, 128)?;
    Ok(lines + railway_levels(&page))
}
pub(super) fn railway_levels(page: &Value) -> i64 {
    page["lines"]
        .as_array()
        .map(|rows| {
            rows.iter()
                .filter(|r| r["line_kind"] == "railway")
                .filter_map(|r| r["amount_raw_signed"].as_i64())
                .sum()
        })
        .unwrap_or(0)
}

/// One railway order with both ends resolved and checked against the live game (no input).
pub(super) struct Route {
    dir: std::path::PathBuf,
    pub from_province: i32,
    pub from_state: i32,
    pub to_province: i32,
    pub to_state: i32,
    /// The start the caller asked for when the nearest own railway node replaced it.
    pub replaced: Option<Value>,
    /// The province the caller asked for when its nearest own node became the target.
    pub target_asked: Option<i32>,
}
impl Route {
    pub(super) fn json(&self) -> Value {
        let mut out = json!({"from_province":self.from_province,"from_state":self.from_state,
            "to_province":self.to_province,"to_state":self.to_state});
        if let Some(asked) = &self.replaced {
            out["start_replaced"] = asked.clone();
        }
        if let Some(p) = self.target_asked {
            out["target_node_for_province"] = json!(p);
        }
        out
    }
}

/// The nearest province to `from` in land steps that `node` accepts, walking only through
/// provinces `own` accepts, at most `limit` provinces looked at. `from` is never chosen.
/// (province, steps).
pub(super) fn nearest_node(
    neighbours: &HashMap<i32, Vec<i32>>,
    from: i32,
    mut own: impl FnMut(i32) -> Result<bool>,
    mut node: impl FnMut(i32) -> Result<bool>,
    limit: usize,
) -> Result<Option<(i32, usize)>> {
    let mut seen: HashSet<i32> = HashSet::from([from]);
    let mut queue: VecDeque<(i32, usize)> = VecDeque::from([(from, 0)]);
    while let Some((p, d)) = queue.pop_front() {
        if p != from && node(p)? {
            return Ok(Some((p, d)));
        }
        if seen.len() >= limit {
            continue;
        }
        for &q in neighbours.get(&p).map(Vec::as_slice).unwrap_or(&[]) {
            if seen.insert(q) && own(q)? {
                queue.push_back((q, d + 1));
            }
        }
    }
    Ok(None)
}

/// Provinces looked at when searching the nearest own railway node.
const NODE_SEARCH_LIMIT: usize = 4000;

/// Live facts the railway order depends on: the player's tag, the controller of a province and
/// whether it is a railway node now (a supply hub or naval base on the map, or the capital).
struct Live<'a> {
    r: &'a crate::memory::Reader<'a>,
    image: u64,
    sys: super::supplymem::Systems,
    me: String,
    capital: Option<i32>,
}
impl Live<'_> {
    fn own(&self, p: i32) -> Result<bool> {
        Ok(
            super::states::province_controllers(self.r, self.image, &[p])?
                .first()
                .is_some_and(|(_, c)| *c == self.me),
        )
    }
    /// A railway may start here: a supply hub, a naval base, or the capital.
    fn node(&self, p: i32) -> Result<bool> {
        if self.capital == Some(p) {
            return Ok(true);
        }
        Ok(
            super::supplymem::province_rail(self.r, self.image, &self.sys, p)?
                .is_some_and(|x| x.supply_hub || x.naval_base_level > 0),
        )
    }
    /// A node that already gets supply from the network: the capital, a port (by sea) or a hub
    /// with a railway (a hub without one, e.g. a captured one, waits for its railway).
    fn fed(&self, p: i32) -> Result<bool> {
        if self.capital == Some(p) {
            return Ok(true);
        }
        Ok(
            super::supplymem::province_rail(self.r, self.image, &self.sys, p)?
                .is_some_and(|x| x.naval_base_level > 0 || (x.supply_hub && x.rail_level > 0)),
        )
    }
}
fn live<T>(process: &dyn Process, dir: &Path, f: impl Fn(&Live<'_>) -> Result<T>) -> Result<T> {
    let capitals = super::front_plan::read_capitals(dir);
    read(process, |r, image| {
        let g = crate::game::layout(r);
        let gs = r.u64(image + g.gs_global)?;
        let me = super::states::tag_name(r, image, r.i32(gs + g.player_tag)?)?;
        let capital = capitals
            .get(&me)
            .and_then(|s| target_of_state(dir, *s).ok());
        let sys = super::supplymem::systems(r, image, super::supplymem::game_state(r, image)?)?;
        f(&Live {
            r,
            image,
            sys,
            me,
            capital,
        })
    })
}

/// Resolve and check both ends before any input. The game starts a railway only on a railway
/// node (a supply hub, a naval base or the capital) and lays it through own-controlled land, so
/// a start that is no node, an end the player does not control, or a target no own node reaches
/// is refused here with a typed code instead of after a click the game ignores. With
/// `from_nearest` a start that is missing or no node is replaced by the own node nearest to the
/// target over own-controlled land (the game re-uses existing railways on the way, so a farther
/// start still joins the same line); the replacement is a node the network already feeds (the
/// capital, a port, a hub with a railway). With `to_node` the target becomes the own railway
/// node nearest to the given province (the province itself when it is one): a railway feeds a
/// hub, and the hub feeds the provinces around it.
pub(super) fn resolve(
    process: &dyn Process,
    from: (Option<i32>, Option<i32>),
    to: (Option<i32>, Option<i32>),
    from_nearest: bool,
    to_node: bool,
) -> Result<Route> {
    let dir = process
        .identity()
        .and_then(|i| mapdata::game_dir(&i.exe).ok())
        .ok_or_else(|| Error::new("map_data_unavailable", "Game install not found"))?;
    let (asked_p, asked_s) = end(&dir, to.0, to.1, false)?;
    let asked = match (from, from_nearest) {
        ((None, None), true) => None,
        (f, _) => match end(&dir, f.0, f.1, true) {
            Ok(x) => Some(x),
            Err(e) if from_nearest && e.code == "no_node_in_state" => None,
            Err(e) => return Err(e),
        },
    };
    let neighbours = super::wars::province_neighbours(&dir)?;
    let (to_p, to_own, from_ok, found) = live(process, &dir, |l| {
        if !l.own(asked_p)? {
            return Ok((asked_p, false, None, None));
        }
        let to_p = if to_node && !l.node(asked_p)? {
            nearest_node(
                &neighbours,
                asked_p,
                |p| l.own(p),
                |p| l.node(p),
                NODE_SEARCH_LIMIT,
            )?
            .map_or(asked_p, |(p, _)| p)
        } else {
            asked_p
        };
        let from_ok = match asked {
            Some((p, _)) => Some((l.own(p)?, l.node(p)?)),
            None => None,
        };
        let found = nearest_node(
            &neighbours,
            to_p,
            |p| l.own(p),
            |p| l.fed(p),
            NODE_SEARCH_LIMIT,
        )?;
        Ok((to_p, true, from_ok, found))
    })?;
    let to_s = if to_p == asked_p {
        asked_s
    } else {
        state_of(&dir, to_p)?
    };
    if !to_own {
        return Err(Error::new(
            "end_not_controlled",
            format!(
                "Target province {to_p} (state {to_s}) is not controlled by the player: a railway \
                 runs only through own-controlled land. Nothing was clicked"
            ),
        ));
    }
    let hint = found.map_or_else(
        || "no own railway node the network feeds reaches it over own-controlled land".to_string(),
        |(p, d)| {
            format!(
                "the nearest own railway node the network feeds (capital, port, hub with a \
                 railway) over own-controlled land is province {p} ({d} province step(s) from \
                 the target)"
            )
        },
    );
    let mut replaced = None;
    let (from_p, from_s) = match (asked, from_ok) {
        (Some((p, s)), Some((true, true))) => (p, s),
        (Some((p, s)), Some((own, _))) if !from_nearest => {
            let (code, why) = if own {
                (
                    "not_a_railway_node",
                    format!(
                        "province {p} (state {s}) is no railway node now (no supply hub, no \
                         naval base, not the capital)"
                    ),
                )
            } else {
                (
                    "end_not_controlled",
                    format!("start province {p} (state {s}) is not controlled by the player"),
                )
            };
            return Err(Error::new(
                code,
                format!(
                    "The game starts a railway only on an own railway node: {why}; {hint}. Start \
                     there (from_province), or pass from_nearest_node:true. Nothing was clicked"
                ),
            ));
        }
        (asked, _) => {
            let Some((p, _)) = found else {
                return Err(Error::new(
                    "no_node_reachable",
                    format!(
                        "No own railway node (supply hub, naval base or capital) reaches province \
                         {to_p} (state {to_s}) over own-controlled land: no railway can be laid \
                         there now. Supply it by a port, trucks (motorization) or a supply hub, \
                         or take the land between. Nothing was clicked"
                    ),
                ));
            };
            replaced = Some(asked.map_or(
                Value::Null,
                |(p, s)| json!({"from_province":p,"from_state":s}),
            ));
            (p, state_of(&dir, p)?)
        }
    };
    Ok(Route {
        dir,
        from_province: from_p,
        from_state: from_s,
        to_province: to_p,
        to_state: to_s,
        replaced,
        target_asked: (to_p != asked_p).then_some(asked_p),
    })
}

/// Queue work now: construction lines plus the levels queued on railway lines (the readback
/// and the receipt baseline of hoi4_railway_build).
pub(super) fn queue_work_now(process: &dyn Process) -> Result<i64> {
    queue_lines(process)
}

/// Click the railway in: Shift+Click on the start node, a click on the target, then wait up to
/// 5 s for the own queue to hold more work than `before`. Ok with the readback; Err
/// `railway_not_queued` when nothing more was queued (the game refused the path).
pub(super) fn dispatch(
    process: &dyn Process,
    keys: &dyn GameInput,
    route: &Route,
    before: i64,
    clicked: &std::cell::Cell<bool>,
) -> Result<Value> {
    let dir = &route.dir;
    let (from_p, from_s, to_p) = (route.from_province, route.from_state, route.to_province);
    clear_notices(process, keys)?;
    super::construction::select_building(process, keys, "rail_way")?;
    let result = (|| -> Result<Value> {
        // Both ends on screen at once: no Find when they already are, otherwise Find the start
        // and zoom out in one burst until the end shows too (the arrow-key crawl took 10-14 s
        // per railway in the Ethiopia runs). Zooming keeps a railway in progress.
        let ends = aim_both(process, keys, dir, from_s, from_p, to_p)?;
        let (x, y) = match ends {
            Some((a, _)) => a,
            // The construction panel covers the left of the screen: both ends are clicked in
            // the open map to its right (a Shift+Click on Asmara landed on the panel, 6 Nov 2026).
            None => pan_to(process, keys, dir, from_p)?,
        };
        // The game runs while the camera moves: an event that opened meanwhile would take the
        // click (live 10 Nov 2026: a news window over Mogadishu during the aim).
        no_event_open(process)?;
        clicked.set(true);
        keys.shift_click(GAME, x, y)?;
        thread::sleep(Duration::from_millis(120));
        // The Find view would end the railway in progress (live 6 Nov 2026: 'Railway must start
        // on a supply node'), so a target still off screen is reached with the arrow keys.
        let (x, y) = match ends {
            Some((_, b)) => b,
            None => pan_to(process, keys, dir, to_p)?,
        };
        keys.click(GAME, x, y)?;
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            let now = queue_lines(process)?;
            if now > before {
                let mut out = route.json();
                out["queue_work_before"] = json!(before);
                out["queue_work_after"] = json!(now);
                out["note"] = json!("queue work = construction lines + railway levels queued");
                return Ok(out);
            }
            if Instant::now() >= deadline {
                let levels = rail_levels(process, dir, from_p, to_p).map_or_else(
                    |_| String::new(),
                    |(a, b)| format!(" Railway level now: {a} at the start, {b} at the target."),
                );
                return Err(Error::new(
                    "railway_not_queued",
                    format!(
                        "Shift+Click on own railway node {from_p} and a click on own province \
                         {to_p} queued no railway within 5 s (no new line, no new railway \
                         level).{levels} Both ends were checked live before the clicks, so the \
                         game refused the path: the railway on it may already stand or be \
                         queued at its highest level (5), or no path joins them through \
                         own-controlled land; nothing was retried"
                    ),
                ));
            }
            thread::sleep(Duration::from_millis(60));
        }
    })();
    super::construction::leave(process, keys);
    result
}

/// `events_open` (nothing clicked on the map yet) while an event window is open.
fn no_event_open(process: &dyn Process) -> Result<()> {
    let open = read(process, super::events::open_ids)?;
    if open.is_empty() {
        return Ok(());
    }
    Err(Error::new(
        "events_open",
        format!(
            "An event opened while the camera moved ({}); no railway click was sent",
            open.join(", ")
        ),
    ))
}
/// Railway level of two provinces now.
fn rail_levels(process: &dyn Process, dir: &Path, a: i32, b: i32) -> Result<(u32, u32)> {
    live(process, dir, |l| {
        let level = |p: i32| -> Result<u32> {
            Ok(super::supplymem::province_rail(l.r, l.image, &l.sys, p)?
                .map_or(0, |x| x.rail_level))
        };
        Ok((level(a)?, level(b)?))
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn railway_levels_sum_only_railway_lines() {
        let page = json!({"lines":[{"line_kind":"building","amount_raw_signed":1},
            {"line_kind":"railway","amount_raw_signed":4},{"line_kind":"railway","amount_raw_signed":2}]});
        assert_eq!(railway_levels(&page), 6);
        assert_eq!(railway_levels(&json!({})), 0);
    }
    #[test]
    fn nearest_node_walks_own_land_only_and_never_picks_the_start() {
        // 1 - 2 - 3 (node) and 1 - 4 (enemy) - 5 (node, nearer by straight line)
        let n: HashMap<i32, Vec<i32>> = HashMap::from([
            (1, vec![2, 4]),
            (2, vec![1, 3]),
            (3, vec![2]),
            (4, vec![1, 5]),
            (5, vec![4]),
        ]);
        let own = |p: i32| Ok(p != 4);
        let node = |p: i32| Ok(p == 3 || p == 5 || p == 1);
        assert_eq!(nearest_node(&n, 1, own, node, 100).unwrap(), Some((3, 2)));
        // nothing reachable over own land
        let enemy = |p: i32| Ok(p == 1);
        assert_eq!(nearest_node(&n, 1, enemy, node, 100).unwrap(), None);
    }
    #[test]
    fn supply_nodes_file_lines_give_provinces() {
        assert_eq!(
            parse_supply_nodes("1 67 \n1 101 \n\n2 13250\n"),
            vec![67, 101, 13250]
        );
    }
    #[test]
    fn naval_bases_come_from_province_blocks_only() {
        let text = "buildings = {\n infrastructure = 2\n air_base = 3\n 12766 = {\n naval_base = 3\n }\n}\n";
        assert_eq!(parse_naval_base_provinces(text), vec![12766]);
    }
    #[test]
    fn victory_points_are_sorted_largest_first() {
        let text = "victory_points = { #Massaua\n 12766 1\n }\n victory_points = { 13250 3 }\n";
        assert_eq!(parse_victory_points(text), vec![(13250, 3.0), (12766, 1.0)]);
    }
}
