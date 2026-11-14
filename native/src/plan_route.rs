//! Victory-point routes: a `vp_targets` entry may give the way to its province as `path`
//! (waypoint provinces in order, the target last or left out). The war step then advances along
//! that path one leg at a time instead of choosing its own way to the target's state. A child
//! module of `plan_run` (declared there with `#[path]`), like plan_war.rs: every order still
//! goes through `State::act`; this module's memory (`State::routes`) is this server life's.
//!
//! The rules, for any country and any map:
//!   the leg       the first waypoint the player does not hold is the leg's objective: the war
//!                 step's target is its state (battle plans) or the province itself (direct
//!                 moves), not the far victory point. A waypoint lost again comes first.
//!   the jump-off  the waypoint before the objective is held; an army standing in neither its
//!                 state nor the objective's state goes there first (its plan stopped, one move
//!                 to that province over land it may enter), and draws its front from there, so
//!                 the attack starts where the plan's route starts.
//!   the pace      when a waypoint falls the next leg waits until the army's supply is back at
//!                 `war.supply_resume`, at most `war.rest_max_days` while it is at or above
//!                 `war.supply_hold` (an executing plan is stopped for the wait). The general
//!                 limits (`war.supply_floor`, the rest line, the bound after a captured node)
//!                 still apply to every order.
//!   land          a waypoint is passable when the player or a country it is at war with holds
//!                 it. A waypoint another country holds (no unit crosses a neutral border), or
//!                 one in no state the executor can read around the target, closes the path.
//!   fall-back     with no path, a closed path or a jump-off the army could not reach, the war
//!                 step routes on its own as before. Which of the two is in use, and why, is the
//!                 run result's `war` note `route:<army>`.
use super::{list, plan_war, text, GameHour, State, Tools, DAY};
use crate::error::Result;
use serde_json::{json, Value};
use std::collections::{HashMap, HashSet};

/// States read at most to find the state of one waypoint.
const SEARCH_STATES: usize = 32;
/// Game days a path stays closed after its jump-off could not be reached.
const CLOSED_DAYS: GameHour = 10;
/// Refused or timed-out moves to a jump-off before the path is closed.
const STAGING_TRIES: u8 = 2;

/// What the routes keep (this server life).
#[derive(Default)]
pub(super) struct Memory {
    /// province -> its state (a province never changes its state)
    states: HashMap<i64, i64>,
    /// states whose provinces are all in `states`
    scanned: HashSet<i64>,
    /// army -> (victory-point province, leg number, game hour the last waypoint fell)
    progress: HashMap<String, (i64, usize, Option<GameHour>)>,
    /// army -> (jump-off province, game hour the move there was sent)
    staging: HashMap<String, (i64, GameHour)>,
    /// army -> (jump-off province, moves there that failed)
    staging_fails: HashMap<String, (i64, u8)>,
    /// army -> (victory-point province, game hour until which its path is closed, why)
    closed: HashMap<String, (i64, GameHour, String)>,
}

/// An army was renamed: what is kept for it follows the name.
pub(super) fn army_renamed(memory: &mut Memory, old: &str, new: &str) {
    fn rekey<V>(map: &mut HashMap<String, V>, old: &str, new: &str) {
        if let Some(v) = map.remove(old) {
            map.insert(new.to_string(), v);
        }
    }
    rekey(&mut memory.progress, old, new);
    rekey(&mut memory.staging, old, new);
    rekey(&mut memory.staging_fails, old, new);
    rekey(&mut memory.closed, old, new);
}

/// One leg of a plan path.
#[derive(Clone, Debug, PartialEq)]
pub(super) struct Leg {
    /// the state of the waypoint to take now
    pub state: i64,
    /// the waypoint to take now
    pub province: i64,
    /// (state, province) of the held waypoint before it: where the leg starts
    pub from: Option<(i64, i64)>,
    /// the leg's number (1 is the first waypoint) and the number of waypoints
    pub step: usize,
    pub of: usize,
}

