//! Volunteers and the attaché as a standing opportunity rule (plan key `volunteers`; engineer 118).
//!
//! The plan's rows name the war to help (`target`), the army sent (formations not needed by an operation inside the
//! recall lock) and the window as predicates (`when` to send, `until` to recall: the strategist computes the dates from
//! the defines with scripts/hoi4facts/volunteers.py: send no later than the first own war minus the arrival recall lock
//! and the transfer days, recall from the first own war minus the army's recall days and a margin). The rule never reads a war the player could not see: it presses the diplomacy
//! view's own action through `hoi4_diplomacy_act` (send_volunteers, recall_volunteers, send_attache) and the game's own legality answers (a war not started, the cap, the
//! divisions-required floor, an ideology gate and the recall lock all refuse with nothing posted). Weekly retries; the
//! rule sets decide (`volunteers`, `attaches`: allowed in FAIR on the agent's own initiative, owner 28 Nov).
use serde_json::{json, Value};

use super::{list, text, GameHour, State, Tools, DAY};
use crate::error::Result;

/// How long to wait before pressing again after an error code from the tool: a refusal by the game is retried next
/// week (the war may start, the lock may end); a missing family or route waits a month (the run's backend decides).
pub(super) fn retry_after(code: &str) -> GameHour {
    match code {
        "volunteers_unvalidated" | "in_process_unavailable" | "unknown_tool" => 30 * DAY,
        _ => 7 * DAY,
    }
}

const SENT: &str = "volunteers_sent";
const DONE: &str = "volunteers_done";
const FOREVER: GameHour = 1_000_000 * DAY;

/// The lane step (plan_affairs LANES `volunteers`): the plan's rows.
pub(super) fn volunteers_step(
    state: &mut State,
    tools: &dyn Tools,
    plan: &Value,
    lease: &str,
    now: GameHour,
) -> Result<Option<String>> {
    if !plan["volunteers"].is_array() {
        return Ok(None);
    }
    rows_step(state, tools, plan, lease, now)
}

