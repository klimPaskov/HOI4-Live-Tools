#![allow(dead_code)]
use crate::{
    error::{Error, Result},
    memory::ReadMemory,
    platform::Process,
};
use std::{
    cell::{Cell, RefCell},
    collections::BTreeMap,
};
struct Fixture {
    tracked_reads: RefCell<BTreeMap<u64, usize>>,
    data: BTreeMap<u64, u8>,
    checks: Cell<u32>,
    replaced: bool,
    id_reads: Cell<u32>,
    changed_id: bool,
    fault: Option<(u64, usize, Vec<u8>)>,
    fault_reads: Cell<usize>,
}
impl Fixture {
    fn put(&mut self, a: u64, data: &[u8]) {
        for (i, v) in data.iter().enumerate() {
            self.data.insert(a + i as u64, *v);
        }
    }
    fn region(&mut self, a: u64, n: usize) {
        self.put(a, &vec![0; n]);
    }
    fn word(&mut self, a: u64, n: u64) {
        self.put(a, &n.to_le_bytes());
    }
    fn number(&mut self, a: u64, n: i32) {
        self.put(a, &n.to_le_bytes());
    }
    fn string(&mut self, a: u64, text: &str) {
        assert!(text.len() <= 15);
        self.region(a, 32);
        self.word(a, a + 16);
        self.word(a + 8, text.len() as u64);
        self.put(a + 16, text.as_bytes());
    }
    fn object(&mut self, a: u64, name: &str, vt: u64) {
        self.region(vt - 8, 32);
        self.region(vt + 32, 128);
        self.word(a, vt);
        self.word(vt - 8, vt + 16);
        self.word(vt + 24, vt + 32);
        self.put(vt + 32, format!("{}{name}\0", name.len()).as_bytes());
    }
    fn array(&mut self, a: u64, data: u64, count: i32) {
        self.region(a, 16);
        self.word(a, data);
        self.number(a + 8, count);
        self.number(a + 12, count);
    }
    fn new() -> Self {
        let mut f = Self {
            tracked_reads: RefCell::new(BTreeMap::new()),
            data: BTreeMap::new(),
            checks: Cell::new(0),
            replaced: false,
            id_reads: Cell::new(0),
            changed_id: false,
            fault: None,
            fault_reads: Cell::new(0),
        };
        let base = 0x10000;
        f.word(base + 0x44086a8, 0x30000);
        f.word(base + 0x44085f8, 0x32000);
        f.word(base + 0x439f450, 0x36000);
        f.word(base + 0x43f3eb8, 0x37000);
        for (a, n) in [
            (0x30000, 0x700),
            (0x32000, 0x700),
            (0x20000, 0x1400),
            (0x28000, 0x200),
            (0x2a000, 0x50),
        ] {
            f.region(a, n);
        }
        f.word(0x30000, base + 0x42c7598);
        f.word(0x32000, base + 0x42c9a88);
        f.number(0x30460, 60759371);
        f.number(0x304b4, 4);
        f.number(0x30518, 1);
        f.number(0x30620, 2);
        f.number(0x30628, 1);
        let id = b"aabbccdd-1122-4344-8566-778899aabbcc";
        f.region(0x30718, 32);
        f.word(0x30718, 0x60000);
        f.word(0x30720, id.len() as u64);
        f.put(0x60000, id);
        f.put(0x326c1, &[1]);
        f.array(0x30340, 0x39000, 3);
        f.region(0x39000, 12);
        f.number(0x39004, 2);
        f.array(0x30310, 0x3a000, 3);
        f.region(0x3a000, 24);
        f.word(0x3a000, 0xdead0000);
        f.word(0x3a008, 0xbeef0000);
        f.word(0x3a010, 0x20000);
        f.array(0x37000, 0x38000, 2);
        f.string(0x38000, "---");
        f.string(0x38020, "GER");
        f.string(0x20010, "German Reich");
        f.object(0x20000, "CCountry", 0x50000);
        f.word(0x21348, 0x2a000);
        f.object(0x2a000, "CNationalFocusProgress", 0x50100);
        f.word(0x20f38, 0x28000);
        f.object(0x28000, "CTechnologyStatus", 0x50200);
        f.word(0x28130, 0x20000);
        f.number(0x28138, 1);
        f.array(0x280a0, 0x3b000, 4);
        f.region(0x3b000, 32);
        for i in 0..4 {
            let a = 0x2c000 + i * 0x100;
            f.region(a, 64);
            f.object(a, "CResearchSlot", 0x50300);
            f.word(a + 0x10, 0x28000);
            f.word(0x3b000 + i * 8, a);
        }
        f
    }
}
impl ReadMemory for Fixture {
    fn read(&self, a: u64, n: usize) -> Result<Vec<u8>> {
        *self.tracked_reads.borrow_mut().entry(a).or_default() += 1;
        if let Some((address, after, replacement)) = &self.fault {
            if a == *address {
                self.fault_reads.set(self.fault_reads.get() + 1);
                if self.fault_reads.get() > *after {
                    assert_eq!(replacement.len(), n);
                    return Ok(replacement.clone());
                }
            }
        }
        if a == 0x60000 {
            self.id_reads.set(self.id_reads.get() + 1);
            if self.changed_id && self.id_reads.get() > 1 {
                return Ok(b"aabbccdd-1122-4344-8566-778899aabbcd".to_vec());
            }
        }
        (0..n)
            .map(|i| {
                self.data.get(&(a + i as u64)).copied().ok_or_else(|| {
                    Error::new("unexpected_read", format!("Fixture forbids reading {a:x}"))
                })
            })
            .collect()
    }
}
impl Process for Fixture {
    fn image_base(&self) -> u64 {
        0x10000
    }
    fn check(&self) -> Result<()> {
        self.checks.set(self.checks.get() + 1);
        if self.replaced && self.checks.get() > 1 {
            Err(Error::new("ownership_changed", "Fixture replacement"))
        } else {
            Ok(())
        }
    }
}
impl Fixture {
    fn economy(&mut self) {
        self.object(0x20000, "CCountry", 0x10000 + 0x41fafb0);
        self.number(0x20008, 1);
        self.region(0x40000, 0x400);
        self.region(0x41000, 0x100);
        self.word(0x20f40, 0x40000);
        self.word(0x20f68, 0x41000);
        self.word(0x40000, 0x10000 + 0x42066f8);
        self.word(0x40290, 0x20000);
        self.word(0x41000, 0x10000 + 0x4202e30);
        self.word(0x410c8, 0x20000);
        for (off, raw) in [
            (0x2b8, 1_099_999i64),
            (0x318, 599_999),
            (0x378, 2_099_999),
            (0x390, 350_000),
        ] {
            self.put(0x40000 + off, &raw.to_le_bytes());
        }
        for (off, n) in [(0x2f8, 1), (0x300, 2), (0x3b8, 3), (0x3c0, 4), (0x3b0, 5)] {
            self.number(0x40000 + off, n);
        }
        self.put(0x410e0, &(-150_001i64).to_le_bytes());
    }
}

