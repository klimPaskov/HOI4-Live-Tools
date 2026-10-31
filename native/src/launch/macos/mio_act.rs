//! Military-industrial organisation actions with ordinary input:
//! `hoi4_mio_design_team`, `hoi4_mio_policy`, `hoi4_mio_trait`. UNVALIDATED:
//! written from `interface/military_industrial_organization/*.gui` and the
//! `industrial_org_entry` window of `countrypoliticsview.gui`; no live run has
//! located any of these windows, and the earlier read found no runtime
//! organisation object, so the system may simply be absent in the running
//! game (the Arms Against Tyranny DLC).
//!
//! Absence is decided from the screen: the politics view (Q) must hold at
//! least one shown `industrial_org_entry` window (the design-team slot of the
//! organisation system). Without one every tool refuses with
//! `mio_system_absent` after opening nothing but the politics view; use
//! `hoi4_politics_choose` for the designer ideas of that case.
//!
//! Route: the entry's `open_roster_button` opens the organisation list; a row
//! (`name`, `assign_cost`, `background_button`, `details_button`) is chosen by
//! its localised name; the row's details button opens the detail window whose
//! `open_policies` button opens the policy list and whose trait tree holds the
//! `industrial_organisation_trait_entry` windows (`icon_frame` button,
//! `queue_number` text). Nothing about an organisation is readable from memory,
//! so successes are read back from the windows (verification `gui_only`).
use super::{
    country_status, loc, mios, mgmt, panelkit as pk, script,
    ui::{accept_command, clear_notices, open_politics, read, settled, wait},
    ui_action::{self, refuse, Driven, Fail, Job},
    Guard, InputAuthority,
};
use crate::{
    error::{Error, Result},
    gui_tree,
    memory::Reader,
    platform::{macos_input::MacKeys, Process},
};
use serde_json::{json, Value};
use std::{cell::RefCell, thread, time::Duration};

pub(super) const TOOLS: [&str; 3] = ["hoi4_mio_design_team", "hoi4_mio_policy", "hoi4_mio_trait"];

fn pause(ms: u64) {
    thread::sleep(Duration::from_millis(ms));
}

// ----------------------------------------------------------------- planning

/// Everything the request names, resolved against the install's scripts.
#[derive(Clone, Debug, PartialEq)]
pub(super) struct Names {
    pub organization: String,
    pub org_display: String,
    pub policy: Option<(String, String)>,
}

/// Organisation script lookup with the tag verdict; pure.
pub(super) fn plan_org(catalog: &mios::Catalog, key: &str, tag: &str) -> Result<()> {
    let Some((_org, allowed)) = catalog.orgs.iter().find(|(o, _)| o.key == key) else {
        let mut near: Vec<&str> = catalog
            .orgs
            .iter()
            .map(|(o, _)| o.key.as_str())
            .filter(|k| k.contains(key) || key.contains(*k) || k.starts_with(&format!("{tag}_")))
            .take(8)
            .collect();
        near.sort_unstable();
        return Err(Error::new(
            "organization_unknown",
            format!(
                "No organisation {key:?} in the install's scripts (hoi4_mios lists them){}",
                if near.is_empty() {
                    String::new()
                } else {
                    format!("; similar: {}", near.join(", "))
                }
            ),
        ));
    };
    if mios::eval(allowed, tag) == Some(false) {
        return Err(Error::new(
            "organization_not_allowed",
            format!("{key} is not available to {tag} (its allowed block rules the tag out)"),
        ));
    }
    Ok(())
}
/// Policy keys of the install's policy scripts.
pub(super) fn policy_keys(entries: &[script::Entry]) -> Vec<String> {
    entries
        .iter()
        .filter(|e| e.key.starts_with("mio_policy_") && matches!(e.val, script::Val::Block(_)))
        .map(|e| e.key.clone())
        .collect()
}
pub(super) fn plan_policy(policies: &[String], key: &str) -> Result<()> {
    if policies.iter().any(|p| p == key) {
        return Ok(());
    }
    let near: Vec<&str> = policies
        .iter()
        .map(String::as_str)
        .filter(|p| p.contains(key) || key.contains(*p))
        .take(8)
        .collect();
    Err(Error::new(
        "policy_unknown",
        format!(
            "No organisation policy {key:?}{}",
            if near.is_empty() {
                format!("; {} policies exist, e.g. {}", policies.len(), policies.iter().take(6).cloned().collect::<Vec<_>>().join(", "))
            } else {
                format!("; similar: {}", near.join(", "))
            }
        ),
    ))
}

