// Own-country object semantics derived from Clausewitz-MCP calling/{api,game}.py.
// Copyright (c) 2026 smerss, MIT; ../LICENSE.clausewitz retains full notice.
// Offsets below belong only to the independently inspected current Linux ELF.
use crate::{
    calendar,
    error::{Error, Result},
    memory::Reader,
    platform::Process,
};
use serde_json::{json, Value};
use std::time::{Instant, SystemTime, UNIX_EPOCH};
pub const BUILD_ID: &str = "a4971c9e8ac9a6f2487dea109e01f9839ee59db7";
pub const SHA256: &str = "56fc2945cdbf2475a316db3285eb3458ae5d577db149db6dfdba6f004de3a88a";
/// Additional own-campaign context for ordinary clock input. The persisted
/// playthrough ID distinguishes campaigns, but is not a load-generation token.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ClockContext {
    pub playthrough_id: String,
    pub player: String,
    pub anchors: [u64; 3],
    pub date_raw: i32,
    pub difficulty: i32,
    pub historical_raw: i32,
    pub paused: bool,
    pub speed: u8,
}

pub fn clock_context(process: &dyn Process) -> Result<ClockContext> {
    process.begin_snapshot()?;
    let r = Reader::new(process);
    let base = process.image_base();
    let (_, context) = context_fields(&r, base)?;
    process.check()?;
    Ok(context)
}
fn context_fields(r: &Reader<'_>, base: u64) -> Result<(SnapshotFields, ClockContext)> {
    let initial = roots(r, base)?;
    let [gs, idler, _] = initial;
    if r.u64(gs)? != base + 0x42c7598 || r.u64(idler)? != base + 0x42c9a88 {
        return Err(Error::new(
            "campaign_unavailable",
            "Current in-game context required",
        ));
    }
    // Current-build getter 0x200e720 returns GS+0x718. The generator and
    // telemetry caller identify this string as playthrough_id. Serialization
    // can restore it, so no claim is made that every reload changes it.
    let playthrough_id = r.string(gs + 0x718)?;
    if playthrough_id.len() != 36
        || !playthrough_id.bytes().enumerate().all(|(i, b)| {
            if [8, 13, 18, 23].contains(&i) {
                b == b'-'
            } else {
                b.is_ascii_hexdigit()
            }
        })
    {
        return Err(Error::new(
            "campaign_unavailable",
            "Bounded playthrough identity unavailable",
        ));
    }
    let historical_raw = r.i32(gs + 0x628)?;
    // Use the same local read bracket as the campaign identity. Calling the
    // public snapshot here would refresh maps and check ownership twice.
    let observed = snapshot_fields(r, base)?;
    let date_raw = observed.date.raw;
    if observed.anchors != initial
        || observed.historical_raw != historical_raw
        || roots(r, base)? != initial
        || r.string(gs + 0x718)? != playthrough_id
        || r.i32(gs + 0x628)? != historical_raw
        || r.i32(gs + 0x460)? != date_raw
        || r.i32(gs + 0x4b4)? != observed.speed
        || r.byte(idler + 0x6c1)? != u8::from(observed.paused)
        || r.i32(gs + 0x620)? != observed.difficulty
        || r.i32(gs + 0x518)? != observed.tag_id
    {
        return Err(Error::new(
            "torn_read",
            "Clock input context changed during observation",
        ));
    }
    let context = ClockContext {
        playthrough_id,
        anchors: initial,
        historical_raw,
        date_raw,
        player: observed.player.clone(),
        difficulty: observed.difficulty,
        paused: observed.paused,
        speed: observed.speed as u8,
    };
    Ok((observed, context))
}
fn pointer(reader: &Reader<'_>, address: u64) -> Result<u64> {
    let value = reader.u64(address)?;
    if !(0x10000..=0x7fff_ffff_0000).contains(&value) {
        return Err(Error::new(
            "invalid_pointer",
            "Required game object is unavailable",
        ));
    }
    Ok(value)
}
fn roots(reader: &Reader<'_>, base: u64) -> Result<[u64; 3]> {
    Ok([
        pointer(reader, base + 0x44086a8)?,
        pointer(reader, base + 0x44085f8)?,
        pointer(reader, base + 0x439f450)?,
    ])
}
fn integer(data: Vec<u8>) -> i32 {
    i32::from_le_bytes(data.try_into().unwrap())
}
fn address(data: Vec<u8>) -> Result<u64> {
    let value = u64::from_le_bytes(data.try_into().unwrap());
    if !(0x10000..=0x7fff_ffff_0000).contains(&value) {
        return Err(Error::new(
            "invalid_pointer",
            "Array object pointer unavailable",
        ));
    }
    Ok(value)
}
pub fn snapshot(process: &dyn Process) -> Result<Value> {
    let start = Instant::now();
    process.begin_snapshot()?;
    let r = Reader::new(process);
    let fields = snapshot_fields(&r, process.image_base())?;
    process.check()?;
    // Include the final ownership/exec check in reported snapshot time.
    Ok(snapshot_value(fields, &r, start))
}
/// The wait baseline and payload share one read/ownership bracket. A separate
/// clock read beside an overview would not establish coherent campaign context.
pub fn wait_snapshot(process: &dyn Process) -> Result<(Value, ClockContext)> {
    let start = Instant::now();
    process.begin_snapshot()?;
    let r = Reader::new(process);
    let (fields, context) = context_fields(&r, process.image_base())?;
    process.check()?;
    Ok((snapshot_value(fields, &r, start), context))
}
fn snapshot_value(fields: SnapshotFields, r: &Reader<'_>, start: Instant) -> Value {
    json!({"source":"native process_vm_readv","owned":true,"build_id":BUILD_ID,"player":fields.player,"country":fields.country,"date":fields.date,"speed":fields.speed,"paused":fields.paused,"difficulty":fields.difficulty,"historical_ai":fields.historical_raw!=0,"focus":fields.focus,"research":fields.research,"default_mode":"FAIR","observation_paused_game":false,"capabilities_unverified":["native_commands","production","construction","army","diplomacy","progress_days","daily_governor"],"freshness":{"observed_unix_ms":SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_millis(),"elapsed_ms":start.elapsed().as_secs_f64()*1000.0,"read_calls":r.calls.get(),"read_bytes":r.bytes.get()}})
}

