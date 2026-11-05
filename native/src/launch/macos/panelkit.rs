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
    memory::Reader,
    platform::{game_input::GameInput, Process},
    xlayout::{self, fld, img},
};

pub(super) const GAME: (u32, u32) = (1920, 1080);

const MANAGER: u64 = 0x670;
const GUI_FROM_IDLER: u64 = 0x4c0;
const POPUP_LIST: u64 = 0xa70;
const VIEW_ROOT: u64 = 0x490;
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
/// Owners of the fields above (macOS 1.19.4.1 RTTI names and vtables). The
/// GUI object at idler+0x4c0, grid items and text widgets have no reviewed
/// class yet; their Linux offsets need an anchor function.
const IDLER: xlayout::Class = xlayout::Class::new("CInGameIdler", 0x35089d8);
const GAME_GUI: xlayout::Class = xlayout::Class::new("idler.gui", 0);
const CONTAINER: xlayout::Class = xlayout::Class::new("CContainerWindow", 0x36ba978);
const GRID: xlayout::Class = xlayout::Class::new("CStandardGridBox", 0x36cca80);
const GRID_ITEM: xlayout::Class = xlayout::Class::new("grid.item", 0);
const TEXT_BOX: xlayout::Class = xlayout::Class::new("gui.text", 0);

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
    vtable: 0x354e610,
    root_name: "country_doctrine_view",
};
pub(super) const DOCTRINE_SELECTION: Class = Class {
    name: "NDoctrines::CDoctrineSelectionList",
    vtable: 0x354e920,
    root_name: "doctrine_selection_list",
};
pub(super) const POLITICS_VIEW: Class = Class {
    name: "CCountryPoliticsView",
    vtable: 0x36511f0,
    root_name: "countrypoliticsview",
};
pub(super) const OFFICER_VIEW: Class = Class {
    name: "CCountryArmyOfficerCorpView",
    vtable: 0x36a2a78,
    root_name: "countryofficercorpview",
};
pub(super) const AGENCY_VIEW: Class = Class {
    name: "CCountryIntelligenceAgencyView",
    vtable: 0x36249c0,
    root_name: "countryintelligenceagencyview",
};
/// Arms Against Tyranny: the top bar's diplomacy button (E) opens the International
/// Market, whose `diplo_view_button` opens the diplomacy view (1.19.4.1).
pub(super) const MARKET_VIEW: Class = Class {
    name: "CCountryInternationalMarketView",
    vtable: 0x3613100,
    root_name: "countryinternationalmarketview",
};
pub(super) const AGENCY_UPGRADES: Class = Class {
    name: "CAgencyUpgradesWindow",
    vtable: 0x3622a70,
    root_name: "intelligence_agency_upgrades_window",
};
pub(super) const AGENCY_LOGO: Class = Class {
    name: "CAgencyLogoSelectionWindow",
    vtable: 0x3621f70,
    root_name: "name_logo_agency_selection_window",
};
pub(super) const OPERATIVE_BAR: Class = Class {
    name: "COperativeBottomBar",
    vtable: 0x36257e8,
    root_name: "operative_leader_window",
};
pub(super) const OPERATIVE_ORDERS: Class = Class {
    name: "COperativeOrderBar",
    vtable: 0x3626830,
    root_name: "operative_order_bar",
};
pub(super) const OPERATIVE_RECRUIT: Class = Class {
    name: "COperativeLeaderRecruitmentWindow",
    vtable: 0x369bd60,
    root_name: "operative_leader_window",
};
pub(super) const MIO_LIST: Class = Class {
    name: "NIndustrialOrganisation::COrganisationListWindow",
    vtable: 0x362d5b0,
    root_name: "industrial_organisation_list_window",
};
pub(super) const MIO_DETAIL: Class = Class {
    name: "NIndustrialOrganisation::COrganisationDetailWindow",
    vtable: 0x362c958,
    root_name: "industrial_organisation_detail_window",
};
pub(super) const MIO_POLICY: Class = Class {
    name: "NIndustrialOrganisation::CPolicyWindow",
    vtable: 0x362de58,
    root_name: "industrial_organisation_policy_window",
};

