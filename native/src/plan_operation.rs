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
//! - FAR LINE (`offensive.deep: true`): the group's one offensive is drawn once, in peace, at the
//!   far side of what must fall (see `far_line`: behind the states that hold DEEP_SHARE of the
//!   enemies' victory points), so no second arrow is ever needed; the plan's phase lines are not
//!   used, the war's progress is every enemy province taken, and a war that took none for
//!   `stall_days` turns the group's arrow on the touching states with the most points in one place.
//! - REPLACE, NEVER ADD (`replace_arrows: true`): an arrow that misses its unit's line is deleted
//!   alone (hoi4_army_plan delete_order: the front and its planning bonus stay) before the new
//!   one is drawn; at war, an arrow that will not delete alone clears the unit's orders once and
//!   the front and the arrow are drawn again. Without it a changed line adds an arrow, and the
//!   game gives a second arrow on one front no divisions.
//! - REGROUP AT WAR (`regroup_at_war: true`): the group is formed again with the line armies
//!   outside it while the country fights someone else, as long as the operation's own enemies are
//!   not at war with us; never with an army another running operation owns or one standing in
//!   contact with an enemy (see `in_contact`). Without it the regroup runs only at full peace.
//! - TWO GROUPS (`group_cap`): more line armies than one field marshal commands fight in two
//!   groups split by geography, each with ONE front on its own piece of the border and one
//!   offensive through its side of every line (see `Split`).
//! - RESERVES (`reserve_orders: true`): each `reserve` row's army is owned and gets a standing
//!   order (a march to its station, then a fallback line or an area defence there) until its
//!   release (see `reserves`).
//! - FINISH: every enemy out of the war: the attack orders are cleared, the `redeploy` rows move
//!   their armies as whole armies to the next theatre (see `redeploy`), and
//!   `on_capitulation.next` starts its PREPARE at once.
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
/// Game hours a line army may stand at war without an executing order of its own before it
/// gets its own front and offensive.
const SOLO_AFTER: GameHour = 2 * DAY;
/// States in a solo line army's objective line.
const SOLO_STATES: usize = 2;
/// Organisation share under which a strike formation opens no new strike (qc4: strikes at
/// 17-31% organisation took nothing and cost 1:1), and the days it waits for it at most.
const STRIKE_WORN: f64 = 0.4;
const STRIKE_REST_DAYS: f64 = 3.0;
/// Divisions of a driven infantry army that strike from one province.
const STACK_INFANTRY: i64 = 4;
/// Tries to rename a formed army to its plan name.
/// Renames an operation's prepare sends: none. A name is cosmetic: the operation orders a formed
/// army under the game's name (bound by identity in `state.layout`) and the layout's own name step
/// renames it when the executor has nothing else to do. Every rename in the prepare cost ~5 s, a
/// game day before the war at speed 3, and half of them failed (qc5: five renames, all failed).
const RENAME_TRIES: i64 = 0;
/// Supply percent under which a strike formation opens no new strike.
const STRIKE_SUPPLY: f64 = 20.0;
/// War days a line army may stand in one state before it is driven by objectives.
const STUCK_DAYS: i64 = 7;
/// States in a rolling contact line of the operation's group.
const ROLL_STATES: usize = 3;
/// Game days without a gain before a rolling contact line rolls on.
const ROLL_STALL_DAYS: i64 = 4;
/// Game days a wing stands at war with no state of its line taken and no division across the
/// border before it gives the first state of its line up to the group's plan and attacks the next
/// one (qv1, rs5, qv2, 19 Nov 2026: Armee Nord's 17 divisions stood 14-21 days in East Prussia on
/// the save's arrow at Danzig and Gdynia, none of them in Poland, while the group fought alone).
const WING_STALL_DAYS: i64 = 5;
/// The far line (`offensive.deep`): the share of the enemies' victory points the line must lie
/// behind (the surrender limit, 0.8 at the base, with margin: Poland stood at 0.81 for weeks
/// without capitulating on the bed, b38f/b38g), the depth band of its states (a share of the
/// enemy's depth) and the sideways gap under which the nearer of two states gives way.
const DEEP_SHARE: f64 = 0.95;
const DEEP_BAND: f64 = 0.3;
const DEEP_GAP: f64 = 1.0 / 6.0;
/// Map distance within which the states of a stalled far line's next target lie together.
const DEEP_CLUSTER: f64 = 45.0;
/// Game days a strike formation keeps one objective before it turns to the next (qc1: at 4 the
/// column flipped between Warsaw and Krakow every four days and took neither).
const STRIKE_MAX_DAYS: f64 = 10.0;

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
    /// an armour formation on its own front whose attack is the game's spearhead order painted
    /// through `line` (the owner's war model, 19 Nov 2026), not the strike engine's direct moves
    spear: bool,
    /// one of two army groups of an operation with `group_cap` (true: the low side of the split
    /// axis): its own piece of the front's states and of every line
    side: Option<bool>,
}

fn key(id: &str, what: &str) -> String {
    format!("op:{id}:{what}")
}

/// The declaration of the operation's war is due and waits for its prepare (set by the
/// diplomacy step). From then on a secondary step that has had its try (a mobile formation's
/// assembly, a march to its start state) no longer holds the war: the formation fights with what
/// it has and the rest inside its own army (qs1, qu2, qv2, 19 Nov 2026: one failing detach of
/// Panzergruppe Nord held the declaration to 18-21 September, qv1 declared on the 2nd).
fn due(state: &State, id: &str) -> bool {
    state.done.contains_key(&key(id, "due"))
}

