//! Shared GUI discovery and click helpers for the doctrine, intelligence and
//! industrial-organisation windows (all UNVALIDATED: no live run has mapped
//! where these windows live in the interface manager).
//!
//! A window is found by its exact class vtable (image-relative offsets taken
//! from the RTTI vtable map of macOS build 089EA519) instead of a guessed
//! manager slot: first the popup list of the game GUI (`gui+0x9e0`, the list
//! that already holds notices and command dialogs), then every pointer in the
//! first 0x400 bytes of the interface manager (`idler+0x670`, where the
//! research, politics and diplomacy views sit at fixed slots). The window's
//! root container is read at `+0x430` (the offset every reviewed view uses);
//! other small offsets are accepted only when the root's GUI name equals the
//! name the game's `.gui` file defines. A window that is not found is reported
//! as absent, never guessed.
//!
//! Widgets are then found by the names of the game's `.gui` files with
//! `gui_tree`; grid boxes expose their items at `+0x190` and an item's own
//! window at `+0x10` (both reviewed for the special-projects grids).
use super::ui::{read, settled};
use crate::{
    error::{Error, Result},
    game,
    gui_tree::{self, Rect},
    memory::{Reader, StringAbi},
    platform::{macos_input::MacKeys, Process},
};

pub(super) const GAME: (u32, u32) = (1920, 1080);

const MANAGER: u64 = 0x670;
const GUI_FROM_IDLER: u64 = 0x4c0;
const POPUP_LIST: u64 = 0x9e0;
const VIEW_ROOT: u64 = 0x430;
/// Other root offsets seen on reviewed windows (roster/recruit windows keep it
/// at +0x20, the politics selection window at +0x8); only trusted when the root
/// carries the expected GUI name.
const ROOT_FALLBACKS: [u64; 6] = [0x20, 0x28, 0x30, 0x8, 0x10, 0x18];
const MANAGER_SCAN: usize = 0x400;
/// Container object -> GUI-object base (gui_tree layout, macOS 089EA519).
const CONTAINER_BASE: u64 = 0x30;
const GRID_ITEMS: u64 = 0x190;
const ITEM_CONTAINER: u64 = 0x10;
const TEXT: u64 = 0x100;
const GRID_LIMIT: usize = 256;

/// A window class and the root GUI name its `.gui` file defines.
#[derive(Clone, Copy, Debug)]
pub(super) struct Class {
    pub name: &'static str,
    /// Image-relative vtable (RTTI vtable map, macOS 089EA519).
    pub vtable: u64,
    pub root_name: &'static str,
}

pub(super) const DOCTRINE_VIEW: Class = Class {
    name: "NDoctrines::CCountryDoctrineView",
    vtable: 0x334bf68,
    root_name: "country_doctrine_view",
};
pub(super) const DOCTRINE_SELECTION: Class = Class {
    name: "NDoctrines::CDoctrineSelectionList",
    vtable: 0x334c270,
    root_name: "doctrine_selection_list",
};
pub(super) const POLITICS_VIEW: Class = Class {
    name: "CCountryPoliticsView",
    vtable: 0x3444508,
    root_name: "countrypoliticsview",
};
pub(super) const OFFICER_VIEW: Class = Class {
    name: "CCountryArmyOfficerCorpView",
    vtable: 0x3494960,
    root_name: "countryofficercorpview",
};
pub(super) const AGENCY_VIEW: Class = Class {
    name: "CCountryIntelligenceAgencyView",
    vtable: 0x3418c58,
    root_name: "countryintelligenceagencyview",
};
pub(super) const AGENCY_UPGRADES: Class = Class {
    name: "CAgencyUpgradesWindow",
    vtable: 0x3416db0,
    root_name: "intelligence_agency_upgrades_window",
};
pub(super) const AGENCY_LOGO: Class = Class {
    name: "CAgencyLogoSelectionWindow",
    vtable: 0x34162c0,
    root_name: "name_logo_agency_selection_window",
};
pub(super) const OPERATIVE_BAR: Class = Class {
    name: "COperativeBottomBar",
    vtable: 0x3419878,
    root_name: "operative_leader_window",
};
pub(super) const OPERATIVE_ORDERS: Class = Class {
    name: "COperativeOrderBar",
    vtable: 0x341a8a8,
    root_name: "operative_order_bar",
};
pub(super) const OPERATIVE_RECRUIT: Class = Class {
    name: "COperativeLeaderRecruitmentWindow",
    vtable: 0x348dcf8,
    root_name: "operative_leader_window",
};
pub(super) const MIO_LIST: Class = Class {
    name: "NIndustrialOrganisation::COrganisationListWindow",
    vtable: 0x3421530,
    root_name: "industrial_organisation_list_window",
};
pub(super) const MIO_DETAIL: Class = Class {
    name: "NIndustrialOrganisation::COrganisationDetailWindow",
    vtable: 0x3420958,
    root_name: "industrial_organisation_detail_window",
};
pub(super) const MIO_POLICY: Class = Class {
    name: "NIndustrialOrganisation::CPolicyWindow",
    vtable: 0x3421dd8,
    root_name: "industrial_organisation_policy_window",
};

