//! Division template management with ordinary input: create, edit in place, rename, set
//! priority and delete templates through the Division Designer and the Recruit & Deploy
//! panel, plus a read of the battalions and companies the designer currently offers.
//!
//! Everything the player may ask for is validated before any input from memory (sub-unit
//! definitions, own templates, divisions and training lines); every click is verified against
//! the designer's working copy, and the saved result against the own template list.
//! The Division Designer spends army experience when a template is saved (5 per battalion
//! added, 10 per company, 25 per new regiment column on this build, read from the window and
//! reported); it never costs political power.
use super::{
    deployment,
    designer::{self, Grid},
    settle::Verdict,
    template_get, ui,
    ui::read,
    units::{self, Def, Layout5},
};
use crate::{
    error::{Error, Result},
    game,
    platform::{macos_input::MacKeys, Process},
};
use serde_json::{json, Value};

/// Widest division the tool will build (the game's designer allows more; the playbook never
/// wants more than 40).
pub(super) const MAX_WIDTH: f64 = 40.0;
/// Longest template name accepted.
pub(super) const MAX_NAME: usize = 40;

pub(super) fn create_tool() -> Value {
    json!({"name":"hoi4_template_create","description":"Create a new own division template with ordinary input (U panel, Division Designer button, Create Empty, battalion slots, name box, Save). `regiments` is up to 5 columns of up to 5 battalion keys from common/units (infantry, mountaineers, cavalry, motorized, artillery_brigade, anti_tank_brigade, light_armor, ...; one list per column, top to bottom, no gaps); `support` is up to 5 company keys (engineer, recon, artillery, logistics_company, ...). Saving spends army experience (5 per battalion, 10 per company, 25 per extra column on this build; the cost is read from the window and returned). Refused before any input: name_taken, name_not_typable, unit_unknown, unit_wrong_slot (a company in a column or a battalion in support), column_mixed_groups (one column holds one group: infantry/mountaineers together, cavalry or artillery in their own column), template_too_wide (> 40), regiments_gap, support_duplicate, empty_template, insufficient_experience (checked before Save). unit_not_unlocked and slot_locked are found while building (nothing is saved; the draft is dropped). Returns the receipt and the resulting hoi4_template_get row (width, hit points, organisation, manpower, equipment need, ...).","inputSchema":{"type":"object","properties":{"name":{"type":"string","maxLength":40},"regiments":{"type":"array","maxItems":5,"items":{"type":"array","maxItems":5,"items":{"type":"string","maxLength":64}}},"support":{"type":"array","maxItems":5,"items":{"type":"string","maxLength":64}},"priority":{"type":"string","enum":["reserve","regular","elite"]},"request_id":{"type":"string"},"lease_token":{"type":"string"}},"required":["name","regiments","request_id","lease_token"],"additionalProperties":false}})
}
pub(super) fn edit_tool() -> Value {
    json!({"name":"hoi4_template_edit","description":"Edit an own division template in place through its Edit button: give the complete new `regiments` and/or `support` layout (omitted = unchanged), `priority` (reserve, regular, elite) and/or `new_name`. Only the differing slots are touched (Shift+click removes, slot click plus a choice replaces or adds). Verified from the designer's working copy and from the saved template. Templates used by divisions or training lines are refused (template_in_use) for layout changes unless allow_in_use is true: the game then reorganises those divisions at the new composition. Same refusals as hoi4_template_create plus template_unknown, nothing_to_change.","inputSchema":{"type":"object","properties":{"template":{"type":"string","maxLength":128},"regiments":{"type":"array","maxItems":5,"items":{"type":"array","maxItems":5,"items":{"type":"string","maxLength":64}}},"support":{"type":"array","maxItems":5,"items":{"type":"string","maxLength":64}},"priority":{"type":"string","enum":["reserve","regular","elite"]},"new_name":{"type":"string","maxLength":40},"allow_in_use":{"type":"boolean"},"request_id":{"type":"string"},"lease_token":{"type":"string"}},"required":["template","request_id","lease_token"],"additionalProperties":false}})
}
pub(super) fn rename_tool() -> Value {
    json!({"name":"hoi4_template_rename","description":"Rename an own division template (designer name box, Update). Refused with name_taken, name_not_typable or template_unknown before input. Divisions keep using the template.","inputSchema":{"type":"object","properties":{"template":{"type":"string","maxLength":128},"new_name":{"type":"string","maxLength":40},"request_id":{"type":"string"},"lease_token":{"type":"string"}},"required":["template","new_name","request_id","lease_token"],"additionalProperties":false}})
}
pub(super) fn delete_tool() -> Value {
    json!({"name":"hoi4_template_delete","description":"Delete an own division template (its Delete button in the U panel and the game's confirmation). The game refuses a template used by divisions (also as garrison template) or by training lines; that is reported as template_in_use (with the division and line counts) before any click. Verified when the template is gone from the own list.","inputSchema":{"type":"object","properties":{"template":{"type":"string","maxLength":128},"request_id":{"type":"string"},"lease_token":{"type":"string"}},"required":["template","request_id","lease_token"],"additionalProperties":false}})
}
pub(super) fn options_tool() -> Value {
    json!({"name":"hoi4_template_options","description":"Battalion and company types the Division Designer offers now (unlocked), read from its selection window (opens the designer on an empty draft and closes it again; nothing is saved and no experience is spent): per type the key, game name, group, army experience to add, width, hit points, organisation, manpower, supply use and equipment need from the definitions. Also the extra experience for a new column per group and the army experience held.","inputSchema":{"type":"object","properties":{"lease_token":{"type":"string"}},"required":["lease_token"],"additionalProperties":false}})
}

