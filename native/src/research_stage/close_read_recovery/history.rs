//! Constant-size diagnostics for discarded whole Research snapshots. No input,
//! lease, guard, persistence or destination pin belongs inside these closures.
use crate::error::{Error, Result};
use serde::Serialize;
use serde_json::{json, Value};
use std::time::{Duration, Instant};

#[derive(Clone, Copy, Debug)]
pub(crate) enum Phase {
    DestinationInitial,
    DestinationAfterDeparture,
    SelectedIdentity,
}
impl Phase {
    fn name(self) -> &'static str {
        match self {
            Self::DestinationInitial => "mixed_destination_initial",
            Self::DestinationAfterDeparture => "mixed_destination_after_departure",
            Self::SelectedIdentity => "selected_research_identity",
        }
    }
    fn producer(self) -> &'static str {
        match self {
            Self::DestinationInitial | Self::DestinationAfterDeparture => "mixed_destination",
            Self::SelectedIdentity => "research_identity",
        }
    }
}

#[derive(Clone, Serialize)]
struct Rejected {
    producer: &'static str,
    phase: &'static str,
    code: &'static str,
    reason: String,
}
impl Rejected {
    fn new(phase: Phase, error: &Error) -> Self {
        Self {
            producer: phase.producer(),
            phase: phase.name(),
            code: error.code,
            // The shared recovery primitive already bounds and sanitizes this
            // cause. Never copy a payload or pointer-bearing observation here.
            reason: error.message.clone(),
        }
    }
}

#[derive(Default)]
pub(crate) struct History {
    rejected_reads: u64,
    recovered_observations: u64,
    /// Only observations with rejected tears that never recovered, not pure
    /// permanent/precondition refusals that had no discarded snapshot.
    failed_observations: u64,
    first_failure: Option<Rejected>,
    latest_failure: Option<Rejected>,
    latest_rejection_recovered: bool,
    poisoned: bool,
}
impl History {
    pub(crate) fn observe<T>(
        &mut self,
        phase: Phase,
        until: Instant,
        read: impl FnMut() -> Result<T>,
    ) -> Result<T> {
        self.observe_with(phase, until, read, Instant::now, std::thread::sleep)
    }
    fn observe_with<T>(
        &mut self,
        phase: Phase,
        until: Instant,
        read: impl FnMut() -> Result<T>,
        now: impl FnMut() -> Instant,
        sleep: impl FnMut(Duration),
    ) -> Result<T> {
        if self.poisoned {
            return Err(full());
        }
        // This request-local vector contains at most four errors and is folded
        // immediately. The worker lifetime retains only first/latest causes.
        let mut rejected = Vec::new();
        let result = super::recover(until, read, now, sleep, &mut rejected)
            .map_err(|cause| super::at_phase(cause, phase.name()));
        let retained = self.retain(phase, &rejected, result.is_ok());
        // Preserve the actual producer/deadline failure even if diagnostics
        // capacity also failed. Poison prevents any subsequent observation.
        match result {
            Err(cause) => Err(cause),
            Ok(value) => retained.map(|()| value),
        }
    }
    fn retain(&mut self, phase: Phase, rejected: &[Error], recovered: bool) -> Result<()> {
        let Some(first) = rejected.first() else {
            return Ok(());
        };
        let counts = self
            .rejected_reads
            .checked_add(rejected.len() as u64)
            .zip(
                self.recovered_observations
                    .checked_add(u64::from(recovered)),
            )
            .zip(self.failed_observations.checked_add(u64::from(!recovered)));
        let Some(((total, success), failure)) = counts else {
            self.poisoned = true;
            return Err(full());
        };
        self.rejected_reads = total;
        self.recovered_observations = success;
        self.failed_observations = failure;
        self.first_failure
            .get_or_insert_with(|| Rejected::new(phase, first));
        self.latest_failure = rejected.last().map(|cause| Rejected::new(phase, cause));
        self.latest_rejection_recovered = recovered;
        Ok(())
    }
    /// Clean reads leave earlier recovery outcomes intact. No optional field is
    /// added to legacy/zero-rejection reports, receipts or stored plan hashes.
    pub(crate) fn summary(&self) -> Option<Value> {
        if self.rejected_reads == 0 && !self.poisoned {
            return None;
        }
        Some(json!({"rejected_reads":self.rejected_reads,
            "recovered_observations":self.recovered_observations,
            "failed_observations":self.failed_observations,
            "first_failure":self.first_failure,"latest_failure":self.latest_failure,
            "latest_rejection_recovered":self.latest_rejection_recovered,
            "unavailable":self.poisoned,"scope":"worker_lifetime",
            "historical":true,"current_state_verified":false,"input_authorized":false}))
    }
}
fn full() -> Error {
    Error::new(
        "research_history_full",
        "Research read history counter exhausted; no new observation",
    )
}

#[cfg(test)]
mod tests;
