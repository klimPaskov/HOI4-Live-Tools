//! Private point preparation only, no dispatch or eligibility authority.
use crate::{
    error::{Error, Result},
    game, gui, own_focus,
    platform::Process,
    research_attempt::{self, PointCandidate, Surface},
    research_candidate_observation::{codec, sprite_surface, OwnedClientProfile},
};
use serde_json::Value;
#[derive(Clone, Copy)]
#[allow(dead_code)] // Unregistered private candidate; owner integration constructs stages.
pub(crate) enum Stage {
    Node,
    Start,
}
fn unavailable(s: &str) -> Error {
    Error::new("focus_geometry_unavailable", s)
}
fn ui(facts: Option<&crate::focus_ui_facts::FocusUiFacts>, stage: Stage) -> Result<()> {
    let facts = facts.ok_or_else(|| unavailable("Witnessed own UI facts unavailable"))?;
    if facts.focus_mode_selected() != Some(true)
        || facts.focus_view_local_ready() != Some(true)
        || facts.own_research_info_local_counts() != Some([0, 0])
    {
        return Err(unavailable(
            "Known own focus mode/view and own research Info local blockers required",
        ));
    }
    let expected = if matches!(stage, Stage::Start) {
        [1, 0]
    } else {
        [0, 0]
    };
    if facts.focus_detail_local_counts() != Some(expected) {
        return Err(unavailable(
            "Known complete own focus detail local blockers required",
        ));
    }
    // These scoped local facts never mean global modal-clear or topmost routing.
    Ok(())
}
fn controls_state(c: &Value) -> Result<()> {
    let state = c["disabled_state_raw"]
        .as_i64()
        .ok_or_else(|| unavailable("Known Standard state required"))?;
    let flags = c["flags74_raw"]
        .as_u64()
        .and_then(|v| u32::try_from(v).ok())
        .ok_or_else(|| unavailable("Known Standard flags required"))?;
    if !(0..=2).contains(&state) || flags & (1 << 11) == 0 || flags & (1 << 12) != 0 {
        return Err(unavailable(
            "Supported nondisabled local focus control branch required",
        ));
    }
    Ok(())
}
fn surface(
    d: &Value,
    key: &str,
    stage: Stage,
    facts: Option<&crate::focus_ui_facts::FocusUiFacts>,
) -> Result<Surface> {
    ui(facts, stage)?;
    if d["owned"] != true || d["build_id"] != game::BUILD_ID {
        return Err(unavailable("Fresh own verified focus detail required"));
    }
    let command = &d["command_observation"];
    if command["requested_key"] != key || command["same_reader_clock_context"] != true {
        return Err(unavailable("Same-reader own command context required"));
    }
    if matches!(stage, Stage::Start)
        && (!command
            .as_object()
            .is_some_and(|v| v.contains_key("active_key"))
            || !command["active_key"].is_null())
    {
        return Err(unavailable(
            "Known no active own focus required before start point",
        ));
    }
    let catalog = d["focuses"]
        .as_array()
        .ok_or_else(|| unavailable("Own canonical focus result required"))?;
    if catalog.len() != 1 || catalog[0]["key"] != key {
        return Err(unavailable("Exact canonical requested own focus required"));
    }
    let details = d["details"]
        .as_array()
        .filter(|v| v.len() <= 32)
        .ok_or_else(|| unavailable("Bounded own detail windows required"))?;
    // Complete producer counts cover omitted nonmatching retained panels.
    // Detailed body rows are authoritative only for this requested key.
    let mut visible = Vec::new();
    for detail in details.iter().filter(|d| d["selected_key"] == key) {
        let w = &detail["window"];
        match w["materialized"].as_bool() {
            Some(false) => {}
            Some(true) => match w["conservative_container_visibility"].as_bool() {
                Some(true) => visible.push(detail),
                Some(false) => {}
                None => return Err(unavailable("Requested detail visibility unknown")),
            },
            None => return Err(unavailable("Requested detail materialization unknown")),
        }
    }
    match stage {
        Stage::Node => {
            if !visible.is_empty() {
                return Err(unavailable(
                    "Own focus details must be closed before node selection",
                ));
            }
            let a = &d["node_association"];
            if a["requested_key"] != key || a["materialized"] != true {
                return Err(unavailable("Exact own current focus item required"));
            }
            let controls = a["controls"]
                .as_array()
                .filter(|v| v.len() <= 2)
                .ok_or_else(|| unavailable("Bounded own focus controls required"))?;
            let chosen = controls
                .iter()
                .filter(|c| c["name"] == "symbol")
                .collect::<Vec<_>>();
            if chosen.len() != 1 {
                return Err(unavailable("Unique named symbol control required"));
            }
            let c = chosen[0];
            if c["type_encoding"] != "15CButtonStandard"
                || c["binding_verified"] != true
                || c["disabled_state3"] != false
                || c["renderer_control_join_verified"] != true
            {
                return Err(unavailable(
                    "Exact own focus node callback/control required",
                ));
            }
            controls_state(c)?;
            sprite_surface(&c["render_diagnostic"])
        }
        Stage::Start => {
            if visible.len() != 1 || visible[0]["selected_key"] != key {
                return Err(unavailable(
                    "Unique visible selected requested own focus required",
                ));
            }
            let start = &visible[0]["window"]["start"];
            if start["materialized"] != true || start["disabled"] != false {
                return Err(unavailable("Known nondisabled own start control required"));
            }
            controls_state(start)?;
            let b = &start["ordinary_binding"];
            if b["selected_own_key"] != key
                || b["available"] != true
                || b["callback_route_verified"] != true
                || b["renderer_control_join_verified"] != true
            {
                return Err(unavailable("Exact own selected start callback required"));
            }
            sprite_surface(&b["render_diagnostic"])
        }
    }
}
/// Separate witnessed observations do not create frame/atomic/routing authority.
#[allow(dead_code)] // Private integration candidate has no registered caller.
pub(crate) fn observe(
    process: &dyn Process,
    key: &str,
    stage: Stage,
    p: &OwnedClientProfile,
) -> Result<(PointCandidate, own_focus::FocusContext)> {
    own_focus::validate_key(key)?;
    let (detail, context) = own_focus::command_owned(process, key, true)?;
    let s = surface(&detail, key, stage, context.ui_facts.as_ref())?;
    let geometry = gui::input_geometry(process)?;
    let c = codec(&geometry, p)?;
    process.check()?;
    Ok((research_attempt::point_candidate(&s, &c)?, context))
}
#[cfg(test)]
mod tests;
