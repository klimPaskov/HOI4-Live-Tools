//! Wait-only consumer: pins one worker, never starts/stops or changes its plan.
use super::worker_events::{Disposition, Events, Snapshot};
use super::{
    assignment_wait, game, historical_wait_timeout, json, same_wait_context, Commands, Duration,
    Error, Instant, Result, Value, WaitDeadline,
};
use std::sync::Arc;

struct Watch {
    events: Option<Arc<Events>>,
    instance: Option<u64>,
    sequence: u64,
}
impl Watch {
    fn new(events: Option<Arc<Events>>) -> Result<Self> {
        let initial = events
            .as_ref()
            .map(|e| e.snapshot())
            .transpose()?
            .unwrap_or_default();
        Ok(Self {
            events,
            instance: initial.instance,
            sequence: initial.sequence,
        })
    }
    fn check(&mut self) -> Result<Option<Value>> {
        let Some(events) = &self.events else {
            return Ok(None);
        };
        let snapshot = events.snapshot()?;
        self.accept(snapshot)
    }
    fn accept(&mut self, s: Snapshot) -> Result<Option<Value>> {
        self.sequence = s.sequence;
        if s.retired {
            return Err(Error::new(
                "session_closed",
                "Owned wait retired; no further game read",
            ));
        }
        // A wait begun without a worker never attaches to a later plan.
        let Some(instance) = self.instance else {
            return Ok(None);
        };
        if s.instance != Some(instance) {
            return Ok(Some(
                json!({"reason":"daily_worker_replaced","instance":instance,
                "current_instance":s.instance,"sequence":s.sequence,"input_authorized":false,
                "completion_proven":false,"diagnostics_historical":true}),
            ));
        }
        Ok(s.terminal.map(|terminal| {
            let reason = match terminal.disposition {
                Disposition::Stopped => "daily_worker_stopped",
                Disposition::Quarantined => "daily_worker_quarantined",
                Disposition::RequestedStop => "daily_worker_requested_stop",
            };
            json!({"reason":reason,"instance":instance,"sequence":s.sequence,
                "plan_revision":s.plan_revision,"disposition":terminal.disposition,
                "actionable_error":terminal.disposition != Disposition::RequestedStop,
                "original_error":{"code":terminal.code,"reason":terminal.reason,"entry_id":terminal.entry_id,"phase":terminal.phase},
                "quiescent":true,"input_authorized":false,"completion_proven":false,
                "diagnostics_historical":true})
        }))
    }
    fn pause(&mut self, until: Instant) -> Result<Option<Value>> {
        match &self.events {
            Some(events) => {
                let s = events.wait_after(self.sequence, until)?;
                self.accept(s)
            }
            None => {
                std::thread::sleep(until.saturating_duration_since(Instant::now()));
                Ok(None)
            }
        }
    }
}
enum Read<T> {
    Fresh(T),
    Deadline(WaitDeadline),
    Interrupted(Value),
}
fn read<T>(
    watch: &mut Watch,
    until: Instant,
    rejected: &mut u64,
    mut producer: impl FnMut() -> Result<T>,
) -> Result<Read<T>> {
    loop {
        if Instant::now() >= until {
            return Ok(Read::Deadline(WaitDeadline::BeforeRead));
        }
        let event = watch.check()?;
        if Instant::now() >= until {
            return Ok(Read::Deadline(WaitDeadline::BeforeRead));
        }
        if let Some(event) = event {
            return Ok(Read::Interrupted(event));
        }
        let result = producer();
        // Real producer/context failures retain precedence; only tears recover.
        if let Err(error) = &result {
            if error.code != "torn_read" {
                return Err(result.err().unwrap());
            }
            *rejected = rejected.saturating_add(1);
        }
        if Instant::now() >= until {
            return Ok(Read::Deadline(if result.is_ok() {
                WaitDeadline::LateCompletion
            } else {
                WaitDeadline::TornExhausted
            }));
        }
        let event = watch.check()?;
        if Instant::now() >= until {
            return Ok(Read::Deadline(WaitDeadline::LateCompletion));
        }
        if let Some(event) = event {
            return Ok(Read::Interrupted(event));
        }
        match result {
            Ok(value) => return Ok(Read::Fresh(value)),
            Err(_) => {
                let event = watch.pause(until.min(Instant::now() + Duration::from_millis(250)))?;
                if Instant::now() < until {
                    if let Some(event) = event {
                        return Ok(Read::Interrupted(event));
                    }
                }
            }
        }
    }
}
fn interrupted(
    event: Value,
    state: Option<(&Value, Instant)>,
    start: Instant,
    rejected: u64,
) -> Value {
    json!({"reasons":[event["reason"]],"worker_event":event,
        "state":state.map(|(value,_)|value),"state_historical":true,
        "current_state_verified":false,"input_authorized":false,"completion_proven":false,
        "state_role":if state.is_some(){"last_snapshot_accepted_before_worker_interruption"}else{"none"},
        "state_age_ms":state.map(|(_,received)|received.elapsed().as_millis()),
        "state_age_basis":"time_since_accepted_snapshot",
        "elapsed_seconds":start.elapsed().as_secs_f64(),"rejected_torn_reads":rejected})
}
fn timed_out(
    watch: &mut Watch,
    state: Value,
    start: Instant,
    received: Instant,
    rejected: u64,
    refusal: Option<WaitDeadline>,
) -> Result<Value> {
    let mut value = historical_wait_timeout(state, start, received, rejected, refusal);
    // A late event is diagnostic only: it cannot become a timely interruption
    // or replace the original deadline/refusal and earlier accepted sample.
    if let Some(event) = watch.check()? {
        value["worker_event_historical"] = event;
        value["worker_event_timely"] = json!(false);
    }
    Ok(value)
}
pub(super) fn wait(
    commands: Option<&dyn Commands>,
    timeout: Duration,
    target: Option<i32>,
    assignments: Option<assignment_wait::Target>,
    producer: impl FnMut() -> Result<(Value, game::ClockContext)>,
) -> Result<Value> {
    let start = Instant::now();
    run_from(
        start,
        commands.and_then(Commands::wait_events),
        timeout,
        target,
        assignments,
        producer,
    )
}
fn run_from(
    start: Instant,
    events: Option<Arc<Events>>,
    timeout: Duration,
    target: Option<i32>,
    assignments: Option<assignment_wait::Target>,
    mut producer: impl FnMut() -> Result<(Value, game::ClockContext)>,
) -> Result<Value> {
    let until = start + timeout;
    let mut watch = Watch::new(events)?;
    let mut rejected = 0;
    let (before, baseline) = match read(&mut watch, until, &mut rejected, &mut producer)? {
        Read::Fresh(sample) => sample,
        Read::Interrupted(event) => return Ok(interrupted(event, None, start, rejected)),
        Read::Deadline(refusal) => return Err(refusal.error()),
    };
    let mut current = before.clone();
    let mut received = Instant::now();
    let mut last_date = baseline.date_raw;
    loop {
        if Instant::now() >= until {
            return timed_out(&mut watch, current, start, received, rejected, None);
        }
        let event = watch.check()?;
        if Instant::now() >= until {
            continue;
        }
        if let Some(event) = event {
            return Ok(interrupted(
                event,
                Some((&current, received)),
                start,
                rejected,
            ));
        }
        let mut reasons = Vec::new();
        if let Some(assignments) = &assignments {
            if assignments.reached(&current) {
                reasons.push("assignments_reached");
            }
        } else {
            for (field, reason) in [("focus", "focus_changed"), ("research", "research_changed")] {
                if current[field] != before[field] {
                    reasons.push(reason);
                }
            }
        }
        if target.is_some_and(|date| {
            current["date"]["raw"]
                .as_i64()
                .is_some_and(|n| n >= i64::from(date))
        }) {
            reasons.push("date_reached");
        }
        if !reasons.is_empty() && Instant::now() < until {
            return Ok(
                json!({"reasons":reasons,"state":current,"elapsed_seconds":start.elapsed().as_secs_f64(),"rejected_torn_reads":rejected,"state_age_ms":received.elapsed().as_millis()}),
            );
        }
        let event = watch.pause(until.min(Instant::now() + Duration::from_millis(250)))?;
        if Instant::now() >= until {
            continue;
        }
        if let Some(event) = event {
            return Ok(interrupted(
                event,
                Some((&current, received)),
                start,
                rejected,
            ));
        }
        let outcome = read(&mut watch, until, &mut rejected, || {
            let sample = producer()?;
            same_wait_context(&baseline, &sample.1, last_date)?;
            Ok(sample)
        })?;
        match outcome {
            Read::Fresh((observed, context)) => {
                current = observed;
                last_date = context.date_raw;
                received = Instant::now();
            }
            Read::Interrupted(event) => {
                return Ok(interrupted(
                    event,
                    Some((&current, received)),
                    start,
                    rejected,
                ))
            }
            Read::Deadline(refusal) => {
                return timed_out(
                    &mut watch,
                    current,
                    start,
                    received,
                    rejected,
                    Some(refusal),
                )
            }
        }
    }
}

#[cfg(test)]
mod tests;
