// Process read semantics derived from Clausewitz-MCP calling/process/linux.py.
// Copyright (c) 2026 smerss (MIT). Ownership/build guards are new project code.
use super::*;
use crate::game::{BUILD_ID, SHA256};
use sha2::{Digest, Sha256};
use std::{
    fs::{self, File},
    io::Read,
    os::unix::fs::MetadataExt,
};
struct Region {
    start: u64,
    end: u64,
    readable: bool,
}
pub struct LinuxProcess {
    session: Session,
    base: u64,
    regions: Vec<Region>,
    device: u64,
    inode: u64,
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
        let session = load_session(path)?;
        if session.build_id != BUILD_ID || session.sha256 != SHA256 {
            return Err(Error::new(
                "unsupported_build",
                "Session executable build is not verified",
            ));
        }
        let meta = fs::metadata(format!("/proc/{}/exe", session.game.pid))?;
        let mut process = Self {
            session,
            base: 0,
            regions: Vec::new(),
            device: meta.dev(),
            inode: meta.ino(),
        };
        process.check()?;
        let exe_path = format!("/proc/{}/exe", process.session.game.pid);
        let mut file = File::open(&exe_path)?;
        if file.metadata()?.len() > 256 << 20 {
            return Err(Error::new("invalid_elf", "Executable exceeds bound"));
        }
        let mut bytes = Vec::new();
        file.read_to_end(&mut bytes)?;
        if format!("{:x}", Sha256::digest(&bytes)) != SHA256 || elf_build_id(&bytes)? != BUILD_ID {
            return Err(Error::new(
                "unsupported_build",
                "Actual executable hash/build differs",
            ));
        }
        let maps = fs::read_to_string(format!("/proc/{}/maps", process.session.game.pid))?;
        let mut base = None;
        for line in maps.lines() {
            let fields: Vec<_> = line.split_whitespace().collect();
            if fields.len() < 5 {
                return Err(Error::new("invalid_maps", "Short mapping"));
            }
            let (lo, hi) = fields[0]
                .split_once('-')
                .ok_or_else(|| Error::new("invalid_maps", "Malformed mapping"))?;
            let number = |s: &str| {
                u64::from_str_radix(s, 16)
                    .map_err(|_| Error::new("invalid_maps", "Malformed address"))
            };
            let start = number(lo)?;
            let end = number(hi)?;
            process.regions.push(Region {
                start,
                end,
                readable: fields[1].starts_with('r'),
            });
            if fields.len() > 5 && fields[5..].join(" ") == process.session.game.exe {
                if fields[4].parse::<u64>().ok() != Some(process.inode) {
                    return Err(Error::new(
                        "invalid_maps",
                        "Executable mapping inode differs",
                    ));
                }
                // PT_LOAD virtual/file offsets can differ across segments.
                // This verified PIE has its ELF header at virtual address zero.
                if number(fields[2])? == 0 && base.replace(start).is_some() {
                    return Err(Error::new(
                        "invalid_maps",
                        "Ambiguous executable header mapping",
                    ));
                }
            }
        }
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
impl Process for LinuxProcess {
    fn check(&self) -> Result<()> {
        // SAFETY: getuid has no arguments and no memory effects.
        let uid = unsafe { libc::getuid() };
        if uid == 0 || self.session.game.uid != uid || self.session.supervisor.uid != uid {
            return Err(Error::new(
                "ownership_changed",
                "Runtime and owned game must use the same ordinary user",
            ));
        }
        let (game, state, parent) = identity(self.session.game.pid)?;
        let (supervisor, sup_state, _) = identity(self.session.supervisor.pid)?;
        let meta = fs::metadata(format!("/proc/{}/exe", game.pid))?;
        if game != self.session.game
            || supervisor != self.session.supervisor
            || parent != supervisor.pid
            || matches!(state, 'Z' | 'X')
            || matches!(sup_state, 'Z' | 'X')
            || meta.dev() != self.device
            || meta.ino() != self.inode
        {
            return Err(Error::new(
                "ownership_changed",
                "PID creation, parent or executable ownership changed",
            ));
        }
        Ok(())
    }
    fn image_base(&self) -> u64 {
        self.base
    }
}
impl ReadMemory for LinuxProcess {
    fn read(&self, address: u64, size: usize) -> Result<Vec<u8>> {
        if size == 0 {
            return Ok(Vec::new());
        }
        let end = address
            .checked_add(size as u64)
            .ok_or_else(|| Error::new("read_bound", "Address overflow"))?;
        if size > 1 << 20
            || !self
                .regions
                .iter()
                .any(|m| m.readable && m.start <= address && end <= m.end)
        {
            return Err(Error::new(
                "unmapped_read",
                "Outside bounded readable process mapping",
            ));
        }
        let mut data = vec![0; size];
        let local = libc::iovec {
            iov_base: data.as_mut_ptr().cast(),
            iov_len: size,
        };
        let remote = libc::iovec {
            iov_base: address as usize as *mut libc::c_void,
            iov_len: size,
        };
        // SAFETY: local buffer is allocated for size bytes; remote iovec is an
        // address in the checked owned process, never dereferenced by Rust.
        let n = unsafe { libc::process_vm_readv(self.session.game.pid, &local, 1, &remote, 1, 0) };
        if n < 0 {
            return Err(std::io::Error::last_os_error().into());
        }
        if n as usize != size {
            return Err(Error::new(
                "short_read",
                "process_vm_readv returned incomplete data",
            ));
        }
        Ok(data)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn identity_comm_parentheses() {
        let text = format!(
            "44 (game (worker) name) R 12 {} 789",
            vec!["0"; 17].join(" ")
        );
        assert_eq!(parse_stat(&text).unwrap(), ('R', 12, "789".into()));
        assert!(parse_stat("44 (short) R 12").is_err());
    }
    #[test]
    fn malformed_elf_explicitly_refused() {
        assert!(elf_build_id(&[0; 64]).is_err());
    }
}
