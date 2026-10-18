//! Read-only own construction queue for the exact verified Linux ELF.
//! Semantics consulted: smerss/Clausewitz-MCP calling/game.py (MIT).
//! Layout/field names independently recovered from constructors, serializers,
//! keyword initializers and arithmetic; see PROVENANCE.md. No engine calls.
mod buildings;
pub mod controls;
pub mod find_view;
mod location;
mod selection;
mod ui;
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
const RAW_LIMIT: i64 = 100_000_000_000;
const COUNTER_LIMIT: i32 = 1_000_000;

fn invalid(message: &str) -> Error {
    Error::new("invalid_construction", message)
}
fn pointer(p: u64) -> Result<()> {
    if !(0x10000..=0x7fff_ffff_0000).contains(&p) || p % 8 != 0 {
        return Err(invalid("Bounded aligned construction pointer required"));
    }
    Ok(())
}
fn signed(r: &Reader<'_>, address: u64) -> Result<i64> {
    Ok(i64::from_le_bytes(r.read(address, 8)?.try_into().unwrap()))
}
fn boolean(r: &Reader<'_>, address: u64) -> Result<bool> {
    match r.byte(address)? {
        0 => Ok(false),
        1 => Ok(true),
        _ => Err(invalid("Construction boolean outside reviewed encoding")),
    }
}
fn members(r: &Reader<'_>, address: u64, bound: usize) -> Result<Vec<u64>> {
    let (header, data, count) = r.array_header(address)?;
    let capacity = i32::from_le_bytes(header[8..12].try_into().unwrap()) as usize;
    if capacity > bound || (count > 0 && data % 8 != 0) {
        return Err(invalid(
            "Construction array exceeds capacity/alignment policy",
        ));
    }
    let bytes = if count == 0 {
        Vec::new()
    } else {
        r.read(data, count * 8)?
    };
    let mut unique = BTreeSet::new();
    let mut result = Vec::with_capacity(count);
    for bytes in bytes.chunks_exact(8) {
        let p = u64::from_le_bytes(bytes.try_into().unwrap());
        pointer(p)?;
        if !unique.insert(p) {
            return Err(invalid("Unique construction collection pointers required"));
        }
        result.push(p);
    }
    Ok(result)
}

