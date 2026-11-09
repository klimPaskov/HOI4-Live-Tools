//! Output, progress and efficiency of one own production line (macOS 1.19.4.1 layout; on a
//! build without these fields every value is null with the reason).
//!
//! Mapped live on 9 November 2026 (Italy, 1 January 1936, after the first daily tick) and
//! against the game code (Mach-O DFA875A5):
//! - line +0x28 cost of one unit, +0x30 IC put into the line on the last daily tick, +0x38 IC
//!   accumulated toward the unit in progress (all i64, 1/100000). Every historical line of
//!   `history/units/ITA_1936_nsb.txt` and `ITA_1936_naval_mtg.txt` matched exactly:
//!   progress = history progress x cost + one day's output (ship_hull_heavy_1 0.70 x 8351.25 +
//!   4.764 = 5850.639; infantry 0.1 x 0.5 + 6.243 - 12 units x 0.5 = 0.293).
//! - line +0x48 units still to build (i32; -1 = until stopped, the military default); every
//!   historical ship line reads 1 = its `amount = 1`.
//! - line +0xc0 per factory slot efficiency (array of i64, percent at 1/32768: the engine's
//!   1/100000 -> 1/32768 conversion at 0x102c21a10). The game grows only the active slots
//!   (loop bound line +0x14) and clamps them to the cap.
//! - cap (function 0x101468b2a) = max(0, BASE_FACTORY_MAX_EFFICIENCY_FACTOR + 100 x (country
//!   modifier 0xa9 + the line's organisation bonus BONUS_PRODUCTION_EFFICIENCY_CAP_FACTOR));
//!   a new line starts at BASE_FACTORY_START_EFFICIENCY_FACTOR + 100 x modifier 0xab; naval
//!   lines are fixed at 100 (no efficiency). Italy: modifiers 0xa9 = -0.10 and 0xab = -0.05, the
//!   Beretta line bonus +0.03: cap 43 for infantry and 40 for support and trucks, which is
//!   exactly where their active slots stand after the first day, and new lines read 5.0.
//! - country +0x560 the modifier table {i32 id, i32, i64 value 1/100000} (size at +0x56c);
//!   line +0xa0 the organisation bonus table {i32 key, i32, i64 value} (size +0xac); bonus
//!   keys at image 0x372bec0 + kind x 32 + 0x18 (kind 2 = efficiency cap); the two defines at
//!   image 0x371dae8 (start) and 0x371daf8 (max), read live so a mod's defines are honoured.
use crate::{
    error::Result,
    memory::{Reader, StringAbi},
    xlayout::{fld, img, Class},
};
use serde_json::{json, Value};

const FIXED: f64 = 100_000.0;
const SLOT_SCALE: f64 = 32_768.0;
const LINE: Class = Class::new("CProductionLine", 0x358ce80);
const COUNTRY: Class = Class::new("CCountry", 0x357e388);
const BONUS: Class = Class::new("NIndustrialOrganisation::CTraitBonus", 0x358c418);
const LINE_COST: u64 = 0x28;
const LINE_OUTPUT: u64 = 0x30;
const LINE_PROGRESS: u64 = 0x38;
const LINE_AMOUNT: u64 = 0x48;
const LINE_ACTIVE: u64 = 0x14;
const LINE_SLOTS: u64 = 0xc0;
const LINE_BONUS: u64 = 0x98;
const BONUS_DATA: u64 = 0x8;
const BONUS_SIZE: u64 = 0x14;
const COUNTRY_MODIFIERS: u64 = 0x560;
const COUNTRY_MODIFIER_COUNT: u64 = 0x56c;
const MODIFIER_MAX_EFFICIENCY: i32 = 0xa9;
const MODIFIER_START_EFFICIENCY: i32 = 0xab;
const DEFINE_START_EFFICIENCY: u64 = 0x371dae8;
const DEFINE_MAX_EFFICIENCY: u64 = 0x371daf8;
const BONUS_KEYS: u64 = 0x372bec0;
const BONUS_KIND_CAP: u64 = 2;
const SLOT_LIMIT: usize = 256;
const TABLE_LIMIT: usize = 4096;

