//! Create and save SAP ALV layouts from a column list (`[layouts.<TCODE>.<NAME>]`).
//!
//! Port of `check_select_layout` (the `setup:` branch), `SetupLayout` and `SetupLayout_li` of the
//! VBA macro (`Control_id_.bas`). STATUS: built, NOT run live. Every control id below comes from
//! the macro, none has been verified against a live system.
//!
//! Entry points:
//! * [`create_layout`]: create + save one layout on the CURRENT screen of a session (the report
//!   result list must be displayed). An existing layout is left alone unless `overwrite`.
//! * [`apply_columns`]: open the editor, pick the columns and confirm without saving.
//! * [`auto_create_on_missing`]: opt-in (`--create-missing-layout` / `[global]
//!   create_missing_layout`) hook that the layout choosers call before they prompt or fail.
//! * [`create_layout_on_current_screen`]: the `--create-layout` command and the TUI item.
//!
//! The decision logic ([`create_core`], [`decide_overwrite`]) is separate from the GUI code
//! (seam [`CreateGui`]) so it is unit tested with a fake.
#![allow(dead_code)]

use anyhow::{anyhow, Result};
use sap_scripting::*;
use std::fmt;
use std::sync::{Mutex, OnceLock};
use std::thread;
use std::time::{Duration, Instant};

use crate::utils::layout_editors::{
    pick_columns_grid, pick_columns_list, PickReport, RealGridScreen, RealListScreen,
    GRID_ROW_LIMIT,
};
use crate::utils::layout_store::{LayoutSpec, LayoutStore};
use crate::utils::sap_ctrl_utils::{exist_ctrl, hit_ctrl};

/// Object path of the grid-style layout editor below `wnd[1]`.
pub const GRID_EDITOR_BASE: &str = "/usr/tabsG_TS_ALV/tabpALV_M_R1/ssubSUB_DYN0510:SAPLSKBH:0620";
/// Object path of the list-style layout editor below `wnd[1]`.
pub const LIST_EDITOR_BASE: &str = "/usr/tabsTS_LINES/tabpLI01/ssubSUB810:SAPLSKBH:0810";

const GRID_SAVE_DIALOG: &str =
    "wnd[2]/usr/tabsG50_TABSTRIP/tabpTAB_D0501/ssubD0505_SUBSCREEN:SAPLSLVC_DIALOG:0501/";
/// The choose-layout dialog as an ALV grid (column `VARIANT` = name).
const CHOOSER_GRIDS: [&str; 2] = [
    "wnd[1]/usr/ssubD0500_SUBSCREEN:SAPLSLVC_DIALOG:0501/cntlG51_CONTAINER/shellcont/shell",
    "wnd[1]/usr/cntlGRID/shellcont/shell",
];

// ---------------------------------------------------------------------------------------------
// profiles
// ---------------------------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EditorStyle {
    /// Two `GuiGridView`s (ZMDESNR, ZVT11, 149).
    Grid,
    /// Two `GuiTableControl`s (VT11, VL06O, LT23, LX03, LX02).
    List,
}

/// A control of the main window: a menu item (`GuiMenu.select`) or a button (`GuiButton.press`).
/// Never mix them up: a menu is not a button.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Control {
    Menu(&'static str),
    Button(&'static str),
}

impl Control {
    pub fn id(&self) -> &'static str {
        match self {
            Control::Menu(i) | Control::Button(i) => i,
        }
    }
}

/// The save dialog of the list-style editor.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ListDialog {
    /// With the user-specific checkbox `chkG_FOR_USER` (dialog A: VT11, VL06O, LT23).
    UserSpecific,
    /// Without the checkbox (dialog B: LX03, LX02).
    Plain,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SaveFlow {
    /// Grid editor: save button in the editor, dialog in `wnd[2]`, overwrite popup in `wnd[3]`.
    Grid,
    /// List editor: confirm the editor, then the `save` control opens a dialog in `wnd[1]`,
    /// overwrite popup in `wnd[2]`.
    List { save: Control, dialog: ListDialog },
}

#[derive(Debug, Clone, Copy)]
pub struct Profile {
    pub tcode: &'static str,
    pub style: EditorStyle,
    /// Controls that open the layout editor, tried in order (first that opens it wins).
    pub open_editor: &'static [Control],
    /// Controls that open the choose-layout dialog (used by the existence check).
    pub choose_layout: &'static [Control],
    pub save: SaveFlow,
    /// Longest layout name SAP accepts for this tcode (the macro knows 12 for LX03/LX02 only).
    pub max_name_len: Option<usize>,
}

const BTN_CHANGE: Control = Control::Button("wnd[0]/tbar[1]/btn[32]");
const BTN_CHOOSE: Control = Control::Button("wnd[0]/tbar[1]/btn[33]");

