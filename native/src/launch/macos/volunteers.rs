//! Volunteers and the attaché through the in-process module's diplomacy family (engineer 118; one diplomacy tool on the
//! owner's rule, engineer 120: hoi4_diplomacy_act's send_volunteers / recall_volunteers keys come here): the diplomacy view's
//! own actions, built as the view builds them (sender = the player, receiver = the named country, the game date) with
//! the action class's own constructor, checked by the command's own "can do" (the action's legality: the gates the view
//! greys its button by, among them the volunteer cap, the divisions-required floor and the recall lock) and posted.
//! send and recall carry divisions; the module writes at most the action's inline places per action and posts one
//! action per group, so a request of n divisions is ceil(n / places) commands. No dialog is opened, no mouse path.
//! These keys run only under `HOI4_BACKEND=inprocess` until the live session S25-VOLUNTEERS passes.
use super::inproc::{self, Channel, Routed};
use super::inproc_units::{request, routed};
use crate::error::{Error, Result};
use serde_json::Value;

/// Set when S25-VOLUNTEERS passes live (docs/LIVE_VALIDATION_QUEUE.md); the module's IPC_LIVE_PASSED entry moves with it.
const LIVE_PASSED: bool = false;
/// The module's own limit per request (it chunks them into actions).
pub(super) const MAX_DIVISIONS: usize = 64;

/// What the tool may do.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Action {
    Send,
    Recall,
}

impl Action {
    /// The hoi4_diplomacy_act keys this module path answers (send_attache keeps the view's own row).
    pub(super) fn from_key(s: &str) -> Option<Self> {
        match s {
            "send_volunteers" => Some(Self::Send),
            "recall_volunteers" | "return_volunteers" => Some(Self::Recall),
            _ => None,
        }
    }
    #[cfg(test)]
    pub(super) fn parse(s: &str) -> Result<Self> {
        match s {
            "send" => Ok(Self::Send),
            "recall" => Ok(Self::Recall),
            _ => Err(Error::new(
                "invalid_arguments",
                "action must be send or recall",
            )),
        }
    }
    pub(super) fn op(self) -> &'static str {
        match self {
            Self::Send => "diplomacy.send_volunteers",
            Self::Recall => "diplomacy.recall_volunteers",
        }
    }
    pub(super) fn takes_divisions(self) -> bool {
        true
    }
}

pub(super) fn allowed_mode(mode: inproc::Mode) -> Result<()> {
    match mode {
        inproc::Mode::InProcess => Ok(()),
        inproc::Mode::Auto if LIVE_PASSED => Ok(()),
        _ => Err(Error::new(
            "volunteers_unvalidated",
            "send_volunteers / recall_volunteers run through the in-process module only, and only with HOI4_BACKEND=inprocess until its \
             live session S25-VOLUNTEERS passes (docs/LIVE_VALIDATION_QUEUE.md); nothing was posted",
        )),
    }
}

/// A country tag as the game writes it (three upper-case letters or digits).
pub(super) fn valid_tag(tag: &str) -> bool {
    tag.len() == 3
        && tag
            .bytes()
            .all(|b| b.is_ascii_uppercase() || b.is_ascii_digit())
}

/// The module's request for one call: objs = 0 (no object), vals = the receiver's tag index, divs = the divisions.
pub(super) fn module_request(action: Action, tag_index: i32, divisions: &[u64]) -> Result<Value> {
    if tag_index <= 0 {
        return Err(Error::new(
            "invalid_target",
            "the receiver has no tag index",
        ));
    }
    if action.takes_divisions() && (divisions.is_empty() || divisions.len() > MAX_DIVISIONS) {
        return Err(Error::new(
            "invalid_arguments",
            format!("{} needs 1..{MAX_DIVISIONS} divisions", action.op()),
        ));
    }
    let divs: &[u64] = if action.takes_divisions() {
        divisions
    } else {
        &[]
    };
    request(action.op(), &[0], divs, &[i64::from(tag_index)], None)
}

/// Post one call; the module's reply on success. A refusal by the game's own check (the view would grey the button)
/// is the answer, not a reason to try another path: there is none.
pub(super) fn post(
    ch: &mut Channel,
    action: Action,
    tag_index: i32,
    divisions: &[u64],
) -> Result<Value> {
    let req = module_request(action, tag_index, divisions)?;
    let reply = ch.post_raw(req)?;
    match routed(reply.clone(), "this volunteer action now") {
        Routed::Done(v) => Ok(v),
        Routed::Refused(what) => Err(Error::new(
            "refused_by_game",
            format!("the game's own check refused {what} (cap, divisions required, recall lock, war or ideology gate); nothing was posted"),
        )),
        Routed::Gui => Err(Error::new(
            "in_process_refused",
            format!("the module did not post: {}", reply.get("reason").and_then(Value::as_str).unwrap_or("no reason given")),
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn actions_map_to_the_module_ops() {
        assert_eq!(
            Action::parse("send").unwrap().op(),
            "diplomacy.send_volunteers"
        );
        assert_eq!(
            Action::parse("recall").unwrap().op(),
            "diplomacy.recall_volunteers"
        );
        assert!(Action::parse("expeditionary").is_err());
    }

    #[test]
    fn requests_carry_the_tag_and_the_divisions() {
        let r = module_request(Action::Send, 17, &[0x10, 0x20]).unwrap();
        assert_eq!(r["cmd"], "diplomacy.send_volunteers");
        assert_eq!(r["objs"], "0x0");
        assert_eq!(r["divs"], "0x10,0x20");
        assert!(module_request(Action::Send, 17, &[]).is_err());
        assert!(module_request(Action::Recall, 0, &[0x10]).is_err());
        assert!(module_request(Action::Send, 17, &[1; MAX_DIVISIONS + 1]).is_err());
    }

    #[test]
    fn auto_mode_waits_for_the_live_session() {
        assert!(allowed_mode(inproc::Mode::InProcess).is_ok());
        assert!(allowed_mode(inproc::Mode::Auto).is_err());
        assert!(allowed_mode(inproc::Mode::Gui).is_err());
    }

    #[test]
    fn tags_and_diplomacy_keys() {
        assert!(valid_tag("SPR") && valid_tag("D01"));
        assert!(!valid_tag("spr") && !valid_tag("SPRR"));
        assert_eq!(Action::from_key("send_volunteers"), Some(Action::Send));
        assert_eq!(Action::from_key("recall_volunteers"), Some(Action::Recall));
        assert_eq!(Action::from_key("send_attache"), None);
        assert_eq!(Action::from_key("guarantee"), None);
    }
}
