//! LT03 inputs read from files: the stock DB (the ZMDESNR export, "to_sn"), the VL06O delivery
//! list, and shipment-number lists. Pure file/text handling, no SAP, no prompts, so a headless
//! caller can use it as a library.
//!
//! Columns are matched by sanitised header name (`Customer Part #` -> `customer_part_`), never by
//! position, and the first sheet row that holds the wanted headers is the header row. Material /
//! bin / delivery keys compare case-insensitively like the macro's dictionaries.
#![allow(dead_code)]

use crate::abc::raw::decode_export;
use crate::abc::table::sanitize_header;
use anyhow::{anyhow, bail, Result};
use std::collections::{HashMap, HashSet};
use std::path::Path;

/// Rows of cells of a text-with-tabs export or of one sheet of an `.xlsx`.
///
/// `sheet`: sheet name (case-insensitive) for workbooks; `None` = first sheet. Ignored for text.
pub fn read_grid(path: &Path, sheet: Option<&str>) -> Result<Vec<Vec<String>>> {
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .map(str::to_ascii_lowercase)
        .unwrap_or_default();
    match ext.as_str() {
        "xlsx" | "xlsm" | "xls" => {
            use calamine::{open_workbook_auto, Data, Reader};
            let mut wb = open_workbook_auto(path)
                .map_err(|e| anyhow!("cannot open {}: {e}", path.display()))?;
            let name = match sheet {
                Some(want) => wb
                    .sheet_names()
                    .iter()
                    .find(|n| n.eq_ignore_ascii_case(want.trim()))
                    .cloned()
                    .ok_or_else(|| {
                        anyhow!(
                            "sheet '{want}' not found in {} (sheets: {})",
                            path.display(),
                            wb.sheet_names().join(", ")
                        )
                    })?,
                None => wb
                    .sheet_names()
                    .first()
                    .cloned()
                    .ok_or_else(|| anyhow!("{} has no sheets", path.display()))?,
            };
            let range = wb
                .worksheet_range(&name)
                .map_err(|e| anyhow!("cannot read sheet '{name}' of {}: {e}", path.display()))?;
            let cell = |d: &Data| match d {
                Data::Float(f) if f.fract() == 0.0 => format!("{}", *f as i64),
                Data::Empty => String::new(),
                other => other.to_string(),
            };
            Ok(range.rows().map(|r| r.iter().map(cell).collect()).collect())
        }
        "txt" | "tsv" | "csv" => {
            let bytes =
                std::fs::read(path).map_err(|e| anyhow!("cannot read {}: {e}", path.display()))?;
            Ok(grid_from_text(&decode_export(&bytes)?))
        }
        other => bail!(
            "unsupported file type '.{other}' ({}); use .xlsx or text with tabs",
            path.display()
        ),
    }
}

/// Tab-separated text -> rows (blank lines skipped).
pub fn grid_from_text(text: &str) -> Vec<Vec<String>> {
    text.lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| l.split('\t').map(str::to_string).collect())
        .collect()
}

/// Header compare key: underscores ignored, so `storagebin`, `storage_bin` and `Storage Bin` match.
fn loose(name: &str) -> String {
    name.replace('_', "")
}

/// A grid with a resolved header row: `col("material")` by sanitised name.
pub struct Sheet<'a> {
    header: usize,
    names: Vec<String>,
    grid: &'a [Vec<String>],
}

impl<'a> Sheet<'a> {
    /// Header row = first row containing every name of `required` (sanitised compare).
    pub fn find(grid: &'a [Vec<String>], required: &[&str]) -> Result<Self> {
        for (i, row) in grid.iter().enumerate() {
            let names: Vec<String> = row.iter().map(|c| loose(&sanitize_header(c))).collect();
            if required
                .iter()
                .all(|r| names.iter().any(|n| *n == loose(r)))
            {
                return Ok(Self {
                    header: i,
                    names,
                    grid,
                });
            }
        }
        bail!("no header row with the columns: {}", required.join(", "))
    }

    pub fn col(&self, name: &str) -> Option<usize> {
        let want = loose(name);
        self.names.iter().position(|n| *n == want)
    }