// ---- arguments ----------------------------------------------------------------------------
/// `[a-z0-9_]{1,64}`
pub(super) fn unit_key(v: &Value) -> Result<String> {
    let s = v
        .as_str()
        .ok_or_else(|| Error::new("invalid_arguments", "Unit keys must be strings"))?;
    if s.is_empty() || s.len() > 64 || !s.bytes().all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_') {
        return Err(Error::new(
            "invalid_arguments",
            format!("Invalid unit key {s:?} (lowercase letters, digits and underscore)"),
        ));
    }
    Ok(s.to_string())
}
/// Regiment columns: at most 5 columns of at most 5 keys; trailing empty columns are
/// dropped, an empty column before a filled one is refused.
pub(super) fn parse_regiments(v: &Value) -> Result<Vec<Vec<String>>> {
    let cols = v
        .as_array()
        .filter(|c| c.len() <= 5)
        .ok_or_else(|| Error::new("invalid_arguments", "regiments must be at most 5 columns"))?;
    let mut out: Vec<Vec<String>> = Vec::new();
    for col in cols {
        let rows = col
            .as_array()
            .filter(|r| r.len() <= 5)
            .ok_or_else(|| Error::new("invalid_arguments", "a column holds at most 5 battalions"))?;
        out.push(rows.iter().map(unit_key).collect::<Result<Vec<_>>>()?);
    }
    while out.last().is_some_and(Vec::is_empty) {
        out.pop();
    }
    if out.iter().any(Vec::is_empty) {
        return Err(Error::new(
            "regiments_gap",
            "An empty regiment column stands before a filled one; the designer fills columns left to right",
        ));
    }
    Ok(out)
}
pub(super) fn parse_support(v: &Value) -> Result<Vec<String>> {
    let rows = v
        .as_array()
        .filter(|r| r.len() <= 5)
        .ok_or_else(|| Error::new("invalid_arguments", "support holds at most 5 companies"))?;
    let out = rows.iter().map(unit_key).collect::<Result<Vec<_>>>()?;
    let mut seen = std::collections::BTreeSet::new();
    if let Some(dup) = out.iter().find(|k| !seen.insert(k.as_str())) {
        return Err(Error::new(
            "support_duplicate",
            format!("The designer takes each support company once ({dup} is listed twice)"),
        ));
    }
    Ok(out)
}
pub(super) fn name_arg(v: Option<&Value>, what: &str) -> Result<Option<String>> {
    let Some(v) = v else { return Ok(None) };
    let s = v
        .as_str()
        .ok_or_else(|| Error::new("invalid_arguments", format!("{what} must be a string")))?;
    check_name(s)?;
    Ok(Some(s.to_string()))
}
pub(super) fn check_name(s: &str) -> Result<()> {
    if s.trim() != s || s.is_empty() || s.chars().count() > MAX_NAME {
        return Err(Error::new(
            "invalid_arguments",
            format!("Template names are 1..{MAX_NAME} characters without leading or trailing space"),
        ));
    }
    if !designer::typable(s) {
        return Err(Error::new(
            "name_not_typable",
            "Template names may use letters, digits, space and - _ . , / ' ( ) only",
        ));
    }
    Ok(())
}
pub(super) fn priority_arg(v: Option<&Value>) -> Result<Option<i32>> {
    match v {
        None => Ok(None),
        Some(v) => v
            .as_str()
            .and_then(units::priority_value)
            .map(Some)
            .ok_or_else(|| Error::new("invalid_arguments", "priority is reserve, regular or elite")),
    }
}

