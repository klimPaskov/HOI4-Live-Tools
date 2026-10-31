//! `hoi4_doctrine_adopt`: adopt a grand doctrine or a subdoctrine through the
//! doctrine window with ordinary input. UNVALIDATED: written from the game's
//! `interface/doctrines/doctrines_view.gui` and `countryofficercorpview.gui`
//! definitions; no live run has located the window yet.
//!
//! Route (all names from those `.gui` files): Shift+E opens the officer corps
//! view, its `<folder>_doctrine_button` (the container's `button`) opens the
//! doctrine view, the `folders_grid` tab picks the folder, the grand doctrine
//! slot (`grand_doctrine_button`) or a track's `sub_doctrine_button` opens the
//! `doctrine_selection_list`, the row whose `doctrine_name` is the doctrine's
//! localised name is selected through its `select_button` and `unlock_button`
//! adopts it (a confirmation, if the game asks, is accepted). Success is read
//! from the doctrine system in memory (`hoi4_doctrines`: validated for grand
//! doctrines; the track subdoctrine pointer is itself unvalidated).
//!
//! Track mastery rewards are not a choice in this build's UI (the reward
//! milestones unlock by themselves as mastery accrues, `reward_item` has no
//! button), so there is nothing to click for them.
use super::{
    doctrines::{self, Catalog, GrandDef, Snapshot, SubDef},
    journal::Outcome,
    loc, mgmt, panelkit as pk,
    settle::Verdict,
    ui::{accept_command, clear_notices, read, wait},
    ui_action::{self, refuse, Driven, Job},
    Guard, InputAuthority,
};
use crate::{
    error::{Error, Result},
    gui_tree,
    platform::{macos_input::MacKeys, Process},
};
use serde_json::{json, Value};
use std::{cell::RefCell, thread, time::Duration};

/// ANSI virtual key code of `e` (Shift+E opens the officer corps view).
const KEY_E: u16 = 14;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Kind {
    Grand,
    Sub,
}
/// What the request names, resolved against the catalogue and the folder
/// layout; refusals about the current slot state come later ([`check`]).
#[derive(Clone, Debug, PartialEq)]
pub(super) struct Target {
    pub id: String,
    pub kind: Kind,
    pub folder: String,
    pub folder_index: usize,
    pub name_key: String,
    pub xp_type: String,
    pub xp_cost: Option<f64>,
    /// (track index inside the folder, track id) for a subdoctrine.
    pub track: Option<(usize, String)>,
    pub multi_track: bool,
    pub xor: Vec<String>,
}

pub(super) fn folder_keys(catalog: &Catalog) -> Vec<String> {
    if catalog.folders.is_empty() {
        ["land", "naval", "air", "special_forces"]
            .iter()
            .map(|s| s.to_string())
            .collect()
    } else {
        catalog.folders.iter().map(|f| f.key.clone()).collect()
    }
}

fn grand_def_of<'a>(catalog: &'a Catalog, name_key: &str) -> Option<&'a GrandDef> {
    catalog.grand.iter().find(|g| g.name_key == name_key)
}