const BASE: u64 = 0x10000;
const DB: u64 = 0xc0000;
const LIST: u64 = 0xc1000;
const TEMPLATE: u64 = 0xc2000;
const OTHER: u64 = 0xc3000;
const SENTINEL: u64 = 0xc4000;

fn fixture() -> Fixture {
    let mut f = Fixture::new();
    f.economy();
    f.array(0x40070, 0x70000, 3);
    f.word(BASE + 0x439f400, DB);
    f.word(DB, BASE + 0x41b3ed8);
    f.array(DB + 0x40, LIST, 3);
    for (index, template) in [SENTINEL, TEMPLATE, OTHER].iter().enumerate() {
        f.word(LIST + index as u64 * 8, *template);
    }
    for (template, key) in [(TEMPLATE, "infrastructure"), (OTHER, "arms_factory")] {
        f.word(template, BASE + 0x41b49c8);
        f.put(template + 0x18, &[1]);
        f.string(template + 0x248, key);
        // Neither intern nor localized name is needed for this direct key join.
    }
    for (i, line, table, handle) in [
        (0, 0x80000, 0x4205680, 0xffff00010000003bu64),
        (1, 0x81000, 0x42ee398, 0xf234567812345678),
        (2, 0x82000, 0x4205680, 0x123400020000003b),
    ] {
        f.word(0x70000 + i * 8, line);
        f.word(line, BASE + table);
        f.word(line + 0x20, 0x40000);
        f.word(line + 8, handle);
        f.number(line + 0x14, 12);
        f.number(line + 0x18, 2);
        f.word(line + 0x28, 100_000_000);
        f.word(line + 0x30, 750_000);
        f.word(line + 0x38, 25_000_000);
        f.number(line + 0x40, 15);
        f.number(line + 0x44, i as i32);
        f.number(line + 0x48, 3);
    }
    for (line, building, template) in [(0x80000, 0xd0000, TEMPLATE), (0x82000, 0xd1000, OTHER)] {
        f.word(line + 0x70, building);
        f.number(line + 0x78, 1);
        f.put(line + 0x8c, &[1]);
        f.put(line + 0x99, &[1]);
        f.word(building, BASE + 0x41b3e60);
        f.word(building + 0x1c0, template);
        // No location/foreign state or railway payload bytes are readable.
    }
    f
}

