use hoi4_live_native::{launch, platform, protocol::Server};
use std::{io, path::PathBuf};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args().skip(1);
    let session = match args.next().as_deref() {
        None => None,
        Some("--session") => Some(PathBuf::from(
            args.next().ok_or("--session requires a manifest")?,
        )),
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
            eprintln!("hoi4-live-native [--session OWNED_RUN_MANIFEST]\nStdio MCP; observation tools only.\nLinux: --serve PRIVATE_CONFIG / --connect RUN_DIRECTORY / --close RUN_DIRECTORY.\nWindows/macOS launch adapters unverified.");
            return Ok(());
        }
        Some(_) => return Err("Unknown argument".into()),
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
    let stdin = io::stdin();
    let stdout = io::stdout();
    hoi4_live_native::framing::serve(stdin.lock(), stdout.lock(), |message| {
        server.handle(message)
    })?;
    Ok(())
}
