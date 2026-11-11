//! Loader for SAP "text with tabs" list exports (SAP export_type 1) -> [`Table`].
//!
//! Derived from the real `mustgo` exports (vt11, vl06o, zmdesnr, lt23; see
//! `tests/fixtures/mustgo/README.txt`). Observed format: ASCII/UTF-8, no BOM, CRLF line ends,
//! tab separated. A tcode-specific preamble (0-6 lines) comes first, then the header line,
//! one blank line, then the data rows. Every row starts with a tab (list border), so the first
//! column is blank. Numbers are right-aligned, space padded, with `,` thousands and `.`
//! decimals (`34,800.000`); dates and times are already `yyyy-mm-dd` / `hh:mm:ss` for the
//! observed SAP user settings (the date format is a parameter). Not observed, therefore not
//! handled: other encodings, negative numbers, summary/separator rows.
#![allow(dead_code)]

use super::table::{normalize_cell, sanitize_header, Table};
use anyhow::{anyhow, Result};
use chrono::{NaiveDate, NaiveDateTime, NaiveTime};

/// Parse options.
#[derive(Debug, Clone)]
pub struct RawOptions {
    /// chrono format of dates in the export (default: observed `%Y-%m-%d`).
    pub date_format: String,
    /// Sanitised headers whose cells are kept as raw text (no date/number conversion), e.g.
    /// the VT11 `Description` column which holds text like `11/04/2026 1800` (decision 26).
    pub raw_columns: Vec<String>,
}

impl Default for RawOptions {
    fn default() -> Self {
        RawOptions {
            date_format: "%Y-%m-%d".to_string(),
            raw_columns: vec!["description".to_string()],
        }
    }
}

/// Windows-1252 `0x80..=0x9F` (the range where it differs from Latin-1); `None` = undefined.
const CP1252_HIGH: [Option<char>; 32] = [
    Some('\u{20AC}'),
    None,
    Some('\u{201A}'),
    Some('\u{0192}'),
    Some('\u{201E}'),
    Some('\u{2026}'),
    Some('\u{2020}'),
    Some('\u{2021}'),
    Some('\u{02C6}'),
    Some('\u{2030}'),
    Some('\u{0160}'),
    Some('\u{2039}'),
    Some('\u{0152}'),
    None,
    Some('\u{017D}'),
    None,
    None,
    Some('\u{2018}'),
    Some('\u{2019}'),
    Some('\u{201C}'),
    Some('\u{201D}'),
    Some('\u{2022}'),
    Some('\u{2013}'),
    Some('\u{2014}'),
    Some('\u{02DC}'),
    Some('\u{2122}'),
    Some('\u{0161}'),
    Some('\u{203A}'),
    Some('\u{0153}'),
    None,
    Some('\u{017E}'),
    Some('\u{0178}'),
];

/// Decode Windows-1252 (SAP's default text-file encoding on Windows); an undefined byte is an
/// error rather than a guess.
fn decode_cp1252(b: &[u8]) -> Result<String> {
    let mut s = String::with_capacity(b.len());
    for (i, &x) in b.iter().enumerate() {
        match x {
            0x80..=0x9F => match CP1252_HIGH[usize::from(x - 0x80)] {
                Some(c) => s.push(c),
                None => {
                    return Err(anyhow!(
                        "export is neither UTF-8 nor Windows-1252 (undefined byte 0x{x:02X} at {i})"
                    ))
                }
            },
            // ASCII and 0xA0..=0xFF map to the same code points (Latin-1)
            _ => s.push(char::from(x)),
        }
    }
    Ok(s)
}

/// Decode export bytes: UTF-8 (a BOM is dropped); when the bytes are not valid UTF-8 the file
/// is read as Windows-1252 (a real VT11 export had a non-breaking space, byte `0xA0`, in a
/// "Seal Number" text). Bytes that are neither are an error.
pub fn decode_export(bytes: &[u8]) -> Result<String> {
    let b = bytes.strip_prefix(&[0xEF, 0xBB, 0xBF]).unwrap_or(bytes);
    match std::str::from_utf8(b) {
        Ok(s) => Ok(s.to_string()),
        Err(_) => {
            // UTF-16 (BOM) would "decode" as garbage in Windows-1252: say what it is instead
            if b.starts_with(&[0xFF, 0xFE]) || b.starts_with(&[0xFE, 0xFF]) {
                return Err(anyhow!(
                    "export looks like UTF-16 (byte order mark); only UTF-8 and Windows-1252 text is supported"
                ));
            }
            decode_cp1252(b)
        }
    }
}

