//! The game builds this runtime can read, in one place. The build gate's
//! refusal and `--supported-builds` both speak from this table, so a user whose
//! game was updated is told which builds work and what to use meanwhile,
//! instead of a bare mismatch.

use serde_json::{json, Value};

/// One game executable the runtime's address tables were made for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Build {
    /// `macos` or `linux`, as `std::env::consts::OS` spells it.
    pub system: &'static str,
    pub label: &'static str,
    pub game_version: &'static str,
    /// Mach-O UUID or GNU build ID of the executable.
    pub build_id: &'static str,
    pub sha256: &'static str,
}

pub const SUPPORTED: [Build; 2] = [
    Build {
        system: "macos",
        label: "macOS (the x86_64 game, under Rosetta on Apple silicon)",
        game_version: "1.19.4.1",
        build_id: "DFA875A5-25BA-35E0-9736-627F8A5C4396",
        sha256: "d440ad8823e1ca5106b2ff87bc5135acf1b40d56f6bf0a76b52b5b247a668e07",
    },
    Build {
        system: "linux",
        label: "Linux x86_64",
        game_version: "1.19.4.1",
        build_id: crate::game::BUILD_ID,
        sha256: crate::game::SHA256,
    },
];

pub const WINDOWS: &str =
    "the native runtime's Windows adapter is unfinished; on Windows the Python server is the supported path and has no build check";

/// What works while the installed game is a build this runtime cannot read.
pub const MEANWHILE: &str = "Until a release supports your build: (1) `hoi4-live update` installs a newer HOI4 Live Tools when one exists; (2) Steam can keep the game on an earlier version (Properties > Betas) when Paradox offers it; (3) the Python server (hoi4-live-tools) reads the screen instead of memory and has no build check: full on Windows, limited on macOS, not available on Linux.";

fn list() -> String {
    SUPPORTED
        .iter()
        .map(|b| {
            format!(
                "HOI4 {} on {} (build {})",
                b.game_version, b.label, b.build_id
            )
        })
        .collect::<Vec<_>>()
        .join("; ")
}

/// The refusal for a game executable that is not a supported build. `found`
/// says what was seen, in a few words.
pub fn refusal(found: &str) -> String {
    format!(
        "This HOI4 build is not one that hoi4-live-native {} can read ({found}). It reads: {}. {MEANWHILE}",
        env!("CARGO_PKG_VERSION"),
        list()
    )
}

/// `--supported-builds`: the table as JSON, for the installer's doctor.
pub fn report() -> Value {
    json!({
        "runtime": env!("CARGO_PKG_VERSION"),
        "system": std::env::consts::OS,
        "builds": SUPPORTED.iter().map(|b| json!({
            "system": b.system,
            "label": b.label,
            "game_version": b.game_version,
            "build_id": b.build_id,
            "sha256": b.sha256,
        })).collect::<Vec<_>>(),
        "windows": WINDOWS,
        "meanwhile": MEANWHILE,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn refusal_names_every_supported_build_and_what_works_meanwhile() {
        let text = refusal("SHA-256 0123456789abcdef");
        assert!(text.contains("SHA-256 0123456789abcdef"));
        for build in &SUPPORTED {
            assert!(text.contains(build.game_version));
            assert!(text.contains(build.build_id));
        }
        assert!(text.contains("hoi4-live update"));
        assert!(text.contains("Python server"));
        assert!(text.contains(env!("CARGO_PKG_VERSION")));
    }

    #[test]
    fn report_lists_one_build_per_supported_system_with_full_hashes() {
        let report = report();
        let builds = report["builds"].as_array().unwrap();
        assert_eq!(builds.len(), SUPPORTED.len());
        for build in builds {
            assert_eq!(build["sha256"].as_str().unwrap().len(), 64);
            assert!(!build["build_id"].as_str().unwrap().is_empty());
        }
        let systems: Vec<_> = builds
            .iter()
            .map(|b| b["system"].as_str().unwrap())
            .collect();
        assert_eq!(systems, ["macos", "linux"]);
        assert!(report["windows"]
            .as_str()
            .unwrap()
            .contains("Python server"));
        assert_eq!(report["system"], std::env::consts::OS);
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn macos_entry_is_the_build_the_reader_accepts() {
        assert_eq!(SUPPORTED[0].build_id, crate::platform::macos::BUILD_UUID);
        assert_eq!(SUPPORTED[0].sha256, crate::platform::macos::SHA256);
    }
}
