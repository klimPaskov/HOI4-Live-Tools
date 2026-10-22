//! Same-snapshot own production identity and exact Linux ordinary UI controls.
//! This observer never opens the panel, moves the pointer or grants input.
use super::*;
use crate::memory::StringAbi;

/// Accept only the exact opaque representation emitted by production_lines.
pub fn validate_target(handle: &str, equipment_id: &str) -> Result<u64> {
    let parsed = handle
        .strip_prefix("0x")
        .filter(|s| {
            s.len() == 16
                && s.bytes()
                    .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        })
        .and_then(|s| u64::from_str_radix(s, 16).ok())
        .filter(|n| matches!(*n as u32, 0x38 | 0x39));
    if equipment_id.is_empty()
        || equipment_id.len() > 128
        || !equipment_id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_')
    {
        return Err(Error::new(
            "invalid_arguments",
            "Canonical equipment_id required",
        ));
    }
    parsed.ok_or_else(|| {
        Error::new(
            "invalid_arguments",
            "Exact full production line_handle required",
        )
    })
}

pub fn tool() -> Value {
    json!({"name":"hoi4_production_controls",
        "description":"Linux read-only preview: joins an exact own current production line_handle and base equipment_id to its production row, named one-factory add/subtract controls, ordinary callback and local blockers. Optional same-snapshot renderer geometry. Never opens a panel, moves the cursor, pauses or authorizes input. Full line collection is validated before reading UI. Factory buttons clamp to at least one and depend on keyboard modifiers. Live validation pending.",
        "inputSchema":{"type":"object","properties":{
            "line_handle":{"type":"string","pattern":"^0x[0-9a-f]{16}$"},
            "equipment_id":{"type":"string","minLength":1,"maxLength":128},
            "include_geometry":{"type":"boolean","default":false}},
            "required":["line_handle","equipment_id"],"additionalProperties":false},
        "annotations":{"readOnlyHint":true}})
}

