//! Combined own construction opener/choice diagnostics, never an input grant.
//! Exact current Linux bindings independently recovered; see PROVENANCE.md.
use super::{invalid, members, pointer, selection};
use crate::{
    economy, error::Result, game, memory::Reader, platform::Process, research_ui::Witnesses,
};
use serde_json::{json, Value};
use std::{
    collections::{BTreeMap, BTreeSet},
    time::Instant,
};
mod binding;
#[cfg_attr(not(any(target_os = "linux", test)), allow(dead_code))]
mod scope;
pub(crate) use scope::Scope;

#[cfg_attr(not(any(target_os = "linux", test)), allow(dead_code))]
pub(crate) struct Observation {
    pub(crate) value: Value,
    pub(crate) campaign: game::ClockContext,
    scopes: scope::Scopes,
}
#[cfg_attr(not(any(target_os = "linux", test)), allow(dead_code))]
impl Observation {
    pub(crate) fn scope(&self, stage: crate::construction_stage::Stage) -> Result<Scope> {
        self.scopes.scope(stage)
    }
}

fn exact(r: &Reader<'_>, address: u64, primary: u64) -> Result<()> {
    pointer(address)?;
    if r.u64(address)? != primary {
        return Err(invalid("Exact construction control owner/type required"));
    }
    Ok(())
}
fn unavailable(reason: &str) -> Value {
    json!({"available":false,"reason":reason,"input_authorized":false,
        "effective_routing_verified":false,"current_frame_verified":false,"global_modal_clear_verified":false})
}
pub fn validate_key(key: &str) -> Result<()> {
    if key.is_empty()
        || key.len() > 128
        || !key.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_')
    {
        return Err(crate::error::Error::new(
            "invalid_arguments",
            "Bounded canonical building_id required",
        ));
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn opener(
    r: &Reader<'_>,
    base: u64,
    idler: u64,
    manager: u64,
    registry: u64,
    geometry: bool,
    trace: &scope::Recorder<'_>,
    identity: &mut Option<scope::Opener>,
) -> Result<Value> {
    let topbar = r.u64(manager + 0x188)?;
    if topbar == 0 {
        return Ok(unavailable("topbar_unmaterialized"));
    }
    exact(r, topbar, base + 0x426c3d0)?;
    if r.u64(topbar + 0x68)? != idler || r.u64(topbar + 0x70)? != manager {
        return Err(invalid("Topbar must belong to own idler and manager"));
    }
    let mut window = r.u64(topbar + 0x78)?;
    let mut path = vec![binding::local(r, base, window, registry)?];
    let mut named_path = vec![window];
    if window == 0 {
        return Ok(unavailable("topbar_root_unmaterialized"));
    }
    for name in [
        "country_menu_window",
        "after_faction_button",
        "after_intel_agency_button",
    ] {
        let Some(child) = binding::named(r, window, true, name)? else {
            let mut out = unavailable("named_topbar_container_unavailable");
            out["missing_name"] = json!(name);
            out["path"] = json!(path);
            return Ok(out);
        };
        if name == "country_menu_window" && r.u64(topbar + 0x98)? != child {
            return Err(invalid("Named country menu must equal topbar alias"));
        }
        let mut local = binding::local(r, base, child, registry)?;
        local["name"] = json!(name);
        path.push(local);
        window = child;
        named_path.push(child);
    }
    let Some(button) = binding::named(r, window, false, "construction_button")? else {
        return Ok(unavailable("construction_button_unavailable"));
    };
    if r.u64(topbar + 0x4db8)? != button {
        return Err(invalid("Named construction opener must equal topbar alias"));
    }
    let mut control_identity = None;
    let mut out = binding::control(
        r,
        base,
        topbar,
        window,
        button,
        registry,
        binding::Kind::Opener,
        geometry,
        trace,
        &mut control_identity,
    )?;
    if let Some(control) = control_identity {
        *identity = Some(scope::Opener {
            topbar,
            named_path,
            control,
        });
    }
    out["named_path_verified"] = json!(true);
    out["all_named_containers_settled_shown"] =
        json!(path.iter().all(|p| p["settled_visible"] == true));
    out["path"] = json!(path);
    out["managed_target_mode"] = json!(17);
    out["toggle_requires_already_open_noop"] = json!(true);
    Ok(out)
}

#[allow(clippy::too_many_arguments)]
fn choice(
    r: &Reader<'_>,
    base: u64,
    panel: u64,
    registry: u64,
    key: &str,
    shown: bool,
    geometry: bool,
    trace: &scope::Recorder<'_>,
    identity: &mut Option<scope::Choice>,
) -> Result<Value> {
    if panel == 0 || !shown {
        return Ok(unavailable("choice_panel_not_settled_shown"));
    }
    let items = members(r, panel + 0x320, 256)?;
    let db = r.u64(base + 0x439f400)?;
    exact(r, db, base + 0x41b3ed8)?;
    let ordered_catalog = members(r, db + 0x40, 4096)?;
    let catalog: BTreeMap<_, _> = ordered_catalog
        .iter()
        .copied()
        .enumerate()
        .map(|(i, p)| (p, i))
        .collect();
    let mut seen_templates = BTreeSet::new();
    let mut seen_keys = BTreeSet::new();
    let mut selected = None;
    let mut unknown = 0;
    let mut joins = Vec::new();
    for item in &items {
        if r.u64(*item)? != base + 0x43100f8 {
            unknown += 1;
            continue;
        }
        let template = r.u64(item + 0x20)?;
        let Some(ordinal) = catalog.get(&template) else {
            return Err(invalid(
                "Construction choice template outside fresh public catalog",
            ));
        };
        if !seen_templates.insert(template) {
            return Err(invalid("Unique construction choice templates required"));
        }
        exact(r, template, base + 0x41b49c8)?;
        if !super::boolean(r, template + 0x18)? {
            unknown += 1;
            continue;
        }
        let stored = r.i32(template + 0x2b8)?;
        if stored < 0 || stored as usize != *ordinal {
            return Err(invalid("Construction choice ordinal/catalog mismatch"));
        }
        let canonical = r.string_bounded(template + 0x248, 128)?;
        validate_key(&canonical).map_err(|_| invalid("Bounded canonical choice key required"))?;
        if !seen_keys.insert(canonical.clone()) {
            return Err(invalid(
                "Unique canonical construction choice keys required",
            ));
        }
        joins.push((*item, template, *ordinal));
        if canonical == key {
            selected = Some((*item, template));
        }
    }
    let mut out = if let Some((item, template)) = selected {
        // Only the requested public item reaches window/control payload.
        let window = r.u64(item + 0x10)?;
        let local = binding::local(r, base, window, registry)?;
        let mut control_identity = None;
        let mut value = if window == 0 {
            unavailable("choice_window_unmaterialized")
        } else if let Some(button) = binding::named(r, window, false, "start_construction_button")?
        {
            binding::control(
                r,
                base,
                item,
                window,
                button,
                registry,
                binding::Kind::Choice,
                geometry,
                trace,
                &mut control_identity,
            )?
        } else {
            unavailable("choice_button_unavailable")
        };
        if unknown == 0 {
            if let Some(control) = control_identity {
                *identity = Some(scope::Choice {
                    key: key.to_owned(),
                    database: db,
                    ordered_catalog,
                    joins,
                    requested_item: item,
                    requested_template: template,
                    control,
                });
            }
        }
        value["window"] = local;
        value["canonical_choice_join_verified"] = json!(true);
        value
    } else {
        unavailable("requested_choice_unavailable")
    };
    out["building_id"] = json!(key);
    out["total_item_count"] = json!(items.len());
    out["unsupported_item_count"] = json!(unknown);
    out["complete_choice_coverage"] = json!(unknown == 0);
    Ok(out)
}

pub(crate) fn blockers(
    r: &Reader<'_>,
    base: u64,
    country: u64,
    idler: u64,
    manager: u64,
    registry: u64,
) -> Result<Value> {
    let focus = r.u64(manager + 0x130)?;
    let politics = r.u64(manager + 0x128)?;
    if focus == 0 || politics == 0 {
        return Ok(unavailable("own_blocker_roots_unmaterialized"));
    }
    exact(r, focus, base + 0x4249c70)?;
    if r.u64(focus + 0x2f0)? != idler {
        return Err(invalid("Focus blocker view must belong to own idler"));
    }
    let displayed = r.u64(focus + 0x11d8)?;
    if displayed != 0 && displayed != country {
        return Ok(unavailable("focus_country_binding_unavailable"));
    }
    exact(r, politics, base + 0x42546e8)?;
    if r.u64(politics + 0x2f0)? != idler || r.u64(politics + 0x38)? != manager {
        return Err(invalid("Politics blocker view must belong to own manager"));
    }
    let political = binding::local(r, base, r.u64(politics + 0x2f8)?, registry)?;
    // Same Reader/Witnesses; only own root/window flags, no selected tech payload.
    let facts = crate::focus_ui_facts::observe(
        r,
        base,
        idler,
        manager,
        focus,
        registry,
        r.i32(country + 8)?,
    )?;
    let clear = political["settled_visible"] == false
        && facts.focus_view_local_hidden() == Some(true)
        && facts.research_panel_local_ready() == Some(false)
        && facts.research_tree_local_ready() == Some(false)
        && facts.focus_detail_local_counts() == Some([0, 0])
        && facts.own_research_info_local_counts() == Some([0, 0]);
    let mut out = facts.local_summary();
    out["available"] = json!(true);
    out["politics_root"] = political;
    out["focus_root_settled_hidden"] = json!(facts.focus_view_local_hidden());
    out["research_panel_settled_visible"] = json!(facts.research_panel_local_ready());
    out["research_tree_settled_visible"] = json!(facts.research_tree_local_ready());
    out["conservative_scoped_clear"] = json!(clear);
    Ok(out)
}

/// Own local UI only. No queue, map, foreign location or input operation occurs.
pub fn snapshot(process: &dyn Process, key: &str, include_geometry: bool) -> Result<Value> {
    Ok(observe(process, key, include_geometry, None)?.value)
}

/// Optional same-snapshot Find view read for Find stages.
#[derive(Clone, Copy, Debug)]
pub(crate) struct FindRead {
    pub(crate) state_id: Option<i32>,
    /// Place only: also sum the requested building's queued amount in the state.
    pub(crate) placement: bool,
}
pub(crate) fn observe(
    process: &dyn Process,
    key: &str,
    include_geometry: bool,
    find: Option<FindRead>,
) -> Result<Observation> {
    validate_key(key)?;
    let start = Instant::now();
    process.begin_snapshot()?;
    let witnesses = Witnesses::new(process);
    let trace = scope::Recorder::new(&witnesses);
    let r = Reader::new(&trace);
    let base = process.image_base();
    witnesses.section("own_production");
    let (country, production) = economy::own_production(&r, base)?;
    witnesses.section("campaign");
    let campaign = game::context_from_reader(&r, base)?;
    let [gs, idler, _] = campaign.anchors;
    witnesses.section("interface");
    let registry = r.u64(idler + 0x4f8)?;
    exact(&r, registry, base + 0x42736f8)?;
    if r.u64(base + 0x45778e0)? != registry {
        return Err(invalid("Current input GUI must equal own registry"));
    }
    let manager = r.u64(idler + 0x6b8)?;
    pointer(manager)?;
    if r.u64(manager + 0x180)? != idler {
        return Err(invalid("Construction manager must belong to own idler"));
    }
    let mode = r.i32(manager + 0x3d8)?;
    witnesses.section("selection");
    let selected = selection::observe(&r, base, manager)?;
    witnesses.section("panel");
    let panel = r.u64(manager + 0x150)?;
    let mut panel_root = None;
    let panel_state = if panel == 0 {
        json!({"materialized":false,"settled_visible":null})
    } else {
        exact(&r, panel, base + 0x43101d8)?;
        if r.u64(panel + 0x2f0)? != idler {
            return Err(invalid("Construction panel must belong to own idler"));
        }
        let root = r.u64(panel + 0x2f8)?;
        panel_root = Some(root);
        binding::local(&r, base, root, registry)?
    };
    witnesses.section("gates");
    let flags150 = r.byte(registry + 0x150)?;
    let flags12 = r.byte(registry + 0x12)?;
    let receiver158 = r.u64(registry + 0x158)? == 0;
    let receiver230 = r.u64(registry + 0x230)? == 0;
    let gates = json!({"gui_global_join_verified":true,"flag150_raw":flags150,"flag12_raw":flags12,
        "receiver158_null":receiver158,"text_receiver230_null":receiver230,
        "conservative_local_gate_clear":flags150==0&&flags12==0&&receiver158&&receiver230});
    let first = r.byte(manager + 0x3e1)?;
    let second = r.byte(manager + 0x3e2)?;
    let gs_gate = r.byte(gs + 0xa59)?;
    let idler_gate = r.byte(idler + 0x8f0)?;
    let engine = json!({"manager3e1_raw":first,"manager3e2_raw":second,"gs_a59_raw":gs_gate,"idler8f0_raw":idler_gate,
        "known_scalar_gate_clear":first==0&&second==0&&gs_gate!=0&&idler_gate==0,
        "auxiliary_idler7b0_present":r.u64(idler+0x7b0)?!=0,
        "auxiliary_manager218_present":r.u64(manager+0x218)?!=0,
        "auxiliary_manager248_present":r.u64(manager+0x248)?!=0,
        "complete_engine_eligibility_available":false});
    let mut opener_identity = None;
    let mut choice_identity = None;
    witnesses.section("opener");
    let opener = opener(
        &r,
        base,
        idler,
        manager,
        registry,
        include_geometry,
        &trace,
        &mut opener_identity,
    )?;
    witnesses.section("choice");
    let choice = choice(
        &r,
        base,
        panel,
        registry,
        key,
        panel_state["settled_visible"] == true,
        include_geometry,
        &trace,
        &mut choice_identity,
    )?;
    witnesses.section("blockers");
    trace.blockers(manager);
    let blockers = blockers(&r, base, country, idler, manager, registry)?;
    witnesses.section("find_view");
    let mut find_identity = None;
    let find_value = match find {
        Some(request) => {
            let own_tag = r.i32(country + 8)?;
            let (value, identity) = super::find_view::observe_in(
                &r,
                base,
                gs,
                idler,
                registry,
                manager,
                own_tag,
                request.state_id,
            )?;
            find_identity = Some(identity);
            let mut value = value;
            if let (true, Some(state_id)) = (request.placement, request.state_id) {
                // Same witnessed snapshot; full own collection, then one summary.
                let lines = super::lines_in(&r, base, production, own_tag, true)?;
                let summary = super::queue_summary(&lines, key, state_id);
                value["placement"] = json!({"building_id":key,"state_id":state_id,
                    "in_state":summary.in_state,"elsewhere":summary.elsewhere,
                    "complete":summary.complete,"line_total":lines.total});
            }
            value
        }
        None => json!(null),
    };
    witnesses.section("input_geometry");
    let geometry = if include_geometry {
        crate::gui::input_geometry_from_reader(&r, base)?
    } else {
        json!({"requested":false})
    };
    let (calls, bytes) = witnesses.finish()?;
    process.check()?;
    let scopes = scope::Scopes {
        base,
        anchors: campaign.anchors,
        playthrough_id: campaign.playthrough_id.clone(),
        player: campaign.player.clone(),
        country,
        production,
        registry,
        manager,
        panel,
        panel_root,
        opener: opener_identity,
        choice: choice_identity,
        include_geometry,
        find: find_identity,
        words: trace.finish(),
    };
    let value = json!({"source":"native own construction controls","build_id":game::BUILD_ID,
        "scope":"own_named_ui_and_public_requested_building_only","player":campaign.player,
        "building_id":key,"date_raw":campaign.date_raw,"paused":campaign.paused,
        "managed_mode_raw":mode,"selection":selected,"panel":panel_state,
        "opener":opener,"choice":choice,"local_gui_gates":gates,"opener_engine_gates":engine,
        "scoped_blockers":blockers,"input_geometry":geometry,"geometry_requested":include_geometry,
        "input_authorized":false,"effective_routing_verified":false,"current_frame_verified":false,
        "global_modal_clear_verified":false,"eligibility_available":false,"live_validation":"pending",
        "observation_paused_game":false,
        "freshness":{"observed_unix_ms":crate::daily_observation::unix_ms()?,"elapsed_ms":start.elapsed().as_secs_f64()*1000.0,
            "read_calls_kind":"logical_read_requests","read_calls":r.calls.get()+calls as u64,"read_bytes":r.bytes.get()+bytes}});
    let mut value = value;
    if find.is_some() {
        value["find"] = find_value;
    }
    Ok(Observation {
        value,
        campaign,
        scopes,
    })
}

pub(crate) fn tool() -> Value {
    json!({"name":"hoi4_construction_controls","description":"Combined read-only own construction opener and one canonical public building choice. Exact named paths, Standard callback bindings, managed/selected modes, local receiver gates and scoped own blockers; optional exact renderer/input geometry. No queue/map payload, input, eligibility or global modal/frame/routing authority. Unknown controls and predicates stay explicit. Never pauses or moves the cursor.","inputSchema":{"type":"object","properties":{"building_id":{"type":"string","minLength":1,"maxLength":128},"include_geometry":{"type":"boolean","default":false}},"required":["building_id"],"additionalProperties":false},"annotations":{"readOnlyHint":true}})
}
