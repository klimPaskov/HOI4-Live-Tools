//! Air range against strategic regions, from the map files: what share of a region a wing reaches
//! from its air base, and air base overstacking.
//!
//! The game shows a wing's mission efficiency per region and, under it, "Coverage in this region":
//! the part of the region within the plane's air range of its base. Here it is the share of the
//! region's land provinces whose unit position (map/unitstacks.txt) lies within the range of the
//! base province's position, at NDefines.NGame.MAP_SCALE_PIXEL_TO_KM km per map pixel (read from
//! common/defines; 7.114 in vanilla). The range is the variant's air_range as the variant carries it
//! (design stats; whether country bonuses such as an MIO range trait are already in it is a live
//! check, LIVE_VALIDATION_QUEUE air-coverage). Nothing names a country or a map: any mod's map works.
//!
//! An air base holds 200 aircraft per level; more stationed than that is overstacking, which the
//! game punishes with mission efficiency (the read reports the excess; the penalty's size is the
//! game's and is not computed here).

use std::collections::HashMap;
use std::path::Path;
use std::sync::{Arc, Mutex, OnceLock};

use serde_json::{json, Value};

/// Vanilla NDefines.NGame.MAP_SCALE_PIXEL_TO_KM (used when the define is unreadable).
pub(super) const DEFAULT_KM_PER_PIXEL: f64 = 7.114;

/// The land provinces' positions of every strategic region and the map scale, per install.
pub(super) struct RegionGeometry {
    pub km_per_pixel: f64,
    pub land: HashMap<i32, Vec<(f64, f64)>>,
    pub province: Arc<HashMap<i32, (f64, f64)>>,
}

/// Region geometry from the game files (cached per content folder); None when the map is unreadable.
pub(super) fn geometry(dir: &Path) -> Option<Arc<RegionGeometry>> {
    static CACHE: OnceLock<Mutex<HashMap<std::path::PathBuf, Arc<RegionGeometry>>>> =
        OnceLock::new();
    let cache = CACHE.get_or_init(|| Mutex::new(HashMap::new()));
    if let Some(found) = cache.lock().unwrap().get(dir) {
        return Some(found.clone());
    }
    let positions = super::mapdata::positions(dir).ok()?;
    let regions = super::mapdata::province_regions(dir).ok()?;
    let water = super::mapdata::water_provinces(dir);
    let km = std::fs::read_to_string(dir.join("common/defines/00_defines.lua"))
        .ok()
        .and_then(|t| super::leader_act::define(&t, "MAP_SCALE_PIXEL_TO_KM"))
        .filter(|k| *k > 0.0)
        .unwrap_or(DEFAULT_KM_PER_PIXEL);
    let out = Arc::new(build(&positions, &regions, &water, km));
    cache.lock().unwrap().insert(dir.to_path_buf(), out.clone());
    Some(out)
}

pub(super) fn build(
    positions: &Arc<HashMap<i32, (f64, f64)>>,
    regions: &HashMap<i32, i32>,
    water: &std::collections::HashSet<i32>,
    km_per_pixel: f64,
) -> RegionGeometry {
    let mut land: HashMap<i32, Vec<(f64, f64)>> = HashMap::new();
    let mut ids: Vec<(&i32, &i32)> = regions.iter().collect();
    ids.sort();
    for (p, r) in ids {
        if water.contains(p) {
            continue;
        }
        if let Some(xy) = positions.get(p) {
            land.entry(*r).or_default().push(*xy);
        }
    }
    RegionGeometry {
        km_per_pixel,
        land,
        province: positions.clone(),
    }
}

impl RegionGeometry {
    fn km(&self, a: (f64, f64), b: (f64, f64)) -> f64 {
        ((a.0 - b.0).powi(2) + (a.1 - b.1).powi(2)).sqrt() * self.km_per_pixel
    }

