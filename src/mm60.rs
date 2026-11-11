//! MM60 (material list) export driven by the material column of an LX03 export.
//!
//! Standalone tcode, like every other one: no joining, no formulas, no formatting. It reads
//! the `Material` (and `Plnt`) column of the newest LX03 export (or `--lx03-file`), pastes the
//! materials into the multi-selection `MS_MATNR`, CLEARS the plant selection (`MS_WERKS`; the
//! plants are not copied), runs, optionally picks a layout, and exports with the usual
//! `export_type` / `--rname-dir` conventions (`<reports>\[<rname-dir>\]MM60\<ts>_MM60.<ext>`).
//!
//! Columns are matched by header name (any order). Rows are de-duplicated by the pair
//! (material, plant) only, never by material alone; without a plant column nothing is
//! de-duplicated.
//!
//! Run live by the owner (2026-11-11): the whole flow works, including the local-file export
//! (`menu[0]/menu[3]/menu[2]`; a dialog not titled "save list in file" is rejected) and the
//! no-data detection after execute. Control ids: `btn%_MS_MATNR_%_APP_%-VALU_PUSH`,
//! `btn%_MS_WERKS_%_APP_%-VALU_PUSH`, execute `btn[8]`, layout button `btn[33]` + grid (both
//! recorded live), spreadsheet export `menu[0]/menu[3]/menu[1]` (VBA).
#![allow(dead_code)]

use crate::abc::raw::decode_export;
use crate::utils::cli_overrides::can_prompt;
use crate::utils::export_outcome::{classify_after_execute, ClassifyOptions, ExportOutcome};
use crate::utils::sap_ctrl_utils::exist_ctrl;
use crate::utils::sap_export_utils::{export_to_target, export_type_to_extension, ExportTarget};
use crate::utils::sap_file_utils::{get_tcode_file_path, save_sap_file};
use crate::utils::sap_tcode_utils::{assert_tcode, variant_select};
use crate::utils::sap_wnd_utils::check_export_window;
use crate::zmdetpc::{close_wnd1, paste_multi, press};
use anyhow::{anyhow, Result as AnyResult};
use sap_scripting::*;
use std::path::{Path, PathBuf};
use windows::core::Result;

const MATNR_BUTTON: &str = "wnd[0]/usr/btn%_MS_MATNR_%_APP_%-VALU_PUSH";
const WERKS_BUTTON: &str = "wnd[0]/usr/btn%_MS_WERKS_%_APP_%-VALU_PUSH";
/// Only exists on the selection screen (after execute still there = no list).
const SELECTION_MARKER: &str = "/usr/btn%_MS_MATNR_%_APP_%-VALU_PUSH";
const LOCAL_FILE_MENUS: &[&str] = &["wnd[0]/mbar/menu[0]/menu[3]/menu[2]"];
const SPREADSHEET_MENU: &str = "wnd[0]/mbar/menu[0]/menu[3]/menu[1]";

/// MM60 export parameters.
#[derive(Debug, Clone)]
pub struct MM60Params {
    pub variant: Option<String>,
    pub layout: Option<String>,
    /// `Some(0..=3)` = local file; `None` = Excel `.xlsx` (like LX03).
    pub export_type: Option<u8>,
    /// Materials to paste (already de-duplicated by the caller, see [`paste_list`]).
    pub materials: Vec<String>,
    pub ignore_display_error: bool,
    pub t_code: String,
}

impl Default for MM60Params {
    fn default() -> Self {
        Self {
            variant: None,
            layout: None,
            export_type: None,
            materials: Vec::new(),
            ignore_display_error: false,
            t_code: "MM60".to_string(),
        }
    }
}

// ----------------------------------------------------------------------------------------
// LX03 source: find, read, de-duplicate (pure / filesystem only, no SAP)
// ----------------------------------------------------------------------------------------

/// One data row of an LX03 export (`plant` is `None` when the export has no plant column).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LxRow {
    pub material: String,
    pub plant: Option<String>,
}

fn is_material_header(c: &str) -> bool {
    matches!(
        c.trim().to_ascii_lowercase().as_str(),
        "material" | "material_number" | "material number"
    )
}

fn is_plant_header(c: &str) -> bool {
    matches!(c.trim().to_ascii_lowercase().as_str(), "plnt" | "plant")
}

