// Process read semantics derived from Clausewitz-MCP calling/process/linux.py.
// Copyright (c) 2026 smerss (MIT). Ownership/build guards are new project code.
use super::*;
use crate::game::{BUILD_ID, SHA256};
use sha2::{Digest, Sha256};
use std::{
    fs::{self, File},
    io::Read,
    os::unix::fs::MetadataExt,
    sync::RwLock,
};
mod translated;
pub use translated::verify_translator;
pub(crate) use translated::{guest_mapping_present, is_reviewed_translator, FEX_SHA256};
#[derive(Clone, Debug, PartialEq, Eq)]
struct Region {
    start: u64,
    end: u64,
    readable: bool,
    permissions: [u8; 4],
    offset: u64,
    device: (u64, u64),
    inode: u64,
    path: Option<String>,
}
/// `parse_mappings` admits only ordered, non-overlapping regions. A read must
/// fit in one readable region, including at boundaries and across guard pages.
fn readable_span(regions: &[Region], address: u64, end: u64) -> bool {
    address < end
        && regions
            .partition_point(|region| region.start <= address)
            .checked_sub(1)
            .is_some_and(|index| {
                let region = &regions[index];
                region.readable && end <= region.end
            })
}
enum ExpectedBuild {
    VerifiedGame,
    // Generated ELF expectations cannot be selected by a production manifest,
    // environment variable, command-line option, or public runtime API.
    #[cfg(test)]
    GeneratedChild {
        build_id: String,
        sha256: String,
    },
}
const BOOT_ID_BOUND: usize = 64;
const AUXV_BOUND: usize = 4096;
const RANDOM_BYTES: usize = 16;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum KernelArch {
    X86_64,
    Aarch64,
}
impl KernelArch {
    fn from_elf(bytes: &[u8]) -> Result<Self> {
        if bytes.get(..7) != Some(&[0x7f, b'E', b'L', b'F', 2, 1, 1]) {
            return Err(Error::new(
                "unsupported_build",
                "Expected little-endian ELF64 kernel executable",
            ));
        }
        match u16::from_le_bytes(field(bytes, 18)?) {
            62 => Ok(Self::X86_64),
            183 => Ok(Self::Aarch64),
            _ => Err(Error::new(
                "unsupported_build",
                "Unreviewed kernel executable architecture",
            )),
        }
    }
    fn user_end(self) -> u64 {
        match self {
            Self::X86_64 => 1u64 << 47,
            Self::Aarch64 => 1u64 << 52,
        }
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
struct ExecWitness {
    boot: [u8; 16],
    random_address: u64,
    random: [u8; RANDOM_BYTES],
}
impl ExpectedBuild {
    fn build_id(&self) -> &str {
        match self {
            Self::VerifiedGame => BUILD_ID,
            #[cfg(test)]
            Self::GeneratedChild { build_id, .. } => build_id,
        }
    }
    fn sha256(&self) -> &str {
        match self {
            Self::VerifiedGame => SHA256,
            #[cfg(test)]
            Self::GeneratedChild { sha256, .. } => sha256,
        }
    }
}
pub struct LinuxProcess {
    session: Session,
    base: u64,
    regions: RwLock<Vec<Region>>,
    device: u64,
    inode: u64,
    // Captured from the owned process, never supplied by a session manifest.
    witness: ExecWitness,
    kernel_arch: KernelArch,
    translated: Option<translated::GuestImage>,
}
pub fn parse_stat(text: &str) -> Result<(char, i32, String)> {
    let close = text
        .rfind(") ")
        .ok_or_else(|| Error::new("invalid_identity", "Malformed process stat"))?;
    let fields: Vec<_> = text[close + 2..].split_whitespace().collect();
    if fields.len() < 20 {
        return Err(Error::new("invalid_identity", "Short process stat"));
    }
    let parent = fields[1]
        .parse()
        .map_err(|_| Error::new("invalid_identity", "Malformed parent PID"))?;
    if fields[19].parse::<u64>().is_err() {
        return Err(Error::new(
            "invalid_identity",
            "Malformed creation identity",
        ));
    }
    Ok((
        fields[0]
            .chars()
            .next()
            .ok_or_else(|| Error::new("invalid_identity", "No process state"))?,
        parent,
        fields[19].to_owned(),
    ))
}
fn identity(pid: i32) -> Result<(Identity, char, i32)> {
    if pid <= 0 {
        return Err(Error::new("invalid_identity", "PID must be positive"));
    }
    let p = std::path::PathBuf::from(format!("/proc/{pid}"));
    let (state, parent, start_ticks) = parse_stat(&fs::read_to_string(p.join("stat"))?)?;
    let exe = fs::read_link(p.join("exe"))?
        .into_os_string()
        .into_string()
        .map_err(|_| Error::new("invalid_identity", "Executable path is not UTF-8"))?;
    Ok((
        Identity {
            pid,
            start_ticks,
            exe,
            uid: fs::metadata(p)?.uid(),
        },
        state,
        parent,
    ))
}
pub fn capture_identity(pid: i32) -> Result<Identity> {
    Ok(identity(pid)?.0)
}
fn bounded_proc_bytes(path: &Path, bound: usize) -> Result<Vec<u8>> {
    let mut bytes = Vec::new();
    File::open(path)?
        .take(bound as u64 + 1)
        .read_to_end(&mut bytes)?;
    if bytes.len() > bound {
        return Err(Error::new(
            "invalid_exec_witness",
            "Process witness exceeds bound",
        ));
    }
    Ok(bytes)
}
fn parse_boot_id(bytes: &[u8]) -> Result<[u8; 16]> {
    let text = bytes.strip_suffix(b"\n").unwrap_or(bytes);
    if bytes.len() > BOOT_ID_BOUND || text.len() != 36 {
        return Err(Error::new(
            "invalid_exec_witness",
            "Malformed boot identity",
        ));
    }
    let mut hex = Vec::with_capacity(32);
    for (index, byte) in text.iter().copied().enumerate() {
        if [8, 13, 18, 23].contains(&index) {
            if byte != b'-' {
                return Err(Error::new(
                    "invalid_exec_witness",
                    "Malformed boot identity",
                ));
            }
        } else if byte.is_ascii_hexdigit() {
            hex.push(byte);
        } else {
            return Err(Error::new(
                "invalid_exec_witness",
                "Malformed boot identity",
            ));
        }
    }
    let mut boot = [0u8; 16];
    for (index, pair) in hex.chunks_exact(2).enumerate() {
        let digits = std::str::from_utf8(pair)
            .map_err(|_| Error::new("invalid_exec_witness", "Malformed boot identity"))?;
        boot[index] = u8::from_str_radix(digits, 16)
            .map_err(|_| Error::new("invalid_exec_witness", "Malformed boot identity"))?;
    }
    if boot == [0; 16] {
        return Err(Error::new("invalid_exec_witness", "Empty boot identity"));
    }
    Ok(boot)
}
fn boot_id() -> Result<[u8; 16]> {
    parse_boot_id(&bounded_proc_bytes(
        Path::new("/proc/sys/kernel/random/boot_id"),
        BOOT_ID_BOUND,
    )?)
}
#[cfg(test)]
fn parse_random_address(bytes: &[u8]) -> Result<u64> {
    parse_random_address_arch(bytes, KernelArch::X86_64)
}
fn parse_random_address_arch(bytes: &[u8], arch: KernelArch) -> Result<u64> {
    // Both reviewed kernel architectures use little-endian 64-bit auxv words.
    // Only the independently verified AArch64 translator gets its wider bound.
    if bytes.is_empty() || bytes.len() > AUXV_BOUND || bytes.len() % 16 != 0 {
        return Err(Error::new(
            "invalid_exec_witness",
            "Malformed auxiliary vector",
        ));
    }
    let mut address = None;
    let mut terminated = false;
    for (index, pair) in bytes.chunks_exact(16).enumerate() {
        let kind = u64::from_le_bytes(pair[..8].try_into().unwrap());
        let value = u64::from_le_bytes(pair[8..].try_into().unwrap());
        if kind == 0 {
            if value != 0 || (index + 1) * 16 != bytes.len() {
                return Err(Error::new(
                    "invalid_exec_witness",
                    "Malformed auxiliary terminator",
                ));
            }
            terminated = true;
            break;
        }
        if kind == 25 && address.replace(value).is_some() {
            return Err(Error::new(
                "invalid_exec_witness",
                "Duplicate AT_RANDOM witness",
            ));
        }
    }
    let address = address.ok_or_else(|| Error::new("invalid_exec_witness", "AT_RANDOM absent"))?;
    let end = address.checked_add(RANDOM_BYTES as u64);
    if !terminated || address < 0x10000 || !matches!(end, Some(end) if end <= arch.user_end()) {
        return Err(Error::new(
            "invalid_exec_witness",
            "AT_RANDOM outside bounded user address space",
        ));
    }
    Ok(address)
}
fn random_address(pid: i32, arch: KernelArch) -> Result<u64> {
    parse_random_address_arch(
        &bounded_proc_bytes(Path::new(&format!("/proc/{pid}/auxv")), AUXV_BOUND)?,
        arch,
    )
}
fn read_exact_remote(pid: i32, address: u64, data: &mut [u8]) -> Result<()> {
    if pid <= 0 || data.len() > 1 << 20 || address.checked_add(data.len() as u64).is_none() {
        return Err(Error::new("read_bound", "Invalid bounded process read"));
    }
    let remote_address = usize::try_from(address)
        .map_err(|_| Error::new("read_bound", "Remote address exceeds pointer width"))?;
    let local = libc::iovec {
        iov_base: data.as_mut_ptr().cast(),
        iov_len: data.len(),
    };
    let remote = libc::iovec {
        iov_base: remote_address as *mut libc::c_void,
        iov_len: data.len(),
    };
    // SAFETY: the local slice is valid for exactly its length. The remote
    // address is passed only to the read syscall, never dereferenced by Rust.
    // Callers bracket witness reads with ownership checks or apply map bounds.
    let n = unsafe { libc::process_vm_readv(pid, &local, 1, &remote, 1, 0) };
    if n < 0 {
        return Err(std::io::Error::last_os_error().into());
    }
    if n as usize != data.len() {
        return Err(Error::new(
            "short_read",
            "process_vm_readv returned incomplete data",
        ));
    }
    Ok(())
}
// Internal generated-child tests can exercise a kernel short read directly.
// Production calls are bracketed by ownership/exec/maps checks in read_many.
fn read_many_remote(pid: i32, requests: &[(u64, usize)]) -> Result<Vec<Vec<u8>>> {
    crate::memory::validate_read_batch(requests)?;
    if pid <= 0 {
        return Err(Error::new("read_bound", "Positive owned PID required"));
    }
    let mut output: Vec<Vec<u8>> = requests.iter().map(|(_, size)| vec![0; *size]).collect();
    for (request_chunk, buffer_chunk) in requests.chunks(128).zip(output.chunks_mut(128)) {
        let mut local = Vec::with_capacity(request_chunk.len());
        let mut remote = Vec::with_capacity(request_chunk.len());
        let mut expected = 0usize;
        for (&(address, size), bytes) in request_chunk.iter().zip(buffer_chunk.iter_mut()) {
            if size == 0 {
                continue;
            }
            let address = usize::try_from(address)
                .map_err(|_| Error::new("read_bound", "Remote pointer width"))?;
            local.push(libc::iovec {
                iov_base: bytes.as_mut_ptr().cast(),
                iov_len: size,
            });
            remote.push(libc::iovec {
                iov_base: address as *mut libc::c_void,
                iov_len: size,
            });
            expected += size;
        }
        if local.is_empty() {
            continue;
        }
        // SAFETY: all allocated local Vecs retain length/storage through the call;
        // <=128 disjoint valid local buffers, matching remote lengths, flags0.
        // Remote pointers are used only by the read syscall, never Rust derefs.
        let count = unsafe {
            libc::process_vm_readv(
                pid,
                local.as_ptr(),
                local.len() as _,
                remote.as_ptr(),
                remote.len() as _,
                0,
            )
        };
        if count < 0 {
            return Err(std::io::Error::last_os_error().into());
        }
        if count as usize != expected {
            return Err(Error::new(
                "short_read",
                "Incomplete vectored read; whole batch discarded",
            ));
        }
    }
    Ok(output)
}
#[cfg(all(test, target_arch = "x86_64"))]
fn exec_witness(pid: i32) -> Result<ExecWitness> {
    exec_witness_arch(pid, KernelArch::X86_64)
}
fn exec_witness_arch(pid: i32, arch: KernelArch) -> Result<ExecWitness> {
    let boot = boot_id()?;
    let address = random_address(pid, arch)?;
    let mut random = [0u8; RANDOM_BYTES];
    // Exactly 16 bytes at the kernel-provided, bounded AT_RANDOM pointer. This
    // independent primitive must not call ReadMemory::read or Process::check.
    read_exact_remote(pid, address, &mut random)?;
    if random_address(pid, arch)? != address || boot_id()? != boot {
        return Err(Error::new(
            "ownership_changed",
            "Exec or boot changed while sampling witness",
        ));
    }
    Ok(ExecWitness {
        boot,
        random_address: address,
        random,
    })
}
pub fn verify_executable(path: &Path) -> Result<()> {
    executable_bytes(path, &ExpectedBuild::VerifiedGame).map(|_| ())
}
fn executable_bytes(path: &Path, expected: &ExpectedBuild) -> Result<Vec<u8>> {
    let file = File::open(path)?;
    let metadata = file.metadata()?;
    if !metadata.is_file() || metadata.len() > 256 << 20 {
        return Err(Error::new(
            "invalid_elf",
            "Executable exceeds bound or is not a file",
        ));
    }
    let mut bytes = Vec::new();
    file.take((256 << 20) + 1).read_to_end(&mut bytes)?;
    if bytes.len() > 256 << 20 {
        return Err(Error::new("invalid_elf", "Executable exceeds bound"));
    }
    if format!("{:x}", Sha256::digest(&bytes)) != expected.sha256()
        || elf_build_id(&bytes)? != expected.build_id()
    {
        return Err(Error::new(
            "unsupported_build",
            "Actual executable hash/build differs",
        ));
    }
    Ok(bytes)
}
fn mappings(pid: i32) -> Result<Vec<Region>> {
    let path = format!("/proc/{pid}/maps");
    let mut text = String::new();
    File::open(path)?
        .take((4 << 20) + 1)
        .read_to_string(&mut text)?;
    if text.len() > 4 << 20 {
        return Err(Error::new("invalid_maps", "Process mappings exceed bound"));
    }
    parse_mappings(&text)
}
fn parse_mappings(text: &str) -> Result<Vec<Region>> {
    let mut regions = Vec::new();
    let mut previous_end = 0;
    for line in text.lines() {
        let f: Vec<_> = line.split_whitespace().collect();
        if f.len() < 5 {
            return Err(Error::new("invalid_maps", "Short mapping"));
        }
        let permissions = f[1].as_bytes();
        if permissions.len() != 4
            || !matches!(permissions[0], b'r' | b'-')
            || !matches!(permissions[1], b'w' | b'-')
            || !matches!(permissions[2], b'x' | b'-')
            || !matches!(permissions[3], b'p' | b's')
        {
            return Err(Error::new("invalid_maps", "Malformed mapping permissions"));
        }
        let (lo, hi) = f[0]
            .split_once('-')
            .ok_or_else(|| Error::new("invalid_maps", "Malformed mapping"))?;
        let parse = |s: &str| {
            u64::from_str_radix(s, 16).map_err(|_| Error::new("invalid_maps", "Malformed address"))
        };
        let (start, end) = (parse(lo)?, parse(hi)?);
        if start >= end {
            return Err(Error::new("invalid_maps", "Invalid mapping range"));
        }
        if start < previous_end {
            return Err(Error::new(
                "invalid_maps",
                "Mapping ranges overlap or are out of order",
            ));
        }
        previous_end = end;
        let (major, minor) = f[3]
            .split_once(':')
            .ok_or_else(|| Error::new("invalid_maps", "Malformed mapping device"))?;
        let inode = f[4]
            .parse()
            .map_err(|_| Error::new("invalid_maps", "Malformed mapping inode"))?;
        regions.push(Region {
            start,
            end,
            readable: permissions[0] == b'r',
            permissions: permissions.try_into().unwrap(),
            offset: parse(f[2])?,
            device: (parse(major)?, parse(minor)?),
            inode,
            path: (f.len() > 5).then(|| f[5..].join(" ")),
        });
    }
    if regions.is_empty() {
        return Err(Error::new("invalid_maps", "Process mappings are empty"));
    }
    Ok(regions)
}
fn field<const N: usize>(data: &[u8], a: usize) -> Result<[u8; N]> {
    data.get(
        a..a.checked_add(N)
            .ok_or_else(|| Error::new("invalid_elf", "Offset overflow"))?,
    )
    .and_then(|v| v.try_into().ok())
    .ok_or_else(|| Error::new("invalid_elf", "Truncated ELF field"))
}
pub fn elf_build_id(data: &[u8]) -> Result<String> {
    if data.get(..6) != Some(&[0x7f, b'E', b'L', b'F', 2, 1])
        || u16::from_le_bytes(field(data, 18)?) != 62
    {
        return Err(Error::new(
            "unsupported_build",
            "Expected little-endian ELF64 x86_64",
        ));
    }
    let offset = usize::try_from(u64::from_le_bytes(field(data, 32)?))
        .map_err(|_| Error::new("invalid_elf", "Program header offset"))?;
    let stride = usize::from(u16::from_le_bytes(field(data, 54)?));
    let count = usize::from(u16::from_le_bytes(field(data, 56)?));
    if stride < 56 || count > 4096 {
        return Err(Error::new("invalid_elf", "Program header bound"));
    }
    for i in 0..count {
        let a = offset
            .checked_add(i * stride)
            .ok_or_else(|| Error::new("invalid_elf", "Program header overflow"))?;
        if u32::from_le_bytes(field(data, a)?) != 4 {
            continue;
        }
        let off = usize::try_from(u64::from_le_bytes(field(data, a + 8)?))
            .map_err(|_| Error::new("invalid_elf", "Note offset"))?;
        let size = usize::try_from(u64::from_le_bytes(field(data, a + 32)?))
            .map_err(|_| Error::new("invalid_elf", "Note size"))?;
        let end = off
            .checked_add(size)
            .filter(|v| *v <= data.len())
            .ok_or_else(|| Error::new("invalid_elf", "Truncated note"))?;
        let mut n = off;
        while n < end {
            let names = u32::from_le_bytes(field(data, n)?) as usize;
            let desc = u32::from_le_bytes(field(data, n + 4)?) as usize;
            let kind = u32::from_le_bytes(field(data, n + 8)?);
            if names > 4096 || desc > 65536 {
                return Err(Error::new("invalid_elf", "Note bound"));
            }
            let name = n + 12;
            let value = name + ((names + 3) & !3);
            let next = value + ((desc + 3) & !3);
            if next > end {
                return Err(Error::new("invalid_elf", "Truncated note record"));
            }
            if kind == 3 && data.get(name..name + names) == Some(b"GNU\0".as_slice()) {
                return Ok(data[value..value + desc]
                    .iter()
                    .map(|v| format!("{v:02x}"))
                    .collect());
            }
            n = next;
        }
    }
    Err(Error::new("unsupported_build", "GNU build ID missing"))
}
impl LinuxProcess {
    pub fn open(path: &Path) -> Result<Self> {
        Self::open_expected(load_session(path)?, ExpectedBuild::VerifiedGame)
    }
    fn open_expected(session: Session, expected: ExpectedBuild) -> Result<Self> {
        if session.build_id != expected.build_id() || session.sha256 != expected.sha256() {
            return Err(Error::new(
                "unsupported_build",
                "Session executable build is not verified",
            ));
        }
        let meta = fs::metadata(format!("/proc/{}/exe", session.game.pid))?;
        check_ownership(&session, meta.dev(), meta.ino())?;
        let exe_path = format!("/proc/{}/exe", session.game.pid);
        let (bytes, kernel_arch) = match &session.translated {
            Some(image) => (
                translated::translator_bytes(Path::new(&exe_path), &image.translator_sha256)?,
                KernelArch::Aarch64,
            ),
            None => (
                executable_bytes(Path::new(&exe_path), &expected)?,
                KernelArch::X86_64,
            ),
        };
        if KernelArch::from_elf(&bytes)? != kernel_arch {
            return Err(Error::new(
                "unsupported_build",
                "Kernel executable architecture differs from explicit session mode",
            ));
        }
        check_ownership(&session, meta.dev(), meta.ino())?;
        let witness = exec_witness_arch(session.game.pid, kernel_arch)?;
        check_ownership(&session, meta.dev(), meta.ino())?;
        let mut process = Self {
            session,
            base: 0,
            regions: RwLock::new(Vec::new()),
            device: meta.dev(),
            inode: meta.ino(),
            witness,
            kernel_arch,
            translated: None,
        };
        process.check()?;
        let regions = mappings(process.session.game.pid)?;
        if let Some(image) = &process.session.translated {
            let guest = translated::GuestImage::capture(
                process.session.game.pid,
                image,
                &expected,
                &regions,
            )?;
            process.base = guest.base();
            process.translated = Some(guest);
            *process
                .regions
                .get_mut()
                .map_err(|_| Error::new("invalid_maps", "Mapping cache poisoned"))? = regions;
            process.check()?;
            return Ok(process);
        }
        let expected_device = (
            u64::from(libc::major(process.device)),
            u64::from(libc::minor(process.device)),
        );
        let mut base = None;
        for region in &regions {
            if region.path.as_deref() == Some(process.session.game.exe.as_str()) {
                if region.inode != process.inode || region.device != expected_device {
                    return Err(Error::new(
                        "invalid_maps",
                        "Executable mapping device/inode differs",
                    ));
                }
                // PT_LOAD virtual/file offsets can differ across segments.
                // This verified PIE has its ELF header at virtual address zero.
                if region.offset == 0 && base.replace(region.start).is_some() {
                    return Err(Error::new(
                        "invalid_maps",
                        "Ambiguous executable header mapping",
                    ));
                }
            }
        }
        *process
            .regions
            .get_mut()
            .map_err(|_| Error::new("invalid_maps", "Mapping cache poisoned"))? = regions;
        process.base =
            base.ok_or_else(|| Error::new("invalid_maps", "Owned executable mapping unavailable"))?;
        if process.read(process.base, 64)? != bytes[..64] {
            return Err(Error::new(
                "unsupported_build",
                "Mapped executable header differs",
            ));
        }
        process.check()?;
        Ok(process)
    }
}
fn check_ownership(session: &Session, device: u64, inode: u64) -> Result<()> {
    // SAFETY: getuid has no arguments and no memory effects.
    let uid = unsafe { libc::getuid() };
    if uid == 0 || session.game.uid != uid || session.supervisor.uid != uid {
        return Err(Error::new(
            "ownership_changed",
            "Runtime and owned game must use the same ordinary user",
        ));
    }
    let (game, state, parent) = identity(session.game.pid)?;
    let (supervisor, sup_state, _) = identity(session.supervisor.pid)?;
    let meta = fs::metadata(format!("/proc/{}/exe", game.pid))?;
    if game != session.game
        || supervisor != session.supervisor
        || parent != supervisor.pid
        || matches!(state, 'Z' | 'X')
        || matches!(sup_state, 'Z' | 'X')
        || meta.dev() != device
        || meta.ino() != inode
    {
        return Err(Error::new(
            "ownership_changed",
            "PID creation, parent or executable ownership changed",
        ));
    }
    Ok(())
}
impl Process for LinuxProcess {
    fn begin_snapshot(&self) -> Result<()> {
        self.check()?;
        let regions = mappings(self.session.game.pid)?;
        *self
            .regions
            .write()
            .map_err(|_| Error::new("invalid_maps", "Mapping cache poisoned"))? = regions;
        self.check()
    }
    fn check(&self) -> Result<()> {
        check_ownership(&self.session, self.device, self.inode)?;
        if exec_witness_arch(self.session.game.pid, self.kernel_arch)? != self.witness {
            return Err(Error::new(
                "ownership_changed",
                "Boot or owned executable address-space witness changed",
            ));
        }
        if let Some(guest) = &self.translated {
            guest.check(self.session.game.pid, &mappings(self.session.game.pid)?)?;
        }
        // A successful witness read alone does not validate PID ownership.
        check_ownership(&self.session, self.device, self.inode)
    }
    fn image_base(&self) -> u64 {
        self.base
    }
    fn identity(&self) -> Option<&Identity> {
        Some(&self.session.game)
    }
}
impl ReadMemory for LinuxProcess {
    fn read_many(&self, requests: &[(u64, usize)]) -> Result<Vec<Vec<u8>>> {
        crate::memory::validate_read_batch(requests)?;
        // No public selectable PID, build relaxation or stale map cache reuse.
        self.begin_snapshot()?;
        let mapped = |regions: &[Region]| {
            requests.iter().all(|&(address, size)| {
                size == 0 || readable_span(regions, address, address + size as u64)
            })
        };
        if !mapped(
            &self
                .regions
                .read()
                .map_err(|_| Error::new("invalid_maps", "Mapping cache poisoned"))?,
        ) {
            return Err(Error::new(
                "unmapped_read",
                "Batch outside readable mapped regions",
            ));
        }
        let primary = read_many_remote(self.session.game.pid, requests);
        self.finish_read_many(requests, primary)
    }

    fn read(&self, address: u64, size: usize) -> Result<Vec<u8>> {
        if size == 0 {
            return Ok(Vec::new());
        }
        let end = address
            .checked_add(size as u64)
            .ok_or_else(|| Error::new("read_bound", "Address overflow"))?;
        if size > 1 << 20 {
            return Err(Error::new("read_bound", "Read exceeds byte bound"));
        }
        let contains = |regions: &[Region]| readable_span(regions, address, end);
        let available = contains(
            &self
                .regions
                .read()
                .map_err(|_| Error::new("invalid_maps", "Mapping cache poisoned"))?,
        );
        if !available {
            // A parent-bound reader can open before the game allocates campaign
            // objects. Refresh only for an address outside the known map; never
            // rediscover a PID or override a kernel permission failure.
            self.check()?;
            let regions = mappings(self.session.game.pid)?;
            self.check()?;
            if !contains(&regions) {
                return Err(Error::new(
                    "unmapped_read",
                    "Outside bounded readable process mapping",
                ));
            }
            *self
                .regions
                .write()
                .map_err(|_| Error::new("invalid_maps", "Mapping cache poisoned"))? = regions;
        }
        // Scalar fields use cached bounds in both modes. The caller must bracket
        // the complete observation with begin_snapshot/check and discard every
        // field if the final identity/guest witness fails. Only read_many owns
        // an independent final bracket; scalar reads are not freshness proofs.
        let mut data = vec![0; size];
        read_exact_remote(self.session.game.pid, address, &mut data)?;
        Ok(data)
    }
}

impl LinuxProcess {
    // Isolated final bracket also gives generated tests a deterministic seam:
    // they can exec/protect their child after a real read, before verification.
    fn finish_read_many(
        &self,
        requests: &[(u64, usize)],
        primary: Result<Vec<Vec<u8>>>,
    ) -> Result<Vec<Vec<u8>>> {
        let mapped = |regions: &[Region]| {
            requests.iter().all(|&(address, size)| {
                size == 0 || readable_span(regions, address, address + size as u64)
            })
        };
        // Even a kernel failure gets the independent ownership/exec/maps bracket.
        let after = self.begin_snapshot().and_then(|()| {
            if mapped(
                &self
                    .regions
                    .read()
                    .map_err(|_| Error::new("invalid_maps", "Mapping cache poisoned"))?,
            ) {
                Ok(())
            } else {
                Err(Error::new("unmapped_read", "Batch mappings changed"))
            }
        });
        match (primary, after) {
            (Ok(bytes), Ok(())) => Ok(bytes),
            (Err(mut first), Err(second)) => {
                first
                    .message
                    .push_str(&format!("; final verification: {second}"));
                Err(first)
            }
            (Err(first), Ok(())) => Err(first),
            (Ok(_), Err(second)) => Err(second),
        }
    }
}

#[cfg(test)]
mod tests;