    /// Share (0..1) of `region`'s land provinces within `range_km` of province `base`; None when the
    /// base or the region has no position.
    pub(super) fn coverage(&self, region: i32, base: i32, range_km: f64) -> Option<f64> {
        let from = *self.province.get(&base)?;
        let pts = self.land.get(&region).filter(|p| !p.is_empty())?;
        let hit = pts
            .iter()
            .filter(|p| self.km(from, **p) <= range_km)
            .count();
        Some((hit as f64 / pts.len() as f64 * 1000.0).round() / 1000.0)
    }

    /// The least range (km) that reaches `share` of `region` from province `base`.
    pub(super) fn range_for(&self, region: i32, base: i32, share: f64) -> Option<f64> {
        let from = *self.province.get(&base)?;
        let pts = self.land.get(&region).filter(|p| !p.is_empty())?;
        let mut d: Vec<f64> = pts.iter().map(|p| self.km(from, *p)).collect();
        d.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
        let k = ((share * d.len() as f64).ceil() as usize).clamp(1, d.len()) - 1;
        Some(d[k].round())
    }
}

/// The coverage row a wing carries: {region_id, share, range_km, range_for_full_km}; Null when
/// the wing has no region, no range or no position.
pub(super) fn wing_row(
    g: &RegionGeometry,
    region: i32,
    base_province: i32,
    range_km: Option<f64>,
) -> Value {
    let Some(range) = range_km.filter(|r| *r > 0.0) else {
        return Value::Null;
    };
    if region <= 0 {
        return Value::Null;
    }
    match g.coverage(region, base_province, range) {
        Some(share) => json!({"region_id":region,"share":share,"range_km":range,
            "range_for_full_km":g.range_for(region, base_province, 1.0)}),
        None => Value::Null,
    }
}

/// Aircraft above the base's capacity (0 when it fits).
pub(super) fn over_capacity(capacity: i32, stationed: i32) -> i32 {
    (stationed - capacity).max(0)
}

pub(super) const METHOD: &str = "coverage: share of the region's land provinces (map/unitstacks.txt positions) within the variant's air_range of the base province, at MAP_SCALE_PIXEL_TO_KM km per pixel; the game's own 'Coverage in this region' is the reference (live check: LIVE_VALIDATION_QUEUE air-coverage)";

#[cfg(test)]
mod tests {
    use super::*;

    fn geo() -> RegionGeometry {
        // region 1: provinces 1..10 at x = 10..100 on a line; province 50 (water) in region 1; base province 99 at 0
        let mut pos = HashMap::new();
        let mut regions = HashMap::new();
        for i in 1..=10 {
            pos.insert(i, (f64::from(i) * 10.0, 0.0));
            regions.insert(i, 1);
        }
        pos.insert(50, (0.0, 0.0));
        regions.insert(50, 1);
        pos.insert(99, (0.0, 0.0));
        regions.insert(99, 2);
        let water = [50].into_iter().collect();
        build(&Arc::new(pos), &regions, &water, 1.0)
    }

    #[test]
    fn coverage_counts_land_within_range() {
        let g = geo();
        assert_eq!(g.coverage(1, 99, 50.0), Some(0.5));
        assert_eq!(g.coverage(1, 99, 1000.0), Some(1.0));
        assert_eq!(g.coverage(1, 99, 5.0), Some(0.0));
        assert_eq!(g.coverage(3, 99, 50.0), None);
        assert_eq!(g.coverage(1, 12345, 50.0), None);
        assert_eq!(g.range_for(1, 99, 0.9), Some(90.0));
        assert_eq!(g.range_for(1, 99, 1.0), Some(100.0));
    }

    #[test]
    fn wing_row_and_overstack() {
        let g = geo();
        let row = wing_row(&g, 1, 99, Some(50.0));
        assert_eq!(row["share"], 0.5);
        assert_eq!(row["range_for_full_km"], 100.0);
        assert!(wing_row(&g, 0, 99, Some(50.0)).is_null());
        assert!(wing_row(&g, 1, 99, None).is_null());
        assert_eq!(over_capacity(200, 250), 50);
        assert_eq!(over_capacity(200, 150), 0);
    }
}
