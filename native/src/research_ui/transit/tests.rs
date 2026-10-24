use super::*;
use crate::{
    actions::Epoch,
    daily_governor::{EconomyTarget, Entry, Intent},
};
use std::{
    cell::{Cell, RefCell},
    collections::BTreeMap,
    time::Duration,
};

const BASE: u64 = 0x10000;
const GS: u64 = 0x30000;
const IDLER: u64 = 0x32000;
const COUNTRY: u64 = 0x20000;
const STATUS: u64 = 0x28000;
const DB: u64 = 0x36000;
const REGISTRY: u64 = 0x70000;
const MANAGER: u64 = 0x90000;
const VIEW: u64 = 0x91000;
const TREE: u64 = 0x92000;
const PANEL: u64 = 0xa0000;
const TREE_ROOT: u64 = 0xd0000;
const FACILITIES: u64 = 0x330000;
const FACILITIES_ROOT: u64 = 0x331000;
const PARENT: u64 = 0x332000;
const SLOT_DATA: u64 = 0x3b000;
const SLOT: u64 = 0x2c000;
const CATALOG_DATA: u64 = 0x62000;
const OWN_DATA: u64 = 0x82000;
const TECHNOLOGY: u64 = 0x100000;
const TEMPLATE: u64 = 0x110000;
const POISON: u64 = 0xdead0000;
const PLAYTHROUGH: &str = "aabbccdd-1122-4344-8566-778899aabbcc";

#[test]
fn bind_current_uses_observed_settled_panel_without_assumed_source_retry() {
    for mode in [23, 5] {
        let f = Fixture::new(mode);
        let (mut anchor, s) = Anchor::bind_current(&f, &f.scope(), f.until).unwrap();
        assert_eq!(
            s.panel(),
            if mode == 23 {
                ManagedPanel::Idle
            } else {
                ManagedPanel::Research
            }
        );
        assert_eq!(anchor.observe(&f).unwrap().panel(), s.panel());
    }
}

#[test]
fn bind_current_unknown_source_cannot_fall_back_or_read_research_payload() {
    let f = Fixture::new(13);
    assert_eq!(
        Anchor::bind_current(&f, &f.scope(), f.until)
            .unwrap_err()
            .code,
        "research_transit_precondition"
    );
    assert!(!f.reads.borrow().iter().any(|(a, _)| *a == TECHNOLOGY));
}

#[test]
fn ordinary_input_read_requires_extra_keyboard_facts_and_original_owner_first() {
    let mut f = Fixture::new(23);
    let (mut anchor, _) = Anchor::bind_current(&f, &f.scope(), f.until).unwrap();
    // Observation-only sparse memory intentionally supplies no keyboard gates.
    assert!(anchor.observe_for_input(&f).is_err());
    f.word(MANAGER + 0xf0, POISON);
    f.reads.borrow_mut().clear();
    assert_eq!(
        anchor.observe_for_input(&f).unwrap_err().code,
        "ownership_changed"
    );
    assert!(!f
        .reads
        .borrow()
        .iter()
        .any(|(a, _)| *a == POISON || *a == MANAGER + 0x3e1));
}

