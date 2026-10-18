use hoi4_live_native::{launch, platform, protocol::Server};
use std::{io, path::PathBuf};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args().skip(1);
    let session = match args.next().as_deref() {
        None => None,
        Some("--session") => Some(PathBuf::from(
            args.next().ok_or("--session requires a manifest")?,
        )),
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
            eprintln!("hoi4-live-native [--session OWNED_RUN_MANIFEST]\nStdio MCP; observations by default.\nLinux: --serve PRIVATE_CONFIG / --connect RUN_DIRECTORY / --close RUN_DIRECTORY.\nOptional: --bind-input RUN_DIRECTORY PRIVATE_INPUT_CONFIG (ordinary clock keys; optional owned ID research preview).\nExplicit: --close-window RUN_DIRECTORY PRIVATE_WINDOW_CONFIG (one WM close attempt; no signal fallback).\nWindows/macOS launch/input adapters unverified.");
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
    let server = Server {
        process,
        unavailable,
    };
    #[cfg(target_os = "macos")]
    let commands = match (&mac_input, &server.process) {
        (Some(directory), Some(process)) => {
            Some(launch::macos::MacClock::bind(process.as_ref(), directory)?)
        }
        (Some(_), None) => return Err("--mac-input requires an open owned session".into()),
        _ => None,
    };
    #[cfg(not(target_os = "macos"))]
    let _ = &mac_input;
    let stdin = io::stdin();
    let stdout = io::stdout();
    hoi4_live_native::framing::serve(stdin.lock(), stdout.lock(), |message| {
        #[cfg(target_os = "macos")]
        if let Some(commands) = &commands {
            return server.handle_with_commands(message, Some(commands));
        }
        server.handle(message)
    })?;
    Ok(())
}