/// Rows of cells (text or xlsx) -> [`LxRow`]. The header is the first row that has a
/// `Material` cell; the plant column is optional; rows with a blank material are skipped.
pub fn rows_from_grid(grid: &[Vec<String>]) -> AnyResult<Vec<LxRow>> {
    let (h, mcol) = grid
        .iter()
        .enumerate()
        .find_map(|(i, r)| r.iter().position(|c| is_material_header(c)).map(|c| (i, c)))
        .ok_or_else(|| anyhow!("no 'Material' column found in the LX03 file"))?;
    let pcol = grid[h].iter().position(|c| is_plant_header(c));
    let cell = |r: &Vec<String>, c: usize| r.get(c).map(|v| v.trim().to_string());
    Ok(grid[h + 1..]
        .iter()
        .filter_map(|r| {
            let material = cell(r, mcol).filter(|m| !m.is_empty())?;
            let plant = pcol.and_then(|c| cell(r, c));
            Some(LxRow { material, plant })
        })
        .collect())
}

/// Parse a SAP "text with tabs" LX03 export (UTF-8 or Windows-1252).
pub fn parse_lx03_text(bytes: &[u8]) -> AnyResult<Vec<LxRow>> {
    let text = decode_export(bytes)?;
    let grid: Vec<Vec<String>> = text
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| l.split('\t').map(str::to_string).collect())
        .collect();
    rows_from_grid(&grid)
}

fn data_to_string(d: &calamine::Data) -> String {
    use calamine::Data;
    match d {
        Data::Float(f) if f.fract() == 0.0 => format!("{}", *f as i64),
        Data::Empty => String::new(),
        other => other.to_string(),
    }
}

/// Parse the first sheet of an `.xlsx` LX03 export.
pub fn read_lx03_xlsx(path: &Path) -> AnyResult<Vec<LxRow>> {
    use calamine::{open_workbook_auto, Reader};
    let mut wb =
        open_workbook_auto(path).map_err(|e| anyhow!("cannot open {}: {e}", path.display()))?;
    let range = wb
        .worksheet_range_at(0)
        .ok_or_else(|| anyhow!("{} has no sheets", path.display()))?
        .map_err(|e| anyhow!("cannot read {}: {e}", path.display()))?;
    let grid: Vec<Vec<String>> = range
        .rows()
        .map(|r| r.iter().map(data_to_string).collect())
        .collect();
    rows_from_grid(&grid)
}

/// Read an LX03 export by extension (`.xlsx`/`.xls`, or text with tabs).
pub fn read_lx03_file(path: &Path) -> AnyResult<Vec<LxRow>> {
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .map(str::to_ascii_lowercase)
        .unwrap_or_default();
    match ext.as_str() {
        "xlsx" | "xls" => read_lx03_xlsx(path),
        "txt" | "tsv" => {
            let bytes =
                std::fs::read(path).map_err(|e| anyhow!("cannot read {}: {e}", path.display()))?;
            parse_lx03_text(&bytes)
        }
        other => Err(anyhow!(
            "unsupported LX03 file type '.{other}' ({}); use .txt or .xlsx",
            path.display()
        )),
    }
}

/// Newest `.txt` / `.tsv` / `.xlsx` file directly in `dir` (by modified time).
pub fn newest_lx03_file(dir: &Path) -> Option<PathBuf> {
    std::fs::read_dir(dir)
        .ok()?
        .flatten()
        .filter(|e| e.path().is_file())
        .filter(|e| {
            e.path()
                .extension()
                .and_then(|x| x.to_str())
                .is_some_and(|x| matches!(x.to_ascii_lowercase().as_str(), "txt" | "tsv" | "xlsx"))
        })
        .filter_map(|e| Some((e.metadata().ok()?.modified().ok()?, e.path())))
        .max_by_key(|(t, _)| *t)
        .map(|(_, p)| p)
}

