//! LT03 (create transfer order for a delivery) over the SAP GUI: the real [`Lt03Screen`].
//!
//! Control ids, column indices and key sequences are taken from the `manual_to` macro, where they
//! ran live; the port has since been run live by the owner (dry run and real TO creation). Originally unverified:
//! * table `TBL` and its columns (SELMG 0, LGPLA 2, VERME 4, WDATU 9), the info fields
//!   `RL03T-AUSME/OFFME/VERME` and `LTAP-MATNR`;
//! * paging with Page Down / Page Up (VKeys 82 / 81) and the delivery-overview probe
//!   (`tabpANIT ... tblSAPML03TD1522`);
//! * the skip sequence (cancel `tbar[0]/btn[12]`, then next material `tbar[1]/btn[17]`, VKey 17 as
//!   fallback) and Shift+F4 (VKey 16) for "next material" after picks;
//! * [`RealLt03Screen::finish`] with `save = false` (dry run / nothing picked): leaving LT03 without
//!   saving is not in the macro; [`leave_without_saving`] follows a live recording (Cancel / F12
//!   until the delivery field is back). The stock overview layout (columns, GR date as
//!   `yyyy-mm-dd`) was checked against a screenshot.
#![allow(dead_code)]

use crate::lt03_runner::{EnterOutcome, Lt03Screen, MaterialInfo, OpenOutcome, PickTarget, RawRow};
use crate::utils::sap_ctrl_utils::{exist_ctrl, hit_ctrl};
use crate::utils::sap_tcode_utils::assert_tcode;
use anyhow::{anyhow, bail, Result};
use com_shim::HasIDispatch;
use sap_scripting::*;
use std::thread::sleep;
use std::time::Duration;

pub const TBL: &str =
    "wnd[0]/usr/tabsFUNC_TABSTRIP/tabpAQVB/ssubD0106_S:SAPML03T:1061/tblSAPML03TD1061";
const OVERVIEW_PROBE: &str = "/usr/tabsFUNC_TABSTRIP/tabpANIT/ssubD0152_S:SAPML03T:1522/tblSAPML03TD1522/ctxtVBLKP-VBELN[0,0]";

pub const COL_SELMG: usize = 0;
pub const COL_LGPLA: usize = 2;
pub const COL_VERME: usize = 4;
pub const COL_WDATU: usize = 9;
pub const CID_SELMG: &str = "txtRL03T-SELMG";
pub const CID_LGPLA: &str = "ctxtLQUA-LGPLA";
pub const CID_VERME: &str = "txtLQUA-VERME";
pub const CID_WDATU: &str = "ctxtLQUA-WDATU";

const F_AUSME: &str = "wnd[0]/usr/txtRL03T-AUSME";
const F_OFFME: &str = "wnd[0]/usr/txtRL03T-OFFME";
const F_VERME: &str = "wnd[0]/usr/txtRL03T-VERME";
const F_MATNR: &str = "wnd[0]/usr/ctxtLTAP-MATNR";

fn pause(ms: u64) {
    sleep(Duration::from_millis(ms));
}

fn qty(s: &str) -> f64 {
    s.trim().replace(',', "").parse::<f64>().unwrap_or(0.0)
}

/// The LT03 screen of one SAP session.
pub struct RealLt03Screen<'a> {
    pub session: &'a GuiSession,
    cols: std::cell::Cell<Option<Cols>>,
}

/// Column indices of the stock table (resolved by field name, see `resolve_cols`).
#[derive(Clone, Copy)]
struct Cols {
    selmg: usize,
    lgpla: usize,
    verme: usize,
    wdatu: usize,
}

impl Default for Cols {
    fn default() -> Self {
        Self {
            selmg: COL_SELMG,
            lgpla: COL_LGPLA,
            verme: COL_VERME,
            wdatu: COL_WDATU,
        }
    }
}

thread_local! {
    static LAST_COLS: std::cell::RefCell<String> = const { std::cell::RefCell::new(String::new()) };
}

impl<'a> RealLt03Screen<'a> {
    pub fn new(session: &'a GuiSession) -> Self {
        Self {
            session,
            cols: std::cell::Cell::new(None),
        }
    }

