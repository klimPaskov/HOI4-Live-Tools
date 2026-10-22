//! Ordinary W closing proof for Linux ELF a4971c9e only. Callback186d120
//! toggles mode5 only on tab0; on the facilities tab it merely changes tabs.
//! Static layout recovery is not live acceptance. Never invoke these functions.
use super::{array, exact, UiContext, Witnesses};
use crate::{
    error::{Error, Result},
    game,
    memory::{Reader, StringAbi},
    platform::Process,
    research_stage::PanelBinding,
};
use serde_json::{json, Value};

#[doc(hidden)]
pub struct Observation {
    pub body: Value,
    pub context: UiContext,
    pub binding: PanelBinding,
    pub selected_folder: Option<String>,
}
fn refuse(message: &str) -> Error {
    Error::new("research_precondition", message)
}
/// A missing/animating window is never evidence of a closed panel.
fn settled(r: &Reader<'_>, base: u64, root: u64, registry: u64) -> Result<bool> {
    if !(0x10000..=0x7fff_ffff_0000).contains(&root) {
        return Err(refuse("Materialized closing root required"));
    }
    exact(r, root, base + 0x4347d58)?;
    if r.u64(root + 0x98)? != registry {
        return Err(refuse("Closing root belongs to another registry"));
    }
    let transitions = [r.i32(root + 0xeb0)?, r.i32(root + 0xef8)?];
    if transitions != [0, 0] {
        return Err(if transitions.iter().all(|v| matches!(v, 0..=2)) {
            Error::new(
                "research_panel_transition",
                "Closing root is in a known animation transition",
            )
        } else {
            refuse("Closing root has an unknown transition state")
        });
    }
    Ok(r.byte(root + 0xa5)? & 8 != 0)
}
fn named_child(r: &Reader<'_>, root: u64, key: &str) -> Result<u64> {
    crate::gui_registry::lookup(r, root + 0xbc8, &array(r, root + 0xbb0, 128)?, key, 128)?
        .ok_or_else(|| refuse("Named closing child unavailable"))
}
/// Returns exact panel/facilities roots after checking ownership before child
/// traversal. Facilities is CFacilitiesTabView, not a CContainerWindow.
fn roots(
    r: &Reader<'_>,
    base: u64,
    view: u64,
    tree: u64,
    idler: u64,
    registry: u64,
) -> Result<([u64; 4], u64, bool)> {
    let root = r.u64(view + 0x2f8)?;
    let tree_root = r.u64(tree + 0x2f8)?;
    let panel_visible = settled(r, base, root, registry)?;
    if settled(r, base, tree_root, registry)? {
        return Err(refuse("Research tree must be closed before panel closing"));
    }
    //1596c50 joins named facilities parent;2651f50/16f5980 bind its
    // embedded view to that parent(+60) and own idler(+68).16f5ad0 creates
    // facilities_view_Instance at+70.15975a0 hides it through2653af0.
    let parent = r.u64(view + 0x910)?;
    if named_child(r, root, "facilities")? != parent {
        return Err(refuse("Facilities parent named join changed"));
    }
    settled(r, base, parent, registry)?;
    let facilities = r.u64(view + 0x900)?;
    exact(r, facilities, base + 0x42f2708)?;
    if r.u64(facilities + 0x68)? != idler || r.u64(facilities + 0x60)? != parent {
        return Err(refuse("Facilities view owner changed"));
    }
    let facilities_root = r.u64(facilities + 0x70)?;
    if named_child(r, parent, "facilities_view_Instance")? != facilities_root {
        return Err(refuse("Facilities embedded root named join changed"));
    }
    if settled(r, base, facilities_root, registry)? {
        return Err(refuse("Facilities view must be closed on the research tab"));
    }
    Ok((
        [root, tree_root, facilities, facilities_root],
        parent,
        panel_visible,
    ))
}

