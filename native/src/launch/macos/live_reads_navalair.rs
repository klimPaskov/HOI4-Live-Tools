//! Opt-in live read-only checks of the naval, air and design reads
//! (`cargo test --release -- --ignored live_navalair --nocapture` with
//! `HOI4_LIVE_SESSION=<owned session.json>`); read-only binding, no input.
use super::{airread, designs, naval};
use crate::platform;
use serde_json::{json, Map, Value};
use std::path::PathBuf;

fn session() -> Option<Box<dyn platform::Process + Send + Sync>> {
    let path = PathBuf::from(std::env::var_os("HOI4_LIVE_SESSION")?);
    Some(platform::open(&path).expect("owned session opens"))
}
fn args(v: Value) -> Map<String, Value> {
    v.as_object().cloned().unwrap_or_default()
}
fn show(name: &str, v: &Value) {
    let text = serde_json::to_string_pretty(v).unwrap();
    if let Ok(dir) = std::env::var("HOI4_LIVE_OUT") {
        std::fs::write(format!("{dir}/{name}.json"), &text).unwrap();
    }
    eprintln!("{name}: {} bytes", text.len());
}
#[test]
#[ignore = "needs a running owned game"]
fn live_navalair_fleets() {
    let Some(p) = session() else { return };
    let t = std::time::Instant::now();
    let v = naval::fleets_tool(p.as_ref(), &args(json!({"ships":true}))).expect("fleets");
    eprintln!("hoi4_fleets {} ms", t.elapsed().as_millis());
    show("fleets", &v);
}
#[test]
#[ignore = "needs a running owned game"]
fn live_navalair_convoys() {
    let Some(p) = session() else { return };
    let v = naval::convoys_tool(p.as_ref(), &args(json!({}))).expect("convoys");
    show("convoys", &v);
}
#[test]
#[ignore = "needs a running owned game"]
fn live_navalair_air() {
    let Some(p) = session() else { return };
    let t = std::time::Instant::now();
    let v = airread::tool(p.as_ref(), &args(json!({}))).expect("air");
    eprintln!("hoi4_air {} ms", t.elapsed().as_millis());
    show("air", &v);
}
#[test]
#[ignore = "needs a running owned game"]
fn live_navalair_designs() {
    let Some(p) = session() else { return };
    let t = std::time::Instant::now();
    let v = designs::tool(p.as_ref(), &args(json!({"detail":true}))).expect("designs");
    eprintln!("hoi4_designs {} ms", t.elapsed().as_millis());
    show("designs", &v);
}
