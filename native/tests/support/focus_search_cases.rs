use super::*;

const ITEM: u64 = 0x300000;
const CURRENT: u64 = 0x310000;
const CACHE: u64 = 0x320000;
const MATCHES: u64 = 0x330000;

fn fixture() -> Fake {
    let mut f = Fake::new();
    f.array(VIEW + 0x1138, CURRENT, &[ITEM]);
    f.region(ITEM, 0x2b8);
    f.word(ITEM, 0x4249b90);
    f.word(ITEM + 0x2b0, VIEW);
    f.word(ITEM + 0x20, F);
    f.array(VIEW + 0x12f8, CACHE, &[ITEM]);
    // Only each 0x50-byte row's leading item pointer is mapped. Cached text
    // would fault if the production observer tried to read it.
    f.array(VIEW + 0x1310, MATCHES, &[0]);
    f.int(VIEW + 0x1358, 0);
    f
}
fn result(f: &Fake) -> Result<serde_json::Value> {
    own_focus::search_state(f, "GER_rhineland")
}

#[test]
fn search_highlight_joins_own_current_item_cache_and_match_index() {
    let f = fixture();
    let v = result(&f).unwrap();
    let s = &v["search"];
    assert_eq!(s["selected_key"], "GER_rhineland");
    assert_eq!(s["selected_matches_requested"], true);
    assert_eq!(s["selected_in_matches"], true);
    assert_eq!(s["requested_match_count"], 1);
    assert_eq!(s["complete_current_item_coverage"], true);
    assert_eq!(s["query_text_available"], false);
    assert_eq!(s["query_control_binding_available"], false);
    assert_eq!(v["input_authorized"], false);
    assert_eq!(v["observation_paused_game"], false);
    assert!(f.untouched(CACHE + 8, 0x48) && f.untouched(MATCHES + 4, 4));
    assert!(f.untouched(PANEL, 0x1000));
}

#[test]
fn search_empty_unmaterialized_and_unmatched_selection_remain_explicit() {
    let mut f = fixture();
    f.array(VIEW + 0x1310, MATCHES, &[]);
    f.int(VIEW + 0x1358, -1);
    let v = result(&f).unwrap();
    assert!(v["search"]["selected_key"].is_null());
    assert_eq!(v["search"]["match_count"], 0);
    f.int(VIEW + 0x1358, 0);
    let v = result(&f).unwrap();
    assert_eq!(v["search"]["selected_key"], "GER_rhineland");
    assert_eq!(v["search"]["selected_in_matches"], false);
    let mut f = fixture();
    f.word(VIEW + 0x11d8, 0);
    assert_eq!(result(&f).unwrap()["search"]["available"], false);
    assert!(f.untouched(VIEW + 0x12f8, 0x64));
}

#[test]
fn search_foreign_owner_and_cache_pointer_refuse_before_foreign_payload() {
    let mut f = fixture();
    f.word(ITEM + 0x2b0, VIEW + 8);
    assert_eq!(result(&f).unwrap_err().code, "foreign_focus_scope");
    assert!(f.untouched(ITEM + 0x20, 8) && f.untouched(CACHE, 0x50));
    let mut f = fixture();
    f.word(CACHE, 0x440000);
    assert_eq!(result(&f).unwrap_err().code, "invalid_focus_search");
    assert!(f.untouched(0x440000, 0x1000));
    let mut f = fixture();
    f.word(ITEM + 0x20, 0x450000);
    assert_eq!(result(&f).unwrap_err().code, "invalid_focus_search");
    assert!(f.untouched(0x450000, 0x1000));
}

#[test]
fn search_all_rows_validate_before_a_matching_selection_is_reported() {
    for duplicate in [true, false] {
        let mut f = fixture();
        f.int(VIEW + 0x1300, 2);
        f.int(VIEW + 0x1304, 2);
        f.word(CACHE + 0x50, if duplicate { ITEM } else { 0x440000 });
        assert_eq!(result(&f).unwrap_err().code, "invalid_focus_search");
        assert!(f.untouched(MATCHES, 8));
    }
}

#[test]
fn search_invalid_indices_capacities_and_alignment_fail_closed() {
    for (address, value) in [
        (MATCHES, -1),
        (MATCHES, 1),
        (VIEW + 0x1358, -2),
        (VIEW + 0x1358, 1),
        (VIEW + 0x1300, 4097),
        (VIEW + 0x1318, 4097),
    ] {
        let mut f = fixture();
        f.int(address, value);
        assert_eq!(result(&f).unwrap_err().code, "invalid_focus_search");
    }
    for header in [VIEW + 0x12f8, VIEW + 0x1310] {
        let mut f = fixture();
        f.word(header, 0x440001);
        assert_eq!(result(&f).unwrap_err().code, "invalid_focus_search");
        assert!(f.untouched(0x440001, 0x1000));
    }
}

