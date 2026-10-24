//! Original-scope ordinary Escape admission, not mouse routing or delivery.
//! The panel may close; only the retained canonical selection reader certifies
//! that this one attempt cleared selection without changing the complete queue.
use super::{exact, local, named, pointer, shortcut, Window};
use crate::{
    construction::{mac_ui::SelectionAnchor, members},
    error::{Error, Result},
    game::{self, ClockContext},
    memory::{Reader, StringAbi},
    platform::{Identity, Process},
    research_ui::Witnesses,
};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};

#[derive(Clone, Debug, PartialEq, Eq)]
enum OuterReceiver {
    Direct,
    Hotkeys {
        character_mode: u8,
        idle_stage: Option<i32>,
    },
}
#[derive(Clone, Debug, PartialEq, Eq)]
struct Binding {
    // Ordered owning edges, compared before any replacement payload.
    edges: Vec<(u64, u64)>,
    callback_edges: Vec<(u64, u64)>,
    panel: Window,
    popup_windows: Vec<Window>,
    outer_receiver: OuterReceiver,
    paths: Vec<(u64, Vec<u64>)>,
    button: u64,
    escape_invalidation: Option<u8>,
    escape_target: shortcut::EscapeTarget,
}
impl Binding {
    fn require_original(&self, r: &Reader<'_>, base: u64) -> Result<()> {
        // Scope/owner aliases precede control metadata. The exact requested
        // target then precedes callback payload, without pinning a stale prefix.
        for &(at, value) in &self.edges {
            if r.u64(at)? != value {
                return Err(changed("Original cancellation edge changed before payload"));
            }
        }
        self.escape_target.require_original(r, base)?;
        for &(at, value) in &self.callback_edges {
            if r.u64(at)? != value {
                return Err(changed(
                    "Original cancellation callback changed before payload",
                ));
            }
        }
        Ok(())
    }
}
#[derive(Clone, Debug)]
pub(crate) struct CancelAnchor {
    identity: Identity,
    base: u64,
    context: ClockContext,
    selection: SelectionAnchor,
    binding: Binding,
}
fn changed(message: &str) -> Error {
    Error::new("construction_cancel_changed", message)
}
fn same_context(before: &ClockContext, now: &ClockContext) -> Result<()> {
    if before.anchors != now.anchors
        || before.player != now.player
        || before.playthrough_id != now.playthrough_id
        || before.difficulty != now.difficulty
        || before.historical_raw != now.historical_raw
        || now.date_raw < before.date_raw
    {
        return Err(changed("Original construction cancellation scope changed"));
    }
    Ok(())
}
fn edge(r: &Reader<'_>, at: u64, edges: &mut Vec<(u64, u64)>) -> Result<u64> {
    let value = r.u64(at)?;
    edges.push((at, value));
    Ok(value)
}
#[track_caller]
fn owned(r: &Reader<'_>, at: u64, value: u64, edges: &mut Vec<(u64, u64)>) -> Result<()> {
    if edge(r, at, edges)? != value {
        return Err(changed(&format!(
            "Own cancellation backlink differs; check={}",
            std::panic::Location::caller().line()
        )));
    }
    Ok(())
}
fn hidden(r: &Reader<'_>, base: u64, at: u64, gui: u64, edges: &mut Vec<(u64, u64)>) -> Result<()> {
    let value = edge(r, at, edges)?;
    if value != 0 {
        let window = local(r, base, value, gui)?;
        if window.flags & 8 != 0 || !window.idle() {
            return Err(changed(
                "Competing Find/menu window is visible or animating",
            ));
        }
    }
    Ok(())
}
fn callback(
    r: &Reader<'_>,
    base: u64,
    button: u64,
    view: u64,
    edges: &mut Vec<(u64, u64)>,
) -> Result<()> {
    exact(r, base, button + 0x70, 0x34ab360)?;
    owned(r, button + 0x90, 0, edges)?;
    owned(r, button + 0xc0, 0, edges)?;
    if r.byte(button + 0x8c)? != 0 || r.i32(button + 0x88)? != 1 {
        return Err(changed("Exactly one inactive-list close observer required"));
    }
    let node = edge(r, button + 0x78, edges)?;
    pointer(node)?;
    owned(r, button + 0x80, node, edges)?;
    owned(r, node + 8, 0, edges)?;
    owned(r, node + 0x10, 0, edges)?;
    if r.byte(node + 0x18)? != 0 {
        return Err(changed("Removed close observer refused"));
    }
    let glue = view + 0x40;
    owned(r, node, glue, edges)?;
    exact(r, base, glue, 0x34466a8)?;
    // The reviewed constructor has one virtual-member callback, not competing
    // notification bundles. All nineteen other std::function pointers absent.
    for index in 2..=20 {
        owned(r, glue + index * 0x30, 0, edges)?;
    }
    let function = edge(r, glue + 0x30, edges)?;
    exact(r, base, function, 0x3446738)?;
    owned(r, function + 8, 0x91, edges)?;
    owned(r, function + 0x10, 0, edges)?;
    owned(r, function + 0x18, view, edges)
}
// CGameGui owns these complete pointer arrays as lifetime collections; they
// are not the current keyboard route. Exact hidden plane-selection windows are
// retained here because their ordinary update returns alive without UI work.
// This narrowly admits the observed inert class; unknown classes still refuse.
pub(in crate::construction) fn hidden_lifetimes(
    r: &Reader<'_>,
    base: u64,
    gui: u64,
    edges: &mut Vec<(u64, u64)>,
) -> Result<Vec<Window>> {
    let mut entries = Vec::new();
    let mut seen = std::collections::BTreeSet::new();
    for at in [gui + 0x9e0, gui + 0x9f8] {
        let (header, data, count) = r.array_header(at)?;
        let capacity = i32::from_le_bytes(header[8..12].try_into().unwrap());
        if count > 64 || capacity > 64 || entries.len() + count > 64 {
            return Err(changed("Complete popup lifetime collection exceeds bound"));
        }
        if data != 0 {
            pointer(data)?;
        }
        owned(r, at, data, edges)?;
        owned(
            r,
            at + 8,
            u64::from_le_bytes(header[8..16].try_into().unwrap()),
            edges,
        )?;
        for index in 0..count {
            let object = edge(r, data + index as u64 * 8, edges)?;
            pointer(object)?;
            if !seen.insert(object) {
                return Err(changed("Duplicate popup lifetime membership refused"));
            }
            entries.push(object);
        }
    }
    let mut windows = Vec::new();
    let mut roots = std::collections::BTreeSet::new();
    for object in entries {
        // Exact current-build CPlaneTypeSelectionWindow primary; do not infer
        // payload shape from RTTI names or a compatible-looking first word.
        owned(r, object, base + 0x33ec9d0, edges)?;
        if r.byte(object + 0x438)? != 0 || r.byte(object + 0x439)? != 0 {
            return Err(changed("Popup lifetime is closing or timed"));
        }
        let root = edge(r, object + 0x430, edges)?;
        pointer(root)?;
        // Require the original primary form before any root payload.
        owned(r, root, base + 0x34ac0b0, edges)?;
        owned(r, root + 0x88, gui, edges)?;
        if !roots.insert(root) {
            return Err(changed("Popup lifetime roots are not unique"));
        }
        let window = local(r, base, root, gui)?;
        if window.flags & 8 != 0 || !window.idle() {
            return Err(changed("Popup lifetime window is visible or animating"));
        }
        windows.push(window);
    }
    Ok(windows)
}
// Escape-specific ordinary SDL route. A current HotkeyManager is an owned
// forwarding wrapper, not a second game receiver. Physical Escape remains
// engine kind1; insertText's shipped control filter also rejects U+001B.
// Character capture stages remain unsupported, as do unknown outer handlers.
fn escape_receiver(
    r: &Reader<'_>,
    base: u64,
    application: u64,
    edges: &mut Vec<(u64, u64)>,
) -> Result<OuterReceiver> {
    let receiver = edge(r, base + 0x35d8dd0, edges)?;
    if receiver == 0 {
        return Ok(OuterReceiver::Direct);
    }
    pointer(receiver)?;
    owned(r, receiver, base + 0x34a7db8, edges)?;
    // The wrapper's getter must resolve the original application BEFORE
    // current wrapper payload; its application+38 receiver is already retained.
    owned(r, base + 0x3639ad8, application, edges)?;
    owned(r, base + 0x34a7db8 + 0x20, base + 0x26ebf10, edges)?;
    let character_mode = r.byte(base + 0x35d8dd8)?;
    let idle_stage = match character_mode {
        0 => None,
        1 => {
            let stage = r.i32(receiver + 0x64)?;
            if stage != 0 {
                return Err(changed("Hotkey character capture is not idle"));
            }
            Some(stage)
        }
        _ => return Err(changed("Unsupported HotkeyManager character mode")),
    };
    Ok(OuterReceiver::Hotkeys {
        character_mode,
        idle_stage,
    })
}
fn image_base(base: u64) -> Result<()> {
    if base < 0x10000
        || base % 0x1000 != 0
        || base
            .checked_add(0x3639ae0)
            .is_none_or(|p| p > 0x7fff_ffff_0000)
    {
        return Err(changed("Bounded aligned image base required"));
    }
    Ok(())
}
fn from_reader(
    r: &Reader<'_>,
    base: u64,
    idler: u64,
    original: Option<&Binding>,
) -> Result<Binding> {
    if r.string_abi() != StringAbi::LibCxx {
        return Err(Error::new(
            "unsupported_platform",
            "Mac cancellation layout required",
        ));
    }
    image_base(base)?;
    pointer(idler)?;
    if let Some(before) = original {
        before.require_original(r, base)?;
    }
    exact(r, base, idler, 0x3308f90)?;
    // Exact own event receiver20 copies the event into idler+710. It does not
    // invoke the close observer or bypass the ordinary queue.
    if r.u64(base + 0x3308f90 + 0x20)? != base + 0x776fe0 {
        return Err(changed("Owned ordinary event receiver differs"));
    }
    // Main-frame routing reaches the ordinary GUI path only through this
    // byte gate; the alternate path is outside this narrow qualification.
    if r.byte(idler + 0x7b5)? != 1 || r.byte(base + 0x3519990)? != 0 {
        return Err(changed("Ordinary owned main-frame GUI dispatch required"));
    }
    let mut edges = Vec::new();
    let gui = edge(r, idler + 0x4c0, &mut edges)?;
    exact(r, base, gui, 0x34974d0)?;
    owned(r, base + 0x35dc058, gui, &mut edges)?;
    // No focused receiver is followed; lifetime windows use a separate typed guard.
    owned(r, gui + 0x138, 0, &mut edges)?;
    owned(r, gui + 0x210, 0, &mut edges)?;
    // The Escape prepass may set this existing singleton's +411 byte before
    // routing; forbid unbound allocation/alternate types, not ordinary invalidation.
    let escape_invalidation = match r.byte(base + 0x35dbfa8)? {
        0 => None,
        1 => {
            let singleton = edge(r, base + 0x35dbfa0, &mut edges)?;
            exact(r, base, singleton, 0x34aabd0)?;
            let invalidated = r.byte(singleton + 0x411)?;
            if invalidated > 1 {
                return Err(changed("Unsupported Escape GUI invalidation state"));
            }
            Some(invalidated)
        }
        _ => return Err(changed("Unsupported Escape GUI invalidation flag")),
    };
    let popup_windows = hidden_lifetimes(r, base, gui, &mut edges)?;
    let gfx = edge(r, gui + 0x98, &mut edges)?;
    exact(r, base, gfx, 0x33dafa8)?;
    let application = edge(r, idler + 0x4f8, &mut edges)?;
    pointer(application)?;
    // Proven resource/application path; no polymorphic first-word inference.
    owned(r, application + 0x300, gfx, &mut edges)?;
    owned(r, application + 0x308, gui, &mut edges)?;
    owned(r, application + 0x38, idler, &mut edges)?;
    let outer_receiver = escape_receiver(r, base, application, &mut edges)?;
    let pump = edge(r, base + 0x35d8c60, &mut edges)?;
    exact(r, base, pump, 0x34a6760)?;
    if r.byte(pump + 0x44)? != 0 {
        return Err(changed("SDL keyboard diversion is enabled"));
    }
    // Cocoa keyCode53 -> scancode41; SDL's producer reads this current
    // semantic keymap entry. Do not assume the named/disk Escape mapping.
    if r.i32(base + 0x3638d28)? != 0x1b
        || r.byte(base + 0x3638aab)? != 0
        || r.read(base + 0x3638880, 2)? != [0, 0]
    {
        return Err(changed(
            "Current unmodified released SDL Escape mapping required",
        ));
    }
    let manager = edge(r, idler + 0x670, &mut edges)?;
    pointer(manager)?;
    owned(r, manager + 0x180, idler, &mut edges)?;
    if r.i32(manager + 0x3a8)? != 17
        || r.i32(manager + 0x3c0)? != 1
        || r.byte(manager + 0x3b1)? != 0
        || r.byte(manager + 0x3b2)? != 0
        || r.i32(manager + 0xb4)? != 0
    {
        return Err(changed(
            "Already-open ordinary selection with unblocked empty-history manager required",
        ));
    }
    let view = edge(r, manager + 0x150, &mut edges)?;
    exact(r, base, view, 0x343aa90)?;
    owned(r, view + 0x428, idler, &mut edges)?;
    owned(r, view + 0x30, manager, &mut edges)?;
    let root = edge(r, view + 0x430, &mut edges)?;
    let panel = local(r, base, root, gui)?;
    if !panel.visible() || !panel.idle() {
        return Err(changed(
            "Construction panel must already be visible and idle",
        ));
    }
    let find = edge(r, manager + 0x190, &mut edges)?;
    exact(r, base, find, 0x3457c38)?;
    owned(r, find + 0xc30, idler, &mut edges)?;
    hidden(r, base, find + 0xc38, gui, &mut edges)?;
    let menu = edge(r, manager + 0x218, &mut edges)?;
    exact(r, base, menu, 0x3461428)?;
    owned(r, menu + 0x2668, idler, &mut edges)?;
    owned(r, menu + 0x2670, manager, &mut edges)?;
    hidden(r, base, menu + 0x2678, gui, &mut edges)?;
    let career = edge(r, menu + 0x2690, &mut edges)?;
    if career != 0 {
        exact(r, base, career, 0x33f8b50)?;
        owned(r, career + 0x1880, idler, &mut edges)?;
        owned(r, career + 0x1890, menu, &mut edges)?;
        hidden(r, base, career + 0x1888, gui, &mut edges)?;
    }
    let children = members(r, panel.object + 0xd30, 4096)?;
    if original.is_some_and(|b| b.paths.first() != Some(&(panel.object, children.clone()))) {
        return Err(changed(
            "Original close-button membership changed before names/payload",
        ));
    }
    let mut paths = Vec::new();
    let button = named(r, base, panel.object, "close_button", &mut paths)?
        .ok_or_else(|| changed("Unique current construction close_button required"))?;
    if original.is_some_and(|b| b.button != button) {
        return Err(changed("Original close button replaced"));
    }
    exact(r, base, button, 0x34aafa0)?;
    let mut callback_edges = Vec::new();
    owned(r, button + 0x58, gui, &mut callback_edges)?;
    callback(r, base, button, view, &mut callback_edges)?;
    let escape_target = shortcut::observe_escape_current_prefix(
        r,
        base,
        gui,
        button,
        panel.object,
        original.map(|b| &b.escape_target),
    )?;
    let binding = Binding {
        edges,
        callback_edges,
        panel,
        popup_windows,
        outer_receiver,
        paths,
        button,
        escape_invalidation,
        escape_target,
    };
    if original.is_some_and(|b| b != &binding) {
        return Err(changed("Original Escape target/owner binding changed"));
    }
    Ok(binding)
}
impl CancelAnchor {
    pub(crate) fn bind(process: &dyn Process, selection: &SelectionAnchor) -> Result<Self> {
        observe(process, selection, selection.context(), None)
    }
    pub(crate) fn require_before(&self, process: &dyn Process) -> Result<()> {
        observe(process, &self.selection, &self.context, Some(self)).map(|_| ())
    }
    pub(crate) fn private_baseline(&self) -> Result<Value> {
        // Full typed binding remains in memory; durable evidence stays bounded.
        let bytes = format!("{:?}", self.binding).into_bytes();
        Ok(
            json!({"process":self.identity,"base":self.base,"anchors":self.context.anchors,
            "binding_sha256":format!("{:x}",Sha256::digest(&bytes)),"binding_bytes":bytes.len()}),
        )
    }
}
fn observe(
    process: &dyn Process,
    selection: &SelectionAnchor,
    expected: &ClockContext,
    original: Option<&CancelAnchor>,
) -> Result<CancelAnchor> {
    if process.string_abi() != StringAbi::LibCxx {
        return Err(Error::new(
            "unsupported_platform",
            "Mac cancellation layout required",
        ));
    }
    let identity = process
        .identity()
        .cloned()
        .ok_or_else(|| changed("Exact process identity required"))?;
    let base = process.image_base();
    image_base(base)?;
    if original.is_some_and(|a| a.identity != identity || a.base != base) {
        return Err(changed(
            "Original cancellation process replaced before reads",
        ));
    }
    process.begin_snapshot()?;
    let w = Witnesses::new(process);
    let r = Reader::new(&w);
    selection.require_original_edges(&r, base, &identity)?;
    // Root replacement is refused before context/foreign own-country payload.
    for (offset, expected) in [
        game::MACOS.gs_global,
        game::MACOS.idler_global,
        game::MACOS.technology_db_global,
    ]
    .into_iter()
    .zip(expected.anchors)
    {
        if r.u64(base + offset)? != expected {
            return Err(changed(
                "Original cancellation roots changed before payload",
            ));
        }
    }
    if let Some(a) = original {
        a.binding.require_original(&r, base)?;
    }
    let context = game::context_from_reader(&r, base)?;
    same_context(expected, &context)?;
    let binding = from_reader(&r, base, context.anchors[1], original.map(|a| &a.binding))?;
    w.finish()?;
    process.check()?;
    if process.identity() != Some(&identity) {
        return Err(changed("Cancellation process changed during observation"));
    }
    Ok(CancelAnchor {
        identity,
        base,
        context,
        selection: selection.clone(),
        binding,
    })
}

#[cfg(test)]
mod tests;
