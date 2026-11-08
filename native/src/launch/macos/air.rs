//! Air wings and their missions with ordinary input. The Air Forces
//! Overview (L) lists own wings with base, region and active mission icons;
//! a wing is chosen by its row, a mission by its toolbar button (identified
//! by the icon frame, which is the engine's mission index + 1), and a region
//! by a right click on a state the Find view has centred. Verified from the
//! overview: the wing shows a region and the mission's icon.
use super::ui::{clear_notices, find_state, read, settled, wait};
use crate::{
    error::{Error, Result},
    game,
    gui_tree::{self, Rect},
    memory::Reader,
    platform::{
        game_input::{GameInput, Key},
        Process,
    },
    xlayout::{fld, img, Class},
};
use serde_json::{json, Value};

#[derive(Clone, Copy)]
struct Ui {
    manager: u64,
    overview: u64,
    overview_root: u64,
    selection: u64,
    selection_root: u64,
    container_vtable: u64,
    container_base: u64,
    grid_items: u64,
    item_container: u64,
    text: u64,
    icon_sprite: u64,
    sprite_frame: u64,
}
/// macOS Mach-O 089EA519, mapped live on 18 October 2026 (Germany test run):
/// the military overview (+0x158) shows air wings in air_units/item_grid;
/// the air selection view (+0x200) holds missions_grid and the toolbar; a
/// fighter wing given Air Superiority over Brandenburg showed "Eastern
/// Germany" and frame 1.
const MACOS: Ui = Ui {
    manager: 0x670,
    overview: 0x158,
    overview_root: 0x490,
    selection: 0x200,
    selection_root: 0x28,
    container_vtable: 0x36ba978,
    container_base: 0x30,
    grid_items: 0x190,
    item_container: 0x10,
    text: 0x100,
    // 1.19.4: CIcon renderer moved 0xd0 -> 0xd8 (live: mission toolbar icon +0xd8 is the CSprite).
    icon_sprite: 0xd8,
    sprite_frame: 0x34,
};
/// Owners of the fields above (macOS 1.19.4.1 RTTI names and vtables; 0 where
/// none was reviewed).
const IDLER: Class = Class::new("CInGameIdler", 0x35089d8);
const MANAGER: Class = Class::new("idler.manager", 0);
const OVERVIEW_VIEW: Class = Class::new("air.overview.view", 0);
const SELECTION_VIEW: Class = Class::new("air.selection.view", 0);
const CONTAINER: Class = Class::new("CContainerWindow", 0x36ba978);
const GRID: Class = Class::new("CStandardGridBox", 0x36cca80);
const GRID_ITEM: Class = Class::new("grid.item", 0);
const TEXT_BOX: Class = Class::new("gui.text", 0);
const ICON: Class = Class::new("CIcon", 0x36b6880);
const SPRITE: Class = Class::new("CSprite", 0x36cc528);
impl Ui {
    /// The macOS keys translated for this build (identity on macOS).
    fn resolve(&self, abi: crate::memory::StringAbi) -> Result<Self> {
        Ok(Self {
            manager: fld(abi, &IDLER, self.manager)?,
            overview: fld(abi, &MANAGER, self.overview)?,
            overview_root: fld(abi, &OVERVIEW_VIEW, self.overview_root)?,
            selection: fld(abi, &MANAGER, self.selection)?,
            selection_root: fld(abi, &SELECTION_VIEW, self.selection_root)?,
            container_vtable: img(abi, self.container_vtable)?,
            container_base: fld(abi, &CONTAINER, self.container_base)?,
            grid_items: fld(abi, &GRID, self.grid_items)?,
            item_container: fld(abi, &GRID_ITEM, self.item_container)?,
            text: fld(abi, &TEXT_BOX, self.text)?,
            icon_sprite: fld(abi, &ICON, self.icon_sprite)?,
            sprite_frame: fld(abi, &SPRITE, self.sprite_frame)?,
        })
    }
}
const GAME: (u32, u32) = (1920, 1080);
/// Visible band of the overview's wing rows.
// 1.19.4.1: the overview list runs down to the "Active in N strategic areas" footer at
// y about 1050 (measured live 1 November 2026 with 70 wings).
const ROWS: (i32, i32) = (380, 1040);

