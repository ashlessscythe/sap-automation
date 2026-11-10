//! Table operations (spec 4.1-4.11): filter with Excel criteria, dedup, lookup, add column,
//! sort, remove-one, count-same, datetime. Pure functions over [`Table`].
#![allow(dead_code)]

use super::table::{ColPos, Table};
use anyhow::Result;
use chrono::NaiveDate;
use std::cmp::Ordering;
use std::collections::{HashMap, HashSet};

// ---------- values & criteria ----------

fn parse_num(s: &str) -> Option<f64> {
    let t = s.trim();
    if t.is_empty() {
        return None;
    }
    t.parse::<f64>().ok().filter(|f| f.is_finite())
}

fn parse_date(s: &str) -> Option<NaiveDate> {
    NaiveDate::parse_from_str(s.trim(), "%Y-%m-%d").ok()
}

/// Compare two values numerically, else as ISO dates; `None` if neither applies.
pub fn cmp_values(a: &str, b: &str) -> Option<Ordering> {
    if let (Some(x), Some(y)) = (parse_num(a), parse_num(b)) {
        return x.partial_cmp(&y);
    }
    if let (Some(x), Some(y)) = (parse_date(a), parse_date(b)) {
        return Some(x.cmp(&y));
    }
    None
}

/// Excel-style wildcard match (`*`, `?`), case-insensitive, whole string.
pub fn wildcard_match(pattern: &str, text: &str) -> bool {
    let p: Vec<char> = pattern.to_lowercase().chars().collect();
    let t: Vec<char> = text.to_lowercase().chars().collect();
    let (mut pi, mut ti) = (0usize, 0usize);
    let (mut star, mut mark) = (None::<usize>, 0usize);
    while ti < t.len() {
        if pi < p.len() && (p[pi] == '?' || p[pi] == t[ti]) && p[pi] != '*' {
            pi += 1;
            ti += 1;
        } else if pi < p.len() && p[pi] == '*' {
            star = Some(pi);
            mark = ti;
            pi += 1;
        } else if let Some(s) = star {
            pi = s + 1;
            mark += 1;
            ti = mark;
        } else {
            return false;
        }
    }
    while pi < p.len() && p[pi] == '*' {
        pi += 1;
    }
    pi == p.len()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CmpOp {
    Gt,
    Lt,
    Ge,
    Le,
}

/// One Excel filter criterion.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Crit {
    Blank,
    NonBlank,
    Eq(String),
    Ne(String),
    Cmp(CmpOp, String),
}

impl Crit {
    /// `=` blank, `<>` non-blank, `<>x` not equal, `>n` `<n` `>=n` `<=n`, `=x` or `x` equal.
    pub fn parse(raw: &str) -> Crit {
        let s = raw.trim();
        if s == "=" || s.is_empty() {
            return Crit::Blank;
        }
        if s == "<>" {
            return Crit::NonBlank;
        }
        if let Some(r) = s.strip_prefix("<>") {
            return Crit::Ne(r.trim().to_string());
        }
        for (p, op) in [
            (">=", CmpOp::Ge),
            ("<=", CmpOp::Le),
            (">", CmpOp::Gt),
            ("<", CmpOp::Lt),
        ] {
            if let Some(r) = s.strip_prefix(p) {
                return Crit::Cmp(op, r.trim().to_string());
            }
        }
        Crit::Eq(s.strip_prefix('=').unwrap_or(s).trim().to_string())
    }

    fn eq_match(want: &str, cell: &str) -> bool {
        if let (Some(a), Some(b)) = (parse_num(want), parse_num(cell)) {
            return a == b;
        }
        wildcard_match(want, cell.trim())
    }

    pub fn matches(&self, cell: &str) -> bool {
        let c = cell.trim();
        match self {
            Crit::Blank => c.is_empty(),
            Crit::NonBlank => !c.is_empty(),
            Crit::Eq(w) => Self::eq_match(w, c),
            Crit::Ne(w) => !Self::eq_match(w, c),
            Crit::Cmp(op, w) => match cmp_values(c, w) {
                Some(o) => match op {
                    CmpOp::Gt => o == Ordering::Greater,
                    CmpOp::Lt => o == Ordering::Less,
                    CmpOp::Ge => o != Ordering::Less,
                    CmpOp::Le => o != Ordering::Greater,
                },
                None => false,
            },
        }
    }
}

