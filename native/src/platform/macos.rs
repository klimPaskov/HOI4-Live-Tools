//! Read-only task access to one owned macOS game process (x86_64 Mach-O under
//! Rosetta). The calling binary must be signed with the
//! `com.apple.security.cs.debugger` entitlement and the user must hold the
//! `_developer` task-port authorization. Never root, never SIP changes and
//! never re-signing the game; a denied task port is a capability error.
use super::{Identity, Process, Session};
use crate::{
    error::{Error, Result},
    memory::{ReadMemory, StringAbi},
};
use sha2::{Digest, Sha256};
use std::ffi::CStr;

/// The only Mach-O build whose layout has been inspected (1.19.3.0 c01a).
pub const BUILD_UUID: &str = "089EA519-6780-353B-8E39-4E9309B444D4";
pub const SHA256: &str = "09419b087f8b4656c00df337bf838ff34c9b1d816b76cef5383ae6a4ac15b1fb";

type MachPort = u32;
type KernReturn = i32;
extern "C" {
    static mach_task_self_: MachPort;
    fn mach_vm_read_overwrite(
        task: MachPort,
        address: u64,
        size: u64,
        data: u64,
        out_size: *mut u64,
    ) -> KernReturn;
    fn mach_vm_region_recurse(
        task: MachPort,
        address: *mut u64,
        size: *mut u64,
        depth: *mut u32,
        info: *mut i32,
        count: *mut u32,
    ) -> KernReturn;
    fn mach_port_deallocate(task: MachPort, name: MachPort) -> KernReturn;
}
/// natural_t count of vm_region_submap_info_64 (packed to 4 bytes).
const SUBMAP_INFO_COUNT: u32 = 19;
const MH_MAGIC_64: u32 = 0xfeed_facf;
const CPU_TYPE_X86_64: u32 = 0x0100_0007;
const MH_EXECUTE: u32 = 2;
const LC_UUID: u32 = 0x1b;

/// Current identity of `pid` from the kernel: start time, executable, uid.
pub fn capture_identity(pid: i32) -> Result<Identity> {
    // SAFETY: proc_bsdinfo is plain data; the kernel fills at most its size.
    let mut info: libc::proc_bsdinfo = unsafe { std::mem::zeroed() };
    let size = std::mem::size_of::<libc::proc_bsdinfo>() as i32;
    // SAFETY: the buffer is valid for `size` bytes.
    let got = unsafe {
        libc::proc_pidinfo(
            pid,
            libc::PROC_PIDTBSDINFO,
            0,
            (&mut info as *mut libc::proc_bsdinfo).cast(),
            size,
        )
    };
    if got != size {
        return Err(Error::new(
            "process_absent",
            "Game process identity unavailable",
        ));
    }
    let mut path = [0u8; 4096];
    // SAFETY: the buffer is valid for its length.
    let n = unsafe { libc::proc_pidpath(pid, path.as_mut_ptr().cast(), path.len() as u32) };
    if n <= 0 {
        return Err(Error::new(
            "process_absent",
            "Game executable path unavailable",
        ));
    }
    let exe = CStr::from_bytes_until_nul(&path)
        .map_err(|_| Error::new("process_absent", "Invalid executable path"))?
        .to_string_lossy()
        .into_owned();
    Ok(Identity {
        pid,
        start_ticks: format!("{}.{:06}", info.pbi_start_tvsec, info.pbi_start_tvusec),
        exe,
        uid: info.pbi_uid,
    })
}

/// Verify the executable file is the inspected Mach-O build.
pub fn verify_executable(path: &str) -> Result<()> {
    let bytes = std::fs::read(path)?;
    if format!("{:x}", Sha256::digest(&bytes)) != SHA256 || macho_uuid(&bytes)? != BUILD_UUID {
        return Err(Error::new(
            "unsupported_build",
            "Game executable is not the inspected macOS build",
        ));
    }
    Ok(())
}

