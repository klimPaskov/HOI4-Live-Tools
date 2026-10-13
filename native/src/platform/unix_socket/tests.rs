use super::*;
use std::{
    fs,
    io::{Read, Write},
    os::unix::{
        ffi::OsStringExt,
        fs::{DirBuilderExt, PermissionsExt},
    },
    path::PathBuf,
    sync::atomic::{AtomicUsize, Ordering},
};

static NEXT: AtomicUsize = AtomicUsize::new(0);
struct Directory(PathBuf);
impl Directory {
    fn new() -> Self {
        let path = PathBuf::from(format!(
            "/tmp/h4-sun-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::DirBuilder::new().mode(0o700).create(&path).unwrap();
        Self(path)
    }
    fn path(&self, bytes: usize) -> PathBuf {
        let prefix = self.0.as_os_str().as_bytes().len() + 1;
        assert!(bytes > prefix);
        self.0.join("s".repeat(bytes - prefix))
    }
}
impl Drop for Directory {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[test]
fn native_path_limit_reserves_nul_and_counts_raw_bytes() {
    assert_eq!(
        address().sun_path.len(),
        108,
        "Linux pathname ABI array includes NUL"
    );
    validate_path(Path::new(&format!("/{}", "a".repeat(106)))).unwrap();
    let long = PathBuf::from(format!("/{}", "a".repeat(107)));
    let error = validate_path(&long).unwrap_err();
    assert_eq!(error.code, "invalid_endpoint");
    assert!(error.message.contains("108 bytes"));
    assert!(error.message.contains("107 bytes plus the terminating NUL"));
    let unicode = PathBuf::from(format!("/{}", "é".repeat(53)));
    assert_eq!(unicode.as_os_str().as_bytes().len(), 107);
    validate_path(&unicode).unwrap();
    let long_unicode = PathBuf::from(format!("/a{}", "é".repeat(53)));
    assert!(long_unicode.to_str().unwrap().chars().count() < 107);
    assert_eq!(long_unicode.as_os_str().as_bytes().len(), 108);
    assert_eq!(
        validate_path(&long_unicode).unwrap_err().code,
        "invalid_endpoint"
    );
    // Unix path validation must neither count lossy Unicode nor require UTF8.
    let mut raw = vec![b'/'];
    raw.extend_from_slice(&[0xff; 106]);
    validate_path(&PathBuf::from(std::ffi::OsString::from_vec(raw))).unwrap();
    for path in [
        PathBuf::from("relative"),
        PathBuf::from(std::ffi::OsString::from_vec(b"/a\0b".to_vec())),
    ] {
        assert_eq!(validate_path(&path).unwrap_err().code, "invalid_endpoint");
    }
}

#[test]
fn maximum_terminated_path_binds_and_connects_real_linux_socket() {
    // SAFETY: ordinary fixture UID query only; no game, display or process input.
    assert_ne!(
        unsafe { libc::getuid() },
        0,
        "Socket fixture requires ordinary user"
    );
    let directory = Directory::new();
    let path = directory.path(107);
    validate_path(&path).unwrap();
    let listener = std::os::unix::net::UnixListener::bind(&path).unwrap();
    fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).unwrap();
    let mut client = connect(&path).unwrap();
    client
        .set_write_timeout(Some(Duration::from_secs(2)))
        .unwrap();
    client
        .set_read_timeout(Some(Duration::from_secs(2)))
        .unwrap();
    let (mut accepted, _) = listener.accept().unwrap();
    accepted
        .set_read_timeout(Some(Duration::from_secs(2)))
        .unwrap();
    accepted
        .set_write_timeout(Some(Duration::from_secs(2)))
        .unwrap();
    client.write_all(b"107+NUL").unwrap();
    let mut bytes = [0; 7];
    accepted.read_exact(&mut bytes).unwrap();
    assert_eq!(&bytes, b"107+NUL");
    accepted.write_all(b"ok").unwrap();
    let mut ack = [0; 2];
    client.read_exact(&mut ack).unwrap();
    assert_eq!(&ack, b"ok");
}

#[test]
fn overlength_connector_returns_typed_error_without_named_files() {
    let directory = Directory::new();
    let path = directory.path(108);
    assert_eq!(fs::read_dir(&directory.0).unwrap().count(), 0);
    let error = connect(&path).unwrap_err();
    assert_eq!(error.code, "invalid_endpoint");
    assert!(fs::symlink_metadata(&path).is_err());
    assert_eq!(fs::read_dir(&directory.0).unwrap().count(), 0);
}