    fn cell_id(ctl: &str, col: usize, row: usize) -> String {
        format!("{TBL}/{ctl}[{col},{row}]")
    }

    /// Text of any text-like control; empty when it does not exist.
    fn text(&self, id: &str) -> String {
        let Ok(c) = self.session.find_by_id(id.to_string()) else {
            return String::new();
        };
        if let Some(t) = c.downcast::<GuiTextField>() {
            return t.text().unwrap_or_default();
        }
        if let Some(t) = c.downcast::<GuiCTextField>() {
            return t.text().unwrap_or_default();
        }
        if let Some(t) = c.downcast::<GuiLabel>() {
            return t.text().unwrap_or_default();
        }
        String::new()
    }

    fn set_cell(&self, id: &str, value: &str) -> Result<()> {
        let c = self
            .session
            .find_by_id(id.to_string())
            .map_err(|e| anyhow!("{id}: {e}"))?;
        if let Some(t) = c.downcast::<GuiTextField>() {
            t.set_text(value.to_string())?;
            return Ok(());
        }
        if let Some(t) = c.downcast::<GuiCTextField>() {
            t.set_text(value.to_string())?;
            return Ok(());
        }
        bail!("{id} is not a text field")
    }

    fn status(&self) -> String {
        hit_ctrl(self.session, 0, "/sbar", "Text", "Get", "").unwrap_or_default()
    }

    fn vkey(&self, key: i16) -> Result<()> {
        let w = self
            .session
            .find_by_id("wnd[0]".to_string())
            .map_err(|e| anyhow!("wnd[0]: {e}"))?;
        // wnd[0] is a GuiMainWindow; other windows are modal / frame windows
        if let Some(m) = w.downcast::<GuiMainWindow>() {
            m.send_v_key(key)?;
        } else if let Some(f) = w.downcast::<GuiFrameWindow>() {
            f.send_v_key(key)?;
        } else if let Some(m) = w.downcast::<GuiModalWindow>() {
            m.send_v_key(key)?;
        } else {
            bail!("wnd[0] is not a window that accepts keys");
        }
        Ok(())
    }

    fn press(&self, id: &str) -> Result<()> {
        let c = self
            .session
            .find_by_id(id.to_string())
            .map_err(|e| anyhow!("{id}: {e}"))?;
        c.downcast::<GuiButton>()
            .ok_or_else(|| anyhow!("{id} is not a button"))?
            .press()?;
        Ok(())
    }

    fn table(&self) -> Option<GuiTableControl> {
        self.session
            .find_by_id(TBL.to_string())
            .ok()
            .and_then(|c| c.downcast::<GuiTableControl>())
    }

    fn page_size(&self) -> usize {
        self.table()
            .and_then(|t| t.visible_row_count().ok().or_else(|| t.row_count().ok()))
            .map(|n| n.max(0) as usize)
            .unwrap_or(0)
    }

    /// Scrollbar position of the table (index of the first visible row) when readable.
    fn top_row(&self) -> Option<usize> {
        let t = self.table()?;
        let sb = GuiScrollbar::from(t.vertical_scrollbar().ok()?.get_idispatch().clone());
        sb.position().ok().map(|p| p.max(0) as usize)
    }

    /// Column indices by field name, read from the names of the cells in the table's first
    /// visible row (`LQUA-WDATU[9,0]` -> field `LQUA-WDATU`, column 9). A field the screen does
    /// not show keeps the built-in index. The result is printed once per run.
    fn scan_cols(&self) -> Cols {
        let mut cols = Cols::default();
        let Some(t) = self.table() else {
            return cols;
        };
        let mut seen: Vec<String> = Vec::new();
        for c in 0..40 {
            let Ok(cell) = t.get_cell(0, c) else { break };
            let Ok(name) = cell.name() else { continue };
            let field = name.split('[').next().unwrap_or("").to_string();
            match field.as_str() {
                "RL03T-SELMG" => cols.selmg = c as usize,
                "LQUA-LGPLA" => cols.lgpla = c as usize,
                "LQUA-VERME" => cols.verme = c as usize,
                "LQUA-WDATU" => cols.wdatu = c as usize,
                _ => {}
            }
            seen.push(format!("{c}={field}"));
        }
        if !seen.is_empty() {
            let line = seen.join(" ");
            if LAST_COLS.with(|l| l.replace(line.clone()) != line) {
                println!(
                    "LT03 stock table columns: {line} -> using SELMG={} LGPLA={} VERME={} WDATU={}",
                    cols.selmg, cols.lgpla, cols.verme, cols.wdatu
                );
            }
        }
        cols
    }

