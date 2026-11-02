//! Immutable original selection/queue evidence. Readback does not revisit the
//! old panel, choice controls, callback, renderer or route cache. All decisions
//! use typed witnessed facts; private journal JSON is never parsed as authority.
use super::{catalog, exact, location, Binding, Observation, Selection};
use crate::{
    construction::{self, controls::validate_key, members, QueueSummary},
    economy,
    error::{Error, Result},
    game::{self, ClockContext},
    memory::{Reader, StringAbi},
    platform::Process,
    research_ui::Witnesses,
};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};

// Prospective readback foundation only; no placement adapter is registered.
#[allow(dead_code)]
#[path = "placement_state.rs"]
mod placement_state;
#[allow(unused_imports)]
pub(crate) use placement_state::{
    PlacementAnchor, PlacementDelta, PlacementProof, PlacementReadback,
};

// Readback only. A future ordinary Escape attempt must retain this original
// scope and complete queue; this adds no key transport or command permission.
#[allow(dead_code)]
mod clearing_state;
#[allow(unused_imports)]
pub(crate) use clearing_state::{ClearSelectionAnchor, ClearSelectionReadback, ClearedProof};

#[cfg(test)]
#[path = "../../../tests/support/mac_construction_selection_state.rs"]
mod tests;

#[derive(Clone, Debug)]
pub(crate) struct SelectionState {
    context: ClockContext,
    binding: Binding,
    selection: Selection,
}

#[derive(Clone, Debug)]
pub(crate) struct SelectionAnchor {
    original: SelectionState,
}

/// A witnessed inactive-to-requested-selection transition. It retains both
/// exact queues/scopes; this is never a fresh baseline or input authority.
#[derive(Clone, Debug)]
pub(crate) struct SelectedProof {
    original: SelectionAnchor,
    selected: SelectionAnchor,
}
// Prospective composite handoff; existing primitive actors remain unchanged.
#[allow(dead_code)]
impl SelectedProof {
    pub(crate) fn original_anchor(&self) -> &SelectionAnchor {
        &self.original
    }
    pub(crate) fn selected_anchor(&self) -> &SelectionAnchor {
        &self.selected
    }
    pub(crate) fn context(&self) -> &ClockContext {
        self.selected.context()
    }
}

fn changed(message: &str) -> Error {
    Error::new("construction_state_changed", message)
}

fn supported(selection: &Selection) -> Result<()> {
    if matches!(selection, Selection::Unsupported(_)) {
        return Err(Error::new(
            "construction_selection_unavailable",
            "Only inactive or canonical ordinary construction selection is supported",
        ));
    }
    Ok(())
}

fn complete_queue(binding: &Binding, summary: &QueueSummary) -> Result<()> {
    if !summary.complete
        || binding.queue_members.len() != binding.queue_lines.len()
        || binding.queue_lines.iter().any(|row| {
            !row.building
                || row.key.is_none()
                || row.building_identity.is_none()
                || row.location.is_none()
        })
    {
        return Err(Error::new(
            "construction_queue_unavailable",
            "Complete canonical own building queue required for selection evidence",
        ));
    }
    Ok(())
}

fn same_context(before: &ClockContext, now: &ClockContext) -> Result<()> {
    if before.anchors != now.anchors
        || before.player != now.player
        || before.playthrough_id != now.playthrough_id
        || before.difficulty != now.difficulty
        || before.historical_raw != now.historical_raw
        || now.date_raw < before.date_raw
    {
        return Err(changed(
            "Construction campaign, player, roots, setup or chronology changed",
        ));
    }
    Ok(())
}

impl SelectionState {
    #[cfg(test)]
    pub(crate) fn context(&self) -> &ClockContext {
        &self.context
    }
}

