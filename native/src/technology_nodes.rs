//! Independently recovered Linux current-node observation. No input authority.
// Canonical own technology joins follow research.rs/Clausewitz-MCP calling/game.py.
// Copyright (c) 2026 smerss, MIT; see ../LICENSE.clausewitz.
use crate::{
    error::{Error, Result},
    memory::Reader,
    research_ui::{array, exact, nullable_type},
};
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet};

/// Caller must use its fresh Witnesses Reader and call Witnesses::finish after
/// this helper; it must already establish own mode6, displayed tag and slot.
/// registry is contextual only: unknown item GUI types provide no owner layout.
pub(crate) fn current_nodes(
    r: &Reader<'_>,
    base: u64,
    tree: u64,
    own_status: u64,
    _registry: u64,
) -> Result<Value> {
    exact(r, tree, base + 0x424de40)?;
    exact(r, own_status, base + 0x42108f8)?;
    let mut records = Vec::new();
    // Current inline records, NOT the embedded callback objects at 5e0/868.
    for offset in [0xdd8, 0xdf0] {
        let (header, data, count) = r.array_header(tree + offset)?;
        let cap = i32::from_le_bytes(header[8..12].try_into().unwrap()) as usize;
        if cap > 2048 || (count != 0 && data % 8 != 0) || records.len() + count > 512 {
            return Err(Error::new(
                "invalid_array",
                "Current-node aggregate bound exceeded",
            ));
        }
        if count != 0 {
            let payload = r.read(data, count * 40)?;
            records.extend(
                payload
                    .chunks_exact(40)
                    .map(|entry| u64::from_le_bytes(entry[32..40].try_into().unwrap())),
            );
        }
    }
    let mut items = BTreeSet::new();
    let mut ids: BTreeMap<i32, Value> = BTreeMap::new();
    let mut catalog = None;
    let mut public_keys = BTreeMap::new();
    for item in records.iter().copied() {
        if !items.insert(item) {
            continue;
        }
        exact(r, item, base + 0x424e2e0)?;
        if r.u64(item + 0x30)? != tree {
            return Err(Error::new(
                "invalid_gui_owner",
                "Technology item must belong to own tree",
            ));
        }
        let technology = r.u64(item + 0xc8)?;
        exact(r, technology, base + 0x42107f8)?;
        // Foreign ownership refused before reading template or GUI payload.
        if r.u64(technology + 0x158)? != own_status {
            return Err(Error::new(
                "invalid_research_owner",
                "Node technology must belong to own status",
            ));
        }
        if catalog.is_none() {
            let db = r.u64(base + 0x439f450)?;
            exact(r, db, base + 0x42110c8)?;
            let templates = array(r, db + 0x48, 8192)?;
            let own = array(r, own_status + 0x88, 8192)?;
            if templates.is_empty() || own.len() != templates.len() - 1 {
                return Err(Error::new(
                    "research_unavailable",
                    "Initialized own catalog required",
                ));
            }
            catalog = Some((templates, own));
        }
        let template = r.u64(technology + 0x160)?;
        exact(r, template, base + 0x4211038)?;
        let id = r.i32(template + 0x30)?;
        let (templates, own) = catalog.as_ref().unwrap();
        if id <= 0
            || templates.get(id as usize) != Some(&template)
            || own.get(id as usize - 1) != Some(&technology)
        {
            return Err(Error::new(
                "invalid_research_join",
                "Canonical own node join required",
            ));
        }
        let key = r.string_bounded(template + 8, 256)?;
        if key.is_empty() || key.chars().any(char::is_control) {
            return Err(Error::new(
                "invalid_research_id",
                "Bounded public node technology key required",
            ));
        }
        if public_keys.get(&key).is_some_and(|prior| *prior != id) {
            return Err(Error::new(
                "invalid_research_id",
                "Unique public node keys required",
            ));
        }
        public_keys.insert(key.clone(), id);
        let gui =
            json!({"id": key, "gui_type_encoding": nullable_type(r, base, r.u64(item + 0x10)?)?});
        if let Some(previous) = ids.get(&id) {
            if previous != &gui {
                return Err(Error::new(
                    "ambiguous_gui_node",
                    "Repeated canonical ID has conflicting GUI types",
                ));
            }
        } else {
            ids.insert(id, gui);
        }
    }
    let nodes: Vec<_> = ids
        .into_iter()
        .map(|(id, gui)| {
            json!({
                "id": gui["id"], "definition_index": id, "gui_type_encoding": gui["gui_type_encoding"], "visibility":"unavailable",
                "eligibility":"unavailable", "input_authorized":false
            })
        })
        .collect();
    Ok(
        json!({"record_count":records.len(), "node_count":nodes.len(), "nodes":nodes,
        "effective_hit_routing_verified":false,"input_authorized":false}),
    )
}

#[cfg(test)]
#[path = "technology_nodes/tests.rs"]
mod tests;
