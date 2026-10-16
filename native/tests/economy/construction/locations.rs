use super::*;
use hoi4_live_native::construction::page_with_location;

const STATE: u64 = 0xe0000;
const PROVINCE: u64 = 0xe1000;
const STATE_STATUS: u64 = STATE + 0x120;
const PROVINCE_STATUS: u64 = PROVINCE + 0x1b0;

pub(super) fn located() -> Fixture {
    let mut f = fixture();
    f.array(0x302c8, 0xf0000, 3);
    f.word(0xf0008, STATE);
    f.array(0x302b0, 0xf1000, 3);
    f.word(0xf1010, PROVINCE);
    f.word(STATE, BASE + 0x41fba98);
    f.number(STATE + 0x48, 1);
    f.number(STATE + 0xc8, 1);
    f.number(STATE + 0xcc, 1);
    f.word(PROVINCE, BASE + 0x41fb728);
    f.number(PROVINCE + 0xa4, 2);
    f.number(PROVINCE + 0x1a8, 1);
    f.word(PROVINCE + 0xe0, STATE);
    for (status, flags, id, building, template, index, map, list) in [
        (STATE_STATUS, 2, 1, 0xd0000, TEMPLATE, 1, 0xf2000, 0xf4000),
        (PROVINCE_STATUS, 1, 2, 0xd1000, OTHER, 2, 0xf3000, 0xf5000),
    ] {
        f.word(status, BASE + 0x41b4768);
        f.number(status + 0x68, flags);
        f.number(status + 0x6c, id);
        f.number(status + 0x70, 0);
        f.word(building + 0x1b8, status);
        f.number(template + 0x2b8, index);
        f.array(status + 0x20, map, 3);
        f.number(map + index as u64 * 4, 1);
        f.array(status + 0x38, list, 2);
        f.word(list + 8, building);
    }
    f
}
fn observe(f: &Fixture) -> Result<serde_json::Value> {
    page_with_location(f, 0, 32, true)
}
#[test]
fn joins_state_and_province_with_reciprocal_building_slots_without_enumeration() {
    let f = located();
    let v = observe(&f).unwrap();
    assert_eq!(v["location_requested"], true);
    assert_eq!(v["location_available_count"], 2);
    assert_eq!(v["lines"][0]["location"]["state_id"], 1);
    assert_eq!(v["lines"][0]["location"]["kind"], "state");
    assert!(v["lines"][0]["location"]["province_id"].is_null());
    assert_eq!(v["lines"][2]["location"]["state_id"], 1);
    assert_eq!(v["lines"][2]["location"]["province_id"], 2);
    assert_eq!(v["lines"][2]["location"]["kind"], "province");
    assert_eq!(v["lines"][1]["location_available"], false);
    assert_eq!(
        v["lines"][1]["location"]["reason"],
        "railway_location_unavailable"
    );
    for index in [0, 2] {
        assert_eq!(
            v["lines"][index]["location"]["scope"],
            "own_owned_and_controlled_territory"
        );
        assert_eq!(v["lines"][index]["location"]["input_authorized"], false);
    }
    production_not_read(
        &f,
        &[
            0xf0000, 0xf0010, 0xf1000, 0xf1008, 0xf2000, 0xf2008, 0xf3000, 0xf3004, 0xf4000,
            0xf5000,
        ],
    );
}
#[test]
fn default_does_not_touch_any_location_even_when_foreign_or_invalid() {
    let mut f = located();
    f.word(0xd01b8, u64::MAX);
    f.word(0xd11b8, u64::MAX);
    let v = snapshot(&f).unwrap();
    assert_eq!(v["location_requested"], false);
    assert_eq!(v["location_available_count"], 0);
    production_not_read(&f, &[0xd01b8, 0xd11b8, STATE, PROVINCE, TEMPLATE + 0x2b8]);
}
#[test]
fn foreign_owner_or_controller_excludes_ids_and_building_membership_payload() {
    for offset in [0xc8, 0xcc] {
        let mut f = located();
        f.number(STATE + offset, 2);
        let v = observe(&f).unwrap();
        assert_eq!(v["location_available_count"], 0);
        assert!(v["lines"][0]["location"].get("state_id").is_none());
        assert!(v["lines"][2]["location"].get("province_id").is_none());
        production_not_read(
            &f,
            &[
                STATE + 0x48,
                PROVINCE + 0xa4,
                STATE_STATUS + 0x20,
                PROVINCE_STATUS + 0x20,
                TEMPLATE + 0x2b8,
                OTHER + 0x2b8,
            ],
        );
    }
    let mut f = located();
    f.number(PROVINCE + 0x1a8, 2);
    let v = observe(&f).unwrap();
    assert_eq!(v["location_available_count"], 1);
    assert_eq!(
        v["lines"][2]["location"]["reason"],
        "location_outside_own_control"
    );
    production_not_read(
        &f,
        &[
            PROVINCE + 0xe0,
            PROVINCE + 0xa4,
            PROVINCE_STATUS + 0x20,
            OTHER + 0x2b8,
        ],
    );
}
#[test]
fn national_overrides_and_unknown_status_kinds_never_reach_geographic_ids() {
    for (offset, value) in [(0x68, 0), (0x68, 3), (0x68, -1), (0x70, 1), (0x70, -1)] {
        let mut f = located();
        f.number(STATE_STATUS + offset, value);
        let v = observe(&f).unwrap();
        assert_eq!(v["lines"][0]["location_available"], false);
        production_not_read(
            &f,
            &[STATE_STATUS + 0x6c, STATE_STATUS + 0x20, TEMPLATE + 0x2b8],
        );
    }
}
#[test]
fn bad_ids_types_catalogs_and_reciprocal_links_refuse() {
    for (address, bytes) in [
        (STATE_STATUS, (BASE + 0x41b49c8).to_le_bytes().to_vec()),
        (STATE, (BASE + 0x41fb728).to_le_bytes().to_vec()),
        (PROVINCE, (BASE + 0x41fba98).to_le_bytes().to_vec()),
        (STATE_STATUS + 0x6c, 0i32.to_le_bytes().to_vec()),
        (PROVINCE_STATUS + 0x6c, (-1i32).to_le_bytes().to_vec()),
        (STATE + 0x48, 2i32.to_le_bytes().to_vec()),
        (PROVINCE + 0xa4, 1i32.to_le_bytes().to_vec()),
        (0x302d0, 4097i32.to_le_bytes().to_vec()),
        (0x302bc, 2i32.to_le_bytes().to_vec()),
        (STATE_STATUS + 0x28, 4097i32.to_le_bytes().to_vec()),
        (STATE_STATUS + 0x44, 1i32.to_le_bytes().to_vec()),
        (TEMPLATE + 0x2b8, (-1i32).to_le_bytes().to_vec()),
        (TEMPLATE + 0x2b8, 3i32.to_le_bytes().to_vec()),
        (0xf2004, (-1i32).to_le_bytes().to_vec()),
        (0xf2004, 2i32.to_le_bytes().to_vec()),
        (0xf4008, 0xd1000u64.to_le_bytes().to_vec()),
        (0xf5008, 0xd0000u64.to_le_bytes().to_vec()),
        (0x302c8, 0xf0001u64.to_le_bytes().to_vec()),
        (STATE_STATUS + 0x20, 0xf2001u64.to_le_bytes().to_vec()),
    ] {
        let mut f = located();
        f.put(address, &bytes);
        assert!(observe(&f).is_err(), "{address:x}");
    }
}
#[test]
fn embedded_status_identity_is_required_before_building_map_read() {
    let mut f = located();
    let wrong = 0xe5000;
    f.word(0xd01b8, wrong);
    f.word(wrong, BASE + 0x41b4768);
    f.number(wrong + 0x68, 2);
    f.number(wrong + 0x6c, 1);
    f.number(wrong + 0x70, 0);
    assert_eq!(observe(&f).unwrap_err().code, "invalid_construction");
    production_not_read(&f, &[wrong + 0x20, TEMPLATE + 0x2b8]);
}
#[test]
fn aliased_status_map_cannot_replace_the_actual_template_catalog_ordinal() {
    let mut f = located();
    // A wrong ordinal still maps to this exact building. Membership in both
    // catalogs alone would accept it; the fresh ordered template join must not.
    f.number(TEMPLATE + 0x2b8, 2);
    f.number(0xf2008, 1);
    assert_eq!(observe(&f).unwrap_err().code, "invalid_construction");
    production_not_read(&f, &[STATE_STATUS + 0x20, 0xf2008, 0xf4008]);
}
#[test]
fn all_location_identity_and_authorization_bytes_are_final_witnesses() {
    let baseline = located();
    observe(&baseline).unwrap();
    for (address, size) in [
        (0xd01b8, 8),
        (0xd11b8, 8),
        (STATE_STATUS, 8),
        (PROVINCE_STATUS, 8),
        (STATE_STATUS + 0x68, 4),
        (STATE_STATUS + 0x6c, 4),
        (STATE_STATUS + 0x70, 4),
        (PROVINCE_STATUS + 0x68, 4),
        (PROVINCE_STATUS + 0x6c, 4),
        (PROVINCE_STATUS + 0x70, 4),
        (0x302c8, 16),
        (0x302b0, 16),
        (0xf0008, 8),
        (0xf1010, 8),
        (STATE, 8),
        (STATE + 0xc8, 4),
        (STATE + 0xcc, 4),
        (STATE + 0x48, 4),
        (PROVINCE, 8),
        (PROVINCE + 0x1a8, 4),
        (PROVINCE + 0xe0, 8),
        (PROVINCE + 0xa4, 4),
        (TEMPLATE + 0x2b8, 4),
        (OTHER + 0x2b8, 4),
        (STATE_STATUS + 0x20, 16),
        (STATE_STATUS + 0x38, 16),
        (PROVINCE_STATUS + 0x20, 16),
        (PROVINCE_STATUS + 0x38, 16),
        (0xf2004, 4),
        (0xf3008, 4),
        (0xf4008, 8),
        (0xf5008, 8),
    ] {
        let mut bytes = (0..size)
            .map(|i| baseline.data[&(address + i as u64)])
            .collect::<Vec<_>>();
        bytes[0] ^= 1;
        let mut f = located();
        f.fault = Some((
            address,
            baseline.tracked_reads.borrow()[&address] - 1,
            bytes,
        ));
        assert_eq!(
            page_with_location(&f, 0, 1, true).unwrap_err().code,
            "torn_read",
            "{address:x}"
        );
    }
}
#[test]
fn pagination_still_checks_out_of_page_locations_and_reports_full_coverage() {
    let f = located();
    let v = page_with_location(&f, 1, 1, true).unwrap();
    assert_eq!(v["location_available_count"], 2);
    assert_eq!(v["lines"][0]["location_available"], false);
    let mut f = located();
    f.word(0xf5008, 0xd0000);
    assert!(page_with_location(&f, 0, 1, true).is_err());
}
#[test]
fn protocol_accepts_only_boolean_location_option_for_construction() {
    let s = hoi4_live_native::protocol::Server {
        process: None,
        unavailable: None,
    };
    for value in [json!(null), json!(1), json!("true"), json!([])] {
        let v=s.handle(json!({"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"hoi4_construction_queue","arguments":{"include_location":value}}})).unwrap();
        assert!(v.to_string().contains("invalid_arguments"));
    }
    for value in [true, false] {
        let v=s.handle(json!({"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"hoi4_construction_queue","arguments":{"include_location":value}}})).unwrap();
        assert!(!v.to_string().contains("invalid_arguments"));
        let v=s.handle(json!({"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"hoi4_production_lines","arguments":{"include_location":value}}})).unwrap();
        assert!(v.to_string().contains("invalid_arguments"));
    }
}
