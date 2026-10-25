//! Opt-in live read-only checks (`cargo test --release -- --ignored live_`
//! with `HOI4_LIVE_SESSION=<owned session.json>`): open the owned process
//! with the same read-only binding as `--session` (no input binding) and
//! print what the unit-level read tools return. Skipped by default.
use super::{commanders, supply, template_get};
use crate::platform;
use std::path::PathBuf;

fn session() -> Option<Box<dyn platform::Process + Send + Sync>> {
    let path = PathBuf::from(std::env::var_os("HOI4_LIVE_SESSION")?);
    Some(platform::open(&path).expect("owned session opens"))
}
fn dump(name: &str, v: &serde_json::Value) {
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
fn live_template_get() {
    let Some(p) = session() else { return };
    let started = std::time::Instant::now();
    let v = template_get::get(p.as_ref(), None).expect("template read");
    eprintln!("hoi4_template_get {} ms", started.elapsed().as_millis());
    dump("template_get", &v);
}
#[test]
#[ignore = "needs a running owned game"]
fn live_supply() {
    let Some(p) = session() else { return };
    let started = std::time::Instant::now();
    let v = supply::report(p.as_ref(), 0.7).expect("supply read");
    eprintln!("hoi4_supply {} ms", started.elapsed().as_millis());
    dump("supply", &v);
}
#[test]
#[ignore = "needs a running owned game"]
fn live_commanders() {
    let Some(p) = session() else { return };
    let started = std::time::Instant::now();
    let v = commanders::list(p.as_ref()).expect("commanders read");
    eprintln!("hoi4_commanders {} ms", started.elapsed().as_millis());
    dump("commanders", &v);
}
#[test]
#[ignore = "needs a running owned game"]
fn live_leaders() {
    let Some(p) = session() else { return };
    let v = super::ui::read(p.as_ref(), super::units::leaders);
    eprintln!("{v:?}");
}
