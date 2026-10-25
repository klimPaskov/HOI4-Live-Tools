//! Digest alarms: a pure function over the already-read observation JSON that
//! says which routine things are wrong right now and the one tool call that
//! fixes each. It reads nothing, pauses nothing and presses nothing; a missing
//! observation produces no alarm for its codes (unknown is not "fine" or
//! "broken"), so callers pass only what they actually read.
//!
//! Thresholds come from the `alarms` object of `plan.json` (all optional):
//!
//! | key                        | default | alarm raised when                                   |
//! |----------------------------|---------|-----------------------------------------------------|
//! | `research_slot_empty`      | true    | any research slot has no technology                 |
//! | `no_active_focus`          | true    | the national focus slot is empty                    |
//! | `idle_civilian_factories`  | 4       | civilian factories free for projects >= N           |
//! | `idle_military_factories`  | 2       | military factories on no production line >= N       |
//! | `political_power`          | 150     | political power > N                                 |
//! | `unassigned_divisions`     | 1       | divisions in no army >= N                           |
//! | `armies_over_max`          | true    | an army holds more than `armies.max` (default 24)   |
//! | `armies_without_general`   | true    | an army of >= `armies.min_for_general` (8) has none |
//! | `open_events`              | true    | any event window is open                            |
//! | `peace_conference_open`    | true    | the peace conference is open                        |
//! | `consumer_goods_ratio`     | 0.35    | `economy.consumer_goods_ratio` (0..1) > N           |
//! | `target_date_reached`     | true    | the plan's `stop_on_date` is reached or passed (needs `game_hour`) |
//! | `own_state_occupied`, `border_uncovered`, `threatened_state_undefended` | true | the `hoi4_wars` alert of that code (a proxy-based stage A read) |
//!
//! A boolean key set to `false`, or a numeric key set to `false`/`null`
//! disables that alarm. Every alarm is `{code, message, fix:{tool, args_hint}}`;
//! `args_hint` names the arguments the caller must still fill in (it never
//! carries a lease token or request id).
use serde_json::{json, Value};

/// The observations an alarm pass may use. Every field is optional.
#[derive(Default, Clone, Copy)]
pub struct Observations<'a> {
    /// `hoi4_economy`
    pub economy: Option<&'a Value>,
    /// `hoi4_research` (`slots[].researching`)
    pub research: Option<&'a Value>,
    /// `hoi4_focus_completed` (`active_key`)
    pub focus: Option<&'a Value>,
    /// `hoi4_armies`
    pub armies: Option<&'a Value>,
    /// `hoi4_events`
    pub events: Option<&'a Value>,
    /// `hoi4_production_lines`; with `economy` it gives the idle military factories
    pub production_lines: Option<&'a Value>,
    /// `hoi4_wars`; its alerts (`own_state_occupied`, `border_uncovered`,
    /// `threatened_state_undefended`) pass through unless disabled by key
    pub wars: Option<&'a Value>,
    /// Current game hour (`hoi4_overview` `date.raw`), for `target_date_reached`
    pub game_hour: Option<i64>,
}

/// The plan's `stop_on_date` as (game hour at 00:00 of that day, "YYYY.MM.DD"):
/// `"1939.09.01"` (`-` or `/` also split) or `{year, month, day}`. `Ok(None)`
/// when the key is missing or null; `invalid_plan` for anything else.
pub fn stop_on_date(plan: &Value) -> crate::error::Result<Option<(i64, String)>> {
    let bad = || {
        crate::error::Error::new(
            "invalid_plan",
            "stop_on_date must be \"YYYY.MM.DD\" or {year,month,day} inside 1936..2000",
        )
    };
    let (year, month, day) = match plan.get("stop_on_date") {
        None | Some(Value::Null) => return Ok(None),
        Some(Value::String(text)) => {
            let parts: Vec<i64> = text
                .split(['.', '-', '/'])
                .map(|p| p.trim().parse::<i64>().map_err(|_| bad()))
                .collect::<Result<_, _>>()?;
            // a trailing hour field ("1939.09.01.12") is accepted and ignored
            if !(3..=4).contains(&parts.len()) {
                return Err(bad());
            }
            (parts[0], parts[1], parts[2])
        }
        Some(Value::Object(object)) => {
            let field = |k: &str| object.get(k).and_then(Value::as_i64).ok_or_else(bad);
            (field("year")?, field("month")?, field("day")?)
        }
        Some(_) => return Err(bad()),
    };
    let month_index = usize::try_from(month).map_err(|_| bad())?;
    let raw = crate::calendar::encode(year, month_index, day).map_err(|_| bad())?;
    Ok(Some((i64::from(raw), format!("{year:04}.{month:02}.{day:02}"))))
}

