//! Division moves through the in-process module (engineer 105, family `move`): the command a right click on a
//! province posts for each selected division (CMoveCommand), built by the game's own constructor and checked by the
//! game's own "can do" before it is posted (a refusal is deleted, never posted). hoi4_army_move keeps its name, schema
//! and Verified read-back (the divisions' headings); the mouse path stays the fallback in the same call. The family is
//! off under `HOI4_BACKEND=auto` until its live session (S20-MOVE) passes.
use super::inproc::{game_refused, Channel, Routed};
use super::inproc_units::request;
use serde_json::{json, Value};
use std::collections::BTreeMap;

/// The module's per-request object limit (one command per division).
const MAX_OBJS: usize = 32;

/// The province a state order goes to by command, chosen as the mouse path aims: the state's port first (where a sea
/// transfer lands), else its largest victory point (the state's city), else its first province.
pub(super) fn state_target(
    provinces: &[i32],
    ports: &[i32],
    victory: &[(i32, f64)],
) -> Option<i32> {
    if let Some(p) = ports.iter().find(|p| provinces.contains(p)) {
        return Some(*p);
    }
    victory
        .iter()
        .filter(|(p, _)| provinces.contains(p))
        .max_by(|a, b| a.1.total_cmp(&b.1).then(b.0.cmp(&a.0)))
        .map(|(p, _)| *p)
        .or_else(|| provinces.first().copied())
}

/// The divisions an order still has to reach, in the order given: neither marching on `to` (or on another of the
/// `wanted` provinces) nor standing in one of them (the target province, or every province of the target state).
pub(super) fn pending(
    ids: &[String],
    headings: &BTreeMap<String, i32>,
    at: &BTreeMap<String, Option<i32>>,
    to: i32,
    wanted: &[i32],
) -> Vec<String> {
    ids.iter()
        .filter(|d| {
            !headings
                .get(*d)
                .is_some_and(|h| *h == to || wanted.contains(h))
        })
        .filter(|d| {
            !at.get(*d)
                .copied()
                .flatten()
                .is_some_and(|p| wanted.contains(&p))
        })
        .cloned()
        .collect()
}

/// The answer of the posts for one order: Done when any division's command was posted (the read-back judges the
/// whole order, and a mouse pass after it would order the posted divisions twice), Refused when the game's own check
/// refused every division and nothing was posted, Gui otherwise (nothing posted: the mouse path decides).
pub(super) fn routed(replies: Vec<Value>, what: &str) -> Routed {
    let posted: i64 = replies.iter().filter_map(|r| r["posted"].as_i64()).sum();
    if posted > 0 {
        return Routed::Done(json!({"backend":"inprocess","module":replies}));
    }
    if !replies.is_empty() && replies.iter().all(game_refused) {
        return Routed::Refused(what.to_string());
    }
    Routed::Gui
}

/// Each division ordered to the province, one command each, up to 32 divisions per request.
pub(super) fn move_divisions(ch: &mut Channel, divs: &[u64], province: i32) -> Routed {
    if divs.is_empty() || province < 1 {
        return Routed::Gui;
    }
    let mut replies = Vec::new();
    for part in divs.chunks(MAX_OBJS) {
        let posted_before = replies
            .iter()
            .any(|r: &Value| r["posted"].as_i64().unwrap_or(0) > 0);
        match request("move.division", part, &[], &[i64::from(province)], None)
            .and_then(|r| ch.post_raw(r))
        {
            Ok(reply) => replies.push(reply),
            // a later request failed after earlier divisions moved: the read-back judges the whole (never re-sent)
            Err(_) if posted_before => break,
            Err(_) => return Routed::Gui,
        }
    }
    routed(replies, "this move to this province")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_move_request_names_the_divisions_and_one_province() {
        let r = request("move.division", &[0x10, 0x20], &[], &[1234], None).unwrap();
        assert_eq!(r["cmd"], "move.division");
        assert_eq!(r["objs"], "0x10,0x20");
        assert_eq!(r["vals"], "1234");
        assert!(r.get("divs").is_none() && r.get("obj2").is_none());
    }

    #[test]
    fn a_state_order_aims_at_the_port_then_the_largest_victory_point_then_the_first_province() {
        let provinces = [11, 12, 13];
        assert_eq!(state_target(&provinces, &[99, 13], &[(12, 5.0)]), Some(13));
        assert_eq!(
            state_target(&provinces, &[99], &[(12, 5.0), (11, 10.0), (77, 50.0)]),
            Some(11)
        );
        assert_eq!(
            state_target(&provinces, &[], &[(12, 3.0), (11, 3.0)]),
            Some(11)
        );
        assert_eq!(state_target(&provinces, &[], &[]), Some(11));
        assert_eq!(state_target(&[], &[1], &[(1, 1.0)]), None);
    }

    #[test]
    fn only_divisions_neither_marching_on_the_target_nor_standing_there_are_ordered() {
        let ids: Vec<String> = ["a", "b", "c", "d"].iter().map(|s| s.to_string()).collect();
        let headings = BTreeMap::from([("a".to_string(), 7), ("b".to_string(), 3)]);
        let at = BTreeMap::from([
            ("a".to_string(), Some(1)),
            ("b".to_string(), Some(1)),
            ("c".to_string(), Some(8)),
            ("d".to_string(), None),
        ]);
        assert_eq!(pending(&ids, &headings, &at, 7, &[7, 8]), vec!["b", "d"]);
    }

    #[test]
    fn any_posted_division_settles_in_process_and_a_full_game_refusal_is_the_answer() {
        let posted = json!({"ok":false,"posted":2,"objects":3,"per_object":[{"result":"posted"},{"result":"posted"},{"result":"refused_by_game_check"}]});
        assert!(matches!(routed(vec![posted], "x"), Routed::Done(_)));
        let refused = json!({"ok":false,"posted":0,"objects":2,"per_object":[{"result":"refused_by_game_check"},{"result":"refused_by_game_check"}]});
        assert!(matches!(
            routed(vec![refused.clone()], "x"),
            Routed::Refused(_)
        ));
        let module = json!({"ok":false,"posted":0,"objects":1,"per_object":[{"result":"not_own"}]});
        assert!(matches!(routed(vec![refused, module], "x"), Routed::Gui));
        let off = json!({"ok":false,"error":"family move off: posting off"});
        assert!(matches!(routed(vec![off], "x"), Routed::Gui));
        assert!(matches!(routed(vec![], "x"), Routed::Gui));
    }
}
