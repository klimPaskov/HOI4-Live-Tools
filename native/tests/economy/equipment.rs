use super::*;
const BASE: u64 = 0x10000;
const DB: u64 = 0xc0000;
const ARRAY: u64 = 0xc1000;
const TYPE: u64 = 0xc2000;
const OTHER: u64 = 0xc3000;
const SENTINEL: u64 = 0xc4000;
const INTERN: u32 = 0x123456;
fn fixture() -> Fixture {
    let mut f = production_fixture();
    f.word(BASE + 0x439f440, DB);
    f.word(DB, BASE + 0x41f72d8);
    f.array(DB + 0x60, ARRAY, 2);
    f.word(ARRAY, SENTINEL);
    f.word(ARRAY + 8, TYPE);
    f.word(TYPE, BASE + 0x41f8810);
    f.put(TYPE + 8, &INTERN.to_le_bytes());
    f.string(TYPE + 0x10, "infantry_eq_1");
    for (line, variant) in [(0x80000, 0xd0000), (0x81000, 0xe0000), (0x82000, 0xf0000)] {
        f.word(line + 0x88, variant);
        f.word(variant, BASE + 0x41fa1a0);
        f.put(variant + 0x14, &INTERN.to_le_bytes());
        f.word(variant + 0x3f0, TYPE);
    }
    f
}
fn snapshot(f: &Fixture) -> serde_json::Value {
    hoi4_live_native::production_lines::snapshot(f).unwrap()
}
#[test]
fn intern_keys_are_not_indices_and_distinct_variants_share_a_base_type() {
    let f = fixture();
    let v = snapshot(&f);
    assert_eq!(v["equipment_id_kind"], "base_equipment_type_key");
    assert_eq!(v["equipment_ids_available"], true);
    assert_eq!(v["equipment_id_unavailable_count"], 0);
    assert_eq!(v["custom_variant_identity_available"], false);
    for row in v["lines"].as_array().unwrap() {
        assert_eq!(row["equipment_id"], "infantry_eq_1");
        assert!(row["equipment_unavailable_reason"].is_null());
        assert_eq!(row["input_authorized"], false);
    }
    assert_eq!(v["lines"][1]["line_handle"], "0xffff000100000039");
    production_not_read(&f, &[SENTINEL, 0xd0018, 0xd001c, 0xe0018, 0xf0018]);
    assert_eq!(f.tracked_reads.borrow()[&(DB + 0x60)], 2);
}
#[test]
fn foreign_owner_prevents_every_variant_and_catalog_read() {
    let mut f = fixture();
    f.word(0x80020, 0xdead0000);
    assert_eq!(
        hoi4_live_native::production_lines::snapshot(&f)
            .unwrap_err()
            .code,
        "invalid_production_owner"
    );
    production_not_read(
        &f,
        &[
            0x80088,
            0xd0000,
            0xd03f0,
            BASE + 0x439f440,
            DB,
            TYPE,
            TYPE + 8,
            TYPE + 0x10,
        ],
    );
}
#[test]
fn missing_catalog_membership_never_reads_the_unselected_type_payload() {
    for count in [0, 1] {
        let mut f = fixture();
        f.array(DB + 0x60, ARRAY, count);
        let v = snapshot(&f);
        assert_eq!(v["equipment_ids_available"], false);
        assert_eq!(v["equipment_id_unavailable_count"], 3);
        for row in v["lines"].as_array().unwrap() {
            assert!(row["equipment_id"].is_null());
            assert_eq!(
                row["equipment_unavailable_reason"],
                "type_outside_reviewed_base_catalog"
            );
        }
        production_not_read(&f, &[TYPE, TYPE + 8, TYPE + 0x10, SENTINEL]);
    }
}
#[test]
fn nulls_and_unknown_primary_types_are_explicit_and_never_assume_payload() {
    for (address, value, reason, blocked) in [
        (0x80088, 0, "no_variant", 0xd0000),
        (
            0xd0000,
            BASE + 0x41fa1a8,
            "unsupported_variant_type",
            0xd03f0,
        ),
        (0xd03f0, 0, "no_type", 0xd0014),
        (
            TYPE,
            BASE + 0x41f8818,
            "unsupported_equipment_type",
            TYPE + 8,
        ),
    ] {
        let mut f = fixture();
        f.word(address, value);
        let v = snapshot(&f);
        assert_eq!(v["lines"][0]["equipment_unavailable_reason"], reason);
        assert!(v["lines"][0]["equipment_id"].is_null());
        production_not_read(&f, &[blocked]);
    }
}
#[test]
fn catalog_header_pointer_and_duplicate_bounds_precede_selected_payload() {
    for (capacity, count, data, code) in [
        (8193, 2, ARRAY, "invalid_production_equipment"),
        (2, 3, ARRAY, "invalid_array"),
        (-1, 0, 0, "invalid_array"),
        (2, 2, ARRAY + 1, "invalid_production_equipment"),
    ] {
        let mut f = fixture();
        f.word(DB + 0x60, data);
        f.number(DB + 0x68, capacity);
        f.number(DB + 0x6c, count);
        assert_eq!(
            hoi4_live_native::production_lines::snapshot(&f)
                .unwrap_err()
                .code,
            code
        );
        production_not_read(&f, &[ARRAY, ARRAY + 1, TYPE, TYPE + 0x10]);
    }
    for value in [0, TYPE + 1, SENTINEL] {
        let mut f = fixture();
        f.word(ARRAY + 8, value);
        assert_eq!(
            hoi4_live_native::production_lines::snapshot(&f)
                .unwrap_err()
                .code,
            "invalid_production_equipment"
        );
        production_not_read(&f, &[TYPE, TYPE + 0x10, SENTINEL]);
    }
    let mut f = fixture();
    f.word(DB, BASE + 0x41f72e0);
    assert_eq!(
        hoi4_live_native::production_lines::snapshot(&f)
            .unwrap_err()
            .code,
        "invalid_production_equipment"
    );
    production_not_read(&f, &[DB + 0x60, TYPE]);
}
#[test]
fn intern_mismatch_refuses_before_name_and_is_checked_for_cached_types() {
    for variant in [0xd0000, 0xf0000] {
        let mut f = fixture();
        f.put(variant + 0x14, &(INTERN + 1).to_le_bytes());
        assert_eq!(
            hoi4_live_native::production_lines::page(&f, 0, 1)
                .unwrap_err()
                .code,
            "invalid_production_equipment"
        );
        if variant == 0xd0000 {
            production_not_read(&f, &[TYPE + 0x10]);
        }
    }
}
#[test]
fn malformed_names_and_duplicate_selected_keys_never_become_equipment_identity() {
    for name in ["", "bad/key", "bad key", "équipement"] {
        let mut f = fixture();
        f.string(TYPE + 0x10, name);
        assert_eq!(
            hoi4_live_native::production_lines::snapshot(&f)
                .unwrap_err()
                .code,
            "invalid_production_equipment"
        );
    }
    let mut f = fixture();
    f.word(TYPE + 0x18, 129);
    assert_eq!(
        hoi4_live_native::production_lines::snapshot(&f)
            .unwrap_err()
            .code,
        "invalid_string"
    );
    let mut f = fixture();
    f.word(ARRAY, OTHER);
    f.word(OTHER, BASE + 0x41f8810);
    f.put(OTHER + 8, &(INTERN + 1).to_le_bytes());
    f.string(OTHER + 0x10, "infantry_eq_1");
    f.word(0xf03f0, OTHER);
    f.put(0xf0014, &(INTERN + 1).to_le_bytes());
    assert_eq!(
        hoi4_live_native::production_lines::page(&f, 0, 1)
            .unwrap_err()
            .code,
        "invalid_production_equipment"
    );
}
#[test]
fn every_join_catalog_and_key_byte_is_witnessed_before_any_page_returns() {
    for (address, width) in [
        (0x80088, 8),
        (0xd0000, 8),
        (0xd03f0, 8),
        (0xd0014, 4),
        (BASE + 0x439f440, 8),
        (DB, 8),
        (DB + 0x60, 16),
        (ARRAY, 16),
        (TYPE, 8),
        (TYPE + 8, 4),
        (TYPE + 0x10, 32),
        (0xf03f0, 8),
        (0xf0014, 4),
    ] {
        let mut f = fixture();
        let mut changed: Vec<_> = (0..width).map(|i| f.data[&(address + i)]).collect();
        changed[0] ^= 8;
        f.fault = Some((address, 1, changed));
        assert_eq!(
            hoi4_live_native::production_lines::page(&f, 0, 1)
                .unwrap_err()
                .code,
            "torn_read",
            "witness {address:x}"
        );
    }
}
#[test]
fn fresh_snapshots_do_not_cache_catalog_across_requests_or_process_replacement() {
    let mut f = fixture();
    assert_eq!(snapshot(&f)["equipment_ids_available"], true);
    f.word(ARRAY + 8, OTHER);
    assert_eq!(snapshot(&f)["equipment_ids_available"], false);
    let mut f = fixture();
    f.replaced = true;
    assert_eq!(
        hoi4_live_native::production_lines::snapshot(&f)
            .unwrap_err()
            .code,
        "ownership_changed"
    );
    let mut f = fixture();
    f.changed_id = true;
    assert_eq!(
        hoi4_live_native::production_lines::snapshot(&f)
            .unwrap_err()
            .code,
        "torn_read"
    );
}
#[test]
fn longest_allowed_type_key_fits_full_mcp_page_and_external_bytes_are_witnessed() {
    let mut f = fixture();
    let key = "A".repeat(128);
    f.region(TYPE + 0x10, 32);
    f.word(TYPE + 0x10, 0xc5000);
    f.word(TYPE + 0x18, 128);
    f.put(0xc5000, key.as_bytes());
    let mut v = snapshot(&f);
    assert_eq!(v["lines"][0]["equipment_id"], key);
    let row = v["lines"][0].clone();
    v["lines"] = serde_json::json!(vec![row; 128]);
    let wire=serde_json::to_string(&serde_json::json!({"jsonrpc":"2.0","id":1,"result":{"content":[{"type":"text","text":serde_json::to_string(&v).unwrap()}]}})).unwrap();
    assert!(wire.len() <= hoi4_live_native::framing::LIMIT);
    f.fault = Some((0xc5000, 0, vec![b'B'; 128]));
    // First new observation is B; a later change must fail rather than reuse A.
    assert_eq!(snapshot(&f)["lines"][0]["equipment_id"], "B".repeat(128));
    f.fault_reads.set(0);
    f.fault = Some((0xc5000, 1, vec![b'C'; 128]));
    assert_eq!(
        hoi4_live_native::production_lines::snapshot(&f)
            .unwrap_err()
            .code,
        "torn_read"
    );
}

#[path = "equipment/controls.rs"]
mod controls;
