//! Unit-leader trait assignment with ordinary input: `hoi4_general_trait`. Written from
//! `interface/armytraittreewindow.gui`, `interface/traittreewindow_common.gui`,
//! `interface/leadergroups.gui` and `interface/unitleaderwindow.gui`; live-verified on macOS
//! 1.19.4.1 (pp13, 17 Nov 2026: the tree listed for Erich von Manstein, then Fortress Buster
//! bought, command power 16 -> 1, the entry owned).
//!
//! The game charges NDefines.NMilitary.UNIT_LEADER_ASSIGN_TRAIT_COST command power (15 in
//! 1.19.4) for an assignable trait (`trait_type = assignable_trait` or
//! `assignable_terrain_trait` in `common/unit_leader/*.txt`), given that the leader has a free
//! trait slot (UNIT_LEADER_INITIAL_TRAIT_SLOT + UNIT_LEADER_TRAIT_SLOT_PER_LEVEL per level) and
//! the trait's parents. Which traits exist, their names and the cost are read from the
//! (mod-merged) game files: nothing about a country's generals is fixed here.
//!
//! Route to the window (`unitleader_trait_tree_window`):
//! * the general leads an army or army group: the card's `assign_trait` button
//!   (`leader_groups_portrait_window` / `army_group_portrait_window`, shown while a trait can be
//!   picked);
//! * otherwise (or when that button is hidden): an own army's card, the army panel's commander
//!   slot (the commander list `armyleaderwindow`), the general's row (`armyleaderentry`) and its
//!   `traits_btn` (OPEN_LEADER_DETAILS).
//!
//! In the window the trait trees are the `tree` windows of `regular_traits` (basic, assignable
//! and field-marshal columns) and `terrain_traits`. Each entry is a `unit_leader_trait` window
//! (`traittreewindow_common.gui`) whose three backgrounds tell its state: `picked_bg` shown =
//! owned, `pickable_bg` shown = can be picked now, `disabled_bg` = locked; its `name` text is the
//! trait's localised name. (The older `picked_trait` / `pickable_trait` / `disabled_trait`
//! templates with one `bg` button are read the same way.) The `add_trait_<n>` windows of
//! `active_traits` ("PICK_TRAIT") are counted as the free trait slots the window shows.
//!
//! Verified by the entry turning to the owned state, else by command power dropping by about the
//! cost (`verification` trait_tree_and_command_power).
use super::{
    armies, commanders, country_status, loc, mgmt, panelkit as pk,
    ui::{accept_command, clear_notices, dismiss, read, settled, wait},
    ui_action::{self, refuse, Driven, Fail, Job},
    Guard, InputAuthority,
};
use crate::{
    error::{Error, Result},
    game,
    gui_tree::{self, Rect},
    memory::Reader,
    platform::{game_input::GameInput, Process},
    script::{self, Block},
    xlayout::{self, fld},
};
use serde_json::{json, Value};
use std::{
    cell::RefCell,
    collections::HashMap,
    path::Path,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

pub(super) const TOOLS: [&str; 1] = ["hoi4_general_trait"];

#[track_caller]
fn pause(ms: u64) {
    crate::phase::sleep(Duration::from_millis(ms));
}

// ------------------------------------------------------------ the game files

/// Fallbacks used only when `common/defines/00_defines.lua` cannot be read (1.19.4 values).
const ASSIGN_COST_FALLBACK: f64 = 15.0;
const BASE_MAX_FALLBACK: f64 = 80.0;

/// One assignable trait of the scripts.
#[derive(Clone, Debug, Default, PartialEq)]
pub(super) struct TraitDef {
    pub key: String,
    /// The script's `type` (field_marshal, corps_commander, navy, all), as written.
    pub kind: String,
    /// assignable_terrain_trait (drawn in the terrain tree).
    pub terrain: bool,
}

/// Assignable traits, the other trait keys, English names and the defines.
#[derive(Clone, Debug, Default)]
pub(super) struct Catalog {
    pub assignable: Vec<TraitDef>,
    /// Keys of every other trait (basic, status, personality, ...): earned, not assigned.
    pub other: Vec<(String, String)>,
    pub names: HashMap<String, String>,
    pub assign_cost: f64,
    pub base_max: f64,
}
impl Catalog {
    pub(super) fn display(&self, key: &str) -> String {
        self.names
            .get(key)
            .cloned()
            .unwrap_or_else(|| key.to_string())
    }
    /// The assignable trait an on-screen entry name stands for.
    pub(super) fn assignable_shown(&self, shown: &str) -> Option<&TraitDef> {
        self.assignable
            .iter()
            .find(|t| loc::same(&self.display(&t.key), shown) || loc::same(&t.key, shown))
    }
}

/// Assignable traits (and the other trait keys with their type) of the `leader_traits` blocks.
pub(super) fn traits_from(entries: &[script::Entry]) -> (Vec<TraitDef>, Vec<(String, String)>) {
    let mut assignable = Vec::new();
    let mut other = Vec::new();
    for top in entries.iter().filter(|e| e.key == "leader_traits") {
        for t in top.val.block() {
            let script::Val::Block(body) = &t.val else {
                continue;
            };
            let kind = body
                .text("type")
                .map(str::to_string)
                .unwrap_or_else(|| body.words("type").join(" "));
            match body.text("trait_type").unwrap_or("") {
                "assignable_trait" | "assignable_terrain_trait" => {
                    let terrain = body.text("trait_type") == Some("assignable_terrain_trait");
                    if !assignable.iter().any(|d: &TraitDef| d.key == t.key) {
                        assignable.push(TraitDef {
                            key: t.key.clone(),
                            kind,
                            terrain,
                        });
                    }
                }
                other_type => other.push((t.key.clone(), other_type.to_string())),
            }
        }
    }
    (assignable, other)
}

/// A number define (`NAME = 15.0,`) of 00_defines.lua.
pub(super) fn define(text: &str, name: &str) -> Option<f64> {
    text.lines()
        .map(str::trim_start)
        .find(|l| l.starts_with(name) && l[name.len()..].trim_start().starts_with('='))
        .and_then(|l| l.split('=').nth(1))
        .and_then(|v| {
            v.trim()
                .split(|c: char| !(c.is_ascii_digit() || c == '.' || c == '-'))
                .next()
                .and_then(|n| n.parse().ok())
        })
}

fn load(dir: &Path) -> Catalog {
    let (assignable, other) = traits_from(&script::parse_dir(&dir.join("common/unit_leader")));
    let keys: Vec<&str> = assignable
        .iter()
        .map(|t| t.key.as_str())
        .chain(other.iter().map(|(k, _)| k.as_str()))
        .collect();
    let names = loc::lookup(dir, &keys);
    let defines =
        std::fs::read_to_string(dir.join("common/defines/00_defines.lua")).unwrap_or_default();
    Catalog {
        assignable,
        other,
        names,
        assign_cost: define(&defines, "UNIT_LEADER_ASSIGN_TRAIT_COST")
            .unwrap_or(ASSIGN_COST_FALLBACK),
        base_max: define(&defines, "BASE_MAX_COMMAND_POWER").unwrap_or(BASE_MAX_FALLBACK),
    }
}
fn catalog(process: &dyn Process) -> Result<Arc<Catalog>> {
    let dir = mgmt::game_dir(process)?;
    let c = script::cached(&dir, "unit_leader_traits", load);
    if c.assignable.is_empty() {
        return Err(Error::new(
            "catalog_unavailable",
            "The game's common/unit_leader scripts list no assignable trait",
        ));
    }
    Ok(c)
}

/// The asked trait (script key or localised name) as (key, display name); pure.
pub(super) fn resolve_trait(catalog: &Catalog, asked: &str) -> Result<(String, String)> {
    let asked = asked.trim();
    if let Some(t) = catalog
        .assignable
        .iter()
        .find(|t| t.key == asked)
        .or_else(|| catalog.assignable_shown(asked))
    {
        return Ok((t.key.clone(), catalog.display(&t.key)));
    }
    if let Some((key, kind)) = catalog
        .other
        .iter()
        .find(|(k, _)| k == asked || loc::same(&catalog.display(k), asked))
    {
        return Err(Error::new(
            "trait_not_assignable",
            format!(
                "{key} is a {} trait: the game grants it, it cannot be bought with command power",
                if kind.is_empty() {
                    "non-assignable"
                } else {
                    kind.as_str()
                }
            ),
        ));
    }
    let lower = asked.to_lowercase();
    let near: Vec<String> = catalog
        .assignable
        .iter()
        .map(|t| (t.key.clone(), catalog.display(&t.key)))
        .filter(|(k, d)| {
            !lower.is_empty() && (k.contains(&lower) || d.to_lowercase().contains(&lower))
        })
        .take(8)
        .map(|(k, d)| format!("{k} ({d})"))
        .collect();
    Err(Error::new(
        "trait_unknown",
        format!(
            "No assignable unit-leader trait {asked:?} in the game's scripts{}",
            if near.is_empty() {
                format!(
                    "; {} exist (list:true shows the ones this general is offered)",
                    catalog.assignable.len()
                )
            } else {
                format!("; similar: {}", near.join(", "))
            }
        ),
    ))
}

/// Command power refusal before input; pure. The message carries the cost first and the held
/// power second, the order `plan_military::cp_wait_days` reads.
pub(super) fn cp_refusal(
    cost: f64,
    held: Option<f64>,
    general: &str,
) -> Option<(&'static str, String)> {
    match held {
        Some(cp) if cp < cost => Some((
            "insufficient_command_power",
            format!(
                "A unit-leader trait costs {cost} command power and {cp} are held (assigning one to {general}); nothing was changed"
            ),
        )),
        _ => None,
    }
}

// --------------------------------------------------------------- the window

const TREE_ROOT: &str = "unitleader_trait_tree_window";
/// The trait tree window's class: a persistent view of the interface manager (macOS 1.19.4.1,
/// pp12 17 Nov 2026: CArmyLeaderTraitWindow at manager slot 0x2b0, its root container at +0x60;
/// the CUnitLeaderTraitTree objects of its two tree panels are made while it is open).
const TREE_CLASS: pk::Class = pk::Class {
    name: "CArmyLeaderTraitWindow",
    vtable: 0x3640050,
    root_name: TREE_ROOT,
};
/// Fixed paths to the window's root container: (interface-manager slot, the fields followed
/// from the slot's view; the last word read is the root container pointer). macOS 1.19.4.1:
/// slot 0x2b0 holds CArmyLeaderTraitWindow, root at +0x60 (pp12, 17 Nov 2026,
/// `dbg_find_root:2b0:unitleader_trait_tree_window`). Another build falls back to the class
/// lookup, the pop-up list and the manager slots, then a two-level search (remembered).
const TREE_PATHS: &[(u64, &[u64])] = &[(0x2b0, &[0x60])];
/// Interface manager of the in-game idler (the same reviewed field panelkit reads).
const IDLER: xlayout::Class = xlayout::Class::new("CInGameIdler", 0x35089d8);
const MANAGER: u64 = 0x670;
const MANAGER_SCAN: usize = 0x400;
/// Root-container fields seen on reviewed windows (views +0x490/+0x430, pop-ups +0x20..+0x30,
/// child windows +0x50/+0x68..., the trait window +0x60). Searched by the root's .gui name, so a
/// build whose slot moved is still found.
const ROOT_OFFSETS: [u64; 13] = [
    0x60, 0x490, 0x430, 0x20, 0x28, 0x30, 0x50, 0x68, 0x70, 0x78, 0x8, 0x10, 0x18,
];
/// (slot, field, root field) found by the two-level search.
static LEARNED: Mutex<Option<(u64, u64, u64)>> = Mutex::new(None);

/// armytraittreewindow.gui geometry, anchored on the close button (x = -44 from the right edge,
/// y = 11, window 1340 wide): the scrolled panels regular_traits (294,110 636x640) and
/// terrain_traits (939,110 410x640); an entry click inside it bought a trait live (pp13, 17 Nov 2026).
const WINDOW_W: i32 = 1340;
const CLOSE_AT: (i32, i32) = (44, 11);
const REGULAR_CLIP: (i32, i32, i32, i32) = (294, 110, 636, 640);
const TERRAIN_CLIP: (i32, i32, i32, i32) = (939, 110, 410, 640);

/// The commander list row's `traits_btn` (unitleaderwindow.gui 318,56, 96 px left of
/// promote_btn) from the row's leader_button rectangle, the way armies.rs places Promote
/// (centre 402 px right, 22 px up from the portrait's bottom, measured live 1 Nov 2026).
/// Live pp13 (17 Nov 2026): the Traits button opened the tree from a commander list row.
const TRAITS_BTN_CENTRE: (f64, f64) = (306.0, 22.0);

fn plausible(p: u64) -> bool {
    (0x10000..=0x7fff_ffff_0000).contains(&p) && p % 8 == 0
}
fn named_root(r: &Reader<'_>, i: u64, p: u64) -> Option<u64> {
    if !plausible(p) {
        return None;
    }
    let root = gui_tree::container(r, p, i).ok()??;
    (pk::container_name(r, i, root).ok()??.as_str() == TREE_ROOT).then_some(root)
}
fn shown_root(r: &Reader<'_>, i: u64, p: u64) -> Option<u64> {
    named_root(r, i, p).filter(|root| gui_tree::visible(r, *root).unwrap_or(false))
}
fn manager(r: &Reader<'_>, i: u64) -> Result<u64> {
    let idler = r.u64(i + game::layout(r).idler_global)?;
    r.u64(idler + fld(r.string_abi(), &IDLER, MANAGER)?)
}

/// The shown trait tree window's root container, without a slow search.
fn find_tree(r: &Reader<'_>, i: u64) -> Result<Option<u64>> {
    let manager = manager(r, i)?;
    for (slot, hops) in TREE_PATHS {
        let mut w = r.u64(manager + slot).unwrap_or(0);
        for hop in hops.iter() {
            w = if plausible(w) {
                r.u64(w + hop).unwrap_or(0)
            } else {
                0
            };
        }
        if let Some(root) = shown_root(r, i, w) {
            return Ok(Some(root));
        }
    }
    let learned = LEARNED.lock().ok().and_then(|g| *g);
    if let Some((slot, field, root_at)) = learned {
        let view = r.u64(manager + slot).unwrap_or(0);
        let w = if plausible(view) {
            r.u64(view + field).unwrap_or(0)
        } else {
            0
        };
        if plausible(w) {
            if let Some(root) = shown_root(r, i, r.u64(w + root_at).unwrap_or(0)) {
                return Ok(Some(root));
            }
        }
    }
    if TREE_CLASS.vtable != 0 {
        if let Some(root) = pk::locate(r, i, &TREE_CLASS)? {
            if gui_tree::visible(r, root)? {
                return Ok(Some(root));
            }
        }
    }
    if let Ok(roots) = pk::popup_roots_named(r, i, &[TREE_ROOT]) {
        if let Some(root) = roots.first() {
            return Ok(Some(*root));
        }
    }
    if !plausible(manager) {
        return Ok(None);
    }
    let slots = r.read(manager, MANAGER_SCAN)?;
    for c in slots.chunks_exact(8) {
        let view = u64::from_le_bytes(c.try_into().unwrap());
        if !plausible(view) {
            continue;
        }
        for off in ROOT_OFFSETS {
            if let Some(root) = shown_root(r, i, r.u64(view + off).unwrap_or(0)) {
                return Ok(Some(root));
            }
        }
    }
    Ok(None)
}
/// One slow search (manager slot -> field -> root field) for a shown trait tree window; the
/// path found is remembered for this server life.
fn learn_tree(r: &Reader<'_>, i: u64) -> Result<Option<u64>> {
    let manager = manager(r, i)?;
    if !plausible(manager) {
        return Ok(None);
    }
    let slots = r.read(manager, MANAGER_SCAN)?;
    let mut budget: u32 = 150_000;
    for (s, c) in slots.chunks_exact(8).enumerate() {
        let view = u64::from_le_bytes(c.try_into().unwrap());
        if !plausible(view) {
            continue;
        }
        let Ok(fields) = r.read(view, 0x800) else {
            continue;
        };
        for (f, d) in fields.chunks_exact(8).enumerate() {
            let w = u64::from_le_bytes(d.try_into().unwrap());
            if !plausible(w) {
                continue;
            }
            for off in ROOT_OFFSETS {
                if budget == 0 {
                    return Ok(None);
                }
                budget -= 1;
                if let Some(root) = shown_root(r, i, r.u64(w + off).unwrap_or(0)) {
                    if let Ok(mut g) = LEARNED.lock() {
                        *g = Some(((s * 8) as u64, (f * 8) as u64, off));
                    }
                    return Ok(Some(root));
                }
            }
        }
    }
    Ok(None)
}
fn tree_open(r: &Reader<'_>, i: u64) -> Result<bool> {
    Ok(find_tree(r, i)?.is_some())
}
fn tree_root(r: &Reader<'_>, i: u64) -> Result<u64> {
    find_tree(r, i)?.ok_or_else(|| {
        Error::new(
            "window_not_open",
            "The unit-leader trait tree window is not shown",
        )
    })
}
/// Wait for the window after the click that opens it; one slow search when the quick lookups
/// do not find it.
fn await_tree(process: &dyn Process) -> std::result::Result<(), Fail> {
    if wait(process, "trait tree window", tree_open).is_ok() {
        return Ok(());
    }
    if read(process, learn_tree).ok().flatten().is_some() {
        return Ok(());
    }
    Err(Fail::Failed(Error::new(
        "window_not_found",
        "The trait tree window did not appear where it was looked for (pop-up list, interface \
         manager slots, two-level search); map it live (dbg_views, \
         dbg_find_root:SLOT:unitleader_trait_tree_window) into leader_act TREE_PATHS",
    )))
}

/// State of one tree entry as drawn.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Shown {
    Owned,
    Available,
    Locked,
}
impl Shown {
    fn word(self) -> &'static str {
        match self {
            Shown::Owned => "owned",
            Shown::Available => "available",
            Shown::Locked => "locked",
        }
    }
}
/// One entry of a trait tree.
#[derive(Clone, Debug, PartialEq)]
pub(super) struct TraitEntry {
    pub name: String,
    pub state: Shown,
    /// The clickable background (unclipped: an entry scrolled out of its panel still has one).
    pub rect: Option<Rect>,
    pub terrain: bool,
}

