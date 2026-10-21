//! Same-snapshot own production identity and exact Linux ordinary UI controls.
//! This observer never opens the panel, moves the pointer or grants input.
use super::*;
use crate::memory::StringAbi;

/// Accept only the exact opaque representation emitted by production_lines.
pub fn validate_target(handle: &str, equipment_id: &str) -> Result<u64> {
    let parsed = handle
        .strip_prefix("0x")
        .filter(|s| {
            s.len() == 16
                && s.bytes()
                    .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        })
        .and_then(|s| u64::from_str_radix(s, 16).ok())
        .filter(|n| matches!(*n as u32, 0x38 | 0x39));
    if equipment_id.is_empty()
        || equipment_id.len() > 128
        || !equipment_id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_')
    {
        return Err(Error::new(
            "invalid_arguments",
            "Canonical equipment_id required",
        ));
    }
    parsed.ok_or_else(|| {
        Error::new(
            "invalid_arguments",
            "Exact full production line_handle required",
        )
    })
}

pub fn tool() -> Value {
    json!({"name":"hoi4_production_controls",
        "description":"Linux read-only preview: joins an exact own current production line_handle and base equipment_id to its production row, named one-factory add/subtract controls, ordinary callback and local blockers. Optional same-snapshot renderer geometry. Never opens a panel, moves the cursor, pauses or authorizes input. Full line collection is validated before reading UI. Factory buttons clamp to at least one and depend on keyboard modifiers. Live validation pending.",
        "inputSchema":{"type":"object","properties":{
            "line_handle":{"type":"string","pattern":"^0x[0-9a-f]{16}$"},
            "equipment_id":{"type":"string","minLength":1,"maxLength":128},
            "include_geometry":{"type":"boolean","default":false}},
            "required":["line_handle","equipment_id"],"additionalProperties":false},
        "annotations":{"readOnlyHint":true}})
}

pub fn snapshot(process: &dyn Process, handle: &str, key: &str, geometry: bool) -> Result<Value> {
    let handle = validate_target(handle, key)?;
    let start = Instant::now();
    process.begin_snapshot()?;
    let witnesses = Witnesses::new(process);
    let reader = Reader::new(&witnesses);
    if reader.string_abi() != StringAbi::LibStdCxx {
        return Err(Error::new(
            "capability_unavailable",
            "Production control layout requires the reviewed Linux build",
        ));
    }
    let base = process.image_base();
    let (_, production) = economy::own_production(&reader, base)?;
    let campaign = game::context_from_reader(&reader, base)?;
    let collection = collection(&reader, base, production)?;
    let selected = collection
        .known
        .iter()
        .find(|line| line.handle == handle)
        .ok_or_else(|| {
            Error::new(
                "production_target_unavailable",
                "Full handle absent from current own equipment lines",
            )
        })?;
    if selected.equipment_id.as_deref() != Some(key) || !selected.counters_valid {
        return Err(Error::new(
            "production_target_unavailable",
            "Own line requires matching canonical equipment and bounded counters",
        ));
    }
    let target = TargetLine {
        address: selected.address,
        handle,
        naval: selected.naval,
        equipment_id: key.to_owned(),
        requested: selected.requested,
    };
    let observation = controls::observe(&reader, base, &target, geometry)?;
    let mut value = observation.value;
    // Internal scope is intentionally never serialized as process addresses.
    let _scope = observation.scope;
    if geometry {
        value["input_geometry"] = crate::gui::input_geometry_from_reader(&reader, base)?;
    }
    let (end_calls, end_bytes) = witnesses.finish()?;
    process.check()?;
    value["source"] = json!("native own production controls");
    value["build_id"] = json!(game::layout(&reader).build_id);
    value["player"] = json!(campaign.player);
    value["playthrough_id"] = json!(campaign.playthrough_id);
    value["date_raw"] = json!(campaign.date_raw);
    value["line_handle"] = json!(format!("0x{handle:016x}"));
    value["equipment_id"] = json!(key);
    value["requested_factories"] = json!(selected.requested);
    value["line_kind"] = json!(if selected.naval { "naval" } else { "military" });
    value["collection_count"] = json!(collection.entries.len());
    value["input_authorized"] = json!(false);
    value["observation_paused_game"] = json!(false);
    value["live_validation"] = json!("pending");
    value["freshness"] = json!({"elapsed_ms":start.elapsed().as_secs_f64()*1000.0,
        "read_calls_kind":"logical_read_requests", "read_calls":reader.calls.get()+end_calls as u64,
        "read_bytes":reader.bytes.get()+end_bytes});
    Ok(value)
}
