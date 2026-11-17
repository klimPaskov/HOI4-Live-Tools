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
//!                 for that (orders.defense_reserve). A standing fallback line is manned when
//!                 the front ahead breaks (the enemy holds land in a state the army stands in, or
//!                 its supply is under war.supply_hold or its organisation under a quarter of its best): the army's
//!                 plan is executed once (hoi4_army_plan execute; `fall_back: false` keeps it on
//!                 its front). Built: the fallback and area tools; not yet: the area settings
//!                 (UNBUILT_ARGS) and the delete tool. One action per tick, judged once a game
//!                 day. Done-memory `dl:<id>:on`, `dl:<id>:back`, `dl:<id>:off`
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
//!                 they stop (on: false). Built: hoi4_army_exercise (card readback). Done-memory
//!                 `ex:<id>:on`, `ex:<id>:off`
//!   production_priority [equipment keys] (gap register production.line_priority_order): the lines
//!                 whose equipment starts with these keys go to the top of the production view's
//!                 priority order, in the plan's order (a higher line gets resources and factories
//!                 first; shortages and lost factories hit the bottom lines). Once a week the
//!                 matching lines (hoi4_production_lines) are compared with the last round; when
//!                 the set changed (a line added or gone) a round sends hoi4_production_priority
//!                 {line, equipment, to: top} for each, last first, one per tick, so the first
//!                 key ends on top. Done-memory `po:<handles>` (the round), `po:<handles>:<n>`
//!   commanders [{"general","traits":[trait keys or names]}], manage.cp_trait_prefer [trait keys
//!                 or names], manage.command_power_max (rule cp_spend, manage.off "cp_spend"):
//!                 command power near its cap buys assignable unit-leader traits
//!                 (hoi4_general_trait), the plan's picks first, then the first trait the tree
//!                 offers the generals leading the most divisions. Done-memory
//!                 `cps:<general>:<trait>`
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
    /// army -> the highest organisation_mean seen (organisation is absolute, not a percent: the
    /// fall-back compares against the army's own best)
    org_peak: std::collections::HashMap<String, f64>,
    /// Wake alarms raised since the last run result (cp_spend).
    alarms: Vec<Value>,
    /// cp_spend: why each general / general:trait could not be given a trait last time.
    cp_why: BTreeMap<String, String>,
}

/// Assigns a unit-leader trait for command power (the trait tree window).
pub(super) const TRAIT_TOOL: &str = "hoi4_general_trait";
/// NDefines.NCountry.BASE_MAX_COMMAND_POWER of 1.19.4, used only when neither a tooltip read
/// ("x / y") nor `manage.command_power_max` gives the cap (a mod or a raised cap sets the latter).
const BASE_MAX_COMMAND_POWER: f64 = 80.0;
/// NDefines.NMilitary.UNIT_LEADER_ASSIGN_TRAIT_COST of 1.19.4: below it nothing is tried (the
/// tool reads the install's own value and refuses insufficient_command_power).
const TRAIT_COST: f64 = 15.0;

/// The fallback-line order (battle-plan toolbar).
pub(super) const FALLBACK_TOOL: &str = "hoi4_army_fallback";
/// The area-defence order (battle-plan toolbar).
pub(super) const AREA_DEFENSE_TOOL: &str = "hoi4_army_area_defense";
/// Deletes an army's order of one kind (battle-plan toolbar delete).
pub(super) const DELETE_ORDER_TOOL: &str = "hoi4_army_orders_delete";
/// Sets the default garrison template (Occupied Territories view).
pub(super) const GARRISON_TOOL: &str = "hoi4_occupation_garrison";
/// Moves a production line in the priority order (production view arrows).
pub(super) const PRIORITY_TOOL: &str = "hoi4_production_priority";
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

/// The wake alarms raised since the last call (the run result's `alarms`).
pub(super) fn take_alarms(state: &mut State) -> Vec<Value> {
    std::mem::take(&mut state.seams.alarms)
}

/// Raise a `wake_player` alarm once per `reason` per `manage.wake_repeat_days` (7).
fn wake(state: &mut State, plan: &Value, now: GameHour, rule: &str, reason: &str, message: String) {
    if state.skipped(now, "wake", reason) {
        return;
    }
    let days = plan["manage"]["wake_repeat_days"]
        .as_i64()
        .filter(|d| *d > 0)
        .unwrap_or(7);
    state.skip(now, "wake", reason, days * DAY);
    state.seams.alarms.retain(|a| a["reason"] != json!(reason));
    state.seams.alarms.push(
        json!({"code": "wake_player", "rule": rule, "reason": reason,
        "message": message, "evidence": {}}),
    );
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
    if let Some(done) = exercise_step(state, tools, plan, lease, now)? {
        return Ok(Some(done));
    }
    if let Some(done) = exercise_default_step(state, tools, plan, lease, now)? {
        return Ok(Some(done));
    }
    if let Some(done) = cp_spend_step(state, tools, plan, lease, now)? {
        return Ok(Some(done));
    }
    production_order_step(state, tools, plan, lease, now)
}

/// The cap of an "x / y" command power text (the first number after a slash that follows a
/// number); pure.
pub(super) fn slash_max(text: &str) -> Option<f64> {
    let number = |s: &str| -> Option<f64> {
        let t: String = s
            .chars()
            .take_while(|c| c.is_ascii_digit() || *c == '.')
            .collect();
        t.trim_end_matches('.').parse().ok()
    };
    for (at, _) in text.match_indices('/') {
        let before = text[..at].trim_end();
        let tail: String = before
            .chars()
            .rev()
            .take_while(|c| c.is_ascii_digit() || *c == '.')
            .collect();
        if tail.is_empty() {
            continue;
        }
        if let Some(max) = number(text[at + 1..].trim_start()) {
            return Some(max);
        }
    }
    None
}

/// Whether command power is high enough to spend: within 20 of the cap, or at 80% of it, and
/// at least one trait's cost; pure.
pub(super) fn cp_should_spend(cp: f64, max: f64) -> bool {
    cp >= TRAIT_COST && (cp >= max - 20.0 || cp >= 0.8 * max)
}

