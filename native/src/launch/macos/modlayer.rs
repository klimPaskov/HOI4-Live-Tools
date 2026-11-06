//! Mod-aware game content: with mods enabled the game reads each file from the last mod that
//! has it (mods in load order over the install), and a mod's `replace_path` hides the install's
//! (and earlier mods') files in that folder. The readers use plain paths, so the merged view is
//! materialised once as a tree of symbolic links in the temporary folder and handed out instead
//! of the install. Without enabled mods the install is used as before.
//!
//! Sources, all read-only: the user folder (`HOI4_USERDIR`, else the running game's
//! `-userdir=` argument, else the platform default), its `dlc_load.json` `enabled_mods`, and
//! each mod's `.mod` descriptor (`path`, `replace_path`).
use std::{
    collections::{BTreeMap, HashMap},
    path::{Path, PathBuf},
    sync::{Mutex, OnceLock},
};

/// Folders the tools read from the game's content.
const LAYERED: [&str; 5] = ["map", "history", "common", "localisation", "interface"];

#[derive(Clone, Debug, PartialEq)]
pub(super) struct ModDescriptor {
    pub path: PathBuf,
    pub replace_paths: Vec<String>,
}

/// `path` and `replace_path` entries of a `.mod` descriptor (relative paths are taken from
/// the user folder, as the launcher writes them).
pub(super) fn parse_descriptor(text: &str, userdir: &Path) -> Option<ModDescriptor> {
    let mut path = None;
    let mut replace = Vec::new();
    for line in text.lines() {
        let line = line.split('#').next().unwrap_or("").trim();
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        let value = value.trim().trim_matches('"').to_string();
        match key.trim() {
            "path" | "archive" if !value.is_empty() => {
                let p = PathBuf::from(&value);
                path = Some(if p.is_absolute() { p } else { userdir.join(p) });
            }
            "replace_path" if !value.is_empty() => {
                replace.push(value.trim_end_matches('/').to_string())
            }
            _ => {}
        }
    }
    Some(ModDescriptor {
        path: path?,
        replace_paths: replace,
    })
}

/// `enabled_mods` of a `dlc_load.json` (descriptor paths relative to the user folder).
pub(super) fn parse_enabled_mods(json: &str) -> Vec<String> {
    serde_json::from_str::<serde_json::Value>(json)
        .ok()
        .and_then(|v| v.get("enabled_mods").cloned())
        .and_then(|v| v.as_array().cloned())
        .unwrap_or_default()
        .into_iter()
        .filter_map(|v| v.as_str().map(str::to_string))
        .collect()
}

/// `-userdir=` from a command line.
pub(super) fn userdir_argument(args: &str) -> Option<PathBuf> {
    let at = args.find("-userdir=")?;
    let rest = &args[at + "-userdir=".len()..];
    // The path may hold spaces; it ends at the next ` -` option or the end.
    let end = rest.find(" -").unwrap_or(rest.len());
    let p = rest[..end].trim().trim_matches('"');
    (!p.is_empty()).then(|| PathBuf::from(p))
}

fn userdir(exe: &str) -> Option<PathBuf> {
    if let Some(d) = std::env::var_os("HOI4_USERDIR") {
        return Some(PathBuf::from(d));
    }
    // The running game's own argument (the process list holds every user's command lines;
    // only lines starting with this executable are looked at).
    if let Ok(out) = std::process::Command::new("ps")
        .args(["-ww", "-axo", "args="])
        .output()
    {
        let text = String::from_utf8_lossy(&out.stdout);
        for line in text.lines() {
            if line.trim_start().starts_with(exe) {
                if let Some(d) = userdir_argument(line) {
                    return Some(d);
                }
            }
        }
    }
    let home = std::env::var_os("HOME").map(PathBuf::from)?;
    let documents = home.join("Documents/Paradox Interactive/Hearts of Iron IV");
    if documents.is_dir() {
        return Some(documents);
    }
    let linux = home.join(".local/share/Paradox Interactive/Hearts of Iron IV");
    linux.is_dir().then_some(linux)
}

/// Relative file path -> source file, for the layered folders: the install, then each mod in
/// order (its replace_paths first remove what is below them).
pub(super) fn merge(install: &Path, mods: &[ModDescriptor]) -> BTreeMap<String, PathBuf> {
    let mut files: BTreeMap<String, PathBuf> = BTreeMap::new();
    let add_tree = |root: &Path, files: &mut BTreeMap<String, PathBuf>| {
        for top in LAYERED {
            walk(&root.join(top), top, files);
        }
    };
    add_tree(install, &mut files);
    for m in mods {
        for r in &m.replace_paths {
            let prefix = format!("{r}/");
            files.retain(|k, _| {
                // replace_path hides the files directly in that folder, not its sub-folders.
                !(k.starts_with(&prefix) && !k[prefix.len()..].contains('/'))
            });
        }
        add_tree(&m.path, &mut files);
    }
    files
}
fn walk(dir: &Path, rel: &str, out: &mut BTreeMap<String, PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for e in entries.flatten() {
        let name = e.file_name().to_string_lossy().to_string();
        let path = e.path();
        let key = format!("{rel}/{name}");
        if path.is_dir() {
            walk(&path, &key, out);
        } else {
            out.insert(key, path);
        }
    }
}

