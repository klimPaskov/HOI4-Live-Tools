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
//!   auto_events            answer news and single-option events with option 0 (default true);
//!                          other events after `wait_events` seconds (default 90) with
//!                          `event_options[event]` (default 0)
//!   research_priority      technology keys; the first not-yet-researched key the game accepts
//!                          (then the catalog in order, unless research_fallback is false)
//!   focus_path             focus keys; the first not completed key the game accepts
//!   politics               [{"category","slot","idea"}] one-shot picks
//!   decisions              decision keys taken whenever enabled
//!   training               {"template","queue","batch","deploy_state"} keep a training queue
//!   production_priority    equipment prefixes that receive military factories on no line
//!   production_min_idle    idle military factories needed before rebalancing (default 2)
//!   production             [{"equipment","factories"}] lines added once
//!   construction           [{"building","states":[..]}] spent while civilian factories are idle
//!   construction_min_idle  idle civilian factories needed first (default 4)
//!   armies                 {"max":24,"min_for_general":8,"generals":["Name",..]} keep every division in an
//!                          army of at most `max`: create armies where divisions stand unassigned (folding in
//!                          standing armies when the game refuses and "absorb" is true), split oversized ones, and give the next
//!                          listed general to an army of at least `min_for_general` divisions
use crate::error::{Error, Result};
use serde_json::{json, Map, Value};
use std::{
    collections::{HashMap, HashSet},
    path::Path,
    time::{Duration, Instant},
};

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
    Nothing,
}

pub struct State {
    boot: u64,
    bad: HashMap<String, Instant>,
    done: HashSet<String>,
    seen: HashMap<String, Instant>,
    seq: u64,
    sent: HashMap<String, (String, Value)>,
}
impl Default for State {
    /// Request ids carry the start time so a restarted server never reuses
    /// an id that an earlier run already bound to a different action.
    fn default() -> Self {
        Self {
            boot: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_or(0, |d| d.as_secs()),
            bad: HashMap::new(),
            done: HashSet::new(),
            seen: HashMap::new(),
            seq: 0,
            sent: HashMap::new(),
        }
    }
}

