//! Research through the in-process module (engineer 79, family `research`): a technology into a research slot, as the
//! research view posts it: SetResearch(own tag, the technology's template, the slot, no XP boost, 1, 0) through the
//! view's own constructor, after the game's own "can do" (the slot exists; the country can research the template now).
//! The template comes from the game's technology database (no view is opened). An exclusive pair is switched by another
//! command the module does not post: the game's check refuses it here and the mouse path serves it in the same call.
//! hoi4_research_start keeps its name, schema and Verified read-back (the slot holds the technology). The family is off
//! under `HOI4_BACKEND=auto` until its live session (S12-RESEARCH) passes.
use super::inproc::{Channel, Routed};
use super::inproc_units::{request, routed};

/// One technology template into one slot.
pub(super) fn start(ch: &mut Channel, template: u64, slot: usize) -> Routed {
    let Ok(slot) = i64::try_from(slot) else {
        return Routed::Gui;
    };
    match request("research.start", &[template], &[], &[slot], None).and_then(|r| ch.post_raw(r)) {
        // a game refusal may be an exclusive pair the view switches with another command: the mouse path decides
        Ok(reply) => match routed(reply, "this technology in this slot now") {
            Routed::Refused(_) => Routed::Gui,
            other => other,
        },
        Err(_) => Routed::Gui,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_research_request_carries_the_template_and_the_slot() {
        let r = request("research.start", &[0x6000_4000], &[], &[2], None).unwrap();
        assert_eq!(r["cmd"], "research.start");
        assert_eq!(r["objs"], "0x60004000");
        assert_eq!(r["vals"], "2");
        assert!(r.get("obj2").is_none() && r.get("divs").is_none());
    }
}
