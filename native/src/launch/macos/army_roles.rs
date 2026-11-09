//! Plan role names for army tools. The plan and the brief name armies by their `army_layout`
//! role (`name_hint`: "North_strike", "South_army"), the game by "Army 1..8". An army-addressed
//! argument that names no own army or army group is rewritten before the tool runs: to the one
//! army whose name matches it loosely (case, spaces, underscores and dashes aside), else to the
//! army the executor bound to that role (`layout` in plan_state.json next to plan.json). The
//! reply names every rewrite (`army_resolved`). Seen live 9 Nov 2026 (Italy run s1): the player
//! sent hoi4_army_front {"army":"North_strike"} and got army_unknown.
use serde_json::{json, Value};
use std::collections::BTreeMap;

/// Arguments that hold one army name.
const KEYS: [&str; 4] = ["army", "source", "target", "into"];

/// Tools whose army arguments may name a role.
pub(super) fn addressed(tool: &str) -> bool {
    (tool.starts_with("hoi4_army_") && tool != "hoi4_army_create")
        || matches!(tool, "hoi4_division_disband" | "hoi4_supply")
}

/// Case, spaces, underscores and dashes do not matter.
fn key(name: &str) -> String {
    name.chars()
        .filter(|c| !matches!(c, ' ' | '_' | '-'))
        .flat_map(char::to_lowercase)
        .collect()
}

/// The army `asked` stands for when no own army or army group carries exactly that name: the
/// single army or group matching it loosely, else the army bound to the role of that name.
/// None when it already names one exactly, or names nothing known.
fn resolve<'a>(
    asked: &str,
    names: &'a [String],
    roles: &'a BTreeMap<String, String>,
) -> Option<&'a str> {
    if names.iter().any(|n| n == asked) {
        return None;
    }
    let k = key(asked);
    let loose: Vec<&String> = names.iter().filter(|n| key(n) == k).collect();
    if let [one] = loose.as_slice() {
        return Some(one.as_str());
    }
    let bound = roles
        .get(asked)
        .or_else(|| {
            let mut hits = roles.iter().filter(|(hint, _)| key(hint) == k);
            match (hits.next(), hits.next()) {
                (Some((_, army)), None) => Some(army),
                _ => None,
            }
        })
        .filter(|a| !a.is_empty())?;
    // The bound army as the game names it now (a renamed army keeps its role).
    names
        .iter()
        .find(|n| *n == bound)
        .or_else(|| {
            let mut hits = names.iter().filter(|n| key(n) == key(bound));
            match (hits.next(), hits.next()) {
                (Some(n), None) => Some(n),
                _ => None,
            }
        })
        .map(String::as_str)
}

/// Every army name the arguments address.
pub(super) fn asked(arguments: &Value) -> Vec<String> {
    KEYS.iter()
        .filter_map(|k| arguments.get(*k).and_then(Value::as_str))
        .chain(
            arguments
                .get("armies")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .filter_map(Value::as_str),
        )
        .map(str::to_string)
        .collect()
}

/// `arguments` with each role or loose army name replaced by the army it stands for, and the
/// (asked, army) pairs that were replaced.
pub(super) fn rewrite(
    arguments: &Value,
    names: &[String],
    roles: &BTreeMap<String, String>,
) -> (Value, Vec<(String, String)>) {
    let mut out = arguments.clone();
    let mut done = Vec::new();
    let mut swap = |v: &mut Value| {
        if let Some(asked) = v.as_str() {
            if let Some(army) = resolve(asked, names, roles) {
                done.push((asked.to_string(), army.to_string()));
                *v = json!(army);
            }
        }
    };
    for k in KEYS {
        if let Some(v) = out.get_mut(k) {
            swap(v);
        }
    }
    if let Some(list) = out.get_mut("armies").and_then(Value::as_array_mut) {
        for v in list {
            swap(v);
        }
    }
    (out, done)
}

/// Role bindings (`name_hint` -> army) in a plan_state.json body.
pub(super) fn bound(saved: &Value) -> BTreeMap<String, String> {
    saved["layout"]
        .as_object()
        .into_iter()
        .flatten()
        .filter_map(|(hint, army)| {
            Some((
                hint.clone(),
                army.as_str().filter(|a| !a.is_empty())?.to_string(),
            ))
        })
        .collect()
}

/// Role names a plan gives (`army_layout[].name_hint`).
pub(super) fn planned(plan: &Value) -> Vec<String> {
    plan["army_layout"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|e| e["name_hint"].as_str().filter(|h| !h.is_empty()))
        .map(str::to_string)
        .collect()
}

