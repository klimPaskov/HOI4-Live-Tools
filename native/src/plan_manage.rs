//! Continuous management: the standing rules a strong player runs all game long (coverage
//! audit section 4, rules 3, 10-13, 15-17, 19, 21, 23, 25, 35), one verified action at a time
//! through the same tools as every other plan step. A child module of `plan_run` (declared
//! there with `#[path]`), so it shares the runner's `State`, failure ledger and done-memory:
//! one-shot work is kept in `State::done` and the skip timers in `State::bad`, both persisted
//! in plan_state.json. All cadences are game time.
//!
//! Hooks in plan_run.rs: [`training_step`] is tick step 6, [`step`] runs right after the
//! extras step, [`peace`] replaces the stop when a peace conference opens, the extras step's
//! default air missions leave wings [`air_plan_claims`] names alone, and `run` adds
//! [`take_alarms`] to its `alarms` and [`digest`] as `manage`.
//!
//! Plan keys:
//!   training         {"template","queue":6,"batch":2,"deploy_state","when"?,"cap"?,"only_if_stockpile"?}
//!                    or a LIST of such objects (each template kept at `queue` training lines, the
//!                    first one short of its queue trains `batch`, one action per tick)
//!   training_rules   {"only_if_stockpile":{"<archetype>":n},"cap_template"?,"cap"?} for every training
//!                    entry. `cap`: never more than `cap` divisions of `cap_template` (every template when
//!                    absent) fielded (hoi4_supply) plus queued; an unreadable count waits.
//!                    `only_if_stockpile`: training waits while hoi4_stockpile holds less than n of
//!                    that archetype (or variant key). Without the read it is a proxy: training waits
//!                    while any fielded division lacks equipment of that archetype (the game
//!                    reinforces from the stockpile first, so it holds less than n); otherwise the
//!                    guard is unknown, training goes ahead and the digest says so
//!   templates        [{"id","name","base"?,"regiments","support"?,"priority"?,"xp"?,"when"?,
//!                    "upgrade_fielded"?,"regimental_support"?,"role"?}] division templates made once per
//!                    `id` (else per name): `base` names an existing template edited in place
//!                    (hoi4_template_edit, renamed to `name`; `upgrade_fielded` true sends allow_in_use);
//!                    otherwise a template `name` that does not exist is created (hoi4_template_create).
//!                    `regiments` is a list of columns (lists of battalion keys) or a flat list (runs of
//!                    the same key fill columns of 5). `priority` reserve/regular/elite. `xp` is the
//!                    army experience needed first (default for a create: 5 per battalion, 10 per
//!                    company, 25 per extra column, the tool's costs), read from hoi4_doctrines; an
//!                    entry short of XP waits and blocks the ones after it. `regimental_support`
//!                    (the 1.19 row, one company per column) is sent as the tools' argument of that
//!                    name once plan_run::UNBUILT_ARGS stops holding it back; until then it is
//!                    skipped with a note (gap register templates.regimental_support).
//!                    `role` "strike" / "assault" / "armour" / "mobile" puts the template on elite
//!                    reinforcement priority once a war with a `war.enemies` tag runs, "garrison" on
//!                    reserve (hoi4_template_edit priority only; no battalion changes), and the
//!                    entry's `priority` (else regular) returns at peace; `manage.war_priority`
//!                    false turns this off. Checked once a game day while an entry waits
//!   template_cleanup [{"template","rename_to"?,"when"?}] once a game day: delete (or rename) each
//!                    template once (`template_cleanup_step`); never one the plan trains or edits
//!   occupation       {"default_law"|"law","when"?,"ladder"?,"harsher_at_resistance":50,
//!                    "milder_at_compliance":40,"milder_below_resistance":25,"mildest"?,"harshest"?,
//!                    "change_days":30,"per_state"?} every 7 game days from hoi4_occupation (states
//!                    occupied by us or owned and resisting): the default law is set once when `when`
//!                    holds, then a state at resistance >= 50 moves the law one step harsher on
//!                    `ladder` (default: the laws hoi4_occupation lists as shown to the player, from the
//!                    game files, by the resistance target they set, mild to harsh), and every state at
//!                    compliance >= 40 with resistance < 25 one step milder, at most once per
//!                    `change_days`, through hoi4_occupation_law. `per_state` {state id: law} sets
//!                    the law of one occupied state once (hoi4_occupation_law {law, state}); that
//!                    argument is held back in plan_run::UNBUILT_ARGS until the per-state law button
//!                    is driven, and the note says so (gap register occupation.per_state_law); a
//!                    garrison template action does not exist
//!   peace            {"take":[state ids by rank],"puppet"?:[state ids],"liberate"?:[state ids],
//!                    "max_cost"?,"max_rounds":10} when the conference opens: hoi4_peace_options, then
//!                    one state per action through hoi4_peace_take (puppet:true / release:true for the
//!                    other lists), hoi4_peace_submit when the round's points are spent, hoi4_peace_exit
//!                    when nothing is left (or after 5 idle ticks with states still wanted, or after
//!                    `max_rounds`). Without `peace` the runner stops as before and wakes the player;
//!                    20 idle ticks, or an order the ledger refuses, stop and wake too. A country
//!                    tag in any list stands for the states it owns (hoi4_states {query: TAG}), in
//!                    the read's order, after the ids listed before it: the conference demands
//!                    state by state, and a puppet demand on all of a country's states puppets the
//!                    whole country. A state already wanted by an earlier list keeps that kind
//!   auto_import      {"budget":N,"enabled"?:true} (or N, or true): every 7 game days a resource deficit
//!                    on a line (hoi4_production_shortages; lines matching production_priority when it
//!                    is set; else hoi4_resources shortages) turns automatic importing on with the
//!                    budget through hoi4_auto_import, once per budget; a deficit that outlasts
//!                    `manage.deficit_wake_days` (14) with it on, or any deficit without auto_import
//!                    (or enabled:false), wakes the player. With a `budget`, a civilian factory count
//!                    that moved by `budget_step` (2) since the last check re-reads the view and
//!                    sends the budget again when the game has cut it (EC27, every 7 game days)
//!   air_plan         [{"wing"?|"airframe"+"version"?,"region","mission","base_state"?|"base_region"?,
//!                    "when"?,"switch_when"?,"then_region"?,"then_mission"?|"fallback_mission"?}] every 3
//!                    game days from hoi4_air: each matching wing (exact `wing`, else every wing of that
//!                    aircraft type and variant) is rebased (hoi4_air_rebase: to `base_state`, or to the
//!                    base of `base_region` with the most free room when its own base is outside that
//!                    region or over capacity) and then flies `mission` over `region` (hoi4_air_mission);
//!                    once `switch_when` holds, over `then_region` with `then_mission`. Wings the air plan
//!                    names are left out of `air.war_missions`.
//!                    `follow_region` true: the wing is based inside the region it flies over now
//!                    (`region`, or `then_region` once switched), so a mission region that moves with
//!                    the front takes its wings along: own base there with the most free room; with
//!                    none, `base_state` / `base_region` as above.
//!                    `deploy` true or {"min_planes":50,"max_wings"?,"wing_size":100} (needs
//!                    `airframe`): reserve planes of the entry's airframe (hoi4_stockpile variants)
//!                    beyond what its wings below `wing_size` draw as reinforcements found a new wing
//!                    (hoi4_air_wing_create, at most 100 a wing) at `base_state`, else at the base of
//!                    the mission region (`follow_region`) or `base_region` with the most free room,
//!                    once at least `min_planes` wait and fewer than `max_wings` wings match; the new
//!                    wing matches the entry and gets its mission on the next tick
//!   politics_auto    {"enabled":true,"threshold":100,"cap":2000,"cap_margin":30,"order"?,"never"?,
//!                    "recheck_days":30,"recheck_pp":25,"idle_wake_days":30} or false (rule 4, run by
//!                    the politics domain after the plan's `politics` picks): political power at or
//!                    above `threshold` beyond the reserve (the cost of each pending pick that could
//!                    be bought now: its `when` holds and it is not backing off after a refusal), or
//!                    within `cap_margin` of `cap`, buys the first affordable entry of `order`
//!                    [{"category","slot","ideas"?,"cost"?,"when"?,"why"?}] the game enables (`ideas`
//!                    ranked best first, only above the current idea; without `ideas` a vacant slot
//!                    takes the first enabled idea). A slot whose wanted idea was disabled is read
//!                    again once the power has grown by `recheck_pp`. Default order, naming no game
//!                    key: every vacant military staff seat (`politics_auto_default`); laws and
//!                    political advisors only when the order names them; conscription only within
//!                    manage.conscription_laws; `never` ideas are skipped. Every entry passed over
//!                    says why in the digest. Wakes: `political_power_at_cap`, `political_power_idle`
//!   manage           thresholds of the standing rules (all optional; the rules of this object run
//!                    only when the plan has it, the resource rule also with `auto_import`, the alerts
//!                    and the ledger always):
//!                    "off":["alerts","ledger","manpower","stability","equipment_fill","resources",
//!                    "stockpile","trucks","overdrawn","convoys","fuel"] switches rules off;
//!                    "convoys_free":0.1 free convoys (hoi4_convoys) below that share of the stock, or a
//!                    need above the stock, put idle dockyards on the convoy line (or a new line of
//!                    the newest convoy design), every 3 game days; no idle dockyard wakes the player;
//!                    "fuel_floor":0.25 fuel below that share of the capacity (hoi4_resources) turns
//!                    auto-import on, once a game day; no auto_import, or fuel still low after
//!                    "deficit_wake_days", wakes the player; "wake_repeat_days":7 one wake per reason;
//!                    "alerts_ignore":[alert names]; "alert_persist_days":14;
//!                    "manpower_months":2 free manpower below that many months of losses (hoi4_war_stats)
//!                    takes the next enabled law of "conscription_laws" in government slot 0 (the next
//!                    listed after the current law, else the first listed one harsher than it in the
//!                    order the game's window lists the laws); with no list (the default) it never
//!                    raises conscription, which costs factory output, and wakes the player instead;
//!                    "stability_floor":0.5 wakes below it;
//!                    "equipment_fill_floor":0.8 an army whose divisions' mean equipment fill
//!                    (hoi4_supply) is below it wakes the player and nudges `disband`;
//!                    stockpile and trucks (rules 11 and 21, once a game day from hoi4_stockpile):
//!                    every `stockpile_targets` entry {"<archetype or variant>": days} (default
//!                    infantry_equipment at "stockpile_days_warning":30) whose days of cover fall
//!                    below its days, and trucks (motorized_equipment) below the warning days or
//!                    below "trucks_ratio":1.5 x what the divisions need, raise that equipment's
//!                    production line by `rebalance_step` idle military factories (a new line
//!                    when none builds it); with no idle factory, cover below
//!                    "stockpile_days_critical":7 wakes the player. A stock that is not falling,
//!                    or one whose trend the server has not measured yet, is left alone;
//!                    "deficit_wake_days":14;
//!                    overdrawn (supply.split_overdrawn, once a game day from hoi4_supply): in an
//!                    overdrawn province whose divisions' mean supply is below "overdrawn_floor":35
//!                    the weakest divisions of the army with most divisions there (at most
//!                    "overdrawn_share":0.5 of them) are detached until their supply need covers the
//!                    shortfall and moved to a province with room; their army rests
//!                    "overdrawn_rest_days":10 before the war step or consolidation take it back.
//!                    These keys act whether or not the plan has `manage` (defaults shown):
//!                    "rebalance_step":3 idle military factories per action to a priority line (tick
//!                    step 6b, `production_priority` in plan_run.rs: the line whose equipment the
//!                    divisions lack most, from hoi4_stockpile fielded_short, else hoi4_supply; with no
//!                    shortage the line with the fewest days of stockpile cover, else the lines in
//!                    turn; the line's read efficiency goes in the note); "day0_production":false skips the
//!                    day-0 assignment of free military factories to the kept starting lines;
//!                    "war_priority":false (templates above); "war_civs":true keeps industrial_complex
//!                    jobs of `construction` at war (plan_run.rs); "mature_line_efficiency":30 (percent)
//!                    and, without an efficiency read, "mature_line_days":30
//!                    (plan_affairs.rs, the mature-line guard); "invasion_base_days":60,
//!                    "invasion_base_divisions":4, "invasion_techs" (plan_military.rs, invasions)
//!
//! Standing rules without a plan key: the game's alert row (hoi4_alerts, once a game day, rule
//! 3) nudges the step that fixes each alert by clearing its skip timer, and wakes the player for
//! the alerts that need judgement, the unknown ones, the ones the plan has no key for and the ones
//! that persist; an order the failure ledger refuses (3 failures in 10 game days, rule 35) wakes
//! the player with the order. A wake is the alarm {"code":"wake_player","rule","reason","message",
//! "evidence"} in the run result, once per reason per `manage.wake_repeat_days`.
//!
//! `when` / `switch_when` go through one function, `when_holds`: a `crate::plan_predicate`
//! predicate (dates, stability, war support, wars, state control, focuses, ...) judged with the
//! tick's reads; an atom that cannot be read is unknown and the item waits.
use super::{item_id, list, skip_key, strings, text, GameHour, State, Tick, Tools, DAY};
use crate::error::Result;
use serde_json::{json, Map, Value};
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

/// What the runner keeps for these rules besides `done` and the skip timers.
#[derive(Default)]
pub(super) struct Memory {
    /// Wake alarms raised since the last run result.
    alarms: Vec<Value>,
    /// The last status line of each rule (the run result's `manage`).
    notes: BTreeMap<String, String>,
    /// The open peace conference.
    peace: Option<Conference>,
    /// hoi4_supply, read at most once a game day.
    supply: Option<(GameHour, Value)>,
    /// hoi4_stockpile, read at most once a game day.
    stockpile: Option<(GameHour, Value)>,
    /// (game hour, own casualties so far) samples for the manpower rule.
    losses: Vec<(GameHour, f64)>,
    /// When the current resource deficit was first seen.
    deficit_since: Option<GameHour>,
    /// Alert name -> the game hour it was first seen in the current unbroken run of days.
    alert_since: HashMap<String, GameHour>,
    /// The next priority line in turn for idle factories when no shortage is known.
    feed_turn: usize,
    /// Since when political power above the threshold found nothing to buy (`politics_auto`).
    pp_idle_since: Option<GameHour>,
    /// `politics_auto`: what the last options read of each order slot found.
    pp_reads: HashMap<String, SlotRead>,
    /// `politics_auto`: per category, when its seats were last listed and the vacant ones.
    pp_vacant: HashMap<String, (GameHour, Vec<i64>)>,
    /// Since when the fuel stock has been below `manage.fuel_floor`.
    fuel_low_since: Option<GameHour>,
    /// Since when free convoys have been below `manage.convoys_free` of the stock.
    convoys_low_since: Option<GameHour>,
    /// Civilian factories (hoi4_economy) at the last auto-import budget check.
    import_civs: Option<i64>,
    /// Divisions pulled out of overdrawn provinces: their new army and where it goes.
    spread: Vec<Spread>,
}

/// An army formed from divisions pulled out of an overdrawn province (supply.split_overdrawn).
#[derive(Clone)]
struct Spread {
    army: String,
    state: i64,
    province: i64,
    moved: bool,
    since: GameHour,
}

/// What the runner did in the open peace conference.
#[derive(Default)]
struct Conference {
    taken: Vec<i64>,
    dropped: HashSet<i64>,
    /// The round's points are spent (a take was refused as too dear): submit next.
    spent: bool,
    rounds: u64,
    exited: bool,
    /// Ticks in a row the window offered nothing to do (the other victors' turn).
    waits: u32,
}

/// Ticks of waiting with states still wanted before the runner exits the conference; four
/// times as many with nothing usable at all stop the runner and wake the player.
const PEACE_PATIENCE: u32 = 5;

/// The states `tag` owns (hoi4_states {query: tag}), None when unreadable or not a tag.
fn tag_states(tools: &dyn Tools, tag: &str) -> Option<Vec<i64>> {
    let tag = tag.trim();
    if tag.len() != 3
        || !tag
            .bytes()
            .all(|b| b.is_ascii_uppercase() || b.is_ascii_digit())
    {
        return None;
    }
    let read = tools.call("hoi4_states", &json!({"query": tag})).ok()?;
    let rows = read["states"].as_array()?;
    Some(
        rows.iter()
            .filter(|r| r["owner"].as_str() == Some(tag))
            .filter_map(|r| r["state_id"].as_i64())
            .collect(),
    )
}

fn peace_wait(state: &mut State, plan: &Value, now: GameHour, why: &str) -> Tick {
    let conf = state.manage.peace.get_or_insert_with(Conference::default);
    conf.waits += 1;
    let waits = conf.waits;
    if waits < 4 * PEACE_PATIENCE {
        return Tick::Nothing;
    }
    wake(
        state,
        plan,
        now,
        25,
        "peace_stuck",
        format!("the peace conference offered nothing to do for {waits} ticks ({why}); the player finishes it"),
        json!({"waits": waits}),
    );
    Tick::Stop("peace conference open: nothing to do".into())
}

pub(super) use crate::plan_predicate::Truth;

/// Whether a plan item's `when` (or `switch_when`) holds now: missing or empty holds; anything
/// else is a `crate::plan_predicate` predicate judged with this tick's reads (`super::judge`):
/// an unknown atom is Unknown (the item waits) and is listed in the run's `predicates_unknown`.
fn when_holds(state: &mut State, tools: &dyn Tools, when: &Value, now: GameHour) -> Truth {
    match when {
        Value::Null => Truth::True,
        Value::String(s) if s.trim().is_empty() => Truth::True,
        _ => super::judge(state, tools, when, now),
    }
}

/// The wake alarms raised since the last call (the run result's `alarms`).
pub(super) fn take_alarms(state: &mut State) -> Vec<Value> {
    std::mem::take(&mut state.manage.alarms)
}

/// The last status of each rule (the run result's `manage`), or None when nothing ran.
pub(super) fn digest(state: &State) -> Option<Value> {
    (!state.manage.notes.is_empty()).then(|| json!(state.manage.notes))
}

/// The rule's status line in the run result's `manage` digest (the last one per rule).
pub(super) fn note(state: &mut State, rule: &str, line: impl Into<String>) {
    state.manage.notes.insert(rule.to_string(), line.into());
}

fn off(plan: &Value, rule: &str) -> bool {
    strings(&plan["manage"], "off").iter().any(|r| r == rule)
}

/// A fraction threshold: 0.5 or 50 (percent) both mean one half.
fn fraction(v: &Value, default: f64) -> f64 {
    v.as_f64()
        .map_or(default, |f| if f > 1.0 { f / 100.0 } else { f })
}

fn round3(v: f64) -> f64 {
    (v * 1000.0).round() / 1000.0
}

/// Raise a `wake_player` alarm once per `reason` per `manage.wake_repeat_days` (7).
fn wake(
    state: &mut State,
    plan: &Value,
    now: GameHour,
    rule: u8,
    reason: &str,
    message: String,
    evidence: Value,
) {
    if state.skipped(now, "wake", reason) {
        return;
    }
    let days = plan["manage"]["wake_repeat_days"]
        .as_i64()
        .filter(|d| *d > 0)
        .unwrap_or(7);
    state.skip(now, "wake", reason, days * DAY);
    state.manage.alarms.retain(|a| a["reason"] != json!(reason));
    state.manage.alarms.push(
        json!({"code": "wake_player", "rule": rule, "reason": reason,
        "message": message, "evidence": evidence}),
    );
}

/// Clear the skip timers of these kinds so their step runs on the next tick.
fn nudge(state: &mut State, kinds: &[&str]) -> usize {
    let before = state.bad.len();
    state.bad.retain(|key, _| {
        !kinds
            .iter()
            .any(|kind| key.strip_prefix(kind).is_some_and(|r| r.starts_with(':')))
    });
    before - state.bad.len()
}

/// hoi4_supply, read at most once a game day (None when unreadable).
fn supply(state: &mut State, tools: &dyn Tools, now: GameHour) -> Option<Value> {
    if let Some((at, read)) = &state.manage.supply {
        if now - *at < DAY {
            return Some(read.clone());
        }
    }
    let read = tools.call("hoi4_supply", &json!({})).ok()?;
    state.manage.supply = Some((now, read.clone()));
    Some(read)
}

/// Archetypes the fielded divisions lack that no production line makes ("name (short n)").
fn unproduced_army_gaps(read: &Value) -> Vec<String> {
    list(read, "equipment")
        .iter()
        .filter(|row| {
            row["fielded_short"].as_f64().unwrap_or(0.0) > 0.0
                && row["production_lines"].as_i64().unwrap_or(0) == 0
        })
        .map(|row| {
            format!(
                "{} (short {:.0})",
                text(&row["archetype"]),
                row["fielded_short"].as_f64().unwrap_or(0.0)
            )
        })
        .collect()
}

/// hoi4_stockpile, read at most once a game day (None when unreadable or listing nothing).
fn stockpile(state: &mut State, tools: &dyn Tools, now: GameHour) -> Option<Value> {
    if let Some((at, read)) = &state.manage.stockpile {
        if now - *at < DAY {
            return Some(read.clone()).filter(|r| r["equipment"].is_array());
        }
    }
    let read = tools.call("hoi4_stockpile", &json!({})).ok()?;
    state.manage.stockpile = Some((now, read.clone()));
    Some(read).filter(|r| r["equipment"].is_array())
}

/// Everything after the extras step, at most one action: the alert row and the failure ledger
/// (no input), templates, occupation, resources, the air plan and the `manage` rules.
pub fn step(
    state: &mut State,
    tools: &dyn Tools,
    plan: &Value,
    lease: &str,
    now: GameHour,
) -> Result<Option<String>> {
    // the tick only gets here when no conference is open
    state.manage.peace = None;
    ledger_wake(state, plan, now);
    alerts(state, tools, plan, now);
    if let Some(done) = templates_step(state, tools, plan, lease, now)? {
        return Ok(Some(done));
    }
    if let Some(done) = template_cleanup_step(state, tools, plan, lease, now)? {
        return Ok(Some(done));
    }
    if let Some(done) = occupation_step(state, tools, plan, lease, now)? {
        return Ok(Some(done));
    }
    if let Some(done) = resources_step(state, tools, plan, lease, now)? {
        return Ok(Some(done));
    }
    if let Some(done) = import_budget_rule(state, tools, plan, lease, now)? {
        return Ok(Some(done));
    }
    if let Some(done) = fuel_rule(state, tools, plan, lease, now)? {
        return Ok(Some(done));
    }
    if let Some(done) = convoy_rule(state, tools, plan, lease, now)? {
        return Ok(Some(done));
    }
    if let Some(done) = air_plan_step(state, tools, plan, lease, now)? {
        return Ok(Some(done));
    }
    if let Some(done) = country_rules(state, tools, plan, lease, now)? {
        return Ok(Some(done));
    }
    if let Some(done) = stockpile_rule(state, tools, plan, lease, now)? {
        return Ok(Some(done));
    }
    if let Some(done) = overdrawn_rule(state, tools, plan, lease, now)? {
        return Ok(Some(done));
    }
    equipment_rule(state, tools, plan, now);
    Ok(None)
}

// ---------------------------------------------------------------------------------------------
// Rule 35: an order refused 3 times in 10 game days wakes the player.

fn ledger_wake(state: &mut State, plan: &Value, now: GameHour) {
    if off(plan, "ledger") {
        return;
    }
    let stuck: Vec<(String, Vec<GameHour>)> = state
        .failures
        .iter()
        .filter(|(_, times)| times.iter().filter(|t| now - **t < 10 * DAY).count() >= 3)
        .map(|(order, times)| (order.clone(), times.clone()))
        .collect();
    for (order, times) in stuck {
        let mut reason = format!("refused:{order}");
        let mut end = reason.len().min(120);
        while !reason.is_char_boundary(end) {
            end -= 1;
        }
        reason.truncate(end);
        wake(
            state,
            plan,
            now,
            35,
            &reason,
            format!(
                "the same order failed {} times in 10 game days and is no longer sent",
                times.len()
            ),
            json!({"order": order, "failed_at_hours": times}),
        );
    }
}

// ---------------------------------------------------------------------------------------------
// Rule 3: the game's alert row.

enum Route {
    /// Nothing to do.
    Info,
    /// (step, skip kinds cleared, plan keys the step needs)
    Step(
        &'static str,
        &'static [&'static str],
        &'static [&'static str],
    ),
    /// The player decides (why).
    Wake(&'static str),
}

/// Which step fixes an alert of `hoi4_alerts` (the names of `alerts.rs`).
fn route(alert: &str) -> Route {
    match alert {
        "at_war" => Route::Info,
        "no_national_focus" => Route::Step("focus", &["focus"], &["focus_path", "focus_fallback"]),
        "missing_equipment_production" => Route::Step(
            "production",
            &["prod", "prodset"],
            &["production", "production_priority"],
        ),
        "modify_government" => Route::Step("politics", &["pol"], &["politics"]),
        "unfulfilled_import_requests" | "insufficient_resources" | "insufficient_energy" => {
            Route::Step("resources", &["mres"], &["auto_import"])
        }
        "air_wings_without_mission" => Route::Step(
            "air",
            &["aircheck", "airwing", "mair", "airplan"],
            &["air", "air_plan"],
        ),
        "no_divisions_in_training" => Route::Step("training", &["train"], &["training"]),
        "resistance" => Route::Step("occupation", &["mocc"], &["occupation"]),
        "unassigned_divisions" => Route::Step("armies", &["armycreate"], &["armies"]),
        "decisions_available" => Route::Step("decisions", &[], &["decisions", "decision_rules"]),
        "doctrines_available" => Route::Step("doctrines", &["doccheck"], &["doctrines"]),
        "not_enough_garrison" => Route::Wake(
            "garrisons are short of manpower or equipment; no garrison-template action exists yet",
        ),
        "free_dockyards" => Route::Wake("dockyards are idle: which ships to build is a decision"),
        "events_or_decisions_timeout" => Route::Wake("an event or decision is about to time out"),
        "modify_officer_corps" => Route::Wake("a high command or military advisor can be picked"),
        "mio_traits" => Route::Wake("military industrial organisation traits can be unlocked"),
        "admiral_assignable" => Route::Wake("an admiral can be assigned to a naval headquarters"),
        _ => Route::Wake("an alert the executor does not know"),
    }
}

fn alerts(state: &mut State, tools: &dyn Tools, plan: &Value, now: GameHour) {
    if off(plan, "alerts") || state.skipped(now, "malerts", "x") {
        return;
    }
    state.skip(now, "malerts", "x", DAY);
    let Ok(read) = tools.call("hoi4_alerts", &json!({})) else {
        note(state, "alerts", "hoi4_alerts unreadable");
        return;
    };
    let ignore = strings(&plan["manage"], "alerts_ignore");
    let persist = plan["manage"]["alert_persist_days"]
        .as_i64()
        .filter(|d| *d > 0)
        .unwrap_or(14);
    let mut seen: HashSet<String> = HashSet::new();
    let mut lines: Vec<String> = Vec::new();
    for alert in list(&read, "alerts") {
        let frame = alert["frame"].as_i64().unwrap_or(-1);
        let name = alert["alert"]
            .as_str()
            .map_or_else(|| format!("frame_{frame}"), String::from);
        if !seen.insert(name.clone()) || ignore.contains(&name) {
            continue;
        }
        let since = *state.manage.alert_since.entry(name.clone()).or_insert(now);
        let evidence =
            json!({"alert": name, "frame": frame, "todo": alert["todo"], "seen_since_hour": since});
        let reason = format!("alert:{name}");
        match route(&name) {
            Route::Info => lines.push(format!("{name}: info")),
            Route::Wake(why) => {
                lines.push(format!("{name}: wake"));
                wake(
                    state,
                    plan,
                    now,
                    3,
                    &reason,
                    format!("game alert {name}: {why}"),
                    evidence,
                );
            }
            Route::Step(step, kinds, needs) => {
                if needs.iter().all(|key| plan[*key].is_null()) {
                    lines.push(format!("{name}: wake (no {})", needs.join("/")));
                    wake(
                        state,
                        plan,
                        now,
                        3,
                        &reason,
                        format!(
                            "game alert {name}: the plan has no {} for the {step} step",
                            needs.join(" or ")
                        ),
                        evidence,
                    );
                    continue;
                }
                let cleared = nudge(state, kinds);
                lines.push(format!("{name}: {step} step nudged ({cleared} timers)"));
                if name == "missing_equipment_production" && now - since >= persist * DAY {
                    // The game raises it for equipment that reinforcements (divisions, garrisons,
                    // air wings) or queued divisions request and no line makes. Army gaps are
                    // read (hoi4_stockpile) and owned by the stockpile rule; the others are not
                    // read, so nudging the production step never closes them (n1, 16 Nov 2026:
                    // open 230 days). Say which case it is, once every 60 game days.
                    let gaps = stockpile(state, tools, now)
                        .map(|read| unproduced_army_gaps(&read))
                        .unwrap_or_default();
                    if !state.skipped(now, "megap", "x") {
                        state.skip(now, "megap", "x", 60 * DAY);
                        let why = if gaps.is_empty() {
                            "every equipment the divisions lack has a production line, so the request \
                             comes from garrison or air reinforcement or a queued division, which the \
                             server does not read: hover the alert (its tooltip lists the equipment), add \
                             a line with hoi4_production_add, or put the alert in manage.alerts_ignore \
                             with the reason"
                                .to_string()
                        } else {
                            format!(
                                "the divisions lack equipment no line makes: {}; add a line with \
                                 hoi4_production_add or a production entry",
                                gaps.join(", ")
                            )
                        };
                        wake(
                            state,
                            plan,
                            now,
                            3,
                            &reason,
                            format!(
                                "game alert {name} has stayed {} game days: {why}",
                                (now - since) / DAY
                            ),
                            evidence,
                        );
                    }
                    continue;
                }
                if now - since >= persist * DAY {
                    wake(
                        state,
                        plan,
                        now,
                        3,
                        &reason,
                        format!(
                            "game alert {name} has stayed {} game days although the {step} step was nudged",
                            (now - since) / DAY
                        ),
                        evidence,
                    );
                }
            }
        }
    }
    state
        .manage
        .alert_since
        .retain(|name, _| seen.contains(name));
    let line = if lines.is_empty() {
        "no alerts".to_string()
    } else {
        lines.join("; ")
    };
    note(state, "alerts", line);
}

// ---------------------------------------------------------------------------------------------
// Rule 12 / W1-4: training, one template or a list, with training_rules.

