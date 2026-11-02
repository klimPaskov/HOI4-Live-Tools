//! Independently recovered Linux production-panel/topbar observations only.
//! Managed production mode is2; constructor5 is update-interface flags.
use crate::production_lines::controls::geometry_trace;
use crate::{
    economy,
    error::{Error, Result},
    game,
    memory::{Reader, StringAbi},
    node_reuse::Reuse,
    platform::Process,
    research_ui::Witnesses,
};
use serde_json::{json, Value};
use std::collections::BTreeSet;
#[cfg(test)]
#[path = "production_panel/tests.rs"]
mod tests;
/// Diagnose only the already witnessed candidate. No additional reads, input,
/// or arbitrary observation strings/addresses enter the error message.
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
pub(crate) fn require_open_candidate(value: &Value, geometry_present: bool) -> Result<()> {
    require_candidate(value, geometry_present, false)
}
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
pub(crate) fn require_close_candidate(value: &Value, geometry_present: bool) -> Result<()> {
    require_candidate(value, geometry_present, true)
}
fn require_candidate(value: &Value, geometry_present: bool, closing: bool) -> Result<()> {
    let preconditions = if closing {
        "close_click_preconditions_clear"
    } else {
        "new_click_preconditions_clear"
    };
    let gates = [
        ("build", value["build_id"] == game::BUILD_ID),
        ("preconditions", value[preconditions] == true),
        ("opener", value["opener"]["local_dispatch_ready"] == true),
        ("geometry", geometry_present),
    ];
    let failed: Vec<_> = gates
        .iter()
        .filter(|(_, clear)| !clear)
        .map(|(name, _)| *name)
        .collect();
    if failed.is_empty() {
        return Ok(());
    }
    // Typed projections distinguish absent/unknown from a literal false. Keep
    // this bounded even if a future producer supplies malformed diagnostics.
    let mut details = serde_json::Map::new();
    for (name, pointer) in [
        ("panel_hidden", "/root/locally_hidden_settled"),
        ("panel_shown", "/root/locally_shown_settled"),
        ("engine", "/engine_gates/conservative_engine_clear"),
        ("engine_known", "/engine_gates/known_gates_clear"),
        ("settings", "/engine_gates/settings/predicate_clear"),
        ("menu", "/engine_gates/menu/predicate_clear"),
        ("overview", "/engine_gates/overview/predicate_clear"),
        ("auxiliary_present", "/engine_gates/auxiliary_present"),
        (
            "selections_hidden",
            "/production_selections/production_equipment_selections_hidden_settled",
        ),
        ("common", "/common_blockers/conservative_scoped_clear"),
        (
            "politics_visible",
            "/common_blockers/politics_root/settled_visible",
        ),
        ("focus_hidden", "/common_blockers/focus_root_settled_hidden"),
        (
            "research_visible",
            "/common_blockers/research_panel_settled_visible",
        ),
        (
            "research_tree_visible",
            "/common_blockers/research_tree_settled_visible",
        ),
        ("gui", "/common_gui_gates_clear"),
        ("opener_available", "/opener/available"),
        ("named_path", "/opener/named_path_verified"),
        (
            "containers_shown",
            "/opener/all_named_containers_settled_shown",
        ),
        ("callback", "/opener/callback/available"),
        ("optional_gate", "/opener/local/optional_gate/clear"),
        ("delegate_null", "/opener/local/delegate_d0_null"),
        ("renderer_available", "/opener/render_diagnostic/available"),
    ] {
        details.insert(
            name.into(),
            json!(value.pointer(pointer).and_then(Value::as_bool)),
        );
    }
    for (name, pointer) in [
        ("mode", "/mode_raw"),
        ("game_ready_raw", "/engine_gates/gs_a59_raw"),
        ("owner_gate_raw", "/engine_gates/idler_8f0_raw"),
        ("can_update_raw", "/engine_gates/idler_can_update_raw"),
        ("opener_flags", "/opener/local/flags74_raw"),
        ("opener_state", "/opener/local/state_ac_raw"),
        ("opener_block", "/opener/local/block201_raw"),
    ] {
        details.insert(
            name.into(),
            json!(value.pointer(pointer).and_then(Value::as_i64)),
        );
    }
    for (name, pointer) in [
        (
            "manager_transition",
            "/engine_gates/manager_transition_bytes",
        ),
        (
            "focus_details",
            "/common_blockers/focus_detail_visible_animating_counts",
        ),
        (
            "research_info",
            "/common_blockers/own_research_info_visible_animating_counts",
        ),
    ] {
        let pair = value
            .pointer(pointer)
            .and_then(Value::as_array)
            .filter(|v| v.len() == 2)
            .and_then(|v| Some([v[0].as_i64()?, v[1].as_i64()?]));
        details.insert(name.into(), json!(pair));
    }
    Err(Error::new(
        "production_precondition",
        format!(
            "Production {} refused before input; failed={}; observed={}",
            if closing { "closing" } else { "opening" },
            failed.join(","),
            Value::Object(details)
        ),
    ))
}
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
pub(crate) struct Observation {
    pub(crate) value: Value,
    pub(crate) campaign: game::ClockContext,
    pub(crate) identity: Vec<u64>,
    pub(crate) scope: Vec<u64>,
    pub(crate) geometry: Vec<(u64, Vec<u8>)>,
}
fn invalid(message: &str) -> Error {
    Error::new("invalid_production_panel", message)
}
fn pointer(p: u64) -> Result<()> {
    if !(0x10000..=0x7fff_ffff_0000).contains(&p) || p % 8 != 0 {
        return Err(invalid("Bounded aligned production panel pointer required"));
    }
    Ok(())
}
fn exact(r: &Reader<'_>, p: u64, primary: u64) -> Result<()> {
    pointer(p)?;
    if r.u64(p)? != primary {
        return Err(invalid("Exact production panel primary required"));
    }
    Ok(())
}
fn unavailable(reason: &str) -> Value {
    json!({"available":false,"reason":reason,"input_authorized":false,"effective_routing_verified":false,"current_frame_verified":false})
}
fn members(r: &Reader<'_>, address: u64) -> Result<Vec<u64>> {
    let (h, data, n) = r.array_header(address)?;
    let cap = i32::from_le_bytes(h[8..12].try_into().unwrap());
    if !(0..=256).contains(&cap) || n > 256 {
        return Err(invalid("Production named collection exceeds bound"));
    }
    if n == 0 {
        return Ok(Vec::new());
    }
    pointer(data)?;
    let bytes = r.read(data, n * 8)?;
    let mut seen = BTreeSet::new();
    bytes
        .chunks_exact(8)
        .map(|b| {
            let p = u64::from_le_bytes(b.try_into().unwrap());
            pointer(p)?;
            if !seen.insert(p) {
                return Err(invalid("Duplicate production named member"));
            }
            Ok(p)
        })
        .collect()
}
fn named(r: &Reader<'_>, window: u64, children: bool, key: &str) -> Result<Option<u64>> {
    let (values, names) = if children {
        (0xbb0, 0xbc8)
    } else {
        (0x9a0, 0x9b8)
    };
    crate::gui_registry::lookup(r, window + names, &members(r, window + values)?, key, 128)
}
fn window(r: &Reader<'_>, base: u64, p: u64, registry: u64) -> Result<Value> {
    exact(r, p, base + 0x4347d58)?;
    if r.u64(p + 0x98)? != registry {
        return Err(invalid("Production window must belong to own registry"));
    }
    let flags = r.byte(p + 0xa5)?;
    let states = [r.i32(p + 0xeb0)?, r.i32(p + 0xef8)?];
    let settled = states == [0, 0];
    Ok(
        json!({"materialized":true,"flags_raw":flags,"states_raw":states,"settled":settled,"literal_shown":flags&8!=0,"locally_shown_settled":settled&&flags&8!=0,"locally_hidden_settled":settled&&flags&8==0}),
    )
}
/// Exact predicates2099c80/171d3e0/171d4a0 explicitly returnfalse on null.
/// This is an engine predicate branch, never a claim of a materialized Hidden.
fn engine_window(
    r: &Reader<'_>,
    base: u64,
    root: u64,
    registry: u64,
    scope: &mut Vec<u64>,
) -> Result<Value> {
    if root == 0 {
        return Ok(
            json!({"predicate_clear":true,"materialized":false,"reason":"engine_false_by_null"}),
        );
    }
    scope.push(root);
    let mut v = window(r, base, root, registry)?;
    v["predicate_clear"] = json!(v["locally_hidden_settled"] == true);
    Ok(v)
}
fn selections(
    r: &Reader<'_>,
    base: u64,
    view: u64,
    idler: u64,
    registry: u64,
    scope: &mut Vec<u64>,
) -> Result<Value> {
    let mut items = Vec::new();
    for off in [0x1530, 0x1538] {
        let p = r.u64(view + off)?;
        if p == 0 {
            items.push(unavailable("selection_unmaterialized"));
            continue;
        }
        exact(r, p, base + 0x4321298)?;
        if r.u64(p + 0x7f0)? != idler || r.u64(p + 0x7f8)? != view {
            return Err(invalid("Production selection owner changed"));
        }
        scope.push(p);
        let root = r.u64(p + 0x800)?;
        if root == 0 {
            items.push(unavailable("selection_root_unmaterialized"));
            continue;
        }
        scope.push(root);
        items.push(window(r, base, root, registry)?);
    }
    Ok(
        json!({"production_equipment_selections_hidden_settled":items.iter().all(|v|v["locally_hidden_settled"]==true),"selections":items}),
    )
}
#[allow(clippy::too_many_arguments)]
pub(crate) fn engine(
    r: &Reader<'_>,
    base: u64,
    gs: u64,
    idler: u64,
    manager: u64,
    registry: u64,
    scope: &mut Vec<u64>,
) -> Result<Value> {
    let manager_flags = [r.byte(manager + 0x3e1)?, r.byte(manager + 0x3e2)?];
    let gs_gate = r.byte(gs + 0xa59)?;
    let owner_gate = r.byte(idler + 0x8f0)?;
    // Internal GUI-update phase, set/cleared by engine-owned wrappers. A
    // sampled zero is normal between callbacks, not an external click veto.
    // Ordinary X11 delivery lets the engine enter its own callback phase;
    // this observer must never set the byte or invoke the callback directly.
    let can_update = r.byte(idler + 0x5b2)?;
    let settings = r.u64(idler + 0x7b0)?;
    let settings_state = if settings == 0 {
        json!({"predicate_clear":true,"reason":"engine_false_by_null"})
    } else {
        exact(r, settings, base + 0x431a2f0)?;
        if r.u64(settings + 0x30)? != idler {
            return Err(invalid("Settings blocker must belong to own idler"));
        }
        scope.push(settings);
        engine_window(r, base, r.u64(settings + 0x550)?, registry, scope)?
    };
    let menu = r.u64(manager + 0x218)?;
    let (menu_state, overview_state) = if menu == 0 {
        // No null-menu branch exists in1709c20: its two helpers dereference it.
        (
            unavailable("menu_owner_unmaterialized"),
            unavailable("menu_owner_unmaterialized"),
        )
    } else {
        exact(r, menu, base + 0x425bf80)?;
        if r.u64(menu + 0x19a0)? != idler || r.u64(menu + 0x19a8)? != manager {
            return Err(invalid("Menu blocker must belong to own manager"));
        }
        scope.push(menu);
        let local = engine_window(r, base, r.u64(menu + 0x19b0)?, registry, scope)?;
        let overview = r.u64(menu + 0x19c8)?;
        let state = if overview == 0 {
            json!({"predicate_clear":true,"reason":"engine_false_by_null"})
        } else {
            exact(r, overview, base + 0x4300540)?;
            if r.u64(overview + 0x1078)? != idler {
                return Err(invalid("Overview blocker must belong to own idler"));
            }
            scope.push(overview);
            engine_window(r, base, r.u64(overview + 0x1080)?, registry, scope)?
        };
        (local, state)
    };
    //179abf0's optional object's class/owner mapping remains unproved. Never
    // dereference its40/48 payload or classify its unknown window as Hidden.
    let auxiliary = r.u64(manager + 0x248)?;
    let known = manager_flags == [0, 0]
        && gs_gate != 0
        && owner_gate == 0
        && settings_state["predicate_clear"] == true
        && menu_state["predicate_clear"] == true
        && overview_state["predicate_clear"] == true;
    Ok(
        json!({"known_gates_clear":known,"conservative_engine_clear":known&&auxiliary==0,
        "complete_engine_eligibility_verified":false,"manager_transition_bytes":manager_flags,
        "gs_a59_raw":gs_gate,"idler_8f0_raw":owner_gate,"idler_can_update_raw":can_update,
        "idler_can_update_role":"internal_gui_update_phase_observation_only",
        "settings":settings_state,"menu":menu_state,"overview":overview_state,
        "auxiliary_present":auxiliary!=0,"auxiliary_predicate_clear":if auxiliary==0{Some(true)}else{None}}),
    )
}
fn callback(
    r: &Reader<'_>,
    base: u64,
    topbar: u64,
    button: u64,
    scope: &mut Vec<u64>,
) -> Result<Value> {
    let mut out = unavailable("opener_observer_unavailable");
    if r.u64(button + 0x80)? != base + 0x4347488 || r.u64(button + 0xa0)? != 0 {
        return Ok(out);
    }
    let n = r.i32(button + 0x98)?;
    if !(0..=16).contains(&n) {
        return Err(invalid("Opener listener bound"));
    }
    if n != 1 {
        return Ok(out);
    }
    let link = r.u64(button + 0x88)?;
    pointer(link)?;
    if r.u64(button + 0x90)? != link
        || r.u64(link + 8)? != 0
        || r.u64(link + 0x10)? != 0
        || r.byte(link + 0x18)? != 0
    {
        return Err(invalid("Malformed opener listener"));
    }
    let glue = topbar + 0xdb0;
    if r.u64(link)? != glue {
        return Ok(out);
    }
    scope.extend([link, glue]);
    if r.u64(glue)? != base + 0x426c518
        || r.u64(glue + 0x18)? != base + 0x1886560
        || r.u64(glue + 0x20)? != base + 0x1886530
    {
        return Ok(out);
    }
    if r.read(glue + 0x28, 0x260)?.iter().any(|b| *b != 0) {
        out["reason"] = json!("opener_alternate_callback_bundle");
        return Ok(out);
    }
    let capture = r.u64(glue + 8)?;
    pointer(capture)?;
    scope.push(capture);
    if r.u64(capture)? != base + 0x186d0d0
        || r.u64(capture + 8)? != 0
        || r.u64(capture + 0x10)? != topbar
    {
        return Ok(out);
    }
    out["available"] = json!(true);
    out["reason"] = json!("exact_production_first_void_callback");
    Ok(out)
}
/// Caller must bind the exact Standard in its validated own-registry Container.
/// All reads use the caller's snapshot witnesses; the joined position also
/// enters its pre-dispatch identity scope.
pub(crate) fn optional_gate(
    r: &Reader<'_>,
    base: u64,
    window: u64,
    button: u64,
    scope: &mut Vec<u64>,
) -> Result<Value> {
    let gate = r.u64(button + 0x1a8)?;
    if gate == 0 {
        return Ok(json!({"absent":true,"clear":true,"reason":"absent_optional_gate"}));
    }
    // Reuse the reviewed exact named-container E78 position join. Never read
    // an unrelated optional object's payload, even if it has a familiar type.
    if gate != r.u64(window + 0xe78)? {
        return Ok(json!({"absent":false,"clear":null,"reason":"unbound_optional_gate"}));
    }
    pointer(gate)?;
    if r.u64(gate)? != base + 0x43484f0 {
        return Ok(json!({"absent":false,"clear":null,"reason":"unsupported_owned_position_type"}));
    }
    let descend = match r.byte(gate + 0x128)? {
        0 => false,
        1 => true,
        _ => {
            return Err(invalid(
                "Optional position boolean outside reviewed encoding",
            ))
        }
    };
    scope.push(gate);
    Ok(
        json!({"absent":false,"clear":descend,"reason":"exact_named_window_e78_position",
        "own_position_join_verified":true,"descend_raw":descend}),
    )
}
#[allow(clippy::too_many_arguments)]
fn opener(
    r: &Reader<'_>,
    base: u64,
    idler: u64,
    manager: u64,
    registry: u64,
    geometry: bool,
    scope: &mut Vec<u64>,
    transcript: &mut Vec<(u64, Vec<u8>)>,
) -> Result<Value> {
    let topbar = r.u64(manager + 0x188)?;
    if topbar == 0 {
        return Ok(unavailable("topbar_unmaterialized"));
    }
    exact(r, topbar, base + 0x426c3d0)?;
    if r.u64(topbar + 0x68)? != idler || r.u64(topbar + 0x70)? != manager {
        return Err(invalid("Topbar must belong to own idler/manager"));
    }
    scope.push(topbar);
    let mut p = r.u64(topbar + 0x78)?;
    if p == 0 {
        return Ok(unavailable("topbar_root_unmaterialized"));
    }
    scope.push(p);
    let mut path = vec![window(r, base, p, registry)?];
    for key in [
        "country_menu_window",
        "after_faction_button",
        "after_intel_agency_button",
    ] {
        let Some(child) = named(r, p, true, key)? else {
            return Ok(unavailable("named_topbar_path_unavailable"));
        };
        if key == "country_menu_window" && r.u64(topbar + 0x98)? != child {
            return Err(invalid("Country menu alias differs"));
        }
        p = child;
        scope.push(p);
        path.push(window(r, base, p, registry)?);
    }
    let Some(button) = named(r, p, false, "production_button")? else {
        return Ok(unavailable("production_button_unavailable"));
    };
    if r.u64(topbar + 0x4dc0)? != button {
        return Err(invalid("Production opener named alias differs"));
    }
    scope.push(button);
    let mut out = unavailable("opener_standard_unavailable");
    out["named_path_verified"] = json!(true);
    out["all_named_containers_settled_shown"] =
        json!(path.iter().all(|v| v["locally_shown_settled"] == true));
    out["path"] = json!(path);
    if r.u64(button)? != base + 0x43470c8 {
        return Ok(out);
    }
    if r.u64(button + 0x68)? != registry {
        return Err(invalid("Opener Standard must belong to own registry"));
    }
    let flags = u32::from_le_bytes(r.read(button + 0x74, 4)?.try_into().unwrap());
    let state = r.i32(button + 0xac)?;
    if !(0..=3).contains(&state) {
        return Err(invalid("Opener state outside reviewed enum"));
    }
    let optional = optional_gate(r, base, p, button, scope)?;
    let blocked = r.byte(button + 0x201)?;
    let delegate = r.u64(button + 0xd0)?;
    let binding = callback(r, base, topbar, button, scope)?;
    let local = matches!(state, 0 | 2)
        && flags & 0xc00 == 0x800
        && optional["clear"] == true
        && blocked == 0
        && delegate == 0
        && binding["available"] == true
        && out["all_named_containers_settled_shown"] == true;
    out["available"] = json!(binding["available"] == true);
    out["reason"] = json!("named_production_opener");
    out["callback"] = binding;
    out["local_dispatch_ready"] = json!(local);
    out["local"] = json!({"flags74_raw":flags,"state_ac_raw":state,"optional_gate_1a8_null":optional["absent"],
        "optional_gate":optional,"block201_raw":blocked,"delegate_d0_null":delegate==0});
    if geometry && local {
        let render = r.u64(button + 0xe8)?;
        if render == 0 {
            out["render_diagnostic"] = unavailable("renderer_unmaterialized");
            return Ok(out);
        }
        pointer(render)?;
        let primary = r.u64(render)?;
        if ![0x4356e08, 0x43593d8, 0x434b370]
            .iter()
            .any(|v| primary == base + v)
        {
            out["render_diagnostic"] = unavailable("renderer_layout_unavailable");
            return Ok(out);
        }
        let owner = r.u64(render + 0x138)?;
        if owner != button {
            return Err(invalid("Opener renderer owner differs"));
        }
        scope.push(render);
        let trace = geometry_trace::Transcript::new(r);
        trace.seed(button + 0xe8, render.to_le_bytes().to_vec())?;
        trace.seed(render, primary.to_le_bytes().to_vec())?;
        trace.seed(render + 0x138, owner.to_le_bytes().to_vec())?;
        let reader = Reader::new(&trace);
        let mut reuse = Reuse::new(base, 0);
        out["render_diagnostic"] = crate::focus_geometry::observe(
            &reader, &mut reuse, p, button, registry, primary, render,
        )?;
        *transcript = trace.finish();
    }
    Ok(out)
}
pub(crate) fn observe(
    process: &dyn Process,
    include_opener: bool,
    include_geometry: bool,
) -> Result<Observation> {
    process.begin_snapshot()?;
    let witnesses = Witnesses::new(process);
    let r = Reader::new(&witnesses);
    if r.string_abi() != StringAbi::LibStdCxx {
        return Err(Error::new(
            "capability_unavailable",
            "Production panel requires reviewed Linux build",
        ));
    }
    let base = process.image_base();
    let (country, production) = economy::own_production(&r, base)?;
    let campaign = game::context_from_reader(&r, base)?;
    let idler = r.u64(base + 0x44085f8)?;
    exact(&r, idler, base + 0x42c9a88)?;
    let registry = r.u64(idler + 0x4f8)?;
    exact(&r, registry, base + 0x42736f8)?;
    let manager = r.u64(idler + 0x6b8)?;
    pointer(manager)?;
    if r.u64(manager + 0x180)? != idler {
        return Err(invalid("Production manager owner differs"));
    }
    let mode = r.i32(manager + 0x3d8)?;
    let view = r.u64(manager + 0xd8)?;
    let mut value = unavailable("production_view_unmaterialized");
    value["mode_raw"] = json!(mode);
    value["managed_target_mode"] = json!(2);
    let mut identity = Vec::new();
    let mut scope = Vec::new();
    let mut geometry = Vec::new();
    if view != 0 {
        exact(&r, view, base + 0x4314180)?;
        if r.u64(view + 0x2f0)? != idler {
            return Err(invalid("Production view owner differs"));
        }
        let root = r.u64(view + 0x2f8)?;
        if root == 0 {
            value["reason"] = json!("production_root_unmaterialized");
            value["root"] = json!({"materialized":false});
        } else {
            value["root"] = window(&r, base, root, registry)?;
            identity = vec![country, production, idler, registry, manager, view, root];
            if identity.iter().copied().collect::<BTreeSet<_>>().len() != 7 {
                return Err(invalid("Distinct own panel identities required"));
            }
            scope = identity.clone();
            value["available"] = json!(true);
            value["reason"] = json!("exact_own_production_panel");
            if include_opener {
                let engine = engine(
                    &r,
                    base,
                    campaign.anchors[0],
                    idler,
                    manager,
                    registry,
                    &mut scope,
                )?;
                let selections = selections(&r, base, view, idler, registry, &mut scope)?;
                let common = crate::construction::controls::blockers(
                    &r, base, country, idler, manager, registry,
                )?;
                if r.u64(base + 0x45778e0)? != registry {
                    return Err(invalid("Input GUI must equal own registry"));
                }
                let gui_clear = r.byte(registry + 0x150)? == 0
                    && r.byte(registry + 0x12)? == 0
                    && r.u64(registry + 0x158)? == 0
                    && r.u64(registry + 0x230)? == 0;
                let clear = engine["conservative_engine_clear"] == true
                    && selections["production_equipment_selections_hidden_settled"] == true
                    && common["conservative_scoped_clear"] == true
                    && gui_clear;
                let ready = clear && mode == 23 && value["root"]["locally_hidden_settled"] == true;
                let close_ready =
                    clear && mode == 2 && value["root"]["locally_shown_settled"] == true;
                value["engine_gates"] = engine;
                value["production_selections"] = selections;
                value["common_blockers"] = common;
                value["common_gui_gates_clear"] = json!(gui_clear);
                value["opener"] = opener(
                    &r,
                    base,
                    idler,
                    manager,
                    registry,
                    include_geometry && (ready || close_ready),
                    &mut scope,
                    &mut geometry,
                )?;
                value["new_click_preconditions_clear"] =
                    json!(ready && value["opener"]["local_dispatch_ready"] == true);
                value["close_click_preconditions_clear"] =
                    json!(close_ready && value["opener"]["local_dispatch_ready"] == true);
                if include_geometry {
                    value["input_geometry"] = crate::gui::input_geometry_from_reader(&r, base)?;
                }
            }
        }
    }
    witnesses.finish()?;
    process.check()?;
    value["build_id"] = json!(game::layout(&r).build_id);
    value["source"] = json!("native own production panel");
    value["observation_paused_game"] = json!(false);
    value["live_validation"] = json!("pending");
    Ok(Observation {
        value,
        campaign,
        identity,
        scope,
        geometry,
    })
}
