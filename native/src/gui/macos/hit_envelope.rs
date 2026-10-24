//! Pure outer-rejection math for a restricted exact Mac GUI hit subset.
//!
//! Inputs must already represent the original typed, witnessed node/resource
//! binding and the effective translation at its virtual80 call. This module
//! reads no memory, invokes no engine functions and grants no route/input proof.
//! `Possible` deliberately includes texture-mask rejection and other no-effects.

// Independently recovered for Mach-O089EA519; see native/PROVENANCE.md.
// Extends the attributed Clausewitz observation approach; no upstream hit code.
pub const SPRITE_TYPE_PRIMARY_RVA: u64 = 0x34bde68;
// Observer policy, not engine constants. Values remain well inside exact i32
// conversion/addition limits; all arithmetic below still checks intermediates.
pub const MAX_CANVAS: i32 = 32_768;
pub const MAX_DIMENSION: i32 = 16_384;
pub const MAX_TILES: i32 = 4_096;
pub const MAX_COORDINATE: f32 = 1_000_000.0;
pub const MAX_LOCAL_SCALE: f32 = 16.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Decision {
    Rejected,
    Possible,
    Unsupported(Unsupported),
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Unsupported {
    InheritedScale,
    Rotation,
    SizeDispatch,
    HitDispatch,
    ResourceType,
    UnloadedResource,
    ZeroSizeFallback,
    TextAlignment,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GeometryError {
    Canvas,
    Point,
    Translation,
    Dimensions,
    TileCount,
    Scale,
    NonFinite,
    Overflow,
}

/// `canvas` is the known node's packed +120/+124 dimensions. Translation is the
/// fresh effective b0/b4, not an unverified previous-frame cached rectangle.
/// Inherited scale is the prepared accumulated scale from05e0, distinct from
/// Sprite's own local114. Non-unit inherited transforms remain unsupported.
#[derive(Clone, Copy, Debug)]
pub struct Common {
    pub canvas: [i32; 2],
    pub translation: [f32; 2],
    pub inherited_scale: f32,
    pub rotation: f32,
}

#[derive(Clone, Copy, Debug)]
pub struct SpriteType {
    pub primary_rva: u64,
    pub texture: [i32; 2], // resource8c/90
    pub tiles: [i32; 2],   // resource78/7c
    pub orientation: i32,  // resourcefc: exactly1 swaps the denominator pair
    pub loaded_index: i32, // resource188: >=0 implies lazy helper's no-write arm
}
#[derive(Clone, Copy, Debug)]
pub enum SpriteResource {
    SpriteType(SpriteType),
    Unsupported,
}

fn common(point: [f32; 2], c: Common) -> Result<Option<Unsupported>, GeometryError> {
    if c.canvas.iter().any(|v| !(1..=MAX_CANVAS).contains(v)) {
        return Err(GeometryError::Canvas);
    }
    if point.iter().any(|v| !v.is_finite()) {
        return Err(GeometryError::NonFinite);
    }
    if point[0] < 0.0
        || point[1] < 0.0
        || point[0] >= c.canvas[0] as f32
        || point[1] >= c.canvas[1] as f32
    {
        return Err(GeometryError::Point);
    }
    if c.translation.iter().any(|v| !v.is_finite())
        || !c.inherited_scale.is_finite()
        || !c.rotation.is_finite()
    {
        return Err(GeometryError::NonFinite);
    }
    if c.translation.iter().any(|v| v.abs() > MAX_COORDINATE) {
        return Err(GeometryError::Translation);
    }
    if c.inherited_scale <= 0.0 || c.inherited_scale > MAX_LOCAL_SCALE {
        return Err(GeometryError::Scale);
    }
    if c.inherited_scale != 1.0 {
        return Ok(Some(Unsupported::InheritedScale));
    }
    if c.rotation != 0.0 {
        return Ok(Some(Unsupported::Rotation));
    }
    Ok(None)
}
fn finite(v: f32) -> Result<f32, GeometryError> {
    v.is_finite().then_some(v).ok_or(GeometryError::NonFinite)
}
fn trunc(v: f32) -> Result<i32, GeometryError> {
    finite(v)?;
    // Every conversion here is within this policy bound, avoiding Rust's
    // saturating float cast versus x86 cvtt indefinite-value differences.
    if v.abs() > MAX_COORDINATE {
        return Err(GeometryError::Overflow);
    }
    Ok(v.trunc() as i32)
}
fn dimensions(dims: [i32; 2]) -> Result<(), GeometryError> {
    if dims.iter().any(|v| !(0..=MAX_DIMENSION).contains(v)) {
        return Err(GeometryError::Dimensions);
    }
    Ok(())
}
fn add(a: i32, b: i32) -> Result<i32, GeometryError> {
    a.checked_add(b).ok_or(GeometryError::Overflow)
}

// Exact scalar grouping of1027ed3e0. Do not reassociate y as H/2-ty-h:
// mulss H*.5; subss ty-halfH; addss height+that; xor sign.
fn position(c: Common, height: i32) -> Result<[f32; 2], GeometryError> {
    let half_x = finite((c.canvas[0] as f32) * 0.5)?;
    let x = finite(half_x + c.translation[0])?;
    let half_y = finite((c.canvas[1] as f32) * 0.5)?;
    let y = finite(c.translation[1] - half_y)?;
    let y = finite((height as f32) + y)?;
    Ok([x, -y])
}

fn sprite_resource(r: SpriteType) -> Result<Option<Unsupported>, GeometryError> {
    if r.primary_rva != SPRITE_TYPE_PRIMARY_RVA {
        return Ok(Some(Unsupported::ResourceType));
    }
    if r.loaded_index < 0 {
        return Ok(Some(Unsupported::UnloadedResource));
    }
    if r.texture.iter().any(|v| !(1..=MAX_DIMENSION).contains(v)) {
        return Err(GeometryError::Dimensions);
    }
    if r.tiles.iter().any(|v| !(1..=MAX_TILES).contains(v)) {
        return Err(GeometryError::TileCount);
    }
    Ok(None)
}
fn divisors(r: SpriteType) -> [i32; 2] {
    if r.orientation == 1 {
        [r.tiles[1], r.tiles[0]]
    } else {
        r.tiles
    }
}
//10284fb20 returns truncated per-tile dimensions;10284b8d0 multiplies those
// integers by local114 and truncates again. They are not hit extents.
fn sprite_size(r: SpriteType, scale: f32) -> Result<[i32; 2], GeometryError> {
    let ds = divisors(r);
    let mut out = [0; 2];
    for axis in 0..2 {
        let reciprocal = finite(1.0_f32 / (ds[axis] as f32))?;
        let first = finite((r.texture[axis] as f32) * reciprocal)?;
        let first = trunc(first)?;
        let second = finite((first as f32) * scale)?;
        out[axis] = trunc(second)?;
    }
    Ok(out)
}

/// Exact outer rejection of1027ed6a0 -> known CSpriteType virtual88.
/// The caller must bind each exact getter: CSprite size/hit use168; animated
/// and Shield size uses168 while hit uses1b0. Unknown size dispatch is explicit.
/// This result never establishes a positive texture/mask hit.
pub fn sprite(
    point: [f32; 2],
    c: Common,
    local_scale: f32,
    size: SpriteResource,
    hit: SpriteResource,
) -> Result<Decision, GeometryError> {
    if let Some(reason) = common(point, c)? {
        return Ok(Decision::Unsupported(reason));
    }
    if !local_scale.is_finite() {
        return Err(GeometryError::NonFinite);
    }
    if local_scale <= 0.0 || local_scale > MAX_LOCAL_SCALE {
        return Err(GeometryError::Scale);
    }
    let SpriteResource::SpriteType(size) = size else {
        return Ok(Decision::Unsupported(Unsupported::SizeDispatch));
    };
    let SpriteResource::SpriteType(hit) = hit else {
        return Ok(Decision::Unsupported(Unsupported::HitDispatch));
    };
    for r in [size, hit] {
        if let Some(reason) = sprite_resource(r)? {
            return Ok(Decision::Unsupported(reason));
        }
    }
    let size = sprite_size(size, local_scale)?;
    if size == [0, 0] {
        //1027ed3a0 substitutes node12c/130 when virtual160 returns qword0.
        return Ok(Decision::Unsupported(Unsupported::ZeroSizeFallback));
    }
    let p = position(c, size[1])?;
    let top_left = [trunc(p[0])?, trunc(p[1])?];
    let point = [trunc(point[0])?, trunc(point[1])?];
    let ds = divisors(hit);
    for axis in 0..2 {
        if top_left[axis] > point[axis] {
            return Ok(Decision::Rejected);
        }
        //10284fe70: dimension*scale, then *reciprocal, then +integer origin.
        let reciprocal = finite(1.0_f32 / (ds[axis] as f32))?;
        let scaled = finite((hit.texture[axis] as f32) * local_scale)?;
        let extent = finite(scaled * reciprocal)?;
        let end = finite(extent + (top_left[axis] as f32))?;
        if end <= point[axis] as f32 {
            return Ok(Decision::Rejected);
        }
    }
    Ok(Decision::Possible)
}

/// Cornered virtual80 first compares the float point against virtual150 with
/// size argument0, then i32 dimensions16c/170. Texture tests only further reject.
pub fn cornered(point: [f32; 2], c: Common, dims: [i32; 2]) -> Result<Decision, GeometryError> {
    if let Some(reason) = common(point, c)? {
        return Ok(Decision::Unsupported(reason));
    }
    dimensions(dims)?;
    let top_left = position(c, 0)?;
    for axis in 0..2 {
        let end = finite((dims[axis] as f32) + top_left[axis])?;
        if top_left[axis] > point[axis] || point[axis] >= end {
            return Ok(Decision::Rejected);
        }
    }
    Ok(Decision::Possible)
}

/// InstantText virtual80's alignment0 rejection branch. It truncates both the
/// integer-offset+translation boundaries and the point; both edges are strict.
/// Later virtual calls/byte218 may reject an inside point, never make a miss hit.
pub fn instant_text_alignment0(
    point: [f32; 2],
    c: Common,
    offset: [i32; 2],
    dims: [i32; 2],
    alignment: i32,
) -> Result<Decision, GeometryError> {
    if let Some(reason) = common(point, c)? {
        return Ok(Decision::Unsupported(reason));
    }
    if alignment != 0 {
        return Ok(Decision::Unsupported(Unsupported::TextAlignment));
    }
    dimensions(dims)?;
    if offset
        .iter()
        .any(|v| v.unsigned_abs() > MAX_COORDINATE as u32)
    {
        return Err(GeometryError::Overflow);
    }
    let left = trunc(finite((offset[0] as f32) + c.translation[0])?)?;
    let top = trunc(finite((offset[1] as f32) - c.translation[1])?)?;
    let point = [trunc(point[0])?, trunc(point[1])?];
    if left >= point[0]
        || add(left, dims[0])? <= point[0]
        || top >= point[1]
        || add(top, dims[1])? <= point[1]
    {
        return Ok(Decision::Rejected);
    }
    Ok(Decision::Possible)
}

/// C2dPositionObject virtual80 -> virtual2b8. No canvas-half adjustment:
/// left=trunc(b0), top=trunc(-b4); lower inclusive, upper exclusive. The compared
/// point remains f32 (unlike Sprite/InstantText). Width/height are12c/130.
pub fn position_object(
    point: [f32; 2],
    c: Common,
    dims: [i32; 2],
) -> Result<Decision, GeometryError> {
    if let Some(reason) = common(point, c)? {
        return Ok(Decision::Unsupported(reason));
    }
    dimensions(dims)?;
    let left = trunc(c.translation[0])?;
    let top = trunc(-c.translation[1])?;
    let right = add(left, dims[0])?;
    let bottom = add(top, dims[1])?;
    if point[0] < left as f32
        || point[0] >= right as f32
        || point[1] < top as f32
        || point[1] >= bottom as f32
    {
        return Ok(Decision::Rejected);
    }
    Ok(Decision::Possible)
}

pub mod variants;

#[cfg(test)]
mod tests;
