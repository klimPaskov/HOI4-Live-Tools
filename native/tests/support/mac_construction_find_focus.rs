use super::*;
use crate::{
    memory::StringAbi,
    own_focus::assignment_fixture::{Fixture, BASE},
};

const FIND: u64 = 0x900000;
const ROOT: u64 = 0x910000;
const GUI: u64 = 0x920000;
const BOX: u64 = 0x930000;
const KIND: u64 = 0x940000;
const OTHER: u64 = 0x950000;

fn fixture() -> Fixture {
    let mut f = Fixture::new(StringAbi::LibCxx);
    for (at, value) in [
        (BASE + 0x35dc058, GUI),
        (FIND + 0xc40, BOX),
        (ROOT, BASE + 0x34ac0b0),
        (ROOT + 0x88, GUI),
        (BOX, BASE + EDIT_BOX),
        (BOX + 0x180, BASE + TEXT_RECEIVER),
        (BOX + 0x58, GUI),
        (BOX + 0x50, KIND),
        (KIND, BASE + 0x34ae140),
        (GUI + 0x138, BOX),
        (GUI + 0x210, BOX + 0x180),
    ] {
        f.word(at, value);
    }
    f.put(ROOT + 0x95, &[8]);
    f.put(BOX + 0x64, &0x804u32.to_le_bytes());
    f.put(BOX + 0x239, &[1]);
    f.string(KIND + 0x28, "to_find_text");
    f.array(ROOT + 0xd30, &[BOX]);
    f
}

fn read(f: &Fixture, root: u64) -> Result<Option<Focus>> {
    observe(&Reader::new(f), BASE, FIND, root, GUI)
}

#[test]
fn exact_mac_edit_box_and_both_aliases_are_required_for_focus() {
    let f = fixture();
    let focus = read(&f, ROOT).unwrap().unwrap();
    assert!(focus.focused());
    assert_eq!(focus.summary()["delivery_verified"], false);
    assert_eq!(focus.summary()["input_authorized"], false);
    assert!(f.untouched(BOX + 0x1a0, 8));
    assert!(f.untouched(GUI + 0x158, 8));
    assert!(f.untouched(GUI + 0x230, 8));
}

#[test]
fn one_matching_alias_or_linux_text_alias_never_proves_focus() {
    for (at, value) in [
        (GUI + 0x138, 0),
        (GUI + 0x138, OTHER),
        (GUI + 0x210, 0),
        (GUI + 0x210, BOX),
        (GUI + 0x210, BOX + 0x1a0),
        (GUI + 0x210, OTHER),
    ] {
        let mut f = fixture();
        f.word(at, value);
        assert!(!read(&f, ROOT).unwrap().unwrap().focused());
        assert!(f.untouched(OTHER, 8));
    }
}

#[test]
fn hidden_and_unmaterialized_find_skip_edit_box_and_alias_payloads() {
    for root in [0, ROOT] {
        let mut f = fixture();
        f.put(ROOT + 0x95, &[0]);
        f.word(FIND + 0xc40, OTHER);
        assert!(read(&f, root).unwrap().is_none());
        assert!(f.untouched(FIND + 0xc40, 8));
        assert!(f.untouched(GUI + 0x138, 8));
        assert!(f.untouched(GUI + 0x210, 8));
        assert!(f.untouched(OTHER, 8));
    }
}

#[test]
fn absent_edit_box_stays_unknown() {
    let mut f = fixture();
    f.word(FIND + 0xc40, 0);
    assert!(read(&f, ROOT).unwrap().is_none());
    assert!(f.untouched(GUI + 0x138, 8));
}

#[test]
fn foreign_scope_type_or_receiver_refuses_before_alias_reads() {
    for (at, value) in [
        (BASE + 0x35dc058, OTHER),
        (ROOT + 0x88, OTHER),
        (BOX, BASE + EDIT_BOX + 8),
        (BOX + 0x58, OTHER),
        (BOX + 0x180, BASE + TEXT_RECEIVER + 8),
    ] {
        let mut f = fixture();
        f.word(at, value);
        assert_eq!(read(&f, ROOT).unwrap_err().code, "invalid_construction");
        assert!(f.untouched(GUI + 0x138, 8));
        assert!(f.untouched(GUI + 0x210, 8));
        assert!(f.untouched(OTHER, 8));
    }
}

#[test]
fn missing_renamed_or_ambiguous_named_child_refuses() {
    let mut f = fixture();
    f.array(ROOT + 0xd30, &[]);
    assert!(read(&f, ROOT).is_err());
    let mut f = fixture();
    f.string(KIND + 0x28, "other_text");
    assert!(read(&f, ROOT).is_err());
    let mut f = fixture();
    f.array(ROOT + 0xd30, &[BOX, BOX]);
    assert!(read(&f, ROOT).is_err());
}
