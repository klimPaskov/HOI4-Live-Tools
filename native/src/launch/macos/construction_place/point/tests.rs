use super::super::tests::fixture::*;
use super::*;

#[test]
fn actual_resident_reader_binds_original_point_source_panel_find_and_scene() {
    let mut f = point_memory();
    let a = Anchor::bind(&f, 1, &context(&f)).unwrap();
    assert_eq!(a.observe(&f, true).unwrap(), [62, 10]);
    f.int(BASE + 0x384b170, 960);
    f.int(BASE + 0x384b174, 540);
    assert_eq!(a.observe(&f, true).unwrap(), [960, 540]);
    assert_eq!(
        a.private_baseline()["world_bits"],
        json!([0x453c001c_u32, 0x411b3333_u32, 0x44c4172d_u32])
    );
    assert_eq!(a.private_baseline()["modal_route_qualified"], false);
    assert!(f.untouched(FIND + 0xd68, 24));
    assert!(f.untouched(FIND + 0xda8, 8));
}
#[test]
fn unsupported_or_multiple_sdl_windows_refuse_before_next_window_payload() {
    let mut f = point_memory();
    f.word(WINDOW + 0xe8, FOREIGN);
    assert!(Anchor::bind(&f, 1, &context(&f)).is_err());
    assert!(f.untouched(FOREIGN, 0x1000));
}
#[test]
fn retained_geometry_and_ui_edges_refuse_before_replacement_payload() {
    for at in [
        BASE + 0x384b480,
        VIDEO + 0x2c8,
        WINDOW + 0xd8,
        DRIVER + 0x10,
        MANAGER + 0x150,
        PANEL + 0x490,
        MANAGER + 0x190,
        FIND + 0xd58,
    ] {
        let mut f = point_memory();
        let a = Anchor::bind(&f, 1, &context(&f)).unwrap();
        f.word(at, FOREIGN);
        assert_eq!(
            a.observe(&f, false).unwrap_err().code,
            "construction_placement_changed"
        );
        assert!(f.untouched(FOREIGN, 0x1000));
    }
}
#[test]
fn hidden_or_animating_panel_and_visible_find_refuse_without_query_payload() {
    for (at, raw) in [
        (ROOT + 0x95, vec![0]),
        (ROOT + 0x13d8, 1i32.to_le_bytes().to_vec()),
        (ROOT + 0x1390, 2i32.to_le_bytes().to_vec()),
        (FIND_ROOT + 0x95, vec![8]),
    ] {
        let mut f = point_memory();
        let a = Anchor::bind(&f, 1, &context(&f)).unwrap();
        f.put(at, &raw);
        assert!(a.observe(&f, false).is_err());
        assert!(f.untouched(FIND + 0xd68, 24));
        assert!(f.untouched(FIND + 0xda8, 8));
    }
}
#[test]
fn visible_find_is_refused_even_at_initial_bind_before_query_or_rows() {
    let mut f = point_memory();
    f.put(FIND_ROOT + 0x95, &[8]);
    assert!(Anchor::bind(&f, 1, &context(&f)).is_err());
    assert!(f.untouched(FIND + 0xd68, 24));
    assert!(f.untouched(FIND + 0xda8, 8));
}
#[test]
fn unmaterialized_hidden_find_does_not_require_any_old_control_payload() {
    let mut f = point_memory();
    f.word(FIND + 0xd58, 0);
    let a = Anchor::bind(&f, 1, &context(&f)).unwrap();
    assert!(a.observe(&f, true).is_ok());
    assert!(f.untouched(FIND_ROOT, 0x2000));
}
#[test]
fn scene_initial_unknown_and_retained_source_changes_refuse_before_foreign_payload() {
    let mut f = point_memory();
    f.array(GS + 0x5e0, &[FOREIGN]);
    assert!(Anchor::bind(&f, 1, &context(&f)).is_err());
    assert!(f.untouched(FOREIGN, 0x1000));
    let mut f = point_memory();
    let a = Anchor::bind(&f, 1, &context(&f)).unwrap();
    f.word(GS + 0x5e0, FOREIGN);
    assert!(a.observe(&f, true).is_err());
    assert!(f.untouched(FOREIGN, 0x1000));
}
#[test]
fn changed_sdl_focus_and_original_map_scope_never_use_new_payload() {
    for at in [BASE + 0x384b168, DATA + 0x130, PROVINCE + 0xc0] {
        let mut f = point_memory();
        let a = Anchor::bind(&f, 1, &context(&f)).unwrap();
        f.word(at, FOREIGN);
        assert!(a.observe(&f, false).is_err());
        assert!(f.untouched(FOREIGN, 0x1000));
    }
}
#[test]
fn fixed_edge_interceptor_checks_full_partial_and_overlapping_reads() {
    let pins = [
        (0x10000, 0x0123456789abcdef_u64),
        (0x10008, 0xfedcba9876543210),
    ];
    let bytes: [u8; 16] = [pins[0].1.to_le_bytes(), pins[1].1.to_le_bytes()]
        .concat()
        .try_into()
        .unwrap();
    inspect(&pins, 0x10000, &bytes).unwrap();
    inspect(&pins, 0x10003, &bytes[3..7]).unwrap();
    let mut wrong = bytes;
    wrong[9] ^= 1;
    assert!(inspect(&pins, 0x10000, &wrong).is_err());
    assert!(inspect(&pins, 0x10009, &wrong[9..10]).is_err());
    inspect(&pins, 0x20000, &[0; 8]).unwrap();
    assert!(inspect(&pins, u64::MAX, &[0; 8]).is_err());
}
#[test]
fn process_or_campaign_replacement_refuses_as_original_scope() {
    let mut f = point_memory();
    let a = Anchor::bind(&f, 1, &context(&f)).unwrap();
    f.id.start_ticks.push('1');
    assert!(a.observe(&f, false).is_err());
    let mut f = point_memory();
    let a = Anchor::bind(&f, 1, &context(&f)).unwrap();
    f.string(
        GS + 0x6c8, // Existing libc++ campaign-fixture layout.
        "bbbbbbbb-1122-4344-8566-778899aabbcc",
    );
    assert!(a.observe(&f, false).is_err());
}
#[test]
fn late_geometry_and_source_witness_tears_do_not_bind_a_new_point() {
    for at in [GUI + 0x98, BASE + 0x384b170] {
        let mut f = point_memory();
        let expected = context(&f);
        f.tear(at, 8, 2);
        assert_eq!(
            Anchor::bind(&f, 1, &expected).err().unwrap().code,
            "torn_read"
        );
    }
}
