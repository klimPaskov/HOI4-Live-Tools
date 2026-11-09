//! Military domains besides the war step (coverage audit wave 2: W2-1 army groups, W2-4 navy,
//! W2-5 air superiority, W2-9 casualty ratio, W2-11 naval invasions; section 4 rules 27, 28, 31),
//! one verified action at a time through the same tools as every other plan step. A child
//! module of `plan_run` (declared there with `#[path]`) like plan_manage.rs: one-shot work is
//! kept in `State::done` keyed by the item's `id` (else its list position) and the timers in
//! `State::bad`, both persisted in plan_state.json; every order goes through `State::act` and
//! its failure ledger (an order refused 3 times in 10 game days is not sent again, and
//! plan_manage's rule 35 wakes the player). All cadences are game time.
//!
//! Hooks in plan_run.rs: [`step`] runs in the tick right after plan_manage's step; [`hold_step`]
//! runs right after the events, before the layout and the war step (in peace too); the war
//! step's guard loop gives an army [`holding`] a corridor no guard order, only its pocket
//! fall-back; `run` adds [`take_alarms`] to its `alarms` and [`digest`] as `military`. A wake is
//! the alarm {"code":"wake_player","rule","reason","message","evidence"}, once per reason per
//! `manage.wake_repeat_days` (7).
//!
//! Plan keys:
//!   army_group   [{"id","name"?,"armies":[..],"marshal"?,"when"?}] (W2-1, audit G4-G5): once
//!                `when` holds and every entry of `armies` exists (an army_layout name_hint means
//!                the army playing that role, anything else an army name, hoi4_armies), the armies
//!                are grouped once (hoi4_army_group_create); then `marshal` (a general's name, or
//!                a list tried in order) leads the group once (hoi4_army_commander with the
//!                group's name; the tool promotes a general to field marshal itself, 40 command
//!                power: short of it the step waits 7 game days). Never churn: a group that
//!                already has a field marshal keeps him, `reassign` is never sent (a marshal who
//!                leads another army is refused and the next name is tried), and an army that
//!                already belongs to another group is never regrouped (the player is woken).
//!                `name` is the digest label (the game names the group). Done-memory
//!                `agrp:<id>` (grouped) and `agfm:<id>` (led); checked once a game day
//!   navy         {"posture":"none"|"coastal"|"blue_water","fleets":[{"fleet","state_id"?,
//!                "admiral"?,"missions":[{"id"?,"mission","regions":[..],"task_force"?,
//!                "state_id"?,"when"?}]}],"convoy_escort"?,"admirals"?,"repair_below"?,
//!                "repair_until"?} (W2-4, audit K2-K4, K10, rule 31), every 3 game days from
//!                hoi4_fleets, one action per tick. posture "none" switches the step off (the
//!                other postures are for people and the lint). In order:
//!                1. any own task force on a mission whose strength is below `repair_below`
//!                   (default 0.5; 50 means 50%) is sent to port with mission hold and left
//!                   there until it is back at `repair_until` (default 0.9);
//!                2. admirals once: `fleets[].admiral` or `admirals` ({"<fleet>":"<admiral>"} or
//!                   [{"fleet","admiral"}]) through hoi4_fleet_admiral, only for a fleet without
//!                   one, and never an admiral who leads another fleet (the tool would move him);
//!                3. per task force of a listed fleet, the planned mission is the first entry of
//!                   `missions` (task_force absent or this one) whose `when` holds; its `regions`
//!                   (sea region display names, or {"region","state_id"}) are added to the
//!                   fleet's area first (hoi4_fleet_region; the coastal `state_id` is the
//!                   region's, the entry's, the fleet entry's, else the fleet's home port state),
//!                   then the mission is given (hoi4_fleet_mission): an idle task force (hold)
//!                   always gets it, one on another mission once per entry (a mission the player
//!                   sets afterwards stands), reserve task forces never.
//!                `convoy_escort` {"fleet","task_force"?,"regions":[..],"when"?,"on"?} is a
//!                convoy_escort entry put before that fleet's own missions. Done-memory
//!                `nrep:<task force>`, `nadm:<fleet>`, `nreg:<fleet>|<region>`,
//!                `nmis:<task force>:<entry>`
//!   invasions    [{"id","from_state","target_state","target_province"?,"armies":[..],"fleet"?,
//!                "when"?,"with_harbor"?,"source_offset"?,"landing_offset"?,"stage_days"?,
//!                "prepare_days"?,"execute_days"?,"land_days"?}] (W2-11, audit H16): one invasion
//!                at a time, the first not finished whose `when` holds, through
//!                hoi4_naval_invasion (state_id = `target_state`, the state of `target_province`:
//!                `facts.py province <id>` names it; the tool lands on a state). Its armies are
//!                driven one after another (the game's base cap is one invasion plan; an army over
//!                the division cap, base 4, is the game's refusal): `fleet` first gets
//!                invasion_support on every sound task force not already on patrol or strike
//!                force (once per invasion), the army moves to `from_state` (hoi4_army_move once),
//!                the invasion is ordered (source_state_id `from_state`), and after `prepare_days`
//!                (default 14) its plan is executed (hoi4_army_plan execute:true, every 5 game days
//!                while the game answers plan_not_executable: preparation or naval dominance).
//!                Landed when `target_province` (else any province of `target_state`) is held by
//!                the player after the first army executed. Aborted, with a wake, on a typed
//!                refusal (not_coastal, source_not_coastal, own_state, no_such_army,
//!                invalid_target, ...), an order the ledger refuses, an army lost, and after
//!                `stage_days` (default 20) waiting for an army or transports, `execute_days`
//!                (default 60) not executable, or `land_days` (default 30) without a landing.
//!                The war step leaves the armies of the running invasion alone (its cooldown is
//!                held; list them in `war.reserve` too), the navy step its fleet, and the rests
//!                below its armies. Done-memory `inv:<id>:start`, `:move:<army>`,
//!                `:ordered:<army>`, `:exec:<army>`, `:tf:<task force>`, `:fleet`, `:landed`,
//!                `:aborted`
//!   hold_targets [{"id","army","provinces":[province ids along the corridor, rear first],"when"?,
//!                "until"?,"state"?}] (lesson L01: Somaliland, Jubaland, Afar and Wello were lost
//!                behind the strike armies): `army` (an army name, or an army_layout name_hint:
//!                the army playing that role) holds the supply corridor behind the advance. The
//!                hold starts once `when` holds and ends for good once `until` holds; an entry
//!                without `army` is the player's (never acted on). While it runs the army's post
//!                is the most forward listed province the player controls, unless a listed
//!                province behind it is lost: then the rearmost such province (retake the corridor
//!                first); with none held the player is woken and the army waits. Judged once a
//!                game day and at once when a listed province changes controller (its state's
//!                hoi4_states read, every tick through the tick's read cache): when the army is not
//!                known to be heading for or standing in its post, one hoi4_army_move {army,
//!                state_id (the province's state), province_id}; a receipt whose pre-check says
//!                "already heading or standing there: nothing was sent" is trusted and the move is
//!                not sent again until the post changes (a move that was sent is checked once more
//!                a day later through that pre-check). An executing battle plan is stopped first
//!                (hoi4_army_plan execute:false). Never for an army with encirclement_risk
//!                (hoi4_armies threat) or during the war step's pocket fall-back from one (the
//!                pocket timer): the fall-back owns it then, and the post is ordered again once it
//!                is over. Never for an army the player ordered (war.player_claim_seconds) or one
//!                in the running naval invasion. The order goes through the failure ledger: 3
//!                failures in 10 game days and it is not sent again, the player is woken. A
//!                province's state is learned from the own and the enemies' hoi4_states reads
//!                (victory points, provinces held by others), then by reading candidate states
//!                (`state` first, then the nearest to the corridor, at most 8 a tick); a province
//!                found in none wakes the player. Binding: while the hold runs the army is bound
//!                in `war_guard` to its post's state (its guard post before the hold is kept, and
//!                is the army_layout guard_states_in_order[0] after a restart); the war step
//!                neither drives it, nor draws it to retake a state, nor sends its guard order.
//!                When the hold ends (`until`, the entry removed, its army changed) the army is
//!                bound to its old guard post again with its guard stage reset, so the war step
//!                sends it back there and draws its front; an army that was no guard goes back
//!                to the war step. One army, one hold (the first entry in list order). Done-memory
//!                `hold:<id>:on`, `hold:<id>:released`; the post, the last order and the province
//!                index are this server life's
//!   manage      the standing rules here run only when the plan has `manage` (like plan_manage's):
//!                "casualty_ratio_max":2.0 own / enemy losses (hoi4_war_stats, per enemy over the
//!                war relations with the player) over "casualty_window_days":14 above which, with
//!                at least "casualty_min_losses":1000 own losses, every attacking army on that
//!                enemy's front (plan executing, or driven by the war step; guards and invasion
//!                armies never) rests "casualty_rest_days":7 (W2-9, rule 28): its plan is stopped
//!                (hoi4_army_plan execute:false), the war step leaves it alone, and when the rest
//!                is over a plan this rule stopped is executed again; the player is woken with
//!                the numbers. One enemy: every attacking army is on its front; several: the
//!                controller of the army's war target state decides, and an army without one is
//!                unknown and never rested. An army is judged again only a full window after it
//!                resumed. Sampled once a game day.
//!                "enemy_air_superiority":0.6 (W2-5, rule 27): every 3 game days, a front region
//!                (where own CAS wings fly, plus "air_front_regions":[ids or names]) whose enemy
//!                air superiority is above it gets one more fighter wing per action
//!                (hoi4_air_mission air_superiority; standing-by wings first, never a wing the
//!                air_plan names or one over another such region, a moved wing stays 14 days),
//!                and when that is the only front region its attacking armies hold
//!                "air_hold_days":10 (resumed earlier once no front region is above the limit).
//!                The superiority is read from hoi4_air_regions `regions[].enemy_air_superiority`;
//!                no tool maps it yet (hoi4_air lists air_superiority_by_region as not mapped), so
//!                today the rule reports unknown and acts on nothing.
//!                "off":["casualties","air_superiority"] switches these rules off
//!   air_ops      not a key: air_plan (plan_manage.rs) sends its `mission` to hoi4_air_mission as
//!                written, so strategic_bombing, naval_strike, port_strike, logistics_strike,
//!                interception, air_supply, naval_mines, recon, ... are air_plan entries already.
//!
//! Rests (both rules): `mrest:<cas|air>:<army>` (the start) and `mstopped:<army>` (the plan was
//! stopped by the rule) in the done-memory, the end in the skip timer `mrest`, `mresumed:<army>`
//! when it ended; the war step's cooldown for the army is held to the end of the rest (again
//! after a server restart).
//!
//! `when` goes through `when_holds`: a `crate::plan_predicate` predicate judged with the tick's
//! reads; an atom that cannot be read is unknown and the item waits.
use super::{item_id, list, strings, text, GameHour, State, Tools, DAY};
use crate::error::Result;
use crate::plan_predicate::{Cache, Ctx, Truth};
use serde_json::{json, Value};
use std::cell::RefCell;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::time::Duration;

/// Losses per enemy tag: (own, enemy), in men.
type Losses = BTreeMap<String, (f64, f64)>;

/// What the runner keeps for these rules besides `done` and the skip timers.
#[derive(Default)]
pub(super) struct Memory {
    /// Wake alarms raised since the last run result.
    alarms: Vec<Value>,
    /// The last status line of each rule (the run result's `military`).
    notes: BTreeMap<String, String>,
    /// (game hour, enemy tag -> (own losses, enemy losses)) once a game day, for rule 28.
    losses: Vec<(GameHour, Losses)>,
    /// Armies whose war cooldown this server life holds for a rest (after a restart the
    /// persisted rest sets it again once).
    cooled: HashSet<String>,
    /// hold_targets: army -> (the hold's id, the army's `war_guard` post before the hold).
    held: HashMap<String, (String, Option<i32>)>,
    /// hold_targets: hold id -> the last move it sent.
    hold_orders: HashMap<String, HoldOrder>,
    /// hold_targets: hold id -> the controllers of its provinces last seen.
    hold_seen: HashMap<String, String>,
    /// Province -> state, learned from hoi4_states reads (map facts: they never change).
    province_state: HashMap<i64, i64>,
    /// States whose every province was read into `province_state`.
    scanned: HashSet<i64>,
}

/// The last move a hold sent.
struct HoldOrder {
    army: String,
    post: i64,
    at: GameHour,
    /// The receipt's pre-check said the army was already heading for or standing in the post.
    under_way: bool,
}

/// An army was renamed (army_layout gives it its role's name): its rests and holds follow it.
pub(super) fn army_renamed(memory: &mut Memory, old: &str, new: &str) {
    if memory.cooled.remove(old) {
        memory.cooled.insert(new.to_string());
    }
    if let Some(v) = memory.held.remove(old) {
        memory.held.insert(new.to_string(), v);
    }
    for order in memory.hold_orders.values_mut() {
        if order.army == old {
            order.army = new.to_string();
        }
    }
}

/// The wake alarms raised since the last call (the run result's `alarms`).
pub(super) fn take_alarms(state: &mut State) -> Vec<Value> {
    std::mem::take(&mut state.military.alarms)
}

/// The last status of each rule (the run result's `military`), or None when nothing ran.
pub(super) fn digest(state: &State) -> Option<Value> {
    (!state.military.notes.is_empty()).then(|| json!(state.military.notes))
}

/// Everything after plan_manage's step, at most one action: rests that are over, the running
/// naval invasion, the casualty and air superiority rules, army groups, the navy.
pub(super) fn step(
    state: &mut State,
    tools: &dyn Tools,
    plan: &Value,
    lease: &str,
    now: GameHour,
) -> Result<Option<String>> {
    if let Some(done) = rest_step(state, tools, lease, now)? {
        return Ok(Some(done));
    }
    if let Some(done) = invasion_step(state, tools, plan, lease, now)? {
        return Ok(Some(done));
    }
    if let Some(done) = casualty_rule(state, tools, plan, lease, now)? {
        return Ok(Some(done));
    }
    if let Some(done) = air_superiority_rule(state, tools, plan, lease, now)? {
        return Ok(Some(done));
    }
    if let Some(done) = army_group_step(state, tools, plan, lease, now)? {
        return Ok(Some(done));
    }
    navy_step(state, tools, plan, lease, now)
}

// ---------------------------------------------------------------------------------------------
// Shared helpers.

/// Whether a plan item's `when` holds now: missing or empty holds; anything else is a
/// `crate::plan_predicate` predicate (an unknown atom is Unknown: the item waits).
fn when_holds(state: &mut State, tools: &dyn Tools, when: &Value, now: GameHour) -> Truth {
    match when {
        Value::Null => Truth::True,
        Value::String(s) if s.trim().is_empty() => Truth::True,
        _ => super::judge(state, tools, when, now),
    }
}

fn note(state: &mut State, rule: &str, line: impl Into<String>) {
    state.military.notes.insert(rule.to_string(), line.into());
}

fn off(plan: &Value, rule: &str) -> bool {
    strings(&plan["manage"], "off").iter().any(|r| r == rule)
}

/// A fraction: 0.5 or 50 (percent) both mean one half.
fn as_fraction(v: &Value) -> Option<f64> {
    v.as_f64().map(|f| if f > 1.0 { f / 100.0 } else { f })
}

fn fraction(v: &Value, default: f64) -> f64 {
    as_fraction(v).unwrap_or(default)
}

/// A positive whole number of game days, else `default`.
fn days(v: &Value, default: i64) -> i64 {
    v.as_i64().filter(|d| *d > 0).unwrap_or(default)
}

/// The error code of a refused order (`State::act` reports "code: message").
fn code(why: &str) -> &str {
    why.split(':').next().unwrap_or("").trim()
}

/// Raise a `wake_player` alarm once per `reason` per `manage.wake_repeat_days` (7).
fn wake(
    state: &mut State,
    plan: &Value,
    now: GameHour,
    rule: Value,
    reason: &str,
    message: String,
    evidence: Value,
) {
    if state.skipped(now, "wake", reason) {
        return;
    }
    let days = days(&plan["manage"]["wake_repeat_days"], 7);
    state.skip(now, "wake", reason, days * DAY);
    state
        .military
        .alarms
        .retain(|a| a["reason"] != json!(reason));
    state.military.alarms.push(
        json!({"code": "wake_player", "rule": rule, "reason": reason,
        "message": message, "evidence": evidence}),
    );
}

/// An army entry of a plan item: an army_layout name_hint means the army playing that role,
/// anything else is an army name.
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

/// The strength of a task force (hoi4_fleets) as a fraction, None when unreadable.
fn strength(tf: &Value) -> Option<f64> {
    tf["strength_percent"]
        .as_f64()
        .map(|p| p / 100.0)
        .or_else(|| {
            let max = tf["max_strength"].as_f64().filter(|m| *m > 0.0)?;
            Some(tf["strength"].as_f64()? / max)
        })
}

fn same_name(a: &str, b: &str) -> bool {
    !a.trim().is_empty() && a.trim().eq_ignore_ascii_case(b.trim())
}

// ---------------------------------------------------------------------------------------------
// Rests (rules 27 and 28): stop an army's plan, hold the war step off it, resume at the end.

/// Whether the army rests for either rule.
fn resting(state: &State, army: &str) -> bool {
    state.done.keys().any(|k| {
        k.strip_prefix("mrest:")
            .and_then(|r| r.split_once(':'))
            .is_some_and(|(_, a)| a == army)
    })
}

