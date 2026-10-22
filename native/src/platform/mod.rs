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
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct TranslatedImage {
    pub executable: String,
    pub translator_sha256: String,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Session {
    pub game: Identity,
    pub supervisor: Identity,
    pub build_id: String,
    pub sha256: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub translated: Option<TranslatedImage>,
}
pub trait Process: ReadMemory {
    fn check(&self) -> Result<()>;
    fn begin_snapshot(&self) -> Result<()> {
        self.check()
    }
    fn image_base(&self) -> u64;
    fn identity(&self) -> Option<&Identity> {
        None
    }
}
#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "macos")]
pub mod macos;
#[cfg(target_os = "macos")]
pub mod macos_input;
#[cfg(target_os = "linux")]
mod unix_socket;
#[cfg(target_os = "linux")]
pub mod x11;
#[cfg(target_os = "linux")]
pub mod x11_close;
#[cfg(target_os = "linux")]
mod xauthority;
#[cfg(target_os = "linux")]
pub(crate) use unix_socket::connect as connect_socket;
#[cfg(target_os = "linux")]
pub(crate) use unix_socket::connect_until as connect_socket_until;
#[cfg(target_os = "linux")]
pub(crate) use unix_socket::validate_path as validate_socket_path;
pub fn open(path: &Path) -> Result<Box<dyn Process + Send + Sync>> {
    #[cfg(target_os = "linux")]
    {
        Ok(Box::new(linux::LinuxProcess::open(path)?))
    }
    #[cfg(target_os = "macos")]
    {
        Ok(Box::new(macos::MacProcess::open(path)?))
    }
    #[cfg(not(any(target_os = "linux", target_os = "macos")))]
    {
        let _ = path;
        Err(Error::new(
            "platform_unavailable",
            "Native Windows build adapter remains unverified",
        ))
    }
}
#[cfg(target_os = "linux")]
pub use linux::{capture_identity, elf_build_id, parse_stat, verify_executable, verify_translator};
#[cfg(target_os = "linux")]
pub(crate) use linux::{guest_mapping_present, is_reviewed_translator};
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

#[cfg(test)]
mod session_tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn native_session_json_remains_unchanged_and_translation_is_explicit() {
        let native = json!({
            "game":{"pid":2,"start_ticks":"20","exe":"/game","uid":1000},
            "supervisor":{"pid":1,"start_ticks":"10","exe":"/owner","uid":1000},
            "build_id":"known", "sha256":"known"
        });
        let session: Session = serde_json::from_value(native.clone()).unwrap();
        assert!(session.translated.is_none());
        assert_eq!(serde_json::to_value(session).unwrap(), native);
        let mut translated = native;
        translated["translated"] = json!({"executable":"/game", "translator_sha256":"pinned"});
        let session: Session = serde_json::from_value(translated.clone()).unwrap();
        assert_eq!(session.translated.unwrap().executable, "/game");
        translated["translated"]["build_override"] = json!("unreviewed");
        assert!(serde_json::from_value::<Session>(translated).is_err());
    }
}
