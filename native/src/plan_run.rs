//! Standing-plan runner: the routine a player declares once in `plan.json`
//! (research priorities, a focus path, construction, training, production,
//! politics, decisions) is carried out one verified action at a time through
//! the same tools a model would call, so the model only wakes for war,
//! diplomacy, armies and real decisions. Nothing here pauses the game or
//! bypasses a tool: every action is an ordinary tool call under the caller's
//! input lease, journaled and verified like any other.
//!
//! Plan keys (all optional; the file is re-read every tick):
//!   active                 false pauses the runner
//!   stop_on_date           "YYYY.MM.DD" or {"year","month","day"}: when the game date (hoi4_overview) reaches or
//!                          passes it the runner pauses the game once through hoi4_time (paused:true, its own
//!                          request id; skipped when the overview already says paused), reports
//!                          `stopped_on_date:true` with a `reason`, and takes no other action again. A refused or
//!                          uncertain pause is reported in `stop_on_date_pause` and never retried. Alarm `target_date_reached`
//!   stop_on_date_pause     false: only stop acting at stop_on_date, never pause (default true)
//!   stop_on_failure        true returns the first refused/diverged action instead of trying other work
//!   (all timers are GAME time, measured in in-game hours from `hoi4_overview`'s date,
//!   so a failed action is skipped for N game days whatever the speed, and nothing
//!   advances while the game is paused)
//!   alarms                 thresholds for the compact digest's `alarms` list; see `crate::alarms`
//!   event_prefix_options   {"DOD_":"last"}: option for every event whose id starts with the prefix
//!   auto_events            answer news and single-option events with option 0 (default true);
//!                          other events after `wait_events_days` game days (default 2) with
//!                          `event_options[event]` (default 0)
//!   research_priority      technology keys; the first not-yet-researched key the game accepts
//!                          (then the catalog in order, unless research_fallback is false)
//!   research_fallback      default true: an empty slot with nothing startable in research_priority takes the
//!                          first startable catalogue tech (this can be naval hull junk; every pick raises the
//!                          alarm `research_fallback_used`). false or "priority_only": never use the catalogue
//!   research_fallback_prefixes   ["infantry","support_"]: the catalogue fallback only takes keys starting with one
//!   research_fallback_categories ["armor","air"]: ... or containing one (catalogue keys carry no category field,
//!                          so a category is a word inside the key). With either list the fallback is restricted
//!   focus_path             focus keys; the first not completed key the game accepts. A key the game blocks
//!                          (focus_not_available, prerequisite, exclusive) is skipped for `skip_hours.focus_blocked`
//!                          and the next key is tried at once; `focus_blocked:[key:reason]` is in the digest
//!   focus_fallback         focus keys tried only after every focus_path key is completed or skipped
//!   skip_hours             {"tech":72,"tech_fallback":168,"focus":24,"focus_blocked":6}: game hours a refused key
//!                          is left alone (defaults shown); all optional
//!                          The digest carries one `research` line and one `focus` line per run (like `construction`)
//!                          and the alarms `research_slot_idle_plan_exhausted` / `no_active_focus_plan_exhausted`
//!                          when nothing from the plan could be started. `active` missing means active; only
//!                          `"active": false` pauses the runner (result reason `plan inactive (active:false)`)
//!   politics               [{"category","slot","idea"}] one-shot picks
//!   decisions              decision keys taken whenever enabled
//!   training               {"template","queue","batch","deploy_state"} keep a training queue
//!   production_priority    equipment prefixes that receive military factories on no line
//!   production_min_idle    idle military factories needed before rebalancing (default 2)
//!   production             [{"equipment","factories"}] lines added once
//!   construction           [{"building","states":[..],"count":1}] spent while civilian factories are idle;
//!                          states are ids (or numeric strings, or exact state names, resolved with
//!                          hoi4_states); states with no free slot are remembered full for
//!                          `construction_full_days` (default 14) game days; free slots are read with
//!                          hoi4_state_slots unless `construction_probe_slots` is false
//!   construction_min_idle  idle civilian factories needed first (default 4)
//!   construction_fallback_any_own_state  true: when every listed state is full, build in other own states
//!                          (hoi4_states with the player tag); every tick's result names why
//!                          nothing was queued in `construction`
//!   war                    {"enemies":["FRA"],"min_army":6,"per_target":2,"reserve":["Army 9"],"cooldown_hours":12}
//!                          send armies at the nearest enemy-held states (own states held by an enemy first):
//!                          step into the state, draw front and offensive lines, execute the plan
//!   armies                 {"max":24,"min_for_general":8,"generals":["Name",..]} keep every division in an
//!                          army of at most `max`: create armies where divisions stand unassigned (folding in
//!                          standing armies when the game refuses and "absorb" is true), split oversized ones, and give the next
//!                          listed general to an army of at least `min_for_general` divisions
use crate::error::{Error, Result};
use serde_json::{json, Map, Value};
use std::{
    collections::{HashMap, HashSet},
    path::Path,
    time::Duration,
};

/// Game time is counted in in-game hours (the `raw` value of the `date` object
/// that `hoi4_overview` returns). Every skip, cooldown and patience below is a
/// span of game hours, so the plan behaves the same at speed 1, 5 or paused.
pub type GameHour = i64;
/// One in-game day in [`GameHour`] units.
pub const DAY: GameHour = 24;

/// One tool call by name (a read, or an action with its own request id).
pub trait Tools {
    fn call(&self, name: &str, args: &Value) -> Result<Value>;
}

/// What one tick did.
#[derive(Debug, PartialEq)]
pub enum Tick {
    Idle,
    Acted(String),
    /// An event the plan will not answer yet; the model may choose
    /// (`new` on the first tick that sees it).
    Waiting {
        event: String,
        new: bool,
    },
    Stop(String),
    /// `stop_on_date` reached: the runner is done (the text says how the pause went).
    DateStop(String),
    Nothing,
}

pub struct State {
    stop_on_failure: bool,
    boot: u64,
    bad: HashMap<String, GameHour>,
    last_hour: GameHour,
    done: HashSet<String>,
    seen: HashMap<String, GameHour>,
    seq: u64,
    sent: HashMap<String, (String, Value)>,
    war_assign: HashMap<String, i32>,
    war_stage: HashMap<String, u8>,
    war_cool: HashMap<String, GameHour>,
    centers: HashMap<i32, (f64, f64)>,
    con: ConState,
    /// stop_on_date: (target hour, outcome of the single pause attempt)
    date_stop: Option<(i64, String)>,
    stages: Stages,
}

/// What the research and focus stages saw this tick (the digest lines).
#[derive(Default)]
struct Stages {
    /// (digest line, a slot is idle and the plan has nothing left to start)
    research: Option<(String, bool)>,
    /// (digest line, no focus is active and the plan has nothing left to start)
    focus: Option<(String, bool)>,
    /// catalogue techs the fallback started during this run
    fallback_used: Vec<String>,
    /// short refusal reason per focus key, for `focus_blocked`
    focus_why: HashMap<String, String>,
}

/// Skip window in game hours: `skip_hours.<key>` of the plan, else `default`.
fn skip_hours(plan: &Value, key: &str, default: GameHour) -> GameHour {
    plan["skip_hours"][key]
        .as_i64()
        .filter(|h| *h > 0)
        .unwrap_or(default)
}

/// Memory of the construction stage (all times in game hours).
#[derive(Default)]
struct ConState {
    /// free slots read by hoi4_state_slots: (free, hour read), decremented per queued building
    free: HashMap<i64, (u32, GameHour)>,
    /// largest `count` worth trying in a state (a partial add proved it)
    cap: HashMap<i64, i64>,
    /// consecutive transient failures per state
    fails: HashMap<i64, u32>,
    /// state name -> id, from hoi4_states
    names: HashMap<String, i64>,
    /// own states (hour read, ids), for the optional fallback
    own: Option<(GameHour, Vec<i64>)>,
    /// rotates the first candidate so the listed states take turns
    rr: usize,
    pause_until: GameHour,
    probe_off_until: GameHour,
    queued_total: u64,
    last_queue: Option<GameHour>,
    baseline: Option<GameHour>,
    /// the last tick queued a building: leave the next tick to armies and war
    yield_next: bool,
    /// this tick's digest line (None when the stage was not reached)
    digest: Option<String>,
}
impl Default for State {
    /// Request ids carry the start time so a restarted server never reuses
    /// an id that an earlier run already bound to a different action.
    fn default() -> Self {
        Self {
            stop_on_failure: false,
            boot: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_or(0, |d| d.as_secs()),
            bad: HashMap::new(),
            last_hour: 0,
            done: HashSet::new(),
            seen: HashMap::new(),
            seq: 0,
            sent: HashMap::new(),
            war_assign: HashMap::new(),
            war_stage: HashMap::new(),
            war_cool: HashMap::new(),
            centers: HashMap::new(),
            con: ConState::default(),
            date_stop: None,
            stages: Stages::default(),
        }
    }
}

fn skip_key(kind: &str, key: &str) -> String {
    format!("{kind}:{key}")
}
impl State {
    fn skip(&mut self, now: GameHour, kind: &str, key: &str, hours: GameHour) {
        self.bad.insert(skip_key(kind, key), now + hours);
    }
    fn skipped(&self, now: GameHour, kind: &str, key: &str) -> bool {
        self.bad
            .get(&skip_key(kind, key))
            .is_some_and(|until| *until > now)
    }
    /// One action with a fresh id; an uncertain dispatch is settled by
    /// retrying its own id once. If readback remains uncertain, stop the
    /// runner before it can try a fallback under a new id.
    fn act(
        &mut self,
        tools: &dyn Tools,
        lease: &str,
        name: &str,
        args: Value,
        tag: &str,
    ) -> Result<(bool, String)> {
        self.seq += 1;
        let mut args = args;
        if let Some(object) = args.as_object_mut() {
            object.insert(
                "request_id".into(),
                json!(format!("plan-{tag}-{}-{}", self.boot, self.seq)),
            );
            object.insert("lease_token".into(), json!(lease));
        }
        let id = args["request_id"].as_str().unwrap_or("").to_string();
        self.sent.insert(id, (name.to_string(), args.clone()));
        let mut result = tools.call(name, &args);
        if matches!(&result, Err(e) if e.code == "action_uncertain") {
            // Another dispatch of this run is unresolved and blocks input: settle
            // it by retrying its own request id, then send this one again.
            let blocker = match &result {
                Err(e) => e
                    .message
                    .split("Unresolved dispatch ")
                    .nth(1)
                    .and_then(|rest| rest.split_whitespace().next())
                    .map(String::from),
                _ => None,
            };
            if let Some((blocked_name, blocked_args)) =
                blocker.and_then(|b| self.sent.get(&b).cloned())
            {
                let _ = tools.call(&blocked_name, &blocked_args);
            }
            result = tools.call(name, &args);
        }
        match result {
            Err(e) if e.code == "action_uncertain" => Err(e),
            Err(e) if self.stop_on_failure => Err(e),
            Err(e) => {
                let mut text = format!("{}: {}", e.code, e.message);
                let mut end = text.len().min(160);
                while !text.is_char_boundary(end) {
                    end -= 1;
                }
                text.truncate(end);
                Ok((false, text))
            }
            Ok(body) => {
                let status = body["receipt"]["status"].as_str().unwrap_or("").to_string();
                match status.as_str() {
                    "Verified" => Ok((true, status)),
                    "NotApplied" | "Diverged" if self.stop_on_failure => Err(Error::new(
                        "plan_action_failed",
                        format!("{name} returned {status}; standing plan stopped"),
                    )),
                    "NotApplied" | "Diverged" => Ok((false, status)),
                    _ => Err(Error::new(
                        "action_uncertain",
                        format!("{name} returned no terminal receipt; settle request {} before new input", args["request_id"]),
                    )),
                }
            }
        }
    }
}

fn text(v: &Value) -> &str {
    v.as_str().unwrap_or("")
}
fn list<'a>(v: &'a Value, key: &str) -> &'a [Value] {
    v[key].as_array().map(Vec::as_slice).unwrap_or(&[])
}
fn strings(v: &Value, key: &str) -> Vec<String> {
    list(v, key)
        .iter()
        .filter_map(|s| s.as_str().map(String::from))
        .collect()
}