#[doc(hidden)]
pub fn observe(process: &dyn Process) -> Result<Observation> {
    // Refuse before any build-specific read. Installed Process binding validates
    // the exact executable build; fixture readers must explicitly select ABI.
    if process.string_abi() != StringAbi::LibStdCxx {
        return Err(Error::new(
            "capability_unavailable",
            "Research closing requires reviewed Linux build",
        ));
    }
    process.begin_snapshot()?;
    let witnesses = Witnesses::new(process);
    let r = Reader::new(&witnesses);
    let base = process.image_base();
    witnesses.section("close_own_research");
    let (canonical, own) = crate::research::observe_from_reader(&r, base, 0, 1, true)?;
    let own = own.ok_or_else(|| refuse("Closing own campaign context unavailable"))?;
    let [country, status] = own.own;
    let [gs, idler, _] = own.campaign.anchors;
    let registry = r.u64(idler + 0x4f8)?;
    exact(&r, registry, base + 0x42736f8)?;
    let manager = r.u64(idler + 0x6b8)?;
    if r.u64(manager + 0x180)? != idler || r.u64(base + 0x45778e0)? != registry {
        return Err(refuse("Own closing manager/input registry changed"));
    }
    let view = r.u64(manager + 0xf0)?;
    let tree = r.u64(manager + 0xf8)?;
    exact(&r, view, base + 0x424dc38)?;
    exact(&r, tree, base + 0x424de40)?;
    if r.u64(view + 0x2f0)? != idler || r.u64(tree + 0x2f0)? != idler || r.i32(tree + 0xe24)? != 6 {
        return Err(refuse("Own closing research views changed"));
    }
    let displayed = r.i32(tree + 0xe20)?;
    if displayed != 0 && displayed != r.i32(status + 0x138)? {
        return Err(refuse("Foreign or unknown displayed research country"));
    }
    let tab = r.i32(view + 0x300)?;
    if tab != 0 {
        return Err(refuse("Ordinary W closing requires research tab0"));
    }
    let selected = r.u64(tree + 0xe08)?;
    let selected_index = if selected == 0 {
        None
    } else {
        Some(
            own.slots
                .iter()
                .position(|p| *p == selected)
                .ok_or_else(|| refuse("Closing selected slot outside own status"))?,
        )
    };
    witnesses.section("close_roots");
    let (roots, facilities_parent, panel_visible) = roots(&r, base, view, tree, idler, registry)?;
    let mode = r.i32(manager + 0x3d8)?;
    if !matches!((mode, panel_visible), (5, true) | (23, false)) {
        return Err(if matches!(mode, 5 | 23) {
            Error::new(
                "research_panel_transition",
                "Research mode and literal panel visibility have not settled",
            )
        } else {
            refuse("Closing requires shown Research or known closed idle state")
        });
    }
    let (folder_index, folder_windows, selected_folder) =
        crate::category_ui::closing_selection(&r, base, tree, roots[1], registry)?;
    witnesses.section("close_engine");
    let engine =
        crate::production_panel::engine(&r, base, gs, idler, manager, registry, &mut Vec::new())?;
    witnesses.section("close_windows");
    let common =
        crate::construction::controls::blockers(&r, base, country, idler, manager, registry)?;
    witnesses.section("close_gui_flags");
    let gui_clear = r.byte(registry + 0x150)? == 0
        && r.byte(registry + 0x12)? == 0
        && r.u64(registry + 0x158)? == 0
        && r.u64(registry + 0x230)? == 0;
    let common_clear = common["available"] == true
        && common["politics_root"]["settled_visible"] == false
        && common["focus_root_settled_hidden"] == true
        && common["research_panel_settled_visible"] == panel_visible
        && common["research_tree_settled_visible"] == false
        && common["focus_detail_visible_animating_counts"] == json!([0, 0])
        && common["own_research_info_visible_animating_counts"] == json!([0, 0]);
    if engine["conservative_engine_clear"] != true || !common_clear || !gui_clear {
        return Err(refuse(&format!(
            "Closing gates refused; engine={}, own_windows={}, gui={}",
            engine["conservative_engine_clear"] == true,
            common_clear,
            gui_clear
        )));
    }
    let context = UiContext {
        campaign: own.campaign,
        anchors: [country, status, registry, manager, view, tree],
        slots: own.slots,
    };
    let binding = PanelBinding {
        anchors: context.anchors,
        slots: context.slots.clone(),
        roots,
        facilities_parent,
        selected_slot_pointer: selected,
        folder_index,
        folder_windows,
        tab,
    };
    witnesses.finish()?;
    process.check()?;
    // No physical addresses enter the public-shaped body. The private binding
    // is supplied separately to the durable stage and redacted by Execution.
    let body = json!({"owned":true,"build_id":game::BUILD_ID,
        "canonical_research":canonical,"own_slot_count":context.slots.len(),
        "selected_slot":selected_index,"active_mode_raw":mode,
        "ordinary_tree":{"materialized":true,"visible_predicate":false},
        "panel":{"materialized":true,"visible_predicate":panel_visible},
        "technology_info":{"visible_count":0,"windows":[]}});
    Ok(Observation {
        body,
        context,
        binding,
        selected_folder,
    })
}

#[cfg(test)]
mod tests;
