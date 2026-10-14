//! Own equipment-line observations for the verified Linux build only.
//! Independently recovered constructors/save/handle/keyword witnesses:
//! production-lines-v1, ELF a4971c9e8ac9a6f2487dea109e01f9839ee59db7.
//! No equipment traversal, UI/callback/command, allocation or input authority.
use crate::{
    economy,
    error::{Error, Result},
    game,
    memory::Reader,
    platform::Process,
    research_ui::{primary_type_encoding, Witnesses},
};
use serde_json::{json, Value};
use std::{
    collections::BTreeSet,
    time::{Instant, SystemTime, UNIX_EPOCH},
};
const LINE_LIMIT: usize = 2048;
const PAGE_LIMIT: usize = 128;
const COUNTER_LIMIT: i32 = 1_000_000;
#[derive(Clone, Debug, PartialEq, Eq)]
#[doc(hidden)]
pub struct ProductionContext {
    pub campaign: game::ClockContext,
    pub own: [u64; 2],
    /// Collection members only; unsupported subtype owner layouts are unproved.
    pub entries: Vec<u64>,
}
pub fn snapshot(process: &dyn Process) -> Result<Value> {
    page(process, 0, 32)
}
pub fn page(process: &dyn Process, offset: usize, limit: usize) -> Result<Value> {
    Ok(observe(process, offset, limit)?.0)
}
/// Fresh own status/collection context, not a cross-load line identity proof.
#[doc(hidden)]
pub fn owned_snapshot(process: &dyn Process) -> Result<(Value, ProductionContext)> {
    observe(process, 0, 32)
}
fn observe(
    process: &dyn Process,
    offset: usize,
    limit: usize,
) -> Result<(Value, ProductionContext)> {
    if offset > LINE_LIMIT || !(1..=PAGE_LIMIT).contains(&limit) {
        return Err(Error::new(
            "invalid_arguments",
            "Equipment-line page exceeds bounds",
        ));
    }
    let start = Instant::now();
    process.begin_snapshot()?;
    let witnesses = Witnesses::new(process);
    let reader = Reader::new(&witnesses);
    let base = process.image_base();
    let (country, production) = economy::own_production(&reader, base)?;
    let campaign = game::context_from_reader(&reader, base)?;
    let (header, data, count) = reader.array_header(production + 0x58)?;
    let capacity = i32::from_le_bytes(header[8..12].try_into().unwrap()) as usize;
    if capacity > LINE_LIMIT || (count > 0 && data % 8 != 0) {
        return Err(Error::new(
            "invalid_production_array",
            "Bounded aligned equipment-line array required",
        ));
    }
    let payload = if count == 0 {
        Vec::new()
    } else {
        reader.read(data, count * 8)?
    };
    let entries: Vec<u64> = payload
        .chunks_exact(8)
        .map(|bytes| u64::from_le_bytes(bytes.try_into().unwrap()))
        .collect();
    let mut pointers = BTreeSet::new();
    for &line in &entries {
        if !(0x10000..=0x7fff_ffff_0000).contains(&line) || line % 8 != 0 || !pointers.insert(line)
        {
            return Err(Error::new(
                "invalid_production_array",
                "Unique bounded aligned equipment-line pointers required",
            ));
        }
    }
    let mut lines = Vec::new();
    let mut unsupported = Vec::new();
    let mut handles = BTreeSet::new();
    let mut anomalous = 0;
    for (index, &line) in entries.iter().enumerate() {
        let primary = reader.u64(line)?;
        let (kind, expected_word) = if primary == base + 0x42ede08 {
            ("military", 0x38u32)
        } else if primary == base + 0x42edf88 {
            ("naval", 0x39u32)
        } else {
            let metadata = match primary_type_encoding(&reader, base, primary) {
                Ok(name) => Some(name),
                Err(error) if error.code == "invalid_gui_type" => None,
                Err(error) => return Err(error),
            };
            unsupported.push(json!({"entry_index":index,"type_encoding":metadata,"ownership_verified":false,
                    "field_layout_available":false,"owner_layout_available":false,"input_authorized":false}));
            continue;
        };
        // The constructor/loader and reciprocal serializer prove status20.
        // Never read handle, counters or equipment before this owner guard.
        if reader.u64(line + 0x20)? != production {
            return Err(Error::new(
                "invalid_production_owner",
                "Equipment line must belong to own production status",
            ));
        }
        let handle = reader.u64(line + 8)?;
        if handle == 0 || handle as u32 != expected_word || !handles.insert(handle) {
            return Err(Error::new(
                "invalid_production_handle",
                "Unique full typed equipment-line handle required",
            ));
        }
        let active = reader.i32(line + 0x14)?;
        let damaged = reader.i32(line + 0x18)?;
        let requested = reader.i32(line + 0xe8)?;
        let within_policy = [active, damaged, requested]
            .iter()
            .all(|n| (0..=COUNTER_LIMIT).contains(n));
        if !within_policy {
            anomalous += 1;
        }
        lines.push(json!({"entry_index":index,"line_kind":kind,
            "line_handle":format!("0x{handle:016x}"),
            "handle_scope":"own_current_snapshot_collection",
            "equipment_id":null,
            "factory_counters":{"active_factories_raw_signed":active,"damaged_factories_raw_signed":damaged,
                "requested_factories_raw_signed":requested,"available_under_reviewed_policy":within_policy,
                "unavailable_reason":if within_policy {Value::Null} else {json!("anomalous_counter_policy")}},
            "input_authorized":false}));
    }
    let (end_calls, end_bytes) = witnesses.finish()?;
    process.check()?;
    let supported_count = lines.len();
    let unsupported_count = unsupported.len();
    let end = count.min(offset + limit);
    // Page indices refer only to this fresh collection. Validate and witness the
    // entire collection above before exposing any subset, including metadata.
    for rows in [&mut lines, &mut unsupported] {
        rows.retain(|row| {
            row["entry_index"]
                .as_u64()
                .is_some_and(|index| (offset as u64..end as u64).contains(&index))
        });
    }
    let context = ProductionContext {
        campaign,
        own: [country, production],
        entries,
    };
    let value = json!({"source":"native own equipment production lines","build_id":game::BUILD_ID,
        "scope":"own_equipment_line_collection_only","owned":true,"player":context.campaign.player,
        "handle_scope":{"kind":"own_current_snapshot_collection","player":context.campaign.player,
            "playthrough_id":context.campaign.playthrough_id,"date_raw":context.campaign.date_raw,
            "process_epoch":process.identity().map(|id|json!({"pid":id.pid,"start_ticks":id.start_ticks,"uid":id.uid})),
            "cross_load_identity_available":false},
        "collection_count":count,"total_count":count,"known_line_count":supported_count,"supported_count":supported_count,
        "unsupported_count":unsupported_count,"complete_catalog":unsupported_count==0,
        "catalog_coverage":"all_types_in_scoped_equipment_line_collection",
        "offset":offset,"limit":limit,"page_count":lines.len()+unsupported.len(),
        "next_offset":if end<count {Some(end)} else {None},
        "page_index_scope":"fresh_snapshot_collection_only",
        "anomalous_counter_count":anomalous,"counter_data_complete":unsupported_count==0&&anomalous==0,
        "counter_policy":{"minimum":0,"maximum":COUNTER_LIMIT,"engine_invariant_claimed":false},
        "lines":lines,"unsupported_type_metadata":unsupported,
        "collection_policy":{"maximum_capacity":LINE_LIMIT,"engine_invariant_claimed":false},
        "general_production_collection_available":false,
        "equipment_ids_available":false,"ui_allocation_available":false,"idle_capacity_available":false,"eligibility_available":false,
        "cross_load_identity_available":false,"input_authorized":false,"observation_paused_game":false,
        "freshness":{"observed_unix_ms":SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_millis(),
            "elapsed_ms":start.elapsed().as_secs_f64()*1000.0,"read_calls_kind":"logical_read_requests",
            "read_calls":reader.calls.get()+end_calls as u64,"read_bytes":reader.bytes.get()+end_bytes}});
    Ok((value, context))
}
