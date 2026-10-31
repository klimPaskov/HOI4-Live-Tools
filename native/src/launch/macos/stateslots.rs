//! Building slots of own states, read from the game's state view: the view's
//! "shared_slot_count" text is `unlocked/maximum` slots and its slot grid holds
//! one item per maximum slot, with a building template in each occupied one.
//! Free slots (unlocked minus occupied) tell where a factory can still be
//! queued; states are opened with the Find view and one click on the state.
use super::ui::{clear_notices, dismiss, find_state, read, wait};
use crate::{
    error::{Error, Result},
    game, gui_tree,
    memory::{Reader, StringAbi},
    platform::{macos_input::MacKeys, Process},
};
use serde_json::{json, Value};

struct Ui {
    manager: u64,
    view: u64,
    view_root: u64,
    grid_items: u64,
    item_template: u64,
    text: u64,
}
/// macOS Mach-O 089EA519, mapped live on 19 October 2026: Brandenburg showed
/// "12/25 Unlocked Slots" with 12 occupied slot items; the construction map
/// mode's tooltip for the same state read "used 12 out of 12 building slots".
const MACOS: Ui = Ui {
    manager: 0x670,
    view: 0xc8,
    view_root: 0x490,
    grid_items: 0x190,
    item_template: 0x28,
    text: 0x100,
};
const GAME: (u32, u32) = (1920, 1080);

fn ui(r: &Reader<'_>) -> Result<&'static Ui> {
    match r.string_abi() {
        StringAbi::LibCxx => Ok(&MACOS),
        StringAbi::LibStdCxx => Err(Error::new(
            "capability_unavailable",
            "State slot reads are not reviewed for this build",
        )),
    }
}
fn root(r: &Reader<'_>, image: u64) -> Result<u64> {
    let u = ui(r)?;
    let idler = r.u64(image + game::layout(r).idler_global)?;
    let view = r.u64(r.u64(idler + u.manager)? + u.view)?;
    gui_tree::container(r, r.u64(view + u.view_root)?, image)?
        .ok_or_else(|| Error::new("invalid_gui", "State view root unavailable"))
}
fn find(r: &Reader<'_>, image: u64, object: u64, name: &str, depth: u32) -> Result<Option<u64>> {
    for child in gui_tree::children(r, object)? {
        if gui_tree::name(r, child, image)?.as_deref() == Some(name) {
            return Ok(Some(child));
        }
        if depth < 5 {
            if let Some(inner) = gui_tree::container(r, child, image)? {
                if let Some(found) = find(r, image, inner, name, depth + 1)? {
                    return Ok(Some(found));
                }
            }
        }
    }
    Ok(None)
}
/// Digits of the first two numbers in `text` (markup bytes are skipped).
fn two_numbers(text: &str) -> Option<(u32, u32)> {
    let mut numbers = Vec::new();
    let mut current = String::new();
    for c in text.chars() {
        if c.is_ascii_digit() {
            current.push(c);
        } else if !current.is_empty() {
            numbers.push(current.parse::<u32>().ok()?);
            current.clear();
        }
        if numbers.len() == 2 {
            break;
        }
    }
    if !current.is_empty() && numbers.len() < 2 {
        numbers.push(current.parse::<u32>().ok()?);
    }
    (numbers.len() >= 2).then(|| (numbers[0], numbers[1]))
}
#[derive(Clone, Debug, PartialEq)]
struct Slots {
    name: String,
    unlocked: u32,
    maximum: u32,
    used: u32,
}
fn slots(r: &Reader<'_>, image: u64) -> Result<Option<Slots>> {
    let u = ui(r)?;
    let window = root(r, image)?;
    if !gui_tree::visible(r, window)? {
        return Ok(None);
    }
    let Some(info_base) = find(r, image, window, "state_info_window", 0)? else {
        return Ok(None);
    };
    let Some(info) = gui_tree::container(r, info_base, image)? else {
        return Ok(None);
    };
    let text = |name: &str| -> Result<Option<String>> {
        Ok(match find(r, image, info, name, 0)? {
            Some(t) => Some(r.string_bounded(t + u.text, 128)?),
            None => None,
        })
    };
    let (Some(name), Some(count)) = (text("state_name")?, text("shared_slot_count")?) else {
        return Ok(None);
    };
    let Some((unlocked, maximum)) = two_numbers(&count) else {
        return Ok(None);
    };
    let grid = find(r, image, info, "state_shared_slot_building_entries", 0)?
        .ok_or_else(|| Error::new("gui_target_unavailable", "Slot grid not found"))?;
    let (_, data, n) = r.array_header(grid + u.grid_items)?;
    let mut used = 0;
    for index in 0..n.min(64) as u64 {
        let item = r.u64(data + index * 8)?;
        if r.u64(item + u.item_template)? != 0 {
            used += 1;
        }
    }
    Ok(Some(Slots {
        name,
        unlocked,
        maximum,
        used,
    }))
}

/// Free building slots of own states `ids` (opens each state's view).
pub(super) fn read_states(process: &dyn Process, keys: &MacKeys, ids: &[i32]) -> Result<Value> {
    clear_notices(process, keys)?;
    // F1 returns the map to its plain mode (an air or construction map mode
    // would turn the click on a state into a different action).
    keys.press_code(122)?;
    std::thread::sleep(std::time::Duration::from_millis(500));
    let mut rows = Vec::new();
    for &id in ids {
        find_state(process, keys, id)?;
        std::thread::sleep(std::time::Duration::from_millis(600));
        let opened = |r: &Reader<'_>, i: u64| -> Result<bool> {
            Ok(slots(r, i)?.is_some_and(|s| s.unlocked > 0 || s.maximum > 0))
        };
        let mut seen = Err(Error::new("input_readback_failed", "no click"));
        // Divisions standing at the state's centre take the click as a
        // selection; try points around it (the first that opens the view wins).
        const POINTS: [(f64, f64); 6] = [
            (960.0, 540.0),
            (900.0, 610.0),
            (1020.0, 610.0),
            (900.0, 470.0),
            (1020.0, 470.0),
            (960.0, 660.0),
        ];
        for (x, y) in POINTS {
            if read(process, opened)? {
                seen = Ok(());
                break;
            }
            keys.click(GAME, x, y)?;
            // The view slides in; give it time before judging the click.
            std::thread::sleep(std::time::Duration::from_millis(900));
            seen = wait(process, "state view", opened);
            if seen.is_ok() {
                break;
            }
        }
        let mut why = String::new();
        let row = match seen {
            Ok(()) => read(process, slots)?,
            Err(e) => {
                why = e.message;
                None
            }
        };
        rows.push(match row {
            Some(s) => json!({"state_id":id,"name":s.name,"unlocked_slots":s.unlocked,
                "max_slots":s.maximum,"used_slots":s.used,
                "free_slots":s.unlocked.saturating_sub(s.used)}),
            None => json!({"state_id":id,"error":format!("state view did not open (not an own state?) {why}")}),
        });
        dismiss(process, keys, |r, i| {
            Ok(!gui_tree::visible(r, root(r, i)?)?)
        });
    }
    Ok(json!({"states":rows,
        "note":"free_slots = unlocked - used; queued projects may also reserve slots"}))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn parses_the_slot_text() {
        assert_eq!(
            two_numbers("\u{11}Y12\u{11}!/\u{11}Y25\u{11}! Unlocked Slots"),
            Some((12, 25))
        );
        assert_eq!(two_numbers("5/5"), Some((5, 5)));
        assert_eq!(two_numbers("none"), None);
    }
}
