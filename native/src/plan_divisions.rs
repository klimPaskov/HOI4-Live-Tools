//! Division management: which fielded formations are worth keeping, which should be converted
//! to a better layout in place, and which disbanded so their equipment and manpower refill the
//! divisions that fight (owner, 23 Nov 2026: "weak or unnecessary formations ... disbanding and
//! reallocating their equipment and manpower to better units. However, the agent should
//! evaluate whether a division is actually useful before disbanding it").
//!
//! The judgement reads `hoi4_template_get` (a pure memory read: width, composition, equipment
//! need and the summed equipment stats of every own template with its division count) and
//! compares each fielded template with the plan's own templates (`training[].template`,
//! `templates[].name`; with none of them fielded, the template most divisions use). A template is
//! weak when
//!   - it is small: less than half the width of its reference. Each division takes a place in
//!     the combat and the stacking limit and a slot in an army, and splits what a battle can
//!     put into the width, so two small ones fight worse than one full one (game fact:
//!     COMBAT_STACKING_* and the combat width rules); or
//!   - it uses the width worse in both ways that matter: less defence AND less attack per width
//!     than its reference (the equipment-stat estimate of hoi4_template_get, validated:false).
//!
//! A weak template whose combat battalion kinds the reference also has is CONVERTED in place
//! (`hoi4_template_edit allow_in_use`): its divisions keep their men, the battalions the two
//! share and most of their experience (land.md A16: experience is lost per changed battalion,
//! nothing for an unchanged one), and the game reorganises them at the new layout. Any other
//! weak template is DISBANDED through the `disband` rule (own land, never overseas or in a
//! pocket, bounded by `disband.max`): its equipment and manpower go back to the pool the
//! fighting divisions draw from (L11: home divisions disbanded took the front from 30% to 100%
//! equipment). A template the plan names in `formations.keep` (a role the numbers do not show:
//! mountain, marine, garrison duty, a coming landing) is never judged; one the plan upgrades
//! itself (`templates[].base`) is left to that entry.
//!
//! Plan key (OFF unless set, never run in the game yet):
//!   formations  {"act": true, "keep": ["Gebirgs-Division", ..], "when": predicate}
//! Without `act` the judgement still feeds the omissions row `weak_formation_fielded`.
use super::{list, strings, text, GameHour, State, Tools, DAY};
use crate::error::Result;
use serde_json::{json, Value};
use std::collections::BTreeSet;

#[derive(Clone, Debug, PartialEq)]
pub(super) enum Action {
    Convert,
    Disband,
}

#[derive(Clone, Debug, PartialEq)]
pub(super) struct Verdict {
    pub template: String,
    pub divisions: u64,
    pub action: Action,
    pub reference: String,
    pub why: String,
}

/// Battalion kinds that make a template mobile; any armour kind makes it armour.
const MOBILE: &[&str] = &[
    "motorized",
    "mechanized",
    "amphibious_mechanized",
    "cavalry",
    "camelry",
    "motorcycle",
];

fn family(row: &Value) -> &'static str {
    let kinds = kinds(row);
    if kinds.iter().any(|k| k.contains("armor")) {
        "armour"
    } else if kinds.iter().any(|k| MOBILE.contains(&k.as_str())) {
        "mobile"
    } else {
        "foot"
    }
}

fn kinds(row: &Value) -> BTreeSet<String> {
    row["composition"]
        .as_object()
        .map(|m| m.keys().cloned().collect())
        .unwrap_or_default()
}

fn width(row: &Value) -> f64 {
    row["totals"]["width"].as_f64().unwrap_or(0.0)
}

/// (defence, attack) per width from the equipment-stat estimate; None without one.
fn per_width(row: &Value) -> Option<(f64, f64)> {
    let w = width(row);
    let est = &row["equipment_stats_estimate"];
    if w <= 0.0 || !est.is_object() {
        return None;
    }
    let get = |k: &str| est[k].as_f64().unwrap_or(0.0);
    Some((
        get("defense") / w,
        (get("soft_attack") + get("hard_attack")) / w,
    ))
}

fn used(row: &Value) -> u64 {
    row["divisions_using_template"].as_u64().unwrap_or(0)
}