/// Whether the army's last rest ended less than `span` game hours ago.
fn resumed_within(state: &State, army: &str, now: GameHour, span: GameHour) -> bool {
    state
        .done
        .get(&format!("mresumed:{army}"))
        .is_some_and(|at| now - *at < span)
}

/// Start a rest of `rest_days`: an executing plan is stopped first (the only input). Returns
/// the action's line when an order was sent.
fn start_rest(
    state: &mut State,
    tools: &dyn Tools,
    lease: &str,
    row: &Value,
    why: &str,
    rest_days: i64,
    now: GameHour,
) -> Result<Option<String>> {
    let army = text(&row["army"]).to_string();
    if army.is_empty() || state.skipped(now, "mrestfail", &army) {
        return Ok(None);
    }
    let mut line = None;
    if row["plan_executing"] == json!(true) {
        let (ok, msg) = state.act(
            tools,
            lease,
            "hoi4_army_plan",
            json!({"army": army, "execute": false}),
            "mr",
        )?;
        if !ok {
            state.skip(now, "mrestfail", &army, 2 * DAY);
            return Ok(Some(format!("rest {army}: stop plan: {ok} {msg}")));
        }
        state.done.insert(format!("mstopped:{army}"), now);
        line = Some(format!(
            "{army} rests {rest_days} days, plan stopped: {ok} {msg}"
        ));
    }
    state.done.insert(format!("mrest:{why}:{army}"), now);
    state.skip(now, "mrest", &army, rest_days * DAY);
    state.war_cool.insert(army.clone(), now + rest_days * DAY);
    state.military.cooled.insert(army);
    Ok(line)
}

/// Rests that are over end here: the war step may drive the army again, and a plan the rule
/// stopped is executed again.
fn rest_step(
    state: &mut State,
    tools: &dyn Tools,
    lease: &str,
    now: GameHour,
) -> Result<Option<String>> {
    let rests: Vec<(String, String, String)> = state
        .done
        .keys()
        .filter_map(|k| {
            let (why, army) = k.strip_prefix("mrest:")?.split_once(':')?;
            Some((k.clone(), why.to_string(), army.to_string()))
        })
        .collect();
    if rests.is_empty() {
        return Ok(None);
    }
    let mut roster: Option<Value> = None;
    for (key, why, army) in rests {
        let until = state
            .bad
            .get(&format!("mrest:{army}"))
            .copied()
            .filter(|u| *u > now);
        if let Some(until) = until {
            if state.military.cooled.insert(army.clone()) {
                state.war_cool.insert(army, until);
            }
            continue;
        }
        state.done.remove(&key);
        state.military.cooled.remove(&army);
        state.war_cool.remove(&army);
        state.done.insert(format!("mresumed:{army}"), now);
        if state.done.remove(&format!("mstopped:{army}")).is_none() {
            note(state, "rest", format!("{army}: rest ({why}) over"));
            continue;
        }
        if roster.is_none() {
            roster = tools.call("hoi4_armies", &json!({"threat": false})).ok();
        }
        let row = roster.as_ref().and_then(|r| army_row(r, &army)).cloned();
        let Some(row) = row else {
            note(
                state,
                "rest",
                format!("{army}: rest ({why}) over, army not found"),
            );
            continue;
        };
        if row["plan_executing"] == json!(true)
            || row["battle_plan_orders"].as_u64().unwrap_or(0) == 0
        {
            note(
                state,
                "rest",
                format!("{army}: rest ({why}) over, no plan to execute"),
            );
            continue;
        }
        let (ok, msg) = state.act(
            tools,
            lease,
            "hoi4_army_plan",
            json!({"army": army, "execute": true}),
            "mr",
        )?;
        note(state, "rest", format!("{army}: rest ({why}) over: {ok}"));
        return Ok(Some(format!(
            "rest over ({why}): {army} executes its plan again: {ok} {msg}"
        )));
    }
    Ok(None)
}

/// Armies attacking now: plan executing, or driven by the war step; never guards or the
/// `claimed` ones, never resting ones or ones resumed less than `span` ago.
fn attacking<'a>(
    state: &State,
    roster: &'a Value,
    claimed: &HashSet<String>,
    now: GameHour,
    span: GameHour,
) -> Vec<&'a Value> {
    list(roster, "armies")
        .iter()
        .filter(|a| {
            let n = text(&a["army"]);
            !n.is_empty()
                && !claimed.contains(n)
                && !state.war_guard.contains_key(n)
                && !resting(state, n)
                && !resumed_within(state, n, now, span)
                && (a["plan_executing"] == json!(true) || state.war_assign.contains_key(n))
        })
        .collect()
}

// ---------------------------------------------------------------------------------------------
// W2-11: naval invasions.

fn invasion_key(entry: &Value, index: usize) -> String {
    item_id(entry).unwrap_or_else(|| format!("#{index}"))
}

fn invasion_ended(state: &State, id: &str) -> bool {
    state.done.contains_key(&format!("inv:{id}:landed"))
        || state.done.contains_key(&format!("inv:{id}:aborted"))
}

/// The running invasion: the first started and not finished.
fn active_invasion(state: &State, plan: &Value) -> Option<(String, Value)> {
    list(plan, "invasions")
        .iter()
        .enumerate()
        .find_map(|(i, e)| {
            let id = invasion_key(e, i);
            (state.done.contains_key(&format!("inv:{id}:start")) && !invasion_ended(state, &id))
                .then(|| (id, e.clone()))
        })
}

/// The armies and the fleet of the running invasion (other steps leave them alone).
fn invasion_claims(state: &State, plan: &Value) -> (HashSet<String>, Option<String>) {
    let Some((_, entry)) = active_invasion(state, plan) else {
        return (HashSet::new(), None);
    };
    let armies = strings(&entry, "armies")
        .iter()
        .map(|h| resolve(state, h))
        .collect();
    let fleet = entry["fleet"]
        .as_str()
        .filter(|f| !f.trim().is_empty())
        .map(String::from);
    (armies, fleet)
}

/// Whether the landing holds: `province` (else any province of the state) is held by `me`.
fn landed(tools: &dyn Tools, target: i64, province: Option<i64>, me: &str) -> Truth {
    if me.is_empty() {
        return Truth::Unknown;
    }
    let Ok(read) = tools.call("hoi4_states", &json!({"query": target.to_string()})) else {
        return Truth::Unknown;
    };
    let rows = list(&read, "states");
    let Some(row) = rows
        .iter()
        .find(|r| r["state_id"].as_i64() == Some(target))
        .or(rows.first())
    else {
        return Truth::Unknown;
    };
    if text(&row["controller"]) == me {
        return Truth::True;
    }
    let provinces = list(row, "provinces");
    match province {
        Some(p) => provinces
            .iter()
            .find(|r| r["province"].as_i64() == Some(p))
            .map_or(Truth::Unknown, |r| {
                Truth::from_bool(text(&r["controller"]) == me)
            }),
        None if provinces.is_empty() => Truth::Unknown,
        None => Truth::from_bool(provinces.iter().any(|r| text(&r["controller"]) == me)),
    }
}

/// Abort the invasion: no more input for it, the player decides.
fn abort_invasion(
    state: &mut State,
    plan: &Value,
    now: GameHour,
    id: &str,
    armies: &[String],
    why: String,
    evidence: Value,
) {
    state.done.insert(format!("inv:{id}:aborted"), now);
    for army in armies {
        state.war_cool.remove(army);
    }
    note(state, &format!("invasion:{id}"), format!("aborted: {why}"));
    wake(
        state,
        plan,
        now,
        json!("H16"),
        &format!("invasion:{id}"),
        format!("naval invasion {id} aborted: {why}"),
        evidence,
    );
}

/// `fleet` of an invasion: every sound task force not on patrol or strike force gets
/// invasion_support, once per invasion.
fn invasion_fleet(
    state: &mut State,
    tools: &dyn Tools,
    plan: &Value,
    lease: &str,
    id: &str,
    entry: &Value,
    now: GameHour,
) -> Result<Option<String>> {
    let Some(fleet) = entry["fleet"].as_str().filter(|f| !f.trim().is_empty()) else {
        return Ok(None);
    };
    let key = format!("inv:{id}:fleet");
    if state.done.contains_key(&key) || state.skipped(now, "invfleet", id) {
        return Ok(None);
    }
    let Ok(read) = tools.call("hoi4_fleets", &json!({})) else {
        state.skip(now, "invfleet", id, DAY);
        return Ok(None);
    };
    let Some(row) = list(&read, "fleets")
        .iter()
        .find(|f| same_name(fleet, text(&f["fleet"])))
    else {
        note(
            state,
            &format!("invasion:{id}"),
            format!("fleet {fleet} not found (hoi4_fleets): no invasion support"),
        );
        state.skip(now, "invfleet", id, 7 * DAY);
        return Ok(None);
    };
    let repair_below = fraction(&plan["navy"]["repair_below"], 0.5);
    for tf in list(row, "task_forces") {
        let name = text(&tf["task_force"]).to_string();
        let mission = tf["mission"]["name"].as_str().unwrap_or("");
        if name.is_empty()
            || matches!(
                mission,
                "invasion_support" | "strike_force" | "patrol" | "reserve"
            )
            || strength(tf).is_some_and(|s| s < repair_below)
            || state.skipped(now, "navtf", &name)
            || state.done.contains_key(&format!("inv:{id}:tf:{name}"))
        {
            continue;
        }
        let (ok, why) = state.act(
            tools,
            lease,
            "hoi4_fleet_mission",
            json!({"task_force": name, "mission": "invasion_support"}),
            "ni",
        )?;
        if ok || code(&why) == "nothing_to_change" {
            state.done.insert(format!("inv:{id}:tf:{name}"), now);
        } else {
            state.skip(now, "navtf", &name, 3 * DAY);
            if code(&why) == "no_assigned_region" {
                wake(
                    state,
                    plan,
                    now,
                    json!("H16"),
                    &format!("invasion_fleet:{id}"),
                    format!("invasion {id}: fleet {fleet} has no sea region; add the route's regions (navy.fleets[].missions[].regions)"),
                    json!({"fleet": fleet, "task_force": name, "refusal": why}),
                );
            }
        }
        return Ok(Some(format!(
            "invasion {id}: {name} invasion_support: {ok} {why}"
        )));
    }
    state.done.insert(key, now);
    Ok(None)
}

fn invasion_step(
    state: &mut State,
    tools: &dyn Tools,
    plan: &Value,
    lease: &str,
    now: GameHour,
) -> Result<Option<String>> {
    let entries = list(plan, "invasions");
    if entries.is_empty() {
        return Ok(None);
    }
    let (id, entry) = match active_invasion(state, plan) {
        Some(active) => active,
        None => {
            let mut start = None;
            for (i, e) in entries.iter().enumerate() {
                let id = invasion_key(e, i);
                if invasion_ended(state, &id) {
                    continue;
                }
                match when_holds(state, tools, &e["when"], now) {
                    Truth::True => {
                        start = Some((id, e.clone()));
                        break;
                    }
                    t => note(
                        state,
                        &format!("invasion:{id}"),
                        format!("waits: when {}", t.label()),
                    ),
                }
            }
            let Some((id, e)) = start else {
                return Ok(None);
            };
            state.done.insert(format!("inv:{id}:start"), now);
            (id, e)
        }
    };
    let armies: Vec<String> = strings(&entry, "armies")
        .iter()
        .map(|h| resolve(state, h))
        .collect();
    // the war step leaves the invasion's armies alone while it runs
    for army in &armies {
        let until = state.war_cool.get(army).copied().unwrap_or(0);
        state
            .war_cool
            .insert(army.clone(), until.max(now + 2 * DAY));
    }
    if state.skipped(now, "inv", &id) {
        return Ok(None);
    }
    let label = format!("invasion:{id}");
    let (Some(from), Some(target)) = (entry["from_state"].as_i64(), entry["target_state"].as_i64())
    else {
        abort_invasion(
            state,
            plan,
            now,
            &id,
            &armies,
            "from_state and target_state (the state of target_province) are needed".into(),
            json!({"entry": entry}),
        );
        return Ok(None);
    };
    if armies.is_empty() {
        abort_invasion(
            state,
            plan,
            now,
            &id,
            &armies,
            "no armies".into(),
            json!({"entry": entry}),
        );
        return Ok(None);
    }
    let province = entry["target_province"].as_i64();
    let started = state
        .done
        .get(&format!("inv:{id}:start"))
        .copied()
        .unwrap_or(now);
    let stage_days = days(&entry["stage_days"], 20) * DAY;
    let prepare = days(&entry["prepare_days"], 14) * DAY;
    let execute_days = days(&entry["execute_days"], 60) * DAY;
    let land_days = days(&entry["land_days"], 30) * DAY;
    let me = tools
        .call("hoi4_overview", &json!({}))
        .map(|o| text(&o["player"]).to_string())
        .unwrap_or_default();
    let Ok(roster) = tools.call("hoi4_armies", &json!({"threat": false})) else {
        state.skip(now, "inv", &id, DAY);
        note(state, &label, "hoi4_armies unreadable");
        return Ok(None);
    };
    let exec_at =
        |state: &State, army: &str| state.done.get(&format!("inv:{id}:exec:{army}")).copied();
    let landing = landed(tools, target, province, &me);
    let any_exec = armies.iter().any(|a| exec_at(state, a).is_some());
    if landing == Truth::True {
        state.done.insert(format!("inv:{id}:landed"), now);
        for army in &armies {
            state.war_cool.remove(army);
        }
        let how = if any_exec {
            "landed"
        } else {
            "target already held, nothing ordered"
        };
        note(state, &label, format!("{how} in state {target}"));
        return Ok(None);
    }
    let mut turn = started;
    for army in &armies {
        if let Some(at) = exec_at(state, army) {
            if army_row(&roster, army).is_none() {
                abort_invasion(
                    state,
                    plan,
                    now,
                    &id,
                    &armies,
                    format!("{army} was lost before the landing"),
                    json!({"army": army, "target_state": target}),
                );
                return Ok(None);
            }
            turn = at;
            continue;
        }
        let ordered = state.done.get(&format!("inv:{id}:ordered:{army}")).copied();
        let row = army_row(&roster, army).cloned();
        if let Some(at) = ordered {
            // ordered: wait for the preparation, then execute
            let Some(row) = row else {
                abort_invasion(
                    state,
                    plan,
                    now,
                    &id,
                    &armies,
                    format!("{army} was lost after the order"),
                    json!({"army": army}),
                );
                return Ok(None);
            };
            if row["plan_executing"] == json!(true) {
                state.done.insert(format!("inv:{id}:exec:{army}"), now);
                note(state, &label, format!("{army} executing"));
                return Ok(None);
            }
            if now - at < prepare {
                note(
                    state,
                    &label,
                    format!(
                        "{army} prepares ({} of {} days)",
                        (now - at) / DAY,
                        prepare / DAY
                    ),
                );
                state.skip(now, "inv", &id, (at + prepare - now).min(DAY));
                return Ok(None);
            }
            if now - at > prepare + execute_days {
                abort_invasion(
                    state,
                    plan,
                    now,
                    &id,
                    &armies,
                    format!(
                        "{army}'s invasion plan could not be executed for {} days (preparation or naval dominance on the route)",
                        (now - at - prepare) / DAY
                    ),
                    json!({"army": army, "ordered_at_hour": at, "fleet": entry["fleet"]}),
                );
                return Ok(None);
            }
            if state.skipped(now, "invexec", army) {
                return Ok(None);
            }
            let (ok, why) = state.act(
                tools,
                lease,
                "hoi4_army_plan",
                json!({"army": army, "execute": true}),
                "ie",
            )?;
            if ok {
                state.done.insert(format!("inv:{id}:exec:{army}"), now);
            } else {
                // 5 game days apart: never 3 refusals in the ledger's 10 days
                state.skip(now, "invexec", army, 5 * DAY);
            }
            note(state, &label, format!("{army} execute: {ok} {why}"));
            return Ok(Some(format!("invasion {id}: {army} executes: {ok} {why}")));
        }
        // this army's turn: transports, staging, the order
        let Some(row) = row else {
            if now - turn > stage_days {
                abort_invasion(
                    state,
                    plan,
                    now,
                    &id,
                    &armies,
                    format!("{army} did not appear in {} days", stage_days / DAY),
                    json!({"army": army}),
                );
            } else {
                note(state, &label, format!("waiting for {army}"));
                state.skip(now, "inv", &id, DAY);
            }
            return Ok(None);
        };
        if let Some(line) = invasion_fleet(state, tools, plan, lease, &id, &entry, now)? {
            return Ok(Some(line));
        }
        let here = row["main_state"].as_i64();
        if here != Some(from) {
            if now - turn > stage_days {
                abort_invasion(
                    state,
                    plan,
                    now,
                    &id,
                    &armies,
                    format!(
                        "{army} did not reach state {from} in {} days",
                        stage_days / DAY
                    ),
                    json!({"army": army, "main_state": here, "from_state": from}),
                );
                return Ok(None);
            }
            let moved = format!("inv:{id}:move:{army}");
            if here.is_some() && !state.done.contains_key(&moved) {
                let (ok, why) = state.act(
                    tools,
                    lease,
                    "hoi4_army_move",
                    json!({"army": army, "state_id": from}),
                    "im",
                )?;
                if ok {
                    state.done.insert(moved, now);
                } else {
                    state.skip(now, "inv", &id, DAY);
                }
                return Ok(Some(format!(
                    "invasion {id}: {army} to the port state {from}: {ok} {why}"
                )));
            }
            note(state, &label, format!("{army} on its way to state {from}"));
            state.skip(now, "inv", &id, DAY / 2);
            return Ok(None);
        }
        let mut args = json!({"state_id": target, "army": army, "source_state_id": from});
        for key in ["with_harbor", "source_offset", "landing_offset"] {
            if !entry[key].is_null() {
                args[key] = entry[key].clone();
            }
        }
        let (ok, why) = state.act(tools, lease, "hoi4_naval_invasion", args, "in")?;
        if ok {
            state.done.insert(format!("inv:{id}:ordered:{army}"), now);
            note(state, &label, format!("{army} ordered"));
        } else {
            let transient = matches!(
                code(&why),
                "no_transports"
                    | "enemy_required"
                    | "army_location_unknown"
                    | "selection_mismatch"
                    | "button_disabled"
                    | "input_readback_failed"
                    | "gui_target_unavailable"
                    | "gui_geometry_unavailable"
            );
            if transient && now - turn <= stage_days {
                state.skip(now, "inv", &id, 2 * DAY);
                note(
                    state,
                    &label,
                    format!("{army} order refused, retried: {why}"),
                );
            } else {
                abort_invasion(
                    state,
                    plan,
                    now,
                    &id,
                    &armies,
                    format!("the order for {army} was refused: {why}"),
                    json!({"army": army, "target_state": target, "from_state": from, "refusal": why}),
                );
            }
        }
        return Ok(Some(format!(
            "invasion {id}: {army} from {from} to state {target}: {ok} {why}"
        )));
    }
    // every army executes: wait for the landing
    if now - turn > land_days {
        abort_invasion(
            state,
            plan,
            now,
            &id,
            &armies,
            format!(
                "no landing in state {target} {} days after the last execute",
                land_days / DAY
            ),
            json!({"target_state": target, "target_province": province, "landing": landing.label()}),
        );
        return Ok(None);
    }
    note(
        state,
        &label,
        format!("executing, landing {}", landing.label()),
    );
    state.skip(now, "inv", &id, DAY / 2);
    Ok(None)
}

