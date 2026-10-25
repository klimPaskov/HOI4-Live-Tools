//! Pure settlement rules for unresolved Mac actions.
//!
//! Rule: compare only what the action can change. The game keeps running at
//! speed 3-5 while an action is journaled, so everything else drifts: other
//! research slots finish, other production lines gain efficiency, armies move
//! and lose strength, other queue lines progress, the date advances. A rule
//! that requires such state to stay equal returns `Unknown` forever, and an
//! unresolved `Dispatching` intent freezes all shared Mac input.
//!
//! Therefore each rule names the exact fields its action can write and
//! reads nothing else:
//! * `Reached` only when the exact target effect is present;
//! * `Unchanged` only when every effect the action could have is provably
//!   absent in those fields (drift in unrelated fields is ignored);
//! * `Diverged` when the compared fields show a settled outcome that is
//!   neither (recorded, never retried, does not freeze input);
//! * `Unknown` when a partial or foreign effect makes the fields ambiguous.
//!
//! Nothing here sends input, extends a deadline or replaces journal ownership.
use super::journal::Outcome;
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug, PartialEq)]
pub(super) enum Verdict {
    Reached,
    Unchanged,
    Diverged,
    Unknown,
}
impl Verdict {
    pub(super) fn outcome(self, detail: Value) -> Outcome {
        match self {
            Self::Reached => Outcome::Reached(detail),
            Self::Unchanged => Outcome::Unchanged(detail),
            Self::Diverged => Outcome::Diverged(detail),
            Self::Unknown => Outcome::Unknown,
        }
    }
}

/// A queue that should grow by `want` (training lines of one template,
/// construction of one building in one state). Lines of the same kind may
/// finish on their own, so a fall below `start` is ambiguous: recorded as
/// `Diverged`, never as `Reached` or `Unchanged`.
pub(super) fn queue_growth(start: i64, now: i64, want: i64) -> Verdict {
    if now >= start + want {
        Verdict::Reached
    } else if now == start {
        Verdict::Unchanged
    } else {
        Verdict::Diverged
    }
}

/// Battle-plan orders of one army. Only an order drawn by this action raises
/// the count; executing plans consume orders on their own, so a fall is
/// `Diverged` (the action may or may not have added one).
pub(super) fn order_growth(before: usize, now: usize) -> Verdict {
    if now > before {
        Verdict::Reached
    } else if now == before {
        Verdict::Unchanged
    } else {
        Verdict::Diverged
    }
}

/// Training lines without a deployment location. Zero is the target.
pub(super) fn locations(start: usize, now: usize) -> Verdict {
    if now == 0 {
        Verdict::Reached
    } else if now < start {
        Verdict::Diverged
    } else {
        Verdict::Unchanged
    }
}

/// Own decision. `verified` is the dispatch's own proof (political power spent,
/// timer started or entry gone, observed in the same attempt). Political
/// power only rises on its own, so it cannot show "not taken" and is not
/// compared for Unchanged beyond the fall that proves a spend.
pub(super) fn decision(verified: bool, timer: bool, pp_before: i64, pp_now: i64) -> Verdict {
    if verified || timer || pp_now < pp_before {
        Verdict::Reached
    } else {
        Verdict::Unchanged
    }
}

/// National focus: only the focus slot and the completed set are compared.
/// A focus that completed inside the window was started by this action
/// (the pre-action check refused completed targets).
pub(super) fn focus(active: Option<&str>, key_completed: bool, key: &str) -> Verdict {
    match active {
        Some(a) if a == key => Verdict::Reached,
        Some(_) => Verdict::Unknown,
        None if key_completed => Verdict::Reached,
        None => Verdict::Unchanged,
    }
}

