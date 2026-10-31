//! Air UI reads on macOS (089EA519): the strategic air map's base icons, the air selection toolbar,
//! the wing creation / reorganisation window and the strategic air region window. Memory reads only;
//! every rectangle is a click target, never an authorisation.
//!
//! Mapped live on 26 October 2026 (Germany, 1 January 1936, 1920x1080, F3 strategic air map mode):
//! * `CMapIconManagerImpl` (image global +0x372c530) holds one array per map icon type (header
//!   stride 0x18 from +0x8); the array whose elements carry the `CAirBaseMapIcon` vtable lists every
//!   air base icon of every country: world position floats at +0x48/+0x50 (x, z of the
//!   `air_base` row of map/buildings.txt), the icon's container window at +0x10 whose `bg_btn`
//!   rectangle sits 7 px left and 6 px above the base circle's centre (clicks at that centre hit the
//!   base: seen twice, 26 October 2026);
//! * pop-up windows live in the game GUI's list (idler +0x4c0, list +0x9e0, root +0x430): the wing
//!   window `air_wing_reorganization` (`CAirWingReorganizationWindow`) has `list_left` (reserve
//!   equipment) and `list_right` (the wing), entries with `count`, `name` (class) and `type`
//!   (variant) texts and a `bg` button, `confirm_btn`, `close_button`, `current_plane_count`;
//! * the strategic air view (manager +0x198, root +0x430) is the region window a left click on a
//!   region opens: `region_name`, `wing_type_left_text_1..4` (our fighters, bombers, CAS, AA),
//!   `wing_type_right_text_1..4` (the enemy's, same order), detection and efficiency percentages.
use super::{air, navalui as nui};
use crate::{
    error::{Error, Result},
    game,
    gui_tree::{self, Rect},
    memory::Reader,
};

pub(super) const GAME: (u32, u32) = (1920, 1080);
const ICON_GLOBAL: u64 = 0x372c530;
const ICON_VTABLE: u64 = 0x35e77d8;
const ICON_ARRAYS: u64 = 0x8;
const ICON_ARRAY_STRIDE: u64 = 0x18;
const ICON_TYPES: u64 = 34;
const ICON_CONTAINER: u64 = 0x10;
const ICON_POSITION: u64 = 0x48;
/// The base circle's centre relative to the `bg_btn` rectangle's top-left corner.
pub(super) const ICON_CENTER: (i32, i32) = (7, 6);
const GUI_FROM_IDLER: u64 = 0x4c0;
const POPUP_LIST: u64 = 0xa70;
const POPUP_ROOT: u64 = 0x490;
const CONTAINER_BASE: u64 = 0x30;
pub(super) const SLOT_STRATEGIC: u64 = 0x198;
const SLOT_SELECTION: u64 = 0x200;
const SELECTED_WINGS: u64 = 0x60;
const STRATEGIC_ROOT: u64 = 0x490;