/// One pass over the plan: the first thing that can be done is done.
pub fn tick(
    state: &mut State,
    tools: &dyn Tools,
    plan: &Value,
    lease: &str,
    now: GameHour,
) -> Result<Tick> {
    if plan["active"] == json!(false) {
        return Ok(Tick::Idle);
    }
    state.stop_on_failure = match plan.get("stop_on_failure") {
        None => false,
        Some(Value::Bool(value)) => *value,
        _ => {
            return Err(Error::new(
                "invalid_plan",
                "stop_on_failure must be boolean",
            ))
        }
    };
    // stop_on_date comes before everything, events included: once the date is
    // reached the runner pauses once (journal-safe) and then never acts again.
    if let Some((target, label)) = crate::alarms::stop_on_date(plan)? {
        let pause = match plan.get("stop_on_date_pause") {
            None | Some(Value::Bool(true)) => true,
            Some(Value::Bool(false)) => false,
            _ => {
                return Err(Error::new(
                    "invalid_plan",
                    "stop_on_date_pause must be boolean",
                ))
            }
        };
        let known = matches!(&state.date_stop, Some((t, _)) if *t == target);
        if known || now >= target {
            if !known {
                let outcome = date_stop_pause(state, tools, lease, pause);
                state.date_stop = Some((target, outcome));
            }
            let outcome = state.date_stop.as_ref().map_or("", |(_, o)| o.as_str());
            let at = i32::try_from(now)
                .ok()
                .and_then(|raw| crate::calendar::decode(raw).ok())
                .map_or_else(|| format!("hour {now}"), |d| d.text);
            return Ok(Tick::DateStop(format!(
                "stop_on_date {label} reached (game date {at}); pause: {outcome}"
            )));
        }
    }
    state.con.digest = None;
    state.stages.research = None;
    state.stages.focus = None;
    state.con.baseline.get_or_insert(now);
    // 1. events (they block every other input)
    let events = tools.call("hoi4_events", &json!({}))?;
    if events["peace_conference_open"] == json!(true) {
        return Ok(Tick::Stop("peace conference open".into()));
    }
    if let Some(top) = list(&events, "events").last() {
        let key = text(&top["event"]).to_string();
        let new = !state.seen.contains_key(&key);
        let first = *state.seen.entry(key.clone()).or_insert(now);
        let auto = plan["auto_events"] != json!(false);
        let news = key.contains("news") || list(top, "options").len() == 1;
        let waited = now.saturating_sub(first);
        let patience = plan["wait_events_days"]
            .as_i64()
            .map_or(2 * DAY, |days| days.saturating_mul(DAY));
        // A choice event is the model's decision unless the plan names an option for it
        // or explicitly allows answering every event ("auto_events": "all").
        let chosen = plan["event_options"][&key].as_u64().or_else(|| {
            // {"event_prefix_options": {"DOD_": "last"}}: a rule for a whole family of events
            plan["event_prefix_options"]
                .as_object()?
                .iter()
                .find_map(|(prefix, v)| {
                    key.starts_with(prefix.as_str()).then(|| {
                        if v == "last" {
                            list(top, "options").len().saturating_sub(1) as u64
                        } else {
                            v.as_u64().unwrap_or(0)
                        }
                    })
                })
        });
        let all = plan["auto_events"] == json!("all");
        if auto && (news || chosen.is_some() || (waited > patience && all)) {
            let option = chosen.unwrap_or(0);
            let (ok, why) = state.act(
                tools,
                lease,
                "hoi4_event_choose",
                json!({"event": key, "option": option}),
                "ev",
            )?;
            return Ok(Tick::Acted(format!(
                "event {key} option {option}: {ok} {why}"
            )));
        }
        return Ok(Tick::Waiting { event: key, new });
    }
    // 2. research
    let research = tools.call("hoi4_research", &json!({"limit": 1}))?;
    let slots: Vec<Option<String>> = list(&research, "slots")
        .iter()
        .map(|s| s["researching"].as_str().map(String::from))
        .collect();
    if let Some(empty) = slots.iter().position(Option::is_none) {
        let idle_slots = slots.iter().filter(|s| s.is_none()).count();
        let mut done: HashSet<String> = HashSet::new();
        let mut offset = 0;
        loop {
            let page = tools.call("hoi4_research", &json!({"offset": offset, "limit": 100}))?;
            done.extend(
                list(&page, "researched")
                    .iter()
                    .filter_map(|t| t["id"].as_str().map(String::from)),
            );
            match page["next_offset"].as_u64() {
                Some(next) if next > offset => offset = next,
                _ => break,
            }
        }
        let busy: HashSet<String> = slots.iter().flatten().cloned().collect();
        let priority = strings(plan, "research_priority");
        let open: Vec<&String> = priority
            .iter()
            .filter(|t| !done.contains(*t) && !busy.contains(*t))
            .collect();
        let waiting = open.iter().filter(|t| state.skipped(now, "tech", t)).count();
        let candidates: Vec<String> = open
            .into_iter()
            .filter(|t| !state.skipped(now, "tech", t))
            .take(3)
            .cloned()
            .collect();
        let mut refused: Vec<String> = Vec::new();
        for tech in candidates {
            let (ok, why) = state.act(
                tools,
                lease,
                "hoi4_research_start",
                json!({"technology": tech, "slot": empty}),
                "rs",
            )?;
            if ok {
                state.stages.research = Some((
                    format!("research: started {tech} slot {empty} ({idle_slots} slot(s) were idle)"),
                    false,
                ));
                return Ok(Tick::Acted(format!("research {tech} slot {empty}")));
            }
            state.skip(now, "tech", &tech, skip_hours(plan, "tech", 3 * DAY));
            refused.push(format!("{tech}:{}", why.split(':').next().unwrap_or("")));
        }
        // Nothing from the priority list is startable: never leave a slot idle.
        // Walk the technology catalog; the game refuses ineligible ones before
        // any input, so a few attempts per tick are cheap. Plans can switch this
        // off ("priority_only") or restrict it to wanted key prefixes/words.
        let fallback_on = plan["research_fallback"] != json!(false)
            && plan["research_fallback"] != json!("priority_only");
        let prefixes = strings(plan, "research_fallback_prefixes");
        let categories = strings(plan, "research_fallback_categories");
        let wanted = |tech: &str| {
            (prefixes.is_empty() && categories.is_empty())
                || prefixes.iter().any(|p| tech.starts_with(p.as_str()))
                || categories.iter().any(|c| tech.contains(c.as_str()))
        };
        if fallback_on {
            let mut tried = 0;
            let mut offset = 0;
            'catalog: loop {
                let page = tools.call(
                    "hoi4_technology_list",
                    &json!({"offset": offset, "limit": 100}),
                )?;
                for tech in strings(&page, "technologies") {
                    if done.contains(&tech)
                        || busy.contains(&tech)
                        || !wanted(&tech)
                        || state.skipped(now, "tech", &tech)
                    {
                        continue;
                    }
                    let (ok, _) = state.act(
                        tools,
                        lease,
                        "hoi4_research_start",
                        json!({"technology": tech, "slot": empty}),
                        "rf",
                    )?;
                    if ok {
                        state.stages.fallback_used.push(tech.clone());
                        state.stages.research = Some((
                            format!("research: started {tech} slot {empty} from the catalogue fallback (priority list had nothing startable)"),
                            false,
                        ));
                        return Ok(Tick::Acted(format!(
                            "research {tech} slot {empty} (fallback)"
                        )));
                    }
                    state.skip(
                        now,
                        "tech",
                        &tech,
                        skip_hours(plan, "tech_fallback", 7 * DAY),
                    );
                    tried += 1;
                    if tried >= 6 {
                        break 'catalog;
                    }
                }
                match page["next_offset"].as_u64() {
                    Some(next) if next > offset => offset = next,
                    _ => break,
                }
            }
        }
        let fallback = if !fallback_on {
            "catalogue fallback off"
        } else if prefixes.is_empty() && categories.is_empty() {
            "catalogue fallback found nothing startable"
        } else {
            "restricted catalogue fallback found nothing startable"
        };
        state.stages.research = Some((
            format!(
                "research: {idle_slots} slot(s) idle, plan exhausted ({} priority done, {} on skip timer, refused now [{}]; {fallback})",
                priority.iter().filter(|t| done.contains(*t)).count(),
                waiting,
                refused.join(", ")
            ),
            true,
        ));
    } else if !slots.is_empty() {
        state.stages.research = Some((format!("research: all {} slot(s) busy", slots.len()), false));
    }
    // 3. focus
    let focus = tools.call("hoi4_focus_completed", &json!({"limit": 100}))?;
    if let Some(active) = focus["active_key"].as_str() {
        state.stages.focus = Some((format!("focus: active {active}"), false));
    } else {
        let completed = strings(&focus, "completed");
        let path = strings(plan, "focus_path");
        let fallback = strings(plan, "focus_fallback");
        let startable = |state: &State, keys: &[String]| -> Vec<String> {
            keys.iter()
                .filter(|f| !completed.contains(*f) && !state.skipped(now, "focus", f))
                .cloned()
                .collect()
        };
        let mut candidates: Vec<String> = startable(state, &path).into_iter().take(8).collect();
        candidates.extend(startable(state, &fallback).into_iter().take(4));
        for key in candidates {
            let (ok, why) = state.act(
                tools,
                lease,
                "hoi4_focus_start",
                json!({"focus": key}),
                "fo",
            )?;
            if ok {
                state.stages.focus_why.remove(&key);
                state.stages.focus = Some((format!("focus: started {key}"), false));
                return Ok(Tick::Acted(format!("focus {key}")));
            }
            // A focus the game disables (requirements, exclusive branch) is
            // retried soon and does not hold up the next key in the path.
            let blocked = why.contains("focus_not_available")
                || why.contains("prerequisite")
                || why.contains("exclusive");
            let hours = if blocked {
                skip_hours(plan, "focus_blocked", 6)
            } else {
                skip_hours(plan, "focus", DAY)
            };
            state.skip(now, "focus", &key, hours);
            let code = why.split(':').next().unwrap_or("refused").to_string();
            state.stages.focus_why.insert(key, code);
        }
        if !path.is_empty() || !fallback.is_empty() {
            let left = startable(state, &path).len() + startable(state, &fallback).len();
            let all_done = path.iter().chain(&fallback).all(|f| completed.contains(f));
            let blocked = focus_blocked(state, plan, now);
            let line = if all_done {
                "focus: none active, focus_path and focus_fallback are all completed".to_string()
            } else {
                format!("focus: none active, nothing started; blocked [{}]", blocked.join(", "))
            };
            state.stages.focus = Some((line, left == 0));
        }
    }
    // 4. politics (one-shot picks)
    for (index, pick) in list(plan, "politics").iter().enumerate() {
        let key = format!("pol:{index}:{}", text(&pick["idea"]));
        if state.done.contains(&key) || state.skipped(now, "pol", &index.to_string()) {
            continue;
        }
        let (ok, why) = state.act(
            tools,
            lease,
            "hoi4_politics_choose",
            json!({"category": pick["category"], "slot": pick["slot"], "idea": pick["idea"]}),
            "po",
        )?;
        if ok || why.contains("already holds") {
            state.done.insert(key);
        } else {
            // Often just short of political power: try again later.
            state.skip(now, "pol", &index.to_string(), DAY);
        }
        return Ok(Tick::Acted(format!(
            "politics {} : {ok} {why}",
            text(&pick["idea"])
        )));
    }
    // 5. decisions
    let wanted = strings(plan, "decisions");
    if !wanted.is_empty() {
        let offered = tools.call("hoi4_decisions_list", &json!({"lease_token": lease}))?;
        for d in list(&offered, "decisions") {
            let key = text(&d["decision"]);
            if wanted.iter().any(|w| w == key)
                && d["enabled"] == json!(true)
                && !d["active_timer"].as_bool().unwrap_or(false)
                && !state.skipped(now, "dec", key)
            {
                let (ok, why) = state.act(
                    tools,
                    lease,
                    "hoi4_decision_take",
                    json!({"decision": key}),
                    "de",
                )?;
                state.skip(now, "dec", key, if ok { 30 * DAY } else { 3 * DAY });
                return Ok(Tick::Acted(format!("decision {key}: {ok} {why}")));
            }
        }
    }
    // 6. training
    if plan["training"].is_object() {
        let tr = &plan["training"];
        let template = text(&tr["template"]).to_string();
        let templates = tools.call("hoi4_division_templates", &json!({}))?;
        let have = list(&templates, "templates")
            .iter()
            .find(|t| text(&t["template"]) == template)
            .and_then(|t| t["queued_lines"].as_u64());
        let queue = tr["queue"].as_u64().unwrap_or(6);
        if let Some(have) = have {
            if have < queue && !state.skipped(now, "train", &template) {
                let count = tr["batch"].as_u64().unwrap_or(2).min(queue - have);
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
                return Ok(Tick::Acted(format!("train {count} {template}: {ok} {why}")));
            }
        }
    }
    // 6b. military factories on no line go to the first priority line that exists
    let economy = tools.call("hoi4_economy", &json!({}))?;
    let total_military = economy["factory_trigger_count"]["military"]
        .as_i64()
        .unwrap_or(0);
    let lines = tools.call("hoi4_production_lines", &json!({"limit": 128}))?;
    let military: Vec<(i64, String, i64)> = list(&lines, "lines")
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
    let idle = total_military - military.iter().map(|m| m.2).sum::<i64>();
    if idle >= plan["production_min_idle"].as_i64().unwrap_or(2)
        && !state.skipped(now, "prodset", "x")
    {
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
        for want in priority {
            if let Some((index, equipment, factories)) = military
                .iter()
                .find(|m| !m.1.is_empty() && m.1.starts_with(&want))
            {
                let (ok, why) = state.act(
                    tools,
                    lease,
                    "hoi4_production_set",
                    json!({"line": index, "equipment": equipment, "factories": factories + idle}),
                    "ps",
                )?;
                if !ok {
                    state.skip(now, "prodset", "x", 2 * DAY);
                }
                return Ok(Tick::Acted(format!(
                    "production_set {equipment} {factories}+{idle}: {ok} {why}"
                )));
            }
        }
    }
    // 7. production lines added once
    for line in list(plan, "production") {
        let equipment = text(&line["equipment"]).to_string();
        if state.done.contains(&format!("prod:{equipment}"))
            || state.skipped(now, "prod", &equipment)
        {
            continue;
        }
        let (ok, why) = state.act(
            tools,
            lease,
            "hoi4_production_add",
            json!({"equipment": equipment, "factories": line["factories"].as_u64().unwrap_or(5)}),
            "pr",
        )?;
        if ok {
            state.done.insert(format!("prod:{equipment}"));
        } else {
            state.skip(now, "prod", &equipment, 14 * DAY);
        }
        return Ok(Tick::Acted(format!("production {equipment}: {ok} {why}")));
    }
    // 8. construction with idle civilian factories. It comes before the armies
    // (which always find something to retry) so they cannot starve it.
    if let Some(step) = construction_step(state, tools, plan, lease, now, &economy)? {
        return Ok(Tick::Acted(step));
    }
    // 9. every division in an army of sensible size, generals only for real armies
    if plan["armies"].is_object() {
        let cfg = &plan["armies"];
        let max = cfg["max"].as_u64().unwrap_or(24);
        let min_general = cfg["min_for_general"].as_u64().unwrap_or(8);
        let roster = tools.call("hoi4_armies", &json!({}))?;
        for entry in list(&roster, "unassigned_by_state") {
            let Some(id) = entry["state_id"].as_i64() else {
                continue;
            };
            let key = id.to_string();
            if state.skipped(now, "armycreate", &key) {
                continue;
            }
            // "absorb": true|"assign" folds unassigned divisions into the army that
            // stands there (or makes a new one when none does) without deleting it;
            // "assign" never falls back to the merging absorb_existing path.
            let assign = cfg["absorb"] == json!(true) || cfg["absorb"] == json!("assign");
            let assigned = if assign {
                let (ok, why) = state.act(
                    tools,
                    lease,
                    "hoi4_army_assign",
                    json!({"state_id": id}),
                    "as",
                )?;
                // an unmapped control or an older backend: try the older paths
                let unavailable = !ok
                    && cfg["absorb"] == json!(true)
                    && (why.contains("capability_unavailable") || why.contains("unknown"));
                (!unavailable).then_some((ok, why))
            } else {
                None
            };
            if let Some((ok, why)) = assigned {
                state.skip(now, "armycreate", &key, if ok { DAY } else { 2 * DAY });
                return Ok(Tick::Acted(format!("army_assign state {id}: {ok} {why}")));
            }
            let (ok, why) = state.act(
                tools,
                lease,
                "hoi4_army_create",
                json!({"state_id": id}),
                "ac",
            )?;
            let (ok, why) = if !ok
                && why.contains("Divisions of an existing")
                && cfg["absorb"] == json!(true)
            {
                state.act(
                    tools,
                    lease,
                    "hoi4_army_create",
                    json!({"state_id": id, "absorb_existing": true}),
                    "ab",
                )?
            } else {
                (ok, why)
            };
            // Whether or not it worked, give the game time before trying the
            // same state again (a box selection can miss stacked divisions).
            state.skip(now, "armycreate", &key, if ok { DAY } else { 2 * DAY });
            return Ok(Tick::Acted(format!("army_create state {id}: {ok} {why}")));
        }
        for army in list(&roster, "armies") {
            let name = text(&army["army"]);
            let count = army["division_count"].as_u64().unwrap_or(0);
            if count > max && !state.skipped(now, "armysplit", name) {
                let chunk = (count - max).clamp(1, 19);
                let (ok, why) = state.act(
                    tools,
                    lease,
                    "hoi4_army_split",
                    json!({"army": name, "count": chunk}),
                    "sp",
                )?;
                if !ok {
                    state.skip(now, "armysplit", name, 2 * DAY);
                }
                return Ok(Tick::Acted(format!(
                    "army_split {name} {chunk}: {ok} {why}"
                )));
            }
        }
        let generals = strings(cfg, "generals");
        let used: HashSet<String> = list(&roster, "armies")
            .iter()
            .filter_map(|a| a["commander"].as_str().map(String::from))
            .collect();
        let mut spare = generals.iter().filter(|g| !used.contains(*g));
        for army in list(&roster, "armies") {
            let name = text(&army["army"]);
            if army["commander"].is_null()
                && army["division_count"].as_u64().unwrap_or(0) >= min_general
                && !state.skipped(now, "armygeneral", name)
            {
                let Some(general) = spare.next() else { break };
                let (ok, why) = state.act(
                    tools,
                    lease,
                    "hoi4_army_commander",
                    json!({"army": name, "general": general}),
                    "gn",
                )?;
                state.skip(
                    now,
                    "armygeneral",
                    name,
                    if ok { 14 * DAY } else { 2 * DAY },
                );
                return Ok(Tick::Acted(format!(
                    "general {general} -> {name}: {ok} {why}"
                )));
            }
        }
    }
    // 8b. war executive
    if plan["war"].is_object() && plan["war"]["enabled"] != json!(false) {
        if let Some(step) = war_step(state, tools, plan, lease, now)? {
            return Ok(Tick::Acted(step));
        }
    }
    Ok(Tick::Nothing)
}