const REG: u64 = 0x200000;
const MAN: u64 = 0x210000;
const PANEL: u64 = 0x220000;
const TOP: u64 = 0x230000;
const ROOT: u64 = 0x240000;
const MENU: u64 = 0x250000;
const FACTION: u64 = 0x260000;
const INTEL: u64 = 0x270000;
const CW: u64 = 0x280000;
const OPEN: u64 = 0x290000;
const CHOOSE: u64 = 0x2a0000;
const ITEM: u64 = 0x2b0000;
const ITEMS: u64 = 0x2c0000;
const FOCUS: u64 = 0x2d0000;
const POLITICS: u64 = 0x2e0000;
const FROOT: u64 = 0x2f0000;
const PROOT: u64 = 0x300000;
const RESEARCH: u64 = 0x310000;
const TREE: u64 = 0x320000;
const RROOT: u64 = 0x330000;
const TROOT: u64 = 0x340000;
const CONSTRUCTION_ROOT: u64 = 0x350000;
const GRAPHICS: u64 = 0x360000;
fn text(f: &mut Fixture, address: u64, value: &str) {
    if value.len() <= 15 {
        f.string(address, value);
        return;
    }
    f.region(address, 32);
    f.word(address, address + 0x10000000);
    f.word(address + 8, value.len() as u64);
    f.put(address + 0x10000000, value.as_bytes());
}
fn named(f: &mut Fixture, window: u64, children: bool, name: &str, value: u64) {
    let (a, b) = if children {
        (0xbb0, 0xbc8)
    } else {
        (0x9a0, 0x9b8)
    };
    f.array(window + a, window + 0x3000, 1);
    f.word(window + 0x3000, value);
    f.array(window + b, window + 0x4000, 1);
    text(f, window + 0x4000, name);
}
fn window(f: &mut Fixture, p: u64, shown: bool) {
    f.region(p, 0xf00);
    f.word(p, BASE + 0x4347d58);
    f.word(p + 0x98, REG);
    f.put(p + 0xa5, &[if shown { 8 } else { 0 }]);
}
fn control(f: &mut Fixture, button: u64, owner: u64, choice: bool) {
    f.region(button, 0x208);
    f.word(button, BASE + 0x43470c8);
    f.word(button + 0x68, REG);
    f.number(button + 0x74, 0x800);
    f.number(button + 0xac, 2);
    f.word(button + 0x80, BASE + 0x4347488);
    let link = button + 0x3000;
    f.word(button + 0x88, link);
    f.word(button + 0x90, link);
    f.number(button + 0x98, 1);
    let (off, primary, manager, invoker, member) = if choice {
        (0x28, 0x43104c0, 0x297df40, 0x297df10, 0x296f810)
    } else {
        (0x21f0, 0x426c518, 0x1886560, 0x1886530, 0x186d470)
    };
    let glue = owner + off;
    f.region(glue, 0x288);
    f.word(glue, BASE + primary);
    f.word(glue + 0x18, BASE + manager);
    f.word(glue + 0x20, BASE + invoker);
    let capture = button + 0x4000;
    f.region(capture, 24);
    f.word(capture, BASE + member);
    f.word(capture + 0x10, owner);
    f.word(glue + 8, capture);
    f.region(link, 32);
    f.word(link, glue);
    // DC is one byte; never authorize reads of its unproved trailing bytes.
    for a in button + 0xdd..button + 0xe0 {
        f.data.remove(&a);
    }
}
fn controls_fixture() -> Fixture {
    let mut f = fixture();
    f.region(REG, 0x240);
    f.word(REG, BASE + 0x42736f8);
    f.word(BASE + 0x45778e0, REG);
    f.word(0x324f8, REG);
    f.region(MAN, 0x500);
    f.word(0x326b8, MAN);
    f.word(MAN + 0x180, 0x32000);
    f.number(MAN + 0x3d8, 17);
    f.word(MAN + 0x150, PANEL);
    f.word(MAN + 0x188, TOP);
    f.region(PANEL, 0x340);
    f.word(PANEL, BASE + 0x43101d8);
    f.word(PANEL + 0x2f0, 0x32000);
    f.word(PANEL + 0x2f8, CONSTRUCTION_ROOT);
    f.region(TOP, 0x5000);
    f.word(TOP, BASE + 0x426c3d0);
    f.word(TOP + 0x68, 0x32000);
    f.word(TOP + 0x70, MAN);
    f.word(TOP + 0x78, ROOT);
    f.word(TOP + 0x98, MENU);
    f.word(TOP + 0x4db8, OPEN);
    for p in [ROOT, MENU, FACTION, INTEL, CW, CONSTRUCTION_ROOT] {
        window(&mut f, p, true);
    }
    for p in [FROOT, PROOT, RROOT, TROOT] {
        window(&mut f, p, false);
    }
    for (parent, name, child) in [
        (ROOT, "country_menu_window", MENU),
        (MENU, "after_faction_button", FACTION),
        (FACTION, "after_intel_agency_button", INTEL),
    ] {
        named(&mut f, parent, true, name, child);
    }
    named(&mut f, INTEL, false, "construction_button", OPEN);
    f.array(PANEL + 0x320, ITEMS, 1);
    f.word(ITEMS, ITEM);
    f.region(ITEM, 0x300);
    f.word(ITEM, BASE + 0x43100f8);
    f.word(ITEM + 0x20, TEMPLATE);
    f.word(ITEM + 0x10, CW);
    f.number(TEMPLATE + 0x2b8, 1);
    named(&mut f, CW, false, "start_construction_button", CHOOSE);
    control(&mut f, OPEN, TOP, false);
    control(&mut f, CHOOSE, ITEM, true);
    f.word(MAN + 0x128, POLITICS);
    f.word(MAN + 0x130, FOCUS);
    f.region(FOCUS, 0x1200);
    f.word(FOCUS, BASE + 0x4249c70);
    f.word(FOCUS + 0x2f0, 0x32000);
    f.word(FOCUS + 0x2f8, FROOT);
    f.word(FOCUS + 0x11d8, 0x20000);
    f.region(POLITICS, 0x300);
    f.word(POLITICS, BASE + 0x42546e8);
    f.word(POLITICS + 0x2f0, 0x32000);
    f.word(POLITICS + 0x38, MAN);
    f.word(POLITICS + 0x2f8, PROOT);
    f.word(MAN + 0xf0, RESEARCH);
    f.region(RESEARCH, 0x300);
    f.word(RESEARCH, BASE + 0x424dc38);
    f.word(RESEARCH + 0x2f0, 0x32000);
    f.word(RESEARCH + 0x2f8, RROOT);
    f.word(MAN + 0xf8, TREE);
    f.region(TREE, 0xe28);
    f.word(TREE, BASE + 0x424de40);
    f.word(TREE + 0x2f0, 0x32000);
    f.word(TREE + 0x2f8, TROOT);
    f.number(TREE + 0xe20, 1);
    f.number(TREE + 0xe24, 6);
    f.put(0x30a59, &[1]);
    f.put(0x328f0, &[0]);
    f.word(0x327b0, 0);
    f
}