fn libcxx(r: &Reader<'_>) -> Result<()> {
    match r.string_abi() {
        StringAbi::LibCxx => Ok(()),
        StringAbi::LibStdCxx => Err(Error::new(
            "capability_unavailable",
            "These windows are not reviewed for this build",
        )),
    }
}
fn plausible(p: u64) -> bool {
    (0x10000..=0x7fff_ffff_0000).contains(&p) && p % 8 == 0
}

/// GUI name of a container object.
pub(super) fn container_name(r: &Reader<'_>, image: u64, object: u64) -> Result<Option<String>> {
    gui_tree::name(r, object + CONTAINER_BASE, image)
}

fn root_of(r: &Reader<'_>, image: u64, object: u64, class: &Class) -> Option<u64> {
    let pick = |offset: u64, need_name: bool| -> Option<u64> {
        let p = r.u64(object + offset).ok()?;
        if !plausible(p) {
            return None;
        }
        let root = gui_tree::container(r, p, image).ok()??;
        if need_name {
            let name = container_name(r, image, root).ok()??;
            if name != class.root_name {
                return None;
            }
        }
        Some(root)
    };
    pick(VIEW_ROOT, false).or_else(|| ROOT_FALLBACKS.iter().find_map(|o| pick(*o, true)))
}

/// Root container of the live window of `class`, or None when the game holds no
/// such window object.
pub(super) fn locate(r: &Reader<'_>, image: u64, class: &Class) -> Result<Option<u64>> {
    libcxx(r)?;
    let idler = r.u64(image + game::layout(r).idler_global)?;
    let want = image + class.vtable;
    let gui = r.u64(idler + GUI_FROM_IDLER)?;
    if plausible(gui) {
        if let Ok((_, data, count)) = r.array_header(gui + POPUP_LIST) {
            for index in 0..count.min(64) as u64 {
                let Ok(window) = r.u64(data + index * 8) else {
                    continue;
                };
                if plausible(window) && r.u64(window).ok() == Some(want) {
                    if let Some(root) = root_of(r, image, window, class) {
                        return Ok(Some(root));
                    }
                }
            }
        }
    }
    let manager = r.u64(idler + MANAGER)?;
    if !plausible(manager) {
        return Err(Error::new("invalid_gui", "Interface manager unavailable"));
    }
    let bytes = r.read(manager, MANAGER_SCAN)?;
    for chunk in bytes.chunks_exact(8) {
        let p = u64::from_le_bytes(chunk.try_into().unwrap());
        if plausible(p) && r.u64(p).ok() == Some(want) {
            if let Some(root) = root_of(r, image, p, class) {
                return Ok(Some(root));
            }
        }
    }
    Ok(None)
}

/// The window exists and is shown.
pub(super) fn is_open(r: &Reader<'_>, image: u64, class: &Class) -> Result<bool> {
    match locate(r, image, class)? {
        Some(root) => gui_tree::visible(r, root),
        None => Ok(false),
    }
}
#[cfg(test)]
fn root_name(r: &Reader<'_>, root: u64) -> Option<String> {
    container_name(r, 0x1_0000_0000, root).ok().flatten()
}
/// Root of a window that must exist (shown or not).
pub(super) fn root(r: &Reader<'_>, image: u64, class: &Class) -> Result<u64> {
    locate(r, image, class)?.ok_or_else(|| {
        Error::new(
            "window_not_found",
            format!("No {} object found in the interface", class.name),
        )
    })
}
/// Root of a window that must be shown.
pub(super) fn open_root(r: &Reader<'_>, image: u64, class: &Class) -> Result<u64> {
    let root = root(r, image, class)?;
    if gui_tree::visible(r, root)? {
        Ok(root)
    } else {
        Err(Error::new(
            "window_not_open",
            format!("{} is not shown", class.name),
        ))
    }
}

