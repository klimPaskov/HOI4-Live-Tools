//! Pointer-free projection of the existing witnessed own Focus/Politics model.
//! This is observation only; no stage preflight or command authority is exposed.
use crate::{
    error::{Error, Result},
    focus_stage::OwnState,
};
use serde_json::{json, Map, Value};

pub(crate) fn tool() -> Value {
    json!({"name":"hoi4_focus_ui","description":"Fresh compact own Focus/Politics state for one exact canonical focus ID. Reports active focus, managed mode, settled local roots and complete own detail/research-Info blocker counts. Requires owned Focus binding, but no input lease. Pure reads; no geometry, input, eligibility or global modal authority.",
        "inputSchema":{"type":"object","properties":{"focus_id":{"type":"string","minLength":1,"maxLength":128}},"required":["focus_id"],"additionalProperties":false},"annotations":{"readOnlyHint":true}})
}
pub(crate) fn key(args: &Map<String, Value>) -> Result<&str> {
    if args.len() != 1 {
        return Err(Error::new("invalid_arguments", "Only focus_id is accepted"));
    }
    let key = args
        .get("focus_id")
        .and_then(Value::as_str)
        .ok_or_else(|| Error::new("invalid_arguments", "Exact canonical focus_id required"))?;
    crate::own_focus::validate_key(key)?;
    Ok(key)
}
pub(crate) fn report(
    state: &OwnState,
    focus_date_raw: i32,
    politics_date_raw: i32,
    observed_unix_ms: u64,
    elapsed_ms: f64,
) -> Value {
    let counts = |value: Option<[usize; 2]>| {
        value.map(|[visible, animating]| json!({"visible":visible,"animating":animating}))
    };
    json!({"source":"native witnessed own Focus/Politics","build_id":crate::game::BUILD_ID,
        "owned":true,"player":state.epoch.player,"scope":"complete_own_local_gui_flags_only",
        "requested_focus_id":state.canonical_focus_id,"active_focus":state.active_focus,
        "managed_mode":state.managed_mode,"politics_root":state.politics_root,
        "focus_root":state.focus_root,"own_detail_counts":counts(state.own_detail_counts),
        "own_research_info_counts":counts(state.own_research_info_counts),
        "research_panel_root":state.research_panel_root,"research_tree_root":state.research_tree_root,
        "selected_focus_id":state.selected_focus,"requested_detail_ready":state.requested_detail_ready,
        "date_raw":focus_date_raw,"politics_date_raw":politics_date_raw,
        "freshness":{"observed_unix_ms":observed_unix_ms,"elapsed_ms":elapsed_ms},
        "global_modal_clear_available":false,"effective_routing_verified":false,
        "eligibility_available":false,"input_authorized":false,"observation_paused_game":false})
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        actions::Epoch,
        focus_stage::{ActiveFocus, LocalRoot},
        platform::Identity,
    };

    #[test]
    fn observation_schema_and_parser_accept_no_authority_or_geometry_arguments() {
        let schema = tool();
        assert_eq!(schema["annotations"]["readOnlyHint"], true);
        assert_eq!(schema["inputSchema"]["additionalProperties"], false);
        let good = json!({"focus_id":"GER_remilitarize_the_rhineland"});
        assert_eq!(key(good.as_object().unwrap()).unwrap(), good["focus_id"]);
        for bad in [
            json!({}),
            json!({"focus_id":1}),
            json!({"focus_id":""}),
            json!({"focus_id":"GER_a","lease_token":"t"}),
            json!({"focus_id":"GER_a","include_geometry":true}),
            json!({"focus_id":"GER_a","stage":"close_politics"}),
            json!({"focus_id":"x".repeat(129)}),
        ] {
            assert!(key(bad.as_object().unwrap()).is_err());
        }
    }
    #[test]
    fn report_preserves_unknown_blockers_and_omits_private_epoch_and_input_authority() {
        let mut state = OwnState {
            epoch: Epoch {
                game: Identity {
                    pid: 123,
                    start_ticks: "456".into(),
                    exe: "/owned/private-game".into(),
                    uid: 1000,
                },
                campaign: "private-session".into(),
                player: "GER".into(),
            },
            active_focus: ActiveFocus::Active("GER_rhineland".into()),
            canonical_focus_id: Some("GER_rhineland".into()),
            managed_mode: Some(12),
            politics_root: LocalRoot::Ready,
            focus_root: LocalRoot::Hidden,
            own_detail_counts: None,
            own_research_info_counts: Some([1, 2]),
            research_panel_root: LocalRoot::Unknown,
            research_tree_root: LocalRoot::Unknown,
            selected_focus: None,
            requested_detail_ready: None,
        };
        let out = report(&state, 60759371, 60759372, 1000, 4.5);
        assert!(out["own_detail_counts"].is_null());
        assert_eq!(
            out["own_research_info_counts"],
            json!({"visible":1,"animating":2})
        );
        assert_eq!(
            out["active_focus"],
            json!({"state":"Active","focus_id":"GER_rhineland"})
        );
        assert_eq!(out["politics_root"], "Ready");
        assert_eq!(out["focus_root"], "Hidden");
        for flag in [
            "input_authorized",
            "observation_paused_game",
            "effective_routing_verified",
            "global_modal_clear_available",
            "eligibility_available",
        ] {
            assert_eq!(out[flag], false);
        }
        let text = out.to_string();
        for private in [
            "private-session",
            "private-game",
            "start_ticks",
            "uid",
            "lease_token",
            "anchors",
        ] {
            assert!(!text.contains(private));
        }
        state.active_focus = ActiveFocus::Unknown;
        state.politics_root = LocalRoot::Animating;
        state.focus_root = LocalRoot::Unknown;
        state.own_detail_counts = Some([0, 0]);
        let out = report(&state, 60759371, 60759371, 1000, 4.5);
        assert_eq!(out["active_focus"], json!({"state":"Unknown"}));
        assert_eq!(out["politics_root"], "Animating");
        assert_eq!(out["focus_root"], "Unknown");
        assert_eq!(out["own_detail_counts"], json!({"visible":0,"animating":0}));
        assert!(serde_json::to_vec(&out).unwrap().len() < 2048);
    }
}