fn skip_key(kind: &str, key: &str) -> String {
    format!("{kind}:{key}")
}
impl State {
    fn skip(&mut self, now: Instant, kind: &str, key: &str, seconds: u64) {
        self.bad
            .insert(skip_key(kind, key), now + Duration::from_secs(seconds));
    }
    fn skipped(&self, now: Instant, kind: &str, key: &str) -> bool {
        self.bad
            .get(&skip_key(kind, key))
            .is_some_and(|until| *until > now)
    }
    /// One action with a fresh id; an uncertain dispatch is settled by
    /// retrying its own id once. (verified, status or error text)
    fn act(
        &mut self,
        tools: &dyn Tools,
        lease: &str,
        name: &str,
        args: Value,
        tag: &str,
    ) -> (bool, String) {
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
            Err(e) => {
                let mut text = format!("{}: {}", e.code, e.message);
                text.truncate(160);
                (false, text)
            }
            Ok(body) => {
                let status = body["receipt"]["status"].as_str().unwrap_or("").to_string();
                (status == "Verified", status)
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
    now: Instant,
) -> Result<Tick> {
    if plan["active"] == json!(false) {
        return Ok(Tick::Idle);
    }
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
        let waited = now.saturating_duration_since(first).as_secs();
        let patience = plan["wait_events"].as_u64().unwrap_or(90);
        if auto && (news || waited > patience) {
            let option = plan["event_options"][&key].as_u64().unwrap_or(0);
            let (ok, why) = state.act(
                tools,
                lease,
                "hoi4_event_choose",
                json!({"event": key, "option": option}),
                "ev",
            );
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
        let candidates: Vec<String> = strings(plan, "research_priority")
            .into_iter()
            .filter(|t| !done.contains(t) && !busy.contains(t) && !state.skipped(now, "tech", t))
            .take(3)
            .collect();
        for tech in candidates {
            let (ok, _) = state.act(
                tools,
                lease,
                "hoi4_research_start",
                json!({"technology": tech, "slot": empty}),
                "rs",
            );
            if ok {
                return Ok(Tick::Acted(format!("research {tech} slot {empty}")));
            }
            state.skip(now, "tech", &tech, 900);
        }
        // Nothing from the priority list is startable: never leave a slot idle.
        // Walk the technology catalog; the game refuses ineligible ones before
        // any input, so a few attempts per tick are cheap.
        if plan["research_fallback"] != json!(false) {
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
                    );
                    if ok {
                        return Ok(Tick::Acted(format!(
                            "research {tech} slot {empty} (fallback)"
                        )));
                    }
                    state.skip(now, "tech", &tech, 1800);
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
    }
    // 3. focus
    let focus = tools.call("hoi4_focus_completed", &json!({"limit": 100}))?;
    if focus["active_key"].as_str().is_none() {
        let completed = strings(&focus, "completed");
        let candidates: Vec<String> = strings(plan, "focus_path")
            .into_iter()
            .filter(|f| !completed.contains(f) && !state.skipped(now, "focus", f))
            .take(2)
            .collect();
        for key in candidates {
            let (ok, _) = state.act(
                tools,
                lease,
                "hoi4_focus_start",
                json!({"focus": key}),
                "fo",
            );
            if ok {
                return Ok(Tick::Acted(format!("focus {key}")));
            }
            state.skip(now, "focus", &key, 1200);
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
        );
        if ok || why.contains("invalid_target") {
            state.done.insert(key);
        } else {
            state.skip(now, "pol", &index.to_string(), 1200);
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
                );
                state.skip(now, "dec", key, if ok { 3600 } else { 900 });
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
                );
                if !ok {
                    state.skip(now, "train", &template, 600);
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
                );
                if !ok {
                    state.skip(now, "prodset", "x", 600);
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
        );
        if ok {
            state.done.insert(format!("prod:{equipment}"));
        } else {
            state.skip(now, "prod", &equipment, 3600);
        }
        return Ok(Tick::Acted(format!("production {equipment}: {ok} {why}")));
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
            let (ok, why) = state.act(
                tools,
                lease,
                "hoi4_army_create",
                json!({"state_id": id}),
                "ac",
            );
            let (ok, why) = if !ok && why.contains("existing army")
                || why.contains("Divisions of an existing")
            {
                state.act(
                    tools,
                    lease,
                    "hoi4_army_create",
                    json!({"state_id": id, "absorb_existing": true}),
                    "ab",
                )
            } else {
                (ok, why)
            };
            // Whether or not it worked, give the game time before trying the
            // same state again (a box selection can miss stacked divisions).
            state.skip(now, "armycreate", &key, if ok { 120 } else { 300 });
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
                );
                if !ok {
                    state.skip(now, "armysplit", name, 300);
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
                );
                state.skip(now, "armygeneral", name, if ok { 3600 } else { 300 });
                return Ok(Tick::Acted(format!(
                    "general {general} -> {name}: {ok} {why}"
                )));
            }
        }
    }
    // 8. construction with idle civilian factories
    let idle_civilian = economy["civilian_factories_available_for_projects"]
        .as_i64()
        .unwrap_or(0);
    if idle_civilian >= plan["construction_min_idle"].as_i64().unwrap_or(4) {
        for job in list(plan, "construction") {
            let building = text(&job["building"]);
            for state_id in list(job, "states").iter().filter_map(Value::as_i64) {
                let key = format!("{building}:{state_id}");
                if state.skipped(now, "con", &key) {
                    continue;
                }
                let (ok, why) = state.act(
                    tools,
                    lease,
                    "hoi4_construction_add",
                    json!({"building": building, "state_id": state_id, "count": 1}),
                    "co",
                );
                if !ok {
                    // state full or not allowed: retry much later
                    state.skip(now, "con", &key, 2400);
                }
                return Ok(Tick::Acted(format!(
                    "construct {building} {state_id}: {ok} {why}"
                )));
            }
        }
    }
    Ok(Tick::Nothing)
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
    let start = Instant::now();
    let deadline = start + Duration::from_secs(seconds);
    let mut actions: Vec<String> = Vec::new();
    let reason = loop {
        let plan = read_plan(plan_path)?;
        let now = Instant::now();
        let step = tick(state, tools, &plan, lease, now)?;
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
            Tick::Idle => break json!("plan inactive"),
            Tick::Nothing => Duration::from_secs(12),
        };
        if Instant::now() + pause >= deadline {
            break json!("deadline");
        }
        std::thread::sleep(pause);
    };
    Ok(
        json!({"reason": reason, "actions": actions, "elapsed_seconds": start.elapsed().as_secs_f64()}),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;

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

    #[test]
    fn starts_the_first_unresearched_priority_in_an_empty_slot() {
        let tools = fake(vec![(
            "hoi4_research",
            json!({"slots": [{"researching": "radio"}, {"researching": null}],
                   "researched": [{"id": "construction1"}]}),
        )]);
        let plan = json!({"research_priority": ["construction1", "radio", "support_weapons"]});
        let mut state = State::default();
        let step = tick(&mut state, &tools, &plan, "lease", Instant::now()).unwrap();
        assert_eq!(step, Tick::Acted("research support_weapons slot 1".into()));
        let calls = tools.calls.borrow();
        let (name, args) = calls.last().unwrap();
        assert_eq!(name, "hoi4_research_start");
        assert_eq!(args["slot"], json!(1));
        assert_eq!(args["lease_token"], json!("lease"));
        assert!(args["request_id"].as_str().unwrap().starts_with("plan-rs-"));
    }

    #[test]
    fn a_choice_event_waits_for_the_model_then_takes_the_planned_option() {
        let events = json!({"events": [{"event": "x.1", "options": ["a", "b"]}]});
        let tools = fake(vec![("hoi4_events", events)]);
        let mut state = State::default();
        let plan = json!({"wait_events": 90, "event_options": {"x.1": 1}});
        let t0 = Instant::now();
        assert_eq!(
            tick(&mut state, &tools, &plan, "l", t0).unwrap(),
            Tick::Waiting {
                event: "x.1".into(),
                new: true
            }
        );
        let later = t0 + Duration::from_secs(120);
        let step = tick(&mut state, &tools, &plan, "l", later).unwrap();
        assert!(matches!(&step, Tick::Acted(s) if s.starts_with("event x.1 option 1")));
    }

    #[test]
    fn news_and_single_option_events_are_answered_at_once() {
        let events = json!({"events": [{"event": "news.5", "options": ["a", "b"]}]});
        let tools = fake(vec![("hoi4_events", events)]);
        let mut state = State::default();
        let step = tick(&mut state, &tools, &json!({}), "l", Instant::now()).unwrap();
        assert!(matches!(step, Tick::Acted(_)));
    }

    #[test]
    fn an_open_peace_conference_stops_the_runner() {
        let tools = fake(vec![(
            "hoi4_events",
            json!({"peace_conference_open": true}),
        )]);
        let mut state = State::default();
        let step = tick(&mut state, &tools, &json!({}), "l", Instant::now()).unwrap();
        assert!(matches!(step, Tick::Stop(_)));
    }

    #[test]
    fn an_inactive_plan_touches_nothing() {
        let tools = fake(vec![]);
        let mut state = State::default();
        let step = tick(
            &mut state,
            &tools,
            &json!({"active": false}),
            "l",
            Instant::now(),
        )
        .unwrap();
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
        let step = tick(&mut state, &tools, &json!({}), "l", Instant::now()).unwrap();
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
        let now = Instant::now();
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
                    "hoi4_army_split" => {
                        assert_eq!(args["count"], json!(16));
                        Ok(json!({"receipt": {"status": "Verified"}}))
                    }
                    _ => Ok(json!({})),
                }
            }
        }
        let plan = json!({"armies": {"max": 24}});
        let mut state = State::default();
        let now = Instant::now();
        let first = tick(&mut state, &Roster, &plan, "l", now).unwrap();
        assert!(matches!(&first, Tick::Acted(s) if s.starts_with("army_create state 66: true")));
        // the state is retried later; the oversized army is split next
        state.skip(now, "armycreate", "66", 300);
        let second = tick(&mut state, &Roster, &plan, "l", now).unwrap();
        assert!(matches!(&second, Tick::Acted(s) if s.starts_with("army_split Army 1 16: true")));
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
        let step = tick(&mut state, &Roster, &plan, "l", Instant::now()).unwrap();
        assert!(matches!(&step, Tick::Acted(s) if s.starts_with("general B -> Army 3: true")));
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
}
