use hoi4_live_native::{platform, protocol::Server};
use serde_json::{json, Value};
use std::{
    io::{self, BufRead, Write},
    path::PathBuf,
};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args().skip(1);
    let session = match args.next().as_deref() {
        None => None,
        Some("--session") => Some(PathBuf::from(
            args.next().ok_or("--session requires a manifest")?,
        )),
        Some("--help") => {
            eprintln!("hoi4-live-native [--session OWNED_RUN_MANIFEST]\nStdio MCP; read-only Linux foundation. Windows/macOS adapters unverified.");
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
    let mut input = stdin.lock();
    let stdout = io::stdout();
    let mut output = stdout.lock();
    loop {
        // A bound is applied before allocating an arbitrarily long input line.
        let mut line = Vec::new();
        let mut too_large = false;
        loop {
            let bytes = input.fill_buf()?;
            if bytes.is_empty() {
                break;
            }
            let end = bytes
                .iter()
                .position(|v| *v == b'\n')
                .map(|v| v + 1)
                .unwrap_or(bytes.len());
            if line.len() + end <= 1 << 20 {
                line.extend_from_slice(&bytes[..end]);
            } else {
                too_large = true;
            }
            let complete = bytes[end - 1] == b'\n';
            input.consume(end);
            if complete {
                break;
            }
        }
        if line.is_empty() && !too_large {
            break;
        }
        let reply = if too_large {
            Some(
                json!({"jsonrpc":"2.0","id":null,"error":{"code":-32600,"message":"Request exceeds bound"}}),
            )
        } else {
            match serde_json::from_slice::<Value>(&line) {
                Ok(msg) => server.handle(msg),
                Err(_) => Some(
                    json!({"jsonrpc":"2.0","id":null,"error":{"code":-32700,"message":"Parse error"}}),
                ),
            }
        };
        if let Some(reply) = reply {
            serde_json::to_writer(&mut output, &reply)?;
            output.write_all(b"\n")?;
            output.flush()?;
        }
    }
    Ok(())
}
