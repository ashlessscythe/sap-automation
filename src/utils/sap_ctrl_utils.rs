//! SAP control probes; large API used from different T-code modules and tests.
#![allow(dead_code)]

use com_shim::HasIDispatch;
use sap_scripting::*;
use windows::core::Result;

/// Check if a control exists in the SAP GUI
///
/// This function checks if a control exists in the SAP GUI at the specified window index
/// and with the specified ID suffix.
pub fn exist_ctrl(
    session: &GuiSession,
    wnd_idx: i32,
    id_suffix: &str,
    silent: bool,
) -> Result<CtrlBand> {
    let wnd_id = format!("wnd[{wnd_idx}]");
    let full_id = format!("{wnd_id}{id_suffix}");
    let full_id_for_log = full_id.clone();

    let ctrl_result = session.find_by_id(full_id);
    let cband = ctrl_result.is_ok();

    // Initialize default values for ctext and ctype
    let mut ctext = String::new();
    let mut ctype = String::new();

    // If control exists, try to get its text and type
    if cband {
        if let Ok(component) = ctrl_result {
            // Try to get component type using the component's name
            if let Ok(name) = component.name() {
                ctype = name;
            }

            // Try to get text based on component type
            if let Some(window) = component.downcast::<GuiFrameWindow>() {
                ctext = window.text().unwrap_or_default();
            } else if let Some(window) = component.downcast::<GuiMainWindow>() {
                ctext = window.text().unwrap_or_default();
            } else if let Some(window) = component.downcast::<GuiModalWindow>() {
                ctext = window.text().unwrap_or_default();
            } else if let Some(label) = component.downcast::<GuiLabel>() {
                ctext = label.text().unwrap_or_default();
            } else if let Some(text_field) = component.downcast::<GuiTextField>() {
                ctext = text_field.text().unwrap_or_default();
            } else if let Some(text_field) = component.downcast::<GuiCTextField>() {
                ctext = text_field.text().unwrap_or_default();
            } else if let Some(statusbar) = component.downcast::<GuiStatusbar>() {
                ctext = statusbar.text().unwrap_or_default();
            }
        }
    }

    if !cband && !silent {
        println!("Control not found: {full_id_for_log}");
    }

    Ok(CtrlBand {
        cband,
        ctext,
        ctype,
    })
}

/// Struct to hold the result of exist_ctrl
#[derive(Debug)]
pub struct CtrlBand {
    pub cband: bool,
    pub ctext: String,
    pub ctype: String,
}

/// Get text from SAP GUI controls
///
/// This function gets text from SAP GUI controls at the specified window index
/// and with the specified ID suffix.
pub fn hit_ctrl(
    session: &GuiSession,
    wnd_idx: i32,
    id_suffix: &str,
    prop: &str,
    action: &str,
    value: &str,
) -> Result<String> {
    let wnd_id = format!("wnd[{wnd_idx}]");
    let full_id = format!("{wnd_id}{id_suffix}");
    let full_id_for_log = full_id.clone();

    let ctrl_result = session.find_by_id(full_id);
    match ctrl_result {
        Ok(ctrl) => {
            if action == "Get" {
                if prop == "Text" {
                    if let Some(text_field) = ctrl.downcast::<GuiTextField>() {
                        text_field.text()
                    } else if let Some(text_field) = ctrl.downcast::<GuiCTextField>() {
                        text_field.text()
                    } else if let Some(label) = ctrl.downcast::<GuiLabel>() {
                        label.text()
                    } else if let Some(statusbar) = ctrl.downcast::<GuiStatusbar>() {
                        statusbar.text()
                    } else {
                        Ok("".to_string())
                    }
                } else {
                    Ok("".to_string())
                }
            } else if action == "Set" {
                if prop == "Text" {
                    if let Some(text_field) = ctrl.downcast::<GuiTextField>() {
                        text_field.set_text(value.to_string())?;
                    } else if let Some(text_field) = ctrl.downcast::<GuiCTextField>() {
                        text_field.set_text(value.to_string())?;
                    }
                }
                Ok("".to_string())
            } else {
                Ok("".to_string())
            }
        }
        Err(_) => {
            println!("Control not found: {full_id_for_log}");
            Ok("".to_string())
        }
    }
}

