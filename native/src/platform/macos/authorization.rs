//! Decide, without displaying OS UI, whether macOS will hand this process a
//! task port, then request the port within a bounded time.
//! Nothing here enables Developer Tools, alters policy or collects credentials.
//!
//! macOS evaluates a different authorization right depending on how the
//! requesting binary is signed. A binary holding the debugger entitlement is
//! judged on `system.privilege.taskport.debug`, which passes with no password
//! for a member of `_developer` while Developer mode is on and never expires.
//! Any other binary is judged on `system.privilege.taskport`, which needs the
//! owner's password and lapses after about ten hours. The preflight asks about
//! the right that applies to the running binary, so it neither refuses access
//! macOS would grant nor lets an unsigned build raise the password dialog.
use crate::error::{Error, Result};
use std::ffi::{c_char, c_void, CStr};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::time::Duration;

#[repr(C)]
struct Item {
    name: *const c_char,
    value_length: usize,
    value: *mut c_void,
    flags: u32,
}
#[repr(C)]
struct Rights {
    count: u32,
    items: *mut Item,
}

// Security.framework Authorization.h: ExtendRights verifies the requested
// right; DestroyRights with a null output reference leaves session state alone.
// InteractionAllowed and PreAuthorize must never be included.
const QUERY_FLAGS: u32 = (1 << 1) | (1 << 3);
const TASK_PORT: &CStr = c"system.privilege.taskport";
const TASK_PORT_DEBUG: &CStr = c"system.privilege.taskport.debug";
const DEBUGGER_ENTITLEMENT: &CStr = c"com.apple.security.cs.debugger";
const UTF8: u32 = 0x0800_0100;
/// A granted request answers in milliseconds; anything slower is waiting on
/// a system dialog this tool must not answer.
const ACQUIRE_TIMEOUT: Duration = Duration::from_secs(5);

#[link(name = "Security", kind = "framework")]
extern "C" {
    fn AuthorizationCreate(
        rights: *const Rights,
        environment: *const Rights,
        flags: u32,
        authorization: *mut *mut c_void,
    ) -> i32;
    fn SecTaskCreateFromSelf(allocator: *const c_void) -> *const c_void;
    fn SecTaskCopyValueForEntitlement(
        task: *const c_void,
        entitlement: *const c_void,
        error: *mut *const c_void,
    ) -> *const c_void;
}
#[link(name = "CoreFoundation", kind = "framework")]
extern "C" {
    fn CFStringCreateWithCString(
        allocator: *const c_void,
        text: *const c_char,
        encoding: u32,
    ) -> *const c_void;
    fn CFGetTypeID(value: *const c_void) -> usize;
    fn CFBooleanGetTypeID() -> usize;
    fn CFBooleanGetValue(value: *const c_void) -> bool;
    fn CFRelease(value: *const c_void);
}

/// The system facts the decision depends on, behind a seam for tests.
trait Policy {
    /// Whether the running binary's own code signature carries the debugger
    /// entitlement, as the kernel recorded it when the process started.
    fn holds_debugger_entitlement(&self) -> bool;
    /// Status of a no-UI query for `right`; 0 means it is held.
    fn query(&self, right: &CStr) -> i32;
}

struct System;
impl Policy for System {
    fn holds_debugger_entitlement(&self) -> bool {
        // SAFETY: every reference created here is checked for null and
        // released once; the entitlement name is a live C string constant.
        unsafe {
            let task = SecTaskCreateFromSelf(std::ptr::null());
            if task.is_null() {
                return false;
            }
            let name =
                CFStringCreateWithCString(std::ptr::null(), DEBUGGER_ENTITLEMENT.as_ptr(), UTF8);
            if name.is_null() {
                CFRelease(task);
                return false;
            }
            let value = SecTaskCopyValueForEntitlement(task, name, std::ptr::null_mut());
            let held = !value.is_null()
                && CFGetTypeID(value) == CFBooleanGetTypeID()
                && CFBooleanGetValue(value);
            if !value.is_null() {
                CFRelease(value);
            }
            CFRelease(name);
            CFRelease(task);
            held
        }
    }
    fn query(&self, right: &CStr) -> i32 {
        query_right(right, |rights, flags| {
            // SAFETY: rights and its single item remain live throughout this
            // synchronous call. Null environment/output are documented options.
            unsafe { AuthorizationCreate(rights, std::ptr::null(), flags, std::ptr::null_mut()) }
        })
    }
}

