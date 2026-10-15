//! Read-only search selection for the exact owned Focus view. The search cache
//! contains current-item pointers, not canonical Focus pointers. Validate that
//! join before reporting keys; never expose its cached text or foreign payload.
use super::*;
mod control;
pub(super) use control::observe as control;

const MAX_ITEMS: usize = 4096;

fn invalid(message: &str) -> Error {
    Error::new("invalid_focus_search", message)
}

pub(super) fn observe(
    r: &Reader<'_>,
    base: u64,
    view: u64,
    country: u64,
    keys: &BTreeMap<u64, String>,
    requested: &str,
) -> Result<Value> {
    if !keys.values().any(|key| key == requested) {
        return Err(Error::new(
            "focus_unavailable",
            "Exact own canonical focus required",
        ));
    }
    // A null displayed country is allowed by the catalog reader, but it cannot
    // establish ownership of a retained search result. Do not read its arrays.
    if r.u64(view + 0x11d8)? != country {
        return Ok(
            json!({"available":false,"reason":"own_displayed_focus_view_required","input_authorized":false}),
        );
    }
    let current = pointers(r, view + 0x1138, MAX_ITEMS)?;
    unique(&current)?;
    let mut own_items = BTreeMap::new();
    for item in current {
        exact(r, item, base + 0x4249b90)?;
        if r.u64(item + 0x2b0)? != view {
            return Err(Error::new(
                "foreign_focus_scope",
                "Search item must belong to own view",
            ));
        }
        let focus = r.u64(item + 0x20)?;
        let key = keys
            .get(&focus)
            .ok_or_else(|| invalid("Current search item outside own canonical catalog"))?;
        own_items.insert(item, key);
    }
    let (header, data, count) = r.array_header(view + 0x12f8)?;
    let capacity = i32::from_le_bytes(header[8..12].try_into().unwrap()) as usize;
    if capacity > MAX_ITEMS || (count > 0 && data % 8 != 0) {
        return Err(invalid("Search catalog exceeds bounded aligned records"));
    }
    let mut records = Vec::with_capacity(count);
    let mut seen = BTreeSet::new();
    for index in 0..count {
        // The engine builds 0x50-byte cache rows from view.current_items. Only
        // their first pointer is relevant; cached names/descriptions stay unread.
        let item = r.u64(data + index as u64 * 0x50)?;
        if !seen.insert(item) {
            return Err(invalid("Duplicate search cache item"));
        }
        records.push(
            *own_items
                .get(&item)
                .ok_or_else(|| invalid("Search cache item absent from current own items"))?,
        );
    }
    let (header, data, count) = r.array_header(view + 0x1310)?;
    let capacity = i32::from_le_bytes(header[8..12].try_into().unwrap()) as usize;
    if capacity > MAX_ITEMS || count > records.len() || (count > 0 && data % 8 != 0) {
        return Err(invalid("Search matches exceed bounded aligned catalog"));
    }
    let mut matched = BTreeSet::new();
    let mut requested_matches = 0usize;
    for index in 0..count {
        // Each 8-byte result is a cache index followed by a ranking value. The
        // rank has no eligibility meaning and is outside this observation.
        let raw = r.i32(data + index as u64 * 8)?;
        let index = usize::try_from(raw).map_err(|_| invalid("Negative search result index"))?;
        let key = records
            .get(index)
            .ok_or_else(|| invalid("Search result index outside cache"))?;
        if !matched.insert(index) {
            return Err(invalid("Duplicate search result index"));
        }
        requested_matches += usize::from(key.as_str() == requested);
    }
    let raw = r.i32(view + 0x1358)?;
    let selected_index = if raw == -1 {
        None
    } else {
        let index =
            usize::try_from(raw).map_err(|_| invalid("Invalid negative search selection"))?;
        if index >= records.len() {
            return Err(invalid("Selected search index outside cache"));
        }
        Some(index)
    };
    let selected_key = selected_index.map(|index| records[index]);
    Ok(json!({
        "available":true,"requested_key":requested,
        "current_item_count":own_items.len(),"search_cache_count":records.len(),
        "complete_current_item_coverage":seen.len()==own_items.len(),
        "match_count":count,"requested_match_count":requested_matches,
        "selected_key":selected_key,
        "selected_matches_requested":selected_key.is_some_and(|key| key==requested),
        "selected_in_matches":selected_index.is_some_and(|index| matched.contains(&index)),
        "query_text_available":false,"query_control_binding_available":false,
        "selection_meaning":"current own search highlight; not an open detail or assigned Focus",
        "eligibility_available":false,"input_authorized":false
    }))
}
