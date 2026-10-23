//! Read-only own map Find view, camera look-at and requested own state anchor.
//! Exact current Linux layout independently recovered; see PROVENANCE.md.
//! No input, search, camera movement, placement or eligibility authority.
use super::{invalid, location, pointer};
use crate::{
    economy, error::Result, game, memory::Reader, platform::Process, research_ui::Witnesses,
};
use serde_json::{json, Value};
use std::time::Instant;

const FIND_VIEW: u64 = 0x4318868;
const RESULT_ROW: u64 = 0x4318b08;
const CONTAINER: u64 = 0x4347d58;
/// CEditBox embedded text receiver, as in the live-verified Focus search box.
const TEXT_RECEIVER: u64 = 0x4349be8;
const STATE: u64 = 0x41fba98;
const STATE_TEMPLATE: u64 = 0x41fc068;
const STATE_KIND: i32 = 2;
/// Reviewed camera setter writes this look-at height for every centring.
const LOOK_AT_HEIGHT: f32 = 9.5;
/// Only entry 0 is read; the count bound matches the reader's array limit.
const RESULT_BOUND: usize = 20_000;
const ENTRY: usize = 0x50;

pub fn validate_state_id(id: i64) -> Result<i32> {
    if !(1..4096).contains(&id) {
        return Err(crate::error::Error::new(
            "invalid_arguments",
            "state_id must be a positive bounded state ID",
        ));
    }
    Ok(id as i32)
}
/// Exact type check with an object-specific refusal. The observed primary is
/// reported image-relative (static RVA), never as a live absolute address.
fn typed(r: &Reader<'_>, base: u64, address: u64, rva: u64, what: &str) -> Result<()> {
    pointer(address)?;
    let primary = r.u64(address)?;
    if primary != base + rva {
        let observed = primary
            .checked_sub(base)
            .filter(|v| *v < 0x1000_0000)
            .map_or("outside_image".to_string(), |v| format!("rva_{v:x}"));
        return Err(crate::error::Error::new(
            "invalid_construction",
            format!("Find view {what} type differs (expected rva_{rva:x}, observed {observed})"),
        ));
    }
    Ok(())
}
fn query(r: &Reader<'_>, address: u64) -> Result<Value> {
    let text = r.string_bounded(address, 64)?;
    Ok(if text.bytes().all(|b| (0x20..0x7f).contains(&b)) {
        json!(text)
    } else {
        json!({"unsupported":"non_printable_ascii","length":text.len()})
    })
}
fn floats(bytes: &[u8]) -> Vec<f32> {
    bytes
        .chunks_exact(4)
        .map(|v| f32::from_le_bytes(v.try_into().unwrap()))
        .collect()
}
/// One state-kind row or entry object, joined to the fresh state catalog.
fn state_object(r: &Reader<'_>, base: u64, gs: u64, object: u64) -> Result<i32> {
    typed(r, base, object, STATE, "state")?;
    let id = r.i32(object + 0x48)?;
    if location::catalog_pointer(r, gs + 0x2c8, id, 4096)? != object {
        return Err(invalid("Find result state must join its current catalog"));
    }
    Ok(id)
}
fn refreshing() -> crate::error::Error {
    crate::error::Error::new(
        "torn_read",
        "Find first row and first result entry disagree; results refreshing",
    )
}
fn first_row(r: &Reader<'_>, base: u64, gs: u64, find: u64) -> Result<Value> {
    let (_, data, count) = r.array_header(find + 0x8c8)?;
    if count > RESULT_BOUND || (count > 0 && data % 8 != 0) {
        return Err(invalid("Find results exceed reviewed bound"));
    }
    let row = r.u64(find + 0x8c0)?;
    let mut out = json!({"result_count":count,"first_row":null});
    if row == 0 {
        return Ok(out);
    }
    if count == 0 {
        return Err(invalid("Find first row requires a result entry"));
    }
    typed(r, base, row, RESULT_ROW, "first row")?;
    let kind = r.i32(row + 0x2b8)?;
    // Fresh rows (2a3ede0) fill all three object fields; recycled rows (2a3f160)
    // fill only their kind's field and zero the others (Run084).
    let field = match kind {
        1 => Some(0x2a0),
        STATE_KIND => Some(0x2a8),
        3 => Some(0x2b0),
        _ => None,
    };
    let entry = r.read(data, ENTRY)?;
    let entry_object = u64::from_le_bytes(entry[0x40..0x48].try_into().unwrap());
    let entry_kind = i32::from_le_bytes(entry[0x48..0x4c].try_into().unwrap());
    if entry_kind != kind {
        // Typing refreshes results per key; a row/entry disagreement is a
        // snapshot taken mid-refresh, not a layout fact.
        return Err(refreshing());
    }
    let mut first = json!({"kind_raw":kind,"state_id":null,"supported":kind==STATE_KIND});
    if let Some(field) = field {
        let object = r.u64(row + field)?;
        if object == 0 || object != entry_object {
            return Err(refreshing());
        }
        if kind == STATE_KIND {
            first["state_id"] = json!(state_object(r, base, gs, object)?);
        }
    }
    out["first_row"] = first;
    Ok(out)
}
fn target(
    r: &Reader<'_>,
    base: u64,
    gs: u64,
    own_tag: i32,
    id: i32,
    look_at: &[f32],
) -> Result<(Value, u64, Option<u64>)> {
    let state = location::catalog_pointer(r, gs + 0x2c8, id, 4096)?;
    if state_object(r, base, gs, state)? != id {
        return Err(invalid("Requested state ID/catalog mismatch"));
    }
    // Authorization precedes anchor template payload.
    if !location::own_state(r, base, state, own_tag)? {
        return Ok((
            json!({"state_id":id,"own_owned_and_controlled":false,"anchor":null,
            "camera_centered":false}),
            state,
            None,
        ));
    }
    // The camera centring routine 0x207b9a0 reads this exact state template's
    // map position (Run081 observed the CStateTemplate primary here).
    let template = r.u64(state + 0x20)?;
    typed(r, base, template, STATE_TEMPLATE, "state template")?;
    let x = r.i32(template + 0x140)?;
    let y = r.i32(template + 0x144)?;
    let centered = look_at.len() == 3 && look_at[0] == x as f32 && look_at[2] == y as f32;
    Ok((
        json!({"state_id":id,"own_owned_and_controlled":true,
        "anchor":{"kind":"state_template_map_position","map_position":[x,y]},"camera_centered":centered}),
        state,
        Some(template),
    ))
}
/// Stable own identities pinned by Find stages; semantic view facts excluded.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct FindIdentity {
    pub(crate) find: u64,
    pub(crate) window: u64,
    pub(crate) text_box: u64,
    pub(crate) camera: u64,
    pub(crate) state: Option<u64>,
    pub(crate) template: Option<u64>,
}
/// Find view, camera and optional requested state inside the CALLER's witnessed
/// bracket. The caller already proved own production, campaign and manager.
#[allow(clippy::too_many_arguments)]
pub(crate) fn observe_in(
    r: &Reader<'_>,
    base: u64,
    gs: u64,
    idler: u64,
    registry: u64,
    manager: u64,
    own_tag: i32,
    state_id: Option<i32>,
) -> Result<(Value, FindIdentity)> {
    let find = r.u64(manager + 0x190)?;
    typed(r, base, find, FIND_VIEW, "object")?;
    if r.u64(find + 0x860)? != idler {
        return Err(invalid("Find view must belong to own idler"));
    }
    let window = r.u64(find + 0x868)?;
    let mut text_box = 0;
    let mut view = json!({"materialized":window!=0,"window":null,"query":null,
        "text_box_focused":null,"results":null});
    if window != 0 {
        pointer(window)?;
        let flags = r.byte(window + 0xa5)?;
        let container = r.u64(window)? == base + CONTAINER;
        view["window"] = json!({"exact_container":container,"flags_raw":flags,
            "visible_flag":flags&8!=0,"close_requested_flag":flags&0x10!=0,
            "settled_visibility_verified":false});
        text_box = r.u64(find + 0x870)?;
        // GUI+158 is keyboard focus (the edit box); GUI+230 is the separate text
        // receiver embedded at edit+1a0 (Run083 saw the view open but +230 never
        // equal to the box itself). Unknown receiver layout stays unknown.
        view["text_box_focused"] = if text_box == 0 {
            json!(null)
        } else {
            pointer(text_box)?;
            if r.u64(text_box + 0x1a0)? == base + TEXT_RECEIVER {
                json!(
                    r.u64(registry + 0x158)? == text_box
                        && r.u64(registry + 0x230)? == text_box + 0x1a0
                )
            } else {
                json!(null)
            }
        };
        view["query"] = query(r, find + 0x878)?;
        view["results"] = first_row(r, base, gs, find)?;
    }
    let camera = r.u64(idler + 0x750)?;
    pointer(camera)?;
    let values = floats(&r.read(camera + 0x698, 0x18)?);
    let (eye, look_at) = values.split_at(3);
    if !values.iter().all(|v| v.is_finite()) || look_at[1] != LOOK_AT_HEIGHT {
        return Err(invalid("Camera look-at outside reviewed invariant"));
    }
    let (requested, state, template) = match state_id {
        Some(id) => {
            let (value, state, template) = target(r, base, gs, own_tag, id, look_at)?;
            (value, Some(state), template)
        }
        None => (json!(null), None, None),
    };
    Ok((
        json!({"find_view":view,"camera":{"eye":eye,"look_at":look_at,"look_at_height_invariant":true},
            "target":requested}),
        FindIdentity {
            find,
            window,
            text_box,
            camera,
            state,
            template,
        },
    ))
}
pub fn snapshot(process: &dyn Process, state_id: Option<i32>) -> Result<Value> {
    if let Some(id) = state_id {
        validate_state_id(i64::from(id))?;
    }
    if process.string_abi() == crate::memory::StringAbi::LibCxx {
        return super::mac_find::snapshot(process, state_id);
    }
    super::require_linux_ui(process.string_abi())?;
    let start = Instant::now();
    process.begin_snapshot()?;
    let witnesses = Witnesses::new(process);
    let r = Reader::new(&witnesses);
    let base = process.image_base();
    witnesses.section("own_production");
    let (country, _) = economy::own_production(&r, base)?;
    let own_tag = r.i32(country + 8)?;
    witnesses.section("campaign");
    let campaign = game::context_from_reader(&r, base)?;
    let [gs, idler, _] = campaign.anchors;
    witnesses.section("interface");
    let registry = r.u64(idler + 0x4f8)?;
    typed(&r, base, registry, 0x42736f8, "registry")?;
    let manager = r.u64(idler + 0x6b8)?;
    pointer(manager)?;
    if r.u64(manager + 0x180)? != idler {
        return Err(invalid("Construction manager must belong to own idler"));
    }
    witnesses.section("find_view");
    let (observed, _) = observe_in(&r, base, gs, idler, registry, manager, own_tag, state_id)?;
    let (calls, bytes) = witnesses.finish()?;
    process.check()?;
    Ok(
        json!({"source":"native own construction find view","build_id":game::BUILD_ID,
        "scope":"own_find_view_camera_and_requested_own_state","player":campaign.player,
        "date_raw":campaign.date_raw,"paused":campaign.paused,"find_view":observed["find_view"],
        "camera":observed["camera"],"target":observed["target"],"input_authorized":false,"placement_eligibility_verified":false,
        "effective_routing_verified":false,"current_frame_verified":false,
        "live_validation":"pending","observation_paused_game":false,
        "freshness":{"observed_unix_ms":crate::daily_observation::unix_ms()?,
            "elapsed_ms":start.elapsed().as_secs_f64()*1000.0,
            "read_calls_kind":"logical_read_requests","read_calls":r.calls.get()+calls as u64,
            "read_bytes":r.bytes.get()+bytes}}),
    )
}
pub(crate) fn tool() -> Value {
    json!({"name":"hoi4_construction_find_view","description":"Read-only own map Find view using independently recovered Linux and native Mac layouts. Visibility flags, bounded query, first result row joined to its current state catalog, camera eye/look-at and an optional requested own state's template map position. Mac hidden views skip stale query/row payload; Mac focus and wrapped-camera equivalence remain unqualified. No input, search, camera movement, placement, eligibility, current-frame or routing authority. Never pauses or moves the cursor.","inputSchema":{"type":"object","properties":{"state_id":{"type":"integer","minimum":1,"maximum":4095}},"additionalProperties":false},"annotations":{"readOnlyHint":true}})
}
