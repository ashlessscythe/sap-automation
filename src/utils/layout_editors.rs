//! Column pickers of the two SAP ALV layout editors, behind small seam traits.
//!
//! * grid-style editor (two `GuiGridView`s: displayed columns on the left, column set on the
//!   right; ZMDESNR, ZVT11, the 149 report): [`GridScreen`] / [`pick_columns_grid`],
//! * list-style editor (two `GuiTableControl`s; VT11, VL06O, LT23, LX03, LX02):
//!   [`ListScreen`] / [`pick_columns_list`].
//!
//! The algorithms are pure (they only talk to the seam, so tests use a fake, see `fakes`); the
//! real implementations ([`RealGridScreen`], [`RealListScreen`]) go over the SAP GUI scripting
//! API. UNVERIFIED live: control ids and behaviour come from the VBA macro (`SetupLayout`,
//! `SetupLayout_li` of `Control_id_.bas`).
//!
//! A table control object is stale after any scroll or button press, so the real list screen
//! looks the table up again (`find_by_id`) on every single access.
#![allow(dead_code)]

use anyhow::{anyhow, Result};
use com_shim::HasIDispatch;
use sap_scripting::*;
use std::thread;
use std::time::Duration;

/// Cap of "page down" steps of one scan (a layout has far fewer fields than this).
pub const MAX_PAGES: i32 = 200;
/// Cap of "select everything, move right" rounds when clearing the displayed list.
pub const MAX_CLEAR_ROUNDS: usize = 20;
/// Row limit of the grid editor scan (like the macro's `limit`).
pub const GRID_ROW_LIMIT: i32 = 200;

/// Blank filler of a table row: empty or only underscores.
pub fn is_blank_cell(text: &str) -> bool {
    let t = text.trim();
    t.is_empty() || t.chars().all(|c| c == '_')
}

/// Column titles match when equal after trimming, ignoring case.
pub fn same_column(a: &str, b: &str) -> bool {
    a.trim().to_uppercase() == b.trim().to_uppercase()
}

/// What a column picker did.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct PickReport {
    /// Requested columns that were moved to the displayed list.
    pub moved: Vec<String>,
    /// Requested columns that were already displayed after clearing (key columns that SAP does
    /// not let one remove); not missing.
    pub already_present: Vec<String>,
    /// Requested columns that could not be found in the column set.
    pub missing: Vec<String>,
    /// Columns that stay displayed although they were not requested (not removable).
    pub kept_extra: Vec<String>,
}

impl PickReport {
    pub fn is_complete(&self) -> bool {
        self.missing.is_empty()
    }
}

// ---------------------------------------------------------------------------------------------
// list-style editor
// ---------------------------------------------------------------------------------------------

/// The two tables of the list-style editor.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Side {
    /// Displayed columns (`tblSAPLSKBHTC_WRITE_LIST`).
    Left,
    /// Column set to choose from (`tblSAPLSKBHTC_FIELD_LIST`).
    Right,
}

/// Seam over the list-style editor. Rows are addressed by their VISIBLE index (0-based) on the
/// current page; the scroll position is the absolute index of the first visible row.
pub trait ListScreen {
    fn visible_rows(&mut self, side: Side) -> Result<i32>;
    /// Text of the first column of a visible row ("" when the row has no cell).
    fn row_text(&mut self, side: Side, visible_row: i32) -> Result<String>;
    fn scroll_position(&mut self, side: Side) -> Result<i32>;
    /// Scroll so that `position` is the first visible row (SAP clamps at the end).
    fn set_scroll_position(&mut self, side: Side, position: i32) -> Result<()>;
    /// Select a visible row. `Ok(false)` = the row cannot be selected (locked column).
    fn select_row(&mut self, side: Side, visible_row: i32) -> Result<bool>;
    /// Take a visible row out of the selection again.
    fn deselect_row(&mut self, side: Side, visible_row: i32) -> Result<()>;
    /// Press the move button: `to == Right` moves the selected displayed rows away, `to == Left`
    /// moves the selected column-set rows into the layout.
    fn move_selected(&mut self, to: Side) -> Result<()>;
    /// Sort the column set alphabetically.
    fn sort_available(&mut self) -> Result<()>;
}