fn shown_child(r: &Reader<'_>, i: u64, object: u64, name: &str) -> Result<Option<u64>> {
    Ok(match gui_tree::child(r, i, object, name)? {
        Some(b) if gui_tree::shown(r, b)? => Some(b),
        _ => None,
    })
}
fn read_entry(r: &Reader<'_>, i: u64, entry: u64, terrain: bool) -> Result<Option<TraitEntry>> {
    let template = pk::container_name(r, i, entry)?.unwrap_or_default();
    let (state, button) = if gui_tree::child(r, i, entry, "disabled_bg")?.is_some()
        || gui_tree::child(r, i, entry, "pickable_bg")?.is_some()
    {
        if let Some(b) = shown_child(r, i, entry, "picked_bg")? {
            (Shown::Owned, Some(b))
        } else if let Some(b) = shown_child(r, i, entry, "pickable_bg")? {
            if gui_tree::disabled(r, b)? {
                (Shown::Locked, Some(b))
            } else {
                (Shown::Available, Some(b))
            }
        } else {
            (Shown::Locked, shown_child(r, i, entry, "disabled_bg")?)
        }
    } else {
        let state = match template.as_str() {
            "picked_trait" => Shown::Owned,
            "pickable_trait" => Shown::Available,
            "disabled_trait" => Shown::Locked,
            _ => return Ok(None),
        };
        (state, gui_tree::child(r, i, entry, "bg")?)
    };
    let name = pk::text(r, i, entry, "name", 0)?.unwrap_or_default();
    if name.trim().is_empty() {
        return Ok(None);
    }
    let rect = button.and_then(|b| gui_tree::rect_unclipped(r, i, b, pk::GAME).ok());
    Ok(Some(TraitEntry {
        name,
        state,
        rect,
        terrain,
    }))
}
fn walk(
    r: &Reader<'_>,
    i: u64,
    object: u64,
    terrain: bool,
    depth: u32,
    out: &mut Vec<TraitEntry>,
) -> Result<()> {
    for child in gui_tree::children(r, object)? {
        let Some(inner) = gui_tree::container(r, child, i)? else {
            continue;
        };
        if !gui_tree::visible(r, inner).unwrap_or(true) {
            continue;
        }
        match read_entry(r, i, inner, terrain)? {
            Some(e) => out.push(e),
            None if depth > 0 => walk(r, i, inner, terrain, depth - 1, out)?,
            None => {}
        }
    }
    Ok(())
}
/// Every entry of both trees, tree order.
fn tree_entries(r: &Reader<'_>, i: u64) -> Result<Vec<TraitEntry>> {
    let root = tree_root(r, i)?;
    let mut out = Vec::new();
    for (panel, terrain) in [("regular_traits", false), ("terrain_traits", true)] {
        let Some(base) = pk::find(r, i, root, panel, 3)? else {
            continue;
        };
        let Some(object) = gui_tree::container(r, base, i)? else {
            continue;
        };
        let Some(tree) = pk::find(r, i, object, "tree", 0)? else {
            continue;
        };
        let Some(tree) = gui_tree::container(r, tree, i)? else {
            continue;
        };
        walk(r, i, tree, terrain, 2, &mut out)?;
    }
    if out.is_empty() {
        return Err(Error::new(
            "gui_target_unavailable",
            "The trait tree window shows no trait entry",
        ));
    }
    Ok(out)
}
/// The add_trait_<n> ("PICK_TRAIT") windows shown in active_traits.
fn free_slots(r: &Reader<'_>, i: u64) -> Result<Option<usize>> {
    let root = tree_root(r, i)?;
    let Some(base) = pk::find(r, i, root, "active_traits", 2)? else {
        return Ok(None);
    };
    let Some(object) = gui_tree::container(r, base, i)? else {
        return Ok(None);
    };
    let mut n = 0;
    for k in 0..8 {
        if pk::shown(r, i, object, &format!("add_trait_{k}"), 0)? {
            n += 1;
        }
    }
    Ok(Some(n))
}
fn leader_shown(r: &Reader<'_>, i: u64) -> Result<Option<String>> {
    pk::text(r, i, tree_root(r, i)?, "name", 0)
}
/// The visible areas of the regular and the terrain tree.
fn clips(r: &Reader<'_>, i: u64) -> Result<[Rect; 2]> {
    let root = tree_root(r, i)?;
    let c = pk::rect_of(r, i, root, "close_button", 0)?;
    let (x0, y0) = (c.x + CLOSE_AT.0 - WINDOW_W, c.y - CLOSE_AT.1);
    let at = |(x, y, w, h): (i32, i32, i32, i32)| Rect {
        x: x0 + x,
        y: y0 + y,
        w,
        h,
    };
    Ok([at(REGULAR_CLIP), at(TERRAIN_CLIP)])
}
pub(super) fn inside(clip: &Rect, x: f64, y: f64) -> bool {
    x >= f64::from(clip.x)
        && x <= f64::from(clip.x + clip.w)
        && y >= f64::from(clip.y)
        && y <= f64::from(clip.y + clip.h)
}

