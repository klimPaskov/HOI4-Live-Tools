//! Bounded read-only own focus model for the verified Linux build.
//! UI/catalog layouts independently recovered; see native/PROVENANCE.md.
//! Typed/string/array ideas consulted: smerss/Clausewitz-MCP, MIT,
//! Copyright (c) 2026 smerss. See ../LICENSE.clausewitz.
use crate::{
    error::{Error, Result},
    game,
    memory::{ReadMemory, Reader},
    platform::Process,
};
use serde_json::{json, Value};
use std::{
    cell::{Cell, RefCell},
    collections::{BTreeMap, BTreeSet},
    time::{Instant, SystemTime, UNIX_EPOCH},
};
mod completion;
mod search;
pub use completion::snapshot as completion_snapshot;

struct Witnesses<'a> {
    process: &'a dyn Process,
    reads: RefCell<BTreeMap<(u64, usize), WitnessRead>>,
    section: Cell<&'static str>,
    regions: RefCell<BTreeMap<u64, (u64, &'static str)>>,
}
struct WitnessRead {
    bytes: Vec<u8>,
    section: &'static str,
}
impl ReadMemory for Witnesses<'_> {
    fn read(&self, a: u64, n: usize) -> Result<Vec<u8>> {
        let bytes = self.process.read(a, n)?;
        let mut reads = self.reads.borrow_mut();
        if let Some(old) = reads.get(&(a, n)) {
            if old.bytes != bytes {
                return Err(self.changed("observation", a, n, old.section));
            }
        } else {
            reads.insert(
                (a, n),
                WitnessRead {
                    bytes: bytes.clone(),
                    section: self.section.get(),
                },
            );
        }
        Ok(bytes)
    }
}
impl Witnesses<'_> {
    // Diagnostics only: bounded known object extents label existing reads.
    // They never trigger reads, loosen a witness or confer ownership. Messages
    // contain static labels and relative offsets, never addresses or values.
    fn region(&self, address: u64, size: u64, label: &'static str) {
        if let Some(end) = address.checked_add(size) {
            self.regions.borrow_mut().insert(address, (end, label));
        }
    }
    fn changed(&self, phase: &str, address: u64, size: usize, section: &str) -> Error {
        let regions = self.regions.borrow();
        let object = regions.range(..=address).rev().find(|(_, (end, _))| {
            address
                .checked_add(size as u64)
                .is_some_and(|last| last <= *end)
        });
        let field = match object {
            Some((start, (_, label))) => format!("{label}+0x{:x} size={size}", address - start),
            None => format!("unclassified size={size}"),
        };
        Error::new(
            "torn_read",
            format!("Focus {phase} changed: {section}; {field}"),
        )
    }
    fn finish(&self) -> Result<(usize, usize)> {
        let reads = self.reads.borrow();
        let mut bytes = 0;
        for ((a, n), old) in reads.iter() {
            if self.process.read(*a, *n)? != old.bytes {
                return Err(self.changed("witness", *a, *n, old.section));
            }
            bytes += n;
        }
        Ok((reads.len(), bytes))
    }
}
fn exact(r: &Reader<'_>, p: u64, table: u64) -> Result<()> {
    if p == 0 || p % 8 != 0 || r.u64(p)? != table {
        return Err(Error::new(
            "invalid_focus_type",
            "Exact primary focus/UI type required",
        ));
    }
    Ok(())
}
fn pointers(r: &Reader<'_>, a: u64, bound: usize) -> Result<Vec<u64>> {
    let (h, data, n) = r.array_header(a)?;
    let cap = i32::from_le_bytes(h[8..12].try_into().unwrap()) as usize;
    if cap > bound || (n > 0 && data % 8 != 0) {
        return Err(Error::new(
            "invalid_array",
            "Focus array exceeds bounds/alignment",
        ));
    }
    let bytes = if n == 0 {
        Vec::new()
    } else {
        r.read(data, n * 8)?
    };
    Ok(bytes
        .chunks_exact(8)
        .map(|x| u64::from_le_bytes(x.try_into().unwrap()))
        .collect())
}
fn unique(values: &[u64]) -> Result<BTreeSet<u64>> {
    let set: BTreeSet<_> = values.iter().copied().collect();
    if set.len() != values.len() || set.contains(&0) {
        return Err(Error::new(
            "invalid_focus_catalog",
            "Unique nonnull canonical pointers required",
        ));
    }
    Ok(set)
}
#[allow(clippy::too_many_arguments)]
fn named_start(
    r: &Reader<'_>,
    witnesses: &Witnesses<'_>,
    base: u64,
    window: u64,
    panel: u64,
    registry: u64,
    selected_key: &str,
    mode: (bool, bool),
) -> Result<Value> {
    let (visible, include_geometry) = mode;
    // Exact CContainerWindow virtual88 uses these paired registries.
    let children = pointers(r, window + 0x9a0, 256)?;
    let (h, data, n) = r.array_header(window + 0x9b8)?;
    let cap = i32::from_le_bytes(h[8..12].try_into().unwrap()) as usize;
    if cap > 256 || n != children.len() || (n > 0 && data % 8 != 0) {
        return Err(Error::new(
            "invalid_control_binding",
            "Paired focus button registry required",
        ));
    }
    let mut start = None;
    let mut names = BTreeSet::new();
    for (i, child) in children.iter().enumerate() {
        let name = r.string_bounded(data + i as u64 * 32, 256)?;
        if !names.insert(name.clone()) {
            return Err(Error::new(
                "invalid_control_binding",
                "Duplicate button registry name",
            ));
        }
        if name == "start" {
            if start.is_some() {
                return Err(Error::new(
                    "invalid_control_binding",
                    "Duplicate start control",
                ));
            }
            exact(r, *child, base + 0x43470c8)?;
            witnesses.region(*child, 0xb0, "start_control");
            if r.u64(child + 0x68)? != registry {
                return Err(Error::new(
                    "invalid_focus_owner",
                    "Named start must belong to own registry before control payload",
                ));
            }
            let state = r.i32(child + 0xac)?;
            let flags = u32::from_le_bytes(r.read(child + 0x74, 4)?.try_into().unwrap());
            let binding = if visible {
                crate::focus_start_binding::observe(
                    r,
                    base,
                    panel,
                    *child,
                    registry,
                    selected_key,
                    include_geometry,
                )?
            } else {
                json!({"available":false,"reason":"detail_not_locally_visible","input_authorized":false})
            };
            start = Some(
                json!({"materialized":true,"disabled":state==3,"disabled_state_raw":state,"flags74_raw":flags,"ordinary_binding":binding}),
            );
        }
    }
    Ok(start.unwrap_or_else(|| json!({"materialized":false,"disabled":null})))
}
#[derive(Clone, Copy)]
enum FocusQuery<'a> {
    Node(&'a str),
    SelectedDetail(&'a str),
    Search(&'a str, bool),
}
impl<'a> FocusQuery<'a> {
    fn key(self) -> &'a str {
        match self {
            Self::Node(key) | Self::SelectedDetail(key) | Self::Search(key, _) => key,
        }
    }
}
/// Caller must open a Process with the verified build/session contract. This
/// module has no native call or input capability. Pagination bounds output only;
/// all keys/memberships are validated before a page can be returned.
fn snapshot_model(
    process: &dyn Process,
    offset: usize,
    limit: usize,
    prefix: Option<&str>,
    query: Option<FocusQuery<'_>>,
    include_geometry: bool,
    command_context: bool,
) -> Result<(Value, Option<FocusContext>)> {
    let requested = query.map(FocusQuery::key);
    if limit == 0
        || limit > 100
        || offset > 4096
        || prefix.is_some_and(|s| {
            s.len() > 128 || !s.bytes().all(|c| c.is_ascii_alphanumeric() || c == b'_')
        })
    {
        return Err(Error::new(
            "invalid_query",
            "Bounded focus pagination/filter required",
        ));
    }
    let start = Instant::now();
    process.begin_snapshot()?;
    let w = Witnesses {
        process,
        reads: RefCell::new(BTreeMap::new()),
        section: Cell::new("roots"),
        regions: RefCell::new(BTreeMap::new()),
    };
    let r = Reader::new(&w);
    let base = process.image_base();
    let gs = r.u64(base + 0x44086a8)?;
    let idler = r.u64(base + 0x44085f8)?;
    exact(&r, gs, base + 0x42c7598)?;
    exact(&r, idler, base + 0x42c9a88)?;
    w.region(gs, 0x520, "game_state");
    w.region(idler, 0x6c0, "idler");
    let tag = r.i32(gs + 0x518)?;
    if !(1..4096).contains(&tag) {
        return Err(Error::new(
            "player_unavailable",
            "Positive own tag required before payload reads",
        ));
    }
    let index = i32::from_le_bytes(
        r.array_entry(gs + 0x340, tag as usize, 4)?
            .try_into()
            .unwrap(),
    );
    if index < 0 {
        return Err(Error::new("player_unavailable", "Own country index absent"));
    }
    let country = u64::from_le_bytes(
        r.array_entry(gs + 0x310, index as usize, 8)?
            .try_into()
            .unwrap(),
    );
    r.object_type(country, "CCountry")?;
    w.region(country, 0x1350, "own_country");
    if r.i32(country + 8)? != tag {
        return Err(Error::new(
            "invalid_focus_owner",
            "Own country tag join required",
        ));
    }
    let registry = r.u64(idler + 0x4f8)?;
    // Match current native research reader's exact current-game GUI primary.
    exact(&r, registry, base + 0x42736f8)?;
    let manager = r.u64(idler + 0x6b8)?;
    w.region(manager, 0x188, "manager");
    if r.u64(manager + 0x180)? != idler {
        return Err(Error::new(
            "invalid_focus_owner",
            "Own interface manager required",
        ));
    }
    let view = r.u64(manager + 0x130)?;
    exact(&r, view, base + 0x4249c70)?;
    w.region(view, 0x11e0, "focus_view");
    let displayed = r.u64(view + 0x11d8)?;
    if r.u64(view + 0x2f0)? != idler || (displayed != 0 && displayed != country) {
        return Err(Error::new(
            "foreign_focus_scope",
            "Focus view must display own country before catalog payload",
        ));
    }
    let tree = r.u64(country + 0x1338)?;
    exact(&r, tree, base + 0x4202410)?;
    w.region(tree, 0x40, "own_tree");
    let db = r.u64(base + 0x439faf0)?;
    exact(&r, db, base + 0x4202590)?;
    w.region(db, 0x68, "catalog");
    w.section.set("catalog");
    let db_ptrs = pointers(&r, db + 0x58, 20000)?;
    // This global registry proves membership only. Its append path does not
    // guarantee uniqueness or exclude null entries; neither grants ownership.
    let canonical: BTreeSet<_> = db_ptrs.into_iter().filter(|p| *p != 0).collect();
    let focuses = pointers(&r, tree + 0x30, 4096)?;
    let own = unique(&focuses)?;
    let mut keys = BTreeMap::new();
    let mut key_set = BTreeSet::new();
    let mut output = Vec::new();
    for focus in focuses {
        if !canonical.contains(&focus) {
            return Err(Error::new(
                "invalid_focus_catalog",
                "Own focus absent from canonical database",
            ));
        }
        exact(&r, focus, base + 0x42024d0)?;
        if r.u64(focus + 0x160)? != tree {
            return Err(Error::new(
                "invalid_focus_owner",
                "Shared/null/foreign tree ownership is unverified",
            ));
        }
        let key = r.string_bounded(focus + 0x18, 256)?;
        if key.is_empty()
            || !key.bytes().all(|c| c.is_ascii_alphanumeric() || c == b'_')
            || !key_set.insert(key.clone())
        {
            return Err(Error::new(
                "invalid_focus_catalog",
                "Unique bounded canonical focus keys required",
            ));
        }
        keys.insert(focus, key.clone());
        if prefix.is_none_or(|filter| key.starts_with(filter)) {
            output.push(json!({"key":key}));
        }
    }
    let mut command_search = None;
    let mut search_anchors = None;
    if let Some(FocusQuery::Search(key, include_control)) = query {
        w.section.set("search_selection");
        w.region(view, 0x1380, "focus_view");
        let mut search = search::observe(&r, base, view, country, &keys, key)?;
        if include_control && search["available"] == true {
            w.section.set("search_control");
            let (control, binding) = search::control(&r, base, view, registry, &w)?;
            search["query_control"] = control;
            search_anchors = binding;
            search["query_text_available"] = search["query_control"]["available"].clone();
            search["query_control_binding_available"] =
                search["query_control"]["available"].clone();
        }
        if command_context {
            command_search = Some(search);
        } else {
            let (end_calls, end_bytes) = w.finish()?;
            process.check()?;
            return Ok((
                json!({
                    "source":"native own Focus search selection","build_id":game::BUILD_ID,
                    "owned":true,"search":search,"input_authorized":false,
                    "observation_paused_game":false,
                    "freshness":{"observed_unix_ms":SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_millis(),
                        "elapsed_ms":start.elapsed().as_secs_f64()*1000.0,"read_calls":r.calls.get()+end_calls as u64,"read_bytes":r.bytes.get()+end_bytes}
                }),
                None,
            ));
        }
    }
    if command_context && requested.is_some_and(|key| !keys.values().any(|value| value == key)) {
        return Err(Error::new(
            "focus_unavailable",
            "Exact own canonical focus required",
        ));
    }
    let mut context = if command_context {
        w.section.set("own_context");
        let progress = r.u64(country + 0x1348)?;
        exact(&r, progress, base + 0x4202638)?;
        w.region(progress, 0x18, "own_progress");
        if r.u64(progress + 8)? != country {
            return Err(Error::new(
                "invalid_focus_owner",
                "Own focus progress required before active payload",
            ));
        }
        let active = r.u64(progress + 0x10)?;
        let active_key = if active == 0 {
            None
        } else {
            Some(keys.get(&active).cloned().ok_or_else(|| {
                Error::new(
                    "invalid_focus_owner",
                    "Active focus outside own canonical catalog",
                )
            })?)
        };
        let status = r.u64(country + 0xf38)?;
        exact(&r, status, base + 0x42108f8)?;
        w.region(status, 0x13c, "own_status");
        if r.u64(status + 0x130)? != country || r.i32(status + 0x138)? != tag {
            return Err(Error::new(
                "invalid_focus_owner",
                "Own status context required",
            ));
        }
        let slots = pointers(&r, status + 0xa0, 32)?;
        // Reject aliased scope anchors before dereferencing any slot payload.
        let slot_set: BTreeSet<_> = slots.iter().copied().collect();
        if slot_set.len() != slots.len() {
            return Err(Error::new(
                "invalid_focus_context",
                "Duplicate own slot pointers",
            ));
        }
        for slot in &slots {
            exact(&r, *slot, base + 0x4210898)?;
            if r.u64(slot + 0x10)? != status {
                return Err(Error::new(
                    "invalid_focus_owner",
                    "Own slot context required",
                ));
            }
        }
        let campaign = game::context_from_reader(&r, base)?;
        if campaign.anchors[..2] != [gs, idler] {
            return Err(Error::new("torn_read", "Focus campaign roots contradicted"));
        }
        Some(FocusContext {
            campaign,
            anchors: [country, progress, tree, view, registry, manager, status],
            slots,
            active_focus: active,
            active_key,
            selected: vec![],
            matching_details: vec![],
            ui_facts: None,
            search_anchors,
        })
    } else {
        None
    };
    w.section.set("node_binding");
    let node_association = if let Some(FocusQuery::Node(key)) = query {
        Some(crate::focus_node_binding::observe(
            &r,
            base,
            view,
            registry,
            &keys,
            key,
            include_geometry,
        )?)
    } else {
        None
    };
    w.section.set("detail");
    let panels = pointers(&r, view + 0x1180, 32)?;
    unique(&panels)?;
    let mut details = Vec::new();
    for panel in panels {
        exact(&r, panel, base + 0x43023f8)?;
        w.region(panel, 0x7d8, "detail_panel");
        if r.u64(panel + 0x7b8)? != idler || r.u64(panel + 0x7c0)? != view {
            return Err(Error::new(
                "invalid_focus_owner",
                "Own focus detail joins required",
            ));
        }
        let selected = r.u64(panel + 0x7d0)?;
        if let Some(c) = context.as_mut() {
            c.selected.push([panel, selected]);
        }
        let key = if selected == 0 {
            None
        } else {
            if !own.contains(&selected) {
                return Err(Error::new(
                    "invalid_focus_catalog",
                    "Selected detail outside own canonical focus tree",
                ));
            }
            keys.get(&selected).cloned()
        };
        if requested.is_some_and(|target| key.as_deref() != Some(target)) {
            details.push(json!({"selected_key":key,"window_payload_read":false}));
            continue;
        }
        let window = r.u64(panel + 0x7c8)?;
        if let Some(c) = context.as_mut() {
            c.matching_details.push([panel, selected, window]);
        }
        let observed = if window == 0 {
            json!({"materialized":false,"detail_visibility_flag":null,"conservative_container_visibility":null,"start":{"materialized":false}})
        } else {
            exact(&r, window, base + 0x4347d58)?;
            w.region(window, 0xefc, "detail_window");
            if r.u64(window + 0x98)? != registry {
                return Err(Error::new(
                    "invalid_focus_owner",
                    "Own detail GUI registry required",
                ));
            }
            let visible = r.byte(window + 0xa5)? & 8 != 0;
            let animation_clear = r.i32(window + 0xef8)? != 2 && r.i32(window + 0xeb0)? != 2;
            let control = if selected == 0 {
                // Normal detail creation requires a selected focus. A retained
                // null selection supplies no binding for a start control.
                json!({"materialized":false,"binding":"unavailable: no selected focus"})
            } else {
                named_start(
                    &r,
                    &w,
                    base,
                    window,
                    panel,
                    registry,
                    key.as_deref().unwrap(),
                    (visible && animation_clear, include_geometry),
                )?
            };
            json!({"materialized":true,"detail_visibility_flag":visible,"conservative_container_visibility":visible&&animation_clear,"start":control})
        };
        details.push(json!({"selected_key":key,"window":observed}));
    }
    if let Some(c) = context.as_mut() {
        w.section.set("ui_facts");
        c.ui_facts = Some(crate::focus_ui_facts::observe(
            &r, base, idler, manager, view, registry, tag,
        )?);
    }
    if command_context
        && details
            .iter()
            .filter(|row| {
                row["selected_key"].as_str() == requested
                    && row["window"]["conservative_container_visibility"] == true
            })
            .count()
            > 1
    {
        return Err(Error::new(
            "ambiguous_focus_detail",
            "Multiple visible matching own focus details",
        ));
    }
    if let Some(c) = context.as_ref() {
        if r.u64(country + 0x1348)? != c.anchors[1]
            || r.u64(c.anchors[1] + 8)? != country
            || r.u64(c.anchors[1] + 0x10)? != c.active_focus
        {
            return Err(Error::new("torn_read", "Focus progress context changed"));
        }
    }
    let total = output.len();
    let page: Vec<_> = output.into_iter().skip(offset).take(limit).collect();
    let (end_calls, end_bytes) = w.finish()?;
    process.check()?;
    let mut body = json!({"source":"native own focus catalog and UI diagnostics","build_id":game::BUILD_ID,"owned":true,"catalog_count":own.len(),"filtered_count":total,"offset":offset,"limit":limit,"next_offset":if offset.saturating_add(page.len())<total {Some(offset+page.len())}else{None},"focuses":page,"details":details,"node_association":node_association,"numeric_ids_available":false,"eligibility_available":false,"input_authorized":false,"branch_visibility_available":false,"observation_paused_game":false,"freshness":{"observed_unix_ms":SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_millis(),"elapsed_ms":start.elapsed().as_secs_f64()*1000.0,"read_calls":r.calls.get()+end_calls as u64,"read_bytes":r.bytes.get()+end_bytes}});
    if let Some(c) = context.as_ref() {
        body["command_observation"] = json!({"active_key":c.active_key,"requested_key":requested,"same_reader_clock_context":true,"eligibility_available":false,"input_authorized":false});
    }
    if let Some(search) = command_search {
        body["search"] = search;
    }
    Ok((body, context))
}

pub fn snapshot(
    process: &dyn Process,
    offset: usize,
    limit: usize,
    prefix: Option<&str>,
) -> Result<Value> {
    Ok(snapshot_model(process, offset, limit, prefix, None, false, false)?.0)
}
/// Exact own public focus key, read-only current item/control association.
pub fn node_detail(process: &dyn Process, key: &str) -> Result<Value> {
    validate_key(key)?;
    Ok(snapshot_model(
        process,
        0,
        1,
        Some(key),
        Some(FocusQuery::Node(key)),
        false,
        false,
    )?
    .0)
}
pub(crate) fn validate_key(key: &str) -> Result<()> {
    if key.is_empty()
        || key.len() > 128
        || !key.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_')
    {
        return Err(Error::new(
            "invalid_argument",
            "Exact bounded focus key required",
        ));
    }
    Ok(())
}

/// Current search highlight joined through the complete bounded cache to own
/// current items and canonical keys. Does not read text, click or start a Focus.
pub fn search_state(process: &dyn Process, key: &str) -> Result<Value> {
    search_observation(process, key, false)
}

/// Opt-in query text and exact ordinary-control diagnostics. No input authority.
pub fn search_state_with_control(process: &dyn Process, key: &str) -> Result<Value> {
    search_observation(process, key, true)
}

fn search_observation(process: &dyn Process, key: &str, include_control: bool) -> Result<Value> {
    validate_key(key)?;
    Ok(snapshot_model(
        process,
        0,
        1,
        Some(key),
        Some(FocusQuery::Search(key, include_control)),
        false,
        false,
    )?
    .0)
}

/// Opt-in deep renderer diagnostics; default node/list readers remain metadata-only.
pub fn node_detail_with_geometry(
    process: &dyn Process,
    key: &str,
    include_geometry: bool,
) -> Result<Value> {
    validate_key(key)?;
    Ok(snapshot_model(
        process,
        0,
        1,
        Some(key),
        Some(FocusQuery::Node(key)),
        include_geometry,
        false,
    )?
    .0)
}

/// Compact selected-window observation. The node symbol is a separate control,
/// never the selected detail's Start binding. Reuses exactly one fresh reader
/// bracket with complete selected-detail ownership and field witnesses. Tree-node
/// controls belong to View and are deliberately outside this observation.
pub fn selected_detail(process: &dyn Process, key: &str, include_geometry: bool) -> Result<Value> {
    validate_key(key)?;
    let body = snapshot_model(
        process,
        0,
        1,
        Some(key),
        Some(FocusQuery::SelectedDetail(key)),
        include_geometry,
        false,
    )?
    .0;
    let details = body["details"].as_array().ok_or_else(|| {
        Error::new(
            "invalid_focus_detail",
            "Own selected-detail model unavailable",
        )
    })?;
    let mut selected = Vec::new();
    for row in details {
        match row.get("selected_key") {
            Some(Value::String(selected_key)) => selected.push((selected_key, row)),
            Some(Value::Null) => (),
            _ => {
                return Err(Error::new(
                    "invalid_focus_detail",
                    "Explicit selected-detail key required",
                ))
            }
        }
    }
    if selected.len() > 1 {
        return Err(Error::new(
            "ambiguous_focus_detail",
            "Multiple own selected focus details",
        ));
    }
    let (selected_key, row) = selected.first().ok_or_else(|| {
        Error::new(
            "focus_detail_unavailable",
            "No own focus detail is selected",
        )
    })?;
    if selected_key.as_str() != key {
        return Err(Error::new(
            "focus_detail_not_selected",
            "Requested own focus detail is not selected",
        ));
    }
    let window = &row["window"];
    match window["materialized"].as_bool() {
        Some(true) => (),
        Some(false) => {
            return Err(Error::new(
                "focus_detail_unavailable",
                "Own selected detail window is not materialized",
            ))
        }
        None => {
            return Err(Error::new(
                "invalid_focus_detail",
                "Explicit own selected-detail window required",
            ))
        }
    }
    // Preserve existing observed booleans/reasons and unknowns verbatim. A
    // missing Start/binding is never synthesized into a false route witness.
    Ok(
        json!({"source":"native own selected focus detail","build_id":body["build_id"],
        "scope":"own_selected_focus_detail_only","owned":body["owned"],
        "selected_key":selected_key,"selected_detail_count":selected.len(),"window":window,
        "include_geometry":include_geometry,"freshness":body["freshness"],
        "numeric_ids_available":body["numeric_ids_available"],
        "eligibility_available":body["eligibility_available"],
        "branch_visibility_available":body["branch_visibility_available"],
        "input_authorized":body["input_authorized"],"observation_paused_game":body["observation_paused_game"]}),
    )
}

#[derive(Clone, Debug, PartialEq, Eq)]
#[doc(hidden)]
pub struct FocusContext {
    pub campaign: game::ClockContext,
    /// Own country, progress, tree, view, registry, manager and research status.
    pub anchors: [u64; 7],
    pub slots: Vec<u64>,
    pub active_focus: u64,
    pub active_key: Option<String>,
    pub selected: Vec<[u64; 2]>,
    pub matching_details: Vec<[u64; 3]>,
    pub ui_facts: Option<crate::focus_ui_facts::FocusUiFacts>,
    /// Root/edit/renderer identities for the same-reader search command only.
    pub search_anchors: Option<[u64; 3]>,
}
/// Same witnessed own command/UI context plus exact search control. No input.
#[doc(hidden)]
pub fn command_search(process: &dyn Process, key: &str) -> Result<(Value, FocusContext)> {
    validate_key(key)?;
    let (body, context) = snapshot_model(
        process,
        0,
        1,
        Some(key),
        Some(FocusQuery::Search(key, true)),
        false,
        true,
    )?;
    Ok((body, context.expect("Search command context requested")))
}
/// Same witnessed reader for a future ordinary focus stage, never an action.
#[doc(hidden)]
pub fn command_owned(
    process: &dyn Process,
    key: &str,
    include_geometry: bool,
) -> Result<(Value, FocusContext)> {
    validate_key(key)?;
    let (body, context) = snapshot_model(
        process,
        0,
        1,
        Some(key),
        Some(FocusQuery::Node(key)),
        include_geometry,
        true,
    )?;
    Ok((body, context.expect("Command focus context requested")))
}

/// Same command context and selected-detail witnesses, without tree-node controls.
/// Start uses the selected window's named button; View still requires command_owned.
#[doc(hidden)]
pub fn command_selected_detail(
    process: &dyn Process,
    key: &str,
    include_geometry: bool,
) -> Result<(Value, FocusContext)> {
    validate_key(key)?;
    let (body, context) = snapshot_model(
        process,
        0,
        1,
        Some(key),
        Some(FocusQuery::SelectedDetail(key)),
        include_geometry,
        true,
    )?;
    Ok((body, context.expect("Command focus context requested")))
}
