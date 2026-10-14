//! Reuses the exact-build Frame raw hit witness, never Sprite/Text payload.
//! Provenance: private RESEARCH-STANDARD-HIT-CANDIDATE-PROOF and renderer
//! resource constructor proof; frame dimensions/hit resource alias is required.
use crate::{
    error::{Error, Result},
    memory::Reader,
    node_reuse::Reuse,
    research_ui::exact,
};
use serde_json::{json, Value};

pub(super) fn observe(
    r: &Reader<'_>,
    reuse: &mut Reuse,
    container: u64,
    button: u64,
    registry: u64,
    render: u64,
) -> Result<Value> {
    let base = reuse.base();
    exact(r, container, base + 0x4347d58)?;
    exact(r, button, base + 0x43470c8)?;
    exact(r, render, base + 0x434b370)?;
    if r.u64(container + 0x98)? != registry
        || r.u64(button + 0x68)? != registry
        || r.u64(render + 0x138)? != button
    {
        return Err(Error::new(
            "invalid_gui_owner",
            "Own Standard/Frame/container required before resource payload",
        ));
    }
    let mut out = json!({"render_type_encoding":"20CFrameAnimatedSprite","field_layout_verified":false,"resource_layout_verified":false,"resource_alias_matches":false,"client_rectangle_available":false,"current_frame_verified":false,"effective_routing_verified":false,"input_authorized":false});
    let resource = r.u64(render + 0x1b0)?;
    if resource == 0 || r.u64(render + 0x168)? != resource {
        out["reason"] = json!("frame_resource_alias_unavailable");
        return Ok(out);
    }
    out["resource_alias_matches"] = json!(true);
    out["resource_type_encoding"] = reuse.nullable_type(r, resource)?;
    if r.u64(resource)? != base + 0x434b668 {
        out["reason"] = json!("resource_layout_unavailable");
        return Ok(out);
    }
    exact(r, resource, base + 0x434b668)?;
    out["resource_layout_verified"] = json!(true);
    let frames = r.i32(resource + 0x90)?;
    let grid = r.i32(resource + 0x94)?;
    let orientation = u32::from_le_bytes(r.read(resource + 0x10c, 4)?.try_into().unwrap());
    let handle = i64::from_le_bytes(r.read(resource + 0x1a0, 8)?.try_into().unwrap());
    let lazy = r.byte(resource + 0x1d3)?;
    out["resource_cache"] = json!({"frame_count_raw":frames,"grid_other_raw":grid,"orientation_raw":orientation,"handle_raw":handle,"lazy_raw":lazy,"getter_cache_equivalent":false,"texture_width_raw":null,"texture_height_raw":null});
    if !(1..=4096).contains(&frames) || !(1..=4096).contains(&grid) || !matches!(orientation, 0 | 1)
    {
        out["reason"] = json!("frame_resource_grid_orientation_unavailable");
        return Ok(out);
    }
    if lazy != 0 && handle == -1 {
        out["reason"] = json!("frame_resource_cache_unavailable");
        return Ok(out);
    }
    out["resource_cache"]["getter_cache_equivalent"] = json!(true);
    let width = r.i32(resource + 0xa4)?;
    let height = r.i32(resource + 0xa8)?;
    if !(1..=65536).contains(&width) || !(1..=65536).contains(&height) {
        out["reason"] = json!("frame_resource_dimensions_unavailable");
        return Ok(out);
    }
    out["resource_cache"]["texture_width_raw"] = json!(width);
    out["resource_cache"]["texture_height_raw"] = json!(height);
    out["frame_raw_geometry"] =
        crate::info_research_button::frame_raw_geometry(r, reuse, render, container, registry)?;
    out["field_layout_verified"] = json!(true);
    out["reason"] = json!("exact_frame_raw_geometry_only");
    Ok(out)
}
