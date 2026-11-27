//! Own army commanders: a read-only roster (`hoi4_commanders`) and the
//! input-free pre-check `hoi4_army_commander` runs before its first click.
//!
//! Sources, none of them new memory offsets:
//! * assigned commanders: each own army's / army group's leader (the same
//!   validated `armies()` read, leader object +0x88, name +0x38);
//! * listed commanders: the rows of the army panel's commander list (the
//!   validated grid `hoi4_army_commander` clicks), readable only while that
//!   list is open. A closed list is never trusted to refuse a name.
//!
//! * the country's own army leaders (`CCountryCharacters` +0x70, see
//!   `units::leaders`): character id, skill, attack/defense/planning/logistics
//!   and traits, checked against `common/characters/GER.txt` for Rundstedt,
//!   List, Bock, Manstein and Heinrici (all five stats equal). They are the
//!   character's base values; levels gained in the campaign are not read.
use super::armies;
use super::ui::read;
use super::units;
use crate::{
    error::{Error, Result},
    platform::Process,
};
use serde_json::{json, Value};
use std::collections::BTreeMap;

/// An own army or army group and who leads it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Target {
    pub name: String,
    pub is_group: bool,
    pub commander: Option<String>,
}
/// One row of the commander list. `promote` is true while the row shows its
/// Promote button (a general who is not yet a field marshal).
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Row {
    pub name: String,
    pub promote: bool,
}
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(super) struct Snapshot {
    pub targets: Vec<Target>,
    /// Rows of the commander list while it is open (fresh), else None.
    pub rows: Option<Vec<Row>>,
    /// Rows still present in memory while the list is closed (may be stale).
    pub stale_rows: Option<Vec<Row>>,
    /// Every army leader of the country (None when the read failed).
    pub leaders: Option<Vec<units::Leader>>,
}

fn key(name: &str) -> String {
    name.to_lowercase()
}

/// Armies and army groups out of the `armies()` view.
pub(super) fn targets_from_view(view: &Value) -> Vec<Target> {
    let mut out = Vec::new();
    for g in view["army_groups"].as_array().into_iter().flatten() {
        if let Some(name) = g["army_group"].as_str() {
            out.push(Target {
                name: name.to_string(),
                is_group: true,
                commander: g["field_marshal"].as_str().map(str::to_string),
            });
        }
    }
    for a in view["armies"].as_array().into_iter().flatten() {
        if let Some(name) = a["army"].as_str() {
            out.push(Target {
                name: name.to_string(),
                is_group: false,
                commander: a["commander"].as_str().map(str::to_string),
            });
        }
    }
    out
}

/// Rank a target needs: army groups are led by field marshals, armies by
/// generals (a field marshal may also lead an army).
pub(super) fn required_rank(is_group: bool) -> &'static str {
    if is_group {
        "field_marshal"
    } else {
        "general"
    }
}
/// Whether the panel must promote the listed general first.
pub(super) fn needs_promotion(is_group: bool, promote_shown: bool) -> bool {
    is_group && promote_shown
}

fn levenshtein(a: &str, b: &str) -> usize {
    let b: Vec<char> = b.chars().collect();
    let mut prev: Vec<usize> = (0..=b.len()).collect();
    for (i, ca) in a.chars().enumerate() {
        let mut cur = vec![i + 1];
        for (j, cb) in b.iter().enumerate() {
            let cost = usize::from(ca != *cb);
            cur.push((prev[j] + cost).min(prev[j + 1] + 1).min(cur[j] + 1));
        }
        prev = cur;
    }
    prev[b.len()]
}
/// Up to `n` candidates closest to `wanted`: names containing it (or inside
/// it) or sharing a whole word first, then by edit distance, then alphabetical.
pub(super) fn closest_names(wanted: &str, candidates: &[String], n: usize) -> Vec<String> {
    let w = key(wanted);
    let words: Vec<&str> = w.split_whitespace().collect();
    let mut scored: Vec<(u8, usize, String)> = candidates
        .iter()
        .map(|c| {
            let k = key(c);
            let near = !w.is_empty()
                && (k.contains(&w)
                    || w.contains(&k)
                    || k.split_whitespace().any(|t| words.contains(&t)));
            (u8::from(!near), levenshtein(&w, &k), c.clone())
        })
        .collect();
    scored.sort();
    scored.dedup_by(|a, b| a.2 == b.2);
    scored.into_iter().take(n).map(|(_, _, c)| c).collect()
}