// ---------- filter ----------

#[derive(Debug, Clone, Copy, Default)]
pub struct FilterOpts {
    /// AND with the existing selection (otherwise it is cleared first).
    pub keep_prev: bool,
    /// Remove non-matching rows instead of hiding them.
    pub delete: bool,
}

fn apply_selection(t: &mut Table, matched: Vec<bool>, opts: FilterOpts) {
    for (i, m) in matched.into_iter().enumerate() {
        t.visible[i] = m && (!opts.keep_prev || t.visible[i]);
    }
    if opts.delete {
        t.materialize();
    }
}

/// `Sel(T, col matches crit)` (spec 4.1).
pub fn filter(t: &mut Table, col: &str, crit: &str, opts: FilterOpts) -> Result<()> {
    let c = t.col(col)?;
    let crit = Crit::parse(crit);
    if !opts.keep_prev {
        t.reset_selection();
    }
    let matched = t.rows.iter().map(|r| crit.matches(&r[c])).collect();
    apply_selection(t, matched, opts);
    Ok(())
}

/// Advanced filter (spec 4.2): criteria in one group are ANDed, groups are ORed.
pub fn adv_filter(t: &mut Table, groups: &[Vec<(String, String)>], opts: FilterOpts) -> Result<()> {
    let mut compiled: Vec<Vec<(usize, Crit)>> = Vec::new();
    for g in groups {
        let mut cg = Vec::new();
        for (col, crit) in g {
            cg.push((t.col(col)?, Crit::parse(crit)));
        }
        compiled.push(cg);
    }
    if !opts.keep_prev {
        t.reset_selection();
    }
    let matched = t
        .rows
        .iter()
        .map(|r| {
            compiled
                .iter()
                .any(|g| g.iter().all(|(c, crit)| crit.matches(&r[*c])))
        })
        .collect();
    apply_selection(t, matched, opts);
    Ok(())
}

// ---------- dedup ----------

/// Remove rows whose key was already seen (first kept, case-insensitive). Empty `cols` = whole
/// row. With `visible_only`, only visible rows take part; hidden rows are left untouched.
pub fn dedup(t: &mut Table, cols: &[&str], visible_only: bool) -> Result<()> {
    let idx: Vec<usize> = if cols.is_empty() {
        (0..t.headers.len()).collect()
    } else {
        cols.iter().map(|c| t.col(c)).collect::<Result<_>>()?
    };
    let mut seen = HashSet::new();
    let mut keep = Vec::with_capacity(t.rows.len());
    for (i, r) in t.rows.iter().enumerate() {
        if visible_only && !t.visible[i] {
            keep.push(true);
            continue;
        }
        let key: Vec<String> = idx.iter().map(|&c| r[c].trim().to_lowercase()).collect();
        keep.push(seen.insert(key.join("\u{1f}")));
    }
    retain_rows(t, &keep);
    Ok(())
}

fn retain_rows(t: &mut Table, keep: &[bool]) {
    let mut i = 0;
    t.rows.retain(|_| {
        let k = keep[i];
        i += 1;
        k
    });
    let mut j = 0;
    t.visible.retain(|_| {
        let k = keep[j];
        j += 1;
        k
    });
}

// ---------- lookup ----------

pub struct LookupSpec<'a> {
    pub src_key: &'a str,
    pub src_val: &'a str,
    pub dst_col: &'a str,
    pub new_col: &'a str,
    pub pos: ColPos,
    /// Written when not found (spec default `-`).
    pub not_found: &'a str,
    /// Written instead of the source value when found.
    pub found_value: Option<&'a str>,
    /// Look only at visible source rows.
    pub skip_hidden: bool,
}

