use super::*;

#[test]
fn identity_comm_parentheses() {
    let text = format!(
        "44 (game (worker) name) R 12 {} 789",
        vec!["0"; 17].join(" ")
    );
    assert_eq!(parse_stat(&text).unwrap(), ('R', 12, "789".into()));
    assert!(parse_stat("44 (short) R 12").is_err());
}

#[test]
fn malformed_elf_explicitly_refused() {
    assert!(elf_build_id(&[0; 64]).is_err());
}

#[test]
fn mappings_parse_executable_identity_and_paths_with_spaces_once() {
    let mappings = parse_mappings(
        "1000-2000 r--p 00000000 08:02 4321 /fixture with spaces\n\
         2000-3000 ---p 00001000 00:00 0\n",
    )
    .unwrap();
    assert_eq!(mappings.len(), 2);
    assert_eq!((mappings[0].start, mappings[0].end), (0x1000, 0x2000));
    assert_eq!(mappings[0].device, (8, 2));
    assert_eq!(mappings[0].inode, 4321);
    assert_eq!(mappings[0].path.as_deref(), Some("/fixture with spaces"));
    assert!(mappings[0].readable);
    assert_eq!(mappings[1].offset, 0x1000);
    assert!(!mappings[1].readable);
    assert_eq!(mappings[1].path, None);
}

#[test]
fn malformed_mappings_are_explicitly_refused() {
    for text in [
        "",
        "1000-2000 r--p",
        "2000-1000 r--p 0 00:00 0",
        "1000-2000 rxwp 0 00:00 0",
        "1000-2000 r--p no-offset 00:00 0",
        "1000-2000 r--p 0 no-device 0",
        "1000-2000 r--p 0 00:gg 0",
        "1000-2000 r--p 0 00:00 no-inode",
        "2000-3000 r--p 0 00:00 0\n1000-2000 r--p 0 00:00 0",
        "1000-3000 r--p 0 00:00 0\n2000-4000 r--p 0 00:00 0",
        "1000-2000 r--p 0 00:00 0\n1000-2000 r--p 0 00:00 0",
    ] {
        assert_eq!(parse_mappings(text).err().unwrap().code, "invalid_maps");
    }
}

#[test]
fn indexed_mapping_bounds_preserve_guard_pages_and_single_region_reads() {
    let regions = parse_mappings(
        "10000-11000 r--p 0 00:00 0\n\
         11000-12000 r--p 0 00:00 0\n\
         12000-13000 ---p 0 00:00 0\n\
         14000-15000 r--p 0 00:00 0\n",
    )
    .unwrap();
    for (address, end, expected) in [
        (0xffff, 0x10001, false),
        (0x10000, 0x11000, true),
        (0x10fff, 0x11000, true),
        (0x10fff, 0x11001, false), // Even adjacent readable regions stay separate.
        (0x11000, 0x11001, true),
        (0x12000, 0x12001, false),
        (0x13000, 0x13001, false),
        (0x14000, 0x15000, true),
        (0x15000, 0x15001, false),
        (0x14001, 0x14000, false),
        (0x14000, 0x14000, false),
        (u64::MAX - 1, u64::MAX, false),
    ] {
        assert_eq!(readable_span(&regions, address, end), expected);
    }
    assert!(!readable_span(&[], 0x10000, 0x10001));
}

#[test]
fn indexed_mapping_bounds_match_sparse_address_space_reference() {
    let mut text = String::new();
    for index in 0..2048u64 {
        let start = 0x10000 + index * 0x3000;
        let permissions = if index % 7 == 0 { "---p" } else { "r--p" };
        text.push_str(&format!(
            "{start:x}-{:x} {permissions} 0 00:00 0\n",
            start + 0x2000
        ));
    }
    let regions = parse_mappings(&text).unwrap();
    for index in 0..2048u64 {
        for offset in [0, 0x1000, 0x1fff, 0x2000, 0x2fff] {
            let address = 0x10000 + index * 0x3000 + offset;
            for size in [1, 2, 0x1000, 0x2001] {
                let expected = regions.iter().any(|region| {
                    region.readable && region.start <= address && address + size <= region.end
                });
                assert_eq!(readable_span(&regions, address, address + size), expected);
            }
        }
    }
}

#[test]
fn production_verifier_refuses_the_test_executable() {
    let executable = std::env::current_exe().unwrap();
    assert_eq!(
        verify_executable(&executable).unwrap_err().code,
        "unsupported_build"
    );
}

