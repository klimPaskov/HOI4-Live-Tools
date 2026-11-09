use hoi4_live_native::{launch, platform, protocol::Server};
use std::{io, path::PathBuf};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args().skip(1);
    let session = match args.next().as_deref() {
        Some("--record") => {
            let session = PathBuf::from(
                args.next()
                    .ok_or("--record requires an owned session manifest")?,
            );
            let config = PathBuf::from(args.next().ok_or("--record requires a recording config")?);
            if args.next().is_some() {
                return Err("Unexpected extra argument".into());
            }
            println!("{}", hoi4_live_native::recording::run(&session, &config)?);
            return Ok(());
        }
        Some("--list-tools") => {
            println!("{}", hoi4_live_native::protocol::list_tools_offline());
            return Ok(());
        }
        None => None,
        Some("--session") => Some(PathBuf::from(
            args.next().ok_or("--session requires a manifest")?,
        )),
        Some("--bind-management") => {
            let directory = PathBuf::from(
                args.next()
                    .ok_or("--bind-management requires run directory")?,
            );
            let config = PathBuf::from(
                args.next()
                    .ok_or("--bind-management requires private management binding")?,
            );
            if args.next().is_some() {
                return Err("Unexpected extra argument".into());
            }
            launch::bind_management(&directory, &config)?;
            return Ok(());
        }
        Some("--bind-input") => {
            let directory =
                PathBuf::from(args.next().ok_or("--bind-input requires run directory")?);
            let config = PathBuf::from(
                args.next()
                    .ok_or("--bind-input requires private input config")?,
            );
            if args.next().is_some() {
                return Err("Unexpected extra argument".into());
            }
            launch::bind_input(&directory, &config)?;
            return Ok(());
        }
        Some("--close-window") => {
            let directory =
                PathBuf::from(args.next().ok_or("--close-window requires run directory")?);
            let config = PathBuf::from(
                args.next()
                    .ok_or("--close-window requires private window config")?,
            );
            if args.next().is_some() {
                return Err("Unexpected extra argument".into());
            }
            launch::close_window(&directory, &config)?;
            return Ok(());
        }
        Some(mode @ ("--serve" | "--connect" | "--close")) => {
            let path = PathBuf::from(args.next().ok_or("Endpoint mode requires a path")?);
            if args.next().is_some() {
                return Err("Unexpected extra argument".into());
            }
            match mode {
                "--serve" => launch::serve(&path)?,
                "--connect" => launch::connect(&path)?,
                _ => launch::close(&path)?,
            }
            return Ok(());
        }
        Some("--help") => {
            eprintln!("hoi4-live-native [--session OWNED_RUN_MANIFEST]\nStdio MCP; observations by default.\nmacOS 15+: --record OWNED_SESSION RECORDING_CONFIG (owned window only; no input).\nLinux: --serve PRIVATE_CONFIG / --connect RUN_DIRECTORY / --close RUN_DIRECTORY.\nOptional: --bind-input RUN_DIRECTORY PRIVATE_INPUT_CONFIG (ordinary clock keys; optional owned ID research preview).\nLinux: --bind-management RUN_DIRECTORY BINDING (full management tools; needs a verified layout for the running build).\nExplicit: --close-window RUN_DIRECTORY PRIVATE_WINDOW_CONFIG (one WM close attempt; no signal fallback).\nWindows/macOS launch/input adapters unverified.");
            return Ok(());
        }
        Some(_) => return Err("Unknown argument".into()),
    };
    // macOS: optional ordinary clock input bound to the owned session.
    let mac_input = match args.next().as_deref() {
        None => None,
        Some("--mac-input") if cfg!(target_os = "macos") => Some(PathBuf::from(
            args.next().ok_or("--mac-input requires a run directory")?,
        )),
        Some(_) => return Err("Unexpected extra argument".into()),
    };
    if args.next().is_some() {
        return Err("Unexpected extra argument".into());
    }
    let (process, unavailable) = match session {
        Some(p) => match platform::open(&p) {
            Ok(process) => (Some(process), None),
            Err(e) => {
                eprintln!("{e}");
                (None, Some(e))
            }
        },
        None => (None, None),
    };
    #[cfg(target_os = "macos")]
    let process = process.map(std::sync::Arc::<dyn platform::Process + Send + Sync>::from);
    #[cfg(target_os = "macos")]
    let commands = match (&mac_input, &process) {
        (Some(directory), Some(process)) => {
            Some(launch::macos::MacClock::bind(process.clone(), directory)?)
        }
        (Some(_), None) => return Err("--mac-input requires an open owned session".into()),
        _ => None,
    };
    #[cfg(not(target_os = "macos"))]
    let _ = &mac_input;
    #[cfg(target_os = "macos")]
    let process = process.map(|p| Box::new(p) as Box<dyn platform::Process + Send + Sync>);
    let server = Server {
        process,
        unavailable,
    };
    let stdin = io::stdin();
    let stdout = io::stdout();
    #[cfg(target_os = "macos")]
    if let Some(commands) = &commands {
        // Two lanes: tool calls that take no input lease (memory reads) answer at once on
        // their own threads; everything else keeps one ordered worker (the single input owner).
        // HOI4_SERIAL=1 restores the one-lane server for comparison.
        if std::env::var_os("HOI4_SERIAL").is_none() {
            let listed = server.handle_with_commands(
                serde_json::json!({"jsonrpc":"2.0","id":0,"method":"tools/list"}),
                Some(commands),
            );
            let reads = hoi4_live_native::framing::read_tools(listed.as_ref());
            let stop = std::sync::atomic::AtomicBool::new(false);
            return std::thread::scope(|scope| {
                // The change watcher (hoi4_observe / hoi4_wait_change): own-situation reads on
                // their own thread, like the read lane. HOI4_WATCH=0 turns it off.
                if std::env::var("HOI4_WATCH").map_or(true, |v| v != "0") {
                    let (server, stop) = (&server, &stop);
                    scope.spawn(move || watch_loop(server, commands, stop));
                }
                let served = hoi4_live_native::framing::serve_lanes_idle(
                    stdin.lock(),
                    io::stdout(),
                    |m| hoi4_live_native::framing::is_read_call(m, &reads),
                    |message| server.handle_with_commands(message, Some(commands)),
                    4,
                    || autopilot_step(&server, commands),
                );
                stop.store(true, std::sync::atomic::Ordering::SeqCst);
                Ok(served?)
            });
        }
    }
    hoi4_live_native::framing::serve(stdin.lock(), stdout.lock(), |message| {
        #[cfg(target_os = "macos")]
        if let Some(commands) = &commands {
            return server.handle_with_commands(message, Some(commands));
        }
        server.handle(message)
    })?;
    Ok(())
}

