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
    Ok(observe(process, handle, key, Some(geometry))?.value)
}
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
pub(crate) struct Observation {
    pub(crate) value: Value,
    pub(crate) campaign: game::ClockContext,
    pub(crate) line: crate::production_step::LineIdentity,
    pub(crate) requested: i32,
    pub(crate) scope: Vec<u64>,
    pub(crate) geometry: std::collections::BTreeMap<String, Vec<(u64, Vec<u8>)>>,
}
/// None requests canonical own-line readback without accessing any GUI object.
pub(crate) fn observe(
    process: &dyn Process,
    handle: &str,
    key: &str,
    ui: Option<bool>,
) -> Result<Observation> {
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
    let (country, production) = economy::own_production(&reader, base)?;
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
    // Reuse the positively validated equipment join. Re-reading line+88 and
    // following a newly changed pointer here would traverse an unproved object
    // before the final witness could reject the replacement.
    let [variant, base_type] = selected.equipment_identity.ok_or_else(|| {
        Error::new(
            "production_target_unavailable",
            "Exact own equipment identity unavailable",
        )
    })?;
    let line = crate::production_step::LineIdentity {
        country,
        production,
        line: selected.address,
        variant,
        base_type,
        handle,
        equipment_id: key.into(),
    };
    let (mut value, mut scope, geometry) = match ui {
        Some(include) => {
            let o = controls::observe(&reader, base, &target, include)?;
            (o.value, o.scope, o.geometry)
        }
        None => (json!({}), Vec::new(), std::collections::BTreeMap::new()),
    };
    if ui == Some(true) {
        let idler = campaign.anchors[1];
        let registry = reader.u64(idler + 0x4f8)?;
        let manager = reader.u64(idler + 0x6b8)?;
        if reader.u64(base + 0x45778e0)? != registry {
            return Err(Error::new(
                "invalid_production_controls",
                "Input GUI must equal own production registry",
            ));
        }
        value["common_blockers"] = crate::construction::controls::blockers(
            &reader, base, country, idler, manager, registry,
        )?;
        value["engine_gates"] = crate::production_panel::engine(
            &reader,
            base,
            campaign.anchors[0],
            idler,
            manager,
            registry,
            &mut scope,
        )?;
        value["common_gui_gates_clear"] = json!(
            reader.byte(registry + 0x150)? == 0
                && reader.byte(registry + 0x12)? == 0
                && reader.u64(registry + 0x158)? == 0
                && reader.u64(registry + 0x230)? == 0
        );
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
    Ok(Observation {
        value,
        campaign,
        line,
        requested: selected.requested,
        scope,
        geometry,
    })
}