/// Get text from SAP GUI error messages
///
/// This function gets text from SAP GUI error messages at the specified window index
/// and with the specified ID suffix.
pub fn get_sap_text_errors(
    session: &GuiSession,
    wnd_idx: i32,
    id_suffix: &str,
    max_lines: i32,
    prefix: Option<&str>,
) -> Result<String> {
    let mut result = String::new();
    let prefix_str = prefix.unwrap_or("");

    for i in 1..=max_lines {
        let id = format!("{id_suffix}{i}");
        let text = hit_ctrl(session, wnd_idx, &id, "Text", "Get", "")?;
        if !text.is_empty() {
            if !result.is_empty() {
                result.push('\n');
            }
            result.push_str(&format!("{prefix_str}{text}"));
        }
    }

    Ok(result)
}

/// Cell type prefix for SAP multi-value paste tables (`ctxt` vs `txt`).
/// VL06O/VT11 use `ctxtRSCSEL_255-SLOW_I`; y_dn3_47000149 delivery multi uses `txtRSCSEL_255-SLOW_I`.
fn multi_paste_cell_id(table_id: &str, cell_prefix: &str, row: i32) -> String {
    format!("{table_id}/{cell_prefix}RSCSEL_255-SLOW_I[1,{row}]")
}

fn read_multi_paste_cell(session: &GuiSession, full_field_id: &str) -> Result<Option<String>> {
    let Ok(field) = session.find_by_id(full_field_id.to_string()) else {
        return Ok(None);
    };
    if let Some(text_field) = field.downcast::<GuiCTextField>() {
        return Ok(Some(text_field.text()?));
    }
    if let Some(text_field) = field.downcast::<GuiTextField>() {
        return Ok(Some(text_field.text()?));
    }
    Ok(None)
}

fn write_multi_paste_cell(session: &GuiSession, full_field_id: &str, value: &str) -> Result<bool> {
    let Ok(field) = session.find_by_id(full_field_id.to_string()) else {
        return Ok(false);
    };
    if let Some(text_field) = field.downcast::<GuiCTextField>() {
        text_field.set_text(value.to_string())?;
        return Ok(true);
    }
    if let Some(text_field) = field.downcast::<GuiTextField>() {
        text_field.set_text(value.to_string())?;
        return Ok(true);
    }
    Ok(false)
}