fn geometry_fixture() -> Fixture {
    let mut f = controls_fixture();
    f.region(GRAPHICS, 0x200);
    f.word(GRAPHICS, BASE + 0x42417f8);
    f.word(BASE + 0x4577890, GRAPHICS);
    f.word(REG + 0xb8, GRAPHICS);
    for (i, n) in [512, 288, 2560, 1440].into_iter().enumerate() {
        f.number(GRAPHICS + 0x178 + i as u64 * 4, n);
    }
    for (i, n) in [5.0f32, 5.0, 0.2, 0.2].into_iter().enumerate() {
        f.put(GRAPHICS + 0x1d0 + i as u64 * 4, &n.to_le_bytes());
    }
    f
}

fn metadata(f: &mut Fixture, object: u64, primary: u64, name: &str) {
    let primary = BASE + primary;
    let info = BASE + 0x435a800;
    let string = BASE + 0x3516000;
    f.word(object, primary);
    f.word(primary - 16, 0);
    f.word(primary - 8, info);
    f.word(info + 8, string);
    f.region(string, 128);
    f.put(string, name.as_bytes());
}

fn production_not_read(f: &Fixture, addresses: &[u64]) {
    for a in addresses {
        assert!(
            !f.tracked_reads.borrow().contains_key(a),
            "Forbidden payload address"
        );
    }
}

use super::*;
use crate::construction::controls::{observe as observed, snapshot};

