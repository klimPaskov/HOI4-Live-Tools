//! Concentrated command: formations, the tasks inside them, their commanders and the theatre's
//! army group. A child module of `plan_run` (declared there with `#[path]`) like plan_military.rs.
//!
//! The principle: commanders gain experience and traits from the battles of the divisions they
//! lead, and a field marshal from the armies of his group. Command is therefore concentrated in a
//! few standing formations that fight, each led by a general who keeps it, all under one field
//! marshal per theatre. A guard post, a corridor hold or a border watch is a task given to
//! divisions inside such a formation (its front line covers the state), never an army of its
//! own: an army of one or two divisions gives its general almost nothing to learn from, leaves
//! commanders idle, and its small neighbourhood makes the pocket rule fire on every enemy move.
//!
//! [`concentrate`] rewrites the plan for the tick (the plan file is never changed):
//!   - an `army_layout` entry with `task_of` is a task of that formation: no army is made for it,
//!     its states join the formation's `covers`, and anything that named it (hold_targets,
//!     vp_targets, army_group) names the formation;
//!   - an entry below `armies.min_divisions` (default 3) without a named general is a task of the
//!     formation of its state (strike first), else of the first formation;
//!   - at most `armies.max_per_group` formations (default 3) per theatre: strike roles first,
//!     then roles with a named general, in plan order; the rest become tasks the same way.
//!     A theatre is the whole layout when the plan has at most one army group, else each group;
//!   - one army group over the theatre's formations: the plan's single group is widened to every
//!     formation, a plan without one gets `ag_theatre` (marshal: the best free field marshal);
//!   - `armies.concentrate: false` keeps the plan as written.
//!
//! [`command_step`] then brings the game in line, one verified action at a time:
//!   1. an army bound to a task, and any other army standing in the theatre that no formation
//!      plays, is merged into its formation (hoi4_army_merge; never above `armies.max`, 24);
//!   2. every army with at least `armies.min_for_general` divisions (default 1) gets a general
//!      once: the layout's general, then `armies.generals`, then the best free commander of the
//!      country for its role (attack and planning for strike formations, defence otherwise;
//!      marshal candidates and field-marshal traits are kept for the army group). A commander
//!      is never moved from another army (no reassignment: assigning resets his bonuses);
//!   3. the army groups (plan_military.rs `army_group_step`).
use super::{ints, list, plan_military, strings, text, GameHour, State, Tools, DAY};
use crate::error::Result;
use serde_json::{json, Value};
use std::collections::{BTreeMap, HashMap, HashSet};

/// Default cap of formations under one field marshal.
const MAX_PER_GROUP: usize = 3;
/// Default smallest formation: an entry below it without a named general is a task.
const MIN_DIVISIONS: u64 = 3;

/// Traits that mark a field marshal (kept for the army group when generals are picked).
pub(super) const MARSHAL_TRAITS: [&str; 9] = [
    "offensive_doctrine",
    "defensive_doctrine",
    "logistics_wizard",
    "thorough_planner",
    "fast_planner",
    "inflexible_strategist",
    "organisational_leader",
    "aggressive_assaulter",
    "charismatic",
];

fn push_unique(target: &mut Value, key: &str, ids: &[i64]) {
    let mut have = ints(&target[key]);
    for id in ids {
        if !have.contains(id) {
            have.push(*id);
        }
    }
    if !have.is_empty() {
        target[key] = json!(have);
    }
}

