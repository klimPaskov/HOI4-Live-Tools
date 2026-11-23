use hoi4_live_native::{launch, platform, protocol::Server};
#[cfg(target_os = "macos")]
use hoi4_live_native::resident::{autopilot_step, watch_loop};
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
        Some("--doctor") => {
            match args.next().as_deref() {
                None => {}
                #[cfg(target_os = "macos")]
                Some("--sign") => {
                    println!("{}", hoi4_live_native::doctor::sign_self()?);
                    // Entitlements are read when a process starts, so the
                    // signed binary is checked by a fresh run of itself.
                    let code = std::process::Command::new(std::env::current_exe()?)
                        .arg("--doctor")
                        .status()?
                        .code()
                        .unwrap_or(1);
                    std::process::exit(code);
                }
                Some(_) => return Err("Unexpected extra argument".into()),
            }
            let report = hoi4_live_native::doctor::report();
            print!("{}", report.render());
            std::process::exit(if report.usable() { 0 } else { 1 });
        }
        Some("--supported-builds") => {
            println!("{}", hoi4_live_native::builds::report());
            return Ok(());
        }
        Some("--attach") => {
            let pid: i32 = args
                .next()
                .ok_or("--attach requires the game's process ID")?
                .parse()
                .map_err(|_| "--attach requires a numeric process ID")?;
            let directory = PathBuf::from(args.next().ok_or("--attach requires a run directory")?);
            if args.next().is_some() {
                return Err("Unexpected extra argument".into());
            }
            println!("{}", launch::attach(pid, &directory)?);
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
            eprintln!("hoi4-live-native [--session OWNED_RUN_MANIFEST]\nStdio MCP; observations by default.\n--doctor: will memory access work on this computer, and the one step to take if not (macOS: --doctor --sign signs this binary).\n--supported-builds: the game builds this runtime reads, as JSON.\nmacOS: --attach PID RUN_DIRECTORY writes RUN_DIRECTORY/session.json for a game you started yourself (same user, supported build); then --session RUN_DIRECTORY/session.json --mac-input RUN_DIRECTORY.\nmacOS 15+: --record OWNED_SESSION RECORDING_CONFIG (owned window only; no input).\nLinux: --serve PRIVATE_CONFIG / --connect RUN_DIRECTORY / --close RUN_DIRECTORY.\nOptional: --bind-input RUN_DIRECTORY PRIVATE_INPUT_CONFIG (ordinary clock keys; optional owned ID research preview).\nLinux: --bind-management RUN_DIRECTORY BINDING (full management tools; needs a verified layout for the running build).\nExplicit: --close-window RUN_DIRECTORY PRIVATE_WINDOW_CONFIG (one WM close attempt; no signal fallback).\nWindows/macOS launch/input adapters unverified.");
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
