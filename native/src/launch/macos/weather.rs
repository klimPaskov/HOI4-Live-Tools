//! Weather and ground by strategic region, an EVALUATOR from the map's own weather tables (engineer 110, 28 Nov
//! 2026). Nothing here reads game memory: the live weather of a province is not read yet; this is what the game's
//! files make likely for a region on a day of the year, for any map or mod that ships the same files.
//!
//! The game's rules (1.20.1.0):
//! * `map/strategicregions/*.txt` `weather = { period = { between = { D.M D.M } temperature = { lo hi }
//!   no_phenomenon rain_light rain_heavy snow blizzard sandstorm mud min_snow_level } }`: per region and period the
//!   phenomena's weights, the temperature range and the region's mud factor.
//! * `common/weather.txt` `settings`: temperature classes (`weather_extreme_cold` ...), mud from collected rain
//!   (`water_gain_on_rain_light/heavy`, `water_gain_max`, the cutoff `water_gain_min`, drying by temperature
//!   `water_dryout_temperature/multiplier`, frozen below `min_temperature`), ground snow from snowfall
//!   (`snow_gain_on_snowing/blizzard`, `snow_gain_max`, melting `snow_melt_temperature/multiplier`, snowing only
//!   below `temperature`'s first value, `weather_ground_snow_medium/high` bands).
//! * `common/modifiers/00_static_modifiers.txt` `weather_*`: what each phenomenon and ground state does (1.20.1.0:
//!   mud attack -40% speed -50%, blizzard attack speed -25% supply -25%, deep ground snow attack -40% speed -70%,
//!   extreme cold attack -20% breakthrough -30%).
//!
//! The model (an estimate, stated as such in every answer): a day's phenomenon chances are the period's weights
//! normalised; rain falls on the part of the temperature range above the snowing limit, snow below it; water and
//! snow are collected and dried / melted day by day at the period's mean temperature over two simulated years (the
//! second is kept); a region is in mud while its water is above the cutoff and the mean is above freezing, with a
//! chance of the period's mud factor (capped at 1); ground snow takes the band its collected snow falls in; the
//! temperature classes take the share of the range inside each band. The expected modifiers are those chances times
//! the static modifiers.
use crate::error::{Error, Result};
use crate::script::{self, Block, Entry, Val};
use serde_json::{json, Map, Value};
use std::collections::{BTreeMap, HashMap};
use std::path::Path;
use std::sync::Arc;

pub(super) const TOOL: &str = "hoi4_weather";
const MONTH_DAYS: [usize; 12] = [31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31];
/// The phenomena a period weighs, in the order of `Period::w`.
const PHENOMENA: [&str; 6] = [
    "no_phenomenon",
    "rain_light",
    "rain_heavy",
    "snow",
    "blizzard",
    "sandstorm",
];
/// The modifiers the war and air lanes judge by.
const KEYS: [&str; 9] = [
    "army_attack_factor",
    "army_speed_factor",
    "army_attack_speed_factor",
    "breakthrough_factor",
    "supply_factor",
    "attrition",
    "winter_attrition",
    "air_mission_efficiency",
    "org_loss_when_moving",
];