fn fields(line: &str) -> Vec<&str> {
    line.split('\t').collect()
}

/// Index of the header line. Candidates are the non-blank lines with at least 2 fields that
/// have at least as many fields as the next non-blank line (preamble lines are normally shorter
/// than the header). When several lines qualify (a preamble of equally wide lines, as in the
/// ZMDETPC export), the first one starting with a tab (the list border, like every header and
/// data row) wins; otherwise the first candidate.
fn find_header(lines: &[&str]) -> Option<usize> {
    let ne: Vec<usize> = (0..lines.len())
        .filter(|&i| !lines[i].trim().is_empty())
        .collect();
    let mut first: Option<usize> = None;
    for (k, &i) in ne.iter().enumerate() {
        let n = fields(lines[i]).len();
        if n < 2 {
            continue;
        }
        let qualifies = match ne.get(k + 1) {
            None => true,
            Some(&j) => n >= fields(lines[j]).len(),
        };
        if !qualifies {
            continue;
        }
        if lines[i].starts_with('\t') {
            return Some(i);
        }
        first.get_or_insert(i);
    }
    first
}

/// `-?d{1,3}(,ddd)+(.d+)?` -> commas removed.
fn strip_thousands(s: &str) -> Option<String> {
    let body = s.strip_prefix('-').unwrap_or(s);
    let (int, frac) = match body.split_once('.') {
        Some((i, f)) => (i, Some(f)),
        None => (body, None),
    };
    if frac.is_some_and(|f| f.is_empty() || !f.chars().all(|c| c.is_ascii_digit())) {
        return None;
    }
    let groups: Vec<&str> = int.split(',').collect();
    let ok = groups.len() > 1
        && (1..=3).contains(&groups[0].len())
        && groups.iter().all(|g| g.chars().all(|c| c.is_ascii_digit()))
        && groups[1..].iter().all(|g| g.len() == 3);
    ok.then(|| s.replace(',', ""))
}

fn plausible_year(y: i32) -> bool {
    (1900..=2200).contains(&y)
}

/// Date -> `yyyy-mm-dd`, time -> `hh:mm:ss`, date + time -> `yyyy-mm-dd hh:mm:ss`; anything
/// else is returned unchanged.
fn convert_datetime(v: &str, date_format: &str) -> Option<String> {
    use chrono::Datelike;
    if v.len() < 7 {
        return None;
    }
    if let Ok(t) = NaiveTime::parse_from_str(v, "%H:%M:%S") {
        return Some(t.format("%H:%M:%S").to_string());
    }
    if let Ok(d) = NaiveDate::parse_from_str(v, date_format) {
        if plausible_year(d.year()) {
            return Some(d.format("%Y-%m-%d").to_string());
        }
    }
    if let Ok(dt) = NaiveDateTime::parse_from_str(v, &format!("{date_format} %H:%M:%S")) {
        if plausible_year(dt.year()) {
            return Some(dt.format("%Y-%m-%d %H:%M:%S").to_string());
        }
    }
    None
}

fn convert_cell(raw: &str, date_format: &str) -> String {
    let v = raw.trim();
    if let Some(n) = strip_thousands(v) {
        return n;
    }
    convert_datetime(v, date_format).unwrap_or_else(|| v.to_string())
}