/// Generals by the divisions of the armies they lead, most first (ties by name); pure.
pub(super) fn generals_by_divisions(armies: &[Value]) -> Vec<String> {
    let mut led: BTreeMap<String, u64> = BTreeMap::new();
    for a in armies {
        let general = text(&a["commander"]);
        if general.is_empty() {
            continue;
        }
        *led.entry(general.to_string()).or_default() += a["division_count"].as_u64().unwrap_or(0);
    }
    let mut out: Vec<(String, u64)> = led.into_iter().collect();
    out.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    out.into_iter().map(|(g, _)| g).collect()
}

/// The command power cap: a tooltip read "x / y", else manage.command_power_max, else the
/// 1.19.4 define.
fn command_power_max(tools: &dyn Tools, plan: &Value) -> f64 {
    tools
        .call("hoi4_tooltip", &json!({"target": "command_power"}))
        .ok()
        .and_then(|r| slash_max(&r.to_string()))
        .filter(|m| *m > 0.0)
        .or_else(|| {
            plan["manage"]["command_power_max"]
                .as_f64()
                .filter(|m| *m > 0.0)
        })
        .unwrap_or(BASE_MAX_COMMAND_POWER)
}

/// How long the rule rests after the trait tree window was not found: longer than any campaign,
/// since the same binary finds it no better later.
const CP_WINDOW_REST_DAYS: i64 = 20 * 365;

/// Refusals that mean "this general (or this trait) cannot take a trait now": the pick waits a
/// month and the next one is tried next week.
/// Game days a general with no free trait slot or no enabled trait is left alone.
const CP_GENERAL_REST_DAYS: i64 = 180;
const CP_PICK_REFUSALS: &[&str] = &[
    "unknown_commander",
    "general_not_listed",
    "wrong_leader",
    "trait_unknown",
    "trait_not_assignable",
    "trait_not_offered",
    "trait_unavailable",
    "trait_not_visible",
    "no_free_trait_slot",
    "no_enabled_trait",
];

/// Rule (manage.off "cp_spend"): command power is not left at its cap. Once a week command
/// power is read (hoi4_country_status fields.command_power.value; the cap from hoi4_tooltip
/// target command_power when it reads "x / y", else manage.command_power_max, else the 1.19.4
/// BASE_MAX_COMMAND_POWER 80); when it is within 20 of the cap or at 80% of it, one assignable
/// trait is bought (hoi4_general_trait): first the plan's `commanders` [{general, traits}] in
/// order (each general:trait once, done-memory `cps:<general>:<trait>`), then the first trait
/// the tree lets the general pick for the generals leading the most divisions (hoi4_armies),
/// the earliest of `manage.cp_trait_prefer` (trait keys or names) first. One trait a week. A
/// pick the game refuses waits 30 days (its reason kept for the wake); short command power waits
/// until it has grown. When nothing can be bought (no general, no reading, every pick refused)
/// the rule notes why and wakes the player.
fn cp_spend_step(
    state: &mut State,
    tools: &dyn Tools,
    plan: &Value,
    lease: &str,
    now: GameHour,
) -> Result<Option<String>> {
    let key = "cp_spend";
    if strings(&plan["manage"], "off").iter().any(|r| r == key) || state.skipped(now, "cps", "x") {
        return Ok(None);
    }
    state.skip(now, "cps", "x", 7 * DAY);
    let Some(cp) = tools
        .call("hoi4_country_status", &json!({}))
        .ok()
        .and_then(|r| r["fields"]["command_power"]["value"].as_f64())
    else {
        note(
            state,
            key,
            "command power unreadable (hoi4_country_status gave no fields.command_power.value)",
        );
        return Ok(None);
    };
    let max = command_power_max(tools, plan);
    if !cp_should_spend(cp, max) {
        note(
            state,
            key,
            format!("command power {cp:.0} of {max:.0}: kept"),
        );
        return Ok(None);
    }
    // the plan's own picks first, in order
    let mut picks: Vec<(String, Option<String>)> = Vec::new();
    for c in list(plan, "commanders") {
        let general = text(&c["general"]);
        if general.is_empty() {
            continue;
        }
        for t in strings(c, "traits") {
            if !state.done.contains_key(&format!("cps:{general}:{t}")) {
                picks.push((general.to_string(), Some(t)));
            }
        }
    }
    // then the first enabled trait of the generals leading the most divisions
    let armies = tools.call("hoi4_armies", &json!({"threat": false}));
    let armies_read = armies.is_ok();
    if let Ok(read) = &armies {
        for general in generals_by_divisions(list(read, "armies")) {
            picks.push((general, None));
        }
    }
    let prefer = strings(&plan["manage"], "cp_trait_prefer");
    let mut held: Vec<String> = Vec::new();
    for (general, trait_name) in picks {
        let pick = match &trait_name {
            Some(t) => format!("{general}:{t}"),
            None => general.clone(),
        };
        if state.skipped(now, "cpp", &pick) {
            let why = state.seams.cp_why.get(&pick).cloned().unwrap_or_default();
            held.push(format!("{pick}: {why}"));
            continue;
        }
        let mut args = json!({"general": general});
        match &trait_name {
            Some(t) => args["trait"] = json!(t),
            None if !prefer.is_empty() => args["prefer"] = json!(prefer),
            None => {}
        }
        if !built(TRAIT_TOOL) {
            note(
                state,
                key,
                unbuilt(TRAIT_TOOL, &args, "command_power.traits"),
            );
            return Ok(None);
        }
        let (ok, why) = state.act(tools, lease, TRAIT_TOOL, args, "cp")?;
        let code = super::plan_military::code(&why).to_string();
        if ok {
            if let Some(t) = &trait_name {
                state.done.insert(format!("cps:{general}:{t}"), now);
            }
            state.seams.cp_why.remove(&pick);
        } else if code == "trait_owned" {
            if let Some(t) = &trait_name {
                state.done.insert(format!("cps:{general}:{t}"), now);
            }
            // the next pick next tick, not next week
            state.skip(now, "cps", "x", DAY);
        } else if code == "insufficient_command_power" {
            let days = super::plan_military::cp_wait_days(&why);
            state.skip(now, "cps", "x", days * DAY);
        } else if code == "window_not_found" {
            // The tool cannot reach the trait tree (a build where the Leader Details window is
            // not at its mapped place; before pp13, 17 Nov 2026, every run): no input is spent
            // on it again this run (a yearly retry only repeated the same refusal, g17b, g17c).
            state.skip(now, "cps", "x", CP_WINDOW_REST_DAYS * DAY);
            state.seams.cp_why.insert(pick.clone(), why.clone());
        } else {
            if CP_PICK_REFUSALS.contains(&code.as_str()) {
                // another pick next tick
                state.skip(now, "cps", "x", DAY);
            }
            // A general with nothing to pick (no free slot, every entry locked) stays so until
            // he gains skill: half a year, not a month (g17e: four army generals refused 5-6
            // times each in two game years until the ledger gave them up).
            let rest = if matches!(code.as_str(), "no_free_trait_slot" | "no_enabled_trait") {
                CP_GENERAL_REST_DAYS
            } else {
                30
            };
            state.skip(now, "cpp", &pick, rest * DAY);
            state.seams.cp_why.insert(pick.clone(), why.clone());
        }
        let line = format!("command power {cp:.0} of {max:.0}: trait for {pick}: {ok} {why}");
        note(state, key, line.clone());
        return Ok(Some(line));
    }
    let why = if !armies_read && held.is_empty() {
        "no general to give a trait: no plan commanders and hoi4_armies unreadable".to_string()
    } else if held.is_empty() {
        "no general to give a trait: no plan commanders and no army has a commander".to_string()
    } else {
        format!("no enabled trait for any general: {}", held.join("; "))
    };
    note(
        state,
        key,
        format!("command power {cp:.0} of {max:.0} unspent: {why}"),
    );
    wake(
        state,
        plan,
        now,
        key,
        "cp_spend:nothing_to_buy",
        format!("Command power is {cp:.0} of {max:.0} and nothing can be bought with it ({why}). Give a general a trait by hand (hoi4_general_trait list:true shows what each general can pick), or name traits in the plan's commanders list"),
    );
    Ok(None)
}