/// Walk the filled rows of `side` from the top, page by page, until a blank row or the end of
/// the table. `visit(screen, absolute_row, visible_row, text)` returns `true` to stop. The
/// table is left wherever the scan ended: callers reset the scroll position themselves.
pub fn scan_side<S: ListScreen>(
    s: &mut S,
    side: Side,
    visit: &mut dyn FnMut(&mut S, i32, i32, &str) -> Result<bool>,
) -> Result<()> {
    s.set_scroll_position(side, 0)?;
    let vrc = s.visible_rows(side)?.max(1);
    let mut pos = s.scroll_position(side)?;
    let mut last_abs = pos - 1;
    for _ in 0..MAX_PAGES {
        let mut reached_blank = false;
        for r in 0..vrc {
            let abs = pos + r;
            if abs <= last_abs {
                continue; // overlap after SAP clamped the last page
            }
            let text = s.row_text(side, r)?;
            if is_blank_cell(&text) {
                reached_blank = true;
                break;
            }
            last_abs = abs;
            if visit(s, abs, r, text.trim())? {
                return Ok(());
            }
        }
        if reached_blank {
            break;
        }
        s.set_scroll_position(side, pos + vrc)?;
        let next = s.scroll_position(side)?;
        if next == pos {
            break; // scrollbar did not move: end of the table
        }
        pos = next;
    }
    Ok(())
}

/// Texts of the filled rows of `side`.
pub fn filled_texts<S: ListScreen>(s: &mut S, side: Side) -> Result<Vec<String>> {
    let mut out = Vec::new();
    scan_side(s, side, &mut |_, _, _, t| {
        out.push(t.to_string());
        Ok(false)
    })?;
    s.set_scroll_position(side, 0)?;
    Ok(out)
}

/// Empty the displayed list: select every filled row (all pages), press move-right, repeat until
/// nothing is left or the round cap is hit. Rows that cannot be selected (locked key columns
/// such as `Delivery` in the VL06O item view) are left out of the selection. When a round moves
/// nothing (SAP refuses the whole move because a locked row is part of it), the rest is removed
/// one row at a time ([`clear_one_by_one`]). Returns the texts that stay displayed.
pub fn clear_left<S: ListScreen>(s: &mut S) -> Result<Vec<String>> {
    let mut previous: Option<usize> = None;
    for _ in 0..MAX_CLEAR_ROUNDS {
        let mut texts: Vec<String> = Vec::new();
        let mut selected = 0usize;
        scan_side(s, Side::Left, &mut |sc, _abs, vis, t| {
            if sc.select_row(Side::Left, vis)? {
                selected += 1;
            }
            texts.push(t.to_string());
            Ok(false)
        })?;
        s.set_scroll_position(Side::Left, 0)?;
        if texts.is_empty() || selected == 0 {
            return Ok(texts); // empty, or only locked rows are left
        }
        if previous == Some(texts.len()) {
            return clear_one_by_one(s); // nothing moved last round
        }
        previous = Some(texts.len());
        s.move_selected(Side::Right)?;
    }
    filled_texts(s, Side::Left)
}

/// Remove the displayed rows one at a time from the top; a row that is still there after its
/// own move is locked and stays. Returns the texts that stay displayed.
pub fn clear_one_by_one<S: ListScreen>(s: &mut S) -> Result<Vec<String>> {
    // drop whatever the failed bulk round left selected
    scan_side(s, Side::Left, &mut |sc, _abs, vis, _t| {
        sc.deselect_row(Side::Left, vis)?;
        Ok(false)
    })?;
    let mut kept: Vec<String> = Vec::new();
    let mut i = 0i32;
    for _ in 0..(MAX_PAGES * 10) {
        s.set_scroll_position(Side::Left, i)?;
        let vis = i - s.scroll_position(Side::Left)?;
        if vis < 0 || vis >= s.visible_rows(Side::Left)?.max(1) {
            break;
        }
        let text = s.row_text(Side::Left, vis)?;
        if is_blank_cell(&text) {
            break;
        }
        let text = text.trim().to_string();
        if !s.select_row(Side::Left, vis)? {
            println!("Column ({text}) is locked in the layout, left in place");
            kept.push(text);
            i += 1;
            continue;
        }
        s.move_selected(Side::Right)?;
        s.set_scroll_position(Side::Left, i)?;
        let vis = i - s.scroll_position(Side::Left)?;
        let after = if vis >= 0 {
            s.row_text(Side::Left, vis)?
        } else {
            String::new()
        };
        if same_column(&after, &text) {
            println!("Column ({text}) cannot be removed from the layout, left in place");
            s.deselect_row(Side::Left, vis)?;
            kept.push(text);
            i += 1;
        }
    }
    s.set_scroll_position(Side::Left, 0)?;
    Ok(kept)
}

