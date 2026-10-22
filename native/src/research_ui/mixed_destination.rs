//! Read-only retained Research destination identity on reviewed Linux ELF
//! a4971c9e. This does not authorize closing, opening, selection or any input.
//! Canonical own Research, retained roots and production share one witnessed
//! process bracket. Unmaterialized roots are refused, never bound after exit.
use super::{closing, exact, UiContext, Witnesses};
use crate::{
    error::{Error, Result},
    game,
    memory::{Reader, StringAbi},
    platform::Process,
    research::ResearchContext,
    research_stage::PanelBinding,
};
use serde_json::{json, Value};

#[doc(hidden)]
pub struct Observation {
    pub research: closing::Observation,
    /// Own country, production, idler, registry, manager, production view/root.
    /// Private witness only; do not put addresses in public summaries.
    pub production_identity: [u64; 7],
}
fn refuse(message: &str) -> Error {
    Error::new("research_precondition", message)
}
fn pointer(p: u64) -> Result<()> {
    if !(0x10000..=0x7fff_ffff_0000).contains(&p) || p % 8 != 0 {
        return Err(refuse("Bounded materialized destination identity required"));
    }
    Ok(())
}

/// Static private identities only. Visibility, selection and Info state remain
/// the ordinary action observer's responsibility; this never grants input.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Identity {
    pub context: UiContext,
    pub roots: [u64; 4],
    pub facilities_parent: u64,
}

/// Retain the original static identities throughout Research panel/tree/Info
/// continuation. Canonical assignments and campaign context are witnessed here,
/// but no mutable selection is promoted to a new original binding.
pub fn observe_identity(process: &dyn Process) -> Result<Identity> {
    if process.string_abi() != StringAbi::LibStdCxx {
        return Err(Error::new(
            "capability_unavailable",
            "Research continuation identity requires reviewed Linux build",
        ));
    }
    process.begin_snapshot()?;
    let witnesses = Witnesses::new(process);
    let r = Reader::new(&witnesses);
    let base = process.image_base();
    witnesses.section("research_identity_own");
    let (_, own) = crate::research::observe_from_reader(&r, base, 0, 1, true)?;
    let own = own.ok_or_else(|| refuse("Own continuation campaign context unavailable"))?;
    witnesses.section("research_identity_retained");
    let identity = continuation_identity(&r, base, own)?;
    witnesses.finish()?;
    process.check()?;
    Ok(identity)
}