// ---- pure checks and plan ---------------------------------------------------------------------
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Desired {
    pub columns: Vec<Vec<String>>,
    pub support: Vec<String>,
}
impl Desired {
    pub(super) fn of(l: &Layout5) -> Self {
        Self {
            columns: l.columns.clone(),
            support: l.support.clone(),
        }
    }
}
fn closest(keys: &[&str], want: &str) -> Vec<String> {
    let mut scored: Vec<(usize, &str)> = keys
        .iter()
        .map(|k| (edit_distance(k, want), *k))
        .collect();
    scored.sort();
    scored.into_iter().take(5).map(|(_, k)| k.to_string()).collect()
}
fn edit_distance(a: &str, b: &str) -> usize {
    let a: Vec<char> = a.chars().collect();
    let b: Vec<char> = b.chars().collect();
    let mut row: Vec<usize> = (0..=b.len()).collect();
    for i in 1..=a.len() {
        let mut prev = row[0];
        row[0] = i;
        for j in 1..=b.len() {
            let tmp = row[j];
            row[j] = (row[j] + 1)
                .min(row[j - 1] + 1)
                .min(prev + usize::from(a[i - 1] != b[j - 1]));
            prev = tmp;
        }
    }
    row[b.len()]
}
/// The support companies share one group id; `engineer` exists in every ruleset the tool
/// supports, so its group marks the company group (width 0 as the fallback).
fn is_company(defs: &[Def], d: &Def) -> bool {
    match defs.iter().find(|x| x.key == "engineer") {
        Some(e) => d.group == e.group,
        None => d.width == 0.0,
    }
}
/// Layout checks against the sub-unit catalog; returns the combat width.
pub(super) fn check_units(defs: &[Def], want: &Desired) -> Result<f64> {
    let keys: Vec<&str> = defs.iter().map(|d| d.key.as_str()).collect();
    let find = |key: &str| -> Result<&Def> {
        defs.iter().find(|d| d.key == key).ok_or_else(|| {
            Error::new(
                "unit_unknown",
                format!(
                    "No battalion or company {key:?}; closest keys: {}",
                    closest(&keys, key).join(", ")
                ),
            )
        })
    };
    let mut width = 0.0;
    let mut battalions = 0;
    for (c, col) in want.columns.iter().enumerate() {
        let mut group: Option<(i32, &str)> = None;
        for key in col {
            let d = find(key)?;
            match group {
                None => group = Some((d.group, key.as_str())),
                Some((g, first)) if g != d.group => {
                    return Err(Error::new(
                        "column_mixed_groups",
                        format!(
                            "Column {c} mixes {first} and {key}: a regiment column holds battalions of one group only (infantry, mobile, armored, support ...); put {key} in its own column"
                        ),
                    ))
                }
                _ => {}
            }
            if is_company(defs, d) {
                return Err(Error::new(
                    "unit_wrong_slot",
                    format!("{key} is a support company; put it in `support`"),
                ));
            }
            width += d.width;
            battalions += 1;
        }
    }
    for key in &want.support {
        let d = find(key)?;
        if !is_company(defs, d) {
            return Err(Error::new(
                "unit_wrong_slot",
                format!("{key} is a battalion; put it in a `regiments` column"),
            ));
        }
    }
    if battalions == 0 {
        return Err(Error::new(
            "empty_template",
            "A template needs at least one battalion",
        ));
    }
    if width > MAX_WIDTH {
        return Err(Error::new(
            "template_too_wide",
            format!("Combat width {width} exceeds the limit of {MAX_WIDTH}"),
        ));
    }
    Ok(width)
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum Op {
    Remove {
        grid: Grid,
        col: usize,
        row: usize,
    },
    Choose {
        grid: Grid,
        col: usize,
        row: usize,
        key: String,
    },
}
/// Slot operations that turn `cur` into `want`: surplus units are removed bottom-up and
/// right-to-left (so nothing closes up under another step), differing units replaced in
/// place, then missing units added column by column, top to bottom.
pub(super) fn plan(cur: &Desired, want: &Desired) -> Vec<Op> {
    let mut ops = Vec::new();
    let empty: Vec<String> = Vec::new();
    for c in (0..5).rev() {
        let have = cur.columns.get(c).unwrap_or(&empty).len();
        let need = want.columns.get(c).unwrap_or(&empty).len();
        for r in (need..have).rev() {
            ops.push(Op::Remove {
                grid: Grid::Regiments,
                col: c,
                row: r,
            });
        }
    }
    for r in (want.support.len()..cur.support.len()).rev() {
        ops.push(Op::Remove {
            grid: Grid::Support,
            col: 0,
            row: r,
        });
    }
    for c in 0..5 {
        let have = cur.columns.get(c).unwrap_or(&empty);
        let need = want.columns.get(c).unwrap_or(&empty);
        for (r, key) in need.iter().enumerate() {
            if have.get(r) != Some(key) {
                ops.push(Op::Choose {
                    grid: Grid::Regiments,
                    col: c,
                    row: r,
                    key: key.clone(),
                });
            }
        }
    }
    for (r, key) in want.support.iter().enumerate() {
        if cur.support.get(r) != Some(key) {
            ops.push(Op::Choose {
                grid: Grid::Support,
                col: 0,
                row: r,
                key: key.clone(),
            });
        }
    }
    ops
}
/// Settlement of a template save. Saving is atomic: either the template with the wanted
/// layout is in the own list under its final name, or it is as before. Only the template
/// being edited is compared (other templates and the date are never read).
pub(super) fn settle_layout(before: Option<&Layout5>, now: Option<&Layout5>, want: &Layout5) -> Verdict {
    if now == Some(want) {
        Verdict::Reached
    } else if now == before {
        Verdict::Unchanged
    } else {
        Verdict::Diverged
    }
}
/// Settlement of a deletion: the template is gone (reached) or still as it was (unchanged).
pub(super) fn settle_delete(before: &Layout5, now: Option<&Layout5>) -> Verdict {
    match now {
        None => Verdict::Reached,
        Some(n) if n == before => Verdict::Unchanged,
        Some(_) => Verdict::Diverged,
    }
}

// ---- reads for the pre-checks ----------------------------------------------------------------
pub(super) struct Usage {
    pub divisions: usize,
    pub training_lines: usize,
}
pub(super) fn usage(process: &dyn Process, name: &str) -> Result<Usage> {
    read(process, |r, image| {
        let templates = units::templates(r, image)?;
        let t = templates
            .iter()
            .find(|t| t.name == name)
            .ok_or_else(|| template_unknown(&templates.iter().map(|t| t.name.clone()).collect::<Vec<_>>(), name))?;
        let divisions = units::divisions(r, image)?
            .iter()
            .filter(|d| d.template == t.address)
            .count();
        Ok(Usage {
            divisions,
            training_lines: deployment::queued(r, image, name)?,
        })
    })
}
pub(super) fn template_unknown(names: &[String], name: &str) -> Error {
    Error::new(
        "template_unknown",
        format!("No own division template named {name:?}. Templates: {}.", names.join(", ")),
    )
}
pub(super) fn layouts(process: &dyn Process) -> Result<Vec<Layout5>> {
    read(process, units::template_layouts)
}
pub(super) fn catalog(process: &dyn Process) -> Result<Vec<Def>> {
    read(process, units::catalog)
}
fn name_taken(list: &[Layout5], name: &str, except: Option<&str>) -> Result<()> {
    if list
        .iter()
        .any(|l| l.name.eq_ignore_ascii_case(name) && Some(l.name.as_str()) != except)
    {
        return Err(Error::new(
            "name_taken",
            format!("An own template is already called {name:?}"),
        ));
    }
    Ok(())
}

// ---- planning a request ------------------------------------------------------------------------
/// A fully checked request: the layout the own list must hold afterwards.
#[derive(Debug)]
pub(super) struct Planned {
    /// Existing template to edit (None = create).
    pub existing: Option<String>,
    pub want: Layout5,
    pub before: Option<Layout5>,
    pub width: f64,
    pub changes: Vec<&'static str>,
}
pub(super) struct Input<'a> {
    pub existing: Option<&'a str>,
    pub name: Option<String>,
    pub regiments: Option<Vec<Vec<String>>>,
    pub support: Option<Vec<String>>,
    pub priority: Option<i32>,
    pub allow_in_use: bool,
}
/// The layout the own list must hold afterwards, and the template as it is now (None = create).
/// No refusals: a retried request uses this alone to settle.
pub(super) fn build_want(input: &Input<'_>, list: &[Layout5]) -> Result<(Layout5, Option<Layout5>, Vec<&'static str>)> {
    let before = match input.existing {
        Some(name) => Some(
            list.iter()
                .find(|l| l.name == name)
                .cloned()
                .ok_or_else(|| template_unknown(&list.iter().map(|l| l.name.clone()).collect::<Vec<_>>(), name))?,
        ),
        None => None,
    };
    let mut want = before.clone().unwrap_or(Layout5 {
        name: String::new(),
        columns: Vec::new(),
        support: Vec::new(),
        priority: 1,
    });
    let mut changes = Vec::new();
    if let Some(n) = &input.name {
        if n != &want.name {
            changes.push("name");
        }
        want.name = n.clone();
    }
    if let Some(c) = &input.regiments {
        if *c != want.columns {
            changes.push("regiments");
        }
        want.columns = c.clone();
    }
    if let Some(s) = &input.support {
        if *s != want.support {
            changes.push("support");
        }
        want.support = s.clone();
    }
    if let Some(p) = input.priority {
        if p != want.priority {
            changes.push("priority");
        }
        want.priority = p;
    }
    if before.is_none() {
        changes = vec!["new template"];
    }
    Ok((want, before, changes))
}
/// Every refusal that needs no input, in order. Pure over the supplied reads.
pub(super) fn plan_request(
    input: &Input<'_>,
    list: &[Layout5],
    defs: &[Def],
    usage: Option<&Usage>,
) -> Result<Planned> {
    let (want, before, changes) = build_want(input, list)?;
    if let Some(n) = &input.name {
        name_taken(list, n, input.existing)?;
    }
    if want.name.is_empty() {
        return Err(Error::new("invalid_arguments", "A new template needs a name"));
    }
    if before.is_some() && changes.is_empty() {
        return Err(Error::new(
            "nothing_to_change",
            "The template already has exactly that layout, name and priority",
        ));
    }
    let width = check_units(defs, &Desired::of(&want))?;
    if let (Some(u), true) = (usage, changes.contains(&"regiments") || changes.contains(&"support")) {
        if (u.divisions > 0 || u.training_lines > 0) && !input.allow_in_use {
            return Err(Error::new(
                "template_in_use",
                format!(
                    "{} division(s) and {} training line(s) use {:?}; changing its composition makes the game reorganise them. Pass allow_in_use: true to do it anyway, or build a new template and train that.",
                    u.divisions, u.training_lines, input.existing.unwrap_or("")
                ),
            ));
        }
    }
    Ok(Planned {
        existing: input.existing.map(str::to_string),
        want,
        before,
        width,
        changes,
    })
}

