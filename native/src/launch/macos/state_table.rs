//! The state table: every state a country owns or controls in one memory pass, with live
//! building levels, shared building slots (unlocked, used, queued, free), resources, victory
//! points and the start-of-game facts, filtered and sorted in the server. Nothing walks states
//! through the interface: a large country (the Soviet Union, the USA) is read as fast as a small
//! one, and the interface is touched only to act on the few states chosen from the table.
//!
//! Unlocked shared slots are computed from the game's own rules and files: the state category's
//! `local_building_slots`, times one plus the `global_building_slots_factor` of the researched
//! technologies and active ideas, plus flat `global_building_slots`, plus the history's
//! `add_extra_state_shared_building_slots` (read from the state itself where the build maps it, so
//! slots a focus or decision adds during the game count too), capped at the define
//! `MAX_SHARED_SLOTS`. A construction refusal still marks a state full.
use super::{mapdata, mios, states, ui::read, units};
use crate::{
    error::{Error, Result},
    memory::Reader,
    platform::Process,
    script::{self, Block, Entry},
};
use serde_json::{json, Map, Value};
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::path::Path;

/// One state's facts from history/states.
#[derive(Clone, Debug, Default, PartialEq)]
pub(super) struct StateFacts {
    pub category: String,
    pub extra_slots: f64,
    pub resources: BTreeMap<String, f64>,
}

/// The slot rules of one content folder (mods included).
#[derive(Clone, Debug, Default, PartialEq)]
pub(super) struct Catalog {
    pub category_slots: HashMap<String, f64>,
    /// technology or idea key -> (global_building_slots_factor, global_building_slots)
    pub slot_modifiers: HashMap<String, (f64, f64)>,
    pub states: HashMap<i32, StateFacts>,
    pub max_shared: u32,
}

fn slot_modifier(block: &[Entry]) -> Option<(f64, f64)> {
    let factor = block.number("global_building_slots_factor");
    let flat = block.number("global_building_slots");
    (factor.is_some() || flat.is_some()).then(|| (factor.unwrap_or(0.0), flat.unwrap_or(0.0)))
}

/// (id, facts) of one history/states file.
pub(super) fn parse_state(text: &str) -> Option<(i32, StateFacts)> {
    let root = script::parse(text);
    let state = root.find_val("state")?.block();
    let id = state.number("id")? as i32;
    let history = state.find_val("history").map(|h| h.block()).unwrap_or(&[]);
    let resources = state
        .find_val("resources")
        .map(|r| {
            r.block()
                .iter()
                .filter_map(|e| Some((e.key.clone(), e.val.scalar()?.parse::<f64>().ok()?)))
                .collect()
        })
        .unwrap_or_default();
    Some((
        id,
        StateFacts {
            category: state.text("state_category").unwrap_or_default().to_string(),
            extra_slots: history
                .number("add_extra_state_shared_building_slots")
                .unwrap_or(0.0),
            resources,
        },
    ))
}

/// category key -> local_building_slots, from common/state_category.
pub(super) fn parse_categories(entries: &[Entry]) -> HashMap<String, f64> {
    let mut out = HashMap::new();
    for top in entries.iter().filter(|e| e.key == "state_categories") {
        for c in top.val.block() {
            if let Some(n) = c.val.block().number("local_building_slots") {
                out.insert(c.key.clone(), n);
            }
        }
    }
    out
}

/// technology key -> slot modifier, from common/technologies.
pub(super) fn parse_technologies(entries: &[Entry], out: &mut HashMap<String, (f64, f64)>) {
    for top in entries.iter().filter(|e| e.key == "technologies") {
        for t in top.val.block() {
            if let Some(m) = slot_modifier(t.val.block()) {
                out.insert(t.key.clone(), m);
            }
        }
    }
}