fn names(process: &dyn Process, organization: &str, policy: Option<&str>) -> Result<Names> {
    let dir = mgmt::game_dir(process)?;
    let catalog = mios::catalog(&dir);
    if catalog.orgs.is_empty() {
        return Err(Error::new(
            "catalog_unavailable",
            "The install's organisation scripts could not be read",
        ));
    }
    let tag = read(process, |r, i| {
        Ok(mgmt::clock(r, i)?["player"].as_str().unwrap_or("").to_string())
    })?;
    plan_org(&catalog, organization, &tag)?;
    let mut keys = vec![organization];
    let policy_checked = match policy {
        Some(p) => {
            let policies = policy_keys(&script::parse_dir(
                &dir.join("common/military_industrial_organization/policies"),
            ));
            plan_policy(&policies, p)?;
            keys.push(p);
            Some(p)
        }
        None => None,
    };
    let map = loc::lookup(&dir, &keys);
    let org_display = map.get(organization).cloned().ok_or_else(|| {
        Error::new(
            "display_name_unknown",
            format!("No English name for {organization} in the install's localisation"),
        )
    })?;
    let policy = match policy_checked {
        Some(p) => Some((
            p.to_string(),
            map.get(p).cloned().ok_or_else(|| {
                Error::new("display_name_unknown", format!("No English name for {p}"))
            })?,
        )),
        None => None,
    };
    Ok(Names {
        organization: organization.to_string(),
        org_display,
        policy,
    })
}

// -------------------------------------------------------------- the windows

/// Every widget named `wanted` below `object`, also inside grid items.
fn deep_all(
    r: &Reader<'_>,
    image: u64,
    object: u64,
    wanted: &str,
    depth: u32,
    budget: &mut u32,
    out: &mut Vec<u64>,
) -> Result<()> {
    for child in gui_tree::children(r, object)? {
        if *budget == 0 {
            return Ok(());
        }
        *budget -= 1;
        if gui_tree::name(r, child, image)?.as_deref() == Some(wanted) {
            out.push(child);
            continue;
        }
        if depth == 0 {
            continue;
        }
        if let Some(inner) = gui_tree::container(r, child, image)? {
            deep_all(r, image, inner, wanted, depth - 1, budget, out)?;
        } else {
            let mut grids = Vec::new();
            if r.u64(child)? == image + 0x36cca80 {
                grids.push(child);
            }
            for grid in grids {
                for item in pk::items(r, image, grid)? {
                    deep_all(r, image, item.frame, wanted, depth - 1, budget, out)?;
                }
            }
        }
    }
    Ok(())
}
fn all_named(r: &Reader<'_>, image: u64, root: u64, wanted: &str, depth: u32) -> Result<Vec<u64>> {
    let mut out = Vec::new();
    let mut budget = 6000;
    deep_all(r, image, root, wanted, depth, &mut budget, &mut out)?;
    Ok(out)
}

/// Shown `industrial_org_entry` windows of the politics view (container objects).
fn org_entries(r: &Reader<'_>, i: u64) -> Result<Vec<u64>> {
    let root = pk::open_root(r, i, &pk::POLITICS_VIEW)?;
    let mut out = Vec::new();
    for base in all_named(r, i, root, "industrial_org_entry", 8)? {
        if gui_tree::shown(r, base)? {
            if let Some(object) = gui_tree::container(r, base, i)? {
                out.push(object);
            }
        }
    }
    Ok(out)
}

