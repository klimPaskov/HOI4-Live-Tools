//! War-step rules: the gates and thresholds the war step in plan_run.rs consults before it
//! orders a driven army (planning before an offensive, supply and organisation limits, the
//! pocket fall-back, the stall response, the bound pause after a captured supply node) and the
//! disband step's land-route guard. A child module of `plan_run` (declared there with
//! `#[path]`), like plan_military.rs: every order still goes through `State::act` and its
//! failure ledger; this module's memory (`State::war_rules`) is this server life's, like the
//! war step's own stages. All cadences are game time.
//!
//! Hooks in plan_run.rs: `war_step` reads [`Limits`] once per tick and feeds [`observe`] the
//! armies' organisation; [`drawn`] marks an offensive line, [`execute_gate`] holds its execute
//! order; [`captured`] starts a bound when the army's target falls; [`relocation`] and
//! [`second_direction`] are the stall response before the direct move; [`pocket_risk`] decides
//! `pocket_fall_back`; `disband_step` keeps only divisions [`home_land`] reaches; `run` adds
//! [`digest`] as `war`.
//!
//! Plan keys (all under `war`, all optional; percentages may be written 0.5 or 50):
//!   planning_wait      false: execute an offensive the tick after its line is drawn (the old
//!                      behaviour). Default on: after `hoi4_army_offensive` the execute order waits
//!                      until the army's planning bonus reaches "planning_share" (0.9) of
//!                      "planning_cap" (0.30, the game's base; techs, doctrines and traits raise it)
//!                      at "planning_gain" (0.02 a game day), at most "planning_max_days" (15).
//!                      The army's planning bonus is read (hoi4_armies planning.mean, the mean of
//!                      its divisions): it executes once the bonus reaches that share of the cap,
//!                      or once it has stopped rising for a game day (it stands at the army's own
//!                      cap, which techs, doctrines, traits and the HQ distance set and no tool
//!                      reads). Without the read the bonus is estimated from the game days since
//!                      the line was drawn and the digest says so; the 1.19 x0.8 without a deployed
//!                      HQ scales cap and speed alike, so the wait is the same. No wait when the
//!                      goal deadline is less than "planning_skip_days" (20) away. A rest resumes at
//!                      once (planning rebuilt while resting)
//!   supply_floor       50: an executing army below this supply percent rests (plan stopped), and no
//!                      army below it gets a new attack order (execute, front, offensive, direct
//!                      or stall move), except a move to retake an own state
//!   supply_hold        35: below it nothing attacks, own states included, and a rest does not
//!                      end at rest_max_days (attrition starts at 35%)
//!   supply_resume      70: a rested army resumes at or above it (with the organisation below)
//!   rest_below_pct     0.30 / resume_above_pct 0.80: rest below, resume at, this share of the
//!                      army's maximum organisation. No tool reads maximum organisation: it is the
//!                      highest organisation_mean (hoi4_supply) seen for the army this server life.
//!                      An absolute "rest_below" / "resume_above" in the plan replaces the share
//!                      (the old behaviour; rest_max_days 20 still resumes when supply allows)
//!   pocket_share       0.5: an army falls back when this share of the land provinces around it is
//!                      held by others (hoi4_armies threat.share) and it stands outside a fully held
//!                      own state; the game-side flag (encirclement_risk, 0.7) falls back anywhere.
//!                      The neck back to own land is not readable, so it is not judged
//!   second_direction   false: a stall goes straight to the direct move. Default: the first stall
//!                      on a target (no progress for stall_days) brings a second direction from
//!                      the target's own neighbours (hoi4_wars enemy_held_adjacent_states): an
//!                      idle driven army standing in another own state next to the target is sent
//!                      at it too (it draws its own front and offensive); with none, the stalled
//!                      army stops its plan, moves to the nearest other own state next to the
//!                      target and draws its front and offensive again from there (planning wait
//!                      included). The next stall on the same target is the direct move, with the
//!                      back-off. State adjacency is read, not province adjacency
//!   bound              false: no bound pause. Default: when a driven army's target falls and the
//!                      state holds an own supply hub or port (hoi4_supply supply_nodes, by the
//!                      provinces of hoi4_states) or railway, the army's next execute (or direct
//!                      move) waits until a captured node feeds the provinces where own divisions
//!                      stand (a flow above 0) or "bound_rail_days" (10; "bound_core_days" 5 in an
//!                      own state) have passed, and its supply is at the floor; at most
//!                      "bound_max_days" (12). Fronts and offensive lines are drawn meanwhile, so
//!                      the bound builds planning
//!
//! The disband guard has no key: `disband` takes a division only where the capital reaches it
//! over own-controlled land (hoi4_wars capital_land; overseas 75% of the equipment is lost) and
//! never one of an army at encirclement risk; without that read it disbands nothing.
use super::{list, text, GameHour, State, Tools, DAY};
use crate::error::Result;
use serde_json::{json, Value};
use std::collections::{BTreeMap, HashMap, HashSet};

/// What these rules keep (this server life).
#[derive(Default)]
pub(super) struct Memory {
    /// The last status line per rule and army (the run result's `war`).
    notes: BTreeMap<String, String>,
    /// army -> (target, game hour its offensive line was drawn)
    drawn: HashMap<String, (i32, GameHour)>,
    /// army -> highest organisation_mean seen (the maximum organisation estimate)
    peak: HashMap<String, f64>,
    /// army -> the bound pause after a captured supply node or railway
    bound: HashMap<String, Bound>,
    /// army -> the target a second direction was already tried for
    second: HashMap<String, i32>,
    /// army -> the move to another side of the target (stall response)
    via: HashMap<String, Via>,
    /// army -> (highest planning bonus read, game hour it was first read at that level)
    planning: HashMap<String, (f64, GameHour)>,
}

#[derive(Clone)]
struct Bound {
    state: i32,
    since: GameHour,
    /// provinces of own supply hubs and ports in the captured state
    nodes: Vec<i64>,
    rail: bool,
    core: bool,
}

#[derive(Clone, Copy)]
struct Via {
    target: i32,
    state: i32,
    /// the move was sent (else the plan was stopped and the move is next)
    moving: bool,
    at: GameHour,
}

/// An army was renamed: what is kept for it follows the name.
pub(super) fn army_renamed(memory: &mut Memory, old: &str, new: &str) {
    fn rekey<V>(map: &mut HashMap<String, V>, old: &str, new: &str) {
        if let Some(v) = map.remove(old) {
            map.insert(new.to_string(), v);
        }
    }
    rekey(&mut memory.drawn, old, new);
    rekey(&mut memory.peak, old, new);
    rekey(&mut memory.bound, old, new);
    rekey(&mut memory.second, old, new);
    rekey(&mut memory.via, old, new);
}

