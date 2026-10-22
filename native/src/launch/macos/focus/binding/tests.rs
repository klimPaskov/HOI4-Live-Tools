use super::*;
use crate::memory::ReadMemory;
use std::cell::{Cell, RefCell};

const KEY: &str = "GER_rhineland";
const GS: u64 = 0x100000;
const IDLER: u64 = 0x110000;
const COUNTRY: u64 = 0x120000;
const OWN_TREE: u64 = 0x130000;
const DATABASE: u64 = 0x140000;
const PROGRESS: u64 = 0x150000;
const TARGET: u64 = 0x160000;
const OTHER: u64 = 0x170000;
const REPLACEMENT: u64 = 0x180000;
const FOREIGN: u64 = 0x190000;
const MANAGER: u64 = 0x200000;
const FOCUS_VIEW: u64 = 0x210000;
const POLITICS_VIEW: u64 = 0x220000;
const FOCUS_ROOT: u64 = 0x230000;
const POLITICS_ROOT: u64 = 0x240000;
const GOAL: u64 = 0x250000;
const ITEM: u64 = 0x300000;
const SECOND_ITEM: u64 = 0x310000;
const NODE: u64 = 0x320000;
const SYMBOL: u64 = 0x340000;
const BACKGROUND: u64 = 0x350000;
const SECOND_SYMBOL: u64 = 0x360000;
const PANEL: u64 = 0x400000;
const SECOND_PANEL: u64 = 0x410000;
const DETAIL_FRAME: u64 = 0x420000;
const START: u64 = 0x440000;
const SECOND_START: u64 = 0x450000;
const OPEN_BUTTON: u64 = 0x460000;