/// Paste values into a scrollable table in SAP GUI
///
/// This function pastes values into a scrollable table in SAP GUI at the specified window index.
/// It handles scrolling through the table to paste all values, even when there are thousands.
/// This is a faithful port of the VBA implementation from deliv_packages.md
///
/// `cell_prefix` is typically `"ctxt"` (VL06O/VT11) or `"txt"` (149 delivery multi).
fn paste_values_by_rows(
    session: &GuiSession,
    wnd_idx: i32,
    table_id: &str,
    values: &[String],
    batch_size: usize,
    cell_prefix: &str,
) -> Result<bool> {
    if values.is_empty() {
        return Ok(true);
    }

    let full_table_id = format!("wnd[{wnd_idx}]/usr/{table_id}");

    // Check if table exists
    let table_exists = exist_ctrl(session, wnd_idx, &format!("/usr/{table_id}"), true)?;
    if !table_exists.cband {
        println!("Table not found: {full_table_id}");
        return Ok(false);
    }

    // Clean the values to ensure no trailing commas
    let clean_values: Vec<String> = values
        .iter()
        .map(|v| v.trim().trim_end_matches(',').to_string())
        .collect();

    println!(
        "Starting to paste {} values using VBA-style scrolling (cell_prefix={})",
        clean_values.len(),
        cell_prefix
    );

    // Process values in batches, similar to VBA approach
    let mut values_pasted = 0;
    let mut scroll_position = 0;

    while values_pasted < clean_values.len() {
        // Set scrollbar position to show the current batch
        // Based on VBA code, we increment by 7 for each batch
        if values_pasted > 0 {
            scroll_position += 7;
            // Try to scroll down using Page Down (more reliable)
            if let Ok(window) = session.find_by_id(format!("wnd[{wnd_idx}]")) {
                if let Some(wnd) = window.downcast::<GuiModalWindow>() {
                    // Send Page Down keys until we find an empty row at index 1
                    let mut page_down_count = 0;
                    loop {
                        wnd.send_v_key(82)?; // Page Down key
                        page_down_count += 1;
                        std::thread::sleep(std::time::Duration::from_millis(50));
                        // Check if row 1 is empty (indicating we've scrolled to a new area)
                        let check_field_id = multi_paste_cell_id(table_id, cell_prefix, 1);
                        let check_full_id = format!("wnd[{wnd_idx}]/usr/{check_field_id}");
                        if let Some(text) = read_multi_paste_cell(session, &check_full_id)? {
                            if text.is_empty() {
                                println!(
                                    "Found empty row at index 1 after {page_down_count} Page Down presses"
                                );
                                break;
                            }
                        }
                        // Safety limit to prevent infinite loop
                        if page_down_count >= 5 {
                            println!("Reached safety limit of 5 Page Down presses");
                            break;
                        }
                    }
                }
            }
        }
        // Calculate how many values we can paste in this batch
        let remaining_values = clean_values.len() - values_pasted;
        let current_batch_size = std::cmp::min(batch_size, remaining_values);
        println!(
            "Pasting batch: positions {} to {} (scroll_pos: {})",
            values_pasted,
            values_pasted + current_batch_size - 1,
            scroll_position
        );
        // Determine correct start index after scrolling
        let mut start_index = 0;
        if values_pasted > 0 {
            let field_id = multi_paste_cell_id(table_id, cell_prefix, 0);
            let full_field_id = format!("wnd[{wnd_idx}]/usr/{field_id}");
            if let Some(text) = read_multi_paste_cell(session, &full_field_id)? {
                if !text.trim().is_empty() {
                    start_index = 1;
                }
            }
        }
        let mut local_index = start_index;
        for i in 0..current_batch_size {
            let global_index = values_pasted + i;
            let field_id = multi_paste_cell_id(table_id, cell_prefix, local_index as i32);
            let full_field_id = format!("wnd[{wnd_idx}]/usr/{field_id}");
            let clean_value = clean_values[global_index].clone();
            if write_multi_paste_cell(session, &full_field_id, &clean_value)? {
                println!("  Pasted value {clean_value} at local index {local_index}");
                local_index += 1;
            } else {
                println!("  Field not found or not a text field: {full_field_id}");
                break;
            }
        }
        // Update counters
        values_pasted += local_index - start_index;
        // If we couldn't paste any values in this batch, we might be at the end
        if local_index == start_index {
            println!("Could not paste any values in this batch, stopping");
            break;
        }
        // Ensure we don't exceed the total number of values
        if values_pasted > clean_values.len() {
            values_pasted = clean_values.len();
        }
        println!(
            "Pasted {} values so far ({} remaining)",
            values_pasted,
            clean_values.len() - values_pasted
        );
        // If we've pasted all values, we're done
        if values_pasted >= clean_values.len() {
            break;
        }
    }

    println!(
        "Total values pasted: {}/{}",
        values_pasted,
        clean_values.len()
    );

    // Return true if we pasted at least some values
    Ok(values_pasted > 0)
}

/// What kind of values a multi-selection holds; decides how they are cleaned for the clipboard.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PasteKind {
    /// Free text / alphanumeric ids: trimmed only.
    Text,
    /// Purely numeric ids (e.g. transfer order numbers): every non-digit is removed.
    Numeric,
}

