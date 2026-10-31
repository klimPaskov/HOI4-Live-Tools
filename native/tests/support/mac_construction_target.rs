use super::*;
use hoi4_live_native::construction::target_snapshot::snapshot as target;
use serde_json::json;
const PRODUCTION: u64 = 0x300000;
const BUILDING_DB: u64 = 0x310000;
const BUILDING_TEMPLATE: u64 = 0x320000;
const STATE: u64 = 0x330000;
const BUILDING: u64 = 0x340000;
const LINE: u64 = 0x350000;
const NULL_TEMPLATE: u64 = 0x31f000;

pub(super) fn construction() -> MacFixture {
    let mut f = MacFixture::new();
    f.int(COUNTRY + 8, 1);
    for (a, v) in [
        (COUNTRY + 0xd10, PRODUCTION),
        (PRODUCTION, BASE + 0x358e0c8),
        (PRODUCTION + 0x2a8, COUNTRY),
        (BASE + 0x3703ad0, BUILDING_DB),
        (BUILDING_DB, BASE + 0x352eea8),
        (BUILDING_TEMPLATE, BASE + 0x352fa88),
        (STATE, BASE + 0x357f410),
        (BUILDING, BASE + 0x352ee30),
        (BUILDING + 0x1a8, STATE + 0x110),
        (BUILDING + 0x1b0, BUILDING_TEMPLATE),
        (STATE + 0x110, BASE + 0x352f828),
        (LINE, BASE + 0x358c690),
        (LINE + 8, 0x10000003b),
        (LINE + 0x20, PRODUCTION),
        (LINE + 0x28, 100000),
        (LINE + 0x30, 100),
        (LINE + 0x38, 0),
        (LINE + 0x78, BUILDING),
    ] {
        f.word(a, v);
    }
    f.array(PRODUCTION + 0x70, 0x360000, &[LINE]);
    f.array(
        BUILDING_DB + 0x40,
        0x370000,
        &[NULL_TEMPLATE, BUILDING_TEMPLATE],
    );
    f.word(NULL_TEMPLATE, BASE + 0x352f078);
    f.put(NULL_TEMPLATE + 0x18, &[0]);
    f.put(BUILDING_TEMPLATE + 0x18, &[1]);
    f.string(BUILDING_TEMPLATE + 0x228, "arms_factory");
    f.int(BUILDING_TEMPLATE + 0x290, 1);
    f.array(GS + 0x290, 0x380000, &[0, STATE]);
    for (a, v) in [
        (STATE + 0x40, 1),
        (STATE + 0xc0, 1),
        (STATE + 0xc4, 1),
        (STATE + 0x178, 2),
        (STATE + 0x17c, 1),
        (STATE + 0x180, 0),
        (LINE + 0x14, 1),
        (LINE + 0x18, 0),
        (LINE + 0x40, 1),
        (LINE + 0x44, 0),
        (LINE + 0x48, 2),
        (LINE + 0x80, 0),
    ] {
        f.int(a, v);
    }
    f.array(STATE + 0x130, 0x390000, &[]);
    f.word(STATE + 0x130, 0x390000);
    f.int(STATE + 0x138, 2);
    f.int(STATE + 0x13c, 2);
    f.int(0x390004, 0);
    f.array(STATE + 0x148, 0x3a0000, &[BUILDING]);
    f.put(LINE + 0x94, &[0]);
    f.put(LINE + 0xa1, &[0]);
    f
}
#[test]
fn mac_construction_target_uses_only_independent_mac_layout_and_libcxx() {
    let f = construction();
    let v = target(&f, "arms_factory", 1).unwrap();
    assert_eq!(v["queued"], json!({"in_state":2,"elsewhere":0}));
    assert_eq!(v["player"], "GER");
    assert_eq!(v["complete"], true);
    f.assert_no_foreign_payload();
    assert!(!f
        .reads
        .borrow()
        .iter()
        .any(|(a, _)| *a == BASE + 0x439f400 || *a == IDLER + 0x670));
}

#[test]
fn mac_target_false_map_option_preserves_default_fields_and_avoids_camera_reads() {
    let f = construction(); // No map/camera bytes exist in this fixture.
    let mut default = target(&f, "arms_factory", 1).unwrap();
    let mut explicit = hoi4_live_native::construction::target_snapshot::snapshot_with_map_point(
        &f,
        "arms_factory",
        1,
        false,
    )
    .unwrap();
    assert_eq!(default.get("map_point"), None);
    assert_eq!(explicit.get("map_point"), None);
    for field in ["read_calls", "read_bytes"] {
        assert_eq!(default["freshness"][field], explicit["freshness"][field]);
    }
    default.as_object_mut().unwrap().remove("freshness");
    explicit.as_object_mut().unwrap().remove("freshness");
    assert_eq!(default, explicit);
    assert!(!f
        .reads
        .borrow()
        .iter()
        .any(|(a, _)| { *a == BASE + 0x372b328 || *a == BASE + 0x3844d08 || *a == IDLER + 0x718 }));
}
#[test]
fn mac_construction_target_empty_queue_still_validates_target_and_does_not_read_ui() {
    let mut f = construction();
    f.array(PRODUCTION + 0x70, 0, &[]);
    let v = target(&f, "arms_factory", 1).unwrap();
    assert_eq!(v["queued"], json!({"in_state":0,"elsewhere":0}));
    assert_eq!(
        target(&f, "unknown_key", 1).unwrap_err().code,
        "construction_target_unavailable"
    );
    assert!(!f.reads.borrow().iter().any(|(a, _)| *a == LINE));
}
#[test]
fn mac_construction_target_foreign_control_and_torn_scope_never_prove_counts() {
    let mut f = construction();
    f.int(STATE + 0xc4, 2);
    assert_eq!(
        target(&f, "arms_factory", 1).unwrap_err().code,
        "construction_target_unavailable"
    );
    assert!(!f
        .reads
        .borrow()
        .iter()
        .any(|(a, _)| *a == STATE + 0x40 || *a == BUILDING_DB || *a == LINE));
    for field in [
        PRODUCTION + 0x70,
        BUILDING_DB + 0x40,
        STATE + 0xc4,
        LINE + 0x48,
        COUNTRY + 0xd10,
    ] {
        let f = construction();
        f.final_fault.set(Some(field));
        assert_eq!(
            target(&f, "arms_factory", 1).unwrap_err().code,
            "torn_read",
            "{field:x}"
        );
    }
}
#[test]
fn mac_construction_target_unavailable_building_is_not_zero() {
    let mut f = construction();
    f.word(LINE + 0x78, 0);
    let v = target(&f, "arms_factory", 1).unwrap();
    assert_eq!(v["complete"], false);
    assert!(v["queued"].is_null());
}

#[test]
fn mac_construction_target_null_template_requires_exact_inactive_slot_zero() {
    for (address, bytes) in [
        (NULL_TEMPLATE, (BASE + 0x352f080).to_le_bytes().to_vec()),
        (NULL_TEMPLATE + 0x18, vec![1]),
        (BUILDING_TEMPLATE, (BASE + 0x352f078).to_le_bytes().to_vec()),
    ] {
        let mut f = construction();
        f.put(address, &bytes);
        assert_eq!(
            target(&f, "arms_factory", 1).unwrap_err().code,
            "invalid_construction"
        );
        assert!(!f
            .reads
            .borrow()
            .iter()
            .any(|(a, _)| *a == NULL_TEMPLATE + 0x228 || *a == NULL_TEMPLATE + 0x290));
    }
}
