//! rName registry: which steps a report runs and where its output goes (spec section 5, item 11).
//! Only `mustgo` is defined so far. Steps are declarative; executors are wired in later phases.
#![allow(dead_code)]

use super::run::RunDate;
use chrono::NaiveDate;
use std::path::{Path, PathBuf};

/// One step of a report run. Mirrors the spec 5.1 rows; SAP steps are stubs until Phase 4.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Step {
    /// VT11 export (config-agnostic params resolved by caller).
    Vt11,
    /// Keep rows of `vt11` with `transportplanningpt = TPDC`.
    FilterVt11Tpdc,
    /// VL06O by shipment numbers from `vt11`.
    Vl06o,
    /// ZMDESNR by deliveries from `vl06o`.
    ZmdesnrByDeliveries,
    /// Selection/dedup of `sn_del` (spec rows 13-15).
    FilterSnDel,
    /// LT23 by transfer orders from `sn_del`, then `mustgo` post-process.
    Lt23,
    /// Lookup/delete of `sn_del` rows with no matching TO (rows 17-18).
    LookupLt23,
    /// ZMDESNR by deliveries + TOs, `Filter_Copy_MG`, week/export_date columns.
    ZmdesnrByDeliveriesAndTos,
    /// ZMDETPC summarised by deliveries from `vl06o` (xdock/toyota).
    Zmdetpc,
    /// Merge ZMDETPC into `vl06o` (spec 4.12, xdock/toyota rules).
    MergeZmdetpc,
    /// Trailer estimate per plant (toyota, spec 5.2 rows 31-32).
    Trailers,
    /// Write output tables.
    Save,
}

#[derive(Debug, Clone, Copy)]
pub struct ReportDef {
    pub name: &'static str,
    /// Output folder under the reports dir (`K2XX`, `INV`, `ABC`, or the rName itself).
    pub folder: &'static str,
    pub steps: &'static [Step],
    /// Tables written by the save step (spec 5.1: `LT23`, `SNR_COPY`, `PIVOT`).
    pub output_tables: &'static [&'static str],
}

pub const MUSTGO: ReportDef = ReportDef {
    name: "mustgo",
    folder: "K2XX",
    steps: &[
        Step::Vt11,
        Step::FilterVt11Tpdc,
        Step::Vl06o,
        Step::ZmdesnrByDeliveries,
        Step::FilterSnDel,
        Step::Lt23,
        Step::LookupLt23,
        Step::ZmdesnrByDeliveriesAndTos,
        Step::Save,
    ],
    output_tables: &["lt23", "snr_copy", "pivot"],
};

/// `xdock` (spec 5.3): VT11 -> VL06O (layout `xdock`) -> ZMDETPC -> merge -> pivot plant/total.
/// The ZVT11 Closed step of the macro (`A!38`) is dropped: nothing uses its output (item 10).
pub const XDOCK: ReportDef = ReportDef {
    name: "xdock",
    folder: "INV",
    steps: &[
        Step::Vt11,
        Step::Vl06o,
        Step::Zmdetpc,
        Step::MergeZmdetpc,
        Step::Save,
    ],
    output_tables: &["vt11", "vl06o", "pivot"],
};

/// `toyota` (spec 5.2): the `xdock` flow with VT11 variant `FG_TOYO_XD`, plus a trailers table
/// (`ROUND(total / 14.5)` per plant) written next to the pivot instead of cells on its sheet.
pub const TOYOTA: ReportDef = ReportDef {
    name: "toyota",
    folder: "INV",
    steps: &[
        Step::Vt11,
        Step::Vl06o,
        Step::Zmdetpc,
        Step::MergeZmdetpc,
        Step::Trailers,
        Step::Save,
    ],
    output_tables: &["vt11", "vl06o", "pivot", "trailers"],
};

const REPORTS: &[ReportDef] = &[MUSTGO, XDOCK, TOYOTA];

/// Case-insensitive lookup by rName.
pub fn find_report(name: &str) -> Option<&'static ReportDef> {
    REPORTS
        .iter()
        .find(|r| r.name.eq_ignore_ascii_case(name.trim()))
}

