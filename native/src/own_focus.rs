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
    cell::RefCell,
    collections::{BTreeMap, BTreeSet},
    time::{Instant, SystemTime, UNIX_EPOCH},
};

struct Witnesses<'a> {
    process: &'a dyn Process,
    reads: RefCell<BTreeMap<(u64, usize), Vec<u8>>>,
}
impl ReadMemory for Witnesses<'_> {
    fn read(&self, a: u64, n: usize) -> Result<Vec<u8>> {
        let bytes = self.process.read(a, n)?;
        let mut reads = self.reads.borrow_mut();
        if let Some(old) = reads.get(&(a, n)) {
            if *old != bytes {
                return Err(Error::new("torn_read", "Focus observation changed"));
            }
        } else {
            reads.insert((a, n), bytes.clone());
        }
        Ok(bytes)
    }
}
impl Witnesses<'_> {
    fn finish(&self) -> Result<(usize, usize)> {
        let reads = self.reads.borrow();
        let mut bytes = 0;
        for ((a, n), old) in reads.iter() {
            if self.process.read(*a, *n)? != *old {
                return Err(Error::new("torn_read", "Focus witness changed"));
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
fn named_start(
    r: &Reader<'_>,
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
/// Caller must open a Process with the verified build/session contract. This
/// module has no native call or input capability. Pagination bounds output only;
/// all keys/memberships are validated before a page can be returned.
fn snapshot_model(
    process: &dyn Process,
    offset: usize,
    limit: usize,
    prefix: Option<&str>,
    requested: Option<&str>,
    include_geometry: bool,
) -> Result<Value> {
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
    };
    let r = Reader::new(&w);
    let base = process.image_base();
    let gs = r.u64(base + 0x44086a8)?;
    let idler = r.u64(base + 0x44085f8)?;
    exact(&r, gs, base + 0x42c7598)?;
    exact(&r, idler, base + 0x42c9a88)?;
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
    if r.u64(manager + 0x180)? != idler {
        return Err(Error::new(
            "invalid_focus_owner",
            "Own interface manager required",
        ));
    }
    let view = r.u64(manager + 0x130)?;
    exact(&r, view, base + 0x4249c70)?;
    let displayed = r.u64(view + 0x11d8)?;
    if r.u64(view + 0x2f0)? != idler || (displayed != 0 && displayed != country) {
        return Err(Error::new(
            "foreign_focus_scope",
            "Focus view must display own country before catalog payload",
        ));
    }
    let tree = r.u64(country + 0x1338)?;
    exact(&r, tree, base + 0x4202410)?;
    let db = r.u64(base + 0x439faf0)?;
    exact(&r, db, base + 0x4202590)?;
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
    let node_association = if let Some(key) = requested {
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
    let panels = pointers(&r, view + 0x1180, 32)?;
    unique(&panels)?;
    let mut details = Vec::new();
    for panel in panels {
        exact(&r, panel, base + 0x43023f8)?;
        if r.u64(panel + 0x7b8)? != idler || r.u64(panel + 0x7c0)? != view {
            return Err(Error::new(
                "invalid_focus_owner",
                "Own focus detail joins required",
            ));
        }
        let selected = r.u64(panel + 0x7d0)?;
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
        let observed = if window == 0 {
            json!({"materialized":false,"detail_visibility_flag":null,"conservative_container_visibility":null,"start":{"materialized":false}})
        } else {
            exact(&r, window, base + 0x4347d58)?;
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
    let total = output.len();
    let page: Vec<_> = output.into_iter().skip(offset).take(limit).collect();
    let (end_calls, end_bytes) = w.finish()?;
    process.check()?;
    Ok(
        json!({"source":"native own focus catalog and UI diagnostics","build_id":game::BUILD_ID,"owned":true,"catalog_count":own.len(),"filtered_count":total,"offset":offset,"limit":limit,"next_offset":if offset.saturating_add(page.len())<total {Some(offset+page.len())}else{None},"focuses":page,"details":details,"node_association":node_association,"numeric_ids_available":false,"eligibility_available":false,"input_authorized":false,"branch_visibility_available":false,"observation_paused_game":false,"freshness":{"observed_unix_ms":SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_millis(),"elapsed_ms":start.elapsed().as_secs_f64()*1000.0,"read_calls":r.calls.get()+end_calls as u64,"read_bytes":r.bytes.get()+end_bytes}}),
    )
}

pub fn snapshot(
    process: &dyn Process,
    offset: usize,
    limit: usize,
    prefix: Option<&str>,
) -> Result<Value> {
    snapshot_model(process, offset, limit, prefix, None, false)
}
/// Exact own public focus key, read-only current item/control association.
pub fn node_detail(process: &dyn Process, key: &str) -> Result<Value> {
    validate_key(key)?;
    snapshot_model(process, 0, 1, Some(key), Some(key), false)
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

/// Opt-in deep renderer diagnostics; default node/list readers remain metadata-only.
pub fn node_detail_with_geometry(
    process: &dyn Process,
    key: &str,
    include_geometry: bool,
) -> Result<Value> {
    validate_key(key)?;
    snapshot_model(process, 0, 1, Some(key), Some(key), include_geometry)
}