// ---------------------------------------------------------------------------------------------
// W2-9, rule 28: casualty ratio per enemy front.

/// Own and enemy losses per enemy tag from hoi4_war_stats (the relations with the player);
/// None when the read has no player.
fn losses_by_enemy(stats: &Value) -> Option<Losses> {
    let me = stats["player"]["tag"].as_str()?;
    let mut out: Losses = BTreeMap::new();
    for war in list(stats, "wars")
        .iter()
        .filter(|w| w["includes_player"] == json!(true))
    {
        for rel in list(war, "relations") {
            if rel["attributed"] == json!(false) {
                continue;
            }
            let countries = list(rel, "countries");
            let [a, b] = countries else { continue };
            let (own, other) = if text(&a["tag"]) == me {
                (a, b)
            } else if text(&b["tag"]) == me {
                (b, a)
            } else {
                continue;
            };
            let (Some(o), Some(e)) = (own["casualties"].as_f64(), other["casualties"].as_f64())
            else {
                continue;
            };
            let entry = out.entry(text(&other["tag"]).to_string()).or_default();
            entry.0 += o;
            entry.1 += e;
        }
    }
    Some(out)
}

fn casualty_rule(
    state: &mut State,
    tools: &dyn Tools,
    plan: &Value,
    lease: &str,
    now: GameHour,
) -> Result<Option<String>> {
    let m = &plan["manage"];
    if !m.is_object() || off(plan, "casualties") || state.skipped(now, "mcas", "x") {
        return Ok(None);
    }
    let max = m["casualty_ratio_max"]
        .as_f64()
        .filter(|v| *v > 0.0)
        .unwrap_or(2.0);
    let window = days(&m["casualty_window_days"], 14) * DAY;
    let min_losses = m["casualty_min_losses"].as_f64().unwrap_or(1000.0);
    let rest_days = days(&m["casualty_rest_days"], 7);
    let sample = tools
        .call("hoi4_war_stats", &json!({}))
        .ok()
        .and_then(|s| losses_by_enemy(&s))
        .filter(|s| !s.is_empty());
    let Some(sample) = sample else {
        state.military.losses.clear();
        state.skip(now, "mcas", "x", DAY);
        note(
            state,
            "casualty_ratio",
            "not at war, or hoi4_war_stats unreadable: nothing to judge",
        );
        return Ok(None);
    };
    let losses = &mut state.military.losses;
    if losses.last().is_none_or(|(h, _)| now - *h >= DAY) {
        losses.push((now, sample.clone()));
    }
    losses.retain(|(h, _)| now - *h <= window + 30 * DAY);
    let base = losses
        .iter()
        .rev()
        .find(|(h, _)| now - *h >= window)
        .map(|(_, b)| b.clone());
    let Some(base) = base else {
        state.skip(now, "mcas", "x", DAY);
        note(
            state,
            "casualty_ratio",
            format!("collecting loss history ({} game days)", window / DAY),
        );
        return Ok(None);
    };
    let mut bad = Vec::new();
    let mut lines = Vec::new();
    for (tag, (own, enemy)) in &sample {
        let Some((own0, enemy0)) = base.get(tag) else {
            lines.push(format!("{tag}: no {}-day history yet", window / DAY));
            continue;
        };
        let lost = (own - own0).max(0.0);
        let killed = (enemy - enemy0).max(0.0);
        let ratio = if killed > 0.0 {
            lost / killed
        } else if lost > 0.0 {
            f64::INFINITY
        } else {
            lines.push(format!("{tag}: no losses in {} days", window / DAY));
            continue;
        };
        lines.push(format!("{tag}: {lost}/{killed} = {ratio:.2} (max {max})"));
        if lost >= min_losses && ratio > max {
            bad.push((tag.clone(), lost, killed, ratio));
        }
    }
    note(state, "casualty_ratio", lines.join("; "));
    if bad.is_empty() {
        state.skip(now, "mcas", "x", DAY);
        return Ok(None);
    }
    let Ok(roster) = tools.call("hoi4_armies", &json!({"threat": false})) else {
        state.skip(now, "mcas", "x", DAY);
        return Ok(None);
    };
    let (claimed, _) = invasion_claims(state, plan);
    let single = sample.len() == 1;
    let mut controllers: HashMap<i32, Option<String>> = HashMap::new();
    for (tag, lost, killed, ratio) in bad {
        let mut front: Vec<Value> = Vec::new();
        let mut unknown: Vec<String> = Vec::new();
        for a in attacking(state, &roster, &claimed, now, window) {
            let name = text(&a["army"]).to_string();
            let facing = if single {
                Some(tag.clone())
            } else {
                state.war_assign.get(&name).and_then(|t| {
                    controllers
                        .entry(*t)
                        .or_insert_with(|| {
                            tools
                                .call("hoi4_states", &json!({"query": t.to_string()}))
                                .ok()
                                .and_then(|v| {
                                    list(&v, "states")
                                        .first()
                                        .map(|r| text(&r["controller"]).to_string())
                                })
                                .filter(|c| !c.is_empty())
                        })
                        .clone()
                })
            };
            match facing {
                Some(f) if f == tag => front.push(a.clone()),
                Some(_) => {}
                None => unknown.push(name),
            }
        }
        let names: Vec<&str> = front.iter().map(|a| text(&a["army"])).collect();
        wake(
            state,
            plan,
            now,
            json!(28),
            &format!("casualty_ratio:{tag}"),
            format!(
                "own losses against {tag} over {} game days are {ratio:.2} times the enemy's ({lost} vs {killed}, limit {max}); resting {} for {rest_days} days",
                window / DAY,
                if names.is_empty() { "no army (none known on that front)".to_string() } else { names.join(", ") }
            ),
            json!({"enemy": tag, "own_losses": lost, "enemy_losses": killed,
                "ratio": if ratio.is_finite() { json!((ratio * 1000.0).round() / 1000.0) } else { Value::Null },
                "max": max, "window_days": window / DAY, "armies": names, "front_unknown": unknown}),
        );
        for row in &front {
            if let Some(line) = start_rest(state, tools, lease, row, "cas", rest_days, now)? {
                return Ok(Some(format!(
                    "casualty ratio {ratio:.2} against {tag}: {line}"
                )));
            }
        }
    }
    state.skip(now, "mcas", "x", DAY);
    Ok(None)
}

// ---------------------------------------------------------------------------------------------
// W2-5, rule 27: enemy air superiority over our front.

fn air_superiority_rule(
    state: &mut State,
    tools: &dyn Tools,
    plan: &Value,
    lease: &str,
    now: GameHour,
) -> Result<Option<String>> {
    let m = &plan["manage"];
    if !m.is_object() || off(plan, "air_superiority") || state.skipped(now, "mairsup", "x") {
        return Ok(None);
    }
    let limit = fraction(&m["enemy_air_superiority"], 0.6);
    let hold_days = days(&m["air_hold_days"], 10);
    let regions = tools
        .call("hoi4_air_regions", &json!({}))
        .unwrap_or(Value::Null);
    let read: Vec<(i64, String, f64)> = list(&regions, "regions")
        .iter()
        .filter_map(|r| {
            Some((
                r["region_id"].as_i64()?,
                text(&r["name"]).to_string(),
                as_fraction(&r["enemy_air_superiority"])?,
            ))
        })
        .collect();
    if read.is_empty() {
        state.skip(now, "mairsup", "x", 3 * DAY);
        note(state, "air_superiority", "unknown: no tool reads enemy air superiority (hoi4_air_regions has no enemy_air_superiority; hoi4_air: air_superiority_by_region not mapped): no action");
        return Ok(None);
    }
    let Ok(air) = tools.call("hoi4_air", &json!({})) else {
        state.skip(now, "mairsup", "x", 3 * DAY);
        note(state, "air_superiority", "hoi4_air unreadable");
        return Ok(None);
    };
    let wings = list(&air, "wings");
    let mut front: Vec<(i64, String)> = Vec::new();
    for w in wings {
        if strings(w, "missions").iter().any(|m| m == "cas") {
            if let Some(id) = w["region"]["id"].as_i64() {
                if !front.iter().any(|(f, _)| *f == id) {
                    front.push((id, text(&w["region"]["name"]).to_string()));
                }
            }
        }
    }
    for want in list(m, "air_front_regions") {
        let hit = read.iter().find(|(id, name, _)| {
            want.as_i64() == Some(*id) || want.as_str().is_some_and(|w| same_name(w, name))
        });
        if let Some((id, name, _)) = hit {
            if !front.iter().any(|(f, _)| f == id) {
                front.push((*id, name.clone()));
            }
        }
    }
    if front.is_empty() {
        state.skip(now, "mairsup", "x", 3 * DAY);
        note(
            state,
            "air_superiority",
            "no front region (no CAS wing over a region, no manage.air_front_regions)",
        );
        return Ok(None);
    }
    let level = |id: i64| read.iter().find(|r| r.0 == id).map(|r| r.2);
    let troubled: Vec<(i64, String, f64)> = front
        .iter()
        .filter_map(|(id, name)| {
            level(*id)
                .filter(|v| *v > limit)
                .map(|v| (*id, name.clone(), v))
        })
        .collect();
    if troubled.is_empty() {
        // every front region at or below the limit: offensives held for it go again
        let held: Vec<String> = state
            .done
            .keys()
            .filter_map(|k| k.strip_prefix("mrest:air:").map(String::from))
            .collect();
        for army in &held {
            state.bad.remove(&format!("mrest:{army}"));
        }
        state.skip(now, "mairsup", "x", 3 * DAY);
        note(
            state,
            "air_superiority",
            format!(
                "enemy at or below {limit} over every front region; {} hold(s) lifted",
                held.len()
            ),
        );
        return Ok(None);
    }
    let troubled_ids: HashSet<i64> = troubled.iter().map(|t| t.0).collect();
    let (claimed, _) = invasion_claims(state, plan);
    let roster = if front.len() == 1 {
        tools.call("hoi4_armies", &json!({"threat": false})).ok()
    } else {
        None
    };
    for (rid, rname, v) in &troubled {
        wake(
            state,
            plan,
            now,
            json!(27),
            &format!("air_superiority:{rid}"),
            format!("enemy air superiority over {rname} is {:.0}% (limit {:.0}%): fighters go there and offensives under it hold", v * 100.0, limit * 100.0),
            json!({"region": rid, "name": rname, "enemy_air_superiority": v, "limit": limit}),
        );
        let pick = wings
            .iter()
            .filter(|w| {
                let kind = text(&w["aircraft_type"]).to_lowercase();
                super::war_mission(&kind) == Some("air_superiority") && !kind.starts_with("cv_")
            })
            .filter(|w| {
                let over = w["region"]["id"].as_i64();
                over.is_none_or(|r| !troubled_ids.contains(&r))
            })
            .filter(|w| !super::plan_manage::air_plan_claims(plan, w))
            .filter(|w| {
                let n = text(&w["wing"]);
                !n.is_empty() && !state.skipped(now, "airsup", n)
            })
            .min_by_key(|w| w["standing_by"] != json!(true));
        if let Some(w) = pick {
            let name = text(&w["wing"]).to_string();
            let (ok, why) = state.act(
                tools,
                lease,
                "hoi4_air_mission",
                json!({"wing": name, "mission": "air_superiority", "region": rid.to_string()}),
                "aa",
            )?;
            state.skip(now, "airsup", &name, if ok { 14 * DAY } else { 3 * DAY });
            return Ok(Some(format!(
                "air superiority: enemy {:.0}% over {rname}: {name} air_superiority there: {ok} {why}",
                v * 100.0
            )));
        }
        let Some(roster) = roster.as_ref() else {
            note(state, "air_superiority", format!("enemy {:.0}% over {rname}; no fighter left to send; offensives not held (several front regions: which armies fight under {rname} is unknown)", v * 100.0));
            continue;
        };
        let span = hold_days * DAY;
        let rows: Vec<Value> = attacking(state, roster, &claimed, now, span)
            .into_iter()
            .cloned()
            .collect();
        for row in &rows {
            if let Some(line) = start_rest(state, tools, lease, row, "air", hold_days, now)? {
                return Ok(Some(format!(
                    "air superiority: enemy {:.0}% over {rname}: {line}",
                    v * 100.0
                )));
            }
        }
        note(
            state,
            "air_superiority",
            format!(
                "enemy {:.0}% over {rname}; no fighter left to send; {} army(ies) hold",
                v * 100.0,
                rows.len()
            ),
        );
    }
    state.skip(now, "mairsup", "x", 3 * DAY);
    Ok(None)
}

// ---------------------------------------------------------------------------------------------
// W2-1: army groups and field marshals.

/// The field marshal candidates of an entry, in order.
fn marshals(entry: &Value) -> Vec<String> {
    match &entry["marshal"] {
        Value::String(s) if !s.trim().is_empty() => vec![s.trim().to_string()],
        Value::Array(a) => a
            .iter()
            .filter_map(Value::as_str)
            .filter(|s| !s.trim().is_empty())
            .map(|s| s.trim().to_string())
            .collect(),
        _ => Vec::new(),
    }
}

