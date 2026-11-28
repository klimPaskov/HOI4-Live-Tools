//! Training through the in-process module (engineer 82, family `training`): the deployment view's own posts, each
//! checked by the game's own "can do" (and, for the Train button, the game's "can train" the view calls first).
//! - hoi4_train: CreateConveyor(template) as the template row's Train button; the new queue read back from the own
//!   deployment status; SetConveyorSeries(queue, 1) as its + (a new queue trains forever); AddConveyorLine(queue, n)
//!   as Add Unit clicks, one post at a time with the queue's lines read after each and only the remainder asked
//!   (at most 5 a post; a surplus the game added is taken off the next ask); SetConveyorLocation(queue, a province of
//!   the deploy state) as the map click in the queue's location mode. A failure after the queue was made cancels it
//!   (training.remove), so no empty queue is left behind.
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

fn post(
    route: &mut Option<Channel>,
    op: &str,
    objs: &[u64],
    vals: &[i64],
    what: &str,
) -> Result<Option<Value>> {
    super::module_in_process(route, |ch| {
        Ok(
            match request(op, objs, &[], vals, None).and_then(|r| ch.post_raw(r)) {
                Ok(reply) => routed(reply, what),
                Err(_) => Routed::Gui,
            },
        )
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
    let Some(created) = post(
        route,
        "training.create",
        &[template],
        &[],
        "training this template now",
    )?
    else {
        return Ok(None);
    };
    let fresh = |qs: &[Queue]| {
        qs.iter()
            .find(|q| !before.contains(&q.address) && q.template == name)
            .cloned()
    };
    let queue =
        fresh(&until(process, "the new queue", |qs| fresh(qs).is_some())?).ok_or_else(|| {
            Error::new(
                "input_readback_failed",
                "The new training queue left the list",
            )
        })?;
    let q = queue.address;
    let step = |route: &mut Option<Channel>, op: &str, vals: &[i64], what: &str| -> Result<Value> {
        post(route, op, &[q], vals, what)?.ok_or_else(|| {
            Error::new(
                "input_readback_failed",
                format!("The training queue was created in process, but {op} was not posted (finish it in the panel)"),
            )
        })
    };
    let lines_of = |qs: &[Queue]| qs.iter().find(|x| x.address == q).map(|x| x.lines);
    let mut replies = vec![created];
    let filled = |route: &mut Option<Channel>, replies: &mut Vec<Value>| -> Result<(i32, usize)> {
        if queue.amount == 0 {
            replies.push(step(
                route,
                "training.series",
                &[1],
                "one division per line",
            )?);
            until(process, "the series amount", |qs| {
                qs.iter().any(|x| x.address == q && x.amount == 1)
            })?;
        }
        // the lines: one Add Unit post at a time, the queue read after each, only the remainder
        // posted (S15-TRAINING, 28 Nov 2026: 7 asked, the module posted 5 and 1 at once and the
        // queue stood at 9)
        let got = fill_lines(count, queue.lines.max(1), |v| {
            let before = lines_of(&ui::read(process, deployment::queues)?).unwrap_or(0);
            replies.push(step(
                route,
                "training.add_lines",
                &[v],
                "more lines for this queue",
            )?);
            let now = until(process, "the added lines", |qs| {
                lines_of(qs).is_some_and(|n| n > before)
            })?;
            Ok(lines_of(&now).unwrap_or(before))
        })?;
        replies.push(step(
            route,
            "training.location",
            &[i64::from(province)],
            "this deployment location",
        )?);
        until(process, "the deployment location", |qs| {
            qs.iter().any(|x| x.address == q && x.location == province)
        })?;
        Ok(got)
    };
    match filled(&mut *route, &mut replies) {
        Ok((lines, posts)) => Ok(Some(
            json!({"backend":"inprocess","queue":format!("{q:#x}"),
            "province":province,"lines":lines,"asked":count,"line_posts":posts,
            "over":(lines as i64 - count as i64).max(0),"posts":replies}),
        )),
        Err(e) => {
            // a refusal after the queue was made: the queue it made is cancelled (S15-TRAINING: a
            // refused add left a one-line queue behind)
            let removed = post(route, "training.remove", &[q], &[], "cancel this queue")
                .ok()
                .flatten()
                .is_some()
                && until(process, "the cancelled queue", |qs| lines_of(qs).is_none()).is_ok();
            Err(Error::new(
                e.code,
                format!(
                    "{} ({})",
                    e.message,
                    if removed {
                        "the queue this call had created was cancelled"
                    } else {
                        "the queue this call had created could not be cancelled: it stands in the deployment view"
                    }
                ),
            ))
        }
    }
}

/// The most lines one Add Unit post asks for (the Shift-click's five).
const LINES_PER_POST: i64 = 5;
/// Posts that add no line before the queue is given up.
const LINE_POSTS_MAX: usize = 12;

/// The queue's lines filled up to `count`, from `lines` standing: each `post(v)` asks for `v` more
/// lines and returns the lines the queue holds after it. Only the remainder is asked for, at most
/// five a post; when a post adds more than it asked (the game's own rounding), the surplus is
/// taken off the next ask. Returns (lines, posts). An overshoot of the last post stands (no
/// command takes a line off a queue); it is in the reply.
pub(super) fn fill_lines(
    count: usize,
    mut lines: i32,
    mut post: impl FnMut(i64) -> Result<i32>,
) -> Result<(i32, usize)> {
    let mut surplus = 0i64;
    let mut posts = 0usize;
    while (lines as i64) < count as i64 {
        if posts >= LINE_POSTS_MAX {
            return Err(Error::new(
                "input_readback_failed",
                format!("The training queue holds {lines} of {count} lines after {posts} posts"),
            ));
        }
        let left = count as i64 - lines as i64;
        let ask = (left - surplus).clamp(1, LINES_PER_POST);
        let now = post(ask)?;
        posts += 1;
        let added = (now - lines) as i64;
        if added <= 0 {
            return Err(Error::new(
                "input_readback_failed",
                format!("An Add Unit post for {ask} line(s) added none ({lines} of {count})"),
            ));
        }
        surplus = (added - ask).max(0);
        lines = now;
    }
    Ok((lines, posts))
}

/// Every own queue without a deployment province gets `province`. Ok(None): nothing was posted.
pub(super) fn locate(
    process: &dyn Process,
    route: &mut Option<Channel>,
    province: i32,
) -> Result<Option<Value>> {
    let open: Vec<u64> = ui::read(process, deployment::queues)?
        .into_iter()
        .filter(|q| q.location <= 0)
        .map(|q| q.address)
        .collect();
    if open.is_empty() {
        return Ok(Some(json!({"backend":"inprocess","posted":0})));
    }
    let objs: Vec<u64> = open.iter().copied().take(32).collect();
    let Some(reply) = post(
        route,
        "training.location",
        &objs,
        &[i64::from(province)],
        "this deployment location",
    )?
    else {
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
    fn lines_are_posted_as_the_remainder_after_each_read() {
        // the game as S15-TRAINING saw it: each post added one line more than it asked
        let mut asked = Vec::new();
        let mut q = 1;
        let (lines, posts) = fill_lines(7, 1, |v| {
            asked.push(v);
            q += v as i32 + 1;
            Ok(q)
        })
        .unwrap();
        assert_eq!((lines, posts), (7, 1));
        assert_eq!(asked, vec![5]);
        // an exact game: 12 asked from 1 is 5, 5, 1
        let mut asked = Vec::new();
        let mut q = 1;
        let got = fill_lines(12, 1, |v| {
            asked.push(v);
            q += v as i32;
            Ok(q)
        })
        .unwrap();
        assert_eq!(got, (12, 3));
        assert_eq!(asked, vec![5, 5, 1]);
        // the surplus learnt on the first post is taken off the next ask
        let mut asked = Vec::new();
        let mut q = 1;
        let got = fill_lines(12, 1, |v| {
            asked.push(v);
            q += v as i32 + 1;
            Ok(q)
        })
        .unwrap();
        assert_eq!(got, (12, 2));
        assert_eq!(asked, vec![5, 4]);
        // nothing to add, or a post that adds nothing
        assert_eq!(fill_lines(1, 1, |_| unreachable!()).unwrap(), (1, 0));
        assert!(fill_lines(3, 1, |_| Ok(1)).is_err());
        // a refusal is passed on (the caller cancels the queue)
        let e = fill_lines(6, 1, |_| {
            Err(Error::new("refused_by_game_check", "more lines"))
        })
        .unwrap_err();
        assert_eq!(e.code, "refused_by_game_check");
    }

    #[test]
    fn the_requests_name_the_queue_and_the_values() {
        let r = request("training.create", &[0x7000_1000], &[], &[], None).unwrap();
        assert_eq!(r["cmd"], "training.create");
        let r = request("training.add_lines", &[0x7000_2000], &[], &[23], None).unwrap();
        assert_eq!(r["vals"], "23");
        let r = request(
            "training.location",
            &[0x7000_2000, 0x7000_3000],
            &[],
            &[6521],
            None,
        )
        .unwrap();
        assert_eq!(
            (r["objs"].as_str(), r["vals"].as_str()),
            (Some("0x70002000,0x70003000"), Some("6521"))
        );
    }
}