/// The system is present when the politics view holds a shown entry.
pub(super) fn absent_refusal(entries: usize) -> Option<(&'static str, String)> {
    if entries == 0 {
        return Some((
            "mio_system_absent",
            "The politics view shows no military-industrial organisation slot (the organisation system is not in use in this game); designers are ideas, see hoi4_politics_choose".into(),
        ));
    }
    None
}
fn refusal(r: Option<(&'static str, String)>) -> std::result::Result<(), Fail> {
    match r {
        Some((code, message)) => Err(refuse(code, message)),
        None => Ok(()),
    }
}

fn open_system(process: &dyn Process, keys: &MacKeys<'_>, slot: usize) -> std::result::Result<(), Fail> {
    clear_notices(process, keys)?;
    if !pk::open_now(process, &pk::POLITICS_VIEW) {
        open_politics(process, keys, |r, i| pk::is_open(r, i, &pk::POLITICS_VIEW))?;
        pause(400);
    }
    let entries = read(process, org_entries)?;
    refusal(absent_refusal(entries.len()))?;
    if slot >= entries.len() {
        return Err(refuse(
            "slot_unknown",
            format!("The politics view shows {} organisation slots, not {}", entries.len(), slot + 1),
        ));
    }
    Ok(())
}

fn open_roster(process: &dyn Process, keys: &MacKeys<'_>, slot: usize) -> Result<()> {
    pk::click(process, keys, "organisation slot", |r, i| {
        let entry = *org_entries(r, i)?
            .get(slot)
            .ok_or_else(|| Error::new("slot_unknown", "Organisation slot left the politics view"))?;
        pk::rect_of(r, i, entry, "open_roster_button", 2)
    })?;
    wait(process, "open organisation list", |r, i| pk::is_open(r, i, &pk::MIO_LIST))?;
    pause(400);
    Ok(())
}

/// Row windows of the organisation list (grid items, else child windows).
fn list_frames(r: &Reader<'_>, i: u64) -> Result<Vec<u64>> {
    let root = pk::open_root(r, i, &pk::MIO_LIST)?;
    let list = pk::need(r, i, root, "organisation_list", 4)?;
    if let Ok(items) = pk::items(r, i, list) {
        if !items.is_empty() {
            return Ok(items.into_iter().map(|x| x.frame).collect());
        }
    }
    let mut out = Vec::new();
    if let Some(object) = gui_tree::container(r, list, i)? {
        for child in gui_tree::children(r, object)? {
            if let Some(inner) = gui_tree::container(r, child, i)? {
                out.push(inner);
            }
        }
    }
    Ok(out)
}
/// (names on screen, index of the row with this display name).
fn find_row(r: &Reader<'_>, i: u64, display: &str) -> Result<(Vec<String>, Option<usize>)> {
    let mut seen = Vec::new();
    let mut found = None;
    for (n, frame) in list_frames(r, i)?.into_iter().enumerate() {
        let name = pk::text(r, i, frame, "name", 2)?.unwrap_or_default();
        if found.is_none() && loc::same(&name, display) {
            found = Some(n);
        }
        seen.push(name);
    }
    Ok((seen, found))
}
fn row_rect(r: &Reader<'_>, i: u64, display: &str, button: &str) -> Result<gui_tree::Rect> {
    let (_, found) = find_row(r, i, display)?;
    let index = found.ok_or_else(|| Error::new("gui_target_unavailable", "Organisation row not found"))?;
    let frame = list_frames(r, i)?[index];
    pk::rect_of(r, i, frame, button, 2)
}
fn require_row(process: &dyn Process, display: &str) -> std::result::Result<(), Fail> {
    let (seen, found) = settled(process, "organisation rows", |r, i| find_row(r, i, display))?;
    if found.is_none() {
        return Err(refuse(
            "organization_not_offered",
            format!(
                "The organisation list shows {} but not {display:?}; nothing was changed",
                if seen.is_empty() { "no rows".to_string() } else { seen.join(" | ") }
            ),
        ));
    }
    Ok(())
}

fn open_detail(process: &dyn Process, keys: &MacKeys<'_>, display: &str) -> std::result::Result<(), Fail> {
    require_row(process, display)?;
    pk::click(process, keys, "organisation details", |r, i| row_rect(r, i, display, "details_button"))?;
    wait(process, "open organisation detail window", |r, i| pk::is_open(r, i, &pk::MIO_DETAIL))?;
    pause(400);
    Ok(())
}

fn leave(process: &dyn Process, keys: &MacKeys<'_>) {
    for class in [&pk::MIO_POLICY, &pk::MIO_DETAIL, &pk::MIO_LIST] {
        if pk::open_now(process, class) {
            let clicked = pk::click(process, keys, "close window", |r, i| {
                let root = pk::open_root(r, i, class)?;
                pk::rect_of(r, i, root, "close_button", 4)
            });
            if clicked.is_err() {
                pk::dismiss_window(process, keys, class);
            } else {
                let _ = wait(process, "window closed", |r, i| Ok(!pk::is_open(r, i, class)?));
            }
        }
    }
    pk::dismiss_window(process, keys, &pk::POLITICS_VIEW);
}

fn political_power(process: &dyn Process) -> Option<f64> {
    country_status::status(process)
        .ok()
        .and_then(|v| v["fields"]["political_power"]["value"].as_f64())
}

// ------------------------------------------------------------- design team

fn design_team(process: &dyn Process, keys: &MacKeys<'_>, display: &str, slot: usize) -> Driven {
    let result: Driven = (|| {
        open_system(process, keys, slot)?;
        open_roster(process, keys, slot)?;
        require_row(process, display)?;
        let before = political_power(process);
        pk::click(process, keys, "organisation row", |r, i| row_rect(r, i, display, "background_button"))?;
        let confirmed = accept_command(process, keys)?;
        pause(600);
        let closed = !pk::open_now(process, &pk::MIO_LIST);
        let after = political_power(process);
        Ok(json!({
            "assigned": closed,
            "list_closed": closed,
            "political_power_before": before,
            "political_power_after": after,
            "confirmation_accepted": confirmed,
            "verification": "gui_only",
        }))
    })();
    leave(process, keys);
    result
}

// ------------------------------------------------------------------ policy

fn policy(process: &dyn Process, keys: &MacKeys<'_>, display: &str, slot: usize, policy_name: &str) -> Driven {
    let result: Driven = (|| {
        open_system(process, keys, slot)?;
        open_roster(process, keys, slot)?;
        open_detail(process, keys, display)?;
        pk::click(process, keys, "open policies", |r, i| {
            let root = pk::open_root(r, i, &pk::MIO_DETAIL)?;
            pk::rect_of(r, i, root, "open_policies", 4)
        })?;
        wait(process, "open policy window", |r, i| pk::is_open(r, i, &pk::MIO_POLICY))?;
        pause(400);
        let wanted = policy_name.to_string();
        let rows = |r: &Reader<'_>, i: u64| -> Result<(Vec<String>, Option<gui_tree::Rect>)> {
            let root = pk::open_root(r, i, &pk::MIO_POLICY)?;
            let list = pk::need(r, i, root, "policy_list", 4)?;
            let frames: Vec<u64> = match pk::items(r, i, list) {
                Ok(items) if !items.is_empty() => items.into_iter().map(|x| x.frame).collect(),
                _ => match gui_tree::container(r, list, i)? {
                    Some(object) => gui_tree::children(r, object)?
                        .into_iter()
                        .filter_map(|c| gui_tree::container(r, c, i).ok().flatten())
                        .collect(),
                    None => Vec::new(),
                },
            };
            let mut seen = Vec::new();
            let mut rect = None;
            for frame in frames {
                let name = pk::text(r, i, frame, "name", 2)?.unwrap_or_default();
                if rect.is_none() && loc::same(&name, &wanted) {
                    rect = pk::rect_of(r, i, frame, "background_button", 2).ok();
                }
                seen.push(name);
            }
            Ok((seen, rect))
        };
        let (seen, rect) = settled(process, "policy rows", rows)?;
        let Some(rect) = rect else {
            return Err(refuse(
                "policy_not_offered",
                format!(
                    "The policy window shows {} but not {policy_name:?} (equipment type or organisation size rule it out); nothing was changed",
                    if seen.is_empty() { "no rows".to_string() } else { seen.join(" | ") }
                ),
            ));
        };
        let (x, y) = rect.center();
        keys.click(pk::GAME, x, y)?;
        let confirmed = accept_command(process, keys)?;
        pause(600);
        let attached = read(process, |r, i| {
            let root = pk::open_root(r, i, &pk::MIO_DETAIL)?;
            pk::text(r, i, root, "attached_policy_name", 4)
        })
        .ok()
        .flatten();
        let ok = attached.as_deref().is_some_and(|a| loc::same(a, &wanted));
        Ok(json!({
            "policy_attached": ok,
            "attached_policy_text": attached,
            "confirmation_accepted": confirmed,
            "verification": "gui_only",
        }))
    })();
    leave(process, keys);
    result
}

