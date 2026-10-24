//! Retained own Research navigation observations for Linux ELF a4971c9e.
//! No input, routing/eligibility, journal, public JSON constructor or goal receipt.
//! The owner must separately qualify keyboard blockers and the ordinary stage.
use super::{array, closing, exact, UiContext, Witnesses};
use crate::{
    daily_governor::Scope,
    daily_panel_navigation::transit::{ManagedPanel, Snapshot},
    error::{Error, Result},
    memory::{ReadMemory, Reader, StringAbi},
    platform::{Identity, Process},
    research,
    research_stage::{OwnState, PanelBinding},
};
use std::{
    collections::{BTreeMap, BTreeSet},
    time::Instant,
};

fn refused(message: &str) -> Error {
    Error::new("research_transit_precondition", message)
}
fn changed(message: &str) -> Error {
    Error::new("ownership_changed", message)
}
fn deadline(until: Instant, now: &dyn Fn() -> Instant) -> Result<()> {
    if now() >= until {
        Err(Error::new(
            "panel_transit_deadline",
            "Original Research transit observation expired",
        ))
    } else {
        Ok(())
    }
}
fn pointer(p: u64) -> Result<()> {
    if !(0x10000..=0x7fff_ffff_0000).contains(&p) || p % 8 != 0 {
        return Err(refused(
            "Bounded aligned original Research pointer required",
        ));
    }
    Ok(())
}

// One original bound wraps scalar reads AND the final batched witnesses. An
// underlying concrete error is preserved; a late successful sample is discarded.
struct Timed<'a> {
    inner: &'a dyn Process,
    until: Instant,
    now: &'a dyn Fn() -> Instant,
}
impl Timed<'_> {
    fn completed<T>(&self, result: Result<T>) -> Result<T> {
        let boundary = deadline(self.until, self.now);
        match result {
            Err(error) => Err(error),
            Ok(value) => {
                boundary?;
                Ok(value)
            }
        }
    }
}
impl ReadMemory for Timed<'_> {
    fn string_abi(&self) -> StringAbi {
        self.inner.string_abi()
    }
    fn read(&self, address: u64, size: usize) -> Result<Vec<u8>> {
        deadline(self.until, self.now)?;
        self.completed(self.inner.read(address, size))
    }
    fn read_many(&self, requests: &[(u64, usize)]) -> Result<Vec<Vec<u8>>> {
        deadline(self.until, self.now)?;
        self.completed(self.inner.read_many(requests))
    }
}
impl Process for Timed<'_> {
    fn image_base(&self) -> u64 {
        self.inner.image_base()
    }
    fn identity(&self) -> Option<&Identity> {
        self.inner.identity()
    }
    fn begin_snapshot(&self) -> Result<()> {
        deadline(self.until, self.now)?;
        self.completed(self.inner.begin_snapshot())
    }
    fn check(&self) -> Result<()> {
        deadline(self.until, self.now)?;
        self.completed(self.inner.check())
    }
}

#[derive(Debug, PartialEq, Eq)]
struct Members {
    header: Vec<u8>,
    pointers: Vec<u64>,
}
fn members(
    r: &Reader<'_>,
    address: u64,
    limit: usize,
    original: Option<&Members>,
) -> Result<Members> {
    let (header, data, count) = r.array_header(address)?;
    let capacity = i32::from_le_bytes(header[8..12].try_into().unwrap()) as usize;
    if capacity > limit || (count != 0 && data % 8 != 0) {
        return Err(refused(
            "Research transit complete array exceeds policy/alignment",
        ));
    }
    // Do not follow a substituted backing array even if its values might match.
    if original.is_some_and(|old| old.header != header) {
        return Err(changed(
            "Original Research transit array header changed before payload",
        ));
    }
    let bytes = if count == 0 {
        Vec::new()
    } else {
        r.read(data, count * 8)?
    };
    let pointers: Vec<_> = bytes
        .chunks_exact(8)
        .map(|b| u64::from_le_bytes(b.try_into().unwrap()))
        .collect();
    if original.is_some_and(|old| old.pointers != pointers) {
        return Err(changed(
            "Original Research transit membership changed before object payload",
        ));
    }
    let mut unique = BTreeSet::new();
    for p in &pointers {
        pointer(*p)?;
        if !unique.insert(*p) {
            return Err(refused("Unique Research transit members required"));
        }
    }
    if r.read(address, 16)? != header {
        return Err(Error::new("torn_read", "Research transit array changed"));
    }
    Ok(Members { header, pointers })
}

