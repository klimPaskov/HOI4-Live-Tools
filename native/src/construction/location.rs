//! Optional selected-building location joins for the exact Linux build.
//! Only territorial statuses with own ownership AND control are supported.
//! No foreign payload, national override, railway path or engine call.
use super::{invalid, pointer};
use crate::{error::Result, memory::Reader};
use serde_json::{json, Value};

pub(super) fn unavailable(reason: &str) -> Value {
    json!({"available":false,"reason":reason,"input_authorized":false})
}
pub(super) fn exact(r: &Reader<'_>, address: u64, primary: u64) -> Result<()> {
    pointer(address)?;
    if r.u64(address)? != primary {
        return Err(invalid("Exact construction location primary required"));
    }
    Ok(())
}
fn entry(r: &Reader<'_>, header: u64, index: i32, stride: usize, bound: usize) -> Result<Vec<u8>> {
    let (bytes, data, count) = r.array_header(header)?;
    let capacity = i32::from_le_bytes(bytes[8..12].try_into().unwrap()) as usize;
    if index < 0 || index as usize >= count || capacity > bound || data % stride as u64 != 0 {
        return Err(invalid(
            "Construction location index/capacity/alignment policy",
        ));
    }
    // Read exactly the selected entry; never enumerate a geographic catalog.
    r.read(data + index as u64 * stride as u64, stride)
}
pub(super) fn catalog_pointer(r: &Reader<'_>, header: u64, id: i32, bound: usize) -> Result<u64> {
    if id <= 0 {
        return Err(invalid("Positive construction location ID required"));
    }
    Ok(u64::from_le_bytes(
        entry(r, header, id, 8, bound)?.try_into().unwrap(),
    ))
}
pub(super) fn own_state(r: &Reader<'_>, base: u64, state: u64, tag: i32) -> Result<bool> {
    let l = super::layout(r);
    exact(r, state, base + l.state_vtable)?;
    // Authorization fields precede state ID or any reciprocal building data.
    Ok(r.i32(state + l.state_owner)? == tag && r.i32(state + l.state_controller)? == tag)
}
pub(super) fn observe(
    r: &Reader<'_>,
    base: u64,
    own_tag: i32,
    building: u64,
    template: u64,
    ordinal: usize,
) -> Result<Value> {
    let l = super::layout(r);
    let status = r.u64(building + l.building_status)?;
    if status == 0 {
        return Ok(unavailable("building_status_absent"));
    }
    exact(r, status, base + l.status_vtable)?;
    let flags = r.i32(status + 0x68)?;
    if flags < 0 {
        return Ok(unavailable("unsupported_location_status_flags"));
    }
    if r.i32(status + 0x70)? != 0 {
        return Ok(unavailable("national_location_override_unsupported"));
    }
    // Constructors store exactly 2 (state) or 1 (province) and the engine tests
    // only bit 0; Run089 found a queued state building whose live status carried
    // other bits. Bit 0 picks the kind; the exact embedded status address,
    // catalog ID join and reciprocal building slot are the proof. A failed proof
    // stays a hard error only for the constructor values.
    let constructor_flags = flags == 1 || flags == 2;
    match joined(r, base, own_tag, building, template, ordinal, status, flags) {
        Err(error) if !constructor_flags && error.code == "invalid_construction" => {
            let mut value = unavailable("unproven_location_status_kind");
            value["status_flags_raw"] = json!(flags);
            Ok(value)
        }
        result => result,
    }
}
#[allow(clippy::too_many_arguments)]
fn joined(
    r: &Reader<'_>,
    base: u64,
    own_tag: i32,
    building: u64,
    template: u64,
    ordinal: usize,
    status: u64,
    flags: i32,
) -> Result<Value> {
    let l = super::layout(r);
    let g = crate::game::layout(r);
    let id = r.i32(status + 0x6c)?;
    let gs = r.u64(base + g.gs_global)?;
    exact(r, gs, base + g.gs_vtable)?;
    let (state, province) = if flags & 1 == 0 {
        (catalog_pointer(r, gs + l.gs_states, id, 4096)?, None)
    } else {
        let province = catalog_pointer(r, gs + l.gs_provinces, id, 20000)?;
        exact(r, province, base + l.province_vtable)?;
        if r.i32(province + l.province_owner)? != own_tag {
            return Ok(unavailable("location_outside_own_control"));
        }
        (r.u64(province + l.province_state)?, Some(province))
    };
    if !own_state(r, base, state, own_tag)? {
        return Ok(unavailable("location_outside_own_ownership_or_control"));
    }
    let state_id = r.i32(state + l.state_id)?;
    if catalog_pointer(r, gs + l.gs_states, state_id, 4096)? != state {
        return Err(invalid(
            "Construction state ID must join its current catalog",
        ));
    }
    let province_id = if let Some(province) = province {
        if r.i32(province + l.province_id)? != id || status != province + l.province_status {
            return Err(invalid("Construction province ID/embedded status mismatch"));
        }
        Some(id)
    } else {
        if state_id != id || status != state + l.state_status {
            return Err(invalid("Construction state ID/embedded status mismatch"));
        }
        None
    };
    // The template ordinal is distinct from its interned key. Require its
    // exact position in the fresh ordered catalog after territorial authority.
    let template_index = r.i32(template + l.template_ordinal)?;
    if template_index < 0 || template_index as usize != ordinal {
        return Err(invalid(
            "Construction template ordinal must match fresh catalog",
        ));
    }
    let slot = i32::from_le_bytes(
        entry(r, status + 0x20, template_index, 4, 4096)?
            .try_into()
            .unwrap(),
    );
    let selected = u64::from_le_bytes(entry(r, status + 0x38, slot, 8, 4096)?.try_into().unwrap());
    if selected != building {
        return Err(invalid(
            "Construction location must reciprocally contain building",
        ));
    }
    Ok(
        json!({"available":true,"state_id":state_id,"province_id":province_id,
        "kind":if province.is_some(){"province"}else{"state"},"status_flags_raw":flags,
        "scope":"own_owned_and_controlled_territory",
        "identity":"current_catalog_ids_and_embedded_building_status",
        "input_authorized":false}),
    )
}
