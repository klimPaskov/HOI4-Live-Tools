//! `hoi4_wars`, stage A: the player's land borders, occupied own states and
//! threatened own states, from data that is already validated on macOS plus
//! install map files. Pure read: no input, no lease, no cursor, no pause.
//!
//! Sources and their evidence:
//! - owner and controller of every state, and the tag table: the same reads
//!   as `hoi4_states` (`construction::MACOS`, `game::MACOS`), live validated.
//! - the state each own division stands in: `armies::division_states`, live
//!   validated.
//! - state adjacency: derived here from `map/provinces.bmp` (two land
//!   provinces touch where neighbouring pixels differ), `map/definition.csv`
//!   (colour to province id and land/sea/lake) and `history/states` (province
//!   lists). Vanilla install files only (mods are ignored); sea crossings
//!   (`map/adjacencies.csv`, type `sea`) are deliberately NOT land adjacency.
//!   Not yet compared with the live map (checks in the module notes below).
//!
//! What this tool does NOT know (reported, never guessed):
//! - war sides, leaders, war score and goal timers (`offset_not_validated`).
//!   Hostility is the war enemies, a country that controls an own-owned state
//!   (proxy), and tags the caller passes in `enemies`.
//! - whether any army holds a front-line order (order contents are unmapped).
//!   `covered` only says own divisions stand in the border states.
//!
//! Stage B (war relations) lives in `diplomacy_state.rs`: the offsets were
//! discovered by vtable scans on 25 Oct 2026 (CDiplomacyStatus at
//! `CCountry+0xd30`, relation list at `+0x20`, countries paired by relation
//! pointer identity). `wars` is the own-war list; hostility adds the war
//! enemies. Sides, leaders, war score and justification timers stay
//! `offset_not_validated`.
use super::ui::read;
use crate::{
    error::{Error, Result},
    game,
    memory::Reader,
    platform::Process,
};
use serde_json::{json, Map, Value};
use std::{
    collections::{BTreeMap, BTreeSet, HashMap},
    path::{Path, PathBuf},
    sync::{Arc, Mutex, OnceLock},
    time::Instant,
};

const DEFAULT_MAX_STATES: usize = 200;
const STATE_BUDGET: u64 = 4096;

type Adjacency = BTreeMap<i32, BTreeSet<i32>>;

// ---------------------------------------------------------------------------
// Map data: state adjacency from the install files.

/// `definition.csv` lines `id;r;g;b;type;...`: packed rgb to (id, is land).
fn parse_definition(text: &str) -> HashMap<u32, (i32, bool)> {
    let mut out = HashMap::new();
    for line in text.lines() {
        let f: Vec<&str> = line.split(';').collect();
        if f.len() < 5 {
            continue;
        }
        let (Ok(id), Ok(r), Ok(g), Ok(b)) = (
            f[0].trim().parse::<i32>(),
            f[1].trim().parse::<u32>(),
            f[2].trim().parse::<u32>(),
            f[3].trim().parse::<u32>(),
        ) else {
            continue;
        };
        if id <= 0 || r > 255 || g > 255 || b > 255 {
            continue;
        }
        out.insert((r << 16) | (g << 8) | b, (id, f[4].trim() == "land"));
    }
    out
}

/// Pairs of touching land provinces (4-neighbourhood) in a 24 bpp BMP.
fn land_edges(bmp: &[u8], defs: &HashMap<u32, (i32, bool)>) -> Result<BTreeSet<(i32, i32)>> {
    let bad = |m: &'static str| Error::new("map_data_unavailable", m);
    if bmp.len() < 54 || &bmp[..2] != b"BM" {
        return Err(bad("provinces.bmp is not a BMP"));
    }
    let le32 = |o: usize| u32::from_le_bytes(bmp[o..o + 4].try_into().unwrap());
    let offset = le32(10) as usize;
    let width = le32(18) as i32;
    let height = le32(22) as i32;
    let bpp = u16::from_le_bytes([bmp[28], bmp[29]]);
    if width <= 0 || height == 0 || bpp != 24 || le32(30) != 0 {
        return Err(bad("provinces.bmp must be uncompressed 24 bpp"));
    }
    let (w, h) = (width as usize, height.unsigned_abs() as usize);
    let stride = (w * 3).div_ceil(4) * 4;
    if offset.saturating_add(stride * h) > bmp.len() {
        return Err(bad("provinces.bmp is truncated"));
    }
    let pixel = |x: usize, row: &[u8]| {
        let o = x * 3;
        (u32::from(row[o + 2]) << 16) | (u32::from(row[o + 1]) << 8) | u32::from(row[o])
    };
    // Row order does not change adjacency, so bottom-up and top-down agree.
    let rows: Vec<&[u8]> = (0..h)
        .map(|y| &bmp[offset + y * stride..offset + y * stride + w * 3])
        .collect();
    let mut colour_pairs: BTreeSet<(u32, u32)> = BTreeSet::new();
    let mut last = (0u32, 0u32);
    let mut note = |a: u32, b: u32, set: &mut BTreeSet<(u32, u32)>| {
        let pair = if a < b { (a, b) } else { (b, a) };
        if pair != last {
            last = pair;
            set.insert(pair);
        }
    };
    for y in 0..h {
        let mut prev = pixel(0, rows[y]);
        for x in 0..w {
            let cur = pixel(x, rows[y]);
            if x > 0 && cur != prev {
                note(prev, cur, &mut colour_pairs);
            }
            if y > 0 {
                let above = pixel(x, rows[y - 1]);
                if above != cur {
                    note(above, cur, &mut colour_pairs);
                }
            }
            prev = cur;
        }
    }
    let mut out = BTreeSet::new();
    for (a, b) in colour_pairs {
        if let (Some(&(ia, true)), Some(&(ib, true))) = (defs.get(&a), defs.get(&b)) {
            out.insert(if ia < ib { (ia, ib) } else { (ib, ia) });
        }
    }
    Ok(out)
}

