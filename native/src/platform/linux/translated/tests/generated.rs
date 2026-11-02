use super::*;
use serde_json::Value;
use std::{
    io::{BufRead, BufReader, Write},
    process::{Child, ChildStdin, Command, Stdio},
    sync::mpsc,
    time::Duration,
};
struct Owned {
    child: Child,
    input: ChildStdin,
    output: mpsc::Receiver<String>,
}
impl Owned {
    fn start(translator: &Path, guest: &Path) -> Self {
        let mut child = Command::new(translator)
            .arg(guest)
            .env_remove("HOI4_FEX_FIXTURE_FD_GENERATION")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()
            .unwrap();
        let input = child.stdin.take().unwrap();
        let stdout = child.stdout.take().unwrap();
        let (tx, output) = mpsc::channel();
        std::thread::spawn(move || {
            for line in BufReader::new(stdout).lines() {
                let Ok(line) = line else { break };
                if line.len() > 4096 || tx.send(line).is_err() {
                    break;
                }
            }
        });
        Self {
            child,
            input,
            output,
        }
    }
    fn ready(&self, expected_argc: u64) -> (u64, Vec<u8>) {
        let text = self
            .output
            .recv_timeout(Duration::from_secs(20))
            .expect("Owned translated fixture did not become ready");
        let value: Value = serde_json::from_str(&text).unwrap();
        assert_eq!(value["ready"], true);
        assert_eq!(
            value["argc"], expected_argc,
            "Preserve the pinned FEX FD-exec argv limitation explicitly"
        );
        assert_eq!(value["quit_only"], expected_argc == 0);
        let address = value["address"].as_u64().unwrap();
        let nonce = value["nonce"].as_str().unwrap();
        assert_eq!(nonce.len(), 32);
        let bytes = nonce
            .as_bytes()
            .chunks_exact(2)
            .map(|v| u8::from_str_radix(std::str::from_utf8(v).unwrap(), 16).unwrap())
            .collect();
        (address, bytes)
    }
    fn send(&mut self, command: &str) {
        writeln!(self.input, "{command}").unwrap();
        self.input.flush().unwrap();
    }
}
impl Drop for Owned {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}
#[test]
#[ignore = "requires owned ARM64/FEX fixture"]
fn owned_fex_guest_reads_and_selfexec_execveat_invalidate_original_reader() {
    assert_ne!(
        unsafe { libc::getuid() },
        0,
        "Fixture requires ordinary parent ownership"
    );
    let translator = fs::canonicalize(
        std::env::var_os("HOI4_FEX_TEST_TRANSLATOR").expect("Explicit verified FEX path required"),
    )
    .unwrap();
    let guest = fs::canonicalize(
        std::env::var_os("HOI4_FEX_TEST_GUEST").expect("Explicit generated x86 fixture required"),
    )
    .unwrap();
    verify_translator(&translator, FEX_SHA256).unwrap();
    assert!(is_reviewed_translator(&translator).unwrap());
    let bytes = fs::read(&guest).unwrap();
    let build_id = elf_build_id(&bytes).unwrap();
    let sha256 = format!("{:x}", Sha256::digest(&bytes));
    let mut owned = Owned::start(&translator, &guest);
    let mut ready = owned.ready(1);
    let session = Session {
        game: capture_identity(owned.child.id() as i32).unwrap(),
        supervisor: capture_identity(std::process::id() as i32).unwrap(),
        build_id: build_id.clone(),
        sha256: sha256.clone(),
        translated: Some(TranslatedImage {
            executable: guest.to_str().unwrap().into(),
            translator_sha256: FEX_SHA256.into(),
        }),
    };
    assert!(guest_mapping_present(session.game.pid, &guest).unwrap());
    let mut reader = LinuxProcess::open_expected(
        session.clone(),
        ExpectedBuild::GeneratedChild {
            build_id: build_id.clone(),
            sha256: sha256.clone(),
        },
    )
    .unwrap();
    assert_eq!(reader.read(ready.0, ready.1.len()).unwrap(), ready.1);
    assert_eq!(reader.read_many(&[(ready.0, 16)]).unwrap()[0], ready.1);
    for (command, expected_argc) in [("selfexec", 1), ("execveat", 0)] {
        let before = capture_identity(session.game.pid).unwrap();
        let observation = (|| -> Result<Vec<u8>> {
            reader.begin_snapshot()?;
            let field = reader.read(ready.0, 16)?;
            assert_eq!(field, ready.1);
            owned.send(command);
            ready = owned.ready(expected_argc);
            // A scalar field is accepted only after the whole snapshot's
            // final check; exec must discard the already-read field.
            reader.check()?;
            Ok(field)
        })();
        assert_eq!(observation.unwrap_err().code, "ownership_changed");
        assert_eq!(
            capture_identity(session.game.pid).unwrap(),
            before,
            "Kernel creation/executable identity alone must remain equal"
        );
        assert_eq!(reader.check().unwrap_err().code, "ownership_changed");
        assert_eq!(
            reader.read_many(&[(ready.0, 16)]).unwrap_err().code,
            "ownership_changed"
        );
        // Fresh binding is explicit in this isolated generated fixture only.
        reader = LinuxProcess::open_expected(
            session.clone(),
            ExpectedBuild::GeneratedChild {
                build_id: build_id.clone(),
                sha256: sha256.clone(),
            },
        )
        .unwrap();
        assert_eq!(reader.read(ready.0, 16).unwrap(), ready.1);
    }
    owned.send("quit");
    assert!(owned.child.wait().unwrap().success());
}
