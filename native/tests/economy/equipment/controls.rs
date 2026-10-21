use super::*;
use hoi4_live_native::production_lines::control_snapshot::{snapshot as controls, validate_target};
const HANDLE: &str = "0xffff000100000038";
const KEY: &str = "infantry_eq_1";
const REGISTRY: u64 = 0x200000;
const MANAGER: u64 = 0x210000;
const VIEW: u64 = 0x220000;
const ROOT: u64 = 0x230000;
const ROWS: u64 = 0x240000;
const ROW: u64 = 0x250000;
const WINDOW: u64 = 0x260000;

fn window(f: &mut Fixture, address: u64, shown: bool) {
    f.region(address, 0xf00);
    f.word(address, BASE + 0x4347d58);
    f.word(address + 0x98, REGISTRY);
    f.put(address + 0xa5, &[if shown { 8 } else { 0 }]);
}

fn materialized_view() -> Fixture {
    let mut f = unavailable_view();
    f.number(MANAGER + 0x3d8, 2);
    f.word(MANAGER + 0xd8, VIEW);
    f.region(VIEW, 0x1600);
    f.word(VIEW, BASE + 0x4314180);
    f.word(VIEW + 0x2f0, 0x32000);
    f.word(VIEW + 0x2f8, ROOT);
    window(&mut f, ROOT, true);
    for (offset, dialog, root) in [(0x1530, 0x270000, 0x280000), (0x1538, 0x290000, 0x2a0000)] {
        f.word(VIEW + offset, dialog);
        f.region(dialog, 0x808);
        f.word(dialog, BASE + 0x4321298);
        f.word(dialog + 0x7f0, 0x32000);
        f.word(dialog + 0x7f8, VIEW);
        f.word(dialog + 0x800, root);
        window(&mut f, root, false);
    }
    f.array(VIEW + 0x14f0, ROWS, 1);
    f.word(ROWS, ROW);
    f.region(ROW, 0x1030);
    f.word(ROW, BASE + 0x4264f48);
    f.word(ROW + 0x538, REGISTRY);
    f.word(ROW + 0x590, VIEW);
    f.word(ROW + 0xfd0, 0xffff000100000038);
    f.word(ROW + 0x10, WINDOW);
    window(&mut f, WINDOW, true);
    f
}

fn unavailable_view() -> Fixture {
    let mut f = fixture();
    f.word(0x324f8, REGISTRY);
    f.word(REGISTRY, BASE + 0x42736f8);
    f.word(0x326b8, MANAGER);
    f.word(MANAGER + 0x180, 0x32000);
    f.number(MANAGER + 0x3d8, 0);
    f.word(MANAGER + 0xd8, 0);
    f
}

#[test]
fn target_is_full_typed_opaque_handle_not_index_or_low_word() {
    for handle in [
        "0",
        "0x38",
        "ffff000100000038",
        "0Xffff000100000038",
        "0xFFFF000100000038",
        "0xffff0001000000390",
        "0xffff00010000003a",
        "0x0000000000000000",
        "0xzzzz000100000038",
    ] {
        assert_eq!(
            validate_target(handle, KEY).unwrap_err().code,
            "invalid_arguments"
        );
    }
    for key in ["", "bad/key", "with space", "équipement"] {
        assert_eq!(
            validate_target(HANDLE, key).unwrap_err().code,
            "invalid_arguments"
        );
    }
    assert_eq!(
        validate_target(HANDLE, &"x".repeat(129)).unwrap_err().code,
        "invalid_arguments"
    );
    assert_eq!(validate_target(HANDLE, KEY).unwrap(), 0xffff000100000038);
}

#[test]
fn unmaterialized_view_is_fresh_read_only_unavailability_after_full_line_join() {
    let f = unavailable_view();
    let value = controls(&f, HANDLE, KEY, false).unwrap();
    assert_eq!(value["available"], false);
    assert_eq!(value["reason"], "production_view_unmaterialized");
    assert_eq!(value["input_authorized"], false);
    assert_eq!(value["observation_paused_game"], false);
    assert_eq!(value["line_handle"], HANDLE);
    assert_eq!(value["equipment_id"], KEY);
    assert_eq!(value["requested_factories"], 15);
    assert_eq!(value["collection_count"], 3);
    assert_eq!(value["line_kind"], "military");
    assert!(!value.to_string().contains("2097152"));
    production_not_read(&f, &[BASE + 0x45778e0]);
}

#[test]
fn wrong_generation_or_equipment_never_reads_ui() {
    for (handle, key) in [("0xffff000200000038", KEY), (HANDLE, "another_equipment")] {
        let f = unavailable_view();
        assert_eq!(
            controls(&f, handle, key, false).unwrap_err().code,
            "production_target_unavailable"
        );
        production_not_read(&f, &[0x324f8, REGISTRY, 0x326b8, MANAGER + 0x180]);
    }
}

#[test]
fn full_collection_validation_precedes_selected_ui_even_for_other_lines() {
    for (address, replacement, code) in [
        (0x82020, 0xdead0000, "invalid_production_owner"),
        (0x82008, 0xffff000100000038, "invalid_production_handle"),
        (0x70010, 0x81000, "invalid_production_array"),
    ] {
        let mut f = unavailable_view();
        f.word(address, replacement);
        assert_eq!(controls(&f, HANDLE, KEY, false).unwrap_err().code, code);
        production_not_read(&f, &[0x324f8, REGISTRY, 0x326b8]);
    }
}

