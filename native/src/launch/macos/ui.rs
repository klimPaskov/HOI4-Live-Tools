//! Shared fresh-read helpers for ordinary macOS UI actions.
use crate::{
    error::{Error, Result},
    memory::Reader,
    platform::Process,
};
use std::{
    thread,
    time::{Duration, Instant},
};

const STAGE_WAIT: Duration = Duration::from_secs(3);
const SETTLE_WAIT: Duration = Duration::from_secs(2);

/// One fresh read bracket; transient UI churn is retried a few times.
pub(super) fn read<T>(
    process: &dyn Process,
    f: impl Fn(&Reader<'_>, u64) -> Result<T>,
) -> Result<T> {
    let mut attempt = 0;
    loop {
        process.begin_snapshot()?;
        let r = Reader::new(process);
        match f(&r, process.image_base()) {
            Err(e)
                if attempt < 3
                    && matches!(
                        e.code,
                        "torn_read" | "short_read" | "invalid_gui" | "invalid_array"
                    ) =>
            {
                attempt += 1;
                thread::sleep(Duration::from_millis(5));
            }
            result => {
                process.check()?;
                return result;
            }
        }
    }
}

/// Bounded wait for a memory predicate after an input.
pub(super) fn wait(
    process: &dyn Process,
    stage: &str,
    f: impl Fn(&Reader<'_>, u64) -> Result<bool>,
) -> Result<()> {
    let deadline = Instant::now() + STAGE_WAIT;
    loop {
        if read(process, &f)? {
            return Ok(());
        }
        if Instant::now() >= deadline {
            return Err(Error::new(
                "input_readback_failed",
                format!("{stage}: expected game change not observed"),
            ));
        }
        thread::sleep(Duration::from_millis(30));
    }
}

/// Press Escape (at most three times) until `closed` holds; best effort, used
/// to leave the UI tidy after the game refuses a target.
pub(super) fn dismiss(
    process: &dyn Process,
    keys: &crate::platform::macos_input::MacKeys,
    closed: impl Fn(&Reader<'_>, u64) -> Result<bool>,
) {
    for _ in 0..3 {
        if read(process, &closed).unwrap_or(true) {
            return;
        }
        if keys
            .press(crate::platform::macos_input::Key::Escape)
            .is_err()
        {
            return;
        }
        thread::sleep(Duration::from_millis(250));
    }
}
/// A click target whose rectangle is identical in two reads 60 ms apart, so
/// sliding panels and animated windows have come to rest before input.
pub(super) fn settled<T: PartialEq>(
    process: &dyn Process,
    stage: &str,
    f: impl Fn(&Reader<'_>, u64) -> Result<T>,
) -> Result<T> {
    let deadline = Instant::now() + SETTLE_WAIT;
    let mut last = read(process, &f)?;
    loop {
        thread::sleep(Duration::from_millis(60));
        let now = read(process, &f)?;
        if now == last {
            return Ok(now);
        }
        if Instant::now() >= deadline {
            return Err(Error::new(
                "gui_target_unsettled",
                format!("{stage}: click target kept moving"),
            ));
        }
        last = now;
    }
}
