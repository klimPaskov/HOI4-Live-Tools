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
    country_status, loc, mgmt, mios, panelkit as pk, script,
    ui::{accept_command, clear_notices, open_politics, read, settled, wait},
    ui_action::{self, refuse, Driven, Fail, Job},
    Guard, InputAuthority,
};
use crate::{
    error::{Error, Result},
    gui_tree,
    memory::Reader,
    platform::{game_input::GameInput, Process},
    xlayout::{fld, img, Class},
};
use serde_json::{json, Value};
use std::{cell::RefCell, time::Duration};

pub(super) const TOOLS: [&str; 4] = [
    "hoi4_mio_design_team",
    "hoi4_mio_policy",
    "hoi4_mio_trait",
    "hoi4_mio_line",
];

#[track_caller]
fn pause(ms: u64) {
    crate::phase::sleep(Duration::from_millis(ms));
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
                format!(
                    "; {} policies exist, e.g. {}",
                    policies.len(),
                    policies
                        .iter()
                        .take(6)
                        .cloned()
                        .collect::<Vec<_>>()
                        .join(", ")
                )
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
        Ok(mgmt::clock(r, i)?["player"]
            .as_str()
            .unwrap_or("")
            .to_string())
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
            if r.u64(child)? == image + img(r.string_abi(), 0x36cca80)? {
                grids.push(child);
            }
            for grid in grids {
                for item in pk::items(r, image, grid)? {
                    // A grid item's own window carries the template name (the politics view's
                    // research_production slots are industrial_org_entry items, 1.19.4.1).
                    if pk::container_name(r, image, item.frame)?.as_deref() == Some(wanted) {
                        const CONTAINER: Class = Class::new("CContainerWindow", 0x36ba978);
                        out.push(item.frame + fld(r.string_abi(), &CONTAINER, 0x30)?);
                        continue;
                    }
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

fn open_system(
    process: &dyn Process,
    keys: &dyn GameInput,
    slot: usize,
) -> std::result::Result<(), Fail> {
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
            format!(
                "The politics view shows {} organisation slots, not {}",
                entries.len(),
                slot + 1
            ),
        ));
    }
    Ok(())
}

/// The politics-view slot whose roster showed each organisation (display name), learned from
/// every roster read this session: a later action on that organisation opens its slot first
/// instead of walking the slots of other kinds.
static SLOT_OF: std::sync::Mutex<Option<std::collections::HashMap<String, usize>>> =
    std::sync::Mutex::new(None);
/// The slot whose roster is open (set by `open_roster`).
static OPEN_SLOT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(usize::MAX);
fn learn_slot(name: &str) {
    let slot = OPEN_SLOT.load(std::sync::atomic::Ordering::Relaxed);
    if slot == usize::MAX || name.is_empty() {
        return;
    }
    if let Ok(mut map) = SLOT_OF.lock() {
        map.get_or_insert_with(Default::default)
            .insert(name.to_string(), slot);
    }
}
fn known_slot(name: &str) -> Option<usize> {
    SLOT_OF
        .lock()
        .ok()
        .and_then(|m| m.as_ref().and_then(|m| m.get(name).copied()))
}
/// The slots to try for an organisation: the one a roster showed it in, else the asked one,
/// then every other.
fn slot_order(asked: usize, count: usize, display: &str) -> Vec<usize> {
    let first = known_slot(display).filter(|s| *s < count).unwrap_or(asked);
    std::iter::once(first)
        .chain((0..count).filter(|s| *s != first))
        .collect()
}

fn open_roster(process: &dyn Process, keys: &dyn GameInput, slot: usize) -> Result<()> {
    pk::click(process, keys, "organisation slot", |r, i| {
        let entry = *org_entries(r, i)?.get(slot).ok_or_else(|| {
            Error::new("slot_unknown", "Organisation slot left the politics view")
        })?;
        pk::rect_of(r, i, entry, "open_roster_button", 2)
    })?;
    wait(process, "open organisation list", |r, i| {
        pk::is_open(r, i, &pk::MIO_LIST)
    })?;
    OPEN_SLOT.store(slot, std::sync::atomic::Ordering::Relaxed);
    pause(400);
    Ok(())
}

/// Probe-only: words of the organisation list widget; pointers that resolve to GUI containers
/// (and pointers to arrays of them) are named, to map the smooth listbox's rows.
pub(super) fn debug_list(process: &dyn Process) -> Result<Value> {
    read(process, |r, i| {
        let root = pk::open_root(r, i, &pk::MIO_LIST)?;
        let list = pk::need(r, i, root, "organisation_list", 4)?;
        let mut out = Vec::new();
        let words = r.read(list, 0x400)?;
        for (k, c) in words.chunks_exact(8).enumerate() {
            let v = u64::from_le_bytes(c.try_into().unwrap());
            let mut note = String::new();
            if (0x10000..0x7fff_ffff_0000).contains(&v) {
                if let Ok(Some(cont)) = gui_tree::container(r, v, i) {
                    note = format!("container {:?}", gui_tree::name(r, cont, i).ok().flatten());
                } else if let Ok((_, data, count)) = r.array_header(v) {
                    if (1..200).contains(&count) {
                        let first = r.u64(data).unwrap_or(0);
                        let named = gui_tree::container(r, first, i)
                            .ok()
                            .flatten()
                            .and_then(|c| gui_tree::name(r, c, i).ok().flatten());
                        note = format!("array count {count} first {first:#x} {named:?}");
                    }
                }
            }
            if !note.is_empty() || v != 0 {
                out.push(
                    json!({"off": format!("{:#x}", k * 8), "v": format!("{v:#x}"), "note": note}),
                );
            }
        }
        // Children of the widget and of sub-objects it points into (embedded containers).
        let mut kids = Vec::new();
        let mut bases = vec![list];
        for c in words.chunks_exact(8) {
            let v = u64::from_le_bytes(c.try_into().unwrap());
            if v > list && v < list + 0x400 {
                bases.push(v);
            }
        }
        for b in bases {
            if let Ok(children) = gui_tree::children(r, b) {
                let names: Vec<Value> = children
                    .iter()
                    .take(12)
                    .map(|c| {
                        let inner = gui_tree::container(r, *c, i).ok().flatten();
                        let name = inner
                            .and_then(|x| gui_tree::name(r, x, i).ok().flatten())
                            .or_else(|| gui_tree::name(r, *c, i).ok().flatten());
                        json!({"child": format!("{c:#x}"), "name": name})
                    })
                    .collect();
                kids.push(
                    json!({"base": format!("{b:#x}"), "children": children.len(), "first": names}),
                );
            }
        }
        // std::vector-style (begin, end) pairs whose elements are GUI containers, in the widget
        // and in objects it points to (two levels).
        let mut vectors = Vec::new();
        let scan = |obj: u64, path: String, vectors: &mut Vec<Value>| {
            let Ok(bytes) = r.read(obj, 0x300) else {
                return;
            };
            let words: Vec<u64> = bytes
                .chunks_exact(8)
                .map(|c| u64::from_le_bytes(c.try_into().unwrap()))
                .collect();
            for k in 0..words.len().saturating_sub(1) {
                let (b, e) = (words[k], words[k + 1]);
                if !(0x10000..0x7fff_ffff_0000).contains(&b)
                    || e <= b
                    || (e - b) % 8 != 0
                    || e - b > 8 * 200
                {
                    continue;
                }
                let first = r.u64(b).unwrap_or(0);
                let name = gui_tree::container(r, first, i)
                    .ok()
                    .flatten()
                    .and_then(|c| gui_tree::name(r, c, i).ok().flatten())
                    .or_else(|| gui_tree::name(r, first, i).ok().flatten());
                if name.is_some() {
                    vectors.push(json!({"at": format!("{path}+{:#x}", k * 8), "count": (e - b) / 8, "first_name": name}));
                }
            }
        };
        scan(list, "list".into(), &mut vectors);
        // the listbox object behind the widget, and two levels of pointers from it
        if let Ok(Some(lb)) = gui_tree::container(r, list, i) {
            scan(lb, "lb".into(), &mut vectors);
            if let Ok(b) = r.read(lb, 0x300) {
                for (k, c) in b.chunks_exact(8).enumerate() {
                    let v = u64::from_le_bytes(c.try_into().unwrap());
                    if (0x10000..0x7fff_ffff_0000).contains(&v) {
                        scan(v, format!("lb[{:#x}]", k * 8), &mut vectors);
                        if let Ok(b2) = r.read(v, 0x100) {
                            for (k2, c2) in b2.chunks_exact(8).enumerate() {
                                let v2 = u64::from_le_bytes(c2.try_into().unwrap());
                                if (0x10000..0x7fff_ffff_0000).contains(&v2) {
                                    scan(
                                        v2,
                                        format!("lb[{:#x}][{:#x}]", k * 8, k2 * 8),
                                        &mut vectors,
                                    );
                                }
                            }
                        }
                    }
                }
            }
        }
        for c in words.chunks_exact(8).enumerate() {
            let v = u64::from_le_bytes(c.1.try_into().unwrap());
            if (0x10000..0x7fff_ffff_0000).contains(&v) {
                scan(v, format!("[{:#x}]", c.0 * 8), &mut vectors);
            }
        }
        Ok(json!({"list": format!("{list:#x}"), "kids": kids, "vectors": vectors}))
    })
}
/// Probe-only (`dbg_line_roster`): the roster a production line opens, as the line tool reads it.
pub(super) fn debug_line_roster(process: &dyn Process) -> Result<Value> {
    read(process, |r, i| {
        let Some(roster) = line_roster(r, i)? else {
            return Ok(json!({"roster": null}));
        };
        let rows: Vec<Value> = roster
            .rows
            .iter()
            .map(|row| {
                json!({"name": row.name, "organisation": row.organisation,
                "rect": row.rect.map(|x| [x.x, x.y, x.w, x.h])})
            })
            .collect();
        Ok(
            json!({"open": roster.open, "list": roster.list.map(|x| [x.x, x.y, x.w, x.h]),
            "rows": rows}),
        )
    })
}

