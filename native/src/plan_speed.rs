//! Game speed by the war the country manages (rule `speed`, manage.off "speed"; plan key
//! `speed` {"peace": 3, "war": 2} in the game's own speeds 1-5, defaults 3 and 2).
//!
//! Owner, 23 Nov 2026: while the country only builds its economy and army, the game's speed 3
//! is the default; while it has a war to manage, speed 2. Value judgement: at war an order and
//! its read-back cost about a game day each at speed 3, and the war's orders are the ones whose
//! timing decides battles; in peace the same delay costs nothing that compounds. In multiplayer
//! the human sets the pace: the rule sets the speed only when the phase changes (a war to manage
//! begins or ends), remembers what it set, and when a human changes the speed inside the phase
//! (higher or lower) it never sets it again for that phase: it tells the player once (a note in
//! the run result's `manage.speed` and a `wake_player` alarm; the server has no in-game chat
//! tool) and plays on. It never pauses or unpauses.
//!
//! Four levels by the WEIGHT of what is due (owner, 23 Nov 2026: "Sometimes the agent should
//! bump it to 4 speed as well"; "4 speed is not only when there is nothing to do, but only like
//! minor actions, like choosing a focus, choosin a research, spending pp, etc"; "speed 5 is fine
//! too for long empty stretches"):
//!   2  a war is managed (below);
//!   3  HEAVY peace work (plan_run.rs LANE_WEIGHT: production and construction rework, army
//!      layout, templates, training, air and navy setup, an operation preparing), from its first
//!      action until no heavy lane has acted for HEAVY_QUIET;
//!   4  the normal speed while only MINOR one-click actions come due (a focus, a research, a law
//!      or advisor, a decision, an officer-corps or doctrine pick, an event, a design-company
//!      trait): the executor does them at 4 on their day;
//!   5  a long empty stretch: no lane acted for EMPTY_QUIET, no open omission, and the next thing
//!      the executor knows it will act on (a research slot or the focus completing, by the
//!      durations it has seen; a date of the plan) further than FAR_ENTER; at 5 a game day passes
//!      in under a second, so it steps down to 4 FAR_LEAVE before that thing.
//! Hysteresis: a faster speed (3 -> 4, 4 -> 5) is taken at most once per RISE_HOLD after the last
//! change; a slower one at once (heavy work starts, a lane acts during 5, a war).
//!
//! A war to manage (value judgement): an operation of ours executing (from its execution
//! until it ends), a war an operation names declared within the last week (the declaration
//! before the operation executes), or being attacked: our casualties rose or an own state was
//! lost within the last week. A border with a country at war where nothing happens (holds
//! facing each other, no losses for a week: the phoney war with France from September 1939) is
//! peace for this rule: the speed of building the economy and the army. (w46c1, 23 Nov 2026:
//! the bordering enemy alone made the whole phoney war a "war" from the first tick.)
//!
//! A human's change is a speed read while the game runs that differs from what the rule set,
//! when the reading before it was also running at the rule's speed. A different speed after a
//! pause (the launcher's or the harness's own unpause, its start speed) is not a human's choice:
//! the rule sets its speed again. While paused the rule waits and its phase speed stays due.
//!
//! Speeds: `hoi4_time` and `hoi4_overview` count from 0 (internal speed 0-4 = the game's 1-5):
//! the game's speed 3 is internal 2. Done-memory (cleared on a phase change):
//! `spd:phase:<peace|war>`, `spd:set:<internal>` (the rule's speed this phase), `spd:human`,
//! `spd:last:<internal>:<1 running|0 paused>` (the reading before). Kept across phases:
//! `spdw:fight` (the last loss seen), `spdw:own` (own losses at the last read), `spdw:occ:<state>`,
//! `spdw:calm:<TAG>` (an operation's enemy seen not at war), `spdw:new:<TAG>` (its war began).
use super::{list, strings, GameHour, State, Tools};
use crate::error::Result;
use serde_json::{json, Value};

const RULE: &str = "speed";
/// How often the phase is read (game hours): a declaration is seen within a quarter of a day.
const READ_HOURS: GameHour = 6;
/// A loss, an own state lost or a planned war declared counts this long (a game week).
const QUIET: GameHour = 7 * 24;
/// STABILITY (owner, 23 Nov 2026: "make sure that it wouldn't be constant switching of speed, i
/// don't want it to go from 4 then to 3 every few seconds. It still has to be somewhat stable"):
/// a speed, once set, holds at least DWELL_GAME of game time and DWELL_REAL of real time; only a
/// war (declared by us or on us, an operation executing) steps down at once, and speed 5 steps
/// down at once before its next due thing (a game day passes in under a second there).
const DWELL_GAME: GameHour = 14 * 24;
/// The real-time half of the dwell (seconds).
const DWELL_REAL: i64 = 90;
/// A guard for the owner's stability requirement, not a play rule: at most this many speed
/// changes in a game year outside war starts and ends; once spent, the speed stays until the next
/// year (speed 5's step down and the war's changes are always allowed). A breach shows in the
/// sitrep's CLEANLINESS block.
const YEAR_BUDGET: i64 = 6;
/// Heavy work looked at over this window: speed 3 stays while any heavy action fell inside it
/// (no 4-3-4 over a stretch of recurring heavy work)...
const HEAVY_WINDOW: GameHour = 14 * 24;
/// ...and speed 4 steps down to 3 only for heavy work on at least this many distinct days of the
/// window (a single short heavy task is done at 4).
const HEAVY_DAYS: usize = 3;
/// Speed 5 needs this long without any lane acting.
const EMPTY_QUIET: GameHour = 2 * 24;
/// Speed 5 only when the next known thing to act on is further than this (about two months).
const FAR_ENTER: GameHour = 60 * 24;
/// From speed 5 back down this long before the next known thing (a margin of more than a week).
const FAR_LEAVE: GameHour = 10 * 24;
/// Speed 5 at most this many times a run (owner: "speed 5 should be rare").
const FAR_MAX: i64 = 5;

