//! An event answer through the in-process module (engineer 76, family `events`): CSelectEventOptionCommand built by the
//! event window's own constructor from the open window's fields, as its option click builds it, after the window's
//! own gate (it accepts a click now), the game's option test for the option and the command's own "can do" check.
//! hoi4_event_choose keeps its name, schema and Verified read (the window closes); the mouse path stays the fallback.
//! The family is off under `HOI4_BACKEND=auto` until its live session (S8-EVENTS) passes.
use super::inproc::{Channel, Routed};
use super::inproc_units::{request, routed};

/// The option at place `shown` among the `count` options the window shows (0-based, the order hoi4_events lists).
/// The module walks the event's own options with the game's option test and refuses to guess when its count differs.
pub(super) fn answer(ch: &mut Channel, window: u64, shown: usize, count: usize) -> Routed {
    let Some(vals) = answer_vals(shown, count) else {
        return Routed::Gui;
    };
    match request("events.answer", &[window], &[], &vals, None).and_then(|r| ch.post_raw(r)) {
        Ok(reply) => routed(reply, "this option of this event now"),
        Err(_) => Routed::Gui,
    }
}

/// vals for the module: the shown place and the shown count; None outside what a window shows (1..8 options).
fn answer_vals(shown: usize, count: usize) -> Option<Vec<i64>> {
    ((1..=8).contains(&count) && shown < count).then(|| vec![shown as i64, count as i64])
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn an_answer_names_one_window_and_the_shown_place_and_count() {
        let r = request(
            "events.answer",
            &[0x6000_1000],
            &[],
            &answer_vals(1, 3).unwrap(),
            None,
        )
        .unwrap();
        assert_eq!(r["cmd"], "events.answer");
        assert_eq!(r["objs"], "0x60001000");
        assert_eq!(r["vals"], "1,3");
        assert!(r.get("divs").is_none() && r.get("obj2").is_none());
    }

    #[test]
    fn places_outside_the_shown_options_go_to_the_mouse() {
        assert_eq!(answer_vals(0, 1), Some(vec![0, 1]));
        assert_eq!(answer_vals(3, 3), None);
        assert_eq!(answer_vals(0, 0), None);
        assert_eq!(answer_vals(0, 9), None);
    }

    #[test]
    fn an_unavailable_option_is_a_refusal_and_a_count_mismatch_the_mouse() {
        let refused = json!({"ok":true,"posted":0,"objects":1,"per_object":[{"result":"refused_by_game_check"}]});
        assert!(matches!(routed(refused, "x"), Routed::Refused(_)));
        let mismatch = json!({"ok":true,"posted":0,"objects":1,"per_object":[{"result":"second_object_invalid"}]});
        assert!(matches!(routed(mismatch, "x"), Routed::Gui));
        let done = json!({"ok":true,"posted":1,"objects":1,"executed":1,"per_object":[{"result":"posted"}]});
        assert!(matches!(routed(done, "x"), Routed::Done(_)));
    }
}
