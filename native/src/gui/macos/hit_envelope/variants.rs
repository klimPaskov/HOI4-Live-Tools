//! Pure, restrictive outer-rejection extensions for the exact Mac GUI artifact.
//!
//! This is not a process reader or route predicate. Every supplied class tag,
//! ownership edge, dispatch target and consumed field needs a caller's original
//! binding and final witnesses. `Possible` never means a positive hit; an
//! `Unsupported` or error must never be converted into `Rejected`.

use super as basis;
pub use basis::{Common, GeometryError};
use basis::{MAX_CANVAS, MAX_COORDINATE, MAX_DIMENSION, MAX_LOCAL_SCALE, MAX_TILES};

pub const FRAME_RENDERER_PRIMARY_RVA: u64 = 0x36be530;
pub const FRAME_RESOURCE_PRIMARY_RVA: u64 = 0x36be828;
pub const MASKED_RENDERER_PRIMARY_RVA: u64 = 0x35edb70;
pub const MASKED_RESOURCE_PRIMARY_RVA: u64 = 0x35ec9d8;
pub const PROGRESS_RENDERER_PRIMARY_RVA: u64 = 0x36c9210;
pub const PROGRESS_RESOURCE_PRIMARY_RVA: u64 = 0x36c9130;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Decision {
    Rejected,
    Possible,
    Unsupported(Unsupported),
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Unsupported {
    RendererPrimary,
    ResourcePrimary,
    IntegerPoint,
    NegativeMaskedOrigin,
    TrigBasis,
    Basis(basis::Unsupported),
}
#[derive(Clone, Copy, Debug)]
pub struct FrameRenderer {
    pub primary_rva: u64,
    pub common: Common,
    pub local_scale: f32, // own renderer+114, not inherited scale
}
#[derive(Clone, Copy, Debug)]
pub struct FrameResource {
    pub primary_rva: u64,
    pub dimensions: [i32; 2], // +8c/+90, separately bound size168 and hit1b0
    pub tiles: [i32; 2],      // +78/+7c
    pub orientation: i32,     // +fc: exactly1 swaps tile denominators
    pub loaded_index: i32,    // +188: conservative no-write branch, >=0
}
#[derive(Clone, Copy, Debug)]
pub struct MaskedRenderer {
    pub primary_rva: u64,
    pub common: Common,
    pub local_scale: f32, // affects size/position, not the resource's hit test
}
#[derive(Clone, Copy, Debug)]
pub struct MaskedResource {
    pub primary_rva: u64,
    pub dimensions: [i32; 2], // +210/+214; v90 is pure, without a lazy-load gate
    pub tiles: [i32; 2],      // +78/+7c, consumed only for the size168 resource
    pub orientation: i32,     // +fc, consumed only for the size168 resource
}
#[derive(Clone, Copy, Debug)]
pub struct ProgressRenderer {
    pub primary_rva: u64,
    pub common: Common,
    pub rotation: f32, // renderer+160, independent of the Common restriction
}
#[derive(Clone, Copy, Debug)]
pub struct ProgressResource {
    pub primary_rva: u64,
    pub dimensions: [i32; 2], // +dc/+e0, renderer virtual160's direct size
}
/// This is an explicit arithmetic precondition, not proof of a runtime import.
/// Static analysis resolves the shipped stub symbol, not the current dynamic import
/// cell. A caller lacking its own accepted normal-sincos-zero basis must pass
/// `Unqualified`. Neither variant grants route, event or input authority.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProgressTrig {
    Unqualified,
    NormalSincosZero,
}