/// MANAGEMENT LOAD (owner, 23 Nov 2026: "5 speed is logical for like smaller countries or like
/// isolated countries, where there isn't much to manage. And later in the game when everything
/// is built up then it makes sense to continue with lower speeds"): the real seconds of executor
/// work per game day (every lane's actions, measured) against the real seconds a game day lasts
/// at a speed (measured by this rule's reads, these defaults until measured). A speed is light
/// enough when the work takes at most LOAD_SHARE of its day: the rest is left for the reads, the
/// player and the unexpected. No country or date in the rule: a small country has little load
/// and may run faster; a built-up one has a steady stream and stays at 3.
const DAY_MS_DEFAULT: [i64; 5] = [6000, 3000, 1800, 900, 300];
/// The share of a game day the executor's work may take at a speed.
const LOAD_SHARE: f64 = 0.5;
/// The load is looked at over this window for 4 and over FAR_LOAD_WINDOW for 5.
const LOAD_WINDOW: GameHour = 14 * 24;
const FAR_LOAD_WINDOW: GameHour = 56 * 24;

/// Real milliseconds a game day lasts at game speed `speed` (1-5): measured, else the default.
fn day_ms(state: &State, speed: i64) -> f64 {
    state
        .done
        .get(&format!("spdw:dayms:{speed}"))
        .copied()
        .unwrap_or(DAY_MS_DEFAULT[(speed.clamp(1, 5) - 1) as usize]) as f64
}

/// Whether the measured load leaves the executor time at game speed `speed`; pure but for reads.
fn light_at(state: &State, speed: i64, now: GameHour, window: GameHour) -> bool {
    state.round.load(now, window) * 1000.0 <= LOAD_SHARE * day_ms(state, speed)
}

/// Learns how long a game day lasts at the speed the game ran at between two reads (both running
/// at the same speed): a mean that moves a quarter of the way to each new measurement.
fn learn_day(state: &mut State, speed: i64, running: bool, now: GameHour, real_ms: i64) {
    let last = state.done.get("spdw:read_at").copied();
    let last_real = state.done.get("spdw:read_ms").copied();
    let last_speed = state.done.get("spdw:read_speed").copied();
    if let (Some(at), Some(ms), Some(sp)) = (last, last_real, last_speed) {
        if running && sp == speed && now > at && real_ms > ms {
            let per_day = (real_ms - ms) as f64 / ((now - at) as f64 / 24.0);
            if per_day > 10.0 && per_day < 60_000.0 {
                let key = format!("spdw:dayms:{speed}");
                let old = state.done.get(&key).map_or(per_day, |v| *v as f64);
                state.done.insert(key, (old * 0.75 + per_day * 0.25) as i64);
            }
        }
    }
    state.done.insert("spdw:read_at".into(), now);
    state.done.insert("spdw:read_ms".into(), real_ms);
    state
        .done
        .insert("spdw:read_speed".into(), if running { speed } else { -1 });
}

/// Wall-clock seconds (a test sets its own clock).
/// Wall-clock milliseconds (a test sets its own clock).
fn real_ms() -> i64 {
    real_secs() * 1000
}

#[cfg(not(test))]
fn real_secs() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs() as i64)
}
#[cfg(test)]
thread_local! {
    static REAL: std::cell::Cell<i64> = const { std::cell::Cell::new(1_000_000) };
}
#[cfg(test)]
fn real_secs() -> i64 {
    REAL.with(|r| r.get())
}

/// The operations of the plan that execute now (executed, not ended), from the executor's
/// memory (`op:<id>:exec` without `op:<id>:end`); pure.
pub(super) fn executing_operations(state: &State) -> Vec<String> {
    let mut out: Vec<String> = state
        .done
        .keys()
        .filter_map(|k| k.strip_prefix("op:")?.strip_suffix(":exec"))
        .filter(|id| !id.contains(':'))
        .filter(|id| !state.done.contains_key(&format!("op:{id}:end")))
        .map(str::to_string)
        .collect();
    out.sort();
    out
}