#[test]
fn selected_unknown_equipment_or_anomalous_counter_does_not_read_ui() {
    for (address, value) in [(0x80088, 0), (0x800e8, 1_000_001)] {
        let mut f = unavailable_view();
        if address == 0x80088 {
            f.word(address, value);
        } else {
            f.number(address, value as i32);
        }
        assert_eq!(
            controls(&f, HANDLE, KEY, false).unwrap_err().code,
            "production_target_unavailable"
        );
        production_not_read(&f, &[0x324f8, REGISTRY, 0x326b8]);
    }
}

#[test]
fn unrelated_line_changes_or_process_replacement_reject_entire_control_observation() {
    let mut f = unavailable_view();
    f.fault = Some((0x820e8, 1, 16i32.to_le_bytes().to_vec()));
    assert_eq!(
        controls(&f, HANDLE, KEY, false).unwrap_err().code,
        "torn_read"
    );
    let mut f = unavailable_view();
    f.replaced = true;
    assert_eq!(
        controls(&f, HANDLE, KEY, false).unwrap_err().code,
        "ownership_changed"
    );
}

#[test]
fn typed_row_association_and_scoped_dialogs_do_not_invent_controls_or_input() {
    let f = materialized_view();
    let value = controls(&f, HANDLE, KEY, false).unwrap();
    assert_eq!(value["available"], true);
    assert_eq!(value["locally_ready"], true);
    assert_eq!(value["controls"]["add"]["binding_available"], false);
    assert_eq!(value["controls"]["subtract"]["binding_available"], false);
    assert_eq!(value["global_modal_clear_verified"], false);
    assert_eq!(value["input_authorized"], false);
    assert!(value.get("scope").is_none());
    assert!(value.get("input_geometry").is_none());
}

#[test]
fn production_uses_manager_mode_two_not_view_constructor_argument_five() {
    for mode in [2, 5, 17, 23] {
        let mut f = materialized_view();
        f.number(MANAGER + 0x3d8, mode);
        let value = controls(&f, HANDLE, KEY, false).unwrap();
        assert_eq!(value["mode_raw"], mode);
        assert_eq!(value["locally_ready"], mode == 2);
    }
}

#[test]
fn unjoined_ui_handle_or_unknown_row_type_cannot_traverse_row_window() {
    for (field, value) in [(ROW + 0xfd0, 0xffff000200000038), (ROW, BASE + 0x4264f50)] {
        let mut f = materialized_view();
        f.word(field, value);
        let value = controls(&f, HANDLE, KEY, false).unwrap();
        assert_eq!(value["reason"], "own_production_row_unmaterialized");
        production_not_read(&f, &[ROW + 0x10, WINDOW]);
    }
}

#[test]
fn foreign_row_owner_and_duplicate_members_fail_before_target_payload() {
    let mut f = materialized_view();
    f.word(ROW + 0x590, VIEW + 8);
    assert_eq!(
        controls(&f, HANDLE, KEY, false).unwrap_err().code,
        "invalid_production_controls"
    );
    production_not_read(&f, &[ROW + 0xfd0, ROW + 0x10, WINDOW]);
    let mut f = materialized_view();
    f.array(VIEW + 0x14f0, ROWS, 2);
    f.word(ROWS + 8, ROW);
    assert_eq!(
        controls(&f, HANDLE, KEY, false).unwrap_err().code,
        "invalid_production_controls"
    );
    production_not_read(&f, &[ROW, ROW + 0xfd0]);
}

#[test]
fn visible_missing_or_moving_dialog_never_clears_scoped_blockers() {
    for (field, bytes) in [
        (VIEW + 0x1530, 0u64.to_le_bytes().to_vec()),
        (0x270800, 0u64.to_le_bytes().to_vec()),
        (0x2800a5, vec![8]),
        (0x280eb0, 1i32.to_le_bytes().to_vec()),
    ] {
        let mut f = materialized_view();
        f.put(field, &bytes);
        let v = controls(&f, HANDLE, KEY, false).unwrap();
        assert_eq!(v["locally_ready"], false);
        assert_eq!(
            v["scoped_blockers"]["production_equipment_selections_hidden_settled"],
            false
        );
    }
}

#[test]
fn final_witness_rejects_changed_ui_chain_and_local_state() {
    for (field, bytes) in [
        (MANAGER + 0xd8, (VIEW + 8).to_le_bytes().to_vec()),
        (ROW + 0xfd0, 0xffff000200000038u64.to_le_bytes().to_vec()),
        (ROW + 0x10, (WINDOW + 8).to_le_bytes().to_vec()),
        (0x2800a5, vec![8]),
    ] {
        let mut f = materialized_view();
        f.fault = Some((field, 1, bytes));
        assert_eq!(
            controls(&f, HANDLE, KEY, false).unwrap_err().code,
            "torn_read"
        );
    }
}

#[test]
fn replaced_variant_is_rejected_without_following_the_unvalidated_pointer() {
    let mut f = unavailable_view();
    f.fault = Some((0x80088, 1, 0xdead0000u64.to_le_bytes().to_vec()));
    assert_eq!(
        controls(&f, HANDLE, KEY, false).unwrap_err().code,
        "torn_read"
    );
    production_not_read(&f, &[0xdead0000, 0xdead03f0]);
}
