#![allow(dead_code)] // headless export helpers are only used by src/abc (not by every target)

use sap_scripting::*;
use windows::core::Result;

use crate::utils::sap_ctrl_utils::exist_ctrl;
use crate::utils::sap_file_utils::{get_tcode_file_path, save_sap_file};

/// Map export_type 0..=4 to file extension
pub fn export_type_to_extension(export_type: u8) -> &'static str {
    match export_type {
        0 | 1 => "txt", // unconverted or text with tabs
        2 => "rtf",     // rich text format
        3 => "html",    // HTML format
        4 => "",        // clipboard - no file
        _ => "txt",     // default to txt for unknown values
    }
}

/// Fully resolved destination of a headless export: no config is read to build it.
/// `export_type` 0..=3 (a file is written; 4 = clipboard is not supported here), `dir` the
/// directory and `file_name` the complete file name (built by the caller, see
/// [`export_file_name`]).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExportTarget {
    pub export_type: u8,
    pub dir: String,
    pub file_name: String,
}

impl ExportTarget {
    pub fn full_path(&self) -> std::path::PathBuf {
        std::path::Path::new(&self.dir).join(&self.file_name)
    }
}

/// `<yyyymmddhhmmss>_<tcode>.<ext>` (same pattern as `get_tcode_file_path`, but with the time
/// passed in so no config/timezone lookup is needed).
pub fn export_file_name(tcode: &str, export_type: u8, now: chrono::NaiveDateTime) -> String {
    format!(
        "{}_{}.{}",
        now.format("%Y%m%d%H%M%S"),
        tcode,
        export_type_to_extension(export_type)
    )
}

/// Open the "SAVE LIST IN FILE..." dialog through the first menu path in `candidates` that
/// exists and opens a modal window titled "save list in file" (case-insensitive). A modal
/// window with another title is closed again and the next candidate is tried. Returns false
/// when none worked.
pub fn open_local_file_dialog(session: &GuiSession, candidates: &[&str]) -> bool {
    for path in candidates {
        let Ok(menu) = session.find_by_id((*path).to_string()) else {
            println!("Export menu {path}: not found on the current screen");
            continue;
        };
        let Some(item) = menu.downcast::<GuiMenu>() else {
            println!("Export menu {path}: found but is not a menu item");
            continue;
        };
        if let Err(e) = item.select() {
            println!("Export menu {path}: select failed: {e}");
            continue;
        }
        // the dialog can take a moment to open (heavier reports): wait up to ~3 s
        let mut w = None;
        for _ in 0..15 {
            if let Ok(c) = exist_ctrl(session, 1, "", true) {
                if c.cband {
                    w = Some(c);
                    break;
                }
            }
            std::thread::sleep(std::time::Duration::from_millis(200));
        }
        let Some(w) = w else {
            println!("Export menu {path}: selected, but no dialog (wnd[1]) opened");
            continue;
        };
        if dialog_title_is_save_list(&w.ctext) {
            return true;
        }
        println!(
            "Menu {path} opened a dialog titled '{}', not the local file export; closing it",
            w.ctext
        );
        close_dialog_wnd1(session);
    }
    false
}

/// Close `wnd[1]` and make sure it is gone (close, then F12, polling up to ~1 s each), so a
/// stale dialog cannot be mistaken for the next candidate's dialog.
fn close_dialog_wnd1(session: &GuiSession) {
    let gone = || {
        !exist_ctrl(session, 1, "", true)
            .map(|c| c.cband)
            .unwrap_or(false)
    };
    let wait_gone = || {
        for _ in 0..5 {
            if gone() {
                return true;
            }
            std::thread::sleep(std::time::Duration::from_millis(200));
        }
        gone()
    };
    if let Ok(win) = session.find_by_id("wnd[1]".to_string()) {
        if let Some(f) = win.downcast::<GuiFrameWindow>() {
            let _ = f.close();
        }
    }
    if wait_gone() {
        return;
    }
    if let Ok(win) = session.find_by_id("wnd[1]".to_string()) {
        if let Some(m) = win.downcast::<GuiModalWindow>() {
            let _ = m.send_v_key(12);
        }
    }
    if !wait_gone() {
        println!("Warning: dialog wnd[1] could not be closed");
    }
}

/// True for the "Save list in file..." dialog title (case-insensitive).
pub fn dialog_title_is_save_list(title: &str) -> bool {
    title.to_lowercase().contains("save list in file")
}

/// Wait until `path` exists and is not empty (SAP writes the file asynchronously).
pub fn wait_for_file(path: &std::path::Path, timeout: std::time::Duration) -> bool {
    let start = std::time::Instant::now();
    loop {
        if std::fs::metadata(path)
            .map(|m| m.len() > 0)
            .unwrap_or(false)
        {
            return true;
        }
        if start.elapsed() >= timeout {
            return false;
        }
        std::thread::sleep(std::time::Duration::from_millis(200));
    }
}