fn army_group_step(
    state: &mut State,
    tools: &dyn Tools,
    plan: &Value,
    lease: &str,
    now: GameHour,
) -> Result<Option<String>> {
    let entries = list(plan, "army_group");
    if entries.is_empty() || state.skipped(now, "milag", "x") {
        return Ok(None);
    }
    let keyed: Vec<(String, &Value)> = entries
        .iter()
        .enumerate()
        .map(|(i, e)| (item_id(e).unwrap_or_else(|| format!("#{i}")), e))
        .collect();
    let open = |state: &State, id: &str, e: &Value| {
        !state.done.contains_key(&format!("agrp:{id}"))
            || (!marshals(e).is_empty() && !state.done.contains_key(&format!("agfm:{id}")))
    };
    // nothing left to do: no reads at all
    if !keyed.iter().any(|(id, e)| open(state, id, e)) {
        return Ok(None);
    }
    let Ok(roster) = tools.call("hoi4_armies", &json!({"threat": false})) else {
        state.skip(now, "milag", "x", DAY);
        note(state, "army_group", "hoi4_armies unreadable");
        return Ok(None);
    };
    let mut lines = Vec::new();
    for (id, entry) in keyed {
        if !open(state, &id, entry) || state.skipped(now, "agrp", &id) {
            continue;
        }
        let label = entry["name"]
            .as_str()
            .filter(|n| !n.trim().is_empty())
            .map_or_else(|| id.clone(), str::to_string);
        let wanted: Vec<String> = strings(entry, "armies")
            .iter()
            .map(|h| resolve(state, h))
            .collect();
        let rows: Vec<&Value> = wanted.iter().filter_map(|n| army_row(&roster, n)).collect();
        let groups: HashSet<Option<&str>> = rows.iter().map(|r| r["army_group"].as_str()).collect();
        let created = format!("agrp:{id}");
        if !state.done.contains_key(&created) {
            let when = when_holds(state, tools, &entry["when"], now);
            if when != Truth::True {
                lines.push(format!("{label}: waits, when {}", when.label()));
                continue;
            }
            if wanted.len() < 2 {
                lines.push(format!("{label}: a group needs two armies or more"));
                continue;
            }
            if rows.len() < wanted.len() {
                let missing: Vec<&str> = wanted
                    .iter()
                    .filter(|n| army_row(&roster, n).is_none())
                    .map(String::as_str)
                    .collect();
                lines.push(format!("{label}: waiting for {}", missing.join(", ")));
                continue;
            }
            if groups.len() == 1 && groups.iter().all(Option::is_some) {
                // already one group (made by hand, or before a restart)
                state.done.insert(created, now);
            } else if let Some(r) = rows.iter().find(|r| r["army_group"].as_str().is_some()) {
                let (army, group) = (text(&r["army"]), text(&r["army_group"]));
                lines.push(format!(
                    "{label}: {army} already belongs to {group}; not regrouped"
                ));
                wake(
                    state,
                    plan,
                    now,
                    json!("G4"),
                    &format!("army_group:{id}"),
                    format!("army group {label}: {army} already belongs to the army group {group}; armies are never regrouped by the executor"),
                    json!({"army_group": id, "army": army, "group": group, "armies": wanted}),
                );
                state.skip(now, "agrp", &id, 7 * DAY);
                continue;
            } else {
                let names: Vec<&str> = rows.iter().map(|r| text(&r["army"])).collect();
                let (ok, why) = state.act(
                    tools,
                    lease,
                    "hoi4_army_group_create",
                    json!({"armies": names}),
                    "ag",
                )?;
                if ok {
                    state.done.insert(created, now);
                } else {
                    state.skip(now, "agrp", &id, 2 * DAY);
                }
                note(state, "army_group", format!("{label}: created {ok}"));
                return Ok(Some(format!(
                    "army group {label} from {}: {ok} {why}",
                    names.join(", ")
                )));
            }
        }
        // the field marshal, once
        let candidates = marshals(entry);
        let led = format!("agfm:{id}");
        if candidates.is_empty() || state.done.contains_key(&led) {
            continue;
        }
        let group = match groups.iter().next() {
            Some(Some(g)) if groups.len() == 1 && rows.len() == wanted.len() => g.to_string(),
            _ => {
                lines.push(format!("{label}: the armies are not one group now"));
                continue;
            }
        };
        let leader = list(&roster, "army_groups")
            .iter()
            .find(|g| text(&g["army_group"]) == group)
            .and_then(|g| g["field_marshal"].as_str())
            .filter(|m| !m.trim().is_empty());
        if let Some(leader) = leader {
            state.done.insert(led, now);
            lines.push(format!(
                "{label}: {group} is led by {leader} (never reassigned)"
            ));
            continue;
        }
        let Some(general) = candidates
            .iter()
            .find(|c| !state.skipped(now, "agfmc", &format!("{id}|{c}")))
            .cloned()
        else {
            lines.push(format!("{label}: no listed field marshal is free"));
            wake(
                state,
                plan,
                now,
                json!("G5"),
                &format!("army_group_marshal:{id}"),
                format!("army group {label} ({group}): none of {} can lead it (unknown or leading another army; never reassigned)", candidates.join(", ")),
                json!({"army_group": id, "group": group, "marshal": candidates}),
            );
            continue;
        };
        let (ok, why) = state.act(
            tools,
            lease,
            "hoi4_army_commander",
            json!({"army": group, "general": general}),
            "af",
        )?;
        if ok {
            state.done.insert(led, now);
        } else {
            match code(&why) {
                "commander_unavailable" | "unknown_commander" => {
                    state.skip(now, "agfmc", &format!("{id}|{general}"), 30 * DAY)
                }
                "insufficient_command_power" | "promotion_unavailable" => {
                    state.skip(now, "agrp", &id, 7 * DAY)
                }
                _ => state.skip(now, "agrp", &id, 2 * DAY),
            }
        }
        note(
            state,
            "army_group",
            format!("{label}: field marshal {general} {ok}"),
        );
        return Ok(Some(format!(
            "army group {label} ({group}) field marshal {general}: {ok} {why}"
        )));
    }
    state.skip(now, "milag", "x", DAY);
    if !lines.is_empty() {
        note(state, "army_group", lines.join("; "));
    }
    Ok(None)
}

// ---------------------------------------------------------------------------------------------
// W2-4, rule 31: fleets, regions, missions, admirals, repair.

/// One listed fleet: its name, its coastal state for region orders and its mission entries
/// (entry key, entry) in order.
struct FleetPlan {
    fleet: String,
    state_id: Option<i64>,
    missions: Vec<(String, Value)>,
}

fn fleet_plans(cfg: &Value) -> Vec<FleetPlan> {
    let mut out: Vec<FleetPlan> = list(cfg, "fleets")
        .iter()
        .filter_map(|f| {
            let fleet = f["fleet"].as_str().filter(|n| !n.trim().is_empty())?;
            let missions = list(f, "missions")
                .iter()
                .enumerate()
                .map(|(i, m)| {
                    let key = item_id(m).unwrap_or_else(|| format!("{fleet}#{i}"));
                    (key, m.clone())
                })
                .collect();
            Some(FleetPlan {
                fleet: fleet.trim().to_string(),
                state_id: f["state_id"].as_i64(),
                missions,
            })
        })
        .collect();
    let escort = &cfg["convoy_escort"];
    if let Some(fleet) = escort["fleet"].as_str().filter(|f| !f.trim().is_empty()) {
        if escort["on"] != json!(false) {
            let mut entry = json!({"mission": "convoy_escort", "regions": escort["regions"],
                "when": escort["when"], "task_force": escort["task_force"]});
            if let Some(map) = entry.as_object_mut() {
                map.retain(|_, v| !v.is_null());
            }
            let key = "convoy_escort".to_string();
            match out.iter_mut().find(|p| same_name(&p.fleet, fleet)) {
                Some(p) => p.missions.insert(0, (key, entry)),
                None => out.push(FleetPlan {
                    fleet: fleet.trim().to_string(),
                    state_id: None,
                    missions: vec![(key, entry)],
                }),
            }
        }
    }
    out
}

/// (fleet, admiral) pairs: `fleets[].admiral`, then `admirals` (an object or a list).
fn admiral_plans(cfg: &Value) -> Vec<(String, String)> {
    let mut out: Vec<(String, String)> = Vec::new();
    let mut add = |fleet: &str, admiral: &str| {
        if !fleet.trim().is_empty()
            && !admiral.trim().is_empty()
            && !out.iter().any(|(f, _)| same_name(f, fleet))
        {
            out.push((fleet.trim().to_string(), admiral.trim().to_string()));
        }
    };
    for f in list(cfg, "fleets") {
        add(text(&f["fleet"]), text(&f["admiral"]));
    }
    match &cfg["admirals"] {
        Value::Object(map) => {
            for (fleet, admiral) in map {
                add(fleet, text(admiral));
            }
        }
        Value::Array(rows) => {
            for r in rows {
                add(text(&r["fleet"]), text(&r["admiral"]));
            }
        }
        _ => {}
    }
    out
}

/// A region of a mission entry: a display name, or {"region","state_id"}.
fn region_spec(v: &Value) -> Option<(String, Option<i64>)> {
    match v {
        Value::String(s) if !s.trim().is_empty() => Some((s.trim().to_string(), None)),
        Value::Object(_) => {
            let name = v["region"].as_str().filter(|s| !s.trim().is_empty())?;
            Some((name.trim().to_string(), v["state_id"].as_i64()))
        }
        _ => None,
    }
}

fn navy_step(
    state: &mut State,
    tools: &dyn Tools,
    plan: &Value,
    lease: &str,
    now: GameHour,
) -> Result<Option<String>> {
    let cfg = &plan["navy"];
    if !cfg.is_object() {
        return Ok(None);
    }
    if cfg["posture"] == json!("none") {
        note(state, "navy", "posture none: no naval orders");
        return Ok(None);
    }
    if state.skipped(now, "mnavy", "x") {
        return Ok(None);
    }
    let Ok(read) = tools.call("hoi4_fleets", &json!({})) else {
        state.skip(now, "mnavy", "x", 3 * DAY);
        note(state, "navy", "hoi4_fleets unreadable");
        return Ok(None);
    };
    let fleets = list(&read, "fleets");
    let repair_below = fraction(&cfg["repair_below"], 0.5);
    let repair_until = fraction(&cfg["repair_until"], 0.9).max(repair_below);
    let (_, invasion_fleet) = invasion_claims(state, plan);
    // 1. damaged task forces to port (every fleet)
    for tf in fleets.iter().flat_map(|f| list(f, "task_forces")) {
        let name = text(&tf["task_force"]).to_string();
        let (Some(share), Some(mission)) = (strength(tf), tf["mission"]["name"].as_str()) else {
            continue;
        };
        if name.is_empty() {
            continue;
        }
        let repairing = format!("nrep:{name}");
        if state.done.contains_key(&repairing) {
            if share >= repair_until {
                state.done.remove(&repairing);
            }
            continue;
        }
        if share >= repair_below || mission == "reserve" {
            continue;
        }
        if mission == "hold" {
            state.done.insert(repairing, now);
            continue;
        }
        if state.skipped(now, "navtf", &name) {
            continue;
        }
        let (ok, why) = state.act(
            tools,
            lease,
            "hoi4_fleet_mission",
            json!({"task_force": name, "mission": "hold"}),
            "nh",
        )?;
        if ok {
            state.done.insert(repairing, now);
        } else {
            state.skip(now, "navtf", &name, 3 * DAY);
        }
        return Ok(Some(format!(
            "navy: {name} at {:.0}% strength to port (hold): {ok} {why}",
            share * 100.0
        )));
    }
    // 2. admirals, once per fleet, never moved from another fleet
    for (fleet, admiral) in admiral_plans(cfg) {
        let Some(row) = fleets.iter().find(|f| same_name(&fleet, text(&f["fleet"]))) else {
            continue;
        };
        let name = text(&row["fleet"]).to_string();
        let key = format!("nadm:{name}");
        if state.done.contains_key(&key) || state.skipped(now, "nadm", &name) {
            continue;
        }
        if row["admiral"]
            .as_str()
            .is_some_and(|a| !a.trim().is_empty())
        {
            state.done.insert(key, now);
            continue;
        }
        if let Some(other) = fleets.iter().find(|f| {
            text(&f["fleet"]) != name
                && f["admiral"]
                    .as_str()
                    .is_some_and(|a| same_name(a, &admiral))
        }) {
            let other = text(&other["fleet"]).to_string();
            state.skip(now, "nadm", &name, 30 * DAY);
            wake(
                state,
                plan,
                now,
                json!(31),
                &format!("admiral:{name}"),
                format!("fleet {name}: the planned admiral {admiral} leads {other}; he is not moved (no reassignment churn)"),
                json!({"fleet": name, "admiral": admiral, "leads": other}),
            );
            continue;
        }
        let (ok, why) = state.act(
            tools,
            lease,
            "hoi4_fleet_admiral",
            json!({"fleet": name, "admiral": admiral}),
            "na",
        )?;
        if ok || code(&why) == "nothing_to_change" {
            state.done.insert(key, now);
        } else if code(&why) == "unknown_commander" {
            state.skip(now, "nadm", &name, 30 * DAY);
            wake(
                state,
                plan,
                now,
                json!(31),
                &format!("admiral:{name}"),
                format!("fleet {name}: no admiral {admiral} is offered"),
                json!({"fleet": name, "admiral": admiral, "refusal": why}),
            );
        } else {
            state.skip(now, "nadm", &name, 7 * DAY);
        }
        return Ok(Some(format!(
            "navy: admiral {admiral} -> {name}: {ok} {why}"
        )));
    }
    // 3. regions, then the planned mission per task force
    let mut on_plan = 0;
    let mut missing: Vec<String> = Vec::new();
    for fp in fleet_plans(cfg) {
        let Some(row) = fleets
            .iter()
            .find(|f| same_name(&fp.fleet, text(&f["fleet"])))
        else {
            missing.push(fp.fleet.clone());
            continue;
        };
        let fleet = text(&row["fleet"]).to_string();
        if invasion_fleet
            .as_deref()
            .is_some_and(|f| same_name(f, &fleet))
        {
            continue;
        }
        let assigned: Vec<(String, String)> = list(row, "assigned_regions")
            .iter()
            .map(|r| (text(&r["key"]).to_string(), text(&r["name"]).to_string()))
            .collect();
        let home = row["home_port"]["state_id"].as_i64();
        for tf in list(row, "task_forces") {
            let tname = text(&tf["task_force"]).to_string();
            if tname.is_empty()
                || state.done.contains_key(&format!("nrep:{tname}"))
                || state.skipped(now, "navtf", &tname)
            {
                continue;
            }
            let Some(current) = tf["mission"]["name"].as_str() else {
                continue;
            };
            if current == "reserve" {
                continue;
            }
            let mut chosen = None;
            for (key, m) in &fp.missions {
                if m["task_force"]
                    .as_str()
                    .is_some_and(|t| !same_name(t, &tname))
                {
                    continue;
                }
                if when_holds(state, tools, &m["when"], now) == Truth::True {
                    chosen = Some((key.clone(), m.clone()));
                    break;
                }
            }
            let Some((key, m)) = chosen else { continue };
            let Some(mission) = m["mission"].as_str().filter(|s| !s.trim().is_empty()) else {
                continue;
            };
            // the entry's sea regions first
            let mut waiting = false;
            for spec in list(&m, "regions") {
                let Some((region, sid)) = region_spec(spec) else {
                    continue;
                };
                let rkey = format!("{fleet}|{region}");
                if assigned
                    .iter()
                    .any(|(k, n)| same_name(n, &region) || same_name(k, &region))
                    || state.done.contains_key(&format!("nreg:{rkey}"))
                {
                    continue;
                }
                if state.skipped(now, "nreg", &rkey) {
                    waiting = true;
                    continue;
                }
                let Some(sid) = sid.or(m["state_id"].as_i64()).or(fp.state_id).or(home) else {
                    note(
                        state,
                        "navy",
                        format!("{fleet}: no coastal state for region {region}"),
                    );
                    waiting = true;
                    continue;
                };
                let (ok, why) = state.act(
                    tools,
                    lease,
                    "hoi4_fleet_region",
                    json!({"fleet": fleet, "region": region, "state_id": sid}),
                    "nr",
                )?;
                if ok || code(&why) == "already_assigned" {
                    state.done.insert(format!("nreg:{rkey}"), now);
                } else if code(&why) == "region_not_found" {
                    state.skip(now, "nreg", &rkey, 30 * DAY);
                    wake(
                        state,
                        plan,
                        now,
                        json!(31),
                        &format!("sea_region:{rkey}"),
                        format!(
                            "fleet {fleet}: sea region {region} was not found from state {sid}"
                        ),
                        json!({"fleet": fleet, "region": region, "state_id": sid, "refusal": why}),
                    );
                } else {
                    state.skip(now, "nreg", &rkey, 7 * DAY);
                }
                return Ok(Some(format!(
                    "navy: {fleet} region {region} (from state {sid}): {ok} {why}"
                )));
            }
            if waiting {
                continue;
            }
            if current == mission {
                on_plan += 1;
                continue;
            }
            let once = format!("nmis:{tname}:{key}");
            if current != "hold" && state.done.contains_key(&once) {
                continue;
            }
            let (ok, why) = state.act(
                tools,
                lease,
                "hoi4_fleet_mission",
                json!({"task_force": tname, "mission": mission}),
                "nm",
            )?;
            if ok || code(&why) == "nothing_to_change" {
                let prefix = format!("nmis:{tname}:");
                state.done.retain(|k, _| !k.starts_with(&prefix));
                state.done.insert(once, now);
            } else {
                state.skip(now, "navtf", &tname, 3 * DAY);
                if matches!(code(&why), "no_assigned_region" | "mission_unavailable") {
                    wake(
                        state,
                        plan,
                        now,
                        json!(31),
                        &format!("fleet_mission:{tname}"),
                        format!("task force {tname} ({fleet}) cannot take {mission}: {why}"),
                        json!({"fleet": fleet, "task_force": tname, "mission": mission, "refusal": why}),
                    );
                }
            }
            return Ok(Some(format!(
                "navy: {tname} ({fleet}) {current} -> {mission}: {ok} {why}"
            )));
        }
    }
    state.skip(now, "mnavy", "x", 3 * DAY);
    let mut line = format!("{on_plan} task force(s) on their planned mission");
    if !missing.is_empty() {
        line.push_str(&format!("; fleets not found: {}", missing.join(", ")));
    }
    note(state, "navy", line);
    Ok(None)
}

// ---------------------------------------------------------------------------------------------
// hold_targets (lesson L01): an army holds the supply corridor behind the advance.

/// Candidate states read per call to learn the states of a corridor's provinces.
const HOLD_SCAN: usize = 8;

/// A running hold: its id, its army (resolved) and its plan entry.
struct Hold<'a> {
    id: String,
    army: String,
    entry: &'a Value,
}

/// Whether the army is bound to a running hold (the war step leaves it alone but for its
/// pocket fall-back).
pub(super) fn holding(state: &State, army: &str) -> bool {
    state.military.held.contains_key(army)
}

/// A read through the tick's read cache (shared with the plan predicates): each tool and
/// arguments at most once per tick.
fn cached(
    state: &mut State,
    tools: &dyn Tools,
    name: &str,
    args: &Value,
    now: GameHour,
) -> Option<Value> {
    let cache = match state.pcache.take() {
        Some((hour, cache)) if hour == now => cache,
        _ => Cache::default(),
    };
    let mut ctx = Ctx::with_cache(tools, now, cache);
    let out = ctx.read(name, args).cloned();
    state.pcache = Some((now, ctx.into_cache()));
    out
}

/// Keeps the last reply of the calls made through it: `State::act` reports a receipt's status
/// only, and the hold reads the move pre-check's note in the receipt's detail.
struct Receipts<'a> {
    inner: &'a dyn Tools,
    last: RefCell<Option<Value>>,
}
impl Tools for Receipts<'_> {
    fn call(&self, name: &str, args: &Value) -> Result<Value> {
        let out = self.inner.call(name, args);
        *self.last.borrow_mut() = out.as_ref().ok().cloned();
        out
    }
}

