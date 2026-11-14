//! Executor steps written ahead of their game action: the plan key, its judgement and its
//! digest exist, the tool that carries the order out is still to be built against the live game.
//! A child module of `plan_run` (declared there with `#[path]`) like plan_military.rs: one-shot
//! work is kept in `State::done`, the timers in `State::bad`, every order goes through
//! `State::act` and its failure ledger. All cadences are game time.
//!
//! The seam: each step works out the order it would send and sends it only when its tool is in
//! `plan_run::RECEIPT_TOOLS` (a tool is listed there once its handler answers with a terminal
//! receipt, and a test checks every listed tool is served). Until then nothing is sent, nothing
//! is marked done, and the digest (`seams`) says what waits on which tool and why, so the plan
//! acts the day the tool lands without another change here. The gap register row of each step
//! stays "not executable" until then.
//!
//! Hook in plan_run.rs: [`step`] is the last domain (`seams`) of the scheduler; `run` adds
//! [`digest`] as `seams`.
//!
//! Plan keys:
//!   defense_lines [{"id","army","kind":"fallback"|"area_defense","states":[own state ids],
//!                 "settings"?:[..],"when"?,"until"?}] (gap register orders.defensive_lines):
//!                 a defensive order from the battle-plan toolbar for `army` (an army name, or an
//!                 army_layout name_hint: the army playing that role). `fallback` is the fallback
//!                 line (toolbar button btn_order_draw_fallback, key C): a line drawn through
//!                 `states`, behind the front, that the army mans and digs in on (rivers and
//!                 forts in those states are the point). `area_defense` is the area defence order
//!                 (btn_area_defense, key V): the army spreads over `states` and covers what
//!                 `settings` names, the toggles of the order's settings window
//!                 (battleplantools_area_defense_config_window: victory_points, ports,
//!                 coastlines, airbases, borders, forts, rail, facility; none given = the game's
//!                 default). Once `when` holds and the army exists (hoi4_armies) the order is
//!                 sent once (hoi4_army_fallback / hoi4_army_area_defense {army, states,
//!                 settings?}); once `until` holds it is deleted (the toolbar's delete order,
//!                 btn_order_delete, key DEL: hoi4_army_orders_delete {army, order}). The war step
//!                 still drives an army that is not in `war.reserve` (or a guard): the lint asks
//!                 for that (orders.defense_reserve). One action per tick, judged once a game
//!                 day. Done-memory `dl:<id>:on`, `dl:<id>:off`
//!   garrison_template {"template"} (gap register occupation.garrison_template): the template the
//!                 Occupied Territories view puts on garrison duty (its garrison_template_button,
//!                 MAIN_GARRISON_TEMPLATE; the per-state garrison entries of the same view follow
//!                 it). `template` is a templates[] id (its `name`) or a template name. Once the
//!                 player occupies a state (hoi4_occupation role occupied_by_player) and the
//!                 template exists (hoi4_division_templates) it is set once
//!                 (hoi4_occupation_garrison {template}). Done-memory `gt:<name>`
//!   xp_budget.army[] purpose "exercise" (gap register training.exercises): army exercises,
//!                 the battle-plan toolbar's exercise button (btn_exercise, key K; Shift+K stops at
//!                 the maximum, icn_exercise_stop_at_max). Divisions train toward
//!                 NDefines.NMilitary.TRAINING_MAX_LEVEL (Regular) at UNIT_EXPERIENCE_PER_TRAINING_DAY
//!                 and the country earns army XP up to TRAINING_MAX_DAILY_COUNTRY_EXP a day; it
//!                 costs TRAINING_ATTRITION extra attrition (equipment) and fuel
//!                 (ARMY_TRAINING_FUEL_MULT; out of fuel OUT_OF_FUEL_TRAINING_XP_GAIN_MULT of the
//!                 XP). Once `when` holds at peace the entry's `armies` (names or army_layout
//!                 roles; none = every own army) start exercising, stopping at the maximum
//!                 (hoi4_army_exercise {armies, on: true, stop_at_max: true}); when a war starts
//!                 they stop (on: false). Done-memory `ex:<id>:on`, `ex:<id>:off`
use super::{item_id, list, strings, text, GameHour, State, Tools, DAY, RECEIPT_TOOLS};
use crate::error::Result;
use crate::plan_predicate::Truth;
use serde_json::{json, Value};
use std::collections::BTreeMap;