pub fn report_names() -> Vec<&'static str> {
    REPORTS.iter().map(|r| r.name).collect()
}

/// `<date>` part of output file names: `MM-DD-YYYY` for a single date, `WK_<n>` for a range
/// (week number per Excel `WEEKNUM`, Sunday start, taken from the range start).
pub fn date_label(d: &RunDate) -> String {
    match d.date2 {
        Some(end) if end != d.date1 => format!("WK_{}", excel_weeknum(d.date1)),
        _ => d.date1.format("%m-%d-%Y").to_string(),
    }
}

/// Excel `WEEKNUM(date)` default (week 1 contains Jan 1, weeks start Sunday).
pub fn excel_weeknum(date: NaiveDate) -> u32 {
    use chrono::{Datelike, Duration};
    let jan1 = NaiveDate::from_ymd_opt(date.year(), 1, 1).expect("valid jan 1");
    let w1 = jan1 - Duration::days(i64::from(jan1.weekday().num_days_from_sunday()));
    ((date - w1).num_days() / 7) as u32 + 1
}

/// Output file: `<reports_dir>\<folder>\<today yyyy-mm-dd>\<rName>\<DATE>_<NAME>_<table>.tsv`.
pub fn output_path(
    reports_dir: &Path,
    def: &ReportDef,
    today: NaiveDate,
    run_date: &RunDate,
    table: &str,
) -> PathBuf {
    reports_dir
        .join(def.folder)
        .join(today.format("%Y-%m-%d").to_string())
        .join(def.name)
        .join(format!(
            "{}_{}_{}.tsv",
            date_label(run_date),
            def.name.to_ascii_uppercase(),
            table.to_ascii_uppercase()
        ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn d(y: i32, m: u32, day: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, day).unwrap()
    }

    #[test]
    fn lookup_is_case_insensitive() {
        assert_eq!(find_report(" MustGo ").unwrap().folder, "K2XX");
        assert!(find_report("nope").is_none());
    }

    #[test]
    fn labels() {
        let single = RunDate {
            date1: d(2026, 11, 5),
            date2: None,
        };
        assert_eq!(date_label(&single), "11-05-2026");
        let week = RunDate {
            date1: d(2026, 11, 1),
            date2: Some(d(2026, 11, 7)),
        };
        assert_eq!(date_label(&week), "WK_45");
    }

    #[test]
    fn weeknum_matches_run_rs() {
        assert_eq!(excel_weeknum(d(2026, 11, 1)), 45);
        assert_eq!(excel_weeknum(d(2026, 1, 1)), 1);
        assert_eq!(excel_weeknum(d(2025, 12, 28)), 53);
    }

    #[test]
    fn xdock_output_path() {
        let rd = RunDate {
            date1: d(2026, 11, 5),
            date2: None,
        };
        let p = output_path(Path::new("reports"), &XDOCK, d(2026, 11, 10), &rd, "vl06o");
        assert_eq!(
            p,
            Path::new("reports")
                .join("INV")
                .join("2026-11-10")
                .join("xdock")
                .join("11-05-2026_XDOCK_VL06O.tsv")
        );
        assert_eq!(find_report("XDock").unwrap().folder, "INV");
        let p = output_path(Path::new("r"), &TOYOTA, d(2026, 11, 10), &rd, "trailers");
        assert_eq!(
            p,
            Path::new("r")
                .join("INV")
                .join("2026-11-10")
                .join("toyota")
                .join("11-05-2026_TOYOTA_TRAILERS.tsv")
        );
    }

    #[test]
    fn mustgo_output_path() {
        let rd = RunDate {
            date1: d(2026, 11, 5),
            date2: None,
        };
        let p = output_path(
            Path::new("reports"),
            &MUSTGO,
            d(2026, 11, 9),
            &rd,
            "snr_copy",
        );
        assert_eq!(
            p,
            Path::new("reports")
                .join("K2XX")
                .join("2026-11-09")
                .join("mustgo")
                .join("11-05-2026_MUSTGO_SNR_COPY.tsv")
        );
    }
}