/// idea key -> slot modifier (its `modifier` block), from common/ideas.
pub(super) fn parse_ideas(entries: &[Entry], out: &mut HashMap<String, (f64, f64)>) {
    for top in entries.iter().filter(|e| e.key == "ideas") {
        for category in top.val.block() {
            for idea in category.val.block() {
                if let Some(m) = idea
                    .val
                    .block()
                    .find_val("modifier")
                    .and_then(|m| slot_modifier(m.block()))
                {
                    out.insert(idea.key.clone(), m);
                }
            }
        }
    }
}

fn max_shared_slots(dir: &Path) -> u32 {
    for entry in std::fs::read_dir(dir.join("common/defines"))
        .into_iter()
        .flatten()
        .flatten()
    {
        let Ok(text) = std::fs::read_to_string(entry.path()) else {
            continue;
        };
        for line in text.lines() {
            let line = line.split("--").next().unwrap_or("").trim();
            if let Some(rest) = line.strip_prefix("MAX_SHARED_SLOTS") {
                if let Some(n) = rest
                    .trim_start()
                    .strip_prefix('=')
                    .and_then(|v| v.trim().trim_end_matches(',').trim().parse::<f64>().ok())
                {
                    return n as u32;
                }
            }
        }
    }
    25
}

pub(super) fn catalog(dir: &Path) -> std::sync::Arc<Catalog> {
    script::cached(dir, "state_table", |dir| {
        let mut slot_modifiers = HashMap::new();
        parse_technologies(
            &script::parse_dir(&dir.join("common/technologies")),
            &mut slot_modifiers,
        );
        parse_ideas(
            &script::parse_dir(&dir.join("common/ideas")),
            &mut slot_modifiers,
        );
        let mut states = HashMap::new();
        for entry in std::fs::read_dir(dir.join("history/states"))
            .into_iter()
            .flatten()
            .flatten()
        {
            if let Ok(bytes) = std::fs::read(entry.path()) {
                if let Some((id, f)) = parse_state(&String::from_utf8_lossy(&bytes)) {
                    states.insert(id, f);
                }
            }
        }
        Catalog {
            category_slots: parse_categories(&script::parse_dir(
                &dir.join("common/state_category"),
            )),
            slot_modifiers,
            states,
            max_shared: max_shared_slots(dir),
        }
    })
}

/// Unlocked shared slots of a state under the given (factor, flat) bonus.
pub(super) fn unlocked(catalog: &Catalog, facts: &StateFacts, bonus: (f64, f64)) -> Option<u32> {
    let base = *catalog.category_slots.get(&facts.category)?;
    let n = (base * (1.0 + bonus.0) + 1e-6).floor() + bonus.1 + facts.extra_slots;
    Some((n.max(0.0) as u32).min(catalog.max_shared))
}

/// The own country's slot bonus: researched technologies and active ideas.
pub(super) fn own_bonus(catalog: &Catalog, keys: impl IntoIterator<Item = String>) -> (f64, f64) {
    let mut bonus = (0.0, 0.0);
    for k in keys {
        if let Some((f, n)) = catalog.slot_modifiers.get(&k) {
            bonus.0 += f;
            bonus.1 += n;
        }
    }
    bonus
}

/// (entry, owner tag, controller tag, building levels when the row is wanted, extra shared slots
/// read from the state when the build maps them)
type LiveRow = (
    units::StateEntry,
    String,
    String,
    Option<BTreeMap<String, u32>>,
    Option<i32>,
);
/// What one memory bracket gives the table.
struct Live {
    own: String,
    rows: Vec<LiveRow>,
    queued: BTreeMap<i32, i64>,
    bonus_keys: Vec<String>,
    controllers: HashMap<i32, String>,
}

