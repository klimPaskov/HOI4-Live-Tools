use super::*;
use hoi4_live_native::construction::find_view::{snapshot as find_view, validate_state_id};

const REGISTRY: u64 = 0x400000;
const MANAGER: u64 = 0x410000;
const FIND: u64 = 0x420000;
const WINDOW: u64 = 0x430000;
const TEXT: u64 = 0x440000;
const ENTRIES: u64 = 0x450000;
const ROW: u64 = 0x460000;
const CAMERA: u64 = 0x470000;
const STATE: u64 = 0xe0000;
const PROVINCE: u64 = 0xe1000;

fn floats(f: &mut Fixture, address: u64, values: &[f32]) {
    for (i, v) in values.iter().enumerate() {
        f.put(address + i as u64 * 4, &v.to_le_bytes());
    }
}
fn find_fixture() -> Fixture {
    let mut f = locations::located();
    f.region(REGISTRY, 0x240);
    f.word(REGISTRY, BASE + 0x42736f8);
    f.word(0x324f8, REGISTRY);
    f.region(MANAGER, 0x200);
    f.word(0x326b8, MANAGER);
    f.word(MANAGER + 0x180, 0x32000);
    f.word(MANAGER + 0x190, FIND);
    f.region(FIND, 0x910);
    f.word(FIND, BASE + 0x4318868);
    f.word(FIND + 0x860, 0x32000);
    f.word(FIND + 0x868, WINDOW);
    f.region(WINDOW, 0x100);
    f.word(WINDOW, BASE + 0x4347d58);
    f.put(WINDOW + 0xa5, &[8]);
    f.word(FIND + 0x870, TEXT);
    f.region(TEXT, 0x100);
    f.word(REGISTRY + 0x230, TEXT);
    f.string(FIND + 0x878, "1");
    f.array(FIND + 0x8c8, ENTRIES, 1);
    f.region(ENTRIES, 0x50);
    f.word(ENTRIES + 0x40, STATE);
    f.number(ENTRIES + 0x48, 2);
    f.word(FIND + 0x8c0, ROW);
    f.region(ROW, 0x2c0);
    f.word(ROW, BASE + 0x4318b08);
    f.number(ROW + 0x2b8, 2);
    for offset in [0x2a0, 0x2a8, 0x2b0] {
        f.word(ROW + offset, STATE);
    }
    f.word(STATE + 0x20, PROVINCE);
    f.number(PROVINCE + 0x140, 3120);
    f.number(PROVINCE + 0x144, 1480);
    f.word(0x32750, CAMERA);
    f.region(CAMERA, 0x700);
    floats(
        &mut f,
        CAMERA + 0x698,
        &[3120.0, 300.0, 1390.0, 3120.0, 9.5, 1480.0],
    );
    f
}

#[test]
fn find_view_joins_first_state_row_entry_catalog_and_centred_camera() {
    let f = find_fixture();
    let v = find_view(&f, Some(1)).unwrap();
    let view = &v["find_view"];
    assert_eq!(view["materialized"], true);
    assert_eq!(view["window"]["exact_container"], true);
    assert_eq!(view["window"]["visible_flag"], true);
    assert_eq!(view["window"]["close_requested_flag"], false);
    assert_eq!(view["window"]["settled_visibility_verified"], false);
    assert_eq!(view["text_box_focused"], true);
    assert_eq!(view["query"], "1");
    assert_eq!(view["results"]["result_count"], 1);
    assert_eq!(view["results"]["first_row"]["state_id"], 1);
    assert_eq!(view["results"]["first_row"]["supported"], true);
    assert_eq!(v["camera"]["look_at"], json!([3120.0, 9.5, 1480.0]));
    assert_eq!(v["target"]["own_owned_and_controlled"], true);
    assert_eq!(v["target"]["anchor"]["province_id"], 2);
    assert_eq!(v["target"]["anchor"]["map_position"], json!([3120, 1480]));
    assert_eq!(v["target"]["camera_centered"], true);
    for flag in [
        "input_authorized",
        "placement_eligibility_verified",
        "effective_routing_verified",
        "current_frame_verified",
    ] {
        assert_eq!(v[flag], false);
    }
}

#[test]
fn camera_away_from_anchor_is_reported_not_centred() {
    let mut f = find_fixture();
    floats(&mut f, CAMERA + 0x6a4, &[3121.0, 9.5, 1480.0]);
    let v = find_view(&f, Some(1)).unwrap();
    assert_eq!(v["target"]["camera_centered"], false);
}

#[test]
fn foreign_state_never_reads_anchor_province_payload() {
    let mut f = find_fixture();
    f.number(STATE + 0xcc, 7);
    let v = find_view(&f, Some(1)).unwrap();
    assert_eq!(v["target"]["own_owned_and_controlled"], false);
    assert!(v["target"]["anchor"].is_null());
    production_not_read(&f, &[STATE + 0x20, PROVINCE + 0x140]);
}

#[test]
fn row_entry_disagreement_and_uncataloged_state_refuse() {
    let mut f = find_fixture();
    f.number(ENTRIES + 0x48, 1);
    assert!(find_view(&f, None).is_err());
    let mut f = find_fixture();
    f.word(ROW + 0x2b0, STATE + 8);
    assert!(find_view(&f, None).is_err());
    let mut f = find_fixture();
    f.number(STATE + 0x48, 2);
    assert!(find_view(&f, None).is_err());
    let mut f = find_fixture();
    f.word(ROW, BASE + 0x4318868);
    assert!(find_view(&f, None).is_err());
}

#[test]
fn foreign_owner_and_camera_invariant_refuse() {
    let mut f = find_fixture();
    f.word(FIND + 0x860, 0x33000);
    assert!(find_view(&f, None).is_err());
    let mut f = find_fixture();
    floats(&mut f, CAMERA + 0x6a8, &[10.0]);
    assert!(find_view(&f, None).is_err());
    let mut f = find_fixture();
    floats(&mut f, CAMERA + 0x698, &[f32::NAN]);
    assert!(find_view(&f, None).is_err());
}

#[test]
fn unmaterialized_view_reads_no_view_payload() {
    let mut f = find_fixture();
    f.word(FIND + 0x868, 0);
    let v = find_view(&f, None).unwrap();
    assert_eq!(v["find_view"]["materialized"], false);
    assert!(v["find_view"]["results"].is_null());
    assert!(v["target"].is_null());
    production_not_read(&f, &[FIND + 0x878, FIND + 0x8c8, ROW]);
}

#[test]
fn state_id_bounds_are_explicit() {
    assert!(validate_state_id(0).is_err());
    assert!(validate_state_id(4096).is_err());
    assert_eq!(validate_state_id(64).unwrap(), 64);
}

#[test]
fn type_refusals_name_the_object_and_image_relative_primary_only() {
    let mut f = find_fixture();
    f.word(PROVINCE, BASE + 0x41fb7a8);
    let e = find_view(&f, Some(1)).unwrap_err();
    assert!(e.message.contains("anchor province"));
    assert!(e.message.contains("observed rva_41fb7a8"));
    let mut f = find_fixture();
    f.word(FIND, 0x7f00_0000_0000);
    let e = find_view(&f, None).unwrap_err();
    assert!(e.message.contains("object type differs"));
    assert!(e.message.contains("outside_image"));
    assert!(!e.message.contains("7f0000000000"));
}
