use super::*;
use crate::memory::ReadMemory;
use std::{cell::RefCell, collections::BTreeMap};
const B: u64 = 0x100000000;
const I: u64 = 0x2000000;
const GUI: u64 = I + 0x10000;
const GFX: u64 = I + 0x20000;
const APP: u64 = I + 0x30000;
const PUMP: u64 = I + 0x40000;
const MAN: u64 = I + 0x50000;
const VIEW: u64 = I + 0x60000;
const ROOT: u64 = I + 0x70000;
const FIND: u64 = I + 0x80000;
const MENU: u64 = I + 0x90000;
const BUTTON: u64 = I + 0xa0000;
const DESC: u64 = I + 0xb0000;
const NODE: u64 = I + 0xc0000;
const FUNC: u64 = I + 0xd0000;
const SPRITE: u64 = I + 0xe0000;
const POS: u64 = I + 0xf0000;
const CHILDREN: u64 = I + 0x100000;
const SOURCES: u64 = I + 0x110000;
const KEYS: u64 = I + 0x120000;
const FOREIGN: u64 = I + 0x130000;
const OTHERROOT: u64 = I + 0x140000;
struct Fake {
    bytes: BTreeMap<u64, u8>,
    reads: RefCell<Vec<(u64, usize)>>,
    tear: Option<u64>,
}
impl Fake {
    fn put(&mut self, a: u64, b: &[u8]) {
        for (i, v) in b.iter().enumerate() {
            self.bytes.insert(a + i as u64, *v);
        }
    }
    fn word(&mut self, a: u64, v: u64) {
        self.put(a, &v.to_le_bytes());
    }
    fn int(&mut self, a: u64, v: i32) {
        self.put(a, &v.to_le_bytes());
    }
    fn byte(&mut self, a: u64, v: u8) {
        self.put(a, &[v]);
    }
    fn array(&mut self, a: u64, data: u64, v: &[u64]) {
        self.word(a, data);
        self.int(a + 8, v.len() as i32);
        self.int(a + 12, v.len() as i32);
        for (i, n) in v.iter().enumerate() {
            self.word(data + i as u64 * 8, *n);
        }
    }
    fn window(&mut self, a: u64, visible: u8) {
        self.word(a, B + 0x34ac0b0);
        self.word(a + 0x88, GUI);
        self.byte(a + 0x95, visible);
        self.int(a + 0x12b8, 0);
        self.int(a + 0x1270, 0);
    }
    fn not_read(&self, a: u64) {
        assert!(
            !self
                .reads
                .borrow()
                .iter()
                .any(|(p, n)| *p <= a && a < *p + *n as u64),
            "unexpected payload {a:x}"
        );
    }
}
impl ReadMemory for Fake {
    fn read(&self, a: u64, n: usize) -> Result<Vec<u8>> {
        let repeated = self.reads.borrow().iter().any(|(x, _)| *x == a);
        self.reads.borrow_mut().push((a, n));
        let mut b = (0..n)
            .map(|i| {
                self.bytes
                    .get(&(a + i as u64))
                    .copied()
                    .ok_or_else(|| Error::new("fixture_unmapped", format!("{a:x}+{i:x}")))
            })
            .collect::<Result<Vec<_>>>()?;
        if self.tear == Some(a) && repeated {
            b[0] ^= 1;
        }
        Ok(b)
    }
    fn string_abi(&self) -> StringAbi {
        StringAbi::LibCxx
    }
}
impl Process for Fake {
    fn check(&self) -> Result<()> {
        Ok(())
    }
    fn image_base(&self) -> u64 {
        B
    }
}
fn fixture() -> Fake {
    let mut f = Fake {
        bytes: BTreeMap::new(),
        reads: RefCell::new(Vec::new()),
        tear: None,
    };
    for (a, v) in [
        (I, B + 0x3308f90),
        (B + 0x3308f90 + 0x20, B + 0x776fe0),
        (I + 0x4c0, GUI),
        (GUI, B + 0x34974d0),
        (B + 0x35dc058, GUI),
        (GUI + 0x138, 0),
        (GUI + 0x210, 0),
        (GUI + 0x98, GFX),
        (GFX, B + 0x33dafa8),
        (I + 0x4f8, APP),
        (APP + 0x300, GFX),
        (APP + 0x308, GUI),
        (APP + 0x38, I),
        (B + 0x35d8dd0, 0),
        (B + 0x35d8c60, PUMP),
        (PUMP, B + 0x34a6760),
        (I + 0x670, MAN),
        (MAN + 0x180, I),
        (MAN + 0x150, VIEW),
        (VIEW, B + 0x343aa90),
        (VIEW + 0x428, I),
        (VIEW + 0x30, MAN),
        (VIEW + 0x430, ROOT),
        (MAN + 0x190, FIND),
        (FIND, B + 0x3457c38),
        (FIND + 0xc30, I),
        (FIND + 0xc38, 0),
        (MAN + 0x218, MENU),
        (MENU, B + 0x3461428),
        (MENU + 0x2668, I),
        (MENU + 0x2670, MAN),
        (MENU + 0x2678, 0),
        (MENU + 0x2690, 0),
        (BUTTON, B + 0x34aafa0),
        (BUTTON + 0x50, DESC),
        (BUTTON + 0x58, GUI),
        (DESC, B + 0x34ab7c0),
        (BUTTON + 0x70, B + 0x34ab360),
        (BUTTON + 0x90, 0),
        (BUTTON + 0xc0, 0),
        (BUTTON + 0x78, NODE),
        (BUTTON + 0x80, NODE),
        (NODE + 8, 0),
        (NODE + 0x10, 0),
        (NODE, VIEW + 0x40),
        (VIEW + 0x40, B + 0x34466a8),
        (VIEW + 0x70, FUNC),
        (FUNC, B + 0x3446738),
        (FUNC + 8, 0x91),
        (FUNC + 0x10, 0),
        (FUNC + 0x18, VIEW),
        (BUTTON + 0xd0, GFX),
        (BUTTON + 0xd8, SPRITE),
        (SPRITE, B + 0x34bdb10),
        (SPRITE + 0x138, BUTTON),
        (SPRITE + 0x108, POS),
        (ROOT + 0x1238, POS),
        (POS, B + 0x34b0428),
        (BUTTON + 0x178, POS),
    ] {
        f.word(a, v);
    }
    for a in [GUI + 0x9e0, GUI + 0x9f8] {
        f.array(a, 0, &[]);
    }
    f.byte(I + 0x7b5, 1);
    f.byte(B + 0x3519990, 0);
    f.put(GUI + 0x10, &[1, 0, 0]);
    f.byte(GUI + 0x130, 0);
    f.byte(B + 0x35dbfa8, 0);
    f.byte(PUMP + 0x44, 0);
    f.int(B + 0x3638d28, 0x1b);
    f.byte(B + 0x3638aab, 0);
    f.put(B + 0x3638880, &[0, 0]);
    f.int(MAN + 0x3a8, 17);
    f.int(MAN + 0x3c0, 1);
    f.byte(MAN + 0x3b1, 0);
    f.byte(MAN + 0x3b2, 0);
    f.int(MAN + 0xb4, 0);
    f.window(ROOT, 8);
    f.array(ROOT + 0xd30, CHILDREN, &[BUTTON]);
    f.put(DESC + 0x28, &[0; 24]);
    f.byte(DESC + 0x28, 24);
    f.put(DESC + 0x29, b"close_button");
    f.byte(BUTTON + 0x8c, 0);
    f.int(BUTTON + 0x88, 1);
    f.byte(NODE + 0x18, 0);
    for index in 2..=20 {
        f.word(VIEW + 0x40 + index * 0x30, 0);
    }
    f.int(BUTTON + 0x64, 0x804);
    f.int(BUTTON + 0x9c, 0);
    f.byte(BUTTON + 0xcc, 8);
    f.byte(BUTTON + 0x1d1, 0);
    f.byte(POS + 0x128, 1);
    f.array(GFX + 0x68, SOURCES, &[SPRITE]);
    f.word(BUTTON + 0xa0, KEYS);
    f.int(BUTTON + 0xa8, 1);
    f.int(BUTTON + 0xac, 1);
    f.put(KEYS, &[0; 40]);
    f.int(KEYS, 0x1b);
    f
}
fn capture(f: &Fake, old: Option<&Binding>) -> Result<Binding> {
    from_reader(&Reader::new(f), B, I, old)
}
#[test]
fn exact_escape_route_and_virtual_close_capture_are_typed() {
    let f = fixture();
    let b = capture(&f, None).unwrap();
    assert_eq!(b.button, BUTTON);
    assert_eq!(b.paths[0].1, vec![BUTTON]);
    f.not_read(FOREIGN);
}
#[test]
fn current_t_cannot_stand_in_for_escape() {
    let mut f = fixture();
    f.int(KEYS, 0x74);
    assert!(capture(&f, None).is_err());
}
#[test]
fn focus_refuses_without_following_receiver_or_key_collection() {
    for at in [GUI + 0x138, GUI + 0x210] {
        let mut f = fixture();
        f.word(at, FOREIGN);
        assert!(capture(&f, None).is_err());
        f.not_read(FOREIGN);
        f.not_read(SOURCES);
        f.not_read(KEYS);
    }
}
#[test]
fn hidden_find_and_menu_must_be_locally_idle() {
    for field in [FIND + 0xc38, MENU + 0x2678] {
        for (shown, transition) in [(8, 0), (0, 1), (0, 2)] {
            let mut f = fixture();
            f.window(OTHERROOT, shown);
            f.int(OTHERROOT + 0x1270, transition);
            f.word(field, OTHERROOT);
            assert!(capture(&f, None).is_err());
            f.not_read(BUTTON);
        }
    }
}
#[test]
fn foreign_menu_owner_and_type_refuse_before_window_payload() {
    for (a, v) in [
        (MENU, B + 0x3457c38),
        (MENU + 0x2668, FOREIGN),
        (MENU + 0x2670, FOREIGN),
    ] {
        let mut f = fixture();
        f.word(a, v);
        f.word(MENU + 0x2678, FOREIGN);
        assert!(capture(&f, None).is_err());
        f.not_read(FOREIGN);
        f.not_read(BUTTON);
    }
}
#[test]
fn outer_override_current_receiver_and_pump_diversion_refuse() {
    for (a, v) in [(APP + 0x38, FOREIGN), (PUMP, B + 0x34974d0)] {
        let mut f = fixture();
        f.word(a, v);
        assert!(capture(&f, None).is_err());
        f.not_read(FOREIGN);
        f.not_read(BUTTON);
    }
    let mut f = fixture();
    f.byte(PUMP + 0x44, 1);
    assert!(capture(&f, None).is_err());
    f.not_read(BUTTON);
}
#[test]
fn callbacks_are_single_active_exact_virtual_member_only() {
    for (a, v) in [
        (FUNC + 8, 0x90),
        (FUNC + 0x10, 8),
        (FUNC + 0x18, FOREIGN),
        (VIEW + 0x40 + 0x60, FOREIGN),
        (BUTTON + 0x90, FOREIGN),
        (BUTTON + 0xc0, FOREIGN),
    ] {
        let mut f = fixture();
        f.word(a, v);
        assert!(capture(&f, None).is_err());
        f.not_read(FOREIGN);
        f.not_read(KEYS);
    }
    for a in [BUTTON + 0x8c, NODE + 0x18] {
        let mut f = fixture();
        f.byte(a, 1);
        assert!(capture(&f, None).is_err());
        f.not_read(KEYS);
    }
}
#[test]
fn unknown_popup_refuses_before_typed_payload() {
    let mut f = fixture();
    f.array(GUI + 0x9e0, FOREIGN, &[OTHERROOT]);
    f.word(OTHERROOT, B + 0x33ec9d8);
    assert!(capture(&f, None).is_err());
    f.not_read(OTHERROOT + 0x430);
    f.not_read(BUTTON);
}
#[test]
fn history_and_transition_refuse_before_button() {
    for (a, v) in [
        (MAN + 0xb4, 1),
        (MAN + 0x3a8, 23),
        (MAN + 0x3c0, 0),
        (ROOT + 0x1270, 1),
    ] {
        let mut f = fixture();
        f.int(a, v);
        assert!(capture(&f, None).is_err());
        f.not_read(BUTTON);
    }
}
#[test]
fn retained_edges_refuse_replacement_before_its_payload() {
    for a in [
        I + 0x670,
        MAN + 0x150,
        VIEW + 0x430,
        MAN + 0x190,
        MAN + 0x218,
        B + 0x35d8c60,
    ] {
        let mut f = fixture();
        let b = capture(&f, None).unwrap();
        f.reads.borrow_mut().clear();
        f.word(a, FOREIGN);
        assert!(capture(&f, Some(&b)).is_err());
        f.not_read(FOREIGN);
    }
}
#[test]
fn same_name_button_replacement_is_not_followed() {
    let mut f = fixture();
    let b = capture(&f, None).unwrap();
    f.reads.borrow_mut().clear();
    f.word(CHILDREN, FOREIGN);
    assert!(capture(&f, Some(&b)).is_err());
    f.not_read(FOREIGN);
}
#[test]
fn replaced_keyboard_sources_and_targets_refuse_before_payload() {
    for a in [SOURCES, SPRITE + 0x138] {
        let mut f = fixture();
        let b = capture(&f, None).unwrap();
        f.reads.borrow_mut().clear();
        f.word(a, FOREIGN);
        assert!(capture(&f, Some(&b)).is_err());
        f.not_read(FOREIGN);
    }
}
#[test]
fn unknown_preceding_renderer_is_never_silently_skipped() {
    let mut f = fixture();
    f.array(GFX + 0x68, SOURCES, &[SPRITE, FOREIGN]);
    f.word(FOREIGN, B + 0x34974d0);
    assert!(capture(&f, None).is_err());
    f.not_read(FOREIGN + 0x138);
    f.not_read(KEYS);
}
#[test]
fn final_witness_refuses_changed_keyboard_gate() {
    let mut f = fixture();
    f.tear = Some(GUI + 0x10);
    let w = Witnesses::new(&f);
    from_reader(&Reader::new(&w), B, I, None).unwrap();
    assert_eq!(w.finish().unwrap_err().code, "torn_read");
}