fn continuation_identity(r: &Reader<'_>, base: u64, own: ResearchContext) -> Result<Identity> {
    let [country, status] = own.own;
    let [_, idler, _] = own.campaign.anchors;
    let registry = r.u64(idler + 0x4f8)?;
    pointer(registry)?;
    exact(r, registry, base + 0x42736f8)?;
    let manager = r.u64(idler + 0x6b8)?;
    pointer(manager)?;
    if r.u64(manager + 0x180)? != idler || r.u64(base + 0x45778e0)? != registry {
        return Err(refuse("Own continuation manager/input registry changed"));
    }
    if !matches!(r.i32(manager + 0x3d8)?, 2 | 5 | 6 | 23) {
        return Err(refuse("Unsupported Research continuation identity mode"));
    }
    let view = r.u64(manager + 0xf0)?;
    let tree = r.u64(manager + 0xf8)?;
    pointer(view)?;
    pointer(tree)?;
    exact(r, view, base + 0x424dc38)?;
    exact(r, tree, base + 0x424de40)?;
    if r.u64(view + 0x2f0)? != idler
        || r.u64(tree + 0x2f0)? != idler
        || r.i32(tree + 0xe24)? != 6
        || r.i32(view + 0x300)? != 0
    {
        return Err(refuse("Own continuation Research views or tab changed"));
    }
    let displayed = r.i32(tree + 0xe20)?;
    if displayed != 0 && displayed != r.i32(status + 0x138)? {
        return Err(refuse("Foreign or unknown continuation Research country"));
    }
    // Unlike the departure helper, either Research root may be shown here.
    // Exact materialized root types, registry, transitions and named facilities
    // joins are still mandatory. No Info/control or event-routing traversal.
    let root = r.u64(view + 0x2f8)?;
    let tree_root = r.u64(tree + 0x2f8)?;
    closing::settled(r, base, root, registry)?;
    closing::settled(r, base, tree_root, registry)?;
    let named = |parent: u64, key: &str| -> Result<u64> {
        crate::gui_registry::lookup(
            r,
            parent + 0xbc8,
            &super::array(r, parent + 0xbb0, 128)?,
            key,
            128,
        )?
        .ok_or_else(|| refuse("Named continuation child unavailable"))
    };
    let facilities_parent = r.u64(view + 0x910)?;
    if named(root, "facilities")? != facilities_parent {
        return Err(refuse("Continuation facilities parent named join changed"));
    }
    closing::settled(r, base, facilities_parent, registry)?;
    let facilities = r.u64(view + 0x900)?;
    pointer(facilities)?;
    exact(r, facilities, base + 0x42f2708)?;
    if r.u64(facilities + 0x68)? != idler || r.u64(facilities + 0x60)? != facilities_parent {
        return Err(refuse("Continuation facilities view owner changed"));
    }
    let facilities_root = r.u64(facilities + 0x70)?;
    if named(facilities_parent, "facilities_view_Instance")? != facilities_root {
        return Err(refuse("Continuation facilities root named join changed"));
    }
    closing::settled(r, base, facilities_root, registry)?;
    let identity = Identity {
        context: UiContext {
            campaign: own.campaign,
            anchors: [country, status, registry, manager, view, tree],
            slots: own.slots,
        },
        roots: [root, tree_root, facilities, facilities_root],
        facilities_parent,
    };
    let pointers: Vec<_> = identity
        .context
        .anchors
        .iter()
        .chain(identity.roots.iter())
        .chain([&identity.facilities_parent])
        .chain(identity.context.slots.iter())
        .copied()
        .collect();
    for p in &pointers {
        pointer(*p)?;
    }
    if !(1..=32).contains(&identity.context.slots.len())
        || pointers
            .iter()
            .copied()
            .collect::<std::collections::BTreeSet<_>>()
            .len()
            != pointers.len()
    {
        return Err(refuse("Complete distinct continuation identities required"));
    }
    Ok(identity)
}

/// Target slot is exact and empty. The mode2 source must be literally shown;
/// idle23 must have the same retained source hidden. Research is hidden in both.
/// No transport, screenshots, cursor operations, retries or input lease occurs.
pub fn observe(process: &dyn Process, target_slot: u32) -> Result<Observation> {
    if target_slot >= 32 {
        return Err(refuse("Bounded exact destination slot required"));
    }
    // Refuse before build-specific reads. The installed Process validates the
    // exact executable build; fixture processes explicitly provide this ABI.
    if process.string_abi() != StringAbi::LibStdCxx {
        return Err(Error::new(
            "capability_unavailable",
            "Mixed Research destination requires reviewed Linux build",
        ));
    }
    process.begin_snapshot()?;
    let witnesses = Witnesses::new(process);
    let r = Reader::new(&witnesses);
    let base = process.image_base();
    witnesses.section("mixed_destination_own_research");
    let (canonical, own) = crate::research::observe_from_reader(&r, base, 0, 1, true)?;
    let own = own.ok_or_else(|| refuse("Own destination campaign context unavailable"))?;
    witnesses.section("mixed_destination_own_production");
    let (country, production) = crate::economy::own_production(&r, base)?;
    if country != own.own[0] {
        return Err(refuse("Production and Research own country differ"));
    }
    witnesses.section("mixed_destination_retained");
    let observation = retained(&r, base, own, canonical, production, target_slot)?;
    let [country, _, registry, manager, _, _] = observation.research.context.anchors;
    let [gs, idler, _] = observation.research.context.campaign.anchors;
    witnesses.section("mixed_destination_engine");
    let engine =
        crate::production_panel::engine(&r, base, gs, idler, manager, registry, &mut Vec::new())?;
    witnesses.section("mixed_destination_windows");
    let common =
        crate::construction::controls::blockers(&r, base, country, idler, manager, registry)?;
    witnesses.section("mixed_destination_gui");
    let gui_clear = r.byte(registry + 0x150)? == 0
        && r.byte(registry + 0x12)? == 0
        && r.u64(registry + 0x158)? == 0
        && r.u64(registry + 0x230)? == 0;
    if engine["conservative_engine_clear"] != true
        || common["available"] != true
        || common["conservative_scoped_clear"] != true
        || !gui_clear
    {
        return Err(refuse(
            "Retained destination engine, own windows or GUI gates refused",
        ));
    }
    witnesses.finish()?;
    process.check()?;
    Ok(observation)
}

