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
    let r = Reader::new(process);
    let base = process.image_base();
    let gs = r.u64(base + 0x44086a8)?;
    let idler = r.u64(base + 0x44085f8)?;
    if r.u64(gs)? != base + 0x42c7598 || r.u64(idler)? != base + 0x42c9a88 {
        return Err(Error::new("campaign_unavailable", "In-game GUI required"));
    }
    // The owner supplies this exact CGui to the ordinary technology view.
    // CGui constructor stores CGraphics at +b8; the game-derived constructor
    // installs CGameGraphics, also published by the graphics global setter.
    let gui = r.u64(idler + 0x4f8)?;
    let graphics = r.u64(base + 0x4577890)?;
    if r.u64(gui)? != base + 0x434c068
        || r.u64(graphics)? != base + 0x42417f8
        || r.u64(gui + 0xb8)? != graphics
    {
        return Err(Error::new(
            "gui_unavailable",
            "Verified game GUI/graphics required",
        ));
    }
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
        || r.u64(gui)? != base + 0x434c068
        || r.u64(gui + 0xb8)? != graphics
        || r.u64(graphics)? != base + 0x42417f8
        || r.read(graphics + 0x40, 8)? != pointer
        || r.read(graphics + 0x178, 16)? != dimensions
        || r.read(graphics + 0x1d0, 16)? != scales
    {
        return Err(Error::new("torn_read", "GUI input geometry changed"));
    }
    process.check()?;
    Ok(json!({
        "source":"native own GUI diagnostics","build_id":game::BUILD_ID,
        "physical_dimensions":&dims[..2],"logical_dimensions":&dims[2..],
        "logical_per_physical":&ratios[..2],"physical_per_logical":&ratios[2..],
        "logical_pointer":point,
        "input_mapping_verified":false,
        "unverified":["X11 client origin","HiDPI conversion","per-control scalar/pivot","effective hit routing"],
        "observation_paused_game":false,
        "freshness":{
            "observed_unix_ms":SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_millis(),
            "elapsed_ms":start.elapsed().as_secs_f64()*1000.0,
            "read_calls":r.calls.get(),"read_bytes":r.bytes.get()
        }
    }))
}
