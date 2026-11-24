//! Army structure through the in-process module (engineer 75, family `units`): a new army of given divisions, divisions
//! into an existing army, a general or field marshal, a new name, armies into a new or an existing army group or out of
//! one, an empty shell removed. Each is the command the game's own control posts (the new-army card, the right click on
//! an army's card, the commander list, the name box, the army-group cards), checked by the game's own "can do" before it
//! is posted. The tools keep their names, schemas and Verified read-backs; the mouse path stays the fallback in the same
//! call. The family is off under `HOI4_BACKEND=auto` until its live session (S6-UNITS) passes.
use super::inproc::{all_posted, game_refused, Channel, Routed};
use crate::error::{Error, Result};
use serde_json::{json, Value};

/// The module's per-request object limit; a new army or a transfer carries its divisions in `divs` (up to 512).
const MAX_OBJS: usize = 32;
const MAX_DIVS: usize = 512;
/// A name travels as character codes and the module builds the game's short string form: at most 22 bytes.
pub(super) const NAME_MAX: usize = 22;

/// One `units.*` post as the module parses it: objs (hex), divs (hex), vals (integers), obj2 (hex).
pub(super) fn request(
    op: &str,
    objs: &[u64],
    divs: &[u64],
    vals: &[i64],
    obj2: Option<u64>,
) -> Result<Value> {
    if objs.is_empty() || objs.len() > MAX_OBJS {
        return Err(Error::new("invalid_arguments", "1..32 objects per request"));
    }
    if divs.len() > MAX_DIVS {
        return Err(Error::new("invalid_arguments", "at most 512 divisions per request"));
    }
    let hex = |v: &[u64]| v.iter().map(|o| format!("0x{o:x}")).collect::<Vec<_>>().join(",");
    let mut req = json!({"op":"post","cmd":op,"objs":hex(objs)});
    if !divs.is_empty() {
        req["divs"] = json!(hex(divs));
    }
    if !vals.is_empty() {
        req["vals"] = json!(vals.iter().map(i64::to_string).collect::<Vec<_>>().join(","));
    }
    if let Some(o) = obj2 {
        req["obj2"] = json!(format!("0x{o:x}"));
    }
    Ok(req)
}

/// A name as the module takes it: printable ASCII without quote or backslash, 1..22 bytes. None: the mouse path types it.
pub(super) fn name_codes(name: &str) -> Option<Vec<i64>> {
    let ok = !name.is_empty()
        && name.len() <= NAME_MAX
        && name.bytes().all(|b| (32..=126).contains(&b) && b != b'"' && b != b'\\');
    ok.then(|| name.bytes().map(i64::from).collect())
}

/// The divisions of a new army or a transfer in chunks the module takes in one request (one command each).
pub(super) fn chunks(divs: &[u64]) -> impl Iterator<Item = &[u64]> {
    divs.chunks(MAX_DIVS)
}

/// The answer of one post: Done when every object was posted (and, for a new army or a transfer, the module read the
/// divisions in their army), Refused when the game's own check refused everything, Gui otherwise (nothing that the
/// module could not explain was posted; the mouse path decides).
pub(super) fn routed(reply: Value, what: &str) -> Routed {
    if game_refused(&reply) {
        return Routed::Refused(what.to_string());
    }
    if all_posted(&reply) {
        return Routed::Done(json!({"backend":"inprocess","module":reply}));
    }
    Routed::Gui
}

fn post(ch: &mut Channel, req: Result<Value>, what: &str) -> Routed {
    match req.and_then(|r| ch.post_raw(r)) {
        Ok(reply) => routed(reply, what),
        Err(_) => Routed::Gui,
    }
}

/// A new army of these divisions (the new-army card): the first division names the theatre, as the UI reads it.
pub(super) fn new_army(ch: &mut Channel, divs: &[u64]) -> Routed {
    let Some(first) = divs.first() else {
        return Routed::Gui;
    };
    if divs.len() > MAX_DIVS {
        return Routed::Gui;
    }
    post(ch, request("units.new_army", &[*first], divs, &[], None), "a new army of these divisions")
}

