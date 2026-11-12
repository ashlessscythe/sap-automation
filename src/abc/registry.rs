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
    /// `misc`: replace an marked shipping point by the parenthesised ship-to name part.
    ShipToRegex,
    /// Trailer estimate per plant (toyota, spec 5.2 rows 31-32).
    Trailers,
    /// ZVT11 Closed by deliveries from `vl06o` (k2xx, spec 5.5 row 55).
    Zvt11Closed,
    /// Merge ZMDETPC into `vl06o` without the xdock/toyota rules (k2xx, spec 4.12 steps 1-4).
    MergeZmdetpcPlain,
    /// Read the delivery list from the k2xx block's `vl06o` TSV of the same run (relabel).
    ReadK2xxVl06o,
    /// ZMDESNR by serial / History (relabel: PL55 pallet 1 serials).
    ZmdesnrHistory,
    /// `filter_copy_snr`: AL55 effective plant, blocks B1-B7, `_of_labels`, `export_date`, pivot.
    FilterCopyRelabel,
    /// Read relabel's `combined` TSV of the same run (repack: yellow rows = exclusion list).
    ReadRelabelCombined,
    /// ZMDESNR by deliveries excluding parents (repack, spec 5.7 row 68).
    ZmdesnrByDeliveriesExcluding,
    /// `filter_copy_repack`: blocks R1-R4, History rows, KC/SC deletion, `export_date`, pivot.
    FilterCopyRepack,
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

/// `toyota` (spec 5.2): the `xdock` flow with VT11 variant `VT11_TOYOTA_VARIANT`, plus a trailers table
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

/// `misc` (spec 5.4): VT11 -> VL06O (layout `inv`) -> dedup + date -> regex on the shipping
/// point -> pivot of `number_of_packages` by `shipping_pointreceiving_pt`. No ZMDETPC.
pub const MISC: ReportDef = ReportDef {
    name: "misc",
    folder: "INV",
    steps: &[Step::Vt11, Step::Vl06o, Step::ShipToRegex, Step::Save],
    output_tables: &["vt11", "vl06o", "pivot"],
};

/// `k2xx` (spec 5.5): VT11 -> VL06O (layout `pallets`) -> ZVT11 Closed -> ZMDETPC -> merge
/// (plain rules) -> pivot plant/total (owner decision of 2026-11-10, overrides spec 7.0 item 1
/// "no pivot"). `relabel` / `repack` read its `vl06o`.
pub const K2XX: ReportDef = ReportDef {
    name: "k2xx",
    folder: "K2XX",
    steps: &[
        Step::Vt11,
        Step::Vl06o,
        Step::Zvt11Closed,
        Step::Zmdetpc,
        Step::MergeZmdetpcPlain,
        Step::Save,
    ],
    output_tables: &["zvt11", "vt11", "vl06o", "pivot"],
};

/// `relabel` (spec 5.6, 9.5): the deliveries come from the k2xx block's `vl06o` TSV of the same
/// run (owner decision), ZMDESNR by deliveries, ZMDESNR History for the PL55 pallet-1 serials,
/// then the blocks / `_of_labels` / pivot customer x plant. `save full_label` (A!62) is left out.
pub const RELABEL: ReportDef = ReportDef {
    name: "relabel",
    folder: "K2XX",
    steps: &[
        Step::ReadK2xxVl06o,
        Step::ZmdesnrByDeliveries,
        Step::ZmdesnrHistory,
        Step::FilterCopyRelabel,
        Step::Save,
    ],
    output_tables: &["combined", "sn_del", "snr_orig", "pivot"],
};

/// `repack` (spec 5.7, 9.5): the deliveries come from the k2xx block's `vl06o` TSV and the
/// exclusion list from the yellow rows of relabel's `combined` TSV, both of the same run. ZMDESNR
/// by deliveries excluding parents, ZMDESNR History for the R2 serials, then the blocks R1-R4,
/// `export_date` and the pivot customer x plant (count of `lm_serial_number`).
pub const REPACK: ReportDef = ReportDef {
    name: "repack",
    folder: "K2XX",
    steps: &[
        Step::ReadK2xxVl06o,
        Step::ReadRelabelCombined,
        Step::ZmdesnrByDeliveriesExcluding,
        Step::ZmdesnrHistory,
        Step::FilterCopyRepack,
        Step::Save,
    ],
    output_tables: &["snr_repack", "repack_sn", "pivot"],
};

const REPORTS: &[ReportDef] = &[MUSTGO, XDOCK, TOYOTA, MISC, K2XX, RELABEL, REPACK];

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

/// Reports that must run (same day) before `rname`, in run order. Unknown rName: none.
pub fn prerequisites(rname: &str) -> &'static [&'static str] {
    match rname.trim().to_ascii_lowercase().as_str() {
        "relabel" => &["k2xx"],
        "repack" => &["k2xx", "relabel"],
        _ => &[],
    }
}

/// Prerequisites in order followed by `rname` itself (canonical registry name when known).
pub fn chain(rname: &str) -> Vec<&'static str> {
    let mut out: Vec<&'static str> = prerequisites(rname).to_vec();
    if let Some(def) = find_report(rname) {
        out.push(def.name);
    }
    out
}

