//! THE GROUND AN ATTACK STARTS ON (engineer 110, 28 Nov 2026): the declaration that opens an
//! operation waits while the ground of the attack's regions is bad (mud more likely than not, or
//! medium / high ground snow, read with hoi4_weather from the map's own weather tables) and clears
//! within the wait the plan allows; past that it goes ahead and says what the ground costs.
//!
//! Why: on 1.20.1.0 mud takes 40% of the attackers' attack and half their speed, deep ground snow
//! 40% attack and 70% speed (the weather_* static modifiers); a planning bonus or a date that
//! cannot be had on dry ground is not worth a week of attacks at 0.6 of their strength. A long
//! winter or a monsoon does not hold the war: when the ground does not clear within the wait, the
//! attack starts and the note carries the expected attack factor for the pace.
use serde_json::Value;

/// Hold the declaration? `answer` is hoi4_weather's reply for the attack's regions, `wait` the
/// days the plan allows past its date, `waited` the days already past it. Returns the reason to
/// hold, or None with the note for the pace (Some(note) in the second slot) when the war goes on.
pub(super) fn judge(answer: &Value, wait: i64, waited: i64) -> (Option<String>, Option<String>) {
    let s = &answer["summary"];
    let bad = s["bad_ground_regions"].as_array().map_or(0, Vec::len);
    let all = answer["regions"]
        .as_array()
        .map_or(0, |r| r.iter().filter(|x| x["outlook"].is_object()).count());
    if all == 0 {
        return (None, None);
    }
    let attack = s["attack_factor_now_worst"].as_f64().unwrap_or(0.0);
    if bad * 2 <= all {
        let note = (attack < -0.05).then(|| {
            format!("ground: expected attack factor {attack:+.2} in the worst of {all} regions")
        });
        return (None, note);
    }
    let clears = s["bad_ground_clears_in_days"].as_i64();
    let left = wait - waited;
    match clears {
        Some(d) if d > 0 && d <= left => (
            Some(format!(
                "bad ground in {bad} of {all} attack regions (attack factor {attack:+.2}) clears in \
                 {d} days, within the {wait}-day wait: the declaration waits"
            )),
            None,
        ),
        _ => (
            None,
            Some(format!(
                "bad ground in {bad} of {all} attack regions (attack factor {attack:+.2}) does not \
                 clear within the wait: the attack goes ahead at that cost"
            )),
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn answer(bad: &[i64], regions: usize, clears: Value, attack: f64) -> Value {
        let rows: Vec<Value> = (0..regions)
            .map(|i| json!({"region": i, "outlook": {}}))
            .collect();
        json!({"regions": rows, "summary": {"bad_ground_regions": bad,
            "bad_ground_clears_in_days": clears, "attack_factor_now_worst": attack}})
    }

    #[test]
    fn mud_that_clears_within_the_wait_holds_the_declaration() {
        let (hold, _) = judge(&answer(&[1, 2], 3, json!(5), -0.4), 14, 0);
        assert!(hold.unwrap().contains("clears in 5 days"));
        // the wait is spent: the attack goes on and says what it costs
        let (hold, note) = judge(&answer(&[1, 2], 3, json!(5), -0.4), 14, 12);
        assert!(hold.is_none());
        assert!(note.unwrap().contains("goes ahead"));
    }

    #[test]
    fn a_long_winter_or_dry_ground_never_holds() {
        let (hold, note) = judge(&answer(&[1, 2], 2, Value::Null, -0.48), 14, 0);
        assert!(hold.is_none() && note.is_some());
        let (hold, note) = judge(&answer(&[], 3, json!(0), 0.0), 14, 0);
        assert!(hold.is_none() && note.is_none());
        // no read (an old binary, no game files): nothing to judge
        assert_eq!(judge(&json!({}), 14, 0), (None, None));
    }
}