fn alarm(code: &str, message: String, tool: &str, args_hint: Value) -> Value {
    json!({"code": code, "message": message, "fix": {"tool": tool, "args_hint": args_hint}})
}

/// Threshold `key` of the `alarms` section: `Some(n)` when enabled.
fn threshold(config: &Value, key: &str, default: f64) -> Option<f64> {
    match config.get(key) {
        None => Some(default),
        Some(Value::Null | Value::Bool(false)) => None,
        Some(value) => value.as_f64().or(Some(default)),
    }
}

/// Flag key: enabled unless set to `false`/`null`.
fn enabled(config: &Value, key: &str) -> bool {
    !matches!(config.get(key), Some(Value::Null | Value::Bool(false)))
}

fn items<'a>(value: &'a Value, key: &str) -> &'a [Value] {
    value[key].as_array().map(Vec::as_slice).unwrap_or(&[])
}

/// Military factories that sit on no production line, when both reads exist.
pub fn idle_military_factories(economy: &Value, lines: &Value) -> Option<i64> {
    let total = economy["factory_trigger_count"]["military"].as_i64()?;
    let used: i64 = items(lines, "lines")
        .iter()
        .filter(|l| l["line_kind"] == json!("military"))
        .map(|l| {
            l["factory_counters"]["requested_factories_raw_signed"]
                .as_i64()
                .unwrap_or(0)
        })
        .sum();
    Some((total - used).max(0))
}