/// Tick step 6: keep each training entry's template at its queue (one action per tick).
pub(super) fn training_step(
    state: &mut State,
    tools: &dyn Tools,
    plan: &Value,
    lease: &str,
    now: GameHour,
) -> Result<Option<String>> {
    let entries: Vec<&Value> = match &plan["training"] {
        entry @ Value::Object(_) => vec![entry],
        Value::Array(items) => items.iter().filter(|v| v.is_object()).collect(),
        _ => return Ok(None),
    };
    if entries.is_empty() {
        return Ok(None);
    }
    let rules = &plan["training_rules"];
    let templates = tools.call("hoi4_division_templates", &json!({}))?;
    'entries: for tr in entries {
        let template = text(&tr["template"]).to_string();
        if template.is_empty() || state.skipped(now, "train", &template) {
            continue;
        }
        let Some(have) = list(&templates, "templates")
            .iter()
            .find(|t| text(&t["template"]) == template)
            .and_then(|t| t["queued_lines"].as_u64())
        else {
            continue;
        };
        let queue = tr["queue"].as_u64().unwrap_or(6);
        if have >= queue || when_holds(state, tools, &tr["when"], now) != Truth::True {
            continue;
        }
        let mut count = tr["batch"].as_u64().unwrap_or(2).min(queue - have);
        let cap = tr["cap"].as_u64().or_else(|| {
            rules["cap_template"]
                .as_str()
                .is_none_or(|t| t == template)
                .then(|| rules["cap"].as_u64())
                .flatten()
        });
        let mut guards: Vec<(String, u64)> = Vec::new();
        for source in [&rules["only_if_stockpile"], &tr["only_if_stockpile"]] {
            if let Some(map) = source.as_object() {
                for (archetype, n) in map {
                    guards.push((archetype.clone(), n.as_u64().unwrap_or(0)));
                }
            }
        }
        let mut said: Vec<String> = Vec::new();
        if cap.is_some() || !guards.is_empty() {
            let read = supply(state, tools, now);
            if let Some(cap) = cap {
                let Some(fielded) = read.as_ref().map(|s| {
                    list(s, "divisions")
                        .iter()
                        .filter(|d| text(&d["template"]) == template)
                        .count() as u64
                }) else {
                    note(
                        state,
                        "training",
                        format!("{template}: waits, cap {cap} but hoi4_supply is unreadable"),
                    );
                    state.skip(now, "train", &template, DAY);
                    continue;
                };
                let room = cap.saturating_sub(fielded + have);
                if room == 0 {
                    note(
                        state,
                        "training",
                        format!(
                            "{template}: at its cap ({fielded} fielded + {have} queued >= {cap})"
                        ),
                    );
                    state.skip(now, "train", &template, DAY);
                    continue;
                }
                count = count.min(room);
            }
            let stock = if guards.is_empty() {
                None
            } else {
                stockpile(state, tools, now)
            };
            for (archetype, n) in &guards {
                let held = stock.as_ref().and_then(|s| {
                    let row = archetype_row(s, archetype)?;
                    list(s, "equipment")
                        .iter()
                        .find(|r| text(&r["archetype"]) == row)?["stockpile"]
                        .as_f64()
                });
                if let Some(held) = held {
                    if held < *n as f64 {
                        note(
                            state,
                            "training",
                            format!("{template}: waits, {archetype} stockpile {held:.0} below {n}"),
                        );
                        state.skip(now, "train", &template, DAY);
                        continue 'entries;
                    }
                    said.push(format!("{archetype} {held:.0} >= {n}"));
                    continue;
                }
                match read.as_ref().map(|s| stockpile_guard(s, archetype)) {
                    Some(Truth::False) => {
                        note(state, "training", format!("{template}: waits, fielded divisions lack {archetype} (stockpile below {n})"));
                        state.skip(now, "train", &template, DAY);
                        continue 'entries;
                    }
                    _ => said.push(format!("{archetype} >= {n} unknown (no stockpile read)")),
                }
            }
        }
        let (ok, why) = state.act(
            tools,
            lease,
            "hoi4_train",
            json!({"template": template, "count": count, "deploy_state_id": tr["deploy_state"]}),
            "tr",
        )?;
        if !ok {
            state.skip(now, "train", &template, 2 * DAY);
        }
        if !said.is_empty() {
            note(
                state,
                "training",
                format!("{template}: trained; {}", said.join(", ")),
            );
        }
        return Ok(Some(format!("train {count} {template}: {ok} {why}")));
    }
    Ok(None)
}

/// The `only_if_stockpile` proxy for one archetype: False while a fielded division lacks it
/// (the stockpile is drained into reinforcements first), else Unknown (the stockpile itself is
/// not readable).
fn stockpile_guard(supply: &Value, archetype: &str) -> Truth {
    let short = list(supply, "divisions").iter().any(|d| {
        list(d, "equipment").iter().any(|e| {
            text(&e["archetype"]).starts_with(archetype)
                && e["have"].as_f64().unwrap_or(0.0) < e["need"].as_f64().unwrap_or(0.0)
        })
    });
    if short {
        Truth::False
    } else {
        Truth::Unknown
    }
}

// ---------------------------------------------------------------------------------------------
// Rule 23 / W1-6: division templates by plan.

/// `regiments` as the template tools take it: columns of battalion keys. A flat list is cut
/// into columns by runs of the same key, five per column.
fn columns(v: &Value) -> Option<Value> {
    let items = v.as_array().filter(|a| !a.is_empty())?;
    if items.iter().all(Value::is_array) {
        return Some(v.clone());
    }
    let keys: Vec<&str> = items.iter().filter_map(Value::as_str).collect();
    if keys.len() != items.len() {
        return None;
    }
    let mut out: Vec<Vec<&str>> = Vec::new();
    for key in keys {
        match out.last_mut() {
            Some(column) if column.len() < 5 && column[0] == key => column.push(key),
            _ => out.push(vec![key]),
        }
    }
    Some(json!(out))
}

/// Army experience a create costs (the tool's costs: 5 per battalion, 10 per company, 25 per
/// column after the first).
fn create_cost(regiments: &Value, support: usize) -> f64 {
    let cols: &[Value] = regiments.as_array().map_or(&[], Vec::as_slice);
    let battalions: usize = cols.iter().map(|c| c.as_array().map_or(0, Vec::len)).sum();
    (battalions * 5 + support * 10 + cols.len().saturating_sub(1) * 25) as f64
}

fn templates_step(
    state: &mut State,
    tools: &dyn Tools,
    plan: &Value,
    lease: &str,
    now: GameHour,
) -> Result<Option<String>> {
    let entries = list(plan, "templates");
    if entries.is_empty() {
        return Ok(None);
    }
    // At war the strike templates (role strike, assault, armour, mobile) take elite
    // reinforcement priority and the garrison templates reserve, once each; at peace again the
    // entry's own priority (else regular) returns. A priority-only edit changes no battalion.
    if plan["manage"]["war_priority"] != json!(false) && !state.skipped(now, "mwarprio", "x") {
        let mut war: Option<bool> = None;
        for entry in entries {
            let name = text(&entry["name"]).trim().to_string();
            let wanted = match text(&entry["role"]).trim().to_lowercase().as_str() {
                "strike" | "assault" | "armour" | "armor" | "mobile" => "elite",
                "garrison" => "reserve",
                _ => continue,
            };
            let key = format!("warprio:{name}");
            if name.is_empty() || state.skipped(now, "warprio", &name) {
                continue;
            }
            let at_war = *war.get_or_insert_with(|| at_war(state, tools, plan, now));
            let set = state.done.contains_key(&key);
            let priority = match (at_war, set) {
                (true, false) => wanted,
                (false, true) => entry["priority"].as_str().unwrap_or("regular"),
                _ => continue,
            };
            let (ok, why) = state.act(
                tools,
                lease,
                "hoi4_template_edit",
                json!({"template": name, "priority": priority}),
                "mq",
            )?;
            if ok || why.starts_with("nothing_to_change") {
                if at_war {
                    state.done.insert(key, now);
                } else {
                    state.done.remove(&key);
                }
            } else {
                state.skip(now, "warprio", &name, 7 * DAY);
            }
            let when = if at_war { "at war" } else { "peace" };
            note(
                state,
                "war_priority",
                format!("{name} -> {priority} ({when}): {ok}"),
            );
            return Ok(Some(format!(
                "template {name} reinforcement priority {priority}: {ok} {why}"
            )));
        }
        if war.is_some() {
            state.skip(now, "mwarprio", "x", DAY);
        }
    }
    if state.skipped(now, "mtpl", "x") {
        return Ok(None);
    }
    let mut have: Option<HashSet<String>> = None;
    let mut army_xp: Option<Option<f64>> = None;
    let mut waits: Vec<String> = Vec::new();
    let mut refused = false;
    for entry in entries {
        let name = text(&entry["name"]).trim().to_string();
        if name.is_empty() {
            continue;
        }
        let key =
            item_id(entry).map_or_else(|| format!("tpl:name:{name}"), |id| format!("tpl:id:{id}"));
        if state.done.contains_key(&key) {
            continue;
        }
        if state.skipped(now, "tpl", &key) {
            refused = true;
            continue;
        }
        if when_holds(state, tools, &entry["when"], now) != Truth::True {
            waits.push(format!("{name}: when not met"));
            continue;
        }
        if have.is_none() {
            let read = tools.call("hoi4_division_templates", &json!({}))?;
            have = Some(
                list(&read, "templates")
                    .iter()
                    .map(|t| text(&t["template"]).to_string())
                    .collect(),
            );
        }
        let existing = have.as_ref().map_or_else(HashSet::new, Clone::clone);
        let base = entry["base"]
            .as_str()
            .map(str::trim)
            .filter(|b| !b.is_empty());
        let edit =
            base.filter(|b| existing.contains(*b) && (*b == name || !existing.contains(&name)));
        if edit.is_none() && existing.contains(&name) {
            state.done.insert(key, now);
            continue;
        }
        let mut extra: Vec<String> = Vec::new();
        let mut regimental = strings(entry, "regimental_support");
        if regimental.is_empty() {
            regimental = strings(entry, "regimental");
        }
        if let (Some(b), None) = (base, edit) {
            extra.push(format!("base {b} not found, created new"));
        }
        let regiments = columns(&entry["regiments"]);
        let support: Vec<Value> = list(entry, "support")
            .iter()
            .filter(|s| s.is_string())
            .cloned()
            .collect();
        if edit.is_none() && regiments.is_none() {
            note(
                state,
                "templates",
                format!("{name}: no regiments to create it from"),
            );
            state.skip(now, "tpl", &key, 30 * DAY);
            continue;
        }
        let need = entry["xp"]
            .as_f64()
            .unwrap_or_else(|| match (&regiments, edit) {
                (Some(r), None) => create_cost(r, support.len()),
                _ => 0.0,
            });
        if need > 0.0 {
            if army_xp.is_none() {
                army_xp = Some(
                    tools
                        .call("hoi4_doctrines", &json!({}))
                        .ok()
                        .and_then(|d| d["experience"]["army"]["value"].as_f64()),
                );
            }
            // an unreadable XP leaves the check to the tool's own insufficient_experience
            if let Some(Some(xp)) = army_xp {
                if xp < need {
                    waits.push(format!("{name}: army XP {xp} < {need}"));
                    break;
                }
            }
        }
        let mut args = Map::new();
        let verb = if let Some(target) = edit {
            args.insert("template".into(), json!(target));
            if target != name {
                args.insert("new_name".into(), json!(name));
            }
            if entry["upgrade_fielded"] == json!(true) {
                args.insert("allow_in_use".into(), json!(true));
            }
            "edit"
        } else {
            args.insert("name".into(), json!(name));
            "create"
        };
        if let Some(r) = regiments {
            args.insert("regiments".into(), r);
        }
        if !support.is_empty() {
            args.insert("support".into(), json!(support));
        }
        if let Some(p) = entry["priority"].as_str() {
            args.insert("priority".into(), json!(p));
        }
        let tool = if verb == "edit" {
            "hoi4_template_edit"
        } else {
            "hoi4_template_create"
        };
        if !regimental.is_empty() {
            if super::arg_built(tool, "regimental_support") {
                args.insert("regimental_support".into(), json!(regimental));
            } else {
                extra.push(
                    "regimental support row skipped: filling it is not built yet \
                     (gap register templates.regimental_support)"
                        .into(),
                );
            }
        }
        let (ok, why) = state.act(tools, lease, tool, Value::Object(args), "tp")?;
        if ok || why.contains("nothing_to_change") || why.contains("name_taken") {
            state.done.insert(key, now);
        } else {
            state.skip(now, "tpl", &key, 7 * DAY);
        }
        if !extra.is_empty() {
            note(state, "templates", format!("{name}: {}", extra.join("; ")));
        }
        return Ok(Some(format!("template {verb} {name}: {ok} {why}")));
    }
    if !waits.is_empty() {
        note(state, "templates", waits.join("; "));
    }
    // everything done: weekly; an entry waiting (when, XP, a refusal's timer): daily
    let pending = refused || !waits.is_empty();
    state.skip(now, "mtpl", "x", if pending { DAY } else { 7 * DAY });
    Ok(None)
}

// ---------------------------------------------------------------------------------------------
// Rule 17 / W1-7: occupation law.

/// The default ladder, mild to harsh: the laws hoi4_occupation lists as shown to the player (the
/// game files' common/occupation_laws, mods included), ranked by the resistance target they
/// set, highest first (a law that cuts resistance more is harsher), list order within a tie.
fn occupation_ladder(read: &Value) -> Vec<String> {
    let mut shown: Vec<(f64, String)> = list(read, "laws")
        .iter()
        .filter(|l| l["shown"] == json!(true))
        .filter_map(|l| {
            let key = l["key"].as_str().filter(|k| !k.is_empty())?;
            Some((
                l["resistance_target"].as_f64().unwrap_or(0.0),
                key.to_string(),
            ))
        })
        .collect();
    shown.sort_by(|a, b| b.0.total_cmp(&a.0));
    shown.into_iter().map(|(_, key)| key).collect()
}

/// The template names the plan's own steps still use: training entries and `templates[]`
/// names and bases. Cleanup never deletes or renames one of them.
fn templates_in_use(plan: &Value) -> HashSet<String> {
    let training: Vec<&Value> = match &plan["training"] {
        entry @ Value::Object(_) => vec![entry],
        Value::Array(items) => items.iter().collect(),
        _ => Vec::new(),
    };
    training
        .iter()
        .map(|t| text(&t["template"]))
        .chain(
            list(plan, "templates")
                .iter()
                .flat_map(|t| [text(&t["name"]), text(&t["base"])]),
        )
        .map(|n| n.trim().to_string())
        .filter(|n| !n.is_empty())
        .collect()
}

/// `template_cleanup` [{"id"?,"template","rename_to"?,"when"?}] once a game day while an entry
/// is open: once `when` holds, `template` is renamed to `rename_to` (hoi4_template_rename), or
/// without it deleted (hoi4_template_delete). A template that is gone (or already carries the new
/// name) counts as done. One the plan's training or templates still name is never touched; one
/// with training lines queued waits; one the game refuses as used by divisions
/// (template_in_use) or whose new name is taken waits 30 game days and is tried again (a
/// disband or a reorganisation may free it). Each wait and its reason go in the digest
/// (`manage.template_cleanup`).
fn template_cleanup_step(
    state: &mut State,
    tools: &dyn Tools,
    plan: &Value,
    lease: &str,
    now: GameHour,
) -> Result<Option<String>> {
    let entries = list(plan, "template_cleanup");
    if entries.is_empty() || state.skipped(now, "mtclean", "x") {
        return Ok(None);
    }
    state.skip(now, "mtclean", "x", DAY);
    let used = templates_in_use(plan);
    let mut read: Option<Value> = None;
    let mut waits: Vec<String> = Vec::new();
    for (index, entry) in entries.iter().enumerate() {
        let template = text(&entry["template"]).trim().to_string();
        let key = item_id(entry).map_or_else(
            || format!("tclean:{index}:{template}"),
            |id| format!("tclean:id:{id}"),
        );
        if template.is_empty()
            || state.done.contains_key(&key)
            || state.skipped(now, "tclean", &key)
        {
            continue;
        }
        let rename = entry["rename_to"]
            .as_str()
            .map(str::trim)
            .filter(|n| !n.is_empty() && *n != template);
        if used.contains(&template) {
            waits.push(format!("{template}: the plan still trains or edits it"));
            continue;
        }
        if when_holds(state, tools, &entry["when"], now) != Truth::True {
            waits.push(format!("{template}: when not met"));
            continue;
        }
        if read.is_none() {
            read = Some(tools.call("hoi4_division_templates", &json!({}))?);
        }
        let rows = read.as_ref().map_or(&[][..], |r| list(r, "templates"));
        let row = |name: &str| rows.iter().find(|t| text(&t["template"]) == name);
        let Some(current) = row(&template) else {
            // gone already (or renamed already): nothing left to do
            state.done.insert(key, now);
            continue;
        };
        let queued = current["queued_lines"].as_u64().unwrap_or(0);
        if queued > 0 {
            waits.push(format!("{template}: {queued} training line(s) queued"));
            continue;
        }
        let (tool, args, what) = match rename {
            Some(new) if row(new).is_some() => {
                waits.push(format!("{template}: a template named {new} exists"));
                state.skip(now, "tclean", &key, 30 * DAY);
                continue;
            }
            Some(new) => (
                "hoi4_template_rename",
                json!({"template": template, "new_name": new}),
                format!("renamed to {new}"),
            ),
            None => (
                "hoi4_template_delete",
                json!({"template": template}),
                "deleted".to_string(),
            ),
        };
        let (ok, why) = state.act(tools, lease, tool, args, "mc")?;
        if ok {
            state.done.insert(key, now);
            // the next entry goes on the next tick
            state.bad.remove("mtclean:x");
        } else {
            let code = why.split(':').next().unwrap_or("refused").to_string();
            waits.push(format!("{template}: {code}"));
            state.skip(now, "tclean", &key, 30 * DAY);
        }
        note(
            state,
            "template_cleanup",
            format!("{template} {what}: {ok}"),
        );
        return Ok(Some(format!("template {template} {what}: {ok} {why}")));
    }
    if !waits.is_empty() {
        note(state, "template_cleanup", waits.join("; "));
    }
    Ok(None)
}

fn occupation_step(
    state: &mut State,
    tools: &dyn Tools,
    plan: &Value,
    lease: &str,
    now: GameHour,
) -> Result<Option<String>> {
    let occ = &plan["occupation"];
    if !occ.is_object() || state.skipped(now, "mocc", "x") {
        return Ok(None);
    }
    state.skip(now, "mocc", "x", 7 * DAY);
    let Ok(read) = tools.call("hoi4_occupation", &json!({})) else {
        note(state, "occupation", "hoi4_occupation unreadable");
        return Ok(None);
    };
    let mut extra: Vec<&str> = vec!["garrison template: no action exists yet"];
    // the law of single states: {state id: law}, each set once while the state is occupied
    let per_state: Vec<(i64, String)> = occ["per_state"]
        .as_object()
        .map(|m| {
            m.iter()
                .filter_map(|(k, v)| Some((k.parse().ok()?, v.as_str()?.to_string())))
                .collect()
        })
        .unwrap_or_default();
    let per_state_built = super::arg_built("hoi4_occupation_law", "state");
    if !per_state.is_empty() && !per_state_built {
        extra.push(
            "per_state not applied: setting one state's law is not built yet \
             (hoi4_occupation_law state; gap register occupation.per_state_law)",
        );
    }
    let current = read["player_occupation_law"]["key"]
        .as_str()
        .unwrap_or("")
        .to_string();
    let mut ladder = strings(occ, "ladder");
    if ladder.is_empty() {
        ladder = occupation_ladder(&read);
    }
    let states: Vec<&Value> = list(&read, "states")
        .iter()
        .filter(|s| {
            matches!(
                text(&s["role"]),
                "occupied_by_player" | "owned_non_core_or_resisting"
            )
        })
        .collect();
    let resistance = |s: &Value| s["resistance"].as_f64().unwrap_or(0.0);
    let hottest = states
        .iter()
        .copied()
        .max_by(|a, b| resistance(a).total_cmp(&resistance(b)));
    let via = hottest.and_then(|s| s["state_id"].as_i64());
    let tail = extra.join("; ");
    let Some(via) = via else {
        note(
            state,
            "occupation",
            format!("no occupied or resisting state; law {current}; {tail}"),
        );
        return Ok(None);
    };
    let change_days = occ["change_days"].as_i64().filter(|d| *d > 0).unwrap_or(30);
    // the plan's default law, once
    if let Some(law) = occ["default_law"]
        .as_str()
        .or(occ["law"].as_str())
        .filter(|l| !l.is_empty())
    {
        let key = format!("occ:default:{law}");
        if !state.done.contains_key(&key)
            && when_holds(state, tools, &occ["when"], now) == Truth::True
        {
            if current == law {
                state.done.insert(key, now);
            } else {
                let (ok, why) = state.act(
                    tools,
                    lease,
                    "hoi4_occupation_law",
                    json!({"law": law, "via_state": via}),
                    "ol",
                )?;
                if ok {
                    state.done.insert(key, now);
                    state.skip(now, "occchange", "x", change_days * DAY);
                }
                note(
                    state,
                    "occupation",
                    format!("default law {current} -> {law}: {ok}; {tail}"),
                );
                return Ok(Some(format!("occupation default law {law}: {ok} {why}")));
            }
        }
    }
    if per_state_built {
        let held = |id: i64| {
            states.iter().any(|s| {
                s["state_id"].as_i64() == Some(id) && text(&s["role"]) == "occupied_by_player"
            })
        };
        for (id, law) in &per_state {
            let key = format!("occ:state:{id}:{law}");
            if state.done.contains_key(&key) || state.skipped(now, "occstate", &key) || !held(*id) {
                continue;
            }
            let (ok, why) = state.act(
                tools,
                lease,
                "hoi4_occupation_law",
                json!({"law": law, "state": id, "via_state": id}),
                "ol",
            )?;
            if ok {
                state.done.insert(key, now);
            } else {
                state.skip(now, "occstate", &key, 7 * DAY);
            }
            note(
                state,
                "occupation",
                format!("state {id} law {law}: {ok}; {tail}"),
            );
            return Ok(Some(format!(
                "occupation law of state {id} {law}: {ok} {why}"
            )));
        }
    }
    let harsher_at = occ["harsher_at_resistance"].as_f64().unwrap_or(50.0);
    let milder_at = occ["milder_at_compliance"].as_f64().unwrap_or(40.0);
    let milder_below = occ["milder_below_resistance"].as_f64().unwrap_or(25.0);
    let max_res = hottest.map_or(0.0, resistance);
    let calm = states.iter().all(|s| {
        s["compliance"].as_f64().unwrap_or(0.0) >= milder_at && resistance(s) < milder_below
    });
    if ladder.is_empty() {
        note(
            state,
            "occupation",
            format!("law {current}: no ladder (hoi4_occupation lists no law shown to the player and the plan has no occupation.ladder); {tail}"),
        );
        return Ok(None);
    }
    let Some(index) = ladder.iter().position(|l| *l == current) else {
        note(
            state,
            "occupation",
            format!("law {current} is not on the ladder; {tail}"),
        );
        return Ok(None);
    };
    let bound = |key: &str, default: usize| {
        occ[key]
            .as_str()
            .and_then(|l| ladder.iter().position(|x| x == l))
            .unwrap_or(default)
    };
    let (mildest, harshest) = (bound("mildest", 0), bound("harshest", ladder.len() - 1));
    let next = if max_res >= harsher_at && index < harshest {
        index + 1
    } else if calm && max_res < harsher_at && index > mildest {
        index - 1
    } else {
        note(
            state,
            "occupation",
            format!("law {current} kept (resistance max {max_res}); {tail}"),
        );
        return Ok(None);
    };
    if state.skipped(now, "occchange", "x") {
        note(
            state,
            "occupation",
            format!(
                "law {current}: a change to {} waits for change_days {change_days}; {tail}",
                ladder[next]
            ),
        );
        return Ok(None);
    }
    let law = ladder[next].clone();
    let (ok, why) = state.act(
        tools,
        lease,
        "hoi4_occupation_law",
        json!({"law": law, "via_state": via}),
        "ol",
    )?;
    state.skip(
        now,
        "occchange",
        "x",
        if ok { change_days * DAY } else { 7 * DAY },
    );
    note(
        state,
        "occupation",
        format!("law {current} -> {law} (resistance max {max_res}): {ok}; {tail}"),
    );
    Ok(Some(format!(
        "occupation law {current} -> {law} (resistance max {max_res}): {ok} {why}"
    )))
}

// ---------------------------------------------------------------------------------------------
// Rule 25 / W1-8: the peace conference.

/// The tick's events step when a peace conference is open.
pub(super) fn peace(
    state: &mut State,
    tools: &dyn Tools,
    plan: &Value,
    lease: &str,
    now: GameHour,
) -> Result<Tick> {
    let p = &plan["peace"];
    if !p.is_object() {
        wake(
            state,
            plan,
            now,
            25,
            "peace_conference_open",
            "a peace conference is open and the plan has no `peace`: the player makes the demands"
                .into(),
            json!({}),
        );
        return Ok(Tick::Stop("peace conference open".into()));
    }
    let mut wanted: Vec<(i64, &str)> = Vec::new();
    let mut tags: Vec<String> = Vec::new();
    for (key, kind) in [
        ("take", "take"),
        ("puppet", "puppet"),
        ("liberate", "liberate"),
    ] {
        for v in list(p, key) {
            // A country tag stands for every state it owns: the conference demands state by
            // state (a puppet demand on all of a country's states puppets the whole country).
            let ids = match (v.as_i64(), v.as_str()) {
                (Some(id), _) => vec![id],
                (None, Some(tag)) => match tag_states(tools, tag) {
                    Some(ids) if !ids.is_empty() => ids,
                    Some(_) => {
                        tags.push(format!("{key} {tag}: owns no state"));
                        vec![]
                    }
                    None => {
                        tags.push(format!("{key} {tag}: its states are unreadable"));
                        vec![]
                    }
                },
                _ => vec![],
            };
            for id in ids {
                if !wanted.iter().any(|w| w.0 == id) {
                    wanted.push((id, kind));
                }
            }
        }
    }
    // force_government [TAG]: one demand per country, on one of its states
    let force_built = super::arg_built("hoi4_peace_take", "force_government");
    for v in list(p, "force_government") {
        let Some(tag) = v.as_str().filter(|t| !t.is_empty()) else {
            continue;
        };
        if !force_built {
            tags.push(format!(
                "force_government {tag}: the demand is not built yet \
                 (gap register peace.force_government)"
            ));
            continue;
        }
        match tag_states(tools, tag).and_then(|ids| ids.first().copied()) {
            Some(id) if !wanted.iter().any(|w| w.0 == id) => wanted.push((id, "force_government")),
            Some(_) => {}
            None => tags.push(format!("force_government {tag}: owns no readable state")),
        }
    }
    if !tags.is_empty() {
        note(state, "peace", format!("not demanded: {}", tags.join(", ")));
    }
    let conf = state.manage.peace.get_or_insert_with(Conference::default);
    if conf.exited {
        return Ok(peace_wait(state, plan, now, "exited, window still open"));
    }
    let open = |c: &Conference, id: i64| !c.taken.contains(&id) && !c.dropped.contains(&id);
    let ids: Vec<i64> = wanted
        .iter()
        .map(|w| w.0)
        .filter(|id| open(conf, *id))
        .take(16)
        .collect();
    let Ok(read) = tools.call("hoi4_peace_options", &json!({"states": ids})) else {
        return Ok(peace_wait(
            state,
            plan,
            now,
            "hoi4_peace_options unreadable",
        ));
    };
    if read["peace_conference_open"] != json!(true) {
        return Ok(peace_wait(state, plan, now, "window not open yet"));
    }
    let conf = state.manage.peace.get_or_insert_with(Conference::default);
    for row in list(&read, "states") {
        if row["own"] == json!(true) {
            if let Some(id) = row["state_id"].as_i64() {
                conf.dropped.insert(id);
            }
        }
    }
    let pending: Vec<(i64, &str)> = wanted
        .iter()
        .copied()
        .filter(|(id, _)| open(conf, *id))
        .collect();
    let enabled = |b: &str| read["controls"][b]["enabled"] == json!(true);
    let (end_turn, done) = (enabled("end_turn_button"), enabled("done_button"));
    let points_left = read["own_remaining_points"]
        .as_f64()
        .is_none_or(|p| p > 0.0);
    let rounds_left = conf.rounds < p["max_rounds"].as_u64().unwrap_or(10);
    let (spent, taken, total) = (conf.spent, conf.taken.len(), wanted.len());
    let patience_over = conf.waits >= PEACE_PATIENCE;
    let (tool, args, what) =
        if end_turn && rounds_left && !spent && points_left && !pending.is_empty() {
            let (id, kind) = pending[0];
            let mut args = json!({"states": [id]});
            match kind {
                "puppet" => args["puppet"] = json!(true),
                "liberate" => args["release"] = json!(true),
                "force_government" => args["force_government"] = json!(true),
                _ => {}
            }
            if let Some(max) = p["max_cost"].as_f64() {
                args["max_cost"] = json!(max);
            }
            ("hoi4_peace_take", args, format!("peace {kind} state {id}"))
        } else if end_turn && rounds_left && (!pending.is_empty() || !done) {
            ("hoi4_peace_submit", json!({}), "peace submit".to_string())
        } else if done && (pending.is_empty() || !rounds_left || patience_over) {
            (
                "hoi4_peace_exit",
                json!({}),
                format!("peace exit (took {taken} of {total})"),
            )
        } else {
            // the other victors' turn
            return Ok(peace_wait(state, plan, now, "no usable control"));
        };
    let (ok, why) = state.act(tools, lease, tool, args.clone(), "pc")?;
    let conf = state.manage.peace.get_or_insert_with(Conference::default);
    conf.waits = 0;
    match tool {
        "hoi4_peace_take" => {
            let id = args["states"][0].as_i64().unwrap_or(0);
            if ok {
                conf.taken.push(id);
            } else if why.starts_with("cost_exceeds_points") {
                conf.spent = true;
            } else if !why.starts_with("round_not_ready") {
                conf.dropped.insert(id);
            }
        }
        "hoi4_peace_submit" if ok => {
            conf.rounds += 1;
            conf.spent = false;
        }
        "hoi4_peace_exit" if ok => conf.exited = true,
        _ => {}
    }
    let line = format!(
        "taken {:?}, dropped {:?}, rounds {}",
        conf.taken,
        conf.dropped.iter().collect::<BTreeSet<_>>(),
        conf.rounds
    );
    // what the plan names but cannot be demanded stays in the digest after an action too
    let line = if tags.is_empty() {
        line
    } else {
        format!("{line}; not demanded: {}", tags.join(", "))
    };
    note(state, "peace", line);
    if why.starts_with("skipped:") {
        wake(
            state,
            plan,
            now,
            25,
            "peace_refused",
            format!("{what}: {why}; the player finishes the conference"),
            json!({"order": tool, "args": args}),
        );
        return Ok(Tick::Stop(format!("peace conference open: {what} refused")));
    }
    Ok(Tick::Acted(format!("{what}: {ok} {why}")))
}

// ---------------------------------------------------------------------------------------------
// Rule 16 / W1-12: resource deficits and automatic importing.

