//! Request-local exact-ID observation; unrelated GUI payload is outside scope.
use crate::{
    error::{Error, Result},
    memory::Reader,
    research_ui::{array, exact},
};
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet};

pub(crate) struct Catalog {
    templates: Vec<u64>,
    own: Vec<u64>,
    keys: Vec<String>,
    pub technology: u64,
}
impl Catalog {
    pub(crate) fn load(r: &Reader<'_>, base: u64, status: u64, key: &str) -> Result<Self> {
        let db = r.u64(base + 0x439f450)?;
        exact(r, db, base + 0x42110c8)?;
        let templates = array(r, db + 0x48, 8192)?;
        let own = array(r, status + 0x88, 8192)?;
        if templates.is_empty() || templates[0] != 0 || own.len() != templates.len() - 1 {
            return Err(Error::new(
                "research_unavailable",
                "Initialized canonical catalog required",
            ));
        }
        let mut keys = vec![String::new()];
        let mut matches = Vec::new();
        for (index, &template) in templates.iter().enumerate().skip(1) {
            exact(r, template, base + 0x4211038)?;
            if r.i32(template + 0x30)? != index as i32 {
                return Err(Error::new(
                    "invalid_research_join",
                    "Canonical public definition index required",
                ));
            }
            let public = r.string_bounded(template + 8, 256)?;
            if public.is_empty() || public.chars().any(char::is_control) {
                return Err(Error::new(
                    "invalid_research_id",
                    "Bounded public technology key required",
                ));
            }
            if public == key {
                matches.push(index);
            }
            keys.push(public);
        }
        if matches.len() > 1 {
            return Err(Error::new(
                "invalid_research_id",
                "Requested public key is ambiguous",
            ));
        }
        let index = *matches.first().ok_or_else(|| {
            Error::new(
                "node_unavailable",
                "Exact public technology key unavailable",
            )
        })?;
        let result = Self {
            technology: own[index - 1],
            templates,
            own,
            keys,
        };
        result.validate(r, base, status, result.technology)?;
        Ok(result)
    }
    pub(crate) fn validate(
        &self,
        r: &Reader<'_>,
        base: u64,
        status: u64,
        technology: u64,
    ) -> Result<usize> {
        exact(r, technology, base + 0x42107f8)?;
        if r.u64(technology + 0x158)? != status {
            return Err(Error::new(
                "invalid_research_owner",
                "Own technology status required",
            ));
        }
        let template = r.u64(technology + 0x160)?;
        exact(r, template, base + 0x4211038)?;
        let id = r.i32(template + 0x30)?;
        if id <= 0
            || self.templates.get(id as usize) != Some(&template)
            || self.own.get(id as usize - 1) != Some(&technology)
        {
            return Err(Error::new(
                "invalid_research_join",
                "Canonical own technology required",
            ));
        }
        Ok(id as usize)
    }
}
pub(crate) fn observe(
    r: &Reader<'_>,
    base: u64,
    tree: u64,
    status: u64,
    registry: u64,
    key: &str,
    catalog: &Catalog,
) -> Result<Value> {
    let mut records = Vec::new();
    for offset in [0xdd8, 0xdf0] {
        let (header, data, count) = r.array_header(tree + offset)?;
        let cap = i32::from_le_bytes(header[8..12].try_into().unwrap()) as usize;
        if cap > 2048 || (count != 0 && data % 8 != 0) || records.len() + count > 512 {
            return Err(Error::new(
                "invalid_array",
                "Current-node aggregate bound exceeded",
            ));
        }
        if count != 0 {
            records.extend(
                r.read(data, count * 40)?
                    .chunks_exact(40)
                    .map(|e| u64::from_le_bytes(e[16..24].try_into().unwrap())),
            );
        }
    }
    let mut seen = BTreeSet::new();
    let mut targets = Vec::new();
    let mut public_keys = BTreeMap::new();
    for &item in &records {
        if !seen.insert(item) {
            continue;
        }
        exact(r, item, base + 0x424e2e0)?;
        if r.u64(item + 0x30)? != tree {
            return Err(Error::new(
                "invalid_gui_owner",
                "Technology item must belong to own tree",
            ));
        }
        let technology = r.u64(item + 0xc8)?;
        let id = catalog.validate(r, base, status, technology)?;
        let public = &catalog.keys[id];
        if public_keys.get(public).is_some_and(|prior| *prior != id) {
            return Err(Error::new(
                "invalid_research_id",
                "Unique public node keys required",
            ));
        }
        public_keys.insert(public, id);
        if technology == catalog.technology {
            targets.push(item);
        }
    }
    if targets.is_empty() {
        return Err(Error::new(
            "node_unavailable",
            "Exact own current node unavailable",
        ));
    }
    // Complete every current item's ownership/canonical checks before GUI work.
    let mut reuse = crate::node_reuse::Reuse::new(base, tree);
    let mut bound = Vec::new();
    let mut target_type = None;
    for &item in &targets {
        let gui = r.u64(item + 0x10)?;
        let binding =
            crate::node_binding::observe(r, gui, catalog.technology, registry, &mut reuse)?;
        let encoding = reuse.nullable_type(r, gui)?;
        if target_type.as_ref().is_some_and(|prior| prior != &encoding) {
            return Err(Error::new(
                "ambiguous_gui_node",
                "Repeated canonical ID has conflicting GUI types",
            ));
        }
        target_type = Some(encoding);
        if binding["binding_verified"] == true {
            bound.push(item);
        }
    }
    if bound.len() > 1 {
        return Err(Error::new(
            "ambiguous_gui_node",
            "Multiple distinct bound items for requested key",
        ));
    }
    let render = if let Some(&item) = bound.first() {
        crate::node_detail::observe(r, &mut reuse, item, catalog.technology, registry)?
    } else {
        json!({"reason":"ordinary_binding_unavailable","client_rectangle_available":false,"input_authorized":false})
    };
    Ok(
        json!({"id":key,"record_count":records.len(),"distinct_current_items":seen.len(),"distinct_bound_items":bound.len(),"render_diagnostic":render,"client_rectangle_available":false,"input_authorized":false}),
    )
}
