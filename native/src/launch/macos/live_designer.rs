//! Opt-in live read-only checks of the Division Designer and template rows
//! (`HOI4_LIVE_SESSION=<owned session.json> cargo test --release live_ -- --ignored`, test
//! binary signed with the debugger entitlement). Nothing here sends input.
use super::{deployment, designer, ui::read, units};
use crate::platform;
use std::path::PathBuf;

fn session() -> Option<Box<dyn platform::Process + Send + Sync>> {
    let path = PathBuf::from(std::env::var_os("HOI4_LIVE_SESSION")?);
    Some(platform::open(&path).expect("owned session opens"))
}

#[test]
#[ignore = "needs a running owned game with the Recruit & Deploy panel open"]
fn live_template_rows() {
    let Some(p) = session() else { return };
    let rows = read(p.as_ref(), |r, i| {
        let mut out = Vec::new();
        for n in deployment::templates(r, i)? {
            let mut line = format!("{n}:");
            for b in ["add_template_button", "edit_button", "delete_button"] {
                match deployment::row_button(r, i, &n, b) {
                    Ok((rect, disabled)) => line += &format!(" {b}={rect:?} disabled={disabled}"),
                    Err(e) => line += &format!(" {b}=ERR {}", e.message),
                }
            }
            out.push(line);
        }
        match deployment::designer_button(r, i) {
            Ok(rect) => out.push(format!("designer button {rect:?}")),
            Err(e) => out.push(format!("designer button ERR {}", e.message)),
        }
        Ok(out)
    });
    eprintln!("{rows:#?}");
}

#[test]
#[ignore = "needs a running owned game with the Division Designer open"]
fn live_designer() {
    let Some(p) = session() else { return };
    let report = read(p.as_ref(), |r, i| {
        let mut out = Vec::new();
        out.push(format!("open {:?}", designer::is_open(r, i)));
        let d = designer::draft(r, i)?;
        out.push(format!("draft {d:?}"));
        for g in [designer::Grid::Regiments, designer::Grid::Support] {
            for s in designer::slots(r, i, g, &d)? {
                out.push(format!("{g:?} {s:?}"));
            }
        }
        out.push(format!("dropdown {:?}", designer::dropdown(r, i)));
        out.push(format!("texts {:?}", designer::texts(r, i)));
        out.push(format!("save {:?}", designer::save_button(r, i)));
        out.push(format!("close {:?}", designer::close_button(r, i)));
        for p in 0..3 {
            out.push(format!("priority {p} {:?}", designer::priority_button(r, i, p)));
        }
        out.push(format!("selection {:?}", designer::selection(r, i)));
        out.push(format!("layouts {:?}", units::template_layouts(r, i)?));
        Ok(out)
    });
    match report {
        Ok(lines) => eprintln!("{}", lines.join("\n")),
        Err(e) => eprintln!("ERR {} {}", e.code, e.message),
    }
}

/// One-off cleanup helper: click the cancel button of the LAST conveyor card in the Recruit &
/// Deploy list with ordinary clicks. Sends input; only run by the exclusive input owner.
#[test]
#[ignore = "sends input"]
fn live_cancel_last_conveyor() {
    use crate::{gui_tree, memory::Reader, platform::macos_input::MacKeys};
    let Some(p) = session() else { return };
    let process: std::sync::Arc<dyn platform::Process + Send + Sync> = std::sync::Arc::from(p);
    let keys = MacKeys::bind_owned(process.clone()).expect("bind");
    fn find(r: &Reader<'_>, image: u64, o: u64, n: &str) -> crate::error::Result<Option<u64>> {
        for c in gui_tree::children(r, o)? {
            if gui_tree::name(r, c, image)?.as_deref() == Some(n) {
                return Ok(Some(c));
            }
            if let Some(inner) = gui_tree::container(r, c, image)? {
                if let Some(f) = find(r, image, inner, n)? {
                    return Ok(Some(f));
                }
            }
        }
        Ok(None)
    }
    let rect = read(process.as_ref(), |r: &Reader<'_>, image| {
        let idler = r.u64(image + crate::game::layout(r).idler_global)?;
        let view = r.u64(r.u64(idler + 0x670)? + 0x138)?;
        let root = gui_tree::container(r, r.u64(view + 0x430)?, image)?.unwrap();
        let grid = find(r, image, root, "deployments_list_gridbox")?.expect("grid");
        let (_, data, count) = r.array_header(grid + 0x190)?;
        let mut found = None;
        for i in 0..count as u64 {
            let item = r.u64(data + i * 8)?;
            if r.object_type(item, "CMilitaryDeploymentConveyorView").is_err() {
                continue;
            }
            let frame = gui_tree::container(r, r.u64(item + 0x10)?, image)?.unwrap();
            let btn = gui_tree::child(r, image, frame, "cancel_conveyor_button")?.unwrap();
            found = Some(gui_tree::rect(r, image, btn, (1920, 1080))?);
        }
        Ok(found)
    })
    .expect("read")
    .expect("a conveyor");
    eprintln!("last conveyor cancel {rect:?}");
    let (x, y) = rect.center();
    keys.click((1920, 1080), x, y).expect("click");
    std::thread::sleep(std::time::Duration::from_millis(800));
}

/// Owner-only manual helper: HOI4_X/HOI4_Y game coordinates to click (HOI4_KEY=letter to press).
#[test]
#[ignore = "sends input"]
fn live_click() {
    use crate::platform::macos_input::{Key, MacKeys};
    let Some(p) = session() else { return };
    let process: std::sync::Arc<dyn platform::Process + Send + Sync> = std::sync::Arc::from(p);
    let keys = MacKeys::bind_owned(process).expect("bind");
    if let Ok(k) = std::env::var("HOI4_KEY") {
        let key = match k.as_str() {
            "return" => Key::Return,
            "escape" => Key::Escape,
            other => Key::letter(other.chars().next().unwrap()).unwrap(),
        };
        keys.press(key).expect("key");
        return;
    }
    let x: f64 = std::env::var("HOI4_X").unwrap().parse().unwrap();
    let y: f64 = std::env::var("HOI4_Y").unwrap().parse().unwrap();
    keys.click((1920, 1080), x, y).expect("click");
}