/// Parse an export. Rules: preamble and blank lines are skipped; the leading blank column and
/// any other blank-header column without data are dropped; a blank-header column with data is
/// named `customer` (`customer_2`, ... for further ones); short rows are padded, a row with
/// extra non-blank fields is an error; cells are trimmed, thousands separators removed, dates
/// and times converted, leading zeros of integers stripped and `.000` dropped. An input with no
/// header line gives an empty table (no columns); a header only gives a table with no rows.
pub fn parse_export(name: &str, text: &str, opts: &RawOptions) -> Result<Table> {
    let lines: Vec<&str> = text.lines().collect();
    let Some(h) = find_header(&lines) else {
        return Ok(Table::new(name, Vec::new(), Vec::new()));
    };
    let header: Vec<&str> = fields(lines[h]);
    let width = header.len();
    let mut rows: Vec<Vec<&str>> = Vec::new();
    for (off, line) in lines[h + 1..].iter().enumerate() {
        if line.trim().is_empty() {
            continue;
        }
        let f = fields(line);
        if f.len() > width && f[width..].iter().any(|c| !c.trim().is_empty()) {
            return Err(anyhow!(
                "{name}: line {} has {} fields, header has {width}",
                h + off + 2,
                f.len()
            ));
        }
        rows.push(f);
    }
    let has_data = |c: usize| {
        rows.iter()
            .any(|r| r.get(c).is_some_and(|v| !v.trim().is_empty()))
    };
    let keep: Vec<usize> = (0..width)
        .filter(|&c| !header[c].trim().is_empty() || has_data(c))
        .collect();
    let mut blanks = 0;
    let headers: Vec<String> = keep
        .iter()
        .map(|&c| {
            if header[c].trim().is_empty() {
                blanks += 1;
                if blanks == 1 {
                    String::new() // from_raw fills `customer`
                } else {
                    format!("customer_{blanks}")
                }
            } else {
                header[c].to_string()
            }
        })
        .collect();
    let raw_cols: Vec<bool> = headers
        .iter()
        .map(|h| opts.raw_columns.contains(&sanitize_header(h)))
        .collect();
    let data: Vec<Vec<String>> = rows
        .iter()
        .map(|r| {
            keep.iter()
                .enumerate()
                .map(|(i, &c)| {
                    let cell = r.get(c).copied().unwrap_or("");
                    if raw_cols[i] {
                        cell.trim().to_string()
                    } else {
                        convert_cell(cell, &opts.date_format)
                    }
                })
                .collect()
        })
        .collect();
    Ok(Table::from_raw(name, &headers, data))
}

/// [`decode_export`] + [`parse_export`].
pub fn load_export(name: &str, bytes: &[u8], opts: &RawOptions) -> Result<Table> {
    parse_export(name, &decode_export(bytes)?, opts)
}

/// Rename columns (sanitised `from` -> `to`); columns not listed and aliases not present are
/// left alone. Used to map the short list-screen headers to the canonical names.
pub fn rename_headers(t: &mut Table, aliases: &[(&str, &str)]) {
    for h in &mut t.headers {
        if let Some((_, to)) = aliases.iter().find(|(from, _)| h == from) {
            *h = to.to_string();
        }
    }
}

