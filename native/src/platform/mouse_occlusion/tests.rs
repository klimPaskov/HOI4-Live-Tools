use super::*;

const POINT: [f64; 2] = [960.0, 500.0];
const FRAME: [f64; 4] = [0.0, 53.0, 1920.0, 1108.0];
const CURSOR_LEVEL: i32 = 2_147_483_630;
fn game() -> Window {
    Window {
        id: 3732,
        owner: 41945,
        layer: 0,
        bounds: FRAME,
        alpha: 1.0,
        onscreen: true,
        owner_name: Some("hoi4".into()),
        name: Some("Hearts of Iron IV".into()),
    }
}
fn overlay() -> Window {
    Window {
        id: 3670,
        owner: 31316,
        layer: 3,
        bounds: [660.0, 372.0, 600.0, 500.0],
        owner_name: Some("Docker Desktop".into()),
        name: None,
        ..game()
    }
}
fn check(windows: &[Window]) -> Result<()> {
    require_target(POINT, 3732, 41945, FRAME, windows, None)
}
fn owner() -> CursorOwner {
    cursor_owner(168, "168 88 Sat Oct 24 01:02:03 2026 /System/Library/PrivateFrameworks/SkyLight.framework/Resources/WindowServer\n").unwrap()
}
fn cursor(bounds: [f64; 4]) -> Window {
    Window {
        id: 3,
        owner: 168,
        layer: CURSOR_LEVEL,
        bounds,
        owner_name: Some("Window Server".into()),
        name: Some("Cursor".into()),
        ..game()
    }
}
fn with_cursor(windows: &[Window], point: [f64; 2]) -> Result<()> {
    require_target(
        POINT,
        3732,
        41945,
        FRAME,
        windows,
        Some(Cursor {
            level: CURSOR_LEVEL,
            point,
            owner: &owner(),
        }),
    )
}