/// Divisions into an existing army (the right click on its card), up to 512 per command.
pub(super) fn transfer(ch: &mut Channel, army: u64, divs: &[u64]) -> Routed {
    let mut replies = Vec::new();
    for part in chunks(divs) {
        match post(ch, request("units.transfer", &[army], part, &[], None), "these divisions into this army") {
            Routed::Done(v) => replies.push(v),
            other if replies.is_empty() => return other,
            // a later chunk failed after earlier ones moved: the settle read judges the whole (never re-sent)
            _ => break,
        }
    }
    Routed::Done(json!({"backend":"inprocess","module":replies}))
}

/// A general for an army or a field marshal for an army group (None = unassign).
pub(super) fn leader(ch: &mut Channel, group: u64, leader: Option<u64>) -> Routed {
    post(ch, request("units.leader", &[group], &[], &[], leader), "this commander for this army")
}

/// A new name for an army or army group; Gui when the name cannot travel (the mouse path types it).
pub(super) fn rename(ch: &mut Channel, group: u64, name: &str) -> Routed {
    let Some(codes) = name_codes(name) else {
        return Routed::Gui;
    };
    post(ch, request("units.rename", &[group], &[], &codes, None), "this name")
}

/// A new army group of these armies (they must share a theatre; the module checks it).
pub(super) fn army_group_new(ch: &mut Channel, armies: &[u64]) -> Routed {
    if armies.len() < 2 || armies.len() > MAX_OBJS {
        return Routed::Gui;
    }
    post(ch, request("units.army_group_new", armies, &[], &[], None), "a new army group of these armies")
}

/// Armies into an existing army group.
pub(super) fn to_army_group(ch: &mut Channel, armies: &[u64], group: u64) -> Routed {
    post(ch, request("units.to_army_group", armies, &[], &[], Some(group)), "these armies into this army group")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn requests_follow_the_module_parser() {
        let r = request("units.new_army", &[0x10], &[0x10, 0x20], &[], None).unwrap();
        assert_eq!(r["cmd"], "units.new_army");
        assert_eq!(r["objs"], "0x10");
        assert_eq!(r["divs"], "0x10,0x20");
        assert!(r.get("vals").is_none() && r.get("obj2").is_none());
        let r = request("units.leader", &[0xa], &[], &[], Some(0xb)).unwrap();
        assert_eq!(r["obj2"], "0xb");
        assert!(request("units.transfer", &[], &[1], &[], None).is_err());
        assert!(request("units.transfer", &[1], &vec![1; 513], &[], None).is_err());
    }

    #[test]
    fn names_travel_as_printable_ascii_up_to_22_bytes() {
        assert_eq!(name_codes("Armee Nord").unwrap().len(), 10);
        assert_eq!(name_codes("AB").unwrap(), vec![65, 66]);
        assert!(name_codes("").is_none());
        assert!(name_codes("Panzergruppe Kleist Sud").is_none()); // 23 bytes
        assert!(name_codes("Armee Süd").is_none()); // not ASCII: the mouse path types it
        assert!(name_codes("a\"b").is_none());
    }

    #[test]
    fn a_game_refusal_is_the_answer_and_partial_posts_go_to_the_gui() {
        let refused = json!({"ok":false,"posted":0,"objects":1,"per_object":[{"result":"refused_by_game_check"}]});
        assert!(matches!(routed(refused, "x"), Routed::Refused(_)));
        let done = json!({"ok":true,"posted":1,"objects":1,"per_object":[{"result":"posted"}],"army":"0x1"});
        assert!(matches!(routed(done, "x"), Routed::Done(_)));
        let off = json!({"ok":false,"error":"family units off: posting off"});
        assert!(matches!(routed(off, "x"), Routed::Gui));
    }

    #[test]
    fn a_split_takes_the_army_s_first_divisions_in_roster_order() {
        let view = json!({"armies":[{"army":"A","divisions":["1:1","1:2","1:3"]},{"army":"B","divisions":["2:1"]}]});
        assert_eq!(
            super::super::armies::first_divisions(&view, "A", 2).unwrap(),
            vec!["1:1".to_string(), "1:2".to_string()]
        );
        assert!(super::super::armies::first_divisions(&view, "B", 2).is_none());
        assert!(super::super::armies::first_divisions(&view, "C", 1).is_none());
    }

    #[test]
    fn divisions_split_into_module_sized_requests() {
        let v: Vec<u64> = (1..=1100).collect();
        let sizes: Vec<usize> = chunks(&v).map(<[u64]>::len).collect();
        assert_eq!(sizes, vec![512, 512, 76]);
    }
}
