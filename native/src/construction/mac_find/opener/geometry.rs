//! Exact requested Standard geometry; current caches locate an ordinary click,
//! never establish atomic frame completion or delivery.
use super::changed;
use crate::{
    construction::{mac_panel::routing, mac_ui, pointer},
    error::Result,
    gui::macos::canvas_from_reader,
    gui_tree::{self, Rect},
    memory::Reader,
};
const GAME: (u32, u32) = (1920, 1080);
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Target {
    edges: Vec<(u64, u64)>,
    button: u64,
    graphics: u64,
    rect: Rect,
}
fn edge(r: &Reader<'_>, at: u64, want: u64, edges: &mut Vec<(u64, u64)>) -> Result<()> {
    if r.u64(at)? != want {
        return Err(changed("Original Find button geometry edge changed"));
    }
    edges.push((at, want));
    Ok(())
}
impl Target {
    pub(super) fn original_edges(&self, r: &Reader<'_>) -> Result<()> {
        for &(at, want) in &self.edges {
            if r.u64(at)? != want {
                return Err(changed("Original Find geometry replaced before payload"));
            }
        }
        Ok(())
    }
    pub(super) fn point(&self) -> (f64, f64) {
        (
            f64::from(self.rect.x + self.rect.w / 2),
            f64::from(self.rect.y + self.rect.h / 2),
        )
    }
}
pub(super) fn observe(
    r: &Reader<'_>,
    base: u64,
    gui: u64,
    gfx: u64,
    root: u64,
    button: u64,
    route: bool,
) -> Result<Target> {
    let mut edges = Vec::new();
    edge(r, button, base + 0x36b9738, &mut edges)?;
    edge(r, button + 0x58, gui, &mut edges)?;
    let desc = r.u64(button + 0x50)?;
    pointer(desc)?;
    edge(r, button + 0x50, desc, &mut edges)?;
    // Exact Standard descriptor, already named by the complete parent scan.
    edge(r, desc, base + 0x36b9f98, &mut edges)?;
    edge(r, button + 0xd8, gfx, &mut edges)?;
    let sprite = r.u64(button + 0xe0)?;
    pointer(sprite)?;
    edge(r, button + 0xe0, sprite, &mut edges)?;
    let primary = r.u64(sprite)?;
    let resource_primary = match primary {
        p if p == base + 0x36cc528 => base + 0x36cc880,
        p if p == base + 0x36be530 => base + 0x36be828,
        _ => return Err(changed("Exact Find Standard sprite required")),
    };
    edge(r, sprite, primary, &mut edges)?;
    edge(r, sprite + 0x138, button, &mut edges)?;
    let parent = r.u64(button + 0x180)?;
    edge(r, button + 0x180, parent, &mut edges)?;
    // All three attachment identities are required even in the null variant.
    // Compare own fields before following a possible nonnull parent payload.
    edge(r, root + 0x1358, parent, &mut edges)?;
    edge(r, sprite + 0x108, parent, &mut edges)?;
    if parent != 0 {
        pointer(parent)?;
        edge(r, parent, base + 0x36bea88, &mut edges)?;
        if r.byte(parent + 0x128)? == 0 {
            return Err(changed("Find graphical parent is inactive"));
        }
    }
    let resource = r.u64(sprite + 8)?;
    pointer(resource)?;
    edge(r, sprite + 8, resource, &mut edges)?;
    edge(r, resource, resource_primary, &mut edges)?;
    let canvas = canvas_from_reader(r, base, gui)?;
    if canvas.object != gfx
        || canvas.physical != [1920, 1080]
        || canvas.logical != canvas.physical
        || canvas.scale != 1.0
    {
        return Err(changed("Find click needs the qualified1920x1080 canvas"));
    }
    if r.byte(button + 0x65)? & 8 == 0 || ![0, 2].contains(&r.i32(button + 0x9c)?) {
        return Err(changed("Find button is hidden or not ready"));
    }
    let cc = r.byte(button + 0xd4)?;
    if cc & 8 == 0 || cc & 0x40 != 0 || r.byte(button + 0x1d9)? != 0 || r.u64(button + 0xc8)? != 0 {
        return Err(changed(
            "Find button is disabled or has an unsupported handler",
        ));
    }
    let rect = gui_tree::rect_unclipped(r, base, button, GAME)?;
    if rect.x < 0 || rect.y < 0 || rect.x + rect.w > 1920 || rect.y + rect.h > 1080 {
        return Err(changed(
            "Find button must be fully inside the qualified canvas",
        ));
    }
    if route {
        routing::observe(r, base, gui, gfx, button, rect)?.require_find_pre_down(true)?;
    }
    // These are locating/correlation facts; no permission or delivery boolean.
    let _ = mac_ui::window(r, base, root, gui)?;
    Ok(Target {
        edges,
        button,
        graphics: gfx,
        rect,
    })
}