static PROFILES: &[Profile] = &[
    Profile {
        tcode: "ZMDESNR",
        style: EditorStyle::Grid,
        open_editor: &[
            BTN_CHANGE,
            Control::Menu("wnd[0]/mbar/menu[4]/menu[0]/menu[0]"),
        ],
        choose_layout: &[
            BTN_CHOOSE,
            Control::Menu("wnd[0]/mbar/menu[4]/menu[0]/menu[1]"),
        ],
        save: SaveFlow::Grid,
        max_name_len: None,
    },
    Profile {
        tcode: "ZVT11",
        style: EditorStyle::Grid,
        open_editor: &[BTN_CHANGE],
        choose_layout: &[BTN_CHOOSE],
        save: SaveFlow::Grid,
        max_name_len: None,
    },
    Profile {
        tcode: "Y_DN3_47000149",
        style: EditorStyle::Grid,
        open_editor: &[
            BTN_CHANGE,
            Control::Menu("wnd[0]/mbar/menu[3]/menu[0]/menu[0]"),
        ],
        choose_layout: &[BTN_CHOOSE],
        save: SaveFlow::Grid,
        max_name_len: None,
    },
    Profile {
        tcode: "VT11",
        style: EditorStyle::List,
        open_editor: &[Control::Menu("wnd[0]/mbar/menu[3]/menu[0]/menu[0]")],
        choose_layout: &[Control::Menu("wnd[0]/mbar/menu[3]/menu[0]/menu[1]")],
        save: SaveFlow::List {
            save: Control::Menu("wnd[0]/mbar/menu[3]/menu[0]/menu[3]"),
            dialog: ListDialog::UserSpecific,
        },
        max_name_len: None,
    },
    Profile {
        tcode: "VL06O",
        style: EditorStyle::List,
        open_editor: &[
            Control::Menu("wnd[0]/mbar/menu[3]/menu[2]/menu[0]"),
            BTN_CHANGE,
        ],
        choose_layout: &[
            Control::Menu("wnd[0]/mbar/menu[3]/menu[2]/menu[1]"),
            BTN_CHOOSE,
        ],
        save: SaveFlow::List {
            save: Control::Menu("wnd[0]/mbar/menu[3]/menu[2]/menu[3]"),
            dialog: ListDialog::UserSpecific,
        },
        max_name_len: None,
    },
    Profile {
        tcode: "LT23",
        style: EditorStyle::List,
        open_editor: &[
            Control::Menu("wnd[0]/mbar/menu[3]/menu[2]/menu[0]"),
            BTN_CHANGE,
        ],
        choose_layout: &[BTN_CHOOSE],
        save: SaveFlow::List {
            save: Control::Menu("wnd[0]/mbar/menu[3]/menu[2]/menu[3]"),
            dialog: ListDialog::UserSpecific,
        },
        max_name_len: None,
    },
    Profile {
        tcode: "LX03",
        style: EditorStyle::List,
        open_editor: &[Control::Menu("wnd[0]/mbar/menu[3]/menu[2]/menu[0]")],
        choose_layout: &[BTN_CHOOSE],
        save: SaveFlow::List {
            save: Control::Button("wnd[0]/tbar[1]/btn[36]"),
            dialog: ListDialog::Plain,
        },
        max_name_len: Some(12),
    },
    Profile {
        tcode: "LX02",
        style: EditorStyle::List,
        open_editor: &[Control::Menu("wnd[0]/mbar/menu[3]/menu[2]/menu[0]")],
        choose_layout: &[BTN_CHOOSE],
        save: SaveFlow::List {
            save: Control::Button("wnd[0]/tbar[1]/btn[36]"),
            dialog: ListDialog::Plain,
        },
        max_name_len: Some(12),
    },
];

/// Tcodes that have a layout profile.
pub fn supported_tcodes() -> Vec<&'static str> {
    PROFILES.iter().map(|p| p.tcode).collect()
}

/// Profile of `tcode` (case-insensitive, trimmed); an unsupported tcode is a clear error.
pub fn profile_for(tcode: &str) -> std::result::Result<&'static Profile, LayoutError> {
    let t = tcode.trim();
    PROFILES
        .iter()
        .find(|p| p.tcode.eq_ignore_ascii_case(t))
        .ok_or_else(|| LayoutError::UnsupportedTcode {
            tcode: t.to_string(),
        })
}

/// Name rules of a profile: not empty, not longer than the tcode accepts.
pub fn check_name(profile: &Profile, name: &str) -> std::result::Result<(), LayoutError> {
    let n = name.trim();
    if n.is_empty() {
        return Err(LayoutError::EmptyName);
    }
    if let Some(max) = profile.max_name_len {
        if n.chars().count() > max {
            return Err(LayoutError::NameTooLong {
                tcode: profile.tcode.to_string(),
                name: n.to_string(),
                max,
            });
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------------------------------
// errors and outcomes
// ---------------------------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LayoutError {
    UnsupportedTcode {
        tcode: String,
    },
    EmptyName,
    NameTooLong {
        tcode: String,
        name: String,
        max: usize,
    },
    DialogOpen {
        title: String,
    },
    WrongTransaction {
        expected: String,
        actual: String,
    },
    EditorNotOpened {
        tcode: String,
    },
    ChooserNotOpened {
        tcode: String,
    },
    MissingColumns {
        tcode: String,
        name: String,
        missing: Vec<String>,
    },
    NameNotAccepted {
        name: String,
        shown: String,
    },
    SaveDialogNotOpened {
        name: String,
        step: String,
    },
    LeftoverDialog {
        title: String,
    },
}

impl fmt::Display for LayoutError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            LayoutError::UnsupportedTcode { tcode } => write!(
                f,
                "layouts cannot be created for tcode '{tcode}' (supported: {})",
                supported_tcodes().join(", ")
            ),
            LayoutError::EmptyName => write!(f, "the layout name is empty"),
            LayoutError::NameTooLong { tcode, name, max } => write!(
                f,
                "layout name '{name}' has {} characters, {tcode} accepts at most {max}",
                name.chars().count()
            ),
            LayoutError::DialogOpen { title } => write!(
                f,
                "a SAP dialog is open ('{title}'): show the report result list and close dialogs first"
            ),
            LayoutError::WrongTransaction { expected, actual } => write!(
                f,
                "the session shows transaction '{actual}', not {expected}: run the report in SAP so its result list is displayed, then retry"
            ),
            LayoutError::EditorNotOpened { tcode } => write!(
                f,
                "the layout editor of {tcode} did not open (is the report's result list displayed?)"
            ),
            LayoutError::ChooserNotOpened { tcode } => write!(
                f,
                "could not open the choose-layout dialog of {tcode} (is the report's result list displayed?)"
            ),
            LayoutError::MissingColumns {
                tcode,
                name,
                missing,
            } => write!(
                f,
                "layout '{name}' ({tcode}) was NOT saved: column(s) not found in SAP: {}",
                missing.join(", ")
            ),
            LayoutError::NameNotAccepted { name, shown } => write!(
                f,
                "SAP did not accept the layout name '{name}' (the field shows '{shown}'): too long or invalid; nothing was saved"
            ),
            LayoutError::SaveDialogNotOpened { name, step } => write!(
                f,
                "layout '{name}': the save dialog did not open ({step})"
            ),
            LayoutError::LeftoverDialog { title } => {
                write!(f, "a SAP dialog stayed open after saving: '{title}'")
            }
        }
    }
}

impl std::error::Error for LayoutError {}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CreateOutcome {
    /// New layout saved (and applied to the displayed list).
    Created,
    /// An existing layout was replaced (`overwrite`).
    Overwritten,
    /// The layout exists in SAP and was left unchanged.
    Exists,
}

