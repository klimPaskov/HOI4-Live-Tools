//! Steam must never see or register a run. The launcher refuses while a Steam
//! client, its web helper or steamcmd runs (on Linux, and on the Windows host
//! when the game runs under WSL), clears the environment (no Steam variables
//! reach the game) and gives native launches an empty private HOME so the
//! Steam API cannot find a client pipe even if Steam starts later.
use super::*;

/// Process names (Linux comm/argv0/exe basename, or Windows image names
/// without `.exe`) that block a launch.
const BLOCKING: &[&str] = &[
    "steam",
    "steamcmd",
    "steamwebhelper",
    "steam.sh",
    "steamcmd.sh",
];

/// Opt-out for WSL hosts whose Windows interop is disabled by design and whose
/// harness checks the Windows side itself. Read from the server environment.
pub(super) const WINDOWS_CHECK_ENV: &str = "HOI4_LIVE_WINDOWS_STEAM_CHECK";

pub(super) fn blocking_name(name: &str) -> bool {
    let base = name.rsplit(['/', '\\']).next().unwrap_or(name).trim();
    let lower = base.to_ascii_lowercase();
    let stem = lower.strip_suffix(".exe").unwrap_or(&lower);
    BLOCKING.contains(&stem)
}

fn refusal(name: &str, where_: &str) -> Error {
    Error::new(
        "steam_running",
        format!(
            "{name} is running on the {where_}; close Steam and steamcmd before launching \
             (the game is never started while Steam can see it)"
        ),
    )
}

/// Every visible process on this Linux host, by comm, argv0 and exe basename.
pub(super) fn check_linux(proc_root: &Path) -> Result<()> {
    for entry in fs::read_dir(proc_root)? {
        let entry = entry?;
        if entry.file_name().to_string_lossy().parse::<i32>().is_err() {
            continue;
        }
        let dir = entry.path();
        let mut names = Vec::new();
        if let Ok(comm) = fs::read_to_string(dir.join("comm")) {
            names.push(comm.trim().to_string());
        }
        if let Ok(cmdline) = fs::read(dir.join("cmdline")) {
            if let Some(argv0) = cmdline.split(|b| *b == 0).next() {
                names.push(String::from_utf8_lossy(argv0).into_owned());
            }
        }
        if let Ok(exe) = fs::read_link(dir.join("exe")) {
            names.push(exe.to_string_lossy().into_owned());
        }
        if let Some(name) = names.iter().find(|n| blocking_name(n)) {
            return Err(refusal(name, "Linux host"));
        }
    }
    Ok(())
}

pub(super) fn is_wsl(proc_root: &Path) -> bool {
    proc_root.join("sys/fs/binfmt_misc/WSLInterop").exists()
        || fs::read_to_string(proc_root.join("version"))
            .is_ok_and(|v| v.to_ascii_lowercase().contains("microsoft"))
}

/// Image names from `tasklist /FO CSV /NH` output.
pub(super) fn tasklist_names(output: &str) -> Vec<String> {
    output
        .lines()
        .filter_map(|line| line.trim().strip_prefix('"'))
        .filter_map(|rest| rest.split('"').next())
        .map(str::to_string)
        .collect()
}

fn check_windows_host() -> Result<()> {
    if std::env::var(WINDOWS_CHECK_ENV).as_deref() == Ok("skip") {
        return Ok(());
    }
    for tool in ["/mnt/c/Windows/System32/tasklist.exe", "tasklist.exe"] {
        let Ok(out) = Command::new(tool)
            .args(["/FO", "CSV", "/NH"])
            .stdin(Stdio::null())
            .stderr(Stdio::null())
            .output()
        else {
            continue;
        };
        if !out.status.success() {
            continue;
        }
        let text = String::from_utf8_lossy(&out.stdout);
        if let Some(name) = tasklist_names(&text).iter().find(|n| blocking_name(n)) {
            return Err(refusal(name, "Windows host"));
        }
        return Ok(());
    }
    Err(Error::new(
        "steam_check_unavailable",
        format!(
            "WSL detected but the Windows process list is unreadable (tasklist.exe); \
             enable interop, or set {WINDOWS_CHECK_ENV}=skip when the harness verifies \
             the Windows host itself"
        ),
    ))
}

pub(super) fn refuse_running_steam() -> Result<()> {
    let proc_root = Path::new("/proc");
    check_linux(proc_root)?;
    if is_wsl(proc_root) {
        check_windows_host()?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names() {
        for n in [
            "steam",
            "steamcmd",
            "steamwebhelper",
            "/home/u/.steam/ubuntu12_32/steam",
            "/home/u/steamcmd/linux32/steamcmd",
            "steam.sh",
            "Steam.exe",
            "steamcmd.exe",
            "C:\\Program Files (x86)\\Steam\\steamwebhelper.exe",
        ] {
            assert!(blocking_name(n), "{n}");
        }
        for n in [
            "hoi4",
            "steam-runtime-tools",
            "ipcserver",
            "bash",
            "steamy",
            "x.steam",
        ] {
            assert!(!blocking_name(n), "{n}");
        }
    }

    #[test]
    fn tasklist() {
        let out = "\"System Idle Process\",\"0\",\"Services\",\"0\",\"8 K\"\r\n\
                   \"steam.exe\",\"4242\",\"Console\",\"1\",\"90,000 K\"\r\n";
        let names = tasklist_names(out);
        assert_eq!(names, ["System Idle Process", "steam.exe"]);
        assert!(names.iter().any(|n| blocking_name(n)));
    }

    fn fake_proc(entries: &[(&str, &str, &[u8])]) -> PathBuf {
        let root = std::env::temp_dir().join(format!(
            "steam-guard-{}-{}",
            std::process::id(),
            entries.len()
        ));
        let _ = fs::remove_dir_all(&root);
        for (pid, comm, cmdline) in entries {
            let d = root.join(pid);
            fs::create_dir_all(&d).unwrap();
            fs::write(d.join("comm"), format!("{comm}\n")).unwrap();
            fs::write(d.join("cmdline"), cmdline).unwrap();
        }
        fs::create_dir_all(root.join("self")).unwrap();
        root
    }

    #[test]
    fn linux_scan() {
        let clean = fake_proc(&[("1", "systemd", b"/sbin/init\0"), ("77", "bash", b"bash\0")]);
        assert!(check_linux(&clean).is_ok());
        assert!(!is_wsl(&clean));
        let busy = fake_proc(&[
            ("1", "systemd", b"/sbin/init\0"),
            (
                "90",
                "ld-linux.so.2",
                b"/home/u/steamcmd/linux32/steamcmd\0+login\0",
            ),
            ("91", "bash", b"bash\0"),
        ]);
        let err = check_linux(&busy).unwrap_err();
        assert!(format!("{err:?}").contains("steam_running"));
        let _ = fs::remove_dir_all(clean);
        let _ = fs::remove_dir_all(busy);
    }
}
