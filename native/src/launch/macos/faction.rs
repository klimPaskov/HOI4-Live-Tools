//! Faction and incoming-proposal tools (UNVALIDATED, built offline from
//! `interface/factions/new_faction_window.gui`, `countryfactionview.gui`, `topbarfactionbutton.gui`,
//! `popupwindow.gui` and the diplomacy localisation).
//!
//! * `hoi4_faction_create`: top bar `faction_button` (shortcut ctrl+Q) opens `new_faction_window`
//!   (`faction_template_grid`: one `new_faction_template_item` per template with `name`, `manifest/manifest_name`,
//!   `rule_window/rule_text`, `goal_window/goal_text`, `select_button`; `create_faction_button` below), which opens
//!   `faction_setup_window` (`name_selection` edit box, `invite_countries` button with `invite_countries_window`:
//!   `countries_grid` rows `invite_country_item` with `select_button` / `selected_icon`, `invite_button`; and the
//!   final `create_faction`). The name box is typed with the same verified key-by-key routine as design names.
//! * `hoi4_faction_leave`: `countryfactionview`'s `leave_faction_button` (sprite `GFX_dismantle_faction_button`:
//!   for the leader it dismantles) and the game's confirmation; needs confirm:"leave_faction".
//! * `hoi4_diplomacy_incoming` / `hoi4_faction_invite_accept`: the diplomacy proposal pop-ups
//!   (`diplomacy_standard_popup_window`: `title`, `description`, `decline_button`, `accept_button`; classes
//!   `CStandardDiplomacyPopup`, `CScriptedGUIDiplomacyPopup`, `CIncomingLendLeaseDiplomacyPopup`,
//!   `CForeignManpowerDiplomacyPopup`) listed from the game GUI's pop-up list and answered with their own buttons.
//!
//! Windows are found by name inside the faction views (class vtables from the RTTI map, fixed manager slots
//! +0x168 / +0x170 as fallback); no field offset is assumed. Memory proves the result (faction membership,
//! pop-up gone); nothing is repeated after possible input.
use super::{
    design_edit, designer, diplomacy_state,
    journal::Outcome,
    kit::{self, same_name, Env, Popup},
    navalui as nui,
    settle::Verdict,
    ui::{clear_notices, read, settled, wait},
};
use crate::{
    error::{Error, Result},
    gui_tree::{self, Rect},
    memory::Reader,
    platform::{macos_input::MacKeys, Process},
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};
use std::{thread, time::{Duration, Instant}};

pub(super) const TOOLS: [&str; 3] = ["hoi4_faction_create", "hoi4_faction_leave", "hoi4_faction_invite_accept"];
pub(super) const INCOMING: &str = "hoi4_diplomacy_incoming";
/// Actions of this module that send input (the read form of `hoi4_diplomacy_incoming` is separate).
pub(super) fn handles(name: &str) -> bool {
    TOOLS.contains(&name) || name == INCOMING
}

/// `CCountryFactionView` and `CFactionSetupWindow` (RTTI map) and their interface manager slots.
const FACTION_VIEW: (u64, u64) = (0x3536180, 0x168);
const SETUP_VIEW: (u64, u64) = (0x353a060, 0x170);
const NAME_BOX_FROM_ICON: (f64, f64) = (222.0, 1.0);

fn pause(ms: u64) {
    thread::sleep(Duration::from_millis(ms));
}

// ---- windows --------------------------------------------------------------------------------------
fn view_of(r: &Reader<'_>, image: u64, (vtable, slot): (u64, u64)) -> Result<u64> {
    if let Some(view) = kit::find_view(r, image, vtable)? {
        return Ok(view);
    }
    let view = r.u64(kit::manager(r, image)? + slot)?;
    if view == 0 {
        return Err(Error::new("invalid_gui", "Faction view unavailable"));
    }
    Ok(view)
}
/// The named window of the faction views and whether it is shown.
fn window(r: &Reader<'_>, image: u64, name: &str) -> Result<Option<(u64, bool)>> {
    for view in [SETUP_VIEW, FACTION_VIEW] {
        let Ok(object) = view_of(r, image, view) else {
            continue;
        };
        if let Some(w) = kit::find_window(r, image, object, name)? {
            return Ok(Some((w, gui_tree::visible(r, w)?)));
        }
    }
    Ok(None)
}
fn open_window(r: &Reader<'_>, image: u64, name: &str) -> Result<Option<u64>> {
    Ok(window(r, image, name)?.filter(|(_, shown)| *shown).map(|(w, _)| w))
}
fn need(r: &Reader<'_>, image: u64, name: &str) -> Result<u64> {
    open_window(r, image, name)?.ok_or_else(|| Error::new("gui_target_unavailable", format!("{name} is not open")))
}
fn button(r: &Reader<'_>, image: u64, window: &str, widget: &str) -> Result<(Rect, i32)> {
    kit::rect_deep(r, image, need(r, image, window)?, widget)
}
/// Close whichever faction windows are open through their own close buttons (best effort).
fn close_windows(process: &dyn Process, keys: &MacKeys<'_>) {
    for name in ["invite_countries_window", "faction_setup_window", "new_faction_window", "countryfactionview"] {
        if let Ok(Some(_)) = read(process, |r, i| open_window(r, i, name)) {
            if let Ok((rect, _)) = read(process, |r, i| button(r, i, name, "close_button")) {
                let _ = kit::click(keys, rect);
                let _ = wait(process, "faction window closed", |r, i| Ok(open_window(r, i, name)?.is_none()));
            }
        }
    }
}
fn open_from_topbar(process: &dyn Process, keys: &MacKeys<'_>, window_name: &str) -> Result<()> {
    let rect = settled(process, "faction button", |r, i| kit::topbar_button(r, i, "faction_button")).map_err(|e| {
        Error::new(
            "capability_unavailable",
            format!(
                "The top bar shows no faction button ({}); creating a faction needs the 'can create factions' rule, \
                 and the button is hidden while the player has no visible faction",
                e.message
            ),
        )
    })?;
    kit::click(keys, rect)?;
    wait(process, window_name, |r, i| Ok(open_window(r, i, window_name)?.is_some()))
}

