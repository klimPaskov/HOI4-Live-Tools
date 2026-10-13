//! Current-build, read-only input-coordinate diagnostics. These observations do
//! not authorize clicks or establish an X11 client-origin/HiDPI mapping.
use crate::{
    error::{Error, Result},
    game,
    memory::Reader,
    platform::Process,
};
use serde_json::{json, Value};
use std::time::{Instant, SystemTime, UNIX_EPOCH};

pub fn input_geometry(process: &dyn Process) -> Result<Value> {
    let start = Instant::now();
    process.begin_snapshot()?;
    let observed = crate::research_ui::Witnesses::new(process);
    let r = Reader::new(&observed);
    let base = process.image_base();
    let gs = r.u64(base + 0x44086a8)?;
    let idler = r.u64(base + 0x44085f8)?;
    if r.u64(gs)? != base + 0x42c7598 || r.u64(idler)? != base + 0x42c9a88 {
        return Err(Error::new("campaign_unavailable", "In-game GUI required"));
    }
    // CGameGui's constructor calls CGui at offset zero, then installs its own
    // primary vptr. Requiring the transient base vptr refuses the actual game.
    // CGui stores CGraphics at +b8; the game-derived graphics constructor
    // installs CGameGraphics, also published by the graphics global setter.
    let gui = r.u64(idler + 0x4f8)?;
    let graphics = r.u64(base + 0x4577890)?;
    if r.u64(gui)? != base + 0x42736f8 {
        return Err(Error::new(
            "gui_unavailable",
            "Exact primary CGameGui required",
        ));
    }
    if r.u64(graphics)? != base + 0x42417f8 {
        return Err(Error::new(
            "gui_unavailable",
            "Exact primary CGameGraphics required",
        ));
    }
    if r.u64(gui + 0xb8)? != graphics {
        return Err(Error::new(
            "gui_unavailable",
            "GUI graphics owner link changed",
        ));
    }
    // The exact idler's input binding setter stores its receiver at +500;
    // ordinary GUI update passes that receiver to its virtual +c8. Metadata
    // identifies the concrete implementation only; no receiver payload/getter
    // layout or client-coordinate origin is inferred from the type name.
    let input_source = r.u64(idler + 0x500)?;
    let (source_type, metadata_available) =
        match crate::research_ui::nullable_type(&r, base, input_source) {
            Ok(value) => (value, input_source != 0),
            Err(error) if error.code == "invalid_gui_type" => (Value::Null, false),
            Err(error) => return Err(error),
        };
    let pointer = r.read(graphics + 0x40, 8)?;
    let dimensions = r.read(graphics + 0x178, 16)?;
    let scales = r.read(graphics + 0x1d0, 16)?;
    let dims: Vec<i32> = dimensions
        .chunks_exact(4)
        .map(|v| i32::from_le_bytes(v.try_into().unwrap()))
        .collect();
    let ratios: Vec<f32> = scales
        .chunks_exact(4)
        .map(|v| f32::from_le_bytes(v.try_into().unwrap()))
        .collect();
    let point: Vec<f32> = pointer
        .chunks_exact(4)
        .map(|v| f32::from_le_bytes(v.try_into().unwrap()))
        .collect();
    if dims.iter().any(|v| !(1..=16384).contains(v))
        || ratios.iter().any(|v| !v.is_finite() || *v <= 0.0)
        || point
            .iter()
            .any(|v| !v.is_finite() || v.abs() > 1_000_000.0 || v.fract() != 0.0)
    {
        return Err(Error::new(
            "invalid_gui_geometry",
            "GUI dimensions/scales/pointer outside bounds",
        ));
    }
    for axis in 0..2 {
        let inverse = dims[axis + 2] as f32 / dims[axis] as f32;
        let forward = dims[axis] as f32 / dims[axis + 2] as f32;
        if (ratios[axis] / inverse - 1.0).abs() > 0.0001
            || (ratios[axis + 2] / forward - 1.0).abs() > 0.0001
        {
            return Err(Error::new(
                "invalid_gui_geometry",
                "GUI scale/dimension mismatch",
            ));
        }
    }
    if r.u64(base + 0x44086a8)? != gs
        || r.u64(base + 0x44085f8)? != idler
        || r.u64(gs)? != base + 0x42c7598
        || r.u64(idler)? != base + 0x42c9a88
        || r.u64(idler + 0x4f8)? != gui
        || r.u64(base + 0x4577890)? != graphics
        || r.u64(gui)? != base + 0x42736f8
        || r.u64(gui + 0xb8)? != graphics
        || r.u64(graphics)? != base + 0x42417f8
        || r.read(graphics + 0x40, 8)? != pointer
        || r.read(graphics + 0x178, 16)? != dimensions
        || r.read(graphics + 0x1d0, 16)? != scales
    {
        return Err(Error::new("torn_read", "GUI input geometry changed"));
    }
    let (end_calls, end_bytes) = observed.finish()?;
    process.check()?;
    Ok(json!({
        "source":"native own GUI diagnostics","build_id":game::BUILD_ID,
        "physical_dimensions":&dims[..2],"logical_dimensions":&dims[2..],
        "logical_per_physical":&ratios[..2],"physical_per_logical":&ratios[2..],
        "logical_pointer":point,
        "input_source":{"materialized":input_source!=0,"type_encoding":source_type,"type_metadata_available":metadata_available,"field_layout_verified":false,"client_origin_verified":false},
        "input_mapping_verified":false,
        "unverified":["X11 client origin","HiDPI conversion","per-control scalar/pivot","effective hit routing"],
        "observation_paused_game":false,
        "freshness":{
            "observed_unix_ms":SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_millis(),
            "elapsed_ms":start.elapsed().as_secs_f64()*1000.0,
            "read_calls_kind":"logical_read_requests",
            "read_calls":r.calls.get()+end_calls as u64,"read_bytes":r.bytes.get()+end_bytes
        }
    }))
}
