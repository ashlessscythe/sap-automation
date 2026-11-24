//! The selectable menu inside the shell body: pure selection / filter state ([`MenuView`]),
//! its key handling and the ratatui widget ([`draw_menu`]).
//!
//! Keys: Up/Down, `k`/`j`, PageUp/PageDown, Home/End, digits (jump to item number, two digits
//! combine), `/` starts a filter (type to narrow, Backspace, Esc clears), Enter / Space choose,
//! Esc / `q` cancel (`None`). Filtering keeps the original item numbers.
#![allow(dead_code)]

use super::state::truncate_ellipsis;
use crossterm::event::{Event, KeyCode, KeyEventKind, KeyModifiers};
use ratatui::{
    layout::{Margin, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, Scrollbar, ScrollbarOrientation, ScrollbarState},
    Frame,
};

/// What the worker asked the UI to show.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum UiRequest {
    Menu(MenuSpec),
    /// An action ended; the log stays visible until a key is pressed.
    Acknowledge {
        ok: bool,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MenuSpec {
    pub title: String,
    pub items: Vec<String>,
    pub default: Option<usize>,
    /// Distinguishes consecutive menus so the view resets its selection.
    pub id: u64,
}

/// Result of one key press.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MenuEvent {
    None,
    Redraw,
    /// Index into the ORIGINAL item list.
    Choose(usize),
    Cancel,
}

/// UI-owned interaction state of the menu.
#[derive(Clone, Debug, Default)]
pub struct MenuView {
    spec_id: Option<u64>,
    /// Position in the VISIBLE (filtered) list.
    pub selected: usize,
    pub filter: String,
    pub filtering: bool,
    num_buf: String,
    /// Rows the list showed last time (page size for PageUp/PageDown).
    pub page: usize,
    first_visible: usize,
}

/// Indices of the items that match `filter` (case-insensitive, every word must occur).
pub fn visible_indices(items: &[String], filter: &str) -> Vec<usize> {
    let words: Vec<String> = filter.split_whitespace().map(str::to_lowercase).collect();
    items
        .iter()
        .enumerate()
        .filter(|(_, it)| {
            let l = it.to_lowercase();
            words.iter().all(|w| l.contains(w))
        })
        .map(|(i, _)| i)
        .collect()
}

impl MenuView {
    /// Reset for a new menu (no-op for the same one).
    pub fn sync(&mut self, spec: &MenuSpec) {
        if self.spec_id == Some(spec.id) {
            return;
        }
        *self = MenuView {
            spec_id: Some(spec.id),
            selected: spec.default.filter(|d| *d < spec.items.len()).unwrap_or(0),
            page: self.page.max(1),
            ..MenuView::default()
        };
    }

    pub fn visible(&self, spec: &MenuSpec) -> Vec<usize> {
        visible_indices(&spec.items, &self.filter)
    }

    fn clamp(&mut self, n: usize) {
        self.selected = self.selected.min(n.saturating_sub(1));
    }

    /// The original index of the highlighted item.
    pub fn current(&self, spec: &MenuSpec) -> Option<usize> {
        self.visible(spec).get(self.selected).copied()
    }

    pub fn handle_event(&mut self, ev: &Event, spec: &MenuSpec) -> MenuEvent {
        match ev {
            Event::Resize(_, _) => MenuEvent::Redraw,
            Event::Key(k) if k.kind != KeyEventKind::Release => {
                self.handle_key(k.code, k.modifiers, spec)
            }
            _ => MenuEvent::None,
        }
    }