    pub fn rows(&self) -> impl Iterator<Item = &'a Vec<String>> {
        self.grid[self.header + 1..].iter()
    }
}

fn cell(row: &[String], c: usize) -> String {
    row.get(c).map(|v| v.trim().to_string()).unwrap_or_default()
}

/// Numbers as SAP/the macro read them (`Val`): thousands commas removed, junk = 0.
pub fn parse_qty(s: &str) -> f64 {
    s.trim().replace(',', "").parse::<f64>().unwrap_or(0.0)
}

fn key(s: &str) -> String {
    s.trim().to_ascii_uppercase()
}

// ---------------------------------------------------------------------------------------------
// stock DB (ZMDESNR export, "to_sn")

/// Column names of the stock DB (all optional overrides, defaults match the export).
#[derive(Debug, Clone)]
pub struct DbColumns {
    pub material: String,
    pub bin: String,
    pub qty: String,
    pub shipto: String,
    pub blocked: String,
}

impl Default for DbColumns {
    fn default() -> Self {
        Self {
            material: "material".into(),
            bin: "storage_bin".into(),
            qty: "quantity".into(),
            shipto: "shipto_party".into(),
            blocked: "blocked".into(),
        }
    }
}

/// Bins per material from the stock DB.
#[derive(Debug, Default, Clone)]
pub struct StockDb {
    /// material -> every bin seen (any ship-to): the universe when relabel is allowed.
    all_bins: HashMap<String, HashSet<String>>,
    /// (material, ship-to) -> bins of that ship-to: the whitelist.
    ship_bins: HashMap<(String, String), HashSet<String>>,
    /// material -> largest quantity seen (full-pallet hint).
    full_pallet: HashMap<String, f64>,
    /// Bins marked blocked (X / TRUE) on any row.
    blacklist: HashSet<String>,
    pub rows: usize,
}

impl StockDb {
    /// Build from a grid. The material, bin, quantity and ship-to columns are required; the
    /// blocked column is optional (no column = empty blacklist).
    pub fn from_grid(grid: &[Vec<String>], cols: &DbColumns) -> Result<Self> {
        let sh = Sheet::find(grid, &[&cols.material, &cols.bin, &cols.qty, &cols.shipto])?;
        let (cm, cb, cq, cs) = (
            sh.col(&cols.material).unwrap(),
            sh.col(&cols.bin).unwrap(),
            sh.col(&cols.qty).unwrap(),
            sh.col(&cols.shipto).unwrap(),
        );
        let cblk = sh.col(&cols.blocked);
        let mut db = Self::default();
        for r in sh.rows() {
            let (m, b) = (cell(r, cm), cell(r, cb));
            if m.is_empty() || b.is_empty() {
                continue;
            }
            db.rows += 1;
            let (mk, bk) = (key(&m), key(&b));
            if let Some(c) = cblk {
                let v = cell(r, c).to_ascii_uppercase();
                if v == "X" || v == "TRUE" {
                    db.blacklist.insert(bk.clone());
                }
            }
            db.all_bins
                .entry(mk.clone())
                .or_default()
                .insert(bk.clone());
            db.ship_bins
                .entry((mk.clone(), key(&cell(r, cs))))
                .or_default()
                .insert(bk);
            let q = parse_qty(&cell(r, cq));
            let e = db.full_pallet.entry(mk).or_insert(q);
            if q > *e {
                *e = q;
            }
        }
        Ok(db)
    }

    pub fn from_file(path: &Path, sheet: Option<&str>, cols: &DbColumns) -> Result<Self> {
        Self::from_grid(&read_grid(path, sheet)?, cols)
    }

    /// Bins of `material` for `ship_to` (the whitelist); `None` when there are none.
    pub fn whitelist(&self, material: &str, ship_to: &str) -> Option<&HashSet<String>> {
        self.ship_bins
            .get(&(key(material), key(ship_to)))
            .filter(|s| !s.is_empty())
    }

    /// Every bin of `material` regardless of ship-to (relabel universe).
    pub fn all_bins(&self, material: &str) -> Option<&HashSet<String>> {
        self.all_bins.get(&key(material))
    }