#[test]
fn q16_high_level_overlay_blocks_newgame_but_misses_singleplayer() {
    assert_eq!(
        check(&[overlay(), game()]).unwrap_err().code,
        "input_point_occluded"
    );
    require_target(
        [960.0, 369.0],
        3732,
        41945,
        FRAME,
        &[overlay(), game()],
        None,
    )
    .unwrap();
}
#[test]
fn foreign_window_behind_selected_does_not_block() {
    check(&[
        game(),
        Window {
            layer: 0,
            ..overlay()
        },
    ])
    .unwrap();
}
#[test]
fn another_owned_window_covering_point_is_not_the_selected_window() {
    assert_eq!(
        check(&[
            Window {
                owner: 41945,
                ..overlay()
            },
            game()
        ])
        .unwrap_err()
        .code,
        "input_point_occluded"
    );
}
#[test]
fn nonintersecting_front_windows_do_not_block() {
    check(&[
        Window {
            bounds: [0.0, 0.0, 100.0, 100.0],
            ..overlay()
        },
        game(),
    ])
    .unwrap();
}
#[test]
fn transparent_window_does_not_prove_click_through() {
    assert_eq!(
        check(&[
            Window {
                alpha: 0.0,
                ..overlay()
            },
            game()
        ])
        .unwrap_err()
        .code,
        "input_point_occluded"
    );
}
#[test]
fn owned_target_identity_frame_and_opacity_are_exact() {
    for changed in [
        Window { owner: 1, ..game() },
        Window {
            bounds: [1.0, 53.0, 1920.0, 1108.0],
            ..game()
        },
        Window {
            alpha: 0.5,
            ..game()
        },
        Window { layer: 3, ..game() },
    ] {
        assert_eq!(check(&[changed]).unwrap_err().code, "input_window_metadata");
    }
    assert_eq!(
        check(&[overlay()]).unwrap_err().code,
        "input_window_metadata"
    );
}
#[test]
fn point_bounds_are_half_open_and_nonfinite_points_refuse() {
    for point in [[0.0, 53.0], [1919.999, 1160.999]] {
        require_target(point, 3732, 41945, FRAME, &[game()], None).unwrap();
    }
    for point in [
        [-0.001, 500.0],
        [1920.0, 500.0],
        [100.0, 1161.0],
        [100.0, 52.999],
        [f64::NAN, 500.0],
        [100.0, f64::INFINITY],
    ] {
        assert_eq!(
            require_target(point, 3732, 41945, FRAME, &[game()], None)
                .unwrap_err()
                .code,
            "invalid_arguments"
        );
    }
}
#[test]
fn genuine_custom_and_normal_hotspot_cursors_are_narrow_exceptions() {
    for bounds in [[960.0, 500.0, 32.0, 32.0], [948.0, 489.0, 23.0, 22.0]] {
        with_cursor(&[cursor(bounds), game()], POINT).unwrap();
    }
}
#[test]
fn ignored_cursor_does_not_hide_foreign_overlay_beneath() {
    assert_eq!(
        with_cursor(
            &[cursor([960.0, 500.0, 32.0, 32.0]), overlay(), game()],
            POINT
        )
        .unwrap_err()
        .code,
        "input_point_occluded"
    );
}
#[test]
fn cursor_label_level_owner_and_opacity_must_all_match() {
    let original = cursor([948.0, 489.0, 23.0, 22.0]);
    for changed in [
        Window {
            owner: 31316,
            ..original.clone()
        },
        Window {
            owner_name: None,
            ..original.clone()
        },
        Window {
            name: None,
            ..original.clone()
        },
        Window {
            name: Some("fake Cursor".into()),
            ..original.clone()
        },
        Window {
            layer: 3,
            ..original.clone()
        },
        Window {
            alpha: 0.0,
            ..original
        },
    ] {
        assert_eq!(
            with_cursor(&[changed, game()], POINT).unwrap_err().code,
            "input_point_occluded"
        );
    }
}
#[test]
fn huge_or_stale_cursor_rectangles_refuse() {
    assert_eq!(
        with_cursor(&[cursor([900.0, 450.0, 128.0, 128.0]), game()], POINT)
            .unwrap_err()
            .code,
        "input_point_occluded"
    );
    assert_eq!(
        with_cursor(&[cursor([948.0, 489.0, 23.0, 22.0]), game()], [10.0, 10.0])
            .unwrap_err()
            .code,
        "input_point_occluded"
    );
    assert_eq!(
        with_cursor(
            &[cursor([948.0, 489.0, 23.0, 22.0]), game()],
            [f64::NAN, 500.0]
        )
        .unwrap_err()
        .code,
        "input_point_occluded"
    );
}
#[test]
fn cursor_requires_fresh_owner_proof_and_is_unique() {
    let c = cursor([948.0, 489.0, 23.0, 22.0]);
    assert_eq!(
        check(&[c.clone(), game()]).unwrap_err().code,
        "input_point_occluded"
    );
    assert_eq!(
        with_cursor(&[c.clone(), Window { id: 4, ..c }, game()], POINT)
            .unwrap_err()
            .code,
        "input_point_occluded"
    );
}
#[test]
fn duplicate_window_ids_and_missing_collections_refuse() {
    assert_eq!(
        check(&[game(), game()]).unwrap_err().code,
        "input_window_metadata"
    );
    assert_eq!(check(&[]).unwrap_err().code, "input_window_metadata");
    assert_eq!(
        check(&vec![game(); 4097]).unwrap_err().code,
        "input_window_metadata"
    );
}
#[test]
fn malformed_metadata_refuses_even_when_rectangle_is_not_hit_or_behind() {
    let mutations = [
        Window { id: 0, ..overlay() },
        Window {
            owner: 0,
            ..overlay()
        },
        Window {
            bounds: [f64::NAN, 0.0, 1.0, 1.0],
            ..overlay()
        },
        Window {
            bounds: [0.0, 0.0, -1.0, 1.0],
            ..overlay()
        },
        Window {
            bounds: [0.0, 0.0, 1.0, 0.0],
            ..overlay()
        },
        Window {
            bounds: [1e20, 0.0, 1.0, 1.0],
            ..overlay()
        },
        Window {
            alpha: f64::NAN,
            ..overlay()
        },
        Window {
            alpha: 1.1,
            ..overlay()
        },
        Window {
            onscreen: false,
            ..overlay()
        },
    ];
    for w in mutations {
        assert_eq!(
            check(&[game(), w]).unwrap_err().code,
            "input_window_metadata"
        );
    }
}
#[test]
fn cursor_identity_uses_uid_path_and_start_not_pid_or_display_name_alone() {
    let expected = owner();
    let alternate = cursor_owner(168, "168 88 Sat Oct 24 01:02:03 2026 /System/Library/PrivateFrameworks/SkyLight.framework/Versions/A/Resources/WindowServer").unwrap();
    assert_ne!(expected, alternate); // Exact identity is retained within each observation.
    let changed = cursor_owner(168, "168 88 Sat Oct 24 01:02:04 2026 /System/Library/PrivateFrameworks/SkyLight.framework/Resources/WindowServer").unwrap();
    assert_ne!(expected, changed);
    for text in ["", "168 501 Sat Oct 24 01:02:03 2026 /System/Library/PrivateFrameworks/SkyLight.framework/Resources/WindowServer", "168 0 Sat Oct 24 01:02:03 2026 /System/Library/PrivateFrameworks/SkyLight.framework/Resources/WindowServer", "169 88 Sat Oct 24 01:02:03 2026 /System/Library/PrivateFrameworks/SkyLight.framework/Resources/WindowServer", "168 88 Sat Oct 24 01:02:03 2026 /tmp/WindowServer", "168 88 Sat Oct 24 25:02:03 2026 /System/Library/PrivateFrameworks/SkyLight.framework/Resources/WindowServer", "168 88 Sat Oct 24 01:02:03 2026 /System/Library/PrivateFrameworks/SkyLight.framework/Resources/WindowServer extra", "168 88 Sat Oct 24 01:02:03 2026 /System/Library/PrivateFrameworks/SkyLight.framework/Resources/WindowServer\n168 88 Sat Oct 24 01:02:03 2026 /tmp/x"] {
        assert_eq!(cursor_owner(168, text).unwrap_err().code, "input_window_metadata");
    }
}
