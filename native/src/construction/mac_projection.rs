//! Passive ray arithmetic for Mach-O 089EA519. The scalar f32 grouping follows
//! 0x102762460/0x102762150 and normalization body 0x102ac6970. The child module
//! supplies one-projection terrain arithmetic. Neither supplies memory reads,
//! input, frame freshness or placement authority.
use crate::error::{Error, Result};

pub(crate) mod terrain;

// Observer policy, not an asserted engine limit. Each integer is exact in f32.
const MAX_CANVAS: i32 = 32_768;

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Ray {
    origin: [f32; 3],
    direction: [f32; 3],
}

impl Ray {
    pub(crate) fn origin(&self) -> [f32; 3] {
        self.origin
    }

    pub(crate) fn direction(&self) -> [f32; 3] {
        self.direction
    }
}

fn unavailable(message: &'static str) -> Error {
    Error::new("construction_projection_unavailable", message)
}

fn finite(value: f32) -> Result<f32> {
    if value.is_finite() {
        Ok(value)
    } else {
        Err(unavailable("Finite ray arithmetic required"))
    }
}

fn add(a: f32, b: f32) -> Result<f32> {
    finite(a + b)
}

fn sub(a: f32, b: f32) -> Result<f32> {
    finite(a - b)
}

fn mul(a: f32, b: f32) -> Result<f32> {
    finite(a * b)
}

fn div(a: f32, b: f32) -> Result<f32> {
    finite(a / b)
}