/// The plan for this tick with command concentrated (see the module text); None when nothing
/// changes.
pub(super) fn concentrate(plan: &Value) -> Option<Value> {
    let cfg = &plan["armies"];
    if cfg["concentrate"] == json!(false) {
        return None;
    }
    let layout = plan["army_layout"].as_array()?;
    let max_per_group = cfg["max_per_group"]
        .as_u64()
        .map_or(MAX_PER_GROUP, |n| n as usize);
    let min_divisions = cfg["min_divisions"].as_u64().unwrap_or(MIN_DIVISIONS);
    let hints: Vec<String> = layout
        .iter()
        .map(|e| text(&e["name_hint"]).to_string())
        .collect();
    let groups = list(plan, "army_group");
    // theatre of each entry: its group's index when the plan has several groups, else 0
    let theatre_of = |hint: &str| -> usize {
        if groups.len() < 2 {
            return 0;
        }
        groups
            .iter()
            .position(|g| strings(g, "armies").iter().any(|a| a == hint))
            .map_or(usize::MAX, |i| i + 1)
    };
    let strike = |i: usize| text(&layout[i]["role"]) == "strike";
    let general = |i: usize| !text(&layout[i]["general"]).trim().is_empty();
    // formations before the cap: no task_of, big enough or with a general
    let mut candidates: Vec<usize> = (0..layout.len())
        .filter(|&i| !hints[i].is_empty())
        .filter(|&i| text(&layout[i]["task_of"]).is_empty())
        .filter(|&i| layout[i]["count"].as_u64().unwrap_or(1) >= min_divisions || general(i))
        .collect();
    candidates.sort_by_key(|&i| (!strike(i), !general(i), i));
    let mut kept: Vec<usize> = Vec::new();
    let mut per: HashMap<usize, usize> = HashMap::new();
    for i in candidates {
        let t = theatre_of(&hints[i]);
        let n = per.entry(t).or_default();
        if max_per_group == 0 || t == usize::MAX || *n < max_per_group {
            *n += 1;
            kept.push(i);
        }
    }
    if kept.is_empty() {
        return None;
    }
    kept.sort_unstable();
    let kept_hints: Vec<String> = kept.iter().map(|&i| hints[i].clone()).collect();
    // the formation each other entry is a task of
    let mut tasks: BTreeMap<String, String> = BTreeMap::new();
    let mut far: Vec<String> = Vec::new();
    for i in 0..layout.len() {
        if hints[i].is_empty() || kept.contains(&i) {
            continue;
        }
        let e = &layout[i];
        let named = text(&e["task_of"]);
        let home = e["state"].as_i64();
        let t = theatre_of(&hints[i]);
        let same_theatre: Vec<usize> = kept
            .iter()
            .copied()
            .filter(|&k| groups.len() < 2 || theatre_of(&hints[k]) == t)
            .collect();
        let pool = if same_theatre.is_empty() {
            kept.clone()
        } else {
            same_theatre
        };
        let covers_home = |k: usize| {
            let f = &layout[k];
            home.is_some_and(|h| {
                f["state"].as_i64() == Some(h)
                    || ints(&f["axis"]).contains(&h)
                    || ints(&f["covers"]).contains(&h)
                    || ints(&f["guard_states_in_order"]).contains(&h)
            })
        };
        let by_name = kept.iter().copied().find(|&k| hints[k] == named);
        let placed = by_name
            .or_else(|| {
                pool.iter()
                    .copied()
                    .filter(|&k| layout[k]["state"].as_i64() == home)
                    .min_by_key(|&k| (!strike(k), k))
            })
            .or_else(|| pool.iter().copied().find(|&k| covers_home(k)));
        // no formation works in its state: the plan's references go to the first one, and its
        // army joins the nearest formation on the map (command_step knows the state positions)
        if placed.is_none() {
            far.push(hints[i].clone());
        }
        tasks.insert(hints[i].clone(), hints[placed.unwrap_or(pool[0])].clone());
    }
    // a task named as task_of of another task resolves to that task's formation
    let resolve = |h: &str| -> String { tasks.get(h).cloned().unwrap_or_else(|| h.to_string()) };
    let mut out = plan.clone();
    let mut new_layout: Vec<Value> = kept.iter().map(|&i| layout[i].clone()).collect();
    for (task, formation) in &tasks {
        if far.contains(task) {
            continue;
        }
        let Some(e) = layout.iter().find(|e| text(&e["name_hint"]) == task) else {
            continue;
        };
        let Some(f) = new_layout
            .iter_mut()
            .find(|f| text(&f["name_hint"]) == formation)
        else {
            continue;
        };
        let mut covered: Vec<i64> = e["state"].as_i64().into_iter().collect();
        covered.extend(ints(&e["guard_states_in_order"]));
        covered.extend(ints(&e["covers"]));
        push_unique(f, "covers", &covered);
        push_unique(f, "axis", &ints(&e["axis"]));
    }
    // already concentrated and nothing to widen: leave the plan alone
    let one_theatre = groups.len() < 2;
    let group_ok = !one_theatre
        || (groups.len() == 1 && {
            let have = strings(&groups[0], "armies");
            kept_hints.iter().all(|h| have.contains(h))
                && have.iter().all(|h| !tasks.contains_key(h))
        })
        || (groups.is_empty() && kept_hints.len() < 2);
    if tasks.is_empty() && group_ok {
        return None;
    }
    out["army_layout"] = json!(new_layout);
    // anything that named a task names its formation; a hold of a task is the formation's front
    if let Some(holds) = out["hold_targets"].as_array_mut() {
        for h in holds {
            let army = text(&h["army"]).to_string();
            if let Some(f) = tasks.get(&army) {
                h["task_of"] = json!(f);
                if let Some(o) = h.as_object_mut() {
                    o.remove("army");
                }
            }
        }
    }
    if let Some(vps) = out["vp_targets"].as_array_mut() {
        for v in vps {
            let army = text(&v["army"]).to_string();
            if tasks.contains_key(&army) {
                v["army"] = json!(resolve(&army));
            }
        }
    }
    let mut new_groups: Vec<Value> = groups.to_vec();
    for g in &mut new_groups {
        let mut armies: Vec<String> = Vec::new();
        for a in strings(g, "armies") {
            let a = resolve(&a);
            if !armies.contains(&a) {
                armies.push(a);
            }
        }
        g["armies"] = json!(armies);
    }
    if one_theatre && kept_hints.len() >= 2 {
        if new_groups.is_empty() {
            new_groups.push(json!({"id": "ag_theatre", "name": "theatre", "armies": kept_hints}));
        } else {
            let mut armies = strings(&new_groups[0], "armies");
            for h in &kept_hints {
                if !armies.contains(h) {
                    armies.push(h.clone());
                }
            }
            new_groups[0]["armies"] = json!(armies);
        }
    }
    new_groups.retain(|g| list(g, "armies").len() >= 2);
    out["army_group"] = json!(new_groups);
    out["_formations"] = json!({"formations": kept_hints, "tasks": tasks, "nearest": far});
    Some(out)
}