/// LC_UUID of a thin x86_64 MH_EXECUTE image, formatted 8-4-4-4-12 uppercase.
pub fn macho_uuid(image: &[u8]) -> Result<String> {
    let word = |o: usize| -> Result<u32> {
        image
            .get(o..o + 4)
            .map(|b| u32::from_le_bytes(b.try_into().unwrap()))
            .ok_or_else(|| Error::new("unsupported_build", "Truncated Mach-O header"))
    };
    if word(0)? != MH_MAGIC_64 || word(4)? != CPU_TYPE_X86_64 || word(12)? != MH_EXECUTE {
        return Err(Error::new(
            "unsupported_build",
            "Not a thin x86_64 Mach-O executable",
        ));
    }
    let (count, size) = (word(16)? as usize, word(20)? as usize);
    let end = 32usize
        .checked_add(size)
        .filter(|&e| e <= image.len() && count <= 512)
        .ok_or_else(|| Error::new("unsupported_build", "Mach-O load commands out of bounds"))?;
    let mut at = 32;
    for _ in 0..count {
        let (cmd, len) = (word(at)?, word(at + 4)? as usize);
        if len < 8 || at + len > end {
            return Err(Error::new(
                "unsupported_build",
                "Invalid Mach-O load command",
            ));
        }
        if cmd == LC_UUID && len == 24 {
            let u = &image[at + 8..at + 24];
            let hex = u.iter().map(|b| format!("{b:02X}")).collect::<String>();
            return Ok(format!(
                "{}-{}-{}-{}-{}",
                &hex[..8],
                &hex[8..12],
                &hex[12..16],
                &hex[16..20],
                &hex[20..]
            ));
        }
        at += len;
    }
    Err(Error::new("unsupported_build", "Mach-O UUID absent"))
}

