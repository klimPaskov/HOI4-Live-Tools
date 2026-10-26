//! Canonical diplomacy action keys, how they map to the rows of the game's
//! diplomacy view, and the pure settlement rule of "compare only what the
//! action can change": each action names the relation (or war goal, or war,
//! or faction membership) between the player and the one target country that
//! it can create or remove, and nothing else is compared.
//!
//! Row names are the English ones the macOS 1.19.3.0 diplomacy view lists
//! (seen live on 26 October 2026 toward Poland). Rows the player's current
//! situation does not offer simply do not appear; resolving a key against the
//! rows that are offered gives the typed refusal `action_not_offered` with the
//! list of what is offered.
use super::settle::Verdict;
use crate::error::{Error, Result};
use std::collections::BTreeSet;

/// Which party the relation points from: `FromMe` means the first party of
/// the relation object is the player.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Dir {
    FromMe,
    ToMe,
    Either,
}

#[derive(Clone, Copy, Debug)]
pub(super) enum Effect {
    /// One of these relation kinds with the target appears.
    Appears(&'static [(&'static str, Dir)]),
    /// One of these relation kinds with the target disappears.
    Disappears(&'static [(&'static str, Dir)]),
    /// One more own war goal against the target (justification started).
    GoalAdded,
    /// One own war goal against the target fewer (justification cancelled).
    GoalRemoved,
    /// A war relation with the target appears.
    WarStarted,
    /// The target joins the player's faction.
    FactionJoined,
    /// The target leaves or is removed from the player's faction.
    FactionLeft,
    /// No memory evidence is known; the offered rows and political power are
    /// the only proof.
    RowsOnly,
}

#[derive(Clone, Copy, Debug)]
pub(super) struct Spec {
    pub key: &'static str,
    /// Row names (English, case-insensitive) that realise the key.
    pub rows: &'static [&'static str],
    pub effect: Effect,
    /// The target's AI decides: no relation appears when it refuses.
    pub ai_decides: bool,
    /// The row opens a dialog the player must Send (false: the row click acts).
    pub dialog: bool,
}

const G: &[(&str, Dir)] = &[("guarantee", Dir::FromMe)];
const PACT: &[(&str, Dir)] = &[("non_aggression", Dir::Either)];
const ACCESS_ASK: &[(&str, Dir)] = &[("military_access", Dir::FromMe)];
const ACCESS_OFFER: &[(&str, Dir)] = &[
    ("military_access", Dir::ToMe),
    ("offer_military_access", Dir::Either),
];
const DOCK_ASK: &[(&str, Dir)] = &[("docking_rights", Dir::Either)];
const DOCK_OFFER: &[(&str, Dir)] = &[("offer_docking_rights", Dir::Either), ("docking_rights", Dir::Either)];
const IMPROVE: &[(&str, Dir)] = &[("improve_relations", Dir::FromMe)];
const LEND: &[(&str, Dir)] = &[("lend_lease", Dir::Either)];
const ATTACHE: &[(&str, Dir)] = &[("attache", Dir::FromMe)];
const BOOST: &[(&str, Dir)] = &[("boost_party_popularity", Dir::FromMe)];
const EMBARGO: &[(&str, Dir)] = &[("embargo", Dir::FromMe)];
const LICENCE: &[(&str, Dir)] = &[("licence_production_access", Dir::Either)];

