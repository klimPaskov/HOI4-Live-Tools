//! The operation step: a large land war carried by drawn and executed battle plans (plan key
//! `operations`, one entry per operation).
//!
//! The game's own AI takes Poland in seven weeks with one army-group front on the border and one
//! offensive whose target line spans the country, executed on the declaration; per-army order
//! cascades and single-province strikes lose (beds e1b-e1h and Italy i23a, 18 Nov 2026). This
//! step does what the AI does and nothing else:
//!
//! - OWNERSHIP: from `prepare_from` until every enemy is out of the war the operation owns its
//!   formation armies, its hold armies and their army groups. [`guard`] refuses every other
//!   step's land order on an owned name (no move, regroup, exercise, strike, guard post or retake
//!   reaches them); the war step and the layout step do not see them at all.
//! - PREPARE (a reconciler, one order per call inside the war burst): exercises stopped, the
//!   group formed when its line armies have none, line armies without orders of their own, then
//!   each mobile and wing army's front and offensive, the group's front and its one offensive
//!   through the target line, each hold's front. Every call compares the wanted orders with the
//!   battle-plan read-back (`hoi4_armies` plan) and acts only on a difference; an offensive whose
//!   target misses the line is cleared and drawn again at most twice.
//! - EXECUTE in the tick `execute_on` holds (the declaration step is chained in front of this
//!   step in the same tick): the group and every mobile and wing army, never a hold; the
//!   executing flag is read back and an attacker found not executing is executed again.
//! - ADVANCE: every two game days the line's states are counted; when all are held or none was
//!   gained for `stall_days` (7) the next line is drawn: the plan's next phase line, else the one
//!   enemy state with the most victory points per distance. A mobile or wing army follows the
//!   operation's line once its own line is held.
//! - FINISH: every enemy out of the war: the attack orders are cleared and `on_capitulation.next`
//!   starts its PREPARE at once.
//!
//! State lives in the persisted done-memory (`op:<id>:...` keys), so a restart resumes.
use super::{ints, item_id, list, strings, text, GameHour, State, Tools, Truth, DAY};
use crate::error::Result;
use serde_json::{json, Value};
use std::collections::{HashMap, HashSet};

/// Game days without a gain on the line before the next line is drawn.
const STALL_DAYS: i64 = 7;
/// Game hours between two progress counts of an executing operation.
const CHECK_HOURS: GameHour = 2 * DAY;
/// Clear-and-redraws of one unit's orders for one line.
const REDRAWS: i64 = 2;
/// Share of a line's states an offensive's target must cover.
const COVER: f64 = 0.8;

/// Land order tools another step may not send for an owned army (by its `army` argument).
const LAND_ORDERS: [&str; 11] = [
    "hoi4_army_move",
    "hoi4_army_front",
    "hoi4_army_offensive",
    "hoi4_army_plan",
    "hoi4_army_fallback",
    "hoi4_army_area_defense",
    "hoi4_army_attack",
    "hoi4_army_gather",
    "hoi4_army_assign_plan",
    "hoi4_army_split",
    "hoi4_army_rename",
];

#[derive(Clone, Copy, Debug, PartialEq)]
enum Role {
    Group,
    Line,
    Mobile,
    Wing,
    Hold,
}

/// One unit the operation orders: an army or an army group, by its game name.
#[derive(Clone, Debug)]
struct Unit {
    name: String,
    role: Role,
    enemy: String,
    own_states: Vec<i64>,
    click: Option<i64>,
    line: Vec<i64>,
    anchor: Option<i64>,
}

fn key(id: &str, what: &str) -> String {
    format!("op:{id}:{what}")
}

fn op_id(op: &Value, index: usize) -> String {
    item_id(op).unwrap_or_else(|| format!("#{index}"))
}

fn started(state: &State, id: &str) -> bool {
    state.done.contains_key(&key(id, "prep")) && !state.done.contains_key(&key(id, "end"))
}

/// Whether a predicate holds (a missing or empty one holds).
fn holds(state: &mut State, tools: &dyn Tools, when: &Value, now: GameHour) -> bool {
    match when {
        Value::Null => true,
        Value::String(s) if s.trim().is_empty() => true,
        _ => super::judge(state, tools, when, now) == Truth::True,
    }
}

/// Starts the operations whose `prepare_from` (or `execute_on`) holds and maps every owned army
/// and army group to its operation (`State::op_owned`). Reads only.
pub(super) fn refresh(state: &mut State, tools: &dyn Tools, plan: &Value, now: GameHour) {
    let ops = list(plan, "operations");
    if ops.is_empty() {
        state.op_owned.clear();
        return;
    }
    if state.op_owned_at == Some(now) {
        return;
    }
    state.op_owned_at = Some(now);
    for (i, op) in ops.iter().enumerate() {
        let id = op_id(op, i);
        if state.done.contains_key(&key(&id, "prep")) || state.done.contains_key(&key(&id, "end")) {
            continue;
        }
        if op["prepare_from"].is_null() && op["execute_on"].is_null() {
            continue;
        }
        let from = !op["prepare_from"].is_null() && holds(state, tools, &op["prepare_from"], now);
        let on = !op["execute_on"].is_null() && holds(state, tools, &op["execute_on"], now);
        if from || on {
            state.done.insert(key(&id, "prep"), now);
        }
    }
    let active: Vec<(String, &Value)> = ops
        .iter()
        .enumerate()
        .map(|(i, op)| (op_id(op, i), op))
        .filter(|(id, _)| started(state, id))
        .collect();
    let mut owned: HashMap<String, String> = HashMap::new();
    if active.is_empty() {
        state.op_owned = owned;
        return;
    }
    let roster = tools
        .call("hoi4_armies", &json!({"threat": false}))
        .unwrap_or(Value::Null);
    for (id, op) in active {
        for name in owned_names(state, plan, op, &roster) {
            owned.entry(name).or_insert_with(|| id.clone());
        }
    }
    state.op_owned = owned;
}

