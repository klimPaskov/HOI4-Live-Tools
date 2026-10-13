//! Bounded own research panel diagnostics, independently recovered for the
//! verified Linux build. Descriptors never authorize input or claim hit routing.
// Own canonical technology joins derive from research.rs/Clausewitz-MCP
// calling/game.py semantics. Copyright (c) 2026 smerss, MIT;
// see ../LICENSE.clausewitz. UI layouts are independently recovered.
use crate::{
    error::{Error, Result},
    game,
    memory::{ReadMemory, Reader},
    platform::Process,
};
use serde_json::{json, Value};
use std::{
    cell::RefCell,
    collections::BTreeMap,
    time::{Instant, SystemTime, UNIX_EPOCH},
};

/// Witness reads always reach the process. Request-local shared metadata/proofs
/// may avoid duplicate reads, but their original bytes must pass finish below.
struct Witnesses<'a> {
    process: &'a dyn Process,
    reads: RefCell<BTreeMap<(u64, usize), Vec<u8>>>,
}
impl ReadMemory for Witnesses<'_> {
    fn read(&self, address: u64, size: usize) -> Result<Vec<u8>> {
        let bytes = self.process.read(address, size)?;
        let mut reads = self.reads.borrow_mut();
        if let Some(prior) = reads.get(&(address, size)) {
            if *prior != bytes {
                return Err(Error::new("torn_read", "Research UI field changed"));
            }
        } else {
            reads.insert((address, size), bytes.clone());
        }
        Ok(bytes)
    }
}
impl Witnesses<'_> {
    fn finish(&self) -> Result<(usize, usize)> {
        let reads = self.reads.borrow();
        let requests: Vec<_> = reads.keys().copied().collect();
        let result = self.process.read_many(&requests)?;
        if result.len() != reads.len() {
            return Err(Error::new(
                "short_read",
                "Witness batch response count mismatch",
            ));
        }
        let mut bytes = 0;
        for (((_, size), prior), current) in reads.iter().zip(result.iter()) {
            if current.len() != *size {
                return Err(Error::new(
                    "short_read",
                    "Witness batch response length mismatch",
                ));
            }
            if current != prior {
                return Err(Error::new("torn_read", "Research UI witness changed"));
            }
            bytes += size;
        }
        // Logical read/byte metrics unchanged; actual kernel syscalls are NOT
        // these counts. Linux uses <=128 iovs/chunk plus identity witness reads.
        Ok((reads.len(), bytes))
    }
}
pub(crate) fn exact(r: &Reader<'_>, pointer: u64, vptr: u64) -> Result<()> {
    if pointer % 8 != 0 || r.u64(pointer)? != vptr {
        return Err(Error::new(
            "invalid_gui_type",
            "Exact primary research UI type required",
        ));
    }
    Ok(())
}
pub(crate) fn array(r: &Reader<'_>, address: u64, bound: usize) -> Result<Vec<u64>> {
    let (header, data, count) = r.array_header(address)?;
    let capacity = i32::from_le_bytes(header[8..12].try_into().unwrap()) as usize;
    if capacity > bound || (count > 0 && data % 8 != 0) {
        return Err(Error::new(
            "invalid_array",
            "Research UI array exceeds bound/alignment",
        ));
    }
    let payload = if count == 0 {
        Vec::new()
    } else {
        r.read(data, count * 8)?
    };
    Ok(payload
        .chunks_exact(8)
        .map(|v| u64::from_le_bytes(v.try_into().unwrap()))
        .collect())
}
fn window(r: &Reader<'_>, base: u64, pointer: u64, registry: u64) -> Result<Value> {
    if pointer == 0 {
        return Ok(json!({"materialized":false,"visible_predicate":null}));
    }
    exact(r, pointer, base + 0x4347d58)?;
    if r.u64(pointer + 0x98)? != registry {
        return Err(Error::new(
            "invalid_gui_owner",
            "Research container must belong to own GUI registry",
        ));
    }
    let flags = r.byte(pointer + 0xa5)?;
    let states = [r.i32(pointer + 0xef8)?, r.i32(pointer + 0xeb0)?];
    Ok(
        json!({"materialized":true,"visible_predicate":flags&8!=0&&states.iter().all(|v|*v!=2),"effective_hit_routing_verified":false}),
    )
}
/// Type metadata is diagnostic only. No field layout follows from an unknown
/// name. Tables/typeinfo must lie in this ELF's data.rel.ro; names in rodata.
fn type_encoding(r: &Reader<'_>, base: u64, object: u64) -> Result<String> {
    primary_type_encoding(r, base, r.u64(object)?)
}
pub(crate) fn primary_type_encoding(r: &Reader<'_>, base: u64, table: u64) -> Result<String> {
    let in_section = |p: u64, size: u64, low: u64, high: u64| {
        p >= base + low && p.checked_add(size).is_some_and(|end| end <= base + high)
    };
    let metadata = |p, size| in_section(p, size, 0x41adde0, 0x436d9f8);
    if !metadata(table.saturating_sub(16), 24)
        || i64::from_le_bytes(r.read(table - 16, 8)?.try_into().unwrap()) != 0
    {
        return Err(Error::new(
            "invalid_gui_type",
            "Main-image primary GUI type metadata required",
        ));
    }
    let info = r.u64(table - 8)?;
    if !metadata(info, 16) {
        return Err(Error::new(
            "invalid_gui_type",
            "GUI type information outside verified image",
        ));
    }
    let name = r.u64(info + 8)?;
    if !in_section(name, 128, 0x3506000, 0x3975f39) {
        return Err(Error::new(
            "invalid_gui_type",
            "GUI type name outside verified image",
        ));
    }
    let bytes = r.read(name, 128)?;
    let end = bytes
        .iter()
        .position(|v| *v == 0)
        .ok_or_else(|| Error::new("invalid_gui_type", "GUI type name exceeds bound"))?;
    if end == 0
        || !bytes[..end]
            .iter()
            .all(|v| v.is_ascii_alphanumeric() || *v == b'_')
    {
        return Err(Error::new(
            "invalid_gui_type",
            "Invalid bounded GUI type encoding",
        ));
    }
    Ok(String::from_utf8(bytes[..end].to_vec()).unwrap())
}
pub(crate) fn nullable_type(r: &Reader<'_>, base: u64, object: u64) -> Result<Value> {
    if object == 0 {
        Ok(Value::Null)
    } else {
        Ok(json!(type_encoding(r, base, object)?))
    }
}
fn render_details(r: &Reader<'_>, base: u64, object: u64) -> Result<Value> {
    let table = r.u64(object)?;
    if ![base + 0x434c5b0, base + 0x434b370, base + 0x4356e08].contains(&table) {
        // Even a matching RTTI name never supplies a renderer field layout.
        return Ok(json!({"field_layout_verified":false}));
    }
    let frame = u32::from_le_bytes(r.read(object + 0x34, 4)?.try_into().unwrap());
    let hit_resource = r.u64(
        object
            + if table == base + 0x434c5b0 {
                0x160
            } else if table == base + 0x434b370 {
                0x1b0
            } else {
                // Exact primary CSprite: the plain sprite factory passes its
                // direct resource to the base constructor; getter +2c0 uses
                // +168. This is independently verified, not a name fallback.
                0x168
            },
    )?;
    let mut value = json!({"current_frame_raw":frame,"hit_resource_type_encoding":nullable_type(r,base,hit_resource)?,"geometry_verified":false});
    if table == base + 0x434b370 {
        // Different getters use +168 and +1b0. The factory constructor stores
        // the same direct resource object in both; observe equality afresh.
        let dimension_resource = r.u64(object + 0x168)?;
        value["dimension_resource_type_encoding"] = nullable_type(r, base, dimension_resource)?;
        value["resources_same"] = json!(dimension_resource == hit_resource);
    }
    Ok(value)
}