/// All own construction lines inside the CALLER's witnessed bracket.
pub(crate) struct Lines {
    pub(crate) rows: Vec<Value>,
    pub(crate) unsupported: Vec<Value>,
    pub(crate) total: usize,
    pub(crate) building_count: usize,
    pub(crate) unavailable_buildings: usize,
    pub(crate) anomalous: usize,
    pub(crate) locations: usize,
}
/// Construction queue, building and location layout of one inspected build.
/// Queued-line fields (+0x28..+0x99) are shared with the Linux line layout;
/// the macOS base fields through +0x88 were matched live on equipment lines.
#[derive(Debug)]
pub(crate) struct Layout {
    building_line_vtable: u64,
    railway_line_vtable: u64,
    pub(crate) building_vtable: u64,
    pub(crate) building_status: u64,
    pub(crate) building_template: u64,
    pub(crate) database_global: u64,
    pub(crate) database_vtable: u64,
    pub(crate) template_vtable: u64,
    pub(crate) template_key: u64,
    pub(crate) template_ordinal: u64,
    pub(crate) status_vtable: u64,
    pub(crate) state_vtable: u64,
    pub(crate) state_owner: u64,
    pub(crate) state_controller: u64,
    pub(crate) state_id: u64,
    pub(crate) state_status: u64,
    pub(crate) province_vtable: u64,
    pub(crate) province_owner: u64,
    pub(crate) province_state: u64,
    pub(crate) province_id: u64,
    pub(crate) province_status: u64,
    pub(crate) gs_provinces: u64,
    pub(crate) gs_states: u64,
}
/// Linux ELF a4971c9e (libstdc++).
const LINUX: Layout = Layout {
    building_line_vtable: 0x4205680,
    railway_line_vtable: 0x42ee398,
    building_vtable: 0x41b3e60,
    building_status: 0x1b8,
    building_template: 0x1c0,
    database_global: 0x439f400,
    database_vtable: 0x41b3ed8,
    template_vtable: 0x41b49c8,
    template_key: 0x248,
    template_ordinal: 0x2b8,
    status_vtable: 0x41b4768,
    state_vtable: 0x41fba98,
    state_owner: 0xc8,
    state_controller: 0xcc,
    state_id: 0x48,
    state_status: 0x120,
    province_vtable: 0x41fb728,
    province_owner: 0x1a8,
    province_state: 0xe0,
    province_id: 0xa4,
    province_status: 0x1b0,
    gs_provinces: 0x2b0,
    gs_states: 0x2c8,
};
/// macOS Mach-O 089EA519 (libc++), mapped live on 18 October 2026: vtables by
/// RTTI name, the database by its sole global, keys/ordinals by catalog
/// contents, state/province IDs by catalog index and owners by own tag.
const MACOS: Layout = Layout {
    building_line_vtable: 0x3389c40,
    railway_line_vtable: 0x338a7a8,
    building_vtable: 0x332dcb0,
    building_status: 0x1a8,
    building_template: 0x1b0,
    database_global: 0x34d0ac0,
    database_vtable: 0x332dd28,
    template_vtable: 0x332e908,
    template_key: 0x228,
    template_ordinal: 0x290,
    status_vtable: 0x332e6a8,
    state_vtable: 0x337caf0,
    state_owner: 0xc0,
    state_controller: 0xc4,
    state_id: 0x40,
    state_status: 0x110,
    province_vtable: 0x337c7e8,
    province_owner: 0x188,
    province_state: 0xc0,
    province_id: 0x9c,
    province_status: 0x190,
    gs_provinces: 0x278,
    gs_states: 0x290,
};
pub(crate) fn layout(r: &Reader<'_>) -> &'static Layout {
    match r.string_abi() {
        crate::memory::StringAbi::LibStdCxx => &LINUX,
        crate::memory::StringAbi::LibCxx => &MACOS,
    }
}
pub(crate) fn lines_in(
    r: &Reader<'_>,
    base: u64,
    production: u64,
    own_tag: i32,
    include_location: bool,
) -> Result<Lines> {
    let l = layout(r);
    let entries = members(r, production + 0x70, LINE_LIMIT)?;
    let mut rows = Vec::new();
    let mut unsupported = Vec::new();
    let mut handles = BTreeSet::new();
    let mut catalog = None;
    let mut unavailable_buildings = 0;
    let mut building_count = 0;
    let mut anomalous = 0;
    let mut locations = 0;
    for (index, &line) in entries.iter().enumerate() {
        let primary = r.u64(line)?;
        let is_building = primary == base + l.building_line_vtable;
        if !is_building && primary != base + l.railway_line_vtable {
            let name = match primary_type_encoding(r, base, primary) {
                Ok(name) => Some(name),
                Err(error) if error.code == "invalid_gui_type" => None,
                Err(error) => return Err(error),
            };
            unsupported.push(json!({"entry_index":index,"type_encoding":name,
                "ownership_verified":false,"field_layout_available":false,"input_authorized":false}));
            continue;
        }
        // Membership alone does not prove a known line's reciprocal owner.
        if r.u64(line + 0x20)? != production {
            return Err(invalid(
                "Construction line must belong to own production status",
            ));
        }
        // Shared serializer and both allocators prove the full eight bytes.
        // Railway uses a dynamic kind; only buildings corroborate kind 0x3b.
        let handle = r.u64(line + 8)?;
        if handle == 0 || (is_building && handle as u32 != 0x3b) || !handles.insert(handle) {
            return Err(invalid("Unique nonzero full construction handle required"));
        }
        let active = r.i32(line + 0x14)?;
        let damaged = r.i32(line + 0x18)?;
        let cost = signed(r, line + 0x28)?;
        let speed = signed(r, line + 0x30)?;
        let produced = signed(r, line + 0x38)?;
        let queued = r.i32(line + 0x40)?;
        let priority = r.i32(line + 0x44)?;
        let amount = r.i32(line + 0x48)?;
        let mut numeric_policy = [cost, speed, produced]
            .iter()
            .all(|v| (0..=RAW_LIMIT).contains(v))
            && [active, damaged, queued, priority, amount]
                .iter()
                .all(|v| (0..=COUNTER_LIMIT).contains(v));
        let mut location = location::unavailable(if include_location {
            "railway_location_unavailable"
        } else {
            "not_requested"
        });
        let details = if is_building {
            building_count += 1;
            let repairs = r.i32(line + 0x78)?;
            let conversion = boolean(r, line + 0x8c)?;
            let allied = boolean(r, line + 0x99)?;
            numeric_policy &= (0..=COUNTER_LIMIT).contains(&repairs);
            let building = buildings::observe(r, base, line, &mut catalog)?;
            unavailable_buildings += usize::from(building.key.is_none());
            if include_location {
                location = match building.identity {
                    Some((building, template, ordinal)) => {
                        location::observe(r, base, own_tag, building, template, ordinal)?
                    }
                    None => location::unavailable("building_identity_unavailable"),
                };
                locations += usize::from(location["available"] == true);
            }
            json!({"building_id":building.key,"building_id_unavailable_reason":building.unavailable,
                "to_repair_raw_signed":repairs,"conversion":conversion,"allied_build":allied})
        } else {
            Value::Null
        };
        anomalous += usize::from(!numeric_policy);
        let fraction = if numeric_policy && cost > 0 {
            json!({"numerator":produced.to_string(),"denominator":cost.to_string()})
        } else {
            Value::Null
        };
        rows.push(json!({"entry_index":index,"line_kind":if is_building {"building"} else {"railway"},
            "line_handle":format!("0x{handle:016x}"),
            "handle_scope":"own_current_snapshot_collection",
            "cost_raw_signed":cost.to_string(),"speed_raw_signed":speed.to_string(),
            "produced_raw_signed":produced.to_string(),"active_factories_raw_signed":active,
            "damaged_factories_raw_signed":damaged,"queued_factories_raw_signed":queued,
            "priority_raw_signed":priority,"amount_raw_signed":amount,
            "numeric_policy_satisfied":numeric_policy,"produced_over_cost":fraction,
            "fraction_unavailable_reason":if !numeric_policy {json!("anomalous_numeric_policy")} else if cost==0 {json!("zero_cost")} else {Value::Null},
            "building":details,"location_available":location["available"],"location":location,
            "input_authorized":false}));
    }
    Ok(Lines {
        total: entries.len(),
        rows,
        unsupported,
        building_count,
        unavailable_buildings,
        anomalous,
        locations,
    })
}
/// Queued amount of one canonical building in one own state versus elsewhere.
/// Complete only when every line of that building has a proven own location.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct QueueSummary {
    pub(crate) in_state: i64,
    pub(crate) elsewhere: i64,
    pub(crate) complete: bool,
}
pub(crate) fn queue_summary(lines: &Lines, building: &str, state_id: i32) -> QueueSummary {
    let mut summary = QueueSummary {
        in_state: 0,
        elsewhere: 0,
        complete: lines.unsupported.is_empty() && lines.anomalous == 0,
    };
    for row in &lines.rows {
        if row["line_kind"] != "building" || row["building"]["building_id"] != building {
            continue;
        }
        let amount = row["amount_raw_signed"].as_i64().unwrap_or(-1);
        if amount < 0 || row["location_available"] != true {
            summary.complete = false;
            continue;
        }
        if row["location"]["state_id"].as_i64() == Some(i64::from(state_id)) {
            summary.in_state += amount;
        } else {
            summary.elsewhere += amount;
        }
    }
    summary
}
pub fn snapshot(process: &dyn Process) -> Result<Value> {
    page(process, 0, 32)
}
pub fn page(process: &dyn Process, offset: usize, limit: usize) -> Result<Value> {
    page_with_location(process, offset, limit, false)
}
pub fn page_with_location(
    process: &dyn Process,
    offset: usize,
    limit: usize,
    include_location: bool,
) -> Result<Value> {
    page_with_options(process, offset, limit, include_location, false)
}
/// Optional UI rows require the same snapshot's own territorial location joins.
pub fn page_with_options(
    process: &dyn Process,
    offset: usize,
    limit: usize,
    include_location: bool,
    include_ui: bool,
) -> Result<Value> {
    let include_location = include_location || include_ui;
    if offset > LINE_LIMIT || !(1..=PAGE_LIMIT).contains(&limit) {
        return Err(Error::new(
            "invalid_arguments",
            "Construction page exceeds bounds",
        ));
    }
    let start = Instant::now();
    process.begin_snapshot()?;
    let witnesses = Witnesses::new(process);
    let r = Reader::new(&witnesses);
    let base = process.image_base();
    let (country, production) = economy::own_production(&r, base)?;
    let own_tag = if include_location {
        r.i32(country + 8)?
    } else {
        0
    };
    let context = game::context_from_reader(&r, base)?;
    let Lines {
        rows: mut_rows,
        unsupported: mut_unsupported,
        total,
        building_count,
        unavailable_buildings,
        anomalous,
        locations,
    } = lines_in(&r, base, production, own_tag, include_location)?;
    let mut rows = mut_rows;
    let mut unsupported = mut_unsupported;
    let mut ui = if include_ui {
        ui::observe(&r, base, &rows)?
    } else {
        json!({"requested":false,"input_authorized":false})
    };
    let (end_calls, end_bytes) = witnesses.finish()?;
    process.check()?;
    let known = rows.len();
    let unknown = unsupported.len();
    let end = total.min(offset + limit);
    if let Some(ui_rows) = ui.get_mut("rows").and_then(Value::as_array_mut) {
        ui_rows.retain(|row| {
            row["entry_index"]
                .as_u64()
                .is_some_and(|i| (offset as u64..end as u64).contains(&i))
        });
    }
    // Full collection validation/witnesses precede pagination, even an empty page.
    for list in [&mut rows, &mut unsupported] {
        list.retain(|row| {
            row["entry_index"]
                .as_u64()
                .is_some_and(|i| (offset as u64..end as u64).contains(&i))
        });
    }
    Ok(
        json!({"source":"native own construction queue","build_id":game::layout(&r).build_id,
        "scope":"own_production_construction_collection_only","player":context.player,
        "handle_scope":{"kind":"own_current_snapshot_collection","player":context.player,
            "playthrough_id":context.playthrough_id,"date_raw":context.date_raw,
            "process_epoch":process.identity().map(|id|json!({"pid":id.pid,"start_ticks":id.start_ticks,"uid":id.uid})),
            "cross_load_identity_available":false},
        "total_count":total,"supported_count":known,"unsupported_count":unknown,
        "complete_catalog":unknown==0,"catalog_coverage":"types_in_own_construction_collection",
        "building_count":building_count,"building_id_unavailable_count":unavailable_buildings,
        "anomalous_numeric_count":anomalous,"numeric_data_complete":unknown==0&&anomalous==0,
        "offset":offset,"limit":limit,"page_count":rows.len()+unsupported.len(),
        "next_offset":if end<total {Some(end)} else {None},"page_index_scope":"fresh_snapshot_collection_only",
        "lines":rows,"unsupported_type_metadata":unsupported,"ui":ui,
        "numeric_encoding":{"signed_64_bit_fields":"decimal_strings","cost_units_per_point":100000,
            "speed_time_unit_available":false,"fraction":"produced/cost; unclamped; not ETA or completion proof"},
        "policy":{"maximum_collection_capacity":LINE_LIMIT,"maximum_raw_scalar":RAW_LIMIT,
            "maximum_counter":COUNTER_LIMIT,"nonzero_unique_handles_required":true,
            "building_handle_kind":59,"railway_handle_kind":"dynamic","engine_invariant_claimed":false},
        "location_available":locations>0,"location_requested":include_location,
        "location_available_count":locations,"location_coverage":"full_own_collection; available means at least one proven own territorial location",
        "railway_path_available":false,"idle_builders_available":false,
        "allocation_available":false,"eligibility_available":false,"input_authorized":false,
        "live_validation":"pending","observation_paused_game":false,
        "freshness":{"observed_unix_ms":SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_millis(),
            "elapsed_ms":start.elapsed().as_secs_f64()*1000.0,"read_calls_kind":"logical_read_requests",
            "read_calls":r.calls.get()+end_calls as u64,"read_bytes":r.bytes.get()+end_bytes}}),
    )
}

