//! Generated Mac-only layout facts, exercised on every build host.
use super::*;
use crate::{
    memory::StringAbi,
    own_focus::assignment_fixture::{Fixture, BASE, COUNTRY, GS, IDLER},
};

const PRODUCTION: u64 = 0x900000;
const REGISTRY: u64 = 0x910000;
const MANAGER: u64 = 0x920000;
const FIND_OBJECT: u64 = 0x930000;
const WINDOW: u64 = 0x940000;
const FIRST_ROW: u64 = 0x950000;
const ENTRIES: u64 = 0x960000;
const CAMERA: u64 = 0x970000;
const STATE: u64 = 0x980000;
const TEMPLATE: u64 = 0x990000;

fn floats(f: &mut Fixture, address: u64, values: &[f32]) {
    for (i, value) in values.iter().enumerate() {
        f.put(address + i as u64 * 4, &value.to_le_bytes());
    }
}

fn fixture() -> Fixture {
    let mut f = Fixture::new(StringAbi::LibCxx);
    for (field, value) in [
        (COUNTRY + 0xd10, PRODUCTION),
        (PRODUCTION, BASE + 0x338b678),
        (PRODUCTION + 0x290, COUNTRY),
        (IDLER + 0x4c0, REGISTRY),
        (REGISTRY, BASE + 0x34974d0),
        (IDLER + 0x670, MANAGER),
        (MANAGER + 0x180, IDLER),
        (MANAGER + 0x190, FIND_OBJECT),
        (FIND_OBJECT, BASE + FIND),
        (FIND_OBJECT + 0xc30, IDLER),
        (FIND_OBJECT + 0xc38, WINDOW),
        (WINDOW, BASE + 0x34ac0b0),
        (WINDOW + 0x88, REGISTRY),
        (FIND_OBJECT + 0xc88, FIRST_ROW),
        (FIRST_ROW, BASE + ROW),
        (FIRST_ROW + 0x3f8, STATE),
        (IDLER + 0x708, CAMERA),
        (STATE, BASE + 0x337caf0),
        (STATE + 0x20, TEMPLATE),
        (TEMPLATE, BASE + STATE_TEMPLATE),
    ] {
        f.word(field, value);
    }
    f.put(WINDOW + 0x95, &[8]);
    f.int(WINDOW + 0x12b8, 0);
    f.int(WINDOW + 0x1270, 0);
    f.string(FIND_OBJECT + 0xc48, "1");
    f.word(FIND_OBJECT + 0xc90, ENTRIES);
    f.int(FIND_OBJECT + 0xc98, 1);
    f.int(FIND_OBJECT + 0xc9c, 1);
    f.put(ENTRIES, &[0; ENTRY_BYTES]);
    f.word(ENTRIES + 0x30, STATE);
    f.int(ENTRIES + 0x38, 2);
    f.int(FIRST_ROW + 0x408, 2);
    f.array(GS + 0x290, &[0, STATE]);
    f.int(STATE + 0x40, 1);
    f.int(STATE + 0xc0, 1);
    f.int(STATE + 0xc4, 1);
    f.int(TEMPLATE + 0x120, 3120);
    f.int(TEMPLATE + 0x124, 1480);
    floats(
        &mut f,
        CAMERA + 0x698,
        &[3120.0, 300.0, 1390.0, 3120.0, 9.5, 1480.0],
    );
    f
}

#[test]
fn mac_row_and_own_anchor_join_without_linux_offsets() {
    let f = fixture();
    let value = snapshot(&f, Some(1)).unwrap();
    assert_eq!(value["find_view"]["query"], "1");
    assert_eq!(value["find_view"]["results"]["first_row"]["state_id"], 1);
    assert_eq!(value["camera"]["look_at"], json!([3120.0, 9.5, 1480.0]));
    assert_eq!(value["target"]["camera_centered"], true);
    for key in [
        "input_authorized",
        "placement_eligibility_verified",
        "effective_routing_verified",
        "current_frame_verified",
        "observation_paused_game",
    ] {
        assert_eq!(value[key], false);
    }
    for address in [
        IDLER + 0x750,
        FIND_OBJECT + 0x878,
        FIND_OBJECT + 0x8c8,
        TEMPLATE + 0x140,
    ] {
        assert!(f.untouched(address, 8));
    }
}

#[test]
fn hidden_and_unmaterialized_find_skip_retained_query_and_rows() {
    for materialized in [false, true] {
        let mut f = fixture();
        if materialized {
            f.put(WINDOW + 0x95, &[0]);
        } else {
            f.word(FIND_OBJECT + 0xc38, 0);
        }
        f.word(FIND_OBJECT + 0xc88, 0xdead0000);
        let value = snapshot(&f, None).unwrap();
        assert_eq!(value["find_view"]["materialized"], materialized);
        assert!(value["find_view"]["query"].is_null());
        assert!(value["find_view"]["results"].is_null());
        assert!(f.untouched(FIND_OBJECT + 0xc48, 24));
        assert!(f.untouched(FIND_OBJECT + 0xc88, 8));
        assert!(f.untouched(0xdead0000, 8));
    }
}