#[test]
fn boot_identity_is_bounded_and_strictly_parsed() {
    let boot = b"18c21740-0547-4a31-b3bb-22c47e1d3d27";
    assert_eq!(parse_boot_id(boot).unwrap()[..4], [0x18, 0xc2, 0x17, 0x40]);
    let mut newline = boot.to_vec();
    newline.push(b'\n');
    assert_eq!(
        parse_boot_id(&newline).unwrap(),
        parse_boot_id(boot).unwrap()
    );
    for malformed in [
        Vec::new(),
        b"00000000-0000-0000-0000-000000000000".to_vec(),
        b"18c21740_0547-4a31-b3bb-22c47e1d3d27".to_vec(),
        b"18c21740-0547-4a31-b3bb-22c47e1d3d2z".to_vec(),
        b"18c21740-0547-4a31-b3bb-22c47e1d3d27\n\n".to_vec(),
        vec![b'a'; BOOT_ID_BOUND + 1],
    ] {
        assert_eq!(
            parse_boot_id(&malformed).unwrap_err().code,
            "invalid_exec_witness"
        );
    }
}

fn auxiliary_vector(entries: &[(u64, u64)]) -> Vec<u8> {
    entries
        .iter()
        .flat_map(|(kind, value)| kind.to_le_bytes().into_iter().chain(value.to_le_bytes()))
        .collect()
}

#[test]
fn auxiliary_random_pointer_and_length_are_independently_bounded() {
    assert_eq!(
        parse_random_address(&auxiliary_vector(&[(6, 4096), (25, 0x20000), (0, 0)])).unwrap(),
        0x20000
    );
    let last = (1u64 << 47) - RANDOM_BYTES as u64;
    assert_eq!(
        parse_random_address(&auxiliary_vector(&[(25, last), (0, 0)])).unwrap(),
        last
    );
    for malformed in [
        Vec::new(),
        vec![0; 15],
        vec![0; AUXV_BOUND + 16],
        auxiliary_vector(&[(0, 0)]),
        auxiliary_vector(&[(25, 0x20000)]),
        auxiliary_vector(&[(25, 0x20000), (25, 0x30000), (0, 0)]),
        auxiliary_vector(&[(25, 0), (0, 0)]),
        auxiliary_vector(&[(25, 0xffff), (0, 0)]),
        auxiliary_vector(&[(25, last + 1), (0, 0)]),
        auxiliary_vector(&[(25, u64::MAX), (0, 0)]),
        auxiliary_vector(&[(25, 0x20000), (0, 1)]),
        auxiliary_vector(&[(25, 0x20000), (0, 0), (25, 0x20000)]),
    ] {
        assert_eq!(
            parse_random_address(&malformed).unwrap_err().code,
            "invalid_exec_witness"
        );
    }
}

#[test]
fn exec_witness_does_not_remove_reader_send_and_sync() {
    fn require_send_sync<T: Send + Sync>() {}
    require_send_sync::<LinuxProcess>();
}

// These tests are opt-in because their evidence specifically requires an
// ordinary x86_64 Linux user and the existing Yama scope 1. They never lower
// that setting, ptrace a process, or discover/attach to an existing program.
#[cfg(target_arch = "x86_64")]
mod generated {
    use super::*;
    use serde_json::{json, Value};
    use std::{
        io::{self, BufRead, BufReader, Write},
        os::{fd::AsRawFd, unix::process::CommandExt},
        process::{Child, ChildStdin, ChildStdout, Command, Stdio},
        sync::OnceLock,
        time::{Duration, Instant},
    };

    const CHILD_TEST: &str = "platform::linux::tests::generated::generated_child_fixture";
    const CHILD_ENV: &str = "HOI4_NATIVE_READER_CHILD_V1";
    const PREFIX: &str = "HOI4_READER_FIXTURE_V1 ";
    const PAYLOAD_BYTES: usize = 1 << 20;
    const TIMEOUT: Duration = Duration::from_secs(20);

    fn ordinary_scope_one() {
        // SAFETY: getuid has no arguments or memory effects.
        assert_ne!(
            unsafe { libc::getuid() },
            0,
            "Fixture must use an ordinary user"
        );
        assert_eq!(
            fs::read_to_string("/proc/sys/kernel/yama/ptrace_scope")
                .expect("Yama scope must be observable without changing it")
                .trim(),
            "1",
            "Fixture evidence requires the unchanged Yama scope 1"
        );
    }

