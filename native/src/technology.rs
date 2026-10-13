// Technology catalog semantics derived from Clausewitz-MCP calling/game.py.
// Copyright (c) 2026 smerss, MIT; ../LICENSE.clausewitz retains full notice.
// The canonical current-build database/index layout was recovered independently.
use crate::{
    error::{Error, Result},
    game,
    memory::Reader,
    platform::Process,
};
use serde_json::{json, Value};
use std::{
    collections::BTreeSet,
    time::{Instant, SystemTime, UNIX_EPOCH},
};

const MAX_TEMPLATES: usize = 8192;

#[derive(PartialEq, Eq)]
struct Templates {
    header: Vec<u8>,
    pointers: Vec<u64>,
}

/// Loaded public definitions only. Membership never implies player eligibility;
/// no engine functions or foreign-country objects are traversed.
pub fn catalog(process: &dyn Process, filter: &str, offset: usize, limit: usize) -> Result<Value> {
    if filter.chars().count() > 128 || offset > MAX_TEMPLATES || !(1..=100).contains(&limit) {
        return Err(Error::new(
            "invalid_arguments",
            "Invalid catalog query bound",
        ));
    }
    let start = Instant::now();
    process.begin_snapshot()?;
    let r = Reader::new(process);
    let base = process.image_base();
    let gs = r.u64(base + 0x44086a8)?;
    let idler = r.u64(base + 0x44085f8)?;
    if r.u64(gs)? != base + 0x42c7598 || r.u64(idler)? != base + 0x42c9a88 {
        return Err(Error::new(
            "campaign_unavailable",
            "In-game catalog required",
        ));
    }
    let db = r.u64(base + 0x439f450)?;
    if r.u64(db)? != base + 0x42110c8 {
        return Err(Error::new(
            "invalid_type",
            "Current technology database required",
        ));
    }
    let entries = templates(&r, db)?;
    let query = filter.to_lowercase();
    let mut unique = BTreeSet::new();
    let mut matches = Vec::new();
    let mut witnesses = Vec::new();
    // The canonical list includes a null-object sentinel at index zero. Other
    // database arrays contain folders, not technologies, and are never followed.
    for (index, template) in entries.pointers.iter().copied().enumerate().skip(1) {
        if template % 8 != 0
            || r.u64(template)? != base + 0x4211038
            || r.i32(template + 0x30)? != index as i32
        {
            return Err(Error::new(
                "invalid_catalog",
                "Template identity/index mismatch",
            ));
        }
        let name = r.string_bounded(template + 8, 256)?;
        if name.is_empty()
            || name.len() > 256
            || name.chars().any(char::is_control)
            || !unique.insert(name.clone())
        {
            return Err(Error::new(
                "invalid_catalog",
                "Empty or duplicate technology ID",
            ));
        }
        witnesses.push((template, name.clone()));
        if name.to_lowercase().contains(&query) {
            matches.push(name);
        }
    }
    // Stable list headers alone do not detect changed template fields or heap
    // string contents. Keep a local field witness, never a persistent cache.
    for (index, (template, name)) in witnesses.iter().enumerate() {
        if r.u64(*template)? != base + 0x4211038
            || r.i32(*template + 0x30)? != index as i32 + 1
            || r.string_bounded(*template + 8, 256)? != *name
        {
            return Err(Error::new("torn_read", "Technology definition changed"));
        }
    }
    if r.u64(base + 0x44086a8)? != gs
        || r.u64(base + 0x44085f8)? != idler
        || r.u64(base + 0x439f450)? != db
        || r.u64(gs)? != base + 0x42c7598
        || r.u64(idler)? != base + 0x42c9a88
        || r.u64(db)? != base + 0x42110c8
        || templates(&r, db)? != entries
    {
        return Err(Error::new("torn_read", "Loaded catalog anchors changed"));
    }
    process.check()?;
    let page: Vec<_> = matches.iter().skip(offset).take(limit).collect();
    let next = offset.saturating_add(page.len());
    Ok(json!({
        "source":"native loaded technology definitions",
        "build_id":game::BUILD_ID,
        "definition_count":entries.pointers.len()-1,
        "count":matches.len(),
        "offset":offset,
        "technologies":page,
        "next_offset":if next < matches.len() { Some(next) } else { None },
        "eligibility":"unavailable: catalog membership is not player research eligibility",
        "observation_paused_game":false,
        "freshness":{
            "observed_unix_ms":SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_millis(),
            "elapsed_ms":start.elapsed().as_secs_f64()*1000.0,
            "read_calls":r.calls.get(),"read_bytes":r.bytes.get()
        }
    }))
}

fn templates(r: &Reader<'_>, db: u64) -> Result<Templates> {
    let (header, data, count) = r.array_header(db + 0x48)?;
    let capacity = i32::from_le_bytes(header[8..12].try_into().unwrap()) as usize;
    if count == 0 || capacity > MAX_TEMPLATES {
        return Err(Error::new(
            "invalid_array",
            "Technology catalog exceeds bound or lacks sentinel",
        ));
    }
    let bytes = r.read(data, count * 8)?;
    if r.read(db + 0x48, 16)? != header {
        return Err(Error::new("torn_read", "Technology catalog header changed"));
    }
    Ok(Templates {
        header,
        pointers: bytes
            .chunks_exact(8)
            .map(|v| u64::from_le_bytes(v.try_into().unwrap()))
            .collect(),
    })
}
