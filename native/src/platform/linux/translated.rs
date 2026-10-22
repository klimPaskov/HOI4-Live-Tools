//! Explicit FEX reader mode. Guest bytes retain the existing x86 game pin.
//! Mapping continuity is not an independent guest execution-generation proof.
use super::*;
use std::path::PathBuf;

pub(crate) const FEX_SHA256: &str =
    "bb7d1d4147e224ab83eef02c3fae59e86a297fa829c847a4afcc273f106f6057";
enum TranslatorBuild {
    Supported,
    #[cfg(test)]
    Generated(String),
}
pub fn verify_translator(path: &Path, expected_sha256: &str) -> Result<()> {
    translator_bytes(path, expected_sha256).map(|_| ())
}
/// Content-based candidate filter for the owner's duplicate scan. Unknown
/// binaries are not translators; I/O denial remains distinguishable to caller.
pub(crate) fn is_reviewed_translator(path: &Path) -> Result<bool> {
    let mut file = File::open(path)?;
    let metadata = file.metadata()?;
    if !metadata.is_file() || metadata.len() < 64 || metadata.len() > 256 << 20 {
        return Ok(false);
    }
    let mut header = [0; 64];
    file.read_exact(&mut header)?;
    if KernelArch::from_elf(&header).ok() != Some(KernelArch::Aarch64) {
        return Ok(false);
    }
    match verify_translator(path, FEX_SHA256) {
        Ok(()) => Ok(true),
        Err(error) if error.code == "unsupported_translator" => Ok(false),
        Err(error) => Err(error),
    }
}
pub(super) fn translator_bytes(path: &Path, expected_sha256: &str) -> Result<Vec<u8>> {
    if expected_sha256 != FEX_SHA256 {
        return Err(Error::new(
            "unsupported_translator",
            "Translator is not the reviewed FEX binary",
        ));
    }
    translator_expected(path, TranslatorBuild::Supported)
}
fn translator_expected(path: &Path, expected: TranslatorBuild) -> Result<Vec<u8>> {
    let hash = match &expected {
        TranslatorBuild::Supported => FEX_SHA256,
        #[cfg(test)]
        TranslatorBuild::Generated(hash) => hash.as_str(),
    };
    let file = File::open(path)?;
    let metadata = file.metadata()?;
    if !metadata.is_file() || metadata.len() > 256 << 20 {
        return Err(Error::new(
            "unsupported_translator",
            "Translator file exceeds bound",
        ));
    }
    let mut bytes = Vec::new();
    file.take((256 << 20) + 1).read_to_end(&mut bytes)?;
    let valid = bytes.len() <= 256 << 20
        && KernelArch::from_elf(&bytes).ok() == Some(KernelArch::Aarch64)
        && matches!(u16::from_le_bytes(field(&bytes, 16)?), 2 | 3)
        && u32::from_le_bytes(field(&bytes, 20)?) == 1
        && u16::from_le_bytes(field(&bytes, 52)?) == 64
        && format!("{:x}", Sha256::digest(&bytes)) == hash;
    if !valid {
        return Err(Error::new(
            "unsupported_translator",
            "Actual FEX ELF64 AArch64/hash differs",
        ));
    }
    Ok(bytes)
}
#[derive(Clone, Debug, PartialEq, Eq)]
struct FileIdentity {
    device: u64,
    inode: u64,
    size: u64,
    modified: (i64, i64),
    changed: (i64, i64),
}
impl FileIdentity {
    fn of(metadata: &fs::Metadata) -> Self {
        Self {
            device: metadata.dev(),
            inode: metadata.ino(),
            size: metadata.len(),
            modified: (metadata.mtime(), metadata.mtime_nsec()),
            changed: (metadata.ctime(), metadata.ctime_nsec()),
        }
    }
}
fn canonical(path: &Path) -> Result<PathBuf> {
    let resolved = fs::canonicalize(path)?;
    if !path.is_absolute() || resolved != path || resolved.to_str().is_none() {
        return Err(Error::new(
            "invalid_translated_image",
            "Exact canonical absolute guest executable path required",
        ));
    }
    Ok(resolved)
}
fn selected_maps(regions: &[Region], path: &Path, file: &FileIdentity) -> Result<Vec<Region>> {
    let device = (
        u64::from(libc::major(file.device)),
        u64::from(libc::minor(file.device)),
    );
    let path = path
        .to_str()
        .ok_or_else(|| Error::new("invalid_translated_image", "Guest path must be UTF-8"))?;
    let selected = regions
        .iter()
        .filter(|r| {
            r.path.as_deref() == Some(path) || (r.device == device && r.inode == file.inode)
        })
        .cloned()
        .collect::<Vec<_>>();
    if selected.len() > 256 {
        return Err(Error::new(
            "invalid_maps",
            "Guest image mapping count exceeds bound",
        ));
    }
    for r in &selected {
        if r.path.as_deref() != Some(path)
            || r.device != device
            || r.inode != file.inode
            || r.start < 0x10000
            || r.end > 1u64 << 47
            || r.permissions[3] != b'p'
            || (r.permissions[1] == b'w' && r.permissions[2] == b'x')
            || r.offset.checked_add(r.end - r.start).is_none()
        {
            return Err(Error::new(
                "ownership_changed",
                "Exact mapped guest file identity changed",
            ));
        }
    }
    let headers = selected
        .iter()
        .filter(|r| r.offset == 0)
        .collect::<Vec<_>>();
    if headers.len() > 1
        || headers.first().is_some_and(|header| {
            !header.readable || header.permissions[1] == b'w' || header.end - header.start < 64
        })
    {
        return Err(Error::new(
            "invalid_maps",
            "Ambiguous or mutable guest ELF header mapping",
        ));
    }
    Ok(selected)
}
/// Maps-only duplicate/readiness hint: never reads another process's memory.
/// Full guest ELF/header and current ownership proof belongs to LinuxProcess.
pub(crate) fn guest_mapping_present(pid: i32, path: &Path) -> Result<bool> {
    if pid <= 0 {
        return Err(Error::new(
            "invalid_identity",
            "Positive process identity required",
        ));
    }
    let path = canonical(path)?;
    let metadata = fs::metadata(&path)?;
    if !metadata.is_file() {
        return Err(Error::new(
            "invalid_translated_image",
            "Guest executable is not a file",
        ));
    }
    let Some(before) = live_stat(pid)? else {
        return Ok(false);
    };
    let regions = match mappings(pid) {
        Ok(regions) => regions,
        Err(error) => {
            if live_stat(pid)?.is_none() {
                return Ok(false);
            }
            return Err(error);
        }
    };
    let Some(after) = live_stat(pid)? else {
        return Ok(false);
    };
    if before.2 != after.2 {
        return Err(Error::new(
            "ownership_changed",
            "Process creation changed during guest mapping probe",
        ));
    }
    let device = (
        u64::from(libc::major(metadata.dev())),
        u64::from(libc::minor(metadata.dev())),
    );
    // Even an alias or a partial/ambiguous loader map is a duplicate. This is
    // not the full reader's build/header/readiness or ownership proof.
    Ok(regions
        .iter()
        .any(|r| r.device == device && r.inode == metadata.ino()))
}
fn live_stat(pid: i32) -> Result<Option<(char, i32, String)>> {
    let text = match fs::read_to_string(format!("/proc/{pid}/stat")) {
        Ok(text) => text,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error.into()),
    };
    let stat = parse_stat(&text)?;
    Ok((!matches!(stat.0, 'Z' | 'X')).then_some(stat))
}
pub(super) struct GuestImage {
    path: PathBuf,
    file: File,
    identity: FileIdentity,
    regions: Vec<Region>,
    base: u64,
    header: Vec<u8>,
    program_offset: u64,
    program_headers: Vec<u8>,
}
impl GuestImage {
    pub(super) fn base(&self) -> u64 {
        self.base
    }
    pub(super) fn capture(
        pid: i32,
        image: &TranslatedImage,
        expected: &ExpectedBuild,
        regions: &[Region],
    ) -> Result<Self> {
        let path = canonical(Path::new(&image.executable))?;
        let file = File::open(&path)?;
        let identity = FileIdentity::of(&file.metadata()?);
        let bytes = executable_bytes(&path, expected)?;
        if FileIdentity::of(&file.metadata()?) != identity
            || FileIdentity::of(&fs::metadata(&path)?) != identity
        {
            return Err(Error::new(
                "ownership_changed",
                "Guest file changed during initial executable verification",
            ));
        }
        let image = Self::from_verified(path, file, identity, &bytes, regions)?;
        image.check(pid, regions)?;
        Ok(image)
    }
    fn from_verified(
        path: PathBuf,
        file: File,
        identity: FileIdentity,
        bytes: &[u8],
        regions: &[Region],
    ) -> Result<Self> {
        if KernelArch::from_elf(bytes)? != KernelArch::X86_64
            || u16::from_le_bytes(field(bytes, 16)?) != 3
        {
            return Err(Error::new(
                "unsupported_build",
                "Translated guest must be the reviewed x86_64 PIE image",
            ));
        }
        let offset = u64::from_le_bytes(field(bytes, 32)?);
        let count = usize::from(u16::from_le_bytes(field(bytes, 56)?));
        let stride = usize::from(u16::from_le_bytes(field(bytes, 54)?));
        if count == 0 || count > 4096 || stride != 56 {
            return Err(Error::new(
                "invalid_elf",
                "Guest program-header bounds differ",
            ));
        }
        let begin = usize::try_from(offset)
            .map_err(|_| Error::new("invalid_elf", "Guest program-header offset"))?;
        let end = begin
            .checked_add(count * stride)
            .ok_or_else(|| Error::new("invalid_elf", "Guest program-header overflow"))?;
        let headers = bytes
            .get(begin..end)
            .ok_or_else(|| Error::new("invalid_elf", "Guest program-header file bounds"))?
            .to_vec();
        let selected = selected_maps(regions, &path, &identity)?;
        let base = selected
            .iter()
            .find(|r| r.offset == 0)
            .ok_or_else(|| loading("Initial translated guest ELF header not mapped yet"))?
            .start;
        validate_loads(bytes, &headers, &selected, regions, base)?;
        if !selected
            .iter()
            .any(|r| r.permissions[2] == b'x' && r.permissions[1] != b'w')
        {
            return Err(Error::new(
                "invalid_maps",
                "Translated guest executable segment permissions differ",
            ));
        }
        let image = Self {
            path,
            file,
            identity,
            regions: selected,
            base,
            header: bytes
                .get(..64)
                .ok_or_else(|| Error::new("invalid_elf", "Guest ELF header truncated"))?
                .to_vec(),
            program_offset: offset,
            program_headers: headers,
        };
        image.validate_spans()?;
        Ok(image)
    }
    fn validate_spans(&self) -> Result<()> {
        for (offset, size) in [
            (0, self.header.len()),
            (self.program_offset, self.program_headers.len()),
        ] {
            let start = self
                .base
                .checked_add(offset)
                .ok_or_else(|| Error::new("invalid_maps", "Guest span overflow"))?;
            let end = start
                .checked_add(size as u64)
                .ok_or_else(|| Error::new("invalid_maps", "Guest span overflow"))?;
            if !self.regions.iter().any(|r| {
                r.readable
                    && r.permissions[1] != b'w'
                    && r.start <= start
                    && end <= r.end
                    && r.offset + (start - r.start) == offset
            }) {
                return Err(Error::new(
                    "invalid_maps",
                    "Guest ELF/program headers outside immutable file mapping",
                ));
            }
        }
        Ok(())
    }
    fn check_with(
        &self,
        regions: &[Region],
        mut read: impl FnMut(u64, usize) -> Result<Vec<u8>>,
    ) -> Result<()> {
        let current = selected_maps(regions, &self.path, &self.identity)?;
        if coverage(&current)? != coverage(&self.regions)? {
            return Err(Error::new(
                "ownership_changed",
                "Translated guest mapping witness changed; never rebind",
            ));
        }
        // Dynamic loader RELRO may split VMAs and remove write permission.
        // Preserve file coverage and the original permission ceiling; never
        // gain a permission or permit a writable executable image mapping.
        for region in &current {
            let mut cursor = region.start;
            while cursor < region.end {
                let original = self
                    .regions
                    .iter()
                    .find(|r| r.start <= cursor && cursor < r.end)
                    .ok_or_else(|| {
                        Error::new("ownership_changed", "Guest file mapping coverage changed")
                    })?;
                if (0..3).any(|i| {
                    region.permissions[i] != b'-'
                        && region.permissions[i] != original.permissions[i]
                }) {
                    return Err(Error::new(
                        "ownership_changed",
                        "Guest image mapping permission exceeds original ceiling",
                    ));
                }
                cursor = region.end.min(original.end);
            }
        }
        if read(self.base, self.header.len())? != self.header
            || read(self.base + self.program_offset, self.program_headers.len())?
                != self.program_headers
        {
            return Err(Error::new(
                "ownership_changed",
                "Mapped translated guest ELF/program-header witness changed",
            ));
        }
        Ok(())
    }
    pub(super) fn check(&self, pid: i32, regions: &[Region]) -> Result<()> {
        self.check_file()?;
        self.check_with(regions, |address, size| {
            let mut data = vec![0; size];
            read_exact_remote(pid, address, &mut data)?;
            Ok(data)
        })?;
        self.check_file()
    }
    fn check_file(&self) -> Result<()> {
        if canonical(&self.path)? != self.path
            || FileIdentity::of(&self.file.metadata()?) != self.identity
            || FileIdentity::of(&fs::metadata(&self.path)?) != self.identity
        {
            return Err(Error::new(
                "ownership_changed",
                "Original translated guest file changed; never rebind",
            ));
        }
        Ok(())
    }
}