fn resources_step(
    state: &mut State,
    tools: &dyn Tools,
    plan: &Value,
    lease: &str,
    now: GameHour,
) -> Result<Option<String>> {
    if (plan["auto_import"].is_null() && !plan["manage"].is_object())
        || off(plan, "resources")
        || state.skipped(now, "mres", "x")
    {
        return Ok(None);
    }
    state.skip(now, "mres", "x", 7 * DAY);
    let priority = strings(plan, "production_priority");
    let mut short: BTreeSet<String> = BTreeSet::new();
    let mut lines: Vec<String> = Vec::new();
    let mut units = 0.0f64;
    if let Ok(read) = tools.call("hoi4_production_shortages", &json!({})) {
        // A resource short for the whole country that no line draws (coal for energy in 1.19)
        // has no priority line to show it, yet it slows every factory: it counts too (n1, 16 Nov
        // 2026: coal short 205 days under the insufficient_energy alert, the step saw "no deficit").
        let drawn: BTreeSet<String> = list(&read, "lines")
            .iter()
            .flat_map(|l| list(l, "resources"))
            .map(|a| text(&a["resource"]).to_string())
            .collect();
        if let Ok(country) = tools.call("hoi4_resources", &json!({})) {
            if let Some(map) = country["resources"].as_object() {
                for (name, r) in map {
                    if r["shortage"]["short"] == json!(true) && !drawn.contains(name) {
                        let deficit = r["shortage"]["deficit"].as_f64().unwrap_or(0.0);
                        units += deficit;
                        lines.push(format!(
                            "{name} short {deficit:.0} (country demand, no line draws it)"
                        ));
                        short.insert(name.clone());
                    }
                }
            }
        }
        for line in list(&read, "lines") {
            let equipment = text(&line["equipment"]);
            let of: Vec<String> = strings(line, "short_of");
            if of.is_empty()
                || !(priority.is_empty()
                    || priority.iter().any(|p| equipment.starts_with(p.as_str())))
            {
                continue;
            }
            lines.push(format!("{equipment} short of {}", of.join("/")));
            short.extend(of);
        }
    } else if let Ok(read) = tools.call("hoi4_resources", &json!({})) {
        if let Some(map) = read["resources"].as_object() {
            for (name, r) in map {
                if r["shortage"]["short"] == json!(true) {
                    short.insert(name.clone());
                }
            }
        }
    } else {
        note(
            state,
            "resources",
            "unknown: hoi4_production_shortages and hoi4_resources unreadable",
        );
        return Ok(None);
    }
    if short.is_empty() {
        state.manage.deficit_since = None;
        note(state, "resources", "no deficit on a priority line");
        return Ok(None);
    }
    let since = *state.manage.deficit_since.get_or_insert(now);
    let deficit = short.iter().cloned().collect::<Vec<_>>().join("/");
    let evidence = json!({"short_of": short, "lines": lines, "since_hour": since});
    let (allowed, budget) = import_cfg(plan);
    if !allowed {
        note(
            state,
            "resources",
            format!("deficit {deficit}; auto_import not allowed by the plan"),
        );
        wake(
            state,
            plan,
            now,
            16,
            "resource_deficit",
            format!("resource deficit ({deficit}) and the plan has no auto_import: trade or change the lines"),
            evidence,
        );
        return Ok(None);
    }
    let shown = budget.map_or_else(|| "the game's".to_string(), |b| b.to_string());
    if let Some((ok, why)) = import_on(state, tools, plan, lease, now)? {
        note(
            state,
            "resources",
            format!("deficit {deficit}; auto-import on (budget {shown}): {ok}"),
        );
        return Ok(Some(format!(
            "auto_import on (budget {shown}) for {deficit}: {ok} {why}"
        )));
    }
    let days = plan["manage"]["deficit_wake_days"]
        .as_i64()
        .filter(|d| *d > 0)
        .unwrap_or(14);
    note(
        state,
        "resources",
        format!(
            "deficit {deficit} for {} days, auto-import on (budget {shown})",
            (now - since) / DAY
        ),
    );
    if now - since >= days * DAY {
        if let Some(done) = raise_import_budget(state, tools, plan, lease, now, units, &deficit)? {
            return Ok(Some(done));
        }
        wake(
            state,
            plan,
            now,
            16,
            "resource_deficit",
            format!("resource deficit ({deficit}) has lasted {} game days with auto-import on (budget {shown})", (now - since) / DAY),
            evidence,
        );
    }
    Ok(None)
}

/// A deficit that automatic importing has not closed in `deficit_wake_days` raises its budget by
/// the factories the deficit costs (one civilian factory buys about 8 units), up to the factories
/// the game offers and `auto_import.budget_max` (the plan's budget + 2 by default); at most once
/// every 14 game days. None when there is nothing to raise (the player is woken instead).
fn raise_import_budget(
    state: &mut State,
    tools: &dyn Tools,
    plan: &Value,
    lease: &str,
    now: GameHour,
    units: f64,
    deficit: &str,
) -> Result<Option<String>> {
    if state.skipped(now, "mimpraise", "x") {
        return Ok(None);
    }
    state.skip(now, "mimpraise", "x", 14 * DAY);
    let (_, budget) = import_cfg(plan);
    let cap = plan["auto_import"]["budget_max"]
        .as_i64()
        .or(budget.map(|b| b + 2))
        .unwrap_or(i64::MAX);
    let Ok(read) = tools.call("hoi4_auto_import", &json!({"lease_token": lease})) else {
        return Ok(None);
    };
    let auto = &read["auto_import"];
    let (Some(have), Some(available)) = (
        auto["budget_factories"].as_i64(),
        auto["available_factories"].as_i64(),
    ) else {
        return Ok(None);
    };
    if auto["enabled"] != json!(true) {
        return Ok(None);
    }
    let need = ((units / 8.0).ceil() as i64).max(1);
    let to = (have + need).min(available).min(cap);
    if to <= have {
        note(
            state,
            "resources",
            format!(
                "deficit {deficit}: import budget {have} cannot grow (available {available}, cap {})",
                if cap == i64::MAX { "none".to_string() } else { cap.to_string() }
            ),
        );
        return Ok(None);
    }
    let (ok, why) = state.act(
        tools,
        lease,
        "hoi4_auto_import",
        json!({"enabled": true, "budget": to}),
        "ai",
    )?;
    note(
        state,
        "resources",
        format!("deficit {deficit} outlasted auto-import: budget {have} -> {to}: {ok}"),
    );
    Ok(Some(format!(
        "auto_import budget {have} -> {to} for {deficit}: {ok} {why}"
    )))
}

/// The plan's auto-import: (allowed, budget). `auto_import` true, a budget number, or an object
/// {budget?, enabled?}; anything else (absent, false, enabled:false) allows no import.
fn import_cfg(plan: &Value) -> (bool, Option<i64>) {
    let cfg = &plan["auto_import"];
    match cfg {
        Value::Bool(on) => (*on, None),
        Value::Number(n) => (true, n.as_i64()),
        Value::Object(_) => (cfg["enabled"] != json!(false), cfg["budget"].as_i64()),
        _ => (false, None),
    }
}

/// Turn automatic importing on with the plan's budget, once per budget (done-memory
/// `imp:on:<budget>`, shared by every rule that needs imports). None when it was done already.
fn import_on(
    state: &mut State,
    tools: &dyn Tools,
    plan: &Value,
    lease: &str,
    now: GameHour,
) -> Result<Option<(bool, String)>> {
    let (_, budget) = import_cfg(plan);
    let shown = budget.map_or_else(|| "the game's".to_string(), |b| b.to_string());
    let key = format!("imp:on:{shown}");
    if state.done.contains_key(&key) {
        return Ok(None);
    }
    let mut args = json!({"enabled": true});
    if let Some(b) = budget {
        args["budget"] = json!(b);
    }
    let (ok, why) = state.act(tools, lease, "hoi4_auto_import", args, "ai")?;
    if ok {
        state.done.insert(key, now);
    }
    Ok(Some((ok, why)))
}

/// EC27: the game lowers the automatic-import budget when usable civilian factories fall and never
/// raises it again, so a budget cut by bombing or a lost state silently stops covering deficits.
/// While the plan's auto-import has a `budget`, every 7 game days: when the civilian factory count
/// (hoi4_economy) has moved by `auto_import.budget_step` (2) or more since the last check (the first
/// check always reads), hoi4_auto_import is read, and a budget below the plan's (no more than the
/// factories the view says are available, never less than the budget it holds) is sent again.
fn import_budget_rule(
    state: &mut State,
    tools: &dyn Tools,
    plan: &Value,
    lease: &str,
    now: GameHour,
) -> Result<Option<String>> {
    let (allowed, budget) = import_cfg(plan);
    let Some(want) = budget.filter(|_| allowed) else {
        return Ok(None);
    };
    if off(plan, "resources") || state.skipped(now, "mimpb", "x") {
        return Ok(None);
    }
    state.skip(now, "mimpb", "x", 7 * DAY);
    let Some(civs) = tools
        .call("hoi4_economy", &json!({}))
        .ok()
        .and_then(|e| e["factory_trigger_count"]["civilian"].as_i64())
    else {
        note(
            state,
            "import_budget",
            "unknown: hoi4_economy gave no civilian factory count",
        );
        return Ok(None);
    };
    let step = plan["auto_import"]["budget_step"]
        .as_i64()
        .filter(|n| *n > 0)
        .unwrap_or(2);
    if let Some(last) = state.manage.import_civs {
        if (civs - last).abs() < step {
            note(
                state,
                "import_budget",
                format!("{civs} civilian factories ({last} at the last check, step {step}): budget not re-read"),
            );
            return Ok(None);
        }
    }
    let Ok(read) = tools.call("hoi4_auto_import", &json!({"lease_token": lease})) else {
        note(
            state,
            "import_budget",
            "unknown: hoi4_auto_import unreadable",
        );
        return Ok(None);
    };
    state.manage.import_civs = Some(civs);
    let auto = &read["auto_import"];
    if auto["enabled"] != json!(true) {
        note(
            state,
            "import_budget",
            "automatic importing is off: no budget to keep (the resources rule turns it on on a deficit)",
        );
        return Ok(None);
    }
    let Some(have) = auto["budget_factories"].as_i64() else {
        note(
            state,
            "import_budget",
            "unknown: hoi4_auto_import gave no budget",
        );
        return Ok(None);
    };
    let wanted = auto["available_factories"]
        .as_i64()
        .map_or(want, |available| want.min(available.max(have)));
    if have >= wanted {
        note(
            state,
            "import_budget",
            format!("budget {have} of the plan's {want} ({civs} civilian factories)"),
        );
        return Ok(None);
    }
    let (ok, why) = state.act(
        tools,
        lease,
        "hoi4_auto_import",
        json!({"budget": wanted}),
        "ab",
    )?;
    note(
        state,
        "import_budget",
        format!("budget {have} -> {wanted} (plan {want}, {civs} civilian factories): {ok}"),
    );
    Ok(Some(format!(
        "auto_import budget {have} -> {wanted}: {ok} {why}"
    )))
}

/// Rule 26 / W3-10: fuel below `manage.fuel_floor` (0.25 of the capacity, the game's
/// FUEL_PENALTY_START_RATIO by default) turns automatic importing on (oil covers fuel), once a
/// game day from hoi4_resources fuel.ratio. A plan that allows no auto_import wakes the player at
/// once; with imports on, fuel still below the floor after `manage.deficit_wake_days` (14) wakes
/// the player (training missions, refineries and trade are judgement).
fn fuel_rule(
    state: &mut State,
    tools: &dyn Tools,
    plan: &Value,
    lease: &str,
    now: GameHour,
) -> Result<Option<String>> {
    let m = &plan["manage"];
    if !m.is_object() || off(plan, "fuel") || state.skipped(now, "mfuel", "x") {
        return Ok(None);
    }
    state.skip(now, "mfuel", "x", DAY);
    let floor = fraction(&m["fuel_floor"], 0.25);
    let Some(ratio) = tools
        .call("hoi4_resources", &json!({}))
        .ok()
        .and_then(|r| r["fuel"]["ratio"].as_f64())
    else {
        note(state, "fuel", "unknown: hoi4_resources gave no fuel ratio");
        return Ok(None);
    };
    let shown = round3(ratio);
    if ratio >= floor {
        state.manage.fuel_low_since = None;
        note(
            state,
            "fuel",
            format!("{shown} of capacity (floor {floor})"),
        );
        return Ok(None);
    }
    let since = *state.manage.fuel_low_since.get_or_insert(now);
    let evidence = json!({"fuel_ratio": shown, "floor": floor, "since_hour": since});
    if !import_cfg(plan).0 {
        note(
            state,
            "fuel",
            format!("{shown} below {floor}; auto_import not allowed by the plan: player woken"),
        );
        wake(
            state,
            plan,
            now,
            26,
            "fuel_low",
            format!("fuel is at {shown} of capacity, below the floor {floor}, and the plan allows no auto_import: import oil, cut training missions or build refineries"),
            evidence,
        );
        return Ok(None);
    }
    if let Some((ok, why)) = import_on(state, tools, plan, lease, now)? {
        note(
            state,
            "fuel",
            format!("{shown} below {floor}: auto-import on (oil covers fuel): {ok}"),
        );
        return Ok(Some(format!(
            "fuel {shown} below {floor}: auto_import on: {ok} {why}"
        )));
    }
    let days = plan["manage"]["deficit_wake_days"]
        .as_i64()
        .filter(|d| *d > 0)
        .unwrap_or(14);
    note(
        state,
        "fuel",
        format!(
            "{shown} below {floor} for {} days, auto-import on",
            (now - since) / DAY
        ),
    );
    if now - since >= days * DAY {
        wake(
            state,
            plan,
            now,
            26,
            "fuel_low",
            format!("fuel has stayed below {floor} of capacity for {} game days with auto-import on: cut training missions, build refineries or trade for oil", (now - since) / DAY),
            evidence,
        );
    }
    Ok(None)
}

/// Rule 20 / W1-14: free convoys (hoi4_convoys) below `manage.convoys_free` (0.1) of the stock (or
/// of what the game needs, when it needs more than the stock), or a need above the stock, put idle
/// dockyards on convoys every 3 game days: `manage.rebalance_step` (3) more on the convoy line with
/// the fewest dockyards, else a new line of the newest buildable convoy design. Which equipment is
/// a convoy comes from hoi4_naval_production_options (`convoy` of its designs and lines); idle
/// dockyards are hoi4_economy's naval factories minus the naval lines' requests. With no idle
/// dockyard (taking one from a warship line is a decision), no convoy design, or a shortage that
/// outlasts `manage.deficit_wake_days`, the player is woken. With enough convoys, dockyards no line
/// uses go to convoys the same way (no wake), unless `manage.idle_dockyards` is "keep".
fn convoy_rule(
    state: &mut State,
    tools: &dyn Tools,
    plan: &Value,
    lease: &str,
    now: GameHour,
) -> Result<Option<String>> {
    let m = &plan["manage"];
    if !m.is_object() || off(plan, "convoys") || state.skipped(now, "mconvoy", "x") {
        return Ok(None);
    }
    state.skip(now, "mconvoy", "x", 3 * DAY);
    let share = fraction(&m["convoys_free"], 0.1);
    let read = tools.call("hoi4_convoys", &json!({})).ok();
    let counts = read.as_ref().map(|r| &r["convoys"]);
    let (Some(total), Some(free)) = (
        counts.and_then(|c| c["total"].as_f64()),
        counts.and_then(|c| c["free"].as_f64()),
    ) else {
        note(
            state,
            "convoys",
            "unknown: hoi4_convoys gave no convoy counts",
        );
        return Ok(None);
    };
    let needed = counts.and_then(|c| c["needed"].as_f64()).unwrap_or(0.0);
    let floor = (share * total.max(needed)).ceil();
    // Enough convoys: dockyards no line uses still build convoys (more trade and sea supply for
    // every war overseas) unless the plan keeps them (`manage.idle_dockyards`: "keep"). Without
    // this owner the free-dockyards alert stayed open all campaign (live 15 Nov 2026).
    let spare_only = free >= floor && needed <= total;
    if spare_only {
        state.manage.convoys_low_since = None;
        note(
            state,
            "convoys",
            format!("{free} free of {total} (floor {floor}, needed {needed})"),
        );
        if m["idle_dockyards"] == json!("keep") {
            return Ok(None);
        }
    }
    let since = if spare_only {
        now
    } else {
        *state.manage.convoys_low_since.get_or_insert(now)
    };
    let mut evidence = json!({"free": free, "total": total, "needed": needed, "share": share,
        "floor": floor, "since_hour": since});
    let low = format!("{free} free of {total} (floor {floor}, needed {needed})");
    let wake_days = m["deficit_wake_days"]
        .as_i64()
        .filter(|d| *d > 0)
        .unwrap_or(14);
    if !spare_only && now - since >= wake_days * DAY {
        wake(
            state,
            plan,
            now,
            20,
            "convoys_low",
            format!("convoys have been short for {} game days: {low}; escorts, routes or more dockyards are a decision", (now - since) / DAY),
            evidence.clone(),
        );
    }
    let Ok(options) = tools.call("hoi4_naval_production_options", &json!({})) else {
        note(
            state,
            "convoys",
            format!("{low}; hoi4_naval_production_options unreadable"),
        );
        return Ok(None);
    };
    let Some(dockyards) = tools
        .call("hoi4_economy", &json!({}))
        .ok()
        .and_then(|e| e["factory_trigger_count"]["naval"].as_i64())
    else {
        note(
            state,
            "convoys",
            format!("{low}; hoi4_economy gave no dockyard count"),
        );
        return Ok(None);
    };
    let lines = list(&options, "naval_lines");
    let used: i64 = lines
        .iter()
        .filter_map(|l| l["requested_factories"].as_i64())
        .sum();
    let idle = dockyards - used;
    evidence["idle_dockyards"] = json!(idle);
    if spare_only && idle <= 0 {
        return Ok(None);
    }
    if idle <= 0 {
        note(
            state,
            "convoys",
            format!("{low}; no idle dockyard ({dockyards} all on lines): player woken"),
        );
        wake(
            state,
            plan,
            now,
            20,
            "convoys_low",
            format!("convoys are short ({low}) and every dockyard is on a line: which line gives way to convoys is a decision"),
            evidence,
        );
        return Ok(None);
    }
    let step = m["rebalance_step"].as_i64().filter(|n| *n > 0).unwrap_or(3);
    let give = idle.min(step);
    let line = lines
        .iter()
        .filter(|l| l["convoy"] == json!(true) && l["line_index"].is_i64())
        .min_by_key(|l| l["requested_factories"].as_i64().unwrap_or(0));
    let (ok, why, what) = if let Some(l) = line {
        let equipment = text(&l["equipment"]).to_string();
        let has = l["requested_factories"].as_i64().unwrap_or(0);
        let (ok, why) = state.act(
            tools,
            lease,
            "hoi4_production_set",
            json!({"line": l["line_index"], "equipment": equipment, "factories": has + give}),
            "cv",
        )?;
        (ok, why, format!("{equipment} line {has}+{give}"))
    } else {
        let design = list(&options, "designs")
            .iter()
            .filter(|d| d["convoy"] == json!(true) && !text(&d["equipment"]).is_empty())
            .max_by_key(|d| d["design_id"].as_i64().unwrap_or(0));
        let Some(design) = design else {
            if spare_only {
                return Ok(None);
            }
            note(
                state,
                "convoys",
                format!("{low}; no buildable convoy design: player woken"),
            );
            wake(
                state,
                plan,
                now,
                20,
                "convoys_low",
                format!(
                    "convoys are short ({low}) and the Production panel offers no convoy design"
                ),
                evidence,
            );
            return Ok(None);
        };
        let equipment = text(&design["equipment"]).to_string();
        let mut args = json!({"equipment": equipment, "factories": give});
        if design["design_id"].is_i64() {
            args["design"] = design["design_id"].clone();
        }
        let (ok, why) = state.act(tools, lease, "hoi4_production_add", args, "cv")?;
        (ok, why, format!("new {equipment} line with {give}"))
    };
    if !ok {
        state.skip(now, "mconvoy", "x", 7 * DAY);
    }
    note(
        state,
        "convoys",
        format!("{low}: {what} ({idle} idle dockyards): {ok}"),
    );
    Ok(Some(format!("convoys {low}: {what}: {ok} {why}")))
}

// ---------------------------------------------------------------------------------------------
// Rule 19 / W1-13: the air plan.

/// Whether an `air_plan` entry claims this wing (a row of hoi4_air): by `wing`, else by
/// `airframe` (and `version` when the wing's variant is known).
fn wing_matches(entry: &Value, wing: &Value) -> bool {
    if let Some(name) = entry["wing"].as_str().filter(|n| !n.is_empty()) {
        return text(&wing["wing"]) == name;
    }
    let Some(airframe) = entry["airframe"].as_str().filter(|a| !a.is_empty()) else {
        return false;
    };
    text(&wing["aircraft_type"]).starts_with(airframe)
        && entry["version"]
            .as_str()
            .filter(|v| !v.is_empty())
            .is_none_or(|v| wing["variant"].as_str().is_none_or(|w| w == v))
}

/// The extras step's default war missions leave the wings the air plan names alone.
pub(super) fn air_plan_claims(plan: &Value, wing: &Value) -> bool {
    list(plan, "air_plan")
        .iter()
        .any(|entry| wing_matches(entry, wing))
}

/// A strategic region given as an id (number or numeric text) or a name, against `{id, name}`.
fn region_matches(want: &Value, region: &Value) -> bool {
    let id = want
        .as_i64()
        .or_else(|| want.as_str().and_then(|s| s.trim().parse().ok()));
    match id {
        Some(id) => region["id"].as_i64() == Some(id),
        None => want.as_str().is_some_and(|w| {
            !w.is_empty()
                && region["name"]
                    .as_str()
                    .is_some_and(|n| n.eq_ignore_ascii_case(w))
        }),
    }
}

/// The state of the own air base in `region` with the most free room for `planes`, leaving out
/// `except` (the state the wing stands in).
fn roomiest_base(bases: &[Value], region: &Value, except: Option<i64>, planes: i64) -> Option<i64> {
    bases
        .iter()
        .filter(|b| region_matches(region, &b["region"]) && b["state_id"].as_i64() != except)
        .filter(|b| b["free"].as_i64().unwrap_or(0) >= planes)
        .max_by_key(|b| b["free"].as_i64().unwrap_or(0))
        .and_then(|b| b["state_id"].as_i64())
}

/// Whether the wing's own base lies in `region` and is not over capacity.
fn based_in(wing: &Value, bases: &[Value], region: &Value) -> bool {
    bases
        .iter()
        .find(|b| b["base_id"] == wing["air_base"]["base_id"])
        .is_some_and(|b| {
            region_matches(region, &b["region"]) && b["free"].as_i64().unwrap_or(0) >= 0
        })
}

/// The state to rebase a wing to, if it must move: `base_state`; with `follow_region`, the base
/// with the most free room inside the region the wing flies over now (`flying`) unless its own
/// base is there already; else (or with no base with room there) the base of `base_region` with
/// the most free room when its own base is outside that region or over capacity.
fn rebase_target(entry: &Value, flying: &Value, wing: &Value, bases: &[Value]) -> Option<i64> {
    if wing["transfer_to_base_id"].as_i64().is_some() {
        return None;
    }
    let here = wing["air_base"]["state_id"].as_i64();
    if let Some(target) = entry["base_state"].as_i64() {
        // A state without an own air base has nowhere to land (the base the plan counts on
        // is not built yet, or the state was lost): the wing stays and flies from where it is.
        let lands =
            bases.is_empty() || bases.iter().any(|b| b["state_id"].as_i64() == Some(target));
        return (here != Some(target) && lands).then_some(target);
    }
    let planes = wing["aircraft"].as_i64().unwrap_or(0);
    if entry["follow_region"] == json!(true) && !flying.is_null() {
        // the mission region moves with the front: a base inside it keeps the region in range
        if based_in(wing, bases, flying) {
            return None;
        }
        if let Some(target) = roomiest_base(bases, flying, here, planes) {
            return Some(target);
        }
    }
    let want = &entry["base_region"];
    if want.is_null() || based_in(wing, bases, want) {
        return None;
    }
    roomiest_base(bases, want, here, planes)
}

/// An `air_plan` entry's `deploy` settings: (min_planes, max_wings, wing_size), None without one.
fn deploy_settings(entry: &Value) -> Option<(i64, Option<i64>, i64)> {
    let d = &entry["deploy"];
    if !(d == &json!(true) || d.is_object()) {
        return None;
    }
    let min = d["min_planes"].as_i64().filter(|n| *n > 0).unwrap_or(50);
    let size = d["wing_size"].as_i64().filter(|n| *n > 0).unwrap_or(100);
    Some((min, d["max_wings"].as_i64().filter(|n| *n >= 0), size))
}

/// The reserve planes a `deploy` entry turns into a new wing now: (aircraft name for
/// hoi4_air_wing_create, planes), or why none. Reserve = hoi4_stockpile variants of the entry's
/// airframe (and `version`, by design name from hoi4_designs); the entry's own wings below
/// `wing_size` draw their refill from it first, so only what is left beyond that counts.
fn deploy_choice(
    entry: &Value,
    air: &Value,
    stock: &Value,
    designs: &Value,
) -> std::result::Result<(String, i64), String> {
    let Some((min, max_wings, size)) = deploy_settings(entry) else {
        return Err("no deploy".into());
    };
    let Some(airframe) = entry["airframe"].as_str().filter(|a| !a.is_empty()) else {
        return Err("deploy needs the entry's airframe".into());
    };
    let wings: Vec<&Value> = list(air, "wings")
        .iter()
        .filter(|w| wing_matches(entry, w))
        .collect();
    if let Some(max) = max_wings {
        if wings.len() as i64 >= max {
            return Err(format!("{} wing(s) already (max_wings {max})", wings.len()));
        }
    }
    let refill: i64 = wings
        .iter()
        .map(|w| (size - w["aircraft"].as_i64().unwrap_or(size)).max(0))
        .sum();
    let names: HashMap<i64, String> = list(designs, "designs")
        .iter()
        .filter_map(|d| Some((d["id"].as_i64()?, d["name"].as_str()?.trim().to_string())))
        .filter(|(_, n)| !n.is_empty())
        .collect();
    let version = entry["version"].as_str().filter(|v| !v.is_empty());
    let mut reserve: Vec<(Option<String>, i64)> = Vec::new();
    for v in list(stock, "equipment")
        .iter()
        .flat_map(|row| list(row, "variants"))
        .filter(|v| text(&v["type"]).starts_with(airframe))
    {
        let amount = v["amount"].as_f64().unwrap_or(0.0).floor() as i64;
        let name = v["design_id"]
            .as_i64()
            .and_then(|id| names.get(&id).cloned());
        if let Some(want) = version {
            // a variant whose design name is unknown may still be the planned one: the tool
            // matches the reserve row by the version name
            if name.as_ref().is_some_and(|n| !n.eq_ignore_ascii_case(want)) {
                continue;
            }
        }
        if amount > 0 {
            reserve.push((name.or_else(|| version.map(String::from)), amount));
        }
    }
    let total: i64 = reserve.iter().map(|(_, n)| n).sum();
    if total - refill < min {
        return Err(format!(
            "{total} {airframe} in reserve, {refill} kept for the refill of {} wing(s): below min_planes {min}",
            wings.len()
        ));
    }
    reserve.sort_by_key(|(_, n)| std::cmp::Reverse(*n));
    match reserve.into_iter().find(|(_, n)| *n >= min) {
        Some((Some(name), n)) => Ok((name, n.min(size).min(100))),
        Some((None, _)) => Err(format!(
            "the {airframe} reserve variant has no design name (hoi4_designs); name it in `version`"
        )),
        None => Err(format!(
            "no single {airframe} variant has min_planes {min} in reserve"
        )),
    }
}

/// Where a new wing of `planes` is founded: `base_state` when its own base has room; else the base
/// with the most free room in the mission region (`follow_region`) or in `base_region`.
fn deploy_base(entry: &Value, flying: &Value, bases: &[Value], planes: i64) -> Option<i64> {
    if let Some(target) = entry["base_state"].as_i64() {
        return bases
            .iter()
            .any(|b| {
                b["state_id"].as_i64() == Some(target) && b["free"].as_i64().unwrap_or(0) >= planes
            })
            .then_some(target);
    }
    if entry["follow_region"] == json!(true) && !flying.is_null() {
        if let Some(target) = roomiest_base(bases, flying, None, planes) {
            return Some(target);
        }
    }
    let want = &entry["base_region"];
    (!want.is_null())
        .then(|| roomiest_base(bases, want, None, planes))
        .flatten()
}

fn air_plan_step(
    state: &mut State,
    tools: &dyn Tools,
    plan: &Value,
    lease: &str,
    now: GameHour,
) -> Result<Option<String>> {
    let entries = list(plan, "air_plan");
    if entries.is_empty() || state.skipped(now, "mair", "x") {
        return Ok(None);
    }
    let Ok(air) = tools.call("hoi4_air", &json!({})) else {
        state.skip(now, "mair", "x", 3 * DAY);
        note(state, "air_plan", "hoi4_air unreadable");
        return Ok(None);
    };
    let bases = list(&air, "air_bases");
    let mut flying = 0;
    let mut deploy_notes: Vec<String> = Vec::new();
    for (index, entry) in entries.iter().enumerate() {
        if when_holds(state, tools, &entry["when"], now) != Truth::True {
            continue;
        }
        let switched = !entry["switch_when"].is_null()
            && when_holds(state, tools, &entry["switch_when"], now) == Truth::True;
        let region = if switched && !entry["then_region"].is_null() {
            &entry["then_region"]
        } else {
            &entry["region"]
        };
        let mission = switched
            .then(|| {
                entry["then_mission"]
                    .as_str()
                    .or(entry["fallback_mission"].as_str())
            })
            .flatten()
            .or(entry["mission"].as_str())
            .filter(|m| !m.is_empty());
        let region_arg = match region {
            Value::Number(n) => Some(n.to_string()),
            Value::String(s) if !s.trim().is_empty() => Some(s.trim().to_string()),
            _ => None,
        };
        for wing in list(&air, "wings")
            .iter()
            .filter(|w| wing_matches(entry, w))
        {
            let name = text(&wing["wing"]).to_string();
            if name.is_empty() || state.skipped(now, "airplan", &name) {
                continue;
            }
            // A base the game refused (full, or out of the wing's reach) is not asked again for
            // 30 days; the wing flies its mission from where it stands meanwhile (n1, 16 Nov 2026:
            // the same refused rebase every 15 days and no mission in between).
            let target = rebase_target(entry, region, wing, bases)
                .filter(|_| !state.skipped(now, "airrebase", &name));
            if let Some(base) = target {
                let (ok, why) = state.act(
                    tools,
                    lease,
                    "hoi4_air_rebase",
                    json!({"wing": name, "state_id": base}),
                    "ab",
                )?;
                if why.contains("rebase_refused") {
                    state.skip(now, "airrebase", &name, 30 * DAY);
                    note(
                        state,
                        "air_plan",
                        format!("{name}: the game refused the base in state {base}; the wing flies from its own base for 30 days"),
                    );
                } else if !ok {
                    state.skip(now, "airplan", &name, 7 * DAY);
                }
                return Ok(Some(format!(
                    "air_plan {name} rebase to state {base}: {ok} {why}"
                )));
            }
            let (Some(mission), Some(region_arg)) = (mission, region_arg.as_ref()) else {
                continue;
            };
            let missions = strings(wing, "missions");
            if region_matches(region, &wing["region"]) && missions.iter().any(|m| m == mission) {
                flying += 1;
                continue;
            }
            let (ok, why) = state.act(
                tools,
                lease,
                "hoi4_air_mission",
                json!({"wing": name, "mission": mission, "region": region_arg}),
                "ap",
            )?;
            if !ok {
                state.skip(now, "airplan", &name, 7 * DAY);
            }
            return Ok(Some(format!(
                "air_plan {name} {mission} over region {region_arg}: {ok} {why}"
            )));
        }
        // reserve planes of the entry's airframe become a new wing (W: air.deploy_new_wings)
        let key = item_id(entry).unwrap_or_else(|| index.to_string());
        if deploy_settings(entry).is_none() || state.skipped(now, "airdeploy", &key) {
            continue;
        }
        let stock = stockpile(state, tools, now).unwrap_or(Value::Null);
        let airframe = text(&entry["airframe"]);
        let in_reserve = !airframe.is_empty()
            && list(&stock, "equipment")
                .iter()
                .flat_map(|row| list(row, "variants"))
                .any(|v| text(&v["type"]).starts_with(airframe));
        let designs = if in_reserve {
            tools
                .call("hoi4_designs", &json!({"kind": "plane"}))
                .unwrap_or(Value::Null)
        } else {
            Value::Null
        };
        let (aircraft, planes) = match deploy_choice(entry, &air, &stock, &designs) {
            Ok(choice) => choice,
            Err(why) => {
                deploy_notes.push(format!("{key}: {why}"));
                continue;
            }
        };
        let Some(base) = deploy_base(entry, region, bases, planes) else {
            deploy_notes.push(format!(
                "{key}: {planes} {aircraft} wait: no own air base of the plan with room for them"
            ));
            continue;
        };
        let (ok, why) = state.act(
            tools,
            lease,
            "hoi4_air_wing_create",
            json!({"state_id": base, "aircraft": aircraft}),
            "aw",
        )?;
        if !ok {
            state.skip(now, "airdeploy", &key, 7 * DAY);
        }
        return Ok(Some(format!(
            "air_plan {key}: new wing of {planes} {aircraft} from reserve at state {base}: {ok} {why}"
        )));
    }
    state.skip(now, "mair", "x", 3 * DAY);
    let mut line = format!("{flying} wing(s) on their planned mission");
    if !deploy_notes.is_empty() {
        line.push_str(&format!("; deploy: {}", deploy_notes.join("; ")));
    }
    note(state, "air_plan", line);
    Ok(None)
}

