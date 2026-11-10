//! Pure post-processing stages of the `INV` reports `xdock`, `toyota` and `misc`
//! (spec 4.12, 5.2-5.4, verified in 9.6 against the xdock run of 2026-11-05).
//!
//! Same rules as `mustgo.rs`: tables with sanitised headers in, tables out, columns matched by
//! name, no SAP access. The calls that feed these stages (VT11, VL06O, ZMDETPC) live in the
//! runner seam.
//!
//! Decisions (spec 7.0 / 9.6):
//! * **Q36:** the VL06O dedup keeps the *first* row of a delivery (what the macro does), not a
//!   row that happens to have a plant.
//! * **Item 3:** the macro's `Columns.Replace "0","1"` is applied as "a `total` cell that is
//!   exactly `0` becomes `1`" (the substring reading, `10` -> `11`, is taken to be a bug).
//! * **Pivot:** long form, sorted ascending by label, a blank label is `(blank)` and sorts last
//!   (as Excel shows it), no grand-total row (the same shape as the `mustgo` pivot).
//! * **Trailers (toyota):** a separate `trailers` table (`plant`, `trailers` = `ROUND(total /
//!   14.5)`) instead of cells next to the pivot.
#![allow(dead_code)]

use super::ops;
use super::table::{ColPos, Table};
use anyhow::{anyhow, Result};
use regex::Regex;
use std::cmp::Ordering;
use std::collections::BTreeMap;

/// Pallets per trailer used for the toyota estimate (spec 5.2 row 32).
pub const TRAILER_DIVISOR: f64 = 14.5;

/// Label of a blank group in the pivot.
pub const BLANK_LABEL: &str = "(blank)";

/// Delivery key for joining: trimmed, leading zeros dropped (`0085614256` = `85614256`;
/// `8012734818` is left as it is).
pub fn delivery_key(raw: &str) -> String {
    let t = raw.trim();
    let s = t.trim_start_matches('0');
    if s.is_empty() && !t.is_empty() {
        "0".to_string()
    } else {
        s.to_string()
    }
}

fn key_cmp(a: &str, b: &str) -> Ordering {
    let digits = |s: &str| !s.is_empty() && s.chars().all(|c| c.is_ascii_digit());
    if digits(a) && digits(b) {
        a.len().cmp(&b.len()).then_with(|| a.cmp(b))
    } else {
        a.cmp(b)
    }
}

fn parse_num(s: &str) -> Option<f64> {
    let t = s.trim().replace(',', "");
    if t.is_empty() {
        None
    } else {
        t.parse::<f64>().ok()
    }
}

fn fmt_num(v: f64) -> String {
    if v.fract() == 0.0 && v.abs() < 1e15 {
        format!("{}", v as i64)
    } else {
        let s = format!("{v:.6}");
        s.trim_end_matches('0').trim_end_matches('.').to_string()
    }
}

/// VL06O prepare (spec 5.2 row 27 and the dedup of 9.6): one row per delivery (first kept), a
/// `date` column holding `date_text` on every row (added at the end).
pub fn prepare_vl06o(vl06o: &mut Table, date_text: &str) -> Result<()> {
    vl06o.reset_selection();
    ops::dedup(vl06o, &["delivery"], false)?;
    ops::add_col_value(vl06o, "date", date_text, false)
}

/// Distinct, non-blank delivery numbers for the ZMDETPC call (order of the table).
pub fn delivery_values(vl06o: &Table) -> Result<Vec<String>> {
    let c = vl06o.col("delivery")?;
    let mut seen = std::collections::HashSet::new();
    let mut out = Vec::new();
    for r in &vl06o.rows {
        let v = r[c].trim();
        if !v.is_empty() && seen.insert(delivery_key(v)) {
            out.push(v.to_string());
        }
    }
    if out.is_empty() {
        return Err(anyhow!("vl06o: no deliveries"));
    }
    Ok(out)
}