/// The enemies at war (not won) from a hoi4_wars read; pure.
fn enemies_at_war(wars: &Value) -> Vec<String> {
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

/// Updates the memory of what happened (losses, own states lost, planned wars declared) and
/// says why there is a war to manage now, or None; `stats` is a hoi4_war_stats read (None when
/// unread).
pub(super) fn war_reason(
    state: &mut State,
    plan: &Value,
    wars: &Value,
    stats: Option<&Value>,
    now: GameHour,
) -> Option<String> {
    let at_war = enemies_at_war(wars);
    // a planned war declared: an operation's enemy seen not at war before, at war now
    let planned: Vec<String> = list(plan, "operations")
        .iter()
        .flat_map(|op| strings(op, "enemies"))
        .collect();
    for tag in &planned {
        let calm = format!("spdw:calm:{tag}");
        let new = format!("spdw:new:{tag}");
        if !at_war.contains(tag) {
            state.done.insert(calm, now);
            state.done.remove(&new);
        } else if state.done.contains_key(&calm) {
            state.done.entry(new).or_insert(now);
        }
    }
    // being attacked: own losses rose, an own state was lost
    if let Some(own) = stats
        .and_then(super::plan_military::losses_by_enemy)
        .map(|l| l.values().map(|(o, _)| *o).sum::<f64>())
    {
        let own = own.round() as GameHour;
        let before = state.done.insert("spdw:own".into(), own);
        if before.is_some_and(|b| own > b) {
            state.done.insert("spdw:fight".into(), now);
        }
    }
    for occ in list(wars, "occupied_own_states") {
        let Some(id) = occ["state_id"].as_i64().or_else(|| occ.as_i64()) else {
            continue;
        };
        if state.done.insert(format!("spdw:occ:{id}"), now).is_none() {
            state.done.insert("spdw:fight".into(), now);
        }
    }
    let ops = executing_operations(state);
    if !ops.is_empty() {
        return Some(format!("operation {} executes", ops.join(", ")));
    }
    let declared: Vec<&String> = planned
        .iter()
        .filter(|t| {
            state
                .done
                .get(&format!("spdw:new:{t}"))
                .is_some_and(|at| now - *at < QUIET)
        })
        .collect();
    if !declared.is_empty() {
        return Some(format!(
            "the war with {} was declared this week",
            declared
                .iter()
                .map(|t| t.as_str())
                .collect::<Vec<_>>()
                .join(", ")
        ));
    }
    if state
        .done
        .get("spdw:fight")
        .is_some_and(|at| now - *at < QUIET)
        && !at_war.is_empty()
    {
        return Some(format!(
            "it is attacked (losses or an own state lost this week; at war with {})",
            at_war.join(", ")
        ));
    }
    None
}

/// The game speed (1-5) the plan wants for a phase (`war`, `peace`, `wait`, `far`): war and
/// peace never above 3, waiting never above 4; pure.
pub(super) fn wanted(plan: &Value, phase: &str) -> i64 {
    let (default, max) = match phase {
        "war" => (2, 3),
        "wait" => (4, 4),
        "far" => (5, 5),
        _ => (3, 3),
    };
    plan["speed"][phase]
        .as_i64()
        .unwrap_or(default)
        .clamp(1, max)
}

/// What the rule reads for the phase without a war (see `next_phase`).
#[derive(Debug, Default, Clone, Copy)]
pub(super) struct Calm {
    /// A heavy action fell inside HEAVY_WINDOW, or an operation prepares.
    pub heavy_recent: bool,
    /// Heavy actions on HEAVY_DAYS distinct days of the window, or an operation prepares.
    pub heavy_sustained: bool,
    /// Speed 5 may start: alone (no human), budget left, no lane acted for EMPTY_QUIET, no open
    /// omission, the next due thing further than FAR_ENTER.
    pub far_ok: bool,
    /// Speed 5 must end: a lane acted, an omission opened, or the next due thing is within
    /// FAR_LEAVE (or unknown).
    pub far_exit: bool,
    /// The dwell is over and the year's budget is not spent.
    pub may_change: bool,
    /// The measured load leaves the executor time at speed 4.
    pub light: bool,
}

/// The next phase: `war` (2), `peace` (3, heavy work), `wait` (4, minor work), `far` (5, a long
/// empty stretch); `now_phase` None before the first phase; pure.
pub(super) fn next_phase(now_phase: Option<&str>, war: bool, c: Calm) -> &'static str {
    if war {
        return "war";
    }
    let calm = || {
        if c.heavy_recent || !c.light {
            "peace"
        } else if c.far_ok {
            "far"
        } else {
            "wait"
        }
    };
    match now_phase {
        // the first phase, and the end of a war: what the work wants now
        None | Some("war") => calm(),
        Some("far") if c.far_exit => {
            if c.heavy_sustained || !c.light {
                "peace"
            } else {
                "wait"
            }
        }
        Some("far") => "far",
        Some("wait") if !c.may_change => "wait",
        Some("wait") if c.heavy_sustained || !c.light => "peace",
        Some("wait") if c.far_ok => "far",
        Some("wait") => "wait",
        _ if !c.may_change || c.heavy_recent || !c.light => "peace",
        _ if c.far_ok => "far",
        _ => "wait",
    }
}

/// Game hours to every future date in the plan, from any string value; pure.
fn plan_dates(plan: &Value, now: GameHour, out: &mut Vec<GameHour>) {
    match plan {
        Value::String(t) => {
            let t = t
                .trim_start_matches("date:")
                .trim_start_matches(">=")
                .trim();
            if t.len() >= 8 && t.as_bytes()[4] == b'.' {
                if let Some(h) = super::date_hour(t).filter(|h| *h > now) {
                    out.push(h - now);
                }
            }
        }
        Value::Array(items) => items.iter().for_each(|v| plan_dates(v, now, out)),
        Value::Object(map) => map.values().for_each(|v| plan_dates(v, now, out)),
        _ => {}
    }
}