/// What these steps keep (this server life).
#[derive(Default)]
pub(super) struct Memory {
    /// The last status line per step and item (the run result's `seams`).
    notes: BTreeMap<String, String>,
}

/// The fallback-line order (battle-plan toolbar).
pub(super) const FALLBACK_TOOL: &str = "hoi4_army_fallback";
/// The area-defence order (battle-plan toolbar).
pub(super) const AREA_DEFENSE_TOOL: &str = "hoi4_army_area_defense";
/// Deletes an army's order of one kind (battle-plan toolbar delete).
pub(super) const DELETE_ORDER_TOOL: &str = "hoi4_army_orders_delete";
/// Sets the default garrison template (Occupied Territories view).
pub(super) const GARRISON_TOOL: &str = "hoi4_occupation_garrison";
/// Starts or stops army exercises (battle-plan toolbar exercise button).
pub(super) const EXERCISE_TOOL: &str = "hoi4_army_exercise";
/// The area-defence settings the order's window offers (its toggle buttons, btn_setting_<name>).
pub(super) const AREA_SETTINGS: &[&str] = &[
    "victory_points",
    "ports",
    "coastlines",
    "airbases",
    "borders",
    "forts",
    "rail",
    "facility",
];

/// Whether the live build has delivered `tool` (it answers with receipts and is served).
pub(super) fn built(tool: &str) -> bool {
    RECEIPT_TOOLS.contains(&tool)
}

/// The last status of each step (the run result's `seams`), or None when nothing ran.
pub(super) fn digest(state: &State) -> Option<Value> {
    (!state.seams.notes.is_empty()).then(|| json!(state.seams.notes))
}

fn note(state: &mut State, key: &str, line: impl Into<String>) {
    state.seams.notes.insert(key.to_string(), line.into());
}

/// The seam steps in order, at most one action.
pub(super) fn step(
    state: &mut State,
    tools: &dyn Tools,
    plan: &Value,
    lease: &str,
    now: GameHour,
) -> Result<Option<String>> {
    if let Some(done) = defense_lines_step(state, tools, plan, lease, now)? {
        return Ok(Some(done));
    }
    if let Some(done) = garrison_step(state, tools, plan, lease, now)? {
        return Ok(Some(done));
    }
    exercise_step(state, tools, plan, lease, now)
}

/// The line a seam writes when its tool is still to be built: what it would send.
fn unbuilt(tool: &str, args: &Value, row: &str) -> String {
    format!(
        "would send {tool} {args}, but {tool} is not built yet (gap register {row}): nothing sent"
    )
}

