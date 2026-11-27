//! The resident services of a bound tool server: the change watcher (the feed behind
//! `hoi4_observe` / `hoi4_wait_change`) and the executor step behind `hoi4_autopilot`.
//! Platform-neutral: they only call tools through the server. The macOS stdio server runs them
//! beside its two lanes; the Linux supervisor runs them on its own threads once the management
//! tools are bound.
use crate::protocol::Server;

/// One read tool through the server, its JSON body (None on error).
pub fn read_tool(
    server: &Server,
    commands: &dyn crate::protocol::Commands,
    name: &str,
    args: serde_json::Value,
) -> Option<serde_json::Value> {
    let reply = server.handle_with_commands(
        serde_json::json!({"jsonrpc":"2.0","id":0,"method":"tools/call",
            "params":{"name":name,"arguments":args}}),
        Some(commands),
    )?;
    let result = &reply["result"];
    if result["isError"] == serde_json::json!(true) {
        return None;
    }
    serde_json::from_str(result["content"][0]["text"].as_str()?).ok()
}

/// Time between two watcher samples.
pub const WATCH_PERIOD: std::time::Duration = std::time::Duration::from_millis(300);

/// The change watcher's sampling state: one `sample` per period.
#[derive(Default)]
pub struct Watcher {
    tick: u64,
}
impl Watcher {
    /// One sample of the own situation into the change feed: cheap reads every sample,
    /// supply/research/focus/economy every third (about once a second), wars every seventh.
    pub fn sample(&mut self, server: &Server, commands: &dyn crate::protocol::Commands) {
        use crate::watch::{feed, Compact, Limits, Reads};
        use serde_json::json;
        let tick = self.tick;
        let read = |name: &str, args: serde_json::Value| read_tool(server, commands, name, args);
        // Army threats and own-province control every 10th sample (about 3 s): they read the
        // provinces around every army.
        let deep = tick % 10 == 0;
        let overview = read("hoi4_overview", json!({}));
        let tag = overview
            .as_ref()
            .and_then(|o| o["player"].as_str().map(str::to_string));
        let mut r = Reads {
            events: read("hoi4_events", json!({})),
            armies: read("hoi4_armies", json!({"threat": deep})),
            alerts: read("hoi4_alerts", json!({})),
            states: if deep {
                tag.and_then(|t| read("hoi4_states", json!({"query": t})))
            } else {
                None
            },
            overview,
            ..Reads::default()
        };
        if tick % 3 == 0 {
            r.supply = read("hoi4_supply", json!({}));
            r.research = read("hoi4_research", json!({"limit": 1}));
            r.focus = read("hoi4_focus_completed", json!({"limit": 1}));
            r.economy = read("hoi4_economy", json!({}));
        }
        if tick % 7 == 0 {
            r.wars = read("hoi4_wars", json!({}));
        }
        feed().push(Compact::from_reads(&r), Limits::default());
        self.tick += 1;
    }
}

/// Sample the own situation into the change feed until `stop` (see [`Watcher::sample`]).
pub fn watch_loop(
    server: &Server,
    commands: &dyn crate::protocol::Commands,
    stop: &std::sync::atomic::AtomicBool,
) {
    let mut watcher = Watcher::default();
    while !stop.load(std::sync::atomic::Ordering::SeqCst) {
        watcher.sample(server, commands);
        std::thread::sleep(WATCH_PERIOD);
    }
}