/// The entry to click; pure. `want` is the asked trait's display name; without one the first
/// available assignable entry, the earliest of `prefer` (display names) first.
pub(super) fn choose(
    entries: &[TraitEntry],
    assignable: impl Fn(&str) -> bool,
    want: Option<&str>,
    prefer: &[String],
    free: Option<usize>,
) -> std::result::Result<usize, (&'static str, String)> {
    let offered = || -> String {
        let shown: Vec<String> = entries
            .iter()
            .filter(|e| assignable(&e.name))
            .map(|e| format!("{} ({})", e.name, e.state.word()))
            .collect();
        if shown.is_empty() {
            "no assignable trait".into()
        } else {
            shown.join(" | ")
        }
    };
    let locked = |what: String| -> (&'static str, String) {
        if free == Some(0) {
            (
                "no_free_trait_slot",
                format!("{what}: the general has no free trait slot (slots grow with skill level); nothing was changed"),
            )
        } else {
            (
                "trait_unavailable",
                format!("{what}: the game locks it (a parent trait missing, a mutually exclusive trait owned, or no free slot); nothing was changed"),
            )
        }
    };
    if let Some(want) = want {
        let Some(index) = entries.iter().position(|e| loc::same(&e.name, want)) else {
            return Err((
                "trait_not_offered",
                format!(
                    "The general's trait tree does not show {want:?} (a trait of the other rank or another leader type); shown: {}",
                    offered()
                ),
            ));
        };
        return match entries[index].state {
            Shown::Available => Ok(index),
            Shown::Owned => Err((
                "trait_owned",
                format!("The general already has {want}; nothing was changed"),
            )),
            Shown::Locked => Err(locked(format!("{want} is locked"))),
        };
    }
    let available: Vec<usize> = (0..entries.len())
        .filter(|k| entries[*k].state == Shown::Available && assignable(&entries[*k].name))
        .collect();
    for p in prefer {
        if let Some(k) = available.iter().find(|k| loc::same(&entries[**k].name, p)) {
            return Ok(*k);
        }
    }
    available.first().copied().ok_or_else(|| {
        if free == Some(0) {
            locked("No assignable trait can be picked".into())
        } else {
            (
                "no_enabled_trait",
                format!(
                    "No assignable trait can be picked now; shown: {}; nothing was changed",
                    offered()
                ),
            )
        }
    })
}

