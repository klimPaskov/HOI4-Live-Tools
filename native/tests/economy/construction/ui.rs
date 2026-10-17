use super::*;
use hoi4_live_native::construction::page_with_options;
#[path = "selection.rs"]
mod selection;

const REGISTRY: u64 = 0x110000;
const MANAGER: u64 = 0x111000;
const PANEL: u64 = 0x112000;
const ROOT: u64 = 0x114000;
const ITEMS: u64 = 0x116000;
const ROW: u64 = 0x117000;
const WINDOW: u64 = 0x119000;
const POINTERS: u64 = 0x11b000;
const NAMES: u64 = 0x11c000;
const TEXT: u64 = 0x11d000;
const HANDLE: u64 = 0xffff00010000003b;

fn visible_window(f: &mut Fixture, p: u64) {
    f.word(p, BASE + 0x4347d58);
    f.word(p + 0x98, REGISTRY);
    f.put(p + 0xa5, &[8]);
    f.number(p + 0xeb0, 0);
    f.number(p + 0xef8, 0);
}
fn fixture_ui() -> Fixture {
    let mut f = locations::located();
    f.word(0x324f8, REGISTRY);
    f.word(REGISTRY, BASE + 0x42736f8);
    f.word(0x326b8, MANAGER);
    f.word(MANAGER + 0x180, 0x32000);
    f.number(MANAGER + 0x3f0, 0);
    f.word(MANAGER + 0x150, PANEL);
    f.word(PANEL, BASE + 0x43101d8);
    f.word(PANEL + 0x2f0, 0x32000);
    f.word(PANEL + 0x2f8, ROOT);
    visible_window(&mut f, ROOT);
    f.array(PANEL + 0x308, ITEMS, 1);
    f.word(ITEMS, ROW);
    f.word(ROW, BASE + 0x4265548);
    f.word(ROW + 0x538, REGISTRY);
    f.word(ROW + 0x540, HANDLE);
    f.word(ROW + 0xce8, HANDLE);
    f.word(ROW + 0x10, WINDOW);
    visible_window(&mut f, WINDOW);
    f.array(WINDOW + 0xa60, POINTERS, 1);
    f.array(WINDOW + 0xa78, NAMES, 1);
    f.word(POINTERS, TEXT);
    f.string(NAMES, "location");
    f.word(TEXT, BASE + 0x434c8a0);
    f.word(TEXT + 0x68, REGISTRY);
    f.string(TEXT + 0x110, "Brandenburg");
    f
}
fn observe(f: &Fixture) -> Result<serde_json::Value> {
    page_with_options(f, 0, 32, false, true)
}
#[test]
fn ui_joins_full_own_handle_and_location_but_does_not_claim_fresh_text_or_authority() {
    let v = observe(&fixture_ui()).unwrap();
    assert_eq!(v["location_requested"], true);
    let ui = &v["ui"];
    assert_eq!(ui["available"], true);
    assert_eq!(ui["joined_row_count"], 1);
    assert_eq!(ui["rows"][0]["line_handle"], format!("0x{HANDLE:016x}"));
    assert_eq!(ui["rows"][0]["building_id"], "infrastructure");
    assert_eq!(ui["rows"][0]["location"]["state_id"], 1);
    assert_eq!(ui["rows"][0]["displayed_location_text"], "Brandenburg");
    for field in [
        "display_text_freshness_verified",
        "canonical_name_agreement_verified",
        "global_modal_clear_verified",
        "effective_routing_verified",
        "input_authorized",
    ] {
        assert_eq!(ui[field], false);
    }
    assert_eq!(ui["live_validation"], "pending");
}
#[test]
fn omitted_ui_avoids_all_gui_roots_and_hidden_panel_avoids_row_payload() {
    let f = fixture_ui();
    assert_eq!(snapshot(&f).unwrap()["ui"]["requested"], false);
    production_not_read(&f, &[0x324f8, 0x326b8, REGISTRY, MANAGER, PANEL, ROW, TEXT]);
    for address in [ROOT + 0xa5, ROOT + 0xeb0, ROOT + 0xef8] {
        let mut f = fixture_ui();
        if address == ROOT + 0xa5 {
            f.put(address, &[0]);
        } else {
            f.number(address, 2);
        }
        assert_eq!(
            observe(&f).unwrap()["ui"]["reason"],
            "root_not_locally_visible"
        );
        assert_eq!(observe(&f).unwrap()["ui"]["row_coverage"], "not_checked");
        production_not_read(&f, &[PANEL + 0x308, ITEMS, ROW]);
    }
    let mut f = fixture_ui();
    f.word(MANAGER + 0x150, 0);
    assert_eq!(observe(&f).unwrap()["ui"]["reason"], "panel_unmaterialized");
}
#[test]
fn unjoined_full_handles_foreign_locations_and_unknown_rows_never_read_text_payload() {
    for handle in [HANDLE ^ (1u64 << 32), 0, 0xf234567812345678] {
        let mut f = fixture_ui();
        f.word(ROW + 0x540, handle);
        f.word(ROW + 0xce8, handle);
        let v = observe(&f).unwrap();
        assert_eq!(v["ui"]["unjoined_row_count"], 1);
        assert_eq!(
            v["ui"]["reason"],
            "ui_collection_checked_no_own_located_rows"
        );
        production_not_read(&f, &[ROW + 0x10, WINDOW, TEXT]);
    }
    let mut f = fixture_ui();
    f.number(0xe00c8, 2);
    assert_eq!(observe(&f).unwrap()["ui"]["unjoined_row_count"], 1);
    production_not_read(&f, &[ROW + 0x10, WINDOW, TEXT]);
    let mut f = fixture_ui();
    f.word(ROW, BASE + 0x4265549);
    assert_eq!(observe(&f).unwrap()["ui"]["unsupported_row_count"], 1);
    production_not_read(&f, &[ROW + 0x540, ROW + 0xce8, ROW + 0x10, WINDOW, TEXT]);
}
#[test]
fn exact_types_owner_links_and_both_handle_copies_are_required() {
    for address in [
        REGISTRY,
        MANAGER + 0x180,
        PANEL,
        PANEL + 0x2f0,
        ROOT,
        ROOT + 0x98,
        ROW + 0x538,
        ROW + 0xce8,
        WINDOW,
        WINDOW + 0x98,
        TEXT,
        TEXT + 0x68,
    ] {
        let mut f = fixture_ui();
        f.word(address, 0xdead0000);
        assert_eq!(
            observe(&f).unwrap_err().code,
            "invalid_construction",
            "{address:x}"
        );
    }
}
#[test]
fn paired_text_arrays_reject_mismatch_duplicates_and_oversized_names() {
    for address in [WINDOW + 0xa68, WINDOW + 0xa80, PANEL + 0x310] {
        let mut f = fixture_ui();
        f.number(address, 2049);
        assert_eq!(observe(&f).unwrap_err().code, "invalid_construction");
    }
    let mut f = fixture_ui();
    f.array(WINDOW + 0xa78, NAMES, 0);
    assert_eq!(observe(&f).unwrap_err().code, "invalid_construction");
    let mut f = fixture_ui();
    f.array(WINDOW + 0xa60, POINTERS, 2);
    f.array(WINDOW + 0xa78, NAMES, 2);
    f.word(POINTERS + 8, TEXT + 0x1000);
    f.string(NAMES + 32, "location");
    assert_eq!(observe(&f).unwrap_err().code, "invalid_construction");
    let mut f = fixture_ui();
    f.word(NAMES + 8, 129);
    assert_eq!(observe(&f).unwrap_err().code, "invalid_string");
    let mut f = fixture_ui();
    f.string(NAMES, "other");
    assert!(observe(&f).unwrap()["ui"]["rows"][0]["displayed_location_text"].is_null());
    production_not_read(&f, &[TEXT]);
}
#[test]
fn duplicate_own_rows_and_duplicate_array_members_are_rejected() {
    for duplicate_pointer in [false, true] {
        let mut f = fixture_ui();
        f.array(PANEL + 0x308, ITEMS, 2);
        let second = if duplicate_pointer { ROW } else { 0x120000 };
        f.word(ITEMS + 8, second);
        f.word(second, BASE + 0x4265548);
        f.word(second + 0x538, REGISTRY);
        f.word(second + 0x540, HANDLE);
        f.word(second + 0xce8, HANDLE);
        assert_eq!(observe(&f).unwrap_err().code, "invalid_construction");
    }
}
#[test]
fn final_witness_covers_gui_owners_handles_names_and_displayed_text_even_out_of_page() {
    let baseline = fixture_ui();
    observe(&baseline).unwrap();
    for (address, size) in [
        (0x324f8, 8),
        (0x326b8, 8),
        (REGISTRY, 8),
        (MANAGER + 0x180, 8),
        (MANAGER + 0x150, 8),
        (PANEL, 8),
        (PANEL + 0x2f0, 8),
        (PANEL + 0x2f8, 8),
        (ROOT, 8),
        (ROOT + 0x98, 8),
        (ROOT + 0xa5, 1),
        (ROOT + 0xeb0, 4),
        (ROOT + 0xef8, 4),
        (PANEL + 0x308, 16),
        (ITEMS, 8),
        (ROW, 8),
        (ROW + 0x538, 8),
        (ROW + 0x540, 8),
        (ROW + 0xce8, 8),
        (ROW + 0x10, 8),
        (WINDOW, 8),
        (WINDOW + 0x98, 8),
        (WINDOW + 0xa5, 1),
        (WINDOW + 0xeb0, 4),
        (WINDOW + 0xef8, 4),
        (WINDOW + 0xa60, 16),
        (WINDOW + 0xa78, 16),
        (POINTERS, 8),
        (NAMES, 32),
        (TEXT, 8),
        (TEXT + 0x68, 8),
        (TEXT + 0x110, 32),
    ] {
        let mut bytes = (0..size)
            .map(|i| baseline.data[&(address + i as u64)])
            .collect::<Vec<_>>();
        bytes[0] ^= 1;
        let mut f = fixture_ui();
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
    let v = page_with_options(&fixture_ui(), 1, 1, false, true).unwrap();
    assert_eq!(v["ui"]["joined_row_count"], 1);
    assert!(v["ui"]["rows"].as_array().unwrap().is_empty());
}
#[test]
fn ui_argument_is_boolean_and_only_accepted_by_construction() {
    let s = hoi4_live_native::protocol::Server {
        process: None,
        unavailable: None,
    };
    for value in [json!(null), json!(1), json!("true"), json!([])] {
        let v=s.handle(json!({"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"hoi4_construction_queue","arguments":{"include_ui":value}}})).unwrap();
        assert!(v.to_string().contains("invalid_arguments"));
    }
    for value in [true, false] {
        let v=s.handle(json!({"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"hoi4_construction_queue","arguments":{"include_ui":value}}})).unwrap();
        assert!(!v.to_string().contains("invalid_arguments"));
        let v=s.handle(json!({"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"hoi4_production_lines","arguments":{"include_ui":value}}})).unwrap();
        assert!(v.to_string().contains("invalid_arguments"));
    }
}

#[test]
fn heap_utf8_text_is_bounded_and_payload_is_witnessed() {
    let text = "A deliberately longer location — Łódź";
    let mut f = fixture_ui();
    f.word(TEXT + 0x110, 0x130000);
    f.word(TEXT + 0x118, text.len() as u64);
    f.put(0x130000, text.as_bytes());
    assert_eq!(
        observe(&f).unwrap()["ui"]["rows"][0]["displayed_location_text"],
        text
    );
    f.tracked_reads.borrow_mut().clear();
    let mut bytes = text.as_bytes().to_vec();
    bytes[0] = b'B';
    f.fault = Some((0x130000, 1, bytes));
    assert_eq!(observe(&f).unwrap_err().code, "torn_read");
    let mut f = fixture_ui();
    f.word(TEXT + 0x118, 513);
    assert_eq!(observe(&f).unwrap_err().code, "invalid_string");
    let mut f = fixture_ui();
    f.put(TEXT + 0x120, &[0xff]);
    assert_eq!(observe(&f).unwrap_err().code, "invalid_string");
}

fn repeated_unrelated_names(position: usize) -> Fixture {
    let mut f = fixture_ui();
    let mut entries = vec![
        ("shared", 0x140000),
        ("", 0x140100),
        ("shared", 0x140200),
        ("", 0x140300),
    ];
    entries.insert(position, ("location", TEXT));
    f.array(WINDOW + 0xa60, POINTERS, entries.len() as i32);
    f.array(WINDOW + 0xa78, NAMES, entries.len() as i32);
    for (i, (name, pointer)) in entries.into_iter().enumerate() {
        f.word(POINTERS + i as u64 * 8, pointer);
        f.string(NAMES + i as u64 * 32, name);
    }
    f
}
#[test]
fn location_lookup_accepts_repeated_unrelated_names_without_following_their_payload() {
    for position in [0, 2, 4] {
        let f = repeated_unrelated_names(position);
        assert_eq!(
            observe(&f).unwrap()["ui"]["rows"][0]["displayed_location_text"],
            "Brandenburg"
        );
        production_not_read(&f, &[0x140000, 0x140100, 0x140200, 0x140300]);
    }
}
#[test]
fn later_unrelated_text_name_still_validates_and_participates_in_final_witnesses() {
    let mut f = repeated_unrelated_names(0);
    f.word(NAMES + 128 + 8, 129);
    assert_eq!(observe(&f).unwrap_err().code, "invalid_string");
    production_not_read(&f, &[TEXT]);
    let baseline = repeated_unrelated_names(0);
    observe(&baseline).unwrap();
    let address = NAMES + 128;
    let mut bytes = (0..32)
        .map(|i| baseline.data[&(address + i)])
        .collect::<Vec<_>>();
    bytes[0] ^= 1;
    let mut f = repeated_unrelated_names(0);
    f.fault = Some((
        address,
        baseline.tracked_reads.borrow()[&address] - 1,
        bytes,
    ));
    assert_eq!(observe(&f).unwrap_err().code, "torn_read");
}