#[derive(Clone, Debug, PartialEq)]
pub(super) struct Period {
    /// day of the year (0-based) the period starts and ends on, inclusive
    pub from: usize,
    pub to: usize,
    pub t: (f64, f64),
    pub w: [f64; 6],
    pub mud: f64,
}
#[derive(Clone, Debug, Default, PartialEq)]
pub(super) struct Settings {
    pub bands: Vec<(String, f64, f64)>,
    pub rain_light: f64,
    pub rain_heavy: f64,
    pub water_max: f64,
    pub water_min: f64,
    pub dry_t: (f64, f64),
    pub dry_m: (f64, f64),
    pub mud_min_t: f64,
    pub snow_gain: f64,
    pub blizzard_gain: f64,
    pub snow_max: f64,
    pub melt_t: (f64, f64),
    pub melt_m: (f64, f64),
    pub snow_t: f64,
    pub ground_medium: (f64, f64),
    pub ground_high: (f64, f64),
}
#[derive(Default)]
pub(super) struct Weather {
    pub regions: BTreeMap<i32, Vec<Period>>,
    pub settings: Settings,
    pub mods: HashMap<String, Vec<(String, f64)>>,
}
/// One region on one day of the year.
#[derive(Clone, Debug, PartialEq)]
pub(super) struct Day {
    pub mud: f64,
    pub ground: &'static str,
    pub water: f64,
    pub snow: f64,
    pub chances: Vec<(&'static str, f64)>,
    pub bands: Vec<(String, f64)>,
    pub expected: BTreeMap<String, f64>,
}

fn pair(b: &[Entry], key: &str) -> Option<(f64, f64)> {
    let w = b.words(key);
    Some((w.first()?.parse().ok()?, w.get(1)?.parse().ok()?))
}
/// "D.M" (0-based day and month) -> day of the year.
pub(super) fn day_of(dm: &str) -> Option<usize> {
    let (d, m) = dm.split_once('.')?;
    let (d, m): (usize, usize) = (d.parse().ok()?, m.parse().ok()?);
    (m < 12).then(|| MONTH_DAYS[..m].iter().sum::<usize>() + d.min(MONTH_DAYS[m] - 1))
}
/// The periods of one strategic region file.
pub(super) fn region_periods(entries: &[Entry]) -> Option<(i32, Vec<Period>)> {
    let region = entries.find_val("strategic_region")?.block();
    let id: i32 = region.text("id")?.parse().ok()?;
    let mut out = Vec::new();
    for p in region
        .find_val("weather")
        .map(Val::block)
        .unwrap_or(&[])
        .all("period")
    {
        let b = p.block();
        let between = b.words("between");
        let (Some(from), Some(to)) = (
            between.first().and_then(|s| day_of(s)),
            between.get(1).and_then(|s| day_of(s)),
        ) else {
            continue;
        };
        let Some(t) = pair(b, "temperature") else {
            continue;
        };
        let mut w = [0.0; 6];
        for (i, k) in PHENOMENA.iter().enumerate() {
            w[i] = b.number(k).unwrap_or(0.0);
        }
        out.push(Period {
            from,
            to,
            t,
            w,
            mud: b.number("mud").unwrap_or(0.0),
        });
    }
    (!out.is_empty()).then_some((id, out))
}
/// `common/weather.txt` settings.
pub(super) fn settings(entries: &[Entry]) -> Settings {
    let s = entries.find_val("settings").map(Val::block).unwrap_or(&[]);
    let t = s.find_val("temperature").map(Val::block).unwrap_or(&[]);
    let mud = s.find_val("mud").map(Val::block).unwrap_or(&[]);
    let snow = s.find_val("snowing").map(Val::block).unwrap_or(&[]);
    let bands = [
        "weather_extreme_cold",
        "weather_very_cold",
        "weather_very_hot",
        "weather_extreme_hot",
    ]
    .iter()
    .filter_map(|k| pair(t, k).map(|(a, b)| (k.to_string(), a, b)))
    .collect();
    let n = |b: &[Entry], k: &str| b.number(k).unwrap_or(0.0);
    Settings {
        bands,
        rain_light: n(mud, "water_gain_on_rain_light"),
        rain_heavy: n(mud, "water_gain_on_rain_heavy"),
        water_max: n(mud, "water_gain_max"),
        water_min: n(mud, "water_gain_min"),
        dry_t: pair(mud, "water_dryout_temperature").unwrap_or((0.0, 1.0)),
        dry_m: pair(mud, "water_dryout_multiplier").unwrap_or((0.0, 0.0)),
        mud_min_t: n(mud, "min_temperature"),
        snow_gain: n(snow, "snow_gain_on_snowing"),
        blizzard_gain: n(snow, "snow_gain_on_blizzard"),
        snow_max: n(snow, "snow_gain_max"),
        melt_t: pair(snow, "snow_melt_temperature").unwrap_or((0.0, 1.0)),
        melt_m: pair(snow, "snow_melt_multiplier").unwrap_or((0.0, 0.0)),
        snow_t: pair(snow, "temperature").map_or(0.0, |p| p.0),
        ground_medium: pair(snow, "weather_ground_snow_medium").unwrap_or((f64::MAX, f64::MAX)),
        ground_high: pair(snow, "weather_ground_snow_high").unwrap_or((f64::MAX, f64::MAX)),
    }
}
/// The `weather_*` static modifiers (name without the prefix -> modifiers).
pub(super) fn static_mods(entries: &[Entry]) -> HashMap<String, Vec<(String, f64)>> {
    entries
        .iter()
        .filter_map(|e| {
            let name = e.key.strip_prefix("weather_")?;
            let mods = e
                .val
                .block()
                .iter()
                .filter_map(|m| Some((m.key.clone(), m.val.scalar()?.parse().ok()?)))
                .collect();
            Some((name.to_string(), mods))
        })
        .collect()
}
pub(super) fn load(dir: &Path) -> Weather {
    let mut regions = BTreeMap::new();
    if let Ok(rd) = std::fs::read_dir(dir.join("map/strategicregions")) {
        for e in rd.flatten() {
            if let Ok(text) = std::fs::read_to_string(e.path()) {
                if let Some((id, p)) = region_periods(&script::parse(&text)) {
                    regions.insert(id, p);
                }
            }
        }
    }
    let read = |rel: &str| {
        std::fs::read_to_string(dir.join(rel))
            .map(|t| script::parse(&t))
            .unwrap_or_default()
    };
    Weather {
        regions,
        settings: settings(&read("common/weather.txt")),
        mods: static_mods(&read("common/modifiers/00_static_modifiers.txt")),
    }
}
fn lerp(t: f64, (t0, t1): (f64, f64), (m0, m1): (f64, f64)) -> f64 {
    if t1 <= t0 {
        return m0;
    }
    m0 + (m1 - m0) * ((t - t0) / (t1 - t0)).clamp(0.0, 1.0)
}
fn period_at(periods: &[Period], day: usize) -> Option<&Period> {
    periods
        .iter()
        .find(|p| (p.from..=p.to).contains(&day))
        .or_else(|| periods.iter().min_by_key(|p| p.from.abs_diff(day)))
}
/// The share of `[lo, hi]` inside `[a, b]`.
fn share(lo: f64, hi: f64, a: f64, b: f64) -> f64 {
    if hi <= lo {
        return f64::from(u8::from(lo >= a && lo < b));
    }
    ((hi.min(b) - lo.max(a)).max(0.0) / (hi - lo)).clamp(0.0, 1.0)
}
/// The year of one region, day by day (see the module comment).
pub(super) fn year(
    periods: &[Period],
    s: &Settings,
    mods: &HashMap<String, Vec<(String, f64)>>,
) -> Vec<Day> {
    let (mut water, mut snow) = (0.0f64, 0.0f64);
    let mut out = Vec::with_capacity(365);
    for pass in 0..2 {
        for day in 0..365 {
            let Some(p) = period_at(periods, day) else {
                return Vec::new();
            };
            let mean = (p.t.0 + p.t.1) / 2.0;
            let cold = share(p.t.0, p.t.1, f64::MIN, s.snow_t);
            let total: f64 = p.w.iter().sum();
            let raw: Vec<f64> =
                p.w.iter()
                    .map(|w| if total > 0.0 { w / total } else { 0.0 })
                    .collect();
            let chances = [
                ("no_phenomenon", raw[0]),
                ("rain_light", raw[1] * (1.0 - cold)),
                ("rain_heavy", raw[2] * (1.0 - cold)),
                ("snow", raw[3] * cold),
                ("blizzard", raw[4] * cold),
                ("sandstorm", raw[5]),
            ];
            let rain = chances[1].1 + chances[2].1;
            water += chances[1].1 * s.rain_light + chances[2].1 * s.rain_heavy;
            water -= water * lerp(mean, s.dry_t, s.dry_m) * (1.0 - rain);
            water = water.clamp(0.0, s.water_max.max(0.0));
            snow += chances[3].1 * s.snow_gain + chances[4].1 * s.blizzard_gain;
            if mean > s.melt_t.0 {
                snow -= snow * lerp(mean, s.melt_t, s.melt_m);
            }
            snow = snow.clamp(0.0, s.snow_max.max(0.0));
            if pass == 0 {
                continue;
            }
            let mud = if mean > s.mud_min_t && water >= s.water_min && s.water_min > 0.0 {
                p.mud.min(1.0)
            } else {
                0.0
            };
            let ground = if snow >= s.ground_high.0 {
                "high"
            } else if snow >= s.ground_medium.0 {
                "medium"
            } else {
                "none"
            };
            let bands: Vec<(String, f64)> = s
                .bands
                .iter()
                .map(|(k, a, b)| (k.clone(), share(p.t.0, p.t.1, *a, *b)))
                .collect();
            let mut expected: BTreeMap<String, f64> = BTreeMap::new();
            let mut add = |name: &str, chance: f64| {
                if chance <= 0.0 {
                    return;
                }
                for (k, v) in mods.get(name).map(Vec::as_slice).unwrap_or(&[]) {
                    if KEYS.contains(&k.as_str()) {
                        *expected.entry(k.clone()).or_default() += chance * v;
                    }
                }
            };
            for (k, c) in &chances {
                add(k, *c);
            }
            add("mud", mud);
            if ground != "none" {
                add(&format!("ground_snow_{ground}"), 1.0);
            }
            for (k, c) in &bands {
                add(k.trim_start_matches("weather_"), *c);
            }
            for v in expected.values_mut() {
                *v = (*v * 1000.0).round() / 1000.0;
            }
            out.push(Day {
                mud,
                ground,
                water: (water * 10.0).round() / 10.0,
                snow: (snow * 10.0).round() / 10.0,
                chances: chances.to_vec(),
                bands,
                expected,
            });
        }
    }
    out
}
/// A day is bad ground for an offensive: mud more likely than not, or medium/high ground snow.
pub(super) fn bad(d: &Day) -> bool {
    d.mud >= 0.5 || d.ground != "none"
}
/// The forecast for one region from `day` over `horizon` days: today, the worst attack factor, when bad ground
/// clears (null: not bad today, or bad all the modelled year; a day past the horizon is flagged).
pub(super) fn outlook(days: &[Day], day: usize, horizon: usize) -> Value {
    if days.len() != 365 {
        return Value::Null;
    }
    let at = |k: usize| &days[(day + k) % 365];
    let today = at(0);
    let attack = |d: &Day| d.expected.get("army_attack_factor").copied().unwrap_or(0.0);
    let worst = (0..=horizon).map(|k| attack(at(k))).fold(0.0f64, f64::min);
    // bad ground that lasts past the horizon is still dated, from the whole modelled year, and
    // said to lie beyond the horizon (S23, s126a: null everywhere read as "no answer")
    let clears_at = bad(today).then(|| (1..365).find(|k| !bad(at(*k))));
    let clears = match clears_at {
        Some(found) => json!(found),
        None => Value::Null,
    };
    let clears_beyond = clears_at.flatten().is_some_and(|k| k > horizon);
    let turns_bad = if bad(today) {
        Value::Null
    } else {
        json!((1..=horizon).find(|k| bad(at(*k))))
    };
    json!({
        "mud_chance": today.mud,
        "ground_snow": today.ground,
        "water": today.water, "snow": today.snow,
        "bad_ground": bad(today),
        "phenomena": today.chances.iter().filter(|(_, c)| *c > 0.0)
            .map(|(k, c)| (k.to_string(), json!((c * 1000.0).round() / 1000.0))).collect::<Map<_, _>>(),
        "temperature_bands": today.bands.iter().filter(|(_, c)| *c > 0.0)
            .map(|(k, c)| (k.clone(), json!((c * 1000.0).round() / 1000.0))).collect::<Map<_, _>>(),
        "expected_modifiers": today.expected,
        "worst_attack_factor_in_horizon": (worst * 1000.0).round() / 1000.0,
        "bad_ground_clears_in_days": clears,
        "bad_ground_clears_beyond_horizon": clears_beyond,
        "bad_ground_all_year": clears_at == Some(None),
        "bad_ground_starts_in_days": turns_bad,
    })
}
/// (year, month, day) -> day of the year.
pub(super) fn day_of_date(month: usize, day: usize) -> Option<usize> {
    (1..=12).contains(&month).then(|| {
        MONTH_DAYS[..month - 1].iter().sum::<usize>() + day.clamp(1, MONTH_DAYS[month - 1]) - 1
    })
}
fn catalog(dir: &Path) -> Arc<Weather> {
    script::cached(dir, "weather_tables", load)
}
/// The answer for the regions asked (pure apart from the cached file read).
pub(super) fn answer(w: &Weather, regions: &[i32], day: usize, horizon: usize) -> Value {
    let mut rows = Vec::new();
    let mut worst: Option<f64> = None;
    let mut bad_now = Vec::new();
    let mut clears_max: Option<usize> = Some(0);
    for id in regions {
        let Some(periods) = w.regions.get(id) else {
            rows.push(json!({"region": id, "error": "no weather table"}));
            continue;
        };
        let days = year(periods, &w.settings, &w.mods);
        let o = outlook(&days, day, horizon);
        if let Some(a) = o["expected_modifiers"]["army_attack_factor"].as_f64() {
            worst = Some(worst.map_or(a, |x| x.min(a)));
        }
        if o["bad_ground"] == true {
            bad_now.push(*id);
            clears_max = match (clears_max, o["bad_ground_clears_in_days"].as_u64()) {
                (Some(m), Some(c)) => Some(m.max(c as usize)),
                _ => None,
            };
        }
        rows.push(json!({"region": id, "outlook": o}));
    }
    json!({
        "day_of_year": day,
        "horizon_days": horizon,
        "regions": rows,
        "summary": {
            "attack_factor_now_worst": worst,
            "bad_ground_regions": bad_now,
            "bad_ground_clears_in_days": if bad_now.is_empty() { json!(0) } else { json!(clears_max) },
        },
        "model": "estimate from map/strategicregions weather periods, common/weather.txt settings and the weather_* static modifiers; not the live weather",
    })
}
pub(super) fn call(
    process: &dyn crate::platform::Process,
    args: &Map<String, Value>,
) -> Result<Value> {
    super::allowed(
        args,
        &[
            "regions",
            "states",
            "provinces",
            "day_of_year",
            "date",
            "days",
        ],
    )?;
    let dir = super::mgmt::game_dir(process)?;
    let w = catalog(&dir);
    if w.regions.is_empty() {
        return Err(Error::new(
            "map_data_unavailable",
            "No strategic region has a weather table",
        ));
    }
    let ints = |k: &str| -> Vec<i64> {
        args.get(k)
            .and_then(Value::as_array)
            .map(|a| a.iter().filter_map(Value::as_i64).collect())
            .unwrap_or_default()
    };
    let mut regions: Vec<i32> = ints("regions").into_iter().map(|v| v as i32).collect();
    let states = ints("states");
    if !states.is_empty() {
        let sr = super::mapdata::state_regions(&dir)?;
        regions.extend(
            states
                .iter()
                .filter_map(|s| sr.get(&(*s as i32)).map(|r| r.0)),
        );
    }
    let provinces = ints("provinces");
    if !provinces.is_empty() {
        let pr = super::mapdata::province_regions(&dir)?;
        regions.extend(
            provinces
                .iter()
                .filter_map(|p| pr.get(&(*p as i32)).copied()),
        );
    }
    regions.sort_unstable();
    regions.dedup();
    if regions.is_empty() {
        return Err(Error::new(
            "invalid_arguments",
            "Name regions, states or provinces (strategic region ids, state ids, province ids)",
        ));
    }
    if regions.len() > 64 {
        return Err(Error::new(
            "invalid_arguments",
            "At most 64 regions per call",
        ));
    }
    let day = if let Some(d) = args.get("day_of_year").and_then(Value::as_u64) {
        (d as usize).min(364)
    } else if let Some(s) = args.get("date").and_then(Value::as_str) {
        let p: Vec<usize> = s.split(['.', '-']).filter_map(|x| x.parse().ok()).collect();
        p.get(1)
            .zip(p.get(2))
            .and_then(|(m, d)| day_of_date(*m, *d))
            .ok_or_else(|| Error::new("invalid_arguments", "date is Y.M.D"))?
    } else {
        let raw = crate::game::clock_context(process)?.date_raw as i64;
        let (_, m, d) = crate::focus_rules::live::ymd(raw)
            .ok_or_else(|| Error::new("game_state_unavailable", "Game date unreadable"))?;
        day_of_date(m as usize, d as usize).unwrap_or(0)
    };
    let horizon = args
        .get("days")
        .and_then(Value::as_u64)
        .unwrap_or(30)
        .min(120) as usize;
    Ok(answer(&w, &regions, day, horizon))
}
pub(super) fn tool() -> Value {
    json!({"name": "hoi4_weather", "description": "Read-only, no input (the game's files; the date from memory unless given): the weather and ground an offensive or an air mission meets, per strategic region, as the map's own weather tables make it likely on a day of the year (an estimate, not the live weather of a province). Name `regions` (strategic region ids, as hoi4_air_regions lists them), `states` or `provinces` (mapped to their region); optional `date` (Y.M.D) or `day_of_year`, and `days` (horizon, default 30). Per region: mud chance, ground snow (none/medium/high), phenomenon chances, temperature bands, the expected modifiers (army_attack_factor, army_speed_factor, army_attack_speed_factor, breakthrough_factor, supply_factor, attrition, winter_attrition, air_mission_efficiency ...), the worst attack factor in the horizon, and when bad ground (mud more likely than not, or medium/high ground snow) clears or starts. Every number comes from map/strategicregions, common/weather.txt and the weather_* static modifiers of the running game (a mod's files win).", "inputSchema": {"type": "object", "properties": {"regions": {"type": "array", "items": {"type": "integer"}, "maxItems": 64}, "states": {"type": "array", "items": {"type": "integer"}, "maxItems": 64}, "provinces": {"type": "array", "items": {"type": "integer"}, "maxItems": 64}, "date": {"type": "string", "maxLength": 12}, "day_of_year": {"type": "integer", "minimum": 0, "maximum": 364}, "days": {"type": "integer", "minimum": 0, "maximum": 120}}, "additionalProperties": false}})
}

#[cfg(test)]
mod tests {
    use super::*;

