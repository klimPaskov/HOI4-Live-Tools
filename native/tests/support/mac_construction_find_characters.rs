//! Full retained Find witness/character integration; no OS or key dispatch.
use super::*;
const HOTKEYS: u64 = 0x9f0000;
const APP: u64 = 0x9d0000;

fn empty(f: &mut Fixture) {
    for (at, value) in [
        (BASE + 0x35d8dd0, HOTKEYS),
        (HOTKEYS, BASE + 0x34a7db8),
        (BASE + 0x34a7db8 + 0x20, BASE + 0x26ebf10),
        (BASE + 0x327b430, BASE + 0x34fd970),
        (HOTKEYS + 8, BASE + 0x34fd970),
    ] {
        f.word(at, value);
    }
    f.put(BASE + 0x35d8dd8, &[1]);
    f.int(HOTKEYS + 0x64, 0);
    f.int(HOTKEYS + 0x10, 0);
    f.int(HOTKEYS + 0x14, 0);
    f.put(HOTKEYS + 0x18, &[0]);
    f.put(BASE + 0x34fd974, &[0]);
    f.put(BASE + 0x34fd99c, &[0xff]);
}

#[test]
fn exact_owned_empty_wrapper_supports_each_digit_and_return_under_one_find_witness() {
    let mut f = fixture();
    empty(&mut f);
    let anchor = bind(&f, 64).unwrap();
    assert_eq!(
        anchor.private_baseline().unwrap()["character_forwarding"],
        "owned_hotkeys_flag1_exact_empty_table"
    );
    for prefix in ["", "6", "64"] {
        f.string(FIND + 0xc48, prefix);
        let snapshots = f.snapshots.get();
        assert!(anchor.observe_prefix(&f, prefix).unwrap().is_some());
        assert_eq!(f.snapshots.get(), snapshots + 1);
        assert!(!f.untouched(BASE + 0x34fd974, 1));
        assert!(!f.untouched(EDIT + 0x239, 1));
        assert!(!f.untouched(FIND + 0xc48, 24));
    }
    full_query(&mut f);
    assert!(anchor.observe_return(&f).unwrap().is_some());
}

#[test]
fn current_capture_change_refuses_before_even_a_pending_query_or_return() {
    let mut f = fixture();
    empty(&mut f);
    let anchor = bind(&f, 64).unwrap();
    f.int(HOTKEYS + 0x10, 1);
    assert_eq!(
        anchor.observe_prefix(&f, "6").unwrap_err().code,
        "construction_state_changed"
    );
    assert!(f.untouched(FIND + 0xc48, 24));
    assert!(f.untouched(EDIT, 8));
    full_query(&mut f);
    assert_eq!(
        anchor.observe_return(&f).unwrap_err().code,
        "construction_state_changed"
    );
    assert!(f.untouched(FIND + 0xc48, 24));
}

#[test]
fn original_application_and_override_changes_refuse_before_find_or_foreign_payload() {
    for at in [BASE + 0x35d8dd0, BASE + 0x3639ad8, IDLER + 0x4f8] {
        let mut f = fixture();
        empty(&mut f);
        let anchor = bind(&f, 64).unwrap();
        f.word(at, FOREIGN);
        assert_eq!(
            anchor.observe_prefix(&f, "").unwrap_err().code,
            "construction_state_changed"
        );
        assert!(f.untouched(FOREIGN, 0x400));
        assert!(f.untouched(FIND + 0xc48, 24));
        // Character edges are rejected before even the campaign context read.
        assert!(f.untouched(GS + crate::game::MACOS.player_tag, 4));
    }
}

#[test]
fn changed_sentinel_alias_or_header_is_not_a_new_bind() {
    for (at, value) in [
        (HOTKEYS + 8, FOREIGN),
        (BASE + 0x327b430, FOREIGN),
        (HOTKEYS + 0x10, 1),
    ] {
        let mut f = fixture();
        empty(&mut f);
        let anchor = bind(&f, 64).unwrap();
        if at == HOTKEYS + 0x10 {
            f.int(at, value as i32);
        } else {
            f.word(at, value);
        }
        assert!(anchor.observe_prefix(&f, "").is_err());
        assert!(f.untouched(FOREIGN, 80));
        assert!(f.untouched(BASE + 0x34fd970, 80));
        assert!(f.untouched(FIND + 0xc48, 24));
    }
}

#[test]
fn character_final_witness_tear_discards_otherwise_coherent_find() {
    for (at, size) in [(BASE + 0x34fd99c, 1), (HOTKEYS + 0x10, 4), (APP + 0x38, 8)] {
        let mut f = fixture();
        empty(&mut f);
        // Bind reads each field once, then final witness reads it a second time.
        f.tear(at, size, 2);
        assert_eq!(bind(&f, 64).unwrap_err().code, "torn_read");
    }
}

#[test]
fn default_focus_and_mouse_typing_guard_do_not_request_character_admission() {
    let mut f = fixture();
    // Unsupported outer receiver is irrelevant to a passive focus read and
    // the mouse opener's existing post-click typing-control readback.
    f.word(BASE + 0x35d8dd0, FOREIGN);
    let observed = retained::observe_with_focus(&f, Some(64)).unwrap();
    assert!(observed.characters.is_none());
    assert!(f.untouched(BASE + 0x35d8dd0, 8));
    let control = observed.focus().unwrap().control();
    let find = observed.anchor().unwrap();
    find.observe_guarded(
        &f,
        retained::ViewGuard::Typing {
            window: WINDOW,
            control: &control,
        },
    )
    .unwrap();
    assert!(f.untouched(BASE + 0x35d8dd0, 8));
    assert!(f.untouched(FOREIGN, 8));
}

#[test]
fn final_closed_read_skips_character_and_old_control_payload_after_return() {
    let mut f = fixture();
    empty(&mut f);
    let anchor = bind(&f, 64).unwrap();
    closed(&mut f);
    f.word(FIND + 0xc40, FOREIGN);
    f.word(REGISTRY + 0x138, 0);
    f.word(REGISTRY + 0x210, 0);
    f.word(BASE + 0x35d8dd0, FOREIGN);
    assert!(anchor.observe_finished(&f).unwrap().is_some());
    assert!(f.untouched(BASE + 0x35d8dd0, 8));
    assert!(f.untouched(FIND + 0xc40, 8));
    assert!(f.untouched(FOREIGN, 8));
}

#[test]
fn render_source_membership_is_neither_frozen_nor_followed_by_character_proof() {
    let mut f = fixture();
    empty(&mut f);
    let anchor = bind(&f, 64).unwrap();
    f.word(0x9c0000 + 0x68, FOREIGN);
    f.int(0x9c0000 + 0x70, 20_000);
    f.int(0x9c0000 + 0x74, 1897);
    assert!(anchor.observe_prefix(&f, "").unwrap().is_some());
    assert!(f.untouched(0x9c0000 + 0x68, 16));
    assert!(f.untouched(FOREIGN, 8));
}