// ---------------------------------------------------------------------------------------------
// Rules 13 and 15: manpower and stability (hoi4_country_status), every 7 game days.

/// The player's casualties so far (hoi4_war_stats `casualties_total`), None when not at war.
fn player_losses(stats: &Value) -> Option<f64> {
    let tag = stats["player"]["tag"].as_str()?;
    list(stats, "wars")
        .iter()
        .filter(|w| w["includes_player"] == json!(true))
        .flat_map(|w| list(w, "sides"))
        .flat_map(|s| list(s, "countries"))
        .filter(|c| c["tag"].as_str() == Some(tag))
        .filter_map(|c| c["casualties_total"].as_f64())
        .reduce(f64::max)
}

fn country_rules(
    state: &mut State,
    tools: &dyn Tools,
    plan: &Value,
    lease: &str,
    now: GameHour,
) -> Result<Option<String>> {
    let m = &plan["manage"];
    if !m.is_object()
        || (off(plan, "manpower") && off(plan, "stability"))
        || state.skipped(now, "mcountry", "x")
    {
        return Ok(None);
    }
    state.skip(now, "mcountry", "x", 7 * DAY);
    let status = tools.call("hoi4_country_status", &json!({})).ok();
    if !off(plan, "stability") {
        let floor = fraction(&m["stability_floor"], 0.5);
        match status
            .as_ref()
            .and_then(|s| s["fields"]["stability"]["value"].as_f64())
        {
            None => note(
                state,
                "stability",
                "unknown: hoi4_country_status gave no stability",
            ),
            Some(v) if v < floor => {
                note(
                    state,
                    "stability",
                    format!("{v} below {floor}: player woken"),
                );
                wake(
                    state,
                    plan,
                    now,
                    15,
                    "stability_low",
                    format!("stability {v} is below the floor {floor}: a decision, advisor or law is needed"),
                    json!({"stability": v, "floor": floor}),
                );
            }
            Some(v) => note(state, "stability", format!("{v} (floor {floor})")),
        }
    }
    if off(plan, "manpower") {
        return Ok(None);
    }
    let Some(total) = tools
        .call("hoi4_war_stats", &json!({}))
        .ok()
        .and_then(|w| player_losses(&w))
    else {
        state.manage.losses.clear();
        note(state, "manpower", "not at war: no losses to cover");
        return Ok(None);
    };
    state.manage.losses.retain(|(h, _)| now - *h <= 120 * DAY);
    state.manage.losses.push((now, total));
    let Some((h0, t0)) = state
        .manage
        .losses
        .iter()
        .copied()
        .find(|(h, _)| now - *h >= 7 * DAY)
    else {
        note(state, "manpower", "collecting loss history (7 game days)");
        return Ok(None);
    };
    let per_day = (total - t0).max(0.0) / ((now - h0) as f64 / DAY as f64);
    let months = m["manpower_months"].as_f64().unwrap_or(2.0);
    let need = (per_day * 30.0 * months).round();
    let Some(free) = status
        .as_ref()
        .and_then(|s| s["fields"]["manpower"]["value"].as_f64())
    else {
        note(
            state,
            "manpower",
            "unknown: hoi4_country_status gave no manpower",
        );
        return Ok(None);
    };
    if free >= need {
        note(
            state,
            "manpower",
            format!("{free} free covers {months} months of losses ({need})"),
        );
        return Ok(None);
    }
    // Only a law the plan lists: conscription costs factory output (-10% to -40% from Service
    // by Requirement on), so with no `conscription_laws` the rule wakes the player instead.
    let ladder = strings(m, "conscription_laws");
    if ladder.is_empty() {
        note(
            state,
            "manpower",
            format!("{free} free < {need}; the plan lists no conscription law: player woken"),
        );
        wake(
            state,
            plan,
            now,
            13,
            "manpower_low",
            format!("free manpower {free} is below {months} months of losses ({need}); manage.conscription_laws lists no law the executor may take"),
            json!({"free_manpower": free, "losses_per_day": round3(per_day),
                "months": months, "need": need}),
        );
        return Ok(None);
    }
    let options = tools
        .call(
            "hoi4_politics_options",
            &json!({"category": "government", "slot": 0, "lease_token": lease}),
        )
        .ok();
    let current = options
        .as_ref()
        .and_then(|o| o["current"].as_str().or(o["current"]["idea"].as_str()))
        .unwrap_or("")
        .to_string();
    let enabled: HashSet<String> = options
        .as_ref()
        .map(|o| {
            list(o, "options")
                .iter()
                .filter(|i| i["enabled"] == json!(true))
                .map(|i| text(&i["idea"]).to_string())
                .collect()
        })
        .unwrap_or_default();
    // The game's own ladder: the slot's window lists the law group in the game files' order,
    // lightest first, so no law key is built in and a mod's laws rank the same way.
    let ladder_order: Vec<String> = options
        .as_ref()
        .map(|o| {
            list(o, "options")
                .iter()
                .map(|i| text(&i["idea"]).to_string())
                .collect()
        })
        .unwrap_or_default();
    // The next listed law after the current one; when the current law is not listed, the first
    // listed law harsher than it on the game's ladder (any listed law when either is unknown).
    let rank = |l: &str| ladder_order.iter().position(|v| v == l);
    let next = match ladder.iter().position(|l| *l == current) {
        Some(i) => ladder[i + 1..].iter().find(|l| enabled.contains(*l)),
        None => ladder.iter().find(|l| {
            enabled.contains(*l)
                && match (rank(l), rank(&current)) {
                    (Some(a), Some(b)) => a > b,
                    _ => true,
                }
        }),
    }
    .cloned();
    let evidence = json!({"free_manpower": free, "losses_per_day": round3(per_day),
        "months": months, "need": need, "law": current});
    let Some(law) = next else {
        note(
            state,
            "manpower",
            format!("{free} free < {need}; no harsher enabled law after {current}"),
        );
        wake(
            state,
            plan,
            now,
            13,
            "manpower_low",
            format!("free manpower {free} is below {months} months of losses ({need}) and no harsher conscription law is enabled"),
            evidence,
        );
        return Ok(None);
    };
    let (ok, why) = state.act(
        tools,
        lease,
        "hoi4_politics_choose",
        json!({"category": "government", "slot": 0, "idea": law}),
        "mp",
    )?;
    note(
        state,
        "manpower",
        format!("{free} free < {need}: {current} -> {law}: {ok}"),
    );
    Ok(Some(format!(
        "manpower {free} < {need}: conscription {law}: {ok} {why}"
    )))
}

// ---------------------------------------------------------------------------------------------
// Rule 4: political power has an owner. The plan's `politics` picks spend it first (tick step
// 4); what they leave above the threshold goes to the standing order below, so a thin plan
// never leaves the power to pile up at the cap.

/// What the last options read of one standing-order slot found (`politics_auto`).
struct SlotRead {
    /// Political power at the read.
    pp: f64,
    /// A wanted idea was offered but disabled. The game disables what the power cannot pay as
    /// well as what its conditions refuse, so such a slot is read again once the power has grown.
    disabled: bool,
    /// Why nothing was bought there.
    why: String,
}

/// The default standing order. It names no game key: every vacant seat of the military staff
/// (chiefs and high command) takes the first advisor the game enables there. Laws and
/// political advisors are the plan's to name in `politics_auto.order`: which law is best and
/// what an advisor brings are judgements the game files answer, not the executor. Until a read
/// has listed the category's seats (or `recheck` after the last listing) the first seat stands
/// for them; then every seat that read showed vacant.
fn politics_auto_default(state: &State, now: GameHour, recheck: GameHour) -> Value {
    let seats = match state.manage.pp_vacant.get("military_staff") {
        Some((at, seats)) if now.saturating_sub(*at) < recheck => seats.clone(),
        _ => vec![0],
    };
    Value::Array(
        seats
            .into_iter()
            .map(|slot| {
                json!({"category": "military_staff", "slot": slot, "cost": 50,
                    "why": "a vacant military staff seat: the first advisor the game enables"})
            })
            .collect(),
    )
}

/// Political power now (hoi4_economy).
pub(super) fn political_power(tools: &dyn Tools) -> Option<f64> {
    tools
        .call("hoi4_economy", &json!({}))
        .ok()
        .and_then(|e| e["political_power"]["raw_signed"].as_f64())
        .map(|raw| raw / 100_000.0)
}

/// The idea an entry takes from a slot's options (`offered`: idea, enabled), or why none and
/// whether a wanted idea is offered but disabled (power or conditions).
fn politics_auto_choice(
    entry: &Value,
    current: &str,
    offered: &[(String, bool)],
    never: &[String],
    conscription: &[String],
) -> std::result::Result<String, (String, bool)> {
    let government_0 = text(&entry["category"]) == "government" && entry["slot"] == json!(0);
    let allowed = |idea: &str| {
        !never.iter().any(|n| n == idea)
            && (!government_0 || conscription.iter().any(|c| c == idea))
    };
    let state_of = |idea: &str| offered.iter().find(|(i, _)| i == idea).map(|(_, on)| *on);
    let ideas = strings(entry, "ideas");
    if ideas.is_empty() {
        // a vacant slot: the first idea the game offers
        if !current.is_empty() {
            return Err((format!("holds {current}"), false));
        }
        if let Some((idea, _)) = offered.iter().find(|(i, on)| *on && allowed(i)) {
            return Ok(idea.clone());
        }
        let waiting = offered.iter().any(|(i, on)| !*on && allowed(i));
        return Err(("vacant; nothing enabled".to_string(), waiting));
    }
    // `ideas` best first: only an idea ranked above the current one; an unlisted current idea
    // (a law the plan or a focus chose) is left alone
    let better = match ideas.iter().position(|i| i == current) {
        Some(at) => &ideas[..at],
        None if current.is_empty() => &ideas[..],
        None => {
            return Err((
                format!("holds {current}, which the order does not rank"),
                false,
            ))
        }
    };
    if let Some(idea) = better
        .iter()
        .find(|i| allowed(i) && state_of(i) == Some(true))
    {
        return Ok(idea.clone());
    }
    let waiting = better
        .iter()
        .any(|i| allowed(i) && state_of(i) == Some(false));
    let held = if current.is_empty() {
        "vacant".to_string()
    } else {
        format!("holds {current}")
    };
    Err((format!("{held}; nothing better enabled"), waiting))
}

/// The power the plan's own pending `politics` picks hold back now (the sum of their `cost`),
/// with what each pick does for the digest. A pick holds its cost only while it could be bought
/// now: its `when` holds (or it has none) and it is not backing off after the game refused it
/// (tick step 4). Power held for a pick the game cannot take is idle power, so a gated or
/// refused pick is paid for once it opens, from the power there is then.
pub(super) fn politics_reserve(
    state: &mut State,
    tools: &dyn Tools,
    plan: &Value,
    now: GameHour,
) -> (f64, Vec<String>) {
    let mut reserve = 0.0;
    let mut lines = Vec::new();
    for (index, pick) in list(plan, "politics").iter().enumerate() {
        let key = super::politics_key(index, pick);
        if state.done.contains_key(&key) {
            continue;
        }
        let Some(cost) = pick["cost"].as_f64() else {
            continue;
        };
        let idea = text(&pick["idea"]).to_string();
        // A pick the game refused while the power was below its cost was refused for power:
        // it keeps its cost (live 16 Nov 2026: the stability pick, refused at 76 of 150, let
        // the standing policy spend 81 points the next day and waited two more months).
        let short_of_power = state.pol_pp_at_fail.get(&key).is_some_and(|pp| *pp < cost);
        if state.skipped(now, "pol", &key) && !short_of_power {
            let left = state
                .bad
                .get(&skip_key("pol", &key))
                .map_or(0, |until| until.saturating_sub(now) / DAY);
            lines.push(format!(
                "{idea} not held (refused, tried again in {left} days)"
            ));
            continue;
        }
        match when_holds(state, tools, &pick["when"], now) {
            Truth::True => {
                reserve += cost;
                lines.push(format!("{idea} holds {cost}"));
            }
            Truth::False => lines.push(format!("{idea} not held (its when does not hold)")),
            Truth::Unknown => lines.push(format!("{idea} not held (its when cannot be read)")),
        }
    }
    (reserve, lines)
}

/// `politics_auto` (default on): political power at or above `threshold` (100) beyond what the
/// plan's own pending `politics` picks can take now ([`politics_reserve`]), or within
/// `cap_margin` (30) of `cap` (2000), buys the first affordable entry of `order` (default
/// [`politics_auto_default`]) the game offers. An entry with a `when` that holds and that the
/// power cannot yet pay is saved for: entries after it spend only the power above its cost. One `hoi4_politics_options` read per entry at
/// most every `recheck_days` (30), or sooner once the power has grown by `recheck_pp` (25) since
/// a read that found a wanted idea disabled; the reads stop for a game day after a scan buys
/// nothing. Every entry passed over says why in the digest (`manage.politics_auto`): short of
/// power, its `when`, the last read's finding and when it is read again. Power at the cap wakes
/// the player (`political_power_at_cap`), and so does power above the threshold that nothing in
/// the order can use for `idle_wake_days` (30) (`political_power_idle`).
pub(super) fn politics_auto_step(
    state: &mut State,
    tools: &dyn Tools,
    plan: &Value,
    lease: &str,
    now: GameHour,
) -> Result<Option<String>> {
    let cfg = &plan["politics_auto"];
    if *cfg == json!(false) || cfg["enabled"] == json!(false) || state.skipped(now, "pauto", "x") {
        return Ok(None);
    }
    let Some(pp) = political_power(tools) else {
        note(state, "politics_auto", "unknown: no political power read");
        state.skip(now, "pauto", "x", DAY);
        return Ok(None);
    };
    let threshold = cfg["threshold"].as_f64().unwrap_or(100.0);
    let cap = cfg["cap"].as_f64().unwrap_or(2000.0);
    let at_cap = pp >= cap - cfg["cap_margin"].as_f64().unwrap_or(30.0);
    let pp_text = pp.round();
    if at_cap {
        wake(
            state,
            plan,
            now,
            4,
            "political_power_at_cap",
            format!("political power {pp_text} sits at the cap {cap}: every day adds nothing"),
            json!({"political_power": pp_text, "cap": cap}),
        );
    }
    let (reserve, picks) = politics_reserve(state, tools, plan, now);
    let budget = if at_cap { pp } else { pp - reserve };
    let reserve_text = if picks.is_empty() {
        format!("plan reserve {reserve}")
    } else {
        format!("plan reserve {reserve}: {}", picks.join(", "))
    };
    if pp < threshold && !at_cap {
        state.manage.pp_idle_since = None;
        note(
            state,
            "politics_auto",
            format!("PP {pp_text} below {threshold}: banked ({reserve_text})"),
        );
        return Ok(None);
    }
    let recheck = cfg["recheck_days"]
        .as_i64()
        .filter(|d| *d > 0)
        .unwrap_or(30)
        * DAY;
    let grow = cfg["recheck_pp"]
        .as_f64()
        .filter(|p| *p > 0.0)
        .unwrap_or(25.0);
    let default_order = politics_auto_default(state, now, recheck);
    let order = if cfg["order"].is_array() {
        &cfg["order"]
    } else {
        &default_order
    };
    let never = strings(cfg, "never");
    let conscription = strings(&plan["manage"], "conscription_laws");
    let mut seen: Vec<String> = Vec::new();
    // Whether the power could have paid for any entry: power short of every entry's cost is
    // being saved for the next buy (or the plan's reserve), not idle.
    let mut could_buy = false;
    // Power held back for an earlier entry whose `when` holds but which the power cannot yet pay:
    // the order is a priority, so a cheaper entry after it spends only what lies above its cost
    // (live 15 Nov 2026: the stability advisor, first in the order, never reached its 150 while
    // the power went to 50-point staff picks below it, and stability sat at 0.42 all war).
    let mut saving = 0.0;
    for entry in order.as_array().map_or(&[][..], Vec::as_slice) {
        let (category, slot) = (text(&entry["category"]).to_string(), &entry["slot"]);
        let slot_key = format!("{category}:{slot}");
        let cost = entry["cost"]
            .as_f64()
            .unwrap_or(if category == "military_staff" {
                50.0
            } else {
                150.0
            });
        if budget - saving < cost {
            let urgent = saving == 0.0
                && !entry["when"].is_null()
                && !state.skipped(now, "pauto", &slot_key)
                && matches!(when_holds(state, tools, &entry["when"], now), Truth::True);
            if urgent {
                saving = cost;
                seen.push(format!(
                    "{slot_key} needs {cost} and its when holds: power is saved for it ({} free above the reserve)",
                    budget.round()
                ));
            } else {
                seen.push(format!(
                    "{slot_key} needs {cost}, {} free above the reserve",
                    (budget - saving).round()
                ));
            }
            continue;
        }
        could_buy = true;
        if state.skipped(now, "pauto", &slot_key) {
            let last = state.manage.pp_reads.get(&slot_key);
            if !last.is_some_and(|r| r.disabled && pp >= r.pp + grow) {
                let left = state
                    .bad
                    .get(&skip_key("pauto", &slot_key))
                    .map_or(0, |until| until.saturating_sub(now) / DAY);
                let (why, sooner) =
                    last.map_or((String::from("read lately"), String::new()), |r| {
                        let sooner = if r.disabled {
                            format!(" or at PP {}", (r.pp + grow).round())
                        } else {
                            String::new()
                        };
                        (r.why.clone(), sooner)
                    });
                seen.push(format!(
                    "{slot_key} {why}: read again in {left} days{sooner}"
                ));
                continue;
            }
        }
        match when_holds(state, tools, &entry["when"], now) {
            Truth::True => {}
            Truth::False => {
                seen.push(format!("{slot_key} waits: its when does not hold"));
                continue;
            }
            Truth::Unknown => {
                seen.push(format!("{slot_key} waits: its when cannot be read"));
                continue;
            }
        }
        let options = tools.call(
            "hoi4_politics_options",
            &json!({"category": category, "slot": slot, "lease_token": lease}),
        );
        let Ok(options) = options else {
            state.skip(now, "pauto", &slot_key, recheck);
            let why = "unreadable".to_string();
            seen.push(format!("{slot_key} {why}"));
            state.manage.pp_reads.insert(
                slot_key,
                SlotRead {
                    pp,
                    disabled: false,
                    why,
                },
            );
            continue;
        };
        // every seat of the category and whether it is vacant (the default order's seats)
        if let Some(seats) = options["slots"].as_array() {
            let vacant: Vec<i64> = seats
                .iter()
                .filter(|s| s["current"].is_null() && s["unavailable"].is_null())
                .filter_map(|s| s["slot"].as_i64())
                .collect();
            state
                .manage
                .pp_vacant
                .insert(category.clone(), (now, vacant));
        }
        let current = options["current"]
            .as_str()
            .or(options["current"]["idea"].as_str())
            .unwrap_or("")
            .to_string();
        let offered: Vec<(String, bool)> = list(&options, "options")
            .iter()
            .map(|i| (text(&i["idea"]).to_string(), i["enabled"] == json!(true)))
            .collect();
        let idea = match politics_auto_choice(entry, &current, &offered, &never, &conscription) {
            Ok(idea) => idea,
            Err((why, disabled)) => {
                state.skip(now, "pauto", &slot_key, recheck);
                let shown = if disabled {
                    format!(
                        "{why} (a wanted idea is disabled: read again at PP {})",
                        (pp + grow).round()
                    )
                } else {
                    why.clone()
                };
                seen.push(format!("{slot_key} {shown}"));
                state
                    .manage
                    .pp_reads
                    .insert(slot_key, SlotRead { pp, disabled, why });
                continue;
            }
        };
        let (ok, why) = state.act(
            tools,
            lease,
            "hoi4_politics_choose",
            json!({"category": category, "slot": slot, "idea": idea}),
            "pa",
        )?;
        if ok {
            state.manage.pp_reads.remove(&slot_key);
            // the seat is no longer vacant: the default order reads the seats again
            state.manage.pp_vacant.remove(&category);
        } else {
            state.skip(now, "pauto", &slot_key, 7 * DAY);
            state.manage.pp_reads.insert(
                slot_key.clone(),
                SlotRead {
                    pp,
                    disabled: false,
                    why: format!("{idea} refused ({})", why.split(':').next().unwrap_or("")),
                },
            );
        }
        let reason = entry["why"].as_str().unwrap_or("politics_auto order");
        let from = if current.is_empty() {
            "vacant"
        } else {
            current.as_str()
        };
        let line =
            format!("PP {pp_text} ({reserve_text}): {slot_key} {from} -> {idea} ({reason}): {ok}");
        let picks = state
            .manage
            .notes
            .get("politics_auto_picks")
            .map_or_else(String::new, |p| format!("{p}; "));
        let at = crate::calendar::decode(i32::try_from(now).unwrap_or(0))
            .map_or_else(|_| format!("hour {now}"), |d| d.text);
        note(
            state,
            "politics_auto_picks",
            format!("{picks}{at} {idea}: {ok}"),
        );
        note(state, "politics_auto", line.clone());
        state.manage.pp_idle_since = None;
        return Ok(Some(format!("politics_auto {line} {why}")));
    }
    if seen.is_empty() {
        seen.push(if cfg["order"].is_array() {
            "politics_auto.order is empty".to_string()
        } else {
            "the default order found no vacant military staff seat".to_string()
        });
    }
    // nothing bought: no reads again for a game day; power above the threshold with nothing to
    // buy for long wakes the player
    state.skip(now, "pauto", "x", DAY);
    if !could_buy {
        state.manage.pp_idle_since = None;
        note(
            state,
            "politics_auto",
            format!(
                "PP {pp_text} ({reserve_text}): saved for the next buy [{}]",
                seen.join(", ")
            ),
        );
        return Ok(None);
    }
    let since = *state.manage.pp_idle_since.get_or_insert(now);
    let idle_days = cfg["idle_wake_days"].as_i64().unwrap_or(30);
    if now.saturating_sub(since) >= idle_days * DAY {
        wake(
            state,
            plan,
            now,
            4,
            "political_power_idle",
            format!("political power {pp_text} has had nothing to buy for {idle_days} game days ({}); name advisors, laws or decisions in politics_auto.order or the plan's politics", seen.join("; ")),
            json!({"political_power": pp_text, "reserve": reserve, "picks": picks, "order": seen}),
        );
    }
    note(
        state,
        "politics_auto",
        format!(
            "PP {pp_text} ({reserve_text}): nothing to buy [{}]",
            seen.join(", ")
        ),
    );
    Ok(None)
}

// ---------------------------------------------------------------------------------------------
// Rule 17 (supply.split_overdrawn): a stack that draws more than its province receives starves
// every division in it alike; pulling out enough of them restores the rest.

fn division_id(v: &Value) -> Option<String> {
    v.as_str()
        .map(String::from)
        .or_else(|| v.as_i64().map(|n| n.to_string()))
}

/// The divisions to pull out of an overdrawn province: the army with most divisions there, its
/// weakest ones first (lowest equipment fill, then hit points), until their supply need covers
/// the province's shortfall, at most `share` of its divisions there (at least one, at most 18,
/// never the whole army). (army, division ids, their supply need) or None.
fn overdrawn_pick(
    rows: &[&Value],
    shortfall: f64,
    share: f64,
    army_size: impl Fn(&str) -> usize,
    skip_army: impl Fn(&str) -> bool,
) -> Option<(String, Vec<String>, f64)> {
    let mut by_army: BTreeMap<String, Vec<&Value>> = BTreeMap::new();
    for r in rows {
        if let Some(a) = r["army"].as_str().filter(|a| !a.is_empty()) {
            by_army.entry(a.to_string()).or_default().push(r);
        }
    }
    let (army, mut here) = by_army
        .into_iter()
        .filter(|(a, _)| !skip_army(a))
        .max_by_key(|(a, v)| (v.len(), std::cmp::Reverse(a.clone())))?;
    let total = army_size(&army).max(here.len());
    let cap = ((here.len() as f64 * share).floor() as usize)
        .clamp(1, 18)
        .min(total.saturating_sub(1));
    if cap == 0 {
        return None;
    }
    let key = |r: &Value, k: &str| r[k].as_f64().unwrap_or(1.0);
    here.sort_by(|a, b| {
        key(a, "equipment_fill_min")
            .total_cmp(&key(b, "equipment_fill_min"))
            .then(key(a, "hp_percent").total_cmp(&key(b, "hp_percent")))
    });
    let (mut ids, mut need) = (Vec::new(), 0.0);
    for r in here {
        if ids.len() >= cap || (need >= shortfall && !ids.is_empty()) {
            break;
        }
        if let Some(id) = division_id(&r["id"]) {
            ids.push(id);
            need += r["supply_need"].as_f64().unwrap_or(0.0);
        }
    }
    (!ids.is_empty()).then_some((army, ids, need))
}

/// Where the pulled-out divisions go: a province where own divisions stand that still has
/// `need` of headroom, in the source's state first, then where the same army stands, then
/// anywhere; the most headroom first. (state, province) or None.
fn overdrawn_destination(read: &Value, source: i64, army: &str, need: f64) -> Option<(i64, i64)> {
    let src_state = list(read, "provinces")
        .iter()
        .find(|p| p["province_id"].as_i64() == Some(source))
        .and_then(|p| p["state_id"].as_i64());
    let army_provinces: HashSet<i64> = list(read, "divisions")
        .iter()
        .filter(|d| d["army"].as_str() == Some(army))
        .filter_map(|d| d["province_id"].as_i64())
        .collect();
    list(read, "provinces")
        .iter()
        .filter_map(|p| {
            let (pid, sid) = (p["province_id"].as_i64()?, p["state_id"].as_i64()?);
            let room = p["headroom"].as_f64()?;
            (pid != source && room >= need).then_some((pid, sid, room))
        })
        .max_by(|a, b| {
            let rank = |(pid, sid, _): &(i64, i64, f64)| {
                (Some(*sid) == src_state, army_provinces.contains(pid))
            };
            rank(a).cmp(&rank(b)).then(a.2.total_cmp(&b.2))
        })
        .map(|(pid, sid, _)| (sid, pid))
}

/// Whether `army` is a group the overdrawn rule took out of a stack: it is on its way to fed
/// ground or rests there, and is not merged back into a formation meanwhile (merged the tick
/// after its detach it never moved, and its divisions went into the wrong formation).
pub(super) fn spread_out(state: &State, army: &str) -> bool {
    state.manage.spread.iter().any(|s| s.army == army)
}

