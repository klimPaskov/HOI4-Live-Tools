//! Army exercises: divisions train toward the game's training maximum and the country earns army
//! experience, paid for with extra attrition and fuel (NDefines.NMilitary TRAINING_* in the
//! loaded defines).
//!
//! Start: the army's card selects the army, the battle plan bar slides in
//! (interface/battleplantools.gui, `battleplantools_window`) and its button `btn_exercise` is
//! clicked; a Shift+click is the bar's own "stop at the maximum" choice (the button carries the
//! shortcuts k and shift+k). Stop: the card's own `btn_stop_training`, shown while the army
//! exercises (interface/leadergroups.gui). Both are found by name in the GUI tree, nothing by a
//! fixed position.
//!
//! Readback: the card's `training_marker` (tooltip UNITS_VIEW_EXERCISING) is shown while the army
//! exercises, and `icn_exercise_stop_at_max` while it stops at the maximum. The card is read, not
//! the click: a refusal of the game (no division can train, the button disabled) leaves the marker
//! as it was and the call reports it.
use super::super::{
    invasion::BATTLE_PLAN_TOOLS,
    kit,
    ui::{clear_notices, read, settled, wait},
};
use super::{card_widget, clear_selection, select_army_quick, GAME};
use crate::{
    error::{Error, Result},
    gui_tree,
    memory::Reader,
    platform::{game_input::GameInput, Process},
};
use serde_json::{json, Value};

const BAR: &str = "battleplantools_window";
/// The bar's exercise button, and the animated one it shows while the selection exercises.
const START: &str = "btn_exercise";
const START_ANIM: &str = "btn_exercise_anim";
/// The card's stop button and markers.
const STOP: &str = "btn_stop_training";
const MARKER: &str = "training_marker";
const AT_MAX: &str = "icn_exercise_stop_at_max";

/// What an army card shows of exercises.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in super::super) struct Shown {
    pub exercising: bool,
    pub stop_at_max: bool,
}
impl Shown {
    pub(in super::super) fn json(self) -> Value {
        json!({"exercising":self.exercising,"stop_at_max":self.stop_at_max})
    }
}

fn shown(r: &Reader<'_>, image: u64, army: &str, name: &str) -> Result<bool> {
    match card_widget(r, image, army, name) {
        Ok(w) => gui_tree::shown(r, w),
        Err(e) if e.code == "gui_target_unavailable" && name == AT_MAX => Ok(false),
        Err(e) => Err(e),
    }
}

/// Whether `army` exercises now, from its card.
pub(in super::super) fn state(r: &Reader<'_>, image: u64, army: &str) -> Result<Shown> {
    let exercising = shown(r, image, army, MARKER)?;
    let stop_at_max = exercising && shown(r, image, army, AT_MAX)?;
    Ok(Shown {
        exercising,
        stop_at_max,
    })
}

/// Whether the card says what was asked: on/off, and the stop at the maximum when asked for.
pub(in super::super) fn reached(now: Shown, on: bool, stop_at_max: bool) -> bool {
    if !on {
        return !now.exercising;
    }
    now.exercising && (!stop_at_max || now.stop_at_max)
}

/// The bar's exercise button (the plain one, or the animated one while exercising).
fn bar_button(r: &Reader<'_>, image: u64) -> Result<Option<(gui_tree::Rect, bool)>> {
    let Some(view) = kit::find_view(r, image, BATTLE_PLAN_TOOLS)? else {
        return Ok(None);
    };
    let Some(bar) = kit::find_window(r, image, view, BAR)? else {
        return Ok(None);
    };
    for name in [START, START_ANIM] {
        let Some(button) = kit::find_deep(r, image, bar, name, 6)? else {
            continue;
        };
        if !gui_tree::shown(r, button)? {
            continue;
        }
        if let Ok(rect) = gui_tree::rect(r, image, button, GAME) {
            if rect.x >= 0 && rect.y >= 0 {
                return Ok(Some((rect, gui_tree::disabled(r, button)?)));
            }
        }
    }
    Ok(None)
}

/// Start (`on`) or stop exercises of `army` and wait for its card to say so. A start with
/// `stop_at_max` uses the bar's Shift+click. An army already in the asked state is left alone.
pub(in super::super) fn set(
    process: &dyn Process,
    keys: &dyn GameInput,
    army: &str,
    on: bool,
    stop_at_max: bool,
) -> Result<()> {
    clear_notices(process, keys)?;
    let before = read(process, |r, i| state(r, i, army))?;
    if reached(before, on, stop_at_max) {
        return Ok(());
    }
    if !on || before.exercising {
        // stop from the card (also the first half of switching to stop at the maximum)
        let rect = settled(process, "stop training button", |r, i| {
            let w = card_widget(r, i, army, STOP)?;
            if !gui_tree::shown(r, w)? {
                return Err(Error::new(
                    "gui_target_unavailable",
                    "The army card shows no stop-training button",
                ));
            }
            gui_tree::rect(r, i, w, GAME)
        })?;
        let (x, y) = rect.center();
        keys.click(GAME, x, y)?;
        wait(process, "exercise stopped", |r, i| {
            Ok(!state(r, i, army)?.exercising)
        })
        .map_err(|_| {
            Error::new(
                "input_readback_failed",
                format!("The stop-training click on {army}'s card left the army exercising"),
            )
        })?;
        if !on {
            return Ok(());
        }
    }
    select_army_quick(process, keys, army)?;
    let found = settled(process, "exercise button", bar_button);
    let (rect, disabled) = match found {
        Ok(Some(b)) => b,
        Ok(None) | Err(_) => {
            let _ = clear_selection(process, keys);
            return Err(Error::new(
                "gui_target_unavailable",
                format!("The battle plan bar ({BAR}) shows no {START} button with {army} selected"),
            ));
        }
    };
    if disabled {
        let _ = clear_selection(process, keys);
        return Err(Error::new(
            "exercise_unavailable",
            format!(
                "The game disables the exercise button for {army} now (its tooltip says why: \
                 no division that can train, at the maximum already, or out of supply)"
            ),
        ));
    }
    let (x, y) = rect.center();
    if stop_at_max {
        keys.shift_click(GAME, x, y)?;
    } else {
        keys.click(GAME, x, y)?;
    }
    let done = wait(process, "exercise started", |r, i| {
        Ok(reached(state(r, i, army)?, true, stop_at_max))
    });
    let _ = clear_selection(process, keys);
    done.map_err(|_| {
        let now = read(process, |r, i| state(r, i, army)).ok();
        Error::new(
            "action_not_applied",
            format!(
                "The exercise click for {army} did not show on its card (card now: {})",
                now.map(|s| s.json().to_string())
                    .unwrap_or_else(|| "unreadable".into())
            ),
        )
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn s(exercising: bool, stop_at_max: bool) -> Shown {
        Shown {
            exercising,
            stop_at_max,
        }
    }

    #[test]
    fn the_card_decides_whether_the_order_was_carried_out() {
        assert!(reached(s(true, true), true, true));
        assert!(reached(s(true, false), true, false));
        // exercising without the stop at the maximum is not what was asked
        assert!(!reached(s(true, false), true, true));
        assert!(!reached(s(false, false), true, false));
        assert!(reached(s(false, false), false, false));
        assert!(!reached(s(true, true), false, true));
    }
}
