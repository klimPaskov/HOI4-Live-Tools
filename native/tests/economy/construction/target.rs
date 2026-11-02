use super::*;
use hoi4_live_native::construction::target_snapshot::snapshot as target;

fn complete() -> Fixture {
    let mut f = locations::located();
    f.word(SENTINEL, BASE + 0x41b40a8);
    f.put(SENTINEL + 0x18, &[0]);
    f
}
#[test]
fn construction_target_joins_catalog_own_state_and_complete_queue_without_ui() {
    let f = complete();
    let v = target(&f, "infrastructure", 1).unwrap();
    assert_eq!(v["complete"], true);
    assert_eq!(v["queued"], json!({"in_state":3,"elsewhere":0}));
    assert_eq!(v["line_count"], 3);
    assert_eq!(v["input_authorized"], false);
    assert_eq!(v["observation_paused_game"], false);
    assert!(v.get("identity").is_none());
    production_not_read(&f, &[0xdead0000, 0xbeef0000, SENTINEL + 0x248, 0x326b8]);
}
#[test]
fn construction_target_checks_entire_queue_beyond_public_page_limit() {
    let mut f = complete();
    let original: Vec<_> = f
        .data
        .range(0x80000..0x80100)
        .map(|(a, b)| (*a - 0x80000, *b))
        .collect();
    f.array(0x40070, 0x180000, 129);
    for i in 0..129u64 {
        let line = 0x100000 + i * 0x100;
        for (offset, byte) in &original {
            f.put(line + offset, &[*byte]);
        }
        f.word(0x180000 + i * 8, line);
        f.word(line + 8, ((i + 1) << 32) | 0x3b);
        f.number(line + 0x48, if i == 128 { 7 } else { 1 });
    }
    let v = target(&f, "infrastructure", 1).unwrap();
    assert_eq!(v["queued"]["in_state"], 135);
    assert_eq!(v["line_count"], 129);
    f.word(0x108000 + 0x70, 0);
    let v = target(&f, "infrastructure", 1).unwrap();
    assert_eq!(v["complete"], false);
    assert!(v["queued"].is_null());
}
#[test]
fn construction_target_empty_queue_needs_public_template_and_own_state() {
    let mut f = complete();
    f.array(0x40070, 0, 0);
    let v = target(&f, "arms_factory", 1).unwrap();
    assert_eq!(v["queued"], json!({"in_state":0,"elsewhere":0}));
    assert_eq!(
        target(&f, "missing", 1).unwrap_err().code,
        "construction_target_unavailable"
    );
    f.number(0xe00cc, 2);
    assert_eq!(
        target(&f, "arms_factory", 1).unwrap_err().code,
        "construction_target_unavailable"
    );
}
#[test]
fn construction_target_unknown_building_and_unproven_location_are_not_zero() {
    for field in [0x80070, 0x82070] {
        let mut f = complete();
        f.word(field, 0);
        let v = target(&f, "infrastructure", 1).unwrap();
        assert_eq!(v["complete"], false);
        assert!(v["queued"].is_null());
    }
    let mut f = complete();
    f.number(0xe0190, 1); // own state's embedded status national override
    let v = target(&f, "infrastructure", 1).unwrap();
    assert_eq!(v["complete"], false);
    assert!(v["queued"].is_null());
}
#[test]
fn construction_target_foreign_state_refuses_before_id_catalog_or_queue_payload() {
    for field in [0xe00c8, 0xe00cc] {
        let mut f = complete();
        f.number(field, 2);
        assert_eq!(
            target(&f, "infrastructure", 1).unwrap_err().code,
            "construction_target_unavailable"
        );
        production_not_read(&f, &[0xe0048, DB, 0x80000]);
    }
}
#[test]
fn construction_target_duplicate_keys_wrong_ordinal_and_torn_membership_refuse() {
    let mut f = complete();
    f.string(OTHER + 0x248, "infrastructure");
    assert_eq!(
        target(&f, "infrastructure", 1).unwrap_err().code,
        "invalid_construction"
    );
    let mut f = complete();
    f.number(OTHER + 0x2b8, 1);
    assert_eq!(
        target(&f, "infrastructure", 1).unwrap_err().code,
        "invalid_construction"
    );
    for (field, size) in [(0x40070, 16), (LIST, 24), (0xe00cc, 4), (0x80048, 4)] {
        let mut f = complete();
        let mut changed: Vec<_> = (0..size).map(|i| f.data[&(field + i)]).collect();
        changed[0] ^= 1;
        f.fault = Some((field, 1, changed));
        assert_eq!(
            target(&f, "infrastructure", 1).unwrap_err().code,
            "torn_read",
            "{field:x}"
        );
    }
}

#[test]
fn construction_target_null_template_requires_exact_type_slot_and_inactive_flag() {
    for (field, bytes) in [
        (SENTINEL, (BASE + 0x41b40b0).to_le_bytes().to_vec()),
        (SENTINEL + 0x18, vec![1]),
        (OTHER, (BASE + 0x41b40a8).to_le_bytes().to_vec()),
    ] {
        let mut f = complete();
        f.put(field, &bytes);
        assert_eq!(
            target(&f, "infrastructure", 1).unwrap_err().code,
            "invalid_construction"
        );
        production_not_read(&f, &[SENTINEL + 0x248, SENTINEL + 0x2b8]);
    }
}
#[test]
fn construction_target_protocol_validates_before_process_access() {
    let server = hoi4_live_native::protocol::Server {
        process: None,
        unavailable: None,
    };
    for args in [
        json!({}),
        json!({"building_id":"arms_factory","state_id":0}),
        json!({"building_id":"arms_factory","state_id":1.5}),
        json!({"building_id":"bad key","state_id":1}),
        json!({"building_id":"arms_factory","state_id":1,"country":"ENG"}),
        json!({"building_id":"arms_factory","state_id":1,"click":true}),
        json!({"building_id":"arms_factory","state_id":1,"include_map_point":null}),
        json!({"building_id":"arms_factory","state_id":1,"include_map_point":1}),
        json!({"building_id":"arms_factory","state_id":1,"include_map_point":"true"}),
    ] {
        let reply=server.handle(json!({"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"hoi4_construction_target","arguments":args}})).unwrap();
        assert!(reply.to_string().contains("invalid_arguments"), "{reply}");
    }
}

#[test]
fn construction_target_map_candidate_refuses_linux_abi_before_any_read() {
    let f = complete();
    assert_eq!(
        hoi4_live_native::construction::target_snapshot::snapshot_with_map_point(
            &f,
            "infrastructure",
            1,
            true,
        )
        .unwrap_err()
        .code,
        "unsupported_platform"
    );
    assert!(f.tracked_reads.borrow().is_empty());
    assert_eq!(f.checks.get(), 0);
}

#[test]
fn construction_target_map_candidate_schema_defaults_to_false_without_new_tool() {
    let server = hoi4_live_native::protocol::Server {
        process: None,
        unavailable: None,
    };
    let reply = server
        .handle(json!({"jsonrpc":"2.0","id":1,"method":"tools/list","params":{}}))
        .unwrap();
    let tools = reply["result"]["tools"].as_array().unwrap();
    let tool = tools
        .iter()
        .find(|tool| tool["name"] == "hoi4_construction_target")
        .unwrap();
    assert_eq!(
        tool["inputSchema"]["properties"]["include_map_point"],
        json!({"type":"boolean","default":false})
    );
    assert_eq!(tool["inputSchema"]["additionalProperties"], false);
    assert_eq!(tool["annotations"]["readOnlyHint"], true);
}
