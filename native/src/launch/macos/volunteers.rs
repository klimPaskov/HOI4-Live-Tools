//! Volunteers and the attaché through the in-process module's diplomacy family (engineer 118; one diplomacy tool on the
//! owner's rule, engineer 120: hoi4_diplomacy_act's send_volunteers / recall_volunteers keys come here): the diplomacy view's
//! own actions, built as the view builds them (sender = the player, receiver = the named country, the game date) with
//! the action class's own constructor, checked by the command's own "can do" (the action's legality: the gates the view
//! greys its button by, among them the volunteer cap, the divisions-required floor and the recall lock) and posted.
//! send and recall carry divisions; the module writes at most the action's inline places per action. A recall posts one
//! action per group; a send posts its FIRST group only (engineer 134, s129e V3/V4: the game's check passes every action
//! of one request against the same volunteer count, and the count rises only once the receiver has taken a group): the
//! rest are named as not sent and go in the next call, checked against the new count. No dialog, no mouse path.
//! These keys run only under `HOI4_BACKEND=inprocess` until the live session S25-VOLUNTEERS passes.
use super::inproc::{self, Channel, Routed};
use super::inproc_units::{request, routed};
use super::journal;
use crate::error::{Error, Result};
use serde_json::{json, Value};

/// Set when S25-VOLUNTEERS passes live (docs/LIVE_VALIDATION_QUEUE.md); the module's IPC_LIVE_PASSED entry moves with it.
const LIVE_PASSED: bool = false;
/// The module's own limit per request (it chunks them into actions).
pub(super) const MAX_DIVISIONS: usize = 64;

/// What the tool may do.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Action {
    Send,
    Recall,
    /// engineer 125: an expeditionary force (the diplomacy view's action toward an ally at war; family diplomacy2,
    /// the catalog's send_expeditionary_force row: a division list the module fills through its own allocator)
    Expedition,
}

impl Action {
    /// The hoi4_diplomacy_act keys this module path answers (send_attache keeps the view's own row).
    pub(super) fn from_key(s: &str) -> Option<Self> {
        match s {
            "send_volunteers" => Some(Self::Send),
            "recall_volunteers" | "return_volunteers" => Some(Self::Recall),
            "expeditionary_force" | "send_expeditionary_force" => Some(Self::Expedition),
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
            Self::Expedition => "diplomacy2.send_expeditionary_force",
        }
    }
    /// The module family that serves the action (its own opt-in and live session).
    pub(super) fn family(self) -> &'static str {
        match self {
            Self::Send | Self::Recall => "diplomacy",
            Self::Expedition => "diplomacy2",
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
            "send_volunteers / recall_volunteers / expeditionary_force run through the in-process module only, and only with \
             HOI4_BACKEND=inprocess until their live sessions (S25-VOLUNTEERS, S27-DIPLOMACY-ANY) pass (docs/LIVE_VALIDATION_QUEUE.md); \
             nothing was posted",
        )),
    }
}

/// The verdict of a send or an expedition from how many of the divisions posted left our army (`posted` of the
/// `asked`; a send posts one group per call): all asked gone = reached; none gone = not yet (the receiver answers when
/// the game runs); the posted group gone but divisions left unsent = diverged, naming them for the next call; part of
/// the posted group gone = the game took fewer than posted (its cap) and dropped the rest.
pub(super) fn transfer_outcome(
    gone: usize,
    posted: usize,
    asked: usize,
    not_sent: &[String],
) -> journal::Outcome {
    if gone == 0 {
        journal::Outcome::Unknown
    } else if gone >= asked {
        journal::Outcome::Reached(json!({"divisions_left_our_army": gone}))
    } else if gone >= posted {
        journal::Outcome::Diverged(json!({"divisions_left_our_army": gone, "not_sent": not_sent,
            "why": "one group per call: the game counts volunteers once the receiver has taken them, so the rest go in the next call (send them again by division_ids; the game's check then sees the new count and refuses them if the cap is full)"}))
    } else {
        journal::Outcome::Diverged(json!({"divisions_left_our_army": gone, "not_taken": posted - gone,
            "not_sent": not_sent,
            "why": "the game took fewer divisions than posted (its cap); the rest stay in the army"}))
    }
}