/// The commander list's traits button centre for a row's leader_button rectangle; pure.
pub(super) fn traits_button(portrait: &Rect) -> (f64, f64) {
    (
        f64::from(portrait.x) + TRAITS_BTN_CENTRE.0,
        f64::from(portrait.y + portrait.h) - TRAITS_BTN_CENTRE.1,
    )
}

// ------------------------------------------------------------------ driving

/// Everything the request names, resolved before input.
#[derive(Clone)]
struct Planned {
    general: String,
    /// The army or army group the general leads (its card's assign_trait button).
    leads: Option<String>,
    /// An own army (not a group) whose panel opens the commander list.
    list_army: Option<String>,
    want: Option<(String, String)>,
    prefer: Vec<String>,
    catalog: Arc<Catalog>,
}

fn plan(
    process: &dyn Process,
    general: &str,
    asked: Option<&str>,
    prefer: &[String],
    list_only: bool,
) -> Result<Planned> {
    let catalog = catalog(process)?;
    let want = asked.map(|a| resolve_trait(&catalog, a)).transpose()?;
    let snap = commanders::snapshot(process)?;
    let mut known: Vec<String> = snap
        .leaders
        .iter()
        .flatten()
        .map(|l| l.name.clone())
        .collect();
    known.extend(snap.targets.iter().filter_map(|t| t.commander.clone()));
    let name = known
        .iter()
        .find(|n| n.eq_ignore_ascii_case(general))
        .cloned();
    let name = match name {
        Some(n) => n,
        None if known.is_empty() => general.to_string(),
        None => {
            return Err(Error::new(
                "unknown_commander",
                format!(
                    "No own army leader named {general:?}. Closest: {}",
                    commanders::closest_names(general, &known, 5).join(", ")
                ),
            ))
        }
    };
    let led: Vec<&commanders::Target> = snap
        .targets
        .iter()
        .filter(|t| {
            t.commander
                .as_deref()
                .is_some_and(|c| c.eq_ignore_ascii_case(&name))
        })
        .collect();
    let leads = led.first().map(|t| t.name.clone());
    let list_army = led
        .iter()
        .find(|t| !t.is_group)
        .map(|t| t.name.clone())
        .or_else(|| {
            snap.targets
                .iter()
                .find(|t| !t.is_group)
                .map(|t| t.name.clone())
        });
    if !list_only {
        let held = read(process, country_status::command_power).ok().flatten();
        if let Some((code, message)) = cp_refusal(catalog.assign_cost, held, &name) {
            return Err(Error::new(code, message));
        }
    }
    let prefer = prefer
        .iter()
        .map(|p| match resolve_trait(&catalog, p) {
            Ok((_, display)) => display,
            Err(_) => p.clone(),
        })
        .collect();
    Ok(Planned {
        general: name,
        leads,
        list_army,
        want,
        prefer,
        catalog,
    })
}

