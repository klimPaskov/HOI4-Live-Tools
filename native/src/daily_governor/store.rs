use super::types::invalid;
use crate::error::{Error, Result};
use fs2::FileExt;
use sha2::{Digest, Sha256};
use std::{
    fs::{File, OpenOptions},
    io::{Read, Write},
    path::{Path, PathBuf},
};

const LIMIT: u64 = 1 << 20;
#[cfg(unix)]
use std::os::unix::fs::{MetadataExt, OpenOptionsExt};

pub(super) struct Store {
    path: PathBuf,
    lock_path: PathBuf,
    _lock: File,
    fingerprint: Option<[u8; 32]>,
    #[cfg(unix)]
    parent_identity: (u64, u64),
    #[cfg(unix)]
    lock_identity: (u64, u64),
}
#[cfg(unix)]
fn private(metadata: &std::fs::Metadata, directory: bool) -> Result<()> {
    // Ordinary owned-user persistence only. No chmod of preexisting files.
    if metadata.uid() != unsafe { libc::geteuid() }
        || metadata.mode() & 0o777 != if directory { 0o700 } else { 0o600 }
        || if directory {
            !metadata.is_dir()
        } else {
            !metadata.is_file() || metadata.nlink() != 1
        }
    {
        return Err(Error::new(
            "unsafe_plan_path",
            "Private owned regular files and directory required",
        ));
    }
    Ok(())
}
fn parent(path: &Path) -> Result<&Path> {
    path.parent()
        .filter(|p| !p.as_os_str().is_empty())
        .ok_or_else(|| invalid("Absolute private plan path required"))
}
impl Store {
    pub(super) fn open(path: &Path) -> Result<(Self, Option<Vec<u8>>)> {
        #[cfg(not(unix))]
        {
            let _ = path;
            return Err(Error::new(
                "capability_unavailable",
                "Private atomic daily plan storage is not verified on this platform",
            ));
        }
        #[cfg(unix)]
        {
            if !path.is_absolute() || path.file_name().is_none() {
                return Err(invalid("Absolute plan file required"));
            }
            let p = parent(path)?;
            let pm = p.symlink_metadata()?;
            private(&pm, true)?;
            let mut name = path.file_name().unwrap().to_os_string();
            name.push(".lock");
            let lock_path = p.join(name);
            let lock = OpenOptions::new()
                .read(true)
                .write(true)
                .create(true)
                .truncate(false)
                .mode(0o600)
                .custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC)
                .open(&lock_path)?;
            private(&lock.metadata()?, false)?;
            lock.try_lock_exclusive()
                .map_err(|e| Error::new("plan_busy", e.to_string()))?;
            let lm = lock.metadata()?;
            let mut store = Self {
                path: path.to_owned(),
                lock_path,
                _lock: lock,
                fingerprint: None,
                parent_identity: (pm.dev(), pm.ino()),
                lock_identity: (lm.dev(), lm.ino()),
            };
            let bytes = store.read_current()?;
            store.fingerprint = bytes.as_ref().map(|b| Sha256::digest(b).into());
            Ok((store, bytes))
        }
    }
    fn read_current(&self) -> Result<Option<Vec<u8>>> {
        #[cfg(unix)]
        {
            let pm = parent(&self.path)?.symlink_metadata()?;
            private(&pm, true)?;
            let lm = self.lock_path.symlink_metadata()?;
            private(&lm, false)?;
            if (pm.dev(), pm.ino()) != self.parent_identity
                || (lm.dev(), lm.ino()) != self.lock_identity
            {
                return Err(Error::new(
                    "plan_changed",
                    "Plan directory/writer guard replaced",
                ));
            }
            let meta = match self.path.symlink_metadata() {
                Ok(m) => m,
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
                Err(e) => return Err(e.into()),
            };
            private(&meta, false)?;
            if meta.len() > LIMIT {
                return Err(invalid("Plan file exceeds bound"));
            }
            let file = OpenOptions::new()
                .read(true)
                .custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC)
                .open(&self.path)?;
            let fm = file.metadata()?;
            private(&fm, false)?;
            if (meta.dev(), meta.ino()) != (fm.dev(), fm.ino()) {
                return Err(Error::new(
                    "plan_changed",
                    "Plan path changed while opening",
                ));
            }
            let mut bytes = Vec::new();
            file.take(LIMIT + 1).read_to_end(&mut bytes)?;
            if bytes.len() as u64 > LIMIT {
                return Err(invalid("Plan file exceeds bound"));
            }
            let after = self.path.symlink_metadata()?;
            if (
                after.dev(),
                after.ino(),
                after.len(),
                after.mtime(),
                after.mtime_nsec(),
            ) != (fm.dev(), fm.ino(), fm.len(), fm.mtime(), fm.mtime_nsec())
            {
                return Err(Error::new(
                    "plan_changed",
                    "Plan file changed while reading",
                ));
            }
            Ok(Some(bytes))
        }
        #[cfg(not(unix))]
        {
            Err(Error::new(
                "capability_unavailable",
                "Unverified plan storage",
            ))
        }
    }
    pub(super) fn check(&self) -> Result<()> {
        let current = self.read_current()?.map(|b| Sha256::digest(b).into());
        if current != self.fingerprint {
            return Err(Error::new(
                "plan_changed",
                "Daily plan externally changed; preserve evidence",
            ));
        }
        Ok(())
    }
    pub(super) fn replace(&mut self, bytes: &[u8]) -> Result<()> {
        if bytes.len() as u64 > LIMIT {
            return Err(invalid("Plan state exceeds bound"));
        }
        self.check()?;
        #[cfg(unix)]
        {
            use std::sync::atomic::{AtomicU64, Ordering};
            static NEXT: AtomicU64 = AtomicU64::new(0);
            let p = parent(&self.path)?;
            let name = format!(
                ".daily-plan-{}-{}.tmp",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            );
            let temp = p.join(name);
            let mut file = OpenOptions::new()
                .write(true)
                .create_new(true)
                .mode(0o600)
                .custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC)
                .open(&temp)?;
            let result = (|| -> Result<()> {
                private(&file.metadata()?, false)?;
                file.write_all(bytes)?;
                file.sync_all()?;
                self.check()?;
                std::fs::rename(&temp, &self.path)?;
                File::open(p)?.sync_all()?;
                Ok(())
            })();
            if result.is_err() {
                let _ = std::fs::remove_file(&temp);
            }
            result?;
            self.fingerprint = Some(Sha256::digest(bytes).into());
            self.check()
        }
        #[cfg(not(unix))]
        {
            Err(Error::new(
                "capability_unavailable",
                "Unverified plan storage",
            ))
        }
    }
}