/// One read tool through the server, its JSON body (None on error).
#[cfg(target_os = "macos")]
fn read_tool(
    server: &Server,
    commands: &dyn hoi4_live_native::protocol::Commands,
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

/// Sample the own situation into the change feed until `stop`: cheap reads every 300 ms,
/// supply/research/focus/economy about once a second, wars every 2 s.
#[cfg(target_os = "macos")]
fn watch_loop(
    server: &Server,
    commands: &dyn hoi4_live_native::protocol::Commands,
    stop: &std::sync::atomic::AtomicBool,
) {
    use hoi4_live_native::watch::{feed, Compact, Limits, Reads};
    use serde_json::json;
    let mut tick: u64 = 0;
    while !stop.load(std::sync::atomic::Ordering::SeqCst) {
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
        tick += 1;
        std::thread::sleep(std::time::Duration::from_millis(300));
    }
}

/// Step errors that end the executor: its authority or its game is gone, or the plan cannot be
/// used. Everything else is retried with backoff.
#[cfg(target_os = "macos")]
fn autopilot_stops_on(code: &str) -> bool {
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
/// One executor step on the input lane when it is idle (see `hoi4_autopilot`). True when it ran.
#[cfg(target_os = "macos")]
fn autopilot_step(server: &Server, commands: &dyn hoi4_live_native::protocol::Commands) -> bool {
    use hoi4_live_native::{autopilot::autopilot, watch::feed};
    let Some(token) = autopilot().due() else {
        return false;
    };
    autopilot().set_in_step(true);
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
    let succeeded = actions.iter().filter(|a| !a.contains(": false")).count();
    autopilot().ran_with(actions.len(), succeeded, actions.last().cloned());
    true
}

#[cfg(all(test, target_os = "macos"))]
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
