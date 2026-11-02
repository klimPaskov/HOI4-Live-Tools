use super::*;
use sha2::{Digest, Sha256};
use std::sync::{Arc, Mutex};
const BASE: u64 = 0x140000000;
const PAYLOAD: u64 = 0x200000;
fn sid(last: u8) -> Sid {
    Sid::new(vec![1, 2, 0, 0, 0, 0, 0, 5, 21, 0, 0, 0, last, 0, 0, 0]).unwrap()
}
fn identity(pid: u32, path: &str) -> Identity {
    Identity {
        pid,
        creation_filetime: 133456789012345678 + u64::from(pid),
        executable: path.into(),
        owner: sid(7),
        file: FileIdentity {
            volume: 9,
            id: [pid as u8; 16],
        },
    }
}
fn image_bytes() -> Vec<u8> {
    let mut b = vec![0u8; 1536];
    b[..2].copy_from_slice(b"MZ");
    b[0x3c..0x40].copy_from_slice(&0x80u32.to_le_bytes());
    b[0x80..0x84].copy_from_slice(b"PE\0\0");
    b[0x84..0x86].copy_from_slice(&0x8664u16.to_le_bytes());
    b[0x86..0x88].copy_from_slice(&1u16.to_le_bytes());
    b[0x88..0x8c].copy_from_slice(&123456u32.to_le_bytes());
    b[0x94..0x96].copy_from_slice(&240u16.to_le_bytes());
    b[0x96..0x98].copy_from_slice(&0x22u16.to_le_bytes());
    let o = 0x98;
    b[o..o + 2].copy_from_slice(&0x20bu16.to_le_bytes());
    b[o + 24..o + 32].copy_from_slice(&BASE.to_le_bytes());
    b[o + 32..o + 36].copy_from_slice(&4096u32.to_le_bytes());
    b[o + 36..o + 40].copy_from_slice(&512u32.to_le_bytes());
    b[o + 56..o + 60].copy_from_slice(&65536u32.to_le_bytes());
    b[o + 60..o + 64].copy_from_slice(&512u32.to_le_bytes());
    let section = o + 240;
    b[section..section + 5].copy_from_slice(b".text");
    b[section + 8..section + 12].copy_from_slice(&4096u32.to_le_bytes());
    b[section + 12..section + 16].copy_from_slice(&4096u32.to_le_bytes());
    b[section + 16..section + 20].copy_from_slice(&512u32.to_le_bytes());
    b[512..].fill(0xa5);
    b
}
#[derive(Clone)]
struct Fake(Arc<Mutex<State>>);
struct State {
    game: Metadata,
    supervisor: Metadata,
    image: Image,
    bytes: Vec<u8>,
    trace: Vec<String>,
    current: Sid,
    elevated: bool,
    region: Region,
    payload_reads: usize,
    fail_open: usize,
    opens: usize,
    short: bool,
    read_error: bool,
    after: Option<&'static str>,
    region_changed: bool,
    loaded_changed: bool,
    game_file: FileIdentity,
    supervisor_file: FileIdentity,
    race_open: bool,
}
struct Handle {
    pid: u32,
    image: bool,
    state: Arc<Mutex<State>>,
}
impl Drop for Handle {
    fn drop(&mut self) {
        self.state
            .lock()
            .unwrap()
            .trace
            .push(format!("close:{}:{}", self.pid, self.image));
    }
}
impl Fake {
    fn new() -> (Self, Session, Profile) {
        let game = identity(7, r"C:\Game\hoi4.exe");
        let supervisor = identity(6, r"C:\Tools\owner.exe");
        let bytes = image_bytes();
        let image = Image {
            file: game.file.clone(),
            size: bytes.len() as u64,
            header: bytes.clone(),
            sha256: Sha256::digest(&bytes).into(),
        };
        let profile = Profile {
            pe: PeIdentity::parse(&bytes).unwrap(),
            sha256: image.sha256,
        };
        let f = Self(Arc::new(Mutex::new(State {
            game: Metadata {
                identity: game.clone(),
                parent_pid: 6,
                image_base: BASE,
                image_size: 65536,
            },
            supervisor: Metadata {
                identity: supervisor.clone(),
                parent_pid: 5,
                image_base: BASE,
                image_size: 65536,
            },
            game_file: game.file.clone(),
            supervisor_file: supervisor.file.clone(),
            image,
            bytes,
            trace: vec![],
            current: sid(7),
            elevated: false,
            region: Region {
                start: PAYLOAD,
                allocation: PAYLOAD,
                size: 4096,
                state: 0x1000,
                protection: 4,
                kind: 0x20000,
            },
            payload_reads: 0,
            fail_open: 0,
            opens: 0,
            short: false,
            read_error: false,
            after: None,
            region_changed: false,
            loaded_changed: false,
            race_open: false,
        })));
        (f, Session { game, supervisor }, profile)
    }
    fn reader() -> (Self, OwnedReader<Self>) {
        let (f, s, p) = Self::new();
        let r = OwnedReader::generated(f.clone(), s, p).unwrap();
        f.reset();
        (f, r)
    }
    fn reset(&self) {
        let mut s = self.0.lock().unwrap();
        s.trace.clear();
        s.payload_reads = 0;
    }
    fn mutate(&self, f: impl FnOnce(&mut State)) {
        f(&mut self.0.lock().unwrap())
    }
    fn trace(&self) -> Vec<String> {
        self.0.lock().unwrap().trace.clone()
    }
    fn reads(&self) -> usize {
        self.0.lock().unwrap().payload_reads
    }
}
impl Api for Fake {
    type Process = Handle;
    type Image = Handle;
    fn current_owner(&self) -> Result<(Sid, bool)> {
        let mut s = self.0.lock().unwrap();
        s.trace.push("owner".into());
        Ok((s.current.clone(), s.elevated))
    }
    fn open_process(&self, pid: u32, rights: u32) -> Result<Handle> {
        let mut s = self.0.lock().unwrap();
        s.opens += 1;
        s.trace.push(format!("open:{pid}:{rights:x}"));
        if s.fail_open == s.opens {
            return Err(error("windows_api", "Generated open failure"));
        }
        if rights == QUERY_READ && s.race_open {
            s.game.identity.creation_filetime += 1;
        }
        Ok(Handle {
            pid,
            image: false,
            state: self.0.clone(),
        })
    }
    fn inspect(&self, h: &Handle) -> Result<Metadata> {
        let mut s = self.0.lock().unwrap();
        s.trace.push(format!("inspect:{}", h.pid));
        if s.after == Some("supervisor_exit") && s.payload_reads > 0 && h.pid == 6 {
            return Err(changed());
        }
        Ok(if h.pid == 7 {
            s.game.clone()
        } else {
            s.supervisor.clone()
        })
    }
    fn check_handle(&self, h: &Handle, expected: &Identity) -> Result<()> {
        let mut s = self.0.lock().unwrap();
        s.trace.push(format!("fast:{}", h.pid));
        let actual = if h.pid == 7 {
            &s.game.identity
        } else {
            &s.supervisor.identity
        };
        if (h.pid == 6 && s.after == Some("supervisor_exit") && s.payload_reads > 0)
            || actual.pid != expected.pid
            || actual.creation_filetime != expected.creation_filetime
        {
            return Err(changed());
        }
        Ok(())
    }
    fn open_image(&self, path: &str) -> Result<Handle> {
        let mut s = self.0.lock().unwrap();
        s.trace.push("image_open".into());
        let pid = if path == s.game.identity.executable {
            7
        } else {
            6
        };
        Ok(Handle {
            pid,
            image: true,
            state: self.0.clone(),
        })
    }
    fn image(&self, _: &Handle) -> Result<Image> {
        let mut s = self.0.lock().unwrap();
        s.trace.push("image_sha".into());
        Ok(s.image.clone())
    }
    fn file_identity(&self, h: &Handle) -> Result<FileIdentity> {
        let mut s = self.0.lock().unwrap();
        s.trace.push(format!("file:{}", h.pid));
        Ok(if h.pid == 7 {
            s.game_file.clone()
        } else {
            s.supervisor_file.clone()
        })
    }
    fn region(&self, _: &Handle, address: u64) -> Result<Region> {
        let mut s = self.0.lock().unwrap();
        s.trace.push("region".into());
        Ok(if address == BASE {
            Region {
                start: BASE,
                allocation: BASE,
                size: 65536,
                state: 0x1000,
                protection: 2,
                kind: 0x1000000,
            }
        } else if s.region_changed && s.payload_reads > 0 {
            Region {
                protection: 2,
                ..s.region
            }
        } else {
            s.region
        })
    }
    fn read(&self, _: &Handle, address: u64, size: usize) -> Result<Vec<u8>> {
        let mut s = self.0.lock().unwrap();
        s.trace.push(format!("read:{address:x}:{size}"));
        if address == BASE {
            let mut b = s.bytes[..size].to_vec();
            if s.loaded_changed {
                b[0] ^= 1;
            }
            return Ok(b);
        }
        s.payload_reads += 1;
        match s.after {
            Some("creation") => s.game.identity.creation_filetime += 1,
            Some("parent") => s.game.parent_pid = 99,
            Some("module") => s.game.image_base += 65536,
            Some("image") => s.game_file.id = [77; 16],
            Some("owner") => s.game.identity.owner = sid(8),
            _ => {}
        }
        if s.read_error {
            return Err(error("windows_api", "Generated inaccessible race"));
        }
        Ok(vec![0x42; if s.short { size - 1 } else { size }])
    }
}
#[test]
fn production_allowlist_is_empty_before_vm_rights_or_payload_and_closes_all_handles() {
    let (f, s, _) = Fake::new();
    assert_eq!(
        OwnedReader::open(f.clone(), s).err().unwrap().code,
        "unsupported_build"
    );
    let t = f.trace();
    assert!(!t.iter().any(|v| v == "open:7:100410"));
    assert!(!t.iter().any(|v| v.starts_with("read:")));
    assert_eq!(t.iter().filter(|v| v.starts_with("close:")).count(), 4);
}
#[test]
fn exact_generated_profile_loads_pinned_headers_then_reads_bounded_bytes() {
    let (f, r) = Fake::reader();
    assert_eq!(r.identity().creation_filetime, 133456789012345685);
    assert_eq!(r.image_base(), BASE);
    assert_eq!(r.read(PAYLOAD, 16).unwrap(), vec![0x42; 16]);
    assert_eq!(f.reads(), 1);
    drop(r);
    assert_eq!(
        f.trace().iter().filter(|v| v.starts_with("close:")).count(),
        4
    );
}
#[test]
fn invalid_session_never_opens_a_handle() {
    for mutation in 0..7 {
        let (f, mut s, p) = Fake::new();
        match mutation {
            0 => s.game.pid = 0,
            1 => s.game.creation_filetime = 0,
            2 => s.game.executable = r"Game\hoi4.exe".into(),
            3 => s.supervisor.pid = s.game.pid,
            4 => s.supervisor.creation_filetime = s.game.creation_filetime + 1,
            5 => s.game.executable = r"C:\Game\..\hoi4.exe".into(),
            _ => s.game.file.id = [0; 16],
        }
        assert!(OwnedReader::generated(f.clone(), s, p).is_err());
        assert!(f.trace().is_empty());
    }
}
#[test]
fn full_sid_not_final_rid_is_owner_authority() {
    let (f, s, p) = Fake::new();
    let mut wrong = sid(7).0;
    wrong[7] = 6;
    f.mutate(|v| v.current = Sid::new(wrong).unwrap());
    assert!(OwnedReader::generated(f.clone(), s, p).is_err());
    assert!(!f.trace().iter().any(|v| v.starts_with("open:")));
}
#[test]
fn malformed_sids_are_rejected_without_truncation() {
    for bytes in [
        vec![],
        vec![1; 7],
        vec![2, 0, 0, 0, 0, 0, 0, 5],
        vec![1, 16, 0, 0, 0, 0, 0, 5],
        vec![1; 16],
    ] {
        assert!(Sid::new(bytes).is_err());
    }
    assert_ne!(sid(7), sid(8));
}
#[test]
fn elevated_current_or_foreign_supervisor_is_refused_without_vm_reads() {
    for which in 0..2 {
        let (f, s, p) = Fake::new();
        f.mutate(|v| {
            if which == 0 {
                v.elevated = true
            } else {
                v.supervisor.identity.owner = sid(9)
            }
        });
        assert!(OwnedReader::generated(f.clone(), s, p).is_err());
        assert!(!f.trace().iter().any(|v| v.starts_with("read:")));
    }
}
#[test]
fn every_original_process_and_image_edge_is_validated_before_payload() {
    for which in 0..7 {
        let (f, r) = Fake::reader();
        f.mutate(|v| match which {
            0 => v.game.identity.creation_filetime += 1,
            1 => v.game.parent_pid = 99,
            2 => v.game.identity.executable = r"C:\Other\hoi4.exe".into(),
            3 => v.game.identity.owner = sid(9),
            4 => v.supervisor.identity.creation_filetime += 1,
            5 => v.game.image_base += 65536,
            _ => v.game_file.id = [99; 16],
        });
        assert_eq!(r.read(PAYLOAD, 8).unwrap_err().code, "ownership_changed");
        assert_eq!(f.reads(), 0);
    }
}
#[test]
fn ownership_change_during_read_discards_bytes_and_remains_sticky() {
    for which in [
        "creation",
        "parent",
        "module",
        "image",
        "owner",
        "supervisor_exit",
    ] {
        let (f, r) = Fake::reader();
        f.mutate(|v| v.after = Some(which));
        assert!(r.read(PAYLOAD, 8).is_err());
        assert_eq!(f.reads(), 1);
        f.reset();
        assert!(r.read(PAYLOAD, 8).is_err());
        assert!(f.trace().is_empty(), "sticky {which}");
    }
}
#[test]
fn post_read_identity_check_also_runs_when_transfer_fails() {
    let (f, r) = Fake::reader();
    f.mutate(|v| v.read_error = true);
    assert_eq!(r.read(PAYLOAD, 8).unwrap_err().code, "windows_api");
    let t = f.trace();
    let a = t.iter().position(|v| v.starts_with("read:")).unwrap();
    assert!(t[a + 1..].iter().any(|v| v == "inspect:7"));
}
#[test]
fn zero_oversized_overflow_and_entire_batch_are_refused_before_api_access() {
    let (f, r) = Fake::reader();
    for (a, n) in [
        (0, 1),
        (PAYLOAD, 0),
        (PAYLOAD, MAX_READ + 1),
        (u64::MAX, 8),
        (USER_END - 1, 8),
    ] {
        assert!(r.read(a, n).is_err());
    }
    assert!(r.read_many(&[(PAYLOAD, 4), (PAYLOAD, 0)]).is_err());
    assert!(f.trace().is_empty());
}
#[test]
fn all_or_error_ordered_batch_does_not_expose_a_partial_result() {
    let (f, r) = Fake::reader();
    assert_eq!(
        r.read_many(&[(PAYLOAD, 4), (PAYLOAD + 4, 2)]).unwrap(),
        vec![vec![0x42; 4], vec![0x42; 2]]
    );
    f.mutate(|v| v.short = true);
    assert_eq!(r.read_many(&[(PAYLOAD, 4)]).unwrap_err().code, "short_read");
}
#[test]
fn short_read_is_never_padded_or_cached() {
    let (f, r) = Fake::reader();
    f.mutate(|v| v.short = true);
    assert_eq!(r.read(PAYLOAD, 8).unwrap_err().code, "short_read");
    f.mutate(|v| v.short = false);
    assert!(r.check().is_err());
}
#[test]
fn guarded_inaccessible_uncommitted_unknown_and_cross_region_reads_never_transfer() {
    for which in 0..7 {
        let (f, r) = Fake::reader();
        f.mutate(|v| match which {
            0 => v.region.protection = 0x104,
            1 => v.region.protection = 1,
            2 => v.region.protection = 0x10,
            3 => v.region.state = 0x2000,
            4 => v.region.kind = 0,
            5 => v.region.size = 4,
            _ => v.region.allocation = PAYLOAD + 8,
        });
        assert_eq!(r.read(PAYLOAD, 8).unwrap_err().code, "unreadable_memory");
        assert_eq!(f.reads(), 0);
    }
}
#[test]
fn readable_region_change_after_transfer_is_a_discarded_tear() {
    let (f, r) = Fake::reader();
    f.mutate(|v| v.region_changed = true);
    assert_eq!(r.read(PAYLOAD, 8).unwrap_err().code, "torn_read");
    assert_eq!(f.reads(), 1);
}
#[test]
fn failed_vm_handle_upgrade_closes_query_and_image_handles() {
    let (f, s, p) = Fake::new();
    f.mutate(|v| v.fail_open = 3);
    assert!(OwnedReader::generated(f.clone(), s, p).is_err());
    assert_eq!(
        f.trace().iter().filter(|v| v.starts_with("close:")).count(),
        4
    );
    assert_eq!(f.reads(), 0);
}
#[test]
fn identity_race_during_vm_handle_upgrade_refuses_before_loaded_header() {
    let (f, s, p) = Fake::new();
    f.mutate(|v| v.race_open = true);
    assert!(OwnedReader::generated(f.clone(), s, p).is_err());
    assert!(!f.trace().iter().any(|v| v.starts_with("read:")));
}
#[test]
fn exact_sha_rejects_same_pe_identity_with_changed_file_payload_before_vm_read() {
    let (f, s, p) = Fake::new();
    f.mutate(|v| {
        let mut altered = v.bytes.clone();
        altered[900] ^= 1;
        v.image.sha256 = Sha256::digest(&altered).into();
    });
    assert_eq!(
        OwnedReader::generated(f.clone(), s, p).err().unwrap().code,
        "unsupported_build"
    );
    assert!(!f.trace().iter().any(|v| v.starts_with("read:")));
}
#[test]
fn loaded_header_mismatch_never_returns_a_reader() {
    let (f, s, p) = Fake::new();
    f.mutate(|v| v.loaded_changed = true);
    assert_eq!(
        OwnedReader::generated(f.clone(), s, p).err().unwrap().code,
        "unsupported_build"
    );
    assert_eq!(f.reads(), 0);
}
#[test]
fn invalid_image_base_cannot_issue_a_remote_read() {
    for base in [0, BASE + 1, USER_END] {
        let (f, s, p) = Fake::new();
        f.mutate(|v| v.game.image_base = base);
        assert!(OwnedReader::generated(f.clone(), s, p).is_err());
        assert!(!f.trace().iter().any(|v| v.starts_with("read:")));
    }
}
#[test]
fn pe_identity_has_exact_timestamp_dimensions_and_unsupported_architecture_refusal() {
    let p = PeIdentity::parse(&image_bytes()).unwrap();
    assert_eq!(p.timestamp, 123456);
    assert_eq!(p.image_size, 65536);
    assert_eq!(p.header_size, 512);
    for machine in [0x14cu16, 0xaa64u16] {
        let mut b = image_bytes();
        b[0x84..0x86].copy_from_slice(&machine.to_le_bytes());
        assert!(PeIdentity::parse(&b).is_err());
    }
}
#[test]
fn pe_parser_rejects_truncation_wrap_section_overlap_and_invalid_optional_layout() {
    for which in 0..9 {
        let mut b = image_bytes();
        match which {
            0 => b.truncate(70),
            1 => b[0] = 0,
            2 => b[0x3c..0x40].copy_from_slice(&u32::MAX.to_le_bytes()),
            3 => b[0x86..0x88].copy_from_slice(&97u16.to_le_bytes()),
            4 => b[0x98..0x9a].copy_from_slice(&0x10bu16.to_le_bytes()),
            5 => b[0xd0..0xd4].copy_from_slice(&u32::MAX.to_le_bytes()),
            6 => b[0xd4..0xd8].copy_from_slice(&256u32.to_le_bytes()),
            7 => b[0x194..0x198].copy_from_slice(&0u32.to_le_bytes()),
            _ => b[0x190..0x194].copy_from_slice(&u32::MAX.to_le_bytes()),
        }
        assert!(PeIdentity::parse(&b).is_err(), "case {which}");
    }
}

