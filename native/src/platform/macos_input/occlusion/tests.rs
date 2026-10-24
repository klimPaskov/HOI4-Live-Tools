use super::*;
use std::cell::{Cell, RefCell};
const POINT: [f64; 2] = [960.0, 500.0];
const CURSOR_LEVEL: i32 = 2_147_483_630;

#[test]
fn unrelated_titles_are_never_decoded_or_rejected_for_length() {
    let result = candidate_names(3, CURSOR_LEVEL, [660.0, 372.0, 600.0, 500.0], POINT, || {
        panic!("An unrelated overlay title with arbitrary length must not be read")
    })
    .unwrap();
    assert_eq!(result, (None, None));
}
#[test]
fn off_point_cursor_labels_are_not_read() {
    let result = candidate_names(
        CURSOR_LEVEL,
        CURSOR_LEVEL,
        [10.0, 10.0, 32.0, 32.0],
        POINT,
        || panic!("Non-hit cursor label cannot affect the point exception"),
    )
    .unwrap();
    assert_eq!(result, (None, None));
}
#[test]
fn cursor_candidate_preserves_exact_labels_and_read_errors() {
    let result = candidate_names(
        CURSOR_LEVEL,
        CURSOR_LEVEL,
        [948.0, 489.0, 23.0, 22.0],
        POINT,
        || Ok((Some("Window Server".into()), Some("Cursor".into()))),
    )
    .unwrap();
    assert_eq!(
        result,
        (Some("Window Server".into()), Some("Cursor".into()))
    );
    assert_eq!(
        candidate_names(
            CURSOR_LEVEL,
            CURSOR_LEVEL,
            [948.0, 489.0, 23.0, 22.0],
            POINT,
            || Err(Error::new(
                "input_window_metadata",
                "Malformed candidate title"
            ))
        )
        .unwrap_err()
        .code,
        "input_window_metadata"
    );
}
#[test]
fn movement_during_last_owner_lookup_is_rejected_by_final_cursor_sample() {
    let cursor = Cell::new(POINT);
    let trace = RefCell::new(Vec::new());
    let result = finish_cursor_observation(
        POINT,
        || {
            trace.borrow_mut().push("final cursor");
            Ok(cursor.get())
        },
        || {
            trace.borrow_mut().push("first owner");
            assert_eq!(cursor.get(), POINT);
            trace.borrow_mut().push("last owner");
            cursor.set([961.0, 500.0]);
            Ok(())
        },
    );
    assert_eq!(result.unwrap_err().code, "input_cursor_changed");
    assert_eq!(
        trace.into_inner(),
        vec!["first owner", "last owner", "final cursor"]
    );
}
#[test]
fn stable_cursor_is_read_after_all_owner_work_and_retains_result() {
    let owner_done = Cell::new(false);
    let result = finish_cursor_observation(
        POINT,
        || {
            assert!(owner_done.get());
            Ok(POINT)
        },
        || {
            owner_done.set(true);
            Ok(42)
        },
    )
    .unwrap();
    assert_eq!(result, 42);
}
#[test]
fn coherent_owner_refusal_keeps_cause_without_rescue_or_retry() {
    let result: Result<()> = finish_cursor_observation(
        POINT,
        || panic!("A changed owner cannot be rescued by a later restored cursor"),
        || Err(Error::new("owner_changed", "First coherent refusal")),
    );
    assert_eq!(result.unwrap_err().code, "owner_changed");
}
#[test]
fn unavailable_final_cursor_cannot_authorize_admission() {
    let result = finish_cursor_observation(
        POINT,
        || Err(Error::new("input_unavailable", "No current cursor")),
        || Ok(()),
    );
    assert_eq!(result.unwrap_err().code, "input_unavailable");
}
