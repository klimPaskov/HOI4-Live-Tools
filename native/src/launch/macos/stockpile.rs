//! `hoi4_stockpile`: the own equipment stockpile per archetype and per variant, what the own
//! production lines put out per day, what the fielded divisions still lack, and how many days the
//! stockpile lasts at the trend this server has measured. Pure memory read: no lease, no input,
//! never pauses.
//!
//! Mapped live on 9 November 2026 (Italy, 1 January 1936, macOS 1.19.4.1 DFA875A5):
//! - `CProductionStatus` (country +0xd10) +0x200 is the stockpile, a `CEquipmentVariantPool`
//!   (vtable 0x357cef8): +0x8 archetype rows {CEquipmentType*, i32, i32 variants, i64 amount}
//!   of 0x18 bytes, +0x20 variant rows {CEquipmentVariant*, i64 amount} of 0x10 bytes (amounts
//!   1/100000; the same pool shape as a division's held equipment). Checked against the files
//!   that built the stockpile: `history/countries/ITA - Italy.txt` adds 150 gw_armored_car,
//!   150 + 40 light_tank_chassis_0 (Fiat 3000, 3000B) and 180 light_tank_chassis_1: read 150,
//!   150, 40 and 180, and the light_tank_chassis archetype row is their sum 370 with 3 variants.
//! - The convoy stock is another pool of the same class at country +0xf98 (not listed here).
//! - Production per day comes from the lines (`production_lines::output`: IC of the last daily
//!   tick / unit cost), checked against the history progress of every starting line.
//! - The trend: each call keeps one sample per game day in this server process (31 days); the
//!   net change per day is measured between the newest sample and the oldest one up to 30 days
//!   back. Until two game days have been seen it is null.
use super::{ui::read, units};
use crate::{
    economy,
    error::{Error, Result},
    game,
    memory::Reader,
    platform::Process,
    production_lines,
    xlayout::{fld, img, Class},
};
use serde_json::{json, Map, Value};
use std::collections::{BTreeMap, VecDeque};
use std::sync::Mutex;

const FIXED: f64 = 100_000.0;
const PRODUCTION: Class = Class::new("CProductionStatus", 0x358e0c8);
const POOL: Class = Class::new("CEquipmentVariantPool", 0x357cef8);
const LINE: Class = Class::new("CProductionLine", 0x358ce80);
const VARIANT: Class = Class::new("CEquipmentVariant", 0x357d1b8);
const EQUIPMENT_TYPE: Class = Class::new("CEquipmentType", 0x357bad8);
const PRODUCTION_STOCKPILE: u64 = 0x200;
const POOL_VTABLE: u64 = 0x357cef8;
const POOL_ARCHETYPES: u64 = 0x8;
const POOL_VARIANTS: u64 = 0x20;
const ARCHETYPE_ROW: u64 = 0x18;
const ARCHETYPE_VARIANTS: u64 = 0xc;
const ARCHETYPE_AMOUNT: u64 = 0x10;
const VARIANT_ROW: u64 = 0x10;
const VARIANT_AMOUNT: u64 = 0x8;
const VARIANT_TYPE: u64 = 0x3e0;
const VARIANT_ID: u64 = 0xc;
const TYPE_KEY: u64 = 0x10;
const PRODUCTION_LINES: u64 = 0x58;
const LINE_VARIANT: u64 = 0x88;
const LINE_COST: u64 = 0x28;
const LINE_OUTPUT: u64 = 0x30;
const ROW_LIMIT: usize = 1024;
const LINE_LIMIT: usize = 2048;
const HOURS_PER_DAY: i64 = 24;
const TREND_DAYS: i64 = 30;
const SAMPLE_DAYS: usize = 31;

pub(super) fn tool() -> Value {
    json!({"name":"hoi4_stockpile","description":"Own equipment stockpile from game memory (no input, no lease, never pauses). Per archetype (infantry_equipment, support_equipment, artillery_equipment, motorized_equipment = trucks, light_tank_chassis, ...): stockpile (pieces in storage), variants (each design in storage: type key, design_id, amount), produced_per_day (what the own production lines put out on the last daily tick, with the lines and factories behind it), fielded_need / fielded_have / fielded_short (what the own divisions need, hold and still lack: the stockpile is drawn into them first), cover_after_shortfall (stockpile minus what the divisions lack; negative = the stockpile cannot even fill them), net_per_day (the change of the stockpile per game day measured by this server over up to 30 game days; null until it has seen two game days), consumption_per_day (produced minus net), days_of_cover (stockpile / daily loss; 0 when the divisions already lack more than is stored) with depleting (false = the stock is not falling, days_of_cover null). `equipment` limits the answer to archetypes or variant type keys (a string or a list). Stockpile amounts were checked against the history files that create them; the trend needs the game to run.","inputSchema":{"type":"object","properties":{"equipment":{"anyOf":[{"type":"string","maxLength":96},{"type":"array","items":{"type":"string","maxLength":96},"maxItems":32}]}},"additionalProperties":false}})
}

