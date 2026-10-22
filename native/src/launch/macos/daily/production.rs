//! One original own-line factory step under a durable governor attempt. Panel
//! preparation is covered by the progress intent; the typed adapter clicks once.
use super::super::{production as ui, ui as common};
use crate::{
    construction_stage::{DispatchOutcome, FailureEvidence},
    daily_governor::{
        Attempt, EconomyTarget, Entry, Intent, Observation, Outcome, Readiness, Scope,
        VerifiedReceipt,
    },
    daily_observation::Producer,
    error::{Error, Result},
    game::ClockContext,
    gui_tree::{self, Rect},
    input_lease::InputAuthority,
    memory::Reader,
    platform::{
        macos_input::{Key, MacKeys},
        Identity, Process,
    },
    production_lines::control_snapshot,
    production_progress::Progress,
    production_step::{self as step, FactoryState, FactoryTarget, LineIdentity, State, Target},
    research_ui::Witnesses,
};
use std::{
    path::Path,
    sync::RwLock,
    thread,
    time::{Duration, Instant},
};

const GAME: (u32, u32) = (1920, 1080);
pub(super) struct Journals {
    scope: Scope,
    progress: Progress,
    ordinary: step::Journal,
}
fn deadline(until: Instant) -> Result<()> {
    if Instant::now() >= until {
        return Err(Error::new(
            "production_deadline",
            "Original Mac production deadline expired",
        ));
    }
    Ok(())
}
fn goal(entry: &Entry, scope: &Scope) -> Result<FactoryTarget> {
    entry.validate()?;
    let Intent::Economy {
        target:
            EconomyTarget::Production {
                line_id,
                equipment_id,
                total_factories,
            },
    } = &entry.intent
    else {
        return Err(Error::new(
            "invalid_plan",
            "Existing-line production allocation required",
        ));
    };
    let target = FactoryTarget {
        epoch: scope.epoch.clone(),
        line_handle: line_id.clone(),
        equipment_id: equipment_id.clone(),
        factories: i32::from(*total_factories),
    };
    if target.validate()? as u32 != 0x38 {
        return Err(Error::new(
            "capability_unavailable",
            "Mac daily production currently requires an own military line",
        ));
    }
    Ok(target)
}
struct Canonical {
    state: FactoryState,
    context: ClockContext,
    allocations: Vec<(LineIdentity, i32)>,
}
fn canonical(
    process: &dyn Process,
    scope: &Scope,
    target: &FactoryTarget,
    until: Instant,
) -> Result<Canonical> {
    deadline(until)?;
    if process.identity() != Some(&scope.epoch.game) || target.epoch != scope.epoch {
        return Err(Error::new(
            "ownership_changed",
            "Mac production process/controller differs",
        ));
    }
    let observed =
        control_snapshot::observe(process, &target.line_handle, &target.equipment_id, None)?;
    let context = observed.campaign;
    if observed.line.country != scope.own_country
        || context.anchors != scope.anchors
        || context.player != scope.epoch.player
        || context.difficulty != 2
        || context.historical_raw != 1
        || scope
            .epoch
            .campaign
            .split_once(':')
            .map(|(_, playthrough)| playthrough)
            != Some(context.playthrough_id.as_str())
    {
        return Err(Error::new(
            "ownership_changed",
            "Mac production own scope or campaign differs",
        ));
    }
    let allocations = observed.allocations.ok_or_else(|| {
        Error::new(
            "production_target_unavailable",
            "Complete typed own allocation scope required",
        )
    })?;
    let state = FactoryState {
        epoch: scope.epoch.clone(),
        line: observed.line,
        requested: observed.requested,
    };
    state.validate(target)?;
    deadline(until)?;
    Ok(Canonical {
        state,
        context,
        allocations,
    })
}
pub(super) fn readiness(
    process: &dyn Process,
    entry: &Entry,
    observed: &Observation,
) -> Result<Readiness> {
    let target = goal(entry, &observed.scope)?;
    let current = canonical(
        process,
        &observed.scope,
        &target,
        Instant::now() + Duration::from_secs(1),
    )?;
    if current.context.date_raw < observed.date_raw {
        return Err(Error::new(
            "torn_read",
            "Production readiness chronology regressed",
        ));
    }
    Ok(
        if (i64::from(target.factories) - i64::from(current.state.requested)).abs() <= 1 {
            Readiness::Ready
        } else {
            Readiness::Unavailable("Mac daily production requires exactly one factory step from the original allocation".into())
        },
    )
}
impl Journals {
    pub(super) fn open(directory: &Path, scope: &Scope, entries: &[Entry]) -> Result<Self> {
        scope.validate()?;
        Ok(Self {
            scope: scope.clone(),
            progress: Progress::open(
                &directory.join("daily-production.json"),
                scope.clone(),
                entries,
            )?,
            ordinary: step::Journal::open(&directory.join("production-actions.jsonl"))?,
        })
    }
    pub(super) fn unresolved(&mut self, identity: &Identity) -> Result<bool> {
        if identity != &self.scope.epoch.game {
            return Err(Error::new(
                "ownership_changed",
                "Production retained owner differs",
            ));
        }
        self.progress.check_storage()?;
        self.ordinary.check_owner(&self.scope.epoch)?;
        Ok(self.progress.blocked() || self.ordinary.unresolved())
    }
    pub(super) fn check(&mut self, identity: &Identity) -> Result<()> {
        if self.unresolved(identity)? {
            return Err(Error::new(
                "action_uncertain",
                "Retained Mac production input is unresolved",
            ));
        }
        Ok(())
    }
    #[allow(clippy::too_many_arguments)]
    pub(super) fn attempt(
        &mut self,
        process: &(dyn Process + Send + Sync),
        keys: &MacKeys,
        authority: &InputAuthority,
        token: &str,
        attempt: &Attempt,
        producer: &Producer,
        until: Instant,
    ) -> Result<Outcome> {
        self.check(&attempt.scope.epoch.game)?;
        if attempt.scope != self.scope {
            return Err(Error::new(
                "ownership_changed",
                "Production attempt scope differs",
            ));
        }
        if self.progress.used_steps(attempt.day) >= 1 {
            return Ok(Outcome::NotQueued(
                "Mac production's one-step daily budget is already reserved".into(),
            ));
        }
        let target = goal(&attempt.entry, &attempt.scope)?;
        let before = canonical(process, &self.scope, &target, until)?;
        if (i64::from(target.factories) - i64::from(before.state.requested)).abs() > 1 {
            return Ok(Outcome::NotQueued(
                "Fresh allocation is not one original factory step from the target".into(),
            ));
        }
        self.progress
            .bind_original(&attempt.entry, &before.state, &before.context)?;
        let request = self
            .progress
            .begin_completion(attempt, &before.state, &before.context, 1)?;
        // From here preparation can send input. Any unproved failure leaves the
        // progress intent blocking; a new ID, lease or worker cannot rescue it.
        let run = (|| -> Result<VerifiedReceipt> {
            deadline(until)?;
            if before.state.requested != target.factories {
                common::clear_notices(process, keys)?;
                ui::ensure_open(process, keys)?;
                if common::read(process, ui::selection_open)? {
                    keys.press(Key::Escape)?;
                    common::wait(process, "close production selection", |r, image| {
                        Ok(!ui::selection_open(r, image)?)
                    })?;
                    ui::ensure_open(process, keys)?;
                }
                ui::reveal(process, keys, before.state.line.handle)?;
            }
            let prepared = canonical(process, &self.scope, &target, until)?;
            same_before(&before, &prepared)?;
            let mut backend = MacBackend {
                process,
                keys,
                scope: &self.scope,
                original: &before,
                until,
            };
            let timeout = until
                .saturating_duration_since(Instant::now())
                .min(Duration::from_secs(10));
            deadline(until)?;
            let execution = self.ordinary.execute(
                &request.id,
                request.target.clone(),
                token,
                authority,
                &RwLock::new(true),
                &Cleared(&self.scope.epoch.game),
                &mut backend,
                timeout,
            )?;
            let fresh = canonical(process, &self.scope, &target, until)?;
            same_after(&before, &fresh, &target)?;
            self.progress
                .verified(&request.id, &execution, &fresh.state, &fresh.context)?;
            let observed = producer.observe(process, until)?;
            let (original, original_context) = self.progress.original(&attempt.entry)?;
            let receipt = VerifiedReceipt::from_production(
                attempt.clone(),
                &execution,
                original,
                &fresh.state,
                original_context,
                &fresh.context,
                observed.observation,
            )?;
            deadline(until)?;
            Ok(receipt)
        })();
        match run {
            Ok(receipt) => Ok(Outcome::Verified(Box::new(receipt))),
            Err(cause) => {
                let saved =
                    self.progress
                        .failed(&request.id, &cause, FailureEvidence::Unknown, token);
                Ok(Outcome::Uncertain(format!(
                    "Mac production transaction unverified: {}{}",
                    cause.code,
                    if saved.is_err() {
                        "; progress remains unresolved"
                    } else {
                        ""
                    }
                )))
            }
        }
    }
}
fn same_before(original: &Canonical, current: &Canonical) -> Result<()> {
    if original.state != current.state
        || original.allocations != current.allocations
        || current.context.date_raw < original.context.date_raw
    {
        return Err(Error::new(
            "production_state_changed",
            "Original own line/allocation scope changed before the factory click",
        ));
    }
    Ok(())
}
fn same_after(original: &Canonical, current: &Canonical, target: &FactoryTarget) -> Result<()> {
    let mut expected = original.allocations.clone();
    let Some((_, requested)) = expected
        .iter_mut()
        .find(|(line, _)| *line == original.state.line)
    else {
        return Err(Error::new(
            "production_state_changed",
            "Original line absent from allocation scope",
        ));
    };
    *requested = target.factories;
    if current.state.epoch != original.state.epoch
        || current.state.line != original.state.line
        || current.state.requested != target.factories
        || current.allocations != expected
        || current.context.date_raw < original.context.date_raw
    {
        return Err(Error::new(
            "production_state_changed",
            "Factory step changed its original line or other own allocations",
        ));
    }
    Ok(())
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct Button {
    frame: u64,
    widget: u64,
    rect: Rect,
}
/// Opening a panel can expose its rows while they are still animating. Settle
/// the complete control witness before handing it to the single-click journal;
/// never replace that witness after pointer movement begins.
fn settled_button(until: Instant, mut observe: impl FnMut() -> Result<Button>) -> Result<Button> {
    settle_button_with(
        until.min(Instant::now() + Duration::from_secs(2)),
        &mut observe,
        Instant::now,
        thread::sleep,
    )
}
fn settle_button_with(
    until: Instant,
    mut observe: impl FnMut() -> Result<Button>,
    mut now: impl FnMut() -> Instant,
    mut sleep: impl FnMut(Duration),
) -> Result<Button> {
    let mut previous = None;
    loop {
        if now() >= until {
            return Err(Error::new(
                "production_deadline",
                "Factory control did not settle within the original action deadline",
            ));
        }
        let sample = observe();
        let after = now();
        if after >= until {
            return Err(Error::new(
                "production_deadline",
                "Late factory-control sample discarded",
            ));
        }
        match sample {
            Ok(current) => {
                if previous.as_ref() == Some(&current) {
                    return Ok(current);
                }
                previous = Some(current);
            }
            Err(error) if error.code == "torn_read" => previous = None,
            Err(error) => return Err(error),
        }
        sleep(Duration::from_millis(60).min(until.saturating_duration_since(after)));
    }
}
fn unchanged_button(expected: &Button, current: &Button) -> Result<()> {
    if expected != current {
        return Err(Error::new(
            "production_state_changed",
            format!(
                "Factory control changed before mouse-down: row_changed={}, widget_changed={}, expected_rect={:?}, current_rect={:?}",
                expected.frame != current.frame,
                expected.widget != current.widget,
                expected.rect,
                current.rect,
            ),
        ));
    }
    Ok(())
}
fn button(process: &dyn Process, handle: u64, increasing: bool, until: Instant) -> Result<Button> {
    deadline(until)?;
    process.begin_snapshot()?;
    let witnesses = Witnesses::new(process);
    let r = Reader::new(&witnesses);
    let image = process.image_base();
    if !ui::open(&r, image)? || ui::selection_open(&r, image)? {
        return Err(Error::new(
            "production_precondition",
            "Own production panel must be visible with no equipment selection",
        ));
    }
    let frame = ui::row(&r, image, handle)?.ok_or_else(|| {
        Error::new(
            "production_target_unavailable",
            "Original own line row is not materialized",
        )
    })?;
    let name = if increasing {
        "factories_add_1x"
    } else {
        "factories_sub_1x"
    };
    let matches = gui_tree::children(&r, frame)?
        .into_iter()
        .filter_map(|widget| match gui_tree::name(&r, widget, image) {
            Ok(Some(found)) if found == name => Some(Ok(widget)),
            Ok(_) => None,
            Err(error) => Some(Err(error)),
        })
        .collect::<Result<Vec<_>>>()?;
    if matches.len() != 1 {
        return Err(Error::new(
            "production_target_unavailable",
            "Exactly one named original-line factory control required",
        ));
    }
    let widget = matches[0];
    if !gui_tree::visible(&r, frame)?
        || !gui_tree::shown(&r, widget)?
        || gui_tree::disabled(&r, widget)?
    {
        return Err(Error::new(
            "production_precondition",
            "Original-line factory control is hidden or disabled",
        ));
    }
    let rect = gui_tree::rect_unclipped(&r, image, widget, GAME)?;
    if rect.x < 0 || rect.y < 360 || rect.x + rect.w > 1920 || rect.y + rect.h > 1040 {
        return Err(Error::new(
            "gui_target_unavailable",
            "Factory control lies outside the reviewed own line list",
        ));
    }
    witnesses.finish()?;
    process.check()?;
    deadline(until)?;
    Ok(Button {
        frame,
        widget,
        rect,
    })
}
struct Cleared<'a>(&'a Identity);
impl step::UnresolvedInputs for Cleared<'_> {
    fn clear(&self, identity: &Identity) -> Result<()> {
        // The coordinator holds Control and has checked all foreign journals.
        if identity != self.0 {
            return Err(Error::new(
                "ownership_changed",
                "Cleared production owner differs",
            ));
        }
        Ok(())
    }
}
struct MacBackend<'a, 'p> {
    process: &'a (dyn Process + Send + Sync),
    keys: &'a MacKeys<'p>,
    scope: &'a Scope,
    original: &'a Canonical,
    until: Instant,
}
impl step::Backend for MacBackend<'_, '_> {
    type Candidate = Button;
    fn observe(&mut self, target: &Target, until: Instant) -> Result<State> {
        let observed = canonical(
            self.process,
            self.scope,
            target.factory()?,
            until.min(self.until),
        )?;
        if observed.state.line != self.original.state.line {
            return Err(Error::new(
                "ownership_changed",
                "Original production line changed",
            ));
        }
        Ok(State::Factories(observed.state))
    }
    fn candidate(&mut self, target: &Target, before: &State, until: Instant) -> Result<Button> {
        let until = until.min(self.until);
        let target = target.factory()?;
        settled_button(until, || {
            let current = canonical(self.process, self.scope, target, until)?;
            same_before(self.original, &current)?;
            if &current.state != before.factory()? {
                return Err(Error::new(
                    "production_state_changed",
                    "Factory candidate differs from typed before state",
                ));
            }
            let candidate = button(
                self.process,
                current.state.line.handle,
                target.factories > current.state.requested,
                until,
            )?;
            same_before(
                self.original,
                &canonical(self.process, self.scope, target, until)?,
            )?;
            Ok(candidate)
        })
    }
    fn dispatch_once(
        &mut self,
        target: &Target,
        before: &State,
        candidate: &Button,
        until: Instant,
    ) -> DispatchOutcome {
        let run = (|| {
            let until = until.min(self.until);
            let target = target.factory()?;
            let check = || {
                deadline(until)?;
                self.keys.confirm_input_idle()?;
                let current = canonical(self.process, self.scope, target, until)?;
                same_before(self.original, &current)?;
                if &current.state != before.factory()? {
                    return Err(Error::new(
                        "production_state_changed",
                        "Factory allocation differs from typed before state before mouse-down",
                    ));
                }
                unchanged_button(
                    candidate,
                    &button(
                        self.process,
                        current.state.line.handle,
                        target.factories > current.state.requested,
                        until,
                    )?,
                )?;
                deadline(until)
            };
            let guarded = self.keys.guarded(self.process, &check)?;
            let (x, y) = candidate.rect.center();
            guarded.click(GAME, x, y)?;
            Ok(())
        })();
        // CGEventPost has no server delivery receipt. Only canonical readback
        // can prove this step; transport errors retain possible input.
        match run {
            Ok(()) => DispatchOutcome::Unknown(None),
            Err(error) => DispatchOutcome::Unknown(Some(error)),
        }
    }
    fn confirm_input_idle(&mut self, until: Instant) -> Result<()> {
        deadline(until.min(self.until))?;
        self.keys.confirm_input_idle()?;
        deadline(until.min(self.until))
    }
}