/// Real list header (sanitised) -> canonical name (the xlsx / macro layout names).
pub const VT11_ALIASES: &[(&str, &str)] = &[
    ("shipment", "shipment_number"),
    ("shty", "shipment_type"),
    ("tppt", "transportplanningpt"),
    ("route", "shipment_route"),
    ("description", "descripof_shipment"),
    ("servcagent", "service_agent"),
];
/// VT11 list layout of the INV reports (`xdock`, `toyota`, `misc`): a single `S` column
/// (overall transport status), unlike the `mustgo` layout that has two.
pub const VT11_INV_ALIASES: &[(&str, &str)] = &[
    ("shipment", "shipment_number"),
    ("s", "ovtransport_status"),
    ("shty", "shipment_type"),
    ("tppt", "transportplanningpt"),
    ("st", "shipping_type"),
    ("servcagent", "service_agent"),
    ("actshipend", "actualshipmentend"),
    ("acshet", "actshipmnt_end_time"),
    ("description", "descripof_shipment"),
    ("fa_tracking_id", "forwarding_agent_tracking_id"),
];
pub const VL06O_ALIASES: &[(&str, &str)] = &[
    ("shipto", "shipto_party"),
    ("gm", "total_gds_mvt_stat"),
    ("plnt", "plant"),
    ("shpt", "shipping_pointreceiving_pt"),
    ("nopk", "number_of_packages"),
    ("delivdate", "deliv_datefromto"),
    ("su", "sales_unit"),
    ("wun", "unit_of_weight"),
];
pub const ZMDESNR_ALIASES: &[(&str, &str)] = &[
    ("lm_serial", "lm_serial_number"),
    ("parent_no", "parent_serial_number"),
    ("plnt", "plant"),
    ("cust_part", "customer_part_"),
    ("whn", "warehouse_number"),
    ("sloc", "storage_location"),
    ("typ", "storage_type"),
    ("bin", "storage_bin"),
    ("shipto", "shipto_party"),
    ("to_number", "transfer_order_number"),
    ("created", "created_by"),
    // site_relabel layout (real export of the relabel run of 2026-11-05): short list titles for
    // the long names of the macro sheets
    ("sr_no_verf", "serial_number_2nd_party_verification"),
    ("scan_cpy", "spv_scan_copies"),
    ("doc_date", "document_date"),
    ("plant_doc", "plant_doc_line"),
    ("kanban_num", "kanban_number"),
];
/// ZMDETPC summarised: the list titles already are the canonical names (`Delivery`, `Pallet`,
/// `Child`; a blank column sits between `Pallet` and `Child` and is dropped by the loader).
pub const ZMDETPC_ALIASES: &[(&str, &str)] = &[];
/// ZVT11 (Closed, layout `add_del`). Canonical names are the sanitised layout titles of spec
/// 3.2 / 8.2: `date`, `trailer_number`, `shipment_number`, `delivery`, `plant`,
/// `ship_tofrom_name`, `shipment_end_date`. The real list (run of 2026-11-05) shortens three titles
/// (`Trailer Nu`, `Delivery N`, `Shipment E`) and calls the shipment column `Shipment`; a blank
/// column sits between `Date` and `Trailer Nu` and is dropped by the loader.
pub const ZVT11_ALIASES: &[(&str, &str)] = &[
    ("trailer_nu", "trailer_number"),
    ("shipment", "shipment_number"),
    ("delivery_n", "delivery"),
    ("delivery_number", "delivery"),
    ("plnt", "plant"),
    ("shipment_e", "shipment_end_date"),
];
pub const LT23_ALIASES: &[(&str, &str)] = &[
    ("to_number", "transfer_order_number"),
    ("created_on", "creation_date"),
    ("time", "creation_time"),
    ("plnt", "plant"),
    ("typ", "source_storage_type"),
    ("source_bin", "source_storage_bin"),
    ("cs", "confirmation_status"),
];

/// Load + rename in one go. `tcode` is `vt11`, `vt11_inv` (INV list layout), `vl06o`, `zmdesnr`,
/// `lt23`, `zmdetpc` or `zvt11`.
pub fn load_export_canonical(
    tcode: &str,
    name: &str,
    bytes: &[u8],
    opts: &RawOptions,
) -> Result<Table> {
    let aliases = match tcode {
        "vt11" => VT11_ALIASES,
        "vt11_inv" => VT11_INV_ALIASES,
        "vl06o" => VL06O_ALIASES,
        "zmdesnr" => ZMDESNR_ALIASES,
        "lt23" => LT23_ALIASES,
        "zmdetpc" => ZMDETPC_ALIASES,
        "zvt11" => ZVT11_ALIASES,
        other => return Err(anyhow!("no header aliases known for tcode '{other}'")),
    };
    let mut t = load_export(name, bytes, opts)?;
    rename_headers(&mut t, aliases);
    Ok(t)
}

