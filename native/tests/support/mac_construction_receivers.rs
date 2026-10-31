use super::*;

// Each fixture supplies only the independently mapped class-specific links.
// Forbidden reads below prevent a larger Standard layout from leaking in.
fn family(primary: u64, graphics: u64, renderer: u64) -> MacFixture {
    let mut f = keyboard();
    f.word(PREVIOUS, BASE + primary);
    f.word(PREVIOUS + graphics, GRAPHICS);
    f.word(PREVIOUS + renderer, PREVIOUS_SPRITE);
    f
}

#[test]
fn direct_base_receivers_use_their_own_layout_and_current_no_match_gate() {
    for (primary, graphics, renderer) in [(0x36b6880, 0xe8, 0xd8), (0x36cf3a0, 0xd8, 0xf0)] {
        let f = family(primary, graphics, renderer);
        assert_eq!(reason(&f), "ordinary_t_route_observed");
        not_read(&f, &[PREVIOUS + 0xe0, PREVIOUS + 0x180, PREVIOUS + 0x1d9]);
        for (field, value) in [(PREVIOUS_KEYS, 116), (PREVIOUS + 0x64, 0x806)] {
            let mut f = family(primary, graphics, renderer);
            f.int(field, value);
            assert_eq!(
                reason(&f),
                "preceding_keyboard_target_can_handle_or_consume_t"
            );
            not_read(&f, &[KEYS]);
        }
        for state in [1, 3, 4, -1] {
            let mut f = family(primary, graphics, renderer);
            f.int(PREVIOUS + 0x9c, state);
            assert_eq!(reason(&f), "unsupported_keyboard_button_gate");
            not_read(&f, &[PREVIOUS_KEYS, PREVIOUS + 0xd0]);
        }
        let mut f = family(primary, graphics, renderer);
        f.word(PREVIOUS + 0xc8, FOREIGN);
        assert_eq!(reason(&f), "unsupported_keyboard_button_gate");
        not_read(&f, &[FOREIGN, PREVIOUS_KEYS]);
        let mut f = family(primary, graphics, renderer);
        f.put(PREVIOUS + 0xd4, &[0]);
        f.int(PREVIOUS + 0x64, 0x806);
        f.int(PREVIOUS_KEYS, 116);
        assert_eq!(reason(&f), "ordinary_t_route_observed");
        not_read(&f, &[PREVIOUS_KEYS]);
    }
}

#[test]
fn drag_requires_its_narrow_key_path_without_following_base_shortcuts() {
    let f = family(0x36b8b50, 0xd8, 0x108);
    assert_eq!(reason(&f), "ordinary_t_route_observed");
    not_read(
        &f,
        &[
            PREVIOUS + 0xa8,
            PREVIOUS + 0xc8,
            PREVIOUS + 0xd4,
            PREVIOUS + 0xe0,
            PREVIOUS + 0x180,
            PREVIOUS + 0x1d9,
            PREVIOUS_KEYS,
        ],
    );
    for state in [-1, 1, 2, 3, 4] {
        let mut f = family(0x36b8b50, 0xd8, 0x108);
        f.int(PREVIOUS + 0x9c, state);
        assert_eq!(reason(&f), "unsupported_keyboard_drag_gate");
        not_read(&f, &[PREVIOUS_KEYS, KEYS]);
    }
    for bits in [0x2000, 0x4000, 0x6000] {
        let mut f = family(0x36b8b50, 0xd8, 0x108);
        f.int(PREVIOUS + 0x64, 0x804 | bits);
        assert_eq!(reason(&f), "unsupported_keyboard_drag_gate");
    }
}