/// The general's row in the commander list brought into the list's band; returns the centre of
/// its traits button.
fn list_row_button(
    process: &dyn Process,
    keys: &dyn GameInput,
    general: &str,
) -> std::result::Result<(f64, f64), Fail> {
    let wanted = general.to_lowercase();
    let mut seen: Vec<String> = Vec::new();
    for _ in 0..16 {
        let (rows, band) = read(process, |r, i| {
            Ok((armies::leader_entries(r, i)?, armies::leader_band(r, i)))
        })?;
        let mid = f64::from(band.0 + band.1) / 2.0;
        if let Some((_, portrait, _)) = rows.iter().find(|(n, _, _)| n.to_lowercase() == wanted) {
            let (x, y) = traits_button(portrait);
            if y >= f64::from(band.0) + 6.0 && y <= f64::from(band.1) - 6.0 {
                return Ok((x, y));
            }
            let step = (y - mid).clamp(-400.0, 400.0);
            keys.scroll(pk::GAME, (x, mid), -(step as i32))?;
            pause(300);
            continue;
        }
        let fresh: Vec<String> = rows
            .iter()
            .map(|(n, _, _)| n.clone())
            .filter(|n| !seen.contains(n))
            .collect();
        if fresh.is_empty() {
            break;
        }
        seen.extend(fresh);
        let Some((_, first, _)) = rows.first() else {
            break;
        };
        let at = (f64::from(first.x + first.w / 2), mid);
        keys.scroll(pk::GAME, at, -(band.1 - band.0).max(120))?;
        pause(300);
    }
    dismiss(process, keys, |r, i| Ok(!armies::leader_list_open(r, i)?));
    Err(refuse(
        "general_not_listed",
        format!(
            "The commander list does not show {general}. Closest: {}; nothing was changed",
            commanders::closest_names(general, &seen, 5).join(", ")
        ),
    ))
}

fn close_tree(process: &dyn Process, keys: &dyn GameInput) {
    if read(process, tree_open).unwrap_or(false) {
        let clicked = pk::click(process, keys, "close trait tree", |r, i| {
            pk::rect_of(r, i, tree_root(r, i)?, "close_button", 0)
        });
        let closed = clicked.is_ok()
            && wait(process, "trait tree closed", |r, i| Ok(!tree_open(r, i)?)).is_ok();
        if !closed {
            dismiss(process, keys, |r, i| Ok(!tree_open(r, i)?));
        }
    }
}
fn leave(process: &dyn Process, keys: &dyn GameInput) {
    close_tree(process, keys);
    if read(process, |r, i| {
        Ok(armies::leader_list_open(r, i).unwrap_or(false))
    })
    .unwrap_or(false)
    {
        dismiss(process, keys, |r, i| Ok(!armies::leader_list_open(r, i)?));
    }
}

