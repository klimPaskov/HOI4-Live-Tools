//! Slot-specific callback proof. No node glue or button-data inheritance.
use crate::{
    error::{Error, Result},
    memory::Reader,
    research_ui::{array, exact, nullable_type, render_details},
};
use serde_json::{json, Value};
fn unavailable(reason: &str) -> Value {
    json!({"binding_verified":false,"reason":reason,"client_rectangle_available":false,"input_authorized":false})
}
pub(crate) fn observe(
    r: &Reader<'_>,
    base: u64,
    widget: u64,
    view: u64,
    slot: u64,
    registry: u64,
) -> Result<Value> {
    observe_inner(r, base, widget, view, slot, registry, false)
}
pub(crate) fn observe_geometry(
    r: &Reader<'_>,
    base: u64,
    widget: u64,
    view: u64,
    slot: u64,
    registry: u64,
) -> Result<Value> {
    observe_inner(r, base, widget, view, slot, registry, true)
}
fn observe_inner(
    r: &Reader<'_>,
    base: u64,
    widget: u64,
    view: u64,
    slot: u64,
    registry: u64,
    include_geometry: bool,
) -> Result<Value> {
    // Caller has already verified the complete own status/slot/view model.
    exact(r, widget, base + 0x424e7d8)?;
    exact(r, view, base + 0x424dc38)?;
    exact(r, slot, base + 0x4210898)?;
    exact(r, registry, base + 0x42736f8)?;
    if r.u64(widget + 0x2a8)? != registry
        || r.u64(widget + 0x2b0)? != view
        || r.u64(widget + 0x2b8)? != slot
    {
        return Err(Error::new(
            "invalid_research_join",
            "Own slot widget joins required",
        ));
    }
    let container = r.u64(widget + 0x10)?;
    if container == 0 {
        return Ok(unavailable("container_unmaterialized"));
    }
    if container % 8 != 0 {
        return Err(Error::new(
            "invalid_gui_type",
            "Slot container alignment required",
        ));
    }
    if r.u64(container)? != base + 0x4347d58 {
        return Ok(unavailable("unknown_container_layout"));
    }
    if r.u64(container + 0x98)? != registry {
        return Err(Error::new(
            "invalid_gui_owner",
            "Own slot container registry required",
        ));
    }
    let buttons = array(r, container + 0x958, 64)?;
    if buttons.len() != 1 {
        return Ok(unavailable("ordinary_control_ambiguous_or_absent"));
    }
    let button = buttons[0];
    if button == 0 {
        return Ok(unavailable("control_unmaterialized"));
    }
    let encoding = nullable_type(r, base, button)?;
    if r.u64(button)? != base + 0x4346b10 {
        let mut o = unavailable("unknown_control_layout");
        o["control_type_encoding"] = encoding;
        return Ok(o);
    }
    if r.u64(button + 0x68)? != registry {
        return Err(Error::new(
            "invalid_gui_owner",
            "Own slot control registry required",
        ));
    }
    if r.u64(button + 0x80)? != base + 0x4346ea8 {
        return Ok(unavailable("unknown_observer_layout"));
    }
    if r.u64(button + 0xa0)? != 0 {
        return Ok(unavailable("single_observer_override"));
    }
    let count = r.i32(button + 0x98)?;
    if !(0..=16).contains(&count) {
        return Err(Error::new(
            "invalid_gui_binding",
            "Slot listener count exceeds bound",
        ));
    }
    if count != 1 {
        return Ok(unavailable("listener_route_ambiguous_or_absent"));
    }
    let head = r.u64(button + 0x88)?;
    let tail = r.u64(button + 0x90)?;
    if head == 0 || head % 8 != 0 || head != tail {
        return Err(Error::new(
            "invalid_gui_binding",
            "Single slot listener endpoints malformed",
        ));
    }
    let target = r.u64(head)?;
    if r.u64(head + 8)? != 0 || r.u64(head + 0x10)? != 0 {
        return Err(Error::new(
            "invalid_gui_binding",
            "Single slot listener links malformed",
        ));
    }
    if r.byte(head + 0x18)? != 0 {
        return Ok(unavailable("listener_removed"));
    }
    if target != widget + 0x20 {
        return Ok(unavailable("different_listener_target"));
    }
    if r.u64(target)? != base + 0x424ea18 {
        return Ok(unavailable("unknown_slot_glue_layout"));
    }
    if r.u64(target + 0x18)? != base + 0x15b7be0
        || r.u64(target + 0x20)? != base + 0x15b7bb0
        || r.u64(target + 0x38)? != 0
        || r.u64(target + 0x1d8)? != 0
    {
        return Ok(unavailable("different_slot_callback_route"));
    }
    let capture = r.u64(target + 8)?;
    if capture == 0 {
        return Ok(unavailable("callback_unmaterialized"));
    }
    if capture % 8 != 0 {
        return Err(Error::new(
            "invalid_gui_binding",
            "Slot callback capture alignment required",
        ));
    }
    if r.u64(capture)? != base + 0x15b35e0
        || r.u64(capture + 8)? != 0
        || r.u64(capture + 0x10)? != widget
    {
        return Ok(unavailable("different_slot_callback_capture"));
    }
    // Names come only from the independently proved paired named-button registry.
    // Anonymous installed Background declarations never imply a runtime name.
    let named = array(r, container + 0x9a0, 256)?;
    let (h, names, n) = r.array_header(container + 0x9b8)?;
    let cap = i32::from_le_bytes(h[8..12].try_into().unwrap());
    if cap > 256 || n != named.len() || (n > 0 && names % 8 != 0) {
        return Err(Error::new(
            "invalid_gui_registry",
            "Bounded paired slot control names required",
        ));
    }
    let mut registered = Vec::new();
    for (index, &p) in named.iter().enumerate() {
        if p == button {
            let name = r.string_bounded(names + index as u64 * 32, 256)?;
            if name.chars().any(char::is_control) {
                return Err(Error::new(
                    "invalid_gui_registry",
                    "Control name contains control characters",
                ));
            }
            if !name.is_empty() {
                registered.push(name);
            }
        }
    }
    let render = r.u64(button + 0x110)?;
    let render_type = nullable_type(r, base, render)?;
    let mut details = json!({"field_layout_verified":false});
    let render_table = if render == 0 { 0 } else { r.u64(render)? };
    if render != 0
        && [0x4356e08, 0x434c5b0, 0x434b370]
            .iter()
            .any(|v| render_table == base + v)
    {
        if r.u64(render + 0x138)? != button {
            return Err(Error::new(
                "invalid_gui_owner",
                "Own slot renderer control join required",
            ));
        }
        details = render_details(r, base, render)?;
        if render_table == base + 0x4356e08 {
            let f = |off| -> Result<Value> {
                let v = f32::from_le_bytes(r.read(render + off, 4)?.try_into().unwrap());
                Ok(if v.is_finite() { json!(v) } else { Value::Null })
            };
            details["raw_world_x"] = f(0xb0)?;
            details["raw_world_y"] = f(0xb4)?;
            details["raw_local_scalar"] = f(0x114)?;
            details["cache_currency_verified"] = json!(false);
        }
    }
    let render_diagnostic = if include_geometry {
        let mut reuse = crate::node_reuse::Reuse::new(base, view);
        crate::node_detail::sprite_diagnostic(r, &mut reuse, container, button, registry)?
    } else {
        json!({"reason":"not_requested","field_layout_verified":false,"input_authorized":false})
    };
    Ok(
        json!({"binding_verified":true,"meaning":"one registered ordinary callback targets this own slot widget","control_type_encoding":encoding,"registered_names":registered,"named_association_observed":!registered.is_empty(),"constructor_resource_literal":"research_slot_entry","background_name_inferred":false,"render_type_encoding":render_type,"render_details":details,"render_diagnostic":render_diagnostic,"client_rectangle_available":false,"effective_routing_verified":false,"input_authorized":false}),
    )
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
            for (i, &v) in b.iter().enumerate() {
                self.data.insert(a + i as u64, v);
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
        fn metadata(&mut self, v: u64, info: u64, name: u64, text: &str) {
            self.region(v - 16, 32);
            self.word(v - 8, info);
            self.region(info, 16);
            self.word(info + 8, name);
            self.region(name, 128);
            self.put(name, text.as_bytes());
        }
        fn new() -> Self {
            let mut f = Self {
                data: BTreeMap::new(),
                reads: RefCell::new(BTreeMap::new()),
                fault: None,
            };
            let b = 0x10000;
            for (a, n, t) in [
                (0x20000, 0x300, 0x424e7d8),
                (0x30000, 0x400, 0x424dc38),
                (0x40000, 0x50, 0x4210898),
                (0x50000, 0x200, 0x42736f8),
                (0x60000, 0x1000, 0x4347d58),
                (0x70000, 0x160, 0x4346b10),
            ] {
                f.region(a, n);
                f.word(a, b + t);
            }
            f.word(0x202a8, 0x50000);
            f.word(0x202b0, 0x30000);
            f.word(0x202b8, 0x40000);
            f.word(0x20010, 0x60000);
            f.word(0x60098, 0x50000);
            f.word(0x70068, 0x50000);
            f.array(0x60958, 0xc0000, 1);
            f.word(0xc0000, 0x70000);
            f.word(0x70080, b + 0x4346ea8);
            f.word(0x70088, 0x80000);
            f.word(0x70090, 0x80000);
            f.put(0x70098, &1i32.to_le_bytes());
            f.region(0x80000, 0x20);
            f.word(0x80000, 0x20020);
            f.word(0x20020, b + 0x424ea18);
            f.word(0x20028, 0x90000);
            f.word(0x20038, b + 0x15b7be0);
            f.word(0x20040, b + 0x15b7bb0);
            f.region(0x90000, 0x18);
            f.word(0x90000, b + 0x15b35e0);
            f.word(0x90010, 0x20000);
            f.metadata(b + 0x4346b10, b + 0x4357800, b + 0x350a000, "11CButtonDrag");
            f
        }
        fn observe(&self) -> Result<Value> {
            self.begin_snapshot()?;
            let w = Witnesses::new(self);
            let r = Reader::new(&w);
            let v = super::observe(&r, 0x10000, 0x20000, 0x30000, 0x40000, 0x50000)?;
            w.finish()?;
            self.check()?;
            Ok(v)
        }
    }
    impl ReadMemory for Fixture {
        fn read(&self, a: u64, n: usize) -> Result<Vec<u8>> {
            let mut reads = self.reads.borrow_mut();
            let count = reads.entry(a).or_default();
            *count += 1;
            if let Some((address, b)) = &self.fault {
                if a == *address && *count > 1 {
                    assert_eq!(b.len(), n);
                    return Ok(b.clone());
                }
            }
            (0..n)
                .map(|i| {
                    self.data
                        .get(&(a + i as u64))
                        .copied()
                        .ok_or_else(|| Error::new("unexpected_read", "Unmapped fixture payload"))
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
    fn exact_slot_capture_is_not_node_glue_or_named_background() {
        let f = Fixture::new();
        let v = f.observe().unwrap();
        assert_eq!(v["binding_verified"], true);
        assert_eq!(v["named_association_observed"], false);
        assert_eq!(v["input_authorized"], false);
        assert!(!f.reads.borrow().contains_key(&0x70008));
    }
    #[test]
    fn registry_and_slot_joins_precede_controls() {
        for (a, v, blocked) in [
            (0x202a8, 0x50008, 0x60958),
            (0x202b8, 0x40008, 0x60958),
            (0x60098, 0x50008, 0x60958),
            (0x70068, 0x50008, 0x70088),
        ] {
            let mut f = Fixture::new();
            f.word(a, v);
            assert!(f.observe().is_err());
            assert!(!f.reads.borrow().contains_key(&blocked));
        }
    }
    #[test]
    fn overrides_and_other_targets_never_follow_foreign_capture() {
        let mut f = Fixture::new();
        f.word(0x700a0, 0xdead0000);
        assert_eq!(f.observe().unwrap()["reason"], "single_observer_override");
        assert!(!f.reads.borrow().contains_key(&0x70088));
        let mut f = Fixture::new();
        f.word(0x80000, 0xdead0000);
        assert_eq!(f.observe().unwrap()["reason"], "different_listener_target");
        assert!(!f.reads.borrow().contains_key(&0xdead0000));
    }
    #[test]
    fn wrong_callback_member_adjustment_this_or_invoker_is_unavailable() {
        for (a, v) in [
            (0x90000, 0x10000 + 0x1599290),
            (0x90008, 8),
            (0x90010, 0x20008),
            (0x20040, 0x10000 + 0x15a56e0),
        ] {
            let mut f = Fixture::new();
            f.word(a, v);
            assert_eq!(f.observe().unwrap()["binding_verified"], false);
        }
    }
    #[test]
    fn named_registry_matches_exact_control_only() {
        let mut f = Fixture::new();
        f.array(0x609a0, 0xc1000, 1);
        f.word(0xc1000, 0x70000);
        f.array(0x609b8, 0xc2000, 1);
        f.region(0xc2000, 32);
        f.word(0xc2000, 0xc2010);
        f.word(0xc2008, 10);
        f.put(0xc2010, b"slot_click");
        let v = f.observe().unwrap();
        assert_eq!(v["registered_names"][0], "slot_click");
        assert_eq!(v["named_association_observed"], true);
    }
    #[test]
    fn exact_renderer_owner_precedes_resource_or_raw_fields() {
        let mut f = Fixture::new();
        let b = 0x10000;
        f.word(0x70110, 0xa0000);
        f.region(0xa0000, 0x1b0);
        f.word(0xa0000, b + 0x4356e08);
        f.metadata(b + 0x4356e08, b + 0x4357900, b + 0x350a100, "7CSprite");
        f.word(0xa0138, 0x70008);
        assert_eq!(f.observe().unwrap_err().code, "invalid_gui_owner");
        assert!(!f.reads.borrow().contains_key(&0xa0168));
        assert!(!f.reads.borrow().contains_key(&0xa00b0));
        f.word(0xa0138, 0x70000);
        let v = f.observe().unwrap();
        assert_eq!(v["render_details"]["raw_world_x"], 0.0);
    }
    #[test]
    fn single_listener_structure_and_count_bounds_refuse() {
        for (a, b) in [
            (0x70098, 17i32.to_le_bytes().to_vec()),
            (0x80008, 1u64.to_le_bytes().to_vec()),
            (0x80010, 0x80000u64.to_le_bytes().to_vec()),
        ] {
            let mut f = Fixture::new();
            f.put(a, &b);
            assert_eq!(f.observe().unwrap_err().code, "invalid_gui_binding");
        }
    }
    #[test]
    fn all_slot_proof_fields_survive_final_witness() {
        for (a, n) in [
            (0x202a8, 8),
            (0x202b0, 8),
            (0x202b8, 8),
            (0x60098, 8),
            (0x60958, 16),
            (0xc0000, 8),
            (0x70068, 8),
            (0x70080, 8),
            (0x700a0, 8),
            (0x80000, 8),
            (0x20020, 8),
            (0x20028, 8),
            (0x20038, 8),
            (0x20040, 8),
            (0x90000, 8),
            (0x90008, 8),
            (0x90010, 8),
        ] {
            let mut f = Fixture::new();
            let mut b = (0..n).map(|i| f.data[&(a + i)]).collect::<Vec<_>>();
            b[0] ^= 1;
            f.fault = Some((a, b));
            assert_eq!(f.observe().unwrap_err().code, "torn_read");
        }
    }
    #[test]
    fn ambiguous_control_does_not_guess_first() {
        let mut f = Fixture::new();
        f.array(0x60958, 0xc0000, 2);
        f.word(0xc0008, 0xdead0000);
        assert_eq!(f.observe().unwrap()["binding_verified"], false);
        assert!(!f.reads.borrow().contains_key(&0x70068));
        assert!(!f.reads.borrow().contains_key(&0xdead0000));
    }
}
