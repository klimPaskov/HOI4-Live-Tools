//! Canonical own-tree metadata without platform-specific focus UI traversal.
//! Reuses the independently verified layouts used by completion membership.
use super::*;

pub(super) fn snapshot(
    process: &dyn Process,
    offset: usize,
    limit: usize,
    prefix: Option<&str>,
) -> Result<Value> {
    let started = Instant::now();
    process.begin_snapshot()?;
    let w = Witnesses {
        process,
        reads: RefCell::new(BTreeMap::new()),
        section: Cell::new("catalog_roots"),
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
    let tag = r.i32(gs + g.player_tag)?;
    if !(1..4096).contains(&tag) {
        return Err(Error::new(
            "player_unavailable",
            "Positive own tag required before catalog payload",
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
    exact(&r, country, base + g.country_vtable)?;
    if r.i32(country + 8)? != tag {
        return Err(Error::new(
            "invalid_focus_owner",
            "Own country tag join required",
        ));
    }
    let tree = r.u64(country + g.country_focus_tree)?;
    exact(&r, tree, base + l.tree_vtable)?;
    let db = r.u64(base + l.database_global)?;
    exact(&r, db, base + l.database_vtable)?;
    w.section.set("catalog");
    let canonical: BTreeSet<_> = pointers(&r, db + 0x58, 20000)?
        .into_iter()
        .filter(|p| *p != 0)
        .collect();
    let focuses = pointers(&r, tree + l.tree_focuses, 4096)?;
    let own = unique(&focuses)?;
    if !own.is_subset(&canonical) {
        return Err(Error::new(
            "invalid_focus_catalog",
            "Own focus absent from canonical database",
        ));
    }
    let mut keys = BTreeSet::new();
    let mut output = Vec::new();
    // Validate every own member, including those outside the requested page.
    for focus in focuses {
        exact(&r, focus, base + l.focus_vtable)?;
        if r.u64(focus + l.focus_tree)? != tree {
            return Err(Error::new(
                "invalid_focus_owner",
                "Shared/null/foreign focus tree ownership is unverified",
            ));
        }
        let key = r.string_bounded(focus + 0x18, 256)?;
        if key.is_empty()
            || !key.bytes().all(|c| c.is_ascii_alphanumeric() || c == b'_')
            || !keys.insert(key.clone())
        {
            return Err(Error::new(
                "invalid_focus_catalog",
                "Unique bounded canonical focus keys required",
            ));
        }
        if prefix.is_none_or(|filter| key.starts_with(filter)) {
            output.push(json!({"key":key}));
        }
    }
    let total = output.len();
    let page: Vec<_> = output.into_iter().skip(offset).take(limit).collect();
    let (calls, bytes) = w.finish()?;
    process.check()?;
    Ok(
        json!({"source":"native own canonical focus catalog","build_id":g.build_id,"owned":true,
        "catalog_count":own.len(),"filtered_count":total,"offset":offset,"limit":limit,
        "next_offset":if offset.saturating_add(page.len())<total {Some(offset+page.len())}else{None},
        "focuses":page,"details":[],"details_available":false,"detail_unavailable_reason":"Mac focus UI diagnostics are not ported; no UI payload was read",
        "numeric_ids_available":false,"eligibility_available":false,"input_authorized":false,"branch_visibility_available":false,
        "observation_paused_game":false,"freshness":{"observed_unix_ms":SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_millis(),
            "elapsed_ms":started.elapsed().as_secs_f64()*1000.0,"read_calls":r.calls.get()+calls as u64,"read_bytes":r.bytes.get()+bytes}}),
    )
}