pub fn snapshot(process: &dyn Process, handle: &str, key: &str, geometry: bool) -> Result<Value> {
    Ok(observe(process, handle, key, Some(geometry))?.value)
}
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
pub(crate) struct Observation {
    pub(crate) value: Value,
    pub(crate) campaign: game::ClockContext,
    pub(crate) line: crate::production_step::LineIdentity,
    pub(crate) requested: i32,
    pub(crate) scope: Vec<u64>,
    pub(crate) geometry: std::collections::BTreeMap<String, Vec<(u64, Vec<u8>)>>,
    /// Complete own canonical identities and requested counts, in collection
    /// order. None means an unsupported/unproved member; never infer unchanged.
    #[cfg_attr(not(any(target_os = "macos", test)), allow(dead_code))]
    pub(crate) allocations: Option<Vec<(crate::production_step::LineIdentity, i32)>>,
}
/// None requests canonical own-line readback without accessing any GUI object.
pub(crate) fn observe(
    process: &dyn Process,
    handle: &str,
    key: &str,
    ui: Option<bool>,
) -> Result<Observation> {
    let handle = validate_target(handle, key)?;
    let start = Instant::now();
    process.begin_snapshot()?;
    let witnesses = Witnesses::new(process);
    let reader = Reader::new(&witnesses);
    if ui.is_some() && reader.string_abi() != StringAbi::LibStdCxx {
        return Err(Error::new(
            "capability_unavailable",
            "Production control layout requires the reviewed Linux build",
        ));
    }
    let base = process.image_base();
    let (country, production) = economy::own_production(&reader, base)?;
    let campaign = game::context_from_reader(&reader, base)?;
    let collection = collection(&reader, base, production)?;
    let selected = collection
        .known
        .iter()
        .find(|line| line.handle == handle)
        .ok_or_else(|| {
            Error::new(
                "production_target_unavailable",
                "Full handle absent from current own equipment lines",
            )
        })?;
    if selected.equipment_id.as_deref() != Some(key) || !selected.counters_valid {
        return Err(Error::new(
            "production_target_unavailable",
            "Own line requires matching canonical equipment and bounded counters",
        ));
    }
    let target = TargetLine {
        address: selected.address,
        handle,
        naval: selected.naval,
        equipment_id: key.to_owned(),
        requested: selected.requested,
    };
    // Reuse the positively validated equipment join. Re-reading line+88 and
    // following a newly changed pointer here would traverse an unproved object
    // before the final witness could reject the replacement.
    let [variant, base_type] = selected.equipment_identity.ok_or_else(|| {
        Error::new(
            "production_target_unavailable",
            "Exact own equipment identity unavailable",
        )
    })?;
    let line = crate::production_step::LineIdentity {
        country,
        production,
        line: selected.address,
        variant,
        base_type,
        handle,
        equipment_id: key.into(),
    };
    let allocations = if collection.known.len() == collection.entries.len() {
        collection
            .known
            .iter()
            .map(|entry| {
                let [variant, base_type] = entry.equipment_identity?;
                entry.counters_valid.then_some((
                    crate::production_step::LineIdentity {
                        country,
                        production,
                        line: entry.address,
                        variant,
                        base_type,
                        handle: entry.handle,
                        equipment_id: entry.equipment_id.clone()?,
                    },
                    entry.requested,
                ))
            })
            .collect()
    } else {
        None
    };
    let (mut value, mut scope, geometry) = match ui {
        Some(include) => {
            let o = controls::observe(&reader, base, &target, include)?;
            (o.value, o.scope, o.geometry)
        }
        None => (json!({}), Vec::new(), std::collections::BTreeMap::new()),
    };
    if ui == Some(true) {
        let idler = campaign.anchors[1];
        let registry = reader.u64(idler + 0x4f8)?;
        let manager = reader.u64(idler + 0x6b8)?;
        if reader.u64(base + 0x45778e0)? != registry {
            return Err(Error::new(
                "invalid_production_controls",
                "Input GUI must equal own production registry",
            ));
        }
        value["common_blockers"] = crate::construction::controls::blockers(
            &reader, base, country, idler, manager, registry,
        )?;
        value["engine_gates"] = crate::production_panel::engine(
            &reader,
            base,
            campaign.anchors[0],
            idler,
            manager,
            registry,
            &mut scope,
        )?;
        value["common_gui_gates_clear"] = json!(
            reader.byte(registry + 0x150)? == 0
                && reader.byte(registry + 0x12)? == 0
                && reader.u64(registry + 0x158)? == 0
                && reader.u64(registry + 0x230)? == 0
        );
        value["input_geometry"] = crate::gui::input_geometry_from_reader(&reader, base)?;
    }
    let (end_calls, end_bytes) = witnesses.finish()?;
    process.check()?;
    value["source"] = json!("native own production controls");
    value["build_id"] = json!(game::layout(&reader).build_id);
    value["player"] = json!(campaign.player);
    value["playthrough_id"] = json!(campaign.playthrough_id);
    value["date_raw"] = json!(campaign.date_raw);
    value["line_handle"] = json!(format!("0x{handle:016x}"));
    value["equipment_id"] = json!(key);
    value["requested_factories"] = json!(selected.requested);
    value["line_kind"] = json!(if selected.naval { "naval" } else { "military" });
    value["collection_count"] = json!(collection.entries.len());
    value["input_authorized"] = json!(false);
    value["observation_paused_game"] = json!(false);
    value["live_validation"] = json!("pending");
    value["freshness"] = json!({"elapsed_ms":start.elapsed().as_secs_f64()*1000.0,
        "read_calls_kind":"logical_read_requests", "read_calls":reader.calls.get()+end_calls as u64,
        "read_bytes":reader.bytes.get()+end_bytes});
    Ok(Observation {
        value,
        campaign,
        line,
        requested: selected.requested,
        scope,
        geometry,
        allocations,
    })
}

#[cfg(test)]
mod mac_canonical_tests {
    use super::*;
    use crate::memory::ReadMemory;
    use std::{
        cell::{Cell, RefCell},
        collections::BTreeMap,
    };