fn live(
    r: &Reader<'_>,
    image: u64,
    tag: &str,
    provinces: &dyn Fn(i32) -> Vec<i32>,
) -> Result<Live> {
    let own = crate::game::context_from_reader(r, image)?.player;
    let mut names: HashMap<i32, String> = HashMap::new();
    let mut name = |idx: i32| -> String {
        names
            .entry(idx)
            .or_insert_with(|| states::tag_name(r, image, idx).unwrap_or_default())
            .clone()
    };
    let extra_at = crate::construction::layout(r).state_extra_slots;
    let mut rows = Vec::new();
    let mut wanted_provinces = Vec::new();
    for s in units::state_scan(r, image)? {
        let (o, c) = (name(s.owner), name(s.controller));
        if o == tag || c == tag {
            let levels = units::state_levels(r, image, s.ptr).ok();
            let extra = (extra_at != 0)
                .then(|| r.i32(s.ptr + extra_at).ok())
                .flatten()
                .filter(|n| (0..=100).contains(n));
            wanted_provinces.extend(provinces(s.id));
            rows.push((s, o, c, levels, extra));
        }
    }
    let controllers = states::province_controllers(r, image, &wanted_provinces)
        .unwrap_or_default()
        .into_iter()
        .collect();
    let (queued, bonus_keys) = if own == tag {
        let queued = crate::construction::queued_shared_by_state(r, image).unwrap_or_default();
        let mut keys: Vec<String> = crate::research::assignments(r, image)
            .map(|(_, done)| done.into_iter().collect())
            .unwrap_or_default();
        if let Ok(l) = mios::snapshot(r, image) {
            keys.extend(l.active_ideas);
        }
        (queued, keys)
    } else {
        (BTreeMap::new(), Vec::new())
    };
    Ok(Live {
        own,
        rows,
        queued,
        bonus_keys,
        controllers,
    })
}

/// Filters and order asked of the table.
#[derive(Clone, Debug, Default, PartialEq)]
pub(super) struct Query {
    pub ids: Option<BTreeSet<i32>>,
    pub owned_only: bool,
    pub min_free_slots: Option<i64>,
    pub building: Option<String>,
    pub min_level: Option<u32>,
    pub max_level: Option<u32>,
    pub resource: Option<String>,
    pub sort: Option<String>,
    pub limit: Option<usize>,
    pub offset: usize,
}
pub(super) const SORTS: [&str; 8] = [
    "free_slots",
    "factories",
    "infrastructure",
    "victory_points",
    "level",
    "resource",
    "id",
    "name",
];
impl Query {
    /// The table arguments of a hoi4_states call (None: the plain lookup).
    pub(super) fn from_args(args: &Map<String, Value>) -> Result<Option<Query>> {
        let keys = [
            "ids",
            "owned_only",
            "min_free_slots",
            "building",
            "min_level",
            "max_level",
            "resource",
            "sort",
            "limit",
            "offset",
        ];
        if !keys.iter().any(|k| args.contains_key(*k)) {
            return Ok(None);
        }
        let bad = |m: &str| Error::new("invalid_arguments", m);
        let word = |k: &str| -> Result<Option<String>> {
            match args.get(k) {
                None => Ok(None),
                Some(v) => v
                    .as_str()
                    .filter(|s| {
                        !s.is_empty()
                            && s.len() <= 64
                            && s.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_')
                    })
                    .map(|s| Some(s.to_string()))
                    .ok_or_else(|| bad(&format!("{k} must be a game key"))),
            }
        };
        let num = |k: &str, max: i64| -> Result<Option<i64>> {
            match args.get(k) {
                None => Ok(None),
                Some(v) => v
                    .as_i64()
                    .filter(|n| (0..=max).contains(n))
                    .map(Some)
                    .ok_or_else(|| bad(&format!("{k} must be 0..{max}"))),
            }
        };
        let ids = match args.get("ids") {
            None => None,
            Some(v) => Some(
                v.as_array()
                    .filter(|a| (1..=1000).contains(&a.len()))
                    .ok_or_else(|| bad("ids must list 1..1000 state ids"))?
                    .iter()
                    .map(|x| {
                        x.as_i64()
                            .filter(|n| (1..4096).contains(n))
                            .map(|n| n as i32)
                            .ok_or_else(|| bad("state id must be 1..4095"))
                    })
                    .collect::<Result<BTreeSet<i32>>>()?,
            ),
        };
        let sort = word("sort")?;
        if let Some(s) = &sort {
            if !SORTS.contains(&s.as_str()) {
                return Err(bad(&format!("sort must be one of {}", SORTS.join(", "))));
            }
        }
        let q = Query {
            ids,
            owned_only: args
                .get("owned_only")
                .map(|v| {
                    v.as_bool()
                        .ok_or_else(|| bad("owned_only must be true or false"))
                })
                .transpose()?
                .unwrap_or(false),
            min_free_slots: num("min_free_slots", 25)?,
            building: word("building")?,
            min_level: num("min_level", 100)?.map(|n| n as u32),
            max_level: num("max_level", 100)?.map(|n| n as u32),
            resource: word("resource")?,
            sort,
            limit: num("limit", 1000)?.map(|n| n as usize),
            offset: num("offset", 4096)?.unwrap_or(0) as usize,
        };
        if (q.min_level.is_some() || q.max_level.is_some() || q.sort.as_deref() == Some("level"))
            && q.building.is_none()
        {
            return Err(bad("min_level, max_level and sort level need building"));
        }
        if q.sort.as_deref() == Some("resource") && q.resource.is_none() {
            return Err(bad("sort resource needs resource"));
        }
        Ok(Some(q))
    }
}

