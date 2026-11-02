#![cfg(unix)]

#[path = "support/mac_construction_panel.rs"]
mod construction_panel;
#[path = "support/mac_construction_target.rs"]
mod construction_target;
#[path = "support/mac_construction_ui.rs"]
mod construction_ui;

use hoi4_live_native::{
    actions::Epoch,
    daily_governor::{Entry, Governor, Intent, Plan, Scope},
    daily_observation::Producer,
    error::{Error, Result},
    game,
    memory::{ReadMemory, StringAbi},
    platform::{Identity, Process},
};
use serde_json::Value;
use std::{
    cell::{Cell, RefCell},
    collections::BTreeMap,
    fs,
    os::unix::fs::PermissionsExt,
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
    time::{Duration, Instant},
};

const BASE: u64 = 0x1_0000_0000;
const GS: u64 = 0x100000;
const IDLER: u64 = 0x110000;
const COUNTRY: u64 = 0x120000;
const STATUS: u64 = 0x130000;
const DATABASE: u64 = 0x140000;
const FOCUS: u64 = 0x150000;
const SLOT: u64 = 0x160000;
const TEMPLATE: u64 = 0x170000;
const TECHNOLOGY: u64 = 0x180000;
const FOREIGN: u64 = 0xdead0000;
const DATE: i32 = 60759371;

