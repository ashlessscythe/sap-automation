//! `toyota` stages on the REAL SAP text exports (tests/fixtures/toyota/real/, tiny: 3 shipments,
//! 3 deliveries, one plant) compared with the macro output `11-05-2026_TOYOTA.xlsx` converted to
//! TSV (tests/fixtures/toyota/expected_*.tsv, see the README there). Known differences are
//! asserted exactly; edge cases (several plants, blank plant, deliveries without ZMDETPC row,
//! rounding) use small synthetic tables around the real ones.
mod common;

use common::{load_fixture, load_real_in};
use sap_automation::abc::inv;
use sap_automation::abc::table::Table;

const RUN_DATE: &str = "2026-11-05";

fn vt11() -> Table {
    load_real_in("toyota", "vt11_inv", "raw_vt11.txt")
}
fn vl06o_raw() -> Table {
    load_real_in("toyota", "vl06o", "raw_vl06o.txt")
}
fn zmdetpc() -> Table {
    load_real_in("toyota", "zmdetpc", "raw_zmdetpc.txt")
}

fn merged_of(vl: Table, z: &Table) -> Table {
    let mut vl = vl;
    inv::prepare_vl06o(&mut vl, RUN_DATE).unwrap();
    inv::merge_zmdetpc(&vl, z).unwrap()
}

fn merged() -> Table {
    merged_of(vl06o_raw(), &zmdetpc())
}

fn t(headers: &[&str], rows: &[&[&str]]) -> Table {
    Table::new(
        "t",
        headers.iter().map(|s| s.to_string()).collect(),
        rows.iter()
            .map(|r| r.iter().map(|s| s.to_string()).collect())
            .collect(),
    )
}

#[test]
fn loader_shapes() {
    let vt = vt11();
    assert_eq!((vt.nrows(), vt.headers.len()), (3, 13));
    assert_eq!(vt.rows[0][0], "13000443");
    let vl = vl06o_raw();
    assert_eq!((vl.nrows(), vl.headers.len()), (12, 7));
    // 12 item rows, 3 distinct deliveries; the file starts with a blank line, no preamble
    assert_eq!(vl.headers[0], "delivery");
    let z = zmdetpc();
    assert_eq!((z.nrows(), z.headers.len()), (3, 3));
    assert_eq!(z.rows[0], ["13000821", "2", "0"]);
}

/// Differences to the xlsx VT11 sheet: only the list-width truncation of free-text columns
/// (our value is a prefix of the xlsx one); Excel dates were written back as text.
#[test]
fn vt11_matches_xlsx_sheet() {
    let ours = vt11();
    let x = load_fixture("toyota", "expected_vt11.tsv");
    assert_eq!(x.headers, ours.headers);
    assert_eq!(x.nrows(), ours.nrows());
    let trunc = [
        "seal_number_2",
        "descripof_shipment",
        "forwarding_agent_tracking_id",
    ];
    for (c, name) in ours.headers.iter().enumerate() {
        for (o, e) in ours.rows.iter().zip(&x.rows) {
            if trunc.contains(&name.as_str()) {
                assert!(e[c].starts_with(&o[c]), "{name}: {:?} vs {:?}", o[c], e[c]);
            } else {
                assert_eq!(o[c], e[c], "column {name}");
            }
        }
    }
}

/// Differences to the xlsx `TOYOTA_VL06O` sheet: none in the first 12 columns (3 rows, all with
/// a ZMDETPC row). The sheet's trailing columns M:O (`delivery_`, `pallet`, `child` again) hold
/// the same values as I:K here, because every delivery has a ZMDETPC row (in xdock they were
/// unaligned leftovers); they are not part of our table.
#[test]
fn vl06o_matches_xlsx_sheet() {
    let ours = merged();
    let x = load_fixture("toyota", "expected_vl06o.tsv");
    assert_eq!((x.nrows(), x.headers.len()), (3, 15));
    assert_eq!(ours.headers, x.headers[..12]);
    assert_eq!(ours.nrows(), 3);
    for (o, e) in ours.rows.iter().zip(&x.rows) {
        assert_eq!(o[..], e[..12]);
        assert_eq!(
            e[8..11],
            e[12..15],
            "stray columns repeat delivery_/pallet/child"
        );
    }
    // first row per delivery, ordered by delivery, date column
    assert_eq!(
        ours.rows.iter().map(|r| r[0].as_str()).collect::<Vec<_>>(),
        ["13000821", "13000822", "13000823"]
    );
    assert!(ours.rows.iter().all(|r| r[7] == RUN_DATE));
}

