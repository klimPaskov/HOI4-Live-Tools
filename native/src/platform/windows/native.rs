//! Windows 10+ x64 metadata/read implementation. All handles are RAII and
//! non-inheritable; only query/read/synchronize process rights are requested.
//! Dormant: no production build profile or server entry point can call reads.
use super::*;
use sha2::{Digest, Sha256};
use std::{
    ffi::c_void,
    fs::{File, OpenOptions},
    mem::{size_of, zeroed},
    os::windows::{
        fs::{FileExt, OpenOptionsExt},
        io::{AsRawHandle, FromRawHandle, OwnedHandle},
    },
    ptr::null_mut,
};

type Handle = *mut c_void;
#[repr(C)]
struct FileTime {
    low: u32,
    high: u32,
}
#[repr(C)]
struct FileId {
    volume: u64,
    id: [u8; 16],
}
#[repr(C)]
struct FileTag {
    attributes: u32,
    tag: u32,
}
#[repr(C)]
struct MemoryBasic {
    base: Handle,
    allocation: Handle,
    allocation_protect: u32,
    partition: u16,
    region_size: usize,
    state: u32,
    protect: u32,
    kind: u32,
}
#[repr(C)]
struct ProcessEntry {
    size: u32,
    usage: u32,
    pid: u32,
    heap: usize,
    module: u32,
    threads: u32,
    parent: u32,
    priority: i32,
    flags: u32,
    name: [u16; 260],
}
#[repr(C)]
struct ModuleEntry {
    size: u32,
    module: u32,
    pid: u32,
    global_usage: u32,
    process_usage: u32,
    base: Handle,
    base_size: u32,
    handle: Handle,
    name: [u16; 256],
    path: [u16; 260],
}
#[link(name = "kernel32")]
unsafe extern "system" {
    fn OpenProcess(rights: u32, inherit: i32, pid: u32) -> Handle;
    fn GetCurrentProcess() -> Handle;
    fn GetProcessId(process: Handle) -> u32;
    fn GetProcessTimes(
        process: Handle,
        creation: *mut FileTime,
        exit: *mut FileTime,
        kernel: *mut FileTime,
        user: *mut FileTime,
    ) -> i32;
    fn QueryFullProcessImageNameW(
        process: Handle,
        flags: u32,
        buffer: *mut u16,
        size: *mut u32,
    ) -> i32;
    fn WaitForSingleObject(handle: Handle, milliseconds: u32) -> u32;
    fn GetModuleHandleW(name: *const u16) -> Handle;
    fn GetProcAddress(
        module: Handle,
        name: *const u8,
    ) -> Option<unsafe extern "system" fn() -> isize>;
    fn CreateToolhelp32Snapshot(flags: u32, pid: u32) -> Handle;
    fn Process32FirstW(snapshot: Handle, entry: *mut ProcessEntry) -> i32;
    fn Process32NextW(snapshot: Handle, entry: *mut ProcessEntry) -> i32;
    fn Module32FirstW(snapshot: Handle, entry: *mut ModuleEntry) -> i32;
    fn Module32NextW(snapshot: Handle, entry: *mut ModuleEntry) -> i32;
    fn GetFileInformationByHandleEx(
        file: Handle,
        class: u32,
        buffer: *mut c_void,
        size: u32,
    ) -> i32;
    fn VirtualQueryEx(
        process: Handle,
        address: *const c_void,
        buffer: *mut MemoryBasic,
        size: usize,
    ) -> usize;
    fn ReadProcessMemory(
        process: Handle,
        address: *const c_void,
        buffer: *mut c_void,
        size: usize,
        read: *mut usize,
    ) -> i32;
}
#[link(name = "advapi32")]
unsafe extern "system" {
    fn OpenProcessToken(process: Handle, rights: u32, token: *mut Handle) -> i32;
    fn GetTokenInformation(
        token: Handle,
        class: u32,
        buffer: *mut c_void,
        size: u32,
        returned: *mut u32,
    ) -> i32;
}
fn os_error(label: &str) -> Error {
    error(
        "windows_api",
        &format!("{label}: {}", std::io::Error::last_os_error()),
    )
}
fn owned(handle: Handle) -> Result<OwnedHandle> {
    if handle.is_null() || handle as isize == -1 {
        return Err(os_error("Invalid read-only handle"));
    }
    // SAFETY: successful API results transfer one real, closable handle here.
    Ok(unsafe { OwnedHandle::from_raw_handle(handle) })
}
fn wchar(text: &[u16]) -> Result<String> {
    let end = text
        .iter()
        .position(|v| *v == 0)
        .ok_or_else(|| error("windows_api", "Unterminated bounded metadata"))?;
    String::from_utf16(&text[..end]).map_err(|_| error("windows_api", "Invalid UTF-16 metadata"))
}
fn token(process: Handle) -> Result<OwnedHandle> {
    let mut handle = null_mut();
    // SAFETY: valid process handle and one writable output; TOKEN_QUERY only.
    if unsafe { OpenProcessToken(process, 8, &mut handle) } == 0 {
        return Err(os_error("OpenProcessToken"));
    }
    owned(handle)
}
fn token_information(token: &OwnedHandle, class: u32) -> Result<(Vec<usize>, usize)> {
    let mut count = 0;
    // SAFETY: documented size query; no output buffer is supplied.
    unsafe { GetTokenInformation(token.as_raw_handle(), class, null_mut(), 0, &mut count) };
    if !(4..=65536).contains(&count) {
        return Err(error("windows_api", "Token information exceeds bound"));
    }
    let mut words = vec![0usize; (count as usize).div_ceil(size_of::<usize>())];
    let mut actual = 0;
    // SAFETY: aligned initialized storage has at least the queried byte count.
    if unsafe {
        GetTokenInformation(
            token.as_raw_handle(),
            class,
            words.as_mut_ptr().cast(),
            count,
            &mut actual,
        )
    } == 0
    {
        return Err(os_error("GetTokenInformation"));
    }
    if actual == 0 || actual > count {
        return Err(error("windows_api", "Invalid returned token length"));
    }
    Ok((words, actual as usize))
}
fn owner(process: Handle) -> Result<(Sid, bool)> {
    let t = token(process)?;
    let (words, len) = token_information(&t, 1)?; // TokenUser / SID_AND_ATTRIBUTES
    if len < size_of::<usize>() + 4 {
        return Err(error("windows_api", "Truncated TOKEN_USER"));
    }
    let start = words.as_ptr() as usize;
    // SAFETY: first machine word of TOKEN_USER is the SID pointer; read only as
    // an integer until its complete referent has been bounded inside this buffer.
    let address = unsafe { std::ptr::read_unaligned(words.as_ptr().cast::<usize>()) };
    let end = start
        .checked_add(len)
        .ok_or_else(|| error("windows_api", "Token buffer overflow"))?;
    if address < start || address.checked_add(8).is_none_or(|v| v > end) {
        return Err(error("windows_api", "SID is outside TOKEN_USER"));
    }
    // SAFETY: eight bytes have just been bounded in owned initialized storage.
    let prefix = unsafe { std::slice::from_raw_parts(address as *const u8, 8) };
    let size = 8 + usize::from(prefix[1]) * 4;
    if size > 68 || address.checked_add(size).is_none_or(|v| v > end) {
        return Err(error("windows_api", "Truncated or oversized SID"));
    }
    // SAFETY: the complete SID has been bounded inside the token buffer.
    let sid = Sid::new(unsafe { std::slice::from_raw_parts(address as *const u8, size) }.to_vec())?;
    let (elevation, bytes) = token_information(&t, 20)?; // TOKEN_ELEVATION
    if bytes != 4 {
        return Err(error("windows_api", "Malformed TOKEN_ELEVATION"));
    }
    // SAFETY: exactly four initialized bytes, naturally aligned.
    let value = unsafe { std::ptr::read_unaligned(elevation.as_ptr().cast::<u32>()) };
    if value > 1 {
        return Err(error("windows_api", "Invalid elevation flag"));
    }
    Ok((sid, value != 0))
}
type ArchitectureQuery = unsafe extern "system" fn(Handle, *mut u16, *mut u16) -> i32;
fn architecture_query() -> Result<ArchitectureQuery> {
    let library: Vec<u16> = "kernel32.dll".encode_utf16().chain(Some(0)).collect();
    // SAFETY: fixed terminated system-module name; the resident module is
    // borrowed, never loaded from a caller path or released by this adapter.
    let module = unsafe { GetModuleHandleW(library.as_ptr()) };
    if module.is_null() {
        return Err(os_error("GetModuleHandleW"));
    }
    // SAFETY: fixed documented function name in the resident system module.
    let function = unsafe { GetProcAddress(module, c"IsWow64Process2".as_ptr().cast()) }
        .ok_or_else(|| {
            error(
                "platform_unavailable",
                "IsWow64Process2 unavailable; native x64 association unverified",
            )
        })?;
    // SAFETY: this exact named export has the published BOOL(HANDLE, USHORT*,
    // USHORT*) ABI. Missing APIs refuse here rather than break unbound startup.
    Ok(unsafe {
        std::mem::transmute::<unsafe extern "system" fn() -> isize, ArchitectureQuery>(function)
    })
}
fn handle_identity(process: &OwnedHandle) -> Result<(u32, u64)> {
    let h = process.as_raw_handle();
    // SAFETY: SYNCHRONIZE was requested; zero timeout never blocks.
    if unsafe { WaitForSingleObject(h, 0) } != 0x102 {
        return Err(changed());
    }
    // SAFETY: a live retained query handle; no PID is reopened here.
    let pid = unsafe { GetProcessId(h) };
    if pid == 0 {
        return Err(os_error("GetProcessId"));
    }
    // SAFETY: integer FILETIME outputs permit zero initialization.
    let mut times: [FileTime; 4] = unsafe { zeroed() };
    let [creation, exit, kernel, user] = &mut times;
    // SAFETY: four disjoint writable FILETIME outputs and a retained handle.
    if unsafe { GetProcessTimes(h, creation, exit, kernel, user) } == 0 {
        return Err(os_error("GetProcessTimes"));
    }
    let created = (u64::from(creation.high) << 32) | u64::from(creation.low);
    // SAFETY: same retained handle, no wait or process-control action.
    if unsafe { WaitForSingleObject(h, 0) } != 0x102 {
        return Err(changed());
    }
    Ok((pid, created))
}
fn file_identity(file: &File) -> Result<FileIdentity> {
    // SAFETY: these plain integer structures have valid all-zero initialization.
    let mut id: FileId = unsafe { zeroed() };
    let mut tag: FileTag = unsafe { zeroed() };
    // SAFETY: valid file and correctly sized writable structure.
    if unsafe {
        GetFileInformationByHandleEx(
            file.as_raw_handle(),
            18,
            (&mut id as *mut FileId).cast(),
            size_of::<FileId>() as u32,
        )
    } == 0
    {
        return Err(os_error("FileIdInfo"));
    }
    // SAFETY: valid file and correctly sized writable structure.
    if unsafe {
        GetFileInformationByHandleEx(
            file.as_raw_handle(),
            9,
            (&mut tag as *mut FileTag).cast(),
            size_of::<FileTag>() as u32,
        )
    } == 0
    {
        return Err(os_error("FileAttributeTagInfo"));
    }
    if tag.attributes & (0x10 | 0x400) != 0 || id.id == [0; 16] {
        return Err(error(
            "ownership_changed",
            "Regular non-reparse executable file required",
        ));
    }
    Ok(FileIdentity {
        volume: id.volume,
        id: id.id,
    })
}
pub(crate) struct Native;
pub(crate) struct NativeImage(File);
impl Native {
    fn executable(path: &str) -> Result<File> {
        if !valid_path(path) {
            return Err(error("invalid_session", "Local exact Win32 path required"));
        }
        // FILE_SHARE_READ excludes writers and deletion for this retained handle.
        let f = OpenOptions::new()
            .read(true)
            .share_mode(1)
            .custom_flags(0x0020_0000)
            .open(path)?;
        file_identity(&f)?;
        Ok(f)
    }
    fn parent(pid: u32) -> Result<u32> {
        // SAFETY: read-only process metadata snapshot, no process rights/control.
        let snapshot = owned(unsafe { CreateToolhelp32Snapshot(2, 0) })?;
        // SAFETY: integer/pointer-free Win32 metadata, initialized per API.
        let mut e: ProcessEntry = unsafe { zeroed() };
        e.size = size_of::<ProcessEntry>() as u32;
        // SAFETY: valid owned snapshot and writable structure of the advertised size.
        let mut more = unsafe { Process32FirstW(snapshot.as_raw_handle(), &mut e) };
        let mut result = None;
        for _ in 0..65536 {
            if more == 0 {
                if std::io::Error::last_os_error().raw_os_error() != Some(18) {
                    return Err(os_error("Process32 iteration"));
                }
                return result.ok_or_else(|| {
                    error("ownership_changed", "Original PID absent from snapshot")
                });
            }
            if e.pid == pid && result.replace(e.parent).is_some() {
                return Err(changed());
            }
            // SAFETY: valid owned snapshot and same bounded output structure.
            more = unsafe { Process32NextW(snapshot.as_raw_handle(), &mut e) };
        }
        Err(error(
            "read_bound",
            "Process metadata census exceeds observer bound",
        ))
    }
    fn module(pid: u32, path: &str) -> Result<(u64, u32)> {
        // SAFETY: read-only module snapshot. ERROR_BAD_LENGTH is refused, not retried.
        let snapshot = owned(unsafe { CreateToolhelp32Snapshot(8 | 16, pid) })?;
        // SAFETY: all-zero C metadata with valid pointers until populated by Win32.
        let mut e: ModuleEntry = unsafe { zeroed() };
        e.size = size_of::<ModuleEntry>() as u32;
        // SAFETY: valid owned snapshot and correctly sized output structure.
        let mut more = unsafe { Module32FirstW(snapshot.as_raw_handle(), &mut e) };
        let mut found = None;
        for _ in 0..4096 {
            if more == 0 {
                if std::io::Error::last_os_error().raw_os_error() != Some(18) {
                    return Err(os_error("Module32 iteration"));
                }
                return found
                    .ok_or_else(|| error("ownership_changed", "Exact executable module missing"));
            }
            if wchar(&e.path)? == path
                && (e.pid != pid || found.replace((e.base as u64, e.base_size)).is_some())
            {
                return Err(changed());
            }
            // SAFETY: valid owned snapshot and correctly sized output structure.
            more = unsafe { Module32NextW(snapshot.as_raw_handle(), &mut e) };
        }
        Err(error("read_bound", "Module census exceeds observer bound"))
    }
}
impl Api for Native {
    type Process = OwnedHandle;
    type Image = NativeImage;
    fn current_owner(&self) -> Result<(Sid, bool)> {
        // SAFETY: borrowed pseudo-handle, never closed or retained as OwnedHandle.
        owner(unsafe { GetCurrentProcess() })
    }
    fn open_process(&self, pid: u32, rights: u32) -> Result<OwnedHandle> {
        if pid == 0 || !matches!(rights, QUERY | QUERY_READ) {
            return Err(error(
                "invalid_access",
                "Only query/read/synchronize rights allowed",
            ));
        }
        // SAFETY: documented API; handle is non-inheritable and read/query only.
        owned(unsafe { OpenProcess(rights, 0, pid) })
    }
    fn inspect(&self, process: &OwnedHandle) -> Result<Metadata> {
        let h = process.as_raw_handle();
        let (pid, creation_filetime) = handle_identity(process)?;
        let mut pm = 0;
        let mut nm = 0;
        let query = architecture_query()?;
        // SAFETY: exact documented function, valid handle and writable outputs.
        if unsafe { query(h, &mut pm, &mut nm) } == 0 {
            return Err(os_error("IsWow64Process2"));
        }
        if pm != 0 || nm != 0x8664 || size_of::<usize>() != 8 {
            return Err(error(
                "unsupported_build",
                "Native x64 Windows process required",
            ));
        }
        let mut path = vec![0u16; 32768];
        let mut count = 32768;
        // SAFETY: buffer contains count writable UTF-16 units; Win32 path mode.
        if unsafe { QueryFullProcessImageNameW(h, 0, path.as_mut_ptr(), &mut count) } == 0 {
            return Err(os_error("QueryFullProcessImageNameW"));
        }
        if count == 0 || count as usize >= path.len() {
            return Err(error("windows_api", "Executable metadata exceeds bound"));
        }
        path[count as usize] = 0;
        let executable = wchar(&path[..=count as usize])?;
        let file = file_identity(&Self::executable(&executable)?)?;
        let (owner, elevated) = owner(h)?;
        if elevated {
            return Err(error(
                "ownership_changed",
                "Elevated process not supported by ordinary-user reader",
            ));
        }
        let parent_pid = Self::parent(pid)?;
        let (image_base, image_size) = Self::module(pid, &executable)?;
        // SAFETY: recheck borrowed live handle after the metadata census.
        if unsafe { WaitForSingleObject(h, 0) } != 0x102 {
            return Err(changed());
        }
        Ok(Metadata {
            identity: Identity {
                pid,
                creation_filetime,
                executable,
                owner,
                file,
            },
            parent_pid,
            image_base,
            image_size,
        })
    }
    fn check_handle(&self, process: &OwnedHandle, expected: &Identity) -> Result<()> {
        if handle_identity(process)? != (expected.pid, expected.creation_filetime) {
            return Err(changed());
        }
        Ok(())
    }
    fn open_image(&self, path: &str) -> Result<NativeImage> {
        Ok(NativeImage(Self::executable(path)?))
    }
    fn image(&self, image: &NativeImage) -> Result<Image> {
        let before = file_identity(&image.0)?;
        let size = image.0.metadata()?.len();
        if size == 0 || size > MAX_IMAGE {
            return Err(error(
                "read_bound",
                "Executable file exceeds observer policy",
            ));
        }
        let mut hash = Sha256::new();
        let mut offset = 0u64;
        let mut header = Vec::new();
        let mut buf = vec![0; 65536];
        while offset < size {
            let n = (size - offset).min(buf.len() as u64) as usize;
            let actual = image.0.seek_read(&mut buf[..n], offset)?;
            if actual != n {
                return Err(error(
                    "short_read",
                    "Incomplete pinned executable file read",
                ));
            }
            hash.update(&buf[..n]);
            let remaining = HEADER_BOUND - header.len();
            header.extend_from_slice(&buf[..n.min(remaining)]);
            offset += n as u64;
        }
        if file_identity(&image.0)? != before || image.0.metadata()?.len() != size {
            return Err(changed());
        }
        Ok(Image {
            file: before,
            size,
            header,
            sha256: hash.finalize().into(),
        })
    }
    fn file_identity(&self, image: &NativeImage) -> Result<FileIdentity> {
        file_identity(&image.0)
    }
    fn region(&self, process: &OwnedHandle, address: u64) -> Result<Region> {
        // SAFETY: zero-initialized C metadata and no local or remote pointer is followed.
        let mut info: MemoryBasic = unsafe { zeroed() };
        // SAFETY: documented metadata query, correctly sized output buffer.
        let count = unsafe {
            VirtualQueryEx(
                process.as_raw_handle(),
                address as *const c_void,
                &mut info,
                size_of::<MemoryBasic>(),
            )
        };
        if count != size_of::<MemoryBasic>() {
            return Err(os_error("VirtualQueryEx"));
        }
        Ok(Region {
            start: info.base as u64,
            allocation: info.allocation as u64,
            size: info.region_size as u64,
            state: info.state,
            protection: info.protect,
            kind: info.kind,
        })
    }
    fn read(&self, process: &OwnedHandle, address: u64, size: usize) -> Result<Vec<u8>> {
        range(address, size)?;
        let mut bytes = vec![0; size];
        let mut count = 0;
        // SAFETY: exact initialized local buffer, validated numeric remote span;
        // the OS performs read-only copying and verifies accessible pages.
        let success = unsafe {
            ReadProcessMemory(
                process.as_raw_handle(),
                address as *const c_void,
                bytes.as_mut_ptr().cast(),
                size,
                &mut count,
            )
        };
        if success == 0 {
            return Err(os_error("ReadProcessMemory"));
        }
        if count != size {
            return Err(error("short_read", "Partial ReadProcessMemory transfer"));
        }
        Ok(bytes)
    }
}
#[cfg(test)]
mod child_tests;
