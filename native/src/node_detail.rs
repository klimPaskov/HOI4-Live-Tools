//! One own bound node's raw rendering diagnosis, never coordinates for input.
use crate::{
    error::{Error, Result},
    memory::Reader,
    node_reuse::Reuse,
    research_ui::{array, exact},
};
use serde_json::{json, Value};
fn float(r: &Reader<'_>, a: u64) -> Result<f32> {
    let v = f32::from_le_bytes(r.read(a, 4)?.try_into().unwrap());
    if !v.is_finite() || v.abs() > 10_000_000.0 {
        return Err(Error::new(
            "invalid_gui_geometry",
            "Finite bounded renderer scalar required",
        ));
    }
    Ok(v)
}
fn unsigned(r: &Reader<'_>, a: u64) -> Result<u32> {
    Ok(u32::from_le_bytes(r.read(a, 4)?.try_into().unwrap()))
}
pub(crate) fn observe(
    r: &Reader<'_>,
    reuse: &mut Reuse,
    item: u64,
    technology: u64,
    registry: u64,
) -> Result<Value> {
    let base = reuse.base();
    let gui = r.u64(item + 0x10)?;
    exact(r, gui, base + 0x4347d58)?;
    if r.u64(gui + 0x98)? != registry {
        return Err(Error::new(
            "invalid_gui_owner",
            "Detail container must belong to own registry",
        ));
    }
    let buttons = array(r, gui + 0x958, 256)?;
    let button = *buttons
        .first()
        .ok_or_else(|| Error::new("gui_unavailable", "Detail control unavailable"))?;
    exact(r, button, base + 0x4346b10)?;
    if r.u64(button + 0x68)? != registry || r.u64(button + 8)? != technology {
        return Err(Error::new(
            "invalid_gui_owner",
            "Detail control own registry/data join required",
        ));
    }
    sprite_diagnostic(r, reuse, gui, button, registry)
}

/// Shared renderer semantics only. Callers must independently prove the control
/// callback/data association; a renderer never establishes what its button does.
pub(crate) fn sprite_diagnostic(
    r: &Reader<'_>,
    reuse: &mut Reuse,
    gui: u64,
    button: u64,
    registry: u64,
) -> Result<Value> {
    let base = reuse.base();
    exact(r, gui, base + 0x4347d58)?;
    exact(r, button, base + 0x4346b10)?;
    if r.u64(gui + 0x98)? != registry || r.u64(button + 0x68)? != registry {
        return Err(Error::new(
            "invalid_gui_owner",
            "Own container and Drag registry required before renderer payload",
        ));
    }
    let render = r.u64(button + 0x110)?;
    let mut out = json!({"render_type_encoding":reuse.nullable_type(r,render)?,"field_layout_verified":false,"client_rectangle_available":false,"input_authorized":false,"effective_routing_verified":false});
    if render == 0 || r.u64(render)? != base + 0x4356e08 {
        out["reason"] = json!("renderer_layout_unavailable");
        return Ok(out);
    }
    if r.u64(render + 0x138)? != button {
        return Err(Error::new(
            "invalid_gui_owner",
            "Renderer must belong to own bound Drag",
        ));
    }
    let resource = r.u64(render + 0x168)?;
    out["resource_type_encoding"] = reuse.nullable_type(r, resource)?;
    if resource == 0 || r.u64(resource)? != base + 0x4357160 {
        out["reason"] = json!("resource_layout_unavailable");
        return Ok(out);
    }
    let scalar = float(r, render + 0x114)?;
    let local = [float(r, render + 0x70)?, float(r, render + 0x74)?];
    let world = [float(r, render + 0xb0)?, float(r, render + 0xb4)?];
    let pivot = [unsigned(r, render + 0x120)?, unsigned(r, render + 0x124)?];
    let mut clip = Vec::new();
    for off in [0x148, 0x14c, 0x150, 0x154] {
        clip.push(r.i32(render + off)?);
    }
    let hierarchy = crate::node_hierarchy::observe(r, reuse, gui, render, registry)?;
    let grid = [r.i32(resource + 0x90)?, r.i32(resource + 0x94)?];
    let dimensions = [r.i32(resource + 0xa4)?, r.i32(resource + 0xa8)?];
    let orientation = unsigned(r, resource + 0x10c)?;
    let handle = i64::from_le_bytes(r.read(resource + 0x1a0, 8)?.try_into().unwrap());
    let lazy = r.byte(resource + 0x1d3)?;
    let reject = r.byte(resource + 0x111)?;
    let present = r.u64(resource + 0x170)? != 0;
    let mask = r.byte(resource + 0x1d0)?;
    let cache_equivalent = lazy == 0 || handle != -1;
    let numeric_ok = scalar > 0.0
        && scalar <= 1024.0
        && grid.iter().all(|v| (1..=131072).contains(v))
        && dimensions.iter().all(|v| (1..=131072).contains(v))
        && matches!(orientation, 0 | 1);
    let computed = if cache_equivalent && numeric_ok {
        let divisors = if orientation == 1 {
            [grid[1], grid[0]]
        } else {
            grid
        };
        let mut values = Vec::new();
        for i in 0..2 {
            let first = (dimensions[i] as f32 / divisors[i] as f32).trunc();
            let second = (first * scalar).trunc();
            if second < 1.0 || second as f64 > i32::MAX as f64 {
                return Err(Error::new(
                    "invalid_gui_geometry",
                    "Cached diagnostic dimensions exceed bound",
                ));
            }
            values.push(second as i32);
        }
        json!(values)
    } else {
        Value::Null
    };
    out["field_layout_verified"] = json!(true);
    out["reason"] = json!("raw_cached_geometry_only");
    out["raw"] = json!({"local_translation":local,"cached_world_translation":world,"local_scalar":scalar,"pivot_dimensions":pivot,"fallback_dimensions":[r.i32(render+0x12c)?,r.i32(render+0x130)?],"frame":unsigned(r,render+0x34)?,"visibility_flag":r.byte(render+0x144)?,"clipping_flag":r.byte(render+0x145)?,"physical_scaled_clip":clip});
    out["resource_cache"] = json!({"grid":grid,"texture_dimensions":dimensions,"orientation_raw":orientation,"lazy_flag":lazy,"handle_raw":handle,"reject_flag":reject,"texture_present":present,"mask_flag":mask,"cache_equivalent":cache_equivalent,"diagnostic_dimensions":computed,"mask_sampling_required":reject==0&&present&&mask!=0,"no_mask_rejection_branch":reject==0&&present&&mask==0});
    out["parent_chain"] = hierarchy;
    Ok(out)
}
