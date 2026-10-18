//! Owned-session fresh daily research producer. No input or foreign payload.
use crate::{
    actions::Epoch,
    daily_governor::{Observation, OwnCounts, Scope},
    error::{Error, Result},
    game::ClockContext,
    memory::{ReadMemory, Reader},
    platform::{Identity, Process},
    research::{self, ResearchContext},
    research_ui::Witnesses,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    path::Path,
    sync::Mutex,
    time::{Instant, SystemTime, UNIX_EPOCH},
};
fn deadline(until: Instant) -> Result<()> {
    if Instant::now() >= until {
        Err(Error::new(
            "daily_deadline",
            "Fresh daily observation exceeded bounded deadline",
        ))
    } else {
        Ok(())
    }
}
pub fn unix_ms() -> Result<u64> {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| Error::new("invalid_clock", "Host wall clock precedes epoch"))?
        .as_millis()
        .try_into()
        .map_err(|_| Error::new("invalid_clock", "Host timestamp exceeds bound"))
}
struct Timed<'a> {
    inner: &'a dyn Process,
    until: Instant,
}
impl ReadMemory for Timed<'_> {
    fn string_abi(&self) -> crate::memory::StringAbi {
        self.inner.string_abi()
    }
    fn read(&self, a: u64, n: usize) -> Result<Vec<u8>> {
        deadline(self.until)?;
        let result = self.inner.read(a, n);
        deadline(self.until)?;
        result
    }
    fn read_many(&self, q: &[(u64, usize)]) -> Result<Vec<Vec<u8>>> {
        deadline(self.until)?;
        let result = self.inner.read_many(q);
        deadline(self.until)?;
        result
    }
}
impl Process for Timed<'_> {
    fn check(&self) -> Result<()> {
        deadline(self.until)?;
        let result = self.inner.check();
        deadline(self.until)?;
        result
    }
    fn begin_snapshot(&self) -> Result<()> {
        deadline(self.until)?;
        let result = self.inner.begin_snapshot();
        deadline(self.until)?;
        result
    }
    fn image_base(&self) -> u64 {
        self.inner.image_base()
    }
    fn identity(&self) -> Option<&Identity> {
        self.inner.identity()
    }
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Record {
    version: u8,
    scope: Scope,
    sequence: u64,
    last_date: i32,
}
struct Counter {
    store: crate::daily_governor::store::Store,
    record: Record,
    poisoned: bool,
    probe_latest_date: i32,
}
/// Process-local pointers never serialize as player IDs or tool arguments.
pub struct Producer {
    expected: ClockContext,
    counter: Mutex<Counter>,
}
pub struct Produced {
    pub observation: Observation,
    pub canonical: Value,
    pub context: ResearchContext,
}
/// Unpublished scheduling evidence only. No input or completion authority.
#[doc(hidden)]
pub struct ResearchProbe {
    context: ResearchContext,
    slots: Vec<Option<String>>,
}
impl ResearchProbe {
    pub fn context(&self) -> &ResearchContext {
        &self.context
    }
    pub fn all_heads_occupied(&self, entries: &[crate::daily_governor::Entry]) -> Result<bool> {
        if entries.is_empty() {
            return Ok(false);
        }
        for entry in entries {
            let crate::daily_governor::Intent::Research {
                technology_id,
                slot,
            } = &entry.intent
            else {
                return Err(Error::new("invalid_plan", "Research queue heads required"));
            };
            match self.slots.get(*slot as usize) {
                Some(Some(current)) if current != technology_id => {}
                Some(_) => return Ok(false),
                None => {
                    return Err(Error::new(
                        "research_unavailable",
                        "Unknown own slot is not occupied/empty evidence",
                    ))
                }
            }
        }
        Ok(true)
    }
}
fn same(a: &ClockContext, b: &ClockContext) -> bool {
    a.player == b.player
        && a.playthrough_id == b.playthrough_id
        && a.anchors == b.anchors
        && a.difficulty == b.difficulty
        && a.historical_raw == b.historical_raw
}
fn own_scope(r: &Reader<'_>, base: u64, scope: &Scope) -> Result<()> {
    let [gs, idler, db] = scope.anchors;
    if r.u64(base + 0x44086a8)? != gs
        || r.u64(base + 0x44085f8)? != idler
        || r.u64(base + 0x439f450)? != db
        || r.u64(gs)? != base + 0x42c7598
        || r.u64(idler)? != base + 0x42c9a88
    {
        return Err(Error::new("ownership_changed", "Daily clock roots changed"));
    }
    let tag = r.i32(gs + 0x518)?;
    if !(1..4096).contains(&tag) {
        return Err(Error::new(
            "player_unavailable",
            "Positive own tag required before clock payload",
        ));
    }
    let index = i32::from_le_bytes(
        r.array_entry(gs + 0x340, tag as usize, 4)?
            .try_into()
            .unwrap(),
    );
    if index < 0
        || u64::from_le_bytes(
            r.array_entry(gs + 0x310, index as usize, 8)?
                .try_into()
                .unwrap(),
        ) != scope.own_country
    {
        return Err(Error::new(
            "ownership_changed",
            "Daily own-country map changed",
        ));
    }
    r.object_type(scope.own_country, "CCountry")?;
    if r.u64(scope.own_country + 0xf38)? != scope.own_status
        || r.u64(scope.own_status)? != base + 0x42108f8
        || r.u64(scope.own_status + 0x130)? != scope.own_country
        || r.i32(scope.own_status + 0x138)? != tag
    {
        return Err(Error::new("ownership_changed", "Daily own status changed"));
    }
    Ok(())
}
fn read(
    process: &dyn Process,
    epoch: &Epoch,
    expected: &ClockContext,
    scope: Option<&Scope>,
    until: Instant,
) -> Result<(Value, ResearchContext)> {
    deadline(until)?;
    if process.identity() != Some(&epoch.game) {
        return Err(Error::new(
            "ownership_changed",
            "Daily producer requires exact owned process identity",
        ));
    }
    let timed = Timed {
        inner: process,
        until,
    };
    timed.begin_snapshot()?;
    let witnesses = Witnesses::new(&timed);
    let r = Reader::new(&witnesses);
    if let Some(scope) = scope {
        own_scope(&r, process.image_base(), scope)?;
    }
    let (body, context) = research::observe_from_reader(&r, process.image_base(), 0, 1, true)?;
    let context = context
        .ok_or_else(|| Error::new("research_unavailable", "Own canonical context missing"))?;
    if context.campaign.player != epoch.player
        || !same(&context.campaign, expected)
        || context.campaign.difficulty != 2
        || context.campaign.historical_raw != 1
        || context.campaign.date_raw < expected.date_raw
    {
        return Err(Error::new(
            "ownership_changed",
            "Daily campaign/player/setup/roots/timeline changed",
        ));
    }
    witnesses.finish()?;
    timed.check()?;
    deadline(until)?;
    if process.identity() != Some(&epoch.game) {
        return Err(Error::new(
            "ownership_changed",
            "Daily process changed after witness finish",
        ));
    }
    Ok((body, context))
}
impl Producer {
    pub fn open(
        process: &dyn Process,
        epoch: Epoch,
        expected: ClockContext,
        path: &Path,
        until: Instant,
    ) -> Result<Self> {
        let (_, context) = read(process, &epoch, &expected, None, until)?;
        let scope = Scope {
            epoch,
            anchors: context.campaign.anchors,
            own_country: context.own[0],
            own_status: context.own[1],
        };
        let (mut store, bytes) = crate::daily_governor::store::Store::open(path)?;
        deadline(until)?;
        let record = if let Some(bytes) = bytes {
            let record: Record = serde_json::from_slice(&bytes)
                .map_err(|e| Error::new("invalid_sequence", e.to_string()))?;
            if record.version != 1
                || record.scope != scope
                || crate::calendar::decode(record.last_date).is_err()
                || context.campaign.date_raw < record.last_date
            {
                return Err(Error::new(
                    "ownership_changed",
                    "Persisted daily sequence scope/chronology differs",
                ));
            }
            record
        } else {
            Record {
                version: 1,
                scope,
                sequence: 0,
                last_date: context.campaign.date_raw,
            }
        };
        store.replace(
            &serde_json::to_vec(&record)
                .map_err(|e| Error::new("invalid_sequence", e.to_string()))?,
        )?;
        deadline(until)?;
        Ok(Self {
            expected,
            counter: Mutex::new(Counter {
                store,
                probe_latest_date: record.last_date,
                record,
                poisoned: false,
            }),
        })
    }
    pub fn scope(&self) -> Result<Scope> {
        Ok(self
            .counter
            .try_lock()
            .map_err(|_| Error::new("input_busy", "Daily producer currently reading"))?
            .record
            .scope
            .clone())
    }
    /// Internal clock probe: no publication, sequence allocation or fsync.
    /// Prove stored own scope before the broader clock reader can follow it.
    pub fn probe_clock(&self, process: &dyn Process, until: Instant) -> Result<i32> {
        deadline(until)?;
        let mut counter = crate::daily_digest_lock::acquire(&self.counter, None)?;
        if counter.poisoned {
            return Err(Error::new(
                "daily_quarantined",
                "Daily sequence uncertainty",
            ));
        }
        if process.identity() != Some(&counter.record.scope.epoch.game) {
            return Err(Error::new(
                "ownership_changed",
                "Daily clock process changed",
            ));
        }
        let timed = Timed {
            inner: process,
            until,
        };
        timed.begin_snapshot()?;
        let w = Witnesses::new(&timed);
        let r = Reader::new(&w);
        own_scope(&r, process.image_base(), &counter.record.scope)?;
        let context = crate::game::context_from_reader(&r, process.image_base())?;
        if !same(&context, &self.expected)
            || context.date_raw < counter.record.last_date.max(counter.probe_latest_date)
        {
            return Err(Error::new(
                "ownership_changed",
                "Daily clock context or chronology changed",
            ));
        }
        w.finish()?;
        timed.check()?;
        deadline(until)?;
        counter.probe_latest_date = context.date_raw;
        Ok(context.date_raw)
    }
    /// Complete canonical reader, no UI, sequence allocation or fsync.
    /// Used only to avoid full UI when every queue head is already occupied.
    pub fn probe_research(&self, process: &dyn Process, until: Instant) -> Result<ResearchProbe> {
        deadline(until)?;
        let mut counter = crate::daily_digest_lock::acquire(&self.counter, None)?;
        if counter.poisoned {
            return Err(Error::new(
                "daily_quarantined",
                "Daily sequence uncertainty",
            ));
        }
        let (canonical, context) = read(
            process,
            &counter.record.scope.epoch,
            &self.expected,
            Some(&counter.record.scope),
            until,
        )?;
        if context.own
            != [
                counter.record.scope.own_country,
                counter.record.scope.own_status,
            ]
            || context.campaign.date_raw < counter.record.last_date.max(counter.probe_latest_date)
        {
            return Err(Error::new(
                "ownership_changed",
                "Daily probe own status or chronology changed",
            ));
        }
        let array = canonical["slots"]
            .as_array()
            .filter(|a| !a.is_empty() && a.len() <= 32)
            .ok_or_else(|| {
                Error::new(
                    "research_unavailable",
                    "Complete nonempty own slots required",
                )
            })?;
        let mut slots = Vec::with_capacity(array.len());
        for (index, slot) in array.iter().enumerate() {
            if slot["slot"].as_u64() != Some(index as u64) {
                return Err(Error::new(
                    "research_unavailable",
                    "Complete own slot order required",
                ));
            }
            slots.push(match slot.get("researching") {
                Some(Value::Null) => None,
                Some(Value::String(key)) if !key.is_empty() && key.len() <= 256 => {
                    Some(key.clone())
                }
                _ => {
                    return Err(Error::new(
                        "research_unavailable",
                        "Unknown own occupancy is not empty",
                    ))
                }
            });
        }
        deadline(until)?;
        counter.probe_latest_date = context.campaign.date_raw;
        Ok(ResearchProbe { context, slots })
    }
    pub fn observe(&self, process: &dyn Process, until: Instant) -> Result<Produced> {
        deadline(until)?;
        self.observe_locked(process, until, false)
    }
    /// Only digest may wait for the producer, within its original read deadline.
    pub fn observe_digest(&self, process: &dyn Process, until: Instant) -> Result<Produced> {
        self.observe_locked(process, until, true)
    }
    fn observe_locked(
        &self,
        process: &dyn Process,
        until: Instant,
        wait: bool,
    ) -> Result<Produced> {
        deadline(until)?;
        let mut counter = crate::daily_digest_lock::acquire(&self.counter, wait.then_some(until))?;
        if counter.poisoned {
            return Err(Error::new(
                "daily_quarantined",
                "Daily sequence persistence uncertain",
            ));
        }
        if let Err(e) = counter.store.check() {
            counter.poisoned = true;
            return Err(e);
        }
        let (canonical, context) = read(
            process,
            &counter.record.scope.epoch,
            &self.expected,
            Some(&counter.record.scope),
            until,
        )?;
        if context.own
            != [
                counter.record.scope.own_country,
                counter.record.scope.own_status,
            ]
            || context.campaign.date_raw < counter.record.last_date.max(counter.probe_latest_date)
        {
            return Err(Error::new(
                "ownership_changed",
                "Daily own status or chronology changed",
            ));
        }
        let slots = canonical["slots"]
            .as_array()
            .filter(|s| !s.is_empty() && s.len() <= 32)
            .ok_or_else(|| {
                Error::new(
                    "research_unavailable",
                    "Complete nonempty own slots required",
                )
            })?;
        let idle = slots.iter().filter(|s| s["researching"].is_null()).count() as u32;
        let stamp = unix_ms()?;
        deadline(until)?;
        let record = Record {
            version: 1,
            scope: counter.record.scope.clone(),
            sequence: counter
                .record
                .sequence
                .checked_add(1)
                .ok_or_else(|| Error::new("sequence_full", "Daily sequence exhausted"))?,
            last_date: context.campaign.date_raw,
        };
        let bytes = serde_json::to_vec(&record)
            .map_err(|e| Error::new("invalid_sequence", e.to_string()))?;
        if let Err(e) = counter.store.replace(&bytes) {
            counter.poisoned = true;
            return Err(e);
        }
        counter.record = record;
        Timed {
            inner: process,
            until,
        }
        .check()?;
        deadline(until)?;
        Ok(Produced {
            observation: Observation {
                scope: counter.record.scope.clone(),
                date_raw: context.campaign.date_raw,
                sequence: counter.record.sequence,
                observed_unix_ms: stamp,
                source: "native witnessed own canonical research and clock context".into(),
                counts: OwnCounts {
                    idle_research_slots: Some(idle),
                    ..OwnCounts::default()
                },
            },
            canonical,
            context,
        })
    }
}
