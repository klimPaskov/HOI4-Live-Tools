//! Persistent owned launch and reconnectable stdio transport. Linux is the first
//! verified process backend; other platforms fail explicitly before launching.
#[cfg(not(target_os = "linux"))]
use crate::error::Error;
use crate::error::Result;
use std::path::Path;
#[cfg(target_os = "linux")]
mod linux;
pub mod macos;
/// Attach to a game the user started: verify the process and write `session.json` (private, owner
/// only) into the run directory. Returns the session as JSON for the caller to show.
pub fn attach(pid: i32, directory: &Path) -> Result<String> {
    #[cfg(target_os = "macos")]
    {
        let session = crate::platform::macos::attached_session(pid)?;
        write_private_json(&directory.join("session.json"), &session)?;
        Ok(serde_json::json!({
            "attached": true,
            "session": directory.join("session.json"),
            "pid": session.game.pid,
            "exe": session.game.exe,
            "start_ticks": session.game.start_ticks,
        })
        .to_string())
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = (pid, directory);
        Err(crate::error::Error::new(
            "platform_unavailable",
            "Attaching to a game the user started is macOS-only today",
        ))
    }
}
/// Write `value` as JSON readable by this user only: a fresh temporary file in the same private
/// directory, then an atomic rename over the target.
#[cfg(any(target_os = "macos", all(test, unix)))]
fn write_private_json<T: serde::Serialize>(path: &Path, value: &T) -> Result<()> {
    use std::io::Write;
    use std::os::unix::fs::{MetadataExt, OpenOptionsExt};
    let directory = path.parent().ok_or_else(|| {
        crate::error::Error::new("invalid_run_directory", "Run directory required")
    })?;
    let meta = std::fs::symlink_metadata(directory)?;
    // SAFETY: getuid has no arguments and no memory effects.
    let uid = unsafe { libc::getuid() };
    if !meta.is_dir() || meta.uid() != uid || meta.mode() & 0o022 != 0 {
        return Err(crate::error::Error::new(
            "invalid_run_directory",
            "The run directory must be a directory of this user that others cannot write",
        ));
    }
    let temporary = path.with_extension(format!("json.{}.tmp", std::process::id()));
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .custom_flags(libc::O_NOFOLLOW)
        .open(&temporary)?;
    let written = serde_json::to_vec_pretty(value)
        .map_err(|e| crate::error::Error::new("invalid_session", e.to_string()))
        .and_then(|bytes| Ok(file.write_all(&bytes)?))
        .and_then(|()| Ok(file.sync_all()?));
    if let Err(e) = written.and_then(|()| Ok(std::fs::rename(&temporary, path)?)) {
        let _ = std::fs::remove_file(&temporary);
        return Err(e);
    }
    Ok(())
}
#[cfg(all(test, unix))]
mod attach_tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;
    fn folder(name: &str, mode: u32) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("hoi4-attach-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir(&dir).unwrap();
        std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(mode)).unwrap();
        dir
    }
    #[test]
    fn session_file_is_private_and_replaced_atomically() {
        let dir = folder("ok", 0o700);
        let path = dir.join("session.json");
        write_private_json(&path, &serde_json::json!({"n": 1})).unwrap();
        write_private_json(&path, &serde_json::json!({"n": 2})).unwrap();
        let meta = std::fs::metadata(&path).unwrap();
        assert_eq!(meta.permissions().mode() & 0o777, 0o600);
        let back: serde_json::Value =
            serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
        assert_eq!(back["n"], 2);
        assert_eq!(std::fs::read_dir(&dir).unwrap().count(), 1);
        std::fs::remove_dir_all(&dir).unwrap();
    }
    #[test]
    fn a_folder_others_can_write_is_refused() {
        let dir = folder("open", 0o777);
        let error = write_private_json(&dir.join("session.json"), &1).unwrap_err();
        assert_eq!(error.code, "invalid_run_directory");
        std::fs::remove_dir_all(&dir).unwrap();
    }
    #[cfg(not(target_os = "macos"))]
    #[test]
    fn attach_is_refused_off_macos() {
        assert_eq!(
            attach(1, Path::new("/nonexistent")).unwrap_err().code,
            "platform_unavailable"
        );
    }
}
pub fn serve(config: &Path) -> Result<()> {
    #[cfg(target_os = "linux")]
    {
        linux::serve(config)
    }
    #[cfg(not(target_os = "linux"))]
    {
        let _ = config;
        Err(Error::new(
            "platform_unavailable",
            "Owned native launcher is not verified on this platform",
        ))
    }
}
pub fn connect(directory: &Path) -> Result<()> {
    #[cfg(target_os = "linux")]
    {
        linux::connect(directory)
    }
    #[cfg(not(target_os = "linux"))]
    {
        let _ = directory;
        Err(Error::new(
            "platform_unavailable",
            "Native endpoint transport is not verified on this platform",
        ))
    }
}
pub fn close(directory: &Path) -> Result<()> {
    #[cfg(target_os = "linux")]
    {
        linux::close(directory)
    }
    #[cfg(not(target_os = "linux"))]
    {
        let _ = directory;
        Err(Error::new(
            "platform_unavailable",
            "Owned native shutdown is not verified on this platform",
        ))
    }
}
/// An explicit one-attempt owned WM_DELETE_WINDOW request; no signal fallback.
pub fn close_window(directory: &Path, config: &Path) -> Result<()> {
    #[cfg(target_os = "linux")]
    {
        linux::close_window(directory, config)
    }
    #[cfg(not(target_os = "linux"))]
    {
        let _ = (directory, config);
        Err(Error::new(
            "platform_unavailable",
            "Owned window shutdown is not verified on this platform",
        ))
    }
}
pub fn bind_management(directory: &Path, config: &Path) -> Result<()> {
    #[cfg(target_os = "linux")]
    {
        linux::bind_management(directory, config)
    }
    #[cfg(not(target_os = "linux"))]
    {
        let _ = (directory, config);
        Err(Error::new(
            "platform_unavailable",
            "The management binding is Linux-only; macOS uses --mac-input",
        ))
    }
}
pub fn bind_input(directory: &Path, config: &Path) -> Result<()> {
    #[cfg(target_os = "linux")]
    {
        linux::bind_input(directory, config)
    }
    #[cfg(not(target_os = "linux"))]
    {
        let _ = (directory, config);
        Err(Error::new(
            "platform_unavailable",
            "Ordinary native input adapter is unverified on this platform",
        ))
    }
}
