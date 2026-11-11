//! Typed result of an SAP export plus shared helpers used by the headless runner
//! (phase 4.0 of `docs/private/abc_headless_plan.md`): outcome classifier (P6) and the
//! value-list preparation used before every multi-value paste (P3).
#![allow(dead_code)]

use std::path::PathBuf;

use sap_scripting::*;
use windows::core::Result;

use crate::utils::sap_ctrl_utils::exist_ctrl;

/// What an export attempt produced.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExportOutcome {
    /// Export succeeded; path of the file that was written (empty for clipboard exports).
    File(PathBuf),
    /// SAP reported that the selection returned nothing (not an error).
    NoData(String),
    /// The export failed.
    Error(String),
}

impl ExportOutcome {
    pub fn is_file(&self) -> bool {
        matches!(self, ExportOutcome::File(_))
    }

    pub fn path(&self) -> Option<&PathBuf> {
        match self {
            ExportOutcome::File(p) => Some(p),
            _ => None,
        }
    }

    /// Legacy boolean view: only a written file counts as success.
    pub fn into_bool(self) -> bool {
        self.is_file()
    }

    /// Build an outcome from the legacy `save_sap_file` / `export_local_file` results.
    pub fn from_saved(saved: bool, dir: &str, file_name: &str) -> Self {
        if saved {
            ExportOutcome::File(PathBuf::from(dir).join(file_name))
        } else {
            ExportOutcome::Error("save_sap_file reported failure".to_string())
        }
    }

    /// Build an outcome from the string returned by `export_local_file`.
    pub fn from_exported_path(path: &str) -> Self {
        ExportOutcome::File(PathBuf::from(path))
    }
}

/// Options for [`classify_messages`].
#[derive(Debug, Clone, Copy, Default)]
pub struct ClassifyOptions {
    /// `ignore_display_error`: treat "no display authorization" as harmless.
    pub ignore_display_error: bool,
}

fn contains_ci(haystack: &str, needle: &str) -> bool {
    haystack.to_lowercase().contains(&needle.to_lowercase())
}

/// Pure classifier (P6). `status` is the `wnd[0]/sbar` text, `popup` the text of a
/// `wnd[1]` information popup, `label` the text of `wnd[0]/usr/lbl[2,4]`.
/// Returns `None` when nothing suspicious is shown.
pub fn classify_messages(
    status: &str,
    popup: &str,
    label: &str,
    opts: ClassifyOptions,
) -> Option<ExportOutcome> {
    if contains_ci(popup, "no shipments") {
        return Some(ExportOutcome::NoData(popup.trim().to_string()));
    }
    if contains_ci(label, "no data") || contains_ci(status, "no data exists") {
        let msg = if contains_ci(label, "no data") {
            label
        } else {
            status
        };
        return Some(ExportOutcome::NoData(msg.trim().to_string()));
    }
    if contains_ci(status, "no display authorization") {
        if opts.ignore_display_error {
            return None;
        }
        return Some(ExportOutcome::Error(status.trim().to_string()));
    }
    if contains_ci(status, "no layouts found") {
        return Some(ExportOutcome::Error(status.trim().to_string()));
    }
    // Status bar error marker (`E:`), case sensitive like the VBA original.
    if status.contains("E:") {
        return Some(ExportOutcome::Error(status.trim().to_string()));
    }
    None
}

/// Read the current screen and classify it (P6).
pub fn classify_screen(
    session: &GuiSession,
    opts: ClassifyOptions,
) -> Result<Option<ExportOutcome>> {
    let status = exist_ctrl(session, 0, "/sbar", true)?.ctext;
    let popup = exist_ctrl(session, 1, "/usr/txtMESSTXT1", true)?.ctext;
    let label = exist_ctrl(session, 0, "/usr/lbl[2,4]", true)?.ctext;
    Ok(classify_messages(&status, &popup, &label, opts))
}

/// Pure part of [`classify_after_execute`]: after pressing execute, still being on the
/// selection screen means no list was produced. A status bar text then is the reason (reported
/// as `NoData`); without any text it is an error (nothing explains why we did not move on).
pub fn classify_post_execute(
    tcode: &str,
    still_on_selection_screen: bool,
    status: &str,
) -> Option<ExportOutcome> {
    if !still_on_selection_screen {
        return None;
    }
    let s = status.trim();
    if s.is_empty() {
        Some(ExportOutcome::Error(format!(
            "{tcode}: still on the selection screen after execute and no status message"
        )))
    } else {
        Some(ExportOutcome::NoData(s.to_string()))
    }
}

