//! Explicit immutable FEX profile. No inherited FEX environment or global
//! configuration writes; this guard never creates or repairs profile entries.
use super::*;
use serde::de::{MapAccess, Visitor};
use sha2::{Digest, Sha256};
use std::{collections::BTreeMap, ffi::OsString, fmt};

const FILES: [&str; 2] = ["Config.json", "ThunksDB.json"];
const LEGACY_GL: [&str; 4] = [
    "@PREFIX_LIB@/libGL.so",
    "@PREFIX_LIB@/libGL.so.1",
    "@PREFIX_LIB@/libGL.so.1.2.0",
    "@PREFIX_LIB@/libGL.so.1.7.0",
];
const SPLIT_GL: [&str; 4] = [
    "@PREFIX_LIB@/libOpenGL.so.0",
    "@PREFIX_LIB@/libOpenGL.so.0.0.0",
    "@PREFIX_LIB@/libGLX.so.0",
    "@PREFIX_LIB@/libGLX.so.0.0.0",
];

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Spec {
    directory: PathBuf,
    config_sha256: String,
    thunks_db_sha256: String,
}

#[derive(Debug, PartialEq, Eq)]
struct Stamp {
    dev: u64,
    ino: u64,
    uid: u32,
    mode: u32,
    len: u64,
    mtime: (i64, i64),
    ctime: (i64, i64),
}
impl Stamp {
    fn from(meta: &fs::Metadata) -> Self {
        Self {
            dev: meta.dev(),
            ino: meta.ino(),
            uid: meta.uid(),
            mode: meta.mode(),
            len: meta.len(),
            mtime: (meta.mtime(), meta.mtime_nsec()),
            ctime: (meta.ctime(), meta.ctime_nsec()),
        }
    }
}

pub(super) struct Guard {
    directory: PathBuf,
    directory_stamp: Stamp,
    appconfig_stamp: Stamp,
    files: [(Stamp, String); 2],
}

fn invalid(message: &'static str) -> Error {
    Error::new("invalid_fex_config", message)
}

fn stamp(path: &Path, directory: bool) -> Result<Stamp> {
    let meta = fs::symlink_metadata(path)?;
    if meta.uid() != uid()
        || uid() == 0
        || meta.mode() & 0o077 != 0
        || meta.mode() & 0o7000 != 0
        || (if directory {
            !meta.is_dir()
        } else {
            !meta.is_file()
        })
    {
        return Err(invalid(
            "FEX profile entries must be private, ordinary-user owned and nonsymlink",
        ));
    }
    Ok(Stamp::from(&meta))
}

fn read(path: &Path, expected: &Stamp, digest: &str) -> Result<Vec<u8>> {
    let mut file = OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
        .open(path)?;
    if Stamp::from(&file.metadata()?) != *expected || stamp(path, false)? != *expected {
        return Err(invalid("FEX profile file identity changed"));
    }
    let mut bytes = Vec::new();
    Read::by_ref(&mut file)
        .take(65537)
        .read_to_end(&mut bytes)?;
    if bytes.len() > 65536 || bytes.len() as u64 != expected.len {
        return Err(invalid("FEX profile file exceeds bound or changed size"));
    }
    if Stamp::from(&file.metadata()?) != *expected || stamp(path, false)? != *expected {
        return Err(invalid("FEX profile file changed during read"));
    }
    if digest.len() != 64
        || !digest
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        || format!("{:x}", Sha256::digest(&bytes)) != digest
    {
        return Err(invalid("FEX profile SHA-256 differs from the explicit pin"));
    }
    Ok(bytes)
}

