//! Opt-in live read-only checks of the equipment designers
//! (`HOI4_LIVE_SESSION=<owned session.json> cargo test --release live_designs -- --ignored`, test
//! binary signed with the debugger entitlement). Nothing here sends input.
use super::{design_ui as ui, ui::read};
use crate::platform;
use std::path::PathBuf;

fn session() -> Option<Box<dyn platform::Process + Send + Sync>> {
    let path = PathBuf::from(std::env::var_os("HOI4_LIVE_SESSION")?);
    Some(platform::open(&path).expect("owned session opens"))
}

#[test]
#[ignore = "needs a running owned game with the Production selection list open"]
fn live_designs_rows() {
    let Some(p) = session() else { return };
    let rows = read(p.as_ref(), ui::rows);
    match rows {
        Ok(rows) => {
            for r in rows {
                eprintln!("{} {} {:?} shown={} variant={:?} obsolete={:?}/{} equip={:?}", r.id, r.key, r.name, r.shown, r.variant_button, r.obsolete_button, r.obsolete_state, r.equipment_button);
            }
        }
        Err(e) => eprintln!("ERR {} {}", e.code, e.message),
    }
}

#[test]
#[ignore = "needs a running owned game with a designer open"]
fn live_designs_state() {
    let Some(p) = session() else { return };
    let open = read(p.as_ref(), ui::open_designer).map(|o| o.map(|(k, _, _)| k.name()));
    eprintln!("open designer {open:?}");
    match read(p.as_ref(), |r, i| match ui::open_designer(r, i)? {
        Some((ui::Kind::Upgrade, _, _)) => ui::upgrade_state(r, i).map(|s| format!("{s:#?}")),
        Some(_) => ui::module_state(r, i).map(|s| format!("{s:#?}")),
        None => Ok("none".to_string()),
    }) {
        Ok(text) => eprintln!("{text}"),
        Err(e) => eprintln!("ERR {} {}", e.code, e.message),
    }
}

#[test]
#[ignore = "needs a running owned game"]
fn live_designs_diff() {
    let Some(p) = session() else { return };
    let a: i32 = std::env::var("DESIGN_A").ok().and_then(|v| v.parse().ok()).unwrap_or(1335);
    let b: i32 = std::env::var("DESIGN_B").ok().and_then(|v| v.parse().ok()).unwrap_or(5186);
    let all = super::designs::read_all(p.as_ref()).expect("designs");
    let find = |id: i32| all.iter().find(|d| d.variant.id == id).map(|d| d.variant.address);
    let (Some(x), Some(y)) = (find(a), find(b)) else {
        eprintln!("designs {a}/{b} not found");
        return;
    };
    let (bx, by) = read(p.as_ref(), |r, _| Ok((r.read(x, 0x500)?, r.read(y, 0x500)?))).expect("read");
    for off in (0..0x500).step_by(4) {
        if bx[off..off + 4] != by[off..off + 4] {
            eprintln!(
                "+{off:#x}: {:#x} ({}) -> {:#x} ({})",
                u32::from_le_bytes(bx[off..off + 4].try_into().unwrap()),
                i32::from_le_bytes(bx[off..off + 4].try_into().unwrap()),
                u32::from_le_bytes(by[off..off + 4].try_into().unwrap()),
                i32::from_le_bytes(by[off..off + 4].try_into().unwrap()),
            );
        }
    }
    eprintln!("addresses {x:#x} {y:#x}");
}
