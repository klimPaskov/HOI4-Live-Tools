use super::*;
const RP: u64 = 0x265000;
const PW: u64 = 0x266000;
const TW: u64 = 0x267000;
fn fixture() -> Fake {
    let mut f = Fake::new();
    f.own_focus_ui_fixture();
    f.region(RP, 0x400);
    f.word(RP, 0x424dc38);
    f.word(RP + 0x2f0, ID);
    f.word(RP + 0x2f8, PW);
    f.word(MAN + 0xf0, RP);
    for (w, flags) in [(PW, 8), (TW, 0)] {
        f.region(w, 0xf00);
        f.word(w, 0x4347d58);
        f.word(w + 0x98, GUI);
        f.put(w + 0xa5, &[flags]);
    }
    f.word(0x2612f8, TW);
    f
}
#[test]
fn focus_research_roots_require_positive_settled_own_facts() {
    let u = fixture().ui().ui_facts.unwrap();
    assert_eq!(u.research_panel_local_ready(), Some(true));
    assert_eq!(u.research_tree_local_ready(), Some(false));
    for w in [PW, TW] {
        for state in [1, 2, 3, -1] {
            let mut f = fixture();
            f.int(w + 0xeb0, state);
            let u = f.ui().ui_facts.unwrap();
            assert_eq!(
                if w == PW {
                    u.research_panel_local_ready()
                } else {
                    u.research_tree_local_ready()
                },
                None
            );
        }
    }
    let mut f = fixture();
    f.word(MAN + 0xf0, 0);
    let u = f.ui().ui_facts.unwrap();
    assert_eq!(u.research_panel_local_ready(), None);
    assert!(f.untouched(PW + 0xa5, 1));
    let mut f = fixture();
    f.word(0x2612f8, 0);
    assert_eq!(f.ui().ui_facts.unwrap().research_tree_local_ready(), None);
}
#[test]
fn focus_research_roots_refuse_foreign_types_or_owners_before_window_flags() {
    for (field, window, code) in [
        (RP, PW, "invalid_focus_ui_type"),
        (RP + 0x2f0, PW, "invalid_focus_owner"),
        (PW, PW, "invalid_focus_ui_type"),
        (PW + 0x98, PW, "invalid_focus_owner"),
        (TW, TW, "invalid_focus_ui_type"),
        (TW + 0x98, TW, "invalid_focus_owner"),
    ] {
        let mut f = fixture();
        f.word(field, 0xdead0000);
        assert_eq!(
            own_focus::command_owned(&f, "GER_rhineland", false)
                .unwrap_err()
                .code,
            code
        );
        assert!(f.untouched(window + 0xa5, 1));
    }
    let mut f = fixture();
    f.int(0x261e20, 2);
    let u = f.ui().ui_facts.unwrap();
    assert_eq!(u.research_tree_local_ready(), None);
    assert_eq!(u.own_research_info_local_counts(), None);
    assert!(f.untouched(0x2612f8, 8));
    assert!(f.untouched(TW + 0xa5, 1));
}
#[test]
fn focus_research_root_identity_visibility_and_transitions_are_final_witnesses() {
    for field in [
        MAN + 0xf0,
        RP,
        RP + 0x2f0,
        RP + 0x2f8,
        PW,
        PW + 0x98,
        PW + 0xa5,
        PW + 0xeb0,
        PW + 0xef8,
        0x2612f8,
        TW,
        TW + 0x98,
        TW + 0xa5,
        TW + 0xeb0,
        TW + 0xef8,
    ] {
        let mut f = fixture();
        f.torn = Some(field);
        assert_eq!(
            own_focus::command_owned(&f, "GER_rhineland", false)
                .unwrap_err()
                .code,
            "torn_read",
            "{field:x}"
        );
    }
}
