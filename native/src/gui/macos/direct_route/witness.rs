//! Consumed-field witnesses for one bounded typed route observation.
use crate::{
    error::{Error, Result},
    memory::{ReadMemory, StringAbi},
    platform,
};
use std::{cell::RefCell, collections::BTreeMap, time::Instant};
/// Batch transport can include unused padding/counters. Witness EVERY parsed
/// field, in its original address, rather than inventing a freeze of padding.
/// Membership payload and all other reads retain full byte-for-byte witnesses.
fn projection(v: &[u8]) -> Vec<u8> {
    if v.len() != 0x15c {
        return v.to_vec();
    }
    let mut out = vec![0; v.len()];
    let mut ranges = vec![(0, 8), (0x108, 0x110), (0x128, 0x129), (0x144, 0x145)];
    if v[0x128] != 0 {
        ranges.push((0xc0, 0xd0));
    }
    if v[0x128] != 0 || v[0x144] != 0 {
        ranges.extend([
            (0x31, 0x32),
            (0x70, 0x78),
            (0x110, 0x118),
            (0x134, 0x135),
            (0x138, 0x140),
            (0x145, 0x146),
            (0x148, 0x15c),
        ]);
    }
    if v[0x144] != 0 {
        ranges.extend([(0x80, 0x84), (0x94, 0x98), (0xb0, 0xbc), (0x120, 0x128)]);
    }
    for (lo, hi) in ranges {
        out[lo..hi].copy_from_slice(&v[lo..hi]);
    }
    out
}
pub(crate) struct Witness<'a> {
    p: &'a dyn platform::Process,
    until: Instant,
    reads: RefCell<BTreeMap<(u64, usize), Vec<u8>>>,
}
impl<'a> Witness<'a> {
    pub(crate) fn new(p: &'a dyn platform::Process, until: Instant) -> Self {
        Self {
            p,
            until,
            reads: RefCell::new(BTreeMap::new()),
        }
    }
    fn boundary(&self) -> Result<()> {
        if Instant::now() >= self.until {
            return Err(Error::new(
                "deadline",
                "Original inventory deadline expired",
            ));
        }
        Ok(())
    }
    pub(crate) fn finish(&self) -> Result<()> {
        for (&(a, n), expected) in self.reads.borrow().iter() {
            self.boundary()?;
            if projection(&self.p.read(a, n)?) != *expected {
                return Err(Error::new(
                    "torn_read",
                    format!("Original GUI consumed metadata changed at {a:#x}/{n}"),
                ));
            }
        }
        self.p.check()?;
        self.boundary()
    }
}
impl ReadMemory for Witness<'_> {
    fn string_abi(&self) -> StringAbi {
        self.p.string_abi()
    }
    fn read(&self, a: u64, n: usize) -> Result<Vec<u8>> {
        self.boundary()?;
        let v = self.p.read(a, n)?;
        let watched = projection(&v);
        self.boundary()?;
        let mut rows = self.reads.borrow_mut();
        if let Some(old) = rows.get(&(a, n)) {
            if *old != watched {
                return Err(Error::new(
                    "torn_read",
                    "Repeated original GUI field changed",
                ));
            }
        } else {
            if rows.len() >= 200000 {
                return Err(Error::new("read_bound", "GUI read-request bound"));
            }
            rows.insert((a, n), watched);
        }
        Ok(v)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{cell::Cell, time::Duration};

    #[test]
    fn consumed_fields_are_witnessed_while_unconsumed_padding_is_not() {
        let mut original = vec![0; 0x15c];
        original[0x128] = 1;
        original[0x144] = 1;
        let watched = projection(&original);
        for at in (0..8)
            .chain(0xc0..0xd0)
            .chain(0x108..0x110)
            .chain(0x128..0x129)
            .chain(0x144..0x146)
            .chain(0x31..0x32)
            .chain(0x70..0x78)
            .chain(0x110..0x118)
            .chain(0x134..0x135)
            .chain(0x138..0x140)
            .chain(0x148..0x15c)
        {
            let mut changed = original.clone();
            changed[at] ^= 1;
            assert_ne!(projection(&changed), watched, "consumed field {at:#x}");
        }
        let mut padding = original.clone();
        padding[0x40] ^= 1;
        assert_eq!(projection(&padding), watched);
    }
    #[test]
    fn inactive_descendants_skip_their_header_but_keep_the_gate() {
        let original = vec![0; 0x15c];
        let mut changed = original.clone();
        changed[0xc0] = 5;
        assert_eq!(projection(&changed), projection(&original));
        changed[0x128] = 1;
        assert_ne!(projection(&changed), projection(&original));
    }
    #[test]
    fn self_cache_and_canvas_fields_remain_explicit_witnesses() {
        let mut original = vec![0; 0x15c];
        original[0x144] = 1;
        for at in (0x80..0x84)
            .chain(0x94..0x98)
            .chain(0xb0..0xbc)
            .chain(0x120..0x128)
        {
            let mut changed = original.clone();
            changed[at] ^= 1;
            assert_ne!(projection(&changed), projection(&original));
        }
    }
    struct Fake {
        bytes: RefCell<Vec<u8>>,
        reads: Cell<usize>,
        valid: Cell<bool>,
        short: Cell<bool>,
    }
    impl Fake {
        fn new() -> Self {
            let mut v = vec![0; 0x15c];
            v[0x128] = 1;
            v[0x144] = 1;
            Self {
                bytes: RefCell::new(v),
                reads: Cell::new(0),
                valid: Cell::new(true),
                short: Cell::new(false),
            }
        }
    }
    impl ReadMemory for Fake {
        fn read(&self, _a: u64, n: usize) -> Result<Vec<u8>> {
            self.reads.set(self.reads.get() + 1);
            let n = if self.short.get() { n - 1 } else { n };
            Ok(self.bytes.borrow()[..n].to_vec())
        }
        fn string_abi(&self) -> StringAbi {
            StringAbi::LibCxx
        }
    }
    impl platform::Process for Fake {
        fn check(&self) -> Result<()> {
            if self.valid.get() {
                Ok(())
            } else {
                Err(Error::new("process_lost", "fixture original identity lost"))
            }
        }
        fn image_base(&self) -> u64 {
            0x100000000
        }
    }
    #[test]
    fn repeated_or_final_consumed_changes_refuse_but_padding_does_not() {
        let p = Fake::new();
        let w = Witness::new(&p, Instant::now() + Duration::from_secs(60));
        w.read(0x10000, 0x15c).unwrap();
        p.bytes.borrow_mut()[0x40] ^= 1;
        w.finish().unwrap();
        p.bytes.borrow_mut()[0x70] ^= 1;
        assert!(w.read(0x10000, 0x15c).is_err());
        assert!(w.finish().is_err());
    }
    #[test]
    fn expired_start_has_zero_reads_and_final_identity_loss_refuses() {
        let p = Fake::new();
        let expired = Witness::new(&p, Instant::now() - Duration::from_secs(1));
        assert!(expired.read(0x10000, 8).is_err());
        assert_eq!(p.reads.get(), 0);
        let w = Witness::new(&p, Instant::now() + Duration::from_secs(60));
        w.read(0x10000, 8).unwrap();
        p.valid.set(false);
        assert!(w.finish().is_err());
    }
    #[test]
    fn short_final_witness_never_produces_a_complete_observation() {
        let p = Fake::new();
        let w = Witness::new(&p, Instant::now() + Duration::from_secs(60));
        w.read(0x10000, 0x15c).unwrap();
        p.short.set(true);
        assert!(w.finish().is_err());
    }
}