/// The game names an operation owns: its formation and hold armies and their army groups.
fn owned_names(state: &State, plan: &Value, op: &Value, roster: &Value) -> Vec<String> {
    let mut armies: Vec<String> = list(op, "formations")
        .iter()
        .map(|f| resolve(state, text(&f["army"])))
        .collect();
    for hold in list(op, "holds") {
        armies.extend(
            list(hold, "armies")
                .iter()
                .map(|a| resolve(state, text(&a["army"]))),
        );
    }
    armies.retain(|a| !a.is_empty());
    let mut names = armies.clone();
    for a in &armies {
        if let Some(g) = army_row(roster, a).and_then(|r| r["army_group"].as_str()) {
            if !names.iter().any(|n| n == g) {
                names.push(g.to_string());
            }
        }
    }
    let _ = plan;
    names
}

/// A layout role name means the army bound to it, anything else is an army name.
fn resolve(state: &State, hint: &str) -> String {
    state
        .layout
        .get(hint)
        .filter(|a| !a.is_empty())
        .cloned()
        .unwrap_or_else(|| hint.to_string())
}

fn army_row<'a>(roster: &'a Value, name: &str) -> Option<&'a Value> {
    list(roster, "armies")
        .iter()
        .find(|a| text(&a["army"]) == name)
}

fn group_row<'a>(roster: &'a Value, name: &str) -> Option<&'a Value> {
    list(roster, "army_groups")
        .iter()
        .find(|g| text(&g["army_group"]) == name)
}

/// The battle-plan orders of an army or army group (the read-back).
fn orders<'a>(roster: &'a Value, name: &str) -> &'a [Value] {
    army_row(roster, name)
        .or_else(|| group_row(roster, name))
        .map_or(&[], |r| list(r, "plan"))
}

/// Refuses another step's land order on an owned army or army group (None lets it pass). An
/// exercise start drops the owned armies from its list; a merge or detach may bring divisions
/// INTO a formation but never take them out; a commander is given only where none leads.
pub(super) fn guard(
    owned: &HashMap<String, String>,
    tool: &str,
    args: &mut Value,
) -> Option<String> {
    if owned.is_empty() {
        return None;
    }
    let refuse = |name: &str| {
        owned.get(name).map(|op| {
            format!(
                "owned_by_operation: {name} belongs to operation {op} from its prepare to the \
                 enemy's capitulation; no other step orders it"
            )
        })
    };
    match tool {
        "hoi4_army_exercise" => {
            if args["on"] != json!(true) {
                return None;
            }
            let all = strings(args, "armies");
            let kept: Vec<String> = all
                .iter()
                .filter(|a| !owned.contains_key(*a))
                .cloned()
                .collect();
            if kept.is_empty() {
                return all.first().and_then(|a| refuse(a));
            }
            if kept.len() < all.len() {
                args["armies"] = json!(kept);
            }
            None
        }
        "hoi4_army_commander" => {
            if args["reassign"] == json!(true) {
                refuse(text(&args["army"]))
            } else {
                None
            }
        }
        "hoi4_army_merge" => refuse(text(&args["source"])),
        "hoi4_army_detach" => refuse(text(&args["army"])),
        "hoi4_army_group_create" => strings(args, "armies").iter().find_map(|a| refuse(a)),
        t if LAND_ORDERS.contains(&t) => refuse(text(&args["army"])),
        _ => None,
    }
}

/// Drops the owned armies from a `hoi4_armies` read (the war and layout steps never see them).
pub(super) fn hide_owned(state: &State, roster: &mut Value) {
    if state.op_owned.is_empty() {
        return;
    }
    if let Some(rows) = roster["armies"].as_array_mut() {
        rows.retain(|a| !state.op_owned.contains_key(text(&a["army"])));
    }
}