fn num_at(row: &Value, path: &[&str]) -> f64 {
    let mut v = row;
    for p in path {
        v = &v[*p];
    }
    v.as_f64().unwrap_or(-1.0)
}

/// Apply the filters and order to built rows (pure, tested).
pub(super) fn select(rows: Vec<Value>, q: &Query) -> (usize, Vec<Value>) {
    let mut kept: Vec<Value> = rows
        .into_iter()
        .filter(|row| {
            let id = row["state_id"].as_i64().unwrap_or(0) as i32;
            if q.ids.as_ref().is_some_and(|ids| !ids.contains(&id)) {
                return false;
            }
            if q.owned_only && row["owner"] != row["tag_queried"] {
                return false;
            }
            if let Some(min) = q.min_free_slots {
                if row["slots"]["free"].as_i64().is_none_or(|f| f < min) {
                    return false;
                }
            }
            if let Some(b) = &q.building {
                let level = row["buildings"][b].as_u64().unwrap_or(0) as u32;
                if q.min_level.is_some_and(|m| level < m) || q.max_level.is_some_and(|m| level > m)
                {
                    return false;
                }
            }
            if let Some(res) = &q.resource {
                if row["resources"][res].as_f64().unwrap_or(0.0) <= 0.0 {
                    return false;
                }
            }
            true
        })
        .collect();
    let matched = kept.len();
    let key = |row: &Value| -> f64 {
        match q.sort.as_deref().unwrap_or("free_slots") {
            "free_slots" => num_at(row, &["slots", "free"]),
            "factories" => num_at(row, &["factories", "total"]),
            "infrastructure" => num_at(row, &["buildings", "infrastructure"]).max(0.0),
            "victory_points" => num_at(row, &["victory_points_total"]).max(0.0),
            "level" => num_at(row, &["buildings", q.building.as_deref().unwrap_or("")]).max(0.0),
            "resource" => num_at(row, &["resources", q.resource.as_deref().unwrap_or("")]).max(0.0),
            _ => 0.0,
        }
    };
    match q.sort.as_deref() {
        Some("id") => kept.sort_by_key(|r| r["state_id"].as_i64().unwrap_or(0)),
        Some("name") => kept.sort_by(|a, b| {
            a["name"]
                .as_str()
                .unwrap_or("")
                .cmp(b["name"].as_str().unwrap_or(""))
        }),
        _ => kept.sort_by(|a, b| {
            key(b)
                .total_cmp(&key(a))
                .then(a["state_id"].as_i64().cmp(&b["state_id"].as_i64()))
        }),
    }
    let kept = kept
        .into_iter()
        .skip(q.offset)
        .take(q.limit.unwrap_or(200))
        .map(|mut r| {
            if let Some(o) = r.as_object_mut() {
                o.remove("tag_queried");
            }
            r
        })
        .collect();
    (matched, kept)
}