#[derive(Clone, Debug, Default, PartialEq)]
pub(super) struct Archetype {
    pub key: String,
    pub stock: f64,
    pub variant_count: i32,
}
#[derive(Clone, Debug, PartialEq)]
pub(super) struct Variant {
    pub key: String,
    pub design_id: i32,
    pub amount: f64,
}
#[derive(Clone, Debug, PartialEq)]
pub(super) struct LineRate {
    pub key: String,
    pub factories: i32,
    pub units_per_day: f64,
}
struct Read {
    hour: i64,
    playthrough: String,
    archetypes: Vec<Archetype>,
    variants: Vec<Variant>,
    lines: Vec<LineRate>,
    fielded: BTreeMap<String, (f64, f64)>,
}

fn is_ptr(p: u64) -> bool {
    (0x10000..=0x7fff_ffff_0000).contains(&p) && p % 8 == 0
}
fn amount(r: &Reader<'_>, a: u64) -> Result<f64> {
    Ok(r.u64(a)? as i64 as f64 / FIXED)
}
fn type_key(r: &Reader<'_>, image: u64, ty: u64) -> Result<String> {
    let abi = r.string_abi();
    if !is_ptr(ty) || r.u64(ty)? != image + img(abi, EQUIPMENT_TYPE.vtable)? {
        return Err(Error::new("invalid_type", "Exact equipment type required"));
    }
    r.string_bounded(ty + fld(abi, &EQUIPMENT_TYPE, TYPE_KEY)?, 128)
}
fn variant_key(r: &Reader<'_>, image: u64, v: u64) -> Result<(String, i32)> {
    let abi = r.string_abi();
    if !is_ptr(v) || r.u64(v)? != image + img(abi, VARIANT.vtable)? {
        return Err(Error::new(
            "invalid_type",
            "Exact equipment variant required",
        ));
    }
    let ty = r.u64(v + fld(abi, &VARIANT, VARIANT_TYPE)?)?;
    Ok((
        type_key(r, image, ty)?,
        r.i32(v + fld(abi, &VARIANT, VARIANT_ID)?)?,
    ))
}
fn bounded(count: usize, what: &str) -> Result<()> {
    if count > ROW_LIMIT {
        return Err(Error::new("invalid_array", format!("{what} exceeds bound")));
    }
    Ok(())
}

/// The stockpile pool of the own production status.
pub(super) fn pool(
    r: &Reader<'_>,
    image: u64,
    production: u64,
) -> Result<(Vec<Archetype>, Vec<Variant>)> {
    let abi = r.string_abi();
    pool_at(
        r,
        image,
        production + fld(abi, &PRODUCTION, PRODUCTION_STOCKPILE)?,
    )
}

/// One equipment pool (the stockpile, the convoy stock or a division's equipment).
pub(super) fn pool_at(
    r: &Reader<'_>,
    image: u64,
    pool: u64,
) -> Result<(Vec<Archetype>, Vec<Variant>)> {
    let abi = r.string_abi();
    if r.u64(pool)? != image + img(abi, POOL_VTABLE)? {
        return Err(Error::new(
            "invalid_type",
            "Equipment stockpile pool not found",
        ));
    }
    let f = |mac: u64| fld(abi, &POOL, mac);
    let (_, data, count) = r.array_header(pool + f(POOL_ARCHETYPES)?)?;
    bounded(count, "Archetype list")?;
    let mut archetypes = Vec::new();
    for i in 0..count as u64 {
        let row = data + i * f(ARCHETYPE_ROW)?;
        archetypes.push(Archetype {
            key: type_key(r, image, r.u64(row)?)?,
            stock: amount(r, row + f(ARCHETYPE_AMOUNT)?)?,
            variant_count: r.i32(row + f(ARCHETYPE_VARIANTS)?)?,
        });
    }
    let (_, data, count) = r.array_header(pool + f(POOL_VARIANTS)?)?;
    bounded(count, "Variant list")?;
    let mut variants = Vec::new();
    for i in 0..count as u64 {
        let row = data + i * f(VARIANT_ROW)?;
        let (key, design_id) = variant_key(r, image, r.u64(row)?)?;
        variants.push(Variant {
            key,
            design_id,
            amount: amount(r, row + f(VARIANT_AMOUNT)?)?,
        });
    }
    Ok((archetypes, variants))
}

