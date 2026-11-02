//! Opt-in live read-only checks of the doctrine, MIO, intelligence and
//! research-extras tools (`cargo test --release --no-run`, sign the test
//! binary with `native/macos/debugger.entitlements`, then run it with
//! `--ignored live_mgmt` and `HOI4_LIVE_SESSION=<owned session.json>`;
//! optionally `HOI4_LIVE_OUT=<dir>` to keep the JSON). The owned process is
//! opened with the same read-only binding as `--session`: no input binding.
use super::{doctrines, intelligence, mios, research_extras};
use crate::platform;
use std::path::PathBuf;

fn session() -> Option<Box<dyn platform::Process + Send + Sync>> {
    let path = PathBuf::from(std::env::var_os("HOI4_LIVE_SESSION")?);
    Some(platform::open(&path).expect("owned session opens"))
}
fn dump(name: &str, v: &serde_json::Value) {
    eprintln!("{name}: {}", serde_json::to_string(v).unwrap());
    if let Ok(dir) = std::env::var("HOI4_LIVE_OUT") {
        std::fs::write(
            format!("{dir}/{name}.json"),
            serde_json::to_string_pretty(v).unwrap(),
        )
        .unwrap();
    }
}

#[test]
#[ignore = "needs a running owned game"]
fn live_mgmt_doctrines() {
    let Some(p) = session() else { return };
    let v = doctrines::doctrines(p.as_ref(), true).expect("doctrine read");
    dump("doctrines", &v);
}
#[test]
#[ignore = "needs a running owned game"]
fn live_mgmt_mios() {
    let Some(p) = session() else { return };
    let v = mios::mios(p.as_ref(), None, false).expect("mio read");
    dump("mios", &v);
}
#[test]
#[ignore = "needs a running owned game"]
fn live_mgmt_intelligence() {
    let Some(p) = session() else { return };
    let v = intelligence::intelligence(p.as_ref(), true).expect("intelligence read");
    dump("intelligence", &v);
}
#[test]
#[ignore = "needs a running owned game"]
fn live_mgmt_research_extras() {
    let Some(p) = session() else { return };
    let v = research_extras::research_extras(p.as_ref()).expect("research extras read");
    dump("research_extras", &v);
}
