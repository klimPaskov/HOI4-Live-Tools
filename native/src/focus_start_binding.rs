//! Own selected focus EVENT start binding, distinct from Research VOID glue.
use crate::{
    error::{Error, Result},
    memory::Reader,
    node_reuse::Reuse,
};
use serde_json::{json, Value};
use std::collections::BTreeSet;
fn aligned(p: u64) -> Result<()> {
    if p == 0 || p % 8 != 0 {
        Err(Error::new(
            "invalid_focus_binding",
            "Aligned nonnull focus listener required",
        ))
    } else {
        Ok(())
    }
}
pub(crate) fn observe(
    r: &Reader<'_>,
    base: u64,
    panel: u64,
    button: u64,
    registry: u64,
    key: &str,
) -> Result<Value> {
    let mut out = json!({"available":false,"reason":"event_binding_unavailable","selected_own_key":key,
 "callback_route_verified":false,"renderer_control_join_verified":false,"renderer_type_encoding":null,
 "readback_meaning":"fresh_own_progress_active_focus_equals_requested_canonical_focus",
 "input_authorized":false,"effective_routing_verified":false,"eligibility_available":false,"client_rectangle_available":false});
    if r.u64(button + 0x68)? != registry {
        return Err(Error::new(
            "invalid_focus_owner",
            "Start Standard must belong to own registry",
        ));
    }
    if r.u64(button + 0x80)? != base + 0x4347488 {
        out["reason"] = json!("observer_layout_unavailable");
        return Ok(out);
    }
    if r.u64(button + 0xa0)? != 0 {
        out["reason"] = json!("observer_override_unavailable");
        return Ok(out);
    }
    let count = r.i32(button + 0x98)?;
    if !(0..=16).contains(&count) {
        return Err(Error::new(
            "invalid_focus_binding",
            "Focus listener count bound",
        ));
    }
    let mut node = r.u64(button + 0x88)?;
    let tail = r.u64(button + 0x90)?;
    let mut previous = 0;
    let mut seen = BTreeSet::new();
    let mut targets = BTreeSet::new();
    for _ in 0..count {
        aligned(node)?;
        if !seen.insert(node) || r.u64(node + 8)? != previous {
            return Err(Error::new(
                "invalid_focus_binding",
                "Malformed focus listener links",
            ));
        }
        if r.byte(node + 0x18)? != 0 {
            return Err(Error::new(
                "invalid_focus_binding",
                "Removed focus listener",
            ));
        }
        if !targets.insert(r.u64(node)?) {
            return Err(Error::new(
                "invalid_focus_binding",
                "Duplicate focus listener",
            ));
        }
        previous = node;
        node = r.u64(node + 0x10)?;
    }
    if node != 0 || previous != tail || (count == 0 && tail != 0) {
        return Err(Error::new(
            "invalid_focus_binding",
            "Focus listener terminal mismatch",
        ));
    }
    if count != 2 || targets != BTreeSet::from([panel + 0x2a8, panel + 0x530]) {
        return Ok(out);
    }
    for (glue, offset, manager, invoker, member) in [
        (panel + 0x2a8, 0x28, 0x283a060, 0x283a030, 0x282ffe0),
        (panel + 0x530, 0x228, 0x283a100, 0x283a0d0, 0x2839080),
    ] {
        if r.u64(glue)? != base + 0x4302598
            || r.u64(glue + offset + 0x10)? != base + manager
            || r.u64(glue + offset + 0x18)? != base + invoker
        {
            out["reason"] = json!("event_variant_unavailable");
            return Ok(out);
        }
        // Generic2838460 constructs these independent manager slots. Refuse
        // additional callback variants instead of interpreting their captures.
        for slot in [
            0x18, 0xd8, 0x118, 0x218, 0x158, 0x98, 0x38, 0x78, 0xb8, 0x1b8, 0x238, 0x258,
        ] {
            if slot != offset + 0x10 && r.u64(glue + slot)? != 0 {
                out["reason"] = json!("additional_event_variant_unavailable");
                return Ok(out);
            }
        }
        let capture = r.u64(glue + offset)?;
        aligned(capture)?;
        if r.u64(capture + 0x10)? != panel {
            return Err(Error::new(
                "invalid_focus_owner",
                "Focus event must capture own detail",
            ));
        }
        if r.u64(capture)? != base + member || r.u64(capture + 8)? != 0 {
            out["reason"] = json!("event_member_unavailable");
            return Ok(out);
        }
    }
    out["callback_route_verified"] = json!(true);
    let renderer = r.u64(button + 0xe8)?;
    let mut reuse = Reuse::new(base, 0);
    out["renderer_type_encoding"] = match reuse.nullable_type(r, renderer) {
        Ok(v) => v,
        Err(e) if e.code == "invalid_gui_type" => Value::Null,
        Err(e) => return Err(e),
    };
    if renderer == 0 {
        out["reason"] = json!("renderer_unavailable");
        return Ok(out);
    }
    let primary = r.u64(renderer)?;
    if ![0x4356e08, 0x434b370, 0x434c5b0, 0x43593d8]
        .iter()
        .any(|p| primary == base + p)
    {
        out["reason"] = json!("renderer_layout_unavailable");
        return Ok(out);
    }
    aligned(renderer)?;
    if r.u64(renderer + 0x138)? != button {
        return Err(Error::new(
            "invalid_focus_owner",
            "Focus renderer must belong to named start",
        ));
    }
    out["renderer_control_join_verified"] = json!(true);
    out["available"] = json!(true);
    out["reason"] = json!("own_selected_event_start_and_noop_listener_verified");
    Ok(out)
}