// Sparse generated fields derived from tests/snapshot.rs owned_research_fixture
// and support/research_close_cases.rs, not a captured process or game payload.
struct Fixture {
    data: BTreeMap<u64, u8>,
    reads: RefCell<Vec<(u64, usize)>>,
    id: Identity,
    image: u64,
    abi: StringAbi,
    checks: Cell<usize>,
    fail_after_check: Option<usize>,
    clock: Cell<Instant>,
    until: Instant,
    late_at: Option<u64>,
    error_at: Option<u64>,
    short_at: Option<u64>,
    late_finish: bool,
    mutate_finish: Option<u64>,
}
impl Fixture {
    fn put(&mut self, a: u64, bytes: &[u8]) {
        self.data
            .extend(bytes.iter().enumerate().map(|(i, b)| (a + i as u64, *b)));
    }
    fn word(&mut self, a: u64, value: u64) {
        self.put(a, &value.to_le_bytes());
    }
    fn number(&mut self, a: u64, value: i32) {
        self.put(a, &value.to_le_bytes());
    }
    fn string(&mut self, a: u64, value: &str, heap: u64) {
        self.put(a, &[0; 32]);
        let p = if value.len() <= 15 { a + 16 } else { heap };
        self.word(a, p);
        self.word(a + 8, value.len() as u64);
        self.put(p, value.as_bytes());
    }
    fn array(&mut self, a: u64, data: u64, values: &[u64]) {
        self.word(a, data);
        self.number(a + 8, values.len() as i32);
        self.number(a + 12, values.len() as i32);
        for (i, value) in values.iter().enumerate() {
            self.word(data + i as u64 * 8, *value);
        }
    }
    fn rtti(&mut self, table: u64, name: &str, ordinal: u64) {
        let info = 0x6000000 + ordinal * 0x100;
        let text = 0x6100000 + ordinal * 0x100;
        self.word(table - 8, info);
        self.word(info + 8, text);
        self.put(text, format!("{}{name}\0", name.len()).as_bytes());
    }
    fn window(&mut self, p: u64, shown: bool) {
        self.word(p, BASE + 0x4347d58);
        self.word(p + 0x98, REGISTRY);
        self.put(p + 0xa5, &[if shown { 8 } else { 0 }]);
        self.number(p + 0xeb0, 0);
        self.number(p + 0xef8, 0);
    }
    fn mode(&mut self, mode: i32) {
        self.number(MANAGER + 0x3d8, mode);
        self.put(PANEL + 0xa5, &[if mode == 5 { 8 } else { 0 }]);
    }
    fn scope(&self) -> Scope {
        Scope {
            epoch: Epoch {
                game: self.id.clone(),
                campaign: format!("fixture:{PLAYTHROUGH}"),
                player: "GER".into(),
            },
            anchors: [GS, IDLER, DB],
            own_country: COUNTRY,
            own_status: STATUS,
        }
    }
    fn bind(&self, source: ManagedPanel) -> Result<(Anchor, Snapshot)> {
        Anchor::bind_with_clock(self, &self.scope(), source, self.until, &|| {
            self.clock.get()
        })
    }
    fn observe(&self, anchor: &mut Anchor) -> Result<Snapshot> {
        anchor.observe_with_clock(self, &|| self.clock.get())
    }
    fn unread(&self, a: u64, size: usize) {
        assert!(
            !self
                .reads
                .borrow()
                .iter()
                .any(|(p, n)| *p < a + size as u64 && a < *p + *n as u64),
            "payload unexpectedly read at {a:x}"
        );
    }
    fn complete_first(&mut self) {
        self.word(SLOT + 0x18, 0);
        self.number(TECHNOLOGY + 0x174, 1);
        self.array(STATUS + 0x40, 0x83000, &[TECHNOLOGY]);
    }
    fn new(mode: i32) -> Self {
        let start = Instant::now();
        let mut f = Self {
            data: BTreeMap::new(),
            reads: RefCell::new(Vec::new()),
            id: Identity {
                pid: 123,
                start_ticks: "456".into(),
                exe: "/fixture/owned-game".into(),
                uid: 1000,
            },
            image: BASE,
            abi: StringAbi::LibStdCxx,
            checks: Cell::new(0),
            fail_after_check: None,
            clock: Cell::new(start),
            until: start + Duration::from_secs(5),
            late_at: None,
            error_at: None,
            short_at: None,
            late_finish: false,
            mutate_finish: None,
        };
        for (global, p) in [
            (0x44086a8, GS),
            (0x44085f8, IDLER),
            (0x439f450, DB),
            (0x43f3eb8, 0x37000),
        ] {
            f.word(BASE + global, p);
        }
        f.word(GS, BASE + 0x42c7598);
        f.word(IDLER, BASE + 0x42c9a88);
        f.number(GS + 0x460, 60759371);
        f.number(GS + 0x4b4, 0);
        f.number(GS + 0x518, 1);
        f.number(GS + 0x620, 2);
        f.number(GS + 0x628, 1);
        f.string(GS + 0x718, PLAYTHROUGH, 0x60000);
        f.put(IDLER + 0x6c1, &[1]);
        f.word(GS + 0x340, 0x39000);
        f.number(GS + 0x348, 2);
        f.number(GS + 0x34c, 2);
        f.number(0x39000, -1);
        f.number(0x39004, 0);
        f.array(GS + 0x310, 0x3a000, &[COUNTRY]);
        f.word(0x37000, 0x38000);
        f.number(0x37008, 2);
        f.number(0x3700c, 2);
        f.string(0x38000, "---", 0);
        f.string(0x38020, "GER", 0);
        f.word(COUNTRY, 0x50000);
        f.rtti(0x50000, "CCountry", 0);
        f.string(COUNTRY + 0x10, "German Reich", 0);
        f.word(COUNTRY + 0x1348, 0x2a000);
        f.word(0x2a000, 0x50100);
        f.rtti(0x50100, "CNationalFocusProgress", 1);
        f.word(0x2a010, 0);
        f.word(COUNTRY + 0xf38, STATUS);
        f.word(STATUS, BASE + 0x42108f8);
        f.rtti(BASE + 0x42108f8, "CTechnologyStatus", 2);
        f.rtti(BASE + 0x4210898, "CResearchSlot", 3);
        f.rtti(BASE + 0x42107f8, "CTechnology", 4);
        f.rtti(BASE + 0x4211038, "CTechnologyTemplate", 5);
        f.word(STATUS + 0x130, COUNTRY);
        f.number(STATUS + 0x138, 1);
        let slots: Vec<_> = (0..4).map(|i| SLOT + i * 0x100).collect();
        let templates: Vec<_> = std::iter::once(0x62500)
            .chain((0..4).map(|i| TEMPLATE + i * 0x1000))
            .collect();
        let technologies: Vec<_> = (0..4).map(|i| TECHNOLOGY + i * 0x1000).collect();
        f.word(DB, BASE + 0x42110c8);
        f.word(0x62500, BASE + 0x4201c78);
        f.array(DB + 0x48, CATALOG_DATA, &templates);
        f.array(STATUS + 0x88, OWN_DATA, &technologies);
        f.array(STATUS + 0x40, 0, &[]);
        f.array(STATUS + 0xa0, SLOT_DATA, &slots);
        for i in 0..4 {
            let s = slots[i];
            let t = templates[i + 1];
            let tech = technologies[i];
            f.word(s, BASE + 0x4210898);
            f.word(s + 0x10, STATUS);
            f.word(s + 0x18, tech);
            f.word(tech, BASE + 0x42107f8);
            f.word(tech + 0x158, STATUS);
            f.word(tech + 0x160, t);
            f.number(tech + 0x174, 0);
            f.put(tech + 0x198, &0i64.to_le_bytes());
            f.word(t, BASE + 0x4211038);
            f.string(t + 8, &format!("owned_tech_{i}"), 0);
            f.number(t + 0x30, i as i32 + 1);
            f.number(t + 0x3cc, 1);
        }
        f.word(IDLER + 0x4f8, REGISTRY);
        f.word(IDLER + 0x6b8, MANAGER);
        f.word(BASE + 0x45778e0, REGISTRY);
        f.word(REGISTRY, BASE + 0x42736f8);
        f.word(MANAGER + 0x180, IDLER);
        f.word(MANAGER + 0xf0, VIEW);
        f.word(MANAGER + 0xf8, TREE);
        f.word(VIEW, BASE + 0x424dc38);
        f.word(TREE, BASE + 0x424de40);
        f.word(VIEW + 0x2f0, IDLER);
        f.word(TREE + 0x2f0, IDLER);
        f.number(TREE + 0xe24, 6);
        f.number(TREE + 0xe20, 1);
        f.number(VIEW + 0x300, 0);
        f.word(VIEW + 0x2f8, PANEL);
        f.word(TREE + 0x2f8, TREE_ROOT);
        f.word(VIEW + 0x910, PARENT);
        f.word(VIEW + 0x900, FACILITIES);
        f.word(FACILITIES, BASE + 0x42f2708);
        f.word(FACILITIES + 0x68, IDLER);
        f.word(FACILITIES + 0x60, PARENT);
        f.word(FACILITIES + 0x70, FACILITIES_ROOT);
        for root in [PANEL, TREE_ROOT, PARENT, FACILITIES_ROOT] {
            f.window(root, false);
        }
        f.array(PANEL + 0xbb0, 0x334000, &[PARENT]);
        f.word(PANEL + 0xbc8, 0x335000);
        f.number(PANEL + 0xbd0, 1);
        f.number(PANEL + 0xbd4, 1);
        f.string(0x335000, "facilities", 0);
        f.array(PARENT + 0xbb0, 0x336000, &[FACILITIES_ROOT]);
        f.word(PARENT + 0xbc8, 0x337000);
        f.number(PARENT + 0xbd0, 1);
        f.number(PARENT + 0xbd4, 1);
        f.string(0x337000, "facilities_view_Instance", 0x338000);
        f.word(TREE + 0xe08, 0);
        f.array(TREE + 0xd78, 0, &[]);
        f.array(TREE + 0xd90, 0, &[]);
        f.number(TREE + 0xe10, 0);
        f.array(TREE + 0xdc0, 0, &[]);
        f.mode(mode);
        f
    }
}
impl ReadMemory for Fixture {
    fn string_abi(&self) -> StringAbi {
        self.abi
    }
    fn read(&self, address: u64, size: usize) -> Result<Vec<u8>> {
        self.reads.borrow_mut().push((address, size));
        if self.late_at == Some(address) {
            self.clock.set(self.until);
        }
        if self.error_at == Some(address) {
            return Err(changed("Injected concrete original-owner error"));
        }
        let mut bytes: Vec<_> = (address..address + size as u64)
            .map(|p| {
                self.data.get(&p).copied().ok_or_else(|| {
                    Error::new("fixture_unmapped", format!("Sparse fixture forbids {p:x}"))
                })
            })
            .collect::<Result<_>>()?;
        if self.short_at == Some(address) {
            bytes.pop();
        }
        Ok(bytes)
    }
    fn read_many(&self, requests: &[(u64, usize)]) -> Result<Vec<Vec<u8>>> {
        crate::memory::validate_read_batch(requests)?;
        let mut result: Vec<_> = requests
            .iter()
            .map(|(p, n)| self.read(*p, *n))
            .collect::<Result<_>>()?;
        if let Some(address) = self.mutate_finish {
            for ((p, n), bytes) in requests.iter().zip(result.iter_mut()) {
                if address >= *p && address < *p + *n as u64 {
                    bytes[(address - p) as usize] ^= 1;
                }
            }
        }
        if self.late_finish {
            self.clock.set(self.until);
        }
        Ok(result)
    }
}
impl Process for Fixture {
    fn image_base(&self) -> u64 {
        self.image
    }
    fn identity(&self) -> Option<&Identity> {
        Some(&self.id)
    }
    fn check(&self) -> Result<()> {
        let n = self.checks.get() + 1;
        self.checks.set(n);
        if self.fail_after_check.is_some_and(|limit| n >= limit) {
            Err(changed("Injected original process check failed"))
        } else {
            Ok(())
        }
    }
}

