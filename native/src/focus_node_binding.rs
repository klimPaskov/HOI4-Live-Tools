//! Exact own focus current item association, no input or guessed research layout.
use crate::{
    error::{Error, Result},
    memory::Reader,
    node_reuse::Reuse,
};
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet};
fn exact(r: &Reader<'_>, p: u64, t: u64) -> Result<()> {
    if p == 0 || p % 8 != 0 || r.u64(p)? != t {
        Err(Error::new(
            "invalid_focus_node",
            "Exact aligned focus item/control required",
        ))
    } else {
        Ok(())
    }
}
fn pointers(r: &Reader<'_>, a: u64, limit: usize) -> Result<Vec<u64>> {
    let (h, p, n) = r.array_header(a)?;
    let cap = i32::from_le_bytes(h[8..12].try_into().unwrap()) as usize;
    if cap > limit || (n > 0 && p % 8 != 0) {
        return Err(Error::new(
            "invalid_focus_node",
            "Bounded focus item array required",
        ));
    }
    let b = if n == 0 { vec![] } else { r.read(p, n * 8)? };
    Ok(b.chunks_exact(8)
        .map(|x| u64::from_le_bytes(x.try_into().unwrap()))
        .collect())
}
pub(crate) fn observe(
    r: &Reader<'_>,
    base: u64,
    view: u64,
    registry: u64,
    keys: &BTreeMap<u64, String>,
    key: &str,
) -> Result<Value> {
    let definition = keys
        .iter()
        .find_map(|(p, k)| (k == key).then_some(*p))
        .ok_or_else(|| {
            Error::new(
                "focus_unavailable",
                "Exact own canonical focus key required",
            )
        })?;
    let items = pointers(r, view + 0x1138, 4096)?;
    let mut seen = BTreeSet::new();
    let mut matches = vec![];
    // Complete current records before ANY requested GUI payload. No early return.
    for item in &items {
        if !seen.insert(*item) {
            return Err(Error::new(
                "invalid_focus_node",
                "Duplicate current focus item",
            ));
        }
        exact(r, *item, base + 0x4249b90)?;
        if r.u64(item + 0x2b0)? != view {
            return Err(Error::new(
                "foreign_focus_scope",
                "Current focus item must belong to own view",
            ));
        }
        let focus = r.u64(item + 0x20)?;
        if !keys.contains_key(&focus) {
            return Err(Error::new(
                "invalid_focus_node",
                "Current item outside own canonical catalog",
            ));
        }
        if focus == definition {
            matches.push(*item);
        }
    }
    if matches.len() > 1 {
        return Err(Error::new(
            "ambiguous_focus_node",
            "Multiple current items for requested focus",
        ));
    }
    let mut out = json!({"scope":"exact_requested_own_focus_current_item","requested_key":key,"current_item_count":items.len(),"materialized":false,"controls":[],"selection_readback":"fresh_own_detail_selected_focus_equals_requested_canonical_focus","numeric_id_available":false,"input_authorized":false,"effective_routing_verified":false,"eligibility_available":false,"client_rectangle_available":false});
    let Some(&item) = matches.first() else {
        return Ok(out);
    };
    out["materialized"] = json!(true);
    let window = r.u64(item + 0x10)?;
    exact(r, window, base + 0x4347d58)?;
    if r.u64(window + 0x98)? != registry {
        return Err(Error::new(
            "invalid_focus_owner",
            "Focus item container must belong to own registry",
        ));
    }
    let children = pointers(r, window + 0x9a0, 256)?;
    let (h, names, count) = r.array_header(window + 0x9b8)?;
    let cap = i32::from_le_bytes(h[8..12].try_into().unwrap()) as usize;
    if cap > 256 || count != children.len() || (count > 0 && names % 8 != 0) {
        return Err(Error::new(
            "invalid_focus_node",
            "Paired current focus controls required",
        ));
    }
    let mut seen_names = BTreeSet::new();
    let mut controls = vec![];
    let mut reuse = Reuse::new(base, 0);
    for (i, child) in children.iter().enumerate() {
        let name = r.string_bounded(names + i as u64 * 32, 256)?;
        if !seen_names.insert(name.clone()) {
            return Err(Error::new(
                "invalid_focus_node",
                "Duplicate focus item control name",
            ));
        }
        if name != "bg" && name != "symbol" {
            continue;
        }
        let mut c = json!({"name":name,"binding_verified":false,"type_encoding":null,"reason":"control_layout_unavailable","input_authorized":false});
        if *child == 0 {
            controls.push(c);
            continue;
        }
        c["type_encoding"] = metadata(r, &mut reuse, *child)?;
        if r.u64(*child)? != base + 0x43470c8 {
            controls.push(c);
            continue;
        }
        if r.u64(child + 0x68)? != registry {
            return Err(Error::new(
                "invalid_focus_owner",
                "Focus Standard must belong to own registry",
            ));
        }
        if name == "symbol" && r.u64(item + 0x2b8)? != *child {
            return Err(Error::new("invalid_focus_node", "Symbol alias mismatch"));
        }
        let ok = callback(r, base, item, *child)?;
        c["binding_verified"] = json!(ok);
        if ok {
            c["reason"] = json!("ordinary_selection_void_and_ui_state_callback_verified");
            c["disabled_state3"] = json!(r.i32(child + 0xac)? == 3);
            let render = r.u64(child + 0xe8)?;
            c["renderer_type_encoding"] = metadata(r, &mut reuse, render)?;
            let primary = if render == 0 { 0 } else { r.u64(render)? };
            if render != 0
                && [0x4356e08, 0x434b370, 0x434c5b0, 0x43593d8]
                    .iter()
                    .any(|p| primary == base + p)
            {
                if r.u64(render + 0x138)? != *child {
                    return Err(Error::new(
                        "invalid_focus_owner",
                        "Focus renderer control mismatch",
                    ));
                }
                c["renderer_control_join_verified"] = json!(true);
            }
        }
        controls.push(c);
    }
    out["controls"] = json!(controls);
    Ok(out)
}
fn metadata(r: &Reader<'_>, reuse: &mut Reuse, p: u64) -> Result<Value> {
    match reuse.nullable_type(r, p) {
        Ok(v) => Ok(v),
        Err(e) if e.code == "invalid_gui_type" => Ok(Value::Null),
        Err(e) => Err(e),
    }
}
fn callback(r: &Reader<'_>, base: u64, item: u64, button: u64) -> Result<bool> {
    if r.u64(button + 0x80)? != base + 0x4347488 || r.u64(button + 0xa0)? != 0 {
        return Ok(false);
    }
    let count = r.i32(button + 0x98)?;
    if !(0..=16).contains(&count) {
        return Err(Error::new("invalid_focus_node", "Listener bound"));
    }
    let mut node = r.u64(button + 0x88)?;
    let tail = r.u64(button + 0x90)?;
    let mut prior = 0;
    let mut seen = BTreeSet::new();
    let mut targets = vec![];
    for _ in 0..count {
        if node == 0
            || node % 8 != 0
            || !seen.insert(node)
            || r.u64(node + 8)? != prior
            || r.byte(node + 0x18)? != 0
        {
            return Err(Error::new(
                "invalid_focus_node",
                "Malformed focus selection listener",
            ));
        }
        targets.push(r.u64(node)?);
        prior = node;
        node = r.u64(node + 0x10)?;
    }
    if node != 0 || prior != tail || (count == 0 && tail != 0) {
        return Err(Error::new(
            "invalid_focus_node",
            "Focus selection terminal mismatch",
        ));
    }
    if targets != vec![item + 0x28] {
        return Ok(false);
    }
    let glue = item + 0x28;
    if r.u64(glue)? != base + 0x424a860 {
        return Ok(false);
    }
    // Exact constructor first VOID +8 and UI-state VOID +148, distinct roles.
    for (off, member) in [(8, 0x150b440), (0x148, 0x150b5f0)] {
        if r.u64(glue + off + 0x10)? != base + 0x1527740
            || r.u64(glue + off + 0x18)? != base + 0x1527710
        {
            return Ok(false);
        }
        let capture = r.u64(glue + off)?;
        if capture == 0 || capture % 8 != 0 {
            return Err(Error::new("invalid_focus_node", "Focus capture alignment"));
        }
        if r.u64(capture + 0x10)? != item {
            return Err(Error::new(
                "invalid_focus_owner",
                "Focus callback must capture own item",
            ));
        }
        if r.u64(capture)? != base + member || r.u64(capture + 8)? != 0 {
            return Ok(false);
        }
    }
    for slot in [
        0xd8, 0x118, 0x218, 0x98, 0x38, 0x78, 0xb8, 0x1b8, 0x238, 0x258,
    ] {
        if r.u64(glue + slot)? != 0 {
            return Ok(false);
        }
    }
    Ok(true)
}
