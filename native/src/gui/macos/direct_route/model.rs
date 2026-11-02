//! Exact restricted05e0 transform/clip reduction and conservative text union.
use super::envelope::{basis, Common, Decision, GeometryError, Unsupported};

pub const EPSILON: f32 = f32::from_bits(0x3a83126f);
pub type Clip = [i32; 4];
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Transform {
    pub xy: [f32; 2],
    pub scale: f32,
    pub unit_path: bool,
}
impl Transform {
    pub const SEED: Self = Self {
        xy: [0.0, 0.0],
        scale: 1.0,
        unit_path: true,
    };
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Clips {
    pub children: Option<Clip>,
    pub self_clip: Option<Clip>,
}
/// children_clip is the scratch10 flag/pointer, NOT the self145 byte. A clip
/// miss never changes this result or suppresses descendant evaluation.
pub fn clips(
    inherited: Option<Clip>,
    mode: i32,
    own: Clip,
    self_present: bool,
    self_candidate: bool,
) -> Clips {
    let children = match mode {
        0 => inherited,
        1 => Some(if let Some(p) = inherited {
            [
                own[0].max(p[0]),
                own[1].max(p[1]),
                own[2].min(p[2]),
                own[3].min(p[3]),
            ]
        } else {
            own
        }),
        _ => None,
    };
    // 7c6..7ea rewrites148/145 only with NONNULL inherited for mode0/1 and
    // self144!=0. Mode1 without inherited follows7ec, preserving self145.
    let rewritten = self_candidate && inherited.is_some() && matches!(mode, 0 | 1);
    let self_clip = if rewritten {
        children
    } else if self_present {
        Some(own)
    } else {
        None
    };
    Clips {
        children,
        self_clip,
    }
}
pub fn clip_contains(clip: Option<Clip>, point: [f32; 2]) -> bool {
    match clip {
        None => true,
        Some(c) => {
            point[0] >= c[0] as f32
                && point[0] <= c[2] as f32
                && point[1] >= c[1] as f32
                && point[1] <= c[3] as f32
        }
    }
}
fn finite(v: f32) -> Result<f32, GeometryError> {
    v.is_finite().then_some(v).ok_or(GeometryError::NonFinite)
}
/// Literal scalar grouping from05e0. incoming scale correction uses the OLD
/// parent scale, while a nonself group's own114 can change the returned scale.
/// Exact zero rotations are a later class-envelope restriction, not invented
/// extra math in this transform procedure (which does not read rotation110).
pub fn transform(
    parent: Transform,
    local: [f32; 2],
    own_scale: f32,
    anchor: bool,
    self_candidate: bool,
    child_traversal: bool,
    canvas: [i32; 2],
) -> Result<Option<Transform>, GeometryError> {
    if !self_candidate && !child_traversal {
        return Ok(None);
    }
    if canvas.iter().any(|v| !(1..=basis::MAX_CANVAS).contains(v)) {
        return Err(GeometryError::Canvas);
    }
    for v in [
        parent.xy[0],
        parent.xy[1],
        parent.scale,
        local[0],
        local[1],
        own_scale,
    ] {
        finite(v)?;
    }
    let scale = if !self_candidate && child_traversal && own_scale.abs() > EPSILON {
        finite(own_scale * parent.scale)?
    } else {
        parent.scale
    };
    let mut x = finite(local[0] + parent.xy[0])?;
    let mut y = finite(local[1] + parent.xy[1])?;
    let correction = finite(parent.scale + -1.0_f32)?;
    if correction.abs() > EPSILON {
        if anchor {
            let dx = finite(local[0] * correction)?;
            x = finite(dx + x)?;
            let dy = finite(local[1] * correction)?;
            y = finite(dy + y)?;
        } else {
            let half_x = finite((canvas[0] as f32) * 0.5)?;
            let dx = finite(half_x + local[0])?;
            let dx = finite(dx * correction)?;
            x = finite(dx + x)?;
            let half_y = finite((canvas[1] as f32) * 0.5)?;
            let dy = finite(local[1] - half_y)?;
            let dy = finite(dy * correction)?;
            y = finite(dy + y)?;
        }
    }
    Ok(Some(Transform {
        xy: [x, y],
        scale,
        unit_path: parent.unit_path && parent.scale == 1.0 && scale == 1.0,
    }))
}
fn convert(d: basis::Decision) -> Decision {
    match d {
        basis::Decision::Rejected => Decision::Rejected,
        basis::Decision::Possible => Decision::Possible,
        basis::Decision::Unsupported(u) => Decision::Unsupported(Unsupported::Basis(u)),
    }
}
/// Alignment0 uses its actual box. For known1..6, all seven switch destinations
/// are covered by nine offset combinations. This is deliberately a superset,
/// not an inferred exact mapping. Every alternative must compute successfully;
/// even one Unsupported/error propagates, and only all-nine rejection is a miss.
pub fn instant_union(
    point: [f32; 2],
    c: Common,
    offset: [i32; 2],
    delta: [i32; 2],
    dims: [i32; 2],
    alignment: i32,
) -> Result<Decision, GeometryError> {
    if alignment == 0 {
        return basis::instant_text_alignment0(point, c, offset, dims, 0).map(convert);
    }
    if !(1..=6).contains(&alignment) {
        return Ok(Decision::Unsupported(Unsupported::Basis(
            basis::Unsupported::TextAlignment,
        )));
    }
    let choices = |axis: usize| -> Result<[i32; 3], GeometryError> {
        Ok([
            offset[axis],
            offset[axis]
                .checked_add(delta[axis])
                .ok_or(GeometryError::Overflow)?,
            offset[axis]
                .checked_add(delta[axis] / 2)
                .ok_or(GeometryError::Overflow)?,
        ])
    };
    let xs = choices(0)?;
    let ys = choices(1)?;
    let mut possible = false;
    for x in xs {
        for y in ys {
            match basis::instant_text_alignment0(point, c, [x, y], dims, 0)? {
                basis::Decision::Rejected => {}
                basis::Decision::Possible => possible = true,
                basis::Decision::Unsupported(u) => {
                    return Ok(Decision::Unsupported(Unsupported::Basis(u)))
                }
            }
        }
    }
    Ok(if possible {
        Decision::Possible
    } else {
        Decision::Rejected
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    fn c() -> Common {
        Common {
            canvas: [1920, 1080],
            translation: [0.0, 0.0],
            inherited_scale: 1.0,
            rotation: 0.0,
        }
    }
    #[test]
    fn clip_inherit_intersect_clear_and_inclusive_edges() {
        let p = Some([10, 20, 100, 200]);
        assert_eq!(
            clips(p, 0, [0; 4], false, true),
            Clips {
                children: p,
                self_clip: p
            }
        );
        let i = clips(p, 1, [0, 30, 90, 220], false, true);
        assert_eq!(i.children, Some([10, 30, 90, 200]));
        assert_eq!(i.self_clip, i.children);
        assert_eq!(
            clips(p, 2, [30, 40, 50, 60], true, true),
            Clips {
                children: None,
                self_clip: Some([30, 40, 50, 60])
            }
        );
        assert!(clip_contains(i.self_clip, [10.0, 30.0]));
        assert!(clip_contains(i.self_clip, [90.0, 200.0]));
        assert!(!clip_contains(i.self_clip, [9.0, 50.0]));
    }
    #[test]
    fn no_inherited_clip_never_invents_self145_rewrite() {
        assert_eq!(
            clips(None, 0, [1, 2, 3, 4], true, true),
            Clips {
                children: None,
                self_clip: Some([1, 2, 3, 4])
            }
        );
        assert_eq!(
            clips(None, 1, [1, 2, 3, 4], false, true),
            Clips {
                children: Some([1, 2, 3, 4]),
                self_clip: None
            }
        );
        assert_eq!(
            clips(None, 1, [1, 2, 3, 4], true, true).self_clip,
            Some([1, 2, 3, 4])
        );
    }
    #[test]
    fn self_inactive_keeps_old_clip_but_forwards_inherited_for_children() {
        let v = clips(Some([10, 10, 20, 20]), 0, [0, 0, 1, 1], true, false);
        assert_eq!(v.self_clip, Some([0, 0, 1, 1]));
        assert_eq!(v.children, Some([10, 10, 20, 20]));
    }
    #[test]
    fn clip_miss_does_not_prune_a_child_that_clears_clip() {
        let p = clips(None, 1, [0, 0, 10, 10], true, true);
        assert!(!clip_contains(p.self_clip, [960.0, 540.0]));
        let ch = clips(p.children, 2, [0; 4], false, true);
        assert!(clip_contains(ch.self_clip, [960.0, 540.0]));
    }
    #[test]
    fn empty_intersection_rejects_every_point_without_removing_children() {
        let v = clips(Some([0, 0, 10, 10]), 1, [20, 20, 30, 30], false, true);
        assert_eq!(v.children, Some([20, 20, 10, 10]));
        assert!(!clip_contains(v.self_clip, [15.0, 15.0]));
    }
    #[test]
    fn unit_transform_adds_original_ancestors_not_cached_values() {
        let p = transform(
            Transform::SEED,
            [20.0, 30.0],
            1.0,
            false,
            false,
            true,
            [1920, 1080],
        )
        .unwrap()
        .unwrap();
        let ch = transform(p, [-5.0, 7.0], 0.8, true, true, false, [1920, 1080])
            .unwrap()
            .unwrap();
        assert_eq!(ch.xy, [15.0, 37.0]);
        assert_eq!(ch.scale, 1.0); // self own scale belongs to class size, not child accumulation
        assert!(ch.unit_path);
    }
    #[test]
    fn group_scale_does_not_change_its_own_incoming_correction() {
        let v = transform(
            Transform::SEED,
            [10.0, 20.0],
            0.5,
            false,
            false,
            true,
            [1920, 1080],
        )
        .unwrap()
        .unwrap();
        assert_eq!(v.xy, [10.0, 20.0]);
        assert_eq!(v.scale, 0.5);
        let ch = transform(v, [100.0, 200.0], 1.0, false, true, false, [1920, 1080])
            .unwrap()
            .unwrap();
        assert_eq!(ch.xy, [-420.0, 390.0]);
        assert!(!ch.unit_path);
        let a = transform(v, [100.0, 200.0], 1.0, true, true, false, [1920, 1080])
            .unwrap()
            .unwrap();
        assert_eq!(a.xy, [60.0, 120.0]);
    }
    #[test]
    fn exact_epsilon_is_strict_and_inactive_node_does_not_mutate() {
        assert_eq!(EPSILON.to_bits(), 0x3a83126f);
        let same = transform(
            Transform::SEED,
            [0.0, 0.0],
            EPSILON,
            false,
            false,
            true,
            [1920, 1080],
        )
        .unwrap()
        .unwrap();
        assert_eq!(same.scale, 1.0);
        let changed = transform(
            Transform::SEED,
            [0.0, 0.0],
            f32::from_bits(EPSILON.to_bits() + 1),
            false,
            false,
            true,
            [1920, 1080],
        )
        .unwrap()
        .unwrap();
        assert_ne!(changed.scale, 1.0);
        assert_eq!(
            transform(
                Transform::SEED,
                [f32::NAN, 0.0],
                0.0,
                false,
                false,
                false,
                [0, 0]
            ),
            Ok(None)
        );
    }
    #[test]
    fn union_keeps_a_possible_shifted_box_and_rejects_far_point() {
        assert_eq!(
            instant_union([35.0, 5.0], c(), [0, 0], [30, 0], [10, 10], 1),
            Ok(Decision::Possible)
        );
        assert_eq!(
            basis::instant_text_alignment0([35.0, 5.0], c(), [0, 0], [10, 10], 0),
            Ok(basis::Decision::Rejected)
        );
        assert_eq!(
            instant_union([100.0, 100.0], c(), [0, 0], [30, 30], [10, 10], 1),
            Ok(Decision::Rejected)
        );
    }
    #[test]
    fn union_negative_odd_half_uses_truncate_toward_zero_and_strict_edges() {
        // -3/2=-1: centered choice is off10-1=9, whose interior contains10.
        assert_eq!(
            instant_union([10.0, 10.0], c(), [10, 10], [-3, -3], [2, 2], 6),
            Ok(Decision::Possible)
        );
        // Offset0/5/10 boxes of width5 have all their integer edges excluded.
        assert_eq!(
            instant_union([0.0, 0.0], c(), [0, 0], [10, 10], [5, 5], 1),
            Ok(Decision::Rejected)
        );
    }
    #[test]
    fn union_overflow_and_unsupported_are_never_misses() {
        assert_eq!(
            instant_union([1.0, 1.0], c(), [i32::MAX, 0], [1, 0], [1, 1], 1),
            Err(GeometryError::Overflow)
        );
        assert_eq!(
            instant_union(
                [1.0, 1.0],
                Common {
                    inherited_scale: 0.8,
                    ..c()
                },
                [0, 0],
                [0, 0],
                [1, 1],
                1
            ),
            Ok(Decision::Unsupported(Unsupported::Basis(
                basis::Unsupported::InheritedScale
            )))
        );
        assert_eq!(
            instant_union([1.0, 1.0], c(), [0, 0], [0, 0], [1, 1], 7),
            Ok(Decision::Unsupported(Unsupported::Basis(
                basis::Unsupported::TextAlignment
            )))
        );
        // An early Possible must not hide an uncomputable later alternative.
        assert_eq!(
            instant_union([1.0, 1.0], c(), [0, 0], [i32::MAX, 0], [10, 10], 1),
            Err(GeometryError::Overflow)
        );
    }
}