/// Move the column `wanted` from the column set to the displayed list. `Ok(false)` = not found
/// (scrollbar stopped moving or a blank row was reached).
pub fn move_one<S: ListScreen>(s: &mut S, wanted: &str) -> Result<bool> {
    let mut found = false;
    scan_side(s, Side::Right, &mut |sc, _abs, vis, t| {
        if same_column(t, wanted) {
            if !sc.select_row(Side::Right, vis)? {
                return Ok(true); // there but not selectable: reported as not found
            }
            sc.move_selected(Side::Left)?;
            found = true;
            return Ok(true);
        }
        Ok(false)
    })?;
    s.set_scroll_position(Side::Right, 0)?;
    Ok(found)
}

/// Build the layout of the list-style editor: clear the displayed list, sort the column set,
/// then move the wanted columns one by one in the given order. A column that is already
/// displayed after clearing (not removable key column) counts as present, not as missing.
pub fn pick_columns_list<S: ListScreen>(s: &mut S, wanted: &[String]) -> Result<PickReport> {
    let remaining = clear_left(s)?;
    s.sort_available()?;
    let mut report = PickReport::default();
    for col in wanted {
        if remaining.iter().any(|r| same_column(r, col)) {
            println!("Column ({col}) is already in the layout");
            report.already_present.push(col.clone());
            continue;
        }
        if move_one(s, col)? {
            println!("Column ({col}) moved to the layout");
            report.moved.push(col.clone());
        } else {
            println!("Column ({col}) not found");
            report.missing.push(col.clone());
        }
    }
    report.kept_extra = remaining
        .into_iter()
        .filter(|r| !wanted.iter().any(|w| same_column(r, w)))
        .collect();
    Ok(report)
}

// ---------------------------------------------------------------------------------------------
// grid-style editor
// ---------------------------------------------------------------------------------------------

/// Seam over the grid-style editor.
pub trait GridScreen {
    /// Move every displayed column to the column set.
    fn clear_left(&mut self) -> Result<()>;
    /// Sort the column set by its text column (`SELTEXT`).
    fn sort_right(&mut self) -> Result<()>;
    fn right_row_count(&mut self) -> Result<i32>;
    fn right_text(&mut self, row: i32) -> Result<String>;
    /// Select the row of the column set and double-click it (moves it to the displayed list).
    fn add_right_row(&mut self, row: i32) -> Result<()>;
}

/// Build the layout of the grid-style editor: clear, sort, then double-click each wanted column
/// in order. Columns that are not found are reported, not skipped silently.
pub fn pick_columns_grid<S: GridScreen>(
    s: &mut S,
    wanted: &[String],
    limit: i32,
) -> Result<PickReport> {
    s.clear_left()?;
    s.sort_right()?;
    let mut report = PickReport::default();
    for col in wanted {
        let rows = s.right_row_count()?.min(limit.max(1));
        let mut hit = None;
        for i in 0..rows {
            let name = s.right_text(i).unwrap_or_default();
            if same_column(&name, col) {
                hit = Some(i);
                break;
            }
        }
        match hit {
            Some(i) => {
                s.add_right_row(i)?;
                println!("Column ({col}) moved to the layout");
                report.moved.push(col.clone());
            }
            None => {
                println!("Column ({col}) not found");
                report.missing.push(col.clone());
            }
        }
    }
    Ok(report)
}

// ---------------------------------------------------------------------------------------------
// real GUI implementations
// ---------------------------------------------------------------------------------------------

fn cell_text(c: &GuiComponent) -> String {
    if let Some(t) = c.downcast::<GuiTextField>() {
        return t.text().unwrap_or_default();
    }
    if let Some(t) = c.downcast::<GuiCTextField>() {
        return t.text().unwrap_or_default();
    }
    if let Some(l) = c.downcast::<GuiLabel>() {
        return l.text().unwrap_or_default();
    }
    String::new()
}

fn press_button(session: &GuiSession, id: &str) -> Result<()> {
    let c = session
        .find_by_id(id.to_string())
        .map_err(|e| anyhow!("button {id} not found: {e}"))?;
    let b = c
        .downcast::<GuiButton>()
        .ok_or_else(|| anyhow!("{id} is not a button"))?;
    b.press()?;
    Ok(())
}

/// List-style editor over the GUI: `base` is the object path below the window
/// (`/usr/tabsTS_LINES/tabpLI01/ssubSUB810:SAPLSKBH:0810`).
pub struct RealListScreen<'a> {
    session: &'a GuiSession,
    wnd: i32,
    base: String,
}

impl<'a> RealListScreen<'a> {
    pub fn new(session: &'a GuiSession, wnd: i32, base: &str) -> Self {
        Self {
            session,
            wnd,
            base: base.to_string(),
        }
    }