// Only the independently supported Mac fields below are mapped. Reading an
// omitted Linux field, foreign object, or payload beyond a bound is a failure.
struct Fake {
    data: BTreeMap<u64, u8>,
    reads: RefCell<Vec<(u64, usize)>>,
    checks: Cell<usize>,
    next: u64,
    torn: Option<u64>,
    fail_final: bool,
}
impl Fake {
    fn empty() -> Self {
        Self {
            data: BTreeMap::new(),
            reads: RefCell::new(vec![]),
            checks: Cell::new(0),
            next: 0x600000,
            torn: None,
            fail_final: false,
        }
    }
    fn put(&mut self, address: u64, bytes: &[u8]) {
        for (index, byte) in bytes.iter().enumerate() {
            self.data.insert(address + index as u64, *byte);
        }
    }
    fn word(&mut self, address: u64, value: u64) {
        self.put(address, &value.to_le_bytes());
    }
    fn int(&mut self, address: u64, value: i32) {
        self.put(address, &value.to_le_bytes());
    }
    fn float(&mut self, address: u64, value: f32) {
        self.put(address, &value.to_le_bytes());
    }
    fn alloc(&mut self) -> u64 {
        let address = self.next;
        self.next += 0x1000;
        address
    }
    fn header(&mut self, address: u64, data: u64, capacity: i32, count: i32) {
        self.word(address, data);
        self.int(address + 8, capacity);
        self.int(address + 12, count);
    }
    fn array(&mut self, address: u64, values: &[u64]) -> u64 {
        let data = if values.is_empty() { 0 } else { self.alloc() };
        self.header(address, data, values.len() as i32, values.len() as i32);
        for (index, value) in values.iter().enumerate() {
            self.word(data + index as u64 * 8, *value);
        }
        data
    }
    fn string(&mut self, address: u64, text: &str) {
        let mut bytes = [0; 24];
        if text.len() <= 22 {
            bytes[0] = (text.len() * 2) as u8;
            bytes[1..1 + text.len()].copy_from_slice(text.as_bytes());
        } else {
            let data = self.alloc();
            let capacity = (text.len() as u64 + 2) & !1;
            bytes[..8].copy_from_slice(&(capacity | 1).to_le_bytes());
            bytes[8..16].copy_from_slice(&(text.len() as u64).to_le_bytes());
            bytes[16..].copy_from_slice(&data.to_le_bytes());
            self.put(data, text.as_bytes());
        }
        self.put(address, &bytes);
    }
    fn focus(&mut self, object: u64, key: &str) {
        self.word(object, 0x3386468);
        self.word(object + 0x130, OWN_TREE);
        self.string(object + 0x18, key);
    }
    fn container(&mut self, object: u64, children: &[u64]) {
        self.word(object, 0x34ac0b0);
        self.put(object + 0x95, &[8]);
        self.array(object + 0xd30, children);
    }
    fn name(&mut self, widget: u64, name: &str) {
        let kind = self.alloc();
        self.word(widget + 0x50, kind);
        self.word(kind, 0x34a0000);
        self.string(kind + 0x28, name);
    }
    fn widget(&mut self, object: u64, name: &str, button: bool, rect: Rect) {
        self.word(object, if button { 0x34aafa0 } else { 0x34a81a8 });
        self.name(object, name);
        self.put(object + 0x65, &[8]);
        if button {
            self.int(object + 0x9c, 0);
        }
        let sprite = self.alloc();
        let kind = self.alloc();
        self.word(object + if button { 0xd8 } else { 0xd0 }, sprite);
        self.word(sprite, 0x34bdb10);
        self.word(sprite + 8, kind);
        self.int(kind + 0x78, 1);
        self.int(kind + 0x8c, rect.w);
        self.int(kind + 0x90, rect.h);
        self.float(sprite + 0xb0, (rect.x - 960) as f32);
        self.float(sprite + 0xb4, (540 - rect.y - rect.h) as f32);
    }
    fn get_word(&self, address: u64) -> u64 {
        u64::from_le_bytes(std::array::from_fn(|i| self.data[&(address + i as u64)]))
    }
    fn untouched(&self, address: u64, size: u64) -> bool {
        self.reads
            .borrow()
            .iter()
            .all(|(a, n)| *a + *n as u64 <= address || *a >= address + size)
    }
    fn new() -> Self {
        let mut f = Self::empty();
        for (address, value) in [
            (0x3519740, GS),
            (0x35196c0, IDLER),
            (GS, 0x3306260),
            (IDLER, 0x3308f90),
            (COUNTRY, 0x337bc38),
            (COUNTRY + 0x10f8, OWN_TREE),
            (COUNTRY + 0x1108, PROGRESS),
            (OWN_TREE, 0x33863a8),
            (0x34d0a10, DATABASE),
            (DATABASE, 0x3386528),
            (PROGRESS, 0x33865d0),
            (PROGRESS + 8, COUNTRY),
            (PROGRESS + 0x10, 0),
            (IDLER + 0x670, MANAGER),
            (MANAGER + 0x130, FOCUS_VIEW),
            (MANAGER + 0x128, POLITICS_VIEW),
            (FOCUS_VIEW + 0x430, FOCUS_ROOT),
            (POLITICS_VIEW + 0x430, POLITICS_ROOT),
            (FOCUS_VIEW + 0x1990, COUNTRY),
            (ITEM, 0x3405b78),
            (ITEM + 0x20, TARGET),
            (ITEM + 0x10, NODE),
            (PANEL, 0x3405158),
            (PANEL + 0xba8, TARGET),
            (PANEL + 0xba0, DETAIL_FRAME),
        ] {
            f.word(address, value);
        }
        f.int(GS + 0x4d8, 1);
        f.int(COUNTRY + 8, 1);
        let tags = f.alloc();
        f.header(GS + 0x308, tags, 2, 2);
        f.int(tags, -1);
        f.int(tags + 4, 0);
        f.array(GS + 0x2d8, &[COUNTRY]);
        f.array(OWN_TREE + 0x28, &[TARGET, OTHER]);
        // Duplicate/null database registry entries are not own membership.
        f.array(DATABASE + 0x58, &[OTHER, TARGET, 0, TARGET]);
        f.array(PROGRESS + 0x40, &[]);
        f.focus(TARGET, KEY);
        f.focus(OTHER, "GER_autarky");
        f.float(FOCUS_VIEW + 0x19b0, 1.0);
        f.array(FOCUS_VIEW + 0x18f0, &[ITEM]);
        f.array(FOCUS_VIEW + 0x1938, &[PANEL]);
        f.container(FOCUS_ROOT, &[]);
        f.container(POLITICS_ROOT, &[GOAL + 0x30]);
        f.container(GOAL, &[OPEN_BUTTON]);
        // Containers appear in parent children lists as their GUI-object base.
        f.word(GOAL + 0x30, 0x34ac100);
        f.name(GOAL + 0x30, "active_goal");
        f.container(NODE, &[SYMBOL, BACKGROUND]);
        f.container(DETAIL_FRAME, &[START]);
        f.widget(
            SYMBOL,
            "symbol",
            false,
            Rect {
                x: 900,
                y: 400,
                w: 40,
                h: 40,
            },
        );
        f.widget(
            BACKGROUND,
            "bg",
            false,
            Rect {
                x: 890,
                y: 390,
                w: 60,
                h: 60,
            },
        );
        f.widget(
            START,
            "start",
            true,
            Rect {
                x: 1050,
                y: 700,
                w: 100,
                h: 30,
            },
        );
        f.widget(
            OPEN_BUTTON,
            "add_national_goal_button",
            true,
            Rect {
                x: 80,
                y: 140,
                w: 180,
                h: 40,
            },
        );
        f
    }
}
impl ReadMemory for Fake {
    fn string_abi(&self) -> StringAbi {
        StringAbi::LibCxx
    }
    fn read(&self, address: u64, size: usize) -> Result<Vec<u8>> {
        let repeated = self.reads.borrow().contains(&(address, size));
        self.reads.borrow_mut().push((address, size));
        let mut bytes = (0..size)
            .map(|i| {
                self.data
                    .get(&(address + i as u64))
                    .copied()
                    .ok_or_else(|| {
                        Error::new(
                            "fixture_unmapped",
                            "Unmapped Mac or foreign fixture payload",
                        )
                    })
            })
            .collect::<Result<Vec<_>>>()?;
        if repeated {
            if let Some(at) = self
                .torn
                .filter(|at| (address..address + size as u64).contains(at))
            {
                bytes[(at - address) as usize] ^= 1;
            }
        }
        Ok(bytes)
    }
}
impl Process for Fake {
    fn check(&self) -> Result<()> {
        let checks = self.checks.get() + 1;
        self.checks.set(checks);
        if self.fail_final && checks > 1 {
            return Err(Error::new(
                "ownership_changed",
                "Generated final identity loss",
            ));
        }
        Ok(())
    }
    fn image_base(&self) -> u64 {
        0
    }
}