/// Game hours to the next thing the executor knows it will act on, from memory reads (research
/// slots and the focus, by the durations seen in this run) and the plan's dates; None when one
/// of them cannot be told (an empty slot or no focus is due now: 0). Remembers what it saw:
/// `spdr:<slot>:<tech>` / `spdf:<focus>` first seen, `spdr:obs` / `spdf:obs` the last
/// duration seen.
fn next_due(state: &mut State, tools: &dyn Tools, plan: &Value, now: GameHour) -> Option<GameHour> {
    let mut due: Vec<GameHour> = Vec::new();
    let research = tools.call("hoi4_research", &json!({})).ok()?;
    let mut seen: Vec<String> = Vec::new();
    for (i, slot) in list(&research, "slots").iter().enumerate() {
        let Some(tech) = slot["researching"].as_str() else {
            due.push(0);
            continue;
        };
        let key = format!("spdr:{i}:{tech}");
        let first = *state.done.entry(key.clone()).or_insert(now);
        seen.push(key);
        due.push(
            state
                .done
                .get("spdr:obs")?
                .saturating_sub(now - first)
                .max(0),
        );
    }
    // a slot whose technology changed: its duration is the latest seen
    let gone: Vec<(String, GameHour)> = state
        .done
        .iter()
        .filter(|(k, _)| k.starts_with("spdr:") && *k != "spdr:obs" && !seen.contains(k))
        .map(|(k, at)| (k.clone(), *at))
        .collect();
    for (k, at) in gone {
        state.done.remove(&k);
        if now - at > 24 {
            state.done.insert("spdr:obs".into(), now - at);
        }
    }
    let focus = tools
        .call("hoi4_focus_completed", &json!({"limit": 1}))
        .ok()?;
    match focus["active_key"].as_str() {
        Some(active) => {
            let key = format!("spdf:{active}");
            let gone: Vec<(String, GameHour)> = state
                .done
                .iter()
                .filter(|(k, _)| k.starts_with("spdf:") && *k != "spdf:obs" && **k != key)
                .map(|(k, at)| (k.clone(), *at))
                .collect();
            for (k, at) in gone {
                state.done.remove(&k);
                if now - at > 24 {
                    state.done.insert("spdf:obs".into(), now - at);
                }
            }
            let first = *state.done.entry(key).or_insert(now);
            due.push(
                state
                    .done
                    .get("spdf:obs")?
                    .saturating_sub(now - first)
                    .max(0),
            );
        }
        None => due.push(0),
    }
    plan_dates(plan, now, &mut due);
    due.into_iter().min()
}

/// What the rule does now; pure.
#[derive(Debug, PartialEq, Eq)]
pub(super) enum Decision {
    /// Nothing (the speed is what the phase wants, the game is paused, or a human chose).
    Keep,
    /// The game already runs at the phase's speed: remember it as the rule's.
    Holds,
    /// Set this internal speed (0-4).
    Set(i64),
    /// A human changed the speed this phase: tell the player once.
    Human,
}

/// `set` is the rule's speed in this phase (None: not yet), `human` whether a human's change
/// was noted, `before` the reading before this one (internal speed, running); pure.
pub(super) fn decide(
    current: i64,
    paused: bool,
    want_internal: i64,
    set: Option<i64>,
    human: bool,
    before: Option<(i64, bool)>,
) -> Decision {
    if paused || human {
        return Decision::Keep;
    }
    match set {
        None if current == want_internal => Decision::Holds,
        None => Decision::Set(want_internal),
        Some(s) if s == current => Decision::Keep,
        // changed while the game ran at the rule's speed: a human chose it
        Some(s) if before == Some((s, true)) => Decision::Human,
        // changed across a pause or before the rule saw it run (a launcher's unpause, its
        // start speed): the rule's speed again
        Some(s) => Decision::Set(s),
    }
}

fn phase_set(state: &State) -> Option<i64> {
    state
        .done
        .keys()
        .find_map(|k| k.strip_prefix("spd:set:").and_then(|v| v.parse().ok()))
}

fn last_read(state: &State) -> Option<(i64, bool)> {
    state.done.keys().find_map(|k| {
        let (speed, running) = k.strip_prefix("spd:last:")?.split_once(':')?;
        Some((speed.parse().ok()?, running == "1"))
    })
}