/// Probe-only (`dbg_find_text:TEXT`): pointer paths (up to depth 4) from the organisation list
/// widget to a string equal to TEXT.
pub(super) fn debug_find_text(process: &dyn Process, text: &str) -> Result<Value> {
    read(process, |r, i| {
        let root = pk::open_root(r, i, &pk::MIO_LIST)?;
        let list = if text.starts_with('@') {
            root
        } else {
            pk::need(r, i, root, "organisation_list", 4)?
        };
        let text = text.trim_start_matches('@');
        let mut hits = Vec::new();
        let mut seen = std::collections::HashSet::new();
        let mut frontier = vec![(list, String::from("list"))];
        let mut reads = 0;
        for _depth in 0..6 {
            let mut next = Vec::new();
            for (obj, path) in frontier {
                if !seen.insert(obj) || reads > 400_000 {
                    continue;
                }
                let Ok(bytes) = r.read(obj, 0x800) else {
                    continue;
                };
                reads += 1;
                for (k, c) in bytes.chunks_exact(8).enumerate() {
                    let off = (k * 8) as u64;
                    if let Some(want) = text.strip_prefix("gui=") {
                        let v = u64::from_le_bytes(c.try_into().unwrap());
                        if let Ok(Some(cont)) = gui_tree::container(r, v, i) {
                            if gui_tree::name(r, cont, i).ok().flatten().as_deref() == Some(want) {
                                hits.push(json!(format!("{path}+{off:#x} -> {v:#x}")));
                            }
                        }
                    } else if let Ok(t) = r.string_bounded(obj + off, 64) {
                        if t == text {
                            hits.push(json!(format!("{path}+{off:#x}")));
                        }
                    }
                    let v = u64::from_le_bytes(c.try_into().unwrap());
                    if (0x10000..0x7fff_ffff_0000).contains(&v) && v % 8 == 0 {
                        next.push((v, format!("{path}+{off:#x}->")));
                    }
                }
            }
            frontier = next;
        }
        Ok(json!({"hits": hits, "reads": reads}))
    })
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
        learn_slot(&name);
        if found.is_none() && loc::same(&name, display) {
            found = Some(n);
        }
        seen.push(name);
    }
    Ok((seen, found))
}
fn row_rect(r: &Reader<'_>, i: u64, display: &str, button: &str) -> Result<gui_tree::Rect> {
    let (_, found) = find_row(r, i, display)?;
    let index =
        found.ok_or_else(|| Error::new("gui_target_unavailable", "Organisation row not found"))?;
    let frame = list_frames(r, i)?[index];
    pk::rect_of(r, i, frame, button, 2)
}
/// The organisation row named `display` drawn in the open roster. The list draws about five rows
/// at a time (pr3, 17 Nov 2026: the materiel roster showed its first five, and Mauser and
/// Rheinmetall further down were refused as not offered): when the name is not drawn the list is
/// wheeled down by the rows drawn less one (one wheel event is about one row) and read again,
/// until the name shows or a page brings nothing new. The list is left where the row shows.
///
/// Returns None when the row is drawn (readable rows), or Some(on-screen index) when the rows
/// could not be read and the detail-window path found it (its detail window is left open).
fn require_row(
    process: &dyn Process,
    keys: &dyn GameInput,
    display: &str,
) -> std::result::Result<Option<usize>, Fail> {
    let mut all: Vec<String> = Vec::new();
    let mut empty_reads = 0;
    for _ in 0..16 {
        let (seen, found) = settled(process, "organisation rows", |r, i| find_row(r, i, display))?;
        if found.is_some() {
            return Ok(None);
        }
        if seen.is_empty() && all.is_empty() {
            // The roster opened but no row is readable (pr3b, 17 Nov 2026: the plane and materiel
            // rosters showed "no rows" after the tank and ship rosters had been read): wait a
            // moment for the rows, then find the row through the detail windows instead.
            empty_reads += 1;
            if empty_reads >= 4 {
                return row_by_details(process, keys, display).map(Some);
            }
            pause(400);
            continue;
        }
        let before = all.len();
        for name in &seen {
            if !all.contains(name) {
                all.push(name.clone());
            }
        }
        if seen.is_empty() || all.len() == before {
            break;
        }
        let list = read(process, list_rect)?;
        let centre = (
            f64::from(list.x + list.w / 2),
            f64::from(list.y + list.h / 2),
        );
        let page = seen.len().saturating_sub(1).max(1) as u32;
        keys.scroll_burst(pk::GAME, centre, -ROW_HEIGHT, page)?;
        pause(500);
    }
    Err(refuse(
        "organization_not_offered",
        format!(
            "The organisation list shows {} but not {display:?}; nothing was changed",
            if all.is_empty() {
                "no rows".to_string()
            } else {
                all.join(" | ")
            }
        ),
    ))
}

/// Rows of the organisation list as drawn (industrial_organization_list.gui: rows 100 high,
/// details_button at 400,50 and background_button over the row): the smooth listbox keeps its
/// rows outside the window tree on 1.19.4.1, so they are found by opening each row's details
/// and reading the detail window's name (industrial_org_name). Returns the row index with its
/// detail window left open.
const ROW_HEIGHT: i32 = 100;
fn list_rect(r: &Reader<'_>, i: u64) -> Result<gui_tree::Rect> {
    let root = pk::open_root(r, i, &pk::MIO_LIST)?;
    let list = pk::need(r, i, root, "organisation_list", 4)?;
    if let Ok(rect) = gui_tree::rect(r, i, list, pk::GAME) {
        return Ok(rect);
    }
    // The smooth listbox has no reviewed renderer: its parent window `organisations` plus the
    // list's own .gui placement (position 10,7, size 515x552).
    // Anchored on the close button (a plain button, x = 550 - 38, y = 4 in the 550-wide window):
    // the list sits at (0,85) + (10,7) inside the window, 515x552.
    let close = pk::need(r, i, root, "close_button", 3)?;
    let c = gui_tree::rect(r, i, close, pk::GAME)?;
    let (x0, y0) = (c.x - 512, c.y - 4);
    Ok(gui_tree::Rect {
        x: x0 + 10,
        y: y0 + 92,
        w: 515,
        h: 552,
    })
}
fn detail_name(r: &Reader<'_>, i: u64) -> Result<String> {
    let root = pk::open_root(r, i, &pk::MIO_DETAIL)?;
    if let Ok(Some(t)) = pk::text(r, i, root, "industrial_org_name", 3) {
        if !t.trim().is_empty() {
            return Ok(t);
        }
    }
    let w = pk::need(r, i, root, "industrial_org_name", 3)?;
    super::navalui::edit_text(r, i, w)
}
fn close_detail(process: &dyn Process, keys: &dyn GameInput) {
    if pk::open_now(process, &pk::MIO_DETAIL) {
        let clicked = pk::click(process, keys, "close detail", |r, i| {
            let root = pk::open_root(r, i, &pk::MIO_DETAIL)?;
            pk::rect_of(r, i, root, "close_button", 4)
        });
        if clicked.is_err() {
            pk::dismiss_window(process, keys, &pk::MIO_DETAIL);
        }
        let _ = wait(process, "detail closed", |r, i| {
            Ok(!pk::is_open(r, i, &pk::MIO_DETAIL)?)
        });
    }
}
fn row_by_details(
    process: &dyn Process,
    keys: &dyn GameInput,
    display: &str,
) -> std::result::Result<usize, Fail> {
    let list = read(process, list_rect)?;
    // The list shows about five rows; a roster holds up to a dozen or more (the materiel roster
    // of Germany lists Carl Walther sixth, live 17 Nov 2026). One wheel event moves the list by
    // about one row (scroll_wheel_factor 98, rows 100 high: pp1, 17 Nov 2026). Each page is read
    // row by row through the detail window; the list is then wheeled down by the rows read less
    // one, and a page whose first row is still the old one had its wheel events lost (the detail
    // window was closing) and is wheeled again once. The returned index is the row's place ON
    // SCREEN (the list is left where it found the row).
    let centre = (
        f64::from(list.x + list.w / 2),
        f64::from(list.y + list.h / 2),
    );
    // The game keeps each roster's scroll, and one wheel event moves 98 px for 100 px rows, so
    // the list can sit part of a row off and the clicks below (row top + 62) land between rows
    // (pp3, 17 Nov 2026: the materiel roster drawn half a row down, "no rows" read). Wheel to
    // the top first so the rows line up with the list's top edge.
    close_detail(process, keys);
    keys.scroll_burst(pk::GAME, centre, ROW_HEIGHT, 40)?;
    pause(600);
    let mut seen: Vec<String> = Vec::new();
    let mut previous_first: Option<String> = None;
    let mut retried = false;
    for _page in 0..8 {
        let mut page: Vec<String> = Vec::new();
        for k in 0..12 {
            let top = list.y + ROW_HEIGHT * k;
            if top + 70 > list.y + list.h {
                break;
            }
            close_detail(process, keys);
            let mut opened = false;
            // a first click right after the list moved can be lost: one retry
            for _ in 0..2 {
                keys.click(pk::GAME, f64::from(list.x + 445), f64::from(top + 62))?;
                if wait(process, "organisation detail", |r, i| {
                    pk::is_open(r, i, &pk::MIO_DETAIL)
                })
                .is_ok()
                {
                    opened = true;
                    break;
                }
            }
            if !opened {
                break; // no row there
            }
            pause(250);
            let name = read(process, detail_name).unwrap_or_default();
            learn_slot(&name);
            if loc::same(&name, display) {
                return Ok(k as usize);
            }
            page.push(name);
        }
        let fresh: Vec<String> = page
            .iter()
            .filter(|n| !n.is_empty() && !seen.contains(n))
            .cloned()
            .collect();
        let unmoved = previous_first.is_some() && previous_first.as_ref() == page.first();
        if fresh.is_empty() && !(unmoved && !retried) {
            break;
        }
        retried = unmoved;
        previous_first = page.first().cloned();
        seen.extend(fresh);
        close_detail(process, keys);
        pause(250);
        let rows = page.len().saturating_sub(1).max(1) as u32;
        keys.scroll_burst(pk::GAME, centre, -ROW_HEIGHT, rows)?;
        pause(500);
    }
    close_detail(process, keys);
    Err(refuse(
        "organization_not_offered",
        format!(
            "The organisation list shows {} but not {display:?}; nothing was changed",
            if seen.is_empty() {
                "no rows".to_string()
            } else {
                seen.join(" | ")
            }
        ),
    ))
}
fn rows_readable(process: &dyn Process) -> bool {
    read(process, |r, i| Ok(!list_frames(r, i)?.is_empty())).unwrap_or(false)
}
/// The open policy window's root: its own class path, else a descendant of the detail window
/// named industrial_organisation_policy_window (it opens inside the detail window).
fn policy_root(r: &Reader<'_>, i: u64) -> Result<u64> {
    if let Ok(root) = pk::open_root(r, i, &pk::MIO_POLICY) {
        return Ok(root);
    }
    // the window drawn as a member of the design-company views: neither its class lookup nor a
    // depth-6 walk of the detail window reached it live (v65i, 23 Nov 2026): its shown root by name
    if let Some(root) = pk::shown_root_search(r, i, pk::MIO_POLICY.root_name)? {
        return Ok(root);
    }
    let detail = pk::open_root(r, i, &pk::MIO_DETAIL)?;
    let w = pk::find(r, i, detail, "industrial_organisation_policy_window", 12)?
        .ok_or_else(|| Error::new("gui_target_unavailable", "Policy window not open"))?;
    let root = gui_tree::container(r, w, i)?
        .ok_or_else(|| Error::new("invalid_gui", "Policy window is not a window"))?;
    if !gui_tree::shown(r, w)? {
        return Err(Error::new(
            "gui_target_unavailable",
            "Policy window not open",
        ));
    }
    Ok(root)
}
fn open_detail(
    process: &dyn Process,
    keys: &dyn GameInput,
    display: &str,
) -> std::result::Result<(), Fail> {
    if !rows_readable(process) {
        row_by_details(process, keys, display)?;
        pause(300);
        return Ok(());
    }
    // the drawn rows are about five; require_row wheels the list down until the row shows
    if require_row(process, keys, display)?.is_some() {
        // found through its detail window, which is open now
        pause(300);
        return Ok(());
    }
    pk::click(process, keys, "organisation details", |r, i| {
        row_rect(r, i, display, "details_button")
    })?;
    wait(process, "open organisation detail window", |r, i| {
        pk::is_open(r, i, &pk::MIO_DETAIL)
    })?;
    pause(400);
    Ok(())
}