    fn handle_key(&mut self, code: KeyCode, mods: KeyModifiers, spec: &MenuSpec) -> MenuEvent {
        let n = self.visible(spec).len();
        let ctrl = mods.contains(KeyModifiers::CONTROL);
        if self.filtering {
            match code {
                KeyCode::Esc => {
                    self.filtering = false;
                    self.filter.clear();
                    self.clamp(spec.items.len());
                }
                KeyCode::Enter => match self.current(spec) {
                    Some(i) => return MenuEvent::Choose(i),
                    None => return MenuEvent::None,
                },
                KeyCode::Backspace => {
                    self.filter.pop();
                    self.selected = 0;
                }
                KeyCode::Up => self.selected = self.selected.saturating_sub(1),
                KeyCode::Down => {
                    self.selected = (self.selected + 1).min(n.saturating_sub(1));
                }
                KeyCode::Char(c) if !ctrl => {
                    self.filter.push(c);
                    self.selected = 0;
                }
                _ => return MenuEvent::None,
            }
            return MenuEvent::Redraw;
        }
        let was_digit = matches!(code, KeyCode::Char(c) if c.is_ascii_digit());
        if !was_digit {
            self.num_buf.clear();
        }
        match code {
            KeyCode::Esc | KeyCode::Char('q') => {
                if !self.filter.is_empty() && code == KeyCode::Esc {
                    self.filter.clear();
                    self.selected = 0;
                    return MenuEvent::Redraw;
                }
                return MenuEvent::Cancel;
            }
            KeyCode::Char('c') if ctrl => return MenuEvent::Cancel,
            KeyCode::Enter | KeyCode::Char(' ') => {
                return match self.current(spec) {
                    Some(i) => MenuEvent::Choose(i),
                    None => MenuEvent::None,
                }
            }
            KeyCode::Up | KeyCode::Char('k') => {
                self.selected = if self.selected == 0 {
                    n.saturating_sub(1)
                } else {
                    self.selected - 1
                };
            }
            KeyCode::Down | KeyCode::Char('j') => {
                self.selected = if self.selected + 1 >= n {
                    0
                } else {
                    self.selected + 1
                };
            }
            KeyCode::PageUp => self.selected = self.selected.saturating_sub(self.page.max(1)),
            KeyCode::PageDown => {
                self.selected = (self.selected + self.page.max(1)).min(n.saturating_sub(1));
            }
            KeyCode::Home => self.selected = 0,
            KeyCode::End => self.selected = n.saturating_sub(1),
            KeyCode::Char('/') => {
                self.filtering = true;
                self.filter.clear();
            }
            KeyCode::Char(c) if c.is_ascii_digit() => {
                let vis = self.visible(spec);
                // number shown next to an item = original index + 1
                let try_buf = format!("{}{}", self.num_buf, c);
                let pick = |s: &str| -> Option<usize> {
                    let v: usize = s.parse().ok()?;
                    vis.iter().position(|&i| i + 1 == v)
                };
                if let Some(p) = pick(&try_buf) {
                    self.num_buf = try_buf;
                    self.selected = p;
                } else if let Some(p) = pick(&c.to_string()) {
                    self.num_buf = c.to_string();
                    self.selected = p;
                } else {
                    return MenuEvent::None;
                }
            }
            _ => return MenuEvent::None,
        }
        self.clamp(n);
        MenuEvent::Redraw
    }
}

/// Number column width for `count` items.
fn num_width(count: usize) -> usize {
    count.max(1).to_string().len()
}

