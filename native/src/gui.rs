//! Current-build, read-only input-coordinate diagnostics. These observations do
//! not authorize clicks or establish an X11 client-origin/HiDPI mapping.
pub(crate) mod macos;

use crate::{
    error::{Error, Result},
    game,
    memory::{Reader, StringAbi},
    platform::Process,
};
use serde_json::{json, Value};
use std::time::{Instant, SystemTime, UNIX_EPOCH};

fn point_source(r: &Reader<'_>, base: u64, receiver: u64) -> Result<Value> {
    let unavailable = |reason| json!({"getter_source_layout_verified":false,"reason":reason});
    if receiver == 0 || r.u64(receiver)? != base + 0x435cee0 {
        return Ok(unavailable("receiver_layout_unavailable"));
    }
    // Constructor-proved CPdxMouse primary +c8 tail-calls the packed point
    // wrapper. Its incoming byte arguments are overwritten, not returned.
    if r.u64(base + 0x435cee0 + 0xc8)? != base + 0x2fad780 {
        return Ok(unavailable("receiver_getter_unavailable"));
    }
    // SDL_DYNAMIC_API can replace this implementation. Never infer globals
    // from the initial ELF relocation or invoke its lazy initializer.
    if r.u64(base + 0x4391098)? != base + 0x301a3a0 {
        return Ok(unavailable("builtin_point_source_unavailable"));
    }
    let point = [r.i32(base + 0x463b960)?, r.i32(base + 0x463b964)?];
    if point.iter().any(|v| v.unsigned_abs() > 1_000_000) {
        return Err(Error::new(
            "invalid_gui_geometry",
            "Source point outside diagnostic bound",
        ));
    }
    Ok(
        json!({"getter_source_layout_verified":true,"source_point_raw":point,"source_origin":"unverified; builtin point state only","fresh_event_verified":false}),
    )
}

fn source_pointer(pointer: u64) -> Result<u64> {
    if !(0x10000..=0x7fff_ffff_0000).contains(&pointer) {
        return Err(Error::new(
            "invalid_input_source",
            "Bounded input subsystem pointer required",
        ));
    }
    Ok(pointer)
}
fn source_window(r: &Reader<'_>, base: u64, dimensions: &[i32]) -> Result<Value> {
    let unavailable = |reason| json!({"association_available":false,"reason":reason,"owned_os_window_verified":false});
    let video = r.u64(base + 0x463bbf8)?;
    if video == 0 {
        return Ok(unavailable("active_video_unavailable"));
    }
    source_pointer(video)?;
    // Nonpolymorphic SDL objects: independently recovered constructor callback
    // fingerprint, not an RTTI name or a guessed layout for another driver.
    for (offset, function) in [(0x48, 0x307d430), (0x58, 0x307e0d0), (0x228, 0x30f5d30)] {
        if r.u64(video + offset)? != base + function {
            return Ok(unavailable("x11_driver_layout_unavailable"));
        }
    }
    let window = r.u64(base + 0x463b958)?;
    if window == 0 {
        return Ok(unavailable("input_window_focus_unavailable"));
    }
    source_pointer(window)?;
    if r.u64(window)? != video + 0x2d8 {
        return Err(Error::new(
            "invalid_input_source",
            "Active SDL window magic required",
        ));
    }
    let size = [r.i32(window + 0x28)?, r.i32(window + 0x2c)?];
    if size.iter().any(|value| !(1..=16384).contains(value)) {
        return Err(Error::new(
            "invalid_input_source",
            "Bounded input window dimensions required",
        ));
    }
    let driver = r.u64(video + 0x5a8)?;
    let data = r.u64(window + 0xd8)?;
    source_pointer(driver)?;
    source_pointer(data)?;
    if r.u64(data)? != window || r.u64(data + 0xe0)? != driver {
        return Err(Error::new(
            "invalid_input_source",
            "Reciprocal X11 input window driver association required",
        ));
    }
    let count = r.i32(driver + 0x2c)?;
    let entries = r.u64(driver + 0x30)?;
    let capacity = r.i32(driver + 0x38)?;
    source_pointer(entries)?;
    if count < 1 || capacity < count || entries % 8 != 0 {
        return Err(Error::new(
            "invalid_input_source",
            "Valid complete input window registry required",
        ));
    }
    if capacity > 128 {
        return Ok(unavailable("input_window_registry_policy_bound"));
    }
    let pointers = r.read(entries, count as usize * 8)?;
    if pointers
        .chunks_exact(8)
        .filter(|bytes| u64::from_le_bytes((*bytes).try_into().unwrap()) == data)
        .count()
        != 1
    {
        return Err(Error::new(
            "invalid_input_source",
            "Unique input window registry membership required",
        ));
    }
    // Only the selected entry is dereferenced. Native XID is a diagnostic until
    // the OS backend independently verifies its PID/class/display ownership.
    let xid = r.u64(data + 8)?;
    if xid == 0 || xid > u32::MAX as u64 {
        return Err(Error::new(
            "invalid_input_source",
            "Bounded native input window ID required",
        ));
    }
    let relative = [r.i32(base + 0x463b984)?, r.i32(base + 0x463b988)?];
    Ok(
        json!({"association_available":true,"association_scope":"builtin input focus and reciprocal X11 driver registry","native_window_id":xid,"window_dimensions":size,"graphics_dimensions_match":size==dimensions[..2],"relative_mode_raw":relative,"normal_absolute_mode":relative==[0,0],"owned_os_window_verified":false,"client_origin_verified":false,"fresh_event_verified":false}),
    )
}