/// Mission names and their icon frames (AIR_MISSION_SPOTTING_FACTORS order + 1).
pub(super) const MISSIONS: [(&str, i32); 14] = [
    ("air_superiority", 1),
    ("cas", 2),
    ("interception", 3),
    ("strategic_bombing", 4),
    ("naval_strike", 5),
    ("kamikaze", 8),
    ("port_strike", 9),
    ("logistics_strike", 10),
    ("air_supply", 11),
    ("training", 12),
    ("naval_mines", 13),
    ("naval_minesweeping", 14),
    ("recon", 15),
    ("naval_patrol", 16),
];
fn mission_name(frame: i32) -> Option<&'static str> {
    MISSIONS.iter().find(|(_, f)| *f == frame).map(|(n, _)| *n)
}

fn ui(r: &Reader<'_>) -> Result<Ui> {
    MACOS.resolve(r.string_abi())
}
/// `slot` and `root` are already translated (fields of `ui(r)`).
pub(super) fn view_root(r: &Reader<'_>, image: u64, slot: u64, root: u64) -> Result<u64> {
    let u = ui(r)?;
    let idler = r.u64(image + game::layout(r).idler_global)?;
    let view = r.u64(r.u64(idler + u.manager)? + slot)?;
    let object = r.u64(view + root)?;
    if r.u64(object)? != image + u.container_vtable {
        return Err(Error::new("invalid_gui", "Air view root required"));
    }
    gui_tree::container(r, object + u.container_base, image)?
        .ok_or_else(|| Error::new("invalid_gui", "Air view root required"))
}
pub(super) fn named_child(
    r: &Reader<'_>,
    image: u64,
    object: u64,
    name: &str,
) -> Result<Option<u64>> {
    for child in gui_tree::children(r, object)? {
        if gui_tree::name(r, child, image)?.as_deref() == Some(name) {
            return gui_tree::container(r, child, image);
        }
    }
    Ok(None)
}
pub(super) fn text(r: &Reader<'_>, base: u64) -> Result<String> {
    r.string_bounded(base + ui(r)?.text, 128)
}
pub(super) fn frame(r: &Reader<'_>, icon: u64) -> Result<i32> {
    let u = ui(r)?;
    r.i32(r.u64(icon + u.icon_sprite)? + u.sprite_frame)
}
pub(super) fn entry_frame(r: &Reader<'_>, image: u64, item: u64) -> Result<Option<u64>> {
    let u = ui(r)?;
    let entry = r.u64(item + u.item_container)?;
    let base = if r.u64(entry)? == image + u.container_vtable {
        entry + u.container_base
    } else {
        entry
    };
    gui_tree::container(r, base, image)
}