impl Snapshot {
    fn visible_rows(&self) -> Option<&Vec<Row>> {
        self.rows.as_ref()
    }
    /// Targets led by `name` (case-insensitive).
    fn led_by(&self, name: &str) -> Vec<&Target> {
        let k = key(name);
        self.targets
            .iter()
            .filter(|t| t.commander.as_deref().is_some_and(|c| key(c) == k))
            .collect()
    }
    /// Every commander name known right now: leaders of own armies and
    /// groups plus rows of a currently open list.
    fn known_names(&self) -> Vec<String> {
        let mut seen = BTreeMap::new();
        for t in &self.targets {
            if let Some(c) = &t.commander {
                seen.entry(key(c)).or_insert_with(|| c.clone());
            }
        }
        for r in self.visible_rows().into_iter().flatten() {
            seen.entry(key(&r.name)).or_insert_with(|| r.name.clone());
        }
        seen.into_values().collect()
    }
}

fn describe(t: &Target) -> String {
    format!(
        "{} {}",
        if t.is_group { "army group" } else { "army" },
        t.name
    )
}

/// Refusals for `hoi4_army_commander` that need no input. `reassign` allows
/// taking a commander who leads another army or group (the game asks for a
/// confirmation, which the action accepts).
pub(super) fn check_assignment(
    snap: &Snapshot,
    army: &str,
    general: &str,
    reassign: bool,
) -> Result<()> {
    let Some(target) = snap.targets.iter().find(|t| t.name == army) else {
        let names: Vec<String> = snap.targets.iter().map(|t| t.name.clone()).collect();
        let close = closest_names(army, &names, 5);
        let groups: Vec<&str> = snap
            .targets
            .iter()
            .filter(|t| t.is_group)
            .map(|t| t.name.as_str())
            .collect();
        let armies: Vec<&str> = snap
            .targets
            .iter()
            .filter(|t| !t.is_group)
            .map(|t| t.name.as_str())
            .collect();
        return Err(Error::new(
            "army_unknown",
            format!(
                "No own army or army group named \"{army}\". Closest: {}. Valid armies: {}. \
                 Valid army groups: {}. Nothing was clicked.",
                close.join(", "),
                armies.join(", "),
                groups.join(", ")
            ),
        ));
    };
    let leads = snap.led_by(general);
    if leads.iter().any(|t| t.name == target.name) {
        return Ok(());
    }
    let known = snap.known_names();
    let k = key(general);
    let is_known = known.iter().any(|n| key(n) == k);
    // Only an open list is complete enough to refuse a name that leads nothing.
    if !is_known && snap.visible_rows().is_some() {
        let close = closest_names(general, &known, 5);
        return Err(Error::new(
            "unknown_commander",
            format!(
                "No commander named \"{general}\" in the commander list. Closest: {}. \
                 Nothing was clicked. Call hoi4_commanders for the roster.",
                close.join(", ")
            ),
        ));
    }
    if !reassign {
        if let Some(other) = leads.first() {
            return Err(Error::new(
                "commander_unavailable",
                format!(
                    "\"{general}\" already commands {}. Nothing was clicked. Pass reassign: true \
                     to move the commander (the game's confirmation is accepted and {} loses \
                     its {}), or pick another commander from hoi4_commanders.",
                    describe(other),
                    describe(other),
                    if other.is_group {
                        "field marshal"
                    } else {
                        "general"
                    }
                ),
            ));
        }
    }
    Ok(())
}

/// One script trigger block as a plan predicate (plan_predicate.rs): `has_completed_focus`
/// becomes `focus_completed:<key>`, `OR`/`AND` become any/all, `NOT` (a NOR in the game's
/// scripts) becomes not-any, `always` a constant; every other trigger becomes an atom the server
/// cannot read (`unread_trigger:<key>`, always unknown), so a trigger is never guessed true.
fn trigger_when(block: &[crate::script::Entry]) -> Value {
    use crate::script::Val;
    let parts: Vec<Value> = block
        .iter()
        .filter(|e| !e.key.is_empty())
        .map(|e| match (e.key.as_str(), &e.val) {
            ("has_completed_focus", Val::Scalar(k)) => json!(format!("focus_completed:{k}")),
            ("OR", Val::Block(b)) => json!({"any": trigger_list(b)}),
            ("AND", Val::Block(b)) => json!({"all": trigger_list(b)}),
            ("NOT", Val::Block(b)) => json!({"not": {"any": trigger_list(b)}}),
            ("always", Val::Scalar(v)) if v == "yes" => json!(true),
            ("always", Val::Scalar(v)) if v == "no" => json!(false),
            (key, _) => json!(format!("unread_trigger:{key}")),
        })
        .collect();
    match parts.len() {
        1 => parts.into_iter().next().unwrap_or(Value::Null),
        _ => json!({ "all": parts }),
    }
}