/// Every state `tag` owns or controls, one memory bracket for every live field.
pub(super) fn table(
    process: &dyn Process,
    dir: &Path,
    tag: &str,
    query: Option<&Query>,
) -> Result<Value> {
    let started = std::time::Instant::now();
    let names: HashMap<i32, String> = states::names(dir)?.into_iter().collect();
    let cat = catalog(dir);
    let centers = mapdata::state_centers(dir).ok();
    let regions = mapdata::state_regions(dir).ok();
    let victory = mapdata::state_victory_points(dir).ok();
    let infrastructure = mapdata::state_infrastructure(dir).ok();
    let factories = mapdata::state_factories(dir);
    let provinces_of = mapdata::state_province_index(dir).ok();
    let vp_provinces = |id: i32| -> Vec<i32> {
        let mut out: Vec<i32> = provinces_of
            .as_ref()
            .and_then(|m| m.get(&id).cloned())
            .unwrap_or_default();
        if let Some(v) = victory.as_ref().and_then(|v| v.get(&id)) {
            out.extend(v.iter().map(|(p, _)| *p));
        }
        out
    };
    let live = read(process, |r, i| live(r, i, tag, &vp_provinces))?;
    let read_ms = started.elapsed().as_millis() as u64;
    let own_bonus = own_bonus(&cat, live.bonus_keys.iter().cloned());
    let (mut owned_factories, mut controlled_factories) =
        (mapdata::Factories::default(), mapdata::Factories::default());
    let (mut vp_total, mut vp_lost) = (0.0f64, 0.0f64);
    let mut vp_kept: Vec<Value> = Vec::new();
    let mut rows = Vec::new();
    for (entry, owner, controller, levels, extra) in &live.rows {
        let id = entry.id;
        let mut row = json!({"state_id": id, "name": names.get(&id).cloned().unwrap_or_default(),
            "owner": owner, "controller": controller, "tag_queried": tag});
        if let Some((x, z)) = centers.as_ref().and_then(|c| c.get(&id)) {
            row["x"] = json!(x.round());
            row["z"] = json!(z.round());
        }
        // the strategic region holding most of the state's provinces (air missions are flown
        // over regions: the executor follows the fighting from region to region with it)
        if let Some((region, _, _)) = regions.as_ref().and_then(|m| m.get(&id)) {
            row["region"] = json!(region);
        }
        if let Some(level) = infrastructure.as_ref().and_then(|m| m.get(&id)) {
            row["infrastructure_start"] = json!(level);
        }
        let facts = cat.states.get(&id);
        if let Some(f) = facts {
            row["category"] = json!(f.category);
            if !f.resources.is_empty() {
                row["resources"] = json!(f.resources);
            }
        }
        if let Some(levels) = levels {
            let built: BTreeMap<&String, &u32> = levels.iter().filter(|(_, l)| **l > 0).collect();
            row["buildings"] = json!(built);
            let lv = |k: &str| i64::from(levels.get(k).copied().unwrap_or(0));
            row["factories"] = json!({"civilian": lv("industrial_complex"),
                "military": lv("arms_factory"), "naval": lv("dockyard"),
                "total": lv("industrial_complex") + lv("arms_factory") + lv("dockyard")});
            if *owner == live.own && *owner == tag {
                let used: i64 = crate::construction::SHARED_SLOT_BUILDINGS
                    .iter()
                    .map(|k| lv(k))
                    .sum();
                let queued = live.queued.get(&id).copied().unwrap_or(0);
                // the state's own extra slots (effects during the game included) when read
                let facts = facts.map(|f| match extra {
                    Some(n) => StateFacts {
                        extra_slots: f64::from(*n),
                        ..f.clone()
                    },
                    None => f.clone(),
                });
                if let Some(u) = facts.as_ref().and_then(|f| unlocked(&cat, f, own_bonus)) {
                    row["slots"] = json!({"unlocked": u, "used": used, "queued": queued,
                        "free": (i64::from(u) - used - queued).max(0)});
                }
            }
        }
        if let Some(f) = factories.get(&id) {
            row["factories_start"] =
                json!({"civilian": f.civilian, "military": f.military, "naval": f.naval});
            let add = |sum: &mut mapdata::Factories| {
                sum.civilian += f.civilian;
                sum.military += f.military;
                sum.naval += f.naval;
            };
            if owner == tag {
                add(&mut owned_factories);
            }
            if controller == tag {
                add(&mut controlled_factories);
            }
        }
        // Provinces another country holds inside the state (an enemy can take part of a state
        // while the state still counts as ours).
        let others: Vec<Value> = provinces_of
            .as_ref()
            .and_then(|m| m.get(&id))
            .map(|ps| {
                ps.iter()
                    .filter_map(|p| {
                        let c = live.controllers.get(p)?;
                        (c != owner).then(|| json!({"province": p, "controller": c}))
                    })
                    .collect()
            })
            .unwrap_or_default();
        if !others.is_empty() {
            row["provinces_held_by_others"] = json!(others);
        }
        if let Some(vps) = victory.as_ref().and_then(|v| v.get(&id)) {
            let mut list = Vec::new();
            let mut sum = 0.0;
            for (p, v) in vps {
                let c = live.controllers.get(p).cloned().unwrap_or_default();
                sum += v;
                if owner == tag {
                    vp_total += v;
                    if !c.is_empty() && c != *owner {
                        vp_lost += v;
                    } else {
                        vp_kept.push(json!({"state_id": id, "province": p, "victory_points": v}));
                    }
                }
                list.push(json!({"province": p, "victory_points": v, "controller": c}));
            }
            row["victory_points_total"] = json!(sum);
            if owner == tag {
                row["victory_points"] = json!(list);
            }
        }
        rows.push(row);
    }
    let sum = |f: &mapdata::Factories| json!({"civilian": f.civilian, "military": f.military, "naval": f.naval, "total": f.total()});
    let live_totals = {
        let (mut c, mut m, mut n, mut free, mut slotted) = (0i64, 0i64, 0i64, 0i64, 0usize);
        for r in rows.iter().filter(|r| r["owner"] == tag) {
            c += r["factories"]["civilian"].as_i64().unwrap_or(0);
            m += r["factories"]["military"].as_i64().unwrap_or(0);
            n += r["factories"]["naval"].as_i64().unwrap_or(0);
            if let Some(f) = r["slots"]["free"].as_i64() {
                free += f;
                slotted += 1;
            }
        }
        json!({"states_owned": rows.iter().filter(|r| r["owner"] == tag).count(),
            "states_controlled": rows.iter().filter(|r| r["controller"] == tag).count(),
            "factories": {"civilian": c, "military": m, "naval": n},
            "free_slots": if slotted > 0 { json!(free) } else { Value::Null }})
    };
    let all = rows.len();
    let (matched, rows) = match query {
        Some(q) => select(rows, q),
        None => {
            let rows = rows
                .into_iter()
                .map(|mut r| {
                    if let Some(o) = r.as_object_mut() {
                        o.remove("tag_queried");
                    }
                    r
                })
                .collect::<Vec<_>>();
            (all, rows)
        }
    };
    let mut out = json!({"tag": tag, "states": rows, "states_total": all, "matched": matched,
        "totals": live_totals,
        "factories_start": {"owned": sum(&owned_factories), "controlled": sum(&controlled_factories),
            "note": "estimate: the factories history/states gives the states this country owns / controls now; construction, conversion and bombing since the start are not in it"},
        "max_shared": cat.max_shared, "read_ms": read_ms, "ms": started.elapsed().as_millis() as u64,
        "source": "one memory pass over the state list (no interface input)"});
    if tag == live.own {
        out["slots_note"] = json!("slots.unlocked is computed from the state category, the researched technologies and active ideas (global_building_slots_factor) and the state's extra slots (read from the state; effects during the game included), capped at MAX_SHARED_SLOTS. free = unlocked - built - queued.");
    }
    if vp_total > 0.0 {
        vp_kept.sort_by(|a, b| {
            b["victory_points"]
                .as_f64()
                .unwrap_or(0.0)
                .total_cmp(&a["victory_points"].as_f64().unwrap_or(0.0))
        });
        vp_kept.truncate(12);
        out["victory_points"] = json!({
            "total": vp_total,
            "held_by_others": vp_lost,
            "share_held_by_others": (vp_lost / vp_total * 1000.0).round() / 1000.0,
            "largest_still_held": vp_kept,
            "note": "estimate from the map files and live province control: a country's surrender progress follows the share of its own victory points others hold (laws, national spirits and events move the threshold)"
        });
    }
    Ok(out)
}

