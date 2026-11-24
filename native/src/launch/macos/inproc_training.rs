//! Training through the in-process module (engineer 82, family `training`): the deployment view's own posts, each
//! checked by the game's own "can do" (and, for the Train button, the game's "can train" the view calls first).
//! - hoi4_train: CreateConveyor(template) as the template row's Train button; the new queue read back from the own
//!   deployment status; SetConveyorSeries(queue, 1) as its + (a new queue trains forever); AddConveyorLine(queue,
//!   count - 1) as Shift-clicks on Add Unit (5 lines each at most); SetConveyorLocation(queue, a province of the deploy
//!   state) as the map click in the queue's location mode.
//! - hoi4_deploy_locations: SetConveyorLocation(every queue without a province, a province of the state).
//!
//! The module also carries the queue's deploy and cancel buttons (training.deploy / training.remove): no MCP tool
//! names them yet. The family is off under `HOI4_BACKEND=auto` until its live session (S15-TRAINING) passes; the mouse
//! path stays the fallback when nothing was posted.
use super::deployment::{self, Queue};
use super::inproc::{Channel, Routed};
use super::inproc_units::{request, routed};
use super::ui;
use crate::{
    error::{Error, Result},
    platform::Process,
};
use serde_json::{json, Value};
use std::time::{Duration, Instant};

/// The lines a train request adds to its new queue (the queue starts with one).
pub(super) fn extra_lines(count: usize) -> i64 {
    count.saturating_sub(1) as i64
}

fn post(route: &mut Option<Channel>, op: &str, objs: &[u64], vals: &[i64], what: &str) -> Result<Option<Value>> {
    super::module_in_process(route, |ch| {
        Ok(match request(op, objs, &[], vals, None).and_then(|r| ch.post_raw(r)) {
            Ok(reply) => routed(reply, what),
            Err(_) => Routed::Gui,
        })
    })
}

/// Re-read the own queues until `done` holds (3 s: a posted command runs on the game's next frame, paused or not).
fn until(process: &dyn Process, what: &str, done: impl Fn(&[Queue]) -> bool) -> Result<Vec<Queue>> {
    let deadline = Instant::now() + Duration::from_secs(3);
    loop {
        let now = ui::read(process, deployment::queues)?;
        if done(&now) {
            return Ok(now);
        }
        if Instant::now() >= deadline {
            return Err(Error::new(
                "input_readback_failed",
                format!("The module posted {what}, but the queue did not show it within 3 s"),
            ));
        }
        crate::phase::sleep(Duration::from_millis(30));
    }
}

/// A queue of `count` divisions of template `name` deploying to `province`. Ok(None): nothing was posted (no channel,
/// or the module could not explain its answer): the mouse path runs. After the first post every failure is an error:
/// the mouse path would add a second queue.
pub(super) fn train(
    process: &dyn Process,
    route: &mut Option<Channel>,
    name: &str,
    count: usize,
    province: i32,
) -> Result<Option<Value>> {
    let Ok(template) = ui::read(process, |r, i| deployment::template_object(r, i, name)) else {
        return Ok(None);
    };
    let before: Vec<u64> = ui::read(process, deployment::queues)?
        .into_iter()
        .map(|q| q.address)
        .collect();
    let Some(created) = post(route, "training.create", &[template], &[], "training this template now")? else {
        return Ok(None);
    };
    let fresh = |qs: &[Queue]| {
        qs.iter()
            .find(|q| !before.contains(&q.address) && q.template == name)
            .cloned()
    };
    let queue = fresh(&until(process, "the new queue", |qs| fresh(qs).is_some())?)
        .ok_or_else(|| Error::new("input_readback_failed", "The new training queue left the list"))?;
    let q = queue.address;
    let step = |route: &mut Option<Channel>, op: &str, vals: &[i64], what: &str| -> Result<Value> {
        post(route, op, &[q], vals, what)?.ok_or_else(|| {
            Error::new(
                "input_readback_failed",
                format!("The training queue was created in process, but {op} was not posted (finish it in the panel)"),
            )
        })
    };
    let mut replies = vec![created];
    if queue.amount == 0 {
        replies.push(step(route, "training.series", &[1], "one division per line")?);
        until(process, "the series amount", |qs| {
            qs.iter().any(|x| x.address == q && x.amount == 1)
        })?;
    }
    let extra = extra_lines(count);
    if extra > 0 {
        replies.push(step(route, "training.add_lines", &[extra], "more lines for this queue")?);
        until(process, "the added lines", |qs| {
            qs.iter().any(|x| x.address == q && x.lines as usize >= count)
        })?;
    }
    replies.push(step(route, "training.location", &[i64::from(province)], "this deployment location")?);
    until(process, "the deployment location", |qs| {
        qs.iter().any(|x| x.address == q && x.location == province)
    })?;
    Ok(Some(json!({"backend":"inprocess","queue":format!("{q:#x}"),"province":province,"posts":replies})))
}

/// Every own queue without a deployment province gets `province`. Ok(None): nothing was posted.
pub(super) fn locate(process: &dyn Process, route: &mut Option<Channel>, province: i32) -> Result<Option<Value>> {
    let open: Vec<u64> = ui::read(process, deployment::queues)?
        .into_iter()
        .filter(|q| q.location <= 0)
        .map(|q| q.address)
        .collect();
    if open.is_empty() {
        return Ok(Some(json!({"backend":"inprocess","posted":0})));
    }
    let objs: Vec<u64> = open.iter().copied().take(32).collect();
    let Some(reply) = post(route, "training.location", &objs, &[i64::from(province)], "this deployment location")? else {
        return Ok(None);
    };
    until(process, "the deployment locations", |qs| {
        qs.iter()
            .filter(|x| objs.contains(&x.address))
            .all(|x| x.location == province)
    })?;
    Ok(Some(reply))
}

/// Own queues without a deployment province, from memory (no panel).
pub(super) fn unlocated(process: &dyn Process) -> Result<usize> {
    Ok(ui::read(process, deployment::queues)?
        .iter()
        .filter(|q| q.location <= 0)
        .count())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_queue_starts_with_one_line_and_the_rest_are_added() {
        assert_eq!(extra_lines(1), 0);
        assert_eq!(extra_lines(6), 5);
        assert_eq!(extra_lines(24), 23);
    }

    #[test]
    fn the_requests_name_the_queue_and_the_values() {
        let r = request("training.create", &[0x7000_1000], &[], &[], None).unwrap();
        assert_eq!(r["cmd"], "training.create");
        let r = request("training.add_lines", &[0x7000_2000], &[], &[23], None).unwrap();
        assert_eq!(r["vals"], "23");
        let r = request("training.location", &[0x7000_2000, 0x7000_3000], &[], &[6521], None).unwrap();
        assert_eq!((r["objs"].as_str(), r["vals"].as_str()), (Some("0x70002000,0x70003000"), Some("6521")));
    }
}