fn trigger_list(block: &[crate::script::Entry]) -> Vec<Value> {
    block
        .iter()
        .filter(|e| !e.key.is_empty())
        .map(|e| trigger_when(std::slice::from_ref(e)))
        .collect()
}

/// The game's `visible` trigger of each character's army-leader role, as a plan predicate, by
/// character id (common/characters of the running game, mods included). A leader whose role is
/// not visible stays in the country's leader list, and an assignment by command reads back
/// verified, but the game takes him off the army again: Armee Pommern was given Hoth, Rommel,
/// Manteuffel, Hausser and Dietrich in turn (g92b, 27 Nov 2026), each hidden until a focus. A
/// character with a role that has no `visible` trigger is absent (always visible). Pure.
pub(super) fn visible_whens(entries: &[crate::script::Entry]) -> BTreeMap<String, Value> {
    use crate::script::{Block, Val};
    let mut out = BTreeMap::new();
    for top in entries.iter().filter(|e| e.key == "characters") {
        for character in top.val.block() {
            let roles: Vec<&[crate::script::Entry]> = ["corps_commander", "field_marshal"]
                .iter()
                .filter_map(|r| character.val.block().find_val(r))
                .map(Val::block)
                .collect();
            if roles.is_empty() {
                continue;
            }
            // a role with no trigger makes the leader visible whatever the other says
            if roles.iter().any(|r| r.find_val("visible").is_none()) {
                continue;
            }
            let whens: Vec<Value> = roles
                .iter()
                .filter_map(|r| r.find_val("visible"))
                .map(|v| trigger_when(v.block()))
                .collect();
            let when = if whens.len() == 1 {
                whens.into_iter().next().unwrap_or(Value::Null)
            } else {
                json!({ "any": whens })
            };
            out.insert(character.key.clone(), when);
        }
    }
    out
}

/// [`visible_whens`] of the running game, read once per install.
fn visible_catalog(process: &dyn Process) -> Option<std::sync::Arc<BTreeMap<String, Value>>> {
    let dir = super::mgmt::game_dir(process).ok()?;
    Some(crate::script::cached(&dir, "leader_visible", |d| {
        visible_whens(&crate::script::parse_dir(&d.join("common/characters")))
    }))
}

/// Adds `visible_when` (the role's visibility trigger as a plan predicate) to every roster row
/// whose character has one; pure over the catalog.
pub(super) fn mark_visibility(rows: &mut [Value], catalog: &BTreeMap<String, Value>) {
    for row in rows {
        if let Some(when) = row["id"].as_str().and_then(|id| catalog.get(id)) {
            row["visible_when"] = when.clone();
        }
    }
}