/// The last status of each rule (the run result's `war`), or None when nothing was said.
pub(super) fn digest(state: &State) -> Option<Value> {
    (!state.war_rules.notes.is_empty()).then(|| json!(state.war_rules.notes))
}

fn note(state: &mut State, key: &str, line: impl Into<String>) {
    state.war_rules.notes.insert(key.to_string(), line.into());
}

/// A share: 0.3 or 30 (percent) both mean 0.3.
fn share(v: &Value, default: f64) -> f64 {
    v.as_f64()
        .map_or(default, |f| if f > 1.0 { f / 100.0 } else { f })
}

/// A supply percentage: 50 or 0.5 both mean 50%.
fn percent(v: &Value, default: f64) -> f64 {
    v.as_f64()
        .map_or(default, |f| if f <= 1.0 { f * 100.0 } else { f })
}

fn days(v: &Value, default: f64) -> f64 {
    v.as_f64().filter(|d| *d >= 0.0).unwrap_or(default)
}

// ---------------------------------------------------------------------------------------------
// Organisation and supply limits.

/// The organisation and supply limits of the war step.
pub(super) struct Limits {
    rest_abs: Option<f64>,
    resume_abs: Option<f64>,
    rest_pct: f64,
    resume_pct: f64,
    supply_floor: f64,
    supply_hold: f64,
    supply_resume: f64,
}

impl Limits {
    pub(super) fn of(plan: &Value) -> Self {
        let cfg = &plan["war"];
        let supply_floor = percent(&cfg["supply_floor"], 50.0);
        let supply_hold = percent(&cfg["supply_hold"], 35.0).min(supply_floor);
        Self {
            rest_abs: cfg["rest_below"].as_f64(),
            resume_abs: cfg["resume_above"].as_f64(),
            rest_pct: share(&cfg["rest_below_pct"], 0.30),
            resume_pct: share(&cfg["resume_above_pct"], 0.80),
            supply_floor,
            supply_hold,
            supply_resume: percent(&cfg["supply_resume"], 70.0).max(supply_floor),
        }
    }
    fn line(&self, state: &State, army: &str, abs: Option<f64>, pct: f64) -> f64 {
        abs.unwrap_or_else(|| pct * state.war_rules.peak.get(army).copied().unwrap_or(0.0))
    }
    /// Organisation below the rest line.
    pub(super) fn tired(&self, state: &State, army: &str, org: f64) -> bool {
        org < self.line(state, army, self.rest_abs, self.rest_pct)
    }
    /// Organisation at or above the resume line.
    pub(super) fn fresh(&self, state: &State, army: &str, org: f64) -> bool {
        org >= self.line(state, army, self.resume_abs, self.resume_pct)
    }
    /// Supply below the floor: no new attack.
    pub(super) fn starving(&self, supply: f64) -> bool {
        supply < self.supply_floor
    }
    /// Supply below the hard hold: no attack at all.
    pub(super) fn held(&self, supply: f64) -> bool {
        supply < self.supply_hold
    }
    /// Supply good enough to resume after a rest.
    pub(super) fn supplied(&self, supply: f64) -> bool {
        supply >= self.supply_resume
    }
    pub(super) fn floor(&self) -> f64 {
        self.supply_floor
    }
}

/// Record each army's organisation: the highest value seen is its maximum estimate.
pub(super) fn observe(state: &mut State, condition: &HashMap<String, (f64, f64)>) {
    for (army, (org, _)) in condition {
        let peak = state.war_rules.peak.entry(army.clone()).or_insert(0.0);
        if *org > *peak {
            *peak = *org;
        }
    }
    if !condition.is_empty() {
        note(
            state,
            "rest",
            "organisation shares are of the highest organisation_mean seen per army (no maximum-organisation read)",
        );
    }
}

/// Say why an army gets no attack order now.
pub(super) fn held_note(state: &mut State, army: &str, why: String) {
    note(state, &format!("hold:{army}"), why);
}

// ---------------------------------------------------------------------------------------------
// Planning before execution, and the bound pause.

/// The army's offensive line into `target` was drawn now: its planning starts.
pub(super) fn drawn(state: &mut State, army: &str, target: i32, now: GameHour) {
    state
        .war_rules
        .drawn
        .insert(army.to_string(), (target, now));
}

/// Why the army's execute order waits (None: execute now): the bound pause, then planning.
/// `planning` is the army's planning bonus read now (hoi4_armies planning.mean), if any.
#[allow(clippy::too_many_arguments)]
pub(super) fn execute_gate(
    state: &mut State,
    plan: &Value,
    army: &str,
    target: i32,
    supply: Option<&Value>,
    supply_mean: Option<f64>,
    planning: Option<f64>,
    now: GameHour,
) -> Option<String> {
    if let Some(why) = bound_wait(state, plan, army, supply, supply_mean, now) {
        return Some(why);
    }
    planning_wait(state, plan, army, target, planning, now)
}

fn planning_wait(
    state: &mut State,
    plan: &Value,
    army: &str,
    target: i32,
    read: Option<f64>,
    now: GameHour,
) -> Option<String> {
    let cfg = &plan["war"];
    let key = format!("planning:{army}");
    if cfg["planning_wait"] == json!(false) {
        return None;
    }
    if let Some(deadline) = plan["goal"]["deadline"].as_str().and_then(super::date_hour) {
        let skip = days(&cfg["planning_skip_days"], 20.0);
        if ((deadline - now) as f64) < skip * DAY as f64 {
            note(
                state,
                &key,
                format!("no planning wait: the goal deadline is less than {skip} days away"),
            );
            return None;
        }
    }
    let cap = share(&cfg["planning_cap"], 0.30);
    let wanted = share(&cfg["planning_share"], 0.9);
    let gain = share(&cfg["planning_gain"], 0.02).max(1e-4);
    let max_days = days(&cfg["planning_max_days"], 15.0);
    let need = (wanted * cap / gain).min(max_days);
    let since = match state.war_rules.drawn.get(army) {
        Some((t, at)) if *t == target => *at,
        _ => {
            drawn(state, army, target, now);
            now
        }
    };
    let elapsed = (now - since) as f64 / DAY as f64;
    if let Some(bonus) = read {
        return planning_read(
            state,
            &key,
            army,
            target,
            bonus,
            cap * wanted,
            elapsed,
            max_days,
            now,
        );
    }
    if elapsed >= need {
        note(
            state,
            &key,
            format!("{army} executes on state {target} after {elapsed:.1} game days of planning (estimated)"),
        );
        return None;
    }
    let why = format!(
        "{army} plans its offensive on state {target}: {elapsed:.1} of {need:.1} game days, about {:.0}% of the {:.0}% cap (estimated from +{:.1}%/day: no tool reads planning)",
        (elapsed * gain).min(cap) * 100.0,
        cap * 100.0,
        gain * 100.0
    );
    note(state, &key, why.clone());
    Some(why)
}