/// How the army gets to its victory-point target.
#[derive(Clone, Debug, PartialEq)]
pub(super) enum Routing {
    /// along the plan's path
    Path(Leg),
    /// the war step's own routing; Some(why) when the plan gave a path that is not followed
    Own(Option<String>),
}

/// The hoi4_states row of one state (every province with its controller), read once per tick.
pub(super) fn state_row(tools: &dyn Tools, rows: &mut HashMap<i64, Value>, state: i64) -> Value {
    rows.entry(state)
        .or_insert_with(|| {
            tools
                .call("hoi4_states", &json!({"query": state.to_string()}))
                .map(|v| {
                    list(&v, "states")
                        .iter()
                        .find(|r| r["state_id"].as_i64() == Some(state))
                        .or_else(|| list(&v, "states").first())
                        .cloned()
                        .unwrap_or_default()
                })
                .unwrap_or_default()
        })
        .clone()
}

/// The state of `province` and who holds it now: the first of `candidates` that lists it.
fn locate(
    memory: &mut Memory,
    tools: &dyn Tools,
    rows: &mut HashMap<i64, Value>,
    province: i64,
    candidates: &[i64],
) -> Option<(i64, String)> {
    if !memory.states.contains_key(&province) {
        for st in candidates.iter().take(SEARCH_STATES) {
            if memory.scanned.contains(st) {
                continue;
            }
            let row = state_row(tools, rows, *st);
            let provinces = list(&row, "provinces");
            if provinces.is_empty() {
                continue;
            }
            for p in provinces {
                if let Some(id) = p["province"].as_i64() {
                    memory.states.insert(id, *st);
                }
            }
            memory.scanned.insert(*st);
            if memory.states.contains_key(&province) {
                break;
            }
        }
    }
    let st = *memory.states.get(&province)?;
    let row = state_row(tools, rows, st);
    let holder = list(&row, "provinces")
        .iter()
        .find(|p| p["province"].as_i64() == Some(province))
        .map(|p| text(&p["controller"]).to_string())?;
    Some((st, holder))
}

/// The waypoints of a `vp_targets` entry: its `path`, then its province (a path that already
/// ends there is not doubled; a province repeated in a row counts once).
fn waypoints(target: &Value) -> Vec<i64> {
    let mut out: Vec<i64> = list(target, "path")
        .iter()
        .filter_map(Value::as_i64)
        .collect();
    if let Some(p) = target["province"].as_i64() {
        out.push(p);
    }
    out.dedup();
    out
}

/// How `army` gets to the victory-point entry `target` now. `candidates` are the states searched
/// for the waypoints, nearest first; `enemies` the countries the player is at war with.
#[allow(clippy::too_many_arguments)]
pub(super) fn choose(
    state: &mut State,
    tools: &dyn Tools,
    rows: &mut HashMap<i64, Value>,
    army: &str,
    target: &Value,
    me: &str,
    enemies: &HashSet<String>,
    candidates: &[i64],
    now: GameHour,
) -> Routing {
    let goal = target["province"].as_i64().unwrap_or(0);
    let way = waypoints(target);
    if list(target, "path").is_empty() || way.len() < 2 {
        plan_war::set_note(state, &format!("route:{army}"), None);
        return Routing::Own(None);
    }
    let own = |state: &mut State, why: String| {
        plan_war::set_note(
            state,
            &format!("route:{army}"),
            Some(format!(
                "{army} goes to province {goal} by the executor's own routing: {why}"
            )),
        );
        Routing::Own(Some(why))
    };
    match state.routes.closed.get(army) {
        Some((p, until, why)) if *p == goal && *until > now => {
            let why = why.clone();
            return own(state, why);
        }
        Some(_) => {
            state.routes.closed.remove(army);
        }
        None => {}
    }
    let mut placed: Vec<(i64, i64, String)> = Vec::new();
    for p in &way {
        match locate(&mut state.routes, tools, rows, *p, candidates) {
            Some((st, holder)) => placed.push((*p, st, holder)),
            None => {
                return own(
                    state,
                    format!(
                        "the plan's path is closed: waypoint {p} is in no state read around the target (land the country may not enter, or no province of this map)"
                    ),
                )
            }
        }
    }
    let Some(at) = placed.iter().position(|(_, _, holder)| holder != me) else {
        // every waypoint is held: nothing to route (the target is taken)
        return Routing::Own(None);
    };
    if let Some((p, _, holder)) = placed[at..]
        .iter()
        .find(|(_, _, holder)| holder != me && !enemies.contains(holder))
    {
        return own(
            state,
            format!(
                "the plan's path is closed: waypoint {p} is held by {}, which {me} is not at war with (no unit crosses a neutral border)",
                if holder.is_empty() { "nobody" } else { holder }
            ),
        );
    }
    let leg = Leg {
        state: placed[at].1,
        province: placed[at].0,
        from: at.checked_sub(1).map(|i| (placed[i].1, placed[i].0)),
        step: at + 1,
        of: placed.len(),
    };
    let from = leg.from.map_or(String::new(), |(st, p)| {
        format!(" from province {p} (state {st})")
    });
    plan_war::set_note(
            state,
        &format!("route:{army}"),
        Some(format!(
            "{army} follows the plan's path to province {goal}: leg {} of {}, province {} (state {}){from}",
            leg.step, leg.of, leg.province, leg.state
        )),
    );
    Routing::Path(leg)
}