#[test]
fn sdl_escape_semantics_pressed_state_and_modifier_are_current_preconditions() {
    for (at, value) in [(B + 0x3638d28, 116), (B + 0x3638aab, 1), (B + 0x3638880, 1)] {
        let mut f = fixture();
        if at == B + 0x3638d28 {
            f.int(at, value);
        } else {
            f.byte(at, value as u8);
        }
        assert!(capture(&f, None).is_err());
        f.not_read(BUTTON);
        f.not_read(KEYS);
    }
}

#[test]
fn main_frame_gui_gate_or_alternate_path_refuses_before_ui_payload() {
    for (at, value) in [(I + 0x7b5, 0), (B + 0x3519990, 1)] {
        let mut f = fixture();
        f.byte(at, value);
        assert!(capture(&f, None).is_err());
        f.not_read(GUI);
        f.not_read(BUTTON);
        f.not_read(KEYS);
    }
}

#[test]
fn unsupported_abi_and_malformed_image_bases_refuse_before_reads() {
    let f = fixture();
    for base in [0, 1, B + 1, u64::MAX] {
        assert!(from_reader(&Reader::new(&f), base, I, None).is_err());
        assert!(f.reads.borrow().is_empty());
    }
    struct Other<'a>(&'a Fake);
    impl ReadMemory for Other<'_> {
        fn read(&self, a: u64, n: usize) -> Result<Vec<u8>> {
            self.0.read(a, n)
        }
        fn string_abi(&self) -> StringAbi {
            StringAbi::LibStdCxx
        }
    }
    assert_eq!(
        from_reader(&Reader::new(&Other(&f)), B, I, None)
            .unwrap_err()
            .code,
        "unsupported_platform"
    );
    assert!(f.reads.borrow().is_empty());
}