pub(super) const SPECS: &[Spec] = &[
    Spec { key: "improve_relations", rows: &["Improve relations"], effect: Effect::Appears(IMPROVE), ai_decides: false, dialog: true },
    Spec { key: "stop_improving_relations", rows: &["Stop improving relations"], effect: Effect::Disappears(IMPROVE), ai_decides: false, dialog: true },
    Spec { key: "guarantee", rows: &["Guarantee independence"], effect: Effect::Appears(G), ai_decides: false, dialog: true },
    Spec { key: "revoke_guarantee", rows: &["Revoke guarantee", "Remove guarantee", "Revoke guarantee independence", "Cancel guarantee"], effect: Effect::Disappears(G), ai_decides: false, dialog: true },
    Spec { key: "non_aggression_pact", rows: &["Non-aggression pact"], effect: Effect::Appears(PACT), ai_decides: true, dialog: true },
    Spec { key: "cancel_non_aggression_pact", rows: &["Cancel non-aggression pact", "Break non-aggression pact", "Cancel pact"], effect: Effect::Disappears(PACT), ai_decides: false, dialog: true },
    Spec { key: "ask_military_access", rows: &["Ask for military access", "Request military access"], effect: Effect::Appears(ACCESS_ASK), ai_decides: true, dialog: true },
    Spec { key: "offer_military_access", rows: &["Offer military access", "Give military access"], effect: Effect::Appears(ACCESS_OFFER), ai_decides: true, dialog: true },
    Spec { key: "revoke_military_access", rows: &["Cancel our military access", "Cancel their military access", "Revoke military access", "Cancel military access", "Remove military access", "Revoke access"], effect: Effect::Disappears(&[("military_access", Dir::Either)]), ai_decides: false, dialog: true },
    Spec { key: "ask_docking_rights", rows: &["Ask for docking rights", "Request docking rights"], effect: Effect::Appears(DOCK_ASK), ai_decides: true, dialog: true },
    Spec { key: "offer_docking_rights", rows: &["Offer docking rights", "Give docking rights"], effect: Effect::Appears(DOCK_OFFER), ai_decides: true, dialog: true },
    Spec { key: "revoke_docking_rights", rows: &["Revoke docking rights", "Cancel docking rights"], effect: Effect::Disappears(&[("docking_rights", Dir::Either), ("offer_docking_rights", Dir::Either)]), ai_decides: false, dialog: false },
    Spec { key: "send_attache", rows: &["Send attaché", "Send attache"], effect: Effect::Appears(ATTACHE), ai_decides: true, dialog: true },
    Spec { key: "invite_to_faction", rows: &["Invite to faction"], effect: Effect::FactionJoined, ai_decides: true, dialog: true },
    Spec { key: "join_faction", rows: &["Request to join faction", "Join faction", "Ask to join faction", "Apply to join faction"], effect: Effect::FactionJoined, ai_decides: true, dialog: true },
    Spec { key: "kick_from_faction", rows: &["Kick from faction", "Remove from faction", "Expel from faction"], effect: Effect::FactionLeft, ai_decides: false, dialog: true },
    Spec { key: "request_licence_production", rows: &["Request License Production", "Request Licence Production"], effect: Effect::Appears(LICENCE), ai_decides: true, dialog: true },
    Spec { key: "start_lend_lease", rows: &["Start Lend lease", "Start Lend-lease", "Offer Lend lease"], effect: Effect::Appears(LEND), ai_decides: false, dialog: true },
    Spec { key: "request_lend_lease", rows: &["Request Lend lease", "Request Lend-lease"], effect: Effect::Appears(LEND), ai_decides: true, dialog: true },
    Spec { key: "send_volunteers", rows: &["Send Volunteers"], effect: Effect::RowsOnly, ai_decides: false, dialog: true },
    Spec { key: "expeditionary_force", rows: &["Expeditionary Force", "Send Expeditionary Force"], effect: Effect::RowsOnly, ai_decides: false, dialog: true },
    Spec { key: "return_expeditions", rows: &["Return Expeditions"], effect: Effect::RowsOnly, ai_decides: false, dialog: false },
    Spec { key: "boost_party_popularity", rows: &["Boost party popularity"], effect: Effect::Appears(BOOST), ai_decides: false, dialog: true },
    Spec { key: "stage_coup", rows: &["Stage a coup"], effect: Effect::RowsOnly, ai_decides: false, dialog: true },
    Spec { key: "embargo", rows: &["Embargo", "Impose embargo"], effect: Effect::Appears(EMBARGO), ai_decides: false, dialog: false },
    Spec { key: "lift_embargo", rows: &["Lift embargo", "Remove embargo", "Cancel embargo"], effect: Effect::Disappears(EMBARGO), ai_decides: false, dialog: false },
    Spec { key: "justify_war_goal", rows: &["Justify War goal", "Justify war goal"], effect: Effect::GoalAdded, ai_decides: false, dialog: true },
    Spec { key: "add_war_goal", rows: &["Add War goal", "Add war goal"], effect: Effect::GoalAdded, ai_decides: false, dialog: true },
    Spec { key: "cancel_war_goal", rows: &["Cancel Justification", "Cancel justification", "Cancel War goal"], effect: Effect::GoalRemoved, ai_decides: false, dialog: true },
    Spec { key: "annex_subject", rows: &["Annex", "Annex subject", "Annex puppet"], effect: Effect::Disappears(&[("subject", Dir::Either)]), ai_decides: false, dialog: true },
    Spec { key: "release_subject", rows: &["Release subject", "Release as puppet", "Release puppet"], effect: Effect::RowsOnly, ai_decides: false, dialog: true },
    Spec { key: "diplomatic_pressure", rows: &["Diplomatic pressure", "Send diplomatic pressure"], effect: Effect::RowsOnly, ai_decides: false, dialog: true },
    Spec { key: "declare_war", rows: &["Declare war"], effect: Effect::WarStarted, ai_decides: false, dialog: true },
    Spec { key: "call_allies", rows: &["Call allies", "Call to arms"], effect: Effect::RowsOnly, ai_decides: false, dialog: true },
    Spec { key: "join_allies", rows: &["Join allies", "Join ally's war"], effect: Effect::RowsOnly, ai_decides: false, dialog: true },
    Spec { key: "ask_state_control", rows: &["Ask for state control"], effect: Effect::RowsOnly, ai_decides: true, dialog: true },
    Spec { key: "give_state_control", rows: &["Give state control"], effect: Effect::RowsOnly, ai_decides: false, dialog: true },
    Spec { key: "request_garrison_support", rows: &["Request Garrison support"], effect: Effect::RowsOnly, ai_decides: true, dialog: false },
];