/// Open the general's trait tree: the card's assign_trait button, else the commander list row.
fn open_tree(
    process: &dyn Process,
    keys: &dyn GameInput,
    p: &Planned,
) -> std::result::Result<(), Fail> {
    clear_notices(process, keys)?;
    // a tree left open may be another leader's
    close_tree(process, keys);
    if let Some(army) = &p.leads {
        let button = read(process, |r, i| {
            let w = armies::card_widget(r, i, army, "assign_trait")?;
            Ok(if gui_tree::shown(r, w)? {
                gui_tree::rect(r, i, w, pk::GAME).ok()
            } else {
                None
            })
        })
        .ok()
        .flatten();
        if let Some(rect) = button {
            let (x, y) = rect.center();
            keys.click(pk::GAME, x, y)?;
            if wait(process, "trait tree window", tree_open).is_ok() {
                pause(300);
                return Ok(());
            }
        }
    }
    let Some(army) = p.list_army.as_deref() else {
        return Err(refuse(
            "no_own_army",
            "The commander list opens from an own army's panel and the country has no army; nothing was changed",
        ));
    };
    armies::select_army_quick(process, keys, army)?;
    pause(300);
    keys.click(pk::GAME, armies::COMMANDER_SLOT.0, armies::COMMANDER_SLOT.1)?;
    wait(process, "commander list open", armies::leader_list_open)?;
    pause(250);
    let (x, y) = list_row_button(process, keys, &p.general)?;
    keys.click(pk::GAME, x, y)?;
    await_tree(process)?;
    pause(300);
    Ok(())
}

/// Scroll the entry's tree until the entry's centre is inside the tree's visible area.
fn reveal(
    process: &dyn Process,
    keys: &dyn GameInput,
    name: &str,
) -> std::result::Result<Rect, Fail> {
    for _ in 0..14 {
        let (entries, clip) = settled(process, "trait entry", |r, i| {
            Ok((tree_entries(r, i)?, clips(r, i)?))
        })?;
        let Some(e) = entries.iter().find(|e| e.name == name) else {
            return Err(Fail::Failed(Error::new(
                "gui_target_unavailable",
                "The trait entry left the tree",
            )));
        };
        let clip = if e.terrain { clip[1] } else { clip[0] };
        let Some(rect) = e.rect else {
            break;
        };
        let (x, y) = rect.center();
        if inside(&clip, x, y) {
            return Ok(rect);
        }
        let (cx, cy) = clip.center();
        let half = f64::from(clip.h) / 2.0;
        let step = (y - cy).clamp(-half, half);
        keys.scroll(pk::GAME, (cx, cy), -(step as i32))?;
        pause(300);
    }
    Err(refuse(
        "trait_not_visible",
        format!("{name} could not be scrolled into the trait tree's view; nothing was changed"),
    ))
}

fn entries_json(entries: &[TraitEntry], catalog: &Catalog) -> Vec<Value> {
    entries
        .iter()
        .map(|e| {
            let def = catalog.assignable_shown(&e.name);
            json!({"name": e.name, "key": def.map(|d| d.key.clone()),
                "assignable": def.is_some(), "leader_type": def.map(|d| d.kind.clone()),
                "terrain_trait": def.map(|d| d.terrain), "terrain": e.terrain,
                "state": e.state.word(),
                "can_pick": e.state == Shown::Available && def.is_some()})
        })
        .collect()
}

fn owned(r: &Reader<'_>, i: u64, name: &str) -> Result<bool> {
    Ok(tree_entries(r, i)?
        .iter()
        .any(|e| e.name == name && e.state == Shown::Owned))
}

fn drive(process: &dyn Process, keys: &dyn GameInput, p: &Planned, list_only: bool) -> Driven {
    let result: Driven = (|| {
        open_tree(process, keys, p)?;
        if let Some(shown) = read(process, leader_shown).ok().flatten() {
            if !shown.trim().is_empty() && !loc::same(&shown, &p.general) {
                return Err(refuse(
                    "wrong_leader",
                    format!(
                        "The trait tree opened for {shown}, not {}; nothing was changed",
                        p.general
                    ),
                ));
            }
        }
        let entries = settled(process, "trait entries", tree_entries)?;
        let free = read(process, free_slots).ok().flatten();
        let before = read(process, country_status::command_power).ok().flatten();
        let catalog = &p.catalog;
        if list_only {
            return Ok(json!({
                "listing_only": true,
                "general": p.general,
                "traits": entries_json(&entries, catalog),
                "free_trait_slots_shown": free,
                "command_power": before,
                "assign_cost": catalog.assign_cost,
                "base_max_command_power": catalog.base_max,
                "verification": "gui_only",
            }));
        }
        let want = p.want.as_ref().map(|(_, d)| d.as_str());
        let index = choose(
            &entries,
            |n| catalog.assignable_shown(n).is_some(),
            want,
            &p.prefer,
            free,
        )
        .map_err(|(code, message)| refuse(code, message))?;
        if let Some((code, message)) = cp_refusal(catalog.assign_cost, before, &p.general) {
            return Err(refuse(code, message));
        }
        let name = entries[index].name.clone();
        let rect = reveal(process, keys, &name)?;
        let (x, y) = rect.center();
        keys.click(pk::GAME, x, y)?;
        let confirmed = accept_command(process, keys)?;
        // the owned background is drawn a moment after the click: poll up to 2.5 s
        let deadline = Instant::now() + Duration::from_millis(2500);
        let mut picked = read(process, |r, i| owned(r, i, &name)).unwrap_or(false);
        while !picked && Instant::now() < deadline {
            pause(150);
            picked = read(process, |r, i| owned(r, i, &name)).unwrap_or(false);
        }
        let after = read(process, country_status::command_power).ok().flatten();
        let dropped = matches!(
            (before, after),
            (Some(b), Some(a)) if b - a >= catalog.assign_cost - 1.0
        );
        Ok(json!({
            "assigned": picked || dropped,
            "general": p.general,
            "trait": name,
            "key": catalog.assignable_shown(&name).map(|d| d.key.clone()),
            "shown_owned": picked,
            "command_power_before": before,
            "command_power_after": after,
            "command_power_dropped": dropped,
            "assign_cost": catalog.assign_cost,
            "confirmation_accepted": confirmed,
            "verification": "trait_tree_and_command_power",
        }))
    })();
    leave(process, keys);
    result
}

// ------------------------------------------------------------------- tools