/// The planning wait on the bonus read from the army: execute at `wanted`, once the bonus has
/// not risen for a game day (it stands at the army's own cap) or after `max_days`.
#[allow(clippy::too_many_arguments)]
fn planning_read(
    state: &mut State,
    key: &str,
    army: &str,
    target: i32,
    bonus: f64,
    wanted: f64,
    elapsed: f64,
    max_days: f64,
    now: GameHour,
) -> Option<String> {
    let seen = state
        .war_rules
        .planning
        .entry(army.to_string())
        .or_insert((bonus, now));
    if bonus > seen.0 + 0.001 {
        *seen = (bonus, now);
    }
    let flat_days = (now - seen.1) as f64 / DAY as f64;
    let pct = |v: f64| v * 100.0;
    let go = if bonus + 1e-9 >= wanted {
        Some(format!(
            "planning {:.0}% reached {:.0}%",
            pct(bonus),
            pct(wanted)
        ))
    } else if bonus > 0.0 && flat_days >= 1.0 {
        Some(format!(
            "planning stopped rising at {:.0}% for {flat_days:.1} game days (the army's cap)",
            pct(bonus)
        ))
    } else if elapsed >= max_days {
        Some(format!(
            "planning {:.0}% after the {max_days} game days at most",
            pct(bonus)
        ))
    } else {
        None
    };
    if let Some(go) = go {
        state.war_rules.planning.remove(army);
        note(
            state,
            key,
            format!("{army} executes on state {target}: {go} (read)"),
        );
        return None;
    }
    let why = format!(
        "{army} plans its offensive on state {target}: planning {:.0}% of {:.0}% wanted after {elapsed:.1} game days (read from hoi4_armies)",
        pct(bonus),
        pct(wanted)
    );
    note(state, key, why.clone());
    Some(why)
}

/// The army's target `old` fell to the player: a bound starts when the state holds an own supply
/// hub, port or railway. `mine` is hoi4_states for the player; `supply` hoi4_supply when read.
#[allow(clippy::too_many_arguments)]
pub(super) fn captured(
    state: &mut State,
    tools: &dyn Tools,
    plan: &Value,
    army: &str,
    old: i32,
    mine: &Value,
    me: &str,
    supply: Option<&Value>,
    now: GameHour,
) {
    let cfg = &plan["war"];
    if cfg["bound"] == json!(false) {
        return;
    }
    let Some(row) = list(mine, "states")
        .iter()
        .find(|r| r["state_id"].as_i64() == Some(i64::from(old)))
    else {
        return;
    };
    if text(&row["controller"]) != me {
        return;
    }
    let core = text(&row["owner"]) == me;
    let provinces: HashSet<i64> = tools
        .call("hoi4_states", &json!({"query": old.to_string()}))
        .ok()
        .and_then(|v| list(&v, "states").first().cloned())
        .map(|r| {
            list(&r, "provinces")
                .iter()
                .filter_map(|p| p["province"].as_i64())
                .collect()
        })
        .unwrap_or_default();
    let read;
    let supply = match supply {
        Some(s) => s,
        None => {
            read = tools.call("hoi4_supply", &json!({})).unwrap_or(Value::Null);
            &read
        }
    };
    let in_state = |p: &Value| {
        p["province_id"]
            .as_i64()
            .is_some_and(|id| provinces.contains(&id))
    };
    let nodes: Vec<i64> = list(supply, "supply_nodes")
        .iter()
        .filter(|n| in_state(n))
        .filter_map(|n| n["province_id"].as_i64())
        .collect();
    let rail = list(supply, "supply_nodes")
        .iter()
        .chain(list(supply, "provinces"))
        .any(|p| {
            (in_state(p) || p["state_id"].as_i64() == Some(i64::from(old)))
                && p["railway_level"].as_i64().unwrap_or(0) > 0
        });
    if nodes.is_empty() && !rail {
        return;
    }
    note(
        state,
        &format!("bound:{army}"),
        format!(
            "{army} took state {old} ({} supply node(s), railway {rail}): holds its next advance until the node feeds the front",
            nodes.len()
        ),
    );
    state.war_rules.bound.insert(
        army.to_string(),
        Bound {
            state: old,
            since: now,
            nodes,
            rail,
            core,
        },
    );
}

/// Why the army's advance waits after a captured supply node (None: no bound, or it is over).
pub(super) fn bound_wait(
    state: &mut State,
    plan: &Value,
    army: &str,
    supply: Option<&Value>,
    supply_mean: Option<f64>,
    now: GameHour,
) -> Option<String> {
    let b = state.war_rules.bound.get(army)?.clone();
    let cfg = &plan["war"];
    let key = format!("bound:{army}");
    let elapsed = (now - b.since) as f64 / DAY as f64;
    let max = days(&cfg["bound_max_days"], 12.0);
    let min = if b.core {
        days(&cfg["bound_core_days"], 5.0)
    } else {
        days(&cfg["bound_rail_days"], 10.0)
    };
    let connected = !b.nodes.is_empty()
        && supply.is_some_and(|s| {
            list(s, "provinces").iter().any(|p| {
                list(p, "flows").iter().any(|f| {
                    f["flow"].as_f64().unwrap_or(0.0) > 0.0
                        && f["node_province"]
                            .as_i64()
                            .is_some_and(|n| b.nodes.contains(&n))
                })
            })
        });
    let floor = Limits::of(plan).floor();
    let supplied = supply_mean.is_none_or(|s| s >= floor);
    if elapsed >= max || ((connected || elapsed >= min) && supplied) {
        state.war_rules.bound.remove(army);
        note(
            state,
            &key,
            format!(
                "{army} advances again {elapsed:.1} days after taking state {} (node connected {connected}, supply {})",
                b.state,
                supply_mean.map_or("unknown".into(), |s| format!("{s:.0}%"))
            ),
        );
        return None;
    }
    let why = format!(
        "{army} holds after taking state {} (supply node {}, railway {}): {elapsed:.1} of {min}-{max} days, node connected {connected}, supply {}",
        b.state,
        !b.nodes.is_empty(),
        b.rail,
        supply_mean.map_or("unknown".into(), |s| format!("{s:.0}%"))
    );
    note(state, &key, why.clone());
    Some(why)
}