/// Headless local-file export: open the dialog via `menu_candidates`, choose the export type,
/// save to `target`, and check that the file really exists. Reads no config.
pub fn export_to_target(
    session: &GuiSession,
    tcode: &str,
    menu_candidates: &[&str],
    target: &ExportTarget,
) -> Result<crate::utils::export_outcome::ExportOutcome> {
    use crate::utils::export_outcome::ExportOutcome;
    if target.export_type > 3 {
        return Ok(ExportOutcome::Error(format!(
            "{tcode}: headless export needs a file export type (0-3), got {}",
            target.export_type
        )));
    }
    if let Err(e) = std::fs::create_dir_all(&target.dir) {
        return Ok(ExportOutcome::Error(format!(
            "{tcode}: cannot create export dir {}: {e}",
            target.dir
        )));
    }
    if !open_local_file_dialog(session, menu_candidates) {
        return Ok(ExportOutcome::Error(format!(
            "{tcode}: local file export dialog did not open (tried {})",
            menu_candidates.join(", ")
        )));
    }
    let (dir, name) = (target.dir.clone(), target.file_name.clone());
    if let Err(e) = export_local_file_impl(session, target.export_type, &|_| {
        (dir.clone(), name.clone())
    }) {
        return Ok(ExportOutcome::Error(format!("{tcode}: export failed: {e}")));
    }
    let path = target.full_path();
    if !wait_for_file(&path, std::time::Duration::from_secs(15)) {
        return Ok(ExportOutcome::Error(format!(
            "{tcode}: export file {} was not written",
            path.display()
        )));
    }
    Ok(ExportOutcome::File(path))
}

/// Perform a "Save list in file" export for the active ALV grid.
/// Assumes the "SAVE LIST IN FILE..." dialog (wnd[1]) is already open.
/// Selects the radio option based on export_type, confirms, and saves to disk unless clipboard.
///
/// Returns the full path of the written file, or an empty string for clipboard export.
pub fn export_local_file(
    session: &GuiSession,
    tcode_for_path: &str,
    export_type: u8,
    filename_suffix: Option<&str>,
) -> Result<String> {
    export_local_file_impl(session, export_type, &|file_extension| {
        // Build base path and name
        let (file_path, base_file_name) = get_tcode_file_path(tcode_for_path, file_extension);
        // Compose filename, optionally appending suffix before extension
        let file_name = if let Some(suffix) = filename_suffix {
            if file_extension.is_empty() {
                format!("{base_file_name}-{suffix}")
            } else if let Some(stripped) =
                base_file_name.strip_suffix(&format!(".{file_extension}"))
            {
                format!("{stripped}-{suffix}.{file_extension}")
            } else {
                format!("{base_file_name}-{suffix}.{file_extension}")
            }
        } else {
            base_file_name
        };
        (file_path, file_name)
    })
}

/// Shared implementation: `name_for(extension)` returns `(directory, file name)`.
fn export_local_file_impl(
    session: &GuiSession,
    export_type: u8,
    name_for: &dyn Fn(&str) -> (String, String),
) -> Result<String> {
    // Select export format based on export_type (0..=4)
    let radio_index = match export_type {
        0 => "0", // unconverted
        1 => "1", // text with tabs (default)
        2 => "2", // rich text format
        3 => "3", // HTML format
        4 => "4", // clipboard
        _ => "1", // default to text with tabs for unknown values
    };

    let radio_id = format!(
        "wnd[1]/usr/subSUBSCREEN_STEPLOOP:SAPLSPO5:0150/sub:SAPLSPO5:0150/radSPOPLI-SELFLAG[{radio_index},0]"
    );

    if let Ok(radio) = session.find_by_id(radio_id) {
        if let Some(radio_button) = radio.downcast::<GuiRadioButton>() {
            radio_button.select()?;
            radio_button.set_focus()?;
        }
    }

    // Press Enter to confirm selection in dlg
    if let Ok(tbar) = session.find_by_id("wnd[1]/tbar[0]/btn[0]".to_string()) {
        if let Some(button) = tbar.downcast::<GuiButton>() {
            button.press()?;
        }
    }

    // Determine extension
    let file_extension = export_type_to_extension(export_type);

    // For clipboard export, don't create a file
    if export_type == 4 {
        println!("Exporting to clipboard - no file created");
        return Ok(String::new());
    }

    // Wait for save dialog to appear
    let _ = exist_ctrl(session, 1, "", true)?; // best-effort check

    let (file_path, file_name) = name_for(file_extension);

    // Save via utility
    save_sap_file(session, &file_path, &file_name, Some(false))?;

    // Small delay for export to finish
    std::thread::sleep(std::time::Duration::from_millis(2000));

    let full_path = format!("{}{}{}", file_path, std::path::MAIN_SEPARATOR, file_name);
    println!("Successfully exported data to {full_path}");

    Ok(full_path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::NaiveDate;

    #[test]
    fn file_name_pattern() {
        let t = NaiveDate::from_ymd_opt(2026, 11, 10)
            .unwrap()
            .and_hms_opt(10, 15, 50)
            .unwrap();
        assert_eq!(export_file_name("VT11", 1, t), "20261110101550_VT11.txt");
        assert_eq!(export_file_name("LT23", 3, t), "20261110101550_LT23.html");
        let target = ExportTarget {
            export_type: 1,
            dir: "out".into(),
            file_name: export_file_name("VT11", 1, t),
        };
        assert_eq!(
            target.full_path(),
            std::path::Path::new("out").join("20261110101550_VT11.txt")
        );
    }

    #[test]
    fn save_list_title() {
        assert!(dialog_title_is_save_list("SAVE LIST IN FILE..."));
        assert!(dialog_title_is_save_list("Save list in file..."));
        assert!(!dialog_title_is_save_list("Select Spreadsheet"));
    }

    #[test]
    fn wait_for_file_checks_existence_and_size() {
        let dir = std::env::temp_dir().join(format!("sap_export_wait_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let p = dir.join("x.txt");
        assert!(!wait_for_file(&p, std::time::Duration::from_millis(50)));
        std::fs::write(&p, b"").unwrap();
        assert!(!wait_for_file(&p, std::time::Duration::from_millis(50)));
        std::fs::write(&p, b"a").unwrap();
        assert!(wait_for_file(&p, std::time::Duration::from_millis(50)));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