fn decision(d: basis::Decision) -> Decision {
    match d {
        basis::Decision::Rejected => Decision::Rejected,
        basis::Decision::Possible => Decision::Possible,
        basis::Decision::Unsupported(u) => Decision::Unsupported(Unsupported::Basis(u)),
    }
}
fn common(point: [f32; 2], c: Common) -> Result<Option<Unsupported>, GeometryError> {
    if c.canvas.iter().any(|v| !(1..=MAX_CANVAS).contains(v)) {
        return Err(GeometryError::Canvas);
    }
    if point.iter().any(|v| !v.is_finite())
        || c.translation.iter().any(|v| !v.is_finite())
        || !c.inherited_scale.is_finite()
        || !c.rotation.is_finite()
    {
        return Err(GeometryError::NonFinite);
    }
    if point[0] < 0.0
        || point[1] < 0.0
        || point[0] >= c.canvas[0] as f32
        || point[1] >= c.canvas[1] as f32
    {
        return Err(GeometryError::Point);
    }
    if c.translation.iter().any(|v| v.abs() > MAX_COORDINATE) {
        return Err(GeometryError::Translation);
    }
    scale(c.inherited_scale)?;
    if c.inherited_scale != 1.0 {
        return Ok(Some(Unsupported::Basis(basis::Unsupported::InheritedScale)));
    }
    if c.rotation != 0.0 {
        return Ok(Some(Unsupported::Basis(basis::Unsupported::Rotation)));
    }
    if point.iter().any(|v| v.trunc() != *v) {
        return Ok(Some(Unsupported::IntegerPoint));
    }
    Ok(None)
}
fn finite(v: f32) -> Result<f32, GeometryError> {
    v.is_finite().then_some(v).ok_or(GeometryError::NonFinite)
}
fn scale(v: f32) -> Result<(), GeometryError> {
    finite(v)?;
    if v <= 0.0 || v > MAX_LOCAL_SCALE {
        return Err(GeometryError::Scale);
    }
    Ok(())
}
fn dimensions(d: [i32; 2]) -> Result<(), GeometryError> {
    if d.iter().any(|v| !(0..=MAX_DIMENSION).contains(v)) {
        return Err(GeometryError::Dimensions);
    }
    Ok(())
}
fn trunc(v: f32) -> Result<i32, GeometryError> {
    finite(v)?;
    if v.abs() > MAX_COORDINATE {
        return Err(GeometryError::Overflow);
    }
    Ok(v.trunc() as i32)
}
fn add(a: i32, b: i32) -> Result<i32, GeometryError> {
    a.checked_add(b).ok_or(GeometryError::Overflow)
}
// Exact scalar grouping of1027ed3e0; no fused/reassociated y expression.
fn position(c: Common, height: i32) -> Result<[i32; 2], GeometryError> {
    let x = finite((c.canvas[0] as f32) * 0.5)?;
    let x = finite(x + c.translation[0])?;
    let y = finite((c.canvas[1] as f32) * 0.5)?;
    let y = finite(c.translation[1] - y)?;
    let y = finite((height as f32) + y)?;
    Ok([trunc(x)?, trunc(-y)?])
}

/// Frame virtual80/150/160 and resource88/90 are exactly the same functions
/// as the frozen050 Sprite calculation. The actual Frame tags are validated
/// BEFORE creating a mathematical normalized record for that implementation.
/// This never reports the observed resource as CSpriteType34bde68.
pub fn frame(
    point: [f32; 2],
    renderer: FrameRenderer,
    size: FrameResource,
    hit: FrameResource,
) -> Result<Decision, GeometryError> {
    if renderer.primary_rva != FRAME_RENDERER_PRIMARY_RVA {
        return Ok(Decision::Unsupported(Unsupported::RendererPrimary));
    }
    if size.primary_rva != FRAME_RESOURCE_PRIMARY_RVA
        || hit.primary_rva != FRAME_RESOURCE_PRIMARY_RVA
    {
        return Ok(Decision::Unsupported(Unsupported::ResourcePrimary));
    }
    if let Some(reason) = common(point, renderer.common)? {
        return Ok(Decision::Unsupported(reason));
    }
    let normalized_math_only = |r: FrameResource| {
        basis::SpriteResource::SpriteType(basis::SpriteType {
            primary_rva: basis::SPRITE_TYPE_PRIMARY_RVA,
            texture: r.dimensions,
            tiles: r.tiles,
            orientation: r.orientation,
            loaded_index: r.loaded_index,
        })
    };
    basis::sprite(
        point,
        renderer.common,
        renderer.local_scale,
        normalized_math_only(size),
        normalized_math_only(hit),
    )
    .map(decision)
}

