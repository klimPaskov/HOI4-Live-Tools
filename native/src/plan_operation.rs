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
    /// a mobile formation with `drive: "strike"`: no battle plan; at war the strike engine
    /// moves it as one column on its line's victory points (plan_strike.rs)
    Strike,
    Wing,
    Hold,
}

/// One unit the operation orders: an army or an army group, by its game name.
#[derive(Clone, Debug)]
struct Unit {
    name: String,
    role: Role,
    /// the armies of a hold group (cleared of own orders: an army with own orders ignores its
    /// group's front, o1a: Armee West Mitte stayed on the Polish border)
    members: Vec<String>,
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
        publish(&state.op_owned);
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
        publish(&owned);
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
    publish(&owned);
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

/// The owned names as the executor last worked them out, for the server's dispatch of calls
/// that do not come from the executor (the player's): the executor's own orders pass
/// `State::act`'s guard, a player's order on an owned army was let through (g17i, 18 Nov 2026:
/// the brief alone forbade it). Kept with the wall time it was set; a snapshot older than
/// `OWNED_FRESH` (an executor switched off) refuses nothing.
static OWNED_NOW: std::sync::Mutex<Option<(std::time::Instant, HashMap<String, String>)>> =
    std::sync::Mutex::new(None);
const OWNED_FRESH: std::time::Duration = std::time::Duration::from_secs(600);

fn publish(owned: &HashMap<String, String>) {
    // unit tests run side by side in one process: only a test that asks for it sets the snapshot
    #[cfg(test)]
    if !owned.contains_key("__test_publish__") {
        return;
    }
    if let Ok(mut slot) = OWNED_NOW.lock() {
        *slot = Some((std::time::Instant::now(), owned.clone()));
    }
}

/// [`guard`] for a call from outside the executor: Some(refusal) for a land order on an army or
/// army group a running operation owns (an exercise list loses its owned armies instead).
pub(crate) fn guard_outside(tool: &str, args: &mut Value) -> Option<String> {
    let slot = OWNED_NOW.lock().ok()?;
    let (at, owned) = slot.as_ref()?;
    if at.elapsed() > OWNED_FRESH {
        return None;
    }
    guard(owned, tool, args)
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
            "mobile" if text(&f["drive"]) == "strike" => Role::Strike,
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
        let mut hold = unit(name, Role::Hold, &front, &Value::Null);
        if armies.len() > 1 {
            hold.members = armies;
        }
        out.push(hold);
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
        members: Vec::new(),
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
fn covers(roster: &Value, u: &Unit, line: &[i64]) -> bool {
    arrow_executing(roster, u, line).is_some()
}

/// States of `line` an offensive's target must hold: COVER of the line for the group (its one
/// country-wide line), one for a mobile or wing army (its stroke lands where the game draws it:
/// Armee Nord's line through 807-98-97-95 reads back in 97/95/96, e1b3 and o1a).
fn need(u: &Unit, line: &[i64]) -> usize {
    if u.role == Role::Group {
        (line.len() as f64 * COVER).ceil().max(1.0) as usize
    } else {
        1
    }
}

/// The executing flag of the unit's offensive that covers `line` (None when none covers it).
fn arrow_executing(roster: &Value, u: &Unit, line: &[i64]) -> Option<bool> {
    let need = need(u, line);
    let mut found = None;
    for off in offensives(roster, &u.name) {
        let target: HashSet<i64> = ints(&off["target"]["states"]).into_iter().collect();
        let hit = line.iter().filter(|s| target.contains(s)).count();
        if line.is_empty() || hit >= need {
            if off["executing"] == json!(true) {
                return Some(true);
            }
            found = Some(false);
        }
    }
    found
}

/// The enemies the player is at war with (hoi4_wars `wars.enemies`).
fn war_tags(wars: &Value) -> Vec<String> {
    list(&wars["wars"], "enemies")
        .iter()
        .filter_map(|t| t.as_str().or_else(|| t["tag"].as_str()).map(str::to_string))
        .collect()
}

/// Whether the unit's offensive draws ran out (for its line or its one-state line).
fn drawn_out(state: &State, id: &str, u: &Unit) -> bool {
    let prefix = key(id, &format!("draw:{}:", u.name));
    state
        .done
        .iter()
        .any(|(k, n)| k.starts_with(&prefix) && *n >= 2 * REDRAWS)
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

/// At war: the enemy state across the contact line (hoi4_wars borders) nearest the middle of
/// `line`; a front clicked there finds the contact line.
fn contact_state(state: &State, wars: &Value, enemy: &str, line: &[i64]) -> Option<i64> {
    let theirs: Vec<i64> = list(wars, "borders")
        .iter()
        .filter(|b| enemy.is_empty() || text(&b["neighbour_tag"]) == enemy)
        .flat_map(|b| ints(&b["neighbour_states"]))
        .collect();
    let pts: Vec<(f64, f64)> = line
        .iter()
        .filter_map(|s| state.centers.get(&(*s as i32)).copied())
        .collect();
    if pts.is_empty() {
        return theirs.first().copied();
    }
    let n = pts.len() as f64;
    let (x, z) = (
        pts.iter().map(|p| p.0).sum::<f64>() / n,
        pts.iter().map(|p| p.1).sum::<f64>() / n,
    );
    theirs
        .iter()
        .filter_map(|s| state.centers.get(&(*s as i32)).map(|p| (*s, *p)))
        .min_by(|a, b| {
            let d = |p: (f64, f64)| (p.0 - x).powi(2) + (p.1 - z).powi(2);
            d(a.1).total_cmp(&d(b.1))
        })
        .map(|(s, _)| s)
        .or_else(|| theirs.first().copied())
}

fn line_tag(line: &[i64]) -> String {
    line.iter()
        .map(i64::to_string)
        .collect::<Vec<_>>()
        .join("-")
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
    if !ok && why.contains("events_open") {
        // an open event holds every order of the operation: the next tick's event pass answers
        // it at once (o1g: germany_mefo_bills.1 in the declaration tick held the attack 7 days)
        state.op_events_now = true;
    }
    let line = format!("operation {id}: {what}: {ok} {why}");
    state.op_notes.insert(id.to_string(), line.clone());
    Ok(line)
}

/// A mobile army's front in peace is a narrow segment: the border between its one own state and
/// the click state only (o1f/o1g: on the whole 26-province border its arrow started over Gdynia
/// whatever the target); every other front takes the whole highlighted border.
fn front_args(u: &Unit, click: i64, peace: bool) -> Value {
    match (u.role, u.own_states.as_slice()) {
        (Role::Mobile, [own]) if peace => {
            json!({"army": u.name, "state_id": click, "from_state": own})
        }
        _ => json!({"army": u.name, "state_id": click}),
    }
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

/// An operation is under way: prepared (its armies own their lines, in peace too) and not
/// finished. The air goes over its regions from the prepare on, so the wings already fly there
/// when the war starts.
pub(super) fn active(state: &State) -> bool {
    state.done.keys().any(|k| {
        k.starts_with("op:")
            && k.ends_with(":prep")
            && !state
                .done
                .contains_key(&format!("{}:end", &k[..k.len() - 5]))
    })
}

/// A running operation against `target` whose prepare has not finished (some front, hold or
/// assembly does not read back yet): its id. The declaration waits for it (o2b, 18 Nov 2026: the
/// war was declared while the West hold's front was still refused, and France took four states).
pub(super) fn unready(state: &State, plan: &Value, target: &str) -> Option<String> {
    list(plan, "operations")
        .iter()
        .enumerate()
        .map(|(i, op)| (op_id(op, i), op))
        .find(|(id, op)| {
            started(state, id)
                && !state.done.contains_key(&key(id, "ready"))
                && strings(op, "enemies").iter().any(|e| e == target)
        })
        .map(|(id, _)| id)
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
    let at_war = war_tags(wars);
    let executed = state.done.contains_key(&key(id, "exec"));
    let (units, group) = units(state, plan, op, roster);
    if executed {
        if let Some(line) = advance(
            state, tools, plan, id, op, &units, roster, wars, &enemies, &at_war, lease, now,
        )? {
            return Ok(Some(line));
        }
        return strikes(
            state, tools, plan, op, &units, roster, &enemies, &at_war, lease, now,
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
    // every order of the prepare reads back (none rests after a failure): a declaration
    // against its enemies may go out
    let resting = state
        .bad
        .iter()
        .any(|(k, until)| *until > now && (k.starts_with("opdraw:") || k.starts_with("opasm:")));
    if !resting {
        state.done.entry(key(id, "ready")).or_insert(now);
    }
    if op["execute_on"].is_null() || !holds(state, tools, &op["execute_on"], now) {
        state.op_notes.insert(
            id.to_string(),
            format!("operation {id}: prepared, waits for execute_on"),
        );
        return Ok(None);
    }
    // EXECUTE: every attacker, never a hold. At war an attacker whose offensive never read back
    // executes its front alone (its divisions attack along the line) rather than stand.
    let front_only = |state: &State, u: &Unit| {
        at_war.contains(&u.enemy) && has_front(roster, &u.name) && drawn_out(state, id, u)
    };
    for u in units.iter().filter(|u| attacker(u)) {
        if executing(roster, &u.name)
            || (offensives(roster, &u.name).is_empty() && !front_only(state, u))
        {
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
        .filter(|u| {
            attacker(u) && (!offensives(roster, &u.name).is_empty() || front_only(state, u))
        })
        .collect();
    if let Some(line) = strikes(
        state, tools, plan, op, &units, roster, &enemies, &at_war, lease, now,
    )? {
        return Ok(Some(line));
    }
    // an attacker still drawing at war (no offensive yet, its draws not out) is waited for: the
    // operation counts as executed only once nothing is left to execute
    let drawing = units.iter().any(|u| {
        attacker(u)
            && at_war.contains(&u.enemy)
            && army_row(roster, &u.name)
                .or_else(|| group_row(roster, &u.name))
                .is_some()
            && !executing(roster, &u.name)
            && offensives(roster, &u.name).is_empty()
            && !drawn_out(state, id, u)
            && state
                .done
                .get(&key(id, &format!("fronts:{}", u.name)))
                .copied()
                .unwrap_or(0)
                < 3 * REDRAWS
    });
    if attackers.iter().all(|u| executing(roster, &u.name)) && !attackers.is_empty() && !drawing {
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

/// The operation's strike formations (`drive: "strike"`) at war: the strike engine moves each as
/// one column, at most `stack` (4) divisions a province, on the victory points of its line's
/// states in order (then the nearest ones), resting by organisation. A segment front could not
/// be drawn for the mobile army (the game took the whole 26-province border every time, beds
/// o1i-o1n and airdbg4-5, 18 Nov 2026), and on the whole border its 15 divisions stood one a
/// province and took nothing in 14 days. The infantry's executed plan follows behind.
#[allow(clippy::too_many_arguments)]
fn strikes(
    state: &mut State,
    tools: &dyn Tools,
    plan: &Value,
    op: &Value,
    units: &[Unit],
    roster: &Value,
    enemies: &[String],
    at_war: &[String],
    lease: &str,
    now: GameHour,
) -> Result<Option<String>> {
    let strikers: Vec<&Unit> = units
        .iter()
        .filter(|u| u.role == Role::Strike && army_row(roster, &u.name).is_some())
        .collect();
    let fighting: HashSet<String> = enemies
        .iter()
        .filter(|e| at_war.contains(e))
        .cloned()
        .collect();
    if strikers.is_empty() || fighting.is_empty() {
        return Ok(None);
    }
    // the largest victory point of every enemy state
    let mut best: HashMap<i64, (i64, f64)> = HashMap::new();
    let mut tags: Vec<&String> = fighting.iter().collect();
    tags.sort();
    for tag in tags {
        let read = tools.call("hoi4_states", &json!({"query": tag}))?;
        for row in list(&read, "states") {
            let Some(sid) = row["state_id"].as_i64() else {
                continue;
            };
            for vp in list(row, "victory_points") {
                let (Some(p), v) = (
                    vp["province"].as_i64(),
                    vp["victory_points"].as_f64().unwrap_or(0.0),
                ) else {
                    continue;
                };
                if best.get(&sid).is_none_or(|b| v > b.1) {
                    best.insert(sid, (p, v));
                }
            }
        }
    }
    let mut rows: Vec<Value> = Vec::new();
    let mut knobs = json!({});
    for u in &strikers {
        for s in &u.line {
            if let Some((p, _)) = best.get(s) {
                rows.push(json!({"army": u.name, "province": p, "path": []}));
            }
        }
        if let Some(f) = list(op, "formations")
            .iter()
            .find(|f| resolve(state, text(&f["army"])) == u.name)
        {
            if f["strike"].is_object() {
                knobs = f["strike"].clone();
            }
        }
    }
    // no posts on covered states: the strike engine would send divisions of the line armies
    // (which the executed plan carries) to hold own states (o2a: Mitte and Süd)
    if knobs.get("cover").is_none() {
        knobs["cover"] = json!(0);
    }
    knobs["way"] = json!(true);
    let mut p = plan.clone();
    p["war"]["strike"] = knobs;
    p["vp_targets"] = json!(rows);
    let reserve: Vec<String> = list(roster, "armies")
        .iter()
        .map(|a| text(&a["army"]).to_string())
        .filter(|n| !strikers.iter().any(|u| &u.name == n))
        .collect();
    let me = player(tools, plan);
    Ok(super::plan_strike::step(state, tools, &p, lease, &me, &fighting, &reserve, now)?.flatten())
}

/// A formation to assemble: its army's game name, the plan's name, {template: count}, the one
/// army it is taken from (a hold part of a wing), else None (every army but the wings), and the
/// states whose divisions it takes first (a hold's own states).
type Mobile = (String, String, Vec<(String, i64)>, Option<String>, Vec<i64>);

/// The assembly of a `mobile` formation: its `divisions` ({template: count}) are gathered out of
/// every other army (a wing keeps its own: it fights on a separate border) with hoi4_army_detach,
/// a source army never emptied; without the army on the map the first detach forms it and the new
/// army is renamed to the formation's name. Bed 1 (bp2): the 8 Panzer and 12 motorised divisions
/// stood one to four in every army, the hold on the French border included. A hold army with
/// `divisions` is assembled the same way, its own states' divisions first, and never out of
/// what a line formation's own `divisions` keep (it26b, 18 Nov 2026: Italy's North Guard and
/// Jubaland Guard never formed and Jubaland was lost).
#[allow(clippy::too_many_arguments)]
fn assemble(
    state: &mut State,
    tools: &dyn Tools,
    id: &str,
    op: &Value,
    units: &[Unit],
    roster: &Value,
    lease: &str,
    now: GameHour,
) -> Result<Option<String>> {
    let wings: HashSet<&str> = units
        .iter()
        .filter(|u| u.role == Role::Wing)
        .map(|u| u.name.as_str())
        .collect();
    let want_of = |f: &Value| -> Option<Vec<(String, i64)>> {
        Some(
            f["divisions"]
                .as_object()?
                .iter()
                .filter_map(|(t, n)| Some((t.clone(), n.as_i64()?)))
                .collect(),
        )
    };
    // a hold army with `from` (a holding part of an attacking wing that has its own land behind
    // it: o1n, Armee Nord attacked Danzig and lost East Prussia's states 5 and 188) is assembled
    // out of that one army; a hold army without `from` out of the others, its states first
    let hold_parts: Vec<(&Value, Vec<i64>)> = list(op, "holds")
        .iter()
        .flat_map(|h| {
            let near: Vec<i64> = list(h, "states").iter().filter_map(Value::as_i64).collect();
            list(h, "armies").iter().map(move |a| (a, near.clone()))
        })
        .collect();
    let mobiles: Vec<Mobile> = list(op, "formations")
        .iter()
        .filter(|f| text(&f["role"]) == "mobile")
        .map(|f| (f, Vec::new()))
        .chain(hold_parts)
        .filter_map(|(f, near)| {
            let want = want_of(f)?;
            let hint = text(&f["army"]).to_string();
            let from = f["from"]
                .as_str()
                .filter(|s| !s.is_empty())
                .map(|s| resolve(state, s));
            Some((resolve(state, &hint), hint, want, from, near))
        })
        .filter(|(name, _, want, _, _)| !name.is_empty() && !want.is_empty())
        .collect();
    // what each line formation keeps of its own `divisions` (a hold never takes those)
    let mut kept: HashMap<(String, String), i64> = HashMap::new();
    for f in list(op, "formations")
        .iter()
        .filter(|f| !matches!(text(&f["role"]), "mobile" | "wing"))
    {
        let army = resolve(state, text(&f["army"]));
        for (t, n) in want_of(f).unwrap_or_default() {
            *kept.entry((army.clone(), t)).or_insert(0) += n;
        }
    }
    let mobile_names: HashSet<String> = mobiles.iter().map(|m| m.0.clone()).collect();
    for (name, hint, want, from, near) in &mobiles {
        // a formed army that kept the game's name: renamed once a day, at most three times; the
        // operation orders it under the game's name meanwhile (o1e: the name box read empty
        // right after the detach)
        if name != hint && army_row(roster, hint).is_none() && army_row(roster, name).is_some() {
            let rk = key(id, &format!("rename:{hint}"));
            let n = state.done.get(&rk).copied().unwrap_or(0);
            if n < 3 && !state.skipped(now, "oprename", hint) {
                state.done.insert(rk, n + 1);
                state.skip(now, "oprename", hint, DAY);
                let line = order(
                    state,
                    tools,
                    lease,
                    id,
                    &format!("name {name} {hint}"),
                    "hoi4_army_rename",
                    json!({"army": name, "name": hint}),
                )?;
                if state.last_ok != Some(false) {
                    state.layout.insert(hint.clone(), hint.clone());
                }
                return Ok(Some(line));
            }
        }
        let done = key(id, &format!("asm:{name}"));
        if state.done.contains_key(&done) || state.skipped(now, "opasm", name) {
            continue;
        }
        let tries = key(id, &format!("asmtry:{name}"));
        let n = state.done.get(&tries).copied().unwrap_or(0);
        if n >= 12 {
            state.op_notes.insert(
                format!("{id}:{name}"),
                format!("{name}: assembly stopped after {n} moves"),
            );
            continue;
        }
        let Ok(read) = tools.call("hoi4_supply", &json!({"summary": false})) else {
            continue;
        };
        let divisions = list(&read, "divisions");
        let size = |army: &str| {
            divisions
                .iter()
                .filter(|d| text(&d["army"]) == army)
                .count()
        };
        // per template: what the army still lacks, and where it stands elsewhere
        let mut moves: Vec<(String, Vec<String>)> = Vec::new();
        // spare per (army, template) of a line formation that names its own divisions
        let mut spare: HashMap<(String, String), i64> = HashMap::new();
        for ((army, t), keep) in &kept {
            let n = divisions
                .iter()
                .filter(|d| text(&d["army"]) == army && text(&d["template"]) == t)
                .count() as i64;
            spare.insert((army.clone(), t.clone()), n - keep);
        }
        for (template, count) in want {
            let have = divisions
                .iter()
                .filter(|d| text(&d["army"]) == name && text(&d["template"]) == template)
                .count() as i64;
            let mut lack = count - have;
            let mut offered: Vec<&Value> = divisions
                .iter()
                .filter(|d| {
                    let army = text(&d["army"]);
                    text(&d["template"]) == template
                        && !army.is_empty()
                        && army != name
                        && match from {
                            Some(src) => army == src,
                            None => !wings.contains(army) && !mobile_names.contains(army),
                        }
                })
                .collect();
            // a hold's own states first (a division across the enemy cannot reach it)
            offered.sort_by_key(|d| !d["state_id"].as_i64().is_some_and(|s| near.contains(&s)));
            for d in offered {
                if lack <= 0 {
                    break;
                }
                let army = text(&d["army"]).to_string();
                // never what a line formation keeps of its own `divisions` (a mobile neither:
                // Italy's North Strike would empty the Gondar Group's Fanteria)
                if from.is_none() {
                    if let Some(left) = spare.get_mut(&(army.clone(), template.clone())) {
                        if *left <= 0 {
                            continue;
                        }
                        *left -= 1;
                    }
                }
                let Some(div) = d["id"].as_str() else {
                    continue;
                };
                match moves.iter_mut().find(|m| m.0 == army) {
                    Some(m) => m.1.push(div.to_string()),
                    None => moves.push((army, vec![div.to_string()])),
                }
                lack -= 1;
            }
        }
        // a source army keeps one division; one detach moves at most 18
        for m in moves.iter_mut() {
            let keep = size(&m.0).saturating_sub(1);
            m.1.truncate(keep.min(18));
        }
        moves.retain(|m| !m.1.is_empty());
        moves.sort_by_key(|m| std::cmp::Reverse(m.1.len()));
        let Some((source, ids)) = moves.into_iter().next() else {
            state.done.insert(done, now);
            continue;
        };
        state.done.insert(tries, n + 1);
        let exists = army_row(roster, name).is_some();
        let before: HashSet<String> = list(roster, "armies")
            .iter()
            .map(|a| text(&a["army"]).to_string())
            .collect();
        let mut args = json!({"army": source, "divisions": ids});
        if exists {
            args["into"] = json!(name);
        }
        let mut line = order(
            state,
            tools,
            lease,
            id,
            &format!("assemble {name}: {} division(s) out of {source}", ids.len()),
            "hoi4_army_detach",
            args,
        )?;
        if state.last_ok == Some(false) {
            state.skip(now, "opasm", name, DAY / 4);
            return Ok(Some(line));
        }
        if !exists {
            // the new army: the one name the roster did not have; renamed to the formation's
            let after = tools.call("hoi4_armies", &json!({"threat": false}))?;
            let new = list(&after, "armies")
                .iter()
                .map(|a| text(&a["army"]).to_string())
                .find(|a| !before.contains(a));
            if let Some(new) = new {
                // the formation is this army from now on, whatever the rename does
                state.layout.insert(hint.clone(), new.clone());
                state.skip(now, "oprename", hint, DAY / 4);
                line = format!("{line}; the formation {hint} is {new}");
            } else {
                state.skip(now, "opasm", name, DAY / 4);
            }
        }
        return Ok(Some(line));
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
        .filter(|u| !matches!(u.role, Role::Group | Role::Hold))
        .map(|u| u.name.clone())
        .chain(units.iter().flat_map(|u| u.members.iter().cloned()))
        .filter(|a| army_row(roster, a).is_some_and(|r| r["exercising"] == json!(true)))
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
    // ASSEMBLE: a mobile formation gathers its templates' divisions out of the other armies
    if let Some(line) = assemble(state, tools, id, op, units, roster, lease, now)? {
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
    // 1. a line army, and an army of a hold group, carries no orders of its own (it would
    //    ignore its group's front): cleared once; a group front drawn while one still had its
    //    own orders is cleared too and drawn again with every army on it
    let mut sets: Vec<(String, Vec<String>)> = Vec::new();
    if let Some(g) = group {
        sets.push((
            g.to_string(),
            units
                .iter()
                .filter(|u| u.role == Role::Line)
                .map(|u| u.name.clone())
                .collect(),
        ));
    }
    for h in units
        .iter()
        .filter(|u| u.role == Role::Hold && !u.members.is_empty())
    {
        sets.push((h.name.clone(), h.members.clone()));
    }
    for (g, members) in sets {
        for m in members {
            let k = key(id, &format!("clr:{m}"));
            if orders(roster, &m).is_empty() || state.done.contains_key(&k) {
                continue;
            }
            state.done.insert(k, now);
            if has_front(roster, &g) {
                state.done.insert(key(id, &format!("regroup:{g}")), 0);
            }
            let line = order(
                state,
                tools,
                lease,
                id,
                &format!("clear the own orders of {m} (it fights on the front of {g})"),
                "hoi4_army_plan",
                json!({"army": m, "clear_orders": true}),
            )?;
            return Ok(Some(line));
        }
        let rk = key(id, &format!("regroup:{g}"));
        if state.done.get(&rk) == Some(&0) {
            state.done.insert(rk, 1);
            let line = order(
                state,
                tools,
                lease,
                id,
                &format!("clear {g} (drawn while an army had own orders): drawn again"),
                "hoi4_army_plan",
                json!({"army": g, "clear_orders": true}),
            )?;
            return Ok(Some(line));
        }
    }
    // 2-4. mobile and wing armies, then the group, then the holds
    let mut ordered: Vec<&Unit> = units
        .iter()
        .filter(|u| matches!(u.role, Role::Mobile | Role::Wing))
        .collect();
    ordered.extend(units.iter().filter(|u| u.role == Role::Group));
    ordered.extend(units.iter().filter(|u| u.role == Role::Hold));
    // a unit already at war with its enemy prepares by the war's rules (it15, 18 Nov 2026:
    // Italy starts at war with Ethiopia; the peace rules cleared every offensive that read back
    // over two of the line's three states until the draws ran out, and nothing ever executed)
    let fighting = war_tags(wars);
    for u in ordered {
        // a line that would not draw is drawn one state at a time, its first state first
        let one = key(id, &format!("one:{}:{}", u.name, line_tag(&u.line)));
        let line = match state.done.get(&one) {
            Some(first) => vec![*first],
            None => u.line.clone(),
        };
        let peace = !fighting.contains(&u.enemy);
        if let Some(line) = reconcile(state, tools, id, u, &line, roster, wars, lease, now, peace)?
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
        let at_line = if peace {
            None
        } else {
            contact_state(state, wars, &u.enemy, line).or_else(|| line.first().copied())
        };
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
            front_args(u, click, peace),
        )?;
        if state.last_ok == Some(false) {
            state.skip(now, "opdraw", &u.name, 2);
        }
        return Ok(Some(line));
    }
    if u.role == Role::Hold || line.is_empty() {
        return Ok(None);
    }
    if covers(roster, u, line) {
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
        if redraws >= REDRAWS && line.len() >= 2 {
            // o1f: the stroke Lodz-Warsaw read back over Gdynia twice; one state at a time
            state
                .done
                .insert(key(id, &format!("one:{}:{tag}", u.name)), line[0]);
            let line = format!(
                "operation {id}: the line {tag} of {} misses after {REDRAWS} redraws: one state at a time, {} first",
                u.name, line[0]
            );
            state.op_notes.insert(id.to_string(), line.clone());
            return Ok(Some(line));
        }
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
    if !peace && line.len() >= 2 && n >= 1 {
        // at war a stroke across several states does not draw on a group front the game has
        // split (e1g1, o1b): the line is taken one state at a time, its first state first. The
        // standing orders are never cleared at war (o1b: a cleared group found no border to
        // click and stood without orders).
        state
            .done
            .insert(key(id, &format!("one:{}:{tag}", u.name)), line[0]);
        let line = format!(
            "operation {id}: the line {tag} does not draw for {}: one state at a time, {} first",
            u.name, line[0]
        );
        state.op_notes.insert(id.to_string(), line.clone());
        return Ok(Some(line));
    }
    if n >= 2 * REDRAWS {
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
    control(tools, me, ids).0
}

/// The states of `ids` the player controls (the state's controller, or every listed province)
/// and the provinces of `ids` it controls: the line's progress is counted in provinces (o1b: no
/// state of Poland's first line was whole in the first two weeks while the front moved).
fn control(tools: &dyn Tools, me: &str, ids: &[i64]) -> (Vec<i64>, usize) {
    let mut states = Vec::new();
    let mut provinces = 0;
    if me.is_empty() {
        return (states, provinces);
    }
    for id in ids {
        let Ok(read) = tools.call("hoi4_states", &json!({"query": id.to_string()})) else {
            continue;
        };
        let Some(row) = list(&read, "states")
            .iter()
            .find(|r| r["state_id"].as_i64() == Some(*id))
        else {
            continue;
        };
        let own: Vec<bool> = list(row, "provinces")
            .iter()
            .map(|p| text(&p["controller"]) == me)
            .collect();
        provinces += own.iter().filter(|o| **o).count();
        if text(&row["controller"]) == me || (!own.is_empty() && own.iter().all(|o| *o)) {
            states.push(*id);
        }
    }
    (states, provinces)
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
    wars: &Value,
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
        let (got, provinces) = control(tools, &me, &line);
        let before = state.done.get(&key(id, "held")).copied().unwrap_or(0);
        if provinces as i64 > before {
            state.done.insert(key(id, "held"), provinces as i64);
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
        let mut wanted: Vec<i64> = if u.role == Role::Group || own_line_held {
            op_line.clone()
        } else {
            still(&u.line)
        };
        // a line that would not draw at war is taken one state at a time
        let one = key(id, &format!("one:{}:{}", u.name, line_tag(&wanted)));
        if let Some(first) = state.done.get(&one).copied() {
            wanted = vec![first];
        }
        if let Some(line) = reconcile(
            state, tools, id, u, &wanted, roster, wars, lease, now, false,
        )? {
            return Ok(Some(line));
        }
        // REPAIR: the offensive on the wanted line executes (a new arrow on an executing front
        // starts unexecuted: the plan is stopped and executed again)
        let arrow = arrow_executing(roster, u, &wanted);
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
        // a stopped attacker with no arrow on its line (the draws gave up or rest) executes
        // its front rather than stand: it26b (18 Nov 2026) stopped the group to execute a new
        // arrow that never read back, and the group stood six weeks
        if arrow.is_none()
            && at_war.contains(&u.enemy)
            && has_front(roster, &u.name)
            && !executing(roster, &u.name)
            && !state.skipped(now, "opexec", &u.name)
        {
            state.skip(now, "opexec", &u.name, DAY / 2);
            let line = order(
                state,
                tools,
                lease,
                id,
                &format!(
                    "{} stands with no arrow on its line: execute its front",
                    u.name
                ),
                "hoi4_army_plan",
                json!({"army": u.name, "execute": true}),
            )?;
            return Ok(Some(line));
        }
    }
    // holds keep their fronts (never executed)
    for u in units.iter().filter(|u| u.role == Role::Hold) {
        if let Some(line) = reconcile(state, tools, id, u, &[], roster, wars, lease, now, false)? {
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
