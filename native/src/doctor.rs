//! `hoi4-live-native --doctor`: will this tool be allowed to read the game's
//! memory on this computer, and if not, what is the one step that fixes it.
//!
//! The check reads system state only. It never changes a security setting,
//! never asks for a password and never touches a game process. The decision
//! for each operating system is a pure function of observed facts, so every
//! platform's logic is tested on every platform.

/// One prerequisite: whether it holds, and the exact step when it does not.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Check {
    pub ok: bool,
    pub what: String,
    pub fix: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Verdict {
    /// Memory access will work and stays working.
    Ready,
    /// Works now, but a step is recommended so it keeps working.
    ReadyForNow,
    /// A listed step is missing.
    NotReady,
    /// This runtime cannot read the game on this system at all.
    Unsupported,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Report {
    pub system: &'static str,
    pub verdict: Verdict,
    pub checks: Vec<Check>,
    pub note: Option<String>,
}

impl Report {
    pub fn usable(&self) -> bool {
        matches!(self.verdict, Verdict::Ready | Verdict::ReadyForNow)
    }
    pub fn render(&self) -> String {
        let headline = match self.verdict {
            Verdict::Ready => "memory access is ready; nothing to renew",
            Verdict::ReadyForNow => "memory access works for now; one step makes it permanent",
            Verdict::NotReady => "memory access is NOT ready",
            Verdict::Unsupported => "this runtime cannot read the game on this system",
        };
        let mut text = format!("{}: {headline}\n", self.system);
        for check in &self.checks {
            text.push_str(&format!(
                "  [{}] {}\n",
                if check.ok { "ok" } else { "missing" },
                check.what
            ));
            if let Some(fix) = &check.fix {
                text.push_str(&format!("        do this once: {fix}\n"));
            }
        }
        if let Some(note) = &self.note {
            text.push_str(&format!("  {note}\n"));
        }
        text
    }
}

fn ok(what: impl Into<String>) -> Check {
    Check {
        ok: true,
        what: what.into(),
        fix: None,
    }
}
fn missing(what: impl Into<String>, fix: impl Into<String>) -> Check {
    Check {
        ok: false,
        what: what.into(),
        fix: Some(fix.into()),
    }
}

/// What macOS shows about this binary and this user. `None` means the fact
/// could not be read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MacFacts {
    /// Path of the running binary, for the commands printed to the user.
    pub exe: String,
    /// The running binary carries `com.apple.security.cs.debugger`.
    pub entitled: bool,
    /// `debugger.entitlements` lies next to the binary.
    pub entitlements_beside: bool,
    /// The binary carries the download quarantine mark.
    pub quarantined: bool,
    pub developer_mode: Option<bool>,
    pub in_developer_group: Option<bool>,
    /// No-UI status of `system.privilege.taskport.debug`; 0 is held.
    pub debug_right: i32,
    /// No-UI status of `system.privilege.taskport`; 0 is held.
    pub plain_right: i32,
}

pub const ENABLE_DEVELOPER_MODE: &str = "sudo DevToolsSecurity -enable";
pub const JOIN_DEVELOPER_GROUP: &str =
    "sudo dseditgroup -o edit -a \"$USER\" -t user _developer   (then log out and back in)";

