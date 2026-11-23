//! Owned Windows launch: this process starts `hoi4.exe` as its direct child, reads it through
//! the owned reader and serves stdio MCP until stdin closes; then it asks the game window to
//! close, waits for the exit and restores the settings it changed.
//!
//! The Windows game reads its user folder from `gameDataPath` in the install's
//! `launcher-settings.json`. The launch backs that file up into the run directory (bytes and
//! SHA-256), points `gameDataPath` at the private user folder of the config, and restores the
//! original bytes only after the owned game has exited (verified by hash). A backup left in
//! the run directory by an interrupted run blocks the next launch until
//! `--windows-restore RUN_DIRECTORY` puts it back.
//!
//! Refuses while any `hoi4.exe` runs (never takes over or touches another game) and while a
//! Steam client runs. The game gets the caller's environment without Steam variables.
use crate::error::{Error, Result};
use serde::Deserialize;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    /// The install folder holding hoi4.exe and launcher-settings.json.
    pub game_dir: PathBuf,
    /// Private user folder (settings, saves, dlc_load.json) for this run.
    pub user_dir: PathBuf,
    /// Run directory for the session record and the settings backup.
    pub run_directory: PathBuf,
    #[serde(default)]
    pub args: Vec<String>,
}

const SETTINGS: &str = "launcher-settings.json";
const BACKUP: &str = "launcher-settings.backup.json";