#[test]
fn canonical_target_and_real_node_geometry_share_one_complete_witness() {
    let f = Fake::new();
    let (owner, tree) = observe(&f, KEY, None, symbols).unwrap();
    assert_eq!(
        (owner.country, owner.tree, owner.target),
        (COUNTRY, OWN_TREE, TARGET)
    );
    assert_eq!(owner.members, [TARGET, OTHER]);
    let node = tree.target.unwrap();
    assert_eq!((node.item, node.frame, node.widget), (ITEM, NODE, SYMBOL));
    assert_eq!(
        node.rect,
        Rect {
            x: 900,
            y: 400,
            w: 40,
            h: 40
        }
    );
    assert_eq!(tree.obstacles.len(), 2);
    assert_eq!(f.checks.get(), 2);
    assert!(f.untouched(0x44086a8, 8));
    assert!(f.untouched(FOREIGN, 0x1000));
}

#[test]
fn real_detail_and_politics_controls_preserve_exact_identity() {
    let f = Fake::new();
    let (_, detail) = observe(&f, KEY, None, detail).unwrap();
    let detail = detail.unwrap();
    assert_eq!(
        (detail.item, detail.frame, detail.widget),
        (PANEL, DETAIL_FRAME, START)
    );
    assert_eq!(
        detail.rect,
        Rect {
            x: 1050,
            y: 700,
            w: 100,
            h: 30
        }
    );
    assert!(!detail.disabled);
    assert_eq!(f.checks.get(), 2);
    let f = Fake::new();
    let (_, open) = observe(&f, KEY, None, focus_button).unwrap();
    assert_eq!((open.frame, open.widget), (GOAL, OPEN_BUTTON));
    assert!(!open.disabled);
}

#[test]
fn canonical_foreign_membership_and_progress_owner_refuse_before_payloads() {
    let mut f = Fake::new();
    f.array(OWN_TREE + 0x28, &[TARGET, FOREIGN]);
    assert_eq!(
        observe(&f, KEY, None, symbols).unwrap_err().code,
        "invalid_focus_owner"
    );
    assert!(f.untouched(FOREIGN, 0x1000));
    assert!(f.untouched(TARGET, 0x200));
    let mut f = Fake::new();
    f.word(PROGRESS + 8, FOREIGN);
    assert_eq!(
        observe(&f, KEY, None, symbols).unwrap_err().code,
        "invalid_focus_owner"
    );
    assert!(f.untouched(OWN_TREE + 0x28, 16));
    assert!(f.untouched(FOREIGN, 0x1000));
}

