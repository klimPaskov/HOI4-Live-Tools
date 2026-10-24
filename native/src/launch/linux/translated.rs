//! Explicit FEX launch selection. The kernel process remains the owned child;
//! the verified x86 guest image is a separate, continuously checked identity.
use super::*;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct FexLaunch {
    pub executable: PathBuf,
    pub sha256: String,
    #[serde(default)]
    pub config_directory: Option<fex_config::Spec>,
}

/// Explicit owner-only validation route, not a claim of campaign acceptance.
#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
pub(super) enum InputProfile {
    #[serde(rename = "fex_2610_x11_1920x1080_validation_v1")]
    Fex2610X11FullHdValidationV1,
}

pub(super) fn require_input_qualification(session: &Session, binding: &Value) -> Result<()> {
    let profile = binding
        .get("translated_input_profile")
        .filter(|v| !v.is_null());
    let Some(image) = &session.translated else {
        if profile.is_some() {
            return Err(Error::new(
                "invalid_input_binding",
                "Translated input profile requires an owned translated session",
            ));
        }
        return Ok(());
    };
    if image.translator_sha256 != platform::FEX_SHA256
        || session.sha256 != crate::game::SHA256
        || session.build_id != crate::game::BUILD_ID
        || profile
            .and_then(|v| serde_json::from_value::<InputProfile>(v.clone()).ok())
            .is_none()
    {
        return Err(Error::new(
            "input_unavailable",
            "Translated input requires the pinned FEX/game builds and explicit 1080p validation profile; campaign acceptance remains unverified",
        ));
    }
    Ok(())
}

pub(super) fn launch_image(game: &Path, fex: Option<&FexLaunch>) -> Result<PathBuf> {
    let Some(fex) = fex else {
        return Ok(game.to_path_buf());
    };
    if !fex.executable.is_absolute() {
        return Err(Error::new(
            "invalid_session",
            "FEX executable must be absolute",
        ));
    }
    let translator = fex.executable.canonicalize()?;
    platform::verify_translator(&translator, &fex.sha256)?;
    Ok(translator)
}

pub(super) fn await_guest(child: &mut OwnedChild, game: &Path) -> Result<Instant> {
    let deadline = Instant::now() + Duration::from_secs(10);
    await_guest_observed(child, game, deadline, platform::guest_mapping_present)?;
    Ok(deadline)
}

pub(super) fn open_reader(
    child: &mut OwnedChild,
    manifest: &Path,
    deadline: Instant,
) -> Result<Box<dyn platform::Process + Send + Sync>> {
    capture_reader(child, deadline, || platform::open(manifest))
}

fn capture_reader<T>(
    child: &mut OwnedChild,
    deadline: Instant,
    mut capture: impl FnMut() -> Result<T>,
) -> Result<T> {
    let mut anchor = None;
    loop {
        child.readiness_anchor_checked(&mut anchor, deadline)?;
        let identity = child.identity.as_ref().ok_or_else(|| {
            Error::new(
                "ownership_changed",
                "Translator identity has not been pinned",
            )
        })?;
        verify_identity(identity)?;
        let result = capture();
        verify_identity(identity)?;
        child.readiness_anchor_checked(&mut anchor, deadline)?;
        match result {
            Ok(reader) => return Ok(reader),
            Err(error) if error.code == "translated_image_loading" => {}
            Err(error) => return Err(error),
        }
        thread::sleep(
            Duration::from_millis(10).min(deadline.saturating_duration_since(Instant::now())),
        );
    }
}