/// First widget (GUI-object base) with this name under `object`, searching
/// nested container windows depth first.
pub(super) fn find(
    r: &Reader<'_>,
    image: u64,
    object: u64,
    wanted: &str,
    depth: u32,
) -> Result<Option<u64>> {
    for child in gui_tree::children(r, object)? {
        if gui_tree::name(r, child, image)?.as_deref() == Some(wanted) {
            return Ok(Some(child));
        }
        if depth > 0 {
            if let Some(inner) = gui_tree::container(r, child, image)? {
                if let Some(found) = find(r, image, inner, wanted, depth - 1)? {
                    return Ok(Some(found));
                }
            }
        }
    }
    Ok(None)
}
pub(super) fn need(
    r: &Reader<'_>,
    image: u64,
    object: u64,
    wanted: &str,
    depth: u32,
) -> Result<u64> {
    find(r, image, object, wanted, depth)?.ok_or_else(|| {
        Error::new("gui_target_unavailable", format!("{wanted} not found"))
    })
}
/// Container object of a named child window.
pub(super) fn need_window(
    r: &Reader<'_>,
    image: u64,
    object: u64,
    wanted: &str,
    depth: u32,
) -> Result<u64> {
    let base = need(r, image, object, wanted, depth)?;
    gui_tree::container(r, base, image)?
        .ok_or_else(|| Error::new("invalid_gui", format!("{wanted} is not a window")))
}
/// A named widget exists, is shown and every window above it (inside `object`) is shown.
pub(super) fn shown(
    r: &Reader<'_>,
    image: u64,
    object: u64,
    wanted: &str,
    depth: u32,
) -> Result<bool> {
    match find(r, image, object, wanted, depth)? {
        Some(base) => gui_tree::shown(r, base),
        None => Ok(false),
    }
}

/// One item of a grid box: the item object and its own window.
#[derive(Clone, Copy, Debug)]
pub(super) struct Item {
    pub frame: u64,
}
pub(super) fn items(r: &Reader<'_>, image: u64, grid: u64) -> Result<Vec<Item>> {
    let (_, data, count) = r.array_header(grid + GRID_ITEMS)?;
    if count > GRID_LIMIT {
        return Err(Error::new("invalid_gui", "Grid item list exceeds bound"));
    }
    let bytes = if count == 0 {
        Vec::new()
    } else {
        r.read(data, count * 8)?
    };
    let mut out = Vec::new();
    for chunk in bytes.chunks_exact(8) {
        let object = u64::from_le_bytes(chunk.try_into().unwrap());
        if !plausible(object) {
            return Err(Error::new("invalid_gui", "Stale grid item"));
        }
        let frame = gui_tree::container(r, r.u64(object + ITEM_CONTAINER)?, image)?
            .ok_or_else(|| Error::new("invalid_gui", "Grid item window required"))?;
        out.push(Item { frame });
    }
    Ok(out)
}
/// Vtable of `gridBoxType` widgets (reviewed on the politics view, macOS
/// 089EA519); used to find grids whose names the `.gui` files do not give
/// (windows the game builds in code).
const GRID_VTABLE: u64 = 0x34be068;
pub(super) fn grids(
    r: &Reader<'_>,
    image: u64,
    object: u64,
    depth: u32,
    out: &mut Vec<u64>,
) -> Result<()> {
    for child in gui_tree::children(r, object)? {
        if r.u64(child)? == image + GRID_VTABLE {
            out.push(child);
        } else if depth > 0 {
            if let Some(inner) = gui_tree::container(r, child, image)? {
                grids(r, image, inner, depth - 1, out)?;
            }
        }
    }
    Ok(())
}
/// Text of a named text box child (game markup removed).
pub(super) fn text(
    r: &Reader<'_>,
    image: u64,
    parent: u64,
    wanted: &str,
    depth: u32,
) -> Result<Option<String>> {
    match find(r, image, parent, wanted, depth)? {
        Some(t) => Ok(Some(super::special_projects::plain(
            &r.string_bounded(t + TEXT, 256)?,
        ))),
        None => Ok(None),
    }
}
pub(super) fn rect(r: &Reader<'_>, image: u64, base: u64) -> Result<Rect> {
    gui_tree::rect(r, image, base, GAME)
}
/// Rectangle of the named widget under `object`.
pub(super) fn rect_of(
    r: &Reader<'_>,
    image: u64,
    object: u64,
    wanted: &str,
    depth: u32,
) -> Result<Rect> {
    rect(r, image, need(r, image, object, wanted, depth)?)
}