/// The units of an operation by role, with the game name of its group (None when the line
/// armies stand in no group yet).
fn units(state: &State, plan: &Value, op: &Value, roster: &Value) -> (Vec<Unit>, Option<String>) {
    let mut out = Vec::new();
    let mut group: Option<String> = None;
    for f in list(op, "formations") {
        let name = resolve(state, text(&f["army"]));
        if name.is_empty() {
            continue;
        }
        let role = match text(&f["role"]) {
            "mobile" => Role::Mobile,
            "wing" => Role::Wing,
            _ => Role::Line,
        };
        if role == Role::Line && group.is_none() {
            group = army_row(roster, &name)
                .and_then(|r| r["army_group"].as_str())
                .map(str::to_string);
        }
        let (front, offensive) = if role == Role::Line {
            (&Value::Null, &Value::Null)
        } else {
            (&f["front"], &f["offensive"])
        };
        out.push(unit(name, role, front, offensive));
    }
    if let Some(g) = &group {
        out.push(unit(g.clone(), Role::Group, &op["front"], &op["offensive"]));
    }
    for hold in list(op, "holds") {
        let armies: Vec<String> = list(hold, "armies")
            .iter()
            .map(|a| resolve(state, text(&a["army"])))
            .filter(|a| !a.is_empty())
            .collect();
        // a hold of one army fronts with that army; of several, with their group
        let name = if armies.len() == 1 {
            armies.first().cloned()
        } else {
            armies.iter().find_map(|a| {
                army_row(roster, a)?["army_group"]
                    .as_str()
                    .map(str::to_string)
            })
        };
        let Some(name) = name else { continue };
        let facing = strings(hold, "facing");
        let front = json!({"enemy": facing.first().cloned().unwrap_or_default(),
                           "own_states": hold["states"], "click_state": hold["click_state"]});
        out.push(unit(name, Role::Hold, &front, &Value::Null));
    }
    let _ = plan;
    (out, group)
}

fn unit(name: String, role: Role, front: &Value, offensive: &Value) -> Unit {
    let line = ints(&offensive["target_line"]);
    let axis = ints(&offensive["axis"]);
    let anchor = axis
        .iter()
        .rev()
        .find(|s| line.contains(s))
        .copied()
        .or_else(|| line.get(line.len() / 2).copied());
    Unit {
        name,
        role,
        enemy: text(&front["enemy"]).to_string(),
        own_states: ints(&front["own_states"]),
        click: front["click_state"].as_i64(),
        line,
        anchor,
    }
}

fn attacker(u: &Unit) -> bool {
    matches!(u.role, Role::Group | Role::Mobile | Role::Wing)
}

fn has_front(roster: &Value, name: &str) -> bool {
    orders(roster, name)
        .iter()
        .any(|o| text(&o["kind"]) == "front")
}

/// The offensives of a unit's fronts.
fn offensives<'a>(roster: &'a Value, name: &str) -> Vec<&'a Value> {
    orders(roster, name)
        .iter()
        .flat_map(|o| list(o, "offensives").iter())
        .collect()
}

/// Whether an offensive of the unit aims at `line` (its target covers COVER of the line's
/// states; a one-state line must be in the target).
fn covers(roster: &Value, name: &str, line: &[i64]) -> bool {
    if line.is_empty() {
        return !offensives(roster, name).is_empty();
    }
    offensives(roster, name).iter().any(|off| {
        let target: HashSet<i64> = ints(&off["target"]["states"]).into_iter().collect();
        let hit = line.iter().filter(|s| target.contains(s)).count();
        hit as f64 >= (line.len() as f64 * COVER).ceil().max(1.0)
    })
}

/// The executing flag of the unit's offensive that covers `line` (None when none covers it).
fn arrow_executing(roster: &Value, name: &str, line: &[i64]) -> Option<bool> {
    let need = (line.len() as f64 * COVER).ceil().max(1.0);
    let mut found = None;
    for off in offensives(roster, name) {
        let target: HashSet<i64> = ints(&off["target"]["states"]).into_iter().collect();
        let hit = line.iter().filter(|s| target.contains(s)).count();
        if line.is_empty() || hit as f64 >= need {
            if off["executing"] == json!(true) {
                return Some(true);
            }
            found = Some(false);
        }
    }
    found
}

fn executing(roster: &Value, name: &str) -> bool {
    if army_row(roster, name).is_some_and(|r| r["plan_executing"] == json!(true)) {
        return true;
    }
    orders(roster, name).iter().any(|o| {
        o["executing"] == json!(true)
            || list(o, "offensives")
                .iter()
                .any(|off| off["executing"] == json!(true))
    })
}

/// The state a front order clicks near: the plan's `click_state`, else the enemy's border state
/// nearest the middle of the own states (map centres when known), else the first own state.
fn click_state(state: &State, wars: &Value, u: &Unit) -> Option<i64> {
    if u.click.is_some() {
        return u.click;
    }
    let theirs: Vec<i64> = list(wars, "borders")
        .iter()
        .filter(|b| text(&b["neighbour_tag"]) == u.enemy)
        .flat_map(|b| ints(&b["neighbour_states"]))
        .collect();
    let centre = |ids: &[i64]| -> Option<(f64, f64)> {
        let pts: Vec<(f64, f64)> = ids
            .iter()
            .filter_map(|s| state.centers.get(&(*s as i32)).copied())
            .collect();
        (!pts.is_empty()).then(|| {
            let n = pts.len() as f64;
            (
                pts.iter().map(|p| p.0).sum::<f64>() / n,
                pts.iter().map(|p| p.1).sum::<f64>() / n,
            )
        })
    };
    if let Some((x, z)) = centre(&u.own_states) {
        if let Some(best) = theirs
            .iter()
            .filter_map(|s| state.centers.get(&(*s as i32)).map(|p| (*s, *p)))
            .min_by(|a, b| {
                let d = |p: (f64, f64)| (p.0 - x).powi(2) + (p.1 - z).powi(2);
                d(a.1).total_cmp(&d(b.1))
            })
        {
            return Some(best.0);
        }
    }
    theirs
        .first()
        .copied()
        .or_else(|| u.own_states.first().copied())
        .or_else(|| u.line.first().copied())
}