// Derived structs reject duplicate fields; this map also rejects duplicate
// database names rather than silently retaining their final definition.
fn unique_map<'de, D, T>(deserializer: D) -> std::result::Result<BTreeMap<String, T>, D::Error>
where
    D: serde::Deserializer<'de>,
    T: Deserialize<'de>,
{
    struct Unique<T>(std::marker::PhantomData<T>);
    impl<'de, T: Deserialize<'de>> Visitor<'de> for Unique<T> {
        type Value = BTreeMap<String, T>;
        fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
            formatter.write_str("an object with unique thunk names")
        }
        fn visit_map<M: MapAccess<'de>>(
            self,
            mut map: M,
        ) -> std::result::Result<Self::Value, M::Error> {
            let mut out = BTreeMap::new();
            while let Some((key, value)) = map.next_entry::<String, T>()? {
                if out.insert(key, value).is_some() {
                    return Err(serde::de::Error::custom("Duplicate thunk name"));
                }
            }
            Ok(out)
        }
    }
    deserializer.deserialize_map(Unique(std::marker::PhantomData))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Configuration {
    #[serde(rename = "Config")]
    config: Settings,
    #[serde(rename = "ThunksDB")]
    thunks: Switches,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Settings {
    #[serde(rename = "RootFS")]
    rootfs: PathBuf,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Switches {
    #[serde(rename = "GL")]
    gl: u8,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Database {
    #[serde(rename = "DB", deserialize_with = "unique_map")]
    entries: BTreeMap<String, Thunk>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Thunk {
    #[serde(rename = "Library")]
    library: String,
    #[serde(rename = "Overlay")]
    overlay: Vec<String>,
}
fn schema(config: &[u8], database: &[u8]) -> Result<()> {
    let config: Configuration = serde_json::from_slice(config)
        .map_err(|_| invalid("FEX Config.json requires only absolute RootFS and GL 0 or 1"))?;
    if !config.config.rootfs.is_absolute() || config.thunks.gl > 1 {
        return Err(invalid(
            "FEX Config.json requires only absolute RootFS and GL 0 or 1",
        ));
    }
    let db: Database = serde_json::from_slice(database)
        .map_err(|_| invalid("FEX ThunksDB.json has unsupported or duplicate fields"))?;
    let gl = db
        .entries
        .get("GL")
        .ok_or_else(|| invalid("FEX GL definition required"))?;
    let legacy: Vec<_> = LEGACY_GL.iter().map(|s| s.to_string()).collect();
    let unified: Vec<_> = LEGACY_GL
        .iter()
        .chain(SPLIT_GL.iter())
        .map(|s| s.to_string())
        .collect();
    if gl.library != "libGL-guest.so" || (gl.overlay != legacy && gl.overlay != unified) {
        return Err(invalid(
            "Only the reviewed legacy or unified FEX GL overlay is supported",
        ));
    }
    Ok(())
}

impl Guard {
    pub(super) fn capture(spec: &Spec) -> Result<Self> {
        if !spec.directory.is_absolute() || spec.directory.canonicalize()? != spec.directory {
            return Err(invalid(
                "FEX configuration directory must be absolute and canonical",
            ));
        }
        let directory_stamp = stamp(&spec.directory, true)?;
        let appconfig_stamp = stamp(&spec.directory.join("AppConfig"), true)?;
        let files = [
            (
                stamp(&spec.directory.join(FILES[0]), false)?,
                spec.config_sha256.clone(),
            ),
            (
                stamp(&spec.directory.join(FILES[1]), false)?,
                spec.thunks_db_sha256.clone(),
            ),
        ];
        let guard = Self {
            directory: spec.directory.clone(),
            directory_stamp,
            appconfig_stamp,
            files,
        };
        let config = read(
            &guard.directory.join(FILES[0]),
            &guard.files[0].0,
            &guard.files[0].1,
        )?;
        let db = read(
            &guard.directory.join(FILES[1]),
            &guard.files[1].0,
            &guard.files[1].1,
        )?;
        schema(&config, &db)?;
        guard.verify()?;
        Ok(guard)
    }

    pub(super) fn verify(&self) -> Result<()> {
        if self.directory.canonicalize()? != self.directory
            || stamp(&self.directory, true)? != self.directory_stamp
            || stamp(&self.directory.join("AppConfig"), true)? != self.appconfig_stamp
        {
            return Err(invalid("Original FEX configuration directory changed"));
        }
        let mut names: Vec<_> = fs::read_dir(&self.directory)?
            .take(4)
            .map(|entry| entry.map(|entry| entry.file_name()))
            .collect::<std::io::Result<_>>()?;
        names.sort();
        if names != ["AppConfig", FILES[0], FILES[1]].map(OsString::from)
            || fs::read_dir(self.directory.join("AppConfig"))?
                .next()
                .is_some()
        {
            return Err(invalid(
                "FEX profile requires exactly two files and an empty AppConfig directory",
            ));
        }
        for (name, (expected, digest)) in FILES.iter().zip(&self.files) {
            read(&self.directory.join(name), expected, digest)?;
        }
        // Reads may change atime only. Preserve the original directory identities
        // and modification/change timestamps across every verification.
        if stamp(&self.directory, true)? != self.directory_stamp
            || stamp(&self.directory.join("AppConfig"), true)? != self.appconfig_stamp
        {
            return Err(invalid(
                "Original FEX directory changed during verification",
            ));
        }
        Ok(())
    }

    pub(super) fn configure(&self, command: &mut Command) -> Result<()> {
        self.verify()?;
        let mut path = self.directory.as_os_str().to_os_string();
        path.push("/");
        command.env("FEX_APP_CONFIG_LOCATION", path);
        Ok(())
    }
}

#[cfg(test)]
mod tests;