    fn expected_image() -> (String, String) {
        static IMAGE: OnceLock<(String, String)> = OnceLock::new();
        IMAGE
            .get_or_init(|| {
                let bytes = fs::read(std::env::current_exe().unwrap()).unwrap();
                (
                    elf_build_id(&bytes).expect("Generated test ELF requires a GNU build ID"),
                    format!("{:x}", Sha256::digest(&bytes)),
                )
            })
            .clone()
    }

    struct Fixture {
        child: Child,
        input: ChildStdin,
        output: ChildStdout,
        pending: Vec<u8>,
        nonce: String,
        address: u64,
        payload: usize,
        page: usize,
    }

    impl Fixture {
        fn spawn() -> Self {
            ordinary_scope_one();
            let mut random = [0u8; 16];
            File::open("/dev/urandom")
                .unwrap()
                .read_exact(&mut random)
                .unwrap();
            let nonce: String = random.iter().map(|byte| format!("{byte:02x}")).collect();
            let mut child = Command::new(std::env::current_exe().unwrap())
                .args([
                    "--exact",
                    CHILD_TEST,
                    "--ignored",
                    "--nocapture",
                    "--test-threads=1",
                ])
                .env(CHILD_ENV, &nonce)
                .stdin(Stdio::piped())
                .stdout(Stdio::piped())
                .stderr(Stdio::inherit())
                .spawn()
                .unwrap();
            let input = child.stdin.take().unwrap();
            let output = child.stdout.take().unwrap();
            let mut fixture = Self {
                child,
                input,
                output,
                pending: Vec::new(),
                nonce,
                address: 0,
                payload: 0,
                page: 0,
            };
            let ready = fixture.response();
            assert_eq!(ready["event"], "ready");
            assert_eq!(ready["pid"].as_u64(), Some(u64::from(fixture.child.id())));
            fixture.address = ready["address"].as_u64().unwrap();
            fixture.payload = usize::try_from(ready["payload"].as_u64().unwrap()).unwrap();
            fixture.page = usize::try_from(ready["page"].as_u64().unwrap()).unwrap();
            assert_eq!(fixture.payload, PAYLOAD_BYTES);
            assert!(fixture.page > 0 && PAYLOAD_BYTES % fixture.page == 0);
            fixture
        }

        fn response(&mut self) -> Value {
            let deadline = Instant::now() + TIMEOUT;
            loop {
                if let Some(end) = self.pending.iter().position(|byte| *byte == b'\n') {
                    let line: Vec<_> = self.pending.drain(..=end).collect();
                    let text = std::str::from_utf8(&line).unwrap();
                    // libtest can put its test-name prefix on the same line.
                    if let Some(begin) = text.find(PREFIX) {
                        let value: Value =
                            serde_json::from_str(&text[begin + PREFIX.len()..]).unwrap();
                        assert_eq!(value["nonce"].as_str(), Some(self.nonce.as_str()));
                        return value;
                    }
                    continue;
                }
                let remaining = deadline.saturating_duration_since(Instant::now());
                assert!(!remaining.is_zero(), "Owned fixture response timed out");
                let mut fd = libc::pollfd {
                    fd: self.output.as_raw_fd(),
                    events: libc::POLLIN,
                    revents: 0,
                };
                // SAFETY: one live, exclusively read child pipe descriptor.
                let ready =
                    unsafe { libc::poll(&mut fd, 1, remaining.as_millis().clamp(1, 1000) as i32) };
                if ready < 0 {
                    let error = io::Error::last_os_error();
                    if error.kind() == io::ErrorKind::Interrupted {
                        continue;
                    }
                    panic!("Fixture pipe poll failed: {error}");
                }
                if ready == 0 {
                    continue;
                }
                let mut bytes = [0u8; 4096];
                let count = self.output.read(&mut bytes).unwrap();
                assert_ne!(
                    count, 0,
                    "Owned fixture closed its pipe before its response"
                );
                self.pending.extend_from_slice(&bytes[..count]);
                assert!(
                    self.pending.len() <= 16384,
                    "Fixture response exceeded bound"
                );
            }
        }

        fn command(&mut self, command: &str) -> Value {
            assert!(command.len() < 64 && !command.contains('\n'));
            writeln!(self.input, "{command}").unwrap();
            self.input.flush().unwrap();
            let response = self.response();
            assert_eq!(response["event"].as_str(), Some(command));
            response
        }

        fn session(&self) -> Session {
            let (build_id, sha256) = expected_image();
            Session {
                game: capture_identity(self.child.id() as i32).unwrap(),
                supervisor: capture_identity(std::process::id() as i32).unwrap(),
                build_id,
                sha256,
                translated: None,
            }
        }