fn query_right(right: &CStr, authorize: impl FnOnce(&Rights, u32) -> i32) -> i32 {
    let mut item = Item {
        name: right.as_ptr(),
        value_length: 0,
        value: std::ptr::null_mut(),
        flags: 0,
    };
    let rights = Rights {
        count: 1,
        items: &mut item,
    };
    authorize(&rights, QUERY_FLAGS)
}

/// Ask about the one right macOS will evaluate for this binary. Returns the
/// right's name when it is held without interaction.
fn preflight(policy: &impl Policy) -> Result<&'static str> {
    let entitled = policy.holds_debugger_entitlement();
    let right = if entitled { TASK_PORT_DEBUG } else { TASK_PORT };
    let name = right.to_str().unwrap_or("system.privilege.taskport");
    let status = policy.query(right);
    if status == 0 {
        return Ok(name);
    }
    let remedy = if entitled {
        "This build is signed with the debugger entitlement, so macOS decides on that right: it is granted with no password and no expiry once Developer mode is on (the owner runs `sudo DevToolsSecurity -enable` once) and the user is a member of the `_developer` group (`sudo dseditgroup -o edit -a \"$USER\" -t user _developer`, then log in again)"
    } else {
        "This build is not signed with the `com.apple.security.cs.debugger` entitlement, so macOS decides on the password-protected right. Sign the tool with `native/macos/debugger.entitlements` (with Developer mode on and `_developer` membership it then needs no renewal), or renew the right locally with `security authorize -u system.privilege.taskport` (it lapses after about 10 hours)"
    };
    Err(Error::new(
        "task_access_denied",
        format!("macOS task-port authorization is unavailable without user interaction: `{name}` is not held (status {status}). {remedy}. No permission prompt or game input was sent"),
    ))
}

enum Attempt {
    Waiting,
    Abandoned,
    Done(Result<u32>),
}

static ACQUIRING: AtomicBool = AtomicBool::new(false);

/// Preflight, then request the task port of `pid`. macOS has the last word:
/// a refusal is reported as `task_access_denied`. The whole exchange returns
/// within `ACQUIRE_TIMEOUT` even when the system holds the request open
/// behind a dialog, and a request still held open is never repeated.
pub(super) fn acquire_task_port(
    request: impl FnOnce() -> (i32, u32) + Send + 'static,
    release: fn(u32),
) -> Result<u32> {
    bounded(&ACQUIRING, ACQUIRE_TIMEOUT, release, move || {
        let right = preflight(&System)?;
        task_port(right, request())
    })
}

fn task_port(right: &str, (status, port): (i32, u32)) -> Result<u32> {
    if status == 0 {
        return Ok(port);
    }
    Err(Error::new(
        "task_access_denied",
        format!("macOS refused read-only task access (task_for_pid status {status}) although `{right}` is held. Check that the tool is signed with `native/macos/debugger.entitlements`, that Developer mode is on and that the user is in `_developer`; the game must run as the same user. The request was not retried and no permission prompt was answered"),
    ))
}

