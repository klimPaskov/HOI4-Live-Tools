//! Query the existing task-port authorization without displaying OS UI.
//! This does not enable Developer Tools, alter policy or collect credentials.
use crate::error::{Error, Result};
use std::ffi::{c_char, c_void};

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
const TASK_PORT: &[u8] = b"system.privilege.taskport\0";

#[link(name = "Security", kind = "framework")]
extern "C" {
    fn AuthorizationCreate(
        rights: *const Rights,
        environment: *const Rights,
        flags: u32,
        authorization: *mut *mut c_void,
    ) -> i32;
}

pub(super) fn require_noninteractive_task_access() -> Result<()> {
    query(|rights, flags| {
        // SAFETY: rights and its single item remain live throughout this
        // synchronous call. Null environment/output are documented options.
        unsafe { AuthorizationCreate(rights, std::ptr::null(), flags, std::ptr::null_mut()) }
    })
}

fn query(authorize: impl FnOnce(&Rights, u32) -> i32) -> Result<()> {
    let mut item = Item {
        name: TASK_PORT.as_ptr().cast(),
        value_length: 0,
        value: std::ptr::null_mut(),
        flags: 0,
    };
    let rights = Rights {
        count: 1,
        items: &mut item,
    };
    let status = authorize(&rights, QUERY_FLAGS);
    if status != 0 {
        return Err(Error::new(
            "task_access_denied",
            format!("macOS task-port authorization is unavailable without user interaction (status {status}); complete Developer Tools authorization locally, then reconnect. No permission prompt or game input was sent"),
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::ffi::CStr;

    #[test]
    fn queries_only_taskport_without_ui_credentials_or_preauthorization() {
        let result = query(|rights, flags| {
            assert_eq!(rights.count, 1);
            assert_eq!(flags, 10);
            assert_eq!(flags & ((1 << 0) | (1 << 4) | (1 << 9)), 0);
            // SAFETY: query supplies exactly one live stack item.
            let item = unsafe { &*rights.items };
            assert_eq!(
                unsafe { CStr::from_ptr(item.name) }.to_bytes_with_nul(),
                TASK_PORT
            );
            assert_eq!(item.value_length, 0);
            assert!(item.value.is_null());
            assert_eq!(item.flags, 0);
            0
        });
        assert!(result.is_ok());
    }

    #[test]
    fn denied_interaction_cancelled_and_unknown_statuses_are_capability_errors() {
        for status in [-60005, -60006, -60007, -60008, -1, 1] {
            let error = query(|_, _| status).unwrap_err();
            assert_eq!(error.code, "task_access_denied");
            assert!(error.message.contains(&status.to_string()));
        }
    }
}
