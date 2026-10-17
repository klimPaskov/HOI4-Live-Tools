//! Explicit local Xauthority authentication, with no environment lookup.
//! The record format was checked against x11rb-protocol 0.13.2's xauth API;
//! this bounded parser is independent and does not copy its implementation.
use crate::error::{Error, Result};
use std::{
    fs::{self, Metadata, OpenOptions},
    io::Read,
    os::unix::fs::{MetadataExt, OpenOptionsExt},
    path::Path,
};

const LIMIT: usize = 64 << 10;
const LOCAL_FAMILY: u16 = 256;
pub(super) const MIT_COOKIE_NAME: &[u8] = b"MIT-MAGIC-COOKIE-1";

fn invalid(message: &'static str) -> Error {
    Error::new("invalid_xauthority", message)
}

struct Records<'a> {
    bytes: &'a [u8],
    offset: usize,
}
impl<'a> Records<'a> {
    fn take(&mut self, count: usize) -> Result<&'a [u8]> {
        let end = self
            .offset
            .checked_add(count)
            .filter(|end| *end <= self.bytes.len())
            .ok_or_else(|| invalid("Truncated Xauthority record"))?;
        let bytes = &self.bytes[self.offset..end];
        self.offset = end;
        Ok(bytes)
    }
    fn u16(&mut self) -> Result<u16> {
        Ok(u16::from_be_bytes(self.take(2)?.try_into().unwrap()))
    }
    fn field(&mut self) -> Result<&'a [u8]> {
        let count = usize::from(self.u16()?);
        self.take(count)
    }
}

fn cookie(bytes: &[u8], hostname: &[u8], display: u16) -> Result<[u8; 16]> {
    if bytes.is_empty() || bytes.len() > LIMIT {
        return Err(invalid("Xauthority file must contain at most 64 KiB"));
    }
    if hostname.is_empty() || hostname.len() > 255 || hostname.contains(&0) {
        return Err(invalid("Exact local hostname required for Xauthority"));
    }
    let display = display.to_string();
    let mut records = Records { bytes, offset: 0 };
    let mut found = None;
    while records.offset < bytes.len() {
        let family = records.u16()?;
        let address = records.field()?;
        let number = records.field()?;
        let name = records.field()?;
        let data = records.field()?;
        // Wildcard families, empty display numbers, alternate hosts/protocols
        // and noncanonical display numbers never authorize this connection.
        if family == LOCAL_FAMILY
            && address == hostname
            && number == display.as_bytes()
            && name == MIT_COOKIE_NAME
        {
            let value: [u8; 16] = data
                .try_into()
                .map_err(|_| invalid("MIT cookie must be exactly 16 bytes"))?;
            if found.replace(value).is_some() {
                return Err(invalid("Ambiguous matching Xauthority records"));
            }
        }
    }
    found.ok_or_else(|| invalid("Exact local display MIT cookie is unavailable"))
}

fn private_file(meta: &Metadata, uid: u32) -> Result<()> {
    if uid == 0 || meta.uid() != uid || !meta.is_file() || meta.mode() & 0o077 != 0 {
        return Err(invalid(
            "Ordinary-user private regular Xauthority file required",
        ));
    }
    if meta.len() == 0 || meta.len() > LIMIT as u64 {
        return Err(invalid("Xauthority file must contain at most 64 KiB"));
    }
    Ok(())
}

fn same_file(left: &Metadata, right: &Metadata) -> bool {
    left.dev() == right.dev()
        && left.ino() == right.ino()
        && left.uid() == right.uid()
        && left.mode() == right.mode()
        && left.len() == right.len()
        && left.mtime() == right.mtime()
        && left.mtime_nsec() == right.mtime_nsec()
        && left.ctime() == right.ctime()
        && left.ctime_nsec() == right.ctime_nsec()
}