/// Why the army waits before its next leg (None: go). When a waypoint falls the next leg is
/// committed only once the army's supply is back at `war.supply_resume`; it goes anyway after
/// `war.rest_max_days` at or above `war.supply_hold`. Without a supply read nothing waits.
#[allow(clippy::too_many_arguments)]
pub(super) fn pace(
    state: &mut State,
    plan: &Value,
    army: &str,
    goal: i64,
    leg: &Leg,
    supply: Option<f64>,
    limits: &plan_war::Limits,
    now: GameHour,
) -> Option<String> {
    let seen = state.routes.progress.get(army).copied();
    let since = match seen {
        Some((g, step, since)) if g == goal && step == leg.step => since,
        Some((g, step, _)) if g == goal && step < leg.step => Some(now),
        _ => None,
    };
    state
        .routes
        .progress
        .insert(army.to_string(), (goal, leg.step, since));
    let at = since?;
    let key = format!("pace:{army}");
    let go = |state: &mut State, line: String| {
        state
            .routes
            .progress
            .insert(army.to_string(), (goal, leg.step, None));
        plan_war::set_note(state, &key, Some(line));
        None
    };
    let Some(supply) = supply else {
        return go(
            state,
            format!(
                "{army} starts leg {} of {} at once: no supply read",
                leg.step, leg.of
            ),
        );
    };
    let days = (now - at) / DAY;
    if limits.supplied(supply) {
        return go(
            state,
            format!(
                "{army} starts leg {} of {} to province {}: supply {supply:.0}% after {days} days",
                leg.step, leg.of, leg.province
            ),
        );
    }
    let max: GameHour = plan["war"]["rest_max_days"].as_i64().unwrap_or(20) * DAY;
    if now - at >= max && !limits.held(supply) {
        return go(
            state,
            format!(
                "{army} starts leg {} of {} to province {} after the {} days at most: supply {supply:.0}% is above the hard hold but never reached the resume line",
                leg.step,
                leg.of,
                leg.province,
                max / DAY
            ),
        );
    }
    let why = format!(
        "{army} holds the waypoint it took before leg {} of {} to province {}: supply {supply:.0}% is below the resume line ({days} days)",
        leg.step, leg.of, leg.province
    );
    plan_war::set_note(state, &key, Some(why.clone()));
    Some(why)
}

/// What the move to a leg's jump-off means for the war step this tick.
pub(super) enum Stage {
    /// The army stands where the leg starts (or the leg has no jump-off).
    Ready,
    /// On its way: no other order.
    Wait,
    /// An order was sent.
    Acted(String),
}

