//! Persistent owned launch and reconnectable stdio transport. Linux is the first
//! verified process backend; other platforms fail explicitly before launching.
#[cfg(not(target_os = "linux"))]
use crate::error::Error;
use crate::error::Result;
use std::path::Path;
#[cfg(target_os = "linux")]
mod linux;
pub fn serve(config: &Path) -> Result<()> {
    #[cfg(target_os = "linux")]
    {
        linux::serve(config)
    }
    #[cfg(not(target_os = "linux"))]
    {
        let _ = config;
        Err(Error::new(
            "platform_unavailable",
            "Owned native launcher is not verified on this platform",
        ))
    }
}
pub fn connect(directory: &Path) -> Result<()> {
    #[cfg(target_os = "linux")]
    {
        linux::connect(directory)
    }
    #[cfg(not(target_os = "linux"))]
    {
        let _ = directory;
        Err(Error::new(
            "platform_unavailable",
            "Native endpoint transport is not verified on this platform",
        ))
    }
}
pub fn close(directory: &Path) -> Result<()> {
    #[cfg(target_os = "linux")]
    {
        linux::close(directory)
    }
    #[cfg(not(target_os = "linux"))]
    {
        let _ = directory;
        Err(Error::new(
            "platform_unavailable",
            "Owned native shutdown is not verified on this platform",
        ))
    }
}
/// An explicit one-attempt owned WM_DELETE_WINDOW request; no signal fallback.
pub fn close_window(directory: &Path, config: &Path) -> Result<()> {
    #[cfg(target_os = "linux")]
    {
        linux::close_window(directory, config)
    }
    #[cfg(not(target_os = "linux"))]
    {
        let _ = (directory, config);
        Err(Error::new(
            "platform_unavailable",
            "Owned window shutdown is not verified on this platform",
        ))
    }
}
pub fn bind_input(directory: &Path, config: &Path) -> Result<()> {
    #[cfg(target_os = "linux")]
    {
        linux::bind_input(directory, config)
    }
    #[cfg(not(target_os = "linux"))]
    {
        let _ = (directory, config);
        Err(Error::new(
            "platform_unavailable",
            "Ordinary native input adapter is unverified on this platform",
        ))
    }
}
