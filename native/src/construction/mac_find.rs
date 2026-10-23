//! Mach-O 089EA519 Find view and own-state camera facts. Offsets are recovered
//! from this build's constructors, row refresh and ordinary centring path.
//! No search, input, engine invocation, placement or current-frame authority.
use super::{find_view::validate_state_id, invalid, location, mac_ui, pointer};
use crate::{
    economy, error::Result, game, memory::Reader, platform::Process, research_ui::Witnesses,
};
use serde_json::{json, Value};
use std::time::Instant;

const FIND: u64 = 0x3457c38;
const ROW: u64 = 0x34580d8;
const STATE_TEMPLATE: u64 = 0x337de78;
const ENTRY_BYTES: usize = 0x40;

fn refreshing() -> crate::error::Error {
    crate::error::Error::new("torn_read", "Mac Find first row and first entry disagree")
}

fn first_row(r: &Reader<'_>, base: u64, gs: u64, find: u64) -> Result<Value> {
    let (_, data, count) = r.array_header(find + 0xc90)?;
    if count > 20_000 || (count > 0 && data % 8 != 0) {
        return Err(invalid("Bounded aligned Mac Find results required"));
    }
    // The refresh path may retain and reinsert the old row when the new
    // result count is zero. It is not a current first result; skip its pointer
    // and payload rather than mistaking this stable empty state for a tear.
    if count == 0 {
        return Ok(json!({"result_count":0,"first_row":null}));
    }
    let row = r.u64(find + 0xc88)?;
    if row == 0 {
        return Ok(json!({"result_count":count,"first_row":null}));
    }
    mac_ui::exact(r, base, row, ROW)?;
    let kind = r.i32(row + 0x408)?;
    let bytes = r.read(data, ENTRY_BYTES)?;
    let entry_kind = i32::from_le_bytes(bytes[0x38..0x3c].try_into().unwrap());
    if kind != entry_kind {
        return Err(refreshing());
    }
    let mut first = json!({"kind_raw":kind,"state_id":null,"supported":kind==2});
    let field = match kind {
        1 => Some(0x3f0),
        2 => Some(0x3f8),
        3 => Some(0x400),
        _ => None,
    };
    if let Some(offset) = field {
        let object = r.u64(row + offset)?;
        let entry_object = u64::from_le_bytes(bytes[0x30..0x38].try_into().unwrap());
        if object == 0 || object != entry_object {
            return Err(refreshing());
        }
        // Other kinds are visible result metadata only. Never follow their
        // province/country payload to infer hidden information.
        if kind == 2 {
            let l = super::layout(r);
            mac_ui::exact(r, base, object, l.state_vtable)?;
            let id = r.i32(object + l.state_id)?;
            if location::catalog_pointer(r, gs + l.gs_states, id, 4096)? != object {
                return Err(invalid("Mac Find state must join its current catalog"));
            }
            first["state_id"] = json!(id);
        }
    }
    Ok(json!({"result_count":count,"first_row":first}))
}

fn view(
    r: &Reader<'_>,
    base: u64,
    gs: u64,
    idler: u64,
    registry: u64,
    manager: u64,
) -> Result<Value> {
    let find = r.u64(manager + 0x190)?;
    mac_ui::exact(r, base, find, FIND)?;
    if r.u64(find + 0xc30)? != idler {
        return Err(invalid("Mac Find view must retain the own idler"));
    }
    let window = r.u64(find + 0xc38)?;
    let mut out = json!({"materialized":window!=0,"window":null,"query":null,
        "text_box_focused":null,"results":null});
    if window == 0 {
        return Ok(out);
    }
    let root = mac_ui::window(r, base, window, registry)?;
    let flags = r.byte(root + 0x95)?;
    let states = [r.i32(root + 0x12b8)?, r.i32(root + 0x1270)?];
    if states.iter().any(|s| !(0..=2).contains(s)) {
        return Err(invalid(
            "Mac Find transition state outside reviewed encoding",
        ));
    }
    let visible = flags & 8 != 0;
    out["window"] = json!({"exact_container":true,"own_registry_verified":true,
        "visible_flag":visible,"local_transitions_idle":states==[0,0],
        "settled_visibility_verified":false});
    // Hidden retained rows/queries cannot be current Find results. Avoid those
    // payloads entirely, including the stale first-row pointer.
    if visible {
        let text = r.string_bounded(find + 0xc48, 64)?;
        out["query"] = if text.bytes().all(|b| (0x20..0x7f).contains(&b)) {
            json!(text)
        } else {
            json!({"unsupported":"non_printable_ascii","length":text.len()})
        };
        out["results"] = first_row(r, base, gs, find)?;
    }
    Ok(out)
}