fn sha256_hex(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

/// The settings text with `gameDataPath` replaced; every other byte kept.
pub(crate) fn redirect_settings(original: &str, user_dir: &str) -> Result<String> {
    let parsed: Value = serde_json::from_str(original.trim_start_matches('\u{feff}'))
        .map_err(|e| Error::new("invalid_settings", e.to_string()))?;
    let old = parsed
        .get("gameDataPath")
        .and_then(Value::as_str)
        .ok_or_else(|| Error::new("invalid_settings", "launcher-settings has no gameDataPath"))?;
    let needle = format!("\"gameDataPath\": {}", serde_json::to_string(old).unwrap());
    let compact = format!("\"gameDataPath\":{}", serde_json::to_string(old).unwrap());
    let value = serde_json::to_string(&user_dir.replace('\\', "/")).unwrap();
    let out = if original.matches(&needle).count() == 1 {
        original.replacen(&needle, &format!("\"gameDataPath\": {value}"), 1)
    } else if original.matches(&compact).count() == 1 {
        original.replacen(&compact, &format!("\"gameDataPath\":{value}"), 1)
    } else {
        return Err(Error::new(
            "invalid_settings",
            "gameDataPath is not written once in the expected form",
        ));
    };
    // The result must still parse and differ only in that value.
    let mut check: Value = serde_json::from_str(out.trim_start_matches('\u{feff}'))
        .map_err(|e| Error::new("invalid_settings", e.to_string()))?;
    let mut want = parsed;
    want["gameDataPath"] = json!(user_dir.replace('\\', "/"));
    check["gameDataPath"] = want["gameDataPath"].clone();
    if check != want {
        return Err(Error::new(
            "invalid_settings",
            "Redirect changed more than gameDataPath",
        ));
    }
    Ok(out)
}

fn write_replace(path: &Path, bytes: &[u8]) -> Result<()> {
    let tmp = path.with_extension("json.hoi4live-tmp");
    std::fs::write(&tmp, bytes)?;
    std::fs::rename(&tmp, path)?;
    Ok(())
}

/// Put the original launcher settings back from the run directory's backup.
pub fn restore(run_directory: &Path) -> Result<Value> {
    let backup_path = run_directory.join(BACKUP);
    let backup: Value = serde_json::from_slice(&std::fs::read(&backup_path)?)
        .map_err(|e| Error::new("invalid_backup", e.to_string()))?;
    let target = PathBuf::from(
        backup["path"]
            .as_str()
            .ok_or_else(|| Error::new("invalid_backup", "Backup has no path"))?,
    );
    let text = backup["original"]
        .as_str()
        .ok_or_else(|| Error::new("invalid_backup", "Backup has no original text"))?;
    let sha = backup["sha256"].as_str().unwrap_or_default();
    if sha256_hex(text.as_bytes()) != sha {
        return Err(Error::new("invalid_backup", "Backup bytes differ from their hash"));
    }
    if game_running()? {
        return Err(Error::new(
            "game_running",
            "Restore only after the game has exited",
        ));
    }
    write_replace(&target, text.as_bytes())?;
    if sha256_hex(&std::fs::read(&target)?) != sha {
        return Err(Error::new("restore_failed", "Restored settings differ"));
    }
    std::fs::remove_file(&backup_path)?;
    Ok(json!({"restored": target, "sha256": sha}))
}

fn processes_named(names: &[&str]) -> Result<Vec<u32>> {
    // tasklist is part of every Windows install; CSV rows "image","pid",...
    let out = std::process::Command::new("tasklist")
        .args(["/fo", "csv", "/nh"])
        .output()?;
    if !out.status.success() {
        return Err(Error::new("windows_api", "tasklist failed"));
    }
    let text = String::from_utf8_lossy(&out.stdout);
    Ok(text
        .lines()
        .filter_map(|line| {
            let mut cols = line.split("\",\"");
            let image = cols.next()?.trim_start_matches('"').to_ascii_lowercase();
            let pid = cols.next()?.parse().ok()?;
            names.contains(&image.as_str()).then_some(pid)
        })
        .collect())
}
fn game_running() -> Result<bool> {
    Ok(!processes_named(&["hoi4.exe"])?.is_empty())
}

fn launch_env() -> Vec<(String, String)> {
    std::env::vars()
        .filter(|(k, _)| {
            let l = k.to_ascii_lowercase();
            !l.starts_with("steam") && !k.to_ascii_uppercase().starts_with("ENABLE_VK_LAYER_VALVE")
        })
        .collect()
}

pub fn serve(config_path: &Path) -> Result<()> {
    let config: Config = serde_json::from_slice(&std::fs::read(config_path)?)
        .map_err(|e| Error::new("invalid_config", e.to_string()))?;
    let exe = config.game_dir.join("hoi4.exe");
    let settings = config.game_dir.join(SETTINGS);
    std::fs::create_dir_all(&config.run_directory)?;
    std::fs::create_dir_all(&config.user_dir)?;
    if config.run_directory.join(BACKUP).exists() {
        return Err(Error::new(
            "restore_pending",
            "A launcher-settings backup from an earlier run is still in the run directory; run --windows-restore first",
        ));
    }
    if game_running()? {
        return Err(Error::new(
            "game_already_running",
            "A hoi4.exe already runs; never take it over",
        ));
    }
    if !processes_named(&["steam.exe", "steamwebhelper.exe"])?.is_empty() {
        return Err(Error::new(
            "steam_running",
            "Close Steam before an owned launch",
        ));
    }
    let original = std::fs::read_to_string(&settings)?;
    let user_dir = config
        .user_dir
        .to_str()
        .ok_or_else(|| Error::new("invalid_config", "user_dir must be Unicode"))?;
    let redirected = redirect_settings(&original, user_dir)?;
    let backup = json!({
        "path": settings,
        "original": original,
        "sha256": sha256_hex(original.as_bytes()),
    });
    write_replace(
        &config.run_directory.join(BACKUP),
        serde_json::to_string_pretty(&backup).unwrap().as_bytes(),
    )?;
    write_replace(&settings, redirected.as_bytes())?;
    let result = run(&config, &exe);
    // Restore whatever happened, once the game is gone.
    let deadline = Instant::now() + Duration::from_secs(120);
    while game_running().unwrap_or(true) && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(500));
    }
    match restore(&config.run_directory) {
        Ok(v) => eprintln!("{v}"),
        Err(e) => eprintln!("launcher settings NOT restored ({e}); run --windows-restore"),
    }
    result
}