/// Shield virtual80 uses a Masked size168 record for two-stage per-tile/local
/// scaling, but a separately bound hit1b0 record for strict UNSCALED unsigned
/// bounds. Negative origins are deliberately unsupported; no wrapping math is
/// accepted. Later texture/mask rejection remains unmodeled.
pub fn masked(
    point: [f32; 2],
    renderer: MaskedRenderer,
    size: MaskedResource,
    hit: MaskedResource,
) -> Result<Decision, GeometryError> {
    if renderer.primary_rva != MASKED_RENDERER_PRIMARY_RVA {
        return Ok(Decision::Unsupported(Unsupported::RendererPrimary));
    }
    if size.primary_rva != MASKED_RESOURCE_PRIMARY_RVA
        || hit.primary_rva != MASKED_RESOURCE_PRIMARY_RVA
    {
        return Ok(Decision::Unsupported(Unsupported::ResourcePrimary));
    }
    if let Some(reason) = common(point, renderer.common)? {
        return Ok(Decision::Unsupported(reason));
    }
    scale(renderer.local_scale)?;
    dimensions(size.dimensions)?;
    dimensions(hit.dimensions)?;
    if size.tiles.iter().any(|v| !(1..=MAX_TILES).contains(v)) {
        return Err(GeometryError::TileCount);
    }
    let tiles = if size.orientation == 1 {
        [size.tiles[1], size.tiles[0]]
    } else {
        size.tiles
    };
    let mut dims = [0; 2];
    for axis in 0..2 {
        let reciprocal = finite(1.0_f32 / (tiles[axis] as f32))?;
        let tiled = finite((size.dimensions[axis] as f32) * reciprocal)?;
        let tiled = trunc(tiled)?;
        let scaled = finite((tiled as f32) * renderer.local_scale)?;
        dims[axis] = trunc(scaled)?;
    }
    if dims == [0, 0] {
        return Ok(Decision::Unsupported(Unsupported::Basis(
            basis::Unsupported::ZeroSizeFallback,
        )));
    }
    let origin = position(renderer.common, dims[1])?;
    if origin.iter().any(|v| *v < 0) {
        return Ok(Decision::Unsupported(Unsupported::NegativeMaskedOrigin));
    }
    let end = [
        add(origin[0], hit.dimensions[0])?,
        add(origin[1], hit.dimensions[1])?,
    ];
    let p = [trunc(point[0])?, trunc(point[1])?];
    // Nonnegative, nonwrapping values make these signed comparisons identical
    // to resource1019d3fd0's strict unsigned comparisons. The hit ignores tiles,
    // orientation and the scale argument; those unused hit fields stay unused.
    if p[0] <= origin[0] || p[0] >= end[0] || p[1] <= origin[1] || p[1] >= end[1] {
        return Ok(Decision::Rejected);
    }
    Ok(Decision::Possible)
}

/// Restrictive zero-angle reduction of102833250 -> resource102832c10.
/// The complete body has exactly one pre-outer-test trig call (1028332e1),
/// symbol ___sincosf_stret. With normal sin(±0)=±0/cos(±0)=1, bounded integer
/// point/origin/pivot arithmetic (<2^24) survives subtract/multiply/truncate/add
/// exactly. No trig call is made here. Current import qualification is external.
pub fn progress(
    point: [f32; 2],
    renderer: ProgressRenderer,
    resource: ProgressResource,
    trig: ProgressTrig,
) -> Result<Decision, GeometryError> {
    if renderer.primary_rva != PROGRESS_RENDERER_PRIMARY_RVA {
        return Ok(Decision::Unsupported(Unsupported::RendererPrimary));
    }
    if resource.primary_rva != PROGRESS_RESOURCE_PRIMARY_RVA {
        return Ok(Decision::Unsupported(Unsupported::ResourcePrimary));
    }
    if let Some(reason) = common(point, renderer.common)? {
        return Ok(Decision::Unsupported(reason));
    }
    finite(renderer.rotation)?;
    if renderer.rotation != 0.0 {
        return Ok(Decision::Unsupported(Unsupported::Basis(
            basis::Unsupported::Rotation,
        )));
    }
    if trig != ProgressTrig::NormalSincosZero {
        return Ok(Decision::Unsupported(Unsupported::TrigBasis));
    }
    dimensions(resource.dimensions)?;
    if resource.dimensions == [0, 0] {
        return Ok(Decision::Unsupported(Unsupported::Basis(
            basis::Unsupported::ZeroSizeFallback,
        )));
    }
    let origin = position(renderer.common, resource.dimensions[1])?;
    let right = add(origin[0], resource.dimensions[0])?;
    let bottom = add(origin[1], resource.dimensions[1])?;
    // The assembly adds one before its signed strict comparison. Check that
    // operation too, even though this module's policy bounds already protect it.
    let bottom_exclusive = add(bottom, 1)?;
    let p = [trunc(point[0])?, trunc(point[1])?];
    if p[0] < origin[0] || p[0] >= right || p[1] <= origin[1] || p[1] >= bottom_exclusive {
        return Ok(Decision::Rejected);
    }
    Ok(Decision::Possible)
}

#[cfg(test)]
mod tests;