/// Run `work` on its own thread and wait at most `limit`. On a timeout the
/// caller gets an error at once; the thread stays parked in the system call,
/// `busy` keeps later calls from stacking more requests behind it, and a port
/// that arrives after the caller has left is released.
fn bounded(
    busy: &'static AtomicBool,
    limit: Duration,
    release: fn(u32),
    work: impl FnOnce() -> Result<u32> + Send + 'static,
) -> Result<u32> {
    if busy.swap(true, Ordering::SeqCst) {
        return Err(pending("an earlier request is still waiting on the system"));
    }
    let shared = Arc::new((Mutex::new(Attempt::Waiting), Condvar::new()));
    let worker = Arc::clone(&shared);
    let spawned = std::thread::Builder::new()
        .name("task-port".into())
        .spawn(move || {
            let result = work();
            let (state, ready) = &*worker;
            let mut state = state.lock().unwrap_or_else(|poison| poison.into_inner());
            if matches!(*state, Attempt::Abandoned) {
                if let Ok(port) = result {
                    release(port);
                }
            } else {
                *state = Attempt::Done(result);
                ready.notify_one();
            }
            busy.store(false, Ordering::SeqCst);
        });
    if spawned.is_err() {
        busy.store(false, Ordering::SeqCst);
        return Err(Error::new(
            "task_access_denied",
            "Could not start the task-port request; nothing was sent to the system",
        ));
    }
    let (state, ready) = &*shared;
    let guard = state.lock().unwrap_or_else(|poison| poison.into_inner());
    let (mut guard, _) = ready
        .wait_timeout_while(guard, limit, |state| matches!(state, Attempt::Waiting))
        .unwrap_or_else(|poison| poison.into_inner());
    match std::mem::replace(&mut *guard, Attempt::Abandoned) {
        Attempt::Done(result) => result,
        _ => Err(pending(&format!(
            "the system did not answer within {} s",
            limit.as_secs_f32()
        ))),
    }
}