fn run(config: &Config, exe: &Path) -> Result<()> {
    let mut child = std::process::Command::new(exe)
        .args(&config.args)
        .current_dir(&config.game_dir)
        .env_clear()
        .envs(launch_env())
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()?;
    let pid = child.id();
    eprintln!("launched hoi4.exe pid {pid}");
    // The executable module appears once the loader has mapped it.
    let started = Instant::now();
    let session = loop {
        match crate::platform::windows::process::owned_session(pid) {
            Ok(s) => break s,
            Err(e) if started.elapsed() < Duration::from_secs(60) => {
                if child.try_wait()?.is_some() {
                    return Err(Error::new("game_exited", "The game exited while starting"));
                }
                let _ = e;
                std::thread::sleep(Duration::from_millis(250));
            }
            Err(e) => return Err(e),
        }
    };
    let record = json!({
        "pid": session.game.pid,
        "creation_filetime": session.game.creation_filetime,
        "executable": session.game.executable,
        "supervisor_pid": session.supervisor.pid,
    });
    std::fs::write(
        config.run_directory.join("session.json"),
        serde_json::to_string_pretty(&record).unwrap(),
    )?;
    let (process, unavailable) = match crate::platform::windows::process::WindowsProcess::open(session)
    {
        Ok(p) => (
            Some(Box::new(p) as Box<dyn crate::platform::Process + Send + Sync>),
            None,
        ),
        Err(e) => {
            eprintln!("{e}");
            (None, Some(e))
        }
    };
    let server = crate::protocol::Server {
        process,
        unavailable,
    };
    let served = crate::framing::serve(std::io::stdin().lock(), std::io::stdout().lock(), |m| {
        server.handle(m)
    });
    drop(server);
    close_game(pid, &mut child);
    served.map_err(Into::into)
}

/// Ask the game's windows to close (WM_CLOSE) and wait for the exit; never terminate it.
fn close_game(pid: u32, child: &mut std::process::Child) {
    #[link(name = "user32")]
    unsafe extern "system" {
        fn EnumWindows(callback: extern "system" fn(isize, isize) -> i32, data: isize) -> i32;
        fn GetWindowThreadProcessId(window: isize, pid: *mut u32) -> u32;
        fn PostMessageW(window: isize, message: u32, w: usize, l: isize) -> i32;
        fn IsWindowVisible(window: isize) -> i32;
    }
    extern "system" fn each(window: isize, data: isize) -> i32 {
        let mut owner = 0u32;
        // SAFETY: valid window handle from EnumWindows and a local output.
        unsafe { GetWindowThreadProcessId(window, &mut owner) };
        // SAFETY: `data` is the address of the target pid on the caller's stack.
        if owner == unsafe { *(data as *const u32) } && unsafe { IsWindowVisible(window) } != 0 {
            // SAFETY: posting WM_CLOSE to a window of the owned game only.
            unsafe { PostMessageW(window, 0x0010, 0, 0) };
        }
        1
    }
    if matches!(child.try_wait(), Ok(Some(_))) {
        return;
    }
    let target = pid;
    // SAFETY: the callback reads `target` only during this call.
    unsafe { EnumWindows(each, &target as *const u32 as isize) };
    let deadline = Instant::now() + Duration::from_secs(90);
    while Instant::now() < deadline {
        if matches!(child.try_wait(), Ok(Some(_))) {
            eprintln!("owned game exited");
            return;
        }
        std::thread::sleep(Duration::from_millis(500));
    }
    eprintln!("owned game still running after the close request; close it from its menu");
}

#[cfg(test)]
mod tests {
    use super::*;

    const ORIGINAL: &str = "{\n\t\"gameId\": \"hoi4\",\n\t\"gameDataPath\": \"%USER_DOCUMENTS%/Paradox Interactive/Hearts of Iron IV\",\n\t\"exePath\": \"./hoi4.exe\"\n}\n";

    #[test]
    fn redirect_replaces_only_the_user_folder() {
        let out = redirect_settings(ORIGINAL, r"C:\runs\ud").unwrap();
        assert_eq!(
            out,
            ORIGINAL.replace(
                "%USER_DOCUMENTS%/Paradox Interactive/Hearts of Iron IV",
                "C:/runs/ud"
            )
        );
    }

    #[test]
    fn redirect_refuses_unexpected_forms() {
        assert_eq!(
            redirect_settings("{\"x\": 1}", "C:/u").unwrap_err().code,
            "invalid_settings"
        );
        let twice = format!(
            "{{\"a\": {{\"gameDataPath\": \"p\"}}, \"gameDataPath\": \"p\"}}"
        );
        assert_eq!(
            redirect_settings(&twice, "C:/u").unwrap_err().code,
            "invalid_settings"
        );
    }

    #[test]
    fn steam_variables_are_dropped() {
        std::env::set_var("SteamAppId", "394360");
        assert!(launch_env().iter().all(|(k, _)| k != "SteamAppId"));
    }
}
