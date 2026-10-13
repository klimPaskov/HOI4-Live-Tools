use super::*;
fn common() -> Common {
    Common {
        world: [-106.0, -241.0],
        anchor: [512, 288],
        scalar: 1.0,
        hierarchy: Hierarchy {
            closed_to_owned_root_1f0: true,
            conditional_unit_cache_consistent: true,
        },
    }
}
fn codec() -> ClientCodec {
    ClientCodec {
        owned_x11_window: 42,
        observed_sdl_window: 42,
        owned_os_binding_verified: true,
        builtin_point_source_verified: true,
        reciprocal_x11_association_verified: true,
        relative_raw: [0, 0],
        client: [512, 288],
        graphics_physical: [512, 288],
        graphics_logical: [2560, 1440],
        inverse: [5.0, 5.0],
        forward: [0.2, 0.2],
        reviewed_builtin_client_origin: true,
    }
}
fn sprite() -> SpriteNode {
    SpriteNode {
        common: common(),
        texture: [72, 72],
        grid: [1, 1],
        orientation: 0,
        orientation_full_width: true,
        cache_equivalent: true,
        no_mask_rejection_branch: true,
        physical_clip: Some([2, 27, 507, 283]),
    }
}
#[test]
fn sprite_origin_sign_height_and_physical_roundtrip_match_independent_example() {
    let p = point_candidate(&Surface::ExactSpriteNode(sprite()), &codec()).unwrap();
    assert_eq!(p.ideal_logical, [186, 349]);
    assert_eq!(p.client_pixel, [37, 70]);
    assert_eq!(p.quantized_logical, [185, 350]);
    assert!(!p.current_frame_verified && !p.effective_routing_verified && !p.input_authorized);
}
#[test]
fn text_zero_angle_counts_and_strict_interior_survive_physical_quantization() {
    let text = TextResearch {
        common: common(),
        width: 369,
        height: 34,
        resource_count: 3,
        hit_divisor: 3,
        angle: -0.0,
    };
    let p = point_candidate(&Surface::ExactZeroAngleTextResearch(text.clone()), &codec()).unwrap();
    assert_eq!(p.ideal_logical, [211, 368]);
    assert_eq!(p.client_pixel, [42, 74]);
    assert_eq!(p.quantized_logical, [210, 370]);
    for bad in [
        TextResearch {
            angle: 0.01,
            ..text.clone()
        },
        TextResearch {
            hit_divisor: 1,
            ..text.clone()
        },
        TextResearch {
            resource_count: 0,
            ..text.clone()
        },
        TextResearch {
            width: 3,
            ..text.clone()
        },
        TextResearch { height: 1, ..text },
    ] {
        assert!(point_candidate(&Surface::ExactZeroAngleTextResearch(bad), &codec()).is_err());
    }
}
#[test]
fn resource_mixed_width_mask_lazy_and_unknown_branches_refuse() {
    for bad in [
        SpriteNode {
            orientation: 0x100,
            ..sprite()
        },
        SpriteNode {
            orientation_full_width: false,
            ..sprite()
        },
        SpriteNode {
            cache_equivalent: false,
            ..sprite()
        },
        SpriteNode {
            no_mask_rejection_branch: false,
            ..sprite()
        },
        SpriteNode {
            grid: [0, 1],
            ..sprite()
        },
        SpriteNode {
            texture: [1, 72],
            ..sprite()
        },
    ] {
        assert!(point_candidate(&Surface::ExactSpriteNode(bad), &codec()).is_err());
    }
    assert!(point_candidate(&Surface::Unsupported, &codec()).is_err());
}
#[test]
fn unit_cache_hierarchy_mapping_window_scale_and_clipping_fail_closed() {
    let mut s = sprite();
    s.common.hierarchy.conditional_unit_cache_consistent = false;
    assert!(point_candidate(&Surface::ExactSpriteNode(s), &codec()).is_err());
    let mut s = sprite();
    s.common.scalar = 1.001;
    assert!(point_candidate(&Surface::ExactSpriteNode(s), &codec()).is_err());
    let mut s = sprite();
    s.common.world[0] = f32::NAN;
    assert!(point_candidate(&Surface::ExactSpriteNode(s), &codec()).is_err());
    let mut s = sprite();
    s.physical_clip = Some([38, 27, 507, 283]);
    assert!(point_candidate(&Surface::ExactSpriteNode(s), &codec()).is_err());
    for c in [
        ClientCodec {
            observed_sdl_window: 43,
            ..codec()
        },
        ClientCodec {
            relative_raw: [0, 1],
            ..codec()
        },
        ClientCodec {
            reviewed_builtin_client_origin: false,
            ..codec()
        },
        ClientCodec {
            client: [513, 288],
            ..codec()
        },
        ClientCodec {
            inverse: [4.0, 5.0],
            ..codec()
        },
    ] {
        assert!(point_candidate(&Surface::ExactSpriteNode(sprite()), &c).is_err());
    }
}
#[test]
fn subpixel_quantization_cannot_turn_a_tiny_hit_into_an_authorized_click() {
    let text = TextResearch {
        common: common(),
        width: 2,
        height: 2,
        resource_count: 1,
        hit_divisor: 1,
        angle: 0.0,
    };
    assert!(point_candidate(&Surface::ExactZeroAngleTextResearch(text), &codec()).is_err());
}

#[test]
fn recorded_text_control_maps_to_independently_verified_interior() {
    let text = TextResearch {
        common: Common {
            world: [634.0, -47.0],
            ..common()
        },
        width: 123,
        height: 36,
        resource_count: 1,
        hit_divisor: 1,
        angle: 0.0,
    };
    let p = point_candidate(&Surface::ExactZeroAngleTextResearch(text), &codec()).unwrap();
    assert_eq!(p.ideal_logical, [951, 173]);
    assert_eq!(p.client_pixel, [190, 35]);
    assert_eq!(p.quantized_logical, [950, 175]);
    assert!(!p.current_frame_verified && !p.input_authorized && !p.effective_routing_verified);
}
