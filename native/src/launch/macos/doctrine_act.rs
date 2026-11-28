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
    ui::{accept_command, clear_notices, read, settled, wait},
    ui_action::{self, refuse, Driven, Job},
    Guard, InputAuthority,
};
use crate::{
    error::{Error, Result},
    gui_tree,
    platform::{game_input::GameInput, Process},
    xlayout::{self, fld, img},
};
use serde_json::{json, Value};
use std::{cell::RefCell, time::Duration};

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
                format!(
                    "Grand doctrine {doctrine} names the unknown folder {}",
                    g.folder
                ),
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
        return Err(Error::new(
            "doctrine_unknown",
            "Folder not in the doctrine system",
        ));
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
                    format!(
                        "{} already fills another track and cannot be used twice",
                        t.id
                    ),
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
                return Ok(if gui_tree::visible(r, root)? {
                    Some(root)
                } else {
                    None
                });
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
    const MANAGER: u64 = 0x670;
    const IDLER: xlayout::Class = xlayout::Class::new("CInGameIdler", 0x35089d8);
    const IFACE_MANAGER: xlayout::Class = xlayout::Class::new("idler.manager", 0);
    const VIEW: xlayout::Class =
        xlayout::Class::new(pk::DOCTRINE_VIEW.name, pk::DOCTRINE_VIEW.vtable);
    const FOLDER: xlayout::Class = xlayout::Class::new("CFolderView", 0);
    const LIST: xlayout::Class =
        xlayout::Class::new(pk::DOCTRINE_SELECTION.name, pk::DOCTRINE_SELECTION.vtable);
    let abi = r.string_abi();
    let idler = r.u64(i + crate::game::layout(r).idler_global)?;
    let manager = r.u64(idler + fld(abi, &IDLER, MANAGER)?)?;
    let view = r.u64(manager + fld(abi, &IFACE_MANAGER, MANAGER_SLOT)?)?;
    if view == 0 || r.u64(view)? != i + img(abi, pk::DOCTRINE_VIEW.vtable)? {
        return Ok(None);
    }
    let folder = r.u64(view + fld(abi, &VIEW, FOLDER_VIEW)?)?;
    if folder == 0 {
        return Ok(None);
    }
    let list = r.u64(folder + fld(abi, &FOLDER, SELECTION)?)?;
    if list == 0 || r.u64(list)? != i + img(abi, pk::DOCTRINE_SELECTION.vtable)? {
        return Ok(None);
    }
    match gui_tree::container(r, r.u64(list + fld(abi, &LIST, SELECTION_ROOT)?)?, i)? {
        Some(root) if gui_tree::visible(r, root)? => Ok(Some(root)),
        _ => Ok(None),
    }
}
fn selection_open(r: &crate::memory::Reader<'_>, i: u64) -> Result<bool> {
    Ok(selection_root(r, i)?.is_some())
}

fn open_view(process: &dyn Process, keys: &dyn GameInput, t: &Target) -> Result<()> {
    if pk::open_now(process, &pk::DOCTRINE_VIEW) {
        return Ok(());
    }
    if !pk::open_now(process, &pk::OFFICER_VIEW) {
        keys.press_shifted(KEY_E)?;
        wait(process, "open officer corps view", |r, i| {
            pk::is_open(r, i, &pk::OFFICER_VIEW)
        })?;
        crate::phase::sleep(Duration::from_millis(300));
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
    crate::phase::sleep(Duration::from_millis(300));
    Ok(())
}

fn select_folder_tab(process: &dyn Process, keys: &dyn GameInput, t: &Target) -> Result<()> {
    pk::click(process, keys, "folder tab", |r, i| {
        let root = pk::open_root(r, i, &pk::DOCTRINE_VIEW)?;
        let grid = pk::need(r, i, root, "folders_grid", 4)?;
        let items = pk::items(r, i, grid)?;
        let item = items.get(t.folder_index).ok_or_else(|| {
            Error::new(
                "folder_tab_missing",
                format!(
                    "The window offers {} folder tabs, not {}",
                    items.len(),
                    t.folder_index + 1
                ),
            )
        })?;
        pk::rect_of(r, i, item.frame, "folder_button", 2)
    })?;
    crate::phase::sleep(Duration::from_millis(300));
    Ok(())
}

fn open_selection(process: &dyn Process, keys: &dyn GameInput, t: &Target) -> Result<()> {
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
    crate::phase::sleep(Duration::from_millis(300));
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
            // The row button draws a tiled sprite with no reviewed renderer; its name text
            // lies inside it, so a point near the text's left edge is on the button.
            rect = pk::rect_of(r, i, item.frame, "select_button", 2)
                .or_else(|_| {
                    // doctrine_icon sits at (10, 4) inside the 246x71 button (doctrines_view.gui):
                    // the button's middle is 113 px right of the icon's left edge.
                    pk::rect_of(r, i, item.frame, "doctrine_icon", 2).map(|ic| gui_tree::Rect {
                        x: ic.x + 103,
                        y: ic.y + 25,
                        w: 20,
                        h: 10,
                    })
                })
                .ok();
        }
        names.push(name);
    }
    Ok((names, found, rect))
}

