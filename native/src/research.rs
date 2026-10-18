// Own research semantics derived from Clausewitz-MCP calling/game.py.
// Copyright (c) 2026 smerss, MIT; ../LICENSE.clausewitz retains full notice.
// Primary types, canonical joins and raw field widths are independently recovered.
use crate::{
    error::{Error, Result},
    game,
    memory::Reader,
    platform::Process,
    research_ui::Witnesses,
};
use serde_json::{json, Value};
use std::{
    collections::{BTreeMap, BTreeSet},
    time::{Instant, SystemTime, UNIX_EPOCH},
};

/// Technology/research layout of one inspected build; image values are
/// relative to the verified image base, the rest are object field offsets.
#[derive(Debug)]
pub(crate) struct Layout {
    pub(crate) database_vtable: u64,
    pub(crate) template_vtable: u64,
    pub(crate) database_templates: u64,
    pub(crate) template_index: u64,
    template_threshold: u64,
    technology_vtable: u64,
    status_vtable: u64,
    slot_vtable: u64,
}
/// Linux ELF a4971c9e (libstdc++).
const LINUX: Layout = Layout {
    database_vtable: 0x42110c8,
    template_vtable: 0x4211038,
    database_templates: 0x48,
    template_index: 0x30,
    template_threshold: 0x3cc,
    technology_vtable: 0x42107f8,
    status_vtable: 0x42108f8,
    slot_vtable: 0x4210898,
};
/// macOS Mach-O 089EA519 (libc++), mapped live on 18 October 2026: vtables by
/// RTTI name, template index/array moved by the 24-byte key string, threshold
/// from the level comparison shared by both builds' code.
const MACOS: Layout = Layout {
    database_vtable: 0x339eda0,
    template_vtable: 0x339ed10,
    database_templates: 0x40,
    template_index: 0x28,
    template_threshold: 0x39c,
    technology_vtable: 0x339e3e8,
    status_vtable: 0x339e4e8,
    slot_vtable: 0x339e488,
};
pub(crate) fn layout(r: &Reader<'_>) -> &'static Layout {
    match r.string_abi() {
        crate::memory::StringAbi::LibStdCxx => &LINUX,
        crate::memory::StringAbi::LibCxx => &MACOS,
    }
}

#[derive(PartialEq, Eq)]
struct Array {
    header: Vec<u8>,
    pointers: Vec<u64>,
}
fn array(r: &Reader<'_>, address: u64, bound: usize) -> Result<Array> {
    let (header, data, count) = r.array_header(address)?;
    let capacity = i32::from_le_bytes(header[8..12].try_into().unwrap()) as usize;
    if capacity > bound || (count > 0 && data % 8 != 0) {
        return Err(Error::new(
            "invalid_array",
            "Research array exceeds bound/alignment",
        ));
    }
    let bytes = if count == 0 {
        Vec::new()
    } else {
        r.read(data, count * 8)?
    };
    if r.read(address, 16)? != header {
        return Err(Error::new("torn_read", "Research array header changed"));
    }
    Ok(Array {
        header,
        pointers: bytes
            .chunks_exact(8)
            .map(|v| u64::from_le_bytes(v.try_into().unwrap()))
            .collect(),
    })
}

#[derive(PartialEq, Eq)]
struct Selected {
    header: Vec<u8>,
    value: Vec<u8>,
}
fn selected(r: &Reader<'_>, address: u64, index: usize, stride: usize) -> Result<Selected> {
    let (header, data, count) = r.array_header(address)?;
    if index >= count {
        return Err(Error::new(
            "player_unavailable",
            "Own country index unavailable",
        ));
    }
    let value = r.read(data + index as u64 * stride as u64, stride)?;
    if r.read(address, 16)? != header {
        return Err(Error::new("torn_read", "Own country array changed"));
    }
    Ok(Selected { header, value })
}
fn exact(r: &Reader<'_>, object: u64, vptr: u64) -> Result<()> {
    if object % 8 != 0 || r.u64(object)? != vptr {
        return Err(Error::new(
            "invalid_type",
            "Exact primary research object required",
        ));
    }
    Ok(())
}

#[derive(PartialEq, Eq)]
struct Technology {
    id: usize,
    template: u64,
    key: String,
    level: i32,
    threshold: i32,
    progress: i64,
}
struct Model<'a, 'b> {
    r: &'a Reader<'b>,
    base: u64,
    status: u64,
    templates: &'a Array,
    own: &'a Array,
}
impl Model<'_, '_> {
    fn technology(&self, pointer: u64) -> Result<Technology> {
        let r = self.r;
        let l = layout(r);
        exact(r, pointer, self.base + l.technology_vtable)?;
        if r.u64(pointer + 0x158)? != self.status {
            return Err(Error::new(
                "invalid_research_owner",
                "Own technology status required",
            ));
        }
        let template = r.u64(pointer + 0x160)?;
        exact(r, template, self.base + l.template_vtable)?;
        let id = r.i32(template + l.template_index)?;
        if id <= 0
            || self.templates.pointers.get(id as usize) != Some(&template)
            || self.own.pointers.get(id as usize - 1) != Some(&pointer)
        {
            return Err(Error::new(
                "invalid_research_join",
                "Canonical own technology join required",
            ));
        }
        let key = r.string_bounded(template + 8, 256)?;
        if key.is_empty() || key.chars().any(char::is_control) {
            return Err(Error::new(
                "invalid_research_id",
                "Bounded public technology key required",
            ));
        }
        Ok(Technology {
            id: id as usize,
            template,
            key,
            level: r.i32(pointer + 0x174)?,
            threshold: r.i32(template + l.template_threshold)?,
            progress: i64::from_le_bytes(r.read(pointer + 0x198, 8)?.try_into().unwrap()),
        })
    }
}

/// Fresh own-country completed membership and active raw counters. No foreign
/// tags, eligibility calls, progress normalization or cached definitions.
pub fn snapshot(process: &dyn Process) -> Result<Value> {
    page(process, 0, 100)
}
/// Internal ownership evidence; never serialize these process-local addresses
/// as public action IDs. The owner must compare this context with its binding.
#[doc(hidden)]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ResearchContext {
    pub campaign: game::ClockContext,
    pub own: [u64; 2],
    pub slots: Vec<u64>,
}
/// Canonical own slots and campaign evidence from one witnessed process bracket.
/// All completed membership is validated; its returned page is bounded to one.
#[doc(hidden)]
pub fn owned_snapshot(process: &dyn Process) -> Result<(Value, ResearchContext)> {
    let (value, context) = observe(process, 0, 1, true)?;
    Ok((
        value,
        context.ok_or_else(|| Error::new("research_unavailable", "Owned context missing"))?,
    ))
}
pub fn page(process: &dyn Process, offset: usize, limit: usize) -> Result<Value> {
    Ok(observe(process, offset, limit, false)?.0)
}
fn observe(
    process: &dyn Process,
    offset: usize,
    limit: usize,
    include_context: bool,
) -> Result<(Value, Option<ResearchContext>)> {
    if offset > 8192 || !(1..=100).contains(&limit) {
        return Err(Error::new(
            "invalid_arguments",
            "Research pagination exceeds bounds",
        ));
    }
    let start = Instant::now();
    process.begin_snapshot()?;
    // The public page keeps its existing direct reader/readbacks. Only the
    // internal owner path brackets context and body with production witnesses.
    let observed = include_context.then(|| Witnesses::new(process));
    let r = match observed.as_ref() {
        Some(witnesses) => Reader::new(witnesses),
        None => Reader::new(process),
    };
    let base = process.image_base();
    let (mut value, context) = observe_from_reader(&r, base, offset, limit, include_context)?;
    let (end_calls, end_bytes) = observed
        .as_ref()
        .map(Witnesses::finish)
        .transpose()?
        .unwrap_or((0, 0));
    process.check()?;
    value["freshness"] = json!({"observed_unix_ms":SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_millis(),"elapsed_ms":start.elapsed().as_secs_f64()*1000.0,"read_calls":r.calls.get()+end_calls as u64,"read_bytes":r.bytes.get()+end_bytes});
    Ok((value, context))
}
/// Unchanged canonical validation core; caller owns witnesses/process bracket.
pub(crate) fn observe_from_reader(
    r: &Reader<'_>,
    base: u64,
    offset: usize,
    limit: usize,
    include_context: bool,
) -> Result<(Value, Option<ResearchContext>)> {
    let g = game::layout(r);
    let l = layout(r);
    let gs = r.u64(base + g.gs_global)?;
    let idler = r.u64(base + g.idler_global)?;
    exact(r, gs, base + g.gs_vtable)?;
    exact(r, idler, base + g.idler_vtable)?;
    let tag = r.i32(gs + g.player_tag)?;
    if !(1..4096).contains(&tag) {
        return Err(Error::new(
            "player_unavailable",
            "Positive own country tag required",
        ));
    }
    let index_witness = selected(r, gs + g.tag_country_index, tag as usize, 4)?;
    let index = i32::from_le_bytes(index_witness.value[..].try_into().unwrap());
    if index < 0 {
        return Err(Error::new("player_unavailable", "Own country index absent"));
    }
    let country_witness = selected(r, gs + g.countries, index as usize, 8)?;
    let country = u64::from_le_bytes(country_witness.value[..].try_into().unwrap());
    r.object_type(country, "CCountry")?;
    let country_vptr = r.u64(country)?;
    let status = r.u64(country + g.country_technology_status)?;
    exact(r, status, base + l.status_vtable)?;
    if r.u64(status + 0x130)? != country || r.i32(status + 0x138)? != tag {
        return Err(Error::new(
            "invalid_research_owner",
            "Own country status link required",
        ));
    }
    let db = r.u64(base + g.technology_db_global)?;
    exact(r, db, base + l.database_vtable)?;
    let templates = array(r, db + l.database_templates, 8192)?;
    let own = array(r, status + 0x88, 8192)?;
    let completed = array(r, status + 0x40, 8192)?;
    let slots = array(r, status + 0xa0, 32)?;
    if templates.pointers.is_empty() || own.pointers.len() != templates.pointers.len() - 1 {
        return Err(Error::new(
            "research_unavailable",
            "Fully initialized own technology list required",
        ));
    }
    let model = Model {
        r,
        base,
        status,
        templates: &templates,
        own: &own,
    };
    let mut witnesses = BTreeMap::new();
    let mut ids = BTreeSet::new();
    let mut completed_output = Vec::new();
    for pointer in &completed.pointers {
        let tech = model.technology(*pointer)?;
        if !ids.insert(tech.id) || tech.level < tech.threshold {
            return Err(Error::new(
                "invalid_completed_research",
                "Duplicate or inconsistent completed membership",
            ));
        }
        completed_output.push(json!({"id":tech.key, "level_raw":tech.level, "completion_threshold_raw":tech.threshold}));
        witnesses.insert(*pointer, tech);
    }
    let mut slot_output = Vec::new();
    let mut slot_witnesses = Vec::new();
    let mut slot_ids = BTreeSet::new();
    let mut slot_pointers = BTreeSet::new();
    for (index, slot) in slots.pointers.iter().copied().enumerate() {
        exact(r, slot, base + l.slot_vtable)?;
        if !slot_pointers.insert(slot) || r.u64(slot + 0x10)? != status {
            return Err(Error::new(
                "invalid_research_owner",
                "Unique own research slot required",
            ));
        }
        let current = r.u64(slot + 0x18)?;
        slot_witnesses.push((slot, current));
        if current == 0 {
            slot_output.push(json!({"slot":index,"researching":null,"progress_raw":null,"progress":null,"days_remaining":null}));
        } else {
            let tech = model.technology(current)?;
            if !slot_ids.insert(tech.id) {
                return Err(Error::new(
                    "invalid_research_join",
                    "Duplicate active technology",
                ));
            }
            slot_output.push(json!({"slot":index,"researching":tech.key,"progress_raw":tech.progress,"progress":null,"days_remaining":null}));
            if let Some(prior) = witnesses.get(&current) {
                if prior != &tech {
                    return Err(Error::new(
                        "torn_read",
                        "Technology changed across membership/slot reads",
                    ));
                }
            }
            witnesses.insert(current, tech);
        }
    }
    let mut keys = BTreeSet::new();
    for (pointer, before) in &witnesses {
        if !keys.insert(&before.key) {
            return Err(Error::new(
                "invalid_research_id",
                "Duplicate public technology key",
            ));
        }
        if model.technology(*pointer)? != *before {
            return Err(Error::new("torn_read", "Own technology fields changed"));
        }
    }
    for (slot, current) in slot_witnesses {
        exact(r, slot, base + l.slot_vtable)?;
        if r.u64(slot + 0x10)? != status || r.u64(slot + 0x18)? != current {
            return Err(Error::new("torn_read", "Own research slot changed"));
        }
    }
    if r.u64(base + g.gs_global)? != gs
        || r.u64(base + g.idler_global)? != idler
        || r.u64(base + g.technology_db_global)? != db
        || r.u64(gs)? != base + g.gs_vtable
        || r.u64(idler)? != base + g.idler_vtable
        || r.i32(gs + g.player_tag)? != tag
        || selected(r, gs + g.tag_country_index, tag as usize, 4)? != index_witness
        || selected(r, gs + g.countries, index as usize, 8)? != country_witness
        || r.u64(country)? != country_vptr
        || r.u64(country + g.country_technology_status)? != status
        || r.u64(status)? != base + l.status_vtable
        || r.u64(status + 0x130)? != country
        || r.i32(status + 0x138)? != tag
        || r.u64(db)? != base + l.database_vtable
        || array(r, db + l.database_templates, 8192)? != templates
        || array(r, status + 0x88, 8192)? != own
        || array(r, status + 0x40, 8192)? != completed
        || array(r, status + 0xa0, 32)? != slots
    {
        return Err(Error::new(
            "torn_read",
            "Own research model anchors changed",
        ));
    }
    // Validate the entire canonical own body before the broader context reader
    // can traverse active technology/template strings. Shared witnesses also
    // refuse changed slot/owner/header bytes before a new payload is followed.
    let campaign = include_context
        .then(|| game::context_from_reader(r, base))
        .transpose()?;
    if campaign
        .as_ref()
        .is_some_and(|c| c.anchors != [gs, idler, db])
    {
        return Err(Error::new(
            "torn_read",
            "Research body and campaign roots differ",
        ));
    }
    let context = campaign.map(|campaign| ResearchContext {
        campaign,
        own: [country, status],
        slots: slots.pointers,
    });
    completed_output.sort_by(|a, b| a["id"].as_str().cmp(&b["id"].as_str()));
    let count = completed_output.len();
    let page: Vec<_> = completed_output.iter().skip(offset).take(limit).collect();
    let next = offset.saturating_add(page.len());
    Ok((
        json!({
            "source":"native own research","build_id":g.build_id,"owned":true,
            "researched":page,"researched_count":count,"offset":offset,
            "next_offset":if next < count { Some(next) } else { None },"slots":slot_output,
            "completion_meaning":"engine completed-membership array",
            "progress_raw_units":"unverified; signed engine counter",
            "eligibility":"unavailable","observation_paused_game":false,
        }),
        context,
    ))
}