#[derive(Debug)]
struct Edges {
    registry: u64,
    manager: u64,
    view: u64,
    tree: u64,
    roots: [u64; 4],
    facilities_parent: u64,
    slots: Members,
    templates: Members,
    technologies: Members,
}
fn link(r: &Reader<'_>, address: u64, original: Option<u64>) -> Result<u64> {
    let current = r.u64(address)?;
    if original.is_some_and(|old| old != current) {
        return Err(changed(
            "Original Research transit owner/root edge changed before payload",
        ));
    }
    pointer(current)?;
    Ok(current)
}
fn read_edges(
    r: &Reader<'_>,
    base: u64,
    scope: &Scope,
    original: Option<&Edges>,
) -> Result<(Edges, i32)> {
    let [_, idler, db] = scope.anchors;
    let registry = link(r, idler + 0x4f8, original.map(|old| old.registry))?;
    exact(r, registry, base + 0x42736f8)?;
    if r.u64(base + 0x45778e0)? != registry {
        return Err(changed(
            "Research transit input registry is not the original own registry",
        ));
    }
    let manager = link(r, idler + 0x6b8, original.map(|old| old.manager))?;
    if r.u64(manager + 0x180)? != idler {
        return Err(changed("Research transit manager owner changed"));
    }
    let mode = r.i32(manager + 0x3d8)?;
    if !matches!(mode, 5 | 23) {
        return Err(refused(
            "Research transit supports only settled Idle23/Research5; departure required",
        ));
    }
    let view = link(r, manager + 0xf0, original.map(|old| old.view))?;
    let tree = link(r, manager + 0xf8, original.map(|old| old.tree))?;
    exact(r, view, base + 0x424dc38)?;
    exact(r, tree, base + 0x424de40)?;
    if r.u64(view + 0x2f0)? != idler
        || r.u64(tree + 0x2f0)? != idler
        || r.i32(tree + 0xe24)? != 6
        || r.i32(view + 0x300)? != 0
    {
        return Err(refused("Ordinary own Research transit views/tab required"));
    }
    let displayed = r.i32(tree + 0xe20)?;
    if displayed != 0 && displayed != r.i32(scope.own_status + 0x138)? {
        return Err(changed(
            "Foreign or unknown Research transit displayed country",
        ));
    }
    let panel = link(r, view + 0x2f8, original.map(|old| old.roots[0]))?;
    let tree_root = link(r, tree + 0x2f8, original.map(|old| old.roots[1]))?;
    // Validate known original windows before traversing their named membership.
    closing::settled_named(r, base, panel, registry, "research_panel")?;
    closing::settled_named(r, base, tree_root, registry, "research_tree")?;
    let facilities_parent = link(r, view + 0x910, original.map(|old| old.facilities_parent))?;
    closing::settled_named(r, base, facilities_parent, registry, "facilities_parent")?;
    let facilities = link(r, view + 0x900, original.map(|old| old.roots[2]))?;
    exact(r, facilities, base + 0x42f2708)?;
    if r.u64(facilities + 0x68)? != idler || r.u64(facilities + 0x60)? != facilities_parent {
        return Err(changed(
            "Original Research transit facilities ownership changed",
        ));
    }
    let facilities_root = link(r, facilities + 0x70, original.map(|old| old.roots[3]))?;
    closing::settled_named(r, base, facilities_root, registry, "facilities_view")?;
    let slots = members(
        r,
        scope.own_status + 0xa0,
        32,
        original.map(|old| &old.slots),
    )?;
    let templates = members(
        r,
        db + research::layout(r).database_templates,
        8192,
        original.map(|old| &old.templates),
    )?;
    let technologies = members(
        r,
        scope.own_status + 0x88,
        8192,
        original.map(|old| &old.technologies),
    )?;
    if slots.pointers.is_empty()
        || templates.pointers.is_empty()
        || technologies.pointers.len() != templates.pointers.len() - 1
    {
        return Err(refused("Complete original Research slots/catalog required"));
    }
    let roots = [panel, tree_root, facilities, facilities_root];
    let mut distinct = BTreeSet::new();
    for p in scope
        .anchors
        .iter()
        .chain([&scope.own_country, &scope.own_status])
        .chain([&registry, &manager, &view, &tree, &facilities_parent])
        .chain(roots.iter())
        .chain(slots.pointers.iter())
    {
        if !distinct.insert(*p) {
            return Err(refused(
                "Distinct own Research transit roots/slots required",
            ));
        }
    }
    Ok((
        Edges {
            registry,
            manager,
            view,
            tree,
            roots,
            facilities_parent,
            slots,
            templates,
            technologies,
        },
        mode,
    ))
}