/// Input coordinates already belong to this canvas. Matrices are the complete
/// column-major blocks at projection-base +0x100 and +0xc0; origin is +0x180.
/// Validation never substitutes another point or repairs an unsupported ray.
pub(crate) fn ray(
    point: [f32; 2],
    canvas: [i32; 2],
    projection: &[f32; 16],
    affine: &[f32; 16],
    origin: [f32; 3],
) -> Result<Ray> {
    if canvas.iter().any(|v| !(1..=MAX_CANVAS).contains(v)) {
        return Err(unavailable("Bounded positive ray canvas required"));
    }
    if point
        .iter()
        .zip(canvas)
        .any(|(p, bound)| !p.is_finite() || !(0.0..bound as f32).contains(p))
    {
        return Err(unavailable(
            "Ray point must be finite and inside its canvas before truncation",
        ));
    }
    if projection
        .iter()
        .chain(affine)
        .chain(origin.iter())
        .any(|v| !v.is_finite())
    {
        return Err(unavailable("Finite ray matrices and origin required"));
    }

    // cvttps2dq followed by cvtdq2ps, then the separate SSE multiply/divide.
    let pixel = point.map(|p| (p as i32) as f32);
    let nx = sub(div(add(pixel[0], pixel[0])?, canvas[0] as f32)?, 1.0)?;
    let ny = sub(1.0, div(add(pixel[1], pixel[1])?, canvas[1] as f32)?)?;

    let mut q = [0.0; 4];
    for (row, value) in q.iter_mut().enumerate() {
        let x = mul(projection[row], nx)?;
        let y = mul(projection[4 + row], ny)?;
        let xy = add(x, y)?;
        let z = mul(projection[8 + row], 0.0)?;
        *value = add(add(z, xy)?, projection[12 + row])?;
    }
    if q[3] == 0.0 {
        return Err(unavailable("Nonzero ray homogeneous divisor required"));
    }
    let reciprocal = div(1.0, q[3])?;
    let projected = [
        mul(q[0], reciprocal)?,
        mul(q[1], reciprocal)?,
        mul(q[2], reciprocal)?,
    ];
    let mut difference = [0.0; 3];
    for (row, value) in difference.iter_mut().enumerate() {
        let x = mul(affine[row], projected[0])?;
        let y = mul(affine[4 + row], projected[1])?;
        let z = mul(affine[8 + row], projected[2])?;
        let world = add(add(z, add(x, y)?)?, affine[12 + row])?;
        *value = sub(world, origin[row])?;
    }

    // CalcNormalized: sqrt(z*z + (x*x + y*y)), then three ordinary divides.
    // Separate operations are intentional: no mul_add or reassociation.
    let x2 = mul(difference[0], difference[0])?;
    let y2 = mul(difference[1], difference[1])?;
    let z2 = mul(difference[2], difference[2])?;
    let squared = add(z2, add(x2, y2)?)?;
    let length = finite(squared.sqrt())?;
    if length == 0.0 {
        return Err(unavailable("Nondegenerate ray direction required"));
    }
    let direction = [
        div(difference[0], length)?,
        div(difference[1], length)?,
        div(difference[2], length)?,
    ];
    Ok(Ray { origin, direction })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn identity() -> [f32; 16] {
        [
            1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0,
        ]
    }

    fn forward() -> [f32; 16] {
        let mut matrix = identity();
        matrix[14] = 1.0;
        matrix
    }

    fn close(actual: [f32; 3], expected: [f32; 3]) {
        for (actual, expected) in actual.into_iter().zip(expected) {
            assert!((actual - expected).abs() < 1e-6, "{actual} != {expected}");
        }
    }

    fn standard(point: [f32; 2]) -> Result<Ray> {
        ray(point, [1920, 1080], &forward(), &identity(), [0.0; 3])
    }

    #[test]
    fn canvas_center_has_the_calibrated_forward_direction() {
        let r = standard([960.0, 540.0]).unwrap();
        assert_eq!(r.origin(), [0.0; 3]);
        assert_eq!(r.direction(), [0.0, 0.0, 1.0]);
    }

    #[test]
    fn screen_left_and_up_follow_the_recovered_orientation() {
        close(
            standard([0.0, 0.0]).unwrap().direction(),
            [-0.577_350_26, 0.577_350_26, 0.577_350_26],
        );
        let bottom_right = standard([1919.0, 1079.0]).unwrap().direction();
        assert!(bottom_right[0] > 0.0 && bottom_right[1] < 0.0 && bottom_right[2] > 0.0);
    }

    #[test]
    fn fractional_pixels_are_truncated_before_projection() {
        assert_eq!(
            standard([960.999, 540.999]).unwrap(),
            standard([960.0, 540.0]).unwrap()
        );
        let next = standard([961.0, 541.0]).unwrap().direction();
        assert!(next[0] > 0.0 && next[1] < 0.0);
    }

    #[test]
    fn last_fractional_pixel_is_valid_but_exact_upper_edge_is_not() {
        assert_eq!(
            standard([1919.75, 1079.75]).unwrap(),
            standard([1919.0, 1079.0]).unwrap()
        );
        assert!(standard([1920.0, 540.0]).is_err());
        assert!(standard([960.0, 1080.0]).is_err());
    }

    #[test]
    fn origin_and_affine_translation_are_independent_inputs() {
        let mut affine = identity();
        affine[12..15].copy_from_slice(&[10.0, 20.0, 30.0]);
        let origin = [10.0, 20.0, 30.0];
        let r = ray([960.0, 540.0], [1920, 1080], &forward(), &affine, origin).unwrap();
        assert_eq!(r.origin(), origin);
        assert_eq!(r.direction(), [0.0, 0.0, 1.0]);
        close(
            ray(
                [960.0, 540.0],
                [1920, 1080],
                &forward(),
                &identity(),
                [1.0, 2.0, 0.0],
            )
            .unwrap()
            .direction(),
            [-0.408_248_28, -0.816_496_55, 0.408_248_28],
        );
    }

    #[test]
    fn column_major_scale_rotation_and_translation_compose_in_order() {
        let mut projection = forward();
        projection[0] = 2.0;
        projection[5] = 4.0;
        projection[12] = 1.0;
        projection[13] = 1.0;
        let affine = [
            0.0, 1.0, 0.0, 0.0, -1.0, 0.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 5.0, -2.0, 0.0, 1.0,
        ];
        close(
            ray([75.0, 25.0], [100, 100], &projection, &affine, [0.0; 3])
                .unwrap()
                .direction(),
            [0.894_427_2, 0.0, 0.447_213_6],
        );
    }

    #[test]
    fn homogeneous_divide_precedes_the_affine_and_origin_difference() {
        let mut projection = forward();
        projection[15] = 2.0;
        assert_eq!(
            ray(
                [75.0, 25.0],
                [100, 100],
                &projection,
                &identity(),
                [0.25, 0.25, 0.0]
            )
            .unwrap()
            .direction(),
            [0.0, 0.0, 1.0]
        );
        projection[15] = -1.0;
        assert_eq!(
            ray([50.0, 50.0], [100, 100], &projection, &identity(), [0.0; 3])
                .unwrap()
                .direction(),
            [0.0, 0.0, -1.0]
        );
    }

    #[test]
    fn projection_product_rounding_is_not_fused() {
        let mut projection = forward();
        projection[0] = f32::from_bits(0x3f80_0001);
        projection[4] = f32::from_bits(0x3eaa_aaab);
        projection[5] = 0.0;
        let r = ray([1.0, 0.0], [3, 3], &projection, &identity(), [0.0; 3]).unwrap();
        assert_eq!(r.direction(), [0.0, 0.0, 1.0]);
        // These synthetic SSE-rounding operands leave a nonzero fused residue.
        assert_ne!(
            f32::from_bits(0xbeaa_aaaa).mul_add(projection[0], projection[4]),
            0.0
        );
    }

    #[test]
    fn projection_addition_does_not_reassociate_cancellation() {
        let mut projection = forward();
        projection[0] = -1e20;
        projection[4] = 3.0;
        projection[5] = 0.0;
        projection[12] = -1e20;
        assert_eq!(
            ray([0.0, 0.0], [100, 100], &projection, &identity(), [0.0; 3])
                .unwrap()
                .direction(),
            [0.0, 0.0, 1.0]
        );
    }

    #[test]
    fn affine_addition_does_not_reassociate_cancellation() {
        let mut projection = forward();
        projection[0] = -1.0;
        let mut affine = identity();
        affine[0] = 1e20;
        affine[4] = 3.0;
        affine[5] = 0.0;
        affine[12] = -1e20;
        assert_eq!(
            ray([0.0, 0.0], [100, 100], &projection, &affine, [0.0; 3])
                .unwrap()
                .direction(),
            [0.0, 0.0, 1.0]
        );
    }

    #[test]
    fn normalization_keeps_the_shipped_squared_sum_grouping() {
        let mut projection = forward();
        projection[12] = 1.0;
        projection[13] = f32::from_bits(0x3980_0000);
        projection[14] = f32::from_bits(0x39b5_04f3);
        let actual = ray([50.0, 50.0], [100, 100], &projection, &identity(), [0.0; 3])
            .unwrap()
            .direction();
        assert_eq!(
            actual.map(f32::to_bits),
            [0x3f80_0000, 0x3980_0000, 0x39b5_04f3]
        );
    }

    #[test]
    fn invalid_canvases_refuse() {
        for canvas in [
            [0, 1080],
            [1920, 0],
            [-1, 1080],
            [1920, -1],
            [32_769, 1080],
            [1, i32::MAX],
        ] {
            assert!(ray([0.0; 2], canvas, &forward(), &identity(), [0.0; 3]).is_err());
        }
        assert!(ray([0.0; 2], [32_768; 2], &forward(), &identity(), [0.0; 3]).is_ok());
    }

    #[test]
    fn point_validation_precedes_integer_conversion() {
        for point in [
            [-0.25, 1.0],
            [1.0, -0.25],
            [f32::NAN, 1.0],
            [1.0, f32::INFINITY],
            [f32::NEG_INFINITY, 1.0],
            [f32::MAX, 1.0],
        ] {
            let e = standard(point).unwrap_err();
            assert_eq!(e.code, "construction_projection_unavailable");
            assert!(e.message.contains("before truncation"));
        }
    }

    #[test]
    fn every_matrix_and_origin_field_must_be_finite() {
        for index in 0..16 {
            for invalid in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
                let mut projection = forward();
                projection[index] = invalid;
                assert!(ray([50.0; 2], [100; 2], &projection, &identity(), [0.0; 3]).is_err());
                let mut affine = identity();
                affine[index] = invalid;
                assert!(ray([50.0; 2], [100; 2], &forward(), &affine, [0.0; 3]).is_err());
            }
        }
        for index in 0..3 {
            let mut origin = [0.0; 3];
            origin[index] = f32::NAN;
            assert!(ray([50.0; 2], [100; 2], &forward(), &identity(), origin).is_err());
        }
    }

    #[test]
    fn zero_homogeneous_divisor_and_overflowing_reciprocal_refuse() {
        for denominator in [0.0, -0.0, f32::from_bits(1)] {
            let mut projection = forward();
            projection[15] = denominator;
            assert!(ray([50.0; 2], [100; 2], &projection, &identity(), [0.0; 3]).is_err());
        }
    }

    #[test]
    fn overflowing_projection_or_affine_intermediate_refuses() {
        let mut projection = forward();
        projection[0] = -f32::MAX;
        projection[4] = f32::MAX;
        assert!(ray([0.0; 2], [100; 2], &projection, &identity(), [0.0; 3]).is_err());
        let mut projection = forward();
        projection[12] = 2.0;
        let mut affine = identity();
        affine[0] = f32::MAX;
        assert!(ray([50.0; 2], [100; 2], &projection, &affine, [0.0; 3]).is_err());
    }

    #[test]
    fn zero_or_underflowed_direction_never_becomes_a_fallback() {
        assert!(ray([50.0; 2], [100; 2], &identity(), &identity(), [0.0; 3]).is_err());
        let mut projection = forward();
        projection[14] = f32::MIN_POSITIVE;
        assert!(ray([50.0; 2], [100; 2], &projection, &identity(), [0.0; 3]).is_err());
    }

    #[test]
    fn overflowing_origin_difference_and_squared_length_refuse() {
        let mut projection = forward();
        projection[12] = f32::MAX;
        assert!(ray(
            [50.0; 2],
            [100; 2],
            &projection,
            &identity(),
            [-f32::MAX, 0.0, 0.0]
        )
        .is_err());
        projection[12] = 1e20;
        assert!(ray([50.0; 2], [100; 2], &projection, &identity(), [0.0; 3]).is_err());
    }
}