fn normalized(mut value: serde_json::Value) -> serde_json::Value {
    let freshness = value["freshness"].as_object_mut().unwrap();
    freshness.remove("elapsed_ms");
    freshness.remove("observed_unix_ms");
    value
}
fn scalar_value(mut value: serde_json::Value) -> serde_json::Value {
    value.as_object_mut().unwrap().remove("freshness");
    value
}
fn copy_region(f: &mut Fixture, old: u64, new: u64, count: u64) {
    let bytes: Vec<_> = (0..count)
        .filter_map(|i| f.data.get(&(old + i)).copied().map(|b| (i, b)))
        .collect();
    for (i, b) in bytes {
        f.put(new + i, &[b]);
    }
}
fn typed(f: &mut Fixture, object: u64, primary: u64, name: &str, slot: u64) {
    let table = BASE + primary;
    let info = BASE + 0x435a800 + slot * 0x40;
    let string = BASE + 0x3516000 + slot * 0x100;
    f.word(object, table);
    f.word(table - 16, 0);
    f.word(table - 8, info);
    f.word(info + 8, string);
    f.region(string, 128);
    f.put(string, format!("{}{name}\0", name.len()).as_bytes());
}
const RENDER: u64 = 0x400000;
const RESOURCE: u64 = 0x410000;
const POS_A: u64 = 0x420000;
const POS_B: u64 = 0x421000;
const CHOICE_RENDER: u64 = 0x430000;
const CHOICE_RESOURCE: u64 = 0x440000;
const CHOICE_A: u64 = 0x450000;
const CHOICE_B: u64 = 0x451000;
fn node(f: &mut Fixture, p: u64) {
    f.region(p, 0x200);
    typed(f, p, 0x43484f0, "C2dPositionObject", 2);
    for off in [0x80, 0x94, 0x114] {
        f.put(p + off, &1.0f32.to_le_bytes());
    }
    f.put(p + 0x128, &[1]);
    f.put(p + 0x144, &[1]);
}
fn sprite_fixture() -> Fixture {
    let mut f = geometry_fixture();
    for root in [GRAPHICS + 0x1f0, GRAPHICS + 0x350] {
        node(&mut f, root);
    }
    f.array(GRAPHICS + 0x1f0 + 0xc0, 0x5f0000, 2);
    f.word(0x5f0000, POS_A);
    f.word(0x5f0008, CHOICE_A);
    for (button, window, render, resource, a, b) in [
        (OPEN, INTEL, RENDER, RESOURCE, POS_A, POS_B),
        (
            CHOOSE,
            CW,
            CHOICE_RENDER,
            CHOICE_RESOURCE,
            CHOICE_A,
            CHOICE_B,
        ),
    ] {
        node(&mut f, render);
        typed(&mut f, render, 0x4356e08, "CSprite", 0);
        f.word(button + 0xe8, render);
        f.word(render + 0x138, button);
        f.word(render + 0x168, resource);
        f.region(resource, 0x200);
        typed(&mut f, resource, 0x4357160, "CSpriteType", 1);
        for off in [0x90, 0x94] {
            f.number(resource + off, 1);
        }
        for off in [0xa4, 0xa8] {
            f.number(resource + off, 200);
        }
        f.word(resource + 0x170, 0x99999990); // presence only, no texture payload.
        for p in [a, b] {
            node(&mut f, p);
        }
        f.word(window + 0xe70, a);
        f.word(window + 0xe78, b);
        f.word(button + 0x1a8, b);
        f.word(a + 0x108, GRAPHICS + 0x1f0);
        f.word(b + 0x108, a);
        f.word(render + 0x108, b);
        f.array(a + 0xc0, a + 0x2000, 1);
        f.word(a + 0x2000, b);
        f.array(b + 0xc0, b + 0x2000, 1);
        f.word(b + 0x2000, render);
    }
    f
}

