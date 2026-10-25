//! Shared read helpers of the doctrine, MIO, intelligence and research-extras
//! tools: own-country join, RTTI type checks, fixed-point conversions and the
//! freshness block every pure read returns.
//!
//! Everything here is read-only and macOS-build specific (libc++ strings,
//! Mach-O image). No offset is copied from another platform; every use site
//! states how it was validated.
use crate::{
    economy,
    error::{Error, Result},
    game,
    memory::{Reader, StringAbi},
    platform::Process,
};
use serde_json::{json, Value};
use std::time::{Instant, SystemTime, UNIX_EPOCH};

/// Experience values (army, navy, air) are stored as raw / 32768; checked
/// 26 October 2026 against the top bar (166.19 / 76.50 / 50.00 raw vs 166 / 76 / 50 shown).
pub(super) const XP_SCALE: f64 = 32768.0;
/// Script-level fixed point (modifier values, bonuses, reductions) is raw / 100000;
/// checked against `cost_reduction = 0.5`, `bonus = 1` in the game's focus files.
pub(super) const FIXED_SCALE: f64 = 100_000.0;

pub(super) fn round(value: f64, places: i32) -> f64 {
    let scale = 10f64.powi(places);
    (value * scale).round() / scale
}

/// Plausible user-space pointer; anything else is a stale or torn object.
pub(super) fn is_pointer(value: u64) -> bool {
    (0x10000..=0x7fff_ffff_0000).contains(&value)
}

pub(super) fn pointer(r: &Reader<'_>, address: u64) -> Result<u64> {
    let value = r.u64(address)?;
    if !is_pointer(value) {
        return Err(Error::new(
            "invalid_pointer",
            "Required game object is unavailable",
        ));
    }
    Ok(value)
}

/// Mangled class name of a polymorphic object (`vtable[-1]` is the type
/// information, whose second word points at the name; the high bit of that
/// word is a linker flag).
pub(super) fn type_name(r: &Reader<'_>, object: u64) -> Result<String> {
    let vtable = r.u64(object)?;
    let info = r.u64(
        vtable
            .checked_sub(8)
            .ok_or_else(|| Error::new("invalid_type", "Invalid vtable"))?,
    )?;
    if !is_pointer(info) {
        return Err(Error::new("invalid_type", "Invalid type information"));
    }
    let name = r.u64(info + 8)? & 0x7fff_ffff_ffff_ffff;
    if !is_pointer(name) {
        return Err(Error::new("invalid_type", "Invalid type name"));
    }
    let bytes = r.read(name, 96)?;
    let end = bytes.iter().position(|b| *b == 0).unwrap_or(bytes.len());
    Ok(String::from_utf8_lossy(&bytes[..end]).to_string())
}

/// `object` must be an instance of the class with this exact mangled name
/// (for example `N10NDoctrines15CDoctrineSystemE` or `19CIntelligenceAgency`).
pub(super) fn expect_type(r: &Reader<'_>, object: u64, mangled: &str) -> Result<()> {
    let found = type_name(r, object)?;
    if found == mangled {
        Ok(())
    } else {
        Err(Error::new(
            "invalid_type",
            format!("Expected {mangled}, found {found}"),
        ))
    }
}

/// libc++ string at `address`: a torn read is transient; anything else that
/// fails the bounded checks is not a string.
pub(super) fn text(r: &Reader<'_>, address: u64, limit: usize) -> Result<Option<String>> {
    if r.string_abi() != StringAbi::LibCxx {
        return Err(Error::new(
            "capability_unavailable",
            "This read is only reviewed for the macOS build",
        ));
    }
    match r.string_bounded(address, limit) {
        Ok(s) => Ok(Some(s)),
        Err(e) if e.code == "torn_read" => Err(e),
        Err(_) => Ok(None),
    }
}

/// Own country object, its dense index and its tag id, joined and validated by
/// the same checks as the production read.
pub(super) struct Own {
    pub country: u64,
    pub index: i32,
    pub gs: u64,
}