/// Normalise a single already-extracted cell the way the loader does (exposed for callers that
/// build tables by hand).
pub fn normalize_export_cell(raw: &str, opts: &RawOptions) -> String {
    normalize_cell(&convert_cell(raw, &opts.date_format))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(text: &str) -> Table {
        parse_export("t", text, &RawOptions::default()).unwrap()
    }

    #[test]
    fn empty_and_header_only() {
        let t = p("");
        assert!(t.is_empty_import() && t.nrows() == 0);
        let t = p("\r\n\r\n");
        assert!(t.is_empty_import());
        let t = p("\tA\tB\r\n");
        assert_eq!(t.headers, vec!["a", "b"]);
        assert_eq!(t.nrows(), 0);
        // preamble only (single field lines) -> empty
        assert!(p("Serial Number History\r\nUser name  :X\r\n").is_empty_import());
    }

    #[test]
    fn preamble_header_blank_line_data() {
        let t = p("Warehouse No.\t\t\tWH01\tExample\r\n\r\n\r\n\tDelivery\tTO Number\t\tQty\tCS\r\n\r\n\t0085\t0209\t\t     144.000\r\n\t0086\t0210\t\t  1,200.500\r\n");
        assert_eq!(t.headers, vec!["delivery", "to_number", "qty", "cs"]);
        assert_eq!(
            t.rows,
            vec![
                vec!["85", "209", "144", ""],
                vec!["86", "210", "1200.5", ""]
            ]
        );
    }

    #[test]
    fn blank_header_with_data_is_customer_and_empty_ones_dropped() {
        let t = p("\tId\t\t\tName\r\n\r\n\t1\tfoo\tbar\tbaz\r\n");
        assert_eq!(t.headers, vec!["id", "customer", "customer_2", "name"]);
        assert_eq!(t.rows[0], vec!["1", "foo", "bar", "baz"]);
    }

    #[test]
    fn ragged_rows() {
        let t = p("\tA\tB\tC\r\n\r\n\t1\t2\r\n\t3\t4\t5\t\r\n");
        assert_eq!(t.rows, vec![vec!["1", "2", ""], vec!["3", "4", "5"]]);
        let e = parse_export(
            "t",
            "\tA\tB\r\n\r\n\t1\t2\r\n\t1\t2\t3\r\n",
            &RawOptions::default(),
        )
        .unwrap_err()
        .to_string();
        assert!(e.contains("line 4"), "{e}");
    }

    #[test]
    fn numbers_dates_times() {
        let t = p("\tN\tD\tT\tX\tDescription\r\n\r\n\t   34,800.000\t2026-11-05\t6:26:01\t1,2\t11/05/2026\r\n");
        assert_eq!(
            t.rows[0],
            vec!["34800", "2026-11-05", "06:26:01", "1,2", "11/05/2026"]
        );
        let o = RawOptions {
            date_format: "%m/%d/%Y".into(),
            raw_columns: vec![],
        };
        let t = parse_export(
            "t",
            "\tD\tE\r\n\r\n\t11/05/2026\t11/05/2026 18:00:00\r\n",
            &o,
        )
        .unwrap();
        assert_eq!(t.rows[0], vec!["2026-11-05", "2026-11-05 18:00:00"]);
        // unparseable / odd values stay raw; a part number that looks like a date is not touched
        assert_eq!(
            normalize_export_cell("1-2-3", &RawOptions::default()),
            "1-2-3"
        );
        assert_eq!(
            normalize_export_cell("1,23", &RawOptions::default()),
            "1,23"
        );
        assert_eq!(
            normalize_export_cell("1,234,567", &RawOptions::default()),
            "1234567"
        );
    }

    #[test]
    fn decode_rules() {
        assert_eq!(decode_export(b"\xEF\xBB\xBFab").unwrap(), "ab");
        // valid UTF-8 stays UTF-8 (a multi-byte char is not re-read as Windows-1252)
        assert_eq!(decode_export("caf\u{e9}".as_bytes()).unwrap(), "caf\u{e9}");
    }

    #[test]
    fn invalid_utf8_falls_back_to_windows_1252() {
        // 0xA0 = non-breaking space (the byte of the real VT11 failure), 0xE9 = e acute
        assert_eq!(
            decode_export(b"FEDX\xA0 729656\xE9").unwrap(),
            "FEDX\u{a0} 729656\u{e9}"
        );
        // the 0x80..0x9F range differs from Latin-1: euro, curly quotes, en dash
        assert_eq!(
            decode_export(&[0x80, 0x93, 0x94, 0x96]).unwrap(),
            "\u{20ac}\u{201c}\u{201d}\u{2013}"
        );
        // bytes undefined in Windows-1252 are an error, not a guess
        let e = decode_export(&[0x66, 0x81]).unwrap_err().to_string();
        assert!(e.contains("0x81"), "{e}");
        // UTF-16 is recognised by its BOM and rejected, not decoded as Windows-1252
        let e = decode_export(&[0xFF, 0xFE, b'a', 0x00])
            .unwrap_err()
            .to_string();
        assert!(e.contains("UTF-16"), "{e}");
        assert!(decode_export(&[0xFE, 0xFF, 0x00, b'a']).is_err());
    }

    #[test]
    fn non_breaking_space_in_a_cell_loads_and_trims() {
        let txt: &[u8] =
            b"\tShipment\tSeal Number 2\r\n\r\n\t13000308\t\xA0FEDX\xA0 729656 S\xA0\r\n";
        let t = load_export("t", txt, &RawOptions::default()).unwrap();
        assert_eq!(t.rows[0][0], "13000308");
        // leading/trailing NBSP are trimmed like spaces, an interior one is kept
        assert_eq!(t.rows[0][1], "FEDX\u{a0} 729656 S");
    }

    #[test]
    fn aliases_are_tolerant() {
        // canonical names pass through, unknown extra columns are kept untouched
        let txt = "\tDelivery\tTO Number\tstorage_bin\tWeird Extra\r\n\r\n\t1\t2\t3\t4\r\n";
        let t =
            load_export_canonical("zmdesnr", "t", txt.as_bytes(), &RawOptions::default()).unwrap();
        assert_eq!(
            t.headers,
            vec![
                "delivery",
                "transfer_order_number",
                "storage_bin",
                "weird_extra"
            ]
        );
        let txt = "\tdelivery\ttransfer_order_number\r\n\r\n\t1\t2\r\n";
        let t = load_export_canonical("lt23", "t", txt.as_bytes(), &RawOptions::default()).unwrap();
        assert_eq!(t.headers, vec!["delivery", "transfer_order_number"]);
    }

    #[test]
    fn equal_width_preamble_does_not_hide_the_header() {
        // ZMDETPC: equally wide preamble lines, then the header (the blank column between
        // Pallet and Child is dropped); zero padded deliveries are normalised.
        let txt = "Track Pallet Count\r\nExample\r\n\r\nUser       :\t\t\tX\r\nRun Date   :\t\t\t2026-11-10\r\nRun Time   :\t\t\t13:57:15\r\n\r\n\tDelivery\tPallet\t\tChild\r\n\r\n\t0013000804\t    1\t\t   0\r\n\t1300000000\t    0\t\t   2\r\n";
        let t =
            load_export_canonical("zmdetpc", "t", txt.as_bytes(), &RawOptions::default()).unwrap();
        assert_eq!(t.headers, vec!["delivery", "pallet", "child"]);
        assert_eq!(
            t.rows,
            vec![vec!["13000804", "1", "0"], vec!["1300000000", "0", "2"]]
        );
    }

    #[test]
    fn vl06o_xdock_layout_headers() {
        let txt = "\tDelivery\tShip-to\tName of the ship-to party\tExternal Delivery ID\tGM\tPlnt\tNo.Pk\r\n\r\n\t13000939\t130034\tZJKBR (1)\t\tC\tPL34\t    1\r\n";
        let t =
            load_export_canonical("vl06o", "t", txt.as_bytes(), &RawOptions::default()).unwrap();
        assert_eq!(
            t.headers,
            vec![
                "delivery",
                "shipto_party",
                "name_of_the_shipto_party",
                "external_delivery_id",
                "total_gds_mvt_stat",
                "plant",
                "number_of_packages"
            ]
        );
    }

    #[test]
    fn zvt11_headers_are_trimmed_before_sanitising() {
        // spec 9.5: the ZVT11 header ` Date` becomes `date`
        let txt = "\t Date\tTrailer Number\tShipment Number\tDelivery Number\tPlant\tShip To/From Name\tShipment End date\r\n\r\n\t2026-11-05\t53145\t13000522\t0085650001\tPL63\tACME (1)\t2026-11-05\r\n";
        let t =
            load_export_canonical("zvt11", "t", txt.as_bytes(), &RawOptions::default()).unwrap();
        assert_eq!(
            t.headers,
            vec![
                "date",
                "trailer_number",
                "shipment_number",
                "delivery",
                "plant",
                "ship_tofrom_name",
                "shipment_end_date"
            ]
        );
        assert_eq!(t.rows[0][3], "85650001");
    }

    #[test]
    fn unknown_tcode() {
        assert!(load_export_canonical("x", "t", b"", &RawOptions::default()).is_err());
    }
}