/// Step errors that end the executor: its authority or its game is gone, or the plan cannot be
/// used. Everything else is retried with backoff.
pub fn autopilot_stops_on(code: &str) -> bool {
    code.contains("lease")
        || matches!(
            code,
            "invalid_plan"
                | "invalid_plan_state"
                | "plan_conflict"
                | "ownership_changed"
                | "owner_changed"
                | "process_absent"
                | "process_lost"
                | "capability_unavailable"
                | ""
        )
}
/// An executor step that lasts this long is logged with its timing.
pub const SLOW_STEP: std::time::Duration = std::time::Duration::from_secs(5);
/// One executor step on the input lane when it is idle (see `hoi4_autopilot`). True when it ran.
pub fn autopilot_step(server: &Server, commands: &dyn crate::protocol::Commands) -> bool {
    use crate::{autopilot::autopilot, watch::feed};
    // a quiet executor looks again on the next game hour (the memory clock, no input)
    let Some(token) = autopilot().due_at(|| {
        read_tool(server, commands, "hoi4_overview", serde_json::json!({}))
            .and_then(|o| o["date"]["raw"].as_i64())
    }) else {
        return false;
    };
    autopilot().set_in_step(true);
    let began = std::time::Instant::now();
    let reply = server.handle_with_commands(
        serde_json::json!({"jsonrpc":"2.0","id":0,"method":"tools/call",
            "params":{"name":"hoi4_plan_run","arguments":{"seconds":1,"lease_token":token}}}),
        Some(commands),
    );
    autopilot().set_in_step(false);
    let result = reply
        .as_ref()
        .map(|r| r["result"].clone())
        .unwrap_or_default();
    let body: serde_json::Value = result["content"][0]["text"]
        .as_str()
        .and_then(|t| serde_json::from_str(t).ok())
        .unwrap_or_default();
    // A step holds the input lane: one that took long is written down with where the time
    // went (the reads and panel walks of a step have no action line of their own).
    let lasted = began.elapsed();
    if lasted >= SLOW_STEP {
        eprintln!(
            "[step] {}",
            serde_json::json!({"unix_ms": std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|d| d.as_millis() as u64)
                    .unwrap_or(0),
                "ms": lasted.as_millis() as u64,
                "reason": body["reason"],
                "actions": body["actions"],
                "timing_ms": body["timing_ms"],
                "error": body["error"]["code"]})
        );
    }
    // Why each army waits or goes (the war rules' status lines) is written down when it changes:
    // the action lines say what was ordered, not what held an order back.
    if body["war"].is_object() {
        static LAST_WAR: std::sync::Mutex<(i64, String)> =
            std::sync::Mutex::new((i64::MIN, String::new()));
        let now = body["war"].to_string();
        // at most one line a game day: the lines carry numbers that move every step
        let day = body["game_hour"].as_i64().unwrap_or(0) / 24;
        if let Ok(mut last) = LAST_WAR.lock() {
            if last.1 != now && last.0 != day {
                eprintln!(
                    "[war] {}",
                    serde_json::json!({"game_hour": body["game_hour"], "war": body["war"]})
                );
                *last = (day, now);
            }
        }
    }
    // The operations' notes (width, planning, axis, opening) likewise, once a game day at most:
    // what the live reader checks on days 1, 5, 10 and 20 of a war
    if body["operations"]["notes"].is_object() {
        static LAST_OPS: std::sync::Mutex<(i64, String)> =
            std::sync::Mutex::new((i64::MIN, String::new()));
        let now = body["operations"]["notes"].to_string();
        let day = body["game_hour"].as_i64().unwrap_or(0) / 24;
        if let Ok(mut last) = LAST_OPS.lock() {
            if last.1 != now && last.0 != day {
                eprintln!(
                    "[ops] {}",
                    serde_json::json!({"game_hour": body["game_hour"],
                        "notes": body["operations"]["notes"]})
                );
                *last = (day, now);
            }
        }
    }
    if result["isError"] == serde_json::json!(true) {
        let code = body["error"]["code"].as_str().unwrap_or("");
        if autopilot_stops_on(code) {
            // A refused step (lease gone, another game, plan unreadable) stops the executor
            // visibly.
            autopilot().disable();
            feed().note(
                "autopilot_stopped",
                true,
                serde_json::json!({"reason": body}),
            );
        } else {
            // A transient failure (a torn read on a busy game day stopped the executor for good,
            // live 8 Nov 2026) counts as a failed step: the executor backs off and tries again.
            feed().note(
                "autopilot_step_failed",
                false,
                serde_json::json!({"reason": body}),
            );
            autopilot().ran_with(1, 0, Some(format!("step: false {code}")));
        }
        return true;
    }
    let actions: Vec<String> = body["actions"]
        .as_array()
        .map(|a| {
            a.iter()
                .filter_map(|x| x.as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default();
    for a in &actions {
        feed().note("autopilot_action", false, serde_json::json!({"action": a}));
    }
    // What the executor cannot do itself goes to the player as an urgent change: a wake is
    // raised once per reason, so each arrives once.
    for alarm in body["alarms"].as_array().into_iter().flatten() {
        if alarm["code"] == serde_json::json!("wake_player") {
            // A wake about a standing game alert repeats what the alert changes already say;
            // it is listed but does not interrupt the player. The others are single items the
            // executor hands over.
            let standing = alarm["reason"]
                .as_str()
                .is_some_and(|r| r.starts_with("alert:"));
            feed().note("wake_player", !standing, alarm.clone());
        }
    }
    let succeeded = actions.iter().filter(|a| !a.contains(": false")).count();
    autopilot().ran_at(
        actions.len(),
        succeeded,
        actions.last().cloned(),
        body["game_hour"].as_i64(),
    );
    true
}

#[cfg(test)]
mod autopilot_stop_tests {
    #[test]
    fn transient_errors_back_off_and_lost_authority_stops() {
        for code in [
            "torn_read",
            "short_read",
            "input_readback_failed",
            "events_open",
            "plan_busy",
        ] {
            assert!(!super::autopilot_stops_on(code), "{code}");
        }
        for code in [
            "input_lease_required",
            "invalid_plan",
            "ownership_changed",
            "process_lost",
            "",
        ] {
            assert!(super::autopilot_stops_on(code), "{code}");
        }
    }
}
