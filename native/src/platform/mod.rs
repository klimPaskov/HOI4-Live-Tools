use crate::{
    error::{Error, Result},
    memory::ReadMemory,
};
use serde::{Deserialize, Serialize};
use std::path::Path;
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct Identity {
    pub pid: i32,
    pub start_ticks: String,
    pub exe: String,
    pub uid: u32,
}
#[derive(Clone, Debug, Deserialize)]
pub struct Session {
    pub game: Identity,
    pub supervisor: Identity,
    pub build_id: String,
    pub sha256: String,
}
pub trait Process: ReadMemory {
    fn check(&self) -> Result<()>;
    fn image_base(&self) -> u64;
    fn identity(&self) -> Option<&Identity> {
        None
    }
}
#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "linux")]
pub mod x11;
pub fn open(path: &Path) -> Result<Box<dyn Process>> {
    #[cfg(target_os = "linux")]
    {
        Ok(Box::new(linux::LinuxProcess::open(path)?))
    }
    #[cfg(not(target_os = "linux"))]
    {
        let _ = path;
        Err(Error::new(
            "platform_unavailable",
            "Native Windows/macOS build adapters remain unverified",
        ))
    }
}
#[cfg(target_os = "linux")]
pub use linux::{elf_build_id, parse_stat};
pub fn load_session(path: &Path) -> Result<Session> {
    let bytes = std::fs::read(path)?;
    if bytes.len() > 65536 {
        return Err(Error::new(
            "invalid_session",
            "Session manifest exceeds bound",
        ));
    }
    serde_json::from_slice(&bytes).map_err(|e| Error::new("invalid_session", e.to_string()))
}