impl CreateOutcome {
    pub fn describe(&self, name: &str) -> String {
        match self {
            CreateOutcome::Created => format!("layout '{name}' created and saved"),
            CreateOutcome::Overwritten => format!("layout '{name}' existed and was overwritten"),
            CreateOutcome::Exists => {
                format!("layout '{name}' exists, skipped; pass --overwrite to replace it")
            }
        }
    }
}

/// Answer to SAP's "layout exists, overwrite?" popup.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PopupAnswer {
    Yes,
    No,
    /// No popup appeared.
    NoPopup,
}

/// Yes only with `overwrite`; every other popup is answered No (safety net: the existence check
/// said the name is free, or the caller did not allow overwriting).
pub fn decide_overwrite(popup_shown: bool, overwrite: bool) -> PopupAnswer {
    match (popup_shown, overwrite) {
        (false, _) => PopupAnswer::NoPopup,
        (true, true) => PopupAnswer::Yes,
        (true, false) => PopupAnswer::No,
    }
}

/// What the save step did.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SaveResult {
    pub popup_shown: bool,
    pub answer: PopupAnswer,
}

/// Outcome from the save step and whether the layout was known to exist beforehand.
pub fn outcome_of(was_existing: bool, save: SaveResult) -> CreateOutcome {
    match save.answer {
        PopupAnswer::Yes => CreateOutcome::Overwritten,
        PopupAnswer::No => CreateOutcome::Exists,
        PopupAnswer::NoPopup => {
            if was_existing {
                CreateOutcome::Overwritten
            } else {
                CreateOutcome::Created
            }
        }
    }
}

/// Does `names` contain the layout `wanted` (trimmed, case-insensitive)?
pub fn names_contain(names: &[String], wanted: &str) -> bool {
    let w = wanted.trim();
    !w.is_empty() && names.iter().any(|n| n.trim().eq_ignore_ascii_case(w))
}

// ---------------------------------------------------------------------------------------------
// core (seam over the GUI, unit tested with a fake)
// ---------------------------------------------------------------------------------------------

/// The GUI steps of [`create_core`].
pub trait CreateGui {
    /// Does a layout called `name` exist in SAP? Opens and closes the choose-layout dialog
    /// without applying anything.
    fn exists(&mut self, name: &str) -> Result<bool>;
    /// Open the editor and build the column list; the editor stays open.
    fn edit_columns(&mut self, columns: &[String]) -> Result<PickReport>;
    /// Leave the editor without saving.
    fn cancel_editor(&mut self) -> Result<()>;
    /// Save the layout; `overwrite` is only the permission to answer Yes to SAP's popup.
    fn save(&mut self, name: &str, description: &str, overwrite: bool) -> Result<SaveResult>;
    /// Always called last: leave the session on the result list (close every dialog).
    fn cleanup(&mut self);
}

/// Existence check, editor, save, cleanup. Exists + no overwrite changes nothing; missing
/// columns cancel the editor and save nothing.
pub fn create_core<G: CreateGui>(
    gui: &mut G,
    tcode: &str,
    spec: &LayoutSpec,
    overwrite: bool,
) -> Result<CreateOutcome> {
    let existing = match gui.exists(&spec.name) {
        Ok(e) => e,
        Err(e) => {
            gui.cleanup();
            return Err(e);
        }
    };
    if existing && !overwrite {
        gui.cleanup();
        return Ok(CreateOutcome::Exists);
    }
    let result = (|| -> Result<CreateOutcome> {
        let report = gui.edit_columns(&spec.columns)?;
        if !report.missing.is_empty() {
            gui.cancel_editor()?;
            return Err(LayoutError::MissingColumns {
                tcode: tcode.to_uppercase(),
                name: spec.name.clone(),
                missing: report.missing,
            }
            .into());
        }
        if !report.kept_extra.is_empty() {
            println!(
                "Note: these columns stay in the layout (SAP does not allow removing them): {}",
                report.kept_extra.join(", ")
            );
        }
        let save = gui.save(&spec.name, &spec.description, overwrite)?;
        Ok(outcome_of(existing, save))
    })();
    gui.cleanup();
    result
}

/// One layout creation at a time per process (parallel sessions could otherwise create the same
/// layout twice); the existence check inside [`create_core`] runs after the lock is taken.
static CREATE_LOCK: Mutex<()> = Mutex::new(());

/// Options of [`create_layout`].
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct CreateOpts {
    /// Replace a layout that already exists in SAP. Default off: an existing layout is skipped.
    pub overwrite: bool,
}

/// Create and save the layout `spec` for `tcode` on the current screen of `session` (the report's
/// result list must be displayed). An existing layout is skipped ([`CreateOutcome::Exists`])
/// unless `opts.overwrite`. Never prompts. The session is left on the result list, also on
/// errors.
pub fn create_layout(
    session: &GuiSession,
    tcode: &str,
    spec: &LayoutSpec,
    opts: CreateOpts,
) -> Result<CreateOutcome> {
    let profile = profile_for(tcode)?;
    check_name(profile, &spec.name)?;
    if spec.columns.is_empty() {
        return Err(anyhow!("layout '{}' has no columns", spec.name));
    }
    let _guard = CREATE_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    ensure_no_dialog(session)?;
    let mut gui = RealCreateGui { session, profile };
    let outcome = create_core(&mut gui, profile.tcode, spec, opts.overwrite)?;
    let status = status_text(session);
    println!("{} (status bar: {status})", outcome.describe(&spec.name));
    Ok(outcome)
}

/// Does the layout exist in SAP? Opens the choose-layout dialog, looks, closes it again.
pub fn layout_exists_in_sap(session: &GuiSession, tcode: &str, name: &str) -> Result<bool> {
    let profile = profile_for(tcode)?;
    let _guard = CREATE_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    ensure_no_dialog(session)?;
    let r = check_exists(session, profile, name);
    let _ = cancel_all_popups(session);
    r
}

/// Open the editor, pick `columns` and confirm WITHOUT saving (the layout is applied to the
/// displayed list only). Missing columns cancel the editor and are an error.
pub fn apply_columns(session: &GuiSession, tcode: &str, columns: &[String]) -> Result<PickReport> {
    let profile = profile_for(tcode)?;
    let cols = crate::utils::layout_store::normalize_columns(columns);
    if cols.is_empty() {
        return Err(anyhow!("no columns given"));
    }
    let _guard = CREATE_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    ensure_no_dialog(session)?;
    let mut gui = RealCreateGui { session, profile };
    let result = (|| -> Result<PickReport> {
        let report = gui.edit_columns(&cols)?;
        if !report.missing.is_empty() {
            gui.cancel_editor()?;
            return Err(LayoutError::MissingColumns {
                tcode: profile.tcode.to_string(),
                name: "(not saved)".to_string(),
                missing: report.missing,
            }
            .into());
        }
        confirm_editor(session, profile)?;
        Ok(report)
    })();
    gui.cleanup();
    result
}