// ---- templates ------------------------------------------------------------------------------------
#[derive(Clone, Debug, PartialEq)]
struct Template {
    name: String,
    manifest: String,
    rule: String,
    goal: String,
    select: Rect,
}
fn templates(r: &Reader<'_>, image: u64) -> Result<Vec<Template>> {
    let window = need(r, image, "new_faction_window")?;
    let grid = kit::find_deep(r, image, window, "faction_template_grid", 4)?
        .ok_or_else(|| Error::new("gui_target_unavailable", "faction_template_grid not found"))?;
    let mut out = Vec::new();
    for item in nui::grid_items(r, grid)?.into_iter().take(24) {
        let Some(frame) = gui_tree::container(r, r.u64(item + 0x10)?, image)? else {
            continue;
        };
        let text = |name: &str| -> Result<String> {
            match kit::find_deep(r, image, frame, name, 3)? {
                Some(w) => nui::text_at(r, w),
                None => Ok(String::new()),
            }
        };
        let Some(select) = kit::find_deep(r, image, frame, "select_button", 1)? else {
            continue;
        };
        out.push(Template {
            name: text("name")?,
            manifest: text("manifest_name")?,
            rule: text("rule_text")?,
            goal: text("goal_text")?,
            select: gui_tree::rect_unclipped(r, image, select, kit::GAME)?,
        });
    }
    Ok(out)
}
/// Choose the template: the named one (name or manifest name), or the only one.
fn pick_template(list: &[(String, String)], wanted: Option<&str>) -> Result<usize> {
    let names = || list.iter().map(|(n, m)| if m.is_empty() { n.clone() } else { format!("{n} ({m})") }).collect::<Vec<_>>().join(" | ");
    match wanted {
        None if list.len() == 1 => Ok(0),
        None => Err(Error::new("template_required", format!("Several faction templates are offered; pass template: {}", names()))),
        Some(w) => {
            let hits: Vec<usize> = (0..list.len()).filter(|i| same_name(&list[*i].0, w) || same_name(&list[*i].1, w)).collect();
            match hits.len() {
                1 => Ok(hits[0]),
                0 => Err(Error::new("template_unknown", format!("No faction template {w:?}; offered: {}", names()))),
                _ => Err(Error::new("template_ambiguous", format!("Several templates match {w:?}: {}", names()))),
            }
        }
    }
}
fn on_screen(rect: Rect) -> bool {
    rect.x >= 0 && rect.y >= 150 && rect.x + rect.w <= 1900 && rect.y + rect.h <= 960
}
/// Screen point of the name edit box, from the icon button that sits left of it.
fn name_box_point(icon: Rect) -> (f64, f64) {
    let (x, y) = icon.center();
    (x + NAME_BOX_FROM_ICON.0, y + NAME_BOX_FROM_ICON.1)
}