/// Clean `values` for a multi-selection: trim, drop trailing commas and line breaks, remove
/// everything but digits for [`PasteKind::Numeric`], and drop what ends up empty. Order and
/// duplicates are kept (the callers de-duplicate where it matters).
pub fn clean_paste_values(values: &[String], kind: PasteKind) -> Vec<String> {
    values
        .iter()
        .map(|v| {
            let t = v.trim().trim_end_matches(',').trim();
            match kind {
                PasteKind::Text => t.replace(['\r', '\n'], ""),
                PasteKind::Numeric => t.chars().filter(|c| c.is_ascii_digit()).collect(),
            }
        })
        .filter(|v: &String| !v.is_empty())
        .collect()
}

/// Do a pasted value and the one SAP shows denote the same entry (SAP may pad leading zeros
/// through a conversion exit, or change the case)?
fn same_entry(shown: &str, expected: &str) -> bool {
    use crate::utils::export_outcome::normalize_value;
    normalize_value(shown).eq_ignore_ascii_case(&normalize_value(expected))
}

/// Text of row `row` of the multi-selection table (tries both cell kinds).
fn multi_row_text(session: &GuiSession, wnd_idx: i32, table_id: &str, row: i32) -> Option<String> {
    for prefix in ["ctxt", "txt"] {
        let id = format!(
            "wnd[{wnd_idx}]/usr/{}",
            multi_paste_cell_id(table_id, prefix, row)
        );
        if let Ok(Some(t)) = read_multi_paste_cell(session, &id) {
            return Some(t);
        }
    }
    None
}

/// Scroll the multi-selection table back to its first row (its own vertical scrollbar).
fn scroll_multi_to_top(session: &GuiSession, wnd_idx: i32, table_id: &str) {
    let table = session
        .find_by_id(format!("wnd[{wnd_idx}]/usr/{table_id}"))
        .ok()
        .and_then(|c| c.downcast::<GuiTableControl>());
    if let Some(sb) = table
        .and_then(|t| t.vertical_scrollbar().ok())
        .map(|c| GuiScrollbar::from(c.get_idispatch().clone()))
    {
        if sb.position().map(|p| p != 0).unwrap_or(false) {
            let _ = sb.set_position(0);
            std::thread::sleep(std::time::Duration::from_millis(150));
        }
    }
}

/// What the first row of the open multi-selection holds against what was meant to be pasted:
/// `Some((matches, shown))`, `None` when the row cannot be read. Catches a paste that put some
/// other list in the table (a stale clipboard, leftover entries of a variant).
pub fn first_entry_check(
    session: &GuiSession,
    wnd_idx: i32,
    table_id: &str,
    expected: &str,
) -> Option<(bool, String)> {
    // after a paste the table may be scrolled to its end: look at the top
    scroll_multi_to_top(session, wnd_idx, table_id);
    let shown = multi_row_text(session, wnd_idx, table_id, 0)?;
    Some((same_entry(&shown, expected), shown))
}

