//! Private own-target and GUI witnesses. Uses only independently reviewed Mac
//! joins; a key or rectangle alone never binds an input target.
use super::*;
use crate::research_ui::Witnesses;
use std::collections::BTreeMap;

#[cfg(test)]
mod tests;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Owner {
    gs: u64,
    idler: u64,
    country: u64,
    status: u64,
    technology_database: u64,
    tree: u64,
    database: u64,
    progress: u64,
    target: u64,
    members: Vec<u64>,
    keys: BTreeMap<String, u64>,
}
fn exact(r: &Reader<'_>, p: u64, table: u64) -> Result<()> {
    if p < 0x10000 || p % 8 != 0 || r.u64(p)? != table {
        return Err(Error::new(
            "invalid_focus_type",
            "Exact Focus object required",
        ));
    }
    Ok(())
}
fn pointers(r: &Reader<'_>, address: u64, bound: usize) -> Result<Vec<u64>> {
    let (h, data, n) = r.array_header(address)?;
    let capacity = i32::from_le_bytes(h[8..12].try_into().unwrap());
    if capacity < 0 || capacity as usize > bound || (n > 0 && data % 8 != 0) {
        return Err(Error::new(
            "invalid_array",
            "Focus collection exceeds its bound",
        ));
    }
    let bytes = if n == 0 { vec![] } else { r.read(data, n * 8)? };
    Ok(bytes
        .chunks_exact(8)
        .map(|b| u64::from_le_bytes(b.try_into().unwrap()))
        .collect())
}
fn owner(r: &Reader<'_>, image: u64, key: &str, expected_status: Option<u64>) -> Result<Owner> {
    ui(r)?;
    let g = game::layout(r);
    let l = own_focus::layout(r);
    let gs = r.u64(image + g.gs_global)?;
    let idler = r.u64(image + g.idler_global)?;
    exact(r, gs, image + g.gs_vtable)?;
    exact(r, idler, image + g.idler_vtable)?;
    let tag = r.i32(gs + g.player_tag)?;
    if !(1..4096).contains(&tag) {
        return Err(Error::new(
            "player_unavailable",
            "Positive own tag required",
        ));
    }
    let index = i32::from_le_bytes(
        r.array_entry(gs + g.tag_country_index, tag as usize, 4)?
            .try_into()
            .unwrap(),
    );
    if index < 0 {
        return Err(Error::new("player_unavailable", "Own country index absent"));
    }
    let country = u64::from_le_bytes(
        r.array_entry(gs + g.countries, index as usize, 8)?
            .try_into()
            .unwrap(),
    );
    exact(r, country, image + g.country_vtable)?;
    if r.i32(country + 8)? != tag {
        return Err(Error::new(
            "invalid_focus_owner",
            "Own tag backlink differs",
        ));
    }
    let status = r.u64(country + g.country_technology_status)?;
    if expected_status.is_some_and(|expected| status != expected) {
        return Err(Error::new(
            "ownership_changed",
            "Original Focus own status changed",
        ));
    }
    let technology = crate::research::layout(r);
    let technology_database = r.u64(image + g.technology_db_global)?;
    exact(r, status, image + technology.status_vtable)?;
    exact(r, technology_database, image + technology.database_vtable)?;
    if r.u64(status + 0x130)? != country || r.i32(status + 0x138)? != tag {
        return Err(Error::new(
            "invalid_focus_owner",
            "Focus own status backlink differs",
        ));
    }
    let tree = r.u64(country + g.country_focus_tree)?;
    let database = r.u64(image + l.database_global)?;
    let progress = r.u64(country + g.country_focus_progress)?;
    exact(r, tree, image + l.tree_vtable)?;
    exact(r, database, image + l.database_vtable)?;
    exact(r, progress, image + l.progress_vtable)?;
    if r.u64(progress + 8)? != country {
        return Err(Error::new(
            "invalid_focus_owner",
            "Own progress backlink differs",
        ));
    }
    let members = pointers(r, tree + l.tree_focuses, 4096)?;
    let unique: BTreeSet<_> = members.iter().copied().collect();
    let canonical: BTreeSet<_> = pointers(r, database + 0x58, 20000)?
        .into_iter()
        .filter(|p| *p != 0)
        .collect();
    if unique.len() != members.len() || unique.contains(&0) || !unique.is_subset(&canonical) {
        return Err(Error::new(
            "invalid_focus_owner",
            "Unique canonical own membership required",
        ));
    }
    let mut keys = BTreeMap::new();
    for &focus in &members {
        exact(r, focus, image + l.focus_vtable)?;
        if r.u64(focus + l.focus_tree)? != tree {
            return Err(Error::new(
                "invalid_focus_owner",
                "Own Focus tree backlink differs",
            ));
        }
        let name = r.string_bounded(focus + 0x18, 256)?;
        if name.is_empty()
            || !name.bytes().all(|c| c.is_ascii_alphanumeric() || c == b'_')
            || keys.insert(name, focus).is_some()
        {
            return Err(Error::new(
                "invalid_focus_catalog",
                "Unique canonical Focus keys required",
            ));
        }
    }
    let target = *keys
        .get(key)
        .ok_or_else(|| Error::new("invalid_target", "Focus absent from own canonical tree"))?;
    let completed = pointers(r, progress + 0x40, 4096)?;
    if !completed.iter().all(|p| unique.contains(p)) {
        return Err(Error::new(
            "invalid_focus_owner",
            "Foreign completed membership",
        ));
    }
    if r.u64(progress + 0x10)? != 0 || completed.contains(&target) {
        return Err(Error::new(
            "invalid_target",
            "Idle Focus and unfinished target required",
        ));
    }
    Ok(Owner {
        gs,
        idler,
        country,
        status,
        technology_database,
        tree,
        database,
        progress,
        target,
        members,
        keys,
    })
}

