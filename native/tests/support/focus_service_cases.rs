use crate::{
    actions::Epoch,
    focus_stage::{ActiveFocus, LocalRoot},
};
const POLITICS: u64 = 0x270000;
const POLITICS_WINDOW: u64 = 0x271000;
fn owned_scene(mode: i32) -> Fake {
    let mut f = Fake::new();
    f.own_focus_ui_fixture();
    f.int(GS + 0x620, 2);
    f.word(0x42c9a88 + 0xc0, 0x20811a0);
    f.word(MAN + 0x128, POLITICS);
    f.region(POLITICS, 0x600);
    f.word(POLITICS, 0x42546e8);
    f.word(POLITICS + 0x2f0, ID);
    f.word(POLITICS + 0x38, MAN);
    f.word(POLITICS + 0x2f8, POLITICS_WINDOW);
    f.region(POLITICS_WINDOW, 0xf00);
    f.word(POLITICS_WINDOW, 0x4347d58);
    f.word(POLITICS_WINDOW + 0x98, GUI);
    f.int(MAN + 0x3d8, mode);
    f.put(POLITICS_WINDOW + 0xa5, &[if mode == 12 { 8 } else { 0 }]);
    f.put(0x2600a5, &[if mode == 13 { 8 } else { 0 }]);
    f.put(WIN + 0xa5, &[if mode == 13 { 8 } else { 0 }]);
    f
}
fn epoch() -> Epoch {
    Epoch {
        game: hoi4_live_native::platform::Identity {
            pid: 123,
            start_ticks: "5".into(),
            exe: "/generated-focus".into(),
            uid: 1000,
        },
        campaign: "generated-own-focus".into(),
        player: "GER".into(),
    }
}
#[test]
fn real_own_producers_supply_settled_navigation_roots_and_requested_selection() {
    for mode in [23, 12, 13] {
        let f = owned_scene(mode);
        let o = state::observe(&f, "GER_rhineland", &epoch(), false).unwrap();
        assert_eq!(o.state.active_focus, ActiveFocus::Idle);
        assert_eq!(o.state.managed_mode, Some(mode));
        assert_eq!(o.state.canonical_focus_id.as_deref(), Some("GER_rhineland"));
        assert_eq!(o.state.own_research_info_counts, Some([0, 0]));
        assert_eq!(
            o.state.politics_root,
            if mode == 12 {
                LocalRoot::Ready
            } else {
                LocalRoot::Hidden
            }
        );
        assert_eq!(
            o.state.focus_root,
            if mode == 13 {
                LocalRoot::Ready
            } else {
                LocalRoot::Hidden
            }
        );
        assert_eq!(
            o.state.selected_focus.as_deref(),
            if mode == 13 {
                Some("GER_rhineland")
            } else {
                None
            }
        );
        assert_eq!(o.context.campaign.difficulty, 2);
        assert_eq!(f.checks.get(), 4); // Two independent complete brackets, not an atomic frame.
    }
}
#[test]
fn active_focus_requires_real_own_progress_catalog_membership() {
    let mut f = owned_scene(13);
    f.word(0x254010, F);
    assert_eq!(
        state::observe(&f, "GER_rhineland", &epoch(), false)
            .unwrap()
            .state
            .active_focus,
        ActiveFocus::Active("GER_rhineland".into())
    );
    let mut f = owned_scene(13);
    f.word(0x254010, 0xdead0000);
    assert_eq!(
        state::observe(&f, "GER_rhineland", &epoch(), false)
            .err()
            .unwrap()
            .code,
        "invalid_focus_owner"
    );
    assert!(f.untouched(0xdead0000, 8));
}
#[test]
fn transition_one_and_unmaterialized_roots_remain_unknown() {
    let mut f = owned_scene(13);
    f.int(0x260000 + 0xeb0, 1);
    let o = state::observe(&f, "GER_rhineland", &epoch(), false).unwrap();
    assert_eq!(o.state.focus_root, LocalRoot::Unknown);
    let mut f = owned_scene(23);
    f.word(POLITICS + 0x2f8, 0);
    assert_eq!(
        state::observe(&f, "GER_rhineland", &epoch(), false)
            .unwrap()
            .state
            .politics_root,
        LocalRoot::Unknown
    );
    let mut f = owned_scene(13);
    f.int(WIN + 0xef8, 1);
    let o = state::observe(&f, "GER_rhineland", &epoch(), false).unwrap();
    assert_eq!(o.state.own_detail_counts, None);
    assert_eq!(o.state.requested_detail_ready, None);
    assert_eq!(o.state.selected_focus, None);
}
#[test]
fn separate_readers_cannot_mix_modes_or_country_anchors() {
    let mut f = owned_scene(23);
    let (body, c) = own_focus::command_owned(&f, "GER_rhineland", false).unwrap();
    f.int(MAN + 0x3d8, 12);
    f.put(POLITICS_WINDOW + 0xa5, &[8]);
    let (p, pc) = hoi4_live_native::politics_focus::owned_snapshot(&f, false).unwrap();
    assert_eq!(
        state::model(&body, &c, &p, &pc, "GER_rhineland", &epoch())
            .unwrap_err()
            .code,
        "torn_read"
    );
    let mut changed = c;
    changed.anchors[0] += 8;
    assert_eq!(state::join(&changed, &pc).unwrap_err().code, "torn_read");
}
#[test]
fn foreign_politics_owner_is_refused_before_local_window_payload() {
    let mut f = owned_scene(13);
    f.word(POLITICS + 0x2f0, 0xdead0000);
    for a in POLITICS_WINDOW..POLITICS_WINDOW + 0xf00 {
        f.data.remove(&a);
    }
    assert_eq!(
        state::observe(&f, "GER_rhineland", &epoch(), false)
            .err()
            .unwrap()
            .code,
        "invalid_gui_owner"
    );
    assert!(f.untouched(POLITICS_WINDOW, 0xf00));
}