        fn reader(&self) -> LinuxProcess {
            let session = self.session();
            let expected = ExpectedBuild::GeneratedChild {
                build_id: session.build_id.clone(),
                sha256: session.sha256.clone(),
            };
            LinuxProcess::open_expected(session, expected).unwrap()
        }

        fn assert_nonce(&self, process: &LinuxProcess, address: u64) {
            assert_eq!(
                process.read(address, self.nonce.len()).unwrap(),
                self.nonce.as_bytes()
            );
        }
    }

    impl Drop for Fixture {
        fn drop(&mut self) {
            // Only the child created by this Fixture is ever signalled. Keep
            // the Child handle unreaped until cleanup; never rediscover a PID.
            if !matches!(self.child.try_wait(), Ok(Some(_))) {
                let _ = self.child.kill();
                let _ = self.child.wait();
            }
        }
    }

    struct Mapping {
        allocation: *mut libc::c_void,
        length: usize,
        page: usize,
    }

    impl Mapping {
        fn new(nonce: &str) -> Self {
            // SAFETY: sysconf has no pointer argument or memory effects.
            let page = usize::try_from(unsafe { libc::sysconf(libc::_SC_PAGESIZE) }).unwrap();
            assert!(page > 0 && PAYLOAD_BYTES % page == 0);
            let length = PAYLOAD_BYTES + 2 * page;
            // SAFETY: new anonymous mapping with no requested fixed address.
            let allocation = unsafe {
                libc::mmap(
                    std::ptr::null_mut(),
                    length,
                    libc::PROT_NONE,
                    libc::MAP_PRIVATE | libc::MAP_ANONYMOUS,
                    -1,
                    0,
                )
            };
            assert_ne!(allocation, libc::MAP_FAILED);
            let mapping = Self {
                allocation,
                length,
                page,
            };
            mapping.protect(libc::PROT_READ | libc::PROT_WRITE);
            // SAFETY: the child owns this writable payload mapping, including
            // the nonce bytes. Guard pages remain inaccessible on both sides.
            unsafe {
                std::ptr::write_bytes(mapping.payload(), 0x5a, PAYLOAD_BYTES);
                std::ptr::copy_nonoverlapping(nonce.as_ptr(), mapping.payload(), nonce.len());
            }
            mapping.protect(libc::PROT_READ);
            mapping
        }

        fn payload(&self) -> *mut u8 {
            // SAFETY: page-sized offset inside the still-owned allocation.
            unsafe { self.allocation.cast::<u8>().add(self.page) }
        }

        fn protect(&self, permissions: i32) {
            // SAFETY: page-aligned payload range belongs to this generated child.
            assert_eq!(
                unsafe { libc::mprotect(self.payload().cast(), PAYLOAD_BYTES, permissions) },
                0
            );
        }
    }

    impl Drop for Mapping {
        fn drop(&mut self) {
            // SAFETY: exactly the allocation this object created and owns.
            assert_eq!(unsafe { libc::munmap(self.allocation, self.length) }, 0);
        }
    }

    fn emit(nonce: &str, event: &str, mapping: Option<&Mapping>) {
        let mut value = json!({"nonce":nonce,"event":event,"pid":std::process::id()});
        if let Some(mapping) = mapping {
            value["address"] = json!(mapping.payload() as u64);
            value["payload"] = json!(PAYLOAD_BYTES);
            value["page"] = json!(mapping.page);
        }
        let mut output = io::stdout().lock();
        writeln!(output, "{PREFIX}{value}").unwrap();
        output.flush().unwrap();
    }

