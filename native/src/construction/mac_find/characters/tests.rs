use super::*;
use crate::{
    memory::{ReadMemory, StringAbi},
    own_focus::assignment_fixture::{Fixture, BASE, FOREIGN, IDLER},
    platform::Process,
    research_ui::Witnesses,
};

const GUI: u64 = 0x900000;
const GFX: u64 = 0x910000;
const APP: u64 = 0x920000;
const PUMP: u64 = 0x930000;
const HOTKEYS: u64 = 0x940000;

fn fixture(flag: Option<u8>) -> Fixture {
    let mut f = Fixture::new(StringAbi::LibCxx);
    for (at, value) in [
        (IDLER + 0x4c0, GUI),
        (GUI, BASE + 0x36a5810),
        (BASE + 0x37eda08, GUI),
        (GUI + 0x98, GFX),
        (GFX, BASE + 0x35e5b58),
        (IDLER + 0x4f8, APP),
        (BASE + 0x384b758, APP),
        (APP + 0x38, IDLER),
        (APP + 0x300, GFX),
        (APP + 0x308, GUI),
        (BASE + 0x35089d8 + 0x20, BASE + 0x7bb2c0),
        (BASE + 0x37ea608, PUMP),
        (PUMP, BASE + 0x36b5160),
        (BASE + 0x37ea778, if flag.is_some() { HOTKEYS } else { 0 }),
    ] {
        f.word(at, value);
    }
    f.put(IDLER + 0x7e1, &[1]);
    f.put(BASE + 0x372b2b8, &[0]);
    f.put(PUMP + 0x44, &[0]);
    if let Some(flag) = flag {
        f.word(HOTKEYS, BASE + 0x36b6490);
        f.word(BASE + 0x36b6490 + 0x20, BASE + 0x2867050);
        f.put(BASE + 0x37ea780, &[flag]);
        if flag == 1 {
            f.int(HOTKEYS + 0x64, 0);
            f.word(BASE + 0x3477260, BASE + 0x370ce20);
            f.word(HOTKEYS + 8, BASE + 0x370ce20);
            f.int(HOTKEYS + 0x10, 0);
            f.int(HOTKEYS + 0x14, 0);
            f.put(HOTKEYS + 0x18, &[0]);
            f.put(BASE + 0x370ce24, &[0]);
            f.put(BASE + 0x370ce4c, &[0xff]);
        }
    }
    f
}

fn witnessed(f: &Fixture, original: Option<&Binding>) -> Result<Binding> {
    f.begin_snapshot()?;
    let w = Witnesses::new(f);
    let r = Reader::new(&w);
    let guard = original.map_or(ReadGuard::Bind, ReadGuard::Original);
    guard.before_context(&r, BASE)?;
    let current = guard.observe(&r, BASE, IDLER, GUI)?;
    w.finish()?;
    f.check()?;
    Ok(current)
}

#[test]
fn direct_and_flag0_never_read_optional_character_payload() {
    for flag in [None, Some(0)] {
        let f = fixture(flag);
        let binding = witnessed(&f, None).unwrap();
        assert_eq!(
            binding.mode,
            if flag.is_none() {
                Mode::Direct
            } else {
                Mode::Disabled
            }
        );
        witnessed(&f, Some(&binding)).unwrap();
        assert!(f.untouched(HOTKEYS + 8, 0x60));
        assert!(f.untouched(BASE + 0x3477260, 8));
        assert!(f.untouched(BASE + 0x370ce20, 80));
        if flag.is_none() {
            assert!(f.untouched(BASE + 0x37ea780, 1));
        }
    }
}