fn garrison_step(
    state: &mut State,
    tools: &dyn Tools,
    plan: &Value,
    lease: &str,
    now: GameHour,
) -> Result<Option<String>> {
    let want = text(&plan["garrison_template"]["template"])
        .trim()
        .to_string();
    if want.is_empty() || state.skipped(now, "sgt", "x") {
        return Ok(None);
    }
    // a templates[] id stands for the name the template step gives it
    let name = list(plan, "templates")
        .iter()
        .find(|t| text(&t["id"]) == want)
        .map(|t| text(&t["name"]).trim().to_string())
        .filter(|n| !n.is_empty())
        .unwrap_or(want);
    if state.done.contains_key(&format!("gt:{name}")) {
        return Ok(None);
    }
    let key = "garrison_template";
    let occupied = match tools.call("hoi4_occupation", &json!({})) {
        Ok(read) => list(&read, "states")
            .iter()
            .any(|s| text(&s["role"]) == "occupied_by_player"),
        Err(e) => {
            note(state, key, format!("hoi4_occupation unreadable: {e}"));
            state.skip(now, "sgt", "x", DAY);
            return Ok(None);
        }
    };
    if !occupied {
        // the Occupied Territories view needs an occupied state to open
        note(state, key, format!("{name}: waits for an occupied state"));
        state.skip(now, "sgt", "x", 7 * DAY);
        return Ok(None);
    }
    let exists = tools
        .call("hoi4_division_templates", &json!({}))
        .is_ok_and(|r| {
            list(&r, "templates")
                .iter()
                .any(|t| text(&t["template"]) == name)
        });
    if !exists {
        note(state, key, format!("waiting for the template {name}"));
        state.skip(now, "sgt", "x", DAY);
        return Ok(None);
    }
    let args = json!({"template": name});
    if !built(GARRISON_TOOL) {
        let line = unbuilt(GARRISON_TOOL, &args, "occupation.garrison_template");
        note(state, key, line);
        state.skip(now, "sgt", "x", 7 * DAY);
        return Ok(None);
    }
    let (ok, why) = state.act(tools, lease, GARRISON_TOOL, args, "gt")?;
    if ok {
        state.done.insert(format!("gt:{name}"), now);
    } else {
        state.skip(now, "sgt", "x", 2 * DAY);
    }
    note(state, key, format!("garrison template {name}: {ok} {why}"));
    Ok(Some(format!("garrison template {name}: {ok} {why}")))
}

fn exercise_step(
    state: &mut State,
    tools: &dyn Tools,
    plan: &Value,
    lease: &str,
    now: GameHour,
) -> Result<Option<String>> {
    let entries: Vec<(String, &Value)> = list(&plan["xp_budget"], "army")
        .iter()
        .enumerate()
        .filter(|(_, e)| text(&e["purpose"]) == "exercise")
        .map(|(i, e)| (item_id(e).unwrap_or_else(|| format!("#{i}")), e))
        .filter(|(id, _)| !state.done.contains_key(&format!("ex:{id}:off")))
        .collect();
    if entries.is_empty() || state.skipped(now, "sex", "x") {
        return Ok(None);
    }
    let at_war = match tools.call("hoi4_wars", &json!({})) {
        Ok(r) => r["wars"]["at_war"].as_bool(),
        Err(_) => None,
    };
    let Some(at_war) = at_war else {
        state.skip(now, "sex", "x", DAY);
        return Ok(None);
    };
    let mut roster: Option<Vec<String>> = None;
    for (id, entry) in entries {
        if state.skipped(now, "ex", &id) {
            continue;
        }
        let key = format!("xp_budget.army:{id}");
        let running = state.done.contains_key(&format!("ex:{id}:on"));
        let (on, done) = if running {
            if !at_war {
                note(
                    state,
                    &key,
                    "exercising (the game stops each army at the maximum)",
                );
                continue;
            }
            (false, format!("ex:{id}:off"))
        } else {
            if at_war {
                note(state, &key, "at war: no exercise started");
                continue;
            }
            let when = when_holds(state, tools, &entry["when"], now);
            if when != Truth::True {
                note(state, &key, format!("waits, when {}", when.label()));
                continue;
            }
            (true, format!("ex:{id}:on"))
        };
        let named: Vec<String> = strings(entry, "armies")
            .iter()
            .map(|a| resolve(state, a))
            .filter(|a| !a.is_empty())
            .collect();
        if roster.is_none() {
            match tools.call("hoi4_armies", &json!({"threat": false})) {
                Ok(r) => {
                    roster = Some(
                        list(&r, "armies")
                            .iter()
                            .map(|a| text(&a["army"]).to_string())
                            .filter(|a| !a.is_empty())
                            .collect(),
                    )
                }
                Err(e) => {
                    note(state, &key, format!("hoi4_armies unreadable: {e}"));
                    state.skip(now, "sex", "x", DAY);
                    return Ok(None);
                }
            }
        }
        let have = roster.as_deref().unwrap_or_default();
        let armies: Vec<String> = if named.is_empty() {
            have.to_vec()
        } else {
            named.into_iter().filter(|a| have.contains(a)).collect()
        };
        if armies.is_empty() {
            note(state, &key, "no own army to exercise yet");
            continue;
        }
        let mut args = json!({"armies": armies, "on": on});
        if on {
            args["stop_at_max"] = json!(true);
        }
        if !built(EXERCISE_TOOL) {
            let line = unbuilt(EXERCISE_TOOL, &args, "training.exercises");
            note(state, &key, line);
            state.skip(now, "ex", &id, 7 * DAY);
            continue;
        }
        let (ok, why) = state.act(tools, lease, EXERCISE_TOOL, args, "ex")?;
        if ok {
            state.done.insert(done, now);
        } else {
            state.skip(now, "ex", &id, 2 * DAY);
        }
        let verb = if on { "start" } else { "stop" };
        note(state, &key, format!("{verb} exercises: {ok} {why}"));
        return Ok(Some(format!("exercise {id}: {verb}: {ok} {why}")));
    }
    state.skip(now, "sex", "x", DAY);
    Ok(None)
}

