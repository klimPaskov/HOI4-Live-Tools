use super::*;

fn common() -> Common {
    Common {
        canvas: [1920, 1080],
        translation: [0.0, 0.0],
        inherited_scale: 1.0,
        rotation: 0.0,
    }
}
fn resource(texture: [i32; 2], tiles: [i32; 2]) -> SpriteType {
    SpriteType {
        primary_rva: SPRITE_TYPE_PRIMARY_RVA,
        texture,
        tiles,
        orientation: 0,
        loaded_index: 7,
    }
}
fn known(r: SpriteType) -> SpriteResource {
    SpriteResource::SpriteType(r)
}
fn standard_sprite(point: [f32; 2], c: Common) -> Result<Decision, GeometryError> {
    let r = resource([100, 20], [1, 1]);
    sprite(point, c, 1.0, known(r), known(r))
}

#[test]
fn position_object_has_inclusive_lower_and_exclusive_upper_edges() {
    let c = Common {
        translation: [100.0, -200.0],
        ..common()
    };
    for point in [[100.0, 200.0], [109.999, 209.999]] {
        assert_eq!(position_object(point, c, [10, 10]), Ok(Decision::Possible));
    }
    for point in [[99.99, 205.0], [110.0, 205.0], [105.0, 210.0]] {
        assert_eq!(position_object(point, c, [10, 10]), Ok(Decision::Rejected));
    }
}
#[test]
fn position_object_uses_its_own_origin_without_canvas_half() {
    assert_eq!(
        position_object([50.0, 50.0], common(), [100, 100]),
        Ok(Decision::Possible)
    );
    assert_eq!(
        position_object([960.0, 540.0], common(), [100, 100]),
        Ok(Decision::Rejected)
    );
}
#[test]
fn position_object_truncates_negative_translation_toward_zero() {
    let c = Common {
        translation: [-0.9, 0.9],
        ..common()
    };
    assert_eq!(
        position_object([0.0, 0.0], c, [1, 1]),
        Ok(Decision::Possible)
    );
    assert_eq!(
        position_object([1.0, 0.0], c, [1, 1]),
        Ok(Decision::Rejected)
    );
}
#[test]
fn position_object_preserves_fractional_point_at_upper_edge() {
    let c = Common {
        translation: [100.9, -200.9],
        ..common()
    };
    assert_eq!(
        position_object([109.999, 209.999], c, [10, 10]),
        Ok(Decision::Possible)
    );
    assert_eq!(
        position_object([110.0, 209.999], c, [10, 10]),
        Ok(Decision::Rejected)
    );
}
#[test]
fn zero_direct_dimensions_have_no_inside_point() {
    for dims in [[0, 10], [10, 0], [0, 0]] {
        assert_eq!(
            position_object([0.0, 0.0], common(), dims),
            Ok(Decision::Rejected)
        );
        assert_eq!(
            cornered([960.0, 540.0], common(), dims),
            Ok(Decision::Rejected)
        );
        assert_eq!(
            instant_text_alignment0([1.0, 1.0], common(), [0, 0], dims, 0),
            Ok(Decision::Rejected)
        );
    }
}
#[test]
fn cornered_origin_does_not_subtract_its_height() {
    assert_eq!(
        cornered([960.0, 540.0], common(), [100, 20]),
        Ok(Decision::Possible)
    );
    assert_eq!(
        cornered([960.0, 539.999], common(), [100, 20]),
        Ok(Decision::Rejected)
    );
    assert_eq!(
        cornered([1060.0, 550.0], common(), [100, 20]),
        Ok(Decision::Rejected)
    );
    assert_eq!(
        cornered([1000.0, 560.0], common(), [100, 20]),
        Ok(Decision::Rejected)
    );
}
#[test]
fn cornered_uses_float_origin_and_float_point() {
    let c = Common {
        translation: [-0.25, 0.25],
        ..common()
    };
    assert_eq!(
        cornered([959.7, 550.0], c, [100, 20]),
        Ok(Decision::Rejected)
    );
    assert_eq!(
        cornered([959.75, 539.75], c, [100, 20]),
        Ok(Decision::Possible)
    );
    assert_eq!(
        cornered([1059.749, 559.749], c, [100, 20]),
        Ok(Decision::Possible)
    );
}
#[test]
fn instant_text_edges_are_strict_on_both_sides() {
    let c = Common {
        translation: [100.0, -200.0],
        ..common()
    };
    for point in [
        [100.0, 210.0],
        [110.0, 210.0],
        [105.0, 200.0],
        [105.0, 220.0],
    ] {
        assert_eq!(
            instant_text_alignment0(point, c, [0, 0], [10, 20], 0),
            Ok(Decision::Rejected)
        );
    }
    assert_eq!(
        instant_text_alignment0([101.0, 201.0], c, [0, 0], [10, 20], 0),
        Ok(Decision::Possible)
    );
}
#[test]
fn instant_text_truncates_point_before_strict_lower_test() {
    let c = Common {
        translation: [100.0, -200.0],
        ..common()
    };
    assert_eq!(
        instant_text_alignment0([100.99, 201.0], c, [0, 0], [10, 20], 0),
        Ok(Decision::Rejected)
    );
    assert_eq!(
        instant_text_alignment0([101.01, 201.0], c, [0, 0], [10, 20], 0),
        Ok(Decision::Possible)
    );
}
#[test]
fn instant_text_integer_offsets_and_y_orientation_are_independent() {
    let c = Common {
        translation: [100.9, -200.9],
        ..common()
    };
    assert_eq!(
        instant_text_alignment0([106.0, 208.0], c, [5, 7], [10, 20], 0),
        Ok(Decision::Possible)
    );
    assert_eq!(
        instant_text_alignment0([105.9, 208.0], c, [5, 7], [10, 20], 0),
        Ok(Decision::Rejected)
    );
}
#[test]
fn instant_text_other_alignments_are_unsupported_even_when_far_away() {
    for alignment in [-1, 1, 2, 3, 4, 5, 6, 7] {
        assert_eq!(
            instant_text_alignment0([1900.0, 1000.0], common(), [0, 0], [10, 20], alignment),
            Ok(Decision::Unsupported(Unsupported::TextAlignment))
        );
    }
}
#[test]
fn sprite_has_top_at_canvas_half_minus_its_size_height() {
    assert_eq!(
        standard_sprite([960.0, 520.0], common()),
        Ok(Decision::Possible)
    );
    assert_eq!(
        standard_sprite([1059.9, 539.9], common()),
        Ok(Decision::Possible)
    );
    for point in [[959.99, 530.0], [1060.0, 530.0], [1000.0, 540.0]] {
        assert_eq!(standard_sprite(point, common()), Ok(Decision::Rejected));
    }
}
#[test]
fn sprite_truncates_both_point_and_fractional_origin() {
    let c = Common {
        translation: [-950.1, 419.1],
        ..common()
    };
    // True float left is about9.9. The engine's integer lower origin is9.
    assert_eq!(standard_sprite([9.5, 100.5], c), Ok(Decision::Possible));
    assert_eq!(standard_sprite([8.99, 100.5], c), Ok(Decision::Rejected));
}
#[test]
fn sprite_tile_orientation_swaps_both_denominators() {
    let r = resource([120, 80], [3, 2]);
    assert_eq!(
        sprite([1010.0, 520.0], common(), 1.0, known(r), known(r)),
        Ok(Decision::Rejected)
    );
    assert_eq!(
        sprite([980.0, 505.0], common(), 1.0, known(r), known(r)),
        Ok(Decision::Possible)
    );
    let swapped = SpriteType {
        orientation: 1,
        ..r
    };
    assert_eq!(
        sprite(
            [1010.0, 520.0],
            common(),
            1.0,
            known(swapped),
            known(swapped)
        ),
        Ok(Decision::Possible)
    );
    assert_eq!(
        sprite(
            [980.0, 505.0],
            common(),
            1.0,
            known(swapped),
            known(swapped)
        ),
        Ok(Decision::Rejected)
    );
}
#[test]
fn sprite_orientation_branch_is_exactly_equal_one() {
    let r = resource([120, 80], [3, 2]);
    for orientation in [-7, 0, 2, i32::MAX] {
        let r = SpriteType { orientation, ..r };
        assert_eq!(
            sprite([1010.0, 520.0], common(), 1.0, known(r), known(r)),
            Ok(Decision::Rejected)
        );
    }
}
#[test]
fn sprite_size_truncates_before_and_after_local_scale() {
    let r = resource([10, 11], [1, 3]);
    // Per-tile height3 then local scale2 gives6, not trunc(22/3)==7.
    assert_eq!(
        sprite([960.0, 533.0], common(), 2.0, known(r), known(r)),
        Ok(Decision::Rejected)
    );
    assert_eq!(
        sprite([960.0, 534.0], common(), 2.0, known(r), known(r)),
        Ok(Decision::Possible)
    );
}
#[test]
fn sprite_f32_hit_product_grouping_preserves_integer_boundary() {
    let r = resource([10, 100], [3, 1]);
    let c = Common {
        translation: [-960.0, 510.0],
        ..common()
    };
    // (10*.3)*(1/3) rounds to1; (10*(1/3))*.3 is one ULP above1.
    assert_eq!(
        sprite([1.0, 0.0], c, 0.3, known(r), known(r)),
        Ok(Decision::Rejected)
    );
    assert_eq!(
        sprite([0.999_999_94, 0.0], c, 0.3, known(r), known(r)),
        Ok(Decision::Possible)
    );
}
#[test]
fn sprite_hit_resource_must_not_be_replaced_by_size_resource() {
    let size = resource([100, 20], [1, 1]);
    let hit = resource([10, 200], [1, 1]);
    assert_eq!(
        sprite([965.0, 650.0], common(), 1.0, known(size), known(hit)),
        Ok(Decision::Possible)
    );
    assert_eq!(
        sprite([980.0, 530.0], common(), 1.0, known(size), known(hit)),
        Ok(Decision::Rejected)
    );
}
#[test]
fn zero_sprite_size_keeps_fallback_unsupported() {
    let r = resource([1, 1], [64, 64]);
    assert_eq!(
        sprite([1900.0, 1000.0], common(), 1.0, known(r), known(r)),
        Ok(Decision::Unsupported(Unsupported::ZeroSizeFallback))
    );
}
#[test]
fn sprite_one_zero_size_component_does_not_trigger_qword_zero_fallback() {
    let r = resource([10, 100], [3, 1]);
    let c = Common {
        translation: [-960.0, 510.0],
        ..common()
    };
    assert_eq!(
        sprite([0.0, 0.0], c, 0.3, known(r), known(r)),
        Ok(Decision::Possible)
    );
}
#[test]
fn unloaded_size_or_hit_resource_is_not_a_proved_miss() {
    let r = resource([100, 20], [1, 1]);
    for index in [-1, -2, i32::MIN] {
        let unready = SpriteType {
            loaded_index: index,
            ..r
        };
        assert_eq!(
            sprite([0.0, 0.0], common(), 1.0, known(unready), known(r)),
            Ok(Decision::Unsupported(Unsupported::UnloadedResource))
        );
        assert_eq!(
            sprite([0.0, 0.0], common(), 1.0, known(r), known(unready)),
            Ok(Decision::Unsupported(Unsupported::UnloadedResource))
        );
    }
}
#[test]
fn unknown_resource_type_refuses_before_interpreting_its_dimensions() {
    let r = resource([100, 20], [1, 1]);
    let unknown = SpriteType {
        primary_rva: 0x34c0380,
        texture: [-1, i32::MAX],
        tiles: [0, 0],
        ..r
    };
    assert_eq!(
        sprite([0.0, 0.0], common(), 1.0, known(unknown), known(r)),
        Ok(Decision::Unsupported(Unsupported::ResourceType))
    );
}
#[test]
fn absent_exact_size_or_hit_dispatch_stays_unsupported() {
    let r = known(resource([100, 20], [1, 1]));
    assert_eq!(
        sprite([0.0, 0.0], common(), 1.0, SpriteResource::Unsupported, r),
        Ok(Decision::Unsupported(Unsupported::SizeDispatch))
    );
    assert_eq!(
        sprite([0.0, 0.0], common(), 1.0, r, SpriteResource::Unsupported),
        Ok(Decision::Unsupported(Unsupported::HitDispatch))
    );
}
#[test]
fn nonunit_inherited_transform_is_not_approximated() {
    for scale in [0.5, 0.8, 1.000_000_1] {
        let c = Common {
            inherited_scale: scale,
            ..common()
        };
        assert_eq!(
            cornered([0.0, 0.0], c, [100, 20]),
            Ok(Decision::Unsupported(Unsupported::InheritedScale))
        );
        assert_eq!(
            standard_sprite([0.0, 0.0], c),
            Ok(Decision::Unsupported(Unsupported::InheritedScale))
        );
    }
}
#[test]
fn nonzero_rotation_cannot_be_silently_rejected_by_unrotated_box() {
    let c = Common {
        rotation: std::f32::consts::FRAC_PI_2,
        ..common()
    };
    assert_eq!(
        cornered([990.0, 520.0], c, [100, 20]),
        Ok(Decision::Unsupported(Unsupported::Rotation))
    );
    assert_eq!(
        position_object([990.0, 520.0], c, [100, 20]),
        Ok(Decision::Unsupported(Unsupported::Rotation))
    );
    assert_eq!(
        standard_sprite([990.0, 520.0], c),
        Ok(Decision::Unsupported(Unsupported::Rotation))
    );
}
#[test]
fn positive_fractional_sprite_local_scales_are_supported_separately() {
    let r = resource([100, 20], [1, 1]);
    for (scale, top) in [(0.5, 530.0), (0.8, 524.0)] {
        assert_eq!(
            sprite([960.0, top], common(), scale, known(r), known(r)),
            Ok(Decision::Possible)
        );
        assert_eq!(
            sprite([960.0, top - 1.0], common(), scale, known(r), known(r)),
            Ok(Decision::Rejected)
        );
    }
}
#[test]
fn invalid_canvas_and_point_are_explicit_errors() {
    for canvas in [[0, 1080], [-1, 1080], [MAX_CANVAS + 1, 1080]] {
        assert_eq!(
            cornered([0.0, 0.0], Common { canvas, ..common() }, [10, 10]),
            Err(GeometryError::Canvas)
        );
    }
    for point in [[-0.01, 0.0], [1920.0, 0.0], [0.0, 1080.0]] {
        assert_eq!(
            cornered(point, common(), [10, 10]),
            Err(GeometryError::Point)
        );
    }
    for value in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
        assert_eq!(
            cornered([value, 0.0], common(), [10, 10]),
            Err(GeometryError::NonFinite)
        );
    }
}
#[test]
fn nonfinite_or_unbounded_translation_never_becomes_miss() {
    for value in [f32::NAN, f32::INFINITY] {
        assert_eq!(
            position_object(
                [0.0, 0.0],
                Common {
                    translation: [value, 0.0],
                    ..common()
                },
                [1, 1]
            ),
            Err(GeometryError::NonFinite)
        );
    }
    assert_eq!(
        position_object(
            [0.0, 0.0],
            Common {
                translation: [MAX_COORDINATE + 1.0, 0.0],
                ..common()
            },
            [1, 1]
        ),
        Err(GeometryError::Translation)
    );
}
#[test]
fn invalid_scales_and_rotation_are_errors_not_rejects() {
    let r = known(resource([100, 20], [1, 1]));
    for scale in [0.0, -1.0, MAX_LOCAL_SCALE + 1.0] {
        assert_eq!(
            sprite([0.0, 0.0], common(), scale, r, r),
            Err(GeometryError::Scale)
        );
    }
    assert_eq!(
        sprite([0.0, 0.0], common(), f32::NAN, r, r),
        Err(GeometryError::NonFinite)
    );
    assert_eq!(
        cornered(
            [0.0, 0.0],
            Common {
                rotation: f32::INFINITY,
                ..common()
            },
            [10, 10]
        ),
        Err(GeometryError::NonFinite)
    );
    assert_eq!(
        cornered(
            [0.0, 0.0],
            Common {
                inherited_scale: 0.0,
                ..common()
            },
            [10, 10]
        ),
        Err(GeometryError::Scale)
    );
}
#[test]
fn bad_texture_and_tile_metadata_are_bounded_errors() {
    let r = resource([100, 20], [1, 1]);
    for texture in [[0, 20], [-1, 20], [MAX_DIMENSION + 1, 20]] {
        assert_eq!(
            sprite(
                [0.0, 0.0],
                common(),
                1.0,
                known(SpriteType { texture, ..r }),
                known(r)
            ),
            Err(GeometryError::Dimensions)
        );
    }
    for tiles in [[0, 1], [-1, 1], [MAX_TILES + 1, 1]] {
        assert_eq!(
            sprite(
                [0.0, 0.0],
                common(),
                1.0,
                known(SpriteType { tiles, ..r }),
                known(r)
            ),
            Err(GeometryError::TileCount)
        );
    }
}
#[test]
fn invalid_direct_dimensions_and_offset_overflow_refuse() {
    for dims in [[-1, 10], [i32::MAX, 10], [10, MAX_DIMENSION + 1]] {
        assert_eq!(
            cornered([0.0, 0.0], common(), dims),
            Err(GeometryError::Dimensions)
        );
        assert_eq!(
            position_object([0.0, 0.0], common(), dims),
            Err(GeometryError::Dimensions)
        );
    }
    assert_eq!(
        instant_text_alignment0([0.0, 0.0], common(), [i32::MIN, 0], [10, 10], 0),
        Err(GeometryError::Overflow)
    );
}
#[test]
fn out_of_policy_truncation_intermediate_refuses_without_saturation() {
    let c = Common {
        translation: [MAX_COORDINATE, 0.0],
        ..common()
    };
    assert_eq!(standard_sprite([0.0, 0.0], c), Err(GeometryError::Overflow));
    let c = Common {
        translation: [MAX_COORDINATE, 0.0],
        ..common()
    };
    assert_eq!(
        instant_text_alignment0([0.0, 0.0], c, [1000, 0], [10, 10], 0),
        Err(GeometryError::Overflow)
    );
}