/// Units per day of every own military and naval line (equipment type key of its design).
fn line_rates(r: &Reader<'_>, image: u64, production: u64) -> Result<Vec<LineRate>> {
    let abi = r.string_abi();
    let (_, data, count) = r.array_header(production + PRODUCTION_LINES)?;
    if count > LINE_LIMIT {
        return Err(Error::new("invalid_array", "Line list exceeds bound"));
    }
    let own = production_lines::own_lines(r, image)?;
    let mut out = Vec::new();
    for i in 0..count as u64 {
        let line = r.u64(data + i * 8)?;
        let handle = r.u64(line + 8)?;
        let Some(own) = own.iter().find(|l| l.handle == handle) else {
            continue;
        };
        let Ok((key, _)) = variant_key(r, image, r.u64(line + fld(abi, &LINE, LINE_VARIANT)?)?)
        else {
            continue;
        };
        let cost = amount(r, line + fld(abi, &LINE, LINE_COST)?)?;
        let output = amount(r, line + fld(abi, &LINE, LINE_OUTPUT)?)?;
        out.push(LineRate {
            key,
            factories: own.requested,
            units_per_day: if cost > 0.0 { output / cost } else { 0.0 },
        });
    }
    Ok(out)
}

/// What the own divisions need and hold per archetype (template need x divisions).
fn fielded(r: &Reader<'_>, image: u64) -> Result<BTreeMap<String, (f64, f64)>> {
    let templates = units::templates(r, image)?;
    let need: BTreeMap<u64, units::Totals> = templates
        .iter()
        .map(|t| (t.address, units::totals(&t.subunits, &t.support_units)))
        .collect();
    let mut out: BTreeMap<String, (f64, f64)> = BTreeMap::new();
    for d in units::divisions(r, image)? {
        let (Some(t), Some(held)) = (need.get(&d.template), d.held.as_ref()) else {
            continue;
        };
        for (k, n) in &t.need {
            let e = out.entry(k.clone()).or_default();
            e.0 += n;
            e.1 += held.get(k).copied().unwrap_or(0.0).min(*n);
        }
    }
    Ok(out)
}

fn observe(process: &dyn Process) -> Result<Read> {
    read(process, |r, image| {
        let (_, production) = economy::own_production(r, image)?;
        let context = game::context_from_reader(r, image)?;
        let (archetypes, variants) = pool(r, image, production)?;
        let lines = line_rates(r, image, production)?;
        let fielded = fielded(r, image).unwrap_or_default();
        Ok(Read {
            hour: context.date_raw as i64,
            playthrough: context.playthrough_id.clone(),
            archetypes,
            variants,
            lines,
            fielded,
        })
    })
}

/// The archetype a variant type key belongs to: the longest archetype key it starts with (or
/// contains, `gw_armored_car_equipment`), else a chassis whose stem it shares
/// (`light_tank_chassis` for `light_tank_equipment_2`).
pub(super) fn archetype_of<'a>(key: &str, archetypes: &'a [String]) -> Option<&'a String> {
    archetypes
        .iter()
        .filter(|a| key.starts_with(a.as_str()))
        .max_by_key(|a| a.len())
        .or_else(|| {
            archetypes
                .iter()
                .filter(|a| key.contains(a.as_str()))
                .max_by_key(|a| a.len())
        })
        .or_else(|| {
            archetypes.iter().find(|a| {
                a.strip_suffix("_chassis").is_some_and(|stem| {
                    key.starts_with(&format!("{stem}_equipment"))
                        || key.starts_with(&format!("{stem}_chassis"))
                })
            })
        })
}

/// Samples of one playthrough: (game hour, stock per archetype), one per game day.
#[derive(Default)]
struct Trend {
    playthrough: String,
    samples: VecDeque<(i64, BTreeMap<String, f64>)>,
}
static TREND: Mutex<Option<Trend>> = Mutex::new(None);

/// Keep this sample (one per game day; a later read of the same day replaces it) and return the
/// net change per day per archetype with the span in days it was measured over.
fn record(
    playthrough: &str,
    hour: i64,
    stock: &BTreeMap<String, f64>,
) -> BTreeMap<String, (f64, f64)> {
    let mut guard = TREND.lock().unwrap_or_else(|p| p.into_inner());
    let trend = guard.get_or_insert_with(Trend::default);
    if trend.playthrough != playthrough {
        *trend = Trend {
            playthrough: playthrough.to_string(),
            samples: VecDeque::new(),
        };
    }
    net_per_day(&mut trend.samples, hour, stock)
}

