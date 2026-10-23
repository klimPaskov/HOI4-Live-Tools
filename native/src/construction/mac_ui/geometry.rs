//! Requested-item geometry for Mach-O 089EA519. All reads belong to the
//! caller's complete own-country/catalog/grid/queue witness bracket. These UI
//! facts locate an ordinary control; they never evaluate script prerequisites
//! or establish atomic hit-cache completion, OS delivery or input permission.
use super::{exact, invalid, pointer, window, Interface};
use crate::{
    construction::mac_panel::routing,
    error::Result,
    gui::macos::canvas_from_reader,
    gui_tree::{self, Rect},
    memory::Reader,
};
use serde_json::{json, Value};

const GAME: (u32, u32) = (1920, 1080);

#[derive(Clone, Debug, PartialEq)]
struct Binding {
    ancestors: [u64; 4],
    button: u64,
    graphics: u64,
    renderer: u64,
    parent: u64,
    rect: Rect,
    routing: Option<routing::Observation>,
}

#[derive(Clone, Debug, PartialEq)]
pub(super) struct Observation {
    binding: Option<Binding>,
    reason: &'static str,
}
impl Observation {
    fn unavailable(reason: &'static str) -> Self {
        Self {
            binding: None,
            reason,
        }
    }
    pub(super) fn available(&self) -> bool {
        self.binding.is_some()
    }
    pub(super) fn summary(&self) -> Value {
        let rect = self
            .binding
            .as_ref()
            .map(|b| json!({"x":b.rect.x,"y":b.rect.y,"width":b.rect.w,"height":b.rect.h}));
        let routing = self
            .binding
            .as_ref()
            .and_then(|b| b.routing.as_ref())
            .map(|r| r.diagnostic_with_parent(true));
        json!({"available":self.available(),"reason":self.reason,"rect":rect,"routing":routing,
            "ancestor_visibility_verified":self.available(),"graphical_parent_verified":self.available(),
            "input_authorized":false,"settled_frame_verified":false,"delivery_verified":false,
            "prerequisites_evaluated":false})
    }
}

pub(super) fn observe(
    r: &Reader<'_>,
    interface: Interface,
    ancestors: [u64; 4],
    button: u64,
    include_routing: bool,
) -> Result<Observation> {
    let Interface {
        base,
        idler,
        manager,
        registry,
    } = interface;
    if r.i32(manager + 0x3a8)? != 17 {
        return Ok(Observation::unavailable("construction_panel_not_selected"));
    }
    // Only the requested item's root/possible/section/frame are visibility
    // ancestors. Other sections still retain complete membership witnesses.
    for ancestor in ancestors {
        if window(r, base, ancestor, registry)? != ancestor {
            return Err(invalid(
                "Canonical construction geometry ancestors required",
            ));
        }
        let flags = r.byte(ancestor + 0x95)?;
        let states = [r.i32(ancestor + 0x12b8)?, r.i32(ancestor + 0x1270)?];
        if states.iter().any(|state| !(0..=2).contains(state)) {
            return Err(invalid(
                "Unknown Mac construction ancestor transition state",
            ));
        }
        if flags & 8 == 0 || states != [0, 0] {
            return Ok(Observation::unavailable(
                "requested_ancestor_not_visible_idle",
            ));
        }
    }
    let flags = r.byte(button + 0x65)?;
    let state = r.i32(button + 0x9c)?;
    if flags & 8 == 0 || ![0, 2].contains(&state) {
        return Ok(Observation::unavailable("requested_button_not_ready"));
    }
    // +CC bit40 is the cached requested-disable result from the UI's last
    // prerequisite evaluation. A clear bit cannot predict the click predicate.
    let control = r.byte(button + 0xcc)?;
    if control & 8 == 0 || control & 0x40 != 0 {
        return Ok(Observation::unavailable("requested_button_input_disabled"));
    }
    if r.byte(button + 0x1d1)? != 0 || r.u64(button + 0xc0)? != 0 {
        return Ok(Observation::unavailable(
            "requested_button_optional_handler",
        ));
    }
    if r.u64(base + 0x35196c0)? != idler {
        return Err(invalid(
            "Construction geometry must retain the own global idler",
        ));
    }
    if r.byte(base + 0x3519990)? != 0 || r.u64(base + 0x35dbf60)? != 0 {
        return Ok(Observation::unavailable(
            "ordinary_callback_branch_unavailable",
        ));
    }
    let canvas = canvas_from_reader(r, base, registry)?;
    if canvas.physical != [GAME.0 as i32, GAME.1 as i32]
        || canvas.logical != canvas.physical
        || canvas.scale != 1.0
    {
        return Ok(Observation::unavailable(
            "requested_canvas_profile_unavailable",
        ));
    }
    if r.u64(button + 0xd0)? != canvas.object {
        return Err(invalid(
            "Construction button and GUI must retain the same graphics owner",
        ));
    }
    let renderer = r.u64(button + 0xd8)?;
    pointer(renderer)?;
    if ![base + 0x34bdb10, base + 0x34afed0].contains(&r.u64(renderer)?) {
        return Err(invalid("Exact Mac construction sprite required"));
    }
    if r.u64(renderer + 0x138)? != button {
        return Err(invalid(
            "Construction renderer must target the same requested button",
        ));
    }
    let parent = r.u64(button + 0x178)?;
    if parent != 0 {
        exact(r, base, parent, 0x34b0428)?;
        if r.u64(renderer + 0x108)? != parent || r.u64(ancestors[3] + 0x1238)? != parent {
            return Err(invalid(
                "Construction graphical parent must join the own item frame and renderer",
            ));
        }
        if r.byte(parent + 0x128)? == 0 {
            return Ok(Observation::unavailable(
                "requested_graphical_parent_disabled",
            ));
        }
    }
    // Null graphical parent is an independently supported variant: do not read
    // an absent parent's attachments or treat +58 as a graphical GUI owner.
    // Use the shared unclipped computation so malformed geometry propagates
    // distinctly from coherent partial/complete offscreen unavailability.
    let rect = gui_tree::rect_unclipped(r, base, button, GAME)?;
    if rect.x < 0
        || rect.y < 0
        || rect.x + rect.w > GAME.0 as i32
        || rect.y + rect.h > GAME.1 as i32
    {
        return Ok(Observation::unavailable(
            "requested_button_not_fully_contained",
        ));
    }
    let routing = include_routing
        .then(|| routing::observe(r, base, registry, canvas.object, button, rect))
        .transpose()?;
    Ok(Observation {
        binding: Some(Binding {
            ancestors,
            button,
            graphics: canvas.object,
            renderer,
            parent,
            rect,
            routing,
        }),
        reason: "requested_geometry_observed",
    })
}