#[test]
fn a_batch_has_two_full_checks_per_process_not_one_census_per_field() {
    let (f, r) = Fake::reader();
    let requests: Vec<_> = (0..64).map(|i| (PAYLOAD + i * 8, 8)).collect();
    assert_eq!(r.read_many(&requests).unwrap().len(), 64);
    let trace = f.trace();
    for pid in [6, 7] {
        assert_eq!(
            trace
                .iter()
                .filter(|v| **v == format!("inspect:{pid}"))
                .count(),
            2
        );
    }
    assert_eq!(f.reads(), 64);
    assert!(trace.iter().filter(|v| v.starts_with("fast:")).count() >= 256);
}
#[test]
fn caller_cancellation_between_transfers_discards_entire_batch_without_a_retry() {
    let (f, r) = Fake::reader();
    let error = r
        .read_many_checked(&[(PAYLOAD, 8), (PAYLOAD + 8, 8)], || {
            if f.reads() > 0 {
                Err(error("session_closed", "Original caller stopped"))
            } else {
                Ok(())
            }
        })
        .unwrap_err();
    assert_eq!(error.code, "session_closed");
    assert_eq!(f.reads(), 1);
    assert_eq!(f.trace().iter().filter(|v| **v == "inspect:7").count(), 2);
    f.reset();
    assert!(r.read(PAYLOAD, 8).is_err());
    assert!(f.trace().is_empty());
}
#[test]
fn late_full_owner_failure_discards_every_previously_read_batch_value() {
    let (f, r) = Fake::reader();
    f.mutate(|s| s.after = Some("owner")); // not a PID/creation change
    assert!(r.read_many(&[(PAYLOAD, 8), (PAYLOAD + 8, 8)]).is_err());
    assert_eq!(f.reads(), 2);
    assert!(r.check().is_err());
}