    fn table_id(&self, side: Side) -> String {
        match side {
            Side::Left => format!("wnd[{}]{}/tblSAPLSKBHTC_WRITE_LIST", self.wnd, self.base),
            Side::Right => format!("wnd[{}]/usr/tblSAPLSKBHTC_FIELD_LIST", self.wnd),
        }
    }

    /// Fresh table object (the old one is stale after a scroll / button press).
    fn table(&self, side: Side) -> Result<GuiTableControl> {
        let id = self.table_id(side);
        self.session
            .find_by_id(id.clone())
            .map_err(|e| anyhow!("table {id} not found: {e}"))?
            .downcast::<GuiTableControl>()
            .ok_or_else(|| anyhow!("{id} is not a GuiTableControl"))
    }

    /// `GuiScrollbar` has no `Type`: wrap the COM object of the table's own property.
    fn scrollbar(&self, side: Side) -> Result<GuiScrollbar> {
        let t = self.table(side)?;
        let c = t.vertical_scrollbar()?;
        Ok(GuiScrollbar::from(c.get_idispatch().clone()))
    }

    fn settle(&self) {
        thread::sleep(Duration::from_millis(120));
    }

    /// Is the editor on screen (displayed list present)?
    pub fn is_open(&self) -> bool {
        self.table(Side::Left).is_ok()
    }
}

impl ListScreen for RealListScreen<'_> {
    fn visible_rows(&mut self, side: Side) -> Result<i32> {
        Ok(self.table(side)?.visible_row_count()?.max(1))
    }

    fn row_text(&mut self, side: Side, visible_row: i32) -> Result<String> {
        let t = self.table(side)?;
        Ok(match t.get_cell(visible_row, 0) {
            Ok(c) => cell_text(&c),
            Err(_) => String::new(), // no cell = blank filler row
        })
    }

    fn scroll_position(&mut self, side: Side) -> Result<i32> {
        Ok(self.scrollbar(side)?.position()?)
    }

    fn set_scroll_position(&mut self, side: Side, position: i32) -> Result<()> {
        let sb = self.scrollbar(side)?;
        let max = sb.maximum().unwrap_or(i32::MAX);
        sb.set_position(position.clamp(0, max.max(0)))?;
        self.settle();
        Ok(())
    }

    fn select_row(&mut self, side: Side, visible_row: i32) -> Result<bool> {
        let pos = self.scroll_position(side)?;
        let t = self.table(side)?;
        let row = t.get_absolute_row(pos + visible_row)?;
        if !row.selectable().unwrap_or(true) {
            return Ok(false);
        }
        Ok(row.set_selected(true).is_ok())
    }

    fn deselect_row(&mut self, side: Side, visible_row: i32) -> Result<()> {
        let pos = self.scroll_position(side)?;
        let t = self.table(side)?;
        if let Ok(row) = t.get_absolute_row(pos + visible_row) {
            if row.selected().unwrap_or(false) {
                let _ = row.set_selected(false);
            }
        }
        Ok(())
    }

    fn move_selected(&mut self, to: Side) -> Result<()> {
        let id = match to {
            // "to right" = remove from the layout (APP_FL_SING)
            Side::Right => format!("wnd[{}]/usr/btnAPP_FL_SING", self.wnd),
            // "to left" = add to the layout (APP_WL_SING)
            Side::Left => format!("wnd[{}]/usr/btnAPP_WL_SING", self.wnd),
        };
        press_button(self.session, &id)?;
        thread::sleep(Duration::from_millis(250));
        // SAP may answer a refused move (locked column) with a message popup: confirm it
        if let Ok(w) = self.session.find_by_id(format!("wnd[{}]", self.wnd + 1)) {
            if let Some(m) = w.downcast::<GuiModalWindow>() {
                println!(
                    "Message after moving columns: {}",
                    m.text().unwrap_or_default()
                );
                let _ = m.send_v_key(0);
                thread::sleep(Duration::from_millis(150));
            }
        }
        Ok(())
    }

    fn sort_available(&mut self) -> Result<()> {
        press_button(
            self.session,
            &format!("wnd[{}]/usr/btn%#AUTOTEXT002", self.wnd),
        )?;
        thread::sleep(Duration::from_millis(250));
        Ok(())
    }
}

/// Grid-style editor over the GUI (`base` as in the list editor, e.g.
/// `/usr/tabsG_TS_ALV/tabpALV_M_R1/ssubSUB_DYN0510:SAPLSKBH:0620`).
pub struct RealGridScreen<'a> {
    session: &'a GuiSession,
    wnd: i32,
    base: String,
}

