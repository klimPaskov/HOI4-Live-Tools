//! Opaque original own queue, not a selection, placement or action proof.
//! Caller owns one fixed deadline. Every fresh observation uses one witness
//! bracket and rejects old edges before replacement payload; no retries/writes.
use crate::{
    construction::{self as c, catalog, location, members, QueueLine},
    daily_governor::Scope,
    daily_observation, economy,
    error::{Error, Result},
    game::{self, ClockContext},
    memory::{ReadMemory, Reader, StringAbi},
    platform::{Identity, Process},
    research_ui::Witnesses,
};
use std::time::Instant;

#[cfg(test)]
mod tests;

fn changed(message: &str) -> Error {
    Error::new("construction_queue_changed", message)
}
fn deadline(until: Instant) -> Result<()> {
    if Instant::now() >= until {
        Err(Error::new(
            "construction_queue_deadline",
            "Original queue read deadline expired",
        ))
    } else {
        Ok(())
    }
}
struct Timed<'a> {
    process: &'a dyn Process,
    until: Instant,
}
impl ReadMemory for Timed<'_> {
    fn string_abi(&self) -> StringAbi {
        self.process.string_abi()
    }
    fn read(&self, address: u64, size: usize) -> Result<Vec<u8>> {
        deadline(self.until)?;
        let result = self.process.read(address, size);
        deadline(self.until)?;
        result
    }
    fn read_many(&self, requests: &[(u64, usize)]) -> Result<Vec<Vec<u8>>> {
        let mut result = Vec::with_capacity(requests.len());
        // Preserve the resident backend's batch path, with the same original
        // bound around each <=128-request transfer. A late batch is discarded.
        for chunk in requests.chunks(128) {
            deadline(self.until)?;
            let batch = self.process.read_many(chunk);
            deadline(self.until)?;
            result.extend(batch?);
        }
        Ok(result)
    }
}
impl Process for Timed<'_> {
    fn identity(&self) -> Option<&Identity> {
        self.process.identity()
    }
    fn image_base(&self) -> u64 {
        self.process.image_base()
    }
    fn begin_snapshot(&self) -> Result<()> {
        deadline(self.until)?;
        let result = self.process.begin_snapshot();
        deadline(self.until)?;
        result
    }
    fn check(&self) -> Result<()> {
        deadline(self.until)?;
        let result = self.process.check();
        deadline(self.until)?;
        result
    }
}
#[derive(Clone, Debug)]
struct Snapshot {
    context: ClockContext,
    production: u64,
    tag: i32,
    state: u64,
    state_header: Vec<u8>,
    province_header: Option<Vec<u8>>,
    catalog: catalog::Catalog,
    catalog_header: Vec<u8>,
    queue_header: Vec<u8>,
    members: Vec<u64>,
    rows: Vec<QueueLine>,
}
#[derive(Clone, Debug)]
pub(crate) struct Anchor {
    scope: Scope,
    base: u64,
    abi: StringAbi,
    key: String,
    state_id: i32,
    original: Snapshot,
}
/// Fresh exact unchanged-queue observation. No input/selection/+1 authority.
#[derive(Debug)]
pub(crate) struct Observation {
    snapshot: Snapshot,
}
impl Observation {
    pub(crate) fn context(&self) -> &ClockContext {
        &self.snapshot.context
    }
    pub(crate) fn line_count(&self) -> usize {
        self.snapshot.members.len()
    }
}
impl Anchor {
    pub(crate) fn bind_scoped(
        process: &dyn Process,
        scope: &Scope,
        key: &str,
        state_id: i32,
        until: Instant,
    ) -> Result<Self> {
        c::controls::validate_key(key)?;
        c::find_view::validate_state_id(i64::from(state_id))?;
        if key == "rail_way" {
            return Err(Error::new(
                "capability_unavailable",
                "Ordinary building queue only",
            ));
        }
        let base = process.image_base();
        let abi = process.string_abi();
        c::pointer(base)?;
        // Scoped preflight only, never joined into proof. The complete reader
        // repeats own_scope and context in its own single encompassing witness.
        daily_observation::scoped_clock(process, scope, until)?;
        let original = observe(process, scope, key, state_id, None, until)?;
        if process.image_base() != base || process.string_abi() != abi {
            return Err(changed("Original base/ABI changed during binding"));
        }
        deadline(until)?;
        Ok(Self {
            scope: scope.clone(),
            base,
            abi,
            key: key.into(),
            state_id,
            original,
        })
    }
    pub(crate) fn scope(&self) -> &Scope {
        &self.scope
    }
    pub(crate) fn game(&self) -> &Identity {
        &self.scope.epoch.game
    }
    pub(crate) fn context(&self) -> &ClockContext {
        &self.original.context
    }
    pub(crate) fn building_id(&self) -> &str {
        &self.key
    }
    pub(crate) fn state_id(&self) -> i32 {
        self.state_id
    }
    pub(crate) fn line_count(&self) -> usize {
        self.original.members.len()
    }
    pub(crate) fn observe_unchanged(
        &self,
        process: &dyn Process,
        until: Instant,
    ) -> Result<Observation> {
        Ok(Observation {
            snapshot: observe(
                process,
                &self.scope,
                &self.key,
                self.state_id,
                Some(self),
                until,
            )?,
        })
    }
}
fn context(context: &ClockContext, scope: &Scope, previous: Option<&ClockContext>) -> Result<()> {
    crate::calendar::decode(context.date_raw)?;
    if context.anchors != scope.anchors
        || context.player != scope.epoch.player
        || context.difficulty != 2
        || context.historical_raw != 1
        || context.playthrough_id.is_empty()
        || !scope
            .epoch
            .campaign
            .ends_with(&format!(":{}", context.playthrough_id))
        || previous.is_some_and(|old| {
            old.playthrough_id != context.playthrough_id || context.date_raw < old.date_raw
        })
    {
        return Err(changed("Original campaign/setup/player/chronology changed"));
    }
    Ok(())
}
fn header(r: &Reader<'_>, address: u64) -> Result<Vec<u8>> {
    Ok(r.array_header(address)?.0)
}
fn same_header(r: &Reader<'_>, address: u64, original: &[u8]) -> Result<()> {
    if header(r, address)? != original {
        return Err(changed("Original collection header changed before payload"));
    }
    Ok(())
}
/// Does not follow any changed production, state or public template pointer.
fn non_queue_edges(r: &Reader<'_>, anchor: &Anchor) -> Result<()> {
    let b = &anchor.original;
    let base = anchor.base;
    let g = game::layout(r);
    let l = c::layout(r);
    if r.u64(anchor.scope.own_country + g.country_production)? != b.production {
        return Err(changed("Original production changed before payload"));
    }
    location::exact(r, b.production, base + g.production_vtable)?;
    if r.u64(b.production + 0x290)? != anchor.scope.own_country {
        return Err(changed("Original production owner changed"));
    }
    let gs = anchor.scope.anchors[0];
    same_header(r, gs + l.gs_states, &b.state_header)?;
    if location::catalog_pointer(r, gs + l.gs_states, anchor.state_id, 4096)? != b.state {
        return Err(changed("Original requested state changed before payload"));
    }
    if !location::own_state(r, base, b.state, b.tag)? {
        return Err(changed("Original state ownership/control changed"));
    }
    if r.u64(base + l.database_global)? != b.catalog.database {
        return Err(changed("Original building database changed before payload"));
    }
    location::exact(r, b.catalog.database, base + l.database_vtable)?;
    same_header(r, b.catalog.database + 0x40, &b.catalog_header)?;
    if members(r, b.catalog.database + 0x40, 4096)? != b.catalog.members {
        return Err(changed(
            "Original full catalog membership changed before template payload",
        ));
    }
    // Complete original public catalog/type/key equality BEFORE queue membership.
    if catalog::observe(r, base)? != b.catalog {
        return Err(changed("Original catalog/target identity changed"));
    }
    Ok(())
}
fn queue_edges(r: &Reader<'_>, anchor: &Anchor) -> Result<()> {
    let b = &anchor.original;
    let l = c::layout(r);
    let gs = anchor.scope.anchors[0];
    same_header(r, b.production + 0x70, &b.queue_header)?;
    if members(r, b.production + 0x70, c::LINE_LIMIT)? != b.members {
        return Err(changed(
            "Original ordered queue membership changed before row payload",
        ));
    }
    if let Some(old) = &b.province_header {
        same_header(r, gs + l.gs_provinces, old)?;
    }
    for row in &b.rows {
        let (building, template, _) = row
            .building_identity
            .ok_or_else(|| changed("Original row lacks building identity"))?;
        let loc = row
            .location
            .as_ref()
            .ok_or_else(|| changed("Original row lacks own location"))?;
        location::exact(r, row.address, anchor.base + l.building_line_vtable)?;
        if r.u64(row.address + 0x20)? != b.production
            || r.u64(row.address + 8)? != row.handle
            || r.i32(row.address + 0x48)? != row.amount
            || r.u64(row.address + 0x70)? != building
        {
            return Err(changed(
                "Original row owner/handle/amount/building changed before payload",
            ));
        }
        location::exact(r, building, anchor.base + l.building_vtable)?;
        if r.u64(building + l.building_template)? != template
            || r.u64(building + l.building_status)? != loc.status
        {
            return Err(changed(
                "Original building template/location changed before payload",
            ));
        }
        location::exact(r, loc.status, anchor.base + l.status_vtable)?;
        let (id, provincial) = loc
            .province
            .map_or((loc.state_id, false), |(_, id)| (id, true));
        if r.i32(loc.status + 0x6c)? != id
            || r.i32(loc.status + 0x70)? != 0
            || (r.i32(loc.status + 0x68)? & 1 != 0) != provincial
        {
            return Err(changed(
                "Original territorial status changed before location payload",
            ));
        }
        if location::catalog_pointer(r, gs + l.gs_states, loc.state_id, 4096)? != loc.state {
            return Err(changed("Original queue state changed before payload"));
        }
        if let Some((province, id)) = loc.province {
            if location::catalog_pointer(r, gs + l.gs_provinces, id, 20000)? != province {
                return Err(changed("Original province changed before payload"));
            }
            location::exact(r, province, anchor.base + l.province_vtable)?;
            if r.i32(province + l.province_owner)? != b.tag
                || r.u64(province + l.province_state)? != loc.state
            {
                return Err(changed("Original province owner/state changed"));
            }
        }
        if !location::own_state(r, anchor.base, loc.state, b.tag)? {
            return Err(changed("Original queue state no longer owned"));
        }
    }
    Ok(())
}
fn observe(
    process: &dyn Process,
    scope: &Scope,
    key: &str,
    state_id: i32,
    original: Option<&Anchor>,
    until: Instant,
) -> Result<Snapshot> {
    scope.validate()?;
    deadline(until)?;
    let base = process.image_base();
    let abi = process.string_abi();
    c::pointer(base)?;
    if process.identity() != Some(&scope.epoch.game) {
        return Err(changed("Original process differs before snapshot"));
    }
    if original.is_some_and(|a| base != a.base || abi != a.abi) {
        return Err(changed("Original base/ABI differs before snapshot"));
    }
    let timed = Timed { process, until };
    timed.begin_snapshot()?;
    let w = Witnesses::new(&timed);
    let r = Reader::new(&w);
    w.section("retained_queue_scope");
    daily_observation::own_scope(&r, base, scope)?;
    if let Some(a) = original {
        non_queue_edges(&r, a)?;
    }
    let campaign = game::context_from_reader(&r, base)?;
    context(&campaign, scope, original.map(|a| &a.original.context))?;
    let (country, production) = economy::own_production(&r, base)?;
    if country != scope.own_country {
        return Err(changed("Original own country differs"));
    }
    let tag = r.i32(country + 8)?;
    let l = c::layout(&r);
    let state_header = header(&r, campaign.anchors[0] + l.gs_states)?;
    let state = location::catalog_pointer(&r, campaign.anchors[0] + l.gs_states, state_id, 4096)?;
    if !location::own_state(&r, base, state, tag)? {
        return Err(changed("Requested state not owned and controlled"));
    }
    if r.i32(state + l.state_id)? != state_id {
        return Err(changed("Requested state ID/catalog mismatch"));
    }
    w.section("retained_queue_catalog");
    let catalog = catalog::observe(&r, base)?;
    catalog.required(key)?;
    let catalog_header = header(&r, catalog.database + 0x40)?;
    if let Some(a) = original {
        queue_edges(&r, a)?;
    }
    w.section("retained_queue_rows");
    let lines = c::lines_in(&r, base, production, tag, true)?;
    if lines.members.len() != lines.known.len()
        || lines.anomalous != 0
        || lines.known.iter().any(|row| {
            !row.building
                || row.key.is_none()
                || row.building_identity.is_none()
                || row.location.is_none()
        })
    {
        return Err(Error::new(
            "construction_queue_unavailable",
            "Complete canonical own ordinary building rows required",
        ));
    }
    for row in &lines.known {
        let (_, template, ordinal) = row.building_identity.as_ref().unwrap();
        let canonical = catalog.required(row.key.as_ref().unwrap())?;
        if canonical.address != *template || canonical.ordinal != *ordinal {
            return Err(changed("Original row/catalog join differs"));
        }
    }
    let queue_header = header(&r, production + 0x70)?;
    let province_header = if lines.known.iter().any(|row| {
        row.location
            .as_ref()
            .is_some_and(|loc| loc.province.is_some())
    }) {
        Some(header(&r, campaign.anchors[0] + l.gs_provinces)?)
    } else {
        None
    };
    if original.is_some_and(|a| {
        a.original.members != lines.members
            || a.original.rows != lines.known
            || a.original.catalog != catalog
    }) {
        return Err(changed("Original ordered rows/catalog changed"));
    }
    w.finish()?;
    timed.check()?;
    if process.identity() != Some(&scope.epoch.game)
        || process.image_base() != base
        || process.string_abi() != abi
    {
        return Err(changed(
            "Original process/base/ABI changed at final witness",
        ));
    }
    deadline(until)?;
    Ok(Snapshot {
        context: campaign,
        production,
        tag,
        state,
        state_header,
        province_header,
        catalog,
        catalog_header,
        queue_header,
        members: lines.members,
        rows: lines.known,
    })
}