/// Map centres of the operation's enemies' states (read once per enemy).
fn learn_centres(state: &mut State, tools: &dyn Tools, tags: &[String]) {
    for tag in tags {
        let k = format!("op_centres:{tag}");
        if tag.is_empty() || state.done.contains_key(&k) {
            continue;
        }
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
            state.done.insert(k, 0);
        }
    }
}

/// One operation order through the ledger; the line names the operation and the unit.
fn order(
    state: &mut State,
    tools: &dyn Tools,
    lease: &str,
    id: &str,
    what: &str,
    tool: &str,
    args: Value,
) -> Result<String> {
    let (ok, why) = state.act(tools, lease, tool, args, "op")?;
    let line = format!("operation {id}: {what}: {ok} {why}");
    state.op_notes.insert(id.to_string(), line.clone());
    Ok(line)
}

fn offensive_args(name: &str, line: &[i64], anchor: Option<i64>) -> Value {
    if line.len() >= 2 {
        let states: Vec<i64> = line.iter().take(12).copied().collect();
        json!({"army": name, "state_id": anchor.filter(|a| line.contains(a)).unwrap_or(states[states.len() / 2]),
               "states": states})
    } else {
        json!({"army": name, "state_id": line[0]})
    }
}

/// The operation step: at most one order per call (the war burst calls it again), None when no
/// operation has anything to do.
pub(super) fn step(
    state: &mut State,
    tools: &dyn Tools,
    plan: &Value,
    lease: &str,
    now: GameHour,
) -> Result<Option<String>> {
    let ops = list(plan, "operations");
    if ops.is_empty() || state.skipped(now, "opstep", "x") {
        return Ok(None);
    }
    refresh(state, tools, plan, now);
    let active: Vec<(String, Value)> = ops
        .iter()
        .enumerate()
        .map(|(i, op)| (op_id(op, i), op.clone()))
        .filter(|(id, _)| started(state, id))
        .collect();
    if active.is_empty() {
        return Ok(None);
    }
    let roster = tools.call("hoi4_armies", &json!({"threat": false}))?;
    let wars = tools.call("hoi4_wars", &json!({}))?;
    for (id, op) in &active {
        state.op_acting = true;
        let out = one(state, tools, plan, id, op, &roster, &wars, lease, now);
        state.op_acting = false;
        if let Some(line) = out? {
            return Ok(Some(line));
        }
    }
    // nothing differs: look again in six game hours (a declaration or a loss wakes it sooner
    // through `wake`)
    state.skip(now, "opstep", "x", 6);
    Ok(None)
}

/// Ends the quiet timer: the next call of the step looks at once (after a declaration).
pub(super) fn wake(state: &mut State) {
    state.bad.remove(&super::skip_key("opstep", "x"));
    state.op_owned_at = None;
}