// ------------------------------------------------------------------- trait

/// One entry of the trait tree as the window shows it.
#[derive(Clone, Debug, PartialEq)]
pub(super) struct TraitEntry {
    pub index: usize,
    pub enabled: bool,
    pub queue: String,
    pub rect: Option<gui_tree::Rect>,
}
fn trait_entries(r: &Reader<'_>, i: u64) -> Result<Vec<TraitEntry>> {
    let root = pk::open_root(r, i, &pk::MIO_DETAIL)?;
    let mut out = Vec::new();
    for (index, base) in all_named(r, i, root, "industrial_organisation_trait_entry", 10)?
        .into_iter()
        .enumerate()
    {
        let Some(entry) = gui_tree::container(r, base, i)? else {
            continue;
        };
        let icon = pk::find(r, i, entry, "icon_frame", 4)?;
        let enabled = match icon {
            Some(b) => gui_tree::shown(r, b)? && !gui_tree::disabled(r, b)?,
            None => false,
        };
        let queue = pk::text(r, i, entry, "queue_number", 4)?.unwrap_or_default();
        let rect = icon.and_then(|b| pk::rect(r, i, b).ok());
        out.push(TraitEntry {
            index,
            enabled,
            queue,
            rect,
        });
    }
    Ok(out)
}
/// Refusal for a trait click, from the entries on screen; pure.
pub(super) fn trait_refusal(entries: &[TraitEntry], index: usize) -> Option<(&'static str, String)> {
    let Some(e) = entries.get(index) else {
        return Some((
            "trait_unknown",
            format!("The trait tree shows {} entries, not {}", entries.len(), index + 1),
        ));
    };
    if !e.queue.is_empty() {
        return Some((
            "trait_already_queued",
            format!("Trait {index} is already queued (position {})", e.queue),
        ));
    }
    if !e.enabled {
        return Some((
            "trait_unavailable",
            format!("The game disables trait {index} (prerequisites, funds or an excluded sibling)"),
        ));
    }
    if e.rect.is_none() {
        return Some((
            "trait_not_visible",
            format!("Trait {index} is scrolled out of the tree window"),
        ));
    }
    None
}