impl SelectionAnchor {
    pub(crate) fn context(&self) -> &ClockContext {
        &self.original.context
    }
    /// Reuse the original ownership/member guard within another observer's
    /// Witnesses before it reads any fresh campaign or replacement payload.
    /// The caller must finish that same bracket and check final process identity.
    pub(crate) fn require_original_edges(
        &self,
        r: &Reader<'_>,
        base: u64,
        identity: &crate::platform::Identity,
    ) -> Result<()> {
        if r.string_abi() != StringAbi::LibCxx {
            return Err(Error::new(
                "unsupported_platform",
                "Mac selection layout required",
            ));
        }
        let b = &self.original.binding;
        if base != b.base || identity != &b.process {
            return Err(changed(
                "Original selection process changed before ownership reads",
            ));
        }
        original_edges(r, self)
    }
    /// Find navigation must not run while a building placement mode is active.
    pub(crate) fn require_inactive(&self) -> Result<()> {
        if self.original.selection != Selection::Inactive {
            return Err(Error::new(
                "construction_selection_unavailable",
                "Find navigation requires inactive construction selection",
            ));
        }
        Ok(())
    }
    /// Preserve the entire original queue and selection across navigation.
    pub(crate) fn require_unchanged(&self, now: &SelectionState) -> Result<()> {
        self.same_scope_and_queue(now)?;
        if self.original.selection != now.selection {
            return Err(changed(
                "Construction selection changed during Find navigation",
            ));
        }
        Ok(())
    }
    /// Observe the original inactive scope and complete queue once. Inactive
    /// is pending; another selected template is a refusal, never a new target.
    /// The caller retains the same attempt/Fence; this grants no input.
    #[allow(dead_code)] // Prospective typed composite only, not yet registered.
    pub(crate) fn observe_selected(&self, process: &dyn Process) -> Result<Option<SelectedProof>> {
        self.require_inactive()?;
        let now = self.observe(process)?;
        self.same_scope_and_queue(&now)?;
        if now.selection == Selection::Inactive {
            return Ok(None);
        }
        if !self.is_selected(&now) {
            return Err(changed(
                "Construction selected another target during selection",
            ));
        }
        Ok(Some(SelectedProof {
            original: self.clone(),
            selected: SelectionAnchor { original: now },
        }))
    }
    pub(crate) fn initially_selected(&self) -> bool {
        matches!(&self.original.selection, Selection::Ordinary(t) if *t == self.original.binding.target)
    }

    pub(crate) fn observe(&self, process: &dyn Process) -> Result<SelectionState> {
        canonical(
            process,
            &self.original.binding.target.key,
            self.original.binding.state_id,
            Some(self),
        )
    }

    pub(crate) fn same_scope_and_queue(&self, state: &SelectionState) -> Result<()> {
        same_context(&self.original.context, &state.context)?;
        if self.original.binding != state.binding {
            return Err(changed(
                "Original construction target, own scope, catalog or complete queue changed",
            ));
        }
        Ok(())
    }

    pub(crate) fn is_selected(&self, state: &SelectionState) -> bool {
        self.same_scope_and_queue(state).is_ok()
            && matches!(&state.selection, Selection::Ordinary(t) if *t == self.original.binding.target)
    }

    pub(crate) fn require_same_initial(&self, other: &Self) -> Result<()> {
        self.same_scope_and_queue(&other.original)?;
        if self.original.selection != other.original.selection {
            return Err(changed("Construction selection changed before input"));
        }
        Ok(())
    }

    /// Bounded private journal evidence. Complete original vectors remain in
    /// memory; their typed-field fingerprint cannot authorize a later replay.
    pub(crate) fn private_baseline(&self) -> Result<Value> {
        let b = &self.original.binding;
        let scope = json!({"game":b.process,"base":b.base,"anchors":self.original.context.anchors,
            "own_tag":b.own_tag,"country":b.country,"production":b.production,
            "state":b.state,"state_id":b.state_id,"registry":b.registry,"manager":b.manager,
            "database":b.catalog.database,
            "target":{"pointer":b.target.address,"key":b.target.key,"ordinal":b.target.ordinal}});
        let selection = match &self.original.selection {
            Selection::Inactive => json!({"mode":0}),
            Selection::Ordinary(t) => {
                json!({"mode":1,"pointer":t.address,"key":t.key,"ordinal":t.ordinal})
            }
            Selection::Unsupported(_) => {
                return Err(changed(
                    "Unsupported selection cannot enter a private baseline",
                ));
            }
        };
        let catalog: Vec<_> = b
            .catalog
            .templates
            .values()
            .map(|t| (&t.address, &t.ordinal, &t.key))
            .collect();
        let queue: Vec<_> = b
            .queue_lines
            .iter()
            .map(|row| {
                json!({"pointer":row.address,"handle":row.handle,"building":row.building,
                "key":row.key,"building_identity":row.building_identity,"amount":row.amount,
                "location":row.location.as_ref().map(|l|(l.state,l.state_id,l.province,l.status))})
            })
            .collect();
        let evidence = json!({"scope":scope,"context":self.original.context,"selection":selection,
            "catalog_members":b.catalog.members,"catalog":catalog,
            "queue_members":b.queue_members,"queue":queue});
        let bytes = serde_json::to_vec(&evidence)
            .map_err(|e| changed(&format!("Selection baseline encoding failed: {e}")))?;
        let baseline = json!({"version":1,"scope":scope,"context":self.original.context,
            "initial_selection":selection,"catalog_member_count":b.catalog.members.len(),
            "queue_member_count":b.queue_members.len(),"binding_bytes":bytes.len(),
            "binding_sha256":format!("{:x}",Sha256::digest(&bytes))});
        if serde_json::to_vec(&baseline)
            .map_err(|e| changed(&e.to_string()))?
            .len()
            > 8192
        {
            return Err(changed(
                "Compact selection baseline exceeds private record budget",
            ));
        }
        Ok(baseline)
    }
}