/// Exact CContainerWindow's button lookup uses these paired arrays. Its other
/// named registry at +a60/+a78 is a different operation and is not consulted.
fn named_research_button(r: &Reader<'_>, container: u64) -> Result<u64> {
    let buttons = array(r, container + 0x9a0, 256)?;
    let (header, names, count) = r.array_header(container + 0x9b8)?;
    let capacity = i32::from_le_bytes(header[8..12].try_into().unwrap()) as usize;
    if capacity > 256 || count != buttons.len() || (count > 0 && names % 8 != 0) {
        return Err(Error::new(
            "invalid_gui_registry",
            "Bounded paired research button/name arrays required",
        ));
    }
    if count > 0 {
        r.read(names, count * 32)?;
    }
    let mut found = None;
    for (index, button) in buttons.iter().enumerate() {
        if r.string_bounded(names + index as u64 * 32, 256)? == "research"
            && found.replace(*button).is_some()
        {
            return Err(Error::new(
                "invalid_gui_registry",
                "Unique ordinary research button name required",
            ));
        }
    }
    found.ok_or_else(|| {
        Error::new(
            "gui_unavailable",
            "Named ordinary research button unavailable",
        )
    })
}

/// Only entries reached through the ordinary own tree are inspected. The
/// engine can retain multiple info windows, including hidden ones; no single
/// selected/eligible target follows merely from vector membership.
fn technology_info(
    r: &Reader<'_>,
    base: u64,
    idler: u64,
    tree: u64,
    status: u64,
    registry: u64,
) -> Result<Value> {
    let entries = array(r, tree + 0xdc0, 64)?;
    if entries.is_empty() {
        return Ok(json!({"count":0,"visible_count":0,"windows":[]}));
    }
    let mut catalog: Option<(Vec<u64>, Vec<u64>)> = None;
    let mut unique = std::collections::BTreeSet::new();
    let mut output = Vec::new();
    let mut visible_count = 0;
    for info in &entries {
        exact(r, *info, base + 0x430bbf0)?;
        if !unique.insert(*info) || r.u64(info + 0xa78)? != idler || r.u64(info + 0xa80)? != tree {
            return Err(Error::new(
                "invalid_gui_owner",
                "Unique own ordinary technology info window required",
            ));
        }
        let container = r.u64(info + 0xa88)?;
        let container_state = window(r, base, container, registry)?;
        if container_state["visible_predicate"] == true {
            visible_count += 1;
        }
        let technology = r.u64(info + 0xa98)?;
        if technology == 0 {
            // The updater explicitly permits a retained empty Info object.
            // It supplies no candidate ID or ordinary-button authority.
            output.push(json!({"technology":null,"window":container_state,"ordinary_research_button":null,"eligibility":"unavailable","input_authorized":false}));
            continue;
        }
        exact(r, technology, base + 0x42107f8)?;
        // Refuse foreign technology before traversing its template/payload.
        if r.u64(technology + 0x158)? != status {
            return Err(Error::new(
                "invalid_research_owner",
                "Info technology must belong to own research status",
            ));
        }
        if catalog.is_none() {
            let db = r.u64(base + 0x439f450)?;
            exact(r, db, base + 0x42110c8)?;
            let templates = array(r, db + 0x48, 8192)?;
            let own = array(r, status + 0x88, 8192)?;
            if templates.is_empty() || own.len() != templates.len() - 1 {
                return Err(Error::new(
                    "research_unavailable",
                    "Initialized own technology catalog required",
                ));
            }
            catalog = Some((templates, own));
        }
        let (templates, own) = catalog.as_ref().unwrap();
        let template = r.u64(technology + 0x160)?;
        exact(r, template, base + 0x4211038)?;
        let id = r.i32(template + 0x30)?;
        if id <= 0
            || templates.get(id as usize) != Some(&template)
            || own.get(id as usize - 1) != Some(&technology)
        {
            return Err(Error::new(
                "invalid_research_join",
                "Canonical own info technology join required",
            ));
        }
        let key = r.string_bounded(template + 8, 256)?;
        if key.is_empty() || key.chars().any(char::is_control) {
            return Err(Error::new(
                "invalid_research_id",
                "Bounded public technology key required",
            ));
        }
        let ordinary = if container == 0 {
            json!({"materialized":false})
        } else {
            let button = named_research_button(r, container)?;
            if button != r.u64(info + 0xac0)? {
                return Err(Error::new(
                    "invalid_gui_registry",
                    "Named research button must match ordinary callback control",
                ));
            }
            exact(r, button, base + 0x43470c8)?;
            let disabled = r.i32(button + 0xac)?;
            let flags = u32::from_le_bytes(r.read(button + 0x74, 4)?.try_into().unwrap());
            json!({"materialized":true,"selected_control_alias_matches":r.u64(info + 0xab0)?==button,"disabled_raw":disabled,"disabled_predicate":disabled==3,"flags_raw":flags,"input_authorized":false})
        };
        output.push(json!({"technology":key,"window":container_state,"ordinary_research_button":ordinary,"eligibility":"unavailable","input_authorized":false}));
    }
    Ok(json!({"count":entries.len(),"visible_count":visible_count,"windows":output}))
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct UiContext {
    pub campaign: game::ClockContext,
    pub anchors: [u64; 6],
    pub slots: Vec<u64>,
}
pub fn snapshot(process: &dyn Process) -> Result<Value> {
    Ok(observe(process, false, false, None)?.0)
}
/// Dedicated diagnostic keeps current-node validation out of clock/UI waits.
pub fn nodes(process: &dyn Process) -> Result<Value> {
    let (value, _) = observe(process, false, true, None)?;
    Ok(
        json!({"source":"native own current research nodes","build_id":game::BUILD_ID,
        "selected_slot":value["selected_slot"],"active_mode_raw":value["active_mode_raw"],
        "current_nodes":value["current_nodes"],"freshness":value["freshness"],
        "owned":true,"input_authorized":false,"observation_paused_game":false}),
    )
}
/// One exact own ID; complete current-record ownership, target GUI only.
pub fn node_detail(process: &dyn Process, key: &str) -> Result<Value> {
    validate_detail_key(key)?;
    observe_target(process, key)
}

pub(crate) fn validate_detail_key(key: &str) -> Result<()> {
    if key.is_empty() || key.len() > 256 || key.chars().any(char::is_control) {
        return Err(Error::new(
            "invalid_arguments",
            "One bounded exact technology ID required",
        ));
    }
    Ok(())
}
/// Validate the whole current own model, then bound the public output page.
pub fn nodes_page(
    process: &dyn Process,
    offset: usize,
    limit: usize,
    prefix: Option<&str>,
) -> Result<Value> {
    if offset > 512
        || !(1..=100).contains(&limit)
        || prefix.is_some_and(|s| {
            s.len() > 128 || !s.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_')
        })
    {
        return Err(Error::new(
            "invalid_arguments",
            "Current node query exceeds bounds",
        ));
    }
    let mut value = nodes(process)?;
    let current = value["current_nodes"]
        .as_object_mut()
        .ok_or_else(|| Error::new("invalid_node_page", "Current node model unavailable"))?;
    let all = current
        .remove("nodes")
        .and_then(|v| v.as_array().cloned())
        .ok_or_else(|| Error::new("invalid_node_page", "Current node list unavailable"))?;
    let filtered: Vec<_> = all
        .into_iter()
        .filter(|node| {
            prefix.is_none_or(|p| node["id"].as_str().is_some_and(|id| id.starts_with(p)))
        })
        .collect();
    let count = filtered.len();
    let page: Vec<_> = filtered.into_iter().skip(offset).take(limit).collect();
    let next = if offset + page.len() < count {
        Some(offset + page.len())
    } else {
        None
    };
    current.insert("nodes".into(), json!(page));
    current.insert("filtered_count".into(), json!(count));
    current.insert("offset".into(), json!(offset));
    current.insert("limit".into(), json!(limit));
    current.insert("next_offset".into(), json!(next));
    Ok(value)
}
pub(crate) fn wait_snapshot(process: &dyn Process) -> Result<(Value, UiContext)> {
    let (value, context) = observe(process, true, false, None)?;
    Ok((
        value,
        context.expect("Context requested by wait observation"),
    ))
}
fn observe(
    process: &dyn Process,
    include_context: bool,
    include_nodes: bool,
    requested_detail: Option<&str>,
) -> Result<(Value, Option<UiContext>)> {
    let start = Instant::now();
    process.begin_snapshot()?;
    let observed = Witnesses {
        process,
        reads: RefCell::new(BTreeMap::new()),
    };
    let r = Reader::new(&observed);
    let base = process.image_base();
    let gs = r.u64(base + 0x44086a8)?;
    let idler = r.u64(base + 0x44085f8)?;
    exact(&r, gs, base + 0x42c7598)?;
    exact(&r, idler, base + 0x42c9a88)?;
    let registry = r.u64(idler + 0x4f8)?;
    exact(&r, registry, base + 0x42736f8)?;
    let tag = r.i32(gs + 0x518)?;
    if !(1..4096).contains(&tag) {
        return Err(Error::new(
            "player_unavailable",
            "Positive own country tag required",
        ));
    }
    // Enforce positive own scope before the broader campaign reader traverses
    // country payloads. Repeated tag reads share the same field witnesses.
    let campaign = if include_context {
        Some(game::context_from_reader(&r, base)?)
    } else {
        None
    };
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
    let status = r.u64(country + 0xf38)?;
    exact(&r, status, base + 0x42108f8)?;
    if r.u64(status + 0x130)? != country || r.i32(status + 0x138)? != tag {
        return Err(Error::new(
            "invalid_research_owner",
            "Own research status required",
        ));
    }
    let slots = array(&r, status + 0xa0, 32)?;
    let mut unique = std::collections::BTreeSet::new();
    for slot in &slots {
        exact(&r, *slot, base + 0x4210898)?;
        if !unique.insert(*slot) || r.u64(slot + 0x10)? != status {
            return Err(Error::new(
                "invalid_research_owner",
                "Unique own research slot required",
            ));
        }
    }
    let manager = r.u64(idler + 0x6b8)?;
    if r.u64(manager + 0x180)? != idler {
        return Err(Error::new(
            "invalid_gui_owner",
            "Research interface owner changed",
        ));
    }
    let view = r.u64(manager + 0xf0)?;
    let tree = r.u64(manager + 0xf8)?;
    exact(&r, view, base + 0x424dc38)?;
    exact(&r, tree, base + 0x424de40)?;
    if r.u64(view + 0x2f0)? != idler || r.u64(tree + 0x2f0)? != idler || r.i32(tree + 0xe24)? != 6 {
        return Err(Error::new(
            "invalid_gui_owner",
            "Ordinary own research views required",
        ));
    }
    let display_tag = r.i32(tree + 0xe20)?;
    if display_tag > 0 && display_tag != tag {
        return Err(Error::new(
            "foreign_ui_context",
            "Own displayed research country required",
        ));
    }
    let selected = r.u64(tree + 0xe08)?;
    let selected_index = if selected == 0 {
        None
    } else {
        Some(slots.iter().position(|v| *v == selected).ok_or_else(|| {
            Error::new(
                "invalid_research_owner",
                "Selected slot is outside own research",
            )
        })?)
    };
    let active = r.i32(manager + 0x3d8)?;
    let panel_state = window(&r, base, r.u64(view + 0x2f8)?, registry)?;
    let tree_state = window(&r, base, r.u64(tree + 0x2f8)?, registry)?;
    let widgets = array(&r, view + 0x308, 32)?;
    if widgets.len() > slots.len() {
        return Err(Error::new(
            "invalid_research_join",
            "More widgets than own research slots",
        ));
    }
    let mut output = Vec::new();
    let mut widget_ids = std::collections::BTreeSet::new();
    for (index, widget) in widgets.iter().enumerate() {
        exact(&r, *widget, base + 0x424e7d8)?;
        if !widget_ids.insert(*widget)
            || r.u64(widget + 0x2a8)? != registry
            || r.u64(widget + 0x2b0)? != view
            || r.u64(widget + 0x2b8)? != slots[index]
        {
            return Err(Error::new(
                "invalid_research_join",
                "Own widget/slot array join required",
            ));
        }
        let container = r.u64(widget + 0x10)?;
        let container_state = window(&r, base, container, registry)?;
        if container == 0 {
            return Err(Error::new(
                "gui_unavailable",
                "Research slot container unavailable",
            ));
        }
        let buttons = array(&r, container + 0x958, 64)?;
        let mut button_output = Vec::new();
        for button in buttons.iter().take(8) {
            let encoding = type_encoding(&r, base, *button)?;
            let mut value = json!({"type_encoding":encoding,"geometry_verified":false});
            let table = r.u64(*button)?;
            let render = if table == base + 0x43470c8 {
                value["disabled_raw"] = json!(r.i32(button + 0xac)?);
                Some(r.u64(button + 0xe8)?)
            } else if table == base + 0x4346b10 {
                // CButtonDrag's constructor/getter use +110. Its +e8 is
                // position data, and +ac is a state machine, not disabled.
                value["state_raw"] = json!(r.i32(button + 0xac)?);
                Some(r.u64(button + 0x110)?)
            } else {
                None
            };
            if let Some(render) = render {
                value["render_type_encoding"] = nullable_type(&r, base, render)?;
                if render != 0 {
                    value["render_details"] = render_details(&r, base, render)?;
                }
            }
            button_output.push(value);
        }
        output.push(json!({"slot":index,"container":container_state,"button_count":buttons.len(),"buttons_observed":button_output,"button_sample_limit":8,"input_authorized":false}));
    }
    let info = technology_info(&r, base, idler, tree, status, registry)?;
    let nodes = if include_nodes {
        if active != 6
            || selected_index.is_none()
            || display_tag < 0
            || tree_state["visible_predicate"] != true
            || panel_state["visible_predicate"] == true
        {
            return Err(Error::new(
                "gui_unavailable",
                "Selected own ordinary research tree must be open",
            ));
        }
        Some(if let Some(key) = requested_detail {
            crate::technology_nodes::current_node_detail(&r, base, tree, status, registry, key)?
        } else {
            crate::technology_nodes::current_nodes(&r, base, tree, status, registry)?
        })
    } else {
        None
    };
    let (end_calls, end_bytes) = observed.finish()?;
    process.check()?;
    let context = campaign.map(|campaign| UiContext {
        campaign,
        anchors: [country, status, registry, manager, view, tree],
        slots: slots.clone(),
    });
    Ok((
        json!({"source":"native own research UI diagnostics","build_id":game::BUILD_ID,"owned":true,"own_slot_count":slots.len(),"widget_count":widgets.len(),"widgets_complete":widgets.len()==slots.len(),"slot_identity":"own array index in current process/status snapshot; not a persistent ID","selected_slot":selected_index,"active_mode_raw":active,"panel":panel_state,"ordinary_tree":tree_state,"slots":output,"technology_info":info,"current_nodes":nodes,"input_authorized":false,"unverified":["effective ancestor visibility and clipping","sprite hit predicates","topmost hit routing","physical client conversion"],"observation_paused_game":false,"freshness":{"observed_unix_ms":SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_millis(),"elapsed_ms":start.elapsed().as_secs_f64()*1000.0,"read_calls_kind":"logical_read_requests","read_calls":r.calls.get()+end_calls as u64,"read_bytes":r.bytes.get()+end_bytes}}),
        context,
    ))
}

fn observe_target(process: &dyn Process, key: &str) -> Result<Value> {
    let start = Instant::now();
    process.begin_snapshot()?;
    let observed = Witnesses {
        process,
        reads: RefCell::new(BTreeMap::new()),
    };
    let r = Reader::new(&observed);
    let base = process.image_base();
    let gs = r.u64(base + 0x44086a8)?;
    let idler = r.u64(base + 0x44085f8)?;
    exact(&r, gs, base + 0x42c7598)?;
    exact(&r, idler, base + 0x42c9a88)?;
    let registry = r.u64(idler + 0x4f8)?;
    exact(&r, registry, base + 0x42736f8)?;
    let tag = r.i32(gs + 0x518)?;
    if !(1..4096).contains(&tag) {
        return Err(Error::new(
            "player_unavailable",
            "Positive own country tag required",
        ));
    }
    // Enforce positive own scope before the broader campaign reader traverses
    // country payloads. Repeated tag reads share the same field witnesses.

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
    let status = r.u64(country + 0xf38)?;
    exact(&r, status, base + 0x42108f8)?;
    if r.u64(status + 0x130)? != country || r.i32(status + 0x138)? != tag {
        return Err(Error::new(
            "invalid_research_owner",
            "Own research status required",
        ));
    }
    let slots = array(&r, status + 0xa0, 32)?;
    let mut unique = std::collections::BTreeSet::new();
    for slot in &slots {
        exact(&r, *slot, base + 0x4210898)?;
        if !unique.insert(*slot) || r.u64(slot + 0x10)? != status {
            return Err(Error::new(
                "invalid_research_owner",
                "Unique own research slot required",
            ));
        }
    }
    let manager = r.u64(idler + 0x6b8)?;
    if r.u64(manager + 0x180)? != idler {
        return Err(Error::new(
            "invalid_gui_owner",
            "Research interface owner changed",
        ));
    }
    let view = r.u64(manager + 0xf0)?;
    let tree = r.u64(manager + 0xf8)?;
    exact(&r, view, base + 0x424dc38)?;
    exact(&r, tree, base + 0x424de40)?;
    if r.u64(view + 0x2f0)? != idler || r.u64(tree + 0x2f0)? != idler || r.i32(tree + 0xe24)? != 6 {
        return Err(Error::new(
            "invalid_gui_owner",
            "Ordinary own research views required",
        ));
    }
    let display_tag = r.i32(tree + 0xe20)?;
    if display_tag > 0 && display_tag != tag {
        return Err(Error::new(
            "foreign_ui_context",
            "Own displayed research country required",
        ));
    }
    let selected = r.u64(tree + 0xe08)?;
    let selected_index = if selected == 0 {
        None
    } else {
        Some(slots.iter().position(|v| *v == selected).ok_or_else(|| {
            Error::new(
                "invalid_research_owner",
                "Selected slot is outside own research",
            )
        })?)
    };
    let active = r.i32(manager + 0x3d8)?;
    let panel_state = window(&r, base, r.u64(view + 0x2f8)?, registry)?;
    let tree_state = window(&r, base, r.u64(tree + 0x2f8)?, registry)?;

    if active != 6
        || selected_index.is_none()
        || display_tag < 0
        || tree_state["visible_predicate"] != true
        || panel_state["visible_predicate"] == true
    {
        return Err(Error::new(
            "gui_unavailable",
            "Selected own ordinary research tree must be open",
        ));
    }
    let catalog = crate::target_node::Catalog::load(&r, base, status, key)?;
    let detail = crate::target_node::observe(&r, base, tree, status, registry, key, &catalog)?;
    let entries = array(&r, tree + 0xdc0, 64)?;
    let mut unique = std::collections::BTreeSet::new();
    let mut visible = 0;
    for info in entries {
        exact(&r, info, base + 0x430bbf0)?;
        if !unique.insert(info) || r.u64(info + 0xa78)? != idler || r.u64(info + 0xa80)? != tree {
            return Err(Error::new(
                "invalid_gui_owner",
                "Unique own technology info window required",
            ));
        }
        let technology = r.u64(info + 0xa98)?;
        if technology == 0 {
            continue;
        }
        catalog.validate(&r, base, status, technology)?;
        if technology == catalog.technology {
            let state = window(&r, base, r.u64(info + 0xa88)?, registry)?;
            if state["visible_predicate"] == true {
                visible += 1;
            }
        }
    }
    if visible > 1 {
        return Err(Error::new(
            "ambiguous_gui_detail",
            "Multiple visible own details for requested key",
        ));
    }
    let (end_calls, end_bytes) = observed.finish()?;
    process.check()?;
    Ok(
        json!({"source":"native own research node detail","build_id":game::BUILD_ID,"owned":true,"id":key,
        "validation_scope":"requested_node_with_complete_current_record_ownership","full_gui_model_validated":false,
        "detail":detail,"own_info_postcondition":{"matching_locally_visible_count":visible,"requested_detail_locally_visible":visible==1,"fresh_transition_verified":false},
        "selected_slot":selected_index,"client_rectangle_available":false,"effective_routing_verified":false,"input_authorized":false,"observation_paused_game":false,
        "freshness":{"observed_unix_ms":SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_millis(),"elapsed_ms":start.elapsed().as_secs_f64()*1000.0,"read_calls_kind":"logical_read_requests","read_calls":r.calls.get()+end_calls as u64,"read_bytes":r.bytes.get()+end_bytes}}),
    )
}

#[cfg(test)]
mod vectored_finish_tests {
    use super::*;
    use std::cell::Cell;
    struct Fake {
        scalar: Cell<usize>,
        batch: Cell<usize>,
        mode: u8,
    }
    impl ReadMemory for Fake {
        fn read(&self, _: u64, n: usize) -> Result<Vec<u8>> {
            self.scalar.set(self.scalar.get() + 1);
            Ok(vec![1; n])
        }
        fn read_many(&self, requests: &[(u64, usize)]) -> Result<Vec<Vec<u8>>> {
            self.batch.set(self.batch.get() + 1);
            let mut values: Vec<_> = requests.iter().map(|(_, n)| vec![1; *n]).collect();
            match self.mode {
                1 => values[1][0] = 2,
                2 => {
                    values.pop();
                }
                3 => {
                    values[0].pop();
                }
                _ => {}
            }
            Ok(values)
        }
    }
    impl Process for Fake {
        fn check(&self) -> Result<()> {
            Ok(())
        }
        fn image_base(&self) -> u64 {
            0
        }
    }
    #[test]
    fn finish_uses_batch_and_keeps_logical_counts() {
        let f = Fake {
            scalar: Cell::new(0),
            batch: Cell::new(0),
            mode: 0,
        };
        let w = Witnesses {
            process: &f,
            reads: RefCell::new(BTreeMap::new()),
        };
        w.read(0x10000, 4).unwrap();
        w.read(0x10010, 8).unwrap();
        assert_eq!(w.finish().unwrap(), (2, 12));
        assert_eq!(f.scalar.get(), 2);
        assert_eq!(f.batch.get(), 1);
    }
    #[test]
    fn final_mutation_and_malformed_batch_fail_whole_model() {
        for (mode, code) in [(1, "torn_read"), (2, "short_read"), (3, "short_read")] {
            let f = Fake {
                scalar: Cell::new(0),
                batch: Cell::new(0),
                mode,
            };
            let w = Witnesses {
                process: &f,
                reads: RefCell::new(BTreeMap::new()),
            };
            w.read(0x10000, 4).unwrap();
            w.read(0x10010, 8).unwrap();
            assert_eq!(w.finish().unwrap_err().code, code);
        }
    }
}
