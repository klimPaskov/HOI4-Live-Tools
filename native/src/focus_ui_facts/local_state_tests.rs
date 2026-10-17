use super::*;
use crate::{memory::ReadMemory, platform::Process, research_ui::Witnesses};
use std::{cell::RefCell, collections::BTreeMap};
const WIN: u64 = 0x100000;
const GUI: u64 = 0x110000;
struct Fixture {
    data: RefCell<BTreeMap<u64, u8>>,
    reads: RefCell<Vec<(u64, usize)>>,
}
impl Fixture {
    fn new(visible: bool, animation: [i32; 2]) -> Self {
        let f = Self {
            data: RefCell::new(BTreeMap::new()),
            reads: RefCell::new(vec![]),
        };
        f.put(WIN, &0x4347d58u64.to_le_bytes());
        f.put(WIN + 0x98, &GUI.to_le_bytes());
        f.put(WIN + 0xa5, &[if visible { 8 } else { 0 }]);
        f.put(WIN + 0xeb0, &animation[0].to_le_bytes());
        f.put(WIN + 0xef8, &animation[1].to_le_bytes());
        f
    }
    fn put(&self, a: u64, bytes: &[u8]) {
        for (i, b) in bytes.iter().enumerate() {
            self.data.borrow_mut().insert(a + i as u64, *b);
        }
    }
    fn read_fact(&self) -> FocusUiFacts {
        let r = Reader::new(self);
        let fact = window(&r, 0, WIN, GUI).unwrap();
        FocusUiFacts {
            mode: 13,
            focus: fact.clone(),
            details: vec![fact.clone()],
            research: Some(vec![fact.clone()]),
            research_panel: fact.clone(),
            research_tree: fact,
        }
    }
}
impl ReadMemory for Fixture {
    fn read(&self, a: u64, n: usize) -> Result<Vec<u8>> {
        self.reads.borrow_mut().push((a, n));
        (0..n)
            .map(|i| {
                self.data
                    .borrow()
                    .get(&(a + i as u64))
                    .copied()
                    .ok_or_else(|| Error::new("fixture_unmapped", "No payload at unverified owner"))
            })
            .collect()
    }
}
impl Process for Fixture {
    fn check(&self) -> Result<()> {
        Ok(())
    }
    fn image_base(&self) -> u64 {
        0
    }
}
#[test]
fn literal_hidden_and_shown_require_both_channels_zero() {
    for v in [false, true] {
        let f = Fixture::new(v, [0, 0]);
        let facts = f.read_fact();
        assert_eq!(facts.focus_view_local_ready(), Some(v));
        assert_eq!(facts.focus_view_local_hidden(), Some(!v));
        assert_eq!(facts.focus_detail_local_counts(), Some([usize::from(v), 0]));
        assert_eq!(
            facts.own_research_info_local_counts(),
            Some([usize::from(v), 0])
        );
    }
}
#[test]
fn pending_or_unknown_channel_cannot_clear_any_local_blockers() {
    for states in [
        [1, 0],
        [0, 1],
        [1, 1],
        [-1, 0],
        [0, 3],
        [i32::MAX, 0],
        [1, 2],
    ] {
        for v in [false, true] {
            let facts = Fixture::new(v, states).read_fact();
            assert_eq!(facts.focus_view_local_ready(), None);
            assert_eq!(facts.focus_view_local_hidden(), None);
            assert_eq!(facts.focus_detail_local_counts(), None);
            assert_eq!(facts.own_research_info_local_counts(), None);
        }
    }
}
#[test]
fn active_channel_blocks_without_becoming_hidden_or_ready() {
    for states in [[2, 0], [0, 2], [2, 2]] {
        for v in [false, true] {
            let facts = Fixture::new(v, states).read_fact();
            assert_eq!(facts.focus_view_local_ready(), None);
            assert_eq!(facts.focus_view_local_hidden(), None);
            assert_eq!(facts.focus_detail_local_counts(), Some([usize::from(v), 1]));
        }
    }
}
#[test]
fn wrong_type_or_registry_refuses_before_visibility_payload() {
    for field in [WIN, WIN + 0x98] {
        let f = Fixture::new(false, [0, 0]);
        f.put(field, &0x220000u64.to_le_bytes());
        f.data.borrow_mut().retain(|a, _| *a < WIN + 0xa5);
        let r = Reader::new(&f);
        let e = window(&r, 0, WIN, GUI).unwrap_err();
        assert_eq!(
            e.code,
            if field == WIN {
                "invalid_focus_ui_type"
            } else {
                "invalid_focus_owner"
            }
        );
        assert!(f.reads.borrow().iter().all(|(a, _)| *a < WIN + 0xa5));
    }
}
#[test]
fn null_or_unmaterialized_is_unknown_not_hidden() {
    let f = Fixture::new(false, [0, 0]);
    let r = Reader::new(&f);
    assert!(window(&r, 0, 0, GUI).unwrap().is_none());
    assert!(f.reads.borrow().is_empty());
    let facts = FocusUiFacts {
        mode: 13,
        focus: None,
        details: vec![None],
        research: None,
        research_panel: None,
        research_tree: None,
    };
    assert_eq!(facts.focus_view_local_hidden(), None);
    assert_eq!(facts.focus_view_local_ready(), None);
    assert_eq!(facts.focus_detail_local_counts(), None);
    assert_eq!(facts.own_research_info_local_counts(), None);
}
#[test]
fn final_witness_covers_owner_type_literal_and_both_states() {
    for (a, bytes) in [
        (WIN, 0x4347000u64.to_le_bytes().to_vec()),
        (WIN + 0x98, 0x220000u64.to_le_bytes().to_vec()),
        (WIN + 0xa5, vec![8]),
        (WIN + 0xeb0, 1i32.to_le_bytes().to_vec()),
        (WIN + 0xef8, 2i32.to_le_bytes().to_vec()),
    ] {
        let f = Fixture::new(false, [0, 0]);
        let w = Witnesses::new(&f);
        let r = Reader::new(&w);
        assert_eq!(
            window(&r, 0, WIN, GUI)
                .unwrap()
                .unwrap()
                .settled_visibility(),
            Some(false)
        );
        f.put(a, &bytes);
        assert_eq!(w.finish().unwrap_err().code, "torn_read");
    }
}