#[test]
fn foreign_target_exposes_no_template_anchor_payload() {
    let mut f = fixture();
    f.int(STATE + 0xc0, 7);
    let value = snapshot(&f, Some(1)).unwrap();
    assert_eq!(value["target"]["own_owned_and_controlled"], false);
    assert!(value["target"]["anchor"].is_null());
    assert!(f.untouched(STATE + 0x20, 8));
    assert!(f.untouched(TEMPLATE, 0x128));
}

#[test]
fn first_result_kind_and_object_disagreement_refuse_as_refresh() {
    for (address, value) in [(ENTRIES + 0x38, 1), (ENTRIES + 0x30, STATE + 8)] {
        let mut f = fixture();
        if address == ENTRIES + 0x30 {
            f.word(address, value);
        } else {
            f.int(address, value as i32);
        }
        assert_eq!(snapshot(&f, None).unwrap_err().code, "torn_read");
    }
}

#[test]
fn visible_empty_results_skip_the_retained_old_row() {
    let mut f = fixture();
    f.int(FIND_OBJECT + 0xc9c, 0);
    f.word(FIND_OBJECT + 0xc88, 0xdead0000);
    let value = snapshot(&f, None).unwrap();
    assert_eq!(value["find_view"]["results"]["result_count"], 0);
    assert!(value["find_view"]["results"]["first_row"].is_null());
    assert!(f.untouched(FIND_OBJECT + 0xc88, 8));
    assert!(f.untouched(0xdead0000, 8));
}

#[test]
fn other_result_kinds_do_not_follow_their_object_payload() {
    for (kind, field) in [(1, 0x3f0), (3, 0x400)] {
        let mut f = fixture();
        f.int(FIRST_ROW + 0x408, kind);
        f.int(ENTRIES + 0x38, kind);
        f.word(FIRST_ROW + field, 0xdead0000);
        f.word(ENTRIES + 0x30, 0xdead0000);
        let value = snapshot(&f, None).unwrap();
        assert_eq!(
            value["find_view"]["results"]["first_row"]["supported"],
            false
        );
        assert!(f.untouched(0xdead0000, 8));
    }
}

#[test]
fn camera_and_anchor_comparison_are_facts_without_delivery_authority() {
    let mut f = fixture();
    floats(&mut f, CAMERA + 0x6a4, &[3121.0, 9.5, 1480.0]);
    assert_eq!(
        snapshot(&f, Some(1)).unwrap()["target"]["camera_centered"],
        false
    );
    for v in [f32::NAN, f32::INFINITY, 1_000_001.0] {
        let mut f = fixture();
        floats(&mut f, CAMERA + 0x698, &[v]);
        assert!(snapshot(&f, Some(1)).is_err());
    }
    let mut f = fixture();
    f.int(TEMPLATE + 0x120, i32::MIN);
    assert!(snapshot(&f, Some(1)).is_err());
}

#[test]
fn stale_own_links_types_state_catalog_and_local_states_refuse() {
    for (field, value) in [
        (FIND_OBJECT, BASE + FIND + 8),
        (FIND_OBJECT + 0xc30, IDLER + 8),
        (MANAGER + 0x180, IDLER + 8),
        (WINDOW + 0x88, REGISTRY + 8),
        (STATE, BASE + 0x337caf8),
        (TEMPLATE, BASE + STATE_TEMPLATE + 8),
    ] {
        let mut f = fixture();
        f.word(field, value);
        assert!(snapshot(&f, Some(1)).is_err(), "field {field:x}");
    }
    let mut f = fixture();
    f.int(WINDOW + 0x1270, 3);
    assert!(snapshot(&f, Some(1)).is_err());
    let mut f = fixture();
    f.int(STATE + 0x40, 2);
    assert!(snapshot(&f, Some(1)).is_err());
}

#[test]
fn final_witness_and_exact_process_check_cannot_be_skipped() {
    let mut f = fixture();
    f.tear(FIRST_ROW + 0x408, 4, 2);
    assert_eq!(snapshot(&f, Some(1)).unwrap_err().code, "torn_read");
    let mut f = fixture();
    f.swap_final_identity = true;
    assert!(snapshot(&f, Some(1)).is_err());
    let mut f = fixture();
    f.fail_final = true;
    assert!(snapshot(&f, Some(1)).is_err());
}