fn name_arg<'a>(args: &'a serde_json::Map<String, Value>, name: &str) -> Result<&'a str> {
    let v = super::string(args, name)?;
    if v.trim().is_empty() || v.len() > 128 || v.chars().any(char::is_control) {
        return Err(Error::new("invalid_arguments", format!("Invalid {name}")));
    }
    Ok(v)
}
/// The optional `prefer` list (trait keys or names); pure.
pub(super) fn prefer_arg(args: &serde_json::Map<String, Value>) -> Result<Vec<String>> {
    match args.get("prefer") {
        None => Ok(Vec::new()),
        Some(Value::Array(items)) if items.len() <= 24 => items
            .iter()
            .map(|v| match v.as_str() {
                Some(s) if !s.trim().is_empty() && s.len() <= 128 => Ok(s.to_string()),
                _ => Err(Error::new(
                    "invalid_arguments",
                    "prefer holds trait keys or names",
                )),
            })
            .collect(),
        Some(_) => Err(Error::new(
            "invalid_arguments",
            "prefer must be a list of at most 24 trait keys or names",
        )),
    }
}

#[allow(clippy::too_many_arguments)]
pub(super) fn call(
    name: &str,
    args: &serde_json::Map<String, Value>,
    process: &(dyn Process + Send + Sync),
    keys: &dyn GameInput,
    guard: &mut Guard,
    authority: &InputAuthority,
    actions: &mut super::journal::GameJournal,
) -> Result<Value> {
    if name != "hoi4_general_trait" {
        return Err(Error::new(
            "capability_unavailable",
            "Unknown unit-leader tool",
        ));
    }
    super::allowed(
        args,
        &[
            "general",
            "trait",
            "prefer",
            "list",
            "request_id",
            "lease_token",
        ],
    )?;
    let id = super::string(args, "request_id")?;
    let token = super::string(args, "lease_token")?;
    let general = name_arg(args, "general")?;
    let asked = match args.get("trait") {
        None => None,
        Some(_) => Some(name_arg(args, "trait")?),
    };
    let prefer = prefer_arg(args)?;
    let list_only = mgmt::flag(args, "list")?;
    if list_only && (asked.is_some() || !prefer.is_empty()) {
        return Err(Error::new(
            "invalid_arguments",
            "list:true takes no trait or prefer (it only returns the trait tree)",
        ));
    }
    let resolved: RefCell<Option<Planned>> = RefCell::new(None);
    let job = Job {
        kind: "general_trait",
        action: json!({"kind":"general_trait","general":general,"trait":asked,
            "prefer":prefer,"list":list_only}),
        plan: Box::new(|process| {
            *resolved.borrow_mut() = Some(plan(process, general, asked, &prefer, list_only)?);
            Ok(())
        }),
        dispatch: Box::new(|process, keys| {
            let p = resolved.borrow().clone().ok_or_else(|| {
                Fail::Failed(Error::new("action_uncertain", "Plan missing at dispatch"))
            })?;
            drive(process, keys, &p, list_only)
        }),
        settle: Box::new(|_, seen| {
            let detail = json!({"tool":"general_trait","general":general,"trait":asked,
                "seen":seen.cloned()});
            Ok(
                match seen
                    .and_then(|v| v.get("listing_only"))
                    .and_then(Value::as_bool)
                {
                    // Listing changes nothing: whatever it saw is its result.
                    Some(true) => super::journal::Outcome::Reached(detail),
                    _ => ui_action::effect(seen, "assigned", detail),
                },
            )
        }),
    };
    ui_action::run_bounded(job, 90, id, token, process, keys, guard, authority, actions)
}