/// The row of state `id` in a hoi4_states read.
fn state_row(read: &Value, id: i64) -> Option<&Value> {
    list(read, "states")
        .iter()
        .find(|r| r["state_id"].as_i64() == Some(id))
}

/// Whether `State::act` refused an order because the failure ledger holds it.
fn ledger_stopped(why: &str) -> bool {
    why.starts_with("skipped:")
}

/// The guard post army_layout gives the army playing a guard role (guard_states_in_order[0]).
fn layout_guard_post(state: &State, plan: &Value, army: &str) -> Option<i32> {
    list(plan, "army_layout")
        .iter()
        .filter(|e| text(&e["role"]) == "guard")
        .find(|e| {
            state
                .layout
                .get(text(&e["name_hint"]))
                .is_some_and(|a| a == army)
        })
        .and_then(|e| {
            list(e, "guard_states_in_order")
                .first()
                .and_then(Value::as_i64)
        })
        .map(|p| p as i32)
}

/// Bind the army to its hold: `war_guard` names its post's state (else what it named), so the
/// war step neither drives it nor draws it to a retake nor sends its guard order. The first
/// bind keeps the guard post the army had and clears what the war step drove it for.
fn bind(state: &mut State, plan: &Value, id: &str, army: &str, post_state: Option<i64>) {
    if !state.military.held.contains_key(army) {
        let guard = state
            .war_guard
            .get(army)
            .copied()
            .or_else(|| layout_guard_post(state, plan, army));
        state
            .military
            .held
            .insert(army.to_string(), (id.to_string(), guard));
        state.war_assign.remove(army);
        state.war_stage.remove(army);
        state.war_move.remove(army);
        state.war_since.remove(army);
        state.war_rest.remove(army);
        state.war_repeat.remove(army);
        state.war_province.remove(army);
    }
    let post = post_state
        .map(|s| s as i32)
        .or_else(|| state.war_guard.get(army).copied())
        .or_else(|| state.military.held.get(army).and_then(|(_, g)| *g))
        .unwrap_or(0);
    state.war_guard.insert(army.to_string(), post);
}

/// The hold is over for the army: bound to the guard post it had again with its guard stage
/// reset (the war step sends it back there and draws its front), else back to the war step.
fn unbind(state: &mut State, plan: &Value, army: &str, why: &str) {
    let Some((id, guard)) = state.military.held.remove(army) else {
        return;
    };
    state.military.hold_orders.remove(&id);
    state.war_stage.remove(&format!("guard:{army}"));
    let back = match guard.or_else(|| layout_guard_post(state, plan, army)) {
        Some(post) => {
            state.war_guard.insert(army.to_string(), post);
            format!("back to its guard post, state {post}")
        }
        None => {
            state.war_guard.remove(army);
            "back to the war step".to_string()
        }
    };
    note(
        state,
        &format!("hold:{id}"),
        format!("{army} released ({why}): {back}"),
    );
}

/// hold_targets: start and end the holds, bind every running hold's army (an army that left
/// its hold is released), and send at most one order (a move the pre-check settles without
/// input does not count: the next hold is looked at).
pub(super) fn hold_step(
    state: &mut State,
    tools: &dyn Tools,
    plan: &Value,
    lease: &str,
    now: GameHour,
) -> Result<Option<String>> {
    let entries = list(plan, "hold_targets");
    if entries.is_empty() && state.military.held.is_empty() {
        return Ok(None);
    }
    let (invading, _) = invasion_claims(state, plan);
    let roles: HashSet<&str> = list(plan, "army_layout")
        .iter()
        .map(|e| text(&e["name_hint"]))
        .filter(|h| !h.is_empty())
        .collect();
    let mut running: Vec<Hold> = Vec::new();
    for (i, e) in entries.iter().enumerate() {
        // an entry without an army is the player's (a mission to watch), never acted on
        let hint = text(&e["army"]).trim();
        if hint.is_empty() {
            continue;
        }
        let id = item_id(e).unwrap_or_else(|| format!("#{i}"));
        let label = format!("hold:{id}");
        if state.done.contains_key(&format!("hold:{id}:released")) {
            continue;
        }
        if !state.done.contains_key(&format!("hold:{id}:on")) {
            match when_holds(state, tools, &e["when"], now) {
                Truth::True => {
                    state.done.insert(format!("hold:{id}:on"), now);
                }
                t => {
                    note(state, &label, format!("waits: when {}", t.label()));
                    continue;
                }
            }
        }
        let until = &e["until"];
        let set = !(until.is_null() || until.as_str().is_some_and(|s| s.trim().is_empty()));
        if set && when_holds(state, tools, until, now) == Truth::True {
            state.done.insert(format!("hold:{id}:released"), now);
            note(state, &label, "over: until holds");
            continue;
        }
        if roles.contains(hint) && state.layout.get(hint).is_none_or(|a| a.is_empty()) {
            note(
                state,
                &label,
                format!("waits for the army playing {hint} (army_layout)"),
            );
            continue;
        }
        let army = resolve(state, hint);
        if invading.contains(&army) {
            note(
                state,
                &label,
                format!("{army} is in the running naval invasion; waits"),
            );
            continue;
        }
        if let Some(other) = running.iter().find(|h| h.army == army) {
            note(
                state,
                &label,
                format!("{army} already holds {}; waits", other.id),
            );
            continue;
        }
        running.push(Hold { id, army, entry: e });
    }
    let ended: Vec<(String, String)> = state
        .military
        .held
        .iter()
        .filter(|(army, (id, _))| !running.iter().any(|h| h.id == *id && h.army == **army))
        .map(|(army, (id, _))| (army.clone(), id.clone()))
        .collect();
    for (army, id) in ended {
        let why = if state.done.contains_key(&format!("hold:{id}:released")) {
            "until holds"
        } else {
            "its hold no longer runs for it"
        };
        unbind(state, plan, &army, why);
    }
    for hold in &running {
        if let Some(line) = hold_one(state, tools, plan, lease, now, hold)? {
            return Ok(Some(line));
        }
    }
    Ok(None)
}

/// Learn the state of every listed province (false while one is unknown): the own and the
/// enemies' hoi4_states rows name their victory point provinces and the provinces others hold;
/// the rest come from reading candidate states, the entry's `state` first and then the nearest
/// to the corridor (else to the army), at most HOLD_SCAN a call. A province in none of them
/// wakes the player.
fn locate(
    state: &mut State,
    tools: &dyn Tools,
    plan: &Value,
    hold: &Hold,
    provinces: &[i64],
    me: &str,
    now: GameHour,
) -> bool {
    let missing = |s: &State| -> Vec<i64> {
        provinces
            .iter()
            .copied()
            .filter(|p| !s.military.province_state.contains_key(p))
            .collect()
    };
    if missing(state).is_empty() {
        return true;
    }
    let label = format!("hold:{}", hold.id);
    if state.skipped(now, "holdfind", &hold.id) {
        return false;
    }
    let mut candidates: Vec<(i64, Option<(f64, f64)>)> = Vec::new();
    let mut tags = vec![me.to_string()];
    for tag in strings(&plan["war"], "enemies") {
        if !tags.contains(&tag) {
            tags.push(tag);
        }
    }
    let mut next = 0;
    while next < tags.len() && next < 8 {
        let tag = tags[next].clone();
        next += 1;
        let Some(read) = cached(state, tools, "hoi4_states", &json!({"query": tag}), now) else {
            continue;
        };
        for row in list(&read, "states") {
            let Some(sid) = row["state_id"].as_i64() else {
                continue;
            };
            for key in ["provinces_held_by_others", "victory_points", "provinces"] {
                for p in list(row, key) {
                    if let Some(pid) = p["province"].as_i64() {
                        state.military.province_state.insert(pid, sid);
                    }
                }
            }
            // the countries holding parts of these states own the rest of the corridor
            let holders = list(row, "provinces_held_by_others")
                .iter()
                .map(|p| text(&p["controller"]))
                .chain([text(&row["owner"]), text(&row["controller"])]);
            for holder in holders {
                if !holder.is_empty() && !tags.iter().any(|t| t == holder) {
                    tags.push(holder.to_string());
                }
            }
            if !candidates.iter().any(|(s, _)| *s == sid) {
                candidates.push((sid, row["x"].as_f64().zip(row["z"].as_f64())));
            }
        }
    }
    if missing(state).is_empty() {
        return true;
    }
    // nearest to the corridor's states already known, else to the army's state
    let centre = |s: i64| {
        candidates
            .iter()
            .find(|(c, _)| *c == s)
            .and_then(|(_, at)| *at)
    };
    let known: Vec<(f64, f64)> = provinces
        .iter()
        .filter_map(|p| state.military.province_state.get(p))
        .filter_map(|s| centre(*s))
        .collect();
    let reference = if known.is_empty() {
        cached(state, tools, "hoi4_armies", &json!({}), now)
            .and_then(|r| army_row(&r, &hold.army).and_then(|a| a["main_state"].as_i64()))
            .and_then(centre)
    } else {
        let n = known.len() as f64;
        Some((
            known.iter().map(|k| k.0).sum::<f64>() / n,
            known.iter().map(|k| k.1).sum::<f64>() / n,
        ))
    };
    let first = hold.entry["state"].as_i64();
    let mut order: Vec<(i64, f64)> = candidates
        .iter()
        .map(|(s, at)| {
            let d = match (reference, at) {
                _ if Some(*s) == first => -1.0,
                (Some((rx, rz)), Some((x, z))) => (x - rx).powi(2) + (z - rz).powi(2),
                _ => f64::MAX,
            };
            (*s, d)
        })
        .collect();
    if let Some(s) = first.filter(|s| !candidates.iter().any(|(c, _)| c == s)) {
        order.push((s, -1.0));
    }
    order.retain(|(s, _)| !state.military.scanned.contains(s));
    order.sort_by(|a, b| a.1.total_cmp(&b.1));
    for (sid, _) in order.iter().take(HOLD_SCAN) {
        let Some(read) = cached(
            state,
            tools,
            "hoi4_states",
            &json!({"query": sid.to_string()}),
            now,
        ) else {
            continue;
        };
        let Some(row) = state_row(&read, *sid) else {
            continue;
        };
        let found = list(row, "provinces");
        if found.is_empty() {
            continue;
        }
        for p in found {
            if let Some(pid) = p["province"].as_i64() {
                state.military.province_state.insert(pid, *sid);
            }
        }
        state.military.scanned.insert(*sid);
        if missing(state).is_empty() {
            return true;
        }
    }
    let left = missing(state);
    if order.len() > HOLD_SCAN {
        note(
            state,
            &label,
            format!("{}: finding the states of provinces {left:?}", hold.army),
        );
        return false;
    }
    // every candidate state was read: these provinces are in none of them
    state.skip(now, "holdfind", &hold.id, DAY);
    note(
        state,
        &label,
        format!(
            "{}: provinces {left:?} are in no own or enemy state; waits",
            hold.army
        ),
    );
    wake(
        state,
        plan,
        now,
        json!("L01"),
        &format!("hold_province:{}", hold.id),
        format!(
            "hold {}: provinces {left:?} are in no own or enemy state read (hoi4_states); check the ids",
            hold.id
        ),
        json!({"army": hold.army, "provinces": left}),
    );
    false
}

/// One running hold: the post from the corridor's controllers, the binding, at most one order.
fn hold_one(
    state: &mut State,
    tools: &dyn Tools,
    plan: &Value,
    lease: &str,
    now: GameHour,
    hold: &Hold,
) -> Result<Option<String>> {
    let (id, army) = (hold.id.as_str(), hold.army.as_str());
    let label = format!("hold:{id}");
    let provinces: Vec<i64> = list(hold.entry, "provinces")
        .iter()
        .filter_map(Value::as_i64)
        .collect();
    if provinces.is_empty() {
        bind(state, plan, id, army, None);
        note(state, &label, format!("{army}: no provinces listed"));
        return Ok(None);
    }
    let me = cached(state, tools, "hoi4_overview", &json!({}), now)
        .map(|o| text(&o["player"]).to_string())
        .unwrap_or_default();
    if me.is_empty() {
        bind(state, plan, id, army, None);
        note(state, &label, "player unknown (hoi4_overview unreadable)");
        return Ok(None);
    }
    if !locate(state, tools, plan, hold, &provinces, &me, now) {
        bind(state, plan, id, army, None);
        return Ok(None);
    }
    // who holds each listed province: its state's read, every tick (a change acts at once)
    let mut mine: Vec<Option<bool>> = Vec::new();
    let mut seen = String::new();
    for p in &provinces {
        let sid = state.military.province_state.get(p).copied().unwrap_or(0);
        let read = cached(
            state,
            tools,
            "hoi4_states",
            &json!({"query": sid.to_string()}),
            now,
        );
        let holder = read
            .as_ref()
            .and_then(|r| state_row(r, sid))
            .and_then(|row| {
                list(row, "provinces")
                    .iter()
                    .find(|x| x["province"].as_i64() == Some(*p))
                    .map(|x| text(&x["controller"]).to_string())
            })
            .filter(|c| !c.is_empty());
        seen.push_str(&format!("{p}:{} ", holder.as_deref().unwrap_or("?")));
        mine.push(holder.map(|c| c == me));
    }
    let changed = state.military.hold_seen.get(id) != Some(&seen);
    state.military.hold_seen.insert(id.to_string(), seen);
    if !changed && state.skipped(now, "hold", id) {
        bind(state, plan, id, army, None);
        return Ok(None);
    }
    state.skip(now, "hold", id, DAY);
    if mine.iter().any(Option::is_none) {
        bind(state, plan, id, army, None);
        state.skip(now, "hold", id, DAY / 4);
        note(
            state,
            &label,
            format!("{army}: who holds a listed province is unknown (hoi4_states); waits"),
        );
        return Ok(None);
    }
    let mine: Vec<bool> = mine.into_iter().map(|m| m == Some(true)).collect();
    let Some(front) = mine.iter().rposition(|m| *m) else {
        bind(state, plan, id, army, None);
        note(
            state,
            &label,
            format!("{army}: no listed province is held; waits"),
        );
        wake(
            state,
            plan,
            now,
            json!("L01"),
            &format!("hold_lost:{id}"),
            format!("hold {id}: every province of the corridor is lost; {army} has no post"),
            json!({"army": army, "provinces": provinces}),
        );
        return Ok(None);
    };
    // a lost province behind the most forward held one comes first: the rearmost
    let at = mine[..front].iter().position(|m| !*m).unwrap_or(front);
    let post = provinces[at];
    let post_state = state
        .military
        .province_state
        .get(&post)
        .copied()
        .unwrap_or(0);
    bind(state, plan, id, army, Some(post_state));
    let why = if at < front {
        format!("province {post} behind the post is lost: retaken first")
    } else {
        "the most forward held province".to_string()
    };
    // the war step's pocket fall-back owns the army and voids the last order
    if state.skipped(now, "pocket", army) {
        state.military.hold_orders.remove(id);
        note(
            state,
            &label,
            format!("{army} falls back from a pocket (war step); province {post} waits"),
        );
        return Ok(None);
    }
    if let Some(o) = state
        .military
        .hold_orders
        .get(id)
        .filter(|o| o.army == army && o.post == post)
    {
        if o.under_way || now - o.at < DAY {
            let how = if o.under_way {
                "heading there or standing there"
            } else {
                "ordered"
            };
            note(
                state,
                &label,
                format!("{army} holds province {post} (state {post_state}), {why}: {how}"),
            );
            return Ok(None);
        }
    }
    let Some(roster) = cached(state, tools, "hoi4_armies", &json!({}), now) else {
        state.skip(now, "hold", id, DAY / 4);
        note(state, &label, "hoi4_armies unreadable; waits");
        return Ok(None);
    };
    let Some(row) = army_row(&roster, army).cloned() else {
        note(
            state,
            &label,
            format!("{army} not found (hoi4_armies); waits"),
        );
        return Ok(None);
    };
    if row["threat"]["encirclement_risk"] == json!(true) {
        state.military.hold_orders.remove(id);
        note(
            state,
            &label,
            format!("{army} is at risk of encirclement: no hold order (the war step's pocket fall-back owns it)"),
        );
        return Ok(None);
    }
    let claim = plan["war"]["player_claim_seconds"].as_u64().unwrap_or(300);
    if crate::autopilot::autopilot()
        .claimed(Duration::from_secs(claim))
        .iter()
        .any(|a| a == army)
    {
        note(
            state,
            &label,
            format!("{army} was ordered by the player: left alone"),
        );
        return Ok(None);
    }
    let stopped = |state: &mut State, what: &str, why: &str| {
        note(state, &label, format!("{army}: {what}: {why}"));
        wake(
            state,
            plan,
            now,
            json!("L01"),
            &label,
            format!("hold {id}: {what} for {army} failed 3 times in 10 game days; the corridor is not held"),
            json!({"army": army, "province": post, "state_id": post_state}),
        );
    };
    // an executing battle plan would take the army away again: stopped first
    if row["plan_executing"] == json!(true) {
        let (ok, why) = state.act(
            tools,
            lease,
            "hoi4_army_plan",
            json!({"army": army, "execute": false}),
            "hp",
        )?;
        if ledger_stopped(&why) {
            stopped(state, "stopping the battle plan", &why);
            return Ok(None);
        }
        // the move follows in two game hours
        state.skip(now, "hold", id, if ok { 2 } else { DAY });
        note(
            state,
            &label,
            format!("{army} stops its battle plan: {ok} {why}"),
        );
        return Ok(Some(format!(
            "hold {id}: {army} stops its battle plan to hold the corridor: {ok} {why}"
        )));
    }
    let receipts = Receipts {
        inner: tools,
        last: RefCell::new(None),
    };
    let (ok, why_sent) = state.act(
        &receipts,
        lease,
        "hoi4_army_move",
        json!({"army": army, "state_id": post_state, "province_id": post}),
        "hd",
    )?;
    let line = format!(
        "hold {id}: {army} -> province {post} (state {post_state}), {why}: {ok} {why_sent}"
    );
    if !ok {
        state.military.hold_orders.remove(id);
        if ledger_stopped(&why_sent) {
            stopped(state, &format!("the move to province {post}"), &why_sent);
            return Ok(None);
        }
        note(state, &label, line.clone());
        return Ok(Some(line));
    }
    let under_way = receipts
        .last
        .borrow()
        .as_ref()
        .and_then(|r| r["receipt"]["detail"]["note"].as_str().map(str::to_string))
        .is_some_and(|n| n.contains("nothing was sent"));
    state.military.hold_orders.insert(
        id.to_string(),
        HoldOrder {
            army: army.to_string(),
            post,
            at: now,
            under_way,
        },
    );
    if under_way {
        // the pre-check settled it without input: not an action, the next hold is looked at
        note(
            state,
            &label,
            format!("{army} holds province {post} (state {post_state}), {why}: already heading or standing there"),
        );
        return Ok(None);
    }
    note(state, &label, line.clone());
    Ok(Some(line))
}

