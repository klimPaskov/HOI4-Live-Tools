use super::*;
use hoi4_live_native::construction::{page, snapshot};
use serde_json::json;
#[path = "construction/locations.rs"]
mod locations;
const BASE: u64 = 0x10000;
const DB: u64 = 0xc0000;
const LIST: u64 = 0xc1000;
const TEMPLATE: u64 = 0xc2000;
const OTHER: u64 = 0xc3000;
const SENTINEL: u64 = 0xc4000;

fn fixture() -> Fixture {
    let mut f = Fixture::new();
    f.economy();
    f.array(0x40070, 0x70000, 3);
    f.word(BASE + 0x439f400, DB);
    f.word(DB, BASE + 0x41b3ed8);
    f.array(DB + 0x40, LIST, 3);
    for (index, template) in [SENTINEL, TEMPLATE, OTHER].iter().enumerate() {
        f.word(LIST + index as u64 * 8, *template);
    }
    for (template, key) in [(TEMPLATE, "infrastructure"), (OTHER, "arms_factory")] {
        f.word(template, BASE + 0x41b49c8);
        f.put(template + 0x18, &[1]);
        f.string(template + 0x248, key);
        // Neither intern nor localized name is needed for this direct key join.
    }
    for (i, line, table, handle) in [
        (0, 0x80000, 0x4205680, 0xffff00010000003bu64),
        (1, 0x81000, 0x42ee398, 0xf234567812345678),
        (2, 0x82000, 0x4205680, 0x123400020000003b),
    ] {
        f.word(0x70000 + i * 8, line);
        f.word(line, BASE + table);
        f.word(line + 0x20, 0x40000);
        f.word(line + 8, handle);
        f.number(line + 0x14, 12);
        f.number(line + 0x18, 2);
        f.word(line + 0x28, 100_000_000);
        f.word(line + 0x30, 750_000);
        f.word(line + 0x38, 25_000_000);
        f.number(line + 0x40, 15);
        f.number(line + 0x44, i as i32);
        f.number(line + 0x48, 3);
    }
    for (line, building, template) in [(0x80000, 0xd0000, TEMPLATE), (0x82000, 0xd1000, OTHER)] {
        f.word(line + 0x70, building);
        f.number(line + 0x78, 1);
        f.put(line + 0x8c, &[1]);
        f.put(line + 0x99, &[1]);
        f.word(building, BASE + 0x41b3e60);
        f.word(building + 0x1c0, template);
        // No location/foreign state or railway payload bytes are readable.
    }
    f
}
fn unknown(f: &mut Fixture, secondary: bool) {
    unknown_production(f, secondary);
    f.array(0x40070, 0x70000, 4);
}
#[test]
fn distinguishes_cost_produced_counts_flags_and_dynamic_rail_handle() {
    let f = fixture();
    let v = snapshot(&f).unwrap();
    assert_eq!(v["total_count"], 3);
    assert_eq!(v["building_count"], 2);
    assert_eq!(v["building_id_unavailable_count"], 0);
    assert_eq!(v["complete_catalog"], true);
    assert_eq!(v["numeric_data_complete"], true);
    assert_eq!(v["handle_scope"]["player"], "GER");
    assert_eq!(v["handle_scope"]["cross_load_identity_available"], false);
    assert!(v["handle_scope"].get("exe").is_none());
    assert_eq!(v["live_validation"], "pending");
    assert_eq!(v["lines"][0]["line_handle"], "0xffff00010000003b");
    assert_eq!(v["lines"][1]["line_handle"], "0xf234567812345678");
    let row = &v["lines"][0];
    assert_eq!(row["cost_raw_signed"], "100000000");
    assert_eq!(row["produced_raw_signed"], "25000000");
    assert_eq!(row["speed_raw_signed"], "750000");
    assert_eq!(
        row["produced_over_cost"],
        json!({"numerator":"25000000","denominator":"100000000"})
    );
    assert_eq!(row["active_factories_raw_signed"], 12);
    assert_eq!(row["damaged_factories_raw_signed"], 2);
    assert_eq!(row["queued_factories_raw_signed"], 15);
    assert_eq!(row["amount_raw_signed"], 3);
    assert_eq!(row["building"]["building_id"], "infrastructure");
    assert_eq!(row["building"]["to_repair_raw_signed"], 1);
    assert_eq!(row["building"]["conversion"], true);
    assert_eq!(row["building"]["allied_build"], true);
    assert!(v["lines"][1]["building"].is_null());
    assert_eq!(v["lines"][1]["line_kind"], "railway");
    assert_eq!(v["lines"][2]["building"]["building_id"], "arms_factory");
    for key in [
        "observation_paused_game",
        "location_available",
        "railway_path_available",
        "idle_builders_available",
        "allocation_available",
        "eligibility_available",
        "input_authorized",
    ] {
        assert_eq!(v[key], false, "{key}");
    }
    production_not_read(
        &f,
        &[
            SENTINEL,
            TEMPLATE + 8,
            TEMPLATE + 0x1e8,
            0xd0008,
            0xd01b8,
            0x81070,
            0x81078,
            0x81080,
            0x8108c,
            0x81098,
            0x810a0,
            0x40058,
        ],
    );
    assert_eq!(f.checks.get(), 2);
}
#[test]
fn foreign_owner_and_wrong_handle_stop_before_scalar_or_building_reads() {
    for (address, value) in [
        (0x80020, 0xdead0000),
        (0x81020, 0xdead0000),
        (0x80008, 0),
        (0x80008, 0x123400000039),
        (0x81008, 0),
        (0x81008, 0xffff00010000003b),
        (0x82008, 0xffff00010000003b),
    ] {
        let mut f = fixture();
        f.word(address, value);
        assert_eq!(snapshot(&f).unwrap_err().code, "invalid_construction");
        let line = address & !0xfff;
        production_not_read(
            &f,
            &[
                line + 0x14,
                line + 0x18,
                line + 0x28,
                line + 0x38,
                line + 0x70,
            ],
        );
        if address & 0xfff == 0x20 {
            production_not_read(&f, &[line + 8]);
        }
    }
}
#[test]
fn rejects_array_caps_alignment_and_duplicate_pointers_before_any_line() {
    for (address, bytes) in [
        (0x40078, 2049i32.to_le_bytes().to_vec()),
        (0x4007c, (-1i32).to_le_bytes().to_vec()),
        (0x4007c, 4i32.to_le_bytes().to_vec()),
        (0x40070, 0x70001u64.to_le_bytes().to_vec()),
        (0x70008, 0x80000u64.to_le_bytes().to_vec()),
        (0x70008, 0x81001u64.to_le_bytes().to_vec()),
        (0x70008, 0u64.to_le_bytes().to_vec()),
    ] {
        let mut f = fixture();
        f.put(address, &bytes);
        assert!(snapshot(&f).is_err());
        production_not_read(&f, &[0x80000, 0x81000, 0x82000]);
    }
}
#[test]
fn unsupported_and_secondary_types_are_metadata_only() {
    for secondary in [false, true] {
        let mut f = fixture();
        unknown(&mut f, secondary);
        let v = page(&f, 3, 1).unwrap();
        assert_eq!(v["unsupported_count"], 1);
        assert_eq!(v["complete_catalog"], false);
        assert_eq!(v["numeric_data_complete"], false);
        assert_eq!(v["page_count"], 1);
        assert_eq!(v["lines"], json!([]));
        let row = &v["unsupported_type_metadata"][0];
        assert_eq!(row["ownership_verified"], false);
        if secondary {
            assert!(row["type_encoding"].is_null());
        } else {
            assert_eq!(row["type_encoding"], "21CGeneratedUnknownLine");
        }
        production_not_read(&f, &[0x90008, 0x90020, 0x90028, 0x90070]);
    }
}
#[test]
fn raw_signed_extremes_are_lossless_and_policy_is_not_an_engine_claim() {
    for offset in [0x28, 0x30, 0x38] {
        for value in [i64::MIN, i64::MAX, -1, 100_000_000_001] {
            let mut f = fixture();
            f.word(0x80000 + offset, value as u64);
            let v = snapshot(&f).unwrap();
            assert_eq!(v["anomalous_numeric_count"], 1);
            assert_eq!(v["numeric_data_complete"], false);
            assert_eq!(v["lines"][0]["numeric_policy_satisfied"], false);
            assert!(v["lines"][0]["produced_over_cost"].is_null());
            let key = match offset {
                0x28 => "cost_raw_signed",
                0x30 => "speed_raw_signed",
                _ => "produced_raw_signed",
            };
            assert_eq!(v["lines"][0][key], value.to_string());
            assert_eq!(v["policy"]["engine_invariant_claimed"], false);
        }
    }
    for offset in [0x14, 0x18, 0x40, 0x44, 0x48, 0x78] {
        let mut f = fixture();
        f.number(0x80000 + offset, -1);
        assert_eq!(snapshot(&f).unwrap()["anomalous_numeric_count"], 1);
    }
}
#[test]
fn zero_cost_is_unavailable_and_overfull_fraction_is_not_clamped() {
    let mut f = fixture();
    f.word(0x80028, 0);
    let v = snapshot(&f).unwrap();
    assert!(v["lines"][0]["produced_over_cost"].is_null());
    assert_eq!(v["lines"][0]["fraction_unavailable_reason"], "zero_cost");
    assert_eq!(v["lines"][0]["numeric_policy_satisfied"], true);
    f.word(0x80028, 1);
    assert_eq!(
        snapshot(&f).unwrap()["lines"][0]["produced_over_cost"],
        json!({"numerator":"25000000","denominator":"1"})
    );
}
#[test]
fn building_nulls_unreviewed_types_and_template_validity_flag_unsets_have_explicit_reasons() {
    for (address, value, reason, blocked) in [
        (0x80070, 0, "no_building", 0xd0000),
        (
            0xd0000,
            BASE + 0x41b3e68,
            "unsupported_building_type",
            0xd01c0,
        ),
        (0xd01c0, 0, "no_template", BASE + 0x439f400),
        (
            TEMPLATE,
            BASE + 0x41b49d0,
            "unsupported_template_type",
            TEMPLATE + 0x18,
        ),
    ] {
        let mut f = fixture();
        f.word(address, value);
        // Restrict the collection to isolate forbidden downstream reads.
        f.array(0x40070, 0x70000, 1);
        let v = snapshot(&f).unwrap();
        assert_eq!(v["building_id_unavailable_count"], 1);
        assert_eq!(
            v["lines"][0]["building"]["building_id_unavailable_reason"],
            reason
        );
        assert!(v["lines"][0]["building"]["building_id"].is_null());
        production_not_read(&f, &[blocked]);
    }
    let mut f = fixture();
    f.put(TEMPLATE + 0x18, &[0]);
    assert_eq!(
        snapshot(&f).unwrap()["lines"][0]["building"]["building_id_unavailable_reason"],
        "template_validity_flag_unset"
    );
    production_not_read(&f, &[TEMPLATE + 0x248]);
}
#[test]
fn fresh_catalog_membership_precedes_template_payload_and_duplicate_keys_refuse() {
    let mut f = fixture();
    f.array(DB + 0x40, LIST, 1);
    let v = snapshot(&f).unwrap();
    assert_eq!(v["building_id_unavailable_count"], 2);
    assert_eq!(
        v["lines"][0]["building"]["building_id_unavailable_reason"],
        "template_outside_reviewed_catalog"
    );
    production_not_read(&f, &[TEMPLATE, OTHER, SENTINEL]);
    let mut f = fixture();
    f.string(OTHER + 0x248, "infrastructure");
    assert_eq!(page(&f, 0, 1).unwrap_err().code, "invalid_construction");
    let mut f = fixture();
    f.word(0xd11c0, TEMPLATE);
    let v = snapshot(&f).unwrap();
    assert_eq!(v["lines"][2]["building"]["building_id"], "infrastructure");
    assert_eq!(f.tracked_reads.borrow()[&(DB + 0x40)], 2);
}
#[test]
fn catalog_type_alignment_capacity_and_unique_pointers_are_mandatory() {
    for (address, value) in [
        (BASE + 0x439f400, DB + 1),
        (DB, BASE + 0x41b3ee0),
        (DB + 0x40, LIST + 1),
        (LIST + 8, SENTINEL),
        (LIST + 8, TEMPLATE + 1),
        (0xd01c0, TEMPLATE + 1),
    ] {
        let mut f = fixture();
        f.word(address, value);
        assert_eq!(snapshot(&f).unwrap_err().code, "invalid_construction");
        production_not_read(&f, &[TEMPLATE + 0x248]);
    }
    let mut f = fixture();
    f.number(DB + 0x48, 4097);
    assert_eq!(snapshot(&f).unwrap_err().code, "invalid_construction");
    production_not_read(&f, &[LIST, TEMPLATE]);
}
#[test]
fn key_and_boolean_boundaries_fail_without_reading_unbounded_payload() {
    for key in ["", "not a key", "x/y", "non_ascii_é"] {
        let mut f = fixture();
        f.string(TEMPLATE + 0x248, key);
        assert_eq!(snapshot(&f).unwrap_err().code, "invalid_construction");
    }
    for address in [0x8008c, 0x80099, TEMPLATE + 0x18] {
        let mut f = fixture();
        f.put(address, &[2]);
        assert_eq!(snapshot(&f).unwrap_err().code, "invalid_construction");
    }
    let mut f = fixture();
    f.word(TEMPLATE + 0x250, 129);
    assert_eq!(snapshot(&f).unwrap_err().code, "invalid_string");
}
#[test]
fn final_witness_covers_all_new_roots_members_identity_and_published_fields() {
    let baseline = fixture();
    snapshot(&baseline).unwrap();
    for (address, size) in [
        (BASE + 0x439f400, 8),
        (DB, 8),
        (DB + 0x40, 16),
        (LIST, 24),
        (TEMPLATE, 8),
        (TEMPLATE + 0x18, 1),
        (TEMPLATE + 0x248, 32),
        (0xd0000, 8),
        (0xd01c0, 8),
        (0x40070, 16),
        (0x70000, 24),
        (0x82000, 8),
        (0x82008, 8),
        (0x82020, 8),
        (0x82014, 4),
        (0x82018, 4),
        (0x82028, 8),
        (0x82030, 8),
        (0x82038, 8),
        (0x82040, 4),
        (0x82044, 4),
        (0x82048, 4),
        (0x82070, 8),
        (0x82078, 4),
        (0x8208c, 1),
        (0x82099, 1),
        (0x81008, 8),
    ] {
        let mut bytes = (0..size)
            .map(|i| baseline.data[&(address + i as u64)])
            .collect::<Vec<_>>();
        bytes[0] ^= 1;
        let mut f = fixture();
        let after = baseline.tracked_reads.borrow()[&address] - 1;
        f.fault = Some((address, after, bytes));
        assert_eq!(page(&f, 0, 1).unwrap_err().code, "torn_read", "{address:x}");
    }
}
#[test]
fn paging_validates_outside_page_and_preserves_global_coverage() {
    let f = fixture();
    let v = page(&f, 1, 1).unwrap();
    assert_eq!(v["total_count"], 3);
    assert_eq!(v["next_offset"], 2);
    assert_eq!(v["lines"][0]["entry_index"], 1);
    assert_eq!(page(&f, 2048, 1).unwrap()["page_count"], 0);
    let mut f = fixture();
    f.word(0x82020, 0xdead0000);
    assert_eq!(page(&f, 0, 1).unwrap_err().code, "invalid_construction");
    for (offset, limit) in [
        (2049, 1),
        (0, 0),
        (0, 129),
        (usize::MAX, 1),
        (0, usize::MAX),
    ] {
        let f = fixture();
        assert_eq!(
            page(&f, offset, limit).unwrap_err().code,
            "invalid_arguments"
        );
        assert_eq!(f.checks.get(), 0);
        assert!(f.tracked_reads.borrow().is_empty());
    }
}
#[test]
fn empty_queue_never_reads_catalog_and_process_or_campaign_change_refuses() {
    let mut f = fixture();
    f.array(0x40070, 0, 0);
    let v = snapshot(&f).unwrap();
    assert_eq!(v["total_count"], 0);
    assert_eq!(v["complete_catalog"], true);
    production_not_read(&f, &[BASE + 0x439f400, 0x80000]);
    let mut f = fixture();
    f.replaced = true;
    assert_eq!(snapshot(&f).unwrap_err().code, "ownership_changed");
    let mut f = fixture();
    f.changed_id = true;
    assert_eq!(snapshot(&f).unwrap_err().code, "torn_read");
}
#[test]
fn construction_protocol_is_read_only_and_rejects_foreign_or_action_arguments() {
    let s = hoi4_live_native::protocol::Server {
        process: None,
        unavailable: None,
    };
    for args in [
        json!({"country":"OTHER"}),
        json!({"allocate":1}),
        json!({"state":1}),
        json!({"offset":2049}),
        json!({"offset":-1}),
        json!({"offset":1.0}),
        json!({"limit":0}),
        json!({"limit":129}),
        json!({"limit":"32"}),
        json!({"limit":null}),
    ] {
        let v=s.handle(json!({"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"hoi4_construction_queue","arguments":args}})).unwrap();
        assert!(v.to_string().contains("invalid_arguments"));
    }
    let v = s
        .handle(json!({"jsonrpc":"2.0","id":1,"method":"tools/list"}))
        .unwrap();
    let tool = v["result"]["tools"]
        .as_array()
        .unwrap()
        .iter()
        .find(|t| t["name"] == "hoi4_construction_queue")
        .unwrap();
    assert_eq!(tool["annotations"]["readOnlyHint"], true);
    assert_eq!(tool["inputSchema"]["additionalProperties"], false);
    let v=s.handle(json!({"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"hoi4_capabilities","arguments":{}}})).unwrap();
    assert!(v.to_string().contains("own_construction_queue"));
}
#[test]
fn maximum_page_fits_mcp_framing_with_full_signed_widths_and_long_keys() {
    let mut f = fixture();
    unknown(&mut f, false);
    let mut v = snapshot(&f).unwrap();
    let mut row = v["lines"][0].clone();
    for key in ["cost_raw_signed", "speed_raw_signed", "produced_raw_signed"] {
        row[key] = json!(i64::MIN.to_string());
    }
    row["building"]["building_id"] = json!("x".repeat(128));
    let mut unknown = v["unsupported_type_metadata"][0].clone();
    unknown["type_encoding"] = json!("x".repeat(127));
    for n in [0, 64, 128] {
        v["lines"] = json!(vec![row.clone(); n]);
        v["unsupported_type_metadata"] = json!(vec![unknown.clone(); 128 - n]);
        let wire=serde_json::to_vec(&json!({"jsonrpc":"2.0","id":1,"result":{"content":[{"type":"text","text":v.to_string()}]}})).unwrap();
        assert!(wire.len() <= hoi4_live_native::framing::LIMIT);
    }
}