/// Output table names of `rname` (including the pivot). Unknown rName: none.
pub fn tables(rname: &str) -> &'static [&'static str] {
    find_report(rname).map_or(&[], |def| def.output_tables)
}

/// Merged workbook: `<reports_dir>\<folder>\<today yyyy-mm-dd>\<rName>\<label>_<RNAME>_MERGED.xlsx`
/// (`label` = `date_label`, i.e. `WK_<n>` for a week).
pub fn merged_path(
    reports_dir: &Path,
    def: &ReportDef,
    today: NaiveDate,
    run_date: &RunDate,
) -> PathBuf {
    reports_dir
        .join(def.folder)
        .join(today.format("%Y-%m-%d").to_string())
        .join(def.name)
        .join(format!(
            "{}_{}_MERGED.xlsx",
            date_label(run_date),
            def.name.to_ascii_uppercase()
        ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prerequisites_and_chain() {
        assert!(prerequisites("mustgo").is_empty());
        assert!(prerequisites("xdock").is_empty());
        assert_eq!(prerequisites("relabel"), ["k2xx"]);
        assert_eq!(prerequisites("RePack"), ["k2xx", "relabel"]);
        assert_eq!(chain("xdock"), ["xdock"]);
        assert_eq!(chain("k2xx"), ["k2xx"]);
        assert_eq!(chain("relabel"), ["k2xx", "relabel"]);
        assert_eq!(chain("repack"), ["k2xx", "relabel", "repack"]);
        for n in report_names() {
            assert_eq!(*chain(n).last().unwrap(), n);
        }
        assert!(chain("nope").is_empty());
    }

    #[test]
    fn tables_per_report() {
        assert_eq!(tables("mustgo"), ["lt23", "snr_copy", "pivot"]);
        assert_eq!(tables("toyota"), ["vt11", "vl06o", "pivot", "trailers"]);
        assert_eq!(tables("Repack"), ["snr_repack", "repack_sn", "pivot"]);
        assert!(tables("nope").is_empty());
        for n in report_names() {
            assert!(tables(n).contains(&"pivot"));
        }
    }

    #[test]
    fn merged_paths() {
        let week = RunDate {
            date1: d(2026, 11, 1),
            date2: Some(d(2026, 11, 7)),
        };
        let p = merged_path(Path::new("r"), &XDOCK, d(2026, 11, 10), &week);
        assert_eq!(
            p,
            Path::new("r")
                .join("INV")
                .join("2026-11-10")
                .join("xdock")
                .join("WK_45_XDOCK_MERGED.xlsx")
        );
        let p = merged_path(Path::new("r"), &REPACK, d(2026, 11, 10), &week);
        assert!(p.ends_with(Path::new("K2XX/2026-11-10/repack/WK_45_REPACK_MERGED.xlsx")));
    }

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
    fn k2xx_output_path() {
        let rd = RunDate {
            date1: d(2026, 11, 5),
            date2: None,
        };
        let p = output_path(Path::new("r"), &K2XX, d(2026, 11, 10), &rd, "zvt11");
        assert_eq!(
            p,
            Path::new("r")
                .join("K2XX")
                .join("2026-11-10")
                .join("k2xx")
                .join("11-05-2026_K2XX_ZVT11.tsv")
        );
        assert_eq!(
            find_report("K2xx").unwrap().output_tables,
            ["zvt11", "vt11", "vl06o", "pivot"]
        );
        let p = output_path(Path::new("r"), &K2XX, d(2026, 11, 10), &rd, "pivot");
        assert!(p.ends_with("11-05-2026_K2XX_PIVOT.tsv"));
    }

    #[test]
    fn relabel_output_path() {
        let rd = RunDate {
            date1: d(2026, 11, 5),
            date2: None,
        };
        let p = output_path(Path::new("r"), &RELABEL, d(2026, 11, 10), &rd, "combined");
        assert_eq!(
            p,
            Path::new("r")
                .join("K2XX")
                .join("2026-11-10")
                .join("relabel")
                .join("11-05-2026_RELABEL_COMBINED.tsv")
        );
        let r = find_report("Relabel").unwrap();
        assert_eq!(r.folder, "K2XX");
        assert_eq!(r.output_tables, ["combined", "sn_del", "snr_orig", "pivot"]);
    }

    #[test]
    fn repack_output_path() {
        let rd = RunDate {
            date1: d(2026, 11, 5),
            date2: None,
        };
        let p = output_path(Path::new("r"), &REPACK, d(2026, 11, 10), &rd, "snr_repack");
        assert_eq!(
            p,
            Path::new("r")
                .join("K2XX")
                .join("2026-11-10")
                .join("repack")
                .join("11-05-2026_REPACK_SNR_REPACK.tsv")
        );
        let r = find_report("RePack").unwrap();
        assert_eq!(r.folder, "K2XX");
        assert_eq!(r.output_tables, ["snr_repack", "repack_sn", "pivot"]);
    }

    #[test]
    fn misc_output_path() {
        let rd = RunDate {
            date1: d(2026, 11, 5),
            date2: None,
        };
        let p = output_path(Path::new("r"), &MISC, d(2026, 11, 10), &rd, "pivot");
        assert_eq!(
            p,
            Path::new("r")
                .join("INV")
                .join("2026-11-10")
                .join("misc")
                .join("11-05-2026_MISC_PIVOT.tsv")
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
