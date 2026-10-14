//! Generated memory maps exercise the real Standard/Sprite/hierarchy producer
//! and consumer. These are not live game, process ownership or input evidence.
use super::*;
use crate::{error::Error, memory::ReadMemory, research_ui::Witnesses};
use std::{
    cell::{Cell, RefCell},
    collections::BTreeMap,
};
const BASE: u64 = 0x10000;
const REG: u64 = 0x20000;
const GRAPH: u64 = 0x30000;
const WINDOW: u64 = 0x40000;
const P0: u64 = 0x50000;
const P1: u64 = 0x51000;
const BUTTON: u64 = 0x60000;
const RENDER: u64 = 0x70000;
const RESOURCE: u64 = 0x80000;
struct Fixture {
    data: BTreeMap<u64, u8>,
    reads: RefCell<Vec<(u64, usize)>>,
    torn: Option<u64>,
    visits: Cell<usize>,
}
impl Fixture {
    fn put(&mut self, a: u64, b: &[u8]) {
        for (i, v) in b.iter().enumerate() {
            self.data.insert(a + i as u64, *v);
        }
    }
    fn zero(&mut self, a: u64, n: usize) {
        self.put(a, &vec![0; n]);
    }
    fn word(&mut self, a: u64, n: u64) {
        self.put(a, &n.to_le_bytes());
    }
    fn number(&mut self, a: u64, n: i32) {
        self.put(a, &n.to_le_bytes());
    }
    fn array(&mut self, a: u64, data: u64, child: u64) {
        self.zero(a, 16);
        self.word(a, data);
        self.number(a + 8, 1);
        self.number(a + 12, 1);
        self.word(data, child);
    }
    fn metadata(&mut self, table: u64, index: u64, name: &str) {
        let info = BASE + 0x435a000 + index * 0x100;
        let text = BASE + 0x3510000 + index * 0x100;
        self.word(BASE + table - 16, 0);
        self.word(BASE + table - 8, info);
        self.zero(info, 16);
        self.word(info + 8, text);
        self.zero(text, 128);
        self.put(text, name.as_bytes());
    }
    fn position(&mut self, p: u64) {
        self.zero(p, 0x160);
        self.word(p, BASE + 0x43484f0);
        self.unit(p);
    }
    fn unit(&mut self, p: u64) {
        for off in [0x80, 0x94, 0x114] {
            self.put(p + off, &1.0f32.to_le_bytes());
        }
        self.put(p + 0x144, &[1]);
        self.put(p + 0x128, &[1]);
    }
    fn new(root: u64) -> Self {
        let mut f = Self {
            data: Default::default(),
            reads: Default::default(),
            torn: None,
            visits: Cell::new(0),
        };
        for (a, n) in [
            (REG, 0xc0),
            (GRAPH, 0x500),
            (WINDOW, 0xf00),
            (BUTTON, 0x120),
            (RENDER, 0x180),
            (RESOURCE, 0x200),
        ] {
            f.zero(a, n);
        }
        for (a, t) in [
            (REG, 0x42736f8),
            (GRAPH, 0x42417f8),
            (WINDOW, 0x4347d58),
            (BUTTON, 0x43470c8),
            (RENDER, 0x4356e08),
            (RESOURCE, 0x4357160),
        ] {
            f.word(a, BASE + t);
        }
        for (t, i, name) in [
            (0x4356e08, 0, "7CSprite"),
            (0x4357160, 1, "11CSpriteType"),
            (0x43484f0, 2, "11C2dPosition"),
            (0x43593d8, 3, "11CTextSprite"),
        ] {
            f.metadata(t, i, name);
        }
        f.word(BASE + 0x4577890, GRAPH);
        f.word(REG + 0xb8, GRAPH);
        f.position(GRAPH + 0x1f0);
        f.position(GRAPH + 0x350);
        f.position(P0);
        f.position(P1);
        f.word(WINDOW + 0x98, REG);
        f.word(WINDOW + 0xe70, P0);
        f.word(WINDOW + 0xe78, P1);
        f.word(BUTTON + 0x68, REG);
        f.word(BUTTON + 0xe8, RENDER);
        f.word(RENDER + 0x138, BUTTON);
        f.word(RENDER + 0x168, RESOURCE);
        f.unit(RENDER);
        f.number(RENDER + 0x120, 512);
        f.number(RENDER + 0x124, 288);
        f.number(RENDER + 0x150, 512);
        f.number(RENDER + 0x154, 288);
        f.number(RESOURCE + 0x90, 1);
        f.number(RESOURCE + 0x94, 1);
        f.number(RESOURCE + 0xa4, 72);
        f.number(RESOURCE + 0xa8, 72);
        f.word(RESOURCE + 0x170, 0xdead0000);
        f.word(RENDER + 0x108, P1);
        f.word(P1 + 0x108, P0);
        f.word(P0 + 0x108, root);
        f.array(P1 + 0xc0, 0xa0000, RENDER);
        f.array(P0 + 0xc0, 0xa0010, P1);
        f.array(root + 0xc0, 0xa0020, P0);
        f
    }
    fn observe(&self) -> Result<Value> {
        let witnesses = Witnesses::new(self);
        let reader = Reader::new(&witnesses);
        let mut reuse = Reuse::new(BASE, 0);
        let result = super::observe(
            &reader,
            &mut reuse,
            WINDOW,
            BUTTON,
            REG,
            BASE + 0x4356e08,
            RENDER,
        )?;
        witnesses.finish()?;
        Ok(result)
    }
    fn untouched(&self, a: u64, n: usize) -> bool {
        self.reads
            .borrow()
            .iter()
            .all(|(p, s)| *p + *s as u64 <= a || *p >= a + n as u64)
    }
}
impl ReadMemory for Fixture {
    fn read(&self, a: u64, n: usize) -> Result<Vec<u8>> {
        self.reads.borrow_mut().push((a, n));
        let mut bytes = (0..n)
            .map(|i| {
                self.data
                    .get(&(a + i as u64))
                    .copied()
                    .ok_or_else(|| Error::new("fixture_unmapped", "Unexpected generated-map read"))
            })
            .collect::<Result<Vec<_>>>()?;
        if self.torn.is_some_and(|p| p >= a && p < a + n as u64) {
            let v = self.visits.get();
            self.visits.set(v + 1);
            if v > 0 {
                bytes[0] ^= 1;
            }
        }
        Ok(bytes)
    }
}
impl crate::platform::Process for Fixture {
    fn image_base(&self) -> u64 {
        BASE
    }
    fn check(&self) -> Result<()> {
        Ok(())
    }
}
#[test]
fn source_to_consumer_preserves_both_owned_scopes_but_only_1f0_prepares_coordinates() {
    for root in [GRAPH + 0x1f0, GRAPH + 0x350] {
        let f = Fixture::new(root);
        let g = f.observe().unwrap();
        let h = &g["parent_chain"];
        assert_eq!(h["graphics_root_closed"], true);
        assert_eq!(h["status"], "closed_to_owned_graphics_root");
        assert_eq!(h["observed_ancestors"], 3);
        assert_eq!(
            h["unit_cache_diagnostic"]["cache_consistent_under_observed_unit_branch"],
            true
        );
        let surface = crate::research_candidate_observation::control_surface(&g);
        if root == GRAPH + 0x1f0 {
            assert!(surface.is_ok());
        } else {
            let e = surface.unwrap_err();
            assert!(e.message.contains("root1f0 coordinate scope required"));
            assert!(e.message.contains("root_scope=owned_graphics_root_350"));
        }
        assert!(f.untouched(0xdead0000, 8));
    }
}
#[test]
fn detached_and_unknown_ancestor_produce_explicit_refusal_without_foreign_payload() {
    for unknown in [false, true] {
        let mut f = Fixture::new(GRAPH + 0x1f0);
        if unknown {
            f.word(RENDER + 0x108, 0x90000);
            f.word(0x90000, BASE + 0x43593d8);
        } else {
            f.word(RENDER + 0x108, 0);
        }
        let g = f.observe().unwrap();
        let status = if unknown {
            "unknown_ancestor"
        } else {
            "detached_ancestor"
        };
        assert_eq!(g["parent_chain"]["status"], status);
        let e = crate::research_candidate_observation::control_surface(&g).unwrap_err();
        assert_eq!(e.code, "research_geometry_unavailable");
        assert!(e.message.contains(status));
        if unknown {
            assert!(e.message.contains("last_type=11CTextSprite"));
            assert!(f.untouched(0x90008, 0x158));
        }
    }
}
#[test]
fn owned_root_capacity_policy_remains_unread_and_diagnostic_instead_of_expanded() {
    let root = GRAPH + 0x1f0;
    let mut f = Fixture::new(root);
    f.number(root + 0xc8, 8193);
    f.word(root + 0xc0, 0xbeef0000);
    let g = f.observe().unwrap();
    let e = crate::research_candidate_observation::control_surface(&g).unwrap_err();
    for s in [
        "unavailable_policy_bound",
        "scope=exact_owned_graphics_root",
        "capacity=8193",
        "count=1",
    ] {
        assert!(e.message.contains(s), "{s}");
    }
    assert!(f.untouched(0xbeef0000, 8));
}
#[test]
fn depth_boundary_still_refuses_without_inspecting_a_seventeenth_parent() {
    let root = GRAPH + 0x1f0;
    let mut f = Fixture::new(root);
    f.word(RENDER + 0x108, 0xc0000);
    for i in 0..16u64 {
        let p = 0xc0000 + i * 0x2000;
        f.position(p);
        f.word(p + 0x108, if i == 15 { root } else { p + 0x2000 });
        f.array(
            p + 0xc0,
            0x100000 + i * 0x10,
            if i == 0 { RENDER } else { p - 0x2000 },
        );
    }
    let g = f.observe().unwrap();
    let e = crate::research_candidate_observation::control_surface(&g).unwrap_err();
    assert!(e.message.contains("status=depth_limit"));
    assert!(e.message.contains("ancestors=16"));
    assert!(f.untouched(root + 0xc0, 16));
}
#[test]
fn rooted_graph_parent_and_membership_mutations_still_discard_the_whole_observation() {
    for a in [
        RENDER + 0x108,
        P1 + 0x108,
        P0 + 0x108,
        GRAPH + 0x1f0 + 0xc0,
        0xa0020,
    ] {
        let mut f = Fixture::new(GRAPH + 0x1f0);
        f.torn = Some(a);
        assert_eq!(f.observe().unwrap_err().code, "torn_read", "{a:x}");
    }
}