pub(super) fn net_per_day(
    samples: &mut VecDeque<(i64, BTreeMap<String, f64>)>,
    hour: i64,
    stock: &BTreeMap<String, f64>,
) -> BTreeMap<String, (f64, f64)> {
    // a date that went back (a loaded save) starts over
    if samples.back().is_some_and(|(h, _)| *h > hour) {
        samples.clear();
    }
    match samples.back_mut() {
        Some((h, s)) if *h / HOURS_PER_DAY == hour / HOURS_PER_DAY => {
            *h = hour;
            *s = stock.clone();
        }
        _ => samples.push_back((hour, stock.clone())),
    }
    while samples.len() > SAMPLE_DAYS {
        samples.pop_front();
    }
    let mut out = BTreeMap::new();
    let Some(oldest) = samples
        .iter()
        .find(|(h, _)| hour - *h <= TREND_DAYS * HOURS_PER_DAY)
    else {
        return out;
    };
    let days = (hour - oldest.0) as f64 / HOURS_PER_DAY as f64;
    if days < 1.0 {
        return out;
    }
    for (k, now) in stock {
        let then = oldest.1.get(k).copied().unwrap_or(0.0);
        out.insert(k.clone(), ((now - then) / days, days));
    }
    out
}

fn r2(v: f64) -> f64 {
    (v * 100.0).round() / 100.0
}

/// Days of cover from the stock, the net change per day and what the divisions lack:
/// (days, depleting). 0 when the divisions already lack more than is stored.
pub(super) fn cover(stock: f64, net: Option<f64>, short: f64) -> (Option<f64>, Option<bool>) {
    if short > stock + 1e-6 {
        return (Some(0.0), Some(true));
    }
    match net {
        Some(n) if n < -1e-9 => (Some(r2(stock / -n)), Some(true)),
        Some(_) => (None, Some(false)),
        None => (None, None),
    }
}

fn wanted(args: &Map<String, Value>) -> Result<Option<Vec<String>>> {
    match args.get("equipment") {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(s)) => Ok(Some(vec![s.clone()])),
        Some(Value::Array(a)) => a
            .iter()
            .map(|v| {
                v.as_str().map(str::to_string).ok_or_else(|| {
                    Error::new("invalid_arguments", "equipment takes keys (strings)")
                })
            })
            .collect::<Result<Vec<_>>>()
            .map(Some),
        Some(_) => Err(Error::new(
            "invalid_arguments",
            "equipment takes a key or a list of keys",
        )),
    }
}