/// Province edges folded to state adjacency (provinces without a state or in
/// the same state are dropped).
fn fold_states(edges: &BTreeSet<(i32, i32)>, state_of: &HashMap<i32, i32>) -> Adjacency {
    let mut out: Adjacency = BTreeMap::new();
    for (a, b) in edges {
        if let (Some(&sa), Some(&sb)) = (state_of.get(a), state_of.get(b)) {
            if sa != sb {
                out.entry(sa).or_default().insert(sb);
                out.entry(sb).or_default().insert(sa);
            }
        }
    }
    out
}

fn province_states(dir: &Path) -> Result<HashMap<i32, i32>> {
    let entries = std::fs::read_dir(dir.join("history/states"))
        .map_err(|_| Error::new("map_data_unavailable", "State history unreadable"))?;
    let mut out = HashMap::new();
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().to_string();
        let Some(id) = name.split('-').next().and_then(|v| v.parse::<i32>().ok()) else {
            continue;
        };
        let Ok(text) = std::fs::read_to_string(entry.path()) else {
            continue;
        };
        for province in super::mapdata::parse_state_provinces(&text) {
            out.entry(province).or_insert(id);
        }
    }
    Ok(out)
}

fn build_adjacency(dir: &Path) -> Result<Adjacency> {
    let definition = std::fs::read_to_string(dir.join("map/definition.csv"))
        .map_err(|_| Error::new("map_data_unavailable", "definition.csv unreadable"))?;
    let bmp = std::fs::read(dir.join("map/provinces.bmp"))
        .map_err(|_| Error::new("map_data_unavailable", "provinces.bmp unreadable"))?;
    let edges = land_edges(&bmp, &parse_definition(&definition))?;
    Ok(fold_states(&edges, &province_states(dir)?))
}

/// State adjacency of the install, built once per install directory.
fn adjacency(dir: &Path) -> Result<Arc<Adjacency>> {
    static CACHE: OnceLock<Mutex<HashMap<PathBuf, Arc<Adjacency>>>> = OnceLock::new();
    let cache = CACHE.get_or_init(|| Mutex::new(HashMap::new()));
    if let Some(found) = cache.lock().unwrap().get(dir) {
        return Ok(found.clone());
    }
    let built = Arc::new(build_adjacency(dir)?);
    cache
        .lock()
        .unwrap()
        .insert(dir.to_path_buf(), built.clone());
    Ok(built)
}

// ---------------------------------------------------------------------------
// Memory snapshot (same validated reads as hoi4_states / hoi4_armies).

#[derive(Clone, Debug, PartialEq, Eq)]
struct StateRow {
    id: i32,
    owner: String,
    controller: String,
}

struct Snapshot {
    player: String,
    states: Vec<StateRow>,
    /// own divisions standing in each state
    divisions: BTreeMap<i32, usize>,
    /// own divisions with no known state (moving or unreadable)
    in_transit: usize,
}