    #[test]
    #[ignore = "Private exact-test child helper; no effect without its pipe handshake"]
    fn generated_child_fixture() {
        let Ok(nonce) = std::env::var(CHILD_ENV) else {
            return;
        };
        let args: Vec<_> = std::env::args().collect();
        assert!(args.windows(2).any(|args| args == ["--exact", CHILD_TEST]));
        assert_eq!(nonce.len(), 32);
        assert!(nonce.bytes().all(|byte| byte.is_ascii_hexdigit()));
        ordinary_scope_one();
        let mut initial = Some(Mapping::new(&nonce));
        let mut additional = None;
        emit(&nonce, "ready", initial.as_ref());
        let input = io::stdin();
        for line in BufReader::new(input.lock()).lines() {
            let command = line.unwrap();
            assert!(command.len() < 64);
            match command.as_str() {
                "add" => {
                    assert!(additional.is_none());
                    additional = Some(Mapping::new(&nonce));
                    emit(&nonce, &command, additional.as_ref());
                }
                "noaccess" => {
                    initial.as_ref().unwrap().protect(libc::PROT_NONE);
                    emit(&nonce, &command, None);
                }
                "readonly" => {
                    initial.as_ref().unwrap().protect(libc::PROT_READ);
                    emit(&nonce, &command, None);
                }
                "lastpage-noaccess" => {
                    let mapping = initial.as_ref().unwrap();
                    // SAFETY: the last page lies within the child's own live
                    // payload. It deliberately invalidates the parent's cache.
                    assert_eq!(
                        unsafe {
                            libc::mprotect(
                                mapping.payload().add(PAYLOAD_BYTES - mapping.page).cast(),
                                mapping.page,
                                libc::PROT_NONE,
                            )
                        },
                        0
                    );
                    emit(&nonce, &command, None);
                }
                "unmap" => {
                    drop(initial.take());
                    emit(&nonce, &command, None);
                }
                "exec-cat" => {
                    emit(&nonce, &command, None);
                    // Replaces this generated child only. The parent proves the
                    // replacement is ready by a nonce echoed through cat's pipe.
                    panic!(
                        "Owned fixture exec failed: {}",
                        Command::new("/bin/cat").exec()
                    );
                }
                "exec-self" => {
                    emit(&nonce, &command, None);
                    // Same PID, parent, start ticks, executable path/device/inode
                    // and build. Only the independently captured exec witness
                    // can distinguish this replacement address space.
                    panic!(
                        "Owned fixture self-exec failed: {}",
                        Command::new(std::env::current_exe().unwrap())
                            .args([
                                "--exact",
                                CHILD_TEST,
                                "--ignored",
                                "--nocapture",
                                "--test-threads=1",
                            ])
                            .env(CHILD_ENV, &nonce)
                            .exec()
                    );
                }
                "change-random" => {
                    let address =
                        random_address(std::process::id() as i32, KernelArch::X86_64).unwrap();
                    // SAFETY: the exact kernel-provided AT_RANDOM pointer lies
                    // in this generated child's own live, writable initial
                    // stack. TLS guards have already been initialized. This
                    // test-only self-write never targets any game or parent.
                    unsafe {
                        let byte = address as *mut u8;
                        byte.write(byte.read() ^ 0xff);
                    }
                    emit(&nonce, &command, None);
                }
                "exit" => {
                    emit(&nonce, &command, None);
                    return;
                }
                _ => panic!("Unknown private fixture command"),
            }
        }
    }

    #[test]
    #[ignore = "Requires ordinary x86_64 Linux user and unchanged Yama scope 1"]
    fn ordinary_parent_reads_known_child_under_yama_scope_one() {
        let fixture = Fixture::spawn();
        let process = fixture.reader();
        assert_eq!(
            identity(fixture.child.id() as i32).unwrap().2,
            std::process::id() as i32
        );
        process.begin_snapshot().unwrap();
        fixture.assert_nonce(&process, fixture.address);
        process.check().unwrap();
        // Generated-child expectations do not change the production selector.
        assert_eq!(
            LinuxProcess::open_expected(fixture.session(), ExpectedBuild::VerifiedGame)
                .err()
                .unwrap()
                .code,
            "unsupported_build"
        );
        ordinary_scope_one();
    }

    #[test]
    #[ignore = "Requires ordinary x86_64 Linux user and unchanged Yama scope 1"]
    fn read_refreshes_mapping_created_after_open() {
        let mut fixture = Fixture::spawn();
        let process = fixture.reader();
        let added = fixture.command("add")["address"].as_u64().unwrap();
        assert!(!process
            .regions
            .read()
            .unwrap()
            .iter()
            .any(|region| { region.readable && region.start <= added && added < region.end }));
        // No begin_snapshot here: this specifically exercises refresh on miss.
        fixture.assert_nonce(&process, added);
        process.check().unwrap();
        process.begin_snapshot().unwrap();
        fixture.assert_nonce(&process, added);
    }

