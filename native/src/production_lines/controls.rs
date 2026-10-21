//! Linux ELF a4971c9e own production-row ordinary control observations.
//! Independently recovered Linux constructors/callbacks; no engine calls/input.
use crate::{
    economy,
    error::{Error, Result},
    memory::{Reader, StringAbi},
    node_reuse::Reuse,
    research_ui::exact as exact_primary,
};
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet};
#[path = "controls_geometry.rs"]
pub(crate) mod geometry_trace;

// The topbar callback 0x186d0d0 passes 2 to manager 0x1709c20, which
// selects manager+0xc8+2*8 and writes active mode+0x3d8. Constructor argument
// 5 to the view base is a different field, not this manager's active mode.
pub(crate) const MANAGED_MODE: i32 = 2;

/// Pointer identities remain internal to the request/controller context.
pub(crate) struct Observation {
    pub(crate) value: Value,
    pub(crate) scope: Vec<u64>,
    pub(crate) geometry: BTreeMap<String, Vec<(u64, Vec<u8>)>>,
}
fn invalid(message: &str) -> Error {
    Error::new("invalid_production_controls", message)
}
fn pointer(p: u64) -> Result<()> {
    if !(0x10000..=0x7fff_ffff_0000).contains(&p) || p % 8 != 0 {
        return Err(invalid("Bounded aligned production UI pointer required"));
    }
    Ok(())
}
fn exact(r: &Reader<'_>, p: u64, primary: u64) -> Result<()> {
    pointer(p)?;
    exact_primary(r, p, primary)
}
fn members(r: &Reader<'_>, address: u64, bound: usize) -> Result<Vec<u64>> {
    let (header, data, count) = r.array_header(address)?;
    let capacity = i32::from_le_bytes(header[8..12].try_into().unwrap());
    if capacity < 0 || capacity as usize > bound || count > bound {
        return Err(invalid("Production UI pointer array exceeds bound"));
    }
    if count == 0 {
        return Ok(Vec::new());
    }
    pointer(data)?;
    let bytes = r.read(data, count * 8)?;
    let mut seen = BTreeSet::new();
    bytes
        .chunks_exact(8)
        .map(|b| {
            let p = u64::from_le_bytes(b.try_into().unwrap());
            pointer(p)?;
            if !seen.insert(p) {
                return Err(invalid("Duplicate production UI member"));
            }
            Ok(p)
        })
        .collect()
}
fn window(r: &Reader<'_>, base: u64, p: u64, registry: u64) -> Result<Value> {
    exact(r, p, base + 0x4347d58)?;
    if r.u64(p + 0x98)? != registry {
        return Err(invalid("Production container must belong to own registry"));
    }
    let flags = r.byte(p + 0xa5)?;
    let states = [r.i32(p + 0xeb0)?, r.i32(p + 0xef8)?];
    let settled = states == [0, 0];
    Ok(json!({"materialized":true,"literal_shown":flags&8!=0,
        "settled":settled,"locally_shown_settled":settled&&flags&8!=0,
        "locally_hidden_settled":settled&&flags&8==0,
        "flags_raw":flags,"states_raw":states,
        "effective_routing_verified":false}))
}
fn modal(
    r: &Reader<'_>,
    base: u64,
    p: u64,
    idler: u64,
    view: u64,
    registry: u64,
    scope: &mut Vec<u64>,
) -> Result<Value> {
    if p == 0 {
        return Ok(json!({"available":false,"locally_hidden_settled":false,
            "reason":"selection_unmaterialized"}));
    }
    exact(r, p, base + 0x4321298)?;
    if r.u64(p + 0x7f0)? != idler || r.u64(p + 0x7f8)? != view {
        return Err(invalid("Production equipment selection owner changed"));
    }
    scope.push(p);
    let root = r.u64(p + 0x800)?;
    if root == 0 {
        return Ok(json!({"available":false,"locally_hidden_settled":false,
            "reason":"selection_root_unmaterialized"}));
    }
    scope.push(root);
    let mut out = window(r, base, root, registry)?;
    out["available"] = json!(true);
    Ok(out)
}
#[allow(clippy::too_many_arguments)]
fn control(
    r: &Reader<'_>,
    base: u64,
    row: u64,
    container: u64,
    registry: u64,
    name: &str,
    alias: u64,
    geometry: bool,
    scope: &mut Vec<u64>,
    transcript: &mut Vec<(u64, Vec<u8>)>,
) -> Result<Value> {
    let controls = members(r, container + 0x9a0, 256)?;
    let named = crate::gui_registry::lookup(r, container + 0x9b8, &controls, name, 128)?;
    let mut out = json!({"binding_available":false,"reason":"named_control_unavailable",
        "local_dispatch_ready":false,"input_authorized":false,"effective_routing_verified":false});
    let Some(button) = named else { return Ok(out) };
    if r.u64(row + alias)? != button {
        return Err(invalid(
            "Named factory control must equal constructor alias",
        ));
    }
    // The callback selects direction/step by pointer identity across all six
    // aliases. A named1x control must not also occupy another branch's alias.
    for other in [0x1000, 0x1008, 0x1010, 0x1018, 0x1020, 0x1028] {
        if other != alias && r.u64(row + other)? == button {
            return Err(invalid("Factory control aliases must be distinct"));
        }
    }
    scope.push(button);
    if r.u64(button)? != base + 0x43470c8 {
        out["reason"] = json!("factory_control_layout_unavailable");
        return Ok(out);
    }
    if r.u64(button + 0x68)? != registry {
        return Err(invalid("Factory Standard must belong to own registry"));
    }
    let state = r.i32(button + 0xac)?;
    if !(0..=3).contains(&state) {
        return Err(invalid("Factory Standard state outside reviewed encoding"));
    }
    let flags = u32::from_le_bytes(r.read(button + 0x74, 4)?.try_into().unwrap());
    out["disabled_raw"] = json!(state);
    out["disabled_state3"] = json!(state == 3);
    out["raw_flags"] = json!(flags);
    out["literal_shown"] = json!(flags & 0x800 != 0);
    let optional_null = r.u64(button + 0x1a8)? == 0;
    let blocked = r.byte(button + 0x201)?;
    let delegate_null = r.u64(button + 0xd0)? == 0;
    let dispatch_ready = matches!(state, 0 | 2)
        && flags & 0xc00 == 0x800
        && optional_null
        && blocked == 0
        && delegate_null;
    out["local_dispatch_ready"] = json!(dispatch_ready);
    out["local"] = json!({"optional_gate_1a8_null":optional_null,
        "block201_raw":blocked,"delegate_d0_null":delegate_null});
    if r.u64(button + 0x80)? != base + 0x4347488 || r.u64(button + 0xa0)? != 0 {
        out["reason"] = json!("factory_observer_override_or_layout_unavailable");
        return Ok(out);
    }
    let count = r.i32(button + 0x98)?;
    if !(0..=16).contains(&count) {
        return Err(invalid("Factory listener count exceeds bound"));
    }
    if count != 1 {
        out["reason"] = json!("factory_listener_route_ambiguous");
        return Ok(out);
    }
    let head = r.u64(button + 0x88)?;
    pointer(head)?;
    if r.u64(button + 0x90)? != head
        || r.u64(head + 8)? != 0
        || r.u64(head + 0x10)? != 0
        || r.byte(head + 0x18)? != 0
    {
        return Err(invalid("Malformed factory ordinary listener"));
    }
    let glue = row + 0xaa8;
    scope.extend([head, glue]);
    if r.u64(head)? != glue
        || r.u64(glue)? != base + 0x4266748
        || r.u64(glue + 0x18)? != 0
        || r.u64(glue + 0x1d8)? != 0
        || r.u64(glue + 0x38)? != base + 0x18157d0
        || r.u64(glue + 0x40)? != base + 0x18157a0
    {
        out["reason"] = json!("factory_callback_variant_unavailable");
        return Ok(out);
    }
    // Only the normal GUI-control tuple28/38/40 is active. The remaining
    // bundles include the kind9 cancellation manager278 and all selectors.
    let inactive = r.read(glue + 8, 0x20)?.iter().all(|v| *v == 0)
        && r.read(glue + 0x48, 0x240)?.iter().all(|v| *v == 0);
    out["inactive_bundle_zero"] = json!(inactive);
    if !inactive {
        out["reason"] = json!("alternate_factory_callback_bundle");
        return Ok(out);
    }
    let capture = r.u64(glue + 0x28)?;
    pointer(capture)?;
    scope.push(capture);
    if r.u64(capture)? != base + 0x17e0c60
        || r.u64(capture + 8)? != 0
        || r.u64(capture + 0x10)? != row
    {
        out["reason"] = json!("factory_callback_capture_unavailable");
        return Ok(out);
    }
    out["binding_available"] = json!(true);
    out["reason"] = json!("exact_ordinary_factory_control");
    if geometry && dispatch_ready {
        let render = r.u64(button + 0xe8)?;
        if render == 0 {
            out["sprite_diagnostic"] =
                json!({"available":false,"reason":"renderer_unmaterialized"});
        } else {
            pointer(render)?;
            let primary = r.u64(render)?;
            if ![0x4356e08, 0x43593d8, 0x434b370]
                .iter()
                .any(|p| primary == base + p)
            {
                out["sprite_diagnostic"] =
                    json!({"available":false,"reason":"renderer_layout_unavailable"});
                return Ok(out);
            }
            let render_owner = r.u64(render + 0x138)?;
            if render_owner != button {
                return Err(invalid("Factory renderer must belong to bound Standard"));
            }
            scope.push(render);
            let trace = geometry_trace::Transcript::new(r);
            trace.seed(button + 0xe8, render.to_le_bytes().to_vec())?;
            trace.seed(render, primary.to_le_bytes().to_vec())?;
            trace.seed(render + 0x138, render_owner.to_le_bytes().to_vec())?;
            let geometry_reader = Reader::new(&trace);
            let mut reuse = Reuse::new(base, 0);
            out["sprite_diagnostic"] = crate::focus_geometry::observe(
                &geometry_reader,
                &mut reuse,
                container,
                button,
                registry,
                primary,
                render,
            )?;
            *transcript = trace.finish();
        }
    }
    Ok(out)
}