#[cfg(test)]
mod queue_summary_tests {
    use super::*;
    fn line(building: &str, state: Option<i64>, amount: i64) -> Value {
        json!({"line_kind":"building","building":{"building_id":building},"amount_raw_signed":amount,
            "location_available":state.is_some(),"location":{"state_id":state}})
    }
    fn lines(rows: Vec<Value>) -> Lines {
        Lines {
            total: rows.len(),
            rows,
            unsupported: Vec::new(),
            building_count: 0,
            unavailable_buildings: 0,
            anomalous: 0,
            locations: 0,
        }
    }
    #[test]
    fn sums_only_the_requested_building_and_needs_every_location() {
        let l = lines(vec![
            line("arms_factory", Some(64), 2),
            line("arms_factory", Some(64), 1),
            line("arms_factory", Some(65), 4),
            line("dockyard", Some(64), 7),
            json!({"line_kind":"railway"}),
        ]);
        assert_eq!(
            queue_summary(&l, "arms_factory", 64),
            QueueSummary {
                in_state: 3,
                elsewhere: 4,
                complete: true
            }
        );
        let l = lines(vec![line("arms_factory", None, 1)]);
        assert!(!queue_summary(&l, "arms_factory", 64).complete);
        let l = lines(vec![line("arms_factory", Some(64), -1)]);
        assert!(!queue_summary(&l, "arms_factory", 64).complete);
        let mut l = lines(vec![line("arms_factory", Some(64), 1)]);
        l.unsupported.push(json!({"entry_index":1}));
        assert!(!queue_summary(&l, "arms_factory", 64).complete);
    }
    #[test]
    fn coastal_and_provincial_lines_count_toward_their_own_state() {
        // A dockyard is a coastal-only state building; a province-located line
        // (naval base) still belongs to the state its province joins.
        let mut naval = line("naval_base", Some(63), 1);
        naval["location"]["province_id"] = json!(9361);
        naval["location"]["kind"] = json!("province");
        let l = lines(vec![
            line("dockyard", Some(63), 1),
            line("dockyard", Some(62), 2),
            naval.clone(),
            line("arms_factory", Some(63), 5),
        ]);
        assert_eq!(
            queue_summary(&l, "dockyard", 63),
            QueueSummary {
                in_state: 1,
                elsewhere: 2,
                complete: true
            }
        );
        assert_eq!(
            queue_summary(&l, "naval_base", 63),
            QueueSummary {
                in_state: 1,
                elsewhere: 0,
                complete: true
            }
        );
        naval["location_available"] = json!(false);
        let l = lines(vec![line("dockyard", Some(63), 1), naval]);
        assert!(queue_summary(&l, "dockyard", 63).complete);
        assert!(!queue_summary(&l, "naval_base", 63).complete);
    }
}