    /// Full-pallet hint (largest quantity of the material), 0 when unknown.
    pub fn full_pallet(&self, material: &str) -> f64 {
        self.full_pallet.get(&key(material)).copied().unwrap_or(0.0)
    }

    pub fn is_blacklisted(&self, bin: &str) -> bool {
        self.blacklist.contains(&key(bin))
    }

    pub fn blacklist_len(&self) -> usize {
        self.blacklist.len()
    }
}

/// Case-insensitive membership helper for whitelist sets.
pub fn set_has(set: Option<&HashSet<String>>, bin: &str) -> bool {
    set.is_some_and(|s| s.contains(&key(bin)))
}

// ---------------------------------------------------------------------------------------------
// deliveries (VL06O)

/// How to pick the ship-to when a delivery lists several.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShipToStrategy {
    First,
    Majority,
}

impl ShipToStrategy {
    pub fn parse(s: &str) -> Self {
        if s.trim().eq_ignore_ascii_case("majority") {
            Self::Majority
        } else {
            Self::First
        }
    }
}

/// One delivery of the VL06O export.
#[derive(Debug, Clone, PartialEq)]
pub struct Delivery {
    pub number: String,
    /// Ship-to values of its rows, in order (blank ones skipped).
    pub ship_tos: Vec<String>,
    /// Materials of its rows (unique, in order).
    pub materials: Vec<String>,
    /// Shipment the delivery was found under (set by the caller that knows it).
    pub shipment: Option<String>,
}

impl Delivery {
    /// Ship-to of the delivery plus a note when the rows disagree (macro `ResolveShipToForDelivery`).
    pub fn ship_to(&self, strategy: ShipToStrategy) -> (Option<String>, Option<String>) {
        let mut counts: Vec<(&str, usize)> = Vec::new();
        for s in &self.ship_tos {
            match counts.iter_mut().find(|(v, _)| v == s) {
                Some(e) => e.1 += 1,
                None => counts.push((s, 1)),
            }
        }
        match counts.len() {
            0 => (
                None,
                Some(format!("No ship-to found for delivery {}", self.number)),
            ),
            1 => (Some(counts[0].0.to_string()), None),
            _ => {
                if strategy == ShipToStrategy::Majority {
                    let mut best = counts[0];
                    for c in &counts {
                        if c.1 > best.1 {
                            best = *c;
                        }
                    }
                    (
                        Some(best.0.to_string()),
                        Some(format!(
                            "Multiple ship-to values; using MAJORITY='{}'.",
                            best.0
                        )),
                    )
                } else {
                    (
                        Some(counts[0].0.to_string()),
                        Some(format!(
                            "Multiple ship-to values; using FIRST='{}'.",
                            counts[0].0
                        )),
                    )
                }
            }
        }
    }
}

/// Header names that carry the shipment number in a VL06O layout.
pub const SHIPMENT_COLUMNS: [&str; 3] = ["shipment_number", "shipment", "shipment_no"];

/// Deliveries (unique, first-appearance order) with their ship-tos and materials.
/// `delivery_col` / `shipto_col` / `material_col` are sanitised header names; the material column
/// is optional. When the layout has a shipment column ([`SHIPMENT_COLUMNS`]) the deliveries carry
/// their shipment.
pub fn deliveries_from_grid(
    grid: &[Vec<String>],
    delivery_col: &str,
    shipto_col: &str,
    material_col: &str,
) -> Result<Vec<Delivery>> {
    let sh = Sheet::find(grid, &[delivery_col, shipto_col])?;
    let (cd, cs) = (sh.col(delivery_col).unwrap(), sh.col(shipto_col).unwrap());
    let cm = sh.col(material_col);
    let csh = SHIPMENT_COLUMNS.iter().find_map(|n| sh.col(n));
    let mut out: Vec<Delivery> = Vec::new();
    let mut index: HashMap<String, usize> = HashMap::new();
    for r in sh.rows() {
        let d = cell(r, cd);
        if d.is_empty() {
            continue;
        }
        let i = *index.entry(key(&d)).or_insert_with(|| {
            out.push(Delivery {
                number: d.clone(),
                ship_tos: Vec::new(),
                materials: Vec::new(),
                shipment: None,
            });
            out.len() - 1
        });
        let s = cell(r, cs);
        if !s.is_empty() {
            out[i].ship_tos.push(s);
        }
        if let (Some(c), None) = (csh, &out[i].shipment) {
            let v = cell(r, c);
            if !v.is_empty() {
                out[i].shipment = Some(v);
            }
        }
        if let Some(c) = cm {
            let m = cell(r, c);
            if !m.is_empty() && !out[i].materials.iter().any(|x| x.eq_ignore_ascii_case(&m)) {
                out[i].materials.push(m);
            }
        }
    }
    Ok(out)
}

