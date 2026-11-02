use super::*;
use std::sync::atomic::{AtomicU64, Ordering};
static NEXT: AtomicU64 = AtomicU64::new(0);
fn elf(machine: u16) -> Vec<u8> {
    let mut b = vec![0; 4096];
    b[..7].copy_from_slice(&[0x7f, b'E', b'L', b'F', 2, 1, 1]);
    b[16..18].copy_from_slice(&3u16.to_le_bytes());
    b[18..20].copy_from_slice(&machine.to_le_bytes());
    b[20..24].copy_from_slice(&1u32.to_le_bytes());
    b[32..40].copy_from_slice(&64u64.to_le_bytes());
    b[52..54].copy_from_slice(&64u16.to_le_bytes());
    b[54..56].copy_from_slice(&56u16.to_le_bytes());
    b[56..58].copy_from_slice(&2u16.to_le_bytes());
    b[64..68].copy_from_slice(&1u32.to_le_bytes());
    b[68..72].copy_from_slice(&5u32.to_le_bytes());
    b[96..104].copy_from_slice(&4096u64.to_le_bytes());
    b[104..112].copy_from_slice(&4096u64.to_le_bytes());
    b[120..124].copy_from_slice(&4u32.to_le_bytes());
    b[128..136].copy_from_slice(&512u64.to_le_bytes());
    b[152..160].copy_from_slice(&20u64.to_le_bytes());
    b[512..516].copy_from_slice(&4u32.to_le_bytes());
    b[516..520].copy_from_slice(&4u32.to_le_bytes());
    b[520..524].copy_from_slice(&3u32.to_le_bytes());
    b[524..528].copy_from_slice(b"GNU\0");
    b[528..532].copy_from_slice(&[1, 2, 3, 4]);
    b
}
struct Fixture {
    path: PathBuf,
    bytes: Vec<u8>,
    regions: Vec<Region>,
}
impl Fixture {
    fn new(machine: u16) -> Self {
        let path = std::env::temp_dir().join(format!(
            "hoi4-translated-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        let bytes = elf(machine);
        fs::write(&path, &bytes).unwrap();
        let path = fs::canonicalize(path).unwrap();
        let m = fs::metadata(&path).unwrap();
        let regions = vec![Region {
            start: 0x20000,
            end: 0x21000,
            readable: true,
            permissions: *b"r-xp",
            offset: 0,
            device: (
                u64::from(libc::major(m.dev())),
                u64::from(libc::minor(m.dev())),
            ),
            inode: m.ino(),
            path: Some(path.to_str().unwrap().into()),
        }];
        Self {
            path,
            bytes,
            regions,
        }
    }
    fn image(&self) -> GuestImage {
        let expected = ExpectedBuild::GeneratedChild {
            build_id: elf_build_id(&self.bytes).unwrap(),
            sha256: format!("{:x}", Sha256::digest(&self.bytes)),
        };
        let bytes = executable_bytes(&self.path, &expected).unwrap();
        let file = File::open(&self.path).unwrap();
        let identity = FileIdentity::of(&file.metadata().unwrap());
        GuestImage::from_verified(self.path.clone(), file, identity, &bytes, &self.regions).unwrap()
    }
    fn read(&self, address: u64, size: usize) -> Result<Vec<u8>> {
        Ok(self.bytes[(address - 0x20000) as usize..(address - 0x20000) as usize + size].to_vec())
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.path);
    }
}
#[test]
fn translator_requires_fixed_production_pin_and_aarch64_elf_not_caller_selected_hash() {
    let fixture = Fixture::new(183);
    let generated = format!("{:x}", Sha256::digest(&fixture.bytes));
    assert!(
        translator_expected(&fixture.path, TranslatorBuild::Generated(generated.clone())).is_ok()
    );
    assert_eq!(
        verify_translator(&fixture.path, &generated)
            .unwrap_err()
            .code,
        "unsupported_translator"
    );
    assert_eq!(
        verify_translator(&fixture.path, FEX_SHA256)
            .unwrap_err()
            .code,
        "unsupported_translator"
    );
    assert!(!is_reviewed_translator(&fixture.path).unwrap());
    let x86 = Fixture::new(62);
    let hash = format!("{:x}", Sha256::digest(&x86.bytes));
    assert!(!is_reviewed_translator(&x86.path).unwrap());
    assert_eq!(
        translator_expected(&x86.path, TranslatorBuild::Generated(hash))
            .unwrap_err()
            .code,
        "unsupported_translator"
    );
    assert_eq!(
        verify_executable(&fixture.path).unwrap_err().code,
        "unsupported_build"
    );
}
#[test]
fn guest_mapping_header_offsets_permissions_and_full_identity_are_never_rebound() {
    let fixture = Fixture::new(62);
    let image = fixture.image();
    assert_eq!(image.base(), 0x20000);
    image
        .check_with(&fixture.regions, |a, s| fixture.read(a, s))
        .unwrap();
    for change in 0..8 {
        let mut changed = fixture.regions.clone();
        match change {
            0 => changed[0].inode += 1,
            1 => changed[0].device.0 += 1,
            2 => changed[0].offset = 4096,
            3 => {
                changed[0].start += 4096;
                changed[0].end += 4096;
            }
            4 => changed[0].path = Some("/different".into()),
            5 => changed[0].permissions = *b"rwxp",
            6 => changed.clear(),
            _ => changed.push(changed[0].clone()),
        }
        assert!(
            image
                .check_with(&changed, |_, _| panic!(
                    "Changed maps must refuse before memory reads"
                ))
                .is_err(),
            "change={change}"
        );
    }
    for offset in [0, 64] {
        assert_eq!(
            image
                .check_with(&fixture.regions, |a, s| {
                    let mut b = fixture.read(a, s)?;
                    if a == image.base + offset {
                        b[0] ^= 1;
                    }
                    Ok(b)
                })
                .unwrap_err()
                .code,
            "ownership_changed"
        );
    }
    assert_eq!(
        image
            .check_with(&fixture.regions, |_, _| Err(Error::new(
                "permission_denied",
                "Generated denied read"
            )))
            .unwrap_err()
            .code,
        "permission_denied"
    );
}
#[test]
fn loader_vma_split_and_permission_tightening_preserve_exact_file_coverage() {
    let fixture = Fixture::new(62);
    let image = fixture.image();
    let mut split = fixture.regions.clone();
    split[0].end = 0x20800;
    let mut tail = split[0].clone();
    tail.start = 0x20800;
    tail.end = 0x21000;
    tail.offset = 0x800;
    tail.permissions = *b"r--p";
    split.push(tail);
    image.check_with(&split, |a, s| fixture.read(a, s)).unwrap();
    split[1].permissions = *b"rw-p";
    assert_eq!(
        image
            .check_with(&split, |_, _| panic!("Permission gain must refuse first"))
            .unwrap_err()
            .code,
        "ownership_changed"
    );
    split[1].permissions = *b"r--p";
    split[1].offset += 8;
    assert_eq!(
        image
            .check_with(&split, |_, _| panic!("Changed offset must refuse first"))
            .unwrap_err()
            .code,
        "ownership_changed"
    );
}
#[test]
fn initial_missing_loads_are_typed_loading_but_wrong_maps_and_later_loss_are_strict() {
    let mut fixture = Fixture::new(62);
    fixture.bytes.resize(8192, 0);
    fixture.bytes[56..58].copy_from_slice(&3u16.to_le_bytes());
    fixture.bytes[68..72].copy_from_slice(&4u32.to_le_bytes());
    fixture.bytes[176..180].copy_from_slice(&1u32.to_le_bytes());
    fixture.bytes[180..184].copy_from_slice(&5u32.to_le_bytes());
    fixture.bytes[184..192].copy_from_slice(&4096u64.to_le_bytes());
    fixture.bytes[192..200].copy_from_slice(&8192u64.to_le_bytes());
    fixture.bytes[208..216].copy_from_slice(&4096u64.to_le_bytes());
    fixture.bytes[216..224].copy_from_slice(&4096u64.to_le_bytes());
    fs::write(&fixture.path, &fixture.bytes).unwrap();
    fixture.regions[0].permissions = *b"r--p";
    let mut code = fixture.regions[0].clone();
    code.start = 0x22000;
    code.end = 0x23000;
    code.offset = 4096;
    code.permissions = *b"r-xp";
    let build = |regions: &[Region]| {
        let file = File::open(&fixture.path).unwrap();
        let identity = FileIdentity::of(&file.metadata().unwrap());
        GuestImage::from_verified(
            fixture.path.clone(),
            file,
            identity,
            &fixture.bytes,
            regions,
        )
    };
    assert_eq!(build(&[]).err().unwrap().code, "translated_image_loading");
    assert_eq!(
        build(&fixture.regions).err().unwrap().code,
        "translated_image_loading"
    );
    let mut partial = fixture.regions.clone();
    let mut half = code.clone();
    half.end = 0x22800;
    partial.push(half);
    assert_eq!(
        build(&partial).err().unwrap().code,
        "translated_image_loading"
    );
    let mut reservation = code.clone();
    reservation.inode = 0;
    reservation.device = (0, 0);
    reservation.path = None;
    reservation.offset = 0;
    reservation.permissions = *b"---p";
    reservation.readable = false;
    let mut reserved = fixture.regions.clone();
    reserved.push(reservation.clone());
    assert_eq!(
        build(&reserved).err().unwrap().code,
        "translated_image_loading"
    );
    reservation.end = 0x22800;
    reserved[1] = reservation;
    assert_eq!(
        build(&reserved).err().unwrap().code,
        "translated_image_loading"
    );
    let mut matching_tail = code.clone();
    matching_tail.start = 0x22800;
    matching_tail.offset = 0x1800;
    let mut next_mapped = reserved.clone();
    next_mapped.push(matching_tail);
    assert_eq!(
        build(&next_mapped).err().unwrap().code,
        "translated_image_loading",
        "A correctly mapped tail ends the missing reservation gap"
    );
    next_mapped[2].offset += 8;
    assert_eq!(build(&next_mapped).err().unwrap().code, "invalid_maps");
    let mut foreign = code.clone();
    foreign.start = 0x22800;
    foreign.offset = 0x1800;
    foreign.path = Some("/foreign-second-half".into());
    foreign.inode += 1;
    reserved.push(foreign);
    assert_eq!(
        build(&reserved).err().unwrap().code,
        "invalid_maps",
        "A foreign overlap after the gap cursor must never be retried as loading"
    );
    let mut complete = fixture.regions.clone();
    complete.push(code.clone());
    let image = build(&complete).unwrap();
    assert_eq!(
        image
            .check_with(&fixture.regions, |_, _| panic!(
                "Bound reader must refuse lost coverage before reading"
            ))
            .unwrap_err()
            .code,
        "ownership_changed"
    );
    for change in 0..5 {
        let mut wrong = complete.clone();
        match change {
            0 => wrong[1].offset += 4096,
            1 => wrong[1].inode += 1,
            2 => wrong[1].permissions = *b"rwxp",
            3 => wrong[1].permissions = *b"r--p",
            _ => wrong.push(wrong[0].clone()),
        }
        let error = build(&wrong).err().unwrap();
        assert_ne!(error.code, "translated_image_loading", "change={change}");
    }
    let mut occupied = fixture.regions.clone();
    code.path = Some("/different-file".into());
    code.inode += 1;
    occupied.push(code);
    assert_eq!(build(&occupied).err().unwrap().code, "invalid_maps");
}
#[test]
fn production_guest_pin_cannot_be_selected_by_translated_manifest() {
    let fixture = Fixture::new(62);
    let image = TranslatedImage {
        executable: fixture.path.to_str().unwrap().into(),
        translator_sha256: FEX_SHA256.into(),
    };
    assert_eq!(
        GuestImage::capture(1, &image, &ExpectedBuild::VerifiedGame, &fixture.regions)
            .err()
            .unwrap()
            .code,
        "unsupported_build"
    );
}
#[test]
fn maps_only_duplicate_probe_detects_any_exact_file_alias_without_memory_reads() {
    use std::os::fd::AsRawFd;
    let fixture = Fixture::new(62);
    let pid = std::process::id() as i32;
    assert!(!guest_mapping_present(pid, &fixture.path).unwrap());
    let file = File::open(&fixture.path).unwrap();
    // SAFETY: two private read-only mappings of this generated one-page file;
    // pointers are never dereferenced and both are unmapped below.
    let first = unsafe {
        libc::mmap(
            std::ptr::null_mut(),
            4096,
            libc::PROT_READ,
            libc::MAP_PRIVATE,
            file.as_raw_fd(),
            0,
        )
    };
    let second = unsafe {
        libc::mmap(
            std::ptr::null_mut(),
            4096,
            libc::PROT_READ,
            libc::MAP_PRIVATE,
            file.as_raw_fd(),
            0,
        )
    };
    assert_ne!(first, libc::MAP_FAILED);
    assert_ne!(second, libc::MAP_FAILED);
    let present = guest_mapping_present(pid, &fixture.path);
    // SAFETY: release exactly the two successful owned mappings above.
    unsafe {
        assert_eq!(libc::munmap(first, 4096), 0);
        assert_eq!(libc::munmap(second, 4096), 0);
    }
    assert!(present.unwrap());
    assert!(!guest_mapping_present(pid, &fixture.path).unwrap());
    assert!(!guest_mapping_present(i32::MAX, &fixture.path).unwrap());
}
#[test]
fn guest_file_replacement_mutation_and_noncanonical_descriptor_are_strict() {
    let fixture = Fixture::new(62);
    let image = fixture.image();
    image.check_file().unwrap();
    let mut changed = fixture.bytes.clone();
    changed.push(0);
    fs::write(&fixture.path, changed).unwrap();
    assert_eq!(image.check_file().unwrap_err().code, "ownership_changed");
    assert!(canonical(Path::new("relative")).is_err());
    let other = Fixture::new(62);
    let file = File::open(&other.path).unwrap();
    let identity = FileIdentity::of(&file.metadata().unwrap());
    assert!(
        GuestImage::from_verified(other.path.clone(), file, identity, &other.bytes, &[]).is_err()
    );
}
#[test]
fn only_validated_aarch64_kernel_uses_wider_at_random_bound_x86_stays_strict() {
    let address = (1u64 << 48) - 16;
    let aux = [25u64, address, 0, 0]
        .into_iter()
        .flat_map(u64::to_le_bytes)
        .collect::<Vec<_>>();
    assert_eq!(
        KernelArch::from_elf(&elf(183)).unwrap(),
        KernelArch::Aarch64
    );
    assert_eq!(
        parse_random_address_arch(&aux, KernelArch::Aarch64).unwrap(),
        address
    );
    assert_eq!(
        parse_random_address_arch(&aux, KernelArch::X86_64)
            .unwrap_err()
            .code,
        "invalid_exec_witness"
    );
    let aux = [25u64, (1u64 << 52) - 15, 0, 0]
        .into_iter()
        .flat_map(u64::to_le_bytes)
        .collect::<Vec<_>>();
    assert!(parse_random_address_arch(&aux, KernelArch::Aarch64).is_err());
}

#[cfg(target_arch = "aarch64")]
mod generated;
