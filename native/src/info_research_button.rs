//! Matching own Info's ordinary Standard/void callback diagnostic. No input.
use crate::{
    error::{Error, Result},
    memory::Reader,
    node_reuse::Reuse,
    research_ui::{exact, named_research_button},
};
use serde_json::{json, Value};
use std::collections::BTreeSet;

fn metadata(r: &Reader<'_>, reuse: &mut Reuse, pointer: u64) -> Result<Value> {
    match reuse.nullable_type(r, pointer) {
        Ok(value) => Ok(value),
        Err(error) if error.code == "invalid_gui_type" => Ok(Value::Null),
        Err(error) => Err(error),
    }
}
fn pointer(pointer: u64) -> Result<()> {
    if pointer == 0 || pointer % 8 != 0 {
        Err(Error::new(
            "invalid_gui_binding",
            "Aligned nonnull ordinary control path required",
        ))
    } else {
        Ok(())
    }
}

pub(crate) fn observe(
    r: &Reader<'_>,
    base: u64,
    info: u64,
    container: u64,
    registry: u64,
) -> Result<Value> {
    // Caller has already proved own Info/canonical selection/local visibility.
    exact(r, info, base + 0x430bbf0)?;
    exact(r, container, base + 0x4347d58)?;
    if r.u64(container + 0x98)? != registry {
        return Err(Error::new(
            "invalid_gui_owner",
            "Research container must belong to own registry",
        ));
    }
    let mut out = json!({"diagnostic_scope":"matching_locally_visible_own_info_research",
        "available":false,"reason":"named_control_unavailable","control_type_encoding":null,
        "control_layout_verified":false,"ordinary_alias_matches":false,"callback_binding_verified":false,
        "renderer_type_encoding":null,"renderer_layout_supported":false,"renderer_control_join_verified":false,
        "geometry_available":false,"eligibility":"unavailable","input_authorized":false,"effective_routing_verified":false});
    let button = match named_research_button(r, container) {
        Ok(button) => button,
        Err(error) if error.code == "gui_unavailable" => return Ok(out),
        Err(error) => return Err(error),
    };
    if button == 0 {
        return Ok(out);
    }
    pointer(button)?;
    if r.u64(info + 0xac0)? != button {
        return Err(Error::new(
            "invalid_gui_registry",
            "Named research must equal Info ordinary control",
        ));
    }
    let mut reuse = Reuse::new(base, 0);
    out["control_type_encoding"] = metadata(r, &mut reuse, button)?;
    if r.u64(button)? != base + 0x43470c8 {
        out["reason"] = json!("control_layout_unavailable");
        return Ok(out);
    }
    out["control_layout_verified"] = json!(true);
    if r.u64(button + 0x68)? != registry {
        return Err(Error::new(
            "invalid_gui_owner",
            "Ordinary Standard must belong to own registry",
        ));
    }
    let alias = r.u64(info + 0xab0)? == button;
    out["ordinary_alias_matches"] = json!(alias);
    if !alias {
        out["reason"] = json!("ordinary_branch_unavailable");
        return Ok(out);
    }
    let disabled = r.i32(button + 0xac)?;
    out["disabled_raw"] = json!(disabled);
    out["disabled_state3"] = json!(disabled == 3);
    out["flags_raw"] = json!(u32::from_le_bytes(
        r.read(button + 0x74, 4)?.try_into().unwrap()
    ));
    if r.u64(button + 0x80)? != base + 0x4347488 {
        out["reason"] = json!("observer_layout_unavailable");
        return Ok(out);
    }
    if r.u64(button + 0xa0)? != 0 {
        out["reason"] = json!("single_observer_override");
        return Ok(out);
    }
    let count = r.i32(button + 0x98)?;
    if !(0..=16).contains(&count) {
        return Err(Error::new(
            "invalid_gui_binding",
            "Listener count exceeds diagnostic bound",
        ));
    }
    let mut link = r.u64(button + 0x88)?;
    let tail = r.u64(button + 0x90)?;
    let mut prior = 0;
    let mut seen = BTreeSet::new();
    let mut target = None;
    for _ in 0..count {
        pointer(link)?;
        if !seen.insert(link) || r.u64(link + 8)? != prior {
            return Err(Error::new(
                "invalid_gui_binding",
                "Malformed ordinary listener links",
            ));
        }
        let listener = r.u64(link)?;
        if r.byte(link + 0x18)? != 0 {
            return Err(Error::new(
                "invalid_gui_binding",
                "Removed ordinary listener is unavailable",
            ));
        }
        target = Some(listener);
        prior = link;
        link = r.u64(link + 0x10)?;
    }
    if link != 0 || prior != tail || (count == 0 && tail != 0) {
        return Err(Error::new(
            "invalid_gui_binding",
            "Listener terminal/count mismatch",
        ));
    }
    let expected = info
        .checked_add(0x2c8)
        .ok_or_else(|| Error::new("invalid_gui_binding", "Callback address overflow"))?;
    if count != 1 || target != Some(expected) {
        out["reason"] = json!("callback_unavailable");
        return Ok(out);
    }
    if r.u64(expected)? != base + 0x430bd18 {
        out["reason"] = json!("callback_layout_unavailable");
        return Ok(out);
    }
    // Research uses FIRST VOID capture +8/+18/+20, not tree Ctrl +28/+38/+40.
    if r.u64(expected + 0x18)? != base + 0x2916700
        || r.u64(expected + 0x20)? != base + 0x29166d0
        || r.u64(expected + 0x38)? != 0
        || r.u64(expected + 0x1d8)? != 0
    {
        out["reason"] = json!("callback_variant_unavailable");
        return Ok(out);
    }
    let capture = r.u64(expected + 8)?;
    pointer(capture)?;
    if r.u64(capture + 0x10)? != info {
        return Err(Error::new(
            "invalid_gui_owner",
            "Void callback must capture own Info",
        ));
    }
    if r.u64(capture)? != base + 0x290cce0 || r.u64(capture + 8)? != 0 {
        out["reason"] = json!("callback_member_unavailable");
        return Ok(out);
    }
    out["callback_binding_verified"] = json!(true);
    let render = r.u64(button + 0xe8)?;
    out["renderer_type_encoding"] = metadata(r, &mut reuse, render)?;
    let primary = if render == 0 { 0 } else { r.u64(render)? };
    if ![
        base + 0x4356e08,
        base + 0x434b370,
        base + 0x434c5b0,
        base + 0x43593d8,
    ]
    .contains(&primary)
    {
        out["reason"] = json!("renderer_layout_unavailable");
        return Ok(out);
    }
    pointer(render)?;
    out["renderer_layout_supported"] = json!(true);
    if r.u64(render + 0x138)? != button {
        return Err(Error::new(
            "invalid_gui_owner",
            "Renderer must belong to own ordinary Standard",
        ));
    }
    out["renderer_control_join_verified"] = json!(true);
    if primary == base + 0x43593d8 {
        out["text_raw_diagnostic"] = text_raw(r, &mut reuse, render, container, registry)?;
        out["available"] = out["text_raw_diagnostic"]["raw_fields_available"].clone();
        out["reason"] = json!("ordinary_void_callback_and_text_renderer_join");
        return Ok(out);
    }
    if primary != base + 0x4356e08 {
        let (resource_offset, alias_offset) = if primary == base + 0x434b370 {
            (0x1b0, 0x168)
        } else {
            (0x160, 8)
        };
        let resource = r.u64(render + resource_offset)?;
        let alias_matches = r.u64(render + alias_offset)? == resource;
        out["resource_alias_matches"] = json!(alias_matches);
        if !alias_matches {
            out["reason"] = json!("resource_alias_unavailable");
            return Ok(out);
        }
        out["resource_type_encoding"] = metadata(r, &mut reuse, resource)?;
        if resource == 0 || r.u64(resource)? != base + 0x434b668 {
            out["reason"] = json!("resource_layout_unavailable");
            return Ok(out);
        }
        pointer(resource)?;
        out["resource_layout_verified"] = json!(true);
        let frames = r.i32(resource + 0x90)?;
        let grid = r.i32(resource + 0x94)?;
        let orientation = u32::from_le_bytes(r.read(resource + 0x10c, 4)?.try_into().unwrap());
        let handle = i64::from_le_bytes(r.read(resource + 0x1a0, 8)?.try_into().unwrap());
        let lazy = r.byte(resource + 0x1d3)?;
        out["resource_cache"] = json!({"scope":"local_cached_resource_facts",
            "frame_count_raw":frames,"grid_other_raw":grid,"orientation_raw":orientation,
            "handle_raw":handle,"lazy_raw":lazy,"getter_cache_equivalent":false,
            "texture_width_raw":null,"texture_height_raw":null});
        if !(1..=4096).contains(&frames) || !(1..=4096).contains(&grid) {
            out["reason"] = json!("resource_grid_out_of_bounds");
            return Ok(out);
        }
        let equivalent = lazy == 0 || handle != -1;
        out["resource_cache"]["getter_cache_equivalent"] = json!(equivalent);
        if equivalent {
            let width = r.i32(resource + 0xa4)?;
            let height = r.i32(resource + 0xa8)?;
            if !(1..=65536).contains(&width) || !(1..=65536).contains(&height) {
                out["reason"] = json!("resource_texture_out_of_bounds");
                return Ok(out);
            }
            out["resource_cache"]["texture_width_raw"] = json!(width);
            out["resource_cache"]["texture_height_raw"] = json!(height);
        }
    }
    if primary == base + 0x434b370 {
        out["frame_raw_geometry"] = frame_raw_geometry(r, &mut reuse, render, container, registry)?;
    }
    out["available"] = json!(true);
    out["reason"] = json!("ordinary_void_callback_and_renderer_join");
    Ok(out)
}