/// Generated Mac canonical memory only. Linux roots and every GUI range are
/// deliberately absent; this fixture never opens a process or sends input.
struct MacFixture {
    data: BTreeMap<u64, u8>,
    id: Identity,
    reads: RefCell<Vec<(u64, usize)>>,
    counts: RefCell<BTreeMap<(u64, usize), usize>>,
    final_fault: Cell<Option<u64>>,
    finishing: Cell<bool>,
    repeated_pointer: Option<(u64, u64)>,
}
impl MacFixture {
    fn put(&mut self, address: u64, bytes: &[u8]) {
        self.data.extend(
            bytes
                .iter()
                .enumerate()
                .map(|(i, byte)| (address + i as u64, *byte)),
        );
    }
    fn word(&mut self, address: u64, value: u64) {
        self.put(address, &value.to_le_bytes());
    }
    fn int(&mut self, address: u64, value: i32) {
        self.put(address, &value.to_le_bytes());
    }
    fn array(&mut self, address: u64, data: u64, entries: &[u64]) {
        self.word(address, data);
        self.int(address + 8, entries.len() as i32);
        self.int(address + 12, entries.len() as i32);
        for (i, entry) in entries.iter().enumerate() {
            self.word(data + i as u64 * 8, *entry);
        }
    }
    fn string(&mut self, address: u64, value: &str) {
        assert!(value.len() <= 22);
        let mut bytes = [0; 24];
        bytes[0] = (value.len() * 2) as u8;
        bytes[1..1 + value.len()].copy_from_slice(value.as_bytes());
        self.put(address, &bytes);
    }
    fn rtti(&mut self, vtable: u64, name: &str, index: u64) {
        let info = 0x200000 + index * 0x100;
        self.word(BASE + vtable - 8, info);
        self.word(info + 8, info + 0x40);
        self.put(info + 0x40, format!("{}{name}\0", name.len()).as_bytes());
    }
    fn new() -> Self {
        let mut f = Self {
            data: BTreeMap::new(),
            id: Identity {
                pid: 120,
                start_ticks: "1792677600.000123".into(),
                exe: "/generated/hoi4.app/Contents/MacOS/hoi4".into(),
                uid: 501,
            },
            reads: RefCell::new(Vec::new()),
            counts: RefCell::new(BTreeMap::new()),
            final_fault: Cell::new(None),
            finishing: Cell::new(false),
            repeated_pointer: None,
        };
        for (address, value) in [
            (BASE + 0x372b040, GS),
            (BASE + 0x372afc0, IDLER),
            (BASE + 0x3706540, DATABASE),
            (BASE + 0x3717d50, 0x196000),
            (GS, BASE + 0x35044e8),
            (IDLER, BASE + 0x35089d8),
            (COUNTRY, BASE + 0x357e388),
            (COUNTRY + 0xd08, STATUS),
            (COUNTRY + 0x1108, FOCUS),
            (FOCUS, BASE + 0x3589270),
            (FOCUS + 0x10, 0),
            (STATUS, BASE + 0x35a7258),
            (STATUS + 0x130, COUNTRY),
            (DATABASE, BASE + 0x35a7d10),
        ] {
            f.word(address, value);
        }
        f.int(GS + 0x428, DATE);
        f.int(GS + 0x47c, 0);
        f.int(GS + 0x4d8, 1);
        f.int(GS + 0x5d0, 2);
        f.int(GS + 0x5d8, 1);
        f.put(IDLER + 0x679, &[1]);
        f.int(STATUS + 0x138, 1);
        f.word(GS + 0x308, 0x194000);
        f.int(GS + 0x310, 2);
        f.int(GS + 0x314, 2);
        f.int(0x194000, -1);
        f.int(0x194004, 0);
        f.array(GS + 0x2d8, 0x195000, &[COUNTRY]);
        f.array(0x196000, 0x196100, &[0, 0]);
        f.string(0x196100, "---");
        f.string(0x196118, "GER");
        f.string(COUNTRY + 0x10, "German Reich");
        let playthrough = "aabbccdd-1122-4344-8566-778899aabbcc";
        f.word(GS + 0x6c8, playthrough.len() as u64 | 1);
        f.word(GS + 0x6d0, playthrough.len() as u64);
        f.word(GS + 0x6d8, 0x197000);
        f.put(0x197000, playthrough.as_bytes());

        f.array(
            DATABASE + 0x40,
            0x193000,
            &[0x171000, TEMPLATE, TEMPLATE + 0x400],
        );
        f.array(STATUS + 0x88, 0x190000, &[TECHNOLOGY, TECHNOLOGY + 0x400]);
        f.array(STATUS + 0x40, 0x191000, &[TECHNOLOGY + 0x400]);
        f.array(STATUS + 0xa0, 0x192000, &[SLOT, SLOT + 0x100]);
        for (i, key) in ["basic_machine_tools", "construction1"].iter().enumerate() {
            let technology = TECHNOLOGY + i as u64 * 0x400;
            let template = TEMPLATE + i as u64 * 0x400;
            f.word(template, BASE + 0x35a7c80);
            f.string(template + 8, key);
            f.int(template + 0x28, i as i32 + 1);
            f.int(template + 0x39c, 1);
            f.word(technology, BASE + 0x35a7158);
            f.word(technology + 0x158, STATUS);
            f.word(technology + 0x160, template);
            f.int(technology + 0x174, i as i32);
            f.word(technology + 0x198, 123);
        }
        for (i, current) in [TECHNOLOGY, 0].into_iter().enumerate() {
            let slot = SLOT + i as u64 * 0x100;
            f.word(slot, BASE + 0x35a71f8);
            f.word(slot + 0x10, STATUS);
            f.word(slot + 0x18, current);
        }
        for (i, (table, name)) in [
            (0x357e388, "CCountry"),
            (0x3589270, "CNationalFocusProgress"),
            (0x35a7258, "CTechnologyStatus"),
            (0x35a71f8, "CResearchSlot"),
            (0x35a7158, "CTechnology"),
            (0x35a7c80, "CTechnologyTemplate"),
        ]
        .into_iter()
        .enumerate()
        {
            f.rtti(table, name, i as u64);
        }
        f
    }
    fn epoch(&self) -> Epoch {
        Epoch {
            game: self.id.clone(),
            campaign: "fixture:aabbccdd-1122-4344-8566-778899aabbcc".into(),
            player: "GER".into(),
        }
    }
    fn clear_reads(&self) {
        self.reads.borrow_mut().clear();
        self.counts.borrow_mut().clear();
    }
    fn assert_no_foreign_payload(&self) {
        assert!(!self
            .reads
            .borrow()
            .iter()
            .any(|(address, _)| (FOREIGN..FOREIGN + 0x2000).contains(address)));
    }
}
impl ReadMemory for MacFixture {
    fn string_abi(&self) -> StringAbi {
        StringAbi::LibCxx
    }
    fn read(&self, address: u64, size: usize) -> Result<Vec<u8>> {
        self.reads.borrow_mut().push((address, size));
        let repeated = {
            let mut counts = self.counts.borrow_mut();
            let count = counts.entry((address, size)).or_default();
            *count += 1;
            *count > 1
        };
        if let Some((field, replacement)) = self.repeated_pointer {
            if address == field && size == 8 && repeated {
                return Ok(replacement.to_le_bytes().to_vec());
            }
        }
        let mut bytes: Vec<_> = (0..size)
            .map(|i| {
                self.data
                    .get(&(address + i as u64))
                    .copied()
                    .ok_or_else(|| {
                        Error::new(
                            "fixture_unmapped",
                            format!("Generated Mac fixture forbids {address:x}/{size}"),
                        )
                    })
            })
            .collect::<Result<_>>()?;
        if self.finishing.get() {
            if let Some(fault) = self.final_fault.get() {
                if (address..address + size as u64).contains(&fault) {
                    bytes[(fault - address) as usize] ^= 1;
                }
            }
        }
        Ok(bytes)
    }
    fn read_many(&self, requests: &[(u64, usize)]) -> Result<Vec<Vec<u8>>> {
        self.finishing.set(true);
        let result = requests.iter().map(|(a, n)| self.read(*a, *n)).collect();
        self.finishing.set(false);
        result
    }
}
impl Process for MacFixture {
    fn check(&self) -> Result<()> {
        Ok(())
    }
    fn begin_snapshot(&self) -> Result<()> {
        self.counts.borrow_mut().clear();
        self.check()
    }
    fn image_base(&self) -> u64 {
        BASE
    }
    fn identity(&self) -> Option<&Identity> {
        Some(&self.id)
    }
}

