//! Laws (idea slots) through the in-process module (engineer 79, family `politics`), as the politics view posts them:
//! the selection item's click (1.20 0x10234a8c0) sends ReplaceIdea(own tag, the slot's current idea, the new idea)
//! when the slot holds an active idea, else AddIdea(own tag, the idea), each after the game's own "can do" (same
//! category, the old one active and removable, the new one allowed and paid for in political power). The objects come
//! from the game's own lists, not the open panel: the new idea from the idea database by key, the current one from the
//! country's ACTIVE ideas (the one of the same category, the category every law slot shows). Advisors and staff are
//! characters, not ideas: a key that is no idea goes to the mouse path in the same call. hoi4_politics_choose keeps its
//! name, schema and Verified read-back. Off under `HOI4_BACKEND=auto` until S13-POLITICS passes.
use super::inproc::{Channel, Routed};
use super::inproc_units::{request, routed};
use super::mgmt;
use crate::{
    error::{Error, Result},
    game,
    memory::Reader,
    xlayout::{fld, img, Class},
};
use serde_json::Value;

/// `TGameItemDatabase<CIdeaDatabase>::_pInstance` (macOS 1.19.4.1 key; 1.20 by its symbol in the migration map).
const IDEA_DB: u64 = 0x3703950;
/// The database's ideas (a pointer array; the serializer's lookup by token walks it from index 1).
const DB_IDEAS: u64 = 0x60;
const ACTIVE_IDEAS: u64 = 0x50;
const IDEA_KEY: u64 = 0x18;
/// The idea's category: the replace check's "same category" field (1.20 and 1.19.4.1).
const IDEA_CATEGORY: u64 = 0x908;
const IDEA_TYPE: &str = "5CIdea";
const POLITICS_TYPE: &str = "16CPoliticalStatus";
const POLITICS: Class = Class::new("CPoliticalStatus", 0x3589e48);
const IDEA: Class = Class::new("CIdea", 0x35882b8);
const DB_LIMIT: usize = 20_000;

/// The law to take and what it replaces.
#[derive(Debug, PartialEq)]
pub(super) struct Law {
    pub idea: u64,
    /// The active idea of the same category (None: the slot is empty, an AddIdea).
    pub current: Option<(u64, String)>,
}

/// The idea of `key` in the game's idea database (exact class CIdea), or None when the key is no idea (an advisor).
pub(super) fn idea_by_key(r: &Reader<'_>, image: u64, key: &str) -> Result<Option<u64>> {
    let abi = r.string_abi();
    let db = mgmt::pointer(r, image + img(abi, IDEA_DB)?)?;
    let ideas = r.pointer_array(db + DB_IDEAS)?;
    if ideas.len() > DB_LIMIT {
        return Err(Error::new("invalid_array", "Implausible idea count"));
    }
    let key_at = fld(abi, &IDEA, IDEA_KEY)?;
    for idea in ideas.into_iter().skip(1) {
        if !mgmt::is_pointer(idea) {
            continue;
        }
        if mgmt::text(r, idea + key_at, 128)?.as_deref() == Some(key) {
            mgmt::expect_type(r, idea, IDEA_TYPE)?;
            return Ok(Some(idea));
        }
    }
    Ok(None)
}

/// The own country's active ideas (pointer, key).
pub(super) fn active(r: &Reader<'_>, image: u64) -> Result<Vec<(u64, String)>> {
    let abi = r.string_abi();
    let own = mgmt::own(r, image)?;
    let politics = mgmt::pointer(r, own.country + game::layout(r).country_politics)?;
    mgmt::expect_type(r, politics, POLITICS_TYPE)?;
    let key_at = fld(abi, &IDEA, IDEA_KEY)?;
    let mut out = Vec::new();
    for idea in r.pointer_array(politics + fld(abi, &POLITICS, ACTIVE_IDEAS)?)? {
        if !mgmt::is_pointer(idea) {
            return Err(Error::new("invalid_pointer", "Stale active idea"));
        }
        out.push((idea, mgmt::text(r, idea + key_at, 128)?.unwrap_or_default()));
    }
    Ok(out)
}

/// The law `key` and the active idea it replaces; None when `key` is no idea. Refuses an idea already active.
pub(super) fn law(r: &Reader<'_>, image: u64, key: &str) -> Result<Option<Law>> {
    let Some(idea) = idea_by_key(r, image, key)? else {
        return Ok(None);
    };
    let at = fld(r.string_abi(), &IDEA, IDEA_CATEGORY)?;
    let category = r.u64(idea + at)?;
    if !mgmt::is_pointer(category) {
        return Err(Error::new(
            "invalid_target",
            format!("Idea {key} has no category"),
        ));
    }
    let own = active(r, image)?;
    if own.iter().any(|(p, _)| *p == idea) {
        return Err(Error::new(
            "invalid_target",
            format!("{key} is already active (own active ideas)"),
        ));
    }
    let mut current = None;
    for (p, k) in own {
        if r.u64(p + at)? == category {
            current = Some((p, k));
            break;
        }
    }
    Ok(Some(Law { idea, current }))
}

/// The module's request: AddIdea(idea) or ReplaceIdea(current -> idea).
pub(super) fn law_request(law: &Law) -> Result<Value> {
    match law.current {
        Some((old, _)) => request("politics.replace", &[law.idea], &[], &[], Some(old)),
        None => request("politics.add", &[law.idea], &[], &[], None),
    }
}

/// Post the law. A game refusal is final (no PP, not allowed): the mouse path would meet the same disabled entry.
pub(super) fn take(ch: &mut Channel, law: &Law) -> Routed {
    match law_request(law).and_then(|r| ch.post_raw(r)) {
        Ok(reply) => routed(
            reply,
            "this law now (political power, the law's own conditions)",
        ),
        Err(_) => Routed::Gui,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_law_replaces_the_active_idea_of_its_category_or_is_added() {
        let r = law_request(&Law {
            idea: 0x6000_2000,
            current: Some((0x6000_1000, "civilian_economy".into())),
        })
        .unwrap();
        assert_eq!(r["cmd"], "politics.replace");
        assert_eq!(r["objs"], "0x60002000");
        assert_eq!(r["obj2"], "0x60001000");
        let r = law_request(&Law {
            idea: 0x6000_2000,
            current: None,
        })
        .unwrap();
        assert_eq!(r["cmd"], "politics.add");
        assert!(r.get("obj2").is_none());
    }
}