fn snapshot(r: &Reader<'_>, image: u64) -> Result<Snapshot> {
    let g = game::layout(r);
    let l = crate::construction::layout(r);
    let gs = r.u64(image + g.gs_global)?;
    let player = super::states::tag_name(r, image, r.i32(gs + g.player_tag)?)?;
    let (_, data, count) = r.array_header(gs + l.gs_states)?;
    let mut tags: HashMap<i32, String> = HashMap::new();
    let mut tag = |index: i32| -> Result<String> {
        if index <= 0 {
            return Ok(String::new());
        }
        if let Some(found) = tags.get(&index) {
            return Ok(found.clone());
        }
        let name = super::states::tag_name(r, image, index)?;
        tags.insert(index, name.clone());
        Ok(name)
    };
    let mut states = Vec::new();
    for index in 1..(count as u64).min(STATE_BUDGET) {
        let state = r.u64(data + index * 8)?;
        if state == 0 || r.u64(state)? != image + l.state_vtable {
            continue;
        }
        states.push(StateRow {
            id: r.i32(state + l.state_id)?,
            owner: tag(r.i32(state + l.state_owner)?)?,
            controller: tag(r.i32(state + l.state_controller)?)?,
        });
    }
    let mut divisions = BTreeMap::new();
    let mut in_transit = 0;
    for place in super::armies::division_states(r, image)?.into_values() {
        match place {
            Some(state) => *divisions.entry(state).or_insert(0) += 1,
            None => in_transit += 1,
        }
    }
    Ok(Snapshot {
        player,
        states,
        divisions,
        in_transit,
    })
}

// ---------------------------------------------------------------------------
// Pure analysis.

struct Input<'a> {
    player: &'a str,
    states: &'a [StateRow],
    divisions: &'a BTreeMap<i32, usize>,
    in_transit: usize,
    /// `Err(reason)` when the map graph could not be built
    adjacency: std::result::Result<&'a Adjacency, String>,
    names: &'a HashMap<i32, String>,
    caller_enemies: &'a BTreeSet<String>,
    /// enemies from war relations (Stage B), `None` when unreadable
    war_enemies: Option<&'a BTreeSet<String>>,
    /// the Stage B `wars` value, or the reason it is missing
    wars: std::result::Result<&'a Value, String>,
    max_states: usize,
}

fn name_of(names: &HashMap<i32, String>, id: i32) -> Value {
    names.get(&id).map_or(Value::Null, |n| json!(n))
}

fn capped(mut rows: Vec<Value>, max: usize) -> (Vec<Value>, bool) {
    let truncated = rows.len() > max;
    rows.truncate(max);
    (rows, truncated)
}

fn alert(code: &str, severity: &str, message: String, tool: &str, args_hint: Value) -> Value {
    json!({"code":code,"severity":severity,"message":message,
        "fix":{"tool":tool,"args_hint":args_hint}})
}

fn ids(list: &[i32]) -> String {
    list.iter()
        .take(12)
        .map(i32::to_string)
        .collect::<Vec<_>>()
        .join(", ")
}