fn adopt(process: &dyn Process, keys: &dyn GameInput, t: &Target, display: &str) -> Driven {
    clear_notices(process, keys)?;
    let result: Driven = (|| {
        open_view(process, keys, t)?;
        select_folder_tab(process, keys, t)?;
        open_selection(process, keys, t)?;
        let (names, found, _) =
            super::ui::settled(process, "doctrine rows", |r, i| rows(r, i, display))?;
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
        crate::phase::sleep(Duration::from_millis(250));
        let unlock_enabled = read(process, |r, i| {
            let root = selection_root(r, i)?
                .ok_or_else(|| Error::new("window_not_open", "Doctrine selection list closed"))?;
            let button = pk::need(r, i, root, "unlock_button", 3)?;
            Ok(gui_tree::shown(r, button)? && !gui_tree::disabled(r, button)?)
        })?;
        if !unlock_enabled {
            // Say why: the row's own cost text (the game's price with every cost modifier) and
            // the experience held now (n1, 16 Nov 2026: refused three times with no reason).
            let cost_text = read(process, |r, i| {
                let root = selection_root(r, i)?.ok_or_else(|| {
                    Error::new("window_not_open", "Doctrine selection list closed")
                })?;
                let grid = pk::need(r, i, root, "doctrines_grid", 6)?;
                let items = pk::items(r, i, grid)?;
                match items.get(row) {
                    Some(item) => pk::text(r, i, item.frame, "xp_cost_text", 2),
                    None => Ok(None),
                }
            })
            .ok()
            .flatten();
            let have = read(process, doctrines::snapshot)
                .ok()
                .and_then(|s| doctrines::xp_index(&t.xp_type).map(|k| s.xp[k]));
            return Err(refuse(
                "doctrine_unavailable",
                unlock_refusal(display, &t.xp_type, cost_text.as_deref(), have),
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

/// The number in a cost text ("55", "£army_experience 55", "55.5").
fn cost_number(text: &str) -> Option<f64> {
    let digits: String = text
        .chars()
        .skip_while(|c| !c.is_ascii_digit())
        .take_while(|c| c.is_ascii_digit() || *c == '.')
        .collect();
    digits.trim_end_matches('.').parse().ok()
}

/// Why Unlock is disabled, from the row's cost text and the experience held: a shortfall names
/// both numbers; a cost that is covered names the precondition as the cause.
pub(super) fn unlock_refusal(
    display: &str,
    xp_type: &str,
    cost_text: Option<&str>,
    have: Option<f64>,
) -> String {
    let cost = cost_text.and_then(cost_number);
    let held = have.map(|h| format!("{} {xp_type} experience held", mgmt::round(h, 1)));
    match (cost, have) {
        (Some(c), Some(h)) if h + 0.05 < c => format!(
            "Unlock disabled: {display} costs {c} {xp_type} experience, {} held ({} short; the \
             game's price with its cost modifiers); nothing was adopted",
            mgmt::round(h, 1),
            mgmt::round(c - h, 1)
        ),
        (Some(c), Some(_)) => format!(
            "The game disables Unlock for {display} although its cost {c} is covered ({}): the \
             cause is not experience but an unmet precondition (its `available` trigger, a choice \
             it excludes, or the track); nothing was adopted",
            held.unwrap_or_default()
        ),
        _ => format!(
            "The game disables Unlock for {display} (cost text {:?}, {}): not enough experience \
             or a precondition is unmet; nothing was adopted",
            cost_text.unwrap_or("unread"),
            held.unwrap_or_else(|| "experience unread".into())
        ),
    }
}

fn leave(process: &dyn Process, keys: &dyn GameInput) {
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
    keys: &dyn GameInput,
    guard: &mut Guard,
    authority: &InputAuthority,
    actions: &mut super::journal::GameJournal,
) -> Result<Value> {
    super::allowed(
        args,
        &["doctrine", "track", "replace", "request_id", "lease_token"],
    )?;
    super::refuse_unbuilt_args("hoi4_doctrine_adopt", args)?;
    let doctrine = super::string(args, "doctrine")?;
    if doctrine.is_empty()
        || doctrine.len() > 96
        || !doctrine
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_')
    {
        return Err(Error::new("invalid_arguments", "Invalid doctrine id"));
    }
    let track = match args.get("track") {
        None => None,
        Some(v) => Some(
            v.as_str()
                .filter(|s| {
                    !s.is_empty()
                        && s.len() <= 64
                        && s.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_')
                })
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
            Ok(
                json!({"clicked": seen, "doctrine": t.id, "folder": t.folder, "track": t.track.map(|x| x.1)}),
            )
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
    json!({"name":"hoi4_doctrine_adopt","description":"Adopt a grand doctrine (e.g. new_mobile_warfare) or a subdoctrine (e.g. mobile_infantry) with ordinary input: Shift+E officer corps view, the folder's doctrine button, the folder tab, the grand doctrine slot or the track's subdoctrine slot, the row whose localised name matches, Unlock (the game's confirmation is accepted). Ids and tracks are those of hoi4_doctrines (include_catalog:true); a subdoctrine that fits several tracks needs `track`. Typed refusals before input: doctrine_unknown, no_grand_doctrine, track_required, track_unknown, already_adopted, grand_doctrine_set, track_occupied, subdoctrine_taken, excluded_by_chosen, insufficient_experience (clear shortfall only; cost modifiers are not read); found while driving: doctrine_not_offered, doctrine_unavailable (Unlock disabled; the message names the game's price read from the row and the experience held, or says the price is covered and a precondition is unmet). Verified from the doctrine system in memory (the folder's grand doctrine or the track's subdoctrine names the doctrine). Track mastery rewards unlock by themselves; they have no button. Results carry validated:false. `replace` (swap the grand doctrine or the track's subdoctrine already chosen, instead of the refusals grand_doctrine_set and track_occupied) is not yet built: refused with not_yet_built.","inputSchema":{"type":"object","properties":{"doctrine":{"type":"string","maxLength":96},"track":{"type":"string","maxLength":64},"replace":{"type":"boolean"},"request_id":{"type":"string"},"lease_token":{"type":"string"}},"required":["doctrine","request_id","lease_token"],"additionalProperties":false}})
}

// ------------------------------------------------------------------ officer corps (list)

/// Interface-manager slot of the officer corps view (CCountryArmyOfficerCorpView, pp13).
const OFFICER_SLOT: u64 = 0x160;
/// Widget names whose text is read in the officer corps listing.
fn texty(name: &str) -> bool {
    ["name", "cost", "text", "title", "desc", "value", "label"]
        .iter()
        .any(|w| name.contains(w))
}

/// Every widget of the open officer corps view under a spirit window or grid (all: every
/// widget): its path, shown state, rect, button state (buttons) and text (text-like names).
fn officer_widgets(r: &crate::memory::Reader<'_>, i: u64, all: bool) -> Result<Vec<Value>> {
    let root = pk::open_root(r, i, &pk::OFFICER_VIEW)?;
    widgets_under(r, i, root, all)
}
fn widgets_under(
    r: &crate::memory::Reader<'_>,
    i: u64,
    root: u64,
    all: bool,
) -> Result<Vec<Value>> {
    let mut out = Vec::new();
    let mut stack = vec![(root, 0u32, String::new(), false)];
    while let Some((obj, depth, path, inside)) = stack.pop() {
        for child in gui_tree::children(r, obj)? {
            let name = gui_tree::name(r, child, i)?.unwrap_or_default();
            let p = format!("{path}/{name}");
            let spirit = all || inside || name.contains("spirit");
            if spirit {
                let rect = gui_tree::rect_unclipped(r, i, child, pk::GAME)
                    .map(|x| json!([x.x, x.y, x.w, x.h]))
                    .unwrap_or(Value::Null);
                let mut row = json!({"path": p, "shown": gui_tree::shown(r, child).unwrap_or(false),
                    "rect": rect});
                if name.contains("button") {
                    row["button_state"] = json!(gui_tree::button_state(r, child).ok());
                }
                if texty(&name) {
                    if let Ok(Some(t)) = pk::text(r, i, obj, &name, 0) {
                        row["text"] = json!(t);
                    }
                }
                out.push(row);
            }
            if depth < 10 && out.len() < 600 {
                if let Ok(Some(inner)) = gui_tree::container(r, child, i) {
                    stack.push((inner, depth + 1, p.clone(), spirit));
                }
                // a grid box keeps its entries as grid items, not as children (pp17)
                if name.contains("grid") {
                    match pk::items(r, i, child) {
                        Ok(items) => {
                            for (k, item) in items.iter().enumerate() {
                                stack.push((item.frame, depth + 1, format!("{p}/#{k}"), spirit));
                            }
                        }
                        Err(e) => out.push(json!({"path": p, "grid_items_error": e.message})),
                    }
                }
            }
        }
    }
    Ok(out)
}

/// The spirit slots of the open officer corps view, from widgets_under(root, false): per view
/// (army, navy, air) and slot index, the entry kind (the gui file's `<kind>_spirit_entry`, read
/// from its glow), the slot button's rect, whether it is the selected slot (button state 2) and
/// whether a spirit is taken (its spirit_icon shown).
fn slots_of(widgets: &[Value]) -> Vec<Value> {
    let mut out: Vec<Value> = Vec::new();
    for w in widgets {
        let Some(path) = w["path"].as_str() else {
            continue;
        };
        let parts: Vec<&str> = path.split('/').filter(|x| !x.is_empty()).collect();
        // officer_core_view / <view>_spirits_view / spirit_grid / #k / leaf
        let [_, view, "spirit_grid", k, leaf] = parts.as_slice() else {
            continue;
        };
        let (Some(view), Some(k)) = (
            view.strip_suffix("_spirits_view"),
            k.strip_prefix('#').and_then(|x| x.parse::<u64>().ok()),
        ) else {
            continue;
        };
        let at = match out
            .iter()
            .position(|s| s["view"] == json!(view) && s["index"] == json!(k))
        {
            Some(at) => at,
            None => {
                out.push(
                    json!({"view": view, "index": k, "entry": null, "rect": null,
                    "selected": false, "taken": false}),
                );
                out.len() - 1
            }
        };
        let slot = &mut out[at];
        match *leaf {
            "button" => {
                slot["rect"] = w["rect"].clone();
                slot["selected"] = json!(w["button_state"] == json!(2));
            }
            "spirit_icon" => slot["taken"] = json!(w["shown"] == json!(true)),
            l => {
                if let Some(kind) = l.strip_suffix("_spirit_glow") {
                    slot["entry"] = json!(kind);
                }
            }
        }
    }
    out.sort_by_key(|s| {
        (
            s["view"].as_str().unwrap_or("").to_string(),
            s["index"].as_u64(),
        )
    });
    out
}

/// The spirits a slot's spirit_window offers, from widgets_under(spirit_window, true): its title
/// and per box_grid item the name, the cost shown and the entry's rect (the click target).
fn spirits_of(widgets: &[Value]) -> (Option<String>, Vec<Value>) {
    let mut title = None;
    let mut out: Vec<Value> = Vec::new();
    for w in widgets {
        let Some(path) = w["path"].as_str() else {
            continue;
        };
        if path == "/title" {
            title = w["text"].as_str().map(str::to_string);
            continue;
        }
        let Some(rest) = path.strip_prefix("/ideas/grid/box_grid/#") else {
            continue;
        };
        let Some((k, leaf)) = rest.split_once('/') else {
            continue;
        };
        let Ok(k) = k.parse::<u64>() else { continue };
        let at = match out.iter().position(|s| s["index"] == json!(k)) {
            Some(at) => at,
            None => {
                out.push(json!({"index": k, "name": null, "cost": null, "rect": null}));
                out.len() - 1
            }
        };
        let spirit = &mut out[at];
        match leaf {
            "idea_entry_bg" => spirit["rect"] = w["rect"].clone(),
            "cost" => {
                spirit["cost"] = w["text"]
                    .as_str()
                    .and_then(|t| t.trim().parse::<f64>().ok())
                    .map_or(Value::Null, |c| json!(c));
            }
            "traits_container/name" => spirit["name"] = w["text"].clone(),
            _ => {}
        }
    }
    out.sort_by_key(|s| s["index"].as_u64());
    (title, out)
}

/// One spirit_window instance as read: drawn (visible or shown) and its title and spirits.
#[derive(Clone, Debug, PartialEq)]
struct SpiritWindow {
    drawn: bool,
    read: (Option<String>, Vec<Value>),
}

/// The window a slot click opened: the only drawn one that lists spirits, else the only one
/// whose list changed with the click; None when that is not one window.
fn pick_window(
    before: &[SpiritWindow],
    after: &[SpiritWindow],
) -> Option<(Option<String>, Vec<Value>)> {
    let drawn: Vec<&SpiritWindow> = after
        .iter()
        .filter(|w| w.drawn && !w.read.1.is_empty())
        .collect();
    if let [one] = drawn.as_slice() {
        return Some(one.read.clone());
    }
    let changed: Vec<&SpiritWindow> = after
        .iter()
        .enumerate()
        .filter(|(k, w)| !w.read.1.is_empty() && before.get(*k).map(|b| &b.read) != Some(&w.read))
        .map(|(_, w)| w)
        .collect();
    match changed.as_slice() {
        [one] => Some(one.read.clone()),
        _ => None,
    }
}

/// hoi4_officer_corps: list:true opens the officer corps view (Shift+E), returns its spirit slots
/// and, with `slot`, the spirits that slot offers; with `slot` and `spirit` it picks that spirit
/// (live-verified fk1, 18 Nov 2026: army:0, army:1, army:2 and air:0); then closes the view.
pub(super) fn officer_call(
    args: &serde_json::Map<String, Value>,
    process: &(dyn Process + Send + Sync),
    keys: &dyn GameInput,
    guard: &mut Guard,
    authority: &InputAuthority,
    actions: &mut super::journal::GameJournal,
) -> Result<Value> {
    super::allowed(
        args,
        &["list", "slot", "spirit", "request_id", "lease_token"],
    )?;
    let bad_slot = || {
        Error::new(
            "invalid_arguments",
            "slot is VIEW:INDEX (army, navy or air)",
        )
    };
    let slot = match args.get("slot") {
        None => None,
        Some(v) => {
            let text = v.as_str().unwrap_or_default();
            let (view, k) = text.split_once(':').ok_or_else(bad_slot)?;
            let k: usize = k.parse().map_err(|_| bad_slot())?;
            if !matches!(view, "army" | "navy" | "air") || k > 5 {
                return Err(bad_slot());
            }
            Some((view.to_string(), k))
        }
    };
    // A spirit to pick (exact name as the window shows it, case-insensitive) needs its slot.
    let spirit = match args.get("spirit") {
        None => None,
        Some(v) => Some(
            v.as_str()
                .map(str::trim)
                .filter(|t| !t.is_empty() && t.len() <= 64)
                .ok_or_else(|| Error::new("invalid_arguments", "spirit is a spirit's name"))?
                .to_string(),
        ),
    };
    if spirit.is_some() && slot.is_none() {
        return Err(Error::new(
            "invalid_arguments",
            "spirit needs slot (VIEW:INDEX) to open its window",
        ));
    }
    let list = mgmt::flag(args, "list")?;
    if !list && spirit.is_none() {
        return Err(Error::new(
            "invalid_arguments",
            "Give list:true (read) or slot and spirit (pick)",
        ));
    }
    let id = super::string(args, "request_id")?;
    let token = super::string(args, "lease_token")?;
    let pick = spirit.clone();
    let job = Job {
        kind: "officer_corps",
        action: json!({"kind":"officer_corps","list":list,
            "slot": slot.as_ref().map(|(v, k)| format!("{v}:{k}")), "spirit": spirit}),
        plan: Box::new(|_| Ok(())),
        dispatch: Box::new(|process, keys| {
            let result: Driven = (|| {
                if !pk::open_now(process, &pk::OFFICER_VIEW) {
                    keys.press_shifted(KEY_E)?;
                    wait(process, "open officer corps view", |r, i| {
                        pk::is_open(r, i, &pk::OFFICER_VIEW)
                    })?;
                }
                // the view slides in from the left edge
                crate::phase::sleep(Duration::from_millis(600));
                let slots = slots_of(&read(process, |r, i| officer_widgets(r, i, false))?);
                let Some((view, k)) = &slot else {
                    return Ok(json!({"listing_only": true, "slots": slots,
                        "verification": "gui_only"}));
                };
                let rect = slots
                    .iter()
                    .find(|s| s["view"] == json!(view) && s["index"] == json!(k))
                    .and_then(|s| s["rect"].as_array().cloned())
                    .ok_or_else(|| {
                        refuse(
                            "slot_missing",
                            format!("The {view} spirits show no slot {k}"),
                        )
                    })?;
                let n = |x: usize| rect.get(x).and_then(Value::as_i64).unwrap_or(0) as i32;
                let button = gui_tree::Rect {
                    x: n(0),
                    y: n(1),
                    w: n(2),
                    h: n(3),
                };
                // The slot button opens the gui file's top-level spirit_window, which the view
                // object holds (not a child of its root, not an interface-manager slot: pp22).
                // Each branch has its own spirit_window (three instances held by the view, pp25),
                // and a closed one keeps its last list: the one this click opened is the one
                // drawn (visible or shown), else the one whose list changed.
                let windows =
                    |r: &crate::memory::Reader<'_>, i: u64| -> Result<Vec<SpiritWindow>> {
                        pk::view_roots(r, i, OFFICER_SLOT, "spirit_window")?
                            .into_iter()
                            .map(|root| {
                                Ok(SpiritWindow {
                                    drawn: gui_tree::visible(r, root).unwrap_or(false)
                                        || gui_tree::shown(r, root).unwrap_or(false),
                                    read: spirits_of(&widgets_under(r, i, root, true)?),
                                })
                            })
                            .collect()
                    };
                let before = read(process, windows)?;
                pk::click(process, keys, "officer corps slot", |_, _| Ok(button))?;
                let after = settled(process, "spirit window", windows)?;
                let spirits = pick_window(&before, &after).ok_or_else(|| {
                    Error::new(
                        "gui_target_unavailable",
                        format!(
                            "The slot's spirit window was not told apart ({} windows: {})",
                            after.len(),
                            after
                                .iter()
                                .map(|w| format!(
                                    "{} drawn {} spirits {}",
                                    w.read.0.as_deref().unwrap_or("?"),
                                    w.drawn,
                                    w.read.1.len()
                                ))
                                .collect::<Vec<_>>()
                                .join("; ")
                        ),
                    )
                })?;
                let Some(want) = &pick else {
                    return Ok(
                        json!({"listing_only": true, "slots": slots, "slot": format!("{view}:{k}"),
                        "title": spirits.0, "spirits": spirits.1, "verification": "gui_only"}),
                    );
                };
                let entry = spirits
                    .1
                    .iter()
                    .find(|e| {
                        e["name"]
                            .as_str()
                            .is_some_and(|n| n.trim().eq_ignore_ascii_case(want))
                    })
                    .cloned()
                    .ok_or_else(|| {
                        refuse(
                            "spirit_unknown",
                            format!(
                                "{} offers no spirit {want:?}; it offers: {}",
                                spirits.0.as_deref().unwrap_or("the slot"),
                                spirits
                                    .1
                                    .iter()
                                    .filter_map(|e| e["name"].as_str())
                                    .collect::<Vec<_>>()
                                    .join(", ")
                            ),
                        )
                    })?;
                let r = entry["rect"].as_array().cloned().unwrap_or_default();
                let n = |x: usize| r.get(x).and_then(Value::as_i64).unwrap_or(0) as i32;
                let target = gui_tree::Rect {
                    x: n(0),
                    y: n(1),
                    w: n(2),
                    h: n(3),
                };
                if target.w <= 0 || target.h <= 0 {
                    return Err(refuse(
                        "gui_geometry_unavailable",
                        format!("The spirit {want:?} has no drawn entry"),
                    ));
                }
                let before_taken = slots
                    .iter()
                    .find(|s| s["view"] == json!(view) && s["index"] == json!(k))
                    .map(|s| s["taken"] == json!(true))
                    .unwrap_or(false);
                // the branch's experience against the price the window shows (every cost
                // modifier applied): a pick the pool cannot pay is refused before the click with
                // both numbers (g92b, 27 Nov 2026: Political Loyalty clicked three times, each
                // read back not taken, no reason given)
                let held = read(process, doctrines::snapshot)
                    .ok()
                    .and_then(|s| doctrines::xp_index(view).map(|x| s.xp[x]));
                if let (Some(price), Some(held)) = (entry["cost"].as_f64(), held) {
                    if held + 1e-6 < price {
                        return Err(refuse(
                            "insufficient_experience",
                            format!(
                                "{want} costs {price} {view} experience and {} is held",
                                mgmt::round(held, 1)
                            ),
                        ));
                    }
                }
                pk::click(process, keys, "officer corps spirit", |_, _| Ok(target))?;
                // the slot is read again until it shows the spirit (the view refreshes on the
                // game's own tick, not at once), up to three seconds
                let mut taken = false;
                for _ in 0..12 {
                    crate::phase::sleep(Duration::from_millis(250));
                    let after = slots_of(&read(process, |r, i| officer_widgets(r, i, false))?);
                    taken = after
                        .iter()
                        .find(|s| s["view"] == json!(view) && s["index"] == json!(k))
                        .map(|s| s["taken"] == json!(true))
                        .unwrap_or(false);
                    if taken && !before_taken {
                        break;
                    }
                }
                Ok(
                    json!({"slot": format!("{view}:{k}"), "spirit": entry["name"],
                    "cost": entry["cost"], "taken_before": before_taken, "taken_after": taken,
                    "title": spirits.0, "verification": "gui_only"}),
                )
            })();
            leave(process, keys);
            result
        }),
        settle: Box::new(|_, seen| {
            Ok(match seen {
                // A pick holds when the slot shows a spirit it did not show before.
                Some(v) if v.get("taken_after").is_some() => {
                    if v["taken_after"] == json!(true) && v["taken_before"] != json!(true) {
                        Outcome::Reached(json!({"tool": "officer_corps", "seen": v}))
                    } else {
                        Outcome::Diverged(json!({"tool": "officer_corps", "seen": v}))
                    }
                }
                // Listing changes nothing: whatever it saw is its result.
                Some(v) => Outcome::Reached(json!({"tool": "officer_corps", "seen": v})),
                None => Outcome::Unobservable,
            })
        }),
    };
    ui_action::run(job, id, token, process, keys, guard, authority, actions)
}

pub(super) fn officer_tool() -> Value {
    json!({"name":"hoi4_officer_corps","description":"Live-verified read (pp18, pp23, 17 Nov 2026) and pick (fk1, 18 Nov 2026: army:0, army:1, army:2, air:0): the officer corps spirits with ordinary input. list:true opens the officer corps view (Shift+E), returns its spirit slots (view army/navy/air, index, entry kind such as academy, army or division_command, the slot button's rect, selected, taken: a spirit shown in the slot) and closes it, changing nothing. With slot \"VIEW:INDEX\" (e.g. \"army:0\") it also opens that slot's spirit window and returns its title and the spirits offered (index, name, cost in the branch's experience, entry rect); the window closes with the view. With slot and spirit (a name the window shows) it picks that spirit: clicks its entry and verifies the slot shows a spirit it did not show before (taken_before/taken_after; Diverged otherwise, e.g. too little experience). Refusals while driving: slot_missing, spirit_unknown (lists the spirits offered). Results carry verification gui_only.","inputSchema":{"type":"object","properties":{"list":{"type":"boolean"},"slot":{"type":"string","maxLength":16},"spirit":{"type":"string","maxLength":64},"request_id":{"type":"string"},"lease_token":{"type":"string"}},"required":["request_id","lease_token"],"additionalProperties":false}})
}

#[cfg(test)]
mod tests {
    use super::super::doctrines::{RawFolder, RawReduction, RawTemplate, RawTrack};
    use super::super::script;
    use super::*;

    #[test]
    fn officer_corps_slots_read_as_the_view_draws_them() {
        // pp18 shape: the army view's academy slot selected, no spirit taken
        let g = "/officer_core_view/army_spirits_view/spirit_grid";
        let w = vec![
            json!({"path": "/officer_core_view/army_spirits_view", "shown": true, "rect": null}),
            json!({"path": format!("{g}/#1/button"), "shown": true, "rect": [189, 328, 168, 70], "button_state": 0}),
            json!({"path": format!("{g}/#1/army_spirit_glow"), "shown": false, "rect": null}),
            json!({"path": format!("{g}/#1/spirit_icon"), "shown": false, "rect": null}),
            json!({"path": format!("{g}/#0/button"), "shown": true, "rect": [23, 328, 168, 70], "button_state": 2}),
            json!({"path": format!("{g}/#0/academy_spirit_glow"), "shown": false, "rect": null}),
            json!({"path": format!("{g}/#0/spirit_icon"), "shown": true, "rect": null}),
        ];
        let slots = slots_of(&w);
        assert_eq!(slots.len(), 2);
        assert_eq!(slots[0]["entry"], "academy");
        assert_eq!(slots[0]["selected"], true);
        assert_eq!(slots[0]["taken"], true);
        assert_eq!(slots[0]["rect"], json!([23, 328, 168, 70]));
        assert_eq!(slots[1]["entry"], "army");
        assert_eq!(slots[1]["selected"], false);
    }

    #[test]
    fn the_spirit_window_a_click_opened_is_the_drawn_or_the_changed_one() {
        let w = |title: &str, n: usize, drawn: bool| SpiritWindow {
            drawn,
            read: (
                Some(title.to_string()),
                (0..n).map(|k| json!({"index": k})).collect(),
            ),
        };
        // pp25: the army window still holds the academy list; the air window was opened
        let before = vec![w("Spirit of the Academy", 11, false), w("", 0, false)];
        let after = vec![
            w("Spirit of the Academy", 11, false),
            w("Spirit of the Air Force", 9, false),
        ];
        assert_eq!(
            pick_window(&before, &after).unwrap().0.as_deref(),
            Some("Spirit of the Air Force")
        );
        // a drawn window wins even when nothing changed (the same slot opened twice)
        let after = vec![
            w("Spirit of the Academy", 11, true),
            w("Spirit of the Air Force", 9, false),
        ];
        assert_eq!(
            pick_window(&after, &after).unwrap().0.as_deref(),
            Some("Spirit of the Academy")
        );
        // nothing drawn and nothing changed: not told apart
        let same = vec![w("Spirit of the Academy", 11, false)];
        assert!(pick_window(&same, &same).is_none());
    }

    #[test]
    fn a_spirit_window_reads_title_names_costs_and_entry_rects() {
        // pp23 shape: "Spirit of the Academy", items listed last-first
        let b = "/ideas/grid/box_grid";
        let w = vec![
            json!({"path": "/title", "shown": true, "text": "Spirit of the Academy"}),
            json!({"path": format!("{b}/#1/idea_entry_bg"), "shown": true, "rect": [778, 201, 241, 83]}),
            json!({"path": format!("{b}/#1/cost"), "shown": true, "text": "20"}),
            json!({"path": format!("{b}/#1/traits_container/name"), "shown": true, "text": "Tenacious Defense"}),
            json!({"path": format!("{b}/#0/idea_entry_bg"), "shown": true, "rect": [547, 201, 241, 83]}),
            json!({"path": format!("{b}/#0/cost"), "shown": true, "text": "20"}),
            json!({"path": format!("{b}/#0/traits_container/name"), "shown": true, "text": "Bold Attack"}),
        ];
        let (title, spirits) = spirits_of(&w);
        assert_eq!(title.as_deref(), Some("Spirit of the Academy"));
        assert_eq!(spirits.len(), 2);
        assert_eq!(spirits[0]["name"], "Bold Attack");
        assert_eq!(spirits[0]["cost"], json!(20.0));
        assert_eq!(spirits[0]["rect"], json!([547, 201, 241, 83]));
        assert_eq!(spirits[1]["name"], "Tenacious Defense");
    }

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
        assert_eq!(
            check(&other, &c, &t).unwrap_err().code,
            "grand_doctrine_set"
        );
    }

    #[test]
    fn a_disabled_unlock_names_the_shortfall_or_the_precondition() {
        let m = unlock_refusal("Great War Infantry", "army", Some("55"), Some(41.2));
        assert!(
            m.contains("costs 55 army experience") && m.contains("13.8 short"),
            "{m}"
        );
        let m = unlock_refusal("Great War Infantry", "army", Some("55"), Some(80.0));
        assert!(
            m.contains("not experience but an unmet precondition"),
            "{m}"
        );
        let m = unlock_refusal("X", "army", None, Some(10.0));
        assert!(
            m.contains("unread") && m.contains("10 army experience held"),
            "{m}"
        );
        assert_eq!(cost_number("£army_experience 57.5"), Some(57.5));
        assert_eq!(cost_number("none"), None);
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
            resolve(&none, &c, "mobile_infantry", None)
                .unwrap_err()
                .code,
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
            resolve(&s, &c, "mobile_infantry", Some("armor"))
                .unwrap_err()
                .code,
            "track_unknown"
        );
    }

    #[test]
    fn subdoctrine_slot_rules() {
        let c = cat();
        let g = "GRAND_DOCTRINE_MOBILE_WARFARE";
        let occupied = snap(
            Some(g),
            vec![track(Some("SUBDOCTRINE_GARRISON")), track(None)],
            500.0,
        );
        let t = resolve(&occupied, &c, "mobile_infantry", None).unwrap();
        assert_eq!(check(&occupied, &c, &t).unwrap_err().code, "track_occupied");
        let same = snap(
            Some(g),
            vec![track(Some("SUBDOCTRINE_MOBILE_INFANTRY")), track(None)],
            500.0,
        );
        assert_eq!(check(&same, &c, &t).unwrap_err().code, "already_adopted");
        // xor with a doctrine chosen on another track
        let xor = snap(
            Some(g),
            vec![track(None), track(Some("SUBDOCTRINE_GARRISON"))],
            500.0,
        );
        assert_eq!(check(&xor, &c, &t).unwrap_err().code, "excluded_by_chosen");
        // a non-shareable subdoctrine already on the other track
        let taken = snap(
            Some(g),
            vec![track(None), track(Some("SUBDOCTRINE_SOLO"))],
            500.0,
        );
        let solo = resolve(&taken, &c, "solo", Some("infantry")).unwrap();
        assert_eq!(
            check(&taken, &c, &solo).unwrap_err().code,
            "subdoctrine_taken"
        );
        // a shareable one may repeat
        let shared_taken = snap(
            Some(g),
            vec![track(None), track(Some("SUBDOCTRINE_SHARED"))],
            500.0,
        );
        let shared = resolve(&shared_taken, &c, "shared", Some("infantry")).unwrap();
        assert!(check(&shared_taken, &c, &shared).is_ok());
    }

    #[test]
    fn settle_compares_only_the_written_slot() {
        let c = cat();
        let g = "GRAND_DOCTRINE_MOBILE_WARFARE";
        let before = snap(Some(g), vec![track(None), track(None)], 500.0);
        let t = resolve(&before, &c, "mobile_infantry", None).unwrap();
        assert_eq!(
            settle_slot(
                &t.name_key,
                slot_now(&before, &t).as_ref().map(|o| o.as_deref())
            ),
            Verdict::Unchanged
        );
        let after = snap(
            Some(g),
            vec![track(Some("SUBDOCTRINE_MOBILE_INFANTRY")), track(None)],
            400.0,
        );
        assert_eq!(
            settle_slot(
                &t.name_key,
                slot_now(&after, &t).as_ref().map(|o| o.as_deref())
            ),
            Verdict::Reached
        );
        let wrong = snap(
            Some(g),
            vec![track(Some("SUBDOCTRINE_GARRISON")), track(None)],
            400.0,
        );
        assert_eq!(
            settle_slot(
                &t.name_key,
                slot_now(&wrong, &t).as_ref().map(|o| o.as_deref())
            ),
            Verdict::Diverged
        );
        // experience drifting or another track filling never matters
        let drift = snap(
            Some(g),
            vec![track(None), track(Some("SUBDOCTRINE_SOLO"))],
            1.0,
        );
        assert_eq!(
            settle_slot(
                &t.name_key,
                slot_now(&drift, &t).as_ref().map(|o| o.as_deref())
            ),
            Verdict::Unchanged
        );
        assert_eq!(settle_slot("X", None), Verdict::Unknown);
    }

    #[test]
    fn tool_descriptor_is_validated_and_requires_a_lease() {
        let t = tool();
        assert!(t["description"]
            .as_str()
            .unwrap()
            .starts_with("Adopt a grand doctrine"));
        let required = t["inputSchema"]["required"].as_array().unwrap();
        assert!(required.iter().any(|v| v == "lease_token"));
    }
}