/// Merged roster entries (JSON) from a snapshot.
pub(super) fn entries(snap: &Snapshot) -> Vec<Value> {
    struct Acc {
        name: String,
        led: Vec<Target>,
        row: Option<(bool, bool)>, // (promote, fresh)
    }
    let mut acc: BTreeMap<String, Acc> = BTreeMap::new();
    for t in &snap.targets {
        if let Some(c) = &t.commander {
            acc.entry(key(c))
                .or_insert_with(|| Acc {
                    name: c.clone(),
                    led: Vec::new(),
                    row: None,
                })
                .led
                .push(t.clone());
        }
    }
    let rows = snap
        .rows
        .as_ref()
        .map(|r| (r, true))
        .or(snap.stale_rows.as_ref().map(|r| (r, false)));
    if let Some((rows, fresh)) = rows {
        for r in rows {
            let a = acc.entry(key(&r.name)).or_insert_with(|| Acc {
                name: r.name.clone(),
                led: Vec::new(),
                row: None,
            });
            a.row = Some((r.promote, fresh));
        }
    }
    let list_fresh = snap.rows.is_some();
    let leaders: BTreeMap<String, &units::Leader> = snap
        .leaders
        .iter()
        .flatten()
        .map(|l| (key(&l.name), l))
        .collect();
    // Leaders the army panel does not list (not recruited, foreign, retired)
    // appear with available unknown so the roster is complete.
    for (k, l) in &leaders {
        acc.entry(k.clone()).or_insert_with(|| Acc {
            name: l.name.clone(),
            led: Vec::new(),
            row: None,
        });
    }
    acc.into_values()
        .map(|a| {
            let leads_group = a.led.iter().any(|t| t.is_group);
            let (rank, rank_source, rank_validated) = if leads_group {
                (json!("field_marshal"), "leads_army_group", true)
            } else if matches!(a.row, Some((true, _))) {
                (json!("general"), "promote_button_shown", false)
            } else {
                (Value::Null, "unknown", false)
            };
            let assigned_to = a.led.first().map(|t| {
                json!({"kind": if t.is_group {"army_group"} else {"army"}, "name": t.name})
            });
            let mut sources = Vec::new();
            if !a.led.is_empty() {
                sources.push("orders_group_leader");
            }
            if a.row.is_some() {
                sources.push("leader_panel_grid");
            }
            let available = if !a.led.is_empty() {
                json!(false)
            } else if list_fresh && a.row.is_some() {
                json!(true)
            } else {
                Value::Null
            };
            let leader = leaders.get(&key(&a.name));
            let listed = a.row.is_some() || !a.led.is_empty();
            json!({"name":a.name,"rank":rank,"rank_source":rank_source,"rank_validated":rank_validated,
                "assigned_to":assigned_to,"promotion_needed_for_group":needs_promotion(true, matches!(a.row, Some((true, _)))),"assigned_to_count":a.led.len(),"available":available,
                "stale":a.row.is_some_and(|(_, fresh)| !fresh) && a.led.is_empty(),
                "skill":leader.map(|l| l.skill),"attack":leader.map(|l| l.attack),
                "defense":leader.map(|l| l.defense),"planning":leader.map(|l| l.planning),
                "logistics":leader.map(|l| l.logistics),
                "traits":leader.map(|l| l.traits.clone()),"id":leader.map(|l| l.id.clone()),
                "in_country_leader_list":leader.is_some(),"listed_in_panel_or_assigned":listed,
                "sources":sources,
                "validated":false,
                "validated_fields":["name","assigned_to","skill","attack","defense","planning","logistics","traits","id"]})
        })
        .collect()
}

/// Read the roster inputs: pure memory reads, no input, no cursor movement.
pub(super) fn snapshot(process: &dyn Process) -> Result<Snapshot> {
    let view = armies::armies(process)?;
    let targets = targets_from_view(&view);
    let leaders = read(process, units::leaders).ok();
    let (open, rows) = read(process, |r, image| {
        let open = match armies::leader_list_open(r, image) {
            Ok(v) => v,
            Err(e) if e.code == "invalid_gui" => return Ok((false, None)),
            Err(e) => return Err(e),
        };
        let rows = match armies::leader_entries(r, image) {
            Ok(rows) => Some(
                rows.into_iter()
                    .map(|(name, _, promote)| Row {
                        name,
                        promote: promote.is_some(),
                    })
                    .collect::<Vec<_>>(),
            ),
            Err(e) if e.code == "gui_target_unavailable" || e.code == "invalid_gui" => None,
            Err(e) => return Err(e),
        };
        Ok((open, rows))
    })?;
    Ok(if open {
        Snapshot {
            targets,
            rows,
            stale_rows: None,
            leaders,
        }
    } else {
        Snapshot {
            targets,
            rows: None,
            stale_rows: rows.filter(|r| !r.is_empty()),
            leaders,
        }
    })
}

/// Words of a name for matching: accents folded, lower case, punctuation as spaces.
fn name_words(name: &str) -> Vec<String> {
    super::states::fold(name)
        .split(|c: char| !c.is_ascii_alphanumeric())
        .filter(|w| !w.is_empty())
        .map(str::to_string)
        .collect()
}

