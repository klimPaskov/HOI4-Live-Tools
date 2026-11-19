//! Where an order's real seconds go: every action tool call (one with a `request_id`) is an
//! order; while it runs, the input backend, the camera helpers, the panel waits, the read-back
//! settlement and every `phase::sleep` charge their time to a named part of the order on the
//! calling thread. Time is exclusive: a sleep inside a wait counts as sleep, not as wait. What
//! no part claims (memory reads, computing) is `other`. The parts are written with the order
//! (`[order]` lines on stderr) so a bed run gives a table by order kind.
// The orders are opened by the macOS adapter; elsewhere only `sleep` is used yet.
#![cfg_attr(not(target_os = "macos"), allow(dead_code))]
use serde_json::{json, Value};
use std::cell::RefCell;
use std::collections::BTreeMap;
use std::time::{Duration, Instant};

struct Frame {
    label: &'static str,
    start: Instant,
    child: Duration,
}
#[derive(Default)]
struct Book {
    active: bool,
    stack: Vec<Frame>,
    parts: BTreeMap<&'static str, (u32, Duration)>,
    sleeps: BTreeMap<String, (u32, Duration)>,
}
thread_local! {
    static BOOK: RefCell<Book> = RefCell::new(Book::default());
}

/// One named part while it runs (dropped = ended). Outside an order it records nothing.
pub(crate) struct Phase(bool);
pub(crate) fn enter(label: &'static str) -> Phase {
    BOOK.with(|b| {
        let mut b = b.borrow_mut();
        if !b.active {
            return Phase(false);
        }
        b.stack.push(Frame {
            label,
            start: Instant::now(),
            child: Duration::ZERO,
        });
        Phase(true)
    })
}
impl Drop for Phase {
    fn drop(&mut self) {
        if !self.0 {
            return;
        }
        BOOK.with(|b| {
            let mut b = b.borrow_mut();
            let Some(f) = b.stack.pop() else { return };
            let spent = f.start.elapsed();
            let e = b.parts.entry(f.label).or_default();
            e.0 += 1;
            e.1 += spent.saturating_sub(f.child);
            if let Some(parent) = b.stack.last_mut() {
                parent.child += spent;
            }
        });
    }
}

/// `std::thread::sleep` that names where it sleeps. Directly in an order's code it is the
/// order's `sleep` part (a fixed sleep); inside a named part (a wait's poll, the input
/// backend's settle) it stays that part's time and is listed under the part's name.
#[track_caller]
pub(crate) fn sleep(d: Duration) {
    let at = std::panic::Location::caller();
    let inside = BOOK.with(|b| {
        let b = b.borrow();
        b.active.then(|| b.stack.last().map(|f| f.label))
    });
    let Some(inside) = inside else {
        std::thread::sleep(d);
        return;
    };
    let p = inside.is_none().then(|| enter("sleep"));
    std::thread::sleep(d);
    drop(p);
    BOOK.with(|b| {
        let mut b = b.borrow_mut();
        let file = at.file().rsplit('/').next().unwrap_or(at.file());
        let key = match inside {
            Some(part) => format!("{part}:{file}:{}", at.line()),
            None => format!("sleep:{file}:{}", at.line()),
        };
        let e = b.sleeps.entry(key).or_default();
        e.0 += 1;
        e.1 += d;
    });
}

/// Names a stretch of an order's time (a named wait's stage) in the order's site list.
pub(crate) fn note(site: &str, d: Duration) {
    BOOK.with(|b| {
        let mut b = b.borrow_mut();
        if !b.active {
            return;
        }
        let e = b.sleeps.entry(site.to_string()).or_default();
        e.0 += 1;
        e.1 += d;
    });
}

/// An order's book; nested orders (an action calling an action) are part of the outer one.
pub(crate) struct Order {
    started: Instant,
    outer: bool,
}
pub(crate) fn begin() -> Order {
    let outer = BOOK.with(|b| {
        let mut b = b.borrow_mut();
        if b.active {
            return false;
        }
        *b = Book {
            active: true,
            ..Book::default()
        };
        true
    });
    Order {
        started: Instant::now(),
        outer,
    }
}
impl Order {
    /// The parts in ms (`total`, each part `[calls, ms]`, `other`, the five longest sleep sites);
    /// None for a nested order.
    pub(crate) fn finish(self) -> Option<Value> {
        if !self.outer {
            return None;
        }
        let total = self.started.elapsed();
        let book = BOOK.with(|b| std::mem::take(&mut *b.borrow_mut()));
        let claimed: Duration = book.parts.values().map(|p| p.1).sum();
        let mut parts = serde_json::Map::new();
        for (k, (n, d)) in &book.parts {
            parts.insert((*k).into(), json!([n, d.as_millis() as u64]));
        }
        parts.insert(
            "other".into(),
            json!([1, total.saturating_sub(claimed).as_millis() as u64]),
        );
        let mut sleeps: Vec<_> = book.sleeps.into_iter().collect();
        sleeps.sort_by_key(|s| std::cmp::Reverse(s.1 .1));
        Some(json!({
            "total": total.as_millis() as u64,
            "parts": parts,
            "sleeps": sleeps.iter().take(10)
                .map(|(at, (n, d))| format!("{at} x{n} {}", d.as_millis()))
                .collect::<Vec<_>>(),
        }))
    }
}
impl Drop for Order {
    fn drop(&mut self) {
        if self.outer {
            BOOK.with(|b| b.borrow_mut().active = false);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parts_are_exclusive_and_the_rest_is_other() {
        let order = begin();
        {
            let _w = enter("wait");
            sleep(Duration::from_millis(30));
            std::thread::sleep(Duration::from_millis(20));
        }
        let v = order.finish().unwrap();
        let wait_ms = v["parts"]["wait"][1].as_u64().unwrap();
        assert!(v["parts"]["sleep"].is_null(), "{v}");
        assert!((50..80).contains(&wait_ms), "{v}");
        assert!(v["sleeps"][0]
            .as_str()
            .unwrap()
            .starts_with("wait:phase.rs:"));
        let order = begin();
        sleep(Duration::from_millis(10));
        let v = order.finish().unwrap();
        assert!(v["parts"]["sleep"][1].as_u64().unwrap() >= 10, "{v}");
    }

    #[test]
    fn nothing_is_recorded_outside_an_order_and_nested_orders_fold_in() {
        sleep(Duration::from_millis(1));
        let outer = begin();
        let inner = begin();
        assert!(inner.finish().is_none());
        let _p = enter("input.click");
        drop(_p);
        let v = outer.finish().unwrap();
        assert_eq!(v["parts"]["input.click"][0], 1);
        // after an order the book is closed again
        let p = enter("wait");
        assert!(!p.0);
    }
}