// ---- create ---------------------------------------------------------------------------------------
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub(super) struct CreatePlan {
    pub template: Option<String>,
    pub name: Option<String>,
    pub invite: Vec<String>,
}
pub(super) fn plan_create(member_of: Option<&(String, Vec<String>)>, template: Option<String>, name: Option<String>, invite: Vec<String>) -> Result<CreatePlan> {
    if let Some((faction, members)) = member_of {
        return Err(Error::new(
            "already_in_faction",
            format!("The player is in the faction {faction} (members: {}); leave it first (hoi4_faction_leave) or invite countries with hoi4_diplomacy_act invite_to_faction", members.join(", ")),
        ));
    }
    if let Some(n) = &name {
        if !designer::typable(n) {
            return Err(Error::new("name_not_typable", "Faction names may use letters, digits, space and - _ . , / ' ( )"));
        }
    }
    Ok(CreatePlan { template, name, invite })
}
/// Membership proves the result: any faction (named as asked when a name was given).
pub(super) fn settle_create(plan: &CreatePlan, now: Option<&(String, Vec<String>)>) -> Verdict {
    match (now, &plan.name) {
        (None, _) => Verdict::Unchanged,
        (Some((f, _)), Some(n)) if !same_name(f, n) => Verdict::Diverged,
        (Some(_), _) => Verdict::Reached,
    }
}
fn faction_now(process: &dyn Process) -> Result<Option<(String, Vec<String>)>> {
    diplomacy_state::player_faction(process)
}
fn invite_rows(r: &Reader<'_>, image: u64) -> Result<Vec<(String, Rect, bool)>> {
    let window = need(r, image, "invite_countries_window")?;
    let grid = kit::find_deep(r, image, window, "countries_grid", 4)?
        .ok_or_else(|| Error::new("gui_target_unavailable", "countries_grid not found"))?;
    let mut out = Vec::new();
    for item in nui::grid_items(r, grid)?.into_iter().take(120) {
        let Some(frame) = gui_tree::container(r, r.u64(item + 0x10)?, image)? else {
            continue;
        };
        let (Some(name), Some(select)) = (
            kit::find_deep(r, image, frame, "name", 1)?,
            kit::find_deep(r, image, frame, "select_button", 1)?,
        ) else {
            continue;
        };
        let selected = match kit::find_deep(r, image, frame, "selected_icon", 1)? {
            Some(w) => gui_tree::shown(r, w)?,
            None => false,
        };
        out.push((nui::text_at(r, name)?, gui_tree::rect_unclipped(r, image, select, kit::GAME)?, selected));
    }
    Ok(out)
}
fn drive_create(process: &dyn Process, keys: &MacKeys<'_>, plan: &CreatePlan) -> Result<Value> {
    clear_notices(process, keys)?;
    let result = (|| -> Result<Value> {
        open_from_topbar(process, keys, "new_faction_window")?;
        pause(500);
        let mut list = settled(process, "faction templates", templates)?;
        let pairs: Vec<(String, String)> = list.iter().map(|t| (t.name.clone(), t.manifest.clone())).collect();
        let index = pick_template(&pairs, plan.template.as_deref())?;
        for _ in 0..4 {
            if on_screen(list[index].select) {
                break;
            }
            let down = list[index].select.y > 500;
            keys.scroll(kit::GAME, (700.0, 450.0), if down { -240 } else { 240 })?;
            pause(300);
            list = settled(process, "faction templates", templates)?;
        }
        let chosen = list[index].clone();
        if !on_screen(chosen.select) {
            return Err(Error::new("gui_target_unavailable", "The template card could not be scrolled into view"));
        }
        kit::click(keys, chosen.select)?;
        pause(400);
        let (create, state) = settled(process, "create faction button", |r, i| button(r, i, "new_faction_window", "create_faction_button"))?;
        if state == 3 {
            return Err(Error::new("button_disabled", "The game disables the template's Create Faction button"));
        }
        kit::click(keys, create)?;
        wait(process, "faction setup window", |r, i| Ok(open_window(r, i, "faction_setup_window")?.is_some()))?;
        pause(600);
        if let Some(name) = &plan.name {
            let (icon, _) = settled(process, "name box anchor", |r, i| button(r, i, "faction_setup_window", "choose_icon_button"))?;
            design_edit::type_name(process, keys, name_box_point(icon), name, &|r, i| {
                let setup = need(r, i, "faction_setup_window")?;
                let edit = kit::find_deep(r, i, setup, "name_selection", 4)?
                    .ok_or_else(|| Error::new("gui_target_unavailable", "name_selection not found"))?;
                nui::edit_text(r, i, edit)
            })?;
        }
        let mut invited = Vec::new();
        if !plan.invite.is_empty() {
            let (open, _) = settled(process, "invite button", |r, i| button(r, i, "faction_setup_window", "invite_countries"))?;
            kit::click(keys, open)?;
            wait(process, "invite window", |r, i| Ok(open_window(r, i, "invite_countries_window")?.is_some()))?;
            pause(500);
            for wanted in &plan.invite {
                let rows = settled(process, "invite rows", invite_rows)?;
                let Some((name, rect, selected)) = rows.iter().find(|(n, _, _)| same_name(n, wanted)).cloned() else {
                    return Err(Error::new(
                        "invite_country_not_listed",
                        format!("{wanted:?} is not offered for the new faction; offered: {}", rows.iter().map(|r| r.0.as_str()).collect::<Vec<_>>().join(", ")),
                    ));
                };
                if !selected {
                    kit::click(keys, rect)?;
                    let n = name.clone();
                    wait(process, "country selected", move |r, i| {
                        Ok(invite_rows(r, i)?.iter().any(|(x, _, s)| same_name(x, &n) && *s))
                    })?;
                }
                invited.push(name);
            }
            let (send, _) = settled(process, "invite confirm", |r, i| button(r, i, "invite_countries_window", "invite_button"))?;
            kit::click(keys, send)?;
            wait(process, "invite window closed", |r, i| Ok(open_window(r, i, "invite_countries_window")?.is_none()))?;
            pause(300);
        }
        let (finish, state) = settled(process, "create faction", |r, i| button(r, i, "faction_setup_window", "create_faction"))?;
        if state == 3 {
            return Err(Error::new("button_disabled", "The game disables Create Faction (name, template or influence requirements)"));
        }
        kit::click(keys, finish)?;
        let answered = kit::answer_confirmation(process, keys, true)?;
        let deadline = Instant::now() + Duration::from_secs(4);
        let found = loop {
            if let Some(f) = faction_now(process)? {
                break Some(f);
            }
            if Instant::now() >= deadline {
                break None;
            }
            pause(100);
        };
        let Some((faction, members)) = found else {
            return Err(Error::new(
                "input_readback_failed",
                "No faction appeared in memory after Create Faction (the game may need a name, an influence minimum or the 'can create factions' rule); nothing was repeated",
            ));
        };
        Ok(json!({"template":chosen.name,"template_rule":chosen.rule,"template_goal":chosen.goal,"faction":faction,"members":members,
            "invited":invited,"confirmation_answered":answered}))
    })();
    close_windows(process, keys);
    result
}