/// Spec 4.12 `merge ZMDETPC into vl06o` with the xdock/toyota rules (step 5).
///
/// Outer join on the delivery number, ordered ascending by it; the result has the `vl06o`
/// columns followed by `delivery_`, `pallet`, `child`, `total`:
/// * `total = pallet + child` for a row that has a ZMDETPC row, else blank, then blank `total`
///   takes `number_of_packages`;
/// * blank `delivery_` takes `delivery`;
/// * `total` exactly `0` becomes `1`.
///
/// `vl06o` must already be one row per delivery ([`prepare_vl06o`]); a delivery repeated in
/// `zmdetpc` keeps its first row.
pub fn merge_zmdetpc(vl06o: &Table, zmdetpc: &Table) -> Result<Table> {
    let vd = vl06o.col("delivery")?;
    let vn = vl06o.col("number_of_packages")?;
    let (zd, zp, zc) = (
        zmdetpc.col("delivery")?,
        zmdetpc.col("pallet")?,
        zmdetpc.col("child")?,
    );
    let mut z: BTreeMap<String, &Vec<String>> = BTreeMap::new();
    for r in &zmdetpc.rows {
        let k = delivery_key(&r[zd]);
        if !k.is_empty() {
            z.entry(k).or_insert(r);
        }
    }
    let width = vl06o.headers.len();
    // sortable entries: (key, vl06o row index or None, zmdetpc key)
    let mut entries: Vec<(String, Option<usize>)> = Vec::new();
    let mut used = std::collections::HashSet::new();
    for (i, r) in vl06o.rows.iter().enumerate() {
        let k = delivery_key(&r[vd]);
        if !k.is_empty() {
            used.insert(k.clone());
        }
        entries.push((k, Some(i)));
    }
    for k in z.keys() {
        if !used.contains(k) {
            entries.push((k.clone(), None));
        }
    }
    entries.sort_by(|a, b| key_cmp(&a.0, &b.0));

    let mut headers = vl06o.headers.clone();
    headers.extend(["delivery_", "pallet", "child", "total"].map(String::from));
    let mut rows = Vec::with_capacity(entries.len());
    for (key, vi) in entries {
        let mut row: Vec<String> = match vi {
            Some(i) => vl06o.rows[i].clone(),
            None => vec![String::new(); width],
        };
        let (mut delivery_, mut pallet, mut child, mut total) =
            (String::new(), String::new(), String::new(), String::new());
        if let Some(zr) = z.get(&key) {
            delivery_ = zr[zd].trim().to_string();
            pallet = zr[zp].trim().to_string();
            child = zr[zc].trim().to_string();
            total = fmt_num(parse_num(&pallet).unwrap_or(0.0) + parse_num(&child).unwrap_or(0.0));
        }
        if total.is_empty() {
            total = row[vn].trim().to_string();
        }
        if delivery_.is_empty() {
            delivery_ = row[vd].trim().to_string();
        }
        if parse_num(&total) == Some(0.0) {
            total = "1".to_string();
        }
        row.extend([delivery_, pallet, child, total]);
        rows.push(row);
    }
    Ok(Table::new("vl06o", headers, rows))
}

/// Pivot as a long table: `group_col`, `sum_col` (sum of the numeric cells; blank or text
/// cells add nothing). Sorted ascending (case-insensitive) with `(blank)` last.
pub fn pivot_sum(t: &Table, group_col: &str, sum_col: &str) -> Result<Table> {
    let (g, s) = (t.col(group_col)?, t.col(sum_col)?);
    let mut sums: BTreeMap<(bool, String), (String, f64)> = BTreeMap::new();
    for r in &t.rows {
        let label = r[g].trim();
        let (blank, label) = if label.is_empty() {
            (true, BLANK_LABEL.to_string())
        } else {
            (false, label.to_string())
        };
        let e = sums
            .entry((blank, label.to_lowercase()))
            .or_insert((label, 0.0));
        e.1 += parse_num(&r[s]).unwrap_or(0.0);
    }
    let rows = sums
        .into_values()
        .map(|(label, v)| vec![label, fmt_num(v)])
        .collect();
    Ok(Table::new(
        "pivot",
        vec![group_col.to_string(), sum_col.to_string()],
        rows,
    ))
}