/// supply.split_overdrawn (`manage.overdrawn_floor` 35, `overdrawn_share` 0.5,
/// `overdrawn_rest_days` 10), once a game day from hoi4_supply: in a province on
/// `overdrawn_provinces` whose divisions' mean supply is below the floor (attrition starts at
/// 35%), the weakest divisions of the army with most divisions there are detached into their own
/// army (hoi4_army_detach), at most `overdrawn_share` of them, until their supply need covers the
/// shortfall; the next tick moves that army to the province with room for them
/// (hoi4_army_move: in the same state first, then where the same army stands). The new army rests
/// there for `overdrawn_rest_days`: the war step does not drive it and the army consolidation
/// does not fold it back until then. No province with room: nothing is detached and the digest
/// says so (the supply relief's railway and hub levers remain).
fn overdrawn_rule(
    state: &mut State,
    tools: &dyn Tools,
    plan: &Value,
    lease: &str,
    now: GameHour,
) -> Result<Option<String>> {
    let m = &plan["manage"];
    // An army on an objective strike is one order: taking divisions out of it undoes the strike.
    if !m.is_object() || off(plan, "overdrawn") || super::plan_strike::owns_armies(state, plan) {
        return Ok(None);
    }
    let rest = m["overdrawn_rest_days"]
        .as_i64()
        .filter(|d| *d >= 0)
        .unwrap_or(10)
        * DAY;
    // a detached group moves on the tick after its detach
    if let Some(at) = state.manage.spread.iter().position(|s| !s.moved) {
        let go = state.manage.spread[at].clone();
        let (ok, why) = state.act(
            tools,
            lease,
            "hoi4_army_move",
            json!({"army": go.army, "state_id": go.state, "province_id": go.province}),
            "so",
        )?;
        state.manage.spread[at].moved = true;
        let line = format!(
            "{} out of the overdrawn stack to state {} province {}: {ok} {why}",
            go.army, go.state, go.province
        );
        note(state, "overdrawn", line.clone());
        return Ok(Some(format!("overdrawn: {line}")));
    }
    state.manage.spread.retain(|s| now - s.since < rest);
    if state.skipped(now, "mspread", "x") {
        return Ok(None);
    }
    state.skip(now, "mspread", "x", DAY);
    let Some(read) = supply(state, tools, now) else {
        note(state, "overdrawn", "unknown: hoi4_supply unreadable");
        return Ok(None);
    };
    let floor = fraction(&m["overdrawn_floor"], 0.35);
    let share = fraction(&m["overdrawn_share"], 0.5);
    let divisions: HashMap<String, &Value> = list(&read, "divisions")
        .iter()
        .filter_map(|d| Some((division_id(&d["id"])?, d)))
        .collect();
    let sizes: HashMap<String, usize> = list(&read, "armies")
        .iter()
        .filter_map(|a| {
            Some((
                a["army"].as_str()?.to_string(),
                a["divisions"].as_u64()? as usize,
            ))
        })
        .collect();
    let mut lines: Vec<String> = Vec::new();
    for o in list(&read, "overdrawn_provinces") {
        let Some(pid) = o["province_id"].as_i64() else {
            continue;
        };
        if state.skipped(now, "spread", &pid.to_string()) {
            continue;
        }
        let rows: Vec<&Value> = list(o, "division_ids")
            .iter()
            .filter_map(|id| divisions.get(&division_id(id)?).copied())
            .collect();
        let pcts: Vec<f64> = rows
            .iter()
            .filter_map(|r| r["supply_percent"].as_f64())
            .collect();
        if pcts.is_empty() {
            continue;
        }
        let mean = pcts.iter().sum::<f64>() / pcts.len() as f64;
        if mean >= floor * 100.0 {
            lines.push(format!(
                "province {pid}: {mean:.0}% supply, above the floor"
            ));
            continue;
        }
        let shortfall = o["shortfall"].as_f64().unwrap_or(0.0);
        let Some((army, ids, need)) = overdrawn_pick(
            &rows,
            shortfall,
            share,
            |a| sizes.get(a).copied().unwrap_or(0),
            |a| {
                super::plan_military::holding(state, a)
                    || super::plan_military::invading(state, plan, a)
                    || state.manage.spread.iter().any(|s| s.army == a)
            },
        ) else {
            lines.push(format!("province {pid}: no army whose divisions may leave"));
            continue;
        };
        let Some((to_state, to_province)) = overdrawn_destination(&read, pid, &army, need) else {
            state.skip(now, "spread", &pid.to_string(), 3 * DAY);
            lines.push(format!(
                "province {pid} ({mean:.0}% supply): no province with {need:.2} supply of room for {} of {army}'s divisions; railway or hub needed",
                ids.len()
            ));
            continue;
        };
        let present: HashSet<String> = sizes.keys().cloned().collect();
        let (ok, why) = state.act(
            tools,
            lease,
            "hoi4_army_detach",
            json!({"army": army, "divisions": ids}),
            "so",
        )?;
        state.skip(now, "spread", &pid.to_string(), 5 * DAY);
        let new = if ok {
            tools
                .call("hoi4_armies", &json!({"threat": false}))
                .ok()
                .and_then(|after| {
                    list(&after, "armies")
                        .iter()
                        .map(|a| text(&a["army"]).to_string())
                        .find(|n| !n.is_empty() && !present.contains(n))
                })
        } else {
            None
        };
        if let Some(new) = &new {
            state.war_cool.insert(new.clone(), now + rest);
            state.manage.spread.push(Spread {
                army: new.clone(),
                state: to_state,
                province: to_province,
                moved: false,
                since: now,
            });
        }
        let line = format!(
            "province {pid} at {mean:.0}% supply (shortfall {shortfall:.2}): {} of {army}'s divisions out into {} for state {to_state} province {to_province}: {ok} {why}",
            ids.len(),
            new.as_deref().unwrap_or("no new army seen")
        );
        note(state, "overdrawn", line.clone());
        return Ok(Some(format!("overdrawn: {line}")));
    }
    let line = if lines.is_empty() {
        "no overdrawn province".to_string()
    } else {
        lines.join("; ")
    };
    note(state, "overdrawn", line);
    Ok(None)
}

// ---------------------------------------------------------------------------------------------
// Rule 10: front equipment fill (hoi4_supply), every 3 game days.

fn equipment_rule(state: &mut State, tools: &dyn Tools, plan: &Value, now: GameHour) {
    if !plan["manage"].is_object()
        || off(plan, "equipment_fill")
        || state.skipped(now, "mfill", "x")
    {
        return;
    }
    state.skip(now, "mfill", "x", 3 * DAY);
    let floor = fraction(&plan["manage"]["equipment_fill_floor"], 0.8);
    let Some(read) = supply(state, tools, now) else {
        note(state, "equipment_fill", "unknown: hoi4_supply unreadable");
        return;
    };
    let mut armies: BTreeMap<String, (f64, usize, BTreeMap<String, usize>)> = BTreeMap::new();
    for d in list(&read, "divisions") {
        let (Some(army), Some(fill)) = (d["army"].as_str(), d["equipment_fill_min"].as_f64())
        else {
            continue;
        };
        let entry = armies.entry(army.to_string()).or_default();
        entry.0 += fill;
        entry.1 += 1;
        for s in strings(d, "short_equipment") {
            *entry.2.entry(s).or_default() += 1;
        }
    }
    if armies.is_empty() {
        note(
            state,
            "equipment_fill",
            "unknown: no division equipment readable",
        );
        return;
    }
    let low: Vec<Value> = armies
        .iter()
        .filter_map(|(army, (sum, n, short))| {
            let mean = sum / *n as f64;
            (mean < floor).then(|| {
                json!({"army": army, "equipment_fill_mean": round3(mean), "divisions": n,
                    "short_equipment": short})
            })
        })
        .collect();
    if low.is_empty() {
        note(
            state,
            "equipment_fill",
            format!("every army at or above {floor}"),
        );
        return;
    }
    let names: Vec<&str> = low.iter().filter_map(|a| a["army"].as_str()).collect();
    let mut line = format!("below {floor}: {}", names.join(", "));
    if plan["disband"].is_object() {
        let cleared = nudge(state, &["disband"]);
        line.push_str(&format!("; disband step nudged ({cleared} timers)"));
    }
    note(state, "equipment_fill", line);
    wake(
        state,
        plan,
        now,
        10,
        "equipment_fill_low",
        format!("{} army(ies) below {floor} mean equipment fill: reinforce priority, production or disbanding is needed", low.len()),
        json!({"armies": low, "floor": floor}),
    );
}

// ---------------------------------------------------------------------------------------------
// Production: idle military factories in small steps, and the day-0 assignment.

/// Military lines (entry index, equipment, requested factories) and the military factories on
/// no line (`hoi4_economy` total minus the lines' requested factories).
fn military_lines(tools: &dyn Tools, economy: &Value) -> Result<(Vec<MilLine>, i64)> {
    let total = economy["factory_trigger_count"]["military"]
        .as_i64()
        .unwrap_or(0);
    let lines = tools.call("hoi4_production_lines", &json!({"limit": 128}))?;
    let military: Vec<MilLine> = list(&lines, "lines")
        .iter()
        .filter(|l| l["line_kind"] == json!("military"))
        .map(|l| {
            (
                l["entry_index"].as_i64().unwrap_or(-1),
                text(&l["equipment_id"]).to_string(),
                l["factory_counters"]["requested_factories_raw_signed"]
                    .as_i64()
                    .unwrap_or(0),
                l["output"]["efficiency"].as_f64(),
            )
        })
        .collect();
    let idle = total - military.iter().map(|m| m.2).sum::<i64>();
    Ok((military, idle))
}

/// Equipment the fielded divisions lack per archetype: hoi4_stockpile fielded_short when read,
/// else hoi4_supply (need minus held).
fn shortfall(stock: Option<&Value>, supply: Option<&Value>) -> HashMap<String, f64> {
    if let Some(rows) = stock.and_then(|s| s["equipment"].as_array()) {
        return rows
            .iter()
            .filter_map(|r| {
                let short = r["fielded_short"].as_f64()?;
                (short > 0.0).then(|| (text(&r["archetype"]).to_string(), short))
            })
            .collect();
    }
    let mut out: HashMap<String, f64> = HashMap::new();
    for d in supply.map_or(&[][..], |s| list(s, "divisions")) {
        for e in list(d, "equipment") {
            let (have, need) = (
                e["have"].as_f64().unwrap_or(0.0),
                e["need"].as_f64().unwrap_or(0.0),
            );
            if need > have {
                *out.entry(text(&e["archetype"]).to_string()).or_default() += need - have;
            }
        }
    }
    out
}

/// One military line: (entry index, equipment, requested factories, efficiency percent read).
type MilLine = (i64, String, i64, Option<f64>);

/// The line that gets the next idle factories: of the `production_priority` lines (the plan's
/// removed equipment never), the one whose archetype the divisions lack most; with no shortage
/// known, the priority lines in turn. The reason goes with it.
fn feed_choice(
    state: &mut State,
    tools: &dyn Tools,
    plan: &Value,
    now: GameHour,
    military: &[MilLine],
) -> Option<(MilLine, String)> {
    let mut priority = strings(plan, "production_priority");
    if priority.is_empty() {
        priority = [
            "infantry_equipment",
            "support_equipment",
            "artillery_equipment",
        ]
        .map(String::from)
        .to_vec();
    }
    let removed: HashSet<String> = list(plan, "production")
        .iter()
        .filter(|l| l["remove"] == json!(true))
        .map(|l| text(&l["equipment"]).to_string())
        .collect();
    let mut lines: Vec<&MilLine> = Vec::new();
    for want in &priority {
        for m in military {
            if !m.1.is_empty()
                && m.1.starts_with(want.as_str())
                && !removed.contains(&m.1)
                && !lines.iter().any(|l| l.0 == m.0)
            {
                lines.push(m);
            }
        }
    }
    if lines.is_empty() {
        return None;
    }
    let stock = stockpile(state, tools, now);
    let read = if stock.is_some() {
        None
    } else {
        supply(state, tools, now)
    };
    let short = shortfall(stock.as_ref(), read.as_ref());
    let lack = |l: &MilLine| -> f64 {
        short
            .iter()
            .filter(|(a, _)| !a.is_empty() && l.1.starts_with(a.as_str()))
            .map(|(_, n)| *n)
            .sum()
    };
    // the first line of the largest shortfall (priority order breaks ties)
    let mut best: Option<(&MilLine, f64)> = None;
    for l in &lines {
        let n = lack(l);
        if n > 0.0 && best.is_none_or(|(_, b)| n > b) {
            best = Some((l, n));
        }
    }
    if let Some((l, n)) = best {
        return Some((l.clone(), format!("fielded divisions lack {n:.0}")));
    }
    // no shortage: the line whose stockpile runs out first (a falling stock only)
    let cover = |l: &MilLine| -> Option<f64> {
        let rows = stock.as_ref()?["equipment"].as_array()?;
        rows.iter()
            .filter(|r| {
                let a = text(&r["archetype"]);
                !a.is_empty() && l.1.starts_with(a)
            })
            .filter_map(|r| r["days_of_cover"].as_f64())
            .reduce(f64::min)
    };
    let mut first: Option<(&MilLine, f64)> = None;
    for l in &lines {
        if let Some(d) = cover(l) {
            if first.is_none_or(|(_, b)| d < b) {
                first = Some((l, d));
            }
        }
    }
    if let Some((l, d)) = first {
        return Some((
            l.clone(),
            format!("no shortage; fewest days of stockpile cover ({d:.0})"),
        ));
    }
    let turn = state.manage.feed_turn % lines.len();
    state.manage.feed_turn = state.manage.feed_turn.wrapping_add(1);
    Some((
        lines[turn].clone(),
        "no shortage known: priority lines in turn".into(),
    ))
}

/// A few idle factories (`manage.rebalance_step`, default 3) to the chosen line.
fn feed(
    state: &mut State,
    tools: &dyn Tools,
    plan: &Value,
    lease: &str,
    now: GameHour,
    military: &[MilLine],
    idle: i64,
) -> Result<Option<String>> {
    let Some(((index, equipment, factories, efficiency), why)) =
        feed_choice(state, tools, plan, now, military)
    else {
        return Ok(None);
    };
    let why = match efficiency {
        Some(e) => format!("{why}; line efficiency {e:.0}%"),
        None => why,
    };
    let step = plan["manage"]["rebalance_step"]
        .as_i64()
        .filter(|n| *n > 0)
        .unwrap_or(3);
    let give = idle.min(step);
    let (ok, result) = state.act(
        tools,
        lease,
        "hoi4_production_set",
        json!({"line": index, "equipment": equipment, "factories": factories + give}),
        "ps",
    )?;
    if !ok {
        state.skip(now, "prodset", "x", 2 * DAY);
    }
    note(
        state,
        "production",
        format!("{idle} idle military factories: +{give} to {equipment} ({why})"),
    );
    Ok(Some(format!(
        "production_set {equipment} {factories}+{give}: {ok} {result}"
    )))
}

/// Tick step 6b: military factories on no line go to the priority lines a few at a time,
/// re-chosen every action, once at least `production_min_idle` (2) are idle. Factories are never
/// taken from a line.
pub(super) fn rebalance_step(
    state: &mut State,
    tools: &dyn Tools,
    plan: &Value,
    lease: &str,
    now: GameHour,
    economy: &Value,
) -> Result<Option<String>> {
    let (military, idle) = military_lines(tools, economy)?;
    if idle < plan["production_min_idle"].as_i64().unwrap_or(2)
        || state.skipped(now, "prodset", "x")
    {
        return Ok(None);
    }
    feed(state, tools, plan, lease, now, &military, idle)
}

/// Before the first unpause (the game paused at the hour this server first saw, no work done
/// yet, a plan with `production` or `production_priority`): free military factories go to the
/// starting lines the plan keeps, first up to a `production[]` entry's factories for an existing
/// line, then the priority lines a few at a time. Runs before everything else; once over (or
/// `manage.day0_production` false) never again (done-memory `day0prod`).
pub(super) fn day0_step(
    state: &mut State,
    tools: &dyn Tools,
    plan: &Value,
    lease: &str,
    now: GameHour,
) -> Result<Option<String>> {
    const KEY: &str = "day0prod";
    if state.done.contains_key(KEY)
        || !(plan["production"].is_array() || plan["production_priority"].is_array())
    {
        return Ok(None);
    }
    let first = state.con.baseline.is_none_or(|b| b >= now);
    let other_work = state.done.keys().any(|k| k != KEY);
    let paused = || {
        tools
            .call("hoi4_overview", &json!({}))
            .is_ok_and(|o| o["paused"] == json!(true))
    };
    if plan["manage"]["day0_production"] == json!(false) || !first || other_work || !paused() {
        state.done.insert(KEY.into(), now);
        return Ok(None);
    }
    let economy = tools.call("hoi4_economy", &json!({}))?;
    let (military, idle) = military_lines(tools, &economy)?;
    if idle <= 0 {
        note(state, "production", "day 0: no free military factory");
        state.done.insert(KEY.into(), now);
        return Ok(None);
    }
    for line in list(plan, "production") {
        if line["remove"] == json!(true) {
            continue;
        }
        let equipment = text(&line["equipment"]);
        let Some(target) = line["factories"].as_i64() else {
            continue;
        };
        let Some((index, have, current, _)) = military
            .iter()
            .find(|m| !equipment.is_empty() && m.1 == equipment)
            .cloned()
        else {
            continue;
        };
        if current >= target {
            continue;
        }
        let give = (target - current).min(idle);
        let (ok, why) = state.act(
            tools,
            lease,
            "hoi4_production_set",
            json!({"line": index, "equipment": have, "factories": current + give}),
            "p0",
        )?;
        if !ok {
            state.done.insert(KEY.into(), now);
        }
        note(
            state,
            "production",
            format!("day 0: starting line {have} {current}+{give} (plan factories {target})"),
        );
        return Ok(Some(format!(
            "day 0 production_set {have} {current}+{give}: {ok} {why}"
        )));
    }
    match feed(state, tools, plan, lease, now, &military, idle)? {
        Some(line) => {
            if line.contains(": false") {
                state.done.insert(KEY.into(), now);
            }
            Ok(Some(format!("day 0 {line}")))
        }
        None => {
            state.done.insert(KEY.into(), now);
            Ok(None)
        }
    }
}

/// At war with one of `war.enemies` (hoi4_wars through the predicate cache); unknown is no.
fn at_war(state: &mut State, tools: &dyn Tools, plan: &Value, now: GameHour) -> bool {
    let any: Vec<Value> = strings(&plan["war"], "enemies")
        .iter()
        .map(|t| json!(format!("at_war_with:{t}")))
        .collect();
    !any.is_empty() && super::judge(state, tools, &json!({ "any": any }), now) == Truth::True
}

/// E42: at war the `construction` filler drops new civilian factories (a civilian factory pays
/// back in years) unless `manage.war_civs` is true or the goal deadline is more than a year away.
pub(super) fn war_drops_civs(
    state: &mut State,
    tools: &dyn Tools,
    plan: &Value,
    now: GameHour,
) -> bool {
    if plan["manage"]["war_civs"] == json!(true) {
        return false;
    }
    if let Some(deadline) = plan["goal"]["deadline"].as_str().and_then(super::date_hour) {
        if deadline - now > 365 * DAY {
            return false;
        }
    }
    at_war(state, tools, plan, now)
}

/// Rules 11 and 21: stockpile cover and trucks (hoi4_stockpile, once a game day). The equipment
/// whose cover is lowest below its target gets `rebalance_step` idle military factories on its
/// line (a new line when none builds it); with no idle factory a cover below
/// `stockpile_days_critical` wakes the player.
fn stockpile_rule(
    state: &mut State,
    tools: &dyn Tools,
    plan: &Value,
    lease: &str,
    now: GameHour,
) -> Result<Option<String>> {
    let m = &plan["manage"];
    let (stock_on, trucks_on) = (!off(plan, "stockpile"), !off(plan, "trucks"));
    if !m.is_object() || !(stock_on || trucks_on) || state.skipped(now, "mstock", "x") {
        return Ok(None);
    }
    state.skip(now, "mstock", "x", DAY);
    let critical = m["stockpile_days_critical"].as_f64().unwrap_or(7.0);
    let warning = m["stockpile_days_warning"].as_f64().unwrap_or(30.0);
    let ratio = m["trucks_ratio"].as_f64().unwrap_or(1.5);
    let Some(read) = stockpile(state, tools, now) else {
        note(
            state,
            "stockpile_days",
            "unknown: hoi4_stockpile unreadable",
        );
        return Ok(None);
    };
    // (equipment, archetype, days of cover, why it is low)
    let mut low: Vec<(String, String, f64, String)> = Vec::new();
    let mut seen: Vec<String> = Vec::new();
    if stock_on {
        let mut targets: Vec<(String, f64)> = plan["stockpile_targets"]
            .as_object()
            .into_iter()
            .flatten()
            .filter(|(k, _)| k.as_str() != "why" && k.as_str() != "note")
            .filter_map(|(k, v)| v.as_f64().map(|d| (k.clone(), d)))
            .collect();
        if targets.is_empty() {
            targets.push(("infantry_equipment".into(), warning));
        }
        for (equipment, want) in targets {
            match crate::plan_predicate::stockpile_days(&read, &equipment) {
                Ok(days) => {
                    seen.push(format!("{equipment} {days:.0}/{want:.0}"));
                    if days < want {
                        let arche = archetype_row(&read, &equipment).unwrap_or(equipment.clone());
                        low.push((
                            equipment,
                            arche,
                            days,
                            format!("{days:.0} days of cover, wanted {want:.0}"),
                        ));
                    }
                }
                Err(why) => seen.push(format!("{equipment} unknown ({why})")),
            }
        }
    }
    if trucks_on {
        if let Some(t) = read["trucks"].as_object() {
            let stock = t.get("stockpile").and_then(Value::as_f64).unwrap_or(0.0);
            let need = t.get("fielded_need").and_then(Value::as_f64).unwrap_or(0.0);
            let days = t.get("days_of_cover").and_then(Value::as_f64);
            seen.push(format!(
                "trucks {stock:.0} in stock, {need:.0} needed, cover {}",
                days.map_or("not falling or unmeasured".into(), |d| format!(
                    "{d:.0} days"
                ))
            ));
            let short_ratio = need > 0.0 && stock < ratio * need;
            if short_ratio || days.is_some_and(|d| d < warning) {
                let why = if short_ratio {
                    format!("{stock:.0} trucks, below {ratio} x the {need:.0} the divisions need")
                } else {
                    format!(
                        "{:.0} days of truck cover, wanted {warning:.0}",
                        days.unwrap_or(0.0)
                    )
                };
                low.push((
                    "motorized_equipment".into(),
                    "motorized_equipment".into(),
                    days.unwrap_or(0.0),
                    why,
                ));
            }
        }
    }
    // Equipment the fielded divisions lack with nothing in stock counts as low even when the
    // plan named no target for it (a shortage at the front is never "not planned").
    if stock_on {
        for row in list(&read, "equipment") {
            let arche = text(&row["archetype"]).to_string();
            let short = row["fielded_short"].as_f64().unwrap_or(0.0);
            let stock = row["stockpile"].as_f64().unwrap_or(-1.0);
            if arche.is_empty() || short <= 0.0 || stock > short || low.iter().any(|l| l.1 == arche)
            {
                continue;
            }
            low.push((
                arche.clone(),
                arche,
                0.0,
                format!("the divisions lack {short:.0} and {stock:.0} are in stock"),
            ));
        }
    }
    note(
        state,
        "stockpile_days",
        format!("cover (days/wanted): {}", seen.join("; ")),
    );
    let Some((equipment, archetype, days, why)) = low
        .into_iter()
        .min_by(|a, b| a.2.partial_cmp(&b.2).unwrap_or(std::cmp::Ordering::Equal))
    else {
        return Ok(None);
    };
    if state.skipped(now, "mstockline", &archetype) {
        return Ok(None);
    }
    let economy = tools.call("hoi4_economy", &json!({}))?;
    let (military, idle) = military_lines(tools, &economy)?;
    let step = m["rebalance_step"].as_i64().filter(|n| *n > 0).unwrap_or(3);
    let give = idle.min(step);
    // No idle factory: a shortage the divisions feel (or a cover under the critical days) takes
    // factories from a line whose equipment nobody lacks and whose stock covers twice its
    // target. Without this owner a stock stayed at 0 for eight months while every factory
    // built what was already plenty (live 15 Nov 2026: artillery 0 from May, the strike army
    // at a sixth of its equipment, the missing-production alert open all year).
    let felt = short_of(&read, &archetype) > 0.0 || days < critical;
    if give <= 0 && felt && m["shift_factories"] != json!(false) {
        if let Some(((index, have, factories, _), take)) =
            donor_line(&read, plan, &military, &archetype, warning, step)
        {
            let (ok, result) = state.act(
                tools,
                lease,
                "hoi4_production_set",
                json!({"line": index, "equipment": have, "factories": factories - take}),
                "pk",
            )?;
            let mut line = format!("{have} {factories}-{take}: {ok} {result}");
            if ok {
                let target = military
                    .iter()
                    .filter(|l| l.1.starts_with(archetype.as_str()))
                    .max_by_key(|l| l.2)
                    .cloned();
                let (ok2, result2) = match &target {
                    Some((i, eq, f, _)) => state.act(
                        tools,
                        lease,
                        "hoi4_production_set",
                        json!({"line": i, "equipment": eq, "factories": f + take}),
                        "pk",
                    )?,
                    None => state.act(
                        tools,
                        lease,
                        "hoi4_production_add",
                        json!({"equipment": archetype, "factories": take}),
                        "pk",
                    )?,
                };
                line.push_str(&format!("; {archetype} +{take}: {ok2} {result2}"));
            }
            state.skip(now, "mstockline", &archetype, 3 * DAY);
            note(
                state,
                "stockpile",
                format!("{equipment}: {why}; {take} factories moved from {have}: {ok}"),
            );
            return Ok(Some(format!(
                "stockpile {equipment} low ({why}), no idle factory: {line}"
            )));
        }
    }
    if give <= 0 {
        note(
            state,
            "stockpile",
            format!("{equipment}: {why}; no idle military factory to raise its line and no line to spare"),
        );
        if days < critical {
            wake(
                state,
                plan,
                now,
                11,
                "stockpile_low",
                format!("{equipment}: {why} (critical {critical:.0} days) and no idle military factory: move factories or cut its use"),
                json!({"equipment": equipment, "days_of_cover": days, "critical": critical}),
            );
        }
        return Ok(None);
    }
    let line = military
        .iter()
        .filter(|l| l.1.starts_with(archetype.as_str()))
        .max_by_key(|l| l.2)
        .cloned();
    let (ok, result) = match &line {
        Some((index, have, factories, _)) => state.act(
            tools,
            lease,
            "hoi4_production_set",
            json!({"line": index, "equipment": have, "factories": factories + give}),
            "pk",
        )?,
        None => state.act(
            tools,
            lease,
            "hoi4_production_add",
            json!({"equipment": archetype, "factories": give}),
            "pk",
        )?,
    };
    state.skip(
        now,
        "mstockline",
        &archetype,
        if ok { 3 * DAY } else { 7 * DAY },
    );
    let what = match &line {
        Some((_, have, factories, _)) => format!("{have} {factories}+{give}"),
        None => format!("new {archetype} line with {give}"),
    };
    note(
        state,
        "stockpile",
        format!("{equipment}: {why}: {what}: {ok}"),
    );
    Ok(Some(format!(
        "stockpile {equipment} low ({why}): {what}: {ok} {result}"
    )))
}

/// What the fielded divisions lack of an archetype (hoi4_stockpile `fielded_short`), 0 unread.
fn short_of(read: &Value, archetype: &str) -> f64 {
    list(read, "equipment")
        .iter()
        .find(|r| text(&r["archetype"]) == archetype)
        .and_then(|r| r["fielded_short"].as_f64())
        .unwrap_or(0.0)
}

/// The line that can spare factories for `needy`: its equipment is not short at the front, its
/// stock covers at least twice its target (`stockpile_targets`, else `warning` days) or is not
/// falling, and it keeps at least one factory. The line with the most factories gives at most
/// `step`. None when no line can spare any.
fn donor_line(
    read: &Value,
    plan: &Value,
    military: &[MilLine],
    needy: &str,
    warning: f64,
    step: i64,
) -> Option<(MilLine, i64)> {
    military
        .iter()
        .filter(|l| l.2 > 1 && !l.1.starts_with(needy))
        .filter(|l| {
            let Some(arche) = archetype_row(read, &l.1) else {
                return false;
            };
            if short_of(read, &arche) > 0.0 {
                return false;
            }
            let want = plan["stockpile_targets"][arche.as_str()]
                .as_f64()
                .or_else(|| plan["stockpile_targets"][l.1.as_str()].as_f64())
                .unwrap_or(warning);
            crate::plan_predicate::stockpile_days(read, &l.1).is_ok_and(|d| d >= 2.0 * want)
        })
        .max_by_key(|l| (l.2, std::cmp::Reverse(l.0)))
        .map(|l| (l.clone(), step.min(l.2 - 1)))
}

/// The archetype row a key names in a hoi4_stockpile read (itself, or the row holding the variant).
fn archetype_row(read: &Value, key: &str) -> Option<String> {
    list(read, "equipment")
        .iter()
        .find(|r| {
            text(&r["archetype"]) == key
                || list(r, "variants").iter().any(|v| text(&v["type"]) == key)
        })
        .map(|r| text(&r["archetype"]).to_string())
}

#[cfg(test)]
mod tests {
    use super::super::{date_hour, extras_step, tick};
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
    fn actions(f: &Fake) -> Vec<(String, Value)> {
        f.calls
            .borrow()
            .iter()
            .filter(|(_, a)| a.get("request_id").is_some())
            .cloned()
            .collect()
    }
    fn reads(f: &Fake, tool: &str) -> usize {
        f.calls.borrow().iter().filter(|(n, _)| n == tool).count()
    }
    fn wakes(state: &mut State) -> Vec<Value> {
        take_alarms(state)
    }

    #[test]
    fn when_is_a_plan_predicate_and_unreadable_atoms_leave_the_item_waiting() {
        let tools = fake(vec![(
            "hoi4_country_status",
            json!({"fields": {"war_support": {"value": 0.3}}}),
        )]);
        let mut state = State::default();
        let at = date_hour("1936.6.1").unwrap();
        for (when, now, want) in [
            (json!(null), at, Truth::True),
            (json!(""), at, Truth::True),
            (json!("date:1936.6.1"), at, Truth::True),
            (json!("date:1936.6.1"), at - 1, Truth::False),
            (json!("1936.06.01"), at + DAY, Truth::True),
            (json!("war_support >= 25"), at, Truth::True),
            (
                json!({"all": ["1936.6.1", "war_support > 0.5"]}),
                at,
                Truth::False,
            ),
            (json!("province_controlled:5097"), at, Truth::Unknown),
            (json!("bop:ETH_centralization > 0.5"), at, Truth::Unknown),
        ] {
            assert_eq!(when_holds(&mut state, &tools, &when, now), want, "{when}");
        }
        // one read per tick (the same game hour) for every predicate that needs it
        assert_eq!(reads(&tools, "hoi4_country_status"), 1);
        assert!(state
            .strategy
            .unknown
            .contains_key("bop:ETH_centralization > 0.5"));
    }

    #[test]
    fn alerts_nudge_their_step_and_wake_for_judgement_once() {
        let tools = fake(vec![(
            "hoi4_alerts",
            json!({"alerts": [
                {"frame": 26, "alert": "no_divisions_in_training", "todo": "hoi4_train"},
                {"frame": 57, "alert": "mio_traits"},
                {"frame": 99, "alert": null, "todo": "unlabelled"},
                {"frame": 53, "alert": "doctrines_available"},
                {"frame": 2, "alert": "at_war"}]}),
        )]);
        let plan = json!({"training": {"template": "Inf", "queue": 2}});
        let mut state = State::default();
        state.skip(START, "train", "Inf", 2 * DAY);
        state.skip(START, "trainee", "x", 2 * DAY);
        assert_eq!(step(&mut state, &tools, &plan, "l", START).unwrap(), None);
        assert!(
            !state.skipped(START, "train", "Inf"),
            "the training step is nudged"
        );
        assert!(
            state.skipped(START, "trainee", "x"),
            "only that kind is cleared"
        );
        let alarms = wakes(&mut state);
        let reasons: Vec<&str> = alarms.iter().filter_map(|a| a["reason"].as_str()).collect();
        assert_eq!(
            reasons,
            vec![
                "alert:mio_traits",
                "alert:frame_99",
                "alert:doctrines_available"
            ],
            "MIO needs judgement, an unknown frame wakes, doctrines has no plan key"
        );
        assert!(alarms
            .iter()
            .all(|a| a["code"] == "wake_player" && a["rule"] == 3));
        assert_eq!(alarms[1]["evidence"]["frame"], 99);
        // read once a game day; a wake is not repeated within wake_repeat_days
        step(&mut state, &tools, &plan, "l", START + 12).unwrap();
        assert_eq!(reads(&tools, "hoi4_alerts"), 1);
        step(&mut state, &tools, &plan, "l", START + DAY).unwrap();
        assert_eq!(reads(&tools, "hoi4_alerts"), 2);
        assert!(wakes(&mut state).is_empty());
        // switched off
        let quiet = json!({"manage": {"off": ["alerts"]}});
        step(&mut state, &tools, &quiet, "l", START + 3 * DAY).unwrap();
        assert_eq!(reads(&tools, "hoi4_alerts"), 2);
    }