// ---------------------------------------------------------------------------------------------
// shipment lists

/// Shipment numbers from free text: separated by whitespace, `,`, `;` or `|`; only tokens with
/// digits are kept, non-digits are stripped (like numeric multi-selection pastes); duplicates are
/// dropped, order kept.
pub fn parse_shipments(text: &str) -> Vec<String> {
    let mut seen = HashSet::new();
    text.split(|c: char| c.is_whitespace() || matches!(c, ',' | ';' | '|'))
        .map(|t| t.chars().filter(char::is_ascii_digit).collect::<String>())
        .filter(|t| !t.is_empty())
        .filter(|t| seen.insert(t.clone()))
        .collect()
}

/// Shipments from a grid: the `shipment_number` column when a header has it, else the first
/// column of every row.
pub fn shipments_from_grid(grid: &[Vec<String>]) -> Vec<String> {
    let header = grid.iter().position(|r| {
        r.iter()
            .any(|c| sanitize_header(c) == "shipment_number" || sanitize_header(c) == "shipment")
    });
    let (start, col) = match header {
        Some(h) => {
            let c = grid[h]
                .iter()
                .position(|c| matches!(sanitize_header(c).as_str(), "shipment_number" | "shipment"))
                .unwrap_or(0);
            (h + 1, c)
        }
        None => (0, 0),
    };
    let joined = grid[start.min(grid.len())..]
        .iter()
        .map(|r| cell(r, col))
        .collect::<Vec<_>>()
        .join("\n");
    parse_shipments(&joined)
}

