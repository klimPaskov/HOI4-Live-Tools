//! Current owned assignment evidence, separate from public completion pages.
//! Bindings describe this observation only; a fresh key lookup after reconnect
//! does not prove that a historical Focus object or tree survived reconnect.
use super::super::{exact, layout, pointers, unique, validate_key, Witnesses};
use crate::{
    error::{Error, Result},
    game::{self, ClockContext},
    memory::Reader,
    platform::{Identity, Process},
    research,
};
use std::{
    cell::{Cell, RefCell},
    collections::{BTreeMap, BTreeSet},
};

#[cfg(test)]
#[allow(dead_code)]
pub(crate) mod fixture;
#[cfg(test)]
mod tests;

/// Process-local canonical identity, never a serialized action ID or authority.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct AssignmentBinding {
    game: Identity,
    roots: [u64; 3],
    country: u64,
    status: u64,
    database: u64,
    tree: u64,
    progress: u64,
    target: u64,
    members: Vec<u64>,
    keys: BTreeMap<String, u64>,
}
impl AssignmentBinding {
    pub(crate) fn game(&self) -> &Identity {
        &self.game
    }
    pub(crate) fn gs(&self) -> u64 {
        self.roots[0]
    }
    pub(crate) fn idler(&self) -> u64 {
        self.roots[1]
    }
    pub(crate) fn country(&self) -> u64 {
        self.country
    }
    pub(crate) fn status(&self) -> u64 {
        self.status
    }
    pub(crate) fn database(&self) -> u64 {
        self.database
    }
    pub(crate) fn tree(&self) -> u64 {
        self.tree
    }
    pub(crate) fn progress(&self) -> u64 {
        self.progress
    }
    pub(crate) fn target(&self) -> u64 {
        self.target
    }
    pub(crate) fn members(&self) -> &[u64] {
        &self.members
    }
    pub(crate) fn keys(&self) -> &BTreeMap<String, u64> {
        &self.keys
    }
}

/// One finished witnessed observation. Busy/active/completed targets remain
/// valid observations; the adapter decides readiness and verifies transitions.
#[derive(Clone, Debug)]
pub(crate) struct AssignmentSnapshot {
    context: ClockContext,
    binding: AssignmentBinding,
    active: Option<String>,
    completed: BTreeSet<String>,
}
impl AssignmentSnapshot {
    pub(crate) fn context(&self) -> &ClockContext {
        &self.context
    }
    pub(crate) fn own(&self) -> [u64; 2] {
        [self.binding.country, self.binding.status]
    }
    pub(crate) fn active_key(&self) -> Option<&str> {
        self.active.as_deref()
    }
    pub(crate) fn completed(&self) -> &BTreeSet<String> {
        &self.completed
    }
    pub(crate) fn binding(&self) -> &AssignmentBinding {
        &self.binding
    }
}

fn changed(message: &str) -> Error {
    Error::new("ownership_changed", message)
}
fn address(pointer: u64) -> bool {
    (0x10000..=0x7fff_ffff_0000).contains(&pointer) && pointer % 8 == 0
}
fn identity(process: &dyn Process, expected: &Identity) -> Result<()> {
    if process.identity() != Some(expected) {
        return Err(changed("Exact Focus assignment process identity required"));
    }
    Ok(())
}

