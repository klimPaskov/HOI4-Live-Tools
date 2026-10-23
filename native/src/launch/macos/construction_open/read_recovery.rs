//! Replace only a wholly discarded read-only opener proof. The original
//! binding, journal and once-only key producer remain outside this loop.
use super::super::action_fence::Fence;
use crate::error::{Error, Result};
use std::{
    thread,
    time::{Duration, Instant},
};

const ATTEMPTS: usize = 4;
const INTERVAL: Duration = Duration::from_millis(2);

pub(super) fn observe<T>(
    fence: &Fence<'_>,
    phase: &'static str,
    producer: impl FnMut() -> Result<T>,
) -> Result<T> {
    recover(
        fence.until,
        phase,
        producer,
        || fence.input_check(),
        Instant::now,
        thread::sleep,
    )
}

fn expired() -> Error {
    Error::new(
        "action_deadline",
        "Original construction opener observation deadline expired",
    )
}

fn boundary(
    until: Instant,
    check: &mut impl FnMut() -> Result<()>,
    now: &mut impl FnMut() -> Instant,
) -> Result<()> {
    if now() >= until {
        return Err(expired());
    }
    // A boundary clock probe has its own bounded recovery. Its refusal must
    // never become another retryable opener observation, even for torn_read.
    check()?;
    if now() >= until {
        return Err(expired());
    }
    Ok(())
}

fn recover<T>(
    until: Instant,
    phase: &'static str,
    mut producer: impl FnMut() -> Result<T>,
    mut check: impl FnMut() -> Result<()>,
    mut now: impl FnMut() -> Instant,
    mut sleep: impl FnMut(Duration),
) -> Result<T> {
    let started = now();
    let mut attempts = 0;
    let run = (|| loop {
        boundary(until, &mut check, &mut now)?;
        attempts += 1;
        let observed = producer();
        let after = boundary(until, &mut check, &mut now);
        // Always perform the post-read boundary. Preserve a coherent refusal
        // even if that boundary also fails; late success/tears are discarded.
        let cause = match observed {
            Err(error) if error.code != "torn_read" => return Err(error),
            observed => {
                after?;
                match observed {
                    Ok(value) => return Ok(value),
                    Err(error) => error,
                }
            }
        };
        if attempts == ATTEMPTS {
            return Err(cause);
        }
        boundary(until, &mut check, &mut now)?;
        sleep(INTERVAL.min(until.saturating_duration_since(now())));
        boundary(until, &mut check, &mut now)?;
    })();
    run.map_err(|error| {
        Error::new(
            error.code,
            format!(
                "phase={phase}; read_attempts={attempts}; read_elapsed_ms={:.3}; {}",
                now().saturating_duration_since(started).as_secs_f64() * 1000.0,
                error.message,
            ),
        )
    })
}

#[cfg(test)]
mod tests;
