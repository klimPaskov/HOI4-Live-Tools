//! One order for each experience pool: the plan's `xp_budget.<army|air|navy>` lists every spend
//! of that pool in the order the plan wants it paid (template edits, designs, officer corps
//! spirits, doctrines). The steps that spend experience live in different domains (templates and
//! officer corps in management, designs in affairs, doctrines in the extras), and each used to
//! spend whatever it found: the doctrines took the army experience the line-artillery template
//! and the officer corps were planned on (Bed 1, 18 Nov 2026: no template edit ever reached the
//! game). Now every spender asks [`ahead`] first: the experience the entries listed before it
//! still need is held back for them, as long as such an entry is DUE (its `when` holds once its
//! own experience clause is left out), not yet done, and not resting after a refusal (a spend
//! the game refuses never blocks the rest). A spend the budget does not list comes after every
//! listed one.
use super::{item_id, list, text, GameHour, State, Tools, Truth};
use serde_json::Value;

/// What spends experience, as the budget's `ref` names it.
pub(super) enum Spender<'a> {
    /// `template:<id>`, `design:<id>`, `doctrine:<id>`.
    Ref(String),
    /// An `officer_corps` entry (the budget names it `spirit:<key>`).
    Spirit(&'a Value),
    /// Something the budget does not list (the doctrine fill): every listed spend comes first.
    Unlisted,
}

/// The predicate without its experience clauses: `xp:...` atoms hold. The budget's `when`
/// carries the spend's own price ("xp:army >= 40"); whether it is due is everything else.
fn without_xp(when: &Value) -> Value {
    match when {
        Value::String(s) if s.trim_start().starts_with("xp:") => Value::Null,
        Value::Object(map) => {
            let mut out = serde_json::Map::new();
            for (k, v) in map {
                match (k.as_str(), v) {
                    ("all", Value::Array(items)) => {
                        let kept: Vec<Value> = items
                            .iter()
                            .map(without_xp)
                            .filter(|i| !i.is_null())
                            .collect();
                        if !kept.is_empty() {
                            out.insert(k.clone(), Value::Array(kept));
                        }
                    }
                    ("any", Value::Array(items)) => {
                        let kept: Vec<Value> = items.iter().map(without_xp).collect();
                        if kept.iter().any(Value::is_null) {
                            // one alternative is only the price: due
                            continue;
                        }
                        out.insert(k.clone(), Value::Array(kept));
                    }
                    _ => {
                        out.insert(k.clone(), v.clone());
                    }
                }
            }
            if out.is_empty() {
                Value::Null
            } else {
                Value::Object(out)
            }
        }
        other => other.clone(),
    }
}

fn norm(s: &str) -> String {
    s.trim()
        .to_lowercase()
        .replace([' ', '-'], "_")
        .trim_end_matches("_spirit")
        .to_string()
}

/// Whether the budget entry `budget` names the officer corps entry `oc`: by key or name, else by
/// the same price and the same `when`.
fn spirit_matches(budget: &Value, oc: &Value) -> bool {
    if text(&budget["purpose"]) != "officer_corps_spirit"
        && !text(&budget["ref"]).starts_with("spirit:")
    {
        return false;
    }
    let want = text(&budget["ref"]).strip_prefix("spirit:").map(norm);
    if let Some(want) = want.as_deref().filter(|w| !w.is_empty()) {
        for field in ["id", "key", "spirit"] {
            if oc[field].as_str().is_some_and(|v| norm(v) == want) {
                return true;
            }
        }
    }
    budget["cost"].as_f64().is_some()
        && budget["cost"].as_f64() == oc["cost"].as_f64()
        && budget["when"] == oc["when"]
}

/// The officer corps entry's done-memory key (as `officer_corps_step` writes it).
pub(super) fn spirit_key(oc: &Value) -> Option<String> {
    Some(format!(
        "ofc:{}:{}",
        oc["slot"].as_str()?,
        oc["spirit"].as_str()?
    ))
}