/// Free shared slots of own states from the table (the memory route of hoi4_state_slots).
pub(super) fn slots_of(process: &dyn Process, dir: &Path, ids: &[i32]) -> Result<Value> {
    let own = read(process, |r, i| {
        Ok(crate::game::context_from_reader(r, i)?.player)
    })?;
    let q = Query {
        ids: Some(ids.iter().copied().collect()),
        owned_only: true,
        limit: Some(ids.len()),
        ..Query::default()
    };
    let t = table(process, dir, &own, Some(&q))?;
    let found: HashMap<i64, &Value> = t["states"]
        .as_array()
        .map(|a| {
            a.iter()
                .map(|r| (r["state_id"].as_i64().unwrap_or(0), r))
                .collect()
        })
        .unwrap_or_default();
    let rows: Vec<Value> = ids
        .iter()
        .map(|id| match found.get(&i64::from(*id)) {
            Some(r) if r["slots"].is_object() => {
                let s = &r["slots"];
                json!({"state_id": id, "name": r["name"], "unlocked_slots": s["unlocked"],
                    "max_slots": t["max_shared"], "used_slots": s["used"].as_i64().unwrap_or(0) + s["queued"].as_i64().unwrap_or(0),
                    "free_slots": s["free"], "queued_shared": s["queued"], "source": "memory"})
            }
            _ => json!({"state_id": id, "error": "not an own state (or its category is unknown)"}),
        })
        .collect();
    Ok(json!({"states": rows, "ms": t["ms"],
        "note": "read from memory, no interface input: free_slots = unlocked - built - queued; unlocked is computed (see hoi4_states slots_note). view:true opens each state's view instead."}))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cat() -> Catalog {
        let mut slot_modifiers = HashMap::new();
        parse_technologies(
            &script::parse(
                "technologies = { concentrated_industry = { global_building_slots_factor = 0.2 research_cost = 2 } basic_machine_tools = { research_cost = 1 } }",
            ),
            &mut slot_modifiers,
        );
        parse_ideas(
            &script::parse(
                "ideas = { economy = { war_economy = { modifier = { global_building_slots_factor = 0.1 } } } country = { five_year = { modifier = { global_building_slots = 1 } } } }",
            ),
            &mut slot_modifiers,
        );
        Catalog {
            category_slots: parse_categories(&script::parse(
                "state_categories = { city = { local_building_slots = 6 color = { 0 200 0 } } }\nstate_categories = { megalopolis = { local_building_slots = 12 } }",
            )),
            slot_modifiers,
            states: HashMap::new(),
            max_shared: 25,
        }
    }

    #[test]
    fn slot_rules_come_from_the_files() {
        let c = cat();
        assert_eq!(c.category_slots["city"], 6.0);
        assert_eq!(c.slot_modifiers["concentrated_industry"], (0.2, 0.0));
        assert_eq!(c.slot_modifiers["war_economy"], (0.1, 0.0));
        assert_eq!(c.slot_modifiers["five_year"], (0.0, 1.0));
        assert!(!c.slot_modifiers.contains_key("basic_machine_tools"));
    }

    #[test]
    fn unlocked_slots_follow_category_bonus_extra_and_cap() {
        let c = cat();
        let city = StateFacts {
            category: "city".into(),
            ..StateFacts::default()
        };
        assert_eq!(unlocked(&c, &city, (0.0, 0.0)), Some(6));
        // 6 x 1.3 = 7.8 -> 7
        let b = own_bonus(
            &c,
            [
                "concentrated_industry".to_string(),
                "war_economy".to_string(),
            ],
        );
        assert_eq!(unlocked(&c, &city, b), Some(7));
        let extra = StateFacts {
            category: "megalopolis".into(),
            extra_slots: 2.0,
            ..StateFacts::default()
        };
        assert_eq!(unlocked(&c, &extra, (1.0, 1.0)), Some(25));
        assert_eq!(unlocked(&c, &extra, (0.0, 0.0)), Some(14));
        let unknown = StateFacts {
            category: "nowhere".into(),
            ..StateFacts::default()
        };
        assert_eq!(unlocked(&c, &unknown, (0.0, 0.0)), None);
    }

    #[test]
    fn history_gives_category_extra_slots_and_resources() {
        let (id, f) = parse_state(
            "state = { id = 6 name = \"STATE_6\" resources = { steel = 12 coal = 3 } state_category = city history = { owner = BEL add_extra_state_shared_building_slots = 2 buildings = { infrastructure = 4 } 1939.1.1 = { add_extra_state_shared_building_slots = 5 } } provinces = { 1 2 } }",
        )
        .unwrap();
        assert_eq!(id, 6);
        assert_eq!(f.category, "city");
        assert_eq!(f.extra_slots, 2.0);
        assert_eq!(f.resources["steel"], 12.0);
    }

    fn row(id: i64, free: i64, owner: &str, infra: u64, steel: f64) -> Value {
        json!({"state_id": id, "name": format!("S{id}"), "owner": owner, "tag_queried": "SOV",
            "slots": {"free": free}, "buildings": {"infrastructure": infra},
            "factories": {"total": id}, "resources": {"steel": steel}})
    }

    #[test]
    fn filters_sort_and_page_in_the_server() {
        let rows = || {
            vec![
                row(1, 0, "SOV", 5, 0.0),
                row(2, 3, "SOV", 2, 10.0),
                row(3, 5, "SOV", 3, 4.0),
                row(4, 4, "GER", 1, 0.0),
            ]
        };
        let mut args = Map::new();
        args.insert("min_free_slots".into(), json!(1));
        args.insert("owned_only".into(), json!(true));
        let q = Query::from_args(&args).unwrap().unwrap();
        let (matched, kept) = select(rows(), &q);
        assert_eq!(matched, 2);
        assert_eq!(kept[0]["state_id"], 3);
        assert!(kept[0].get("tag_queried").is_none());
        let mut args = Map::new();
        args.insert("building".into(), json!("infrastructure"));
        args.insert("max_level".into(), json!(3));
        args.insert("sort".into(), json!("level"));
        args.insert("limit".into(), json!(1));
        let q = Query::from_args(&args).unwrap().unwrap();
        let (matched, kept) = select(rows(), &q);
        assert_eq!((matched, kept.len()), (3, 1));
        assert_eq!(kept[0]["state_id"], 3);
        let mut args = Map::new();
        args.insert("resource".into(), json!("steel"));
        args.insert("sort".into(), json!("resource"));
        let q = Query::from_args(&args).unwrap().unwrap();
        let (_, kept) = select(rows(), &q);
        assert_eq!(kept[0]["state_id"], 2);
        assert_eq!(kept.len(), 2);
    }

    #[test]
    fn table_arguments_are_checked() {
        assert!(Query::from_args(&Map::new()).unwrap().is_none());
        let mut a = Map::new();
        a.insert("sort".into(), json!("level"));
        assert!(Query::from_args(&a).is_err());
        let mut a = Map::new();
        a.insert("sort".into(), json!("bogus"));
        assert!(Query::from_args(&a).is_err());
        let mut a = Map::new();
        a.insert("building".into(), json!("arms factory"));
        assert!(Query::from_args(&a).is_err());
    }
}