#[test]
fn instant_text_box_has_no_button_or_optional_pointer_payload_on_key_path() {
    let f = family(0x36c00e0, 0x88, 0xb0);
    assert_eq!(reason(&f), "ordinary_t_route_observed");
    not_read(
        &f,
        &[
            PREVIOUS + 0x9c,
            PREVIOUS + 0xa8,
            PREVIOUS + 0xc8,
            PREVIOUS + 0xd4,
            PREVIOUS + 0xd8,
            PREVIOUS + 0xe0,
            PREVIOUS + 0x128,
            PREVIOUS + 0x180,
            PREVIOUS + 0x194,
            PREVIOUS + 0x1d9,
            PREVIOUS_KEYS,
        ],
    );
}

#[test]
fn every_family_checks_gui_graphics_and_renderer_before_handler_payload() {
    for (primary, graphics, renderer) in [
        (0x36b6880, 0xe8, 0xd8),
        (0x36cf3a0, 0xd8, 0xf0),
        (0x36b8b50, 0xd8, 0x108),
        (0x36c00e0, 0x88, 0xb0),
    ] {
        for field in [0x58, graphics, renderer] {
            let mut f = family(primary, graphics, renderer);
            f.word(PREVIOUS + field, FOREIGN);
            assert_eq!(read(&f).unwrap_err().code, "invalid_construction");
            not_read(&f, &[FOREIGN, PREVIOUS + 0x9c, PREVIOUS_KEYS, KEYS]);
        }
        for field in [0, 0x58, graphics, renderer, 0x64] {
            let f = family(primary, graphics, renderer);
            f.final_fault.set(Some(PREVIOUS + field));
            assert_eq!(read(&f).unwrap_err().code, "torn_read");
        }
    }
}

#[test]
fn known_renderer_primaries_share_only_the_proven_backlink() {
    for primary in [0x36bb6a0, 0x36c9210, 0x36c04b8, 0x35edb70, 0x36cee08] {
        let mut f = keyboard();
        f.word(PREVIOUS_SPRITE, BASE + primary);
        assert_eq!(reason(&f), "ordinary_t_route_observed");
        not_read(&f, &[PREVIOUS_SPRITE + 0x58, PREVIOUS_SPRITE + 0x120]);
        let mut f = keyboard();
        f.word(PREVIOUS_SPRITE, BASE + primary);
        f.word(PREVIOUS_SPRITE + 0x138, 0);
        assert_eq!(reason(&f), "ordinary_t_route_observed");
        not_read(&f, &[PREVIOUS, PREVIOUS_KEYS]);
    }
    let mut f = keyboard();
    f.word(PREVIOUS_SPRITE, BASE + 0x36cee08 + 0x160);
    assert_eq!(read(&f).unwrap_err().code, "invalid_construction");
    not_read(&f, &[PREVIOUS_SPRITE + 0x138, PREVIOUS]);
}

#[test]
fn nonnull_standard_parent_requires_exact_attachment_and_byte_gate() {
    let mut f = keyboard();
    parent(&mut f, PREVIOUS, PREVIOUS_SPRITE, PREVIOUS_POSITION);
    // Nonzero byte is the actual predicate; adjacent bytes are not read.
    f.put(PREVIOUS_POSITION + 0x128, &[2]);
    assert_eq!(reason(&f), "ordinary_t_route_observed");
    not_read(&f, &[PREVIOUS_POSITION + 0x58, PREVIOUS_POSITION + 0x129]);
    for (field, value) in [
        (PREVIOUS_POSITION, BASE + 0x36cc528),
        (PREVIOUS_SPRITE + 0x108, POSITION),
    ] {
        let mut f = keyboard();
        parent(&mut f, PREVIOUS, PREVIOUS_SPRITE, PREVIOUS_POSITION);
        f.word(field, value);
        assert_eq!(read(&f).unwrap_err().code, "invalid_construction");
        not_read(&f, &[PREVIOUS_POSITION + 0x128, PREVIOUS_KEYS, KEYS]);
    }
    for field in [PREVIOUS_POSITION + 0x128, PREVIOUS + 0x1d9] {
        let mut f = keyboard();
        parent(&mut f, PREVIOUS, PREVIOUS_SPRITE, PREVIOUS_POSITION);
        f.put(field, &[if field == PREVIOUS + 0x1d9 { 1 } else { 0 }]);
        assert_eq!(reason(&f), "unsupported_keyboard_button_gate");
        not_read(&f, &[PREVIOUS + 0x9c, PREVIOUS_KEYS]);
    }
}

