//! Volunteers and the attaché through the in-process module (engineer 118, family `volunteers`): the diplomacy view's
//! own actions, built as the view builds them (sender = the player, receiver = the named country, the game date) with
//! the action class's own constructor, checked by the command's own "can do" (the action's legality: the gates the view
//! greys its button by, among them the volunteer cap, the divisions-required floor and the recall lock) and posted.
//! send and recall carry divisions; the module writes at most the action's inline places per action and posts one
//! action per group, so a request of n divisions is ceil(n / places) commands. No dialog is opened, no mouse path.
//! The family is off under `HOI4_BACKEND=auto` until its live session S24-VOLUNTEERS passes.
use super::inproc::{self, Channel, Routed};
use super::inproc_units::{request, routed};
use crate::error::{Error, Result};
use serde_json::{json, Value};

/// Set when S24-VOLUNTEERS passes live (docs/LIVE_VALIDATION_QUEUE.md); the module's IPC_LIVE_PASSED entry moves with it.
const LIVE_PASSED: bool = false;
/// The module's own limit per request (it chunks them into actions).
pub(super) const MAX_DIVISIONS: usize = 64;

/// What the tool may do.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Action {
    Send,
    Recall,
    Attache,
}

impl Action {
    pub(super) fn parse(s: &str) -> Result<Self> {
        match s {
            "send" => Ok(Self::Send),
            "recall" => Ok(Self::Recall),
            "attache" => Ok(Self::Attache),
            _ => Err(Error::new(
                "invalid_arguments",
                "action must be send, recall or attache",
            )),
        }
    }
    pub(super) fn op(self) -> &'static str {
        match self {
            Self::Send => "volunteers.send",
            Self::Recall => "volunteers.recall",
            Self::Attache => "volunteers.attache",
        }
    }
    pub(super) fn takes_divisions(self) -> bool {
        self != Self::Attache
    }
}

pub(super) fn allowed_mode(mode: inproc::Mode) -> Result<()> {
    match mode {
        inproc::Mode::InProcess => Ok(()),
        inproc::Mode::Auto if LIVE_PASSED => Ok(()),
        _ => Err(Error::new(
            "volunteers_unvalidated",
            "hoi4_volunteers runs through the in-process module only, and only with HOI4_BACKEND=inprocess until its \
             live session S24-VOLUNTEERS passes (docs/LIVE_VALIDATION_QUEUE.md); nothing was posted",
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

pub(super) fn schema() -> Value {
    json!({
        "name": "hoi4_volunteers",
        "description": "Send or recall volunteer divisions, or send an attache, by the diplomacy view's own actions through the in-process module (family volunteers; HOI4_BACKEND=inprocess until S24-VOLUNTEERS passes). The game's own legality decides (the volunteer cap from the defines, the divisions-required floor, the recall lock, war and ideology gates); a refusal posts nothing. Divisions: an army's divisions, or division_ids within it. Air volunteers are a rebase of wings to an air base of the receiver (hoi4_air_rebase), not this tool.",
        "inputSchema": {
            "type": "object",
            "properties": {
                "action": {"type": "string", "enum": ["send", "recall", "attache"]},
                "country": {"type": "string", "description": "the receiving country's tag (the country at war; for recall the country the volunteers serve in)"},
                "army": {"type": "string", "description": "send / recall: the army whose divisions go (all of them unless division_ids)"},
                "division_ids": {"type": "array", "items": {"type": "string"}, "minItems": 1, "maxItems": MAX_DIVISIONS},
                "request_id": {"type": "string"},
                "lease_token": {"type": "string"}
            },
            "required": ["action", "country", "request_id", "lease_token"],
            "additionalProperties": false
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn actions_map_to_the_module_ops() {
        assert_eq!(Action::parse("send").unwrap().op(), "volunteers.send");
        assert_eq!(Action::parse("recall").unwrap().op(), "volunteers.recall");
        assert_eq!(Action::parse("attache").unwrap().op(), "volunteers.attache");
        assert!(Action::parse("expeditionary").is_err());
        assert!(!Action::Attache.takes_divisions());
    }

    #[test]
    fn requests_carry_the_tag_and_the_divisions() {
        let r = module_request(Action::Send, 17, &[0x10, 0x20]).unwrap();
        assert_eq!(r["cmd"], "volunteers.send");
        assert_eq!(r["objs"], "0x0");
        assert_eq!(r["divs"], "0x10,0x20");
        let a = module_request(Action::Attache, 17, &[0x10]).unwrap();
        assert!(a.get("divs").is_none());
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
    fn tags_and_schema() {
        assert!(valid_tag("SPR") && valid_tag("D01"));
        assert!(!valid_tag("spr") && !valid_tag("SPRR"));
        let s = schema();
        assert_eq!(s["name"], "hoi4_volunteers");
        assert_eq!(
            s["inputSchema"]["properties"]["action"]["enum"][2],
            "attache"
        );
    }
}
