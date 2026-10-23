//! Own Mac construction selection, panel and complete choice membership.
//! Exact Mach-O fields are independent of the Linux controls/Find readers.
//! This is an observation foundation, never an input or settled-frame grant.
use super::{
    catalog, controls::validate_key, invalid, location, members, pointer, QueueLine, QueueSummary,
};
use crate::{
    economy,
    error::{Error, Result},
    game, gui_tree,
    memory::{Reader, StringAbi},
    platform::{Identity, Process},
    research_ui::Witnesses,
};
use serde_json::{json, Value};
use std::{collections::BTreeSet, time::Instant};

const GRIDS: [(&str, &str); 4] = [
    ("possible_state_constructions", "non_shared_item_grid"),
    ("possible_shared_constructions", "shared_item_grid"),
    ("possible_province_constructions", "province_item_grid"),
    ("possible_facility_constructions", "facility_item_grid"),
];

#[derive(Clone, Debug, PartialEq, Eq)]
enum Selection {
    Inactive,
    Ordinary(catalog::Template),
    Unsupported(i32),
}
#[derive(Clone, Debug, PartialEq, Eq)]
struct Panel {
    object: u64,
    root: u64,
    flags: Option<u8>,
}
impl Panel {
    fn locally_visible(&self) -> bool {
        self.flags.is_some_and(|flags| flags & 8 != 0)
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
struct Choice {
    item: u64,
    template: catalog::Template,
    window: u64,
    window_flags: u8,
    button: u64,
    button_flags: u8,
    button_state: i32,
}
#[derive(Clone, Debug, PartialEq, Eq)]
struct Choices {
    // Every named-path membership, grid and item/template join remains private.
    paths: Vec<(u64, Vec<u64>)>,
    grids: Vec<(u64, Vec<u64>)>,
    items: Vec<(u64, u64)>,
    complete: bool,
    reason: &'static str,
    target: Option<Choice>,
}
impl Choices {
    fn unavailable(reason: &'static str) -> Self {
        Self {
            paths: vec![],
            grids: vec![],
            items: vec![],
            complete: false,
            reason,
            target: None,
        }
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
struct Binding {
    process: Identity,
    country: u64,
    production: u64,
    state: u64,
    registry: u64,
    manager: u64,
    catalog: catalog::Catalog,
    target: catalog::Template,
    queue_members: Vec<u64>,
    queue_lines: Vec<QueueLine>,
}
/// All facts originate inside one complete witness bracket. JSON renders them;
/// it is never parsed back into target, scope or completion evidence.
pub(crate) struct Observation {
    campaign: game::ClockContext,
    binding: Binding,
    selection: Selection,
    panel: Panel,
    choices: Choices,
    queue: QueueSummary,
    read_calls: u64,
    read_bytes: usize,
    elapsed_ms: f64,
}

fn exact(r: &Reader<'_>, base: u64, object: u64, primary: u64) -> Result<()> {
    pointer(object)?;
    let observed = r.u64(object)?;
    if observed != base + primary {
        let found = observed
            .checked_sub(base)
            .filter(|v| *v < 0x400_0000)
            .map_or("outside_image".to_string(), |v| format!("rva_{v:x}"));
        return Err(invalid(&format!(
            "Exact Mac construction primary required: expected rva_{primary:x}, observed {found}"
        )));
    }
    Ok(())
}
fn window(r: &Reader<'_>, base: u64, value: u64) -> Result<u64> {
    pointer(value)?;
    gui_tree::container(r, value, base)?
        .ok_or_else(|| invalid("Exact Mac construction container required"))
}
/// Full unique child membership precedes lookup. Never use the first matching
/// name or the engine's nonnull null-grid fallback as target evidence.
fn named(
    r: &Reader<'_>,
    base: u64,
    parent: u64,
    key: &str,
    paths: &mut Vec<(u64, Vec<u64>)>,
) -> Result<Option<u64>> {
    let children = members(r, parent + 0xd30, 4096)?;
    let mut found = None;
    for &child in &children {
        if gui_tree::name(r, child, base)?.as_deref() == Some(key) && found.replace(child).is_some()
        {
            return Err(invalid("Unique named Mac construction child required"));
        }
    }
    paths.push((parent, children));
    Ok(found)
}
fn selection(r: &Reader<'_>, manager: u64, catalog: &catalog::Catalog) -> Result<Selection> {
    // Current-file setter writes mode before pointer; Witnesses rechecks both.
    // Mode zero retains a default pointer; never dereference it. The semantic
    // roles of modes 2/3 are deliberately not borrowed from Linux.
    let mode = r.i32(manager + 0x3c0)?;
    match mode {
        0 => Ok(Selection::Inactive),
        1 => {
            let selected = r.u64(manager + 0x3b8)?;
            if selected == r.u64(catalog.database + 0x3a8)? {
                return Err(invalid(
                    "Ordinary selection must differ from the special cache",
                ));
            }
            let selected = catalog
                .templates
                .get(&selected)
                .ok_or_else(|| invalid("Selected building must join the valid public catalog"))?;
            Ok(Selection::Ordinary(selected.clone()))
        }
        mode => Ok(Selection::Unsupported(mode)),
    }
}
fn panel(r: &Reader<'_>, base: u64, idler: u64, manager: u64) -> Result<Panel> {
    let object = r.u64(manager + 0x150)?;
    let mut out = Panel {
        object,
        root: 0,
        flags: None,
    };
    if object == 0 {
        return Ok(out);
    }
    exact(r, base, object, 0x343aa90)?;
    // Constructor/base helper chain, not offsets inferred from another view.
    if r.u64(object + 0x428)? != idler || r.u64(object + 0x30)? != manager {
        return Err(invalid(
            "Mac construction view must retain both own backlinks",
        ));
    }
    let root = r.u64(object + 0x430)?;
    if root != 0 {
        out.root = window(r, base, root)?;
        out.flags = Some(r.byte(out.root + 0x95)?);
    }
    Ok(out)
}
fn choices(
    r: &Reader<'_>,
    base: u64,
    panel: &Panel,
    catalog: &catalog::Catalog,
    target: &catalog::Template,
) -> Result<Choices> {
    if !panel.locally_visible() {
        return Ok(Choices::unavailable("panel_not_locally_visible"));
    }
    let mut out = Choices::unavailable("choice_path_unmaterialized");
    let Some(possible) = named(
        r,
        base,
        panel.root,
        "possible_constructions",
        &mut out.paths,
    )?
    else {
        return Ok(out);
    };
    let possible = window(r, base, possible)?;
    let mut seen_grids = BTreeSet::new();
    let mut seen_items = BTreeSet::new();
    let mut seen_templates = BTreeSet::new();
    let mut requested = None;
    for (section_name, grid_name) in GRIDS {
        let Some(section) = named(r, base, possible, section_name, &mut out.paths)? else {
            return Ok(out);
        };
        let section = window(r, base, section)?;
        let Some(grid) = named(r, base, section, grid_name, &mut out.paths)? else {
            return Ok(out);
        };
        // The inspected factory installs CStandardGridBox. Null/base/other
        // derived grids do not authorize reading this list layout.
        exact(r, base, grid, 0x34be068)?;
        if !seen_grids.insert(grid) {
            return Err(invalid("Distinct construction grids required"));
        }
        let items = members(r, grid + 0x190, 4096)?;
        if out.items.len() + items.len() > 4096 {
            return Err(invalid(
                "Complete construction choice collection exceeds bound",
            ));
        }
        for &item in &items {
            if !seen_items.insert(item) {
                return Err(invalid("Unique construction choice items required"));
            }
            exact(r, base, item, 0x343a9b0)?;
            let template = r.u64(item + 0x20)?;
            if !catalog.templates.contains_key(&template) || !seen_templates.insert(template) {
                return Err(invalid(
                    "Unique valid public construction choice templates required",
                ));
            }
            out.items.push((item, template));
            if template == target.address {
                requested = Some(item);
            }
        }
        out.grids.push((grid, items));
    }
    // Complete membership/type/catalog checks precede ANY target control payload.
    out.complete = true;
    let Some(item) = requested else {
        out.reason = "requested_building_not_in_choices";
        return Ok(out);
    };
    let frame = r.u64(item + 0x10)?;
    if frame == 0 {
        out.reason = "choice_window_unmaterialized";
        return Ok(out);
    }
    let frame = window(r, base, frame)?;
    let window_flags = r.byte(frame + 0x95)?;
    if window_flags & 8 == 0 {
        out.reason = "choice_window_hidden";
        return Ok(out);
    }
    let Some(button) = named(r, base, frame, "start_construction_button", &mut out.paths)? else {
        out.reason = "choice_button_unmaterialized";
        return Ok(out);
    };
    exact(r, base, button, 0x34aafa0)?;
    let button_flags = r.byte(button + 0x65)?;
    let button_state = r.i32(button + 0x9c)?;
    if !(0..=3).contains(&button_state) {
        return Err(invalid("Unknown Mac construction button state"));
    }
    out.target = Some(Choice {
        item,
        template: target.clone(),
        window: frame,
        window_flags,
        button,
        button_flags,
        button_state,
    });
    out.reason = "requested_control_observed";
    Ok(out)
}

pub(crate) fn observe(process: &dyn Process, key: &str, state_id: i32) -> Result<Observation> {
    validate_key(key)?;
    super::find_view::validate_state_id(i64::from(state_id))?;
    if process.string_abi() != StringAbi::LibCxx {
        return Err(Error::new(
            "unsupported_platform",
            "Construction UI probe requires the verified native Mac layout",
        ));
    }
    if key == "rail_way" {
        return Err(Error::new(
            "capability_unavailable",
            "Special construction modes remain unverified",
        ));
    }
    let start = Instant::now();
    process.begin_snapshot()?;
    let process_identity = process.identity().cloned().ok_or_else(|| {
        Error::new(
            "ownership_unavailable",
            "Exact owned process identity required",
        )
    })?;
    let w = Witnesses::new(process);
    let r = Reader::new(&w);
    let base = process.image_base();
    w.section("mac_construction_owner");
    let (country, production) = economy::own_production(&r, base)?;
    let campaign = game::context_from_reader(&r, base)?;
    let [gs, idler, _] = campaign.anchors;
    let own_tag = r.i32(country + 8)?;
    let l = super::layout(&r);
    w.section("mac_construction_target_state");
    let state = location::catalog_pointer(&r, gs + l.gs_states, state_id, 4096)?;
    if !location::own_state(&r, base, state, own_tag)? {
        return Err(Error::new(
            "construction_target_unavailable",
            "Requested state must be owned and controlled by the current player",
        ));
    }
    if r.i32(state + l.state_id)? != state_id {
        return Err(invalid("Requested state ID/catalog mismatch"));
    }
    w.section("mac_construction_catalog");
    let catalog = catalog::observe(&r, base)?;
    let target = catalog.required(key)?.clone();
    w.section("mac_construction_interface");
    let registry = r.u64(idler + 0x4c0)?;
    exact(&r, base, registry, 0x34974d0)?;
    let manager = r.u64(idler + 0x670)?;
    pointer(manager)?;
    if r.u64(manager + 0x180)? != idler {
        return Err(invalid("Mac construction manager must retain own idler"));
    }
    w.section("mac_construction_selection");
    let selection = selection(&r, manager, &catalog)?;
    w.section("mac_construction_panel");
    let panel = panel(&r, base, idler, manager)?;
    w.section("mac_construction_choices");
    let choices = choices(&r, base, &panel, &catalog, &target)?;
    w.section("mac_construction_queue");
    let lines = super::lines_in(&r, base, production, own_tag, true)?;
    let queue = super::queue_summary(&lines, key, state_id);
    let binding = Binding {
        process: process_identity,
        country,
        production,
        state,
        registry,
        manager,
        catalog,
        target,
        queue_members: lines.members,
        queue_lines: lines.known,
    };
    let (calls, bytes) = w.finish()?;
    process.check()?;
    if process.identity() != Some(&binding.process) {
        return Err(Error::new(
            "ownership_changed",
            "Construction process identity changed during observation",
        ));
    }
    Ok(Observation {
        campaign,
        binding,
        selection,
        panel,
        choices,
        queue,
        read_calls: r.calls.get() + calls as u64,
        read_bytes: r.bytes.get() + bytes,
        elapsed_ms: start.elapsed().as_secs_f64() * 1000.0,
    })
}

pub fn snapshot(process: &dyn Process, key: &str, state_id: i32) -> Result<Value> {
    let o = observe(process, key, state_id)?;
    let selection = match &o.selection {
        Selection::Inactive => {
            json!({"mode_raw":0,"supported":true,"kind":"inactive","building_id":null})
        }
        Selection::Ordinary(t) => {
            json!({"mode_raw":1,"supported":true,"kind":"ordinary","building_id":t.key})
        }
        Selection::Unsupported(mode) => {
            json!({"mode_raw":mode,"supported":false,"kind":"unverified","building_id":null})
        }
    };
    let target = o.choices.target.as_ref().map(|c| {
        json!({"building_id":c.template.key,
        "button_flags_raw":c.button_flags,"locally_shown":c.button_flags&8!=0,
        "button_state_raw":c.button_state,"disabled":c.button_state==3})
    });
    Ok(
        json!({"source":"native witnessed Mac construction UI probe",
        "build_id":"089EA519-6780-353B-8E39-4E9309B444D4",
        "player":o.campaign.player,"date_raw":o.campaign.date_raw,"paused":o.campaign.paused,
        "building_id":o.binding.target.key,"state_id":state_id,"target_identity_verified":true,
        "selection":selection,
        "panel":{"materialized":o.panel.object!=0,"root_materialized":o.panel.root!=0,
            "flags_raw":o.panel.flags,"locally_visible":o.panel.locally_visible()},
        "choices":{"complete_membership":o.choices.complete,"grid_count":o.choices.grids.len(),
            "item_count":o.choices.items.len(),"reason":o.choices.reason,"target":target},
        "queue_complete":o.queue.complete,
        "queued":o.queue.complete.then(||json!({"in_state":o.queue.in_state,"elsewhere":o.queue.elsewhere})),
        "input_authorized":false,"geometry_available":false,"settled_frame_verified":false,
        "grid_registry_backlink_verified":false,"callback_binding_verified":false,
        "placement_eligibility_verified":false,"global_modal_clear_verified":false,
        "observation_paused_game":false,
        "freshness":{"read_calls":o.read_calls,"read_bytes":o.read_bytes,"elapsed_ms":o.elapsed_ms,
            "read_calls_kind":"logical_read_requests"}}),
    )
}

pub(crate) fn tool() -> Value {
    json!({"name":"hoi4_construction_ui_probe","description":"Read-only native Mac construction observation: own target/state/catalog/complete queue, independently bound panel owner links, local visibility, ordinary canonical selection and full bounded choice membership before the requested button state. Hidden panels skip choice payload. Unverified modes remain explicit. No geometry, input, navigation, placement, callback or settled-frame authority; never pauses or moves the cursor. Other platforms refuse before process access.",
        "inputSchema":{"type":"object","properties":{"building_id":{"type":"string","minLength":1,"maxLength":128},"state_id":{"type":"integer","minimum":1,"maximum":4095}},"required":["building_id","state_id"],"additionalProperties":false},"annotations":{"readOnlyHint":true}})
}