#[derive(Clone, Debug, PartialEq)]
pub(super) struct BaseIcon {
    pub x: f64,
    pub z: f64,
    pub rect: Rect,
}
impl BaseIcon {
    /// Screen point of the base circle.
    pub fn center(&self) -> (f64, f64) {
        (
            f64::from(self.rect.x + ICON_CENTER.0),
            f64::from(self.rect.y + ICON_CENTER.1),
        )
    }
}
/// Every air base icon the map currently shows (any country), with its bg rectangle.
pub(super) fn base_icons(r: &Reader<'_>, image: u64) -> Result<Vec<BaseIcon>> {
    nui::libcxx(r)?;
    let manager = r.u64(image + ICON_GLOBAL)?;
    if manager == 0 {
        return Err(Error::new("invalid_gui", "Map icon manager unavailable"));
    }
    let mut out = Vec::new();
    for k in 0..ICON_TYPES {
        let Ok((_, data, count)) = r.array_header(manager + ICON_ARRAYS + k * ICON_ARRAY_STRIDE)
        else {
            continue;
        };
        if count == 0 || count > 4096 {
            continue;
        }
        if r.u64(r.u64(data)?)? != image + ICON_VTABLE {
            continue;
        }
        for i in 0..count as u64 {
            let icon = r.u64(data + 8 * i)?;
            let Some(container) = gui_tree::container(r, r.u64(icon + ICON_CONTAINER)?, image)?
            else {
                continue;
            };
            if !gui_tree::visible(r, container)? {
                continue;
            }
            let raw = r.read(icon + ICON_POSITION, 12)?;
            let x = f64::from(f32::from_le_bytes(raw[0..4].try_into().unwrap()));
            let z = f64::from(f32::from_le_bytes(raw[8..12].try_into().unwrap()));
            let Some(bg) = gui_tree::child(r, image, container, "bg_btn")? else {
                continue;
            };
            let Ok(rect) = gui_tree::rect_unclipped(r, image, bg, GAME) else {
                continue;
            };
            out.push(BaseIcon { x, z, rect });
        }
        break;
    }
    Ok(out)
}
/// The icon whose world position is the base position `pos` (x, z).
pub(super) fn icon_at(icons: &[BaseIcon], pos: (f64, f64)) -> Option<&BaseIcon> {
    icons
        .iter()
        .find(|i| (i.x - pos.0).abs() < 0.06 && (i.z - pos.1).abs() < 0.06)
}
/// Whether a base circle at `p` is a safe click point: on the map, clear of the toolbars and the
/// left panels and of the right-hand controls.
pub(super) fn clickable(p: (f64, f64)) -> bool {
    p.0 > 660.0 && p.0 < 1740.0 && p.1 > 160.0 && p.1 < 960.0
        && !(p.0 > 1380.0 && p.1 < 330.0)
        && !(p.0 > 1000.0 && p.0 < 1300.0 && p.1 > 960.0)
}

// ---- pop-up windows ----------------------------------------------------------------------------
fn popup_root(r: &Reader<'_>, image: u64, wanted: &str) -> Result<Option<u64>> {
    nui::libcxx(r)?;
    let idler = r.u64(image + game::layout(r).idler_global)?;
    let gui = r.u64(idler + GUI_FROM_IDLER)?;
    let (_, data, count) = r.array_header(gui + POPUP_LIST)?;
    for index in 0..count.min(32) as u64 {
        let found = (|| -> Result<Option<u64>> {
            let window = r.u64(data + index * 8)?;
            let Some(root) = gui_tree::container(r, r.u64(window + POPUP_ROOT)?, image)? else {
                return Ok(None);
            };
            if gui_tree::name(r, root + CONTAINER_BASE, image)?.as_deref() != Some(wanted)
                || !gui_tree::visible(r, root)?
            {
                return Ok(None);
            }
            Ok(Some(root))
        })();
        if let Ok(Some(root)) = found {
            return Ok(Some(root));
        }
    }
    Ok(None)
}

/// One equipment row of the wing window.
#[derive(Clone, Debug, PartialEq)]
pub(super) struct Entry {
    /// Variant name (`type` text, e.g. "Ju 86").
    pub variant: String,
    /// Equipment class name (`name` text, e.g. "Inter-War Bomber").
    pub class: String,
    pub count: i32,
    pub bg: Rect,
}
pub(super) fn reorg_open(r: &Reader<'_>, image: u64) -> Result<bool> {
    Ok(popup_root(r, image, "air_wing_reorganization")?.is_some())
}
fn reorg(r: &Reader<'_>, image: u64) -> Result<u64> {
    popup_root(r, image, "air_wing_reorganization")?
        .ok_or_else(|| Error::new("gui_target_unavailable", "The wing window is not open"))
}
/// Rows of `list_left` (reserve) or `list_right` (the wing being formed).
pub(super) fn reorg_entries(r: &Reader<'_>, image: u64, side: &str) -> Result<Vec<Entry>> {
    let root = reorg(r, image)?;
    let window = nui::window_of(r, image, root, side)?;
    let grid = nui::child_of(r, image, window, "list")?;
    let mut out = Vec::new();
    for item in nui::grid_items(r, grid)?.into_iter().take(64) {
        let Some(frame) = gui_tree::container(r, r.u64(item + 0x10)?, image)? else {
            continue;
        };
        let count = nui::text_at(r, nui::child_of(r, image, frame, "count")?)?;
        out.push(Entry {
            variant: nui::text_at(r, nui::child_of(r, image, frame, "type")?)?,
            class: nui::text_at(r, nui::child_of(r, image, frame, "name")?)?,
            count: count.trim().replace(',', "").parse().unwrap_or(0),
            bg: gui_tree::rect_unclipped(r, image, nui::child_of(r, image, frame, "bg")?, GAME)?,
        });
    }
    Ok(out)
}
/// (confirm button rectangle, its raw state: 3 disabled).
pub(super) fn reorg_confirm(r: &Reader<'_>, image: u64) -> Result<(Rect, i32)> {
    let root = reorg(r, image)?;
    let button = nui::child_of(r, image, root, "confirm_btn")?;
    Ok((
        gui_tree::rect(r, image, button, GAME)?,
        gui_tree::button_state(r, button)?,
    ))
}
pub(super) fn reorg_close(r: &Reader<'_>, image: u64) -> Result<Rect> {
    let root = reorg(r, image)?;
    gui_tree::rect(r, image, nui::child_of(r, image, root, "close_button")?, GAME)
}
/// Planes now in the wing being formed (the window's own counter).
pub(super) fn reorg_planes(r: &Reader<'_>, image: u64) -> Result<i32> {
    let root = reorg(r, image)?;
    let text = nui::text_at(r, nui::child_of(r, image, root, "current_plane_count")?)?;
    Ok(text.trim().replace(',', "").parse().unwrap_or(0))
}

