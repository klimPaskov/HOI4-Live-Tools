//! Conditional ordinary research point candidates, never authoritative hit
//! rectangles/current-frame/topmost proof. No process reads or input here.
use crate::error::{Error, Result};
use serde::Serialize;

#[derive(Clone, Debug)]
pub struct Hierarchy {
    pub closed_to_owned_root_1f0: bool,
    pub conditional_unit_cache_consistent: bool,
}
#[derive(Clone, Debug)]
pub struct Common {
    pub world: [f32; 2],
    pub anchor: [u32; 2],
    pub scalar: f32,
    pub hierarchy: Hierarchy,
}
#[derive(Clone, Debug)]
pub struct SpriteNode {
    pub common: Common,
    pub texture: [i32; 2],
    pub grid: [i32; 2],
    /// Full DWORD +10c. A byte sample is not admissible orientation evidence.
    pub orientation: u32,
    pub orientation_full_width: bool,
    pub cache_equivalent: bool,
    pub no_mask_rejection_branch: bool,
    pub physical_clip: Option<[i32; 4]>,
}
#[derive(Clone, Debug)]
pub struct TextResearch {
    pub common: Common,
    pub width: i32,
    pub height: i32,
    pub resource_count: u32,
    pub hit_divisor: i32,
    pub angle: f32,
}
#[derive(Clone, Debug)]
pub struct InstantText {
    /// Common anchor is unused: this renderer has its own alignment operands.
    pub common: Common,
    pub position: [i32; 2],
    pub alignment_offset: [i32; 2],
    pub dimensions: [i32; 2],
    pub alignment: u32,
    pub hit_rejected: bool,
}
#[derive(Clone, Debug)]
pub enum Surface {
    ExactSpriteNode(SpriteNode),
    /// Exact Frame renderer/resource uses independently proved grid bounds and
    /// the bounded common-origin projection. This is not a Sprite cast.
    ExactFrameSprite(SpriteNode),
    ExactZeroAngleTextResearch(TextResearch),
    ExactInstantText(InstantText),
    Unsupported,
}
#[derive(Clone, Debug)]
pub struct ClientCodec {
    pub owned_x11_window: u32,
    pub observed_sdl_window: u64,
    pub owned_os_binding_verified: bool,
    pub builtin_point_source_verified: bool,
    pub reciprocal_x11_association_verified: bool,
    pub relative_raw: [i32; 2],
    pub client: [u16; 2],
    pub graphics_physical: [u16; 2],
    pub graphics_logical: [u16; 2],
    pub inverse: [f32; 2],
    pub forward: [f32; 2],
    /// Explicit owner-reviewed profile corroboration; diagnostics alone cannot
    /// set this from their input_mapping_verified/client_origin false fields.
    pub reviewed_builtin_client_origin: bool,
}
#[derive(Clone, Debug, Serialize, PartialEq)]
pub struct PointCandidate {
    pub ideal_logical: [i32; 2],
    pub quantized_logical: [i32; 2],
    pub client_pixel: [i32; 2],
    pub expected_client: [u16; 2],
    pub current_frame_verified: bool,
    pub effective_routing_verified: bool,
    pub input_authorized: bool,
}
fn unavailable(reason: &str) -> Error {
    Error::new("research_geometry_unavailable", reason)
}
fn integer(value: f32) -> Result<i32> {
    if !value.is_finite() || value.abs() > 1_000_000.0 {
        return Err(unavailable("bounded finite float32 conversion required"));
    }
    Ok(value.trunc() as i32)
}
fn origin(common: &Common, height: i32) -> Result<[i32; 2]> {
    validate_common(common)?;
    // Keep the recovered f32 addition/subtraction order, including the sign
    // flip and dimension height. Cache b0/b4 is NOT the upper-left corner.
    Ok([
        integer(common.world[0] + common.anchor[0] as f32 * 0.5)?,
        integer(-((common.world[1] - common.anchor[1] as f32 * 0.5) + height as f32))?,
    ])
}
fn validate_common(common: &Common) -> Result<()> {
    if common.scalar.to_bits() != 1.0f32.to_bits()
        || !common.hierarchy.closed_to_owned_root_1f0
        || !common.hierarchy.conditional_unit_cache_consistent
        || common.anchor.iter().any(|n| *n > 65536)
        || common
            .world
            .iter()
            .any(|n| !n.is_finite() || n.abs() > 1_000_000.0)
    {
        return Err(unavailable("typed owned unit hierarchy/cache unavailable"));
    }
    Ok(())
}
enum Predicate {
    Sprite { origin: [i32; 2], extent: [f32; 2] },
    Text { origin: [i32; 2], size: [i32; 2] },
    InstantText { origin: [i32; 2], size: [i32; 2] },
}
impl Predicate {
    fn center(&self) -> Result<[i32; 2]> {
        let (origin, extent) = match self {
            Self::Sprite { origin, extent } => (*origin, *extent),
            Self::Text { origin, size } | Self::InstantText { origin, size } => {
                (*origin, size.map(|n| n as f32))
            }
        };
        Ok([
            origin[0]
                .checked_add(integer((extent[0] * 0.5).floor())?)
                .ok_or_else(|| unavailable("center overflow"))?,
            origin[1]
                .checked_add(integer((extent[1] * 0.5).floor())?)
                .ok_or_else(|| unavailable("center overflow"))?,
        ])
    }
    fn interior(&self, point: [i32; 2]) -> bool {
        match self {
            Self::Sprite { origin, extent } => (0..2).all(|axis| {
                let delta = point[axis] as f32 - origin[axis] as f32;
                delta >= 1.0 && delta <= extent[axis] - 1.0
            }),
            Self::Text { origin, size } => {
                // Exact zero-angle float/int predicate, not an algebra-only
                // assumption about cached text dimensions.
                let dx = (point[0] as f32 - origin[0] as f32).trunc() as i32;
                let baseline = origin[1] + size[1];
                let dy = point[1] as f32 - baseline as f32;
                dx > 0
                    && dx < size[0]
                    && dy < 0.0
                    && origin[1] < baseline + dy.trunc() as i32
                    && point[1] < baseline
            }
            Self::InstantText { origin, size } => (0..2)
                .all(|axis| point[axis] > origin[axis] && point[axis] < origin[axis] + size[axis]),
        }
    }
}
fn validate_codec(c: &ClientCodec) -> Result<()> {
    if c.owned_x11_window == 0
        || c.observed_sdl_window != u64::from(c.owned_x11_window)
        || !c.owned_os_binding_verified
        || !c.builtin_point_source_verified
        || !c.reciprocal_x11_association_verified
        || c.relative_raw != [0, 0]
        || !c.reviewed_builtin_client_origin
        || c.client != c.graphics_physical
        || c.client
            .iter()
            .chain(c.graphics_logical.iter())
            .any(|n| !(1..=16384).contains(n))
    {
        return Err(unavailable(
            "owned SDL/X11 absolute client profile unavailable",
        ));
    }
    for axis in 0..2 {
        let inverse = f32::from(c.graphics_logical[axis]) / f32::from(c.client[axis]);
        let forward = f32::from(c.client[axis]) / f32::from(c.graphics_logical[axis]);
        if !c.inverse[axis].is_finite()
            || !c.forward[axis].is_finite()
            || c.inverse[axis] <= 0.0
            || c.forward[axis] <= 0.0
            || (c.inverse[axis] / inverse - 1.0).abs() > 0.0001
            || (c.forward[axis] / forward - 1.0).abs() > 0.0001
        {
            return Err(unavailable("fresh graphics ratios inconsistent"));
        }
    }
    Ok(())
}
/// Pure bounded candidate math. Typed constructors are filled only from fresh
/// owner-checked observer outputs by the future owner integration.
pub fn point_candidate(surface: &Surface, codec: &ClientCodec) -> Result<PointCandidate> {
    validate_codec(codec)?;
    let (predicate, clip) = match surface {
        Surface::Unsupported => return Err(unavailable("unsupported renderer/resource branch")),
        Surface::ExactSpriteNode(s) | Surface::ExactFrameSprite(s) => {
            if !s.orientation_full_width
                || !matches!(s.orientation, 0 | 1)
                || !s.cache_equivalent
                || !s.no_mask_rejection_branch
                || s.texture.iter().any(|v| !(1..=65536).contains(v))
                || s.grid.iter().any(|v| !(1..=4096).contains(v))
            {
                return Err(unavailable(
                    "exact Sprite resource/cache/mask/orientation unavailable",
                ));
            }
            let divisor = if s.orientation == 1 {
                [s.grid[1], s.grid[0]]
            } else {
                s.grid
            };
            let mut extent = [0.0; 2];
            let mut dimensions = [0; 2];
            for axis in 0..2 {
                let reciprocal = 1.0f32 / divisor[axis] as f32;
                let first = s.texture[axis] as f32 * reciprocal;
                dimensions[axis] = integer(first.trunc() * s.common.scalar)?;
                extent[axis] = (s.texture[axis] as f32 * s.common.scalar) * reciprocal;
                if dimensions[axis] < 2 || !extent[axis].is_finite() || extent[axis] < 2.0 {
                    return Err(unavailable("Sprite has no robust interior"));
                }
            }
            (
                Predicate::Sprite {
                    origin: origin(&s.common, dimensions[1])?,
                    extent,
                },
                s.physical_clip,
            )
        }
        Surface::ExactZeroAngleTextResearch(t) => {
            if !t.angle.is_finite()
                || t.angle != 0.0
                || !(1..=4096).contains(&t.resource_count)
                || t.hit_divisor != t.resource_count as i32
                || !(2..=65536).contains(&t.width)
                || !(2..=65536).contains(&t.height)
            {
                return Err(unavailable(
                    "zero-angle Text count/divisor/dimensions unavailable",
                ));
            }
            let width = t.width / t.hit_divisor;
            if width < 2 {
                return Err(unavailable("Text has no robust interior"));
            }
            (
                Predicate::Text {
                    origin: origin(&t.common, t.height)?,
                    size: [width, t.height],
                },
                None,
            )
        }
        Surface::ExactInstantText(t) => {
            validate_common(&t.common)?;
            if t.hit_rejected
                || t.alignment > 6
                || t.position
                    .iter()
                    .chain(t.alignment_offset.iter())
                    .any(|v| !(-65536..=65536).contains(v))
                || t.dimensions.iter().any(|v| !(2..=65536).contains(v))
            {
                return Err(unavailable("Bounded enabled InstantText fields required"));
            }
            // Exact2ee8d10: integer alignment before float32 cache addition;
            // truncate each origin before strict integer hit comparisons.
            // This differs from both CTextSprite and its font measurement.
            let [dx, dy] = t.alignment_offset;
            let offset = match t.alignment {
                0 => [0, 0],
                1 => [dx, 0],
                2 => [0, dy],
                3 => [dx, dy],
                4 => [dx / 2, dy / 2],
                5 => [dx / 2, 0],
                6 => [dx / 2, dy],
                _ => unreachable!(),
            };
            let origin = [
                integer((t.position[0] + offset[0]) as f32 + t.common.world[0])?,
                integer((t.position[1] + offset[1]) as f32 - t.common.world[1])?,
            ];
            (
                Predicate::InstantText {
                    origin,
                    size: t.dimensions,
                },
                None,
            )
        }
    };
    let ideal = predicate.center()?;
    let mut pixel = [0; 2];
    let mut logical = [0; 2];
    for axis in 0..2 {
        pixel[axis] = integer((ideal[axis] as f32 * codec.forward[axis]).round())?;
        if pixel[axis] < 0 || pixel[axis] >= i32::from(codec.client[axis]) {
            // Preserve the original refused projection in durable stage reports.
            // These bounded numeric facts are diagnostics, never a clamped click
            // or authority to move the viewport.
            return Err(unavailable(&format!(
                "Point outside client: axis={} ideal={} pixel={} client={}x{}",
                axis, ideal[axis], pixel[axis], codec.client[0], codec.client[1]
            )));
        }
        logical[axis] = integer(pixel[axis] as f32 * codec.inverse[axis])?;
    }
    if !predicate.interior(logical) {
        return Err(unavailable(
            "physical quantization leaves proved local interior",
        ));
    }
    if let Some([left, top, right, bottom]) = clip {
        if left >= right
            || top >= bottom
            || pixel[0] <= left
            || pixel[0] >= right - 1
            || pixel[1] <= top
            || pixel[1] >= bottom - 1
        {
            return Err(unavailable("observed physical clip excludes candidate"));
        }
    }
    Ok(PointCandidate {
        ideal_logical: ideal,
        quantized_logical: logical,
        client_pixel: pixel,
        expected_client: codec.client,
        current_frame_verified: false,
        effective_routing_verified: false,
        input_authorized: false,
    })
}

#[cfg(test)]
mod tests;