#[test]
fn escape_invalidation_accepts_only_existing_typed_singleton_and_boolean_state() {
    const SINGLETON: u64 = 0x30a00000;
    for state in [0, 1] {
        let mut f = fixture();
        f.byte(B + 0x35dbfa8, 1);
        f.word(B + 0x35dbfa0, SINGLETON);
        f.word(SINGLETON, B + 0x34aabd0);
        f.byte(SINGLETON + 0x411, state);
        assert_eq!(capture(&f, None).unwrap().escape_invalidation, Some(state));
    }
    let f = fixture();
    capture(&f, None).unwrap();
    f.not_read(B + 0x35dbfa0); // Flag0 never needs the singleton payload.
}

#[test]
fn escape_invalidation_unknown_gate_or_type_never_follows_payload() {
    const SINGLETON: u64 = 0x30a00000;
    let mut f = fixture();
    f.byte(B + 0x35dbfa8, 2);
    assert!(capture(&f, None).is_err());
    f.not_read(B + 0x35dbfa0);
    for primary in [0, B + 0x346bff0] {
        let mut f = fixture();
        f.byte(B + 0x35dbfa8, 1);
        f.word(B + 0x35dbfa0, SINGLETON);
        f.word(SINGLETON, primary);
        assert!(capture(&f, None).is_err());
        f.not_read(SINGLETON + 0x411);
    }
    let mut f = fixture();
    f.byte(B + 0x35dbfa8, 1);
    f.word(B + 0x35dbfa0, SINGLETON);
    f.word(SINGLETON, B + 0x34aabd0);
    f.byte(SINGLETON + 0x411, 2);
    assert!(capture(&f, None).is_err());
}

