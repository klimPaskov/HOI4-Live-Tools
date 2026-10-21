//! Request-local transcript of reads the guarded geometry producer already makes.
//! Never traverses pointers, adds remote reads, serializes or persists identities.
use crate::{
    error::{Error, Result},
    memory::{ReadMemory, Reader, StringAbi},
};
use std::{cell::RefCell, collections::BTreeMap};
const MAX_READS: usize = 4096;
const MAX_BYTES: usize = 1024 * 1024;
#[derive(Default)]
struct State {
    reads: BTreeMap<(u64, usize), Vec<u8>>,
    bytes: usize,
}
pub(crate) struct Transcript<'a, 'm> {
    inner: &'a Reader<'m>,
    state: RefCell<State>,
}
impl<'a, 'm> Transcript<'a, 'm> {
    pub(crate) fn new(inner: &'a Reader<'m>) -> Self {
        Self {
            inner,
            state: RefCell::new(State::default()),
        }
    }
    /// Include bytes already obtained by the control's authorized exact-type gate.
    pub(crate) fn seed(&self, address: u64, bytes: Vec<u8>) -> Result<()> {
        self.record(address, bytes)
    }
    fn record(&self, address: u64, bytes: Vec<u8>) -> Result<()> {
        let end = address.checked_add(bytes.len() as u64).ok_or_else(|| {
            Error::new(
                "invalid_gui_geometry",
                "Geometry transcript address overflow",
            )
        })?;
        let mut state = self.state.borrow_mut();
        // Compare overlapping reads too: a smaller repeat must not hide a torn
        // resource, pointer, scalar or hierarchy field in a previous larger read.
        for (&(prior, n), old) in &state.reads {
            let prior_end = prior.checked_add(n as u64).ok_or_else(|| {
                Error::new(
                    "invalid_gui_geometry",
                    "Geometry transcript address overflow",
                )
            })?;
            let begin = address.max(prior);
            let finish = end.min(prior_end);
            if begin < finish
                && bytes[(begin - address) as usize..(finish - address) as usize]
                    != old[(begin - prior) as usize..(finish - prior) as usize]
            {
                return Err(Error::new(
                    "torn_read",
                    "Production geometry read bytes changed",
                ));
            }
        }
        let key = (address, bytes.len());
        if state.reads.contains_key(&key) {
            return Ok(());
        }
        if state.reads.len() >= MAX_READS || bytes.len() > MAX_BYTES - state.bytes {
            return Err(Error::new(
                "invalid_gui_geometry",
                "Production geometry transcript exceeds bound",
            ));
        }
        state.bytes += bytes.len();
        state.reads.insert(key, bytes);
        Ok(())
    }
    pub(crate) fn finish(&self) -> Vec<(u64, Vec<u8>)> {
        self.state
            .borrow()
            .reads
            .iter()
            .map(|(&(address, _), bytes)| (address, bytes.clone()))
            .collect()
    }
}
impl ReadMemory for Transcript<'_, '_> {
    fn string_abi(&self) -> StringAbi {
        self.inner.string_abi()
    }
    fn read(&self, address: u64, size: usize) -> Result<Vec<u8>> {
        // Reject oversize requests without allocating or asking the process.
        if size > MAX_BYTES {
            return Err(Error::new(
                "invalid_gui_geometry",
                "Geometry read exceeds transcript bound",
            ));
        }
        let bytes = self.inner.read(address, size)?;
        if bytes.len() != size {
            return Err(Error::new(
                "short_read",
                "Production geometry read incomplete",
            ));
        }
        self.record(address, bytes.clone())?;
        Ok(bytes)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::{Cell, RefCell};
    struct Fake {
        bytes: RefCell<Vec<u8>>,
        calls: Cell<usize>,
    }
    impl ReadMemory for Fake {
        fn string_abi(&self) -> StringAbi {
            StringAbi::LibCxx
        }
        fn read(&self, address: u64, size: usize) -> Result<Vec<u8>> {
            self.calls.set(self.calls.get() + 1);
            Ok(self.bytes.borrow()
                [(address - 0x10000) as usize..(address - 0x10000) as usize + size]
                .to_vec())
        }
    }
    #[test]
    fn forwarding_preserves_abi_and_adds_no_reads() {
        let f = Fake {
            bytes: RefCell::new(vec![1, 2, 3, 4]),
            calls: Cell::new(0),
        };
        let r = Reader::new(&f);
        let t = Transcript::new(&r);
        t.seed(20, vec![8]).unwrap();
        assert_eq!(t.string_abi(), StringAbi::LibCxx);
        assert_eq!(f.calls.get(), 0);
        assert_eq!(t.read(0x10000, 4).unwrap(), vec![1, 2, 3, 4]);
        assert_eq!(f.calls.get(), 1);
        assert_eq!(t.finish(), vec![(20, vec![8]), (0x10000, vec![1, 2, 3, 4])]);
        assert_eq!(f.calls.get(), 1);
    }
    #[test]
    fn changed_repeat_and_overlapping_bytes_refuse() {
        for (address, size) in [(0x10000, 4), (0x10001, 2)] {
            let f = Fake {
                bytes: RefCell::new(vec![1, 2, 3, 4]),
                calls: Cell::new(0),
            };
            let r = Reader::new(&f);
            let t = Transcript::new(&r);
            t.read(0x10000, 4).unwrap();
            f.bytes.borrow_mut()[1] = 9;
            assert_eq!(t.read(address, size).unwrap_err().code, "torn_read");
            assert_eq!(f.calls.get(), 2);
        }
    }
    #[test]
    fn replaced_pointer_with_identical_geometry_changes_transcript() {
        let f = Fake {
            bytes: RefCell::new(vec![1; 16]),
            calls: Cell::new(0),
        };
        let r = Reader::new(&f);
        let first = Transcript::new(&r);
        first.seed(100, 8u64.to_le_bytes().to_vec()).unwrap();
        first.read(0x10000, 8).unwrap();
        let second = Transcript::new(&r);
        second.seed(100, 16u64.to_le_bytes().to_vec()).unwrap();
        second.read(0x10000, 8).unwrap();
        assert_ne!(first.finish(), second.finish());
        assert_eq!(f.calls.get(), 2);
    }
    #[test]
    fn count_and_byte_bounds_refuse() {
        let f = Fake {
            bytes: RefCell::new(vec![]),
            calls: Cell::new(0),
        };
        let r = Reader::new(&f);
        let t = Transcript::new(&r);
        for n in 0..MAX_READS {
            t.seed(n as u64, vec![1]).unwrap();
        }
        assert_eq!(
            t.seed(MAX_READS as u64, vec![1]).unwrap_err().code,
            "invalid_gui_geometry"
        );
        let t = Transcript::new(&r);
        t.seed(0, vec![1; MAX_BYTES]).unwrap();
        assert!(t.seed(MAX_BYTES as u64, vec![1]).is_err());
        assert!(t.read(0, MAX_BYTES + 1).is_err());
        assert_eq!(f.calls.get(), 0);
    }
}
