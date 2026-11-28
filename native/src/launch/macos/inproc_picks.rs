//! The player's own picks through the in-process module (engineer 75, family `picks`): a national focus, posted as the
//! focus tree's start button posts it (SetNationalFocus(own tag, the focus)) after the game's own "can start" check with
//! the player's country. hoi4_focus_start keeps its name, schema and Verified read; the mouse path stays the fallback.
//! The family is off under `HOI4_BACKEND=auto` until its live session (S7-PICKS) passes.
use super::inproc::{Channel, Routed};
use super::inproc_units::{request, routed};

/// A national focus of the own tree (its address from the own-tree walk; the module checks its exact class again).
pub(super) fn focus(ch: &mut Channel, focus: u64) -> Routed {
    match request("picks.focus", &[focus], &[], &[], None).and_then(|r| ch.post_raw(r)) {
        Ok(reply) => routed(reply, "this focus now"),
        Err(_) => Routed::Gui,
    }
}

/// engineer 76 (family `decisions`): an untargeted decision of the own country, as the decisions panel's select posts
/// it: SelectDecision(own tag, the decision) after the game's own "can do" with the player's country.
pub(super) fn decision(ch: &mut Channel, decision: u64) -> Routed {
    match request("decisions.take", &[decision], &[], &[], None).and_then(|r| ch.post_raw(r)) {
        Ok(reply) => routed(reply, "this decision now"),
        Err(_) => Routed::Gui,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_focus_request_carries_one_object_and_nothing_else() {
        let r = request("picks.focus", &[0x6000_1234], &[], &[], None).unwrap();
        assert_eq!(r["cmd"], "picks.focus");
        assert_eq!(r["objs"], "0x60001234");
        assert!(r.get("vals").is_none() && r.get("divs").is_none() && r.get("obj2").is_none());
    }

    #[test]
    fn a_decision_request_carries_one_decision_object() {
        let r = request("decisions.take", &[0x6000_2000], &[], &[], None).unwrap();
        assert_eq!(r["cmd"], "decisions.take");
        assert_eq!(r["objs"], "0x60002000");
        assert!(r.get("vals").is_none() && r.get("obj2").is_none());
    }
}