#[test]
fn escape_invalidation_original_singleton_is_not_replaced_before_key() {
    const SINGLETON: u64 = 0x30a00000;
    let mut f = fixture();
    f.byte(B + 0x35dbfa8, 1);
    f.word(B + 0x35dbfa0, SINGLETON);
    f.word(SINGLETON, B + 0x34aabd0);
    f.byte(SINGLETON + 0x411, 0);
    let before = capture(&f, None).unwrap();
    f.word(B + 0x35dbfa0, FOREIGN);
    assert!(capture(&f, Some(&before)).is_err());
    f.not_read(FOREIGN);
}

const POP: u64 = I + 0x150000;
const POPROOT: u64 = I + 0x160000;
const POPDATA: u64 = I + 0x170000;
fn popup(f: &mut Fake, at: u64) {
    f.array(at, POPDATA, &[POP]);
    f.word(POP, B + 0x33ec9d0);
    f.word(POP + 0x430, POPROOT);
    f.byte(POP + 0x438, 0);
    f.byte(POP + 0x439, 0);
    f.window(POPROOT, 16);
}
#[test]
fn exact_hidden_plane_lifetime_is_admitted_in_either_collection() {
    for at in [GUI + 0x9e0, GUI + 0x9f8] {
        let mut f = fixture();
        popup(&mut f, at);
        let b = capture(&f, None).unwrap();
        assert_eq!(b.popup_windows.len(), 1);
        assert_eq!(b.popup_windows[0].flags, 16);
        capture(&f, Some(&b)).unwrap();
    }
}
#[test]
fn visible_or_animating_plane_lifetime_refuses() {
    for (a, v) in [
        (POPROOT + 0x95, 8),
        (POPROOT + 0x1270, 1),
        (POPROOT + 0x12b8, 2),
    ] {
        let mut f = fixture();
        popup(&mut f, GUI + 0x9e0);
        if a == POPROOT + 0x95 {
            f.byte(a, v as u8);
        } else {
            f.int(a, v);
        }
        assert!(capture(&f, None).is_err());
        f.not_read(BUTTON);
    }
}
#[test]
fn closing_or_timed_lifetimes_refuse_before_root() {
    for a in [POP + 0x438, POP + 0x439] {
        let mut f = fixture();
        popup(&mut f, GUI + 0x9e0);
        f.byte(a, 1);
        assert!(capture(&f, None).is_err());
        f.not_read(POPROOT);
        f.not_read(BUTTON);
    }
}
#[test]
fn lifetime_capacity_and_combined_count_are_bounded_before_payload() {
    for (cap, count) in [(65, 1), (65, 65)] {
        let mut f = fixture();
        popup(&mut f, GUI + 0x9e0);
        f.int(GUI + 0x9e8, cap);
        f.int(GUI + 0x9ec, count);
        assert!(capture(&f, None).is_err());
        f.not_read(POPDATA);
        f.not_read(POP);
    }
}
#[test]
fn duplicate_lifetimes_across_arrays_refuse_before_object_payload() {
    let mut f = fixture();
    popup(&mut f, GUI + 0x9e0);
    f.array(GUI + 0x9f8, FOREIGN, &[POP]);
    assert!(capture(&f, None).is_err());
    f.not_read(POP);
}
#[test]
fn unknown_root_refuses_before_fields() {
    let mut f = fixture();
    popup(&mut f, GUI + 0x9e0);
    f.word(POPROOT, B + 0x34ac0b8);
    assert!(capture(&f, None).is_err());
    f.not_read(POPROOT + 0x88);
    f.not_read(POPROOT + 0x95);
}
#[test]
fn foreign_root_registry_refuses_before_visibility() {
    let mut f = fixture();
    popup(&mut f, GUI + 0x9e0);
    f.word(POPROOT + 0x88, FOREIGN);
    assert!(capture(&f, None).is_err());
    f.not_read(FOREIGN);
    f.not_read(POPROOT + 0x95);
}
#[test]
fn retained_lifetime_edges_refuse_replacements_before_new_payload() {
    for a in [
        GUI + 0x9e0,
        POPDATA,
        POP,
        POP + 0x430,
        POPROOT,
        POPROOT + 0x88,
    ] {
        let mut f = fixture();
        popup(&mut f, GUI + 0x9e0);
        let b = capture(&f, None).unwrap();
        f.reads.borrow_mut().clear();
        f.word(a, FOREIGN);
        assert!(capture(&f, Some(&b)).is_err());
        f.not_read(FOREIGN);
        f.not_read(BUTTON);
    }
}
#[test]
fn retained_lifetime_header_or_hidden_flags_cannot_change() {
    for a in [GUI + 0x9e8, GUI + 0x9ec, POPROOT + 0x95] {
        let mut f = fixture();
        popup(&mut f, GUI + 0x9e0);
        let b = capture(&f, None).unwrap();
        f.reads.borrow_mut().clear();
        if a == POPROOT + 0x95 {
            f.byte(a, 0);
        } else {
            f.int(a, 0);
        }
        assert!(capture(&f, Some(&b)).is_err());
    }
}
#[test]
fn lifetime_consumed_fields_share_final_witnesses() {
    for at in [POPDATA, POP + 0x430, POPROOT + 0x95, POPROOT + 0x1270] {
        let mut f = fixture();
        popup(&mut f, GUI + 0x9e0);
        f.tear = Some(at);
        let w = Witnesses::new(&f);
        let r = Reader::new(&w);
        hidden_lifetimes(&r, B, GUI, &mut Vec::new()).unwrap();
        assert_eq!(w.finish().unwrap_err().code, "torn_read");
    }
}

