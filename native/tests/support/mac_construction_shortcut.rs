use super::*;
use hoi4_live_native::construction::mac_panel::{snapshot_with_options, Options};

#[path = "mac_construction_receivers.rs"]
mod receivers;

const KEYS: u64 = 0x550000;
const SOURCE: u64 = 0x560000;
const PREVIOUS: u64 = 0x570000;
const PREVIOUS_SPRITE: u64 = 0x580000;
const PREVIOUS_KEYS: u64 = 0x590000;
const POSITION: u64 = 0x5a0000;
const PREVIOUS_POSITION: u64 = 0x5b0000;

fn bindings(f: &mut MacFixture, button: u64, data: u64, keys: &[i32]) {
    f.word(button + 0xa0, data);
    f.int(button + 0xa8, keys.len() as i32);
    f.int(button + 0xac, keys.len() as i32);
    for (i, key) in keys.iter().enumerate() {
        let record = data + i as u64 * 40;
        f.put(record, &[0; 40]);
        f.int(record, *key);
    }
}
fn target(f: &mut MacFixture, button: u64, sprite: u64) {
    for (a, v) in [
        (button, BASE + 0x34aafa0),
        (button + 0x58, GUI),
        (button + 0xd0, GRAPHICS),
        (button + 0xd8, sprite),
        (button + 0xc0, 0),
        (button + 0x178, 0),
        (sprite, BASE + 0x34bdb10),
        (sprite + 0x138, button),
    ] {
        f.word(a, v);
    }
    f.int(button + 0x64, 0x804);
    f.int(button + 0x9c, 0);
    f.put(button + 0xcc, &[8]);
    f.put(button + 0x1d1, &[0]);
}
fn keyboard() -> MacFixture {
    let mut f = fixture(2);
    f.word(BASE + 0x35dc058, GUI);
    f.word(GUI + 0x98, GRAPHICS);
    f.word(GRAPHICS, BASE + 0x33dafa8);
    for a in [GUI + 0x138, GUI + 0x210] {
        f.word(a, 0);
    }
    f.put(GUI + 0x10, &[1, 0, 0]);
    f.put(GUI + 0x130, &[0]);
    target(&mut f, BUTTON, SPRITE);
    target(&mut f, PREVIOUS, PREVIOUS_SPRITE);
    f.word(AGENCY + 0x1238, POSITION);
    parent(&mut f, BUTTON, SPRITE, POSITION);
    bindings(&mut f, BUTTON, KEYS, &[116]);
    bindings(&mut f, PREVIOUS, PREVIOUS_KEYS, &[121]);
    // Reverse order: a known nonmatching predecessor, our named T button, then
    // an unmapped later pointer whose payload must never be followed.
    f.array(GRAPHICS + 0x68, SOURCE, &[FOREIGN, SPRITE, PREVIOUS_SPRITE]);
    f
}
fn read(f: &MacFixture) -> Result<Value> {
    snapshot_with_options(
        f,
        Options {
            shortcut: true,
            ..Options::default()
        },
    )
}
fn reason(f: &MacFixture) -> String {
    read(f).unwrap()["opener"]["shortcut"]["reason"]
        .as_str()
        .unwrap()
        .into()
}
fn parent(f: &mut MacFixture, button: u64, sprite: u64, position: u64) {
    f.word(button + 0x178, position);
    f.word(sprite + 0x108, position);
    f.word(position, BASE + 0x34b0428);
    f.put(position + 0x128, &[1]);
}

