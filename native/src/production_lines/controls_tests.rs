use super::*;
use crate::memory::ReadMemory;
use std::{cell::RefCell, collections::BTreeMap};
const BASE: u64 = 0x10000;
const ROW: u64 = 0x20000;
const WINDOW: u64 = 0x30000;
const REG: u64 = 0x40000;
const BUTTON: u64 = 0x50000;
const LINK: u64 = 0x60000;
const CAPTURE: u64 = 0x70000;
const DATA: u64 = 0x80000;
const NAMES: u64 = 0x90000;
#[derive(Default)]
struct Fake {
    bytes: BTreeMap<u64, u8>,
    reads: RefCell<Vec<(u64, usize)>>,
}
impl Fake {
    fn put(&mut self, p: u64, bytes: &[u8]) {
        for (i, b) in bytes.iter().enumerate() {
            self.bytes.insert(p + i as u64, *b);
        }
    }
    fn word(&mut self, p: u64, v: u64) {
        self.put(p, &v.to_le_bytes());
    }
    fn number(&mut self, p: u64, v: i32) {
        self.put(p, &v.to_le_bytes());
    }
    fn zero(&mut self, p: u64, n: usize) {
        self.put(p, &vec![0; n]);
    }
    fn array(&mut self, p: u64, data: u64) {
        self.word(p, data);
        self.number(p + 8, 1);
        self.number(p + 12, 1);
    }
    fn fixture() -> Self {
        let mut f = Self::default();
        f.array(WINDOW + 0x9a0, DATA);
        f.word(DATA, BUTTON);
        f.array(WINDOW + 0x9b8, NAMES);
        f.zero(NAMES, 32);
        f.word(NAMES, NAMES + 16);
        f.word(NAMES + 8, 16); // Name is 16 bytes, hence GNU heap form below.
        f.word(NAMES, 0xa0000);
        f.put(0xa0000, b"factories_add_1x");
        f.zero(ROW + 0x1000, 0x30);
        f.word(ROW + 0x1018, BUTTON);
        f.word(BUTTON, BASE + 0x43470c8);
        f.word(BUTTON + 0x68, REG);
        f.word(BUTTON + 0x1a8, 0);
        f.put(BUTTON + 0x201, &[0]);
        f.word(BUTTON + 0xd0, 0);
        f.number(BUTTON + 0xac, 0);
        f.number(BUTTON + 0x74, 0x800);
        f.word(BUTTON + 0x80, BASE + 0x4347488);
        f.word(BUTTON + 0xa0, 0);
        f.number(BUTTON + 0x98, 1);
        f.word(BUTTON + 0x88, LINK);
        f.word(BUTTON + 0x90, LINK);
        f.zero(LINK, 0x19);
        f.word(LINK, ROW + 0xaa8);
        f.zero(ROW + 0xaa8, 0x288);
        f.word(ROW + 0xaa8, BASE + 0x4266748);
        f.word(ROW + 0xaa8 + 0x18, 0);
        f.word(ROW + 0xaa8 + 0x1d8, 0);
        f.word(ROW + 0xaa8 + 0x38, BASE + 0x18157d0);
        f.word(ROW + 0xaa8 + 0x40, BASE + 0x18157a0);
        f.word(ROW + 0xaa8 + 0x28, CAPTURE);
        f.word(CAPTURE, BASE + 0x17e0c60);
        f.word(CAPTURE + 8, 0);
        f.word(CAPTURE + 16, ROW);
        f
    }
    fn control(&self, geometry: bool) -> Result<Value> {
        super::control(
            &Reader::new(self),
            BASE,
            ROW,
            WINDOW,
            REG,
            "factories_add_1x",
            0x1018,
            geometry,
            &mut Vec::new(),
            &mut Vec::new(),
        )
    }
}
impl ReadMemory for Fake {
    fn read(&self, p: u64, n: usize) -> Result<Vec<u8>> {
        self.reads.borrow_mut().push((p, n));
        (0..n)
            .map(|i| {
                self.bytes
                    .get(&(p + i as u64))
                    .copied()
                    .ok_or_else(|| Error::new("unmapped", "Fixture unavailable"))
            })
            .collect()
    }
}
#[test]
fn exact_callback_join_is_observation_only() {
    let f = Fake::fixture();
    let v = f.control(false).unwrap();
    assert_eq!(v["binding_available"], true);
    assert_eq!(v["input_authorized"], false);
    assert!(!f.reads.borrow().iter().any(|(p, _)| *p == BUTTON + 0xe8));
}
#[test]
fn unknown_class_stops_before_button_payload() {
    let mut f = Fake::fixture();
    f.word(BUTTON, BASE + 1);
    let v = f.control(true).unwrap();
    assert_eq!(v["binding_available"], false);
    assert!(!f.reads.borrow().iter().any(|(p, _)| *p == BUTTON + 0x68));
}
#[test]
fn alias_mismatch_refuses_before_callback() {
    let mut f = Fake::fixture();
    f.word(ROW + 0x1018, BUTTON + 8);
    assert_eq!(
        f.control(true).unwrap_err().code,
        "invalid_production_controls"
    );
    assert!(!f.reads.borrow().iter().any(|(p, _)| *p == BUTTON));
}
#[test]
fn callback_overrides_and_wrong_capture_never_read_renderer() {
    for field in [ROW + 0xaa8 + 0x18, CAPTURE + 16] {
        let mut f = Fake::fixture();
        f.word(field, 1);
        assert_eq!(f.control(true).unwrap()["binding_available"], false);
        assert!(!f.reads.borrow().iter().any(|(p, _)| *p == BUTTON + 0xe8));
    }
}
#[test]
fn malformed_listener_refuses() {
    let mut f = Fake::fixture();
    f.word(LINK + 16, LINK);
    assert_eq!(
        f.control(false).unwrap_err().code,
        "invalid_production_controls"
    );
}
#[test]
fn absent_modal_never_means_hidden() {
    let f = Fake::default();
    let v = modal(&Reader::new(&f), BASE, 0, ROW, WINDOW, REG, &mut Vec::new()).unwrap();
    assert_eq!(v["locally_hidden_settled"], false);
    assert_eq!(v["available"], false);
    assert!(f.reads.borrow().is_empty());
}
#[test]
fn positive_modal_requires_exact_owner_before_root() {
    let mut f = Fake::default();
    let dialog = 0xb0000;
    f.word(dialog, BASE + 0x4321298);
    f.word(dialog + 0x7f0, ROW + 8);
    assert_eq!(
        modal(
            &Reader::new(&f),
            BASE,
            dialog,
            ROW,
            WINDOW,
            REG,
            &mut Vec::new()
        )
        .unwrap_err()
        .code,
        "invalid_production_controls"
    );
    assert!(!f.reads.borrow().iter().any(|(p, _)| *p == dialog + 0x800));
}
#[test]
fn only_zero_transition_states_can_supply_hidden() {
    for state in [0, 1, 2, 7] {
        let mut f = Fake::default();
        f.word(WINDOW, BASE + 0x4347d58);
        f.word(WINDOW + 0x98, REG);
        f.put(WINDOW + 0xa5, &[0]);
        f.number(WINDOW + 0xeb0, state);
        f.number(WINDOW + 0xef8, 0);
        let v = window(&Reader::new(&f), BASE, WINDOW, REG).unwrap();
        assert_eq!(v["locally_hidden_settled"], state == 0);
    }
}

