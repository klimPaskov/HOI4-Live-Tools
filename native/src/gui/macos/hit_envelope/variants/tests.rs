use super::*;

fn common() -> Common {
    Common {
        canvas: [1920, 1080],
        translation: [0.0, 0.0],
        inherited_scale: 1.0,
        rotation: 0.0,
    }
}
fn fr() -> FrameRenderer {
    FrameRenderer {
        primary_rva: FRAME_RENDERER_PRIMARY_RVA,
        common: common(),
        local_scale: 1.0,
    }
}
fn fs(dimensions: [i32; 2]) -> FrameResource {
    FrameResource {
        primary_rva: FRAME_RESOURCE_PRIMARY_RVA,
        dimensions,
        tiles: [1, 1],
        orientation: 0,
        loaded_index: 7,
    }
}
fn mr() -> MaskedRenderer {
    MaskedRenderer {
        primary_rva: MASKED_RENDERER_PRIMARY_RVA,
        common: common(),
        local_scale: 1.0,
    }
}
fn ms(dimensions: [i32; 2]) -> MaskedResource {
    MaskedResource {
        primary_rva: MASKED_RESOURCE_PRIMARY_RVA,
        dimensions,
        tiles: [1, 1],
        orientation: 0,
    }
}
fn pr() -> ProgressRenderer {
    ProgressRenderer {
        primary_rva: PROGRESS_RENDERER_PRIMARY_RVA,
        common: common(),
        rotation: 0.0,
    }
}
fn ps(dimensions: [i32; 2]) -> ProgressResource {
    ProgressResource {
        primary_rva: PROGRESS_RESOURCE_PRIMARY_RVA,
        dimensions,
    }
}
fn pg(p: [f32; 2], r: ProgressRenderer, s: ProgressResource) -> Result<Decision, GeometryError> {
    progress(p, r, s, ProgressTrig::NormalSincosZero)
}
fn unsupported(u: Unsupported) -> Result<Decision, GeometryError> {
    Ok(Decision::Unsupported(u))
}