/// Whether the Air Forces Overview list is shown.
pub(super) fn overview_open(r: &Reader<'_>, image: u64) -> Result<bool> {
    let u = ui(r)?;
    let root = view_root(r, image, u.overview, u.overview_root)?;
    if !gui_tree::visible(r, root)? {
        return Ok(false);
    }
    match named_child(r, image, root, "air_units")? {
        Some(units) => gui_tree::visible(r, units),
        None => Ok(false),
    }
}
#[derive(Clone, Debug, PartialEq)]
pub(super) struct Wing {
    pub name: String,
    pub base: String,
    pub region: String,
    pub missions: Vec<i32>,
    pub rect: Option<Rect>,
}
pub(super) fn wings(r: &Reader<'_>, image: u64) -> Result<Vec<Wing>> {
    let u = ui(r)?;
    let root = view_root(r, image, u.overview, u.overview_root)?;
    let units = named_child(r, image, root, "air_units")?
        .ok_or_else(|| Error::new("gui_target_unavailable", "Air wing list not found"))?;
    let grid = gui_tree::child(r, image, units, "item_grid")?
        .ok_or_else(|| Error::new("gui_target_unavailable", "Air wing list not found"))?;
    let (_, data, count) = r.array_header(grid + u.grid_items)?;
    let mut out = Vec::new();
    for index in 0..count.min(512) as u64 {
        let Some(row) = entry_frame(r, image, r.u64(data + index * 8)?)? else {
            continue;
        };
        let mut wing = Wing {
            name: String::new(),
            base: String::new(),
            region: String::new(),
            missions: Vec::new(),
            rect: None,
        };
        for child in gui_tree::children(r, row)? {
            match gui_tree::name(r, child, image)?.as_deref() {
                Some("unit_name") => wing.name = text(r, child)?,
                Some("base") => wing.base = text(r, child)?,
                Some("assigned_region") => wing.region = text(r, child)?,
                Some("btn_bg") => wing.rect = gui_tree::rect_unclipped(r, image, child, GAME).ok(),
                Some(n) if n.starts_with("mission_icon_") && gui_tree::shown(r, child)? => {
                    wing.missions.push(frame(r, child)?)
                }
                _ => {}
            }
        }
        out.push(wing);
    }
    Ok(out)
}
/// The air selection view's root window.
pub(super) fn selection_root(r: &Reader<'_>, image: u64) -> Result<u64> {
    let u = ui(r)?;
    view_root(r, image, u.selection, u.selection_root)
}
/// Whether wings are selected (the air selection toolbar is shown).
pub(super) fn selection_open(r: &Reader<'_>, image: u64) -> Result<bool> {
    let u = ui(r)?;
    gui_tree::visible(r, view_root(r, image, u.selection, u.selection_root)?)
}
pub(super) fn toolbar_button(r: &Reader<'_>, image: u64, name: &str) -> Result<Rect> {
    let u = ui(r)?;
    let root = view_root(r, image, u.selection, u.selection_root)?;
    let bar = named_child(r, image, root, "air_wing_toolbar")?
        .ok_or_else(|| Error::new("gui_target_unavailable", "Air toolbar not found"))?;
    let button = gui_tree::child(r, image, bar, name)?
        .ok_or_else(|| Error::new("gui_target_unavailable", "Air toolbar button not found"))?;
    gui_tree::rect(r, image, button, GAME)
}
/// (icon frame, rectangle, enabled, toggled on) of the mission buttons.
pub(super) fn mission_buttons(r: &Reader<'_>, image: u64) -> Result<Vec<(i32, Rect, bool, bool)>> {
    let u = ui(r)?;
    let root = view_root(r, image, u.selection, u.selection_root)?;
    let bar = named_child(r, image, root, "mission_toolbar")?
        .ok_or_else(|| Error::new("gui_target_unavailable", "Mission toolbar not found"))?;
    let grid = gui_tree::child(r, image, bar, "missions_grid")?
        .ok_or_else(|| Error::new("gui_target_unavailable", "Mission toolbar not found"))?;
    let (_, data, count) = r.array_header(grid + u.grid_items)?;
    let mut out = Vec::new();
    for index in 0..count.min(32) as u64 {
        let Some(entry) = entry_frame(r, image, r.u64(data + index * 8)?)? else {
            continue;
        };
        let (Some(button), Some(icon)) = (
            gui_tree::child(r, image, entry, "btn")?,
            gui_tree::child(r, image, entry, "icon")?,
        ) else {
            continue;
        };
        let state = gui_tree::button_state(r, button)?;
        out.push((
            frame(r, icon)?,
            gui_tree::rect(r, image, button, GAME)?,
            state != 3,
            state == 2,
        ));
    }
    Ok(out)
}
pub(super) fn deselect(process: &dyn Process, keys: &dyn GameInput) -> Result<()> {
    if read(process, selection_open)? {
        let rect = settled(process, "air toolbar", |r, i| {
            toolbar_button(r, i, "btn_close")
        })?;
        let (x, y) = rect.center();
        keys.click(GAME, x, y)?;
        wait(process, "air wings deselected", |r, i| {
            Ok(!selection_open(r, i)?)
        })?;
    }
    Ok(())
}
pub(super) fn open_overview(process: &dyn Process, keys: &dyn GameInput) -> Result<()> {
    clear_notices(process, keys)?;
    deselect(process, keys)?;
    if !read(process, overview_open)? {
        keys.press(Key::letter('l')?)?;
        wait(process, "air overview open", overview_open)?;
        // A row click right after L was lost now and then on 1.19.4.1 (31 October 2026: the
        // selection never appeared, the same call succeeded later); let the list settle first.
        std::thread::sleep(std::time::Duration::from_millis(400));
    }
    Ok(())
}
fn wing_json(w: &Wing) -> Value {
    json!({"wing":w.name,"air_base":w.base,"region":(!w.region.is_empty()).then_some(&w.region),
        "missions":w.missions.iter().map(|f| mission_name(*f).unwrap_or("other")).collect::<Vec<_>>()})
}