// Raw exact-build hit inputs only. No world/client projection or getter invocation.
pub(crate) fn frame_raw_geometry(
    r: &Reader<'_>,
    reuse: &mut Reuse,
    render: u64,
    container: u64,
    registry: u64,
) -> Result<Value> {
    let mut fields = Vec::new();
    for offset in [0x114, 0xb0, 0xb4] {
        let value = f32::from_le_bytes(r.read(render + offset, 4)?.try_into().unwrap());
        if !value.is_finite() || value.abs() > 1_000_000.0 {
            return Err(Error::new(
                "invalid_gui_geometry",
                "Bounded finite Frame raw geometry required",
            ));
        }
        fields.push(value);
    }
    let anchor_x = u32::from_le_bytes(r.read(render + 0x120, 4)?.try_into().unwrap());
    let anchor_y = u32::from_le_bytes(r.read(render + 0x124, 4)?.try_into().unwrap());
    let fallback_width = r.i32(render + 0x12c)?;
    let fallback_height = r.i32(render + 0x130)?;
    if anchor_x > 65536
        || anchor_y > 65536
        || !(0..=65536).contains(&fallback_width)
        || !(0..=65536).contains(&fallback_height)
    {
        return Err(Error::new(
            "invalid_gui_geometry",
            "Frame anchor/fallback diagnostic bounds exceeded",
        ));
    }
    let resource = r.u64(render + 0x1b0)?; // caller proved exact primary and alias
    let reject = r.byte(resource + 0x111)?;
    let texture_present = r.u64(resource + 0x170)? != 0;
    let mask = r.byte(resource + 0x1d0)?;
    let hierarchy = crate::node_hierarchy::observe(r, reuse, container, render, registry)?;
    Ok(
        json!({"scope":"exact_frame_local_raw_hit_inputs", "scalar_raw":fields[0],
        "cache_translation_raw":[fields[1],fields[2]], "anchor_raw":[anchor_x,anchor_y],
        "fallback_dimensions_raw":[fallback_width,fallback_height], "mask_reject_raw":reject,
        "texture_handle_present":texture_present,"mask_sampling_raw":mask,
        "no_mask_rejection_branch":reject==0 && texture_present && mask==0,
        "parent_chain":hierarchy,"hit_origin_calculated":false,"client_rectangle_available":false,
        "current_frame_verified":false,"effective_routing_verified":false,"input_authorized":false}),
    )
}