/// Lower case, `_` and `-` as spaces, accents folded, single spaces.
pub(super) fn norm(text: &str) -> String {
    let mut out = String::new();
    for c in text.chars() {
        let c = match c {
            '_' | '-' | '\'' | '\u{2019}' => ' ',
            'é' | 'è' | 'É' => 'e',
            c => c.to_ascii_lowercase(),
        };
        out.push(c);
    }
    out.split_whitespace().collect::<Vec<_>>().join(" ")
}

pub(super) fn spec_of_key(text: &str) -> Option<&'static Spec> {
    let wanted = norm(text);
    SPECS.iter().find(|s| norm(s.key) == wanted)
}
/// The spec whose row list holds this offered row name.
pub(super) fn spec_of_row(row: &str) -> Option<&'static Spec> {
    let wanted = norm(row);
    SPECS
        .iter()
        .find(|s| s.rows.iter().any(|r| norm(r) == wanted))
}

/// The offered row an action argument names: a canonical key, an exact row
/// name, or a unique substring of one row name. Errors are typed
/// `action_not_offered` (with the rows that are offered) or
/// `action_ambiguous`.
pub(super) fn resolve(action: &str, offered: &[String]) -> Result<(String, Option<&'static Spec>)> {
    let wanted = norm(action);
    if wanted.is_empty() {
        return Err(Error::new("invalid_arguments", "Empty action"));
    }
    if let Some(spec) = spec_of_key(action) {
        for row in offered {
            if spec.rows.iter().any(|r| norm(r) == norm(row)) {
                return Ok((row.clone(), Some(spec)));
            }
        }
        return Err(not_offered(action, offered, Some(spec)));
    }
    if let Some(row) = offered.iter().find(|r| norm(r) == wanted) {
        return Ok((row.clone(), spec_of_row(row)));
    }
    let hits: Vec<&String> = offered.iter().filter(|r| norm(r).contains(&wanted)).collect();
    match hits.len() {
        1 => Ok((hits[0].clone(), spec_of_row(hits[0]))),
        0 => Err(not_offered(action, offered, None)),
        _ => Err(Error::new(
            "action_ambiguous",
            format!(
                "'{action}' matches several rows: {}",
                hits.iter().map(|h| h.as_str()).collect::<Vec<_>>().join(", ")
            ),
        )),
    }
}
fn not_offered(action: &str, offered: &[String], spec: Option<&Spec>) -> Error {
    let keys: Vec<&str> = SPECS.iter().map(|s| s.key).collect();
    let hint = spec
        .map(|s| format!(" (it would be the row {})", s.rows.join(" / ")))
        .unwrap_or_default();
    Error::new(
        "action_not_offered",
        format!(
            "'{action}' is not offered toward this country now{hint}. Offered rows: {}. Known action keys: {}",
            offered.join(", "),
            keys.join(", ")
        ),
    )
}

/// What a snapshot of the player's relations with the target holds. Built by
/// `diplomacy_state::snapshot`; plain data so the rules are testable.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(super) struct Snapshot {
    /// (relation kind, first party is the player)
    pub rels: BTreeSet<(String, bool)>,
    /// Own war goals (justifying or justified) against the target.
    pub goals: usize,
    pub at_war: bool,
    /// The target is a member of the player's faction.
    pub same_faction: bool,
}