const HOTKEYS: u64 = I + 0x180000;
fn hotkeys(f: &mut Fake, character_mode: u8) {
    f.word(B + 0x35d8dd0, HOTKEYS);
    f.word(HOTKEYS, B + 0x34a7db8);
    f.word(B + 0x3639ad8, APP);
    f.word(B + 0x34a7db8 + 0x20, B + 0x26ebf10);
    f.byte(B + 0x35d8dd8, character_mode);
    if character_mode == 1 {
        f.int(HOTKEYS + 0x64, 0);
    }
}
#[test]
fn exact_hotkey_wrapper_admits_normal_escape_in_both_modes() {
    for mode in [0, 1] {
        let mut f = fixture();
        hotkeys(&mut f, mode);
        let b = capture(&f, None).unwrap();
        assert_eq!(
            b.outer_receiver,
            OuterReceiver::Hotkeys {
                character_mode: mode,
                idle_stage: if mode == 1 { Some(0) } else { None }
            }
        );
        if mode == 0 {
            f.not_read(HOTKEYS + 0x64);
        }
        capture(&f, Some(&b)).unwrap();
    }
}
#[test]
fn foreign_hotkey_application_refuses_before_wrapper_payload() {
    let mut f = fixture();
    hotkeys(&mut f, 1);
    f.word(B + 0x3639ad8, FOREIGN);
    assert!(capture(&f, None).is_err());
    f.not_read(FOREIGN);
    f.not_read(HOTKEYS + 0x64);
    f.not_read(BUTTON);
}
#[test]
fn capturing_and_unknown_hotkey_modes_refuse() {
    for stage in [1, 2, -1, 3] {
        let mut f = fixture();
        hotkeys(&mut f, 1);
        f.int(HOTKEYS + 0x64, stage);
        assert!(capture(&f, None).is_err());
        f.not_read(BUTTON);
    }
    let mut f = fixture();
    hotkeys(&mut f, 2);
    assert!(capture(&f, None).is_err());
    f.not_read(HOTKEYS + 0x64);
}
#[test]
fn hotkey_dispatch_slot_must_match_inspected_wrapper() {
    let mut f = fixture();
    hotkeys(&mut f, 1);
    f.word(B + 0x34a7db8 + 0x20, FOREIGN);
    assert!(capture(&f, None).is_err());
    f.not_read(FOREIGN);
    f.not_read(HOTKEYS + 0x64);
}
#[test]
fn original_hotkey_edges_refuse_replacement_before_foreign_payload() {
    for at in [B + 0x35d8dd0, HOTKEYS, B + 0x3639ad8, B + 0x34a7db8 + 0x20] {
        let mut f = fixture();
        hotkeys(&mut f, 1);
        let b = capture(&f, None).unwrap();
        f.reads.borrow_mut().clear();
        f.word(at, FOREIGN);
        assert!(capture(&f, Some(&b)).is_err());
        f.not_read(FOREIGN);
        f.not_read(BUTTON);
    }
}
#[test]
fn coherent_hotkey_mode_change_cannot_replace_original_binding() {
    let mut f = fixture();
    hotkeys(&mut f, 1);
    let b = capture(&f, None).unwrap();
    f.byte(B + 0x35d8dd8, 0);
    assert!(capture(&f, Some(&b)).is_err());
}
#[test]
fn hotkey_consumed_fields_share_final_witnesses() {
    for at in [B + 0x35d8dd8, HOTKEYS + 0x64, B + 0x3639ad8] {
        let mut f = fixture();
        hotkeys(&mut f, 1);
        f.tear = Some(at);
        let w = Witnesses::new(&f);
        let r = Reader::new(&w);
        escape_receiver(&r, B, APP, &mut Vec::new()).unwrap();
        assert_eq!(w.finish().unwrap_err().code, "torn_read");
    }
}
#[test]
fn two_individually_valid_lifetime_arrays_cannot_exceed_combined_bound() {
    let mut f = fixture();
    let first: Vec<_> = (0..32).map(|i| POP + i * 0x1000).collect();
    let second: Vec<_> = (32..65).map(|i| POP + i * 0x1000).collect();
    f.array(GUI + 0x9e0, POPDATA, &first);
    f.array(GUI + 0x9f8, FOREIGN, &second);
    assert!(capture(&f, None).is_err());
    f.not_read(POP);
    f.not_read(FOREIGN);
}