/// Buildings that share a state's building slots: one state is "full" for all.
const SHARED_SLOT_BUILDINGS: [&str; 7] = [
    "arms_factory",
    "industrial_complex",
    "dockyard",
    "synthetic_refinery",
    "fuel_silo",
    "rocket_site",
    "nuclear_reactor",
];
/// Slot readings older than this are read again before they are trusted.
const SLOT_READING_AGE: GameHour = 3 * DAY;
/// States read with one hoi4_state_slots call per tick.
const PROBE_BATCH: usize = 4;

fn full_key(building: &str, id: i64) -> String {
    if SHARED_SLOT_BUILDINGS.contains(&building) {
        format!("slots:{id}")
    } else {
        format!("{building}:{id}")
    }
}

/// How a refused construction_add is understood.
#[derive(Debug, PartialEq)]
enum Refusal {
    /// no free slot / not allowed / nothing changed: the state is full for a while
    Full,
    /// some of `count` were queued before the rest failed: try one at a time
    Partial,
    /// a pop-up or event blocks input: nothing is wrong with the state
    Blocked,
    /// anything else (missed click, torn read): try this state again soon
    Transient,
}
fn refusal(why: &str) -> Refusal {
    if why.contains("no_free_slot_or_not_allowed") || why.starts_with("NotApplied") {
        Refusal::Full
    } else if why.starts_with("Diverged") {
        Refusal::Partial
    } else if why.contains("ui_blocked") || why.contains("events_open") {
        Refusal::Blocked
    } else {
        Refusal::Transient
    }
}

fn plain(s: &str) -> String {
    s.chars()
        .flat_map(char::to_lowercase)
        .map(|c| match c {
            'ä' => 'a',
            'ö' => 'o',
            'ü' => 'u',
            'ß' => 's',
            'é' | 'è' | 'ê' => 'e',
            'á' | 'à' | 'â' => 'a',
            c => c,
        })
        .collect()
}

/// A listed state as an id: a number, a numeric string, or an exact state name
/// resolved (and remembered) with `hoi4_states`.
fn resolve_state(state: &mut State, tools: &dyn Tools, item: &Value) -> std::result::Result<i64, String> {
    if let Some(id) = item.as_i64() {
        return Ok(id);
    }
    let Some(name) = item.as_str() else {
        return Err(format!("{item}:bad_entry"));
    };
    if let Ok(id) = name.trim().parse::<i64>() {
        return Ok(id);
    }
    if let Some(id) = state.con.names.get(&plain(name)) {
        return Ok(*id);
    }
    let found = tools
        .call("hoi4_states", &json!({"query": name}))
        .map_err(|e| format!("{name}:name_lookup_{}", e.code))?;
    let rows = list(&found, "states");
    let wanted = plain(name);
    let hit = rows
        .iter()
        .find(|r| plain(text(&r["name"])) == wanted)
        .or_else(|| (rows.len() == 1).then(|| &rows[0]))
        .and_then(|r| r["state_id"].as_i64());
    match hit {
        Some(id) => {
            state.con.names.insert(wanted, id);
            Ok(id)
        }
        None => Err(format!("{name}:name_not_found")),
    }
}

/// Own states of the player (for the fallback), read at most every 5 game days.
fn own_states(state: &mut State, tools: &dyn Tools, tag: &str, now: GameHour) -> Vec<i64> {
    if let Some((at, ids)) = &state.con.own {
        if now - *at < 5 * DAY {
            return ids.clone();
        }
    }
    let ids: Vec<i64> = tools
        .call("hoi4_states", &json!({"query": tag}))
        .map(|found| {
            list(&found, "states")
                .iter()
                .filter(|r| {
                    let ok = |k: &str| r[k].as_str().is_none_or(|o| o == tag);
                    ok("owner") && ok("controller")
                })
                .filter_map(|r| r["state_id"].as_i64())
                .collect()
        })
        .unwrap_or_default();
    if !ids.is_empty() {
        state.con.own = Some((now, ids.clone()));
    }
    ids
}

/// One construction decision: at most one building order. Sets the tick's
/// digest line (`state.con.digest`) so the player can see why nothing was queued:
/// `construction: idle=N min=M queued=[..] skipped=[state:reason,..] total=K`.
fn construction_step(
    state: &mut State,
    tools: &dyn Tools,
    plan: &Value,
    lease: &str,
    now: GameHour,
    economy: &Value,
) -> Result<Option<String>> {
    let jobs = list(plan, "construction");
    if jobs.is_empty() {
        return Ok(None);
    }
    let idle = economy["civilian_factories_available_for_projects"]
        .as_i64()
        .unwrap_or(0);
    let min_idle = plan["construction_min_idle"].as_i64().unwrap_or(4);
    let mut skipped: Vec<String> = Vec::new();
    let mut queued: Vec<String> = Vec::new();
    let line = |state: &State, why: &str, queued: &[String], skipped: &[String]| {
        format!(
            "construction: idle={idle} min={min_idle} queued=[{}] skipped=[{}] total={} {why}",
            queued.join(","),
            skipped.join(","),
            state.con.queued_total
        )
    };
    if idle < min_idle {
        state.con.digest = Some(line(state, "(not enough idle civilian factories)", &queued, &skipped));
        return Ok(None);
    }
    if state.con.yield_next {
        state.con.yield_next = false;
        state.con.digest = Some(line(state, "(yielded this tick to armies and war)", &queued, &skipped));
        return Ok(None);
    }
    if now < state.con.pause_until {
        state.con.digest = Some(line(state, "(paused: input was blocked by a pop-up or event)", &queued, &skipped));
        return Ok(None);
    }
    let full_days = plan["construction_full_days"].as_i64().unwrap_or(14).clamp(1, 365);
    let probe = plan["construction_probe_slots"] != json!(false) && now >= state.con.probe_off_until;
    let fallback_all = plan["construction_fallback_any_own_state"] == json!(true);
    let tag = text(&economy["player"]).to_string();

    for (job_index, job) in jobs.iter().enumerate() {
        let building = text(&job["building"]).to_string();
        if building.is_empty() {
            skipped.push(format!("job{job_index}:no_building"));
            continue;
        }
        let want = job["count"].as_i64().unwrap_or(1).clamp(1, 10);
        // listed states first, then (opt-in) every other own state
        let mut ids: Vec<i64> = Vec::new();
        for item in list(job, "states") {
            if state.skipped(now, "conname", &item.to_string()) {
                skipped.push(format!("{}:unresolved", text_or(item)));
                continue;
            }
            match resolve_state(state, tools, item) {
                Ok(id) => {
                    if !ids.contains(&id) {
                        ids.push(id)
                    }
                }
                Err(why) => {
                    state.skip(now, "conname", &item.to_string(), DAY);
                    skipped.push(why);
                }
            }
        }
        let usable = |state: &State, id: i64| {
            !state.skipped(now, "confull", &full_key(&building, id))
                && !state.skipped(now, "con", &format!("{building}:{id}"))
        };
        let mut eligible: Vec<i64> = ids.iter().copied().filter(|id| usable(state, *id)).collect();
        if eligible.is_empty() && (fallback_all || job["fallback_any_own_state"] == json!(true)) && !tag.is_empty() {
            for id in own_states(state, tools, &tag, now) {
                if !ids.contains(&id) && usable(state, id) {
                    eligible.push(id);
                }
            }
        }
        for id in &ids {
            if !eligible.contains(id) {
                let why = if state.skipped(now, "confull", &full_key(&building, *id)) {
                    "full"
                } else {
                    "retry_later"
                };
                skipped.push(format!("{id}:{why}"));
            }
        }
        if eligible.is_empty() {
            continue;
        }
        // take turns between the listed states
        let turn = state.con.rr % eligible.len();
        eligible.rotate_left(turn);
        // read free slots where they are unknown or stale
        if probe && SHARED_SLOT_BUILDINGS.contains(&building.as_str()) {
            let unknown: Vec<i64> = eligible
                .iter()
                .copied()
                .filter(|id| {
                    state
                        .con
                        .free
                        .get(id)
                        .is_none_or(|(_, at)| now - *at >= SLOT_READING_AGE)
                })
                .take(PROBE_BATCH)
                .collect();
            if !unknown.is_empty() {
                match tools.call(
                    "hoi4_state_slots",
                    &json!({"state_ids": unknown, "lease_token": lease}),
                ) {
                    Ok(read) => {
                        for row in list(&read, "states") {
                            let Some(id) = row["state_id"].as_i64() else { continue };
                            match row["free_slots"].as_u64() {
                                Some(free) => {
                                    state.con.free.insert(id, (free as u32, now));
                                    if free == 0 {
                                        state.skip(now, "confull", &full_key(&building, id), full_days * DAY);
                                    }
                                }
                                None => {
                                    // not an own state, or the view never opened
                                    state.skip(now, "con", &format!("{building}:{id}"), 2 * DAY);
                                    skipped.push(format!("{id}:slots_unreadable"));
                                }
                            }
                        }
                    }
                    Err(e) => {
                        if e.code == "action_uncertain" {
                            return Err(e);
                        }
                        // reading slots is optional: add blindly, read again later
                        state.con.probe_off_until = now + DAY;
                        skipped.push(format!("probe:{}", e.code));
                    }
                }
                eligible.retain(|id| usable(state, *id));
            }
            // most free slots first; unknown ones keep their turn order
            eligible.sort_by_key(|id| {
                std::cmp::Reverse(state.con.free.get(id).map_or(1, |(f, _)| (*f).min(99)))
            });
        }
        let Some(&id) = eligible.first() else {
            continue;
        };
        let known_free = state.con.free.get(&id).map(|(f, _)| i64::from(*f));
        let count = want
            .min(state.con.cap.get(&id).copied().unwrap_or(want))
            .min(known_free.unwrap_or(want))
            .max(1);
        let (ok, why) = state.act(
            tools,
            lease,
            "hoi4_construction_add",
            json!({"building": building, "state_id": id, "count": count}),
            "co",
        )?;
        state.con.rr += 1;
        if ok {
            state.con.fails.remove(&id);
            state.con.queued_total += count as u64;
            state.con.last_queue = Some(now);
            state.con.yield_next = true;
            queued.push(format!("{building}@{id}x{count}"));
            if let Some((free, _)) = state.con.free.get_mut(&id) {
                *free = free.saturating_sub(count as u32);
                if *free == 0 {
                    state.skip(now, "confull", &full_key(&building, id), full_days * DAY);
                }
            }
        } else {
            match refusal(&why) {
                Refusal::Full if count > 1 => {
                    // maybe only some slots were free: take them one at a time
                    state.con.cap.insert(id, 1);
                    skipped.push(format!("{id}:one_at_a_time"));
                }
                Refusal::Full => {
                    state.skip(now, "confull", &full_key(&building, id), full_days * DAY);
                    state.con.free.insert(id, (0, now));
                    skipped.push(format!("{id}:full"));
                }
                Refusal::Partial => {
                    state.con.cap.insert(id, 1);
                    state.con.last_queue = Some(now);
                    state.con.free.remove(&id);
                    skipped.push(format!("{id}:partial"));
                }
                Refusal::Blocked => {
                    state.con.pause_until = now + 6;
                    skipped.push(format!("{id}:blocked"));
                }
                Refusal::Transient => {
                    let fails = state.con.fails.entry(id).or_insert(0);
                    *fails += 1;
                    let hours = if *fails >= 3 { 2 * DAY } else { 6 };
                    if *fails >= 3 {
                        *fails = 0;
                    }
                    state.skip(now, "con", &format!("{building}:{id}"), hours);
                    skipped.push(format!("{id}:retry_later"));
                }
            }
        }
        let note = format!("{ok} {why}");
        state.con.digest = Some(line(state, &note, &queued, &skipped));
        return Ok(Some(format!("construct {building} {id}: {ok} {why} x{count}")));
    }
    let why = if jobs.iter().all(|j| list(j, "states").is_empty()) {
        "(no states listed)"
    } else {
        "(every listed state is full, unresolved or waiting to retry)"
    };
    state.con.digest = Some(line(state, why, &queued, &skipped));
    Ok(None)
}
fn text_or(v: &Value) -> String {
    v.as_str().map_or_else(|| v.to_string(), String::from)
}

/// Game hours between two stages of one army's war order.
const WAR_STAGE_PAUSE: GameHour = 2;