// ---- leave ----------------------------------------------------------------------------------------
pub(super) fn plan_leave(member_of: Option<&(String, Vec<String>)>) -> Result<String> {
    member_of
        .map(|(f, _)| f.clone())
        .ok_or_else(|| Error::new("not_in_faction", "The player is not in a faction"))
}
pub(super) fn settle_leave(before: Option<&str>, now: Option<&(String, Vec<String>)>) -> Verdict {
    match (now, before) {
        (None, _) => Verdict::Reached,
        (Some((f, _)), Some(b)) if same_name(f, b) => Verdict::Unchanged,
        (Some(_), None) => Verdict::Unchanged,
        (Some(_), Some(_)) => Verdict::Diverged,
    }
}
fn drive_leave(process: &dyn Process, keys: &MacKeys<'_>, before: &str) -> Result<Value> {
    clear_notices(process, keys)?;
    let result = (|| -> Result<Value> {
        open_from_topbar(process, keys, "countryfactionview")?;
        pause(600);
        let (leave, state) = settled(process, "leave faction button", |r, i| button(r, i, "countryfactionview", "leave_faction_button"))?;
        if state == 3 {
            return Err(Error::new("leave_unavailable", "The game disables the leave / dismantle button of the faction view"));
        }
        kit::click(keys, leave)?;
        let answered = kit::answer_confirmation(process, keys, true)?;
        let deadline = Instant::now() + Duration::from_secs(4);
        loop {
            let now = faction_now(process)?;
            if now.is_none() {
                return Ok(json!({"left":before,"confirmation_answered":answered}));
            }
            if Instant::now() >= deadline {
                return Err(Error::new(
                    "input_readback_failed",
                    "The player is still in the faction after the leave button and its confirmation (an unrecognised dialog may be open); nothing was repeated",
                ));
            }
            pause(100);
        }
    })();
    close_windows(process, keys);
    result
}