// ---------------------------------------------------------------------------------------------
// GUI helpers
// ---------------------------------------------------------------------------------------------

fn find(session: &GuiSession, id: &str) -> Option<GuiComponent> {
    session.find_by_id(id.to_string()).ok()
}

fn window_open(session: &GuiSession, n: i32) -> bool {
    find(session, &format!("wnd[{n}]")).is_some()
}

fn window_title(session: &GuiSession, n: i32) -> String {
    exist_ctrl(session, n, "", true)
        .map(|c| c.ctext)
        .unwrap_or_default()
}

pub(crate) fn status_text(session: &GuiSession) -> String {
    hit_ctrl(session, 0, "/sbar", "Text", "Get", "").unwrap_or_default()
}

fn wait_until(timeout_ms: u64, mut f: impl FnMut() -> bool) -> bool {
    let start = Instant::now();
    loop {
        if f() {
            return true;
        }
        if start.elapsed() >= Duration::from_millis(timeout_ms) {
            return false;
        }
        thread::sleep(Duration::from_millis(120));
    }
}

fn wait_window(session: &GuiSession, n: i32, open: bool, timeout_ms: u64) -> bool {
    wait_until(timeout_ms, || window_open(session, n) == open)
}

/// Press a button; `Ok(false)` when it does not exist (or is not a button).
pub(crate) fn press(session: &GuiSession, id: &str) -> Result<bool> {
    match find(session, id).and_then(|c| c.downcast::<GuiButton>()) {
        Some(b) => {
            b.press()?;
            Ok(true)
        }
        None => Ok(false),
    }
}

/// Select a menu item; `Ok(false)` when it does not exist (or is not a menu).
fn select_menu(session: &GuiSession, id: &str) -> Result<bool> {
    match find(session, id).and_then(|c| c.downcast::<GuiMenu>()) {
        Some(m) => {
            m.select()?;
            Ok(true)
        }
        None => Ok(false),
    }
}

fn do_control(session: &GuiSession, c: &Control) -> Result<bool> {
    match c {
        Control::Menu(id) => select_menu(session, id),
        Control::Button(id) => press(session, id),
    }
}

fn send_vkey(session: &GuiSession, n: i32, key: i16) -> Result<bool> {
    let Some(w) = find(session, &format!("wnd[{n}]")) else {
        return Ok(false);
    };
    if let Some(m) = w.downcast::<GuiModalWindow>() {
        m.send_v_key(key)?;
        return Ok(true);
    }
    if let Some(m) = w.downcast::<GuiFrameWindow>() {
        m.send_v_key(key)?;
        return Ok(true);
    }
    Ok(false)
}

/// Set a text / ctext field and return what the field shows afterwards.
fn set_field(session: &GuiSession, id: &str, text: &str) -> Result<String> {
    let c = find(session, id).ok_or_else(|| anyhow!("field {id} not found"))?;
    if let Some(t) = c.downcast::<GuiCTextField>() {
        t.set_text(text.to_string())?;
        return Ok(t.text().unwrap_or_default());
    }
    if let Some(t) = c.downcast::<GuiTextField>() {
        t.set_text(text.to_string())?;
        return Ok(t.text().unwrap_or_default());
    }
    Err(anyhow!("{id} is not a text field"))
}

/// Set the layout name and check that SAP kept it (a too long name is cut silently).
fn set_name_verified(session: &GuiSession, id: &str, name: &str) -> Result<()> {
    let shown = set_field(session, id, name)?;
    if !shown.trim().eq_ignore_ascii_case(name.trim()) {
        return Err(LayoutError::NameNotAccepted {
            name: name.to_string(),
            shown: shown.trim().to_string(),
        }
        .into());
    }
    Ok(())
}

fn set_checkbox(session: &GuiSession, id: &str, value: bool) -> Result<bool> {
    match find(session, id).and_then(|c| c.downcast::<GuiCheckBox>()) {
        Some(c) => {
            c.set_selected(value)?;
            Ok(true)
        }
        None => Ok(false),
    }
}

fn highest_popup(session: &GuiSession) -> i32 {
    (1..=5)
        .rev()
        .find(|n| window_open(session, *n))
        .unwrap_or(0)
}

/// Close every dialog above the main window: F12 (cancel), then close(), highest first.
fn cancel_all_popups(session: &GuiSession) -> Result<()> {
    for _ in 0..8 {
        let top = highest_popup(session);
        if top == 0 {
            return Ok(());
        }
        let _ = send_vkey(session, top, 12);
        if wait_window(session, top, false, 600) {
            continue;
        }
        if let Some(w) = find(session, &format!("wnd[{top}]")) {
            if let Some(m) = w.downcast::<GuiModalWindow>() {
                let _ = m.close();
            }
        }
        wait_window(session, top, false, 600);
    }
    Ok(())
}

fn ensure_no_dialog(session: &GuiSession) -> Result<()> {
    if window_open(session, 1) {
        return Err(LayoutError::DialogOpen {
            title: window_title(session, 1),
        }
        .into());
    }
    Ok(())
}

fn editor_present(session: &GuiSession, profile: &Profile) -> bool {
    match profile.style {
        EditorStyle::Grid => RealGridScreen::new(session, 1, GRID_EDITOR_BASE).is_open(),
        EditorStyle::List => RealListScreen::new(session, 1, LIST_EDITOR_BASE).is_open(),
    }
}

fn open_editor(session: &GuiSession, profile: &Profile) -> Result<()> {
    for ctl in profile.open_editor {
        if !do_control(session, ctl)? {
            continue;
        }
        if wait_until(4000, || editor_present(session, profile)) {
            thread::sleep(Duration::from_millis(300));
            return Ok(());
        }
        // something opened that is not the editor: close it and try the next control
        cancel_all_popups(session)?;
    }
    Err(LayoutError::EditorNotOpened {
        tcode: profile.tcode.to_string(),
    }
    .into())
}