/// The listed commander a request names, from every name the roster knows (`names`) and the
/// leaders' character ids (`ids`, id -> name). The asked text resolves when exactly one name
/// matches at the first step that matches any: the name itself (any case), the same words with
/// accents folded ("Gunther von Kluge"), the character id ("GER_albert_kesselring"), every asked
/// word among the name's words ("Kesselring", "von Rundstedt"), the asked words inside the name.
/// Two or more at one step refuse as ambiguous; none refuses with the closest names. A roster
/// with no names passes the asked text on unchanged (the mouse path reads the list itself).
/// The player's "Kesselring" was refused unknown_commander although "Albert Kesselring" was
/// listed free (g92a, 27 Nov 2026); pure.
pub(super) fn resolve_name(
    asked: &str,
    names: &[String],
    ids: &[(String, String)],
) -> Result<String> {
    if names.is_empty() {
        return Ok(asked.to_string());
    }
    let mut uniq: Vec<String> = Vec::new();
    for n in names {
        if !uniq.iter().any(|u| u == n) {
            uniq.push(n.clone());
        }
    }
    let want = name_words(asked);
    let joined = want.join(" ");
    let steps: [&dyn Fn(&String) -> bool; 5] = [
        &|n: &String| n.eq_ignore_ascii_case(asked),
        &|n: &String| !want.is_empty() && name_words(n) == want,
        &|n: &String| {
            ids.iter()
                .any(|(id, name)| name == n && id.eq_ignore_ascii_case(asked.trim()))
        },
        &|n: &String| {
            let words = name_words(n);
            !want.is_empty() && want.iter().all(|w| words.contains(w))
        },
        &|n: &String| !joined.is_empty() && name_words(n).join(" ").contains(&joined),
    ];
    for step in steps {
        let hits: Vec<&String> = uniq.iter().filter(|n| step(n)).collect();
        match hits.as_slice() {
            [one] => return Ok((*one).clone()),
            [] => continue,
            many => {
                return Err(Error::new(
                    "ambiguous_commander",
                    format!(
                        "\"{asked}\" names more than one commander: {}. Nothing was clicked; \
                         pass the full name.",
                        many.iter()
                            .map(|s| s.as_str())
                            .collect::<Vec<_>>()
                            .join(", ")
                    ),
                ))
            }
        }
    }
    Err(Error::new(
        "unknown_commander",
        format!(
            "No commander named \"{asked}\". Closest: {}. Nothing was clicked. Call \
             hoi4_commanders for the roster.",
            closest_names(asked, &uniq, 5).join(", ")
        ),
    ))
}

/// The full listed name of the commander `asked` names (see [`resolve_name`]): the country's
/// army leaders, the leaders of its armies and groups and the commander list's rows.
pub(super) fn canonical_name(process: &dyn Process, asked: &str) -> Result<String> {
    let snap = snapshot(process)?;
    let mut names: Vec<String> = snap
        .leaders
        .iter()
        .flatten()
        .map(|l| l.name.clone())
        .collect();
    names.extend(snap.known_names());
    names.extend(snap.stale_rows.iter().flatten().map(|r| r.name.clone()));
    let ids: Vec<(String, String)> = snap
        .leaders
        .iter()
        .flatten()
        .map(|l| (l.id.clone(), l.name.clone()))
        .collect();
    match resolve_name(asked, &names, &ids) {
        // only a complete roster (the country's leaders read, or the open list) refuses a
        // name; otherwise the commander list itself decides, as before
        Err(e)
            if e.code == "unknown_commander" && snap.leaders.is_none() && snap.rows.is_none() =>
        {
            Ok(asked.to_string())
        }
        other => other,
    }
}

pub(super) fn precheck(
    process: &dyn Process,
    army: &str,
    general: &str,
    reassign: bool,
) -> Result<()> {
    check_assignment(&snapshot(process)?, army, general, reassign)
}