/// The plan's own template names: what it trains and the templates it designs.
pub(super) fn plan_templates(plan: &Value) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    for t in list(plan, "training") {
        if let Some(n) = t["template"].as_str() {
            out.insert(n.to_string());
        }
    }
    for t in list(plan, "templates") {
        if let Some(n) = t["name"].as_str() {
            out.insert(n.to_string());
        }
    }
    out
}

/// Templates the plan keeps whatever the numbers say, and those a `templates[]` entry upgrades.
fn protected(plan: &Value) -> BTreeSet<String> {
    let mut out: BTreeSet<String> = strings(&plan["formations"], "keep").into_iter().collect();
    for t in list(plan, "templates") {
        if let Some(n) = t["base"].as_str() {
            out.insert(n.to_string());
        }
    }
    out
}

/// The verdicts on a `hoi4_template_get` read; pure. Empty when nothing to compare with.
pub(super) fn weak_templates(read: &Value, plan: &Value) -> Vec<Verdict> {
    let rows = list(read, "templates");
    let named = plan_templates(plan);
    let keep = protected(plan);
    let mut refs: Vec<&Value> = rows
        .iter()
        .filter(|r| named.contains(text(&r["template"])) && width(r) > 0.0)
        .collect();
    if refs.is_empty() {
        // no plan template exists yet: the backbone (most divisions) is the reference
        if let Some(r) = rows
            .iter()
            .filter(|r| width(r) > 0.0)
            .max_by_key(|r| used(r))
        {
            refs.push(r);
        }
    }
    if refs.is_empty() {
        return Vec::new();
    }
    let score = |r: &Value| per_width(r).map_or(0.0, |(d, a)| d + a);
    let mut out = Vec::new();
    for row in rows {
        let name = text(&row["template"]);
        if used(row) == 0
            || keep.contains(name)
            || refs.iter().any(|r| text(&r["template"]) == name)
            || width(row) <= 0.0
        {
            continue;
        }
        let fam = family(row);
        let reference = refs
            .iter()
            .filter(|r| family(r) == fam)
            .max_by(|a, b| score(a).total_cmp(&score(b)))
            .or_else(|| refs.iter().max_by(|a, b| score(a).total_cmp(&score(b))))
            .copied()
            .unwrap_or(refs[0]);
        let rname = text(&reference["template"]);
        let (w, rw) = (width(row), width(reference));
        let mut why = Vec::new();
        if w * 2.0 < rw {
            why.push(format!(
                "small: width {w:.0} against {rname}'s {rw:.0} (each division takes a place in \
                 combat, in the stacking limit and in an army)"
            ));
        }
        if let (Some((d, a)), Some((rd, ra))) = (per_width(row), per_width(reference)) {
            if d < rd && a < ra {
                why.push(format!(
                    "uses its width worse: defence {d:.1} and attack {a:.1} per width against \
                     {rd:.1} and {ra:.1} (estimate)"
                ));
            }
        }
        if why.is_empty() {
            continue;
        }
        let action = if family(reference) == fam && kinds(row).is_subset(&kinds(reference)) {
            Action::Convert
        } else {
            Action::Disband
        };
        out.push(Verdict {
            template: name.to_string(),
            divisions: used(row),
            action,
            reference: rname.to_string(),
            why: why.join("; "),
        });
    }
    out
}

/// One line per verdict for notes and the omissions row.
pub(super) fn describe(v: &Verdict) -> String {
    match v.action {
        Action::Convert => format!(
            "{} ({} div): convert in place to {}'s layout (hoi4_template_edit allow_in_use): {}",
            v.template, v.divisions, v.reference, v.why
        ),
        Action::Disband => format!(
            "{} ({} div): disband for equipment and manpower (hoi4_division_disband): {}",
            v.template, v.divisions, v.why
        ),
    }
}

/// The columns and support of a template row as `hoi4_template_edit` takes them.
fn layout(row: &Value) -> (Vec<Vec<String>>, Vec<String>) {
    let columns = list(row, "columns")
        .iter()
        .map(|c| {
            c.as_array()
                .into_iter()
                .flatten()
                .filter_map(Value::as_str)
                .map(String::from)
                .collect::<Vec<_>>()
        })
        .filter(|c| !c.is_empty())
        .collect();
    let support = strings(row, "support");
    (columns, support)
}

/// Tries before a conversion the game keeps refusing (experience short, designer refusal) is
/// left; the rule's note keeps naming it, so the player or a plan revision (`formations.keep`,
/// a `templates[]` entry) decides instead.
const CONVERT_TRIES: u64 = 3;