pub fn input_geometry(process: &dyn Process) -> Result<Value> {
    input_geometry_with_source(process, false)
}

pub fn input_geometry_with_source(
    process: &dyn Process,
    include_source_point: bool,
) -> Result<Value> {
    if include_source_point && process.string_abi() != StringAbi::LibCxx {
        return Err(Error::new(
            "unsupported_platform",
            "Optional source-point observation requires the reviewed Mac layout",
        ));
    }
    let start = Instant::now();
    process.begin_snapshot()?;
    let observed = crate::research_ui::Witnesses::new(process);
    let r = Reader::new(&observed);
    let base = process.image_base();
    let mut value = input_geometry_from_reader_with_source(&r, base, include_source_point)?;
    let (end_calls, end_bytes) = observed.finish()?;
    process.check()?;
    value["freshness"] = json!({
        "observed_unix_ms":SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_millis(),
        "elapsed_ms":start.elapsed().as_secs_f64()*1000.0,
        "read_calls_kind":"logical_read_requests",
        "read_calls":r.calls.get()+end_calls as u64,"read_bytes":r.bytes.get()+end_bytes
    });
    Ok(value)
}
/// Caller supplies the encompassing process and final-witness bracket.
/// Shared with combined own control diagnostics; never an input authorization.
pub(crate) fn input_geometry_from_reader(r: &Reader<'_>, base: u64) -> Result<Value> {
    input_geometry_from_reader_with_source(r, base, false)
}

fn input_geometry_from_reader_with_source(
    r: &Reader<'_>,
    base: u64,
    include_source_point: bool,
) -> Result<Value> {
    if r.string_abi() == StringAbi::LibCxx {
        let geometry = macos::from_reader(r, base)?;
        let mut value = geometry.diagnostic();
        if include_source_point {
            let source = macos::source_point::from_reader(r, base, &geometry, None)?;
            value["source_point"] = json!({
                "source":"cached SDL ordinary absolute mouse coordinates",
                "point":source.point(),"window_dimensions":source.binding().dimensions(),
                "owned_sdl_window_association_verified":true,
                "initialized":true,"relative_mode":false,"warp_mode":false,
                "cache_completion_verified":false,"fresh_event_verified":false,
                "current_frame_verified":false,"input_authorized":false,
                "delivery_verified":false
            });
        }
        return Ok(value);
    }
    if include_source_point {
        return Err(Error::new(
            "unsupported_platform",
            "Optional source-point observation requires the reviewed Mac layout",
        ));
    }
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
        match crate::research_ui::nullable_type(r, base, input_source) {
            Ok(value) => (value, input_source != 0),
            Err(error) if error.code == "invalid_gui_type" => (Value::Null, false),
            Err(error) => return Err(error),
        };
    let mut source_point = if metadata_available {
        point_source(r, base, input_source)?
    } else {
        json!({"getter_source_layout_verified":false,"reason":"receiver_metadata_unavailable"})
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
    if source_point["getter_source_layout_verified"] == true {
        let mut logical = Vec::new();
        for (axis, ratio) in ratios.iter().take(2).enumerate() {
            let converted = source_point["source_point_raw"][axis].as_i64().unwrap() as f32 * ratio;
            if !converted.is_finite() || converted.abs() as f64 > i32::MAX as f64 {
                return Err(Error::new(
                    "invalid_gui_geometry",
                    "Source logical point exceeds signed conversion bound",
                ));
            }
            logical.push(converted.trunc());
        }
        source_point["source_logical_point"] = json!(logical);
        source_point["logical_cache_equivalent"] = json!(logical == point);
        source_point["window_association"] = source_window(r, base, &dims)?;
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
    Ok(json!({
        "source":"native own GUI diagnostics","build_id":game::BUILD_ID,
        "physical_dimensions":&dims[..2],"logical_dimensions":&dims[2..],
        "logical_per_physical":&ratios[..2],"physical_per_logical":&ratios[2..],
        "logical_pointer":point,
        "input_source":{"materialized":input_source!=0,"type_encoding":source_type,"type_metadata_available":metadata_available,"field_layout_verified":false,"client_origin_verified":false,"point_diagnostic":source_point},
        "input_mapping_verified":false,
        "unverified":["X11 client origin","HiDPI conversion","per-control scalar/pivot","effective hit routing"],
        "observation_paused_game":false
    }))
}

#[cfg(test)]
mod source_option_tests {
    use super::*;
    use crate::memory::ReadMemory;

    struct NoAccess;
    impl ReadMemory for NoAccess {
        fn read(&self, _: u64, _: usize) -> Result<Vec<u8>> {
            panic!("Optional Mac source flag accessed another platform");
        }
    }
    impl Process for NoAccess {
        fn image_base(&self) -> u64 {
            panic!("Unexpected image access")
        }
        fn check(&self) -> Result<()> {
            panic!("Unexpected snapshot/process access")
        }
    }
    #[test]
    fn non_mac_opt_in_refuses_before_snapshot_or_memory_access() {
        let error = input_geometry_with_source(&NoAccess, true).unwrap_err();
        assert_eq!(error.code, "unsupported_platform");
        let reader = Reader::new(&NoAccess);
        assert_eq!(
            input_geometry_from_reader_with_source(&reader, 0, true)
                .unwrap_err()
                .code,
            "unsupported_platform"
        );
        assert_eq!(reader.calls.get(), 0);
    }
}