    #[test]
    fn a_persisting_alert_wakes_after_alert_persist_days() {
        let tools = fake(vec![(
            "hoi4_alerts",
            json!({"alerts": [{"frame": 39, "alert": "unassigned_divisions"}]}),
        )]);
        let plan = json!({"armies": {"max": 24}, "manage": {"alert_persist_days": 3,
            "off": ["manpower", "stability", "equipment_fill", "resources", "stockpile", "trucks"]}});
        let mut state = State::default();
        for day in 0..3 {
            step(&mut state, &tools, &plan, "l", START + day * DAY).unwrap();
            assert!(wakes(&mut state).is_empty(), "day {day}");
        }
        step(&mut state, &tools, &plan, "l", START + 3 * DAY).unwrap();
        let alarms = wakes(&mut state);
        assert_eq!(alarms.len(), 1);
        assert_eq!(alarms[0]["reason"], "alert:unassigned_divisions");
    }

    #[test]
    fn the_ledger_wakes_for_an_order_refused_three_times() {
        let tools = fake(vec![]);
        tools
            .refuse
            .borrow_mut()
            .insert("hoi4_train".into(), "invalid_target");
        let mut state = State {
            clock: START,
            ..Default::default()
        };
        for _ in 0..4 {
            state
                .act(
                    &tools,
                    "l",
                    "hoi4_train",
                    json!({"template": "Inf", "count": 1}),
                    "tr",
                )
                .unwrap();
        }
        assert_eq!(
            actions(&tools).len(),
            3,
            "the fourth is refused by the ledger"
        );
        step(&mut state, &tools, &json!({}), "l", START).unwrap();
        let alarms = wakes(&mut state);
        assert_eq!(alarms.len(), 1);
        assert_eq!(alarms[0]["rule"], 35);
        assert!(alarms[0]["evidence"]["order"]
            .as_str()
            .unwrap()
            .starts_with("hoi4_train"));
    }

    fn templates_read(rows: &[(&str, u64)]) -> Value {
        json!({"templates": rows.iter().map(|(t, q)| json!({"template": t, "queued_lines": q})).collect::<Vec<_>>()})
    }

    #[test]
    fn template_cleanup_deletes_or_renames_unused_templates_and_never_one_the_plan_uses() {
        let plan = json!({
            "training": [{"template": "Inf", "queue": 1}],
            "template_cleanup": [
                {"template": "Inf"},
                {"template": "Queued"},
                {"template": "Old Cav"},
                {"template": "Militia", "rename_to": "Garrison"},
                {"template": "Gone"}]});
        let read = templates_read(&[("Inf", 1), ("Queued", 2), ("Old Cav", 0), ("Militia", 0)]);
        let tools = fake(vec![("hoi4_division_templates", read.clone())]);
        let mut state = State::default();
        let out = template_cleanup_step(&mut state, &tools, &plan, "l", START)
            .unwrap()
            .unwrap();
        assert!(out.starts_with("template Old Cav deleted: true"), "{out}");
        assert_eq!(actions(&tools)[0].0, "hoi4_template_delete");
        assert_eq!(actions(&tools)[0].1["template"], json!("Old Cav"));
        // the next entry goes on the next tick: the rename
        let tools = fake(vec![(
            "hoi4_division_templates",
            templates_read(&[("Inf", 1), ("Queued", 2), ("Militia", 0)]),
        )]);
        let out = template_cleanup_step(&mut state, &tools, &plan, "l", START + 1)
            .unwrap()
            .unwrap();
        assert!(
            out.starts_with("template Militia renamed to Garrison: true"),
            "{out}"
        );
        assert_eq!(actions(&tools)[0].1["new_name"], json!("Garrison"));
        // then nothing: Inf is trained by the plan, Queued has lines queued, Gone is gone
        let tools = fake(vec![(
            "hoi4_division_templates",
            templates_read(&[("Inf", 1), ("Queued", 2), ("Garrison", 0)]),
        )]);
        assert_eq!(
            template_cleanup_step(&mut state, &tools, &plan, "l", START + 2).unwrap(),
            None
        );
        assert!(actions(&tools).is_empty());
        let line = digest(&state).unwrap()["template_cleanup"]
            .as_str()
            .unwrap()
            .to_string();
        assert!(
            line.contains("Inf: the plan still trains or edits it"),
            "{line}"
        );
        assert!(line.contains("Queued: 2 training line(s) queued"), "{line}");
        assert!(state.done.contains_key("tclean:4:Gone"));
    }

    #[test]
    fn a_template_the_game_finds_in_use_waits_thirty_days() {
        let plan = json!({"template_cleanup": [{"id": "old", "template": "Old"}]});
        let tools = fake(vec![(
            "hoi4_division_templates",
            templates_read(&[("Old", 0)]),
        )]);
        tools
            .refuse
            .borrow_mut()
            .insert("hoi4_template_delete".into(), "template_in_use");
        let mut state = State::default();
        let out = template_cleanup_step(&mut state, &tools, &plan, "l", START)
            .unwrap()
            .unwrap();
        assert!(out.starts_with("template Old deleted: false"), "{out}");
        assert!(state.skipped(START + 29 * DAY, "tclean", "tclean:id:old"));
        assert!(!state.skipped(START + 31 * DAY, "tclean", "tclean:id:old"));
        assert!(digest(&state).unwrap()["template_cleanup"]
            .as_str()
            .unwrap()
            .contains("Old deleted: false"));
    }

    #[test]
    fn the_single_training_object_still_trains_through_the_tick() {
        let tools = fake(vec![(
            "hoi4_division_templates",
            templates_read(&[("Inf", 1)]),
        )]);
        let plan =
            json!({"training": {"template": "Inf", "queue": 4, "batch": 2, "deploy_state": 5}});
        let mut state = State::default();
        let step = tick(&mut state, &tools, &plan, "l", START).unwrap();
        assert_eq!(step, Tick::Acted("train 2 Inf: true Verified".into()));
        let sent = actions(&tools);
        assert_eq!(sent[0].1["deploy_state_id"], 5);
    }

    #[test]
    fn a_training_list_trains_the_first_template_short_of_its_queue() {
        let tools = fake(vec![(
            "hoi4_division_templates",
            templates_read(&[("Inf", 4), ("Mtn", 0)]),
        )]);
        let plan = json!({"training": [
            {"template": "Inf", "queue": 4, "deploy_state": 5},
            {"template": "Later", "queue": 4, "deploy_state": 5, "when": "date:2000.1.1"},
            {"template": "Mtn", "queue": 3, "batch": 5, "deploy_state": 6}]});
        let mut state = State::default();
        let out = training_step(&mut state, &tools, &plan, "l", START).unwrap();
        assert_eq!(out.as_deref(), Some("train 3 Mtn: true Verified"));
        // a refusal is not repeated for 2 game days
        tools
            .refuse
            .borrow_mut()
            .insert("hoi4_train".into(), "invalid_target");
        let out = training_step(&mut state, &tools, &plan, "l", START).unwrap();
        assert!(out.unwrap().contains(": false"));
        assert_eq!(
            training_step(&mut state, &tools, &plan, "l", START + DAY).unwrap(),
            None
        );
        assert_eq!(actions(&tools).len(), 2);
    }

    fn division(template: &str, have: u64, need: u64) -> Value {
        json!({"template": template, "army": "Army 1", "equipment_fill_min": have as f64 / need as f64,
            "equipment": [{"archetype": "infantry_equipment", "have": have, "need": need}],
            "short_equipment": if have < need { json!(["infantry_equipment"]) } else { json!([]) }})
    }

    #[test]
    fn training_rules_cap_the_template_and_guard_the_stockpile() {
        let mut fielded: Vec<Value> = (0..19).map(|_| division("Inf", 100, 100)).collect();
        let tools = fake(vec![
            ("hoi4_division_templates", templates_read(&[("Inf", 0)])),
            ("hoi4_supply", json!({"divisions": fielded})),
        ]);
        let plan = json!({"training": {"template": "Inf", "queue": 4, "batch": 3, "deploy_state": 5},
            "training_rules": {"cap_template": "Inf", "cap": 20, "only_if_stockpile": {"infantry_equipment": 600}}});
        let mut state = State::default();
        let out = training_step(&mut state, &tools, &plan, "l", START).unwrap();
        assert_eq!(
            out.as_deref(),
            Some("train 1 Inf: true Verified"),
            "19 fielded + 1 = cap 20"
        );
        assert!(state.manage.notes["training"].contains("unknown (no stockpile read)"));
        // at the cap: nothing
        fielded.push(division("Inf", 100, 100));
        tools
            .replies
            .borrow_mut()
            .insert("hoi4_supply".into(), json!({"divisions": fielded}));
        let mut state = State::default();
        assert_eq!(
            training_step(&mut state, &tools, &plan, "l", START).unwrap(),
            None
        );
        assert!(state.manage.notes["training"].contains("at its cap"));
        // a fielded division short of rifles: the stockpile guard waits
        let short = vec![division("Inf", 50, 100)];
        tools
            .replies
            .borrow_mut()
            .insert("hoi4_supply".into(), json!({"divisions": short}));
        let mut state = State::default();
        assert_eq!(
            training_step(&mut state, &tools, &plan, "l", START).unwrap(),
            None
        );
        assert!(state.manage.notes["training"].contains("lack infantry_equipment"));
        assert_eq!(actions(&tools).len(), 1);
        // the stockpile read decides when present: 400 rifles held, 600 wanted
        tools.replies.borrow_mut().insert(
            "hoi4_stockpile".into(),
            json!({"equipment": [{"archetype": "infantry_equipment", "stockpile": 400.0, "variants": []}]}),
        );
        let mut state = State::default();
        assert_eq!(
            training_step(&mut state, &tools, &plan, "l", START).unwrap(),
            None
        );
        assert!(state.manage.notes["training"].contains("stockpile 400 below 600"));
        tools.replies.borrow_mut().insert(
            "hoi4_stockpile".into(),
            json!({"equipment": [{"archetype": "infantry_equipment", "stockpile": 900.0, "variants": []}]}),
        );
        let mut state = State::default();
        training_step(&mut state, &tools, &plan, "l", START)
            .unwrap()
            .unwrap();
        assert!(state.manage.notes["training"].contains("infantry_equipment 900 >= 600"));
        assert_eq!(actions(&tools).len(), 2);
    }

    #[test]
    fn at_war_strike_templates_go_elite_and_garrisons_reserve_and_peace_restores_them() {
        let tools = fake(vec![(
            "hoi4_wars",
            json!({"wars": {"validated": true, "at_war": true, "enemies": ["ETH"]}}),
        )]);
        let plan = json!({"war": {"enemies": ["ETH"]}, "templates": [
            {"id": "a", "name": "Assalto", "role": "strike", "priority": "regular"},
            {"id": "g", "name": "Presidio", "role": "garrison"},
            {"id": "l", "name": "Linea", "role": "line"}]});
        let mut state = State::default();
        for id in ["a", "g", "l"] {
            state.done.insert(format!("tpl:id:{id}"), START);
        }
        let out = templates_step(&mut state, &tools, &plan, "l", START).unwrap();
        assert_eq!(
            out.as_deref(),
            Some("template Assalto reinforcement priority elite: true Verified")
        );
        let out = templates_step(&mut state, &tools, &plan, "l", START).unwrap();
        assert_eq!(
            out.as_deref(),
            Some("template Presidio reinforcement priority reserve: true Verified")
        );
        assert_eq!(
            templates_step(&mut state, &tools, &plan, "l", START).unwrap(),
            None
        );
        assert_eq!(actions(&tools).len(), 2);
        // at peace the plan's own priority returns (regular when it names none)
        tools.replies.borrow_mut().insert(
            "hoi4_wars".into(),
            json!({"wars": {"validated": true, "at_war": false, "enemies": []}}),
        );
        let later = START + 2 * DAY;
        let out = templates_step(&mut state, &tools, &plan, "l", later).unwrap();
        assert_eq!(
            out.as_deref(),
            Some("template Assalto reinforcement priority regular: true Verified")
        );
        let out = templates_step(&mut state, &tools, &plan, "l", later).unwrap();
        assert_eq!(
            out.as_deref(),
            Some("template Presidio reinforcement priority regular: true Verified")
        );
        assert!(!state.done.contains_key("warprio:Assalto"));
        // switched off
        let off = json!({"war": {"enemies": ["ETH"]}, "manage": {"war_priority": false},
            "templates": [{"id": "a", "name": "Assalto", "role": "strike"}]});
        let mut state = State::default();
        state.done.insert("tpl:id:a".into(), START);
        assert_eq!(
            templates_step(&mut state, &tools, &off, "l", START).unwrap(),
            None
        );
    }

    #[test]
    fn templates_are_created_once_with_the_xp_guard_and_persist() {
        let tools = fake(vec![
            (
                "hoi4_division_templates",
                templates_read(&[("Infantry Division", 0)]),
            ),
            (
                "hoi4_doctrines",
                json!({"experience": {"army": {"value": 30.0}}}),
            ),
        ]);
        let plan = json!({"templates": [
            {"id": "mtn", "name": "Alpini", "regiments": ["mountaineers", "mountaineers", "mountaineers",
                "mountaineers", "mountaineers", "mountaineers", "artillery_brigade"],
                "support": ["engineer"], "priority": "elite", "regimental_support": ["x"]}]});
        let mut state = State::default();
        // 7 battalions * 5 + 1 company * 10 + 2 extra columns * 25 = 95 > 30: waits
        assert_eq!(step(&mut state, &tools, &plan, "l", START).unwrap(), None);
        assert!(actions(&tools).is_empty());
        assert!(state.manage.notes["templates"].contains("army XP 30 < 95"));
        tools.replies.borrow_mut().insert(
            "hoi4_doctrines".into(),
            json!({"experience": {"army": {"value": 120.0}}}),
        );
        let out = step(&mut state, &tools, &plan, "l", START + DAY).unwrap();
        assert_eq!(
            out.as_deref(),
            Some("template create Alpini: true Verified")
        );
        let sent = actions(&tools);
        assert_eq!(sent[0].0, "hoi4_template_create");
        assert_eq!(
            sent[0].1["regiments"],
            json!([
                [
                    "mountaineers",
                    "mountaineers",
                    "mountaineers",
                    "mountaineers",
                    "mountaineers"
                ],
                ["mountaineers"],
                ["artillery_brigade"]
            ])
        );
        assert_eq!(sent[0].1["priority"], "elite");
        let line = &state.manage.notes["templates"];
        assert!(
            line.contains("regimental support row skipped")
                && line.contains("templates.regimental_support"),
            "{line}"
        );
        assert!(sent[0].1.get("regimental_support").is_none());
        // once per id, and the done-memory survives a restart through plan_state.json
        assert_eq!(
            step(&mut state, &tools, &plan, "l", START + 9 * DAY).unwrap(),
            None
        );
        let memory = state.memory(START + 9 * DAY);
        assert!(memory["done"]["tpl:id:mtn"].is_i64());
        let mut restarted = State::default();
        restarted.restore(&memory, START + 9 * DAY);
        assert_eq!(
            step(&mut restarted, &tools, &plan, "l", START + 9 * DAY).unwrap(),
            None
        );
        assert_eq!(actions(&tools).len(), 1);
    }

    #[test]
    fn a_template_with_a_base_is_edited_and_renamed_and_refusals_wait() {
        let tools = fake(vec![(
            "hoi4_division_templates",
            templates_read(&[("Infantry Division", 2)]),
        )]);
        tools
            .refuse
            .borrow_mut()
            .insert("hoi4_template_edit".into(), "template_in_use");
        let plan = json!({"templates": [{"id": "inf2", "name": "Fanteria", "base": "Infantry Division",
            "support": ["engineer", "recon"], "upgrade_fielded": true}]});
        let mut state = State::default();
        let out = step(&mut state, &tools, &plan, "l", START)
            .unwrap()
            .unwrap();
        assert!(
            out.starts_with("template edit Fanteria: false template_in_use"),
            "{out}"
        );
        let sent = actions(&tools);
        assert_eq!(sent[0].1["template"], "Infantry Division");
        assert_eq!(sent[0].1["new_name"], "Fanteria");
        assert_eq!(sent[0].1["allow_in_use"], true);
        // not repeated for 7 game days
        assert_eq!(
            step(&mut state, &tools, &plan, "l", START + 2 * DAY).unwrap(),
            None
        );
        assert_eq!(actions(&tools).len(), 1);
        tools.refuse.borrow_mut().clear();
        let out = step(&mut state, &tools, &plan, "l", START + 8 * DAY).unwrap();
        assert_eq!(
            out.as_deref(),
            Some("template edit Fanteria: true Verified")
        );
        assert!(state.done.contains_key("tpl:id:inf2"));
    }

    fn occupation_read(law: &str, states: &[(i64, f64, f64)]) -> Value {
        // the laws as hoi4_occupation lists them from the game files: list order, shown or not
        json!({"player_occupation_law": {"key": law},
            "laws": [
                {"key": "no_garrison", "shown": true, "resistance_target": 0.4},
                {"key": "democratic_only", "shown": false, "resistance_target": 0.1},
                {"key": "foreign_civilian_oversight", "shown": true},
                {"key": "local_police_force_garrison", "shown": true, "resistance_target": -0.15},
                {"key": "secret_police_oversight", "shown": true, "resistance_target": -0.3},
                {"key": "colonial_police", "shown": null, "resistance_target": -0.3},
                {"key": "martial_law_occupation", "shown": true, "resistance_target": -0.6},
                {"key": "forced_labor_occupation", "shown": true, "resistance_target": -0.4}],
            "states": states.iter().map(|(id, r, c)| json!({"state_id": id, "role": "occupied_by_player",
                "resistance": r, "compliance": c})).collect::<Vec<_>>()})
    }

    #[test]
    fn the_default_occupation_ladder_is_the_shown_laws_by_their_resistance_target() {
        let ladder = occupation_ladder(&occupation_read("x", &[]));
        assert_eq!(
            ladder,
            [
                "no_garrison",
                "foreign_civilian_oversight",
                "local_police_force_garrison",
                "secret_police_oversight",
                "forced_labor_occupation",
                "martial_law_occupation"
            ]
        );
        // no law list (an older server): no ladder, the step only notes it
        let tools = fake(vec![(
            "hoi4_occupation",
            json!({"player_occupation_law": {"key": "a"},
                "states": [{"state_id": 1, "role": "occupied_by_player", "resistance": 80.0}]}),
        )]);
        let plan = json!({"occupation": {}, "manage": {"off": ["manpower", "stability", "equipment_fill", "resources"]}});
        let mut state = State::default();
        assert_eq!(step(&mut state, &tools, &plan, "l", START).unwrap(), None);
        assert!(state.manage.notes["occupation"].contains("no ladder"));
        assert!(actions(&tools).is_empty());
    }

    #[test]
    fn occupation_sets_the_default_law_then_follows_resistance() {
        let tools = fake(vec![(
            "hoi4_occupation",
            occupation_read(
                "foreign_civilian_oversight",
                &[(271, 10.0, 5.0), (272, 20.0, 5.0)],
            ),
        )]);
        let plan = json!({"occupation": {"default_law": "local_police_force_garrison", "per_state": {"271": "x"}},
            "manage": {"off": ["manpower", "stability", "equipment_fill", "resources"]}});
        let mut state = State::default();
        let out = step(&mut state, &tools, &plan, "l", START).unwrap();
        assert_eq!(
            out.as_deref(),
            Some("occupation default law local_police_force_garrison: true Verified")
        );
        assert_eq!(
            actions(&tools)[0].1["via_state"],
            272,
            "via the most resisting state"
        );
        let line = &state.manage.notes["occupation"];
        assert!(
            line.contains("per_state not applied") && line.contains("occupation.per_state_law"),
            "{line}"
        );
        assert!(actions(&tools)
            .iter()
            .all(|(_, a)| a.get("state").is_none()));
        // checked every 7 game days; resistance 60 moves one step harsher after change_days
        tools.replies.borrow_mut().insert(
            "hoi4_occupation".into(),
            occupation_read("local_police_force_garrison", &[(271, 60.0, 5.0)]),
        );
        assert_eq!(
            step(&mut state, &tools, &plan, "l", START + 3 * DAY).unwrap(),
            None
        );
        assert_eq!(
            step(&mut state, &tools, &plan, "l", START + 7 * DAY).unwrap(),
            None
        );
        assert!(state.manage.notes["occupation"].contains("waits for change_days"));
        let out = step(&mut state, &tools, &plan, "l", START + 35 * DAY).unwrap();
        assert_eq!(
            out.as_deref(),
            Some("occupation law local_police_force_garrison -> secret_police_oversight (resistance max 60): true Verified")
        );
        // calm everywhere: one step milder
        tools.replies.borrow_mut().insert(
            "hoi4_occupation".into(),
            occupation_read("secret_police_oversight", &[(271, 10.0, 45.0)]),
        );
        let out = step(&mut state, &tools, &plan, "l", START + 70 * DAY).unwrap();
        assert!(out.unwrap().contains("-> local_police_force_garrison"));
        assert_eq!(actions(&tools).len(), 3);
    }

    fn peace_read(end_turn: bool, done: bool, points: f64) -> Value {
        json!({"peace_conference_open": true, "own_remaining_points": points,
            "controls": {"end_turn_button": {"enabled": end_turn}, "done_button": {"enabled": done}},
            "states": [{"state_id": 300, "own": true}]})
    }

    #[test]
    fn a_country_tag_in_the_peace_lists_stands_for_its_states() {
        let tools = fake(vec![
            ("hoi4_events", json!({"peace_conference_open": true})),
            ("hoi4_peace_options", peace_read(true, true, 50.0)),
            (
                "hoi4_states",
                json!({"states": [{"state_id": 271, "owner": "ETH"}, {"state_id": 274, "owner": "ETH"},
                                  {"state_id": 550, "owner": "ITA", "controller": "ETH"}]}),
            ),
        ]);
        let plan =
            json!({"peace": {"take": [271], "puppet": ["ETH"], "force_government": ["ETH"]}});
        let mut state = State::default();
        tick(&mut state, &tools, &plan, "l", START).unwrap();
        // the force-government demand waits on its tool argument and says so
        let line = &state.manage.notes["peace"];
        assert!(
            line.contains("force_government ETH") && line.contains("peace.force_government"),
            "{line}"
        );
        assert_eq!(
            tick(&mut state, &tools, &plan, "l", START).unwrap(),
            Tick::Acted("peace puppet state 274: true Verified".into())
        );
        let sent = actions(&tools);
        assert_eq!(sent[0].1["states"], json!([271]));
        assert!(sent[0].1.get("puppet").is_none());
        assert_eq!(sent[1].1["states"], json!([274]));
        assert_eq!(sent[1].1["puppet"], true);
        // a state ETH controls but does not own is not ETH's to give
        assert!(!sent.iter().any(|a| a.1["states"] == json!([550])));
        assert!(!sent.iter().any(|a| a.1.get("force_government").is_some()));
    }

    #[test]
    fn the_peace_step_takes_by_rank_submits_and_exits() {
        let tools = fake(vec![
            ("hoi4_events", json!({"peace_conference_open": true})),
            ("hoi4_peace_options", peace_read(true, true, 50.0)),
        ]);
        let plan = json!({"peace": {"take": [271, 300, 272], "puppet": [273, "ETH"]}});
        let mut state = State::default();
        let first = tick(&mut state, &tools, &plan, "l", START).unwrap();
        assert_eq!(
            first,
            Tick::Acted("peace take state 271: true Verified".into())
        );
        // 300 is already ours; 272 is too dear this round
        tools
            .refuse
            .borrow_mut()
            .insert("hoi4_peace_take".into(), "cost_exceeds_points");
        let second = tick(&mut state, &tools, &plan, "l", START).unwrap();
        assert!(matches!(&second, Tick::Acted(s) if s.starts_with("peace take state 272: false")));
        tools.refuse.borrow_mut().clear();
        let third = tick(&mut state, &tools, &plan, "l", START).unwrap();
        assert_eq!(third, Tick::Acted("peace submit: true Verified".into()));
        let fourth = tick(&mut state, &tools, &plan, "l", START).unwrap();
        assert_eq!(
            fourth,
            Tick::Acted("peace take state 272: true Verified".into())
        );
        let fifth = tick(&mut state, &tools, &plan, "l", START).unwrap();
        assert_eq!(
            fifth,
            Tick::Acted("peace puppet state 273: true Verified".into())
        );
        let sixth = tick(&mut state, &tools, &plan, "l", START).unwrap();
        assert_eq!(
            sixth,
            Tick::Acted("peace exit (took 3 of 4): true Verified".into())
        );
        assert_eq!(
            tick(&mut state, &tools, &plan, "l", START).unwrap(),
            Tick::Nothing
        );
        let sent: Vec<String> = actions(&tools).into_iter().map(|a| a.0).collect();
        assert_eq!(sent.iter().filter(|t| *t == "hoi4_peace_take").count(), 4);
        assert_eq!(actions(&tools)[4].1["puppet"], true);
        assert!(
            state.manage.notes["peace"].contains("puppet ETH"),
            "{}",
            state.manage.notes["peace"]
        );
        // the conference is over: the next tick that reaches the manage step forgets it
        tools
            .replies
            .borrow_mut()
            .insert("hoi4_events".into(), json!({}));
        step(&mut state, &tools, &plan, "l", START + DAY).unwrap();
        assert!(state.manage.peace.is_none());
    }

    #[test]
    fn without_a_peace_plan_the_runner_stops_and_wakes_the_player() {
        let tools = fake(vec![(
            "hoi4_events",
            json!({"peace_conference_open": true}),
        )]);
        let mut state = State::default();
        let out = tick(&mut state, &tools, &json!({}), "l", START).unwrap();
        assert_eq!(out, Tick::Stop("peace conference open".into()));
        let alarms = wakes(&mut state);
        assert_eq!(alarms[0]["reason"], "peace_conference_open");
        assert_eq!(alarms[0]["rule"], 25);
        assert!(actions(&tools).is_empty());
    }

    #[test]
    fn a_peace_order_the_ledger_refuses_stops_and_wakes() {
        let tools = fake(vec![("hoi4_peace_options", peace_read(false, true, 0.0))]);
        tools
            .refuse
            .borrow_mut()
            .insert("hoi4_peace_exit".into(), "exit_not_available");
        // 300 is already ours: nothing is left to take, so the runner exits
        let plan = json!({"peace": {"take": [300]}});
        let mut state = State {
            clock: START,
            ..Default::default()
        };
        for _ in 0..3 {
            let out = peace(&mut state, &tools, &plan, "l", START).unwrap();
            assert!(matches!(out, Tick::Acted(s) if s.contains(": false")));
        }
        let out = peace(&mut state, &tools, &plan, "l", START).unwrap();
        assert!(matches!(out, Tick::Stop(_)), "{out:?}");
        assert_eq!(actions(&tools).len(), 3);
        assert_eq!(wakes(&mut state)[0]["reason"], "peace_refused");
    }

    #[test]
    fn the_peace_step_waits_for_the_other_victors_then_exits_or_wakes() {
        let tools = fake(vec![("hoi4_peace_options", peace_read(false, true, 10.0))]);
        let plan = json!({"peace": {"take": [271]}});
        let mut state = State::default();
        for _ in 0..PEACE_PATIENCE {
            assert_eq!(
                peace(&mut state, &tools, &plan, "l", START).unwrap(),
                Tick::Nothing
            );
        }
        let out = peace(&mut state, &tools, &plan, "l", START).unwrap();
        assert_eq!(
            out,
            Tick::Acted("peace exit (took 0 of 1): true Verified".into())
        );
        // no usable control at all: the player is woken after 4 x patience ticks
        let tools = fake(vec![("hoi4_peace_options", peace_read(false, false, 10.0))]);
        let mut state = State::default();
        let mut stopped = None;
        for n in 0..4 * PEACE_PATIENCE {
            match peace(&mut state, &tools, &plan, "l", START).unwrap() {
                Tick::Nothing => {}
                other => {
                    stopped = Some((n, other));
                    break;
                }
            }
        }
        assert!(matches!(stopped, Some((n, Tick::Stop(_))) if n == 4 * PEACE_PATIENCE - 1));
        assert_eq!(wakes(&mut state)[0]["reason"], "peace_stuck");
        assert!(actions(&tools).is_empty());
    }

    #[test]
    fn a_deficit_turns_auto_import_on_once_and_wakes_when_it_persists() {
        let tools = fake(vec![(
            "hoi4_production_shortages",
            json!({"lines": [{"equipment": "infantry_equipment_1", "short_of": ["steel"]},
                {"equipment": "convoy_1", "short_of": ["chromium"]}]}),
        )]);
        let plan =
            json!({"auto_import": {"budget": 5}, "production_priority": ["infantry_equipment"]});
        let mut state = State::default();
        let out = step(&mut state, &tools, &plan, "l", START).unwrap();
        assert_eq!(
            out.as_deref(),
            Some("auto_import on (budget 5) for steel: true Verified")
        );
        assert_eq!(actions(&tools)[0].1["budget"], 5);
        assert_eq!(actions(&tools)[0].1["enabled"], true);
        assert!(state.done.contains_key("imp:on:5"));
        // weekly; not sent again; a deficit outlasting 14 days wakes the player
        assert_eq!(
            step(&mut state, &tools, &plan, "l", START + 7 * DAY).unwrap(),
            None
        );
        assert!(wakes(&mut state).is_empty());
        assert_eq!(
            step(&mut state, &tools, &plan, "l", START + 14 * DAY).unwrap(),
            None
        );
        assert_eq!(wakes(&mut state)[0]["reason"], "resource_deficit");
        assert_eq!(actions(&tools).len(), 1);
        // a plan without auto_import only wakes
        let mut state = State::default();
        let plan = json!({"manage": {"off": ["manpower", "stability", "equipment_fill"]}});
        assert_eq!(step(&mut state, &tools, &plan, "l", START).unwrap(), None);
        let alarms = wakes(&mut state);
        assert_eq!(alarms[0]["rule"], 16);
        assert_eq!(
            alarms[0]["evidence"]["short_of"],
            json!(["chromium", "steel"])
        );
        assert_eq!(actions(&tools).len(), 1);
    }