    fn resolve_cols(&self) -> Cols {
        if let Some(c) = self.cols.get() {
            return c;
        }
        let c = self.scan_cols();
        self.cols.set(Some(c));
        c
    }

    fn bin_at(&self, row: usize) -> String {
        self.text(&Self::cell_id(CID_LGPLA, self.resolve_cols().lgpla, row))
            .trim()
            .to_string()
    }

    /// Page Down; true when the top row changed.
    fn page_down(&self) -> bool {
        let before = self.bin_at(0);
        let _ = self.vkey(82);
        pause(60);
        self.bin_at(0) != before
    }

    fn page_up(&self) -> bool {
        let before = self.bin_at(0);
        let _ = self.vkey(81);
        pause(60);
        self.bin_at(0) != before
    }

    fn first_page(&self) {
        let mut guard = 0;
        while self.page_up() {
            guard += 1;
            if guard > 100 {
                break;
            }
        }
    }
}

impl Lt03Screen for RealLt03Screen<'_> {
    fn open_delivery(&mut self, warehouse: &str, delivery: &str) -> Result<OpenOutcome> {
        if !assert_tcode(self.session, "lt03", None)? {
            bail!("LT03 could not be started (status: {})", self.status());
        }
        self.set_cell("wnd[0]/usr/ctxtLTAK-LGNUM", warehouse)?;
        self.set_cell("wnd[0]/usr/ctxtVBLKK-VBELN", delivery)?;
        self.vkey(0)?;
        let bar = self.status();
        let low = bar.to_ascii_lowercase();
        if low.contains("no items that are to be picked") {
            return Ok(OpenOutcome::NoItems(bar));
        }
        if low.contains("currently being processed") {
            return Ok(OpenOutcome::InProcess(bar));
        }
        self.vkey(18)?; // Shift+F6: stock figures
        pause(500);
        Ok(OpenOutcome::Ready)
    }

    fn current_material(&mut self) -> Result<Option<MaterialInfo>> {
        if !exist_ctrl(self.session, 0, "/usr/ctxtLTAP-MATNR", true)?.cband {
            return Ok(None);
        }
        let material = self.text(F_MATNR).trim().to_string();
        if material.is_empty() {
            return Ok(None);
        }
        Ok(Some(MaterialInfo {
            material,
            need: qty(&self.text(F_AUSME)),
            open: qty(&self.text(F_OFFME)),
            avail: qty(&self.text(F_VERME)),
        }))
    }

    fn read_stock(&mut self) -> Result<Vec<RawRow>> {
        self.cols.set(None); // re-read the column layout for every stock table
        let cols = self.resolve_cols();
        let ps = self.page_size();
        if ps == 0 {
            return Ok(Vec::new());
        }
        self.first_page();
        let mut rows: Vec<RawRow> = Vec::new();
        let mut page_base = 0usize;
        loop {
            let base = self.top_row().unwrap_or(page_base);
            for r in 0..ps {
                let bin = self.bin_at(r);
                if bin.is_empty() {
                    continue;
                }
                let abs_row = base + r;
                // overlapping last page: a row already read keeps its first copy
                if rows.iter().any(|x| x.abs_row == abs_row) {
                    continue;
                }
                rows.push(RawRow {
                    abs_row,
                    bin,
                    avail: qty(&self.text(&Self::cell_id(CID_VERME, cols.verme, r))),
                    sel: qty(&self.text(&Self::cell_id(CID_SELMG, cols.selmg, r))),
                    gr_text: self
                        .text(&Self::cell_id(CID_WDATU, cols.wdatu, r))
                        .trim()
                        .to_string(),
                });
            }
            if !self.page_down() {
                break;
            }
            page_base += ps;
        }
        self.first_page();
        Ok(rows)
    }

    fn enter_pick(&mut self, abs_row: usize, bin: &str, qty_v: f64) -> Result<bool> {
        let ps = self.page_size();
        if ps == 0 || qty_v <= 0.0 {
            return Ok(false);
        }
        // go to the row's page: scrollbar when available, else page down from the top
        self.first_page();
        let mut top = 0usize;
        if let Some(t) = self.table() {
            if let Ok(sb) = t.vertical_scrollbar() {
                let sb = GuiScrollbar::from(sb.get_idispatch().clone());
                let _ = sb.set_position(abs_row as i32);
                pause(80);
                top = self.top_row().unwrap_or(0);
            }
        }
        if top == 0 && abs_row >= ps {
            for _ in 0..(abs_row / ps) {
                if !self.page_down() {
                    break;
                }
            }
            top = self.top_row().unwrap_or((abs_row / ps) * ps);
        }
        let mut local = abs_row.checked_sub(top).filter(|l| *l < ps);
        if local.is_none_or(|l| self.bin_at(l) != bin) {
            // landed elsewhere: search the visible page, then forward (bounded)
            local = None;
            'search: for _ in 0..50 {
                for r in 0..ps {
                    if self.bin_at(r) == bin {
                        local = Some(r);
                        break 'search;
                    }
                }
                if !self.page_down() {
                    break;
                }
            }
        }
        let Some(local) = local else {
            return Ok(false);
        };
        self.set_cell(
            &Self::cell_id(CID_SELMG, self.resolve_cols().selmg, local),
            &qty_v.to_string(),
        )?;
        self.vkey(0)?;
        Ok(true)
    }

    fn begin_pick_pass(&mut self) -> Result<()> {
        self.first_page();
        Ok(())
    }

    /// Checked entry for the two-pass flow. No searching: the row `abs_row` is brought into view
    /// (scrollbar position, or page down when the scrollbar cannot be read), then bin, available
    /// and selected quantity must equal what the read pass saw, the typed value must read back
    /// unchanged, and only then Enter is pressed. Anything else is a `Mismatch`.
    fn enter_pick_strict(&mut self, t: &PickTarget) -> Result<EnterOutcome> {
        let mismatch = |why: String| Ok(EnterOutcome::Mismatch(why));
        let ps = self.page_size();
        if ps == 0 {
            return mismatch("stock table not found".to_string());
        }
        if t.qty <= 0.0 {
            return mismatch(format!("quantity {} is not positive", t.qty));
        }
        let visible = |top: Option<usize>| {
            top.and_then(|tp| t.abs_row.checked_sub(tp))
                .filter(|l| *l < ps)
        };
        let mut top = self.top_row();
        let mut local = visible(top);
        if local.is_none() {
            if let Some(tb) = self.table() {
                if let Ok(sb) = tb.vertical_scrollbar() {
                    let sb = GuiScrollbar::from(sb.get_idispatch().clone());
                    let _ = sb.set_position(t.abs_row as i32);
                    pause(80);
                }
            }
            top = self.top_row();
            local = visible(top);
        }
        if local.is_none() && top.is_none() {
            // scrollbar position unreadable: page down from the first page, the row checks
            // below catch a wrong landing
            self.first_page();
            for _ in 0..(t.abs_row / ps) {
                if !self.page_down() {
                    break;
                }
            }
            local = Some(t.abs_row % ps);
        }
        let Some(local) = local else {
            return mismatch(format!("row {} could not be scrolled into view", t.abs_row));
        };
        let cols = self.resolve_cols();
        let bin_now = self.bin_at(local);
        if bin_now != t.bin {
            return mismatch(format!(
                "row {} shows bin '{bin_now}', expected '{}'",
                t.abs_row, t.bin
            ));
        }
        let avail_now = qty(&self.text(&Self::cell_id(CID_VERME, cols.verme, local)));
        if (avail_now - t.avail).abs() > 1e-6 {
            return mismatch(format!(
                "row {} shows {avail_now} available, expected {}",
                t.abs_row, t.avail
            ));
        }
        let sel_id = Self::cell_id(CID_SELMG, cols.selmg, local);
        let sel_now = qty(&self.text(&sel_id));
        if (sel_now - t.sel_before).abs() > 1e-6 {
            return mismatch(format!(
                "row {} shows {sel_now} selected, expected {}",
                t.abs_row, t.sel_before
            ));
        }
        self.set_cell(&sel_id, &t.qty.to_string())?;
        let back = qty(&self.text(&sel_id));
        if (back - t.qty).abs() > 1e-6 {
            return mismatch(format!(
                "typed {} into row {} but the cell shows {back}",
                t.qty, t.abs_row
            ));
        }
        self.vkey(0)?;
        Ok(EnterOutcome::Entered)
    }

    fn open_qty(&mut self) -> Result<f64> {
        Ok(qty(&self.text(F_OFFME)))
    }

    fn next_material(&mut self) -> Result<()> {
        self.vkey(16)?; // Shift+F4
        pause(200);
        Ok(())
    }

    fn skip_material(&mut self) -> Result<()> {
        let old = self.text(F_MATNR).trim().to_string();
        let _ = self.press("wnd[0]/tbar[0]/btn[12]"); // cancel the edits
        pause(100);
        let _ = self.press("wnd[0]/tbar[1]/btn[17]"); // Shift+F5: next material
        pause(200);
        let new = self.text(F_MATNR).trim().to_string();
        if !old.is_empty() && new == old {
            let _ = self.vkey(17); // fallback
            pause(200);
        }
        Ok(())
    }

    fn at_overview(&mut self) -> Result<bool> {
        Ok(exist_ctrl(self.session, 0, OVERVIEW_PROBE, true)?.cband)
    }

    fn finish(&mut self, save: bool) -> Result<String> {
        if !save {
            return leave_without_saving(self);
        }
        self.press("wnd[0]/tbar[0]/btn[11]")?;
        if exist_ctrl(self.session, 1, "", true)?.cband {
            if let Ok(w) = self.session.find_by_id("wnd[1]".to_string()) {
                if let Some(m) = w.downcast::<GuiModalWindow>() {
                    m.send_v_key(0)?;
                }
            }
        }
        Ok(self.status())
    }
}

