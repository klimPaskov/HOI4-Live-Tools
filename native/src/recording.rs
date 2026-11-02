//! Bounded, window-only recording. Rust owns configuration, private files and
//! child lifetime; a bundled macOS bridge calls ScreenCaptureKit on its run loop.
//! This never acquires input, pauses, moves the cursor, or reads game memory.
use crate::error::{Error, Result};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Frame {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    pub output: PathBuf,
    pub window_id: u32,
    /// Exact OS window rectangle in logical points, including decoration.
    pub frame: Frame,
    /// Output is one pixel per point. Client geometry is qualified separately.
    pub fps: u32,
    pub max_seconds: u32,
    pub max_bytes: u64,
    pub min_free_bytes: u64,
}

impl Config {
    fn validate(&self) -> Result<()> {
        if !self.output.is_absolute()
            || self.output.file_name().is_none()
            || self
                .output
                .components()
                .any(|c| matches!(c, std::path::Component::ParentDir))
            || self.window_id == 0
            || !(1920..=8192).contains(&self.frame.width)
            || !(1080..=8192).contains(&self.frame.height)
            || self.frame.width % 2 != 0
            || self.frame.height % 2 != 0
            || !(24..=60).contains(&self.fps)
            || !(1..=21600).contains(&self.max_seconds)
            || !(1_000_000..=500_000_000_000).contains(&self.max_bytes)
            || !(1_000_000_000..=500_000_000_000).contains(&self.min_free_bytes)
        {
            return Err(Error::new(
                "invalid_recording",
                "Invalid path, geometry, frame rate or recording budget",
            ));
        }
        Ok(())
    }
}

fn read_bounded(path: &Path) -> Result<Vec<u8>> {
    use std::io::Read;
    let mut bytes = Vec::new();
    std::fs::File::open(path)?
        .take(65537)
        .read_to_end(&mut bytes)?;
    if bytes.len() > 65536 {
        return Err(Error::new(
            "invalid_recording",
            "Recording JSON exceeds bound",
        ));
    }
    Ok(bytes)
}

/// Run until the explicit stop file, duration, or a fail-closed guard ends capture.
pub fn run(session: &Path, config: &Path) -> Result<serde_json::Value> {
    let config: Config = serde_json::from_slice(&read_bounded(config)?)
        .map_err(|e| Error::new("invalid_recording", e.to_string()))?;
    config.validate()?;
    #[cfg(target_os = "macos")]
    {
        macos::run(session, config)
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = session;
        Err(Error::new(
            "platform_unavailable",
            "Window recording is currently available on macOS 15+ only",
        ))
    }
}

#[cfg(target_os = "macos")]
mod macos {
    use super::*;
    use crate::platform::{
        self,
        macos::{capture_identity, check_ownership, verify_executable, BUILD_UUID, SHA256},
    };
    use std::{
        fs::{self, File, OpenOptions},
        io::Write,
        os::unix::fs::{DirBuilderExt, MetadataExt, OpenOptionsExt},
        process::{Child, Command, Stdio},
        thread,
        time::{Duration, Instant},
    };

    struct OwnedChild(Child);
    impl Drop for OwnedChild {
        fn drop(&mut self) {
            // This handle is our unreaped child, never a PID taken from a file.
            if !matches!(self.0.try_wait(), Ok(Some(_))) {
                let _ = self.0.kill();
                let _ = self.0.wait();
            }
        }
    }

    fn new_file(path: &Path, bytes: &[u8], mode: u32) -> Result<File> {
        let mut f = OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(mode)
            .open(path)?;
        f.write_all(bytes)?;
        f.sync_all()?;
        Ok(f)
    }