    #[test]
    #[ignore = "Requires ordinary x86_64 Linux user and unchanged Yama scope 1"]
    fn permission_changes_and_unmap_never_return_cached_bytes() {
        let mut fixture = Fixture::spawn();
        let process = fixture.reader();
        fixture.assert_nonce(&process, fixture.address);
        fixture.command("noaccess");
        // A formerly readable cached mapping cannot override the kernel's
        // current permissions. The failed syscall returns no partial Vec.
        assert_eq!(
            process.read(fixture.address, 32).unwrap_err().code,
            "io_error"
        );
        process.begin_snapshot().unwrap();
        assert_eq!(
            process.read(fixture.address, 32).unwrap_err().code,
            "unmapped_read"
        );
        fixture.command("readonly");
        process.begin_snapshot().unwrap();
        fixture.assert_nonce(&process, fixture.address);
        fixture.command("unmap");
        assert_eq!(
            process.read(fixture.address, 32).unwrap_err().code,
            "io_error"
        );
        process.begin_snapshot().unwrap();
        assert_eq!(
            process.read(fixture.address, 32).unwrap_err().code,
            "unmapped_read"
        );
    }

    #[test]
    #[ignore = "Requires ordinary x86_64 Linux user and unchanged Yama scope 1"]
    fn partial_kernel_read_returns_an_error_without_data() {
        let mut fixture = Fixture::spawn();
        let process = fixture.reader();
        let boundary = fixture.address + fixture.payload as u64 - fixture.page as u64;
        assert_eq!(process.read(boundary - 4, 8).unwrap(), vec![0x5a; 8]);
        fixture.command("lastpage-noaccess");
        // The cached readable VMA still covers both pages, but the kernel can
        // copy only the first four bytes. No incomplete Vec may escape.
        assert_eq!(
            process.read(boundary - 4, 8).unwrap_err().code,
            "short_read"
        );
        process.begin_snapshot().unwrap();
        assert_eq!(
            process.read(boundary - 4, 8).unwrap_err().code,
            "unmapped_read"
        );
    }

    #[test]
    #[ignore = "Requires ordinary x86_64 Linux user and unchanged Yama scope 1"]
    fn read_bounds_and_guard_page_crossing_are_refused() {
        let fixture = Fixture::spawn();
        let process = fixture.reader();
        assert_eq!(process.read(u64::MAX, 0).unwrap(), Vec::<u8>::new());
        for (address, size) in [
            (fixture.address, PAYLOAD_BYTES + 1),
            (u64::MAX, 1),
            (u64::MAX - 1, 8),
        ] {
            assert_eq!(process.read(address, size).unwrap_err().code, "read_bound");
        }
        assert_eq!(
            process
                .read(fixture.address + fixture.payload as u64 - 4, 8)
                .unwrap_err()
                .code,
            "unmapped_read"
        );
        let data = process.read(fixture.address, PAYLOAD_BYTES).unwrap();
        assert_eq!(&data[..fixture.nonce.len()], fixture.nonce.as_bytes());
        assert_eq!(data.len(), PAYLOAD_BYTES);
        assert_eq!(data.last(), Some(&0x5a));
        process.check().unwrap();
    }

    #[test]
    #[ignore = "Requires ordinary x86_64 Linux user and unchanged Yama scope 1"]
    fn changed_creation_executable_uid_and_parent_refuse_snapshot() {
        let fixture = Fixture::spawn();
        let mut process = fixture.reader();
        let original = process.session.clone();
        let mut wrong_start = original.clone();
        wrong_start.game.start_ticks.push('0');
        let mut wrong_executable = original.clone();
        wrong_executable.game.exe.push_str(".replacement");
        let mut wrong_uid = original.clone();
        wrong_uid.game.uid = 0;
        let mut wrong_parent = original.clone();
        wrong_parent.supervisor = original.game.clone();
        for changed in [wrong_start, wrong_executable, wrong_uid, wrong_parent] {
            process.session = changed;
            assert_eq!(
                process.begin_snapshot().unwrap_err().code,
                "ownership_changed"
            );
        }
        process.session = original;
        process.begin_snapshot().unwrap();
        fixture.assert_nonce(&process, fixture.address);
    }

    #[test]
    #[ignore = "Requires ordinary x86_64 Linux user and unchanged Yama scope 1"]
    fn wrong_generated_executable_hash_is_refused() {
        let fixture = Fixture::spawn();
        let mut session = fixture.session();
        session.sha256 = "00".repeat(32);
        let expected = ExpectedBuild::GeneratedChild {
            build_id: session.build_id.clone(),
            sha256: session.sha256.clone(),
        };
        assert_eq!(
            LinuxProcess::open_expected(session, expected)
                .err()
                .unwrap()
                .code,
            "unsupported_build"
        );
    }