/// Where the LX03 list comes from: `source` (`--lx03-file`: a literal path, or a bare slug =
/// `<reports_dir>\<slug>`), else the LX03 export directory of this run
/// (`<reports_dir>\[<rname-dir>\]LX03`). Result is the newest usable file or the given path.
pub fn resolve_lx03_file(
    source: Option<&str>,
    reports_dir: &str,
    rname_dir: Option<&str>,
) -> AnyResult<PathBuf> {
    use crate::utils::cli_overrides::CliOverrides;
    use crate::utils::sap_file_utils::{collapse_backslashes, resolve_export_dir};
    let src = source.map(str::trim).filter(|s| !s.is_empty());
    if let Some(s) = src.filter(|s| CliOverrides::looks_like_path(s)) {
        let p = PathBuf::from(collapse_backslashes(s));
        if p.is_file() {
            return Ok(p);
        }
        if p.is_dir() {
            return newest_lx03_file(&p)
                .ok_or_else(|| anyhow!("no LX03 file (.txt/.xlsx) in {}", p.display()));
        }
        return Err(anyhow!("LX03 file not found: {}", p.display()));
    }
    let dir = match src {
        Some(slug) => PathBuf::from(collapse_backslashes(&format!(
            "{}\\{}",
            reports_dir.trim_end_matches('\\'),
            slug.to_lowercase()
        ))),
        None => PathBuf::from(resolve_export_dir(reports_dir, rname_dir, "LX03")),
    };
    newest_lx03_file(&dir).ok_or_else(|| {
        anyhow!(
            "no LX03 export (.txt/.xlsx) found in {}; run LX03 first or pass --lx03-file",
            dir.display()
        )
    })
}

/// De-duplicate by the pair (material, plant), first occurrence wins. Without a plant on the
/// rows (no plant column) nothing is removed: material alone is not a valid key.
pub fn dedupe_rows(rows: Vec<LxRow>) -> Vec<LxRow> {
    let mut seen = std::collections::HashSet::new();
    rows.into_iter()
        .filter(|r| match &r.plant {
            Some(p) => seen.insert((r.material.clone(), p.clone())),
            None => true,
        })
        .collect()
}

/// Materials to paste: the materials of the de-duplicated rows, in file order.
pub fn paste_list(rows: Vec<LxRow>) -> Vec<String> {
    dedupe_rows(rows).into_iter().map(|r| r.material).collect()
}

// ----------------------------------------------------------------------------------------
// SAP flow
// ----------------------------------------------------------------------------------------

fn fail(msg: &str) -> Result<ExportOutcome> {
    Ok(ExportOutcome::Error(format!("MM60: {msg}")))
}

/// Open the plant multi-selection, clear it (F4 = vkey 16) and close it again (vkey 8).
fn clear_multi(session: &GuiSession, button_id: &str) -> Result<bool> {
    if !press(session, button_id)? {
        return Ok(false);
    }
    if let Ok(w) = session.find_by_id("wnd[1]".to_string()) {
        if let Some(m) = w.downcast::<GuiModalWindow>() {
            m.send_v_key(16)?;
            m.send_v_key(8)?;
            return Ok(true);
        }
    }
    close_wnd1(session);
    Ok(false)
}

/// Grid of the "Select Layout" dialog (recorded live in MM60; same id as 149).
const LAYOUT_GRID: &str =
    "wnd[1]/usr/ssubD0500_SUBSCREEN:SAPLSLVC_DIALOG:0501/cntlG51_CONTAINER/shellcont/shell";

/// Row of `name` in the layout grid: `rows` holds (TEXT, VARIANT) per row. The description
/// (`TEXT`) is matched first, then the layout key (`VARIANT`); trimmed, case-insensitive.
pub fn find_layout_row(rows: &[(String, String)], name: &str) -> Option<usize> {
    let n = name.trim().to_lowercase();
    let by = |pick: fn(&(String, String)) -> &String| {
        rows.iter().position(|r| pick(r).trim().to_lowercase() == n)
    };
    by(|r| &r.0).or_else(|| by(|r| &r.1))
}

/// Ask for another layout name after `missing` was not found (same idea as LX03). Only when
/// [`can_prompt`]: unattended runs (and runs without a terminal) get `None` and fail instead.
/// `None` also when the user exits or enters nothing.
fn prompt_new_layout(missing: &str, available: &[String]) -> Option<String> {
    use dialoguer::{Input, Select};
    if !can_prompt() {
        return None;
    }
    println!("Layout '{missing}' not found. Available: {available:?}");
    let pick = Select::new()
        .with_prompt("What would you like to do?")
        .items(["Enter another layout name", "Exit layout selection"])
        .default(0)
        .interact()
        .unwrap_or(1);
    if pick != 0 {
        return None;
    }
    let name: String = Input::new()
        .with_prompt("Enter new layout name")
        .allow_empty(true)
        .interact_text()
        .unwrap_or_default();
    Some(name.trim().to_string()).filter(|n| !n.is_empty())
}

enum LayoutErr {
    /// Layout absent; carries the available descriptions.
    NotFound(Vec<String>),
    Other(String),
}