impl Class {
    fn owner(&self) -> xlayout::Class {
        xlayout::Class::new(self.name, self.vtable)
    }
}
fn plausible(p: u64) -> bool {
    (0x10000..=0x7fff_ffff_0000).contains(&p) && p % 8 == 0
}

/// GUI name of a container object.
pub(super) fn container_name(r: &Reader<'_>, image: u64, object: u64) -> Result<Option<String>> {
    gui_tree::name(
        r,
        object + fld(r.string_abi(), &CONTAINER, CONTAINER_BASE)?,
        image,
    )
}

fn root_of(r: &Reader<'_>, image: u64, object: u64, class: &Class) -> Option<u64> {
    let owner = class.owner();
    let pick = |offset: u64, need_name: bool| -> Option<u64> {
        let offset = fld(r.string_abi(), &owner, offset).ok()?;
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
    let abi = r.string_abi();
    let idler = r.u64(image + game::layout(r).idler_global)?;
    let want = image + img(abi, class.vtable)?;
    let gui = r.u64(idler + fld(abi, &IDLER, GUI_FROM_IDLER)?)?;
    if plausible(gui) {
        if let Ok((_, data, count)) = r.array_header(gui + fld(abi, &GAME_GUI, POPUP_LIST)?) {
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
    let manager = r.u64(idler + fld(abi, &IDLER, MANAGER)?)?;
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
    // Windows owned by another view rather than the manager.
    for (child, owner_class, offset, root_at) in CHILD_WINDOWS {
        if child.vtable != class.vtable {
            continue;
        }
        let owner = img(abi, owner_class.vtable)?;
        let offset = fld(abi, &owner_class.owner(), *offset)?;
        let root_at = fld(abi, &child.owner(), *root_at)?;
        for chunk in bytes.chunks_exact(8) {
            let p = u64::from_le_bytes(chunk.try_into().unwrap());
            if plausible(p) && r.u64(p).ok() == Some(image + owner) {
                let w = r.u64(p + offset).unwrap_or(0);
                if plausible(w) && r.u64(w).ok() == Some(want) {
                    let root = r
                        .u64(w + root_at)
                        .ok()
                        .filter(|p| plausible(*p))
                        .and_then(|p| gui_tree::container(r, p, image).ok().flatten())
                        .filter(|root| {
                            container_name(r, image, *root).ok().flatten().as_deref()
                                == Some(class.root_name)
                        });
                    if root.is_some() {
                        return Ok(root);
                    }
                }
            }
        }
    }
    Ok(None)
}
/// (child window vtable, owning view vtable, offset of the child in the owner, offset of the
/// child's root container): on 1.19.4.1 the agency upgrades window hangs off the agency view
/// at +0x4a8 with its root at +0x50 (mapped live 1 November 2026).
/// The organisation list hangs off the politics view at +0x3958 (also referenced from each
/// later organisation slot record, stride 0x470) with its root at +0xf0.
const CHILD_WINDOWS: &[(Class, Class, u64, u64)] = &[
    (AGENCY_UPGRADES, AGENCY_VIEW, 0x4a8, 0x50),
    (MIO_LIST, POLITICS_VIEW, 0x3958, 0xf0),
];

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
    find(r, image, object, wanted, depth)?
        .ok_or_else(|| Error::new("gui_target_unavailable", format!("{wanted} not found")))
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
    let abi = r.string_abi();
    let (_, data, count) = r.array_header(grid + fld(abi, &GRID, GRID_ITEMS)?)?;
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
        let item = r.u64(object + fld(abi, &GRID_ITEM, ITEM_CONTAINER)?)?;
        let frame = gui_tree::container(r, item, image)?
            .ok_or_else(|| Error::new("invalid_gui", "Grid item window required"))?;
        out.push(Item { frame });
    }
    Ok(out)
}
/// Vtable of `gridBoxType` widgets (reviewed on the politics view, macOS
/// 089EA519); used to find grids whose names the `.gui` files do not give
/// (windows the game builds in code).
const GRID_VTABLE: u64 = 0x36cca80;
pub(super) fn grids(
    r: &Reader<'_>,
    image: u64,
    object: u64,
    depth: u32,
    out: &mut Vec<u64>,
) -> Result<()> {
    let grid = image + img(r.string_abi(), GRID_VTABLE)?;
    for child in gui_tree::children(r, object)? {
        if r.u64(child)? == grid {
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
            &r.string_bounded(t + fld(r.string_abi(), &TEXT_BOX, TEXT)?, 256)?,
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
    keys: &dyn GameInput,
    stage: &str,
    target: impl Fn(&Reader<'_>, u64) -> Result<Rect>,
) -> Result<Rect> {
    let rect = settled(process, stage, target)?;
    let (x, y) = rect.center();
    keys.click(GAME, x, y)?;
    Ok(rect)
}
/// A window that is not shown yet is closed by Escape (their `.gui` shortcut).
pub(super) fn dismiss_window(process: &dyn Process, keys: &dyn GameInput, class: &'static Class) {
    super::ui::dismiss(process, keys, |r, i| Ok(!is_open(r, i, class)?));
}
pub(super) fn open_now(process: &dyn Process, class: &Class) -> bool {
    read(process, |r, i| is_open(r, i, class)).unwrap_or(false)
}

#[cfg(test)]
pub(super) mod fixture {
    //! A sparse widget tree in the reviewed macOS layout (container vtable
    //! 0x36ba978, base +0x30, type at base+0x50, name at type+0x28, children
    //! array at +0xe50 (1.19.4; +0xd30 in 1.19.3), visibility byte +0x95 bit 8, CButtonStandard renderer
    //! at +0xe0). Mirrors `gui_tree`'s own test fixture.
    use super::super::mgmt::fake::Mem;

    pub(crate) const IMAGE: u64 = 0x1_0000_0000;
    pub(crate) const CONTAINER_VT: u64 = 0x36ba978;
    pub(crate) const BUTTON_VT: u64 = 0x36b9738;
    pub(crate) const SPRITE_VT: u64 = 0x36cc528;
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
            self.mem
                .patch(object + 0x95, &[if visible { 8 } else { 0 }]);
            self.named(object + 0x30, name);
            object
        }
        pub(crate) fn set_children(&mut self, object: u64, children: &[u64]) {
            let list = self.alloc(children.len().max(1) * 8);
            for (i, c) in children.iter().enumerate() {
                self.mem.q(list + i as u64 * 8, *c);
            }
            self.mem.q(object + 0xe50, list);
            self.mem.i(object + 0xe50 + 8, children.len() as i32);
            self.mem.i(object + 0xe50 + 12, children.len() as i32);
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
            self.mem.q(base + 0xe0, sprite);
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
            Rect {
                x: 100,
                y: 200,
                w: 50,
                h: 20
            }
        );
        assert_eq!(
            rect_of(&r, IMAGE, root, "close_button", 1)
                .unwrap()
                .center(),
            (125.0, 210.0)
        );
        assert!(find(&r, IMAGE, root, "missing", 3).unwrap().is_none());
        let inner = need_window(&r, IMAGE, root, "inner", 2).unwrap();
        assert_eq!(
            text(&r, IMAGE, inner, "label", 1).unwrap().as_deref(),
            Some("Hello")
        );
        // depth 0 does not descend into inner windows
        assert!(find(&r, IMAGE, root, "label", 0).unwrap().is_none());
        let grid = need(&r, IMAGE, inner, "g", 1).unwrap();
        let items = items(&r, IMAGE, grid).unwrap();
        assert_eq!(items.len(), 2);
        let names: Vec<String> = items
            .iter()
            .map(|i| {
                text(&r, IMAGE, i.frame, "doctrine_name", 1)
                    .unwrap()
                    .unwrap()
            })
            .collect();
        assert_eq!(names, ["Mobile Warfare", "Mass Assault"]);
        assert_eq!(
            rect_of(&r, IMAGE, items[1].frame, "select_button", 1).unwrap(),
            Rect {
                x: 10,
                y: 20,
                w: 30,
                h: 40
            }
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
        assert_eq!(
            root_name(&r, root),
            Some("country_doctrine_view".to_string())
        );
        assert_eq!(open_root(&r, IMAGE, &DOCTRINE_VIEW).unwrap(), root);
        assert_eq!(
            open_root(&r, IMAGE, &AGENCY_VIEW).unwrap_err().code,
            "window_not_found"
        );
        // hidden windows are found but not open
        t.mem.patch(root + 0x95, &[0]);
        let r = Reader::new(&t.mem);
        assert!(!is_open(&r, IMAGE, &DOCTRINE_VIEW).unwrap());
        assert_eq!(
            open_root(&r, IMAGE, &DOCTRINE_VIEW).unwrap_err().code,
            "window_not_open"
        );
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
        // __DATA,__const vtables of the 1.19.4.1 (f584) Mach-O lie in this window.
        assert!(all
            .iter()
            .all(|c| (0x3480000..0x3800000).contains(&c.vtable)));
    }
}

/// Probe-only (`VTABLE_HEX`): interface-manager slots whose view holds objects of that class,
/// directly or in an array one level down (slot, field, element count).
pub(super) fn debug_owner(process: &dyn Process, spec: &str) -> Result<serde_json::Value> {
    let vt = u64::from_str_radix(spec.trim_start_matches("0x"), 16)
        .map_err(|_| Error::new("invalid_arguments", "VTABLE_HEX"))?;
    read(process, |r, i| {
        let want = i + vt;
        let idler = r.u64(i + game::layout(r).idler_global)?;
        let manager = r.u64(idler + MANAGER)?;
        let slots = r.read(manager, MANAGER_SCAN)?;
        let mut hits = Vec::new();
        for (si, c) in slots.chunks_exact(8).enumerate() {
            let view = u64::from_le_bytes(c.try_into().unwrap());
            if !plausible(view) {
                continue;
            }
            let Ok(body) = r.read(view, 0x6000) else {
                continue;
            };
            for (fi, d) in body.chunks_exact(8).enumerate() {
                let q = u64::from_le_bytes(d.try_into().unwrap());
                if !plausible(q) {
                    continue;
                }
                if r.u64(q).ok() == Some(want) {
                    hits.push(serde_json::json!({"slot": format!("{:#x}", si * 8), "field": format!("{:#x}", fi * 8), "depth": 1}));
                    continue;
                }
                if let Ok(inner) = r.read(q, 0x200) {
                    let n = inner
                        .chunks_exact(8)
                        .filter(|e| {
                            let o = u64::from_le_bytes((*e).try_into().unwrap());
                            plausible(o) && r.u64(o).ok() == Some(want)
                        })
                        .count();
                    if n > 0 {
                        hits.push(serde_json::json!({"slot": format!("{:#x}", si * 8), "field": format!("{:#x}", fi * 8), "depth": 2, "count": n}));
                    }
                }
            }
        }
        Ok(serde_json::json!({"hits": hits}))
    })
}

/// Probe-only (`SLOT_HEX:VTABLE_HEX`): three-level search from one manager slot's view for
/// objects of a class; returns the field paths.
pub(super) fn debug_deep(process: &dyn Process, spec: &str) -> Result<serde_json::Value> {
    let (slot, vt) = spec
        .split_once(':')
        .and_then(|(a, b)| {
            Some((
                u64::from_str_radix(a.trim_start_matches("0x"), 16).ok()?,
                u64::from_str_radix(b.trim_start_matches("0x"), 16).ok()?,
            ))
        })
        .ok_or_else(|| Error::new("invalid_arguments", "SLOT_HEX:VTABLE_HEX"))?;
    read(process, |r, i| {
        let want = i + vt;
        let idler = r.u64(i + game::layout(r).idler_global)?;
        let view = r.u64(r.u64(idler + MANAGER)? + slot)?;
        let words = |p: u64, n: usize| -> Vec<u64> {
            r.read(p, n)
                .map(|b| {
                    b.chunks_exact(8)
                        .map(|c| u64::from_le_bytes(c.try_into().unwrap()))
                        .collect()
                })
                .unwrap_or_default()
        };
        let mut hits = Vec::new();
        for (a, p1) in words(view, 0x2000).into_iter().enumerate() {
            if !plausible(p1) {
                continue;
            }
            for (b, p2) in words(p1, 0x400).into_iter().enumerate() {
                if !plausible(p2) {
                    continue;
                }
                if r.u64(p2).ok() == Some(want) {
                    hits.push(format!("+{:#x} -> +{:#x}", a * 8, b * 8));
                    continue;
                }
                for (c, p3) in words(p2, 0x100).into_iter().enumerate() {
                    if plausible(p3) && r.u64(p3).ok() == Some(want) {
                        hits.push(format!("+{:#x} -> +{:#x} -> +{:#x}", a * 8, b * 8, c * 8));
                    }
                }
            }
            if hits.len() > 40 {
                break;
            }
        }
        Ok(serde_json::json!({"hits": hits}))
    })
}

/// Probe-only (`WIDGET_NAME`): manager slots whose view root contains a widget of that name
/// (root offset tried: VIEW_ROOT and the fallbacks), with the first match's rectangle.
pub(super) fn debug_widget_anywhere(
    process: &dyn Process,
    name: &str,
) -> Result<serde_json::Value> {
    read(process, |r, i| {
        let idler = r.u64(i + game::layout(r).idler_global)?;
        let manager = r.u64(idler + MANAGER)?;
        let slots = r.read(manager, MANAGER_SCAN)?;
        let mut hits = Vec::new();
        for (si, c) in slots.chunks_exact(8).enumerate() {
            let view = u64::from_le_bytes(c.try_into().unwrap());
            if !plausible(view) {
                continue;
            }
            for off in [
                VIEW_ROOT, 0x68, 0x50, 0x60, 0x70, 0x78, 0x430, 0x20, 0x28, 0x30, 0x8, 0x10, 0x18,
            ] {
                let Ok(p) = r.u64(view + off) else { continue };
                if !plausible(p) {
                    continue;
                }
                let Ok(Some(root)) = gui_tree::container(r, p, i) else {
                    continue;
                };
                if let Ok(Some(w)) = find(r, i, root, name, 6) {
                    let rect = gui_tree::rect(r, i, w, GAME)
                        .map(|x| [x.x, x.y, x.w, x.h])
                        .ok();
                    hits.push(serde_json::json!({"slot": format!("{:#x}", si * 8), "root_at": format!("{off:#x}"), "rect": rect, "root": format!("{root:#x}")}));
                    break;
                }
            }
        }
        Ok(serde_json::json!({"hits": hits}))
    })
}

/// Probe-only (`SLOT_HEX:ROOT_HEX`): every named widget under one manager view's root with
/// its rect and position fields.
pub(super) fn debug_tree(process: &dyn Process, spec: &str) -> Result<serde_json::Value> {
    let chain: Vec<u64> = spec
        .split(':')
        .map(|a| u64::from_str_radix(a.trim_start_matches("0x"), 16))
        .collect::<std::result::Result<_, _>>()
        .map_err(|_| Error::new("invalid_arguments", "SLOT_HEX:OFF_HEX[:OFF_HEX...]"))?;
    read(process, |r, i| {
        let idler = r.u64(i + game::layout(r).idler_global)?;
        let mut p = r.u64(idler + MANAGER)?;
        for off in &chain {
            p = r.u64(p + off)?;
        }
        let root =
            gui_tree::container(r, p, i)?.ok_or_else(|| Error::new("invalid_gui", "root"))?;
        let mut out = Vec::new();
        let mut stack = vec![(root, 0u32, String::new())];
        while let Some((obj, depth, path)) = stack.pop() {
            for child in gui_tree::children(r, obj)? {
                let name = gui_tree::name(r, child, i)?.unwrap_or_default();
                let p = format!("{path}/{name}");
                let rect = gui_tree::rect_unclipped(r, i, child, GAME)
                    .map(|x| [x.x, x.y, x.w, x.h])
                    .ok();
                out.push(serde_json::json!({"path": p, "shown": gui_tree::shown(r, child).unwrap_or(false), "rect": rect}));
                if depth < 8 {
                    if let Ok(Some(inner)) = gui_tree::container(r, child, i) {
                        stack.push((inner, depth + 1, p));
                    }
                }
            }
        }
        Ok(serde_json::json!(out))
    })
}

/// Probe-only (`OFF_HEX:...:LEN_HEX`): bytes at the end of a pointer chain from the manager.
pub(super) fn debug_bytes(process: &dyn Process, spec: &str) -> Result<serde_json::Value> {
    let mut chain: Vec<u64> = spec
        .split(':')
        .map(|a| u64::from_str_radix(a.trim_start_matches("0x"), 16))
        .collect::<std::result::Result<_, _>>()
        .map_err(|_| Error::new("invalid_arguments", "OFF_HEX:...:LEN_HEX"))?;
    let len = chain.pop().unwrap_or(0x100).min(0x800);
    read(process, |r, i| {
        let idler = r.u64(i + game::layout(r).idler_global)?;
        let mut p = r.u64(idler + MANAGER)?;
        for off in &chain {
            p = r.u64(p + off)?;
        }
        let b = r.read(p, len as usize)?;
        let rows: Vec<String> = b
            .chunks(16)
            .enumerate()
            .map(|(n, c)| {
                format!(
                    "{:04x}: {}",
                    n * 16,
                    c.iter()
                        .map(|x| format!("{x:02x}"))
                        .collect::<Vec<_>>()
                        .join(" ")
                )
            })
            .collect();
        Ok(serde_json::json!(rows))
    })
}

/// Probe-only: every global alert icon in the top bar (manager +0x188, root +0x68): the icon
/// widget's sprite words, to find the shown frame.
pub(super) fn debug_alerts(process: &dyn Process) -> Result<serde_json::Value> {
    read(process, |r, i| {
        let idler = r.u64(i + game::layout(r).idler_global)?;
        let view = r.u64(r.u64(idler + MANAGER)? + 0x188)?;
        let root = gui_tree::container(r, r.u64(view + 0x68)?, i)?
            .ok_or_else(|| Error::new("invalid_gui", "top bar root"))?;
        let mut stack = vec![(root, 0u32)];
        let mut out = Vec::new();
        while let Some((obj, depth)) = stack.pop() {
            for child in gui_tree::children(r, obj)? {
                let name = gui_tree::name(r, child, i)?.unwrap_or_default();
                if name == "global_alerticon_window" {
                    if let Some(win) = gui_tree::container(r, child, i)? {
                        if let Some(icon) = gui_tree::child(r, i, win, "icon")? {
                            let primary = r.u64(icon)?.wrapping_sub(i);
                            let sprite = r.u64(icon + 0xe0).unwrap_or(0);
                            let words: Vec<String> = r
                                .read(sprite, 0xc0)
                                .map(|b| {
                                    b.chunks_exact(4)
                                        .map(|c| {
                                            format!(
                                                "{:x}",
                                                u32::from_le_bytes(c.try_into().unwrap())
                                            )
                                        })
                                        .collect()
                                })
                                .unwrap_or_default();
                            out.push(serde_json::json!({"shown": gui_tree::shown(r, icon).unwrap_or(false), "icon_vt": format!("{primary:#x}"), "sprite": words.join(" ")}));
                        }
                    }
                } else if depth < 6 {
                    if let Ok(Some(inner)) = gui_tree::container(r, child, i) {
                        stack.push((inner, depth + 1));
                    }
                }
            }
        }
        Ok(serde_json::json!({"count": out.len(), "alerts": out}))
    })
}
