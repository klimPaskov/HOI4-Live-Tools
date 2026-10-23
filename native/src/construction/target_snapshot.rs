//! Complete typed own queue evidence for one public building and own state.
//! No GUI, input, eligibility, capacity, built-level or completion inference.
use super::{boolean, controls::validate_key, invalid, location, members, QueueLine, QueueSummary};
use crate::{
    economy,
    error::{Error, Result},
    game,
    memory::Reader,
    platform::Process,
    research_ui::Witnesses,
};
use serde_json::{json, Value};
use std::{collections::BTreeSet, time::Instant};

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
}

/// An empty queue still needs an existing canonical template. Validate the
/// complete bounded public catalog; invalid sentinel templates have no payload.
fn template(r: &Reader<'_>, base: u64, key: &str) -> Result<(u64, usize)> {
    let l = super::layout(r);
    let db = r.u64(base + l.database_global)?;
    location::exact(r, db, base + l.database_vtable)
        .map_err(|e| phase("public_catalog_database", e))?;
    let catalog = members(r, db + 0x40, 4096)?;
    let mut keys = BTreeSet::new();
    let mut selected = None;
    for (ordinal, address) in catalog.into_iter().enumerate() {
        // The inspected builds have a distinct TNullObject<CBuildingTemplate>
        // primary at catalog slot zero. Only that exact inactive sentinel may
        // omit canonical payload. Other derived/unknown types still refuse.
        if r.u64(address)? == base + l.null_template_vtable {
            if ordinal != 0 || boolean(r, address + 0x18)? {
                return Err(invalid(
                    "Inactive null building template required only at catalog zero",
                ));
            }
            continue;
        }
        location::exact(r, address, base + l.template_vtable)
            .map_err(|e| phase(&format!("public_catalog_template_{ordinal}"), e))?;
        if !boolean(r, address + 0x18)? {
            continue;
        }
        if r.i32(address + l.template_ordinal)? != ordinal as i32 {
            return Err(invalid("Construction template ordinal/catalog mismatch"));
        }
        let canonical = r.string_bounded(address + l.template_key, 128)?;
        validate_key(&canonical).map_err(|_| invalid("Bounded canonical building key required"))?;
        if !keys.insert(canonical.clone()) {
            return Err(invalid("Unique canonical building catalog keys required"));
        }
        if canonical == key {
            selected = Some((address, ordinal));
        }
    }
    selected.ok_or_else(|| {
        Error::new(
            "construction_target_unavailable",
            "Requested building absent from the current valid public catalog",
        )
    })
}

pub(crate) fn observe(process: &dyn Process, key: &str, state_id: i32) -> Result<Observation> {
    validate_key(key)?;
    super::find_view::validate_state_id(i64::from(state_id))?;
    if key == "rail_way" {
        return Err(Error::new(
            "capability_unavailable",
            "Railway path amounts are not building queue evidence",
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
    let (template, ordinal) = template(&r, base, key)?;
    witnesses.section("construction_queue");
    let lines =
        super::lines_in(&r, base, production, own_tag, true).map_err(|e| phase("own_queue", e))?;
    let summary = super::queue_summary(&lines, key, state_id);
    let identity = Identity {
        country,
        production,
        state,
        template,
        ordinal,
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
    })
}

pub fn snapshot(process: &dyn Process, key: &str, state_id: i32) -> Result<Value> {
    let o = observe(process, key, state_id)?;
    // Partial sums stay unavailable publicly; missing data is never zero.
    let counts = o.summary.complete.then(|| {
        json!({"in_state":o.summary.in_state,
        "elsewhere":o.summary.elsewhere})
    });
    Ok(json!({"source":"native witnessed own construction target",
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
            "read_calls_kind":"logical_read_requests"}}))
}

pub(crate) fn tool() -> Value {
    json!({"name":"hoi4_construction_target","description":"Compact read-only queued amounts for one canonical public building and one own-owned-and-controlled state. The complete bounded queue, public catalog, target and reciprocal locations share one witness bracket; no first-page totals. Incomplete evidence exposes null amounts. Never opens UI, moves the cursor, pauses, or proves building eligibility, capacity, built levels or input completion. Railway paths unsupported.",
        "inputSchema":{"type":"object","properties":{"building_id":{"type":"string","minLength":1,"maxLength":128},"state_id":{"type":"integer","minimum":1,"maximum":4095}},"required":["building_id","state_id"],"additionalProperties":false},"annotations":{"readOnlyHint":true}})
}