/// Shipments from a file: `.txt`/`.tsv`/`.csv` (a header with `shipment_number` or one number
/// per line) or `.xlsx` (`sheet`, default first).
pub fn read_shipments_file(path: &Path, sheet: Option<&str>) -> Result<Vec<String>> {
    Ok(shipments_from_grid(&read_grid(path, sheet)?))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn g(rows: &[&[&str]]) -> Vec<Vec<String>> {
        rows.iter()
            .map(|r| r.iter().map(|c| c.to_string()).collect())
            .collect()
    }

    #[test]
    fn stock_db_whitelist_universe_pallet_and_blacklist() {
        let grid = g(&[
            &[
                "Material",
                "Storage Bin",
                "Quantity",
                "Shipto Party",
                "Blocked",
            ],
            &["M1", "b1", "40", "S1", ""],
            &["M1", "B2", "100", "S2", "x"],
            &["m1", "B3", "60", "S1", ""],
            &["M2", "B9", "5", "S2", ""],
            &["", "B0", "9", "S1", ""],
        ]);
        let db = StockDb::from_grid(&grid, &DbColumns::default()).unwrap();
        assert_eq!(db.rows, 4);
        let wl = db.whitelist("M1", "s1").unwrap();
        assert!(set_has(Some(wl), "B1") && set_has(Some(wl), "b3") && !set_has(Some(wl), "B2"));
        assert_eq!(db.all_bins("m1").unwrap().len(), 3);
        assert_eq!(db.full_pallet("M1"), 100.0);
        assert_eq!(db.full_pallet("NOPE"), 0.0);
        assert!(db.is_blacklisted("b2") && !db.is_blacklisted("B1"));
        assert!(db.whitelist("M2", "S1").is_none());
    }

    #[test]
    fn stock_db_without_blocked_column_and_missing_columns() {
        let grid = g(&[
            &["material", "storage_bin", "quantity", "shipto_party"],
            &["M1", "B1", "1,200", "S1"],
        ]);
        let db = StockDb::from_grid(&grid, &DbColumns::default()).unwrap();
        assert_eq!(db.blacklist_len(), 0);
        assert_eq!(db.full_pallet("M1"), 1200.0);
        assert!(StockDb::from_grid(&g(&[&["material", "bin"]]), &DbColumns::default()).is_err());
    }

    #[test]
    fn headers_match_without_underscores() {
        let grid = g(&[
            &["Material", "StorageBin", "Quantity", "Shipto_Party"],
            &["M1", "B1", "5", "S1"],
        ]);
        assert_eq!(
            StockDb::from_grid(&grid, &DbColumns::default())
                .unwrap()
                .rows,
            1
        );
    }

    #[test]
    fn header_row_need_not_be_first() {
        let grid = g(&[
            &["report title"],
            &["material", "storage_bin", "quantity", "shipto_party"],
            &["M1", "B1", "5", "S1"],
        ]);
        assert_eq!(
            StockDb::from_grid(&grid, &DbColumns::default())
                .unwrap()
                .rows,
            1
        );
    }

    #[test]
    fn deliveries_unique_in_order_with_ship_to_resolution() {
        let grid = g(&[
            &["delivery", "shipto_party", "material"],
            &["D2", "S1", "M1"],
            &["D1", "S1", "M1"],
            &["D1", "S2", "M2"],
            &["D1", "S2", "M1"],
            &["D3", "", "M3"],
        ]);
        let d = deliveries_from_grid(&grid, "delivery", "shipto_party", "material").unwrap();
        assert_eq!(
            d.iter().map(|x| x.number.as_str()).collect::<Vec<_>>(),
            ["D2", "D1", "D3"]
        );
        assert_eq!(d[1].materials, ["M1", "M2"]);
        let (s, note) = d[1].ship_to(ShipToStrategy::First);
        assert_eq!(s.as_deref(), Some("S1"));
        assert!(note.unwrap().contains("FIRST"));
        let (s, note) = d[1].ship_to(ShipToStrategy::Majority);
        assert_eq!(s.as_deref(), Some("S2"));
        assert!(note.unwrap().contains("MAJORITY"));
        assert_eq!(
            d[0].ship_to(ShipToStrategy::First),
            (Some("S1".into()), None)
        );
        let (s, note) = d[2].ship_to(ShipToStrategy::First);
        assert!(s.is_none() && note.unwrap().contains("No ship-to"));
    }

    #[test]
    fn deliveries_know_their_shipment_when_the_layout_has_the_column() {
        let grid = g(&[
            &["delivery", "shipto_party", "material", "Shipment Number"],
            &["D1", "S1", "M1", "900"],
            &["D1", "S1", "M2", "900"],
            &["D2", "S1", "M1", "100"],
        ]);
        let d = deliveries_from_grid(&grid, "delivery", "shipto_party", "material").unwrap();
        assert_eq!(d[0].shipment.as_deref(), Some("900"));
        assert_eq!(d[1].shipment.as_deref(), Some("100"));
        // no shipment column: unknown
        let grid = g(&[&["delivery", "shipto_party"], &["D1", "S1"]]);
        let d = deliveries_from_grid(&grid, "delivery", "shipto_party", "material").unwrap();
        assert_eq!(d[0].shipment, None);
    }

    #[test]
    fn shipment_text_formats() {
        assert_eq!(
            parse_shipments("24483092, 24483094;24483177|24483092\r\n 24483200 "),
            ["24483092", "24483094", "24483177", "24483200"]
        );
        assert_eq!(parse_shipments("SH 123 x"), ["123"]);
        assert!(parse_shipments("  \n ").is_empty());
    }

    #[test]
    fn shipment_grid_header_or_first_column() {
        let with_header = g(&[&["other", "Shipment Number"], &["a", "111"], &["b", "222"]]);
        assert_eq!(shipments_from_grid(&with_header), ["111", "222"]);
        let plain = g(&[&["333"], &["444"]]);
        assert_eq!(shipments_from_grid(&plain), ["333", "444"]);
    }
}
