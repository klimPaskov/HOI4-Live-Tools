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
/// Overview offsets of one inspected build. Image offsets are relative to the
/// verified image base; the rest are object field offsets.
#[derive(Debug)]
pub(crate) struct Layout {
    pub(crate) gs_global: u64,
    pub(crate) idler_global: u64,
    pub(crate) technology_db_global: u64,
    tag_db_global: u64,
    pub(crate) gs_vtable: u64,
    pub(crate) idler_vtable: u64,
    playthrough: u64,
    pub(crate) date: u64,
    speed: u64,
    paused: u64,
    pub(crate) player_tag: u64,
    pub(crate) tag_country_index: u64,
    pub(crate) countries: u64,
    difficulty: u64,
    historical: u64,
    tag_stride: u64,
    pub(crate) country_focus_progress: u64,
    pub(crate) country_technology_status: u64,
    pub(crate) country_vtable: u64,
    pub(crate) country_production: u64,
    pub(crate) country_politics: u64,
    pub(crate) country_focus_tree: u64,
    pub(crate) production_vtable: u64,
    pub(crate) politics_vtable: u64,
    /// Image ranges holding primary vtables/type information and type names.
    pub(crate) type_metadata: (u64, u64),
    pub(crate) type_names: (u64, u64),
    focus_name: u64,
    pub(crate) build_id: &'static str,
    source: &'static str,
}
/// Linux ELF a4971c9e (libstdc++).
pub(crate) const LINUX: Layout = Layout {
    gs_global: 0x44086a8,
    idler_global: 0x44085f8,
    technology_db_global: 0x439f450,
    tag_db_global: 0x43f3eb8,
    gs_vtable: 0x42c7598,
    idler_vtable: 0x42c9a88,
    playthrough: 0x718,
    date: 0x460,
    speed: 0x4b4,
    paused: 0x6c1,
    player_tag: 0x518,
    tag_country_index: 0x340,
    countries: 0x310,
    difficulty: 0x620,
    historical: 0x628,
    tag_stride: 32,
    country_focus_progress: 0x1348,
    country_technology_status: 0xf38,
    country_vtable: 0x41fafb0,
    country_production: 0xf40,
    country_politics: 0xf68,
    country_focus_tree: 0x1338,
    production_vtable: 0x42066f8,
    politics_vtable: 0x4202e30,
    type_metadata: (0x41adde0, 0x436d9f8),
    type_names: (0x3506000, 0x3975f39),
    focus_name: 0x60,
    build_id: BUILD_ID,
    source: "native process_vm_readv",
};
/// macOS Mach-O 089EA519 (libc++), mapped live on 18 October 2026 from RTTI
/// roots, value changes across unpause, speed and player switches, and RTTI joins.
pub(crate) const MACOS: Layout = Layout {
    gs_global: 0x3519740,
    idler_global: 0x35196c0,
    technology_db_global: 0x34d0de0,
    tag_db_global: 0x3506580,
    gs_vtable: 0x3306260,
    idler_vtable: 0x3308f90,
    playthrough: 0x6c8,
    date: 0x428,
    speed: 0x47c,
    paused: 0x679,
    player_tag: 0x4d8,
    tag_country_index: 0x308,
    countries: 0x2d8,
    difficulty: 0x5d0,
    historical: 0x5d8,
    tag_stride: 24,
    country_focus_progress: 0x1108,
    country_technology_status: 0xd08,
    country_vtable: 0x337bc38,
    country_production: 0xd10,
    country_politics: 0xd38,
    country_focus_tree: 0x10f8,
    production_vtable: 0x338b678,
    politics_vtable: 0x3387148,
    type_metadata: (0x32ade40, 0x34c8ab8),
    type_names: (0x2cfc2a0, 0x2f18588),
    focus_name: 0x50,
    build_id: "089EA519-6780-353B-8E39-4E9309B444D4",
    source: "native mach_vm_read",
};
/// Each supported string ABI belongs to exactly one inspected build.
pub(crate) fn layout(r: &Reader<'_>) -> &'static Layout {
    match r.string_abi() {
        crate::memory::StringAbi::LibStdCxx => &LINUX,
        crate::memory::StringAbi::LibCxx => &MACOS,
    }
}
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
/// The caller owns the begin/end process bracket and any encompassing field
/// witnesses. Used to bind UI observations to the same fresh campaign read.
pub(crate) fn context_from_reader(r: &Reader<'_>, base: u64) -> Result<ClockContext> {
    Ok(context_fields(r, base)?.1)
}
fn context_fields(r: &Reader<'_>, base: u64) -> Result<(SnapshotFields, ClockContext)> {
    let l = layout(r);
    let initial = roots(r, base)?;
    let [gs, idler, _] = initial;
    if r.u64(gs)? != base + l.gs_vtable || r.u64(idler)? != base + l.idler_vtable {
        return Err(Error::new(
            "campaign_unavailable",
            "Current in-game context required",
        ));
    }
    // Current-build getter 0x200e720 returns GS+0x718. The generator and
    // telemetry caller identify this string as playthrough_id. Serialization
    // can restore it, so no claim is made that every reload changes it.
    let playthrough_id = r.string(gs + l.playthrough)?;
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
    let historical_raw = r.i32(gs + l.historical)?;
    // Use the same local read bracket as the campaign identity. Calling the
    // public snapshot here would refresh maps and check ownership twice.
    let observed = snapshot_fields(r, base)?;
    let date_raw = observed.date.raw;
    if observed.anchors != initial
        || observed.historical_raw != historical_raw
        || roots(r, base)? != initial
        || r.string(gs + l.playthrough)? != playthrough_id
        || r.i32(gs + l.historical)? != historical_raw
        || r.i32(gs + l.date)? != date_raw
        || r.i32(gs + l.speed)? != observed.speed
        || r.byte(idler + l.paused)? != u8::from(observed.paused)
        || r.i32(gs + l.difficulty)? != observed.difficulty
        || r.i32(gs + l.player_tag)? != observed.tag_id
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
    let l = layout(reader);
    Ok([
        pointer(reader, base + l.gs_global)?,
        pointer(reader, base + l.idler_global)?,
        pointer(reader, base + l.technology_db_global)?,
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
    let l = layout(r);
    json!({"source":l.source,"owned":true,"build_id":l.build_id,"player":fields.player,"country":fields.country,"date":fields.date,"speed":fields.speed,"paused":fields.paused,"difficulty":fields.difficulty,"historical_ai":fields.historical_raw!=0,"focus":fields.focus,"research":fields.research,"default_mode":"FAIR","observation_paused_game":false,"capabilities_unverified":["native_commands","production","construction","army","diplomacy","progress_days","daily_governor"],"freshness":{"observed_unix_ms":SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_millis(),"elapsed_ms":start.elapsed().as_secs_f64()*1000.0,"read_calls":r.calls.get(),"read_bytes":r.bytes.get()}})
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
    let l = layout(r);
    let initial = roots(r, base)?;
    let [gs, idler, _] = initial;
    if r.u64(gs)? != base + l.gs_vtable || r.u64(idler)? != base + l.idler_vtable {
        return Err(Error::new(
            "campaign_unavailable",
            "Verified current game and in-game idler required",
        ));
    }
    let raw = r.i32(gs + l.date)?;
    let date = calendar::decode(raw)?;
    let controls = r.read(gs + l.speed, 4)?;
    let speed = integer(controls);
    let paused = r.byte(idler + l.paused)?;
    if !(0..=4).contains(&speed) || paused > 1 {
        return Err(Error::new(
            "invalid_clock",
            "Clock controls outside validated range",
        ));
    }
    let tag_id = r.i32(gs + l.player_tag)?;
    if !(1..4096).contains(&tag_id) {
        return Err(Error::new("player_unavailable", "Invalid player tag index"));
    }
    let country_index = integer(r.array_entry(gs + l.tag_country_index, tag_id as usize, 4)?);
    if country_index < 0 {
        return Err(Error::new(
            "player_unavailable",
            "Player country index absent",
        ));
    }
    let country = address(r.array_entry(gs + l.countries, country_index as usize, 8)?)?;
    r.object_type(country, "CCountry")?;
    let tag_db = pointer(r, base + l.tag_db_global)?;
    // Reads only the player tag/name; foreign-country detail is never traversed.
    let (tag_header, tag_data, tag_count) = r.array_header(tag_db)?;
    if tag_id as usize >= tag_count {
        return Err(Error::new(
            "player_unavailable",
            "Dynamic player tag name is unverified",
        ));
    }
    let tag = r.string(tag_data + tag_id as u64 * l.tag_stride)?;
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
    let progress = pointer(r, country + l.country_focus_progress)?;
    r.object_type(progress, "CNationalFocusProgress")?;
    let current = r.u64(progress + 0x10)?;
    let focus = if current == 0 {
        Value::Null
    } else {
        r.object_type(current, "CNationalFocus")?;
        json!({"key":r.string(current+0x18)?,"name":r.string(current+l.focus_name)?})
    };
    let technology_status = pointer(r, country + l.country_technology_status)?;
    r.object_type(technology_status, "CTechnologyStatus")?;
    if r.u64(technology_status + 0x130)? != country || r.i32(technology_status + 0x138)? != tag_id {
        return Err(Error::new(
            "invalid_research_owner",
            "Own research status country and positive tag required",
        ));
    }
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
        if r.u64(slot + 0x10)? != technology_status {
            return Err(Error::new(
                "invalid_research_owner",
                "Own research slot required before active technology",
            ));
        }
        let technology = r.u64(slot + 0x18)?;
        slot_tokens.push(technology);
        let id = if technology == 0 {
            Value::Null
        } else {
            r.object_type(technology, "CTechnology")?;
            if r.u64(technology + 0x158)? != technology_status {
                return Err(Error::new(
                    "invalid_research_owner",
                    "Own active technology required before template payload",
                ));
            }
            let template = pointer(r, technology + 0x160)?;
            r.object_type(template, "CTechnologyTemplate")?;
            json!(r.string_bounded(template + 8, 256)?)
        };
        research.push(json!({"slot":index,"researching":id,"progress":null,"days_remaining":null}));
    }
    let difficulty = r.i32(gs + l.difficulty)?;
    let historical = r.i32(gs + l.historical)?;
    // Check ownership and anchors at snapshot boundaries, rather than spawning
    // per-field identity probes. A changed observation is refused, never cached.
    if r.u64(technology_status + 0x130)? != country || r.i32(technology_status + 0x138)? != tag_id {
        return Err(Error::new("torn_read", "Own research status changed"));
    }
    let end_slots = r.pointer_array(technology_status + 0xa0)?;
    let mut slots_changed = end_slots != slots;
    if !slots_changed {
        for (slot, token) in slots.iter().zip(slot_tokens) {
            if r.u64(*slot + 0x10)? != technology_status
                || r.u64(*slot + 0x18)? != token
                || (token != 0 && r.u64(token + 0x158)? != technology_status)
            {
                slots_changed = true;
                break;
            }
        }
    }
    if roots(r, base)? != initial
        || r.i32(gs + l.date)? != raw
        || r.i32(gs + l.player_tag)? != tag_id
        || r.u64(progress + 0x10)? != current
        || slots_changed
        || r.i32(gs + l.speed)? != speed
        || r.byte(idler + l.paused)? != paused
        || integer(r.array_entry(gs + l.tag_country_index, tag_id as usize, 4)?) != country_index
        || address(r.array_entry(gs + l.countries, country_index as usize, 8)?)? != country
        || pointer(r, country + l.country_focus_progress)? != progress
        || pointer(r, country + l.country_technology_status)? != technology_status
        || r.i32(gs + l.difficulty)? != difficulty
        || r.i32(gs + l.historical)? != historical
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