/// Every state a formation works in: its home, axis, covers and guard posts (and its tasks').
fn theatre_states(plan: &Value) -> HashSet<i64> {
    let mut out: HashSet<i64> = HashSet::new();
    for e in list(plan, "army_layout") {
        out.extend(e["state"].as_i64());
        for key in ["axis", "covers", "guard_states_in_order"] {
            out.extend(ints(&e[key]));
        }
    }
    out.extend(ints(&plan["armies"]["states"]));
    out.extend(ints(&plan["war"]["theatre"]));
    out
}

/// The country tag commanders must belong to (their character ids start with it).
fn own_tag(state: &State, tools: &dyn Tools, plan: &Value) -> String {
    if let Some(t) = plan["war"]["me"].as_str() {
        return t.to_string();
    }
    if let Some(t) = state
        .persist
        .identity
        .as_ref()
        .and_then(|i| i["player"].as_str())
    {
        return t.to_string();
    }
    tools
        .call("hoi4_overview", &json!({}))
        .ok()
        .and_then(|v| v["player"].as_str().map(str::to_string))
        .unwrap_or_default()
}

/// A commander row is one of ours, free and not marked unassignable.
fn free_own(c: &Value, tag: &str) -> bool {
    let id = text(&c["id"]);
    (tag.is_empty() || id.is_empty() || id.starts_with(&format!("{tag}_")))
        && c["assigned_to"].is_null()
        && c["available"] != json!(false)
        && c["stale"] != json!(true)
}

/// The free commanders of the country that can be picked. The country's leader list also holds
/// characters who are not generals now (advisers, leaders a later focus recruits); the game's
/// commander list never offers those. Once the roster has seen that list (a row says
/// `listed_in_panel_or_assigned`), only the rows it showed are candidates; a roster that has not
/// seen it yet offers every free row.
fn pickable<'a>(rows: &'a [Value], tag: &str) -> Vec<&'a Value> {
    let free: Vec<&Value> = rows.iter().filter(|c| free_own(c, tag)).collect();
    let seen = rows
        .iter()
        .any(|c| c["listed_in_panel_or_assigned"] == json!(true) && c["assigned_to"].is_null());
    if !seen {
        return free;
    }
    free.into_iter()
        .filter(|c| c["listed_in_panel_or_assigned"] != json!(false))
        .collect()
}

fn num(c: &Value, key: &str) -> i64 {
    c[key].as_i64().unwrap_or(0)
}

fn has_marshal_trait(c: &Value) -> bool {
    strings(c, "traits")
        .iter()
        .any(|t| MARSHAL_TRAITS.contains(&t.as_str()))
}

/// Score of a general for a formation's role.
fn general_score(c: &Value, strike: bool) -> i64 {
    let traits = strings(c, "traits");
    let fit: i64 = traits
        .iter()
        .map(|t| match t.as_str() {
            "infantry_officer" | "trait_mountaineer" | "desert_fox" | "hill_fighter"
            | "armor_officer" | "panzer_leader" | "trait_engineer" | "fortress_buster"
                if strike =>
            {
                2
            }
            "unyielding_defender" | "trait_cautious" | "organizer" | "trait_engineer"
                if !strike =>
            {
                2
            }
            _ => 0,
        })
        .sum();
    let (main, second) = if strike {
        (num(c, "attack"), num(c, "planning"))
    } else {
        (num(c, "defense"), num(c, "logistics"))
    };
    num(c, "skill") * 10 + main * 3 + second * 2 + fit - if has_marshal_trait(c) { 6 } else { 0 }
}

/// The best free field marshal of the country (field-marshal traits first, then skill and
/// planning), outside `skip`.
pub(super) fn best_marshal(
    state: &State,
    tools: &dyn Tools,
    plan: &Value,
    skip: &dyn Fn(&str) -> bool,
) -> Option<String> {
    let tag = own_tag(state, tools, plan);
    let read = tools.call("hoi4_commanders", &json!({})).ok()?;
    pickable(list(&read, "commanders"), &tag)
        .into_iter()
        .filter(|c| !skip(text(&c["name"])))
        .max_by_key(|c| {
            (
                has_marshal_trait(c),
                num(c, "skill"),
                num(c, "planning") + num(c, "logistics"),
            )
        })
        .map(|c| text(&c["name"]).to_string())
        .filter(|n| !n.is_empty())
}