/// Marks the declaration against the operation `id` as due (see `due`).
pub(super) fn mark_due(state: &mut State, id: &str, now: GameHour) {
    state.done.entry(key(id, "due")).or_insert(now);
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
    if ops.is_empty() || layout_pending(state, plan, now).is_some() {
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
        if !(from || on) {
            continue;
        }
        // OPERATIONS NEVER STEAL: an operation whose armies a running operation holds waits
        // until that one is done (g17k, 19 Nov 2026: op_sov fired on the Soviet declaration in
        // the middle of the Polish war, took the Polish armies east, and Poland walked into
        // Berlin). The new enemy meets the armies no operation owns (the war step's fronts).
        let mine = army_hints(op);
        let busy = ops.iter().enumerate().find(|(j, other)| {
            let oid = op_id(other, *j);
            oid != id && started(state, &oid) && army_hints(other).iter().any(|a| mine.contains(a))
        });
        if let Some((j, other)) = busy {
            let oid = op_id(other, j);
            state.op_notes.insert(
                id.clone(),
                format!("operation {id}: waits for operation {oid} (it holds the same armies) to finish"),
            );
            continue;
        }
        state.done.insert(key(&id, "prep"), now);
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

/// The army names an operation's formations and holds use (as the plan writes them).
fn army_hints(op: &Value) -> Vec<String> {
    list(op, "formations")
        .iter()
        .map(|f| text(&f["army"]).to_string())
        .chain(
            list(op, "holds")
                .iter()
                .flat_map(|h| list(h, "armies").iter())
                .map(|a| text(&a["army"]).to_string()),
        )
        .filter(|a| !a.is_empty())
        .collect()
}

/// The game names an operation owns: its formation and hold armies and their army groups.
fn owned_names(state: &State, plan: &Value, op: &Value, roster: &Value) -> Vec<String> {
    let mut armies: Vec<String> = list(op, "formations")
        .iter()
        .map(|f| name_of(state, roster, text(&f["army"])))
        .collect();
    for hold in list(op, "holds") {
        armies.extend(
            list(hold, "armies")
                .iter()
                .map(|a| name_of(state, roster, text(&a["army"]))),
        );
    }
    armies.extend(reserve_names(state, op, roster));
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

/// Game days the operations wait at most for the army layout to form.
const LAYOUT_WAIT_DAYS: i64 = 14;

/// The army_layout roles not formed yet (no army bound, not given up), while the operations wait
/// for them: ownership and the prepare start only once the layout has formed, so the layout and
/// the operation's assembly never race over the same armies (it27b, 18 Nov 2026: the operation
/// named and owned Army 2 as North Strike before the layout split it). Dated roles (`when`) and
/// a plan without land orders are not waited for; after LAYOUT_WAIT_DAYS the operations go on.
pub(super) fn layout_pending(
    state: &mut State,
    plan: &Value,
    now: GameHour,
) -> Option<Vec<String>> {
    if !super::land_orders(plan) {
        return None;
    }
    let pending: Vec<String> = list(plan, "army_layout")
        .iter()
        .filter(|e| e["when"].is_null())
        .map(|e| text(&e["name_hint"]).to_string())
        .filter(|h| {
            !h.is_empty()
                && !state.layout.contains_key(h)
                && !state.done.contains_key(&format!("layout:impossible:{h}"))
        })
        .collect();
    if pending.is_empty() {
        return None;
    }
    let since = *state.done.entry("op:layoutwait".to_string()).or_insert(now);
    (now - since < LAYOUT_WAIT_DAYS * DAY).then_some(pending)
}

/// Whether an operation of the plan names `hint` as one of its formation or hold armies.
pub(super) fn names_army(plan: &Value, hint: &str) -> bool {
    !hint.is_empty()
        && list(plan, "operations").iter().any(|op| {
            list(op, "formations")
                .iter()
                .any(|f| text(&f["army"]) == hint)
                || list(op, "holds")
                    .iter()
                    .flat_map(|h| list(h, "armies").iter())
                    .any(|a| text(&a["army"]) == hint)
        })
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

/// The game name of `name` on the map: the game cuts long army names (g17k, pb1 19 Nov 2026:
/// "Grenzschutz Ostpreussen" read back as "Grenzschutz Ostpreus"); a hold the operation did not
/// recognise under its cut name was not owned, and the command step merged it into its wing.
fn on_map(roster: &Value, name: &str) -> String {
    if name.is_empty() || army_row(roster, name).is_some() || group_row(roster, name).is_some() {
        return name.to_string();
    }
    list(roster, "armies")
        .iter()
        .map(|a| text(&a["army"]))
        .filter(|a| a.chars().count() >= 10 && name.starts_with(a))
        .max_by_key(|a| a.len())
        .map_or_else(|| name.to_string(), str::to_string)
}

/// The game name of a plan army (layout role or name) on the map: the bound army, else the
/// plan's own name, each also under the game's cut form. A formation formed as "Army 3" and
/// renamed to a long name is on the map under the cut long name, not under "Army 3".
fn name_of(state: &State, roster: &Value, hint: &str) -> String {
    let bound = resolve(state, hint);
    let found = |n: &str| army_row(roster, n).is_some() || group_row(roster, n).is_some();
    if found(&bound) {
        return bound;
    }
    for n in [on_map(roster, &bound), on_map(roster, hint)] {
        if found(&n) {
            return n;
        }
    }
    bound
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
    if cfg!(test) && !owned.contains_key("__test_publish__") {
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
fn units(state: &State, id: &str, op: &Value, roster: &Value) -> (Vec<Unit>, Option<String>) {
    let mut out = Vec::new();
    let mut group: Option<String> = None;
    let mut lines_in_group = 0;
    let mut line_names: Vec<String> = Vec::new();
    let (_, op_line) = current_line(state, id, op);
    for f in list(op, "formations") {
        let name = name_of(state, roster, text(&f["army"]));
        if name.is_empty() {
            continue;
        }
        let role = match text(&f["role"]) {
            "mobile" if text(&f["drive"]) == "strike" => Role::Strike,
            "mobile" => Role::Mobile,
            "wing" => Role::Wing,
            _ => Role::Line,
        };
        // a line army that stood still behind an executing front is driven by objectives as
        // one stack (see `solo_watch`)
        if matches!(role, Role::Line | Role::Wing)
            && state.done.contains_key(&key(id, &format!("drive:{name}")))
        {
            // its own objectives when the plan gives them (a wing: Torun, Plock, then Warsaw
            // from the north), else the operation's line states nearest it
            let own: Vec<i64> = ints(&f["line"]);
            let line = if own.is_empty() {
                solo_line(state, roster, &name, &op_line)
            } else {
                own
            };
            out.push(Unit {
                name,
                role: Role::Strike,
                members: Vec::new(),
                enemy: op["front"]["enemy"].as_str().unwrap_or("").to_string(),
                own_states: Vec::new(),
                click: None,
                anchor: None,
                line,
                spear: false,
                side: None,
            });
            continue;
        }
        // a line army the group's front left without an executing order fights on its own
        // front (see `solo_line`)
        if role == Role::Line && state.done.contains_key(&key(id, &format!("solo:{name}"))) {
            let enemy = op["front"]["enemy"]
                .as_str()
                .map(str::to_string)
                .or_else(|| strings(op, "enemies").first().cloned())
                .unwrap_or_default();
            let line = solo_line(state, roster, &name, &op_line);
            out.push(Unit {
                own_states: army_row(roster, &name)
                    .and_then(|r| r["main_state"].as_i64())
                    .into_iter()
                    .collect(),
                name,
                role: Role::Wing,
                members: Vec::new(),
                enemy,
                click: None,
                anchor: line.first().copied(),
                line,
                spear: false,
                side: None,
            });
            continue;
        }
        if role == Role::Line {
            lines_in_group += 1;
            line_names.push(name.clone());
            if group.is_none() {
                group = army_row(roster, &name)
                    .and_then(|r| r["army_group"].as_str())
                    .map(str::to_string);
            }
        }
        if role == Role::Strike && spearheads(op) && f["front"].is_object() {
            let mut u = unit(name, Role::Mobile, &f["front"], &f["offensive"]);
            u.line = spear_path(f);
            u.anchor = u.line.last().copied();
            // a spearhead that did not read back where it was painted gives way to the armour's
            // own offensive line (the owner's other choice: "its own narrow front and offensive")
            u.spear = !state
                .done
                .contains_key(&key(id, &format!("nospear:{}", u.name)));
            out.push(u);
            continue;
        }
        let (front, offensive) = if role == Role::Line {
            (&Value::Null, &Value::Null)
        } else {
            (&f["front"], &f["offensive"])
        };
        out.push(unit(name, role, front, offensive));
    }
    if lines_in_group == 0 {
        // every line army fights on its own front: the group has no divisions left to order
        group = None;
    }
    let two = if lines_in_group == 0 {
        Vec::new()
    } else {
        split_groups(state, id, op, roster, &line_names)
    };
    if !two.is_empty() {
        group = two.first().map(|u| u.name.clone());
        out.extend(two);
    } else if let Some(g) = &group {
        let mut u = unit(g.clone(), Role::Group, &op["front"], &op["offensive"]);
        let far = deep(state, id);
        if !far.is_empty() {
            u.anchor = None;
            u.line = far;
        }
        out.push(u);
    }
    for hold in list(op, "holds") {
        let armies: Vec<String> = list(hold, "armies")
            .iter()
            .map(|a| name_of(state, roster, text(&a["army"])))
            .filter(|a| !a.is_empty())
            .collect();
        // an army of the hold that is not on the map leaves the hold to the others (g17m, 19 Nov
        // 2026: Armee West Süd was never formed, the hold waited for a group of two and the
        // West stood without a front while France took nineteen states)
        let present: Vec<String> = armies
            .iter()
            .filter(|a| army_row(roster, a).is_some())
            .cloned()
            .collect();
        let armies = if present.is_empty() { armies } else { present };
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
        // several armies in no group yet: the largest fronts for the hold until they are grouped
        // (g17m: West Süd was bound to a one-division army and the hold waited for a group that
        // nothing formed, while France took the Rhineland)
        let name = name.or_else(|| {
            armies
                .iter()
                .filter_map(|a| Some((a, army_row(roster, a)?["division_count"].as_i64()?)))
                .max_by_key(|(_, n)| *n)
                .map(|(a, _)| a.clone())
        });
        let grouped = name
            .as_deref()
            .is_some_and(|n| group_row(roster, n).is_some());
        let Some(name) = name else { continue };
        let facing = strings(hold, "facing");
        let front = json!({"enemy": facing.first().cloned().unwrap_or_default(),
                           "own_states": hold["states"], "click_state": hold["click_state"]});
        let mut hold = unit(name, Role::Hold, &front, &Value::Null);
        if armies.len() > 1 && grouped {
            hold.members = armies;
        }
        out.push(hold);
    }
    (out, group)
}

/// TWO ARMY GROUPS (`group_cap`, the armies one field marshal commands: 5 in the game's defines):
/// an operation with more line armies than that splits them in two groups by geography. The
/// split axis runs between the two front states (`front.own_states`) farthest apart; the cut lies
/// between the middle two of them, so each group has half of the front's states. Each group draws
/// ONE front on its own piece of the border (a drag along its own states) and carries its one
/// offensive through its side of the line (and of the far line, and of every later line).
#[derive(Clone, Copy, Debug)]
struct Split {
    a: (f64, f64),
    dir: (f64, f64),
    cut: f64,
}

impl Split {
    fn at(&self, p: (f64, f64)) -> f64 {
        (p.0 - self.a.0) * self.dir.0 + (p.1 - self.a.1) * self.dir.1
    }
    fn low(&self, p: (f64, f64)) -> bool {
        self.at(p) < self.cut
    }
}

/// The split axis of an operation with `group_cap` (None without the key or without two front
/// states whose map centres are known).
fn split_axis(state: &State, op: &Value) -> Option<Split> {
    op["group_cap"].as_i64()?;
    let pts: Vec<(f64, f64)> = ints(&op["front"]["own_states"])
        .iter()
        .filter_map(|s| state.centers.get(&(*s as i32)).copied())
        .collect();
    type Pair = (f64, (f64, f64), (f64, f64));
    let mut best: Option<Pair> = None;
    for (i, p) in pts.iter().enumerate() {
        for q in &pts[i + 1..] {
            let d = (q.0 - p.0).hypot(q.1 - p.1);
            if best.is_none_or(|b| d > b.0) {
                best = Some((d, *p, *q));
            }
        }
    }
    let (len, a, b) = best.filter(|b| b.0 > 1e-6)?;
    let dir = ((b.0 - a.0) / len, (b.1 - a.1) / len);
    let mut split = Split { a, dir, cut: 0.0 };
    let mut proj: Vec<f64> = pts.iter().map(|p| split.at(*p)).collect();
    proj.sort_by(f64::total_cmp);
    let m = proj.len() / 2;
    split.cut = (proj[m - 1] + proj[m]) / 2.0;
    Some(split)
}

/// The states of `line` on one side of the split (the whole line when none lies there or a
/// centre is unknown for all).
fn side_line(state: &State, split: &Split, low: bool, line: &[i64]) -> Vec<i64> {
    let out: Vec<i64> = line
        .iter()
        .copied()
        .filter(|s| {
            state
                .centers
                .get(&(*s as i32))
                .is_some_and(|p| split.low(*p) == low)
        })
        .collect();
    if out.is_empty() {
        line.to_vec()
    } else {
        out
    }
}

/// Where an army stands on the map: the centre of its states weighted by its divisions there.
fn army_point(state: &State, roster: &Value, name: &str) -> Option<(f64, f64)> {
    let at = army_row(roster, name)?["states"].as_object()?;
    let (mut x, mut z, mut n) = (0.0, 0.0, 0.0);
    for (sid, count) in at {
        let Some(p) = sid
            .parse::<i32>()
            .ok()
            .and_then(|s| state.centers.get(&s).copied())
        else {
            continue;
        };
        let c = count.as_f64().unwrap_or(0.0);
        x += p.0 * c;
        z += p.1 * c;
        n += c;
    }
    (n > 0.0).then(|| (x / n, z / n))
}

/// The line armies in two parts (the low side's first, each sorted along the axis) when there
/// are more than `group_cap` of them: the half nearer the axis's start to the low side.
fn split_parts(
    state: &State,
    op: &Value,
    roster: &Value,
    lines: &[String],
) -> Option<(Split, Vec<String>, Vec<String>)> {
    let cap = op["group_cap"].as_i64()?.max(1);
    if lines.len() as i64 <= cap {
        return None;
    }
    let split = split_axis(state, op)?;
    let mut by: Vec<(f64, String)> = lines
        .iter()
        .map(|n| {
            let at = army_point(state, roster, n).map_or(split.cut, |p| split.at(p));
            (at, n.clone())
        })
        .collect();
    by.sort_by(|a, b| a.0.total_cmp(&b.0));
    let half = by.len().div_ceil(2);
    let high: Vec<String> = by.split_off(half).into_iter().map(|p| p.1).collect();
    let low: Vec<String> = by.into_iter().map(|p| p.1).collect();
    Some((split, low, high))
}

fn group_of(roster: &Value, name: &str) -> Option<String> {
    army_row(roster, name)?["army_group"]
        .as_str()
        .map(str::to_string)
}

/// Whether `part` stands in one game group that holds no other line army (its group is formed).
fn part_formed(roster: &Value, part: &[String], lines: &[String]) -> Option<String> {
    let g = group_of(roster, part.first()?)?;
    let all = part
        .iter()
        .all(|n| group_of(roster, n).as_deref() == Some(&g));
    let alone = lines
        .iter()
        .filter(|n| !part.contains(n))
        .all(|n| group_of(roster, n).as_deref() != Some(&g));
    (all && alone).then_some(g)
}

/// The two group units of a split operation once both groups are formed (else none: the
/// operation runs as one group until they are).
fn split_groups(
    state: &State,
    id: &str,
    op: &Value,
    roster: &Value,
    lines: &[String],
) -> Vec<Unit> {
    let Some((split, low, high)) = split_parts(state, op, roster, lines) else {
        return Vec::new();
    };
    let (Some(gl), Some(gh)) = (
        part_formed(roster, &low, lines),
        part_formed(roster, &high, lines),
    ) else {
        return Vec::new();
    };
    let far = deep(state, id);
    [(gl, low, true), (gh, high, false)]
        .into_iter()
        .map(|(g, part, is_low)| {
            let mut u = unit(g, Role::Group, &op["front"], &op["offensive"]);
            u.click = None;
            u.own_states = side_line(state, &split, is_low, &u.own_states);
            let whole = if far.is_empty() {
                u.line.clone()
            } else {
                far.clone()
            };
            u.line = side_line(state, &split, is_low, &whole);
            u.anchor = u
                .anchor
                .filter(|a| u.line.contains(a) && far.is_empty())
                .or_else(|| {
                    u.line
                        .get(u.line.len() / 2)
                        .copied()
                        .filter(|_| far.is_empty())
                });
            u.members = part;
            u.side = Some(is_low);
            u
        })
        .collect()
}

/// Objectives of a line army on its own front: the SOLO_STATES states of the operation's line
/// nearest the army (map centres of its states, weighted by its divisions there); the line's
/// first states when no centre is known. Computed afresh each call: a state taken drops out and
/// the next nearest comes in, so the army always has a next objective.
fn solo_line(state: &State, roster: &Value, name: &str, op_line: &[i64]) -> Vec<i64> {
    let Some(row) = army_row(roster, name) else {
        return op_line.iter().take(SOLO_STATES).copied().collect();
    };
    let mut x = 0.0;
    let mut z = 0.0;
    let mut n = 0.0;
    if let Some(at) = row["states"].as_object() {
        for (sid, count) in at {
            let (Ok(sid), Some(count)) = (sid.parse::<i32>(), count.as_f64()) else {
                continue;
            };
            if let Some(p) = state.centers.get(&sid) {
                x += p.0 * count;
                z += p.1 * count;
                n += count;
            }
        }
    }
    if n == 0.0 {
        if let Some(p) = row["main_state"]
            .as_i64()
            .and_then(|s| state.centers.get(&(s as i32)))
        {
            (x, z, n) = (p.0, p.1, 1.0);
        }
    }
    let mut line: Vec<i64> = op_line.to_vec();
    if n > 0.0 {
        let (x, z) = (x / n, z / n);
        let d = |s: &i64| {
            state
                .centers
                .get(&(*s as i32))
                .map_or(f64::MAX, |p| (p.0 - x).powi(2) + (p.1 - z).powi(2))
        };
        line.sort_by(|a, b| d(a).total_cmp(&d(b)));
    }
    line.truncate(SOLO_STATES);
    line
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
        spear: false,
        side: None,
    }
}

/// Whether the operation's armour fights on its own narrow front and the game's spearhead order
/// (`armour: "spearhead"`) or on the strike engine's direct moves (the default). On the Poland bed
/// (19 Nov 2026) the spearheads took Lodz and Krakow by day 14 but then stood or were redrawn, and
/// reached 51-71 victory points on day 49 against 71-112 on the direct moves.
fn spearheads(op: &Value) -> bool {
    op["armour"].as_str() == Some("spearhead")
}

/// The states of a strike formation's thrust, deepest last: its axis without its own start
/// states (Panzergruppe Ost: axis 67-87-10 -> 87, 10), else its target line. The spearhead is
/// painted in one stroke from the front to the deepest of them and takes the states on its way.
fn spear_path(f: &Value) -> Vec<i64> {
    let own: Vec<i64> = ints(&f["front"]["own_states"]);
    let path: Vec<i64> = ints(&f["offensive"]["axis"])
        .into_iter()
        .filter(|s| !own.contains(s))
        .collect();
    if path.is_empty() {
        ints(&f["offensive"]["target_line"])
    } else {
        path
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

/// Whether one offensive `off` of the unit aims at `line` (see `arrow_executing`).
fn aims(u: &Unit, off: &Value, line: &[i64]) -> bool {
    if u.spear {
        // a spearhead reads back as painted: its path and target reach the line's deepest state
        let reach: HashSet<i64> = ints(&off["path_states"])
            .into_iter()
            .chain(ints(&off["target"]["states"]))
            .collect();
        return line.last().is_none_or(|s| reach.contains(s));
    }
    let target: HashSet<i64> = ints(&off["target"]["states"]).into_iter().collect();
    line.is_empty() || line.iter().filter(|s| target.contains(s)).count() >= need(u, line)
}

/// The executing flag of the unit's offensive that covers `line` (None when none covers it).
fn arrow_executing(roster: &Value, u: &Unit, line: &[i64]) -> Option<bool> {
    let mut found = None;
    for off in offensives(roster, &u.name) {
        if aims(u, off, line) {
            if off["executing"] == json!(true) {
                return Some(true);
            }
            found = Some(false);
        }
    }
    found
}

/// The enemies the player is at war with (hoi4_wars `wars.enemies`).
/// The enemies still fighting: hoi4_wars `enemies` without `wars_won` (a country that controls
/// none of its states has capitulated, also when it fights on as a government in exile; the
/// game keeps its war relation for good). g17j3 (19 Nov 2026): Poland in exile kept op_poland
/// and every army for 282 days and held back every row behind its capitulation.
fn war_tags(wars: &Value) -> Vec<String> {
    let won: Vec<&str> = list(&wars["wars"], "wars_won")
        .iter()
        .filter_map(Value::as_str)
        .collect();
    list(&wars["wars"], "enemies")
        .iter()
        .filter_map(|t| t.as_str().or_else(|| t["tag"].as_str()).map(str::to_string))
        .filter(|t| !won.contains(&t.as_str()))
        .collect()
}

/// Whether another running operation owns the army or army group `name`.
fn owned_elsewhere(state: &State, id: &str, name: &str) -> bool {
    state.op_owned.get(name).is_some_and(|op| op != id)
}

/// The states where an own army meets a country at war with us: both sides of every border with
/// such a country (hoi4_wars borders).
fn contact_states(wars: &Value) -> HashSet<i64> {
    let fighting = war_tags(wars);
    list(wars, "borders")
        .iter()
        .filter(|b| fighting.iter().any(|t| t == text(&b["neighbour_tag"])))
        .flat_map(|b| {
            ints(&b["own_border_states"])
                .into_iter()
                .chain(ints(&b["neighbour_states"]))
        })
        .collect()
}

/// Whether an army stands in contact with an enemy (a division in a contact state) or executes a
/// battle plan: such an army is never taken out of its place by a regroup or a redeployment.
fn in_contact(roster: &Value, wars: &Value, name: &str) -> bool {
    let Some(row) = army_row(roster, name) else {
        return false;
    };
    let contact = contact_states(wars);
    let standing = row["states"].as_object().is_some_and(|at| {
        at.iter().any(|(s, n)| {
            n.as_i64().unwrap_or(0) > 0 && s.parse::<i64>().is_ok_and(|s| contact.contains(&s))
        })
    });
    standing || executing(roster, name)
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
        // (the centres live in this server life: after a restart they are read again)
        if tag.is_empty() || (state.done.contains_key(&k) && !state.centers.is_empty()) {
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
        // one of two groups: its own piece of the border (a drag along its own states only)
        (Role::Group, [own]) if peace && u.side.is_some() => {
            json!({"army": u.name, "state_id": click, "from_state": own})
        }
        (Role::Group, owns) if peace && u.side.is_some() && owns.len() >= 2 => {
            json!({"army": u.name, "state_id": click, "from_states": owns})
        }
        _ => json!({"army": u.name, "state_id": click}),
    }
}

/// An offensive line (across the line's states), or with `spear` the game's spearhead order
/// painted from the army through the states in order.
fn offensive_order(name: &str, line: &[i64], anchor: Option<i64>, spear: bool) -> Value {
    let mut args = offensive_line_args(name, line, anchor);
    if spear {
        args["spearhead"] = json!(true);
    }
    args
}

fn offensive_line_args(name: &str, line: &[i64], anchor: Option<i64>) -> Value {
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
    if let Some(pending) = layout_pending(state, plan, now) {
        state.op_notes.insert(
            "layout".to_string(),
            format!(
                "operations wait for the army layout to form: {}",
                pending.join(", ")
            ),
        );
        return Ok(None);
    }
    state.op_notes.remove("layout");
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

/// Whether the war on `target` may start: every running operation against it has prepared
/// (read back), and one runs; true when the plan has no operation against it.
pub(super) fn prepared_against(state: &State, plan: &Value, target: &str) -> bool {
    let ops: Vec<String> = list(plan, "operations")
        .iter()
        .enumerate()
        .filter(|(_, op)| strings(op, "enemies").iter().any(|e| e == target))
        .map(|(i, op)| op_id(op, i))
        .collect();
    ops.is_empty()
        || ops
            .iter()
            .any(|id| started(state, id) && state.done.contains_key(&key(id, "ready")))
            && unready(state, plan, target).is_none()
}

/// An operation executes (its war was declared and it has not finished).
/// When the earliest operation still executing began executing.
pub(super) fn executing_since(state: &State) -> Option<GameHour> {
    state
        .done
        .iter()
        .filter(|(k, _)| {
            k.starts_with("op:")
                && k.ends_with(":exec")
                && !state
                    .done
                    .contains_key(&format!("{}:end", &k[..k.len() - 5]))
        })
        .map(|(_, at)| *at)
        .min()
}

pub(super) fn executing_any(state: &State) -> bool {
    state.done.keys().any(|k| {
        k.starts_with("op:")
            && k.ends_with(":exec")
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

/// An operation is prepared (read back as ready, or primed: its attack reads back and only a
/// march is under way) and waits for its war: not executed, not ended.
pub(super) fn awaiting_war(state: &State) -> bool {
    state.done.keys().any(|k| {
        let base = k
            .strip_suffix(":ready")
            .or_else(|| k.strip_suffix(":primed"));
        k.starts_with("op:")
            && base.is_some_and(|base| {
                !state.done.contains_key(&format!("{base}:exec"))
                    && !state.done.contains_key(&format!("{base}:end"))
            })
    })
}

/// An operation against `target` is prepared and waits for its war.
pub(super) fn awaiting_war_on(state: &State, plan: &Value, target: &str) -> bool {
    list(plan, "operations").iter().enumerate().any(|(i, op)| {
        let id = op_id(op, i);
        strings(op, "enemies").iter().any(|e| e == target)
            && (state.done.contains_key(&key(&id, "ready"))
                || state.done.contains_key(&key(&id, "primed")))
            && !state.done.contains_key(&key(&id, "exec"))
            && !state.done.contains_key(&key(&id, "end"))
    })
}

/// An operation started and not yet read back as ready (its prepare still has orders to give).
pub(super) fn preparing(state: &State) -> bool {
    state.done.keys().any(|k| {
        k.starts_with("op:") && k.ends_with(":prep") && {
            let base = &k[..k.len() - 5];
            !state.done.contains_key(&format!("{base}:ready"))
                && !state.done.contains_key(&format!("{base}:end"))
        }
    })
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
    state.done.insert(
        key(id, "replace_arrows"),
        i64::from(op["replace_arrows"] == json!(true)),
    );
    let executed = state.done.contains_key(&key(id, "exec"));
    let (units, group) = units(state, id, op, roster);
    if executed {
        if state.centers.is_empty() {
            let mut tags = enemies.clone();
            tags.push(player(tools, plan));
            learn_centres(state, tools, &tags);
        }
        solo_watch(state, id, op, roster, &at_war, now);
        // the strike formations first: their orders are quick whole-army moves, and behind the
        // line work they got no turn while a line order kept failing (qb1, 19 Nov 2026: the
        // column stood in Lodz from day 28 to day 56 while the burst ended on resting redraws)
        if let Some(line) = strikes(
            state, tools, plan, op, &units, roster, &enemies, &at_war, lease, now,
        )? {
            return Ok(Some(line));
        }
        if let Some(line) = reserves(state, tools, id, op, roster, wars, lease, now)? {
            return Ok(Some(line));
        }
        return advance(
            state, tools, plan, id, op, &units, roster, wars, &enemies, &at_war, lease, now,
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
    let secondary_free = due(state, id);
    let resting = state.bad.iter().any(|(k, until)| {
        *until > now && (k.starts_with("opdraw:") || (k.starts_with("opasm:") && !secondary_free))
    });
    if !resting {
        state.done.entry(key(id, "ready")).or_insert(now);
    }
    // PRIMED: the attack itself reads back (no front or arrow rests) while only secondary orders
    // (a strike formation's march to its start state) are still under way: the declaration's
    // scheduling (declaration first, air held, the target's diplomacy read ahead) applies from
    // here, not only once the march is done (b38g, 19 Nov 2026: Panzergruppe Nord's march was
    // never finished, the operation never read as ready before the declaration's day, the air
    // batches took the ticks and the war came on 6 September instead of the 1st)
    let drawing = state
        .bad
        .iter()
        .any(|(k, until)| *until > now && k.starts_with("opdraw:"));
    if !drawing {
        state.done.entry(key(id, "primed")).or_insert(now);
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

/// A line army at war whose own read-back shows no executing order for SOLO_AFTER game hours
/// gets its own front and an offensive on its nearest objectives (it becomes a wing of the
/// operation for good). pb1 and g17k (19 Nov 2026): Armee Mitte and Armee Sued stood behind the
/// group's whole-border front with no order of their own for the whole war (24 divisions idle
/// in Silesia), and the group's one country-wide arrow took three states in 56 days.
/// Whether another line army of `row`'s army group is already driven by objectives: one at a
/// time leaves the group's front (qc3, 19 Nov 2026: Mitte and Süd both left it, Poland walked
/// into Silesia behind them and retook 30 of the 50 points it had lost).
fn group_driven(state: &State, id: &str, op: &Value, roster: &Value, row: Option<&Value>) -> bool {
    let Some(group) = row.and_then(|r| r["army_group"].as_str()) else {
        return false;
    };
    list(op, "formations").iter().any(|f| {
        let other = name_of(state, roster, text(&f["army"]));
        state.done.contains_key(&key(id, &format!("drive:{other}")))
            && army_row(roster, &other).and_then(|r| r["army_group"].as_str()) == Some(group)
    })
}

fn solo_watch(
    state: &mut State,
    id: &str,
    op: &Value,
    roster: &Value,
    at_war: &[String],
    now: GameHour,
) {
    // The war runs on few executed battle plans (the owner's rule, 19 Nov 2026: "Do not create
    // multiple front lines ... Keep front lines to a minimum"): an army that idles inside its
    // group's plan is not given a front of its own nor driven as a stack by direct moves any
    // more; the operation's `solo_armies: true` restores the old behaviour for a test.
    if op["solo_armies"] != json!(true) {
        return;
    }
    if !strings(op, "enemies").iter().any(|e| at_war.contains(e)) {
        return;
    }
    for f in list(op, "formations") {
        // a wing is driven when it stands (qc4: Armee Nord stood in East Prussia 56 war days on
        // an executing offensive that never drew); only a line army fights on a front of its own
        let wing = text(&f["role"]) == "wing";
        if !matches!(text(&f["role"]), "" | "line" | "wing") {
            continue;
        }
        let name = name_of(state, roster, text(&f["army"]));
        let solo = key(id, &format!("solo:{name}"));
        let idle = key(id, &format!("idle:{name}"));
        if name.is_empty()
            || state.done.contains_key(&solo)
            || state.done.contains_key(&key(id, &format!("drive:{name}")))
            || army_row(roster, &name).is_none()
        {
            continue;
        }
        // the army's own orders, or its group's executing front (it carries the army: qb1, the
        // group's whole-border front left no border for an army front of its own)
        let row = army_row(roster, &name);
        let own_executing = row.is_some_and(|r| {
            r["plan_executing"] == json!(true)
                || list(r, "plan")
                    .iter()
                    .any(|o| o["executing"] == json!(true))
        }) || row
            .and_then(|r| r["army_group"].as_str())
            .is_some_and(|g| executing(roster, g));
        // an army that stands in the same state for STUCK_DAYS of the war while its group's
        // front executes is moved up by objectives (pb1, qc1, qc2, 19 Nov 2026: Armee Mitte
        // stood in German Silesia 71 for 56 war days on the group's executing front)
        if let Some(main) = row.and_then(|r| r["main_state"].as_i64()) {
            let at = key(id, &format!("at:{name}"));
            let since = key(id, &format!("atsince:{name}"));
            if state.done.get(&at) != Some(&main) {
                state.done.insert(at, main);
                state.done.insert(since, now);
            } else if state
                .done
                .get(&since)
                .is_some_and(|t| now - t >= STUCK_DAYS * DAY)
                && !group_driven(state, id, op, roster, row)
            {
                state.done.insert(key(id, &format!("drive:{name}")), now);
                state.op_notes.insert(
                    format!("{id}:{name}"),
                    format!(
                        "{name}: stood in state {main} for {STUCK_DAYS} war days: driven by objectives as one stack"
                    ),
                );
                continue;
            }
        }
        if own_executing || wing {
            state.done.remove(&idle);
            continue;
        }
        let since = *state.done.entry(idle).or_insert(now);
        if now - since >= SOLO_AFTER {
            state.done.insert(solo, now);
            state.op_notes.insert(
                format!("{id}:{name}"),
                format!(
                    "{name}: no executing order of its own for {} game hours at war: its own front and offensive",
                    (now - since)
                ),
            );
        }
    }
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
    // The enemy's largest victory point (its capital) is every formation's LAST row: it is
    // taken late and from several sides, after the points around it (qc2-qc4, 19 Nov 2026: the
    // columns struck Warsaw, urban behind a river, from one side for weeks at 17-31%
    // organisation and casualties went to 1:1 while Krakow, Kielce and Lublin stood open).
    let capital = best
        .values()
        .max_by(|a, b| a.1.total_cmp(&b.1).then(b.0.cmp(&a.0)))
        .map(|b| b.0);
    let me = player(tools, plan);
    let names: Vec<String> = strikers.iter().map(|u| u.name.clone()).collect();
    let pocket = if op["pockets"] == json!(false) {
        None
    } else {
        pocket_step(state, tools, roster, &names, &fighting, &best, &me, now)?
    };
    let mut rows: Vec<Value> = Vec::new();
    let mut knobs = json!({});
    for u in &strikers {
        if let Some(pk) = &pocket {
            if pk.arms.contains(&u.name) {
                // an arm of the pocket: the neck only while it closes, nothing while it holds
                if !pk.closed {
                    rows.push(json!({"army": u.name, "province": pk.province, "path": []}));
                }
                continue;
            }
        }
        let mut last: Vec<i64> = Vec::new();
        for s in &u.line {
            if let Some((p, _)) = best.get(s) {
                if Some(*p) == capital {
                    last.push(*p);
                } else {
                    rows.push(json!({"army": u.name, "province": p, "path": []}));
                }
            }
        }
        for p in last {
            rows.push(json!({"army": u.name, "province": p, "path": []}));
        }
        if let Some(f) = list(op, "formations")
            .iter()
            .find(|f| name_of(state, roster, text(&f["army"])) == u.name)
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
    concentrate(&mut knobs);
    // a driven infantry army does not fight as one stack: it widens (at most STACK_INFANTRY a
    // province), feeds and garrisons its points (qc5, 19 Nov 2026: Armee Nord and Süd struck as
    // 17- and 20-division stacks and Germany lost 129k to Poland's 68k)
    let id = text(&op["id"]);
    for u in &strikers {
        if state
            .done
            .contains_key(&key(id, &format!("drive:{}", u.name)))
        {
            knobs["armies"][u.name.as_str()] = json!({
                "widen": true, "feed": true, "garrison": true, "stack": STACK_INFANTRY
            });
        }
    }
    let mut p = plan.clone();
    p["war"]["strike"] = knobs;
    p["vp_targets"] = json!(rows);
    // the arms of a closed ring hold it: no strike order goes to them
    let holding: Vec<String> = pocket
        .filter(|pk| pk.closed)
        .map(|pk| pk.arms)
        .unwrap_or_default();
    let reserve: Vec<String> = list(roster, "armies")
        .iter()
        .map(|a| text(&a["army"]).to_string())
        .filter(|n| !strikers.iter().any(|u| &u.name == n) || holding.contains(n))
        .collect();
    Ok(super::plan_strike::step(state, tools, &p, lease, &me, &fighting, &reserve, now)?.flatten())
}

/// Game hours between two reads of the pockets the enemy's map offers.
const POCKET_SCAN: GameHour = 2 * DAY;
/// Game days two armour formations have to meet in a pocket's neck before the pocket is given up
/// ("cut off within about three days": twice that, the march included).
const POCKET_CLOSE_DAYS: i64 = 6;
/// Game days the arms hold a closed ring while the group's plan reduces the pocket.
const POCKET_HOLD_DAYS: i64 = 10;
/// Game days a given-up neck is not tried again.
const POCKET_RETRY_DAYS: i64 = 20;

/// The armour's pocket: the two strike formations sent to meet in its neck, the neck's province
/// they march at, and whether the ring is closed (the neck held: the arms hold it).
struct PocketOrder {
    arms: Vec<String>,
    province: i64,
    closed: bool,
}

/// THE POCKET ORDER, the one place the owner wants direct orders (19 Nov 2026: "Micro generally
/// for creating encirclements"): from the state-control read (hoi4_encirclement_candidates, one
/// neck state) the best pocket two strike formations can cut off is chosen, both march at the
/// neck by whole-army moves, the ring is held (no strike order to the arms) while the group's
/// plan reduces it, and the armour goes back to its objectives when the pocket's states are
/// held or after POCKET_HOLD_DAYS. A neck not taken in POCKET_CLOSE_DAYS is given up for
/// POCKET_RETRY_DAYS. Pockets closed are counted in `op:pockets_closed`. `pockets: false` on the
/// operation switches it off.
#[allow(clippy::too_many_arguments)]
fn pocket_step(
    state: &mut State,
    tools: &dyn Tools,
    roster: &Value,
    strikers: &[String],
    fighting: &HashSet<String>,
    best: &HashMap<i64, (i64, f64)>,
    me: &str,
    now: GameHour,
) -> Result<Option<PocketOrder>> {
    let arms_now = |state: &State| -> Vec<String> {
        strikers
            .iter()
            .filter(|n| state.done.contains_key(&format!("pocket:arm:{n}")))
            .cloned()
            .collect()
    };
    let end = |state: &mut State| {
        state.done.retain(|k, _| {
            !(k.starts_with("pocket:arm:")
                || k.starts_with("pocket:in:")
                || matches!(
                    k.as_str(),
                    "pocket:neck" | "pocket:prov" | "pocket:at" | "pocket:closed"
                ))
        });
    };
    if let Some(neck) = state.done.get("pocket:neck").copied() {
        let arms = arms_now(state);
        let province = state.done.get("pocket:prov").copied().unwrap_or(0);
        let at = state.done.get("pocket:at").copied().unwrap_or(now);
        let inside: Vec<i64> = state
            .done
            .keys()
            .filter_map(|k| k.strip_prefix("pocket:in:").and_then(|s| s.parse().ok()))
            .collect();
        if arms.len() < 2 {
            end(state);
            return Ok(None);
        }
        let closed_at = state.done.get("pocket:closed").copied();
        if closed_at.is_none() && !held(tools, me, &[neck]).is_empty() {
            state.done.insert("pocket:closed".into(), now);
            *state.done.entry("op:pockets_closed".into()).or_insert(0) += 1;
            state.op_notes.insert(
                "pocket".into(),
                format!(
                    "pocket closed at state {neck} by {}: {} state(s) {inside:?} cut off, the ring held",
                    arms.join(" and "),
                    inside.len()
                ),
            );
        }
        if let Some(c) = state.done.get("pocket:closed").copied() {
            let reduced = held(tools, me, &inside).len() == inside.len();
            if reduced || now - c >= POCKET_HOLD_DAYS * DAY {
                state.op_notes.insert(
                    "pocket".into(),
                    format!(
                        "pocket at state {neck}: {} after {} days; the armour goes on",
                        if reduced { "reduced" } else { "held" },
                        (now - c) / DAY
                    ),
                );
                end(state);
                return Ok(None);
            }
            return Ok(Some(PocketOrder {
                arms,
                province,
                closed: true,
            }));
        }
        if now - at >= POCKET_CLOSE_DAYS * DAY {
            state.done.insert(format!("pocket:tried:{neck}"), now);
            state.op_notes.insert(
                "pocket".into(),
                format!(
                    "pocket at state {neck}: neck not taken in {POCKET_CLOSE_DAYS} days, given up"
                ),
            );
            end(state);
            return Ok(None);
        }
        return Ok(Some(PocketOrder {
            arms,
            province,
            closed: false,
        }));
    }
    if strikers.len() < 2
        || state
            .done
            .get("pocket:scan")
            .is_some_and(|t| now >= *t && now - t < POCKET_SCAN)
    {
        return Ok(None);
    }
    state.done.insert("pocket:scan".into(), now);
    let mut tags: Vec<&String> = fighting.iter().collect();
    tags.sort();
    let Ok(read) = tools.call(
        "hoi4_encirclement_candidates",
        &json!({"enemy": tags, "max_gap": 1, "limit": 8}),
    ) else {
        return Ok(None);
    };
    let mains: HashMap<String, (f64, f64)> = strikers
        .iter()
        .filter_map(|n| {
            let s = army_row(roster, n)?["main_state"].as_i64()?;
            Some((n.clone(), state.centers.get(&(s as i32)).copied()?))
        })
        .collect();
    for pk in list(&read, "pockets") {
        if pk["already_cut_off"] == json!(true)
            || pk["pincer_ready"] != json!(true)
            || pk["capital_inside"] == json!(true)
        {
            continue;
        }
        let necks: Vec<i64> = list(pk, "neck_states")
            .iter()
            .filter_map(|n| n["state_id"].as_i64())
            .collect();
        let [neck] = necks.as_slice() else {
            continue;
        };
        let neck = *neck;
        if state
            .done
            .get(&format!("pocket:tried:{neck}"))
            .is_some_and(|t| now - t < POCKET_RETRY_DAYS * DAY)
        {
            continue;
        }
        let Some((province, _)) = best.get(&neck).copied() else {
            continue;
        };
        // the two strike formations nearest the neck (map centres of their main states)
        let Some(at) = state.centers.get(&(neck as i32)).copied() else {
            continue;
        };
        let mut near: Vec<(f64, String)> = strikers
            .iter()
            .filter_map(|n| {
                let (x, z) = mains.get(n).copied()?;
                Some(((x - at.0).hypot(z - at.1), n.clone()))
            })
            .collect();
        near.sort_by(|a, b| a.0.total_cmp(&b.0));
        if near.len() < 2 {
            continue;
        }
        let inside: Vec<i64> = list(pk, "pocket_states")
            .iter()
            .filter_map(|n| n["state_id"].as_i64())
            .collect();
        state.done.insert("pocket:neck".into(), neck);
        state.done.insert(format!("pocket:chosen:{neck}"), now);
        state.done.insert("pocket:prov".into(), province);
        state.done.insert("pocket:at".into(), now);
        for (_, n) in near.iter().take(2) {
            state.done.insert(format!("pocket:arm:{n}"), now);
        }
        for s in &inside {
            state.done.insert(format!("pocket:in:{s}"), 1);
        }
        let arms: Vec<String> = near.iter().take(2).map(|(_, n)| n.clone()).collect();
        state.op_notes.insert(
            "pocket".into(),
            format!(
                "pocket: {} meet in state {neck} (province {province}) to cut off {inside:?}",
                arms.join(" and ")
            ),
        );
        return Ok(Some(PocketOrder {
            arms,
            province,
            closed: false,
        }));
    }
    Ok(None)
}

/// An operation's strike formation fights as ONE stack on whole-army orders: no part of it is
/// split off (no stack limit, widening, feeding, garrison or post), it never rests longer than a
/// day, and a strike that has not taken its objective in STRIKE_MAX_DAYS gives way to the next
/// objective. pb1 and g17k (19 Nov 2026): the division-list orders of the split column failed
/// about half the time ("not on screen", 30 s each) while every whole-army order read back, and
/// the column stood 70 days after its first state between rests, closures and failed splits.
fn concentrate(knobs: &mut Value) {
    for k in ["widen", "feed", "garrison", "halt"] {
        knobs[k] = json!(false);
    }
    knobs["stack"] = json!(0);
    knobs["cover"] = json!(0);
    let capped = |v: &Value, default: f64, max: f64| v.as_f64().unwrap_or(default).min(max);
    knobs["rest_days"] = json!(capped(
        &knobs["rest_days"],
        STRIKE_REST_DAYS,
        STRIKE_REST_DAYS
    ));
    knobs["max_days"] = json!(capped(&knobs["max_days"], STRIKE_MAX_DAYS, STRIKE_MAX_DAYS));
    knobs["close_days"] = json!(capped(&knobs["close_days"], 3.0, 3.0));
    knobs["worn_gate"] = json!(knobs["worn_gate"].as_f64().unwrap_or(STRIKE_WORN));
    // a column under STRIKE_SUPPLY percent supply stops where it stands and waits for the supply
    // and the infantry; it resumes once fed (qc4: Warsaw struck at 4-8% supply)
    if knobs.get("supply_gate").is_none() {
        knobs["supply_gate"] = json!(STRIKE_SUPPLY);
    }
    knobs["rest"] = json!(capped(&knobs["rest"], 0.25, 0.25));
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
    // the attacking mobile formations (not the hold armies) and the line formations with the
    // number of divisions each names
    let strike_names: HashSet<String> = list(op, "formations")
        .iter()
        .filter(|f| text(&f["role"]) == "mobile")
        .map(|f| resolve(state, text(&f["army"])))
        .filter(|a| !a.is_empty())
        .collect();
    let lines: Vec<(String, i64)> = list(op, "formations")
        .iter()
        .filter(|f| text(&f["role"]) == "line")
        .map(|f| {
            let n: i64 = want_of(f).unwrap_or_default().iter().map(|(_, n)| *n).sum();
            (resolve(state, text(&f["army"])), n)
        })
        .filter(|(a, _)| !a.is_empty())
        .collect();
    for (name, hint, want, from, near) in &mobiles {
        // a formed army that kept the game's name: renamed once; the operation orders it under
        // the game's name (o1e: the name box read empty right after the detach; qb2, qc5,
        // 19 Nov 2026: every retry failed the same way and the 7 s tries before the war pushed
        // the declaration 10-18 days late)
        if name != hint && army_row(roster, hint).is_none() && army_row(roster, name).is_some() {
            let rk = key(id, &format!("rename:{hint}"));
            let n = state.done.get(&rk).copied().unwrap_or(0);
            if n < RENAME_TRIES && !state.skipped(now, "oprename", hint) {
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
        let tries = key(id, &format!("asmtry:{name}"));
        let n = state.done.get(&tries).copied().unwrap_or(0);
        if !state.done.contains_key(&done) && n >= 1 && due(state, id) {
            state.done.insert(done, now);
            state.op_notes.insert(
                format!("{id}:{name}"),
                format!(
                    "{name}: assembly left after {n} move(s) at the declaration; the rest fights in its own army"
                ),
            );
            continue;
        }
        if state.done.contains_key(&done) || state.skipped(now, "opasm", name) {
            continue;
        }
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
        // A mobile formation holds only its own templates: what else stands in it (the game's
        // start army, an absorbed trainee) goes to the line formation most short of its own
        // divisions before anything is gathered in (g17k, g17m: Panzergruppe Ost was the start
        // army with 17 infantry divisions and marched to its narrow front with them).
        if strike_names.contains(name.as_str()) && army_row(roster, name).is_some() {
            let own: Vec<&Value> = divisions
                .iter()
                .filter(|d| text(&d["army"]) == name)
                .collect();
            let mut strays: Vec<String> = own
                .iter()
                .filter(|d| !want.iter().any(|(t, _)| t == text(&d["template"])))
                .filter_map(|d| d["id"].as_str().map(str::to_string))
                .collect();
            if strays.len() == own.len() {
                // the formation's army is never emptied: one stays until its own are in
                strays.truncate(strays.len().saturating_sub(1));
            }
            strays.truncate(18);
            let target = lines
                .iter()
                .filter(|(army, _)| army != name && army_row(roster, army).is_some())
                .max_by_key(|(army, n)| n - size(army) as i64)
                .map(|(army, _)| army.clone());
            let out_key = key(id, &format!("asmout:{name}"));
            let k = state.done.get(&out_key).copied().unwrap_or(0);
            if let Some(target) = target.filter(|_| !strays.is_empty()) {
                if k < 6 && !state.skipped(now, "opasmout", name) {
                    state.done.insert(out_key, k + 1);
                    let line = order(
                        state,
                        tools,
                        lease,
                        id,
                        &format!(
                            "assemble {name}: {} division(s) of other templates out to {target}",
                            strays.len()
                        ),
                        "hoi4_army_detach",
                        json!({"army": name, "divisions": strays, "into": target}),
                    )?;
                    if state.last_ok == Some(false) {
                        state.skip(now, "opasmout", name, DAY / 4);
                    }
                    return Ok(Some(line));
                }
            }
        }
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

/// Marches a reserve army may take to its station before it takes its standing order where it
/// stands.
const RESERVE_MARCHES: i64 = 4;
/// Tries of a reserve army's standing order.
const RESERVE_ORDERS: i64 = 2;

/// The reserve rows' army hints released `to: "operation"` (they join the group at its regroup).
fn released_to_group(state: &State, id: &str, op: &Value) -> Vec<String> {
    if op["reserve_orders"] != json!(true) {
        return Vec::new();
    }
    list(op, "reserve")
        .iter()
        .filter(|r| text(&r["to"]) == "operation")
        .map(|r| text(&r["army"]).to_string())
        .filter(|a| !a.is_empty() && state.done.contains_key(&key(id, &format!("rel:{a}"))))
        .collect()
}

/// The reserve rows' armies the operation owns (`reserve_orders: true`), as game names.
fn reserve_names(state: &State, op: &Value, roster: &Value) -> Vec<String> {
    if op["reserve_orders"] != json!(true) {
        return Vec::new();
    }
    list(op, "reserve")
        .iter()
        .map(|r| name_of(state, roster, text(&r["army"])))
        .filter(|a| !a.is_empty())
        .collect()
}

/// Whether an army carries any order of its own (battle plan, fallback or area defence).
fn has_orders(roster: &Value, name: &str) -> bool {
    army_row(roster, name).is_some_and(|r| {
        !list(r, "plan").is_empty()
            || r["battle_plan_orders"].as_i64().unwrap_or(0) > 0
            || r["defensive_orders"].as_i64().unwrap_or(0) > 0
    })
}

/// RESERVES (`reserve_orders: true`): every reserve row's army gets a real standing order. Before
/// its release (`release_when`) it marches as one army to its `station` (whole-army move, at most
/// RESERVE_MARCHES tries, never while it stands in contact with an enemy), then holds there: a
/// fallback line through the row's `fallback` states, else an area defence of the station. Once
/// `release_when` holds the row is released: released `to: "operation"` it joins the group at the
/// group's next regroup; released to a hold it keeps its standing order (one front per border
/// piece: no second front is drawn for it). One order per call.
#[allow(clippy::too_many_arguments)]
fn reserves(
    state: &mut State,
    tools: &dyn Tools,
    id: &str,
    op: &Value,
    roster: &Value,
    wars: &Value,
    lease: &str,
    now: GameHour,
) -> Result<Option<String>> {
    if op["reserve_orders"] != json!(true) {
        return Ok(None);
    }
    for row in list(op, "reserve") {
        let hint = text(&row["army"]).to_string();
        let name = name_of(state, roster, &hint);
        let Some(army) = army_row(roster, &name) else {
            continue;
        };
        let rel = key(id, &format!("rel:{hint}"));
        if !state.done.contains_key(&rel) {
            if !row["release_when"].is_null() && holds(state, tools, &row["release_when"], now) {
                state.done.insert(rel, now);
                let to = text(&row["to"]);
                let line = format!(
                    "operation {id}: reserve {name} released ({})",
                    if to == "operation" {
                        "joins the group at its next regroup".to_string()
                    } else {
                        format!("keeps its standing order for {to}")
                    }
                );
                state.op_notes.insert(format!("{id}:{name}"), line.clone());
                if to == "operation" {
                    // its own orders would keep it out of the group's front
                    state.done.remove(&key(id, &format!("resord:{name}")));
                    if has_orders(roster, &name) {
                        return Ok(Some(order(
                            state,
                            tools,
                            lease,
                            id,
                            &format!("reserve {name} released: clear its standing order"),
                            "hoi4_army_plan",
                            json!({"army": name, "clear_orders": true}),
                        )?));
                    }
                }
                return Ok(Some(line));
            }
        } else {
            continue;
        }
        if state.skipped(now, "opres", &name) {
            continue;
        }
        let station = row["station"].as_i64();
        let total = army["division_count"].as_i64().unwrap_or(0).max(1);
        let there = station.map_or(total, |s| {
            army["states"][s.to_string()].as_i64().unwrap_or(0)
        });
        let marches_key = key(id, &format!("resmarch:{name}"));
        let marches = state.done.get(&marches_key).copied().unwrap_or(0);
        if let Some(s) = station.filter(|_| there * 2 < total) {
            if marches < RESERVE_MARCHES && !in_contact(roster, wars, &name) {
                state.done.insert(marches_key, marches + 1);
                state.skip(now, "opres", &name, 2 * DAY);
                return Ok(Some(order(
                    state,
                    tools,
                    lease,
                    id,
                    &format!("reserve {name} marches to its station {s}"),
                    "hoi4_army_move",
                    json!({"army": name, "state_id": s}),
                )?));
            }
            if marches < RESERVE_MARCHES {
                continue;
            }
        }
        let ord_key = key(id, &format!("resord:{name}"));
        let tries = state.done.get(&ord_key).copied().unwrap_or(0);
        if has_orders(roster, &name) || tries >= RESERVE_ORDERS {
            continue;
        }
        state.done.insert(ord_key, tries + 1);
        state.skip(now, "opres", &name, DAY);
        let fallback = ints(&row["fallback"]);
        let (what, tool, args) = if !fallback.is_empty() {
            (
                format!("reserve {name} holds the fallback line {fallback:?}"),
                "hoi4_army_fallback",
                json!({"army": name, "states": fallback}),
            )
        } else if let Some(s) = station {
            (
                format!("reserve {name} holds its station {s} (area defence)"),
                "hoi4_army_area_defense",
                json!({"army": name, "states": [s]}),
            )
        } else {
            continue;
        };
        return Ok(Some(order(state, tools, lease, id, &what, tool, args)?));
    }
    Ok(None)
}

/// REDEPLOYMENT (the operation's `redeploy` rows, read when its enemies are out): each row's
/// army, unless it stands in contact with an enemy still at war, is cleared of its orders and
/// moved as one army to the row's `state` (Poland to the West, the West to the East). One order
/// per call; each row is ordered once and never holds the operation's end.
#[allow(clippy::too_many_arguments)]
fn redeploy(
    state: &mut State,
    tools: &dyn Tools,
    id: &str,
    op: &Value,
    roster: &Value,
    wars: &Value,
    lease: &str,
    now: GameHour,
) -> Result<Option<String>> {
    for row in list(op, "redeploy") {
        let name = name_of(state, roster, text(&row["army"]));
        let (Some(army), Some(to)) = (army_row(roster, &name), row["state"].as_i64()) else {
            continue;
        };
        let k = key(id, &format!("redeploy:{name}"));
        if state.done.contains_key(&k) {
            continue;
        }
        let total = army["division_count"].as_i64().unwrap_or(0).max(1);
        let there = army["states"][to.to_string()].as_i64().unwrap_or(0);
        if there * 2 >= total || in_contact(roster, wars, &name) {
            state.done.insert(k, now);
            let why = if there * 2 >= total {
                "already there"
            } else {
                "stays: in contact with an enemy"
            };
            state
                .op_notes
                .insert(format!("{id}:{name}"), format!("redeploy {name}: {why}"));
            continue;
        }
        let clear = key(id, &format!("redeploy_clear:{name}"));
        if has_orders(roster, &name) && !state.done.contains_key(&clear) {
            state.done.insert(clear, now);
            return Ok(Some(order(
                state,
                tools,
                lease,
                id,
                &format!("redeploy {name}: clear its orders first"),
                "hoi4_army_plan",
                json!({"army": name, "clear_orders": true}),
            )?));
        }
        state.done.insert(k, now);
        return Ok(Some(order(
            state,
            tools,
            lease,
            id,
            &format!("redeploy {name} to state {to}"),
            "hoi4_army_move",
            json!({"army": name, "state_id": to}),
        )?));
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
    // the group: formed when its line armies stand in none; formed again with every line army
    // when one of them stands outside it before the war (g17m, 19 Nov 2026: the group was made in
    // 1936 of the two line armies that existed, Armee Süd formed later and stood outside it on
    // the group's war day, with no front and no planning bonus). The game keeps the fronts and
    // planning of a regrouped army (probe rp2).
    // With `regroup_at_war: true` the regroup also runs while the country fights someone else
    // (Germany stays at war with Britain after Poland: without it the armies that were not in the
    // group for Poland never join it for France or the USSR): it runs while the OPERATION's
    // enemies are not at war with us, takes an army standing in another group too, and never
    // moves an army another running operation owns or one standing in contact with an enemy
    // (its states on a border with a country at war with us, or across it).
    // the split of two groups needs the front states' map centres
    if op["group_cap"].is_i64() {
        let mut tags: Vec<String> = units.iter().map(|u| u.enemy.clone()).collect();
        tags.push(player(tools, plan));
        learn_centres(state, tools, &tags);
    }
    let at_war_regroup = op["regroup_at_war"] == json!(true);
    let line_armies: Vec<&Unit> = units.iter().filter(|u| u.role == Role::Line).collect();
    let movable = |name: &str| -> bool {
        !at_war_regroup || (!owned_elsewhere(state, id, name) && !in_contact(roster, wars, name))
    };
    let outside_of = |r: &Value| -> bool {
        if at_war_regroup {
            r["army_group"].as_str() != group
        } else {
            r["army_group"].is_null()
        }
    };
    let released: Vec<String> = released_to_group(state, id, op)
        .iter()
        .map(|h| resolve(state, h))
        .collect();
    let outside = line_armies
        .iter()
        .map(|u| &u.name)
        .chain(released.iter())
        .any(|n| army_row(roster, n).is_some_and(outside_of) && movable(n));
    let regroup_key = key(id, "group_again");
    let regroups = state.done.get(&regroup_key).copied().unwrap_or(0);
    let calm = if at_war_regroup {
        let fighting = war_tags(wars);
        !strings(op, "enemies").iter().any(|e| fighting.contains(e))
    } else {
        !state.war_active
    };
    let regroup = group.is_some() && outside && calm && regroups < 3;
    // TWO GROUPS (`group_cap`): each half of the line armies is formed into its own group, one
    // group order per call; the single-group regroup never runs for a split operation
    let line_names: Vec<String> = line_armies.iter().map(|u| u.name.clone()).collect();
    let parts = split_parts(state, op, roster, &line_names);
    if let Some((_, low, high)) = &parts {
        let tries_key = key(id, "split_again");
        let tries = state.done.get(&tries_key).copied().unwrap_or(0);
        for part in [low, high] {
            if part.len() < 2 || part_formed(roster, part, &line_names).is_some() {
                continue;
            }
            let fresh = part.iter().all(|n| group_of(roster, n).is_none());
            if !(calm || fresh)
                || tries >= 6
                || !part.iter().all(|n| movable(n))
                || state.skipped(now, "opgrp", id)
            {
                break;
            }
            let names = part.clone();
            state.skip(now, "opgrp", id, 2 * DAY);
            state.done.insert(tries_key, tries + 1);
            let line = order(
                state,
                tools,
                lease,
                id,
                &format!("form one of the two groups of {}", names.join(", ")),
                "hoi4_army_group_create",
                json!({"armies": names}),
            )?;
            return Ok(Some(line));
        }
    }
    if parts.is_none()
        && (group.is_none() || regroup)
        && line_armies.len() >= 2
        && !state.skipped(now, "opgrp", id)
    {
        let joins = |n: &str| -> bool {
            army_row(roster, n).is_some_and(|r| {
                (outside_of(r) && movable(n)) || (regroup && r["army_group"].as_str() == group)
            })
        };
        let mut names: Vec<String> = line_armies.iter().map(|u| u.name.clone()).collect();
        let planned = list(plan, "army_group")
            .iter()
            .find(|g| item_id(g).as_deref() == op["group"].as_str());
        let mut hints = planned.map(|g| strings(g, "armies")).unwrap_or_default();
        hints.extend(released);
        for hint in hints {
            let n = resolve(state, &hint);
            if joins(&n) && !names.contains(&n) {
                names.push(n);
            }
        }
        names.retain(|n| joins(n));
        if names.len() >= 2 {
            state.skip(now, "opgrp", id, 2 * DAY);
            if regroup {
                state.done.insert(regroup_key, regroups + 1);
            }
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
    let mut tags: Vec<String> = units.iter().map(|u| u.enemy.clone()).collect();
    tags.push(player(tools, plan));
    learn_centres(state, tools, &tags);
    // the group's one offensive at the far side of what must fall, computed once in peace
    if group.is_some() && war_tags(wars).is_empty() {
        if let Some(far) = learn_deep(state, tools, id, op, &strings(op, "enemies")) {
            let line = format!(
                "operation {id}: the group's one offensive runs at the far line {far:?} (behind {:.0}% of the enemy's victory points)",
                DEEP_SHARE * 100.0
            );
            state.op_notes.insert(id.to_string(), line.clone());
            return Ok(Some(line));
        }
    }
    // 1. a line army, and an army of a hold group, carries no orders of its own (it would
    //    ignore its group's front): cleared once; a group front drawn while one still had its
    //    own orders is cleared too and drawn again with every army on it
    let mut sets: Vec<(String, Vec<String>)> = units
        .iter()
        .filter(|u| u.role == Role::Group && u.side.is_some())
        .map(|u| (u.name.clone(), u.members.clone()))
        .collect();
    if let Some(g) = group.filter(|_| sets.is_empty()) {
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
    // 2. the group's front and its offensive FIRST, before any assembly, march or commander: the
    // planning bonus grows only while the line stands (PLANNING_GAIN 0.02 a day, the cap 0.3
    // and more from doctrines and the marshal), and the attack is the battle plan executed
    // with that bonus (the owner's war model, 19 Nov 2026). Drawn after the assembly and the
    // marches (rs5/rs6) the group's line stood six game days later.
    let fighting = war_tags(wars);
    for u in units.iter().filter(|u| u.role == Role::Group) {
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
    // ASSEMBLE: a mobile formation gathers its templates' divisions out of the other armies
    if let Some(line) = assemble(state, tools, id, op, units, roster, lease, now)? {
        return Ok(Some(line));
    }
    // a strike formation marches to its start state (its `front.own_states`, first) before
    // the war, at once after its assembly and before the fronts are drawn (the march is game
    // days, the drawing real seconds: drawn first, the march began 12 days into the bed's prepare
    // and the declaration came 6-9 days late, rd2-rd6 19 Nov 2026): assembled where its divisions stood, the column began the war three to eleven
    // days' march from its first objective (qc1/qc2, 19 Nov 2026: Panzergruppe Ost formed in
    // Pomerania for a Lodz row and went by the Danzig corridor). Until it stands there the
    // operation is not ready (the declaration waits, at most its own cap).
    if !war_tags(wars)
        .iter()
        .any(|t| units.iter().any(|u| &u.enemy == t))
    {
        for u in units.iter().filter(|u| u.role == Role::Strike || u.spear) {
            let Some(start) = u.own_states.first().copied() else {
                continue;
            };
            let Some(row) = army_row(roster, &u.name) else {
                continue;
            };
            let total = row["division_count"].as_i64().unwrap_or(0).max(1);
            let there = row["states"][start.to_string()].as_i64().unwrap_or(0);
            if there * 2 >= total {
                continue;
            }
            state.skip(now, "opasm", &format!("march:{}", u.name), DAY / 2);
            if state.skipped(now, "opmarch", &u.name) {
                continue;
            }
            state.skip(now, "opmarch", &u.name, 2 * DAY);
            let line = order(
                state,
                tools,
                lease,
                id,
                &format!("{} marches to its start state {start}", u.name),
                "hoi4_army_move",
                json!({"army": u.name, "state_id": start}),
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
    // 2-4. the holds first (a hold stands before any attack is drawn: qb2, 19 Nov 2026, the
    // West hold was still undrawn when the declaration's wait ran out and France took three
    // states in a week), then the group, then mobile and wing armies
    let mut ordered: Vec<&Unit> = units.iter().filter(|u| u.role == Role::Hold).collect();
    ordered.extend(units.iter().filter(|u| u.role == Role::Group));
    ordered.extend(
        units
            .iter()
            .filter(|u| matches!(u.role, Role::Mobile | Role::Wing)),
    );
    // a unit already at war with its enemy prepares by the war's rules (it15, 18 Nov 2026:
    // Italy starts at war with Ethiopia; the peace rules cleared every offensive that read back
    // over two of the line's three states until the draws ran out, and nothing ever executed)
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
    // the reserves' standing orders, after every front
    if let Some(line) = reserves(state, tools, id, op, roster, wars, lease, now)? {
        return Ok(Some(line));
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
    // REPLACE, NEVER ADD (`replace_arrows`): an arrow that misses the line goes on its own (the
    // bar's delete tool on that arrow), so the front and its planning bonus stay and the new
    // arrow is the front's only one; the game gives a second arrow on one front no divisions
    // (spr1; b38g, 19 Nov 2026: the opening arrow kept all 56 divisions for weeks and the phase
    // line's arrow had none). At war, when the delete does not read back, the unit's orders are
    // cleared once and the front and the arrow drawn again.
    if stale && !u.spear && op_replaces(state, id) && redraws < REDRAWS {
        let miss: Option<i64> = offensives(roster, &u.name)
            .into_iter()
            .find(|off| !aims(u, off, line))
            .and_then(|off| off["id"].as_i64());
        let nodelete = key(id, &format!("nodelete:{}", u.name));
        if let Some(oid) = miss {
            if !state.done.contains_key(&nodelete) {
                state.done.insert(k, redraws + 1);
                let out = order(
                    state,
                    tools,
                    lease,
                    id,
                    &format!(
                        "delete the arrow {oid} of {} (it misses the line {tag}); the front stays",
                        u.name
                    ),
                    "hoi4_army_plan",
                    json!({"army": u.name, "delete_order": oid}),
                )?;
                if state.last_ok == Some(false) {
                    state.done.insert(nodelete, now);
                    state.skip(now, "opdraw", &u.name, 2);
                }
                return Ok(Some(out));
            }
            if !peace {
                state.done.insert(k, redraws + 1);
                let out = order(
                    state,
                    tools,
                    lease,
                    id,
                    &format!(
                        "clear {} (its arrow misses the line {tag} and would not delete alone): front and arrow drawn again",
                        u.name
                    ),
                    "hoi4_army_plan",
                    json!({"army": u.name, "clear_orders": true}),
                )?;
                return Ok(Some(out));
            }
        }
    }
    // a spearhead's divisions stay on the order they were given (a second arrow on the same front
    // gets none: spr1, 19 Nov 2026): a spear whose order no longer reaches its line is cleared
    // and painted again at war too
    if stale && u.spear && redraws >= 1 {
        state
            .done
            .insert(key(id, &format!("nospear:{}", u.name)), now);
        let line = format!(
            "operation {id}: the spearhead of {} misses the line {tag}: its own offensive line instead",
            u.name
        );
        state.op_notes.insert(id.to_string(), line.clone());
        return Ok(Some(line));
    }
    if stale && (peace || u.spear) {
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
        &format!(
            "{} of {} through {tag}",
            if u.spear { "spearhead" } else { "offensive" },
            u.name
        ),
        "hoi4_army_offensive",
        if u.spear {
            offensive_order(&u.name, &line[line.len() - 1..], None, true)
        } else {
            offensive_order(&u.name, line, u.anchor, false)
        },
    )?;
    if state.last_ok == Some(false) {
        state.skip(now, "opdraw", &u.name, 2);
    }
    Ok(Some(line))
}

/// The operation replaces an arrow that misses its line instead of adding one (`replace_arrows`).
fn op_replaces(state: &State, id: &str) -> bool {
    state.done.get(&key(id, "replace_arrows")) == Some(&1)
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

/// One enemy state as the far line reads it: id, victory points, map centre.
#[derive(Clone, Copy, Debug)]
struct Far {
    id: i64,
    vp: f64,
    x: f64,
    z: f64,
}

/// The group's ONE offensive line, at the far side of what must fall (`offensive.deep`): the
/// enemy states ordered by depth from the group's own front (`origin`) along the way to the
/// enemy's victory points; the nearest of them that hold DEEP_SHARE of the points must fall;
/// the line runs through the deepest of those (a band of DEEP_BAND of the enemy's depth), the
/// nearer of two states side by side giving way to the deeper, listed from one end to the other.
/// A player draws it once and lets it run; a phase line's second arrow on the same front gets no
/// divisions from the game (b38g, 19 Nov 2026: Poland stood at 105-109 points for weeks while
/// the group's opening arrow kept all 56 divisions). Empty when there is nothing to order.
fn far_line(origin: (f64, f64), states: &[Far]) -> Vec<i64> {
    let total: f64 = states.iter().map(|s| s.vp.max(0.0)).sum();
    if states.len() < 2 || total <= 0.0 {
        return Vec::new();
    }
    let (cx, cz) = states.iter().fold((0.0, 0.0), |a, s| {
        (
            a.0 + s.x * s.vp.max(0.0) / total,
            a.1 + s.z * s.vp.max(0.0) / total,
        )
    });
    let (dx, dz) = (cx - origin.0, cz - origin.1);
    let len = (dx * dx + dz * dz).sqrt();
    if len < 1e-6 {
        return Vec::new();
    }
    let (ux, uz) = (dx / len, dz / len);
    let depth = |s: &Far| (s.x - origin.0) * ux + (s.z - origin.1) * uz;
    let side = |s: &Far| -(s.x - origin.0) * uz + (s.z - origin.1) * ux;
    let mut order: Vec<&Far> = states.iter().collect();
    order.sort_by(|a, b| depth(a).total_cmp(&depth(b)).then(a.id.cmp(&b.id)));
    let mut sum = 0.0;
    let mut needed: Vec<&Far> = Vec::new();
    for s in order.iter() {
        if sum >= DEEP_SHARE * total {
            break;
        }
        sum += s.vp.max(0.0);
        needed.push(s);
    }
    let near = order.first().map_or(0.0, |s| depth(s));
    let deepest = needed.iter().map(|s| depth(s)).fold(f64::MIN, f64::max);
    let band = (deepest - near) * DEEP_BAND;
    let (lo, hi) = states
        .iter()
        .map(side)
        .fold((f64::MAX, f64::MIN), |a, v| (a.0.min(v), a.1.max(v)));
    let gap = (hi - lo) * DEEP_GAP;
    let candidates: Vec<&Far> = needed
        .iter()
        .copied()
        .filter(|s| depth(s) >= deepest - band)
        .collect();
    let mut line: Vec<&Far> = candidates
        .iter()
        .copied()
        .filter(|s| {
            !candidates
                .iter()
                .any(|o| o.id != s.id && (side(o) - side(s)).abs() < gap && depth(o) > depth(s))
        })
        .collect();
    line.sort_by(|a, b| side(a).total_cmp(&side(b)));
    line.into_iter().take(12).map(|s| s.id).collect()
}

/// The far line computed for the operation (empty when none was).
fn deep(state: &State, id: &str) -> Vec<i64> {
    (0..12)
        .map_while(|i| state.done.get(&key(id, &format!("deep:{i}"))).copied())
        .collect()
}

/// The operation's phase lines: none once the far line stands (it already lies behind them).
fn phase_lines<'a>(state: &State, id: &str, op: &'a Value) -> &'a [Value] {
    if deep(state, id).is_empty() {
        list(op, "phase_lines")
    } else {
        &[]
    }
}

/// The far line, computed once in peace from the enemies' states (hoi4_states: controller,
/// victory_points_total, x, z) and the centre of the group's own front states; kept in the
/// done-memory so it never moves. None when the operation asks for none or it cannot be read.
fn learn_deep(
    state: &mut State,
    tools: &dyn Tools,
    id: &str,
    op: &Value,
    enemies: &[String],
) -> Option<Vec<i64>> {
    if op["offensive"]["deep"] != json!(true) || !deep(state, id).is_empty() {
        return None;
    }
    let tried = key(id, "deep_tried");
    if state.done.contains_key(&tried) {
        return None;
    }
    let own: Vec<(f64, f64)> = ints(&op["front"]["own_states"])
        .iter()
        .filter_map(|s| state.centers.get(&(*s as i32)).copied())
        .collect();
    if own.is_empty() {
        return None;
    }
    let n = own.len() as f64;
    let origin = own
        .iter()
        .fold((0.0, 0.0), |a, p| (a.0 + p.0 / n, a.1 + p.1 / n));
    let mut rows = Vec::new();
    for tag in enemies {
        let Ok(read) = tools.call("hoi4_states", &json!({"query": tag})) else {
            continue;
        };
        for row in list(&read, "states") {
            if text(&row["controller"]) != tag {
                continue;
            }
            if let (Some(id), Some(x), Some(z)) = (
                row["state_id"].as_i64(),
                row["x"].as_f64(),
                row["z"].as_f64(),
            ) {
                let vp = row["victory_points_total"].as_f64().unwrap_or(0.0);
                rows.push(Far { id, vp, x, z });
            }
        }
    }
    if rows.is_empty() {
        // unread: tried again next time
        return None;
    }
    state.done.insert(tried, 0);
    let line = far_line(origin, &rows);
    if line.len() < 2 {
        return None;
    }
    for (i, s) in line.iter().enumerate() {
        state.done.insert(key(id, &format!("deep:{i}")), *s);
    }
    Some(line)
}

/// The war's progress, wherever it is: the enemies' victory points the player holds (each
/// victory-point province by its controller; a state's whole total by the state's controller when
/// its provinces are not listed), in tenths of a point. The capitulation counts points, so a war
/// that takes empty
/// provinces while every city holds is stalled (b38g, 19 Nov 2026: 109 of 134 points from day 42
/// to day 56 while the front crept east).
fn enemy_points(tools: &dyn Tools, enemies: &[String], me: &str) -> i64 {
    let mut points = 0.0;
    for tag in enemies {
        let Ok(read) = tools.call("hoi4_states", &json!({"query": tag})) else {
            continue;
        };
        for row in list(&read, "states") {
            if text(&row["owner"]) != tag {
                continue;
            }
            let vps = list(row, "victory_points");
            if vps.iter().any(|v| v["controller"].is_string()) {
                points += vps
                    .iter()
                    .filter(|v| text(&v["controller"]) == me)
                    .map(|v| v["victory_points"].as_f64().unwrap_or(0.0))
                    .sum::<f64>();
            } else if text(&row["controller"]) == me {
                points += row["victory_points_total"].as_f64().unwrap_or(0.0);
            }
        }
    }
    (points * 10.0).round() as i64
}

/// The operation's current line: 0 = the group offensive, k = phase line k, beyond the phase
/// lines one state chosen at the time (kept as `auto:<k>`).
fn current_line(state: &State, id: &str, op: &Value) -> (i64, Vec<i64>) {
    let k = state.done.get(&key(id, "line")).copied().unwrap_or(0);
    // a rolling contact line (see `contact_line`) stands for the line until a phase line is due
    let roll: Vec<i64> = (0..ROLL_STATES as i64)
        .map_while(|i| state.done.get(&key(id, &format!("roll:{i}"))).copied())
        .collect();
    if !roll.is_empty() {
        return (k, roll);
    }
    let phases = phase_lines(state, id, op);
    let far = deep(state, id);
    let line = if k == 0 && !far.is_empty() {
        far
    } else if k == 0 {
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

/// The enemy states the own land touches (hoi4_wars borders), the most victory points first
/// (hoi4_states `victory_points_total`), at most ROLL_STATES, none the player controls.
fn contact_line(tools: &dyn Tools, wars: &Value, enemies: &[String], me: &str) -> Vec<i64> {
    let touching: HashSet<i64> = list(wars, "borders")
        .iter()
        .filter(|b| enemies.iter().any(|e| e == text(&b["neighbour_tag"])))
        .flat_map(|b| ints(&b["neighbour_states"]))
        .collect();
    let mut rows: Vec<(f64, i64)> = Vec::new();
    for tag in enemies {
        let Ok(read) = tools.call("hoi4_states", &json!({"query": tag})) else {
            continue;
        };
        for row in list(&read, "states") {
            let Some(sid) = row["state_id"].as_i64() else {
                continue;
            };
            if touching.contains(&sid) && text(&row["controller"]) != me {
                rows.push((row["victory_points_total"].as_f64().unwrap_or(0.0), sid));
            }
        }
    }
    rows.sort_by(|a, b| b.0.total_cmp(&a.0).then(a.1.cmp(&b.1)));
    rows.into_iter().take(ROLL_STATES).map(|(_, s)| s).collect()
}

/// A stalled far line's next target: the enemy state the front touches with the most victory
/// points, with the touching states that hold points within DEEP_CLUSTER of it (one place, so
/// the one arrow drawn through them does not run across the country: b38g's [85, 93, 97], Danzig
/// to Volhynia, read back Diverged). Empty when no touching state holds a point.
fn vp_cluster(
    state: &State,
    tools: &dyn Tools,
    wars: &Value,
    enemies: &[String],
    me: &str,
) -> Vec<i64> {
    let touching: HashSet<i64> = list(wars, "borders")
        .iter()
        .filter(|b| enemies.iter().any(|e| e == text(&b["neighbour_tag"])))
        .flat_map(|b| ints(&b["neighbour_states"]))
        .collect();
    let mut rows: Vec<(f64, i64)> = Vec::new();
    for tag in enemies {
        let Ok(read) = tools.call("hoi4_states", &json!({"query": tag})) else {
            continue;
        };
        for row in list(&read, "states") {
            let Some(sid) = row["state_id"].as_i64() else {
                continue;
            };
            let vp = row["victory_points_total"].as_f64().unwrap_or(0.0);
            if touching.contains(&sid) && text(&row["controller"]) != me && vp > 0.0 {
                rows.push((vp, sid));
            }
        }
    }
    cluster(&rows, &|s| state.centers.get(&(s as i32)).copied())
}

/// The state with the most points and those of `rows` (points, state) within DEEP_CLUSTER of it,
/// most points first, at most ROLL_STATES.
fn cluster(rows: &[(f64, i64)], at: &dyn Fn(i64) -> Option<(f64, f64)>) -> Vec<i64> {
    let mut rows = rows.to_vec();
    rows.sort_by(|a, b| b.0.total_cmp(&a.0).then(a.1.cmp(&b.1)));
    let Some(&(_, top)) = rows.first() else {
        return Vec::new();
    };
    let centre = at(top);
    rows.into_iter()
        .filter(|(_, s)| {
            *s == top
                || match (centre, at(*s)) {
                    (Some(c), Some(p)) => {
                        ((c.0 - p.0).powi(2) + (c.1 - p.1).powi(2)).sqrt() <= DEEP_CLUSTER
                    }
                    _ => false,
                }
        })
        .take(ROLL_STATES)
        .map(|(_, s)| s)
        .collect()
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
        if let Some(line) = redeploy(state, tools, id, op, roster, wars, lease, now)? {
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
        let stall = op["stall_days"].as_i64().unwrap_or(STALL_DAYS) * DAY;
        let taken = !line.is_empty() && got.len() == line.len();
        state.op_notes.insert(
            id.to_string(),
            format!(
                "operation {id}: line {k} holds {}/{} states, day {}",
                got.len(),
                line.len(),
                (now - state.done.get(&key(id, "exec")).copied().unwrap_or(now)) / DAY
            ),
        );
        // the far line: the war's progress is the enemy's victory points taken anywhere (the line
        // lies behind the whole country and is reached last); flat points for `stall_days` while
        // the enemy still holds cities is the stall
        if !deep(state, id).is_empty() {
            let points = enemy_points(tools, enemies, &me);
            let pk = key(id, "enemy_points");
            if points > state.done.get(&pk).copied().unwrap_or(0) {
                state.done.insert(pk, points);
                state.done.insert(key(id, "moved"), now);
            }
        }
        let moved = state.done.get(&key(id, "moved")).copied().unwrap_or(now);
        let stalled = now - moved >= stall;
        let phases = phase_lines(state, id, op).to_vec();
        let due = phases.get(k as usize).is_some_and(|p| !p["when"].is_null()) && {
            let when = phases[k as usize]["when"].clone();
            holds(state, tools, &when, now)
        };
        // ROLLING CONTACT LINE: a line taken or stalled with no phase line due goes on to the
        // enemy states the front touches, the most victory points first (pb1/qb1, 19 Nov 2026:
        // the group's stalled country-wide line jumped to the far eastern border, which did not
        // draw, and the infantry took 3 states in 56 days)
        let roll_stall = ROLL_STALL_DAYS * DAY;
        let rolling = state.done.contains_key(&key(id, "roll:0"));
        let roll_stalled = rolling && now - moved >= roll_stall;
        if !due && (taken || stalled || roll_stalled) {
            let next = if deep(state, id).is_empty() {
                contact_line(tools, wars, enemies, &me)
            } else {
                vp_cluster(state, tools, wars, enemies, &me)
            };
            if !next.is_empty() && next != line {
                for i in 0..ROLL_STATES as i64 {
                    state.done.remove(&key(id, &format!("roll:{i}")));
                }
                for (i, s) in next.iter().enumerate() {
                    state.done.insert(key(id, &format!("roll:{i}")), *s);
                }
                state.done.insert(key(id, "held"), 0);
                state.done.insert(key(id, "moved"), now);
                let line = format!(
                    "operation {id}: line {} ({}/{} states): the contact line {:?}",
                    if taken { "taken" } else { "stalled" },
                    got.len(),
                    line.len(),
                    next
                );
                state.op_notes.insert(id.to_string(), line.clone());
                return Ok(Some(line));
            }
        }
        if due {
            for i in 0..ROLL_STATES as i64 {
                state.done.remove(&key(id, &format!("roll:{i}")));
            }
        }
        // a rolling line whose contact states have not changed keeps attacking them (the far
        // phase line is drawn only when it is due)
        if (taken || stalled || due || line.is_empty()) && (due || !rolling) {
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
        let mut wanted: Vec<i64> = if u.spear {
            spear_line(state, id, roster, u, &me, tools, &op_line, now)
        } else if u.role == Role::Group {
            match (u.side, split_axis(state, op)) {
                (Some(low), Some(split)) => side_line(state, &split, low, &op_line),
                _ => op_line.clone(),
            }
        } else if own_line_held {
            op_line.clone()
        } else {
            still(&u.line)
        };
        if u.role == Role::Wing && !u.spear && !own_line_held && op["wing_stall"] != json!(false) {
            if let Some(line) =
                wing_stall(state, tools, id, u, &me, roster, &mut wanted, lease, now)?
            {
                return Ok(Some(line));
            }
        }
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

/// A wing that stood WING_STALL_DAYS at war with no state of its wanted line taken and none of
/// its divisions in land it does not control gives the line's first state up (to the group's
/// plan, or to a pocket) and attacks the next one: its own orders are cleared once, then its
/// front is clicked at the new line and the offensive drawn and executed as at the start of a
/// war. `wanted` loses the states given up. Never a new front of another army.
#[allow(clippy::too_many_arguments)]
fn wing_stall(
    state: &mut State,
    tools: &dyn Tools,
    id: &str,
    u: &Unit,
    me: &str,
    roster: &Value,
    wanted: &mut Vec<i64>,
    lease: &str,
    now: GameHour,
) -> Result<Option<String>> {
    let gk = key(id, &format!("wingskip:{}", u.name));
    let given = state.done.get(&gk).copied().unwrap_or(0).max(0) as usize;
    let drop = given.min(wanted.len().saturating_sub(1));
    wanted.drain(..drop);
    let sk = key(id, &format!("wingsince:{}", u.name));
    let since = *state.done.entry(sk.clone()).or_insert(now);
    let stands: Vec<i64> = army_row(roster, &u.name)
        .and_then(|r| r["states"].as_object())
        .map(|m| m.keys().filter_map(|k| k.parse().ok()).collect())
        .unwrap_or_default();
    let across = !stands.is_empty() && held(tools, me, &stands).len() < stands.len();
    let gained = !held(tools, me, wanted).is_empty();
    if across || gained {
        state.done.insert(sk, now);
        return Ok(None);
    }
    if now - since < WING_STALL_DAYS * DAY || wanted.len() < 2 {
        return Ok(None);
    }
    let gone = wanted.remove(0);
    state.done.insert(gk, (given + 1) as i64);
    state.done.insert(sk, now);
    state.op_notes.insert(
        format!("{id}:{}", u.name),
        format!(
            "{}: stood {WING_STALL_DAYS} days with no state of its line taken: state {gone} left to the group, the next line {wanted:?}",
            u.name
        ),
    );
    let line = order(
        state,
        tools,
        lease,
        id,
        &format!(
            "{} stands before state {gone}: its orders cleared for the line {wanted:?}",
            u.name
        ),
        "hoi4_army_plan",
        json!({"army": u.name, "clear_orders": true}),
    )?;
    Ok(Some(line))
}

/// The line a spearhead is painted to at war: the states of its own thrust still to take, then
/// the nearest state of the operation's line; a state its spearhead has not taken in
/// STRIKE_MAX_DAYS is given up to the infantry's plan and the spearhead goes on to the next (qt1,
/// 19 Nov 2026: Panzergruppe Ost stood 35 days in Lodz on a Warsaw spearhead, behind the
/// Vistula and the city, while the states round it stood open).
#[allow(clippy::too_many_arguments)]
fn spear_line(
    state: &mut State,
    id: &str,
    roster: &Value,
    u: &Unit,
    me: &str,
    tools: &dyn Tools,
    op_line: &[i64],
    now: GameHour,
) -> Vec<i64> {
    let skipped = |state: &State, s: i64| {
        state
            .done
            .contains_key(&key(id, &format!("spearskip:{}:{s}", u.name)))
    };
    let have = held(tools, me, &u.line);
    let mut next: Vec<i64> = u
        .line
        .iter()
        .copied()
        .filter(|s| !have.contains(s) && !skipped(state, *s))
        .collect();
    if next.is_empty() {
        next = solo_line(state, roster, &u.name, op_line)
            .into_iter()
            .filter(|s| !skipped(state, *s))
            .take(1)
            .collect();
    }
    let Some(target) = next.last().copied() else {
        return next;
    };
    let fk = key(id, &format!("spearfor:{}", u.name));
    let sk = key(id, &format!("spearsince:{}", u.name));
    if state.done.get(&fk) != Some(&target) {
        state.done.insert(fk, target);
        state.done.insert(sk, now);
    } else if state
        .done
        .get(&sk)
        .is_some_and(|t| (now - t) as f64 >= STRIKE_MAX_DAYS * DAY as f64)
    {
        state
            .done
            .insert(key(id, &format!("spearskip:{}:{target}", u.name)), now);
        state.op_notes.insert(
            format!("{id}:{}", u.name),
            format!(
                "{}: state {target} not taken in {STRIKE_MAX_DAYS} days on its spearhead: left to the infantry's plan",
                u.name
            ),
        );
        return spear_line(state, id, roster, u, me, tools, op_line, now);
    }
    next
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