/// Open the organisation's detail window from the roster of `slot`, else from the roster of any
/// other organisation slot: each slot opens the roster of one kind (tanks, ships, planes,
/// materiel), and which kind holds an organisation is the game's business, not the plan's.
fn open_org(
    process: &dyn Process,
    keys: &dyn GameInput,
    slot: usize,
    display: &str,
) -> std::result::Result<usize, Fail> {
    open_system(process, keys, slot)?;
    // The slot count is read once the politics view has settled; a read taken while it was still
    // drawing gave one slot, so only the tank roster was searched (pp5, 17 Nov 2026: Rheinmetall
    // "not offered" from the tank roster right after a listing had found it in another slot).
    let count = slot_count(process, slot, display);
    let mut first: Option<Fail> = None;
    for (n, s) in slot_order(slot, count, display).into_iter().enumerate() {
        if n > 0 {
            leave(process, keys);
            open_system(process, keys, s)?;
        }
        open_roster(process, keys, s)?;
        match open_detail(process, keys, display) {
            Ok(()) => return Ok(s),
            Err(Fail::Refused(e)) if e.code == "organization_not_offered" => {
                fold_refusal(&mut first, s, e);
            }
            Err(other) => return Err(other),
        }
    }
    Err(first
        .unwrap_or_else(|| refuse("organization_not_offered", "No organisation slot offers it")))
}

/// Organisation slots of the politics view, read after it settles; never fewer than the asked
/// slot or the slot a roster read showed the organisation in.
fn slot_count(process: &dyn Process, slot: usize, display: &str) -> usize {
    settled(process, "organisation slots", |r, i| {
        Ok(org_entries(r, i)?.len())
    })
    .unwrap_or(1)
    .max(slot + 1)
    .max(known_slot(display).map_or(0, |s| s + 1))
}

/// Fold one slot's "not offered" refusal into the refusal for every slot tried, so the final
/// message names what each roster showed (pr3: the message named only the tank roster).
fn fold_refusal(all: &mut Option<Fail>, slot: usize, e: Error) {
    match all {
        Some(Fail::Refused(prev)) => {
            prev.message = format!("{}; slot {slot}: {}", prev.message, e.message);
        }
        _ => {
            let mut e = e;
            e.message = format!("slot {slot}: {}", e.message);
            *all = Some(Fail::Refused(e));
        }
    }
}