fn materialise(files: &BTreeMap<String, PathBuf>, root: &Path) -> std::io::Result<()> {
    if root.exists() {
        std::fs::remove_dir_all(root)?;
    }
    std::fs::create_dir_all(root)?;
    for (rel, source) in files {
        let target = root.join(rel);
        if let Some(parent) = target.parent() {
            std::fs::create_dir_all(parent)?;
        }
        #[cfg(unix)]
        std::os::unix::fs::symlink(source, &target)?;
        #[cfg(not(unix))]
        std::fs::copy(source, &target).map(|_| ())?;
    }
    Ok(())
}

/// The merged content folder for the running game's enabled mods, or None without mods (the
/// install is used) or when the mods cannot be read.
pub(super) fn content_dir(install: &Path, exe: &str) -> Option<PathBuf> {
    static CACHE: OnceLock<Mutex<HashMap<PathBuf, Option<PathBuf>>>> = OnceLock::new();
    let cache = CACHE.get_or_init(|| Mutex::new(HashMap::new()));
    if let Some(found) = cache.lock().ok()?.get(install) {
        return found.clone();
    }
    let built = (|| {
        let user = userdir(exe)?;
        let enabled =
            parse_enabled_mods(&std::fs::read_to_string(user.join("dlc_load.json")).ok()?);
        if enabled.is_empty() {
            return None;
        }
        let mods: Vec<ModDescriptor> = enabled
            .iter()
            .filter_map(|d| {
                let text = std::fs::read_to_string(user.join(d)).ok()?;
                parse_descriptor(&text, &user)
            })
            .filter(|m| m.path.is_dir())
            .collect();
        if mods.is_empty() {
            return None;
        }
        let files = merge(install, &mods);
        // One folder per mod set: the hash of the descriptor paths names it.
        use std::hash::{Hash, Hasher};
        let mut h = std::collections::hash_map::DefaultHasher::new();
        for m in &mods {
            m.path.hash(&mut h);
            m.replace_paths.hash(&mut h);
        }
        let root = std::env::temp_dir().join(format!("hoi4-live-content-{:016x}", h.finish()));
        materialise(&files, &root).ok()?;
        // Readers check these two folders to recognise game content.
        (root.join("map").is_dir() && root.join("history").is_dir()).then_some(root)
    })();
    if let Ok(mut c) = cache.lock() {
        c.insert(install.to_path_buf(), built.clone());
    }
    built
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn descriptor_path_and_replace_paths_are_read() {
        let text = "version=\"1.0\"\nname=\"Big Map\"\nreplace_path=\"history/states\"\nreplace_path=\"map/strategicregions/\"\npath=\"/mods/bigmap\" # comment\n";
        let d = parse_descriptor(text, Path::new("/user")).unwrap();
        assert_eq!(d.path, PathBuf::from("/mods/bigmap"));
        assert_eq!(
            d.replace_paths,
            vec!["history/states", "map/strategicregions"]
        );
        let rel = parse_descriptor("path=\"mod/x\"", Path::new("/user")).unwrap();
        assert_eq!(rel.path, PathBuf::from("/user/mod/x"));
    }
    #[test]
    fn enabled_mods_come_from_dlc_load() {
        assert_eq!(
            parse_enabled_mods(
                "{\"enabled_mods\":[\"mod/ugc_1.mod\",\"mod/b.mod\"],\"disabled_dlcs\":[]}"
            ),
            vec!["mod/ugc_1.mod", "mod/b.mod"]
        );
        assert!(parse_enabled_mods("{\"enabled_mods\":[],\"disabled_dlcs\":[]}").is_empty());
    }
    #[test]
    fn userdir_argument_keeps_spaces_in_the_path() {
        assert_eq!(
            userdir_argument("/x/hoi4 -userdir=/Users/a/My Runs/w12/userdata -debug"),
            Some(PathBuf::from("/Users/a/My Runs/w12/userdata"))
        );
        assert_eq!(userdir_argument("/x/hoi4"), None);
    }
    #[test]
    fn mods_override_files_and_replace_path_hides_a_folder() {
        let base = std::env::temp_dir().join(format!("modlayer-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        let install = base.join("install");
        let m = base.join("mod");
        for (p, t) in [
            ("install/history/states/1-A.txt", "a"),
            ("install/history/states/2-B.txt", "b"),
            ("install/map/buildings.txt", "vanilla"),
            ("install/common/resources/00_resources.txt", "r"),
            ("mod/history/states/9-Z.txt", "z"),
            ("mod/map/buildings.txt", "modded"),
        ] {
            let f = base.join(p);
            std::fs::create_dir_all(f.parent().unwrap()).unwrap();
            std::fs::write(f, t).unwrap();
        }
        let files = merge(
            &install,
            &[ModDescriptor {
                path: m.clone(),
                replace_paths: vec!["history/states".into()],
            }],
        );
        assert!(!files.contains_key("history/states/1-A.txt"));
        assert_eq!(
            files["history/states/9-Z.txt"],
            m.join("history/states/9-Z.txt")
        );
        assert_eq!(files["map/buildings.txt"], m.join("map/buildings.txt"));
        assert_eq!(
            files["common/resources/00_resources.txt"],
            install.join("common/resources/00_resources.txt")
        );
        let root = base.join("merged");
        materialise(&files, &root).unwrap();
        assert_eq!(
            std::fs::read_to_string(root.join("map/buildings.txt")).unwrap(),
            "modded"
        );
        let _ = std::fs::remove_dir_all(&base);
    }
}