/// Layout selection, as recorded: `btn[33]` opens the grid in `wnd[1]`, then
/// `setCurrentCell(row, "TEXT")`, `selectedRows = row`, `clickCurrentCell`. A missing layout
/// closes the dialog and, when [`can_prompt`], asks for another name and retries; otherwise
/// `Err(message)` (outer `Result` = COM errors).
fn select_layout(session: &GuiSession, layout: &str) -> Result<std::result::Result<(), String>> {
    let mut layout = layout.to_string();
    loop {
        match try_select_layout(session, &layout)? {
            Ok(()) => return Ok(Ok(())),
            Err(LayoutErr::Other(m)) => return Ok(Err(m)),
            Err(LayoutErr::NotFound(available)) => match prompt_new_layout(&layout, &available) {
                Some(new) => layout = new,
                None => {
                    return Ok(Err(format!(
                        "layout '{layout}' not found (available: {available:?})"
                    )))
                }
            },
        }
    }
}

/// One attempt: open the dialog and pick the row, or close the dialog and say why not.
fn try_select_layout(
    session: &GuiSession,
    layout: &str,
) -> Result<std::result::Result<(), LayoutErr>> {
    if !press(session, "wnd[0]/tbar[1]/btn[33]")? {
        return Ok(Err(LayoutErr::Other(
            "layout button btn[33] not found".into(),
        )));
    }
    // wait for the dialog grid (up to ~5 s)
    let mut grid = None;
    for _ in 0..20 {
        grid = session
            .find_by_id(LAYOUT_GRID.to_string())
            .ok()
            .and_then(|g| g.downcast::<GuiGridView>());
        if grid.is_some() {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(250));
    }
    let Some(grid) = grid else {
        close_wnd1(session);
        return Ok(Err(LayoutErr::Other(
            "layout selection grid did not open".into(),
        )));
    };
    let n = grid.row_count()?;
    let cell = |i: i32, col: &str| grid.get_cell_value(i, col.to_string()).unwrap_or_default();
    let rows: Vec<(String, String)> = (0..n)
        .map(|i| (cell(i, "TEXT"), cell(i, "VARIANT")))
        .collect();
    let Some(row) = find_layout_row(&rows, layout) else {
        let names = rows.iter().map(|r| r.0.trim().to_string()).collect();
        close_wnd1(session);
        return Ok(Err(LayoutErr::NotFound(names)));
    };
    let row = row as i32;
    grid.set_current_cell(row, "TEXT".to_string())?;
    grid.set_selected_rows(row.to_string())?;
    grid.click_current_cell()?;
    Ok(Ok(()))
}

/// Run MM60 for `params.materials` (plant cleared) and export the list.
pub fn run_export_outcome(session: &GuiSession, params: &MM60Params) -> Result<ExportOutcome> {
    println!("Running MM60 export...");
    if params.materials.is_empty() {
        return fail("no materials to paste");
    }
    if !assert_tcode(session, "MM60", Some(0))? {
        return fail("could not start transaction MM60");
    }
    if let Some(v) = params.variant.as_deref().filter(|v| !v.trim().is_empty()) {
        if !variant_select(session, &params.t_code, v)? {
            return fail(&format!("variant '{v}' could not be selected"));
        }
    }
    if !paste_multi(session, "MM60", MATNR_BUTTON, &params.materials)? {
        return fail("could not paste the materials");
    }
    if !clear_multi(session, WERKS_BUTTON)? {
        return fail("could not clear the plant selection");
    }
    if !press(session, "wnd[0]/tbar[1]/btn[8]")? {
        return fail("execute button btn[8] not found");
    }
    let opts = ClassifyOptions {
        ignore_display_error: params.ignore_display_error,
    };
    if let Some(o) = classify_after_execute(session, "MM60", SELECTION_MARKER, opts)? {
        println!("MM60 outcome after execute: {o:?}");
        return Ok(o);
    }

    if let Some(layout) = params.layout.as_deref().filter(|l| !l.trim().is_empty()) {
        if let Err(m) = select_layout(session, layout)? {
            return fail(&m);
        }
        let bar = exist_ctrl(session, 0, "/sbar", true)?.ctext;
        if bar.to_lowercase().contains("no layouts found") {
            return fail(&format!("layout '{layout}': {bar}"));
        }
    }

    match params.export_type {
        Some(t) => {
            let (dir, file_name) = get_tcode_file_path("MM60", export_type_to_extension(t));
            let target = ExportTarget {
                export_type: t,
                dir,
                file_name,
            };
            export_to_target(session, "MM60", LOCAL_FILE_MENUS, &target)
        }
        None => export_xlsx(session),
    }
}

