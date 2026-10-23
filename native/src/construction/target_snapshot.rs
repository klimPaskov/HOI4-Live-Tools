//! Complete typed own queue evidence for one public building and own state.
//! No GUI, input, eligibility, capacity, built-level or completion inference.
use super::{
    catalog, controls::validate_key, invalid, location, mac_map_point, QueueLine, QueueSummary,
};
use crate::{
    economy,
    error::{Error, Result},
    game,
    memory::{Reader, StringAbi},
    platform::Process,
    research_ui::Witnesses,
};
use serde_json::{json, Value};
use std::time::Instant;

fn phase(stage: &str, error: Error) -> Error {
    Error::new(error.code, format!("{stage}: {}", error.message))
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct Identity {
    country: u64,
    production: u64,
    state: u64,
    template: u64,
    ordinal: usize,
    members: Vec<u64>,
    lines: Vec<QueueLine>,
}
pub(crate) struct Observation {
    pub(crate) campaign: game::ClockContext,
    pub(crate) summary: QueueSummary,
    identity: Identity,
    calls: u64,
    bytes: usize,
    elapsed_ms: f64,
    map_point: Option<mac_map_point::Observation>,
}

pub(crate) fn observe(process: &dyn Process, key: &str, state_id: i32) -> Result<Observation> {
    observe_with_map_point(process, key, state_id, false)
}

fn observe_with_map_point(
    process: &dyn Process,
    key: &str,
    state_id: i32,
    include_map_point: bool,
) -> Result<Observation> {
    validate_key(key)?;
    super::find_view::validate_state_id(i64::from(state_id))?;
    if key == "rail_way" {
        return Err(Error::new(
            "capability_unavailable",
            "Railway path amounts are not building queue evidence",
        ));
    }
    if include_map_point && process.string_abi() != StringAbi::LibCxx {
        return Err(Error::new(
            "unsupported_platform",
            "Center-terrain construction diagnostics require the inspected Mac layout",
        ));
    }
    let start = Instant::now();
    process.begin_snapshot()?;
    let witnesses = Witnesses::new(process);
    let r = Reader::new(&witnesses);
    let base = process.image_base();
    witnesses.section("construction_owner");
    let (country, production) = economy::own_production(&r, base)?;
    let campaign = game::context_from_reader(&r, base)?;
    let own_tag = r.i32(country + 8)?;
    let l = super::layout(&r);
    witnesses.section("construction_target_state");
    let state = location::catalog_pointer(&r, campaign.anchors[0] + l.gs_states, state_id, 4096)?;
    if !location::own_state(&r, base, state, own_tag).map_err(|e| phase("target_state", e))? {
        return Err(Error::new(
            "construction_target_unavailable",
            "Requested state must be owned and controlled by the current player",
        ));
    }
    if r.i32(state + l.state_id)? != state_id {
        return Err(invalid("Requested state ID/catalog mismatch"));
    }
    witnesses.section("construction_public_catalog");
    let catalog = catalog::observe(&r, base)?;
    let target = catalog.required(key)?;
    witnesses.section("construction_queue");
    let lines =
        super::lines_in(&r, base, production, own_tag, true).map_err(|e| phase("own_queue", e))?;
    let summary = super::queue_summary(&lines, key, state_id);
    let map_point = if include_map_point {
        witnesses.section("construction_center_terrain");
        Some(mac_map_point::from_reader(&r, base, state_id, None)?)
    } else {
        None
    };
    let identity = Identity {
        country,
        production,
        state,
        template: target.address,
        ordinal: target.ordinal,
        members: lines.members,
        lines: lines.known,
    };
    let (calls, bytes) = witnesses.finish()?;
    process.check()?;
    Ok(Observation {
        campaign,
        summary,
        identity,
        calls: r.calls.get() + calls as u64,
        bytes: r.bytes.get() + bytes,
        elapsed_ms: start.elapsed().as_secs_f64() * 1000.0,
        map_point,
    })
}

pub fn snapshot(process: &dyn Process, key: &str, state_id: i32) -> Result<Value> {
    snapshot_with_map_point(process, key, state_id, false)
}

/// The optional Mac terrain candidate shares the complete target/queue witness
/// bracket. False preserves the original read path and JSON fields exactly.
pub fn snapshot_with_map_point(
    process: &dyn Process,
    key: &str,
    state_id: i32,
    include_map_point: bool,
) -> Result<Value> {
    let o = if include_map_point {
        observe_with_map_point(process, key, state_id, true)?
    } else {
        observe(process, key, state_id)?
    };
    // Partial sums stay unavailable publicly; missing data is never zero.
    let counts = o.summary.complete.then(|| {
        json!({"in_state":o.summary.in_state,
        "elsewhere":o.summary.elsewhere})
    });
    let mut value = json!({"source":"native witnessed own construction target",
        "scope":"one_public_building_in_own_owned_and_controlled_state",
        "player":o.campaign.player,"playthrough_id":o.campaign.playthrough_id,
        "date_raw":o.campaign.date_raw,"paused":o.campaign.paused,
        "building_id":key,"state_id":state_id,"target_identity_verified":true,
        "complete":o.summary.complete,"queued":counts,
        "unavailable_reason":if o.summary.complete { None } else { Some("incomplete_own_queue_evidence") },
        "line_count":o.identity.members.len(),"supported_count":o.identity.lines.len(),
        "input_authorized":false,"eligibility_available":false,"observation_paused_game":false,
        "built_levels_available":false,"cross_load_identity_available":false,
        "freshness":{"elapsed_ms":o.elapsed_ms,"read_calls":o.calls,"read_bytes":o.bytes,
            "read_calls_kind":"logical_read_requests"}});
    if let Some(point) = o.map_point {
        let [candidates, samples, nodes] = point.terrain_counts();
        value["map_point"] = json!({
            "source":"witnessed own canonical center-terrain fallback",
            "client_point":point.center(),"world_point":point.world_point(),
            "province_id":point.province_id(),"state_id":point.state_id(),
            "map_mode_raw":point.mode(),
            "terrain":{"candidates":candidates,"samples":samples,"nodes":nodes},
            "own_owned_and_controlled_state":true,"canonical_province_fallback":true,
            "full_picker_verified":false,"model_hit_testing_verified":false,
            "gui_routing_verified":false,"current_event_verified":false,
            "eligibility_available":false,"input_authorized":false
        });
    }
    Ok(value)
}

pub(crate) fn tool() -> Value {
    json!({"name":"hoi4_construction_target","description":"Compact read-only queued amounts for one canonical public building and one own-owned-and-controlled state. The complete bounded queue, public catalog, target and reciprocal locations share one witness bracket; no first-page totals. Mac-only include_map_point defaults false; true adds a witnessed 1920x1080 center-terrain candidate joined to that own state, not the full picker, GUI routing or input authority. Incomplete queue evidence exposes null amounts. Never opens UI, moves the cursor, pauses, or proves building eligibility, capacity, built levels or input completion. Railway paths unsupported.",
        "inputSchema":{"type":"object","properties":{"building_id":{"type":"string","minLength":1,"maxLength":128},"state_id":{"type":"integer","minimum":1,"maximum":4095},"include_map_point":{"type":"boolean","default":false}},"required":["building_id","state_id"],"additionalProperties":false},"annotations":{"readOnlyHint":true}})
}