// ---------------------------------------------------------------------------------------------
// Pocket fall-back.

/// Whether the army falls back from a forming pocket: the game-side flag (70%) anywhere, else
/// `war.pocket_share` (0.5) of the land around it held by others while it stands outside the
/// fully held own states (`safe`).
pub(super) fn pocket_risk(plan: &Value, army: &Value, safe: &[(i32, (f64, f64))]) -> bool {
    let threat = &army["threat"];
    if threat["encirclement_risk"] == json!(true) {
        return true;
    }
    let Some(held) = threat["share"].as_f64() else {
        return false;
    };
    let limit = share(&plan["war"]["pocket_share"], 0.5);
    let here = army["main_state"].as_i64();
    held >= limit && !safe.iter().any(|(id, _)| Some(i64::from(*id)) == here)
}

// ---------------------------------------------------------------------------------------------
// The stall response: a second direction before the direct move.

/// What a running move to another side of the target means for the war step this tick.
pub(super) enum Relocate {
    /// Nothing running.
    No,
    /// On its way: no other order.
    Wait,
    /// An order was sent.
    Acted(String),
}

/// The move to another side of the target (after the plan was stopped), then the wait for it.
#[allow(clippy::too_many_arguments)]
pub(super) fn relocation(
    state: &mut State,
    tools: &dyn Tools,
    lease: &str,
    army: &str,
    target: i32,
    here: Option<i32>,
    stall: GameHour,
    now: GameHour,
) -> Result<Relocate> {
    let Some(v) = state.war_rules.via.get(army).copied() else {
        return Ok(Relocate::No);
    };
    if v.target != target {
        state.war_rules.via.remove(army);
        return Ok(Relocate::No);
    }
    if !v.moving {
        let (ok, why) = state.act(
            tools,
            lease,
            "hoi4_army_move",
            json!({"army": army, "state_id": v.state}),
            "w2",
        )?;
        if ok {
            state.war_rules.via.insert(
                army.to_string(),
                Via {
                    moving: true,
                    at: now,
                    ..v
                },
            );
            state.war_stage.insert(army.to_string(), 0);
            state.war_move.remove(army);
        } else {
            state.war_rules.via.remove(army);
        }
        return Ok(Relocate::Acted(format!(
            "war second direction {army} -> state {} beside state {target}: {ok} {why}",
            v.state
        )));
    }
    if here == Some(v.state) {
        state.war_rules.via.remove(army);
        state.war_stage.insert(army.to_string(), 0);
        state.war_move.remove(army);
        state
            .war_since
            .insert(army.to_string(), (target, here, now));
        note(
            state,
            &format!("stall:{army}"),
            format!("{army} reached state {} beside state {target}: front and offensive are drawn from there", v.state),
        );
        return Ok(Relocate::No);
    }
    if now - v.at > 2 * stall.max(DAY) {
        state.war_rules.via.remove(army);
        return Ok(Relocate::No);
    }
    Ok(Relocate::Wait)
}

/// The first stall on a target: an idle army beside the target joins the attack, else the
/// stalled army goes round to another own state beside it. None: no second direction (tried
/// before, switched off or nothing beside the target): the war step sends the direct move.
#[allow(clippy::too_many_arguments)]
pub(super) fn second_direction(
    state: &mut State,
    tools: &dyn Tools,
    plan: &Value,
    lease: &str,
    roster: &Value,
    army: &str,
    target: i32,
    here: Option<i32>,
    executing: bool,
    flanks: &HashSet<String>,
    enemies: &[String],
    now: GameHour,
) -> Result<Option<String>> {
    let cfg = &plan["war"];
    if cfg["second_direction"] == json!(false)
        || cfg["direct_moves"] == json!(true)
        || state.war_rules.second.get(army) == Some(&target)
    {
        return Ok(None);
    }
    state.war_rules.second.insert(army.to_string(), target);
    let key = format!("stall:{army}");
    let mut args = json!({"max_states": 1100});
    let tags: Vec<&String> = enemies
        .iter()
        .filter(|t| {
            t.len() == 3
                && t.bytes()
                    .all(|b| b.is_ascii_uppercase() || b.is_ascii_digit())
        })
        .take(32)
        .collect();
    if !tags.is_empty() {
        args["enemies"] = json!(tags);
    }
    let beside: Vec<i32> = tools
        .call("hoi4_wars", &args)
        .ok()
        .and_then(|w| {
            list(&w, "enemy_held_adjacent_states")
                .iter()
                .find(|s| s["state_id"].as_i64() == Some(i64::from(target)))
                .map(|s| {
                    list(s, "adjacent_own_states")
                        .iter()
                        .filter_map(|v| v.as_i64().map(|v| v as i32))
                        .collect()
                })
        })
        .unwrap_or_default();
    if beside.is_empty() {
        note(
            state,
            &key,
            format!("{army} stalled on state {target}: no own state beside it is known (hoi4_wars): direct move"),
        );
        return Ok(None);
    }
    // 1. An idle army beside the target attacks it too.
    let flank = list(roster, "armies")
        .iter()
        .filter(|a| {
            let n = text(&a["army"]);
            n != army
                && flanks.contains(n)
                && a["plan_executing"] != json!(true)
                && !state.war_move.contains(n)
                && !state.war_rest.contains_key(n)
                && state.war_assign.get(n) != Some(&target)
                && a["main_state"]
                    .as_i64()
                    .is_some_and(|s| s as i32 != here.unwrap_or(-1) && beside.contains(&(s as i32)))
        })
        .max_by_key(|a| a["division_count"].as_u64().unwrap_or(0))
        .map(|a| {
            (
                text(&a["army"]).to_string(),
                a["main_state"].as_i64().unwrap_or(0),
            )
        });
    if let Some((other, from)) = flank {
        state.war_assign.insert(other.clone(), target);
        state.war_stage.remove(&other);
        state.war_move.remove(&other);
        state.war_since.remove(&other);
        state.war_rest.remove(&other);
        state.war_cool.remove(&other);
        state
            .war_since
            .insert(army.to_string(), (target, here, now));
        let line = format!(
            "war stalled {army} on state {target}: second direction, {other} attacks it from state {from}"
        );
        note(state, &key, line.clone());
        return Ok(Some(line));
    }
    // 2. Round to another own state beside the target.
    let position = here.and_then(|s| state.centers.get(&s).copied());
    let Some(via) = beside
        .iter()
        .copied()
        .filter(|s| Some(*s) != here)
        .min_by(|a, b| {
            let d = |s: &i32| {
                let (x, z) = state.centers.get(s).copied().unwrap_or((f64::MAX, 0.0));
                position.map_or(0.0, |(px, pz)| (x - px).powi(2) + (z - pz).powi(2))
            };
            d(a).total_cmp(&d(b))
        })
    else {
        note(
            state,
            &key,
            format!("{army} stalled on state {target}: no other own state beside it: direct move"),
        );
        return Ok(None);
    };
    if executing {
        let (ok, why) = state.act(
            tools,
            lease,
            "hoi4_army_plan",
            json!({"army": army, "execute": false}),
            "w2",
        )?;
        if ok {
            state.war_rules.via.insert(
                army.to_string(),
                Via {
                    target,
                    state: via,
                    moving: false,
                    at: now,
                },
            );
        }
        let line = format!(
            "war stalled {army} on state {target}: plan stopped to go round by state {via}: {ok} {why}"
        );
        note(state, &key, line.clone());
        return Ok(Some(line));
    }
    state.war_rules.via.insert(
        army.to_string(),
        Via {
            target,
            state: via,
            moving: false,
            at: now,
        },
    );
    match relocation(state, tools, lease, army, target, here, DAY, now)? {
        Relocate::Acted(line) => {
            note(state, &key, line.clone());
            Ok(Some(line))
        }
        _ => Ok(None),
    }
}