#[cfg(test)]
mod tests {
    use super::super::{date_hour, tick, Tick};
    use super::*;
    use crate::error::Error;
    use std::cell::RefCell;

    const START: GameHour = 1_000_000;

    /// Reads answer from `replies`; actions are Verified unless `refuse` names their tool.
    struct Fake {
        calls: RefCell<Vec<(String, Value)>>,
        replies: RefCell<HashMap<String, Value>>,
        refuse: RefCell<HashMap<String, &'static str>>,
    }
    impl Tools for Fake {
        fn call(&self, name: &str, args: &Value) -> Result<Value> {
            self.calls
                .borrow_mut()
                .push((name.to_string(), args.clone()));
            if args.get("request_id").is_some() {
                if let Some(code) = self.refuse.borrow().get(name) {
                    return Err(Error::new(code, "refused by the fake"));
                }
                return Ok(json!({"receipt": {"status": "Verified"}}));
            }
            Ok(self
                .replies
                .borrow()
                .get(name)
                .cloned()
                .unwrap_or_else(|| json!({})))
        }
    }
    fn fake(replies: Vec<(&str, Value)>) -> Fake {
        Fake {
            calls: RefCell::new(Vec::new()),
            replies: RefCell::new(
                replies
                    .into_iter()
                    .map(|(k, v)| (k.to_string(), v))
                    .collect(),
            ),
            refuse: RefCell::new(HashMap::new()),
        }
    }
    impl Fake {
        fn reply(&self, tool: &str, value: Value) {
            self.replies.borrow_mut().insert(tool.to_string(), value);
        }
        fn refusing(&self, tool: &str, code: &'static str) {
            self.refuse.borrow_mut().insert(tool.to_string(), code);
        }
        fn allow(&self) {
            self.refuse.borrow_mut().clear();
        }
    }
    fn actions(f: &Fake) -> Vec<(String, Value)> {
        f.calls
            .borrow()
            .iter()
            .filter(|(_, a)| a.get("request_id").is_some())
            .cloned()
            .collect()
    }
    fn restarted(state: &State, now: GameHour) -> State {
        let memory = state.memory(now);
        let mut fresh = State::default();
        fresh.restore(&memory, now);
        fresh
    }

    fn roster(armies: &[(&str, Option<&str>, bool)], groups: &[(&str, Option<&str>)]) -> Value {
        json!({
            "armies": armies.iter().map(|(a, g, exec)| json!({"army": a, "army_group": g,
                "commander": null, "plan_executing": exec, "battle_plan_orders": 3,
                "division_count": 8, "main_state": 2})).collect::<Vec<_>>(),
            "army_groups": groups.iter().map(|(g, m)| json!({"army_group": g, "field_marshal": m}))
                .collect::<Vec<_>>()})
    }

    // --- W2-1 army groups -------------------------------------------------------------------

    #[test]
    fn an_army_group_is_made_once_its_armies_exist_and_gets_its_marshal_once() {
        let tools = fake(vec![(
            "hoi4_armies",
            roster(&[("1. Armee", None, false)], &[]),
        )]);
        let plan = json!({"army_group": [{"id": "south", "name": "Heeresgruppe Süd",
            "armies": ["strike_north", "2. Armee"], "marshal": ["Rundstedt", "Bock"],
            "when": "date:1936.1.1"}]});
        let t0 = date_hour("1936.1.1").unwrap();
        let mut state = State::default();
        state
            .layout
            .insert("strike_north".into(), "1. Armee".into());
        // before its date: waits
        assert_eq!(
            army_group_step(&mut state, &tools, &plan, "l", t0 - DAY).unwrap(),
            None
        );
        assert!(state.military.notes["army_group"].contains("when false"));
        // the second army does not exist yet: waits, nothing sent
        assert_eq!(
            army_group_step(&mut state, &tools, &plan, "l", t0).unwrap(),
            None
        );
        assert!(state.military.notes["army_group"].contains("waiting for 2. Armee"));
        tools.reply(
            "hoi4_armies",
            roster(&[("1. Armee", None, false), ("2. Armee", None, false)], &[]),
        );
        // checked once a game day
        assert_eq!(
            army_group_step(&mut state, &tools, &plan, "l", t0 + 12).unwrap(),
            None
        );
        let out = army_group_step(&mut state, &tools, &plan, "l", t0 + DAY).unwrap();
        assert_eq!(
            out.as_deref(),
            Some("army group Heeresgruppe Süd from 1. Armee, 2. Armee: true Verified")
        );
        let sent = actions(&tools);
        assert_eq!(sent[0].0, "hoi4_army_group_create");
        assert_eq!(sent[0].1["armies"], json!(["1. Armee", "2. Armee"]));
        // grouped: the first marshal is refused (leads another army), the next one is tried
        tools.reply(
            "hoi4_armies",
            roster(
                &[
                    ("1. Armee", Some("HG 1"), false),
                    ("2. Armee", Some("HG 1"), false),
                ],
                &[("HG 1", None)],
            ),
        );
        tools.refusing("hoi4_army_commander", "commander_unavailable");
        let out = army_group_step(&mut state, &tools, &plan, "l", t0 + DAY).unwrap();
        assert!(out
            .unwrap()
            .contains("Rundstedt: false commander_unavailable"));
        assert!(
            actions(&tools)[1].1.get("reassign").is_none(),
            "never reassign"
        );
        tools.allow();
        let out = army_group_step(&mut state, &tools, &plan, "l", t0 + DAY).unwrap();
        assert_eq!(
            out.as_deref(),
            Some("army group Heeresgruppe Süd (HG 1) field marshal Bock: true Verified")
        );
        assert_eq!(actions(&tools)[2].1["army"], "HG 1");
        // never repeats, also after a restart through plan_state.json
        let later = t0 + 3 * DAY;
        assert_eq!(
            army_group_step(&mut state, &tools, &plan, "l", later).unwrap(),
            None
        );
        let mut fresh = restarted(&state, later);
        assert!(fresh.done.contains_key("agrp:south") && fresh.done.contains_key("agfm:south"));
        assert_eq!(
            army_group_step(&mut fresh, &tools, &plan, "l", later).unwrap(),
            None
        );
        assert_eq!(actions(&tools).len(), 3);
    }

    #[test]
    fn an_army_group_never_regroups_or_reassigns_a_commander() {
        // one army already belongs to another group: never regrouped, the player is woken
        let tools = fake(vec![(
            "hoi4_armies",
            roster(
                &[("A", Some("Other"), false), ("B", None, false)],
                &[("Other", Some("X"))],
            ),
        )]);
        let plan =
            json!({"army_group": [{"id": "g", "armies": ["A", "B"], "marshal": "Manstein"}]});
        let mut state = State::default();
        assert_eq!(
            army_group_step(&mut state, &tools, &plan, "l", START).unwrap(),
            None
        );
        assert!(actions(&tools).is_empty());
        let alarms = take_alarms(&mut state);
        assert_eq!(alarms[0]["reason"], "army_group:g");
        // the group exists with a field marshal already: done, never reassigned
        tools.reply(
            "hoi4_armies",
            roster(
                &[("A", Some("HG"), false), ("B", Some("HG"), false)],
                &[("HG", Some("Bock"))],
            ),
        );
        assert_eq!(
            army_group_step(&mut state, &tools, &plan, "l", START + 8 * DAY).unwrap(),
            None
        );
        assert!(actions(&tools).is_empty());
        assert!(state.done.contains_key("agrp:g") && state.done.contains_key("agfm:g"));
        assert!(state.military.notes["army_group"].contains("led by Bock (never reassigned)"));
    }

    #[test]
    fn an_army_group_marshal_short_of_command_power_waits_and_the_ledger_holds() {
        let tools = fake(vec![(
            "hoi4_armies",
            roster(
                &[("A", Some("HG"), false), ("B", Some("HG"), false)],
                &[("HG", None)],
            ),
        )]);
        let plan = json!({"army_group": [{"id": "g", "armies": ["A", "B"], "marshal": "Rommel"}]});
        let mut state = State {
            clock: START,
            ..Default::default()
        };
        tools.refusing("hoi4_army_commander", "insufficient_command_power");
        assert!(army_group_step(&mut state, &tools, &plan, "l", START)
            .unwrap()
            .is_some());
        // waits 7 game days for command power
        for day in 1..7 {
            assert_eq!(
                army_group_step(&mut state, &tools, &plan, "l", START + day * DAY).unwrap(),
                None
            );
        }
        assert_eq!(actions(&tools).len(), 1);
        // a generic refusal 3 times in 10 days: the ledger stops sending it
        tools.refusing("hoi4_army_commander", "input_readback_failed");
        let mut sent = 1;
        for hour in [7 * DAY, 9 * DAY + 1, 11 * DAY + 2, 13 * DAY + 3] {
            state.clock = START + hour;
            army_group_step(&mut state, &tools, &plan, "l", START + hour).unwrap();
            sent = actions(&tools).len();
        }
        assert_eq!(
            sent, 4,
            "the order failed 3 times in 10 days: not sent again"
        );
    }

    #[test]
    fn the_tick_reaches_the_military_step() {
        let tools = fake(vec![(
            "hoi4_armies",
            roster(&[("A", None, false), ("B", None, false)], &[]),
        )]);
        let plan = json!({"army_group": [{"id": "g", "armies": ["A", "B"]}]});
        let mut state = State::default();
        let out = tick(&mut state, &tools, &plan, "l", START).unwrap();
        assert_eq!(
            out,
            Tick::Acted("army group g from A, B: true Verified".into())
        );
    }

    // --- W2-4 navy -------------------------------------------------------------------------

    fn fleets(tfs: &[(&str, &str, f64)], regions: &[&str], admiral: Option<&str>) -> Value {
        json!({"fleets": [
            {"fleet": "Squadra 1", "admiral": admiral, "home_port": {"state_id": 2},
             "assigned_regions": regions.iter().map(|r| json!({"key": r, "name": r})).collect::<Vec<_>>(),
             "task_forces": tfs.iter().map(|(t, m, s)| json!({"task_force": t, "strength_percent": s,
                "mission": {"name": m}})).collect::<Vec<_>>()},
            {"fleet": "Squadra 2", "admiral": "Cavagnari", "home_port": {"state_id": 9},
             "assigned_regions": [], "task_forces": []}]})
    }

    #[test]
    fn the_navy_adds_regions_then_gives_idle_task_forces_their_mission() {
        let tools = fake(vec![(
            "hoi4_fleets",
            fleets(
                &[("TF 1", "hold", 100.0), ("TF 2", "training", 100.0)],
                &[],
                Some("Bergamini"),
            ),
        )]);
        let plan = json!({"navy": {"posture": "coastal", "fleets": [{"fleet": "Squadra 1",
            "admiral": "Iachino",
            "missions": [{"id": "war", "mission": "strike_force", "regions": ["Central Mediterranean"],
                          "when": "date:2000.1.1"},
                         {"id": "peace", "mission": "patrol",
                          "regions": ["Central Mediterranean", {"region": "Ionian Sea", "state_id": 5}]}]}]}});
        let mut state = State::default();
        // the fleet has an admiral: never reassigned; the regions come first
        let out = navy_step(&mut state, &tools, &plan, "l", START).unwrap();
        assert_eq!(
            out.as_deref(),
            Some("navy: Squadra 1 region Central Mediterranean (from state 2): true Verified")
        );
        assert!(state.done.contains_key("nadm:Squadra 1"));
        let out = navy_step(&mut state, &tools, &plan, "l", START).unwrap();
        assert_eq!(
            out.as_deref(),
            Some("navy: Squadra 1 region Ionian Sea (from state 5): true Verified")
        );
        // the idle task force gets the planned mission, the training one once
        tools.reply(
            "hoi4_fleets",
            fleets(
                &[("TF 1", "hold", 100.0), ("TF 2", "training", 100.0)],
                &["Central Mediterranean", "Ionian Sea"],
                Some("Bergamini"),
            ),
        );
        let out = navy_step(&mut state, &tools, &plan, "l", START).unwrap();
        assert_eq!(
            out.as_deref(),
            Some("navy: TF 1 (Squadra 1) hold -> patrol: true Verified")
        );
        tools.reply(
            "hoi4_fleets",
            fleets(
                &[("TF 1", "patrol", 100.0), ("TF 2", "training", 100.0)],
                &["Central Mediterranean", "Ionian Sea"],
                Some("Bergamini"),
            ),
        );
        let out = navy_step(&mut state, &tools, &plan, "l", START).unwrap();
        assert_eq!(
            out.as_deref(),
            Some("navy: TF 2 (Squadra 1) training -> patrol: true Verified")
        );
        // the player puts TF 2 back on training: that stands; done -> every 3 game days
        tools.reply(
            "hoi4_fleets",
            fleets(
                &[("TF 1", "patrol", 100.0), ("TF 2", "training", 100.0)],
                &["Central Mediterranean", "Ionian Sea"],
                Some("Bergamini"),
            ),
        );
        assert_eq!(
            navy_step(&mut state, &tools, &plan, "l", START).unwrap(),
            None
        );
        assert_eq!(
            state.military.notes["navy"],
            "1 task force(s) on their planned mission"
        );
        assert_eq!(
            navy_step(&mut state, &tools, &plan, "l", START + DAY).unwrap(),
            None
        );
        // persisted: a restart repeats nothing
        let mut fresh = restarted(&state, START + 3 * DAY);
        assert_eq!(
            navy_step(&mut fresh, &tools, &plan, "l", START + 3 * DAY).unwrap(),
            None
        );
        let sent = actions(&tools);
        assert_eq!(sent.len(), 4);
        assert_eq!(sent[1].1["state_id"], 5);
        // posture none: nothing at all
        let none = json!({"navy": {"posture": "none", "reason": "landlocked"}});
        assert_eq!(
            navy_step(&mut fresh, &tools, &none, "l", START + 9 * DAY).unwrap(),
            None
        );
        assert_eq!(actions(&tools).len(), 4);
    }

    #[test]
    fn damaged_task_forces_go_to_port_and_return_to_their_mission_when_repaired() {
        let tools = fake(vec![(
            "hoi4_fleets",
            fleets(&[("TF 1", "patrol", 40.0)], &["Sea"], Some("A")),
        )]);
        let plan = json!({"navy": {"posture": "blue_water",
            "fleets": [{"fleet": "Squadra 1", "missions": [{"mission": "patrol", "regions": ["Sea"]}]}]}});
        let mut state = State::default();
        let out = navy_step(&mut state, &tools, &plan, "l", START).unwrap();
        assert_eq!(
            out.as_deref(),
            Some("navy: TF 1 at 40% strength to port (hold): true Verified")
        );
        assert_eq!(actions(&tools)[0].1["mission"], "hold");
        // in port, still repairing at 70%: left alone
        tools.reply(
            "hoi4_fleets",
            fleets(&[("TF 1", "hold", 70.0)], &["Sea"], Some("A")),
        );
        assert_eq!(
            navy_step(&mut state, &tools, &plan, "l", START + 3 * DAY).unwrap(),
            None
        );
        // repaired: idle again, so it gets its mission back
        tools.reply(
            "hoi4_fleets",
            fleets(&[("TF 1", "hold", 95.0)], &["Sea"], Some("A")),
        );
        let out = navy_step(&mut state, &tools, &plan, "l", START + 6 * DAY).unwrap();
        assert_eq!(
            out.as_deref(),
            Some("navy: TF 1 (Squadra 1) hold -> patrol: true Verified")
        );
        assert_eq!(actions(&tools).len(), 2);
    }