fn await_guest_observed(
    child: &mut OwnedChild,
    game: &Path,
    deadline: Instant,
    mut present: impl FnMut(i32, &Path) -> Result<bool>,
) -> Result<()> {
    let mut anchor = None;
    loop {
        child.readiness_anchor_checked(&mut anchor, deadline)?;
        let identity = child.identity.as_ref().ok_or_else(|| {
            Error::new(
                "ownership_changed",
                "Translator identity has not been pinned",
            )
        })?;
        verify_identity(identity)?;
        let mapped = present(identity.pid, game)?;
        verify_identity(identity)?;
        child.readiness_anchor_checked(&mut anchor, deadline)?;
        if mapped {
            return Ok(());
        }
        thread::sleep(
            Duration::from_millis(10).min(deadline.saturating_duration_since(Instant::now())),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn input_validation_is_explicit_and_bound_to_both_reviewed_builds() {
        let identity = Identity {
            pid: 123,
            start_ticks: "456".into(),
            exe: "/fixture/FEX".into(),
            uid: 1000,
        };
        let mut session = Session {
            game: identity.clone(),
            supervisor: identity,
            build_id: crate::game::BUILD_ID.into(),
            sha256: crate::game::SHA256.into(),
            translated: Some(platform::TranslatedImage {
                executable: "/fixture/hoi4".into(),
                translator_sha256: platform::FEX_SHA256.into(),
            }),
        };
        let opt_in = json!({"translated_input_profile":"fex_2610_x11_1920x1080_validation_v1"});
        assert!(require_input_qualification(&session, &opt_in).is_ok());
        for invalid in [
            Value::Null,
            json!({}),
            json!({"translated_input_profile":null}),
            json!({"translated_input_profile":true}),
            json!({"translated_input_profile":"unknown"}),
        ] {
            assert_eq!(
                require_input_qualification(&session, &invalid)
                    .unwrap_err()
                    .code,
                "input_unavailable"
            );
        }
        let qualified = session.clone();
        session.sha256 = "unknown".into();
        assert!(require_input_qualification(&session, &opt_in).is_err());
        session = qualified.clone();
        session.build_id = "unknown".into();
        assert!(require_input_qualification(&session, &opt_in).is_err());
        session = qualified;
        session.translated.as_mut().unwrap().translator_sha256 = "unknown".into();
        assert!(require_input_qualification(&session, &opt_in).is_err());
        session.translated = None;
        assert_eq!(
            require_input_qualification(&session, &opt_in)
                .unwrap_err()
                .code,
            "invalid_input_binding"
        );
        assert!(require_input_qualification(&session, &json!({})).is_ok());
    }

    #[test]
    fn absent_translation_preserves_native_launch_selection() {
        let game = Path::new("/generated/game");
        assert_eq!(launch_image(game, None).unwrap(), game);
    }

    #[test]
    fn translator_requires_explicit_absolute_path_and_known_hash() {
        let relative = FexLaunch {
            executable: "FEX".into(),
            sha256: "a".repeat(64),
            config_directory: None,
        };
        assert_eq!(
            launch_image(Path::new("/game"), Some(&relative))
                .unwrap_err()
                .code,
            "invalid_session"
        );
        let unknown = FexLaunch {
            executable: std::env::current_exe().unwrap(),
            sha256: "a".repeat(64),
            config_directory: None,
        };
        assert!(launch_image(Path::new("/game"), Some(&unknown)).is_err());
        assert!(serde_json::from_value::<FexLaunch>(
            json!({"executable":"/FEX","sha256":"unknown","args":["unreviewed"]})
        )
        .is_err());
    }

    fn child() -> OwnedChild {
        let mut command = Command::new("/bin/sleep");
        command
            .arg("30")
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        let mut child = OwnedChild::spawn(&mut command).unwrap();
        child.identity = Some(capture_identity(child.child.id() as i32).unwrap());
        child
    }

    #[test]
    fn guest_readiness_waits_for_mapping_without_relaunching() {
        let mut child = child();
        let pid = child.child.id();
        let game = Path::new("/generated/game");
        let mut reads = 0;
        await_guest_observed(
            &mut child,
            game,
            Instant::now() + Duration::from_secs(5),
            |observed, path| {
                assert_eq!(observed as u32, pid);
                assert_eq!(path, game);
                reads += 1;
                Ok(reads == 3)
            },
        )
        .unwrap();
        assert_eq!(reads, 3);
        assert_eq!(child.child.id(), pid);
    }

    #[test]
    fn guest_readiness_never_accepts_late_mapping_or_retries_errors() {
        let mut child = child();
        let error = await_guest_observed(&mut child, Path::new("/game"), Instant::now(), |_, _| {
            panic!("Expired wait must not read")
        });
        assert_eq!(error.unwrap_err().code, "launch_not_ready");
        let mut reads = 0;
        let error = await_guest_observed(
            &mut child,
            Path::new("/game"),
            Instant::now() + Duration::from_secs(5),
            |_, _| {
                reads += 1;
                Err(Error::new("invalid_maps", "original fixture refusal"))
            },
        );
        assert_eq!(error.unwrap_err().code, "invalid_maps");
        assert_eq!(reads, 1);
    }

    #[test]
    fn reader_capture_retries_only_incomplete_initial_guest_loads() {
        let mut child = child();
        let mut attempts = 0;
        let reader = capture_reader(&mut child, Instant::now() + Duration::from_secs(5), || {
            attempts += 1;
            if attempts < 3 {
                Err(Error::new(
                    "translated_image_loading",
                    "missing fixture PT_LOAD",
                ))
            } else {
                Ok("complete reader")
            }
        })
        .unwrap();
        assert_eq!(reader, "complete reader");
        assert_eq!(attempts, 3);
        for code in [
            "invalid_maps",
            "ownership_changed",
            "unsupported_build",
            "permission_denied",
        ] {
            let mut attempts = 0;
            let refused: Result<()> =
                capture_reader(&mut child, Instant::now() + Duration::from_secs(5), || {
                    attempts += 1;
                    Err(Error::new(code, "original fixture refusal"))
                });
            assert_eq!(refused.unwrap_err().code, code);
            assert_eq!(attempts, 1);
        }
    }

    #[test]
    fn complete_reader_after_original_deadline_is_not_published() {
        let mut child = child();
        let deadline = Instant::now() + Duration::from_millis(100);
        let result = capture_reader(&mut child, deadline, || {
            thread::sleep(
                deadline.saturating_duration_since(Instant::now()) + Duration::from_millis(1),
            );
            Ok("late reader")
        });
        assert_eq!(result.unwrap_err().code, "launch_not_ready");
    }

    #[test]
    #[ignore = "requires owned ARM64/FEX fixture"]
    fn actual_fex_child_readiness_and_duplicate_detection() {
        use std::io::BufRead;
        let translator = PathBuf::from(
            std::env::var_os("HOI4_FEX_TEST_TRANSLATOR").expect("Explicit FEX fixture path"),
        )
        .canonicalize()
        .unwrap();
        let guest = PathBuf::from(
            std::env::var_os("HOI4_FEX_TEST_GUEST").expect("Explicit guest fixture path"),
        )
        .canonicalize()
        .unwrap();
        assert!(platform::is_reviewed_translator(&translator).unwrap());
        let mut command = Command::new(&translator);
        command
            .arg(&guest)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit());
        let mut child = OwnedChild::spawn(&mut command).unwrap();
        child.identity = Some(capture_identity(child.child.id() as i32).unwrap());
        let output = child.child.stdout.take().unwrap();
        let (tx, rx) = mpsc::channel();
        let output_thread = thread::spawn(move || {
            let mut line = String::new();
            std::io::BufReader::new(output)
                .read_line(&mut line)
                .unwrap();
            tx.send(line).unwrap();
        });
        let ready: Value =
            serde_json::from_str(&rx.recv_timeout(Duration::from_secs(20)).unwrap()).unwrap();
        assert_eq!(ready["ready"], true);
        output_thread.join().unwrap();
        await_guest(&mut child, &guest).unwrap();
        assert_eq!(
            duplicate_game(&guest).unwrap_err().code,
            "game_already_running"
        );
        let other = std::env::temp_dir().join(format!("hoi4-unmapped-{}", nonce().unwrap()));
        write_new(&other, b"Unmapped generated game identity fixture").unwrap();
        let unrelated = duplicate_game(&other);
        fs::remove_file(&other).unwrap();
        unrelated.unwrap();
        child
            .child
            .stdin
            .as_mut()
            .unwrap()
            .write_all(b"quit\n")
            .unwrap();
        child.child.stdin.as_mut().unwrap().flush().unwrap();
        assert!(child.child.wait().unwrap().success());
        child.exited = true;
    }
}
