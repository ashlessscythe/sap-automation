//! MM60 material list from the anonymised LX03 export (tests/fixtures/mm60, byte-exact).
use sap_automation::mm60::{dedupe_rows, parse_lx03_text, paste_list, rows_from_grid, LxRow};

const LX03: &[u8] = include_bytes!("fixtures/mm60/lx03.txt");

#[test]
fn lx03_fixture_gives_the_paste_list() {
    let rows = parse_lx03_text(LX03).unwrap();
    assert_eq!(rows.len(), 21);
    assert!(rows.iter().all(|r| r.plant.is_some()));
    let list = paste_list(rows);
    assert_eq!(list.len(), 21);
    assert_eq!(list[0], "90000001");
    assert_eq!(list[20], "90000021");
}

#[test]
fn same_material_in_two_plants_is_kept_twice_exact_duplicates_dropped() {
    let mut rows = parse_lx03_text(LX03).unwrap();
    let first = rows[0].clone();
    rows.push(first); // exact duplicate pair
    rows.push(LxRow {
        material: rows[0].material.clone(),
        plant: Some("OTHER".into()),
    });
    let d = dedupe_rows(rows);
    assert_eq!(d.len(), 22);
    assert_eq!(d.last().unwrap().plant.as_deref(), Some("OTHER"));
}

#[test]
fn mm60_export_columns_are_found_by_name() {
    // the MM60 export itself also has Material + Plnt, in a different position
    let text = include_bytes!("fixtures/mm60/mm60.txt");
    let rows = parse_lx03_text(text).unwrap();
    assert!(rows.len() > 21);
    assert_eq!(rows[0].material, "90000001");
    assert!(rows_from_grid(&[]).is_err());
}
