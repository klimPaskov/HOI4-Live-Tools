use super::*;
use crate::memory::ReadMemory;
use std::cell::{Cell, RefCell};

#[test]
fn typed_assignment_binding_refuses_replacement_before_preparation() {
    use crate::memory::StringAbi;
    use crate::own_focus::assignment_fixture::{Fixture, COUNTRY, FIRST, FOREIGN, SECOND};
    let f = Fixture::new(StringAbi::LibCxx);
    let before = f.observe(FIRST).unwrap();
    assignment_owner(&f, FIRST, &before).unwrap();
    for variant in 0..4 {
        let mut f = Fixture::new(StringAbi::LibCxx);
        let before = f.observe(FIRST).unwrap();
        let replacement = match variant {
            0 => f.replace_focus(FIRST),
            1 => f.replace_focus(SECOND),
            2 => {
                f.word(COUNTRY + f.game_layout().country_focus_tree, FOREIGN);
                FOREIGN
            }
            _ => {
                f.word(COUNTRY + f.game_layout().country_technology_status, FOREIGN);
                FOREIGN
            }
        };
        let prepared = Cell::new(false);
        let result = assignment_owner(&f, FIRST, &before).map(|_| prepared.set(true));
        assert_eq!(result.unwrap_err().code, "ownership_changed");
        assert!(!prepared.get());
        assert!(
            f.untouched(replacement, 8),
            "replacement payload read before binding refusal"
        );
    }
}

#[test]
fn typed_status_replacement_after_initial_binding_refuses_before_payload_or_gui() {
    use crate::memory::StringAbi;
    use crate::own_focus::assignment_fixture::{Fixture, COUNTRY, FIRST, FOREIGN};
    let mut f = Fixture::new(StringAbi::LibCxx);
    let prior = f.observe(FIRST).unwrap();
    let owner = assignment_owner(&f, FIRST, &prior).unwrap();
    f.word(COUNTRY + f.game_layout().country_technology_status, FOREIGN);
    let entered_gui = Cell::new(false);
    let result = observe(&f, FIRST, Some(&owner), |_, _, _| {
        entered_gui.set(true);
        Ok(())
    });
    assert_eq!(result.unwrap_err().code, "ownership_changed");
    assert!(!entered_gui.get());
    assert!(f.untouched(FOREIGN, 8));
}