fn mio_trait(process: &dyn Process, keys: &MacKeys<'_>, display: &str, slot: usize, index: Option<usize>) -> Driven {
    let result: Driven = (|| {
        open_system(process, keys, slot)?;
        open_roster(process, keys, slot)?;
        open_detail(process, keys, display)?;
        let entries = settled(process, "trait entries", trait_entries)?;
        let summary = |entries: &[TraitEntry]| -> Vec<Value> {
            entries
                .iter()
                .map(|e| json!({"index": e.index, "enabled": e.enabled, "queue": e.queue, "visible": e.rect.is_some()}))
                .collect()
        };
        let Some(index) = index else {
            return Ok(json!({"listed": summary(&entries), "trait_queued": false, "listing_only": true, "verification": "gui_only"}));
        };
        refusal(trait_refusal(&entries, index))?;
        pk::click(process, keys, "trait icon", |r, i| {
            trait_entries(r, i)?
                .get(index)
                .and_then(|e| e.rect)
                .ok_or_else(|| Error::new("gui_target_unavailable", "Trait icon left the window"))
        })?;
        let confirmed = accept_command(process, keys)?;
        pause(600);
        let after = read(process, trait_entries).unwrap_or_default();
        let queued = after.get(index).is_some_and(|e| !e.queue.is_empty());
        Ok(json!({
            "trait_queued": queued,
            "queue_text": after.get(index).map(|e| e.queue.clone()),
            "confirmation_accepted": confirmed,
            "entries": summary(&after),
            "verification": "gui_only",
        }))
    })();
    leave(process, keys);
    result
}

