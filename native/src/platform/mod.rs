use crate::{
    error::{Error, Result},
    memory::ReadMemory,
};
use serde::{Deserialize, Serialize};
use std::path::Path;
// Portable ordering/cleanup tests exercise the same strict key sequence used
// by macOS. Other native input adapters keep their existing delivery contracts.
#[cfg(any(target_os = "macos", test))]
pub(crate) mod key_once;
#[cfg(any(target_os = "macos", test))]
pub(crate) mod mouse_occlusion;
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
// Share one opened native handle between the stdio reader and its owned worker.
// Delegation preserves batching, ABI and freshness checks; Arc grants no access.
impl<T: Process + ?Sized> ReadMemory for std::sync::Arc<T> {
    fn read(&self, address: u64, size: usize) -> Result<Vec<u8>> {
        (**self).read(address, size)
    }
    fn read_many(&self, requests: &[(u64, usize)]) -> Result<Vec<Vec<u8>>> {
        (**self).read_many(requests)
    }
    fn string_abi(&self) -> crate::memory::StringAbi {
        (**self).string_abi()
    }
}
impl<T: Process + ?Sized> Process for std::sync::Arc<T> {
    fn check(&self) -> Result<()> {
        (**self).check()
    }
    fn begin_snapshot(&self) -> Result<()> {
        (**self).begin_snapshot()
    }
    fn image_base(&self) -> u64 {
        (**self).image_base()
    }
    fn identity(&self) -> Option<&Identity> {
        (**self).identity()
    }
}

#[cfg(test)]
mod shared_process_tests {
    use super::*;
    use crate::memory::StringAbi;
    use std::sync::{mpsc, Arc, Mutex};
    struct Fixture {
        trace: Arc<Mutex<Vec<&'static str>>>,
        identity: Identity,
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            self.trace.lock().unwrap().push("drop");
        }
    }
    impl ReadMemory for Fixture {
        fn read(&self, _: u64, size: usize) -> Result<Vec<u8>> {
            self.trace.lock().unwrap().push("read");
            Ok(vec![1; size])
        }
        fn read_many(&self, requests: &[(u64, usize)]) -> Result<Vec<Vec<u8>>> {
            self.trace.lock().unwrap().push("batch");
            Ok(requests.iter().map(|(_, n)| vec![2; *n]).collect())
        }
        fn string_abi(&self) -> StringAbi {
            StringAbi::LibCxx
        }
    }
    impl Process for Fixture {
        fn check(&self) -> Result<()> {
            self.trace.lock().unwrap().push("check");
            Ok(())
        }
        fn begin_snapshot(&self) -> Result<()> {
            self.trace.lock().unwrap().push("snapshot");
            Ok(())
        }
        fn image_base(&self) -> u64 {
            0x100000000
        }
        fn identity(&self) -> Option<&Identity> {
            Some(&self.identity)
        }
    }
    #[test]
    fn shared_reader_preserves_native_batch_abi_checks_and_last_owner_lifetime() {
        let trace = Arc::new(Mutex::new(Vec::new()));
        let identity = Identity {
            pid: 7,
            start_ticks: "1.000001".into(),
            exe: "/fixture".into(),
            uid: 501,
        };
        let process: Arc<dyn Process + Send + Sync> = Arc::new(Fixture {
            trace: trace.clone(),
            identity: identity.clone(),
        });
        let reader: &dyn Process = &process;
        assert_eq!(reader.identity(), Some(&identity));
        assert_eq!(reader.image_base(), 0x100000000);
        assert_eq!(reader.string_abi(), StringAbi::LibCxx);
        assert_eq!(
            reader.read_many(&[(0x10000, 1), (0x20000, 2)]).unwrap(),
            vec![vec![2], vec![2, 2]]
        );
        assert_eq!(reader.read(0x10000, 1).unwrap(), vec![1]);
        reader.begin_snapshot().unwrap();
        reader.check().unwrap();
        let owned = process.clone();
        let (send, recv) = mpsc::channel();
        let worker = std::thread::spawn(move || {
            recv.recv().unwrap();
            owned.check().unwrap();
        });
        drop(process);
        assert_eq!(
            *trace.lock().unwrap(),
            vec!["batch", "read", "snapshot", "check"]
        );
        send.send(()).unwrap();
        worker.join().unwrap();
        assert_eq!(
            *trace.lock().unwrap(),
            vec!["batch", "read", "snapshot", "check", "check", "drop"]
        );
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
pub(crate) use linux::{guest_mapping_present, is_reviewed_translator, FEX_SHA256};
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