#[test]
fn public_json_and_read_counts_match_batched_registry_golden() {
    // Five one-entry name registries each add a 16-byte array-header recheck.
    // Batching saves reads for larger registries; all other output is unchanged.
    let golden: Vec<serde_json::Value> =
        serde_json::from_str(include_str!("public-baseline.json")).unwrap();
    for (index, geometry) in [false, true].into_iter().enumerate() {
        let f = geometry_fixture();
        let v = snapshot(&f, "infrastructure", geometry).unwrap();
        assert_eq!(normalized(v), golden[index]);
        assert_eq!(f.checks.get(), 2);
    }
}
#[test]
fn scope_and_clock_context_use_completed_same_witness_observation_without_new_reads() {
    let f = controls_fixture();
    let o = observed(&f, "infrastructure", false, None).unwrap();
    let reads = f.tracked_reads.borrow().clone();
    let checks = f.checks.get();
    assert_eq!(o.campaign.player, "GER");
    assert_eq!(o.campaign.date_raw, 60759371);
    assert!(o.scope(Stage::EnsureOpen).is_ok());
    assert!(o.scope(Stage::SelectBuilding).is_ok());
    assert_eq!(*f.tracked_reads.borrow(), reads);
    assert_eq!(f.checks.get(), checks);
}
#[test]
fn requested_geometry_requires_materialized_identity_but_noop_scope_can_omit_geometry() {
    let f = geometry_fixture();
    let o = observed(&f, "infrastructure", true, None).unwrap();
    assert_eq!(
        o.value["opener"]["render_diagnostic"]["reason"],
        "renderer_unmaterialized"
    );
    for stage in [Stage::EnsureOpen, Stage::SelectBuilding] {
        assert_eq!(
            o.scope(stage).unwrap_err().code,
            "construction_scope_unavailable"
        );
    }
    let o = observed(&controls_fixture(), "infrastructure", false, None).unwrap();
    assert!(o.scope(Stage::EnsureOpen).is_ok());
}
#[test]
fn ensure_open_excludes_choice_materialization_and_selection_semantics() {
    let shown = observed(&controls_fixture(), "infrastructure", false, None).unwrap();
    let mut hidden = controls_fixture();
    hidden.put(CONSTRUCTION_ROOT + 0xa5, &[0]);
    hidden.number(MAN + 0x3d8, 23);
    let hidden = observed(&hidden, "infrastructure", false, None).unwrap();
    assert_eq!(
        shown.scope(Stage::EnsureOpen).unwrap(),
        hidden.scope(Stage::EnsureOpen).unwrap()
    );
    assert!(hidden.scope(Stage::SelectBuilding).is_err());
    let mut selected = controls_fixture();
    selected.number(MAN + 0x3f0, 1);
    selected.word(MAN + 0x3e8, TEMPLATE);
    selected.word(DB + 0x3a8, OTHER);
    let selected = observed(&selected, "infrastructure", false, None).unwrap();
    assert_eq!(selected.value["selection"]["building_id"], "infrastructure");
    assert_eq!(
        shown.scope(Stage::EnsureOpen).unwrap(),
        selected.scope(Stage::EnsureOpen).unwrap()
    );
    assert_eq!(
        shown.scope(Stage::SelectBuilding).unwrap(),
        selected.scope(Stage::SelectBuilding).unwrap()
    );
}
#[test]
fn same_looking_callback_capture_and_named_button_replacements_change_only_target_scope() {
    let original = observed(&controls_fixture(), "infrastructure", false, None).unwrap();
    for capture_only in [false, true] {
        let mut f = controls_fixture();
        if capture_only {
            copy_region(&mut f, OPEN + 0x4000, 0x620000, 24);
            f.word(TOP + 0x21f0 + 8, 0x620000);
        } else {
            let replacement = 0x610000;
            copy_region(&mut f, OPEN, replacement, 0x208);
            copy_region(&mut f, OPEN + 0x3000, replacement + 0x3000, 32);
            copy_region(&mut f, OPEN + 0x4000, replacement + 0x4000, 24);
            f.word(replacement + 0x88, replacement + 0x3000);
            f.word(replacement + 0x90, replacement + 0x3000);
            f.word(TOP + 0x21f0 + 8, replacement + 0x4000);
            f.word(INTEL + 0x3000, replacement);
            f.word(TOP + 0x4db8, replacement);
        }
        let changed = observed(&f, "infrastructure", false, None).unwrap();
        assert_eq!(
            scalar_value(original.value.clone()),
            scalar_value(changed.value.clone())
        );
        assert_ne!(
            original.scope(Stage::EnsureOpen).unwrap(),
            changed.scope(Stage::EnsureOpen).unwrap()
        );
        assert_eq!(
            original.scope(Stage::SelectBuilding).unwrap(),
            changed.scope(Stage::SelectBuilding).unwrap()
        );
    }
}
#[test]
fn same_looking_choice_item_and_template_replacements_never_rebind_select_scope() {
    let original = observed(&controls_fixture(), "infrastructure", false, None).unwrap();
    for item_only in [false, true] {
        let mut f = controls_fixture();
        if item_only {
            let replacement = 0x630000;
            copy_region(&mut f, ITEM, replacement, 0x300);
            f.word(ITEMS, replacement);
            f.word(CHOOSE + 0x3000, replacement + 0x28);
            f.word(CHOOSE + 0x4000 + 0x10, replacement);
        } else {
            let replacement = 0x640000;
            copy_region(&mut f, TEMPLATE, replacement, 0x300);
            f.string(replacement + 0x248, "infrastructure");
            f.word(LIST + 8, replacement);
            f.word(ITEM + 0x20, replacement);
        }
        let changed = observed(&f, "infrastructure", false, None).unwrap();
        assert_eq!(
            scalar_value(original.value.clone()),
            scalar_value(changed.value.clone())
        );
        assert_eq!(
            original.scope(Stage::EnsureOpen).unwrap(),
            changed.scope(Stage::EnsureOpen).unwrap()
        );
        assert_ne!(
            original.scope(Stage::SelectBuilding).unwrap(),
            changed.scope(Stage::SelectBuilding).unwrap()
        );
    }
}
#[test]
fn essential_same_looking_owned_blocker_root_replacement_changes_both_scopes() {
    let original = observed(&controls_fixture(), "infrastructure", false, None).unwrap();
    let mut f = controls_fixture();
    copy_region(&mut f, FROOT, 0x650000, 0xf00);
    f.word(FOCUS + 0x2f8, 0x650000);
    let changed = observed(&f, "infrastructure", false, None).unwrap();
    assert_eq!(
        scalar_value(original.value.clone()),
        scalar_value(changed.value.clone())
    );
    for stage in [Stage::EnsureOpen, Stage::SelectBuilding] {
        assert_ne!(
            original.scope(stage).unwrap(),
            changed.scope(stage).unwrap()
        );
    }
}
#[test]
fn exact_sprite_resource_and_parent_identities_are_pinned_not_geometry_numbers() {
    let original = observed(&sprite_fixture(), "infrastructure", true, None).unwrap();
    assert_eq!(
        original.value["opener"]["renderer_type_encoding"],
        "7CSprite"
    );
    assert!(original.scope(Stage::EnsureOpen).is_ok());
    assert!(original.scope(Stage::SelectBuilding).is_ok());
    for variant in 0..3 {
        let mut f = sprite_fixture();
        match variant {
            0 => {
                copy_region(&mut f, RESOURCE, 0x660000, 0x200);
                f.word(RENDER + 0x168, 0x660000);
            }
            1 => {
                copy_region(&mut f, POS_B, 0x670000, 0x200);
                f.word(INTEL + 0xe78, 0x670000);
                f.word(OPEN + 0x1a8, 0x670000);
                f.word(RENDER + 0x108, 0x670000);
                f.word(POS_A + 0x2000, 0x670000);
            }
            _ => {
                copy_region(&mut f, RENDER, 0x680000, 0x200);
                f.word(OPEN + 0xe8, 0x680000);
                f.word(POS_B + 0x2000, 0x680000);
            }
        }
        let changed = observed(&f, "infrastructure", true, None).unwrap();
        assert_eq!(
            scalar_value(original.value.clone()),
            scalar_value(changed.value.clone())
        );
        assert_ne!(
            original.scope(Stage::EnsureOpen).unwrap(),
            changed.scope(Stage::EnsureOpen).unwrap()
        );
        assert_eq!(
            original.scope(Stage::SelectBuilding).unwrap(),
            changed.scope(Stage::SelectBuilding).unwrap()
        );
    }
    let mut f = sprite_fixture();
    f.number(RESOURCE + 0xa4, 240);
    f.put(RENDER + 0xb0, &2f32.to_le_bytes());
    let changed = observed(&f, "infrastructure", true, None).unwrap();
    assert_eq!(
        original.scope(Stage::EnsureOpen).unwrap(),
        changed.scope(Stage::EnsureOpen).unwrap()
    );
    assert_ne!(
        original.value["opener"]["render_diagnostic"],
        changed.value["opener"]["render_diagnostic"]
    );
}
#[test]
fn missing_or_foreign_identities_leave_public_unavailable_and_refuse_action_scope() {
    let mut f = controls_fixture();
    f.word(FOCUS + 0x11d8, 0xdead0000);
    let o = observed(&f, "infrastructure", false, None).unwrap();
    assert_eq!(o.value["scoped_blockers"]["available"], false);
    assert!(o.scope(Stage::EnsureOpen).is_err());
    production_not_read(&f, &[FROOT, FROOT + 0xa5]);
    let mut f = controls_fixture();
    f.word(ITEM, BASE + 0x43100f9);
    let o = observed(&f, "infrastructure", false, None).unwrap();
    assert_eq!(o.value["choice"]["complete_choice_coverage"], false);
    assert!(o.scope(Stage::EnsureOpen).is_ok());
    assert!(o.scope(Stage::SelectBuilding).is_err());
    production_not_read(&f, &[ITEM + 0x20, ITEM + 0x10]);
}
#[test]
fn scope_identities_and_nonidentity_geometry_bytes_still_fail_final_witness_drift() {
    let baseline = sprite_fixture();
    observed(&baseline, "infrastructure", true, None).unwrap();
    for (address, width) in [
        (TOP + 0x21f0 + 8, 8),
        (FOCUS + 0x2f8, 8),
        (ITEM + 0x20, 8),
        (RENDER + 0x168, 8),
        (RENDER + 0x108, 8),
        (RESOURCE + 0xa4, 4),
    ] {
        let mut bytes = (0..width)
            .map(|i| baseline.data[&(address + i)])
            .collect::<Vec<_>>();
        bytes[0] ^= 1;
        let mut f = sprite_fixture();
        f.fault = Some((
            address,
            baseline.tracked_reads.borrow()[&address] - 1,
            bytes,
        ));
        assert_eq!(
            observed(&f, "infrastructure", true, None)
                .err()
                .unwrap()
                .code,
            "torn_read"
        );
    }
}