/// Bind durable before-evidence before any UI preparation. Never independently
/// adopt a replacement object merely because it has the same Focus key.
pub(super) fn assignment_owner(
    process: &dyn Process,
    key: &str,
    expected: &own_focus::AssignmentSnapshot,
) -> Result<Owner> {
    let binding = expected.binding();
    if process.identity() != Some(binding.game()) {
        return Err(Error::new(
            "ownership_changed",
            "Focus assignment process changed",
        ));
    }
    process.begin_snapshot()?;
    let witnesses = Witnesses::new(process);
    witnesses.section("mac_focus_assignment_owner");
    let r = Reader::new(&witnesses);
    let image = process.image_base();
    let g = game::layout(&r);
    let l = own_focus::layout(&r);
    let changed = || {
        Error::new(
            "ownership_changed",
            "Typed Focus assignment binding changed",
        )
    };
    // Compare links before the broader owner reader may follow replacement
    // country, status, tree, progress or member payloads.
    if r.u64(image + g.gs_global)? != binding.gs()
        || r.u64(image + g.idler_global)? != binding.idler()
        || r.u64(image + g.technology_db_global)? != expected.context().anchors[2]
    {
        return Err(changed());
    }
    exact(&r, binding.gs(), image + g.gs_vtable)?;
    exact(&r, binding.idler(), image + g.idler_vtable)?;
    let tag = r.i32(binding.gs() + g.player_tag)?;
    if !(1..4096).contains(&tag) {
        return Err(changed());
    }
    let index = i32::from_le_bytes(
        r.array_entry(binding.gs() + g.tag_country_index, tag as usize, 4)?
            .try_into()
            .unwrap(),
    );
    if index < 0
        || u64::from_le_bytes(
            r.array_entry(binding.gs() + g.countries, index as usize, 8)?
                .try_into()
                .unwrap(),
        ) != binding.country()
    {
        return Err(changed());
    }
    r.object_type(binding.country(), "CCountry")?;
    if r.i32(binding.country() + 8)? != tag
        || r.u64(binding.country() + g.country_technology_status)? != binding.status()
        || r.u64(binding.country() + g.country_focus_tree)? != binding.tree()
        || r.u64(binding.country() + g.country_focus_progress)? != binding.progress()
        || r.u64(image + l.database_global)? != binding.database()
    {
        return Err(changed());
    }
    exact(
        &r,
        binding.status(),
        image + crate::research::layout(&r).status_vtable,
    )?;
    exact(&r, binding.tree(), image + l.tree_vtable)?;
    if r.u64(binding.status() + 0x130)? != binding.country()
        || r.i32(binding.status() + 0x138)? != tag
        || pointers(&r, binding.tree() + l.tree_focuses, 4096)? != binding.members()
    {
        return Err(changed());
    }
    let own = owner(&r, image, key, Some(binding.status()))?;
    if own.gs != binding.gs()
        || own.idler != binding.idler()
        || own.country != binding.country()
        || own.status != binding.status()
        || own.technology_database != expected.context().anchors[2]
        || own.tree != binding.tree()
        || own.database != binding.database()
        || own.progress != binding.progress()
        || own.target != binding.target()
        || own.members != binding.members()
        || &own.keys != binding.keys()
    {
        return Err(changed());
    }
    witnesses.finish()?;
    process.check()?;
    if process.identity() != Some(binding.game()) {
        return Err(changed());
    }
    Ok(own)
}