pub fn macos_report(facts: &MacFacts) -> Report {
    let mut checks = Vec::new();
    if facts.quarantined {
        checks.push(missing(
            "the binary still carries the download quarantine mark, so macOS may refuse to run it",
            format!("xattr -d com.apple.quarantine '{}'", facts.exe),
        ));
    }
    if facts.entitled {
        checks.push(ok("the tool is signed with the debugger entitlement"));
    } else {
        let sign = format!("'{}' --doctor --sign", facts.exe);
        checks.push(missing(
            "the tool is not signed with the debugger entitlement",
            if facts.entitlements_beside {
                sign
            } else {
                format!("copy debugger.entitlements (in the release archive, or native/macos/ in the repository) next to the binary, then run {sign}")
            },
        ));
    }
    match facts.developer_mode {
        Some(true) => checks.push(ok("Developer mode is on")),
        Some(false) => checks.push(missing("Developer mode is off", ENABLE_DEVELOPER_MODE)),
        None if facts.debug_right == 0 => {}
        None => checks.push(missing(
            "Developer mode could not be read",
            format!("check with `DevToolsSecurity -status`; if it is off: {ENABLE_DEVELOPER_MODE}"),
        )),
    }
    match facts.in_developer_group {
        Some(true) => checks.push(ok("your user is in the `_developer` group")),
        Some(false) => checks.push(missing(
            "your user is not in the `_developer` group",
            JOIN_DEVELOPER_GROUP,
        )),
        None if facts.debug_right == 0 => {}
        None => checks.push(missing(
            "membership of the `_developer` group could not be read",
            format!("check with `id -Gn`; if `_developer` is absent: {JOIN_DEVELOPER_GROUP}"),
        )),
    }
    let setup_done = facts.developer_mode != Some(false) && facts.in_developer_group != Some(false);
    if facts.debug_right == 0 {
        checks.push(ok(
            "macOS grants `system.privilege.taskport.debug` with no password and no expiry",
        ));
    } else if setup_done {
        // Both one-time steps read as done, yet the right is refused: the
        // usual cause is a login session older than the group change.
        checks.push(missing(
            format!(
                "macOS does not grant `system.privilege.taskport.debug` yet (status {})",
                facts.debug_right
            ),
            "log out and back in so the session picks up Developer mode and the group",
        ));
    }
    let (verdict, note) = if facts.entitled && facts.debug_right == 0 {
        (Verdict::Ready, None)
    } else if !facts.entitled && facts.plain_right == 0 {
        (
            Verdict::ReadyForNow,
            Some("An unsigned tool depends on `system.privilege.taskport`, which is held right now but lapses after about 10 hours and needs your password to renew. Sign the tool to stop depending on it.".to_owned()),
        )
    } else {
        (
            Verdict::NotReady,
            Some("The tool changes no security setting and asks for no password; the steps above are yours to run, once.".to_owned()),
        )
    };
    Report {
        system: "macOS",
        verdict,
        checks,
        note,
    }
}

/// What Linux shows. The runtime reads the game with `process_vm_readv` from
/// the launcher process that started it, as the same user.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LinuxFacts {
    /// `kernel.yama.ptrace_scope`; `None` when the kernel has no Yama.
    pub ptrace_scope: Option<u8>,
    pub root: bool,
}

pub fn linux_report(facts: &LinuxFacts) -> Report {
    let launcher = ok("start the game with this tool's launcher (`--serve`) as your own user: the tool reads only a game it started itself, and never one started by Steam or another account");
    let (verdict, check, note) = match facts.ptrace_scope {
        None => (
            Verdict::Ready,
            ok("the kernel places no extra limit on reading your own processes"),
            None,
        ),
        Some(0) => (
            Verdict::Ready,
            ok("`kernel.yama.ptrace_scope` is 0: a user may read their own processes"),
            None,
        ),
        Some(1) => (
            Verdict::Ready,
            ok("`kernel.yama.ptrace_scope` is 1 (the usual default): a process may read the processes it started, which is how the launcher reads the game"),
            None,
        ),
        Some(2) if facts.root => (
            Verdict::ReadyForNow,
            ok("`kernel.yama.ptrace_scope` is 2 (administrators only) and the tool runs as root"),
            Some("Running the tool as root is not recommended. Setting the value to 1 lets your ordinary user run it: sudo sysctl kernel.yama.ptrace_scope=1".to_owned()),
        ),
        Some(2) => (
            Verdict::NotReady,
            missing(
                "`kernel.yama.ptrace_scope` is 2: only administrators may read another process",
                "sudo sysctl kernel.yama.ptrace_scope=1   (to keep it after a restart, put `kernel.yama.ptrace_scope = 1` in a file under /etc/sysctl.d/)",
            ),
            Some("The tool never changes this setting itself.".to_owned()),
        ),
        Some(scope) => (
            Verdict::NotReady,
            missing(
                format!("`kernel.yama.ptrace_scope` is {scope}: reading another process is switched off until the next restart"),
                "set `kernel.yama.ptrace_scope = 1` in the file under /etc/sysctl.d/ (or the boot setting) that sets it to 3, then restart the computer",
            ),
            Some("The tool never changes this setting itself.".to_owned()),
        ),
    };
    Report {
        system: "Linux",
        verdict,
        checks: vec![check, launcher],
        note,
    }
}

/// Whether the native runtime has a verified Windows adapter in this build.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WindowsFacts {
    pub native_adapter_verified: bool,
}

pub fn windows_report(facts: &WindowsFacts) -> Report {
    let same_user = ok("run the game and your coding client as the same Windows user, both without \"Run as administrator\" (or both with it): Windows lets a program read a game of the same user at the same level with no extra permission");
    if facts.native_adapter_verified {
        return Report {
            system: "Windows",
            verdict: Verdict::Ready,
            checks: vec![same_user],
            note: None,
        };
    }
    Report {
        system: "Windows",
        verdict: Verdict::Unsupported,
        checks: vec![
            missing(
                "the native runtime's Windows adapter is unfinished and refuses to attach",
                "use the Windows package instead: unzip the release and run `python install.py` (the Python server is the supported Windows path)",
            ),
            same_user,
        ],
        note: Some("The Python server on Windows needs no setup beyond the line above.".to_owned()),
    }
}