#[test]
fn foreign_node_or_selected_detail_refuses_before_foreign_ui_payload() {
    let mut f = Fake::new();
    f.word(ITEM + 0x20, FOREIGN);
    assert_eq!(
        observe(&f, KEY, None, symbols).unwrap_err().code,
        "foreign_focus_scope"
    );
    assert!(f.untouched(FOREIGN, 0x1000));
    assert!(f.untouched(NODE, 0x1000));
    let mut f = Fake::new();
    f.word(PANEL + 0xba8, FOREIGN);
    assert_eq!(
        observe(&f, KEY, None, detail).unwrap_err().code,
        "foreign_focus_scope"
    );
    assert!(f.untouched(FOREIGN, 0x1000));
    assert!(f.untouched(DETAIL_FRAME, 0x1000));
}

#[test]
fn zero_or_foreign_displayed_country_refuses_before_item_and_panel_arrays() {
    for country in [0, FOREIGN] {
        for detail_probe in [false, true] {
            let mut f = Fake::new();
            f.word(FOCUS_VIEW + 0x1990, country);
            let code = if detail_probe {
                observe(&f, KEY, None, detail).unwrap_err().code
            } else {
                observe(&f, KEY, None, symbols).unwrap_err().code
            };
            assert_eq!(code, "foreign_focus_scope");
            assert!(f.untouched(FOCUS_VIEW + 0x18f0, 16));
            assert!(f.untouched(FOCUS_VIEW + 0x1938, 16));
        }
    }
}

#[test]
fn repeated_items_and_distinct_duplicate_target_nodes_are_ambiguous() {
    for second in [ITEM, SECOND_ITEM] {
        let mut f = Fake::new();
        f.word(SECOND_ITEM, 0x3405b78);
        f.word(SECOND_ITEM + 0x20, TARGET);
        f.array(FOCUS_VIEW + 0x18f0, &[ITEM, second]);
        assert_eq!(
            observe(&f, KEY, None, symbols).unwrap_err().code,
            "invalid_gui"
        );
        // A distinct duplicate refuses before dereferencing its container.
        assert!(f.untouched(SECOND_ITEM + 0x10, 8));
    }
}

#[test]
fn repeated_panels_and_multiple_visible_target_details_are_ambiguous() {
    for second in [PANEL, SECOND_PANEL] {
        let mut f = Fake::new();
        f.word(SECOND_PANEL, 0x3405158);
        f.word(SECOND_PANEL + 0xba8, TARGET);
        f.word(SECOND_PANEL + 0xba0, DETAIL_FRAME);
        f.array(FOCUS_VIEW + 0x1938, &[PANEL, second]);
        assert_eq!(
            observe(&f, KEY, None, detail).unwrap_err().code,
            "invalid_gui"
        );
    }
}

#[test]
fn duplicate_named_node_start_and_opener_controls_refuse() {
    let mut f = Fake::new();
    f.name(SECOND_SYMBOL, "symbol");
    f.array(NODE + 0xd30, &[SYMBOL, SECOND_SYMBOL, BACKGROUND]);
    assert_eq!(
        observe(&f, KEY, None, symbols).unwrap_err().code,
        "invalid_gui"
    );
    let mut f = Fake::new();
    f.name(SECOND_START, "start");
    f.array(DETAIL_FRAME + 0xd30, &[START, SECOND_START]);
    assert_eq!(
        observe(&f, KEY, None, detail).unwrap_err().code,
        "invalid_gui"
    );
    let mut f = Fake::new();
    f.array(GOAL + 0xd30, &[OPEN_BUTTON, OPEN_BUTTON]);
    assert_eq!(
        observe(&f, KEY, None, focus_button).unwrap_err().code,
        "invalid_gui"
    );
}

#[test]
fn oversized_panel_list_refuses_before_its_unmapped_payload() {
    let mut f = Fake::new();
    f.header(FOCUS_VIEW + 0x1938, FOREIGN, 9, 9);
    assert_eq!(
        observe(&f, KEY, None, detail).unwrap_err().code,
        "invalid_array"
    );
    assert!(f.untouched(FOREIGN, 9 * 8));
}