/// Whether a budget entry still waits to be spent: not done, not resting after a refusal.
fn pending(state: &State, plan: &Value, entry: &Value, now: GameHour) -> bool {
    let reference = text(&entry["ref"]);
    let (kind, id) = reference.split_once(':').unwrap_or(("", ""));
    match (kind, text(&entry["purpose"])) {
        ("template", _) => {
            let key = format!("tpl:id:{id}");
            !state.done.contains_key(&key) && !state.skipped(now, "tpl", &key)
        }
        ("design", _) => {
            let key = format!("design:id:{id}");
            !state.done.contains_key(&key) && !state.skipped(now, "des", &key)
        }
        ("doctrine", _) => {
            let key = format!("doc:id:{id}");
            !state.done.contains_key(&key) && !state.skipped(now, "doc", &key)
        }
        ("spirit", _) | (_, "officer_corps_spirit") => list(plan, "officer_corps")
            .iter()
            .find(|oc| spirit_matches(entry, oc))
            .and_then(spirit_key)
            .is_some_and(|key| {
                !state.done.contains_key(&key)
                    && !state.skipped(now, "ofc", &key)
                    && super::plan_manage::ofc_refused(state, &key) < 3
            }),
        // exercises and anything no step spends hold nothing back
        _ => false,
    }
}

/// The experience of `pool` (army, air, navy) the spends listed before `who` still need, and
/// their ids: a spender waits until the pool holds that much plus its own price.
pub(super) fn ahead(
    state: &mut State,
    tools: &dyn Tools,
    plan: &Value,
    pool: &str,
    who: &Spender<'_>,
    now: GameHour,
) -> (f64, Vec<String>) {
    let entries = list(&plan["xp_budget"], pool);
    let position = entries
        .iter()
        .position(|e| match who {
            Spender::Ref(r) => text(&e["ref"]) == r.as_str(),
            Spender::Spirit(oc) => spirit_matches(e, oc),
            Spender::Unlisted => false,
        })
        .unwrap_or(entries.len());
    let mut held = 0.0;
    let mut names = Vec::new();
    for entry in &entries[..position] {
        let cost = entry["cost"].as_f64().unwrap_or(0.0);
        if cost <= 0.0 || !pending(state, plan, entry, now) {
            continue;
        }
        let due = match without_xp(&entry["when"]) {
            Value::Null => Truth::True,
            Value::String(s) if s.trim().is_empty() => Truth::True,
            when => super::judge(state, tools, &when, now),
        };
        if due != Truth::True {
            continue;
        }
        held += cost;
        names.push(item_id(entry).unwrap_or_else(|| text(&entry["ref"]).to_string()));
    }
    (held, names)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn the_price_clause_is_left_out_of_the_due_check() {
        assert_eq!(without_xp(&json!("xp:army >= 40")), Value::Null);
        assert_eq!(
            without_xp(&json!({"all": ["date >= 1938.6.1", "xp:army >= 35"]})),
            json!({"all": ["date >= 1938.6.1"]})
        );
        assert_eq!(
            without_xp(&json!({"any": ["xp:army >= 35", "date >= 1938.6.1"]})),
            Value::Null
        );
        assert_eq!(
            without_xp(&json!("tech_researched:x")),
            json!("tech_researched:x")
        );
    }

    #[test]
    fn a_budget_spirit_finds_its_officer_corps_entry() {
        let budget = json!({"purpose": "officer_corps_spirit", "ref": "spirit:mobile_warfare_army_spirit", "cost": 35, "when": {"all": ["date >= 1938.6.1"]}});
        let same_when = json!({"slot": "army:1", "spirit": "Motorization Drive", "cost": 35, "when": {"all": ["date >= 1938.6.1"]}});
        let other = json!({"slot": "army:0", "spirit": "Professional Officer Corps", "cost": 20});
        assert!(spirit_matches(&budget, &same_when));
        assert!(!spirit_matches(&budget, &other));
        let named = json!({"purpose": "officer_corps_spirit", "ref": "spirit:professional_officer_corps_spirit", "cost": 20});
        assert!(spirit_matches(&named, &other));
    }
}