/// Caller must supply a same-reader, positively owned canonical target line.
/// No absent/null root is classified hidden; no public pointer is returned.
pub(crate) fn observe(
    r: &Reader<'_>,
    base: u64,
    line: &super::TargetLine,
    geometry: bool,
) -> Result<Observation> {
    if r.string_abi() != StringAbi::LibStdCxx {
        return Err(Error::new(
            "capability_unavailable",
            "Production control layout requires reviewed Linux build",
        ));
    }
    pointer(line.address)?;
    let (_, production) = economy::own_production(r, base)?;
    exact(
        r,
        line.address,
        base + if line.naval { 0x42edf88 } else { 0x42ede08 },
    )?;
    if r.u64(line.address + 0x20)? != production
        || r.u64(line.address + 8)? != line.handle
        || r.i32(line.address + 0xe8)? != line.requested
    {
        return Err(invalid(
            "Own production target changed before UI association",
        ));
    }
    let idler = r.u64(base + 0x44085f8)?;
    exact(r, idler, base + 0x42c9a88)?;
    let registry = r.u64(idler + 0x4f8)?;
    exact(r, registry, base + 0x42736f8)?;
    let manager = r.u64(idler + 0x6b8)?;
    pointer(manager)?;
    if r.u64(manager + 0x180)? != idler {
        return Err(invalid("Own production manager required"));
    }
    let mode = r.i32(manager + 0x3d8)?;
    let mut scope = vec![line.address, production, idler, registry, manager];
    let mut geometry_reads = BTreeMap::new();
    let mut out = json!({"available":false,"reason":"production_view_unmaterialized",
        "equipment_id":line.equipment_id,"requested_factories":line.requested,
        "mode_raw":mode,"locally_ready":false,"controls":{},
        "global_modal_clear_verified":false,"effective_routing_verified":false,
        "input_authorized":false,"live_validation":"pending"});
    let view = r.u64(manager + 0xd8)?;
    if view == 0 {
        return Ok(Observation {
            value: out,
            scope,
            geometry: geometry_reads,
        });
    }
    exact(r, view, base + 0x4314180)?;
    if r.u64(view + 0x2f0)? != idler {
        return Err(invalid("Production view must belong to own idler"));
    }
    scope.push(view);
    let root = r.u64(view + 0x2f8)?;
    if root == 0 {
        out["reason"] = json!("production_root_unmaterialized");
        return Ok(Observation {
            value: out,
            scope,
            geometry: geometry_reads,
        });
    }
    scope.push(root);
    out["root"] = window(r, base, root, registry)?;
    let mut blockers = Vec::new();
    for offset in [0x1530, 0x1538] {
        let dialog = r.u64(view + offset)?;
        blockers.push(modal(r, base, dialog, idler, view, registry, &mut scope)?);
    }
    let scoped_clear = blockers.iter().all(|b| b["locally_hidden_settled"] == true);
    out["scoped_blockers"] = json!({"available":scoped_clear,
        "production_equipment_selections_hidden_settled":scoped_clear,"selections":blockers});
    let rows = members(r, view + 0x14f0, 2048)?;
    let mut found = None;
    for row in rows {
        let primary = r.u64(row)?;
        if primary != base + 0x4264f48 && primary != base + 0x4265090 {
            continue;
        }
        if r.u64(row + 0x538)? != registry || r.u64(row + 0x590)? != view {
            return Err(invalid("Current production row owner changed"));
        }
        if r.u64(row + 0xfd0)? != line.handle {
            continue;
        }
        if (primary == base + 0x4265090) != line.naval || found.replace(row).is_some() {
            return Err(invalid("Unique matching typed production row required"));
        }
    }
    let Some(row) = found else {
        out["reason"] = json!("own_production_row_unmaterialized");
        return Ok(Observation {
            value: out,
            scope,
            geometry: geometry_reads,
        });
    };
    scope.push(row);
    let container = r.u64(row + 0x10)?;
    scope.push(container);
    out["row"] = window(r, base, container, registry)?;
    let ready = mode == MANAGED_MODE
        && scoped_clear
        && out["root"]["locally_shown_settled"] == true
        && out["row"]["locally_shown_settled"] == true;
    out["locally_ready"] = json!(ready);
    let mut add_geometry = Vec::new();
    let mut subtract_geometry = Vec::new();
    let add = control(
        r,
        base,
        row,
        container,
        registry,
        "factories_add_1x",
        0x1018,
        geometry && ready,
        &mut scope,
        &mut add_geometry,
    )?;
    let subtract = control(
        r,
        base,
        row,
        container,
        registry,
        "factories_sub_1x",
        0x1000,
        geometry && ready,
        &mut scope,
        &mut subtract_geometry,
    )?;
    if !add_geometry.is_empty() {
        geometry_reads.insert("add".into(), add_geometry);
    }
    if !subtract_geometry.is_empty() {
        geometry_reads.insert("subtract".into(), subtract_geometry);
    }
    out["available"] = json!(true);
    out["reason"] = json!("exact_own_materialized_production_row");
    out["controls"] = json!({"add":add,"subtract":subtract});
    Ok(Observation {
        value: out,
        scope,
        geometry: geometry_reads,
    })
}

#[cfg(test)]
#[path = "controls_tests.rs"]
mod tests;