/// Headless check right after execute (P6 plus "did the screen change"): the generic
/// [`classify_screen`] first, then [`classify_post_execute`] using `selection_marker`, the id
/// of a control that only exists on the selection screen (relative to `wnd[0]`, e.g.
/// `/usr/ctxtT1_LGNUM`). UNVERIFIED live for every tcode that uses it.
pub fn classify_after_execute(
    session: &GuiSession,
    tcode: &str,
    selection_marker: &str,
    opts: ClassifyOptions,
) -> Result<Option<ExportOutcome>> {
    if let Some(o) = classify_screen(session, opts)? {
        return Ok(Some(o));
    }
    let on_sel = exist_ctrl(session, 0, selection_marker, true)?.cband;
    let status = exist_ctrl(session, 0, "/sbar", true)?.ctext;
    Ok(classify_post_execute(tcode, on_sel, &status))
}

/// Strip leading zeros from a purely numeric value (decision 25). `"0000"` becomes `"0"`.
/// Non-numeric values are returned trimmed but otherwise untouched.
pub fn normalize_value(value: &str) -> String {
    let v = value.trim();
    if !v.is_empty() && v.chars().all(|c| c.is_ascii_digit()) {
        let stripped = v.trim_start_matches('0');
        if stripped.is_empty() {
            "0".to_string()
        } else {
            stripped.to_string()
        }
    } else {
        v.to_string()
    }
}

/// Distinct, blank-free list preserving first-seen order, optionally dropping a header
/// row, with every value normalised via [`normalize_value`] (decision 32: each value is
/// sent to SAP once). Use before every multi-value paste.
pub fn paste_value_list<S: AsRef<str>>(raw: &[S], skip_header: bool) -> Vec<String> {
    let mut seen = std::collections::HashSet::new();
    let mut out = Vec::new();
    for v in raw.iter().skip(usize::from(skip_header)) {
        let n = normalize_value(v.as_ref());
        if n.is_empty() {
            continue;
        }
        if seen.insert(n.clone()) {
            out.push(n);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn post_execute_classification() {
        assert_eq!(classify_post_execute("X", false, "anything"), None);
        assert_eq!(
            classify_post_execute("X", true, " No data found "),
            Some(ExportOutcome::NoData("No data found".into()))
        );
        assert!(matches!(
            classify_post_execute("X", true, ""),
            Some(ExportOutcome::Error(m)) if m.starts_with("X:")
        ));
    }

    const DEF: ClassifyOptions = ClassifyOptions {
        ignore_display_error: false,
    };

    #[test]
    fn classifier_clean_screen_is_none() {
        assert_eq!(classify_messages("", "", "", DEF), None);
        assert_eq!(classify_messages("Data selected", "", "", DEF), None);
    }

    #[test]
    fn classifier_status_error() {
        assert!(matches!(
            classify_messages("E: Something broke", "", "", DEF),
            Some(ExportOutcome::Error(_))
        ));
    }

    #[test]
    fn classifier_display_authorization() {
        let s = "You have no display authorization for shipment 1";
        assert!(matches!(
            classify_messages(s, "", "", DEF),
            Some(ExportOutcome::Error(_))
        ));
        let ign = ClassifyOptions {
            ignore_display_error: true,
        };
        assert_eq!(classify_messages(s, "", "", ign), None);
    }

    #[test]
    fn classifier_no_shipments_popup_is_no_data() {
        assert!(matches!(
            classify_messages(
                "",
                "No shipments were found for the selection criteria",
                "",
                DEF
            ),
            Some(ExportOutcome::NoData(_))
        ));
    }

    #[test]
    fn classifier_no_data_label_and_status() {
        assert!(matches!(
            classify_messages("", "", "No data exists", DEF),
            Some(ExportOutcome::NoData(_))
        ));
        assert!(matches!(
            classify_messages("No data exists for selection", "", "", DEF),
            Some(ExportOutcome::NoData(_))
        ));
    }

    #[test]
    fn classifier_no_layouts() {
        assert!(matches!(
            classify_messages("No layouts found", "", "", DEF),
            Some(ExportOutcome::Error(_))
        ));
    }

    #[test]
    fn normalize_strips_leading_zeros() {
        assert_eq!(normalize_value("0001234"), "1234");
        assert_eq!(normalize_value(" 0000 "), "0");
        assert_eq!(normalize_value("A0012"), "A0012");
        assert_eq!(normalize_value(""), "");
    }

    #[test]
    fn paste_list_distinct_blank_free_header_free() {
        let raw = vec!["delivery", "0012", "", " 12 ", "34", "  ", "0034", "56"];
        assert_eq!(paste_value_list(&raw, true), vec!["12", "34", "56"]);
        assert_eq!(
            paste_value_list(&raw, false),
            vec!["delivery", "12", "34", "56"]
        );
    }

    #[test]
    fn outcome_helpers() {
        let f = ExportOutcome::from_saved(true, "C:\\out", "a.txt");
        assert!(f.is_file());
        assert_eq!(f.path().unwrap(), &PathBuf::from("C:\\out").join("a.txt"));
        assert!(!ExportOutcome::NoData("x".into()).into_bool());
        assert!(!ExportOutcome::from_saved(false, "d", "f").is_file());
    }
}