#[test]
fn exact_empty_flag1_path_is_retained_without_key_listener_or_saved_event_reads() {
    let f = fixture(Some(1));
    let b = witnessed(&f, None).unwrap();
    assert_eq!(b.forwarding_kind(), "owned_hotkeys_flag1_exact_empty_table");
    assert_eq!(witnessed(&f, Some(&b)).unwrap(), b);
    for (at, size) in [
        (BASE + 0x370ce28, 32),
        (BASE + 0x370ce50, 32),
        (HOTKEYS + 0x20, 0x44),
        (HOTKEYS + 0x68, 32),
    ] {
        assert!(f.untouched(at, size), "{at:x}/{size}");
    }
}

#[test]
fn all_empty_header_fields_are_required_before_sentinel_payload() {
    for (offset, size, value) in [(0x10, 4, 1u32), (0x14, 4, 1), (0x18, 1, 1)] {
        let mut f = fixture(Some(1));
        f.put(HOTKEYS + offset, &value.to_le_bytes()[..size]);
        assert_eq!(
            witnessed(&f, None).unwrap_err().code,
            "construction_state_changed"
        );
        assert!(f.untouched(BASE + 0x370ce20, 80));
    }
}

#[test]
fn exact_empty_sentinel_states_are_independent_required_bytes() {
    for (offset, value) in [(4, 1), (0x2c, 0), (0x2c, 0xfe)] {
        let mut f = fixture(Some(1));
        f.put(BASE + 0x370ce20 + offset, &[value]);
        assert!(witnessed(&f, None).is_err());
    }
}

#[test]
fn data_and_got_must_both_alias_exact_own_sentinel_before_any_states() {
    for changes in [
        vec![(HOTKEYS + 8, FOREIGN)],
        vec![(BASE + 0x3477260, FOREIGN)],
        vec![(HOTKEYS + 8, FOREIGN), (BASE + 0x3477260, FOREIGN)],
    ] {
        let mut f = fixture(Some(1));
        for (at, value) in changes {
            f.word(at, value);
        }
        assert!(witnessed(&f, None).is_err());
        assert!(f.untouched(FOREIGN, 80));
        assert!(f.untouched(BASE + 0x370ce20, 80));
    }
}

#[test]
fn active_capture_or_unknown_flag_refuses_before_table_payload() {
    for (flag, stage) in [(1, 1), (1, -1), (1, 2), (2, 0)] {
        let mut f = fixture(Some(flag));
        f.int(HOTKEYS + 0x64, stage);
        assert!(witnessed(&f, None).is_err());
        assert!(f.untouched(HOTKEYS + 8, 0x11));
        assert!(f.untouched(BASE + 0x3477260, 8));
    }
}

#[test]
fn malformed_override_and_unknown_class_refuse_before_optional_payload() {
    for receiver in [1, HOTKEYS + 1, FOREIGN] {
        let mut f = fixture(Some(1));
        f.word(BASE + 0x37ea778, receiver);
        if receiver == FOREIGN {
            f.word(FOREIGN, BASE + 0x34a7db0);
        }
        assert!(witnessed(&f, None).is_err());
        assert!(f.untouched(receiver + 8, 0x60));
        if receiver != FOREIGN {
            assert!(f.untouched(receiver, 8));
        }
    }
}

#[test]
fn original_override_replacement_refuses_before_new_primary_or_payload() {
    let mut f = fixture(Some(1));
    let b = witnessed(&f, None).unwrap();
    f.word(BASE + 0x37ea778, FOREIGN);
    assert!(witnessed(&f, Some(&b)).is_err());
    assert!(f.untouched(FOREIGN, 0x100));
}

#[test]
fn original_application_or_graphics_replacement_refuses_before_new_payload() {
    for field in [IDLER + 0x4f8, BASE + 0x384b758, GUI + 0x98] {
        let mut f = fixture(Some(1));
        let b = witnessed(&f, None).unwrap();
        f.word(field, FOREIGN);
        assert!(witnessed(&f, Some(&b)).is_err());
        assert!(f.untouched(FOREIGN, 0x400));
        assert!(f.untouched(HOTKEYS + 8, 0x60));
    }
}