#[test]
fn search_later_duplicate_match_is_not_hidden_by_requested_first_match() {
    let mut f = fixture();
    const SECOND: u64 = 0x340000;
    f.region(SECOND, 0x2b8);
    f.word(SECOND, 0x4249b90);
    f.word(SECOND + 0x2b0, VIEW);
    f.word(SECOND + 0x20, F);
    f.array(VIEW + 0x1138, CURRENT, &[ITEM, SECOND]);
    f.int(VIEW + 0x1300, 2);
    f.int(VIEW + 0x1304, 2);
    f.word(CACHE + 0x50, SECOND);
    f.array(VIEW + 0x1310, MATCHES, &[0, 0]);
    assert_eq!(result(&f).unwrap_err().code, "invalid_focus_search");
}

#[test]
fn search_changed_witnesses_and_final_process_identity_refuse() {
    for address in [
        VIEW + 0x11d8,
        VIEW + 0x12f8,
        VIEW + 0x1304,
        CACHE,
        MATCHES,
        VIEW + 0x1358,
        ITEM + 0x2b0,
    ] {
        let mut f = fixture();
        f.torn = Some(address);
        assert_eq!(result(&f).unwrap_err().code, "torn_read");
    }
    let mut f = fixture();
    f.fail_final = true;
    assert_eq!(result(&f).unwrap_err().code, "ownership_changed");
}

#[test]
fn search_bad_or_absent_key_cannot_read_search_payload() {
    let f = fixture();
    assert_eq!(
        own_focus::search_state(&f, "bad/key").unwrap_err().code,
        "invalid_argument"
    );
    assert!(f.reads.borrow().is_empty());
    assert_eq!(
        own_focus::search_state(&f, "GER_absent").unwrap_err().code,
        "focus_unavailable"
    );
    assert!(f.untouched(VIEW + 0x12f8, 0x64));
}

const EDIT: u64 = 0x350000;
const CONTAINER: u64 = 0x360000;
const CHILDREN: u64 = 0x370000;
const NAMES: u64 = 0x380000;
const LISTENER: u64 = 0x390000;
const QUERY: u64 = 0x3a0000;

fn query_fixture(text: &str) -> Fake {
    let mut f = fixture();
    f.region(EDIT, 0x280);
    f.region(CONTAINER, 0x1000);
    f.region(LISTENER, 0x20);
    f.word(VIEW + 0x1200, EDIT);
    f.word(EDIT, 0x43498b8);
    f.word(EDIT + 0x68, GUI);
    f.word(EDIT + 0x60, CONTAINER);
    f.word(CONTAINER, 0x4347d58);
    f.word(CONTAINER + 0x98, GUI);
    f.array(CONTAINER + 0x9a0, CHILDREN, &[EDIT]);
    f.array(CONTAINER + 0x9b8, NAMES, &[0]);
    f.string(NAMES, "to_find_text");
    f.word(EDIT + 0x80, 0x4349b78);
    f.word(EDIT + 0x88, LISTENER);
    f.word(EDIT + 0x90, LISTENER);
    f.int(EDIT + 0x98, 1);
    f.word(LISTENER, VIEW + 0x678);
    f.word(VIEW + 0x678, 0x424aa30);
    f.word(VIEW + 0x680, VIEW);
    for (offset, member) in [
        (0x688, 0x1510480),
        (0x698, 0x1510480),
        (0x6a8, 0x1510830),
        (0x6b8, 0x1510870),
    ] {
        f.word(VIEW + offset, member);
    }
    let values: Vec<_> = text.chars().map(u32::from).collect();
    let data = if values.len() <= 3 {
        EDIT + 0xc0
    } else {
        QUERY
    };
    f.word(EDIT + 0xb0, data);
    f.word(EDIT + 0xb8, values.len() as u64);
    if data == QUERY {
        f.word(EDIT + 0xc0, values.len() as u64);
    }
    for (index, value) in values.into_iter().chain([0]).enumerate() {
        f.put(data + index as u64 * 4, &value.to_le_bytes());
    }
    f
}
fn query_result(f: &Fake) -> Result<serde_json::Value> {
    own_focus::search_state_with_control(f, "GER_rhineland")
}

#[test]
fn search_type_refusal_identifies_object_without_reading_its_payload() {
    for (object, label, expected, payload, size) in [
        (ITEM, "current_item", 0x4249b90, ITEM + 0x20, 8),
        (EDIT, "query_control", 0x43498b8, EDIT + 0x60, 16),
        (CONTAINER, "query_container", 0x4347d58, CONTAINER + 0x98, 8),
        (EDIT + 0x80, "embedded_editor", 0x4349b78, EDIT + 0x88, 32),
        (
            VIEW + 0x678,
            "own_view_listener",
            0x424aa30,
            VIEW + 0x680,
            8,
        ),
    ] {
        let mut f = query_fixture("");
        f.word(object, 0x43470c8);
        let error = query_result(&f).unwrap_err();
        assert_eq!(error.code, "invalid_focus_type");
        assert_eq!(
            error.message,
            format!("Search {label}: expected_rva=0x{expected:x} observed_rva=0x43470c8")
        );
        assert!(f.untouched(payload, size));
        assert!(f.untouched(0x43470c8, 8));
    }
}