// ---------------------------------------------------------------------------------------------
// The disband guard.

/// Own states the capital reaches over own-controlled land (hoi4_wars `capital_land.states`);
/// None when the read does not say.
pub(super) fn home_land(state: &mut State, tools: &dyn Tools) -> Option<HashSet<i64>> {
    let read = tools.call("hoi4_wars", &json!({})).ok();
    let states = read
        .as_ref()
        .and_then(|r| r["capital_land"]["states"].as_array());
    match states {
        Some(s) => Some(s.iter().filter_map(Value::as_i64).collect()),
        None => {
            let why = read
                .as_ref()
                .and_then(|r| r["capital_land_reason"].as_str())
                .unwrap_or("hoi4_wars gave no capital_land");
            note(
                state,
                "disband",
                format!("disband waits: the land route to the capital is unknown ({why})"),
            );
            None
        }
    }
}

/// Armies at encirclement risk (hoi4_armies threat): never thinned by disbanding.
pub(super) fn encircled(tools: &dyn Tools) -> HashSet<String> {
    tools
        .call("hoi4_armies", &json!({}))
        .map(|r| {
            list(&r, "armies")
                .iter()
                .filter(|a| a["threat"]["encirclement_risk"] == json!(true))
                .map(|a| text(&a["army"]).to_string())
                .collect()
        })
        .unwrap_or_default()
}