fn has(s: &Snapshot, list: &[(&str, Dir)]) -> bool {
    list.iter().any(|(kind, dir)| {
        s.rels.iter().any(|(k, from_me)| {
            k == kind
                && match dir {
                    Dir::FromMe => *from_me,
                    Dir::ToMe => !*from_me,
                    Dir::Either => true,
                }
        })
    })
}

/// Whether the effect is already in place (nothing to change).
pub(super) fn already(effect: Effect, now: &Snapshot) -> bool {
    match effect {
        Effect::Appears(list) => has(now, list),
        Effect::WarStarted => now.at_war,
        Effect::FactionJoined => now.same_faction,
        _ => false,
    }
}

/// Settlement of one action from the compared fields only. `before` is
/// `None` on a retry of an unresolved request (then only reaching the target
/// counts). `proof` is the dispatch's own evidence (offered rows changed or
/// political power was spent).
pub(super) fn verdict(
    spec: Option<&Spec>,
    before: Option<&Snapshot>,
    now: &Snapshot,
    proof: bool,
) -> Verdict {
    let Some(spec) = spec else {
        return if proof { Verdict::Reached } else { Verdict::Unknown };
    };
    let ai = spec.ai_decides;
    let none = Snapshot::default();
    let b = before.unwrap_or(&none);
    // `Some(true)`: the target effect is in the snapshot now and was not before.
    let (reached, absent) = match spec.effect {
        Effect::Appears(list) => (has(now, list) && (before.is_none() || !has(b, list)), !has(now, list)),
        Effect::Disappears(list) => (before.is_some() && has(b, list) && !has(now, list), has(now, list) || before.is_none()),
        Effect::GoalAdded => (now.goals > b.goals && (before.is_some() || now.goals > 0), now.goals <= b.goals),
        Effect::GoalRemoved => (before.is_some() && now.goals < b.goals, now.goals >= b.goals || before.is_none()),
        Effect::WarStarted => (now.at_war && !b.at_war, !now.at_war),
        Effect::FactionJoined => (now.same_faction && !b.same_faction, !now.same_faction),
        Effect::FactionLeft => (before.is_some() && b.same_faction && !now.same_faction, now.same_faction || before.is_none()),
        Effect::RowsOnly => (proof, !proof),
    };
    if reached {
        Verdict::Reached
    } else if matches!(spec.effect, Effect::RowsOnly) {
        Verdict::Unknown
    } else if absent {
        // The relation is not there. A non-AI action whose own proof
        // (rows/PP) says it worked is Reached with a relation class we do not
        // read yet; an AI refusal or a click that did nothing is Unchanged.
        if proof && !ai {
            Verdict::Reached
        } else if before.is_none() {
            Verdict::Unknown
        } else {
            Verdict::Unchanged
        }
    } else {
        Verdict::Diverged
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rows(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| s.to_string()).collect()
    }
    fn snap(rels: &[(&str, bool)]) -> Snapshot {
        Snapshot {
            rels: rels.iter().map(|(k, f)| (k.to_string(), *f)).collect(),
            ..Snapshot::default()
        }
    }

    #[test]
    fn keys_resolve_to_offered_rows() {
        let offered = rows(&["Declare war", "Guarantee independence", "Send attaché", "Non-aggression pact"]);
        assert_eq!(resolve("guarantee", &offered).unwrap().0, "Guarantee independence");
        assert_eq!(resolve("non_aggression_pact", &offered).unwrap().0, "Non-aggression pact");
        assert_eq!(resolve("Non-Aggression Pact", &offered).unwrap().0, "Non-aggression pact");
        assert_eq!(resolve("send_attache", &offered).unwrap().0, "Send attaché");
        assert_eq!(resolve("declare_war", &offered).unwrap().1.unwrap().key, "declare_war");
        assert_eq!(resolve("aggression", &offered).unwrap().0, "Non-aggression pact");
    }

    #[test]
    fn unoffered_and_ambiguous_actions_are_typed_refusals() {
        let offered = rows(&["Ask for military access", "Offer military access"]);
        let e = resolve("guarantee", &offered).unwrap_err();
        assert_eq!(e.code, "action_not_offered");
        assert!(e.message.contains("Ask for military access"));
        assert_eq!(resolve("military access", &offered).unwrap_err().code, "action_ambiguous");
        assert_eq!(resolve("", &offered).unwrap_err().code, "invalid_arguments");
    }

    #[test]
    fn every_spec_key_and_row_is_unique() {
        let mut keys = BTreeSet::new();
        for s in SPECS {
            assert!(keys.insert(s.key), "{}", s.key);
            assert_eq!(spec_of_key(s.key).unwrap().key, s.key);
            assert_eq!(spec_of_row(s.rows[0]).unwrap().key, s.key);
        }
    }

    #[test]
    fn guarantee_settles_on_the_relation_only() {
        let spec = spec_of_key("guarantee");
        let before = snap(&[]);
        let now = snap(&[("guarantee", true)]);
        assert_eq!(verdict(spec, Some(&before), &now, false), Verdict::Reached);
        // a guarantee the target gives us is not ours
        let theirs = snap(&[("guarantee", false)]);
        assert_eq!(verdict(spec, Some(&before), &theirs, false), Verdict::Unchanged);
        // unrelated relations appearing change nothing
        let other = snap(&[("military_access", true)]);
        assert_eq!(verdict(spec, Some(&before), &other, false), Verdict::Unchanged);
        // retry of an unresolved request: only reaching counts
        assert_eq!(verdict(spec, None, &now, false), Verdict::Reached);
        assert_eq!(verdict(spec, None, &before, false), Verdict::Unknown);
    }

    #[test]
    fn ai_refusal_is_unchanged_even_when_political_power_moved() {
        let spec = spec_of_key("non_aggression_pact");
        let before = snap(&[]);
        assert_eq!(verdict(spec, Some(&before), &before, true), Verdict::Unchanged);
        assert_eq!(
            verdict(spec, Some(&before), &snap(&[("non_aggression", false)]), false),
            Verdict::Reached
        );
        // a non-AI action with its own proof but an unread relation class
        let imp = spec_of_key("improve_relations");
        assert_eq!(verdict(imp, Some(&before), &before, true), Verdict::Reached);
        assert_eq!(verdict(imp, Some(&before), &before, false), Verdict::Unchanged);
        let on = snap(&[("improve_relations", true)]);
        assert_eq!(verdict(imp, Some(&before), &on, false), Verdict::Reached);
        let stop = spec_of_key("stop_improving_relations");
        assert_eq!(verdict(stop, Some(&on), &before, false), Verdict::Reached);
    }

    #[test]
    fn war_goals_and_war_are_counted() {
        let justify = spec_of_key("justify_war_goal");
        let mut before = Snapshot::default();
        let now = Snapshot { goals: 1, ..Snapshot::default() };
        assert_eq!(verdict(justify, Some(&before), &now, false), Verdict::Reached);
        assert_eq!(verdict(justify, Some(&before), &before, false), Verdict::Unchanged);
        let cancel = spec_of_key("cancel_war_goal");
        before.goals = 1;
        assert_eq!(verdict(cancel, Some(&before), &Snapshot::default(), false), Verdict::Reached);
        assert_eq!(verdict(cancel, Some(&before), &before, false), Verdict::Unchanged);
        let war = spec_of_key("declare_war");
        let at_war = Snapshot { at_war: true, ..Snapshot::default() };
        assert_eq!(verdict(war, Some(&Snapshot::default()), &at_war, false), Verdict::Reached);
        assert_eq!(verdict(war, Some(&Snapshot::default()), &Snapshot::default(), true), Verdict::Reached);
        assert!(already(Effect::WarStarted, &at_war));
        assert!(!already(Effect::WarStarted, &Snapshot::default()));
    }

    #[test]
    fn revoking_needs_the_relation_to_disappear() {
        let revoke = spec_of_key("revoke_guarantee");
        let before = snap(&[("guarantee", true)]);
        assert_eq!(verdict(revoke, Some(&before), &snap(&[]), false), Verdict::Reached);
        assert_eq!(verdict(revoke, Some(&before), &before, false), Verdict::Unchanged);
        assert_eq!(verdict(revoke, None, &snap(&[]), false), Verdict::Unknown);
    }

    #[test]
    fn faction_membership_effects() {
        let invite = spec_of_key("invite_to_faction");
        let now = Snapshot { same_faction: true, ..Snapshot::default() };
        assert_eq!(verdict(invite, Some(&Snapshot::default()), &now, false), Verdict::Reached);
        assert_eq!(verdict(invite, Some(&Snapshot::default()), &Snapshot::default(), true), Verdict::Unchanged);
        let kick = spec_of_key("kick_from_faction");
        assert_eq!(verdict(kick, Some(&now), &Snapshot::default(), false), Verdict::Reached);
    }
}