fn pending(why: &str) -> Error {
    Error::new(
        "task_access_denied",
        format!("macOS task-port authorization is pending: {why}, most likely behind a system dialog only the owner can answer. Dismiss or answer it locally, then reconnect; this tool does not answer it and will not ask again while it is open"),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;

    struct Fake {
        entitled: bool,
        status: i32,
        asked: RefCell<Vec<String>>,
    }
    impl Fake {
        fn new(entitled: bool, status: i32) -> Self {
            Self {
                entitled,
                status,
                asked: RefCell::new(Vec::new()),
            }
        }
    }
    impl Policy for Fake {
        fn holds_debugger_entitlement(&self) -> bool {
            self.entitled
        }
        fn query(&self, right: &CStr) -> i32 {
            self.asked
                .borrow_mut()
                .push(right.to_str().unwrap().to_owned());
            self.status
        }
    }

    #[test]
    fn entitled_build_passes_on_the_debug_right_alone() {
        let fake = Fake::new(true, 0);
        assert_eq!(preflight(&fake).unwrap(), "system.privilege.taskport.debug");
        assert_eq!(*fake.asked.borrow(), ["system.privilege.taskport.debug"]);
    }

    #[test]
    fn entitled_build_without_the_debug_right_is_refused_with_the_one_time_setup() {
        let fake = Fake::new(true, -60007);
        let error = preflight(&fake).unwrap_err();
        assert_eq!(error.code, "task_access_denied");
        assert!(error
            .message
            .contains("`system.privilege.taskport.debug` is not held (status -60007)"));
        assert!(error.message.contains("DevToolsSecurity -enable"));
        assert!(error.message.contains("_developer"));
        assert!(!error.message.contains("security authorize -u"));
        assert!(error.message.contains("No permission prompt"));
        // The password-protected right is never consulted as a fallback.
        assert_eq!(*fake.asked.borrow(), ["system.privilege.taskport.debug"]);
    }

    #[test]
    fn unentitled_build_passes_on_the_plain_right_alone() {
        let fake = Fake::new(false, 0);
        assert_eq!(preflight(&fake).unwrap(), "system.privilege.taskport");
        assert_eq!(*fake.asked.borrow(), ["system.privilege.taskport"]);
    }

    #[test]
    fn unentitled_build_without_the_plain_right_is_refused_and_told_to_sign_or_renew() {
        let fake = Fake::new(false, -60007);
        let error = preflight(&fake).unwrap_err();
        assert_eq!(error.code, "task_access_denied");
        assert!(error
            .message
            .contains("`system.privilege.taskport` is not held (status -60007)"));
        assert!(error.message.contains("native/macos/debugger.entitlements"));
        assert!(error
            .message
            .contains("security authorize -u system.privilege.taskport"));
        // A held debug right must not let an unsigned build through: macOS
        // would answer its request with the password dialog.
        assert_eq!(*fake.asked.borrow(), ["system.privilege.taskport"]);
    }

    #[test]
    fn every_nonzero_status_is_a_capability_error_for_either_build() {
        for entitled in [true, false] {
            for status in [-60005, -60006, -60007, -60008, -1, 1] {
                let error = preflight(&Fake::new(entitled, status)).unwrap_err();
                assert_eq!(error.code, "task_access_denied");
                assert!(error.message.contains(&format!("(status {status})")));
            }
        }
    }

    #[test]
    fn query_names_one_right_without_ui_credentials_or_preauthorization() {
        for right in [TASK_PORT, TASK_PORT_DEBUG] {
            let status = query_right(right, |rights, flags| {
                assert_eq!(rights.count, 1);
                assert_eq!(flags, 10);
                assert_eq!(flags & ((1 << 0) | (1 << 4) | (1 << 9)), 0);
                // SAFETY: query_right supplies exactly one live stack item.
                let item = unsafe { &*rights.items };
                assert_eq!(unsafe { CStr::from_ptr(item.name) }, right);
                assert_eq!(item.value_length, 0);
                assert!(item.value.is_null());
                assert_eq!(item.flags, 0);
                -60007
            });
            assert_eq!(status, -60007);
        }
    }

    #[test]
    fn a_refused_port_is_reported_with_the_right_that_was_held() {
        assert_eq!(
            task_port("system.privilege.taskport.debug", (0, 77)).unwrap(),
            77
        );
        let error = task_port("system.privilege.taskport.debug", (5, 0)).unwrap_err();
        assert_eq!(error.code, "task_access_denied");
        assert!(error.message.contains("task_for_pid status 5"));
        assert!(error
            .message
            .contains("`system.privilege.taskport.debug` is held"));
        assert!(error.message.contains("not retried"));
    }

    #[test]
    fn an_unsigned_test_binary_does_not_claim_the_entitlement() {
        assert!(!System.holds_debugger_entitlement());
    }

    fn flag() -> &'static AtomicBool {
        Box::leak(Box::new(AtomicBool::new(false)))
    }

    #[test]
    fn bounded_returns_the_answer_and_frees_the_slot() {
        let busy = flag();
        let limit = Duration::from_secs(5);
        assert_eq!(bounded(busy, limit, |_| {}, || Ok(9)).unwrap(), 9);
        let error = bounded(
            busy,
            limit,
            |_| {},
            || Err(Error::new("task_access_denied", "no")),
        )
        .unwrap_err();
        assert_eq!(error.message, "no");
        while busy.load(Ordering::SeqCst) {
            std::thread::yield_now();
        }
    }

    #[test]
    fn a_request_held_open_times_out_is_not_repeated_and_its_late_port_is_released() {
        static RELEASED: AtomicBool = AtomicBool::new(false);
        let busy = flag();
        let (open, held) = std::sync::mpsc::channel::<()>();
        let started = std::time::Instant::now();
        let error = bounded(
            busy,
            Duration::from_millis(50),
            |port| RELEASED.store(port == 41, Ordering::SeqCst),
            move || {
                let _ = held.recv();
                Ok(41)
            },
        )
        .unwrap_err();
        assert!(started.elapsed() < Duration::from_secs(4));
        assert_eq!(error.code, "task_access_denied");
        assert!(error.message.contains("pending"));
        assert!(error.message.contains("did not answer within"));

        // While the first request is still open, a second one is refused at
        // once and its work never runs.
        let again = bounded(
            busy,
            Duration::from_secs(5),
            |_| {},
            || panic!("a second request must not be sent"),
        )
        .unwrap_err();
        assert!(again.message.contains("earlier request is still waiting"));

        // The dialog is finally answered: the late port is given back and the
        // slot frees for the next attach.
        open.send(()).unwrap();
        while busy.load(Ordering::SeqCst) {
            std::thread::yield_now();
        }
        assert!(RELEASED.load(Ordering::SeqCst));
        assert_eq!(
            bounded(busy, Duration::from_secs(5), |_| {}, || Ok(3)).unwrap(),
            3
        );
    }
}
