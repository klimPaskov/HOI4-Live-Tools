//! Selected own construction row text; independently recovered Linux layout.
//! Fresh handle/location joins authorize reads, never UI freshness or input.
use super::{invalid, members, pointer};
use crate::{error::Result, memory::Reader};
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet};

fn exact(r: &Reader<'_>, address: u64, primary: u64) -> Result<()> {
    pointer(address)?;
    if r.u64(address)? != primary {
        return Err(invalid("Exact construction UI primary required"));
    }
    Ok(())
}
fn window(r: &Reader<'_>, base: u64, window: u64, registry: u64) -> Result<Value> {
    exact(r, window, base + 0x4347d58)?;
    if r.u64(window + 0x98)? != registry {
        return Err(invalid("Construction window must belong to own registry"));
    }
    let flags = r.byte(window + 0xa5)?;
    let states = [r.i32(window + 0xeb0)?, r.i32(window + 0xef8)?];
    Ok(
        json!({"visible_predicate":flags&8!=0 && states.iter().all(|s|*s!=2),
        "flags_raw":flags,"states_raw":states}),
    )
}
fn location_text(r: &Reader<'_>, base: u64, window: u64, registry: u64) -> Result<Option<String>> {
    // Exact CContainerWindow's instant-text lookup, not button or child lookup.
    let values = members(r, window + 0xa60, 256)?;
    let (header, names, count) = r.array_header(window + 0xa78)?;
    let capacity = i32::from_le_bytes(header[8..12].try_into().unwrap());
    if capacity > 256 || count != values.len() || (count != 0 && names % 8 != 0) {
        return Err(invalid("Paired construction text names/pointers required"));
    }
    let mut selected = None;
    let mut seen_names = BTreeSet::new();
    for (index, value) in values.into_iter().enumerate() {
        let name = r.string_bounded(names + index as u64 * 32, 128)?;
        if !seen_names.insert(name.clone()) {
            return Err(invalid("Unique construction text registry names required"));
        }
        if name == "location" && selected.replace(value).is_some() {
            return Err(invalid("Unique named construction location required"));
        }
    }
    let Some(textbox) = selected else {
        return Ok(None);
    };
    exact(r, textbox, base + 0x434c8a0)?;
    if r.u64(textbox + 0x68)? != registry {
        return Err(invalid("Construction text must belong to own registry"));
    }
    Ok(Some(r.string_bounded(textbox + 0x110, 512)?))
}
pub(super) fn observe(r: &Reader<'_>, base: u64, lines: &[Value]) -> Result<Value> {
    // Caller already proved positive own player and own production. Exact idler
    // virtual+c0 reads that positive player; no observer-country fallback here.
    let idler = r.u64(base + 0x44085f8)?;
    exact(r, idler, base + 0x42c9a88)?;
    let registry = r.u64(idler + 0x4f8)?;
    exact(r, registry, base + 0x42736f8)?;
    let manager = r.u64(idler + 0x6b8)?;
    pointer(manager)?;
    if r.u64(manager + 0x180)? != idler {
        return Err(invalid(
            "Construction interface manager must belong to own idler",
        ));
    }
    let selection = super::selection::observe(r, base, manager)?;
    let panel = r.u64(manager + 0x150)?;
    let mut out = json!({"requested":true,"available":false,"reason":"panel_unmaterialized",
        "scope":"own_located_building_rows_only","rows":[],"selection":selection,
        "row_coverage":"not_checked",
        "display_text_freshness_verified":false,"canonical_name_agreement_verified":false,
        "global_modal_clear_verified":false,"effective_routing_verified":false,
        "input_authorized":false,"live_validation":"pending"});
    if panel == 0 {
        return Ok(out);
    }
    exact(r, panel, base + 0x43101d8)?;
    if r.u64(panel + 0x2f0)? != idler {
        return Err(invalid("Construction panel must belong to own idler"));
    }
    let root = r.u64(panel + 0x2f8)?;
    if root == 0 {
        out["reason"] = json!("root_unmaterialized");
        return Ok(out);
    }
    let root_state = window(r, base, root, registry)?;
    let visible = root_state["visible_predicate"] == true;
    out["root"] = root_state;
    if !visible {
        out["reason"] = json!("root_not_locally_visible");
        return Ok(out);
    }
    let own: BTreeMap<_, _> = lines
        .iter()
        .filter(|line| line["line_kind"] == "building" && line["location_available"] == true)
        .map(|line| {
            (
                line["line_handle"].as_str().expect("producer full handle"),
                line,
            )
        })
        .collect();
    let items = members(r, panel + 0x308, 2048)?;
    let mut rows = Vec::new();
    let mut seen = BTreeSet::new();
    let mut unsupported = 0;
    let mut unjoined = 0;
    for item in &items {
        if r.u64(*item)? != base + 0x4265548 {
            unsupported += 1;
            continue;
        }
        if r.u64(item + 0x538)? != registry {
            return Err(invalid("Construction row must belong to own registry"));
        }
        let handle = r.u64(item + 0x540)?;
        if handle != r.u64(item + 0xce8)? {
            return Err(invalid("Construction UI full handle copies disagree"));
        }
        let key = format!("0x{handle:016x}");
        let Some(line) = own.get(key.as_str()) else {
            unjoined += 1;
            continue;
        };
        if !seen.insert(handle) {
            return Err(invalid("Duplicate own construction UI handle"));
        }
        // No window or text payload until exact own handle AND location joins.
        let container = r.u64(item + 0x10)?;
        let local = window(r, base, container, registry)?;
        let text = if local["visible_predicate"] == true {
            location_text(r, base, container, registry)?
        } else {
            None
        };
        rows.push(json!({"entry_index":line["entry_index"],"line_handle":key,
            "building_id":line["building"]["building_id"],"location":line["location"],
            "window":local,"displayed_location_text":text,"input_authorized":false}));
    }
    out["available"] = json!(true);
    out["reason"] = json!(if rows.is_empty() {
        "ui_collection_checked_no_own_located_rows"
    } else {
        "own_handle_joined_ui_observation"
    });
    out["row_coverage"] = json!("full UI collection checked before queue-index pagination");
    out["total_ui_row_count"] = json!(items.len());
    out["joined_row_count"] = json!(rows.len());
    out["unsupported_row_count"] = json!(unsupported);
    out["unjoined_row_count"] = json!(unjoined);
    out["rows"] = json!(rows);
    Ok(out)
}
