use super::*;
use std::os::unix::fs::symlink;

struct Fixture {
    parent: PathBuf,
    spec: Spec,
}
impl Fixture {
    fn new(gl: u8, unified: bool) -> Self {
        assert_ne!(uid(), 0, "FEX profile fixtures require an ordinary user");
        let parent = std::env::temp_dir().join(format!("hoi4-fex-profile-{}", nonce().unwrap()));
        fs::DirBuilder::new().mode(0o700).create(&parent).unwrap();
        // /tmp may itself be a symlink; only supply the canonical fixture path.
        let parent = parent.canonicalize().unwrap();
        let directory = parent.join("profile");
        fs::DirBuilder::new()
            .mode(0o700)
            .create(&directory)
            .unwrap();
        fs::DirBuilder::new()
            .mode(0o700)
            .create(directory.join("AppConfig"))
            .unwrap();
        let config = serde_json::to_vec(
            &json!({"Config":{"RootFS":"/fixture/rootfs.sqsh"},"ThunksDB":{"GL":gl}}),
        )
        .unwrap();
        let overlay: Vec<_> = LEGACY_GL
            .iter()
            .chain(if unified { SPLIT_GL.iter() } else { [].iter() })
            .copied()
            .collect();
        let db = serde_json::to_vec(&json!({"DB":{
            "GL":{"Library":"libGL-guest.so","Overlay":overlay},
            "Vulkan":{"Library":"libvulkan-guest.so","Overlay":["@PREFIX_LIB@/libvulkan.so.1"]}
        }}))
        .unwrap();
        write_new(&directory.join(FILES[0]), &config).unwrap();
        write_new(&directory.join(FILES[1]), &db).unwrap();
        Self {
            spec: Spec {
                directory,
                config_sha256: format!("{:x}", Sha256::digest(&config)),
                thunks_db_sha256: format!("{:x}", Sha256::digest(&db)),
            },
            parent,
        }
    }
    fn replace(&mut self, name: &str, bytes: &[u8]) {
        fs::remove_file(self.spec.directory.join(name)).unwrap();
        write_new(&self.spec.directory.join(name), bytes).unwrap();
        let digest = format!("{:x}", Sha256::digest(bytes));
        match name {
            "Config.json" => self.spec.config_sha256 = digest,
            "ThunksDB.json" => self.spec.thunks_db_sha256 = digest,
            _ => panic!("Unknown fixture input"),
        }
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.parent).unwrap();
    }
}

#[test]
fn explicit_profiles_configure_only_the_pinned_child_environment() {
    for (gl, unified) in [(0, false), (1, false), (1, true)] {
        let fixture = Fixture::new(gl, unified);
        let guard = Guard::capture(&fixture.spec).unwrap();
        let mut command = Command::new("/usr/bin/env");
        command.env_clear();
        guard.configure(&mut command).unwrap();
        let output = command.output().unwrap();
        assert!(output.status.success());
        assert_eq!(
            String::from_utf8(output.stdout).unwrap(),
            format!(
                "FEX_APP_CONFIG_LOCATION={}/\n",
                fixture.spec.directory.display()
            )
        );
        guard.verify().unwrap();
    }
}

#[test]
fn absent_profile_preserves_old_launch_schema_and_arbitrary_environment_is_rejected() {
    let fex: translated::FexLaunch =
        serde_json::from_value(json!({"executable":"/FEX","sha256":"pin"})).unwrap();
    assert!(fex.config_directory.is_none());
    for config in [
        json!({"directory":"/profile"}),
        json!({"directory":"/profile","config_sha256":"a","thunks_db_sha256":"b","env":{"LD_PRELOAD":"anything"}}),
    ] {
        assert!(serde_json::from_value::<Spec>(config).is_err());
    }
}

#[test]
fn missing_appconfig_is_refused_without_repair_or_environment_change() {
    let fixture = Fixture::new(1, true);
    fs::remove_dir(fixture.spec.directory.join("AppConfig")).unwrap();
    assert!(Guard::capture(&fixture.spec).is_err());
    assert!(!fixture.spec.directory.join("AppConfig").exists());
}

#[test]
fn hash_pin_and_size_bound_apply_before_deserializing_a_profile() {
    let mut fixture = Fixture::new(1, true);
    fixture.spec.config_sha256 = "0".repeat(64);
    assert!(Guard::capture(&fixture.spec).is_err());
    fixture.replace(FILES[0], &vec![b' '; 65537]);
    assert!(Guard::capture(&fixture.spec).is_err());
}

#[test]
fn private_nonsymlink_entries_are_required() {
    let fixture = Fixture::new(1, true);
    let path = fixture.spec.directory.join(FILES[0]);
    let original = fixture.parent.join("original.json");
    fs::rename(&path, &original).unwrap();
    symlink(&original, &path).unwrap();
    assert!(Guard::capture(&fixture.spec).is_err());
    fs::remove_file(&path).unwrap();
    fs::rename(&original, &path).unwrap();
    fs::set_permissions(&path, fs::Permissions::from_mode(0o644)).unwrap();
    assert!(Guard::capture(&fixture.spec).is_err());
    fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).unwrap();
    fs::set_permissions(&fixture.spec.directory, fs::Permissions::from_mode(0o755)).unwrap();
    assert!(Guard::capture(&fixture.spec).is_err());
}

