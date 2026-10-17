//! Conversion of a same-witness construction observation, never caller JSON.
use crate::{
    construction_stage::{Coverage, FindState, OwnState, PanelState, Stage, Target},
    error::{Error, Result},
    research_attempt::{self, PointCandidate},
    research_candidate_observation::{self as candidate, OwnedClientProfile},
};
use serde_json::Value;

fn bad(message: &'static str) -> Error {
    Error::new("construction_precondition", message)
}
fn panel(value: &Value) -> Result<PanelState> {
    match value["materialized"].as_bool() {
        Some(false) => Ok(PanelState::Unmaterialized),
        Some(true) => {
            let flags = value["flags_raw"]
                .as_u64()
                .and_then(|v| u8::try_from(v).ok())
                .ok_or_else(|| bad("Literal construction panel flags required"))?;
            let states = value["states_raw"]
                .as_array()
                .filter(|a| a.len() == 2)
                .ok_or_else(|| bad("Both construction transition channels required"))?;
            if states.iter().all(|v| v.as_i64() == Some(0)) {
                let shown = flags & 8 != 0;
                if value["settled_visible"].as_bool() != Some(shown) {
                    return Err(bad("Construction panel visibility facts disagree"));
                }
                Ok(if shown {
                    PanelState::Shown
                } else {
                    PanelState::Hidden
                })
            } else {
                Ok(PanelState::Transitional)
            }
        }
        None => Err(bad("Typed construction panel materialization required")),
    }
}
pub(super) fn model(d: &Value, target: &Target) -> Result<OwnState> {
    target.validate()?;
    if d["build_id"] != crate::game::BUILD_ID
        || d["player"] != target.epoch.player
        || d["scope"] != "own_named_ui_and_public_requested_building_only"
        || d["input_authorized"] != false
        || d["observation_paused_game"] != false
    {
        return Err(bad("Exact own construction observation required"));
    }
    if target.stage.is_find() {
        return find_model(d, target);
    }
    if target.stage == Stage::SelectBuilding
        && d["building_id"].as_str() != target.building_id.as_deref()
    {
        return Err(bad("Requested canonical construction key differs"));
    }
    let selected = &d["selection"];
    let mode = selected["mode_raw"]
        .as_i64()
        .and_then(|v| i32::try_from(v).ok())
        .ok_or_else(|| bad("Literal construction selection mode required"))?;
    let ordinary = mode == 1
        && selected["supported"] == true
        && selected["active"] == true
        && selected["reason"] == "ordinary_construction_choice";
    let selected_building = if ordinary {
        Some(
            selected["building_id"]
                .as_str()
                .ok_or_else(|| bad("Fresh canonical selected building required"))?
                .to_owned(),
        )
    } else {
        None
    };
    let choice = &d["choice"];
    let choice_complete = choice["complete_choice_coverage"] == true;
    let control = match target.stage {
        Stage::EnsureOpen => &d["opener"],
        Stage::SelectBuilding => choice,
        _ => return Err(bad("Find stages use their own model")),
    };
    let mut ready = control["available"] == true
        && control["control_type_encoding"] == "15CButtonStandard"
        && control["callback"]["verified"] == true
        && control["local"]["conservative_dispatch_ready"] == true;
    match target.stage {
        Stage::EnsureOpen => {
            ready &= control["named_path_verified"] == true
                && control["all_named_containers_settled_shown"] == true
                && control["managed_target_mode"] == 17
                && d["opener_engine_gates"]["known_scalar_gate_clear"] == true;
        }
        Stage::SelectBuilding => {
            ready &= choice["canonical_choice_join_verified"] == true
                && choice["window"]["settled_visible"] == true;
        }
        _ => unreachable!("Find stages returned above"),
    }
    let state = OwnState {
        epoch: target.epoch.clone(),
        managed_mode: d["managed_mode_raw"]
            .as_i64()
            .and_then(|v| i32::try_from(v).ok()),
        panel: panel(&d["panel"])?,
        selected_mode: Some(mode),
        selected_building,
        // Ordinary selection and complete choices each join fresh bounded catalog
        // membership in the producer; never infer this from the requested string.
        catalog_coverage: if ordinary || choice_complete {
            Coverage::Complete
        } else {
            Coverage::NotChecked
        },
        choice_coverage: if choice_complete {
            Coverage::Complete
        } else {
            Coverage::Unavailable
        },
        requested_choice_present: target.stage == Stage::SelectBuilding
            && choice_complete
            && choice["canonical_choice_join_verified"] == true
            && choice["building_id"].as_str() == target.building_id.as_deref(),
        scoped_blockers_clear: Some(
            d["scoped_blockers"]["available"] == true
                && d["scoped_blockers"]["conservative_scoped_clear"] == true
                && d["local_gui_gates"]["gui_global_join_verified"] == true
                && d["local_gui_gates"]["conservative_local_gate_clear"] == true,
        ),
        local_dispatch_ready: Some(ready),
        find: None,
    };
    state.validate(target)?;
    Ok(state)
}
/// Find stages: own Find view facts from the SAME controls snapshot. Visibility
/// is the exact own container's local visible bit; close (2a3bbe0) hides the
/// window before setting bit 0x10, which then persists while hidden.
fn find_model(d: &Value, target: &Target) -> Result<OwnState> {
    let f = &d["find"];
    let view = &f["find_view"];
    if !f.is_object() || f["camera"]["look_at_height_invariant"] != true {
        return Err(bad("Same-snapshot own Find view required"));
    }
    let window = &view["window"];
    let visible = match view["materialized"].as_bool() {
        Some(true) if window["exact_container"] == true => window["visible_flag"].as_bool(),
        Some(_) => None,
        None => return Err(bad("Typed Find view materialization required")),
    };
    let text_focused = view["text_box_focused"].as_bool();
    let first = &view["results"]["first_row"];
    let first_state_id = (first["supported"] == true)
        .then(|| {
            first["state_id"]
                .as_i64()
                .and_then(|v| i32::try_from(v).ok())
        })
        .flatten();
    let requested = &f["target"];
    let (target_owned, camera_centered) = match target.state_id {
        Some(id) => {
            if requested["state_id"].as_i64() != Some(i64::from(id)) {
                return Err(bad("Requested own state differs from Find observation"));
            }
            (
                requested["own_owned_and_controlled"].as_bool(),
                requested["camera_centered"].as_bool(),
            )
        }
        None => (None, None),
    };
    let gates = &d["local_gui_gates"];
    let common_gates = gates["gui_global_join_verified"] == true
        && gates["flag150_raw"] == 0
        && gates["flag12_raw"] == 0
        && gates["receiver158_null"] == true;
    // FindOpen needs no text receiver; typing and Return need exactly the Find box.
    let ready = common_gates
        && match target.stage {
            Stage::FindOpen => gates["text_receiver230_null"] == true,
            _ => text_focused == Some(true),
        };
    let mode = d["selection"]["mode_raw"]
        .as_i64()
        .and_then(|v| i32::try_from(v).ok());
    let state = OwnState {
        epoch: target.epoch.clone(),
        managed_mode: d["managed_mode_raw"]
            .as_i64()
            .and_then(|v| i32::try_from(v).ok()),
        panel: panel(&d["panel"])?,
        selected_mode: mode,
        selected_building: None,
        catalog_coverage: Coverage::NotChecked,
        choice_coverage: Coverage::Unavailable,
        requested_choice_present: false,
        scoped_blockers_clear: Some(
            d["scoped_blockers"]["available"] == true
                && d["scoped_blockers"]["conservative_scoped_clear"] == true
                && gates["gui_global_join_verified"] == true,
        ),
        local_dispatch_ready: Some(ready),
        find: Some(FindState {
            visible,
            text_focused,
            query: view["query"].as_str().map(str::to_owned),
            first_state_id,
            target_owned,
            camera_centered,
        }),
    };
    state.validate(target)?;
    Ok(state)
}
pub(super) fn point(
    d: &Value,
    target: &Target,
    profile: &OwnedClientProfile,
) -> Result<PointCandidate> {
    model(d, target)?.preflight(target)?;
    let control = match target.stage {
        Stage::EnsureOpen => &d["opener"],
        Stage::SelectBuilding => &d["choice"],
        _ => return Err(bad("Find stages have no pointer candidate")),
    };
    if d["geometry_requested"] != true
        || control["renderer_type_encoding"] != "7CSprite"
        || control["renderer_control_join_verified"] != true
        || control["render_diagnostic"]["render_type_encoding"] != "7CSprite"
    {
        return Err(bad("Exact owned Standard/Sprite renderer required"));
    }
    let surface = candidate::control_surface(&control["render_diagnostic"])?;
    let codec = candidate::codec(&d["input_geometry"], profile)?;
    research_attempt::point_candidate(&surface, &codec)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    fn target(stage: Stage) -> Target {
        Target {
            epoch: crate::actions::Epoch {
                game: crate::platform::Identity {
                    pid: 123,
                    start_ticks: "456".into(),
                    exe: "/fixture/game".into(),
                    uid: 1000,
                },
                campaign: "fixture".into(),
                player: "GER".into(),
            },
            stage,
            building_id: (stage == Stage::SelectBuilding).then(|| "dockyard".into()),
            state_id: None,
        }
    }
    fn data(stage: Stage) -> Value {
        let render = json!({"field_layout_verified":true,"render_type_encoding":"7CSprite","resource_type_encoding":"11CSpriteType",
            "raw":{"visibility_flag":1,"cached_world_translation":[-106,-241],"pivot_dimensions":[512,288],"local_scalar":1,"physical_scaled_clip":[0,0,512,288]},
            "resource_cache":{"cache_equivalent":true,"no_mask_rejection_branch":true,"texture_dimensions":[72,72],"grid":[1,1],"orientation_raw":0},
            "parent_chain":{"graphics_root_closed":true,"status":"closed_to_owned_graphics_root","chain":[{"role":"owned_graphics_root_1f0"}],"unit_cache_diagnostic":{"cache_consistent_under_observed_unit_branch":true}}});
        let control = json!({"available":true,"control_type_encoding":"15CButtonStandard",
            "callback":{"verified":true},"local":{"conservative_dispatch_ready":true},
            "renderer_type_encoding":"7CSprite","renderer_control_join_verified":true,"render_diagnostic":render});
        let mut d = json!({"build_id":crate::game::BUILD_ID,"player":"GER","scope":"own_named_ui_and_public_requested_building_only",
            "building_id":"dockyard","input_authorized":false,"observation_paused_game":false,
            "managed_mode_raw":23,"selection":{"mode_raw":0,"supported":true,"active":false,"reason":"inactive","building_id":null},
            "panel":{"materialized":true,"flags_raw":0,"states_raw":[0,0],"settled_visible":false},
            "opener":control,"choice":control,"opener_engine_gates":{"known_scalar_gate_clear":true},
            "scoped_blockers":{"available":true,"conservative_scoped_clear":true},
            "local_gui_gates":{"gui_global_join_verified":true,"conservative_local_gate_clear":true},
            "geometry_requested":true,
            "input_geometry":{"build_id":crate::game::BUILD_ID,"physical_dimensions":[512,288],"logical_dimensions":[2560,1440],
                "logical_per_physical":[5.0,5.0],"physical_per_logical":[0.2,0.2],"input_mapping_verified":false,
                "input_source":{"point_diagnostic":{"getter_source_layout_verified":true,"window_association":{
                    "association_available":true,"normal_absolute_mode":true,"graphics_dimensions_match":true,
                    "native_window_id":42,"window_dimensions":[512,288],"relative_mode_raw":[0,0]}}}}});
        d["opener"]["named_path_verified"] = json!(true);
        d["opener"]["all_named_containers_settled_shown"] = json!(true);
        d["opener"]["managed_target_mode"] = json!(17);
        if stage == Stage::SelectBuilding {
            d["managed_mode_raw"] = json!(17);
            d["panel"]["flags_raw"] = json!(8);
            d["panel"]["settled_visible"] = json!(true);
            d["choice"]["complete_choice_coverage"] = json!(true);
            d["choice"]["canonical_choice_join_verified"] = json!(true);
            d["choice"]["building_id"] = json!("dockyard");
            d["choice"]["window"] = json!({"settled_visible":true});
        }
        d
    }
    fn profile() -> OwnedClientProfile {
        OwnedClientProfile {
            window: 42,
            client: [512, 288],
            logical: [2560, 1440],
            os_binding_verified: true,
            reviewed_builtin_client_origin: true,
        }
    }
    #[test]
    fn ordinary_stages_require_their_own_binding_and_same_snapshot_mapping() {
        for stage in [Stage::EnsureOpen, Stage::SelectBuilding] {
            let d = data(stage);
            let t = target(stage);
            let p = point(&d, &t, &profile()).unwrap();
            assert_eq!(p.client_pixel, [37, 70]);
            assert!(
                !p.input_authorized && !p.current_frame_verified && !p.effective_routing_verified
            );
            for path in [
                "/local_gui_gates/conservative_local_gate_clear",
                "/scoped_blockers/available",
                "/scoped_blockers/conservative_scoped_clear",
                "/geometry_requested",
            ] {
                let mut changed = d.clone();
                *changed.pointer_mut(path).unwrap() = json!(false);
                assert!(point(&changed, &t, &profile()).is_err(), "{path}");
            }
            let name = if stage == Stage::EnsureOpen {
                "opener"
            } else {
                "choice"
            };
            for path in [
                "available",
                "callback/verified",
                "local/conservative_dispatch_ready",
                "renderer_control_join_verified",
            ] {
                let mut changed = d.clone();
                *changed.pointer_mut(&format!("/{name}/{path}")).unwrap() = json!(false);
                assert!(point(&changed, &t, &profile()).is_err(), "{name}/{path}");
            }
            let mut changed = d.clone();
            changed[name]["renderer_type_encoding"] = json!("11CTextSprite");
            assert!(point(&changed, &t, &profile()).is_err());
            let mut changed = d.clone();
            changed["input_geometry"]["input_source"]["point_diagnostic"]["window_association"]
                ["native_window_id"] = json!(43);
            assert!(point(&changed, &t, &profile()).is_err());
        }
    }
    #[test]
    fn canonical_noops_do_not_need_click_readiness_but_selection_needs_complete_membership() {
        let t = target(Stage::SelectBuilding);
        let mut d = data(t.stage);
        d["selection"] = json!({"mode_raw":1,"supported":true,"active":true,"reason":"ordinary_construction_choice","building_id":"dockyard"});
        d["choice"]["local"]["conservative_dispatch_ready"] = json!(false);
        assert!(model(&d, &t).unwrap().reached(&t));
        d["choice"]["complete_choice_coverage"] = json!(false);
        assert!(!model(&d, &t).unwrap().reached(&t));
        d["choice"]["complete_choice_coverage"] = json!(true);
        d["selection"]["building_id"] = json!("arms_factory");
        assert!(!model(&d, &t).unwrap().reached(&t));
        let open = target(Stage::EnsureOpen);
        assert!(model(&d, &open).unwrap().reached(&open));
        d["panel"]["states_raw"] = json!([1, 0]);
        assert!(!model(&d, &open).unwrap().reached(&open));
    }
    #[test]
    fn closed_or_foreign_or_unbound_choice_never_becomes_dispatchable() {
        let t = target(Stage::SelectBuilding);
        let d = data(t.stage);
        for (path, value) in [
            ("/player", json!("FRA")),
            ("/building_id", json!("arms_factory")),
            ("/choice/canonical_choice_join_verified", json!(false)),
            ("/choice/complete_choice_coverage", json!(false)),
            ("/choice/window/settled_visible", json!(false)),
            ("/panel/materialized", json!(false)),
            ("/panel/states_raw", json!([1, 0])),
            ("/selection/mode_raw", json!(3)),
            ("/input_authorized", json!(true)),
        ] {
            let mut changed = d.clone();
            *changed.pointer_mut(path).unwrap() = value;
            assert!(
                model(&changed, &t).and_then(|s| s.preflight(&t)).is_err(),
                "{path}"
            );
        }
        let mut open = data(Stage::EnsureOpen);
        open["opener_engine_gates"]["known_scalar_gate_clear"] = json!(false);
        assert!(point(&open, &target(Stage::EnsureOpen), &profile()).is_err());
    }
    fn find_data(stage: Stage) -> (Value, Target) {
        let mut t = target(stage);
        t.state_id = matches!(stage, Stage::FindQuery | Stage::FindActivate).then_some(64);
        let mut d = data(Stage::EnsureOpen);
        d["local_gui_gates"] = json!({"gui_global_join_verified":true,"flag150_raw":0,"flag12_raw":0,
            "receiver158_null":true,"text_receiver230_null":stage == Stage::FindOpen,
            "conservative_local_gate_clear":stage == Stage::FindOpen});
        let open = stage != Stage::FindOpen;
        d["find"] = json!({"find_view":{"materialized":true,
            "window":{"exact_container":true,"flags_raw":if open {8} else {0},"visible_flag":open,"close_requested_flag":false,"settled_visibility_verified":false},
            "text_box_focused":open,"query":if stage == Stage::FindActivate {"64"} else {""},
            "results":{"result_count":0,"first_row":null}},
            "camera":{"eye":[0.0,300.0,0.0],"look_at":[0.0,9.5,0.0],"look_at_height_invariant":true},
            "target":if open {json!({"state_id":64,"own_owned_and_controlled":true,"anchor":{"kind":"state_template_map_position","map_position":[1,2]},"camera_centered":false})} else {json!(null)}});
        if stage == Stage::FindActivate {
            d["find"]["find_view"]["results"] =
                json!({"result_count":1,"first_row":{"kind_raw":2,"state_id":64,"supported":true}});
        }
        (d, t)
    }
    #[test]
    fn find_model_uses_same_snapshot_view_facts_and_receiver_gates() {
        for stage in [Stage::FindOpen, Stage::FindQuery, Stage::FindActivate] {
            let (d, t) = find_data(stage);
            let state = model(&d, &t).unwrap();
            assert!(state.preflight(&t).is_ok(), "{stage:?}");
            assert!(!state.reached(&t));
            assert!(point(&d, &t, &profile()).is_err());
        }
        // A previously closed view (persistent 0x10) can be reopened; a foreign
        // container makes visibility unknown.
        let (mut d, t) = find_data(Stage::FindOpen);
        d["find"]["find_view"]["window"]["close_requested_flag"] = json!(true);
        d["find"]["find_view"]["window"]["flags_raw"] = json!(0x10);
        assert!(model(&d, &t).unwrap().preflight(&t).is_ok());
        let (mut d, t) = find_data(Stage::FindQuery);
        d["find"]["find_view"]["window"]["exact_container"] = json!(false);
        assert!(model(&d, &t).unwrap().preflight(&t).is_err());
        // Typing needs exactly the Find text box as receiver.
        let (mut d, t) = find_data(Stage::FindQuery);
        d["find"]["find_view"]["text_box_focused"] = json!(false);
        assert!(model(&d, &t).unwrap().preflight(&t).is_err());
        // The observation must be for the requested state.
        let (mut d, t) = find_data(Stage::FindActivate);
        d["find"]["target"]["state_id"] = json!(65);
        assert!(model(&d, &t).is_err());
        // Missing camera invariant or Find section refuses.
        let (mut d, t) = find_data(Stage::FindOpen);
        d["find"]["camera"]["look_at_height_invariant"] = json!(false);
        assert!(model(&d, &t).is_err());
        let (mut d, t) = find_data(Stage::FindOpen);
        d["find"] = json!(null);
        assert!(model(&d, &t).is_err());
        // Activation completes only with the view closed and the camera centred.
        let (mut d, t) = find_data(Stage::FindActivate);
        d["find"]["find_view"]["window"]["visible_flag"] = json!(false);
        d["find"]["target"]["camera_centered"] = json!(true);
        assert!(model(&d, &t).unwrap().reached(&t));
    }
}