pub(super) fn load_cookie(path: &Path, hostname: &[u8], display: u16) -> Result<[u8; 16]> {
    if !path.is_absolute() {
        return Err(invalid("Explicit absolute Xauthority path required"));
    }
    // SAFETY: getuid only returns the current real UID, without side effects.
    let uid = unsafe { libc::getuid() };
    if uid == 0 {
        return Err(invalid(
            "Xauthority authentication requires an ordinary user",
        ));
    }
    let file = OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK | libc::O_CLOEXEC)
        .open(path)
        .map_err(|_| invalid("Explicit private Xauthority file is unavailable"))?;
    let before = file
        .metadata()
        .map_err(|_| invalid("Xauthority file metadata is unavailable"))?;
    private_file(&before, uid)?;
    let mut bytes = Vec::with_capacity(before.len() as usize);
    // Cap even a concurrently growing file; O_NONBLOCK prevents opening a
    // substituted FIFO from waiting before its file type can be checked.
    (&file)
        .take((LIMIT + 1) as u64)
        .read_to_end(&mut bytes)
        .map_err(|_| invalid("Xauthority file read failed"))?;
    let after = file
        .metadata()
        .map_err(|_| invalid("Xauthority file metadata is unavailable"))?;
    let current =
        fs::symlink_metadata(path).map_err(|_| invalid("Xauthority path changed while reading"))?;
    private_file(&after, uid)?;
    private_file(&current, uid)?;
    if bytes.len() as u64 != before.len()
        || !same_file(&before, &after)
        || !same_file(&after, &current)
    {
        return Err(invalid("Xauthority file changed while reading"));
    }
    cookie(&bytes, hostname, display)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        ffi::CString,
        os::unix::{
            ffi::OsStrExt,
            fs::{symlink, DirBuilderExt, PermissionsExt},
            net::UnixListener,
        },
        path::PathBuf,
        sync::atomic::{AtomicUsize, Ordering},
        time::{Duration, Instant},
    };

    const HOST: &[u8] = b"fixture-host";
    const DISPLAY: u16 = 128;
    const VALUE: [u8; 16] = [0x5a; 16];

    fn record(family: u16, host: &[u8], number: &[u8], name: &[u8], data: &[u8]) -> Vec<u8> {
        let mut bytes = family.to_be_bytes().to_vec();
        for field in [host, number, name, data] {
            bytes.extend_from_slice(&u16::try_from(field.len()).unwrap().to_be_bytes());
            bytes.extend_from_slice(field);
        }
        bytes
    }
    fn valid() -> Vec<u8> {
        record(LOCAL_FAMILY, HOST, b"128", MIT_COOKIE_NAME, &VALUE)
    }

    #[test]
    fn exact_local_cookie_selected_among_unrelated_records() {
        let mut bytes = record(0, HOST, b"128", MIT_COOKIE_NAME, &[1; 16]);
        bytes.extend(record(
            LOCAL_FAMILY,
            b"other-host",
            b"128",
            MIT_COOKIE_NAME,
            &[2; 16],
        ));
        bytes.extend(record(
            LOCAL_FAMILY,
            HOST,
            b"129",
            MIT_COOKIE_NAME,
            &[3; 16],
        ));
        bytes.extend(record(
            LOCAL_FAMILY,
            HOST,
            b"128",
            b"other-protocol",
            &[4; 16],
        ));
        bytes.extend(valid());
        assert_eq!(cookie(&bytes, HOST, DISPLAY).unwrap(), VALUE);
    }

    #[test]
    fn wildcards_noncanonical_display_and_duplicate_matches_are_refused() {
        for bytes in [
            record(65535, HOST, b"128", MIT_COOKIE_NAME, &VALUE),
            record(LOCAL_FAMILY, HOST, b"", MIT_COOKIE_NAME, &VALUE),
            record(LOCAL_FAMILY, HOST, b"0128", MIT_COOKIE_NAME, &VALUE),
            record(LOCAL_FAMILY, HOST, b"128.0", MIT_COOKIE_NAME, &VALUE),
        ] {
            assert_eq!(
                cookie(&bytes, HOST, DISPLAY).unwrap_err().code,
                "invalid_xauthority"
            );
        }
        for other in [VALUE, [0x6b; 16]] {
            let mut bytes = valid();
            bytes.extend(record(LOCAL_FAMILY, HOST, b"128", MIT_COOKIE_NAME, &other));
            assert_eq!(
                cookie(&bytes, HOST, DISPLAY).unwrap_err().code,
                "invalid_xauthority"
            );
        }
    }

    #[test]
    fn cookie_length_and_local_hostname_are_strict() {
        for length in (0..=32).filter(|length| *length != 16) {
            let bytes = record(
                LOCAL_FAMILY,
                HOST,
                b"128",
                MIT_COOKIE_NAME,
                &vec![1; length],
            );
            assert!(cookie(&bytes, HOST, DISPLAY).is_err());
        }
        for host in [b"".as_slice(), b"fixture-host\0".as_slice(), &[b'a'; 256]] {
            assert!(cookie(&valid(), host, DISPLAY).is_err());
        }
    }

    #[test]
    fn truncated_records_and_trailing_partial_family_are_refused() {
        let bytes = valid();
        for end in 0..bytes.len() {
            assert!(
                cookie(&bytes[..end], HOST, DISPLAY).is_err(),
                "prefix {end}"
            );
        }
        let mut trailing = bytes.clone();
        trailing.push(0);
        assert!(cookie(&trailing, HOST, DISPLAY).is_err());
        trailing.extend_from_slice(&[1, 0xff, 0xff]);
        assert!(cookie(&trailing, HOST, DISPLAY).is_err());
    }

    #[test]
    fn full_64k_file_is_allowed_but_larger_input_is_refused() {
        let mut bytes = valid();
        let overhead = record(2, b"", b"", b"", b"").len();
        bytes.extend(record(
            2,
            b"",
            b"",
            b"",
            &vec![0; LIMIT - bytes.len() - overhead],
        ));
        assert_eq!(bytes.len(), LIMIT);
        assert_eq!(cookie(&bytes, HOST, DISPLAY).unwrap(), VALUE);
        bytes.push(0);
        assert!(cookie(&bytes, HOST, DISPLAY).is_err());
    }

    struct PrivateFile {
        directory: PathBuf,
        path: PathBuf,
    }
    impl PrivateFile {
        fn new() -> Option<Self> {
            // SAFETY: read-only UID query. File tests require an ordinary user.
            if unsafe { libc::getuid() } == 0 {
                eprintln!("SKIP: private Xauthority file fixture requires ordinary UID");
                return None;
            }
            static NEXT: AtomicUsize = AtomicUsize::new(0);
            let directory = std::env::temp_dir().join(format!(
                "hoi4-xauthority-fixture-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            fs::DirBuilder::new()
                .mode(0o700)
                .create(&directory)
                .unwrap();
            let path = directory.join("authority");
            let mut file = OpenOptions::new()
                .write(true)
                .create_new(true)
                .mode(0o600)
                .open(&path)
                .unwrap();
            std::io::Write::write_all(&mut file, &valid()).unwrap();
            Some(Self { directory, path })
        }
    }
    impl Drop for PrivateFile {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.directory);
        }
    }

    #[test]
    fn explicit_private_regular_file_is_loaded_and_public_mode_is_refused() {
        let Some(fixture) = PrivateFile::new() else {
            return;
        };
        assert_eq!(load_cookie(&fixture.path, HOST, DISPLAY).unwrap(), VALUE);
        fs::set_permissions(&fixture.path, fs::Permissions::from_mode(0o644)).unwrap();
        assert_eq!(
            load_cookie(&fixture.path, HOST, DISPLAY).unwrap_err().code,
            "invalid_xauthority"
        );
        fs::set_permissions(&fixture.path, fs::Permissions::from_mode(0o600)).unwrap();
        assert_eq!(load_cookie(&fixture.path, HOST, DISPLAY).unwrap(), VALUE);
        let meta = fs::metadata(&fixture.path).unwrap();
        assert!(private_file(&meta, meta.uid().checked_add(1).unwrap()).is_err());
        assert!(private_file(&meta, 0).is_err());
    }

    #[test]
    fn relative_symlink_empty_and_oversized_files_are_refused() {
        let Some(fixture) = PrivateFile::new() else {
            return;
        };
        assert!(load_cookie(Path::new("authority"), HOST, DISPLAY).is_err());
        let alias = fixture.directory.join("alias");
        symlink(&fixture.path, &alias).unwrap();
        assert!(load_cookie(&alias, HOST, DISPLAY).is_err());
        fs::write(&fixture.path, b"").unwrap();
        assert!(load_cookie(&fixture.path, HOST, DISPLAY).is_err());
        fs::write(&fixture.path, vec![0; LIMIT + 1]).unwrap();
        assert!(load_cookie(&fixture.path, HOST, DISPLAY).is_err());
    }

    #[test]
    fn fifo_directory_and_socket_are_refused_without_blocking() {
        let Some(fixture) = PrivateFile::new() else {
            return;
        };
        let fifo = fixture.directory.join("fifo");
        let name = CString::new(fifo.as_os_str().as_bytes()).unwrap();
        // SAFETY: create one private FIFO fixture, never a host/game resource.
        assert_eq!(unsafe { libc::mkfifo(name.as_ptr(), 0o600) }, 0);
        let socket = fixture.directory.join("socket");
        let _listener = UnixListener::bind(&socket).unwrap();
        let start = Instant::now();
        for path in [&fifo, &fixture.directory, &socket] {
            assert!(load_cookie(path, HOST, DISPLAY).is_err());
        }
        assert!(start.elapsed() < Duration::from_secs(1));
    }
}