/// How many divisions the module's one posted action carried (a send's first group), from its reply.
pub(super) fn divisions_posted(body: &Value) -> Option<usize> {
    let m = if body.get("module").is_some() {
        &body["module"]
    } else {
        body
    };
    m["divisions_posted"]
        .as_u64()
        .and_then(|n| usize::try_from(n).ok())
}

/// The game's own volunteer rules as its defines state them (NDiplomacy; a mod's later define file wins), for the
/// refusal text: the cap is (own divisions x VOLUNTEERS_PER_COUNTRY_ARMY) or (their provinces x
/// VOLUNTEERS_PER_TARGET_PROVINCE), whichever the game shows as the limit, plus send_volunteer_size modifiers; a send
/// needs VOLUNTEERS_DIVISIONS_REQUIRED divisions; a recall waits RECALL_VOLUNTEERS_AFTER_ARRIVAL_COOLDOWN days after
/// arrival and needs every volunteer division supplied; sending again waits SEND_VOLUNTEERS_AGAIN_COOLDOWN days.
pub(super) fn rules(texts: &[String]) -> Value {
    let get = |name: &str| {
        texts
            .iter()
            .rev()
            .find_map(|t| match super::army_hq::define_in(t, name) {
                Some(super::army_hq::Def::Num(v)) => Some(v),
                _ => None,
            })
    };
    json!({
        "per_own_division": get("VOLUNTEERS_PER_COUNTRY_ARMY"),
        "per_target_province": get("VOLUNTEERS_PER_TARGET_PROVINCE"),
        "divisions_required": get("VOLUNTEERS_DIVISIONS_REQUIRED"),
        "transfer_days": get("VOLUNTEERS_TRANSFER_SPEED"),
        "recall_after_arrival_days": get("RECALL_VOLUNTEERS_AFTER_ARRIVAL_COOLDOWN"),
        "send_again_after_recall_days": get("SEND_VOLUNTEERS_AGAIN_COOLDOWN"),
    })
}

/// The defines texts of a content folder, in the order the game loads them (later files win).
pub(super) fn define_texts(dir: &std::path::Path) -> Vec<String> {
    let mut files: Vec<std::path::PathBuf> = std::fs::read_dir(dir.join("common/defines"))
        .map(|r| {
            r.filter_map(|e| e.ok().map(|e| e.path()))
                .filter(|p| p.extension().is_some_and(|e| e == "lua"))
                .collect()
        })
        .unwrap_or_default();
    files.sort();
    files
        .iter()
        .filter_map(|f| std::fs::read_to_string(f).ok())
        .collect()
}

/// The refusal text of a volunteer action the game's own check refused, with the rule numbers from its defines.
pub(super) fn refusal(action: Action, rules: &Value) -> String {
    let n = |k: &str| {
        rules[k]
            .as_f64()
            .map_or_else(|| "?".to_string(), |v| format!("{v}"))
    };
    match action {
        Action::Send => format!(
            "the game's own check refused the volunteers: the cap is full or the floor is not met (cap = own divisions x {} or their provinces x {}, plus send_volunteer_size; at least {} own divisions; not after a recall within {} days; the receiver at war and not our ally); nothing was posted",
            n("per_own_division"), n("per_target_province"), n("divisions_required"), n("send_again_after_recall_days")
        ),
        Action::Recall => format!(
            "the game's own check refused the recall: volunteers return only {} days after they arrived ({} days in transit before that) and only when every volunteer division has a supply connection; nothing was posted",
            n("recall_after_arrival_days"), n("transfer_days")
        ),
        Action::Expedition => "the game's own check refused the expeditionary force (the ally at war, the divisions eligible); nothing was posted".to_string(),
    }
}