/// Army identities taken from a `hoi4_armies`-shaped view. Movement, strength,
/// states, commanders and plans are deliberately not captured: they drift.
#[derive(Debug, Default)]
pub(super) struct Roster {
    names: BTreeSet<String>,
    /// Division id -> army name (None = unassigned).
    home: BTreeMap<String, Option<String>>,
    /// Army name -> army group name.
    parent: BTreeMap<String, Option<String>>,
    groups: BTreeSet<String>,
}
fn text(v: &Value) -> String {
    v.as_str().map_or_else(|| v.to_string(), str::to_string)
}
impl Roster {
    pub(super) fn from_view(view: &Value) -> Self {
        let mut out = Self::default();
        for army in view["armies"].as_array().into_iter().flatten() {
            let name = text(&army["army"]);
            for d in army["divisions"].as_array().into_iter().flatten() {
                out.home.insert(text(d), Some(name.clone()));
            }
            out.parent.insert(
                name.clone(),
                army["army_group"].as_str().map(str::to_string),
            );
            out.names.insert(name);
        }
        for d in view["unassigned"].as_array().into_iter().flatten() {
            out.home.insert(text(d), None);
        }
        for g in view["army_groups"].as_array().into_iter().flatten() {
            out.groups.insert(text(&g["army_group"]));
        }
        out
    }
    fn new_names<'a>(&'a self, before: &'a Roster) -> Vec<&'a String> {
        self.names.difference(&before.names).collect()
    }
    /// A division known to both snapshots now belongs to another army.
    fn moved(&self, before: &Roster) -> bool {
        self.home
            .iter()
            .any(|(d, now)| before.home.get(d).is_some_and(|was| was != now))
    }
}

/// Army creation: a new army name appears. Unchanged only when no army was
/// added and no division known to both snapshots changed army. Divisions
/// destroyed, trained or moving do not matter.
pub(super) fn army_create(before: &Roster, now: &Roster) -> Verdict {
    if !now.new_names(before).is_empty() {
        Verdict::Reached
    } else if !now.moved(before) {
        Verdict::Unchanged
    } else {
        Verdict::Unknown
    }
}

/// Adding unassigned divisions to the standing army `army` without a new army:
/// the only changes allowed among divisions known to both snapshots are
/// unassigned -> `army`, drawn from `pool` (the unassigned ids that stood in the
/// state). `Reached` only for exactly `take` such moves; no change at all is
/// `Unchanged`; a new army, a division leaving another army, a foreign id or a
/// partial count is `Unknown` (never guessed).
pub(super) fn army_assign(
    before: &Roster,
    now: &Roster,
    army: &str,
    pool: &[String],
    take: usize,
) -> Verdict {
    if !now.new_names(before).is_empty() || !now.names.contains(army) {
        return Verdict::Unknown;
    }
    let changes: Vec<_> = now
        .home
        .iter()
        .filter_map(|(d, to)| {
            let was = before.home.get(d)?;
            (was != to).then_some((d, was, to))
        })
        .collect();
    if changes.is_empty() {
        return Verdict::Unchanged;
    }
    let only_expected = changes
        .iter()
        .all(|(d, was, to)| was.is_none() && to.as_deref() == Some(army) && pool.contains(*d));
    if only_expected && changes.len() == take {
        Verdict::Reached
    } else {
        Verdict::Unknown
    }
}

/// Splitting `count` divisions off `army`: one new army of exactly `count`
/// divisions, all taken from the source army.
pub(super) fn army_split(before: &Roster, now: &Roster, army: &str, count: usize) -> Verdict {
    let added = now.new_names(before);
    if added.is_empty() {
        return if now.moved(before) {
            Verdict::Unknown
        } else {
            Verdict::Unchanged
        };
    }
    if added.len() == 1 {
        let new = added[0];
        let taken: Vec<_> = now
            .home
            .iter()
            .filter(|(_, h)| h.as_deref() == Some(new.as_str()))
            .collect();
        let from_source = taken.iter().all(|(d, _)| {
            before
                .home
                .get(*d)
                .is_some_and(|h| h.as_deref() == Some(army))
        });
        if taken.len() == count && from_source {
            return Verdict::Reached;
        }
    }
    Verdict::Unknown
}