/// The initial screen: the delivery field is there and the tab strip of the delivery screens is
/// not. (The delivery number is also shown on the later screens, so the field alone is not enough:
/// that stopped one Cancel too early.)
fn at_start_screen(screen: &RealLt03Screen) -> Result<bool> {
    Ok(
        exist_ctrl(screen.session, 0, "/usr/ctxtVBLKK-VBELN", true)?.cband
            && !exist_ctrl(screen.session, 0, "/usr/tabsFUNC_TABSTRIP", true)?.cband,
    )
}

/// Leave LT03 without creating a transfer order: Cancel (F12, `tbar[0]/btn[12]`) until the initial
/// screen with the delivery field is showing again (from the stock overview that is up to three
/// presses; seen in a live recording). A popup that shows up on the way is answered with "No"
/// (`SPOP-OPTION2`), else cancelled with F12. Nothing is saved on this path.
fn leave_without_saving(screen: &RealLt03Screen) -> Result<String> {
    for _ in 0..8 {
        if at_start_screen(screen)? {
            break;
        }
        if exist_ctrl(screen.session, 1, "", true)?.cband {
            if screen.press("wnd[1]/usr/btnSPOP-OPTION2").is_err() {
                if let Ok(w) = screen.session.find_by_id("wnd[1]".to_string()) {
                    if let Some(m) = w.downcast::<GuiModalWindow>() {
                        let _ = m.send_v_key(12);
                    }
                }
            }
        } else {
            let _ = screen.press("wnd[0]/tbar[0]/btn[12]");
        }
        pause(200);
    }
    if !at_start_screen(screen)? {
        bail!("could not get back to the LT03 start screen without saving");
    }
    Ok(screen.status())
}
