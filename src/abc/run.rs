//! Run configuration: date resolution (`get_json_date` equivalent, spec 1.1).
#![allow(dead_code)]

use anyhow::{anyhow, Result};
use chrono::{Datelike, Duration, NaiveDate};

/// Resolved run date: `date1`, plus `date2` for ranges (inclusive).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RunDate {
    pub date1: NaiveDate,
    pub date2: Option<NaiveDate>,
}

impl RunDate {
    /// `(start, end)` for tools that always want both; a single date is `(d, d)`.
    pub fn as_range(&self) -> (NaiveDate, NaiveDate) {
        (self.date1, self.date2.unwrap_or(self.date1))
    }
}

/// Sunday that starts week `week` of `year`, using Excel `WEEKNUM` defaults: week 1 is the
/// week containing Jan 1 and weeks start on Sunday. Week 45/2026 starts 2026-11-01.
pub fn week_start(week: u32, year: i32) -> Result<NaiveDate> {
    if !(1..=53).contains(&week) {
        return Err(anyhow!("week {week} out of range (1-53)"));
    }
    let jan1 = NaiveDate::from_ymd_opt(year, 1, 1).ok_or_else(|| anyhow!("invalid year {year}"))?;
    let week1_start = jan1 - Duration::days(i64::from(jan1.weekday().num_days_from_sunday()));
    Ok(week1_start + Duration::days(7 * i64::from(week - 1)))
}

/// Sunday..Saturday range of a week.
pub fn week_range(week: u32, year: i32) -> Result<(NaiveDate, NaiveDate)> {
    let start = week_start(week, year)?;
    Ok((start, start + Duration::days(6)))
}

fn parse_iso(s: &str) -> Result<NaiveDate> {
    NaiveDate::parse_from_str(s.trim(), "%Y-%m-%d")
        .map_err(|e| anyhow!("invalid date '{s}': expected YYYY-MM-DD ({e})"))
}

/// Parse a run date. Accepted forms (case-insensitive):
/// - `YYYY-MM-DD` single date
/// - `YYYY-MM-DD..YYYY-MM-DD` inclusive range
/// - `WK45` or `WK45/2026` week range (year defaults to `default_year`)
pub fn parse_run_date(raw: &str, default_year: i32) -> Result<RunDate> {
    let s = raw.trim();
    let upper = s.to_ascii_uppercase();
    if let Some(rest) = upper.strip_prefix("WK") {
        let (w, y) = match rest.split_once('/') {
            Some((w, y)) => (
                w,
                y.parse::<i32>()
                    .map_err(|_| anyhow!("invalid year in '{raw}'"))?,
            ),
            None => (rest, default_year),
        };
        let week = w
            .parse::<u32>()
            .map_err(|_| anyhow!("invalid week in '{raw}': expected WK<number>[/<year>]"))?;
        let (d1, d2) = week_range(week, y)?;
        return Ok(RunDate {
            date1: d1,
            date2: Some(d2),
        });
    }
    if let Some((a, b)) = s.split_once("..") {
        let (d1, d2) = (parse_iso(a)?, parse_iso(b)?);
        if d2 < d1 {
            return Err(anyhow!("range end {d2} is before start {d1}"));
        }
        return Ok(RunDate {
            date1: d1,
            date2: Some(d2),
        });
    }
    Ok(RunDate {
        date1: parse_iso(s)?,
        date2: None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn d(y: i32, m: u32, day: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, day).unwrap()
    }

    #[test]
    fn week_45_2026() {
        assert_eq!(
            week_range(45, 2026).unwrap(),
            (d(2026, 11, 1), d(2026, 11, 7))
        );
    }

    #[test]
    fn week_31_2026() {
        assert_eq!(
            week_range(31, 2026).unwrap(),
            (d(2026, 7, 26), d(2026, 8, 1))
        );
    }

    #[test]
    fn week_1_starts_in_previous_year() {
        // 2026-01-01 is a Thursday; week 1 starts Sunday 2025-12-28
        assert_eq!(week_start(1, 2026).unwrap(), d(2025, 12, 28));
    }

    #[test]
    fn week_out_of_range() {
        assert!(week_start(0, 2026).is_err());
        assert!(week_start(54, 2026).is_err());
    }

    #[test]
    fn parse_forms() {
        let r = parse_run_date("WK45", 2026).unwrap();
        assert_eq!(r.as_range(), (d(2026, 11, 1), d(2026, 11, 7)));
        assert_eq!(parse_run_date("wk45/2026", 1999).unwrap(), r);
        assert_eq!(
            parse_run_date("2026-11-05", 2026).unwrap(),
            RunDate {
                date1: d(2026, 11, 5),
                date2: None
            }
        );
        assert_eq!(
            parse_run_date("2026-11-01..2026-11-07", 2026)
                .unwrap()
                .date2,
            Some(d(2026, 11, 7))
        );
        assert!(parse_run_date("2026-11-07..2026-11-01", 2026).is_err());
        assert!(parse_run_date("WKx", 2026).is_err());
        assert!(parse_run_date("garbage", 2026).is_err());
    }
}