#[test]
fn raw_overflowing_follows_are_forwarded_without_reads_or_unbounded_registration() {
    use super::{Follow, Recorder};
    struct Raw {
        value: u64,
        calls: Cell<usize>,
    }
    impl ReadMemory for Raw {
        fn read(&self, address: u64, size: usize) -> Result<Vec<u8>> {
            assert_eq!((address, size), (0x10000, 8));
            self.calls.set(self.calls.get() + 1);
            Ok(self.value.to_le_bytes().to_vec())
        }
    }
    for follow in [
        Follow::Parent(0),
        Follow::Owner,
        Follow::Graphics,
        Follow::Primary,
    ] {
        for word in [
            u64::MAX,
            u64::MAX - 0x108,
            u64::MAX - 0x2f8,
            u64::MAX - 0x350,
            0x8000_0000_0000,
            0,
            1,
        ] {
            let raw = Raw {
                value: word,
                calls: Cell::new(0),
            };
            let trace = Recorder::new(&raw);
            trace.watch(0x10000, follow);
            let reader = crate::memory::Reader::new(&trace);
            assert_eq!(reader.u64(0x10000).unwrap(), word);
            assert_eq!(raw.calls.get(), 1);
            assert_eq!(trace.finish().word(0x10000).unwrap(), word);
            assert!(trace
                .sites
                .borrow()
                .keys()
                .all(|a| (0x10000..=0x7fff_ffff_0000).contains(a)));
            assert_eq!(reader.u64(word).unwrap_err().code, "read_bound");
            assert_eq!(
                raw.calls.get(),
                1,
                "Reader refuses raw pointer before backend"
            );
        }
    }
}