fn when_holds(state: &mut State, tools: &dyn Tools, when: &Value, now: GameHour) -> Truth {
    match when {
        Value::Null => Truth::True,
        Value::String(s) if s.trim().is_empty() => Truth::True,
        _ => super::judge(state, tools, when, now),
    }
}

/// `until` of an item: missing never holds.
fn until_holds(state: &mut State, tools: &dyn Tools, until: &Value, now: GameHour) -> bool {
    match until {
        Value::Null => false,
        Value::String(s) if s.trim().is_empty() => false,
        _ => super::judge(state, tools, until, now) == Truth::True,
    }
}

fn resolve(state: &State, hint: &str) -> String {
    state
        .layout
        .get(hint)
        .filter(|a| !a.is_empty())
        .cloned()
        .unwrap_or_else(|| hint.to_string())
}

/// The order kind of a defense_lines entry and its tool, None for an unknown kind.
fn order_tool(kind: &str) -> Option<&'static str> {
    match kind {
        "fallback" => Some(FALLBACK_TOOL),
        "area_defense" => Some(AREA_DEFENSE_TOOL),
        _ => None,
    }
}

fn defense_lines_step(
    state: &mut State,
    tools: &dyn Tools,
    plan: &Value,
    lease: &str,
    now: GameHour,
) -> Result<Option<String>> {
    let entries = list(plan, "defense_lines");
    if entries.is_empty() || state.skipped(now, "sdl", "x") {
        return Ok(None);
    }
    let keyed: Vec<(String, &Value)> = entries
        .iter()
        .enumerate()
        .map(|(i, e)| (item_id(e).unwrap_or_else(|| format!("#{i}")), e))
        .collect();
    let open = |state: &State, id: &str| !state.done.contains_key(&format!("dl:{id}:off"));
    if !keyed.iter().any(|(id, _)| open(state, id)) {
        return Ok(None);
    }
    let mut roster: Option<Value> = None;
    let mut acted = None;
    for (id, entry) in keyed {
        if !open(state, &id) || state.skipped(now, "dl", &id) {
            continue;
        }
        let key = format!("defense_lines:{id}");
        let kind = text(&entry["kind"]).to_string();
        let Some(tool) = order_tool(&kind) else {
            note(
                state,
                &key,
                format!("kind '{kind}' is not fallback or area_defense: never ordered"),
            );
            continue;
        };
        let army = resolve(state, text(&entry["army"]));
        let states: Vec<i64> = list(entry, "states")
            .iter()
            .filter_map(Value::as_i64)
            .collect();
        let on = format!("dl:{id}:on");
        if state.done.contains_key(&on) {
            if !until_holds(state, tools, &entry["until"], now) {
                note(state, &key, format!("{kind} for {army} stands"));
                continue;
            }
            let args = json!({"army": army, "order": kind});
            if !built(DELETE_ORDER_TOOL) {
                note(state, &key, format!(
                    "until holds: would delete the {kind} order of {army} ({DELETE_ORDER_TOOL} {args}), \
                     but {DELETE_ORDER_TOOL} is not built yet (gap register orders.defensive_lines): \
                     nothing sent"
                ));
                state.skip(now, "dl", &id, 7 * DAY);
                continue;
            }
            let (ok, why) = state.act(tools, lease, DELETE_ORDER_TOOL, args, "dl")?;
            if ok {
                state.done.insert(format!("dl:{id}:off"), now);
            } else {
                state.skip(now, "dl", &id, 2 * DAY);
            }
            note(state, &key, format!("{kind} of {army} deleted: {ok} {why}"));
            acted = Some(format!(
                "defense line {id}: delete {kind} of {army}: {ok} {why}"
            ));
            break;
        }
        if until_holds(state, tools, &entry["until"], now) {
            // over before it began: nothing to order, nothing to delete
            state.done.insert(format!("dl:{id}:off"), now);
            note(state, &key, "until held before the order was sent: dropped");
            continue;
        }
        let when = when_holds(state, tools, &entry["when"], now);
        if when != Truth::True {
            note(state, &key, format!("waits, when {}", when.label()));
            continue;
        }
        if army.is_empty() || states.is_empty() {
            note(state, &key, "needs an army and at least one state");
            continue;
        }
        if roster.is_none() {
            match tools.call("hoi4_armies", &json!({"threat": false})) {
                Ok(r) => roster = Some(r),
                Err(e) => {
                    state.skip(now, "sdl", "x", DAY);
                    note(state, &key, format!("hoi4_armies unreadable: {e}"));
                    return Ok(None);
                }
            }
        }
        let exists = roster
            .as_ref()
            .is_some_and(|r| list(r, "armies").iter().any(|a| text(&a["army"]) == army));
        if !exists {
            note(state, &key, format!("waiting for the army {army}"));
            continue;
        }
        let mut args = json!({"army": army, "states": states});
        if kind == "area_defense" {
            // only the toggles the order's window has (an unknown one would be refused)
            let settings: Vec<String> = strings(entry, "settings")
                .into_iter()
                .filter(|s| AREA_SETTINGS.contains(&s.as_str()))
                .collect();
            if !settings.is_empty() {
                args["settings"] = json!(settings);
            }
        }
        if !built(tool) {
            note(
                state,
                &key,
                format!(
                    "would order {kind} for {army} ({tool} {args}), but {tool} is not built yet \
                     (gap register orders.defensive_lines): nothing sent"
                ),
            );
            state.skip(now, "dl", &id, 7 * DAY);
            continue;
        }
        let (ok, why) = state.act(tools, lease, tool, args, "dl")?;
        if ok {
            state.done.insert(on, now);
        } else {
            state.skip(now, "dl", &id, 2 * DAY);
        }
        note(state, &key, format!("{kind} for {army}: {ok} {why}"));
        acted = Some(format!("defense line {id}: {kind} for {army}: {ok} {why}"));
        break;
    }
    if acted.is_none() {
        state.skip(now, "sdl", "x", DAY);
    }
    Ok(acted)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::Error;
    use std::cell::RefCell;
    use std::collections::HashMap;

    const START: GameHour = 1_000_000;

    struct Fake {
        calls: RefCell<Vec<(String, Value)>>,
        replies: HashMap<&'static str, Value>,
    }
    impl Tools for Fake {
        fn call(&self, name: &str, args: &Value) -> Result<Value> {
            self.calls
                .borrow_mut()
                .push((name.to_string(), args.clone()));
            if args.get("request_id").is_some() {
                return Ok(json!({"receipt": {"status": "Verified"}}));
            }
            self.replies
                .get(name)
                .cloned()
                .ok_or_else(|| Error::new("capability_unavailable", "no fixture"))
        }
    }
    fn game() -> Fake {
        Fake {
            calls: RefCell::new(Vec::new()),
            replies: [
                ("hoi4_overview", json!({"player": "POL"})),
                ("hoi4_armies", json!({"armies": [{"army": "Armia Poznan"}]})),
                (
                    "hoi4_wars",
                    json!({"wars": {"at_war": true, "enemies": ["GER"]}}),
                ),
            ]
            .into_iter()
            .collect(),
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

    #[test]
    fn the_order_tools_are_not_built_yet_and_the_settings_match_the_window() {
        // The day the live build lists a tool in RECEIPT_TOOLS this test changes with it.
        for tool in [FALLBACK_TOOL, AREA_DEFENSE_TOOL, DELETE_ORDER_TOOL] {
            assert!(!built(tool), "{tool} is built: update the gap register row");
        }
        assert!(built("hoi4_army_front"));
        assert_eq!(AREA_SETTINGS.len(), 8);
    }

    #[test]
    fn an_unbuilt_order_tool_sends_nothing_and_the_digest_says_what_waits() {
        let f = game();
        let plan = json!({"defense_lines": [
            {"id": "vistula", "army": "Armia Poznan", "kind": "fallback", "states": [10, 11],
             "when": "at_war_with:GER"},
            {"id": "coast", "army": "Armia Poznan", "kind": "area_defense", "states": [85],
             "settings": ["ports", "victory_points", "moats"]}]});
        let mut state = State::default();
        assert_eq!(step(&mut state, &f, &plan, "l", START).unwrap(), None);
        assert!(actions(&f).is_empty());
        assert!(!state.done.contains_key("dl:vistula:on"));
        let line = &state.seams.notes["defense_lines:vistula"];
        assert!(
            line.contains("would order fallback for Armia Poznan")
                && line.contains("hoi4_army_fallback")
                && line.contains("not built yet"),
            "{line}"
        );
        let line = &state.seams.notes["defense_lines:coast"];
        assert!(
            line.contains("hoi4_army_area_defense")
                && line.contains("\"settings\":[\"ports\",\"victory_points\"]"),
            "{line}"
        );
        assert!(digest(&state).is_some());
        // judged again only after a game day
        assert!(state.skipped(START + 1, "sdl", "x"));
    }

    #[test]
    fn a_defense_line_waits_for_when_the_army_and_rejects_an_unknown_kind() {
        let f = game();
        let plan = json!({"defense_lines": [
            {"id": "a", "army": "Armia Poznan", "kind": "fallback", "states": [10],
             "when": "at_war_with:SOV"},
            {"id": "b", "army": "Armia Krakow", "kind": "fallback", "states": [10]},
            {"id": "c", "army": "Armia Poznan", "kind": "trench", "states": [10]},
            {"id": "d", "army": "Armia Poznan", "kind": "fallback", "states": [10],
             "until": "at_war_with:GER"}]});
        let mut state = State::default();
        assert_eq!(step(&mut state, &f, &plan, "l", START).unwrap(), None);
        assert_eq!(state.seams.notes["defense_lines:a"], "waits, when false");
        assert_eq!(
            state.seams.notes["defense_lines:b"],
            "waiting for the army Armia Krakow"
        );
        assert!(state.seams.notes["defense_lines:c"].contains("not fallback or area_defense"));
        assert!(state.done.contains_key("dl:d:off"));
        assert!(actions(&f).is_empty());
    }

    #[test]
    fn garrison_and_exercise_tools_are_not_built_yet() {
        for tool in [GARRISON_TOOL, EXERCISE_TOOL] {
            assert!(!built(tool), "{tool} is built: update the gap register row");
        }
    }

    #[test]
    fn the_garrison_template_waits_for_an_occupied_state_then_names_what_it_would_set() {
        let mut f = game();
        f.replies.insert(
            "hoi4_occupation",
            json!({"states": [{"state_id": 5, "role": "own_state_held_by_other"}]}),
        );
        f.replies.insert(
            "hoi4_division_templates",
            json!({"templates": [{"template": "Garrison Cavalry", "queued_lines": 0}]}),
        );
        let plan = json!({"garrison_template": {"template": "gar"},
            "templates": [{"id": "gar", "name": "Garrison Cavalry"}]});
        let mut state = State::default();
        assert_eq!(step(&mut state, &f, &plan, "l", START).unwrap(), None);
        assert!(state.seams.notes["garrison_template"].contains("waits for an occupied state"));
        f.replies.insert(
            "hoi4_occupation",
            json!({"states": [{"state_id": 5, "role": "occupied_by_player"}]}),
        );
        let mut state = State::default();
        assert_eq!(step(&mut state, &f, &plan, "l", START).unwrap(), None);
        let line = &state.seams.notes["garrison_template"];
        assert!(
            line.contains("hoi4_occupation_garrison {\"template\":\"Garrison Cavalry\"}")
                && line.contains("occupation.garrison_template"),
            "{line}"
        );
        assert!(actions(&f).is_empty());
        assert!(!state.done.contains_key("gt:Garrison Cavalry"));
    }

    #[test]
    fn exercises_start_at_peace_for_the_named_armies_and_wait_otherwise() {
        let mut f = game();
        f.replies.insert(
            "hoi4_wars",
            json!({"wars": {"at_war": false, "enemies": []}}),
        );
        let plan = json!({"xp_budget": {"army": [
            {"id": "drill", "cost": 0, "purpose": "exercise", "armies": ["west_guard", "Ghost"]},
            {"id": "later", "cost": 0, "purpose": "exercise", "when": "at_war_with:SOV"},
            {"id": "t", "cost": 50, "purpose": "template", "ref": "template:inf"}]}});
        let mut state = State::default();
        state
            .layout
            .insert("west_guard".into(), "Armia Poznan".into());
        assert_eq!(step(&mut state, &f, &plan, "l", START).unwrap(), None);
        let line = &state.seams.notes["xp_budget.army:drill"];
        assert!(
            line.contains("hoi4_army_exercise")
                && line.contains("\"armies\":[\"Armia Poznan\"]")
                && line.contains("\"stop_at_max\":true")
                && line.contains("training.exercises"),
            "{line}"
        );
        assert!(state.seams.notes["xp_budget.army:later"].starts_with("waits, when"));
        assert!(!state.seams.notes.contains_key("xp_budget.army:t"));
        assert!(actions(&f).is_empty());
        // at war nothing starts
        f.replies.insert(
            "hoi4_wars",
            json!({"wars": {"at_war": true, "enemies": ["GER"]}}),
        );
        let mut state = State::default();
        step(&mut state, &f, &plan, "l", START).unwrap();
        assert_eq!(
            state.seams.notes["xp_budget.army:drill"],
            "at war: no exercise started"
        );
    }

    #[test]
    fn a_layout_role_names_the_army() {
        let f = game();
        let plan = json!({"defense_lines": [
            {"id": "v", "army": "west_guard", "kind": "fallback", "states": [10]}]});
        let mut state = State::default();
        state
            .layout
            .insert("west_guard".into(), "Armia Poznan".into());
        step(&mut state, &f, &plan, "l", START).unwrap();
        assert!(state.seams.notes["defense_lines:v"].contains("for Armia Poznan"));
    }
}
