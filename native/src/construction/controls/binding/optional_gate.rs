//! Standard's optional parent traversal gate, scoped to its named own window.
use super::*;

pub(super) fn observe(r: &Reader<'_>, base: u64, window: u64, button: u64) -> Result<Value> {
    let gate = r.u64(button + 0x1a8)?;
    if gate == 0 {
        return Ok(json!({"absent":true,"clear":true,"reason":"absent_optional_gate"}));
    }
    // Exact container construction assigns its named button the e78 position
    // through Standard virtual f0. Never inspect a different optional object.
    // The caller has already validated this exact own-registry container.
    if gate != r.u64(window + 0xe78)? {
        return Ok(json!({"absent":false,"clear":null,"reason":"unbound_optional_gate"}));
    }
    pointer(gate)?;
    if r.u64(gate)? != base + 0x43484f0 {
        return Ok(json!({"absent":false,"clear":null,"reason":"unsupported_owned_position_type"}));
    }
    // Standard virtual10 requires byte128 != 0 before base event dispatch.
    // This is a local traversal flag, not a completed-frame or routing proof.
    let descend = crate::construction::boolean(r, gate + 0x128)?;
    Ok(json!({"absent":false,"clear":descend,
        "reason":"exact_named_window_e78_position",
        "type_encoding":"17C2dPositionObject","own_position_join_verified":true,
        "descend_raw":descend}))
}