/// Confirm (Enter) the editor so that its columns are applied.
pub(crate) fn confirm_editor(session: &GuiSession, profile: &Profile) -> Result<()> {
    if !press(session, "wnd[1]/tbar[0]/btn[0]")? {
        send_vkey(session, 1, 0)?;
    }
    if !wait_window(session, 1, false, 3000) {
        return Err(LayoutError::LeftoverDialog {
            title: window_title(session, 1),
        }
        .into());
    }
    let _ = profile;
    Ok(())
}

// ---------------------------------------------------------------------------------------------
// existence check
// ---------------------------------------------------------------------------------------------

/// Names shown by the grid-style choose dialog (column `VARIANT`; `TEXT` when `VARIANT` cannot be
/// read at all).
fn read_chooser_grid_names(grid: &GuiGridView) -> Vec<String> {
    let rows = grid.row_count().unwrap_or(0);
    let mut variants = Vec::new();
    let mut texts = Vec::new();
    let mut variant_ok = false;
    for i in 0..rows {
        if let Ok(v) = grid.get_cell_value(i, "VARIANT".to_string()) {
            variant_ok = true;
            variants.push(v);
        }
        if let Ok(t) = grid.get_cell_value(i, "TEXT".to_string()) {
            texts.push(t);
        }
    }
    if variant_ok {
        variants
    } else {
        texts
    }
}

/// Classic list-style chooser (labels `lbl[1,row]`): visible labels, then the Find dialog.
fn list_chooser_contains(session: &GuiSession, name: &str) -> bool {
    use crate::utils::select_layout_utils::list_label_matches;
    let labels_match = |wnd: i32| -> bool {
        (1..=80).any(|row| {
            find(session, &format!("wnd[{wnd}]/usr/lbl[1,{row}]"))
                .and_then(|c| c.downcast::<GuiLabel>())
                .is_some_and(|l| list_label_matches(&l.text().unwrap_or_default(), name))
        })
    };
    if labels_match(1) {
        return true;
    }
    // not visible: use Find (wnd[1] toolbar btn[71], search string in wnd[2], hits in wnd[3])
    if !press(session, "wnd[1]/tbar[0]/btn[71]").unwrap_or(false) {
        return false;
    }
    if !wait_window(session, 2, true, 2000) {
        return false;
    }
    if set_field(session, "wnd[2]/usr/txtRSYSF-STRING", name).is_err() {
        return false;
    }
    let _ = press(session, "wnd[2]/tbar[0]/btn[0]");
    if !wait_window(session, 3, true, 2000) {
        return false;
    }
    labels_match(3)
}

fn chooser_contains(session: &GuiSession, name: &str) -> bool {
    for id in CHOOSER_GRIDS {
        if let Some(grid) = find(session, id).and_then(|c| c.downcast::<GuiGridView>()) {
            let names = read_chooser_grid_names(&grid);
            println!("Choose-layout dialog lists {} layouts", names.len());
            return names_contain(&names, name);
        }
    }
    list_chooser_contains(session, name)
}

/// Open the choose-layout dialog, look for `name`, close the dialog. Nothing is applied.
fn check_exists(session: &GuiSession, profile: &Profile, name: &str) -> Result<bool> {
    let mut pressed = false;
    let mut opened = false;
    for ctl in profile.choose_layout {
        if !do_control(session, ctl)? {
            continue;
        }
        pressed = true;
        if wait_window(session, 1, true, 3000) {
            opened = true;
            break;
        }
    }
    if !pressed {
        return Err(LayoutError::ChooserNotOpened {
            tcode: profile.tcode.to_string(),
        }
        .into());
    }
    if !opened {
        // SAP shows no dialog when the user has no layout at all: nothing exists yet
        println!("No choose-layout dialog opened (no saved layouts?): treating '{name}' as new");
        return Ok(false);
    }
    let found = chooser_contains(session, name);
    cancel_all_popups(session)?;
    Ok(found)
}

// ---------------------------------------------------------------------------------------------
// save dialogs
// ---------------------------------------------------------------------------------------------

pub(crate) fn save_grid(
    session: &GuiSession,
    name: &str,
    description: &str,
    overwrite: bool,
) -> Result<SaveResult> {
    let dialog = |what: &str| format!("{GRID_SAVE_DIALOG}{what}");
    if !press(session, "wnd[1]/tbar[0]/btn[5]")? || !wait_window(session, 2, true, 3000) {
        return Err(LayoutError::SaveDialogNotOpened {
            name: name.to_string(),
            step: "save button of the editor".to_string(),
        }
        .into());
    }
    // user-specific stays ticked, "default layout" stays off (like the macro)
    set_checkbox(session, &dialog("chkG51_USPEC"), true)?;
    set_checkbox(session, &dialog("chkLTVARIANT-DEFAULTVAR"), false)?;
    set_name_verified(session, &dialog("txtLTDX-VARIANT"), name)?;
    set_field(session, &dialog("txtLTDXT-TEXT"), description)?;
    press(session, "wnd[2]/tbar[0]/btn[0]")?;
    // "layout exists, overwrite?" popup in wnd[3]
    let popup_shown = wait_window(session, 3, true, 1500);
    let answer = decide_overwrite(popup_shown, overwrite);
    match answer {
        PopupAnswer::Yes => {
            press(session, "wnd[3]/usr/btnSPOP-OPTION1")?;
        }
        PopupAnswer::No => {
            if !press(session, "wnd[3]/usr/btnSPOP-OPTION2")? {
                send_vkey(session, 3, 12)?;
            }
            // wnd[2] is still open: the caller cancels the dialogs, nothing was saved
            return Ok(SaveResult {
                popup_shown,
                answer,
            });
        }
        PopupAnswer::NoPopup => {}
    }
    if !wait_window(session, 2, false, 3000) {
        return Err(LayoutError::LeftoverDialog {
            title: window_title(session, 2),
        }
        .into());
    }
    // green check of the editor
    press(session, "wnd[1]/tbar[0]/btn[0]")?;
    if !wait_window(session, 1, false, 3000) {
        return Err(LayoutError::LeftoverDialog {
            title: window_title(session, 1),
        }
        .into());
    }
    Ok(SaveResult {
        popup_shown,
        answer,
    })
}