/// Own air wings with base, region and missions (opens the air overview).
pub(super) fn list(process: &dyn Process, keys: &dyn GameInput) -> Result<Value> {
    open_overview(process, keys)?;
    let found = settled(process, "air wings", wings)?;
    Ok(
        json!({"wings":found.iter().map(wing_json).collect::<Vec<_>>(),
        "missions":MISSIONS.iter().map(|(n, _)| *n).collect::<Vec<_>>()}),
    )
}
/// Select the named wing through its row in the Air Forces Overview (opens it, scrolls the row into
/// view, one click); returns the row as it was before the click.
pub(super) fn select_wing(
    process: &dyn Process,
    keys: &dyn GameInput,
    name: &str,
) -> Result<Option<Wing>> {
    open_overview(process, keys)?;
    let find = |r: &Reader<'_>, i: u64| -> Result<Option<Rect>> {
        Ok(wings(r, i)?
            .into_iter()
            .find(|w| w.name == name)
            .and_then(|w| w.rect))
    };
    settled(process, "air wing row", find)?
        .ok_or_else(|| Error::new("invalid_target", "No own air wing with this name"))?;
    // One glide brings the row into the list band (only while the overview is open).
    let outcome = super::glide::glide(
        process,
        keys,
        &super::glide::Glide {
            at: (260.0, 650.0),
            pixels: 240,
            band: (f64::from(ROWS.0), f64::from(ROWS.1)),
            search: 0,
        },
        || {
            if read(process, overview_open)? {
                Ok(())
            } else {
                Err(Error::new("input_readback_failed", "Air overview closed"))
            }
        },
        |r, i| Ok(find(r, i)?.map(|x| super::glide::span_y(&x))),
    )?;
    if outcome == super::glide::Glided::Unknown {
        return Err(Error::new(
            "gui_target_unavailable",
            "Air wing left the list",
        ));
    }
    let rect = settled(process, "air wing row", |r, i| {
        find(r, i)?
            .filter(|r| r.y >= ROWS.0 && r.y + r.h <= ROWS.1)
            .ok_or_else(|| Error::new("gui_target_unavailable", "Air wing row not in view"))
    })?;
    let before = read(process, wings)?.into_iter().find(|w| w.name == name);
    let (x, y) = rect.center();
    keys.click(GAME, x.min(330.0), y)?;
    wait(process, "air wing selected", selection_open)?;
    std::thread::sleep(std::time::Duration::from_millis(250));
    Ok(before)
}