/// Click the centre of a rectangle once it has come to rest.
pub(super) fn click(
    process: &dyn Process,
    keys: &MacKeys,
    stage: &str,
    target: impl Fn(&Reader<'_>, u64) -> Result<Rect>,
) -> Result<Rect> {
    let rect = settled(process, stage, target)?;
    let (x, y) = rect.center();
    keys.click(GAME, x, y)?;
    Ok(rect)
}
/// A window that is not shown yet is closed by Escape (their `.gui` shortcut).
pub(super) fn dismiss_window(process: &dyn Process, keys: &MacKeys, class: &'static Class) {
    super::ui::dismiss(process, keys, |r, i| Ok(!is_open(r, i, class)?));
}
pub(super) fn open_now(process: &dyn Process, class: &Class) -> bool {
    read(process, |r, i| is_open(r, i, class)).unwrap_or(false)
}

#[cfg(test)]
pub(super) mod fixture {
    //! A sparse widget tree in the reviewed macOS layout (container vtable
    //! 0x34ac0b0, base +0x30, type at base+0x50, name at type+0x28, children
    //! array at +0xd30, visibility byte +0x95 bit 8, CButtonStandard renderer
    //! at +0xd8). Mirrors `gui_tree`'s own test fixture.
    use super::super::mgmt::fake::Mem;

    pub(crate) const IMAGE: u64 = 0x1_0000_0000;
    pub(crate) const CONTAINER_VT: u64 = 0x34ac0b0;
    pub(crate) const BUTTON_VT: u64 = 0x34aafa0;
    pub(crate) const SPRITE_VT: u64 = 0x34bdb10;
    pub(crate) const TYPE_VT: u64 = 0x100;

    pub(crate) struct Tree {
        pub mem: Mem,
        next: u64,
    }
    impl Tree {
        pub(crate) fn new() -> Self {
            let mem = Mem::default();
            mem.alloc(0x10_0000, 0x10_0000);
            Self {
                mem,
                next: 0x20_0000,
            }
        }
        pub(crate) fn alloc(&mut self, size: usize) -> u64 {
            let at = self.next;
            self.next += (size as u64 + 0x1ff) & !0xff;
            // Leave mapped room before the object: a widget's container
            // pointer is probed at base - 0x30.
            self.mem.alloc(at, size + 0x100);
            at + 0x80
        }
        fn named(&mut self, base: u64, name: &str) {
            let ty = self.alloc(0x100);
            self.mem.q(ty, IMAGE + TYPE_VT);
            self.mem.sso(ty + 0x28, name);
            self.mem.q(base + 0x50, ty);
        }
        /// A container window; returns its object (children are bases).
        pub(crate) fn container(&mut self, name: &str, visible: bool) -> u64 {
            let object = self.alloc(0x1000);
            self.mem.q(object, IMAGE + CONTAINER_VT);
            self.mem.patch(object + 0x95, &[if visible { 8 } else { 0 }]);
            self.named(object + 0x30, name);
            object
        }
        pub(crate) fn set_children(&mut self, object: u64, children: &[u64]) {
            let list = self.alloc(children.len().max(1) * 8);
            for (i, c) in children.iter().enumerate() {
                self.mem.q(list + i as u64 * 8, *c);
            }
            self.mem.q(object + 0xd30, list);
            self.mem.i(object + 0xd30 + 8, children.len() as i32);
            self.mem.i(object + 0xd30 + 12, children.len() as i32);
        }
        /// A button (CButtonStandard) with a laid-out sprite at game
        /// rectangle (x, y, w, h); returns its GUI-object base.
        pub(crate) fn button(&mut self, name: &str, x: i32, y: i32, w: i32, h: i32) -> u64 {
            let base = self.alloc(0x200);
            self.mem.q(base, IMAGE + BUTTON_VT);
            self.named(base, name);
            self.mem.patch(base + 0x65, &[8]);
            let sprite = self.alloc(0x200);
            let kind = self.alloc(0x100);
            self.mem.q(sprite, IMAGE + SPRITE_VT);
            self.mem.q(sprite + 8, kind);
            self.mem.i(kind + 0x78, 1);
            self.mem.i(kind + 0x8c, w);
            self.mem.i(kind + 0x90, h);
            let tx = (x - 960) as f32;
            let ty = (540 - y - h) as f32;
            self.mem.patch(sprite + 0xb0, &tx.to_le_bytes());
            self.mem.patch(sprite + 0xb4, &ty.to_le_bytes());
            self.mem.q(base + 0xd8, sprite);
            base
        }
        /// A plain named widget (no renderer), e.g. a text box with `text`.
        pub(crate) fn text_box(&mut self, name: &str, text: &str) -> u64 {
            let base = self.alloc(0x400);
            self.named(base, name);
            self.mem.patch(base + 0x65, &[8]);
            self.mem.sso(base + 0x100, text);
            base
        }
        /// A grid box with these item windows (object + frame at +0x10).
        pub(crate) fn grid(&mut self, name: &str, frames: &[u64]) -> u64 {
            let base = self.alloc(0x400);
            self.named(base, name);
            let list = self.alloc(frames.len().max(1) * 8);
            for (i, frame) in frames.iter().enumerate() {
                let item = self.alloc(0x40);
                self.mem.q(item + 0x10, *frame);
                self.mem.q(list + i as u64 * 8, item);
            }
            self.mem.q(base + 0x190, list);
            self.mem.i(base + 0x190 + 8, frames.len() as i32);
            self.mem.i(base + 0x190 + 12, frames.len() as i32);
            base
        }
    }
}

#[cfg(test)]
mod tests {
    use super::fixture::*;
    use super::*;
    use crate::memory::Reader;

    /// root "country_doctrine_view" { button "close_button" (100,200,50,20),
    /// container "inner" { text "label" = "Hello", grid "g" [two item windows] } }
    fn world() -> (Tree, u64) {
        let mut t = Tree::new();
        let root = t.container("country_doctrine_view", true);
        let close = t.button("close_button", 100, 200, 50, 20);
        let inner = t.container("inner", true);
        let label = t.text_box("label", "Hello");
        let mut frames = Vec::new();
        for name in ["Mobile Warfare", "Mass Assault"] {
            let frame = t.container("doctrine_list_item", true);
            let n = t.text_box("doctrine_name", name);
            let b = t.button("select_button", 10, 20, 30, 40);
            t.set_children(frame, &[n, b]);
            frames.push(frame);
        }
        let grid = t.grid("g", &frames);
        t.set_children(inner, &[label, grid]);
        // children are GUI-object bases; a container's base is object + 0x30.
        t.set_children(root, &[close, inner + 0x30]);
        (t, root)
    }

    #[test]
    fn finds_nested_widgets_text_rects_and_grid_items() {
        let (t, root) = world();
        let r = Reader::new(&t.mem);
        let close = find(&r, IMAGE, root, "close_button", 3).unwrap().unwrap();
        assert_eq!(
            rect(&r, IMAGE, close).unwrap(),
            Rect { x: 100, y: 200, w: 50, h: 20 }
        );
        assert_eq!(rect_of(&r, IMAGE, root, "close_button", 1).unwrap().center(), (125.0, 210.0));
        assert!(find(&r, IMAGE, root, "missing", 3).unwrap().is_none());
        let inner = need_window(&r, IMAGE, root, "inner", 2).unwrap();
        assert_eq!(text(&r, IMAGE, inner, "label", 1).unwrap().as_deref(), Some("Hello"));
        // depth 0 does not descend into inner windows
        assert!(find(&r, IMAGE, root, "label", 0).unwrap().is_none());
        let grid = need(&r, IMAGE, inner, "g", 1).unwrap();
        let items = items(&r, IMAGE, grid).unwrap();
        assert_eq!(items.len(), 2);
        let names: Vec<String> = items
            .iter()
            .map(|i| text(&r, IMAGE, i.frame, "doctrine_name", 1).unwrap().unwrap())
            .collect();
        assert_eq!(names, ["Mobile Warfare", "Mass Assault"]);
        assert_eq!(
            rect_of(&r, IMAGE, items[1].frame, "select_button", 1).unwrap(),
            Rect { x: 10, y: 20, w: 30, h: 40 }
        );
    }

    #[test]
    fn locates_a_window_by_vtable_in_the_manager_slots() {
        let (mut t, root) = world();
        let layout_global = game::layout(&Reader::new(&t.mem)).idler_global;
        let window = t.alloc(0x500);
        t.mem.q(window, IMAGE + DOCTRINE_VIEW.vtable);
        t.mem.q(window + VIEW_ROOT, root);
        let manager = t.alloc(0x800);
        t.mem.q(manager + 0x58, window);
        // a decoy slot with another vtable must not match
        let decoy = t.alloc(0x500);
        t.mem.q(decoy, IMAGE + 0x1234);
        t.mem.q(decoy + VIEW_ROOT, root);
        t.mem.q(manager + 0x60, decoy);
        let idler = t.alloc(0x800);
        t.mem.q(idler + MANAGER, manager);
        t.mem.alloc(IMAGE + layout_global, 8);
        t.mem.q(IMAGE + layout_global, idler);
        let r = Reader::new(&t.mem);
        assert_eq!(locate(&r, IMAGE, &DOCTRINE_VIEW).unwrap(), Some(root));
        assert!(is_open(&r, IMAGE, &DOCTRINE_VIEW).unwrap());
        assert_eq!(locate(&r, IMAGE, &AGENCY_VIEW).unwrap(), None);
        assert!(!is_open(&r, IMAGE, &AGENCY_VIEW).unwrap());
        assert_eq!(root_name(&r, root), Some("country_doctrine_view".to_string()));
        assert_eq!(open_root(&r, IMAGE, &DOCTRINE_VIEW).unwrap(), root);
        assert_eq!(open_root(&r, IMAGE, &AGENCY_VIEW).unwrap_err().code, "window_not_found");
        // hidden windows are found but not open
        t.mem.patch(root + 0x95, &[0]);
        let r = Reader::new(&t.mem);
        assert!(!is_open(&r, IMAGE, &DOCTRINE_VIEW).unwrap());
        assert_eq!(open_root(&r, IMAGE, &DOCTRINE_VIEW).unwrap_err().code, "window_not_open");
    }

    #[test]
    fn locates_a_popup_window_through_the_gui_popup_list() {
        let (mut t, root) = world();
        let layout_global = game::layout(&Reader::new(&t.mem)).idler_global;
        let window = t.alloc(0x500);
        t.mem.q(window, IMAGE + AGENCY_LOGO.vtable);
        t.mem.q(window + VIEW_ROOT, root);
        let list = t.alloc(8);
        t.mem.q(list, window);
        let gui = t.alloc(0x1000);
        t.mem.q(gui + POPUP_LIST, list);
        t.mem.i(gui + POPUP_LIST + 8, 1);
        t.mem.i(gui + POPUP_LIST + 12, 1);
        let manager = t.alloc(0x800);
        let idler = t.alloc(0x800);
        t.mem.q(idler + MANAGER, manager);
        t.mem.q(idler + GUI_FROM_IDLER, gui);
        t.mem.alloc(IMAGE + layout_global, 8);
        t.mem.q(IMAGE + layout_global, idler);
        let r = Reader::new(&t.mem);
        assert_eq!(locate(&r, IMAGE, &AGENCY_LOGO).unwrap(), Some(root));
        assert_eq!(locate(&r, IMAGE, &DOCTRINE_VIEW).unwrap(), None);
    }

    #[test]
    fn class_table_holds_distinct_reviewed_vtables() {
        let all = [
            DOCTRINE_VIEW,
            DOCTRINE_SELECTION,
            POLITICS_VIEW,
            OFFICER_VIEW,
            AGENCY_VIEW,
            AGENCY_UPGRADES,
            AGENCY_LOGO,
            OPERATIVE_BAR,
            OPERATIVE_ORDERS,
            OPERATIVE_RECRUIT,
            MIO_LIST,
            MIO_DETAIL,
            MIO_POLICY,
        ];
        let mut vt: Vec<u64> = all.iter().map(|c| c.vtable).collect();
        vt.sort_unstable();
        vt.dedup();
        assert_eq!(vt.len(), all.len());
        assert!(all.iter().all(|c| (0x3300000..0x3500000).contains(&c.vtable)));
    }
}
