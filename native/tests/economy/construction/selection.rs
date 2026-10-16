use super::*;

fn selected(mode: i32) -> Fixture {
    let mut f = fixture_ui();
    // No queue or panel is needed for an owned public building selection.
    f.array(0x40070, 0, 0);
    f.word(MANAGER + 0x150, 0);
    f.number(MANAGER + 0x3f0, mode);
    f.word(MANAGER + 0x3e8, TEMPLATE);
    f.number(TEMPLATE + 0x2b8, 1);
    f.word(DB + 0x3a8, if mode == 3 { TEMPLATE } else { SENTINEL });
    if mode == 3 {
        f.string(TEMPLATE + 0x248, "rail_way");
    }
    f
}

#[test]
fn selected_building_is_canonical_even_without_queue_or_panel() {
    for (mode, key, reason) in [
        (1, "infrastructure", "ordinary_construction_choice"),
        (3, "rail_way", "rail_way_choice"),
    ] {
        let f = selected(mode);
        let v = observe(&f).unwrap();
        assert_eq!(v["total_count"], 0);
        assert_eq!(v["ui"]["available"], false);
        let s = &v["ui"]["selection"];
        assert_eq!(s["scope"], "owned_interface_selection");
        assert_eq!(s["mode_raw"], mode);
        assert_eq!(s["supported"], true);
        assert_eq!(s["active"], true);
        assert_eq!(s["building_id"], key);
        assert_eq!(s["reason"], reason);
        for field in [
            "placement_eligibility_verified",
            "map_routing_verified",
            "input_authorized",
        ] {
            assert_eq!(s[field], false);
        }
        production_not_read(&f, &[SENTINEL, OTHER, PANEL, TEMPLATE + 8]);
    }
}

#[test]
fn inactive_unknown_and_omitted_selection_never_follow_template_or_catalog() {
    for mode in [0, 2, -1, 4, i32::MAX] {
        let f = selected(mode);
        let v = observe(&f).unwrap();
        let s = &v["ui"]["selection"];
        assert_eq!(s["supported"], mode == 0);
        assert!(s["building_id"].is_null());
        if mode == 0 {
            assert_eq!(s["active"], false);
        } else {
            assert!(s["active"].is_null());
        }
        production_not_read(&f, &[MANAGER + 0x3e8, BASE + 0x439f400, DB, TEMPLATE]);
    }
    let f = selected(1);
    snapshot(&f).unwrap();
    production_not_read(&f, &[MANAGER + 0x3f0, MANAGER + 0x3e8, BASE + 0x439f400]);
}

#[test]
fn selection_membership_type_validity_ordinal_and_key_fail_closed() {
    let mut f = selected(1);
    f.array(DB + 0x40, LIST, 1);
    assert_eq!(observe(&f).unwrap_err().code, "invalid_construction");
    production_not_read(&f, &[TEMPLATE, TEMPLATE + 0x18, TEMPLATE + 0x248]);
    for (address, value) in [
        (MANAGER + 0x3e8, 0),
        (MANAGER + 0x3e8, TEMPLATE + 1),
        (BASE + 0x439f400, DB + 1),
        (DB, BASE + 0x41b3ee0),
        (TEMPLATE, BASE + 0x41b49d0),
        (LIST + 8, SENTINEL),
    ] {
        let mut f = selected(1);
        f.word(address, value);
        assert_eq!(observe(&f).unwrap_err().code, "invalid_construction");
    }
    for ordinal in [-1, 0, 2, i32::MAX] {
        let mut f = selected(1);
        f.number(TEMPLATE + 0x2b8, ordinal);
        assert_eq!(observe(&f).unwrap_err().code, "invalid_construction");
        production_not_read(&f, &[TEMPLATE + 0x248]);
    }
    for flag in [0, 2] {
        let mut f = selected(1);
        f.put(TEMPLATE + 0x18, &[flag]);
        assert_eq!(observe(&f).unwrap_err().code, "invalid_construction");
    }
    for key in ["", "not a key", "bad/key", "é"] {
        let mut f = selected(1);
        f.string(TEMPLATE + 0x248, key);
        assert_eq!(observe(&f).unwrap_err().code, "invalid_construction");
    }
    let mut f = selected(1);
    f.word(TEMPLATE + 0x250, 129);
    assert_eq!(observe(&f).unwrap_err().code, "invalid_string");
    let mut f = selected(1);
    f.number(DB + 0x48, 4097);
    assert_eq!(observe(&f).unwrap_err().code, "invalid_construction");
    production_not_read(&f, &[LIST, TEMPLATE]);
}

#[test]
fn railway_mode_requires_both_special_cache_and_canonical_key() {
    for (mode, cache) in [(1, TEMPLATE), (3, SENTINEL), (1, 0), (3, TEMPLATE + 1)] {
        let mut f = selected(mode);
        f.word(DB + 0x3a8, cache);
        assert_eq!(observe(&f).unwrap_err().code, "invalid_construction");
    }
    let mut f = selected(3);
    f.string(TEMPLATE + 0x248, "infrastructure");
    assert_eq!(observe(&f).unwrap_err().code, "invalid_construction");
}

#[test]
fn selection_final_witnesses_reject_changes_even_on_empty_page() {
    let baseline = selected(1);
    observe(&baseline).unwrap();
    for (address, size) in [
        (MANAGER + 0x3f0, 4),
        (MANAGER + 0x3e8, 8),
        (BASE + 0x439f400, 8),
        (DB, 8),
        (DB + 0x40, 16),
        (LIST, 24),
        (TEMPLATE, 8),
        (TEMPLATE + 0x18, 1),
        (TEMPLATE + 0x2b8, 4),
        (TEMPLATE + 0x248, 32),
        (DB + 0x3a8, 8),
    ] {
        let mut bytes = (0..size)
            .map(|i| baseline.data[&(address + i)])
            .collect::<Vec<_>>();
        bytes[0] ^= 1;
        let mut f = selected(1);
        f.fault = Some((
            address,
            baseline.tracked_reads.borrow()[&address] - 1,
            bytes,
        ));
        assert_eq!(
            page_with_options(&f, 1, 1, false, true).unwrap_err().code,
            "torn_read",
            "{address:x}"
        );
    }
    let mut f = selected(0);
    f.fault = Some((MANAGER + 0x3f0, 1, 1i32.to_le_bytes().to_vec()));
    assert_eq!(observe(&f).unwrap_err().code, "torn_read");
    production_not_read(&f, &[MANAGER + 0x3e8]);
}
