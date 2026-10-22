use super::*;
use crate::{game::ClockContext, memory::ReadMemory};
use std::{cell::Cell, collections::BTreeMap};
const BASE: u64 = 0x10000;
const GS: u64 = 0x100000;
const IDLER: u64 = 0x110000;
const COUNTRY: u64 = 0x120000;
const STATUS: u64 = 0x130000;
const REG: u64 = 0x140000;
const MANAGER: u64 = 0x150000;
const VIEW: u64 = 0x160000;
const TREE: u64 = 0x170000;
const ROOT: u64 = 0x180000;
#[derive(Default)]
struct Fixture {
    bytes: BTreeMap<u64, u8>,
    changed: Option<u64>,
    reads: Cell<usize>,
    other_abi: bool,
}
impl Fixture {
    fn put(&mut self, p: u64, b: &[u8]) {
        self.bytes
            .extend(b.iter().enumerate().map(|(i, b)| (p + i as u64, *b)));
    }
    fn word(&mut self, p: u64, v: u64) {
        self.put(p, &v.to_le_bytes());
    }
    fn number(&mut self, p: u64, v: i32) {
        self.put(p, &v.to_le_bytes());
    }
    fn new(states: [i32; 2], shown: bool) -> Self {
        let mut f = Self::default();
        f.word(IDLER + 0x4f8, REG);
        f.word(IDLER + 0x6b8, MANAGER);
        f.word(REG, BASE + 0x42736f8);
        f.word(MANAGER + 0x180, IDLER);
        f.word(BASE + 0x45778e0, REG);
        f.word(MANAGER + 0xf0, VIEW);
        f.word(MANAGER + 0xf8, TREE);
        f.word(VIEW, BASE + 0x424dc38);
        f.word(TREE, BASE + 0x424de40);
        f.word(VIEW + 0x2f0, IDLER);
        f.word(TREE + 0x2f0, IDLER);
        f.number(TREE + 0xe24, 6);
        f.number(MANAGER + 0x3d8, 5);
        f.number(TREE + 0xe20, 1);
        f.number(STATUS + 0x138, 1);
        f.word(VIEW + 0x2f8, ROOT);
        f.word(ROOT, BASE + 0x4347d58);
        f.word(ROOT + 0x98, REG);
        f.put(ROOT + 0xa5, &[if shown { 8 } else { 0 }]);
        f.number(ROOT + 0xeb0, states[0]);
        f.number(ROOT + 0xef8, states[1]);
        f
    }
    fn sample(&self) -> Result<Observation> {
        let w = Witnesses::new(self);
        let r = Reader::new(&w);
        let result = from_reader(&r, BASE, own(), &expected())?;
        w.finish()?;
        self.check()?;
        Ok(result)
    }
}
impl ReadMemory for Fixture {
    fn string_abi(&self) -> StringAbi {
        if self.other_abi {
            StringAbi::LibCxx
        } else {
            StringAbi::LibStdCxx
        }
    }
    fn read(&self, p: u64, n: usize) -> Result<Vec<u8>> {
        self.reads.set(self.reads.get() + 1);
        (p..p + n as u64)
            .map(|a| {
                self.bytes
                    .get(&a)
                    .copied()
                    .ok_or_else(|| Error::new("fixture_unmapped", "Unmapped fixture byte"))
            })
            .collect()
    }
    fn read_many(&self, requests: &[(u64, usize)]) -> Result<Vec<Vec<u8>>> {
        requests
            .iter()
            .map(|(p, n)| {
                let mut b = self.read(*p, *n)?;
                if let Some(a) = self.changed {
                    if a >= *p && a < p + *n as u64 {
                        b[(a - p) as usize] ^= 8;
                    }
                }
                Ok(b)
            })
            .collect()
    }
}
impl Process for Fixture {
    fn check(&self) -> Result<()> {
        Ok(())
    }
    fn image_base(&self) -> u64 {
        BASE
    }
}
fn own() -> ResearchContext {
    ResearchContext {
        campaign: ClockContext {
            playthrough_id: "fixture".into(),
            player: "GER".into(),
            anchors: [GS, IDLER, 0x1b0000],
            date_raw: 60759371,
            difficulty: 2,
            historical_raw: 1,
            paused: false,
            speed: 3,
        },
        own: [COUNTRY, STATUS],
        slots: vec![0x190000, 0x1a0000],
    }
}
fn expected() -> UiContext {
    UiContext {
        campaign: own().campaign,
        anchors: [COUNTRY, STATUS, REG, MANAGER, VIEW, TREE],
        slots: own().slots,
    }
}
#[test]
fn own_root_requires_literal_shown_and_both_channels_settled_with_bounded_named_diagnostics() {
    let ready = Fixture::new([0, 0], true).sample().unwrap();
    ready.require_settled_shown().unwrap();
    for state in [[1, 0], [0, 1], [2, 0], [0, 2], [1, 2], [2, 1]] {
        let sample = Fixture::new(state, true).sample().unwrap();
        assert_eq!(sample.binding, ready.binding);
        let e = sample.require_settled_shown().unwrap_err();
        assert_eq!(e.code, "research_open_transition");
        assert!(e.message.contains("root=research_panel"));
        assert!(e.message.contains(&format!("transitions={state:?}")));
        assert!(!e.message.contains(&ROOT.to_string()));
        assert!(e.message.len() < 192);
    }
    let hidden = Fixture::new([0, 0], false).sample().unwrap();
    let e = hidden.require_settled_shown().unwrap_err();
    assert_eq!(e.code, "research_open_transition");
    assert!(e.message.contains("literal_shown=false"));
}
#[test]
fn unknown_animation_foreign_owner_and_wrong_type_never_become_pending_readiness() {
    for state in [[-1, 0], [0, 3], [i32::MAX, 2], [1, -1]] {
        let e = Fixture::new(state, true).sample().err().unwrap();
        assert_eq!(e.code, "research_precondition");
        assert!(e.message.contains("root=research_panel"));
        assert!(e.message.contains(&format!("transitions={state:?}")));
    }
    for (p, v) in [
        (ROOT + 0x98, REG + 8),
        (MANAGER + 0x180, IDLER + 8),
        (VIEW + 0x2f0, IDLER + 8),
        (BASE + 0x45778e0, REG + 8),
        (TREE + 0x2f0, IDLER + 8),
    ] {
        let mut f = Fixture::new([1, 0], true);
        f.word(p, v);
        assert_eq!(f.sample().err().unwrap().code, "research_precondition");
    }
    let mut f = Fixture::new([1, 0], true);
    f.word(ROOT, BASE + 0x42f2708);
    assert_eq!(f.sample().err().unwrap().code, "invalid_gui_type");
    let mut f = Fixture::new([0, 0], true);
    f.number(TREE + 0xe20, 2);
    assert_eq!(f.sample().err().unwrap().code, "research_precondition");
}
#[test]
fn original_context_and_whole_snapshot_witness_cannot_be_replaced() {
    for mutate in 0..5 {
        let f = Fixture::new([1, 0], true);
        let mut original = expected();
        match mutate {
            0 => original.anchors[1] += 8,
            1 => original.slots[1] += 8,
            2 => original.anchors[4] += 8,
            3 => original.campaign.playthrough_id = "other".into(),
            _ => original.campaign.date_raw += 1,
        }
        assert_eq!(
            from_reader(&Reader::new(&f), BASE, own(), &original)
                .err()
                .unwrap()
                .code,
            "ownership_changed"
        );
    }
    for address in [
        VIEW + 0x2f8,
        ROOT + 0xa5,
        ROOT + 0xeb0,
        ROOT + 0x98,
        MANAGER + 0xf0,
    ] {
        let mut f = Fixture::new([1, 0], true);
        f.changed = Some(address);
        assert_eq!(f.sample().err().unwrap().code, "torn_read");
    }
    let f = Fixture {
        other_abi: true,
        ..Default::default()
    };
    assert_eq!(
        observe(&f, &expected()).err().unwrap().code,
        "capability_unavailable"
    );
    assert_eq!(f.reads.get(), 0);
}
