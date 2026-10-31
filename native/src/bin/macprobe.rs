//! Read-only macOS probe: `macprobe SESSION_MANIFEST TOOL...` prints the JSON
//! of the pure-read macOS tools (resources, shortages, industry) for an owned
//! session. Never binds input, never touches the controller epoch or journals.
#[cfg(target_os = "macos")]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args().skip(1);
    let session = std::path::PathBuf::from(args.next().ok_or("usage: macprobe SESSION TOOL...")?);
    let process = hoi4_live_native::platform::open(&session)?;
    for tool in args {
        // `peek:RVA_OR_ADDR:LEN` hex-dumps raw memory (RVA when below the image size).
        if let Some(spec) = tool.strip_prefix("peek:") {
            let mut it = spec.split(':');
            let a = u64::from_str_radix(it.next().unwrap_or("0").trim_start_matches("0x"), 16)?;
            let n: usize = it.next().unwrap_or("64").parse()?;
            let addr = if a < 0x1_0000_0000 { process.image_base() + a } else { a };
            match process.read(addr, n) {
                Ok(b) => {
                    for (i, c) in b.chunks(8).enumerate() {
                        let v = u64::from_le_bytes({ let mut x = [0u8; 8]; x[..c.len()].copy_from_slice(c); x });
                        let rva = if v > process.image_base() && v < process.image_base() + 0x1_0000_0000 { format!(" rva {:#x}", v - process.image_base()) } else { String::new() };
                        println!("{:#06x}: {:#018x}{}", i * 8, v, rva);
                    }
                }
                Err(e) => println!("peek {addr:#x}: {e:?}"),
            }
            continue;
        }
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