struct Scratch(PathBuf);
impl Scratch {
    fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let path = std::env::temp_dir().join(format!(
            "mac-daily-observation-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path).unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o700)).unwrap();
        Self(path)
    }
    fn path(&self) -> PathBuf {
        self.0.join("sequence.json")
    }
}
impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
fn until() -> Instant {
    Instant::now() + Duration::from_secs(2)
}
fn open(f: &MacFixture, scratch: &Scratch) -> Producer {
    Producer::open(
        f,
        f.epoch(),
        game::clock_context(f).unwrap(),
        &scratch.path(),
        until(),
    )
    .unwrap()
}
fn entry() -> Entry {
    Entry {
        id: "research-next".into(),
        intent: Intent::Research {
            technology_id: "next_technology".into(),
            slot: 0,
        },
        navigation: None,
        focus_search: None,
    }
}
fn plan(scope: Scope) -> Plan {
    Plan {
        version: 1,
        revision: 1,
        scope,
        entries: vec![entry()],
        max_actions_per_day: 2,
        max_observation_age_ms: 5000,
        review_day: None,
    }
}

#[test]
fn mac_producer_persists_exact_scope_sequence_and_unknown_counts() {
    let f = MacFixture::new();
    let scratch = Scratch::new();
    let producer = open(&f, &scratch);
    Governor::open(
        &scratch.0.join("plan.json"),
        plan(producer.scope().unwrap()),
    )
    .unwrap();
    let observed = producer.observe(&f, until()).unwrap();
    assert_eq!(observed.observation.sequence, 1);
    assert_eq!(observed.observation.date_raw, DATE);
    assert_eq!(observed.observation.scope.epoch, f.epoch());
    assert_eq!(observed.observation.scope.anchors, [GS, IDLER, DATABASE]);
    assert_eq!(observed.context.own, [COUNTRY, STATUS]);
    assert_eq!(observed.context.slots, [SLOT, SLOT + 0x100]);
    assert_eq!(observed.observation.counts.idle_research_slots, Some(1));
    assert_eq!(observed.observation.counts.idle_civilian_factories, None);
    assert_eq!(observed.observation.counts.idle_military_factories, None);
    assert_eq!(
        observed.canonical["slots"][0]["researching"],
        "basic_machine_tools"
    );
    assert_eq!(observed.canonical["researched"][0]["id"], "construction1");
    let saved = fs::read(scratch.path()).unwrap();
    for _ in 0..2 {
        assert_eq!(producer.probe_clock(&f, until()).unwrap(), DATE);
        assert!(producer
            .probe_research(&f, until())
            .unwrap()
            .all_heads_occupied(&[entry()])
            .unwrap());
    }
    assert_eq!(fs::read(scratch.path()).unwrap(), saved);
    drop(producer);
    let producer = open(&f, &scratch);
    assert_eq!(
        producer.observe(&f, until()).unwrap().observation.sequence,
        2
    );
    let stored: Value = serde_json::from_slice(&fs::read(scratch.path()).unwrap()).unwrap();
    assert_eq!(
        stored["scope"]["epoch"]["game"]["start_ticks"],
        f.id.start_ticks
    );
}

#[test]
fn changed_mac_roots_types_and_foreign_owners_refuse_before_payload() {
    for (field, replacement) in [
        (BASE + 0x372b040, FOREIGN),
        (BASE + 0x372afc0, FOREIGN),
        (BASE + 0x3706540, FOREIGN),
        (GS, BASE + 0x35044f0),
        (IDLER, BASE + 0x35089e0),
        (DATABASE, BASE + 0x35a7d18),
        (0x195000, FOREIGN),
        (COUNTRY + 0xd08, FOREIGN),
        (STATUS, BASE + 0x35a7260),
        (STATUS + 0x130, FOREIGN),
    ] {
        let mut f = MacFixture::new();
        let scratch = Scratch::new();
        let producer = open(&f, &scratch);
        let saved = fs::read(scratch.path()).unwrap();
        f.word(field, replacement);
        f.clear_reads();
        assert_eq!(
            producer.probe_clock(&f, until()).unwrap_err().code,
            "ownership_changed",
            "{field:x}"
        );
        assert_eq!(
            producer.observe(&f, until()).err().unwrap().code,
            "ownership_changed",
            "{field:x}"
        );
        f.assert_no_foreign_payload();
        assert_eq!(fs::read(scratch.path()).unwrap(), saved);
    }
}

#[test]
fn repeated_owner_pointer_change_is_torn_before_new_status_is_followed() {
    let mut f = MacFixture::new();
    let scratch = Scratch::new();
    let producer = open(&f, &scratch);
    let saved = fs::read(scratch.path()).unwrap();
    f.repeated_pointer = Some((COUNTRY + 0xd08, FOREIGN));
    f.clear_reads();
    assert_eq!(
        producer.observe(&f, until()).err().unwrap().code,
        "torn_read"
    );
    f.assert_no_foreign_payload();
    assert_eq!(fs::read(scratch.path()).unwrap(), saved);
}

#[test]
fn mac_canonical_foreign_slots_and_technologies_refuse_before_template_payload() {
    for (field, template) in [
        (SLOT + 0x10, TEMPLATE),
        (TECHNOLOGY + 0x158, TEMPLATE),
        (TECHNOLOGY + 0x400 + 0x158, TEMPLATE + 0x400),
    ] {
        let mut f = MacFixture::new();
        let scratch = Scratch::new();
        let producer = open(&f, &scratch);
        let saved = fs::read(scratch.path()).unwrap();
        f.word(field, FOREIGN);
        f.clear_reads();
        assert_eq!(
            producer.observe(&f, until()).err().unwrap().code,
            "invalid_research_owner",
            "{field:x}"
        );
        f.assert_no_foreign_payload();
        assert!(!f
            .reads
            .borrow()
            .iter()
            .any(|(address, _)| *address == template + 8));
        assert_eq!(fs::read(scratch.path()).unwrap(), saved);
    }
}

#[test]
fn mac_final_witnesses_cover_roots_owners_arrays_keys_and_counters() {
    for address in [
        BASE + 0x372b040,
        BASE + 0x372afc0,
        BASE + 0x3706540,
        GS,
        IDLER,
        DATABASE,
        GS + 0x428,
        GS + 0x4d8,
        GS + 0x308,
        0x194004,
        GS + 0x2d8,
        0x195000,
        COUNTRY,
        BASE + 0x357e388 - 8,
        0x200008,
        0x200040,
        COUNTRY + 0xd08,
        STATUS,
        STATUS + 0x130,
        STATUS + 0x138,
        STATUS + 0x40,
        STATUS + 0x88,
        STATUS + 0xa0,
        DATABASE + 0x40,
        0x190000,
        0x191000,
        0x192000,
        0x193008,
        SLOT + 0x10,
        SLOT + 0x18,
        TECHNOLOGY + 0x158,
        TECHNOLOGY + 0x174,
        TECHNOLOGY + 0x198,
        TEMPLATE + 8,
        TEMPLATE + 0x28,
        TEMPLATE + 0x39c,
        TEMPLATE + 0x400 + 8,
        0x197000,
        0x196118,
    ] {
        let f = MacFixture::new();
        let scratch = Scratch::new();
        let producer = open(&f, &scratch);
        let saved = fs::read(scratch.path()).unwrap();
        f.final_fault.set(Some(address));
        assert_eq!(
            producer.observe(&f, until()).err().unwrap().code,
            "torn_read",
            "{address:x}"
        );
        assert_eq!(fs::read(scratch.path()).unwrap(), saved);
        f.final_fault.set(None);
        assert_eq!(
            producer.observe(&f, until()).unwrap().observation.sequence,
            1
        );
    }
}

#[test]
fn mac_clock_probe_finishes_its_scope_witnesses_without_publishing() {
    let f = MacFixture::new();
    let scratch = Scratch::new();
    let producer = open(&f, &scratch);
    let saved = fs::read(scratch.path()).unwrap();
    for field in [DATABASE, COUNTRY + 0xd08, STATUS, 0x195000, GS + 0x428] {
        f.final_fault.set(Some(field));
        assert_eq!(
            producer.probe_clock(&f, until()).unwrap_err().code,
            "torn_read"
        );
        assert_eq!(fs::read(scratch.path()).unwrap(), saved);
    }
}

#[test]
fn mac_sequence_restart_refuses_regression_and_preserves_exact_process_scope() {
    let mut f = MacFixture::new();
    let scratch = Scratch::new();
    let producer = open(&f, &scratch);
    assert_eq!(
        producer.observe(&f, until()).unwrap().observation.sequence,
        1
    );
    f.int(GS + 0x428, DATE + 1);
    assert_eq!(
        producer.observe(&f, until()).unwrap().observation.sequence,
        2
    );
    let saved = fs::read(scratch.path()).unwrap();
    drop(producer);
    f.int(GS + 0x428, DATE);
    assert_eq!(
        Producer::open(
            &f,
            f.epoch(),
            game::clock_context(&f).unwrap(),
            &scratch.path(),
            until(),
        )
        .err()
        .unwrap()
        .code,
        "ownership_changed"
    );
    assert_eq!(fs::read(scratch.path()).unwrap(), saved);
    f.int(GS + 0x428, DATE + 1);
    f.id.start_ticks = "1792677600.000124".into();
    assert_eq!(
        Producer::open(
            &f,
            f.epoch(),
            game::clock_context(&f).unwrap(),
            &scratch.path(),
            until(),
        )
        .err()
        .unwrap()
        .code,
        "ownership_changed"
    );
    assert_eq!(fs::read(scratch.path()).unwrap(), saved);
    f.id.start_ticks = "1792677600.000123".into();
    let producer = open(&f, &scratch);
    assert_eq!(
        producer.observe(&f, until()).unwrap().observation.sequence,
        3
    );
}

#[test]
fn mac_probe_chronology_and_exact_identity_refuse_before_sequence_advance() {
    let mut f = MacFixture::new();
    let scratch = Scratch::new();
    let producer = open(&f, &scratch);
    let saved = fs::read(scratch.path()).unwrap();
    f.int(GS + 0x428, DATE + 1);
    assert_eq!(producer.probe_clock(&f, until()).unwrap(), DATE + 1);
    f.int(GS + 0x428, DATE);
    assert_eq!(
        producer.observe(&f, until()).err().unwrap().code,
        "ownership_changed"
    );
    assert_eq!(fs::read(scratch.path()).unwrap(), saved);
    f.id.start_ticks = "1792677600.000124".into();
    f.clear_reads();
    assert_eq!(
        producer.observe(&f, until()).err().unwrap().code,
        "ownership_changed"
    );
    assert!(f.reads.borrow().is_empty());
    assert_eq!(fs::read(scratch.path()).unwrap(), saved);
}

#[test]
fn scope_accepts_canonical_mac_and_preserves_existing_linux_ticks_verbatim() {
    let f = MacFixture::new();
    let scratch = Scratch::new();
    let producer = open(&f, &scratch);
    let original = producer.scope().unwrap();
    for (index, start) in [
        "0",
        "000456",
        "18446744073709551616",
        "12345678901234567890123456789012",
        "0.000000",
        "1.000001",
        "1792677600.000123",
        "18446744073709551615.999999",
    ]
    .into_iter()
    .enumerate()
    {
        let mut scope = original.clone();
        scope.epoch.game.start_ticks = start.into();
        Governor::open(
            &scratch.0.join(format!("valid-{index}.json")),
            plan(scope.clone()),
        )
        .unwrap();
        let roundtrip: Scope =
            serde_json::from_value(serde_json::to_value(&scope).unwrap()).unwrap();
        assert_eq!(roundtrip, scope);
    }
    let mut one = original.clone();
    one.epoch.game.start_ticks = "000456".into();
    let mut another = one.clone();
    another.epoch.game.start_ticks = "456".into();
    assert_ne!(one, another);
    let path = scratch.0.join("exact-scope.json");
    drop(Governor::open(&path, plan(one)).unwrap());
    let saved = fs::read(&path).unwrap();
    assert_eq!(
        Governor::open(&path, plan(another)).err().unwrap().code,
        "plan_conflict"
    );
    assert_eq!(fs::read(&path).unwrap(), saved);
}

#[test]
fn scope_refuses_malformed_ambiguous_or_overflowing_mac_start_timestamps() {
    let f = MacFixture::new();
    let scratch = Scratch::new();
    let producer = open(&f, &scratch);
    let original = producer.scope().unwrap();
    for start in [
        "",
        "123456789012345678901234567890123",
        ".000001",
        "1.",
        "1.1",
        "1.00001",
        "1.0000000",
        "1.1000000",
        "1..000001",
        "1.000001.0",
        "01.000001",
        "00.000000",
        "+1.000001",
        "-1.000001",
        "1.-00001",
        "1.+00001",
        " 1.000001",
        "1.000001 ",
        "1.000001\n",
        "1e9.000001",
        "1_000.000001",
        "1,000001",
        "1.０００００１",
        "１.000001",
        "18446744073709551616.000000",
    ] {
        let mut scope = original.clone();
        scope.epoch.game.start_ticks = start.into();
        assert_eq!(
            Governor::open(&scratch.0.join("invalid.json"), plan(scope))
                .err()
                .unwrap()
                .code,
            "invalid_plan",
            "{start:?}"
        );
        assert!(!scratch.0.join("invalid.json").exists());
    }
}

#[test]
fn invalid_mac_process_timestamp_cannot_create_a_sequence_record() {
    let mut f = MacFixture::new();
    let scratch = Scratch::new();
    f.id.start_ticks = "1792677600.0001230".into();
    assert_eq!(
        Producer::open(
            &f,
            f.epoch(),
            game::clock_context(&f).unwrap(),
            &scratch.path(),
            until(),
        )
        .err()
        .unwrap()
        .code,
        "invalid_plan"
    );
    assert!(!scratch.path().exists());
}
