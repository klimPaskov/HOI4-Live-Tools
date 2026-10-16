//! Conversion of a same-witness construction observation, never caller JSON.
use crate::{
    construction_stage::{Coverage, OwnState, PanelState, Stage, Target},
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
}