/// Bring `army` to the jump-off of its leg: stop an executing plan, send one move to the held
/// waypoint before the objective, wait for it. An army in the jump-off's state or already in the
/// objective's state is ready. A move refused or not arrived `STAGING_TRIES` times closes the
/// path for `CLOSED_DAYS` game days (the war step then routes on its own).
#[allow(clippy::too_many_arguments)]
pub(super) fn stage(
    state: &mut State,
    tools: &dyn Tools,
    lease: &str,
    army: &str,
    goal: i64,
    leg: &Leg,
    here: Option<i32>,
    executing: bool,
    stall: GameHour,
    now: GameHour,
) -> Result<Stage> {
    let Some((from_state, from_province)) = leg.from else {
        state.routes.staging.remove(army);
        return Ok(Stage::Ready);
    };
    let here = here.map(i64::from);
    if here == Some(from_state) || here == Some(leg.state) {
        if state.routes.staging.remove(army).is_some() {
            // arrived: the front and the offensive are drawn from here
            state.war_stage.insert(army.to_string(), 0);
            state.war_move.remove(army);
        }
        state.routes.staging_fails.remove(army);
        return Ok(Stage::Ready);
    }
    let fail = |state: &mut State, why: &str| {
        let fails = match state.routes.staging_fails.get(army) {
            Some((p, n)) if *p == from_province => n + 1,
            _ => 1,
        };
        state
            .routes
            .staging_fails
            .insert(army.to_string(), (from_province, fails));
        if fails >= STAGING_TRIES {
            state.routes.staging_fails.remove(army);
            state.routes.closed.insert(
                army.to_string(),
                (
                    goal,
                    now + CLOSED_DAYS * DAY,
                    format!(
                        "the jump-off province {from_province} (state {from_state}) of leg {} was not reached ({why})",
                        leg.step
                    ),
                ),
            );
        }
    };
    match state.routes.staging.get(army).copied() {
        Some((p, at)) if p == from_province => {
            if now - at > 2 * stall.max(DAY) {
                state.routes.staging.remove(army);
                fail(state, "the army did not arrive");
            }
            Ok(Stage::Wait)
        }
        Some(_) => {
            state.routes.staging.remove(army);
            Ok(Stage::Wait)
        }
        None if executing => {
            let (ok, why) = state.act(
                tools,
                lease,
                "hoi4_army_plan",
                json!({"army": army, "execute": false}),
                "wj",
            )?;
            if !ok {
                fail(state, "its plan could not be stopped");
            }
            Ok(Stage::Acted(format!(
                "war route {army}: plan stopped to go to the jump-off province {from_province} (state {from_state}) of leg {} of {}: {ok} {why}",
                leg.step, leg.of
            )))
        }
        None => {
            let (ok, why) = state.act(
                tools,
                lease,
                "hoi4_army_move",
                json!({"army": army, "state_id": from_state, "province_id": from_province}),
                "wj",
            )?;
            if ok {
                state
                    .routes
                    .staging
                    .insert(army.to_string(), (from_province, now));
                state.war_stage.insert(army.to_string(), 0);
                state.war_move.remove(army);
            } else if !why.starts_with("move_held") {
                fail(state, "the move was refused");
            }
            Ok(Stage::Acted(format!(
                "war route {army} -> province {from_province} (state {from_state}), the jump-off of leg {} of {} to province {}: {ok} {why}",
                leg.step, leg.of, leg.province
            )))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::{tick, Tick};
    use super::*;
    use crate::error::Result;
    use std::cell::{Cell, RefCell};

    const START: GameHour = 1_000_000;

    /// GER (own states 42 and 43) at war with FRA (states 28 and 18). State 60 belongs to the
    /// neutral BEL until `annexed`. Provinces: 420, 421 in 42; 430 in 43; 600, 601 in 60;
    /// 280, 281, 282 in 28. Army 1 stands in `here`.
    struct Map {
        here: Cell<i64>,
        annexed: Cell<bool>,
        /// provinces of state 28 the player holds
        taken: RefCell<Vec<i64>>,
        executing: Cell<bool>,
        supply: Cell<f64>,
        refuse_moves: Cell<bool>,
        calls: RefCell<Vec<(String, Value)>>,
    }
    impl Map {
        fn new() -> Self {
            Self {
                here: Cell::new(42),
                annexed: Cell::new(true),
                taken: RefCell::new(vec![]),
                executing: Cell::new(false),
                supply: Cell::new(90.0),
                refuse_moves: Cell::new(false),
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
        fn last(&self) -> (String, Value) {
            self.actions().last().cloned().expect("an action")
        }
    }
    impl Tools for Map {
        fn call(&self, name: &str, args: &Value) -> Result<Value> {
            self.calls
                .borrow_mut()
                .push((name.to_string(), args.clone()));
            if args.get("request_id").is_some() {
                if name == "hoi4_army_move" && self.refuse_moves.get() {
                    return Err(crate::error::Error::new(
                        "input_readback_failed",
                        "The army did not move",
                    ));
                }
                if name == "hoi4_army_plan" {
                    self.executing.set(args["execute"] == json!(true));
                }
                return Ok(json!({"receipt": {"status": "Verified"}}));
            }
            let middle = if self.annexed.get() { "GER" } else { "BEL" };
            let held = |p: i64| {
                if self.taken.borrow().contains(&p) {
                    "GER"
                } else {
                    "FRA"
                }
            };
            Ok(match name {
                "hoi4_research" => json!({"slots": [{"researching": "a"}]}),
                "hoi4_focus_completed" => json!({"active_key": "f"}),
                "hoi4_economy" => json!({"factory_trigger_count": {"military": 0}}),
                "hoi4_overview" => json!({"player": "GER"}),
                "hoi4_wars" => json!({"wars": {"validated": true, "enemies": ["FRA"]}}),
                "hoi4_states" => match args["query"].as_str().unwrap() {
                    "GER" => {
                        let mut rows = vec![
                            json!({"state_id": 42, "owner": "GER", "controller": "GER", "x": 0.0, "z": 0.0}),
                            json!({"state_id": 43, "owner": "GER", "controller": "GER", "x": 0.0, "z": 40.0}),
                        ];
                        if self.annexed.get() {
                            rows.push(json!({"state_id": 60, "owner": "GER", "controller": "GER", "x": 40.0, "z": 0.0}));
                        }
                        json!({"states": rows})
                    }
                    "FRA" => json!({"states": [
                        {"state_id": 28, "owner": "FRA", "controller": "FRA", "x": 80.0, "z": 0.0},
                        {"state_id": 18, "owner": "FRA", "controller": "FRA", "x": 200.0, "z": 0.0}]}),
                    "42" => json!({"states": [{"state_id": 42, "provinces": [
                        {"province": 420, "controller": "GER"}, {"province": 421, "controller": "GER"}]}]}),
                    "43" => json!({"states": [{"state_id": 43, "provinces": [
                        {"province": 430, "controller": "GER"}]}]}),
                    "60" => json!({"states": [{"state_id": 60, "provinces": [
                        {"province": 600, "controller": middle}, {"province": 601, "controller": middle}]}]}),
                    "28" => json!({"states": [{"state_id": 28, "provinces": [
                        {"province": 280, "controller": held(280)},
                        {"province": 281, "controller": held(281)},
                        {"province": 282, "controller": held(282)}]}]}),
                    "18" => json!({"states": [{"state_id": 18, "provinces": [
                        {"province": 180, "controller": "FRA"}]}]}),
                    other => panic!("unexpected query {other}"),
                },
                "hoi4_armies" => json!({"unassigned_by_state": [], "armies": [
                    {"army": "Army 1", "division_count": 10, "main_state": self.here.get(),
                     "battle_plan_orders": 0, "plan_executing": self.executing.get(),
                     "commander": "X", "threat": {"share": 0.1, "encirclement_risk": false}}]}),
                "hoi4_supply" => json!({"armies": [{"army": "Army 1", "organisation_mean": 50.0,
                    "supply_percent_mean": self.supply.get()}], "divisions": [], "provinces": [],
                    "supply_nodes": []}),
                _ => json!({}),
            })
        }
    }

    fn plan(direct: bool) -> Value {
        json!({"war": {"enemies": ["FRA"], "direct_moves": direct, "theatre": [42, 43, 60],
                "planning_wait": false, "second_direction": false},
            "vp_targets": [{"province": 282, "state": 28, "army": "Army 1",
                            "path": [420, 600, 281, 282]}]})
    }

    fn run(state: &mut State, map: &Map, plan: &Value, now: GameHour) -> String {
        match tick(state, map, plan, "l", now).unwrap() {
            Tick::Acted(line) => line,
            other => format!("{other:?}"),
        }
    }

    fn note(state: &State, key: &str) -> String {
        plan_war::digest(state)
            .and_then(|d| d[key].as_str().map(str::to_string))
            .unwrap_or_default()
    }

    #[test]
    fn the_army_goes_to_the_jump_off_before_it_attacks_along_the_path() {
        let map = Map::new();
        let mut state = State::default();
        let plan = plan(false);
        // Army 1 stands in state 42; the leg is 281 from the held waypoint 600 (state 60)
        let line = run(&mut state, &map, &plan, START);
        assert!(
            line.starts_with("war route Army 1 -> province 600 (state 60)"),
            "{line}"
        );
        let (tool, args) = map.last();
        assert_eq!(tool, "hoi4_army_move");
        assert_eq!(
            (args["state_id"].clone(), args["province_id"].clone()),
            (json!(60), json!(600))
        );
        assert!(
            note(&state, "route:Army 1").contains("follows the plan's path to province 282: leg 3 of 4, province 281 (state 28) from province 600"),
            "{}",
            note(&state, "route:Army 1")
        );
        // on its way: no second order
        let before = map.actions().len();
        run(&mut state, &map, &plan, START + 13);
        assert_eq!(map.actions().len(), before);
        // arrived: the front is drawn on the leg's state from there
        map.here.set(60);
        let line = run(&mut state, &map, &plan, START + 26);
        assert!(line.starts_with("war front Army 1 -> state 28"), "{line}");
    }

    #[test]
    fn direct_moves_take_the_next_waypoint_not_the_far_target() {
        let map = Map::new();
        map.here.set(60);
        let mut state = State::default();
        let line = run(&mut state, &map, &plan(true), START);
        assert!(
            line.starts_with("war vp target Army 1 -> province 281 (state 28)"),
            "{line}"
        );
        assert_eq!(map.last().1["province_id"], json!(281));
    }

    #[test]
    fn a_waypoint_in_neutral_land_closes_the_path_and_the_executor_routes_on_its_own() {
        let map = Map::new();
        map.annexed.set(false);
        let mut state = State::default();
        let line = run(&mut state, &map, &plan(false), START);
        // own routing: the battle plan on the target's state from where the army stands
        assert!(line.starts_with("war front Army 1 -> state 28"), "{line}");
        let why = note(&state, "route:Army 1");
        assert!(
            why.contains("own routing")
                && why.contains("waypoint 600 is held by BEL")
                && why.contains("neutral border"),
            "{why}"
        );
        assert!(map.actions().iter().all(|(n, _)| n != "hoi4_army_move"));
    }

    #[test]
    fn a_waypoint_outside_the_states_around_the_target_closes_the_path() {
        let map = Map::new();
        let mut state = State::default();
        let mut plan = plan(false);
        plan["vp_targets"][0]["path"] = json!([420, 9999, 281]);
        let line = run(&mut state, &map, &plan, START);
        assert!(line.starts_with("war front Army 1 -> state 28"), "{line}");
        assert!(
            note(&state, "route:Army 1")
                .contains("waypoint 9999 is in no state read around the target"),
            "{}",
            note(&state, "route:Army 1")
        );
    }

    #[test]
    fn a_target_without_a_path_is_routed_as_before() {
        let map = Map::new();
        let mut state = State::default();
        let mut plan = plan(false);
        plan["vp_targets"][0]
            .as_object_mut()
            .unwrap()
            .remove("path");
        let line = run(&mut state, &map, &plan, START);
        assert!(line.starts_with("war front Army 1 -> state 28"), "{line}");
        assert_eq!(note(&state, "route:Army 1"), "");
    }

    #[test]
    fn the_next_leg_waits_until_the_taken_waypoint_is_supplied() {
        let map = Map::new();
        map.here.set(28);
        map.supply.set(90.0);
        let mut state = State::default();
        let plan = plan(true);
        let line = run(&mut state, &map, &plan, START);
        assert!(
            line.starts_with("war vp target Army 1 -> province 281"),
            "{line}"
        );
        // 281 falls; the army's supply there is above the floor but below the resume line
        map.taken.borrow_mut().push(281);
        map.supply.set(55.0);
        let before = map.actions().len();
        run(&mut state, &map, &plan, START + 2 * DAY);
        run(&mut state, &map, &plan, START + 4 * DAY);
        assert_eq!(map.actions().len(), before, "{:?}", map.actions());
        assert!(
            note(&state, "pace:Army 1")
                .contains("holds the waypoint it took before leg 4 of 4 to province 282"),
            "{}",
            note(&state, "pace:Army 1")
        );
        // supplied again: the last leg is committed
        map.supply.set(75.0);
        let line = run(&mut state, &map, &plan, START + 6 * DAY);
        assert!(
            line.starts_with("war vp target Army 1 -> province 282"),
            "{line}"
        );
    }

    #[test]
    fn the_pace_stops_an_executing_plan_and_ends_after_the_longest_rest() {
        let map = Map::new();
        map.here.set(28);
        let mut state = State::default();
        let plan = plan(false);
        let limits = plan_war::Limits::of(&plan);
        let leg = |step| Leg {
            state: 28,
            province: if step == 3 { 281 } else { 282 },
            from: Some((60, 600)),
            step,
            of: 4,
        };
        assert_eq!(
            pace(
                &mut state,
                &plan,
                "Army 1",
                282,
                &leg(3),
                Some(90.0),
                &limits,
                START
            ),
            None
        );
        // the waypoint fell at 55% supply: wait; at 30% (below the hard hold) even past the maximum
        assert!(pace(
            &mut state,
            &plan,
            "Army 1",
            282,
            &leg(4),
            Some(55.0),
            &limits,
            START + DAY
        )
        .is_some());
        assert!(pace(
            &mut state,
            &plan,
            "Army 1",
            282,
            &leg(4),
            Some(30.0),
            &limits,
            START + 30 * DAY
        )
        .is_some());
        // above the hold after the 20 days at most: go, and no second wait on the same leg
        assert_eq!(
            pace(
                &mut state,
                &plan,
                "Army 1",
                282,
                &leg(4),
                Some(55.0),
                &limits,
                START + 31 * DAY
            ),
            None
        );
        assert_eq!(
            pace(
                &mut state,
                &plan,
                "Army 1",
                282,
                &leg(4),
                Some(40.0),
                &limits,
                START + 32 * DAY
            ),
            None
        );
        // in the war step an executing plan is stopped for the wait
        let mut state = State::default();
        map.executing.set(true);
        run(&mut state, &map, &plan, START);
        map.taken.borrow_mut().push(281);
        map.supply.set(55.0);
        let line = run(&mut state, &map, &plan, START + 2 * DAY);
        assert!(
            line.starts_with("war route Army 1: plan stopped at the waypoint it took"),
            "{line}"
        );
        assert_eq!(map.last().1["execute"], json!(false));
    }

    #[test]
    fn a_jump_off_the_army_cannot_reach_closes_the_path_for_a_while() {
        let map = Map::new();
        map.refuse_moves.set(true);
        let mut state = State::default();
        let plan = plan(false);
        let mut lines = vec![];
        for i in 0..40 {
            lines.push(run(&mut state, &map, &plan, START + i * 2 * DAY));
            if note(&state, "route:Army 1").contains("own routing") {
                break;
            }
        }
        let why = note(&state, "route:Army 1");
        assert!(
            why.contains("the jump-off province 600 (state 60) of leg 3 was not reached"),
            "{why} {lines:?}"
        );
    }
}
