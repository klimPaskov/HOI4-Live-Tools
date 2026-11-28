//! Diplomacy through the in-process module (engineer 78, family `diplomacy`): the diplomacy view's actions as the
//! view sends them: the action object (sender = the player, receiver = the target, the game date; declare war: one of
//! the player's own war goals against the target, as the declare-war dialog lists them) is built with the action
//! class's own constructor, wrapped by the game's command maker, checked by the command's own "can do" (the action's
//! legality, the same call that enables the view's Send button) and posted. No view is opened. hoi4_diplomacy_act keeps
//! its name, schema and Verified read-back (the relation snapshot); the mouse path stays the fallback in the same call.
//! The family is off under `HOI4_BACKEND=auto` until its live session (S10-DIPLOMACY) passes.
use super::inproc::{Channel, Routed};
use super::inproc_units::{request, routed};

/// The module op for a diplomacy key (diplomacy_keys::Spec::key); None = the mouse path (the action needs choices the
/// module does not make: a war goal type and states, a faction, equipment, volunteers).
pub(super) fn op_of(key: &str) -> Option<&'static str> {
    Some(match key {
        "guarantee" => "diplomacy.guarantee",
        "ask_military_access" => "diplomacy.military_access",
        "offer_military_access" => "diplomacy.offer_military_access",
        "non_aggression_pact" => "diplomacy.non_aggression",
        "improve_relations" => "diplomacy.improve_relations",
        "declare_war" => "diplomacy.declare_war",
        _ => return None,
    })
}

/// One action against the country with this tag index (the game's own number for the tag, as the diplomacy view shows
/// it). The module names no object (objs = 0); the target travels as the one value.
pub(super) fn act(ch: &mut Channel, op: &str, tag_index: i32) -> Routed {
    if tag_index <= 0 {
        return Routed::Gui;
    }
    match request(op, &[0], &[], &[i64::from(tag_index)], None).and_then(|r| ch.post_raw(r)) {
        Ok(reply) => routed(reply, "this diplomatic action now"),
        Err(_) => Routed::Gui,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_simple_actions_and_declare_war_have_an_op_and_dialog_actions_do_not() {
        assert_eq!(op_of("guarantee"), Some("diplomacy.guarantee"));
        assert_eq!(op_of("declare_war"), Some("diplomacy.declare_war"));
        assert_eq!(
            op_of("non_aggression_pact"),
            Some("diplomacy.non_aggression")
        );
        for k in [
            "justify_war_goal",
            "join_faction",
            "start_lend_lease",
            "send_volunteers",
            "call_allies",
        ] {
            assert_eq!(
                op_of(k),
                None,
                "{k} needs the dialog's choices: the mouse path"
            );
        }
    }

    #[test]
    fn a_diplomacy_request_names_no_object_and_carries_the_target_tag() {
        let r = request("diplomacy.guarantee", &[0], &[], &[10], None).unwrap();
        assert_eq!(r["cmd"], "diplomacy.guarantee");
        assert_eq!(r["objs"], "0x0");
        assert_eq!(r["vals"], "10");
        assert!(r.get("obj2").is_none() && r.get("divs").is_none());
    }
}