// Active/completed membership may progress, but must still join the original
// own model BEFORE a current technology/template pointer is followed. These
// exact Linux fields are shared with research::Model, not inferred from macOS.
fn model_links(r: &Reader<'_>, base: u64, scope: &Scope, edges: &Edges) -> Result<()> {
    let mut current = BTreeSet::new();
    for slot in &edges.slots.pointers {
        exact(r, *slot, base + 0x4210898)?;
        if r.u64(slot + 0x10)? != scope.own_status {
            return Err(changed("Original Research transit slot owner changed"));
        }
        let technology = r.u64(slot + 0x18)?;
        if technology != 0 {
            current.insert(technology);
        }
    }
    let completed = members(r, scope.own_status + 0x40, 8192, None)?;
    current.extend(completed.pointers);
    let ordinals: BTreeMap<_, _> = edges
        .technologies
        .pointers
        .iter()
        .enumerate()
        .map(|(ordinal, pointer)| (*pointer, ordinal))
        .collect();
    for technology in current {
        let ordinal = *ordinals.get(&technology).ok_or_else(|| {
            changed("Research transit technology is outside original own membership")
        })?;
        exact(r, technology, base + 0x42107f8)?;
        if r.u64(technology + 0x158)? != scope.own_status {
            return Err(changed(
                "Original Research transit technology owner changed",
            ));
        }
        if r.u64(technology + 0x160)? != edges.templates.pointers[ordinal + 1] {
            return Err(changed("Research transit template changed before payload"));
        }
    }
    Ok(())
}

// Only visibility/type/owner facts are needed for navigation. Hidden retained
// Info technologies, callback controls and their template payload are not read.
fn no_visible_info(r: &Reader<'_>, base: u64, idler: u64, tree: u64, registry: u64) -> Result<()> {
    let mut unique = BTreeSet::new();
    let mut containers = BTreeSet::new();
    for info in array(r, tree + 0xdc0, 64)? {
        pointer(info)?;
        exact(r, info, base + 0x430bbf0)?;
        if !unique.insert(info) || r.u64(info + 0xa78)? != idler || r.u64(info + 0xa80)? != tree {
            return Err(changed("Unique own Research transit Info owner required"));
        }
        let container = r.u64(info + 0xa88)?;
        if container != 0 {
            pointer(container)?;
            if !containers.insert(container)
                || closing::settled_named(r, base, container, registry, "research_info")?
            {
                return Err(refused("No shown Research Info permitted during transit"));
            }
        }
    }
    Ok(())
}