/// `hoi4_commanders`: pure read.
pub(super) fn list(process: &dyn Process) -> Result<Value> {
    let snap = snapshot(process)?;
    let list_open = snap.rows.is_some();
    // While the panel list is fresh, leaders it does not list (not recruited, foreign,
    // retired) cannot be assigned: name them apart so a player does not pick them (Ethiopia
    // run 10 tried an unlisted general and Ethiopian leaders).
    let mut rows = entries(&snap);
    if let Some(catalog) = visible_catalog(process) {
        mark_visibility(&mut rows, &catalog);
    }
    let (commanders, not_assignable): (Vec<Value>, Vec<Value>) = rows
        .into_iter()
        .partition(|c| !list_open || c["listed_in_panel_or_assigned"] == json!(true));
    let not_assignable: Vec<Value> = not_assignable
        .into_iter()
        .map(|c| c["name"].clone())
        .collect();
    let targets: Vec<Value> = snap
        .targets
        .iter()
        .map(|t| {
            json!({"kind": if t.is_group {"army_group"} else {"army"}, "name":t.name,
            "commander":t.commander,"needs_rank":required_rank(t.is_group)})
        })
        .collect();
    let unled: Vec<&str> = snap
        .targets
        .iter()
        .filter(|t| t.commander.is_none())
        .map(|t| t.name.as_str())
        .collect();
    let note = if list_open {
        "Commander list is open: available generals are listed with available:true."
    } else {
        "Commander list is closed: only commanders already leading an army or group (and any \
         stale rows, stale:true) are known. Available generals appear after the army panel's \
         commander list is opened (hoi4_army_commander opens it and refuses unknown names with the \
         closest matches)."
    };
    Ok(json!({
        "commanders":commanders,
        "not_assignable":not_assignable,
        "armies_and_groups":targets,
        "without_commander":unled,
        "commander_list_open":list_open,
        "note":note,
        "leader_roster_read":snap.leaders.is_some(),
        "unavailable_fields":{
            "fields":["rank_of_unassigned_leaders","skill_levels_gained_in_the_campaign"],
            "reason":"skill/attack/defense/planning/logistics/traits/id are the character's base values from its template; rank (general or field marshal) and levels gained in the campaign are not stored there"},
        "validated":false,
    }))
}

pub(super) fn tool() -> Value {
    json!({"name":"hoi4_commanders","description":"Own army commanders: every commander who leads an own army or army group (name, rank, assigned_to) plus, while the army panel's commander list is open, every listed general with available true/false. Every army leader of the country is listed with id, skill, attack, defense, planning, logistics and traits (base values, matched against common/characters for five leaders); while the list is open, leaders it does not show (not recruited, foreign, retired) are only named in not_assignable; with the list closed they appear with listed_in_panel_or_assigned false and available null. Pure memory read: no lease, no input, no cursor. A leader whose army role the game keeps hidden until a trigger (common/characters `visible`, e.g. a focus) carries visible_when, that trigger as a plan predicate (unreadable triggers as unread_trigger atoms): such a leader is not assignable until it holds (an assignment reads back and the game removes him again). Use the names with hoi4_army_commander; it refuses unknown_commander, commander_unavailable and army_unknown before any click.","inputSchema":{"type":"object","properties":{},"additionalProperties":false}})
}

#[cfg(test)]
mod tests {
    use super::*;

    /// g92a (27 Nov 2026): the player's "Kesselring" was refused unknown_commander while "Albert
    /// Kesselring" was listed free. A listed commander's surname, folded name or id resolves.
    #[test]
    fn a_listed_commanders_surname_folded_name_or_id_resolves_to_his_full_name() {
        let names: Vec<String> = [
            "Albert Kesselring",
            "Günther von Kluge",
            "Gerd von Rundstedt",
            "Erich von Manstein",
            "Hans von Kluge",
        ]
        .iter()
        .map(|s| s.to_string())
        .collect();
        let ids = vec![(
            "GER_albert_kesselring".to_string(),
            "Albert Kesselring".to_string(),
        )];
        let r = |a: &str| resolve_name(a, &names, &ids);
        assert_eq!(r("Kesselring").unwrap(), "Albert Kesselring");
        assert_eq!(r("albert kesselring").unwrap(), "Albert Kesselring");
        assert_eq!(r("GER_albert_kesselring").unwrap(), "Albert Kesselring");
        assert_eq!(r("Gunther von Kluge").unwrap(), "Günther von Kluge");
        assert_eq!(r("von Rundstedt").unwrap(), "Gerd von Rundstedt");
        assert_eq!(r("Manst").unwrap(), "Erich von Manstein");
        // two Kluges: the surname alone is ambiguous, nothing is guessed
        assert_eq!(r("Kluge").unwrap_err().code, "ambiguous_commander");
        let e = r("Rommel").unwrap_err();
        assert_eq!(e.code, "unknown_commander");
        // an empty roster passes the name on (the mouse path reads the list itself)
        assert_eq!(resolve_name("Rommel", &[], &[]).unwrap(), "Rommel");
    }

