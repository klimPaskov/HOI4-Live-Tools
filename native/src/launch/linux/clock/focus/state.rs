//! Typed own producer join, never caller supplied GUI authority.
use crate::{
    actions::Epoch,
    error::{Error, Result},
    focus_stage::{ActiveFocus, LocalRoot, OwnState},
    own_focus::{self, FocusContext},
    platform::Process,
    politics_focus::{self, PoliticsContext},
};
use serde_json::Value;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Scope {
    focus: [u64; 7],
    politics: [u64; 6],
    slots: Vec<u64>,
}
fn bad(message: &'static str) -> Error {
    Error::new("focus_precondition", message)
}

pub(super) fn local_root(window: &Value) -> Result<LocalRoot> {
    if window["materialized"] == false {
        return Ok(LocalRoot::Unknown);
    }
    if window["materialized"] != true {
        return Err(bad("Typed politics window required"));
    }
    let flags = window["flags_a5_raw"]
        .as_u64()
        .and_then(|v| u8::try_from(v).ok())
        .ok_or_else(|| bad("Literal own visibility flags required"))?;
    let a = window["states_raw"]
        .as_array()
        .filter(|a| a.len() == 2)
        .ok_or_else(|| bad("Both own transition channels required"))?;
    let states = [a[0].as_i64(), a[1].as_i64()];
    if states == [Some(0), Some(0)] {
        return Ok(if flags & 8 != 0 {
            LocalRoot::Ready
        } else {
            LocalRoot::Hidden
        });
    }
    if states.iter().all(|v| matches!(v, Some(0..=2))) {
        Ok(LocalRoot::Animating)
    } else {
        Ok(LocalRoot::Unknown)
    }
}
pub(super) fn pin(pin: &mut Option<Scope>, scope: Scope) -> Result<()> {
    if pin.as_ref().is_some_and(|old| old != &scope) {
        return Err(Error::new(
            "ownership_changed",
            "Pinned own focus/politics roots or slots changed",
        ));
    }
    if pin.is_none() {
        *pin = Some(scope);
    }
    Ok(())
}
pub(super) fn join(c: &FocusContext, p: &PoliticsContext) -> Result<Scope> {
    let a = &c.campaign;
    let b = p.campaign();
    let q = p.anchors();
    if a.player != b.player
        || a.playthrough_id != b.playthrough_id
        || a.anchors != b.anchors
        || a.difficulty != b.difficulty
        || a.historical_raw != b.historical_raw
        || a.date_raw > b.date_raw
        || [c.anchors[0], c.anchors[5], c.anchors[4], c.anchors[3]] != [q[0], q[1], q[2], q[4]]
    {
        return Err(Error::new(
            "torn_read",
            "Own focus and politics contexts disagree",
        ));
    }
    Ok(Scope {
        focus: c.anchors,
        politics: q,
        slots: c.slots.clone(),
    })
}
pub(super) fn model(
    d: &Value,
    c: &FocusContext,
    p: &Value,
    pc: &PoliticsContext,
    key: &str,
    epoch: &Epoch,
) -> Result<OwnState> {
    join(c, pc)?;
    if d["owned"] != true
        || d["build_id"] != crate::game::BUILD_ID
        || d["command_observation"]["requested_key"] != key
        || d["command_observation"]["same_reader_clock_context"] != true
        || c.campaign.player != epoch.player
        || p["country_binding_verified"] != true
    {
        return Err(bad("Fresh exact canonical own command context required"));
    }
    let facts = c
        .ui_facts
        .as_ref()
        .ok_or_else(|| bad("Producer-only own UI facts required"))?;
    if facts.local_summary()["managed_mode_raw"].as_i64() != Some(i64::from(pc.mode_raw())) {
        return Err(Error::new(
            "torn_read",
            "Managed mode changed across own readers",
        ));
    }
    let active = match (c.active_focus, c.active_key.as_ref()) {
        (0, None) => ActiveFocus::Idle,
        (pointer, Some(key)) if pointer != 0 => ActiveFocus::Active(key.clone()),
        _ => return Err(bad("Explicit canonical own active-focus state required")),
    };
    let focus_root = if facts.focus_view_local_ready() == Some(true) {
        LocalRoot::Ready
    } else if facts.focus_view_local_hidden() == Some(true) {
        LocalRoot::Hidden
    } else {
        LocalRoot::Unknown
    };
    let counts = facts.focus_detail_local_counts();
    let rows = d["details"]
        .as_array()
        .filter(|a| a.len() <= 32)
        .ok_or_else(|| bad("Bounded own details required"))?;
    let mut matching = 0;
    for row in rows.iter().filter(|row| row["selected_key"] == key) {
        let window = &row["window"];
        match window["materialized"].as_bool() {
            Some(false) => {}
            Some(true) => match window["conservative_container_visibility"].as_bool() {
                Some(true) => matching += 1,
                Some(false) => {}
                None => return Err(bad("Requested detail visibility unknown")),
            },
            None => return Err(bad("Requested detail materialization unknown")),
        }
    }
    if matching > 1 {
        return Err(bad("Unique requested own detail required"));
    }
    // Complete opaque counts ensure every retained window is settled. Older
    // body visibility predicates alone do not establish readiness at state1.
    let ready = counts.map(|n| n == [1, 0] && matching == 1);
    Ok(OwnState {
        epoch: epoch.clone(),
        active_focus: active,
        canonical_focus_id: Some(key.into()),
        managed_mode: Some(pc.mode_raw()),
        politics_root: local_root(&p["window"])?,
        focus_root,
        own_detail_counts: counts,
        own_research_info_counts: facts.own_research_info_local_counts(),
        selected_focus: if ready == Some(true) {
            Some(key.into())
        } else {
            None
        },
        requested_detail_ready: ready,
    })
}
pub(super) struct Observation {
    pub(super) state: OwnState,
    pub(super) context: FocusContext,
    pub(super) politics_context: PoliticsContext,
    pub(super) politics: Value,
    pub(super) scope: Scope,
}
pub(super) fn observe(
    process: &dyn Process,
    key: &str,
    epoch: &Epoch,
    geometry: bool,
) -> Result<Observation> {
    let (body, context) = own_focus::command_owned(process, key, false)?;
    let (politics, politics_context) = politics_focus::owned_snapshot(process, geometry)?;
    let scope = join(&context, &politics_context)?;
    let state = model(&body, &context, &politics, &politics_context, key, epoch)?;
    Ok(Observation {
        state,
        context,
        politics_context,
        politics,
        scope,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    #[test]
    fn absent_or_unsettled_politics_root_is_never_hidden() {
        assert_eq!(
            local_root(&json!({"materialized":false})).unwrap(),
            LocalRoot::Unknown
        );
        for states in [[1, 0], [0, 2], [99, 0]] {
            assert_ne!(
                local_root(&json!({"materialized":true,"flags_a5_raw":0,"states_raw":states}))
                    .unwrap(),
                LocalRoot::Hidden
            );
        }
        assert_eq!(
            local_root(&json!({"materialized":true,"flags_a5_raw":0,"states_raw":[0,0]})).unwrap(),
            LocalRoot::Hidden
        );
        assert_eq!(
            local_root(&json!({"materialized":true,"flags_a5_raw":8,"states_raw":[0,0]})).unwrap(),
            LocalRoot::Ready
        );
        assert!(
            local_root(&json!({"materialized":true,"flags_a5_raw":256,"states_raw":[0,0]}))
                .is_err()
        );
    }
    #[test]
    fn replacing_one_slot_or_gui_anchor_refuses_instead_of_repinning() {
        let s = Scope {
            focus: [1; 7],
            politics: [2; 6],
            slots: vec![3, 4],
        };
        let mut p = None;
        pin(&mut p, s.clone()).unwrap();
        let mut changed = s.clone();
        changed.slots[1] = 5;
        assert_eq!(pin(&mut p, changed).unwrap_err().code, "ownership_changed");
        let mut changed = s.clone();
        changed.politics[5] = 6;
        assert_eq!(pin(&mut p, changed).unwrap_err().code, "ownership_changed");
        assert_eq!(p, Some(s));
    }
}