    #[test]
    fn an_admiral_is_assigned_once_and_never_moved_from_another_fleet() {
        let tools = fake(vec![("hoi4_fleets", fleets(&[], &[], None))]);
        let plan = json!({"navy": {"posture": "coastal",
            "admirals": {"Squadra 1": "Cavagnari"}}});
        let mut state = State::default();
        assert_eq!(
            navy_step(&mut state, &tools, &plan, "l", START).unwrap(),
            None
        );
        assert!(actions(&tools).is_empty(), "Cavagnari leads Squadra 2");
        assert_eq!(take_alarms(&mut state)[0]["reason"], "admiral:Squadra 1");
        let plan = json!({"navy": {"posture": "coastal", "admirals": [{"fleet": "squadra 1", "admiral": "Iachino"}]}});
        let mut state = State::default();
        let out = navy_step(&mut state, &tools, &plan, "l", START).unwrap();
        assert_eq!(
            out.as_deref(),
            Some("navy: admiral Iachino -> Squadra 1: true Verified")
        );
        assert_eq!(
            navy_step(&mut state, &tools, &plan, "l", START).unwrap(),
            None
        );
        assert_eq!(actions(&tools).len(), 1);
    }

    #[test]
    fn convoy_escort_comes_before_the_fleet_missions_when_it_holds() {
        let tools = fake(vec![(
            "hoi4_fleets",
            fleets(&[("TF 1", "hold", 100.0)], &["Red Sea"], Some("A")),
        )]);
        let plan = json!({"navy": {"posture": "coastal",
            "fleets": [{"fleet": "Squadra 1", "missions": [{"mission": "patrol", "regions": ["Red Sea"]}]}],
            "convoy_escort": {"fleet": "Squadra 1", "regions": ["Red Sea"]}}});
        let mut state = State::default();
        let out = navy_step(&mut state, &tools, &plan, "l", START).unwrap();
        assert_eq!(
            out.as_deref(),
            Some("navy: TF 1 (Squadra 1) hold -> convoy_escort: true Verified")
        );
    }

    // --- W2-9 casualty ratio ----------------------------------------------------------------

    fn stats(own: f64, enemy: f64) -> Value {
        json!({"player": {"tag": "ITA"}, "wars": [{"includes_player": true, "relations": [
            {"attributed": true, "countries": [{"tag": "ITA", "casualties": own},
                                               {"tag": "ETH", "casualties": enemy}]}]}]})
    }

    #[test]
    fn a_bad_casualty_ratio_rests_the_front_armies_and_wakes_with_the_numbers() {
        let tools = fake(vec![
            ("hoi4_war_stats", stats(1000.0, 1000.0)),
            (
                "hoi4_armies",
                roster(
                    &[
                        ("1. Armata", None, true),
                        ("Guard", None, false),
                        ("Home", None, false),
                    ],
                    &[],
                ),
            ),
        ]);
        let plan = json!({"manage": {"casualty_ratio_max": 2.0}});
        let mut state = State::default();
        state.war_guard.insert("Guard".into(), 5);
        assert_eq!(
            casualty_rule(&mut state, &tools, &plan, "l", START).unwrap(),
            None
        );
        assert!(state.military.notes["casualty_ratio"].contains("collecting"));
        // 14 days later: 7000 own vs 2000 enemy = 3.5 > 2
        tools.reply("hoi4_war_stats", stats(8000.0, 3000.0));
        let at = START + 14 * DAY;
        let out = casualty_rule(&mut state, &tools, &plan, "l", at).unwrap();
        assert_eq!(
            out.as_deref(),
            Some("casualty ratio 3.50 against ETH: 1. Armata rests 7 days, plan stopped: true Verified")
        );
        let sent = actions(&tools);
        assert_eq!(sent[0].1["execute"], false);
        let alarms = take_alarms(&mut state);
        assert_eq!(alarms[0]["rule"], 28);
        assert_eq!(alarms[0]["evidence"]["own_losses"], 7000.0);
        assert_eq!(alarms[0]["evidence"]["enemy_losses"], 2000.0);
        assert_eq!(alarms[0]["evidence"]["armies"], json!(["1. Armata"]));
        assert_eq!(
            state.war_cool["1. Armata"],
            at + 7 * DAY,
            "the war step leaves it alone"
        );
        // not repeated (the guard and the idle home army are not on the front)
        assert_eq!(
            casualty_rule(&mut state, &tools, &plan, "l", at).unwrap(),
            None
        );
        assert_eq!(
            casualty_rule(&mut state, &tools, &plan, "l", at + DAY).unwrap(),
            None
        );
        assert_eq!(actions(&tools).len(), 1);
        // the rest survives a restart and ends after 7 days: the plan executes again
        tools.reply("hoi4_armies", roster(&[("1. Armata", None, false)], &[]));
        let mut fresh = restarted(&state, at + 2 * DAY);
        assert_eq!(
            step(&mut fresh, &tools, &plan, "l", at + 2 * DAY).unwrap(),
            None
        );
        assert_eq!(
            fresh.war_cool["1. Armata"],
            at + 7 * DAY,
            "held again after the restart"
        );
        let out = rest_step(&mut fresh, &tools, "l", at + 7 * DAY).unwrap();
        assert_eq!(
            out.as_deref(),
            Some("rest over (cas): 1. Armata executes its plan again: true Verified")
        );
        assert_eq!(actions(&tools)[1].1["execute"], true);
        // judged again only a full window after it resumed
        tools.reply("hoi4_armies", roster(&[("1. Armata", None, true)], &[]));
        assert_eq!(
            casualty_rule(&mut fresh, &tools, &plan, "l", at + 8 * DAY).unwrap(),
            None
        );
        assert_eq!(actions(&tools).len(), 2);
    }

    #[test]
    fn small_or_good_casualty_ratios_rest_nothing_and_the_rule_can_be_off() {
        let tools = fake(vec![
            ("hoi4_war_stats", stats(0.0, 0.0)),
            ("hoi4_armies", roster(&[("A", None, true)], &[])),
        ]);
        let plan = json!({"manage": {}});
        let mut state = State::default();
        casualty_rule(&mut state, &tools, &plan, "l", START).unwrap();
        tools.reply("hoi4_war_stats", stats(900.0, 100.0));
        assert_eq!(
            casualty_rule(&mut state, &tools, &plan, "l", START + 14 * DAY).unwrap(),
            None,
            "below casualty_min_losses"
        );
        tools.reply("hoi4_war_stats", stats(5000.0, 4000.0));
        assert_eq!(
            casualty_rule(&mut state, &tools, &plan, "l", START + 15 * DAY).unwrap(),
            None,
            "1.25 <= 2.0"
        );
        let quiet = json!({"manage": {"off": ["casualties"]}});
        tools.reply("hoi4_war_stats", stats(90000.0, 100.0));
        assert_eq!(
            casualty_rule(&mut state, &tools, &quiet, "l", START + 20 * DAY).unwrap(),
            None
        );
        assert!(actions(&tools).is_empty());
    }

    // --- W2-5 air superiority --------------------------------------------------------------

    fn air() -> Value {
        json!({"wings": [
            {"wing": "CAS 1", "aircraft_type": "small_plane_cas_airframe_1", "missions": ["cas"],
             "region": {"id": 273, "name": "Ethiopia"}, "standing_by": false},
            {"wing": "Planned", "aircraft_type": "small_plane_airframe_1", "missions": [],
             "region": null, "standing_by": true},
            {"wing": "Caccia 1", "aircraft_type": "fighter_equipment_0", "missions": ["interception"],
             "region": {"id": 10, "name": "Italy"}, "standing_by": false},
            {"wing": "Caccia 2", "aircraft_type": "fighter_equipment_0", "missions": [],
             "region": null, "standing_by": true}]})
    }

    #[test]
    fn without_a_superiority_read_the_air_rule_is_unknown_and_acts_on_nothing() {
        let tools = fake(vec![
            (
                "hoi4_air_regions",
                json!({"regions": [{"region_id": 273, "name": "Ethiopia"}]}),
            ),
            ("hoi4_air", air()),
        ]);
        let plan = json!({"manage": {}});
        let mut state = State::default();
        assert_eq!(
            air_superiority_rule(&mut state, &tools, &plan, "l", START).unwrap(),
            None
        );
        assert!(state.military.notes["air_superiority"].starts_with("unknown"));
        assert!(actions(&tools).is_empty());
        assert!(take_alarms(&mut state).is_empty());
    }

    #[test]
    fn enemy_air_superiority_over_the_front_sends_fighters_and_holds_offensives() {
        let tools = fake(vec![
            (
                "hoi4_air_regions",
                json!({"regions": [{"region_id": 273, "name": "Ethiopia", "enemy_air_superiority": 75},
                                   {"region_id": 10, "name": "Italy", "enemy_air_superiority": 0.0}]}),
            ),
            ("hoi4_air", air()),
            ("hoi4_armies", roster(&[("1. Armata", None, true)], &[])),
        ]);
        let plan = json!({"manage": {"enemy_air_superiority": 0.6},
            "air_plan": [{"wing": "Planned", "mission": "air_superiority", "region": 10}]});
        let mut state = State::default();
        // standing-by fighters first; the air plan's wing is never taken
        let out = air_superiority_rule(&mut state, &tools, &plan, "l", START).unwrap();
        assert_eq!(
            out.as_deref(),
            Some("air superiority: enemy 75% over Ethiopia: Caccia 2 air_superiority there: true Verified")
        );
        let sent = actions(&tools);
        assert_eq!(sent[0].1["region"], "273");
        let out = air_superiority_rule(&mut state, &tools, &plan, "l", START).unwrap();
        assert!(out.unwrap().contains("Caccia 1 air_superiority there"));
        assert_eq!(take_alarms(&mut state)[0]["rule"], 27);
        // no fighter left: the only front region's offensive holds
        let out = air_superiority_rule(&mut state, &tools, &plan, "l", START).unwrap();
        assert_eq!(
            out.as_deref(),
            Some("air superiority: enemy 75% over Ethiopia: 1. Armata rests 10 days, plan stopped: true Verified")
        );
        assert_eq!(
            air_superiority_rule(&mut state, &tools, &plan, "l", START).unwrap(),
            None
        );
        assert_eq!(actions(&tools).len(), 3);
        // superiority back below the limit: the hold is lifted and the plan resumes
        tools.reply(
            "hoi4_air_regions",
            json!({"regions": [{"region_id": 273, "name": "Ethiopia", "enemy_air_superiority": 0.3}]}),
        );
        tools.reply("hoi4_armies", roster(&[("1. Armata", None, false)], &[]));
        let at = START + 3 * DAY;
        assert_eq!(
            air_superiority_rule(&mut state, &tools, &plan, "l", at).unwrap(),
            None
        );
        let out = rest_step(&mut state, &tools, "l", at).unwrap();
        assert_eq!(
            out.as_deref(),
            Some("rest over (air): 1. Armata executes its plan again: true Verified")
        );
    }

    // --- W2-11 naval invasions ---------------------------------------------------------------

    fn landing(controller: &str) -> Value {
        json!({"states": [{"state_id": 120, "controller": "ENG",
            "provinces": [{"province": 3320, "controller": controller}, {"province": 3321, "controller": "ENG"}]}]})
    }
    fn invader(state_id: i64, executing: bool) -> Value {
        json!({"armies": [{"army": "Seelöwe", "main_state": state_id, "plan_executing": executing,
            "battle_plan_orders": 1, "division_count": 4}]})
    }

    #[test]
    fn a_naval_invasion_stages_orders_prepares_executes_and_lands() {
        let tools = fake(vec![
            ("hoi4_overview", json!({"player": "GER"})),
            ("hoi4_armies", invader(50, false)),
            ("hoi4_states", landing("ENG")),
            (
                "hoi4_fleets",
                json!({"fleets": [{"fleet": "Hochseeflotte", "task_forces": [
                {"task_force": "TF 1", "mission": {"name": "hold"}, "strength_percent": 100.0},
                {"task_force": "TF 2", "mission": {"name": "strike_force"}, "strength_percent": 100.0}]}]}),
            ),
        ]);
        let plan = json!({
            "invasions": [
                {"id": "sealion", "from_state": 59, "target_state": 120, "target_province": 3320,
                 "armies": ["Seelöwe"], "fleet": "Hochseeflotte", "when": "at_war_with:ENG",
                 "prepare_days": 10},
                {"id": "second", "from_state": 59, "target_state": 121, "armies": ["Other"],
                 "when": "date:1936.1.1"}],
            "navy": {"posture": "blue_water",
                     "fleets": [{"fleet": "Hochseeflotte", "missions": [{"mission": "patrol"}]}]}});
        let mut state = State::default();
        // not at war: the when is unknown/false, nothing starts
        assert_eq!(
            invasion_step(&mut state, &tools, &plan, "l", START).unwrap(),
            None
        );
        assert!(actions(&tools).is_empty());
        let plan_at_war = {
            let mut p = plan.clone();
            p["invasions"][0]["when"] = json!("");
            p
        };
        let plan = plan_at_war;
        // the fleet supports first (TF 2 is already on strike force), then the army stages
        let out = invasion_step(&mut state, &tools, &plan, "l", START).unwrap();
        assert_eq!(
            out.as_deref(),
            Some("invasion sealion: TF 1 invasion_support: true Verified")
        );
        assert_eq!(
            state.war_cool["Seelöwe"],
            START + 2 * DAY,
            "the war step leaves it alone"
        );
        // the navy step leaves the invasion fleet alone while it runs
        assert_eq!(
            navy_step(&mut state, &tools, &plan, "l", START).unwrap(),
            None
        );
        let out = invasion_step(&mut state, &tools, &plan, "l", START).unwrap();
        assert_eq!(
            out.as_deref(),
            Some("invasion sealion: Seelöwe to the port state 59: true Verified")
        );
        assert_eq!(
            invasion_step(&mut state, &tools, &plan, "l", START + DAY).unwrap(),
            None
        );
        // in the port state: the order
        tools.reply("hoi4_armies", invader(59, false));
        let out = invasion_step(&mut state, &tools, &plan, "l", START + 2 * DAY).unwrap();
        assert_eq!(
            out.as_deref(),
            Some("invasion sealion: Seelöwe from 59 to state 120: true Verified")
        );
        let sent = actions(&tools);
        assert_eq!(sent[2].0, "hoi4_naval_invasion");
        assert_eq!(sent[2].1["state_id"], 120);
        assert_eq!(sent[2].1["source_state_id"], 59);
        // preparing: nothing for 10 days; then execute, refused while not ready, 5 days apart
        let ordered = START + 2 * DAY;
        assert_eq!(
            invasion_step(&mut state, &tools, &plan, "l", ordered + 9 * DAY).unwrap(),
            None
        );
        tools.refusing("hoi4_army_plan", "plan_not_executable");
        let out = invasion_step(&mut state, &tools, &plan, "l", ordered + 10 * DAY).unwrap();
        assert!(out.unwrap().contains("executes: false plan_not_executable"));
        assert_eq!(
            invasion_step(&mut state, &tools, &plan, "l", ordered + 12 * DAY).unwrap(),
            None
        );
        tools.allow();
        let out = invasion_step(&mut state, &tools, &plan, "l", ordered + 15 * DAY).unwrap();
        assert_eq!(
            out.as_deref(),
            Some("invasion sealion: Seelöwe executes: true Verified")
        );
        // persisted across a restart; landed when the province is ours
        let mut fresh = restarted(&state, ordered + 16 * DAY);
        assert_eq!(
            invasion_step(&mut fresh, &tools, &plan, "l", ordered + 16 * DAY).unwrap(),
            None
        );
        tools.reply("hoi4_states", landing("GER"));
        assert_eq!(
            invasion_step(&mut fresh, &tools, &plan, "l", ordered + 17 * DAY).unwrap(),
            None
        );
        assert!(fresh.done.contains_key("inv:sealion:landed"));
        // support, move, order, one refused execute, execute: nothing repeated
        assert_eq!(actions(&tools).len(), 5);
        assert!(
            !fresh.war_cool.contains_key("Seelöwe"),
            "released after the landing"
        );
        // the next invasion is judged only now (one at a time); its date is not reached
        invasion_step(&mut fresh, &tools, &plan, "l", ordered + 18 * DAY).unwrap();
        assert!(!fresh.done.contains_key("inv:second:start"));
        assert_eq!(fresh.military.notes["invasion:second"], "waits: when false");
    }

    #[test]
    fn a_refused_invasion_order_aborts_and_wakes_once() {
        let tools = fake(vec![
            ("hoi4_overview", json!({"player": "GER"})),
            ("hoi4_armies", invader(59, false)),
            ("hoi4_states", landing("ENG")),
        ]);
        tools.refusing("hoi4_naval_invasion", "not_coastal");
        let plan = json!({"invasions": [
            {"id": "a", "from_state": 59, "target_state": 120, "armies": ["Seelöwe"]},
            {"id": "b", "from_state": 59, "target_state": 120, "armies": ["Seelöwe"],
             "when": "date:2000.1.1"}]});
        let mut state = State::default();
        let out = invasion_step(&mut state, &tools, &plan, "l", START).unwrap();
        assert!(out.unwrap().contains("false not_coastal"));
        assert!(state.done.contains_key("inv:a:aborted"));
        assert!(
            !state.war_cool.contains_key("Seelöwe"),
            "the armies are released"
        );
        let alarms = take_alarms(&mut state);
        assert_eq!(alarms.len(), 1);
        assert_eq!(alarms[0]["reason"], "invasion:a");
        assert_eq!(alarms[0]["rule"], "H16");
        // never repeated; the next one waits for its date
        assert_eq!(
            invasion_step(&mut state, &tools, &plan, "l", START + DAY).unwrap(),
            None
        );
        assert_eq!(actions(&tools).len(), 1);
        // transports missing: waits, then aborts after stage_days
        let plan = json!({"invasions": [{"id": "c", "from_state": 59, "target_state": 120,
            "armies": ["Seelöwe"], "stage_days": 4}]});
        tools.refusing("hoi4_naval_invasion", "no_transports");
        let mut state = State::default();
        let out = invasion_step(&mut state, &tools, &plan, "l", START).unwrap();
        assert!(out.unwrap().contains("no_transports"));
        assert!(!state.done.contains_key("inv:c:aborted"));
        invasion_step(&mut state, &tools, &plan, "l", START + 2 * DAY).unwrap();
        assert!(!state.done.contains_key("inv:c:aborted"));
        invasion_step(&mut state, &tools, &plan, "l", START + 5 * DAY).unwrap();
        assert!(state.done.contains_key("inv:c:aborted"));
        assert_eq!(take_alarms(&mut state)[0]["reason"], "invasion:c");
    }