struct SnapshotFields {
    anchors: [u64; 3],
    tag_id: i32,
    player: String,
    country: String,
    date: calendar::Date,
    speed: i32,
    paused: bool,
    difficulty: i32,
    historical_raw: i32,
    focus: Value,
    research: Vec<Value>,
}

// The caller owns the complete begin_snapshot/check bracket. This body keeps
// all structural readbacks and fresh bytes; it has no shared active/cached state.
fn snapshot_fields(r: &Reader<'_>, base: u64) -> Result<SnapshotFields> {
    let initial = roots(r, base)?;
    let [gs, idler, _] = initial;
    if r.u64(gs)? != base + 0x42c7598 || r.u64(idler)? != base + 0x42c9a88 {
        return Err(Error::new(
            "campaign_unavailable",
            "Verified current game and in-game idler required",
        ));
    }
    let raw = r.i32(gs + 0x460)?;
    let date = calendar::decode(raw)?;
    let controls = r.read(gs + 0x4b4, 4)?;
    let speed = integer(controls);
    let paused = r.byte(idler + 0x6c1)?;
    if !(0..=4).contains(&speed) || paused > 1 {
        return Err(Error::new(
            "invalid_clock",
            "Clock controls outside validated range",
        ));
    }
    let tag_id = r.i32(gs + 0x518)?;
    if !(0..4096).contains(&tag_id) {
        return Err(Error::new("player_unavailable", "Invalid player tag index"));
    }
    let country_index = integer(r.array_entry(gs + 0x340, tag_id as usize, 4)?);
    if country_index < 0 {
        return Err(Error::new(
            "player_unavailable",
            "Player country index absent",
        ));
    }
    let country = address(r.array_entry(gs + 0x310, country_index as usize, 8)?)?;
    r.object_type(country, "CCountry")?;
    let tag_db = pointer(r, base + 0x43f3eb8)?;
    // Reads only the player tag/name; foreign-country detail is never traversed.
    let (tag_header, tag_data, tag_count) = r.array_header(tag_db)?;
    if tag_id as usize >= tag_count {
        return Err(Error::new(
            "player_unavailable",
            "Dynamic player tag name is unverified",
        ));
    }
    let tag = r.string(tag_data + tag_id as u64 * 32)?;
    if tag.len() != 3
        || !tag.bytes().all(|v| v.is_ascii_uppercase())
        || r.read(tag_db, 16)? != tag_header
    {
        return Err(Error::new(
            "player_unavailable",
            "Player tag name changed or invalid",
        ));
    }
    let name = r.string(country + 0x10)?;
    let progress = pointer(r, country + 0x1348)?;
    r.object_type(progress, "CNationalFocusProgress")?;
    let current = r.u64(progress + 0x10)?;
    let focus = if current == 0 {
        Value::Null
    } else {
        r.object_type(current, "CNationalFocus")?;
        json!({"key":r.string(current+0x18)?,"name":r.string(current+0x60)?})
    };
    let technology_status = pointer(r, country + 0xf38)?;
    r.object_type(technology_status, "CTechnologyStatus")?;
    let slots = r.pointer_array(technology_status + 0xa0)?;
    if slots.len() > 32 {
        return Err(Error::new(
            "invalid_array",
            "Research slot count exceeds bound",
        ));
    }
    let mut research = Vec::new();
    let mut slot_tokens = Vec::new();
    for (index, slot) in slots.iter().copied().enumerate() {
        r.object_type(slot, "CResearchSlot")?;
        let technology = r.u64(slot + 0x18)?;
        slot_tokens.push(technology);
        let id = if technology == 0 {
            Value::Null
        } else {
            r.object_type(technology, "CTechnology")?;
            let template = pointer(r, technology + 0x160)?;
            r.object_type(template, "CTechnologyTemplate")?;
            json!(r.string(template + 8)?)
        };
        research.push(json!({"slot":index,"researching":id,"progress":null,"days_remaining":null}));
    }
    let difficulty = r.i32(gs + 0x620)?;
    let historical = r.i32(gs + 0x628)?;
    // Check ownership and anchors at snapshot boundaries, rather than spawning
    // per-field identity probes. A changed observation is refused, never cached.
    let end_slots = r.pointer_array(technology_status + 0xa0)?;
    let mut slots_changed = end_slots != slots;
    if !slots_changed {
        for (slot, token) in slots.iter().zip(slot_tokens) {
            if r.u64(*slot + 0x18)? != token {
                slots_changed = true;
                break;
            }
        }
    }
    if roots(r, base)? != initial
        || r.i32(gs + 0x460)? != raw
        || r.i32(gs + 0x518)? != tag_id
        || r.u64(progress + 0x10)? != current
        || slots_changed
        || r.i32(gs + 0x4b4)? != speed
        || r.byte(idler + 0x6c1)? != paused
        || integer(r.array_entry(gs + 0x340, tag_id as usize, 4)?) != country_index
        || address(r.array_entry(gs + 0x310, country_index as usize, 8)?)? != country
        || pointer(r, country + 0x1348)? != progress
        || pointer(r, country + 0xf38)? != technology_status
        || r.i32(gs + 0x620)? != difficulty
        || r.i32(gs + 0x628)? != historical
    {
        return Err(Error::new(
            "torn_read",
            "Campaign anchors changed during observation",
        ));
    }
    Ok(SnapshotFields {
        anchors: initial,
        tag_id,
        player: tag,
        country: name,
        date,
        speed,
        paused: paused == 1,
        difficulty,
        historical_raw: historical,
        focus,
        research,
    })
}