pub(crate) fn save_list(
    session: &GuiSession,
    profile: &Profile,
    save: &Control,
    dialog: ListDialog,
    name: &str,
    description: &str,
    overwrite: bool,
) -> Result<SaveResult> {
    // Enter on the editor, then the save control of the list
    confirm_editor(session, profile)?;
    if !do_control(session, save)?
        || !wait_until(3000, || {
            find(session, "wnd[1]/usr/ctxtLTDX-VARIANT").is_some()
        })
    {
        return Err(LayoutError::SaveDialogNotOpened {
            name: name.to_string(),
            step: format!("save control {}", save.id()),
        }
        .into());
    }
    if dialog == ListDialog::UserSpecific {
        set_checkbox(session, "wnd[1]/usr/chkG_FOR_USER", true)?;
    }
    set_name_verified(session, "wnd[1]/usr/ctxtLTDX-VARIANT", name)?;
    set_field(session, "wnd[1]/usr/txtLTDXT-TEXT", description)?;
    send_vkey(session, 1, 0)?;
    // "layout exists, overwrite?" popup in wnd[2]
    let popup_shown = wait_window(session, 2, true, 1500);
    let answer = decide_overwrite(popup_shown, overwrite);
    match answer {
        PopupAnswer::Yes => {
            send_vkey(session, 2, 0)?;
        }
        PopupAnswer::No => {
            if !press(session, "wnd[2]/usr/btnSPOP-OPTION2")? {
                send_vkey(session, 2, 12)?;
            }
            return Ok(SaveResult {
                popup_shown,
                answer,
            });
        }
        PopupAnswer::NoPopup => {}
    }
    if !wait_window(session, 1, false, 3000) {
        return Err(LayoutError::LeftoverDialog {
            title: window_title(session, 1),
        }
        .into());
    }
    Ok(SaveResult {
        popup_shown,
        answer,
    })
}

// ---------------------------------------------------------------------------------------------
// real GUI behind the seam
// ---------------------------------------------------------------------------------------------

struct RealCreateGui<'a> {
    session: &'a GuiSession,
    profile: &'static Profile,
}

impl CreateGui for RealCreateGui<'_> {
    fn exists(&mut self, name: &str) -> Result<bool> {
        check_exists(self.session, self.profile, name)
    }

    fn edit_columns(&mut self, columns: &[String]) -> Result<PickReport> {
        open_editor(self.session, self.profile)?;
        match self.profile.style {
            EditorStyle::Grid => {
                let mut screen = RealGridScreen::new(self.session, 1, GRID_EDITOR_BASE);
                pick_columns_grid(&mut screen, columns, GRID_ROW_LIMIT)
            }
            EditorStyle::List => {
                let mut screen = RealListScreen::new(self.session, 1, LIST_EDITOR_BASE);
                pick_columns_list(&mut screen, columns)
            }
        }
    }

    fn cancel_editor(&mut self) -> Result<()> {
        cancel_all_popups(self.session)
    }

    fn save(&mut self, name: &str, description: &str, overwrite: bool) -> Result<SaveResult> {
        match self.profile.save {
            SaveFlow::Grid => save_grid(self.session, name, description, overwrite),
            SaveFlow::List { save, dialog } => save_list(
                self.session,
                self.profile,
                &save,
                dialog,
                name,
                description,
                overwrite,
            ),
        }
    }

    fn cleanup(&mut self) {
        if let Err(e) = cancel_all_popups(self.session) {
            println!("Could not close the layout dialogs: {e}");
        }
        let top = highest_popup(self.session);
        if top > 0 {
            println!(
                "Warning: dialog wnd[{top}] ('{}') is still open",
                window_title(self.session, top)
            );
        }
    }
}

// ---------------------------------------------------------------------------------------------
// standalone command / TUI
// ---------------------------------------------------------------------------------------------

/// Convert an error into a `windows` error (for the choosers that return `windows::core::Result`).
pub fn to_windows_error(e: &anyhow::Error) -> windows::core::Error {
    windows::core::Error::new(
        windows::Win32::Foundation::E_FAIL,
        windows::core::HSTRING::from(format!("{e:#}")),
    )
}

/// `--create-layout`: the session must show the report of `tcode`; loads
/// `[layouts.<tcode>.<name>]` from config.toml and creates it. Never prompts.
pub fn create_layout_on_current_screen(
    session: &GuiSession,
    tcode: &str,
    name: &str,
    overwrite: bool,
) -> Result<CreateOutcome> {
    let profile = profile_for(tcode)?;
    let store = LayoutStore::load();
    let spec = store
        .lookup(tcode, name)
        .map_err(|e| anyhow!("{e}"))?
        .clone();
    let actual = session.info()?.transaction()?;
    if !actual.trim().eq_ignore_ascii_case(profile.tcode) {
        return Err(LayoutError::WrongTransaction {
            expected: profile.tcode.to_string(),
            actual,
        }
        .into());
    }
    create_layout(session, profile.tcode, &spec, CreateOpts { overwrite })
}

// ---------------------------------------------------------------------------------------------
// opt-in auto-create during export runs
// ---------------------------------------------------------------------------------------------

/// Is auto-create on? CLI `--create-missing-layout` > `[global] create_missing_layout` > off.
pub fn auto_create_enabled() -> bool {
    static CONFIG_FLAG: OnceLock<Option<bool>> = OnceLock::new();
    if let Some(v) = crate::utils::cli_overrides::create_missing_layout_flag() {
        return v;
    }
    CONFIG_FLAG
        .get_or_init(|| {
            crate::utils::config_types::SapConfig::load()
                .ok()
                .and_then(|c| {
                    crate::utils::layout_store::global_create_missing_layout(c.raw_config.as_ref())
                })
        })
        .unwrap_or(false)
}

/// Result of [`auto_create_on_missing`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AutoCreate {
    /// The layout was created and saved; it is applied to the displayed list.
    Applied,
    /// The layout exists in SAP after all (e.g. another session just created it): select it.
    Retry,
    /// Nothing was done. `Some(hint)` explains why when the feature is on (no column list, ...).
    Skipped(Option<String>),
    /// Creation was attempted and failed.
    Failed(String),
}

impl AutoCreate {
    /// Text to append to a "layout not found" error.
    pub fn hint(&self) -> String {
        match self {
            AutoCreate::Skipped(Some(h)) => format!(" (create_missing_layout is on, but {h})"),
            AutoCreate::Failed(m) => format!(" (creating it failed: {m})"),
            _ => String::new(),
        }
    }
}

