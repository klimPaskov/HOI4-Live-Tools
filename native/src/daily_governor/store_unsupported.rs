use crate::error::{Error, Result};
use std::path::Path;
/// Explicit capability refusal until private atomic storage and ownership are
/// independently implemented for non-Unix platforms. No native eager imports.
pub(super) struct Store;
fn unavailable() -> Error {
    Error::new(
        "capability_unavailable",
        "Private atomic daily plan storage is not verified on this platform",
    )
}
impl Store {
    pub(super) fn open(_: &Path) -> Result<(Self, Option<Vec<u8>>)> {
        Err(unavailable())
    }
    pub(super) fn check(&self) -> Result<()> {
        Err(unavailable())
    }
    pub(super) fn replace(&mut self, _: &[u8]) -> Result<()> {
        Err(unavailable())
    }
}