impl<'a> RealGridScreen<'a> {
    pub fn new(session: &'a GuiSession, wnd: i32, base: &str) -> Self {
        Self {
            session,
            wnd,
            base: base.to_string(),
        }
    }

    fn left_id(&self) -> String {
        format!(
            "wnd[{}]{}/cntlCONTAINER2_LAYO/shellcont/shell",
            self.wnd, self.base
        )
    }

    fn right_id(&self) -> String {
        format!(
            "wnd[{}]{}/cntlCONTAINER1_LAYO/shellcont/shell",
            self.wnd, self.base
        )
    }

    fn grid(&self, id: String) -> Result<GuiGridView> {
        self.session
            .find_by_id(id.clone())
            .map_err(|e| anyhow!("grid {id} not found: {e}"))?
            .downcast::<GuiGridView>()
            .ok_or_else(|| anyhow!("{id} is not a GuiGridView"))
    }

    /// Is the editor on screen (both grids present)?
    pub fn is_open(&self) -> bool {
        self.grid(self.left_id()).is_ok() && self.grid(self.right_id()).is_ok()
    }
}

impl GridScreen for RealGridScreen<'_> {
    fn clear_left(&mut self) -> Result<()> {
        let left = self.grid(self.left_id())?;
        left.set_current_cell_row(0)?;
        let rows = left.row_count()?;
        if rows > 0 {
            left.set_selected_rows(format!("0-{}", rows - 1))?;
            press_button(
                self.session,
                &format!("wnd[{}]{}/btnAPP_FL_SING", self.wnd, self.base),
            )?;
            thread::sleep(Duration::from_millis(250));
        }
        // fresh objects after the button press
        self.grid(self.left_id())?.set_current_cell_row(-1)?;
        self.grid(self.right_id())?.set_current_cell_row(1)?;
        Ok(())
    }

    fn sort_right(&mut self) -> Result<()> {
        let right = self.grid(self.right_id())?;
        right.select_column("SELTEXT".to_string())?;
        right.press_column_header("SELTEXT".to_string())?;
        Ok(())
    }

    fn right_row_count(&mut self) -> Result<i32> {
        Ok(self.grid(self.right_id())?.row_count()?)
    }

    fn right_text(&mut self, row: i32) -> Result<String> {
        Ok(self
            .grid(self.right_id())?
            .get_cell_value(row, "SELTEXT".to_string())?)
    }

    fn add_right_row(&mut self, row: i32) -> Result<()> {
        let right = self.grid(self.right_id())?;
        right.set_current_cell_row(row)?;
        right.double_click_current_cell()?;
        thread::sleep(Duration::from_millis(150));
        Ok(())
    }
}

// ---------------------------------------------------------------------------------------------
// tests with fakes
// ---------------------------------------------------------------------------------------------

#[cfg(test)]
pub mod fakes {
    use super::*;
    use std::collections::{BTreeSet, HashSet};

    /// In-memory list-style editor. Selections persist while scrolling (as in SAP); moving
    /// removes the selected rows from their table and appends them to the other one. Rows listed
    /// in `locked` cannot be moved away from the left list. `filler` empty rows trail each
    /// table (a real table control shows blank rows after the last entry).
    pub struct FakeList {
        pub left: Vec<String>,
        pub right: Vec<String>,
        pub vrc: i32,
        pub filler: usize,
        pub locked: HashSet<String>,
        /// Left rows that cannot even be selected.
        pub unselectable: HashSet<String>,
        /// A move-right whose selection holds a locked row moves nothing at all.
        pub reject_locked_moves: bool,
        pub pos: [i32; 2],
        sel: [BTreeSet<String>; 2],
        pub sorted: bool,
        pub moves_right: usize,
        pub moves_left: usize,
    }

    fn ix(side: Side) -> usize {
        match side {
            Side::Left => 0,
            Side::Right => 1,
        }
    }

    impl FakeList {
        pub fn new(left: &[&str], right: &[&str], vrc: i32) -> Self {
            Self {
                left: left.iter().map(|s| s.to_string()).collect(),
                right: right.iter().map(|s| s.to_string()).collect(),
                vrc,
                filler: 0,
                locked: HashSet::new(),
                unselectable: HashSet::new(),
                reject_locked_moves: false,
                pos: [0, 0],
                sel: [BTreeSet::new(), BTreeSet::new()],
                sorted: false,
                moves_right: 0,
                moves_left: 0,
            }
        }