// ---- the editing run ------------------------------------------------------------------------------
fn mismatch(what: &str, want: &Layout5, got: &Layout5) -> Error {
    Error::new(
        "input_readback_failed",
        format!(
            "{what}: the designer holds {:?} / {:?} (priority {}) instead of {:?} / {:?} (priority {})",
            got.columns, got.support, got.priority, want.columns, want.support, want.priority
        ),
    )
}
/// Drive the designer to the planned layout and Save. Cleans the screen up on every path.
pub(super) fn apply(process: &dyn Process, keys: &MacKeys, plan: &Planned) -> Result<Value> {
    let result = apply_inner(process, keys, plan);
    designer::leave(process, keys);
    result
}
fn apply_inner(process: &dyn Process, keys: &MacKeys, plan: &Planned) -> Result<Value> {
    let paused = game::clock_context(process)?.paused;
    match &plan.existing {
        Some(name) => designer::open_template(process, keys, name)?,
        None => {
            designer::open_new_designer(process, keys)?;
            designer::create_empty(process, keys)?;
        }
    }
    let cur = read(process, designer::draft)?;
    if let Some(name) = &plan.existing {
        if &cur.name != name {
            return Err(Error::new(
                "input_readback_failed",
                format!("The designer opened {:?} instead of {name:?}", cur.name),
            ));
        }
    }
    let ops = plan_ops(&cur, &plan.want);
    // Lower bound of the experience the steps cost (5 per battalion added, replaced or
    // removed, 10 per company; new groups cost more): refuse before touching the draft.
    let floor = min_experience(&ops);
    if let Some(have) = read(process, designer::texts)?.experience {
        if floor > have {
            return Err(Error::new(
                "insufficient_experience",
                format!("These steps cost at least {floor} army experience but only {have} are held; nothing was changed"),
            ));
        }
    }
    let mut spent: Vec<Value> = Vec::new();
    for op in &ops {
        match op {
            Op::Remove { grid, col, row } => designer::remove(process, keys, *grid, *col, *row)?,
            Op::Choose {
                grid,
                col,
                row,
                key,
            } => {
                let cost = designer::choose(process, keys, *grid, *col, *row, key)?;
                spent.push(json!({"unit":key,"column":col,"row":row,"experience_listed":cost}));
            }
        }
    }
    let after_slots = read(process, designer::draft)?;
    if after_slots.columns != plan.want.columns || after_slots.support != plan.want.support {
        return Err(mismatch("after the slot steps", &plan.want, &after_slots));
    }
    designer::set_priority(process, keys, plan.want.priority)?;
    designer::set_name(process, keys, &plan.want.name)?;
    let draft = read(process, designer::draft)?;
    if draft != plan.want {
        return Err(mismatch("before saving", &plan.want, &draft));
    }
    let texts = read(process, designer::texts)?;
    if let (Some(cost), Some(have)) = (texts.save_cost, texts.experience) {
        if cost > have {
            return Err(Error::new(
                "insufficient_experience",
                format!("Saving costs {cost} army experience but only {have} are held (the game disables Save); nothing was saved"),
            ));
        }
    }
    if !texts.save_enabled {
        return Err(Error::new(
            "nothing_to_change",
            "The designer offers nothing to save (the template already matches)",
        ));
    }
    designer::save(process, keys, &plan.want)?;
    let after = read(process, designer::texts)?;
    if game::clock_context(process)?.paused != paused {
        return Err(Error::new(
            "unexpected_clock_change",
            "The game's pause state changed while typing; check hoi4_time",
        ));
    }
    Ok(json!({
        "operations": ops.len(),
        "changes": plan.changes,
        "combat_width_planned": plan.width,
        "template_before": plan.before.as_ref().map(|b| json!({"name":b.name,"regiments":b.columns,"support":b.support,"priority":units::priority_name(b.priority)})),
        "placed": spent,
        "experience_cost": texts.save_cost,
        "experience_before": texts.experience,
        "experience_after": after.experience,
        "save_button": texts.save_label,
        "designer_combat_width": texts.width,
        "estimated_production_cost": texts.production,
    }))
}
/// Cheapest possible experience of the steps on this build's price list.
pub(super) fn min_experience(ops: &[Op]) -> i64 {
    ops.iter()
        .map(|o| match o {
            Op::Remove { .. } => 5,
            Op::Choose { grid: Grid::Support, .. } => 10,
            Op::Choose { .. } => 5,
        })
        .sum()
}
fn plan_ops(cur: &Layout5, want: &Layout5) -> Vec<Op> {
    plan(&Desired::of(cur), &Desired::of(want))
}
/// The own template's `hoi4_template_get` row, for the receipt.
pub(super) fn readback(process: &dyn Process, name: &str) -> Result<Value> {
    template_get::get(process, Some(name))
}

