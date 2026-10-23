//! Mach-O 089EA519 Find view and own-state camera facts. Offsets are recovered
//! from this build's constructors, row refresh and ordinary centring path.
//! No search, input, engine invocation, placement or current-frame authority.
use super::{find_view::validate_state_id, invalid, location, mac_ui, pointer};
use crate::{
    economy, error::Result, game, memory::Reader, platform::Process, research_ui::Witnesses,
};
#[cfg(test)]
use serde_json::json;
use serde_json::Value;
use std::time::Instant;

// Prospective typed readback only; no adapter or construction export uses it.
#[allow(dead_code)]
mod retained;
#[allow(unused_imports)]
pub(crate) use retained::{
    observe, CameraState, FindAnchor, FindBinding, FirstResult, Observation, OwnStateAnchor,
    RootLifecycle,
};
use retained::{Query, RequestedState, Results, View};

const FIND: u64 = 0x3457c38;
const ROW: u64 = 0x34580d8;
const STATE_TEMPLATE: u64 = 0x337de78;
const ENTRY_BYTES: usize = 0x40;

fn refreshing() -> crate::error::Error {
    crate::error::Error::new("torn_read", "Mac Find first row and first entry disagree")
}

fn first_row(r: &Reader<'_>, base: u64, gs: u64, find: u64) -> Result<Results> {
    let (_, data, count) = r.array_header(find + 0xc90)?;
    if count > 20_000 || (count > 0 && data % 8 != 0) {
        return Err(invalid("Bounded aligned Mac Find results required"));
    }
    // The refresh path may retain and reinsert the old row when the new
    // result count is zero. It is not a current first result; skip its pointer
    // and payload rather than mistaking this stable empty state for a tear.
    if count == 0 {
        return Ok(Results {
            count: 0,
            first: None,
        });
    }
    let row = r.u64(find + 0xc88)?;
    if row == 0 {
        return Ok(Results { count, first: None });
    }
    mac_ui::exact(r, base, row, ROW)?;
    let kind = r.i32(row + 0x408)?;
    let bytes = r.read(data, ENTRY_BYTES)?;
    let entry_kind = i32::from_le_bytes(bytes[0x38..0x3c].try_into().unwrap());
    if kind != entry_kind {
        return Err(refreshing());
    }
    let mut first = FirstResult {
        kind,
        object: None,
        state_id: None,
    };
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
        first.object = Some(object);
        // Other kinds are visible result metadata only. Never follow their
        // province/country payload to infer hidden information.
        if kind == 2 {
            let l = super::layout(r);
            mac_ui::exact(r, base, object, l.state_vtable)?;
            let id = r.i32(object + l.state_id)?;
            if location::catalog_pointer(r, gs + l.gs_states, id, 4096)? != object {
                return Err(invalid("Mac Find state must join its current catalog"));
            }
            first.state_id = Some(id);
        }
    }
    Ok(Results {
        count,
        first: Some(first),
    })
}

fn view(r: &Reader<'_>, base: u64, gs: u64, registry: u64, find: u64) -> Result<View> {
    let window = r.u64(find + 0xc38)?;
    let mut out = View {
        root: RootLifecycle::Unmaterialized,
        query: None,
        results: None,
        focus_requested: false,
        focus: None,
    };
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
    out.root = RootLifecycle::Materialized {
        pointer: window,
        object: root,
        flags,
        transitions: states,
    };
    // Hidden retained rows/queries cannot be current Find results. Avoid those
    // payloads entirely, including the stale first-row pointer.
    if visible {
        let text = r.string_bounded(find + 0xc48, 64)?;
        out.query = Some(if text.bytes().all(|b| (0x20..0x7f).contains(&b)) {
            Query::Printable(text)
        } else {
            Query::Unsupported { length: text.len() }
        });
        out.results = Some(first_row(r, base, gs, find)?);
    }
    Ok(out)
}

fn camera(r: &Reader<'_>, idler: u64) -> Result<CameraState> {
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
    Ok(CameraState {
        object,
        eye: values[..3].try_into().unwrap(),
        look_at: values[3..].try_into().unwrap(),
    })
}

fn target(r: &Reader<'_>, base: u64, gs: u64, own: i32, id: i32) -> Result<RequestedState> {
    let l = super::layout(r);
    let state = location::catalog_pointer(r, gs + l.gs_states, id, 4096)?;
    mac_ui::exact(r, base, state, l.state_vtable)?;
    if r.i32(state + l.state_id)? != id {
        return Err(invalid("Mac Find target state ID/catalog differs"));
    }
    if !location::own_state(r, base, state, own)? {
        return Ok(RequestedState::Foreign { id, state });
    }
    let template = r.u64(state + 0x20)?;
    mac_ui::exact(r, base, template, STATE_TEMPLATE)?;
    let x = r.i32(template + 0x120)?;
    let z = r.i32(template + 0x124)?;
    if x.abs_diff(0) > 1_000_000 || z.abs_diff(0) > 1_000_000 {
        return Err(invalid("Bounded Mac state map anchor required"));
    }
    Ok(RequestedState::Own(OwnStateAnchor {
        id,
        state,
        template,
        map_position: [x, z],
    }))
}

pub(super) fn snapshot(process: &dyn Process, state_id: Option<i32>) -> Result<Value> {
    observe(process, state_id)?.value()
}

pub(super) fn snapshot_with_focus(
    process: &dyn Process,
    state_id: Option<i32>,
    include_focus: bool,
) -> Result<Value> {
    if include_focus {
        retained::observe_with_focus(process, state_id)?.value()
    } else {
        snapshot(process, state_id)
    }
}

#[cfg(test)]
#[path = "../../tests/support/mac_construction_find.rs"]
mod tests;