fn analyse(input: &Input<'_>) -> Value {
    let player = input.player;
    let rows: BTreeMap<i32, &StateRow> = input.states.iter().map(|s| (s.id, s)).collect();
    let divisions_in = |id: i32| input.divisions.get(&id).copied().unwrap_or(0);

    // Occupied own states, grouped by occupier.
    let mut occupiers: BTreeMap<&str, Vec<i32>> = BTreeMap::new();
    for s in input.states {
        if s.owner == player && !s.controller.is_empty() && s.controller != player {
            occupiers.entry(&s.controller).or_default().push(s.id);
        }
    }
    let mut hostile: BTreeMap<String, Vec<&str>> = BTreeMap::new();
    for tag in occupiers.keys() {
        hostile
            .entry((*tag).to_string())
            .or_default()
            .push("controls_own_owned_state");
    }
    for tag in input.war_enemies.into_iter().flatten() {
        if tag != player {
            hostile
                .entry(tag.clone())
                .or_default()
                .push("war_relation");
        }
    }
    for tag in input.caller_enemies {
        if tag != player {
            hostile
                .entry(tag.clone())
                .or_default()
                .push("caller_supplied");
        }
    }
    let enemies_source = match (
        input.war_enemies.is_some(),
        occupiers.is_empty(),
        input.caller_enemies.is_empty(),
    ) {
        (true, true, true) => "war_relations",
        (true, _, _) => "war_relations_plus_proxy",
        (false, true, true) => "none",
        (false, _, true) => "occupation_proxy",
        (false, true, false) => "caller",
        (false, false, false) => "caller_and_occupation_proxy",
    };
    let is_hostile = |tag: &str| hostile.contains_key(tag);

    let occupied_all: Vec<Value> = occupiers
        .iter()
        .flat_map(|(controller, list)| {
            list.iter()
                .map(move |id| (*controller, *id))
                .collect::<Vec<_>>()
        })
        .map(|(controller, id)| {
            json!({"state_id":id,"name":name_of(input.names,id),"controller":controller,
                "own_divisions":divisions_in(id)})
        })
        .collect();
    let (occupied_own_states, occupied_truncated) = capped(occupied_all, input.max_states);

    let mut alerts = Vec::new();
    for (controller, list) in &occupiers {
        alerts.push(alert(
            "own_state_occupied",
            "high",
            format!(
                "{controller} controls {} own state(s): {}",
                list.len(),
                ids(list)
            ),
            "hoi4_army_move",
            json!({"army":"<army>","state_id":list[0]}),
        ));
    }

    let mut result = json!({
        "source": "native own wars stage A (borders and occupation)",
        "player": player,
        "wars": match &input.wars { Ok(v) => (*v).clone(), Err(_) => Value::Null },
        "wars_reason": match &input.wars { Ok(_) => Value::Null, Err(e) => json!(e) },
        "wars_note": "Stage B: wars come from CWarRelation pointers in CDiplomacyStatus (see diplomacy_state.rs); sides, leaders, war score and justification timers are offset_not_validated",
        "enemies_source": enemies_source,
        "hostile_tags": hostile.iter().map(|(t, why)| json!({"tag":t,"basis":why})).collect::<Vec<_>>(),
        "occupied_own_states": occupied_own_states,
        "occupied_own_states_truncated": occupied_truncated,
        "own_divisions_in_transit": input.in_transit,
        "validation": {
            "war_relations": "live validated structurally (pointer identity on a civil war, relation lists match history); the player's own war not yet observed",
            "owner_controller": "live validated (same read as hoi4_states)",
            "division_states": "live validated (same read as hoi4_armies)",
            "adjacency": "derived from install map files, not yet compared with the live map",
            "covered": "heuristic: own divisions stand in the border states; not a front-line-order read",
            "front_line_orders": "unknown (order contents unmapped)"
        },
    });

    let adjacency = match &input.adjacency {
        Ok(a) => *a,
        Err(reason) => {
            result["borders"] = Value::Null;
            result["borders_reason"] = json!(reason);
            result["threatened_own_states"] = Value::Null;
            result["enemy_held_adjacent_states"] = Value::Null;
            result["alerts"] = json!(alerts);
            return result;
        }
    };

    // Borders: own-controlled states next to a state someone else controls.
    #[derive(Default)]
    struct Border {
        own: BTreeSet<i32>,
        theirs: BTreeSet<i32>,
    }
    let mut borders: BTreeMap<&str, Border> = BTreeMap::new();
    for s in input.states.iter().filter(|s| s.controller == player) {
        for n in adjacency.get(&s.id).into_iter().flatten() {
            let Some(other) = rows.get(n) else { continue };
            if other.controller.is_empty() || other.controller == player {
                continue;
            }
            let b = borders.entry(&other.controller).or_default();
            b.own.insert(s.id);
            b.theirs.insert(*n);
        }
    }
    let mut border_rows: Vec<(bool, Value)> = Vec::new();
    for (tag, b) in &borders {
        let own: Vec<i32> = b.own.iter().copied().collect();
        let held: usize = own.iter().map(|id| divisions_in(*id)).sum();
        let uncovered: Vec<i32> = own
            .iter()
            .copied()
            .filter(|id| divisions_in(*id) == 0)
            .collect();
        let bad = is_hostile(tag);
        if bad && held == 0 {
            alerts.push(alert(
                "border_uncovered",
                "high",
                format!(
                    "No own division stands in any of the {} border state(s) facing {tag}: {}",
                    own.len(),
                    ids(&own)
                ),
                "hoi4_army_move",
                json!({"army":"<army>","state_id":own[0]}),
            ));
        }
        border_rows.push((
            bad,
            json!({"neighbour_tag":tag,"hostile":bad,
                "hostile_basis":hostile.get(*tag).cloned().unwrap_or_default(),
                "own_border_states":own,
                "neighbour_states":b.theirs.iter().collect::<Vec<_>>(),
                "own_divisions_in_border_states":held,
                "uncovered_own_border_states":uncovered,
                "covered":held>0,
                "heuristic":true}),
        ));
    }
    border_rows
        .sort_by_key(|(bad, row)| (!*bad, row["neighbour_tag"].as_str().map(str::to_string)));
    result["borders"] = json!(border_rows.into_iter().map(|(_, r)| r).collect::<Vec<_>>());

    // Own-controlled states with a hostile-controlled neighbour.
    let mut threatened = Vec::new();
    let mut undefended = Vec::new();
    for s in input.states.iter().filter(|s| s.controller == player) {
        let near: Vec<&&StateRow> = adjacency
            .get(&s.id)
            .into_iter()
            .flatten()
            .filter_map(|n| rows.get(n))
            .filter(|o| is_hostile(&o.controller))
            .collect();
        if near.is_empty() {
            continue;
        }
        let tags: BTreeSet<&str> = near.iter().map(|o| o.controller.as_str()).collect();
        let held = divisions_in(s.id);
        if held == 0 {
            undefended.push(s.id);
        }
        threatened.push(json!({"state_id":s.id,"name":name_of(input.names,s.id),
            "owner":s.owner,
            "adjacent_hostile_states":near.iter().map(|o| o.id).collect::<Vec<_>>(),
            "hostile_tags":tags,"own_divisions":held,"defended":held>0}));
    }
    let (threatened, threatened_truncated) = capped(threatened, input.max_states);
    result["threatened_own_states"] = json!(threatened);
    result["threatened_own_states_truncated"] = json!(threatened_truncated);
    if !undefended.is_empty() {
        alerts.push(alert(
            "threatened_state_undefended",
            "medium",
            format!(
                "{} own state(s) next to hostile-held states have no own division: {}",
                undefended.len(),
                ids(&undefended)
            ),
            "hoi4_army_move",
            json!({"army":"<army>","state_id":undefended[0]}),
        ));
    }

    // Hostile-controlled states touching own-controlled states.
    let mut held_adjacent = Vec::new();
    for s in input.states.iter().filter(|s| is_hostile(&s.controller)) {
        let touching: Vec<i32> = adjacency
            .get(&s.id)
            .into_iter()
            .flatten()
            .copied()
            .filter(|n| rows.get(n).is_some_and(|o| o.controller == player))
            .collect();
        if touching.is_empty() {
            continue;
        }
        let near: usize = touching.iter().map(|id| divisions_in(*id)).sum();
        held_adjacent.push(json!({"state_id":s.id,"name":name_of(input.names,s.id),
            "owner":s.owner,"controller":s.controller,
            "adjacent_own_states":touching,"own_divisions_in_adjacent_states":near}));
    }
    let (held_adjacent, held_truncated) = capped(held_adjacent, input.max_states);
    result["enemy_held_adjacent_states"] = json!(held_adjacent);
    result["enemy_held_adjacent_states_truncated"] = json!(held_truncated);
    result["alerts"] = json!(alerts);
    result
}