#[test]
fn named_parent_is_bound_to_its_own_direct_container_before_gate_payload() {
    for field in [BUTTON + 0x180, AGENCY + 0x1358] {
        let mut f = keyboard();
        parent(&mut f, PREVIOUS, PREVIOUS_SPRITE, PREVIOUS_POSITION);
        f.word(field, PREVIOUS_POSITION);
        assert_eq!(read(&f).unwrap_err().code, "invalid_construction");
        not_read(&f, &[KEYS]);
    }
    let mut f = keyboard();
    f.word(POSITION, BASE + 0x36cc528);
    assert_eq!(read(&f).unwrap_err().code, "invalid_construction");
    not_read(&f, &[POSITION + 0x128, SOURCE, KEYS]);
}

#[test]
fn disabled_standard_predecessor_bypasses_shortcuts_only_when_not_consuming() {
    let mut f = keyboard();
    f.int(PREVIOUS + 0x9c, 3);
    f.int(PREVIOUS + 0xd0, 0);
    f.int(PREVIOUS_KEYS, 116);
    assert_eq!(reason(&f), "ordinary_t_route_observed");
    not_read(&f, &[PREVIOUS + 0xa8, PREVIOUS + 0xd4, PREVIOUS_KEYS]);
    for value in [1, 2, 0x100, 0x1000000] {
        let mut f = keyboard();
        f.int(PREVIOUS + 0x9c, 3);
        f.int(PREVIOUS + 0xd0, value);
        assert_eq!(
            reason(&f),
            "preceding_keyboard_target_can_handle_or_consume_t"
        );
        not_read(&f, &[PREVIOUS_KEYS, KEYS]);
    }
    let mut f = keyboard();
    f.int(PREVIOUS + 0x9c, 3);
    f.int(PREVIOUS + 0xd0, 0);
    f.int(PREVIOUS + 0x64, 0x806);
    assert_eq!(
        reason(&f),
        "preceding_keyboard_target_can_handle_or_consume_t"
    );
    let mut f = keyboard();
    f.int(PREVIOUS + 0x9c, 3);
    f.word(PREVIOUS + 0xc8, FOREIGN);
    assert_eq!(reason(&f), "unsupported_keyboard_button_gate");
    not_read(&f, &[FOREIGN, PREVIOUS + 0xd0, PREVIOUS_KEYS]);
    let mut f = keyboard();
    f.int(BUTTON + 0x9c, 3);
    let value = read(&f).unwrap();
    assert!(value["opener"]["shortcut"].is_null());
    not_read(&f, &[SOURCE, BUTTON + 0xd0, KEYS]);
}

#[test]
fn added_parent_and_handler_fields_remain_in_final_witness() {
    for field in [
        AGENCY + 0x1358,
        POSITION,
        POSITION + 0x128,
        BUTTON + 0x180,
        SPRITE + 0x108,
        PREVIOUS_POSITION,
        PREVIOUS_POSITION + 0x128,
        PREVIOUS + 0x180,
        PREVIOUS_SPRITE + 0x108,
        PREVIOUS + 0x1d9,
        PREVIOUS + 0xd0 + 3,
    ] {
        let mut f = keyboard();
        parent(&mut f, PREVIOUS, PREVIOUS_SPRITE, PREVIOUS_POSITION);
        f.int(PREVIOUS + 0x9c, 3);
        f.int(PREVIOUS + 0xd0, 0);
        f.final_fault.set(Some(field));
        assert_eq!(read(&f).unwrap_err().code, "torn_read", "{field:x}");
    }
}