    fn t(name: &str, is_group: bool, c: Option<&str>) -> Target {
        Target {
            name: name.into(),
            is_group,
            commander: c.map(str::to_string),
        }
    }
    fn row(name: &str, promote: bool) -> Row {
        Row {
            name: name.into(),
            promote,
        }
    }
    fn snap(rows: Option<Vec<Row>>) -> Snapshot {
        Snapshot {
            targets: vec![
                t("Army 1", false, Some("Walther von Brauchitsch")),
                t("Army 2", false, None),
                t("Army Group North", true, Some("Fedor von Bock")),
            ],
            rows,
            stale_rows: None,
            leaders: None,
        }
    }
    fn names(v: &[&str]) -> Vec<String> {
        v.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn closest_prefers_containment_and_shared_words_then_distance() {
        let c = names(&[
            "Erwin Rommel",
            "Heinz Guderian",
            "Erich von Manstein",
            "Fedor von Bock",
        ]);
        assert_eq!(closest_names("rommel", &c, 1), ["Erwin Rommel"]);
        assert_eq!(closest_names("Guderain", &c, 1), ["Heinz Guderian"]);
        assert_eq!(closest_names("von", &c, 5).len(), 4);
        assert_eq!(closest_names("x", &[], 5), Vec::<String>::new());
    }
    #[test]
    fn closest_is_bounded_and_deduplicated() {
        let c = names(&["A", "B", "C", "D", "E", "F", "F"]);
        assert_eq!(closest_names("zz", &c, 5).len(), 5);
        let dup = names(&["Same", "Same"]);
        assert_eq!(closest_names("same", &dup, 5), ["Same"]);
    }
    #[test]
    fn targets_parse_armies_and_groups() {
        let view = json!({"army_groups":[{"army_group":"G","field_marshal":"FM"}],
            "armies":[{"army":"A","commander":null},{"army":"B","commander":"X"}]});
        assert_eq!(
            targets_from_view(&view),
            vec![
                t("G", true, Some("FM")),
                t("A", false, None),
                t("B", false, Some("X"))
            ]
        );
    }
    #[test]
    fn unknown_army_lists_valid_names_and_closest() {
        let e = check_assignment(&snap(None), "Army 3", "Anyone", false).unwrap_err();
        assert_eq!(e.code, "army_unknown");
        assert!(e.message.contains("Army 1, Army 2"));
        assert!(e.message.contains("Army Group North"));
    }
    #[test]
    fn open_list_refuses_unknown_name_with_five_closest() {
        let rows = vec![
            row("Erwin Rommel", false),
            row("Heinz Guderian", true),
            row("Erich von Manstein", true),
            row("Gerd von Rundstedt", true),
            row("Wilhelm Keitel", false),
            row("Ewald von Kleist", false),
        ];
        let e = check_assignment(&snap(Some(rows)), "Army 2", "Rommell", false).unwrap_err();
        assert_eq!(e.code, "unknown_commander");
        assert!(e.message.contains("Erwin Rommel"));
        let list = e.message.split("Closest: ").nth(1).unwrap();
        let list = list.split(". Nothing").next().unwrap();
        assert_eq!(list.split(", ").count(), 5);
    }
    #[test]
    fn closed_list_never_refuses_an_unseen_name() {
        assert!(check_assignment(&snap(None), "Army 2", "Erwin Rommel", false).is_ok());
    }
    #[test]
    fn closed_list_still_knows_assigned_commanders() {
        let e = check_assignment(&snap(None), "Army 2", "fedor von bock", false).unwrap_err();
        assert_eq!(e.code, "commander_unavailable");
        assert!(e.message.contains("army group Army Group North"));
    }
    #[test]
    fn assigned_elsewhere_is_unavailable_unless_reassigning() {
        let rows = Some(vec![row("Walther von Brauchitsch", false)]);
        let e = check_assignment(
            &snap(rows.clone()),
            "Army 2",
            "Walther von Brauchitsch",
            false,
        )
        .unwrap_err();
        assert_eq!(e.code, "commander_unavailable");
        assert!(e.message.contains("army Army 1"));
        assert!(check_assignment(&snap(rows), "Army 2", "Walther von Brauchitsch", true).is_ok());
    }
    #[test]
    fn already_leading_the_same_target_is_fine() {
        assert!(check_assignment(&snap(None), "Army 1", "WALTHER VON BRAUCHITSCH", false).is_ok());
    }
    #[test]
    fn free_general_in_open_list_is_accepted() {
        let rows = Some(vec![row("Erwin Rommel", false)]);
        assert!(check_assignment(&snap(rows), "Army 2", "erwin rommel", false).is_ok());
    }
    #[test]
    fn rank_rules_for_armies_and_groups() {
        assert_eq!(required_rank(true), "field_marshal");
        assert_eq!(required_rank(false), "general");
        assert!(needs_promotion(true, true));
        assert!(!needs_promotion(true, false));
        assert!(!needs_promotion(false, true));
    }
    #[test]
    fn entries_merge_assigned_and_listed() {
        let mut s = snap(Some(vec![
            row("Erwin Rommel", true),
            row("Fedor von Bock", false),
        ]));
        let e = entries(&s);
        let find = |n: &str| e.iter().find(|v| v["name"] == n).unwrap().clone();
        let bock = find("Fedor von Bock");
        assert_eq!(bock["rank"], "field_marshal");
        assert_eq!(bock["assigned_to"]["kind"], "army_group");
        assert_eq!(bock["available"], false);
        let rommel = find("Erwin Rommel");
        assert_eq!(rommel["rank"], "general");
        assert_eq!(rommel["rank_validated"], false);
        assert_eq!(rommel["available"], true);
        assert!(rommel["assigned_to"].is_null());
        assert!(rommel["traits"].is_null());
        // Closed list: availability is unknown, not guessed.
        s.rows = None;
        let e = entries(&s);
        assert!(e.iter().all(|v| v["available"] != true));
        assert_eq!(e.len(), 2);
    }
    #[test]
    fn hidden_army_roles_carry_their_visible_trigger_as_a_predicate() {
        let text = r#"characters = {
            GER_hermann_hoth = { name = GER_hermann_hoth
                corps_commander = { skill = 3 visible = { OR = { has_global_flag = GER_civil_war_has_started has_completed_focus = GER_bribe_senior_officers } } } }
            GER_erwin_rommel = { corps_commander = { visible = { has_completed_focus = GER_adopt_new_panzer_doctrine } } }
            GER_kurt_x = { corps_commander = { visible = { NOT = { has_government = fascism has_completed_focus = GER_a } } } }
            GER_erich_von_manstein = { corps_commander = { skill = 4 } }
            GER_both = { field_marshal = { skill = 4 } corps_commander = { visible = { always = no } } }
            GER_adviser = { advisor = { slot = political_advisor } }
        }"#;
        let cat = visible_whens(&crate::script::parse(text));
        assert_eq!(
            cat["GER_hermann_hoth"],
            json!({"any": ["unread_trigger:has_global_flag", "focus_completed:GER_bribe_senior_officers"]})
        );
        assert_eq!(cat["GER_erwin_rommel"], json!("focus_completed:GER_adopt_new_panzer_doctrine"));
        assert_eq!(
            cat["GER_kurt_x"],
            json!({"not": {"any": ["unread_trigger:has_government", "focus_completed:GER_a"]}})
        );
        assert!(!cat.contains_key("GER_erich_von_manstein"));
        assert!(!cat.contains_key("GER_both"), "a role without a trigger is always visible");
        assert!(!cat.contains_key("GER_adviser"));
        let mut rows = vec![
            json!({"name": "Hermann Hoth", "id": "GER_hermann_hoth"}),
            json!({"name": "Erich von Manstein", "id": "GER_erich_von_manstein"}),
        ];
        mark_visibility(&mut rows, &cat);
        assert!(rows[0]["visible_when"].is_object());
        assert!(rows[1].get("visible_when").is_none());
    }
    #[test]
    fn stale_rows_are_marked_and_not_available() {
        let mut s = snap(None);
        s.stale_rows = Some(vec![row("Erwin Rommel", false)]);
        let e = entries(&s);
        let r = e.iter().find(|v| v["name"] == "Erwin Rommel").unwrap();
        assert_eq!(r["stale"], true);
        assert!(r["available"].is_null());
    }
}
