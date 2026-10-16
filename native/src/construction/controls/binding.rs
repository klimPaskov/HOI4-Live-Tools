//! Exact named Standard controls and FIRST VOID construction listeners.
use super::*;
use crate::node_reuse::Reuse;

pub(super) fn local(r: &Reader<'_>, base: u64, window: u64, registry: u64) -> Result<Value> {
    if window == 0 {
        return Ok(json!({"materialized":false,"settled_visible":null}));
    }
    exact(r, window, base + 0x4347d58)?;
    if r.u64(window + 0x98)? != registry {
        return Err(invalid(
            "Construction container belongs to another registry",
        ));
    }
    let flags = r.byte(window + 0xa5)?;
    let states = [r.i32(window + 0xeb0)?, r.i32(window + 0xef8)?];
    Ok(
        json!({"materialized":true,"flags_raw":flags,"states_raw":states,
        "settled_visible":if states==[0,0] {Some(flags&8!=0)} else {None}}),
    )
}

pub(super) fn named(r: &Reader<'_>, window: u64, children: bool, key: &str) -> Result<Option<u64>> {
    let (values_offset, names_offset) = if children {
        (0xbb0, 0xbc8)
    } else {
        (0x9a0, 0x9b8)
    };
    let values = members(r, window + values_offset, 256)?;
    let (header, names, count) = r.array_header(window + names_offset)?;
    let capacity = i32::from_le_bytes(header[8..12].try_into().unwrap());
    if capacity > 256 || count != values.len() || (count > 0 && names % 8 != 0) {
        return Err(invalid(
            "Paired bounded construction control registries required",
        ));
    }
    let mut selected = None;
    for (index, value) in values.into_iter().enumerate() {
        let name = r.string_bounded(names + index as u64 * 32, 128)?;
        if name == key {
            // Engine lookup compares only the requested name. Other controls
            // may share names, including empty names. Still scan every entry
            // and refuse an ambiguous requested key before following payload.
            if selected.is_some() {
                return Err(invalid(&format!(
                    "Ambiguous requested construction {} name: {key}",
                    if children { "container" } else { "button" }
                )));
            }
            selected = Some(value);
        }
    }
    Ok(selected)
}

#[derive(Clone, Copy)]
pub(super) enum Kind {
    Opener,
    Choice,
}

fn callback(r: &Reader<'_>, base: u64, owner: u64, button: u64, kind: Kind) -> Result<Value> {
    let mut out = json!({"verified":false,"reason":"observer_layout_unavailable"});
    if r.u64(button + 0x80)? != base + 0x4347488 {
        return Ok(out);
    }
    if r.u64(button + 0xa0)? != 0 {
        out["reason"] = json!("single_observer_override");
        return Ok(out);
    }
    let count = r.i32(button + 0x98)?;
    if !(0..=16).contains(&count) {
        return Err(invalid("Bounded construction listener count required"));
    }
    let mut link = r.u64(button + 0x88)?;
    let tail = r.u64(button + 0x90)?;
    let mut previous = 0;
    let mut target = 0;
    let mut seen = BTreeSet::new();
    let mut removed = false;
    for _ in 0..count {
        pointer(link)?;
        if !seen.insert(link) || r.u64(link + 8)? != previous {
            return Err(invalid("Malformed construction listener links"));
        }
        target = r.u64(link)?;
        removed |= r.byte(link + 0x18)? != 0;
        previous = link;
        link = r.u64(link + 0x10)?;
    }
    if link != 0 || previous != tail || (count == 0 && tail != 0) {
        return Err(invalid("Construction listener endpoints disagree"));
    }
    let (offset, primary, manager, invoker, member) = match kind {
        Kind::Opener => (0x21f0, 0x426c518, 0x1886560, 0x1886530, 0x186d470),
        Kind::Choice => (0x28, 0x43104c0, 0x297df40, 0x297df10, 0x296f810),
    };
    let glue = owner + offset;
    if count != 1 || target != glue || removed {
        out["reason"] = json!("exact_single_active_listener_required");
        return Ok(out);
    }
    if r.u64(glue)? != base + primary
        || r.u64(glue + 0x18)? != base + manager
        || r.u64(glue + 0x20)? != base + invoker
    {
        out["reason"] = json!("first_void_tuple_unavailable");
        return Ok(out);
    }
    // Includes manager+278: kind9 can otherwise cancel the selected callback.
    let empty = r.read(glue + 0x28, 0x260)?.iter().all(|v| *v == 0);
    out["inactive_bundle_zero"] = json!(empty);
    if !empty {
        out["reason"] = json!("alternate_callback_bundle");
        return Ok(out);
    }
    let capture = r.u64(glue + 8)?;
    pointer(capture)?;
    if r.u64(capture + 0x10)? != owner {
        return Err(invalid(
            "Construction callback must capture exact own object",
        ));
    }
    if r.u64(capture)? != base + member || r.u64(capture + 8)? != 0 {
        out["reason"] = json!("callback_member_unavailable");
        return Ok(out);
    }
    out["verified"] = json!(true);
    out["reason"] = json!("exact_first_void_binding");
    Ok(out)
}