/// Caller supplies canonical own Research and the own-production join from the
/// same Reader. Kept separate so retained identity/torn reads have small tests.
fn retained(
    r: &Reader<'_>,
    base: u64,
    own: ResearchContext,
    canonical: Value,
    production: u64,
    target_slot: u32,
) -> Result<Observation> {
    let slots = canonical["slots"]
        .as_array()
        .filter(|s| !s.is_empty() && s.len() <= 32 && s.len() == own.slots.len())
        .ok_or_else(|| refuse("Complete canonical own destination slots required"))?;
    if canonical["owned"] != true
        || canonical["build_id"] != game::BUILD_ID
        || slots.iter().enumerate().any(|(index, s)| {
            s["slot"].as_u64() != Some(index as u64)
                || !(s
                    .get("researching")
                    .is_some_and(|v| v.is_null() || v.is_string()))
        })
        || slots
            .get(target_slot as usize)
            .and_then(|s| s.get("researching"))
            != Some(&Value::Null)
    {
        return Err(refuse("Exact empty canonical destination slot required"));
    }
    let [country, status] = own.own;
    let [_, idler, _] = own.campaign.anchors;
    let registry = r.u64(idler + 0x4f8)?;
    pointer(registry)?;
    exact(r, registry, base + 0x42736f8)?;
    let manager = r.u64(idler + 0x6b8)?;
    pointer(manager)?;
    if r.u64(manager + 0x180)? != idler || r.u64(base + 0x45778e0)? != registry {
        return Err(refuse("Own destination manager/input registry changed"));
    }
    let view = r.u64(manager + 0xf0)?;
    let tree = r.u64(manager + 0xf8)?;
    for p in [view, tree] {
        pointer(p)?;
    }
    exact(r, view, base + 0x424dc38)?;
    exact(r, tree, base + 0x424de40)?;
    if r.u64(view + 0x2f0)? != idler || r.u64(tree + 0x2f0)? != idler || r.i32(tree + 0xe24)? != 6 {
        return Err(refuse("Own destination Research views changed"));
    }
    let displayed = r.i32(tree + 0xe20)?;
    if displayed != 0 && displayed != r.i32(status + 0x138)? {
        return Err(refuse("Foreign or unknown displayed Research country"));
    }
    let tab = r.i32(view + 0x300)?;
    if tab != 0 {
        return Err(refuse("Retained destination requires Research tab0"));
    }
    let selected = r.u64(tree + 0xe08)?;
    let selected_index = if selected == 0 {
        None
    } else {
        Some(
            own.slots
                .iter()
                .position(|p| *p == selected)
                .ok_or_else(|| refuse("Destination selected slot outside own status"))?,
        )
    };
    let (roots, facilities_parent, panel_visible) =
        closing::roots(r, base, view, tree, idler, registry)?;
    if panel_visible {
        return Err(refuse("Retained Research destination must be hidden"));
    }
    let (folder_index, folder_windows, selected_folder) =
        crate::category_ui::closing_selection(r, base, tree, roots[1], registry)?;
    let production_view = r.u64(manager + 0xd8)?;
    pointer(production_view)?;
    exact(r, production_view, base + 0x4314180)?;
    if r.u64(production_view + 0x2f0)? != idler {
        return Err(refuse("Destination source production view owner changed"));
    }
    let production_root = r.u64(production_view + 0x2f8)?;
    let production_visible = closing::settled(r, base, production_root, registry)?;
    let mode = r.i32(manager + 0x3d8)?;
    if !matches!((mode, production_visible), (2, true) | (23, false)) {
        return Err(refuse(
            "Destination requires shown production2 or settled idle23",
        ));
    }
    let production_identity = [
        country,
        production,
        idler,
        registry,
        manager,
        production_view,
        production_root,
    ];
    let context = UiContext {
        campaign: own.campaign,
        anchors: [country, status, registry, manager, view, tree],
        slots: own.slots,
    };
    let binding = PanelBinding {
        anchors: context.anchors,
        slots: context.slots.clone(),
        roots,
        facilities_parent,
        selected_slot_pointer: selected,
        folder_index,
        folder_windows,
        tab,
    };
    // Explicit pointer bounds and uniqueness stop degenerate injected proofs;
    // native field/type/owner joins above remain independently mandatory.
    let identities: Vec<_> = binding
        .anchors
        .iter()
        .chain(binding.roots.iter())
        .chain([&binding.facilities_parent])
        .chain(binding.slots.iter())
        .copied()
        .collect();
    for p in identities.iter().chain(production_identity.iter()) {
        pointer(*p)?;
    }
    if identities
        .iter()
        .copied()
        .collect::<std::collections::BTreeSet<_>>()
        .len()
        != identities.len()
        || production_identity
            .iter()
            .copied()
            .collect::<std::collections::BTreeSet<_>>()
            .len()
            != 7
    {
        return Err(refuse("Distinct own destination identities required"));
    }
    let body = json!({"owned":true,"build_id":game::BUILD_ID,
        "canonical_research":canonical,"own_slot_count":context.slots.len(),
        "selected_slot":selected_index,"active_mode_raw":mode,
        "ordinary_tree":{"materialized":true,"visible_predicate":false},
        "panel":{"materialized":true,"visible_predicate":false},
        "technology_info":{"visible_count":0,"windows":[]},
        "input_authorized":false,"observation_paused_game":false});
    Ok(Observation {
        research: closing::Observation {
            body,
            context,
            binding,
            selected_folder,
        },
        production_identity,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::memory::ReadMemory;
    use std::{cell::Cell, collections::BTreeMap};
    const BASE: u64 = 0x10000;
    const GS: u64 = 0x100000;
    const IDLER: u64 = 0x110000;
    const COUNTRY: u64 = 0x120000;
    const STATUS: u64 = 0x130000;
    const PRODUCTION: u64 = 0x140000;
    const REG: u64 = 0x150000;
    const MANAGER: u64 = 0x160000;
    const VIEW: u64 = 0x170000;
    const TREE: u64 = 0x180000;
    const PVIEW: u64 = 0x190000;
    const PROOT: u64 = 0x1a0000;
    const SLOT0: u64 = 0x1b0000;
    const SLOT1: u64 = 0x1c0000;
    const ROOT: u64 = 0x200000;
    const TROOT: u64 = 0x210000;
    const PARENT: u64 = 0x220000;
    const FACILITIES: u64 = 0x230000;
    const FROOT: u64 = 0x240000;

    #[derive(Default)]
    struct Fixture {
        bytes: BTreeMap<u64, u8>,
        mutate_at_finish: Option<u64>,
        reads: Cell<usize>,
        abi: Option<StringAbi>,
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
        fn window(&mut self, p: u64, shown: bool) {
            self.word(p, BASE + 0x4347d58);
            self.word(p + 0x98, REG);
            self.put(p + 0xa5, &[if shown { 8 } else { 0 }]);
            self.number(p + 0xeb0, 0);
            self.number(p + 0xef8, 0);
        }
        fn array(&mut self, p: u64, data: u64, count: i32) {
            self.word(p, data);
            self.number(p + 8, count);
            self.number(p + 12, count);
        }
        fn named_child(&mut self, p: u64, child: u64, name: &str) {
            let values = p + 0x2000;
            let names = p + 0x2100;
            self.array(p + 0xbb0, values, 1);
            self.word(values, child);
            self.array(p + 0xbc8, names, 1);
            self.put(names, &[0; 32]);
            let text = if name.len() <= 15 {
                names + 16
            } else {
                p + 0x2200
            };
            self.word(names, text);
            self.word(names + 8, name.len() as u64);
            self.put(text, name.as_bytes());
        }
        fn ready(mode: i32) -> Self {
            let mut f = Self::default();
            f.word(IDLER + 0x4f8, REG);
            f.word(REG, BASE + 0x42736f8);
            f.word(IDLER + 0x6b8, MANAGER);
            f.word(MANAGER + 0x180, IDLER);
            f.word(BASE + 0x45778e0, REG);
            f.word(MANAGER + 0xf0, VIEW);
            f.word(MANAGER + 0xf8, TREE);
            f.word(VIEW, BASE + 0x424dc38);
            f.word(TREE, BASE + 0x424de40);
            f.word(VIEW + 0x2f0, IDLER);
            f.word(TREE + 0x2f0, IDLER);
            f.number(TREE + 0xe24, 6);
            f.number(TREE + 0xe20, 1);
            f.number(STATUS + 0x138, 1);
            f.number(VIEW + 0x300, 0);
            f.word(TREE + 0xe08, SLOT1);
            f.word(VIEW + 0x2f8, ROOT);
            f.word(TREE + 0x2f8, TROOT);
            f.word(VIEW + 0x910, PARENT);
            f.word(VIEW + 0x900, FACILITIES);
            f.word(FACILITIES, BASE + 0x42f2708);
            f.word(FACILITIES + 0x68, IDLER);
            f.word(FACILITIES + 0x60, PARENT);
            f.word(FACILITIES + 0x70, FROOT);
            for root in [ROOT, TROOT, PARENT, FROOT] {
                f.window(root, false);
            }
            f.named_child(ROOT, PARENT, "facilities");
            f.named_child(PARENT, FROOT, "facilities_view_Instance");
            f.array(TREE + 0xd78, 0, 0);
            f.array(TREE + 0xd90, 0, 0);
            f.number(TREE + 0xe10, 0);
            f.word(MANAGER + 0xd8, PVIEW);
            f.word(PVIEW, BASE + 0x4314180);
            f.word(PVIEW + 0x2f0, IDLER);
            f.word(PVIEW + 0x2f8, PROOT);
            f.window(PROOT, mode == 2);
            f.number(MANAGER + 0x3d8, mode);
            f
        }
        fn sample(&self, canonical: Value, slot: u32) -> Result<Observation> {
            let witnesses = Witnesses::new(self);
            let r = Reader::new(&witnesses);
            let out = retained(&r, BASE, own(), canonical, PRODUCTION, slot)?;
            witnesses.finish()?;
            self.check()?;
            Ok(out)
        }
    }
    impl ReadMemory for Fixture {
        fn string_abi(&self) -> StringAbi {
            self.abi.unwrap_or(StringAbi::LibStdCxx)
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
                    if let Some(a) = self.mutate_at_finish {
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
            campaign: game::ClockContext {
                playthrough_id: "test".into(),
                player: "GER".into(),
                anchors: [GS, IDLER, 0x300000],
                date_raw: 706000,
                difficulty: 1,
                historical_raw: 1,
                paused: false,
                speed: 3,
            },
            own: [COUNTRY, STATUS],
            slots: vec![SLOT0, SLOT1],
        }
    }
    fn canonical() -> Value {
        json!({"owned":true,"build_id":game::BUILD_ID,"slots":[
            {"slot":0,"researching":null},{"slot":1,"researching":"support_weapons"}]})
    }
    #[test]
    fn retained_identity_is_exact_across_production_departure() {
        let before = Fixture::ready(2).sample(canonical(), 0).unwrap();
        let after = Fixture::ready(23).sample(canonical(), 0).unwrap();
        assert_eq!(before.research.binding, after.research.binding);
        assert_eq!(before.production_identity, after.production_identity);
        assert_eq!(before.research.context, after.research.context);
        assert_eq!(
            before.research.body["canonical_research"],
            after.research.body["canonical_research"]
        );
        assert_eq!(before.research.body["selected_slot"], 1);
        assert_eq!(before.research.body["active_mode_raw"], 2);
        assert_eq!(after.research.body["active_mode_raw"], 23);
        assert_eq!(
            before.research.binding.roots,
            [ROOT, TROOT, FACILITIES, FROOT]
        );
        assert_eq!(before.research.binding.selected_slot_pointer, SLOT1);
        assert_eq!(before.research.body["input_authorized"], false);
        assert!(!before.research.body.to_string().contains(&ROOT.to_string()));
    }
    #[test]
    fn forbidden_modes_and_inconsistent_source_visibility_refuse() {
        for mode in [-1, 0, 1, 3, 5, 6, 13, 22, 24, i32::MAX] {
            assert_eq!(
                Fixture::ready(mode)
                    .sample(canonical(), 0)
                    .err()
                    .unwrap()
                    .code,
                "research_precondition"
            );
        }
        for (mode, shown) in [(2, false), (23, true)] {
            let mut f = Fixture::ready(mode);
            f.window(PROOT, shown);
            assert!(f.sample(canonical(), 0).is_err());
        }
    }
    #[test]
    fn exact_target_slot_and_complete_canonical_slots_required() {
        for slot in [1, 2, 32, u32::MAX] {
            assert!(Fixture::ready(2).sample(canonical(), slot).is_err());
        }
        for pointer in [
            "/slots/0/researching",
            "/slots/0/slot",
            "/slots/1/researching",
            "/owned",
            "/build_id",
        ] {
            let mut c = canonical();
            *c.pointer_mut(pointer).unwrap() = json!({"unknown":true});
            assert!(Fixture::ready(2).sample(c, 0).is_err(), "{pointer}");
        }
        let mut c = canonical();
        c["slots"][0].as_object_mut().unwrap().remove("researching");
        assert!(Fixture::ready(2).sample(c, 0).is_err());
    }
    #[test]
    fn original_roots_selection_owners_and_array_bounds_are_mandatory() {
        for (address, value) in [
            (VIEW + 0x2f8, 0),
            (TREE + 0x2f8, 0),
            (FACILITIES + 0x70, 0),
            (VIEW + 0x910, PARENT + 8),
            (FACILITIES + 0x60, PARENT + 8),
            (FACILITIES + 0x68, IDLER + 8),
            (PVIEW + 0x2f0, IDLER + 8),
            (PROOT + 0x98, REG + 8),
            (MANAGER + 0x180, IDLER + 8),
            (BASE + 0x45778e0, REG + 8),
            (TREE + 0xe08, SLOT1 + 8),
        ] {
            let mut f = Fixture::ready(2);
            f.word(address, value);
            assert!(f.sample(canonical(), 0).is_err(), "{address:x}");
        }
        for (address, value) in [
            (VIEW + 0x300, 1),
            (TREE + 0xe20, 2),
            (TREE + 0xe10, 1),
            (TREE + 0xd78 + 8, 129),
            (ROOT + 0xeb0, 1),
            (PROOT + 0xef8, 2),
        ] {
            let mut f = Fixture::ready(2);
            f.number(address, value);
            assert!(f.sample(canonical(), 0).is_err(), "{address:x}");
        }
        for root in [ROOT, TROOT, FROOT] {
            let mut f = Fixture::ready(2);
            f.window(root, true);
            assert!(f.sample(canonical(), 0).is_err());
        }
    }
    #[test]
    fn unstable_retained_bindings_discard_entire_witness() {
        for address in [
            IDLER + 0x6b8,
            MANAGER + 0xf0,
            MANAGER + 0xf8,
            VIEW + 0x2f8,
            TREE + 0x2f8,
            VIEW + 0x910,
            VIEW + 0x900,
            FACILITIES + 0x70,
            TREE + 0xe08,
            TREE + 0xd78,
            VIEW + 0x300,
            MANAGER + 0xd8,
            PVIEW + 0x2f8,
            MANAGER + 0x3d8,
            PROOT + 0xa5,
        ] {
            let mut f = Fixture::ready(2);
            f.mutate_at_finish = Some(address);
            let error = f.sample(canonical(), 0).err().unwrap();
            assert_eq!(error.code, "torn_read", "{address:x}: {error:?}");
        }
    }
    #[test]
    fn other_abi_and_out_of_bounds_target_refuse_before_memory_reads() {
        let f = Fixture {
            abi: Some(StringAbi::LibCxx),
            ..Fixture::default()
        };
        assert_eq!(observe(&f, 0).err().unwrap().code, "capability_unavailable");
        assert_eq!(f.reads.get(), 0);
        assert_eq!(observe(&f, 32).err().unwrap().code, "research_precondition");
        assert_eq!(f.reads.get(), 0);
    }

    fn identity_sample(f: &Fixture) -> Result<Identity> {
        let witnesses = Witnesses::new(f);
        let identity = continuation_identity(&Reader::new(&witnesses), BASE, own())?;
        witnesses.finish()?;
        Ok(identity)
    }
    #[test]
    fn continuation_modes_keep_original_static_identity_with_shown_tree_or_panel() {
        let original = Fixture::ready(2)
            .sample(canonical(), 0)
            .unwrap()
            .research
            .binding;
        for mode in [2, 5, 6, 23] {
            let mut f = Fixture::ready(mode);
            f.window(ROOT, mode == 5);
            f.window(TROOT, mode == 6);
            let identity = identity_sample(&f).unwrap();
            assert_eq!(identity.context.anchors, original.anchors);
            assert_eq!(identity.context.slots, original.slots);
            assert_eq!(identity.roots, original.roots);
            assert_eq!(identity.facilities_parent, original.facilities_parent);
            // The identity reader never traverses or judges Info/control state;
            // the normal action observer proves those action preconditions.
        }
    }
    #[test]
    fn continuation_forbidden_modes_wrong_owners_and_named_roots_refuse() {
        for mode in [-1, 0, 1, 3, 13, 22, 24, i32::MAX] {
            assert!(identity_sample(&Fixture::ready(mode)).is_err());
        }
        for (address, value) in [
            (VIEW + 0x2f8, 0),
            (TREE + 0x2f8, 0),
            (VIEW + 0x910, PARENT + 8),
            (FACILITIES + 0x70, FROOT + 8),
            (FACILITIES + 0x68, IDLER + 8),
            (TREE + 0x2f0, IDLER + 8),
            (ROOT + 0x98, REG + 8),
            (MANAGER + 0x180, IDLER + 8),
            (BASE + 0x45778e0, REG + 8),
        ] {
            let mut f = Fixture::ready(6);
            f.word(address, value);
            assert!(identity_sample(&f).is_err(), "{address:x}");
        }
    }
    #[test]
    fn continuation_changed_identity_is_reported_and_torn_identity_never_returns() {
        let original = identity_sample(&Fixture::ready(6)).unwrap();
        let mut changed = Fixture::ready(6);
        let replacement = 0x250000;
        changed.word(TREE + 0x2f8, replacement);
        changed.window(replacement, true);
        assert_ne!(identity_sample(&changed).unwrap().roots, original.roots);
        for address in [
            IDLER + 0x6b8,
            MANAGER + 0xf0,
            MANAGER + 0xf8,
            VIEW + 0x2f8,
            TREE + 0x2f8,
            VIEW + 0x910,
            VIEW + 0x900,
            FACILITIES + 0x70,
            ROOT + 0x2000,
            PARENT + 0x2000,
            MANAGER + 0x3d8,
            TROOT + 0xa5,
        ] {
            let mut f = Fixture::ready(6);
            f.mutate_at_finish = Some(address);
            assert_eq!(
                identity_sample(&f).err().unwrap().code,
                "torn_read",
                "{address:x}"
            );
        }
        let f = Fixture {
            abi: Some(StringAbi::LibCxx),
            ..Fixture::default()
        };
        assert_eq!(
            observe_identity(&f).err().unwrap().code,
            "capability_unavailable"
        );
        assert_eq!(f.reads.get(), 0);
    }
}