/// Check that a clipboard upload really landed: the first entry is in row 0, the table holds at
/// least as many rows as entries and, after scrolling to its end, the last entry is the last
/// filled row. The table is scrolled back to the top afterwards. Not every row is compared.
fn verify_clipboard_paste(
    session: &GuiSession,
    wnd_idx: i32,
    table_id: &str,
    lines: &[String],
) -> bool {
    let (Some(first), Some(last)) = (lines.first(), lines.last()) else {
        return true;
    };
    match multi_row_text(session, wnd_idx, table_id, 0) {
        Some(t) if same_entry(&t, first) => {}
        other => {
            println!("Clipboard paste check failed: row 0 is {other:?}, expected '{first}'");
            return false;
        }
    }
    let table = session
        .find_by_id(format!("wnd[{wnd_idx}]/usr/{table_id}"))
        .ok()
        .and_then(|c| c.downcast::<GuiTableControl>());
    let Some(table) = table else {
        println!(
            "Clipboard paste check: table control not readable, only the first entry was verified"
        );
        return lines.len() == 1;
    };
    if let Ok(rows) = table.row_count() {
        if (rows as usize) < lines.len() {
            println!(
                "Clipboard paste check failed: table has {rows} rows, expected at least {}",
                lines.len()
            );
            return false;
        }
    }
    if lines.len() == 1 {
        return true;
    }
    // A GuiScrollbar has no `Type` property, so `downcast` can't be used: wrap the COM object of
    // the table's own `VerticalScrollbar` property (no object name involved, works for any table).
    let scrollbar = table
        .vertical_scrollbar()
        .ok()
        .map(|c| GuiScrollbar::from(c.get_idispatch().clone()));
    let Some(scrollbar) = scrollbar else {
        println!("Clipboard paste check: no scrollbar, last entry not verified");
        return false;
    };
    let mut ok = false;
    if let Ok(max) = scrollbar.maximum() {
        let _ = scrollbar.set_position(max);
        std::thread::sleep(std::time::Duration::from_millis(150));
        let visible = table.visible_row_count().unwrap_or(8).max(1);
        let shown_last = (0..visible)
            .filter_map(|r| multi_row_text(session, wnd_idx, table_id, r))
            .rfind(|t| !t.trim().is_empty());
        match shown_last {
            Some(t) if same_entry(&t, last) => ok = true,
            other => println!(
                "Clipboard paste check failed: last filled row is {other:?}, expected '{last}'"
            ),
        }
        let _ = scrollbar.set_position(0);
    }
    ok
}

/// Fill the open multi-selection popup from the Windows clipboard: put `values` on the clipboard
/// (one per line), send Shift+F12 (VKey 24, "upload from clipboard") to the popup and verify the
/// result with [`verify_clipboard_paste`]. The key goes to the popup window, so this is the same
/// for every tcode / select-option. Ok(false) = nothing usable pasted; the table may then hold a
/// partial result and the caller must clear it before another attempt.
pub fn paste_values_via_clipboard(
    session: &GuiSession,
    wnd_idx: i32,
    table_id: &str,
    values: &[String],
    kind: PasteKind,
) -> Result<bool> {
    let lines = clean_paste_values(values, kind);
    if lines.is_empty() {
        println!("Clipboard paste: no usable values after cleaning");
        return Ok(false);
    }
    if !exist_ctrl(session, wnd_idx, &format!("/usr/{table_id}"), true)?.cband {
        println!("Table not found: wnd[{wnd_idx}]/usr/{table_id}");
        return Ok(false);
    }
    let text = lines.join("\r\n");
    if let Err(e) = clipboard_win::set_clipboard_string(&text) {
        println!("Clipboard paste: could not write the clipboard: {e}");
        return Ok(false);
    }
    // another program may have grabbed the clipboard in the meantime
    if clipboard_win::get_clipboard_string().ok().as_deref() != Some(text.as_str()) {
        println!("Clipboard paste: clipboard content changed, not pasting");
        return Ok(false);
    }
    println!(
        "Pasting {} values from the clipboard (Shift+F12)",
        lines.len()
    );
    let wnd = session.find_by_id(format!("wnd[{wnd_idx}]"))?;
    if let Some(m) = wnd.downcast::<GuiModalWindow>() {
        m.send_v_key(24)?;
    } else if let Some(f) = wnd.downcast::<GuiFrameWindow>() {
        f.send_v_key(24)?;
    } else {
        println!("Clipboard paste: wnd[{wnd_idx}] is not a window");
        return Ok(false);
    }
    std::thread::sleep(std::time::Duration::from_millis(300));
    if let Ok(popup) = session.find_by_id(format!("wnd[{}]", wnd_idx + 1)) {
        let title = popup
            .downcast::<GuiModalWindow>()
            .and_then(|w| w.text().ok())
            .unwrap_or_default();
        println!("Clipboard paste: a popup opened above the multi-selection ('{title}')");
    }
    Ok(verify_clipboard_paste(session, wnd_idx, table_id, &lines))
}

