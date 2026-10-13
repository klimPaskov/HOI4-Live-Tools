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
    if render == 0 || r.u64(render)? != base + 0x4356e08 {
        out["reason"] = json!("renderer_layout_unavailable");
        return Ok(out);
    }
    pointer(render)?;
    out["renderer_layout_supported"] = json!(true);
    if r.u64(render + 0x138)? != button {
        return Err(Error::new(
            "invalid_gui_owner",
            "CSprite must belong to own ordinary Standard",
        ));
    }
    out["renderer_control_join_verified"] = json!(true);
    out["available"] = json!(true);
    out["reason"] = json!("ordinary_void_callback_and_renderer_join");
    Ok(out)
}