    const IMAGE: u64 = 0x1_0000_0000;
    const GS: u64 = 0x100000;
    const IDLER: u64 = 0x110000;
    const COUNTRY: u64 = 0x120000;
    const RESEARCH: u64 = 0x130000;
    const FOCUS: u64 = 0x140000;
    const PRODUCTION: u64 = 0x150000;
    const LINE: u64 = 0x160000;
    const OTHER: u64 = 0x170000;
    const VARIANT: u64 = 0x180000;
    const TYPE: u64 = 0x190000;
    const EQUIPMENT_DB: u64 = 0x1a0000;
    const TAGS: u64 = 0x1b0000;
    const ARRAY: u64 = 0x1c0000;
    const KEY: &str = "infantry_equipment_1";
    const HANDLE: u64 = 0x0000_0001_0000_0038;
    struct Mac {
        bytes: BTreeMap<u64, u8>,
        reads: RefCell<Vec<(u64, usize)>>,
        final_read: Cell<bool>,
        fault: Option<u64>,
    }
    impl Mac {
        fn put(&mut self, address: u64, bytes: &[u8]) {
            self.bytes.extend(
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
        fn array(&mut self, address: u64, payload: u64, entries: &[u64]) {
            self.word(address, payload);
            self.int(address + 8, entries.len() as i32);
            self.int(address + 12, entries.len() as i32);
            for (index, entry) in entries.iter().enumerate() {
                self.word(payload + index as u64 * 8, *entry);
            }
        }
        fn string(&mut self, address: u64, value: &str) {
            assert!(value.len() <= 22);
            let mut bytes = [0; 24];
            bytes[0] = (value.len() * 2) as u8;
            bytes[1..1 + value.len()].copy_from_slice(value.as_bytes());
            self.put(address, &bytes);
        }
        fn rtti(&mut self, table: u64, name: &str, ordinal: u64) {
            let info = 0x500000 + ordinal * 0x100;
            self.word(IMAGE + table - 8, info);
            self.word(info + 8, info + 0x40);
            self.put(info + 0x40, format!("{}{name}\0", name.len()).as_bytes());
        }
        fn new() -> Self {
            let mut f = Self {
                bytes: BTreeMap::new(),
                reads: RefCell::new(Vec::new()),
                final_read: Cell::new(false),
                fault: None,
            };
            for (address, value) in [
                (IMAGE + 0x3519740, GS),
                (IMAGE + 0x35196c0, IDLER),
                (IMAGE + 0x34d0de0, 0x1d0000),
                (IMAGE + 0x3506580, TAGS),
                (GS, IMAGE + 0x3306260),
                (IDLER, IMAGE + 0x3308f90),
                (COUNTRY, IMAGE + 0x337bc38),
                (COUNTRY + 0xd08, RESEARCH),
                (COUNTRY + 0xd10, PRODUCTION),
                (COUNTRY + 0x1108, FOCUS),
                (RESEARCH, IMAGE + 0x339e4e8),
                (RESEARCH + 0x130, COUNTRY),
                (FOCUS, IMAGE + 0x33865d0),
                (FOCUS + 0x10, 0),
                (PRODUCTION, IMAGE + 0x338b678),
                (PRODUCTION + 0x290, COUNTRY),
                (IMAGE + 0x34d0be0, EQUIPMENT_DB),
                (EQUIPMENT_DB, IMAGE + 0x3377a90),
                (TYPE, IMAGE + 0x3379328),
            ] {
                f.word(address, value);
            }
            f.int(GS + 0x428, crate::calendar::encode(1936, 1, 1).unwrap());
            f.int(GS + 0x47c, 1);
            f.int(GS + 0x4d8, 1);
            f.int(GS + 0x5d0, 2);
            f.int(GS + 0x5d8, 1);
            f.put(IDLER + 0x679, &[0]);
            f.int(COUNTRY + 8, 1);
            f.int(RESEARCH + 0x138, 1);
            f.array(GS + 0x308, 0x1e0000, &[0]);
            f.word(GS + 0x308, 0x1e0000);
            f.int(GS + 0x310, 2);
            f.int(GS + 0x314, 2);
            f.int(0x1e0000, -1);
            f.int(0x1e0004, 0);
            f.array(GS + 0x2d8, 0x1e1000, &[COUNTRY]);
            f.array(TAGS, 0x1e2000, &[0, 0]);
            f.string(0x1e2000, "---");
            f.string(0x1e2018, "GER");
            f.string(COUNTRY + 0x10, "German Reich");
            let playthrough = "aabbccdd-1122-4344-8566-778899aabbcc";
            f.word(GS + 0x6c8, playthrough.len() as u64 | 1);
            f.word(GS + 0x6d0, playthrough.len() as u64);
            f.word(GS + 0x6d8, 0x1e3000);
            f.put(0x1e3000, playthrough.as_bytes());
            f.array(RESEARCH + 0xa0, 0, &[]);
            f.array(PRODUCTION + 0x58, ARRAY, &[LINE, OTHER]);
            f.array(EQUIPMENT_DB + 0x50, 0x1e4000, &[TYPE]);
            f.int(TYPE + 8, 7);
            f.string(TYPE + 0x10, KEY);
            for (line, variant, handle, count) in [
                (LINE, VARIANT, HANDLE, 10),
                (OTHER, VARIANT + 0x1000, HANDLE + (1 << 32), 5),
            ] {
                f.word(line, IMAGE + 0x338a030);
                f.word(line + 0x20, PRODUCTION);
                f.word(line + 8, handle);
                f.int(line + 0x14, count);
                f.int(line + 0x18, 0);
                f.int(line + 0xe8, count);
                f.word(line + 0x88, variant);
                f.word(variant, IMAGE + 0x337aac8);
                f.int(variant + 0x14, 7);
                f.word(variant + 0x3e0, TYPE);
            }
            for (ordinal, (table, name)) in [
                (0x337bc38, "CCountry"),
                (0x33865d0, "CNationalFocusProgress"),
                (0x339e4e8, "CTechnologyStatus"),
            ]
            .into_iter()
            .enumerate()
            {
                f.rtti(table, name, ordinal as u64);
            }
            f
        }
        fn observe(&self) -> Result<Observation> {
            observe(self, &format!("0x{HANDLE:016x}"), KEY, None)
        }
    }
    impl ReadMemory for Mac {
        fn read(&self, address: u64, size: usize) -> Result<Vec<u8>> {
            self.reads.borrow_mut().push((address, size));
            let mut bytes = (0..size)
                .map(|i| {
                    self.bytes
                        .get(&(address + i as u64))
                        .copied()
                        .ok_or_else(|| {
                            Error::new("unmapped", "Generated Mac canonical range absent")
                        })
                })
                .collect::<Result<Vec<_>>>()?;
            if self.final_read.get() {
                if let Some(fault) = self.fault {
                    if (address..address + size as u64).contains(&fault) {
                        bytes[(fault - address) as usize] ^= 1;
                    }
                }
            }
            Ok(bytes)
        }
        fn read_many(&self, requests: &[(u64, usize)]) -> Result<Vec<Vec<u8>>> {
            self.final_read.set(true);
            let result = requests
                .iter()
                .map(|&(address, size)| self.read(address, size))
                .collect();
            self.final_read.set(false);
            result
        }
        fn string_abi(&self) -> StringAbi {
            StringAbi::LibCxx
        }
    }
    impl Process for Mac {
        fn check(&self) -> Result<()> {
            Ok(())
        }
        fn image_base(&self) -> u64 {
            IMAGE
        }
    }
    #[test]
    fn mac_canonical_proves_full_line_and_allocation_scope_without_linux_or_gui_reads() {
        let f = Mac::new();
        let observed = f.observe().unwrap();
        assert_eq!(
            observed.line,
            crate::production_step::LineIdentity {
                country: COUNTRY,
                production: PRODUCTION,
                line: LINE,
                variant: VARIANT,
                base_type: TYPE,
                handle: HANDLE,
                equipment_id: KEY.into()
            }
        );
        assert_eq!(observed.requested, 10);
        assert_eq!(observed.campaign.player, "GER");
        let all = observed.allocations.unwrap();
        assert_eq!(all.len(), 2);
        assert_eq!(all[1].0.line, OTHER);
        assert_eq!(all[1].1, 5);
        assert!(!f
            .reads
            .borrow()
            .iter()
            .any(|(address, _)| *address == IMAGE + 0x44086a8 || *address == IMAGE + 0x45778e0));
        assert!(observed.scope.is_empty() && observed.geometry.is_empty());
    }
    #[test]
    fn mac_ui_controls_remain_unavailable_before_payload_reads() {
        let f = Mac::new();
        assert_eq!(
            observe(&f, &format!("0x{HANDLE:016x}"), KEY, Some(false))
                .err()
                .unwrap()
                .code,
            "capability_unavailable"
        );
        assert!(f.reads.borrow().is_empty());
    }
    #[test]
    fn foreign_line_owner_refuses_before_handle_or_variant_payload() {
        for line in [LINE, OTHER] {
            let mut f = Mac::new();
            f.word(line + 0x20, PRODUCTION + 8);
            assert_eq!(f.observe().err().unwrap().code, "invalid_production_owner");
            assert!(!f
                .reads
                .borrow()
                .iter()
                .any(|(address, _)| *address == line + 8 || *address == line + 0x88));
        }
    }
    #[test]
    fn every_mac_identity_count_and_membership_is_a_final_witness() {
        for fault in [
            COUNTRY + 0xd10,
            PRODUCTION + 0x290,
            PRODUCTION + 0x58,
            ARRAY,
            ARRAY + 8,
            LINE + 0x20,
            LINE + 8,
            LINE + 0x88,
            LINE + 0xe8,
            OTHER + 0xe8,
            VARIANT + 0x3e0,
            TYPE + 0x10,
            EQUIPMENT_DB + 0x50,
        ] {
            let mut f = Mac::new();
            f.fault = Some(fault);
            assert_eq!(
                f.observe().err().unwrap().code,
                "torn_read",
                "fault={fault:x}"
            );
        }
    }
    #[test]
    fn full_handle_survives_order_change_without_rebinding_and_scope_retains_the_order() {
        let mut f = Mac::new();
        f.array(PRODUCTION + 0x58, ARRAY, &[OTHER, LINE]);
        let observed = f.observe().unwrap();
        assert_eq!(observed.line.line, LINE);
        let scope = observed.allocations.unwrap();
        assert_eq!(scope[0].0.line, OTHER);
        assert_eq!(scope[1].0.line, LINE);
    }
    #[test]
    fn unproved_other_equipment_never_becomes_an_unchanged_allocation_scope() {
        let mut f = Mac::new();
        f.word(VARIANT + 0x1000, IMAGE + 1);
        let observed = f.observe().unwrap();
        assert!(observed.allocations.is_none());
        assert_eq!(observed.line.line, LINE);
    }
}