/// Paste values into the open multi-selection table (see [`paste_values_with_scroll_kind`]),
/// values treated as plain text.
pub fn paste_values_with_scroll(
    session: &GuiSession,
    wnd_idx: i32,
    table_id: &str,
    values: &[String],
    batch_size: usize,
    cell_prefix: &str,
) -> Result<bool> {
    paste_values_with_scroll_kind(
        session,
        wnd_idx,
        table_id,
        values,
        batch_size,
        cell_prefix,
        PasteKind::Text,
    )
}

/// Paste values into the open multi-selection table. With `--use-clipboard` the clipboard upload
/// is tried first (verified; on failure the table is cleared and the row-by-row entry runs);
/// otherwise, or as the fallback, the values are typed row by row with Page Down.
///
/// `kind` only matters with `--use-clipboard`: [`PasteKind::Numeric`] strips every non-digit from
/// the values (transfer orders). Without the flag the legacy behaviour is unchanged.
pub fn paste_values_with_scroll_kind(
    session: &GuiSession,
    wnd_idx: i32,
    table_id: &str,
    values: &[String],
    batch_size: usize,
    cell_prefix: &str,
    kind: PasteKind,
) -> Result<bool> {
    if !crate::utils::cli_overrides::use_clipboard() {
        return paste_values_by_rows(session, wnd_idx, table_id, values, batch_size, cell_prefix);
    }
    match paste_values_via_clipboard(session, wnd_idx, table_id, values, kind) {
        Ok(true) => return Ok(true),
        Ok(false) => println!("Clipboard paste did not take, falling back to row-by-row entry"),
        Err(e) => println!("Clipboard paste failed ({e}), falling back to row-by-row entry"),
    }
    // drop whatever the failed upload left behind (Shift+F4, "delete entire selection")
    if let Ok(w) = session.find_by_id(format!("wnd[{wnd_idx}]")) {
        if let Some(m) = w.downcast::<GuiModalWindow>() {
            let _ = m.send_v_key(16);
        }
    }
    let cleaned = clean_paste_values(values, kind);
    paste_values_by_rows(
        session,
        wnd_idx,
        table_id,
        &cleaned,
        batch_size,
        cell_prefix,
    )
}

/// Get the current vertical scrollbar position of a table
///
/// This function gets the current vertical scrollbar position of a table
/// using the same approach as the VBA implementation
pub fn get_scrollbar_position(session: &GuiSession, wnd_idx: i32, table_id: &str) -> Result<i32> {
    let scroll_result = hit_ctrl(
        session,
        wnd_idx,
        &format!("/usr/{table_id}"),
        "Position",
        "GetV",
        "",
    );

    match scroll_result {
        Ok(position_str) => {
            let position = position_str.parse::<i32>().unwrap_or(0);
            println!("Current scrollbar position: {position}");
            Ok(position)
        }
        Err(e) => {
            println!("Failed to get scrollbar position: {e}");
            Ok(0)
        }
    }
}

#[cfg(test)]
mod clipboard_tests {
    use super::*;

    fn v(a: &[&str]) -> Vec<String> {
        a.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn text_values_are_trimmed_and_blank_free() {
        assert_eq!(
            clean_paste_values(&v(&[" 123, ", "", "AB-1\r\n", "  "]), PasteKind::Text),
            v(&["123", "AB-1"])
        );
    }

    #[test]
    fn numeric_values_keep_digits_only() {
        assert_eq!(
            clean_paste_values(
                &v(&["TO 0012", "34,", "x", "5-6", "\u{a0}78\t"]),
                PasteKind::Numeric
            ),
            v(&["0012", "34", "56", "78"])
        );
    }

    #[test]
    fn same_entry_ignores_zero_padding_and_case() {
        assert!(same_entry("0000123", "123"));
        assert!(same_entry("ab1", "AB1"));
        assert!(!same_entry("124", "123"));
    }
}