pub(super) fn resolve(
    snapshot: &Snapshot,
    catalog: &Catalog,
    doctrine: &str,
    track: Option<&str>,
) -> Result<Target> {
    let folders = folder_keys(catalog);
    if let Some(g) = catalog.grand.iter().find(|g| g.id == doctrine) {
        let Some(folder_index) = folders.iter().position(|f| *f == g.folder) else {
            return Err(Error::new(
                "doctrine_unknown",
                format!("Grand doctrine {doctrine} names the unknown folder {}", g.folder),
            ));
        };
        return Ok(Target {
            id: g.id.clone(),
            kind: Kind::Grand,
            folder: g.folder.clone(),
            folder_index,
            name_key: g.name_key.clone(),
            xp_type: g.xp_type.clone(),
            xp_cost: g.xp_cost,
            track: None,
            multi_track: false,
            xor: Vec::new(),
        });
    }
    let candidates: Vec<&SubDef> = catalog.sub.iter().filter(|s| s.id == doctrine).collect();
    if candidates.is_empty() {
        let mut names: Vec<&str> = catalog
            .grand
            .iter()
            .map(|g| g.id.as_str())
            .chain(catalog.sub.iter().map(|s| s.id.as_str()))
            .collect();
        names.sort_unstable();
        names.dedup();
        let hint: Vec<&str> = names
            .iter()
            .copied()
            .filter(|n| n.contains(doctrine) || doctrine.contains(*n))
            .take(8)
            .collect();
        return Err(Error::new(
            "doctrine_unknown",
            format!(
                "No grand doctrine or subdoctrine with id {doctrine:?} in the install's scripts{}",
                if hint.is_empty() {
                    String::new()
                } else {
                    format!("; similar: {}", hint.join(", "))
                }
            ),
        ));
    }
    // A subdoctrine lives in the folder whose grand doctrine offers its track.
    let mut options: Vec<(Target, bool)> = Vec::new();
    for sub in candidates {
        for (folder_index, raw) in snapshot.folders.iter().enumerate() {
            let Some(folder) = folders.get(folder_index) else {
                continue;
            };
            if !sub.folder.is_empty() && sub.folder != *folder {
                continue;
            }
            let Some(grand) = raw
                .grand
                .as_ref()
                .and_then(|g| grand_def_of(catalog, &g.name_key))
            else {
                continue;
            };
            for (t, track_id) in grand.tracks.iter().enumerate() {
                if !sub.tracks.contains(track_id) {
                    continue;
                }
                let wanted = track.is_none_or(|w| w == track_id);
                options.push((
                    Target {
                        id: sub.id.clone(),
                        kind: Kind::Sub,
                        folder: folder.clone(),
                        folder_index,
                        name_key: sub.name_key.clone(),
                        xp_type: sub.xp_type.clone(),
                        xp_cost: sub.xp_cost,
                        track: Some((t, track_id.clone())),
                        multi_track: sub.multi_track,
                        xor: sub.xor.clone(),
                    },
                    wanted,
                ));
            }
        }
    }
    if options.is_empty() {
        return Err(Error::new(
            "no_grand_doctrine",
            format!(
                "Subdoctrine {doctrine} needs a grand doctrine in its folder first (hoi4_doctrines shows the folders)"
            ),
        ));
    }
    let wanted: Vec<&Target> = options.iter().filter(|(_, w)| *w).map(|(t, _)| t).collect();
    match wanted.len() {
        0 => Err(Error::new(
            "track_unknown",
            format!(
                "Subdoctrine {doctrine} fits the tracks {} of your grand doctrine, not {:?}",
                options
                    .iter()
                    .filter_map(|(t, _)| t.track.as_ref().map(|x| x.1.clone()))
                    .collect::<Vec<_>>()
                    .join(", "),
                track.unwrap_or("")
            ),
        )),
        1 => Ok(wanted[0].clone()),
        _ => Err(Error::new(
            "track_required",
            format!(
                "Subdoctrine {doctrine} fits several tracks; pass track as one of: {}",
                wanted
                    .iter()
                    .filter_map(|t| t.track.as_ref().map(|x| x.1.clone()))
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
        )),
    }
}

/// Estimated cost and available experience, for the result and the refusal.
#[derive(Clone, Debug, PartialEq)]
pub(super) struct Estimate {
    pub have: f64,
    pub cost: Option<f64>,
}

/// Refusals about the current slot and experience; pure.
pub(super) fn check(snapshot: &Snapshot, catalog: &Catalog, t: &Target) -> Result<Estimate> {
    let Some(raw) = snapshot.folders.get(t.folder_index) else {
        return Err(Error::new("doctrine_unknown", "Folder not in the doctrine system"));
    };
    match t.kind {
        Kind::Grand => {
            if let Some(g) = &raw.grand {
                return Err(if g.name_key == t.name_key {
                    Error::new(
                        "already_adopted",
                        format!("{} is already the {} grand doctrine", t.id, t.folder),
                    )
                } else {
                    Error::new(
                        "grand_doctrine_set",
                        format!(
                            "The {} folder already holds {}; a grand doctrine cannot be replaced",
                            t.folder, g.name_key
                        ),
                    )
                });
            }
        }
        Kind::Sub => {
            let (index, track_id) = t.track.as_ref().expect("sub target has a track");
            let Some(track) = raw.tracks.get(*index) else {
                return Err(Error::new(
                    "track_unknown",
                    format!("Track {track_id} is not in the folder's status"),
                ));
            };
            if let Some(s) = &track.subdoctrine {
                return Err(if s.name_key == t.name_key {
                    Error::new(
                        "already_adopted",
                        format!("{} already fills track {track_id}", t.id),
                    )
                } else {
                    Error::new(
                        "track_occupied",
                        format!("Track {track_id} already holds {}", s.name_key),
                    )
                });
            }
            let chosen: Vec<&str> = snapshot
                .folders
                .iter()
                .flat_map(|f| f.tracks.iter())
                .filter_map(|tr| tr.subdoctrine.as_ref().map(|s| s.name_key.as_str()))
                .collect();
            if !t.multi_track && chosen.contains(&t.name_key.as_str()) {
                return Err(Error::new(
                    "subdoctrine_taken",
                    format!("{} already fills another track and cannot be used twice", t.id),
                ));
            }
            for key in &chosen {
                if let Some(other) = catalog.sub.iter().find(|s| s.name_key == *key) {
                    if t.xor.contains(&other.id) || other.xor.contains(&t.id) {
                        return Err(Error::new(
                            "excluded_by_chosen",
                            format!("{} excludes or is excluded by {}", t.id, other.id),
                        ));
                    }
                }
            }
        }
    }
    let have = doctrines::xp_index(&t.xp_type)
        .map(|i| snapshot.xp[i])
        .unwrap_or(0.0);
    let reduction = doctrines::best_reduction(snapshot, raw.template);
    let cost = t.xp_cost.map(|c| c * (1.0 - reduction));
    // The cost modifiers (`<folder>_doctrine_cost_factor`) are not read, so
    // only a clear shortfall refuses; the game disables Unlock otherwise.
    if let Some(c) = cost {
        if have < c * 0.75 {
            return Err(Error::new(
                "insufficient_experience",
                format!(
                    "{} {} experience held, about {:.0} needed for {}",
                    mgmt::round(have, 1),
                    t.xp_type,
                    c,
                    t.id
                ),
            ));
        }
    }
    Ok(Estimate { have, cost })
}

/// The slot the action writes: grand doctrine key of the folder, or the
/// subdoctrine key of the track. `None` when the slot is not in the snapshot.
pub(super) fn slot_now(snapshot: &Snapshot, t: &Target) -> Option<Option<String>> {
    let raw = snapshot.folders.get(t.folder_index)?;
    match t.kind {
        Kind::Grand => Some(raw.grand.as_ref().map(|g| g.name_key.clone())),
        Kind::Sub => {
            let (index, _) = t.track.as_ref()?;
            let track = raw.tracks.get(*index)?;
            Some(track.subdoctrine.as_ref().map(|s| s.name_key.clone()))
        }
    }
}

/// Compare only the slot the action fills.
pub(super) fn settle_slot(want: &str, now: Option<Option<&str>>) -> Verdict {
    match now {
        Some(Some(key)) if key == want => Verdict::Reached,
        Some(None) => Verdict::Unchanged,
        Some(Some(_)) => Verdict::Diverged,
        None => Verdict::Unknown,
    }
}

// ---------------------------------------------------------------- the window

fn selection_root(r: &crate::memory::Reader<'_>, i: u64) -> Result<Option<u64>> {
    if let Some(view) = pk::locate(r, i, &pk::DOCTRINE_VIEW)? {
        if let Some(base) = pk::find(r, i, view, "doctrine_selection_list", 6)? {
            if let Some(root) = gui_tree::container(r, base, i)? {
                return Ok(if gui_tree::visible(r, root)? { Some(root) } else { None });
            }
        }
    }
    match pk::locate(r, i, &pk::DOCTRINE_SELECTION)? {
        Some(root) if gui_tree::visible(r, root)? => return Ok(Some(root)),
        _ => {}
    }
    embedded_selection_root(r, i)
}
/// 1.19.4: the list is a member of the doctrine view's `CFolderView` (view +0x498, list pointer
/// +0x4f0, the list's root window `doctrine_selection_list` at list +0x20) and is neither a child of the view's tree nor in
/// the pop-up list or the interface manager (seen live 31 October 2026: the Screens track's
/// "Select a Subdoctrine" window open while both older lookups found nothing).
fn embedded_selection_root(r: &crate::memory::Reader<'_>, i: u64) -> Result<Option<u64>> {
    const MANAGER_SLOT: u64 = 0x108;
    const FOLDER_VIEW: u64 = 0x498;
    const SELECTION: u64 = 0x4f0;
    const SELECTION_ROOT: u64 = 0x20;
    let idler = r.u64(i + crate::game::layout(r).idler_global)?;
    let view = r.u64(r.u64(idler + 0x670)? + MANAGER_SLOT)?;
    if view == 0 || r.u64(view)? != i + pk::DOCTRINE_VIEW.vtable {
        return Ok(None);
    }
    let folder = r.u64(view + FOLDER_VIEW)?;
    if folder == 0 {
        return Ok(None);
    }
    let list = r.u64(folder + SELECTION)?;
    if list == 0 || r.u64(list)? != i + pk::DOCTRINE_SELECTION.vtable {
        return Ok(None);
    }
    match gui_tree::container(r, r.u64(list + SELECTION_ROOT)?, i)? {
        Some(root) if gui_tree::visible(r, root)? => Ok(Some(root)),
        _ => Ok(None),
    }
}
fn selection_open(r: &crate::memory::Reader<'_>, i: u64) -> Result<bool> {
    Ok(selection_root(r, i)?.is_some())
}

fn open_view(process: &dyn Process, keys: &MacKeys<'_>, t: &Target) -> Result<()> {
    if pk::open_now(process, &pk::DOCTRINE_VIEW) {
        return Ok(());
    }
    if !pk::open_now(process, &pk::OFFICER_VIEW) {
        keys.press_shifted(KEY_E)?;
        wait(process, "open officer corps view", |r, i| {
            pk::is_open(r, i, &pk::OFFICER_VIEW)
        })?;
        thread::sleep(Duration::from_millis(300));
    }
    let button = format!("{}_doctrine_button", t.folder);
    pk::click(process, keys, "doctrine folder button", |r, i| {
        let root = pk::open_root(r, i, &pk::OFFICER_VIEW)?;
        let window = pk::need_window(r, i, root, &button, 10)?;
        pk::rect_of(r, i, window, "button", 2)
    })?;
    wait(process, "open doctrine view", |r, i| {
        pk::is_open(r, i, &pk::DOCTRINE_VIEW)
    })?;
    thread::sleep(Duration::from_millis(300));
    Ok(())
}

fn select_folder_tab(process: &dyn Process, keys: &MacKeys<'_>, t: &Target) -> Result<()> {
    pk::click(process, keys, "folder tab", |r, i| {
        let root = pk::open_root(r, i, &pk::DOCTRINE_VIEW)?;
        let grid = pk::need(r, i, root, "folders_grid", 4)?;
        let items = pk::items(r, i, grid)?;
        let item = items.get(t.folder_index).ok_or_else(|| {
            Error::new(
                "folder_tab_missing",
                format!("The window offers {} folder tabs, not {}", items.len(), t.folder_index + 1),
            )
        })?;
        pk::rect_of(r, i, item.frame, "folder_button", 2)
    })?;
    thread::sleep(Duration::from_millis(300));
    Ok(())
}

fn open_selection(process: &dyn Process, keys: &MacKeys<'_>, t: &Target) -> Result<()> {
    if read(process, selection_open)? {
        return Ok(());
    }
    let track_index = t.track.as_ref().map(|x| x.0);
    pk::click(process, keys, "doctrine slot", |r, i| {
        let root = pk::open_root(r, i, &pk::DOCTRINE_VIEW)?;
        match track_index {
            None => {
                let folder = pk::need_window(r, i, root, "doctrine_folder_view", 4)?;
                let slot = pk::need_window(r, i, folder, "grand_doctrine", 3)?;
                pk::rect_of(r, i, slot, "grand_doctrine_button", 2)
            }
            Some(index) => {
                let grid = pk::need(r, i, root, "tracks_grid", 6)?;
                let items = pk::items(r, i, grid)?;
                let item = items.get(index).ok_or_else(|| {
                    Error::new("track_unknown", format!("Track {} is not shown", index + 1))
                })?;
                let slot = pk::need_window(r, i, item.frame, "sub_doctrine", 4)?;
                pk::rect_of(r, i, slot, "sub_doctrine_button", 2)
            }
        }
    })?;
    wait(process, "open doctrine selection list", selection_open)?;
    thread::sleep(Duration::from_millis(300));
    Ok(())
}

/// (display names on screen, index of the wanted row).
fn rows(
    r: &crate::memory::Reader<'_>,
    i: u64,
    display: &str,
) -> Result<(Vec<String>, Option<usize>, Option<gui_tree::Rect>)> {
    let root = selection_root(r, i)?
        .ok_or_else(|| Error::new("window_not_open", "Doctrine selection list is not shown"))?;
    let grid = pk::need(r, i, root, "doctrines_grid", 6)?;
    let mut names = Vec::new();
    let mut found = None;
    let mut rect = None;
    for (n, item) in pk::items(r, i, grid)?.into_iter().enumerate() {
        let name = pk::text(r, i, item.frame, "doctrine_name", 2)?.unwrap_or_default();
        if found.is_none() && loc::same(&name, display) {
            found = Some(n);
            rect = pk::rect_of(r, i, item.frame, "select_button", 2).ok();
        }
        names.push(name);
    }
    Ok((names, found, rect))
}

fn adopt(
    process: &dyn Process,
    keys: &MacKeys<'_>,
    t: &Target,
    display: &str,
) -> Driven {
    clear_notices(process, keys)?;
    let result: Driven = (|| {
        open_view(process, keys, t)?;
        select_folder_tab(process, keys, t)?;
        open_selection(process, keys, t)?;
        let (names, found, _) = super::ui::settled(process, "doctrine rows", |r, i| {
            rows(r, i, display)
        })?;
        let Some(row) = found else {
            return Err(refuse(
                "doctrine_not_offered",
                format!(
                    "The selection list shows {} but not {display:?}; nothing was adopted",
                    names.join(" | ")
                ),
            ));
        };
        pk::click(process, keys, "doctrine row", |r, i| {
            rows(r, i, display)?
                .2
                .ok_or_else(|| Error::new("gui_target_unavailable", "Row select button not found"))
        })?;
        thread::sleep(Duration::from_millis(250));
        let unlock_enabled = read(process, |r, i| {
            let root = selection_root(r, i)?
                .ok_or_else(|| Error::new("window_not_open", "Doctrine selection list closed"))?;
            let button = pk::need(r, i, root, "unlock_button", 3)?;
            Ok(gui_tree::shown(r, button)? && !gui_tree::disabled(r, button)?)
        })?;
        if !unlock_enabled {
            return Err(refuse(
                "doctrine_unavailable",
                "The game disables Unlock for this doctrine (not enough experience or a precondition is unmet); nothing was adopted",
            ));
        }
        pk::click(process, keys, "unlock button", |r, i| {
            let root = selection_root(r, i)?
                .ok_or_else(|| Error::new("window_not_open", "Doctrine selection list closed"))?;
            pk::rect_of(r, i, root, "unlock_button", 3)
        })?;
        let confirmed = accept_command(process, keys)?;
        Ok(json!({"row": row, "rows_shown": names.len(), "confirmation_accepted": confirmed}))
    })();
    leave(process, keys);
    result
}

fn leave(process: &dyn Process, keys: &MacKeys<'_>) {
    if read(process, selection_open).unwrap_or(false) {
        super::ui::dismiss(process, keys, |r, i| Ok(!selection_open(r, i)?));
    }
    pk::dismiss_window(process, keys, &pk::DOCTRINE_VIEW);
    pk::dismiss_window(process, keys, &pk::OFFICER_VIEW);
}

// ------------------------------------------------------------------ the tool

struct Live {
    snapshot: Snapshot,
    catalog: std::sync::Arc<Catalog>,
    target: Target,
    display: Option<String>,
}
fn live(process: &dyn Process, doctrine: &str, track: Option<&str>) -> Result<Live> {
    let dir = mgmt::game_dir(process)?;
    let catalog = doctrines::catalog(&dir);
    if catalog.grand.is_empty() {
        return Err(Error::new(
            "catalog_unavailable",
            "The install's doctrine scripts could not be read",
        ));
    }
    let snapshot = read(process, doctrines::snapshot)?;
    let target = resolve(&snapshot, &catalog, doctrine, track)?;
    let key = target.name_key.clone();
    let display = loc::lookup(&dir, &[key.as_str()]).remove(&key);
    Ok(Live {
        snapshot,
        catalog,
        target,
        display,
    })
}

pub(super) fn call(
    args: &serde_json::Map<String, Value>,
    process: &(dyn Process + Send + Sync),
    keys: &MacKeys<'static>,
    guard: &mut Guard,
    authority: &InputAuthority,
    actions: &mut super::journal::GameJournal,
) -> Result<Value> {
    super::allowed(args, &["doctrine", "track", "request_id", "lease_token"])?;
    let doctrine = super::string(args, "doctrine")?;
    if doctrine.is_empty()
        || doctrine.len() > 96
        || !doctrine.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_')
    {
        return Err(Error::new("invalid_arguments", "Invalid doctrine id"));
    }
    let track = match args.get("track") {
        None => None,
        Some(v) => Some(
            v.as_str()
                .filter(|s| !s.is_empty() && s.len() <= 64 && s.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_'))
                .ok_or_else(|| Error::new("invalid_arguments", "Invalid track id"))?,
        ),
    };
    let id = super::string(args, "request_id")?;
    let token = super::string(args, "lease_token")?;
    let target: RefCell<Option<(Target, String)>> = RefCell::new(None);
    let job = Job {
        kind: "doctrine_adopt",
        action: json!({"kind":"doctrine_adopt","doctrine":doctrine,"track":track}),
        plan: Box::new(|process| {
            let l = live(process, doctrine, track)?;
            let display = l.display.clone().ok_or_else(|| {
                Error::new(
                    "display_name_unknown",
                    format!(
                        "No English display name for {} in the install's localisation",
                        l.target.name_key
                    ),
                )
            })?;
            check(&l.snapshot, &l.catalog, &l.target)?;
            *target.borrow_mut() = Some((l.target, display));
            Ok(())
        }),
        dispatch: Box::new(|process, keys| {
            let (t, display) = target
                .borrow()
                .clone()
                .ok_or_else(|| Error::new("action_uncertain", "Plan missing at dispatch"))?;
            let seen = adopt(process, keys, &t, &display)?;
            Ok(json!({"clicked": seen, "doctrine": t.id, "folder": t.folder, "track": t.track.map(|x| x.1)}))
        }),
        settle: Box::new(|process, _seen| {
            let l = live(process, doctrine, track)?;
            let t = &l.target;
            let now = slot_now(&l.snapshot, t);
            let verdict = settle_slot(&t.name_key, now.as_ref().map(|o| o.as_deref()));
            let detail = json!({"doctrine": t.id, "folder": t.folder, "slot": now.flatten()});
            Ok(match verdict {
                Verdict::Unknown => Outcome::Unknown,
                v => v.outcome(detail),
            })
        }),
    };
    ui_action::run(job, id, token, process, keys, guard, authority, actions)
}

pub(super) fn tool() -> Value {
    json!({"name":"hoi4_doctrine_adopt","description":"UNVALIDATED (no live test yet): adopt a grand doctrine (e.g. new_mobile_warfare) or a subdoctrine (e.g. mobile_infantry) with ordinary input: Shift+E officer corps view, the folder's doctrine button, the folder tab, the grand doctrine slot or the track's subdoctrine slot, the row whose localised name matches, Unlock (the game's confirmation is accepted). Ids and tracks are those of hoi4_doctrines (include_catalog:true); a subdoctrine that fits several tracks needs `track`. Typed refusals before input: doctrine_unknown, no_grand_doctrine, track_required, track_unknown, already_adopted, grand_doctrine_set, track_occupied, subdoctrine_taken, excluded_by_chosen, insufficient_experience (clear shortfall only; cost modifiers are not read); found while driving: doctrine_not_offered, doctrine_unavailable (Unlock disabled). Verified from the doctrine system in memory (the folder's grand doctrine or the track's subdoctrine names the doctrine). Track mastery rewards unlock by themselves; they have no button. Results carry validated:false.","inputSchema":{"type":"object","properties":{"doctrine":{"type":"string","maxLength":96},"track":{"type":"string","maxLength":64},"request_id":{"type":"string"},"lease_token":{"type":"string"}},"required":["doctrine","request_id","lease_token"],"additionalProperties":false}})
}

#[cfg(test)]
mod tests {
    use super::super::doctrines::{RawFolder, RawReduction, RawTemplate, RawTrack};
    use super::super::script;
    use super::*;

    const FOLDERS: &str = "land = { ledger = army } naval = { ledger = navy } air = { ledger = air } special_forces = { ledger = army }";
    const GRAND: &str = r#"
        new_mobile_warfare = { folder = land name = GRAND_DOCTRINE_MOBILE_WARFARE xp_cost = 100 xp_type = army tracks = { infantry armor } }
        new_mass_assault = { folder = land name = GRAND_DOCTRINE_MASS_ASSAULT xp_cost = 100 xp_type = army tracks = { infantry armor } }
        "#;
    const SUB: &str = r#"
        mobile_infantry = { track = infantry name = SUBDOCTRINE_MOBILE_INFANTRY xp_cost = 100 xp_type = army xor = { garrison } }
        garrison = { track = infantry name = SUBDOCTRINE_GARRISON xp_cost = 100 xp_type = army }
        shared = { track = { infantry armor } allow_in_multiple_tracks = yes name = SUBDOCTRINE_SHARED xp_cost = 50 xp_type = army }
        solo = { track = { infantry armor } name = SUBDOCTRINE_SOLO xp_cost = 50 xp_type = army }
        "#;
    fn cat() -> Catalog {
        doctrines::catalog_from(
            &script::parse(FOLDERS),
            &script::parse(GRAND),
            &[("land".to_string(), script::parse(SUB))],
            100.0,
        )
    }
    fn tpl(key: &str) -> RawTemplate {
        RawTemplate {
            name_key: key.into(),
            xp_cost: 100,
            xp_type: 1,
        }
    }
    fn track(sub: Option<&str>) -> RawTrack {
        RawTrack {
            subdoctrine: sub.map(tpl),
            flag: false,
            mastery: 0.0,
            banked: 0.0,
        }
    }
    fn snap(grand: Option<&str>, tracks: Vec<RawTrack>, xp: f64) -> Snapshot {
        let empty = || RawFolder {
            template: 0,
            grand: None,
            tracks: vec![],
        };
        Snapshot {
            xp: [xp, 0.0, 0.0],
            folders: vec![
                RawFolder {
                    template: 0xaa,
                    grand: grand.map(tpl),
                    tracks,
                },
                empty(),
                empty(),
                empty(),
            ],
            reductions: vec![],
            mastery_bonuses: 0,
        }
    }

    #[test]
    fn grand_doctrine_resolves_and_checks_the_slot_and_experience() {
        let c = cat();
        let s = snap(None, vec![], 120.0);
        let t = resolve(&s, &c, "new_mobile_warfare", None).unwrap();
        assert_eq!((t.kind, t.folder_index), (Kind::Grand, 0));
        let est = check(&s, &c, &t).unwrap();
        assert_eq!(est.cost, Some(100.0));
        // A clear shortfall refuses, a small one is left to the disabled button.
        assert_eq!(
            check(&snap(None, vec![], 40.0), &c, &t).unwrap_err().code,
            "insufficient_experience"
        );
        assert!(check(&snap(None, vec![], 80.0), &c, &t).is_ok());
        let held = snap(Some("GRAND_DOCTRINE_MOBILE_WARFARE"), vec![], 500.0);
        assert_eq!(check(&held, &c, &t).unwrap_err().code, "already_adopted");
        let other = snap(Some("GRAND_DOCTRINE_MASS_ASSAULT"), vec![], 500.0);
        assert_eq!(check(&other, &c, &t).unwrap_err().code, "grand_doctrine_set");
    }

    #[test]
    fn best_cost_reduction_lowers_the_estimate() {
        let c = cat();
        let mut s = snap(None, vec![], 55.0);
        s.reductions.push(RawReduction {
            name: "focus".into(),
            uses_left: 1,
            reduction: 0.5,
            folder_template: 0xaa,
        });
        let t = resolve(&s, &c, "new_mobile_warfare", None).unwrap();
        assert_eq!(check(&s, &c, &t).unwrap().cost, Some(50.0));
    }

    #[test]
    fn unknown_ids_list_similar_ones() {
        let c = cat();
        let s = snap(None, vec![], 0.0);
        let e = resolve(&s, &c, "mobile", None).unwrap_err();
        assert_eq!(e.code, "doctrine_unknown");
    }

    #[test]
    fn subdoctrine_needs_a_grand_doctrine_and_a_track() {
        let c = cat();
        let none = snap(None, vec![], 500.0);
        assert_eq!(
            resolve(&none, &c, "mobile_infantry", None).unwrap_err().code,
            "no_grand_doctrine"
        );
        let s = snap(
            Some("GRAND_DOCTRINE_MOBILE_WARFARE"),
            vec![track(None), track(None)],
            500.0,
        );
        let t = resolve(&s, &c, "mobile_infantry", None).unwrap();
        assert_eq!(t.track, Some((0, "infantry".to_string())));
        assert!(check(&s, &c, &t).is_ok());
        // A subdoctrine that fits two tracks needs the track argument.
        assert_eq!(
            resolve(&s, &c, "solo", None).unwrap_err().code,
            "track_required"
        );
        let armor = resolve(&s, &c, "solo", Some("armor")).unwrap();
        assert_eq!(armor.track, Some((1, "armor".to_string())));
        assert_eq!(
            resolve(&s, &c, "mobile_infantry", Some("armor")).unwrap_err().code,
            "track_unknown"
        );
    }

    #[test]
    fn subdoctrine_slot_rules() {
        let c = cat();
        let g = "GRAND_DOCTRINE_MOBILE_WARFARE";
        let occupied = snap(Some(g), vec![track(Some("SUBDOCTRINE_GARRISON")), track(None)], 500.0);
        let t = resolve(&occupied, &c, "mobile_infantry", None).unwrap();
        assert_eq!(check(&occupied, &c, &t).unwrap_err().code, "track_occupied");
        let same = snap(Some(g), vec![track(Some("SUBDOCTRINE_MOBILE_INFANTRY")), track(None)], 500.0);
        assert_eq!(check(&same, &c, &t).unwrap_err().code, "already_adopted");
        // xor with a doctrine chosen on another track
        let xor = snap(Some(g), vec![track(None), track(Some("SUBDOCTRINE_GARRISON"))], 500.0);
        assert_eq!(check(&xor, &c, &t).unwrap_err().code, "excluded_by_chosen");
        // a non-shareable subdoctrine already on the other track
        let taken = snap(Some(g), vec![track(None), track(Some("SUBDOCTRINE_SOLO"))], 500.0);
        let solo = resolve(&taken, &c, "solo", Some("infantry")).unwrap();
        assert_eq!(check(&taken, &c, &solo).unwrap_err().code, "subdoctrine_taken");
        // a shareable one may repeat
        let shared_taken = snap(Some(g), vec![track(None), track(Some("SUBDOCTRINE_SHARED"))], 500.0);
        let shared = resolve(&shared_taken, &c, "shared", Some("infantry")).unwrap();
        assert!(check(&shared_taken, &c, &shared).is_ok());
    }

    #[test]
    fn settle_compares_only_the_written_slot() {
        let c = cat();
        let g = "GRAND_DOCTRINE_MOBILE_WARFARE";
        let before = snap(Some(g), vec![track(None), track(None)], 500.0);
        let t = resolve(&before, &c, "mobile_infantry", None).unwrap();
        assert_eq!(settle_slot(&t.name_key, slot_now(&before, &t).as_ref().map(|o| o.as_deref())), Verdict::Unchanged);
        let after = snap(Some(g), vec![track(Some("SUBDOCTRINE_MOBILE_INFANTRY")), track(None)], 400.0);
        assert_eq!(settle_slot(&t.name_key, slot_now(&after, &t).as_ref().map(|o| o.as_deref())), Verdict::Reached);
        let wrong = snap(Some(g), vec![track(Some("SUBDOCTRINE_GARRISON")), track(None)], 400.0);
        assert_eq!(settle_slot(&t.name_key, slot_now(&wrong, &t).as_ref().map(|o| o.as_deref())), Verdict::Diverged);
        // experience drifting or another track filling never matters
        let drift = snap(Some(g), vec![track(None), track(Some("SUBDOCTRINE_SOLO"))], 1.0);
        assert_eq!(settle_slot(&t.name_key, slot_now(&drift, &t).as_ref().map(|o| o.as_deref())), Verdict::Unchanged);
        assert_eq!(settle_slot("X", None), Verdict::Unknown);
    }

    #[test]
    fn tool_descriptor_is_marked_unvalidated_and_requires_a_lease() {
        let t = tool();
        assert!(t["description"].as_str().unwrap().starts_with("UNVALIDATED (no live test yet):"));
        let required = t["inputSchema"]["required"].as_array().unwrap();
        assert!(required.iter().any(|v| v == "lease_token"));
    }
}
