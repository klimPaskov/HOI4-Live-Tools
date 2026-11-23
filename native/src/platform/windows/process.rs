//! `platform::Process` for a Windows game this process started: the owned reader with the
//! MSVC string layout. Production builds still refuse every executable (no reviewed Windows
//! profile); a `dev-table` build reads the one executable named in
//! `HOI4_LIVE_DEV_WINDOWS_SHA256`, for live layout mapping.
use super::{native::Native, Api, Identity, OwnedReader, Session, QUERY};
use crate::{
    error::{Error, Result},
    memory::{ReadMemory, StringAbi},
};

pub(crate) struct WindowsProcess {
    reader: OwnedReader<Native>,
}

/// The session for a game started by this process: this process is the supervisor, the
/// game must be its direct child, owned by the same unelevated user.
pub(crate) fn owned_session(game_pid: u32) -> Result<Session> {
    let api = Native;
    let me = api.inspect(&api.open_process(std::process::id(), QUERY)?)?;
    let game = api.inspect(&api.open_process(game_pid, QUERY)?)?;
    if game.parent_pid != me.identity.pid {
        return Err(Error::new(
            "ownership_changed",
            "The game is not a direct child of this supervisor",
        ));
    }
    Ok(Session {
        game: game.identity,
        supervisor: me.identity,
    })
}

#[cfg(feature = "dev-table")]
fn development_sha256() -> Result<Option<[u8; 32]>> {
    let Some(text) = std::env::var_os("HOI4_LIVE_DEV_WINDOWS_SHA256") else {
        return Ok(None);
    };
    let text = text.to_string_lossy().trim().to_ascii_lowercase();
    let mut out = [0u8; 32];
    if text.len() != 64 {
        return Err(Error::new(
            "invalid_session",
            "HOI4_LIVE_DEV_WINDOWS_SHA256 must be 64 hex digits",
        ));
    }
    for (i, byte) in out.iter_mut().enumerate() {
        *byte = u8::from_str_radix(&text[2 * i..2 * i + 2], 16).map_err(|_| {
            Error::new(
                "invalid_session",
                "HOI4_LIVE_DEV_WINDOWS_SHA256 must be 64 hex digits",
            )
        })?;
    }
    Ok(Some(out))
}

impl WindowsProcess {
    pub(crate) fn open(session: Session) -> Result<Self> {
        #[cfg(feature = "dev-table")]
        if let Some(sha256) = development_sha256()? {
            return Ok(Self {
                reader: OwnedReader::development(Native, session, sha256)?,
            });
        }
        Ok(Self {
            reader: OwnedReader::open(Native, session)?,
        })
    }
    pub(crate) fn game(&self) -> &Identity {
        self.reader.identity()
    }
}

impl ReadMemory for WindowsProcess {
    fn read(&self, address: u64, size: usize) -> Result<Vec<u8>> {
        self.reader.read(address, size)
    }
    fn read_many(&self, requests: &[(u64, usize)]) -> Result<Vec<Vec<u8>>> {
        self.reader.read_many(requests)
    }
    fn string_abi(&self) -> StringAbi {
        StringAbi::Msvc
    }
}

impl crate::platform::Process for WindowsProcess {
    fn check(&self) -> Result<()> {
        self.reader.check()
    }
    fn image_base(&self) -> u64 {
        self.reader.image_base()
    }
    // No `platform::Identity`: its Unix start ticks and uid have no Windows meaning (the
    // reader holds the creation FILETIME and the full SID). Bindings that need it refuse.
}