fn goal(production: bool) -> Entry {
    Entry {
        id: "retained-goal".into(),
        intent: if production {
            Intent::Economy {
                target: EconomyTarget::Production {
                    line_id: "0x0000000100000038".into(),
                    equipment_id: "infantry_equipment_1".into(),
                    total_factories: 12,
                },
            }
        } else {
            Intent::Focus {
                focus_id: "GER_rhineland".into(),
            }
        },
        navigation: None,
        focus_search: None,
    }
}

#[test]
fn all_busy_slots_produce_typed_idle_and_research_without_assignment_target() {
    for mode in [23, 5] {
        let f = Fixture::new(mode);
        let (_, snapshot) = f
            .bind(if mode == 23 {
                ManagedPanel::Idle
            } else {
                ManagedPanel::Research
            })
            .unwrap();
        assert_eq!(snapshot.research().researching.len(), 4);
        assert!(snapshot.research().researching.iter().all(Option::is_some));
        assert_eq!(
            snapshot.panel(),
            if mode == 23 {
                ManagedPanel::Idle
            } else {
                ManagedPanel::Research
            }
        );
        assert_eq!(snapshot.scope(), &f.scope());
    }
}
#[test]
fn advancing_completion_reaches_research_while_retaining_focus_or_production_goal() {
    for production in [false, true] {
        let mut f = Fixture::new(23);
        let (mut anchor, original) = f.bind(ManagedPanel::Idle).unwrap();
        let entry = goal(production);
        let mut transit =
            crate::daily_panel_navigation::transit::Transit::bind(entry.clone(), original, f.until)
                .unwrap();
        f.mode(5);
        f.number(GS + 0x460, 60759395);
        f.complete_first();
        let sample = f.observe(&mut anchor).unwrap();
        assert_eq!(sample.research().researching[0], None);
        match transit
            .observe(&f.scope(), &entry, |_, _| Ok(sample))
            .unwrap()
        {
            crate::daily_panel_navigation::transit::Progress::AtResearch(proof) => {
                assert_eq!(proof.goal().entry(), &entry);
                assert_eq!(proof.deadline(), f.until);
                assert_eq!(proof.observed().context().date_raw, 60759395);
            }
            _ => panic!("Expected navigation evidence only"),
        }
    }
}
#[test]
fn unsupported_requested_source_and_abi_refuse_before_snapshot_or_reads() {
    for source in [
        ManagedPanel::Production,
        ManagedPanel::Politics,
        ManagedPanel::Focus,
    ] {
        let f = Fixture::new(23);
        assert!(f.bind(source).is_err());
        assert!(f.reads.borrow().is_empty());
        assert_eq!(f.checks.get(), 0);
    }
    let mut f = Fixture::new(23);
    f.abi = StringAbi::LibCxx;
    assert_eq!(
        f.bind(ManagedPanel::Idle).unwrap_err().code,
        "capability_unavailable"
    );
    assert!(f.reads.borrow().is_empty());
    assert_eq!(f.checks.get(), 0);
}
#[test]
fn unknown_observed_mode_refuses_before_views_slots_or_canonical_technology_payload() {
    let f = Fixture::new(99);
    assert_eq!(
        f.bind(ManagedPanel::Idle).unwrap_err().code,
        "research_transit_precondition"
    );
    for p in [VIEW, TREE, STATUS + 0xa0, TECHNOLOGY] {
        f.unread(p, 1);
    }
}
#[test]
fn supplied_known_source_must_match_current_mode() {
    let f = Fixture::new(5);
    assert_eq!(
        f.bind(ManagedPanel::Idle).unwrap_err().code,
        "research_transit_precondition"
    );
    f.unread(TECHNOLOGY, 1);
}
#[test]
fn expired_bind_and_replaced_process_or_base_refuse_before_any_read() {
    let f = Fixture::new(23);
    f.clock.set(f.until);
    assert_eq!(
        f.bind(ManagedPanel::Idle).unwrap_err().code,
        "panel_transit_deadline"
    );
    assert!(f.reads.borrow().is_empty());
    for image in [false, true] {
        let mut f = Fixture::new(23);
        let (mut anchor, _) = f.bind(ManagedPanel::Idle).unwrap();
        f.reads.borrow_mut().clear();
        if image {
            f.image += 0x1000;
        } else {
            f.id.start_ticks = "457".into();
        }
        assert_eq!(
            f.observe(&mut anchor).unwrap_err().code,
            "ownership_changed"
        );
        assert!(f.reads.borrow().is_empty());
    }
}
#[test]
fn invalid_image_range_refuses_without_reads() {
    for image in [0, 1, BASE + 1, u64::MAX, 0x7fff_ffff_0000] {
        let mut f = Fixture::new(23);
        f.image = image;
        assert!(f.bind(ManagedPanel::Idle).is_err());
        assert!(f.reads.borrow().is_empty());
    }
}
#[test]
fn original_campaign_roots_refuse_before_substituted_object_payload() {
    for global in [0x44086a8, 0x44085f8, 0x439f450] {
        let mut f = Fixture::new(23);
        let (mut anchor, _) = f.bind(ManagedPanel::Idle).unwrap();
        f.reads.borrow_mut().clear();
        f.word(BASE + global, POISON);
        assert_eq!(
            f.observe(&mut anchor).unwrap_err().code,
            "ownership_changed"
        );
        f.unread(POISON, 4096);
    }
}
#[test]
fn original_country_status_or_owner_replacement_stops_before_new_payload() {
    for address in [0x3a000, COUNTRY + 0xf38, STATUS + 0x130] {
        let mut f = Fixture::new(23);
        let (mut anchor, _) = f.bind(ManagedPanel::Idle).unwrap();
        f.reads.borrow_mut().clear();
        f.word(address, POISON);
        assert_eq!(
            f.observe(&mut anchor).unwrap_err().code,
            "ownership_changed"
        );
        f.unread(POISON, 4096);
        f.unread(TECHNOLOGY, 1);
    }
}
#[test]
fn original_ui_edges_reject_substitution_before_foreign_payload() {
    for address in [
        IDLER + 0x4f8,
        IDLER + 0x6b8,
        MANAGER + 0xf0,
        MANAGER + 0xf8,
        VIEW + 0x2f8,
        TREE + 0x2f8,
        VIEW + 0x910,
        VIEW + 0x900,
        FACILITIES + 0x70,
    ] {
        let mut f = Fixture::new(23);
        let (mut anchor, _) = f.bind(ManagedPanel::Idle).unwrap();
        f.reads.borrow_mut().clear();
        f.word(address, POISON);
        assert_eq!(
            f.observe(&mut anchor).unwrap_err().code,
            "ownership_changed"
        );
        f.unread(POISON, 4096);
        f.unread(TECHNOLOGY, 1);
    }
}
#[test]
fn wrong_primary_refuses_before_that_objects_owner_fields() {
    let mut f = Fixture::new(23);
    f.word(VIEW, BASE + 0x4347d58);
    assert_eq!(
        f.bind(ManagedPanel::Idle).unwrap_err().code,
        "invalid_gui_type"
    );
    f.unread(VIEW + 0x2f0, 8);
    f.unread(TECHNOLOGY, 1);
}
#[test]
fn original_slot_catalog_and_own_array_headers_refuse_before_backing_payload() {
    for header in [STATUS + 0xa0, DB + 0x48, STATUS + 0x88] {
        let mut f = Fixture::new(23);
        let (mut anchor, _) = f.bind(ManagedPanel::Idle).unwrap();
        f.reads.borrow_mut().clear();
        f.word(header, POISON);
        assert_eq!(
            f.observe(&mut anchor).unwrap_err().code,
            "ownership_changed"
        );
        f.unread(POISON, 4096);
        f.unread(TECHNOLOGY, 1);
    }
}
#[test]
fn original_membership_changes_stop_before_new_slot_template_or_technology() {
    for data in [SLOT_DATA, CATALOG_DATA + 8, OWN_DATA] {
        let mut f = Fixture::new(23);
        let (mut anchor, _) = f.bind(ManagedPanel::Idle).unwrap();
        f.reads.borrow_mut().clear();
        f.word(data, POISON);
        assert_eq!(
            f.observe(&mut anchor).unwrap_err().code,
            "ownership_changed"
        );
        f.unread(POISON, 4096);
        f.unread(TECHNOLOGY, 1);
    }
}
#[test]
fn empty_duplicate_or_over_policy_slots_refuse() {
    for case in 0..3 {
        let mut f = Fixture::new(23);
        match case {
            0 => f.array(STATUS + 0xa0, 0, &[]),
            1 => f.word(SLOT_DATA + 8, SLOT),
            _ => {
                f.number(STATUS + 0xa8, 33);
                f.number(STATUS + 0xac, 33);
            }
        }
        assert!(f.bind(ManagedPanel::Idle).is_err());
        f.unread(TECHNOLOGY, 1);
    }
}
#[test]
fn changed_named_attachment_cannot_redirect_facilities_payload() {
    let mut f = Fixture::new(23);
    let (mut anchor, _) = f.bind(ManagedPanel::Idle).unwrap();
    f.reads.borrow_mut().clear();
    f.word(0x334000, POISON);
    assert_eq!(
        f.observe(&mut anchor).unwrap_err().code,
        "research_precondition"
    );
    f.unread(POISON, 4096);
}
#[test]
fn hidden_info_observation_never_follows_its_technology_or_callback_payload() {
    let mut f = Fixture::new(23);
    f.array(TREE + 0xdc0, 0x400000, &[0x401000]);
    f.word(0x401000, BASE + 0x430bbf0);
    f.word(0x401a78, IDLER);
    f.word(0x401a80, TREE);
    f.word(0x401a88, 0x402000);
    f.window(0x402000, false);
    f.bind(ManagedPanel::Idle).unwrap();
    f.unread(0x401a98, 8);
    f.unread(0x401ab0, 24);
    f.put(0x4020a5, &[8]);
    assert_eq!(
        f.bind(ManagedPanel::Idle).unwrap_err().code,
        "research_transit_precondition"
    );
    f.unread(0x401a98, 8);
}
#[test]
fn foreign_info_owner_refuses_before_container_pointer_or_payload() {
    let mut f = Fixture::new(23);
    f.array(TREE + 0xdc0, 0x400000, &[0x401000]);
    f.word(0x401000, BASE + 0x430bbf0);
    f.word(0x401a78, POISON);
    assert_eq!(
        f.bind(ManagedPanel::Idle).unwrap_err().code,
        "ownership_changed"
    );
    f.unread(0x401a88, 8);
    f.unread(POISON, 4096);
}
#[test]
fn visible_tree_facilities_animation_and_mode_visibility_mismatch_refuse() {
    for root in [TREE_ROOT, FACILITIES_ROOT] {
        let mut f = Fixture::new(23);
        f.put(root + 0xa5, &[8]);
        assert!(f.bind(ManagedPanel::Idle).is_err());
    }
    for root in [PANEL, TREE_ROOT, PARENT, FACILITIES_ROOT] {
        let mut f = Fixture::new(23);
        f.number(root + 0xeb0, 1);
        assert_eq!(
            f.bind(ManagedPanel::Idle).unwrap_err().code,
            "research_panel_transition"
        );
    }
    let mut f = Fixture::new(5);
    f.put(PANEL + 0xa5, &[0]);
    assert_eq!(
        f.bind(ManagedPanel::Research).unwrap_err().code,
        "research_panel_transition"
    );
}
#[test]
fn foreign_display_and_nonordinary_tab_refuse_before_canonical_body() {
    for (address, value) in [(TREE + 0xe20, 2), (VIEW + 0x300, 1)] {
        let mut f = Fixture::new(23);
        f.number(address, value);
        assert!(f.bind(ManagedPanel::Idle).is_err());
        f.unread(TECHNOLOGY, 1);
    }
}
#[test]
fn final_witness_changes_reject_without_advancing_chronology_highwater() {
    for address in [GS + 0x460, STATUS + 0xa0, VIEW + 0x2f8, SLOT + 0x18] {
        let mut f = Fixture::new(23);
        let (mut anchor, _) = f.bind(ManagedPanel::Idle).unwrap();
        f.mutate_finish = Some(address);
        assert_eq!(f.observe(&mut anchor).unwrap_err().code, "torn_read");
        assert_eq!(anchor.last_date, 60759371);
        f.mutate_finish = None;
        f.observe(&mut anchor).unwrap();
    }
}
#[test]
fn advancing_dates_are_monotonic_not_exact_hours() {
    let mut f = Fixture::new(23);
    let (mut anchor, _) = f.bind(ManagedPanel::Idle).unwrap();
    for date in [60759371, 60759372, 60759419] {
        f.number(GS + 0x460, date);
        assert_eq!(f.observe(&mut anchor).unwrap().context().date_raw, date);
    }
    f.number(GS + 0x460, 60759395);
    assert_eq!(
        f.observe(&mut anchor).unwrap_err().code,
        "ownership_changed"
    );
}
#[test]
fn changed_setup_playthrough_and_process_checks_cannot_publish_snapshot() {
    for (address, value) in [(GS + 0x620, 3), (GS + 0x628, 0)] {
        let mut f = Fixture::new(23);
        let (mut anchor, _) = f.bind(ManagedPanel::Idle).unwrap();
        f.number(address, value);
        assert_eq!(
            f.observe(&mut anchor).unwrap_err().code,
            "ownership_changed"
        );
    }
    let mut f = Fixture::new(23);
    let (mut anchor, _) = f.bind(ManagedPanel::Idle).unwrap();
    f.put(0x60000, b"b");
    assert_eq!(
        f.observe(&mut anchor).unwrap_err().code,
        "ownership_changed"
    );
}
#[test]
fn late_success_is_discarded_and_late_concrete_error_keeps_its_cause() {
    let mut f = Fixture::new(23);
    f.late_at = Some(MANAGER + 0x3d8);
    assert_eq!(
        f.bind(ManagedPanel::Idle).unwrap_err().code,
        "panel_transit_deadline"
    );
    let mut f = Fixture::new(23);
    f.late_at = Some(BASE + 0x44086a8);
    f.error_at = f.late_at;
    let error = f.bind(ManagedPanel::Idle).unwrap_err();
    assert_eq!(error.code, "ownership_changed");
    assert!(error.message.contains("Injected concrete"));
}
#[test]
fn original_deadline_covers_final_batch_and_is_never_renewed() {
    let mut f = Fixture::new(23);
    let (mut anchor, _) = f.bind(ManagedPanel::Idle).unwrap();
    let original = anchor.deadline();
    f.late_finish = true;
    assert_eq!(
        f.observe(&mut anchor).unwrap_err().code,
        "panel_transit_deadline"
    );
    assert_eq!(anchor.deadline(), original);
    f.late_finish = false;
    f.reads.borrow_mut().clear();
    assert_eq!(
        f.observe(&mut anchor).unwrap_err().code,
        "panel_transit_deadline"
    );
    assert!(f.reads.borrow().is_empty());
}
#[test]
fn short_reads_refuse_without_partial_snapshot() {
    let mut f = Fixture::new(23);
    f.short_at = Some(BASE + 0x44086a8);
    assert_eq!(f.bind(ManagedPanel::Idle).unwrap_err().code, "short_read");
}
#[test]
fn canonical_typed_helper_and_legacy_body_keep_identical_slot_meaning() {
    let f = Fixture::new(23);
    let witnesses = Witnesses::new(&f);
    let r = Reader::new(&witnesses);
    let (typed_context, typed) = research::navigation_from_reader(&r, BASE).unwrap();
    let (json, legacy_context) = research::observe_from_reader(&r, BASE, 0, 1, true).unwrap();
    assert_eq!(typed_context.own, legacy_context.unwrap().own);
    let expected: Vec<_> = (0..4).map(|i| Some(format!("owned_tech_{i}"))).collect();
    assert_eq!(typed, expected);
    for (i, key) in typed.iter().enumerate() {
        assert_eq!(json["slots"][i]["researching"].as_str(), key.as_deref());
    }
    witnesses.finish().unwrap();
}