/// Read this system's facts and decide.
pub fn report() -> Report {
    #[cfg(target_os = "macos")]
    {
        macos_report(&crate::platform::macos::memory_access_facts())
    }
    #[cfg(target_os = "linux")]
    {
        linux_report(&LinuxFacts {
            ptrace_scope: std::fs::read_to_string("/proc/sys/kernel/yama/ptrace_scope")
                .ok()
                .and_then(|text| text.trim().parse().ok()),
            // SAFETY: geteuid has no preconditions and cannot fail.
            root: unsafe { libc::geteuid() } == 0,
        })
    }
    #[cfg(not(any(target_os = "macos", target_os = "linux")))]
    {
        windows_report(&WindowsFacts {
            native_adapter_verified: false,
        })
    }
}

/// macOS: ad-hoc sign this binary with `debugger.entitlements` from its own
/// folder, then confirm the signature and the entitlement from a fresh read.
/// Needs no password and changes nothing outside the binary itself.
#[cfg(target_os = "macos")]
pub fn sign_self() -> Result<String, String> {
    use std::process::Command;
    let exe = std::env::current_exe().map_err(|e| format!("cannot locate this binary: {e}"))?;
    let entitlements = exe
        .parent()
        .map(|folder| folder.join("debugger.entitlements"))
        .filter(|path| path.is_file())
        .ok_or("debugger.entitlements is not next to the binary; copy it there from the release archive or native/macos/ and run this again")?;
    let run = |args: &[&std::ffi::OsStr]| {
        Command::new("/usr/bin/codesign")
            .args(args)
            .output()
            .map_err(|e| format!("cannot run codesign: {e}"))
    };
    let signed = run(&[
        "--force".as_ref(),
        "--sign".as_ref(),
        "-".as_ref(),
        "--entitlements".as_ref(),
        entitlements.as_os_str(),
        exe.as_os_str(),
    ])?;
    if !signed.status.success() {
        return Err(format!(
            "codesign refused: {}",
            String::from_utf8_lossy(&signed.stderr).trim()
        ));
    }
    let verified = run(&["--verify".as_ref(), "--strict".as_ref(), exe.as_os_str()])?;
    let shown = run(&[
        "--display".as_ref(),
        "--entitlements".as_ref(),
        "-".as_ref(),
        exe.as_os_str(),
    ])?;
    let listed = String::from_utf8_lossy(&shown.stdout).contains("com.apple.security.cs.debugger")
        || String::from_utf8_lossy(&shown.stderr).contains("com.apple.security.cs.debugger");
    if !verified.status.success() || !listed {
        return Err(
            "the binary was signed but the signature or the entitlement did not verify".into(),
        );
    }
    Ok(format!(
        "signed {} with the debugger entitlement and verified it",
        exe.display()
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mac() -> MacFacts {
        MacFacts {
            exe: "/opt/tool/hoi4-live-native".into(),
            entitled: true,
            entitlements_beside: true,
            quarantined: false,
            developer_mode: Some(true),
            in_developer_group: Some(true),
            debug_right: 0,
            plain_right: -60007,
        }
    }
    fn fixes(report: &Report) -> Vec<&str> {
        report
            .checks
            .iter()
            .filter_map(|check| check.fix.as_deref())
            .collect()
    }

    #[test]
    fn macos_signed_tool_with_setup_is_ready_whatever_the_plain_right_says() {
        let report = macos_report(&mac());
        assert_eq!(report.verdict, Verdict::Ready);
        assert!(fixes(&report).is_empty());
        assert!(report.render().contains("nothing to renew"));
    }

    #[test]
    fn macos_names_exactly_the_missing_one_time_step() {
        let report = macos_report(&MacFacts {
            developer_mode: Some(false),
            debug_right: -60007,
            ..mac()
        });
        assert_eq!(report.verdict, Verdict::NotReady);
        assert_eq!(fixes(&report), [ENABLE_DEVELOPER_MODE]);

        let report = macos_report(&MacFacts {
            in_developer_group: Some(false),
            debug_right: -60007,
            ..mac()
        });
        assert_eq!(report.verdict, Verdict::NotReady);
        assert_eq!(fixes(&report), [JOIN_DEVELOPER_GROUP]);

        let report = macos_report(&MacFacts {
            developer_mode: Some(false),
            in_developer_group: Some(false),
            debug_right: -60007,
            ..mac()
        });
        assert_eq!(
            fixes(&report),
            [ENABLE_DEVELOPER_MODE, JOIN_DEVELOPER_GROUP]
        );
    }

    #[test]
    fn macos_setup_done_but_right_refused_asks_for_a_fresh_login() {
        let report = macos_report(&MacFacts {
            debug_right: -60007,
            ..mac()
        });
        assert_eq!(report.verdict, Verdict::NotReady);
        assert_eq!(fixes(&report).len(), 1);
        assert!(fixes(&report)[0].contains("log out and back in"));
    }

    #[test]
    fn macos_unsigned_tool_is_told_to_sign_and_never_counts_the_debug_right() {
        let unsigned = MacFacts {
            entitled: false,
            ..mac()
        };
        let report = macos_report(&unsigned);
        assert_eq!(report.verdict, Verdict::NotReady);
        assert_eq!(
            fixes(&report)[0],
            "'/opt/tool/hoi4-live-native' --doctor --sign"
        );
        let report = macos_report(&MacFacts {
            entitlements_beside: false,
            ..unsigned.clone()
        });
        assert!(fixes(&report)[0].starts_with("copy debugger.entitlements"));

        let report = macos_report(&MacFacts {
            plain_right: 0,
            ..unsigned
        });
        assert_eq!(report.verdict, Verdict::ReadyForNow);
        assert!(report.usable());
        assert!(report.note.unwrap().contains("10 hours"));
    }

    #[test]
    fn macos_quarantine_is_reported_first_with_its_command() {
        let report = macos_report(&MacFacts {
            quarantined: true,
            ..mac()
        });
        assert_eq!(
            fixes(&report),
            ["xattr -d com.apple.quarantine '/opt/tool/hoi4-live-native'"]
        );
    }

    #[test]
    fn macos_unreadable_facts_do_not_block_a_granted_right() {
        let unknown = MacFacts {
            developer_mode: None,
            in_developer_group: None,
            ..mac()
        };
        assert_eq!(macos_report(&unknown).verdict, Verdict::Ready);
        assert!(fixes(&macos_report(&unknown)).is_empty());
        let report = macos_report(&MacFacts {
            debug_right: -60007,
            ..unknown
        });
        assert_eq!(report.verdict, Verdict::NotReady);
        assert_eq!(fixes(&report).len(), 3);
    }

    #[test]
    fn linux_scopes_zero_one_and_no_yama_are_ready() {
        for scope in [None, Some(0), Some(1)] {
            let report = linux_report(&LinuxFacts {
                ptrace_scope: scope,
                root: false,
            });
            assert_eq!(report.verdict, Verdict::Ready, "{scope:?}");
            assert!(fixes(&report).is_empty());
            assert!(report.render().contains("--serve"));
        }
    }

    #[test]
    fn linux_scope_two_needs_the_sysctl_unless_root() {
        let report = linux_report(&LinuxFacts {
            ptrace_scope: Some(2),
            root: false,
        });
        assert_eq!(report.verdict, Verdict::NotReady);
        assert!(fixes(&report)[0].starts_with("sudo sysctl kernel.yama.ptrace_scope=1"));
        let report = linux_report(&LinuxFacts {
            ptrace_scope: Some(2),
            root: true,
        });
        assert_eq!(report.verdict, Verdict::ReadyForNow);
    }

    #[test]
    fn linux_scope_three_needs_a_restart_even_for_root() {
        for root in [false, true] {
            let report = linux_report(&LinuxFacts {
                ptrace_scope: Some(3),
                root,
            });
            assert_eq!(report.verdict, Verdict::NotReady);
            assert!(fixes(&report)[0].contains("restart the computer"));
        }
    }

    #[test]
    fn windows_native_is_reported_unfinished_and_points_to_the_python_package() {
        let report = windows_report(&WindowsFacts {
            native_adapter_verified: false,
        });
        assert_eq!(report.verdict, Verdict::Unsupported);
        assert!(!report.usable());
        assert!(fixes(&report)[0].contains("python install.py"));
        assert!(report.render().contains("same Windows user"));
        let report = windows_report(&WindowsFacts {
            native_adapter_verified: true,
        });
        assert_eq!(report.verdict, Verdict::Ready);
    }

    #[test]
    fn render_lists_each_check_with_its_fix() {
        let text = macos_report(&MacFacts {
            developer_mode: Some(false),
            debug_right: -60007,
            ..mac()
        })
        .render();
        assert!(text.starts_with("macOS: memory access is NOT ready\n"));
        assert!(text.contains("  [missing] Developer mode is off\n        do this once: sudo DevToolsSecurity -enable\n"));
        assert!(text.contains("  [ok] the tool is signed with the debugger entitlement\n"));
    }
}