pub(super) fn tools() -> Vec<Value> {
    vec![
        json!({"name":"hoi4_general_trait","description":"Live-verified (pp13, 17 Nov 2026: a list and an assignment): give an own army general an assignable trait for command power (NDefines UNIT_LEADER_ASSIGN_TRAIT_COST, read from the game's defines; 15 in 1.19.4) with the game's own trait tree window: the card's trait button when the general leads an army or army group, else an own army's panel, its commander list and the general's Traits button; then the trait's entry in the tree. `general` is the name hoi4_commanders lists; `trait` is an assignable trait's script key or localised name (from common/unit_leader, so mods work); without `trait` the first trait the tree lets the general pick is taken, the earliest of `prefer` (keys or names) first. list:true opens the tree and returns every entry (name, script key, assignable, state owned/available/locked, can_pick), the free trait slots shown, command power and the cost, changing nothing. Typed refusals before input: unknown_commander (closest names), trait_unknown, trait_not_assignable (a trait the game grants), insufficient_command_power (\"costs N command power and M are held\"), catalog_unavailable; from the window before the click: no_own_army, general_not_listed, wrong_leader, trait_not_offered (lists the tree), trait_owned, trait_unavailable, no_free_trait_slot, no_enabled_trait, trait_not_visible. Verified by the entry turning to the owned state, else by command power dropping by about the cost.","inputSchema":{"type":"object","properties":{"general":{"type":"string","maxLength":128},"trait":{"type":"string","maxLength":128},"prefer":{"type":"array","items":{"type":"string","maxLength":128},"maxItems":24},"list":{"type":"boolean"},"request_id":{"type":"string"},"lease_token":{"type":"string"}},"required":["general","request_id","lease_token"],"additionalProperties":false}}),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn catalog() -> Catalog {
        let entries = script::parse(
            r#"
            leader_traits = {
                organizer = { type = field_marshal trait_type = basic_trait }
                logistics_wizard = { type = field_marshal trait_type = assignable_trait any_parent = { organizer } }
                offensive_doctrine = { type = field_marshal trait_type = assignable_trait mutually_exclusive = defensive_doctrine }
                panzer_expert = { type = corps_commander trait_type = assignable_trait }
                desert_fox = { type = all trait_type = assignable_terrain_trait }
            }
            "#,
        );
        let (assignable, other) = traits_from(&entries);
        let names = [
            ("logistics_wizard", "Logistics Wizard"),
            ("offensive_doctrine", "Offensive Doctrine"),
            ("panzer_expert", "Panzer Expert"),
            ("organizer", "Organizer"),
        ]
        .into_iter()
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect();
        Catalog {
            assignable,
            other,
            names,
            assign_cost: 15.0,
            base_max: 80.0,
        }
    }

    fn entry(name: &str, state: Shown) -> TraitEntry {
        TraitEntry {
            name: name.into(),
            state,
            rect: Some(Rect {
                x: 10,
                y: 20,
                w: 200,
                h: 40,
            }),
            terrain: false,
        }
    }

    #[test]
    fn assignable_traits_come_from_the_leader_trait_scripts() {
        let c = catalog();
        let keys: Vec<&str> = c.assignable.iter().map(|t| t.key.as_str()).collect();
        assert_eq!(
            keys,
            [
                "logistics_wizard",
                "offensive_doctrine",
                "panzer_expert",
                "desert_fox"
            ]
        );
        assert!(c.assignable[3].terrain);
        assert_eq!(c.assignable[2].kind, "corps_commander");
        assert_eq!(
            c.other,
            [("organizer".to_string(), "basic_trait".to_string())]
        );
    }

    #[test]
    fn a_trait_is_asked_by_key_or_localised_name() {
        let c = catalog();
        assert_eq!(
            resolve_trait(&c, "logistics_wizard").unwrap(),
            (
                "logistics_wizard".to_string(),
                "Logistics Wizard".to_string()
            )
        );
        assert_eq!(
            resolve_trait(&c, "logistics wizard").unwrap().0,
            "logistics_wizard"
        );
        // a key without an English name is shown by its key
        assert_eq!(resolve_trait(&c, "desert_fox").unwrap().1, "desert_fox");
        assert_eq!(
            resolve_trait(&c, "Organizer").unwrap_err().code,
            "trait_not_assignable"
        );
        let e = resolve_trait(&c, "panzer").unwrap_err();
        assert_eq!(e.code, "trait_unknown");
        assert!(e.message.contains("panzer_expert"), "{}", e.message);
    }

    #[test]
    fn the_cost_is_read_from_the_defines() {
        let text = "NMilitary = {\n\tUNIT_LEADER_ASSIGN_TRAIT_COST = 15.0,\t-- cost\n\tBASE_MAX_COMMAND_POWER = 80.0,\n}";
        assert_eq!(define(text, "UNIT_LEADER_ASSIGN_TRAIT_COST"), Some(15.0));
        assert_eq!(define(text, "BASE_MAX_COMMAND_POWER"), Some(80.0));
        assert_eq!(define(text, "MISSING"), None);
    }

    #[test]
    fn short_command_power_is_refused_with_cost_first_and_held_second() {
        assert!(cp_refusal(15.0, Some(15.0), "Erwin Rommel").is_none());
        assert!(cp_refusal(15.0, None, "Erwin Rommel").is_none());
        let (code, message) = cp_refusal(15.0, Some(4.5), "Erwin Rommel").unwrap();
        assert_eq!(code, "insufficient_command_power");
        // the executor's wait reads the first number as the cost and the second as held
        let numbers: Vec<f64> = message
            .split(|c: char| !(c.is_ascii_digit() || c == '.'))
            .filter_map(|w| w.trim_matches('.').parse::<f64>().ok())
            .collect();
        assert_eq!(&numbers[..2], &[15.0, 4.5]);
    }

    #[test]
    fn the_entry_to_click_is_chosen_from_the_tree_on_screen() {
        let c = catalog();
        let assignable = |n: &str| c.assignable_shown(n).is_some();
        let entries = vec![
            entry("Organizer", Shown::Owned),
            entry("Logistics Wizard", Shown::Available),
            entry("Offensive Doctrine", Shown::Available),
            entry("Panzer Expert", Shown::Locked),
        ];
        assert_eq!(
            choose(&entries, assignable, Some("Logistics Wizard"), &[], None),
            Ok(1)
        );
        assert_eq!(
            choose(&entries, assignable, Some("Organizer"), &[], None)
                .unwrap_err()
                .0,
            "trait_owned"
        );
        assert_eq!(
            choose(&entries, assignable, Some("Panzer Expert"), &[], Some(1))
                .unwrap_err()
                .0,
            "trait_unavailable"
        );
        assert_eq!(
            choose(&entries, assignable, Some("Panzer Expert"), &[], Some(0))
                .unwrap_err()
                .0,
            "no_free_trait_slot"
        );
        let e = choose(&entries, assignable, Some("Desert Fox"), &[], None).unwrap_err();
        assert_eq!(e.0, "trait_not_offered");
        assert!(e.1.contains("Logistics Wizard (available)"), "{}", e.1);
        // no trait asked: the first available assignable one, a preferred one first
        assert_eq!(choose(&entries, assignable, None, &[], None), Ok(1));
        let prefer = vec!["Offensive Doctrine".to_string()];
        assert_eq!(choose(&entries, assignable, None, &prefer, None), Ok(2));
        // a basic trait shown as available is never picked
        let basic = vec![
            entry("Organizer", Shown::Available),
            entry("Panzer Expert", Shown::Locked),
        ];
        assert_eq!(
            choose(&basic, assignable, None, &[], None).unwrap_err().0,
            "no_enabled_trait"
        );
        assert_eq!(
            choose(&basic, assignable, None, &[], Some(0))
                .unwrap_err()
                .0,
            "no_free_trait_slot"
        );
    }

    #[test]
    fn the_traits_button_sits_left_of_promote_on_the_row() {
        let portrait = Rect {
            x: 100,
            y: 300,
            w: 60,
            h: 210,
        };
        assert_eq!(traits_button(&portrait), (406.0, 488.0));
        let clip = Rect {
            x: 0,
            y: 100,
            w: 600,
            h: 600,
        };
        assert!(inside(&clip, 300.0, 120.0));
        assert!(!inside(&clip, 300.0, 720.0));
    }

    #[test]
    fn prefer_takes_a_short_list_of_names() {
        let ok = json!({"prefer": ["logistics_wizard", "Offensive Doctrine"]});
        assert_eq!(prefer_arg(ok.as_object().unwrap()).unwrap().len(), 2);
        let bad = json!({"prefer": "logistics_wizard"});
        assert!(prefer_arg(bad.as_object().unwrap()).is_err());
        let empty = json!({"prefer": [""]});
        assert!(prefer_arg(empty.as_object().unwrap()).is_err());
        assert!(prefer_arg(json!({}).as_object().unwrap())
            .unwrap()
            .is_empty());
    }

    #[test]
    fn descriptors_are_live_verified_and_lease_gated() {
        for t in tools() {
            assert!(t["description"]
                .as_str()
                .unwrap()
                .starts_with("Live-verified (pp13, 17 Nov 2026"));
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