fn metadata(r: &Reader<'_>, reuse: &mut Reuse, object: u64) -> Result<Value> {
    match reuse.nullable_type(r, object) {
        Ok(value) => Ok(value),
        Err(error) if error.code == "invalid_gui_type" => Ok(Value::Null),
        Err(error) => Err(error),
    }
}

#[allow(clippy::too_many_arguments)]
pub(super) fn control(
    r: &Reader<'_>,
    base: u64,
    owner: u64,
    window: u64,
    button: u64,
    registry: u64,
    kind: Kind,
    geometry: bool,
) -> Result<Value> {
    let mut out = unavailable("standard_layout_unavailable");
    pointer(button)?;
    let mut reuse = Reuse::new(base, 0); // Ordinary hierarchy bounds, not Focus's exception.
    let primary = r.u64(button)?;
    out["control_type_encoding"] = if primary == base + 0x43470c8 {
        json!("15CButtonStandard")
    } else {
        metadata(r, &mut reuse, button)?
    };
    if primary != base + 0x43470c8 {
        return Ok(out);
    }
    if r.u64(button + 0x68)? != registry {
        return Err(invalid("Construction Standard must belong to own registry"));
    }
    let flags = u32::from_le_bytes(r.read(button + 0x74, 4)?.try_into().unwrap());
    let state = r.i32(button + 0xac)?;
    let optional = r.u64(button + 0x1a8)? == 0;
    let blocked = r.byte(button + 0x201)?;
    let delegate = r.u64(button + 0xd0)? == 0;
    let callback = callback(r, base, owner, button, kind)?;
    out["callback"] = callback;
    out["local"] = json!({"state_ac_raw":state,"flags74_raw":flags,
        "optional_gate_1a8_null":optional,"block201_raw":blocked,"delegate_d0_null":delegate,
        "event_mask_d8_raw":r.i32(button+0xd8)?,"shortcut_dc_raw":r.byte(button+0xdc)?,
        "conservative_dispatch_ready":matches!(state,0|2) && flags&0xc00==0x800 && optional && blocked==0 && delegate});
    if out["callback"]["verified"] != true {
        out["reason"] = json!("callback_binding_unavailable");
        return Ok(out);
    }
    out["available"] = json!(true);
    out["reason"] = json!("named_own_standard_and_callback");
    if geometry {
        let renderer = r.u64(button + 0xe8)?;
        if renderer == 0 {
            out["render_diagnostic"] = unavailable("renderer_unmaterialized");
            return Ok(out);
        }
        pointer(renderer)?;
        let primary = r.u64(renderer)?;
        out["renderer_type_encoding"] = metadata(r, &mut reuse, renderer)?;
        if ![0x4356e08, 0x43593d8, 0x434b370]
            .iter()
            .any(|p| primary == base + p)
        {
            out["render_diagnostic"] = unavailable("renderer_layout_unavailable");
            return Ok(out);
        }
        if r.u64(renderer + 0x138)? != button {
            return Err(invalid(
                "Construction renderer must belong to exact Standard",
            ));
        }
        out["renderer_control_join_verified"] = json!(true);
        out["render_diagnostic"] = crate::focus_geometry::observe(
            r, &mut reuse, window, button, registry, primary, renderer,
        )?;
    }
    Ok(out)
}
