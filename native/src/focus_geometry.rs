//! Focus-owned callback proof precedes shared exact renderer payload semantics.
use crate::{error::Result, memory::Reader, node_reuse::Reuse};
use serde_json::{json, Value};
mod frame;
#[cfg(test)]
mod tests;
pub(crate) fn observe(
    r: &Reader<'_>,
    reuse: &mut Reuse,
    container: u64,
    button: u64,
    registry: u64,
    primary: u64,
    render: u64,
) -> Result<Value> {
    let base = reuse.base();
    if primary == base + 0x4356e08 {
        crate::node_detail::standard_sprite_diagnostic(r, reuse, container, button, registry)
    } else if primary == base + 0x43593d8 {
        let mut raw = crate::info_research_button::text_raw(r, reuse, render, container, registry)?;
        raw["render_type_encoding"] = json!("11CTextSprite");
        Ok(raw)
    } else if primary == base + 0x434b370 {
        frame::observe(r, reuse, container, button, registry, render)
    } else {
        Ok(
            json!({"available":false,"reason":"focus_geometry_renderer_unavailable","field_layout_verified":false,"client_rectangle_available":false,"input_authorized":false,"effective_routing_verified":false}),
        )
    }
}
