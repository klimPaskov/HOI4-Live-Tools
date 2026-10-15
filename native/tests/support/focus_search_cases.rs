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