/// One wing's entry in the stack beside a base circle (`CAirWingMapStackItem`).
#[derive(Clone, Debug, PartialEq)]
pub(super) struct StackItem {
    pub planes: i32,
    pub rect: Rect,
    pub selected: bool,
}
/// The wing stack of the base icon at `pos`, in the game's order. A click on an item selects its
/// wing, Shift adds it (mapped live 26 October 2026: Brandenburg's three wings listed 80/80/80).
pub(super) fn stack_at(r: &Reader<'_>, image: u64, pos: (f64, f64)) -> Result<Option<Vec<StackItem>>> {
    nui::libcxx(r)?;
    let manager = r.u64(image + ICON_GLOBAL)?;
    for k in 0..ICON_TYPES {
        let Ok((_, data, count)) = r.array_header(manager + ICON_ARRAYS + k * ICON_ARRAY_STRIDE)
        else {
            continue;
        };
        if count == 0 || count > 4096 || r.u64(r.u64(data)?)? != image + ICON_VTABLE {
            continue;
        }
        for i in 0..count as u64 {
            let icon = r.u64(data + 8 * i)?;
            let raw = r.read(icon + ICON_POSITION, 12)?;
            let x = f64::from(f32::from_le_bytes(raw[0..4].try_into().unwrap()));
            let z = f64::from(f32::from_le_bytes(raw[8..12].try_into().unwrap()));
            if (x - pos.0).abs() >= 0.06 || (z - pos.1).abs() >= 0.06 {
                continue;
            }
            let Some(container) = gui_tree::container(r, r.u64(icon + ICON_CONTAINER)?, image)? else {
                return Ok(None);
            };
            let Some(grid) = gui_tree::child(r, image, container, "wings_box")? else {
                return Ok(None);
            };
            let mut out = Vec::new();
            for item in nui::grid_items(r, grid)? {
                let Some(frame) = gui_tree::container(r, r.u64(item + 0x10)?, image)? else {
                    continue;
                };
                let bg = nui::child_of(r, image, frame, "count_bg")?;
                let planes = nui::text_at(r, nui::child_of(r, image, frame, "count_txt")?)?;
                out.push(StackItem {
                    planes: planes.trim().replace(',', "").parse().unwrap_or(0),
                    rect: gui_tree::rect_unclipped(r, image, bg, GAME)?,
                    selected: gui_tree::shown(r, nui::child_of(r, image, frame, "count_selected")?)?,
                });
            }
            return Ok(Some(out));
        }
        break;
    }
    Ok(None)
}

