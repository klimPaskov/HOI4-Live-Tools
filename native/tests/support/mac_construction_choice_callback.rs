use super::*;
use hoi4_live_native::construction::mac_ui::{snapshot_with_options, Options};
use serde_json::Value;

const NODE: u64 = 0xd00000;
const FUNCTION: u64 = 0xd10000;

fn bound(count: usize) -> MacFixture {
    let mut f = fixture(true, count);
    let item = ITEMS + (count as u64 - 1) * 0x400;
    let glue = item + 0x30;
    for (field, value) in [
        (BUTTON + 0x70, BASE + 0x34ab360),
        (BUTTON + 0x78, NODE),
        (BUTTON + 0x80, NODE),
        (BUTTON + 0x90, 0),
        (BUTTON + 0xc0, 0),
        (NODE, glue),
        (NODE + 8, 0),
        (NODE + 0x10, 0),
        (glue, BASE + 0x343ade8),
        (glue + 0x30, FUNCTION),
        (FUNCTION, BASE + 0x343ae78),
        (FUNCTION + 8, BASE + 0x1ed5640),
        (FUNCTION + 0x10, 0),
        (FUNCTION + 0x18, item),
    ] {
        f.word(field, value);
    }
    f.int(BUTTON + 0x88, 1);
    f.put(BUTTON + 0x8c, &[0]);
    f.put(BUTTON + 0xcc, &[0]);
    f.put(NODE + 0x18, &[0]);
    for index in 2..=20 {
        f.word(glue + index * 0x30, 0);
    }
    f
}
fn read(f: &MacFixture) -> Result<Value> {
    snapshot_with_options(f, "arms_factory", 1, Options { callback: true })
}

#[test]
fn default_and_explicit_false_keep_callback_payload_unread() {
    let f = fixture(true, 1); // Callback fields are intentionally unmapped.
    let a = observe(&f, "arms_factory", 1).unwrap();
    let reads = f.reads.borrow().clone();
    f.reads.borrow_mut().clear();
    let b = snapshot_with_options(&f, "arms_factory", 1, Options::default()).unwrap();
    assert_eq!(*f.reads.borrow(), reads);
    assert_eq!(a["choices"], b["choices"]);
    assert_eq!(a["callback_binding_verified"], false);
    assert!(a["choices"]["target"]["callback"].is_null());
    not_read(&f, &[BUTTON + 0x70, BUTTON + 0xcc, ITEMS + 0x30]);
}

#[test]
fn complete_choices_bind_only_requested_item_without_exposing_addresses_or_granting_input() {
    let f = bound(70);
    let v = read(&f).unwrap();
    assert_eq!(v["choices"]["item_count"], 70);
    assert_eq!(v["callback_binding_verified"], true);
    let callback = &v["choices"]["target"]["callback"];
    assert_eq!(callback["receiver_matches_requested_item"], true);
    assert_eq!(callback["observer_count"], 1);
    assert_eq!(callback["disable_requested"], false);
    assert_eq!(callback["prerequisites_evaluated"], false);
    assert_eq!(callback["delivery_verified"], false);
    for name in [
        "input_authorized",
        "geometry_available",
        "placement_eligibility_verified",
        "settled_frame_verified",
    ] {
        assert_eq!(v[name], false, "{name}");
    }
    for i in 0..69 {
        not_read(&f, &[ITEMS + i * 0x400 + 0x10, ITEMS + i * 0x400 + 0x30]);
    }
    not_read(&f, &[BUTTON + 0xd8, FOREIGN]);
    let rendered = v.to_string();
    for address in [NODE, FUNCTION, ITEMS + 69 * 0x400] {
        assert!(!rendered.contains(&address.to_string()));
    }
}

#[test]
fn observer_overrides_or_wrong_count_refuse_before_list_and_capture() {
    for (field, value) in [
        (BUTTON + 0x70, BASE + 0x34ab368),
        (BUTTON + 0x90, FOREIGN),
        (BUTTON + 0xc0, FOREIGN),
    ] {
        let mut f = bound(1);
        f.word(field, value);
        assert_eq!(read(&f).unwrap_err().code, "invalid_construction");
        not_read(&f, &[NODE, ITEMS + 0x30, FUNCTION, FOREIGN]);
    }
    for count in [-1, 0, 2, 128, i32::MAX] {
        let mut f = bound(1);
        f.int(BUTTON + 0x88, count);
        assert_eq!(read(&f).unwrap_err().code, "invalid_construction");
        not_read(&f, &[NODE, ITEMS + 0x30, FUNCTION]);
    }
    let mut f = bound(1);
    f.put(BUTTON + 0x8c, &[1]);
    assert_eq!(read(&f).unwrap_err().code, "invalid_construction");
    not_read(&f, &[NODE, ITEMS + 0x30, FUNCTION]);
}