// ---- delete ---------------------------------------------------------------------------------------
pub(super) fn plan_delete(process: &dyn Process, name: &str) -> Result<(Layout5, Usage)> {
    let list = layouts(process)?;
    let before = list
        .iter()
        .find(|l| l.name == name)
        .cloned()
        .ok_or_else(|| template_unknown(&list.iter().map(|l| l.name.clone()).collect::<Vec<_>>(), name))?;
    let u = usage(process, name)?;
    if u.divisions > 0 || u.training_lines > 0 {
        return Err(Error::new(
            "template_in_use",
            format!(
                "{} division(s) and {} training line(s) use {name:?}; the game refuses to delete a template while divisions use it (cancel the lines and re-assign or disband the divisions first)",
                u.divisions, u.training_lines
            ),
        ));
    }
    Ok((before, u))
}
pub(super) fn delete(process: &dyn Process, keys: &MacKeys, name: &str) -> Result<()> {
    let result = delete_inner(process, keys, name);
    deployment::leave(process, keys);
    result
}
fn delete_inner(process: &dyn Process, keys: &MacKeys, name: &str) -> Result<()> {
    ui::clear_notices(process, keys)?;
    deployment::ensure_open(process, keys)?;
    let (rect, disabled) = ui::settled(process, "delete button", |r, i| {
        deployment::row_button(r, i, name, "delete_button")
    })?;
    if disabled {
        return Err(Error::new(
            "template_in_use",
            "The game disables Delete for this template (used by divisions, training lines or as a garrison template)",
        ));
    }
    let (x, y) = rect.center();
    keys.click(designer::GAME, x, y)?;
    if !ui::accept_command(process, keys)? {
        return Err(Error::new(
            "input_readback_failed",
            "The game showed no delete confirmation",
        ));
    }
    ui::wait(process, "template deleted", |r, i| {
        Ok(!units::template_layouts(r, i)?.iter().any(|l| l.name == name))
    })
}