pub(super) fn own(r: &Reader<'_>, image: u64) -> Result<Own> {
    let (country, _) = economy::own_production(r, image)?;
    let g = game::layout(r);
    let gs = r.u64(image + g.gs_global)?;
    let tag = r.i32(country + 8)?;
    let index_bytes = r.array_entry(gs + g.tag_country_index, tag as usize, 4)?;
    let index = i32::from_le_bytes(index_bytes.try_into().unwrap());
    if index < 0 {
        return Err(Error::new("player_unavailable", "Own country index absent"));
    }
    Ok(Own { country, index, gs })
}

/// Game clock block shared by every read result.
pub(super) fn clock(r: &Reader<'_>, image: u64) -> Result<Value> {
    let c = game::context_from_reader(r, image)?;
    Ok(json!({"player":c.player,"date_raw":c.date_raw,"paused":c.paused,"speed":c.speed}))
}

pub(super) fn freshness(started: Instant, calls: u64, bytes: usize) -> Value {
    json!({
        "observed_unix_ms":SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_millis(),
        "elapsed_ms":round(started.elapsed().as_secs_f64() * 1000.0, 1),
        "read_calls":calls,
        "read_bytes":bytes,
    })
}

pub(super) fn game_dir(process: &dyn Process) -> Result<std::path::PathBuf> {
    let exe = &process
        .identity()
        .ok_or_else(|| Error::new("map_data_unavailable", "Game identity unavailable"))?
        .exe;
    super::mapdata::game_dir(exe)
}

/// Optional boolean argument (absent means false; any other type is refused).
pub(super) fn flag(args: &serde_json::Map<String, Value>, name: &str) -> Result<bool> {
    match args.get(name) {
        None => Ok(false),
        Some(Value::Bool(b)) => Ok(*b),
        Some(_) => Err(Error::new(
            "invalid_arguments",
            format!("{name} must be a boolean"),
        )),
    }
}

pub(super) fn i32_at(bytes: &[u8], offset: usize) -> i32 {
    i32::from_le_bytes(bytes[offset..offset + 4].try_into().unwrap())
}
pub(super) fn i64_at(bytes: &[u8], offset: usize) -> i64 {
    i64::from_le_bytes(bytes[offset..offset + 8].try_into().unwrap())
}
pub(super) fn u64_at(bytes: &[u8], offset: usize) -> u64 {
    u64::from_le_bytes(bytes[offset..offset + 8].try_into().unwrap())
}

/// Everything a pure read returns about how it was observed.
pub(super) fn stamp(started: Instant, r: &Reader<'_>) -> Value {
    freshness(started, r.calls.get(), r.bytes.get())
}

#[cfg(test)]
pub(super) mod fake {
    //! A sparse fake process memory for read-layout tests.
    use crate::{
        error::{Error, Result},
        memory::{ReadMemory, StringAbi},
    };
    use std::cell::RefCell;
    use std::collections::BTreeMap;

