//! Offline category association candidate. Caller must verify full own mode6
//! context and use this Reader with the production final Witnesses.
use crate::{
    error::{Error, Result},
    memory::Reader,
    research_ui::{array, exact},
};
use serde_json::{json, Value};
fn refuse(message: &str) -> Error {
    Error::new("invalid_category_ui", message)
}
fn container(r: &Reader<'_>, base: u64, p: u64, registry: u64) -> Result<()> {
    exact(r, p, base + 0x4347d58)?;
    if r.u64(p + 0x98)? != registry {
        return Err(refuse("Foreign category container registry"));
    }
    Ok(())
}
fn names(r: &Reader<'_>, p: u64, pointers: u64, strings: u64) -> Result<Vec<(String, u64)>> {
    let values = array(r, p + pointers, 128)?;
    let (header, data, count) = r.array_header(p + strings)?;
    let cap = i32::from_le_bytes(header[8..12].try_into().unwrap());
    if cap > 128 || count != values.len() || (count > 0 && data % 8 != 0) {
        return Err(refuse("Category named-array bounds/join"));
    }
    let mut out = Vec::new();
    let mut seen = std::collections::BTreeSet::new();
    for (i, v) in values.into_iter().enumerate() {
        let key = r.string_bounded(data + i as u64 * 32, 256)?;
        if key.is_empty()
            || key.chars().any(char::is_control)
            || !seen.insert(key.to_ascii_lowercase())
        {
            return Err(refuse("Empty/control/duplicate category name"));
        }
        out.push((key, v));
    }
    Ok(out)
}
fn lookup(values: &[(String, u64)], key: &str) -> Option<u64> {
    values.iter().find(|(k, _)| k == key).map(|(_, v)| *v)
}
#[allow(clippy::too_many_arguments)]
pub(crate) fn observe(
    r: &Reader<'_>,
    base: u64,
    tree: u64,
    root: u64,
    registry: u64,
    own_status: u64,
    key: &str,
) -> Result<Value> {
    observe_inner(r, base, tree, root, registry, own_status, key, false)
}
#[allow(clippy::too_many_arguments)]
pub(crate) fn observe_geometry(
    r: &Reader<'_>,
    base: u64,
    tree: u64,
    root: u64,
    registry: u64,
    own_status: u64,
    key: &str,
) -> Result<Value> {
    observe_inner(r, base, tree, root, registry, own_status, key, true)
}
#[allow(clippy::too_many_arguments)]
fn observe_inner(
    r: &Reader<'_>,
    base: u64,
    tree: u64,
    root: u64,
    registry: u64,
    own_status: u64,
    key: &str,
    include_geometry: bool,
) -> Result<Value> {
    let mut out = json!({"folder_key":key,"selected_folder_key":null,"selected_index":null,"binding_available":false,"control_type_encoding":null,"raw_flags":null,"reason":"category_unavailable","client_rectangle_available":false,"geometry_available":false,"current_frame_verified":false,"effective_routing_verified":false,"input_authorized":false});
    if key.is_empty() || key.len() > 256 || key.chars().any(char::is_control) {
        return Err(refuse("Invalid category key"));
    }
    exact(r, tree, base + 0x424de40)?;
    exact(r, registry, base + 0x42736f8)?;
    exact(r, own_status, base + 0x42108f8)?;
    let own_tag = r.i32(own_status + 0x138)?;
    let displayed = r.i32(tree + 0xe20)?;
    if own_tag <= 0
        || (displayed > 0 && displayed != own_tag)
        || r.i32(tree + 0xe24)? != 6
        || r.u64(tree + 0x2f8)? != root
    {
        return Err(refuse("Own category context required"));
    }
    let slot = r.u64(tree + 0xe08)?;
    exact(r, slot, base + 0x4210898)?;
    if r.u64(slot + 0x10)? != own_status {
        return Err(refuse("Foreign selected research slot"));
    }
    container(r, base, root, registry)?;
    let db = r.u64(base + 0x439f450)?;
    exact(r, db, base + 0x42110c8)?;
    let folders = array(r, db + 0xa8, 128)?;
    let sentinel = r.u64(base + 0x43a40e0)?;
    exact(r, sentinel, base + 0x42113f0)?;
    if folders.first().copied() != Some(sentinel) {
        return Err(refuse("Reserved category singleton join"));
    }
    let mut canonical = std::collections::BTreeSet::new();
    let mut found = false;
    for (index, p) in folders.into_iter().enumerate() {
        if index == 0 {
            continue;
        } // Exact singleton identity and primary checked above; no payload.
        exact(r, p, base + 0x42112c0)?;
        let k = r.string_bounded(p + 8, 256)?;
        if k.is_empty() || k.chars().any(char::is_control) || !canonical.insert(k.clone()) {
            return Err(refuse("Malformed canonical category key"));
        }
        if k == key {
            found = true;
        }
    }
    if !found {
        out["reason"] = json!("unknown_canonical_folder_key");
        return Ok(out);
    }
    let roots = names(r, root, 0xbb0, 0xbc8)?;
    let Some(tabs) = lookup(&roots, "folder_tabs") else {
        return Ok(out);
    };
    container(r, base, tabs, registry)?;
    let named = names(r, tabs, 0x9a0, 0x9b8)?;
    let windows = array(r, tree + 0xd78, 128)?;
    let controls = array(r, tree + 0xd90, 128)?;
    if windows.is_empty() || windows.len() != controls.len() {
        return Err(refuse("Dense category array mismatch"));
    }
    let selected = r.i32(tree + 0xe10)?;
    if selected < 0 || selected as usize >= windows.len() {
        return Err(refuse("Selected category index bound"));
    }
    // Root names provide a public selected key without reading sibling controls.
    let selected_keys = roots
        .iter()
        .filter(|(k, v)| canonical.contains(k) && *v == windows[selected as usize])
        .map(|(k, _)| k)
        .collect::<Vec<_>>();
    if selected_keys.len() != 1 {
        return Err(refuse("Selected category name ambiguity"));
    }
    container(r, base, windows[selected as usize], registry)?;
    out["selected_folder_key"] = json!(selected_keys[0]);
    out["selected_index"] = json!(selected);
    let Some(window) = lookup(&roots, key) else {
        return Ok(out);
    };
    container(r, base, window, registry)?;
    let Some(button) = lookup(&named, &format!("{key}_tab")) else {
        return Ok(out);
    };
    if button == 0 || button % 8 != 0 {
        return Err(refuse("Category button alignment"));
    }
    let table = r.u64(button)?;
    let (observer, encoding) = match table.checked_sub(base) {
        Some(0x4346b10) => (0x4346ea8, "11CButtonDrag"),
        Some(0x43470c8) => (0x4347488, "15CButtonStandard"),
        _ => {
            out["reason"] = json!("unknown_control_layout");
            return Ok(out);
        }
    };
    out["control_type_encoding"] = json!(encoding);
    if r.u64(button + 0x68)? != registry {
        return Err(refuse("Foreign category control registry"));
    }
    let index = r.i32(button + 0xc8)?;
    if index < 0
        || index as usize >= windows.len()
        || windows[index as usize] != window
        || controls[index as usize] != button
        || r.u64(button + 8)? != window
        || windows.iter().filter(|v| **v == window).count() != 1
        || controls.iter().filter(|v| **v == button).count() != 1
    {
        return Err(refuse("Category dense-index/data join"));
    }
    out["raw_flags"] = json!(r.i32(button + 0x74)? as u32);
    if r.u64(button + 0x80)? != base + observer {
        out["reason"] = json!("unknown_observer_layout");
        return Ok(out);
    }
    if r.u64(button + 0xa0)? != 0 {
        out["reason"] = json!("observer_override");
        return Ok(out);
    }
    let count = r.i32(button + 0x98)?;
    if !(0..=16).contains(&count) {
        return Err(refuse("Category listener bound"));
    }
    if count != 1 {
        out["reason"] = json!("ambiguous_listener_route");
        return Ok(out);
    }
    let head = r.u64(button + 0x88)?;
    if head == 0
        || head % 8 != 0
        || head != r.u64(button + 0x90)?
        || r.u64(head + 8)? != 0
        || r.u64(head + 0x10)? != 0
        || r.byte(head + 0x18)? != 0
    {
        return Err(refuse("Malformed category listener"));
    }
    let glue = tree + 0x358;
    if r.u64(head)? != glue {
        out["reason"] = json!("different_listener_target");
        return Ok(out);
    }
    if r.u64(glue)? != base + 0x424e0a8
        || r.u64(glue + 0x38)? != base + 0x15a5710
        || r.u64(glue + 0x40)? != base + 0x15a56e0
        || r.u64(glue + 0x18)? != 0
        || r.u64(glue + 0x1d8)? != 0
    {
        out["reason"] = json!("different_callback_variant");
        return Ok(out);
    }
    let capture = r.u64(glue + 0x28)?;
    if capture == 0 || capture % 8 != 0 {
        return Err(refuse("Category capture alignment"));
    }
    if r.u64(capture)? != base + 0x1599270
        || r.u64(capture + 8)? != 0
        || r.u64(capture + 0x10)? != tree
    {
        out["reason"] = json!("different_callback_capture");
        return Ok(out);
    }
    out["binding_available"] = json!(true);
    out["reason"] = json!("exact_ordinary_category_association");
    if !include_geometry {
        return Ok(out);
    }
    out["sprite_diagnostic"] = if table != base + 0x4346b10 {
        // Shared sprite_diagnostic is constructor-proved only for exact Drag110.
        // Standard rendererE8 is not substituted or read.
        json!({"reason":"standard_sprite_diagnostic_unavailable","field_layout_verified":false,"client_rectangle_available":false,"input_authorized":false})
    } else {
        let mut reuse = crate::node_reuse::Reuse::new(base, tree);
        crate::node_detail::sprite_diagnostic(r, &mut reuse, tabs, button, registry)?
    };
    Ok(out)
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::{memory::ReadMemory, platform::Process, research_ui::Witnesses};
    use std::{cell::RefCell, collections::BTreeMap};
    struct Fixture {
        data: BTreeMap<u64, u8>,
        reads: RefCell<BTreeMap<u64, usize>>,
        fault: Option<(u64, Vec<u8>)>,
    }
    impl Fixture {
        fn put(&mut self, a: u64, b: &[u8]) {
            for (i, v) in b.iter().enumerate() {
                self.data.insert(a + i as u64, *v);
            }
        }
        fn region(&mut self, a: u64, n: usize) {
            self.put(a, &vec![0; n]);
        }
        fn word(&mut self, a: u64, v: u64) {
            self.put(a, &v.to_le_bytes());
        }
        fn array(&mut self, a: u64, p: u64, n: i32) {
            self.region(a, 16);
            self.word(a, p);
            self.put(a + 8, &n.to_le_bytes());
            self.put(a + 12, &n.to_le_bytes());
        }
        fn string(&mut self, a: u64, text: &str) {
            self.region(a, 32);
            self.word(a, a + 16);
            self.word(a + 8, text.len() as u64);
            self.put(a + 16, text.as_bytes());
        }
        fn new() -> Self {
            let mut f = Self {
                data: BTreeMap::new(),
                reads: RefCell::new(BTreeMap::new()),
                fault: None,
            };
            let b = 0x10000;
            for (p, n, t) in [
                (0x20000, 0xe30, 0x424de40),
                (0x30000, 0xc00, 0x4347d58),
                (0x40000, 0xc00, 0x4347d58),
                (0x50000, 0xc00, 0x4347d58),
                (0x60000, 0x160, 0x4346b10),
                (0x70000, 0x200, 0x42108f8),
                (0x80000, 0x200, 0x42736f8),
                (0x90000, 0xc0, 0x42110c8),
                (0xa0000, 0xd0, 0x42112c0),
                (0xb0000, 0x30, 0x4210898),
            ] {
                f.region(p, n);
                f.word(p, b + t);
            }
            f.word(0x202f8, 0x30000);
            f.put(0x20e24, &6i32.to_le_bytes());
            f.word(0x20e08, 0xb0000);
            f.word(0xb0010, 0x70000);
            f.put(0x70138, &1i32.to_le_bytes());
            for p in [0x30000, 0x40000, 0x50000] {
                f.word(p + 0x98, 0x80000);
            }
            f.word(b + 0x439f450, 0x90000);
            f.array(0x900a8, 0xc0000, 2);
            f.word(0xc0000, 0xa2000);
            f.word(0xc0008, 0xa0000);
            f.word(b + 0x43a40e0, 0xa2000);
            f.word(0xa2000, b + 0x42113f0);
            f.string(0xa0008, "industry_folder");
            f.array(0x30bb0, 0xc1000, 2);
            f.word(0xc1000, 0x40000);
            f.word(0xc1008, 0x50000);
            f.array(0x30bc8, 0xc2000, 2);
            f.string(0xc2000, "folder_tabs");
            f.string(0xc2020, "industry_folder");
            f.array(0x409a0, 0xc3000, 1);
            f.word(0xc3000, 0x60000);
            f.array(0x409b8, 0xc4000, 1);
            // External string exceeds SSO15.
            f.region(0xc4000, 32);
            f.word(0xc4000, 0xc5000);
            f.word(0xc4008, 19);
            f.put(0xc5000, b"industry_folder_tab");
            f.array(0x20d78, 0xc6000, 1);
            f.word(0xc6000, 0x50000);
            f.array(0x20d90, 0xc7000, 1);
            f.word(0xc7000, 0x60000);
            f.word(0x60068, 0x80000);
            f.word(0x60008, 0x50000);
            f.word(0x60080, b + 0x4346ea8);
            f.word(0x60088, 0xd0000);
            f.word(0x60090, 0xd0000);
            f.put(0x60098, &1i32.to_le_bytes());
            f.region(0xd0000, 32);
            f.word(0xd0000, 0x20358);
            f.word(0x20358, b + 0x424e0a8);
            f.word(0x20380, 0xe0000);
            f.word(0x20390, b + 0x15a5710);
            f.word(0x20398, b + 0x15a56e0);
            f.region(0xe0000, 24);
            f.word(0xe0000, b + 0x1599270);
            f.word(0xe0010, 0x20000);
            f
        }
        fn observe(&self) -> Result<Value> {
            let w = Witnesses::new(self);
            let r = Reader::new(&w);
            let v = super::observe(
                &r,
                0x10000,
                0x20000,
                0x30000,
                0x80000,
                0x70000,
                "industry_folder",
            )?;
            w.finish()?;
            Ok(v)
        }
    }
    impl ReadMemory for Fixture {
        fn read(&self, a: u64, n: usize) -> Result<Vec<u8>> {
            let mut reads = self.reads.borrow_mut();
            let count = reads.entry(a).or_default();
            *count += 1;
            if let Some((p, b)) = &self.fault {
                if a == *p && *count > 1 {
                    assert_eq!(b.len(), n);
                    return Ok(b.clone());
                }
            }
            (0..n)
                .map(|i| {
                    self.data
                        .get(&(a + i as u64))
                        .copied()
                        .ok_or_else(|| refuse("Unmapped fixture"))
                })
                .collect()
        }
    }
    impl Process for Fixture {
        fn image_base(&self) -> u64 {
            0x10000
        }
        fn check(&self) -> Result<()> {
            Ok(())
        }
    }
    #[test]
    fn exact_drag_and_standard_associations() {
        for (t, o) in [(0x4346b10, 0x4346ea8), (0x43470c8, 0x4347488)] {
            let mut f = Fixture::new();
            f.word(0x60000, 0x10000 + t);
            f.word(0x60080, 0x10000 + o);
            let v = f.observe().unwrap();
            assert_eq!(v["binding_available"], true);
            assert_eq!(v["input_authorized"], false);
            assert_eq!(v["selected_folder_key"], "industry_folder");
        }
    }
    #[test]
    fn foreign_registry_refuses_before_payload() {
        let mut f = Fixture::new();
        f.word(0x40098, 0x80008);
        assert!(f.observe().is_err());
        assert!(!f.reads.borrow().contains_key(&0x409a0));
        let mut f = Fixture::new();
        f.word(0x60068, 0x80008);
        assert!(f.observe().is_err());
        assert!(!f.reads.borrow().contains_key(&0x600c8));
    }
    #[test]
    fn override_and_wrong_capture_unavailable() {
        let mut f = Fixture::new();
        f.word(0x600a0, 0xdead0000);
        assert_eq!(f.observe().unwrap()["reason"], "observer_override");
        assert!(!f.reads.borrow().contains_key(&0x60088));
        let mut f = Fixture::new();
        f.word(0xe0010, 0x20008);
        assert_eq!(f.observe().unwrap()["binding_available"], false);
    }
    #[test]
    fn duplicate_names_and_dense_index_refuse() {
        let mut f = Fixture::new();
        f.string(0xc2020, "folder_tabs");
        assert!(f.observe().is_err());
        let mut f = Fixture::new();
        f.put(0x600c8, &1i32.to_le_bytes());
        assert!(f.observe().is_err());
    }
    #[test]
    fn capture_mutation_detected_by_final_witness() {
        let mut f = Fixture::new();
        f.fault = Some((0xe0010, 0x20008u64.to_le_bytes().to_vec()));
        assert_eq!(f.observe().unwrap_err().code, "torn_read");
    }
    #[test]
    fn unknown_control_stops_before_owner_payload() {
        let mut f = Fixture::new();
        f.word(0x60000, 0x10000 + 0x4347000);
        assert_eq!(f.observe().unwrap()["reason"], "unknown_control_layout");
        assert!(!f.reads.borrow().contains_key(&0x60068));
    }
    #[test]
    fn reserved_zero_requires_identity_and_primary_without_payload() {
        let f = Fixture::new();
        assert_eq!(f.observe().unwrap()["binding_available"], true);
        assert!(!f.reads.borrow().contains_key(&0xa2008));
        for (a, v) in [
            (0xc0000, 0xa0000),
            (0x10000 + 0x43a40e0, 0xa0000),
            (0xa2000, 0x10000 + 0x4211420),
        ] {
            let mut f = Fixture::new();
            f.word(a, v);
            assert!(f.observe().is_err());
            assert!(!f.reads.borrow().contains_key(&0xa2008));
        }
    }
    #[test]
    fn singleton_final_witness_mutation_refuses() {
        let mut f = Fixture::new();
        f.fault = Some((0x10000 + 0x43a40e0, 0xa2008u64.to_le_bytes().to_vec()));
        assert_eq!(f.observe().unwrap_err().code, "torn_read");
    }
    #[test]
    fn foreign_selected_other_folder_refuses_before_target_control() {
        let mut f = Fixture::new();
        f.region(0xa1000, 0xd0);
        f.word(0xa1000, 0x10000 + 0x42112c0);
        f.string(0xa1008, "electronics");
        f.array(0x900a8, 0xc0000, 3);
        f.word(0xc0010, 0xa1000);
        f.array(0x30bb0, 0xc1000, 3);
        f.word(0xc1010, 0x51000);
        f.array(0x30bc8, 0xc2000, 3);
        f.string(0xc2040, "electronics");
        f.region(0x51000, 0xa0);
        f.word(0x51000, 0x10000 + 0x4347d58);
        f.word(0x51098, 0x80008);
        f.array(0x20d78, 0xc6000, 2);
        f.word(0xc6008, 0x51000);
        f.array(0x20d90, 0xc7000, 2);
        f.word(0xc7008, 0xdead0000);
        f.put(0x20e10, &1i32.to_le_bytes());
        assert!(f.observe().is_err());
        assert!(f.reads.borrow().contains_key(&0x51098));
        assert!(!f.reads.borrow().contains_key(&0x60068));
    }
    impl Fixture {
        fn geometry(&self) -> Result<Value> {
            let w = Witnesses::new(self);
            let r = Reader::new(&w);
            let v = super::observe_geometry(
                &r,
                0x10000,
                0x20000,
                0x30000,
                0x80000,
                0x70000,
                "industry_folder",
            )?;
            w.finish()?;
            Ok(v)
        }
        fn metadata(&mut self, table: u64, info: u64, name: u64, text: &str) {
            self.region(table - 16, 24);
            self.word(table - 8, info);
            self.region(info, 16);
            self.word(info + 8, name);
            self.region(name, 128);
            self.put(name, text.as_bytes());
        }
        fn sprite(&mut self) {
            let b = 0x10000;
            self.region(0xf0000, 0x1b0);
            self.word(0x60110, 0xf0000);
            self.word(0xf0000, b + 0x4356e08);
            self.word(0xf0138, 0x60000);
            self.metadata(b + 0x4356e08, b + 0x4357900, b + 0x350a000, "7CSprite");
            self.region(0xf1000, 0x1f0);
            self.word(0xf0168, 0xf1000);
            self.word(0xf1000, b + 0x4357160);
            self.metadata(b + 0x4357160, b + 0x4357910, b + 0x350a100, "11CSpriteType");
            self.put(0xf0114, &1.0f32.to_le_bytes());
            for a in [0xf1090, 0xf1094, 0xf10a4, 0xf10a8] {
                self.put(a, &1i32.to_le_bytes());
            }
            self.region(0xf2000, 0x500);
            self.word(b + 0x4577890, 0xf2000);
            self.word(0xf2000, b + 0x42417f8);
            self.word(0x800b8, 0xf2000);
            self.word(0xf21f0, b + 0x43484f0);
            self.word(0xf2350, b + 0x43484f0);
            self.region(0x40e70, 16);
            self.word(0x40e70, 0xf3000);
            self.word(0x40e78, 0xf4000);
            self.region(0xf3000, 0x160);
            self.region(0xf4000, 0x160);
            self.word(0xf3000, b + 0x43484f0);
            self.word(0xf4000, b + 0x43484f0);
            self.word(0xf4108, 0xf3000);
            self.array(0xf30c0, 0xf5000, 1);
            self.word(0xf5000, 0xf4000);
        }
    }
    #[test]
    fn geometry_opt_in_preserves_association_and_raw_limits() {
        let mut f = Fixture::new();
        f.sprite();
        let v = f.geometry().unwrap();
        assert_eq!(v["binding_available"], true);
        assert_eq!(v["sprite_diagnostic"]["field_layout_verified"], true);
        assert_eq!(
            v["sprite_diagnostic"]["parent_chain"]["status"],
            "detached_ancestor"
        );
        assert_eq!(v["input_authorized"], false);
        assert_eq!(v["geometry_available"], false);
        assert!(f.observe().unwrap().get("sprite_diagnostic").is_none());
    }
    #[test]
    fn standard_geometry_has_no_borrowed_drag_fields() {
        let mut f = Fixture::new();
        f.word(0x60000, 0x10000 + 0x43470c8);
        f.word(0x60080, 0x10000 + 0x4347488);
        let v = f.geometry().unwrap();
        assert_eq!(
            v["sprite_diagnostic"]["reason"],
            "standard_sprite_diagnostic_unavailable"
        );
        assert!(!f.reads.borrow().contains_key(&0x600e8));
        assert!(!f.reads.borrow().contains_key(&0x60110));
    }
    #[test]
    fn invalid_callback_precedes_render_read() {
        let mut f = Fixture::new();
        f.word(0x600a0, 0xdead0000);
        let v = f.geometry().unwrap();
        assert_eq!(v["binding_available"], false);
        assert!(!f.reads.borrow().contains_key(&0x60110));
    }
    #[test]
    fn wrong_sprite_owner_precedes_resource_payload() {
        let mut f = Fixture::new();
        f.sprite();
        f.word(0xf0138, 0x60008);
        assert_eq!(f.geometry().unwrap_err().code, "invalid_gui_owner");
        assert!(!f.reads.borrow().contains_key(&0xf0168));
    }
    #[test]
    fn sprite_metadata_and_cache_survive_final_witness() {
        for (a, bytes) in [
            (0xf0138, 0x60008u64.to_le_bytes().to_vec()),
            (0xf0114, 2.0f32.to_le_bytes().to_vec()),
            (
                0x10000 + 0x4356e08 - 8,
                (0x10000 + 0x4357920u64).to_le_bytes().to_vec(),
            ),
        ] {
            let mut f = Fixture::new();
            f.sprite();
            f.fault = Some((a, bytes));
            assert_eq!(f.geometry().unwrap_err().code, "torn_read");
        }
    }
    #[test]
    fn null_render_is_unavailable_not_zero_geometry() {
        let f = Fixture::new();
        let v = f.geometry().unwrap();
        assert_eq!(v["binding_available"], true);
        assert_eq!(v["sprite_diagnostic"]["field_layout_verified"], false);
        assert_eq!(v["sprite_diagnostic"]["render_type_encoding"], Value::Null);
    }
    #[test]
    fn malformed_numeric_and_reciprocal_fields_refuse() {
        let mut f = Fixture::new();
        f.sprite();
        f.put(0xf0114, &f32::NAN.to_le_bytes());
        assert_eq!(f.geometry().unwrap_err().code, "invalid_gui_geometry");
        let mut f = Fixture::new();
        f.sprite();
        f.word(0xf4108, 0xf3008);
        assert_eq!(f.geometry().unwrap_err().code, "invalid_gui_position");
    }
}