const KEY: &str = "GER_rhineland";
const GS: u64 = 0x100000;
const IDLER: u64 = 0x110000;
const COUNTRY: u64 = 0x120000;
const STATUS: u64 = 0x1a0000;
const TECHNOLOGY_DATABASE: u64 = 0x1b0000;
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
const OTHER_NODE: u64 = 0x370000;
const OTHER_SYMBOL: u64 = 0x380000;
const OTHER_BACKGROUND: u64 = 0x390000;
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
        self.word(object, 0x3589108);
        self.word(object + 0x130, OWN_TREE);
        self.string(object + 0x18, key);
    }
    fn container(&mut self, object: u64, children: &[u64]) {
        self.word(object, 0x36ba978);
        self.put(object + 0x95, &[8]);
        self.array(object + 0xe50, children);
    }
    fn name(&mut self, widget: u64, name: &str) {
        let kind = self.alloc();
        self.word(widget + 0x50, kind);
        self.word(kind, 0x34a0000);
        self.string(kind + 0x28, name);
    }
    fn widget(&mut self, object: u64, name: &str, button: bool, rect: Rect) {
        self.word(object, if button { 0x36b9738 } else { 0x36b6880 });
        self.name(object, name);
        self.put(object + 0x65, &[8]);
        if button {
            self.int(object + 0x9c, 0);
        }
        let sprite = self.alloc();
        let kind = self.alloc();
        self.word(object + if button { 0xe0 } else { 0xd8 }, sprite);
        self.word(sprite, 0x36cc528);
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
    fn unmap(&mut self, address: u64, size: u64) {
        self.data
            .retain(|a, _| !(address..address + size).contains(a));
    }
    fn with_other_node() -> Self {
        let mut f = Self::new();
        f.word(SECOND_ITEM, 0x3611108);
        f.word(SECOND_ITEM + 0x20, OTHER);
        f.word(SECOND_ITEM + 0x10, OTHER_NODE);
        f.array(FOCUS_VIEW + 0x2d70, &[ITEM, SECOND_ITEM]);
        f.container(OTHER_NODE, &[OTHER_SYMBOL, OTHER_BACKGROUND]);
        f.widget(
            OTHER_SYMBOL,
            "symbol",
            false,
            Rect {
                x: 1100,
                y: 400,
                w: 40,
                h: 40,
            },
        );
        f.widget(
            OTHER_BACKGROUND,
            "bg",
            false,
            Rect {
                x: 1090,
                y: 390,
                w: 60,
                h: 60,
            },
        );
        f
    }
    fn new() -> Self {
        let mut f = Self::empty();
        let g = game::layout(&Reader::new(&f));
        let technology = crate::research::layout(&Reader::new(&f));
        f.word(g.technology_db_global, TECHNOLOGY_DATABASE);
        f.word(TECHNOLOGY_DATABASE, technology.database_vtable);
        f.word(COUNTRY + g.country_technology_status, STATUS);
        f.word(STATUS, technology.status_vtable);
        f.word(STATUS + 0x130, COUNTRY);
        f.int(STATUS + 0x138, 1);
        for (address, value) in [
            (0x372b040, GS),
            (0x372afc0, IDLER),
            (GS, 0x35044e8),
            (IDLER, 0x35089d8),
            (COUNTRY, 0x357e388),
            (COUNTRY + 0x10f8, OWN_TREE),
            (COUNTRY + 0x1108, PROGRESS),
            (OWN_TREE, 0x3589048),
            (0x3704440, DATABASE),
            (DATABASE, 0x35891c8),
            (PROGRESS, 0x3589270),
            (PROGRESS + 8, COUNTRY),
            (PROGRESS + 0x10, 0),
            (IDLER + 0x670, MANAGER),
            (MANAGER + 0x130, FOCUS_VIEW),
            (MANAGER + 0x128, POLITICS_VIEW),
            (FOCUS_VIEW, MACOS.focus_view_vtable),
            (FOCUS_VIEW + 0x490, FOCUS_ROOT),
            (POLITICS_VIEW + 0x490, POLITICS_ROOT),
            (FOCUS_VIEW + 0x2e10, COUNTRY),
            (ITEM, 0x3611108),
            (ITEM + 0x20, TARGET),
            (ITEM + 0x10, NODE),
            (PANEL, 0x36106d8),
            (PANEL + 0xcc8, TARGET),
            (PANEL + 0xcc0, DETAIL_FRAME),
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
        f.float(FOCUS_VIEW + 0x2e34, 1.0);
        f.array(FOCUS_VIEW + 0x2d70, &[ITEM]);
        f.array(FOCUS_VIEW + 0x2db8, &[PANEL]);
        f.container(FOCUS_ROOT, &[]);
        f.container(POLITICS_ROOT, &[GOAL + 0x30]);
        f.container(GOAL, &[OPEN_BUTTON]);
        // Containers appear in parent children lists as their GUI-object base.
        f.word(GOAL + 0x30, 0x36ba9c8);
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
    f.word(PANEL + 0xcc8, FOREIGN);
    assert_eq!(
        observe(&f, KEY, None, detail).unwrap_err().code,
        "foreign_focus_scope"
    );
    assert!(f.untouched(FOREIGN, 0x1000));
    assert!(f.untouched(DETAIL_FRAME, 0x1000));
}

#[test]
fn foreign_country_override_refuses_every_probe_before_gui_or_foreign_payload() {
    for mode in 0..4 {
        let mut f = Fake::new();
        f.word(FOCUS_VIEW + MACOS.country_override, FOREIGN);
        f.unmap(FOCUS_VIEW + MACOS.view_root, 8);
        let error = match mode {
            0 => observe(&f, KEY, None, view_state).unwrap_err(),
            1 => observe(&f, KEY, None, node).unwrap_err(),
            2 => observe(&f, KEY, None, symbols).unwrap_err(),
            _ => observe(&f, KEY, None, detail).unwrap_err(),
        };
        assert_eq!(error.code, "foreign_focus_scope");
        assert_eq!(
            error.message,
            "Focus country override differs from own country"
        );
        assert!(f.untouched(FOCUS_VIEW + MACOS.view_root, 8));
        assert!(f.untouched(FOCUS_VIEW + MACOS.items, 16));
        assert!(f.untouched(FOCUS_VIEW + MACOS.panels, 16));
        assert!(f.untouched(FOREIGN, 0x1000));
    }
}

#[test]
fn null_override_uses_the_same_positive_own_binding_for_node_tree_and_detail() {
    let mut implicit = Fake::new();
    implicit.word(FOCUS_VIEW + MACOS.country_override, 0);
    let (owner, node) = observe(&implicit, KEY, None, node).unwrap();
    let node = node.unwrap();
    assert_eq!(node.view.country_override, Some(0));
    assert_eq!(
        (owner.country, owner.status, owner.tree),
        (COUNTRY, STATUS, OWN_TREE)
    );
    let mut implicit = Fake::new();
    implicit.word(FOCUS_VIEW + MACOS.country_override, 0);
    let (tree_owner, tree) = observe(&implicit, KEY, None, symbols).unwrap();
    assert_eq!(tree_owner, owner);
    assert_eq!(tree.target, Some(node));
    assert_eq!(tree.obstacles.len(), 2);
    let mut implicit = Fake::new();
    implicit.word(FOCUS_VIEW + MACOS.country_override, 0);
    let (detail_owner, control) = observe(&implicit, KEY, None, detail).unwrap();
    assert_eq!(detail_owner, owner);
    let control = control.unwrap();
    assert_eq!((control.item, control.widget), (PANEL, START));
    assert_eq!(control.view.country_override, Some(0));
    assert!(implicit.untouched(FOREIGN, 0x1000));
}

#[test]
fn focus_primary_is_required_before_override_and_gui_payload_for_every_probe() {
    for mode in 0..4 {
        let mut f = Fake::new();
        f.word(FOCUS_VIEW, MACOS.focus_view_vtable + 8);
        f.unmap(FOCUS_VIEW + MACOS.country_override, 8);
        f.unmap(FOCUS_VIEW + MACOS.view_root, 8);
        let error = match mode {
            0 => observe(&f, KEY, None, view_state).unwrap_err(),
            1 => observe(&f, KEY, None, node).unwrap_err(),
            2 => observe(&f, KEY, None, symbols).unwrap_err(),
            _ => observe(&f, KEY, None, detail).unwrap_err(),
        };
        assert_eq!(error.code, "invalid_focus_type");
        assert!(f.untouched(FOCUS_VIEW + MACOS.country_override, 8));
        assert!(f.untouched(FOCUS_VIEW + MACOS.view_root, 8));
        assert!(f.untouched(FOCUS_VIEW + MACOS.items, 16));
        assert!(f.untouched(FOCUS_VIEW + MACOS.panels, 16));
    }
}

fn sparse_readiness(country_override: u64, visible: bool) -> Fake {
    let mut f = Fake::new();
    f.word(FOCUS_VIEW + MACOS.country_override, country_override);
    f.put(FOCUS_ROOT + 0x95, &[if visible { 8 } else { 0 }]);
    // Readiness must work without materialized collections, their payloads,
    // zoom, or root children. A following country pointer is also unmapped.
    f.unmap(FOCUS_VIEW + MACOS.items, 16);
    f.unmap(FOCUS_VIEW + MACOS.panels, 16);
    f.unmap(FOCUS_VIEW + MACOS.zoom, 4);
    f.unmap(FOCUS_ROOT + 0xe50, 16);
    f.unmap(ITEM, 0x1000);
    f.unmap(PANEL, 0x1000);
    f
}

fn assert_no_readiness_payload(f: &Fake) {
    assert!(f.untouched(FOCUS_VIEW + MACOS.items, 16));
    assert!(f.untouched(FOCUS_VIEW + MACOS.panels, 16));
    assert!(f.untouched(FOCUS_VIEW + MACOS.zoom, 4));
    assert!(f.untouched(FOCUS_ROOT + 0xe50, 16));
    assert!(f.untouched(ITEM, 0x1000));
    assert!(f.untouched(PANEL, 0x1000));
    assert!(f.untouched(FOREIGN, 0x1000));
}

#[test]
fn visible_implicit_own_focus_view_is_ready_without_item_or_panel_arrays() {
    let f = sparse_readiness(0, true);
    assert_eq!(
        observe(&f, KEY, None, view_state).unwrap().1,
        FocusViewState::Ready
    );
    assert_no_readiness_payload(&f);
    assert_eq!(f.checks.get(), 2);
}

#[test]
fn hidden_implicit_own_focus_view_is_hidden_without_item_or_panel_arrays() {
    let f = sparse_readiness(0, false);
    assert_eq!(
        observe(&f, KEY, None, view_state).unwrap().1,
        FocusViewState::Hidden
    );
    assert_no_readiness_payload(&f);
    assert_eq!(f.checks.get(), 2);
}

#[test]
fn hidden_own_focus_view_is_hidden_without_item_or_panel_arrays() {
    let f = sparse_readiness(COUNTRY, false);
    assert_eq!(
        observe(&f, KEY, None, view_state).unwrap().1,
        FocusViewState::Hidden
    );
    assert_no_readiness_payload(&f);
    assert_eq!(f.checks.get(), 2);
}

#[test]
fn visible_own_focus_view_is_ready_without_authorizing_a_target() {
    let f = sparse_readiness(COUNTRY, true);
    assert_eq!(
        observe(&f, KEY, None, view_state).unwrap().1,
        FocusViewState::Ready
    );
    assert_no_readiness_payload(&f);
    assert_eq!(f.checks.get(), 2);

    // Ready did not prove the still-unmapped target/obstacle payloads.
    let f = sparse_readiness(COUNTRY, true);
    assert_eq!(
        observe(&f, KEY, None, node).unwrap_err().code,
        "fixture_unmapped"
    );
    assert!(f.untouched(FOCUS_VIEW + MACOS.items, 16));
}

#[test]
fn foreign_readiness_refuses_before_visibility_country_payload_or_arrays() {
    for visible in [false, true] {
        let mut f = sparse_readiness(FOREIGN, visible);
        // A nonzero foreign scope must refuse without treating it as hidden
        // or pending, even when visibility itself has not materialized.
        f.unmap(FOCUS_ROOT + 0x95, 1);
        f.unmap(FOCUS_VIEW + MACOS.view_root, 8);
        let error = observe(&f, KEY, None, view_state).unwrap_err();
        assert_eq!(error.code, "foreign_focus_scope");
        assert_eq!(
            error.message,
            "Focus country override differs from own country"
        );
        assert!(f.untouched(FOCUS_ROOT + 0x95, 1));
        assert_no_readiness_payload(&f);
    }
}

#[test]
fn readiness_states_require_final_view_witness_and_process_check() {
    for (country_override, visible, state) in [
        (0, true, FocusViewState::Ready),
        (0, false, FocusViewState::Hidden),
        (COUNTRY, false, FocusViewState::Hidden),
        (COUNTRY, true, FocusViewState::Ready),
    ] {
        for address in [
            0x372b040,
            0x372afc0,
            game::layout(&Reader::new(&Fake::new())).technology_db_global,
            GS + 0x4d8,
            COUNTRY + 0xd08,
            STATUS + 0x130,
            IDLER + MACOS.manager,
            MANAGER + MACOS.focus_view,
            FOCUS_VIEW,
            FOCUS_VIEW + MACOS.view_root,
            FOCUS_ROOT,
            FOCUS_ROOT + 0x95,
            FOCUS_VIEW + MACOS.country_override,
        ] {
            let mut f = sparse_readiness(country_override, visible);
            f.torn = Some(address);
            let initially_observed = Cell::new(None);
            let result = observe(&f, KEY, None, |r, image, own| {
                let value = view_state(r, image, own)?;
                initially_observed.set(Some(value));
                Ok(value)
            });
            assert_eq!(initially_observed.get(), Some(state));
            assert_eq!(result.unwrap_err().code, "torn_read", "{address:x}");
            assert_eq!(f.checks.get(), 1);
            assert_no_readiness_payload(&f);
        }
        let mut f = sparse_readiness(country_override, visible);
        f.fail_final = true;
        assert_eq!(
            observe(&f, KEY, None, view_state).unwrap_err().code,
            "ownership_changed"
        );
        assert_no_readiness_payload(&f);
    }
}

#[test]
fn null_override_cannot_bypass_positive_player_map_or_status_ownership() {
    for variant in 0..6 {
        let mut f = sparse_readiness(0, true);
        let expected_code = match variant {
            0 | 1 => {
                f.int(GS + 0x4d8, if variant == 0 { 0 } else { -1 });
                "player_unavailable"
            }
            2 => {
                let tags = f.get_word(GS + 0x308);
                f.int(tags + 4, -1);
                "player_unavailable"
            }
            3 => {
                f.int(COUNTRY + 8, 2);
                "invalid_focus_owner"
            }
            4 => {
                f.word(STATUS + 0x130, FOREIGN);
                "invalid_focus_owner"
            }
            _ => {
                f.int(STATUS + 0x138, 2);
                "invalid_focus_owner"
            }
        };
        assert_eq!(
            observe(&f, KEY, None, view_state).unwrap_err().code,
            expected_code
        );
        assert!(f.untouched(IDLER + MACOS.manager, 8));
        assert!(f.untouched(FOCUS_VIEW + MACOS.country_override, 8));
        assert_no_readiness_payload(&f);
    }
}

#[test]
fn changed_own_override_branch_cannot_replace_a_captured_control() {
    for detail_probe in [false, true] {
        let mut implicit = Fake::new();
        implicit.word(FOCUS_VIEW + MACOS.country_override, 0);
        let (owner, candidate) = if detail_probe {
            observe(&implicit, KEY, None, detail).unwrap()
        } else {
            observe(&implicit, KEY, None, node).unwrap()
        };
        let candidate = candidate.unwrap();
        let explicit = Fake::new();
        let current = if detail_probe {
            observe(&explicit, KEY, Some(&owner), detail).unwrap().1
        } else {
            observe(&explicit, KEY, Some(&owner), node).unwrap().1
        }
        .unwrap();
        assert_eq!(current.rect, candidate.rect);
        assert_eq!(current.view.country_override, Some(COUNTRY));
        assert_eq!(
            unchanged(&candidate, &current).unwrap_err().code,
            "focus_state_changed"
        );
    }
}

#[test]
fn repeated_items_and_distinct_duplicate_target_nodes_are_ambiguous() {
    for second in [ITEM, SECOND_ITEM] {
        let mut f = Fake::new();
        f.word(SECOND_ITEM, 0x3611108);
        f.word(SECOND_ITEM + 0x20, TARGET);
        f.array(FOCUS_VIEW + 0x2d70, &[ITEM, second]);
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
        f.word(SECOND_PANEL, 0x36106d8);
        f.word(SECOND_PANEL + 0xcc8, TARGET);
        f.word(SECOND_PANEL + 0xcc0, DETAIL_FRAME);
        f.array(FOCUS_VIEW + 0x2db8, &[PANEL, second]);
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
    f.array(NODE + 0xe50, &[SYMBOL, SECOND_SYMBOL, BACKGROUND]);
    assert_eq!(
        observe(&f, KEY, None, symbols).unwrap_err().code,
        "invalid_gui"
    );
    let mut f = Fake::new();
    f.name(SECOND_START, "start");
    f.array(DETAIL_FRAME + 0xe50, &[START, SECOND_START]);
    assert_eq!(
        observe(&f, KEY, None, detail).unwrap_err().code,
        "invalid_gui"
    );
    let mut f = Fake::new();
    f.array(GOAL + 0xe50, &[OPEN_BUTTON, OPEN_BUTTON]);
    assert_eq!(
        observe(&f, KEY, None, focus_button).unwrap_err().code,
        "invalid_gui"
    );
}

#[test]
fn oversized_panel_list_refuses_before_its_unmapped_payload() {
    let mut f = Fake::new();
    f.header(FOCUS_VIEW + 0x2db8, FOREIGN, 9, 9);
    assert_eq!(
        observe(&f, KEY, None, detail).unwrap_err().code,
        "invalid_array"
    );
    assert!(f.untouched(FOREIGN, 9 * 8));
}

#[test]
fn torn_roots_membership_view_and_renderer_never_return_a_candidate() {
    let reference = Fake::new();
    let sprite = reference.get_word(SYMBOL + 0xd8);
    let sprite_type = reference.get_word(sprite + 8);
    for address in [
        0x372b040,
        COUNTRY + 0x10f8,
        TARGET + 0x130,
        IDLER + 0x670,
        FOCUS_VIEW + 0x2e10,
        FOCUS_VIEW + 0x2d70,
        SYMBOL + 0xd8,
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
    f.word(START, 0x36b6880);
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
        let sprite = f.get_word(BACKGROUND + 0xd8);
        let kind = f.get_word(sprite + 8);
        match variant {
            0 => f.word(sprite, 0x36cc530),
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
            let sprite = changed.get_word(START + 0xe0);
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

#[test]
fn target_probe_matches_full_control_with_fewer_read_requests_and_bytes() {
    let full = Fake::with_other_node();
    let (full_owner, tree) = observe(&full, KEY, None, symbols).unwrap();
    assert_eq!(tree.obstacles.len(), 4);
    let target = Fake::with_other_node();
    let (target_owner, control) = observe(&target, KEY, None, node).unwrap();
    assert_eq!(full_owner, target_owner);
    assert_eq!(tree.target, control);
    assert_eq!(target.checks.get(), 2);
    let full_reads = full.reads.borrow();
    let target_reads = target.reads.borrow();
    let full_bytes: usize = full_reads.iter().map(|(_, n)| *n).sum();
    let target_bytes: usize = target_reads.iter().map(|(_, n)| *n).sum();
    assert!(target_reads.len() < full_reads.len());
    assert!(target_bytes < full_bytes);
    println!(
        "FOCUS_NODE_READ_REQUESTS={} FULL_TREE_READ_REQUESTS={} FOCUS_NODE_BYTES={} FULL_TREE_BYTES={}",
        target_reads.len(), full_reads.len(), target_bytes, full_bytes
    );
}

#[test]
fn target_probe_skips_unrelated_container_and_target_background_geometry() {
    let mut f = Fake::with_other_node();
    f.unmap(SECOND_ITEM + 0x10, 8);
    f.unmap(OTHER_NODE, 0x1000);
    f.unmap(BACKGROUND + 0xd8, 8);
    assert!(observe(&f, KEY, None, node).unwrap().1.is_some());
    assert!(f.untouched(SECOND_ITEM + 0x10, 8));
    assert!(f.untouched(OTHER_NODE, 0x1000));
    assert!(f.untouched(OTHER_SYMBOL, 0x1000));
    assert!(f.untouched(BACKGROUND + 0x65, 1));
    assert!(f.untouched(BACKGROUND + 0xd8, 8));
}

#[test]
fn target_geometry_success_cannot_authorize_an_unproved_drag_background() {
    for missing_background in [false, true] {
        let mut target = Fake::with_other_node();
        let mut full = Fake::with_other_node();
        for f in [&mut target, &mut full] {
            let widget = if missing_background {
                BACKGROUND
            } else {
                OTHER_SYMBOL
            };
            let sprite = f.get_word(widget + 0xd8);
            f.unmap(sprite, 0x1000);
        }
        assert!(observe(&target, KEY, None, node).unwrap().1.is_some());
        assert_eq!(
            observe(&full, KEY, None, symbols).unwrap_err().code,
            "fixture_unmapped"
        );
    }
}

#[test]
fn both_probes_validate_all_items_before_any_target_container_payload() {
    for mode in 0..4 {
        for full in [false, true] {
            let mut f = Fake::new();
            f.unmap(NODE, 0x1000);
            f.word(SECOND_ITEM, if mode == 3 { 0x3611110 } else { 0x3611108 });
            f.word(SECOND_ITEM + 0x20, if mode == 2 { FOREIGN } else { TARGET });
            f.array(
                FOCUS_VIEW + 0x2d70,
                &[ITEM, if mode == 0 { ITEM } else { SECOND_ITEM }],
            );
            let error = if full {
                observe(&f, KEY, None, symbols).unwrap_err()
            } else {
                observe(&f, KEY, None, node).unwrap_err()
            };
            assert_eq!(
                error.code,
                match mode {
                    2 => "foreign_focus_scope",
                    3 => "invalid_focus_type",
                    _ => "invalid_gui",
                }
            );
            assert!(f.untouched(ITEM + 0x10, 8));
            assert!(f.untouched(NODE, 0x1000));
            assert!(f.untouched(FOREIGN, 0x1000));
        }
    }
}

#[test]
fn target_probe_witnesses_non_target_item_membership_and_full_target_geometry() {
    let reference = Fake::with_other_node();
    let sprite = reference.get_word(SYMBOL + 0xd8);
    let kind = reference.get_word(sprite + 8);
    for address in [
        0x372b040,
        COUNTRY + 0x10f8,
        TARGET + 0x130,
        FOCUS_VIEW + 0x2d70,
        SECOND_ITEM,
        SECOND_ITEM + 0x20,
        FOCUS_VIEW + 0x2e10,
        FOCUS_VIEW + 0x2e34,
        ITEM + 0x10,
        NODE + 0x95,
        SYMBOL + 0x65,
        SYMBOL + 0xd8,
        sprite,
        sprite + 0xb0,
        kind + 0x8c,
    ] {
        let mut f = Fake::with_other_node();
        f.torn = Some(address);
        assert_eq!(
            observe(&f, KEY, None, node).unwrap_err().code,
            "torn_read",
            "{address:x}"
        );
    }
}

#[test]
fn target_probe_preserves_absent_hidden_and_ambiguous_symbol_behavior() {
    let mut f = Fake::with_other_node();
    f.array(FOCUS_VIEW + 0x2d70, &[SECOND_ITEM]);
    assert!(observe(&f, KEY, None, node).unwrap().1.is_none());
    assert!(f.untouched(OTHER_NODE, 0x1000));
    let mut f = Fake::new();
    f.put(NODE + 0x95, &[0]);
    assert!(observe(&f, KEY, None, node).unwrap().1.is_none());
    assert!(f.untouched(SYMBOL, 0x100));
    let mut f = Fake::new();
    f.put(SYMBOL + 0x65, &[0]);
    assert_eq!(
        observe(&f, KEY, None, node).unwrap_err().code,
        "gui_target_unavailable"
    );
    let mut f = Fake::new();
    f.name(SECOND_SYMBOL, "symbol");
    f.array(NODE + 0xe50, &[SYMBOL, SECOND_SYMBOL, BACKGROUND]);
    assert_eq!(
        observe(&f, KEY, None, node).unwrap_err().code,
        "invalid_gui"
    );
}

#[test]
fn target_probe_keeps_original_owner_and_final_control_equality() {
    let original = Fake::new();
    let (owner, control) = observe(&original, KEY, None, node).unwrap();
    let control = control.unwrap();
    for zoom_changed in [false, true] {
        let mut f = Fake::new();
        if zoom_changed {
            f.float(FOCUS_VIEW + 0x2e34, 0.5);
        } else {
            let sprite = f.get_word(SYMBOL + 0xd8);
            f.float(sprite + 0xb0, -59.0);
        }
        let changed = observe(&f, KEY, Some(&owner), node).unwrap().1.unwrap();
        assert_eq!(
            unchanged(&control, &changed).unwrap_err().code,
            "focus_state_changed"
        );
    }
    let mut replacement = Fake::new();
    replacement.focus(REPLACEMENT, KEY);
    replacement.array(OWN_TREE + 0x28, &[REPLACEMENT, OTHER]);
    replacement.array(DATABASE + 0x58, &[REPLACEMENT, OTHER]);
    replacement.word(ITEM + 0x20, REPLACEMENT);
    assert_eq!(
        observe(&replacement, KEY, Some(&owner), node)
            .unwrap_err()
            .code,
        "ownership_changed"
    );
    assert!(replacement.untouched(IDLER + 0x670, 8));
}

#[test]
fn read_only_probe_projects_identical_target_and_measures_only_requested_geometry() {
    let target = Fake::with_other_node();
    let target_value = super::super::probe::snapshot(&target, KEY, false).unwrap();
    let full = Fake::with_other_node();
    let full_value = super::super::probe::snapshot(&full, KEY, true).unwrap();
    assert_eq!(target_value["target"], full_value["target"]);
    assert_eq!(target_value["target"]["on_tree"], true);
    assert!(target_value["pan"].is_null());
    assert_eq!(full_value["pan"]["obstacle_count"], 4);
    assert_eq!(full_value["pan"]["required"], false);
    assert!(full_value["pan"]["plan"].is_null());
    assert_eq!(full_value["pan"]["prediction_only"], true);
    assert!(
        target_value["payload_read_calls"].as_u64().unwrap()
            < full_value["payload_read_calls"].as_u64().unwrap()
    );
    assert!(
        target_value["payload_read_bytes"].as_u64().unwrap()
            < full_value["payload_read_bytes"].as_u64().unwrap()
    );
    println!(
        "FOCUS_PROBE_NODE_PAYLOAD_CALLS={} FULL_PAYLOAD_CALLS={} FOCUS_PROBE_NODE_PAYLOAD_BYTES={} FULL_PAYLOAD_BYTES={}",
        target_value["payload_read_calls"], full_value["payload_read_calls"],
        target_value["payload_read_bytes"], full_value["payload_read_bytes"]
    );
    for (process, value) in [(&target, target_value), (&full, full_value)] {
        assert_eq!(value["input_authorized"], false);
        assert_eq!(value["eligibility_available"], false);
        assert_eq!(value["observation_paused_game"], false);
        assert!(value["read_count_scope"]
            .as_str()
            .unwrap()
            .contains("excludes final witness"));
        assert!(value["elapsed_ms"].as_f64().unwrap() >= 0.0);
        assert_eq!(process.checks.get(), 2);
    }
}

#[test]
fn read_only_probe_predicts_pan_without_authorizing_input() {
    let mut f = Fake::with_other_node();
    let sprite = f.get_word(SYMBOL + 0xd8);
    f.float(sprite + 0xb0, -2000.0);
    let value = super::super::probe::snapshot(&f, KEY, true).unwrap();
    assert_eq!(value["pan"]["required"], true);
    assert_eq!(value["pan"]["prediction_only"], true);
    let from = value["pan"]["plan"]["from"].as_array().unwrap();
    let to = value["pan"]["plan"]["to"].as_array().unwrap();
    assert!(to[0].as_f64().unwrap() > from[0].as_f64().unwrap());
    assert_eq!(to[1], from[1]);
    assert_eq!(value["input_authorized"], false);
    assert_eq!(value["observation_paused_game"], false);
    assert_eq!(f.checks.get(), 2);
}

#[test]
fn read_only_probe_never_projects_torn_geometry_or_invalid_own_scope() {
    for include_pan in [false, true] {
        let mut f = Fake::new();
        f.torn = Some(f.get_word(SYMBOL + 0xd8) + 0xb0);
        assert_eq!(
            super::super::probe::snapshot(&f, KEY, include_pan)
                .unwrap_err()
                .code,
            "torn_read"
        );
        let mut f = Fake::new();
        f.int(COUNTRY + 8, 2);
        assert_eq!(
            super::super::probe::snapshot(&f, KEY, include_pan)
                .unwrap_err()
                .code,
            "invalid_focus_owner"
        );
        assert!(f.untouched(IDLER + 0x670, 8));
    }
}