    #[derive(Default)]
    pub(crate) struct Mem {
        pub pages: RefCell<BTreeMap<u64, Vec<u8>>>,
        next_string: std::cell::Cell<u64>,
    }
    impl Mem {
        pub(crate) fn put(&self, address: u64, bytes: &[u8]) {
            self.pages.borrow_mut().insert(address, bytes.to_vec());
        }
        /// Write over a region that starts inside an earlier `put`.
        pub(crate) fn patch(&self, address: u64, bytes: &[u8]) {
            let mut pages = self.pages.borrow_mut();
            for (base, page) in pages.iter_mut() {
                if address >= *base && address + bytes.len() as u64 <= *base + page.len() as u64 {
                    let at = (address - base) as usize;
                    page[at..at + bytes.len()].copy_from_slice(bytes);
                    return;
                }
            }
            panic!("patch outside any page {address:#x}");
        }
        pub(crate) fn alloc(&self, address: u64, size: usize) {
            self.pages.borrow_mut().insert(address, vec![0; size]);
        }
        pub(crate) fn q(&self, address: u64, value: u64) {
            self.patch(address, &value.to_le_bytes());
        }
        pub(crate) fn i(&self, address: u64, value: i32) {
            self.patch(address, &value.to_le_bytes());
        }
        pub(crate) fn l(&self, address: u64, value: i64) {
            self.patch(address, &value.to_le_bytes());
        }
        /// libc++ string at `address`: inline when it fits 22 bytes, else the
        /// long form with its characters in a fresh allocation.
        pub(crate) fn sso(&self, address: u64, text: &str) {
            if text.len() <= 22 {
                let mut bytes = vec![0u8; 24];
                bytes[0] = (text.len() as u8) << 1;
                bytes[1..1 + text.len()].copy_from_slice(text.as_bytes());
                self.patch(address, &bytes);
            } else {
                let at = match self.next_string.get() {
                    0 => 0x9000_0000,
                    n => n,
                };
                self.next_string.set(at + 0x100);
                let mut data = text.as_bytes().to_vec();
                data.push(0);
                data.resize(0x100, 0);
                self.put(at, &data);
                let mut header = Vec::new();
                header.extend_from_slice(&((text.len() as u64 + 32) | 1).to_le_bytes());
                header.extend_from_slice(&(text.len() as u64).to_le_bytes());
                header.extend_from_slice(&at.to_le_bytes());
                self.patch(address, &header);
            }
        }
    }
    impl ReadMemory for Mem {
        fn read(&self, address: u64, size: usize) -> Result<Vec<u8>> {
            let pages = self.pages.borrow();
            for (base, page) in pages.range(..=address).rev().take(1) {
                let end = address + size as u64;
                if end <= *base + page.len() as u64 {
                    let at = (address - base) as usize;
                    return Ok(page[at..at + size].to_vec());
                }
            }
            Err(Error::new("short_read", format!("unmapped {address:#x}")))
        }
        fn string_abi(&self) -> StringAbi {
            StringAbi::LibCxx
        }
    }
}

#[cfg(test)]
mod tests {
    use super::fake::Mem;
    use super::*;

    #[test]
    fn rtti_names_resolve_and_mismatches_fail() {
        let m = Mem::default();
        m.alloc(0x20000, 0x1000);
        // object at 0x20000, vtable pointer 0x20040, typeinfo 0x20080, name 0x200c0
        m.q(0x20000, 0x20040);
        m.q(0x20040 - 8, 0x20080);
        m.q(0x20080 + 8, 0x200c0 | (1 << 63));
        m.patch(0x200c0, b"19CIntelligenceAgency\0");
        let r = Reader::new(&m);
        assert_eq!(type_name(&r, 0x20000).unwrap(), "19CIntelligenceAgency");
        assert!(expect_type(&r, 0x20000, "19CIntelligenceAgency").is_ok());
        assert_eq!(
            expect_type(&r, 0x20000, "8CCountry").unwrap_err().code,
            "invalid_type"
        );
    }

    #[test]
    fn rounding_and_scales() {
        assert_eq!(round(166.1884765625, 2), 166.19);
        assert_eq!(round(5_445_880.0 / XP_SCALE, 2), 166.2);
        assert_eq!(-50_000.0 / FIXED_SCALE, -0.5);
    }

    #[test]
    fn text_reads_libcxx_and_reports_non_strings_as_none() {
        let m = Mem::default();
        m.alloc(0x30000, 0x100);
        m.sso(0x30000, "GER_the_four_year_plan");
        // A long-form header whose size exceeds its capacity is not a string.
        m.q(0x30040, 17);
        m.q(0x30048, 1000);
        m.q(0x30050, 0x30080);
        let r = Reader::new(&m);
        assert_eq!(
            text(&r, 0x30000, 64).unwrap().as_deref(),
            Some("GER_the_four_year_plan")
        );
        assert_eq!(text(&r, 0x30040, 64).unwrap(), None);
    }
}