/// The disband step's note on divisions it left because they stand overseas.
pub(super) fn overseas_note(state: &mut State, overseas: &[String]) {
    if !overseas.is_empty() {
        note(
            state,
            "disband",
            format!(
                "kept {} division(s) the capital does not reach by land (75% of their equipment would be lost): {}",
                overseas.len(),
                overseas.join(", ")
            ),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::super::{tick, Tick};
    use super::*;
    use std::cell::{Cell, RefCell};

    const START: GameHour = 1_000_000;

    /// One driven army (Army 1 in own state 42) against FRA (states 28 and 18); Army 2 idles in
    /// own state 43 beside 28. Supply, organisation, the executing flag and the threat are set
    /// by the test.
    struct W {
        executing: Cell<bool>,
        org: Cell<f64>,
        supply: Cell<f64>,
        share: Cell<f64>,
        here: Cell<i64>,
        taken: Cell<bool>,
        flow: Cell<f64>,
        flank: Cell<bool>,
        /// Army 1's planning bonus as hoi4_armies reads it (None: not read)
        planning: Cell<Option<f64>>,
        calls: RefCell<Vec<(String, Value)>>,
    }
    impl W {
        fn new() -> Self {
            Self {
                executing: Cell::new(false),
                org: Cell::new(50.0),
                supply: Cell::new(90.0),
                share: Cell::new(0.2),
                here: Cell::new(42),
                taken: Cell::new(false),
                flow: Cell::new(0.0),
                flank: Cell::new(false),
                planning: Cell::new(None),
                calls: RefCell::new(vec![]),
            }
        }
        fn actions(&self) -> Vec<(String, Value)> {
            self.calls
                .borrow()
                .iter()
                .filter(|(_, a)| a.get("request_id").is_some())
                .cloned()
                .collect()
        }
    }
    impl Tools for W {
        fn call(&self, name: &str, args: &Value) -> Result<Value> {
            self.calls
                .borrow_mut()
                .push((name.to_string(), args.clone()));
            if args.get("request_id").is_some() {
                return Ok(json!({"receipt": {"status": "Verified"}}));
            }
            let taken = self.taken.get();
            Ok(match name {
                "hoi4_research" => json!({"slots": [{"researching": "a"}]}),
                "hoi4_focus_completed" => json!({"active_key": "f"}),
                "hoi4_economy" => json!({"factory_trigger_count": {"military": 0}}),
                "hoi4_overview" => json!({"player": "GER"}),
                "hoi4_states" => match args["query"].as_str().unwrap() {
                    "GER" => {
                        let mut rows = vec![
                            json!({"state_id": 42, "owner": "GER", "controller": "GER", "x": 0.0, "z": 0.0}),
                            json!({"state_id": 43, "owner": "GER", "controller": "GER", "x": 20.0, "z": 30.0}),
                            json!({"state_id": 44, "owner": "GER", "controller": "GER", "x": 30.0, "z": -10.0}),
                        ];
                        if taken {
                            rows.push(json!({"state_id": 28, "owner": "FRA", "controller": "GER", "x": 10.0, "z": 10.0}));
                        }
                        json!({"states": rows})
                    }
                    "FRA" => {
                        let mut rows = vec![
                            json!({"state_id": 18, "owner": "FRA", "controller": "FRA", "x": 50.0, "z": 50.0}),
                        ];
                        if !taken {
                            rows.push(json!({"state_id": 28, "owner": "FRA", "controller": "FRA", "x": 10.0, "z": 10.0}));
                        }
                        json!({"states": rows})
                    }
                    "28" => json!({"states": [{"state_id": 28, "provinces": [
                        {"province": 280, "controller": "GER"}, {"province": 281, "controller": "GER"}]}]}),
                    other => panic!("unexpected query {other}"),
                },
                "hoi4_armies" => {
                    let mut armies = vec![json!({"army": "Army 1", "division_count": 10,
                        "main_state": self.here.get(), "battle_plan_orders": 2,
                        "plan_executing": self.executing.get(), "commander": "X",
                        "threat": {"share": self.share.get(), "encirclement_risk": self.share.get() >= 0.7},
                        "planning": self.planning.get().map(|p| json!({"mean": p, "min": p, "max": p}))})];
                    if self.flank.get() {
                        armies.push(
                            json!({"army": "Army 2", "division_count": 8, "main_state": 43,
                            "battle_plan_orders": 0, "plan_executing": false, "commander": "Y",
                            "threat": {"share": 0.1, "encirclement_risk": false}}),
                        );
                    }
                    json!({"unassigned_by_state": [], "armies": armies})
                }
                "hoi4_supply" => json!({
                    "armies": [{"army": "Army 1", "organisation_mean": self.org.get(),
                        "supply_percent_mean": self.supply.get()},
                        {"army": "Army 2", "organisation_mean": 50.0, "supply_percent_mean": 90.0}],
                    "supply_nodes": if taken { json!([{"node": 9, "kind": "hub", "province_id": 281,
                        "railway_level": 1}]) } else { json!([]) },
                    "provinces": [{"province_id": 280, "state_id": 28, "railway_level": 1,
                        "flows": [{"node": 9, "flow": self.flow.get(), "node_province": 281}]}]}),
                "hoi4_wars" => json!({"enemy_held_adjacent_states": [
                    {"state_id": 28, "adjacent_own_states": [42, 43, 44]}]}),
                _ => json!({}),
            })
        }
    }
    fn plan(extra: Value) -> Value {
        let mut war = json!({"enemies": ["FRA"], "axes": [[28, 18]], "cooldown_hours": 1});
        for (k, v) in extra.as_object().unwrap() {
            war[k] = v.clone();
        }
        json!({"war": war})
    }
    fn acted(step: Tick) -> String {
        match step {
            Tick::Acted(s) => s,
            other => panic!("expected an action, got {other:?}"),
        }
    }

    #[test]
    fn the_execute_order_waits_for_the_planning_bonus() {
        let w = W::new();
        let plan = plan(json!({}));
        let mut state = State::default();
        assert!(acted(tick(&mut state, &w, &plan, "l", START).unwrap()).starts_with("war front"));
        assert!(acted(tick(&mut state, &w, &plan, "l", START + 3).unwrap())
            .starts_with("war offensive"));
        // 0.9 x 30% at +2%/day: 13.5 game days
        for day in 1..13 {
            assert_eq!(
                tick(&mut state, &w, &plan, "l", START + 3 + day * DAY).unwrap(),
                Tick::Nothing
            );
        }
        let line = &state.war_rules.notes["planning:Army 1"];
        assert!(line.contains("no tool reads planning"), "{line}");
        let step = acted(tick(&mut state, &w, &plan, "l", START + 3 + 14 * DAY).unwrap());
        assert!(step.starts_with("war execute Army 1"), "{step}");
        // switched off: the next tick after the line, like before
        let mut state = State::default();
        let off = plan_with(json!({"planning_wait": false}));
        tick(&mut state, &w, &off, "l", START).unwrap();
        tick(&mut state, &w, &off, "l", START + 3).unwrap();
        assert!(
            acted(tick(&mut state, &w, &off, "l", START + 6).unwrap()).starts_with("war execute")
        );
    }
    #[test]
    fn the_planning_read_decides_the_execute_order() {
        let w = W::new();
        let plan = plan(json!({}));
        let mut state = State::default();
        w.planning.set(Some(0.0));
        tick(&mut state, &w, &plan, "l", START).unwrap();
        tick(&mut state, &w, &plan, "l", START + 3).unwrap();
        // rising below 0.9 x 30%: waits, and says it was read
        for (day, bonus) in [(1, 0.05), (2, 0.1), (3, 0.15)] {
            w.planning.set(Some(bonus));
            assert_eq!(
                tick(&mut state, &w, &plan, "l", START + 3 + day * DAY).unwrap(),
                Tick::Nothing
            );
        }
        let line = &state.war_rules.notes["planning:Army 1"];
        assert!(line.contains("read from hoi4_armies"), "{line}");
        // reached 27%: executes on day 4, long before the 13.5-day estimate
        w.planning.set(Some(0.27));
        let step = acted(tick(&mut state, &w, &plan, "l", START + 3 + 4 * DAY).unwrap());
        assert!(step.starts_with("war execute Army 1"), "{step}");
        assert!(state.war_rules.notes["planning:Army 1"].contains("reached"));
    }

    #[test]
    fn a_planning_bonus_that_stops_rising_is_at_the_armys_cap() {
        let w = W::new();
        let plan = plan(json!({}));
        let mut state = State::default();
        tick(&mut state, &w, &plan, "l", START).unwrap();
        tick(&mut state, &w, &plan, "l", START + 3).unwrap();
        w.planning.set(Some(0.2));
        assert_eq!(
            tick(&mut state, &w, &plan, "l", START + 3 + DAY).unwrap(),
            Tick::Nothing
        );
        // the same 20% a game day later: the cap (a low-planning army), execute
        let step = acted(tick(&mut state, &w, &plan, "l", START + 3 + 2 * DAY + 1).unwrap());
        assert!(step.starts_with("war execute Army 1"), "{step}");
        assert!(state.war_rules.notes["planning:Army 1"].contains("stopped rising"));
    }

    fn plan_with(extra: Value) -> Value {
        plan(extra)
    }

    #[test]
    fn a_near_deadline_and_a_short_max_skip_the_long_wait() {
        let w = W::new();
        let mut p = plan(json!({"planning_max_days": 2}));
        let mut state = State::default();
        tick(&mut state, &w, &p, "l", START).unwrap();
        tick(&mut state, &w, &p, "l", START + 3).unwrap();
        assert_eq!(
            tick(&mut state, &w, &p, "l", START + 3 + DAY).unwrap(),
            Tick::Nothing
        );
        assert!(
            acted(tick(&mut state, &w, &p, "l", START + 3 + 2 * DAY).unwrap())
                .starts_with("war execute")
        );
        // a goal deadline ten days away: no wait at all
        p["goal"] = json!({"deadline": "1936.01.10"});
        let now = super::super::date_hour("1936.01.01").unwrap();
        let mut state = State::default();
        tick(&mut state, &w, &p, "l", now).unwrap();
        tick(&mut state, &w, &p, "l", now + 3).unwrap();
        assert!(acted(tick(&mut state, &w, &p, "l", now + 6).unwrap()).starts_with("war execute"));
    }

    #[test]
    fn supply_below_the_floor_rests_and_below_the_hold_nothing_resumes() {
        let w = W::new();
        let p = plan(json!({"planning_wait": false, "rest_max_days": 10}));
        let mut state = State::default();
        state.war_assign.insert("Army 1".into(), 28);
        state.war_stage.insert("Army 1".into(), 3);
        w.executing.set(true);
        w.supply.set(45.0);
        let step = acted(tick(&mut state, &w, &p, "l", START).unwrap());
        assert!(
            step.starts_with("war rest Army 1") && step.contains("supply 45%"),
            "{step}"
        );
        w.executing.set(false);
        // below the hold: not even after rest_max_days
        w.supply.set(30.0);
        assert_eq!(
            tick(&mut state, &w, &p, "l", START + 11 * DAY).unwrap(),
            Tick::Nothing
        );
        // between hold and resume: rest_max_days resumes
        w.supply.set(40.0);
        let step = acted(tick(&mut state, &w, &p, "l", START + 12 * DAY).unwrap());
        assert!(step.starts_with("war resume Army 1"), "{step}");
        // supply 60 and fresh: resumes before rest_max (70 needed): rest again first
        let mut state = State::default();
        state.war_assign.insert("Army 1".into(), 28);
        state.war_stage.insert("Army 1".into(), 3);
        w.executing.set(true);
        w.supply.set(45.0);
        tick(&mut state, &w, &p, "l", START).unwrap();
        w.executing.set(false);
        w.supply.set(60.0);
        assert_eq!(
            tick(&mut state, &w, &p, "l", START + DAY).unwrap(),
            Tick::Nothing
        );
        w.supply.set(75.0);
        assert!(
            acted(tick(&mut state, &w, &p, "l", START + 2 * DAY).unwrap())
                .starts_with("war resume Army 1")
        );
    }

    #[test]
    fn a_starving_army_gets_no_new_front_and_the_hold_stops_own_state_retakes() {
        let w = W::new();
        w.supply.set(45.0);
        let p = plan(json!({}));
        let mut state = State::default();
        assert_eq!(tick(&mut state, &w, &p, "l", START).unwrap(), Tick::Nothing);
        assert!(w.actions().is_empty());
        assert!(state.war_rules.notes["hold:Army 1"].contains("supply 45%"));
        w.supply.set(55.0);
        assert!(acted(tick(&mut state, &w, &p, "l", START + DAY).unwrap()).starts_with("war front"));
    }

    #[test]
    fn rest_and_resume_follow_the_share_of_the_highest_organisation_seen() {
        let w = W::new();
        let p = plan(json!({"planning_wait": false}));
        let mut state = State::default();
        state.war_assign.insert("Army 1".into(), 28);
        state.war_stage.insert("Army 1".into(), 3);
        w.executing.set(true);
        w.org.set(60.0);
        assert_eq!(tick(&mut state, &w, &p, "l", START).unwrap(), Tick::Nothing);
        // 17 < 0.3 x 60
        w.org.set(17.0);
        assert!(acted(tick(&mut state, &w, &p, "l", START + DAY).unwrap())
            .starts_with("war rest Army 1"));
        w.executing.set(false);
        // 45 < 0.8 x 60 = 48
        w.org.set(45.0);
        assert_eq!(
            tick(&mut state, &w, &p, "l", START + 2 * DAY).unwrap(),
            Tick::Nothing
        );
        w.org.set(49.0);
        assert!(
            acted(tick(&mut state, &w, &p, "l", START + 3 * DAY).unwrap())
                .starts_with("war resume Army 1")
        );
        assert!(state.war_rules.notes["rest"].contains("highest organisation_mean"));
    }

    #[test]
    fn half_the_land_held_by_others_falls_back_only_outside_safe_own_land() {
        let w = W::new();
        w.share.set(0.55);
        let p = plan(json!({}));
        // standing in a fully held own state: a normal front, no fall-back
        let mut state = State::default();
        let step = acted(tick(&mut state, &w, &p, "l", START).unwrap());
        assert!(step.starts_with("war front"), "{step}");
        // in enemy land at 55%: back to the nearest safe state
        w.here.set(28);
        let mut state = State::default();
        let step = acted(tick(&mut state, &w, &p, "l", START).unwrap());
        assert!(step.starts_with("war pocket Army 1: 55%"), "{step}");
        // the old threshold by key
        let old = plan(json!({"pocket_share": 0.7}));
        let mut state = State::default();
        assert!(!acted(tick(&mut state, &w, &old, "l", START).unwrap()).starts_with("war pocket"));
    }

    fn stalled(w: &W, p: &Value) -> (State, GameHour) {
        let mut state = State::default();
        state.war_assign.insert("Army 1".into(), 28);
        state.war_stage.insert("Army 1".into(), 3);
        w.executing.set(true);
        assert_eq!(tick(&mut state, w, p, "l", START).unwrap(), Tick::Nothing);
        (state, START + 8 * DAY)
    }

    #[test]
    fn a_stall_brings_an_idle_army_beside_the_target_first() {
        let w = W::new();
        let p = plan(json!({"planning_wait": false, "reserve": []}));
        let (mut state, now) = stalled(&w, &p);
        w.flank.set(true);
        let step = acted(tick(&mut state, &w, &p, "l", now).unwrap());
        assert!(
            step.contains("second direction, Army 2 attacks it from state 43"),
            "{step}"
        );
        assert_eq!(state.war_assign.get("Army 2"), Some(&28));
        // Army 2 draws its own front at the target next
        let step = acted(tick(&mut state, &w, &p, "l", now + 1).unwrap());
        assert!(step.starts_with("war front Army 2 -> state 28"), "{step}");
        // the next stall of Army 1 on 28 is the direct move
        let later = now + 8 * DAY + 2;
        let moves = |w: &W| {
            w.actions()
                .iter()
                .filter(|(n, a)| n == "hoi4_army_move" && a["army"] == "Army 1")
                .count()
        };
        for h in 0..4 {
            let _ = tick(&mut state, &w, &p, "l", later + h);
        }
        assert_eq!(moves(&w), 1);
    }

    #[test]
    fn without_a_flank_the_stalled_army_goes_round_then_draws_again() {
        let w = W::new();
        let p = plan(json!({"planning_wait": false}));
        let (mut state, now) = stalled(&w, &p);
        let step = acted(tick(&mut state, &w, &p, "l", now).unwrap());
        assert!(
            step.contains("plan stopped to go round by state 44"),
            "{step}"
        );
        w.executing.set(false);
        let step = acted(tick(&mut state, &w, &p, "l", now + 1).unwrap());
        assert!(
            step.starts_with("war second direction Army 1 -> state 44"),
            "{step}"
        );
        // on the way: nothing
        assert_eq!(
            tick(&mut state, &w, &p, "l", now + DAY).unwrap(),
            Tick::Nothing
        );
        // arrived: the front is drawn from there
        w.here.set(44);
        let step = acted(tick(&mut state, &w, &p, "l", now + 2 * DAY).unwrap());
        assert!(step.starts_with("war front Army 1 -> state 28"), "{step}");
    }

    #[test]
    fn switched_off_the_stall_is_the_direct_move_at_once() {
        let w = W::new();
        let p = plan(json!({"planning_wait": false, "second_direction": false}));
        let (mut state, now) = stalled(&w, &p);
        let step = acted(tick(&mut state, &w, &p, "l", now).unwrap());
        assert!(step.starts_with("war stalled Army 1"), "{step}");
        assert_eq!(w.actions().last().unwrap().0, "hoi4_army_move");
    }

    #[test]
    fn a_captured_hub_holds_the_next_execute_until_it_feeds_the_front() {
        let w = W::new();
        let p = plan(json!({"planning_wait": false}));
        let mut state = State::default();
        state.war_assign.insert("Army 1".into(), 28);
        state.war_stage.insert("Army 1".into(), 3);
        w.executing.set(true);
        tick(&mut state, &w, &p, "l", START).unwrap();
        // 28 falls: the army turns to 18 and the bound starts
        w.taken.set(true);
        w.executing.set(false);
        w.here.set(28);
        w.share.set(0.2);
        let step = acted(tick(&mut state, &w, &p, "l", START + DAY).unwrap());
        assert!(step.starts_with("war front Army 1 -> state 18"), "{step}");
        assert!(state.war_rules.bound.contains_key("Army 1"));
        let step = acted(tick(&mut state, &w, &p, "l", START + DAY + 3).unwrap());
        assert!(step.starts_with("war offensive"), "{step}");
        assert_eq!(
            tick(&mut state, &w, &p, "l", START + 3 * DAY).unwrap(),
            Tick::Nothing
        );
        assert!(state.war_rules.notes["bound:Army 1"].contains("holds after taking state 28"));
        // the hub feeds the army's province: execute
        w.flow.set(1.5);
        let step = acted(tick(&mut state, &w, &p, "l", START + 4 * DAY).unwrap());
        assert!(step.starts_with("war execute Army 1 -> state 18"), "{step}");
    }

    #[test]
    fn a_bound_ends_after_the_rail_days_or_the_max() {
        let mut state = State::default();
        let p = plan(json!({}));
        state.war_rules.bound.insert(
            "A".into(),
            Bound {
                state: 1,
                since: START,
                nodes: vec![],
                rail: true,
                core: false,
            },
        );
        assert!(bound_wait(&mut state, &p, "A", None, Some(80.0), START + 9 * DAY).is_some());
        assert!(bound_wait(&mut state, &p, "A", None, Some(40.0), START + 11 * DAY).is_some());
        assert!(bound_wait(&mut state, &p, "A", None, Some(40.0), START + 12 * DAY).is_none());
        state.war_rules.bound.insert(
            "B".into(),
            Bound {
                state: 1,
                since: START,
                nodes: vec![],
                rail: true,
                core: true,
            },
        );
        assert!(bound_wait(&mut state, &p, "B", None, Some(80.0), START + 5 * DAY).is_none());
    }

    #[test]
    fn disband_keeps_overseas_and_encircled_divisions() {
        struct D(RefCell<Vec<(String, Value)>>, bool);
        impl Tools for D {
            fn call(&self, name: &str, args: &Value) -> Result<Value> {
                self.0.borrow_mut().push((name.to_string(), args.clone()));
                if args.get("request_id").is_some() {
                    return Ok(json!({"receipt": {"status": "Verified"}}));
                }
                Ok(match name {
                    "hoi4_overview" => json!({"player": "ITA"}),
                    "hoi4_states" => json!({"states": [
                        {"state_id": 158, "owner": "ITA", "controller": "ITA"},
                        {"state_id": 160, "owner": "ITA", "controller": "ITA"},
                        {"state_id": 550, "owner": "ITA", "controller": "ITA"}]}),
                    "hoi4_wars" if self.1 => {
                        json!({"capital_land": {"capital_state": 2, "states": [2, 158, 160]}})
                    }
                    "hoi4_wars" => {
                        json!({"capital_land": null, "capital_land_reason": "map_data_unavailable"})
                    }
                    "hoi4_armies" => json!({"armies": [
                        {"army": "Army 4", "threat": {"encirclement_risk": true}},
                        {"army": "Army 5", "threat": {"encirclement_risk": false}}]}),
                    "hoi4_supply" => json!({"divisions": [
                        {"army": "Army 3", "id": "o", "template": "CN", "state_id": 550},
                        {"army": "Army 4", "id": "a", "template": "CN", "state_id": 158},
                        {"army": "Army 5", "id": "d", "template": "CN", "state_id": 160}]}),
                    _ => json!({}),
                })
            }
        }
        let plan = json!({"disband": {"templates": ["CN"]}});
        let d = D(RefCell::new(vec![]), true);
        let mut state = State::default();
        let step = super::super::disband_step(&mut state, &d, &plan, "l", START)
            .unwrap()
            .unwrap();
        // Army 3 stands overseas (550), Army 4 is at encirclement risk: only Army 5
        assert!(step.starts_with("disband 1 from Army 5"), "{step}");
        assert!(state.war_rules.notes["disband"].contains("o"));
        // no land-route read: nothing is disbanded
        let d = D(RefCell::new(vec![]), false);
        let mut state = State::default();
        assert_eq!(
            super::super::disband_step(&mut state, &d, &plan, "l", START).unwrap(),
            None
        );
        assert!(state.war_rules.notes["disband"].contains("map_data_unavailable"));
        assert!(!d
            .0
            .borrow()
            .iter()
            .any(|(n, _)| n == "hoi4_division_disband"));
    }
}