/// One war decision: at most one action, or None when nothing needs doing.
fn war_step(
    state: &mut State,
    tools: &dyn Tools,
    plan: &Value,
    lease: &str,
    now: GameHour,
) -> Result<Option<String>> {
    let cfg = &plan["war"];
    let me = match cfg["me"].as_str() {
        Some(t) => t.to_string(),
        None => text(&tools.call("hoi4_overview", &json!({}))?["player"]).to_string(),
    };
    if me.is_empty() {
        return Ok(None);
    }
    let min_army = cfg["min_army"].as_u64().unwrap_or(6);
    let per_target = cfg["per_target"].as_u64().unwrap_or(2) as usize;
    let cooldown: GameHour = cfg["cooldown_hours"].as_i64().unwrap_or(12);
    let reserve = strings(cfg, "reserve");
    // Targets: own states an enemy holds (priority), then enemy-owned states an enemy holds.
    let mine = tools.call("hoi4_states", &json!({"query": me}))?;
    let mut enemies: HashSet<String> = strings(cfg, "enemies").into_iter().collect();
    let mut targets: Vec<(i32, bool)> = Vec::new();
    for row in list(&mine, "states") {
        if let (Some(id), Some(x), Some(z)) = (
            row["state_id"].as_i64(),
            row["x"].as_f64(),
            row["z"].as_f64(),
        ) {
            state.centers.insert(id as i32, (x, z));
            if text(&row["owner"]) == me && text(&row["controller"]) != me {
                enemies.insert(text(&row["controller"]).to_string());
                targets.push((id as i32, true));
            }
        }
    }
    for tag in &enemies {
        let theirs = tools.call("hoi4_states", &json!({"query": tag}))?;
        for row in list(&theirs, "states") {
            let (Some(id), Some(x), Some(z)) = (
                row["state_id"].as_i64(),
                row["x"].as_f64(),
                row["z"].as_f64(),
            ) else {
                continue;
            };
            state.centers.insert(id as i32, (x, z));
            let owner = text(&row["owner"]);
            let controller = text(&row["controller"]);
            if owner == tag && controller == tag && !targets.iter().any(|t| t.0 == id as i32) {
                targets.push((id as i32, false));
            }
        }
    }
    if targets.is_empty() {
        return Ok(None);
    }
    let roster = tools.call("hoi4_armies", &json!({}))?;
    let mut load: HashMap<i32, usize> = HashMap::new();
    for target in state.war_assign.values() {
        *load.entry(*target).or_default() += 1;
    }
    for army in list(&roster, "armies") {
        let name = text(&army["army"]).to_string();
        if army["division_count"].as_u64().unwrap_or(0) < min_army
            || reserve.contains(&name)
            || state.war_cool.get(&name).is_some_and(|until| *until > now)
        {
            continue;
        }
        let here = army["main_state"].as_i64().map(|s| s as i32);
        let position = here.and_then(|s| state.centers.get(&s).copied());
        // Keep the current target while it is still held by an enemy.
        let mut target = state
            .war_assign
            .get(&name)
            .copied()
            .filter(|t| targets.iter().any(|x| x.0 == *t));
        if target.is_none() {
            if let Some(old) = state.war_assign.remove(&name) {
                if let Some(n) = load.get_mut(&old) {
                    *n = n.saturating_sub(1);
                }
            }
            state.war_stage.remove(&name);
            let best = targets
                .iter()
                .filter(|(id, _)| load.get(id).copied().unwrap_or(0) < per_target)
                .filter_map(|(id, liberate)| {
                    let (x, z) = state.centers.get(id).copied()?;
                    let d = position
                        .map_or(0.0, |(px, pz)| ((x - px).powi(2) + (z - pz).powi(2)).sqrt());
                    Some((*id, if *liberate { d * 0.5 } else { d }))
                })
                .min_by(|a, b| a.1.total_cmp(&b.1));
            if let Some((id, _)) = best {
                state.war_assign.insert(name.clone(), id);
                *load.entry(id).or_default() += 1;
                target = Some(id);
            }
        }
        let Some(target) = target else { continue };
        let liberate = targets.iter().any(|t| t.0 == target && t.1);
        let stage = state.war_stage.get(&name).copied().unwrap_or(0);
        let executing = army["plan_executing"] == json!(true);
        let standing_in_target = here == Some(target);
        let (tool, args, label, next_stage) = if liberate {
            if standing_in_target {
                state.war_cool.insert(name.clone(), now + cooldown * 3);
                continue;
            }
            (
                "hoi4_army_move",
                json!({"army": name, "state_id": target}),
                "move",
                0,
            )
        } else if stage == 0 && !executing {
            (
                "hoi4_army_front",
                json!({"army": name, "state_id": target}),
                "front",
                1,
            )
        } else if stage == 1 {
            (
                "hoi4_army_offensive",
                json!({"army": name, "state_id": target}),
                "offensive",
                2,
            )
        } else if stage == 2 {
            (
                "hoi4_army_plan",
                json!({"army": name, "execute": true}),
                "execute",
                3,
            )
        } else {
            state.war_cool.insert(name.clone(), now + cooldown * 3);
            continue;
        };
        let (ok, why) = state.act(tools, lease, tool, args, "wr")?;
        if ok {
            state.war_stage.insert(name.clone(), next_stage);
            state.war_cool.insert(name.clone(), now + WAR_STAGE_PAUSE);
            return Ok(Some(format!("war {label} {name} -> state {target}: ok")));
        }
        // No foothold for a front line yet: step into the state first.
        if label == "front" {
            let (ok, why2) = state.act(
                tools,
                lease,
                "hoi4_army_move",
                json!({"army": name, "state_id": target}),
                "wm",
            )?;
            state.war_cool.insert(name.clone(), now + cooldown);
            return Ok(Some(format!(
                "war move {name} -> state {target}: {ok} {why2} (front: {why})"
            )));
        }
        state.war_cool.insert(name.clone(), now + cooldown);
        return Ok(Some(format!(
            "war {label} {name} -> state {target}: false {why}"
        )));
    }
    Ok(None)
}

/// The single `stop_on_date` pause: `hoi4_time paused:true` under its own request id,
/// skipped when the overview already says paused. The outcome text is final: a refused
/// or uncertain pause is reported and never retried under a new id.
fn date_stop_pause(state: &mut State, tools: &dyn Tools, lease: &str, pause: bool) -> String {
    if !pause {
        return "not requested (stop_on_date_pause false)".into();
    }
    if tools
        .call("hoi4_overview", &json!({}))
        .is_ok_and(|overview| overview["paused"] == json!(true))
    {
        return "already paused".into();
    }
    // The server's own date watcher shares the journal: if it fired or is
    // sending the pause right now, report that instead of a second pause.
    if let Ok(watcher) = tools.call("hoi4_date_stop", &json!({})) {
        let outcome = watcher["outcome"].as_str().unwrap_or("");
        if watcher["fired"] == json!(true) && !outcome.is_empty() {
            return format!("{outcome} (server date watcher)");
        }
        if watcher["in_flight"] == json!(true) {
            return "pause in flight (server date watcher); not re-issued".into();
        }
    }
    match state.act(tools, lease, "hoi4_time", json!({"paused": true}), "sd") {
        Ok((true, _)) => "paused".into(),
        Ok((false, why)) => format!("pause refused ({why}); not retried"),
        Err(e) if e.code == "action_uncertain" => {
            format!("pause uncertain ({}); not retried, settle by hand", e.message)
        }
        Err(e) => format!("pause failed ({}: {}); not retried", e.code, e.message),
    }
}

/// `focus_blocked` digest entries: `key:reason` for every listed focus that is
/// still on its skip timer after a refusal.
fn focus_blocked(state: &State, plan: &Value, now: GameHour) -> Vec<String> {
    let mut keys = strings(plan, "focus_path");
    keys.extend(strings(plan, "focus_fallback"));
    keys.iter()
        .filter(|k| state.skipped(now, "focus", k))
        .filter_map(|k| state.stages.focus_why.get(k).map(|why| format!("{k}:{why}")))
        .collect()
}

/// The current in-game hour from `hoi4_overview`'s `date.raw`. The clock never
/// runs backwards: when the read fails or lacks a date the last known hour is
/// kept, so timers neither expire early nor stall the runner.
pub fn game_hour(state: &mut State, tools: &dyn Tools) -> GameHour {
    if let Some(raw) = tools
        .call("hoi4_overview", &json!({}))
        .ok()
        .and_then(|overview| overview["date"]["raw"].as_i64())
    {
        state.last_hour = state.last_hour.max(raw);
    }
    state.last_hour
}

/// Read `plan.json` (missing means an empty plan).
pub fn read_plan(path: &Path) -> Result<Value> {
    match std::fs::read(path) {
        Ok(bytes) => serde_json::from_slice(&bytes)
            .map_err(|e| Error::new("invalid_plan", format!("plan.json is not valid JSON: {e}"))),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(json!({})),
        Err(e) => Err(e.into()),
    }
}

/// Merge `changes` into the plan file (a null value removes a key) and return it.
pub fn merge_plan(path: &Path, changes: &Map<String, Value>) -> Result<Value> {
    let mut plan = read_plan(path)?;
    let object = plan
        .as_object_mut()
        .ok_or_else(|| Error::new("invalid_plan", "plan.json must hold an object"))?;
    for (key, value) in changes {
        if value.is_null() {
            object.remove(key);
        } else {
            object.insert(key.clone(), value.clone());
        }
    }
    let temporary = path.with_extension("json.tmp");
    std::fs::write(
        &temporary,
        serde_json::to_vec_pretty(&plan).map_err(|e| Error::new("invalid_plan", e.to_string()))?,
    )?;
    std::fs::rename(&temporary, path)?;
    Ok(plan)
}

/// Run the plan for up to `seconds`, returning early when an event needs the
/// model's choice or the runner must stop (peace conference). The plan file is
/// read before every tick, so changes apply at once.
pub fn run(
    state: &mut State,
    tools: &dyn Tools,
    plan_path: &Path,
    lease: &str,
    seconds: u64,
) -> Result<Value> {
    let start = std::time::Instant::now();
    let deadline = start + Duration::from_secs(seconds);
    let mut actions: Vec<String> = Vec::new();
    let mut construction: Option<String> = None;
    let mut research_line: Option<(String, bool)> = None;
    let mut focus_line: Option<(String, bool)> = None;
    let mut stopped_on_date = false;
    state.stages.fallback_used.clear();
    let reason = loop {
        let plan = read_plan(plan_path)?;
        let now = game_hour(state, tools);
        let step = tick(state, tools, &plan, lease, now);
        // the digest survives a failing tick: it says what the stage saw
        match state.con.digest.take() {
            Some(digest) => construction = Some(digest),
            None if construction.is_none() && !list(&plan, "construction").is_empty() => {
                construction = Some(
                    "construction: not reached this tick (an earlier step acted)".into(),
                )
            }
            None => {}
        }
        if let Some(line) = state.stages.research.take() {
            research_line = Some(line);
        }
        if let Some(line) = state.stages.focus.take() {
            focus_line = Some(line);
        }
        let step = step?;
        let pause = match step {
            Tick::Acted(what) => {
                actions.push(what);
                Duration::from_secs(4)
            }
            Tick::Stop(why) => break json!({"stopped": why}),
            Tick::Waiting { event, new } => {
                // The model sees the event once; later runs wait for the
                // plan's patience and then answer it.
                if new {
                    break json!({"event_open": event});
                }
                Duration::from_secs(12)
            }
            Tick::DateStop(why) => {
                stopped_on_date = true;
                break json!(why);
            }
            Tick::Idle => break json!("plan inactive (active:false)"),
            Tick::Nothing => Duration::from_secs(12),
        };
        if std::time::Instant::now() + pause >= deadline {
            break json!("deadline");
        }
        std::thread::sleep(pause);
    };
    let plan = read_plan(plan_path).unwrap_or_else(|_| json!({}));
    let mut alarms = observe_alarms(tools, &plan);
    if let Some(alarm) = construction_alarm(state, tools, &plan, construction.as_deref()) {
        alarms.push(alarm);
    }
    if research_line.is_none() && !strings(&plan, "research_priority").is_empty() {
        research_line = Some((
            "research: not reached this tick (an earlier step acted)".into(),
            false,
        ));
    }
    if focus_line.is_none() && !strings(&plan, "focus_path").is_empty() {
        focus_line = Some((
            "focus: not reached this tick (an earlier step acted)".into(),
            false,
        ));
    }
    let quiet = |code: &str| plan["alarms"][code] == json!(false);
    if research_line.as_ref().is_some_and(|l| l.1) && !quiet("research_slot_idle_plan_exhausted") {
        alarms.push(json!({"code": "research_slot_idle_plan_exhausted",
            "message": research_line.as_ref().map_or("", |l| l.0.as_str()),
            "fix": {"tool": "hoi4_plan", "args_hint": {"op": "set", "plan": {"research_priority": ["<more technology keys>"]}}}}));
    }
    if focus_line.as_ref().is_some_and(|l| l.1) && !quiet("no_active_focus_plan_exhausted") {
        alarms.push(json!({"code": "no_active_focus_plan_exhausted",
            "message": focus_line.as_ref().map_or("", |l| l.0.as_str()),
            "fix": {"tool": "hoi4_plan", "args_hint": {"op": "set", "plan": {"focus_path": ["<more focus keys>"], "focus_fallback": ["<keys>"]}}}}));
    }
    if !state.stages.fallback_used.is_empty() && !quiet("research_fallback_used") {
        alarms.push(json!({"code": "research_fallback_used",
            "message": format!("research_priority had nothing startable; the catalogue fallback picked: {}",
                state.stages.fallback_used.join(", ")),
            "fix": {"tool": "hoi4_plan", "args_hint": {"op": "set", "plan": {"research_priority": ["<wanted keys>"], "research_fallback": "priority_only"}}}}));
    }
    let mut result = json!({"reason": reason, "actions": actions,
        "alarms": alarms,
        "game_hour": state.last_hour,
        "elapsed_seconds": start.elapsed().as_secs_f64()});
    if let Some(digest) = construction {
        result["construction"] = json!(digest);
    }
    if let Some((line, _)) = research_line {
        result["research"] = json!(line);
    }
    if let Some((line, _)) = focus_line {
        result["focus"] = json!(line);
    }
    let blocked = focus_blocked(state, &plan, state.last_hour);
    if !blocked.is_empty() {
        result["focus_blocked"] = json!(blocked);
    }
    if stopped_on_date {
        result["stopped_on_date"] = json!(true);
        result["stop_on_date_pause"] = json!(state.date_stop.as_ref().map_or("", |(_, o)| o.as_str()));
    }
    Ok(result)
}

/// `construction_stalled`: civilian factories sat idle for `alarms.construction_stall_days`
/// (default 5) game days although the plan lists construction and nothing was queued.
fn construction_alarm(
    state: &State,
    tools: &dyn Tools,
    plan: &Value,
    digest: Option<&str>,
) -> Option<Value> {
    if list(plan, "construction").is_empty() || plan["alarms"]["construction_stalled"] == json!(false) {
        return None;
    }
    let days = plan["alarms"]["construction_stall_days"].as_i64().unwrap_or(5);
    let economy = tools.call("hoi4_economy", &json!({})).ok()?;
    let idle = economy["civilian_factories_available_for_projects"].as_i64()?;
    if idle < plan["construction_min_idle"].as_i64().unwrap_or(4) {
        return None;
    }
    let since = state.con.last_queue.or(state.con.baseline)?;
    let waited = state.last_hour - since;
    (waited >= days * DAY).then(|| {
        json!({"code": "construction_stalled",
            "message": format!("{idle} civilian factories idle and nothing queued for {} game days; {}",
                waited / DAY, digest.unwrap_or("construction stage did not run")),
            "fix": {"tool": "hoi4_state_slots", "args_hint": {"state_ids": "<own states>"}}})
    })
}