#[test]
fn unknown_outer_override_refuses_before_optional_payload() {
    let mut f = fixture();
    f.word(B + 0x35d8dd0, FOREIGN);
    f.word(FOREIGN, B + 0x34a7dc0);
    assert!(capture(&f, None).is_err());
    f.not_read(FOREIGN + 0x64);
    f.not_read(B + 0x3639ad8);
    f.not_read(BUTTON);
}
#[test]
fn malformed_outer_override_pointer_refuses_before_primary() {
    for receiver in [1, HOTKEYS + 1] {
        let mut f = fixture();
        f.word(B + 0x35d8dd0, receiver);
        assert!(capture(&f, None).is_err());
        f.not_read(receiver);
        f.not_read(B + 0x3639ad8);
    }
}

const PREFIX_BUTTON: u64 = I + 0x190000;
const PREFIX_RENDERER: u64 = I + 0x1a0000;
const PREFIX_KEYS: u64 = I + 0x1b0000;
fn standard_prefix(f: &mut Fake, key: i32) {
    for (a, value) in [
        (PREFIX_BUTTON, B + 0x34aafa0),
        (PREFIX_BUTTON + 0x58, GUI),
        (PREFIX_BUTTON + 0xd0, GFX),
        (PREFIX_BUTTON + 0xd8, PREFIX_RENDERER),
        (PREFIX_BUTTON + 0x178, POS),
        (PREFIX_BUTTON + 0xc0, 0),
        (PREFIX_BUTTON + 0xa0, PREFIX_KEYS),
        (PREFIX_RENDERER, B + 0x34bdb10),
        (PREFIX_RENDERER + 0x138, PREFIX_BUTTON),
        (PREFIX_RENDERER + 0x108, POS),
    ] {
        f.word(a, value);
    }
    f.int(PREFIX_BUTTON + 0x64, 0x804);
    f.int(PREFIX_BUTTON + 0x9c, 0);
    f.byte(PREFIX_BUTTON + 0xcc, 8);
    f.byte(PREFIX_BUTTON + 0x1d1, 0);
    f.int(PREFIX_BUTTON + 0xa8, 1);
    f.int(PREFIX_BUTTON + 0xac, 1);
    f.put(PREFIX_KEYS, &[0; 40]);
    f.int(PREFIX_KEYS, key);
    f.array(GFX + 0x68, SOURCES, &[SPRITE, PREFIX_RENDERER]);
}
#[test]
fn escape_refresh_accepts_new_inert_prefix_and_its_later_removal() {
    let mut f = fixture();
    let original = capture(&f, None).unwrap();
    standard_prefix(&mut f, 0x74);
    let fresh = capture(&f, Some(&original)).unwrap();
    assert_eq!(original, fresh);
    f.array(GFX + 0x68, SOURCES, &[SPRITE]);
    capture(&f, Some(&original)).unwrap();
}
#[test]
fn escape_refresh_accepts_prefix_array_reallocation_without_target_replacement() {
    let mut f = fixture();
    let original = capture(&f, None).unwrap();
    const NEW_ARRAY: u64 = I + 0x1c0000;
    standard_prefix(&mut f, 0x74);
    f.array(GFX + 0x68, NEW_ARRAY, &[SPRITE, PREFIX_RENDERER]);
    capture(&f, Some(&original)).unwrap();
    // Spare allocation capacity stays metadata: only the active payload is read.
    f.int(GFX + 0x70, 20_000);
    capture(&f, Some(&original)).unwrap();
    f.not_read(NEW_ARRAY + 16);
}
#[test]
fn escape_refresh_allows_current_ready_target_state_instead_of_freezing_hover() {
    let mut f = fixture();
    let original = capture(&f, None).unwrap();
    f.int(BUTTON + 0x9c, 2);
    capture(&f, Some(&original)).unwrap();
    f.byte(BUTTON + 0xcc, 0);
    assert!(capture(&f, Some(&original)).is_err());
}
#[test]
fn escape_refresh_rejects_new_eligible_escape_or_consuming_predecessor() {
    for consuming in [false, true] {
        let mut f = fixture();
        let original = capture(&f, None).unwrap();
        standard_prefix(&mut f, if consuming { 0x74 } else { 0x1b });
        if consuming {
            f.int(PREFIX_BUTTON + 0x64, 0x806);
        }
        assert!(capture(&f, Some(&original)).is_err());
    }
}
#[test]
fn escape_refresh_rejects_unknown_new_renderer_before_target_payload() {
    let mut f = fixture();
    let original = capture(&f, None).unwrap();
    f.array(GFX + 0x68, SOURCES, &[SPRITE, PREFIX_RENDERER]);
    f.word(PREFIX_RENDERER, B + 0xdeadbe0);
    assert!(capture(&f, Some(&original)).is_err());
    f.not_read(PREFIX_RENDERER + 0x138);
}
#[test]
fn escape_refresh_rejects_unknown_new_target_before_owner_or_gate_payload() {
    let mut f = fixture();
    let original = capture(&f, None).unwrap();
    standard_prefix(&mut f, 0x74);
    f.word(PREFIX_BUTTON, B + 0xdeadbe0);
    assert!(capture(&f, Some(&original)).is_err());
    f.not_read(PREFIX_BUTTON + 0x58);
    f.not_read(PREFIX_BUTTON + 0x9c);
}
#[test]
fn escape_refresh_checks_new_predecessor_own_links_and_gate_without_skipping() {
    for at in [
        PREFIX_BUTTON + 0x58,
        PREFIX_BUTTON + 0xd0,
        PREFIX_BUTTON + 0xd8,
        PREFIX_BUTTON + 0x178,
        PREFIX_RENDERER + 0x108,
    ] {
        let mut f = fixture();
        let original = capture(&f, None).unwrap();
        standard_prefix(&mut f, 0x74);
        f.word(at, FOREIGN);
        assert!(capture(&f, Some(&original)).is_err(), "{at:x}");
        if at == PREFIX_BUTTON + 0x178 {
            // New current predecessor types are inspected at their primary
            // only; unsupported/unbound parent payload is never followed.
            f.not_read(FOREIGN + 0x128);
        } else {
            f.not_read(FOREIGN);
        }
    }
}
#[test]
fn escape_refresh_rejects_malformed_full_collection_before_its_payload() {
    for (cap, count) in [(4097, 4097), (20_001, 1), (0, 1), (1, -1)] {
        let mut f = fixture();
        let original = capture(&f, None).unwrap();
        f.word(GFX + 0x68, FOREIGN);
        f.int(GFX + 0x70, cap);
        f.int(GFX + 0x74, count);
        assert!(capture(&f, Some(&original)).is_err());
        f.not_read(FOREIGN);
    }
}
#[test]
fn escape_refresh_requires_unique_current_named_renderer_in_full_collection() {
    for sources in [vec![SPRITE, SPRITE], vec![PREFIX_RENDERER]] {
        let mut f = fixture();
        let original = capture(&f, None).unwrap();
        f.array(GFX + 0x68, SOURCES, &sources);
        f.reads.borrow_mut().clear();
        assert!(capture(&f, Some(&original)).is_err());
        f.not_read(PREFIX_RENDERER);
    }
}
#[test]
fn escape_original_target_edges_refuse_before_replacement_payload() {
    for at in [
        GUI + 0x98,
        GFX,
        BUTTON,
        BUTTON + 0xd0,
        BUTTON + 0xd8,
        SPRITE,
        SPRITE + 0x138,
        SPRITE + 0x108,
        ROOT + 0x1238,
        POS,
        BUTTON + 0x178,
    ] {
        let mut f = fixture();
        let original = capture(&f, None).unwrap();
        f.word(at, FOREIGN);
        f.reads.borrow_mut().clear();
        assert!(capture(&f, Some(&original)).is_err(), "{at:x}");
        f.not_read(FOREIGN);
        f.not_read(SOURCES);
    }
}
#[test]
fn escape_target_renderer_same_family_replacement_is_not_adopted() {
    let mut f = fixture();
    let original = capture(&f, None).unwrap();
    f.word(BUTTON + 0xd8, PREFIX_RENDERER);
    f.word(PREFIX_RENDERER, B + 0x34bdb10);
    f.word(PREFIX_RENDERER + 0x138, BUTTON);
    f.word(PREFIX_RENDERER + 0x108, POS);
    f.array(GFX + 0x68, SOURCES, &[PREFIX_RENDERER]);
    f.reads.borrow_mut().clear();
    assert!(capture(&f, Some(&original)).is_err());
    f.not_read(PREFIX_RENDERER);
}
#[test]
fn escape_target_exact_renderer_primary_is_retained_across_supported_types() {
    let mut f = fixture();
    let original = capture(&f, None).unwrap();
    f.word(SPRITE, B + 0x34afed0);
    f.reads.borrow_mut().clear();
    assert!(capture(&f, Some(&original)).is_err());
    f.not_read(SPRITE + 0x138);
}
#[test]
fn escape_fresh_prefix_and_target_share_original_final_witnesses() {
    for at in [SOURCES, PREFIX_BUTTON + 0x64, PREFIX_KEYS] {
        let mut f = fixture();
        let original = capture(&f, None).unwrap();
        standard_prefix(&mut f, 0x74);
        f.tear = Some(at);
        f.reads.borrow_mut().clear();
        let w = Witnesses::new(&f);
        from_reader(&Reader::new(&w), B, I, Some(&original)).unwrap();
        assert_eq!(w.finish().unwrap_err().code, "torn_read", "{at:x}");
    }
}
#[test]
fn old_strict_t_observer_still_refuses_original_prefix_membership_changes() {
    let mut f = fixture();
    f.int(KEYS, 0x74);
    let original = shortcut::observe_key(
        &Reader::new(&f),
        B,
        GUI,
        BUTTON,
        ROOT,
        shortcut::PlainKey::T,
        None,
    )
    .unwrap();
    original.require_ready().unwrap();
    standard_prefix(&mut f, 0x1b);
    assert_eq!(
        shortcut::observe_key(
            &Reader::new(&f),
            B,
            GUI,
            BUTTON,
            ROOT,
            shortcut::PlainKey::T,
            Some(&original)
        )
        .unwrap_err()
        .message,
        "Original keyboard source membership changed before payload"
    );
}

