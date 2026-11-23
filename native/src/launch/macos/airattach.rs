//! `hoi4_air_attach`: attach own air wings to an army, the game's own way (air.md, 1.19.4): with
//! the wings selected, a right click on the army's card ("Right-Click to attach Airwings to
//! Army"). An attached wing supports that army's battles (close air support over its fights,
//! fighter cover over its provinces) and follows it between regions. Naval and strategic
//! bombers and wings in pilot exercises cannot attach (WING_CAN_NOT_ATTACH_*).
//!
//! Read-back: the selected wing's row in the air selection view shows the army shield
//! (airselectionview.gui `army_attachment_button`, tooltip ATTACHED_AIR_WING_DESC) once it is
//! attached. The memory words of the wing that changed are returned with the result so the
//! attachment field can be mapped.
use super::ui::{clear_notices, read};
use crate::error::{Error, Result};
use crate::platform::game_input::GameInput;
use crate::platform::Process;
use serde_json::{json, Value};
use std::time::{Duration, Instant};

const GAME: (u32, u32) = (1920, 1080);
/// How long the shield may take to show after the right click.
const ATTACH_WAIT: Duration = Duration::from_millis(2500);
/// Wing object bytes compared before and after (the attachment field is somewhere in here).
const WING_BYTES: usize = 0x600;

/// The names of shown widgets called `army_attachment_button` (one per attached selected wing).
fn shields(process: &dyn Process) -> Result<usize> {
    let windows = super::learn_tools::windows(process, false)?;
    Ok(windows
        .iter()
        .filter(|w| w.visible)
        .flat_map(|w| w.widgets.iter())
        .filter(|x| x.shown && x.name == "army_attachment_button")
        .count())
}

fn wing_words(process: &dyn Process, name: &str) -> Option<Vec<u64>> {
    read(process, |r, i| {
        let w = super::airact::own_wing(r, i, name)?
            .ok_or_else(|| Error::new("wing_unknown", "No own wing with this name"))?;
        let bytes = r.read(w.address, WING_BYTES)?;
        Ok(bytes
            .chunks(8)
            .map(|c| {
                let mut x = [0u8; 8];
                x[..c.len()].copy_from_slice(c);
                u64::from_le_bytes(x)
            })
            .collect())
    })
    .ok()
}

/// Offsets (and old -> new values) of the words that changed.
pub(super) fn changed_words(before: &[u64], after: &[u64]) -> Vec<Value> {
    before
        .iter()
        .zip(after)
        .enumerate()
        .filter(|(_, (a, b))| a != b)
        .map(|(i, (a, b))| json!({"offset": format!("{:#x}", i * 8), "before": format!("{a:#x}"), "after": format!("{b:#x}")}))
        .collect()
}

/// Attach each wing to `army`: select it alone (its Air Forces Overview row), right-click the
/// army's card, read the shield back on its selection row. One wing at a time, so the shield
/// count belongs to that wing.
pub(super) fn attach(
    process: &dyn Process,
    keys: &dyn GameInput,
    wings: &[String],
    army: &str,
) -> Result<Value> {
    clear_notices(process, keys)?;
    // the army must exist before any input
    super::armies::card_exists(process, army)?;
    let mut rows = Vec::new();
    for wing in wings {
        let started = Instant::now();
        let before = wing_words(process, wing);
        let row = (|| -> Result<Value> {
            super::air::select_wing(process, keys, wing)?;
            let shown_before = shields(process)?;
            if shown_before > 0 {
                return Ok(json!({"wing": wing, "attached": true, "already": true}));
            }
            let card = super::armies::shown_card(process, keys, army)?;
            let (x, y) = card.center();
            keys.right_click(GAME, x, y)?;
            let deadline = Instant::now() + ATTACH_WAIT;
            let mut shown = 0;
            while Instant::now() < deadline {
                shown = shields(process)?;
                if shown > 0 {
                    break;
                }
                crate::phase::sleep(Duration::from_millis(150));
            }
            Ok(json!({"wing": wing, "attached": shown > 0}))
        })();
        let _ = super::air::deselect(process, keys);
        let mut row = match row {
            Ok(v) => v,
            Err(e) => json!({"wing": wing, "attached": false, "error": e.code, "message": e.message}),
        };
        if let (Some(b), Some(a)) = (before, wing_words(process, wing)) {
            row["changed_words"] = json!(changed_words(&b, &a));
        }
        row["ms"] = json!(started.elapsed().as_millis() as u64);
        rows.push(row);
    }
    let attached = rows.iter().filter(|r| r["attached"] == json!(true)).count();
    if attached == 0 {
        return Err(Error::new(
            "input_readback_failed",
            format!(
                "No wing shows the army shield after the right click on {army}'s card: {}",
                serde_json::to_string(&rows).unwrap_or_default()
            ),
        ));
    }
    Ok(json!({"army": army, "attached": attached, "of": wings.len(), "wings": rows}))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_changed_words_are_reported_with_their_offsets() {
        let before = [1u64, 2, 3, 4];
        let after = [1u64, 9, 3, 5];
        let d = changed_words(&before, &after);
        assert_eq!(d.len(), 2);
        assert_eq!(d[0]["offset"], "0x8");
        assert_eq!(d[0]["after"], "0x9");
        assert_eq!(d[1]["offset"], "0x18");
    }
}