/// The supplied Process owns any original deadline/lease/cancellation checks.
/// All clock, status, catalog and membership reads share this single bracket.
pub(crate) fn snapshot(
    process: &dyn Process,
    key: &str,
    expected_game: &Identity,
    expected_campaign: &ClockContext,
    expected_own: [u64; 2],
) -> Result<AssignmentSnapshot> {
    validate_key(key)?;
    identity(process, expected_game)?;
    if expected_campaign
        .anchors
        .iter()
        .chain(expected_own.iter())
        .any(|p| !address(*p))
    {
        return Err(changed("Bounded aligned Focus assignment scope required"));
    }
    crate::calendar::decode(expected_campaign.date_raw)?;
    process.begin_snapshot()?;
    let witnesses = Witnesses {
        process,
        reads: RefCell::new(BTreeMap::new()),
        section: Cell::new("assignment_roots"),
        regions: RefCell::new(BTreeMap::new()),
    };
    let r = Reader::new(&witnesses);
    let base = process.image_base();
    let g = game::layout(&r);
    let l = layout(&r);
    let technology = research::layout(&r);
    let roots = [
        r.u64(base + g.gs_global)?,
        r.u64(base + g.idler_global)?,
        r.u64(base + g.technology_db_global)?,
    ];
    if roots != expected_campaign.anchors {
        return Err(changed("Focus assignment game roots differ before payload"));
    }
    let [gs, idler, technology_database] = roots;
    exact(&r, gs, base + g.gs_vtable)?;
    exact(&r, idler, base + g.idler_vtable)?;
    exact(&r, technology_database, base + technology.database_vtable)?;
    witnesses.region(gs, 0x720, "game_state");
    let tag = r.i32(gs + g.player_tag)?;
    if !(1..4096).contains(&tag) {
        return Err(Error::new(
            "player_unavailable",
            "Positive own tag required",
        ));
    }
    let index = i32::from_le_bytes(
        r.array_entry(gs + g.tag_country_index, tag as usize, 4)?
            .try_into()
            .unwrap(),
    );
    if index < 0 {
        return Err(Error::new("player_unavailable", "Own country index absent"));
    }
    let country = u64::from_le_bytes(
        r.array_entry(gs + g.countries, index as usize, 8)?
            .try_into()
            .unwrap(),
    );
    if country != expected_own[0] {
        return Err(changed(
            "Focus assignment own-country map differs before payload",
        ));
    }
    exact(&r, country, base + g.country_vtable)?;
    witnesses.region(country, 0x1350, "own_country");
    if r.i32(country + 8)? != tag {
        return Err(Error::new(
            "invalid_focus_owner",
            "Own country tag required",
        ));
    }
    let status = r.u64(country + g.country_technology_status)?;
    if status != expected_own[1] {
        return Err(changed(
            "Focus assignment own status differs before payload",
        ));
    }
    exact(&r, status, base + technology.status_vtable)?;
    witnesses.region(status, 0x13c, "own_status");
    if r.u64(status + 0x130)? != country || r.i32(status + 0x138)? != tag {
        return Err(Error::new(
            "invalid_focus_owner",
            "Own status country and tag backlinks required before payload",
        ));
    }

    witnesses.section.set("assignment_catalog");
    let tree = r.u64(country + g.country_focus_tree)?;
    let progress = r.u64(country + g.country_focus_progress)?;
    let database = r.u64(base + l.database_global)?;
    exact(&r, tree, base + l.tree_vtable)?;
    exact(&r, progress, base + l.progress_vtable)?;
    exact(&r, database, base + l.database_vtable)?;
    witnesses.region(tree, 0x40, "own_tree");
    witnesses.region(progress, 0x50, "own_progress");
    if r.u64(progress + 8)? != country {
        return Err(Error::new(
            "invalid_focus_owner",
            "Own progress backlink required",
        ));
    }
    let members = pointers(&r, tree + l.tree_focuses, 4096)?;
    let own = unique(&members)?;
    let canonical: BTreeSet<_> = pointers(&r, database + 0x58, 20000)?
        .into_iter()
        .filter(|p| *p != 0)
        .collect();
    if !own.is_subset(&canonical) {
        return Err(Error::new(
            "invalid_focus_catalog",
            "Own tree must join canonical database",
        ));
    }
    // Validate every own key, including idle, unfinished and non-target members.
    // Registry nulls/duplicates confer no ownership or key uniqueness.
    let mut keys = BTreeMap::new();
    let mut names = BTreeMap::new();
    for &focus in &members {
        exact(&r, focus, base + l.focus_vtable)?;
        if r.u64(focus + l.focus_tree)? != tree {
            return Err(Error::new(
                "invalid_focus_owner",
                "Own Focus tree backlink required before key payload",
            ));
        }
        let name = r.string_bounded(focus + 0x18, 256)?;
        if name.is_empty()
            || !name.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_')
            || keys.insert(name.clone(), focus).is_some()
        {
            return Err(Error::new(
                "invalid_focus_catalog",
                "Unique canonical own Focus keys required",
            ));
        }
        names.insert(focus, name);
    }
    let target = *keys
        .get(key)
        .ok_or_else(|| Error::new("invalid_target", "Focus absent from own canonical tree"))?;
    witnesses.section.set("assignment_membership");
    let completed: BTreeSet<_> = pointers(&r, progress + 0x40, 4096)?.into_iter().collect();
    let active = r.u64(progress + 0x10)?;
    if !completed.is_subset(&own) || (active != 0 && !own.contains(&active)) {
        return Err(Error::new(
            "invalid_focus_owner",
            "Active/completed Focus must belong to own tree",
        ));
    }

    // The broader clock reader may follow active own payload only after every
    // owner, status, active member and canonical Focus key has been guarded.
    witnesses.section.set("assignment_clock");
    let context = game::context_from_reader(&r, base)?;
    if context.player != expected_campaign.player
        || context.playthrough_id != expected_campaign.playthrough_id
        || context.anchors != roots
        || context.difficulty != expected_campaign.difficulty
        || context.historical_raw != expected_campaign.historical_raw
        || context.date_raw < expected_campaign.date_raw
    {
        return Err(changed(
            "Focus assignment campaign, setup or chronology differs",
        ));
    }
    witnesses.finish()?;
    process.check()?;
    identity(process, expected_game)?;
    Ok(AssignmentSnapshot {
        context,
        binding: AssignmentBinding {
            game: expected_game.clone(),
            roots,
            country,
            status,
            database,
            tree,
            progress,
            target,
            members,
            keys,
        },
        active: names.get(&active).cloned(),
        completed: completed.iter().map(|p| names[p].clone()).collect(),
    })
}