/// `formations.act`: one conversion per tick, then the disband rule with the weak templates
/// added to `disband.templates`. Reads once a game week.
pub(super) fn formations_step(
    state: &mut State,
    tools: &dyn Tools,
    plan: &Value,
    lease: &str,
    now: GameHour,
) -> Result<Option<String>> {
    let cfg = &plan["formations"];
    if cfg["act"] != json!(true) {
        super::plan_manage::note(
            state,
            "formations",
            "formations.act is off: weak templates are named, never converted or disbanded",
        );
        return Ok(None);
    }
    if state.skipped(now, "formations", "read") {
        return Ok(None);
    }
    if !cfg["when"].is_null() && !super::judge(state, tools, &cfg["when"], now).is_true() {
        super::plan_manage::note(
            state,
            "formations",
            format!("waits for formations.when ({})", cfg["when"]),
        );
        state.skip(now, "formations", "read", DAY);
        return Ok(None);
    }
    let read = tools.call("hoi4_template_get", &json!({}))?;
    let verdicts = weak_templates(&read, plan);
    if verdicts.is_empty() {
        state.skip(now, "formations", "read", 7 * DAY);
        super::plan_manage::note(state, "formations", "every fielded template is kept");
        return Ok(None);
    }
    super::plan_manage::note(
        state,
        "formations",
        verdicts.iter().map(describe).collect::<Vec<_>>().join("; "),
    );
    for v in verdicts.iter().filter(|v| v.action == Action::Convert) {
        let key = format!("formconv:{}:", v.template);
        let tries = state.done.keys().filter(|k| k.starts_with(&key)).count() as u64;
        if tries >= CONVERT_TRIES || state.skipped(now, "formconv", &v.template) {
            continue;
        }
        let Some(reference) = list(&read, "templates")
            .iter()
            .find(|r| text(&r["template"]) == v.reference)
        else {
            continue;
        };
        let (regiments, support) = layout(reference);
        let (ok, why) = state.act(
            tools,
            lease,
            "hoi4_template_edit",
            json!({"template": v.template, "regiments": regiments, "support": support,
                "allow_in_use": true}),
            "fc",
        )?;
        if !ok {
            state.done.insert(format!("{key}{tries}"), now);
            state.skip(now, "formconv", &v.template, 14 * DAY);
        } else {
            state.skip(now, "formations", "read", DAY);
        }
        return Ok(Some(format!(
            "formation {} converted to {}'s layout: {ok} {why}",
            v.template, v.reference
        )));
    }
    let gone: Vec<String> = verdicts
        .iter()
        .filter(|v| v.action == Action::Disband)
        .map(|v| v.template.clone())
        .collect();
    if gone.is_empty() {
        state.skip(now, "formations", "read", 7 * DAY);
        return Ok(None);
    }
    let mut with = plan.clone();
    if !with["disband"].is_object() {
        with["disband"] = json!({});
    }
    let mut templates = strings(&with["disband"], "templates");
    for t in gone {
        if !templates.contains(&t) {
            templates.push(t);
        }
    }
    with["disband"]["templates"] = json!(templates);
    let out = super::disband_step(state, tools, &with, lease, now)?;
    if out.is_none() {
        state.skip(now, "formations", "read", 7 * DAY);
        // the verdicts with the disband rule's own reason for leaving them (t1939n, 27 Nov 2026:
        // the row repeated "disband" for 150 days and never said why nothing was disbanded)
        let why = super::plan_manage::note_of(state, "disband")
            .unwrap_or("the disband rule wrote no reason")
            .to_string();
        let verdicts = verdicts.iter().map(describe).collect::<Vec<_>>().join("; ");
        super::plan_manage::note(state, "formations", format!("{verdicts}; disband: {why}"));
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(name: &str, n: u64, comp: &[(&str, u64)], w: f64, def: f64, atk: f64) -> Value {
        let composition: serde_json::Map<String, Value> = comp
            .iter()
            .map(|(k, v)| (k.to_string(), json!(v)))
            .collect();
        let columns: Vec<Vec<&str>> = comp
            .iter()
            .flat_map(|(k, v)| std::iter::repeat_n(*k, *v as usize))
            .collect::<Vec<_>>()
            .chunks(3)
            .map(<[&str]>::to_vec)
            .collect();
        json!({"template": name, "divisions_using_template": n, "composition": composition,
            "columns": columns, "support": ["engineer", "artillery"],
            "totals": {"width": w},
            "equipment_stats_estimate": {"defense": def, "soft_attack": atk, "hard_attack": 0.0}})
    }

    fn germany() -> Value {
        json!({"templates": [
            row("Infanterie-Division", 92, &[("infantry", 9)], 18.0, 360.0, 90.0),
            row("Panzer-Division (M)", 19, &[("medium_armor", 3), ("motorized", 6)], 24.0, 300.0, 300.0),
            row("Panzer-Division", 5, &[("light_armor", 4), ("motorized", 2)], 14.0, 150.0, 120.0),
            row("Sicherungs-Division", 10, &[("infantry", 9)], 18.0, 300.0, 70.0),
            row("Kavallerie-Brigade", 1, &[("cavalry", 4)], 8.0, 160.0, 40.0),
            row("Gebirgs-Brigade", 1, &[("mountaineers", 6)], 12.0, 250.0, 60.0),
            row("Spare", 0, &[("infantry", 2)], 4.0, 1.0, 1.0),
        ]})
    }

    fn plan() -> Value {
        json!({"training": [{"template": "Infanterie-Division"}, {"template": "Panzer-Division (M)"}],
            "formations": {"keep": ["Gebirgs-Brigade"]}})
    }

    #[test]
    fn weak_templates_are_converted_when_the_reference_shares_their_battalions_else_disbanded() {
        let v = weak_templates(&germany(), &plan());
        let by = |n: &str| v.iter().find(|x| x.template == n).cloned();
        // same 9 infantry battalions, less per width: converted in place
        let s = by("Sicherungs-Division").unwrap();
        assert_eq!(s.action, Action::Convert);
        assert_eq!(s.reference, "Infanterie-Division");
        // light tanks are not in the medium panzer layout: disbanded for their equipment
        let p = by("Panzer-Division").unwrap();
        assert_eq!(p.action, Action::Disband);
        assert_eq!(p.reference, "Panzer-Division (M)");
        // a brigade under half the reference width
        let k = by("Kavallerie-Brigade").unwrap();
        assert!(k.why.starts_with("small"), "{}", k.why);
        // kept by the plan, unused, and the references themselves are never judged
        assert!(by("Gebirgs-Brigade").is_none());
        assert!(by("Spare").is_none());
        assert!(by("Infanterie-Division").is_none() && by("Panzer-Division (M)").is_none());
    }

    #[test]
    fn a_template_as_good_per_width_is_kept_and_no_plan_template_means_the_backbone_is_the_reference(
    ) {
        let read = json!({"templates": [
            row("Line", 40, &[("infantry", 9)], 18.0, 360.0, 90.0),
            row("Austrian", 7, &[("infantry", 9)], 18.0, 380.0, 80.0),
        ]});
        // more defence, less attack per width: not worse in both, kept
        assert!(weak_templates(&read, &json!({})).is_empty());
        let weak = json!({"templates": [
            row("Line", 40, &[("infantry", 9)], 18.0, 360.0, 90.0),
            row("Austrian", 7, &[("infantry", 6)], 12.0, 200.0, 50.0),
        ]});
        let v = weak_templates(&weak, &json!({}));
        assert_eq!(v.len(), 1);
        assert_eq!(
            (v[0].action.clone(), v[0].reference.as_str()),
            (Action::Convert, "Line")
        );
    }

    #[test]
    fn a_template_the_plan_upgrades_itself_is_left_to_that_entry() {
        let mut p = plan();
        p["templates"] = json!([{"base": "Sicherungs-Division", "name": "Infanterie-Division"}]);
        assert!(weak_templates(&germany(), &p)
            .iter()
            .all(|v| v.template != "Sicherungs-Division"));
    }

    #[test]
    fn the_layout_sent_is_the_reference_columns_and_support() {
        let r = row("Line", 1, &[("infantry", 4)], 8.0, 1.0, 1.0);
        let (cols, support) = layout(&r);
        assert_eq!(
            cols,
            vec![
                vec!["infantry".to_string(); 3],
                vec!["infantry".to_string()]
            ]
        );
        assert_eq!(support, vec!["engineer", "artillery"]);
    }
}