/// Army group creation over `armies`: all now share one group that did not
/// exist before. Unchanged only when none of them changed group.
pub(super) fn group_create(before: &Roster, now: &Roster, armies: &[String]) -> Verdict {
    let groups: Vec<_> = armies
        .iter()
        .map(|a| now.parent.get(a).cloned().flatten())
        .collect();
    if let Some(Some(g)) = groups.first() {
        if groups.iter().all(|x| x.as_ref() == Some(g)) && !before.groups.contains(g) {
            return Verdict::Reached;
        }
    }
    let unchanged = armies
        .iter()
        .all(|a| now.parent.get(a) == before.parent.get(a))
        && now.groups.is_subset(&before.groups);
    if unchanged {
        Verdict::Unchanged
    } else {
        Verdict::Unknown
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn view(armies: &[(&str, &[&str], Option<&str>)], free: &[&str], groups: &[&str]) -> Roster {
        Roster::from_view(&json!({
            "armies": armies.iter().map(|(n, d, g)| json!({
                "army": n, "divisions": d, "army_group": g,
                "states": {"1": 3}, "main_state": 7, "plan_executing": true})).collect::<Vec<_>>(),
            "unassigned": free,
            "army_groups": groups.iter().map(|g| json!({"army_group": g})).collect::<Vec<_>>(),
        }))
    }

    #[test]
    fn queue_and_order_counts() {
        assert_eq!(queue_growth(2, 5, 3), Verdict::Reached);
        assert_eq!(queue_growth(2, 2, 3), Verdict::Unchanged);
        assert_eq!(queue_growth(2, 3, 3), Verdict::Diverged);
        assert_eq!(queue_growth(2, 1, 3), Verdict::Diverged);
        assert_eq!(order_growth(2, 3), Verdict::Reached);
        assert_eq!(order_growth(2, 2), Verdict::Unchanged);
        assert_eq!(order_growth(2, 1), Verdict::Diverged);
        assert_eq!(locations(3, 0), Verdict::Reached);
        assert_eq!(locations(3, 3), Verdict::Unchanged);
        assert_eq!(locations(3, 2), Verdict::Diverged);
        assert_eq!(locations(3, 4), Verdict::Unchanged);
    }

    #[test]
    fn assign_compares_only_the_moved_unassigned_divisions() {
        let before = view(
            &[("A", &["d1", "d2"], None), ("B", &["d3"], None)],
            &["u1", "u2", "u3"],
            &[],
        );
        let pool: Vec<String> = ["u1", "u2", "u3"].map(String::from).to_vec();
        // two of three absorbed (the army was nearly full); new divisions are ignored
        let now = view(
            &[("A", &["d1", "d2", "u1", "u2"], None), ("B", &["d3"], None)],
            &["u3", "n1"],
            &[],
        );
        assert_eq!(army_assign(&before, &now, "A", &pool, 2), Verdict::Reached);
        assert_eq!(army_assign(&before, &now, "A", &pool, 3), Verdict::Unknown);
        let same = view(
            &[("A", &["d1", "d2"], None), ("B", &["d3"], None)],
            &["u1", "u2", "u3"],
            &[],
        );
        assert_eq!(army_assign(&before, &same, "A", &pool, 3), Verdict::Unchanged);
        // a division taken out of B, a new army or a vanished target is never Reached
        let stolen = view(
            &[("A", &["d1", "d2", "d3"], None), ("B", &[], None)],
            &["u1", "u2", "u3"],
            &[],
        );
        assert_eq!(army_assign(&before, &stolen, "A", &pool, 1), Verdict::Unknown);
        let fresh = view(
            &[("A", &["d1", "d2"], None), ("B", &["d3"], None), ("C", &["u1"], None)],
            &["u2", "u3"],
            &[],
        );
        assert_eq!(army_assign(&before, &fresh, "A", &pool, 1), Verdict::Unknown);
        let merged = view(&[("B", &["d1", "d2", "d3", "u1"], None)], &["u2", "u3"], &[]);
        assert_eq!(army_assign(&before, &merged, "A", &pool, 1), Verdict::Unknown);
    }

    #[test]
    fn decision_political_power_income_is_drift() {
        // Income during the window raises PP; that alone is not "taken".
        assert_eq!(decision(false, false, 100, 160), Verdict::Unchanged);
        assert_eq!(decision(false, false, 100, 70), Verdict::Reached);
        assert_eq!(decision(false, true, 100, 160), Verdict::Reached);
        // Dispatch proved the take; later income must not turn it into NotApplied.
        assert_eq!(decision(true, false, 100, 160), Verdict::Reached);
    }

    #[test]
    fn focus_compares_only_slot_and_completion() {
        assert_eq!(focus(Some("a"), false, "a"), Verdict::Reached);
        assert_eq!(focus(None, false, "a"), Verdict::Unchanged);
        assert_eq!(focus(None, true, "a"), Verdict::Reached);
        assert_eq!(focus(Some("b"), false, "a"), Verdict::Unknown);
    }

    #[test]
    fn create_ignores_movement_strength_and_new_divisions() {
        let before = view(&[("1st", &["d1", "d2"], None)], &["d3"], &[]);
        // Drift: d2 destroyed, d4 trained, army still the same.
        let drift = view(&[("1st", &["d1"], None)], &["d3", "d4"], &[]);
        assert_eq!(army_create(&before, &drift), Verdict::Unchanged);
        let made = view(
            &[("1st", &["d1", "d2"], None), ("2nd", &["d3"], None)],
            &[],
            &[],
        );
        assert_eq!(army_create(&before, &made), Verdict::Reached);
        // d3 joined the 1st army but no new army: foreign effect.
        let foreign = view(&[("1st", &["d1", "d2", "d3"], None)], &[], &[]);
        assert_eq!(army_create(&before, &foreign), Verdict::Unknown);
    }

    #[test]
    fn split_requires_exact_new_army_from_the_source() {
        let before = view(&[("1st", &["d1", "d2", "d3"], None)], &[], &[]);
        let drift = view(&[("1st", &["d1", "d2"], None)], &[], &[]);
        assert_eq!(army_split(&before, &drift, "1st", 1), Verdict::Unchanged);
        let done = view(
            &[("1st", &["d1", "d2"], None), ("2nd", &["d3"], None)],
            &[],
            &[],
        );
        assert_eq!(army_split(&before, &done, "1st", 1), Verdict::Reached);
        // Wrong size: partial.
        let partial = view(
            &[("1st", &["d1"], None), ("2nd", &["d2", "d3"], None)],
            &[],
            &[],
        );
        assert_eq!(army_split(&before, &partial, "1st", 1), Verdict::Unknown);
        // A new army made of foreign divisions.
        let other = view(
            &[("1st", &["d1", "d2", "d3"], None), ("2nd", &["d9"], None)],
            &[],
            &[],
        );
        assert_eq!(army_split(&before, &other, "1st", 1), Verdict::Unknown);
    }

    #[test]
    fn group_create_needs_one_new_shared_group() {
        let names = vec!["1st".to_string(), "2nd".to_string()];
        let before = view(&[("1st", &["d1"], None), ("2nd", &["d2"], None)], &[], &[]);
        let drift = view(&[("1st", &[], None), ("2nd", &["d2"], None)], &["d1"], &[]);
        assert_eq!(group_create(&before, &drift, &names), Verdict::Unchanged);
        let done = view(
            &[("1st", &["d1"], Some("AG")), ("2nd", &["d2"], Some("AG"))],
            &[],
            &["AG"],
        );
        assert_eq!(group_create(&before, &done, &names), Verdict::Reached);
        let half = view(
            &[("1st", &["d1"], Some("AG")), ("2nd", &["d2"], None)],
            &[],
            &["AG"],
        );
        assert_eq!(group_create(&before, &half, &names), Verdict::Unknown);
    }
}