/// The lines that go on top, in order: for each `production_priority` key, the own lines whose
/// equipment starts with it, in the read's order (entry_index, handle, equipment).
pub(super) fn priority_round(lines: &[Value], keys: &[String]) -> Vec<(u64, String, String)> {
    let mut out: Vec<(u64, String, String)> = Vec::new();
    for key in keys {
        for l in lines {
            let equipment = text(&l["equipment_id"]);
            let handle = text(&l["line_handle"]);
            if equipment.is_empty() || !equipment.starts_with(key.as_str()) {
                continue;
            }
            if out.iter().any(|(_, h, _)| h == handle) {
                continue;
            }
            if let Some(index) = l["entry_index"].as_u64() {
                out.push((index, handle.to_string(), equipment.to_string()));
            }
        }
    }
    out
}

/// The "to top" moves a priority round needs: the smallest m such that the shown list without
/// the round's first m lines starts with the round's other lines in order (those then need no
/// move; the first m go to the top last-first).
fn moves_needed(lines: &[Value], round: &[(u64, String, String)]) -> usize {
    let mut shown: Vec<(u64, &str)> = lines
        .iter()
        .filter_map(|l| Some((l["entry_index"].as_u64()?, l["line_handle"].as_str()?)))
        .collect();
    shown.sort_unstable();
    let shown: Vec<&str> = shown.into_iter().map(|(_, h)| h).collect();
    for m in 0..=round.len() {
        let moved: Vec<&str> = round[..m].iter().map(|(_, h, _)| h.as_str()).collect();
        let rest: Vec<&str> = shown
            .iter()
            .copied()
            .filter(|h| !moved.contains(h))
            .collect();
        let want: Vec<&str> = round[m..].iter().map(|(_, h, _)| h.as_str()).collect();
        if rest.len() >= want.len() && rest[..want.len()] == want[..] {
            return m;
        }
    }
    round.len()
}