/// The alarms for these observations under `plan["alarms"]` (plus the plan's
/// `armies` limits). The order is stable: events and the peace conference first
/// (they block input), then research, focus, economy, armies.
pub fn compute(plan: &Value, seen: &Observations) -> Vec<Value> {
    let config = &plan["alarms"];
    let mut out = Vec::new();
    if let (Some(hour), Ok(Some((target, label)))) = (seen.game_hour, stop_on_date(plan)) {
        if hour >= target && enabled(config, "target_date_reached") {
            out.push(alarm(
                "target_date_reached",
                format!("Game date reached the plan's stop_on_date {label}; the runner has stopped acting"),
                "hoi4_time",
                json!({"paused": true}),
            ));
        }
    }
    if let Some(wars) = seen.wars {
        for a in items(wars, "alerts") {
            let code = a["code"].as_str().unwrap_or("");
            if !code.is_empty() && enabled(config, code) {
                out.push(a.clone());
            }
        }
    }
    if let Some(events) = seen.events {
        if events["peace_conference_open"] == json!(true)
            && enabled(config, "peace_conference_open")
        {
            out.push(alarm(
                "peace_conference_open",
                "The peace conference is open; demands need a decision".into(),
                "hoi4_events",
                json!({}),
            ));
        }
        let open = items(events, "events");
        if let Some(top) = open.last().filter(|_| enabled(config, "open_events")) {
            let id = top["event"].as_str().unwrap_or("");
            let choices = top["options"].as_array().map_or(0, Vec::len);
            out.push(alarm(
                "open_event",
                format!(
                    "{} event window(s) open, top: {id} ({choices} option(s))",
                    open.len()
                ),
                "hoi4_event_choose",
                json!({"event": id, "option": "<index>"}),
            ));
        }
    }
    if let Some(research) = seen.research {
        let empty: Vec<usize> = items(research, "slots")
            .iter()
            .enumerate()
            .filter(|(_, s)| s["researching"].as_str().is_none())
            .map(|(i, _)| i)
            .collect();
        if !empty.is_empty() && enabled(config, "research_slot_empty") {
            out.push(alarm(
                "research_slot_empty",
                format!("{} research slot(s) empty: {empty:?}", empty.len()),
                "hoi4_research_start",
                json!({"technology": "<key>", "slot": empty[0]}),
            ));
        }
    }
    if let Some(focus) = seen.focus {
        if focus["active_key"].as_str().is_none() && enabled(config, "no_active_focus") {
            out.push(alarm(
                "no_active_focus",
                "No national focus is in progress".into(),
                "hoi4_focus_start",
                json!({"focus": "<key>"}),
            ));
        }
    }
    if let Some(economy) = seen.economy {
        let civilian = economy["civilian_factories_available_for_projects"].as_i64();
        if let (Some(idle), Some(n)) = (civilian, threshold(config, "idle_civilian_factories", 4.0))
        {
            if idle as f64 >= n {
                out.push(alarm(
                    "idle_civilian_factories",
                    format!("{idle} civilian factories are free for projects (threshold {n})"),
                    "hoi4_construction_add",
                    json!({"building": "<key>", "state_id": "<id>"}),
                ));
            }
        }
        if let (Some(lines), Some(n)) = (
            seen.production_lines,
            threshold(config, "idle_military_factories", 2.0),
        ) {
            if let Some(idle) = idle_military_factories(economy, lines) {
                if idle as f64 >= n {
                    out.push(alarm(
                        "idle_military_factories",
                        format!("{idle} military factories are on no production line (threshold {n})"),
                        "hoi4_production_set",
                        json!({"line": "<entry_index>", "equipment": "<id>", "factories": "<line factories + idle>"}),
                    ));
                }
            }
        }
        let power = economy["political_power"]["integer_trigger_count"].as_i64();
        if let (Some(power), Some(n)) = (power, threshold(config, "political_power", 150.0)) {
            if power as f64 > n {
                out.push(alarm(
                    "political_power_high",
                    format!("{power} political power unspent (threshold {n})"),
                    "hoi4_politics_choose",
                    json!({"category": "<government|research_production|military_staff>", "slot": "<n>", "idea": "<key>"}),
                ));
            }
        }
        if let (Some(ratio), Some(n)) = (
            economy["consumer_goods_ratio"].as_f64(),
            threshold(config, "consumer_goods_ratio", 0.35),
        ) {
            if ratio > n {
                out.push(alarm(
                    "consumer_goods_high",
                    format!(
                        "Consumer goods take {:.0}% of civilian output (threshold {:.0}%)",
                        ratio * 100.0,
                        n * 100.0
                    ),
                    "hoi4_politics_options",
                    json!({"category": "government", "slot": 1}),
                ));
            }
        }
    }
    if let Some(armies) = seen.armies {
        let unassigned = armies["unassigned"].as_array().map_or_else(
            || {
                items(armies, "unassigned_by_state")
                    .iter()
                    .map(|s| s["divisions"].as_u64().unwrap_or(0))
                    .sum::<u64>()
            },
            |a| a.len() as u64,
        );
        if let Some(n) = threshold(config, "unassigned_divisions", 1.0) {
            if unassigned > 0 && unassigned as f64 >= n {
                let state = items(armies, "unassigned_by_state")
                    .first()
                    .map_or(json!("<id>"), |s| s["state_id"].clone());
                out.push(alarm(
                    "unassigned_divisions",
                    format!("{unassigned} division(s) belong to no army (threshold {n})"),
                    "hoi4_army_create",
                    json!({"state_id": state}),
                ));
            }
        }
        let max = plan["armies"]["max"].as_u64().unwrap_or(24);
        let min_general = plan["armies"]["min_for_general"].as_u64().unwrap_or(8);
        for army in items(armies, "armies") {
            let name = army["army"].as_str().unwrap_or("");
            let count = army["division_count"].as_u64().unwrap_or(0);
            if count > max && enabled(config, "armies_over_max") {
                out.push(alarm(
                    "army_over_max",
                    format!("{name} holds {count} divisions (max {max})"),
                    "hoi4_army_split",
                    json!({"army": name, "count": (count - max).clamp(1, 19)}),
                ));
            }
            if army["commander"].is_null()
                && count >= min_general
                && enabled(config, "armies_without_general")
            {
                out.push(alarm(
                    "army_without_general",
                    format!("{name} ({count} divisions) has no general"),
                    "hoi4_army_commander",
                    json!({"army": name, "general": "<name>"}),
                ));
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn codes(alarms: &[Value]) -> Vec<&str> {
        alarms.iter().map(|a| a["code"].as_str().unwrap()).collect()
    }

    fn calm() -> (Value, Value, Value, Value, Value, Value) {
        (
            json!({"factory_trigger_count": {"military": 10}, "civilian_factories_available_for_projects": 1,
                   "political_power": {"integer_trigger_count": 40}}),
            json!({"slots": [{"researching": "a"}, {"researching": "b"}]}),
            json!({"active_key": "f"}),
            json!({"unassigned": [], "unassigned_by_state": [], "armies": [
                {"army": "Army 1", "division_count": 12, "commander": "X"}]}),
            json!({"events": [], "peace_conference_open": false}),
            json!({"lines": [{"line_kind": "military", "entry_index": 0,
                "factory_counters": {"requested_factories_raw_signed": 10}}]}),
        )
    }

    #[test]
    fn a_healthy_country_raises_nothing() {
        let (e, r, f, a, ev, l) = calm();
        let seen = Observations {
            economy: Some(&e),
            research: Some(&r),
            focus: Some(&f),
            armies: Some(&a),
            events: Some(&ev),
            production_lines: Some(&l),
            wars: None,
            game_hour: None,
        };
        assert!(compute(&json!({}), &seen).is_empty());
    }

    #[test]
    fn missing_observations_raise_nothing() {
        assert!(compute(&json!({}), &Observations::default()).is_empty());
    }

    #[test]
    fn every_default_alarm_fires_with_a_one_call_fix() {
        let economy = json!({"factory_trigger_count": {"military": 5},
            "civilian_factories_available_for_projects": 6,
            "political_power": {"integer_trigger_count": 151}, "consumer_goods_ratio": 0.5});
        let research = json!({"slots": [{"researching": "a"}, {"researching": null}]});
        let focus = json!({"active_key": null});
        let armies = json!({"unassigned": [1, 2, 3], "unassigned_by_state": [{"state_id": 66, "divisions": 3}],
            "armies": [{"army": "Army 1", "division_count": 30, "commander": null}]});
        let events = json!({"events": [{"event": "x.1", "options": ["a", "b"]}], "peace_conference_open": true});
        let lines = json!({"lines": [{"line_kind": "military", "factory_counters": {"requested_factories_raw_signed": 2}},
            {"line_kind": "naval", "factory_counters": {"requested_factories_raw_signed": 9}}]});
        let seen = Observations {
            economy: Some(&economy),
            research: Some(&research),
            focus: Some(&focus),
            armies: Some(&armies),
            events: Some(&events),
            production_lines: Some(&lines),
            wars: None,
            game_hour: None,
        };
        let alarms = compute(&json!({}), &seen);
        assert_eq!(
            codes(&alarms),
            [
                "peace_conference_open",
                "open_event",
                "research_slot_empty",
                "no_active_focus",
                "idle_civilian_factories",
                "idle_military_factories",
                "political_power_high",
                "consumer_goods_high",
                "unassigned_divisions",
                "army_over_max",
                "army_without_general",
            ]
        );
        assert_eq!(alarms[1]["fix"]["args_hint"]["event"], json!("x.1"));
        assert_eq!(alarms[2]["fix"]["tool"], json!("hoi4_research_start"));
        assert_eq!(alarms[2]["fix"]["args_hint"]["slot"], json!(1));
        assert!(alarms[5]["message"]
            .as_str()
            .unwrap()
            .starts_with("3 military"));
        assert_eq!(alarms[8]["fix"]["args_hint"]["state_id"], json!(66));
        assert_eq!(alarms[9]["fix"]["args_hint"]["count"], json!(6));
        assert!(alarms
            .iter()
            .all(|a| a["fix"]["args_hint"].get("lease_token").is_none()));
    }

    #[test]
    fn thresholds_are_inclusive_or_exclusive_as_documented() {
        let economy = json!({"factory_trigger_count": {"military": 0},
            "civilian_factories_available_for_projects": 4,
            "political_power": {"integer_trigger_count": 150}});
        let seen = Observations {
            economy: Some(&economy),
            ..Default::default()
        };
        // idle civilian >= N fires at the threshold; political power > N does not
        assert_eq!(
            codes(&compute(&json!({}), &seen)),
            ["idle_civilian_factories"]
        );
        let plan = json!({"alarms": {"idle_civilian_factories": 5, "political_power": 149}});
        assert_eq!(codes(&compute(&plan, &seen)), ["political_power_high"]);
    }

    #[test]
    fn alarms_can_be_disabled_and_use_the_plans_army_limits() {
        let economy = json!({"civilian_factories_available_for_projects": 9,
            "political_power": {"integer_trigger_count": 999}});
        let research = json!({"slots": [{"researching": null}]});
        let armies = json!({"unassigned": [1], "armies": [
            {"army": "A", "division_count": 10, "commander": null}]});
        let seen = Observations {
            economy: Some(&economy),
            research: Some(&research),
            armies: Some(&armies),
            ..Default::default()
        };
        let plan = json!({
            "armies": {"max": 8, "min_for_general": 20},
            "alarms": {"idle_civilian_factories": false, "political_power": null,
                       "research_slot_empty": false, "unassigned_divisions": 2}
        });
        assert_eq!(codes(&compute(&plan, &seen)), ["army_over_max"]);
    }

    #[test]
    fn unassigned_falls_back_to_the_per_state_counts() {
        let armies = json!({"unassigned_by_state": [
            {"state_id": 3, "divisions": 2}, {"state_id": 4, "divisions": 1}], "armies": []});
        let seen = Observations {
            armies: Some(&armies),
            ..Default::default()
        };
        let plan = json!({"alarms": {"unassigned_divisions": 3}});
        let alarms = compute(&plan, &seen);
        assert_eq!(codes(&alarms), ["unassigned_divisions"]);
        assert!(alarms[0]["message"]
            .as_str()
            .unwrap()
            .starts_with("3 division"));
        let plan = json!({"alarms": {"unassigned_divisions": 4}});
        assert!(compute(&plan, &seen).is_empty());
    }

    #[test]
    fn idle_military_needs_both_reads() {
        let economy = json!({"factory_trigger_count": {"military": 7}});
        let seen = Observations {
            economy: Some(&economy),
            ..Default::default()
        };
        assert!(compute(&json!({}), &seen).is_empty());
        assert_eq!(
            idle_military_factories(&economy, &json!({"lines": []})),
            Some(7)
        );
        assert_eq!(
            idle_military_factories(&json!({}), &json!({"lines": []})),
            None
        );
    }

    #[test]
    fn wars_alerts_pass_through_unless_disabled() {
        let wars = json!({"alerts": [
            {"code": "border_uncovered", "message": "m", "fix": {"tool": "hoi4_army_move", "args_hint": {}}},
            {"code": "own_state_occupied", "message": "o", "fix": {"tool": "hoi4_army_move", "args_hint": {}}}]});
        let seen = Observations {
            wars: Some(&wars),
            ..Default::default()
        };
        assert_eq!(compute(&json!({}), &seen).len(), 2);
        let off = compute(&json!({"alarms": {"border_uncovered": false}}), &seen);
        assert_eq!(off.len(), 1);
        assert_eq!(off[0]["code"], "own_state_occupied");
    }

    #[test]
    fn stop_on_date_parses_strings_and_objects_and_rejects_nonsense() {
        let want = crate::calendar::encode(1939, 9, 1).unwrap() as i64;
        for plan in [
            json!({"stop_on_date": "1939.09.01"}),
            json!({"stop_on_date": "1939-9-1"}),
            json!({"stop_on_date": "1939.09.01.12"}),
            json!({"stop_on_date": {"year": 1939, "month": 9, "day": 1}}),
        ] {
            assert_eq!(stop_on_date(&plan).unwrap(), Some((want, "1939.09.01".into())));
        }
        assert_eq!(stop_on_date(&json!({})).unwrap(), None);
        assert_eq!(stop_on_date(&json!({"stop_on_date": null})).unwrap(), None);
        for bad in [json!("1939.13.01"), json!("soon"), json!(1939), json!("1939.02.29"), json!({"year": 1939})] {
            assert_eq!(stop_on_date(&json!({"stop_on_date": bad})).unwrap_err().code, "invalid_plan");
        }
    }

    #[test]
    fn target_date_reached_fires_on_and_after_the_date_only() {
        let target = crate::calendar::encode(1939, 9, 1).unwrap() as i64;
        let plan = json!({"stop_on_date": "1939.09.01"});
        let at = |hour| {
            compute(&plan, &Observations { game_hour: Some(hour), ..Default::default() })
        };
        assert!(at(target - 1).is_empty());
        assert_eq!(codes(&at(target)), ["target_date_reached"]);
        assert_eq!(at(target + 500)[0]["fix"]["tool"], "hoi4_time");
        let off = json!({"stop_on_date": "1939.09.01", "alarms": {"target_date_reached": false}});
        let seen = Observations { game_hour: Some(target), ..Default::default() };
        assert!(compute(&off, &seen).is_empty());
        assert!(compute(&json!({}), &seen).is_empty());
    }
}