    #[test]
    fn a_country_wide_shortage_no_line_draws_counts_and_a_lasting_one_raises_the_import_budget() {
        // n1, 16 Nov 2026: coal short (energy) 205 days, the step saw "no deficit on a priority line"
        let tools = fake(vec![
            (
                "hoi4_production_shortages",
                json!({"lines": [{"equipment": "infantry_equipment_1", "short_of": [],
                    "resources": [{"resource": "steel"}]}]}),
            ),
            (
                "hoi4_resources",
                json!({"resources": {"coal": {"shortage": {"short": true, "deficit": 5.0}},
                    "steel": {"shortage": {"short": false, "deficit": 0.0}}}}),
            ),
            (
                "hoi4_auto_import",
                json!({"auto_import": {"enabled": true, "budget_factories": 3, "available_factories": 6}}),
            ),
        ]);
        let plan = json!({"auto_import": {"budget": 3}, "production_priority": ["infantry_equipment"],
            "manage": {"off": ["manpower", "stability", "equipment_fill", "stockpile", "trucks"]}});
        let mut state = State::default();
        let out = step(&mut state, &tools, &plan, "l", START).unwrap();
        assert_eq!(
            out.as_deref(),
            Some("auto_import on (budget 3) for coal: true Verified")
        );
        step(&mut state, &tools, &plan, "l", START + 7 * DAY).unwrap();
        let out = step(&mut state, &tools, &plan, "l", START + 14 * DAY).unwrap();
        assert_eq!(
            out.as_deref(),
            Some("auto_import budget 3 -> 4 for coal: true Verified")
        );
        let last = actions(&tools).pop().unwrap().1;
        assert_eq!(
            (last["budget"].clone(), last["enabled"].clone()),
            (json!(4), json!(true))
        );
        // the cap (budget + 2 = 5) and the 14-day spacing hold
        set_reply(
            &tools,
            "hoi4_auto_import",
            json!({"auto_import": {"enabled": true, "budget_factories": 5, "available_factories": 6}}),
        );
        let out = step(&mut state, &tools, &plan, "l", START + 28 * DAY).unwrap();
        assert_eq!(out, None);
        assert_eq!(
            wakes(&mut state).last().unwrap()["reason"],
            "resource_deficit"
        );
    }

    #[test]
    fn a_lasting_missing_production_alert_says_whether_an_army_gap_or_an_unread_request_holds_it() {
        let tools = fake(vec![
            (
                "hoi4_alerts",
                json!({"alerts": [{"frame": 8, "alert": "missing_equipment_production"}]}),
            ),
            (
                "hoi4_stockpile",
                json!({"equipment": [{"archetype": "artillery_equipment", "fielded_short": 64.0,
                    "production_lines": 1, "stockpile": 40.0}]}),
            ),
        ]);
        let plan = json!({"production": [{"equipment": "artillery_equipment_1", "factories": 3}],
            "manage": {"alert_persist_days": 3,
            "off": ["manpower", "stability", "equipment_fill", "resources", "stockpile", "trucks"]}});
        let mut state = State::default();
        for day in 0..3 {
            step(&mut state, &tools, &plan, "l", START + day * DAY).unwrap();
        }
        assert!(wakes(&mut state).is_empty());
        step(&mut state, &tools, &plan, "l", START + 3 * DAY).unwrap();
        let alarms = wakes(&mut state);
        assert_eq!(alarms.len(), 1);
        let text = alarms[0]["message"]
            .as_str()
            .unwrap_or_default()
            .to_string()
            + alarms[0]["why"].as_str().unwrap_or_default();
        assert!(
            text.contains("garrison or air reinforcement"),
            "{}",
            alarms[0]
        );
        // not again within 60 days
        for day in 4..40 {
            step(&mut state, &tools, &plan, "l", START + day * DAY).unwrap();
        }
        assert!(wakes(&mut state).is_empty());
        // an army gap without a line is named
        let read = json!({"equipment": [{"archetype": "anti_tank_equipment", "fielded_short": 12.0,
            "production_lines": 0}]});
        assert_eq!(
            unproduced_army_gaps(&read),
            vec!["anti_tank_equipment (short 12)"]
        );
    }

    fn set_reply(tools: &Fake, tool: &str, v: Value) {
        tools.replies.borrow_mut().insert(tool.to_string(), v);
    }

    #[test]
    fn short_convoys_take_idle_dockyards_onto_the_convoy_line_or_a_new_one() {
        let options = json!({
            "designs": [
                {"design_id": 7, "equipment": "convoy_1", "convoy": true},
                {"design_id": 31, "equipment": "convoy_2", "convoy": true},
                {"design_id": 40, "equipment": "ship_hull_light_2", "convoy": false}],
            "naval_lines": [
                {"line_index": 3, "equipment": "ship_hull_light_2", "convoy": false, "requested_factories": 6},
                {"line_index": 5, "equipment": "convoy_1", "convoy": true, "requested_factories": 2}]});
        let tools = fake(vec![
            (
                "hoi4_convoys",
                json!({"convoys": {"total": 200, "in_use": 195, "free": 5, "needed": 150}}),
            ),
            ("hoi4_naval_production_options", options.clone()),
            (
                "hoi4_economy",
                json!({"factory_trigger_count": {"naval": 12}}),
            ),
        ]);
        let plan = json!({"manage": {"convoys_free": 0.1}});
        let mut state = State::default();
        let out = convoy_rule(&mut state, &tools, &plan, "l", START).unwrap();
        assert!(out.unwrap().contains("convoy_1 line 2+3"));
        let acts = actions(&tools);
        assert_eq!(acts[0].0, "hoi4_production_set");
        assert_eq!(acts[0].1["line"], 5);
        assert_eq!(acts[0].1["equipment"], "convoy_1");
        assert_eq!(acts[0].1["factories"], 5); // 12 dockyards - 8 on lines = 4 idle, step 3
                                               // every 3 game days
        assert_eq!(
            convoy_rule(&mut state, &tools, &plan, "l", START + DAY).unwrap(),
            None
        );
        // no convoy line: a new line of the newest convoy design, never a warship
        let mut no_line = options.clone();
        no_line["naval_lines"] = json!([{"line_index": 3, "equipment": "ship_hull_light_2",
            "convoy": false, "requested_factories": 10}]);
        set_reply(&tools, "hoi4_naval_production_options", no_line);
        let mut state = State::default();
        convoy_rule(&mut state, &tools, &plan, "l", START).unwrap();
        let acts = actions(&tools);
        assert_eq!(acts[1].0, "hoi4_production_add");
        assert_eq!(acts[1].1["equipment"], "convoy_2");
        assert_eq!(acts[1].1["design"], 31);
        assert_eq!(acts[1].1["factories"], 2);
        // every dockyard on a line: the player decides which line gives way
        let mut full = options.clone();
        full["naval_lines"][0]["requested_factories"] = json!(10);
        set_reply(&tools, "hoi4_naval_production_options", full);
        let mut state = State::default();
        assert_eq!(
            convoy_rule(&mut state, &tools, &plan, "l", START).unwrap(),
            None
        );
        let alarms = wakes(&mut state);
        assert_eq!(alarms[0]["reason"], "convoys_low");
        assert_eq!(alarms[0]["rule"], 20);
        assert_eq!(actions(&tools).len(), 2);
        assert!(state.manage.notes["convoys"].contains("no idle dockyard"));
        // enough free convoys: nothing, with the reason in the digest
        set_reply(
            &tools,
            "hoi4_convoys",
            json!({"convoys": {"total": 200, "in_use": 100, "free": 100, "needed": 100}}),
        );
        let mut state = State::default();
        assert_eq!(
            convoy_rule(&mut state, &tools, &plan, "l", START).unwrap(),
            None
        );
        assert!(state.manage.notes["convoys"].starts_with("100 free of 200"));
        // switched off
        let mut state = State::default();
        let plan = json!({"manage": {"off": ["convoys"]}});
        assert_eq!(
            convoy_rule(&mut state, &tools, &plan, "l", START).unwrap(),
            None
        );
        assert_eq!(reads(&tools, "hoi4_convoys"), 4);
    }

    #[test]
    fn a_need_above_the_stock_is_a_shortage_even_with_free_convoys() {
        let tools = fake(vec![
            (
                "hoi4_convoys",
                json!({"convoys": {"total": 50, "in_use": 20, "free": 30, "needed": 80}}),
            ),
            (
                "hoi4_naval_production_options",
                json!({"designs": [{"design_id": 2, "equipment": "convoy_1", "convoy": true}],
                    "naval_lines": []}),
            ),
            (
                "hoi4_economy",
                json!({"factory_trigger_count": {"naval": 5}}),
            ),
        ]);
        let mut state = State::default();
        let plan = json!({"manage": {}});
        assert!(convoy_rule(&mut state, &tools, &plan, "l", START)
            .unwrap()
            .is_some());
        assert_eq!(actions(&tools)[0].1["factories"], 3);
    }

    #[test]
    fn dockyards_no_line_uses_build_convoys_unless_the_plan_keeps_them() {
        let replies = || {
            vec![
                (
                    "hoi4_convoys",
                    json!({"convoys": {"total": 50, "in_use": 10, "free": 40, "needed": 10}}),
                ),
                (
                    "hoi4_naval_production_options",
                    json!({"designs": [{"design_id": 2, "equipment": "convoy_1", "convoy": true}],
                        "naval_lines": []}),
                ),
                (
                    "hoi4_economy",
                    json!({"factory_trigger_count": {"naval": 5}}),
                ),
            ]
        };
        let tools = fake(replies());
        let mut state = State::default();
        let plan = json!({"manage": {}});
        assert!(convoy_rule(&mut state, &tools, &plan, "l", START)
            .unwrap()
            .is_some());
        assert_eq!(actions(&tools)[0].1["equipment"], "convoy_1");
        assert!(wakes(&mut state).is_empty());
        let tools = fake(replies());
        let mut state = State::default();
        let keep = json!({"manage": {"idle_dockyards": "keep"}});
        assert!(convoy_rule(&mut state, &tools, &keep, "l", START)
            .unwrap()
            .is_none());
        assert!(actions(&tools).is_empty());
    }

    #[test]
    fn low_fuel_turns_auto_import_on_once_and_wakes_when_it_stays_low() {
        let tools = fake(vec![(
            "hoi4_resources",
            json!({"fuel": {"stock": 1000.0, "capacity": 10000.0, "ratio": 0.1}}),
        )]);
        let plan = json!({"manage": {}, "auto_import": {"budget": 4}});
        let mut state = State::default();
        let out = fuel_rule(&mut state, &tools, &plan, "l", START).unwrap();
        assert!(out.unwrap().contains("auto_import on"));
        let acts = actions(&tools);
        assert_eq!(acts[0].0, "hoi4_auto_import");
        assert_eq!(acts[0].1["enabled"], true);
        assert_eq!(acts[0].1["budget"], 4);
        assert!(state.done.contains_key("imp:on:4"));
        // once a game day; imports on already; after 14 days still low the player is woken
        assert_eq!(
            fuel_rule(&mut state, &tools, &plan, "l", START + 2).unwrap(),
            None
        );
        assert_eq!(reads(&tools, "hoi4_resources"), 1);
        assert_eq!(
            fuel_rule(&mut state, &tools, &plan, "l", START + DAY).unwrap(),
            None
        );
        assert!(wakes(&mut state).is_empty());
        assert_eq!(
            fuel_rule(&mut state, &tools, &plan, "l", START + 14 * DAY).unwrap(),
            None
        );
        let alarms = wakes(&mut state);
        assert_eq!(alarms[0]["reason"], "fuel_low");
        assert_eq!(alarms[0]["rule"], 26);
        assert_eq!(actions(&tools).len(), 1);
        // the resources rule's import counts: no second import action
        let mut state = State::default();
        state.done.insert("imp:on:4".into(), START);
        assert_eq!(
            fuel_rule(&mut state, &tools, &plan, "l", START).unwrap(),
            None
        );
        assert!(state.manage.notes["fuel"].contains("auto-import on"));
        // no auto_import: wake at once, never import
        let mut state = State::default();
        let plan = json!({"manage": {"fuel_floor": 25}});
        assert_eq!(
            fuel_rule(&mut state, &tools, &plan, "l", START).unwrap(),
            None
        );
        assert_eq!(wakes(&mut state)[0]["reason"], "fuel_low");
        assert_eq!(actions(&tools).len(), 1);
        // above the floor: nothing, the ratio in the digest
        set_reply(&tools, "hoi4_resources", json!({"fuel": {"ratio": 0.6}}));
        let mut state = State::default();
        assert_eq!(
            fuel_rule(&mut state, &tools, &plan, "l", START).unwrap(),
            None
        );
        assert!(wakes(&mut state).is_empty());
        assert_eq!(state.manage.notes["fuel"], "0.6 of capacity (floor 0.25)");
    }

    #[test]
    fn a_cut_import_budget_is_sent_again_when_civilian_factories_move() {
        let tools = fake(vec![
            (
                "hoi4_economy",
                json!({"factory_trigger_count": {"civilian": 30}}),
            ),
            (
                "hoi4_auto_import",
                json!({"auto_import": {"enabled": true, "budget_factories": 4, "available_factories": 20}}),
            ),
        ]);
        let plan = json!({"auto_import": {"budget": 10}});
        let mut state = State::default();
        let out = import_budget_rule(&mut state, &tools, &plan, "l", START).unwrap();
        assert!(out.unwrap().contains("budget 4 -> 10"));
        let acts = actions(&tools);
        assert_eq!(acts[0].0, "hoi4_auto_import");
        assert_eq!(
            acts[0].1,
            json!({"budget": 10, "request_id": acts[0].1["request_id"],
            "lease_token": "l"})
        );
        // weekly, and only when the civilian count moved by budget_step (2)
        set_reply(
            &tools,
            "hoi4_economy",
            json!({"factory_trigger_count": {"civilian": 31}}),
        );
        assert_eq!(
            import_budget_rule(&mut state, &tools, &plan, "l", START + 7 * DAY).unwrap(),
            None
        );
        assert_eq!(reads(&tools, "hoi4_auto_import"), 2); // one read, one action
        assert!(state.manage.notes["import_budget"].contains("budget not re-read"));
        // fewer factories available than the plan's budget: no more than they allow
        set_reply(
            &tools,
            "hoi4_economy",
            json!({"factory_trigger_count": {"civilian": 25}}),
        );
        set_reply(
            &tools,
            "hoi4_auto_import",
            json!({"auto_import": {"enabled": true, "budget_factories": 4, "available_factories": 6}}),
        );
        import_budget_rule(&mut state, &tools, &plan, "l", START + 14 * DAY).unwrap();
        assert_eq!(actions(&tools)[1].1["budget"], 6);
        // auto-import off, or a plan without a budget: no budget action
        set_reply(
            &tools,
            "hoi4_auto_import",
            json!({"auto_import": {"enabled": false, "budget_factories": 0}}),
        );
        let mut state = State::default();
        assert_eq!(
            import_budget_rule(&mut state, &tools, &plan, "l", START).unwrap(),
            None
        );
        assert!(state.manage.notes["import_budget"].contains("is off"));
        let mut state = State::default();
        assert_eq!(
            import_budget_rule(
                &mut state,
                &tools,
                &json!({"auto_import": true}),
                "l",
                START
            )
            .unwrap(),
            None
        );
        assert_eq!(actions(&tools).len(), 2);
    }

    fn air_read() -> Value {
        json!({"wings": [
            {"wing": "1. Stormo", "aircraft_type": "small_plane_airframe_0", "variant": "CR.32", "aircraft": 40,
             "air_base": {"base_id": 1, "state_id": 2}, "standing_by": true, "missions": [], "region": null},
            {"wing": "2. Stormo", "aircraft_type": "small_plane_cas_airframe_0", "aircraft": 48,
             "air_base": {"base_id": 3, "state_id": 550}, "standing_by": false, "missions": ["cas"],
             "region": {"id": 273, "name": "Ethiopia"}},
            {"wing": "3. Stormo", "aircraft_type": "fighter_equipment_0", "aircraft": 40,
             "air_base": {"base_id": 1, "state_id": 2}, "standing_by": true, "missions": []}],
            "air_bases": [
                {"base_id": 1, "state_id": 2, "region": {"id": 10, "name": "Italy"}, "free": -20},
                {"base_id": 3, "state_id": 550, "region": {"id": 273, "name": "Ethiopia"}, "free": 100},
                {"base_id": 4, "state_id": 551, "region": {"id": 273, "name": "Ethiopia"}, "free": 300}]})
    }

    #[test]
    fn the_air_plan_rebases_and_assigns_and_switches() {
        let tools = fake(vec![("hoi4_air", air_read())]);
        let plan = json!({"air_plan": [
            {"airframe": "small_plane_airframe_0", "version": "CR.32", "base_region": 273,
             "mission": "air_superiority", "region": 273},
            {"wing": "2. Stormo", "mission": "cas", "region": "Ethiopia", "then_region": 17,
             "switch_when": "date:1936.6.1"}]});
        let mut state = State::default();
        let out = step(&mut state, &tools, &plan, "l", START).unwrap();
        assert_eq!(
            out.as_deref(),
            Some("air_plan 1. Stormo rebase to state 551: true Verified"),
            "over capacity outside the region: to the base with the most room"
        );
        // in transfer: the mission is next, then the CAS wing already flies its plan
        let mut moved = air_read();
        moved["wings"][0]["transfer_to_base_id"] = json!(4);
        tools
            .replies
            .borrow_mut()
            .insert("hoi4_air".into(), moved.clone());
        let out = step(&mut state, &tools, &plan, "l", START).unwrap();
        assert_eq!(
            out.as_deref(),
            Some("air_plan 1. Stormo air_superiority over region 273: true Verified")
        );
        moved["wings"][0]["missions"] = json!(["air_superiority"]);
        moved["wings"][0]["region"] = json!({"id": 273, "name": "Ethiopia"});
        tools.replies.borrow_mut().insert("hoi4_air".into(), moved);
        assert_eq!(step(&mut state, &tools, &plan, "l", START).unwrap(), None);
        assert_eq!(
            state.manage.notes["air_plan"],
            "2 wing(s) on their planned mission"
        );
        // every 3 game days; after switch_when the CAS wing goes to region 17
        let switch = date_hour("1936.6.1").unwrap();
        assert_eq!(
            step(&mut state, &tools, &plan, "l", START + DAY).unwrap(),
            None
        );
        let out = step(&mut state, &tools, &plan, "l", switch.max(START + 3 * DAY)).unwrap();
        assert_eq!(
            out.as_deref(),
            Some("air_plan 2. Stormo cas over region 17: true Verified")
        );
        assert_eq!(actions(&tools).len(), 3);
    }

    #[test]
    fn a_refused_rebase_waits_30_days_and_the_wing_flies_its_mission_meanwhile() {
        let tools = fake(vec![("hoi4_air", air_read())]);
        tools
            .refuse
            .borrow_mut()
            .insert("hoi4_air_rebase".into(), "rebase_refused");
        let plan = json!({"air_plan": [
            {"airframe": "small_plane_airframe_0", "version": "CR.32", "base_region": 273,
             "mission": "air_superiority", "region": 273}]});
        let mut state = State::default();
        let out = step(&mut state, &tools, &plan, "l", START).unwrap();
        assert!(out.unwrap().contains("rebase to state 551: false"));
        let out = step(&mut state, &tools, &plan, "l", START + 3 * DAY).unwrap();
        assert_eq!(
            out.as_deref(),
            Some("air_plan 1. Stormo air_superiority over region 273: true Verified")
        );
        let rebases = actions(&tools)
            .iter()
            .filter(|(n, _)| n == "hoi4_air_rebase")
            .count();
        assert_eq!(rebases, 1);
    }

    #[test]
    fn a_wing_is_not_rebased_to_a_state_without_an_own_air_base() {
        let read = air_read();
        let wing = &read["wings"][0];
        let bases = read["air_bases"].as_array().unwrap();
        // base 3 stands in state 550: the wing goes there
        assert_eq!(
            rebase_target(&json!({"base_state": 550}), &Value::Null, wing, bases),
            Some(550)
        );
        // no own base in state 908 (not built yet): no rebase order
        assert_eq!(
            rebase_target(&json!({"base_state": 908}), &Value::Null, wing, bases),
            None
        );
        // already there
        assert_eq!(
            rebase_target(&json!({"base_state": 2}), &Value::Null, wing, bases),
            None
        );
    }

    #[test]
    fn a_wing_that_follows_its_region_is_based_inside_it() {
        let read = air_read();
        let wing = &read["wings"][0]; // based in state 2 (region Italy, over capacity)
        let bases = read["air_bases"].as_array().unwrap();
        let follow = json!({"follow_region": true, "base_region": 10});
        // flying over Ethiopia: the base there with the most room
        assert_eq!(rebase_target(&follow, &json!(273), wing, bases), Some(551));
        // already based inside the region it flies over: it stays
        let mut there = wing.clone();
        there["air_base"] = json!({"base_id": 3, "state_id": 550});
        assert_eq!(
            rebase_target(&follow, &json!("Ethiopia"), &there, bases),
            None
        );
        // no own base inside the region flown over: the planned base_region decides
        let far = json!({"follow_region": true, "base_region": 273});
        assert_eq!(rebase_target(&far, &json!(999), wing, bases), Some(551));
        // without follow_region the mission region does not move the wing
        assert_eq!(
            rebase_target(&json!({"base_region": 273}), &json!(10), &there, bases),
            None
        );
    }

    fn reserve(amount: f64) -> Value {
        json!({"equipment": [{"archetype": "small_plane_airframe", "stockpile": amount,
            "variants": [{"type": "small_plane_airframe_0", "design_id": 7, "amount": amount}]}]})
    }

    #[test]
    fn reserve_planes_beyond_the_wings_refill_found_a_new_wing() {
        let designs = json!({"designs": [{"id": 7, "name": "CR.32"}]});
        let entry = json!({"airframe": "small_plane_airframe_0", "version": "CR.32",
            "mission": "air_superiority", "region": 273, "base_region": 273, "deploy": true});
        // 1. Stormo holds 40 of 100: 60 of the reserve are its refill
        assert_eq!(
            deploy_choice(&entry, &air_read(), &reserve(160.0), &designs),
            Ok(("CR.32".to_string(), 100))
        );
        let short = deploy_choice(&entry, &air_read(), &reserve(90.0), &designs).unwrap_err();
        assert!(short.contains("below min_planes 50"), "{short}");
        let capped = json!({"airframe": "small_plane_airframe_0", "deploy": {"max_wings": 1}});
        assert!(
            deploy_choice(&capped, &air_read(), &reserve(500.0), &designs)
                .unwrap_err()
                .contains("max_wings 1")
        );
        let named = json!({"wing": "1. Stormo", "deploy": true});
        assert!(
            deploy_choice(&named, &air_read(), &reserve(500.0), &designs)
                .unwrap_err()
                .contains("airframe")
        );
        // another variant than the planned version is left in reserve
        let other = json!({"designs": [{"id": 7, "name": "CR.42"}]});
        assert!(deploy_choice(&entry, &air_read(), &reserve(500.0), &other).is_err());

        let tools = fake(vec![
            ("hoi4_air", air_read()),
            ("hoi4_stockpile", reserve(160.0)),
            ("hoi4_designs", designs),
        ]);
        let mut wings = air_read();
        wings["wings"][0]["air_base"] = json!({"base_id": 4, "state_id": 551});
        wings["wings"][0]["missions"] = json!(["air_superiority"]);
        wings["wings"][0]["region"] = json!({"id": 273, "name": "Ethiopia"});
        tools.replies.borrow_mut().insert("hoi4_air".into(), wings);
        let plan = json!({"air_plan": [entry]});
        let mut state = State::default();
        let out = step(&mut state, &tools, &plan, "l", START).unwrap();
        assert_eq!(
            out.as_deref(),
            Some("air_plan 0: new wing of 100 CR.32 from reserve at state 551: true Verified")
        );
        assert_eq!(
            actions(&tools)[0].1["state_id"],
            json!(551),
            "the base of base_region with the most room"
        );
        // too few in reserve: nothing is sent and the digest says why
        let tools = fake(vec![
            ("hoi4_air", air_read()),
            ("hoi4_stockpile", reserve(30.0)),
        ]);
        let plan = json!({"air_plan": [{"airframe": "fighter_equipment_0", "deploy": true,
            "base_state": 2}]});
        let mut state = State::default();
        assert_eq!(step(&mut state, &tools, &plan, "l", START).unwrap(), None);
        assert!(actions(&tools).is_empty());
        assert!(
            state.manage.notes["air_plan"].contains("0 fighter_equipment_0 in reserve"),
            "{}",
            state.manage.notes["air_plan"]
        );
        assert_eq!(
            reads(&tools, "hoi4_designs"),
            0,
            "no reserve of it: no design read"
        );
    }

    #[test]
    fn a_new_wing_waits_for_a_base_with_room() {
        let bases = air_read()["air_bases"].as_array().unwrap().clone();
        // state 2's base is over capacity
        assert_eq!(
            deploy_base(&json!({"base_state": 2}), &Value::Null, &bases, 50),
            None
        );
        assert_eq!(
            deploy_base(&json!({"base_state": 550}), &Value::Null, &bases, 100),
            Some(550)
        );
        assert_eq!(
            deploy_base(&json!({"follow_region": true}), &json!(273), &bases, 100),
            Some(551)
        );
        assert_eq!(
            deploy_base(&json!({"base_region": 273}), &Value::Null, &bases, 400),
            None
        );
    }

    fn overdrawn_read(pct: f64, room_here: f64) -> Value {
        let div = |id: &str, province: i64, fill: f64| {
            json!({"id": id, "army": "A", "state_id": 10, "province_id": province,
                "supply_percent": pct, "supply_need": 0.5, "equipment_fill_min": fill, "hp_percent": 90.0})
        };
        json!({"divisions": [
                div("1", 100, 0.9), div("2", 100, 0.4), div("3", 100, 0.95), div("4", 100, 0.6),
                div("5", 100, 0.5), div("6", 100, 1.0), div("7", 200, 1.0), div("8", 200, 1.0)],
            "armies": [{"army": "A", "divisions": 8}],
            "provinces": [
                {"province_id": 100, "state_id": 10, "headroom": -1.2},
                {"province_id": 200, "state_id": 10, "headroom": room_here},
                {"province_id": 300, "state_id": 11, "headroom": 5.0}],
            "overdrawn_provinces": [{"province_id": 100, "shortfall": 1.2,
                "division_ids": ["1", "2", "3", "4", "5", "6"]}]})
    }

    #[test]
    fn the_weakest_divisions_leave_an_overdrawn_stack_for_a_province_with_room() {
        let tools = fake(vec![
            ("hoi4_supply", overdrawn_read(20.0, 2.0)),
            (
                "hoi4_armies",
                json!({"armies": [{"army": "A"}, {"army": "Army 9"}]}),
            ),
        ]);
        let plan = json!({"manage": {}});
        let mut state = State::default();
        let out = step(&mut state, &tools, &plan, "l", START).unwrap();
        assert_eq!(
            out.as_deref(),
            Some("overdrawn: province 100 at 20% supply (shortfall 1.20): 3 of A's divisions out into Army 9 for state 10 province 200: true Verified")
        );
        // the three with the lowest equipment fill cover the 1.2 shortfall (3 x 0.5)
        assert_eq!(actions(&tools)[0].1["divisions"], json!(["2", "5", "4"]));
        assert!(state
            .war_cool
            .get("Army 9")
            .is_some_and(|t| *t == START + 10 * DAY));
        // the next tick moves the new army; then nothing until the next game day
        let out = step(&mut state, &tools, &plan, "l", START).unwrap();
        assert_eq!(
            out.as_deref(),
            Some("overdrawn: Army 9 out of the overdrawn stack to state 10 province 200: true Verified")
        );
        assert_eq!(
            actions(&tools)[1].1,
            json!({"army": "Army 9", "state_id": 10, "province_id": 200,
                "request_id": actions(&tools)[1].1["request_id"], "lease_token": "l"})
        );
        assert_eq!(step(&mut state, &tools, &plan, "l", START).unwrap(), None);
        assert_eq!(actions(&tools).len(), 2);
    }

    #[test]
    fn a_group_out_of_an_overdrawn_stack_is_not_merged_back_while_it_rests() {
        use super::super::plan_formations::command_step;
        // formation N1 is army A in state 10; Army 9 is the group detached from it there
        let tools = fake(vec![
            ("hoi4_supply", overdrawn_read(20.0, 2.0)),
            (
                "hoi4_armies",
                json!({"armies": [
                    {"army": "A", "division_count": 5, "commander": "X", "main_state": 10},
                    {"army": "Army 9", "division_count": 3, "commander": "Y", "main_state": 10}]}),
            ),
        ]);
        let plan = json!({"manage": {}, "armies": {},
            "army_layout": [{"name_hint": "N1", "state": 10, "role": "strike"}]});
        let mut state = State::default();
        state.layout.insert("N1".into(), "A".into());
        let merges = |tools: &Fake| {
            actions(tools)
                .iter()
                .filter(|(n, _)| n == "hoi4_army_merge")
                .count()
        };
        let out = step(&mut state, &tools, &plan, "l", START)
            .unwrap()
            .unwrap();
        assert!(out.contains("divisions out into Army 9"), "{out}");
        assert!(spread_out(&state, "Army 9") && !spread_out(&state, "A"));
        // detached and not yet moved: the command step leaves it alone
        assert_eq!(
            command_step(&mut state, &tools, &plan, "l", START).unwrap(),
            None
        );
        let out = step(&mut state, &tools, &plan, "l", START)
            .unwrap()
            .unwrap();
        assert!(
            out.starts_with("overdrawn: Army 9 out of the overdrawn stack"),
            "{out}"
        );
        // moved and resting on fed ground: still not merged
        assert_eq!(
            command_step(&mut state, &tools, &plan, "l", START + DAY).unwrap(),
            None
        );
        assert_eq!(merges(&tools), 0, "{:?}", actions(&tools));
        // the rest is over and the stack is fed: the rule forgets the group, and the stray
        // army in the formation's state is merged as before
        tools
            .replies
            .borrow_mut()
            .insert("hoi4_supply".into(), overdrawn_read(60.0, 2.0));
        assert_eq!(
            step(&mut state, &tools, &plan, "l", START + 10 * DAY).unwrap(),
            None
        );
        assert!(!spread_out(&state, "Army 9"));
        let out = command_step(&mut state, &tools, &plan, "l", START + 10 * DAY)
            .unwrap()
            .unwrap();
        assert!(
            out.starts_with("command: Army 9 (3 division(s)) merged into A (N1): true"),
            "{out}"
        );
        assert_eq!(merges(&tools), 1);
    }

    #[test]
    fn an_overdrawn_stack_above_the_floor_or_without_room_elsewhere_stays() {
        let plan = json!({"manage": {"overdrawn_floor": 35}});
        let tools = fake(vec![("hoi4_supply", overdrawn_read(60.0, 2.0))]);
        let mut state = State::default();
        assert_eq!(step(&mut state, &tools, &plan, "l", START).unwrap(), None);
        assert!(state.manage.notes["overdrawn"].contains("above the floor"));
        // no province has 1.5 of room: nothing leaves, the digest names the lever
        let mut read = overdrawn_read(20.0, 0.1);
        read["provinces"][2]["headroom"] = json!(1.0);
        let tools = fake(vec![("hoi4_supply", read)]);
        let mut state = State::default();
        assert_eq!(step(&mut state, &tools, &plan, "l", START).unwrap(), None);
        assert!(actions(&tools).is_empty());
        assert!(
            state.manage.notes["overdrawn"].contains("railway or hub needed"),
            "{}",
            state.manage.notes["overdrawn"]
        );
        // switched off
        let off = json!({"manage": {"off": ["overdrawn"]}});
        let tools = fake(vec![("hoi4_supply", overdrawn_read(20.0, 2.0))]);
        let mut state = State::default();
        assert_eq!(step(&mut state, &tools, &off, "l", START).unwrap(), None);
        assert!(actions(&tools).is_empty());
    }