/// Toyota trailer estimate (spec 5.2 rows 31-32): per pivot row `ROUND(sum / 14.5)` (halves
/// away from zero, like Excel). Columns `plant`, `trailers`.
pub fn trailers(pivot: &Table) -> Result<Table> {
    let (p, s) = (pivot.col("plant")?, pivot.col("total")?);
    let rows = pivot
        .rows
        .iter()
        .map(|r| {
            let v = parse_num(&r[s]).unwrap_or(0.0);
            vec![r[p].clone(), fmt_num((v / TRAILER_DIVISOR).round())]
        })
        .collect();
    Ok(Table::new(
        "trailers",
        vec!["plant".to_string(), "trailers".to_string()],
        rows,
    ))
}

/// `misc` regex step (spec 5.4 row 47): for rows whose `shipping_pointreceiving_pt` contains
/// `EPDC`, replace it with the first `(...)` group (parentheses included) of
/// `name_of_the_shipto_party`; rows without such a group are left alone.
pub fn misc_ship_to_regex(vl06o: &mut Table) -> Result<()> {
    let (n, p) = (
        vl06o.col("name_of_the_shipto_party")?,
        vl06o.col("shipping_pointreceiving_pt")?,
    );
    let re = Regex::new(r"\((.*?)\)").expect("static regex");
    for r in &mut vl06o.rows {
        if !r[p].contains("EPDC") {
            continue;
        }
        if let Some(m) = re.find(&r[n]) {
            r[p] = m.as_str().to_string();
        }
    }
    Ok(())
}