/// The plan family's row form: `volunteers: [{"id","target","kind":"land"|"air"|"attache","army"?,"when"?,"until"?}]`.
/// A land row sends `army` to `target` while `when` holds and `until` does not, once, and recalls it once `until`
/// holds; an attache row sends the attaché once `when` holds; an air row is a rebase (hoi4_air_rebase), not this rule:
/// it is noted. Each row is done once per `id`; refusals by the game wait a week.
fn rows_step(
    state: &mut State,
    tools: &dyn Tools,
    plan: &Value,
    lease: &str,
    now: GameHour,
) -> Result<Option<String>> {
    if state.skipped(now, "volunteers", "x") {
        return Ok(None);
    }
    state.skip(now, "volunteers", "x", DAY);
    let rules = crate::fair_guard::Run::from_plan(plan);
    let mut notes = Vec::new();
    for row in list(plan, "volunteers") {
        let id = text(&row["id"]).to_string();
        let tag = text(&row["target"]).to_string();
        let kind = text(&row["kind"]);
        if id.is_empty() || tag.is_empty() || state.skipped(now, DONE, &id) {
            continue;
        }
        let holds = |state: &mut State, p: &Value| {
            p.is_null() || super::judge(state, tools, p, now) == crate::plan_predicate::Truth::True
        };
        let ended = !row["until"].is_null() && holds(state, &row["until"]);
        let sent = state.skipped(now, SENT, &id);
        let (key, args) = match kind {
            "land" => {
                let army = text(&row["army"]).to_string();
                if army.is_empty() {
                    notes.push(format!(
                        "{id}: a land row needs `army` (the hoi4_armies name)"
                    ));
                    continue;
                }
                if !rules.allows("volunteers") {
                    continue;
                }
                if sent && ended {
                    ("recall_volunteers", json!({"army": army}))
                } else if !sent && !ended && holds(state, &row["when"]) {
                    ("send_volunteers", json!({"army": army}))
                } else {
                    continue;
                }
            }
            "attache" => {
                if sent || ended || !rules.allows("attaches") || !holds(state, &row["when"]) {
                    continue;
                }
                ("send_attache", json!({}))
            }
            "air" => {
                notes.push(format!(
                    "{id}: air volunteers are a rebase of wings to {tag}'s air base (hoi4_air_rebase)"
                ));
                continue;
            }
            _ => continue,
        };
        if state.skipped(now, "volunteers_try", &id) {
            continue;
        }
        let mut args = args;
        args["action"] = json!(key);
        args["country"] = json!(tag);
        args["request_id"] = json!(format!("vol-{key}-{id}-{now}"));
        args["lease_token"] = json!(lease);
        match tools.call("hoi4_diplomacy_act", &args) {
            Ok(_) => {
                if key == "recall_volunteers" || key == "send_attache" {
                    state.skip(now, DONE, &id, FOREVER);
                }
                state.skip(now, SENT, &id, FOREVER);
                super::plan_manage::note(state, "volunteers", format!("{key} {id} -> {tag}"));
                return Ok(Some(format!("volunteers: {key} {id} to {tag}")));
            }
            Err(e) => {
                state.skip(now, "volunteers_try", &id, retry_after(e.code));
                notes.push(format!("{key} {id}: {} ({})", e.code, e.message));
            }
        }
    }
    if !notes.is_empty() {
        super::plan_manage::note(state, "volunteers", notes.join("; "));
    }
    Ok(None)
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Fake(std::cell::RefCell<Vec<Value>>);
    impl Tools for Fake {
        fn call(&self, name: &str, args: &Value) -> Result<Value> {
            if name == "hoi4_diplomacy_act" {
                self.0.borrow_mut().push(args.clone());
                return Ok(json!({"receipt": {"status": "Unobservable"}}));
            }
            Ok(json!({}))
        }
    }

    #[test]
    fn plan_rows_send_then_recall_through_the_diplomacy_tool() {
        let plan = json!({"volunteers": [
            {"id": "vol_spain", "target": "SPD", "kind": "land", "army": "Legion Condor",
                "until": "date >= 1939.4.1", "why": "x"},
            {"id": "att_spain", "target": "SPD", "kind": "attache", "why": "x"},
            {"id": "vol_air", "target": "SPD", "kind": "air", "why": "x"}]});
        let tools = Fake(std::cell::RefCell::new(Vec::new()));
        let mut state = State::default();
        let t0 = crate::plan_predicate::date_hour("1936.9.1").unwrap();
        let out = volunteers_step(&mut state, &tools, &plan, "l", t0).unwrap();
        assert_eq!(
            out.as_deref(),
            Some("volunteers: send_volunteers vol_spain to SPD")
        );
        let out = volunteers_step(&mut state, &tools, &plan, "l", t0 + DAY + 1).unwrap();
        assert_eq!(
            out.as_deref(),
            Some("volunteers: send_attache att_spain to SPD")
        );
        assert_eq!(
            volunteers_step(&mut state, &tools, &plan, "l", t0 + 2 * DAY + 2).unwrap(),
            None
        );
        let t1 = crate::plan_predicate::date_hour("1939.4.2").unwrap();
        let out = volunteers_step(&mut state, &tools, &plan, "l", t1).unwrap();
        assert_eq!(
            out.as_deref(),
            Some("volunteers: recall_volunteers vol_spain to SPD")
        );
        let sent = tools.0.borrow();
        assert_eq!(sent.len(), 3);
        assert_eq!(sent[0]["action"], "send_volunteers");
        assert_eq!(sent[0]["army"], "Legion Condor");
        assert_eq!(sent[0]["country"], "SPD");
        assert_eq!(sent[1]["action"], "send_attache");
        assert_eq!(sent[2]["action"], "recall_volunteers");
        drop(sent);
        assert_eq!(
            volunteers_step(&mut state, &tools, &plan, "l", t1 + 2 * DAY + 2).unwrap(),
            None
        );
        assert!(super::super::plan_manage::note_of(&state, "volunteers")
            .is_some_and(|n| n.contains("hoi4_air_rebase")));
    }

    #[test]
    fn refusals_retry_weekly_and_a_missing_route_monthly() {
        assert_eq!(retry_after("refused_by_game"), 7 * DAY);
        assert_eq!(retry_after("volunteers_unvalidated"), 30 * DAY);
    }
}