#[test]
fn incomplete_removed_or_foreign_observer_never_follows_its_payload() {
    for (field, value) in [
        (BUTTON + 0x78, NODE + 1),
        (BUTTON + 0x80, NODE + 0x100),
        (NODE + 8, NODE),
        (NODE + 0x10, NODE + 0x100),
        (NODE, FOREIGN),
    ] {
        let mut f = bound(1);
        f.word(field, value);
        assert_eq!(read(&f).unwrap_err().code, "invalid_construction");
        not_read(&f, &[ITEMS + 0x30, FUNCTION, FOREIGN]);
    }
    let mut f = bound(1);
    f.put(NODE + 0x18, &[1]);
    assert_eq!(read(&f).unwrap_err().code, "invalid_construction");
    not_read(&f, &[ITEMS + 0x30, FUNCTION]);
}

#[test]
fn every_additional_callable_refuses_before_requested_function_payload() {
    for index in 2..=20 {
        let mut f = bound(1);
        f.word(ITEMS + 0x30 + index * 0x30, FOREIGN);
        assert_eq!(read(&f).unwrap_err().code, "invalid_construction");
        not_read(&f, &[FUNCTION, FOREIGN]);
    }
}

#[test]
fn exact_item_glue_callable_member_adjustment_and_receiver_are_required() {
    for (field, value) in [
        (ITEMS + 0x30, BASE + 0x3489788), // Other known Mac Legacy glue.
        (FUNCTION, BASE + 0x3489818),     // Other known Mac callable.
        (FUNCTION + 8, BASE + 0x22f27d0), // Topbar opener, not this choice.
        (FUNCTION + 0x10, 8),
        (FUNCTION + 0x18, FOREIGN),
    ] {
        let mut f = bound(1);
        f.word(field, value);
        assert_eq!(read(&f).unwrap_err().code, "invalid_construction");
        not_read(&f, &[FOREIGN]);
    }
}

#[test]
fn disabled_request_and_transient_visual_state_remain_distinct_non_authorizing_facts() {
    for state in 0..=3 {
        for flag in [0_u8, 0x40, 0xff] {
            let mut f = bound(1);
            f.int(BUTTON + 0x9c, state);
            f.put(BUTTON + 0xcc, &[flag]);
            let v = read(&f).unwrap();
            assert_eq!(v["choices"]["target"]["disabled"], state == 3);
            let c = &v["choices"]["target"]["callback"];
            assert_eq!(c["button_flags_cc_raw"], flag);
            assert_eq!(c["disable_requested"], flag & 0x40 != 0);
            assert_eq!(c["prerequisites_evaluated"], false);
            assert_eq!(v["placement_eligibility_verified"], false);
            assert_eq!(v["input_authorized"], false);
        }
    }
}

#[test]
fn changed_membership_capture_or_disable_request_invalidates_same_snapshot() {
    let mut fields = vec![
        BUTTON + 0x70,
        BUTTON + 0x78,
        BUTTON + 0x80,
        BUTTON + 0x88,
        BUTTON + 0x8c,
        BUTTON + 0x90,
        BUTTON + 0xc0,
        BUTTON + 0xcc,
        NODE,
        NODE + 8,
        NODE + 0x10,
        NODE + 0x18,
        ITEMS + 0x30,
        ITEMS + 0x60,
        FUNCTION,
        FUNCTION + 8,
        FUNCTION + 0x10,
        FUNCTION + 0x18,
    ];
    fields.extend((2..=20).map(|index| ITEMS + 0x30 + index * 0x30));
    for field in fields {
        let f = bound(1);
        f.final_fault.set(Some(field));
        assert_eq!(read(&f).unwrap_err().code, "torn_read", "{field:x}");
    }
}

#[test]
fn hidden_or_incomplete_choices_skip_callback_even_when_requested() {
    let f = fixture(false, 0);
    let v = read(&f).unwrap();
    assert_eq!(v["callback_binding_verified"], false);
    not_read(&f, &[ITEMS, BUTTON, NODE, FUNCTION]);
    let mut f = fixture(true, 1);
    f.array(0x480000 + 0xd30, 0, &[]);
    let v = read(&f).unwrap();
    assert_eq!(v["choices"]["complete_membership"], false);
    assert_eq!(v["callback_binding_verified"], false);
    not_read(&f, &[ITEMS + 0x10, BUTTON, NODE, FUNCTION]);
}

#[test]
fn callback_option_is_accepted_only_for_ui_probe_and_never_claims_a_bound_session() {
    let server = Server {
        process: None,
        unavailable: None,
    };
    for callback in [false, true] {
        for name in ["hoi4_construction_ui_probe", "hoi4_construction_target"] {
            let reply=server.handle(json!({"jsonrpc":"2.0","id":1,"method":"tools/call",
                "params":{"name":name,"arguments":{"building_id":"arms_factory","state_id":1,"include_callback":callback}}})).unwrap();
            let content: Value =
                serde_json::from_str(reply["result"]["content"][0]["text"].as_str().unwrap())
                    .unwrap();
            assert_eq!(reply["result"]["isError"], true);
            assert_eq!(
                content["error"]["code"],
                if name == "hoi4_construction_ui_probe" {
                    "capability_unavailable"
                } else {
                    "invalid_arguments"
                }
            );
        }
    }
}
