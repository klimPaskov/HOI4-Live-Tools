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
//!                    `manage.deficit_wake_days` (14) with it on raises the budget (an input of a
//!                    production_priority line at once, every 7 days), and a deficit still there
//!                    after the raise, or any deficit without auto_import
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
//!                    "overdrawn_share":0.5 of them, until their supply need covers the shortfall)
//!                    march to a province with room, in their own army (no new army); a province
//!                    takes only what its headroom holds less the groups already on their way, a
//!                    group is remembered "overdrawn_rest_days":10, a province whose order failed
//!                    is not chosen for 10 days; never in a defender's hold (war.posture "defend",
//!                    no axes): it stands.
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

/// A template's equipment need per division and the variant of each archetype, with the hour read.
type TemplateNeed = (GameHour, HashMap<String, f64>, HashMap<String, String>);

/// What the runner keeps for these rules besides `done` and the skip timers.
#[derive(Default)]
pub(super) struct Memory {
    /// Wake alarms raised since the last run result.
    alarms: Vec<Value>,
    /// The last status line of each rule (the run result's `manage`).
    notes: BTreeMap<String, String>,
    /// The game hour each note was last written (an alert quotes only a current one).
    notes_at: HashMap<String, GameHour>,
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
    pub(super) alert_since: HashMap<String, GameHour>,
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
    /// Reserve plane variants no air plan entry takes: key -> (first seen, amount then).
    air_reserve_seen: HashMap<String, (GameHour, f64)>,
    /// Officer corps picks the game refused, by entry.
    ofc_refused: HashMap<String, u32>,
    /// officer corps picks moved to the branch's next slot after a spirit_unknown
    ofc_slot_shift: HashMap<String, usize>,
    /// (game hour, fuel stock) samples of the fuel rule, the last 30 game days.
    fuel_samples: Vec<(GameHour, f64)>,
    /// Since when free convoys have been below `manage.convoys_free` of the stock.
    convoys_low_since: Option<GameHour>,
    /// Civilian factories (hoi4_economy) at the last auto-import budget check.
    import_civs: Option<i64>,
    /// Why the last raise of the auto-import budget could not happen (the wake says it).
    import_capped: Option<String>,
    /// Production balance: each template's equipment need per division (hoi4_template_get),
    /// read at most every 30 game days.
    tpl_need: HashMap<String, TemplateNeed>,
    /// Production balance: when a line last gave factories away (PR09: once per 30 days).
    pulled: HashMap<String, GameHour>,
    /// Divisions sent out of overdrawn provinces and where they go.
    spread: Vec<Spread>,
}

/// Divisions sent out of an overdrawn province (supply.split_overdrawn), in their own army.
#[derive(Clone)]
struct Spread {
    divisions: Vec<String>,
    province: i64,
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
    let at = state.last_hour;
    state.manage.notes_at.insert(rule.to_string(), at);
}