/// Insert `new_col` into `dst`: for each row, the first `src_val` whose `src_key` equals the
/// row's `dst_col` (case-insensitive), else `not_found` (spec 4.4).
pub fn lookup(dst: &mut Table, src: &Table, spec: &LookupSpec) -> Result<()> {
    let k = src.col(spec.src_key)?;
    let v = src.col(spec.src_val)?;
    let l = dst.col(spec.dst_col)?;
    let mut map: HashMap<String, &str> = HashMap::new();
    for (i, r) in src.rows.iter().enumerate() {
        if spec.skip_hidden && !src.visible[i] {
            continue;
        }
        map.entry(r[k].trim().to_lowercase()).or_insert(&r[v]);
    }
    let fill = dst
        .rows
        .iter()
        .map(|r| match map.get(&r[l].trim().to_lowercase()) {
            Some(val) => spec.found_value.unwrap_or(val).to_string(),
            None => spec.not_found.to_string(),
        })
        .collect();
    dst.insert_column(spec.new_col, &spec.pos, fill)?;
    Ok(())
}

// ---------- column ops ----------

/// Set `col` to `value` on all rows (or visible rows); the column is created at the end if
/// missing (spec 4.5).
pub fn add_col_value(t: &mut Table, col: &str, value: &str, visible_only: bool) -> Result<()> {
    let c = match t.col(col) {
        Ok(c) => c,
        Err(_) => t.insert_column(col, &ColPos::Last, vec![String::new(); t.rows.len()])?,
    };
    for i in 0..t.rows.len() {
        if !visible_only || t.visible[i] {
            t.rows[i][c] = value.to_string();
        }
    }
    Ok(())
}

/// New column = `date + " " + time` (blank if either is blank) (spec 4.11 `morph_datetime`).
pub fn morph_datetime(
    t: &mut Table,
    date_col: &str,
    time_col: &str,
    new_col: &str,
    pos: &ColPos,
) -> Result<()> {
    let (d, tm) = (t.col(date_col)?, t.col(time_col)?);
    let fill = t
        .rows
        .iter()
        .map(|r| {
            if r[d].trim().is_empty() || r[tm].trim().is_empty() {
                String::new()
            } else {
                format!("{} {}", r[d].trim(), r[tm].trim())
            }
        })
        .collect();
    t.insert_column(new_col, pos, fill)?;
    Ok(())
}

/// New column = number of rows sharing the row's `key_col` value if more than one, else
/// `unique_label` (the `bDup` column of the `mustgo` post-process).
pub fn count_same(
    t: &mut Table,
    key_col: &str,
    new_col: &str,
    pos: &ColPos,
    unique_label: &str,
) -> Result<()> {
    let k = t.col(key_col)?;
    let mut counts: HashMap<String, usize> = HashMap::new();
    for r in &t.rows {
        *counts.entry(r[k].trim().to_lowercase()).or_default() += 1;
    }
    let fill = t
        .rows
        .iter()
        .map(|r| match counts[&r[k].trim().to_lowercase()] {
            n if n > 1 => n.to_string(),
            _ => unique_label.to_string(),
        })
        .collect();
    t.insert_column(new_col, pos, fill)?;
    Ok(())
}

// ---------- sort / remove_one / copy ----------