/// The fast lane: at most one `hoi4_time` a phase (and again after a launcher's unpause).
pub(super) fn step(
    state: &mut State,
    tools: &dyn Tools,
    plan: &Value,
    lease: &str,
    now: GameHour,
) -> Result<Option<String>> {
    if strings(&plan["manage"], "off").iter().any(|r| r == RULE)
        || plan["speed"] == json!(false)
        || state.skipped(now, "spd", "x")
    {
        return Ok(None);
    }
    state.skip(now, "spd", "x", READ_HOURS);
    let Ok(overview) = tools.call("hoi4_overview", &json!({})) else {
        return Ok(None);
    };
    let (Some(current), paused) = (
        overview["speed"].as_i64(),
        overview["paused"].as_bool().unwrap_or(false),
    ) else {
        return Ok(None);
    };
    let Ok(wars) = tools.call("hoi4_wars", &json!({})) else {
        return Ok(None);
    };
    let stats = tools.call("hoi4_war_stats", &json!({})).ok();
    let reason = war_reason(state, plan, &wars, stats.as_ref(), now);
    let war = reason.is_some();
    // heavy work (speed 3) or only minor work (4), an empty stretch (5); a run's start counts as
    // heavy work and as an action until seen quiet
    let open = state.sched.omissions;
    if state.round.heavy_days.is_empty() {
        state.round.heavy_days.push(now / 24);
    }
    let acted_at = *state.round.acted_at.get_or_insert(now);
    let preparing = super::plan_operation::preparing(state);
    let window: Vec<GameHour> = state
        .round
        .heavy_days
        .iter()
        .copied()
        .filter(|d| now / 24 - d < HEAVY_WINDOW / 24)
        .collect();
    let heavy_recent = preparing || !window.is_empty();
    let heavy_sustained = preparing || window.len() >= HEAVY_DAYS;
    // a human in the game (multiplayer, a human host, or a speed a human chose in this run):
    // only the human may choose speed 5. The executor is alone only when the plan says so
    // (`speed.alone`, set by an unattended harness) and no human's speed change was seen.
    let alone =
        plan["speed"]["alone"] == json!(true) && !state.done.contains_key("spdw:human_seen");
    let empty = !war && !heavy_recent && now.saturating_sub(acted_at) >= EMPTY_QUIET && open == 0;
    let now_phase: Option<&str> = ["war", "peace", "wait", "far"]
        .into_iter()
        .find(|p| state.done.contains_key(&format!("spd:phase:{p}")));
    let due_in = if empty && (alone || now_phase == Some("far")) {
        next_due(state, tools, plan, now)
    } else {
        None
    };
    let year = now / 24 / 365 - 5000;
    let year_key = format!("spdw:year:{year}");
    let spent = state.done.get(&year_key).copied().unwrap_or(0);
    let real = real_secs();
    learn_day(state, current + 1, !paused, now, real_ms());
    let light = light_at(state, 4, now, LOAD_WINDOW);
    let dwell_over = state
        .done
        .get("spdw:changed")
        .is_none_or(|at| now.saturating_sub(*at) >= DWELL_GAME)
        && state
            .done
            .get("spdw:changed_real")
            .is_none_or(|at| real.saturating_sub(*at) >= DWELL_REAL);
    let calm = Calm {
        heavy_recent,
        heavy_sustained,
        light,
        far_ok: alone
            && empty
            && light_at(state, 5, now, FAR_LOAD_WINDOW)
            && state.done.get("spdw:far_n").copied().unwrap_or(0) < FAR_MAX
            && due_in.is_some_and(|d| d > FAR_ENTER),
        far_exit: !empty || due_in.is_none_or(|d| d <= FAR_LEAVE),
        may_change: dwell_over && spent < YEAR_BUDGET,
    };
    let phase = next_phase(now_phase, war, calm);
    if now_phase != Some(phase) {
        state.done.retain(|k, _| !k.starts_with("spd:"));
        state.done.insert(format!("spd:phase:{phase}"), now);
        if now_phase.is_some() {
            state.done.insert("spdw:changed".into(), now);
            state.done.insert("spdw:changed_real".into(), real);
            // a change outside a war's start or end counts against the year's budget
            if phase != "war" && now_phase != Some("war") {
                *state.done.entry(year_key).or_insert(0) += 1;
            }
        }
        if phase == "far" {
            *state.done.entry("spdw:far_n".into()).or_insert(0) += 1;
        }
    }
    let game = wanted(plan, phase);
    let want = game - 1;
    let set = phase_set(state);
    let human = state.done.contains_key("spd:human");
    let before = last_read(state);
    state.done.retain(|k, _| !k.starts_with("spd:last:"));
    state.done.insert(
        format!("spd:last:{current}:{}", if paused { 0 } else { 1 }),
        now,
    );
    let against = match (&reason, phase) {
        (Some(r), _) => format!(" while it manages a war ({r})"),
        (None, "far") => format!(
            " in a long empty stretch (no lane acted for 2 days, no open omission; the next thing to act on in {} days)",
            due_in.unwrap_or(0) / 24
        ),
        (None, "wait") => " with only minor work due (picks, power, decisions)".to_string(),
        (None, _) => format!(
            " with heavy peace work under way (production, construction, armies, templates, air, an operation preparing){}",
            if open > 0 {
                format!("; {open} open omission(s)")
            } else {
                String::new()
            }
        ),
    };
    match decide(current, paused, want, set, human, before) {
        Decision::Keep => Ok(None),
        Decision::Holds => {
            state.done.insert(format!("spd:set:{current}"), now);
            Ok(None)
        }
        Decision::Human => {
            state.done.insert("spd:human".into(), now);
            state.done.insert("spdw:human_seen".into(), now);
            let message = format!(
                "the game runs at speed {} (set by a human); the agent prefers speed {game}{against}: at war an order and its read-back cost about a game day each at speed 3, heavy peace work speed 3, minor picks speed 4, a long empty stretch speed 5. It keeps the human's speed for this {phase} and does not change it again",
                current + 1
            );
            super::plan_manage::tell_player(state, plan, now, RULE, "speed:human", message);
            Ok(None)
        }
        Decision::Set(speed) => {
            let (ok, why) = state.act(tools, lease, "hoi4_time", json!({"speed": speed}), "spd")?;
            state.done.retain(|k, _| !k.starts_with("spd:set:"));
            // a refused change is not re-sent on a beat: the game's speed counts as the rule's
            state
                .done
                .insert(format!("spd:set:{}", if ok { speed } else { current }), now);
            if ok {
                state.done.retain(|k, _| !k.starts_with("spd:last:"));
                state.done.insert(format!("spd:last:{speed}:1"), now);
            }
            let line = format!(
                "speed {} -> {}{against}: {ok} {why}",
                current + 1,
                speed + 1
            );
            super::plan_manage::note(state, RULE, line.clone());
            Ok(Some(line))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn wars(enemies: &[&str], borders: &[&str]) -> Value {
        json!({"wars": {"enemies": enemies, "wars_won": []},
            "borders": borders.iter().map(|t| json!({"neighbour_tag": t, "hostile": true})).collect::<Vec<_>>(),
            "occupied_own_states": []})
    }

    fn stats(own: f64) -> Value {
        json!({"player": {"tag": "GER"}, "wars": [{"includes_player": true, "relations": [
            {"countries": [{"tag": "GER", "casualties": own}, {"tag": "FRA", "casualties": 10.0}]}]}]})
    }

    const DAY: GameHour = 24;

    #[test]
    fn the_phoney_war_is_peace_an_executing_operation_or_an_attack_is_war() {
        let plan = json!({"operations": [{"id": "op_low", "enemies": ["HOL", "BEL"]},
                                         {"id": "op_fra", "enemies": ["FRA"]}]});
        let mut st = State::default();
        // Germany borders France at war since 1939, nothing happens: peace for this rule
        let phoney = wars(&["FRA", "ENG", "POL"], &["FRA", "HOL"]);
        let t0 = 1000;
        assert_eq!(
            war_reason(&mut st, &plan, &phoney, Some(&stats(500.0)), t0),
            None
        );
        assert_eq!(
            war_reason(&mut st, &plan, &phoney, Some(&stats(500.0)), t0 + DAY),
            None
        );
        // Holland declared on: war at once (before the operation executes)
        let hol = wars(&["FRA", "ENG", "HOL"], &["FRA", "HOL"]);
        let r = war_reason(&mut st, &plan, &hol, Some(&stats(500.0)), t0 + 2 * DAY).unwrap();
        assert!(r.contains("HOL"), "{r}");
        // the operation executes until it ends, even past the week
        st.done.insert("op:op_low:exec".into(), t0 + 3 * DAY);
        let r = war_reason(&mut st, &plan, &hol, Some(&stats(500.0)), t0 + 12 * DAY).unwrap();
        assert!(r.contains("op_low executes"), "{r}");
        st.done.insert("op:op_low:end".into(), t0 + 20 * DAY);
        assert_eq!(executing_operations(&st), Vec::<String>::new());
        // ended, the French border quiet: peace again (France was never seen at peace)
        assert_eq!(
            war_reason(&mut st, &plan, &hol, Some(&stats(500.0)), t0 + 21 * DAY),
            None
        );
        // France attacks: own losses rise, war for a week after the last loss
        assert!(war_reason(&mut st, &plan, &hol, Some(&stats(900.0)), t0 + 22 * DAY).is_some());
        assert!(war_reason(&mut st, &plan, &hol, Some(&stats(900.0)), t0 + 28 * DAY).is_some());
        assert_eq!(
            war_reason(&mut st, &plan, &hol, Some(&stats(900.0)), t0 + 30 * DAY),
            None
        );
        // an own state lost: war
        let mut lost = hol.clone();
        lost["occupied_own_states"] = json!([{"state_id": 42, "controller": "FRA"}]);
        assert!(war_reason(&mut st, &plan, &lost, None, t0 + 31 * DAY).is_some());
        // still held a week later with nothing new: the attack is old news
        assert_eq!(war_reason(&mut st, &plan, &lost, None, t0 + 39 * DAY), None);
    }

    use std::cell::RefCell;
    use std::collections::HashMap;

    struct Game {
        replies: RefCell<HashMap<&'static str, Value>>,
        sent: RefCell<Vec<Value>>,
    }
    impl Game {
        fn new(speed: i64, paused: bool, wars: Value) -> Game {
            Game {
                replies: RefCell::new(
                    [
                        ("hoi4_overview", json!({"speed": speed, "paused": paused})),
                        ("hoi4_wars", wars),
                    ]
                    .into_iter()
                    .collect(),
                ),
                sent: RefCell::new(Vec::new()),
            }
        }
        fn screen(&self, speed: i64, paused: bool) {
            self.replies
                .borrow_mut()
                .insert("hoi4_overview", json!({"speed": speed, "paused": paused}));
        }
    }
    impl Tools for Game {
        fn call(&self, name: &str, args: &Value) -> Result<Value> {
            if args.get("request_id").is_some() {
                self.sent.borrow_mut().push(args.clone());
                if let Some(s) = args["speed"].as_i64() {
                    self.replies.borrow_mut().get_mut("hoi4_overview").unwrap()["speed"] = json!(s);
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
    fn a_phoney_war_runs_at_three_and_the_launchers_unpause_is_not_a_human() {
        // w46c1: the bed starts paused at the tool's speed 2 (the game's 3) with France at war
        let plan = json!({"operations": [{"id": "op_low", "enemies": ["HOL"]}]});
        let tools = Game::new(2, true, wars(&["FRA", "POL"], &["FRA", "HOL"]));
        let mut st = State::default();
        assert!(step(&mut st, &tools, &plan, "l", 1000).unwrap().is_none());
        // the launcher unpauses at the game's speed 4 (internal 3): the peace speed 3 is set
        tools.screen(3, false);
        let out = step(&mut st, &tools, &plan, "l", 1006).unwrap().unwrap();
        assert!(out.contains("speed 4 -> 3 with heavy peace work"), "{out}");
        // the harness pauses and unpauses at the game's speed 2: not a human, set again
        tools.screen(1, true);
        assert!(step(&mut st, &tools, &plan, "l", 1012).unwrap().is_none());
        tools.screen(1, false);
        assert!(step(&mut st, &tools, &plan, "l", 1018).unwrap().is_some());
        assert!(!st.done.contains_key("spd:human"));
        assert_eq!(tools.sent.borrow().len(), 2);
        // Holland declared on: the game's speed 2
        tools
            .replies
            .borrow_mut()
            .insert("hoi4_wars", wars(&["FRA", "POL", "HOL"], &["FRA", "HOL"]));
        let out = step(&mut st, &tools, &plan, "l", 1024).unwrap().unwrap();
        assert!(out.contains("speed 3 -> 2") && out.contains("HOL"), "{out}");
    }

    #[test]
    fn a_humans_change_while_running_is_kept_for_the_phase() {
        let plan = json!({"operations": [{"id": "op_low", "enemies": ["POL"]}]});
        let tools = Game::new(2, false, wars(&[], &["POL"]));
        let mut st = State::default();
        // peace at the game's speed 3: nothing sent, remembered as the rule's
        assert!(step(&mut st, &tools, &plan, "l", 1000).unwrap().is_none());
        // war with Poland: the game's speed 2 (internal 1)
        tools
            .replies
            .borrow_mut()
            .insert("hoi4_wars", wars(&["POL"], &["POL"]));
        let out = step(&mut st, &tools, &plan, "l", 1006).unwrap().unwrap();
        assert!(out.contains("speed 3 -> 2"), "{out}");
        assert_eq!(tools.sent.borrow()[0]["speed"], json!(1));
        // a human raises it to 3 while it runs: one note, never set again in this war
        tools.screen(2, false);
        assert!(step(&mut st, &tools, &plan, "l", 1012).unwrap().is_none());
        assert!(st.done.contains_key("spd:human"));
        assert!(step(&mut st, &tools, &plan, "l", 1018).unwrap().is_none());
        assert_eq!(
            tools.sent.borrow().len(),
            1,
            "no retry after a human's change"
        );
        // the switch
        let off = json!({"manage": {"off": ["speed"]}});
        let mut fresh = State::default();
        assert!(step(&mut fresh, &tools, &off, "l", 1000).unwrap().is_none());
    }

    #[test]
    fn peace_three_war_two_wait_four_never_five() {
        assert_eq!(wanted(&json!({}), "peace"), 3);
        assert_eq!(wanted(&json!({}), "war"), 2);
        assert_eq!(wanted(&json!({}), "wait"), 4);
        assert_eq!(wanted(&json!({"speed": {"peace": 5}}), "peace"), 3);
        assert_eq!(wanted(&json!({"speed": {"wait": 5}}), "wait"), 4);
    }

    fn calm(
        heavy_recent: bool,
        heavy_sustained: bool,
        far_ok: bool,
        far_exit: bool,
        may_change: bool,
    ) -> Calm {
        Calm {
            heavy_recent,
            heavy_sustained,
            far_ok,
            far_exit,
            may_change,
            light: true,
        }
    }

    #[test]
    fn the_phase_by_the_weight_of_the_work_with_dwell_and_war_at_once() {
        // first phase: heavy work 3, minor work 4, an empty stretch 5
        assert_eq!(
            next_phase(None, false, calm(true, false, false, true, true)),
            "peace"
        );
        assert_eq!(
            next_phase(None, false, calm(false, false, false, true, true)),
            "wait"
        );
        // a war: 2 at once from any phase, dwell or not
        for p in ["peace", "wait", "far"] {
            assert_eq!(
                next_phase(Some(p), true, calm(false, false, false, false, false)),
                "war"
            );
        }
        // stepping up waits out the dwell
        assert_eq!(
            next_phase(Some("peace"), false, calm(false, false, false, true, false)),
            "peace"
        );
        assert_eq!(
            next_phase(Some("peace"), false, calm(false, false, false, true, true)),
            "wait"
        );
        // a single short heavy task at 4 is done at 4; sustained heavy work takes 3 after the dwell
        assert_eq!(
            next_phase(Some("wait"), false, calm(true, false, false, true, true)),
            "wait"
        );
        assert_eq!(
            next_phase(Some("wait"), false, calm(true, true, false, true, false)),
            "wait"
        );
        assert_eq!(
            next_phase(Some("wait"), false, calm(true, true, false, true, true)),
            "peace"
        );
        // 5 only when allowed, left at once before the next due thing
        assert_eq!(
            next_phase(Some("wait"), false, calm(false, false, true, false, true)),
            "far"
        );
        assert_eq!(
            next_phase(Some("far"), false, calm(false, false, false, true, false)),
            "wait"
        );
        assert_eq!(wanted(&json!({}), "far"), 5);
    }

    /// A game without a war, with two research slots and a focus running.
    fn calm_game() -> Game {
        let tools = Game::new(2, false, wars(&[], &["HOL"]));
        tools.replies.borrow_mut().insert(
            "hoi4_research",
            json!({"slots": [{"researching": "a"}, {"researching": "b"}]}),
        );
        tools
            .replies
            .borrow_mut()
            .insert("hoi4_focus_completed", json!({"active_key": "f1"}));
        tools
    }

    fn set_real(secs: i64) {
        REAL.with(|r| r.set(secs));
    }

    #[test]
    fn no_four_three_four_on_alternating_minor_and_heavy_days() {
        const DAY: GameHour = 24;
        let plan = json!({"operations": [{"id": "op_low", "enemies": ["HOL"]}]});
        let tools = calm_game();
        let mut st = State::default();
        let t0 = 1000 * 365 * DAY;
        set_real(0);
        // the run's start counts as heavy work: 3
        assert!(step(&mut st, &tools, &plan, "l", t0).unwrap().is_none());
        // heavy work every other day for 60 days: 3 throughout, nothing sent
        let mut changes = 0;
        for day in 1..60 {
            set_real(day * 100);
            if day % 2 == 0 {
                st.round.lane_acted("construction", t0 + day * DAY);
            } else {
                st.round.lane_acted("research", t0 + day * DAY);
            }
            if step(&mut st, &tools, &plan, "l", t0 + day * DAY)
                .unwrap()
                .is_some()
            {
                changes += 1;
            }
        }
        assert_eq!(changes, 0);
        // only minor work for two weeks: 4, once
        for day in 60..80 {
            set_real(day * 100);
            st.round.lane_acted("focus", t0 + day * DAY);
            if let Some(out) = step(&mut st, &tools, &plan, "l", t0 + day * DAY).unwrap() {
                assert!(out.contains("speed 3 -> 4"), "{out}");
                changes += 1;
            }
        }
        assert_eq!(changes, 1);
        // one heavy task inside the quiet stretch: done at 4
        set_real(8100);
        st.round.lane_acted("production", t0 + 81 * DAY);
        assert!(step(&mut st, &tools, &plan, "l", t0 + 81 * DAY)
            .unwrap()
            .is_none());
        // the real-time dwell: game weeks passed but 10 real seconds: heavy days do not change it
        for day in [82, 83, 84] {
            st.round.lane_acted("templates", t0 + day * DAY);
        }
        set_real(7900 + 10);
        st.done.insert("spdw:changed_real".into(), 7900);
        assert!(step(&mut st, &tools, &plan, "l", t0 + 96 * DAY)
            .unwrap()
            .is_none());
        // a war: 2 at once
        tools
            .replies
            .borrow_mut()
            .insert("hoi4_wars", wars(&["HOL"], &["HOL"]));
        let out = step(&mut st, &tools, &plan, "l", t0 + 96 * DAY + 6)
            .unwrap()
            .unwrap();
        assert!(out.contains("-> 2"), "{out}");
    }

    #[test]
    fn speed_five_only_alone_rare_and_left_before_the_next_due_thing() {
        const DAY: GameHour = 24;
        let t0 = 1000 * 365 * DAY;
        let run = |plan: &Value| {
            let tools = calm_game();
            let mut st = State::default();
            st.done.insert("spdr:obs".into(), 100 * DAY);
            st.done.insert("spdf:obs".into(), 120 * DAY);
            set_real(0);
            step(&mut st, &tools, plan, "l", t0).unwrap();
            set_real(10_000);
            let out = step(&mut st, &tools, plan, "l", t0 + 15 * DAY).unwrap();
            (st, tools, out)
        };
        // a human may be present (no `speed.alone`): 4, never 5
        let (_, _, out) = run(&json!({}));
        assert!(out.unwrap().contains("speed 3 -> 4"));
        // alone: 5 in the long empty stretch
        let alone = json!({"speed": {"alone": true}});
        let (mut st, tools, out) = run(&alone);
        let out = out.unwrap();
        assert!(
            out.contains("speed 3 -> 5") && out.contains("empty"),
            "{out}"
        );
        // ten days before the research completes (first seen on day 15, 100 days): 4 at once
        let out = step(&mut st, &tools, &alone, "l", t0 + 106 * DAY)
            .unwrap()
            .unwrap();
        assert!(out.contains("speed 5 -> 4"), "{out}");
        // a human's change seen: never 5 again in this run
        st.done.insert("spdw:human_seen".into(), t0);
        assert!(!st.done.contains_key("spd:phase:far"));
    }

    #[test]
    fn a_light_load_runs_faster_a_heavy_load_stays_at_three_even_with_only_minor_work() {
        const DAY: GameHour = 24;
        let alone = json!({"speed": {"alone": true}});
        let t0 = 1000 * 365 * DAY;
        // LIGHT: a few one-second picks a month (a small or isolated country)
        let tools = calm_game();
        let mut light = State::default();
        light.done.insert("spdr:obs".into(), 100 * DAY);
        light.done.insert("spdf:obs".into(), 120 * DAY);
        light.round.cost(t0 - 40 * DAY, 0.02);
        set_real(0);
        step(&mut light, &tools, &alone, "l", t0).unwrap();
        set_real(10_000);
        let out = step(&mut light, &tools, &alone, "l", t0 + 15 * DAY)
            .unwrap()
            .unwrap();
        assert!(out.contains("-> 5"), "{out}");
        // HEAVY: a built-up country, only minor picks but six 3-second actions a game day
        let mut busy = State::default();
        for h in 0..(15 * DAY) {
            if h % 4 == 0 {
                busy.round.cost(t0 + h, 3.0);
                busy.round.lane_acted("research", t0 + h);
            }
        }
        set_real(0);
        step(&mut busy, &tools, &alone, "l", t0).unwrap();
        set_real(10_000);
        assert!(step(&mut busy, &tools, &alone, "l", t0 + 15 * DAY)
            .unwrap()
            .is_none());
        assert!(busy.done.contains_key("spd:phase:peace"));
    }

    #[test]
    fn a_game_day_is_measured_at_the_speed_it_ran() {
        let mut st = State::default();
        learn_day(&mut st, 4, true, 1000, 0);
        learn_day(&mut st, 4, true, 1024, 1200);
        assert_eq!(day_ms(&st, 4) as i64, 1200);
        // a pause between reads teaches nothing
        learn_day(&mut st, 4, false, 1048, 9000);
        learn_day(&mut st, 4, true, 1072, 99_000);
        assert_eq!(day_ms(&st, 4) as i64, 1200);
    }

    #[test]
    fn the_years_budget_stops_further_changes() {
        const DAY: GameHour = 24;
        let plan = json!({});
        let tools = calm_game();
        let mut st = State::default();
        let t0 = 1000 * 365 * DAY;
        set_real(0);
        step(&mut st, &tools, &plan, "l", t0).unwrap();
        let year = t0 / 24 / 365 - 5000;
        st.done.insert(format!("spdw:year:{year}"), YEAR_BUDGET);
        set_real(100_000);
        assert!(step(&mut st, &tools, &plan, "l", t0 + 30 * DAY)
            .unwrap()
            .is_none());
    }

    #[test]
    fn the_phase_speed_is_set_and_only_a_change_while_running_is_a_humans() {
        // the phase's speed already runs: remembered
        assert_eq!(decide(2, false, 2, None, false, None), Decision::Holds);
        // war starts: speed 2 (internal 1)
        assert_eq!(decide(2, false, 1, None, false, None), Decision::Set(1));
        // paused: waits, the speed stays due
        assert_eq!(decide(2, true, 1, None, false, None), Decision::Keep);
        // it holds
        assert_eq!(
            decide(1, false, 1, Some(1), false, Some((1, true))),
            Decision::Keep
        );
        // changed while it ran at the rule's speed: a human
        assert_eq!(
            decide(2, false, 1, Some(1), false, Some((1, true))),
            Decision::Human
        );
        assert_eq!(
            decide(2, false, 1, Some(1), true, Some((2, true))),
            Decision::Keep
        );
        // changed across a pause: the launcher's unpause, set again
        assert_eq!(
            decide(2, false, 1, Some(1), false, Some((1, false))),
            Decision::Set(1)
        );
    }
}