#[test]
fn directory_alias_cannot_replace_the_explicit_canonical_location() {
    let mut fixture = Fixture::new(1, true);
    let alias = fixture.parent.join("alias");
    symlink(&fixture.spec.directory, &alias).unwrap();
    fixture.spec.directory = alias;
    assert!(Guard::capture(&fixture.spec).is_err());
}

#[test]
fn original_file_identity_is_retained_even_if_replacement_bytes_are_identical() {
    let fixture = Fixture::new(1, true);
    let guard = Guard::capture(&fixture.spec).unwrap();
    let path = fixture.spec.directory.join(FILES[0]);
    let bytes = fs::read(&path).unwrap();
    fs::rename(&path, fixture.parent.join("original.json")).unwrap();
    write_new(&path, &bytes).unwrap();
    assert!(guard.verify().is_err());
    let mut command = Command::new("/usr/bin/env");
    command.env_clear();
    assert!(guard.configure(&mut command).is_err());
    assert_eq!(command.get_envs().count(), 0);
}

#[test]
fn changed_original_metadata_cannot_rebind_after_bytes_are_restored() {
    let fixture = Fixture::new(1, true);
    let guard = Guard::capture(&fixture.spec).unwrap();
    let path = fixture.spec.directory.join(FILES[0]);
    let bytes = fs::read(&path).unwrap();
    fs::write(&path, b"changed").unwrap();
    assert!(guard.verify().is_err());
    fs::write(&path, &bytes).unwrap();
    // Linux may reuse its cached timestamp for rapid successive writes. Make
    // this an explicit metadata-change control, independent of clock resolution.
    File::open(&path)
        .unwrap()
        .set_modified(std::time::UNIX_EPOCH)
        .unwrap();
    assert!(guard.verify().is_err());
}

#[test]
fn per_application_overrides_and_extra_entries_cannot_be_admitted() {
    for extra in ["AppConfig/hoi4.json", "Application.json", "Other.json"] {
        let fixture = Fixture::new(1, true);
        let guard = Guard::capture(&fixture.spec).unwrap();
        write_new(&fixture.spec.directory.join(extra), b"{}").unwrap();
        assert!(guard.verify().is_err());
        assert!(Guard::capture(&fixture.spec).is_err());
    }
}

#[test]
fn original_empty_appconfig_directory_cannot_be_replaced() {
    let fixture = Fixture::new(1, true);
    let guard = Guard::capture(&fixture.spec).unwrap();
    let path = fixture.spec.directory.join("AppConfig");
    fs::rename(&path, fixture.parent.join("old-appconfig")).unwrap();
    fs::DirBuilder::new().mode(0o700).create(&path).unwrap();
    assert!(guard.verify().is_err());
}

#[test]
fn configuration_schema_cannot_enable_other_thunks_or_arbitrary_fex_options() {
    for config in [
        br#"{"Config":{"RootFS":"relative"},"ThunksDB":{"GL":1}}"#.as_slice(),
        br#"{"Config":{"RootFS":"/rootfs","SilentLog":1},"ThunksDB":{"GL":1}}"#,
        br#"{"Config":{"RootFS":"/rootfs"},"ThunksDB":{"GL":2}}"#,
        br#"{"Config":{"RootFS":"/rootfs"},"ThunksDB":{"GL":1,"Vulkan":1}}"#,
        br#"{"Config":{"RootFS":"/rootfs"},"ThunksDB":{"GL":0,"GL":1}}"#,
        br#"{"Config":{"RootFS":"/first","RootFS":"/second"},"ThunksDB":{"GL":1}}"#,
    ] {
        let mut fixture = Fixture::new(1, true);
        fixture.replace(FILES[0], config);
        assert!(Guard::capture(&fixture.spec).is_err());
    }
}

#[test]
fn database_rejects_duplicate_names_fields_and_unreviewed_gl_aliases() {
    let valid = Fixture::new(1, true);
    let db: Value =
        serde_json::from_slice(&fs::read(valid.spec.directory.join(FILES[1])).unwrap()).unwrap();
    let gl = serde_json::to_string(&db["DB"]["GL"]).unwrap();
    let aliases = serde_json::to_string(&db["DB"]["GL"]["Overlay"]).unwrap();
    // Each duplicate alone is valid: rejection cannot be explained by another
    // invalid GL field or alias list in the same object.
    for db in [
        format!(r#"{{"DB":{{"GL":{gl},"GL":{gl}}}}}"#),
        format!(
            r#"{{"DB":{{"GL":{{"Library":"libGL-guest.so","Library":"libGL-guest.so","Overlay":{aliases}}}}}}}"#
        ),
        r#"{"DB":{"GL":{"Library":"libGL-guest.so","Overlay":["/arbitrary/library.so"]}}}"#
            .to_string(),
    ] {
        let mut fixture = Fixture::new(1, true);
        fixture.replace(FILES[1], db.as_bytes());
        assert!(Guard::capture(&fixture.spec).is_err());
    }
}