/// All payloads and their final witnesses use the same reader/ownership bracket.
pub(super) fn observe<T>(
    process: &dyn Process,
    key: &str,
    expected: Option<&Owner>,
    f: impl FnOnce(&Reader<'_>, u64, &Owner) -> Result<T>,
) -> Result<(Owner, T)> {
    process.begin_snapshot()?;
    let w = Witnesses::new(process);
    w.section("mac_focus_owner");
    let r = Reader::new(&w);
    let image = process.image_base();
    let own = owner(&r, image, key, expected.map(|owner| owner.status))?;
    if expected.is_some_and(|old| *old != own) {
        return Err(Error::new(
            "ownership_changed",
            "Original own Focus binding changed",
        ));
    }
    w.section("mac_focus_ui");
    let value = f(&r, image, &own)?;
    w.finish()?;
    process.check()?;
    Ok((own, value))
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct View {
    manager: u64,
    object: u64,
    root: u64,
}
fn view(r: &Reader<'_>, image: u64, own: &Owner, slot: u64) -> Result<View> {
    let manager = r.u64(own.idler + MACOS.manager)?;
    let object = r.u64(manager + slot)?;
    let root = gui_tree::container(r, r.u64(object + MACOS.view_root)?, image)?
        .ok_or_else(|| Error::new("invalid_gui", "Exact Focus view root required"))?;
    Ok(View {
        manager,
        object,
        root,
    })
}
pub(super) fn open(r: &Reader<'_>, image: u64, own: &Owner, slot: u64) -> Result<bool> {
    gui_tree::visible(r, view(r, image, own, slot)?.root)
}
/// Readiness only. None of these states binds a target or authorizes input.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum FocusViewState {
    Hidden,
    Opening,
    Ready,
}
fn displayed_country(r: &Reader<'_>, v: &View, own: &Owner) -> Result<u64> {
    let displayed = r.u64(v.object + MACOS.displayed_country)?;
    if displayed != 0 && displayed != own.country {
        return Err(Error::new(
            "foreign_focus_scope",
            "Displayed Focus country differs from own country",
        ));
    }
    Ok(displayed)
}
/// Used only within the caller's complete ownership/witness bracket. Do not
/// follow the displayed country or inspect item, panel, or renderer payloads.
pub(super) fn view_state(r: &Reader<'_>, image: u64, own: &Owner) -> Result<FocusViewState> {
    let v = view(r, image, own, MACOS.focus_view)?;
    let displayed = displayed_country(r, &v, own)?;
    if !gui_tree::visible(r, v.root)? {
        Ok(FocusViewState::Hidden)
    } else if displayed == 0 {
        Ok(FocusViewState::Opening)
    } else {
        Ok(FocusViewState::Ready)
    }
}
fn own_view(r: &Reader<'_>, image: u64, own: &Owner) -> Result<View> {
    let v = view(r, image, own, MACOS.focus_view)?;
    if displayed_country(r, &v, own)? == 0 {
        return Err(Error::new(
            "foreign_focus_scope",
            "Displayed Focus country is unset",
        ));
    }
    if !gui_tree::visible(r, v.root)? {
        return Err(Error::new(
            "focus_state_changed",
            "Own Focus tree is not visible",
        ));
    }
    Ok(v)
}
fn named(r: &Reader<'_>, image: u64, root: u64, name: &str) -> Result<Option<u64>> {
    let mut found = None;
    for child in gui_tree::children(r, root)? {
        if gui_tree::name(r, child, image)?.as_deref() == Some(name)
            && found.replace(child).is_some()
        {
            return Err(Error::new("invalid_gui", "Duplicate named Focus control"));
        }
    }
    Ok(found)
}
fn window(r: &Reader<'_>, image: u64, p: u64) -> Result<u64> {
    gui_tree::container(r, p, image)?
        .ok_or_else(|| Error::new("invalid_gui", "Exact Focus container required"))
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Control {
    view: View,
    item: u64,
    frame: u64,
    widget: u64,
    zoom: u32,
    pub rect: Rect,
    pub disabled: bool,
}
fn button(r: &Reader<'_>, image: u64, widget: u64) -> Result<(Rect, bool)> {
    exact(r, widget, image + 0x34aafa0)?; // already reviewed CButtonStandard
    if !gui_tree::shown(r, widget)? {
        return Err(Error::new(
            "gui_target_unavailable",
            "Focus button is hidden",
        ));
    }
    let state = gui_tree::button_state(r, widget)?;
    if !(0..=3).contains(&state) {
        return Err(Error::new("invalid_gui", "Unknown Focus button state"));
    }
    let rect = gui_tree::rect(r, image, widget, GAME)?;
    if rect.x < 0 || rect.y < 0 || rect.x + rect.w > 1920 || rect.y + rect.h > 1080 {
        return Err(Error::new(
            "gui_target_unavailable",
            "Focus button is clipped",
        ));
    }
    Ok((rect, state == 3))
}
pub(super) fn focus_button(r: &Reader<'_>, image: u64, own: &Owner) -> Result<Control> {
    let v = view(r, image, own, MACOS.politics_view)?;
    let goal = named(r, image, v.root, "active_goal")?
        .ok_or_else(|| Error::new("gui_target_unavailable", "Focus area missing"))?;
    let frame = window(r, image, goal)?;
    if !gui_tree::visible(r, v.root)? || !gui_tree::visible(r, frame)? {
        return Err(Error::new(
            "gui_target_unavailable",
            "Politics Focus area is hidden",
        ));
    }
    let widget = named(r, image, frame, "add_national_goal_button")?
        .ok_or_else(|| Error::new("gui_target_unavailable", "Focus button missing"))?;
    let (rect, disabled) = button(r, image, widget)?;
    Ok(Control {
        view: v,
        item: goal,
        frame,
        widget,
        zoom: 1f32.to_bits(),
        rect,
        disabled,
    })
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Tree {
    view: View,
    zoom: u32,
    pub target: Option<Control>,
    pub obstacles: Vec<(u64, Rect)>,
}
pub(super) fn same_view(r: &Reader<'_>, image: u64, own: &Owner, tree: &Tree) -> Result<()> {
    unchanged(&tree.view, &own_view(r, image, own)?)
}
struct Items {
    view: View,
    zoom: f32,
    values: Vec<(u64, bool)>,
}
/// Validate the complete item collection before any container or renderer
/// payload, including items following the target and off-screen own members.
fn tree_items(r: &Reader<'_>, image: u64, own: &Owner) -> Result<Items> {
    let v = own_view(r, image, own)?;
    let zoom = f32::from_le_bytes(r.read(v.object + MACOS.zoom, 4)?.try_into().unwrap());
    if !zoom.is_finite() || !(0.1..=4.0).contains(&zoom) {
        return Err(Error::new(
            "invalid_gui_geometry",
            "Bounded Focus zoom required",
        ));
    }
    let items = pointers(r, v.object + MACOS.items, 4096)?;
    let mut seen = BTreeSet::new();
    let mut values = Vec::with_capacity(items.len());
    let mut matches = 0;
    for item in items {
        if !seen.insert(item) {
            return Err(Error::new("invalid_gui", "Duplicate Focus item"));
        }
        exact(r, item, image + MACOS.item_vtable)?;
        let focus = r.u64(item + MACOS.item_focus)?;
        if focus != 0 && !own.members.contains(&focus) {
            return Err(Error::new(
                "foreign_focus_scope",
                "Foreign Focus item refused before payload",
            ));
        }
        let selected = focus == own.target;
        matches += usize::from(selected);
        if matches > 1 {
            return Err(Error::new("invalid_gui", "Duplicate own Focus node"));
        }
        values.push((item, selected));
    }
    Ok(Items {
        view: v,
        zoom,
        values,
    })
}
fn item_frame(r: &Reader<'_>, image: u64, item: u64) -> Result<Option<u64>> {
    let frame = window(r, image, r.u64(item + MACOS.item_container)?)?;
    Ok(gui_tree::visible(r, frame)?.then_some(frame))
}
fn node_control(view: &View, item: u64, frame: u64, symbol: (u64, Rect), zoom: f32) -> Control {
    Control {
        view: view.clone(),
        item,
        frame,
        widget: symbol.0,
        zoom: zoom.to_bits(),
        rect: symbol.1,
        disabled: false,
    }
}
/// A complete target control witness, without a background-drag proof. The
/// full item scan remains mandatory; only unrelated container/renderer payloads
/// and the target's background geometry are omitted.
pub(super) fn node(r: &Reader<'_>, image: u64, own: &Owner) -> Result<Option<Control>> {
    let items = tree_items(r, image, own)?;
    let Some(&(item, _)) = items.values.iter().find(|(_, selected)| *selected) else {
        return Ok(None);
    };
    let Some(frame) = item_frame(r, image, item)? else {
        return Ok(None);
    };
    let widget = named(r, image, frame, "symbol")?
        .ok_or_else(|| Error::new("gui_target_unavailable", "Visible target symbol missing"))?;
    if !gui_tree::shown(r, widget)? {
        return Err(Error::new(
            "gui_target_unavailable",
            "Visible target symbol missing",
        ));
    }
    let rect = gui_tree::rect_zoomed(r, image, widget, GAME, items.zoom)?;
    Ok(Some(node_control(
        &items.view,
        item,
        frame,
        (widget, rect),
        items.zoom,
    )))
}
pub(super) fn symbols(r: &Reader<'_>, image: u64, own: &Owner) -> Result<Tree> {
    let items = tree_items(r, image, own)?;
    let mut target = None;
    let mut obstacles = vec![];
    for (item, selected) in items.values {
        let Some(frame) = item_frame(r, image, item)? else {
            continue;
        };
        let mut symbol = None;
        for name in ["symbol", "bg"] {
            if let Some(widget) = named(r, image, frame, name)? {
                if !gui_tree::shown(r, widget)? {
                    continue;
                }
                // No dropped geometry: an unmeasurable obstacle cannot prove a
                // safe background origin for an ordinary drag.
                let rect = gui_tree::rect_zoomed(r, image, widget, GAME, items.zoom)?;
                obstacles.push((widget, rect));
                if name == "symbol" {
                    symbol = Some((widget, rect));
                }
            }
        }
        if selected {
            let symbol = symbol.ok_or_else(|| {
                Error::new("gui_target_unavailable", "Visible target symbol missing")
            })?;
            target = Some(node_control(&items.view, item, frame, symbol, items.zoom));
        }
    }
    Ok(Tree {
        view: items.view,
        zoom: items.zoom.to_bits(),
        target,
        obstacles,
    })
}
pub(super) fn detail(r: &Reader<'_>, image: u64, own: &Owner) -> Result<Option<Control>> {
    let v = own_view(r, image, own)?;
    let panels = pointers(r, v.object + MACOS.panels, 8)?;
    let mut found = None;
    let mut seen = BTreeSet::new();
    for panel in panels {
        if !seen.insert(panel) {
            return Err(Error::new("invalid_gui", "Duplicate Focus panel"));
        }
        exact(r, panel, image + MACOS.panel_vtable)?;
        let selected = r.u64(panel + MACOS.panel_selected)?;
        if selected != 0 && !own.members.contains(&selected) {
            return Err(Error::new(
                "foreign_focus_scope",
                "Foreign Focus detail refused before payload",
            ));
        }
        if selected != own.target {
            continue;
        }
        let frame = window(r, image, r.u64(panel + MACOS.panel_window)?)?;
        if !gui_tree::visible(r, frame)? {
            continue;
        }
        let widget = named(r, image, frame, "start")?
            .ok_or_else(|| Error::new("gui_target_unavailable", "Focus Start missing"))?;
        let (rect, disabled) = button(r, image, widget)?;
        if found
            .replace(Control {
                view: v.clone(),
                item: panel,
                frame,
                widget,
                zoom: 1f32.to_bits(),
                rect,
                disabled,
            })
            .is_some()
        {
            return Err(Error::new("invalid_gui", "Duplicate visible Focus detail"));
        }
    }
    Ok(found)
}