/// Process-local original memory identity. No Clone/serde/raw-address export.
/// The bound cannot be renewed by an observation or a discarded torn read.
#[derive(Debug)]
pub(crate) struct Anchor {
    scope: Scope,
    base: u64,
    original: Edges,
    last_date: i32,
    until: Instant,
}
impl Anchor {
    /// Bind exactly one current settled Idle/Research observation. This avoids
    /// probing one assumed panel and rediscovering a different owner on retry.
    pub(crate) fn bind_current(
        process: &dyn Process,
        scope: &Scope,
        until: Instant,
    ) -> Result<(Self, Snapshot)> {
        Self::bind_mode(process, scope, None, until, &Instant::now)
    }
    pub(crate) fn bind(
        process: &dyn Process,
        scope: &Scope,
        source: ManagedPanel,
        until: Instant,
    ) -> Result<(Self, Snapshot)> {
        Self::bind_with_clock(process, scope, source, until, &Instant::now)
    }
    fn bind_with_clock(
        process: &dyn Process,
        scope: &Scope,
        source: ManagedPanel,
        until: Instant,
        now: &dyn Fn() -> Instant,
    ) -> Result<(Self, Snapshot)> {
        // A caller's unsupported source/ABI refuses before even a snapshot/read.
        let expected_mode = match source {
            ManagedPanel::Idle => 23,
            ManagedPanel::Research => 5,
            _ => {
                return Err(refused(
                    "Requested transit source needs its own ordinary departure",
                ))
            }
        };
        Self::bind_mode(process, scope, Some(expected_mode), until, now)
    }
    fn bind_mode(
        process: &dyn Process,
        scope: &Scope,
        expected_mode: Option<i32>,
        until: Instant,
        now: &dyn Fn() -> Instant,
    ) -> Result<(Self, Snapshot)> {
        boundary(process, scope, None, until, now)?;
        let base = process.image_base();
        let (original, snapshot) = sample(
            process,
            scope,
            base,
            None,
            expected_mode,
            None,
            false,
            until,
            now,
        )?;
        let last_date = snapshot.context().date_raw;
        Ok((
            Self {
                scope: scope.clone(),
                base,
                original,
                last_date,
                until,
            },
            snapshot,
        ))
    }
    pub(crate) fn observe(&mut self, process: &dyn Process) -> Result<Snapshot> {
        self.observe_with_clock(process, &Instant::now)
    }
    /// Same retained sample and final witness, additionally requiring the
    /// reviewed ordinary keyboard predicate. No input is sent by this read.
    pub(crate) fn observe_for_input(&mut self, process: &dyn Process) -> Result<Snapshot> {
        self.observe_mode(process, &Instant::now, true)
    }
    fn observe_with_clock(
        &mut self,
        process: &dyn Process,
        now: &dyn Fn() -> Instant,
    ) -> Result<Snapshot> {
        self.observe_mode(process, now, false)
    }
    fn observe_mode(
        &mut self,
        process: &dyn Process,
        now: &dyn Fn() -> Instant,
        keyboard: bool,
    ) -> Result<Snapshot> {
        boundary(process, &self.scope, Some(self.base), self.until, now)?;
        let (_, snapshot) = sample(
            process,
            &self.scope,
            self.base,
            Some(&self.original),
            None,
            Some(self.last_date),
            keyboard,
            self.until,
            now,
        )?;
        self.last_date = snapshot.context().date_raw;
        Ok(snapshot)
    }
    pub(crate) fn scope(&self) -> &Scope {
        &self.scope
    }
    pub(crate) fn deadline(&self) -> Instant {
        self.until
    }
}
fn boundary(
    process: &dyn Process,
    scope: &Scope,
    base: Option<u64>,
    until: Instant,
    now: &dyn Fn() -> Instant,
) -> Result<()> {
    if process.string_abi() != StringAbi::LibStdCxx {
        return Err(Error::new(
            "capability_unavailable",
            "Research transit requires reviewed Linux ABI",
        ));
    }
    deadline(until, now)?;
    scope.validate()?;
    if process.identity() != Some(&scope.epoch.game)
        || base.is_some_and(|old| old != process.image_base())
    {
        return Err(changed("Original Research transit process/image changed"));
    }
    let image = process.image_base();
    pointer(image)?;
    if image
        .checked_add(0x45778e8)
        .is_none_or(|end| end > 0x7fff_ffff_0000)
    {
        return Err(refused("Research transit image range exceeds bound"));
    }
    deadline(until, now)
}
#[allow(clippy::too_many_arguments)]
fn sample(
    process: &dyn Process,
    scope: &Scope,
    base: u64,
    original: Option<&Edges>,
    expected_mode: Option<i32>,
    last_date: Option<i32>,
    keyboard: bool,
    until: Instant,
    now: &dyn Fn() -> Instant,
) -> Result<(Edges, Snapshot)> {
    let timed = Timed {
        inner: process,
        until,
        now,
    };
    timed.begin_snapshot()?;
    let witnesses = Witnesses::new(&timed);
    let r = Reader::new(&witnesses);
    witnesses.section("transit_original_own");
    crate::daily_observation::own_scope(&r, base, scope)?;
    witnesses.section("transit_original_edges");
    let (edges, mode) = read_edges(&r, base, scope, original)?;
    if expected_mode.is_some_and(|expected| expected != mode) {
        return Err(refused(
            "Requested Research transit source differs from observed mode",
        ));
    }
    witnesses.section("transit_canonical_research");
    model_links(&r, base, scope, &edges)?;
    let (own, researching) = research::navigation_from_reader(&r, base)?;
    if own.own != [scope.own_country, scope.own_status] || own.slots != edges.slots.pointers {
        return Err(changed("Research transit canonical owner/slots changed"));
    }
    if last_date.is_some_and(|old| own.campaign.date_raw < old) {
        return Err(changed("Research transit chronology regressed"));
    }
    witnesses.section("transit_owned_ui");
    let (roots, parent, shown) = closing::roots(
        &r,
        base,
        edges.view,
        edges.tree,
        scope.anchors[1],
        edges.registry,
    )?;
    if roots != edges.roots || parent != edges.facilities_parent {
        return Err(changed("Original Research transit named roots changed"));
    }
    if shown != (mode == 5) {
        return Err(Error::new(
            "research_panel_transition",
            "Research transit mode/literal visibility not settled",
        ));
    }
    no_visible_info(&r, base, scope.anchors[1], edges.tree, edges.registry)?;
    let selected = r.u64(edges.tree + 0xe08)?;
    let selected_slot = if selected == 0 {
        None
    } else {
        Some(
            edges
                .slots
                .pointers
                .iter()
                .position(|p| *p == selected)
                .ok_or_else(|| {
                    changed("Research transit selection is outside original own slots")
                })? as u32,
        )
    };
    let (folder_index, folder_windows, selected_folder) = crate::category_ui::closing_selection(
        &r,
        base,
        edges.tree,
        edges.roots[1],
        edges.registry,
    )?;
    let context = UiContext {
        campaign: own.campaign,
        anchors: [
            scope.own_country,
            scope.own_status,
            edges.registry,
            edges.manager,
            edges.view,
            edges.tree,
        ],
        slots: own.slots,
    };
    let state = OwnState {
        epoch: scope.epoch.clone(),
        selected_slot,
        selected_folder,
        tree_visible: false,
        panel_visible: Some(shown),
        active_mode: Some(mode),
        researching,
        visible_info: Vec::new(),
        panel_binding: Some(PanelBinding {
            anchors: context.anchors,
            slots: context.slots.clone(),
            roots: edges.roots,
            facilities_parent: edges.facilities_parent,
            selected_slot_pointer: selected,
            folder_index,
            folder_windows,
            tab: 0,
        }),
    };
    let snapshot = Snapshot::from_observed(scope, state, &context)?;
    if keyboard {
        witnesses.section("transit_keyboard");
        keyboard_clear(&r, base, scope, &edges, shown)?;
    }
    witnesses.finish()?;
    timed.check()?;
    boundary(process, scope, Some(base), until, now)?;
    Ok((edges, snapshot))
}

