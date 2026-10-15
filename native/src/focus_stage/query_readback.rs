//! Bounded historical evidence from reads already made after query dispatch.
//! No new reads, acceptance predicate, input retry or journal transition.
use super::OwnState;
use crate::error::Error;
use serde_json::{json, Value};

fn increment(value: &mut Value, key: &str) {
    value[key] = json!(value[key].as_u64().unwrap_or(0).saturating_add(1));
}

pub(crate) fn record(
    transport: &mut Value,
    sample: std::result::Result<(&OwnState, Option<&Value>), &Error>,
) {
    let evidence = &mut transport["postinput_readback"];
    if evidence.is_null() {
        *evidence = json!({"historical":true,"input_authorized":false,
            "reads":0,"accepted_reads":0,"rejected_reads":0,
            "first_rejection":null,"last_rejection":null,"last_observed":null});
    }
    increment(evidence, "reads");
    match sample {
        Err(error) => {
            increment(evidence, "rejected_reads");
            let error = json!({"code":error.code,
                "message":super::read_recovery::bounded_reason(&error.message)});
            if evidence["first_rejection"].is_null() {
                evidence["first_rejection"] = error.clone();
            }
            evidence["last_rejection"] = error;
        }
        Ok((state, selection)) => {
            increment(evidence, "accepted_reads");
            // Keep only the last witnessed snapshot. This is not a verified
            // action result and says nothing about state after the call ends.
            evidence["last_observed"] = json!({
                "active_focus":state.active_focus,"managed_mode":state.managed_mode,
                "focus_root":state.focus_root,"politics_root":state.politics_root,
                "own_detail_counts":state.own_detail_counts,
                "own_research_info_counts":state.own_research_info_counts,
                "research_panel_root":state.research_panel_root,
                "research_tree_root":state.research_tree_root,
                "search_control":state.search_control,"selection":selection});
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejected_reads_keep_counts_and_bounded_first_last_causes_only() {
        let mut transport = json!({"delivery":"ServerConfirmed"});
        for index in 0..100 {
            record(
                &mut transport,
                Err(&Error::new(
                    "torn_read",
                    format!("{index}:{}", "é\n".repeat(200)),
                )),
            );
        }
        let d = &transport["postinput_readback"];
        assert_eq!(d["reads"], 100);
        assert_eq!(d["rejected_reads"], 100);
        assert_eq!(d["accepted_reads"], 0);
        assert!(d["last_observed"].is_null());
        assert!(d["first_rejection"]["message"]
            .as_str()
            .unwrap()
            .starts_with("0:"));
        assert!(d["last_rejection"]["message"]
            .as_str()
            .unwrap()
            .starts_with("99:"));
        for key in ["first_rejection", "last_rejection"] {
            let s = d[key]["message"].as_str().unwrap();
            assert!(s.len() <= 160 && !s.chars().any(char::is_control));
        }
        assert_eq!(transport["delivery"], "ServerConfirmed");
        assert_eq!(d["historical"], true);
        assert_eq!(d["input_authorized"], false);
    }
}