// ------------------------------------------------------------------- tools

fn slot_arg(args: &serde_json::Map<String, Value>) -> Result<usize> {
    match args.get("slot") {
        None => Ok(0),
        Some(v) => v
            .as_u64()
            .filter(|n| *n <= 15)
            .map(|n| n as usize)
            .ok_or_else(|| Error::new("invalid_arguments", "slot must be 0..15")),
    }
}
fn key_arg<'a>(args: &'a serde_json::Map<String, Value>, name: &str) -> Result<&'a str> {
    let v = super::string(args, name)?;
    if v.is_empty() || v.len() > 128 || !v.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_') {
        return Err(Error::new("invalid_arguments", format!("Invalid {name} key")));
    }
    Ok(v)
}

pub(super) fn call(
    name: &str,
    args: &serde_json::Map<String, Value>,
    process: &(dyn Process + Send + Sync),
    keys: &MacKeys<'static>,
    guard: &mut Guard,
    authority: &InputAuthority,
    actions: &mut super::journal::GameJournal,
) -> Result<Value> {
    let id = super::string(args, "request_id")?;
    let token = super::string(args, "lease_token")?;
    let resolved: RefCell<Option<Names>> = RefCell::new(None);
    let need = || -> std::result::Result<Names, Fail> {
        resolved
            .borrow()
            .clone()
            .ok_or_else(|| Fail::Failed(Error::new("action_uncertain", "Plan missing at dispatch")))
    };
    match name {
        "hoi4_mio_design_team" => {
            super::allowed(args, &["organization", "slot", "request_id", "lease_token"])?;
            let organization = key_arg(args, "organization")?;
            let slot = slot_arg(args)?;
            let job = Job {
                kind: "mio_design_team",
                action: json!({"kind":"mio_design_team","organization":organization,"slot":slot}),
                plan: Box::new(|process| {
                    *resolved.borrow_mut() = Some(names(process, organization, None)?);
                    Ok(())
                }),
                dispatch: Box::new(|process, keys| {
                    let n = need()?;
                    design_team(process, keys, &n.org_display, slot)
                }),
                settle: Box::new(|_, seen| {
                    Ok(ui_action::effect(seen, "assigned", json!({"tool":"mio_design_team","organization":organization})))
                }),
            };
            ui_action::run(job, id, token, process, keys, guard, authority, actions)
        }
        "hoi4_mio_policy" => {
            super::allowed(args, &["organization", "policy", "slot", "request_id", "lease_token"])?;
            let organization = key_arg(args, "organization")?;
            let policy_key = key_arg(args, "policy")?;
            let slot = slot_arg(args)?;
            let job = Job {
                kind: "mio_policy",
                action: json!({"kind":"mio_policy","organization":organization,"policy":policy_key,"slot":slot}),
                plan: Box::new(|process| {
                    *resolved.borrow_mut() = Some(names(process, organization, Some(policy_key))?);
                    Ok(())
                }),
                dispatch: Box::new(|process, keys| {
                    let n = need()?;
                    let shown = n.policy.as_ref().map(|p| p.1.clone()).unwrap_or_default();
                    policy(process, keys, &n.org_display, slot, &shown)
                }),
                settle: Box::new(|_, seen| {
                    Ok(ui_action::effect(seen, "policy_attached", json!({"tool":"mio_policy","organization":organization,"policy":policy_key})))
                }),
            };
            ui_action::run(job, id, token, process, keys, guard, authority, actions)
        }
        "hoi4_mio_trait" => {
            super::allowed(args, &["organization", "trait_index", "list", "slot", "request_id", "lease_token"])?;
            let organization = key_arg(args, "organization")?;
            let slot = slot_arg(args)?;
            let list_only = mgmt::flag(args, "list")?;
            let index = match args.get("trait_index") {
                None => None,
                Some(v) => Some(
                    v.as_u64()
                        .filter(|n| *n <= 63)
                        .ok_or_else(|| Error::new("invalid_arguments", "trait_index must be 0..63"))? as usize,
                ),
            };
            if list_only == index.is_some() {
                return Err(Error::new(
                    "invalid_arguments",
                    "Pass exactly one of trait_index (queue that trait) or list:true (return the trait tree)",
                ));
            }
            let job = Job {
                kind: "mio_trait",
                action: json!({"kind":"mio_trait","organization":organization,"trait_index":index,"slot":slot}),
                plan: Box::new(|process| {
                    *resolved.borrow_mut() = Some(names(process, organization, None)?);
                    Ok(())
                }),
                dispatch: Box::new(|process, keys| {
                    let n = need()?;
                    mio_trait(process, keys, &n.org_display, slot, index)
                }),
                settle: Box::new(|_, seen| {
                    Ok(match seen.and_then(|v| v.get("listing_only")).and_then(Value::as_bool) {
                        // Listing changes nothing: whatever it saw is its result.
                        Some(true) => super::journal::Outcome::Reached(json!({"tool":"mio_trait","listing":true})),
                        _ => ui_action::effect(seen, "trait_queued", json!({"tool":"mio_trait","organization":organization,"trait_index":index})),
                    })
                }),
            };
            ui_action::run(job, id, token, process, keys, guard, authority, actions)
        }
        _ => Err(Error::new("capability_unavailable", "Unknown organisation tool")),
    }
}

