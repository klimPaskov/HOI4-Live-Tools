use hoi4_live_native::{
    error::{Error, Result},
    memory::{ReadMemory, StringAbi},
    platform::Process,
    research_folders,
};
use std::cell::Cell;

// No Mac folder offsets have been recovered. Even snapshot/process validation
// must remain unreachable, rather than discovering the ABI after a payload read.
#[derive(Default)]
struct UnavailableMac {
    accesses: Cell<usize>,
}
impl UnavailableMac {
    fn accessed(&self) -> Error {
        self.accesses.set(self.accesses.get() + 1);
        Error::new("fixture_access", "Unreviewed Mac process access")
    }
}
impl ReadMemory for UnavailableMac {
    fn read(&self, _: u64, _: usize) -> Result<Vec<u8>> {
        Err(self.accessed())
    }
    fn read_many(&self, _: &[(u64, usize)]) -> Result<Vec<Vec<u8>>> {
        Err(self.accessed())
    }
    fn string_abi(&self) -> StringAbi {
        StringAbi::LibCxx
    }
}
impl Process for UnavailableMac {
    fn check(&self) -> Result<()> {
        Err(self.accessed())
    }
    fn begin_snapshot(&self) -> Result<()> {
        Err(self.accessed())
    }
    fn image_base(&self) -> u64 {
        let _ = self.accessed();
        0x100000000
    }
}

#[test]
fn owned_mac_catalog_refuses_before_any_process_access() {
    let process = UnavailableMac::default();
    let error = research_folders::owned_snapshot(&process).unwrap_err();
    assert_eq!(error.code, "unsupported_platform");
    assert_eq!(process.accesses.get(), 0);
}

#[test]
fn public_mac_catalog_refuses_before_any_process_access() {
    let process = UnavailableMac::default();
    let error = research_folders::snapshot(&process).unwrap_err();
    assert_eq!(error.code, "unsupported_platform");
    assert_eq!(process.accesses.get(), 0);
}