    pub(super) fn run(session_path: &Path, mut config: Config) -> Result<serde_json::Value> {
        let session: platform::Session = serde_json::from_slice(&read_bounded(session_path)?)
            .map_err(|e| Error::new("invalid_session", e.to_string()))?;
        if session.build_id != BUILD_UUID
            || session.sha256 != SHA256
            || session.translated.is_some()
        {
            return Err(Error::new(
                "unsupported_build",
                "Recorder requires the inspected owned native Mac session",
            ));
        }
        check_ownership(&session)?;
        verify_executable(&session.game.exe)?;
        let parent = config
            .output
            .parent()
            .ok_or_else(|| Error::new("invalid_recording", "Output parent missing"))?
            .canonicalize()?;
        // A fresh, private run directory; never replace previous video/evidence.
        config.output = parent.join(config.output.file_name().unwrap());
        fs::DirBuilder::new().mode(0o700).create(&config.output)?;
        let meta = fs::symlink_metadata(&config.output)?;
        if !meta.is_dir() || meta.mode() & 0o077 != 0 || meta.uid() != session.game.uid {
            return Err(Error::new(
                "permission_denied",
                "Recorder directory is not private and owned",
            ));
        }
        let owner = capture_identity(std::process::id() as i32)?;
        let envelope =
            serde_json::json!({"session": session, "recorder_owner": owner, "config": config});
        new_file(
            &config.output.join("request.json"),
            &serde_json::to_vec_pretty(&envelope)
                .map_err(|e| Error::new("invalid_recording", e.to_string()))?,
            0o600,
        )?;
        let helper = config.output.join("window-recorder");
        new_file(
            &helper,
            include_bytes!(concat!(env!("OUT_DIR"), "/hoi4-window-recorder")),
            0o700,
        )?;
        let log = new_file(&config.output.join("bridge.log"), &[], 0o600)?;
        // Revalidate after hashing/extraction, before capture starts. No task port.
        check_ownership(&session)?;
        let mut child = OwnedChild(
            Command::new(helper)
                .arg(config.output.join("request.json"))
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(log)
                .spawn()?,
        );
        let deadline = Instant::now() + Duration::from_secs(u64::from(config.max_seconds) + 25);
        let status = loop {
            if let Some(status) = child.0.try_wait()? {
                break status;
            }
            if Instant::now() >= deadline {
                return Err(Error::new(
                    "recording_timeout",
                    "Recorder exceeded finalization deadline; partial evidence preserved",
                ));
            }
            thread::sleep(Duration::from_millis(100));
        };
        let result: serde_json::Value =
            serde_json::from_slice(&read_bounded(&config.output.join("result.json"))?)
                .map_err(|e| Error::new("recording_failed", e.to_string()))?;
        if !status.success()
            || result["status"] != "complete"
            || result["recording_started"] != true
            || result["frames_complete"].as_u64().unwrap_or(0) == 0
            || result["recorded_seconds"].as_f64().unwrap_or(0.0) <= 0.0
        {
            return Err(Error::new(
                "recording_failed",
                format!(
                    "Capture did not complete; inspect {}",
                    config.output.join("result.json").display()
                ),
            ));
        }
        Ok(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[cfg(target_os = "macos")]
    #[test]
    fn native_frame_status_policy_distinguishes_idle_and_capture_loss() {
        // Runs only the pure bridge predicate; no capture, TCC or desktop access.
        assert!(
            std::process::Command::new(concat!(env!("OUT_DIR"), "/hoi4-window-recorder"))
                .arg("--self-test-frame-status")
                .status()
                .unwrap()
                .success()
        );
    }
    fn config() -> Config {
        Config {
            output: std::env::temp_dir().join("recording-fixture"),
            window_id: 42,
            frame: Frame {
                x: -1920,
                y: 53,
                width: 1920,
                height: 1108,
            },
            fps: 30,
            max_seconds: 7200,
            max_bytes: 20_000_000_000,
            min_free_bytes: 4_000_000_000,
        }
    }
    #[test]
    fn recording_contract_rejects_unbounded_or_nonfilmable_configuration() {
        assert!(config().validate().is_ok());
        for change in [
            |c: &mut Config| c.output = PathBuf::from("relative"),
            |c: &mut Config| c.output = std::env::temp_dir().join("../other"),
            |c: &mut Config| c.window_id = 0,
            |c: &mut Config| c.frame.width = 1918,
            |c: &mut Config| c.frame.height = 1078,
            |c: &mut Config| c.frame.width = 1921,
            |c: &mut Config| c.frame.height = 8194,
            |c: &mut Config| c.max_seconds = 21601,
            |c: &mut Config| c.max_seconds = 0,
            |c: &mut Config| c.fps = 61,
            |c: &mut Config| c.max_bytes = u64::MAX,
            |c: &mut Config| c.min_free_bytes = 0,
        ] {
            let mut c = config();
            change(&mut c);
            assert!(c.validate().is_err());
        }
    }
    #[test]
    fn recording_contract_rejects_unrecognized_capture_options() {
        let mut json = serde_json::to_value(config()).unwrap();
        json["display_capture"] = true.into();
        assert!(serde_json::from_value::<Config>(json).is_err());
        let mut json = serde_json::to_value(config()).unwrap();
        json["frame"]["scale"] = 2.into();
        assert!(serde_json::from_value::<Config>(json).is_err());
    }
}