fn camera(r: &Reader<'_>, idler: u64) -> Result<([f32; 3], [f32; 3])> {
    // Exact idler virtual +148/+150 returns this+708. The ordinary state
    // centring path reads template+120 and calls 101929770, which writes the
    // eye at +698 and look-at at +6a4, setting look-at height from its constant.
    let object = r.u64(idler + 0x708)?;
    pointer(object)?;
    let bytes = r.read(object + 0x698, 24)?;
    let values: Vec<f32> = bytes
        .chunks_exact(4)
        .map(|b| f32::from_le_bytes(b.try_into().unwrap()))
        .collect();
    if values
        .iter()
        .any(|v| !v.is_finite() || v.abs() > 1_000_000.0)
    {
        return Err(invalid("Finite bounded Mac camera coordinates required"));
    }
    Ok((
        values[..3].try_into().unwrap(),
        values[3..].try_into().unwrap(),
    ))
}

fn target(r: &Reader<'_>, base: u64, gs: u64, own: i32, id: i32, look: [f32; 3]) -> Result<Value> {
    let l = super::layout(r);
    let state = location::catalog_pointer(r, gs + l.gs_states, id, 4096)?;
    mac_ui::exact(r, base, state, l.state_vtable)?;
    if r.i32(state + l.state_id)? != id {
        return Err(invalid("Mac Find target state ID/catalog differs"));
    }
    if !location::own_state(r, base, state, own)? {
        return Ok(json!({"state_id":id,"own_owned_and_controlled":false,
            "anchor":null,"camera_centered":false}));
    }
    let template = r.u64(state + 0x20)?;
    mac_ui::exact(r, base, template, STATE_TEMPLATE)?;
    let x = r.i32(template + 0x120)?;
    let z = r.i32(template + 0x124)?;
    if x.abs_diff(0) > 1_000_000 || z.abs_diff(0) > 1_000_000 {
        return Err(invalid("Bounded Mac state map anchor required"));
    }
    Ok(json!({"state_id":id,"own_owned_and_controlled":true,
        "anchor":{"kind":"state_template_map_position","map_position":[x,z]},
        "camera_centered":look[0]==x as f32 && look[2]==z as f32,
        "comparison":"exact_template_position_without_wrap_adjustment"}))
}

pub(super) fn snapshot(process: &dyn Process, state_id: Option<i32>) -> Result<Value> {
    let start = Instant::now();
    process.begin_snapshot()?;
    let identity = process
        .identity()
        .cloned()
        .ok_or_else(|| invalid("Owned Mac Find process required"))?;
    let base = process.image_base();
    let w = Witnesses::new(process);
    let r = Reader::new(&w);
    w.section("mac_find_own_scope");
    let (country, _) = economy::own_production(&r, base)?;
    let own = r.i32(country + 8)?;
    let campaign = game::context_from_reader(&r, base)?;
    let [gs, idler, _] = campaign.anchors;
    let registry = r.u64(idler + 0x4c0)?;
    mac_ui::exact(&r, base, registry, 0x34974d0)?;
    let manager = r.u64(idler + 0x670)?;
    pointer(manager)?;
    if r.u64(manager + 0x180)? != idler {
        return Err(invalid("Mac Find manager must retain the own idler"));
    }
    w.section("mac_find_view");
    let observed = view(&r, base, gs, idler, registry, manager)?;
    w.section("mac_find_camera");
    let (eye, look_at) = camera(&r, idler)?;
    let requested = match state_id {
        Some(id) => target(
            &r,
            base,
            gs,
            own,
            validate_state_id(i64::from(id))?,
            look_at,
        )?,
        None => Value::Null,
    };
    let (calls, bytes) = w.finish()?;
    process.check()?;
    if process.identity() != Some(&identity) || process.image_base() != base {
        return Err(invalid("Mac Find process identity changed"));
    }
    Ok(
        json!({"source":"native own Mac construction find view","build_id":game::MACOS.build_id,
        "scope":"own_find_view_camera_and_requested_own_state","player":campaign.player,
        "date_raw":campaign.date_raw,"paused":campaign.paused,"find_view":observed,
        "camera":{"eye":eye,"look_at":look_at},"target":requested,
        "input_authorized":false,"placement_eligibility_verified":false,
        "effective_routing_verified":false,"current_frame_verified":false,
        "live_validation":"pending","observation_paused_game":false,
        "freshness":{"observed_unix_ms":crate::daily_observation::unix_ms()?,
            "elapsed_ms":start.elapsed().as_secs_f64()*1000.0,
            "read_calls_kind":"logical_read_requests","read_calls":r.calls.get()+calls as u64,
            "read_bytes":r.bytes.get()+bytes}}),
    )
}

#[cfg(test)]
#[path = "../../tests/support/mac_construction_find.rs"]
mod tests;