#[test]
fn ordinary_t_is_current_and_reverse_ordered_without_geometry_or_later_payload() {
    let f = keyboard();
    let v = read(&f).unwrap();
    let k = &v["opener"]["shortcut"];
    assert_eq!(k["reason"], "ordinary_t_route_observed");
    assert_eq!(k["source_count"], 3);
    assert_eq!(k["inspected_receiver_count"], 2);
    assert_eq!(k["input_authorized"], false);
    assert_eq!(k["cache_completion_verified"], false);
    assert_eq!(k["delivery_verified"], false);
    not_read(
        &f,
        &[
            FOREIGN,
            GRAPHICS + 0x170,
            GRAPHICS + 0x50,
            GUI + 0x120,
            BASE + 0x35dbf60,
        ],
    );
}
#[test]
fn shortcut_payload_is_opt_in_and_selected_panel_never_requires_a_toggle() {
    let f = fixture(2);
    snapshot(&f, false).unwrap();
    not_read(&f, &[BASE + 0x35dc058, BUTTON + 0xa0]);
    for shown in [0, 8] {
        let mut f = fixture(17);
        f.put(ROOT + 0x95, &[shown]);
        let v = read(&f).unwrap();
        assert_eq!(v["reason"], "already_selected_no_toggle");
        assert!(v["opener"].is_null());
        not_read(&f, &[TOPBAR, BASE + 0x35dc058, BUTTON]);
    }
}
#[test]
fn focus_and_dispatch_modes_refuse_without_receiver_or_collection_payload() {
    for a in [GUI + 0x138, GUI + 0x210] {
        let mut f = keyboard();
        f.word(a, FOREIGN);
        assert_eq!(reason(&f), "focused_keyboard_or_text_receiver");
        not_read(&f, &[FOREIGN, GRAPHICS + 0x68, SOURCE, KEYS]);
    }
    for (a, v) in [
        (GUI + 0x10, 0),
        (GUI + 0x11, 1),
        (GUI + 0x12, 1),
        (GUI + 0x130, 1),
    ] {
        let mut f = keyboard();
        f.put(a, &[v]);
        assert_eq!(reason(&f), "unsupported_keyboard_dispatch_mode");
        not_read(&f, &[GRAPHICS + 0x68, SOURCE, KEYS]);
    }
}
#[test]
fn prior_matching_or_consuming_button_refuses_even_when_named_target_matches() {
    for matching in [false, true] {
        let mut f = keyboard();
        if matching {
            f.int(PREVIOUS_KEYS, 116);
        } else {
            f.int(PREVIOUS + 0x64, 0x806);
        }
        assert_eq!(
            reason(&f),
            "preceding_keyboard_target_can_handle_or_consume_t"
        );
        not_read(&f, &[KEYS]);
    }
    let mut f = keyboard();
    f.put(PREVIOUS + 0xcc, &[0]);
    f.int(PREVIOUS + 0x64, 0x806);
    assert_eq!(reason(&f), "ordinary_t_route_observed");
    not_read(&f, &[PREVIOUS_KEYS]);
}
#[test]
fn current_target_requires_exact_unmodified_single_t_not_disk_configuration() {
    for (offset, value) in [(0, 117), (8, 1), (28, 1)] {
        let mut f = keyboard();
        f.int(KEYS + offset, value);
        if offset == 28 {
            f.word(KEYS + 16, FOREIGN);
            f.int(KEYS + 24, 1);
        }
        assert_eq!(reason(&f), "named_button_requires_one_plain_t_binding");
        not_read(&f, &[FOREIGN]);
    }
    let mut f = keyboard();
    bindings(&mut f, BUTTON, KEYS, &[116, 116]);
    assert_eq!(reason(&f), "named_button_requires_one_plain_t_binding");
    let mut f = keyboard();
    f.put(BUTTON + 0xcc, &[0]);
    assert_eq!(reason(&f), "named_keyboard_shortcut_disabled");
}
#[test]
fn unsupported_earlier_types_and_foreign_owners_are_not_silently_skipped() {
    for (a, value) in [
        (PREVIOUS_SPRITE, BASE + 0x34ac070),
        (PREVIOUS, BASE + 0x34ac0b0),
        (PREVIOUS + 0x58, FOREIGN),
        (PREVIOUS + 0xd0, FOREIGN),
        (PREVIOUS + 0xd8, FOREIGN),
        (BASE + 0x35dc058, FOREIGN),
        (BUTTON + 0xd0, FOREIGN),
        (SPRITE + 0x138, FOREIGN),
    ] {
        let mut f = keyboard();
        f.word(a, value);
        assert!(read(&f).is_err());
        not_read(&f, &[FOREIGN, PREVIOUS_KEYS, KEYS]);
    }
}
#[test]
fn handler_wrapper_parent_and_pressed_predecessors_refuse_before_shortcuts() {
    let mut f = keyboard();
    f.word(PREVIOUS + 0xc0, FOREIGN);
    assert_eq!(reason(&f), "unsupported_keyboard_button_gate");
    not_read(&f, &[FOREIGN, PREVIOUS_KEYS, KEYS]);
    for state in [1, -1, 4] {
        let mut f = keyboard();
        f.int(PREVIOUS + 0x9c, state);
        assert_eq!(reason(&f), "unsupported_keyboard_button_gate");
        not_read(&f, &[PREVIOUS_KEYS]);
    }
    let mut f = keyboard();
    f.put(PREVIOUS + 0x1d1, &[1]);
    assert_eq!(reason(&f), "unsupported_keyboard_button_gate");
}
#[test]
fn missing_duplicate_and_bounded_sources_cannot_claim_delivery() {
    for sources in [vec![PREVIOUS_SPRITE], vec![SPRITE, PREVIOUS_SPRITE, SPRITE]] {
        let mut f = keyboard();
        f.array(GRAPHICS + 0x68, SOURCE, &sources);
        assert_eq!(reason(&f), "named_keyboard_renderer_missing_or_duplicated");
        not_read(&f, &[PREVIOUS_KEYS, KEYS]);
    }
    for (a, value) in [
        (GRAPHICS + 0x70, 20_001),
        (BUTTON + 0xa8, 129),
        (KEYS + 24, -1),
        (KEYS + 28, 1),
    ] {
        let mut f = keyboard();
        f.int(a, value);
        assert!(read(&f).is_err());
    }
}
#[test]
fn reserved_keyboard_capacity_never_expands_active_membership_reads() {
    for capacity in [10_368, 20_000] {
        let mut f = keyboard();
        f.int(GRAPHICS + 0x70, capacity);
        let value = read(&f).unwrap();
        assert_eq!(value["opener"]["shortcut"]["source_count"], 3);
        assert_eq!(value["opener"]["shortcut"]["ordinary_route_observed"], true);
        not_read(&f, &[SOURCE + 24, FOREIGN]);
    }
    let mut f = keyboard();
    f.int(GRAPHICS + 0x70, 10_368);
    f.int(GRAPHICS + 0x74, 4097);
    assert_eq!(read(&f).unwrap_err().code, "invalid_construction");
    not_read(&f, &[SOURCE, PREVIOUS_KEYS, KEYS]);
}
#[test]
fn full_membership_and_each_inspected_binding_survive_final_witness() {
    for a in [
        SOURCE,
        SOURCE + 16,
        GRAPHICS + 0x70,
        KEYS + 39,
        PREVIOUS_KEYS + 4,
        PREVIOUS + 0x64,
        GUI + 0x138,
        BASE + 0x35dc058,
    ] {
        let f = keyboard();
        f.final_fault.set(Some(a));
        assert_eq!(read(&f).unwrap_err().code, "torn_read", "{a:x}");
    }
}
#[test]
fn shortcut_option_rejects_non_boolean_before_process_access() {
    let server = Server {
        process: None,
        unavailable: None,
    };
    for v in [Value::Null, json!(1), json!("true"), json!({})] {
        let r = server
            .handle(
                json!({"jsonrpc":"2.0","id":1,"method":"tools/call","params":{
            "name":"hoi4_construction_panel_probe","arguments":{"include_shortcut":v}}}),
            )
            .unwrap();
        assert!(r.to_string().contains("invalid_arguments"));
    }
}