        fn rows(&self, side: Side) -> &Vec<String> {
            match side {
                Side::Left => &self.left,
                Side::Right => &self.right,
            }
        }

        fn total(&self, side: Side) -> i32 {
            (self.rows(side).len() + self.filler) as i32
        }

        fn max_pos(&self, side: Side) -> i32 {
            (self.total(side) - self.vrc).max(0)
        }

        fn clamp(&mut self, side: Side) {
            let m = self.max_pos(side);
            let i = ix(side);
            self.pos[i] = self.pos[i].clamp(0, m);
        }
    }

    impl ListScreen for FakeList {
        fn visible_rows(&mut self, _side: Side) -> Result<i32> {
            Ok(self.vrc)
        }

        fn row_text(&mut self, side: Side, visible_row: i32) -> Result<String> {
            let abs = (self.pos[ix(side)] + visible_row) as usize;
            Ok(self.rows(side).get(abs).cloned().unwrap_or_default())
        }

        fn scroll_position(&mut self, side: Side) -> Result<i32> {
            Ok(self.pos[ix(side)])
        }

        fn set_scroll_position(&mut self, side: Side, position: i32) -> Result<()> {
            self.pos[ix(side)] = position;
            self.clamp(side);
            Ok(())
        }

        fn select_row(&mut self, side: Side, visible_row: i32) -> Result<bool> {
            let abs = (self.pos[ix(side)] + visible_row) as usize;
            let name = self
                .rows(side)
                .get(abs)
                .cloned()
                .ok_or_else(|| anyhow!("select_row: no row {abs}"))?;
            if side == Side::Left && self.unselectable.contains(&name) {
                return Ok(false);
            }
            self.sel[ix(side)].insert(name);
            Ok(true)
        }

        fn deselect_row(&mut self, side: Side, visible_row: i32) -> Result<()> {
            let abs = (self.pos[ix(side)] + visible_row) as usize;
            if let Some(name) = self.rows(side).get(abs).cloned() {
                self.sel[ix(side)].remove(&name);
            }
            Ok(())
        }

        fn move_selected(&mut self, to: Side) -> Result<()> {
            let from = if to == Side::Right {
                Side::Left
            } else {
                Side::Right
            };
            let locked = self.locked.clone();
            if to == Side::Right
                && self.reject_locked_moves
                && self.sel[ix(from)].iter().any(|r| locked.contains(r))
            {
                // SAP refuses the whole move and keeps the selection
                self.moves_right += 1;
                return Ok(());
            }
            let picked = std::mem::take(&mut self.sel[ix(from)]);
            let mut moved = Vec::new();
            let src = match from {
                Side::Left => &mut self.left,
                Side::Right => &mut self.right,
            };
            src.retain(|r| {
                let take = picked.contains(r) && !(from == Side::Left && locked.contains(r));
                if take {
                    moved.push(r.clone());
                }
                !take
            });
            let dst = match to {
                Side::Left => &mut self.left,
                Side::Right => &mut self.right,
            };
            dst.extend(moved);
            if to == Side::Right {
                self.moves_right += 1;
            } else {
                self.moves_left += 1;
            }
            self.clamp(Side::Left);
            self.clamp(Side::Right);
            Ok(())
        }

        fn sort_available(&mut self) -> Result<()> {
            self.right.sort_by_key(|s| s.to_uppercase());
            self.sorted = true;
            Ok(())
        }
    }

    /// In-memory grid-style editor.
    pub struct FakeGrid {
        pub left: Vec<String>,
        pub right: Vec<String>,
        pub sorted: bool,
    }

    impl FakeGrid {
        pub fn new(left: &[&str], right: &[&str]) -> Self {
            Self {
                left: left.iter().map(|s| s.to_string()).collect(),
                right: right.iter().map(|s| s.to_string()).collect(),
                sorted: false,
            }
        }
    }