/// Country-wide inputs of the efficiency formula, read once per snapshot.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Efficiency {
    /// Base cap in percent (define) plus 100 x the country's cap modifier.
    pub cap_base: f64,
    /// Start efficiency of a new factory slot in percent.
    pub start: f64,
    /// Key of the organisation bonus that raises a line's cap.
    pub cap_bonus_key: i32,
}

/// Rounded to 3 decimals.
fn r3(v: f64) -> f64 {
    (v * 1000.0).round() / 1000.0
}

/// A modifier value of an own country (0 when the country has none).
pub(crate) fn country_modifier(r: &Reader<'_>, country: u64, id: i32) -> Result<f64> {
    let abi = r.string_abi();
    let data = r.u64(country + fld(abi, &COUNTRY, COUNTRY_MODIFIERS)?)?;
    let count = r.i32(country + fld(abi, &COUNTRY, COUNTRY_MODIFIER_COUNT)?)?;
    if !(0..=TABLE_LIMIT as i32).contains(&count) || (count > 0 && data % 8 != 0) {
        return Err(crate::error::Error::new(
            "invalid_modifier_table",
            "Country modifier table out of bounds",
        ));
    }
    let bytes = if count == 0 {
        Vec::new()
    } else {
        r.read(data, count as usize * 16)?
    };
    Ok(lookup(&bytes, id).unwrap_or(0.0))
}

/// Value of `key` in a table of 16-byte {i32 key, i32, i64 value 1/100000} rows.
fn lookup(bytes: &[u8], key: i32) -> Option<f64> {
    bytes.chunks_exact(16).find_map(|row| {
        (i32::from_le_bytes(row[..4].try_into().unwrap()) == key)
            .then(|| i64::from_le_bytes(row[8..].try_into().unwrap()) as f64 / FIXED)
    })
}

/// The country-wide part of the efficiency formula for the own `country`.
pub(crate) fn efficiency_inputs(r: &Reader<'_>, base: u64, country: u64) -> Result<Efficiency> {
    let abi = r.string_abi();
    let max = r.i32(base + img(abi, DEFINE_MAX_EFFICIENCY)?)? as f64;
    let start = r.i32(base + img(abi, DEFINE_START_EFFICIENCY)?)? as f64;
    let cap_mod = country_modifier(r, country, MODIFIER_MAX_EFFICIENCY)?;
    let start_mod = country_modifier(r, country, MODIFIER_START_EFFICIENCY)?;
    let cap_bonus_key = r.i32(base + img(abi, BONUS_KEYS)? + BONUS_KIND_CAP * 32 + 0x18)?;
    Ok(Efficiency {
        cap_base: max + 100.0 * cap_mod,
        start: start + 100.0 * start_mod,
        cap_bonus_key,
    })
}

/// Efficiency of the active slots: (mean, min) in percent; the first slot when none is active.
fn slot_efficiency(slots: &[f64], active: usize) -> Option<(f64, f64)> {
    let used = &slots[..active.min(slots.len()).max(slots.len().min(1))];
    if used.is_empty() {
        return None;
    }
    let mean = used.iter().sum::<f64>() / used.len() as f64;
    let min = used.iter().cloned().fold(f64::INFINITY, f64::min);
    Some((mean, min))
}

/// The line's output object (see the module doc). `naval` lines have no efficiency.
pub(crate) fn observe(
    r: &Reader<'_>,
    line: u64,
    naval: bool,
    inputs: Option<&Efficiency>,
) -> Value {
    match read(r, line, naval, inputs) {
        Ok(v) => v,
        Err(e) => json!({"available": false, "reason": e.code, "detail": e.message}),
    }
}