/// Read what the alarms need (pure reads, never input) and compute them. A
/// failed read just leaves that observation out.
pub fn observe_alarms(tools: &dyn Tools, plan: &Value) -> Vec<Value> {
    let read = |name: &str, args: Value| tools.call(name, &args).ok();
    let economy = read("hoi4_economy", json!({}));
    let research = read("hoi4_research", json!({"limit": 1}));
    let focus = read("hoi4_focus_completed", json!({"limit": 1}));
    let armies = read("hoi4_armies", json!({}));
    let events = read("hoi4_events", json!({}));
    let lines = read("hoi4_production_lines", json!({"limit": 128}));
    let wars = read("hoi4_wars", json!({}));
    let game_hour = read("hoi4_overview", json!({})).and_then(|o| o["date"]["raw"].as_i64());
    crate::alarms::compute(
        plan,
        &crate::alarms::Observations {
            economy: economy.as_ref(),
            research: research.as_ref(),
            focus: focus.as_ref(),
            armies: armies.as_ref(),
            events: events.as_ref(),
            production_lines: lines.as_ref(),
            wars: wars.as_ref(),
            game_hour,
        },
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;

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
            if name.starts_with("hoi4_") && args.get("request_id").is_some() {
                return Ok(json!({"receipt": {"status": "Verified"}}));
            }
            Ok(self.replies.get(name).cloned().unwrap_or_else(|| json!({})))
        }
    }
    fn fake(replies: Vec<(&'static str, Value)>) -> Fake {
        Fake {
            calls: RefCell::new(Vec::new()),
            replies: replies.into_iter().collect(),
        }
    }
    fn names(f: &Fake) -> Vec<String> {
        f.calls.borrow().iter().map(|(n, _)| n.clone()).collect()
    }

    struct ActionReplies<'a> {
        reads: &'a dyn Tools,
        actions: RefCell<Vec<(String, Value)>>,
        replies: RefCell<std::collections::VecDeque<Result<Value>>>,
    }
    impl Tools for ActionReplies<'_> {
        fn call(&self, name: &str, args: &Value) -> Result<Value> {
            if args.get("request_id").is_some() {
                self.actions.borrow_mut().push((name.into(), args.clone()));
                self.replies
                    .borrow_mut()
                    .pop_front()
                    .expect("unexpected extra action")
            } else {
                self.reads.call(name, args)
            }
        }
    }
    fn uncertain() -> Result<Value> {
        Err(Error::new(
            "action_uncertain",
            "Own readback unavailable after input",
        ))
    }

    #[test]
    fn uncertain_research_stops_before_another_priority_or_catalog_fallback() {
        let reads = fake(vec![(
            "hoi4_research",
            json!({"slots": [{"researching": null}]}),
        )]);
        let tools = ActionReplies {
            reads: &reads,
            actions: RefCell::new(vec![]),
            replies: RefCell::new([uncertain(), uncertain()].into()),
        };
        let mut state = State::default();
        let error = tick(
            &mut state,
            &tools,
            &json!({"research_priority": ["radio", "construction1"]}),
            "l",
            START,
        )
        .unwrap_err();
        assert_eq!(error.code, "action_uncertain");
        let calls = tools.actions.borrow();
        assert_eq!(calls.len(), 2);
        assert_eq!(calls[0], calls[1], "Only the same request may be settled");
        assert_eq!(calls[0].1["technology"], "radio");
        assert!(!names(&reads).contains(&"hoi4_technology_list".into()));
        assert!(
            state.bad.is_empty(),
            "Unfinished research must not be skipped"
        );
    }

    #[test]
    fn same_request_settled_as_verified_completes_normally() {
        let reads = fake(vec![]);
        let tools = ActionReplies {
            reads: &reads,
            actions: RefCell::new(vec![]),
            replies: RefCell::new(
                [uncertain(), Ok(json!({"receipt":{"status":"Verified"}}))].into(),
            ),
        };
        let result = State::default()
            .act(
                &tools,
                "l",
                "hoi4_research_start",
                json!({"technology":"radio", "slot":0}),
                "rs",
            )
            .unwrap();
        assert_eq!(result, (true, "Verified".into()));
        let calls = tools.actions.borrow();
        assert_eq!(calls.len(), 2);
        assert_eq!(calls[0], calls[1]);
    }

    #[test]
    fn nonterminal_or_missing_receipts_stop_instead_of_triggering_fallbacks() {
        let reads = fake(vec![]);
        for body in [
            json!({}),
            json!({"receipt":{"status":"Dispatching"}}),
            json!({"receipt":{"status":"Unknown"}}),
        ] {
            let tools = ActionReplies {
                reads: &reads,
                actions: RefCell::new(vec![]),
                replies: RefCell::new([Ok(body)].into()),
            };
            let error = State::default()
                .act(
                    &tools,
                    "l",
                    "hoi4_research_start",
                    json!({"technology":"radio", "slot":0}),
                    "rs",
                )
                .unwrap_err();
            assert_eq!(error.code, "action_uncertain");
            assert_eq!(tools.actions.borrow().len(), 1);
        }
    }

    #[test]
    fn refused_action_diagnostics_truncate_unicode_without_panicking() {
        let reads = fake(vec![]);
        let tools = ActionReplies {
            reads: &reads,
            actions: RefCell::new(vec![]),
            replies: RefCell::new([Err(Error::new("invalid_target", "界".repeat(100)))].into()),
        };
        let (ok, reason) = State::default()
            .act(&tools, "l", "hoi4_army_front", json!({}), "wr")
            .unwrap();
        assert!(!ok);
        assert!(reason.starts_with("invalid_target: 界"));
        assert!(reason.len() <= 160);
    }

    #[test]
    fn starts_the_first_unresearched_priority_in_an_empty_slot() {
        let tools = fake(vec![(
            "hoi4_research",
            json!({"slots": [{"researching": "radio"}, {"researching": null}],
                   "researched": [{"id": "construction1"}]}),
        )]);
        let plan = json!({"research_priority": ["construction1", "radio", "support_weapons"]});
        let mut state = State::default();
        let step = tick(&mut state, &tools, &plan, "lease", START).unwrap();
        assert_eq!(step, Tick::Acted("research support_weapons slot 1".into()));
        let calls = tools.calls.borrow();
        let (name, args) = calls.last().unwrap();
        assert_eq!(name, "hoi4_research_start");
        assert_eq!(args["slot"], json!(1));
        assert_eq!(args["lease_token"], json!("lease"));
        assert!(args["request_id"].as_str().unwrap().starts_with("plan-rs-"));
    }

    #[test]
    fn strict_plan_stops_before_other_candidates_or_domains_after_failure() {
        for reply in [
            Err(Error::new(
                "invalid_target",
                "Technology no longer eligible",
            )),
            Ok(json!({"receipt":{"status":"NotApplied"}})),
            Ok(json!({"receipt":{"status":"Diverged"}})),
        ] {
            let reads = fake(vec![(
                "hoi4_research",
                json!({"slots":[{"researching":null}]}),
            )]);
            let tools = ActionReplies {
                reads: &reads,
                actions: RefCell::new(vec![]),
                replies: RefCell::new([reply].into()),
            };
            let result = tick(
                &mut State::default(),
                &tools,
                &json!({"stop_on_failure":true,"research_priority":["construction1","radio"],"focus_path":["f"]}),
                "lease",
                START,
            );
            assert!(result.is_err());
            assert_eq!(tools.actions.borrow().len(), 1);
            assert!(!names(&reads)
                .iter()
                .any(|n| n == "hoi4_focus_completed" || n == "hoi4_economy"));
        }
    }

    #[test]
    fn invalid_stop_policy_refuses_before_any_observation_or_action() {
        let tools = fake(vec![]);
        assert_eq!(
            tick(
                &mut State::default(),
                &tools,
                &json!({"stop_on_failure":"true"}),
                "lease",
                START
            )
            .unwrap_err()
            .code,
            "invalid_plan"
        );
        assert!(names(&tools).is_empty());
    }

    #[test]
    fn a_choice_event_waits_for_the_model_but_a_planned_option_is_taken_at_once() {
        let events = json!({"events": [{"event": "x.1", "options": ["a", "b"]}]});
        let tools = fake(vec![("hoi4_events", events)]);
        let mut state = State::default();
        let t0 = START;
        assert_eq!(
            tick(&mut state, &tools, &json!({}), "l", t0).unwrap(),
            Tick::Waiting {
                event: "x.1".into(),
                new: true
            }
        );
        let planned = json!({"event_options": {"x.1": 1}});
        let step = tick(&mut state, &tools, &planned, "l", t0).unwrap();
        assert!(matches!(&step, Tick::Acted(s) if s.starts_with("event x.1 option 1")));
        let family = json!({"event_prefix_options": {"x.": "last"}});
        let step = tick(&mut State::default(), &tools, &family, "l", t0).unwrap();
        assert!(matches!(&step, Tick::Acted(s) if s.starts_with("event x.1 option 1")));
    }

    struct Clock(RefCell<Value>);
    impl Tools for Clock {
        fn call(&self, name: &str, _: &Value) -> Result<Value> {
            match name {
                "hoi4_overview" => match &*self.0.borrow() {
                    Value::Null => Err(Error::new("unavailable", "no snapshot")),
                    value => Ok(value.clone()),
                },
                _ => Ok(json!({})),
            }
        }
    }

    #[test]
    fn game_hour_follows_the_overview_date_and_never_runs_backwards() {
        let clock = Clock(RefCell::new(json!({"date": {"raw": 1234}})));
        let mut state = State::default();
        assert_eq!(game_hour(&mut state, &clock), 1234);
        *clock.0.borrow_mut() = json!({"date": {"raw": 1200}});
        assert_eq!(game_hour(&mut state, &clock), 1234);
        *clock.0.borrow_mut() = Value::Null;
        assert_eq!(game_hour(&mut state, &clock), 1234);
        *clock.0.borrow_mut() = json!({"player": "GER"});
        assert_eq!(game_hour(&mut state, &clock), 1234);
        *clock.0.borrow_mut() = json!({"date": {"raw": 1300}});
        assert_eq!(game_hour(&mut state, &clock), 1300);
    }

    #[test]
    fn a_skip_lasts_game_days_and_does_not_depend_on_real_time() {
        let mut state = State::default();
        state.skip(START, "focus", "f", 3 * DAY);
        // many real seconds may pass, but only game time releases the skip
        assert!(state.skipped(START, "focus", "f"));
        assert!(state.skipped(START + 3 * DAY - 1, "focus", "f"));
        assert!(!state.skipped(START + 3 * DAY, "focus", "f"));
        assert!(!state.skipped(START, "focus", "g"));
    }

    #[test]
    fn a_refused_focus_is_retried_after_a_game_day_not_before() {
        struct Refusing;
        impl Tools for Refusing {
            fn call(&self, name: &str, _: &Value) -> Result<Value> {
                match name {
                    "hoi4_research" => Ok(json!({"slots": [{"researching": "a"}]})),
                    "hoi4_focus_completed" => Ok(json!({"active_key": null, "completed": []})),
                    "hoi4_focus_start" => Err(Error::new("invalid_target", "not eligible")),
                    _ => Ok(json!({})),
                }
            }
        }
        let plan = json!({"focus_path": ["F1"]});
        let mut state = State::default();
        let first = tick(&mut state, &Refusing, &plan, "l", START).unwrap();
        assert_eq!(first, Tick::Nothing);
        assert!(state.skipped(START, "focus", "F1"));
        // same game hour or any time within the day: untouched
        assert_eq!(
            tick(&mut state, &Refusing, &plan, "l", START + DAY - 1).unwrap(),
            Tick::Nothing
        );
        // a day later it is tried again
        assert!(!state.skipped(START + DAY, "focus", "F1"));
    }

    #[test]
    fn event_patience_counts_game_days() {
        let events = json!({"events": [{"event": "x.1", "options": ["a", "b"]}]});
        let tools = fake(vec![("hoi4_events", events)]);
        let plan = json!({"auto_events": "all", "wait_events_days": 3});
        let mut state = State::default();
        assert_eq!(
            tick(&mut state, &tools, &plan, "l", START).unwrap(),
            Tick::Waiting {
                event: "x.1".into(),
                new: true
            }
        );
        // paused or slow game: no game time passed, the model keeps its decision
        assert_eq!(
            tick(&mut state, &tools, &plan, "l", START + 3 * DAY).unwrap(),
            Tick::Waiting {
                event: "x.1".into(),
                new: false
            }
        );
        let step = tick(&mut state, &tools, &plan, "l", START + 3 * DAY + 1).unwrap();
        assert!(matches!(&step, Tick::Acted(s) if s.starts_with("event x.1 option 0")));
    }

    #[test]
    fn observe_alarms_reads_only_and_ignores_failed_reads() {
        let tools = fake(vec![
            ("hoi4_focus_completed", json!({"active_key": null})),
            ("hoi4_research", json!({"slots": [{"researching": null}]})),
        ]);
        let alarms = observe_alarms(&tools, &json!({}));
        let codes: Vec<&str> = alarms.iter().map(|a| a["code"].as_str().unwrap()).collect();
        assert_eq!(codes, ["research_slot_empty", "no_active_focus"]);
        assert!(names(&tools)
            .iter()
            .all(|n| !n.contains("_start") && !n.contains("choose")));
    }

    #[test]
    fn news_and_single_option_events_are_answered_at_once() {
        let events = json!({"events": [{"event": "news.5", "options": ["a", "b"]}]});
        let tools = fake(vec![("hoi4_events", events)]);
        let mut state = State::default();
        let step = tick(&mut state, &tools, &json!({}), "l", START).unwrap();
        assert!(matches!(step, Tick::Acted(_)));
    }

    #[test]
    fn an_open_peace_conference_stops_the_runner() {
        let tools = fake(vec![(
            "hoi4_events",
            json!({"peace_conference_open": true}),
        )]);
        let mut state = State::default();
        let step = tick(&mut state, &tools, &json!({}), "l", START).unwrap();
        assert!(matches!(step, Tick::Stop(_)));
    }

    #[test]
    fn an_inactive_plan_touches_nothing() {
        let tools = fake(vec![]);
        let mut state = State::default();
        let step = tick(&mut state, &tools, &json!({"active": false}), "l", START).unwrap();
        assert_eq!(step, Tick::Idle);
        assert!(names(&tools).is_empty());
    }

    #[test]
    fn idle_military_factories_join_the_first_priority_line() {
        let tools = fake(vec![
            ("hoi4_research", json!({"slots": [{"researching": "a"}]})),
            ("hoi4_focus_completed", json!({"active_key": "f"})),
            (
                "hoi4_economy",
                json!({"factory_trigger_count": {"military": 20}, "civilian_factories_available_for_projects": 0}),
            ),
            (
                "hoi4_production_lines",
                json!({"lines": [
                    {"line_kind": "military", "entry_index": 3, "equipment_id": "infantry_equipment_2",
                     "factory_counters": {"requested_factories_raw_signed": 12}},
                    {"line_kind": "military", "entry_index": 4, "equipment_id": "support_equipment_1",
                     "factory_counters": {"requested_factories_raw_signed": 4}}]}),
            ),
        ]);
        let mut state = State::default();
        let step = tick(&mut state, &tools, &json!({}), "l", START).unwrap();
        assert!(
            matches!(&step, Tick::Acted(s) if s.starts_with("production_set infantry_equipment_2 12+4"))
        );
        let calls = tools.calls.borrow();
        assert_eq!(calls.last().unwrap().1["factories"], json!(16));
    }

    #[test]
    fn a_failed_construction_state_is_skipped_for_a_long_time() {
        struct Failing;
        impl Tools for Failing {
            fn call(&self, name: &str, args: &Value) -> Result<Value> {
                match name {
                    "hoi4_research" => Ok(json!({"slots": [{"researching": "a"}]})),
                    "hoi4_focus_completed" => Ok(json!({"active_key": "f"})),
                    "hoi4_economy" => Ok(json!({"factory_trigger_count": {"military": 0},
                        "civilian_factories_available_for_projects": 9})),
                    "hoi4_construction_add" => {
                        assert!(args["request_id"].is_string());
                        Err(Error::new("input_readback_failed", "no free slot"))
                    }
                    _ => Ok(json!({})),
                }
            }
        }
        let plan = json!({"construction": [{"building": "arms_factory", "states": [4, 5]}]});
        let mut state = State::default();
        let now = START;
        let first = tick(&mut state, &Failing, &plan, "l", now).unwrap();
        assert!(
            matches!(&first, Tick::Acted(s) if s.starts_with("construct arms_factory 4: false"))
        );
        let second = tick(&mut state, &Failing, &plan, "l", now).unwrap();
        assert!(matches!(&second, Tick::Acted(s) if s.starts_with("construct arms_factory 5")));
        assert_eq!(
            tick(&mut state, &Failing, &plan, "l", now).unwrap(),
            Tick::Nothing
        );
    }

    /// Idle civilian factories, no other work; construction answers by state.
    struct Builder {
        slots: HashMap<i64, u32>,
        full: Vec<i64>,
        added: RefCell<Vec<(i64, i64)>>,
        probe_fails: bool,
        own: Vec<i64>,
    }
    impl Builder {
        fn new() -> Self {
            Self {
                slots: HashMap::new(),
                full: vec![],
                added: RefCell::new(vec![]),
                probe_fails: false,
                own: vec![],
            }
        }
    }
    impl Tools for Builder {
        fn call(&self, name: &str, args: &Value) -> Result<Value> {
            match name {
                "hoi4_research" => Ok(json!({"slots": [{"researching": "a"}]})),
                "hoi4_focus_completed" => Ok(json!({"active_key": "f"})),
                "hoi4_economy" => Ok(json!({"factory_trigger_count": {"military": 0},
                    "civilian_factories_available_for_projects": 20, "player": "GER"})),
                "hoi4_states" => {
                    let q = args["query"].as_str().unwrap();
                    if q == "GER" {
                        let rows: Vec<Value> = self
                            .own
                            .iter()
                            .map(|id| json!({"state_id": id, "owner": "GER", "controller": "GER"}))
                            .collect();
                        Ok(json!({"states": rows}))
                    } else if q == "Hessen" {
                        Ok(json!({"states": [{"state_id": 60, "name": "Hessen"}]}))
                    } else {
                        Ok(json!({"states": []}))
                    }
                }
                "hoi4_state_slots" => {
                    if self.probe_fails {
                        return Err(Error::new("ui_blocked", "pop-up"));
                    }
                    let ids: Vec<i64> = args["state_ids"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .filter_map(Value::as_i64)
                        .collect();
                    let rows: Vec<Value> = ids
                        .iter()
                        .map(|id| {
                            json!({"state_id": id, "free_slots": self.slots.get(id).copied().unwrap_or(0)})
                        })
                        .collect();
                    Ok(json!({"states": rows}))
                }
                "hoi4_construction_add" => {
                    let id = args["state_id"].as_i64().unwrap();
                    if self.full.contains(&id) {
                        return Err(Error::new("no_free_slot_or_not_allowed", "no slot"));
                    }
                    self.added
                        .borrow_mut()
                        .push((id, args["count"].as_i64().unwrap()));
                    Ok(json!({"receipt": {"status": "Verified"}}))
                }
                _ => Ok(json!({})),
            }
        }
    }
    fn acted(step: Tick) -> String {
        match step {
            Tick::Acted(s) => s,
            other => panic!("expected an action, got {other:?}"),
        }
    }

    #[test]
    fn construction_chooses_states_by_free_slots_and_respects_the_count() {
        let mut b = Builder::new();
        b.slots = [(51, 1), (55, 0), (60, 3)].into();
        let plan = json!({"construction": [{"building": "industrial_complex", "states": [51, 55, 60], "count": 3}]});
        let mut state = State::default();
        let first = acted(tick(&mut state, &b, &plan, "l", START).unwrap());
        assert!(first.starts_with("construct industrial_complex 60: true"), "{first}");
        // the full state was never tried; three slots were free, three asked
        assert_eq!(*b.added.borrow(), vec![(60, 3)]);
        let digest = state.con.digest.clone().unwrap();
        assert!(
            digest.contains("idle=20") && digest.contains("queued=[industrial_complex@60x3]"),
            "{digest}"
        );
        assert!(!digest.contains("55:"), "probing marks 55 full silently: {digest}");
        // the next tick yields to armies and war, the one after builds in state 51 (1 free)
        assert_eq!(tick(&mut state, &b, &plan, "l", START + 1).unwrap(), Tick::Nothing);
        assert!(state.con.digest.clone().unwrap().contains("yielded"));
        let third = acted(tick(&mut state, &b, &plan, "l", START + 2).unwrap());
        assert!(third.starts_with("construct industrial_complex 51"), "{third}");
        assert_eq!(b.added.borrow()[1], (51, 1));
        assert!(state.con.digest.clone().unwrap().contains("55:full"));
    }

    #[test]
    fn construction_names_and_numeric_strings_are_resolved_not_dropped() {
        let mut b = Builder::new();
        b.slots = [(60, 2), (54, 1)].into();
        let plan = json!({"construction": [{"building": "arms_factory", "states": ["Hessen", "54", "Nowhere"]}]});
        let mut state = State::default();
        let step = acted(tick(&mut state, &b, &plan, "l", START).unwrap());
        assert!(step.starts_with("construct arms_factory 60"), "{step}");
        assert!(state
            .con
            .digest
            .clone()
            .unwrap()
            .contains("Nowhere:name_not_found"));
        assert_eq!(state.con.names.get("hessen"), Some(&60));
    }

    #[test]
    fn full_states_are_remembered_for_a_game_time_window_then_retried() {
        let mut b = Builder::new();
        b.full = vec![4];
        let plan = json!({"construction": [{"building": "arms_factory", "states": [4]}],
            "construction_probe_slots": false, "construction_full_days": 10});
        let mut state = State::default();
        let first = acted(tick(&mut state, &b, &plan, "l", START).unwrap());
        assert!(first.contains("construct arms_factory 4: false"), "{first}");
        assert_eq!(
            tick(&mut state, &b, &plan, "l", START + 9 * DAY).unwrap(),
            Tick::Nothing
        );
        let digest = state.con.digest.clone().unwrap();
        assert!(
            digest.contains("skipped=[4:full]") && digest.contains("every listed state is full"),
            "{digest}"
        );
        b.full.clear();
        let again = acted(tick(&mut state, &b, &plan, "l", START + 10 * DAY).unwrap());
        assert!(again.starts_with("construct arms_factory 4: true"), "{again}");
    }

    #[test]
    fn fallback_builds_in_other_own_states_when_the_listed_ones_are_full() {
        let mut b = Builder::new();
        b.slots = [(51, 0), (70, 2)].into();
        b.own = vec![51, 70, 71];
        let plan = json!({"construction": [{"building": "industrial_complex", "states": [51]}]});
        let mut state = State::default();
        // without the opt-in nothing else is touched
        assert_eq!(tick(&mut state, &b, &plan, "l", START).unwrap(), Tick::Nothing);
        assert!(b.added.borrow().is_empty());
        let plan = json!({"construction": [{"building": "industrial_complex", "states": [51]}],
            "construction_fallback_any_own_state": true});
        let step = acted(tick(&mut state, &b, &plan, "l", START).unwrap());
        assert!(step.starts_with("construct industrial_complex 70: true"), "{step}");
    }

    #[test]
    fn construction_waits_below_min_idle_and_names_the_reason() {
        let b = Builder::new();
        let plan = json!({"construction": [{"building": "arms_factory", "states": [4]}], "construction_min_idle": 30});
        let mut state = State::default();
        assert_eq!(tick(&mut state, &b, &plan, "l", START).unwrap(), Tick::Nothing);
        let digest = state.con.digest.clone().unwrap();
        assert!(digest.starts_with("construction: idle=20 min=30"), "{digest}");
        assert!(b.added.borrow().is_empty());
    }

    #[test]
    fn a_failing_slot_read_is_not_fatal_and_a_blocked_add_pauses_every_state() {
        let mut b = Builder::new();
        b.probe_fails = true;
        let plan = json!({"construction": [{"building": "arms_factory", "states": [4, 5]}]});
        let mut state = State::default();
        let step = acted(tick(&mut state, &b, &plan, "l", START).unwrap());
        assert!(step.starts_with("construct arms_factory 4: true"), "{step}");
        assert!(state
            .con
            .digest
            .clone()
            .unwrap()
            .contains("probe:ui_blocked"));
        // an event or pop-up refusing input is not the state's fault
        struct Blocked;
        impl Tools for Blocked {
            fn call(&self, name: &str, _: &Value) -> Result<Value> {
                match name {
                    "hoi4_research" => Ok(json!({"slots": [{"researching": "a"}]})),
                    "hoi4_focus_completed" => Ok(json!({"active_key": "f"})),
                    "hoi4_economy" => {
                        Ok(json!({"civilian_factories_available_for_projects": 9}))
                    }
                    "hoi4_construction_add" => Err(Error::new("ui_blocked", "event open")),
                    _ => Ok(json!({})),
                }
            }
        }
        let plan = json!({"construction": [{"building": "arms_factory", "states": [4, 5]}], "construction_probe_slots": false});
        let mut state = State::default();
        acted(tick(&mut state, &Blocked, &plan, "l", START).unwrap());
        assert_eq!(
            tick(&mut state, &Blocked, &plan, "l", START + 2).unwrap(),
            Tick::Nothing
        );
        assert!(state.con.digest.clone().unwrap().contains("paused"));
        // no state was marked failed or full: after the pause both are tried again
        assert!(!state.skipped(START + 7, "confull", "slots:4"));
        assert!(matches!(
            tick(&mut state, &Blocked, &plan, "l", START + 7).unwrap(),
            Tick::Acted(_)
        ));
    }

    #[test]
    fn construction_runs_before_armies_that_always_have_something_to_retry() {
        struct Both(Builder);
        impl Tools for Both {
            fn call(&self, name: &str, args: &Value) -> Result<Value> {
                if name == "hoi4_armies" {
                    return Ok(
                        json!({"unassigned_by_state": [{"state_id": 56, "divisions": 4}], "armies": []}),
                    );
                }
                if name == "hoi4_army_create" {
                    return Ok(json!({"receipt": {"status": "Verified"}}));
                }
                self.0.call(name, args)
            }
        }
        let mut inner = Builder::new();
        inner.slots = [(4, 5)].into();
        let tools = Both(inner);
        let plan = json!({"construction": [{"building": "arms_factory", "states": [4]}], "armies": {"max": 24}});
        let mut state = State::default();
        let first = acted(tick(&mut state, &tools, &plan, "l", START).unwrap());
        assert!(first.starts_with("construct arms_factory 4"), "{first}");
        let second = acted(tick(&mut state, &tools, &plan, "l", START + 1).unwrap());
        assert!(second.starts_with("army_create state 56"), "{second}");
    }

    #[test]
    fn a_stalled_construction_raises_an_alarm_after_the_configured_days() {
        let b = Builder::new();
        let mut state = State {
            last_hour: START + 6 * DAY,
            ..State::default()
        };
        state.con.baseline = Some(START);
        let plan = json!({"construction": [{"building": "arms_factory", "states": [4]}]});
        let alarm =
            construction_alarm(&state, &b, &plan, Some("construction: idle=20 [4:full]")).unwrap();
        assert_eq!(alarm["code"], json!("construction_stalled"));
        assert!(alarm["message"].as_str().unwrap().contains("4:full"));
        state.con.last_queue = Some(START + 5 * DAY);
        assert!(construction_alarm(&state, &b, &plan, None).is_none());
        let off = json!({"construction": [{"building": "x", "states": [1]}], "alarms": {"construction_stalled": false}});
        state.con.last_queue = None;
        assert!(construction_alarm(&state, &b, &off, None).is_none());
    }

    #[test]
    fn unassigned_divisions_get_an_army_and_big_armies_are_split() {
        struct Roster;
        impl Tools for Roster {
            fn call(&self, name: &str, args: &Value) -> Result<Value> {
                match name {
                    "hoi4_research" => Ok(json!({"slots": [{"researching": "a"}]})),
                    "hoi4_focus_completed" => Ok(json!({"active_key": "f"})),
                    "hoi4_economy" => Ok(json!({"factory_trigger_count": {"military": 0},
                        "civilian_factories_available_for_projects": 0})),
                    "hoi4_armies" => Ok(json!({
                        "unassigned_by_state": [{"state_id": 66, "divisions": 3}],
                        "armies": [{"army": "Army 1", "division_count": 40, "commander": null}]})),
                    "hoi4_army_create" => {
                        if args.get("absorb_existing").is_some() {
                            Ok(json!({"receipt": {"status": "Verified"}}))
                        } else {
                            Err(Error::new(
                                "invalid_target",
                                "Divisions of an existing army stand here",
                            ))
                        }
                    }
                    "hoi4_army_assign" => {
                        Err(Error::new("capability_unavailable", "control not mapped"))
                    }
                    "hoi4_army_split" => {
                        assert_eq!(args["count"], json!(16));
                        Ok(json!({"receipt": {"status": "Verified"}}))
                    }
                    _ => Ok(json!({})),
                }
            }
        }
        let plan = json!({"armies": {"max": 24, "absorb": true}});
        let mut state = State::default();
        let now = START;
        // the assign control is unmapped: the older create/merge path still works
        let first = tick(&mut state, &Roster, &plan, "l", now).unwrap();
        assert!(matches!(&first, Tick::Acted(s) if s.starts_with("army_create state 66: true")));
        // the state is retried later; the oversized army is split next
        state.skip(now, "armycreate", "66", 2 * DAY);
        let second = tick(&mut state, &Roster, &plan, "l", now).unwrap();
        assert!(matches!(&second, Tick::Acted(s) if s.starts_with("army_split Army 1 16: true")));
    }

    #[test]
    fn unassigned_divisions_are_assigned_to_the_standing_army_before_any_merge() {
        struct Assigning(RefCell<Vec<String>>, bool);
        impl Tools for Assigning {
            fn call(&self, name: &str, args: &Value) -> Result<Value> {
                self.0.borrow_mut().push(name.to_string());
                match name {
                    "hoi4_research" => Ok(json!({"slots": [{"researching": "a"}]})),
                    "hoi4_focus_completed" => Ok(json!({"active_key": "f"})),
                    "hoi4_economy" => Ok(json!({"civilian_factories_available_for_projects": 0})),
                    "hoi4_armies" => Ok(json!({
                        "unassigned_by_state": [{"state_id": 56, "divisions": 4}], "armies": []})),
                    "hoi4_army_assign" if self.1 => {
                        assert_eq!(args["state_id"], json!(56));
                        Ok(json!({"receipt": {"status": "Verified"}}))
                    }
                    "hoi4_army_assign" => {
                        Err(Error::new("capability_unavailable", "control not mapped"))
                    }
                    _ => Ok(json!({})),
                }
            }
        }
        let plan = json!({"armies": {"absorb": true}});
        let tools = Assigning(RefCell::new(vec![]), true);
        let step = acted(tick(&mut State::default(), &tools, &plan, "l", START).unwrap());
        assert!(step.starts_with("army_assign state 56: true"), "{step}");
        assert!(!tools.0.borrow().contains(&"hoi4_army_create".to_string()));
        // "assign" never merges: an unmapped control just waits and reports
        let plan = json!({"armies": {"absorb": "assign"}});
        let tools = Assigning(RefCell::new(vec![]), false);
        let mut state = State::default();
        let step = acted(tick(&mut state, &tools, &plan, "l", START).unwrap());
        assert!(step.starts_with("army_assign state 56: false capability_unavailable"), "{step}");
        assert!(!tools.0.borrow().contains(&"hoi4_army_create".to_string()));
        assert!(state.skipped(START + DAY, "armycreate", "56"));
    }

    #[test]
    fn only_real_armies_get_a_general() {
        struct Roster;
        impl Tools for Roster {
            fn call(&self, name: &str, args: &Value) -> Result<Value> {
                match name {
                    "hoi4_research" => Ok(json!({"slots": [{"researching": "a"}]})),
                    "hoi4_focus_completed" => Ok(json!({"active_key": "f"})),
                    "hoi4_economy" => Ok(json!({"factory_trigger_count": {"military": 0},
                        "civilian_factories_available_for_projects": 0})),
                    "hoi4_armies" => Ok(json!({"unassigned_by_state": [], "armies": [
                        {"army": "Army 2", "division_count": 3, "commander": null},
                        {"army": "Army 3", "division_count": 12, "commander": null}]})),
                    "hoi4_army_commander" => {
                        assert_eq!(args["army"], json!("Army 3"));
                        assert_eq!(args["general"], json!("B"));
                        Ok(json!({"receipt": {"status": "Verified"}}))
                    }
                    _ => Ok(json!({})),
                }
            }
        }
        let plan = json!({"armies": {"generals": ["B", "C"]}});
        let mut state = State::default();
        let step = tick(&mut state, &Roster, &plan, "l", START).unwrap();
        assert!(matches!(&step, Tick::Acted(s) if s.starts_with("general B -> Army 3: true")));
    }

    struct War {
        liberate: bool,
        front_ok: bool,
        calls: RefCell<Vec<(String, Value)>>,
    }
    impl Tools for War {
        fn call(&self, name: &str, args: &Value) -> Result<Value> {
            self.calls
                .borrow_mut()
                .push((name.to_string(), args.clone()));
            match name {
                "hoi4_research" => Ok(json!({"slots": [{"researching": "a"}]})),
                "hoi4_focus_completed" => Ok(json!({"active_key": "f"})),
                "hoi4_economy" => Ok(json!({"factory_trigger_count": {"military": 0},
                    "civilian_factories_available_for_projects": 0})),
                "hoi4_overview" => Ok(json!({"player": "GER"})),
                "hoi4_states" => match args["query"].as_str().unwrap() {
                    "GER" => Ok(json!({"states": [
                        {"state_id": 42, "owner": "GER", "controller": "GER", "x": 0.0, "z": 0.0},
                        {"state_id": 5, "owner": "GER", "controller": if self.liberate { "POL" } else { "GER" },
                         "x": 100.0, "z": 0.0}]})),
                    "FRA" => Ok(json!({"states": [
                        {"state_id": 28, "owner": "FRA", "controller": "FRA", "x": if self.liberate { 300.0 } else { 10.0 }, "z": 10.0},
                        {"state_id": 18, "owner": "FRA", "controller": "FRA", "x": 50.0, "z": 50.0}]})),
                    "POL" => Ok(json!({"states": []})),
                    other => panic!("unexpected query {other}"),
                },
                "hoi4_armies" => Ok(json!({"unassigned_by_state": [], "armies": [
                    {"army": "Army 1", "division_count": 10, "main_state": 42,
                     "battle_plan_orders": 0, "plan_executing": false, "commander": "X"}]})),
                "hoi4_army_front" if !self.front_ok => Err(Error::new(
                    "input_readback_failed",
                    "No front line was placed",
                )),
                _ if args.get("request_id").is_some() => {
                    Ok(json!({"receipt": {"status": "Verified"}}))
                }
                _ => Ok(json!({})),
            }
        }
    }
    fn war_plan() -> Value {
        json!({"war": {"enemies": ["FRA"], "cooldown": 40}})
    }
    fn last_action(w: &War) -> (String, Value) {
        w.calls
            .borrow()
            .iter()
            .rev()
            .find(|(n, _)| n.starts_with("hoi4_army_"))
            .cloned()
            .unwrap()
    }

    #[test]
    fn an_army_fronts_the_nearest_enemy_state_then_draws_the_offensive_and_executes() {
        let w = War {
            liberate: false,
            front_ok: true,
            calls: RefCell::new(vec![]),
        };
        let mut state = State::default();
        let now = START;
        let first = tick(&mut state, &w, &war_plan(), "l", now).unwrap();
        assert!(matches!(&first, Tick::Acted(s) if s.starts_with("war front Army 1 -> state 28")));
        let (name, args) = last_action(&w);
        assert_eq!(
            (name.as_str(), &args["state_id"]),
            ("hoi4_army_front", &json!(28))
        );
        let later = now + 3;
        let second = tick(&mut state, &w, &war_plan(), "l", later).unwrap();
        assert!(matches!(&second, Tick::Acted(s) if s.starts_with("war offensive")));
        let third = tick(&mut state, &w, &war_plan(), "l", later + 3).unwrap();
        assert!(matches!(&third, Tick::Acted(s) if s.starts_with("war execute")));
    }

    #[test]
    fn without_a_foothold_the_army_steps_into_the_state() {
        let w = War {
            liberate: false,
            front_ok: false,
            calls: RefCell::new(vec![]),
        };
        let mut state = State::default();
        let step = tick(&mut state, &w, &war_plan(), "l", START).unwrap();
        assert!(
            matches!(&step, Tick::Acted(s) if s.starts_with("war move Army 1 -> state 28: true"))
        );
    }

    #[test]
    fn uncertain_front_stops_before_move_fallback_and_preserves_the_war_stage() {
        let reads = War {
            liberate: false,
            front_ok: true,
            calls: RefCell::new(vec![]),
        };
        let tools = ActionReplies {
            reads: &reads,
            actions: RefCell::new(vec![]),
            replies: RefCell::new([uncertain(), uncertain()].into()),
        };
        let mut state = State::default();
        let error = tick(&mut state, &tools, &war_plan(), "l", START).unwrap_err();
        assert_eq!(error.code, "action_uncertain");
        let calls = tools.actions.borrow();
        assert_eq!(calls.len(), 2);
        assert_eq!(calls[0], calls[1]);
        assert_eq!(calls[0].0, "hoi4_army_front");
        assert!(state.war_stage.is_empty());
        assert!(state.war_cool.is_empty());
    }

    #[test]
    fn own_states_held_by_an_enemy_come_first_and_are_attacked_by_moving_in() {
        let w = War {
            liberate: true,
            front_ok: true,
            calls: RefCell::new(vec![]),
        };
        let mut state = State::default();
        let step = tick(&mut state, &w, &war_plan(), "l", START).unwrap();
        assert!(matches!(&step, Tick::Acted(s) if s.starts_with("war move Army 1 -> state 5")));
    }

    #[test]
    fn merging_the_plan_replaces_keys_and_null_removes_them() {
        let dir = std::env::temp_dir().join(format!("plan-run-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("plan.json");
        let _ = std::fs::remove_file(&path);
        let mut first = Map::new();
        first.insert("a".into(), json!(1));
        first.insert("b".into(), json!(2));
        merge_plan(&path, &first).unwrap();
        let mut second = Map::new();
        second.insert("b".into(), Value::Null);
        second.insert("c".into(), json!([3]));
        let merged = merge_plan(&path, &second).unwrap();
        assert_eq!(merged, json!({"a": 1, "c": [3]}));
        std::fs::remove_dir_all(&dir).unwrap();
    }

    fn plan_file(plan: &Value) -> std::path::PathBuf {
        use std::sync::atomic::{AtomicUsize, Ordering};
        static N: AtomicUsize = AtomicUsize::new(0);
        let path = std::env::temp_dir().join(format!(
            "plan-run-test-{}-{}.json",
            std::process::id(),
            N.fetch_add(1, Ordering::SeqCst)
        ));
        std::fs::write(&path, serde_json::to_vec(plan).unwrap()).unwrap();
        path
    }
    fn target() -> GameHour {
        crate::calendar::encode(1939, 9, 1).unwrap() as GameHour
    }
    fn time_calls(f: &Fake) -> Vec<Value> {
        f.calls
            .borrow()
            .iter()
            .filter(|(n, _)| n == "hoi4_time")
            .map(|(_, a)| a.clone())
            .collect()
    }
    fn date_plan() -> Value {
        json!({"stop_on_date": "1939.09.01", "research_priority": ["radio"], "focus_path": ["F"]})
    }

    #[test]
    fn before_the_stop_date_the_runner_works_normally() {
        let tools = fake(vec![("hoi4_research", json!({"slots": [{"researching": null}]}))]);
        let step = tick(&mut State::default(), &tools, &date_plan(), "l", target() - 1).unwrap();
        assert_eq!(step, Tick::Acted("research radio slot 0".into()));
        assert!(time_calls(&tools).is_empty());
    }

    #[test]
    fn on_the_stop_date_the_game_is_paused_once_and_nothing_else_is_done() {
        let tools = fake(vec![
            ("hoi4_research", json!({"slots": [{"researching": null}]})),
            ("hoi4_events", json!({"events": [{"event": "news.1", "options": ["a"]}]})),
        ]);
        let mut state = State::default();
        let step = tick(&mut state, &tools, &date_plan(), "lease", target()).unwrap();
        assert!(matches!(&step, Tick::DateStop(s) if s.contains("1939.09.01") && s.ends_with("pause: paused")), "{step:?}");
        let pauses = time_calls(&tools);
        assert_eq!(pauses.len(), 1);
        assert_eq!(pauses[0]["paused"], json!(true));
        assert_eq!(pauses[0]["lease_token"], json!("lease"));
        assert!(pauses[0]["request_id"].as_str().unwrap().starts_with("plan-sd-"));
        assert!(names(&tools).iter().all(|n| matches!(n.as_str(), "hoi4_overview" | "hoi4_time" | "hoi4_date_stop")));
        // later ticks (and a later date) never pause again nor act
        for hour in [target(), target() + 5 * DAY] {
            let step = tick(&mut state, &tools, &date_plan(), "lease", hour).unwrap();
            assert!(matches!(step, Tick::DateStop(_)));
        }
        assert_eq!(time_calls(&tools).len(), 1);
        assert!(!names(&tools).contains(&"hoi4_events".to_string()));
    }

    #[test]
    fn a_runner_started_after_the_date_stops_at_once() {
        let tools = fake(vec![]);
        let step = tick(&mut State::default(), &tools, &date_plan(), "l", target() + 40 * DAY).unwrap();
        assert!(matches!(step, Tick::DateStop(_)));
        assert_eq!(time_calls(&tools).len(), 1);
    }

    #[test]
    fn an_already_paused_game_gets_no_pause_action() {
        let tools = fake(vec![("hoi4_overview", json!({"paused": true}))]);
        let step = tick(&mut State::default(), &tools, &date_plan(), "l", target()).unwrap();
        assert!(matches!(&step, Tick::DateStop(s) if s.ends_with("already paused")));
        assert!(time_calls(&tools).is_empty());
    }

    struct WithWatcher<'a>(&'a Fake, Value);
    impl Tools for WithWatcher<'_> {
        fn call(&self, name: &str, args: &Value) -> Result<Value> {
            if name == "hoi4_date_stop" {
                return Ok(self.1.clone());
            }
            self.0.call(name, args)
        }
    }

    #[test]
    fn a_pause_the_server_watcher_fired_or_is_sending_is_not_repeated() {
        for (watcher, expect) in [
            (json!({"fired": true, "outcome": "paused"}), "paused (server date watcher)"),
            (json!({"in_flight": true}), "pause in flight (server date watcher); not re-issued"),
        ] {
            let reads = fake(vec![]);
            let tools = WithWatcher(&reads, watcher);
            let step = tick(&mut State::default(), &tools, &date_plan(), "l", target() + 9 * DAY).unwrap();
            assert!(matches!(&step, Tick::DateStop(s) if s.ends_with(expect)), "{step:?}");
            assert!(time_calls(&reads).is_empty());
        }
    }

    #[test]
    fn stop_on_date_pause_false_only_stops_acting() {
        let tools = fake(vec![]);
        let mut plan = date_plan();
        plan["stop_on_date_pause"] = json!(false);
        let step = tick(&mut State::default(), &tools, &plan, "l", target()).unwrap();
        assert!(matches!(&step, Tick::DateStop(s) if s.contains("not requested")));
        assert!(time_calls(&tools).is_empty());
        assert_eq!(names(&tools), Vec::<String>::new());
    }

    #[test]
    fn a_refused_pause_is_reported_and_never_retried() {
        let reads = fake(vec![]);
        let tools = ActionReplies {
            reads: &reads,
            actions: RefCell::new(vec![]),
            replies: RefCell::new([Err(Error::new("invalid_target", "no window"))].into()),
        };
        let mut state = State::default();
        let step = tick(&mut state, &tools, &date_plan(), "l", target()).unwrap();
        assert!(matches!(&step, Tick::DateStop(s) if s.contains("pause refused") && s.contains("not retried")), "{step:?}");
        for _ in 0..3 {
            assert!(matches!(
                tick(&mut state, &tools, &date_plan(), "l", target() + DAY).unwrap(),
                Tick::DateStop(_)
            ));
        }
        assert_eq!(tools.actions.borrow().len(), 1);
    }

    #[test]
    fn an_uncertain_pause_settles_its_own_id_once_then_reports_without_retrying() {
        let reads = fake(vec![]);
        let tools = ActionReplies {
            reads: &reads,
            actions: RefCell::new(vec![]),
            replies: RefCell::new([uncertain(), uncertain()].into()),
        };
        let mut state = State::default();
        let step = tick(&mut state, &tools, &date_plan(), "l", target()).unwrap();
        assert!(matches!(&step, Tick::DateStop(s) if s.contains("pause uncertain")), "{step:?}");
        assert!(matches!(
            tick(&mut state, &tools, &date_plan(), "l", target() + 1).unwrap(),
            Tick::DateStop(_)
        ));
        let calls = tools.actions.borrow();
        assert_eq!(calls.len(), 2);
        assert_eq!(calls[0], calls[1], "only the same request id may be settled");
    }

    #[test]
    fn invalid_stop_dates_are_refused_before_any_call() {
        for plan in [
            json!({"stop_on_date": "tomorrow"}),
            json!({"stop_on_date": "1939.02.30"}),
            json!({"stop_on_date": "1939.09.01", "stop_on_date_pause": "yes"}),
        ] {
            let tools = fake(vec![]);
            let at = target();
            assert_eq!(tick(&mut State::default(), &tools, &plan, "l", at).unwrap_err().code, "invalid_plan");
            assert!(names(&tools).is_empty());
        }
    }

    #[test]
    fn a_later_stop_date_rearms_the_runner() {
        let tools = fake(vec![]);
        let mut state = State::default();
        tick(&mut state, &tools, &date_plan(), "l", target()).unwrap();
        let later = json!({"stop_on_date": {"year": 1940, "month": 1, "day": 1}});
        let step = tick(&mut state, &tools, &later, "l", target() + DAY).unwrap();
        assert!(!matches!(step, Tick::DateStop(_)));
    }

    #[test]
    fn run_reports_stopped_on_date_and_the_alarm_names_it() {
        let tools = fake(vec![("hoi4_overview", json!({"date": {"raw": target() + 30}}))]);
        let path = plan_file(&date_plan());
        let result = run(&mut State::default(), &tools, &path, "l", 1).unwrap();
        assert_eq!(result["stopped_on_date"], json!(true));
        assert!(result["reason"].as_str().unwrap().contains("stop_on_date 1939.09.01"));
        assert_eq!(result["stop_on_date_pause"], json!("paused"));
        assert!(result["actions"].as_array().unwrap().is_empty());
        let codes: Vec<&str> = result["alarms"]
            .as_array()
            .unwrap()
            .iter()
            .map(|a| a["code"].as_str().unwrap())
            .collect();
        assert!(codes.contains(&"target_date_reached"), "{codes:?}");
        assert_eq!(time_calls(&tools).len(), 1);
        let _ = std::fs::remove_file(path);
    }

    // ----- research and focus never stall

    fn catalogue() -> Value {
        json!({"technologies": ["naval_hull1", "infantry_weapons1", "armor_basic", "radio"]})
    }
    fn idle_research() -> Value {
        json!({"slots": [{"researching": null}], "researched": [{"id": "radio"}]})
    }
    fn started(tools: &Fake) -> Vec<String> {
        tools
            .calls
            .borrow()
            .iter()
            .filter(|(n, _)| n == "hoi4_research_start")
            .map(|(_, a)| a["technology"].as_str().unwrap().to_string())
            .collect()
    }

    #[test]
    fn the_default_fallback_still_takes_the_first_catalogue_tech_and_is_recorded() {
        let tools = fake(vec![("hoi4_research", idle_research()), ("hoi4_technology_list", catalogue())]);
        let mut state = State::default();
        let step = tick(&mut state, &tools, &json!({"research_priority": ["radio"]}), "l", START).unwrap();
        assert_eq!(step, Tick::Acted("research naval_hull1 slot 0 (fallback)".into()));
        assert_eq!(state.stages.fallback_used, ["naval_hull1"]);
        assert!(state.stages.research.as_ref().unwrap().0.contains("catalogue fallback"));
    }

    #[test]
    fn priority_only_or_false_never_touches_the_catalogue_and_flags_the_idle_slot() {
        for mode in [json!("priority_only"), json!(false)] {
            let tools = fake(vec![("hoi4_research", idle_research()), ("hoi4_technology_list", catalogue())]);
            let mut state = State::default();
            let plan = json!({"research_priority": ["radio"], "research_fallback": mode});
            let step = tick(&mut state, &tools, &plan, "l", START).unwrap();
            assert_eq!(step, Tick::Nothing);
            assert!(!names(&tools).contains(&"hoi4_technology_list".to_string()));
            let (line, idle) = state.stages.research.clone().unwrap();
            assert!(idle && line.contains("plan exhausted") && line.contains("fallback off"), "{line}");
        }
    }

    #[test]
    fn fallback_prefixes_and_categories_restrict_the_catalogue_pick() {
        let tools = fake(vec![("hoi4_research", idle_research()), ("hoi4_technology_list", catalogue())]);
        let plan = json!({"research_fallback_prefixes": ["infantry"]});
        tick(&mut State::default(), &tools, &plan, "l", START).unwrap();
        assert_eq!(started(&tools), ["infantry_weapons1"]);
        let tools = fake(vec![("hoi4_research", idle_research()), ("hoi4_technology_list", catalogue())]);
        let plan = json!({"research_fallback_categories": ["armor"]});
        tick(&mut State::default(), &tools, &plan, "l", START).unwrap();
        assert_eq!(started(&tools), ["armor_basic"]);
        // nothing in the catalogue matches: nothing is started and the plan is exhausted
        let tools = fake(vec![("hoi4_research", idle_research()), ("hoi4_technology_list", catalogue())]);
        let mut state = State::default();
        let plan = json!({"research_fallback_prefixes": ["air_"]});
        assert_eq!(tick(&mut state, &tools, &plan, "l", START).unwrap(), Tick::Nothing);
        assert!(started(&tools).is_empty());
        assert!(state.stages.research.unwrap().1);
    }

    #[test]
    fn refused_techs_use_the_configured_skip_windows() {
        let reads = fake(vec![("hoi4_research", idle_research())]);
        let tools = ActionReplies {
            reads: &reads,
            actions: RefCell::new(vec![]),
            replies: RefCell::new([Err(Error::new("invalid_target", "no"))].into()),
        };
        let mut state = State::default();
        let plan = json!({"research_priority": ["a"], "research_fallback": false, "skip_hours": {"tech": 10}});
        tick(&mut state, &tools, &plan, "l", START).unwrap();
        assert!(state.skipped(START + 9, "tech", "a"));
        assert!(!state.skipped(START + 10, "tech", "a"));
    }

    fn focus_reads() -> Fake {
        fake(vec![
            ("hoi4_research", json!({"slots": [{"researching": "x"}]})),
            ("hoi4_focus_completed", json!({"active_key": null, "completed": ["Done"]})),
        ])
    }
    fn blocked() -> Result<Value> {
        Err(Error::new("focus_not_available", "requirements unmet"))
    }

    #[test]
    fn a_blocked_focus_is_skipped_briefly_and_the_next_key_starts_in_the_same_tick() {
        let reads = focus_reads();
        let tools = ActionReplies {
            reads: &reads,
            actions: RefCell::new(vec![]),
            replies: RefCell::new([blocked(), Ok(json!({"receipt": {"status": "Verified"}}))].into()),
        };
        let mut state = State::default();
        let plan = json!({"focus_path": ["Done", "A", "B"]});
        let step = tick(&mut state, &tools, &plan, "l", START).unwrap();
        assert_eq!(step, Tick::Acted("focus B".into()));
        let tried: Vec<String> = tools.actions.borrow().iter().map(|a| a.1["focus"].as_str().unwrap().into()).collect();
        assert_eq!(tried, ["A", "B"]);
        assert!(state.skipped(START + 5, "focus", "A"));
        assert!(!state.skipped(START + 6, "focus", "A"), "default blocked window is 6 game hours, not a day");
        assert_eq!(focus_blocked(&state, &plan, START + 1), ["A:focus_not_available"]);
        assert!(focus_blocked(&state, &plan, START + 6).is_empty());
    }

    #[test]
    fn the_blocked_window_and_the_focus_fallback_list_are_configurable() {
        let reads = focus_reads();
        let tools = ActionReplies {
            reads: &reads,
            actions: RefCell::new(vec![]),
            replies: RefCell::new([blocked(), Ok(json!({"receipt": {"status": "Verified"}}))].into()),
        };
        let mut state = State::default();
        let plan = json!({"focus_path": ["A"], "focus_fallback": ["Z"], "skip_hours": {"focus_blocked": 30}});
        let step = tick(&mut state, &tools, &plan, "l", START).unwrap();
        assert_eq!(step, Tick::Acted("focus Z".into()));
        assert!(state.skipped(START + 29, "focus", "A"));
        assert!(!state.skipped(START + 30, "focus", "A"));
    }

    #[test]
    fn other_focus_refusals_keep_the_day_long_skip() {
        let reads = focus_reads();
        let tools = ActionReplies {
            reads: &reads,
            actions: RefCell::new(vec![]),
            replies: RefCell::new([Err(Error::new("ui_blocked", "popup"))].into()),
        };
        let mut state = State::default();
        tick(&mut state, &tools, &json!({"focus_path": ["A"]}), "l", START).unwrap();
        assert!(state.skipped(START + DAY - 1, "focus", "A"));
        assert!(!state.skipped(START + DAY, "focus", "A"));
    }

    #[test]
    fn an_exhausted_or_fully_blocked_focus_plan_is_flagged_idle() {
        let reads = focus_reads();
        let mut state = State::default();
        let plan = json!({"focus_path": ["Done"], "focus_fallback": ["Done"]});
        assert_eq!(tick(&mut state, &reads, &plan, "l", START).unwrap(), Tick::Nothing);
        let (line, idle) = state.stages.focus.clone().unwrap();
        assert!(idle && line.contains("all completed"), "{line}");
        // everything left is on a skip timer
        let tools = ActionReplies {
            reads: &reads,
            actions: RefCell::new(vec![]),
            replies: RefCell::new([blocked()].into()),
        };
        let mut state = State::default();
        let plan = json!({"focus_path": ["Done", "A"]});
        assert_eq!(tick(&mut state, &tools, &plan, "l", START).unwrap(), Tick::Nothing);
        let (line, idle) = state.stages.focus.clone().unwrap();
        assert!(idle && line.contains("A:focus_not_available"), "{line}");
        // a plan with no focus keys at all says nothing
        let mut state = State::default();
        tick(&mut state, &reads, &json!({}), "l", START).unwrap();
        assert!(state.stages.focus.is_none());
    }

    #[test]
    fn run_digest_has_research_and_focus_lines_and_the_exhaustion_alarms() {
        let tools = fake(vec![
            ("hoi4_research", idle_research()),
            ("hoi4_focus_completed", json!({"active_key": null, "completed": ["Done"]})),
            ("hoi4_overview", json!({"date": {"raw": START}})),
        ]);
        let path = plan_file(&json!({"research_priority": ["radio"], "research_fallback": "priority_only",
            "focus_path": ["Done"]}));
        let result = run(&mut State::default(), &tools, &path, "l", 1).unwrap();
        assert!(result["research"].as_str().unwrap().contains("plan exhausted"));
        assert!(result["focus"].as_str().unwrap().starts_with("focus: none active"));
        let codes: Vec<&str> = result["alarms"].as_array().unwrap().iter().map(|a| a["code"].as_str().unwrap()).collect();
        assert!(codes.contains(&"research_slot_idle_plan_exhausted"), "{codes:?}");
        assert!(codes.contains(&"no_active_focus_plan_exhausted"), "{codes:?}");
        assert!(!codes.contains(&"research_fallback_used"));
        // silenced by the plan's alarms section
        let path2 = plan_file(&json!({"research_priority": ["radio"], "research_fallback": false,
            "focus_path": ["Done"], "alarms": {"research_slot_idle_plan_exhausted": false, "no_active_focus_plan_exhausted": false}}));
        let result = run(&mut State::default(), &tools, &path2, "l", 1).unwrap();
        let codes: Vec<&str> = result["alarms"].as_array().unwrap().iter().map(|a| a["code"].as_str().unwrap()).collect();
        assert!(!codes.iter().any(|c| c.ends_with("plan_exhausted")), "{codes:?}");
        let _ = std::fs::remove_file(path);
        let _ = std::fs::remove_file(path2);
    }

    #[test]
    fn run_reports_the_fallback_pick_and_focus_blocked_entries() {
        let tools = fake(vec![
            ("hoi4_research", idle_research()),
            ("hoi4_technology_list", catalogue()),
            ("hoi4_focus_completed", json!({"active_key": "Busy"})),
        ]);
        let path = plan_file(&json!({"research_priority": ["radio"]}));
        let mut state = State::default();
        let result = run(&mut state, &tools, &path, "l", 1).unwrap();
        let alarm = result["alarms"].as_array().unwrap().iter().find(|a| a["code"] == "research_fallback_used").cloned();
        assert!(alarm.unwrap()["message"].as_str().unwrap().contains("naval_hull1"));
        assert!(result["research"].as_str().unwrap().contains("catalogue fallback"));
        // focus_blocked comes from skip timers still running
        state.skip(START, "focus", "A", 6);
        state.stages.focus_why.insert("A".into(), "focus_not_available".into());
        state.last_hour = START;
        let path2 = plan_file(&json!({"focus_path": ["A"]}));
        let result = run(&mut state, &fake(vec![("hoi4_focus_completed", json!({"active_key": "Busy"}))]), &path2, "l", 1).unwrap();
        assert_eq!(result["focus_blocked"], json!(["A:focus_not_available"]));
        assert_eq!(result["focus"], json!("focus: active Busy"));
        let _ = std::fs::remove_file(path);
        let _ = std::fs::remove_file(path2);
    }

    #[test]
    fn an_inactive_plan_reports_why_in_the_run_result() {
        let path = plan_file(&json!({"active": false}));
        let result = run(&mut State::default(), &fake(vec![]), &path, "l", 1).unwrap();
        assert_eq!(result["reason"], json!("plan inactive (active:false)"));
        let _ = std::fs::remove_file(path);
    }
}