#[test]
fn follow_registration_preserves_backend_errors_and_reader_upper_boundary() {
    use super::{Follow, Recorder};
    struct Refusal(Cell<usize>);
    impl ReadMemory for Refusal {
        fn read(&self, _: u64, _: usize) -> Result<Vec<u8>> {
            self.0.set(self.0.get() + 1);
            Err(Error::new("fixture_refusal", "Original backend refusal"))
        }
    }
    let raw = Refusal(Cell::new(0));
    let trace = Recorder::new(&raw);
    trace.watch(0x10000, Follow::Graphics);
    let e = trace.read(0x10000, 8).unwrap_err();
    assert_eq!(e.code, "fixture_refusal");
    assert_eq!(e.message, "Original backend refusal");
    assert_eq!(raw.0.get(), 1);
    assert!(trace.finish().0.is_empty());
    trace.node(0x7fff_ffff_0000, 0);
    trace.owner(u64::MAX);
    trace.control(u64::MAX, u64::MAX, u64::MAX);
    trace.blockers(u64::MAX);
    assert!(trace
        .sites
        .borrow()
        .keys()
        .all(|a| (0x10000..=0x7fff_ffff_0000).contains(a)));
    assert!(!trace
        .sites
        .borrow()
        .contains_key(&(0x7fff_ffff_0000 + 0x108)));
    assert_eq!(raw.0.get(), 1, "Registration adds no process reads");
}