#[test]
fn torn_roots_membership_view_and_renderer_never_return_a_candidate() {
    let reference = Fake::new();
    let sprite = reference.get_word(SYMBOL + 0xd0);
    let sprite_type = reference.get_word(sprite + 8);
    for address in [
        0x3519740,
        COUNTRY + 0x10f8,
        TARGET + 0x130,
        IDLER + 0x670,
        FOCUS_VIEW + 0x1990,
        FOCUS_VIEW + 0x18f0,
        SYMBOL + 0xd0,
        sprite + 0xb0,
        sprite_type + 0x8c,
    ] {
        let mut f = Fake::new();
        f.torn = Some(address);
        assert_eq!(
            observe(&f, KEY, None, symbols).unwrap_err().code,
            "torn_read",
            "{address:x}"
        );
    }
    let mut f = Fake::new();
    f.fail_final = true;
    assert_eq!(
        observe(&f, KEY, None, detail).unwrap_err().code,
        "ownership_changed"
    );
}

#[test]
fn hidden_start_or_detail_refuses_and_disabled_start_remains_explicit() {
    let mut f = Fake::new();
    f.put(START + 0x65, &[0]);
    assert_eq!(
        observe(&f, KEY, None, detail).unwrap_err().code,
        "gui_target_unavailable"
    );
    assert!(f.untouched(START + 0x9c, 4));
    let mut f = Fake::new();
    f.put(DETAIL_FRAME + 0x95, &[0]);
    assert!(observe(&f, KEY, None, detail).unwrap().1.is_none());
    assert!(f.untouched(START, 0x100));
    let mut f = Fake::new();
    f.int(START + 0x9c, 3);
    assert!(observe(&f, KEY, None, detail).unwrap().1.unwrap().disabled);
    let mut f = Fake::new();
    f.int(START + 0x9c, 4);
    assert_eq!(
        observe(&f, KEY, None, detail).unwrap_err().code,
        "invalid_gui"
    );
}

#[test]
fn an_icon_named_start_cannot_supply_button_state() {
    let mut f = Fake::new();
    f.word(START, 0x34a81a8);
    assert_eq!(
        observe(&f, KEY, None, detail).unwrap_err().code,
        "invalid_focus_type"
    );
    assert!(f.untouched(START + 0x9c, 4));
}

#[test]
fn every_visible_obstacle_requires_supported_finite_geometry() {
    for variant in 0..3 {
        let mut f = Fake::new();
        let sprite = f.get_word(BACKGROUND + 0xd0);
        let kind = f.get_word(sprite + 8);
        match variant {
            0 => f.word(sprite, 0x34bdb18),
            1 => f.int(kind + 0x8c, 0),
            _ => f.float(sprite + 0xb0, f32::NAN),
        }
        let error = observe(&f, KEY, None, symbols).unwrap_err();
        assert_eq!(
            error.code,
            if variant == 0 {
                "gui_geometry_unavailable"
            } else {
                "invalid_gui_geometry"
            }
        );
    }
}

#[test]
fn same_key_replacement_cannot_adopt_the_original_owner_binding() {
    let f = Fake::new();
    let (original, ()) = observe(&f, KEY, None, |_, _, _| Ok(())).unwrap();
    let mut replacement = Fake::new();
    replacement.focus(REPLACEMENT, KEY);
    replacement.array(OWN_TREE + 0x28, &[REPLACEMENT, OTHER]);
    replacement.array(DATABASE + 0x58, &[OTHER, REPLACEMENT]);
    let (new, ()) = observe(&replacement, KEY, None, |_, _, _| Ok(())).unwrap();
    assert_eq!(new.target, REPLACEMENT);
    assert_eq!(original.target, TARGET);
    let error = observe(
        &replacement,
        KEY,
        Some(&original),
        |_, _, _| -> Result<()> {
            panic!("Replaced original target must refuse before any UI probe")
        },
    )
    .unwrap_err();
    assert_eq!(error.code, "ownership_changed");
    assert!(replacement.untouched(IDLER + 0x670, 8));
}

#[test]
fn final_control_equality_detects_movement_and_disabled_state_changes() {
    let f = Fake::new();
    let (original_owner, original) = observe(&f, KEY, None, detail).unwrap();
    let original = original.unwrap();
    for change_state in [false, true] {
        let mut changed = Fake::new();
        if change_state {
            changed.int(START + 0x9c, 3);
        } else {
            let sprite = changed.get_word(START + 0xd8);
            changed.float(sprite + 0xb0, 91.0);
        }
        let current = observe(&changed, KEY, Some(&original_owner), detail)
            .unwrap()
            .1
            .unwrap();
        assert_eq!(
            unchanged(&original, &current).unwrap_err().code,
            "focus_state_changed"
        );
    }
}