#[test]
fn local_dispatch_gates_do_not_erase_binding_or_read_geometry() {
    for (field, value, width) in [
        (BUTTON + 0x1a8, 0xb0000u64, 8),
        (BUTTON + 0x201, 1, 1),
        (BUTTON + 0xd0, 0xb0000, 8),
        (BUTTON + 0xac, 1, 4),
        (BUTTON + 0x74, 0xc00, 4),
    ] {
        let mut f = Fake::fixture();
        f.put(field, &value.to_le_bytes()[..width]);
        let v = f.control(true).unwrap();
        assert_eq!(v["binding_available"], true);
        assert_eq!(v["local_dispatch_ready"], false);
        assert!(!f.reads.borrow().iter().any(|(p, _)| *p == BUTTON + 0xe8));
    }
    let mut f = Fake::fixture();
    f.number(BUTTON + 0xac, 2);
    assert_eq!(f.control(false).unwrap()["local_dispatch_ready"], true);
}
#[test]
fn unused_callback_selectors_and_kind9_cancel_are_refused() {
    for offset in [
        8, 0x48, 0x88, 0xc8, 0x108, 0x148, 0x188, 0x1c8, 0x208, 0x248, 0x278,
    ] {
        let mut f = Fake::fixture();
        f.word(ROW + 0xaa8 + offset, 1);
        assert_eq!(f.control(true).unwrap()["binding_available"], false);
        assert!(!f.reads.borrow().iter().any(|(p, _)| *p == BUTTON + 0xe8));
    }
}

#[test]
fn selected_one_step_alias_cannot_occupy_another_callback_branch() {
    for offset in [0x1000, 0x1008, 0x1010, 0x1020, 0x1028] {
        let mut f = Fake::fixture();
        f.word(ROW + offset, BUTTON);
        assert_eq!(
            f.control(true).unwrap_err().code,
            "invalid_production_controls"
        );
        assert!(!f.reads.borrow().iter().any(|(p, _)| *p == BUTTON));
    }
    let f = Fake::fixture();
    assert_eq!(f.control(false).unwrap()["binding_available"], true);
    for offset in [0x1000, 0x1008, 0x1010, 0x1020, 0x1028] {
        assert!(f.reads.borrow().contains(&(ROW + offset, 8)));
    }
}