/// Multi-key stable sort. Numbers/dates compare by value, other text case-insensitively;
/// blanks always last. The visibility mask moves with the rows.
pub fn sort_by(t: &mut Table, keys: &[(&str, bool)]) -> Result<()> {
    let idx: Vec<(usize, bool)> = keys
        .iter()
        .map(|(c, desc)| Ok((t.col(c)?, *desc)))
        .collect::<Result<_>>()?;
    let mut order: Vec<usize> = (0..t.rows.len()).collect();
    order.sort_by(|&a, &b| {
        for &(c, desc) in &idx {
            let (x, y) = (t.rows[a][c].trim(), t.rows[b][c].trim());
            let o = match (x.is_empty(), y.is_empty()) {
                (true, true) => Ordering::Equal,
                (true, false) => return Ordering::Greater,
                (false, true) => return Ordering::Less,
                _ => {
                    let o =
                        cmp_values(x, y).unwrap_or_else(|| x.to_lowercase().cmp(&y.to_lowercase()));
                    if desc {
                        o.reverse()
                    } else {
                        o
                    }
                }
            };
            if o != Ordering::Equal {
                return o;
            }
        }
        Ordering::Equal
    });
    t.rows = order.iter().map(|&i| t.rows[i].clone()).collect();
    t.visible = order.iter().map(|&i| t.visible[i]).collect();
    Ok(())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Which {
    First,
    Last,
}

/// Remove exactly one occurrence per distinct `col` value (the fixed intent of `remove_one`,
/// spec 4.9 / item 9), in the table's current order. `mustgo` sorts first and removes the
/// `Last` row of each delivery (the earliest TO).
pub fn remove_one(t: &mut Table, col: &str, which: Which) -> Result<()> {
    let c = t.col(col)?;
    let mut keep = vec![true; t.rows.len()];
    let mut chosen: HashMap<String, usize> = HashMap::new();
    for (i, r) in t.rows.iter().enumerate() {
        let key = r[c].trim().to_lowercase();
        match which {
            Which::First => {
                chosen.entry(key).or_insert(i);
            }
            Which::Last => {
                chosen.insert(key, i);
            }
        }
    }
    for i in chosen.into_values() {
        keep[i] = false;
    }
    retain_rows(t, &keep);
    Ok(())
}

/// Duplicate a table under a new name, selection included.
pub fn copy_table(t: &Table, name: &str) -> Table {
    let mut c = t.clone();
    c.name = name.to_string();
    c
}

#[cfg(test)]
mod tests {
    use super::*;

    fn s(v: &[&str]) -> Vec<String> {
        v.iter().map(|x| x.to_string()).collect()
    }

    fn tbl(headers: &[&str], rows: &[&[&str]]) -> Table {
        Table::new("t", s(headers), rows.iter().map(|r| s(r)).collect())
    }

    fn col(t: &Table, c: &str) -> Vec<String> {
        let i = t.col(c).unwrap();
        t.rows.iter().map(|r| r[i].clone()).collect()
    }

    #[test]
    fn wildcards() {
        assert!(wildcard_match("KC*", "kc123"));
        assert!(!wildcard_match("KC*", "xkc"));
        assert!(wildcard_match("a?c", "ABC"));
        assert!(wildcard_match("*b*", "abc"));
        assert!(wildcard_match("*", ""));
        assert!(!wildcard_match("a", "ab"));
    }

    #[test]
    fn criteria() {
        assert!(Crit::parse("=").matches("  "));
        assert!(!Crit::parse("=").matches("x"));
        assert!(Crit::parse("<>").matches("x"));
        assert!(Crit::parse("pal").matches("PAL"));
        assert!(Crit::parse("=pal").matches("PAL"));
        assert!(Crit::parse("<>PAL").matches("SHP"));
        assert!(Crit::parse("<>PAL").matches(""));
        assert!(!Crit::parse("<>KC*").matches("KC1"));
        assert!(Crit::parse(">=10").matches("10"));
        assert!(Crit::parse(">10").matches("11.5"));
        assert!(!Crit::parse("<5").matches("abc"));
        assert!(Crit::parse("5").matches("5.0"));
        assert!(Crit::parse(">=2026-11-01").matches("2026-11-05"));
        assert!(!Crit::parse("<2026-11-01").matches("2026-11-05"));
    }

    #[test]
    fn filter_hides_then_and_then_delete() {
        let mut t = tbl(
            &["parent", "status"],
            &[&["p1", "SHP"], &["", "SHP"], &["p2", "PAL"], &["p3", "SHP"]],
        );
        filter(&mut t, "parent", "<>", FilterOpts::default()).unwrap();
        assert_eq!(t.visible, vec![true, false, true, true]);
        let keep = FilterOpts {
            keep_prev: true,
            ..Default::default()
        };
        filter(&mut t, "status", "<>PAL", keep).unwrap();
        assert_eq!(t.visible, vec![true, false, false, true]);
        assert_eq!(t.nrows(), 4);
        // without keep_prev the selection restarts
        filter(&mut t, "status", "PAL", FilterOpts::default()).unwrap();
        assert_eq!(t.visible, vec![false, false, true, false]);
        filter(
            &mut t,
            "status",
            "SHP",
            FilterOpts {
                delete: true,
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(col(&t, "parent"), s(&["p1", "", "p3"]));
        assert_eq!(t.visible_count(), 3);
    }

    #[test]
    fn adv_filter_and_or() {
        let mut t = tbl(
            &["a", "b"],
            &[&["1", "x"], &["1", "y"], &["2", "x"], &["3", "z"]],
        );
        let groups = vec![
            vec![
                ("a".to_string(), "1".to_string()),
                ("b".to_string(), "x".to_string()),
            ],
            vec![("a".to_string(), "3".to_string())],
        ];
        adv_filter(&mut t, &groups, FilterOpts::default()).unwrap();
        assert_eq!(t.visible, vec![true, false, false, true]);
    }

    #[test]
    fn dedup_keeps_first_case_insensitive() {
        let mut t = tbl(&["p", "v"], &[&["A", "1"], &["a", "2"], &["B", "3"]]);
        dedup(&mut t, &["p"], false).unwrap();
        assert_eq!(col(&t, "v"), s(&["1", "3"]));
    }

    #[test]
    fn dedup_whole_row() {
        let mut t = tbl(&["p", "v"], &[&["A", "1"], &["a", "1"], &["A", "2"]]);
        dedup(&mut t, &[], false).unwrap();
        assert_eq!(t.nrows(), 2);
    }

    #[test]
    fn dedup_visible_only_leaves_hidden_alone() {
        let mut t = tbl(&["p"], &[&["a"], &["a"], &["a"], &["b"]]);
        t.visible = vec![true, false, true, true];
        dedup(&mut t, &["p"], true).unwrap();
        // visible dup at index 2 removed; hidden duplicate at index 1 untouched
        assert_eq!(col(&t, "p"), s(&["a", "a", "b"]));
        assert_eq!(t.visible, vec![true, false, true]);
    }

    #[test]
    fn lookup_found_notfound_and_found_value() {
        let src = tbl(&["to", "val"], &[&["1", "a"], &["1", "b"], &["2", "c"]]);
        let mut dst = tbl(&["to", "x"], &[&["1", "-"], &["3", "-"], &["2", "-"]]);
        lookup(
            &mut dst,
            &src,
            &LookupSpec {
                src_key: "to",
                src_val: "val",
                dst_col: "to",
                new_col: "got",
                pos: ColPos::Last,
                not_found: "x",
                found_value: None,
                skip_hidden: false,
            },
        )
        .unwrap();
        assert_eq!(col(&dst, "got"), s(&["a", "x", "c"]));

        let mut dst2 = tbl(&["to"], &[&["1"], &["9"]]);
        lookup(
            &mut dst2,
            &src,
            &LookupSpec {
                src_key: "to",
                src_val: "val",
                dst_col: "to",
                new_col: "count",
                pos: ColPos::Last,
                not_found: "x",
                found_value: Some("1"),
                skip_hidden: false,
            },
        )
        .unwrap();
        assert_eq!(col(&dst2, "count"), s(&["1", "x"]));
    }

    #[test]
    fn lookup_skip_hidden() {
        let mut src = tbl(&["k", "v"], &[&["1", "a"], &["1", "b"]]);
        src.visible[0] = false;
        let mut dst = tbl(&["k"], &[&["1"]]);
        lookup(
            &mut dst,
            &src,
            &LookupSpec {
                src_key: "k",
                src_val: "v",
                dst_col: "k",
                new_col: "n",
                pos: ColPos::Last,
                not_found: "-",
                found_value: None,
                skip_hidden: true,
            },
        )
        .unwrap();
        assert_eq!(col(&dst, "n"), s(&["b"]));
    }

    #[test]
    fn add_col_value_creates_and_respects_visibility() {
        let mut t = tbl(&["a"], &[&["1"], &["2"]]);
        t.visible[1] = false;
        add_col_value(&mut t, "export_date", "2026-11-05", true).unwrap();
        assert_eq!(col(&t, "export_date"), s(&["2026-11-05", ""]));
        add_col_value(&mut t, "export_date", "z", false).unwrap();
        assert_eq!(col(&t, "export_date"), s(&["z", "z"]));
    }

    #[test]
    fn datetime_and_count_same() {
        let mut t = tbl(
            &["delivery", "creation_date", "creation_time"],
            &[
                &["d1", "2026-11-05", "08:00:00"],
                &["d1", "2026-11-05", "09:00:00"],
                &["d2", "2026-11-05", ""],
            ],
        );
        morph_datetime(
            &mut t,
            "creation_date",
            "creation_time",
            "datetime",
            &ColPos::After("creation_time".into()),
        )
        .unwrap();
        assert_eq!(
            col(&t, "datetime"),
            s(&["2026-11-05 08:00:00", "2026-11-05 09:00:00", ""])
        );
        count_same(&mut t, "delivery", "bdup", &ColPos::Last, "unique").unwrap();
        assert_eq!(col(&t, "bdup"), s(&["2", "2", "unique"]));
    }

    #[test]
    fn sort_multi_key_numeric_desc_blank_last() {
        let mut t = tbl(
            &["d", "n"],
            &[&["10", "a"], &["9", "b"], &["", "c"], &["10", "z"]],
        );
        t.visible[1] = false;
        sort_by(&mut t, &[("d", true), ("n", true)]).unwrap();
        assert_eq!(col(&t, "n"), s(&["z", "a", "b", "c"]));
        // visibility moved with the row for "b"
        assert_eq!(t.visible, vec![true, true, false, true]);
    }

    #[test]
    fn mustgo_post_process_shape() {
        // 3 deliveries: d1 has 3 TOs, d2 has 2, d3 has 1 (unique -> removed entirely).
        let mut t = tbl(
            &["delivery", "to", "creation_date", "creation_time"],
            &[
                &["d1", "t1", "2026-11-05", "08:00:00"],
                &["d1", "t2", "2026-11-05", "09:00:00"],
                &["d1", "t3", "2026-11-05", "10:00:00"],
                &["d2", "t4", "2026-11-05", "07:00:00"],
                &["d2", "t5", "2026-11-05", "11:00:00"],
                &["d3", "t6", "2026-11-05", "12:00:00"],
            ],
        );
        dedup(&mut t, &["to"], false).unwrap();
        morph_datetime(
            &mut t,
            "creation_date",
            "creation_time",
            "datetime",
            &ColPos::After("creation_time".into()),
        )
        .unwrap();
        sort_by(&mut t, &[("delivery", true), ("datetime", true)]).unwrap();
        count_same(&mut t, "delivery", "bdup", &ColPos::Last, "unique").unwrap();
        filter(
            &mut t,
            "bdup",
            "<>unique",
            FilterOpts {
                delete: true,
                ..Default::default()
            },
        )
        .unwrap();
        remove_one(&mut t, "delivery", Which::Last).unwrap();
        // earliest TO of each delivery gone: t1 and t4
        assert_eq!(col(&t, "to"), s(&["t5", "t3", "t2"]));
    }

    #[test]
    fn remove_one_first_and_last() {
        let mut t = tbl(
            &["c", "v"],
            &[&["a", "1"], &["a", "2"], &["b", "3"], &["a", "4"]],
        );
        remove_one(&mut t, "c", Which::First).unwrap();
        assert_eq!(col(&t, "v"), s(&["2", "4"]));
        let mut t = tbl(&["c", "v"], &[&["a", "1"], &["a", "2"], &["b", "3"]]);
        remove_one(&mut t, "c", Which::Last).unwrap();
        assert_eq!(col(&t, "v"), s(&["1"]));
    }

    #[test]
    fn copy_keeps_rows_and_selection() {
        let mut t = tbl(&["a"], &[&["1"], &["2"]]);
        t.visible[0] = false;
        let c = copy_table(&t, "snr_orig");
        assert_eq!(c.name, "snr_orig");
        assert_eq!(c.visible, t.visible);
    }
}
