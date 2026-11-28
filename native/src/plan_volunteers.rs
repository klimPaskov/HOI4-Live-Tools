//! Volunteers and the attaché as a standing opportunity rule (plan key `volunteers`; engineer 118).
//!
//! The plan names the wars it would help (receivers: the tags the strategist expects at war, e.g. both sides' tags of a
//! civil war in the files), the army it sends (formations not needed by an operation inside the recall lock), and the
//! window the strategist computed from the defines (`send_until` = the first own war minus the arrival recall lock and
//! the transfer days; `recall_from` = the first own war minus the recall days of the army and a margin; see
//! scripts/hoi4facts/volunteers.py). The rule never reads a war the player could not see: it presses the diplomacy
//! view's own action through `hoi4_volunteers` and the game's own legality answers (a war not started, the cap, the
//! divisions-required floor, an ideology gate and the recall lock all refuse with nothing posted). Weekly retries; the
//! rule sets decide (`volunteers`, `attaches`: allowed in FAIR on the agent's own initiative, owner 28 Nov).
use serde_json::{json, Value};

use super::{list, text, GameHour, State, Tools, DAY};
use crate::error::Result;

/// The phase of the rule for one receiver at `now`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Phase {
    /// before `send_until`: send when the game allows it
    Send,
    /// between `send_until` and `recall_from`: hold what was sent
    Hold,
    /// from `recall_from`: bring them home before our own war
    Recall,
}

pub(super) fn phase(
    now: GameHour,
    send_until: Option<GameHour>,
    recall_from: Option<GameHour>,
) -> Phase {
    if recall_from.is_some_and(|r| now >= r) {
        Phase::Recall
    } else if send_until.is_some_and(|s| now >= s) {
        Phase::Hold
    } else {
        Phase::Send
    }
}

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

/// The lane step (plan_affairs LANES `volunteers`).
pub(super) fn volunteers_step(
    state: &mut State,
    tools: &dyn Tools,
    plan: &Value,
    lease: &str,
    now: GameHour,
) -> Result<Option<String>> {
    let cfg = &plan["volunteers"];
    if !cfg.is_object() || cfg["enabled"] == json!(false) || state.skipped(now, "volunteers", "x") {
        return Ok(None);
    }
    state.skip(now, "volunteers", "x", DAY);
    let rules = crate::fair_guard::Run::from_plan(plan);
    let date = |v: &Value| v.as_str().and_then(crate::plan_predicate::date_hour);
    let ph = phase(now, date(&cfg["send_until"]), date(&cfg["recall_from"]));
    let army = text(&cfg["army"]).to_string();
    if rules.allows("volunteers") && !army.is_empty() {
        for r in list(cfg, "receivers") {
            let Some(tag) = r.as_str() else { continue };
            let action = match ph {
                Phase::Send if !state.skipped(now, SENT, tag) => "send",
                Phase::Recall
                    if state.skipped(now, SENT, tag) && !state.skipped(now, DONE, tag) =>
                {
                    "recall"
                }
                _ => continue,
            };
            if state.skipped(now, "volunteers_try", tag) {
                continue;
            }
            let args = json!({"action":action,"country":tag,"army":army,
                "request_id":format!("vol-{action}-{tag}-{now}"),"lease_token":lease});
            match tools.call("hoi4_volunteers", &args) {
                Ok(_) => {
                    if action == "send" {
                        // one force per war; it returns when the war ends (the game) or at recall_from (the rule)
                        state.skip(now, SENT, tag, FOREVER);
                    } else {
                        state.skip(now, DONE, tag, FOREVER);
                    }
                    super::plan_manage::note(
                        state,
                        "volunteers",
                        format!("{action} {army} -> {tag}"),
                    );
                    return Ok(Some(format!("volunteers: {action} {army} to {tag}")));
                }
                Err(e) => {
                    state.skip(now, "volunteers_try", tag, retry_after(e.code));
                    super::plan_manage::note(
                        state,
                        "volunteers",
                        format!("{action} {tag}: {} ({})", e.code, e.message),
                    );
                }
            }
        }
    }
    if rules.allows("attaches") && ph != Phase::Recall {
        for r in list(cfg, "attache") {
            let Some(tag) = r.as_str() else { continue };
            if state.skipped(now, "attache_sent", tag) || state.skipped(now, "attache_try", tag) {
                continue;
            }
            let args = json!({"action":"attache","country":tag,
                "request_id":format!("vol-attache-{tag}-{now}"),"lease_token":lease});
            match tools.call("hoi4_volunteers", &args) {
                Ok(_) => {
                    state.skip(now, "attache_sent", tag, FOREVER);
                    super::plan_manage::note(state, "volunteers", format!("attache -> {tag}"));
                    return Ok(Some(format!("volunteers: attache to {tag}")));
                }
                Err(e) => {
                    state.skip(now, "attache_try", tag, retry_after(e.code));
                    super::plan_manage::note(
                        state,
                        "volunteers",
                        format!("attache {tag}: {}", e.code),
                    );
                }
            }
        }
    }
    Ok(None)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn phases_follow_the_window() {
        let (s, r) = (Some(100 * DAY), Some(200 * DAY));
        assert_eq!(phase(10 * DAY, s, r), Phase::Send);
        assert_eq!(phase(150 * DAY, s, r), Phase::Hold);
        assert_eq!(phase(200 * DAY, s, r), Phase::Recall);
        assert_eq!(phase(10 * DAY, None, None), Phase::Send);
        assert_eq!(phase(300 * DAY, None, r), Phase::Recall);
    }

    #[test]
    fn refusals_retry_weekly_and_a_missing_route_monthly() {
        assert_eq!(retry_after("refused_by_game"), 7 * DAY);
        assert_eq!(retry_after("volunteers_unvalidated"), 30 * DAY);
    }
}