/// Draw the menu into `area`. Never panics for any size; updates `view.page`.
pub fn draw_menu(frame: &mut Frame, area: Rect, spec: &MenuSpec, view: &mut MenuView) {
    if area.width == 0 || area.height == 0 {
        view.page = 1;
        return;
    }
    view.sync(spec);
    let bordered = area.height >= 5 && area.width >= 12;
    let inner = if bordered {
        let b = Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::DarkGray))
            .title(format!(
                " {} ",
                truncate_ellipsis(&spec.title, area.width as usize - 4)
            ));
        let i = b.inner(area);
        frame.render_widget(b, area);
        i
    } else {
        area
    };
    if inner.width == 0 || inner.height == 0 {
        view.page = 1;
        return;
    }
    // the last row shows the filter line while filtering (or when a filter is set)
    let show_filter = (view.filtering || !view.filter.is_empty()) && inner.height >= 2;
    let list_h = inner.height as usize - usize::from(show_filter);
    view.page = list_h.max(1);
    let vis = view.visible(spec);
    view.clamp(vis.len());
    // keep the selection inside the window
    if view.selected < view.first_visible {
        view.first_visible = view.selected;
    }
    if view.selected >= view.first_visible + list_h {
        view.first_visible = view.selected + 1 - list_h.max(1);
    }
    view.first_visible = view.first_visible.min(vis.len().saturating_sub(list_h));
    let nw = num_width(spec.items.len());
    let text_w = inner.width.saturating_sub(1) as usize; // scrollbar column
    let mut lines: Vec<Line> = Vec::new();
    for (pos, &orig) in vis.iter().enumerate().skip(view.first_visible).take(list_h) {
        let selected = pos == view.selected;
        let label = format!(
            "{} {:>nw$}. {}",
            if selected { ">" } else { " " },
            orig + 1,
            spec.items[orig]
        );
        let label = truncate_ellipsis(&label, text_w);
        let style = if selected {
            Style::default()
                .fg(Color::Black)
                .bg(Color::Cyan)
                .add_modifier(Modifier::BOLD)
        } else if spec.items[orig].contains("(Not available") {
            Style::default().fg(Color::DarkGray)
        } else {
            Style::default()
        };
        lines.push(Line::from(Span::styled(label, style)));
    }
    if vis.is_empty() {
        lines.push(Line::from(Span::styled(
            "no match",
            Style::default().fg(Color::DarkGray),
        )));
    }
    let list_area = Rect {
        height: list_h as u16,
        width: inner.width.saturating_sub(1).max(1).min(inner.width),
        ..inner
    };
    frame.render_widget(Paragraph::new(lines), list_area);
    if vis.len() > list_h && inner.width >= 2 {
        let mut sb =
            ScrollbarState::new(vis.len().saturating_sub(list_h) + 1).position(view.first_visible);
        let sb_area = Rect {
            height: list_h as u16,
            ..if bordered {
                area.inner(Margin {
                    vertical: 1,
                    horizontal: 0,
                })
            } else {
                area
            }
        };
        frame.render_stateful_widget(
            Scrollbar::new(ScrollbarOrientation::VerticalRight),
            sb_area,
            &mut sb,
        );
    }
    if show_filter {
        let r = Rect {
            y: inner.y + inner.height - 1,
            height: 1,
            ..inner
        };
        let cursor = if view.filtering { "_" } else { "" };
        frame.render_widget(
            Paragraph::new(Line::from(vec![
                Span::styled("/ ", Style::default().fg(Color::Yellow)),
                Span::raw(truncate_ellipsis(
                    &format!("{}{}", view.filter, cursor),
                    (inner.width as usize).saturating_sub(2),
                )),
            ])),
            r,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::{KeyEvent, KeyEventState};
    use ratatui::{backend::TestBackend, Terminal};

    fn spec(n: usize) -> MenuSpec {
        MenuSpec {
            title: "Main Menu".into(),
            items: (0..n).map(|i| format!("Item {i} thing")).collect(),
            default: None,
            id: 1,
        }
    }

    fn key(c: KeyCode) -> Event {
        Event::Key(KeyEvent {
            code: c,
            modifiers: KeyModifiers::NONE,
            kind: KeyEventKind::Press,
            state: KeyEventState::NONE,
        })
    }

    fn render(w: u16, h: u16, sp: &MenuSpec, v: &mut MenuView) -> String {
        let mut t = Terminal::new(TestBackend::new(w, h)).unwrap();
        t.draw(|f| draw_menu(f, f.area(), sp, v)).unwrap();
        let b = t.backend().buffer().clone();
        let mut s = String::new();
        for y in 0..b.area.height {
            for x in 0..b.area.width {
                s.push_str(b[(x, y)].symbol());
            }
            s.push('\n');
        }
        s
    }

    #[test]
    fn selection_moves_wraps_and_chooses() {
        let sp = spec(5);
        let mut v = MenuView::default();
        v.sync(&sp);
        assert_eq!(v.handle_event(&key(KeyCode::Down), &sp), MenuEvent::Redraw);
        v.handle_event(&key(KeyCode::Char('j')), &sp);
        assert_eq!(v.selected, 2);
        v.handle_event(&key(KeyCode::Char('k')), &sp);
        assert_eq!(v.selected, 1);
        v.handle_event(&key(KeyCode::End), &sp);
        assert_eq!(v.selected, 4);
        v.handle_event(&key(KeyCode::Down), &sp);
        assert_eq!(v.selected, 0, "wraps");
        v.handle_event(&key(KeyCode::Up), &sp);
        assert_eq!(v.selected, 4);
        assert_eq!(
            v.handle_event(&key(KeyCode::Enter), &sp),
            MenuEvent::Choose(4)
        );
        assert_eq!(
            v.handle_event(&key(KeyCode::Char('q')), &sp),
            MenuEvent::Cancel
        );
        assert_eq!(v.handle_event(&key(KeyCode::Esc), &sp), MenuEvent::Cancel);
    }

    #[test]
    fn default_selection_and_new_menu_resets() {
        let mut sp = spec(5);
        sp.default = Some(3);
        let mut v = MenuView::default();
        v.sync(&sp);
        assert_eq!(v.selected, 3);
        v.handle_event(&key(KeyCode::Home), &sp);
        v.sync(&sp);
        assert_eq!(v.selected, 0, "same id keeps state");
        sp.id = 2;
        v.sync(&sp);
        assert_eq!(v.selected, 3, "new id resets to the default");
        sp.default = Some(99);
        sp.id = 3;
        v.sync(&sp);
        assert_eq!(v.selected, 0, "out of range default");
    }

    #[test]
    fn number_shortcuts_jump() {
        let sp = spec(30);
        let mut v = MenuView::default();
        v.sync(&sp);
        v.handle_event(&key(KeyCode::Char('7')), &sp);
        assert_eq!(v.selected, 6);
        v.handle_event(&key(KeyCode::Char('2')), &sp); // 72 does not exist: restart with 2
        assert_eq!(v.selected, 1);
        v.handle_event(&key(KeyCode::Char('1')), &sp); // 21
        assert_eq!(v.selected, 20);
        v.handle_event(&key(KeyCode::Char('k')), &sp);
        v.handle_event(&key(KeyCode::Char('3')), &sp);
        assert_eq!(v.selected, 2, "buffer reset by another key");
        assert_eq!(
            v.handle_event(&key(KeyCode::Char('0')), &sp),
            MenuEvent::Redraw,
            "30 exists"
        );
        assert_eq!(v.selected, 29);
    }

    #[test]
    fn filter_narrows_keeps_numbers_and_escapes() {
        let mut sp = spec(0);
        sp.items = ["VT11 - Plan", "ZVT11 - Report", "VL06O - List", "Exit"]
            .map(String::from)
            .to_vec();
        let mut v = MenuView::default();
        v.sync(&sp);
        v.handle_event(&key(KeyCode::Char('/')), &sp);
        assert!(v.filtering);
        for c in "vt11".chars() {
            v.handle_event(&key(KeyCode::Char(c)), &sp);
        }
        assert_eq!(v.visible(&sp), vec![0, 1]);
        v.handle_event(&key(KeyCode::Down), &sp);
        assert_eq!(
            v.handle_event(&key(KeyCode::Enter), &sp),
            MenuEvent::Choose(1),
            "original index"
        );
        let out = render(60, 12, &sp, &mut v);
        assert!(out.contains("2. ZVT11"), "original number kept:\n{out}");
        assert!(!out.contains("Exit"));
        assert!(out.contains("/ vt11_"));
        // q while typing is text, not cancel
        v.handle_event(&key(KeyCode::Char('q')), &sp);
        assert_eq!(v.filter, "vt11q");
        assert!(v.visible(&sp).is_empty());
        let out = render(60, 12, &sp, &mut v);
        assert!(out.contains("no match"));
        assert_eq!(v.handle_event(&key(KeyCode::Enter), &sp), MenuEvent::None);
        v.handle_event(&key(KeyCode::Esc), &sp);
        assert!(!v.filtering && v.filter.is_empty());
        assert_eq!(v.visible(&sp).len(), 4);
    }

    #[test]
    fn esc_clears_a_committed_filter_before_cancelling() {
        let sp = spec(4);
        let mut v = MenuView {
            filter: "item".into(),
            ..Default::default()
        };
        v.sync(&sp);
        v.filter = "item".into();
        assert_eq!(v.handle_event(&key(KeyCode::Esc), &sp), MenuEvent::Redraw);
        assert_eq!(v.handle_event(&key(KeyCode::Esc), &sp), MenuEvent::Cancel);
    }

    #[test]
    fn long_menu_scrolls_to_selection_and_renders_scrollbar() {
        let sp = spec(40);
        let mut v = MenuView::default();
        let out = render(50, 10, &sp, &mut v);
        assert!(out.contains("1. Item 0"));
        assert!(!out.contains("Item 39"));
        v.handle_event(&key(KeyCode::End), &sp);
        let out = render(50, 10, &sp, &mut v);
        assert!(out.contains("40. Item 39"), "{out}");
        assert!(!out.contains(" 1. Item 0"));
        assert!(v.page >= 1);
        v.handle_event(&key(KeyCode::PageUp), &sp);
        assert!(v.selected < 39);
    }

    #[test]
    fn tiny_and_resized_menus_do_not_panic() {
        let sp = spec(40);
        let mut v = MenuView::default();
        for (w, h) in [
            (0u16, 0u16),
            (1, 1),
            (3, 2),
            (12, 5),
            (20, 3),
            (200, 80),
            (5, 40),
        ] {
            render(w, h, &sp, &mut v);
            v.handle_event(&key(KeyCode::End), &sp);
            render(w, h, &sp, &mut v);
            v.handle_event(&key(KeyCode::Char('/')), &sp);
            render(w, h, &sp, &mut v);
            v.handle_event(&key(KeyCode::Esc), &sp);
        }
        let empty = MenuSpec {
            items: vec![],
            ..spec(0)
        };
        let mut v = MenuView::default();
        render(30, 8, &empty, &mut v);
        assert_eq!(
            v.handle_event(&key(KeyCode::Enter), &empty),
            MenuEvent::None
        );
        assert_eq!(
            v.handle_event(&Event::Resize(5, 5), &empty),
            MenuEvent::Redraw
        );
    }

    #[test]
    fn release_events_are_ignored() {
        let sp = spec(3);
        let mut v = MenuView::default();
        v.sync(&sp);
        let rel = Event::Key(KeyEvent {
            code: KeyCode::Enter,
            modifiers: KeyModifiers::NONE,
            kind: KeyEventKind::Release,
            state: KeyEventState::NONE,
        });
        assert_eq!(v.handle_event(&rel, &sp), MenuEvent::None);
    }
}