impl Observation {
    pub(crate) fn selection_anchor(&self) -> Result<SelectionAnchor> {
        supported(&self.selection)?;
        complete_queue(&self.binding, &self.queue)?;
        Ok(SelectionAnchor {
            original: SelectionState {
                context: self.campaign.clone(),
                binding: self.binding.clone(),
                selection: self.selection.clone(),
            },
        })
    }

    /// Canonical pre-input comparison only. Current control/geometry/route
    /// readiness is checked separately, allowing legitimate hover changes.
    pub(crate) fn selection_unchanged(&self, now: &Self) -> Result<()> {
        let anchor = self.selection_anchor()?;
        let current = now.selection_anchor()?;
        anchor.require_same_initial(&current)
    }
}

/// Initial canonical-only read permits an exact already-selected zero-input
/// result even if the old panel/choice/renderer payload has disappeared.
pub(crate) fn bind_selection(
    process: &dyn Process,
    key: &str,
    state_id: i32,
) -> Result<SelectionAnchor> {
    Ok(SelectionAnchor {
        original: canonical(process, key, state_id, None)?,
    })
}

fn entry(r: &Reader<'_>, header: u64, index: usize, stride: usize) -> Result<Vec<u8>> {
    let (bytes, data, _) = r.array_header(header)?;
    if i32::from_le_bytes(bytes[8..12].try_into().unwrap()) > 4096 || data % stride as u64 != 0 {
        return Err(changed(
            "Own construction country map exceeds alignment/capacity policy",
        ));
    }
    r.array_entry(header, index, stride)
}

/// Guard original pointer edges before any replacement payload. Repeated reads
/// through the same Witnesses reject a mid-observation edge change immediately.
fn original_non_queue_edges(r: &Reader<'_>, anchor: &SelectionAnchor) -> Result<()> {
    let b = &anchor.original.binding;
    let g = game::layout(r);
    let l = construction::layout(r);
    let roots = [
        r.u64(b.base + g.gs_global)?,
        r.u64(b.base + g.idler_global)?,
        r.u64(b.base + g.technology_db_global)?,
    ];
    if roots != anchor.original.context.anchors {
        return Err(changed("Construction game roots changed before payload"));
    }
    let [gs, idler, _] = roots;
    exact(r, b.base, gs, g.gs_vtable)?;
    exact(r, b.base, idler, g.idler_vtable)?;
    let tag = r.i32(gs + g.player_tag)?;
    if tag != b.own_tag || !(1..4096).contains(&tag) {
        return Err(changed("Construction player changed before own payload"));
    }
    let index = i32::from_le_bytes(
        entry(r, gs + g.tag_country_index, tag as usize, 4)?
            .try_into()
            .unwrap(),
    );
    if index < 0
        || u64::from_le_bytes(
            entry(r, gs + g.countries, index as usize, 8)?
                .try_into()
                .unwrap(),
        ) != b.country
    {
        return Err(changed(
            "Construction own-country map changed before payload",
        ));
    }
    exact(r, b.base, b.country, g.country_vtable)?;
    if r.i32(b.country + 8)? != b.own_tag {
        return Err(changed(
            "Construction country tag changed before production payload",
        ));
    }
    if r.u64(b.country + g.country_production)? != b.production {
        return Err(changed("Construction production changed before payload"));
    }
    exact(r, b.base, b.production, g.production_vtable)?;
    if r.u64(b.production + crate::game::layout(r).production(0x290))? != b.country {
        return Err(changed(
            "Construction production owner changed before queue payload",
        ));
    }
    if r.u64(idler + 0x4c0)? != b.registry || r.u64(idler + 0x670)? != b.manager {
        return Err(changed("Construction own interface changed before payload"));
    }
    exact(r, b.base, b.registry, 0x36a5810)?;
    if r.u64(b.manager + 0x180)? != idler {
        return Err(changed("Construction manager owner changed before payload"));
    }
    if r.u64(b.base + l.database_global)? != b.catalog.database {
        return Err(changed("Construction database changed before payload"));
    }
    if location::catalog_pointer(r, gs + l.gs_states, b.state_id, 4096)? != b.state {
        return Err(changed("Construction state mapping changed before payload"));
    }
    if !location::own_state(r, b.base, b.state, b.own_tag)? {
        return Err(Error::new(
            "construction_target_unavailable",
            "Requested state must remain owned and controlled by the current player",
        ));
    }
    exact(r, b.base, b.catalog.database, l.database_vtable)?;
    if members(r, b.catalog.database + 0x40, 4096)? != b.catalog.members {
        return Err(changed(
            "Construction catalog membership changed before payload",
        ));
    }
    Ok(())
}