#[test]
fn current_active_or_completed_foreign_pointer_refuses_before_technology_payload() {
    for completed in [false, true] {
        let mut f = Fixture::new(23);
        let (mut anchor, _) = f.bind(ManagedPanel::Idle).unwrap();
        f.reads.borrow_mut().clear();
        if completed {
            f.array(STATUS + 0x40, 0x83000, &[POISON]);
        } else {
            f.word(SLOT + 0x18, POISON);
        }
        assert_eq!(
            f.observe(&mut anchor).unwrap_err().code,
            "ownership_changed"
        );
        f.unread(POISON, 4096);
    }
}
#[test]
fn original_slot_and_technology_owners_precede_forward_payload() {
    for slot in [false, true] {
        let mut f = Fixture::new(23);
        let (mut anchor, _) = f.bind(ManagedPanel::Idle).unwrap();
        f.reads.borrow_mut().clear();
        if slot {
            f.word(SLOT + 0x10, POISON);
        } else {
            f.word(TECHNOLOGY + 0x158, POISON);
        }
        assert_eq!(
            f.observe(&mut anchor).unwrap_err().code,
            "ownership_changed"
        );
        if slot {
            f.unread(SLOT + 0x18, 8);
        } else {
            f.unread(TECHNOLOGY + 0x160, 8);
        }
        f.unread(POISON, 4096);
    }
}
#[test]
fn substituted_template_refuses_before_new_template_primary_or_string() {
    let mut f = Fixture::new(23);
    let (mut anchor, _) = f.bind(ManagedPanel::Idle).unwrap();
    f.reads.borrow_mut().clear();
    f.word(TECHNOLOGY + 0x160, POISON);
    assert_eq!(
        f.observe(&mut anchor).unwrap_err().code,
        "ownership_changed"
    );
    f.unread(POISON, 4096);
}
#[test]
fn final_process_check_failure_discards_the_typed_sample() {
    let mut f = Fixture::new(23);
    f.fail_after_check = Some(2);
    assert_eq!(
        f.bind(ManagedPanel::Idle).unwrap_err().code,
        "ownership_changed"
    );
    assert_eq!(f.checks.get(), 2);
}
