//! Opt-in owned synthetic child only; never opens or discovers a game. These
//! fixtures are not executed on macOS and do not qualify any HOI4 build.
use super::*;
use std::{
    io::{BufRead, BufReader, Write},
    process::{Child, Command, Stdio},
    sync::mpsc,
    time::Duration,
};
const CHILD: &str = "platform::windows::native::child_tests::owned_child_server";
#[test]
#[ignore = "internal synthetic child endpoint; invoked only by the parent fixture"]
fn owned_child_server() {
    if std::env::var_os("HOI4_WINDOWS_SYNTHETIC_CHILD").as_deref()
        != Some(std::ffi::OsStr::new("1"))
    {
        return;
    }
    let bytes = Box::new([0x5au8; 32]);
    println!("\nOWNED_WINDOWS_BYTES {}", bytes.as_ptr() as usize);
    std::io::stdout().flush().unwrap();
    let mut end = String::new();
    std::io::stdin().read_line(&mut end).unwrap();
    drop(bytes);
}
struct ChildCleanup(Child);
impl Drop for ChildCleanup {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}
#[test]
fn native_owned_child_read_and_exit_refusal() {
    let (owner, elevated) = Native.current_owner().unwrap();
    if elevated {
        // Preserve the ordinary-user production policy even on elevated CI.
        // This covers the actual metadata/pre-open refusal; positive child
        // transfer remains explicitly pending on that runner.
        let fake = |pid, creation_filetime| Identity {
            pid,
            creation_filetime,
            executable: r"C:\Synthetic\fixture.exe".into(),
            owner: owner.clone(),
            file: FileIdentity {
                volume: 1,
                id: [pid as u8; 16],
            },
        };
        let result = OwnedReader::open(
            Native,
            Session {
                game: fake(7, 20),
                supervisor: fake(6, 10),
            },
        );
        assert_eq!(result.err().unwrap().code, "ownership_changed");
        eprintln!("Positive native owned-child transfer PENDING: elevated Windows runner; no process opened");
        return;
    }
    let mut child = ChildCleanup(
        Command::new(std::env::current_exe().unwrap())
            .args(["--exact", CHILD, "--ignored", "--nocapture"])
            .env("HOI4_WINDOWS_SYNTHETIC_CHILD", "1")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()
            .unwrap(),
    );
    let stdout = child.0.stdout.take().unwrap();
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        for line in BufReader::new(stdout).lines().take(32) {
            let line = line.unwrap();
            if let Some(v) = line.strip_prefix("OWNED_WINDOWS_BYTES ") {
                let _ = tx.send(v.parse::<u64>().unwrap());
                break;
            }
        }
    });
    let address = rx.recv_timeout(Duration::from_secs(5)).unwrap();
    let api = Native;
    let gh = api.open_process(child.0.id(), QUERY).unwrap();
    let sh = api.open_process(std::process::id(), QUERY).unwrap();
    let game = api.inspect(&gh).unwrap().identity;
    let supervisor = api.inspect(&sh).unwrap().identity;
    let image = api.open_image(&game.executable).unwrap();
    let bytes = api.image(&image).unwrap();
    let profile = Profile {
        pe: PeIdentity::parse(&bytes.header).unwrap(),
        sha256: bytes.sha256,
    };
    drop(image);
    drop(gh);
    drop(sh);
    let reader = OwnedReader::generated(api, Session { game, supervisor }, profile).unwrap();
    assert_eq!(reader.read(address, 32).unwrap(), vec![0x5a; 32]);
    drop(child.0.stdin.take());
    let deadline = std::time::Instant::now() + Duration::from_secs(5);
    loop {
        if let Some(status) = child.0.try_wait().unwrap() {
            assert!(status.success());
            break;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "Synthetic child did not exit after EOF"
        );
        std::thread::sleep(Duration::from_millis(5));
    }
    assert!(reader.read(address, 32).is_err());
}
#[test]
fn documented_x64_ffi_structure_sizes_are_exact() {
    assert_eq!(size_of::<MemoryBasic>(), 48);
    assert_eq!(size_of::<ProcessEntry>(), 568);
    assert_eq!(size_of::<ModuleEntry>(), 1080);
    assert_eq!(size_of::<FileId>(), 24);
}