    impl GridScreen for FakeGrid {
        fn clear_left(&mut self) -> Result<()> {
            let l = std::mem::take(&mut self.left);
            self.right.extend(l);
            Ok(())
        }
        fn sort_right(&mut self) -> Result<()> {
            self.right.sort_by_key(|s| s.to_uppercase());
            self.sorted = true;
            Ok(())
        }
        fn right_row_count(&mut self) -> Result<i32> {
            Ok(self.right.len() as i32)
        }
        fn right_text(&mut self, row: i32) -> Result<String> {
            self.right
                .get(row as usize)
                .cloned()
                .ok_or_else(|| anyhow!("no row {row}"))
        }
        fn add_right_row(&mut self, row: i32) -> Result<()> {
            let r = self.right.remove(row as usize);
            self.left.push(r);
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::fakes::*;
    use super::*;

    fn names(prefix: &str, n: usize) -> Vec<String> {
        (0..n).map(|i| format!("{prefix} {i:02}")).collect()
    }

    fn v(a: &[&str]) -> Vec<String> {
        a.iter().map(|s| s.to_string()).collect()
    }

    fn refs(v: &[String]) -> Vec<&str> {
        v.iter().map(String::as_str).collect()
    }

    #[test]
    fn blank_cells_and_matching() {
        assert!(is_blank_cell(""));
        assert!(is_blank_cell("  "));
        assert!(is_blank_cell("_______"));
        assert!(is_blank_cell(" ___ "));
        assert!(!is_blank_cell("Plant"));
        assert!(!is_blank_cell("_Plant"));
        assert!(same_column(" plant ", "PLANT"));
        assert!(!same_column("Plant", "Plants"));
    }

    #[test]
    fn list_editor_builds_the_layout_in_order_over_many_pages() {
        let left = names("Old", 30); // three pages of 12
        let right = names("Col", 45);
        let mut right: Vec<&str> = right.iter().map(String::as_str).collect();
        right.extend(["Delivery", "Plant", "Material"]);
        let left: Vec<&str> = left.iter().map(String::as_str).collect();
        let mut f = FakeList::new(&left, &right, 12);
        f.filler = 6;
        let wanted = v(&["Plant", "col 40", "Delivery", "Col 03", "Material"]);
        let r = pick_columns_list(&mut f, &wanted).unwrap();
        assert!(r.is_complete(), "{r:?}");
        assert_eq!(r.moved, wanted);
        // order of the layout == order requested
        assert_eq!(
            f.left,
            ["Plant", "Col 40", "Delivery", "Col 03", "Material"]
        );
        assert!(f.sorted);
        // the 30 old ones were moved to the right list
        assert!(f.right.iter().any(|c| c == "Old 29"));
        assert!(!f.right.iter().any(|c| c == "Plant"));
    }

    #[test]
    fn list_editor_clears_more_than_one_page_in_one_round() {
        let l = names("Old", 40);
        let lr: Vec<&str> = l.iter().map(String::as_str).collect();
        let mut f = FakeList::new(&lr, &["A"], 8);
        let rest = clear_left(&mut f).unwrap();
        assert!(rest.is_empty());
        assert!(f.left.is_empty());
        // selections persist across pages, so one press empties everything
        assert_eq!(f.moves_right, 1);
        assert_eq!(f.right.len(), 41);
    }

    #[test]
    fn list_editor_reports_missing_columns() {
        let mut f = FakeList::new(&["X"], &["Plant", "Material"], 12);
        let wanted = v(&["Plant", "No Such Column", "Material", "Other"]);
        let r = pick_columns_list(&mut f, &wanted).unwrap();
        assert!(!r.is_complete());
        assert_eq!(r.missing, ["No Such Column", "Other"]);
        assert_eq!(r.moved, ["Plant", "Material"]);
    }

    #[test]
    fn list_editor_keeps_unremovable_key_columns_as_present() {
        let mut f = FakeList::new(&["Shipment Number", "Delivery", "Extra"], &["Plant"], 12);
        f.locked.insert("Shipment Number".to_string());
        f.locked.insert("Delivery".to_string());
        let wanted = v(&["shipment number", "Plant", "Delivery"]);
        let r = pick_columns_list(&mut f, &wanted).unwrap();
        assert!(r.is_complete(), "{r:?}");
        assert_eq!(r.already_present, ["shipment number", "Delivery"]);
        assert_eq!(r.moved, ["Plant"]);
        assert!(r.kept_extra.is_empty());
        assert_eq!(f.left, ["Shipment Number", "Delivery", "Plant"]);
    }

    #[test]
    fn list_editor_reports_unrequested_columns_that_stay() {
        let mut f = FakeList::new(&["Key A", "Key B"], &["Plant"], 12);
        f.locked.insert("Key A".to_string());
        f.locked.insert("Key B".to_string());
        let r = pick_columns_list(&mut f, &v(&["Plant"])).unwrap();
        assert_eq!(r.kept_extra, ["Key A", "Key B"]);
    }

    #[test]
    fn list_editor_terminates_when_nothing_can_be_removed() {
        let mut f = FakeList::new(&["Key A", "Key B"], &[], 12);
        f.locked.insert("Key A".to_string());
        f.locked.insert("Key B".to_string());
        let rest = clear_left(&mut f).unwrap();
        assert_eq!(rest, ["Key A", "Key B"]);
        // one bulk round that moved nothing, then one attempt per row
        assert!(f.moves_right <= 3);
    }

    #[test]
    fn list_editor_finds_a_column_on_the_last_page_and_stops_at_the_end() {
        let right = names("Col", 40);
        let rr: Vec<&str> = right.iter().map(String::as_str).collect();
        let mut f = FakeList::new(&[], &rr, 12);
        // last row of the last (clamped, overlapping) page
        assert!(move_one(&mut f, "Col 39").unwrap());
        // absent: scan ends without looping forever, no filler needed
        assert!(!move_one(&mut f, "Nothing").unwrap());
        assert_eq!(f.pos[1], 0, "scrolled back to the top");
    }

    #[test]
    fn list_editor_handles_a_short_list_with_filler_rows() {
        let mut f = FakeList::new(&["Old"], &["Plant"], 12);
        f.filler = 20;
        let r = pick_columns_list(&mut f, &v(&["Plant", "Nope"])).unwrap();
        assert_eq!(r.moved, ["Plant"]);
        assert_eq!(r.missing, ["Nope"]);
    }

    #[test]
    fn grid_editor_picks_in_order_and_reports_missing() {
        let mut g = FakeGrid::new(&["Old 1", "Old 2"], &["Zeta", "Plant", "Material", "Alpha"]);
        let wanted = v(&["Material", "alpha", "Nope", "Plant"]);
        let r = pick_columns_grid(&mut g, &wanted, 200).unwrap();
        assert_eq!(r.moved, ["Material", "alpha", "Plant"]);
        assert_eq!(r.missing, ["Nope"]);
        assert!(g.sorted);
        assert_eq!(g.left, ["Material", "Alpha", "Plant"]);
        assert!(g.right.iter().any(|c| c == "Old 1"));
    }

    #[test]
    fn grid_editor_respects_the_row_limit() {
        let many = names("Col", 50);
        let rr: Vec<&str> = many.iter().map(String::as_str).collect();
        let mut g = FakeGrid::new(&[], &rr);
        let r = pick_columns_grid(&mut g, &v(&["Col 45"]), 10).unwrap();
        assert_eq!(r.missing, ["Col 45"]);
        let r = pick_columns_grid(&mut g, &v(&["Col 05"]), 10).unwrap();
        assert_eq!(r.moved, ["Col 05"]);
        assert_eq!(refs(&g.left), ["Col 05"]);
    }

    fn strs(v: &[&str]) -> Vec<String> {
        v.iter().map(|s| s.to_string()).collect()
    }

    /// VL06O item view: `Delivery` is locked and SAP refuses a move that includes it.
    #[test]
    fn locked_row_that_blocks_the_bulk_move_is_cleared_around() {
        let mut f = FakeList::new(
            &["Delivery", "Old A", "Old B", "Old C", "Old D"],
            &["Material", "Plant"],
            3,
        );
        f.locked.insert("Delivery".to_string());
        f.reject_locked_moves = true;
        let r = pick_columns_list(&mut f, &strs(&["Delivery", "Plant", "Material"])).unwrap();
        assert_eq!(f.left, strs(&["Delivery", "Plant", "Material"]));
        assert_eq!(r.already_present, strs(&["Delivery"]));
        assert_eq!(r.moved, strs(&["Plant", "Material"]));
        assert!(r.missing.is_empty() && r.kept_extra.is_empty());
    }

    #[test]
    fn unselectable_row_is_left_out_of_the_selection() {
        let mut f = FakeList::new(&["Delivery", "Old A", "Old B"], &["Plant"], 2);
        f.locked.insert("Delivery".to_string());
        f.unselectable.insert("Delivery".to_string());
        f.reject_locked_moves = true;
        let r = pick_columns_list(&mut f, &strs(&["Plant"])).unwrap();
        assert_eq!(f.left, strs(&["Delivery", "Plant"]));
        assert_eq!(r.kept_extra, strs(&["Delivery"]));
        assert_eq!(f.moves_right, 1, "one bulk move, no fallback needed");
    }

    #[test]
    fn locked_row_in_the_middle_with_refused_bulk_move() {
        let mut f = FakeList::new(&["Old A", "Key", "Old B", "Old C"], &[], 2);
        f.locked.insert("Key".to_string());
        f.reject_locked_moves = true;
        assert_eq!(clear_left(&mut f).unwrap(), strs(&["Key"]));
        assert_eq!(f.left, strs(&["Key"]));
    }
}