// ---- incoming proposals ---------------------------------------------------------------------------
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub(super) struct Proposal {
    pub index: usize,
    /// Pop-up window class: standard, scripted, incoming_lend_lease, foreign_manpower, base.
    pub window: String,
    pub title: String,
    pub description: String,
    pub class: String,
    #[serde(skip)]
    pub accept: Option<Rect>,
    #[serde(skip)]
    pub decline: Option<Rect>,
}
fn window_class(kind: u64) -> &'static str {
    match kind {
        0x365b5f8 => "standard",
        0x365b720 => "scripted",
        0x365b848 => "incoming_lend_lease",
        0x365b970 => "foreign_manpower",
        _ => "base",
    }
}
/// What the proposal is, from the game's own wording (localisation keys quoted in the module notes);
/// a heuristic, the title and description are always returned as well.
pub(super) fn classify(window: &str, title: &str, description: &str) -> &'static str {
    let text = format!("{title} {description}").to_lowercase();
    if window == "incoming_lend_lease" {
        "lend_lease_request"
    } else if window == "foreign_manpower" {
        "manpower_request"
    } else if text.contains("join their faction") || text.contains("invite to faction") || text.contains("join the faction") || (text.contains("faction") && text.contains("invit")) {
        "faction_invitation"
    } else if text.contains("guarantee") {
        "guarantee"
    } else if text.contains("non-aggression") {
        "non_aggression_pact"
    } else if text.contains("military access") {
        "military_access"
    } else if text.contains("peace") || text.contains("surrender") {
        "peace_offer"
    } else if text.contains("call to arms") || text.contains("join the war") || text.contains("join our war") {
        "call_to_arms"
    } else {
        "other"
    }
}
fn proposals_in(r: &Reader<'_>, image: u64) -> Result<Vec<Proposal>> {
    let mut out = Vec::new();
    for Popup { kind, root, index } in kit::popups(r, image, &kit::DIPLOMACY_VTABLES)? {
        let text = |name: &str| -> Result<String> {
            match gui_tree::child(r, image, root, name)? {
                Some(w) => kit::long_text(r, w),
                None => Ok(String::new()),
            }
        };
        let rect = |name: &str| -> Option<Rect> {
            let w = gui_tree::child(r, image, root, name).ok()??;
            if gui_tree::shown(r, w).ok()? && gui_tree::button_state(r, w).ok()? != 3 {
                gui_tree::rect(r, image, w, kit::GAME).ok()
            } else {
                None
            }
        };
        let window = window_class(kind).to_string();
        let (title, description) = (text("title")?, text("description")?);
        let class = classify(&window, &title, &description).to_string();
        out.push(Proposal {
            index,
            window,
            title,
            description,
            class,
            accept: rect("accept_button"),
            decline: rect("decline_button"),
        });
    }
    Ok(out)
}
fn proposals_json(list: &[Proposal]) -> Value {
    let rows: Vec<Value> = list
        .iter()
        .map(|p| {
            json!({"index":p.index,"class":p.class,"window":p.window,"title":p.title,"description":p.description,
                "can_accept":p.accept.is_some(),"can_refuse":p.decline.is_some()})
        })
        .collect();
    json!({"proposals":rows,"count":rows.len(),"validated":false,
        "note":"Read from the open diplomacy pop-ups (memory only, nothing is clicked). class is a heuristic from the game's wording; answer with hoi4_diplomacy_incoming {answer:accept|refuse}"})
}
/// Pure read of the open proposals (no lease, no input).
pub(super) fn incoming_read(process: &dyn Process) -> Result<Value> {
    Ok(proposals_json(&read(process, proposals_in)?))
}
/// Choose the proposal an answer applies to: by `index`, by text in the title or description, or
/// the only candidate (restricted to `only_class` when given).
pub(super) fn pick_proposal(list: &[Proposal], which: Option<&str>, index: Option<usize>, only_class: Option<&str>) -> Result<usize> {
    let candidates: Vec<usize> = (0..list.len())
        .filter(|i| only_class.is_none_or(|c| list[*i].class == c))
        .filter(|i| index.is_none_or(|x| list[*i].index == x))
        .filter(|i| {
            which.is_none_or(|w| {
                let w = w.to_lowercase();
                list[*i].title.to_lowercase().contains(&w) || list[*i].description.to_lowercase().contains(&w)
            })
        })
        .collect();
    match candidates.len() {
        1 => Ok(candidates[0]),
        0 => Err(Error::new(
            "no_incoming_proposal",
            match only_class {
                Some(c) => format!("No open {c} proposal matches (open: {})", list.iter().map(|p| p.title.as_str()).collect::<Vec<_>>().join(" | ")),
                None => format!("No open proposal matches (open: {})", list.iter().map(|p| p.title.as_str()).collect::<Vec<_>>().join(" | ")),
            },
        )),
        _ => Err(Error::new(
            "proposal_ambiguous",
            format!(
                "Several proposals match; pass index or which. Candidates: {}",
                candidates.iter().map(|i| format!("{}: {}", list[*i].index, list[*i].title)).collect::<Vec<_>>().join(" | ")
            ),
        )),
    }
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub(super) struct AnswerPlan {
    pub accept: bool,
    pub title: String,
    pub description: String,
    pub class: String,
    pub in_faction_before: bool,
}
/// The proposal is the same one while title and description are equal (the pop-up list shifts).
fn present(list: &[Proposal], plan: &AnswerPlan) -> bool {
    list.iter().any(|p| p.title == plan.title && p.description == plan.description)
}
/// Gone from the list means answered; an accepted faction invitation must also have put the player
/// into a faction (else the proposal lapsed some other way).
pub(super) fn settle_answer(plan: &AnswerPlan, list: &[Proposal], in_faction_now: bool) -> Verdict {
    if present(list, plan) {
        return Verdict::Unchanged;
    }
    if plan.accept && plan.class == "faction_invitation" && !in_faction_now && !plan.in_faction_before {
        return Verdict::Diverged;
    }
    Verdict::Reached
}
fn drive_answer(process: &dyn Process, keys: &MacKeys<'_>, plan: &AnswerPlan) -> Result<Value> {
    // Notices may sit on top of the proposal, but nothing here may touch a diplomacy pop-up:
    // only the OK-only notice classes are dismissed.
    clear_notices(process, keys)?;
    let wanted = plan.clone();
    let rect = settled(process, "proposal button", move |r, i| {
        let list = proposals_in(r, i)?;
        let p = list
            .iter()
            .find(|p| p.title == wanted.title && p.description == wanted.description)
            .ok_or_else(|| Error::new("no_incoming_proposal", "The proposal is no longer open"))?;
        (if wanted.accept { p.accept } else { p.decline })
            .ok_or_else(|| Error::new("button_disabled", "The game does not offer that answer for this proposal"))
    })?;
    kit::click(keys, rect)?;
    let expect = plan.clone();
    wait(process, "proposal answered", move |r, i| Ok(!present(&proposals_in(r, i)?, &expect))).map_err(|_| {
        Error::new("input_readback_failed", "The proposal pop-up stayed open after its button; nothing was repeated")
    })?;
    pause(300);
    let in_faction = faction_now(process)?;
    Ok(json!({"answered":if plan.accept {"accept"} else {"refuse"},"title":plan.title,"class":plan.class,
        "in_faction_now":in_faction.as_ref().map(|f| f.0.clone())}))
}

// ---- dispatch -------------------------------------------------------------------------------------
pub(super) fn call(name: &str, args: &Map<String, Value>, env: Env<'_>) -> Result<Value> {
    match name {
        "hoi4_faction_create" => create(args, env),
        "hoi4_faction_leave" => leave(args, env),
        "hoi4_faction_invite_accept" => answer(args, env, true),
        _ => answer(args, env, false),
    }
}
fn ids(args: &Map<String, Value>) -> Result<(String, String)> {
    Ok((super::string(args, "request_id")?.to_string(), super::string(args, "lease_token")?.to_string()))
}
fn create(args: &Map<String, Value>, env: Env<'_>) -> Result<Value> {
    super::allowed(args, &["template", "name", "invite", "request_id", "lease_token"])?;
    let template = kit::opt_text_arg(args, "template", 64)?;
    let name = kit::opt_text_arg(args, "name", 40)?;
    let invite: Vec<String> = match args.get("invite") {
        None => Vec::new(),
        Some(v) => v
            .as_array()
            .filter(|a| a.len() <= 8)
            .and_then(|a| a.iter().map(|c| c.as_str().filter(|c| !c.trim().is_empty() && c.len() <= 64 && !c.chars().any(char::is_control)).map(|c| c.trim().to_string())).collect())
            .ok_or_else(|| Error::new("invalid_arguments", "invite must list up to 8 country names"))?,
    };
    let (id, token) = ids(args)?;
    let action = json!({"kind":"faction_create","template":template,"name":name,"invite":invite});
    kit::run(
        env,
        kit::Request { id: &id, token: &token, seconds: 240, action },
        &|p| {
            let member = faction_now(p)?;
            plan_create(member.as_ref(), template.clone(), name.clone(), invite.clone())
        },
        &|p, planned| {
            let fallback = CreatePlan { template: template.clone(), name: name.clone(), invite: invite.clone() };
            let plan = planned.unwrap_or(&fallback);
            let now = faction_now(p)?;
            Ok(settle_create(plan, now.as_ref()).outcome(json!({"faction":now.as_ref().map(|f| f.0.clone())})))
        },
        &|p, keys, planned| drive_create(p, keys, planned),
    )
}
fn leave(args: &Map<String, Value>, env: Env<'_>) -> Result<Value> {
    super::allowed(args, &["confirm", "request_id", "lease_token"])?;
    if args.get("confirm").and_then(Value::as_str) != Some("leave_faction") {
        return Err(Error::new(
            "confirmation_required",
            "Leaving a faction ends its alliance obligations (as the faction leader it dismantles the faction). Pass confirm:\"leave_faction\"; nothing was sent",
        ));
    }
    let (id, token) = ids(args)?;
    let action = json!({"kind":"faction_leave"});
    kit::run(
        env,
        kit::Request { id: &id, token: &token, seconds: 120, action },
        &|p| {
            let member = faction_now(p)?;
            let before = plan_leave(member.as_ref())?;
            kit::remember(&id, &before);
            Ok(before)
        },
        &|p, planned| {
            let before = planned.cloned().or_else(|| kit::recall::<String>(&id));
            let now = faction_now(p)?;
            Ok(settle_leave(before.as_deref(), now.as_ref()).outcome(json!({"faction":now.as_ref().map(|f| f.0.clone())})))
        },
        &|p, keys, before| drive_leave(p, keys, before),
    )
}
/// `hoi4_faction_invite_accept` (accept or `decline:true`) and `hoi4_diplomacy_incoming {answer}`.
fn answer(args: &Map<String, Value>, env: Env<'_>, faction_only: bool) -> Result<Value> {
    let (accept, which, index, only_class) = if faction_only {
        super::allowed(args, &["decline", "which", "index", "request_id", "lease_token"])?;
        (
            !args.get("decline").and_then(Value::as_bool).unwrap_or(false),
            kit::opt_text_arg(args, "which", 128)?,
            index_arg(args)?,
            Some("faction_invitation"),
        )
    } else {
        super::allowed(args, &["answer", "which", "index", "request_id", "lease_token"])?;
        let accept = match args.get("answer").and_then(Value::as_str) {
            Some("accept") => true,
            Some("refuse") => false,
            _ => return Err(Error::new("invalid_arguments", "answer must be accept or refuse")),
        };
        (accept, kit::opt_text_arg(args, "which", 128)?, index_arg(args)?, None)
    };
    let (id, token) = ids(args)?;
    let action = json!({"kind":"diplomacy_incoming","accept":accept,"which":which,"index":index,"only_class":only_class});
    kit::run(
        env,
        kit::Request { id: &id, token: &token, seconds: 90, action },
        &|p| {
            let list = read(p, proposals_in)?;
            let pick = pick_proposal(&list, which.as_deref(), index, only_class)?;
            let chosen = &list[pick];
            if (accept && chosen.accept.is_none()) || (!accept && chosen.decline.is_none()) {
                return Err(Error::new("button_disabled", "The game does not offer that answer for this proposal"));
            }
            let planned = AnswerPlan {
                accept,
                title: chosen.title.clone(),
                description: chosen.description.clone(),
                class: chosen.class.clone(),
                in_faction_before: faction_now(p)?.is_some(),
            };
            kit::remember(&id, &planned);
            Ok(planned)
        },
        &|p, planned| {
            let Some(plan) = planned.cloned().or_else(|| kit::recall::<AnswerPlan>(&id)) else {
                return Ok(Outcome::Unknown);
            };
            let list = read(p, proposals_in)?;
            let member = faction_now(p)?.is_some();
            Ok(settle_answer(&plan, &list, member).outcome(json!({"open_proposals":list.len()})))
        },
        &|p, keys, planned| drive_answer(p, keys, planned),
    )
}
fn index_arg(args: &Map<String, Value>) -> Result<Option<usize>> {
    match args.get("index") {
        None => Ok(None),
        Some(v) => v
            .as_u64()
            .filter(|n| *n < 32)
            .map(|n| Some(n as usize))
            .ok_or_else(|| Error::new("invalid_arguments", "index must be 0..31 (the index shown by hoi4_diplomacy_incoming)")),
    }
}

// ---- tool list ------------------------------------------------------------------------------------
pub(super) fn tools() -> Vec<Value> {
    let lease = json!({"request_id":{"type":"string"},"lease_token":{"type":"string"}});
    let with = |extra: Value| -> Value {
        let mut m = extra.as_object().cloned().unwrap_or_default();
        m.extend(lease.as_object().cloned().unwrap_or_default());
        Value::Object(m)
    };
    vec![
        json!({"name":"hoi4_faction_create","description":format!("{}Create a faction through the ordinary windows: the top bar faction button (ctrl+Q) opens the New Faction window with its templates (name, manifest, rule, goal), the chosen template's card, Create Faction, then the setup window: the name typed key by key into the name box (each key verified), optional `invite` country names picked in the Invite Countries window, and Create Faction. Refused with already_in_faction (read from memory) before any input; capability_unavailable when the top bar shows no faction button (the 'can create factions' rule is missing); template_required / template_unknown (offered templates listed, after the window was opened and closed again), invite_country_not_listed, button_disabled. Verified by the player being in a faction (named as asked) in memory. Windows are closed afterwards. Result has validated:false.", kit::UNVALIDATED),
            "inputSchema":{"type":"object","properties":with(json!({"template":{"type":"string","maxLength":64},"name":{"type":"string","maxLength":40},"invite":{"type":"array","items":{"type":"string"},"maxItems":8}})),"required":["request_id","lease_token"],"additionalProperties":false}}),
        json!({"name":"hoi4_faction_leave","description":format!("{}Leave the player's faction (as the leader this dismantles it) with the faction view's leave button (top bar faction button, leave_faction_button) and the game's confirmation. Needs confirm:\"leave_faction\" (else confirmation_required, nothing sent); not_in_faction before any input. Verified by the player no longer being in a faction. Result has validated:false.", kit::UNVALIDATED),
            "inputSchema":{"type":"object","properties":with(json!({"confirm":{"type":"string","enum":["leave_faction"]}})),"required":["confirm","request_id","lease_token"],"additionalProperties":false}}),
        json!({"name":"hoi4_faction_invite_accept","description":format!("{}Answer the open faction invitation pop-up ('<country> wants us to join their faction'): accept by default, `decline:true` refuses. Uses the pop-up's own Accept / Decline buttons; only a faction_invitation proposal is touched (no_incoming_proposal otherwise; see hoi4_diplomacy_incoming). Verified by the pop-up closing and, when accepted, the player being in a faction. Result has validated:false.", kit::UNVALIDATED),
            "inputSchema":{"type":"object","properties":with(json!({"decline":{"type":"boolean"},"which":{"type":"string","maxLength":128},"index":{"type":"integer","minimum":0,"maximum":31}})),"required":["request_id","lease_token"],"additionalProperties":false}}),
        json!({"name":INCOMING,"description":format!("{}Incoming diplomatic proposals (guarantee, alliance or faction invitation, lend-lease request, call to arms, ...) as open pop-ups. Without `answer` it is a pure read (memory only, no lease, nothing clicked): index, class (heuristic), title, description, whether Accept/Refuse are offered. With answer:\"accept\"|\"refuse\" (plus request_id and lease_token) it clicks that answer on ONE proposal chosen by `index`, by text in `which`, or the only open one (proposal_ambiguous / no_incoming_proposal otherwise). Verified by the pop-up closing. Result has validated:false.", kit::UNVALIDATED),
            "inputSchema":{"type":"object","properties":{"answer":{"type":"string","enum":["accept","refuse"]},"which":{"type":"string","maxLength":128},"index":{"type":"integer","minimum":0,"maximum":31},"request_id":{"type":"string"},"lease_token":{"type":"string"}},"additionalProperties":false}}),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn member(name: &str) -> Option<(String, Vec<String>)> {
        Some((name.to_string(), vec!["GER".into(), "ITA".into()]))
    }
    fn proposal(index: usize, window: &str, title: &str, description: &str) -> Proposal {
        Proposal {
            index,
            window: window.into(),
            title: title.into(),
            description: description.into(),
            class: classify(window, title, description).into(),
            accept: Some(Rect { x: 1, y: 1, w: 2, h: 2 }),
            decline: Some(Rect { x: 5, y: 1, w: 2, h: 2 }),
        }
    }
    #[test]
    fn create_refuses_inside_a_faction_and_untypable_names() {
        let m = member("Axis");
        assert_eq!(plan_create(m.as_ref(), None, None, vec![]).unwrap_err().code, "already_in_faction");
        assert_eq!(plan_create(None, None, Some("Pakt\u{e4}".into()), vec![]).unwrap_err().code, "name_not_typable");
        let p = plan_create(None, Some("Fascist".into()), Some("Stahlpakt".into()), vec!["Italy".into()]).unwrap();
        assert_eq!(p.name.as_deref(), Some("Stahlpakt"));
    }
    #[test]
    fn create_and_leave_settlement() {
        let p = CreatePlan { template: None, name: Some("Stahlpakt".into()), invite: vec![] };
        assert_eq!(settle_create(&p, None), Verdict::Unchanged);
        assert_eq!(settle_create(&p, member("stahlpakt").as_ref()), Verdict::Reached);
        assert_eq!(settle_create(&p, member("Other").as_ref()), Verdict::Diverged);
        let any = CreatePlan { template: None, name: None, invite: vec![] };
        assert_eq!(settle_create(&any, member("Whatever").as_ref()), Verdict::Reached);

        assert_eq!(plan_leave(member("Axis").as_ref()).unwrap(), "Axis");
        assert_eq!(plan_leave(None).unwrap_err().code, "not_in_faction");
        assert_eq!(settle_leave(Some("Axis"), None), Verdict::Reached);
        assert_eq!(settle_leave(Some("Axis"), member("axis").as_ref()), Verdict::Unchanged);
        assert_eq!(settle_leave(Some("Axis"), member("Comintern").as_ref()), Verdict::Diverged);
        assert_eq!(settle_leave(None, member("Axis").as_ref()), Verdict::Unchanged);
    }
    #[test]
    fn templates_are_picked_by_name_or_manifest() {
        let list = vec![("Alliance".to_string(), "Pact of Steel".to_string()), ("Bloc".to_string(), String::new())];
        assert_eq!(pick_template(&list, Some("bloc")).unwrap(), 1);
        assert_eq!(pick_template(&list, Some("pact of steel")).unwrap(), 0);
        assert_eq!(pick_template(&list, None).unwrap_err().code, "template_required");
        assert_eq!(pick_template(&list[..1], None).unwrap(), 0);
        let e = pick_template(&list, Some("zzz")).unwrap_err();
        assert_eq!(e.code, "template_unknown");
        assert!(e.message.contains("Alliance (Pact of Steel)") && e.message.contains("Bloc"));
        let twice = vec![("A".to_string(), "x".to_string()), ("a".to_string(), "y".to_string())];
        assert_eq!(pick_template(&twice, Some("a")).unwrap_err().code, "template_ambiguous");
    }
    #[test]
    fn classifies_by_the_games_wording() {
        let invite = classify("standard", "Invite to faction", "Italy wants us to join their faction: Axis.");
        assert_eq!(invite, "faction_invitation");
        assert_eq!(classify("standard", "Guarantee independence", "Poland offers a guarantee"), "guarantee");
        assert_eq!(classify("incoming_lend_lease", "x", "y"), "lend_lease_request");
        assert_eq!(classify("standard", "Non-aggression pact", "..."), "non_aggression_pact");
        assert_eq!(classify("standard", "Call to arms", "France asks"), "call_to_arms");
        assert_eq!(classify("standard", "Hello", "world"), "other");
    }
    #[test]
    fn proposals_are_picked_unambiguously() {
        let list = vec![
            proposal(0, "standard", "Invite to faction", "Italy wants us to join their faction: Axis."),
            proposal(1, "standard", "Guarantee independence", "Hungary offers a guarantee"),
            proposal(2, "standard", "Invite to faction", "Japan wants us to join their faction: Co-Prosperity."),
        ];
        assert_eq!(pick_proposal(&list, Some("hungary"), None, None).unwrap(), 1);
        assert_eq!(pick_proposal(&list, None, Some(2), None).unwrap(), 2);
        assert_eq!(pick_proposal(&list, None, None, None).unwrap_err().code, "proposal_ambiguous");
        assert_eq!(pick_proposal(&list, Some("japan"), None, Some("faction_invitation")).unwrap(), 2);
        assert_eq!(pick_proposal(&list, None, None, Some("faction_invitation")).unwrap_err().code, "proposal_ambiguous");
        assert_eq!(pick_proposal(&list, Some("zzz"), None, None).unwrap_err().code, "no_incoming_proposal");
        assert_eq!(pick_proposal(&[], None, None, None).unwrap_err().code, "no_incoming_proposal");
    }
    #[test]
    fn answer_settlement() {
        let list = vec![proposal(0, "standard", "Invite to faction", "Italy wants us to join their faction: Axis.")];
        let accept = AnswerPlan { accept: true, title: list[0].title.clone(), description: list[0].description.clone(), class: "faction_invitation".into(), in_faction_before: false };
        assert_eq!(settle_answer(&accept, &list, false), Verdict::Unchanged);
        assert_eq!(settle_answer(&accept, &[], true), Verdict::Reached);
        assert_eq!(settle_answer(&accept, &[], false), Verdict::Diverged);
        let refuse = AnswerPlan { accept: false, ..accept.clone() };
        assert_eq!(settle_answer(&refuse, &[], false), Verdict::Reached);
        let other = AnswerPlan { class: "guarantee".into(), ..accept };
        assert_eq!(settle_answer(&other, &[], false), Verdict::Reached);
    }
    #[test]
    fn proposals_json_lists_buttons_and_text() {
        let mut p = proposal(0, "standard", "T", "D");
        p.decline = None;
        let v = proposals_json(&[p]);
        assert_eq!(v["count"], 1);
        assert_eq!(v["proposals"][0]["can_accept"], true);
        assert_eq!(v["proposals"][0]["can_refuse"], false);
        assert_eq!(v["validated"], false);
    }
    #[test]
    fn name_box_sits_right_of_the_icon_button() {
        let icon = Rect { x: 100, y: 400, w: 40, h: 40 };
        assert_eq!(name_box_point(icon), (342.0, 421.0));
        assert!(on_screen(Rect { x: 10, y: 300, w: 197, h: 300 }));
        assert!(!on_screen(Rect { x: 10, y: 900, w: 197, h: 300 }));
    }
    #[test]
    fn tool_list_is_marked_unvalidated_and_incoming_needs_no_lease() {
        let list = tools();
        assert_eq!(list.len(), TOOLS.len() + 1);
        for entry in &list {
            assert!(entry["description"].as_str().unwrap().starts_with("UNVALIDATED (no live test yet): "));
        }
        let incoming = list.iter().find(|t| t["name"] == INCOMING).unwrap();
        assert!(incoming["inputSchema"].get("required").is_none());
        assert!(handles("hoi4_faction_leave") && handles(INCOMING) && !handles("hoi4_fleet_merge"));
        assert_eq!(index_arg(json!({"index":3}).as_object().unwrap()).unwrap(), Some(3));
        assert_eq!(index_arg(json!({"index":99}).as_object().unwrap()).unwrap_err().code, "invalid_arguments");
    }
}
