//! Passive center-terrain fallback for Mach-O 089EA519 / SHA09419b08.
//! Recovered chains: 10192fc80, 102762150, 1019302e0, 101931270..12f4;
//! Independently recovered own province/definition and map-mode joins; see
//! native/PROVENANCE.md for exact-build evidence and qualification limits.
//! The caller supplies exact process/build/campaign checks and one encompassing
//! witnessed Reader, then finishes every witness and process check before use.
//! This is neither the full picker nor scene/model, cursor, GUI or input proof.
use super::{mac_projection, pointer};
use crate::{
    error::{Error, Result},
    game,
    memory::{Reader, StringAbi},
};
use std::collections::BTreeMap;

const CENTER: [i32; 2] = [960, 540];
const CANVAS: [i32; 2] = [1920, 1080];
const MAX_ADDRESS: u64 = 0x7fff_ffff_0000;
// Observer policy, not asserted engine maxima. Public terrain traversal adds
// its own node/sample budgets; no height/bitmap allocation is read wholesale.
const MAX_CELLS: u64 = 64 * 1024 * 1024;
const COUNTRY_LIMIT: usize = 4096;
const PROVINCE_LIMIT: usize = 20_000;

#[derive(Clone, Debug, PartialEq, Eq)]
struct Header {
    data: u64,
    capacity: usize,
    count: usize,
}
#[derive(Clone, Debug, PartialEq, Eq)]
struct Grid {
    object: u64,
    data: u64,
    dimensions: [i32; 2],
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Binding {
    base: u64,
    gs: u64,
    idler: u64,
    tag: i32,
    index: usize,
    country: u64,
    state: u64,
    state_id: i32,
    gui: u64,
    graphics: u64,
    resource: u64,
    map: u64,
    camera: u64,
    data: u64,
    mode_object: u64,
    mode: i32,
    wrap: u8,
    catalogs: [Header; 5], // tag indexes, countries, states, provinces, definitions
    dimensions: [i32; 2],
    height: Grid,
    bitmap: Grid,
    province: u64,
    province_id: i32,
    definition: u64,
}
#[derive(Debug)]
pub(crate) struct Observation {
    binding: Binding,
    world: [f32; 3],
    counts: [usize; 3], // candidates, samples (including repeats), nodes
}
impl Observation {
    pub(crate) fn binding(&self) -> &Binding {
        &self.binding
    }
    pub(crate) fn center(&self) -> [i32; 2] {
        CENTER
    }
    pub(crate) fn world_point(&self) -> [f32; 3] {
        self.world
    }
    pub(crate) fn province_id(&self) -> i32 {
        self.binding.province_id
    }
    pub(crate) fn state_id(&self) -> i32 {
        self.binding.state_id
    }
    pub(crate) fn mode(&self) -> i32 {
        self.binding.mode
    }
    pub(crate) fn terrain_counts(&self) -> [usize; 3] {
        self.counts
    }
}
fn invalid(message: &'static str) -> Error {
    Error::new("construction_map_point_unavailable", message)
}
fn same<T: PartialEq>(value: &T, original: Option<&T>) -> Result<()> {
    if original.is_some_and(|old| old != value) {
        return Err(Error::new(
            "construction_map_point_changed",
            "Original center-terrain binding changed before replacement payload",
        ));
    }
    Ok(())
}
fn edge(r: &Reader<'_>, address: u64, original: Option<u64>) -> Result<u64> {
    let value = r.u64(address)?;
    same(&value, original.as_ref())?;
    pointer(value)?;
    Ok(value)
}
fn exact(r: &Reader<'_>, base: u64, object: u64, primary: u64) -> Result<()> {
    if r.u64(object)? != base + primary {
        return Err(invalid("Exact Mac center-terrain primary type required"));
    }
    Ok(())
}
fn header(
    r: &Reader<'_>,
    address: u64,
    bound: usize,
    stride: usize,
    original: Option<&Header>,
) -> Result<Header> {
    let (raw, data, count) = r.array_header(address)?;
    let h = Header {
        data,
        capacity: i32::from_le_bytes(raw[8..12].try_into().unwrap()) as usize,
        count,
    };
    same(&h, original)?; // Never follow a replacement array, even at the same index.
    if h.capacity > bound || h.count == 0 || h.data % stride as u64 != 0 {
        return Err(invalid("Bounded aligned nonempty map catalog required"));
    }
    span(h.data, h.capacity as u64 * stride as u64)?;
    Ok(h)
}
fn slot(h: &Header, index: usize, stride: usize) -> Result<u64> {
    if index >= h.count {
        return Err(invalid("Requested map catalog index unavailable"));
    }
    Ok(h.data + (index * stride) as u64)
}
fn span(data: u64, bytes: u64) -> Result<()> {
    if data < 0x10000 || data.checked_add(bytes).is_none_or(|end| end > MAX_ADDRESS) {
        return Err(invalid("Bounded map grid/catalog span required"));
    }
    Ok(())
}
fn dimensions(r: &Reader<'_>, address: u64) -> Result<[i32; 2]> {
    let raw = r.read(address, 8)?;
    let d = std::array::from_fn(|i| i32::from_le_bytes(raw[i * 4..i * 4 + 4].try_into().unwrap()));
    if d.iter().any(|v| !(1..=32768).contains(v)) || d[0] as u64 * d[1] as u64 > MAX_CELLS {
        return Err(invalid("Map grid dimensions exceed observer policy"));
    }
    Ok(d)
}
fn floats<const N: usize>(r: &Reader<'_>, address: u64) -> Result<[f32; N]> {
    let raw = r.read(address, N * 4)?;
    let result =
        std::array::from_fn(|i| f32::from_le_bytes(raw[i * 4..i * 4 + 4].try_into().unwrap()));
    if result.iter().any(|v| !v.is_finite()) {
        return Err(invalid("Finite camera projection fields required"));
    }
    Ok(result)
}

/// Exact owned bindings precede all relevant payload. Only modes 0 and 27 have
/// reviewed canonical-province semantics here; 1/2/39 select regions instead. Other
/// modes remain unsupported. Camera matrices may move within one retained
/// binding, but a different derived province/definition is never adopted.
pub(crate) fn from_reader(
    r: &Reader<'_>,
    base: u64,
    requested_state_id: i32,
    original: Option<&Binding>,
) -> Result<Observation> {
    if r.string_abi() != StringAbi::LibCxx {
        return Err(Error::new(
            "unsupported_platform",
            "Exact Mac map layout required",
        ));
    }
    // Existing bounded/aligned pointer policy, plus space for every used RVA.
    // This does not replace the caller's exact Mach-O image/build validation.
    pointer(base)?;
    if base.checked_add(0x36330a8).is_none_or(|v| v > MAX_ADDRESS)
        || !(1..COUNTRY_LIMIT as i32).contains(&requested_state_id)
    {
        return Err(invalid(
            "Bounded image and positive requested state required",
        ));
    }
    same(&base, original.map(|b| &b.base))?;
    same(&requested_state_id, original.map(|b| &b.state_id))?;
    let g = &game::MACOS;
    let gs = edge(r, base + g.gs_global, original.map(|b| b.gs))?;
    exact(r, base, gs, g.gs_vtable)?;
    let idler = edge(r, base + g.idler_global, original.map(|b| b.idler))?;
    exact(r, base, idler, g.idler_vtable)?;
    let tag = r.i32(gs + g.player_tag)?;
    same(&tag, original.map(|b| &b.tag))?;
    if !(1..COUNTRY_LIMIT as i32).contains(&tag) {
        return Err(invalid("Positive own player tag required"));
    }
    let indexes = header(
        r,
        gs + g.tag_country_index,
        COUNTRY_LIMIT,
        4,
        original.map(|b| &b.catalogs[0]),
    )?;
    let index = r.i32(slot(&indexes, tag as usize, 4)?)?;
    if index < 0 {
        return Err(invalid("Own country mapping required"));
    }
    let index = index as usize;
    same(&index, original.map(|b| &b.index))?;
    let countries = header(
        r,
        gs + g.countries,
        COUNTRY_LIMIT,
        8,
        original.map(|b| &b.catalogs[1]),
    )?;
    let country = edge(r, slot(&countries, index, 8)?, original.map(|b| b.country))?;
    exact(r, base, country, g.country_vtable)?;
    if r.i32(country + 8)? != tag {
        return Err(invalid("Canonical own country must precede state payload"));
    }
    let states = header(
        r,
        gs + 0x290,
        COUNTRY_LIMIT,
        8,
        original.map(|b| &b.catalogs[2]),
    )?;
    let state = edge(
        r,
        slot(&states, requested_state_id as usize, 8)?,
        original.map(|b| b.state),
    )?;
    exact(r, base, state, 0x337caf0)?;
    if r.i32(state + 0xc0)? != tag || r.i32(state + 0xc4)? != tag {
        return Err(invalid(
            "Requested state must be owned and controlled before payload",
        ));
    }
    if r.i32(state + 0x40)? != requested_state_id {
        return Err(invalid("Requested canonical state identity required"));
    }
    let mode_object = edge(r, base + 0x36330a0, original.map(|b| b.mode_object))?;
    if r.u64(mode_object + 0x48)? != gs {
        return Err(invalid("Map mode must retain own GS before enum payload"));
    }
    let mode = r.i32(mode_object)?; // Nonpolymorphic object: this is not a vptr.
    same(&mode, original.map(|b| &b.mode))?;
    if ![0, 27].contains(&mode) {
        return Err(invalid(
            "Only reviewed canonical-province map modes 0/27 supported",
        ));
    }
    let gui = edge(r, idler + 0x4c0, original.map(|b| b.gui))?;
    exact(r, base, gui, 0x34974d0)?;
    let graphics = edge(r, gui + 0x98, original.map(|b| b.graphics))?;
    exact(r, base, graphics, 0x33dafa8)?;
    let resource = edge(r, idler + 0x4f8, original.map(|b| b.resource))?;
    if r.u64(resource + 0x300)? != graphics || r.u64(resource + 0x308)? != gui {
        return Err(invalid("Own resource GUI/graphics backlinks required"));
    }
    let map = edge(r, resource + 0x310, original.map(|b| b.map))?;
    exact(r, base, map, 0x33dab60)?;
    exact(r, base, map + 8, 0x33dab98)?;
    let camera = edge(r, idler + 0x708, original.map(|b| b.camera))?;
    let data = edge(r, base + 0x3519a00, original.map(|b| b.data))?;
    exact(r, base, data, 0x330b5b8)?;
    if r.u64(map + 0x18)? != graphics
        || r.u64(map + 0x20)? != camera
        || r.u64(map + 0x28)? != data
        || r.u64(base + 0x35dc028)? != graphics
    {
        return Err(invalid(
            "Exact own graphical map/camera/data joins required",
        ));
    }
    let canvas = r.read(graphics + 0x170, 16)?;
    let d: [i32; 4] =
        std::array::from_fn(|i| i32::from_le_bytes(canvas[i * 4..i * 4 + 4].try_into().unwrap()));
    if d != [1920, 1080, 1920, 1080]
        || floats::<1>(r, graphics + 0x2c5b0)? != [1.0]
        || r.u64(base + 0x32a1d28)? != base + 0x2ac6970
        || r.read(camera + 0x670, 2)? != [0, 0]
    {
        return Err(invalid("Qualified center canvas/scale/projection required"));
    }
    let wrap = r.byte(camera + 0x6f8)?;
    same(&wrap, original.map(|b| &b.wrap))?;
    if wrap != 1 {
        return Err(invalid("Only captured camera wrap encoding 1 supported"));
    }
    let dimensions = dimensions(r, data + 0x40)?;
    same(&dimensions, original.map(|b| &b.dimensions))?;
    let height_object = edge(r, data + 0x130, original.map(|b| b.height.object))?;
    let bitmap_object = edge(r, data + 0x7d0, original.map(|b| b.bitmap.object))?;
    // Neither grid is treated as a polymorphic object. Read only known fields.
    let height = Grid {
        object: height_object,
        data: r.u64(height_object + 0x28)?,
        dimensions: self::dimensions(r, height_object + 8)?,
    };
    let bitmap = Grid {
        object: bitmap_object,
        data: r.u64(bitmap_object + 0x18)?,
        dimensions: self::dimensions(r, bitmap_object + 0x30)?,
    };
    same(&height, original.map(|b| &b.height))?;
    same(&bitmap, original.map(|b| &b.bitmap))?;
    if r.i32(height_object + 0x10)? != 1
        || r.i32(height_object + 0x1c)? != height.dimensions[0]
        || bitmap.dimensions != dimensions
    {
        return Err(invalid(
            "Contiguous height bytes and matching province bitmap required",
        ));
    }
    span(
        height.data,
        height.dimensions[0] as u64 * height.dimensions[1] as u64,
    )?;
    span(bitmap.data, dimensions[0] as u64 * dimensions[1] as u64 * 2)?;
    let ray = mac_projection::ray(
        CENTER.map(|v| v as f32),
        CANVAS,
        &floats::<16>(r, camera + 0x110)?,
        &floats::<16>(r, camera + 0xd0)?,
        floats::<3>(r, camera + 0x190)?,
    )?;
    let mut samples = BTreeMap::new();
    let hit = mac_projection::terrain::intersect(&ray, dimensions, height.dimensions, &mut |p| {
        if let Some(v) = samples.get(&p) {
            return Ok(*v);
        }
        let offset = p[1] as u64 * height.dimensions[0] as u64 + p[0] as u64;
        let value = r.byte(height.data + offset)?;
        samples.insert(p, value);
        Ok(value)
    })?
    .ok_or_else(|| invalid("No supported center terrain hit; no plane fallback"))?;
    let [x, _, z] = hit.point;
    if x < 0.0 || x >= dimensions[0] as f32 || z < 0.0 || z >= dimensions[1] as f32 {
        return Err(invalid(
            "Center terrain point lies outside the province bitmap",
        ));
    }
    let offset = (z as u64 * dimensions[0] as u64 + x as u64) * 2;
    let province_id = i32::from(u16::from_le_bytes(
        r.read(bitmap.data + offset, 2)?.try_into().unwrap(),
    ));
    same(&province_id, original.map(|b| &b.province_id))?;
    if province_id <= 0 {
        return Err(invalid("Positive canonical province ID required"));
    }
    let provinces = header(
        r,
        gs + 0x278,
        PROVINCE_LIMIT,
        8,
        original.map(|b| &b.catalogs[3]),
    )?;
    let province = edge(
        r,
        slot(&provinces, province_id as usize, 8)?,
        original.map(|b| b.province),
    )?;
    exact(r, base, province, 0x337c7e8)?;
    if r.i32(province + 0x188)? != tag {
        return Err(invalid(
            "Own province control required before province payload",
        ));
    }
    if r.u64(province + 0xc0)? != state || r.i32(province + 0x9c)? != province_id {
        return Err(invalid(
            "Center province must join the original requested own state",
        ));
    }
    let definition = edge(r, province + 0xb8, original.map(|b| b.definition))?;
    let definitions = header(
        r,
        data + 0x208,
        PROVINCE_LIMIT,
        8,
        original.map(|b| &b.catalogs[4]),
    )?;
    if r.u64(slot(&definitions, province_id as usize, 8)?)? != definition
        || r.i32(definition + 0xbc)? != province_id
    {
        return Err(invalid(
            "Canonical province definition join required before flags",
        ));
    }
    // Definition headers are nonpolymorphic. No arbitrary first-word RTTI read.
    if r.byte(definition + 0xca)? & 3 == 0 {
        return Err(invalid(
            "Province definition does not permit canonical fallback",
        ));
    }
    Ok(Observation {
        binding: Binding {
            base,
            gs,
            idler,
            tag,
            index,
            country,
            state,
            state_id: requested_state_id,
            gui,
            graphics,
            resource,
            map,
            camera,
            data,
            mode_object,
            mode,
            wrap,
            catalogs: [indexes, countries, states, provinces, definitions],
            dimensions,
            height,
            bitmap,
            province,
            province_id,
            definition,
        },
        world: hit.point,
        counts: [hit.candidates, hit.samples, hit.nodes],
    })
}

#[cfg(test)]
mod tests;