/// Pivot and trailers vs the xlsx pivot sheet (columns B:C pivot, E:F trailer cells).
/// Differences: the sheet's pivot has the header `Row Labels` / `Sum of total` and a
/// `Grand Total` row (ours: `plant` / `total`, no grand total). The trailer cells E/F are the
/// macro's `unique(column B)` and `ROUND(SUMIF(...)/14.5, 0)` copied down: E1/F1 are an artefact
/// of the header cell (`Row Labels`, 0) and E3 is `Grand Total` with a blank F (division by a
/// text sum failed); ours is a separate `trailers` table with the plant rows only
/// (`plant`, `trailers`), here `PL55` = ROUND(4 / 14.5) = 0, equal to the sheet's F2.
#[test]
fn pivot_and_trailers_match_xlsx_except_documented_differences() {
    let x = load_fixture("toyota", "expected_pivot.tsv");
    // the file has the header row as its first line, which `from_tsv` takes as headers
    assert_eq!(
        x.headers,
        ["Row Labels", "Sum of total", "", "Row Labels", "0"]
    );
    assert_eq!(x.rows[0], ["PL55", "4", "", "PL55", "0"]);
    assert_eq!(x.rows[1], ["Grand Total", "4", "", "Grand Total", ""]);

    let p = inv::pivot_sum(&merged(), "plant", "total").unwrap();
    assert_eq!(p.headers, ["plant", "total"]);
    assert_eq!(p.rows, vec![vec!["PL55".to_string(), "4".to_string()]]);
    assert_eq!(p.rows[0][..], x.rows[0][..2]);

    let tr = inv::trailers(&p).unwrap();
    assert_eq!(tr.headers, ["plant", "trailers"]);
    assert_eq!(tr.rows, vec![vec!["PL55".to_string(), "0".to_string()]]);
    assert_eq!(tr.rows[0][1], x.rows[0][4]);
}

#[test]
fn edge_deliveries_without_zmdetpc_row_fall_back_to_packages() {
    // drop the pallet row of 13000822 and the whole table of 13000823
    let mut z = zmdetpc();
    z.rows.retain(|r| r[0] == "13000821");
    let m = merged_of(vl06o_raw(), &z);
    let tot: Vec<&str> = m.rows.iter().map(|r| r[11].as_str()).collect();
    assert_eq!(
        tot,
        ["2", "1", "1"],
        "missing rows take number_of_packages (1)"
    );
    assert_eq!(
        m.rows[1][8], "13000822",
        "delivery_ is filled from delivery"
    );
    assert_eq!((m.rows[1][9].as_str(), m.rows[1][10].as_str()), ("", ""));
    let p = inv::pivot_sum(&m, "plant", "total").unwrap();
    assert_eq!(p.rows, vec![vec!["PL55".to_string(), "4".to_string()]]);

    // an empty ZMDETPC table (columns only) is not an error here: everything falls back
    let empty = t(&["delivery", "pallet", "child"], &[]);
    let m = merged_of(vl06o_raw(), &empty);
    assert_eq!(m.nrows(), 3);
    assert!(m.rows.iter().all(|r| r[11] == "1"));
}

#[test]
fn edge_zero_total_becomes_one() {
    // pallet + child = 0 on a ZMDETPC row: the exact 0 becomes 1 (spec item 3 reading)
    let mut z = zmdetpc();
    z.rows[2] = vec!["13000823".into(), "0".into(), "0".into()];
    let m = merged_of(vl06o_raw(), &z);
    assert_eq!(m.rows[2][11], "1");
}

#[test]
fn edge_several_plants_blank_plant_and_rounding() {
    let vl = t(
        &["delivery", "plant", "number_of_packages"],
        &[
            &["1", "PL55", "1"],
            &["2", "PL33", "1"],
            &["3", "", "1"],
            &["4", "PL55", "1"],
            &["5", "PL55", "1"],
        ],
    );
    let z = t(
        &["delivery", "pallet", "child"],
        &[
            &["1", "10", "0"],
            &["2", "6", "1"],
            &["3", "7", "0"],
            &["4", "10", "9"],
            &["5", "0", "0"],
        ],
    );
    let m = merged_of(vl, &z);
    let p = inv::pivot_sum(&m, "plant", "total").unwrap();
    // PL55: 10 + 19 + 1 (0 -> 1) = 30; PL33 = 7; blank = 7, last
    assert_eq!(
        p.rows,
        vec![
            vec!["PL33".to_string(), "7".to_string()],
            vec!["PL55".to_string(), "30".to_string()],
            vec!["(blank)".to_string(), "7".to_string()],
        ]
    );
    // 7 / 14.5 = 0.48 -> 0; 30 / 14.5 = 2.07 -> 2
    let tr = inv::trailers(&p).unwrap();
    let got: Vec<(&str, &str)> = tr
        .rows
        .iter()
        .map(|r| (r[0].as_str(), r[1].as_str()))
        .collect();
    assert_eq!(got, [("PL33", "0"), ("PL55", "2"), ("(blank)", "0")]);
    // exact halves round away from zero (Excel): 7.25 / 14.5 = 0.5 -> 1, 29 / 14.5 = 2
    let half = t(
        &["plant", "total"],
        &[&["A", "7.25"], &["B", "29"], &["C", "21.75"]],
    );
    let tr = inv::trailers(&half).unwrap();
    let v: Vec<&str> = tr.rows.iter().map(|r| r[1].as_str()).collect();
    assert_eq!(v, ["1", "2", "2"]);
}
