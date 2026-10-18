//! Own completed membership, independently recovered from this Linux build's
//! CHasCompletedNationalFocusTrigger. Upstream's FP_COMPLETED (+0x20) is not
//! used: the current engine predicate scans progress+0x40, count+0x4c.
use super::*;

/// Pure observed membership, not an assertion about completion cause or time.
/// Repeat counts, bypass causes, changed/shared trees and eligibility are not
/// inferred. All members are validated before returning a bounded sorted page.
pub fn snapshot(process: &dyn Process, offset: usize, limit: usize) -> Result<Value> {
    if offset > 4096 || !(1..=100).contains(&limit) {
        return Err(Error::new(
            "invalid_arguments",
            "Completed focus pagination exceeds bounds",
        ));
    }
    let started = Instant::now();
    process.begin_snapshot()?;
    let w = Witnesses {
        process,
        reads: RefCell::new(BTreeMap::new()),
        section: Cell::new("completion_roots"),
        regions: RefCell::new(BTreeMap::new()),
    };
    let r = Reader::new(&w);
    let base = process.image_base();
    let g = game::layout(&r);
    let l = layout(&r);
    let gs = r.u64(base + g.gs_global)?;
    let idler = r.u64(base + g.idler_global)?;
    exact(&r, gs, base + g.gs_vtable)?;
    exact(&r, idler, base + g.idler_vtable)?;
    w.region(gs, 0x520, "game_state");
    let date = crate::calendar::decode(r.i32(gs + g.date)?)?;
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
    r.object_type(country, "CCountry")?;
    w.region(country, 0x1350, "own_country");
    if r.i32(country + 8)? != tag {
        return Err(Error::new(
            "invalid_focus_owner",
            "Own country tag required",
        ));
    }
    let progress = r.u64(country + g.country_focus_progress)?;
    exact(&r, progress, base + l.progress_vtable)?;
    w.region(progress, 0x50, "own_progress");
    if r.u64(progress + 8)? != country {
        return Err(Error::new(
            "invalid_focus_owner",
            "Own progress required before membership payload",
        ));
    }
    let tree = r.u64(country + g.country_focus_tree)?;
    exact(&r, tree, base + l.tree_vtable)?;
    w.region(tree, 0x40, "own_tree");
    let own = unique(&pointers(&r, tree + l.tree_focuses, 4096)?)?;
    let db = r.u64(base + l.database_global)?;
    exact(&r, db, base + l.database_vtable)?;
    // As in the catalog observer, registry duplicates/nulls confer no ownership.
    let canonical: BTreeSet<_> = pointers(&r, db + 0x58, 20000)?
        .into_iter()
        .filter(|p| *p != 0)
        .collect();
    if !own.is_subset(&canonical) {
        return Err(Error::new(
            "invalid_focus_catalog",
            "Own tree must join the canonical database",
        ));
    }
    w.section.set("completed_membership");
    let members = pointers(&r, progress + 0x40, 4096)?;
    let active = r.u64(progress + 0x10)?;
    let completed: BTreeSet<_> = members.iter().copied().collect();
    if !completed.is_subset(&own) || (active != 0 && !own.contains(&active)) {
        return Err(Error::new(
            "invalid_focus_owner",
            "Active/completed focus must belong to the current own tree",
        ));
    }
    // The engine predicate is membership-based. Repeated identical pointers
    // need one canonical validation; no repeat count or reward is inferred.
    let mut selected = completed.clone();
    if active != 0 {
        selected.insert(active);
    }
    let mut keys = BTreeMap::new();
    let mut distinct_keys = BTreeSet::new();
    for pointer in selected {
        exact(&r, pointer, base + l.focus_vtable)?;
        if r.u64(pointer + l.focus_tree)? != tree {
            return Err(Error::new(
                "invalid_focus_owner",
                "Shared/null/foreign focus tree ownership is unverified",
            ));
        }
        let key = r.string_bounded(pointer + 0x18, 256)?;
        if key.is_empty()
            || !key.bytes().all(|c| c.is_ascii_alphanumeric() || c == b'_')
            || !distinct_keys.insert(key.clone())
        {
            return Err(Error::new(
                "invalid_focus_catalog",
                "Distinct canonical focus keys required",
            ));
        }
        keys.insert(pointer, key);
    }
    let mut finished: Vec<_> = completed.iter().map(|p| keys[p].clone()).collect();
    finished.sort();
    let count = finished.len();
    let page: Vec<_> = finished.into_iter().skip(offset).take(limit).collect();
    let next = (offset.saturating_add(page.len()) < count).then_some(offset + page.len());
    let (end_calls, end_bytes) = w.finish()?;
    process.check()?;
    Ok(json!({
        "source":"native own focus completion membership",
        "build_id":g.build_id,"owned":true,"date":date,
        "active_key":keys.get(&active),"completed":page,
        "completed_count":count,"membership_entries":members.len(),
        "offset":offset,"limit":limit,"next_offset":next,
        "completion_meaning":"engine has-completed-focus membership; cause and repeat count unavailable",
        "eligibility_available":false,"input_authorized":false,
        "observation_paused_game":false,
        "freshness":{
            "observed_unix_ms":SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_millis(),
            "elapsed_ms":started.elapsed().as_secs_f64()*1000.0,
            "read_calls":r.calls.get()+end_calls as u64,"read_bytes":r.bytes.get()+end_bytes
        }
    }))
}
