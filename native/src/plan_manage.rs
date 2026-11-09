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
//!                    absent) fielded (hoi4_supply) plus queued; an unreadable count waits. No tool reads
//!                    the equipment stockpile, so `only_if_stockpile` is a proxy: training waits while
//!                    any fielded division lacks equipment of that archetype (the game reinforces from
//!                    the stockpile first, so it holds less than n); otherwise the guard is unknown,
//!                    training goes ahead and the digest says so
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
//!                    (the 1.19 row) is accepted and skipped with a note: no tool fills that row yet.
//!                    `role` "strike" / "assault" / "armour" / "mobile" puts the template on elite
//!                    reinforcement priority once a war with a `war.enemies` tag runs, "garrison" on
//!                    reserve (hoi4_template_edit priority only; no battalion changes), and the
//!                    entry's `priority` (else regular) returns at peace; `manage.war_priority`
//!                    false turns this off. Checked once a game day while an entry waits
//!   occupation       {"default_law"|"law","when"?,"ladder"?,"harsher_at_resistance":50,
//!                    "milder_at_compliance":40,"milder_below_resistance":25,"mildest"?,"harshest"?,
//!                    "change_days":30,"per_state"?} every 7 game days from hoi4_occupation (states
//!                    occupied by us or owned and resisting): the default law is set once when `when`
//!                    holds, then a state at resistance >= 50 moves the law one step harsher on
//!                    `ladder` (default: the game's Default Law list, mild to harsh), and every state at
//!                    compliance >= 40 with resistance < 25 one step milder, at most once per
//!                    `change_days`, through hoi4_occupation_law. `per_state` is noted, not applied
//!                    (the tool sets the default law only); a garrison template action does not exist
//!   peace            {"take":[state ids by rank],"puppet"?:[state ids],"liberate"?:[state ids],
//!                    "max_cost"?,"max_rounds":10} when the conference opens: hoi4_peace_options, then
//!                    one state per action through hoi4_peace_take (puppet:true / release:true for the
//!                    other lists), hoi4_peace_submit when the round's points are spent, hoi4_peace_exit
//!                    when nothing is left (or after 5 idle ticks with states still wanted, or after
//!                    `max_rounds`). Without `peace` the runner stops as before and wakes the player;
//!                    20 idle ticks, or an order the ledger refuses, stop and wake too. A tag in
//!                    `puppet` is noted (the tool takes state ids)
//!   auto_import      {"budget":N,"enabled"?:true} (or N, or true): every 7 game days a resource deficit
//!                    on a line (hoi4_production_shortages; lines matching production_priority when it
//!                    is set; else hoi4_resources shortages) turns automatic importing on with the
//!                    budget through hoi4_auto_import, once per budget; a deficit that outlasts
//!                    `manage.deficit_wake_days` (14) with it on, or any deficit without auto_import
//!                    (or enabled:false), wakes the player
//!   air_plan         [{"wing"?|"airframe"+"version"?,"region","mission","base_state"?|"base_region"?,
//!                    "when"?,"switch_when"?,"then_region"?,"then_mission"?|"fallback_mission"?}] every 3
//!                    game days from hoi4_air: each matching wing (exact `wing`, else every wing of that
//!                    aircraft type and variant) is rebased (hoi4_air_rebase: to `base_state`, or to the
//!                    base of `base_region` with the most free room when its own base is outside that
//!                    region or over capacity) and then flies `mission` over `region` (hoi4_air_mission);
//!                    once `switch_when` holds, over `then_region` with `then_mission`. Wings the air plan
//!                    names are left out of `air.war_missions`
//!   manage           thresholds of the standing rules (all optional; the rules of this object run
//!                    only when the plan has it, the resource rule also with `auto_import`, the alerts
//!                    and the ledger always):
//!                    "off":["alerts","ledger","manpower","stability","equipment_fill","resources",
//!                    "stockpile","trucks"] switches rules off; "wake_repeat_days":7 one wake per reason;
//!                    "alerts_ignore":[alert names]; "alert_persist_days":14;
//!                    "manpower_months":2 free manpower below that many months of losses (hoi4_war_stats)
//!                    takes the next enabled law of "conscription_laws" in government slot 0 (the next
//!                    listed after the current law, else the first listed one harsher than it on the
//!                    vanilla ladder); with no list (the default) it never raises conscription, which
//!                    costs factory output, and wakes the player instead; "stability_floor":0.5 wakes below it;
//!                    "equipment_fill_floor":0.8 an army whose divisions' mean equipment fill
//!                    (hoi4_supply) is below it wakes the player and nudges `disband`;
//!                    "stockpile_days_critical":7 / "stockpile_days_warning":30 and "trucks_ratio":1.5
//!                    are reported unknown (no tool reads the stockpile); "deficit_wake_days":14.
//!                    These keys act whether or not the plan has `manage` (defaults shown):
//!                    "rebalance_step":3 idle military factories per action to a priority line (tick
//!                    step 6b, `production_priority` in plan_run.rs); "day0_production":false skips the
//!                    day-0 assignment of free military factories to the kept starting lines;
//!                    "war_priority":false (templates above); "war_civs":true keeps industrial_complex
//!                    jobs of `construction` at war (plan_run.rs); "mature_line_days":30
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
use super::{item_id, list, strings, text, GameHour, State, Tick, Tools, DAY};
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
    /// (game hour, own casualties so far) samples for the manpower rule.
    losses: Vec<(GameHour, f64)>,
    /// When the current resource deficit was first seen.
    deficit_since: Option<GameHour>,
    /// Alert name -> the game hour it was first seen in the current unbroken run of days.
    alert_since: HashMap<String, GameHour>,
    /// The next priority line in turn for idle factories when no shortage is known.
    feed_turn: usize,
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