#[cfg(test)]
mod proof_tests {
    use super::*;
    use crate::actions::Epoch;
    use std::cell::Cell;

    fn control() -> Button {
        Button {
            frame: 0x1000,
            widget: 0x2000,
            rect: Rect {
                x: 300,
                y: 400,
                w: 20,
                h: 20,
            },
        }
    }
    #[test]
    fn moving_or_rebuilt_factory_control_must_settle_before_dispatch() {
        let start = Instant::now();
        let clock = Cell::new(start);
        let initial = control();
        let mut moved = initial.clone();
        moved.rect.x += 15;
        let mut rebuilt = moved.clone();
        rebuilt.widget += 8;
        let mut samples = [initial, moved, rebuilt.clone(), rebuilt.clone()].into_iter();
        let found = settle_button_with(
            start + Duration::from_secs(1),
            || Ok(samples.next().expect("bounded samples")),
            || clock.get(),
            |duration| clock.set(clock.get() + duration),
        )
        .unwrap();
        assert_eq!(found, rebuilt);
        assert_eq!(clock.get() - start, Duration::from_millis(180));

        // The final guard still refuses a later change; settlement grants no
        // permission to replace the candidate after moving the pointer.
        let mut changed = found.clone();
        changed.frame += 8;
        changed.rect.y += 1;
        let error = unchanged_button(&found, &changed).unwrap_err();
        assert_eq!(error.code, "production_state_changed");
        assert!(error
            .message
            .contains("row_changed=true, widget_changed=false"));
        assert!(error.message.contains("expected_rect="));
        assert!(error.message.contains("current_rect="));
    }
    #[test]
    fn torn_control_sample_resets_stability_but_other_failures_refuse() {
        let start = Instant::now();
        let clock = Cell::new(start);
        let mut samples = [
            Ok(control()),
            Err(Error::new("torn_read", "generated UI churn")),
            Ok(control()),
            Ok(control()),
        ]
        .into_iter();
        settle_button_with(
            start + Duration::from_secs(1),
            || samples.next().expect("bounded samples"),
            || clock.get(),
            |duration| clock.set(clock.get() + duration),
        )
        .unwrap();
        assert_eq!(clock.get() - start, Duration::from_millis(180));
        let error = settle_button_with(
            start + Duration::from_secs(1),
            || Err(Error::new("ownership_changed", "generated replaced owner")),
            || start,
            |_| panic!("owner changes must not be retried"),
        )
        .unwrap_err();
        assert_eq!(error.code, "ownership_changed");
    }
    #[test]
    fn settlement_never_extends_deadline_or_accepts_a_late_matching_sample() {
        let start = Instant::now();
        let until = start + Duration::from_millis(100);
        let clock = Cell::new(start);
        let calls = Cell::new(0);
        let error = settle_button_with(
            until,
            || {
                calls.set(calls.get() + 1);
                if calls.get() == 2 {
                    clock.set(until);
                }
                Ok(control())
            },
            || clock.get(),
            |duration| clock.set(clock.get() + duration),
        )
        .unwrap_err();
        assert_eq!(error.code, "production_deadline");
        assert_eq!(calls.get(), 2);
        assert!(settle_button_with(
            until,
            || panic!("expired settlement must not read"),
            || until,
            |_| panic!("expired settlement must not sleep"),
        )
        .is_err());

        clock.set(start);
        calls.set(0);
        let error = settle_button_with(
            until,
            || {
                let mut value = control();
                calls.set(calls.get() + 1);
                value.rect.x += calls.get();
                Ok(value)
            },
            || clock.get(),
            |duration| clock.set(clock.get() + duration),
        )
        .unwrap_err();
        assert_eq!(error.code, "production_deadline");
        assert_eq!(clock.get(), until);
        assert_eq!(calls.get(), 2);
    }
    fn snapshot() -> Canonical {
        let epoch = Epoch {
            game: Identity {
                pid: 71,
                start_ticks: "123.000456".into(),
                exe: "/generated/hoi4".into(),
                uid: 501,
            },
            campaign: "original:fixture".into(),
            player: "GER".into(),
        };
        let line = LineIdentity {
            country: 0x10000,
            production: 0x20000,
            line: 0x30000,
            variant: 0x40000,
            base_type: 0x50000,
            handle: 0x0000000100000038,
            equipment_id: "infantry_equipment_1".into(),
        };
        let mut other = line.clone();
        other.line += 0x1000;
        other.variant += 0x1000;
        other.handle += 1 << 32;
        Canonical {
            state: FactoryState {
                epoch,
                line: line.clone(),
                requested: 10,
            },
            context: ClockContext {
                playthrough_id: "fixture".into(),
                player: "GER".into(),
                anchors: [0x60000, 0x70000, 0x80000],
                date_raw: crate::calendar::encode(1936, 1, 2).unwrap(),
                difficulty: 2,
                historical_raw: 1,
                paused: false,
                speed: 1,
            },
            allocations: vec![(line, 10), (other, 5)],
        }
    }
    fn target(original: &Canonical) -> FactoryTarget {
        FactoryTarget {
            epoch: original.state.epoch.clone(),
            line_handle: format!("0x{:016x}", original.state.line.handle),
            equipment_id: original.state.line.equipment_id.clone(),
            factories: 11,
        }
    }
    fn reached() -> Canonical {
        let mut current = snapshot();
        current.state.requested = 11;
        current.allocations[0].1 = 11;
        current
    }
    #[test]
    fn exact_one_factory_preserves_original_line_and_every_other_allocation() {
        let original = snapshot();
        same_before(&original, &snapshot()).unwrap();
        same_after(&original, &reached(), &target(&original)).unwrap();
        let mut progressed = reached();
        progressed.context.date_raw += 1;
        same_after(&original, &progressed, &target(&original)).unwrap();
    }
    #[test]
    fn replacement_reorder_other_line_and_extra_factory_cannot_complete() {
        let original = snapshot();
        for alter in [
            |c: &mut Canonical| c.state.line.line += 8,
            |c: &mut Canonical| c.state.line.variant += 8,
            |c: &mut Canonical| c.state.line.base_type += 8,
            |c: &mut Canonical| c.allocations.reverse(),
            |c: &mut Canonical| c.allocations[1].1 += 1,
            |c: &mut Canonical| c.allocations[1].0.line += 8,
            |c: &mut Canonical| c.state.requested += 1,
            |c: &mut Canonical| c.context.date_raw -= 1,
            |c: &mut Canonical| c.state.epoch.game.start_ticks = "124.000456".into(),
        ] {
            let mut current = reached();
            alter(&mut current);
            assert_eq!(
                same_after(&original, &current, &target(&original))
                    .unwrap_err()
                    .code,
                "production_state_changed"
            );
        }
    }
    #[test]
    fn preparation_cannot_adopt_a_changed_count_or_order_as_its_baseline() {
        let original = snapshot();
        for alter in [
            |c: &mut Canonical| c.state.requested += 1,
            |c: &mut Canonical| c.allocations[1].1 += 1,
            |c: &mut Canonical| c.allocations.reverse(),
        ] {
            let mut current = snapshot();
            alter(&mut current);
            assert!(same_before(&original, &current).is_err());
        }
    }
}