/// Spreadsheet (Excel) export, same as the macro: menu, pick the format popup, save `.xlsx`.
fn export_xlsx(session: &GuiSession) -> Result<ExportOutcome> {
    match session
        .find_by_id(SPREADSHEET_MENU.to_string())
        .ok()
        .and_then(|m| m.downcast::<GuiMenu>())
    {
        Some(menu) => menu.select()?,
        None => return fail("spreadsheet export menu not found"),
    }
    if !check_export_window(session, "MM60", "")? {
        return fail("export window check failed");
    }
    let (file_path, file_name) = get_tcode_file_path("MM60", "xlsx");
    let saved = save_sap_file(session, &file_path, &file_name, Some(true))?;
    Ok(ExportOutcome::from_saved(saved, &file_path, &file_name))
}

/// Boolean wrapper like the other tcodes; see [`run_export_outcome`].
pub fn run_export(session: &GuiSession, params: &MM60Params) -> Result<bool> {
    Ok(run_export_outcome(session, params)?.into_bool())
}

#[cfg(test)]
mod tests {
    use super::*;

    const LX: &[u8] = include_bytes!("../tests/fixtures/mm60/lx03.txt");

    #[test]
    fn parses_the_real_lx03_layout() {
        let rows = parse_lx03_text(LX).unwrap();
        assert_eq!(rows.len(), 21);
        assert_eq!(rows[0].material, "90000001");
        assert_eq!(rows[0].plant.as_deref(), Some("PL01"));
    }

    #[test]
    fn column_order_does_not_matter() {
        let grid = vec![
            vec!["Qty".into(), "Material".into(), "Plant".into()],
            vec!["1".into(), "A1".into(), "P1".into()],
            vec!["2".into(), "".into(), "P1".into()],
        ];
        let r = rows_from_grid(&grid).unwrap();
        assert_eq!(
            r,
            vec![LxRow {
                material: "A1".into(),
                plant: Some("P1".into())
            }]
        );
        assert!(rows_from_grid(&[vec!["x".into()]]).is_err());
    }

    fn row(m: &str, p: Option<&str>) -> LxRow {
        LxRow {
            material: m.into(),
            plant: p.map(str::to_string),
        }
    }

    #[test]
    fn dedupes_only_by_material_and_plant() {
        let rows = vec![
            row("M1", Some("P1")),
            row("M1", Some("P2")),
            row("M1", Some("P1")),
            row("M2", Some("P1")),
        ];
        assert_eq!(paste_list(rows), vec!["M1", "M1", "M2"]);
        // no plant column: nothing is removed
        let rows = vec![row("M1", None), row("M1", None)];
        assert_eq!(paste_list(rows), vec!["M1", "M1"]);
    }

    #[test]
    fn layout_row_by_text_then_key() {
        let rows = vec![
            ("Standard".to_string(), "/STD".to_string()),
            ("Value".to_string(), "/VALUE".to_string()),
        ];
        assert_eq!(find_layout_row(&rows, " value "), Some(1));
        assert_eq!(find_layout_row(&rows, "/std"), Some(0));
        assert_eq!(find_layout_row(&rows, "nope"), None);
    }

    #[test]
    fn newest_file_and_resolution() {
        let dir = std::env::temp_dir().join(format!("mm60_src_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let lx = dir.join("LX03");
        std::fs::create_dir_all(&lx).unwrap();
        assert!(newest_lx03_file(&lx).is_none());
        std::fs::write(lx.join("a.txt"), b"x").unwrap();
        std::thread::sleep(std::time::Duration::from_millis(50));
        std::fs::write(lx.join("b.txt"), b"x").unwrap();
        std::fs::write(lx.join("c.csv"), b"x").unwrap();
        let reports = dir.to_string_lossy().to_string();
        let got = resolve_lx03_file(None, &reports, None).unwrap();
        assert_eq!(got.file_name().unwrap(), "b.txt");
        // literal path wins
        let lit = lx.join("a.txt");
        let got = resolve_lx03_file(Some(&lit.to_string_lossy()), &reports, None).unwrap();
        assert_eq!(got, lit);
        // slug = <reports>\<slug>
        let got = resolve_lx03_file(Some("LX03"), &reports, None).unwrap();
        assert_eq!(got.file_name().unwrap(), "b.txt");
        assert!(resolve_lx03_file(Some("nothere"), &reports, None).is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
