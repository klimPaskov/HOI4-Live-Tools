//! Replace only a wholly discarded read-only production proof. The transport,
//! cursor movement, journal writes and candidate adoption stay outside this loop.
use super::*;

const ATTEMPTS: usize = 4;
const INTERVAL: Duration = Duration::from_millis(2);

pub(super) fn stage<T>(name: &'static str, result: Result<T>) -> Result<T> {
    result.map_err(|error| Error::new(error.code, format!("stage={name}; {}", error.message)))
}

pub(super) fn observe<T>(
    process: &dyn Process,
    until: Instant,
    phase: &'static str,
    producer: impl FnMut() -> Result<T>,
) -> Result<T> {
    recover(
        until,
        phase,
        producer,
        || process.check(),
        Instant::now,
        thread::sleep,
    )
}

fn expired() -> Error {
    Error::new(
        "production_deadline",
        "Original Mac production observation deadline expired",
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
    // The action-local Process is the original Fence, retaining its lease,
    // cancellation and exact process checks. Never replace it with a raw reader.
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
    let run = (|| {
        loop {
            boundary(until, &mut check, &mut now)?;
            attempts += 1;
            let result = producer();
            let after = boundary(until, &mut check, &mut now);
            // Preserve a coherent refusal, even when its read also consumed
            // the remaining time. Successful/torn late samples are discarded.
            let cause = match result {
                Err(error) if error.code != "torn_read" => return Err(error),
                result => {
                    after?;
                    match result {
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
        }
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