fn read(r: &Reader<'_>, line: u64, naval: bool, inputs: Option<&Efficiency>) -> Result<Value> {
    let abi = r.string_abi();
    if abi == StringAbi::LibStdCxx {
        return Err(crate::error::Error::new(
            "layout_unmapped",
            "Line output fields are mapped for the macOS build only",
        ));
    }
    let f = |mac: u64| fld(abi, &LINE, mac);
    let cost = r.u64(line + f(LINE_COST)?)? as i64 as f64 / FIXED;
    let output = r.u64(line + f(LINE_OUTPUT)?)? as i64 as f64 / FIXED;
    let progress = r.u64(line + f(LINE_PROGRESS)?)? as i64 as f64 / FIXED;
    let amount = r.i32(line + f(LINE_AMOUNT)?)?;
    let active = r.i32(line + f(LINE_ACTIVE)?)?.clamp(0, SLOT_LIMIT as i32) as usize;
    let mut v = json!({
        "unit_cost": r3(cost),
        "ic_last_day": r3(output),
        "units_per_day": (cost > 0.0).then(|| r3(output / cost)),
        "progress_ic": r3(progress),
        "progress": (cost > 0.0).then(|| r3((progress / cost).clamp(0.0, 1.0))),
        "days_to_next_unit": (output > 0.0 && cost > 0.0).then(|| r3(((cost - progress).max(0.0)) / output)),
        "units_left": (amount >= 0).then_some(amount),
        "available": true,
    });
    if naval {
        v["efficiency"] = Value::Null;
        v["efficiency_note"] = json!("naval lines have no production efficiency");
        return Ok(v);
    }
    let (_, data, count) = r.array_header(line + f(LINE_SLOTS)?)?;
    let count = count.min(SLOT_LIMIT);
    let bytes = if count == 0 {
        Vec::new()
    } else {
        r.read(data, count * 8)?
    };
    let slots: Vec<f64> = bytes
        .chunks_exact(8)
        .map(|b| i64::from_le_bytes(b.try_into().unwrap()) as f64 / SLOT_SCALE)
        .collect();
    let (mean, min) = slot_efficiency(&slots, active).unwrap_or((0.0, 0.0));
    v["efficiency"] = json!(r3(mean));
    v["efficiency_min"] = json!(r3(min));
    if let Some(inputs) = inputs {
        let bonus = line + f(LINE_BONUS)?;
        let bonus_data = r.u64(bonus + fld(abi, &BONUS, BONUS_DATA)?)?;
        let bonus_count = r.i32(bonus + fld(abi, &BONUS, BONUS_SIZE)?)?;
        let bonus = if (1..=64).contains(&bonus_count) {
            lookup(
                &r.read(bonus_data, bonus_count as usize * 16)?,
                inputs.cap_bonus_key,
            )
            .unwrap_or(0.0)
        } else {
            0.0
        };
        let cap = (inputs.cap_base + 100.0 * bonus).max(0.0);
        v["efficiency_cap"] = json!(r3(cap));
        v["efficiency_start"] = json!(r3(inputs.start));
        v["at_cap"] = json!(mean + 0.05 >= cap);
    }
    Ok(v)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(key: i32, value: i64) -> Vec<u8> {
        let mut b = key.to_le_bytes().to_vec();
        b.extend([0u8; 4]);
        b.extend(value.to_le_bytes());
        b
    }

    #[test]
    fn modifier_rows_are_found_by_key() {
        let mut t = row(0xa9, -10_000);
        t.extend(row(0xab, -5_000));
        assert_eq!(lookup(&t, 0xa9), Some(-0.1));
        assert_eq!(lookup(&t, 0xab), Some(-0.05));
        assert_eq!(lookup(&t, 7), None);
    }

    #[test]
    fn efficiency_is_the_mean_of_the_active_slots() {
        // Italy's historical infantry line after one day: 4 active slots at 43, the rest at 42.
        let mut slots = vec![43.0; 4];
        slots.extend(vec![42.0; 146]);
        assert_eq!(slot_efficiency(&slots, 4), Some((43.0, 43.0)));
        // no factory working: the first slot stands for the line
        assert_eq!(slot_efficiency(&slots[4..], 0), Some((42.0, 42.0)));
        assert_eq!(slot_efficiency(&[], 3), None);
        assert_eq!(slot_efficiency(&[40.0, 20.0], 2), Some((30.0, 20.0)));
    }

    #[test]
    fn slot_scale_is_one_32768th_of_a_percent() {
        // raw slot values read live: 0x158000 (43), 0x150000 (42), 0x320000 (naval 100), 0x28000 (5)
        assert_eq!(0x158000 as f64 / SLOT_SCALE, 43.0);
        assert_eq!(0x150000 as f64 / SLOT_SCALE, 42.0);
        assert_eq!(0x320000 as f64 / SLOT_SCALE, 100.0);
        assert_eq!(0x28000 as f64 / SLOT_SCALE, 5.0);
    }
}