pub(super) fn stockpile(process: &dyn Process, args: &Map<String, Value>) -> Result<Value> {
    if let Some(k) = args.keys().find(|k| k.as_str() != "equipment") {
        return Err(Error::new(
            "invalid_arguments",
            format!("Unexpected argument {k}"),
        ));
    }
    let wanted = wanted(args)?;
    let read = observe(process)?;
    let keys: Vec<String> = read.archetypes.iter().map(|a| a.key.clone()).collect();
    // Archetypes the divisions need but none is stored are listed with stock 0.
    let mut stock: BTreeMap<String, f64> = read
        .archetypes
        .iter()
        .map(|a| (a.key.clone(), a.stock))
        .collect();
    for k in read.fielded.keys() {
        stock.entry(k.clone()).or_insert(0.0);
    }
    let all: Vec<String> = stock.keys().cloned().collect();
    let trend = record(&read.playthrough, read.hour, &stock);
    let mut rows = Vec::new();
    for (key, amount) in &stock {
        let variants: Vec<&Variant> = read
            .variants
            .iter()
            .filter(|v| {
                archetype_of(&v.key, &keys).or_else(|| archetype_of(&v.key, &all)) == Some(key)
            })
            .collect();
        if let Some(w) = &wanted {
            let hit = w
                .iter()
                .any(|x| x == key || variants.iter().any(|v| &v.key == x));
            if !hit {
                continue;
            }
        }
        let lines: Vec<&LineRate> = read
            .lines
            .iter()
            .filter(|l| archetype_of(&l.key, &all) == Some(key))
            .collect();
        let produced: f64 = lines.iter().map(|l| l.units_per_day).sum();
        let (need, have) = read.fielded.get(key).copied().unwrap_or((0.0, 0.0));
        let short = (need - have).max(0.0);
        let net = trend.get(key).map(|(n, _)| *n);
        let (days, depleting) = cover(*amount, net, short);
        let archetype = read.archetypes.iter().find(|a| &a.key == key);
        rows.push(json!({
            "archetype": key,
            "stockpile": r2(*amount),
            "variant_count": archetype.map(|a| a.variant_count),
            "variants": variants.iter().map(|v| json!({"type": v.key, "design_id": v.design_id, "amount": r2(v.amount)})).collect::<Vec<_>>(),
            "produced_per_day": r2(produced),
            "production_lines": lines.len(),
            "factories": lines.iter().map(|l| l.factories).sum::<i32>(),
            "fielded_need": r2(need),
            "fielded_have": r2(have),
            "fielded_short": r2(short),
            "cover_after_shortfall": r2(amount - short),
            "net_per_day": net.map(r2),
            "trend_days": trend.get(key).map(|(_, d)| r2(*d)),
            "consumption_per_day": net.map(|n| r2((produced - n).max(0.0))),
            "days_of_cover": days,
            "depleting": depleting,
        }));
    }
    let trucks = rows
        .iter()
        .find(|r| r["archetype"] == "motorized_equipment")
        .cloned();
    Ok(json!({
        "source": "memory: own production status stockpile pool, production lines, divisions",
        "equipment": rows,
        "trucks": trucks,
        "trend_note": "net_per_day is measured by this server between game days it has seen (up to 30); it is null until the game has run past one more day",
        "validated": {"stockpile": "variant and archetype amounts matched the history files that created them (1 January 1936, Italy)",
            "produced_per_day": "line output matched every starting line's history progress after one daily tick",
            "fielded": "the template need and held equipment of hoi4_supply", "trend": "computed, not compared with the logistics tab"},
        "input_authorized": false,
        "observation_paused_game": false
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn keys(v: &[&str]) -> Vec<String> {
        v.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn variants_map_to_their_archetype() {
        let a = keys(&[
            "infantry_equipment",
            "armored_car_equipment",
            "light_tank_chassis",
            "motorized_equipment",
            "train_equipment",
        ]);
        assert_eq!(
            archetype_of("infantry_equipment_1", &a).unwrap(),
            "infantry_equipment"
        );
        assert_eq!(
            archetype_of("gw_armored_car_equipment", &a).unwrap(),
            "armored_car_equipment"
        );
        assert_eq!(
            archetype_of("light_tank_chassis_1", &a).unwrap(),
            "light_tank_chassis"
        );
        assert_eq!(
            archetype_of("light_tank_equipment_2", &a).unwrap(),
            "light_tank_chassis"
        );
        assert_eq!(
            archetype_of("motorized_equipment_1", &a).unwrap(),
            "motorized_equipment"
        );
        assert_eq!(archetype_of("ship_hull_heavy_1", &a), None);
    }

    #[test]
    fn the_trend_needs_a_game_day_and_follows_the_oldest_sample() {
        let mut s = VecDeque::new();
        let st = |n: f64| BTreeMap::from([("infantry_equipment".to_string(), n)]);
        assert!(net_per_day(&mut s, 1000 * 24, &st(6000.0)).is_empty());
        // same day: replaced, still no trend
        assert!(net_per_day(&mut s, 1000 * 24 + 10, &st(5990.0)).is_empty());
        assert_eq!(s.len(), 1);
        let t = net_per_day(&mut s, 1002 * 24 + 10, &st(5790.0));
        assert_eq!(t["infantry_equipment"], (-100.0, 2.0));
        // a loaded older save starts over
        assert!(net_per_day(&mut s, 900 * 24, &st(1.0)).is_empty());
        assert_eq!(s.len(), 1);
    }

    #[test]
    fn samples_older_than_thirty_days_are_left_out() {
        let mut s = VecDeque::new();
        let st = |n: f64| BTreeMap::from([("x".to_string(), n)]);
        for day in 0..40 {
            net_per_day(&mut s, day * 24, &st(1000.0 - 10.0 * day as f64));
        }
        assert!(s.len() <= SAMPLE_DAYS);
        let t = net_per_day(&mut s, 40 * 24, &st(600.0));
        assert_eq!(t["x"], (-10.0, 30.0));
    }

    #[test]
    fn cover_is_zero_when_the_divisions_lack_more_than_is_stored() {
        assert_eq!(cover(100.0, Some(-10.0), 0.0), (Some(10.0), Some(true)));
        assert_eq!(cover(100.0, Some(5.0), 0.0), (None, Some(false)));
        assert_eq!(cover(100.0, Some(0.0), 0.0), (None, Some(false)));
        assert_eq!(cover(100.0, None, 0.0), (None, None));
        assert_eq!(cover(100.0, None, 150.0), (Some(0.0), Some(true)));
    }
}