    #[test]
    fn default_war_missions_leave_the_air_plan_wings_alone() {
        let tools = fake(vec![
            ("hoi4_air", air_read()),
            (
                "hoi4_wars",
                json!({"borders": [{"hostile": true, "own_border_states": [550]}]}),
            ),
        ]);
        let plan = json!({"air": {"war_missions": true},
            "air_plan": [{"airframe": "small_plane_airframe_0", "mission": "cas", "region": 273}]});
        assert!(air_plan_claims(&plan, &air_read()["wings"][0]));
        assert!(!air_plan_claims(&plan, &air_read()["wings"][2]));
        // 3. Stormo stands by at a base in the home country: the front is out of its range
        let mut state = State::default();
        let out = extras_step(&mut state, &tools, &plan, "l", START).unwrap();
        assert_eq!(out, None, "no wing is based at the front");
        // based at the front it flies; 1. Stormo belongs to the air plan
        let mut at_front = air_read();
        at_front["wings"][2]["air_base"] = json!({"base_id": 3, "state_id": 550});
        let tools = fake(vec![
            ("hoi4_air", at_front),
            (
                "hoi4_wars",
                json!({"borders": [{"hostile": true, "own_border_states": [550]}]}),
            ),
        ]);
        let mut state = State::default();
        let out = extras_step(&mut state, &tools, &plan, "l", START).unwrap();
        assert_eq!(
            out.as_deref(),
            Some("air 3. Stormo air_superiority over state 550: true Verified"),
            "1. Stormo belongs to the air plan"
        );
    }

    fn war_stats(total: f64) -> Value {
        json!({"player": {"tag": "ITA"}, "wars": [{"includes_player": true, "sides": [
            {"countries": [{"tag": "ITA", "casualties_total": total}]},
            {"countries": [{"tag": "ETH", "casualties_total": 1.0}]}]}]})
    }

    fn pp(points: f64) -> Value {
        json!({"political_power": {"raw_signed": points * 100_000.0}})
    }

    #[test]
    fn unspent_political_power_buys_the_standing_order_above_the_plan_reserve() {
        let tools = fake(vec![
            ("hoi4_economy", pp(150.0)),
            (
                "hoi4_politics_options",
                json!({"current": null, "options": [
                    {"idea": "ITA_off", "enabled": false}, {"idea": "ITA_chief", "enabled": true}]}),
            ),
        ]);
        // a pending plan pick of 100 leaves 50: enough for the chief of army (50), not a law
        let plan = json!({"politics": [{"id": "p", "category": "government", "slot": 3,
            "idea": "x", "cost": 100}]});
        let mut state = State::default();
        let out = politics_auto_step(&mut state, &tools, &plan, "l", START).unwrap();
        assert!(
            out.as_deref()
                .unwrap_or("")
                .contains("military_staff:0 vacant -> ITA_chief"),
            "{out:?}"
        );
        let acts = actions(&tools);
        assert_eq!(acts.len(), 1);
        assert_eq!(acts[0].0, "hoi4_politics_choose");
        assert_eq!(acts[0].1["idea"], "ITA_chief");
        assert!(state.manage.notes["politics_auto"].contains("vacant military staff seat"));
        assert!(state.manage.notes["politics_auto_picks"].contains("ITA_chief: true"));
        // below the threshold the power is banked and no window opens
        let low = fake(vec![("hoi4_economy", pp(90.0))]);
        let mut state = State::default();
        assert_eq!(
            politics_auto_step(&mut state, &low, &json!({}), "l", START).unwrap(),
            None
        );
        assert_eq!(reads(&low, "hoi4_politics_options"), 0);
        assert!(state.manage.notes["politics_auto"].contains("banked"));
        // switched off
        let mut state = State::default();
        let off = json!({"politics_auto": false});
        assert_eq!(
            politics_auto_step(&mut state, &tools, &off, "l", START).unwrap(),
            None
        );
    }

    #[test]
    fn politics_auto_moves_only_up_its_ranking_and_never_into_conscription() {
        let offered: Vec<(String, bool)> =
            ["war_economy", "partial_economic_mobilisation", "x_law"]
                .iter()
                .map(|s| (s.to_string(), true))
                .collect();
        let economy = json!({"category": "government", "slot": 2,
            "ideas": ["war_economy", "partial_economic_mobilisation"]});
        let none: Vec<String> = Vec::new();
        assert_eq!(
            politics_auto_choice(&economy, "civilian_economy", &offered, &none, &none),
            Err((
                "holds civilian_economy, which the order does not rank".into(),
                false
            ))
        );
        assert_eq!(
            politics_auto_choice(
                &economy,
                "partial_economic_mobilisation",
                &offered,
                &none,
                &none
            ),
            Ok("war_economy".into())
        );
        assert!(politics_auto_choice(&economy, "war_economy", &offered, &none, &none).is_err());
        let never = vec!["war_economy".to_string()];
        assert!(politics_auto_choice(&economy, "", &offered, &never, &none)
            .is_ok_and(|i| i == "partial_economic_mobilisation"));
        // government slot 0 (conscription) only within manage.conscription_laws
        let draft = json!({"category": "government", "slot": 0});
        assert!(politics_auto_choice(&draft, "", &offered, &none, &none).is_err());
        let allowed = vec!["x_law".to_string()];
        assert_eq!(
            politics_auto_choice(&draft, "", &offered, &none, &allowed),
            Ok("x_law".into())
        );
        // a slot without a ranking is filled only while vacant
        let chief = json!({"category": "military_staff", "slot": 0});
        assert!(politics_auto_choice(&chief, "ITA_a", &offered, &none, &none).is_err());
        // a wanted idea the game disables (power or conditions) is told apart from nothing wanted
        let greyed = vec![
            ("war_economy".to_string(), false),
            ("partial_economic_mobilisation".to_string(), true),
        ];
        assert_eq!(
            politics_auto_choice(
                &economy,
                "partial_economic_mobilisation",
                &greyed,
                &none,
                &none
            ),
            Err((
                "holds partial_economic_mobilisation; nothing better enabled".into(),
                true
            ))
        );
        assert_eq!(
            politics_auto_choice(&chief, "", &greyed[..1], &none, &none),
            Err(("vacant; nothing enabled".into(), true))
        );
        // a disabled idea the plan never takes is no reason to read again
        assert_eq!(
            politics_auto_choice(&chief, "", &greyed[..1], &never, &none),
            Err(("vacant; nothing enabled".into(), false))
        );
    }

    #[test]
    fn a_pick_refused_for_power_keeps_its_cost_in_reserve() {
        let tools = fake(vec![
            ("hoi4_economy", pp(120.0)),
            (
                "hoi4_politics_options",
                json!({"current": null, "options": [{"idea": "ITA_roatta", "enabled": true}]}),
            ),
        ]);
        let plan = json!({
            "politics": [{"id": "stab", "category": "government", "slot": 5,
                          "idea": "ITA_mazzolini", "cost": 150}],
            "politics_auto": {"threshold": 0, "order": [
                {"category": "military_staff", "slot": 5, "ideas": ["ITA_roatta"], "cost": 50}]}});
        let mut state = State::default();
        // refused at 76 of its 150: the refusal was the power, so the cost stays held back
        state.skip(START, "pol", "pol:id:stab", 3 * DAY);
        state.pol_pp_at_fail.insert("pol:id:stab".into(), 76.0);
        assert_eq!(
            politics_auto_step(&mut state, &tools, &plan, "l", START).unwrap(),
            None
        );
        let line = &state.manage.notes["politics_auto"];
        assert!(line.contains("plan reserve 150"), "{line}");
    }

    #[test]
    fn a_gated_or_refused_plan_pick_holds_no_power_back() {
        // the live case: a law behind war support and an advisor the game refused kept 200 PP
        // in reserve all year, so the standing order never had enough to buy anything
        let tools = fake(vec![
            ("hoi4_economy", pp(192.0)),
            (
                "hoi4_country_status",
                json!({"fields": {"war_support": {"value": 0.4}}}),
            ),
            (
                "hoi4_politics_options",
                json!({"current": null, "options": [{"idea": "ITA_roatta", "enabled": true}]}),
            ),
        ]);
        let plan = json!({
            "politics": [
                {"id": "we", "category": "government", "slot": 2, "idea": "war_economy",
                 "cost": 150, "when": "war_support > 0.5"},
                {"id": "st", "category": "military_staff", "slot": 4, "idea": "ITA_starace",
                 "cost": 50}],
            "politics_auto": {"threshold": 0, "order": [
                {"category": "military_staff", "slot": 5, "ideas": ["ITA_roatta"], "cost": 50}]}});
        let mut state = State::default();
        // the advisor was refused: it backs off (tick step 4)
        state.skip(START, "pol", "pol:id:st", 12 * DAY);
        let out = politics_auto_step(&mut state, &tools, &plan, "l", START).unwrap();
        assert!(
            out.as_deref().unwrap_or("").contains("-> ITA_roatta"),
            "{out:?}"
        );
        let line = &state.manage.notes["politics_auto"];
        assert!(line.contains("plan reserve 0"), "{line}");
        assert!(
            line.contains("war_economy not held (its when does not hold)"),
            "{line}"
        );
        assert!(
            line.contains("ITA_starace not held (refused, tried again in 12 days)"),
            "{line}"
        );
        // once the gate opens the law holds its cost again: 192 - 150 leaves 42, short of 50
        let open = fake(vec![
            ("hoi4_economy", pp(192.0)),
            (
                "hoi4_country_status",
                json!({"fields": {"war_support": {"value": 0.6}}}),
            ),
        ]);
        let mut state = State::default();
        state.skip(START, "pol", "pol:id:st", 12 * DAY);
        assert_eq!(
            politics_auto_step(&mut state, &open, &plan, "l", START).unwrap(),
            None
        );
        let line = &state.manage.notes["politics_auto"];
        assert!(
            line.contains("plan reserve 150: war_economy holds 150"),
            "{line}"
        );
        assert!(
            line.contains("military_staff:5 needs 50, 42 free above the reserve"),
            "every entry passed over says why: {line}"
        );
        assert_eq!(reads(&open, "hoi4_politics_options"), 0);
    }

    #[test]
    fn a_slot_whose_wanted_idea_is_disabled_is_read_again_once_the_power_has_grown() {
        let tools = fake(vec![
            ("hoi4_economy", pp(140.0)),
            (
                "hoi4_politics_options",
                json!({"current": "partial_economic_mobilisation", "options": [
                    {"idea": "war_economy", "enabled": false},
                    {"idea": "partial_economic_mobilisation", "enabled": true}]}),
            ),
        ]);
        let plan = json!({"politics_auto": {"order": [{"category": "government", "slot": 2,
            "ideas": ["war_economy", "partial_economic_mobilisation"], "cost": 100}]}});
        let mut state = State::default();
        assert_eq!(
            politics_auto_step(&mut state, &tools, &plan, "l", START).unwrap(),
            None
        );
        assert!(
            state.manage.notes["politics_auto"]
                .contains("a wanted idea is disabled: read again at PP 165"),
            "{}",
            state.manage.notes["politics_auto"]
        );
        assert_eq!(reads(&tools, "hoi4_politics_options"), 1);
        // a day later with a little more power: not read again, and the note says when it will be
        let later = START + DAY;
        tools
            .replies
            .borrow_mut()
            .insert("hoi4_economy".into(), pp(150.0));
        politics_auto_step(&mut state, &tools, &plan, "l", later).unwrap();
        assert_eq!(reads(&tools, "hoi4_politics_options"), 1);
        assert!(
            state.manage.notes["politics_auto"].contains("read again in 29 days or at PP 165"),
            "{}",
            state.manage.notes["politics_auto"]
        );
        // the power has grown by recheck_pp: the slot is read again long before recheck_days
        tools
            .replies
            .borrow_mut()
            .insert("hoi4_economy".into(), pp(166.0));
        tools.replies.borrow_mut().insert(
            "hoi4_politics_options".into(),
            json!({"current": "partial_economic_mobilisation", "options": [
                {"idea": "war_economy", "enabled": true}]}),
        );
        let out = politics_auto_step(&mut state, &tools, &plan, "l", later + 2 * DAY).unwrap();
        assert!(
            out.as_deref()
                .unwrap_or("")
                .contains("partial_economic_mobilisation -> war_economy"),
            "{out:?}"
        );
        // a slot that holds the best idea already waits recheck_days whatever the power does
        let full = fake(vec![
            ("hoi4_economy", pp(140.0)),
            (
                "hoi4_politics_options",
                json!({"current": "war_economy", "options": [{"idea": "war_economy", "enabled": true}]}),
            ),
        ]);
        let mut state = State::default();
        politics_auto_step(&mut state, &full, &plan, "l", START).unwrap();
        full.replies
            .borrow_mut()
            .insert("hoi4_economy".into(), pp(400.0));
        politics_auto_step(&mut state, &full, &plan, "l", START + 2 * DAY).unwrap();
        assert_eq!(reads(&full, "hoi4_politics_options"), 1);
    }

    #[test]
    fn the_default_order_fills_every_vacant_military_staff_seat_without_a_game_key() {
        // the first read lists the seats; the chief's seat is taken, seats 3 and 5 are vacant
        let tools = fake(vec![
            ("hoi4_economy", pp(200.0)),
            (
                "hoi4_politics_options",
                json!({"current": "X_chief", "options": [{"idea": "X_a", "enabled": true}],
                    "slots": [{"slot": 0, "current": "X_chief"}, {"slot": 1, "current": "X_nav"},
                        {"slot": 2, "current": "X_air"}, {"slot": 3, "current": null},
                        {"slot": 4, "current": "X_hc"}, {"slot": 5, "current": null}]}),
            ),
        ]);
        let plan = json!({});
        let mut state = State::default();
        assert_eq!(
            politics_auto_step(&mut state, &tools, &plan, "l", START).unwrap(),
            None
        );
        assert!(state.manage.notes["politics_auto"].contains("military_staff:0 holds X_chief"));
        tools.replies.borrow_mut().insert(
            "hoi4_politics_options".into(),
            json!({"current": null, "options": [{"idea": "X_b", "enabled": false},
                {"idea": "X_c", "enabled": true}]}),
        );
        let out = politics_auto_step(&mut state, &tools, &plan, "l", START + DAY).unwrap();
        assert!(
            out.as_deref()
                .unwrap_or("")
                .contains("military_staff:3 vacant -> X_c"),
            "{out:?}"
        );
        let acts = actions(&tools);
        assert_eq!(acts.len(), 1);
        assert_eq!(acts[0].1["slot"], 3);
        // laws are the plan's to name: the default order never opens a government slot
        assert!(tools
            .calls
            .borrow()
            .iter()
            .all(|(_, a)| a["category"] != json!("government")));
    }

    #[test]
    fn power_at_the_cap_or_with_nothing_to_buy_wakes_the_player() {
        let tools = fake(vec![
            ("hoi4_economy", pp(1990.0)),
            (
                "hoi4_politics_options",
                json!({"current": "war_economy", "options": [{"idea": "a", "enabled": true}]}),
            ),
        ]);
        let plan = json!({"politics_auto": {"idle_wake_days": 0}});
        let mut state = State::default();
        assert_eq!(
            politics_auto_step(&mut state, &tools, &plan, "l", START).unwrap(),
            None
        );
        assert!(actions(&tools).is_empty());
        let reasons: Vec<Value> = wakes(&mut state)
            .iter()
            .map(|w| w["reason"].clone())
            .collect();
        assert_eq!(
            reasons,
            [
                json!("political_power_at_cap"),
                json!("political_power_idle")
            ]
        );
        assert!(state.manage.notes["politics_auto"].contains("nothing to buy"));
        // the scan waits a game day before reading the windows again
        let opened = reads(&tools, "hoi4_politics_options");
        politics_auto_step(&mut state, &tools, &plan, "l", START + 2).unwrap();
        assert_eq!(reads(&tools, "hoi4_politics_options"), opened);
    }

    #[test]
    fn power_saved_for_a_pick_it_cannot_yet_pay_is_not_idle() {
        let tools = fake(vec![("hoi4_economy", pp(60.0))]);
        let plan = json!({
            "politics": [{"category": "military_staff", "slot": 3, "idea": "X_a", "cost": 100}],
            "politics_auto": {"threshold": 0, "idle_wake_days": 0,
                "order": [{"category": "government", "slot": 3, "ideas": ["X_b"], "cost": 150}]}});
        let mut state = State::default();
        for day in 0..40 {
            politics_auto_step(&mut state, &tools, &plan, "l", START + day * DAY).unwrap();
        }
        assert!(wakes(&mut state).is_empty());
        assert!(state.manage.notes["politics_auto"].contains("saved for the next buy"));
        assert!(actions(&tools).is_empty());
    }

    #[test]
    fn power_is_saved_for_an_urgent_entry_before_cheaper_ones_after_it() {
        let tools = fake(vec![
            ("hoi4_economy", pp(120.0)),
            (
                "hoi4_country_status",
                json!({"fields": {"stability": {"value": 0.42}}}),
            ),
            (
                "hoi4_politics_options",
                json!({"current": null, "options": [{"idea": "X_staff", "enabled": true}]}),
            ),
        ]);
        let plan = json!({"politics_auto": {"threshold": 0, "idle_wake_days": 0, "order": [
            {"category": "government", "slot": 5, "ideas": ["X_calm"], "cost": 150,
             "when": "stability < 0.5"},
            {"category": "military_staff", "slot": 5, "ideas": ["X_staff"], "cost": 50}]}});
        let mut state = State::default();
        politics_auto_step(&mut state, &tools, &plan, "l", START).unwrap();
        assert!(actions(&tools).is_empty());
        assert!(state.manage.notes["politics_auto"].contains("power is saved for it"));
        // stable again: the urgent entry no longer holds and the cheaper one is bought
        tools.replies.borrow_mut().insert(
            "hoi4_country_status".into(),
            json!({"fields": {"stability": {"value": 0.7}}}),
        );
        politics_auto_step(&mut state, &tools, &plan, "l", START + DAY).unwrap();
        assert_eq!(actions(&tools).len(), 1);
        assert_eq!(actions(&tools)[0].1["idea"], "X_staff");
    }

    #[test]
    fn manpower_below_two_months_of_losses_takes_only_a_listed_law() {
        let tools = fake(vec![
            (
                "hoi4_country_status",
                json!({"fields": {"manpower": {"value": 50000.0}, "stability": {"value": 0.7}}}),
            ),
            ("hoi4_war_stats", war_stats(10000.0)),
            (
                "hoi4_politics_options",
                json!({"current": "volunteer_only", "options": [
                {"idea": "limited_conscription", "enabled": true},
                {"idea": "extensive_conscription", "enabled": false}]}),
            ),
        ]);
        // without manage.conscription_laws the rule never takes a law: it wakes the player
        let unlisted = json!({"manage": {"off": ["equipment_fill", "resources"]}});
        let mut state = State::default();
        assert_eq!(
            step(&mut state, &tools, &unlisted, "l", START).unwrap(),
            None
        );
        tools
            .replies
            .borrow_mut()
            .insert("hoi4_war_stats".into(), war_stats(50000.0));
        assert_eq!(
            step(&mut state, &tools, &unlisted, "l", START + 10 * DAY).unwrap(),
            None
        );
        assert!(state.manage.notes["manpower"].contains("lists no conscription law"));
        assert_eq!(wakes(&mut state)[0]["reason"], "manpower_low");
        assert!(actions(&tools).is_empty());
        // listed: the first listed law harsher than the current one
        tools
            .replies
            .borrow_mut()
            .insert("hoi4_war_stats".into(), war_stats(10000.0));
        let plan = json!({"manage": {"off": ["equipment_fill", "resources"],
            "conscription_laws": ["limited_conscription", "extensive_conscription"]}});
        let mut state = State::default();
        assert_eq!(step(&mut state, &tools, &plan, "l", START).unwrap(), None);
        assert!(state.manage.notes["manpower"].contains("collecting"));
        // 10 days later 40000 more losses: 4000 a day, 240000 in two months > 50000 free
        tools
            .replies
            .borrow_mut()
            .insert("hoi4_war_stats".into(), war_stats(50000.0));
        assert_eq!(
            step(&mut state, &tools, &plan, "l", START + 3 * DAY).unwrap(),
            None
        );
        let out = step(&mut state, &tools, &plan, "l", START + 10 * DAY).unwrap();
        assert_eq!(
            out.as_deref(),
            Some("manpower 50000 < 240000: conscription limited_conscription: true Verified")
        );
        assert_eq!(actions(&tools)[0].1["slot"], 0);
        // no harsher enabled law: the player is woken instead
        tools.replies.borrow_mut().insert(
            "hoi4_politics_options".into(),
            json!({"current": "limited_conscription", "options": [{"idea": "extensive_conscription", "enabled": false}]}),
        );
        tools
            .replies
            .borrow_mut()
            .insert("hoi4_war_stats".into(), war_stats(90000.0));
        assert_eq!(
            step(&mut state, &tools, &plan, "l", START + 17 * DAY).unwrap(),
            None
        );
        assert_eq!(wakes(&mut state)[0]["reason"], "manpower_low");
        assert_eq!(actions(&tools).len(), 1);
    }

    #[test]
    fn low_stability_and_low_equipment_fill_wake_and_an_unread_stockpile_is_unknown() {
        let tools = fake(vec![
            (
                "hoi4_country_status",
                json!({"fields": {"stability": {"value": 0.42}}}),
            ),
            (
                "hoi4_supply",
                json!({"divisions": [division("Inf", 50, 100), division("Inf", 70, 100)]}),
            ),
        ]);
        let plan =
            json!({"manage": {"off": ["manpower", "resources"]}, "disband": {"templates": ["X"]}});
        let mut state = State::default();
        state.skip(START, "disband", "x", 5 * DAY);
        assert_eq!(step(&mut state, &tools, &plan, "l", START).unwrap(), None);
        let alarms = wakes(&mut state);
        let reasons: Vec<&str> = alarms.iter().filter_map(|a| a["reason"].as_str()).collect();
        assert_eq!(reasons, vec!["stability_low", "equipment_fill_low"]);
        assert_eq!(
            alarms[1]["evidence"]["armies"][0]["equipment_fill_mean"],
            0.6
        );
        assert!(
            !state.skipped(START, "disband", "x"),
            "the disband step is nudged"
        );
        let manage = digest(&state).unwrap();
        assert!(manage["stockpile_days"]
            .as_str()
            .unwrap()
            .starts_with("unknown"));
        // no repeat inside the cadence and the wake window
        step(&mut state, &tools, &plan, "l", START + 3 * DAY).unwrap();
        assert!(wakes(&mut state).is_empty());
        assert_eq!(reads(&tools, "hoi4_country_status"), 1);
    }

    fn stock_rows(inf_days: Value, trucks: f64, truck_need: f64) -> Value {
        let trucks = json!({"archetype": "motorized_equipment", "stockpile": trucks,
            "fielded_need": truck_need, "fielded_short": 0.0, "days_of_cover": null,
            "depleting": false, "variants": []});
        json!({"equipment": [
            {"archetype": "infantry_equipment", "stockpile": 900.0, "fielded_short": 0.0,
             "days_of_cover": inf_days, "depleting": true,
             "variants": [{"type": "infantry_equipment_1", "amount": 900.0}]},
            trucks.clone()], "trucks": trucks})
    }

    fn lines_with(idle: i64) -> Vec<(&'static str, Value)> {
        vec![
            (
                "hoi4_economy",
                json!({"factory_trigger_count": {"military": 20 + idle}}),
            ),
            (
                "hoi4_production_lines",
                json!({"lines": [
                    {"entry_index": 0, "line_kind": "military", "equipment_id": "infantry_equipment_1",
                     "factory_counters": {"requested_factories_raw_signed": 12}, "output": {"efficiency": 55.0}},
                    {"entry_index": 1, "line_kind": "military", "equipment_id": "artillery_equipment_1",
                     "factory_counters": {"requested_factories_raw_signed": 8}, "output": {"efficiency": 40.0}}]}),
            ),
        ]
    }

    #[test]
    fn low_stockpile_cover_raises_its_line_and_wakes_without_idle_factories() {
        let mut replies = lines_with(5);
        replies.push(("hoi4_stockpile", stock_rows(json!(12.0), 500.0, 100.0)));
        let tools = fake(replies);
        let plan = json!({"manage": {}});
        let mut state = State::default();
        let out = step(&mut state, &tools, &plan, "l", START)
            .unwrap()
            .unwrap();
        assert!(out.starts_with("stockpile infantry_equipment low"), "{out}");
        let sent = actions(&tools);
        assert_eq!(sent[0].0, "hoi4_production_set");
        assert_eq!(sent[0].1["line"], 0);
        assert_eq!(sent[0].1["factories"], 15);
        // once a game day, and the line waits after a raise
        assert_eq!(
            step(&mut state, &tools, &plan, "l", START + 2).unwrap(),
            None
        );
        // no idle factory, 5 days of cover: the player is woken
        let mut replies = lines_with(0);
        replies.push(("hoi4_stockpile", stock_rows(json!(5.0), 500.0, 100.0)));
        let tools = fake(replies);
        let mut state = State::default();
        assert_eq!(step(&mut state, &tools, &plan, "l", START).unwrap(), None);
        assert!(actions(&tools).is_empty());
        assert_eq!(wakes(&mut state)[0]["reason"], "stockpile_low");
        // a plan target: artillery has no line falling, infantry at 40 days is above its 35
        let mut replies = lines_with(5);
        replies.push(("hoi4_stockpile", stock_rows(json!(40.0), 500.0, 100.0)));
        let tools = fake(replies);
        let mut state = State::default();
        let targets =
            json!({"manage": {}, "stockpile_targets": {"infantry_equipment_1": 35, "why": "x"}});
        assert_eq!(
            step(&mut state, &tools, &targets, "l", START).unwrap(),
            None
        );
        assert!(state.manage.notes["stockpile_days"].contains("infantry_equipment_1 40/35"));
        // a stock that is not falling is left alone
        let mut replies = lines_with(5);
        replies.push(("hoi4_stockpile", stock_rows(json!(null), 500.0, 100.0)));
        let tools = fake(replies);
        let mut state = State::default();
        let mut quiet = json!({"manage": {}});
        quiet["stockpile_targets"] = json!({"infantry_equipment": 30});
        assert_eq!(step(&mut state, &tools, &quiet, "l", START).unwrap(), None);
    }

    #[test]
    fn a_shortage_at_the_front_takes_factories_from_a_well_stocked_line_when_none_is_idle() {
        let mut replies = lines_with(0);
        let trucks = json!({"archetype": "motorized_equipment", "stockpile": 500.0,
            "fielded_need": 100.0, "fielded_short": 0.0, "days_of_cover": null,
            "depleting": false, "variants": []});
        replies.push((
            "hoi4_stockpile",
            json!({"equipment": [
                {"archetype": "infantry_equipment", "stockpile": 900.0, "fielded_short": 0.0,
                 "days_of_cover": null, "depleting": false,
                 "variants": [{"type": "infantry_equipment_1", "amount": 900.0}]},
                {"archetype": "artillery_equipment", "stockpile": 0.0, "fielded_short": 30.0,
                 "days_of_cover": 0.0, "depleting": true,
                 "variants": [{"type": "artillery_equipment_1", "amount": 0.0}]},
                trucks.clone()], "trucks": trucks}),
        ));
        let tools = fake(replies);
        let plan = json!({"manage": {}});
        let mut state = State::default();
        let out = step(&mut state, &tools, &plan, "l", START)
            .unwrap()
            .unwrap();
        assert!(out.contains("no idle factory"), "{out}");
        let sent = actions(&tools);
        assert_eq!(sent.len(), 2, "{sent:?}");
        assert_eq!(
            (sent[0].1["line"].clone(), sent[0].1["factories"].clone()),
            (json!(0), json!(9))
        );
        assert_eq!(
            (sent[1].1["line"].clone(), sent[1].1["factories"].clone()),
            (json!(1), json!(11))
        );
        // switched off by the plan: the player is woken instead
        let mut replies = lines_with(0);
        replies.push(("hoi4_stockpile", stock_rows(json!(5.0), 500.0, 100.0)));
        let tools = fake(replies);
        let mut state = State::default();
        let off = json!({"manage": {"shift_factories": false}});
        assert_eq!(step(&mut state, &tools, &off, "l", START).unwrap(), None);
        assert!(actions(&tools).is_empty());
    }

    #[test]
    fn few_trucks_start_a_truck_line_and_the_rules_switch_off() {
        let mut replies = lines_with(4);
        replies.push(("hoi4_stockpile", stock_rows(json!(null), 120.0, 100.0)));
        let tools = fake(replies);
        let plan = json!({"manage": {}});
        let mut state = State::default();
        let out = step(&mut state, &tools, &plan, "l", START)
            .unwrap()
            .unwrap();
        assert!(out.contains("below 1.5 x"), "{out}");
        let sent = actions(&tools);
        assert_eq!(sent[0].0, "hoi4_production_add");
        assert_eq!(sent[0].1["equipment"], "motorized_equipment");
        assert_eq!(sent[0].1["factories"], 3);
        // switched off: nothing
        let mut replies = lines_with(4);
        replies.push(("hoi4_stockpile", stock_rows(json!(3.0), 120.0, 100.0)));
        let tools = fake(replies);
        let mut state = State::default();
        let off = json!({"manage": {"off": ["stockpile", "trucks"]}});
        assert_eq!(step(&mut state, &tools, &off, "l", START).unwrap(), None);
        assert_eq!(reads(&tools, "hoi4_stockpile"), 0);
    }

    #[test]
    fn idle_factories_go_to_the_line_with_the_least_stockpile_cover() {
        let mut replies = lines_with(3);
        replies.push((
            "hoi4_stockpile",
            json!({"equipment": [
                {"archetype": "infantry_equipment", "fielded_short": 0.0, "days_of_cover": 80.0, "variants": []},
                {"archetype": "artillery_equipment", "fielded_short": 0.0, "days_of_cover": 20.0, "variants": []}]}),
        ));
        let tools = fake(replies);
        let plan = json!({"production_priority": ["infantry_equipment", "artillery_equipment"]});
        let mut state = State::default();
        let economy = tools.call("hoi4_economy", &json!({})).unwrap();
        let out = rebalance_step(&mut state, &tools, &plan, "l", START, &economy)
            .unwrap()
            .unwrap();
        assert!(
            out.starts_with("production_set artillery_equipment_1 8+3"),
            "{out}"
        );
        let line = &state.manage.notes["production"];
        assert!(
            line.contains("fewest days of stockpile cover (20)"),
            "{line}"
        );
        assert!(line.contains("line efficiency 40%"), "{line}");
        // a shortage read from the stockpile wins over cover
        let mut replies = lines_with(3);
        replies.push((
            "hoi4_stockpile",
            json!({"equipment": [
                {"archetype": "infantry_equipment", "fielded_short": 300.0, "days_of_cover": 80.0, "variants": []},
                {"archetype": "artillery_equipment", "fielded_short": 0.0, "days_of_cover": 20.0, "variants": []}]}),
        ));
        let tools = fake(replies);
        let mut state = State::default();
        let out = rebalance_step(&mut state, &tools, &plan, "l", START, &economy)
            .unwrap()
            .unwrap();
        assert!(
            out.starts_with("production_set infantry_equipment_1 12+3"),
            "{out}"
        );
    }
}