fn production_order_step(
    state: &mut State,
    tools: &dyn Tools,
    plan: &Value,
    lease: &str,
    now: GameHour,
) -> Result<Option<String>> {
    let keys = strings(plan, "production_priority");
    if keys.is_empty() || state.skipped(now, "spo", "x") {
        return Ok(None);
    }
    let key = "production_priority";
    let lines = match tools.call("hoi4_production_lines", &json!({"limit": 128})) {
        Ok(read) => list(&read, "lines").to_vec(),
        Err(e) => {
            note(state, key, format!("hoi4_production_lines unreadable: {e}"));
            state.skip(now, "spo", "x", DAY);
            return Ok(None);
        }
    };
    // a line the plan removes is not ordered (pr2/pp2: the round moved the hull lines marked
    // `remove` to the top first, and their removal then aimed at a moved index)
    let removed: Vec<String> = list(plan, "production")
        .iter()
        .filter(|l| l["remove"] == json!(true))
        .map(|l| text(&l["equipment"]).to_string())
        .collect();
    let round: Vec<(u64, String, String)> = priority_round(&lines, &keys)
        .into_iter()
        .filter(|(_, _, e)| !removed.contains(e))
        .collect();
    if round.is_empty() {
        note(state, key, "no own line of the priority equipment yet");
        state.skip(now, "spo", "x", 7 * DAY);
        return Ok(None);
    }
    let mut handles: Vec<&str> = round.iter().map(|(_, h, _)| h.as_str()).collect();
    handles.sort_unstable();
    let sig = format!("po:{}", handles.join(","));
    if state.done.contains_key(&sig) {
        note(
            state,
            key,
            format!("{} lines on top in the plan's order", round.len()),
        );
        state.skip(now, "spo", "x", 7 * DAY);
        return Ok(None);
    }
    // A new round moves only what is out of place: with "to top" moves the round's first m
    // lines go up last-first, so m is the smallest count after which the rest of the list
    // already starts with the round's remaining lines in order (pr2 moved every planned line
    // again after each added or removed line: 13 moves a round).
    if !(0..round.len()).any(|n| state.done.contains_key(&format!("{sig}:{n}"))) {
        let m = moves_needed(&lines, &round);
        for n in 0..round.len() - m {
            state.done.insert(format!("{sig}:{n}"), now);
        }
        if m == 0 {
            state.done.insert(sig.clone(), now);
            note(
                state,
                key,
                format!("{} lines already on top in the plan's order", round.len()),
            );
            state.skip(now, "spo", "x", 7 * DAY);
            return Ok(None);
        }
    }
    // the next line of the round: the last one not yet moved
    let Some(n) = (0..round.len()).find(|n| !state.done.contains_key(&format!("{sig}:{n}"))) else {
        state.done.insert(sig, now);
        return Ok(None);
    };
    let (index, _, equipment) = &round[round.len() - 1 - n];
    let args = json!({"line": index, "equipment": equipment, "to": "top"});
    if !built(PRIORITY_TOOL) {
        note(
            state,
            key,
            unbuilt(PRIORITY_TOOL, &args, "production.line_priority_order"),
        );
        state.skip(now, "spo", "x", 7 * DAY);
        return Ok(None);
    }
    let (ok, why) = state.act(tools, lease, PRIORITY_TOOL, args, "po")?;
    if ok {
        state.done.insert(format!("{sig}:{n}"), now);
        if n + 1 == round.len() {
            state.done.insert(sig, now);
        }
    } else {
        state.skip(now, "spo", "x", 2 * DAY);
    }
    let line = format!("priority {equipment} (line {index}) to the top: {ok} {why}");
    note(state, key, line.clone());
    Ok(Some(line))
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

/// The mean equipment fill of the fielded divisions (hoi4_supply equipment_fill_min).
fn mean_fill(read: &Value) -> Option<f64> {
    let fills: Vec<f64> = list(read, "divisions")
        .iter()
        .filter_map(|d| d["equipment_fill_min"].as_f64())
        .collect();
    (!fills.is_empty()).then(|| fills.iter().sum::<f64>() / fills.len() as f64)
}

/// Rule (manage.off "exercises"; a plan with its own xp_budget.army "exercise" entries keeps
/// them instead): at peace the armies keep exercising, each stopping by itself at the training
/// maximum, so army experience accrues for doctrines, templates and designs and new divisions
/// reach the maximum before a war (pr3b, 17 Nov 2026: no exercise in two years, doctrines
/// waited 412 days on experience). Exercises wear equipment, so they start only while the
/// fielded divisions hold their equipment (mean fill >= manage.exercise_fill, default 0.9).
/// Every 14 game days the armies not exercising are started together; a set of armies the game
/// refuses (all at the maximum) waits 60 days. At war the rule stops the armies it started.
fn exercise_default_step(
    state: &mut State,
    tools: &dyn Tools,
    plan: &Value,
    lease: &str,
    now: GameHour,
) -> Result<Option<String>> {
    let planned = list(&plan["xp_budget"], "army")
        .iter()
        .any(|e| text(&e["purpose"]) == "exercise");
    if planned
        || strings(&plan["manage"], "off")
            .iter()
            .any(|r| r == "exercises")
        || state.skipped(now, "exd", "x")
    {
        return Ok(None);
    }
    let Some(at_war) = tools
        .call("hoi4_wars", &json!({}))
        .ok()
        .and_then(|r| r["wars"]["at_war"].as_bool())
    else {
        state.skip(now, "exd", "x", DAY);
        return Ok(None);
    };
    let armies = match tools.call("hoi4_armies", &json!({"threat": false})) {
        Ok(r) => list(&r, "armies").to_vec(),
        Err(_) => {
            state.skip(now, "exd", "x", DAY);
            return Ok(None);
        }
    };
    let exercising = |want: bool| -> Vec<String> {
        armies
            .iter()
            .filter(|a| a["exercising"].as_bool() == Some(want))
            .filter(|a| a["division_count"].as_u64().unwrap_or(1) > 0)
            .map(|a| text(&a["army"]).to_string())
            .filter(|a| !a.is_empty())
            .collect()
    };
    if at_war {
        let running = exercising(true);
        if running.is_empty() || !state.done.contains_key("exd:started") {
            state.skip(now, "exd", "x", 7 * DAY);
            return Ok(None);
        }
        let (ok, why) = state.act(
            tools,
            lease,
            EXERCISE_TOOL,
            json!({"armies": running, "on": false}),
            "xd",
        )?;
        if ok {
            state.done.remove("exd:started");
        } else {
            state.skip(now, "exd", "x", 2 * DAY);
        }
        note(
            state,
            "exercises",
            format!("at war: exercises stopped: {ok} {why}"),
        );
        return Ok(Some(format!("exercises stopped at war: {ok} {why}")));
    }
    state.skip(now, "exd", "x", 14 * DAY);
    let idle = exercising(false);
    if idle.is_empty() {
        note(state, "exercises", "every army exercises or none can");
        return Ok(None);
    }
    let floor = plan["manage"]["exercise_fill"].as_f64().unwrap_or(0.9);
    match super::plan_manage::supply(state, tools, now)
        .as_ref()
        .and_then(mean_fill)
    {
        Some(fill) if fill < floor => {
            note(
                state,
                "exercises",
                format!("waits: divisions' mean equipment fill {fill:.2} below {floor:.2} (exercises wear equipment)"),
            );
            return Ok(None);
        }
        None => {
            note(state, "exercises", "waits: equipment fill unreadable");
            return Ok(None);
        }
        _ => {}
    }
    let set = format!("exdset:{}", idle.join("|"));
    if state.skipped(now, "exdset", &set) {
        return Ok(None);
    }
    let (ok, why) = state.act(
        tools,
        lease,
        EXERCISE_TOOL,
        json!({"armies": idle, "on": true, "stop_at_max": true}),
        "xd",
    )?;
    if ok {
        state.done.insert("exd:started".into(), now);
    } else {
        state.skip(now, "exdset", &set, 60 * DAY);
    }
    note(
        state,
        "exercises",
        format!("start exercises ({} armies): {ok} {why}", idle.len()),
    );
    Ok(Some(format!(
        "exercises start {} armies: {ok} {why}",
        idle.len()
    )))
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

/// Why the front ahead of `army`'s fallback line is breaking, or None while it holds: the enemy
/// controls land in a state the army stands in (hoi4_wars occupied_own_states), or the army's
/// supply has fallen under war.supply_hold (35% by default) or its organisation under a quarter of its best
/// (hoi4_supply). Reads only.
fn front_breaking(
    memory: &mut Memory,
    tools: &dyn Tools,
    plan: &Value,
    army: &str,
) -> Option<String> {
    let roster = tools.call("hoi4_armies", &json!({"threat": false})).ok()?;
    let standing: Vec<i64> = list(&roster, "armies")
        .iter()
        .find(|a| text(&a["army"]) == army)
        .and_then(|a| a["states"].as_object())
        .map(|m| m.keys().filter_map(|k| k.parse().ok()).collect())
        .unwrap_or_default();
    if let Ok(wars) = tools.call("hoi4_wars", &json!({})) {
        for row in list(&wars, "occupied_own_states") {
            let id = row["state_id"].as_i64().or(row.as_i64());
            if let Some(id) = id.filter(|id| standing.contains(id)) {
                return Some(format!(
                    "the enemy holds land in state {id}, where it stands"
                ));
            }
        }
    }
    // a percent, or a fraction as plan_war reads it (0.35 is 35%)
    let hold =
        plan["war"]["supply_hold"]
            .as_f64()
            .map_or(35.0, |f| if f <= 1.0 { f * 100.0 } else { f });
    let supply = tools.call("hoi4_supply", &json!({"summary": true})).ok()?;
    let row = list(&supply, "armies")
        .iter()
        .find(|a| text(&a["army"]) == army)
        .cloned()?;
    if let Some(p) = row["supply_percent_mean"].as_f64().filter(|p| *p < hold) {
        return Some(format!("supply {p:.0}% under {hold:.0}%"));
    }
    if let Some(o) = row["organisation_mean"].as_f64() {
        let peak = memory.org_peak.entry(army.to_string()).or_insert(o);
        *peak = peak.max(o);
        if *peak > 0.0 && o < 0.25 * *peak {
            return Some(format!(
                "organisation {o:.1} under a quarter of its best {:.1}",
                *peak
            ));
        }
    }
    None
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
                // A fallback line is manned when the front ahead of it breaks or goes unfed: the
                // army's plan is executed once and it falls back to the line.
                let back = format!("dl:{id}:back");
                if kind == "fallback"
                    && entry["fall_back"] != json!(false)
                    && !state.done.contains_key(&back)
                {
                    if let Some(why) = front_breaking(&mut state.seams, tools, plan, &army) {
                        let (ok, res) = state.act(
                            tools,
                            lease,
                            "hoi4_army_plan",
                            json!({"army": army, "execute": true}),
                            "dl",
                        )?;
                        if ok {
                            state.done.insert(back, now);
                        } else {
                            state.skip(now, "dl", &id, DAY);
                        }
                        note(
                            state,
                            &key,
                            format!("{army} falls back to the line ({why}): {ok} {res}"),
                        );
                        acted = Some(format!(
                            "defense line {id}: {army} falls back ({why}): {ok} {res}"
                        ));
                        break;
                    }
                }
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
            if !settings.is_empty() && !super::arg_built(AREA_DEFENSE_TOOL, "settings") {
                note(
                    state,
                    &format!("{key}:settings"),
                    format!(
                        "settings {settings:?} held back: the order's toggles are not built yet \
                         (the game's defaults apply)"
                    ),
                );
            } else if !settings.is_empty() {
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
        /// tool -> (refusal code, message) for its orders
        refusing: HashMap<&'static str, (&'static str, &'static str)>,
    }
    impl Tools for Fake {
        fn call(&self, name: &str, args: &Value) -> Result<Value> {
            self.calls
                .borrow_mut()
                .push((name.to_string(), args.clone()));
            if args.get("request_id").is_some() {
                if let Some((code, message)) = self.refusing.get(name) {
                    return Err(Error::new(code, *message));
                }
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
            refusing: HashMap::new(),
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
        assert!(built(FALLBACK_TOOL) && built(AREA_DEFENSE_TOOL));
        assert!(
            !built(DELETE_ORDER_TOOL),
            "delete is built: update the gap register row"
        );
        assert!(!super::super::arg_built(AREA_DEFENSE_TOOL, "settings"));
        assert!(built("hoi4_army_front"));
        assert_eq!(AREA_SETTINGS.len(), 8);
    }

    #[test]
    fn the_defensive_orders_are_sent_once_and_area_settings_wait_for_their_toggles() {
        let f = game();
        let plan = json!({"defense_lines": [
            {"id": "vistula", "army": "Armia Poznan", "kind": "fallback", "states": [10, 11]},
            {"id": "coast", "army": "Armia Poznan", "kind": "area_defense", "states": [85],
             "settings": ["ports", "victory_points", "moats"]}]});
        let mut state = State::default();
        let out = step(&mut state, &f, &plan, "l", START).unwrap().unwrap();
        assert!(out.contains("fallback for Armia Poznan: true"), "{out}");
        assert!(state.done.contains_key("dl:vistula:on"));
        let out = step(&mut state, &f, &plan, "l", START + DAY)
            .unwrap()
            .unwrap();
        assert!(out.contains("area_defense for Armia Poznan: true"), "{out}");
        let sent = actions(&f);
        assert_eq!(sent[0].0, "hoi4_army_fallback");
        assert_eq!(sent[0].1["states"], json!([10, 11]));
        assert_eq!(sent[1].0, "hoi4_army_area_defense");
        assert!(
            sent[1].1.get("settings").is_none(),
            "settings are held back"
        );
        assert!(state.seams.notes["defense_lines:coast:settings"].contains("held back"));
        assert!(digest(&state).is_some());
    }

    #[test]
    fn organisation_is_judged_against_the_armys_best_and_a_fractional_supply_hold_is_a_percent() {
        let mut f = game();
        f.replies.insert(
            "hoi4_armies",
            json!({"armies": [{"army": "Armia Poznan", "states": {"10": 4}}]}),
        );
        // militia: 20 organisation at full strength is not a broken front
        f.replies.insert(
            "hoi4_supply",
            json!({"armies": [{"army": "Armia Poznan", "supply_percent_mean": 60.0,
                "organisation_mean": 20.0}]}),
        );
        let plan = json!({"war": {"supply_hold": 0.5}, "defense_lines": [
            {"id": "v", "army": "Armia Poznan", "kind": "fallback", "states": [11]}]});
        let mut state = State::default();
        step(&mut state, &f, &plan, "l", START).unwrap();
        step(&mut state, &f, &plan, "l", START + DAY).unwrap();
        assert_eq!(actions(&f).len(), 1, "only the line itself");
        // worn to 4 of its best 20: it falls back
        f.replies.insert(
            "hoi4_supply",
            json!({"armies": [{"army": "Armia Poznan", "supply_percent_mean": 60.0,
                "organisation_mean": 4.0}]}),
        );
        let out = step(&mut state, &f, &plan, "l", START + 2 * DAY)
            .unwrap()
            .unwrap();
        assert!(out.contains("under a quarter of its best 20.0"), "{out}");
        // supply_hold 0.5 reads as 50%: 40% supply falls back
        let mut state = State::default();
        f.replies.insert(
            "hoi4_supply",
            json!({"armies": [{"army": "Armia Poznan", "supply_percent_mean": 40.0,
                "organisation_mean": 20.0}]}),
        );
        step(&mut state, &f, &plan, "l", START).unwrap();
        let out = step(&mut state, &f, &plan, "l", START + DAY)
            .unwrap()
            .unwrap();
        assert!(out.contains("supply 40% under 50%"), "{out}");
    }

    #[test]
    fn an_army_falls_back_to_its_line_once_when_its_front_goes_unfed() {
        let mut f = game();
        f.replies.insert(
            "hoi4_armies",
            json!({"armies": [{"army": "Armia Poznan", "states": {"10": 4}}]}),
        );
        f.replies.insert(
            "hoi4_supply",
            json!({"armies": [{"army": "Armia Poznan", "supply_percent_mean": 20.0,
                "organisation_mean": 60.0}]}),
        );
        let plan = json!({"defense_lines": [
            {"id": "vistula", "army": "Armia Poznan", "kind": "fallback", "states": [11]}]});
        let mut state = State::default();
        step(&mut state, &f, &plan, "l", START).unwrap();
        let out = step(&mut state, &f, &plan, "l", START + DAY)
            .unwrap()
            .unwrap();
        assert!(
            out.contains("falls back (supply 20% under 35%): true"),
            "{out}"
        );
        let sent = actions(&f);
        assert_eq!(sent[1].0, "hoi4_army_plan");
        assert_eq!(sent[1].1["execute"], true);
        // once only
        step(&mut state, &f, &plan, "l", START + 2 * DAY).unwrap();
        assert_eq!(actions(&f).len(), 2);
        // fall_back:false keeps the army on its front
        let mut state = State::default();
        let f2 = {
            let mut g = game();
            g.replies = f.replies.clone();
            g
        };
        let plan = json!({"defense_lines": [
            {"id": "v", "army": "Armia Poznan", "kind": "fallback", "states": [11], "fall_back": false}]});
        step(&mut state, &f2, &plan, "l", START).unwrap();
        step(&mut state, &f2, &plan, "l", START + DAY).unwrap();
        assert_eq!(actions(&f2).len(), 1);
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
    fn the_exercise_tool_is_built_and_the_garrison_tool_not_yet() {
        assert!(built(EXERCISE_TOOL));
        assert!(
            !built(GARRISON_TOOL),
            "{GARRISON_TOOL} is built: update the gap register row"
        );
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
    fn idle_armies_exercise_by_default_at_peace_while_the_equipment_holds() {
        let mut f = game();
        f.replies
            .insert("hoi4_wars", json!({"wars": {"at_war": false}}));
        f.replies.insert(
            "hoi4_armies",
            json!({"armies": [
                {"army": "Armee Nord", "exercising": false, "division_count": 12},
                {"army": "Armee Sued", "exercising": true, "division_count": 10},
                {"army": "Leer", "exercising": false, "division_count": 0}]}),
        );
        f.replies.insert(
            "hoi4_supply",
            json!({"divisions": [{"equipment_fill_min": 0.6}, {"equipment_fill_min": 0.8}]}),
        );
        let plan = json!({});
        let mut state = State::default();
        // fill 0.7 < 0.9: waits, no input
        assert_eq!(step(&mut state, &f, &plan, "l", START).unwrap(), None);
        assert!(state.seams.notes["exercises"].contains("below 0.90"));
        f.replies.insert(
            "hoi4_supply",
            json!({"divisions": [{"equipment_fill_min": 1.0}, {"equipment_fill_min": 0.95}]}),
        );
        let out = step(&mut state, &f, &plan, "l", START + 15 * DAY)
            .unwrap()
            .unwrap();
        assert!(out.contains("exercises start 1 armies: true"), "{out}");
        let sent = actions(&f);
        assert_eq!(sent[0].1["armies"], json!(["Armee Nord"]));
        assert_eq!(sent[0].1["stop_at_max"], json!(true));
        // a war: the armies still exercising are stopped once
        f.replies
            .insert("hoi4_wars", json!({"wars": {"at_war": true}}));
        let out = step(&mut state, &f, &plan, "l", START + 30 * DAY)
            .unwrap()
            .unwrap();
        assert!(out.contains("exercises stopped at war: true"), "{out}");
        assert_eq!(actions(&f)[1].1["on"], json!(false));
        // switched off by the plan
        let off = json!({"manage": {"off": ["exercises"]}});
        let mut state = State::default();
        f.replies
            .insert("hoi4_wars", json!({"wars": {"at_war": false}}));
        assert_eq!(step(&mut state, &f, &off, "l", START).unwrap(), None);
    }

    #[test]
    fn exercises_start_at_peace_for_the_named_armies_and_stop_when_a_war_starts() {
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
        let out = step(&mut state, &f, &plan, "l", START).unwrap().unwrap();
        assert!(out.contains("exercise drill: start: true"), "{out}");
        assert!(state.done.contains_key("ex:drill:on"));
        let sent = actions(&f);
        assert_eq!(sent.len(), 1);
        assert_eq!(sent[0].0, "hoi4_army_exercise");
        assert_eq!(sent[0].1["armies"], json!(["Armia Poznan"]));
        assert_eq!(sent[0].1["on"], json!(true));
        assert_eq!(sent[0].1["stop_at_max"], json!(true));
        // the next day: the running entry is left alone, the other still waits
        step(&mut state, &f, &plan, "l", START + DAY).unwrap();
        assert!(state.seams.notes["xp_budget.army:later"].starts_with("waits, when"));
        assert!(!state.seams.notes.contains_key("xp_budget.army:t"));
        assert_eq!(actions(&f).len(), 1);
        // a war starts: the running entry stops once
        f.replies.insert(
            "hoi4_wars",
            json!({"wars": {"at_war": true, "enemies": ["GER"]}}),
        );
        let out = step(&mut state, &f, &plan, "l", START + 3 * DAY)
            .unwrap()
            .unwrap();
        assert!(out.contains("exercise drill: stop: true"), "{out}");
        let sent = actions(&f);
        assert_eq!(sent.len(), 2);
        assert_eq!(sent[1].1["on"], json!(false));
        assert!(sent[1].1.get("stop_at_max").is_none());
        assert!(state.done.contains_key("ex:drill:off"));
        // at war nothing new starts
        let mut state = State::default();
        step(&mut state, &f, &plan, "l", START).unwrap();
        assert_eq!(
            state.seams.notes["xp_budget.army:later"],
            "at war: no exercise started"
        );
    }

    #[test]
    fn priority_lines_go_to_the_top_last_first_and_a_new_line_starts_a_new_round() {
        let mut f = game();
        let lines = |extra: bool| {
            let mut l = vec![
                json!({"entry_index": 0, "line_handle": "0xa", "equipment_id": "fighter_equipment_0"}),
                json!({"entry_index": 1, "line_handle": "0xb", "equipment_id": "artillery_equipment_1"}),
                json!({"entry_index": 2, "line_handle": "0xc", "equipment_id": "infantry_equipment_1"}),
            ];
            if extra {
                l.push(json!({"entry_index": 3, "line_handle": "0xd", "equipment_id": "infantry_equipment_2"}));
            }
            json!({"lines": l})
        };
        f.replies.insert("hoi4_production_lines", lines(false));
        let plan = json!({"production_priority": ["infantry_equipment", "artillery_equipment"]});
        let round = priority_round(
            list(&lines(true), "lines"),
            &strings(&plan, "production_priority"),
        );
        let order: Vec<&str> = round.iter().map(|(_, h, _)| h.as_str()).collect();
        assert_eq!(order, ["0xc", "0xd", "0xb"]);
        let mut state = State::default();
        let mut t = START;
        let mut out = Vec::new();
        for _ in 0..4 {
            if let Some(line) = step(&mut state, &f, &plan, "l", t).unwrap() {
                out.push(line);
            }
            t += DAY;
        }
        let sent = actions(&f);
        assert_eq!(sent.len(), 2, "{out:?}");
        assert!(sent
            .iter()
            .all(|(n, a)| n == "hoi4_production_priority" && a["to"] == "top"));
        // the last of the round first, so the first key ends on top
        assert_eq!(sent[0].1["equipment"], "artillery_equipment_1");
        assert_eq!(sent[1].1["equipment"], "infantry_equipment_1");
        assert!(state.seams.notes["production_priority"].contains("on top"));
        // a new rifle line: a new round of three
        f.replies.insert("hoi4_production_lines", lines(true));
        for _ in 0..12 {
            step(&mut state, &f, &plan, "l", t).unwrap();
            t += DAY;
        }
        let sent = actions(&f);
        assert_eq!(sent.len(), 5);
        assert_eq!(sent[4].1["line"], json!(2));
    }

    #[test]
    fn a_round_moves_only_the_lines_out_of_place_and_skips_removed_ones() {
        let line = |i: u64, h: &str, e: &str| json!({"entry_index": i, "line_handle": h, "equipment_id": e});
        let keys = vec![
            "infantry_equipment".to_string(),
            "artillery_equipment".to_string(),
            "support_equipment".to_string(),
        ];
        // already in order on top: no move
        let lines = vec![
            line(0, "0xa", "infantry_equipment_1"),
            line(1, "0xb", "artillery_equipment_1"),
            line(2, "0xc", "support_equipment_1"),
            line(3, "0xd", "ship_hull_light_2"),
        ];
        let round = priority_round(&lines, &keys);
        assert_eq!(moves_needed(&lines, &round), 0);
        // only the rifles fell below: one move
        let lines = vec![
            line(0, "0xb", "artillery_equipment_1"),
            line(1, "0xc", "support_equipment_1"),
            line(2, "0xa", "infantry_equipment_1"),
        ];
        let round = priority_round(&lines, &keys);
        assert_eq!(moves_needed(&lines, &round), 1);
        // the support line first: art and rifles move
        let lines = vec![
            line(0, "0xc", "support_equipment_1"),
            line(1, "0xa", "infantry_equipment_1"),
            line(2, "0xb", "artillery_equipment_1"),
        ];
        let round = priority_round(&lines, &keys);
        assert_eq!(moves_needed(&lines, &round), 2);
        // another line above all of them: all three move
        let lines = vec![
            line(0, "0xd", "ship_hull_light_2"),
            line(1, "0xa", "infantry_equipment_1"),
            line(2, "0xb", "artillery_equipment_1"),
            line(3, "0xc", "support_equipment_1"),
        ];
        let round = priority_round(&lines, &keys);
        assert_eq!(moves_needed(&lines, &round), 3);
        // a removed hull is never ordered
        let mut f = game();
        f.replies.insert(
            "hoi4_production_lines",
            json!({"lines": [
            line(0, "0xa", "infantry_equipment_1"), line(1, "0xh", "ship_hull_heavy_2")]}),
        );
        let plan = json!({"production_priority": ["infantry_equipment", "ship_hull_heavy"],
            "production": [{"equipment": "ship_hull_heavy_2", "remove": true}]});
        let mut state = State::default();
        assert_eq!(step(&mut state, &f, &plan, "l", START).unwrap(), None);
        assert!(actions(&f).is_empty());
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

    fn cp_game(cp: f64) -> Fake {
        let mut f = game();
        f.replies.insert(
            "hoi4_country_status",
            json!({"fields": {"command_power": {"value": cp}}}),
        );
        f.replies.insert(
            "hoi4_armies",
            json!({"armies": [
                {"army": "Armee 1", "commander": "Erwin Rommel", "division_count": 10},
                {"army": "Armee 2", "commander": "Gerd von Rundstedt", "division_count": 24},
                {"army": "Armee 3", "commander": "Gerd von Rundstedt", "division_count": 2},
                {"army": "Armee 4", "division_count": 30}]}),
        );
        f
    }

    #[test]
    fn the_trait_tool_is_built() {
        assert!(built(TRAIT_TOOL));
    }

    #[test]
    fn command_power_is_spent_near_its_cap_and_the_cap_comes_from_a_tooltip() {
        assert!(cp_should_spend(60.0, 80.0));
        assert!(cp_should_spend(64.0, 80.0));
        assert!(!cp_should_spend(59.0, 80.0));
        assert!(cp_should_spend(85.0, 100.0));
        assert!(!cp_should_spend(10.0, 20.0), "below one trait's cost");
        assert_eq!(slash_max("Command Power: 63.5 / 100"), Some(100.0));
        assert_eq!(slash_max("{\"text\":\"a/b 5 / 120.\"}"), Some(120.0));
        assert_eq!(slash_max("no cap here"), None);
        assert_eq!(slash_max("and/or"), None);
        let armies = list(&cp_game(0.0).replies["hoi4_armies"], "armies").to_vec();
        assert_eq!(
            generals_by_divisions(&armies),
            ["Gerd von Rundstedt", "Erwin Rommel"]
        );
    }

    #[test]
    fn command_power_buys_the_plans_traits_first_then_for_the_largest_command() {
        let f = cp_game(70.0);
        let plan = json!({"commanders": [{"general": "Erwin Rommel", "traits": ["panzer_expert"]}],
            "manage": {"cp_trait_prefer": ["logistics_wizard"]}});
        let mut state = State::default();
        let out = step(&mut state, &f, &plan, "l", START).unwrap().unwrap();
        assert!(
            out.contains("trait for Erwin Rommel:panzer_expert: true"),
            "{out}"
        );
        let sent = actions(&f);
        assert_eq!(sent[0].0, TRAIT_TOOL);
        assert_eq!(sent[0].1["general"], json!("Erwin Rommel"));
        assert_eq!(sent[0].1["trait"], json!("panzer_expert"));
        assert!(state.done.contains_key("cps:Erwin Rommel:panzer_expert"));
        // one trait a week
        assert_eq!(step(&mut state, &f, &plan, "l", START + DAY).unwrap(), None);
        assert_eq!(actions(&f).len(), 1);
        // the plan's list is done: the general leading the most divisions, preferences passed on
        let out = step(&mut state, &f, &plan, "l", START + 8 * DAY)
            .unwrap()
            .unwrap();
        assert!(out.contains("trait for Gerd von Rundstedt: true"), "{out}");
        let sent = actions(&f);
        assert_eq!(sent[1].1["general"], json!("Gerd von Rundstedt"));
        assert_eq!(sent[1].1["prefer"], json!(["logistics_wizard"]));
        assert!(sent[1].1.get("trait").is_none());
    }

    #[test]
    fn command_power_below_the_threshold_is_kept_and_the_rule_can_be_switched_off() {
        let mut f = cp_game(50.0);
        f.replies
            .insert("hoi4_tooltip", json!({"text": "Command Power 50 / 120"}));
        let mut state = State::default();
        assert_eq!(step(&mut state, &f, &json!({}), "l", START).unwrap(), None);
        assert!(state.seams.notes["cp_spend"].contains("50 of 120: kept"));
        // the plan's cap without a tooltip
        let f = cp_game(50.0);
        let plan = json!({"manage": {"command_power_max": 60}});
        let mut state = State::default();
        assert!(step(&mut state, &f, &plan, "l", START).unwrap().is_some());
        assert_eq!(actions(&f).len(), 1);
        // switched off
        let f = cp_game(80.0);
        let off = json!({"manage": {"off": ["cp_spend"]}});
        let mut state = State::default();
        assert_eq!(step(&mut state, &f, &off, "l", START).unwrap(), None);
        assert!(actions(&f).is_empty());
        assert!(!state.seams.notes.contains_key("cp_spend"));
    }

    #[test]
    fn nothing_to_buy_wakes_the_player_and_short_power_waits_until_it_has_grown() {
        let mut f = cp_game(75.0);
        f.refusing.insert(
            TRAIT_TOOL,
            ("no_enabled_trait", "No assignable trait can be picked now"),
        );
        let plan = json!({});
        let mut state = State::default();
        // both generals are refused, a day apart
        let out = step(&mut state, &f, &plan, "l", START).unwrap().unwrap();
        assert!(
            out.contains("Gerd von Rundstedt: false no_enabled_trait"),
            "{out}"
        );
        let out = step(&mut state, &f, &plan, "l", START + DAY + 1)
            .unwrap()
            .unwrap();
        assert!(out.contains("Erwin Rommel: false"), "{out}");
        assert!(take_alarms(&mut state).is_empty());
        // then nothing is left to try: a note and one wake naming why
        assert_eq!(
            step(&mut state, &f, &plan, "l", START + 2 * DAY + 2).unwrap(),
            None
        );
        assert!(state.seams.notes["cp_spend"].contains("no enabled trait for any general"));
        let alarms = take_alarms(&mut state);
        assert_eq!(alarms.len(), 1);
        assert_eq!(alarms[0]["reason"], json!("cp_spend:nothing_to_buy"));
        assert_eq!(actions(&f).len(), 2);

        // short command power: the rule waits until it has grown (15 - 4 at 0.4 a day)
        let mut f = cp_game(75.0);
        f.refusing.insert(
            TRAIT_TOOL,
            (
                "insufficient_command_power",
                "A unit-leader trait costs 15 command power and 4 are held",
            ),
        );
        let mut state = State::default();
        assert!(step(&mut state, &f, &plan, "l", START).unwrap().is_some());
        assert_eq!(
            step(&mut state, &f, &plan, "l", START + 20 * DAY).unwrap(),
            None
        );
        assert_eq!(actions(&f).len(), 1);
        assert!(step(&mut state, &f, &plan, "l", START + 29 * DAY)
            .unwrap()
            .is_some());
    }
}