#[test]
fn escape_refresh_accepts_known_renderer_without_gui_recipient_only() {
    let mut f = fixture();
    let original = capture(&f, None).unwrap();
    f.word(PREFIX_RENDERER, B + 0x34bdb10);
    f.word(PREFIX_RENDERER + 0x138, 0);
    f.array(GFX + 0x68, SOURCES, &[SPRITE, PREFIX_RENDERER]);
    capture(&f, Some(&original)).unwrap();
    // A null target does not turn an unknown renderer into a permitted miss.
    f.word(PREFIX_RENDERER, B + 0xdeadbe0);
    assert!(capture(&f, Some(&original)).is_err());
}
#[test]
fn escape_initial_bind_requires_current_ready_prefix_before_any_action() {
    let mut f = fixture();
    standard_prefix(&mut f, 0x1b);
    assert!(capture(&f, None).is_err());
    let mut f = fixture();
    f.array(GFX + 0x68, SOURCES, &[]);
    assert!(capture(&f, None).is_err());
}

#[test]
fn original_requested_button_primary_precedes_old_callback_payload() {
    let mut f = fixture();
    let original = capture(&f, None).unwrap();
    f.word(BUTTON, B + 0x34a81a8);
    f.reads.borrow_mut().clear();
    assert!(capture(&f, Some(&original)).is_err());
    f.not_read(BUTTON + 0x58);
    f.not_read(BUTTON + 0x70);
    f.not_read(FUNC + 8);
}