/// One shared helper for every layout chooser: the layout `layout` of `tcode` was not found. When
/// auto-create is on and `[layouts.<TCODE>.<layout>]` exists, create and save it. The result
/// list must be displayed (open dialogs are closed first).
pub fn auto_create_on_missing(session: &GuiSession, tcode: &str, layout: &str) -> AutoCreate {
    if !auto_create_enabled() {
        return AutoCreate::Skipped(None);
    }
    let name = layout.trim();
    let tcode_u = tcode.trim().to_uppercase();
    if let Err(e) = profile_for(&tcode_u) {
        return AutoCreate::Skipped(Some(e.to_string()));
    }
    let store = LayoutStore::load();
    let spec = match store.lookup(&tcode_u, name) {
        Ok(s) => s.clone(),
        Err(e) => {
            println!("Layout '{name}' not found and cannot be created: {e}");
            return AutoCreate::Skipped(Some(e.to_string()));
        }
    };
    println!(
        "Layout '{name}' not found for {tcode_u}: creating it from [layouts.{tcode_u}.{name}]"
    );
    let _ = cancel_all_popups(session);
    match create_layout(session, &tcode_u, &spec, CreateOpts::default()) {
        Ok(CreateOutcome::Created) | Ok(CreateOutcome::Overwritten) => AutoCreate::Applied,
        Ok(CreateOutcome::Exists) => AutoCreate::Retry,
        Err(e) => AutoCreate::Failed(format!("{e:#}")),
    }
}

