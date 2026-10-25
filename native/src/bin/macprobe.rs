//! Read-only macOS probe: `macprobe SESSION_MANIFEST TOOL...` prints the JSON
//! of the pure-read macOS tools (resources, shortages, industry) for an owned
//! session. Never binds input, never touches the controller epoch or journals.
#[cfg(target_os = "macos")]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args().skip(1);
    let session = std::path::PathBuf::from(args.next().ok_or("usage: macprobe SESSION TOOL...")?);
    let process = hoi4_live_native::platform::open(&session)?;
    for tool in args {
        match hoi4_live_native::launch::macos::read_only_call(process.as_ref(), &tool) {
            Ok(v) => println!("{}", serde_json::to_string_pretty(&v)?),
            Err(e) => println!(
                "{}",
                serde_json::to_string(&serde_json::json!({"error": e}))?
            ),
        }
    }
    Ok(())
}
#[cfg(not(target_os = "macos"))]
fn main() {
    eprintln!("macprobe is macOS only");
}
