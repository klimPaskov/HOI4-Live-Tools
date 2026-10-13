//! Producer-only, own local GUI facts. No global modal/topmost assertion.
use crate::{
    error::{Error, Result},
    memory::Reader,
};
use serde_json::{json, Value};
use std::collections::BTreeSet;
#[derive(Clone, Debug, PartialEq, Eq)]
struct LocalWindow {
    pointer: u64,
    visible: bool,
    animation: [i32; 2],
}
impl LocalWindow {
    fn animating(&self) -> bool {
        self.animation.contains(&2)
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
#[doc(hidden)]
pub struct FocusUiFacts {
    mode: i32,
    focus: Option<LocalWindow>,
    details: Vec<Option<LocalWindow>>,
    research: Option<Vec<Option<LocalWindow>>>,
}
impl FocusUiFacts {
    pub fn focus_mode_selected(&self) -> Option<bool> {
        (0..=23).contains(&self.mode).then_some(self.mode == 13)
    }
    pub fn focus_view_local_ready(&self) -> Option<bool> {
        self.focus.as_ref().map(|w| w.visible && !w.animating())
    }
    pub fn focus_detail_local_counts(&self) -> Option<[usize; 2]> {
        counts(&self.details)
    }
    pub fn own_research_info_local_counts(&self) -> Option<[usize; 2]> {
        self.research.as_ref().and_then(|v| counts(v))
    }
    pub fn local_summary(&self) -> Value {
        json!({"scope":"complete_own_local_gui_flags_only","managed_mode_raw":self.mode,"focus_mode_selected":self.focus_mode_selected(),"focus_view_local_ready":self.focus_view_local_ready(),"focus_detail_visible_animating_counts":self.focus_detail_local_counts(),"own_research_info_visible_animating_counts":self.own_research_info_local_counts(),"global_modal_clear_available":false,"effective_routing_verified":false,"input_authorized":false})
    }
}
fn counts(v: &[Option<LocalWindow>]) -> Option<[usize; 2]> {
    let mut c = [0, 0];
    for w in v {
        let w = w.as_ref()?;
        c[0] += usize::from(w.visible);
        c[1] += usize::from(w.animating());
    }
    Some(c)
}
fn exact(r: &Reader<'_>, p: u64, t: u64) -> Result<()> {
    if p == 0 || p % 8 != 0 || r.u64(p)? != t {
        Err(Error::new(
            "invalid_focus_ui_type",
            "Exact own GUI primary required",
        ))
    } else {
        Ok(())
    }
}
fn window(r: &Reader<'_>, base: u64, p: u64, registry: u64) -> Result<Option<LocalWindow>> {
    if p == 0 {
        return Ok(None);
    }
    exact(r, p, base + 0x4347d58)?;
    if r.u64(p + 0x98)? != registry {
        return Err(Error::new(
            "invalid_focus_owner",
            "Own local GUI registry required",
        ));
    }
    Ok(Some(LocalWindow {
        pointer: p,
        visible: r.byte(p + 0xa5)? & 8 != 0,
        animation: [r.i32(p + 0xeb0)?, r.i32(p + 0xef8)?],
    }))
}
fn array(r: &Reader<'_>, a: u64, bound: usize) -> Result<Vec<u64>> {
    let (h, p, n) = r.array_header(a)?;
    let cap = i32::from_le_bytes(h[8..12].try_into().unwrap()) as usize;
    if cap > bound || (n > 0 && p % 8 != 0) {
        return Err(Error::new(
            "invalid_focus_ui_array",
            "Bounded own GUI array required",
        ));
    }
    let b = if n == 0 { vec![] } else { r.read(p, n * 8)? };
    let v: Vec<_> = b
        .chunks_exact(8)
        .map(|x| u64::from_le_bytes(x.try_into().unwrap()))
        .collect();
    let u: BTreeSet<_> = v.iter().copied().collect();
    if u.len() != v.len() || u.contains(&0) {
        return Err(Error::new(
            "invalid_focus_ui_array",
            "Unique nonnull own GUI entries required",
        ));
    }
    Ok(v)
}
// Called only after complete canonical own panel/selection validation. All
// retained windows are inspected for flags, never their selected-tech payload.
pub(crate) fn observe(
    r: &Reader<'_>,
    base: u64,
    idler: u64,
    manager: u64,
    view: u64,
    registry: u64,
    tag: i32,
) -> Result<FocusUiFacts> {
    let mode = r.i32(manager + 0x3d8)?;
    let focus = window(r, base, r.u64(view + 0x2f8)?, registry)?;
    let panels = array(r, view + 0x1180, 32)?;
    let mut details = vec![];
    for panel in panels {
        exact(r, panel, base + 0x43023f8)?;
        if r.u64(panel + 0x7b8)? != idler || r.u64(panel + 0x7c0)? != view {
            return Err(Error::new(
                "invalid_focus_owner",
                "Own focus detail before window payload",
            ));
        }
        details.push(window(r, base, r.u64(panel + 0x7c8)?, registry)?);
    }
    let tree = r.u64(manager + 0xf8)?;
    let research = if tree == 0 {
        None
    } else {
        exact(r, tree, base + 0x424de40)?;
        if r.u64(tree + 0x2f0)? != idler {
            return Err(Error::new(
                "invalid_focus_owner",
                "Own research tree required",
            ));
        }
        let displayed = r.i32(tree + 0xe20)?;
        if r.i32(tree + 0xe24)? != 6 || (displayed != 0 && displayed != tag) {
            None
        } else {
            let infos = array(r, tree + 0xdc0, 64)?;
            let mut windows = vec![];
            for info in infos {
                exact(r, info, base + 0x430bbf0)?;
                if r.u64(info + 0xa78)? != idler || r.u64(info + 0xa80)? != tree {
                    return Err(Error::new(
                        "invalid_focus_owner",
                        "Own research Info before window payload",
                    ));
                }
                windows.push(window(r, base, r.u64(info + 0xa88)?, registry)?);
            }
            Some(windows)
        }
    };
    Ok(FocusUiFacts {
        mode,
        focus,
        details,
        research,
    })
}