#[test]
fn search_type_diagnostics_hide_outside_section_values_and_never_follow_them() {
    for table in [0, 0x41adddf, 0x436d9f8, u64::MAX] {
        let mut f = query_fixture("");
        f.word(EDIT, table);
        let error = query_result(&f).unwrap_err();
        assert_eq!(error.code, "invalid_focus_type");
        assert_eq!(error.message, "Search query_control: expected_rva=0x43498b8 observed_rva=outside_verified_table_section");
        assert!(f.untouched(EDIT + 0x60, 16));
    }
    let mut f = query_fixture("");
    f.word(EDIT + 0x60, CONTAINER + 1);
    let error = query_result(&f).unwrap_err();
    assert_eq!(error.code, "invalid_focus_type");
    assert_eq!(
        error.message,
        "Search query_container: non-null aligned object required"
    );
    assert!(f.untouched(CONTAINER, 0x1000));
}

#[test]
fn exact_search_query_reads_utf32_only_after_own_name_and_listener_join() {
    for text in ["", "abc", "Å世🦀", "four year plan", "Rhineland"] {
        let f = query_fixture(text);
        let v = query_result(&f).unwrap();
        let c = &v["search"]["query_control"];
        assert_eq!(c["query_text"], text);
        assert_eq!(c["ordinary_listener_binding_verified"], true);
        assert_eq!(v["search"]["query_text_available"], true);
        assert_eq!(c["keyboard_focus_verified"], false);
        assert_eq!(c["input_authorized"], false);
        assert_eq!(c["render_diagnostic"]["available"], false);
        assert!(f.untouched(CACHE + 8, 0x48));
    }
}

#[test]
fn query_is_opt_in_and_unmaterialized_control_is_explicit() {
    let mut f = query_fixture("four year plan");
    result(&f).unwrap();
    assert!(f.untouched(EDIT, 0x280) && f.untouched(QUERY, 64));
    f.word(VIEW + 0x1200, 0);
    let v = query_result(&f).unwrap();
    assert_eq!(v["search"]["query_control"]["available"], false);
    assert_eq!(v["search"]["query_text_available"], false);
}

#[test]
fn query_foreign_or_substituted_binding_refuses_before_text_payload() {
    for (address, value) in [
        (EDIT + 0x68, GUI + 8),
        (CONTAINER + 0x98, GUI + 8),
        (CHILDREN, EDIT + 8),
        (LISTENER, VIEW + 0x680),
        (VIEW + 0x680, VIEW + 8),
        (VIEW + 0x6a8, 0x1510838),
        (VIEW + 0x6b0, 8),
        (EDIT + 0xa0, LISTENER),
    ] {
        let mut f = query_fixture("four year plan");
        f.word(address, value);
        assert!(query_result(&f).is_err());
        assert!(f.untouched(EDIT + 0xb0, 16) && f.untouched(QUERY, 64));
    }
    let mut f = query_fixture("four year plan");
    f.string(NAMES, "wrong_name");
    assert!(query_result(&f).is_err());
    assert!(f.untouched(EDIT + 0xb0, 16));
}

#[test]
fn query_malformed_wide_strings_refuse_without_unbounded_payload() {
    for (address, value) in [
        (EDIT + 0xb8, 129),
        (EDIT + 0xc0, 4097),
        (EDIT + 0xc0, 1),
        (EDIT + 0xb0, QUERY + 1),
        (QUERY, 0xd800),
        (QUERY, 0x110000),
        (QUERY, 0),
        (QUERY + 14 * 4, 1),
    ] {
        let mut f = query_fixture("four year plan");
        f.word(address, value);
        assert_eq!(query_result(&f).unwrap_err().code, "invalid_focus_search");
    }
    let mut f = query_fixture("abc");
    f.word(EDIT + 0xb8, 4);
    assert_eq!(query_result(&f).unwrap_err().code, "invalid_focus_search");
}

#[test]
fn query_all_paired_names_validate_before_text_and_later_duplicates_refuse() {
    let mut f = query_fixture("four year plan");
    f.array(CONTAINER + 0x9a0, CHILDREN, &[EDIT, 0x3b0000]);
    f.int(CONTAINER + 0x9c0, 2);
    f.int(CONTAINER + 0x9c4, 2);
    f.string(NAMES + 32, "to_find_text");
    assert_eq!(query_result(&f).unwrap_err().code, "invalid_focus_search");
    assert!(f.untouched(EDIT + 0xb0, 16) && f.untouched(0x3b0000, 0x1000));
}

#[test]
fn query_binding_text_and_flags_are_witnessed_through_final_identity_check() {
    for address in [
        VIEW + 0x1200,
        EDIT + 0x68,
        EDIT + 0xb0,
        EDIT + 0xb8,
        QUERY,
        LISTENER,
        VIEW + 0x6a8,
        EDIT + 0x271,
    ] {
        let mut f = query_fixture("four year plan");
        f.torn = Some(address);
        assert_eq!(query_result(&f).unwrap_err().code, "torn_read");
    }
    let mut f = query_fixture("four year plan");
    f.fail_final = true;
    assert_eq!(query_result(&f).unwrap_err().code, "ownership_changed");
}