/// A country tag as the game writes it (three upper-case letters or digits), or `?<index>` for a
/// country created during the game, as the readers print it.
pub(super) fn valid_tag(tag: &str) -> bool {
    (tag.len() == 3
        && tag
            .bytes()
            .all(|b| b.is_ascii_uppercase() || b.is_ascii_digit()))
        || super::diplomacy_state::dynamic_index(tag).is_some()
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
    // engineer 134: a send posts its first group and skips the rest (named by the caller as not sent)
    if action == Action::Send && first_group_posted(&reply) {
        return Ok(json!({"backend":"inprocess","module":reply}));
    }
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

/// A send's reply where the first action was posted and every later one skipped (nothing refused, nothing unexplained).
fn first_group_posted(reply: &Value) -> bool {
    let objs = reply["per_object"].as_array();
    reply["posted"].as_i64() == Some(1)
        && reply["refused"].as_i64() == Some(0)
        && objs.is_some_and(|o| {
            o.first().is_some_and(|x| x["result"] == "posted")
                && o.iter().skip(1).all(|x| x["result"] == "skipped")
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_send_posts_its_first_group_and_names_the_rest() {
        let reply = json!({"posted":1,"refused":0,"objects":2,"places":2,"divisions_posted":2,
            "per_object":[{"obj":"0x0","result":"posted"},{"obj":"0x0","result":"skipped"}]});
        assert!(first_group_posted(&reply));
        assert_eq!(divisions_posted(&json!({"module":reply})), Some(2));
        let refused = json!({"posted":0,"refused":1,"per_object":[{"result":"refused_by_game_check"}]});
        assert!(!first_group_posted(&refused));
        // s129e: 3 asked, the first group of 2 left, 1 named for the next call
        let not_sent = vec!["00000010:00000000".to_string()];
        match transfer_outcome(2, 2, 3, &not_sent) {
            journal::Outcome::Diverged(v) => assert_eq!(v["not_sent"][0], "00000010:00000000"),
            _ => panic!("expected diverged"),
        }
        // the cap took 1 of the 2 posted
        match transfer_outcome(1, 2, 2, &[]) {
            journal::Outcome::Diverged(v) => assert_eq!(v["not_taken"], 1),
            _ => panic!("expected diverged"),
        }
    }

    #[test]
    fn the_rules_come_from_the_defines_and_a_later_file_wins() {
        let base = "NDiplomacy = {\n\tVOLUNTEERS_PER_COUNTRY_ARMY = 0.05,\n\tRECALL_VOLUNTEERS_AFTER_ARRIVAL_COOLDOWN = 180, -- days\n}".to_string();
        let modded = "NDefines.NDiplomacy.RECALL_VOLUNTEERS_AFTER_ARRIVAL_COOLDOWN = 90".to_string();
        let r = rules(&[base, modded]);
        assert_eq!(r["per_own_division"], 0.05);
        assert_eq!(r["recall_after_arrival_days"], 90.0);
        assert!(refusal(Action::Recall, &r).contains("90 days after they arrived"));
        assert!(refusal(Action::Send, &r).contains("own divisions x 0.05"));
    }

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
        // a country created during the game, as the readers print it (s129e: the Spanish Republic)
        assert!(valid_tag("?440") && !valid_tag("?") && !valid_tag("?44x") && !valid_tag("?0"));
        // s129e V3: 3 asked, the cap took 2
        assert!(matches!(
            transfer_outcome(2, 3, 3, &[]),
            journal::Outcome::Diverged(_)
        ));
        assert!(matches!(
            transfer_outcome(3, 3, 3, &[]),
            journal::Outcome::Reached(_)
        ));
        assert!(matches!(
            transfer_outcome(0, 3, 3, &[]),
            journal::Outcome::Unknown
        ));
        assert_eq!(Action::from_key("send_volunteers"), Some(Action::Send));
        assert_eq!(Action::from_key("recall_volunteers"), Some(Action::Recall));
        assert_eq!(Action::from_key("send_attache"), None);
        assert_eq!(Action::from_key("guarantee"), None);
    }
}