    #[test]
    fn invasion_when_is_a_plan_predicate() {
        let tools = fake(vec![]);
        let plan = json!({"invasions": [{"id": "x", "from_state": 1, "target_state": 2,
            "armies": ["A"], "when": "date:1936.6.1"}]});
        let mut state = State::default();
        let at = date_hour("1936.6.1").unwrap();
        assert_eq!(
            invasion_step(&mut state, &tools, &plan, "l", at - DAY).unwrap(),
            None
        );
        assert!(!state.done.contains_key("inv:x:start"));
        invasion_step(&mut state, &tools, &plan, "l", at).unwrap();
        assert!(state.done.contains_key("inv:x:start"));
    }

    // --- hold_targets (lesson L01) ---------------------------------------------------------

    /// A small war map: Eritrea (1: 101, 102) and home (4: 401) are Italian, Tigray (2: 201,
    /// 202) and Wello (3: 301) Ethiopian; a province's controller is its state's owner unless
    /// `holder` says otherwise. Moves are Verified, with the pre-check's "nothing was sent"
    /// note while `heading`, or refused with `refuse`.
    struct Corridor {
        calls: RefCell<Vec<(String, Value)>>,
        holder: RefCell<HashMap<i64, &'static str>>,
        armies: RefCell<Value>,
        heading: RefCell<bool>,
        refuse: RefCell<Option<&'static str>>,
    }
    const MAP: [(i64, &str, f64, &[i64]); 4] = [
        (1, "ITA", 0.0, &[101, 102]),
        (2, "ETH", 100.0, &[201, 202]),
        (3, "ETH", 200.0, &[301]),
        (4, "ITA", -100.0, &[401]),
    ];
    impl Tools for Corridor {
        fn call(&self, name: &str, args: &Value) -> Result<Value> {
            self.calls
                .borrow_mut()
                .push((name.to_string(), args.clone()));
            if args.get("request_id").is_some() {
                if name == "hoi4_army_move" {
                    if let Some(code) = *self.refuse.borrow() {
                        return Err(Error::new(code, "refused by the fake"));
                    }
                    if *self.heading.borrow() {
                        return Ok(json!({"receipt": {"status": "Verified", "detail": {
                            "note": "already heading or standing there: nothing was sent"}}}));
                    }
                }
                return Ok(json!({"receipt": {"status": "Verified"}}));
            }
            Ok(match name {
                "hoi4_overview" => json!({"player": "ITA"}),
                "hoi4_armies" => self.armies.borrow().clone(),
                "hoi4_states" => self.states(text(&args["query"])),
                _ => json!({}),
            })
        }
    }
    impl Corridor {
        fn new() -> Self {
            let c = Corridor {
                calls: RefCell::new(Vec::new()),
                holder: RefCell::new(HashMap::new()),
                armies: RefCell::new(json!({})),
                heading: RefCell::new(false),
                refuse: RefCell::new(None),
            };
            c.army(1, false);
            c
        }
        fn army(&self, main_state: i64, risk: bool) {
            *self.armies.borrow_mut() = json!({"armies": [{"army": "Guardia", "division_count": 8,
                "main_state": main_state, "plan_executing": false, "battle_plan_orders": 0,
                "threat": {"share": if risk { 0.8 } else { 0.1 }, "encirclement_risk": risk}}]});
        }
        fn take(&self, province: i64, tag: &'static str) {
            self.holder.borrow_mut().insert(province, tag);
        }
        fn controller(&self, province: i64, owner: &'static str) -> &'static str {
            self.holder
                .borrow()
                .get(&province)
                .copied()
                .unwrap_or(owner)
        }
        fn states(&self, query: &str) -> Value {
            let mut rows = Vec::new();
            for (id, owner, x, provinces) in MAP {
                let controller = self.controller(provinces[0], owner);
                let mut row = json!({"state_id": id, "owner": owner, "controller": controller,
                    "x": x, "z": 0.0});
                if query == id.to_string() {
                    row["provinces"] = provinces
                        .iter()
                        .map(|p| json!({"province": p, "controller": self.controller(*p, owner)}))
                        .collect();
                } else if query == owner || query == controller {
                    let others: Vec<Value> = provinces
                        .iter()
                        .filter(|p| self.controller(**p, owner) != owner)
                        .map(|p| json!({"province": p, "controller": self.controller(*p, owner)}))
                        .collect();
                    if !others.is_empty() {
                        row["provinces_held_by_others"] = json!(others);
                    }
                } else {
                    continue;
                }
                rows.push(row);
            }
            json!({"states": rows})
        }
        /// The moves sent (actions only), as their arguments.
        fn moves(&self) -> Vec<Value> {
            self.calls
                .borrow()
                .iter()
                .filter(|(n, a)| n == "hoi4_army_move" && a.get("request_id").is_some())
                .map(|(_, a)| a.clone())
                .collect()
        }
    }

    fn corridor_plan() -> Value {
        json!({"war": {"enemies": ["ETH"], "supply_first": false, "rest": false},
            "hold_targets": [{"id": "north", "army": "Guardia",
                "provinces": [101, 102, 201, 301]}]})
    }

    /// One hold step at game hour `now` (a fresh tick: its read cache is empty).
    fn hold_at(
        state: &mut State,
        tools: &dyn Tools,
        plan: &Value,
        now: GameHour,
    ) -> Option<String> {
        state.clock = now;
        state.pcache = None;
        hold_step(state, tools, plan, "l", now).unwrap()
    }

    fn war_at(state: &mut State, tools: &dyn Tools, plan: &Value, now: GameHour) -> Option<String> {
        state.clock = now;
        super::super::war_step(state, tools, plan, "l", now).unwrap()
    }

    #[test]
    fn a_hold_post_advances_as_the_corridor_provinces_are_taken() {
        let tools = Corridor::new();
        let plan = corridor_plan();
        let mut state = State::default();
        // Eritrea is held, Tigray not yet: the post is the last Eritrean province
        let out = hold_at(&mut state, &tools, &plan, START).unwrap();
        assert!(
            out.starts_with("hold north: Guardia -> province 102 (state 1)"),
            "{out}"
        );
        let first = &tools.moves()[0];
        assert_eq!(
            (&first["army"], &first["state_id"], &first["province_id"]),
            (&json!("Guardia"), &json!(1), &json!(102))
        );
        assert_eq!(state.war_guard.get("Guardia"), Some(&1));
        assert!(holding(&state, "Guardia"));
        // the provinces' states were learned once (map facts)
        assert_eq!(state.military.province_state.get(&301), Some(&3));
        // nothing changes within the day: no order
        assert_eq!(hold_at(&mut state, &tools, &plan, START + 6), None);
        // Tigray's province is taken: the post moves forward at once, not a day later
        tools.take(201, "ITA");
        let out = hold_at(&mut state, &tools, &plan, START + 8).unwrap();
        assert!(
            out.starts_with("hold north: Guardia -> province 201 (state 2)"),
            "{out}"
        );
        assert_eq!(tools.moves().len(), 2);
        assert_eq!(
            state.war_guard.get("Guardia"),
            Some(&2),
            "the binding follows the post"
        );
        tools.take(301, "ITA");
        hold_at(&mut state, &tools, &plan, START + 10).unwrap();
        assert_eq!(tools.moves()[2]["state_id"], 3);
        assert_eq!(tools.moves()[2]["province_id"], 301);
    }

    #[test]
    fn a_lost_province_behind_the_post_pulls_the_post_back() {
        let tools = Corridor::new();
        tools.take(201, "ITA");
        tools.take(301, "ITA");
        let plan = corridor_plan();
        let mut state = State::default();
        hold_at(&mut state, &tools, &plan, START).unwrap();
        assert_eq!(tools.moves()[0]["province_id"], 301);
        // Ethiopia retakes an Eritrean province behind the army: retaken first
        tools.take(102, "ETH");
        let out = hold_at(&mut state, &tools, &plan, START + 3).unwrap();
        assert!(
            out.contains("-> province 102 (state 1), province 102 behind the post is lost"),
            "{out}"
        );
        assert_eq!(tools.moves().len(), 2);
        // two lost behind the front: the rearmost first
        tools.take(201, "ETH");
        tools.take(101, "ETH");
        hold_at(&mut state, &tools, &plan, START + 5).unwrap();
        assert_eq!(tools.moves()[2]["province_id"], 101);
        // everything lost: no post, the player is woken
        tools.take(301, "ETH");
        tools.take(102, "ETH");
        assert_eq!(hold_at(&mut state, &tools, &plan, START + 7), None);
        assert_eq!(tools.moves().len(), 3);
        let alarms = take_alarms(&mut state);
        assert_eq!(alarms[0]["reason"], "hold_lost:north");
    }

    #[test]
    fn a_move_the_pre_check_reports_under_way_is_not_sent_again() {
        let tools = Corridor::new();
        let plan = corridor_plan();
        let mut state = State::default();
        hold_at(&mut state, &tools, &plan, START).unwrap();
        assert_eq!(tools.moves().len(), 1);
        // a day later the sent move is checked once more: the pre-check says it is under way
        *tools.heading.borrow_mut() = true;
        assert_eq!(
            hold_at(&mut state, &tools, &plan, START + DAY),
            None,
            "no input: no action"
        );
        assert_eq!(tools.moves().len(), 2);
        assert!(state.military.notes["hold:north"].contains("already heading or standing there"));
        // then never again while the post stands, whatever the days
        for day in 2..9 {
            hold_at(&mut state, &tools, &plan, START + day * DAY);
        }
        // a listed province ahead of the post changing hands is looked at at once, but the
        // post stands: still not sent again
        tools.take(301, "ENG");
        hold_at(&mut state, &tools, &plan, START + 9 * DAY + 1);
        assert_eq!(
            state.military.hold_seen["north"],
            "101:ITA 102:ITA 201:ETH 301:ENG "
        );
        assert_eq!(tools.moves().len(), 2);
        // a new post is ordered again
        tools.take(201, "ITA");
        hold_at(&mut state, &tools, &plan, START + 9 * DAY + 3);
        assert_eq!(tools.moves().len(), 3);
        assert_eq!(tools.moves()[2]["province_id"], 201);
    }

    #[test]
    fn the_failure_ledger_stops_a_refused_hold_move_and_wakes_the_player() {
        let tools = Corridor::new();
        *tools.refuse.borrow_mut() = Some("input_readback_failed");
        let plan = corridor_plan();
        let mut state = State::default();
        for day in 0..3 {
            let out = hold_at(&mut state, &tools, &plan, START + day * DAY).unwrap();
            assert!(out.contains("false input_readback_failed"), "{out}");
        }
        assert_eq!(tools.moves().len(), 3);
        assert!(take_alarms(&mut state).is_empty());
        // three failures in ten game days: never sent again, the player is woken once
        for day in 3..8 {
            assert_eq!(hold_at(&mut state, &tools, &plan, START + day * DAY), None);
        }
        assert_eq!(tools.moves().len(), 3);
        let alarms = take_alarms(&mut state);
        assert_eq!(alarms.len(), 1);
        assert_eq!(alarms[0]["reason"], "hold:north");
        assert!(alarms[0]["message"]
            .as_str()
            .unwrap()
            .contains("failed 3 times"));
    }

    #[test]
    fn the_war_step_never_retasks_a_held_army() {
        // without a hold the war step sends the army at the nearest Ethiopian state
        let tools = Corridor::new();
        let plan = corridor_plan();
        let mut free = State::default();
        let out = war_at(&mut free, &tools, &json!({"war": plan["war"]}), START).unwrap();
        assert!(out.starts_with("war front Guardia -> state 2"), "{out}");
        // held: no order for it, also when an own province is lost (no retake draw either)
        let tools = Corridor::new();
        let mut state = State::default();
        state.war_assign.insert("Guardia".into(), 2);
        state.war_stage.insert("Guardia".into(), 1);
        hold_at(&mut state, &tools, &plan, START).unwrap();
        assert!(state.war_assign.is_empty() && state.war_stage.is_empty());
        let before = tools.calls.borrow().len();
        assert_eq!(war_at(&mut state, &tools, &plan, START + 1), None);
        tools.take(101, "ETH");
        assert_eq!(war_at(&mut state, &tools, &plan, START + 2), None);
        let sent: Vec<(String, Value)> = tools.calls.borrow()[before..]
            .iter()
            .filter(|(_, a)| a.get("request_id").is_some())
            .cloned()
            .collect();
        assert!(sent.is_empty(), "{sent:?}");
        assert!(!state.war_assign.contains_key("Guardia"));
    }

    #[test]
    fn until_releases_the_army_back_to_its_guard_post() {
        let tools = Corridor::new();
        let t0 = date_hour("1936.5.1").unwrap();
        let plan = json!({"war": {"enemies": ["ETH"], "supply_first": false, "rest": false},
            "army_layout": [{"name_hint": "guard_eritrea", "state": 4, "role": "guard",
                "templates": ["Fanteria"], "count": 2, "guard_states_in_order": [4]}],
            "hold_targets": [{"id": "north", "army": "guard_eritrea",
                "provinces": [101, 102, 201], "when": "date:1936.5.1", "until": "date:1936.6.1"}]});
        let mut state = State::default();
        state
            .layout
            .insert("guard_eritrea".into(), "Guardia".into());
        state.war_guard.insert("Guardia".into(), 4);
        state.war_stage.insert("guard:Guardia".into(), 2);
        // before `when`: not held, the guard post stands
        assert_eq!(hold_at(&mut state, &tools, &plan, t0 - DAY), None);
        assert!(!holding(&state, "Guardia"));
        hold_at(&mut state, &tools, &plan, t0).unwrap();
        assert_eq!(
            state.war_guard.get("Guardia"),
            Some(&1),
            "the hold replaces the guard post"
        );
        assert_eq!(war_at(&mut state, &tools, &plan, t0 + 1), None);
        // `until` holds: back to the guard post, which the war step orders at once
        let t1 = date_hour("1936.6.1").unwrap();
        assert_eq!(hold_at(&mut state, &tools, &plan, t1), None);
        assert!(!holding(&state, "Guardia"));
        assert_eq!(state.war_guard.get("Guardia"), Some(&4));
        assert!(state.done.contains_key("hold:north:released"));
        assert!(state.military.notes["hold:north"].contains("back to its guard post, state 4"));
        let out = war_at(&mut state, &tools, &plan, t1).unwrap();
        assert!(
            out.starts_with("war guard move Guardia -> state 4"),
            "{out}"
        );
        // over for good, also after a restart
        let mut fresh = restarted(&state, t1 + DAY);
        assert_eq!(hold_at(&mut fresh, &tools, &plan, t1 + DAY), None);
        assert!(!holding(&fresh, "Guardia"));
        assert_eq!(tools.moves().len(), 2);
    }

    #[test]
    fn a_held_army_in_a_pocket_is_left_to_the_war_steps_fall_back() {
        let tools = Corridor::new();
        let plan = corridor_plan();
        let mut state = State::default();
        tools.army(1, true);
        // at risk of encirclement: bound, but no hold order
        assert_eq!(hold_at(&mut state, &tools, &plan, START), None);
        assert!(tools.moves().is_empty());
        assert!(holding(&state, "Guardia"));
        assert!(state.military.notes["hold:north"].contains("pocket fall-back owns it"));
        // the war step's fall-back moves it to the nearest fully held own state
        let out = war_at(&mut state, &tools, &plan, START + 1).unwrap();
        assert!(out.starts_with("war pocket Guardia"), "{out}");
        assert_eq!(tools.moves()[0]["state_id"], 4);
        assert!(tools.moves()[0].get("province_id").is_none());
        // while it falls back the hold sends nothing
        tools.army(4, false);
        for hour in [DAY, 2 * DAY] {
            assert_eq!(hold_at(&mut state, &tools, &plan, START + hour), None);
        }
        assert_eq!(tools.moves().len(), 1);
        // the fall-back is over: the post is ordered again
        let out = hold_at(&mut state, &tools, &plan, START + 4 * DAY).unwrap();
        assert!(
            out.starts_with("hold north: Guardia -> province 102"),
            "{out}"
        );
    }

    #[test]
    fn the_tick_binds_holds_before_the_war_step() {
        let tools = Corridor::new();
        let plan = corridor_plan();
        let mut state = State::default();
        let out = tick(&mut state, &tools, &plan, "l", START).unwrap();
        assert!(
            matches!(&out, Tick::Acted(s) if s.starts_with("hold north: Guardia -> province 102")),
            "{out:?}"
        );
        // the next ticks leave the army to the hold: the war step sends it nowhere
        tick(&mut state, &tools, &plan, "l", START + 1).unwrap();
        assert_eq!(tools.moves().len(), 1);
        assert!(!tools
            .calls
            .borrow()
            .iter()
            .any(|(n, a)| a["army"] == "Guardia"
                && a.get("request_id").is_some()
                && n != "hoi4_army_move"));
    }
}
