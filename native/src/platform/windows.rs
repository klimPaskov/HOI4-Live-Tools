//! Dormant Windows ownership/read foundation. Intentionally implements neither
//! Process nor ReadMemory: there is no reviewed Windows game ABI/layout here.
//! All policies below are observer bounds, not engine or Windows maxima.
use crate::error::{Error, Result};
use std::sync::atomic::{AtomicBool, Ordering};
mod pe;
pub(crate) use pe::PeIdentity;
#[cfg(target_os = "windows")]
pub(crate) mod native;
#[cfg(test)]
mod tests;

pub(crate) const QUERY: u32 = 0x0400 | 0x0010_0000;
pub(crate) const QUERY_READ: u32 = QUERY | 0x0010;
const USER_END: u64 = 0x7fff_ffff_0000;
const MAX_READ: usize = 1 << 20;
pub(crate) const MAX_IMAGE: u64 = 512 << 20;
pub(crate) const HEADER_BOUND: usize = 65536;
fn error(code: &'static str, text: &str) -> Error {
    Error::new(code, text)
}
fn changed() -> Error {
    error(
        "ownership_changed",
        "Original Windows process/image ownership changed",
    )
}

/// Complete binary SID; no RID/hash is substituted for a Unix uid.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Sid(Vec<u8>);
impl Sid {
    pub(crate) fn new(bytes: Vec<u8>) -> Result<Self> {
        if bytes.len() < 8
            || bytes[0] != 1
            || bytes[1] > 15
            || bytes.len() != 8 + usize::from(bytes[1]) * 4
        {
            return Err(error(
                "invalid_session",
                "Exact bounded Windows SID required",
            ));
        }
        Ok(Self(bytes))
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct FileIdentity {
    pub volume: u64,
    pub id: [u8; 16],
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Identity {
    pub pid: u32,
    /// Unrounded Windows 100ns creation FILETIME, kept as a u64.
    pub creation_filetime: u64,
    pub executable: String,
    pub owner: Sid,
    pub file: FileIdentity,
}
fn valid_path(path: &str) -> bool {
    let b = path.as_bytes();
    b.len() >= 4
        && b[0].is_ascii_alphabetic()
        && b[1] == b':'
        && b[2] == b'\\'
        && path.encode_utf16().count() <= 32767
        && !path.contains('\0')
        && !path[2..].contains(':')
        && !path.contains('/')
        && path[3..]
            .split('\\')
            .all(|v| !v.is_empty() && v != "." && v != "..")
}
impl Identity {
    fn validate(&self) -> Result<()> {
        if self.pid == 0
            || self.creation_filetime == 0
            || !valid_path(&self.executable)
            || self.file.id == [0; 16]
        {
            return Err(error(
                "invalid_session",
                "Exact local Windows process/file identity required",
            ));
        }
        Sid::new(self.owner.0.clone())?;
        Ok(())
    }
}
#[derive(Clone, Debug)]
pub(crate) struct Session {
    pub game: Identity,
    pub supervisor: Identity,
}
impl Session {
    fn validate(&self) -> Result<()> {
        self.game.validate()?;
        self.supervisor.validate()?;
        if self.game.pid == self.supervisor.pid
            || self.game.owner != self.supervisor.owner
            || self.game.creation_filetime < self.supervisor.creation_filetime
        {
            return Err(error(
                "invalid_session",
                "Distinct original owned Windows supervisor required",
            ));
        }
        Ok(())
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Metadata {
    pub identity: Identity,
    pub parent_pid: u32,
    pub image_base: u64,
    pub image_size: u32,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Image {
    pub file: FileIdentity,
    pub size: u64,
    pub header: Vec<u8>,
    pub sha256: [u8; 32],
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Region {
    pub start: u64,
    pub allocation: u64,
    pub size: u64,
    pub state: u32,
    pub protection: u32,
    pub kind: u32,
}
impl Region {
    fn covers(self, address: u64, end: u64) -> bool {
        let readable = matches!(
            self.protection & 0xff,
            0x02 | 0x04 | 0x08 | 0x20 | 0x40 | 0x80
        );
        self.state == 0x1000
            && self.protection & !0x6ff == 0
            && self.protection & 0x100 == 0
            && readable
            && self.start <= address
            && self.start >= 0x10000
            && self.allocation >= 0x10000
            && self.allocation <= self.start
            && matches!(self.kind, 0x20000 | 0x40000 | 0x1000000)
            && self.size > 0
            && self
                .start
                .checked_add(self.size)
                .is_some_and(|v| v >= end && v <= USER_END)
    }
}
/// Implementations own their handles (including snapshots/tokens/files) with
/// RAII. No interface for write, injection, privilege adjustment or control.
pub(crate) trait Api {
    type Process;
    type Image;
    fn current_owner(&self) -> Result<(Sid, bool)>;
    fn open_process(&self, pid: u32, rights: u32) -> Result<Self::Process>;
    fn inspect(&self, process: &Self::Process) -> Result<Metadata>;
    /// Cheap original-handle liveness/PID/creation check; no census/file reopen.
    fn check_handle(&self, process: &Self::Process, expected: &Identity) -> Result<()>;
    fn open_image(&self, path: &str) -> Result<Self::Image>;
    fn image(&self, image: &Self::Image) -> Result<Image>;
    fn file_identity(&self, image: &Self::Image) -> Result<FileIdentity>;
    fn region(&self, process: &Self::Process, address: u64) -> Result<Region>;
    fn read(&self, process: &Self::Process, address: u64, size: usize) -> Result<Vec<u8>>;
}
#[cfg(test)]
#[derive(Clone, Debug)]
struct Profile {
    pe: PeIdentity,
    sha256: [u8; 32],
}
enum ExpectedBuild {
    Production,
    // Cannot be selected by any production argument, environment or manifest.
    #[cfg(test)]
    Generated(Profile),
}
fn validate_profile(
    _image: &Image,
    _pe: &PeIdentity,
    _size: u32,
    expected: ExpectedBuild,
) -> Result<()> {
    match expected {
        ExpectedBuild::Production => Err(error(
            "unsupported_build",
            "No Windows game profile/ABI/layout is reviewed",
        )),
        #[cfg(test)]
        ExpectedBuild::Generated(profile) => {
            if _image.sha256 != profile.sha256 || *_pe != profile.pe || _size != _pe.image_size {
                return Err(error(
                    "unsupported_build",
                    "Windows executable does not match exact generated PE/SHA identity",
                ));
            }
            Ok(())
        }
    }
}
/// Immutable owned handle and file witnesses; never game/input authority.
pub(crate) struct OwnedReader<A: Api> {
    api: A,
    session: Session,
    game: A::Process,
    supervisor: A::Process,
    game_image: A::Image,
    supervisor_image: A::Image,
    original: Metadata,
    poisoned: AtomicBool,
}
impl<A: Api> OwnedReader<A> {
    pub(crate) fn open(api: A, session: Session) -> Result<Self> {
        Self::open_with(api, session, ExpectedBuild::Production)
    }
    #[cfg(test)]
    fn generated(api: A, session: Session, profile: Profile) -> Result<Self> {
        Self::open_with(api, session, ExpectedBuild::Generated(profile))
    }
    fn open_with(api: A, session: Session, expected: ExpectedBuild) -> Result<Self> {
        session.validate()?;
        let (owner, elevated) = api.current_owner()?;
        if elevated || owner != session.game.owner {
            return Err(changed());
        }
        let supervisor = api.open_process(session.supervisor.pid, QUERY)?;
        let queried = api.open_process(session.game.pid, QUERY)?;
        let sm = api.inspect(&supervisor)?;
        let gm = api.inspect(&queried)?;
        if sm.identity != session.supervisor
            || gm.identity != session.game
            || gm.parent_pid != session.supervisor.pid
        {
            return Err(changed());
        }
        let game_image = api.open_image(&session.game.executable)?;
        let supervisor_image = api.open_image(&session.supervisor.executable)?;
        if api.file_identity(&game_image)? != session.game.file
            || api.file_identity(&supervisor_image)? != session.supervisor.file
        {
            return Err(changed());
        }
        let image = api.image(&game_image)?;
        if image.file != session.game.file || image.size == 0 || image.size > MAX_IMAGE {
            return Err(changed());
        }
        let pe = PeIdentity::parse(&image.header)?;
        validate_profile(&image, &pe, gm.image_size, expected)?;
        range(gm.image_base, 1)?;
        if gm.image_base % 65536 != 0
            || gm
                .image_base
                .checked_add(u64::from(gm.image_size))
                .is_none_or(|v| v > USER_END)
        {
            return Err(error(
                "invalid_maps",
                "Invalid original Windows image mapping",
            ));
        }
        let game = api.open_process(session.game.pid, QUERY_READ)?;
        drop(queried);
        let reader = Self {
            api,
            session,
            game,
            supervisor,
            game_image,
            supervisor_image,
            original: gm,
            poisoned: AtomicBool::new(false),
        };
        reader.check()?;
        // Only a cfg(test) approved profile can currently reach any VM read.
        let loaded = reader.read(reader.original.image_base, pe.header_size as usize)?;
        if loaded
            != image
                .header
                .get(..pe.header_size as usize)
                .ok_or_else(|| error("invalid_pe", "Truncated on-disk headers"))?
        {
            return Err(error(
                "unsupported_build",
                "Loaded executable headers differ from pinned image",
            ));
        }
        reader.check()?;
        Ok(reader)
    }
    fn keep<T>(&self, result: Result<T>) -> Result<T> {
        if result.is_err() {
            self.poisoned.store(true, Ordering::Release);
        }
        result
    }
    pub(crate) fn check(&self) -> Result<()> {
        if self.poisoned.load(Ordering::Acquire) {
            return Err(error(
                "ownership_changed",
                "Windows reader lifetime is quarantined",
            ));
        }
        self.keep((|| {
            let (owner, elevated) = self.api.current_owner()?;
            if elevated || owner != self.session.game.owner {
                return Err(changed());
            }
            let s = self.api.inspect(&self.supervisor)?;
            let g = self.api.inspect(&self.game)?;
            if s.identity != self.session.supervisor
                || g != self.original
                || g.identity != self.session.game
                || g.parent_pid != self.session.supervisor.pid
                || self.api.file_identity(&self.game_image)? != self.session.game.file
                || self.api.file_identity(&self.supervisor_image)? != self.session.supervisor.file
            {
                return Err(changed());
            }
            Ok(())
        })())
    }
    pub(crate) fn identity(&self) -> &Identity {
        &self.session.game
    }
    pub(crate) fn image_base(&self) -> u64 {
        self.original.image_base
    }
    pub(crate) fn read(&self, address: u64, size: usize) -> Result<Vec<u8>> {
        self.read_many(&[(address, size)])?
            .pop()
            .ok_or_else(|| error("short_read", "One-item batch did not return one value"))
    }
    pub(crate) fn read_many(&self, requests: &[(u64, usize)]) -> Result<Vec<Vec<u8>>> {
        self.read_many_checked(requests, || Ok(()))
    }
    /// One full ownership/mapping bracket per batch. A caller may supply its
    /// original cancellation/deadline/lease check; this module never renews it.
    pub(crate) fn read_many_checked(
        &self,
        requests: &[(u64, usize)],
        mut boundary: impl FnMut() -> Result<()>,
    ) -> Result<Vec<Vec<u8>>> {
        crate::memory::validate_read_batch(requests)?;
        for &(address, size) in requests {
            range(address, size)?;
        }
        let result = (|| {
            boundary()?;
            self.check()?;
            let mut checkpoint = || -> Result<()> {
                boundary()?;
                self.api.check_handle(&self.game, &self.session.game)?;
                self.api
                    .check_handle(&self.supervisor, &self.session.supervisor)?;
                boundary()
            };
            let observed = (|| {
                let mut values = Vec::with_capacity(requests.len());
                for &(address, size) in requests {
                    checkpoint()?;
                    let end = range(address, size)?;
                    let region = self.api.region(&self.game, address)?;
                    if !region.covers(address, end) {
                        return Err(error(
                            "unreadable_memory",
                            "Uncommitted, guarded or cross-region Windows read",
                        ));
                    }
                    checkpoint()?;
                    let transfer = self.api.read(&self.game, address, size);
                    // Even a failed transfer gets the cheap post-read boundary.
                    let post = checkpoint();
                    let bytes = transfer?;
                    post?;
                    if bytes.len() != size {
                        return Err(error("short_read", "Incomplete Windows memory read"));
                    }
                    let after = self.api.region(&self.game, address)?;
                    if after != region || !after.covers(address, end) {
                        return Err(error("torn_read", "Windows memory region changed"));
                    }
                    checkpoint()?;
                    values.push(bytes);
                }
                Ok(values)
            })();
            // Always finish the full original bracket, even on a short read or
            // cancellation. Nothing from an incomplete batch is returned.
            let final_owner = self.check();
            let final_boundary = checkpoint();
            match observed {
                Err(cause) => Err(cause),
                Ok(values) => {
                    final_owner?;
                    final_boundary?;
                    Ok(values)
                }
            }
        })();
        self.keep(result)
    }
}
fn range(address: u64, size: usize) -> Result<u64> {
    if size == 0 || size > MAX_READ || address < 0x10000 {
        return Err(error("read_bound", "Invalid bounded Windows read"));
    }
    address
        .checked_add(size as u64)
        .filter(|end| *end <= USER_END)
        .ok_or_else(|| error("read_bound", "Windows read address overflow"))
}