#[test]
fn original_classes_receiver_slot_and_owner_aliases_cannot_be_replaced() {
    for field in [
        IDLER,
        GUI,
        GFX,
        APP + 0x38,
        APP + 0x300,
        APP + 0x308,
        PUMP,
        BASE + 0x36b6490 + 0x20,
    ] {
        let mut f = fixture(Some(1));
        let b = witnessed(&f, None).unwrap();
        f.word(field, FOREIGN);
        assert!(witnessed(&f, Some(&b)).is_err());
        assert!(f.untouched(FOREIGN, 8));
        assert!(f.untouched(HOTKEYS + 8, 0x60));
    }
}

#[test]
fn mode_change_is_not_adopted_and_refuses_before_new_optional_table() {
    for (before, after) in [(0, 1), (1, 0)] {
        let mut f = fixture(Some(before));
        let b = witnessed(&f, None).unwrap();
        f.put(BASE + 0x37ea780, &[after]);
        assert!(witnessed(&f, Some(&b)).is_err());
        assert!(f.untouched(HOTKEYS + 8, 0x60));
    }
}

#[test]
fn replacement_table_refuses_before_replacement_payload_on_retained_read() {
    let mut f = fixture(Some(1));
    let b = witnessed(&f, None).unwrap();
    f.word(HOTKEYS + 8, FOREIGN);
    assert!(witnessed(&f, Some(&b)).is_err());
    assert!(f.untouched(FOREIGN, 80));
    assert!(f.untouched(BASE + 0x370ce20, 80));
}

#[test]
fn ordinary_mainframe_pump_and_alternate_gates_remain_required() {
    for (at, value) in [(IDLER + 0x7e1, 0), (BASE + 0x372b2b8, 1), (PUMP + 0x44, 1)] {
        let mut f = fixture(Some(1));
        f.put(at, &[value]);
        assert!(witnessed(&f, None).is_err());
        assert!(f.untouched(HOTKEYS + 8, 0x60));
    }
}

#[test]
fn sentinel_stage_header_and_owning_edge_tears_are_discarded_by_final_witness() {
    for (at, size) in [
        (BASE + 0x370ce24, 1),
        (BASE + 0x370ce4c, 1),
        (HOTKEYS + 0x10, 4),
        (HOTKEYS + 0x64, 4),
        (APP + 0x38, 8),
    ] {
        let mut f = fixture(Some(1));
        f.tear(at, size, 2);
        assert_eq!(witnessed(&f, None).unwrap_err().code, "torn_read", "{at:x}");
    }
}

#[test]
fn bounded_base_and_non_mac_abi_refuse_without_reads() {
    for base in [0, 1, BASE + 1, u64::MAX, 0x7fff_ffff_0000] {
        let f = fixture(Some(1));
        assert!(observe(&Reader::new(&f), base, IDLER, GUI).is_err());
        assert!(f.read_counts().is_empty());
    }
    let mut f = fixture(Some(1));
    f.abi = StringAbi::LibStdCxx;
    assert_eq!(
        observe(&Reader::new(&f), BASE, IDLER, GUI)
            .unwrap_err()
            .code,
        "unsupported_platform"
    );
    assert!(f.read_counts().is_empty());
}

#[test]
fn short_required_state_is_not_treated_as_an_empty_table() {
    struct Short<'a>(&'a Fixture);
    impl ReadMemory for Short<'_> {
        fn string_abi(&self) -> StringAbi {
            StringAbi::LibCxx
        }
        fn read(&self, at: u64, size: usize) -> Result<Vec<u8>> {
            if at == BASE + 0x370ce4c {
                return Ok(Vec::new());
            }
            self.0.read(at, size)
        }
    }
    let f = fixture(Some(1));
    assert_eq!(
        observe(&Reader::new(&Short(&f)), BASE, IDLER, GUI)
            .unwrap_err()
            .code,
        "short_read"
    );
}
