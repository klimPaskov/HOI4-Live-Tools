//! Exact-file MapModes requested ordinary mouse target and callback.
//! Ordinary cache correlation only; no atomic generation/delivery guarantee.
use crate::{
    construction::{
        mac_panel::{cancel::hidden_lifetimes, Window},
        mac_ui, members, pointer,
    },
    error::{Error, Result},
    game,
    memory::{Reader, StringAbi},
};
#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::construction::mac_find) struct Binding {
    edges: Vec<(u64, u64)>,
    paths: Vec<(u64, Vec<u64>)>,
    mapmodes: u64,
    root: u64,
    button: u64,
    geometry: super::geometry::Target,
    popup_windows: Vec<Window>,
    receiver: MouseReceiver,
    callback_edges: Vec<(u64, u64)>,
}
fn changed(s: &str) -> Error {
    Error::new("construction_find_open_changed", s)
}
fn edge(r: &Reader<'_>, at: u64, edges: &mut Vec<(u64, u64)>) -> Result<u64> {
    let v = r.u64(at)?;
    edges.push((at, v));
    Ok(v)
}
fn own(r: &Reader<'_>, at: u64, want: u64, edges: &mut Vec<(u64, u64)>) -> Result<()> {
    if edge(r, at, edges)? != want {
        return Err(changed("Original Find opener owner/alias differs"));
    }
    Ok(())
}
fn window(r: &Reader<'_>, base: u64, p: u64, gui: u64, shown: bool) -> Result<()> {
    if mac_ui::window(r, base, p, gui)? != p
        || (r.byte(p + 0x95)? & 8 != 0) != shown
        || r.i32(p + 0x12b8)? != 0
        || r.i32(p + 0x1270)? != 0
    {
        return Err(changed(
            "Exact own Find opener window must be locally idle in its required visibility",
        ));
    }
    Ok(())
}
fn hidden(r: &Reader<'_>, base: u64, at: u64, gui: u64, edges: &mut Vec<(u64, u64)>) -> Result<()> {
    let p = edge(r, at, edges)?;
    if p != 0 {
        own(r, p, base + 0x34ac0b0, edges)?;
        own(r, p + 0x88, gui, edges)?;
        window(r, base, p, gui, false)?;
    }
    Ok(())
}
impl Binding {
    pub(super) fn original_edges(&self, r: &Reader<'_>) -> Result<()> {
        for &(at, v) in &self.edges {
            if r.u64(at)? != v {
                return Err(changed("Original Find click edge replaced before payload"));
            }
        }
        for (parent, children) in &self.paths {
            if members(r, parent + 0xd30, 4096)? != *children {
                return Err(changed(
                    "Original Find click child membership replaced before payload",
                ));
            }
        }
        self.geometry.original_edges(r)?;
        for &(at, v) in &self.callback_edges {
            if r.u64(at)? != v {
                return Err(changed("Original Find callback changed before payload"));
            }
        }
        Ok(())
    }
    pub(super) fn point(&self) -> (f64, f64) {
        self.geometry.point()
    }
    pub(super) fn same_target(&self, now: &Self) -> bool {
        self == now
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
enum MouseReceiver {
    Direct,
    Hotkeys {
        character_mode: u8,
        idle_stage: Option<i32>,
    },
}
fn mouse_receiver(
    r: &Reader<'_>,
    base: u64,
    app: u64,
    edges: &mut Vec<(u64, u64)>,
) -> Result<MouseReceiver> {
    let receiver = edge(r, base + 0x35d8dd0, edges)?;
    if receiver == 0 {
        return Ok(MouseReceiver::Direct);
    }
    crate::construction::pointer(receiver)?;
    own(r, receiver, base + 0x34a7db8, edges)?;
    own(r, base + 0x3639ad8, app, edges)?;
    own(r, base + 0x34a7db8 + 0x20, base + 0x26ebf10, edges)?;
    let character_mode = r.byte(base + 0x35d8dd8)?;
    let idle_stage = match character_mode {
        0 => None,
        1 => {
            if r.i32(receiver + 0x64)? != 0 {
                return Err(changed("Hotkey character capture must be idle"));
            }
            Some(0)
        }
        _ => return Err(changed("Unknown Hotkey character mode")),
    };
    // Engine kind0 is forwarded unchanged at26ebf4e->26ec05a. This is
    // mouse-only proof, never admission for printable character delivery.
    Ok(MouseReceiver::Hotkeys {
        character_mode,
        idle_stage,
    })
}
// Keep the requested path narrow even if the shared named scan later visits
// nested or unrelated containers. Only this direct root has a visibility proof.
pub(super) fn require_direct_member(
    root: u64,
    button: u64,
    paths: &[(u64, Vec<u64>)],
) -> Result<()> {
    let mut roots = paths.iter().filter(|(parent, _)| *parent == root);
    let Some((_, children)) = roots.next() else {
        return Err(changed(
            "Find button must retain direct MapModes root membership",
        ));
    };
    if roots.next().is_some() || children.iter().filter(|&&p| p == button).count() != 1 {
        return Err(changed(
            "Find button must be one direct MapModes root member",
        ));
    }
    Ok(())
}
pub(super) fn observe(
    r: &Reader<'_>,
    base: u64,
    owners: [u64; 5],
    old: Option<&Binding>,
    route: bool,
) -> Result<Binding> {
    let [gs, idler, gui, manager, find] = owners;
    if r.string_abi() != StringAbi::LibCxx {
        return Err(Error::new(
            "unsupported_platform",
            "Mac click layout required",
        ));
    }
    if base < 0x10000
        || base % 4096 != 0
        || base
            .checked_add(0x3639ae0)
            .is_none_or(|p| p > 0x7fff_ffff_0000)
    {
        return Err(changed("Bounded aligned image base required"));
    }
    if let Some(o) = old {
        o.original_edges(r)?;
    }
    let mut edges = Vec::new();
    own(r, base + game::MACOS.gs_global, gs, &mut edges)?;
    own(r, base + game::MACOS.idler_global, idler, &mut edges)?;
    own(r, gs, base + game::MACOS.gs_vtable, &mut edges)?;
    own(r, idler, base + 0x3308f90, &mut edges)?;
    own(r, gui, base + 0x34974d0, &mut edges)?;
    own(r, base + 0x35196c0, idler, &mut edges)?;
    own(r, base + 0x35dbf60, 0, &mut edges)?;
    own(r, idler + 0x4c0, gui, &mut edges)?;
    own(r, base + 0x35dc058, gui, &mut edges)?;
    own(r, idler + 0x670, manager, &mut edges)?;
    own(r, manager + 0x180, idler, &mut edges)?;
    own(r, manager + 0x190, find, &mut edges)?;
    if r.i32(manager + 0x3c0)? != 0 {
        return Err(changed(
            "Find click requires inactive ordinary construction selection",
        ));
    }
    if r.u64(base + 0x3308f90 + 0x20)? != base + 0x776fe0
        || r.byte(idler + 0x7b5)? != 1
        || r.byte(base + 0x3519990)? != 0
    {
        return Err(changed("Ordinary owned main-frame receiver required"));
    }
    own(r, gui + 0x138, 0, &mut edges)?;
    own(r, gui + 0x210, 0, &mut edges)?;
    let popup_windows = hidden_lifetimes(r, base, gui, &mut edges)?;
    let gfx = edge(r, gui + 0x98, &mut edges)?;
    own(r, gfx, base + 0x33dafa8, &mut edges)?;
    let app = edge(r, idler + 0x4f8, &mut edges)?;
    pointer(app)?;
    own(r, app + 0x300, gfx, &mut edges)?;
    own(r, app + 0x308, gui, &mut edges)?;
    own(r, app + 0x38, idler, &mut edges)?;
    let receiver = mouse_receiver(r, base, app, &mut edges)?;
    // Callback21459d0's precise blocked-manager path20ed8d0.
    if r.byte(gs + 0x9b9)? != 1 || r.byte(idler + 0x898)? != 0 {
        return Err(changed("Find callback is currently blocked"));
    }
    let settings = edge(r, idler + 0x768, &mut edges)?;
    if settings != 0 {
        own(r, settings, base + 0x345b9e0, &mut edges)?;
        own(r, settings + 0x28, idler, &mut edges)?;
        hidden(r, base, settings + 0x7e0, gui, &mut edges)?;
    }
    let menu = edge(r, manager + 0x218, &mut edges)?;
    own(r, menu, base + 0x3461428, &mut edges)?;
    own(r, menu + 0x2668, idler, &mut edges)?;
    own(r, menu + 0x2670, manager, &mut edges)?;
    hidden(r, base, menu + 0x2678, gui, &mut edges)?;
    let career = edge(r, menu + 0x2690, &mut edges)?;
    if career != 0 {
        own(r, career, base + 0x33f8b50, &mut edges)?;
        own(r, career + 0x1880, idler, &mut edges)?;
        own(r, career + 0x1890, menu, &mut edges)?;
        hidden(r, base, career + 0x1888, gui, &mut edges)?;
    }
    let lobby = edge(r, manager + 0x248, &mut edges)?;
    if lobby != 0 {
        own(r, lobby, base + 0x346ff10, &mut edges)?;
        own(r, lobby + 0x48, idler, &mut edges)?;
        hidden(r, base, lobby + 0x38, gui, &mut edges)?;
        hidden(r, base, lobby + 0x40, gui, &mut edges)?;
    }
    // The ordinary callback does not materialize a null Find root.
    own(r, find, base + 0x3457c38, &mut edges)?;
    own(r, find + 0xc30, idler, &mut edges)?;
    let hidden_find = edge(r, find + 0xc38, &mut edges)?;
    if hidden_find == 0 {
        return Err(changed("Ordinary click needs a materialized hidden Find"));
    }
    own(r, hidden_find, base + 0x34ac0b0, &mut edges)?;
    own(r, hidden_find + 0x88, gui, &mut edges)?;
    window(r, base, hidden_find, gui, false)?;
    let map = edge(r, manager + 0x228, &mut edges)?;
    own(r, map, base + 0x3466b88, &mut edges)?;
    own(r, map + 0x30, manager, &mut edges)?;
    own(r, map + 0x22a8, manager, &mut edges)?;
    own(r, map + 0x22b0, idler, &mut edges)?;
    let root = edge(r, map + 0x23d0, &mut edges)?;
    own(r, root, base + 0x34ac0b0, &mut edges)?;
    own(r, root + 0x88, gui, &mut edges)?;
    window(r, base, root, gui, true)?;
    let mut paths = Vec::new();
    let button = mac_ui::named(r, base, root, "open_find_view_button", &mut paths)?
        .ok_or_else(|| changed("Unique current named Find opener required"))?;
    require_direct_member(root, button, &paths)?;
    if old.is_some_and(|b| b.button != button) {
        return Err(changed("Original Find click button replaced"));
    }
    let geometry = super::geometry::observe(r, base, gui, gfx, root, button, route)?;
    let mut callback_edges = Vec::new();
    own(r, button, base + 0x34aafa0, &mut callback_edges)?;
    own(r, button + 0x58, gui, &mut callback_edges)?;
    own(r, button + 0x70, base + 0x34ab360, &mut callback_edges)?;
    own(r, button + 0x90, 0, &mut callback_edges)?;
    own(r, button + 0xc0, 0, &mut callback_edges)?;
    if r.i32(button + 0x88)? != 1 || r.byte(button + 0x8c)? != 0 {
        return Err(changed("Exactly one active Find callback required"));
    }
    let node = edge(r, button + 0x78, &mut callback_edges)?;
    pointer(node)?;
    own(r, button + 0x80, node, &mut callback_edges)?;
    own(r, node + 8, 0, &mut callback_edges)?;
    own(r, node + 0x10, 0, &mut callback_edges)?;
    if r.byte(node + 0x18)? != 0 {
        return Err(changed("Removed click observer refused"));
    }
    let glue = map + 0x1350;
    own(r, node, glue, &mut callback_edges)?;
    own(r, glue, base + 0x3467198, &mut callback_edges)?;
    for i in 2..=20 {
        own(r, glue + i * 0x30, 0, &mut callback_edges)?;
    }
    let function = edge(r, glue + 0x30, &mut callback_edges)?;
    own(r, function, base + 0x3467228, &mut callback_edges)?;
    own(r, function + 8, base + 0x21459d0, &mut callback_edges)?;
    own(r, function + 0x10, 0, &mut callback_edges)?;
    own(r, function + 0x18, map, &mut callback_edges)?;
    // Own named membership remains immutable; incidental route/source lists do not.
    for (parent, _) in &paths {
        edge(r, parent + 0xd30, &mut edges)?;
    }
    let now = Binding {
        edges,
        paths,
        mapmodes: map,
        root,
        button,
        geometry,
        popup_windows,
        receiver,
        callback_edges,
    };
    if old.is_some_and(|b| !b.same_target(&now)) {
        return Err(changed("Original Find click target changed"));
    }
    Ok(now)
}