fn leave(process: &dyn Process, keys: &dyn GameInput) {
    for class in [&pk::MIO_POLICY, &pk::MIO_DETAIL, &pk::MIO_LIST] {
        if pk::open_now(process, class) {
            let clicked = pk::click(process, keys, "close window", |r, i| {
                let root = pk::open_root(r, i, class)?;
                pk::rect_of(r, i, root, "close_button", 4)
            });
            if clicked.is_err() {
                pk::dismiss_window(process, keys, class);
            } else {
                let _ = wait(process, "window closed", |r, i| {
                    Ok(!pk::is_open(r, i, class)?)
                });
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

fn design_team(process: &dyn Process, keys: &dyn GameInput, display: &str, slot: usize) -> Driven {
    let result: Driven = (|| {
        // Each slot of the politics view is the roster of one kind (tanks, ships, planes,
        // materiel: pp1, 17 Nov 2026, the first slot's tooltip "for Tanks"); an organisation
        // can only join the slot of its kind, so the asked slot comes first and every other
        // slot is tried after it (pr2 asked every organisation in slot 0).
        open_system(process, keys, slot)?;
        let count = slot_count(process, slot, display);
        let before = political_power(process);
        let mut found: Option<Option<usize>> = None;
        let mut first: Option<Fail> = None;
        for (n, s) in slot_order(slot, count, display).into_iter().enumerate() {
            if n > 0 {
                leave(process, keys);
                open_system(process, keys, s)?;
            }
            open_roster(process, keys, s)?;
            let attempt = if rows_readable(process) {
                require_row(process, keys, display)
            } else {
                row_by_details(process, keys, display).map(Some)
            };
            match attempt {
                Ok(k) => {
                    found = Some(k);
                    break;
                }
                Err(Fail::Refused(e)) if e.code == "organization_not_offered" => {
                    fold_refusal(&mut first, s, e);
                }
                Err(other) => return Err(other),
            }
        }
        let Some(on_screen) = found else {
            return Err(first.unwrap_or_else(|| {
                refuse("organization_not_offered", "No organisation slot offers it")
            }));
        };
        if let Some(k) = on_screen {
            close_detail(process, keys);
            let list = read(process, list_rect)?;
            keys.click(
                pk::GAME,
                f64::from(list.x + 200),
                f64::from(list.y + ROW_HEIGHT * k as i32 + 30),
            )?;
        } else {
            pk::click(process, keys, "organisation row", |r, i| {
                row_rect(r, i, display, "background_button")
            })?;
        }
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

// ------------------------------------------------------------ line manufacturer

/// Close the organisation windows only (the production view stays).
fn close_lists(process: &dyn Process, keys: &dyn GameInput) {
    for class in [&pk::MIO_POLICY, &pk::MIO_DETAIL, &pk::MIO_LIST] {
        if pk::open_now(process, class) {
            let clicked = pk::click(process, keys, "close window", |r, i| {
                let root = pk::open_root(r, i, class)?;
                pk::rect_of(r, i, root, "close_button", 4)
            });
            if clicked.is_err() {
                pk::dismiss_window(process, keys, class);
            }
            let _ = wait(process, "window closed", |r, i| {
                Ok(!pk::is_open(r, i, class)?)
            });
        }
    }
}

/// The roster a production line's organisation button opens. The production view keeps its own
/// COrganisationListWindow (macOS 1.19.4.1, pp13 17 Nov 2026: embedded at view +0x2030; its root
/// container at +0xf0 is made the first time the roster opens). Its `organisation_list` is a
/// smooth listbox whose rows are not GUI children: the listbox object (the widget itself) keeps
/// a linked list of its COrganisationListItem objects, head node at +0xa0 and the count at
/// +0xb0, node = {item, prev, next}; each item's row window is at item +0x58 (name,
/// details_button, background_button). The first row, "Produce without a Military Industrial
/// Organization", has no details button: clicking it removes the line's organisation.
const LINE_ROSTER_AT: u64 = 0x2030;
/// How far into the production view the roster is looked for on a build where it moved.
const LINE_ROSTER_SPAN: u64 = 0x10000;
const ROSTER_ROOT: u64 = 0xf0;
const LISTBOX_VTABLE: u64 = 0x36cbb20;
const LISTBOX_ITEMS: u64 = 0xa0;
const LISTBOX_COUNT: u64 = 0xb0;
const NODE_NEXT: u64 = 0x10;
const ITEM_FRAME: u64 = 0x58;
/// Rows drawn in full in the roster's 552 px list at ROW_HEIGHT each.
const ROSTER_ROWS_DRAWN: usize = 5;

#[derive(Debug, Clone, PartialEq)]
struct RosterRow {
    name: String,
    /// An organisation row (it has a details button); false for the "produce without" row.
    organisation: bool,
    rect: Option<gui_tree::Rect>,
}
#[derive(Debug, Clone, PartialEq)]
struct LineRoster {
    open: bool,
    close: Option<gui_tree::Rect>,
    list: Option<gui_tree::Rect>,
    rows: Vec<RosterRow>,
}

fn roster_object(r: &Reader<'_>, i: u64) -> Result<Option<u64>> {
    let view = super::production::view(r, i)?;
    let want = i + img(r.string_abi(), pk::MIO_LIST.vtable)?;
    if r.u64(view + LINE_ROSTER_AT).ok() == Some(want) {
        return Ok(Some(view + LINE_ROSTER_AT));
    }
    // another build: the first embedded roster whose root is laid out
    let mut at = 0u64;
    while at < LINE_ROSTER_SPAN {
        if let Ok(bytes) = r.read(view + at, 0x400) {
            for (k, c) in bytes.chunks_exact(8).enumerate() {
                let obj = view + at + k as u64 * 8;
                if u64::from_le_bytes(c.try_into().unwrap()) == want
                    && r.u64(obj + ROSTER_ROOT).unwrap_or(0) != 0
                {
                    return Ok(Some(obj));
                }
            }
        }
        at += 0x400;
    }
    Ok(None)
}

/// The production roster as drawn: None until it has been opened once on this game.
fn line_roster(r: &Reader<'_>, i: u64) -> Result<Option<LineRoster>> {
    let Some(obj) = roster_object(r, i)? else {
        return Ok(None);
    };
    let p = r.u64(obj + ROSTER_ROOT)?;
    if p == 0 {
        return Ok(None);
    }
    let Some(root) = gui_tree::container(r, p, i)? else {
        return Ok(None);
    };
    let open = match pk::find(r, i, root, "organisations", 1)? {
        Some(w) => gui_tree::shown(r, w)?,
        None => false,
    };
    let close = pk::find(r, i, root, "close_button", 3)?
        .and_then(|c| gui_tree::rect(r, i, c, pk::GAME).ok());
    let list = pk::need(r, i, root, "organisation_list", 3)?;
    if r.u64(list)? != i + img(r.string_abi(), LISTBOX_VTABLE)? {
        return Err(Error::new(
            "roster_unreadable",
            "The production roster's organisation_list is not the reviewed smooth listbox",
        ));
    }
    let count = r.u64(list + LISTBOX_COUNT)?;
    if count > 64 {
        return Err(Error::new(
            "roster_unreadable",
            "Roster row count out of bounds",
        ));
    }
    let mut rows = Vec::new();
    let mut node = r.u64(list + LISTBOX_ITEMS)?;
    for _ in 0..count {
        let item = r.u64(node)?;
        let frame = gui_tree::container(r, r.u64(item + ITEM_FRAME)?, i)?
            .ok_or_else(|| Error::new("roster_unreadable", "Roster row window not found"))?;
        let name = pk::text(r, i, frame, "name", 2)?.unwrap_or_default();
        let organisation = match pk::find(r, i, frame, "details_button", 2)? {
            Some(d) => gui_tree::shown(r, d)?,
            None => false,
        };
        let rect = pk::find(r, i, frame, "background_button", 2)?
            .and_then(|b| gui_tree::rect(r, i, b, pk::GAME).ok());
        rows.push(RosterRow {
            name,
            organisation,
            rect,
        });
        node = r.u64(node + NODE_NEXT)?;
    }
    // The list's place in the 550-wide window, anchored on the close button (see list_rect).
    let list_at = close.map(|c| gui_tree::Rect {
        x: c.x - 512 + 10,
        y: c.y - 4 + 92,
        w: 515,
        h: 552,
    });
    Ok(Some(LineRoster {
        open,
        close,
        list: list_at,
        rows,
    }))
}

/// Where row `k` of a roster wheeled to its top is clicked: on its background (not its details
/// button). None when the row is not drawn in full.
fn roster_row_point(list: gui_tree::Rect, k: usize) -> Option<(f64, f64)> {
    (k < ROSTER_ROWS_DRAWN && (k as i32 + 1) * ROW_HEIGHT <= list.h).then(|| {
        (
            f64::from(list.x + 200),
            f64::from(list.y + ROW_HEIGHT * k as i32 + 30),
        )
    })
}

/// The row to take: the organisation named `display`, else the first organisation offered.
fn roster_pick(rows: &[RosterRow], display: Option<&str>) -> std::result::Result<usize, String> {
    let offered: Vec<&str> = rows
        .iter()
        .filter(|r| r.organisation)
        .map(|r| r.name.as_str())
        .collect();
    let found = rows
        .iter()
        .position(|r| r.organisation && display.is_none_or(|d| loc::same(&r.name, d)));
    found.ok_or_else(|| {
        if offered.is_empty() {
            "the roster offers no organisation for this equipment".to_string()
        } else {
            format!("offered: {}", offered.join(", "))
        }
    })
}

fn close_line_roster(process: &dyn Process, keys: &dyn GameInput) {
    let open = |r: &Reader<'_>, i: u64| Ok(line_roster(r, i)?.is_some_and(|x| x.open));
    if read(process, open).unwrap_or(false) {
        let _ = pk::click(process, keys, "close organisation roster", |r, i| {
            line_roster(r, i)?
                .and_then(|x| x.close)
                .ok_or_else(|| Error::new("gui_target_unavailable", "Roster close button"))
        });
        let _ = wait(process, "organisation roster closed", |r, i| {
            Ok(!open(r, i)?)
        });
    }
}

/// The game's button state for a disabled button (gui_tree::button_state).
const BUTTON_DISABLED: i32 = 3;
/// Room below the drawn button that must be on screen too: a button whose square ended at
/// 1078 (the list's bottom) opened nothing (g17e, convoy line drawn at 203,1072 6x6).
const DRAWN_MARGIN: i32 = 16;
/// Where a button drawn at LINE_BUTTON_SCALE shows inside its full-size rect: the scaled
/// square at the rect's bottom-left corner.
const LINE_BUTTON_SCALE: f64 = 0.45;
fn logo_square(rect: gui_tree::Rect) -> gui_tree::Rect {
    let side = ((f64::from(rect.h.min(rect.w)) * LINE_BUTTON_SCALE).round() as i32).max(1);
    gui_tree::Rect {
        x: rect.x,
        y: rect.y + rect.h - side,
        w: side,
        h: side,
    }
}

/// Attach an organisation to an own production line. The politics view's organisation slots
/// only browse the rosters (live pp4, 17 Nov 2026: a row click there changed nothing): the game
/// assigns an organisation from a task, and for production that is the line's
/// `attach_manufacturer` button, which opens the roster of the organisations able to make that
/// equipment. A production line costs no political power (ASSIGN_INDUSTRIAL_MANUFACTURER_PP_COST_
/// PER_DAY 0) and task capacity is off (ENABLE_TASK_CAPACITY false) in 1.19.4.1, so any offered
/// row can take the line. `display` None takes the first organisation the roster offers. The
/// rows are read from memory before any row click, and a row is clicked only when it is an
/// organisation row drawn in full with the list wheeled to its top. Verified by the line's own
/// row turning to the attached state (`has_mio_background`).
fn line_assign(
    process: &dyn Process,
    keys: &dyn GameInput,
    display: Option<&str>,
    handle: u64,
    list_only: bool,
) -> Driven {
    let result: Driven = (|| {
        if pk::open_now(process, &pk::POLITICS_VIEW) {
            leave(process, keys);
        }
        super::production::ensure_open(process, keys)?;
        pause(300);
        super::production::reveal_named(process, keys, handle, "attach_manufacturer", 20)?;
        let before = settled(process, "line organisation", |r, i| {
            super::production::line_mio(r, i, handle)?.ok_or_else(|| {
                Error::new("gui_target_unavailable", "Production line row not laid out")
            })
        })?;
        if !before.can_attach {
            return Err(refuse(
                "line_takes_no_organization",
                "The line shows no organisation button: no organisation makes this equipment",
            ));
        }
        if before.attached && !list_only {
            return Ok(json!({"assigned": true, "already_attached": true}));
        }
        if !before.attached && before.button_state == Some(BUTTON_DISABLED) {
            // pp15, 17 Nov 2026: Germany's convoy line shows the button, the click opens
            // nothing, and the game's tooltip says the country has no organisation for this
            // equipment type.
            return Err(refuse(
                "line_takes_no_organization",
                "The line's organisation button is disabled: the game has no organisation of this \
                 country for this equipment type",
            ));
        }
        close_line_roster(process, keys);
        let button = if before.attached {
            "change_manufacturer"
        } else {
            "attach_manufacturer"
        };
        // The last line cannot be wheeled higher than the screen's bottom edge (a new line is
        // always last), and its drawn button is then cut by the edge (pp14, 17 Nov 2026: drawn at
        // 203,1076 6x6, the click opened nothing). Such a line is moved to the top for the
        // roster and back to the bottom afterwards, so the line order is as before.
        let drawn_on_screen = |r: &Reader<'_>, i: u64| {
            Ok(
                super::production::shown_button(r, i, handle, button)?.is_some_and(|b| {
                    let mut drawn = logo_square(b);
                    drawn.h += DRAWN_MARGIN;
                    super::production::in_list(drawn)
                }),
            )
        };
        if !read(process, drawn_on_screen)? {
            let last = read(process, |r, i| {
                Ok(super::production::shown_order(r, i)?.last() == Some(&handle))
            })?;
            if !last {
                return Err(refuse(
                    "row_not_visible",
                    "The line's organisation button is not wholly on screen",
                ));
            }
            super::production::prioritize(process, keys, handle, super::production::Move::Top)?;
            MOVED_LINE.store(handle, std::sync::atomic::Ordering::Relaxed);
            super::production::reveal_named(process, keys, handle, button, 20)?;
            if !read(process, drawn_on_screen)? {
                return Err(refuse(
                    "row_not_visible",
                    "The line's organisation button is not wholly on screen at the top",
                ));
            }
        }

        // The button is drawn at scale 0.45 (countryproductionlineview.gui) while its rect keeps
        // the sprite's full size, and the drawing shrinks toward the rect's bottom-left corner
        // (pp8, 17 Nov 2026: rect 203,338 63x63, logo drawn about 216,386; the rect's centre
        // missed it). Only the drawn square is clicked: it opens the roster and changes nothing.
        let rect = pk::click(process, keys, "line organisation button", |r, i| {
            let rect = super::production::shown_button(r, i, handle, button)?.ok_or_else(|| {
                Error::new("gui_target_unavailable", "Organisation button not shown")
            })?;
            Ok(logo_square(rect))
        })?;
        wait(process, "open organisation roster", |r, i| {
            Ok(line_roster(r, i)?.is_some_and(|x| x.open))
        })
        .map_err(|e| {
            Error::new(
                e.code,
                format!(
                    "{} (drawn button {},{} {}x{}, button state {})",
                    e.message,
                    rect.x,
                    rect.y,
                    rect.w,
                    rect.h,
                    before
                        .button_state
                        .map_or_else(|| "unread".to_string(), |s| s.to_string())
                ),
            )
        })?;
        pause(300);
        let roster = read(process, |r, i| {
            line_roster(r, i)?.ok_or_else(|| Error::new("roster_unreadable", "Roster not laid out"))
        })?;
        let offered: Vec<String> = roster
            .rows
            .iter()
            .filter(|x| x.organisation)
            .map(|x| x.name.clone())
            .collect();
        if list_only {
            return Ok(json!({"listing_only": true, "attached": before.attached,
                "offered": offered, "verification": "roster_rows"}));
        }
        if offered.is_empty() {
            // The line shows the button, but no organisation of this country makes the
            // equipment (g17e: Germany's convoy line; no German organisation lists convoys).
            return Err(refuse(
                "line_takes_no_organization",
                "The roster offers no organisation for this equipment (only the 'Produce without' row)",
            ));
        }
        let k = roster_pick(&roster.rows, display)
            .map_err(|why| Fail::Refused(Error::new("organization_not_offered", why)))?;
        let list = roster
            .list
            .ok_or_else(|| Error::new("gui_target_unavailable", "Roster list not placed"))?;
        // wheel the list to its top so the rows sit at their places, then read them again
        let centre = (
            f64::from(list.x + list.w / 2),
            f64::from(list.y + list.h / 2),
        );
        keys.scroll_burst(pk::GAME, centre, ROW_HEIGHT, 40)?;
        pause(500);
        let again = read(process, |r, i| {
            line_roster(r, i)?.ok_or_else(|| Error::new("roster_unreadable", "Roster not laid out"))
        })?;
        let row = again
            .rows
            .get(k)
            .filter(|x| x.organisation && x.name == roster.rows[k].name && again.open)
            .ok_or_else(|| {
                Error::new(
                    "input_readback_failed",
                    "The roster changed before the row click",
                )
            })?;
        let Some((x, y)) = roster_row_point(list, k) else {
            return Err(refuse(
                "row_not_visible",
                format!("{} is row {} of the roster, below the rows drawn in full; attach it in the game", row.name, k + 1),
            ));
        };
        keys.click(pk::GAME, x, y)?;
        let attached = wait(process, "line organisation attached", |r, i| {
            Ok(super::production::line_mio(r, i, handle)?.is_some_and(|m| m.attached))
        })
        .is_ok();
        Ok(json!({"assigned": attached,
            "attached_before": before.attached, "attached_after": attached,
            "row": row.name, "row_index": k, "offered": offered, "verification": "line_row"}))
    })();
    close_line_roster(process, keys);
    close_lists(process, keys);
    let moved = MOVED_LINE.swap(0, std::sync::atomic::Ordering::Relaxed);
    if moved == handle {
        let back =
            super::production::prioritize(process, keys, handle, super::production::Move::Bottom);
        return match (result, back) {
            (Ok(mut v), Ok(())) => {
                v["moved_to_top_and_back"] = json!(true);
                Ok(v)
            }
            (Ok(mut v), Err(e)) => {
                v["moved_to_top_and_back"] = json!(false);
                v["priority_not_restored"] = json!(e.message);
                Ok(v)
            }
            (Err(e), _) => Err(e),
        };
    }
    result
}
/// The line moved to the top for its roster, to be put back last (0: none).
static MOVED_LINE: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

// ------------------------------------------------------------------ policy

fn policy(
    process: &dyn Process,
    keys: &dyn GameInput,
    display: &str,
    slot: usize,
    policy_name: &str,
) -> Driven {
    let result: Driven = (|| {
        open_org(process, keys, slot, display)?;
        pk::click(process, keys, "open policies", |r, i| {
            let root = pk::open_root(r, i, &pk::MIO_DETAIL)?;
            pk::rect_of(r, i, root, "open_policies", 4)
        })?;
        if let Err(e) = wait(process, "open policy window", |r, i| {
            Ok(policy_root(r, i).is_ok())
        }) {
            // Debug aid: HOI4_DEBUG_HOLD keeps the windows open for probing.
            if let Some(secs) = std::env::var("HOI4_DEBUG_HOLD")
                .ok()
                .and_then(|v| v.parse::<u64>().ok())
            {
                crate::phase::sleep(std::time::Duration::from_secs(secs.min(60)));
            }
            return Err(e.into());
        }
        pause(400);
        let wanted = policy_name.to_string();
        let rows = |r: &Reader<'_>, i: u64| -> Result<(Vec<String>, Option<gui_tree::Rect>)> {
            let root = policy_root(r, i)?;
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
    /// The icon button's raw state (0 normal, 2 toggled on, 3 disabled); 2 is read as a trait
    /// the organisation already holds (g17c, 17 Nov 2026: trait 0 of Mauser and Rheinmetall,
    /// queued on the first day, read enabled for months and every click left it unqueued).
    pub state: i32,
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
        let state = match icon {
            Some(b) => gui_tree::button_state(r, b)?,
            None => 3,
        };
        let enabled = match icon {
            Some(b) => gui_tree::shown(r, b)? && state != 3 && state != 2,
            None => false,
        };
        // The queue number keeps its .gui placeholder ("99") while hidden: only a shown
        // number is a queue position (every trait read "99" unqueued, live 6 Nov 2026).
        let shown = pk::find(r, i, entry, "queue_number", 4)?
            .map(|w| gui_tree::shown(r, w))
            .transpose()?
            .unwrap_or(false);
        let queue = if shown {
            pk::text(r, i, entry, "queue_number", 4)?.unwrap_or_default()
        } else {
            String::new()
        };
        let rect = icon.and_then(|b| pk::rect(r, i, b).ok());
        out.push(TraitEntry {
            index,
            enabled,
            queue,
            rect,
            state,
        });
    }
    Ok(out)
}
/// Refusal for a trait click, from the entries on screen; pure.
pub(super) fn trait_refusal(
    entries: &[TraitEntry],
    index: usize,
) -> Option<(&'static str, String)> {
    let Some(e) = entries.get(index) else {
        return Some((
            "trait_unknown",
            format!(
                "The trait tree shows {} entries, not {}",
                entries.len(),
                index + 1
            ),
        ));
    };
    if !e.queue.is_empty() {
        return Some((
            "trait_already_queued",
            format!("Trait {index} is already queued (position {})", e.queue),
        ));
    }
    if e.state == 2 {
        return Some((
            "trait_owned",
            format!("Trait {index} is held already (its icon is drawn toggled on)"),
        ));
    }
    if !e.enabled {
        return Some((
            "trait_unavailable",
            format!(
                "The game disables trait {index} (prerequisites, funds or an excluded sibling)"
            ),
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

fn mio_trait(
    process: &dyn Process,
    keys: &dyn GameInput,
    display: &str,
    slot: usize,
    index: Option<usize>,
) -> Driven {
    let result: Driven = (|| {
        // The slot whose list offered the organisation: the re-open below must use it, not the
        // asked slot (g17a/g17b, 17 Nov 2026: a trait of Rheinmetall, found in another slot's
        // list, was queued, and the re-open through the asked tank slot then answered
        // organization_not_offered, so a queued trait read as a refusal every year).
        let found = open_org(process, keys, slot, display)?;
        let entries = settled(process, "trait entries", trait_entries)?;
        let summary = |entries: &[TraitEntry]| -> Vec<Value> {
            entries
                .iter()
                .map(|e| json!({"index": e.index, "enabled": e.enabled, "queue": e.queue, "visible": e.rect.is_some(), "owned": e.state == 2, "state": e.state}))
                .collect()
        };
        let Some(index) = index else {
            return Ok(
                json!({"listed": summary(&entries), "trait_queued": false, "listing_only": true, "verification": "gui_only"}),
            );
        };
        refusal(trait_refusal(&entries, index))?;
        pk::click(process, keys, "trait icon", |r, i| {
            trait_entries(r, i)?
                .get(index)
                .and_then(|e| e.rect)
                .ok_or_else(|| Error::new("gui_target_unavailable", "Trait icon left the window"))
        })?;
        let confirmed = accept_command(process, keys)?;
        pause(300);
        // The queue number is drawn a moment after the click (a queued trait read unqueued at
        // 600 ms, live 6 Nov 2026): poll up to 2 s.
        let deadline = std::time::Instant::now() + Duration::from_millis(2000);
        let mut after = read(process, trait_entries).unwrap_or_default();
        while !after.get(index).is_some_and(|e| !e.queue.is_empty())
            && std::time::Instant::now() < deadline
        {
            pause(150);
            after = read(process, trait_entries).unwrap_or_default();
        }
        if after.is_empty() {
            // Queueing a trait detaches the detail window from the list window (window_not_found
            // right after a click that did queue, live 6 Nov 2026): reopen it and read again.
            leave(process, keys);
            open_system(process, keys, found)?;
            open_roster(process, keys, found)?;
            open_detail(process, keys, display)?;
            after = read(process, trait_entries).unwrap_or_default();
        }
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

/// The traits `auto` queues: enabled, visible, not queued and not held entries, in the tree's
/// order, until `depth` enabled entries wait in the queue (a queued entry the game shows disabled
/// does not count: it is never unlocked and holds the points idle). `entries` = (index, enabled,
/// queued, owned, visible). Pure.
pub(super) fn auto_picks(entries: &[(usize, bool, bool, bool, bool)], depth: usize) -> Vec<usize> {
    let waiting = entries
        .iter()
        .filter(|(_, enabled, queued, owned, _)| *enabled && *queued && !*owned)
        .count();
    entries
        .iter()
        .filter(|(_, enabled, queued, owned, visible)| *enabled && !*queued && !*owned && *visible)
        .map(|(index, ..)| *index)
        .take(depth.saturating_sub(waiting))
        .collect()
}

/// Several traits queued in ONE visit of the organisation's tree window (the game's completion
/// queue unlocks them in order as points come): the `asked` indices in order, then with `auto`
/// the first open entries until that many wait in the queue. Each click is read back from the
/// entry's queue number; a trait already queued counts as reached; a refused one is listed with
/// its reason and the rest go on.
fn mio_traits_many(
    process: &dyn Process,
    keys: &dyn GameInput,
    display: &str,
    slot: usize,
    asked: &[usize],
    auto: Option<usize>,
) -> Driven {
    let result: Driven = (|| {
        let found = open_org(process, keys, slot, display)?;
        let mut entries = settled(process, "trait entries", trait_entries)?;
        let flags = |entries: &[TraitEntry]| -> Vec<(usize, bool, bool, bool, bool)> {
            entries
                .iter()
                .map(|e| {
                    (
                        e.index,
                        e.enabled,
                        !e.queue.is_empty(),
                        e.state == 2,
                        e.rect.is_some(),
                    )
                })
                .collect()
        };
        let mut wanted: Vec<usize> = asked.to_vec();
        if let Some(depth) = auto {
            for i in auto_picks(&flags(&entries), depth) {
                if !wanted.contains(&i) {
                    wanted.push(i);
                }
            }
        }
        let summary = |entries: &[TraitEntry]| -> Vec<Value> {
            entries
                .iter()
                .map(|e| json!({"index": e.index, "enabled": e.enabled, "queue": e.queue, "visible": e.rect.is_some(), "owned": e.state == 2}))
                .collect()
        };
        if wanted.is_empty() {
            return Ok(
                json!({"nothing_to_queue": true, "changed": 0, "entries": summary(&entries),
                "verification": "gui_only"}),
            );
        }
        let mut queued: Vec<usize> = Vec::new();
        let mut unchanged: Vec<Value> = Vec::new();
        for index in wanted.iter().copied() {
            if entries.is_empty() {
                // a queued trait can detach the detail window (mio_trait): open it again
                leave(process, keys);
                open_system(process, keys, found)?;
                open_roster(process, keys, found)?;
                open_detail(process, keys, display)?;
                entries = settled(process, "trait entries", trait_entries)?;
            }
            if let Some((code, why)) = trait_refusal(&entries, index) {
                if code == "trait_already_queued" {
                    queued.push(index);
                } else {
                    unchanged.push(json!({"index": index, "code": code, "why": why}));
                }
                continue;
            }
            pk::click(process, keys, "trait icon", |r, i| {
                trait_entries(r, i)?
                    .get(index)
                    .and_then(|e| e.rect)
                    .ok_or_else(|| {
                        Error::new("gui_target_unavailable", "Trait icon left the window")
                    })
            })?;
            accept_command(process, keys)?;
            let deadline = std::time::Instant::now() + Duration::from_millis(2000);
            entries = read(process, trait_entries).unwrap_or_default();
            while !entries.is_empty()
                && !entries.get(index).is_some_and(|e| !e.queue.is_empty())
                && std::time::Instant::now() < deadline
            {
                pause(150);
                entries = read(process, trait_entries).unwrap_or_default();
            }
            if entries.is_empty() {
                leave(process, keys);
                open_system(process, keys, found)?;
                open_roster(process, keys, found)?;
                open_detail(process, keys, display)?;
                entries = settled(process, "trait entries", trait_entries)?;
            }
            if entries.get(index).is_some_and(|e| !e.queue.is_empty()) {
                queued.push(index);
            } else {
                unchanged.push(json!({"index": index, "code": "action_not_applied",
                    "why": "no queue number after the click"}));
            }
        }
        Ok(json!({
            "trait_queued": unchanged.is_empty(),
            "some_queued": !queued.is_empty(),
            "queued": queued,
            "changed": queued.len(),
            "unchanged": unchanged,
            "entries": summary(&entries),
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
        return Err(Error::new(
            "invalid_arguments",
            format!("Invalid {name} key"),
        ));
    }
    Ok(v)
}

/// engineer 84: one organisation change in process (family `mio`). The posts run in the journal's dispatch, one module
/// request each (the module checks the own organisation, the tree, the policy window's gate and the type's own "can
/// do" before each post); `held` judges the module's fresh organisations read (and the replies) after the settle
/// window. A game refusal of the first post is the typed error (the mouse would send the same refused click); a
/// module refusal of its own (nothing posted) runs the mouse path `gui` in the same call.
#[allow(clippy::too_many_arguments)]
fn run_in_process(
    kind: &'static str,
    action: Value,
    id: &str,
    token: &str,
    process: &(dyn Process + Send + Sync),
    keys: &dyn GameInput,
    guard: &mut Guard,
    authority: &InputAuthority,
    actions: &mut super::journal::GameJournal,
    ch: super::inproc::Channel,
    posts: Vec<(Value, String)>,
    held: &dyn Fn(&super::inproc_mio::Mios, &[Value]) -> Option<bool>,
    gui: &dyn Fn(&dyn Process, &dyn GameInput) -> Driven,
) -> Result<Value> {
    use super::journal::Outcome;
    guard.observe_context(process)?;
    let epoch = guard.epoch.clone();
    authority.authorize(token, &epoch)?;
    let ch = RefCell::new(ch);
    let replies: RefCell<Vec<Value>> = RefCell::new(Vec::new());
    let gui_seen: RefCell<Option<Value>> = RefCell::new(None);
    let posted_at: std::cell::Cell<Option<std::time::Instant>> = std::cell::Cell::new(None);
    let mut settle = || -> Result<Outcome> {
        let (m, _) = super::inproc_mio::read(&mut ch.borrow_mut())?;
        let rs = replies.borrow();
        let detail = json!({"tool":kind,"backend":"inprocess","replies":rs.len()});
        let early = posted_at
            .get()
            .is_some_and(|t| t.elapsed() < super::inproc::IPC_SETTLE);
        Ok(match held(&m, &rs) {
            Some(true) => Outcome::Reached(detail),
            _ if early => Outcome::Unknown,
            Some(false) => Outcome::Unchanged(detail),
            None => Outcome::Diverged(detail),
        })
    };
    let mut dispatch = || -> Result<()> {
        authority.authorize(token, &epoch)?;
        for (k, (req, what)) in posts.iter().enumerate() {
            let reply = ch.borrow_mut().post_raw(req.clone())?;
            match super::inproc_units::routed(reply, what) {
                super::inproc::Routed::Done(v) => {
                    replies.borrow_mut().push(v);
                    posted_at.set(Some(std::time::Instant::now()));
                }
                super::inproc::Routed::Refused(w) if k == 0 => {
                    return Err(super::inproc::refused_error(&w))
                }
                super::inproc::Routed::Refused(w) => {
                    replies.borrow_mut().push(json!({"refused_by_game_check":w}))
                }
                super::inproc::Routed::Gui if k == 0 => {
                    keys.preflight()?;
                    return match gui(process, keys) {
                        Ok(seen) => {
                            *gui_seen.borrow_mut() = Some(seen);
                            Ok(())
                        }
                        Err(Fail::Refused(e)) | Err(Fail::Failed(e)) => Err(e),
                    };
                }
                super::inproc::Routed::Gui => {
                    return Err(Error::new(
                        "action_uncertain",
                        "The module refused a later command of this call (nothing of it posted); the earlier ones were posted",
                    ))
                }
            }
        }
        Ok(())
    };
    let receipt = actions.execute(id, &epoch, action, &mut settle, &mut dispatch)?;
    Ok(
        json!({"receipt":receipt,"tool":kind,"validated":false,"observed":gui_seen.into_inner(),
        "in_process":replies.into_inner()}),
    )
}

/// The `mio` route and the organisation `key` of the module's read: None = the mouse path (no channel, the family not
/// usable in this mode, the read failed, or `key` is no own organisation: the mouse path also takes localised names).
fn mio_route(key: &str) -> Option<(super::inproc::Channel, super::inproc_mio::Mios)> {
    let mut ch = super::inproc::route("mio")?;
    let (m, _) = super::inproc_mio::read(&mut ch).ok()?;
    m.org(key)?;
    Some((ch, m))
}

pub(super) fn call(
    name: &str,
    args: &serde_json::Map<String, Value>,
    process: &(dyn Process + Send + Sync),
    keys: &dyn GameInput,
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
                    Ok(ui_action::effect(
                        seen,
                        "assigned",
                        json!({"tool":"mio_design_team","organization":organization}),
                    ))
                }),
            };
            ui_action::run_bounded(job, 90, id, token, process, keys, guard, authority, actions)
        }
        "hoi4_mio_policy" => {
            super::allowed(
                args,
                &[
                    "organization",
                    "policy",
                    "slot",
                    "request_id",
                    "lease_token",
                ],
            )?;
            let organization = key_arg(args, "organization")?;
            let policy_key = key_arg(args, "policy")?;
            let slot = slot_arg(args)?;
            let action = json!({"kind":"mio_policy","organization":organization,"policy":policy_key,"slot":slot});
            // engineer 84: in process, the policy window's item by its token (the attached one: nothing to post)
            if !actions.receipt_exists(id) {
                if let Some((ch, m)) = mio_route(organization) {
                    if let (Some(org), Some(tok)) = (m.org(organization), m.policy(policy_key)) {
                        let posts = if org.policy == tok {
                            Vec::new()
                        } else {
                            vec![(
                                super::inproc_mio::policy_request(org, tok)?,
                                "this policy now (its cooldown, political power, the policy's own conditions)".to_string(),
                            )]
                        };
                        let held = |m: &super::inproc_mio::Mios, _: &[Value]| {
                            super::inproc_mio::policy_held(m, organization, tok, false)
                        };
                        let gui = |p: &dyn Process, k: &dyn GameInput| -> Driven {
                            let n = names(p, organization, Some(policy_key))?;
                            let shown = n.policy.as_ref().map(|x| x.1.clone()).unwrap_or_default();
                            policy(p, k, &n.org_display, slot, &shown)
                        };
                        return run_in_process(
                            "mio_policy",
                            action,
                            id,
                            token,
                            process,
                            keys,
                            guard,
                            authority,
                            actions,
                            ch,
                            posts,
                            &held,
                            &gui,
                        );
                    }
                }
            }
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
                    Ok(ui_action::effect(
                        seen,
                        "policy_attached",
                        json!({"tool":"mio_policy","organization":organization,"policy":policy_key}),
                    ))
                }),
            };
            ui_action::run_bounded(job, 90, id, token, process, keys, guard, authority, actions)
        }
        "hoi4_mio_trait" => {
            super::allowed(
                args,
                &[
                    "organization",
                    "trait_index",
                    "trait_indices",
                    "auto",
                    "list",
                    "slot",
                    "bulk",
                    "request_id",
                    "lease_token",
                ],
            )?;
            let organization = key_arg(args, "organization")?;
            let slot = slot_arg(args)?;
            let many: Vec<usize> = match args.get("trait_indices") {
                None => Vec::new(),
                Some(v) => {
                    let list = v
                        .as_array()
                        .filter(|l| (1..=16).contains(&l.len()))
                        .ok_or_else(|| {
                            Error::new("invalid_arguments", "trait_indices must list 1..16 indices")
                        })?;
                    let mut out: Vec<usize> = Vec::new();
                    for x in list {
                        let i = x.as_u64().filter(|n| *n <= 63).ok_or_else(|| {
                            Error::new("invalid_arguments", "a trait index must be 0..63")
                        })? as usize;
                        if !out.contains(&i) {
                            out.push(i);
                        }
                    }
                    out
                }
            };
            let auto = match args.get("auto") {
                None => None,
                Some(v) => Some(v.as_u64().filter(|n| (1..=5).contains(n)).ok_or_else(|| {
                    Error::new(
                        "invalid_arguments",
                        "auto must be 1..5 (traits waiting in the queue)",
                    )
                })? as usize),
            };
            if !many.is_empty() || auto.is_some() {
                if args.get("trait_index").is_some() || args.get("list").is_some() {
                    return Err(Error::new(
                        "invalid_arguments",
                        "trait_indices/auto queue several traits; leave out trait_index and list",
                    ));
                }
                // engineer 84: in process, each trait by the tree's click (unlock with a free point, else queue)
                if !actions.receipt_exists(id) {
                    if let Some((ch, m)) = mio_route(organization) {
                        let org = m
                            .org(organization)
                            .expect("mio_route checked the organisation");
                        let mut points = org.points;
                        let mut tokens = Vec::new();
                        let mut posts = Vec::new();
                        let mut picks = many.clone();
                        if let Some(a) = auto {
                            picks.extend(super::inproc_mio::auto_picks(org, a, &many));
                        }
                        for i in picks {
                            match super::inproc_mio::step(org, i, points) {
                                Ok(st) => {
                                    if matches!(st, super::inproc_mio::Step::Unlock(_)) {
                                        points -= 1;
                                    }
                                    let t = match st {
                                        super::inproc_mio::Step::Unlock(t)
                                        | super::inproc_mio::Step::Queue(t) => t,
                                    };
                                    tokens.push(t);
                                    posts.push((
                                        super::inproc_mio::trait_request(org, st)?,
                                        format!("trait {i} now"),
                                    ));
                                }
                                // held already: counts as reached, as on the mouse path
                                Err(("trait_already_queued" | "trait_unavailable", _)) => {}
                                Err((code, msg)) => return Err(Error::new(code, msg)),
                            }
                        }
                        let action = json!({"kind":"mio_trait","organization":organization,"trait_indices":many,"auto":auto,"slot":slot});
                        let held = |m: &super::inproc_mio::Mios, _: &[Value]| {
                            let mut all = true;
                            for t in &tokens {
                                all &= super::inproc_mio::trait_held(m, organization, *t)?;
                            }
                            Some(all)
                        };
                        let gui = |p: &dyn Process, k: &dyn GameInput| -> Driven {
                            let n = names(p, organization, None)?;
                            mio_traits_many(p, k, &n.org_display, slot, &many, auto)
                        };
                        return run_in_process(
                            "mio_trait",
                            action,
                            id,
                            token,
                            process,
                            keys,
                            guard,
                            authority,
                            actions,
                            ch,
                            posts,
                            &held,
                            &gui,
                        );
                    }
                }
                let job = Job {
                    kind: "mio_trait",
                    action: json!({"kind":"mio_trait","organization":organization,"trait_indices":many,"auto":auto,"slot":slot}),
                    plan: Box::new(|process| {
                        *resolved.borrow_mut() = Some(names(process, organization, None)?);
                        Ok(())
                    }),
                    dispatch: Box::new(|process, keys| {
                        let n = need()?;
                        mio_traits_many(process, keys, &n.org_display, slot, &many, auto)
                    }),
                    settle: Box::new(|_, seen| {
                        let detail = json!({"tool":"mio_trait","organization":organization,
                            "changed": seen.and_then(|v| v.get("changed")).cloned().unwrap_or(json!(0)),
                            "unchanged": seen.and_then(|v| v.get("unchanged")).cloned().unwrap_or(json!([]))});
                        let flag = |k: &str| seen.and_then(|v| v.get(k)).and_then(Value::as_bool);
                        Ok(
                            if flag("nothing_to_queue") == Some(true)
                                || flag("trait_queued") == Some(true)
                            {
                                super::journal::Outcome::Reached(detail)
                            } else if flag("some_queued") == Some(true) {
                                super::journal::Outcome::Diverged(detail)
                            } else {
                                ui_action::effect(seen, "trait_queued", detail)
                            },
                        )
                    }),
                };
                return ui_action::run_bounded(
                    job, 150, id, token, process, keys, guard, authority, actions,
                );
            }
            let list_only = mgmt::flag(args, "list")?;
            let index =
                match args.get("trait_index") {
                    None => None,
                    Some(v) => Some(v.as_u64().filter(|n| *n <= 63).ok_or_else(|| {
                        Error::new("invalid_arguments", "trait_index must be 0..63")
                    })? as usize),
                };
            if list_only == index.is_some() {
                return Err(Error::new(
                    "invalid_arguments",
                    "Pass exactly one of trait_index (queue that trait) or list:true (return the trait tree)",
                ));
            }
            // engineer 84: in process, the tree in the game's order (list) or the trait's click
            if !actions.receipt_exists(id) {
                if let Some((ch, m)) = mio_route(organization) {
                    let org = m
                        .org(organization)
                        .expect("mio_route checked the organisation");
                    let Some(i) = index else {
                        return Ok(ui_action::result(
                            "mio_trait",
                            Value::Null,
                            Some(super::inproc_mio::listing(org)),
                        ));
                    };
                    let st = super::inproc_mio::step(org, i, org.points)
                        .map_err(|(c, msg)| Error::new(c, msg))?;
                    let t = match st {
                        super::inproc_mio::Step::Unlock(t) | super::inproc_mio::Step::Queue(t) => t,
                    };
                    let posts = vec![(
                        super::inproc_mio::trait_request(org, st)?,
                        format!("trait {i} now"),
                    )];
                    let action = json!({"kind":"mio_trait","organization":organization,"trait_index":index,"slot":slot});
                    let held = |m: &super::inproc_mio::Mios, _: &[Value]| {
                        super::inproc_mio::trait_held(m, organization, t)
                    };
                    let gui = |p: &dyn Process, k: &dyn GameInput| -> Driven {
                        let n = names(p, organization, None)?;
                        mio_trait(p, k, &n.org_display, slot, index)
                    };
                    return run_in_process(
                        "mio_trait",
                        action,
                        id,
                        token,
                        process,
                        keys,
                        guard,
                        authority,
                        actions,
                        ch,
                        posts,
                        &held,
                        &gui,
                    );
                }
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
                    Ok(
                        match seen
                            .and_then(|v| v.get("listing_only"))
                            .and_then(Value::as_bool)
                        {
                            // Listing changes nothing: whatever it saw is its result.
                            Some(true) => super::journal::Outcome::Reached(
                                json!({"tool":"mio_trait","listing":true}),
                            ),
                            _ => ui_action::effect(
                                seen,
                                "trait_queued",
                                json!({"tool":"mio_trait","organization":organization,"trait_index":index}),
                            ),
                        },
                    )
                }),
            };
            ui_action::run_bounded(job, 90, id, token, process, keys, guard, authority, actions)
        }
        "hoi4_mio_line" => {
            super::allowed(
                args,
                &[
                    "line",
                    "equipment",
                    "organization",
                    "list",
                    "request_id",
                    "lease_token",
                ],
            )?;
            let organization = match args.get("organization") {
                None => None,
                Some(_) => Some(key_arg(args, "organization")?),
            };
            let list_only = mgmt::flag(args, "list")?;
            let equipment = super::string(args, "equipment")?;
            let index = args
                .get("line")
                .and_then(Value::as_u64)
                .filter(|v| *v < 2048)
                .ok_or_else(|| Error::new("invalid_arguments", "line must be 0..2047"))?
                as usize;
            let (handle, address, naval) = super::production::lines(process)?
                .get(index)
                .filter(|l| l.key.as_deref() == Some(equipment))
                .map(|l| (l.handle, l.address, l.naval))
                .ok_or_else(|| {
                    Error::new(
                        "invalid_target",
                        "No own line with this index and equipment",
                    )
                })?;
            // engineer 84: in process, the roster row of a named own organisation (its key from the module's read); the
            // module reads the line's organisation back itself. The first offered organisation, a localised name, a
            // naval line and list:true stay on the mouse path (the roster decides what is offered)
            if let (false, false, Some(org_key)) =
                (list_only || actions.receipt_exists(id), naval, organization)
            {
                if let Some((ch, m)) = mio_route(org_key) {
                    let org = m.org(org_key).expect("mio_route checked the organisation");
                    let posts = vec![(
                        super::inproc_mio::line_request(address, Some(org.address))?,
                        "this organisation on this line (it must make the line's equipment)"
                            .to_string(),
                    )];
                    let action = json!({"kind":"mio_line","line":index,"equipment":equipment,
                        "organization":organization,"list":list_only});
                    let held = |_: &super::inproc_mio::Mios, rs: &[Value]| {
                        let r = rs.first()?;
                        Some(
                            r["module"]["executed"].as_i64()? >= 1
                                && r["module"]["executed"] == r["module"]["posted"],
                        )
                    };
                    let gui = |p: &dyn Process, k: &dyn GameInput| -> Driven {
                        let n = names(p, org_key, None)?;
                        line_assign(p, k, Some(&n.org_display), handle, false)
                    };
                    return run_in_process(
                        "mio_line", action, id, token, process, keys, guard, authority, actions,
                        ch, posts, &held, &gui,
                    );
                }
            }
            let display: RefCell<Option<String>> = RefCell::new(None);
            let job = Job {
                kind: "mio_line",
                action: json!({"kind":"mio_line","line":index,"equipment":equipment,
                    "organization":organization,"list":list_only}),
                plan: Box::new(|process| {
                    if let Some(org) = organization {
                        let n = names(process, org, None)?;
                        *display.borrow_mut() = Some(n.org_display);
                    }
                    Ok(())
                }),
                dispatch: Box::new(|process, keys| {
                    let shown = display.borrow().clone();
                    line_assign(process, keys, shown.as_deref(), handle, list_only)
                }),
                settle: Box::new(|_, seen| {
                    let detail = json!({"tool":"mio_line","organization":organization,
                        "line":index,"equipment":equipment,"seen":seen.cloned()});
                    Ok(
                        match seen
                            .and_then(|v| v.get("listing_only"))
                            .and_then(Value::as_bool)
                        {
                            Some(true) => super::journal::Outcome::Reached(detail),
                            _ => ui_action::effect(seen, "assigned", detail),
                        },
                    )
                }),
            };
            ui_action::run_bounded(job, 90, id, token, process, keys, guard, authority, actions)
        }
        _ => Err(Error::new(
            "capability_unavailable",
            "Unknown organisation tool",
        )),
    }
}

pub(super) fn tools() -> Vec<Value> {
    vec![
        json!({"name":"hoi4_mio_design_team","description":"Browse only, assigns nothing (live pp4, 17 Nov 2026): the politics view's organisation slots open a roster to look at; the game assigns an organisation from a task (a production line: hoi4_mio_line; a technology; a design). Kept for older plans: put a military-industrial organisation into a design-team slot of the politics view: Q politics view, the slot's roster button (the `slot`-th shown organisation slot, default 0), the organisation's row in the list (found by its localised name; `organization` is the script key from hoi4_mios, e.g. GER_porsche_organization). Typed refusals: organization_unknown, organization_not_allowed (allowed block rules the tag out) before input; mio_system_absent (the politics view shows no organisation slot: the organisation system is not in use in this game, designers are ideas, use hoi4_politics_choose), slot_unknown, organization_not_offered (lists the rows) found from the windows before the first state-changing click. Verified only by the list closing (political power before/after is recorded): verification gui_only. Results carry validated:false.","inputSchema":{"type":"object","properties":{"organization":{"type":"string","maxLength":128},"slot":{"type":"integer","minimum":0,"maximum":15},"request_id":{"type":"string"},"lease_token":{"type":"string"}},"required":["organization","request_id","lease_token"],"additionalProperties":false}}),
        json!({"name":"hoi4_mio_policy","description":"UNVALIDATED (no live test yet): attach a policy to an organisation: politics view, the slot's roster, the organisation's details button, Open policies, the policy row (by localised name). `organization` and `policy` are script keys (mio_policy_general_ruthless_contracts, ...). Typed refusals: organization_unknown, organization_not_allowed, policy_unknown (before input); mio_system_absent, slot_unknown, organization_not_offered, policy_not_offered (lists the rows) from the windows before the click. Verified from the detail window's attached policy name only (verification gui_only). Results carry validated:false.","inputSchema":{"type":"object","properties":{"organization":{"type":"string","maxLength":128},"policy":{"type":"string","maxLength":128},"slot":{"type":"integer","minimum":0,"maximum":15},"request_id":{"type":"string"},"lease_token":{"type":"string"}},"required":["organization","policy","request_id","lease_token"],"additionalProperties":false}}),
        json!({"name":"hoi4_mio_line","description":"Attach an organisation to an own production line with ordinary input, or read the line's state. The production line's organisation button (drawn at scale 0.45 in its rect's bottom-left corner) opens the roster of the organisations that make that equipment, under a 'Produce without a Military Industrial Organization' entry; its rows are read from memory (name, organisation or not) before any row click, the list is wheeled to its top, read again, and only an organisation row drawn in full is clicked (live pp14, 17 Nov 2026: Rheinmetall attached to a new anti-air line). The last line, whose button the screen edge cuts, is moved to the top for the roster and back to the bottom after. `organization` (localised name or key) picks the row, else the first organisation offered. A line that has an organisation answers assigned, already_attached without input. list:true opens the roster, answers attached and offered (the organisations' names) and closes it, changing nothing. Verified from the line's own row (has_mio_background). A line costs no political power. The politics view's organisation slots only browse rosters. The line is addressed like hoi4_production_set (line = entry_index from hoi4_production_lines, with its equipment). Typed refusals: invalid_target, organization_unknown, organization_not_allowed (before input); line_takes_no_organization (no organisation makes the equipment: the line shows no button, its button is disabled, or its roster offers only the 'Produce without' row); organization_not_offered (the roster's organisations named); row_not_visible (the row is below the five drawn in full); roster_unreadable (the roster's layout is not the reviewed one).","inputSchema":{"type":"object","properties":{"line":{"type":"integer","minimum":0,"maximum":2047},"equipment":{"type":"string","maxLength":128},"organization":{"type":"string","maxLength":128},"list":{"type":"boolean"},"request_id":{"type":"string"},"lease_token":{"type":"string"}},"required":["line","equipment","request_id","lease_token"],"additionalProperties":false}}),
        json!({"name":"hoi4_mio_trait","description":"Live-verified (6 Nov 2026 on 1.19.4 with 10 DLCs: a trait queued, read back by reopening the detail window): queue a trait of an organisation's trait tree: politics view, the slot's roster, the organisation's details button, the trait icon at `trait_index` (0-based in the order the tree window lists its entries; trait entries carry no text, so call with list:true first: it returns each entry's index, enabled state, queue number and whether it is visible, changing nothing). Exactly one of trait_index and list. Typed refusals: organization_unknown, organization_not_allowed (before input); mio_system_absent, slot_unknown, organization_not_offered, trait_unknown, trait_already_queued, trait_unavailable, trait_not_visible from the windows before the click. Verified from the entry's queue number only (verification gui_only). Results carry validated:false.","inputSchema":{"type":"object","properties":{"organization":{"type":"string","maxLength":128},"trait_index":{"type":"integer","minimum":0,"maximum":63},"trait_indices":{"type":"array","minItems":1,"maxItems":16,"items":{"type":"integer","minimum":0,"maximum":63},"description":"several traits queued in order in ONE visit of the tree window (the game's completion queue unlocks them as points come); each read back from its queue number; an already queued trait counts as reached; refused ones are listed in the receipt (unchanged)"},"auto":{"type":"integer","minimum":1,"maximum":5,"description":"in the same visit, queue the first open (enabled, visible, not held) traits until this many enabled traits wait in the queue; with trait_indices they come after those"},"list":{"type":"boolean"},"slot":{"type":"integer","minimum":0,"maximum":15},"bulk":{"type":"boolean"},"request_id":{"type":"string"},"lease_token":{"type":"string"}},"required":["organization","request_id","lease_token"],"additionalProperties":false}}),
    ]
}

#[cfg(test)]
mod tests {
    #[test]
    fn auto_queues_open_traits_until_the_queue_holds_enough_enabled_ones() {
        // (index, enabled, queued, owned, visible)
        let tree = [
            (0, true, false, true, true),   // held
            (1, true, true, false, true),   // queued, enabled
            (2, false, true, false, true),  // queued but disabled: does not count
            (3, true, false, false, false), // scrolled out
            (4, true, false, false, true),
            (5, true, false, false, true),
            (6, false, false, false, true),
        ];
        assert_eq!(super::auto_picks(&tree, 3), vec![4, 5]);
        assert_eq!(super::auto_picks(&tree, 2), vec![4]);
        assert!(
            super::auto_picks(&tree, 1).is_empty(),
            "one enabled trait waits already"
        );
    }

    fn row(name: &str, organisation: bool) -> super::RosterRow {
        super::RosterRow {
            name: name.into(),
            organisation,
            rect: None,
        }
    }

    #[test]
    fn the_production_roster_never_picks_the_produce_without_row() {
        let rows = [
            row("Produce without a Military Industrial Organization", false),
            row("Rheinmetall", true),
            row("Krupp", true),
        ];
        assert_eq!(super::roster_pick(&rows, None), Ok(1));
        assert_eq!(super::roster_pick(&rows, Some("Krupp")), Ok(2));
        let why = super::roster_pick(&rows, Some("Mauser")).unwrap_err();
        assert!(why.contains("Rheinmetall, Krupp"), "{why}");
        let none = super::roster_pick(&rows[..1], None).unwrap_err();
        assert!(none.contains("no organisation"), "{none}");
    }

    #[test]
    fn only_rows_drawn_in_full_are_clicked() {
        let list = super::gui_tree::Rect {
            x: 550,
            y: 172,
            w: 515,
            h: 552,
        };
        // live pp13: row 1 (Rheinmetall) drawn about 313..394 in game pixels
        assert_eq!(super::roster_row_point(list, 1), Some((750.0, 302.0)));
        assert_eq!(super::roster_row_point(list, 4), Some((750.0, 602.0)));
        assert_eq!(super::roster_row_point(list, 5), None);
    }

    #[test]
    fn the_line_logo_is_the_scaled_square_at_the_bottom_left() {
        let r = super::logo_square(super::gui_tree::Rect {
            x: 203,
            y: 338,
            w: 63,
            h: 63,
        });
        assert_eq!((r.x, r.y, r.w, r.h), (203, 373, 28, 28));
        // centre 217,387: the logo drawn about 216,386 (pp8)
        assert_eq!(r.center(), (217.0, 387.0));
    }

    use super::super::mios::{Catalog, Org};
    use super::super::script;
    use super::*;

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
            plan_org(&c, "GER_porsche_organization", "ENG")
                .unwrap_err()
                .code,
            "organization_not_allowed"
        );
        assert_eq!(
            plan_org(&c, "generic_tank_organization", "GER")
                .unwrap_err()
                .code,
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
        let rect = Some(gui_tree::Rect {
            x: 1,
            y: 2,
            w: 3,
            h: 4,
        });
        let entry = |index, enabled, queue: &str, rect| TraitEntry {
            index,
            enabled,
            queue: queue.into(),
            rect,
            state: if enabled { 0 } else { 3 },
        };
        let mut owned = entry(4, false, "", rect);
        owned.state = 2;
        let entries = vec![
            entry(0, true, "", rect),
            entry(1, true, "2", rect),
            entry(2, false, "", rect),
            entry(3, true, "", None),
            owned,
        ];
        assert_eq!(trait_refusal(&entries, 4).unwrap().0, "trait_owned");
        assert!(trait_refusal(&entries, 0).is_none());
        assert_eq!(
            trait_refusal(&entries, 1).unwrap().0,
            "trait_already_queued"
        );
        assert_eq!(trait_refusal(&entries, 2).unwrap().0, "trait_unavailable");
        assert_eq!(trait_refusal(&entries, 3).unwrap().0, "trait_not_visible");
        assert_eq!(trait_refusal(&entries, 9).unwrap().0, "trait_unknown");
    }

    #[test]
    fn descriptors_are_unvalidated_and_lease_gated() {
        for t in tools() {
            let description = t["description"].as_str().unwrap();
            if t["name"] == "hoi4_mio_trait" {
                assert!(description.starts_with("Live-verified (6 Nov 2026"));
            } else if t["name"] == "hoi4_mio_design_team" {
                assert!(description.starts_with("Browse only, assigns nothing"));
            } else if t["name"] == "hoi4_mio_line" {
                assert!(description.starts_with("Attach an organisation to an own production line"));
            } else {
                assert!(description.starts_with("UNVALIDATED (no live test yet):"));
            }
            assert!(t["inputSchema"]["required"]
                .as_array()
                .unwrap()
                .iter()
                .any(|v| v == "lease_token"));
            assert!(TOOLS.contains(&t["name"].as_str().unwrap()));
        }
        assert_eq!(tools().len(), TOOLS.len());
    }
}
