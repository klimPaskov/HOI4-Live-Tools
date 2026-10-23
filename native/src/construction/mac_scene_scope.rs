//! Own-only scene admission for Mach-O 089EA519. The current GS picker list can
//! include relationship-admitted foreign objects, so list membership alone is
//! insufficient. Recoveries: GS setup 1006e4f00, insertion 1006e5700, ownership
//! update 1017dd9f0, own fleet member getter 1005f4790, picker 101930e90.
//! Caller owns process/build/context checks and the encompassing final witnesses.
//! No model payload, intersection, GUI routing, input or placement authority.
use super::{invalid, mac_ui, members};
use crate::{
    error::{Error, Result},
    game,
    memory::{Reader, StringAbi},
};
use std::collections::BTreeMap;

const LIMIT: usize = 4096; // Observer policy, not an engine limit.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Kind {
    Army,
    TaskForce,
}

impl Kind {
    fn primary(self) -> u64 {
        match self {
            Self::Army => 0x32af5a8,
            Self::TaskForce => 0x331ef98,
        }
    }
    fn raw(self) -> i32 {
        match self {
            Self::Army => 0,
            Self::TaskForce => 1,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Entry {
    object: u64,
    kind: Kind,
}

impl Entry {
    pub(crate) fn object(&self) -> u64 {
        self.object
    }
    pub(crate) fn kind(&self) -> Kind {
        self.kind
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Binding {
    base: u64,
    gs: u64,
    tag: i32,
    index: usize,
    country: u64,
    armies: Vec<u64>,
    fleets: Vec<u64>,
    tasks: Vec<Vec<u64>>,
    entries: Vec<Entry>,
}

impl Binding {
    pub(crate) fn entries(&self) -> &[Entry] {
        &self.entries
    }
}

fn changed(message: &'static str) -> Error {
    Error::new("construction_scene_changed", message)
}

fn own_entry(r: &Reader<'_>, address: u64, index: usize, stride: usize) -> Result<Vec<u8>> {
    let (header, data, _) = r.array_header(address)?;
    let capacity = i32::from_le_bytes(header[8..12].try_into().unwrap());
    if capacity > LIMIT as i32 || data % stride as u64 != 0 {
        return Err(invalid("Bounded aligned scene country mapping required"));
    }
    r.array_entry(address, index, stride)
}

/// Retain complete ordered source lists. An original binding rejects replacement
/// country/fleet/scene edges before replacement object payload. Movement/model
/// state is deliberately absent: this does not prove that any ray misses them.
pub(crate) fn from_reader(
    r: &Reader<'_>,
    base: u64,
    original: Option<&Binding>,
) -> Result<Binding> {
    if r.string_abi() != StringAbi::LibCxx {
        return Err(Error::new(
            "unsupported_platform",
            "Mac scene layout required",
        ));
    }
    if original.is_some_and(|b| b.base != base) {
        return Err(changed("Original scene image changed"));
    }
    let g = &game::MACOS;
    let gs = r.u64(base + g.gs_global)?;
    if original.is_some_and(|b| b.gs != gs) {
        return Err(changed("Original scene GS changed before payload"));
    }
    mac_ui::exact(r, base, gs, g.gs_vtable)?;
    let tag = r.i32(gs + g.player_tag)?;
    if !(1..4096).contains(&tag) || original.is_some_and(|b| b.tag != tag) {
        return Err(changed("Original own scene tag required"));
    }
    let index = i32::from_le_bytes(
        own_entry(r, gs + g.tag_country_index, tag as usize, 4)?
            .try_into()
            .unwrap(),
    );
    if index < 0 || original.is_some_and(|b| b.index != index as usize) {
        return Err(changed("Original own scene country index required"));
    }
    let index = index as usize;
    let country = u64::from_le_bytes(
        own_entry(r, gs + g.countries, index, 8)?
            .try_into()
            .unwrap(),
    );
    if original.is_some_and(|b| b.country != country) {
        return Err(changed("Original own scene country changed before payload"));
    }
    mac_ui::exact(r, base, country, g.country_vtable)?;
    if r.i32(country + 8)? != tag {
        return Err(invalid("Own scene country tag must join before lists"));
    }
    let armies = members(r, country + 0x250, LIMIT)?;
    let fleets = members(r, country + 0x238, 1024)?;
    let scene = members(r, gs + 0x5e0, LIMIT)?;
    if original.is_some_and(|b| {
        b.armies != armies
            || b.fleets != fleets
            || b.entries.iter().map(|e| e.object).ne(scene.iter().copied())
    }) {
        return Err(changed(
            "Original scene or own source list changed before objects",
        ));
    }
    let mut pool: BTreeMap<u64, Kind> = armies.iter().map(|&a| (a, Kind::Army)).collect();
    let mut tasks = Vec::with_capacity(fleets.len());
    for (i, fleet) in fleets.iter().enumerate() {
        mac_ui::exact(r, base, *fleet, 0x32fe7b0)?;
        let current = members(r, fleet + 0xb0, LIMIT)?;
        if original.is_some_and(|b| b.tasks.get(i) != Some(&current)) {
            return Err(changed(
                "Original own fleet members changed before task payload",
            ));
        }
        if pool.len().saturating_add(current.len()) > LIMIT {
            return Err(invalid("Complete own scene sources exceed observer budget"));
        }
        for &object in &current {
            if pool.insert(object, Kind::TaskForce).is_some() {
                return Err(invalid("Own scene source identity must be unambiguous"));
            }
        }
        tasks.push(current);
    }
    // Check ALL memberships before following ANY candidate payload. In
    // particular, never skip an unknown entry to certify terrain fallback.
    let entries = scene
        .iter()
        .map(|&object| {
            pool.get(&object)
                .copied()
                .map(|kind| Entry { object, kind })
                .ok_or_else(|| invalid("Scene entry lacks exact own source membership"))
        })
        .collect::<Result<Vec<_>>>()?;
    for entry in &entries {
        mac_ui::exact(r, base, entry.object, entry.kind.primary())?;
        if r.i32(entry.object + 0x1d8)? != tag {
            return Err(invalid("Own scene owner required before candidate kind"));
        }
        if r.i32(entry.object + 8)? != entry.kind.raw() {
            return Err(invalid("Scene type and selectable kind disagree"));
        }
    }
    Ok(Binding {
        base,
        gs,
        tag,
        index,
        country,
        armies,
        fleets,
        tasks,
        entries,
    })
}

#[cfg(test)]
mod tests;
