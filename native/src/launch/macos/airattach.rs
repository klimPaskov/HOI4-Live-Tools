//! `hoi4_air_attach`: attach own air wings to an army, the game's own way (air.md, 1.19.4): with
//! the wings selected, a right click on the army's card ("Right-Click to attach Airwings to
//! Army"). An attached wing supports that army's battles (close air support over its fights,
//! fighter cover over its provinces) and follows it between regions. Naval and strategic
//! bombers and wings in pilot exercises cannot attach (WING_CAN_NOT_ATTACH_*).
//!
//! Read-back: the wing object's attached-army pointer (airread::WING_ATTACHED_ARMY) equals the
//! army's object after the click (mapped live on 23 November 2026, 1.19.4.1, by diffing the wing
//! and the army around two attachments). The selection rows were not readable: their list box keeps
//! no child windows the reader walks.
use super::airread;
use super::ui::{clear_notices, read};
use crate::error::{Error, Result};
use crate::platform::game_input::GameInput;
use crate::platform::Process;
use crate::xlayout::fld;
use serde_json::{json, Value};
use std::time::{Duration, Instant};

const GAME: (u32, u32) = (1920, 1080);
/// How long the link may take to change after the right click.
const ATTACH_WAIT: Duration = Duration::from_millis(2500);

/// The army object `wing` is attached to (0 when none).
fn link(process: &dyn Process, wing: &str) -> Result<u64> {
    read(process, |r, i| {
        let w = super::airact::own_wing(r, i, wing)?
            .ok_or_else(|| Error::new("wing_unknown", format!("No own wing named {wing}")))?;
        let at = fld(r.string_abi(), &airread::WING, airread::WING_ATTACHED_ARMY)?;
        r.u64(w.address + at)
    })
}

/// Attach each wing to `army`: a wing already linked to it is left alone; else it is selected
/// alone (its Air Forces Overview row), the army's card right-clicked and the wing's link read
/// back. One wing at a time.
pub(super) fn attach(
    process: &dyn Process,
    keys: &dyn GameInput,
    wings: &[String],
    army: &str,
) -> Result<Value> {
    clear_notices(process, keys)?;
    // the army must exist before any input
    let target = super::armies::army_address(process, army)?;
    let mut rows_out = Vec::new();
    for wing in wings {
        let started = Instant::now();
        let row = (|| -> Result<Value> {
            if link(process, wing)? == target {
                return Ok(json!({"wing": wing, "attached": true, "already": true}));
            }
            super::air::select_wing(process, keys, wing)?;
            let card = super::armies::shown_card(process, keys, army)?;
            let (x, y) = card.center();
            keys.right_click(GAME, x, y)?;
            let deadline = Instant::now() + ATTACH_WAIT;
            let mut attached = false;
            while Instant::now() < deadline {
                if link(process, wing)? == target {
                    attached = true;
                    break;
                }
                crate::phase::sleep(Duration::from_millis(150));
            }
            Ok(json!({"wing": wing, "attached": attached}))
        })();
        let _ = super::air::deselect(process, keys);
        let mut row = match row {
            Ok(v) => v,
            Err(e) => {
                json!({"wing": wing, "attached": false, "error": e.code, "message": e.message})
            }
        };
        row["ms"] = json!(started.elapsed().as_millis() as u64);
        rows_out.push(row);
    }
    let attached = rows_out
        .iter()
        .filter(|r| r["attached"] == json!(true))
        .count();
    if attached == 0 {
        return Err(Error::new(
            "input_readback_failed",
            format!(
                "No wing is attached to {army} after the right click on its card (the game refuses wings in pilot exercises, naval and strategic bombers): {}",
                serde_json::to_string(&rows_out).unwrap_or_default()
            ),
        ));
    }
    Ok(json!({"army": army, "attached": attached, "of": wings.len(), "wings": rows_out}))
}

/// Attach in process (family airwing): one Attach(wing, army) command per wing not yet linked to the army, then the
/// same read-back as the GUI path (each wing's army link equals the army's object). None hands the call to the GUI
/// path (a wing or the army not known by address, or the module did not post); a game-check refusal is the answer.
pub(super) fn attach_inprocess(
    process: &dyn Process,
    ch: &mut super::inproc::Channel,
    wings: &[String],
    army: &str,
) -> Result<Option<Value>> {
    let target = super::armies::army_address(process, army)?;
    let (_, own) = super::airact::own(process)?;
    let mut rows_out = Vec::new();
    let mut post = Vec::new();
    for wing in wings {
        if link(process, wing)? == target {
            rows_out.push(json!({"wing": wing, "attached": true, "already": true}));
            continue;
        }
        match super::inproc::wing_address(&own, wing) {
            Some(a) => post.push((wing.clone(), a)),
            None => return Ok(None),
        }
    }
    let started = Instant::now();
    if !post.is_empty() {
        let objs: Vec<u64> = post.iter().map(|(_, a)| *a).collect();
        let Ok(r) = ch.post("airwing.attach", &objs, &[], Some(target), None) else {
            return Ok(None);
        };
        if super::inproc::game_refused(&r) {
            return Err(super::inproc::refused_error(
                "attaching these wings to the army",
            ));
        }
        if !super::inproc::all_posted(&r) {
            return Ok(None);
        }
        let deadline = Instant::now() + ATTACH_WAIT;
        let mut pending: Vec<String> = post.iter().map(|(w, _)| w.clone()).collect();
        while !pending.is_empty() && Instant::now() < deadline {
            pending.retain(|w| link(process, w).map(|l| l != target).unwrap_or(true));
            if !pending.is_empty() {
                crate::phase::sleep(Duration::from_millis(15));
            }
        }
        for (w, _) in &post {
            rows_out.push(
                json!({"wing": w, "attached": !pending.contains(w), "backend": "inprocess",
                "ms": started.elapsed().as_millis() as u64}),
            );
        }
    }
    let attached = rows_out
        .iter()
        .filter(|r| r["attached"] == json!(true))
        .count();
    if attached == 0 {
        return Err(Error::new(
            "input_readback_failed",
            format!(
                "No wing is attached to {army} after the in-process attach (the game refuses wings in pilot exercises, naval and strategic bombers): {}",
                serde_json::to_string(&rows_out).unwrap_or_default()
            ),
        ));
    }
    Ok(Some(
        json!({"army": army, "attached": attached, "of": wings.len(), "wings": rows_out, "backend": "inprocess"}),
    ))
}
