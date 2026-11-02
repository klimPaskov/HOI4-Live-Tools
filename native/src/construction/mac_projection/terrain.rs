//! Passive terrain traversal for Mach-O 089EA519: 0x1019302e0 and
//! 0x102a9bc00/0x102a9bd80, with height sampling from 0x1007c5240.
//! One projection only; a miss grants no plane fallback or picking authority.
use crate::error::{Error, Result};
// Observer budgets, not asserted engine limits. The 256-entry candidate cap
// below follows the recovered engine buffer and preserves its traversal order.
const MAX_NODES: usize = 16_384;
const MAX_SAMPLES: usize = 4_096;
const MAX_CANDIDATES: usize = 256;
#[derive(Debug)]
pub(crate) struct Hit {
    pub(crate) point: [f32; 3],
    pub(crate) candidates: usize,
    pub(crate) samples: usize,
    pub(crate) nodes: usize,
}
fn fail(message: &'static str) -> Error {
    Error::new("construction_terrain_unavailable", message)
}
fn finite(v: f32) -> Result<f32> {
    if v.is_finite() {
        Ok(v)
    } else {
        Err(fail("Nonfinite terrain arithmetic"))
    }
}
fn not_nan(v: f32) -> Result<f32> {
    if !v.is_nan() {
        Ok(v)
    } else {
        Err(fail("Indeterminate parallel slab boundary"))
    }
}
fn sse_min(a: f32, b: f32) -> f32 {
    if a < b {
        a
    } else {
        b
    }
}
fn sse_max(a: f32, b: f32) -> f32 {
    if a > b {
        a
    } else {
        b
    }
}
fn length(v: [f32; 3]) -> Result<f32> {
    let x = finite(v[0] * v[0])?;
    let y = finite(v[1] * v[1])?;
    let z = finite(v[2] * v[2])?;
    finite(finite(z + finite(x + y)?)?.sqrt())
}
fn slab(origin: [f32; 3], direction: [f32; 3], lo: [f32; 3], hi: [f32; 3]) -> Result<bool> {
    let mut near = [0.; 3];
    let mut far = [0.; 3];
    for i in 0..3 {
        let reciprocal = not_nan(1.0 / direction[i])?;
        let (n, f) = if reciprocal < 0.0 {
            (hi[i], lo[i])
        } else {
            (lo[i], hi[i])
        };
        near[i] = not_nan((n - origin[i]) * reciprocal)?;
        far[i] = not_nan((f - origin[i]) * reciprocal)?;
    }
    if near[0] > far[1] || near[1] > far[0] {
        return Ok(false);
    }
    let near_xy = sse_max(near[1], near[0]);
    let far_xy = sse_min(far[1], far[0]);
    if near_xy > far[2] || near[2] > far_xy {
        return Ok(false);
    }
    Ok(sse_max(near[2], near_xy) < f32::MAX && sse_min(far[2], far_xy) > 0.0)
}
pub(crate) fn height_index(point: [f32; 2], dimensions: [i32; 2]) -> Result<[usize; 2]> {
    let mut out = [0; 2];
    for i in 0..2 {
        if !(1..=32768).contains(&dimensions[i])
            || !point[i].is_finite()
            || point[i].abs() > 1_000_000.0
        {
            return Err(fail("Bounded height coordinates required"));
        }
        out[i] = (finite(point[i] + 0.5)? as i32).clamp(0, dimensions[i] - 1) as usize;
    }
    Ok(out)
}
struct Search<'a, F> {
    origin: [f32; 3],
    direction: [f32; 3],
    height_dimensions: [i32; 2],
    sample: &'a mut F,
    points: Vec<[f32; 3]>,
    nodes: usize,
    samples: usize,
}
impl<F: FnMut([usize; 2]) -> Result<u8>> Search<'_, F> {
    fn visit(&mut self, x: f32, z: f32, width: f32, height: f32) -> Result<()> {
        self.nodes += 1;
        if self.nodes > MAX_NODES {
            return Err(fail("Terrain traversal exceeds observer budget"));
        }
        let leaf = width < 2.0 && height < 2.0;
        if leaf {
            if self.points.len() == MAX_CANDIDATES {
                return Ok(());
            }
            self.samples += 1;
            if self.samples > MAX_SAMPLES {
                return Err(fail("Terrain sampling exceeds observer budget"));
            }
            let index = height_index([x, z], self.height_dimensions)?;
            let h = ((*self.sample)(index)? as f32 * 0.1_f32).max(9.5_f32);
            let end_x = finite(x + width)?;
            let end_z = finite(z + height)?;
            if !slab(self.origin, self.direction, [x, 0., z], [end_x, h, end_z])? {
                return Ok(());
            }
            let delta = [
                finite(finite(end_x - 0.5)? - self.origin[0])?,
                finite(h - self.origin[1])?,
                finite(finite(end_z - 0.5)? - self.origin[2])?,
            ];
            let distance = length(delta)?;
            self.points.push([
                finite(finite(self.direction[0] * distance)? + self.origin[0])?,
                h,
                finite(finite(self.direction[2] * distance)? + self.origin[2])?,
            ]);
            return Ok(());
        }
        let w = if width < 2.0 {
            width
        } else {
            finite(width * 0.5)?
        };
        let h = if height < 2.0 {
            height
        } else {
            finite(height * 0.5)?
        };
        let split_x = finite(x + w)?;
        let split_z = finite(z + h)?;
        // Exact engine quadrant order, including repeated narrow-axis cells.
        for (a, b) in [(x, z), (split_x, z), (x, split_z), (split_x, split_z)] {
            if slab(
                self.origin,
                self.direction,
                [a, 0., b],
                [finite(a + w)?, 25.5, finite(b + h)?],
            )? {
                self.visit(a, b, w, h)?;
            }
        }
        Ok(())
    }
}
/// One already-calibrated projection. Each callback supplies only the requested
/// byte from a separately owned, bounded height grid. The caller must witness
/// every sampled byte and all owners. No candidate means no supported hit;
/// this function neither substitutes the engine's plane fallback nor handles
/// alternate projections, scene objects, GUI precedence or eligibility.
pub(crate) fn intersect(
    ray: &super::Ray,
    map_dimensions: [i32; 2],
    height_dimensions: [i32; 2],
    sample: &mut impl FnMut([usize; 2]) -> Result<u8>,
) -> Result<Option<Hit>> {
    let origin = ray.origin();
    let direction = ray.direction();
    if map_dimensions
        .into_iter()
        .chain(height_dimensions)
        .any(|v| !(1..=32768).contains(&v))
        || origin
            .into_iter()
            .chain(direction)
            .any(|v| !v.is_finite() || v.abs() > 1_000_000.)
    {
        return Err(fail("Bounded finite terrain inputs required"));
    }
    let norm = length(direction)?;
    if (norm - 1.0).abs() > 0.001 {
        return Err(fail("Normalized terrain ray required"));
    }
    let mut search = Search {
        origin,
        direction,
        height_dimensions,
        sample,
        points: Vec::new(),
        nodes: 0,
        samples: 0,
    };
    search.visit(0., 0., map_dimensions[0] as f32, map_dimensions[1] as f32)?;
    let mut chosen = None;
    let mut minimum = f32::MAX;
    for point in &search.points {
        let d = length([
            finite(point[0] - origin[0])?,
            finite(point[1] - origin[1])?,
            finite(point[2] - origin[2])?,
        ])?;
        if d < minimum {
            minimum = d;
            chosen = Some(*point);
        }
    }
    Ok(chosen.map(|point| Hit {
        point,
        candidates: search.points.len(),
        samples: search.samples,
        nodes: search.nodes,
    }))
}
#[cfg(test)]
mod tests {
    use super::*;
    // Synthetic rays are confined to this descendant fixture module.
    fn intersect(
        origin: [f32; 3],
        direction: [f32; 3],
        map: [i32; 2],
        height: [i32; 2],
        sample: &mut impl FnMut([usize; 2]) -> Result<u8>,
    ) -> Result<Option<Hit>> {
        super::intersect(
            &super::super::Ray { origin, direction },
            map,
            height,
            sample,
        )
    }
    #[test]
    fn parallel_inside_box_is_supported() {
        assert!(slab([0.5, 20., 0.5], [0., -1., 0.], [0.; 3], [1., 9.5, 1.]).unwrap());
    }
    #[test]
    fn parallel_outside_box_misses() {
        assert!(!slab([1.5, 20., 0.5], [0., -1., 0.], [0.; 3], [1., 9.5, 1.]).unwrap());
    }
    #[test]
    fn indeterminate_parallel_boundary_refuses() {
        assert!(slab([0., 20., 0.5], [0., -1., 0.], [0.; 3], [1., 9.5, 1.]).is_err());
    }
    #[test]
    fn box_behind_ray_misses() {
        assert!(!slab([0.5, 20., 0.5], [0., 1., 0.], [0.; 3], [1., 9.5, 1.]).unwrap());
    }
    #[test]
    fn index_rounds_then_clamps() {
        assert_eq!(height_index([1.49, 1.5], [4, 4]).unwrap(), [1, 2]);
        assert_eq!(height_index([-5., 20.], [4, 4]).unwrap(), [0, 3]);
    }
    #[test]
    fn invalid_index_inputs_refuse() {
        assert!(height_index([f32::NAN, 0.], [4, 4]).is_err());
        assert!(height_index([0.; 2], [0, 4]).is_err());
    }
    #[test]
    fn one_leaf_floors_water_height() {
        let mut calls = vec![];
        let h = intersect([0.5, 20., 0.5], [0., -1., 0.], [1, 1], [1, 1], &mut |p| {
            calls.push(p);
            Ok(0)
        })
        .unwrap()
        .unwrap();
        assert_eq!(calls, [[0, 0]]);
        assert_eq!(h.point, [0.5, 9.5, 0.5]);
        assert_eq!((h.nodes, h.samples, h.candidates), (1, 1, 1));
    }
    #[test]
    fn one_leaf_preserves_high_height() {
        let h = intersect([0.5, 30., 0.5], [0., -1., 0.], [1, 1], [1, 1], &mut |_| {
            Ok(200)
        })
        .unwrap()
        .unwrap();
        assert_eq!(h.point, [0.5, 20., 0.5]);
    }
    #[test]
    fn narrow_axis_does_not_halve_leaf_width() {
        let mut seen = vec![];
        let h = intersect([0.25, 20., 0.25], [0., -1., 0.], [1, 2], [1, 2], &mut |p| {
            seen.push(p);
            Ok(0)
        })
        .unwrap()
        .unwrap();
        assert_eq!(seen, [[0, 0]]);
        assert_eq!(h.nodes, 2);
    }
    #[test]
    fn unavailable_height_never_substitutes_plane() {
        assert!(intersect(
            [0.5, 20., 0.5],
            [0., -1., 0.],
            [1, 1],
            [1, 1],
            &mut |_| Err(fail("sample unavailable"))
        )
        .is_err());
    }
    #[test]
    fn oblique_leaf_uses_representative_distance() {
        let h = intersect(
            [0.25, 20., 0.25],
            [0., -0.8, 0.6],
            [1, 1],
            [1, 1],
            &mut |_| Ok(0),
        )
        .unwrap();
        assert!(h.is_none());
    }
    #[test]
    fn zero_direction_refuses() {
        assert!(intersect([0.5, 20., 0.5], [0.; 3], [1, 1], [1, 1], &mut |_| Ok(0)).is_err());
    }
    #[test]
    fn representative_hit_is_not_a_plane_intersection() {
        let h = intersect(
            [0.5, 20., 0.5],
            [0., -0.8, 0.6],
            [1, 32],
            [1, 32],
            &mut |_| Ok(0),
        )
        .unwrap()
        .unwrap();
        let d = (10.5_f32 * 10.5 + 8.0_f32 * 8.0).sqrt();
        assert_eq!(h.point, [0.5, 9.5, 0.5 + 0.6 * d]);
        assert_ne!(h.point[2], 8.375);
    }
    #[test]
    fn exhausted_candidate_capacity_never_reads_another_leaf() {
        let mut sample = |_| -> Result<u8> { panic!("sample after engine cap") };
        let mut s = Search {
            origin: [0.5, 20., 0.5],
            direction: [0., -1., 0.],
            height_dimensions: [1, 1],
            sample: &mut sample,
            points: vec![[0.; 3]; 256],
            nodes: 0,
            samples: 0,
        };
        s.visit(0., 0., 1., 1.).unwrap();
        assert_eq!(s.samples, 0);
    }
    #[test]
    fn traversal_budget_exhaustion_is_an_error() {
        let mut sample = |_| Ok(0);
        let mut s = Search {
            origin: [0.5, 20., 0.5],
            direction: [0., -1., 0.],
            height_dimensions: [1, 1],
            sample: &mut sample,
            points: vec![],
            nodes: MAX_NODES,
            samples: 0,
        };
        assert!(s.visit(0., 0., 1., 1.).is_err());
    }
}