fn coverage(regions: &[Region]) -> Result<Vec<(u64, u64, u64)>> {
    let mut result: Vec<(u64, u64, u64)> = Vec::new();
    for r in regions {
        if let Some(previous) = result.last_mut() {
            if previous.1 > r.start {
                return Err(Error::new(
                    "invalid_maps",
                    "Overlapping translated guest image mappings",
                ));
            }
            if previous.1 == r.start && previous.2 + (previous.1 - previous.0) == r.offset {
                previous.1 = r.end;
                continue;
            }
        }
        result.push((r.start, r.end, r.offset));
    }
    Ok(result)
}

fn loading(reason: &str) -> Error {
    Error::new("translated_image_loading", reason)
}
fn validate_loads(
    bytes: &[u8],
    headers: &[u8],
    regions: &[Region],
    all_regions: &[Region],
    base: u64,
) -> Result<()> {
    let mut zero_load = false;
    for h in headers.chunks_exact(56) {
        if u32::from_le_bytes(field(h, 0)?) != 1 {
            continue;
        }
        let offset = u64::from_le_bytes(field(h, 8)?);
        let address = u64::from_le_bytes(field(h, 16)?);
        let size = u64::from_le_bytes(field(h, 32)?);
        let memory = u64::from_le_bytes(field(h, 40)?);
        if size > memory
            || offset
                .checked_add(size)
                .is_none_or(|end| end > bytes.len() as u64)
            || address % 4096 != offset % 4096
        {
            return Err(Error::new(
                "invalid_elf",
                "Guest load segment bounds differ",
            ));
        }
        zero_load |= offset == 0 && address == 0 && size >= 64;
        let start = base
            .checked_add(address)
            .ok_or_else(|| Error::new("invalid_maps", "Guest segment overflow"))?;
        let end = start
            .checked_add(size)
            .filter(|end| *end <= 1u64 << 47)
            .ok_or_else(|| Error::new("invalid_maps", "Guest segment outside x86 address space"))?;
        let mut cursor = start;
        while cursor < end {
            let region = regions
                .iter()
                .find(|r| r.start <= cursor && cursor < r.end)
                .ok_or_else(|| {
                    let missing_end = regions
                        .iter()
                        .filter(|r| {
                            r.start > cursor
                                && r.start < end
                                && r.offset == offset + (r.start - start)
                        })
                        .map(|r| r.start)
                        .min()
                        .unwrap_or(end);
                    // Inspect the entire missing gap, not just its first
                    // address: a later foreign VMA is never loader readiness.
                    if all_regions.iter().any(|r| {
                        r.start < missing_end
                            && cursor < r.end
                            && (r.inode != 0 || r.permissions != *b"---p")
                    }) {
                        Error::new(
                            "invalid_maps",
                            "Required guest load address occupied by a different mapping",
                        )
                    } else {
                        loading("Initial translated guest required PT_LOAD coverage not mapped yet")
                    }
                })?;
            if region.offset + (cursor - region.start) != offset + (cursor - start) {
                return Err(Error::new(
                    "invalid_maps",
                    "Guest load segment mapped at different file offset",
                ));
            }
            cursor = end.min(region.end);
        }
    }
    if !zero_load {
        return Err(Error::new(
            "invalid_elf",
            "Guest image header requires zero-based PIE load",
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests;