    const REGION: &str = "strategic_region={ id=131 name=\"STRATEGICREGION_131\" provinces={ 1 2 3 }
      weather={
        period={ between={ 0.0 30.0 } temperature={ -22.0 3.0 } no_phenomenon=0.2 rain_light=0.05 rain_heavy=0.05 snow=0.6 blizzard=0.1 mud=1.6 sandstorm=0 }
        period={ between={ 0.1 27.1 } temperature={ -21.0 5.0 } no_phenomenon=0.35 rain_light=0.1 rain_heavy=0.05 snow=0.5 blizzard=0.05 mud=1.6 }
        period={ between={ 0.2 30.2 } temperature={ -15.0 8.0 } no_phenomenon=0.4 rain_light=0.15 rain_heavy=0.05 snow=0.3 blizzard=0.1 mud=1.6 }
        period={ between={ 0.3 29.3 } temperature={ -3.0 16.0 } no_phenomenon=0.6 rain_light=0.45 rain_heavy=0.1 snow=0.15 mud=1.6 }
        period={ between={ 0.4 30.4 } temperature={ 4.0 21.0 } no_phenomenon=0.6 rain_light=0.4 rain_heavy=0.1 mud=1.6 }
        period={ between={ 0.5 29.5 } temperature={ 9.0 25.0 } no_phenomenon=0.6 rain_light=0.25 rain_heavy=0.15 mud=1.6 }
        period={ between={ 0.6 30.6 } temperature={ 10.0 27.0 } no_phenomenon=0.6 rain_light=0.25 rain_heavy=0.15 mud=1.6 }
        period={ between={ 0.7 30.7 } temperature={ 10.0 26.0 } no_phenomenon=0.6 rain_light=0.25 rain_heavy=0.15 mud=1.6 }
        period={ between={ 0.8 29.8 } temperature={ 4.0 21.0 } no_phenomenon=0.35 rain_light=0.3 rain_heavy=0.2 mud=1.6 }
        period={ between={ 0.9 30.9 } temperature={ -3.0 15.0 } no_phenomenon=0.35 rain_light=0.25 rain_heavy=0.15 snow=0.35 mud=1.6 }
        period={ between={ 0.10 29.10 } temperature={ -10.0 9.0 } no_phenomenon=0.35 rain_light=0.1 rain_heavy=0.15 snow=0.4 blizzard=0.15 mud=1.6 }
        period={ between={ 0.11 30.11 } temperature={ -16.0 7.0 } no_phenomenon=0.2 rain_light=0.13 rain_heavy=0.07 snow=0.55 blizzard=0.1 mud=1.6 }
      } }";
    const SETTINGS: &str = "settings = { temperature = { weather_extreme_cold = { -100.0 -12.0 } weather_very_cold = { -12.0 -4.0 } }
      mud = { water_gain_on_rain_light = 0.5 water_gain_on_rain_heavy = 1.8 water_gain_max = 200.0 water_gain_min = 5.0
              water_dryout_temperature = { 2.0 38.0 } water_dryout_multiplier = { 0.0005 0.4 } min_temperature = -1.0 }
      snowing = { snow_gain_on_snowing = 1.0 snow_gain_on_blizzard = 5.0 snow_gain_max = 80.0
              snow_melt_temperature = { 0.0 12.0 } snow_melt_multiplier = { 0.001 0.9 } temperature = { 1.0 -4.0 }
              weather_ground_snow_medium = { 20.0 40.0 } weather_ground_snow_high = { 40.0 1000.0 } } }";
    const MODS: &str = "weather_mud = { army_attack_factor = -0.4 army_speed_factor = -0.5 }
      weather_blizzard = { army_attack_speed_factor = -0.25 supply_factor = -0.25 }
      weather_ground_snow_high = { army_attack_factor = -0.4 army_speed_factor = -0.7 }
      weather_ground_snow_medium = { army_attack_factor = -0.2 }
      weather_extreme_cold = { army_attack_factor = -0.2 }
      other_modifier = { army_attack_factor = 1 }";

    fn weather() -> Weather {
        let (id, p) = region_periods(&script::parse(REGION)).unwrap();
        let mut w = Weather {
            settings: settings(&script::parse(SETTINGS)),
            mods: static_mods(&script::parse(MODS)),
            ..Weather::default()
        };
        w.regions.insert(id, p);
        w
    }

    #[test]
    fn periods_settings_and_modifiers_are_read_from_the_files() {
        let w = weather();
        let p = &w.regions[&131];
        assert_eq!(p.len(), 12);
        assert_eq!((p[1].from, p[1].to), (31, 58));
        assert_eq!(p[0].t, (-22.0, 3.0));
        assert_eq!(w.settings.water_min, 5.0);
        assert_eq!(w.settings.ground_high.0, 40.0);
        assert_eq!(w.settings.snow_t, 1.0);
        assert!(w.mods.contains_key("mud") && !w.mods.contains_key("other_modifier"));
        assert_eq!(day_of("30.11"), Some(364));
        assert_eq!(day_of_date(6, 22), Some(172));
    }

    #[test]
    fn deep_winter_is_high_ground_snow_and_summer_is_clear() {
        let w = weather();
        let days = year(&w.regions[&131], &w.settings, &w.mods);
        assert_eq!(days.len(), 365);
        let jan = &days[day_of_date(1, 20).unwrap()];
        assert_eq!(jan.ground, "high", "{jan:?}");
        assert!(jan.expected["army_attack_factor"] <= -0.4);
        let july = &days[day_of_date(7, 15).unwrap()];
        assert_eq!(july.ground, "none");
        assert!(!bad(july), "{july:?}");
    }

    #[test]
    fn the_outlook_says_when_bad_ground_clears_and_the_answer_sums_up() {
        let w = weather();
        let march = day_of_date(3, 1).unwrap();
        let a = answer(&w, &[131, 999], march, 120);
        let row = &a["regions"][0]["outlook"];
        assert_eq!(row["bad_ground"], true, "{row}");
        // snow, then the thaw's mud: dry ground comes back by summer
        let clears = row["bad_ground_clears_in_days"].as_u64().unwrap();
        assert!(clears > 30 && clears <= 120, "{clears}");
        assert_eq!(a["regions"][1]["error"], "no weather table");
        assert_eq!(a["summary"]["bad_ground_regions"], json!([131]));
        let june = answer(&w, &[131], day_of_date(6, 22).unwrap(), 30);
        assert_eq!(june["summary"]["bad_ground_clears_in_days"], 0);
        // S23 (s126a): a clearing past the horizon is still dated and flagged, never null
        let short = answer(&w, &[131], march, 10);
        let row = &short["regions"][0]["outlook"];
        assert_eq!(
            row["bad_ground_clears_in_days"].as_u64(),
            Some(clears),
            "{row}"
        );
        assert_eq!(row["bad_ground_clears_beyond_horizon"], true);
        assert_eq!(row["bad_ground_all_year"], false);
    }
}
