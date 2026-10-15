//! Exact edit renderer layouts. Raw fields and conditional hierarchy only;
//! no font getter, resource traversal, current-frame claim or input authority.
use super::*;

pub(super) fn observe(
    r: &Reader<'_>,
    base: u64,
    edit: u64,
    container: u64,
    registry: u64,
) -> Result<Value> {
    let render = r.u64(edit + 0x1b0)?;
    let mut out = json!({"available":false,"reason":"search_renderer_layout_unavailable",
        "renderer_control_join_verified":false,"input_authorized":false});
    if render == 0 {
        out["reason"] = json!("search_renderer_unmaterialized");
        return Ok(out);
    }
    if render % 8 != 0 {
        return Err(invalid("Aligned search renderer required"));
    }
    let primary = r.u64(render)?;
    if primary != base + 0x43593d8 && primary != base + 0x434cc68 {
        out["observed_table_rva"] = primary
            .checked_sub(base)
            .filter(|v| (0x41adde0..0x436d9f8).contains(v))
            .map(|v| json!(format!("0x{v:x}")))
            .unwrap_or(Value::Null);
        // Unknown renderer payload and table remain unread.
        return Ok(out);
    }
    if r.u64(render + 0x138)? != edit {
        return Err(invalid("Search text renderer and control differ"));
    }
    let mut reuse = Reuse::for_owned_focus(base);
    if primary == base + 0x43593d8 {
        out = crate::info_research_button::text_raw(r, &mut reuse, render, container, registry)?;
        out["render_type_encoding"] = json!("11CTextSprite");
        out["available"] = out["raw_fields_available"].clone();
    } else {
        // CInstantTextObject ctor2ee8270, hit2ee8d10 and origin2ee8a40.
        // Do not read CTextSprite+228/+22c/+238 or its font resource here.
        let floats = [0xb0, 0xb4, 0x114].map(|offset| {
            r.read(render + offset, 4)
                .map(|b| f32::from_le_bytes(b.try_into().unwrap()))
        });
        let [x, y, scalar] = floats;
        let (x, y, scalar) = (x?, y?, scalar?);
        let position = [r.i32(render + 0x1f0)?, r.i32(render + 0x1f4)?];
        let alignment_offset = [r.i32(render + 0x208)?, r.i32(render + 0x20c)?];
        let dimensions = [r.i32(render + 0x210)?, r.i32(render + 0x214)?];
        let alignment = r.i32(render + 0x218)?;
        let flags = r.byte(render + 0x228)?;
        if [x, y, scalar]
            .iter()
            .any(|v| !v.is_finite() || v.abs() > 1_000_000.0)
            || position
                .iter()
                .chain(alignment_offset.iter())
                .any(|v| !(-65536..=65536).contains(v))
            || dimensions.iter().any(|v| !(0..=65536).contains(v))
            || !(0..=6).contains(&alignment)
        {
            return Err(invalid("Bounded InstantText geometry required"));
        }
        let hierarchy = crate::node_hierarchy::observe(r, &mut reuse, container, render, registry)?;
        out = json!({"available":true,"reason":"exact_instant_text_raw_hit_inputs",
            "render_type_encoding":"18CInstantTextObject","field_layout_verified":true,
            "cache_translation_raw":[x,y],"scalar_raw":scalar,
            "position_raw":position,"alignment_offset_raw":alignment_offset,
            "dimensions_raw":dimensions,"alignment_raw":alignment,"flags228_raw":flags,
            "hit_rejected_flag":flags&1!=0,"parent_chain":hierarchy,
            "resource_getter_invoked":false,"current_frame_verified":false,
            "effective_routing_verified":false,"input_authorized":false});
    }
    out["renderer_control_join_verified"] = json!(true);
    Ok(out)
}