/// Position helper kept for symmetry with `mustgo` (a column inserted last).
pub fn add_last(t: &mut Table, name: &str, fill: Vec<String>) -> Result<usize> {
    t.insert_column(name, &ColPos::Last, fill)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn t(name: &str, headers: &[&str], rows: &[&[&str]]) -> Table {
        Table::new(
            name,
            headers.iter().map(|s| s.to_string()).collect(),
            rows.iter()
                .map(|r| r.iter().map(|s| s.to_string()).collect())
                .collect(),
        )
    }

    fn vl() -> Table {
        t(
            "vl06o",
            &["delivery", "plant", "number_of_packages"],
            &[
                &["300", "FV58", "4"],
                &["300", "", "4"],
                &["100", "FV55", "2"],
                &["200", "", "7"],
                &["400", "FW61", "9"],
            ],
        )
    }

    #[test]
    fn keys() {
        assert_eq!(delivery_key("0085614256"), "85614256");
        assert_eq!(delivery_key("8012734818"), "8012734818");
        assert_eq!(delivery_key("000"), "0");
        assert_eq!(delivery_key(""), "");
        assert_eq!(key_cmp("99", "100"), Ordering::Less, "numeric, not text");
    }

    #[test]
    fn prepare_keeps_first_row_and_adds_date() {
        let mut v = vl();
        prepare_vl06o(&mut v, "2026-11-05").unwrap();
        assert_eq!(v.nrows(), 4);
        assert_eq!(v.rows[0], vec!["300", "FV58", "4", "2026-11-05"]);
        assert_eq!(v.headers.last().unwrap(), "date");
        assert_eq!(
            delivery_values(&v).unwrap(),
            vec!["300", "100", "200", "400"]
        );
    }

    #[test]
    fn merge_sorts_sums_and_fills() {
        let mut v = vl();
        prepare_vl06o(&mut v, "d").unwrap();
        let z = t(
            "zmdetpc",
            &["delivery", "pallet", "child"],
            &[
                &["0000000300", "2", "3"],
                &["100", "0", "0"],
                &["200", "1", ""],
            ],
        );
        let m = merge_zmdetpc(&v, &z).unwrap();
        assert_eq!(
            m.headers,
            vec![
                "delivery",
                "plant",
                "number_of_packages",
                "date",
                "delivery_",
                "pallet",
                "child",
                "total"
            ]
        );
        let d: Vec<&str> = m.rows.iter().map(|r| r[0].as_str()).collect();
        assert_eq!(d, vec!["100", "200", "300", "400"], "ascending by delivery");
        // 100: pallet+child = 0 -> 1 (item 3)
        assert_eq!(m.rows[0][4..], ["100", "0", "0", "1"]);
        // 200: 1 + blank
        assert_eq!(m.rows[1][7], "1");
        // 300: zero padded ZMDETPC delivery, 2 + 3
        assert_eq!(m.rows[2][4..], ["0000000300", "2", "3", "5"]);
        // 400: no ZMDETPC row -> delivery_ = delivery, total = number_of_packages
        assert_eq!(m.rows[3][4..], ["400", "", "", "9"]);
    }

    #[test]
    fn zero_replace_is_exact_not_substring() {
        let mut v = t("v", &["delivery", "number_of_packages"], &[&["1", "10"]]);
        prepare_vl06o(&mut v, "d").unwrap();
        let z = t("z", &["delivery", "pallet", "child"], &[&["1", "10", "0"]]);
        let m = merge_zmdetpc(&v, &z).unwrap();
        assert_eq!(m.rows[0].last().unwrap(), "10");
    }

    #[test]
    fn merge_outer_join_keeps_zmdetpc_only_rows() {
        let v = t("v", &["delivery", "number_of_packages"], &[&["2", "5"]]);
        let z = t("z", &["delivery", "pallet", "child"], &[&["1", "1", "1"]]);
        let m = merge_zmdetpc(&v, &z).unwrap();
        assert_eq!(m.nrows(), 2);
        assert_eq!(m.rows[0], vec!["", "", "1", "1", "1", "2"]);
        assert_eq!(m.rows[1], vec!["2", "5", "2", "", "", "5"]);
    }

    #[test]
    fn merge_requires_columns() {
        let v = t("v", &["delivery"], &[&["1"]]);
        let z = t("z", &["delivery", "pallet", "child"], &[]);
        assert!(merge_zmdetpc(&v, &z).is_err());
    }

    #[test]
    fn pivot_sorts_and_puts_blank_last() {
        let m = t(
            "m",
            &["plant", "total"],
            &[
                &["FW61", "2"],
                &["", "3"],
                &["FV55", "10"],
                &["FV55", "1,200"],
                &["fv55", "x"],
            ],
        );
        let p = pivot_sum(&m, "plant", "total").unwrap();
        assert_eq!(p.headers, vec!["plant", "total"]);
        assert_eq!(
            p.rows,
            vec![
                vec!["FV55", "1210"],
                vec!["FW61", "2"],
                vec!["(blank)", "3"]
            ]
        );
    }

    #[test]
    fn trailers_round_half_up() {
        let p = t(
            "pivot",
            &["plant", "total"],
            &[
                &["FV55", "29"],
                &["FV56", "14"],
                &["FV58", "7"],
                &["(blank)", "0"],
            ],
        );
        let tr = trailers(&p).unwrap();
        assert_eq!(
            tr.rows,
            vec![
                vec!["FV55", "2"],
                vec!["FV56", "1"],
                vec!["FV58", "0"],
                vec!["(blank)", "0"]
            ]
        );
        // 7.25 = 14.5 / 2 rounds away from zero, like Excel ROUND
        let h = t("pivot", &["plant", "total"], &[&["X", "7.25"]]);
        assert_eq!(trailers(&h).unwrap().rows[0][1], "1");
    }

    #[test]
    fn misc_regex_replaces_only_epdc_rows() {
        let mut v = t(
            "vl06o",
            &["name_of_the_shipto_party", "shipping_pointreceiving_pt"],
            &[
                &["ACME (EP-1) plant (x)", "EPDC"],
                &["No parens", "EPDC"],
                &["ACME (EP-2)", "E01"],
            ],
        );
        misc_ship_to_regex(&mut v).unwrap();
        assert_eq!(v.rows[0][1], "(EP-1)");
        assert_eq!(v.rows[1][1], "EPDC");
        assert_eq!(v.rows[2][1], "E01");
    }
}