// ---------------------------------------------------------------------------
// Tool entry point.

fn parse_args(args: &Map<String, Value>) -> Result<(BTreeSet<String>, usize)> {
    let mut enemies = BTreeSet::new();
    if let Some(list) = args.get("enemies") {
        let list = list
            .as_array()
            .filter(|l| l.len() <= 32)
            .ok_or_else(|| Error::new("invalid_arguments", "enemies must list at most 32 tags"))?;
        for tag in list {
            let tag = tag
                .as_str()
                .filter(|t| {
                    t.len() == 3
                        && t.bytes()
                            .all(|b| b.is_ascii_uppercase() || b.is_ascii_digit())
                })
                .ok_or_else(|| Error::new("invalid_tag", "Tags are three upper-case letters"))?;
            enemies.insert(tag.to_string());
        }
    }
    let max = match args.get("max_states") {
        None => DEFAULT_MAX_STATES,
        Some(v) => v
            .as_u64()
            .filter(|n| (1..=1100).contains(n))
            .ok_or_else(|| Error::new("invalid_arguments", "max_states must be 1..1100"))?
            as usize,
    };
    Ok((enemies, max))
}

pub(super) fn wars(process: &dyn Process, dir: &Path, args: &Map<String, Value>) -> Result<Value> {
    let (enemies, max_states) = parse_args(args)?;
    let started = Instant::now();
    let snap = read(process, snapshot)?;
    let graph = adjacency(dir);
    let names: HashMap<i32, String> = super::states::names(dir)
        .map(|n| n.into_iter().collect())
        .unwrap_or_default();
    let graph_ref = match &graph {
        Ok(g) => Ok(g.as_ref()),
        Err(e) => Err(format!("{}: {}", e.code, e.message)),
    };
    let stage_b = super::diplomacy_state::player_wars(process);
    let (war_enemies, wars_value) = match &stage_b {
        Ok(w) => (Some(&w.enemies), Ok(&w.json)),
        Err(e) => (None, Err(format!("{}: {}", e.code, e.message))),
    };
    let mut out = analyse(&Input {
        player: &snap.player,
        states: &snap.states,
        divisions: &snap.divisions,
        in_transit: snap.in_transit,
        adjacency: graph_ref,
        names: &names,
        caller_enemies: &enemies,
        war_enemies,
        wars: wars_value,
        max_states,
    });
    out["observation_paused_game"] = json!(false);
    out["input_authorized"] = json!(false);
    out["freshness"] = json!({"elapsed_ms": started.elapsed().as_secs_f64() * 1000.0});
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bmp(width: usize, rows_top_down: &[&[u32]]) -> Vec<u8> {
        let h = rows_top_down.len();
        let stride = (width * 3).div_ceil(4) * 4;
        let mut out = vec![0u8; 54 + stride * h];
        out[..2].copy_from_slice(b"BM");
        out[10..14].copy_from_slice(&54u32.to_le_bytes());
        out[14..18].copy_from_slice(&40u32.to_le_bytes());
        out[18..22].copy_from_slice(&(width as u32).to_le_bytes());
        out[22..26].copy_from_slice(&(h as u32).to_le_bytes());
        out[26..28].copy_from_slice(&1u16.to_le_bytes());
        out[28..30].copy_from_slice(&24u16.to_le_bytes());
        // BMP stores the bottom row first.
        for (y, row) in rows_top_down.iter().rev().enumerate() {
            for (x, c) in row.iter().enumerate() {
                let o = 54 + y * stride + x * 3;
                out[o] = (c & 0xff) as u8;
                out[o + 1] = ((c >> 8) & 0xff) as u8;
                out[o + 2] = ((c >> 16) & 0xff) as u8;
            }
        }
        out
    }

    const A: u32 = 0x010000;
    const B: u32 = 0x020000;
    const C: u32 = 0x030000;
    const SEA: u32 = 0x000100;
    const LAKE: u32 = 0x000200;

    fn defs() -> HashMap<u32, (i32, bool)> {
        parse_definition(
            "0;0;0;0;land;false;unknown;0\n1;1;0;0;land;false;plains;1\n2;2;0;0;land;false;forest;1\n3;3;0;0;land;false;hills;1\n4;0;1;0;sea;true;ocean;0\n5;0;2;0;lake;false;lakes;0\nbad\n",
        )
    }

    #[test]
    fn definition_maps_colours_and_land_flag() {
        let d = defs();
        assert_eq!(d[&A], (1, true));
        assert_eq!(d[&SEA], (4, false));
        assert_eq!(d[&LAKE], (5, false));
    }

    #[test]
    fn land_edges_use_horizontal_and_vertical_contact_only() {
        // A|B on top, C below B only, sea below A, lake right of C.
        let image = bmp(3, &[&[A, A, B], &[SEA, C, B], &[SEA, SEA, LAKE]]);
        let edges = land_edges(&image, &defs()).unwrap();
        // A-B (row 0), A-C (vertical at x=1), B-C (row 1). Sea and lake never count.
        assert_eq!(
            edges.into_iter().collect::<Vec<_>>(),
            vec![(1, 2), (1, 3), (2, 3)]
        );
    }

    #[test]
    fn diagonal_contact_is_not_adjacency() {
        let image = bmp(2, &[&[A, SEA], &[SEA, B]]);
        assert!(land_edges(&image, &defs()).unwrap().is_empty());
    }

    #[test]
    fn rows_are_padded_to_four_bytes() {
        // width 3 -> stride 12 (9 data + 3 pad); a wrong stride would misread.
        let image = bmp(3, &[&[A, B, B], &[A, A, B]]);
        let edges = land_edges(&image, &defs()).unwrap();
        assert_eq!(edges.into_iter().collect::<Vec<_>>(), vec![(1, 2)]);
    }

    #[test]
    fn rejects_bad_bitmaps() {
        assert_eq!(
            land_edges(b"nope", &defs()).unwrap_err().code,
            "map_data_unavailable"
        );
        let mut image = bmp(2, &[&[A, B]]);
        image[28] = 8; // 8 bpp
        assert!(land_edges(&image, &defs()).is_err());
        let mut image = bmp(2, &[&[A, B]]);
        image.truncate(58);
        assert!(land_edges(&image, &defs()).is_err());
    }

    #[test]
    fn fold_drops_same_state_and_stateless_provinces() {
        let edges: BTreeSet<(i32, i32)> = [(1, 2), (1, 3), (2, 9)].into_iter().collect();
        let state_of: HashMap<i32, i32> = [(1, 10), (2, 10), (3, 11)].into_iter().collect();
        let adj = fold_states(&edges, &state_of);
        assert_eq!(adj[&10], BTreeSet::from([11]));
        assert_eq!(adj[&11], BTreeSet::from([10]));
        assert_eq!(adj.len(), 2);
    }

    // --- analysis fixtures ------------------------------------------------

    fn row(id: i32, owner: &str, controller: &str) -> StateRow {
        StateRow {
            id,
            owner: owner.into(),
            controller: controller.into(),
        }
    }

    /// 1-2-3-4 chain: GER 1,2 | POL 3 | FRA 4 ; plus GER 5 touching FRA 4 only.
    fn adjacency_fixture() -> Adjacency {
        let mut a: Adjacency = BTreeMap::new();
        for (x, y) in [(1, 2), (2, 3), (3, 4), (4, 5)] {
            a.entry(x).or_default().insert(y);
            a.entry(y).or_default().insert(x);
        }
        a
    }

    fn run(
        states: &[StateRow],
        divisions: &[(i32, usize)],
        enemies: &[&str],
        adj: std::result::Result<&Adjacency, String>,
        max: usize,
    ) -> Value {
        let divisions: BTreeMap<i32, usize> = divisions.iter().copied().collect();
        let names: HashMap<i32, String> = [(2, "Border".to_string())].into_iter().collect();
        let enemies: BTreeSet<String> = enemies.iter().map(|s| s.to_string()).collect();
        analyse(&Input {
            player: "GER",
            states,
            divisions: &divisions,
            in_transit: 2,
            adjacency: adj,
            names: &names,
            caller_enemies: &enemies,
            war_enemies: None,
            wars: Err("test".to_string()),
            max_states: max,
        })
    }

    fn codes(v: &Value) -> Vec<String> {
        v["alerts"]
            .as_array()
            .unwrap()
            .iter()
            .map(|a| a["code"].as_str().unwrap().to_string())
            .collect()
    }

    #[test]
    fn peaceful_borders_are_listed_but_raise_no_alert() {
        let adj = adjacency_fixture();
        let states = [
            row(1, "GER", "GER"),
            row(2, "GER", "GER"),
            row(3, "POL", "POL"),
            row(4, "FRA", "FRA"),
            row(5, "GER", "GER"),
        ];
        let v = run(&states, &[], &[], Ok(&adj), 50);
        assert_eq!(v["wars"], Value::Null);
        assert_eq!(v["wars_reason"], "test");
        assert_eq!(v["enemies_source"], "none");
        assert_eq!(v["own_divisions_in_transit"], 2);
        let b = v["borders"].as_array().unwrap();
        assert_eq!(b.len(), 2);
        let pol = b.iter().find(|x| x["neighbour_tag"] == "POL").unwrap();
        assert_eq!(pol["own_border_states"], json!([2]));
        assert_eq!(pol["neighbour_states"], json!([3]));
        assert_eq!(pol["covered"], false);
        assert_eq!(pol["heuristic"], true);
        assert_eq!(pol["hostile"], false);
        assert!(codes(&v).is_empty());
        assert_eq!(v["threatened_own_states"], json!([]));
    }

    #[test]
    fn caller_supplied_enemy_makes_uncovered_border_an_alert() {
        let adj = adjacency_fixture();
        let states = [
            row(1, "GER", "GER"),
            row(2, "GER", "GER"),
            row(3, "POL", "POL"),
            row(4, "FRA", "FRA"),
            row(5, "GER", "GER"),
        ];
        let v = run(&states, &[(1, 5)], &["POL"], Ok(&adj), 50);
        assert_eq!(v["enemies_source"], "caller");
        // Divisions stand in state 1, not in the border state 2.
        let pol = &v["borders"][0];
        assert_eq!(pol["neighbour_tag"], "POL");
        assert_eq!(pol["hostile"], true);
        assert_eq!(pol["covered"], false);
        assert_eq!(pol["own_divisions_in_border_states"], 0);
        assert_eq!(pol["uncovered_own_border_states"], json!([2]));
        let c = codes(&v);
        assert!(c.contains(&"border_uncovered".to_string()));
        assert!(c.contains(&"threatened_state_undefended".to_string()));
        let alert = v["alerts"]
            .as_array()
            .unwrap()
            .iter()
            .find(|a| a["code"] == "border_uncovered")
            .unwrap();
        assert_eq!(alert["fix"]["tool"], "hoi4_army_move");
        assert_eq!(alert["fix"]["args_hint"]["state_id"], 2);
        assert_eq!(v["threatened_own_states"][0]["state_id"], 2);
        assert_eq!(v["threatened_own_states"][0]["name"], "Border");
        assert_eq!(v["enemy_held_adjacent_states"][0]["state_id"], 3);
        assert_eq!(
            v["enemy_held_adjacent_states"][0]["adjacent_own_states"],
            json!([2])
        );
    }

    #[test]
    fn a_division_in_a_border_state_covers_it() {
        let adj = adjacency_fixture();
        let states = [
            row(1, "GER", "GER"),
            row(2, "GER", "GER"),
            row(3, "POL", "POL"),
        ];
        let v = run(&states, &[(2, 3)], &["POL"], Ok(&adj), 50);
        assert_eq!(v["borders"][0]["covered"], true);
        assert_eq!(v["borders"][0]["own_divisions_in_border_states"], 3);
        assert!(codes(&v).is_empty());
        assert_eq!(v["threatened_own_states"][0]["defended"], true);
    }

    #[test]
    fn occupation_marks_the_occupier_hostile_and_alerts() {
        let adj = adjacency_fixture();
        // GER owns state 2 but FRA controls it; ally-looking POL holds state 3.
        let states = [
            row(1, "GER", "GER"),
            row(2, "GER", "FRA"),
            row(3, "POL", "POL"),
            row(4, "FRA", "FRA"),
        ];
        let v = run(&states, &[], &[], Ok(&adj), 50);
        assert_eq!(v["enemies_source"], "occupation_proxy");
        assert_eq!(v["hostile_tags"][0]["tag"], "FRA");
        assert_eq!(v["occupied_own_states"][0]["state_id"], 2);
        assert_eq!(v["occupied_own_states"][0]["controller"], "FRA");
        let c = codes(&v);
        assert!(c.contains(&"own_state_occupied".to_string()));
        // State 1 touches FRA-held state 2 with no division.
        assert!(c.contains(&"border_uncovered".to_string()));
        assert_eq!(v["borders"][0]["neighbour_tag"], "FRA");
        assert_eq!(v["borders"][0]["own_border_states"], json!([1]));
    }

    #[test]
    fn adjacency_failure_keeps_occupation_and_says_why() {
        let states = [row(1, "GER", "GER"), row(2, "GER", "FRA")];
        let v = run(&states, &[], &[], Err("map_data_unavailable: x".into()), 50);
        assert_eq!(v["borders"], Value::Null);
        assert_eq!(v["borders_reason"], "map_data_unavailable: x");
        assert_eq!(v["occupied_own_states"].as_array().unwrap().len(), 1);
        assert_eq!(codes(&v), vec!["own_state_occupied"]);
    }

    #[test]
    fn lists_are_capped_and_flagged() {
        let adj = adjacency_fixture();
        let states = [
            row(1, "GER", "FRA"),
            row(2, "GER", "FRA"),
            row(3, "GER", "FRA"),
        ];
        let v = run(&states, &[], &[], Ok(&adj), 2);
        assert_eq!(v["occupied_own_states"].as_array().unwrap().len(), 2);
        assert_eq!(v["occupied_own_states_truncated"], true);
    }

    #[test]
    fn war_relation_enemies_are_hostile_and_reported() {
        let states = [row(1, "GER", "GER"), row(2, "POL", "POL")];
        let adj: Adjacency = [(1, BTreeSet::from([2])), (2, BTreeSet::from([1]))]
            .into_iter()
            .collect();
        let divisions: BTreeMap<i32, usize> = BTreeMap::new();
        let names: HashMap<i32, String> = HashMap::new();
        let none = BTreeSet::new();
        let war = BTreeSet::from(["POL".to_string()]);
        let wars = json!({"at_war":true,"count":1});
        let v = analyse(&Input {
            player: "GER",
            states: &states,
            divisions: &divisions,
            in_transit: 0,
            adjacency: Ok(&adj),
            names: &names,
            caller_enemies: &none,
            war_enemies: Some(&war),
            wars: Ok(&wars),
            max_states: 10,
        });
        assert_eq!(v["enemies_source"], "war_relations");
        assert_eq!(v["wars"]["count"], 1);
        assert_eq!(v["borders"][0]["hostile"], true);
        assert_eq!(v["borders"][0]["hostile_basis"][0], "war_relation");
    }

    #[test]
    fn arguments_are_validated() {
        let ok = json!({"enemies":["POL","FRA"],"max_states":10});
        let (e, m) = parse_args(ok.as_object().unwrap()).unwrap();
        assert_eq!((e.len(), m), (2, 10));
        for bad in [
            json!({"enemies":["pol"]}),
            json!({"enemies":"POL"}),
            json!({"max_states":0}),
            json!({"max_states":5000}),
        ] {
            assert!(parse_args(bad.as_object().unwrap()).is_err(), "{bad}");
        }
    }

    /// Manual check against the real install:
    /// `HOI4_GAME_DIR=... cargo test --release real_install -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn real_install_adjacency_prints_for_manual_comparison() {
        let dir = std::env::var("HOI4_GAME_DIR").expect("HOI4_GAME_DIR");
        let started = Instant::now();
        let adj = build_adjacency(Path::new(&dir)).unwrap();
        eprintln!(
            "states with neighbours: {} in {:?}",
            adj.len(),
            started.elapsed()
        );
        for id in [64, 10, 11, 16, 89, 87] {
            eprintln!("state {id}: {:?}", adj.get(&id));
        }
        assert!(adj.len() > 500);
        for (a, set) in &adj {
            for b in set {
                assert!(adj[b].contains(a), "asymmetric {a}-{b}");
            }
        }
    }
}