fn note(state: &mut State, rule: &str, line: impl Into<String>) {
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
    if let Some(done) = occupation_step(state, tools, plan, lease, now)? {
        return Ok(Some(done));
    }
    if let Some(done) = resources_step(state, tools, plan, lease, now)? {
        return Ok(Some(done));
    }
    if let Some(done) = air_plan_step(state, tools, plan, lease, now)? {
        return Ok(Some(done));
    }
    if let Some(done) = country_rules(state, tools, plan, lease, now)? {
        return Ok(Some(done));
    }
    equipment_rule(state, tools, plan, now);
    unread_rules(state, plan);
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
            for (archetype, n) in &guards {
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
        if !entry["regimental_support"].is_null() || !entry["regimental"].is_null() {
            extra.push("regimental support row skipped (no tool fills it yet)".into());
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

/// The game's Default Law list, mild to harsh (as hoi4_occupation_law lists it); a mod's laws
/// go in `occupation.ladder`.
const OCCUPATION_LADDER: [&str; 9] = [
    "no_garrison",
    "foreign_civilian_oversight",
    "local_police_force_garrison",
    "secret_police_oversight",
    "military_governor_occupation",
    "martial_law_occupation",
    "forced_labor_occupation",
    "harsh_quotas_occupation",
    "brutally_oppressive_occupation",
];

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
    if !occ["per_state"].is_null() {
        extra.push("per_state not applied (hoi4_occupation_law sets the default law only)");
    }
    let current = read["player_occupation_law"]["key"]
        .as_str()
        .unwrap_or("")
        .to_string();
    let mut ladder = strings(occ, "ladder");
    if ladder.is_empty() {
        ladder = OCCUPATION_LADDER.map(String::from).to_vec();
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
    let harsher_at = occ["harsher_at_resistance"].as_f64().unwrap_or(50.0);
    let milder_at = occ["milder_at_compliance"].as_f64().unwrap_or(40.0);
    let milder_below = occ["milder_below_resistance"].as_f64().unwrap_or(25.0);
    let max_res = hottest.map_or(0.0, resistance);
    let calm = states.iter().all(|s| {
        s["compliance"].as_f64().unwrap_or(0.0) >= milder_at && resistance(s) < milder_below
    });
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
            match v.as_i64() {
                Some(id) if !wanted.iter().any(|w| w.0 == id) => wanted.push((id, kind)),
                Some(_) => {}
                None => tags.push(format!("{key} {}", text(v))),
            }
        }
    }
    if !tags.is_empty() {
        note(
            state,
            "peace",
            format!(
                "not taken (hoi4_peace_take takes state ids): {}",
                tags.join(", ")
            ),
        );
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
    if let Ok(read) = tools.call("hoi4_production_shortages", &json!({})) {
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
    let cfg = &plan["auto_import"];
    let (allowed, budget) = match cfg {
        Value::Bool(on) => (*on, None),
        Value::Number(n) => (true, n.as_i64()),
        Value::Object(_) => (cfg["enabled"] != json!(false), cfg["budget"].as_i64()),
        _ => (false, None),
    };
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
    let key = format!("imp:on:{shown}");
    if !state.done.contains_key(&key) {
        let mut args = json!({"enabled": true});
        if let Some(b) = budget {
            args["budget"] = json!(b);
        }
        let (ok, why) = state.act(tools, lease, "hoi4_auto_import", args, "ai")?;
        if ok {
            state.done.insert(key, now);
        }
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

/// The state to rebase a wing to, if it must move: `base_state`, or the base of `base_region`
/// with the most free room when the wing's own base is outside that region or over capacity.
fn rebase_target(entry: &Value, wing: &Value, bases: &[Value]) -> Option<i64> {
    if wing["transfer_to_base_id"].as_i64().is_some() {
        return None;
    }
    let here = wing["air_base"]["state_id"].as_i64();
    if let Some(target) = entry["base_state"].as_i64() {
        return (here != Some(target)).then_some(target);
    }
    let want = &entry["base_region"];
    if want.is_null() {
        return None;
    }
    let own = bases
        .iter()
        .find(|b| b["base_id"] == wing["air_base"]["base_id"]);
    if own
        .is_some_and(|b| region_matches(want, &b["region"]) && b["free"].as_i64().unwrap_or(0) >= 0)
    {
        return None;
    }
    let planes = wing["aircraft"].as_i64().unwrap_or(0);
    bases
        .iter()
        .filter(|b| region_matches(want, &b["region"]) && b["state_id"].as_i64() != here)
        .filter(|b| b["free"].as_i64().unwrap_or(0) >= planes)
        .max_by_key(|b| b["free"].as_i64().unwrap_or(0))
        .and_then(|b| b["state_id"].as_i64())
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
    let mut flying = 0;
    for entry in entries {
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
            if let Some(base) = rebase_target(entry, wing, list(&air, "air_bases")) {
                let (ok, why) = state.act(
                    tools,
                    lease,
                    "hoi4_air_rebase",
                    json!({"wing": name, "state_id": base}),
                    "ab",
                )?;
                if !ok {
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
    }
    state.skip(now, "mair", "x", 3 * DAY);
    note(
        state,
        "air_plan",
        format!("{flying} wing(s) on their planned mission"),
    );
    Ok(None)
}

// ---------------------------------------------------------------------------------------------
// Rules 13 and 15: manpower and stability (hoi4_country_status), every 7 game days.

/// The vanilla conscription laws, lightest first (`manage.conscription_laws` replaces them).
const CONSCRIPTION_LADDER: [&str; 7] = [
    "disarmed_nation",
    "volunteer_only",
    "limited_conscription",
    "extensive_conscription",
    "service_by_requirement",
    "all_adults_serve",
    "scraping_the_barrel",
];

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
            .and_then(|s| s["stability"]["value"].as_f64())
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
        .and_then(|s| s["manpower"]["value"].as_f64())
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
    // The next listed law after the current one; when the current law is not listed, the first
    // listed law harsher than it on the vanilla ladder (any listed law when either is unknown).
    let rank = |l: &str| CONSCRIPTION_LADDER.iter().position(|v| *v == l);
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
            )
        })
        .collect();
    let idle = total - military.iter().map(|m| m.2).sum::<i64>();
    Ok((military, idle))
}

/// Equipment the fielded divisions lack per archetype (hoi4_supply: need minus held). This is
/// the stand-in for days of stockpile cover: no tool reads the stockpile.
fn shortfall(supply: Option<&Value>) -> HashMap<String, f64> {
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

/// One military line: (entry index, equipment, requested factories).
type MilLine = (i64, String, i64);

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
    let read = supply(state, tools, now);
    let short = shortfall(read.as_ref());
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
    let Some(((index, equipment, factories), why)) = feed_choice(state, tools, plan, now, military)
    else {
        return Ok(None);
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
        let Some((index, have, current)) = military
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

/// Rules 11 and 21 need the equipment stockpile, which no tool reads: they report unknown.
fn unread_rules(state: &mut State, plan: &Value) {
    let m = &plan["manage"];
    if !m.is_object() {
        return;
    }
    if !off(plan, "stockpile") {
        let critical = m["stockpile_days_critical"].as_i64().unwrap_or(7);
        let warning = m["stockpile_days_warning"].as_i64().unwrap_or(30);
        note(state, "stockpile_days", format!("unknown (critical {critical}, warning {warning} days): no tool reads the equipment stockpile"));
    }
    if !off(plan, "trucks") {
        let ratio = m["trucks_ratio"].as_f64().unwrap_or(1.5);
        note(state, "trucks", format!("unknown (wanted {ratio}x): no tool reads the truck stockpile (hoi4_supply lists trucks_and_trains_in_stock unavailable)"));
    }
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
        assert!(state.manage.notes["templates"].contains("regimental support row skipped"));
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
        json!({"player_occupation_law": {"key": law},
            "states": states.iter().map(|(id, r, c)| json!({"state_id": id, "role": "occupied_by_player",
                "resistance": r, "compliance": c})).collect::<Vec<_>>()})
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
        assert!(state.manage.notes["occupation"].contains("per_state not applied"));
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
        assert!(state.manage.notes["peace"].contains("puppet ETH"));
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

    #[test]
    fn manpower_below_two_months_of_losses_takes_only_a_listed_law() {
        let tools = fake(vec![
            (
                "hoi4_country_status",
                json!({"manpower": {"value": 50000.0}, "stability": {"value": 0.7}}),
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
    fn low_stability_and_low_equipment_fill_wake_and_unread_rules_say_unknown() {
        let tools = fake(vec![
            ("hoi4_country_status", json!({"stability": {"value": 0.42}})),
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
        assert!(manage["trucks"].as_str().unwrap().starts_with("unknown"));
        assert!(manage["stockpile_days"]
            .as_str()
            .unwrap()
            .starts_with("unknown"));
        // no repeat inside the cadence and the wake window
        step(&mut state, &tools, &plan, "l", START + 3 * DAY).unwrap();
        assert!(wakes(&mut state).is_empty());
        assert_eq!(reads(&tools, "hoi4_country_status"), 1);
    }
}
