// Derived from Clausewitz-MCP calling/api.py, Copyright (c) 2026 smerss (MIT).
// Current-build epoch and text-hour convention independently recovered.
use crate::error::{Error, Result};
use serde::Serialize;
const EPOCH: i64 = 43_800_000;
const MONTHS: [i64; 12] = [31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31];
#[derive(Debug, Serialize, PartialEq, Eq)]
pub struct Date {
    pub raw: i32,
    pub year: i64,
    pub month: usize,
    pub day: i64,
    pub display_hour: i64,
    pub text: String,
}
pub fn decode(raw: i32) -> Result<Date> {
    let hours = i64::from(raw) - EPOCH;
    let year = hours.div_euclid(8760);
    if !(1936..=2000).contains(&year) {
        return Err(Error::new(
            "invalid_date",
            "Outside verified campaign calendar",
        ));
    }
    let mut day = hours.rem_euclid(8760) / 24;
    let display_hour = hours.rem_euclid(24) + 1;
    let mut month = 1;
    for length in MONTHS {
        if day < length {
            break;
        }
        day -= length;
        month += 1;
    }
    day += 1;
    Ok(Date {
        raw,
        year,
        month,
        day,
        display_hour,
        text: format!("{year:04}.{month:02}.{day:02}.{display_hour:02}"),
    })
}
pub fn encode(year: i64, month: usize, day: i64) -> Result<i32> {
    if !(1936..=2000).contains(&year)
        || !(1..=12).contains(&month)
        || !(1..=MONTHS[month - 1]).contains(&day)
    {
        return Err(Error::new(
            "invalid_date",
            "Expected a date in the verified 365-day calendar",
        ));
    }
    Ok((EPOCH + 24 * (365 * year + MONTHS[..month - 1].iter().sum::<i64>() + day - 1)) as i32)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn actual_initial_clock() {
        assert_eq!(decode(60_759_371).unwrap().text, "1936.01.01.12");
    }
    #[test]
    fn calendar_boundaries() {
        assert!(encode(1936, 2, 29).is_err());
        assert!(encode(1936, 0, 1).is_err());
        assert!(encode(1936, 13, 1).is_err());
        assert!(encode(1935, 12, 31).is_err());
        assert_eq!(
            decode(encode(1936, 3, 1).unwrap() - 1).unwrap().text,
            "1936.02.28.24"
        );
        assert_eq!(
            decode(encode(1937, 1, 1).unwrap() - 1).unwrap().text,
            "1936.12.31.24"
        );
    }
}