// ---------------------------------------------------------------------------------------------
// tests
// ---------------------------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_supported_tcode_has_a_consistent_profile() {
        let all = supported_tcodes();
        assert_eq!(
            all,
            [
                "ZMDESNR",
                "ZVT11",
                "Y_DN3_47000149",
                "VT11",
                "VL06O",
                "LT23",
                "LX03",
                "LX02"
            ]
        );
        for t in all {
            let p = profile_for(t).unwrap();
            assert_eq!(p.tcode, t);
            assert!(!p.open_editor.is_empty(), "{t}");
            assert!(!p.choose_layout.is_empty(), "{t}");
            match (p.style, p.save) {
                (EditorStyle::Grid, SaveFlow::Grid) => {}
                (EditorStyle::List, SaveFlow::List { .. }) => {}
                other => panic!("{t}: editor and save flow do not match: {other:?}"),
            }
            // every control id is a window-0 path
            for c in p.open_editor.iter().chain(p.choose_layout) {
                assert!(c.id().starts_with("wnd[0]/"), "{t}: {}", c.id());
            }
        }
    }

    #[test]
    fn profile_values_follow_the_macro() {
        let zm = profile_for("zmdesnr").unwrap();
        assert_eq!(zm.style, EditorStyle::Grid);
        assert_eq!(
            zm.open_editor,
            [
                Control::Button("wnd[0]/tbar[1]/btn[32]"),
                Control::Menu("wnd[0]/mbar/menu[4]/menu[0]/menu[0]")
            ]
        );
        let z = profile_for(" ZVT11 ").unwrap();
        assert_eq!(z.open_editor, [Control::Button("wnd[0]/tbar[1]/btn[32]")]);
        let n = profile_for("y_dn3_47000149").unwrap();
        assert_eq!(
            n.open_editor[1],
            Control::Menu("wnd[0]/mbar/menu[3]/menu[0]/menu[0]")
        );
        let vt = profile_for("VT11").unwrap();
        assert_eq!(vt.style, EditorStyle::List);
        assert_eq!(
            vt.open_editor,
            [Control::Menu("wnd[0]/mbar/menu[3]/menu[0]/menu[0]")]
        );
        assert_eq!(
            vt.save,
            SaveFlow::List {
                save: Control::Menu("wnd[0]/mbar/menu[3]/menu[0]/menu[3]"),
                dialog: ListDialog::UserSpecific
            }
        );
        for t in ["VL06O", "LT23"] {
            let p = profile_for(t).unwrap();
            assert_eq!(
                p.open_editor,
                [
                    Control::Menu("wnd[0]/mbar/menu[3]/menu[2]/menu[0]"),
                    Control::Button("wnd[0]/tbar[1]/btn[32]")
                ]
            );
            assert_eq!(
                p.save,
                SaveFlow::List {
                    save: Control::Menu("wnd[0]/mbar/menu[3]/menu[2]/menu[3]"),
                    dialog: ListDialog::UserSpecific
                }
            );
        }
        for t in ["LX03", "LX02"] {
            let p = profile_for(t).unwrap();
            assert_eq!(p.max_name_len, Some(12));
            assert_eq!(
                p.save,
                SaveFlow::List {
                    save: Control::Button("wnd[0]/tbar[1]/btn[36]"),
                    dialog: ListDialog::Plain
                }
            );
        }
    }

    #[test]
    fn menus_and_buttons_are_not_mixed_up() {
        // the open/choose/save controls that the macro selects as menus are menus here
        let vt = profile_for("VT11").unwrap();
        assert!(matches!(vt.open_editor[0], Control::Menu(_)));
        assert!(matches!(vt.choose_layout[0], Control::Menu(_)));
        let lx = profile_for("LX03").unwrap();
        assert!(matches!(lx.choose_layout[0], Control::Button(_)));
    }

    #[test]
    fn unsupported_tcodes_are_a_clear_error() {
        for t in ["MB52", "MB51", "MM60", "ZMDETPC", "", "nope"] {
            let e = profile_for(t).unwrap_err();
            let msg = e.to_string();
            assert!(
                msg.contains("cannot be created") && msg.contains("VT11"),
                "{msg}"
            );
        }
    }

    #[test]
    fn name_rules() {
        let lx = profile_for("LX03").unwrap();
        assert!(check_name(lx, "TWELVE_CHARS").is_ok());
        let e = check_name(lx, "THIRTEEN_CHAR").unwrap_err();
        assert!(matches!(e, LayoutError::NameTooLong { max: 12, .. }));
        assert!(e.to_string().contains("at most 12"));
        assert_eq!(check_name(lx, "  "), Err(LayoutError::EmptyName));
        let vt = profile_for("VT11").unwrap();
        assert!(check_name(vt, "A_VERY_LONG_NAME_FOR_VT11").is_ok());
    }

    #[test]
    fn overwrite_decision() {
        assert_eq!(decide_overwrite(false, false), PopupAnswer::NoPopup);
        assert_eq!(decide_overwrite(false, true), PopupAnswer::NoPopup);
        assert_eq!(decide_overwrite(true, false), PopupAnswer::No);
        assert_eq!(decide_overwrite(true, true), PopupAnswer::Yes);
    }

    #[test]
    fn outcome_mapping() {
        let r = |popup_shown, answer| SaveResult {
            popup_shown,
            answer,
        };
        assert_eq!(
            outcome_of(false, r(false, PopupAnswer::NoPopup)),
            CreateOutcome::Created
        );
        assert_eq!(
            outcome_of(true, r(true, PopupAnswer::Yes)),
            CreateOutcome::Overwritten
        );
        assert_eq!(
            outcome_of(true, r(false, PopupAnswer::NoPopup)),
            CreateOutcome::Overwritten
        );
        assert_eq!(
            outcome_of(false, r(true, PopupAnswer::No)),
            CreateOutcome::Exists
        );
    }

    #[test]
    fn names_are_compared_trimmed_and_case_insensitive() {
        let n = vec!["  MY_LAYOUT ".to_string(), "Other".to_string()];
        assert!(names_contain(&n, "my_layout"));
        assert!(names_contain(&n, " other"));
        assert!(!names_contain(&n, "my"));
        assert!(!names_contain(&n, ""));
    }

    #[test]
    fn outcome_text() {
        assert!(CreateOutcome::Exists
            .describe("X")
            .contains("exists, skipped; pass --overwrite to replace it"));
    }

    // ---- create_core with a fake GUI -------------------------------------------------------

    #[derive(Default)]
    struct FakeGui {
        existing: bool,
        missing: Vec<String>,
        popup_on_save: bool,
        fail_edit: bool,
        calls: Vec<String>,
        answered: Option<PopupAnswer>,
    }

    impl CreateGui for FakeGui {
        fn exists(&mut self, name: &str) -> Result<bool> {
            self.calls.push(format!("exists:{name}"));
            Ok(self.existing)
        }
        fn edit_columns(&mut self, columns: &[String]) -> Result<PickReport> {
            self.calls.push("edit".to_string());
            if self.fail_edit {
                return Err(anyhow!("editor did not open"));
            }
            Ok(PickReport {
                moved: columns.to_vec(),
                missing: self.missing.clone(),
                ..Default::default()
            })
        }
        fn cancel_editor(&mut self) -> Result<()> {
            self.calls.push("cancel_editor".to_string());
            Ok(())
        }
        fn save(&mut self, _n: &str, _d: &str, overwrite: bool) -> Result<SaveResult> {
            self.calls.push("save".to_string());
            let answer = decide_overwrite(self.popup_on_save, overwrite);
            self.answered = Some(answer);
            Ok(SaveResult {
                popup_shown: self.popup_on_save,
                answer,
            })
        }
        fn cleanup(&mut self) {
            self.calls.push("cleanup".to_string());
        }
    }

    fn spec() -> LayoutSpec {
        LayoutSpec {
            tcode: "VT11".to_string(),
            name: "EXAMPLE".to_string(),
            columns: vec!["A".to_string(), "B".to_string()],
            description: "EXAMPLE".to_string(),
        }
    }

    #[test]
    fn existing_layout_without_overwrite_changes_nothing() {
        let mut g = FakeGui {
            existing: true,
            ..Default::default()
        };
        let o = create_core(&mut g, "VT11", &spec(), false).unwrap();
        assert_eq!(o, CreateOutcome::Exists);
        // no editor, no save
        assert_eq!(g.calls, ["exists:EXAMPLE", "cleanup"]);
    }

    #[test]
    fn existing_layout_with_overwrite_is_replaced() {
        let mut g = FakeGui {
            existing: true,
            popup_on_save: true,
            ..Default::default()
        };
        let o = create_core(&mut g, "VT11", &spec(), true).unwrap();
        assert_eq!(o, CreateOutcome::Overwritten);
        assert_eq!(g.answered, Some(PopupAnswer::Yes));
    }

    #[test]
    fn new_layout_is_created() {
        let mut g = FakeGui::default();
        let o = create_core(&mut g, "VT11", &spec(), false).unwrap();
        assert_eq!(o, CreateOutcome::Created);
        assert_eq!(g.calls, ["exists:EXAMPLE", "edit", "save", "cleanup"]);
    }

    #[test]
    fn overwrite_popup_without_permission_is_answered_no() {
        // the existence check said "free", but SAP raises the popup anyway
        let mut g = FakeGui {
            popup_on_save: true,
            ..Default::default()
        };
        let o = create_core(&mut g, "VT11", &spec(), false).unwrap();
        assert_eq!(o, CreateOutcome::Exists);
        assert_eq!(g.answered, Some(PopupAnswer::No));
        assert_eq!(g.calls.last().unwrap(), "cleanup");
    }

    #[test]
    fn missing_columns_cancel_the_editor_and_save_nothing() {
        let mut g = FakeGui {
            missing: vec!["B".to_string()],
            ..Default::default()
        };
        let e = create_core(&mut g, "VT11", &spec(), false).unwrap_err();
        let msg = e.to_string();
        assert!(msg.contains("NOT saved") && msg.contains('B'), "{msg}");
        assert!(g.calls.contains(&"cancel_editor".to_string()));
        assert!(!g.calls.contains(&"save".to_string()));
        assert_eq!(g.calls.last().unwrap(), "cleanup");
    }

    #[test]
    fn errors_still_clean_up() {
        let mut g = FakeGui {
            fail_edit: true,
            ..Default::default()
        };
        assert!(create_core(&mut g, "VT11", &spec(), false).is_err());
        assert_eq!(g.calls.last().unwrap(), "cleanup");
    }

    #[test]
    fn auto_create_hints() {
        assert_eq!(AutoCreate::Skipped(None).hint(), "");
        assert!(AutoCreate::Skipped(Some("no [layouts.X.Y] section".into()))
            .hint()
            .contains("no [layouts.X.Y] section"));
        assert!(AutoCreate::Failed("boom".into()).hint().contains("boom"));
        assert_eq!(AutoCreate::Applied.hint(), "");
    }
}