fn keyboard_clear(r: &Reader<'_>, base: u64, scope: &Scope, e: &Edges, shown: bool) -> Result<()> {
    // Same reviewed predicate as ordinary Research closing, in this original
    // Anchor's bounded witness. Discovery of another target cannot replace it.
    let [gs, idler, _] = scope.anchors;
    let engine = crate::production_panel::engine(
        r,
        base,
        gs,
        idler,
        e.manager,
        e.registry,
        &mut Vec::new(),
    )?;
    let common = crate::construction::controls::blockers(
        r,
        base,
        scope.own_country,
        idler,
        e.manager,
        e.registry,
    )?;
    let gui_clear = r.byte(e.registry + 0x150)? == 0
        && r.byte(e.registry + 0x12)? == 0
        && r.u64(e.registry + 0x158)? == 0
        && r.u64(e.registry + 0x230)? == 0;
    let common_clear = common["available"] == true
        && common["politics_root"]["settled_visible"] == false
        && common["focus_root_settled_hidden"] == true
        && common["research_panel_settled_visible"] == shown
        && common["research_tree_settled_visible"] == false
        && common["focus_detail_visible_animating_counts"] == serde_json::json!([0, 0])
        && common["own_research_info_visible_animating_counts"] == serde_json::json!([0, 0]);
    if engine["conservative_engine_clear"] != true || !common_clear || !gui_clear {
        return Err(refused(
            "Ordinary Research transit keyboard/capture/window gates refused",
        ));
    }
    Ok(())
}

mod animation;

#[cfg(test)]
mod tests;