    #[test]
    #[ignore = "Requires ordinary x86_64 Linux user and unchanged Yama scope 1"]
    fn different_executable_exec_refuses_snapshot() {
        let mut fixture = Fixture::spawn();
        let process = fixture.reader();
        fixture.command("exec-cat");
        let echo = json!({"nonce":fixture.nonce,"event":"cat-ready"});
        writeln!(fixture.input, "{PREFIX}{echo}").unwrap();
        fixture.input.flush().unwrap();
        assert_eq!(fixture.response()["event"], "cat-ready");
        assert_eq!(
            process.begin_snapshot().unwrap_err().code,
            "ownership_changed"
        );
        assert_eq!(process.check().unwrap_err().code, "ownership_changed");
    }

    #[test]
    #[ignore = "Requires ordinary x86_64 Linux user and unchanged Yama scope 1"]
    fn same_elf_exec_refuses_snapshot_even_when_process_identity_matches() {
        let mut fixture = Fixture::spawn();
        let process = fixture.reader();
        let old_identity = process.session.game.clone();
        fixture.command("exec-self");
        let ready = fixture.response();
        assert_eq!(ready["event"], "ready");
        assert_eq!(ready["pid"].as_u64(), Some(u64::from(fixture.child.id())));
        assert_eq!(
            capture_identity(fixture.child.id() as i32).unwrap(),
            old_identity
        );
        let metadata = fs::metadata(format!("/proc/{}/exe", fixture.child.id())).unwrap();
        assert_eq!(
            (metadata.dev(), metadata.ino()),
            (process.device, process.inode)
        );
        assert_ne!(
            exec_witness(fixture.child.id() as i32).unwrap(),
            process.witness
        );
        assert_eq!(
            process.begin_snapshot().unwrap_err().code,
            "ownership_changed"
        );
        assert_eq!(process.check().unwrap_err().code, "ownership_changed");
    }

    #[test]
    #[ignore = "Requires ordinary x86_64 Linux user and unchanged Yama scope 1"]
    fn random_content_change_refuses_snapshot_without_identity_or_address_change() {
        let mut fixture = Fixture::spawn();
        let process = fixture.reader();
        fixture.command("change-random");
        assert_eq!(
            capture_identity(fixture.child.id() as i32).unwrap(),
            process.session.game
        );
        let changed = exec_witness(fixture.child.id() as i32).unwrap();
        assert_eq!(changed.boot, process.witness.boot);
        assert_eq!(changed.random_address, process.witness.random_address);
        assert_ne!(changed.random, process.witness.random);
        assert_eq!(
            process.begin_snapshot().unwrap_err().code,
            "ownership_changed"
        );
    }

    #[test]
    #[ignore = "Requires ordinary x86_64 Linux user and unchanged Yama scope 1"]
    fn boot_address_and_content_mismatch_refuse_without_injected_runtime_witness() {
        let fixture = Fixture::spawn();
        let mut process = fixture.reader();
        let captured = process.witness.clone();
        let mut wrong_boot = captured.clone();
        wrong_boot.boot[0] ^= 0xff;
        let mut wrong_address = captured.clone();
        wrong_address.random_address = u64::MAX;
        let mut wrong_content = captured.clone();
        wrong_content.random[0] ^= 0xff;
        // Private-field mutation is available only inside unit tests. Every
        // live sample still uses the actual bounded procfs/kernel pointer;
        // the false expected address never becomes a remote read address.
        for changed in [wrong_boot, wrong_address, wrong_content] {
            process.witness = changed;
            assert_eq!(
                process.begin_snapshot().unwrap_err().code,
                "ownership_changed"
            );
        }
        process.witness = captured;
        process.begin_snapshot().unwrap();
        fixture.assert_nonce(&process, fixture.address);
    }