#[allow(clippy::too_many_arguments)]
fn one(
    state: &mut State,
    tools: &dyn Tools,
    plan: &Value,
    id: &str,
    op: &Value,
    roster: &Value,
    wars: &Value,
    lease: &str,
    now: GameHour,
) -> Result<Option<String>> {
    let enemies = strings(op, "enemies");
    let at_war: Vec<String> = list(&wars["wars"], "enemies")
        .iter()
        .filter_map(|t| t.as_str().or_else(|| t["tag"].as_str()).map(str::to_string))
        .collect();
    let executed = state.done.contains_key(&key(id, "exec"));
    let (units, group) = units(state, plan, op, roster);
    if executed {
        return advance(
            state, tools, plan, id, op, &units, roster, &enemies, &at_war, lease, now,
        );
    }
    if let Some(line) = prepare(
        state,
        tools,
        plan,
        id,
        op,
        &units,
        group.as_deref(),
        roster,
        wars,
        lease,
        now,
    )? {
        return Ok(Some(line));
    }
    if op["execute_on"].is_null() || !holds(state, tools, &op["execute_on"], now) {
        state.op_notes.insert(
            id.to_string(),
            format!("operation {id}: prepared, waits for execute_on"),
        );
        return Ok(None);
    }
    // EXECUTE: every attacker, never a hold
    for u in units.iter().filter(|u| attacker(u)) {
        if executing(roster, &u.name) || offensives(roster, &u.name).is_empty() {
            continue;
        }
        if state.skipped(now, "opexec", &u.name) {
            continue;
        }
        state.skip(now, "opexec", &u.name, 3);
        let line = order(
            state,
            tools,
            lease,
            id,
            &format!("execute {}", u.name),
            "hoi4_army_plan",
            json!({"army": u.name, "execute": true}),
        )?;
        return Ok(Some(line));
    }
    let attackers: Vec<&Unit> = units
        .iter()
        .filter(|u| attacker(u) && !offensives(roster, &u.name).is_empty())
        .collect();
    if attackers.iter().all(|u| executing(roster, &u.name)) && !attackers.is_empty() {
        state.done.insert(key(id, "exec"), now);
        state.done.insert(key(id, "line"), 0);
        state.done.insert(key(id, "moved"), now);
        state.op_notes.insert(
            id.to_string(),
            format!(
                "operation {id}: executing ({})",
                attackers
                    .iter()
                    .map(|u| u.name.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
        );
    }
    Ok(None)
}

#[allow(clippy::too_many_arguments)]
fn prepare(
    state: &mut State,
    tools: &dyn Tools,
    plan: &Value,
    id: &str,
    op: &Value,
    units: &[Unit],
    group: Option<&str>,
    roster: &Value,
    wars: &Value,
    lease: &str,
    now: GameHour,
) -> Result<Option<String>> {
    // exercising divisions lose organisation: an owned army stops before its war
    let training: Vec<String> = units
        .iter()
        .filter(|u| u.role != Role::Group)
        .filter(|u| army_row(roster, &u.name).is_some_and(|r| r["exercising"] == json!(true)))
        .map(|u| u.name.clone())
        .collect();
    if !training.is_empty() && !state.skipped(now, "opex", id) {
        state.skip(now, "opex", id, DAY);
        let line = order(
            state,
            tools,
            lease,
            id,
            &format!("stop exercises of {}", training.join(", ")),
            "hoi4_army_exercise",
            json!({"armies": training, "on": false}),
        )?;
        return Ok(Some(line));
    }
    // the group: formed when its line armies stand in none (never regrouped)
    let line_armies: Vec<&Unit> = units.iter().filter(|u| u.role == Role::Line).collect();
    if group.is_none() && line_armies.len() >= 2 && !state.skipped(now, "opgrp", id) {
        let mut names: Vec<String> = line_armies.iter().map(|u| u.name.clone()).collect();
        let planned = list(plan, "army_group")
            .iter()
            .find(|g| item_id(g).as_deref() == op["group"].as_str());
        for hint in planned.map(|g| strings(g, "armies")).unwrap_or_default() {
            let n = resolve(state, &hint);
            let free = army_row(roster, &n).is_some_and(|r| r["army_group"].is_null());
            if free && !names.contains(&n) {
                names.push(n);
            }
        }
        names.retain(|n| army_row(roster, n).is_some_and(|r| r["army_group"].is_null()));
        if names.len() >= 2 {
            state.skip(now, "opgrp", id, 2 * DAY);
            let line = order(
                state,
                tools,
                lease,
                id,
                &format!("form the group of {}", names.join(", ")),
                "hoi4_army_group_create",
                json!({"armies": names}),
            )?;
            return Ok(Some(line));
        }
    }
    // a formation's named commander where none leads
    for f in list(op, "formations") {
        let name = resolve(state, text(&f["army"]));
        let Some(general) = f["commander"].as_str().filter(|g| !g.is_empty()) else {
            continue;
        };
        let leaderless = army_row(roster, &name).is_some_and(|r| r["commander"].is_null());
        if leaderless && !state.skipped(now, "opgen", &name) {
            state.skip(now, "opgen", &name, 7 * DAY);
            let line = order(
                state,
                tools,
                lease,
                id,
                &format!("{general} commands {name}"),
                "hoi4_army_commander",
                json!({"army": name, "general": general}),
            )?;
            return Ok(Some(line));
        }
    }
    let mut tags: Vec<String> = units.iter().map(|u| u.enemy.clone()).collect();
    tags.push(player(tools, plan));
    learn_centres(state, tools, &tags);
    // 1. a line army carries no orders of its own (it would ignore the group front): cleared
    //    once, before the group's front stands
    if let Some(g) = group {
        if !has_front(roster, g) {
            for u in units.iter().filter(|u| u.role == Role::Line) {
                let k = key(id, &format!("clr:{}", u.name));
                if orders(roster, &u.name).is_empty() || state.done.contains_key(&k) {
                    continue;
                }
                state.done.insert(k, now);
                let line = order(
                    state,
                    tools,
                    lease,
                    id,
                    &format!("clear the own orders of line army {}", u.name),
                    "hoi4_army_plan",
                    json!({"army": u.name, "clear_orders": true}),
                )?;
                return Ok(Some(line));
            }
        }
    }
    // 2-4. mobile and wing armies, then the group, then the holds
    let mut ordered: Vec<&Unit> = units
        .iter()
        .filter(|u| matches!(u.role, Role::Mobile | Role::Wing))
        .collect();
    ordered.extend(units.iter().filter(|u| u.role == Role::Group));
    ordered.extend(units.iter().filter(|u| u.role == Role::Hold));
    for u in ordered {
        if let Some(line) = reconcile(state, tools, id, u, &u.line, roster, wars, lease, now, true)?
        {
            return Ok(Some(line));
        }
    }
    // 5. execution mode, once per attacker
    if let Some(mode) = op["execution"]
        .as_str()
        .filter(|m| matches!(*m, "careful" | "rush"))
    {
        for u in units.iter().filter(|u| attacker(u)) {
            let k = key(id, &format!("mode:{}", u.name));
            if state.done.contains_key(&k) || orders(roster, &u.name).is_empty() {
                continue;
            }
            state.done.insert(k, now);
            let line = order(
                state,
                tools,
                lease,
                id,
                &format!("{mode} mode for {}", u.name),
                "hoi4_army_plan",
                json!({"army": u.name, "mode": mode}),
            )?;
            return Ok(Some(line));
        }
    }
    Ok(None)
}

/// One unit's wanted orders against the read-back: a front, then (attackers) an offensive that
/// covers `line`. A wrong offensive is cleared and drawn again at most REDRAWS times per line
/// (in peace; at war a missed line is drawn again over the standing front).
#[allow(clippy::too_many_arguments)]
fn reconcile(
    state: &mut State,
    tools: &dyn Tools,
    id: &str,
    u: &Unit,
    line: &[i64],
    roster: &Value,
    wars: &Value,
    lease: &str,
    now: GameHour,
    peace: bool,
) -> Result<Option<String>> {
    if state.skipped(now, "opdraw", &u.name) {
        return Ok(None);
    }
    if army_row(roster, &u.name).is_none() && group_row(roster, &u.name).is_none() {
        state.op_notes.insert(
            format!("{id}:{}", u.name),
            format!("{} is not on the map", u.name),
        );
        return Ok(None);
    }
    if !has_front(roster, &u.name) {
        // at war the front is clicked at the line to take: the nearest highlighted border there
        // is the contact line, not the pre-war border
        let at_line = (!peace).then(|| line.first().copied()).flatten();
        let Some(click) = at_line
            .or_else(|| click_state(state, wars, u))
            .or_else(|| line.first().copied())
        else {
            return Ok(None);
        };
        let fk = key(id, &format!("fronts:{}", u.name));
        let fronts = state.done.get(&fk).copied().unwrap_or(0);
        if fronts >= 3 * REDRAWS {
            state.op_notes.insert(
                format!("{id}:{}", u.name),
                format!("{}: no front reads back after {fronts} draws", u.name),
            );
            return Ok(None);
        }
        state.done.insert(fk, fronts + 1);
        let line = order(
            state,
            tools,
            lease,
            id,
            &format!("front of {} near state {click}", u.name),
            "hoi4_army_front",
            json!({"army": u.name, "state_id": click}),
        )?;
        if state.last_ok == Some(false) {
            state.skip(now, "opdraw", &u.name, 2);
        }
        return Ok(Some(line));
    }
    if u.role == Role::Hold || line.is_empty() {
        return Ok(None);
    }
    if covers(roster, &u.name, line) {
        return Ok(None);
    }
    let tag = line
        .iter()
        .map(i64::to_string)
        .collect::<Vec<_>>()
        .join("-");
    let stale = !offensives(roster, &u.name).is_empty();
    let k = key(id, &format!("redraw:{}:{tag}", u.name));
    let redraws = state.done.get(&k).copied().unwrap_or(0);
    if stale && peace {
        if redraws >= REDRAWS {
            state.op_notes.insert(
                format!("{id}:{}", u.name),
                format!(
                    "{}: offensive misses the line {tag} after {REDRAWS} redraws",
                    u.name
                ),
            );
            return Ok(None);
        }
        state.done.insert(k, redraws + 1);
        let line = order(
            state,
            tools,
            lease,
            id,
            &format!("clear {} (offensive misses the line {tag})", u.name),
            "hoi4_army_plan",
            json!({"army": u.name, "clear_orders": true}),
        )?;
        return Ok(Some(line));
    }
    let tries = key(id, &format!("draw:{}:{tag}", u.name));
    let n = state.done.get(&tries).copied().unwrap_or(0);
    if !peace && n == REDRAWS {
        // at war the standing front may no longer take a new line (the game split the group
        // front, e1g1): cleared once, the next calls draw front and line afresh
        state.done.insert(tries, n + 1);
        let line = order(
            state,
            tools,
            lease,
            id,
            &format!("clear {} (the new line {tag} would not draw)", u.name),
            "hoi4_army_plan",
            json!({"army": u.name, "clear_orders": true}),
        )?;
        return Ok(Some(line));
    }
    if n > 2 * REDRAWS {
        state.op_notes.insert(
            format!("{id}:{}", u.name),
            format!(
                "{}: the line {tag} does not read back after {n} draws",
                u.name
            ),
        );
        return Ok(None);
    }
    state.done.insert(tries, n + 1);
    let line = order(
        state,
        tools,
        lease,
        id,
        &format!("offensive of {} through {tag}", u.name),
        "hoi4_army_offensive",
        offensive_args(&u.name, line, u.anchor),
    )?;
    if state.last_ok == Some(false) {
        state.skip(now, "opdraw", &u.name, 2);
    }
    Ok(Some(line))
}

/// The states of `ids` the player controls.
fn held(tools: &dyn Tools, me: &str, ids: &[i64]) -> Vec<i64> {
    if ids.is_empty() || me.is_empty() {
        return Vec::new();
    }
    let Ok(read) = tools.call("hoi4_states", &json!({"query": me, "ids": ids})) else {
        return Vec::new();
    };
    list(&read, "states")
        .iter()
        .filter(|r| text(&r["controller"]) == me)
        .filter_map(|r| r["state_id"].as_i64())
        .filter(|s| ids.contains(s))
        .collect()
}

fn player(tools: &dyn Tools, plan: &Value) -> String {
    if let Some(me) = plan["war"]["me"].as_str() {
        return me.to_string();
    }
    tools
        .call("hoi4_overview", &json!({}))
        .map(|o| text(&o["player"]).to_string())
        .unwrap_or_default()
}

/// The operation's current line: 0 = the group offensive, k = phase line k, beyond the phase
/// lines one state chosen at the time (kept as `auto:<k>`).
fn current_line(state: &State, id: &str, op: &Value) -> (i64, Vec<i64>) {
    let k = state.done.get(&key(id, "line")).copied().unwrap_or(0);
    let phases = list(op, "phase_lines");
    let line = if k == 0 {
        ints(&op["offensive"]["target_line"])
    } else if let Some(p) = phases.get(k as usize - 1) {
        ints(&p["target_line"])
    } else {
        state
            .done
            .get(&key(id, &format!("auto:{k}")))
            .map(|s| vec![*s])
            .unwrap_or_default()
    };
    (k, line)
}

/// The next enemy state to draw a line on: most victory points (plan vp_targets) per distance
/// from the line just reached; the enemy's states as hoi4_states lists them.
fn next_state(
    state: &State,
    tools: &dyn Tools,
    plan: &Value,
    enemies: &[String],
    from: &[i64],
) -> Option<i64> {
    let mut vp: HashMap<i64, f64> = HashMap::new();
    for v in list(plan, "vp_targets") {
        if let Some(s) = v["state"].as_i64() {
            *vp.entry(s).or_default() += v["vp"].as_f64().unwrap_or(1.0);
        }
    }
    let pts: Vec<(f64, f64)> = from
        .iter()
        .filter_map(|s| state.centers.get(&(*s as i32)).copied())
        .collect();
    let mut best: Option<(f64, i64)> = None;
    for tag in enemies {
        let Ok(read) = tools.call("hoi4_states", &json!({"query": tag})) else {
            continue;
        };
        for row in list(&read, "states") {
            if text(&row["controller"]) != tag {
                continue;
            }
            let Some(sid) = row["state_id"].as_i64() else {
                continue;
            };
            let worth = vp
                .get(&sid)
                .copied()
                .or_else(|| row["victory_points_total"].as_f64())
                .unwrap_or(0.0)
                + 1.0;
            let dist = match (row["x"].as_f64(), row["z"].as_f64()) {
                (Some(x), Some(z)) if !pts.is_empty() => pts
                    .iter()
                    .map(|p| ((p.0 - x).powi(2) + (p.1 - z).powi(2)).sqrt())
                    .fold(f64::MAX, f64::min),
                _ => 1000.0,
            };
            let score = worth / (dist + 100.0);
            if best.is_none_or(|(b, _)| score > b) {
                best = Some((score, sid));
            }
        }
    }
    best.map(|(_, s)| s)
}

#[allow(clippy::too_many_arguments)]
fn advance(
    state: &mut State,
    tools: &dyn Tools,
    plan: &Value,
    id: &str,
    op: &Value,
    units: &[Unit],
    roster: &Value,
    enemies: &[String],
    at_war: &[String],
    lease: &str,
    now: GameHour,
) -> Result<Option<String>> {
    // FINISH: every enemy out of the war
    if !enemies.is_empty() && !enemies.iter().any(|e| at_war.contains(e)) {
        for u in units.iter().filter(|u| attacker(u)) {
            let k = key(id, &format!("fin:{}", u.name));
            if state.done.contains_key(&k) || orders(roster, &u.name).is_empty() {
                continue;
            }
            state.done.insert(k, now);
            let line = order(
                state,
                tools,
                lease,
                id,
                &format!("enemy out of the war: clear {}", u.name),
                "hoi4_army_plan",
                json!({"army": u.name, "clear_orders": true}),
            )?;
            return Ok(Some(line));
        }
        state.done.insert(key(id, "end"), now);
        let next = op["on_capitulation"]["next"].as_str().unwrap_or("");
        if !next.is_empty() {
            state.done.entry(key(next, "prep")).or_insert(now);
        }
        state.op_owned_at = None;
        let line = format!(
            "operation {id}: done ({} out of the war){}",
            enemies.join(", "),
            if next.is_empty() {
                String::new()
            } else {
                format!("; {next} prepares")
            }
        );
        state.op_notes.insert(id.to_string(), line.clone());
        return Ok(Some(line));
    }
    let (k, line) = current_line(state, id, op);
    // ADVANCE: count the line every CHECK_HOURS
    if !state.skipped(now, "opweek", id) {
        state.skip(now, "opweek", id, CHECK_HOURS);
        let me = player(tools, plan);
        let got = held(tools, &me, &line);
        let before = state.done.get(&key(id, "held")).copied().unwrap_or(0);
        if got.len() as i64 > before {
            state.done.insert(key(id, "held"), got.len() as i64);
            state.done.insert(key(id, "moved"), now);
        }
        let moved = state.done.get(&key(id, "moved")).copied().unwrap_or(now);
        let stall = op["stall_days"].as_i64().unwrap_or(STALL_DAYS) * DAY;
        let taken = !line.is_empty() && got.len() == line.len();
        let stalled = now - moved >= stall;
        state.op_notes.insert(
            id.to_string(),
            format!(
                "operation {id}: line {k} holds {}/{} states, day {}",
                got.len(),
                line.len(),
                (now - state.done.get(&key(id, "exec")).copied().unwrap_or(now)) / DAY
            ),
        );
        let due = list(op, "phase_lines")
            .get(k as usize)
            .is_some_and(|p| !p["when"].is_null())
            && {
                let when = list(op, "phase_lines")[k as usize]["when"].clone();
                holds(state, tools, &when, now)
            };
        if taken || stalled || due || line.is_empty() {
            let phases = list(op, "phase_lines");
            let mut next = k + 1;
            let mut chosen: Option<Vec<i64>> = None;
            while (next as usize) <= phases.len() {
                let want = ints(&phases[next as usize - 1]["target_line"]);
                let have = held(tools, &me, &want);
                let rest: Vec<i64> = want.into_iter().filter(|s| !have.contains(s)).collect();
                if !rest.is_empty() {
                    chosen = Some(rest);
                    break;
                }
                next += 1;
            }
            if chosen.is_none() {
                let from = if got.is_empty() {
                    line.clone()
                } else {
                    got.clone()
                };
                if let Some(s) = next_state(state, tools, plan, enemies, &from) {
                    if line != [s] {
                        state.done.insert(key(id, &format!("auto:{next}")), s);
                        chosen = Some(vec![s]);
                    }
                }
            }
            if let Some(rest) = chosen {
                state.done.insert(key(id, "line"), next);
                state.done.insert(key(id, "held"), 0);
                state.done.insert(key(id, "moved"), now);
                let why = if due {
                    "phase line due"
                } else if taken {
                    "line taken"
                } else if stalled {
                    "line stalled"
                } else {
                    "no line"
                };
                let line = format!(
                    "operation {id}: {why} ({}/{} states): next line {next} through {:?}",
                    got.len(),
                    line.len(),
                    rest
                );
                state.op_notes.insert(id.to_string(), line.clone());
                return Ok(Some(line));
            }
        }
    }
    // the wanted line of each attacker: the states of the operation's line still to take; a
    // mobile or wing army keeps its own line until that is held
    let me = player(tools, plan);
    let still = |line: &[i64]| -> Vec<i64> {
        let have = held(tools, &me, line);
        let rest: Vec<i64> = line.iter().copied().filter(|s| !have.contains(s)).collect();
        if rest.is_empty() {
            line.to_vec()
        } else {
            rest
        }
    };
    let op_line = still(&line);
    for u in units.iter().filter(|u| attacker(u)) {
        let own_line_held = u.line.is_empty() || held(tools, &me, &u.line).len() == u.line.len();
        let wanted: Vec<i64> = if u.role == Role::Group || own_line_held {
            op_line.clone()
        } else {
            still(&u.line)
        };
        if let Some(line) = reconcile(
            state,
            tools,
            id,
            u,
            &wanted,
            roster,
            &Value::Null,
            lease,
            now,
            false,
        )? {
            return Ok(Some(line));
        }
        // REPAIR: the offensive on the wanted line executes (a new arrow on an executing front
        // starts unexecuted: the plan is stopped and executed again)
        let arrow = arrow_executing(roster, &u.name, &wanted);
        if arrow == Some(false) && !state.skipped(now, "opexec", &u.name) {
            state.skip(now, "opexec", &u.name, DAY / 4);
            let rk = key(id, &format!("restart:{}:{k}", u.name));
            let restarts = state.done.get(&rk).copied().unwrap_or(0);
            let stop_first = executing(roster, &u.name) && restarts < REDRAWS;
            if stop_first {
                state.done.insert(rk, restarts + 1);
            }
            let line = order(
                state,
                tools,
                lease,
                id,
                &if stop_first {
                    format!(
                        "{}: the new line does not execute: stop, then execute",
                        u.name
                    )
                } else {
                    format!("{} is not executing: execute", u.name)
                },
                "hoi4_army_plan",
                json!({"army": u.name, "execute": !stop_first}),
            )?;
            if stop_first {
                // the execute follows in the next call
                state.bad.remove(&super::skip_key("opexec", &u.name));
            }
            return Ok(Some(line));
        }
    }
    // holds keep their fronts (never executed)
    for u in units.iter().filter(|u| u.role == Role::Hold) {
        if let Some(line) = reconcile(
            state,
            tools,
            id,
            u,
            &[],
            roster,
            &Value::Null,
            lease,
            now,
            false,
        )? {
            return Ok(Some(line));
        }
    }
    Ok(None)
}

/// The operations' status lines for the digest.
pub(super) fn digest(state: &State) -> Option<Value> {
    if state.op_notes.is_empty() && state.op_owned.is_empty() {
        return None;
    }
    let mut owned: Vec<(&String, &String)> = state.op_owned.iter().collect();
    owned.sort();
    Some(
        json!({"notes": state.op_notes, "owned": owned.into_iter().map(|(a, o)| json!([a, o])).collect::<Vec<_>>()}),
    )
}

#[cfg(test)]
#[path = "plan_operation/tests.rs"]
mod tests;
