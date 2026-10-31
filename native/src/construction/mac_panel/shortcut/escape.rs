//! Escape's requested target is retained; its current complete keyboard prefix
//! is recomputed. Prefix membership is not original control identity. The caller
//! supplies one encompassing Witnesses scope; this grants no input authority.
use super::{invalid, observe_key, pointer, sprite, PlainKey, Reader, Result};

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::construction::mac_panel) struct EscapeTarget {
    base: u64,
    gui: u64,
    graphics: u64,
    button: u64,
    container: u64,
    renderer: u64,
    renderer_primary: u64,
    named_parent: u64,
    // Ordered owner/type/attachment edges only: never the transient source
    // vector, incidental predecessor identities, flags or shortcut contents.
    edges: Vec<(u64, u64)>,
}
fn edge(r: &Reader<'_>, at: u64, edges: &mut Vec<(u64, u64)>) -> Result<u64> {
    let value = r.u64(at)?;
    edges.push((at, value));
    Ok(value)
}
fn owned(r: &Reader<'_>, at: u64, expected: u64, edges: &mut Vec<(u64, u64)>) -> Result<()> {
    if edge(r, at, edges)? != expected {
        return Err(invalid("Original Escape target ownership differs"));
    }
    Ok(())
}
fn typed(r: &Reader<'_>, at: u64, expected: u64, edges: &mut Vec<(u64, u64)>) -> Result<()> {
    pointer(at)?;
    owned(r, at, expected, edges)
}
impl EscapeTarget {
    /// Compare original pointers before dereferencing replacement GUI, graphics,
    /// renderer or parent payload. Caller also retains process/campaign roots.
    pub(in crate::construction::mac_panel) fn require_original(
        &self,
        r: &Reader<'_>,
        base: u64,
    ) -> Result<()> {
        if base != self.base {
            return Err(invalid("Original Escape image base changed"));
        }
        for &(at, value) in &self.edges {
            if r.u64(at)? != value {
                return Err(invalid(
                    "Original Escape target edge changed before payload",
                ));
            }
        }
        Ok(())
    }
}
fn target(
    r: &Reader<'_>,
    base: u64,
    gui: u64,
    button: u64,
    container: u64,
) -> Result<EscapeTarget> {
    let mut edges = Vec::new();
    owned(r, base + 0x37eda08, gui, &mut edges)?;
    typed(r, gui, base + 0x36a5810, &mut edges)?;
    let graphics = edge(r, gui + 0x98, &mut edges)?;
    typed(r, graphics, base + 0x35e5b58, &mut edges)?;
    typed(r, button, base + 0x36b9738, &mut edges)?;
    owned(r, button + 0x58, gui, &mut edges)?;
    owned(r, button + 0xd8, graphics, &mut edges)?;
    let renderer = edge(r, button + 0xe0, &mut edges)?;
    sprite(r, base, renderer)?;
    let renderer_primary = edge(r, renderer, &mut edges)?;
    owned(r, renderer + 0x138, button, &mut edges)?;
    // The already-owned direct container supplies its own attached graphical
    // parent; do not infer a generic GUI owner from graphical object payload.
    typed(r, container, base + 0x36ba978, &mut edges)?;
    owned(r, container + 0x88, gui, &mut edges)?;
    let named_parent = edge(r, container + 0x1358, &mut edges)?;
    typed(r, named_parent, base + 0x36bea88, &mut edges)?;
    owned(r, button + 0x180, named_parent, &mut edges)?;
    owned(r, renderer + 0x108, named_parent, &mut edges)?;
    Ok(EscapeTarget {
        base,
        gui,
        graphics,
        button,
        container,
        renderer,
        renderer_primary,
        named_parent,
        edges,
    })
}
/// Validate the immutable original target first, then evaluate the complete
/// current Escape prefix using the unchanged strict predicate with no stale
/// prefix. Unknown recipients and conflicts still refuse; Unsupported is never
/// treated as an inert predecessor. A returned target certifies this observation
/// only, and callers must repeat this function immediately before input.
pub(in crate::construction::mac_panel) fn observe_escape_current_prefix(
    r: &Reader<'_>,
    base: u64,
    gui: u64,
    button: u64,
    container: u64,
    original: Option<&EscapeTarget>,
) -> Result<EscapeTarget> {
    if let Some(before) = original {
        if before.base != base
            || before.gui != gui
            || before.button != button
            || before.container != container
        {
            return Err(invalid("Original Escape requested control changed"));
        }
        before.require_original(r, base)?;
    }
    let now = target(r, base, gui, button, container)?;
    if original.is_some_and(|before| before != &now) {
        return Err(invalid("Original Escape target binding changed"));
    }
    let current = observe_key(r, base, gui, button, container, PlainKey::Escape, None)?;
    current.require_ready()?;
    // These joins are independently read by the shared current-prefix reader;
    // one caller Witnesses must include every target and prefix field consumed.
    if current.graphics != now.graphics
        || current.renderer != now.renderer
        || current.named_parent != now.named_parent
    {
        return Err(invalid("Escape target changed within current observation"));
    }
    Ok(now)
}