pub struct MacProcess {
    session: Session,
    task: MachPort,
    base: u64,
}
impl MacProcess {
    pub fn open(path: &std::path::Path) -> Result<Self> {
        Self::attach(super::load_session(path)?)
    }
    pub fn attach(session: Session) -> Result<Self> {
        if session.translated.is_some() {
            return Err(Error::new(
                "platform_unavailable",
                "Linux FEX sessions cannot use the macOS adapter",
            ));
        }
        if session.build_id != BUILD_UUID || session.sha256 != SHA256 {
            return Err(Error::new(
                "unsupported_build",
                "Session executable build is not verified",
            ));
        }
        check_ownership(&session)?;
        verify_executable(&session.game.exe)?;
        let mut task: MachPort = 0;
        // SAFETY: task_for_pid writes one port name into `task`.
        let kr = unsafe { libc::task_for_pid(mach_task_self_, session.game.pid, &mut task) };
        if kr != 0 {
            return Err(Error::new(
                "task_access_denied",
                "Read-only task access denied; sign with the debugger entitlement and authorize Developer Tools access",
            ));
        }
        let mut process = Self {
            session,
            task,
            base: 0,
        };
        process.base = process.find_image()?;
        // Ownership again after attaching, so the port belongs to the same process.
        check_ownership(&process.session)?;
        Ok(process)
    }
    /// The main image is the x86_64 MH_EXECUTE header carrying the verified UUID.
    /// UUID of a Mach-O image mapped at `address`: header first, then exactly
    /// its declared load commands (bounded), never a guessed window.
    fn image_uuid(&self, address: u64) -> Option<String> {
        let header = self.read(address, 32).ok()?;
        let size = u32::from_le_bytes(header[20..24].try_into().unwrap()) as usize;
        if u32::from_le_bytes(header[..4].try_into().unwrap()) != MH_MAGIC_64 || size > 1 << 16 {
            return None;
        }
        macho_uuid(&self.read(address, 32 + size).ok()?).ok()
    }
    fn find_image(&self) -> Result<u64> {
        let (mut address, mut depth) = (0u64, 0u32);
        for _ in 0..1_000_000 {
            let (mut size, mut info, mut count) = (0u64, [0i32; 19], SUBMAP_INFO_COUNT);
            // SAFETY: all out pointers are valid; info holds SUBMAP_INFO_COUNT words.
            let kr = unsafe {
                mach_vm_region_recurse(
                    self.task,
                    &mut address,
                    &mut size,
                    &mut depth,
                    info.as_mut_ptr(),
                    &mut count,
                )
            };
            if kr != 0 {
                break;
            }
            // is_submap is the boolean_t at byte 48 of the packed struct.
            if info[12] != 0 {
                depth += 1;
                continue;
            }
            if self.image_uuid(address).as_deref() == Some(BUILD_UUID) {
                return Ok(address);
            }
            address = address.saturating_add(size);
        }
        Err(Error::new(
            "unsupported_build",
            "Verified game image not mapped in the owned process",
        ))
    }
}
fn check_ownership(session: &Session) -> Result<()> {
    // SAFETY: getuid has no arguments and no memory effects.
    let uid = unsafe { libc::getuid() };
    if uid == 0 || session.game.uid != uid {
        return Err(Error::new(
            "ownership_changed",
            "Owned non-root game process required",
        ));
    }
    if capture_identity(session.game.pid)? != session.game {
        return Err(Error::new(
            "ownership_changed",
            "Game process identity changed",
        ));
    }
    Ok(())
}
impl Drop for MacProcess {
    fn drop(&mut self) {
        // SAFETY: releases the send right obtained from task_for_pid.
        unsafe {
            mach_port_deallocate(mach_task_self_, self.task);
        }
    }
}
impl Process for MacProcess {
    fn check(&self) -> Result<()> {
        check_ownership(&self.session)
    }
    fn image_base(&self) -> u64 {
        self.base
    }
    fn identity(&self) -> Option<&Identity> {
        Some(&self.session.game)
    }
}
impl ReadMemory for MacProcess {
    fn read(&self, address: u64, size: usize) -> Result<Vec<u8>> {
        let mut buffer = vec![0u8; size];
        let mut got = 0u64;
        // SAFETY: buffer is valid for `size` bytes; the kernel copies at most that.
        let kr = unsafe {
            mach_vm_read_overwrite(
                self.task,
                address,
                size as u64,
                buffer.as_mut_ptr() as u64,
                &mut got,
            )
        };
        if kr != 0 || got != size as u64 {
            return Err(Error::new("read_failed", "Process memory read failed"));
        }
        Ok(buffer)
    }
    fn string_abi(&self) -> StringAbi {
        StringAbi::LibCxx
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn image(uuid: [u8; 16], cputype: u32) -> Vec<u8> {
        let mut d = vec![0u8; 256];
        for (o, v) in [
            (0, MH_MAGIC_64),
            (4, cputype),
            (12, MH_EXECUTE),
            (16, 2),
            (20, 40),
        ] {
            d[o..o + 4].copy_from_slice(&v.to_le_bytes());
        }
        // A 16-byte unrelated command, then LC_UUID.
        d[32..36].copy_from_slice(&0x2u32.to_le_bytes());
        d[36..40].copy_from_slice(&16u32.to_le_bytes());
        d[48..52].copy_from_slice(&LC_UUID.to_le_bytes());
        d[52..56].copy_from_slice(&24u32.to_le_bytes());
        d[56..72].copy_from_slice(&uuid);
        d
    }
    const UUID: [u8; 16] = [
        0x08, 0x9E, 0xA5, 0x19, 0x67, 0x80, 0x35, 0x3B, 0x8E, 0x39, 0x4E, 0x93, 0x09, 0xB4, 0x44,
        0xD4,
    ];
    #[test]
    fn macho_uuid_reads_the_verified_build_and_refuses_other_images() {
        assert_eq!(
            macho_uuid(&image(UUID, CPU_TYPE_X86_64)).unwrap(),
            BUILD_UUID
        );
        // arm64 slices, truncated headers and runaway command tables refuse.
        assert!(macho_uuid(&image(UUID, 0x0100_000c)).is_err());
        assert!(macho_uuid(&image(UUID, CPU_TYPE_X86_64)[..20]).is_err());
        let mut runaway = image(UUID, CPU_TYPE_X86_64);
        runaway[20..24].copy_from_slice(&100_000u32.to_le_bytes());
        assert!(macho_uuid(&runaway).is_err());
        let mut zero = image(UUID, CPU_TYPE_X86_64);
        zero[36..40].copy_from_slice(&0u32.to_le_bytes());
        assert!(macho_uuid(&zero).is_err());
    }
    #[test]
    fn own_process_identity_is_stable_and_non_root() {
        // SAFETY: getpid has no arguments and no memory effects.
        let pid = unsafe { libc::getpid() };
        let a = capture_identity(pid).unwrap();
        assert_eq!(a, capture_identity(pid).unwrap());
        assert!(a.exe.starts_with('/'));
        assert!(capture_identity(-1).is_err());
    }
}