// Exact ctor2f75ad0/table43593d8 and Text resource4359740 only.
// Proof RESEARCH-TEXT-SPRITE-BLOCKER-PROOF b6808fbd4532ea3bf8cc833feff4ec50d603f490bdd4e69d02a22c3e4aa0afae.
pub(crate) fn text_raw(
    r: &Reader<'_>,
    reuse: &mut Reuse,
    render: u64,
    container: u64,
    registry: u64,
) -> Result<Value> {
    let resource = r.u64(render + 0x168)?;
    let mut out = json!({"scope":"exact_text_sprite_raw_hit_inputs","resource_type_encoding":metadata(r,reuse,resource)?,
        "resource_layout_verified":false,"raw_fields_available":false,"reason":"resource_layout_unavailable",
        "hit_projection_available":false,"frame_unwrapping_verified":false,
        "hierarchy_status":"unavailable_text_leaf_not_reviewed","client_rectangle_available":false,
        "current_frame_verified":false,"effective_routing_verified":false,"input_authorized":false});
    if resource == 0 || r.u64(resource)? != reuse.base() + 0x4359740 {
        return Ok(out);
    }
    pointer(resource)?;
    out["resource_layout_verified"] = json!(true);
    let count = u32::from_le_bytes(r.read(resource + 0x8c, 4)?.try_into().unwrap());
    let width = r.i32(render + 0x228)?;
    let height = r.i32(render + 0x22c)?;
    let divisor = r.i32(render + 0x238)?;
    let angle = f32::from_le_bytes(r.read(render + 0x110, 4)?.try_into().unwrap());
    if count == 0
        || count > 4096
        || !(0..=65536).contains(&width)
        || !(0..=65536).contains(&height)
        || !(1..=4096).contains(&divisor)
        || !angle.is_finite()
        || angle.abs() > 1_000_000.0
    {
        out["reason"] = json!("text_raw_fields_out_of_bounds");
        return Ok(out);
    }
    let mut common = Vec::new();
    for offset in [0x114, 0xb0, 0xb4] {
        let value = f32::from_le_bytes(r.read(render + offset, 4)?.try_into().unwrap());
        if !value.is_finite() || value.abs() > 1_000_000.0 {
            out["reason"] = json!("text_common_fields_out_of_bounds");
            return Ok(out);
        }
        common.push(value);
    }
    let anchor_x = u32::from_le_bytes(r.read(render + 0x120, 4)?.try_into().unwrap());
    let anchor_y = u32::from_le_bytes(r.read(render + 0x124, 4)?.try_into().unwrap());
    let fallback_width = r.i32(render + 0x12c)?;
    let fallback_height = r.i32(render + 0x130)?;
    if anchor_x > 65536
        || anchor_y > 65536
        || !(0..=65536).contains(&fallback_width)
        || !(0..=65536).contains(&fallback_height)
    {
        out["reason"] = json!("text_common_fields_out_of_bounds");
        return Ok(out);
    }
    out["scalar_raw"] = json!(common[0]);
    out["cache_translation_raw"] = json!([common[1], common[2]]);
    out["anchor_raw"] = json!([anchor_x, anchor_y]);
    out["fallback_dimensions_raw"] = json!([fallback_width, fallback_height]);
    out["parent_chain"] = crate::node_hierarchy::observe(r, reuse, container, render, registry)?;
    out["hierarchy_status"] = json!("typed_leaf_observed");
    out["resource_count_raw"] = json!(count);
    out["renderer_width_raw"] = json!(width);
    out["renderer_height_raw"] = json!(height);
    out["hit_width_divisor_raw"] = json!(divisor);
    out["transform_110_raw"] = json!(angle);
    out["dimension_width_semantics"] = json!("unsigned_renderer_228_divided_by_resource_8c");
    out["hit_width_semantics"] = json!("signed_renderer_228_divided_by_renderer_238");
    out["raw_fields_available"] = json!(true);
    out["reason"] = json!("exact_text_raw_fields_only");
    Ok(out)
}