#[test]
fn frame_requires_its_actual_resource_primary_before_normalization() {
    let wrong = FrameResource {
        primary_rva: basis::SPRITE_TYPE_PRIMARY_RVA,
        dimensions: [-1, -1],
        ..fs([100, 20])
    };
    assert_eq!(
        frame([970.0, 530.0], fr(), wrong, fs([100, 20])),
        unsupported(Unsupported::ResourcePrimary)
    );
    assert_eq!(
        frame([970.0, 530.0], fr(), fs([100, 20]), wrong),
        unsupported(Unsupported::ResourcePrimary)
    );
    assert_eq!(
        frame([970.0, 530.0], fr(), fs([100, 20]), fs([100, 20])),
        Ok(Decision::Possible)
    );
}
#[test]
fn every_renderer_primary_is_guarded_before_geometry() {
    let c = Common {
        canvas: [0, 0],
        ..common()
    };
    assert_eq!(
        frame(
            [0.0, 0.0],
            FrameRenderer {
                primary_rva: 0,
                common: c,
                ..fr()
            },
            fs([1, 1]),
            fs([1, 1])
        ),
        unsupported(Unsupported::RendererPrimary)
    );
    assert_eq!(
        masked(
            [0.0, 0.0],
            MaskedRenderer {
                primary_rva: FRAME_RENDERER_PRIMARY_RVA,
                common: c,
                ..mr()
            },
            ms([1, 1]),
            ms([1, 1])
        ),
        unsupported(Unsupported::RendererPrimary)
    );
    assert_eq!(
        pg(
            [0.0, 0.0],
            ProgressRenderer {
                primary_rva: 0,
                common: c,
                ..pr()
            },
            ps([1, 1])
        ),
        unsupported(Unsupported::RendererPrimary)
    );
}
#[test]
fn frame_lower_edges_inclusive_upper_edges_exclusive() {
    let s = fs([100, 20]);
    for p in [[960.0, 520.0], [1059.0, 539.0]] {
        assert_eq!(frame(p, fr(), s, s), Ok(Decision::Possible));
    }
    for p in [
        [959.0, 530.0],
        [1060.0, 530.0],
        [970.0, 519.0],
        [970.0, 540.0],
    ] {
        assert_eq!(frame(p, fr(), s, s), Ok(Decision::Rejected));
    }
}
#[test]
fn frame_keeps_size_and_hit_resource_roles_separate() {
    // Size puts the origin at(960,520); hit is10x200, not size100x20.
    assert_eq!(
        frame([965.0, 650.0], fr(), fs([100, 20]), fs([10, 200])),
        Ok(Decision::Possible)
    );
    assert_eq!(
        frame([980.0, 530.0], fr(), fs([100, 20]), fs([10, 200])),
        Ok(Decision::Rejected)
    );
}
#[test]
fn frame_no_write_qualification_is_per_resource() {
    let unloaded = FrameResource {
        loaded_index: -1,
        ..fs([100, 20])
    };
    for (a, b) in [(unloaded, fs([100, 20])), (fs([100, 20]), unloaded)] {
        assert_eq!(
            frame([970.0, 530.0], fr(), a, b),
            unsupported(Unsupported::Basis(basis::Unsupported::UnloadedResource))
        );
    }
}
#[test]
fn frame_preserves_two_stage_size_truncation() {
    let s = FrameResource {
        tiles: [1, 3],
        ..fs([100, 11])
    };
    let r = FrameRenderer {
        local_scale: 2.0,
        ..fr()
    };
    // floor(11/3)=3, then*2=6. Collapsing to floor(22/3)=7 shifts top to533.
    assert_eq!(
        frame([970.0, 533.0], r, s, fs([100, 20])),
        Ok(Decision::Rejected)
    );
    assert_eq!(
        frame([970.0, 534.0], r, s, fs([100, 20])),
        Ok(Decision::Possible)
    );
}
#[test]
fn frame_preserves_hit_multiplication_order_at_one_ulp_boundary() {
    let r = FrameRenderer {
        local_scale: 0.3,
        ..fr()
    };
    let h = FrameResource {
        tiles: [3, 1],
        ..fs([10, 100])
    };
    // (10*.3)*(1/3) rounds1, whereas (10*(1/3))*.3 exceeds1.
    assert_eq!(
        frame([961.0, 535.0], r, fs([100, 20]), h),
        Ok(Decision::Rejected)
    );
    assert_eq!(
        frame([960.0, 535.0], r, fs([100, 20]), h),
        Ok(Decision::Possible)
    );
}
#[test]
fn frame_zero_qword_size_remains_unsupported() {
    let small = FrameResource {
        tiles: [2, 2],
        ..fs([1, 1])
    };
    assert_eq!(
        frame([960.0, 540.0], fr(), small, fs([100, 20])),
        unsupported(Unsupported::Basis(basis::Unsupported::ZeroSizeFallback))
    );
}
#[test]
fn masked_strict_unsigned_edges_differ_from_frame() {
    let s = ms([100, 20]);
    for p in [
        [960.0, 530.0],
        [1060.0, 530.0],
        [970.0, 520.0],
        [970.0, 540.0],
    ] {
        assert_eq!(masked(p, mr(), s, s), Ok(Decision::Rejected));
    }
    assert_eq!(masked([961.0, 521.0], mr(), s, s), Ok(Decision::Possible));
    assert_eq!(masked([1059.0, 539.0], mr(), s, s), Ok(Decision::Possible));
}
#[test]
fn masked_half_scale_does_not_shrink_unscaled_hit_bounds() {
    let r = MaskedRenderer {
        local_scale: 0.5,
        ..mr()
    };
    assert_eq!(
        masked([1035.0, 545.0], r, ms([100, 20]), ms([100, 20])),
        Ok(Decision::Possible)
    );
    // A scaled50x10 hit box would falsely reject the point above.
    assert_eq!(
        masked([1060.0, 545.0], r, ms([100, 20]), ms([100, 20])),
        Ok(Decision::Rejected)
    );
}
#[test]
fn masked_double_scale_does_not_expand_unscaled_hit_width() {
    let r = MaskedRenderer {
        local_scale: 2.0,
        ..mr()
    };
    assert_eq!(
        masked([1059.0, 510.0], r, ms([100, 20]), ms([100, 20])),
        Ok(Decision::Possible)
    );
    assert_eq!(
        masked([1070.0, 510.0], r, ms([100, 20]), ms([100, 20])),
        Ok(Decision::Rejected)
    );
}
#[test]
fn masked_separate_hit_resource_is_not_size_scaled_or_tiled() {
    let h = MaskedResource {
        tiles: [0, 0],
        orientation: i32::MIN,
        ..ms([10, 200])
    };
    // Hit virtual88 never consumes tile/orientation fields of its resource.
    assert_eq!(
        masked([965.0, 650.0], mr(), ms([100, 20]), h),
        Ok(Decision::Possible)
    );
    assert_eq!(
        masked([980.0, 530.0], mr(), ms([100, 20]), h),
        Ok(Decision::Rejected)
    );
}
#[test]
fn masked_orientation_swaps_size_denominators_only_when_exactly_one() {
    let size = MaskedResource {
        tiles: [2, 4],
        orientation: 1,
        ..ms([100, 40])
    };
    let hit = ms([100, 5]);
    assert_eq!(
        masked([970.0, 521.0], mr(), size, hit),
        Ok(Decision::Possible)
    );
    for orientation in [0, 2, -1] {
        assert_eq!(
            masked(
                [970.0, 521.0],
                mr(),
                MaskedResource {
                    orientation,
                    ..size
                },
                hit
            ),
            Ok(Decision::Rejected)
        );
    }
}
#[test]
fn masked_preserves_two_truncations_before_position() {
    let r = MaskedRenderer {
        local_scale: 2.0,
        ..mr()
    };
    let size = MaskedResource {
        tiles: [1, 3],
        ..ms([100, 11])
    };
    assert_eq!(
        masked([970.0, 534.0], r, size, ms([100, 20])),
        Ok(Decision::Rejected)
    );
    assert_eq!(
        masked([970.0, 535.0], r, size, ms([100, 20])),
        Ok(Decision::Possible)
    );
}
#[test]
fn masked_position_truncates_fractional_origin_toward_zero() {
    let r = MaskedRenderer {
        common: Common {
            translation: [0.9, 0.9],
            ..common()
        },
        ..mr()
    };
    assert_eq!(
        masked([961.0, 520.0], r, ms([100, 20]), ms([100, 20])),
        Ok(Decision::Possible)
    );
    assert_eq!(
        masked([960.0, 520.0], r, ms([100, 20]), ms([100, 20])),
        Ok(Decision::Rejected)
    );
    assert_eq!(
        masked([961.0, 519.0], r, ms([100, 20]), ms([100, 20])),
        Ok(Decision::Rejected)
    );
}
#[test]
fn masked_negative_origin_is_unsupported_not_a_miss() {
    for translation in [[-961.0, 0.0], [0.0, 521.0]] {
        let r = MaskedRenderer {
            common: Common {
                translation,
                ..common()
            },
            ..mr()
        };
        assert_eq!(
            masked([10.0, 10.0], r, ms([100, 20]), ms([100, 20])),
            unsupported(Unsupported::NegativeMaskedOrigin)
        );
    }
}
#[test]
fn masked_zero_hit_dimension_is_empty_with_valid_nonzero_size() {
    for dimensions in [[0, 20], [100, 0], [0, 0]] {
        assert_eq!(
            masked([970.0, 530.0], mr(), ms([100, 20]), ms(dimensions)),
            Ok(Decision::Rejected)
        );
    }
}
#[test]
fn masked_single_zero_size_component_is_not_qword_fallback() {
    assert_eq!(
        masked([970.0, 530.0], mr(), ms([0, 20]), ms([100, 20])),
        Ok(Decision::Possible)
    );
    assert_eq!(
        masked([970.0, 545.0], mr(), ms([100, 0]), ms([100, 20])),
        Ok(Decision::Possible)
    );
    assert_eq!(
        masked([970.0, 545.0], mr(), ms([0, 0]), ms([100, 20])),
        unsupported(Unsupported::Basis(basis::Unsupported::ZeroSizeFallback))
    );
}
#[test]
fn masked_zero_hit_is_not_permission_for_malformed_size_data() {
    assert_eq!(
        masked(
            [970.0, 530.0],
            mr(),
            MaskedResource {
                tiles: [0, 1],
                ..ms([100, 20])
            },
            ms([0, 0])
        ),
        Err(GeometryError::TileCount)
    );
    assert_eq!(
        masked([970.0, 530.0], mr(), ms([-1, 20]), ms([0, 0])),
        Err(GeometryError::Dimensions)
    );
}
#[test]
fn masked_unknown_resource_refuses_before_geometry_interpretation() {
    let bad = MaskedResource {
        primary_rva: FRAME_RESOURCE_PRIMARY_RVA,
        dimensions: [-1, -1],
        tiles: [0, 0],
        orientation: 0,
    };
    for (s, h) in [(bad, ms([100, 20])), (ms([100, 20]), bad)] {
        assert_eq!(
            masked([970.0, 530.0], mr(), s, h),
            unsupported(Unsupported::ResourcePrimary)
        );
    }
}
#[test]
fn progress_has_inclusive_left_exclusive_right_strict_top_inclusive_bottom() {
    for p in [[960.0, 521.0], [1059.0, 540.0]] {
        assert_eq!(pg(p, pr(), ps([100, 20])), Ok(Decision::Possible));
    }
    for p in [
        [959.0, 530.0],
        [1060.0, 530.0],
        [970.0, 520.0],
        [970.0, 541.0],
    ] {
        assert_eq!(pg(p, pr(), ps([100, 20])), Ok(Decision::Rejected));
    }
}
#[test]
fn progress_bottom_edge_cannot_use_sprite_upper_exclusion() {
    assert_eq!(
        pg([970.0, 540.0], pr(), ps([100, 20])),
        Ok(Decision::Possible)
    );
    assert_eq!(
        frame([970.0, 540.0], fr(), fs([100, 20]), fs([100, 20])),
        Ok(Decision::Rejected)
    );
}
#[test]
fn progress_preserves_signed_origin_with_viewport_overlap() {
    let r = ProgressRenderer {
        common: Common {
            translation: [-970.0, 0.0],
            ..common()
        },
        ..pr()
    };
    assert_eq!(pg([0.0, 530.0], r, ps([100, 20])), Ok(Decision::Possible));
    assert_eq!(pg([90.0, 530.0], r, ps([100, 20])), Ok(Decision::Rejected));
}
#[test]
fn progress_fractional_origin_is_truncated_before_integer_reduction() {
    let r = ProgressRenderer {
        common: Common {
            translation: [0.9, 0.9],
            ..common()
        },
        ..pr()
    };
    assert_eq!(pg([960.0, 520.0], r, ps([100, 20])), Ok(Decision::Possible));
    assert_eq!(pg([960.0, 519.0], r, ps([100, 20])), Ok(Decision::Rejected));
    assert_eq!(pg([960.0, 539.0], r, ps([100, 20])), Ok(Decision::Possible));
}
#[test]
fn progress_single_zero_dimension_has_empty_integer_interval() {
    for dimensions in [[0, 20], [100, 0]] {
        assert_eq!(
            pg([970.0, 530.0], pr(), ps(dimensions)),
            Ok(Decision::Rejected)
        );
    }
    assert_eq!(
        pg([970.0, 530.0], pr(), ps([0, 0])),
        unsupported(Unsupported::Basis(basis::Unsupported::ZeroSizeFallback))
    );
}
#[test]
fn progress_requires_explicit_normal_zero_sincos_basis() {
    assert_eq!(
        progress(
            [970.0, 530.0],
            pr(),
            ps([100, 20]),
            ProgressTrig::Unqualified
        ),
        unsupported(Unsupported::TrigBasis)
    );
    assert_eq!(
        pg([970.0, 530.0], pr(), ps([100, 20])),
        Ok(Decision::Possible)
    );
    assert_eq!(
        pg(
            [970.0, 530.0],
            ProgressRenderer {
                rotation: -0.0,
                ..pr()
            },
            ps([100, 20])
        ),
        Ok(Decision::Possible)
    );
}
#[test]
fn progress_no_near_zero_angle_approximation() {
    for rotation in [f32::from_bits(1), 0.0001, -0.0001, 1.0] {
        assert_eq!(
            pg(
                [970.0, 530.0],
                ProgressRenderer { rotation, ..pr() },
                ps([100, 20])
            ),
            unsupported(Unsupported::Basis(basis::Unsupported::Rotation))
        );
    }
    assert_eq!(
        pg(
            [970.0, 530.0],
            ProgressRenderer {
                rotation: f32::NAN,
                ..pr()
            },
            ps([100, 20])
        ),
        Err(GeometryError::NonFinite)
    );
}
#[test]
fn progress_unknown_resource_does_not_reuse_sprite_fields() {
    assert_eq!(
        pg(
            [970.0, 530.0],
            pr(),
            ProgressResource {
                primary_rva: basis::SPRITE_TYPE_PRIMARY_RVA,
                dimensions: [-1, -1]
            }
        ),
        unsupported(Unsupported::ResourcePrimary)
    );
}
#[test]
fn every_new_class_rejects_fractional_point_as_unsupported() {
    let p = [970.25, 530.0];
    assert_eq!(
        frame(p, fr(), fs([100, 20]), fs([100, 20])),
        unsupported(Unsupported::IntegerPoint)
    );
    assert_eq!(
        masked(p, mr(), ms([100, 20]), ms([100, 20])),
        unsupported(Unsupported::IntegerPoint)
    );
    assert_eq!(
        pg(p, pr(), ps([100, 20])),
        unsupported(Unsupported::IntegerPoint)
    );
}
#[test]
fn every_new_class_refuses_nonunit_inherited_transform() {
    for inherited_scale in [0.5, 0.8, 2.0] {
        let c = Common {
            inherited_scale,
            ..common()
        };
        let expected = unsupported(Unsupported::Basis(basis::Unsupported::InheritedScale));
        assert_eq!(
            frame(
                [970.0, 530.0],
                FrameRenderer { common: c, ..fr() },
                fs([100, 20]),
                fs([100, 20])
            ),
            expected
        );
        assert_eq!(
            masked(
                [970.0, 530.0],
                MaskedRenderer { common: c, ..mr() },
                ms([100, 20]),
                ms([100, 20])
            ),
            expected
        );
        assert_eq!(
            pg(
                [970.0, 530.0],
                ProgressRenderer { common: c, ..pr() },
                ps([100, 20])
            ),
            expected
        );
    }
}
#[test]
fn common_rotation_is_independent_of_progress_own_zero_angle() {
    let c = Common {
        rotation: 0.001,
        ..common()
    };
    let expected = unsupported(Unsupported::Basis(basis::Unsupported::Rotation));
    assert_eq!(
        frame(
            [970.0, 530.0],
            FrameRenderer { common: c, ..fr() },
            fs([100, 20]),
            fs([100, 20])
        ),
        expected
    );
    assert_eq!(
        masked(
            [970.0, 530.0],
            MaskedRenderer { common: c, ..mr() },
            ms([100, 20]),
            ms([100, 20])
        ),
        expected
    );
    assert_eq!(
        pg(
            [970.0, 530.0],
            ProgressRenderer { common: c, ..pr() },
            ps([100, 20])
        ),
        expected
    );
}
#[test]
fn malformed_consumed_sizes_tiles_and_scales_are_errors() {
    for dimensions in [[-1, 20], [100, MAX_DIMENSION + 1], [i32::MAX, 20]] {
        assert_eq!(
            masked([970.0, 530.0], mr(), ms(dimensions), ms([100, 20])),
            Err(GeometryError::Dimensions)
        );
        assert_eq!(
            pg([970.0, 530.0], pr(), ps(dimensions)),
            Err(GeometryError::Dimensions)
        );
    }
    for tiles in [[0, 1], [-1, 1], [1, MAX_TILES + 1]] {
        assert_eq!(
            frame(
                [970.0, 530.0],
                fr(),
                FrameResource {
                    tiles,
                    ..fs([100, 20])
                },
                fs([100, 20])
            ),
            Err(GeometryError::TileCount)
        );
        assert_eq!(
            masked(
                [970.0, 530.0],
                mr(),
                MaskedResource {
                    tiles,
                    ..ms([100, 20])
                },
                ms([100, 20])
            ),
            Err(GeometryError::TileCount)
        );
    }
    for local_scale in [0.0, -1.0, MAX_LOCAL_SCALE + 1.0] {
        assert_eq!(
            frame(
                [970.0, 530.0],
                FrameRenderer {
                    local_scale,
                    ..fr()
                },
                fs([100, 20]),
                fs([100, 20])
            ),
            Err(GeometryError::Scale)
        );
        assert_eq!(
            masked(
                [970.0, 530.0],
                MaskedRenderer {
                    local_scale,
                    ..mr()
                },
                ms([100, 20]),
                ms([100, 20])
            ),
            Err(GeometryError::Scale)
        );
    }
}
#[test]
fn nonfinite_canvas_point_and_translation_do_not_become_misses() {
    for value in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
        assert_eq!(
            pg([value, 530.0], pr(), ps([100, 20])),
            Err(GeometryError::NonFinite)
        );
        assert_eq!(
            masked(
                [970.0, 530.0],
                MaskedRenderer {
                    common: Common {
                        translation: [value, 0.0],
                        ..common()
                    },
                    ..mr()
                },
                ms([100, 20]),
                ms([100, 20])
            ),
            Err(GeometryError::NonFinite)
        );
        assert_eq!(
            frame(
                [970.0, 530.0],
                FrameRenderer {
                    local_scale: value,
                    ..fr()
                },
                fs([100, 20]),
                fs([100, 20])
            ),
            Err(GeometryError::NonFinite)
        );
    }
    for canvas in [[0, 1080], [1920, -1], [MAX_CANVAS + 1, 1080]] {
        assert_eq!(
            pg(
                [970.0, 530.0],
                ProgressRenderer {
                    common: Common { canvas, ..common() },
                    ..pr()
                },
                ps([100, 20])
            ),
            Err(GeometryError::Canvas)
        );
    }
    for p in [[-1.0, 530.0], [1920.0, 530.0], [970.0, 1080.0]] {
        assert_eq!(pg(p, pr(), ps([100, 20])), Err(GeometryError::Point));
    }
}
#[test]
fn translation_and_conversion_bounds_prevent_wrap_or_saturating_cast() {
    let c = Common {
        translation: [MAX_COORDINATE + 1.0, 0.0],
        ..common()
    };
    assert_eq!(
        pg(
            [970.0, 530.0],
            ProgressRenderer { common: c, ..pr() },
            ps([100, 20])
        ),
        Err(GeometryError::Translation)
    );
    let c = Common {
        translation: [MAX_COORDINATE, 0.0],
        ..common()
    };
    assert_eq!(
        pg(
            [970.0, 530.0],
            ProgressRenderer { common: c, ..pr() },
            ps([100, 20])
        ),
        Err(GeometryError::Overflow)
    );
    assert_eq!(
        masked(
            [970.0, 530.0],
            MaskedRenderer { common: c, ..mr() },
            ms([100, 20]),
            ms([100, 20])
        ),
        Err(GeometryError::Overflow)
    );
    assert_eq!(
        frame(
            [970.0, 530.0],
            FrameRenderer { common: c, ..fr() },
            fs([100, 20]),
            fs([100, 20])
        ),
        Err(GeometryError::Overflow)
    );
}
#[test]
fn progress_large_valid_integer_differences_stay_exact_and_bounded() {
    // Exercises a near-policy signed origin, far from the on-screen point,
    // without overflow, integer precision loss or a false positive.
    let r = ProgressRenderer {
        common: Common {
            translation: [-1_000_000.0, 0.0],
            ..common()
        },
        ..pr()
    };
    assert_eq!(
        pg([1919.0, 1079.0], r, ps([MAX_DIMENSION, MAX_DIMENSION])),
        Ok(Decision::Rejected)
    );
}

#[test]
fn zero_angle_instruction_reduction_preserves_bounded_integer_points() {
    // Independent check of the omitted rotation sequence, not an OS trig call.
    // Low return lane is sin(±0), high lane cos(±0)=1 by the explicit contract.
    for sin in [0.0_f32, -0.0_f32] {
        for origin in [[960_i32, 520_i32], [-999_040, -16_384], [999_999, 999_999]] {
            for height in [0_i32, 1, 16_384] {
                for p in [[0_i32, 0_i32], [960, 540], [32_767, 32_767]] {
                    let dx = (p[0] as f32) - (origin[0] as f32);
                    let pivot = origin[1].checked_add(height).unwrap();
                    let dy = (p[1] as f32) - (pivot as f32);
                    let dx_sin = dx * sin;
                    let dy_sin = sin * dy;
                    let dx_cos = dx * 1.0_f32;
                    let rx = (dx_cos - dy_sin).trunc() as i32;
                    let dy_cos = dy * 1.0_f32;
                    let ry = (dy_cos + dx_sin).trunc() as i32;
                    assert_eq!([rx + origin[0], ry + origin[1] + height], p);
                }
            }
        }
    }
}