    #[test]
    #[ignore = "Requires ordinary x86_64 Linux user and unchanged Yama scope 1"]
    fn owned_child_exit_refuses_observation() {
        let mut fixture = Fixture::spawn();
        let process = fixture.reader();
        fixture.command("exit");
        assert!(fixture.child.wait().unwrap().success());
        assert!(process.begin_snapshot().is_err());
        assert!(process.check().is_err());
    }
    #[test]
    #[ignore = "Generated owned ordinary x86_64 Linux child only; scope1 unchanged"]
    fn vectored_child_more_than_one_chunk_matches_scalar_bytes() {
        let fixture = Fixture::spawn();
        let process = fixture.reader();
        let requests = vec![(fixture.address, 32); 257];
        let expected = process.read(fixture.address, 32).unwrap();
        let results = process.read_many(&requests).unwrap();
        assert_eq!(results.len(), 257);
        assert!(results.iter().all(|bytes| *bytes == expected));
        process.check().unwrap();
    }
    #[test]
    #[ignore = "Generated owned ordinary x86_64 Linux child only; scope1 unchanged"]
    fn vectored_kernel_partial_read_is_error_never_prefix_success() {
        let mut fixture = Fixture::spawn();
        let boundary = fixture.address + fixture.payload as u64 - fixture.page as u64;
        fixture.command("lastpage-noaccess");
        assert_eq!(
            read_many_remote(
                fixture.child.id() as i32,
                &[(fixture.address, 32), (boundary - 4, 8)]
            )
            .unwrap_err()
            .code,
            "short_read"
        );
        let process = fixture.reader();
        assert_eq!(
            process
                .read_many(&[(fixture.address, 32), (boundary - 4, 8)])
                .unwrap_err()
                .code,
            "unmapped_read"
        );
    }
    #[test]
    #[ignore = "Generated owned ordinary x86_64 Linux child only; scope1 unchanged"]
    fn vectored_new_mapping_permissions_unmap_and_bad_batch_refuse() {
        let mut fixture = Fixture::spawn();
        let process = fixture.reader();
        let added = fixture.command("add")["address"].as_u64().unwrap();
        assert_eq!(
            process.read_many(&[(added, 32)]).unwrap()[0],
            process.read(fixture.address, 32).unwrap()
        );
        assert_eq!(
            process
                .read_many(&[(fixture.address, 32), (u64::MAX, 8)])
                .unwrap_err()
                .code,
            "read_bound"
        );
        fixture.command("noaccess");
        assert_eq!(
            process
                .read_many(&[(fixture.address, 32)])
                .unwrap_err()
                .code,
            "unmapped_read"
        );
        fixture.command("readonly");
        assert!(process.read_many(&[(fixture.address, 32)]).is_ok());
        fixture.command("unmap");
        assert_eq!(
            process
                .read_many(&[(fixture.address, 32)])
                .unwrap_err()
                .code,
            "unmapped_read"
        );
    }
    #[test]
    #[ignore = "Generated owned ordinary x86_64 Linux child only; scope1 unchanged"]
    fn vectored_creation_and_same_elf_exec_refuse() {
        let mut fixture = Fixture::spawn();
        let mut process = fixture.reader();
        let original = process.session.game.start_ticks.clone();
        process.session.game.start_ticks.push('0');
        assert_eq!(
            process
                .read_many(&[(fixture.address, 32)])
                .unwrap_err()
                .code,
            "ownership_changed"
        );
        process.session.game.start_ticks = original;
        fixture.command("exec-self");
        // The command reply precedes exec. Require the replacement's nonce-
        // checked ready reply before asserting its stable refusal category.
        let ready = fixture.response();
        assert_eq!(ready["event"], "ready");
        assert_eq!(ready["pid"].as_u64(), Some(u64::from(fixture.child.id())));
        assert_eq!(
            process
                .read_many(&[(fixture.address, 32)])
                .unwrap_err()
                .code,
            "ownership_changed"
        );
    }
    #[test]
    #[ignore = "Generated owned ordinary x86_64 Linux child only; scope1 unchanged"]
    fn vectored_final_bracket_discards_real_bytes_after_mapping_change() {
        let mut fixture = Fixture::spawn();
        let process = fixture.reader();
        let requests = [(fixture.address, 32)];
        let real_bytes = read_many_remote(fixture.child.id() as i32, &requests);
        assert!(real_bytes.is_ok());
        fixture.command("noaccess");
        assert_eq!(
            process
                .finish_read_many(&requests, real_bytes)
                .unwrap_err()
                .code,
            "unmapped_read"
        );
    }
    #[test]
    #[ignore = "Generated owned ordinary x86_64 Linux child only; scope1 unchanged"]
    fn vectored_final_bracket_discards_real_bytes_after_exec() {
        let mut fixture = Fixture::spawn();
        let process = fixture.reader();
        let requests = [(fixture.address, 32)];
        let real_bytes = read_many_remote(fixture.child.id() as i32, &requests);
        assert!(real_bytes.is_ok());
        fixture.command("exec-self");
        let ready = fixture.response();
        assert_eq!(ready["event"], "ready");
        assert_eq!(ready["pid"].as_u64(), Some(u64::from(fixture.child.id())));
        assert_eq!(
            process
                .finish_read_many(&requests, real_bytes)
                .unwrap_err()
                .code,
            "ownership_changed"
        );
    }
}
