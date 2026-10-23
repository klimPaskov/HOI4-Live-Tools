//! Independently bound Mac construction-panel opener. This observer does not
//! select a building, place construction, follow Find or move the camera.
use super::{
    invalid,
    mac_ui::{exact, named, window},
    pointer,
};
use crate::{
    error::{Error, Result},
    game,
    gui_tree::{self, Rect},
    memory::{Reader, StringAbi},
    platform::Process,
    research_ui::Witnesses,
};
use serde_json::{json, Value};
use std::{collections::BTreeSet, time::Instant};

const GAME: (u32, u32) = (1920, 1080);
const CONSTRUCTION: i32 = 0x11;
mod routing;

#[derive(Clone, Debug, PartialEq, Eq)]
struct Window {
    object: u64,
    flags: u8,
    states: [i32; 2],
}
impl Window {
    fn visible(&self) -> bool {
        self.flags & 8 != 0 && !self.states.contains(&2)
    }
    fn idle(&self) -> bool {
        self.states == [0, 0]
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
struct Callback {
    nodes: Vec<u64>,
    glue: u64,
    function: u64,
}
#[derive(Clone, Debug, PartialEq)]
struct Opener {
    topbar: u64,
    windows: Vec<Window>,
    paths: Vec<(u64, Vec<u64>)>,
    button: u64,
    flags: u8,
    state: i32,
    callback: Callback,
    rect: Option<Rect>,
    routing: Option<routing::Observation>,
}
pub(crate) struct Observation {
    context: game::ClockContext,
    managed_mode: i32,
    selection_mode: i32,
    panel: Option<Window>,
    opener: Option<Opener>,
    reason: &'static str,
    read_calls: u64,
    read_bytes: usize,
    elapsed_ms: f64,
}

fn local(r: &Reader<'_>, base: u64, object: u64, registry: u64) -> Result<Window> {
    let object = window(r, base, object, registry)?;
    let flags = r.byte(object + 0x95)?;
    let states = [r.i32(object + 0x12b8)?, r.i32(object + 0x1270)?];
    if states.iter().any(|state| !(0..=2).contains(state)) {
        return Err(invalid("Unknown Mac opener container transition state"));
    }
    Ok(Window {
        object,
        flags,
        states,
    })
}

fn callback(r: &Reader<'_>, base: u64, button: u64, topbar: u64) -> Result<Callback> {
    exact(r, base, button + 0x70, 0x34ab360)?;
    if r.u64(button + 0x90)? != 0 {
        return Err(invalid("Mac opener cannot use a single-observer override"));
    }
    if r.byte(button + 0x8c)? != 0 {
        return Err(invalid("Mac opener observer notification is in progress"));
    }
    if r.u64(button + 0xc0)? != 0 {
        return Err(invalid("Mac opener has an unverified input handler"));
    }
    let mut link = r.u64(button + 0x78)?;
    let tail = r.u64(button + 0x80)?;
    let count = r.i32(button + 0x88)?;
    if !(0..=128).contains(&count) {
        return Err(invalid("Bounded Mac opener observer count required"));
    }
    let mut previous = 0;
    let mut nodes = Vec::new();
    let mut observers = Vec::new();
    let mut seen = BTreeSet::new();
    for _ in 0..count {
        pointer(link)?;
        if !seen.insert(link) || r.u64(link + 8)? != previous {
            return Err(invalid("Malformed Mac opener observer membership"));
        }
        if r.byte(link + 0x18)? != 0 {
            return Err(invalid(
                "Inactive Mac opener observer refuses input binding",
            ));
        }
        nodes.push(link);
        observers.push(r.u64(link)?);
        previous = link;
        link = r.u64(link + 0x10)?;
    }
    if link != 0 || previous != tail || (count == 0 && tail != 0) {
        return Err(invalid("Mac opener observer endpoints disagree"));
    }
    let glue = topbar + 0x3290;
    if observers != [glue] {
        return Err(invalid(
            "Exactly the own Mac construction opener observer required",
        ));
    }
    exact(r, base, glue, 0x3489788)?;
    // Ordinary left-down/release notify more than the first function. An
    // additional handler could suppress or change the intended view toggle.
    for offset in [0x60, 0x150, 0x180, 0x1b0, 0x1e0, 0x2d0, 0x3c0] {
        if r.u64(glue + offset)? != 0 {
            return Err(invalid("Mac opener has an additional click callback"));
        }
    }
    let function = r.u64(glue + 0x30)?;
    exact(r, base, function, 0x3489818)?;
    if r.u64(function + 8)? != base + 0x22f27d0
        || r.u64(function + 0x10)? != 0
        || r.u64(function + 0x18)? != topbar
    {
        return Err(invalid(
            "Mac opener callback must retain the exact own receiver and member",
        ));
    }
    Ok(Callback {
        nodes,
        glue,
        function,
    })
}

fn opener(
    r: &Reader<'_>,
    base: u64,
    idler: u64,
    registry: u64,
    manager: u64,
    geometry: bool,
    include_routing: bool,
) -> Result<Option<Opener>> {
    let topbar = r.u64(manager + 0x188)?;
    if topbar == 0 {
        return Ok(None);
    }
    exact(r, base, topbar, 0x3489640)?;
    if r.u64(topbar + 0x58)? != idler
        || r.u64(topbar + 0x60)? != manager
        || r.u64(topbar + 0x30)? != manager
    {
        return Err(invalid(
            "Mac topbar must retain the exact own idler and manager",
        ));
    }
    if r.string_bounded(topbar + 0x40, 128)? != "topbar" {
        return Err(invalid("Canonical Mac topbar name required"));
    }
    let root = r.u64(topbar + 0x68)?;
    if root == 0 {
        return Ok(None);
    }
    let mut windows = vec![local(r, base, root, registry)?];
    let mut paths = Vec::new();
    for key in [
        "country_menu_window",
        "after_faction_button",
        "after_intel_agency_button",
    ] {
        let Some(child) = named(r, base, windows.last().unwrap().object, key, &mut paths)? else {
            return Ok(None);
        };
        if key == "country_menu_window" && r.u64(topbar + 0x88)? != child {
            return Err(invalid(
                "Mac topbar country-menu alias and membership differ",
            ));
        }
        windows.push(local(r, base, child, registry)?);
    }
    let Some(button) = named(
        r,
        base,
        windows.last().unwrap().object,
        "construction_button",
        &mut paths,
    )?
    else {
        return Ok(None);
    };
    if r.u64(topbar + 0x7428)? != button {
        return Err(invalid(
            "Mac topbar construction alias and membership differ",
        ));
    }
    exact(r, base, button, 0x34aafa0)?;
    if r.u64(button + 0x58)? != registry {
        return Err(invalid(
            "Mac opener button must retain the own GUI registry",
        ));
    }
    let flags = r.byte(button + 0x65)?;
    let state = r.i32(button + 0x9c)?;
    if !(0..=3).contains(&state) {
        return Err(invalid("Unknown Mac opener button state"));
    }
    let callback = callback(r, base, button, topbar)?;
    let mut routing = None;
    let rect = if geometry
        && state != 3
        && flags & 8 != 0
        && windows.iter().all(|w| w.visible() && w.idle())
    {
        let canvas = crate::gui::macos::canvas_from_reader(r, base, registry)?;
        if canvas.physical != [GAME.0 as i32, GAME.1 as i32]
            || canvas.logical != canvas.physical
            || canvas.scale != 1.0
        {
            return Err(Error::new(
                "gui_geometry_unavailable",
                "Mac opener rectangle requires the reviewed 1920x1080 canvas at GUI scale 1",
            ));
        }
        let rect = gui_tree::rect(r, base, button, GAME)?;
        if rect.x < 0
            || rect.y < 0
            || rect.x + rect.w > GAME.0 as i32
            || rect.y + rect.h > GAME.1 as i32
        {
            return Err(invalid(
                "Mac opener must lie fully inside the verified client",
            ));
        }
        if include_routing {
            routing = Some(routing::observe(
                r,
                base,
                registry,
                canvas.object,
                button,
                rect,
            )?);
        }
        Some(rect)
    } else {
        None
    };
    Ok(Some(Opener {
        topbar,
        windows,
        paths,
        button,
        flags,
        state,
        callback,
        rect,
        routing,
    }))
}

fn observe(process: &dyn Process, geometry: bool, routing: bool) -> Result<Observation> {
    if process.string_abi() != StringAbi::LibCxx {
        return Err(Error::new(
            "unsupported_platform",
            "Construction panel probe requires the verified native Mac layout",
        ));
    }
    let start = Instant::now();
    process.begin_snapshot()?;
    let identity = process.identity().cloned().ok_or_else(|| {
        Error::new(
            "ownership_unavailable",
            "Exact owned process identity required",
        )
    })?;
    let w = Witnesses::new(process);
    let r = Reader::new(&w);
    let base = process.image_base();
    w.section("mac_construction_panel_context");
    let context = game::context_from_reader(&r, base)?;
    let idler = context.anchors[1];
    let registry = r.u64(idler + 0x4c0)?;
    exact(&r, base, registry, 0x34974d0)?;
    let manager = r.u64(idler + 0x670)?;
    pointer(manager)?;
    if r.u64(manager + 0x180)? != idler {
        return Err(invalid("Mac panel manager must retain own idler"));
    }
    let managed_mode = r.i32(manager + 0x3a8)?;
    let selection_mode = r.i32(manager + 0x3c0)?;
    w.section("mac_construction_panel_root");
    let view = r.u64(manager + 0x150)?;
    exact(&r, base, view, 0x343aa90)?;
    if r.u64(view + 0x428)? != idler || r.u64(view + 0x30)? != manager {
        return Err(invalid(
            "Mac construction panel must retain both own backlinks",
        ));
    }
    let root = r.u64(view + 0x430)?;
    let panel = if root == 0 {
        None
    } else {
        Some(local(&r, base, root, registry)?)
    };
    w.section("mac_construction_topbar_opener");
    // An already-selected view must never receive another toggle, including
    // while its visibility is pending. Do not require or read an opener then.
    let (opener, reason) = if managed_mode == CONSTRUCTION {
        (None, "already_selected_no_toggle")
    } else if !(0..=0x17).contains(&managed_mode) {
        (None, "unverified_managed_mode")
    } else if selection_mode != 0 {
        (None, "placement_selection_active_or_unverified")
    } else if panel.as_ref().is_none_or(|p| p.flags & 8 != 0 || !p.idle()) {
        (None, "closed_panel_not_observed")
    } else {
        let opener = opener(&r, base, idler, registry, manager, geometry, routing)?;
        let reason = if opener.is_some() {
            "opener_observed"
        } else {
            "opener_not_materialized"
        };
        (opener, reason)
    };
    let (calls, bytes) = w.finish()?;
    process.check()?;
    if process.identity() != Some(&identity) {
        return Err(Error::new(
            "ownership_changed",
            "Mac panel process identity changed during observation",
        ));
    }
    Ok(Observation {
        context,
        managed_mode,
        selection_mode,
        panel,
        opener,
        reason,
        read_calls: r.calls.get() + calls as u64,
        read_bytes: r.bytes.get() + bytes,
        elapsed_ms: start.elapsed().as_secs_f64() * 1000.0,
    })
}

pub fn snapshot(process: &dyn Process, geometry: bool) -> Result<Value> {
    snapshot_with_routing(process, geometry, false)
}

pub fn snapshot_with_routing(
    process: &dyn Process,
    geometry: bool,
    routing: bool,
) -> Result<Value> {
    validate_options(geometry, routing)?;
    let o = observe(process, geometry, routing)?;
    let panel = o.panel.as_ref().map(|p| {
        json!({"engine_visible":p.visible(),"locally_visible":p.flags&8!=0,
        "transition_states_raw":p.states,"local_transitions_idle":p.idle()})
    });
    let opener = o.opener.as_ref().map(|p| json!({"alias_and_membership_verified":true,
        "callback_binding_verified":true,"observer_count":p.callback.nodes.len(),
        "ancestor_count":p.windows.len(),"ancestors_locally_visible":p.windows.iter().all(Window::visible),
        "ancestors_local_transitions_idle":p.windows.iter().all(Window::idle),
        "locally_shown":p.flags&8!=0,"button_state_raw":p.state,"disabled":p.state==3,
        "rect":p.rect.map(|r|json!({"x":r.x,"y":r.y,"w":r.w,"h":r.h})),
        "routing":p.routing.as_ref().map(routing::Observation::diagnostic)}));
    Ok(
        json!({"source":"native witnessed Mac construction-panel opener probe","player":o.context.player,
        "date_raw":o.context.date_raw,"paused":o.context.paused,"managed_mode_raw":o.managed_mode,
        "placement_selection_mode_raw":o.selection_mode,"panel":panel,"opener":opener,"reason":o.reason,
        "routing_requested":routing,"input_authorized":false,"settled_frame_verified":false,"global_modal_clear_verified":false,
        "observation_paused_game":false,"freshness":{"read_calls":o.read_calls,"read_bytes":o.read_bytes,
        "elapsed_ms":o.elapsed_ms,"read_calls_kind":"logical_read_requests"}}),
    )
}

pub(crate) fn validate_options(geometry: bool, routing: bool) -> Result<()> {
    if routing && !geometry {
        return Err(Error::new(
            "invalid_arguments",
            "include_routing requires include_geometry=true",
        ));
    }
    Ok(())
}

pub(crate) fn tool() -> Value {
    json!({"name":"hoi4_construction_panel_probe","description":"Read-only Mac construction panel/topbar opener binding. Verifies own manager/view, managed mode, named aliases, exact callback and optional renderer rectangle on the independently checked 1920x1080 canvas at GUI scale 1. Optional include_routing requires include_geometry and adds complete bounded mouse-route/cursor correlation, without inspecting unrelated target payloads. Already-selected mode skips the toggle control even while visibility is pending. No input, atomic cache completion, settled-frame, global routing or placement authority; never pauses or moves the cursor.",
        "inputSchema":{"type":"object","properties":{"include_geometry":{"type":"boolean","default":false},"include_routing":{"type":"boolean","default":false}},"additionalProperties":false},"annotations":{"readOnlyHint":true}})
}