pub(super) fn tools() -> Vec<Value> {
    vec![
        json!({"name":"hoi4_mio_design_team","description":"UNVALIDATED (no live test yet): put a military-industrial organisation into a design-team slot of the politics view: Q politics view, the slot's roster button (the `slot`-th shown organisation slot, default 0), the organisation's row in the list (found by its localised name; `organization` is the script key from hoi4_mios, e.g. GER_porsche_organization). Typed refusals: organization_unknown, organization_not_allowed (allowed block rules the tag out) before input; mio_system_absent (the politics view shows no organisation slot: the organisation system is not in use in this game, designers are ideas, use hoi4_politics_choose), slot_unknown, organization_not_offered (lists the rows) found from the windows before the first state-changing click. Verified only by the list closing (political power before/after is recorded): verification gui_only. Results carry validated:false.","inputSchema":{"type":"object","properties":{"organization":{"type":"string","maxLength":128},"slot":{"type":"integer","minimum":0,"maximum":15},"request_id":{"type":"string"},"lease_token":{"type":"string"}},"required":["organization","request_id","lease_token"],"additionalProperties":false}}),
        json!({"name":"hoi4_mio_policy","description":"UNVALIDATED (no live test yet): attach a policy to an organisation: politics view, the slot's roster, the organisation's details button, Open policies, the policy row (by localised name). `organization` and `policy` are script keys (mio_policy_general_ruthless_contracts, ...). Typed refusals: organization_unknown, organization_not_allowed, policy_unknown (before input); mio_system_absent, slot_unknown, organization_not_offered, policy_not_offered (lists the rows) from the windows before the click. Verified from the detail window's attached policy name only (verification gui_only). Results carry validated:false.","inputSchema":{"type":"object","properties":{"organization":{"type":"string","maxLength":128},"policy":{"type":"string","maxLength":128},"slot":{"type":"integer","minimum":0,"maximum":15},"request_id":{"type":"string"},"lease_token":{"type":"string"}},"required":["organization","policy","request_id","lease_token"],"additionalProperties":false}}),
        json!({"name":"hoi4_mio_trait","description":"UNVALIDATED (no live test yet): queue a trait of an organisation's trait tree: politics view, the slot's roster, the organisation's details button, the trait icon at `trait_index` (0-based in the order the tree window lists its entries; trait entries carry no text, so call with list:true first: it returns each entry's index, enabled state, queue number and whether it is visible, changing nothing). Exactly one of trait_index and list. Typed refusals: organization_unknown, organization_not_allowed (before input); mio_system_absent, slot_unknown, organization_not_offered, trait_unknown, trait_already_queued, trait_unavailable, trait_not_visible from the windows before the click. Verified from the entry's queue number only (verification gui_only). Results carry validated:false.","inputSchema":{"type":"object","properties":{"organization":{"type":"string","maxLength":128},"trait_index":{"type":"integer","minimum":0,"maximum":63},"list":{"type":"boolean"},"slot":{"type":"integer","minimum":0,"maximum":15},"request_id":{"type":"string"},"lease_token":{"type":"string"}},"required":["organization","request_id","lease_token"],"additionalProperties":false}}),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use super::super::mios::{Catalog, Org};
    use super::super::script;

    fn catalog() -> Catalog {
        let entries = script::parse(
            r#"
            GER_porsche_organization = { icon = GFX_x allowed = { original_tag = GER } }
            generic_tank_organization = { icon = GFX_y allowed = { NOT = { original_tag = GER } } }
            "#,
        );
        Catalog {
            orgs: mios::orgs_from(&entries),
            designers: Default::default(),
        }
    }

    #[test]
    fn organisation_plans_use_the_tag_verdict() {
        let c = catalog();
        assert!(plan_org(&c, "GER_porsche_organization", "GER").is_ok());
        assert_eq!(
            plan_org(&c, "GER_porsche_organization", "ENG").unwrap_err().code,
            "organization_not_allowed"
        );
        assert_eq!(
            plan_org(&c, "generic_tank_organization", "GER").unwrap_err().code,
            "organization_not_allowed"
        );
        let e = plan_org(&c, "porsche", "GER").unwrap_err();
        assert_eq!(e.code, "organization_unknown");
        assert!(e.message.contains("GER_porsche_organization"));
        let _ = Org::default();
    }

    #[test]
    fn policy_keys_come_from_the_policy_scripts() {
        let entries = script::parse(
            "mio_policy_a = { icon = x } mio_policy_b = { icon = y } @var = 3 other = { z = 1 }",
        );
        let keys = policy_keys(&entries);
        assert_eq!(keys, ["mio_policy_a", "mio_policy_b"]);
        assert!(plan_policy(&keys, "mio_policy_a").is_ok());
        let e = plan_policy(&keys, "mio_policy_c").unwrap_err();
        assert_eq!(e.code, "policy_unknown");
        let e = plan_policy(&keys, "policy_a").unwrap_err();
        assert!(e.message.contains("mio_policy_a"), "{}", e.message);
    }

    #[test]
    fn missing_organisation_slots_mean_the_system_is_absent() {
        assert_eq!(absent_refusal(0).unwrap().0, "mio_system_absent");
        assert!(absent_refusal(2).is_none());
    }

    #[test]
    fn trait_clicks_are_refused_from_the_entries_on_screen() {
        let rect = Some(gui_tree::Rect { x: 1, y: 2, w: 3, h: 4 });
        let entry = |index, enabled, queue: &str, rect| TraitEntry {
            index,
            enabled,
            queue: queue.into(),
            rect,
        };
        let entries = vec![
            entry(0, true, "", rect),
            entry(1, true, "2", rect),
            entry(2, false, "", rect),
            entry(3, true, "", None),
        ];
        assert!(trait_refusal(&entries, 0).is_none());
        assert_eq!(trait_refusal(&entries, 1).unwrap().0, "trait_already_queued");
        assert_eq!(trait_refusal(&entries, 2).unwrap().0, "trait_unavailable");
        assert_eq!(trait_refusal(&entries, 3).unwrap().0, "trait_not_visible");
        assert_eq!(trait_refusal(&entries, 9).unwrap().0, "trait_unknown");
    }

    #[test]
    fn descriptors_are_unvalidated_and_lease_gated() {
        for t in tools() {
            assert!(t["description"].as_str().unwrap().starts_with("UNVALIDATED (no live test yet):"));
            assert!(t["inputSchema"]["required"].as_array().unwrap().iter().any(|v| v == "lease_token"));
            assert!(TOOLS.contains(&t["name"].as_str().unwrap()));
        }
        assert_eq!(tools().len(), TOOLS.len());
    }
}