/// "North_strike = Army 1, South_army (not bound to an army yet)" for refusals.
pub(super) fn describe(roles: &BTreeMap<String, String>, planned: &[String]) -> String {
    let mut parts: Vec<String> = roles
        .iter()
        .map(|(hint, army)| format!("{hint} = {army}"))
        .collect();
    parts.extend(
        planned
            .iter()
            .filter(|h| !roles.contains_key(*h))
            .map(|h| format!("{h} (not bound to an army yet)")),
    );
    parts.join(", ")
}

#[cfg(test)]
mod tests {
    use super::*;
    fn names() -> Vec<String> {
        ["Army 1", "Army 2", "Army 6", "Italian Theater 1"]
            .iter()
            .map(|s| s.to_string())
            .collect()
    }
    fn roles() -> BTreeMap<String, String> {
        [
            ("North_strike", "Army 1"),
            ("South_army", "Army 6"),
            ("Gone_guard", "Army 9"),
        ]
        .into_iter()
        .map(|(a, b)| (a.to_string(), b.to_string()))
        .collect()
    }
    #[test]
    fn a_plan_role_becomes_the_army_bound_to_it_and_the_reply_says_so() {
        let args = json!({"army": "North_strike", "state_id": 550, "request_id": "x"});
        let (out, done) = rewrite(&args, &names(), &roles());
        assert_eq!(out["army"], json!("Army 1"));
        assert_eq!(out["state_id"], json!(550));
        assert_eq!(done, vec![("North_strike".into(), "Army 1".into())]);
        // spelled the way a brief reads it
        let (out, _) = rewrite(&json!({"army": "north strike"}), &names(), &roles());
        assert_eq!(out["army"], json!("Army 1"));
    }
    #[test]
    fn an_exact_army_name_is_never_rewritten() {
        let args = json!({"source": "Army 2", "target": "Army 6"});
        let (out, done) = rewrite(&args, &names(), &roles());
        assert_eq!(out, args);
        assert!(done.is_empty());
    }
    #[test]
    fn a_loose_army_name_is_taken_when_only_one_army_matches() {
        let (out, done) = rewrite(&json!({"army": "army 2"}), &names(), &roles());
        assert_eq!(out["army"], json!("Army 2"));
        assert_eq!(done.len(), 1);
    }
    #[test]
    fn group_lists_and_merge_pairs_are_resolved_too() {
        let args = json!({"armies": ["North_strike", "South_army", "Army 2"]});
        let (out, done) = rewrite(&args, &names(), &roles());
        assert_eq!(out["armies"], json!(["Army 1", "Army 6", "Army 2"]));
        assert_eq!(done.len(), 2);
        let (out, _) = rewrite(
            &json!({"source": "South_army", "target": "North_strike"}),
            &names(),
            &roles(),
        );
        assert_eq!(
            (&out["source"], &out["target"]),
            (&json!("Army 6"), &json!("Army 1"))
        );
    }
    #[test]
    fn unknown_or_vanished_roles_stay_as_asked_for_the_army_unknown_refusal() {
        // a role whose army no longer exists, and a name nobody knows
        for asked in ["Gone_guard", "Jubaland_guard"] {
            let (out, done) = rewrite(&json!({"army": asked}), &names(), &roles());
            assert_eq!(out["army"], json!(asked));
            assert!(done.is_empty());
        }
    }
    #[test]
    fn a_renamed_army_keeps_answering_to_its_role() {
        // the layout step renamed Army 1 to its role and rebound the role
        let names: Vec<String> = vec!["North strike".into(), "Army 6".into()];
        let roles: BTreeMap<String, String> =
            [("North_strike".to_string(), "North strike".to_string())].into();
        let (out, _) = rewrite(&json!({"army": "North_strike"}), &names, &roles);
        assert_eq!(out["army"], json!("North strike"));
    }
    #[test]
    fn bindings_and_role_names_are_read_from_the_plan_files() {
        let saved = json!({"layout": {"North_strike": "Army 1", "Empty": ""}});
        let roles = bound(&saved);
        assert_eq!(roles.len(), 1);
        let plan = json!({"army_layout": [{"name_hint": "North_strike"}, {"name_hint": "Jubaland_guard"}]});
        let planned = planned(&plan);
        assert_eq!(
            describe(&roles, &planned),
            "North_strike = Army 1, Jubaland_guard (not bound to an army yet)"
        );
        assert!(addressed("hoi4_army_front") && addressed("hoi4_supply"));
        assert!(!addressed("hoi4_army_create") && !addressed("hoi4_train"));
        assert_eq!(
            asked(&json!({"army": "A", "armies": ["B"]})),
            vec!["A", "B"]
        );
    }
}
