//! Training through the in-process module (engineer 82, family `training`): the deployment view's own posts, each
//! checked by the game's own "can do" (and, for the Train button, the game's "can train" the view calls first).
//! - hoi4_train: CreateConveyor(template) as the template row's Train button; the new queue read back from the own
//!   deployment status; SetConveyorSeries(queue, 1) as its + (a new queue trains forever); AddConveyorLine(queue, n)
//!   as Add Unit clicks, ONE LINE a post, the queue's lines read after each until they stand still, and posted
//!   again only while short (S15-TRAINING on master: a post of 5 left the queue at 9 for 7 asked; posts of 1 and 2
//!   were exact, so a line a post is the exact path); SetConveyorLocation(queue, a province of
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
    // the game's own check refusing a later line (a battalion cap, manpower): the lines it took
    // stay and the reply says why it stopped (live 126, s126a 28 Nov 2026: a Gebirgs-Division
    // queue of 7 refused its 5th line and the whole 4-line queue was cancelled as "nothing posted")
    let mut stopped_by: Option<String> = None;
    let mut filled =
        |route: &mut Option<Channel>, replies: &mut Vec<Value>| -> Result<(i32, usize)> {
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
            // the lines: one Add Unit post of ONE line at a time, the queue read after each until it
            // stands still, and posted again only while short (S15-TRAINING, 28 Nov 2026: 7 asked, posts
            // of 5 and then the remainder left the queue at 9; asks of 1 and 2 were exact)
            let got = fill_lines(count, queue.lines.max(1), |v| {
                let before = lines_of(&ui::read(process, deployment::queues)?).unwrap_or(0);
                match step(
                    route,
                    "training.add_lines",
                    &[v],
                    "more lines for this queue",
                ) {
                    Ok(reply) => replies.push(reply),
                    Err(e) if e.code == "refused_by_game_check" && before >= 1 => {
                        stopped_by = Some(e.message);
                        return Ok(before);
                    }
                    Err(e) => return Err(e),
                }
                // a post the game takes but does not carry out (its own check when the command runs;
                // S15-TRAINING s115a, 28 Nov 2026: 7 asked, the line count stopped at 5 at war in 1939)
                // adds nothing: the lines standing are the answer, the queue is kept
                let Ok(now) = until(process, "the added lines", |qs| {
                    lines_of(qs).is_some_and(|n| n > before)
                }) else {
                    return Ok(before);
                };
                let first = lines_of(&now).unwrap_or(before);
                // a post may show its lines over more than one frame: the count it answers is the one
                // that stands still over two reads
                settled(first, || {
                    crate::phase::sleep(Duration::from_millis(SETTLE_MS));
                    Ok(lines_of(&ui::read(process, deployment::queues)?).unwrap_or(first))
                })
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
        Ok((lines, posts)) => {
            let short = (count as i64 - lines as i64).max(0);
            let mut out = json!({"backend":"inprocess","queue":format!("{q:#x}"),
            "province":province,"lines":lines,"asked":count,"line_posts":posts,
            "over":(lines as i64 - count as i64).max(0),"short":short,"posts":replies});
            if short > 0 {
                out["short_reason"] = json!(match &stopped_by {
                    Some(why) => format!(
                        "the game's own check refused line {} of {count} before it was posted ({why}); the queue keeps its {lines} line(s): a cap of the template (e.g. special forces) or manpower",
                        lines + 1
                    ),
                    None => format!(
                        "the game took the Add Unit post but added no line at {lines} of {count} (its own check when the command runs, e.g. manpower or the template); the queue keeps its {lines} line(s): ask again later"
                    ),
                });
                if stopped_by.is_some() {
                    out["short_code"] = json!("refused_by_game_check");
                }
            }
            Ok(Some(out))
        }
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

/// The lines one Add Unit post asks for: one (S15-TRAINING: a post of 5 overshot, asks of 1 and 2 were exact).
const LINES_PER_POST: i64 = 1;
/// The wait between two reads of a queue's lines after a post (about three game frames).
const SETTLE_MS: u64 = 50;
/// Reads after a post before its count is taken as it stands.
const SETTLE_READS: usize = 6;

/// The lines a queue stands at after a post: re-read (`read`) until two reads agree, at most
/// SETTLE_READS times; the last read is the answer when they never agree.
pub(super) fn settled(first: i32, mut read: impl FnMut() -> Result<i32>) -> Result<i32> {
    let mut last = first;
    for _ in 0..SETTLE_READS {
        let now = read()?;
        if now == last {
            return Ok(now);
        }
        last = now;
    }
    Ok(last)
}

/// The queue's lines filled up to `count`, from `lines` standing: each `post(v)` asks for `v` more
/// lines and returns the lines the queue holds after it (settled). One line is asked a post, and
/// a post is made only while the queue is short, so the queue never passes `count` by more than
/// the game adds to a single one-line post (which is in the reply as `over`; no command takes a
/// line off a queue). A post that adds nothing ends the filling short (the game's own check when the
/// command runs: the lines it took stand, the caller reports the shortfall). Returns (lines, posts).
pub(super) fn fill_lines(
    count: usize,
    mut lines: i32,
    mut post: impl FnMut(i64) -> Result<i32>,
) -> Result<(i32, usize)> {
    let mut posts = 0usize;
    while (lines as i64) < count as i64 {
        let left = count as i64 - lines as i64;
        let ask = left.clamp(1, LINES_PER_POST);
        let now = post(ask)?;
        posts += 1;
        if now <= lines {
            break;
        }
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
    fn lines_are_posted_one_a_post_while_the_queue_is_short() {
        // S15-TRAINING on master (s96c): 7 asked from a 1-line queue; one post a line, each exact,
        // gives 7 in 6 posts (never 9)
        let mut asked = Vec::new();
        let mut q = 1;
        let (lines, posts) = fill_lines(7, 1, |v| {
            asked.push(v);
            q += v as i32;
            Ok(q)
        })
        .unwrap();
        assert_eq!((lines, posts), (7, 6));
        assert!(asked.iter().all(|&v| v == 1));
        // a game that adds two for one post: no further post once the count is reached, the
        // overshoot is at most that one post's extra (8 for 7, not 9)
        let mut q = 1;
        let got = fill_lines(7, 1, |v| {
            q += if q == 5 { 2 } else { v as i32 };
            Ok(q)
        })
        .unwrap();
        assert_eq!(got, (7, 5));
        let mut q = 1;
        let got = fill_lines(6, 1, |v| {
            q += if q == 5 { 2 } else { v as i32 };
            Ok(q)
        })
        .unwrap();
        assert_eq!(got, (7, 5));
        // nothing to add, or a post that adds nothing
        assert_eq!(fill_lines(1, 1, |_| unreachable!()).unwrap(), (1, 0));
        assert_eq!(fill_lines(3, 1, |_| Ok(1)).unwrap(), (1, 1));
        // s115a: the count stops rising at 5 of 7: five lines stand, the sixth post ends it
        let mut q = 1;
        let got = fill_lines(7, 1, |_| {
            q = (q + 1).min(5);
            Ok(q)
        })
        .unwrap();
        assert_eq!(got, (5, 5));
        // a refusal is passed on (the caller cancels the queue)
        let e = fill_lines(6, 1, |_| {
            Err(Error::new("refused_by_game_check", "more lines"))
        })
        .unwrap_err();
        assert_eq!(e.code, "refused_by_game_check");
    }

    #[test]
    fn a_post_s_lines_are_the_count_that_stands_still() {
        // the first read after a post saw part of it (6 of 8): the next reads settle on 8
        let mut reads = vec![8, 8].into_iter();
        assert_eq!(settled(6, || Ok(reads.next().unwrap())).unwrap(), 8);
        // already still
        assert_eq!(settled(3, || Ok(3)).unwrap(), 3);
        // never still: the last read
        let mut n = 0;
        assert_eq!(
            settled(0, || {
                n += 1;
                Ok(n)
            })
            .unwrap(),
            SETTLE_READS as i32
        );
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