// ---- options ---------------------------------------------------------------------------------------
/// What the designer offers: opens it on an empty draft, reads both selection levels, closes.
pub(super) fn options(process: &dyn Process, keys: &MacKeys) -> Result<Value> {
    let result = options_inner(process, keys);
    designer::leave(process, keys);
    result
}
fn options_inner(process: &dyn Process, keys: &MacKeys) -> Result<Value> {
    designer::open_new_designer(process, keys)?;
    designer::create_empty(process, keys)?;
    let defs = catalog(process)?;
    let battalions = designer::list_group_units(process, keys, Grid::Regiments)?;
    let companies = designer::list_group_units(process, keys, Grid::Support)?;
    let t = read(process, designer::texts)?;
    let detail = |listing: &designer::Listing| -> Result<Value> {
        let rows = listing
            .units
            .iter()
            .map(|u| {
                let def = defs.iter().find(|d| d.key == u.key);
                let sub = match def {
                    Some(d) => Some(read(process, |r, i| units::read_subunit(r, i, d.address))?),
                    None => None,
                };
                Ok(json!({
                    "key": u.key,
                    "name": u.label,
                    "group": u.group,
                    "experience_to_add": u.cost,
                    "width": sub.as_ref().map(|s| s.width),
                    "hit_points": sub.as_ref().map(|s| s.hp),
                    "organisation": sub.as_ref().map(|s| s.org),
                    "manpower": sub.as_ref().map(|s| s.manpower),
                    "supply_use": sub.as_ref().map(|s| s.supply),
                    "training_days": sub.as_ref().map(|s| s.training_days),
                    "equipment_need": sub.as_ref().map(|s| s.need.iter().map(|(k, v)| (k.clone(), json!(v))).collect::<serde_json::Map<_, _>>()),
                }))
            })
            .collect::<Result<Vec<_>>>()?;
        Ok(json!(rows))
    };
    Ok(json!({
        "battalions": detail(&battalions)?,
        "new_column_cost": battalions.groups.iter().map(|g| json!({"group":g.0,"experience_for_new_column":g.1})).collect::<Vec<_>>(),
        "support_companies": detail(&companies)?,
        "army_experience": t.experience,
        "note": "Companies already in the open template are not offered again; this listing is for an empty draft. Locked units (not researched) are absent, the designer does not draw them.",
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    fn s(v: &[&str]) -> Vec<String> {
        v.iter().map(|x| x.to_string()).collect()
    }
    fn defs() -> Vec<Def> {
        let d = |key: &str, group: i32, width: f64| Def {
            address: 0,
            key: key.into(),
            group,
            width,
        };
        vec![
            d("infantry", 1, 2.0),
            d("mountaineers", 1, 2.0),
            d("artillery_brigade", 2, 3.0),
            d("engineer", 9, 0.0),
            d("recon", 9, 0.0),
            d("light_armor", 3, 2.0),
        ]
    }
    fn desired(cols: &[&[&str]], sup: &[&str]) -> Desired {
        Desired {
            columns: cols.iter().map(|c| s(c)).collect(),
            support: s(sup),
        }
    }
    #[test]
    fn regiments_parse_and_gaps_are_typed() {
        let v = json!([["infantry", "infantry"], [], []]);
        assert_eq!(parse_regiments(&v).unwrap(), vec![s(&["infantry", "infantry"])]);
        let e = parse_regiments(&json!([[], ["infantry"]])).unwrap_err();
        assert_eq!(e.code, "regiments_gap");
        assert!(parse_regiments(&json!([[1]])).is_err());
        assert!(parse_regiments(&json!([[], [], [], [], [], []])).is_err());
        assert!(parse_regiments(&json!([["a", "a", "a", "a", "a", "a"]])).is_err());
        assert!(parse_regiments(&json!([["Infantry"]])).is_err());
    }
    #[test]
    fn support_duplicates_are_typed() {
        assert_eq!(parse_support(&json!(["engineer"])).unwrap(), s(&["engineer"]));
        assert_eq!(
            parse_support(&json!(["engineer", "engineer"])).unwrap_err().code,
            "support_duplicate"
        );
    }
    #[test]
    fn names_are_validated_for_typing() {
        assert!(check_name("Infanterie-Division (mot.)").is_ok());
        assert!(check_name("Zz Test A").is_ok());
        assert_eq!(check_name("Bad%").unwrap_err().code, "name_not_typable");
        assert_eq!(check_name("Ünicode").unwrap_err().code, "name_not_typable");
        assert!(check_name(" lead").is_err());
        assert!(check_name("").is_err());
        assert!(check_name(&"x".repeat(41)).is_err());
    }
    #[test]
    fn units_are_checked_against_the_catalog() {
        let d = defs();
        let ok = desired(&[&["infantry", "infantry"], &["artillery_brigade"]], &["engineer"]);
        assert_eq!(check_units(&d, &ok).unwrap(), 7.0);
        let e = check_units(&d, &desired(&[&["infantri"]], &[])).unwrap_err();
        assert_eq!(e.code, "unit_unknown");
        assert!(e.message.contains("infantry"));
        assert_eq!(
            check_units(&d, &desired(&[&["engineer"]], &[])).unwrap_err().code,
            "unit_wrong_slot"
        );
        assert_eq!(
            check_units(&d, &desired(&[&["infantry"]], &["infantry"])).unwrap_err().code,
            "unit_wrong_slot"
        );
        assert_eq!(
            check_units(&d, &desired(&[], &["engineer"])).unwrap_err().code,
            "empty_template"
        );
        assert!(check_units(&d, &desired(&[&["infantry", "mountaineers"]], &[])).is_ok());
        assert_eq!(
            check_units(&d, &desired(&[&["infantry", "artillery_brigade"]], &[])).unwrap_err().code,
            "column_mixed_groups"
        );
        let wide: Vec<&str> = vec!["infantry"; 5];
        let w = desired(&[&wide, &wide, &wide, &wide, &wide], &[]);
        assert_eq!(check_units(&d, &w).unwrap_err().code, "template_too_wide");
    }
    #[test]
    fn plan_builds_columns_in_order_for_a_new_template() {
        let cur = desired(&[], &[]);
        let want = desired(&[&["infantry", "infantry"], &["infantry"]], &["engineer"]);
        let ops = plan(&cur, &want);
        let keyed: Vec<(usize, usize)> = ops
            .iter()
            .filter_map(|o| match o {
                Op::Choose { grid: Grid::Regiments, col, row, .. } => Some((*col, *row)),
                _ => None,
            })
            .collect();
        assert_eq!(keyed, vec![(0, 0), (0, 1), (1, 0)]);
        assert!(matches!(ops.last(), Some(Op::Choose { grid: Grid::Support, row: 0, .. })));
        assert_eq!(ops.len(), 4);
    }
    #[test]
    fn plan_removes_bottom_up_then_changes_then_adds() {
        let cur = desired(
            &[&["infantry", "infantry", "infantry"], &["infantry", "infantry"]],
            &["engineer", "recon"],
        );
        let want = desired(&[&["infantry", "light_armor"], &["infantry", "infantry", "infantry"]], &["engineer"]);
        let ops = plan(&cur, &want);
        assert_eq!(
            ops,
            vec![
                Op::Remove { grid: Grid::Regiments, col: 0, row: 2 },
                Op::Remove { grid: Grid::Support, col: 0, row: 1 },
                Op::Choose { grid: Grid::Regiments, col: 0, row: 1, key: "light_armor".into() },
                Op::Choose { grid: Grid::Regiments, col: 1, row: 2, key: "infantry".into() },
            ]
        );
    }
    #[test]
    fn experience_floor_counts_each_step() {
        let cur = desired(&[&["infantry", "infantry"]], &[]);
        let want = desired(&[&["infantry"], &["infantry"]], &["engineer"]);
        assert_eq!(min_experience(&plan(&cur, &want)), 5 + 5 + 10);
    }
    #[test]
    fn plan_of_an_identical_layout_is_empty() {
        let a = desired(&[&["infantry"]], &["engineer"]);
        assert!(plan(&a, &a).is_empty());
    }
    #[test]
    fn right_columns_go_before_left_ones() {
        let cur = desired(&[&["infantry"], &["infantry"], &["infantry", "infantry"]], &[]);
        let want = desired(&[&["infantry"]], &[]);
        let rows: Vec<(usize, usize)> = plan(&cur, &want)
            .iter()
            .map(|o| match o {
                Op::Remove { col, row, .. } => (*col, *row),
                _ => unreachable!(),
            })
            .collect();
        assert_eq!(rows, vec![(2, 1), (2, 0), (1, 0)]);
    }
    fn layout(name: &str, cols: &[&[&str]], sup: &[&str], p: i32) -> Layout5 {
        Layout5 {
            name: name.into(),
            columns: cols.iter().map(|c| s(c)).collect(),
            support: s(sup),
            priority: p,
        }
    }
    #[test]
    fn requests_are_refused_before_input() {
        let list = vec![
            layout("Alpha", &[&["infantry"]], &[], 1),
            layout("Beta", &[&["infantry", "infantry"]], &["engineer"], 1),
        ];
        let d = defs();
        let create = |name: &str, cols: Vec<Vec<String>>| {
            plan_request(
                &Input {
                    existing: None,
                    name: Some(name.into()),
                    regiments: Some(cols),
                    support: Some(vec![]),
                    priority: None,
                    allow_in_use: false,
                },
                &list,
                &d,
                None,
            )
        };
        assert_eq!(create("alpha", vec![s(&["infantry"])]).unwrap_err().code, "name_taken");
        assert!(create("Gamma", vec![s(&["infantry"])]).is_ok());
        let edit = |existing: &str, new_name: Option<&str>, regs: Option<Vec<Vec<String>>>, usage: Option<Usage>, allow: bool| {
            plan_request(
                &Input {
                    existing: Some(existing),
                    name: new_name.map(str::to_string),
                    regiments: regs,
                    support: None,
                    priority: None,
                    allow_in_use: allow,
                },
                &list,
                &d,
                usage.as_ref(),
            )
        };
        assert_eq!(edit("Nope", None, None, None, false).unwrap_err().code, "template_unknown");
        assert_eq!(edit("Alpha", None, None, None, false).unwrap_err().code, "nothing_to_change");
        assert_eq!(edit("Alpha", Some("Beta"), None, None, false).unwrap_err().code, "name_taken");
        assert!(edit("Alpha", Some("Alpha"), Some(vec![s(&["infantry", "infantry"])]), None, false).is_ok());
        let used = || Some(Usage { divisions: 3, training_lines: 0 });
        assert_eq!(
            edit("Alpha", None, Some(vec![s(&["infantry", "infantry"])]), used(), false).unwrap_err().code,
            "template_in_use"
        );
        assert!(edit("Alpha", None, Some(vec![s(&["infantry", "infantry"])]), used(), true).is_ok());
        // A rename of a used template is not a composition change.
        assert!(edit("Alpha", Some("Alpha 2"), None, used(), false).is_ok());
    }
    #[test]
    fn save_settlement_compares_only_the_edited_template() {
        let before = layout("A", &[&["infantry"]], &[], 1);
        let want = layout("A2", &[&["infantry", "infantry"]], &[], 2);
        assert_eq!(settle_layout(Some(&before), Some(&want), &want), Verdict::Reached);
        assert_eq!(settle_layout(Some(&before), Some(&before), &want), Verdict::Unchanged);
        assert_eq!(settle_layout(None, None, &want), Verdict::Unchanged);
        let other = layout("A2", &[&["infantry", "light_armor"]], &[], 2);
        assert_eq!(settle_layout(Some(&before), Some(&other), &want), Verdict::Diverged);
        assert_eq!(settle_delete(&before, None), Verdict::Reached);
        assert_eq!(settle_delete(&before, Some(&before)), Verdict::Unchanged);
    }
    #[test]
    fn edit_distance_orders_close_keys() {
        assert_eq!(edit_distance("infantry", "infantri"), 1);
        let c = closest(&["infantry", "cavalry", "engineer"], "infantri");
        assert_eq!(c[0], "infantry");
    }
}