fn original_edges(r: &Reader<'_>, anchor: &SelectionAnchor) -> Result<()> {
    original_non_queue_edges(r, anchor)?;
    let b = &anchor.original.binding;
    if members(r, b.production + 0x70, construction::LINE_LIMIT)? != b.queue_members {
        return Err(changed(
            "Construction queue membership changed before payload",
        ));
    }
    Ok(())
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum QueueRead {
    Unchanged,
    Placement,
}

fn canonical(
    process: &dyn Process,
    key: &str,
    state_id: i32,
    expected: Option<&SelectionAnchor>,
) -> Result<SelectionState> {
    canonical_with_queue(process, key, state_id, expected, QueueRead::Unchanged)
}

fn canonical_with_queue(
    process: &dyn Process,
    key: &str,
    state_id: i32,
    expected: Option<&SelectionAnchor>,
    queue_read: QueueRead,
) -> Result<SelectionState> {
    validate_key(key)?;
    construction::find_view::validate_state_id(i64::from(state_id))?;
    if process.string_abi() != StringAbi::LibCxx {
        return Err(Error::new(
            "unsupported_platform",
            "Selection evidence requires the verified Mac layout",
        ));
    }
    if key == "rail_way" {
        return Err(Error::new(
            "capability_unavailable",
            "Special construction selection remains unsupported",
        ));
    }
    process.begin_snapshot()?;
    let identity = process.identity().cloned().ok_or_else(|| {
        Error::new(
            "ownership_unavailable",
            "Exact selection process identity required",
        )
    })?;
    let base = process.image_base();
    if expected
        .is_some_and(|a| a.original.binding.process != identity || a.original.binding.base != base)
    {
        return Err(Error::new(
            "ownership_changed",
            "Original selection process or image changed",
        ));
    }
    let w = Witnesses::new(process);
    let r = Reader::new(&w);
    w.section("mac_construction_selection_original_edges");
    if let Some(anchor) = expected {
        match queue_read {
            QueueRead::Unchanged => original_edges(&r, anchor)?,
            QueueRead::Placement => original_non_queue_edges(&r, anchor)?,
        }
    }
    w.section("mac_construction_selection_owner");
    let (country, production) = economy::own_production(&r, base)?;
    let context = game::context_from_reader(&r, base)?;
    if let Some(anchor) = expected {
        same_context(&anchor.original.context, &context)?;
    }
    let [gs, idler, _] = context.anchors;
    let own_tag = r.i32(country + 8)?;
    let l = construction::layout(&r);
    w.section("mac_construction_selection_state");
    let state = location::catalog_pointer(&r, gs + l.gs_states, state_id, 4096)?;
    if !location::own_state(&r, base, state, own_tag)? {
        return Err(Error::new(
            "construction_target_unavailable",
            "Requested state must remain owned and controlled by the current player",
        ));
    }
    if r.i32(state + l.state_id)? != state_id {
        return Err(changed("Requested selection state ID/catalog mismatch"));
    }
    w.section("mac_construction_selection_catalog");
    let catalog = catalog::observe(&r, base)?;
    let target = catalog.required(key)?.clone();
    if expected.is_some_and(|a| {
        a.original.binding.catalog != catalog || a.original.binding.target != target
    }) {
        return Err(changed(
            "Original construction catalog or canonical target changed",
        ));
    }
    w.section("mac_construction_selection_interface");
    let registry = r.u64(idler + 0x4c0)?;
    exact(&r, base, registry, 0x36a5810)?;
    let manager = r.u64(idler + 0x670)?;
    construction::pointer(manager)?;
    if r.u64(manager + 0x180)? != idler {
        return Err(changed(
            "Construction manager no longer belongs to own idler",
        ));
    }
    w.section("mac_construction_selection_mode");
    let selection = super::selection(&r, manager, &catalog)?;
    supported(&selection)?;
    if queue_read == QueueRead::Placement
        && expected.is_none_or(|anchor| selection != anchor.original.selection)
    {
        return Err(changed(
            "Original exact ordinary selection required before replacement queue payload",
        ));
    }
    w.section("mac_construction_selection_queue");
    let lines = construction::lines_in(&r, base, production, own_tag, true)?;
    let summary = construction::queue_summary(&lines, key, state_id);
    let binding = Binding {
        process: identity,
        base,
        own_tag,
        country,
        production,
        state,
        state_id,
        registry,
        manager,
        catalog,
        target,
        queue_members: lines.members,
        queue_lines: lines.known,
    };
    complete_queue(&binding, &summary)?;
    w.finish()?;
    process.check()?;
    if process.identity() != Some(&binding.process) || process.image_base() != binding.base {
        return Err(Error::new(
            "ownership_changed",
            "Selection process identity changed during observation",
        ));
    }
    Ok(SelectionState {
        context,
        binding,
        selection,
    })
}