/// One action at most: merges into formations, generals, army groups (see the module text).
pub(super) fn command_step(
    state: &mut State,
    tools: &dyn Tools,
    plan: &Value,
    lease: &str,
    now: GameHour,
) -> Result<Option<String>> {
    if (plan["armies"].is_object() || plan["army_layout"].is_array())
        && !state.skipped(now, "command", "armies")
    {
        if let Some(done) = merge_and_lead(state, tools, plan, lease, now)? {
            return Ok(Some(done));
        }
    }
    plan_military::army_group_step(state, tools, plan, lease, now)
}

fn merge_and_lead(
    state: &mut State,
    tools: &dyn Tools,
    plan: &Value,
    lease: &str,
    now: GameHour,
) -> Result<Option<String>> {
    let cfg = &plan["armies"];
    let roster = tools.call("hoi4_armies", &json!({"threat": false}))?;
    let armies = list(&roster, "armies");
    let size = |name: &str| {
        armies
            .iter()
            .find(|a| text(&a["army"]) == name)
            .and_then(|a| a["division_count"].as_u64())
            .unwrap_or(0)
    };
    let present: HashSet<String> = armies
        .iter()
        .map(|a| text(&a["army"]).to_string())
        .collect();
    // 1. merges into formations
    if cfg["concentrate"] != json!(false) && !list(plan, "army_layout").is_empty() {
        let max = cfg["max"].as_u64().unwrap_or(24);
        let formations: Vec<String> = list(plan, "army_layout")
            .iter()
            .map(|e| text(&e["name_hint"]).to_string())
            .filter(|h| !h.is_empty())
            .collect();
        let tasks: HashMap<String, String> = plan["_formations"]["tasks"]
            .as_object()
            .map(|m| {
                m.iter()
                    .filter_map(|(k, v)| Some((k.clone(), v.as_str()?.to_string())))
                    .collect()
            })
            .unwrap_or_default();
        let nearest: HashSet<String> = strings(&plan["_formations"], "nearest")
            .into_iter()
            .collect();
        let formation_army = |state: &State, hint: &str| -> Option<String> {
            state
                .layout
                .get(hint)
                .filter(|a| !a.is_empty() && present.contains(*a))
                .cloned()
        };
        let playing: HashSet<String> = formations
            .iter()
            .filter_map(|h| formation_army(state, h))
            .collect();
        let theatre = theatre_states(plan);
        // the nearest formation needs state positions: after a restart the war step has not
        // read them yet (sk2: a Jubaland guard went to the Eritrean formation), so read the own
        // states once when an army stands where no position is known
        let unknown = armies.iter().any(|a| {
            a["main_state"]
                .as_i64()
                .is_some_and(|s| !state.centers.contains_key(&(s as i32)))
        });
        if unknown && !state.skipped(now, "command", "centers") {
            state.skip(now, "command", "centers", DAY);
            let tag = own_tag(state, tools, plan);
            if let Ok(read) = tools.call("hoi4_states", &json!({"query": tag})) {
                for row in list(&read, "states") {
                    if let (Some(id), Some(x), Some(z)) = (
                        row["state_id"].as_i64(),
                        row["x"].as_f64(),
                        row["z"].as_f64(),
                    ) {
                        state.centers.insert(id as i32, (x, z));
                    }
                }
            }
        }
        let reserve = strings(&plan["war"], "reserve");
        for a in armies {
            let name = text(&a["army"]).to_string();
            let n = a["division_count"].as_u64().unwrap_or(0);
            if n == 0
                || playing.contains(&name)
                || reserve.contains(&name)
                || plan_military::invading(state, plan, &name)
                || state.skipped(now, "cmerge", &name)
            {
                continue;
            }
            let bound = state
                .layout
                .iter()
                .find(|(_, army)| **army == name)
                .map(|(h, _)| h.clone());
            let placed = match &bound {
                Some(h) if nearest.contains(h) => None,
                Some(h) => match tasks.get(h) {
                    Some(f) => Some(f.clone()),
                    None => continue, // a role of a plan that is not concentrated
                },
                None => None,
            };
            let hint = match placed {
                Some(f) => f,
                None => {
                    let here = a["main_state"].as_i64();
                    if bound.is_none() && !here.is_some_and(|s| theatre.contains(&s)) {
                        continue;
                    }
                    // the formation standing in the same state, else the nearest one
                    let mut best: Option<(String, f64)> = None;
                    for h in &formations {
                        let Some(f) = formation_army(state, h) else {
                            continue;
                        };
                        let there = armies
                            .iter()
                            .find(|x| text(&x["army"]) == f)
                            .and_then(|x| x["main_state"].as_i64());
                        let d = match (here, there) {
                            (Some(x), Some(y)) if x == y => 0.0,
                            (Some(x), Some(y)) => {
                                match (
                                    state.centers.get(&(x as i32)),
                                    state.centers.get(&(y as i32)),
                                ) {
                                    (Some(p), Some(q)) => {
                                        ((p.0 - q.0).powi(2) + (p.1 - q.1).powi(2)).sqrt()
                                    }
                                    _ => 1.0e9,
                                }
                            }
                            _ => 1.0e9,
                        };
                        if best.as_ref().is_none_or(|b| d < b.1) {
                            best = Some((h.clone(), d));
                        }
                    }
                    match best {
                        Some((h, _)) => h,
                        None => continue,
                    }
                }
            };
            let Some(target) = formation_army(state, &hint) else {
                continue; // the formation has no army yet: the layout makes it first
            };
            if target == name || size(&target) + n > max {
                continue;
            }
            let (ok, why) = state.act(
                tools,
                lease,
                "hoi4_army_merge",
                json!({"source": name, "target": target}),
                "cm",
            )?;
            if ok {
                if let Some(h) = bound {
                    state.layout.remove(&h);
                }
                state.war_guard.remove(&name);
                state.war_assign.remove(&name);
                state.war_stage.remove(&name);
                state.war_move.remove(&name);
                plan_military::forget_army(state, &name);
            } else {
                state.skip(now, "cmerge", &name, 2 * DAY);
            }
            return Ok(Some(format!(
                "command: {name} ({n} division(s)) merged into {target} ({hint}): {ok} {why}"
            )));
        }
    }
    // 2. a general for every army that has none
    let min_general = cfg["min_for_general"].as_u64().unwrap_or(1).max(1);
    let leaderless: Vec<(String, Option<String>)> = armies
        .iter()
        .filter(|a| a["commander"].is_null())
        .filter(|a| a["division_count"].as_u64().unwrap_or(0) >= min_general)
        .map(|a| text(&a["army"]).to_string())
        .filter(|n| !state.skipped(now, "cgen", n))
        .map(|n| {
            let hint = state
                .layout
                .iter()
                .find(|(_, a)| **a == n)
                .map(|(h, _)| h.clone());
            (n, hint)
        })
        .collect();
    if leaderless.is_empty() {
        state.skip(now, "command", "armies", DAY / 2);
        return Ok(None);
    }
    let read = tools.call("hoi4_commanders", &json!({}))?;
    let tag = own_tag(state, tools, plan);
    let marshals: HashSet<String> = list(plan, "army_group")
        .iter()
        .flat_map(|g| match &g["marshal"] {
            Value::String(s) => vec![s.clone()],
            Value::Array(a) => a
                .iter()
                .filter_map(|v| v.as_str().map(str::to_string))
                .collect(),
            _ => Vec::new(),
        })
        .collect();
    let rows = list(&read, "commanders");
    let busy: HashSet<String> = rows
        .iter()
        .filter(|c| !c["assigned_to"].is_null())
        .map(|c| text(&c["name"]).to_string())
        .chain(
            armies
                .iter()
                .filter_map(|a| a["commander"].as_str().map(str::to_string)),
        )
        .collect();
    let usable = |state: &State, name: &str| {
        !name.is_empty()
            && !busy.contains(name)
            && !state.skipped(now, "cgenx", name)
            && rows
                .iter()
                .find(|c| text(&c["name"]) == name)
                .is_none_or(|c| free_own(c, &tag))
    };
    for (army, hint) in leaderless {
        let entry = hint.as_ref().and_then(|h| {
            list(plan, "army_layout")
                .iter()
                .find(|e| text(&e["name_hint"]) == h.as_str())
        });
        let strike = entry.is_none_or(|e| text(&e["role"]) != "guard");
        let mut wanted: Vec<String> = entry
            .map(|e| text(&e["general"]).to_string())
            .into_iter()
            .chain(strings(cfg, "generals"))
            .collect();
        let mut pool: Vec<&Value> = pickable(rows, &tag)
            .into_iter()
            .filter(|c| !marshals.contains(text(&c["name"])))
            .collect();
        pool.sort_by_key(|c| std::cmp::Reverse(general_score(c, strike)));
        wanted.extend(pool.iter().map(|c| text(&c["name"]).to_string()));
        let Some(general) = wanted.into_iter().find(|g| usable(state, g)) else {
            state.skip(now, "cgen", &army, 7 * DAY);
            continue;
        };
        let (ok, why) = state.act(
            tools,
            lease,
            "hoi4_army_commander",
            json!({"army": army, "general": general}),
            "cg",
        )?;
        if ok {
            state.skip(now, "cgen", &army, 365 * DAY);
        } else {
            state.skip(now, "cgen", &army, DAY);
            // A name the commander list does not offer is not a general of this country now:
            // asking again next month only repeats the refusal.
            let wait = if plan_military::code(&why) == "unknown_commander" {
                365
            } else {
                30
            };
            state.skip(now, "cgenx", &general, wait * DAY);
        }
        return Ok(Some(format!(
            "command: general {general} -> {army}: {ok} {why}"
        )));
    }
    state.skip(now, "command", "armies", DAY / 2);
    Ok(None)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn layout_plan() -> Value {
        json!({
            "armies": {"generals": ["Messe"]},
            "army_layout": [
                {"name_hint": "N1", "state": 550, "role": "strike", "count": 6, "general": "Messe", "templates": ["a"]},
                {"name_hint": "N2", "state": 550, "role": "strike", "count": 3, "general": "Vecchiarelli", "templates": ["a"]},
                {"name_hint": "NG", "state": 550, "role": "guard", "count": 2, "general": "Biroli", "templates": ["b"], "guard_states_in_order": [550, 842]},
                {"name_hint": "GA", "state": 550, "role": "guard", "count": 1, "templates": ["b"], "guard_states_in_order": [908]},
                {"name_hint": "S1", "state": 559, "role": "strike", "count": 2, "general": "Baldassarre", "templates": ["c"]},
                {"name_hint": "SG", "state": 559, "role": "guard", "count": 1, "templates": ["c"], "guard_states_in_order": [559]},
                {"name_hint": "JG", "state": 844, "role": "guard", "count": 1, "templates": ["c"]}
            ],
            "hold_targets": [{"id": "h1", "army": "GA", "provinces": [1]}],
            "vp_targets": [{"province": 5, "state": 842, "army": "NG"}],
            "army_group": [{"id": "ag", "armies": ["N1", "N2", "NG"], "marshal": ["Graziani"]}]
        })
    }

    #[test]
    fn small_roles_become_tasks_of_a_few_formations_under_one_group() {
        let out = concentrate(&layout_plan()).unwrap();
        let hints: Vec<&str> = list(&out, "army_layout")
            .iter()
            .map(|e| text(&e["name_hint"]))
            .collect();
        assert_eq!(hints, ["N1", "N2", "S1"]);
        assert_eq!(out["_formations"]["tasks"]["NG"], "N1");
        assert_eq!(out["_formations"]["tasks"]["GA"], "N1");
        assert_eq!(out["_formations"]["tasks"]["SG"], "S1");
        // a task in a state no formation works in joins the nearest formation on the map
        assert_eq!(out["_formations"]["nearest"], json!(["JG"]));
        // the formation covers its tasks' states
        let covers = ints(&out["army_layout"][0]["covers"]);
        assert!(covers.contains(&842) && covers.contains(&908), "{covers:?}");
        // holds of tasks become the formation's front, vp targets name the formation
        assert!(out["hold_targets"][0]["army"].is_null());
        assert_eq!(out["hold_targets"][0]["task_of"], "N1");
        assert_eq!(out["vp_targets"][0]["army"], "N1");
        // the one group covers every formation and keeps its marshal list
        assert_eq!(out["army_group"][0]["armies"], json!(["N1", "N2", "S1"]));
        assert_eq!(out["army_group"][0]["marshal"], json!(["Graziani"]));
        // a concentrated plan stays as it is
        assert!(concentrate(&out).is_none());
    }

    #[test]
    fn plan_can_keep_its_layout_or_raise_the_cap() {
        let mut plan = layout_plan();
        plan["armies"]["concentrate"] = json!(false);
        assert!(concentrate(&plan).is_none());
        let mut plan = layout_plan();
        plan["armies"]["max_per_group"] = json!(4);
        plan["armies"]["min_divisions"] = json!(1);
        let out = concentrate(&plan).unwrap();
        assert_eq!(list(&out, "army_layout").len(), 4);
        assert_eq!(out["_formations"]["tasks"]["GA"], "N1");
    }

    #[test]
    fn a_plan_without_a_group_gets_one_and_explicit_tasks_are_kept() {
        let mut plan = layout_plan();
        plan.as_object_mut().unwrap().remove("army_group");
        plan["army_layout"][1]["task_of"] = json!("S1");
        let out = concentrate(&plan).unwrap();
        assert_eq!(out["_formations"]["tasks"]["N2"], "S1");
        assert_eq!(out["army_group"][0]["id"], "ag_theatre");
        assert_eq!(out["army_group"][0]["armies"], json!(["N1", "NG", "S1"]));
    }

    #[test]
    fn generals_fit_the_role_and_marshals_are_kept_for_groups() {
        let attacker = json!({"skill": 3, "attack": 4, "defense": 1, "planning": 2, "logistics": 1, "traits": []});
        let defender = json!({"skill": 3, "attack": 1, "defense": 4, "planning": 1, "logistics": 2, "traits": ["trait_cautious"]});
        let marshal = json!({"skill": 3, "attack": 4, "defense": 1, "planning": 2, "logistics": 1, "traits": ["offensive_doctrine"]});
        assert!(general_score(&attacker, true) > general_score(&defender, true));
        assert!(general_score(&defender, false) > general_score(&attacker, false));
        assert!(general_score(&attacker, true) > general_score(&marshal, true));
    }

    use std::cell::RefCell;

    /// Replies by tool name; actions are Verified and recorded.
    struct Game {
        replies: HashMap<&'static str, Value>,
        actions: RefCell<Vec<(String, Value)>>,
    }
    impl Tools for Game {
        fn call(&self, name: &str, args: &Value) -> Result<Value> {
            if args.get("request_id").is_some() {
                self.actions
                    .borrow_mut()
                    .push((name.to_string(), args.clone()));
                return Ok(json!({"receipt": {"status": "Verified"}}));
            }
            Ok(self.replies.get(name).cloned().unwrap_or_else(|| json!({})))
        }
    }
    fn game(replies: Vec<(&'static str, Value)>) -> Game {
        Game {
            replies: replies.into_iter().collect(),
            actions: RefCell::new(Vec::new()),
        }
    }

    #[test]
    fn task_armies_and_stray_armies_are_merged_into_their_formations() {
        let plan = concentrate(&layout_plan()).unwrap();
        let tools = game(vec![(
            "hoi4_armies",
            json!({"armies": [
                {"army": "Army 1", "division_count": 6, "commander": "Messe", "main_state": 550},
                {"army": "Army 2", "division_count": 3, "commander": "Vecchiarelli", "main_state": 550},
                {"army": "Army 3", "division_count": 2, "commander": "Baldassarre", "main_state": 559},
                {"army": "Army 7", "division_count": 2, "commander": null, "main_state": 559},
                {"army": "Army 5", "division_count": 1, "commander": null, "main_state": 842},
                {"army": "Home", "division_count": 4, "commander": null, "main_state": 1}]}),
        )]);
        let mut state = State::default();
        for (h, a) in [
            ("N1", "Army 1"),
            ("N2", "Army 2"),
            ("S1", "Army 3"),
            ("GA", "Army 5"),
        ] {
            state.layout.insert(h.into(), a.into());
        }
        state.war_guard.insert("Army 5".into(), 908);
        let out = command_step(&mut state, &tools, &plan, "l", 1000)
            .unwrap()
            .unwrap();
        assert!(
            out.starts_with("command: Army 7 (2 division(s)) merged into Army 3 (S1)"),
            "{out}"
        );
        // (the game removes a merged army; this fake roster keeps listing it)
        state.skip(1000, "cmerge", "Army 7", DAY);
        let out = command_step(&mut state, &tools, &plan, "l", 1001)
            .unwrap()
            .unwrap();
        assert!(
            out.starts_with("command: Army 5 (1 division(s)) merged into Army 1 (N1)"),
            "{out}"
        );
        assert!(!state.layout.contains_key("GA") && !state.war_guard.contains_key("Army 5"));
        state.skip(1001, "cmerge", "Army 5", DAY);
        // a home army outside the theatre is never touched; no free general is known here, so
        // next comes the theatre's army group over the three formations
        let out = command_step(&mut state, &tools, &plan, "l", 1002)
            .unwrap()
            .unwrap();
        assert!(
            out.starts_with("army group ag from Army 1, Army 2, Army 3: true"),
            "{out}"
        );
        assert!(!tools
            .actions
            .borrow()
            .iter()
            .any(|(n, a)| n == "hoi4_army_merge" && a["source"] == "Home"));
    }

    #[test]
    fn a_task_far_from_every_formation_joins_the_nearest_on_the_map() {
        let plan = concentrate(&layout_plan()).unwrap();
        let tools = game(vec![
            (
                "hoi4_armies",
                json!({"armies": [
                {"army": "Army 1", "division_count": 6, "commander": "Messe", "main_state": 550},
                {"army": "Army 3", "division_count": 2, "commander": "Baldassarre", "main_state": 559},
                {"army": "Army 8", "division_count": 1, "commander": null, "main_state": 844}]}),
            ),
            (
                "hoi4_states",
                json!({"states": [
                {"state_id": 550, "x": 0.0, "z": 0.0},
                {"state_id": 559, "x": 100.0, "z": 0.0},
                {"state_id": 844, "x": 120.0, "z": 0.0}]}),
            ),
        ]);
        let plan = {
            let mut p = plan;
            p["war"] = json!({"me": "ITA"});
            p
        };
        let mut state = State::default();
        for (h, a) in [("N1", "Army 1"), ("S1", "Army 3"), ("JG", "Army 8")] {
            state.layout.insert(h.into(), a.into());
        }
        // the positions come from the own states read (none known after a restart)
        let out = command_step(&mut state, &tools, &plan, "l", 1000)
            .unwrap()
            .unwrap();
        assert!(
            out.starts_with("command: Army 8 (1 division(s)) merged into Army 3 (S1)"),
            "{out}"
        );
    }

    #[test]
    fn leaderless_armies_get_the_best_free_general_for_their_role() {
        let plan = json!({"war": {"me": "ITA"}, "armies": {"generals": ["Busy"]},
            "army_group": [{"id": "ag", "armies": ["X", "Y"], "marshal": ["Graziani"]}]});
        let tools = game(vec![
            (
                "hoi4_armies",
                json!({"armies": [
                {"army": "Army 9", "division_count": 1, "commander": null, "main_state": 1},
                {"army": "Army 1", "division_count": 5, "commander": "Busy", "main_state": 1}]}),
            ),
            (
                "hoi4_commanders",
                json!({"commanders": [
                {"name": "Busy", "id": "ITA_busy", "assigned_to": {"kind": "army", "name": "Army 1"}, "skill": 5},
                {"name": "Graziani", "id": "ITA_graziani", "assigned_to": null, "skill": 4, "attack": 4, "traits": ["offensive_doctrine"]},
                {"name": "Foreign", "id": "ETH_x", "assigned_to": null, "skill": 5, "attack": 5},
                {"name": "Weak", "id": "ITA_weak", "assigned_to": null, "skill": 1, "attack": 1},
                {"name": "Guillet", "id": "ITA_guillet", "assigned_to": null, "skill": 3, "attack": 3, "planning": 2}]}),
            ),
        ]);
        let mut state = State::default();
        let out = command_step(&mut state, &tools, &plan, "l", 1000)
            .unwrap()
            .unwrap();
        // never the busy one (no reassignment), the marshal candidate or a foreign commander
        assert!(
            out.starts_with("command: general Guillet -> Army 9: true"),
            "{out}"
        );
        // given once: the army is not looked at again for a year
        assert!(state.skipped(1000 + 300 * DAY, "cgen", "Army 9"));
    }

    #[test]
    fn generals_come_from_the_rows_the_commander_list_showed() {
        // The leader list also names characters who are not generals now; they outrank the
        // real ones on paper and the game's list never offers them.
        let plan = json!({"war": {"me": "ITA"}, "armies": {}});
        let roster = |listed: bool| {
            game(vec![
                (
                    "hoi4_armies",
                    json!({"armies": [
                    {"army": "Guard", "division_count": 3, "commander": null, "main_state": 1}]}),
                ),
                (
                    "hoi4_commanders",
                    json!({"commanders": [
                    {"name": "Messe", "id": "ITA_messe", "skill": 4,
                        "assigned_to": {"kind": "army", "name": "Strike"},
                        "listed_in_panel_or_assigned": true},
                    {"name": "Later Hero", "id": "ITA_hero", "assigned_to": null, "skill": 5,
                        "attack": 5, "available": null, "listed_in_panel_or_assigned": false},
                    {"name": "Bastico", "id": "ITA_bastico", "assigned_to": null, "skill": 1,
                        "attack": 1, "available": null, "listed_in_panel_or_assigned": listed}]}),
                ),
            ])
        };
        let out = command_step(&mut State::default(), &roster(true), &plan, "l", 1000)
            .unwrap()
            .unwrap();
        assert!(
            out.starts_with("command: general Bastico -> Guard: true"),
            "{out}"
        );
        // the list was never seen (only assigned rows are known): every free row may be tried
        let out = command_step(&mut State::default(), &roster(false), &plan, "l", 1000)
            .unwrap()
            .unwrap();
        assert!(
            out.starts_with("command: general Later Hero -> Guard: true"),
            "{out}"
        );
    }

    #[test]
    fn a_group_without_a_free_listed_marshal_takes_the_best_free_field_marshal() {
        let plan = json!({"war": {"me": "ITA"},
            "army_group": [{"id": "ag", "armies": ["A", "B"]}]});
        let tools = game(vec![
            (
                "hoi4_armies",
                json!({
                "armies": [
                    {"army": "A", "division_count": 5, "commander": "x", "army_group": "Army Group 1"},
                    {"army": "B", "division_count": 5, "commander": "y", "army_group": "Army Group 1"}],
                "army_groups": [{"army_group": "Army Group 1", "field_marshal": null}]}),
            ),
            (
                "hoi4_commanders",
                json!({"commanders": [
                {"name": "Plain", "id": "ITA_plain", "assigned_to": null, "skill": 3, "traits": []},
                {"name": "Graziani", "id": "ITA_graziani", "assigned_to": null, "skill": 2, "traits": ["offensive_doctrine"]}]}),
            ),
        ]);
        let mut state = State::default();
        let out = plan_military::army_group_step(&mut state, &tools, &plan, "l", 1000)
            .unwrap()
            .unwrap();
        assert!(out.contains("field marshal Graziani: true"), "{out}");
        let (name, args) = tools.actions.borrow().last().cloned().unwrap();
        assert_eq!(
            (name.as_str(), &args["army"]),
            ("hoi4_army_commander", &json!("Army Group 1"))
        );
    }
}
