//! Own economy read semantics independently recovered from current Linux ELF.
//! Consulted smerss/Clausewitz-MCP calling/api.py, MIT; ../LICENSE.clausewitz.
use crate::{
    error::{Error, Result},
    game,
    memory::Reader,
    platform::Process,
    research_ui::Witnesses,
};
use serde_json::{json, Value};
use std::time::{Instant, SystemTime, UNIX_EPOCH};
const FIXED: i64 = 100000;
const RAW_LIMIT: i64 = 100_000_000_000;
const COUNT_LIMIT: i64 = 1_000_000;
fn exact(r: &Reader<'_>, p: u64, table: u64) -> Result<()> {
    if p % 8 != 0 || r.u64(p)? != table {
        return Err(Error::new(
            "invalid_economy_type",
            "Exact own economy primary required",
        ));
    }
    Ok(())
}
fn raw(r: &Reader<'_>, a: u64, signed: bool) -> Result<i64> {
    let n = i64::from_le_bytes(r.read(a, 8)?.try_into().unwrap());
    if !(-RAW_LIMIT..=RAW_LIMIT).contains(&n) || (!signed && n < 0) {
        return Err(Error::new(
            "invalid_economy_value",
            "Fixed-point economy value exceeds policy",
        ));
    }
    Ok(n)
}
fn count(r: &Reader<'_>, a: u64) -> Result<i64> {
    let n = i64::from(r.i32(a)?);
    if !(0..=COUNT_LIMIT).contains(&n) {
        return Err(Error::new(
            "invalid_economy_value",
            "Economy adjustment exceeds policy",
        ));
    }
    Ok(n)
}
fn subtract(n: i64, a: i64, b: i64) -> Result<i64> {
    let v = n
        .checked_sub(a)
        .and_then(|v| v.checked_sub(b))
        .ok_or_else(|| Error::new("invalid_economy_value", "Economy arithmetic overflow"))?;
    if !(0..=COUNT_LIMIT).contains(&v) {
        return Err(Error::new(
            "invalid_economy_value",
            "Factory trigger count outside policy",
        ));
    }
    Ok(v)
}
fn own_entry(r: &Reader<'_>, a: u64, index: usize, stride: usize) -> Result<Vec<u8>> {
    let (h, data, _) = r.array_header(a)?;
    let cap = i32::from_le_bytes(h[8..12].try_into().unwrap());
    if cap > 4096 || data % (stride as u64) != 0 {
        return Err(Error::new(
            "invalid_array",
            "Own country map exceeds policy/alignment",
        ));
    }
    r.array_entry(a, index, stride)
}
/// Existing own tag/country/production join. Caller owns process brackets and
/// final witnesses; no aggregate-count or GUI semantics follow from this root.
pub(crate) fn own_production(r: &Reader<'_>, base: u64) -> Result<(u64, u64)> {
    let g = game::layout(r);
    let gs = r.u64(base + g.gs_global)?;
    exact(r, gs, base + g.gs_vtable)?;
    let idler = r.u64(base + g.idler_global)?;
    exact(r, idler, base + g.idler_vtable)?;
    let tag = r.i32(gs + g.player_tag)?;
    if !(1..4096).contains(&tag) {
        return Err(Error::new(
            "player_unavailable",
            "Positive own country tag required",
        ));
    }
    let index = i32::from_le_bytes(
        own_entry(r, gs + g.tag_country_index, tag as usize, 4)?
            .try_into()
            .unwrap(),
    );
    if index < 0 {
        return Err(Error::new("player_unavailable", "Own country index absent"));
    }
    let country = u64::from_le_bytes(
        own_entry(r, gs + g.countries, index as usize, 8)?
            .try_into()
            .unwrap(),
    );
    exact(r, country, base + g.country_vtable)?;
    if r.i32(country + 8)? != tag {
        return Err(Error::new(
            "invalid_economy_owner",
            "Own country tag join required",
        ));
    }
    let production = r.u64(country + g.country_production)?;
    exact(r, production, base + g.production_vtable)?;
    if r.u64(production + game::layout(r).production(0x290))? != country {
        return Err(Error::new(
            "invalid_economy_owner",
            "Own production country join required",
        ));
    }
    Ok((country, production))
}
pub fn snapshot(process: &dyn Process) -> Result<Value> {
    let started = Instant::now();
    process.begin_snapshot()?;
    let w = Witnesses::new(process);
    let r = Reader::new(&w);
    let base = process.image_base();
    let (country, production) = own_production(&r, base)?;
    let g = game::layout(&r);
    let politics = r.u64(country + g.country_politics)?;
    exact(&r, politics, base + g.politics_vtable)?;
    if r.u64(politics + 0xc8)? != country {
        return Err(Error::new(
            "invalid_economy_owner",
            "Own politics country join required",
        ));
    }
    let context = game::context_from_reader(&r, base)?;
    let mil = raw(&r, production + g.production(0x2b8), false)? / FIXED;
    let naval = raw(&r, production + g.production(0x318), false)? / FIXED;
    let civ = raw(&r, production + g.production(0x378), false)? / FIXED;
    let military = subtract(
        mil,
        count(&r, production + g.production(0x2f8))?,
        count(&r, production + g.production(0x300))?,
    )?;
    let civilian = subtract(
        civ,
        count(&r, production + g.production(0x3b8))?,
        count(&r, production + g.production(0x3c0))?,
    )?;
    let naval = subtract(naval, 0, 0)?;
    let project_cost = raw(&r, production + g.production(0x390), false)? / FIXED;
    let project_adjustment = count(&r, production + g.production(0x3b0))?;
    let project = civ
        .checked_sub(project_cost)
        .and_then(|v| v.checked_sub(project_adjustment))
        .ok_or_else(|| {
            Error::new(
                "invalid_economy_value",
                "Project arithmetic/policy unavailable",
            )
        })?
        .max(0);
    let project = subtract(project, 0, 0)?;
    let pp = raw(&r, politics + 0xe0, true)?;
    let (end_calls, end_bytes) = w.finish()?;
    process.check()?;
    Ok(
        json!({"source":"native own economy trigger counts","build_id":g.build_id,"owned":true,"player":context.player,"factory_trigger_count":{"military":military,"naval":naval,"civilian":civilian},"civilian_factories_available_for_projects":project,"political_power":{"raw_signed":pp,"raw_units_per_point":FIXED,"exact_rational":{"numerator":pp,"denominator":FIXED},"integer_trigger_count":pp/FIXED},"unavailable":{"ui_factory_totals":true,"idle_builders":true,"stability":true,"war_support":true,"shortages":true},"freshness":{"observed_unix_ms":SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_millis(),"elapsed_ms":started.elapsed().as_secs_f64()*1000.0,"read_calls_kind":"logical_read_requests","read_calls":r.calls.get()+end_calls as u64,"read_bytes":r.bytes.get()+end_bytes},"observation_paused_game":false,"input_authorized":false}),
    )
}