/// Screen point for the region click: one of `state`'s provinces that lie in `region` (all of
/// its provinces when the region is unknown), nearest their mean, away from the air base icons
/// and the wings' map markers, which take a right click for themselves (live 8 Nov 2026: the
/// mean of Tigray's Danakil provinces sat on the Eritrean base and the order did nothing).
fn region_point(
    process: &dyn Process,
    dir: Option<&std::path::Path>,
    state: i32,
    region: Option<i32>,
) -> Option<(f64, f64)> {
    let dir = dir?;
    let positions = super::mapdata::positions(dir).ok()?;
    let in_region = super::mapdata::province_regions(dir).ok();
    let world: Vec<(f64, f64)> = super::mapdata::state_provinces(dir, state)
        .ok()?
        .into_iter()
        .filter(|p| match (region, &in_region) {
            (Some(r), Some(map)) => map.get(p) == Some(&r),
            _ => true,
        })
        .filter_map(|p| positions.get(&p).copied())
        .collect();
    if world.is_empty() {
        return None;
    }
    read(process, |r, i| {
        let icons = super::airui::base_icons(r, i).unwrap_or_default();
        let mut pts = Vec::new();
        for (x, z) in &world {
            if let Some(p) = super::screenmap::to_screen(r, i, *x, *z)? {
                pts.push(p);
            }
        }
        if pts.is_empty() {
            return Ok(None);
        }
        let n = pts.len() as f64;
        let mean = pts
            .iter()
            .fold((0.0, 0.0), |a, p| (a.0 + p.0 / n, a.1 + p.1 / n));
        let clear = |p: &(f64, f64)| {
            icons.iter().all(|b| {
                let (bx, by) = b.center();
                // the base circle and the wing markers stacked below it (seen live: three
                // wings' markers reach about 90 px under the circle)
                (p.0 - bx).abs() > 95.0 || !(-45.0..140.0).contains(&(p.1 - by))
            })
        };
        pts.sort_by(|a, b| {
            let d = |p: &(f64, f64)| (p.0 - mean.0).powi(2) + (p.1 - mean.1).powi(2);
            d(a).total_cmp(&d(b))
        });
        Ok(pts.into_iter().find(clear))
    })
    .ok()
    .flatten()
}
/// Give wing `name` mission `mission` over the region containing `state`.
pub(super) fn assign(
    process: &dyn Process,
    keys: &dyn GameInput,
    name: &str,
    mission: &str,
    state: i32,
) -> Result<()> {
    let wanted = MISSIONS
        .iter()
        .find(|(n, _)| *n == mission)
        .map(|(_, f)| *f)
        .ok_or_else(|| Error::new("invalid_arguments", "Unknown mission"))?;
    // Already flying this mission over the state's region: nothing to click (a repeated order
    // used to wait out the whole readback for a change that could not come).
    let dir = process
        .identity()
        .and_then(|i| super::mapdata::game_dir(&i.exe).ok());
    let region = dir
        .as_ref()
        .and_then(|dir| super::mapdata::state_regions(dir).ok())
        .and_then(|m| m.get(&state).map(|v| v.0));
    if let Some(region) = region {
        let bit = 1 << (wanted - 1);
        if read(process, |r, i| super::airact::own_wing(r, i, name))?
            .is_some_and(|w| w.mission_mask & bit != 0 && w.region_id == region)
        {
            return Ok(());
        }
    }
    select_wing(process, keys, name)?;
    let buttons = settled(process, "mission buttons", mission_buttons)?;
    let (_, button, enabled, _) = buttons
        .into_iter()
        .find(|(f, _, _, _)| *f == wanted)
        .ok_or_else(|| Error::new("invalid_target", "Mission not offered for this wing"))?;
    if !enabled {
        let _ = deselect(process, keys);
        return Err(Error::new(
            "invalid_target",
            "The game disables this mission for this wing",
        ));
    }
    // A mission button flips the wing's mission bit at once (a held wing keeps its last bit, and the
    // toolbar then shows the button off although the bit is set: clicking it again would clear the bit
    // and the right click would assign a region with no mission, seen live on JG 136 and KG 1,
    // 26 October 2026). So the bit in memory decides whether to click, not the button's look.
    let bit = 1 << (wanted - 1);
    let has_bit = |r: &Reader<'_>, i: u64| -> Result<bool> {
        Ok(super::airact::own_wing(r, i, name)?.is_some_and(|w| w.mission_mask & bit != 0))
    };
    let (x, y) = button.center();
    if !read(process, has_bit)? {
        keys.click(GAME, x, y)?;
        if let Err(error) = wait(process, "mission chosen", has_bit) {
            let _ = deselect(process, keys);
            return Err(error);
        }
    }
    // The state's centre where the camera already is (aiming height); the Find view
    // (which centres it and waits for the camera to rest) otherwise.
    // Strategic air regions are large: a click on the state's projected centre lands in the
    // right region from far higher than unit orders allow.
    // Project only once the camera rests (air mode may have just zoomed it in).
    super::screenmap::wait_camera_still(process, 700);
    let high_ok = read(process, super::screenmap::camera_height).is_ok_and(|h| h <= 1400.0);
    let target = high_ok
        .then(|| region_point(process, dir.as_deref(), state, region))
        .flatten()
        .filter(|(x, y)| (520.0..1500.0).contains(x) && (160.0..800.0).contains(y));
    let target = match target {
        Some(p) => p,
        None => {
            find_state(process, keys, state)?;
            region_point(process, dir.as_deref(), state, region)
                .filter(|(x, y)| (420.0..1700.0).contains(x) && (120.0..900.0).contains(y))
                .unwrap_or((960.0, 540.0))
        }
    };
    if !read(process, has_bit)? {
        let _ = deselect(process, keys);
        return Err(Error::new(
            "input_readback_failed",
            "The mission choice was lost before the region click",
        ));
    }
    let area = |w: &super::airread::Wing| (w.mission_mask, w.region_id);
    let mem_before = read(process, |r, i| super::airact::own_wing(r, i, name))?.map(|w| area(&w));
    // A camera still gliding would carry the click elsewhere: wait until it rests (at once
    // when it already does; the Find view waited too).
    super::screenmap::wait_camera_still(process, 600);
    if std::env::var_os("HOI4_TRACE").is_some() {
        let bases = read(process, |r, i| {
            Ok(super::airui::base_icons(r, i)
                .unwrap_or_default()
                .iter()
                .map(|b| b.center())
                .collect::<Vec<_>>())
        })
        .unwrap_or_default();
        eprintln!("[trace] air region click at {target:?} (state {state}, region {region:?}, bases {bases:?})");
    }
    keys.right_click(GAME, target.0, target.1)?;
    let result = wait(process, "air mission assigned", |r, i| {
        Ok(super::airact::own_wing(r, i, name)?.is_some_and(|w| {
            w.mission_mask & bit != 0
                && w.region_id > 0
                && mem_before.is_none_or(|b| b != area(&w))
        }))
    })
    .map_err(|_| {
        Error::new(
            "input_readback_failed",
            "The wing's mission or region did not change (the region is out of the wing's range, or the wing already has it)",
        )
    });
    let _ = deselect(process, keys);
    result
}