// ---- air selection toolbar -----------------------------------------------------------------------
/// Names of the selected wings: the air selection view keeps them in an array at +0x60 (found
/// live 26 October 2026: `Jagdgeschwader 132` after one row click, two names after a Shift click on
/// a second entry of the base panel).
pub(super) fn selected_wings(r: &Reader<'_>, image: u64) -> Result<Vec<String>> {
    let view = nui::view(r, image, SLOT_SELECTION)?;
    let (_, data, count) = r.array_header(view + SELECTED_WINGS)?;
    if count > 64 {
        return Err(Error::new("invalid_array", "Selection exceeds bound"));
    }
    let mut out = Vec::new();
    for i in 0..count as u64 {
        let wing = r.u64(data + 8 * i)?;
        if r.u64(wing)? != image + super::airread::WING_VTABLE {
            return Err(Error::new("invalid_object", "Selected wing expected"));
        }
        out.push(r.string_bounded(wing + super::airread::WING_NAME, 128)?);
    }
    Ok(out)
}
/// Rectangle and raw state (3 disabled) of a named air toolbar button.
pub(super) fn toolbar(r: &Reader<'_>, image: u64, name: &str) -> Result<(Rect, i32)> {
    let root = air::selection_root(r, image)?;
    let bar = air::named_child(r, image, root, "air_wing_toolbar")?
        .ok_or_else(|| Error::new("gui_target_unavailable", "Air toolbar not found"))?;
    let button = nui::child_of(r, image, bar, name)?;
    Ok((
        gui_tree::rect(r, image, button, GAME)?,
        gui_tree::button_state(r, button)?,
    ))
}

// ---- strategic air region window -----------------------------------------------------------------
#[derive(Clone, Debug, PartialEq)]
pub(super) struct RegionView {
    pub name: String,
    /// Our and the enemy's active planes: fighters, bombers, CAS, anti-air guns.
    pub ours: [i32; 4],
    pub theirs: [i32; 4],
    pub our_detection: String,
    pub our_efficiency: String,
    pub enemy_detection: String,
    pub enemy_efficiency: String,
    pub close: Rect,
}
fn strategic_root(r: &Reader<'_>, image: u64) -> Result<Option<u64>> {
    let view = nui::view(r, image, SLOT_STRATEGIC)?;
    let Some(root) = gui_tree::container(r, r.u64(view + STRATEGIC_ROOT)?, image)? else {
        return Ok(None);
    };
    Ok(gui_tree::visible(r, root)?.then_some(root))
}
pub(super) fn region_view(r: &Reader<'_>, image: u64) -> Result<Option<RegionView>> {
    let Some(root) = strategic_root(r, image)? else {
        return Ok(None);
    };
    let text = |name: &str| -> Result<String> {
        Ok(nui::text_at(r, nui::child_of(r, image, root, name)?)?.trim().to_string())
    };
    let number = |name: &str| -> Result<i32> {
        Ok(text(name)?.replace(',', "").parse().unwrap_or(0))
    };
    let close = gui_tree::rect(r, image, nui::child_of(r, image, root, "close_button")?, GAME)?;
    let name = text("region_name")?;
    if name.is_empty() {
        return Ok(None);
    }
    Ok(Some(RegionView {
        name,
        ours: [
            number("wing_type_left_text_1")?,
            number("wing_type_left_text_2")?,
            number("wing_type_left_text_3")?,
            number("wing_type_left_text_4")?,
        ],
        theirs: [
            number("wing_type_right_text_1")?,
            number("wing_type_right_text_2")?,
            number("wing_type_right_text_3")?,
            number("wing_type_right_text_4")?,
        ],
        our_detection: text("detection_left_text")?,
        our_efficiency: text("effeciency_left_text")?,
        enemy_detection: text("detection_right_text")?,
        enemy_efficiency: text("effeciency_right_text")?,
        close,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn icon_matching_and_click_points() {
        let icons = vec![
            BaseIcon { x: 3024.78, z: 1565.10, rect: Rect { x: 1397, y: 603, w: 189, h: 30 } },
            BaseIcon { x: 2975.56, z: 1479.55, rect: Rect { x: 100, y: 100, w: 189, h: 30 } },
        ];
        let hit = icon_at(&icons, (3024.78, 1565.1)).unwrap();
        // The point clicked live on 26 October 2026 (game pixels).
        assert_eq!(hit.center(), (1404.0, 609.0));
        assert!(icon_at(&icons, (3000.0, 1500.0)).is_none());
        assert!(clickable((1404.0, 609.0)));
        assert!(!clickable((300.0, 300.0)));
        assert!(!clickable((1600.0, 200.0)));
        assert!(!clickable((2455.0, 896.0)));
    }
}