/// The war is a defender's hold: `war.posture` "defend" and no axes in force.
fn defending(plan: &Value) -> bool {
    plan["war"]["posture"] == json!("defend")
        && plan["war"]["axes"].as_array().is_none_or(|a| a.is_empty())
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
pub(super) fn supply(state: &mut State, tools: &dyn Tools, now: GameHour) -> Option<Value> {
    if let Some((at, read)) = &state.manage.supply {
        if now - *at < DAY {
            return Some(read.clone());
        }
    }
    let read = tools.call("hoi4_supply", &json!({})).ok()?;
    state.manage.supply = Some((now, read.clone()));
    Some(read)
}

/// Air wings whose plane family no own production line makes ("type (wings: a, b)"), from
/// hoi4_air and hoi4_production_lines; empty when either read fails.
fn unproduced_air_gaps(tools: &dyn Tools) -> Vec<String> {
    let (Ok(air), Ok(lines)) = (
        tools.call("hoi4_air", &json!({})),
        tools.call("hoi4_production_lines", &json!({"limit": 128})),
    ) else {
        return Vec::new();
    };
    let made: BTreeSet<String> = list(&lines, "lines")
        .iter()
        .map(|l| family(text(&l["equipment_id"])).to_string())
        .collect();
    let mut gaps: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for wing in list(&air, "wings") {
        let kind = text(&wing["aircraft_type"]);
        if kind.is_empty() || made.contains(family(kind)) {
            continue;
        }
        gaps.entry(family(kind).to_string())
            .or_default()
            .push(text(&wing["wing"]).to_string());
    }
    gaps.into_iter()
        .map(|(kind, wings)| format!("{kind} (wings: {})", wings.join(", ")))
        .collect()
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

/// The manage rules as lanes of the pass (plan_run.rs `lanes`), in their old chain order. The
/// first, `manage`, takes the reads every rule leans on (the failure ledger's wake, the game's
/// alert row) and never acts.
pub(super) const LANES: [super::Lane; 19] = [
    ("manage", |state, tools, plan, _, now| {
        // the tick only gets here when no conference is open
        state.manage.peace = None;
        ledger_wake(state, plan, now);
        alerts(state, tools, plan, now);
        Ok(None)
    }),
    ("templates", templates_step),
    ("template_cleanup", template_cleanup_step),
    ("occupation", occupation_step),
    ("resources", resources_step),
    ("import_budget", import_budget_rule),
    ("fuel", fuel_rule),
    ("convoys", convoy_rule),
    ("air_plan", air_plan_step),
    ("officer_corps", officer_corps_step),
    ("air_reserve", air_reserve_step),
    ("air_idle", air_idle_step),
    ("country", country_rules),
    // the stockpile rule first: a shortage the front feels (or a cover target) is the war's
    ("stockpile", stockpile_rule),
    ("balance", balance_rule),
    ("naval_queue", naval_queue_rule),
    ("dockyards", dockyard_rule),
    ("overdrawn", overdrawn_rule),
    ("equipment", |state, tools, plan, _, now| {
        equipment_rule(state, tools, plan, now);
        Ok(None)
    }),
];

/// The manage rules in one chain (the first that acts ends it): the tests' view of the old
/// domain; the executor offers each rule as its own lane.
#[cfg_attr(not(test), allow(dead_code))]
pub fn step(
    state: &mut State,
    tools: &dyn Tools,
    plan: &Value,
    lease: &str,
    now: GameHour,
) -> Result<Option<String>> {
    // a direct call stands for a tick at `now` (the ledger counts game hours)
    state.clock = now;
    for (_, rule) in LANES {
        if let Some(done) = rule(state, tools, plan, lease, now)? {
            return Ok(Some(done));
        }
    }
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
        // a vacant political advisor seat: the plan's dated picks, then the standing order's
        // vacancy fill (politics_auto.fill_vacant) owns it
        "modify_government" => Route::Step(
            "politics",
            &["pol", "pauto"],
            &["politics", "politics_auto"],
        ),
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
        "free_dockyards" => Route::Step("dockyards", &["mdock"], &["production", "manage"]),
        // a timed decision or mission (the decisions step answers what the plan wants and
        // notes what it leaves, which is the reason); an event times out into its first option
        "events_or_decisions_timeout" => Route::Step(
            "decisions",
            &[],
            &["decisions", "decision_rules", "auto_events", "event_rules"],
        ),
        // the officer corps view: its spirits (plan officer_corps, paid in branch experience)
        // and the military high command seats (politics_auto)
        "modify_officer_corps" => Route::Step(
            "officer_corps",
            &["mofc", "pauto"],
            &["officer_corps", "politics_auto"],
        ),
        "mio_traits" => Route::Step("mio", &[], &["mio"]),
        "research_slot_available" => Route::Info,
        "free_civilian_factories" => {
            Route::Step("construction", &["con", "confull"], &["construction"])
        }
        "free_military_factories" => Route::Step(
            "production",
            &["mbal", "prodset"],
            &["production", "production_priority"],
        ),
        // the naval queue rule removes hulls the plan does not name (PR06/PR07); a line the plan
        // names is its choice, noted by the rule with its reason
        "outdated_equipment_in_production" => Route::Step("naval_queue", &["mnq"], &["production"]),
        "operative_recruitable" => Route::Step("intelligence", &[], &["intelligence"]),
        "special_project_available" | "scientist_unassigned" => {
            Route::Step("special_projects", &[], &["special_projects"])
        }
        "admiral_assignable" => Route::Step("navy", &["nadm"], &["navy"]),
        _ => Route::Wake("an alert the executor does not know"),
    }
}

/// "; <key>: <note>" for an alert: a note kept under the alert's own name, else the owning
/// step's, wherever that step keeps it (manage, affairs or military rules: g17a, 17 Nov 2026,
/// the navy and special-project wakes carried no reason because only manage notes were read).
fn step_reason(state: &State, alert: &str, step: &str) -> String {
    // the route's plan keys too: a step that writes no note of its own (politics) leaves the
    // reason to the rule that owns the rest (politics_auto: g17e, modify_government 290 days)
    let keys = match route(alert) {
        Route::Step(_, _, keys) => keys,
        _ => &[][..],
    };
    [alert, step]
        .iter()
        .chain(keys.iter())
        .filter(|k| !k.is_empty())
        .find_map(|key| {
            // a manage note older than a month (the slowest rule cadence) is no reason for an alert open today
            // (g17i, 18 Nov 2026: free_military_factories quoted the day-0 "no free military
            // factory" for 361 days)
            let current = state
                .manage
                .notes_at
                .get(*key)
                .is_none_or(|at| state.last_hour - *at <= 31 * DAY);
            state
                .manage
                .notes
                .get(*key)
                .filter(|_| current)
                .map(String::as_str)
                .or_else(|| super::plan_affairs::note_of(state, key))
                .or_else(|| super::plan_military::note_of(state, key))
                .map(|n| format!("; {key}: {n}"))
        })
        .unwrap_or_default()
}

/// The game's open actionable alerts as omissions for the player's wake (`hoi4_wait_change`
/// `now.omissions`): the item, how many game days it has been open, the game's own todo, the
/// executor rule that owns it, that rule's dated note, and whether the note says there is
/// nothing to do while the game says there is.
fn omission_rows(state: &State, read: &Value, ignore: &[String], now: GameHour) -> Vec<Value> {
    let mut rows: Vec<Value> = Vec::new();
    for alert in list(read, "alerts") {
        let frame = alert["frame"].as_i64().unwrap_or(-1);
        let name = alert["alert"]
            .as_str()
            .map_or_else(|| format!("frame_{frame}"), String::from);
        if rows.iter().any(|r| r["item"] == json!(name)) {
            continue;
        }
        let owner = match route(&name) {
            Route::Info => continue,
            Route::Wake(_) => "player",
            Route::Step(step, _, _) => step,
        };
        let owner = if ignore.contains(&name) {
            "player (manage.alerts_ignore)"
        } else {
            owner
        };
        let since = state.manage.alert_since.get(&name).copied().unwrap_or(now);
        let reason = step_reason(state, &name, owner);
        let note = reason.trim_start_matches("; ").to_string();
        let lower = note.to_lowercase();
        let contradicts = ["no free", "none ", "nothing", "no idle", "no factory"]
            .iter()
            .any(|w| lower.contains(w));
        rows.push(json!({"item": name, "days_open": (now - since) / DAY,
            "todo": alert["todo"], "owner": owner,
            "owner_note": if note.is_empty() { Value::Null } else { json!(note) },
            "note_contradicts_read": contradicts}));
    }
    rows.sort_by_key(|r| -r["days_open"].as_i64().unwrap_or(0));
    rows
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
        if !seen.insert(name.clone()) {
            continue;
        }
        let since = *state.manage.alert_since.entry(name.clone()).or_insert(now);
        let evidence =
            json!({"alert": name, "frame": frame, "todo": alert["todo"], "seen_since_hour": since});
        let reason = format!("alert:{name}");
        if ignore.contains(&name) {
            // The plan leaves it to someone (its alerts_ignore_why); it was dropped unseen while
            // nothing did it (g17a, 17 Nov 2026: mio_traits open 465 days, never woken). Once
            // per unbroken spell past the persist days the player hears why it is still open.
            lines.push(format!("{name}: ignored by the plan"));
            let once = format!("alertignored:{name}:{since}");
            if now - since >= persist * DAY && !state.done.contains_key(&once) {
                state.done.insert(once, now);
                let why = text(&plan["manage"]["alerts_ignore_why"][&name]).to_string();
                let step = match route(&name) {
                    Route::Step(step, _, _) => step,
                    _ => "",
                };
                wake(
                    state,
                    plan,
                    now,
                    3,
                    &reason,
                    format!(
                        "game alert {name} has stayed {} game days; the plan ignores it{}{}",
                        (now - since) / DAY,
                        if why.is_empty() {
                            String::new()
                        } else {
                            format!(" ({why})")
                        },
                        step_reason(state, &name, step)
                    ),
                    evidence,
                );
            }
            continue;
        }
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
                        // pp5 (17 Nov 2026), the alert's own tooltip: "Equipment required for Air
                        // Reinforcements: Transport Plane, Naval Bomber": start wings whose plane
                        // no line makes. Those are read (hoi4_air wings against the lines).
                        let air = unproduced_air_gaps(tools);
                        let why = if gaps.is_empty() && !air.is_empty() {
                            format!(
                                "air wings need planes no line makes: {}; the game asks for them \
                                 as air reinforcements. Add a line for the type the plan uses, or \
                                 put the alert in manage.alerts_ignore with alerts_ignore_why \
                                 naming the wings the plan keeps understrength",
                                air.join(", ")
                            )
                        } else if gaps.is_empty() {
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
                            "game alert {name} has stayed {} game days although the {step} step was nudged{}",
                            (now - since) / DAY,
                            // the owning step's last status line is the written reason (why it
                            // waits, what it deliberately leaves): pr2 kept five alerts open for
                            // 467-753 days with none
                            step_reason(state, &name, step)
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
    let omissions = omission_rows(state, &read, &ignore, now);
    crate::watch::feed().set_omissions(Value::Array(omissions));
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
    // Several entries of one template (one per deploy state) add up: entry i trains while the
    // template's queued lines are below the queues of the entries up to i whose `when` holds.
    // The game counts queued lines per template, so comparing each entry with its own queue let
    // only the largest act (pr3b, 17 Nov 2026: eight Infanterie-Division entries meant 29 lines,
    // 5 ran, 55 divisions on 1938.1.1 against the AI's 66).
    let mut queued_before: HashMap<String, u64> = HashMap::new();
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
        if when_holds(state, tools, &tr["when"], now) != Truth::True {
            continue;
        }
        // an entry whose deploy state the game refused rests (and adds no queue meanwhile)
        let place = format!("{template}@{}", tr["deploy_state"]);
        if state.skipped(now, "traindeploy", &place) {
            continue;
        }
        let prior = queued_before.entry(template.clone()).or_insert(0);
        let target = *prior + queue;
        *prior = target;
        if have >= target {
            continue;
        }
        let mut count = tr["batch"].as_u64().unwrap_or(2).min(target - have);
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
        // Training is paced by what production can equip: every archetype a fielded division of
        // the template needs and the plan does not guard waits for one batch's need in stock
        // (g17a/g17b, 17 Nov 2026: the plan guarded rifles only while support equipment, the
        // binding one, stood at 0 days of cover).
        if let Some((archetype, held, need)) =
            batch_short(state, tools, now, &template, count, &guards)
        {
            note(
                state,
                "training",
                format!(
                    "{template}: waits, {archetype} stockpile {held:.0} below one batch's need \
                     {need:.0} (training is paced by what production can equip)"
                ),
            );
            state.skip(now, "train", &template, DAY);
            continue 'entries;
        }
        let (ok, why) = state.act(
            tools,
            lease,
            "hoi4_train",
            json!({"template": template, "count": count, "deploy_state_id": tr["deploy_state"]}),
            "tr",
        )?;
        if !ok && why.contains("No location was set") {
            // The game does not let this template deploy in that state (g17a, 17 Nov 2026):
            // the same entry would be refused every time, so it rests with the reason.
            state.skip(now, "traindeploy", &place, 60 * DAY);
            note(
                state,
                "training",
                format!(
                    "{template}: the game refuses deploy state {} for it; that entry rests 60 days: {why}",
                    tr["deploy_state"]
                ),
            );
            return Ok(Some(format!("train {count} {template}: {ok} {why}")));
        }
        if !ok {
            state.skip(now, "train", &template, 2 * DAY);
            note(
                state,
                "training",
                format!("{template}: not queued, tried again in 2 days: {why}"),
            );
            return Ok(Some(format!("train {count} {template}: {ok} {why}")));
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

/// The first archetype a fielded division of `template` needs, the plan's `guards` do not name,
/// and the stockpile holds less of than `count` divisions need: (archetype, held, need). None
/// when nothing is short, or when the template has no fielded division or the reads fail.
fn batch_short(
    state: &mut State,
    tools: &dyn Tools,
    now: GameHour,
    template: &str,
    count: u64,
    guards: &[(String, u64)],
) -> Option<(String, f64, f64)> {
    let read = supply(state, tools, now)?;
    let division = list(&read, "divisions")
        .iter()
        .find(|d| text(&d["template"]) == template && !list(d, "equipment").is_empty())?;
    let needs: Vec<(String, f64)> = list(division, "equipment")
        .iter()
        .filter_map(|e| {
            let need = e["need"].as_f64().filter(|n| *n > 0.0)?;
            Some((text(&e["archetype"]).to_string(), need))
        })
        .filter(|(a, _)| {
            !guards
                .iter()
                .any(|(g, _)| a.starts_with(g.as_str()) || g.starts_with(a.as_str()))
        })
        .collect();
    if needs.is_empty() {
        return None;
    }
    let stock = stockpile(state, tools, now)?;
    // the batch draws its equipment while it trains: what the lines make in the first
    // TRAINING_COVER_DAYS counts with the stock (sk27a, 18 Nov 2026: nothing trained for 16 days
    // while support equipment stood at 54 of 60 with 2 a day coming)
    needs.into_iter().find_map(|(archetype, per)| {
        let row = list(&stock, "equipment")
            .iter()
            .find(|r| text(&r["archetype"]) == archetype)?;
        let held = row["stockpile"].as_f64()?;
        let made = row["produced_per_day"]["units"]
            .as_f64()
            .or_else(|| row["produced_per_day"].as_f64())
            .unwrap_or(0.0)
            .max(0.0);
        let need = per * count as f64;
        (held + made * TRAINING_COVER_DAYS < need).then_some((archetype, held, need))
    })
}

/// Game days of line output a training batch may count on besides the stock.
const TRAINING_COVER_DAYS: f64 = 30.0;

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

/// Equipment per piece of each battalion and company key, from every own template's units
/// (hoi4_template_get): {unit: {archetype: pieces}}.
fn unit_needs(read: &Value) -> HashMap<String, HashMap<String, f64>> {
    let mut out: HashMap<String, HashMap<String, f64>> = HashMap::new();
    for t in list(read, "templates") {
        for u in list(t, "units") {
            let key = text(&u["unit"]).to_string();
            if key.is_empty() || out.contains_key(&key) {
                continue;
            }
            let need = u["equipment_need"]
                .as_object()
                .map(|m| {
                    m.iter()
                        .filter_map(|(a, n)| Some((a.clone(), n.as_f64()?)))
                        .collect()
                })
                .unwrap_or_default();
            out.insert(key, need);
        }
    }
    out
}

/// Why an edit of `target` to `regiments` + `support` cannot be equipped from the stockpile
/// for every fielded division of the template ("artillery_equipment 40 < 912 for 38 fielded
/// divisions (+24 each)"), None when it can, when no division is fielded, or when a battalion's
/// need is not known from any own template (the tool's own checks then decide).
fn edit_stock_short(
    tools: &dyn Tools,
    target: &str,
    regiments: &Value,
    support: &[Value],
) -> Option<String> {
    let read = tools.call("hoi4_template_get", &json!({})).ok()?;
    let row = list(&read, "templates")
        .iter()
        .find(|t| text(&t["template"]) == target)?;
    let fielded = row["divisions_using_template"].as_u64().unwrap_or(0);
    if fielded == 0 {
        return None;
    }
    let needs = unit_needs(&read);
    let mut now_need: HashMap<String, f64> = HashMap::new();
    for u in list(row, "units") {
        for (a, n) in u["equipment_need"].as_object().into_iter().flatten() {
            *now_need.entry(a.clone()).or_insert(0.0) += n.as_f64().unwrap_or(0.0);
        }
    }
    let keys: Vec<&str> = regiments
        .as_array()
        .into_iter()
        .flatten()
        .flat_map(|c| c.as_array().into_iter().flatten())
        .chain(support.iter())
        .filter_map(Value::as_str)
        .collect();
    let mut new_need: HashMap<String, f64> = HashMap::new();
    for k in keys {
        for (a, n) in needs.get(k)? {
            *new_need.entry(a.clone()).or_insert(0.0) += n;
        }
    }
    let mut added: Vec<(String, f64)> = new_need
        .iter()
        .map(|(a, n)| (a.clone(), n - now_need.get(a).copied().unwrap_or(0.0)))
        .filter(|(_, d)| *d > 0.0)
        .collect();
    if added.is_empty() {
        return None;
    }
    added.sort_by(|a, b| a.0.cmp(&b.0));
    let archetypes: Vec<&str> = added.iter().map(|(a, _)| a.as_str()).collect();
    let stock = tools
        .call("hoi4_stockpile", &json!({"equipment": archetypes}))
        .ok()?;
    added.into_iter().find_map(|(a, per)| {
        let row = list(&stock, "equipment")
            .iter()
            .find(|r| text(&r["archetype"]) == a)?;
        let cover = row["cover_after_shortfall"]
            .as_f64()
            .or_else(|| row["stockpile"].as_f64())?;
        let need = per * fielded as f64;
        (cover < need).then(|| {
            format!("{a} {cover:.0} < {need:.0} for {fielded} fielded divisions (+{per:.0} each)")
        })
    })
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
            // the spends the plan's xp_budget lists before this edit are paid first (plan_xp.rs)
            let who = item_id(entry).map_or(super::plan_xp::Spender::Unlisted, |id| {
                super::plan_xp::Spender::Ref(format!("template:{id}"))
            });
            let (before, names) = super::plan_xp::ahead(state, tools, plan, "army", &who, now);
            // an unreadable XP leaves the check to the tool's own insufficient_experience
            if let Some(Some(xp)) = army_xp {
                if xp < need + before {
                    let held = if names.is_empty() {
                        String::new()
                    } else {
                        format!(" ({before:.0} held for {} first)", names.join(", "))
                    };
                    waits.push(format!("{name}: army XP {xp} < {need}{held}"));
                    break;
                }
            }
        }
        // an edit that adds equipment the stock cannot give every fielded division of the
        // template waits (value judgement: the divisions lose strength on every changed
        // battalion and training, which needs every equipment, stops; g17i, 18 Nov 2026: the
        // infantry took two line artillery battalions with 3 artillery factories, artillery
        // stock 0-10 for a year, training nearly stopped). `stock_guard: false` opts out.
        if let (Some(target), Some(r)) = (edit, regiments.as_ref()) {
            if entry["stock_guard"] != json!(false) {
                if let Some(short) = edit_stock_short(tools, target, r, &support) {
                    waits.push(format!("{name}: edit waits for stock: {short}"));
                    state.skip(now, "tpl", &key, 7 * DAY);
                    continue;
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
            // A slot the designer does not open (a fifth row before a doctrine milestone) or a
            // unit not yet researched does not change in a week: rest 90 days and say so (gr1-gr6:
            // Panzer-Division 36 refused slot_locked 9 times). Lint: templates.column_height.
            let days = match super::plan_military::code(&why) {
                "slot_locked" | "unit_not_unlocked" | "template_too_wide" => {
                    extra.push(format!(
                        "refused {why}: rests 90 days, the plan must change it"
                    ));
                    90
                }
                _ => 7,
            };
            state.skip(now, "tpl", &key, days * DAY);
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
            // a new round: the points changed, so a take refused for its price is a new order
            let stale: Vec<String> = state
                .failures
                .keys()
                .filter(|k| k.starts_with("hoi4_peace_take "))
                .cloned()
                .collect();
            for signature in stale {
                state.failures.remove(&signature);
                state.failed_run.remove(&signature);
                state.failed_wall.remove(&signature);
                state.failed_cost.remove(&signature);
            }
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
    // an input of a production_priority line: the budget is raised at once, not after
    // deficit_wake_days (e8, 16 Nov 2026: the two rifle lines ran short of steel ~70 game days
    // on a budget of one while every division stood at half its rifles)
    let mut urgent = false;
    // the country's deficit of each short resource (hoi4_resources): the units a raise must buy,
    // for the resources the lines draw too (a priority line short of 30 rubber needs four
    // factories, not one: one civilian factory buys about 8 units)
    let mut deficits: BTreeMap<String, f64> = BTreeMap::new();
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
                    if r["shortage"]["short"] == json!(true) {
                        deficits.insert(
                            name.clone(),
                            r["shortage"]["deficit"].as_f64().unwrap_or(0.0),
                        );
                    }
                    if r["shortage"]["short"] == json!(true) && !drawn.contains(name) {
                        let deficit = r["shortage"]["deficit"].as_f64().unwrap_or(0.0);
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
            urgent |= !priority.is_empty();
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
    units += short.iter().filter_map(|r| deficits.get(r)).sum::<f64>();
    if short.is_empty() {
        state.manage.deficit_since = None;
        state.manage.import_capped = None;
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
    let lasting = now - since >= days * DAY;
    if urgent || lasting {
        if let Some(done) =
            raise_import_budget(state, tools, plan, lease, now, units, &deficit, urgent)?
        {
            return Ok(Some(done));
        }
    }
    if lasting {
        let days = (now - since) / DAY;
        let (message, evidence) = match state.manage.import_capped.clone() {
            Some(cap) => {
                let mut evidence = evidence;
                evidence["import_capped"] = json!(cap);
                (
                    format!(
                        "resource deficit ({deficit}) has lasted {days} game days and imports cannot \
                         grow: {cap}; free civilian factories (an economy law with fewer consumer \
                         goods) or move the lines off the short resource"
                    ),
                    evidence,
                )
            }
            None => (
                format!("resource deficit ({deficit}) has lasted {days} game days with auto-import on (budget {shown})"),
                evidence,
            ),
        };
        wake(state, plan, now, 16, "resource_deficit", message, evidence);
    }
    Ok(None)
}

/// A deficit that automatic importing has not closed in `deficit_wake_days` (an input of a
/// production_priority line: at once, every 7 days) raises its budget by
/// the factories the deficit costs (one civilian factory buys about 8 units), up to the factories
/// the game offers and `auto_import.budget_max` (the plan's budget + 2 by default); at most once
/// every 14 game days. None when there is nothing to raise (the player is woken instead).
#[allow(clippy::too_many_arguments)]
fn raise_import_budget(
    state: &mut State,
    tools: &dyn Tools,
    plan: &Value,
    lease: &str,
    now: GameHour,
    units: f64,
    deficit: &str,
    urgent: bool,
) -> Result<Option<String>> {
    if state.skipped(now, "mimpraise", "x") {
        return Ok(None);
    }
    state.skip(now, "mimpraise", "x", if urgent { 7 } else { 14 } * DAY);
    let (_, budget) = import_cfg(plan);
    let cap = plan["auto_import"]["budget_max"]
        .as_i64()
        .or(budget.map(|b| b + 2))
        .unwrap_or(i64::MAX);
    // every way out says why in the digest (e13: the budget stayed 1 for 46 days of steel
    // deficit and nothing said what stopped the raise)
    let read = match tools.call("hoi4_auto_import", &json!({"lease_token": lease})) {
        Ok(read) => read,
        Err(e) => {
            note(
                state,
                "resources",
                format!(
                    "deficit {deficit}: import budget not raised: hoi4_auto_import unreadable ({})",
                    e.message
                ),
            );
            return Ok(None);
        }
    };
    let auto = &read["auto_import"];
    let (Some(have), Some(available)) = (
        auto["budget_factories"].as_i64(),
        auto["available_factories"].as_i64(),
    ) else {
        note(
            state,
            "resources",
            format!("deficit {deficit}: import budget not raised: budget or available factories unread ({auto})"),
        );
        return Ok(None);
    };
    if auto["enabled"] != json!(true) {
        note(
            state,
            "resources",
            format!(
                "deficit {deficit}: import budget not raised: automatic importing reads {}",
                auto["enabled"]
            ),
        );
        return Ok(None);
    }
    let need = ((units / 8.0).ceil() as i64).max(1);
    let to = (have + need).min(available).min(cap);
    if to <= have {
        // e14, 16 Nov 2026: Ethiopia lost one of its two civilian factories in April; consumer
        // goods took the other, the Trade view offered 0 for trade and the game set the budget to
        // 0, so no steel came for the rest of the war while the wake said "budget 1"
        let why = if available <= have {
            format!(
                "budget {have} of {available} civilian factories the game offers for trade (the rest \
                 serve consumer goods or there are none)"
            )
        } else {
            format!("budget {have} at the plan's cap {cap}")
        };
        note(
            state,
            "resources",
            format!("deficit {deficit}: import budget cannot grow: {why}"),
        );
        state.manage.import_capped = Some(why);
        return Ok(None);
    }
    state.manage.import_capped = None;
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
/// Fuel per game day over the last 30 game days of samples (the game stores no burn rate):
/// "; net -120/day, 95 days left" once 5 days are sampled, else empty. Pure but for the samples.
fn fuel_trend(state: &mut State, now: GameHour, stock: f64) -> String {
    let samples = &mut state.manage.fuel_samples;
    samples.retain(|(at, _)| now - *at <= 30 * DAY && *at <= now);
    samples.push((now, stock));
    let (first_at, first) = samples[0];
    let days = (now - first_at) as f64 / DAY as f64;
    if days < 5.0 {
        return String::new();
    }
    let net = (stock - first) / days;
    if net < 0.0 {
        format!("; net {net:.0}/day, {:.0} days left", stock / -net)
    } else {
        format!("; net +{net:.0}/day")
    }
}

/// Days of fuel left at the sampled rate, None while it does not fall or is not sampled.
fn fuel_days_left(state: &State) -> Option<f64> {
    let samples = &state.manage.fuel_samples;
    let (&(a, first), &(b, last)) = (samples.first()?, samples.last()?);
    let days = (b - a) as f64 / DAY as f64;
    let net = (last - first) / days.max(1.0);
    (days >= 5.0 && net < 0.0).then(|| last / -net)
}

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
    let read = tools.call("hoi4_resources", &json!({})).ok();
    let Some(ratio) = read.as_ref().and_then(|r| r["fuel"]["ratio"].as_f64()) else {
        note(state, "fuel", "unknown: hoi4_resources gave no fuel ratio");
        return Ok(None);
    };
    let trend = read
        .as_ref()
        .and_then(|r| r["fuel"]["stock"].as_f64())
        .map(|stock| fuel_trend(state, now, stock))
        .unwrap_or_default();
    let shown = round3(ratio);
    if ratio >= floor {
        state.manage.fuel_low_since = None;
        note(
            state,
            "fuel",
            format!("{shown} of capacity (floor {floor}){trend}"),
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
/// hoi4_air_wing_create, planes), or why none. `wings` are the entry's own (wing_owners).
/// Reserve = hoi4_stockpile variants of the entry's
/// airframe (and `version`, by design name from hoi4_designs); the entry's own wings below
/// `wing_size` draw their refill from it first, so only what is left beyond that counts.
fn deploy_choice(
    entry: &Value,
    wings: &[&Value],
    stock: &Value,
    designs: &Value,
) -> std::result::Result<(String, i64), String> {
    let Some((min, max_wings, size)) = deploy_settings(entry) else {
        return Err("no deploy".into());
    };
    let Some(airframe) = entry["airframe"].as_str().filter(|a| !a.is_empty()) else {
        return Err("deploy needs the entry's airframe".into());
    };
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
        // hoi4_designs names it design_id (st1, 17 Nov 2026: read as `id`, no reserve variant
        // ever had a name and no planned wing was founded)
        .filter_map(|d| {
            let id = d["design_id"].as_i64().or_else(|| d["id"].as_i64())?;
            Some((id, d["name"].as_str()?.trim().to_string()))
        })
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

/// Wings an entry may own: `deploy.max_wings`, else `count` planes in wings of
/// `deploy.wing_size` (100) rounded up, else every matching wing.
fn wing_cap(entry: &Value) -> Option<usize> {
    if let Some(max) = entry["deploy"]["max_wings"].as_i64().filter(|n| *n >= 0) {
        return Some(max as usize);
    }
    let count = entry["count"].as_i64().filter(|n| *n > 0)?;
    let size = entry["deploy"]["wing_size"]
        .as_i64()
        .filter(|n| *n > 0)
        .unwrap_or(100);
    Some(((count + size - 1) / size) as usize)
}

/// Which `air_plan` entry owns each wing (wing name -> entry index). Two entries of one airframe
/// (the fighters of two axes) used to claim the same wings and send them back and forth between
/// their bases; now a wing has one owner. Entries whose `when` holds choose first, in plan order,
/// then the others; an entry takes up to its cap (`wing_cap`), the wings at its base or over its
/// region first; matching wings left over join the first active entry they match, so none
/// stands idle at war.
fn wing_owners(entries: &[Value], active: &[bool], wings: &[Value]) -> HashMap<String, usize> {
    let mut owner: HashMap<String, usize> = HashMap::new();
    let order: Vec<usize> = (0..entries.len())
        .filter(|i| active[*i])
        .chain((0..entries.len()).filter(|i| !active[*i]))
        .collect();
    for &i in &order {
        let entry = &entries[i];
        let cap = wing_cap(entry).unwrap_or(usize::MAX);
        let rank = |w: &Value| -> u8 {
            let base = entry["base_state"].as_i64();
            if base.is_some() && w["air_base"]["state_id"].as_i64() == base {
                0
            } else if [&entry["region"], &entry["then_region"]]
                .iter()
                .any(|r| !r.is_null() && region_matches(r, &w["region"]))
            {
                1
            } else {
                2
            }
        };
        let mut free: Vec<&Value> = wings
            .iter()
            .filter(|w| {
                let name = text(&w["wing"]);
                !name.is_empty() && !owner.contains_key(name) && wing_matches(entry, w)
            })
            .collect();
        free.sort_by(|a, b| {
            rank(a)
                .cmp(&rank(b))
                .then_with(|| text(&a["wing"]).cmp(text(&b["wing"])))
        });
        for w in free.into_iter().take(cap) {
            owner.insert(text(&w["wing"]).to_string(), i);
        }
    }
    for w in wings {
        let name = text(&w["wing"]);
        if name.is_empty() || owner.contains_key(name) {
            continue;
        }
        if let Some(i) = (0..entries.len()).find(|i| active[*i] && wing_matches(&entries[*i], w)) {
            owner.insert(name.to_string(), i);
        }
    }
    owner
}

pub(super) fn air_plan_step(
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
    let all_wings = list(&air, "wings");
    let active: Vec<bool> = entries
        .iter()
        .map(|e| when_holds(state, tools, &e["when"], now) == Truth::True)
        .collect();
    let owners = wing_owners(entries, &active, all_wings);
    let mut flying = 0;
    let mut deploy_notes: Vec<String> = Vec::new();
    for (index, entry) in entries.iter().enumerate() {
        let owned: Vec<&Value> = all_wings
            .iter()
            .filter(|w| owners.get(text(&w["wing"])) == Some(&index))
            .collect();
        let switched = active[index]
            && !entry["switch_when"].is_null()
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
        // Before its `when` holds an entry flies nothing and moves nothing, but the planes made
        // for it still become its wings (below) and train in peace (air_idle_step).
        for wing in owned.iter().copied().filter(|_| active[index]) {
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
        let (aircraft, planes) = match deploy_choice(entry, &owned, &stock, &designs) {
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

/// Missions held first when fuel runs short at war: the ones that do not hold the front.
const FUEL_CUT_ORDER: [&str; 9] = [
    "training",
    "strategic_bombing",
    "port_strike",
    "logistics_strike",
    "naval_patrol",
    "recon",
    "naval_mines",
    "naval_minesweeping",
    "air_supply",
];

/// Whether the wing flies `mission` now (the mask keeps the last mission after Hold: judge by
/// the region, as hoi4_air says).
fn flies(wing: &Value, mission: &str) -> bool {
    wing["region"].is_object() && strings(wing, "missions").iter().any(|m| m == mission)
}

/// W: air.training_xp, air.idle_wings, air.fuel_mission_cutback. Every 3 game days, at most one
/// action (needs a `manage` object; `manage.off` "air_idle" turns it off):
/// - fuel below `manage.air_training_fuel` (0.5 of capacity): a training wing is held;
/// - at war, fuel below `manage.fuel_floor` (0.25) or fewer than `manage.fuel_days_floor` (30)
///   days left at the sampled rate (fuel_rule): a wing on a mission that does not hold the
///   front is held, in FUEL_CUT_ORDER (superiority, CAS, interception and naval strike stay);
/// - a wing with no mission trains over its own base's region (wing experience up to the
///   training maximum, and the country's air experience that pays plane designs, the air
///   doctrine and the air officer corps) unless `manage.air_training` is false; at war the war
///   missions and the air plan take training wings like standing ones;
/// - else the `air_idle` note says why each idle wing stands.
fn air_idle_step(
    state: &mut State,
    tools: &dyn Tools,
    plan: &Value,
    lease: &str,
    now: GameHour,
) -> Result<Option<String>> {
    let m = &plan["manage"];
    if !m.is_object() || off(plan, "air_idle") || state.skipped(now, "mairidle", "x") {
        return Ok(None);
    }
    let Ok(air) = tools.call("hoi4_air", &json!({})) else {
        state.skip(now, "mairidle", "x", 3 * DAY);
        note(state, "air_idle", "hoi4_air unreadable");
        return Ok(None);
    };
    let wings = list(&air, "wings");
    if wings.is_empty() {
        state.skip(now, "mairidle", "x", 3 * DAY);
        note(state, "air_idle", "no wings");
        return Ok(None);
    }
    let fuel = tools
        .call("hoi4_resources", &json!({}))
        .ok()
        .and_then(|r| r["fuel"]["ratio"].as_f64());
    let war = at_war(state, tools, plan, now);
    let train_floor = fraction(&m["air_training_fuel"], 0.5);
    let floor = fraction(&m["fuel_floor"], 0.25);
    let usable = |w: &&Value| {
        let n = text(&w["wing"]);
        !n.is_empty() && !state.skipped(now, "airidle", n)
    };
    // fuel first: training goes below the training floor, the rear missions below the floor
    let days_floor = m["fuel_days_floor"].as_f64().unwrap_or(30.0);
    let short = fuel_days_left(state).is_some_and(|d| d < days_floor);
    let cut: &[&str] = match fuel {
        Some(f) if war && (f < floor || short) => &FUEL_CUT_ORDER,
        Some(f) if f < train_floor => &FUEL_CUT_ORDER[..1],
        _ => &[],
    };
    for mission in cut {
        if let Some(w) = wings.iter().filter(usable).find(|w| flies(w, mission)) {
            let name = text(&w["wing"]).to_string();
            let (ok, why) =
                state.act(tools, lease, "hoi4_air_hold", json!({"wing": name}), "ah")?;
            state.skip(now, "airidle", &name, if ok { 30 * DAY } else { 7 * DAY });
            return Ok(Some(format!(
                "air fuel {:.2} (training floor {train_floor}, floor {floor}): {name} off {mission}: {ok} {why}",
                fuel.unwrap_or(0.0)
            )));
        }
    }
    let idle: Vec<&Value> = wings.iter().filter(|w| !w["region"].is_object()).collect();
    let why = if m["air_training"] == json!(false) {
        format!(
            "manage.air_training is false ({})",
            m["air_training_why"].as_str().unwrap_or("no reason given")
        )
    } else {
        match fuel {
            None => "fuel unread, no training".to_string(),
            Some(f) if f < train_floor => {
                format!("fuel {f:.2} below the training floor {train_floor}")
            }
            Some(_) => {
                // a wing already training reads region null too: it is not idle (sk27a, 18 Nov
                // 2026: 17 wings were sent "training" again every 30 days, 4 times each)
                let pick = idle.iter().copied().filter(usable).find(|w| {
                    !strings(w, "missions").iter().any(|m| m == "training")
                        && w["air_base"]["state_id"].as_i64().is_some()
                        && w["transfer_to_base_id"].as_i64().is_none()
                });
                if let Some(w) = pick {
                    let name = text(&w["wing"]).to_string();
                    let base = w["air_base"]["state_id"].as_i64().unwrap_or(0);
                    let (ok, why) = state.act(
                        tools,
                        lease,
                        "hoi4_air_mission",
                        json!({"wing": name, "mission": "training", "state_id": base}),
                        "at",
                    )?;
                    // a wing the game offers no mission at all (no planes, or a type with no
                    // land-based mission: g17f, LG 'Greifswald' refused 5 times in 6 months)
                    // waits half a year, not a month
                    let none_offered = !ok
                        && why.contains("invalid_target")
                        && why.trim_end().ends_with("it can fly:");
                    state.skip(
                        now,
                        "airidle",
                        &name,
                        if none_offered { 180 } else { 30 } * DAY,
                    );
                    return Ok(Some(format!(
                        "air {name} trains over its base (state {base}){}: {ok} {why}",
                        if war {
                            " away from the fronts"
                        } else {
                            " in peace"
                        }
                    )));
                }
                "training refused or tried within 30 days".to_string()
            }
        }
    };
    state.skip(now, "mairidle", "x", 3 * DAY);
    let training = wings.iter().filter(|w| flies(w, "training")).count();
    note(
        state,
        "air_idle",
        if idle.is_empty() {
            format!("every wing has a mission ({training} training)")
        } else {
            format!(
                "{} wing(s) stand by: {why}; {training} training",
                idle.len()
            )
        },
    );
    Ok(None)
}

/// Experience a spirit slot costs by its place in the branch (common/idea_tags/00_idea.txt
/// slot costs 20, 35, 50; the window shows the real cost and an entry's `cost` overrides).
const SPIRIT_SLOT_COST: [f64; 3] = [20.0, 35.0, 50.0];

/// The refusals an officer corps pick has had (plan_xp.rs: a dropped pick holds nothing back).
pub(super) fn ofc_refused(state: &State, key: &str) -> u32 {
    state.manage.ofc_refused.get(key).copied().unwrap_or(0)
}

/// Spirit slots per officer corps branch (hoi4_officer_corps list, pp24: army 3, navy 3, air 2).
fn spirit_slots(branch: &str) -> usize {
    if branch == "air" {
        2
    } else {
        3
    }
}

/// A `when` predicate with its date terms ("date >= ...") dropped: a lone date becomes no
/// condition, an `all` keeps its other terms; anything else is kept whole.
fn without_dates(when: &Value) -> Value {
    let dated = |v: &Value| {
        v.as_str()
            .is_some_and(|s| s.trim_start().starts_with("date"))
    };
    match when {
        v if dated(v) => Value::Null,
        Value::Object(m) if m.len() == 1 && m.get("all").is_some_and(Value::is_array) => {
            let kept: Vec<Value> = list(when, "all")
                .iter()
                .filter(|t| !dated(t))
                .cloned()
                .collect();
            if kept.is_empty() {
                Value::Null
            } else {
                json!({"all": kept})
            }
        }
        _ => when.clone(),
    }
}

/// W: advisors.officer_corps_spirits. `officer_corps`: [{slot: "army:0", spirit: "Bold Attack",
/// cost?, when?}] picked in plan order, one a tick, once the branch's experience covers the
/// cost (`xp:<branch> >= cost`) on top of `manage.xp_reserve.<branch>` and of what the
/// `xp_budget` spends listed before it still need (plan_xp.rs); a pick the game does not take
/// rests 30 days, the third refusal drops the entry with a note. A spirit its slot does not offer
/// (`spirit_unknown`: the plan named the wrong slot, fk1 18 Nov 2026: Effective Training
/// Programs is air:0, not air:1) is looked for in the branch's other slots the next day. Every 7
/// game days.
fn officer_corps_step(
    state: &mut State,
    tools: &dyn Tools,
    plan: &Value,
    lease: &str,
    now: GameHour,
) -> Result<Option<String>> {
    let entries = list(plan, "officer_corps");
    if entries.is_empty() || state.skipped(now, "mofc", "x") {
        return Ok(None);
    }
    let mut waiting: Vec<String> = Vec::new();
    for entry in entries {
        let (Some(slot), Some(spirit)) = (entry["slot"].as_str(), entry["spirit"].as_str()) else {
            continue;
        };
        let key = format!("ofc:{slot}:{spirit}");
        if state.done.contains_key(&key) || state.skipped(now, "ofc", &key) {
            continue;
        }
        if state.manage.ofc_refused.get(&key).copied().unwrap_or(0) >= 3 {
            continue;
        }
        // a date in the entry's `when` is the latest day, not the first: a spirit the
        // experience pays for now is bought now, after the xp_budget spends listed before it
        // (owner, 18 Nov 2026; g17i: the officer corps waited 682 days on 1938 dates with the
        // experience on hand). `when_strict: true` keeps the dates.
        let when = if entry["when_strict"] == json!(true) {
            entry["when"].clone()
        } else {
            without_dates(&entry["when"])
        };
        if when_holds(state, tools, &when, now) != Truth::True {
            waiting.push(format!("{spirit} ({slot}): when"));
            continue;
        }
        let (branch, index) = slot.split_once(':').unwrap_or((slot, "0"));
        let cost = entry["cost"]
            .as_f64()
            .unwrap_or_else(|| SPIRIT_SLOT_COST[index.parse::<usize>().unwrap_or(0).min(2)]);
        let reserve = plan["manage"]["xp_reserve"][branch].as_f64().unwrap_or(0.0);
        let (before, names) = super::plan_xp::ahead(
            state,
            tools,
            plan,
            branch,
            &super::plan_xp::Spender::Spirit(entry),
            now,
        );
        let need = cost + reserve + before;
        let gate = json!(format!("xp:{branch} >= {need}"));
        if when_holds(state, tools, &gate, now) != Truth::True {
            let held = if names.is_empty() {
                String::new()
            } else {
                format!(" ({before:.0} held for {} first)", names.join(", "))
            };
            waiting.push(format!(
                "{spirit} ({slot}): needs {need} {branch} experience{held}"
            ));
            continue;
        }
        // the slot tried: the plan's, else the next one of the branch after a spirit_unknown
        let shift = state.manage.ofc_slot_shift.get(&key).copied().unwrap_or(0);
        let first: usize = index.parse().unwrap_or(0);
        let tried = format!("{branch}:{}", (first + shift) % spirit_slots(branch));
        let (ok, why) = state.act(
            tools,
            lease,
            "hoi4_officer_corps",
            json!({"slot": tried, "spirit": spirit}),
            "oc",
        )?;
        if ok {
            state.done.insert(key, now);
        } else if super::plan_military::code(&why) == "spirit_unknown"
            && shift + 1 < spirit_slots(branch)
        {
            state.manage.ofc_slot_shift.insert(key.clone(), shift + 1);
            state.skip(now, "ofc", &key, DAY);
            note(
                state,
                "officer_corps",
                format!("{spirit}: not offered in {tried}; the next slot of {branch} tomorrow"),
            );
        } else {
            *state.manage.ofc_refused.entry(key.clone()).or_insert(0) += 1;
            state.skip(now, "ofc", &key, 30 * DAY);
        }
        return Ok(Some(format!(
            "officer corps {spirit} ({tried}): {ok} {why}"
        )));
    }
    state.skip(now, "mofc", "x", 7 * DAY);
    note(
        state,
        "officer_corps",
        if waiting.is_empty() {
            "every planned spirit is taken or dropped".to_string()
        } else {
            format!("waiting: {}", waiting.join("; "))
        },
    );
    Ok(None)
}

/// Game days a reserve variant must stand without shrinking before it becomes a wing.
const RESERVE_STANDS: GameHour = 14 * DAY;

/// W: air.deploy_new_wings for the planes no `air_plan` deploy entry takes (g17e: 90 close air
/// support planes stood in reserve for two years). A reserve variant of at least
/// `manage.air_reserve_min` (50) planes that has not shrunk for 14 game days (the wings of its
/// kind are full and draw nothing from it) becomes a new wing of up to 100 planes at the base of
/// a wing of the same kind with room, else at the own base with the most room. Carrier planes
/// stay in reserve for the carriers. Every 7 game days, at most one action.
fn air_reserve_step(
    state: &mut State,
    tools: &dyn Tools,
    plan: &Value,
    lease: &str,
    now: GameHour,
) -> Result<Option<String>> {
    let m = &plan["manage"];
    if !m.is_object() || off(plan, "air_reserve") || state.skipped(now, "mairres", "x") {
        return Ok(None);
    }
    state.skip(now, "mairres", "x", 7 * DAY);
    let Some(stock) = stockpile(state, tools, now) else {
        note(state, "air_reserve", "hoi4_stockpile unreadable");
        return Ok(None);
    };
    let min = m["air_reserve_min"]
        .as_i64()
        .filter(|n| *n > 0)
        .unwrap_or(50) as f64;
    let variants: Vec<&Value> = list(&stock, "equipment")
        .iter()
        .flat_map(|row| list(row, "variants"))
        .filter(|v| {
            let t = text(&v["type"]);
            t.contains("plane") && !t.starts_with("cv_")
        })
        .collect();
    if variants.is_empty() {
        state.manage.air_reserve_seen.clear();
        note(state, "air_reserve", "no planes in reserve");
        return Ok(None);
    }
    let designs = tools
        .call("hoi4_designs", &json!({"kind": "plane"}))
        .unwrap_or(Value::Null);
    let names: HashMap<i64, String> = list(&designs, "designs")
        .iter()
        // hoi4_designs names it design_id (st1, 17 Nov 2026: read as `id`, no reserve variant
        // ever had a name and no planned wing was founded)
        .filter_map(|d| {
            let id = d["design_id"].as_i64().or_else(|| d["id"].as_i64())?;
            Some((id, d["name"].as_str()?.trim().to_string()))
        })
        .filter(|(_, n)| !n.is_empty())
        .collect();
    let entries = list(plan, "air_plan");
    let mut seen: HashMap<String, (GameHour, f64)> = HashMap::new();
    let mut waiting: Vec<String> = Vec::new();
    let mut ready: Vec<(String, String, f64)> = Vec::new();
    for v in variants {
        let kind = text(&v["type"]).to_string();
        let amount = v["amount"].as_f64().unwrap_or(0.0).floor();
        let name = v["design_id"]
            .as_i64()
            .and_then(|id| names.get(&id).cloned());
        let planned = entries.iter().any(|e| {
            deploy_settings(e).is_some()
                && e["airframe"]
                    .as_str()
                    .is_some_and(|a| !a.is_empty() && kind.starts_with(a))
                && e["version"]
                    .as_str()
                    .filter(|w| !w.is_empty())
                    .is_none_or(|w| name.as_ref().is_none_or(|n| n.eq_ignore_ascii_case(w)))
        });
        if planned || amount < min {
            continue;
        }
        let key = format!("{kind}#{}", v["design_id"]);
        let (since, then) = match state.manage.air_reserve_seen.get(&key) {
            // the wings of its kind drew from it: they were short, not full
            Some((_, then)) if amount < then * 0.9 => (now, amount),
            Some(seen) => *seen,
            None => (now, amount),
        };
        seen.insert(key.clone(), (since, then));
        let label = name.clone().unwrap_or_else(|| kind.clone());
        if now - since < RESERVE_STANDS || state.skipped(now, "airres", &key) {
            waiting.push(format!("{label} {amount:.0}"));
            continue;
        }
        match name {
            Some(n) => ready.push((key, n, amount)),
            None => waiting.push(format!("{label} {amount:.0} (no design name)")),
        }
    }
    state.manage.air_reserve_seen = seen;
    ready.sort_by(|a, b| b.2.total_cmp(&a.2));
    let Some((key, aircraft, amount)) = ready.into_iter().next() else {
        note(
            state,
            "air_reserve",
            if waiting.is_empty() {
                "no unplanned reserve planes".to_string()
            } else {
                format!("unplanned reserve, watched 14 days: {}", waiting.join(", "))
            },
        );
        return Ok(None);
    };
    let kind = key.split('#').next().unwrap_or("").to_string();
    let planes = amount.min(100.0) as i64;
    let Ok(air) = tools.call("hoi4_air", &json!({})) else {
        note(state, "air_reserve", "hoi4_air unreadable");
        return Ok(None);
    };
    let bases = list(&air, "air_bases");
    let room = |b: &Value| b["free"].as_i64().unwrap_or(0);
    let kin: HashSet<i64> = list(&air, "wings")
        .iter()
        .filter(|w| text(&w["aircraft_type"]) == kind)
        .filter_map(|w| w["air_base"]["state_id"].as_i64())
        .collect();
    let base = bases
        .iter()
        .filter(|b| room(b) >= planes)
        .max_by_key(|b| {
            (
                b["state_id"].as_i64().is_some_and(|s| kin.contains(&s)),
                room(b),
            )
        })
        .and_then(|b| b["state_id"].as_i64());
    let Some(base) = base else {
        note(
            state,
            "air_reserve",
            format!("{amount:.0} {aircraft} in reserve: no own air base with room for {planes}"),
        );
        return Ok(None);
    };
    let (ok, why) = state.act(
        tools,
        lease,
        "hoi4_air_wing_create",
        json!({"state_id": base, "aircraft": aircraft}),
        "ar",
    )?;
    if ok {
        state.manage.air_reserve_seen.remove(&key);
    } else {
        state.skip(now, "airres", &key, 14 * DAY);
    }
    Ok(Some(format!(
        "air reserve: new wing of {aircraft} ({amount:.0} unused in reserve) at state {base}: {ok} {why}"
    )))
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

/// The vacancy fill that follows the plan's own `politics_auto.order` (off with
/// `politics_auto.fill_vacant: false`): every seat a read showed vacant, political advisors
/// first, then theorists and designers, then the military staff, takes the first advisor the
/// game enables there (`never` respected). Until a category has been read, one seat stands for
/// it (its read lists the seats). Without it a plan whose order named keys the game does not
/// offer let the power pile up beside six vacant seats (gr3 16 Nov, pm2 17 Nov 2026: 165 PP,
/// every seat vacant, nothing bought).
fn politics_auto_fill(state: &State, now: GameHour, recheck: GameHour) -> Vec<Value> {
    let mut out = Vec::new();
    for (category, seed, cost) in [
        ("government", 0, 150),
        ("research_production", 5, 100),
        ("military_staff", 0, 50),
    ] {
        let seats = match state.manage.pp_vacant.get(category) {
            Some((at, seats)) if now.saturating_sub(*at) < recheck => seats.clone(),
            _ => vec![seed],
        };
        for slot in seats {
            out.push(
                json!({"category": category, "slot": slot, "cost": cost, "fill": true,
                "why": "a vacant seat: the first advisor the game enables"}),
            );
        }
    }
    out
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
    // (a law the plan or a focus chose) is left alone. A staff or design seat holds no law: an
    // advisor there the order does not name gives the seat to the one it names (fk1, 18 Nov
    // 2026: a naval officer sat in the high command seat the order gave Rommel, with 540
    // political power idle).
    let advisor_seat = matches!(
        text(&entry["category"]),
        "military_staff" | "research_production"
    );
    let better = match ideas.iter().position(|i| i == current) {
        Some(at) => &ideas[..at],
        None if current.is_empty() || advisor_seat => &ideas[..],
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

/// `politics_auto` (default on): political power at or above `threshold` (0; a plan's own number
/// counts only with its `threshold_why`) beyond what the
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
    // power is banked only for a reason: the plan's own pending picks (the reserve below) or a
    // threshold the plan explains (`threshold_why`); a bare number is no reason to leave power
    // unspent while the order has an affordable pick (owner, 18 Nov 2026: "in the beginning of
    // the campaigns, you want to spend it as soon as possible on best things"; g17i's 150 kept
    // the power at 149 for months)
    let threshold = if text(&cfg["threshold_why"]).trim().is_empty() {
        0.0
    } else {
        cfg["threshold"].as_f64().unwrap_or(0.0)
    };
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
    let mut order = if cfg["order"].is_array() {
        cfg["order"].clone()
    } else {
        default_order
    };
    if cfg["fill_vacant"] != json!(false) {
        if let Some(entries) = order.as_array_mut() {
            for fill in politics_auto_fill(state, now, recheck) {
                let taken = entries
                    .iter()
                    .any(|e| e["category"] == fill["category"] && e["slot"] == fill["slot"]);
                if !taken {
                    entries.push(fill);
                }
            }
        }
    }
    let order = &order;
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
        let mut current = options["current"]
            .as_str()
            .or(options["current"]["idea"].as_str())
            .unwrap_or("")
            .to_string();
        let offered: Vec<(String, bool)> = list(&options, "options")
            .iter()
            .map(|i| (text(&i["idea"]).to_string(), i["enabled"] == json!(true)))
            .collect();
        let mut slot = slot.clone();
        let mut choice = politics_auto_choice(entry, &current, &offered, &never, &conscription);
        // The order names advisors, not seats: an entry whose seat holds what the order does not
        // rank (or holds anything, for a vacancy fill) takes its idea at another vacant seat of
        // the category that offers it (gr3 16 Nov 2026: Ribbentrop held seat 4, the order's
        // Göring entry stopped there while seat 5 stood vacant all year).
        if choice.is_err() && !current.is_empty() {
            let vacant = state
                .manage
                .pp_vacant
                .get(&category)
                .map(|(_, v)| v.clone())
                .unwrap_or_default();
            for seat in vacant.into_iter().filter(|v| json!(v) != slot) {
                let Ok(other) = tools.call(
                    "hoi4_politics_options",
                    &json!({"category": category, "slot": seat, "lease_token": lease}),
                ) else {
                    continue;
                };
                let offered: Vec<(String, bool)> = list(&other, "options")
                    .iter()
                    .map(|i| (text(&i["idea"]).to_string(), i["enabled"] == json!(true)))
                    .collect();
                let mut moved = entry.clone();
                moved["slot"] = json!(seat);
                if let Ok(idea) = politics_auto_choice(&moved, "", &offered, &never, &conscription)
                {
                    slot = json!(seat);
                    current.clear();
                    choice = Ok(idea);
                    break;
                }
            }
        }
        let idea = match choice {
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
        let line = format!(
            "PP {pp_text} ({reserve_text}): {category}:{slot} {from} -> {idea} ({reason}): {ok}"
        );
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

/// Where the pulled-out divisions go: a province where own divisions stand whose headroom, less
/// what groups already on their way there will draw (`load`), still holds at least `least`;
/// one with room for the whole `need` first, then in the source's state, then where the same
/// army stands, then the most room. Provinces `avoid` names (a march order there failed) are
/// passed over. (state, province, room) or None.
fn overdrawn_destination(
    read: &Value,
    source: i64,
    army: &str,
    need: f64,
    least: f64,
    load: &HashMap<i64, f64>,
    avoid: impl Fn(i64) -> bool,
) -> Option<(i64, i64, f64)> {
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
            let room = p["headroom"].as_f64()? - load.get(&pid).copied().unwrap_or(0.0);
            (pid != source && room >= least && !avoid(pid)).then_some((pid, sid, room))
        })
        .max_by(|a, b| {
            let rank = |(pid, sid, room): &(i64, i64, f64)| {
                (
                    *room >= need,
                    Some(*sid) == src_state,
                    army_provinces.contains(pid),
                )
            };
            rank(a).cmp(&rank(b)).then(a.2.total_cmp(&b.2))
        })
        .map(|(pid, sid, room)| (sid, pid, room))
}

/// supply.split_overdrawn (`manage.overdrawn_floor` 35, `overdrawn_share` 0.5,
/// `overdrawn_rest_days` 10), once a game day from hoi4_supply: in a province on
/// `overdrawn_provinces` whose divisions' mean supply is below the floor (attrition starts at
/// 35%), the weakest divisions of the army with most divisions there (at most `overdrawn_share`
/// of them, until their supply need covers the shortfall) march to the province with room for
/// them (hoi4_army_move with `divisions`: in the same state first, then where the same army
/// stands). They stay in their army: no new army is made. A province takes only what its
/// headroom holds, less what groups already on their way there will draw, so the next group
/// goes elsewhere; a group is remembered for `overdrawn_rest_days` (its divisions are not picked
/// again meanwhile). A province whose march order failed is not chosen again for 10 days. No
/// province with room: nothing moves and the digest says so (the supply relief's railway and
/// hub levers remain).
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
    // A defender in its hold stands and entrenches: splitting stacks into new armies sent to
    // provinces with room scattered its line (e11, 16 Nov 2026: Tigray's armies broke into
    // Army 1-3, which marched into Wello's 7944 while Italy walked through the west to the
    // capital; the game's AI as Ethiopia never moves and held months longer).
    if defending(plan) {
        return Ok(None);
    }
    let rest = m["overdrawn_rest_days"]
        .as_i64()
        .filter(|d| *d >= 0)
        .unwrap_or(10)
        * DAY;
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
    // Groups on their way: what they will draw where they go (until they stand there) and
    // their divisions, which are not picked again. g17b (17 Nov 2026) sent Armies 4, 6 and 7
    // one after another to province 241, each against the same day-old headroom.
    let mut load: HashMap<i64, f64> = HashMap::new();
    let mut leaving: HashMap<String, i64> = HashMap::new();
    for s in &state.manage.spread {
        for id in &s.divisions {
            let row = divisions.get(id);
            let at = row.and_then(|d| d["province_id"].as_i64());
            if at != Some(s.province) {
                *load.entry(s.province).or_default() +=
                    row.and_then(|d| d["supply_need"].as_f64()).unwrap_or(0.0);
                if let Some(at) = at {
                    leaving.insert(id.clone(), at);
                }
            }
        }
    }
    let mut lines: Vec<String> = Vec::new();
    for o in list(&read, "overdrawn_provinces") {
        let Some(pid) = o["province_id"].as_i64() else {
            continue;
        };
        if state.skipped(now, "spread", &pid.to_string()) {
            continue;
        }
        let all: Vec<&Value> = list(o, "division_ids")
            .iter()
            .filter_map(|id| divisions.get(&division_id(id)?).copied())
            .collect();
        let pcts: Vec<f64> = all
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
        // divisions already on their way out no longer count against the province
        let going: f64 = all
            .iter()
            .filter(|r| division_id(&r["id"]).is_some_and(|id| leaving.contains_key(&id)))
            .filter_map(|r| r["supply_need"].as_f64())
            .sum();
        let shortfall = o["shortfall"].as_f64().unwrap_or(0.0) - going;
        if shortfall <= 0.0 {
            lines.push(format!(
                "province {pid} ({mean:.0}% supply): enough divisions are already on their way out"
            ));
            continue;
        }
        let rows: Vec<&Value> = all
            .iter()
            .copied()
            .filter(|r| division_id(&r["id"]).is_none_or(|id| !leaving.contains_key(&id)))
            .collect();
        let Some((army, mut ids, mut need)) = overdrawn_pick(
            &rows,
            shortfall,
            share,
            |a| sizes.get(a).copied().unwrap_or(0),
            |a| {
                super::plan_military::holding(state, a)
                    || super::plan_military::invading(state, plan, a)
            },
        ) else {
            lines.push(format!("province {pid}: no army whose divisions may leave"));
            continue;
        };
        let need_of = |id: &String| {
            divisions
                .get(id)
                .and_then(|d| d["supply_need"].as_f64())
                .unwrap_or(0.0)
        };
        let least = ids.iter().map(need_of).fold(f64::INFINITY, f64::min);
        let Some((to_state, to_province, room)) =
            overdrawn_destination(&read, pid, &army, need, least, &load, |p| {
                state.skipped(now, "spreadto", &p.to_string())
            })
        else {
            state.skip(now, "spread", &pid.to_string(), 3 * DAY);
            lines.push(format!(
                "province {pid} ({mean:.0}% supply): no province with {need:.2} supply of room for {} of {army}'s divisions; railway or hub needed",
                ids.len()
            ));
            continue;
        };
        // no province holds them all: as many as the one with most room feeds (the weakest
        // first); the rest go the next day, elsewhere
        while need > room && ids.len() > 1 {
            if let Some(last) = ids.pop() {
                need -= need_of(&last);
            }
        }
        let (ok, why) = state.act(
            tools,
            lease,
            "hoi4_army_move",
            json!({"army": army, "state_id": to_state, "province_id": to_province, "divisions": ids}),
            "so",
        )?;
        if ok {
            state.skip(now, "spread", &pid.to_string(), 5 * DAY);
            state.manage.spread.push(Spread {
                divisions: ids.clone(),
                province: to_province,
                since: now,
            });
        } else {
            // The order to that province did not take (g17b: "No division of the army took a
            // march order to province 241", four times, with other groups standing on it):
            // the next try goes to another province, the next game day.
            state.skip(now, "spreadto", &to_province.to_string(), 10 * DAY);
            state.skip(now, "spread", &pid.to_string(), DAY);
        }
        let line = format!(
            "province {pid} at {mean:.0}% supply (shortfall {shortfall:.2}): {} of {army}'s divisions to state {to_state} province {to_province} ({room:.2} supply of room), in their army: {ok} {why}",
            ids.len()
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
    // no priority line on the map: the items that never go obsolete (economy research A16:
    // trucks, support equipment, trains, then infantry), then any line the plan keeps; a free
    // factory is never left idle for want of a named line (g17i, 18 Nov 2026: 8 of 38 idle)
    let fallback: Vec<String> = [
        "support_equipment",
        "motorized_equipment",
        "train_equipment",
        "infantry_equipment",
    ]
    .map(String::from)
    .to_vec();
    for wants in [&priority, &fallback] {
        for want in wants {
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
        if !lines.is_empty() {
            break;
        }
    }
    if lines.is_empty() {
        lines = military
            .iter()
            .filter(|m| !m.1.is_empty() && !removed.contains(&m.1))
            .take(1)
            .collect();
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
    // no shortage: a line below the plan's own count (production[].factories) first, in
    // priority order: plane lines show no stock cover (their output becomes wings) and never
    // won the cover choice below (g17f 1937: fighters on 1 factory of the plan's 6)
    for l in &lines {
        let planned: i64 = list(plan, "production")
            .iter()
            .filter(|e| text(&e["equipment"]) == l.1)
            .filter_map(|e| e["factories"].as_i64())
            .sum();
        if l.2 < planned {
            return Some((
                (*l).clone(),
                format!("below the plan's {planned} factories ({} now)", l.2),
            ));
        }
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
    // half the free factories at least (the rest go to the line chosen next, in the same turn)
    let give = idle.min(step.max((idle + 1) / 2));
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
        "production_set {equipment} {factories}+{give} of {idle} idle: {ok} {result}"
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
    // at least one idle factory whatever the plan's minimum: with `production_min_idle` 0 and
    // none idle the step "gave" +0 to a line every tick, a Verified order that changes nothing
    // (e1, 16 Nov 2026: 266 times in the paused setup, so the setup never completed)
    // the balance rule owns idle factories while it is on: two owners fought over the same
    // factories (pr1: +2 by the feed, -1 by the balance, again and again)
    let balance_on = !off(plan, "balance") && !off(plan, "stockpile");
    // one free factory is enough by default: it loses its output the day it is free
    if balance_on
        || idle < plan["production_min_idle"].as_i64().unwrap_or(1).max(1)
        || state.skipped(now, "prodset", "x")
    {
        return Ok(None);
    }
    // every free factory in this turn: fed by cover, re-chosen after each line from fresh reads
    let mut lines: Vec<String> = Vec::new();
    let (mut military, mut idle) = (military, idle);
    while idle > 0 && lines.len() < BALANCE_TURN_SETS {
        let Some(line) = feed(state, tools, plan, lease, now, &military, idle)? else {
            break;
        };
        let failed = line.contains(": false");
        lines.push(line);
        if failed {
            break;
        }
        let economy = tools.call("hoi4_economy", &json!({}))?;
        let before = idle;
        (military, idle) = military_lines(tools, &economy)?;
        if idle >= before {
            // the read does not show the factories given: no second order on a stale read
            break;
        }
    }
    Ok((!lines.is_empty()).then(|| lines.join("; ")))
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
        note(
            state,
            "production",
            "day 0: no free military factory (the balance rule assigns later ones)",
        );
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
pub(super) fn at_war(state: &mut State, tools: &dyn Tools, plan: &Value, now: GameHour) -> bool {
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
                // an unmeasured cover (no consumption in peace) is no critical cover: the
                // trucks get idle factories, never factories pulled from another line (pm2,
                // 17 Nov 2026: rifles 14 -> 10 for trucks in the first January)
                low.push((
                    "motorized_equipment".into(),
                    "motorized_equipment".into(),
                    days.unwrap_or(f64::MAX),
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
    // A shortage resting on its skip timer steps aside: the lowest cover among the rest is
    // worked (an equipment no line can make must not hold back every other target).
    let Some((equipment, archetype, days, why)) = low
        .into_iter()
        .filter(|l| !state.skipped(now, "mstockline", &l.1))
        .min_by(|a, b| a.2.partial_cmp(&b.2).unwrap_or(std::cmp::Ordering::Equal))
    else {
        return Ok(None);
    };
    let economy = tools.call("hoi4_economy", &json!({}))?;
    let (military, idle) = military_lines(tools, &economy)?;
    let step = m["rebalance_step"].as_i64().filter(|n| *n > 0).unwrap_or(3);
    // the plan's dated lines under their floor keep the idle factories they lack (the balance
    // fills them first); a stock below its target gets only the rest (g17j day 0: trucks 7 -> 10
    // took the rifles' planned 12)
    let (floors, _) = production_floors(plan, &read, now);
    let arche_of = |eq: &str| archetype_row(&read, eq).unwrap_or_else(|| family(eq).to_string());
    let asked = |a: &str| -> i64 {
        military
            .iter()
            .filter(|l| arche_of(&l.1) == a)
            .map(|l| l.2)
            .sum()
    };
    let reserved: i64 = floors
        .iter()
        .filter(|(a, _)| a.as_str() != archetype)
        .map(|(a, f)| (f - asked(a)).max(0))
        .sum();
    let give = (idle - reserved).max(0).min(step);
    // No idle factory: a shortage the divisions feel (or a cover under the critical days) takes
    // factories from a line whose equipment nobody lacks and whose stock covers twice its
    // target. Without this owner a stock stayed at 0 for eight months while every factory
    // built what was already plenty (live 15 Nov 2026: artillery 0 from May, the strike army
    // at a sixth of its equipment, the missing-production alert open all year).
    let felt = short_of(&read, &archetype) > 0.0 || days < critical;
    if give <= 0 && felt && m["shift_factories"] != json!(false) {
        let recent: HashSet<String> = state
            .manage
            .pulled
            .iter()
            .filter(|(_, at)| now - **at < 30 * DAY)
            .map(|(e, _)| e.clone())
            .collect();
        let candidates: Vec<MilLine> = military
            .iter()
            .filter(|l| !recent.contains(&l.1))
            .filter(|l| floors.get(&arche_of(&l.1)).is_none_or(|f| l.2 > *f))
            .cloned()
            .collect();
        let rates = supply(state, tools, now)
            .map(|s| division_rates(&s, &read))
            .unwrap_or_default();
        if let Some(((index, have, factories, _), take)) =
            donor_line(&read, plan, &candidates, &archetype, warning, step, &rates)
        {
            // PR09: a moved factory restarts at the start efficiency: a line gives at most a
            // fifth of itself and once in 30 days
            let take = take.min((factories / 5).max(1));
            state.manage.pulled.insert(have.clone(), now);
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
    rates: &HashMap<String, (f64, f64, f64)>,
) -> Option<(MilLine, i64)> {
    // The plan's own allocation (production[].factories for this equipment) is a floor no
    // shortage shift cuts below: plane output goes straight into wings, so a fighter line shows
    // no stock to protect and gave its factories one by one to rifles and support equipment
    // (g17f, 1936-37: the plan's 6 fighter factories down to 1, support 4 -> 11).
    let floor = |equipment: &str| -> i64 {
        list(plan, "production")
            .iter()
            .filter(|e| text(&e["equipment"]) == equipment)
            .filter_map(|e| e["factories"].as_i64())
            .sum::<i64>()
            .max(1)
    };
    military
        .iter()
        .filter(|l| l.2 > floor(&l.1) && !l.1.starts_with(needy))
        .filter(|l| {
            let Some(arche) = archetype_row(read, &l.1) else {
                return false;
            };
            if short_of(read, &arche) > 0.0 {
                return false;
            }
            // Equipment the main template's divisions use never pays for equipment they do not
            // use: a few trucks short at the front took artillery and support factories again
            // and again while those bound the training (g17c 1936, reproduced by the production
            // simulation: 7 trucks short of 605, artillery 2 -> 1, support 3 -> 2).
            if rates.contains_key(&arche) && !rates.contains_key(needy) {
                return false;
            }
            // Never make the donor the binding equipment of the main template: after the move
            // its stock and output must still equip at least as many divisions over the next
            // DONOR_HORIZON_DAYS as the needy item's (g17b, 17 Nov 2026: artillery cut 2 -> 1
            // for support equipment, which then made twice what the divisions took while
            // artillery bound at 0.09 divisions a day). The stock counts: rifles held for 21
            // divisions while support equipment held for 2 at the same daily rate, and a rate-
            // only guard kept all 16 rifle factories (g17c, 1937.01).
            if let (Some(&(rd, fd, sd)), Some(&(rn, fnd, sn))) =
                (rates.get(&arche), rates.get(needy))
            {
                let take = step.min(l.2 - 1).min((l.2 / 5).max(1)) as f64;
                let donor_after = if fd > 0.0 { rd * (fd - take) / fd } else { rd };
                let needy_after = if fnd > 0.0 { rn + rn / fnd * take } else { rn };
                if sd + donor_after * DONOR_HORIZON_DAYS < sn + needy_after * DONOR_HORIZON_DAYS {
                    return false;
                }
            }
            let want = plan["stockpile_targets"][arche.as_str()]
                .as_f64()
                .or_else(|| plan["stockpile_targets"][l.1.as_str()].as_f64())
                .unwrap_or(warning);
            crate::plan_predicate::stockpile_days(read, &l.1).is_ok_and(|d| d >= 2.0 * want)
        })
        .max_by_key(|l| (l.2, std::cmp::Reverse(l.0)))
        .map(|l| (l.clone(), step.min(l.2 - floor(&l.1))))
}

/// The horizon over which a donor's stock and output are weighed against the needy item's: half
/// a year, the time a moved factory needs to regain most of its efficiency (PR09).
const DONOR_HORIZON_DAYS: f64 = 180.0;

/// Divisions a day the current output of each archetype equips for the template most fielded
/// divisions use: archetype -> (produced per day / that template's need per division, factories
/// on it, divisions the stockpile equips), from hoi4_supply division rows and a hoi4_stockpile
/// read. Empty when either lacks it.
fn division_rates(supply: &Value, stock: &Value) -> HashMap<String, (f64, f64, f64)> {
    let mut count: BTreeMap<String, usize> = BTreeMap::new();
    for d in list(supply, "divisions") {
        if !list(d, "equipment").is_empty() {
            *count.entry(text(&d["template"]).to_string()).or_default() += 1;
        }
    }
    let Some(main) = count
        .iter()
        .max_by_key(|(_, n)| **n)
        .map(|(t, _)| t.clone())
    else {
        return HashMap::new();
    };
    let Some(division) = list(supply, "divisions")
        .iter()
        .find(|d| text(&d["template"]) == main && !list(d, "equipment").is_empty())
    else {
        return HashMap::new();
    };
    let mut out = HashMap::new();
    for e in list(division, "equipment") {
        let arche = text(&e["archetype"]);
        let Some(need) = e["need"].as_f64().filter(|n| *n > 0.0) else {
            continue;
        };
        if let Some(row) = list(stock, "equipment")
            .iter()
            .find(|r| text(&r["archetype"]) == arche)
        {
            let made = row["produced_per_day"].as_f64().unwrap_or(0.0);
            let factories = row["factories"].as_f64().unwrap_or(0.0);
            let held = row["stockpile"].as_f64().unwrap_or(0.0).max(0.0);
            out.insert(arche.to_string(), (made / need, factories, held / need));
        }
    }
    out
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

// ---------------------------------------------------------------------------------------------
// Production balance (playbook PR02, PR09, PR10, PR11): one line per equipment, no line asking for
// factories that do not exist, new and idle factories by need, pulls only from a large surplus.

/// Every own production line (all pages of hoi4_production_lines, 128 a page). Err when any page
/// is unreadable: a caller must never take "no line read" for "no line exists" (a limit above
/// the tool's bound made every read fail and the plan's lines were added again at the bottom,
/// live 17 Nov 2026).
/// The game's dockyard cap of a naval line (00_defines NProduction: capital ships 5, convoys 15,
/// the rest 10), from the hull's equipment key.
fn dockyard_cap(equipment: &str) -> i64 {
    if equipment.starts_with("convoy") {
        15
    } else if equipment.contains("heavy") || equipment.contains("carrier") {
        5
    } else {
        10
    }
}

/// What the dockyard rule does with idle dockyards.
#[derive(Debug, PartialEq)]
enum DockAct {
    /// Raise a naval line (entry index, equipment, new requested dockyards).
    Set(i64, String, i64),
    /// Start a line of this equipment with this many dockyards.
    Add(String, i64),
}

/// Idle dockyards (`naval` total minus what the naval lines request) go to the naval lines the
/// plan names (`production[]` with a ship or convoy equipment), the least-manned first, up to the
/// game's cap per line; a planned equipment without a line gets one. What no planned line takes
/// is the convoy rule's (convoys, or `manage.idle_dockyards: "keep"`). Free dockyards stood open
/// 300+ days in gr1-gr6 while the plan's own ship lines ran on one dockyard each.
fn dockyard_decide(lines: &[(i64, String, i64)], planned: &[String], idle: i64) -> Option<DockAct> {
    if idle <= 0 || planned.is_empty() {
        return None;
    }
    let pick = lines
        .iter()
        .filter(|l| l.2 < dockyard_cap(&l.1))
        .filter(|l| planned.iter().any(|p| l.1.starts_with(family(p))))
        .min_by_key(|l| l.2);
    if let Some((index, equipment, have)) = pick {
        let to = (have + idle).min(dockyard_cap(equipment));
        return Some(DockAct::Set(*index, equipment.clone(), to));
    }
    let missing = planned
        .iter()
        .find(|p| !lines.iter().any(|l| l.1.starts_with(family(p))))?;
    Some(DockAct::Add(
        missing.clone(),
        idle.min(dockyard_cap(missing)),
    ))
}

/// What the naval queue rule (PR06/PR07) does with the ship lines the plan does not name.
#[derive(Debug, PartialEq)]
enum NavalQueue {
    /// Remove this line (entry index, equipment, progress of its ship in progress).
    Remove(i64, String, f64),
    /// Every unplanned line holds a ship past half its cost: wait for it (equipment, progress).
    Finish(Vec<(String, f64)>),
}

/// PR07 (start naval queue) and PR06 (finish the unit in progress): a ship line whose hull the
/// plan's `production[]` does not name is removed once its ship in progress is under half done
/// (a ship past half is finished first; the line is removed after it, when the next hull
/// starts from nothing). Convoys stay (the convoy rule owns them). `lines`: (entry index,
/// equipment, progress 0..1 or None when unread).
fn naval_queue_decide(
    lines: &[(i64, String, Option<f64>)],
    planned: &[String],
) -> Option<NavalQueue> {
    let unplanned: Vec<&(i64, String, Option<f64>)> = lines
        .iter()
        .filter(|l| l.1.starts_with("ship_hull"))
        .filter(|l| !planned.iter().any(|p| l.1.starts_with(family(p))))
        .collect();
    if unplanned.is_empty() {
        return None;
    }
    if let Some(l) = unplanned
        .iter()
        .filter_map(|l| l.2.map(|p| (l, p)))
        .filter(|(_, p)| *p < 0.5)
        .min_by(|a, b| a.1.total_cmp(&b.1))
    {
        return Some(NavalQueue::Remove(l.0 .0, l.0 .1.clone(), l.1));
    }
    Some(NavalQueue::Finish(
        unplanned
            .iter()
            .filter_map(|l| l.2.map(|p| (l.1.clone(), p)))
            .collect(),
    ))
}

/// Rule: the start naval queue (manage.off "naval_queue"): hulls the plan does not name are
/// removed once their ship in progress is under half built (PR06, PR07). The game's
/// "outdated equipment in production" alert for a hull the plan does name is the plan's choice
/// (noted with its reason).
fn naval_queue_rule(
    state: &mut State,
    tools: &dyn Tools,
    plan: &Value,
    lease: &str,
    now: GameHour,
) -> Result<Option<String>> {
    if off(plan, "naval_queue") || state.skipped(now, "mnq", "x") {
        return Ok(None);
    }
    state.skip(now, "mnq", "x", 7 * DAY);
    let Ok(read) = all_lines(tools) else {
        return Ok(None);
    };
    let naval: Vec<(i64, String, Option<f64>)> = read
        .iter()
        .filter(|l| l["line_kind"] == json!("naval"))
        .map(|l| {
            (
                l["entry_index"].as_i64().unwrap_or(-1),
                text(&l["equipment_id"]).to_string(),
                l["output"]["progress"].as_f64(),
            )
        })
        .collect();
    let planned: Vec<String> = list(plan, "production")
        .iter()
        .filter(|e| e["remove"] != json!(true))
        .map(|e| text(&e["equipment"]).to_string())
        .filter(|e| e.starts_with("ship_hull"))
        .collect();
    let kept: Vec<String> = naval
        .iter()
        .filter(|l| l.1.starts_with("ship_hull"))
        .filter(|l| planned.iter().any(|p| l.1.starts_with(family(p))))
        .map(|l| l.1.clone())
        .collect();
    if !kept.is_empty() {
        note(
            state,
            "naval_queue",
            format!(
                "ship lines kept because production[] names their hull (an outdated-design alert on them is the plan's choice): {}",
                kept.join(", ")
            ),
        );
    }
    match naval_queue_decide(&naval, &planned) {
        None => Ok(None),
        Some(NavalQueue::Finish(waits)) => {
            note(
                state,
                "naval_queue",
                format!(
                    "unplanned ship lines finish their ship first (past half built): {}",
                    waits
                        .iter()
                        .map(|(e, p)| format!("{e} {:.0}%", p * 100.0))
                        .collect::<Vec<_>>()
                        .join(", ")
                ),
            );
            Ok(None)
        }
        Some(NavalQueue::Remove(index, equipment, progress)) => {
            let (ok, why) = state.act(
                tools,
                lease,
                "hoi4_production_set",
                json!({"line": index, "equipment": equipment, "factories": 0}),
                "nq",
            )?;
            if ok {
                // the next unplanned line in the same tick a day later
                state.skip(now, "mnq", "x", DAY);
            }
            let line = format!(
                "removed unplanned {equipment} line {index} (ship in progress {:.0}%): {ok}",
                progress * 100.0
            );
            note(state, "naval_queue", line.clone());
            Ok(Some(format!("naval queue: {line} {why}")))
        }
    }
}

/// Rule: no idle dockyard (manage.off "dockyards").
fn dockyard_rule(
    state: &mut State,
    tools: &dyn Tools,
    plan: &Value,
    lease: &str,
    now: GameHour,
) -> Result<Option<String>> {
    if off(plan, "dockyards") || state.skipped(now, "mdock", "x") {
        return Ok(None);
    }
    state.skip(now, "mdock", "x", 3 * DAY);
    let economy = tools.call("hoi4_economy", &json!({}))?;
    let Some(total) = economy["factory_trigger_count"]["naval"].as_i64() else {
        return Ok(None);
    };
    let Ok(read) = all_lines(tools) else {
        return Ok(None);
    };
    let naval: Vec<(i64, String, i64)> = read
        .iter()
        .filter(|l| l["line_kind"] == json!("naval"))
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
    let idle = total - naval.iter().map(|l| l.2).sum::<i64>();
    let planned: Vec<String> = list(plan, "production")
        .iter()
        .map(|e| text(&e["equipment"]).to_string())
        .filter(|e| e.starts_with("ship_hull") || e.starts_with("convoy"))
        .collect();
    let Some(act) = dockyard_decide(&naval, &planned, idle) else {
        if idle > 0 {
            note(
                state,
                "dockyards",
                format!("{idle} idle dockyards; every planned ship line is at its cap: the convoy rule's"),
            );
        }
        return Ok(None);
    };
    let (ok, why, line) = match &act {
        DockAct::Set(index, equipment, to) => {
            let (ok, why) = state.act(
                tools,
                lease,
                "hoi4_production_set",
                json!({"line": index, "equipment": equipment, "factories": to}),
                "dk",
            )?;
            (
                ok,
                why,
                format!("{equipment} line {index} -> {to} dockyards"),
            )
        }
        DockAct::Add(equipment, n) => {
            let (ok, why) = state.act(
                tools,
                lease,
                "hoi4_production_add",
                json!({"equipment": equipment, "factories": n}),
                "dk",
            )?;
            (ok, why, format!("new {equipment} line with {n} dockyards"))
        }
    };
    if !ok {
        state.skip(now, "mdock", "x", 14 * DAY);
    }
    note(state, "dockyards", format!("{idle} idle: {line}: {ok}"));
    Ok(Some(format!("dockyards {idle} idle: {line}: {ok} {why}")))
}

pub(super) fn all_lines(tools: &dyn Tools) -> Result<Vec<Value>> {
    const PAGE: usize = 128;
    let mut out: Vec<Value> = Vec::new();
    loop {
        let read = tools.call(
            "hoi4_production_lines",
            &json!({"offset": out.len(), "limit": PAGE}),
        )?;
        let page = list(&read, "lines");
        let n = page.len();
        out.extend(page.iter().cloned());
        if n < PAGE || out.len() >= 2048 {
            return Ok(out);
        }
    }
}

/// The equipment family of a key: the key without a trailing `_<number>` (infantry_equipment_1
/// and infantry_equipment_2 are one family).
pub(super) fn family(key: &str) -> &str {
    match key.rsplit_once('_') {
        Some((head, tail)) if !tail.is_empty() && tail.bytes().all(|b| b.is_ascii_digit()) => head,
        _ => key,
    }
}

/// A military production line as the balance rule sees it.
#[derive(Clone, Debug, PartialEq)]
struct ProdLine {
    index: i64,
    equipment: String,
    archetype: String,
    requested: i64,
    active: i64,
    efficiency: f64,
    start: f64,
}

/// What the balance rule asks for (one action per tick).
#[derive(Clone, Debug, PartialEq)]
enum BalanceAct {
    /// Close a duplicate line that holds nothing worth keeping (young, or no active factory).
    Remove {
        index: i64,
        equipment: String,
        why: String,
    },
    /// Set a line's requested factories.
    Set {
        index: i64,
        equipment: String,
        factories: i64,
        why: String,
    },
    /// A new line for equipment the divisions need and no line makes.
    Add {
        equipment: String,
        factories: i64,
        why: String,
    },
}

/// The inputs of one balance decision (pure, for tests).
#[derive(Default)]
struct BalanceIn {
    lines: Vec<ProdLine>,
    /// Military factories the country has.
    total: i64,
    /// Equipment the planned and fielded divisions still need, by archetype (units).
    need: HashMap<String, f64>,
    stock: HashMap<String, f64>,
    per_day: HashMap<String, f64>,
    /// `production[].factories` by archetype: the target of a line no division needs.
    targets: HashMap<String, i64>,
    /// The variant a new line of an archetype makes (from the templates' equipment).
    variant: HashMap<String, String>,
    /// `production_priority` archetypes, top first (ties go to the higher one).
    priority: Vec<String>,
    /// Lines (equipment) that gave factories away within the last 30 days.
    pulled: HashSet<String>,
    /// The plan's dated lines (`production[].factories` from its `from` date, the latest reached
    /// entry of an archetype): floors filled before any need, from idle factories or from a
    /// line above its own floor (the day-0 split, then the ramps by date).
    floors: HashMap<String, i64>,
    /// The equipment a floor's new line makes (the plan entry's equipment).
    floor_equipment: HashMap<String, String>,
    /// A shortfall this many days of production long or longer may pull from a surplus line.
    pull_days: f64,
}

/// Days of production the archetype's shortfall stands for (0 when its stock covers the need,
/// infinite when nothing makes it).
fn fill_days(b: &BalanceIn, arche: &str) -> f64 {
    let short =
        b.need.get(arche).copied().unwrap_or(0.0) - b.stock.get(arche).copied().unwrap_or(0.0);
    if short <= 0.0 {
        return 0.0;
    }
    let rate = b.per_day.get(arche).copied().unwrap_or(0.0);
    if rate <= 0.0 {
        f64::INFINITY
    } else {
        short / rate
    }
}

fn priority_rank(b: &BalanceIn, arche: &str) -> usize {
    b.priority
        .iter()
        .position(|p| p == arche || family(p) == arche || p.starts_with(arche))
        .unwrap_or(b.priority.len())
}

fn balance_decide(b: &BalanceIn) -> Option<BalanceAct> {
    // 1. one line per equipment: a later duplicate that holds nothing worth keeping is closed
    //    (its factories are free again and go by need); a mature duplicate is kept (PR02).
    let mut seen: HashMap<&str, &ProdLine> = HashMap::new();
    for line in &b.lines {
        match seen.get(line.equipment.as_str()) {
            None => {
                seen.insert(&line.equipment, line);
            }
            Some(first) => {
                // keep the more efficient line (the older on a tie)
                let (keep, drop) = if line.efficiency > first.efficiency {
                    (line, *first)
                } else {
                    (*first, line)
                };
                if drop.active == 0 || drop.efficiency <= drop.start + 5.0 {
                    return Some(BalanceAct::Remove {
                        index: drop.index,
                        equipment: drop.equipment.clone(),
                        why: format!(
                            "a second {} line (line {} at {:.0}%, {} active) beside line {}: one line per equipment",
                            drop.equipment, drop.index, drop.efficiency, drop.active, keep.index
                        ),
                    });
                }
            }
        }
    }
    // 2. no line asks for factories that do not exist: the game gives each new factory to the
    //    first line with an unfilled request, so a large request funnels a year of new
    //    factories into one line (gr1-gr3: rifles 37 requested, 21 active, 200k in stock).
    //    Trimming a request moves no factory and costs no efficiency.
    // Only requests the factories cannot fill are trimmed: a line just given idle factories
    // reads them active a little later, and trimming it then fought the feed (pr1).
    let asked: i64 = b.lines.iter().map(|l| l.requested).sum();
    if let Some(line) = b
        .lines
        .iter()
        .filter(|_| asked > b.total)
        .filter(|l| l.requested > l.active.max(1) + 1)
        .max_by_key(|l| l.requested - l.active)
    {
        return Some(BalanceAct::Set {
            index: line.index,
            equipment: line.equipment.clone(),
            factories: line.active.max(1),
            why: format!(
                "{} asks for {} factories and has {}: the request is trimmed to what exists, new factories go by need",
                line.equipment, line.requested, line.active
            ),
        });
    }
    let requested: i64 = b.lines.iter().map(|l| l.requested).sum();
    let idle = b.total - requested;
    let line_of = |arche: &str| {
        b.lines
            .iter()
            .filter(|l| l.archetype == arche)
            .max_by(|x, y| {
                x.efficiency
                    .partial_cmp(&y.efficiency)
                    .unwrap_or(std::cmp::Ordering::Equal)
                    .then(y.index.cmp(&x.index))
            })
    };
    if let Some(act) = floor_decide(b, idle, &line_of) {
        return Some(act);
    }
    // the archetypes short of what the divisions need, longest shortfall first
    let mut short: Vec<(String, f64)> = b
        .need
        .keys()
        .map(|a| (a.clone(), fill_days(b, a)))
        .filter(|(_, d)| *d > 0.0)
        .collect();
    short.sort_by(|x, y| {
        y.1.partial_cmp(&x.1)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then(priority_rank(b, &x.0).cmp(&priority_rank(b, &y.0)))
            .then(x.0.cmp(&y.0))
    });
    if idle > 0 {
        // 3. idle and new factories by need, in chunks of max(2, a fifth of the line) (PR10)
        for (arche, days) in &short {
            let shown = if days.is_finite() {
                format!("{days:.0} days of production")
            } else {
                "nothing makes it".into()
            };
            // the free factories shared out over the short equipment in one turn, by need
            let share = (idle + short.len() as i64 - 1) / short.len().max(1) as i64;
            match line_of(arche) {
                Some(line) => {
                    let give = idle.min((line.requested / 5).max(2).max(share));
                    return Some(BalanceAct::Set {
                        index: line.index,
                        equipment: line.equipment.clone(),
                        factories: line.requested + give,
                        why: format!(
                            "{idle} idle: +{give} to {} ({arche} short by {shown})",
                            line.equipment
                        ),
                    });
                }
                None => {
                    if let Some(variant) = b.variant.get(arche) {
                        return Some(BalanceAct::Add {
                            equipment: variant.clone(),
                            factories: idle.min(2.max(idle / 4)),
                            why: format!(
                                "{idle} idle: a {variant} line ({arche} short, no line makes it)"
                            ),
                        });
                    }
                }
            }
        }
        // no division is short: lines the plan sizes (planes, trucks for later) up to their target
        let mut planned: Vec<(&ProdLine, i64)> = b
            .targets
            .iter()
            .filter_map(|(a, t)| {
                let have: i64 = b
                    .lines
                    .iter()
                    .filter(|l| &l.archetype == a)
                    .map(|l| l.requested)
                    .sum();
                line_of(a).filter(|_| have < *t).map(|l| (l, *t - have))
            })
            .collect();
        planned.sort_by_key(|(l, gap)| (priority_rank(b, &l.archetype), -gap, l.index));
        if let Some((line, gap)) = planned.first() {
            let give = idle.min(*gap);
            return Some(BalanceAct::Set {
                index: line.index,
                equipment: line.equipment.clone(),
                factories: line.requested + give,
                why: format!(
                    "{idle} idle, no division short: +{give} to {} (plan target {})",
                    line.equipment,
                    line.requested + gap
                ),
            });
        }
        return None;
    }
    // 4. no idle factory: a long shortfall may pull from a line with a large surplus, at most a
    //    fifth of it and once in 30 days per line (PR09: a moved factory restarts at the start
    //    efficiency, so pulls are rare and small). The freed factories go by need next tick.
    let (needy, days) = short.first()?;
    if *days < b.pull_days {
        return None;
    }
    let donor = b
        .lines
        .iter()
        .filter(|l| l.requested >= 5 && l.archetype != *needy && !b.pulled.contains(&l.equipment))
        .filter(|l| {
            let stock = b.stock.get(&l.archetype).copied().unwrap_or(0.0);
            let need = b.need.get(&l.archetype).copied().unwrap_or(0.0);
            let planned = b.targets.get(&l.archetype).copied().unwrap_or(0);
            let floor = b.floors.get(&l.archetype).copied().unwrap_or(0);
            stock > 0.0 && stock >= 2.0 * need && l.requested > planned.max(floor)
        })
        .max_by_key(|l| (l.requested, -l.index))?;
    let take = (donor.requested / 5).max(1);
    let shown = if days.is_finite() {
        format!("{days:.0} days")
    } else {
        "no line".into()
    };
    Some(BalanceAct::Set {
        index: donor.index,
        equipment: donor.equipment.clone(),
        factories: donor.requested - take,
        why: format!(
            "{needy} short by {shown}, no idle factory: {take} of {} factories leave {} (stock {:.0} covers twice its need {:.0})",
            donor.requested,
            donor.equipment,
            b.stock.get(&donor.archetype).copied().unwrap_or(0.0),
            b.need.get(&donor.archetype).copied().unwrap_or(0.0)
        ),
    })
}

/// Factories an archetype's lines ask for.
fn asked_for(b: &BalanceIn, arche: &str) -> i64 {
    b.lines
        .iter()
        .filter(|l| l.archetype == arche)
        .map(|l| l.requested)
        .sum()
}

/// Step 2b of the balance: the plan's dated lines first. A line under its floor gets the idle
/// factories (a new line when none runs); with none idle, a line above its own floor (or one
/// the plan neither sizes nor needs) gives the gap, at once: a ramp is the plan's decision and
/// a factory idle or on the wrong line loses more than the efficiency a move costs.
fn floor_decide<'a>(
    b: &'a BalanceIn,
    idle: i64,
    line_of: &dyn Fn(&str) -> Option<&'a ProdLine>,
) -> Option<BalanceAct> {
    let mut gaps: Vec<(&String, i64)> = b
        .floors
        .iter()
        .map(|(a, f)| (a, f - asked_for(b, a)))
        .filter(|(_, g)| *g > 0)
        .collect();
    gaps.sort_by(|x, y| {
        priority_rank(b, x.0)
            .cmp(&priority_rank(b, y.0))
            .then(y.1.cmp(&x.1))
            .then(x.0.cmp(y.0))
    });
    for (arche, gap) in gaps {
        let floor = b.floors[arche];
        if idle > 0 {
            let give = idle.min(gap);
            return match line_of(arche) {
                Some(line) => Some(BalanceAct::Set {
                    index: line.index,
                    equipment: line.equipment.clone(),
                    factories: line.requested + give,
                    why: format!(
                        "{idle} idle: +{give} to {} (the plan's line is {floor} factories by now)",
                        line.equipment
                    ),
                }),
                None => b
                    .floor_equipment
                    .get(arche)
                    .or_else(|| b.variant.get(arche))
                    .map(|v| BalanceAct::Add {
                        equipment: v.clone(),
                        factories: give,
                        why: format!("{idle} idle: the plan's {v} line ({floor} factories by now)"),
                    }),
            };
        }
        let Some(line) = line_of(arche) else {
            continue;
        };
        let surplus = |l: &ProdLine| match b.floors.get(&l.archetype) {
            Some(f) => l.requested - f,
            None if fill_days(b, &l.archetype) <= 0.0 && !b.targets.contains_key(&l.archetype) => {
                l.requested
            }
            None => 0,
        };
        let Some(donor) = b
            .lines
            .iter()
            .filter(|l| l.archetype != *arche && surplus(l) > 0)
            .max_by_key(|l| (surplus(l), -l.index))
        else {
            continue;
        };
        let take = gap.min(surplus(donor));
        return Some(BalanceAct::Set {
            index: donor.index,
            equipment: donor.equipment.clone(),
            factories: donor.requested - take,
            why: format!(
                "no idle factory: {take} of {} factories leave {} (above its plan line) for {} (the plan's line is {floor}, it asks {})",
                donor.requested, donor.equipment, line.equipment, line.requested
            ),
        });
    }
    None
}

/// The plan's dated production floors by archetype: the latest reached `production` entry with
/// `factories` (not a `remove` or a `replace`), and the equipment it names.
fn production_floors(
    plan: &Value,
    stock: &Value,
    now: GameHour,
) -> (HashMap<String, i64>, HashMap<String, String>) {
    let mut floors: HashMap<String, (GameHour, i64)> = HashMap::new();
    let mut equipment: HashMap<String, String> = HashMap::new();
    // opt-in (`production_floors: true`): a plan whose lines are only a day-0 suggestion keeps
    // the balance by need alone
    if plan["production_floors"] != json!(true) {
        return (HashMap::new(), HashMap::new());
    }
    for entry in list(plan, "production") {
        let eq = text(&entry["equipment"]);
        let Some(f) = entry["factories"].as_i64() else {
            continue;
        };
        if eq.is_empty()
            || entry["remove"] == json!(true)
            || !entry["replace"].is_null()
            || entry["floor"] == json!(false)
            || !super::reached(entry, now)
        {
            continue;
        }
        let from = match entry.get("from") {
            Some(v) if !v.is_null() => crate::alarms::stop_on_date(&json!({"stop_on_date": v}))
                .ok()
                .flatten()
                .map_or(0, |(h, _)| h),
            _ => 0,
        };
        let a = archetype_row(stock, eq).unwrap_or_else(|| family(eq).to_string());
        if floors.get(&a).is_none_or(|(at, _)| from >= *at) {
            floors.insert(a.clone(), (from, f));
            equipment.insert(a, eq.to_string());
        }
    }
    (
        floors.into_iter().map(|(a, (_, f))| (a, f)).collect(),
        equipment,
    )
}

/// The training entries whose `when` holds now (the planned force the equipment is for).
fn active_training(
    state: &mut State,
    tools: &dyn Tools,
    plan: &Value,
    now: GameHour,
) -> Vec<(String, Option<u64>)> {
    let entries: Vec<Value> = match &plan["training"] {
        entry @ Value::Object(_) => vec![entry.clone()],
        Value::Array(items) => items.iter().filter(|v| v.is_object()).cloned().collect(),
        _ => Vec::new(),
    };
    let mut out: Vec<(String, Option<u64>)> = Vec::new();
    for e in entries {
        let template = text(&e["template"]).to_string();
        if template.is_empty() || when_holds(state, tools, &e["when"], now) != Truth::True {
            continue;
        }
        let cap = e["cap"].as_u64();
        match out.iter_mut().find(|(t, _)| *t == template) {
            Some(slot) => slot.1 = slot.1.max(cap),
            None => out.push((template, cap)),
        }
    }
    out
}

/// Rule: production balance (manage.off "balance" or "stockpile" turns it off). Free military
/// factories are all given in one turn: the balance decides again after each line it feeds (by
/// need, from fresh reads) until none is free, at most BALANCE_TURN_SETS lines (a free factory
/// loses its output the day it is free: it27c had 10 free on day 1, fed two a turn, for 50 days).
fn balance_rule(
    state: &mut State,
    tools: &dyn Tools,
    plan: &Value,
    lease: &str,
    now: GameHour,
) -> Result<Option<String>> {
    let mut lines: Vec<String> = Vec::new();
    while lines.len() < BALANCE_TURN_SETS {
        let Some(line) = balance_once(state, tools, plan, lease, now)? else {
            break;
        };
        if lines.last() == Some(&line) {
            // the read did not show the last order: no second order on a stale read
            break;
        }
        // only the free factories are given on: a pull or a closed duplicate ends the turn
        let more = !line.contains(": false") && line.contains(" idle");
        lines.push(line);
        if !more {
            break;
        }
    }
    Ok((!lines.is_empty()).then(|| lines.join("; ")))
}

/// Most production lines the balance feeds in one turn.
const BALANCE_TURN_SETS: usize = 8;

/// One balance decision and its action (see `balance_rule`).
fn balance_once(
    state: &mut State,
    tools: &dyn Tools,
    plan: &Value,
    lease: &str,
    now: GameHour,
) -> Result<Option<String>> {
    // it works from the stockpile read: off with the stockpile rules too
    if off(plan, "balance") || off(plan, "stockpile") || state.skipped(now, "mbal", "x") {
        return Ok(None);
    }
    let rest = |state: &mut State, why: String| {
        note(state, "production_balance", why);
        state.skip(now, "mbal", "x", DAY);
        Ok(None)
    };
    let Ok(economy) = tools.call("hoi4_economy", &json!({})) else {
        return rest(state, "unknown: hoi4_economy unreadable".into());
    };
    let Some(total) = economy["factory_trigger_count"]["military"].as_i64() else {
        return rest(state, "unknown: no military factory count".into());
    };
    let Ok(rows) = all_lines(tools) else {
        return rest(state, "unknown: hoi4_production_lines unreadable".into());
    };
    let Some(stock) = stockpile(state, tools, now) else {
        return rest(state, "unknown: hoi4_stockpile unreadable".into());
    };
    let mut b = BalanceIn {
        total,
        pull_days: plan["manage"]["balance_pull_days"]
            .as_f64()
            .unwrap_or(120.0),
        ..Default::default()
    };
    for line in rows.iter().filter(|l| l["line_kind"] == json!("military")) {
        let equipment = text(&line["equipment_id"]).to_string();
        if equipment.is_empty() {
            continue;
        }
        let archetype =
            archetype_row(&stock, &equipment).unwrap_or_else(|| family(&equipment).to_string());
        b.lines.push(ProdLine {
            index: line["entry_index"].as_i64().unwrap_or(-1),
            equipment,
            archetype,
            requested: line["factory_counters"]["requested_factories_raw_signed"]
                .as_i64()
                .unwrap_or(0),
            // an unread active count is no evidence of an unfilled request
            active: line["factory_counters"]["active_factories_raw_signed"]
                .as_i64()
                .unwrap_or_else(|| {
                    line["factory_counters"]["requested_factories_raw_signed"]
                        .as_i64()
                        .unwrap_or(0)
                }),
            efficiency: line["output"]["efficiency"].as_f64().unwrap_or(0.0),
            start: line["output"]["efficiency_start"].as_f64().unwrap_or(10.0),
        });
    }
    let reserve_share = plan["manage"]["balance_reserve"]
        .as_f64()
        .filter(|r| *r >= 0.0)
        .unwrap_or(0.8);
    for row in list(&stock, "equipment") {
        let a = text(&row["archetype"]).to_string();
        if a.is_empty() {
            continue;
        }
        b.stock
            .insert(a.clone(), row["stockpile"].as_f64().unwrap_or(0.0));
        b.per_day
            .insert(a.clone(), row["produced_per_day"].as_f64().unwrap_or(0.0));
        let short = row["fielded_short"].as_f64().unwrap_or(0.0);
        // a reserve in stock against attrition and new divisions: the game's own AI keeps 0.8 of
        // the equipment in use (PRODUCTION_EQUIPMENT_SURPLUS_FACTOR); without it a line whose
        // planned force was complete counted as surplus and gave its factories away (pr1,
        // 17 Nov 2026: rifles 14 -> 11 and the worst-covered equipment by 1938)
        let reserve = reserve_share * row["fielded_need"].as_f64().unwrap_or(0.0);
        if short + reserve > 0.0 {
            *b.need.entry(a).or_insert(0.0) += short + reserve;
        }
    }
    // the planned force: divisions the active training entries still add, times each template's need
    let training = active_training(state, tools, plan, now);
    if !training.is_empty() {
        let fielded: HashMap<String, u64> = supply(state, tools, now)
            .map(|s| {
                let mut m = HashMap::new();
                for d in list(&s, "divisions") {
                    *m.entry(text(&d["template"]).to_string()).or_insert(0) += 1;
                }
                m
            })
            .unwrap_or_default();
        for (template, cap) in training {
            let fresh = state
                .manage
                .tpl_need
                .get(&template)
                .is_some_and(|(at, _, _)| now - *at < 30 * DAY);
            if !fresh {
                if let Ok(read) = tools.call("hoi4_template_get", &json!({"template": template})) {
                    let t = &list(&read, "templates")
                        .first()
                        .cloned()
                        .unwrap_or(Value::Null);
                    let need: HashMap<String, f64> = t["totals"]["equipment_need"]
                        .as_object()
                        .map(|m| {
                            m.iter()
                                .filter_map(|(k, v)| v.as_f64().map(|n| (k.clone(), n)))
                                .collect()
                        })
                        .unwrap_or_default();
                    let variants: HashMap<String, String> = list(t, "equipment_variants")
                        .iter()
                        .filter_map(|v| {
                            let (a, k) = (text(&v["archetype"]), text(&v["variant"]));
                            (!a.is_empty() && !k.is_empty()).then(|| (a.to_string(), k.to_string()))
                        })
                        .collect();
                    state
                        .manage
                        .tpl_need
                        .insert(template.clone(), (now, need, variants));
                }
            }
            let Some((_, per_div, variants)) = state.manage.tpl_need.get(&template) else {
                continue;
            };
            let have = fielded.get(&template).copied().unwrap_or(0);
            // without a cap the next queue of divisions stands for the plan (no endless need)
            let more = match cap {
                Some(cap) => cap.saturating_sub(have),
                None => 8,
            } as f64;
            for (a, n) in per_div {
                *b.need.entry(a.clone()).or_insert(0.0) += more * n;
            }
            for (a, v) in variants {
                b.variant.entry(a.clone()).or_insert_with(|| v.clone());
            }
        }
    }
    for entry in list(plan, "production") {
        let equipment = text(&entry["equipment"]);
        if equipment.is_empty() || entry["remove"] == json!(true) {
            continue;
        }
        let a = archetype_row(&stock, equipment).unwrap_or_else(|| family(equipment).to_string());
        if let Some(f) = entry["factories"].as_i64() {
            if !b.need.contains_key(&a) {
                b.targets.insert(a, f);
            }
        }
    }
    (b.floors, b.floor_equipment) = production_floors(plan, &stock, now);
    b.priority = strings(plan, "production_priority")
        .iter()
        .map(|k| archetype_row(&stock, k).unwrap_or_else(|| family(k).to_string()))
        .collect();
    b.pulled = state
        .manage
        .pulled
        .iter()
        .filter(|(_, at)| now - **at < 30 * DAY)
        .map(|(e, _)| e.clone())
        .collect();
    let mut needs: Vec<String> = b
        .need
        .keys()
        .map(|a| {
            let d = fill_days(&b, a);
            format!(
                "{a} {}",
                if d.is_finite() {
                    format!("{d:.0}d")
                } else {
                    "no line".into()
                }
            )
        })
        .collect();
    needs.sort();
    let Some(act) = balance_decide(&b) else {
        // military factories on no line never wait for a shortfall: the balance found nothing to
        // move, so the idle ones go to the priority lines (g17i, 18 Nov 2026: 38 factories, 30
        // requested, 8 idle for 361 days under a stale "no free military factory")
        let requested: i64 = b.lines.iter().map(|l| l.requested).sum();
        if total > requested {
            let since = *state.done.entry("milidle".into()).or_insert(now);
            // a free factory loses its output the day it is free: fed the first day it is seen
            // (the rules above had their turn in this pass: the balance found nothing to move)
            let _ = since;
            if !state.skipped(now, "prodset", "x") {
                let (military, idle) = military_lines(tools, &economy)?;
                if idle > 0 {
                    if let Some(line) = feed(state, tools, plan, lease, now, &military, idle)? {
                        return Ok(Some(line));
                    }
                }
            }
        } else {
            state.done.remove("milidle");
        }
        return rest(
            state,
            format!(
                "{} military lines, {} factories, {} requested; shortfall in production days: {}",
                b.lines.len(),
                total,
                b.lines.iter().map(|l| l.requested).sum::<i64>(),
                if needs.is_empty() {
                    "none".into()
                } else {
                    needs.join(", ")
                }
            ),
        );
    };
    let (ok, why, what) = match &act {
        BalanceAct::Remove {
            index,
            equipment,
            why,
        } => {
            let (ok, r) = state.act(
                tools,
                lease,
                "hoi4_production_set",
                json!({"line": index, "equipment": equipment, "factories": 0}),
                "bl",
            )?;
            (ok, r, why.clone())
        }
        BalanceAct::Set {
            index,
            equipment,
            factories,
            why,
        } => {
            let before = b
                .lines
                .iter()
                .find(|l| l.index == *index)
                .map_or(0, |l| l.requested);
            let (ok, r) = state.act(
                tools,
                lease,
                "hoi4_production_set",
                json!({"line": index, "equipment": equipment, "factories": factories}),
                "bl",
            )?;
            if ok && *factories < before && why.contains("leave") {
                state.manage.pulled.insert(equipment.clone(), now);
            }
            (ok, r, why.clone())
        }
        BalanceAct::Add {
            equipment,
            factories,
            why,
        } => {
            let (ok, r) = state.act(
                tools,
                lease,
                "hoi4_production_add",
                json!({"equipment": equipment, "factories": factories}),
                "bl",
            )?;
            if ok {
                state.done.insert(format!("prod:{equipment}"), now);
            }
            (ok, r, why.clone())
        }
    };
    if !ok {
        state.skip(now, "mbal", "x", 2 * DAY);
    }
    note(state, "production_balance", format!("{what}: {ok}"));
    Ok(Some(format!("production balance: {what}: {ok} {why}")))
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
        /// The refusal's message (default "refused by the fake").
        message: RefCell<Option<&'static str>>,
    }
    impl Tools for Fake {
        fn call(&self, name: &str, args: &Value) -> Result<Value> {
            self.calls
                .borrow_mut()
                .push((name.to_string(), args.clone()));
            if args.get("request_id").is_some() {
                if let Some(code) = self.refuse.borrow().get(name) {
                    let message = self.message.borrow().unwrap_or("refused by the fake");
                    return Err(Error::new(code, message));
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
    #[test]
    fn an_officer_corps_date_is_the_latest_day_not_the_first() {
        assert_eq!(without_dates(&json!("date >= 1938.3.1")), Value::Null);
        assert_eq!(
            without_dates(&json!({"all": ["date >= 1938.3.1", "xp:army >= 20"]})),
            json!({"all": ["xp:army >= 20"]})
        );
        assert_eq!(
            without_dates(&json!({"any": ["date >= 1938.3.1", "at_war"]})),
            json!({"any": ["date >= 1938.3.1", "at_war"]})
        );
    }

    #[test]
    fn a_template_edit_waits_while_the_stock_cannot_equip_the_fielded_divisions() {
        let inf = json!({"unit": "infantry", "equipment_need": {"infantry_equipment": 100.0}});
        let art = json!({"unit": "artillery_brigade",
                         "equipment_need": {"artillery_equipment": 24.0}});
        let read = json!({"templates": [
            {"template": "Inf", "divisions_using_template": 38, "units": [inf.clone(), inf.clone()]},
            {"template": "Art", "divisions_using_template": 1, "units": [art.clone()]}]});
        let stock = |cover: f64| {
            json!({"equipment": [{"archetype": "artillery_equipment", "stockpile": cover,
                                  "cover_after_shortfall": cover}]})
        };
        let regiments = json!([
            ["infantry", "infantry"],
            ["artillery_brigade", "artillery_brigade"]
        ]);
        let tools = fake(vec![
            ("hoi4_template_get", read.clone()),
            ("hoi4_stockpile", stock(40.0)),
        ]);
        let short = edit_stock_short(&tools, "Inf", &regiments, &[]).unwrap();
        assert!(
            short.starts_with("artillery_equipment 40 < 1824 for 38"),
            "{short}"
        );
        let tools = fake(vec![
            ("hoi4_template_get", read.clone()),
            ("hoi4_stockpile", stock(2000.0)),
        ]);
        assert_eq!(edit_stock_short(&tools, "Inf", &regiments, &[]), None);
        // a battalion no own template carries: not judged here
        let other = json!([["infantry", "infantry"], ["anti_tank_brigade"]]);
        let tools = fake(vec![
            ("hoi4_template_get", read),
            ("hoi4_stockpile", stock(0.0)),
        ]);
        assert_eq!(edit_stock_short(&tools, "Inf", &other, &[]), None);
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
            message: RefCell::new(None),
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
    fn a_long_open_alert_carries_its_owning_steps_reason_wherever_the_step_keeps_it() {
        let tools = fake(vec![(
            "hoi4_alerts",
            json!({"alerts": [{"frame": 67, "alert": "admiral_assignable"},
                {"frame": 59, "alert": "special_project_available"},
                {"frame": 57, "alert": "mio_traits"}]}),
        )]);
        let plan = json!({"navy": {"posture": "coastal"}, "special_projects": {"priority": []},
            "mio": [{"organisation": "M"}],
            "manage": {"alert_persist_days": 3, "alerts_ignore": ["mio_traits"],
                "alerts_ignore_why": {"mio_traits": "the player queues traits"},
                "off": ["manpower", "stability", "equipment_fill", "resources", "stockpile", "trucks"]}});
        let mut state = State::default();
        // g17a (17 Nov 2026): the navy and affairs steps keep their notes outside `manage`
        super::super::plan_military::put_note(&mut state, "navy", "Flotta 1 is the reserve fleet");
        super::super::plan_affairs::put_note(&mut state, "mio_traits", "M: no designer slot");
        note(&mut state, "special_projects", "facility 3 busy");
        for day in 0..3 {
            step(&mut state, &tools, &plan, "l", START + day * DAY).unwrap();
            assert!(wakes(&mut state).is_empty(), "day {day}");
        }
        step(&mut state, &tools, &plan, "l", START + 3 * DAY).unwrap();
        let alarms = wakes(&mut state);
        let messages: Vec<&str> = alarms
            .iter()
            .filter(|a| {
                a["reason"]
                    .as_str()
                    .is_some_and(|r| r.starts_with("alert:"))
            })
            .filter_map(|a| a["message"].as_str())
            .collect();
        assert_eq!(messages.len(), 3, "{messages:?}");
        assert!(messages[0].ends_with("; navy: Flotta 1 is the reserve fleet"));
        assert!(messages[1].ends_with("; special_projects: facility 3 busy"));
        // the ignored alert is no longer dropped unseen: once per spell, with both reasons
        assert!(messages[2].contains("the plan ignores it (the player queues traits)"));
        assert!(messages[2].ends_with("; mio_traits: M: no designer slot"));
        step(&mut state, &tools, &plan, "l", START + 20 * DAY).unwrap();
        assert!(!wakes(&mut state)
            .iter()
            .any(|a| a["reason"] == "alert:mio_traits"));
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
        // each refusal rests the identical order (1, then 3, then 9 game hours)
        for hour in [START, START + 1, START + 4, START + 5] {
            state.clock = hour;
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
        step(&mut state, &tools, &json!({}), "l", START + 5).unwrap();
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

    #[test]
    fn a_deploy_state_the_game_refuses_rests_that_entry_with_its_reason() {
        let tools = fake(vec![(
            "hoi4_division_templates",
            templates_read(&[("Inf", 0)]),
        )]);
        let plan = json!({"training": [
            {"template": "Inf", "queue": 2, "deploy_state": 5},
            {"template": "Inf", "queue": 2, "deploy_state": 6}]});
        let mut state = State::default();
        tools
            .refuse
            .borrow_mut()
            .insert("hoi4_train".into(), "input_readback_failed");
        *tools.message.borrow_mut() =
            Some("No location was set: the game does not let these divisions deploy in that state");
        let out = training_step(&mut state, &tools, &plan, "l", START).unwrap();
        assert!(out.unwrap().contains(": false"));
        assert!(state.manage.notes["training"].contains("refuses deploy state 5"));
        // the next tick trains the other entry; the refused one is not sent again
        tools.refuse.borrow_mut().clear();
        let out = training_step(&mut state, &tools, &plan, "l", START + 1).unwrap();
        assert_eq!(out.as_deref(), Some("train 2 Inf: true Verified"));
        let sent = actions(&tools);
        assert_eq!(sent.len(), 2);
        assert_eq!(sent[1].1["deploy_state_id"], 6);
        // any other refusal holds the template 2 days, with the reason noted
        tools
            .refuse
            .borrow_mut()
            .insert("hoi4_train".into(), "action_not_applied");
        *tools.message.borrow_mut() = None;
        let mut state = State::default();
        training_step(&mut state, &tools, &plan, "l", START).unwrap();
        assert!(state.manage.notes["training"].contains("tried again in 2 days"));
        assert_eq!(
            training_step(&mut state, &tools, &plan, "l", START + DAY).unwrap(),
            None
        );
    }

    #[test]
    fn entries_of_one_template_add_their_queues() {
        // 5 lines queued: the first entry (queue 5) is full, the second (queue 4) brings the
        // template to 9 and trains in its own deploy state; a third entry whose `when` does not
        // hold adds nothing.
        let tools = fake(vec![(
            "hoi4_division_templates",
            templates_read(&[("Inf", 5)]),
        )]);
        let plan = json!({"training": [
            {"template": "Inf", "queue": 5, "batch": 2, "deploy_state": 5},
            {"template": "Inf", "queue": 4, "batch": 3, "deploy_state": 6},
            {"template": "Inf", "queue": 4, "deploy_state": 7, "when": "date:2000.1.1"}]});
        let mut state = State::default();
        let out = training_step(&mut state, &tools, &plan, "l", START).unwrap();
        assert_eq!(out.as_deref(), Some("train 3 Inf: true Verified"));
        let sent = actions(&tools);
        assert_eq!(sent[0].1["deploy_state_id"], 6);
        // 9 queued: every holding entry is full
        let tools = fake(vec![(
            "hoi4_division_templates",
            templates_read(&[("Inf", 9)]),
        )]);
        let mut state = State::default();
        assert_eq!(
            training_step(&mut state, &tools, &plan, "l", START).unwrap(),
            None
        );
    }

    fn division(template: &str, have: u64, need: u64) -> Value {
        json!({"template": template, "army": "Army 1", "equipment_fill_min": have as f64 / need as f64,
            "equipment": [{"archetype": "infantry_equipment", "have": have, "need": need}],
            "short_equipment": if have < need { json!(["infantry_equipment"]) } else { json!([]) }})
    }

    #[test]
    fn a_donor_line_never_becomes_the_binding_equipment_of_the_main_template() {
        // g17b 1937.05: artillery 2 factories make 1.8 a day (0.15 divisions a day at 12 each),
        // support equipment 8 make 3.6 (0.12 at 30 each)
        let supply = json!({"divisions": [{"template": "Inf", "equipment": [
            {"archetype": "artillery_equipment", "have": 12, "need": 12},
            {"archetype": "support_equipment", "have": 28, "need": 30}]}]});
        let stock = json!({"equipment": [
            {"archetype": "artillery_equipment", "stockpile": 40.0, "produced_per_day": 1.8,
             "factories": 2, "fielded_short": 0.0, "variants": [{"type": "artillery_equipment_1"}]},
            {"archetype": "support_equipment", "stockpile": 0.0, "produced_per_day": 3.6,
             "factories": 8, "fielded_short": 2.0, "variants": [{"type": "support_equipment_1"}]}]});
        let rates = division_rates(&supply, &stock);
        assert!((rates["artillery_equipment"].0 - 0.15).abs() < 1e-9);
        assert!((rates["support_equipment"].0 - 0.12).abs() < 1e-9);
        let lines: Vec<MilLine> = vec![(1, "artillery_equipment_1".into(), 2, None)];
        // taking one artillery factory leaves 3.3 + 0.075 x 180 = 16.8 divisions in half a year
        // against support's 0 + 0.135 x 180 = 24.3: refused
        assert_eq!(
            donor_line(
                &stock,
                &json!({}),
                &lines,
                "support_equipment",
                10.0,
                3,
                &rates
            ),
            None
        );
        // without rates (no division read) the old rule decides
        assert!(donor_line(
            &stock,
            &json!({}),
            &lines,
            "support_equipment",
            10.0,
            3,
            &HashMap::new()
        )
        .is_none_or(|(l, _)| l.1 == "artillery_equipment_1"));
    }

    #[test]
    fn a_donor_keeps_the_factories_the_plan_gives_its_line() {
        // g17f: the fighter line (no stock: its planes become wings) donated 5 -> 1
        let stock = json!({"equipment": [
            {"archetype": "small_plane_airframe", "stockpile": 0.0, "produced_per_day": 0.5,
             "factories": 5, "fielded_short": 0.0, "variants": [{"type": "small_plane_airframe_1"}]}]});
        let lines: Vec<MilLine> = vec![(4, "small_plane_airframe_1".into(), 5, None)];
        let plan = json!({"production": [{"equipment": "small_plane_airframe_1", "factories": 6}]});
        let rates = HashMap::new();
        assert!(donor_line(&stock, &plan, &lines, "support_equipment", 10.0, 3, &rates).is_none());
        // above the plan's count it gives only what lies above it
        let lines: Vec<MilLine> = vec![(4, "small_plane_airframe_1".into(), 7, None)];
        if let Some((_, n)) =
            donor_line(&stock, &plan, &lines, "support_equipment", 10.0, 3, &rates)
        {
            assert_eq!(n, 1);
        }
    }

    #[test]
    fn a_donor_holding_stock_for_many_divisions_gives_at_an_equal_daily_rate() {
        // g17c 1937.01: rifles 16 factories with 21289 in stock (21 divisions), support
        // equipment 6 factories with 64 (2 divisions), both near 0.14 divisions a day
        let supply = json!({"divisions": [{"template": "Inf", "equipment": [
            {"archetype": "infantry_equipment", "have": 1012, "need": 1012},
            {"archetype": "support_equipment", "have": 30, "need": 30}]}]});
        let stock = json!({"equipment": [
            {"archetype": "infantry_equipment", "stockpile": 21289.0, "produced_per_day": 140.0,
             "days_of_cover": 383.0, "factories": 16, "fielded_short": 0.0, "variants": [{"type": "infantry_equipment_1"}]},
            {"archetype": "support_equipment", "stockpile": 64.0, "produced_per_day": 4.2,
             "factories": 6, "fielded_short": 0.0, "variants": [{"type": "support_equipment_1"}]}]});
        let rates = division_rates(&supply, &stock);
        assert!((rates["infantry_equipment"].2 - 21289.0 / 1012.0).abs() < 1e-9);
        let lines: Vec<MilLine> = vec![(1, "infantry_equipment_1".into(), 16, None)];
        let got = donor_line(
            &stock,
            &json!({}),
            &lines,
            "support_equipment",
            10.0,
            3,
            &rates,
        );
        assert_eq!(
            got.map(|(l, n)| (l.1, n)),
            Some(("infantry_equipment_1".into(), 3))
        );
    }

    #[test]
    fn training_waits_for_one_batch_of_every_equipment_the_template_needs() {
        let fielded = vec![json!({"template": "Inf", "army": "Army 1", "equipment": [
            {"archetype": "infantry_equipment", "have": 900, "need": 900},
            {"archetype": "support_equipment", "have": 28, "need": 30}]})];
        let stock = |support: f64| {
            json!({"equipment": [
                {"archetype": "infantry_equipment", "stockpile": 2000.0, "variants": []},
                {"archetype": "support_equipment", "stockpile": support, "variants": []}]})
        };
        let tools = fake(vec![
            ("hoi4_division_templates", templates_read(&[("Inf", 0)])),
            ("hoi4_supply", json!({"divisions": fielded})),
            ("hoi4_stockpile", stock(10.0)),
        ]);
        // the plan guards rifles only; support equipment binds: 10 held, a batch of 2 needs 60
        let plan = json!({"training": {"template": "Inf", "queue": 4, "batch": 2, "deploy_state": 5,
            "only_if_stockpile": {"infantry_equipment": 600}}});
        let mut state = State::default();
        assert_eq!(
            training_step(&mut state, &tools, &plan, "l", START).unwrap(),
            None
        );
        assert!(
            state.manage.notes["training"]
                .contains("support_equipment stockpile 10 below one batch's need 60"),
            "{}",
            state.manage.notes["training"]
        );
        assert!(actions(&tools).is_empty());
        tools
            .replies
            .borrow_mut()
            .insert("hoi4_stockpile".into(), stock(70.0));
        let mut state = State::default();
        assert_eq!(
            training_step(&mut state, &tools, &plan, "l", START)
                .unwrap()
                .as_deref(),
            Some("train 2 Inf: true Verified")
        );
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
        // refused once: the identical exit is not sent again unchanged in the same moment
        let out = peace(&mut state, &tools, &plan, "l", START).unwrap();
        assert!(matches!(out, Tick::Acted(s) if s.contains(": false")));
        let out = peace(&mut state, &tools, &plan, "l", START).unwrap();
        assert!(matches!(out, Tick::Stop(_)), "{out:?}");
        assert_eq!(actions(&tools).len(), 1);
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
    fn a_priority_line_raise_buys_the_country_deficit_and_a_capped_budget_says_why() {
        // a priority line short of 30 rubber: one civilian factory buys about 8 units, so +4
        let tools = fake(vec![
            (
                "hoi4_production_shortages",
                json!({"lines": [{"equipment": "motorized_equipment_1", "short_of": ["rubber"],
                    "resources": [{"resource": "rubber"}]}]}),
            ),
            (
                "hoi4_resources",
                json!({"resources": {"rubber": {"shortage": {"short": true, "deficit": 30.0}}}}),
            ),
            (
                "hoi4_auto_import",
                json!({"auto_import": {"enabled": true, "budget_factories": 3, "available_factories": 20}}),
            ),
        ]);
        let plan = json!({"auto_import": {"budget": 3, "budget_max": 12},
            "production_priority": ["motorized_equipment"],
            "manage": {"off": ["manpower", "stability", "equipment_fill", "stockpile", "trucks"]}});
        let mut state = State::default();
        step(&mut state, &tools, &plan, "l", START).unwrap();
        let out = step(&mut state, &tools, &plan, "l", START + 7 * DAY).unwrap();
        assert_eq!(
            out.as_deref(),
            Some("auto_import budget 3 -> 7 for rubber: true Verified")
        );
        // e14: the game offers no civilian factory for trade (consumer goods take the last one)
        let tools = fake(vec![
            (
                "hoi4_production_shortages",
                json!({"lines": [{"equipment": "infantry_equipment_0", "short_of": ["steel"],
                    "resources": [{"resource": "steel"}]}]}),
            ),
            (
                "hoi4_resources",
                json!({"resources": {"steel": {"shortage": {"short": true, "deficit": 4.0}}}}),
            ),
            (
                "hoi4_auto_import",
                json!({"auto_import": {"enabled": true, "budget_factories": 0, "available_factories": 0}}),
            ),
        ]);
        let plan = json!({"auto_import": {"budget": 1}, "production_priority": ["infantry_equipment"],
            "manage": {"off": ["manpower", "stability", "equipment_fill", "stockpile", "trucks"]}});
        let mut state = State::default();
        for d in [0, 7, 14] {
            step(&mut state, &tools, &plan, "l", START + d * DAY).unwrap();
        }
        let last = wakes(&mut state).pop().unwrap();
        assert_eq!(last["reason"], "resource_deficit");
        let message = last["message"].as_str().unwrap();
        assert!(message.contains("imports cannot grow"), "{message}");
        assert!(
            message.contains("budget 0 of 0 civilian factories"),
            "{message}"
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
    fn naval_queue_removes_unplanned_hulls_under_half_and_finishes_the_rest() {
        let lines = vec![
            (8, "ship_hull_submarine_1".to_string(), Some(0.7)),
            (12, "ship_hull_light_2".to_string(), Some(0.2)),
            (14, "ship_hull_cruiser_panzerschiff".to_string(), Some(0.05)),
            (15, "convoy_1".to_string(), Some(0.0)),
        ];
        // the plan names cruisers: the Panzerschiff line is a cruiser hull and stays
        let planned = vec!["ship_hull_cruiser_2".to_string()];
        assert_eq!(
            naval_queue_decide(&lines, &planned),
            Some(NavalQueue::Remove(12, "ship_hull_light_2".into(), 0.2))
        );
        // only the sub past half is left: it finishes first
        let left = vec![lines[0].clone(), lines[2].clone(), lines[3].clone()];
        assert_eq!(
            naval_queue_decide(&left, &planned),
            Some(NavalQueue::Finish(vec![(
                "ship_hull_submarine_1".into(),
                0.7
            )]))
        );
        // every hull planned, convoys never touched
        let all = vec![
            "ship_hull_submarine_1".to_string(),
            "ship_hull_cruiser_2".to_string(),
        ];
        assert_eq!(naval_queue_decide(&left, &all), None);
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
    fn two_entries_of_one_airframe_own_different_wings_and_do_not_trade_bases() {
        // 1117c: the fighters of two axes, one airframe and version, bases 550 and 551
        let mut read = air_read();
        read["wings"] = json!([
            {"wing": "JG 1", "aircraft_type": "small_plane_airframe_1", "variant": "Bf 109 D", "aircraft": 100,
             "air_base": {"base_id": 3, "state_id": 550}, "standing_by": true, "missions": []},
            {"wing": "JG 2", "aircraft_type": "small_plane_airframe_1", "variant": "Bf 109 D", "aircraft": 100,
             "air_base": {"base_id": 4, "state_id": 551}, "standing_by": true, "missions": []}]);
        let tools = fake(vec![("hoi4_air", read.clone())]);
        let plan = json!({"air_plan": [
            {"airframe": "small_plane_airframe_1", "version": "Bf 109 D", "base_state": 551, "region": 273,
             "mission": "air_superiority", "deploy": {"max_wings": 1}},
            {"airframe": "small_plane_airframe_1", "version": "Bf 109 D", "base_state": 550, "region": 273,
             "mission": "air_superiority", "deploy": {"max_wings": 1}}]});
        let entries = list(&plan, "air_plan");
        let owners = wing_owners(entries, &[true, true], list(&read, "wings"));
        assert_eq!(owners["JG 2"], 0, "the wing at the entry's base");
        assert_eq!(owners["JG 1"], 1);
        // each wing flies from where it is: missions, never a rebase
        let mut state = State::default();
        for at in [START, START + 3 * DAY] {
            step(&mut state, &tools, &plan, "l", at).unwrap();
        }
        assert!(actions(&tools).iter().all(|(n, _)| n == "hoi4_air_mission"));
        // a third wing of the same kind beyond both caps joins the first active entry
        read["wings"].as_array_mut().unwrap().push(json!(
            {"wing": "JG 3", "aircraft_type": "small_plane_airframe_1", "variant": "Bf 109 D", "aircraft": 100,
             "air_base": {"base_id": 4, "state_id": 551}, "standing_by": true, "missions": []}));
        let owners = wing_owners(entries, &[false, true], list(&read, "wings"));
        assert_eq!(owners["JG 3"], 1);
        let owners = wing_owners(entries, &[false, false], list(&read, "wings"));
        assert!(!owners.contains_key("JG 3"));
    }

    #[test]
    fn an_entry_that_waits_for_the_war_still_founds_its_wings_from_the_planes_made() {
        let tools = fake(vec![
            ("hoi4_air", air_read()),
            (
                "hoi4_stockpile",
                json!({"equipment": [{"archetype": "small_plane_cas_airframe",
                "variants": [{"type": "small_plane_cas_airframe_1", "design_id": 9, "amount": 120.0}]}]}),
            ),
            (
                "hoi4_designs",
                json!({"designs": [{"design_id": 9, "name": "Ju 87"}]}),
            ),
        ]);
        let plan = json!({"air_plan": [
            {"airframe": "small_plane_cas_airframe_1", "version": "Ju 87", "base_state": 551, "region": 273,
             "mission": "cas", "when": "date:2000.1.1", "deploy": {"min_planes": 100, "max_wings": 1}}]});
        let mut state = State::default();
        let out = step(&mut state, &tools, &plan, "l", START).unwrap();
        assert_eq!(
            out.as_deref(),
            Some("air_plan 0: new wing of 100 Ju 87 from reserve at state 551: true Verified")
        );
        assert!(actions(&tools)
            .iter()
            .all(|(n, _)| n == "hoi4_air_wing_create"));
    }

    #[test]
    fn a_wing_already_training_is_not_sent_training_again() {
        let mut read = air_read();
        for w in read["wings"].as_array_mut().unwrap() {
            w["missions"] = json!(["training"]);
        }
        let tools = fake(vec![
            ("hoi4_air", read),
            (
                "hoi4_resources",
                json!({"fuel": {"ratio": 0.9, "stock": 90000.0}}),
            ),
        ]);
        let mut state = State::default();
        let out = air_idle_step(&mut state, &tools, &peace_plan(), "l", START).unwrap();
        assert!(
            !out.as_deref()
                .unwrap_or("")
                .contains("trains over its base"),
            "{out:?}"
        );
        assert!(actions(&tools).is_empty());
    }

    fn peace_plan() -> Value {
        json!({"manage": {"fuel_floor": 0.3}, "war": {"enemies": ["POL"]}})
    }

    #[test]
    fn idle_wings_train_over_their_base_in_peace_and_short_fuel_holds_the_training() {
        let mut fuel = json!({"fuel": {"ratio": 0.9, "stock": 90000.0}});
        let tools = fake(vec![
            ("hoi4_air", air_read()),
            ("hoi4_resources", fuel.clone()),
        ]);
        let mut state = State::default();
        let out = air_idle_step(&mut state, &tools, &peace_plan(), "l", START).unwrap();
        assert_eq!(
            out.as_deref(),
            Some("air 1. Stormo trains over its base (state 2) in peace: true Verified")
        );
        assert_eq!(actions(&tools)[0].1["mission"], "training");
        // the other idle wing next tick; nothing is sent twice
        let out = air_idle_step(&mut state, &tools, &peace_plan(), "l", START + 1).unwrap();
        assert!(out.unwrap().starts_with("air 3. Stormo trains"));
        // fuel under the training floor: a training wing is held, the others say why they stand
        let mut read = air_read();
        read["wings"][0]["missions"] = json!(["training"]);
        read["wings"][0]["region"] = json!({"id": 10, "name": "Italy"});
        fuel["fuel"]["ratio"] = json!(0.4);
        tools.replies.borrow_mut().insert("hoi4_air".into(), read);
        tools
            .replies
            .borrow_mut()
            .insert("hoi4_resources".into(), fuel);
        let mut state = State::default();
        let out = air_idle_step(&mut state, &tools, &peace_plan(), "l", START).unwrap();
        assert!(out.unwrap().contains("1. Stormo off training: true"));
        assert_eq!(
            air_idle_step(&mut state, &tools, &peace_plan(), "l", START + 1).unwrap(),
            None
        );
        assert!(state.manage.notes["air_idle"].contains("below the training floor 0.5"));
        // a plan that keeps wings home says so
        let mut plan = peace_plan();
        plan["manage"]["air_training"] = json!(false);
        plan["manage"]["air_training_why"] = json!("fuel kept for the war");
        tools.replies.borrow_mut().insert(
            "hoi4_resources".into(),
            json!({"fuel": {"ratio": 0.9, "stock": 90000.0}}),
        );
        let mut state = State::default();
        air_idle_step(&mut state, &tools, &plan, "l", START).unwrap();
        assert!(state.manage.notes["air_idle"].contains("fuel kept for the war"));
    }

    #[test]
    fn fuel_falling_at_war_holds_the_rear_missions_first() {
        let mut read = air_read();
        read["wings"][0]["missions"] = json!(["air_superiority"]);
        read["wings"][0]["region"] = json!({"id": 10, "name": "Italy"});
        read["wings"][2]["missions"] = json!(["port_strike"]);
        read["wings"][2]["region"] = json!({"id": 10, "name": "Italy"});
        let tools = fake(vec![
            ("hoi4_air", read),
            (
                "hoi4_resources",
                json!({"fuel": {"ratio": 0.6, "stock": 6000.0}}),
            ),
            ("hoi4_wars", json!({"wars": {"enemies": ["POL"]}})),
        ]);
        let mut state = State::default();
        // 10 days at -1000 a day: 6 days left, under the 30-day floor
        assert!(fuel_trend(&mut state, START - 10 * DAY, 16000.0).is_empty());
        assert_eq!(
            fuel_trend(&mut state, START, 6000.0),
            "; net -1000/day, 6 days left"
        );
        assert_eq!(fuel_days_left(&state).map(|d| d.round()), Some(6.0));
        let plan = peace_plan();
        assert!(at_war(&mut state, &tools, &plan, START));
        let out = air_idle_step(&mut state, &tools, &plan, "l", START).unwrap();
        assert!(
            out.unwrap().contains("3. Stormo off port_strike"),
            "superiority stays"
        );
    }

    #[test]
    fn officer_corps_spirits_are_bought_in_plan_order_when_the_experience_covers_them() {
        let tools = fake(vec![("hoi4_country_status", json!({"fields": {}}))]);
        let plan = json!({"officer_corps": [
            {"slot": "army:0", "spirit": "Bold Attack"},
            {"slot": "air:1", "spirit": "Air Superiority Doctrine", "cost": 50}],
            "manage": {"xp_reserve": {"army": 10}}});
        let mut state = State::default();
        // experience unread: nothing is bought, the note says what waits
        assert_eq!(
            officer_corps_step(&mut state, &tools, &plan, "l", START).unwrap(),
            None
        );
        assert!(state.manage.notes["officer_corps"].contains("needs 30 army experience"));
        assert!(actions(&tools).is_empty());
        // a refusal rests 30 days and three drop the entry
        let mut state = State::default();
        state.done.insert("ofc:army:0:Bold Attack".into(), START);
        state
            .manage
            .ofc_refused
            .insert("ofc:air:1:Air Superiority Doctrine".into(), 3);
        assert_eq!(
            officer_corps_step(&mut state, &tools, &plan, "l", START).unwrap(),
            None
        );
        assert_eq!(
            state.manage.notes["officer_corps"],
            "every planned spirit is taken or dropped"
        );
    }

    #[test]
    fn a_spirit_waits_for_the_experience_an_earlier_budget_spend_still_needs() {
        // the xp_budget lists the line-artillery template before the army spirit: with 60 army
        // experience the spirit (35) waits while the template (40) is due and undone
        let tools = fake(vec![
            ("hoi4_country_status", json!({"fields": {}})),
            (
                "hoi4_doctrines",
                json!({"experience": {"army": {"value": 60.0}}}),
            ),
            (
                "hoi4_officer_corps",
                json!({"receipt": {"status": "Verified"}}),
            ),
        ]);
        let plan = json!({
            "officer_corps": [{"slot": "army:1", "spirit": "Motorization Drive", "cost": 35}],
            "xp_budget": {"army": [
                {"id": "x_tpl", "cost": 40, "purpose": "template", "ref": "template:t_art", "when": "xp:army >= 40"},
                {"id": "x_oc", "cost": 35, "purpose": "officer_corps_spirit", "ref": "spirit:motorization_drive"}]}});
        let mut state = State::default();
        assert_eq!(
            officer_corps_step(&mut state, &tools, &plan, "l", START).unwrap(),
            None
        );
        assert!(
            state.manage.notes["officer_corps"].contains("held for x_tpl first"),
            "{:?}",
            state.manage.notes
        );
        // the template edit done: the spirit is bought
        let mut state = State::default();
        state.done.insert("tpl:id:t_art".into(), START);
        let out = officer_corps_step(&mut state, &tools, &plan, "l", START).unwrap();
        assert!(out.is_some_and(|l| l.contains("Motorization Drive (army:1): true")));
        assert!(state.done.contains_key("ofc:army:1:Motorization Drive"));
    }

    #[test]
    fn reserve_planes_no_entry_takes_become_a_wing_once_they_stand_14_days() {
        let stock = |n: f64| {
            json!({"equipment": [{"archetype": "small_plane_cas_airframe",
                "variants": [{"type": "small_plane_cas_airframe_0", "design_id": 7, "amount": n}]},
                {"archetype": "cv_small_plane_airframe",
                "variants": [{"type": "cv_small_plane_airframe_1", "design_id": 8, "amount": 300.0}]}]})
        };
        let tools = fake(vec![
            ("hoi4_air", air_read()),
            ("hoi4_stockpile", stock(90.0)),
            (
                "hoi4_designs",
                json!({"designs": [{"id": 7, "name": "Hs 123"}, {"id": 8, "name": "Bf 109 T"}]}),
            ),
        ]);
        let plan = json!({"manage": {}});
        let mut state = State::default();
        assert_eq!(
            air_reserve_step(&mut state, &tools, &plan, "l", START).unwrap(),
            None
        );
        assert!(state.manage.notes["air_reserve"].contains("Hs 123 90"));
        // drawn down by the wings meanwhile: the watch starts again
        tools
            .replies
            .borrow_mut()
            .insert("hoi4_stockpile".into(), stock(60.0));
        state.manage.stockpile = None;
        assert_eq!(
            air_reserve_step(&mut state, &tools, &plan, "l", START + 7 * DAY).unwrap(),
            None
        );
        state.manage.stockpile = None;
        assert_eq!(
            air_reserve_step(&mut state, &tools, &plan, "l", START + 14 * DAY).unwrap(),
            None
        );
        state.manage.stockpile = None;
        let out = air_reserve_step(&mut state, &tools, &plan, "l", START + 21 * DAY).unwrap();
        assert_eq!(
            out.as_deref(),
            Some("air reserve: new wing of Hs 123 (60 unused in reserve) at state 550: true Verified"),
            "the close air support wing's base has room; carrier planes stay"
        );
        // an air plan entry that founds wings of that airframe owns them instead
        let plan = json!({"manage": {}, "air_plan": [{"airframe": "small_plane_cas_airframe", "deploy": true}]});
        let mut state = State::default();
        state.manage.air_reserve_seen.insert(
            "small_plane_cas_airframe_0#7".into(),
            (START - 30 * DAY, 60.0),
        );
        assert_eq!(
            air_reserve_step(&mut state, &tools, &plan, "l", START).unwrap(),
            None
        );
        assert_eq!(
            state.manage.notes["air_reserve"],
            "no unplanned reserve planes"
        );
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

    /// deploy_choice with the wings of air_read() the entry matches as its own.
    fn choice(
        entry: &Value,
        stock: &Value,
        designs: &Value,
    ) -> std::result::Result<(String, i64), String> {
        let read = air_read();
        let own: Vec<&Value> = list(&read, "wings")
            .iter()
            .filter(|w| wing_matches(entry, w))
            .collect();
        deploy_choice(entry, &own, stock, designs)
    }

    fn reserve(amount: f64) -> Value {
        json!({"equipment": [{"archetype": "small_plane_airframe", "stockpile": amount,
            "variants": [{"type": "small_plane_airframe_0", "design_id": 7, "amount": amount}]}]})
    }

    #[test]
    fn reserve_planes_beyond_the_wings_refill_found_a_new_wing() {
        let designs = json!({"designs": [{"design_id": 7, "name": "CR.32"}]});
        let entry = json!({"airframe": "small_plane_airframe_0", "version": "CR.32",
            "mission": "air_superiority", "region": 273, "base_region": 273, "deploy": true});
        // 1. Stormo holds 40 of 100: 60 of the reserve are its refill
        assert_eq!(
            choice(&entry, &reserve(160.0), &designs),
            Ok(("CR.32".to_string(), 100))
        );
        let short = choice(&entry, &reserve(90.0), &designs).unwrap_err();
        assert!(short.contains("below min_planes 50"), "{short}");
        let capped = json!({"airframe": "small_plane_airframe_0", "deploy": {"max_wings": 1}});
        assert!(choice(&capped, &reserve(500.0), &designs)
            .unwrap_err()
            .contains("max_wings 1"));
        let named = json!({"wing": "1. Stormo", "deploy": true});
        assert!(choice(&named, &reserve(500.0), &designs)
            .unwrap_err()
            .contains("airframe"));
        // another variant than the planned version is left in reserve
        let other = json!({"designs": [{"id": 7, "name": "CR.42"}]});
        assert!(choice(&entry, &reserve(500.0), &other).is_err());

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
        let tools = fake(vec![("hoi4_supply", overdrawn_read(20.0, 2.0))]);
        let plan = json!({"manage": {}});
        let mut state = State::default();
        let out = step(&mut state, &tools, &plan, "l", START).unwrap();
        assert_eq!(
            out.as_deref(),
            Some("overdrawn: province 100 at 20% supply (shortfall 1.20): 3 of A's divisions to state 10 province 200 (2.00 supply of room), in their army: true Verified")
        );
        // the three with the lowest equipment fill cover the 1.2 shortfall (3 x 0.5); they march
        // in their own army: one order, no detach, no new army
        assert_eq!(
            actions(&tools)[0].1,
            json!({"army": "A", "state_id": 10, "province_id": 200, "divisions": ["2", "5", "4"],
                "request_id": actions(&tools)[0].1["request_id"], "lease_token": "l"})
        );
        assert!(state.war_cool.is_empty());
        // nothing more until the next game day
        assert_eq!(step(&mut state, &tools, &plan, "l", START).unwrap(), None);
        assert_eq!(actions(&tools).len(), 1);
    }

    #[test]
    fn a_defender_in_its_hold_keeps_its_overdrawn_stack() {
        let tools = fake(vec![
            ("hoi4_supply", overdrawn_read(20.0, 2.0)),
            (
                "hoi4_armies",
                json!({"armies": [{"army": "A"}, {"army": "Army 9"}]}),
            ),
        ]);
        let plan = json!({"manage": {}, "war": {"posture": "defend", "axes": []}});
        let mut state = State::default();
        let out = step(&mut state, &tools, &plan, "l", START).unwrap();
        assert!(out.is_none_or(|s| !s.starts_with("overdrawn")));
        assert!(actions(&tools).is_empty());
        // its counter-attack phase (axes) splits again
        let plan = json!({"manage": {}, "war": {"posture": "defend", "axes": [[10]]}});
        let out = step(&mut state, &tools, &plan, "l", START).unwrap();
        assert!(out.is_some_and(|s| s.starts_with("overdrawn")));
    }

    #[test]
    fn groups_out_of_an_overdrawn_stack_never_form_armies_and_spread_over_the_room() {
        // g17b (17 Nov 2026): every split made a new army (Army 1-8) and three of them were
        // sent to the same province against the same day-old headroom
        let mut read = overdrawn_read(20.0, 1.6);
        read["overdrawn_provinces"][0]["shortfall"] = json!(3.0);
        let tools = fake(vec![("hoi4_supply", read.clone())]);
        let plan = json!({"manage": {"overdrawn_rest_days": 20}});
        let mut state = State::default();
        let out = step(&mut state, &tools, &plan, "l", START)
            .unwrap()
            .unwrap();
        assert!(
            out.contains("3 of A's divisions to state 10 province 200"),
            "{out}"
        );
        // five days on, those three are still on their way: they are not picked again, and
        // province 200 (1.6 of room, 1.5 of it promised) cannot take another
        let out = step(&mut state, &tools, &plan, "l", START + 5 * DAY)
            .unwrap()
            .unwrap();
        assert!(
            out.contains("1 of A's divisions to state 11 province 300"),
            "{out}"
        );
        let moves = actions(&tools);
        assert_eq!(moves.len(), 2);
        assert!(moves.iter().all(|(n, _)| n == "hoi4_army_move"));
        assert_eq!(moves[1].1["divisions"], json!(["1"]));
        // the stack's shortfall is covered by what is leaving: nothing more is sent
        tools.replies.borrow_mut().insert("hoi4_supply".into(), {
            let mut r = read;
            r["overdrawn_provinces"][0]["shortfall"] = json!(1.4);
            r
        });
        assert_eq!(
            step(&mut state, &tools, &plan, "l", START + 10 * DAY).unwrap(),
            None
        );
        assert!(
            state.manage.notes["overdrawn"].contains("already on their way out"),
            "{}",
            state.manage.notes["overdrawn"]
        );
        assert_eq!(actions(&tools).len(), 2);
    }

    #[test]
    fn a_province_whose_march_order_failed_is_not_chosen_again() {
        let tools = fake(vec![("hoi4_supply", overdrawn_read(20.0, 2.0))]);
        tools
            .refuse
            .borrow_mut()
            .insert("hoi4_army_move".into(), "input_readback_failed");
        let plan = json!({"manage": {}});
        let mut state = State::default();
        let out = step(&mut state, &tools, &plan, "l", START)
            .unwrap()
            .unwrap();
        assert!(out.contains("to state 10 province 200"), "{out}");
        assert!(out.contains("false input_readback_failed"), "{out}");
        assert!(state.manage.spread.is_empty());
        // the next game day the same divisions go to the other province with room
        tools.refuse.borrow_mut().clear();
        let out = step(&mut state, &tools, &plan, "l", START + DAY)
            .unwrap()
            .unwrap();
        assert!(out.contains("to state 11 province 300"), "{out}");
        let moves = actions(&tools);
        assert_eq!(moves.len(), 2);
        assert_eq!(moves[0].1["divisions"], moves[1].1["divisions"]);
    }

    #[test]
    fn an_overdrawn_stack_above_the_floor_or_without_room_elsewhere_stays() {
        let plan = json!({"manage": {"overdrawn_floor": 35}});
        let tools = fake(vec![("hoi4_supply", overdrawn_read(60.0, 2.0))]);
        let mut state = State::default();
        assert_eq!(step(&mut state, &tools, &plan, "l", START).unwrap(), None);
        assert!(state.manage.notes["overdrawn"].contains("above the floor"));
        // no province has 1.5 of room: the one with most room takes what it feeds (two of the
        // three, the weakest first), the third waits for the next day
        let mut read = overdrawn_read(20.0, 0.1);
        read["provinces"][2]["headroom"] = json!(1.0);
        let tools = fake(vec![("hoi4_supply", read.clone())]);
        let mut state = State::default();
        let out = step(&mut state, &tools, &plan, "l", START)
            .unwrap()
            .unwrap();
        assert!(
            out.contains("2 of A's divisions to state 11 province 300"),
            "{out}"
        );
        assert_eq!(actions(&tools)[0].1["divisions"], json!(["2", "5"]));
        // no province feeds even one more division: nothing leaves, the digest names the lever
        read["provinces"][2]["headroom"] = json!(0.4);
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
        // below a threshold the plan explains the power is banked and no window opens
        let low = fake(vec![("hoi4_economy", pp(90.0))]);
        let mut state = State::default();
        let saving = json!({"politics_auto": {"threshold": 100, "threshold_why": "a law first"}});
        assert_eq!(
            politics_auto_step(&mut state, &low, &saving, "l", START).unwrap(),
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
    fn an_ordered_advisor_replaces_a_staff_holder_the_order_does_not_name() {
        // fk1: a naval officer held the high command seat the order gives Rommel
        let offered: Vec<(String, bool)> = ["GER_erwin_rommel", "GER_erich_bey"]
            .iter()
            .map(|s| (s.to_string(), true))
            .collect();
        let none: Vec<String> = Vec::new();
        let seat = json!({"category": "military_staff", "slot": 5, "ideas": ["GER_erwin_rommel"]});
        assert_eq!(
            politics_auto_choice(&seat, "GER_erich_bey", &offered, &none, &none),
            Ok("GER_erwin_rommel".into())
        );
        assert!(politics_auto_choice(&seat, "GER_erwin_rommel", &offered, &none, &none).is_err());
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
        let plan = json!({"politics_auto": {"fill_vacant": false, "order": [{"category": "government", "slot": 2,
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
        let plan = json!({"politics_auto": {"fill_vacant": false}});
        let mut state = State::default();
        // the seat read first holds the chief: the read lists seats 3 and 5 vacant, and the
        // first advisor enabled at seat 3 is bought at once
        let out = politics_auto_step(&mut state, &tools, &plan, "l", START).unwrap();
        assert!(
            out.as_deref()
                .unwrap_or("")
                .contains("military_staff:3 vacant -> X_a"),
            "{out:?}"
        );
        tools.calls.borrow_mut().clear();
        state
            .manage
            .pp_vacant
            .insert("military_staff".into(), (START, vec![3]));
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
    fn power_the_order_cannot_spend_fills_a_vacant_advisor_seat() {
        // the order names a key the game does not offer; seat 5 of the government is vacant
        let tools = fake(vec![
            ("hoi4_economy", pp(200.0)),
            (
                "hoi4_politics_options",
                json!({"current": null, "options": [{"idea": "X_minister", "enabled": true}],
                    "slots": [{"slot": 0, "current": "law"}, {"slot": 5, "current": null}]}),
            ),
        ]);
        let plan = json!({"politics_auto": {"threshold": 60, "order": [
            {"category": "government", "slot": 4, "ideas": ["not_a_key"], "cost": 150}]}});
        let mut state = State::default();
        let mut bought = None;
        for day in 0..4 {
            if let Some(out) =
                politics_auto_step(&mut state, &tools, &plan, "l", START + day * DAY).unwrap()
            {
                bought = Some(out);
                break;
            }
        }
        let out = bought.expect("the vacancy fill buys an advisor");
        assert!(out.contains("-> X_minister"), "{out}");
        let acts = actions(&tools);
        assert_eq!(acts.len(), 1);
        assert_eq!(acts[0].1["idea"], "X_minister");
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
        // once a game day, and the line waits after a raise (the factories still free are fed
        // at once by the idle rule, never a second stockpile raise)
        let again = step(&mut state, &tools, &plan, "l", START + 2).unwrap();
        assert!(
            again.as_deref().is_none_or(|l| !l.starts_with("stockpile")),
            "{again:?}"
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
        // (the 5 free factories go to a line at once: the idle feed, no stockpile raise)
        let fed = step(&mut state, &tools, &targets, "l", START).unwrap();
        assert!(
            fed.as_deref().is_none_or(|l| !l.starts_with("stockpile")),
            "{fed:?}"
        );
        assert!(state.manage.notes["stockpile_days"].contains("infantry_equipment_1 40/35"));
        // a stock that is not falling is left alone
        let mut replies = lines_with(5);
        replies.push(("hoi4_stockpile", stock_rows(json!(null), 500.0, 100.0)));
        let tools = fake(replies);
        let mut state = State::default();
        let mut quiet = json!({"manage": {}});
        quiet["stockpile_targets"] = json!({"infantry_equipment": 30});
        let fed = step(&mut state, &tools, &quiet, "l", START).unwrap();
        assert!(
            fed.as_deref().is_none_or(|l| !l.starts_with("stockpile")),
            "{fed:?}"
        );
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
        // PR09: a line gives at most a fifth of itself (12 -> 10)
        assert_eq!(
            (sent[0].1["line"].clone(), sent[0].1["factories"].clone()),
            (json!(0), json!(10))
        );
        assert_eq!(
            (sent[1].1["line"].clone(), sent[1].1["factories"].clone()),
            (json!(1), json!(10))
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
        let off = json!({"manage": {"off": ["stockpile", "trucks", "air_reserve"]}});
        assert_eq!(step(&mut state, &tools, &off, "l", START).unwrap(), None);
        assert_eq!(reads(&tools, "hoi4_stockpile"), 0);
    }

    #[test]
    fn a_shortage_resting_on_its_skip_does_not_hold_back_the_next_one() {
        let mut replies = lines_with(5);
        let mut rows = stock_rows(json!(12.0), 500.0, 100.0);
        rows["equipment"].as_array_mut().unwrap().push(json!({
            "archetype": "anti_air_equipment", "stockpile": 0.0, "fielded_short": 50.0,
            "days_of_cover": null, "depleting": false, "variants": []}));
        replies.push(("hoi4_stockpile", rows));
        let tools = fake(replies);
        let plan = json!({"manage": {}});
        let mut state = State::default();
        // anti-air (no cover at all) has failed and rests: infantry is worked meanwhile
        state.skip(START, "mstockline", "anti_air_equipment", 7 * DAY);
        let out = step(&mut state, &tools, &plan, "l", START)
            .unwrap()
            .unwrap();
        assert!(out.starts_with("stockpile infantry_equipment low"), "{out}");
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
        let plan = json!({"production_priority": ["infantry_equipment", "artillery_equipment"], "manage": {"off": ["balance"]}});
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

    #[test]
    fn no_idle_factory_is_no_order_even_with_a_zero_minimum() {
        let tools = fake(lines_with(0));
        let plan = json!({"production_priority": ["infantry_equipment"], "production_min_idle": 0});
        let mut state = State::default();
        let economy = tools.call("hoi4_economy", &json!({})).unwrap();
        let out = rebalance_step(&mut state, &tools, &plan, "l", START, &economy).unwrap();
        assert!(out.is_none(), "{out:?}");
    }

    fn pl(index: i64, equipment: &str, requested: i64, active: i64, eff: f64) -> ProdLine {
        ProdLine {
            index,
            equipment: equipment.into(),
            archetype: family(equipment).into(),
            requested,
            active,
            efficiency: eff,
            start: 10.0,
        }
    }

    #[test]
    fn idle_dockyards_go_to_the_planned_ship_lines_up_to_their_cap() {
        let lines = vec![
            (8, "ship_hull_submarine_1".to_string(), 1),
            (9, "ship_hull_heavy_2".to_string(), 5),
        ];
        let planned = vec!["ship_hull_submarine".to_string()];
        assert_eq!(
            dockyard_decide(&lines, &planned, 3),
            Some(DockAct::Set(8, "ship_hull_submarine_1".into(), 4))
        );
        // a planned equipment without a line gets one; nothing planned: the convoy rule's
        let planned = vec!["ship_hull_light_2".to_string()];
        assert_eq!(
            dockyard_decide(&lines, &planned, 4),
            Some(DockAct::Add("ship_hull_light_2".into(), 4))
        );
        assert_eq!(dockyard_decide(&lines, &[], 4), None);
        assert_eq!(dockyard_decide(&lines, &planned, 0), None);
    }

    #[test]
    fn air_wings_whose_plane_no_line_makes_are_named() {
        let tools = fake(vec![
            (
                "hoi4_air",
                json!({"wings": [
                {"wing": "JG 1", "aircraft_type": "small_plane_airframe_0"},
                {"wing": "KuFlGr 106", "aircraft_type": "small_plane_naval_bomber_airframe_1"},
                {"wing": "KGrzbV 1", "aircraft_type": "transport_plane_equipment_1"}]}),
            ),
            (
                "hoi4_production_lines",
                json!({"lines": [
                {"equipment_id": "small_plane_airframe_1"}, {"equipment_id": "infantry_equipment_1"}]}),
            ),
        ]);
        assert_eq!(
            super::unproduced_air_gaps(&tools),
            vec![
                "small_plane_naval_bomber_airframe (wings: KuFlGr 106)".to_string(),
                "transport_plane_equipment (wings: KGrzbV 1)".to_string()
            ]
        );
    }

    #[test]
    fn family_drops_the_tier_number_only() {
        assert_eq!(family("infantry_equipment_1"), "infantry_equipment");
        assert_eq!(
            family("ship_hull_cruiser_panzerschiff"),
            "ship_hull_cruiser_panzerschiff"
        );
        assert_eq!(family("convoy_1"), "convoy");
    }

    #[test]
    fn a_young_duplicate_line_is_closed_and_a_mature_one_kept() {
        let mut b = BalanceIn {
            total: 20,
            ..Default::default()
        };
        b.lines = vec![
            pl(0, "infantry_equipment_1", 10, 10, 40.0),
            pl(7, "infantry_equipment_1", 3, 3, 10.0),
        ];
        match balance_decide(&b) {
            Some(BalanceAct::Remove { index, .. }) => assert_eq!(index, 7),
            other => panic!("{other:?}"),
        }
        b.lines[1].efficiency = 35.0;
        assert!(!matches!(
            balance_decide(&b),
            Some(BalanceAct::Remove { .. })
        ));
    }

    #[test]
    fn the_plans_dated_lines_are_floors_filled_before_any_need() {
        // g17j day 0: trucks 10 (the stockpile rule), rifles 10 of the plan's 12, none idle
        let mut b = BalanceIn {
            total: 26,
            ..Default::default()
        };
        b.lines = vec![
            pl(0, "artillery_equipment_1", 4, 4, 10.0),
            pl(2, "motorized_equipment_1", 10, 10, 10.0),
            pl(3, "infantry_equipment_1", 10, 10, 10.0),
            pl(1, "support_equipment_1", 2, 2, 10.0),
        ];
        for (a, f) in [
            ("infantry_equipment", 12),
            ("artillery_equipment", 4),
            ("support_equipment", 5),
            ("motorized_equipment", 7),
        ] {
            b.floors.insert(a.into(), f);
        }
        b.need.insert("motorized_equipment".into(), 900.0);
        b.per_day.insert("motorized_equipment".into(), 10.0);
        // support is 3 short and the trucks 3 above: the trucks give the support its line
        match balance_decide(&b) {
            Some(BalanceAct::Set {
                index, factories, ..
            }) => assert_eq!((index, factories), (2, 7)),
            other => panic!("{other:?}"),
        }
        // idle factories go to the floors before the trucks' shortfall
        b.lines[1].requested = 7;
        b.lines[3].requested = 5;
        b.total = 28;
        match balance_decide(&b) {
            Some(BalanceAct::Set {
                index, factories, ..
            }) => assert_eq!((index, factories), (3, 12)),
            other => panic!("{other:?}"),
        }
        // a ramp by date with no line: a new line from the idle factories
        b.lines[2].requested = 12;
        b.total = 31;
        b.floors.insert("small_plane_airframe".into(), 8);
        b.floor_equipment.insert(
            "small_plane_airframe".into(),
            "small_plane_airframe_1".into(),
        );
        match balance_decide(&b) {
            Some(BalanceAct::Add {
                equipment,
                factories,
                ..
            }) => assert_eq!(
                (equipment.as_str(), factories),
                ("small_plane_airframe_1", 3)
            ),
            other => panic!("{other:?}"),
        }
        // the line then grows to its floor from a line above its own (none here: it waits)
        b.total = 28;
        b.lines.push(pl(4, "small_plane_airframe_1", 1, 1, 10.0));
        b.lines[1].requested = 6;
        assert_eq!(balance_decide(&b), None);
    }

    #[test]
    fn a_request_above_the_factories_that_exist_is_trimmed() {
        let mut b = BalanceIn {
            total: 30,
            ..Default::default()
        };
        b.lines = vec![
            pl(0, "infantry_equipment_1", 37, 21, 40.0),
            pl(1, "artillery_equipment_1", 2, 2, 40.0),
        ];
        assert_eq!(
            balance_decide(&b),
            Some(BalanceAct::Set { index: 0, equipment: "infantry_equipment_1".into(), factories: 21,
                why: "infantry_equipment_1 asks for 37 factories and has 21: the request is trimmed to what exists, new factories go by need".into() })
        );
    }

    #[test]
    fn idle_factories_go_to_the_longest_shortfall_all_in_one_turn() {
        let mut b = BalanceIn {
            total: 30,
            ..Default::default()
        };
        b.lines = vec![
            pl(0, "infantry_equipment_1", 20, 20, 40.0),
            pl(1, "artillery_equipment_1", 2, 2, 40.0),
        ];
        b.need = HashMap::from([
            ("infantry_equipment".into(), 5000.0),
            ("artillery_equipment".into(), 400.0),
        ]);
        b.stock = HashMap::from([
            ("infantry_equipment".into(), 20000.0),
            ("artillery_equipment".into(), 10.0),
        ]);
        b.per_day = HashMap::from([
            ("infantry_equipment".into(), 50.0),
            ("artillery_equipment".into(), 0.8),
        ]);
        match balance_decide(&b) {
            Some(BalanceAct::Set {
                index, factories, ..
            }) => {
                assert_eq!(index, 1);
                assert_eq!(
                    factories, 10,
                    "the only short equipment takes all 8 free factories in one turn"
                );
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn a_need_without_a_line_gets_a_new_line_of_the_templates_variant() {
        let mut b = BalanceIn {
            total: 12,
            ..Default::default()
        };
        b.lines = vec![pl(0, "infantry_equipment_1", 10, 10, 40.0)];
        b.need = HashMap::from([("support_equipment".into(), 300.0)]);
        b.variant = HashMap::from([("support_equipment".into(), "support_equipment_1".into())]);
        assert!(
            matches!(balance_decide(&b), Some(BalanceAct::Add { equipment, factories: 2, .. }) if equipment == "support_equipment_1")
        );
    }

    #[test]
    fn a_long_shortfall_pulls_a_fifth_from_a_large_surplus_once() {
        let mut b = BalanceIn {
            total: 30,
            pull_days: 120.0,
            ..Default::default()
        };
        b.lines = vec![
            pl(0, "infantry_equipment_1", 28, 28, 50.0),
            pl(1, "artillery_equipment_1", 2, 2, 40.0),
        ];
        b.need = HashMap::from([("artillery_equipment".into(), 900.0)]);
        b.stock = HashMap::from([
            ("infantry_equipment".into(), 200000.0),
            ("artillery_equipment".into(), 0.0),
        ]);
        b.per_day = HashMap::from([("artillery_equipment".into(), 0.8)]);
        match balance_decide(&b) {
            Some(BalanceAct::Set {
                index, factories, ..
            }) => assert_eq!((index, factories), (0, 23)),
            other => panic!("{other:?}"),
        }
        b.pulled.insert("infantry_equipment_1".into());
        assert_eq!(balance_decide(&b), None, "one pull per line in 30 days");
        b.pulled.clear();
        b.per_day.insert("artillery_equipment".into(), 100.0);
        assert_eq!(
            balance_decide(&b),
            None,
            "a short shortfall waits for new factories"
        );
    }
}

#[cfg(test)]
#[path = "plan_manage_sim.rs"]
mod sim;
