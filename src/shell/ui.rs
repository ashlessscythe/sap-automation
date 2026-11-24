//! Pure rendering of the shell: `draw(frame, &ShellState, &mut ViewState)`.
#![allow(dead_code)]

use super::menu::{draw_menu, UiRequest};
use super::state::{
    format_elapsed, truncate_ellipsis, Level, LogEntry, SapStatus, SessionRow, SessionSnapshot,
    ShellState, ViewState,
};
use ratatui::{
    layout::{Constraint, Layout, Margin, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{
        Block, Borders, Cell, Gauge, Paragraph, Row, Scrollbar, ScrollbarOrientation,
        ScrollbarState, Table, Wrap,
    },
    Frame,
};

pub fn level_color(l: Level) -> Color {
    match l {
        Level::Debug => Color::DarkGray,
        Level::Info => Color::Gray,
        Level::Warn => Color::Yellow,
        Level::Error => Color::Red,
    }
}

fn entry_line(e: &LogEntry, show_ts: bool, show_tag: bool) -> Line<'static> {
    let mut spans: Vec<Span<'static>> = Vec::new();
    if show_ts {
        spans.push(Span::styled(
            format!("{} ", e.timestamp),
            Style::default().fg(Color::DarkGray),
        ));
    }
    let color = level_color(e.level);
    if show_tag {
        if let Some(t) = &e.tag {
            spans.push(Span::styled(
                format!("[{t}] "),
                Style::default().fg(Color::Cyan),
            ));
        }
    }
    if e.level >= Level::Warn {
        spans.push(Span::styled(
            format!("{} ", e.level.label()),
            Style::default().fg(color).add_modifier(Modifier::BOLD),
        ));
    }
    spans.push(Span::styled(e.text.clone(), Style::default().fg(color)));
    Line::from(spans)
}

/// Rows a line needs when wrapped at `width` (character-wrap estimate, at least one row).
fn wrapped_rows(line: &Line, width: usize) -> usize {
    if width == 0 {
        return 1;
    }
    line.width().div_ceil(width).max(1)
}

fn header(state: &ShellState) -> Line<'static> {
    let mut spans = vec![Span::styled(
        format!(" {} ", state.title),
        Style::default()
            .fg(Color::Black)
            .bg(Color::Cyan)
            .add_modifier(Modifier::BOLD),
    )];
    if let Some(t) = &state.tcode {
        spans.push(Span::raw(" "));
        spans.push(Span::styled(
            format!(" {t} "),
            Style::default()
                .fg(Color::Black)
                .bg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        ));
    }
    Line::from(spans)
}

fn status_line(state: &ShellState, view: &ViewState, width: usize) -> Line<'static> {
    let (dot_color, sap_text) = match &state.sap {
        SapStatus::Connected(Some(i)) => (Color::Green, format!("SAP {i}")),
        SapStatus::Connected(None) => (Color::Green, "SAP connected".to_string()),
        SapStatus::Disconnected => (Color::Red, "SAP disconnected".to_string()),
        SapStatus::Unknown => (Color::DarkGray, "SAP ?".to_string()),
    };
    let mut spans = vec![
        Span::styled(" ● ", Style::default().fg(dot_color)),
        Span::raw(sap_text),
    ];
    // Optional items in descending importance; dropped when they do not fit.
    let mut optional: Vec<String> = vec![
        state.mode.label().to_string(),
        format_elapsed(state.elapsed()),
    ];
    if let Some((d, t)) = state.sessions.as_ref().and_then(|s| s.progress) {
        optional.push(format!("days {d}/{t}"));
    }
    optional.extend(state.extras.iter().map(|(l, v)| format!("{l}: {v}")));
    optional.push(format!(
        "{} lines, {} warn, {} err",
        state.log_len(),
        state.count(Level::Warn),
        state.count(Level::Error)
    ));
    optional.push(if view.follow { "follow" } else { "paused" }.to_string());
    let keys = match &state.request {
        Some(UiRequest::Menu(_)) => "Up/Down Enter  / filter  1-9 jump  q back".to_string(),
        Some(UiRequest::Acknowledge { .. }) => {
            "any key / q: back to menu  PgUp/PgDn scroll".to_string()
        }
        None if state.mode == super::state::Mode::Interactive => {
            "running  PgUp/PgDn  Home/End  f follow".to_string()
        }
        None => "q quit  PgUp/PgDn  Home/End  f follow".to_string(),
    };
    if state.request.is_some() {
        // what the user must do now outranks counters
        optional.insert(2.min(optional.len()), keys);
    } else {
        optional.push(keys);
    }
    let mut used: usize = spans.iter().map(|s| s.width()).sum();
    for item in optional {
        let add = 3 + item.chars().count();
        if used + add > width {
            continue;
        }
        used += add;
        spans.push(Span::styled(" | ", Style::default().fg(Color::DarkGray)));
        spans.push(Span::raw(item));
    }
    Line::from(spans).style(Style::default().bg(Color::Rgb(30, 30, 40)))
}

/// Render the whole shell. Updates `view.page` / `view.total_lines` for scrolling.
pub fn draw(frame: &mut Frame, state: &ShellState, view: &mut ViewState) {
    let area = frame.area();
    if area.width == 0 || area.height == 0 {
        return;
    }
    let show_cmd = area.height >= 12 && !state.command_line.is_empty();
    let footer_h = u16::from(area.height >= 2);
    let fixed = 1 + u16::from(show_cmd) + footer_h;
    let n_sessions = state.sessions.as_ref().map_or(0, |s| s.rows.len());
    let (pane_h, pane) = session_pane_layout(area.height, fixed, n_sessions);
    let rows = Layout::vertical([
        Constraint::Length(1),
        Constraint::Length(u16::from(show_cmd)),
        Constraint::Length(pane_h),
        Constraint::Min(0),
        Constraint::Length(footer_h),
    ])
    .split(area);

    frame.render_widget(Paragraph::new(header(state)), rows[0]);
    if show_cmd {
        let avail = (area.width as usize).saturating_sub(2);
        let line = Line::from(vec![
            Span::styled("$ ", Style::default().fg(Color::Green)),
            Span::styled(
                truncate_ellipsis(&state.command_line, avail),
                Style::default().fg(Color::White),
            ),
        ]);
        frame.render_widget(Paragraph::new(line), rows[1]);
    }
    if let (Some(snap), true) = (&state.sessions, pane_h > 0) {
        draw_sessions(frame, rows[2], snap, pane);
    }
    match &state.request {
        Some(UiRequest::Menu(spec)) => {
            let body = rows[3];
            // with enough room the log tail stays visible under the menu
            if body.height >= 16 {
                let log_h = (body.height / 3).max(5);
                let menu_area = Rect {
                    height: body.height - log_h,
                    ..body
                };
                let log_area = Rect {
                    y: body.y + menu_area.height,
                    height: log_h,
                    ..body
                };
                draw_menu(frame, menu_area, spec, &mut view.menu);
                draw_log(frame, log_area, state, view);
            } else {
                draw_menu(frame, body, spec, &mut view.menu);
            }
        }
        _ => draw_log(frame, rows[3], state, view),
    }
    if rows[4].height > 0 {
        frame.render_widget(
            Paragraph::new(status_line(state, view, area.width as usize)),
            rows[4],
        );
    }
    if view.help {
        draw_help(frame, area);
    }
}

/// Lines of the key help overlay.
pub const HELP_LINES: &[&str] = &[
    "Up / Down, PgUp / PgDn, Home / End, mouse wheel   scroll the log",
    "f                                                 follow the end of the log",
    "q                                                 quit (when the run is done)",
    "Ctrl-C                                            abort the process",
    "? / F1                                            this help",
    "",
    "Menus:  Up/Down, Enter, 1-9 jump, / filter, q or Esc back",
    "After an action: any key returns to the menu",
    "",
    "Select text: hold Shift while dragging (mouse capture is on)",
    "Plain output instead of the shell: SAP_NO_SHELL=1",
    "",
    "Press any key to close",
];

fn draw_help(frame: &mut Frame, area: Rect) {
    use ratatui::widgets::Clear;
    let w = (HELP_LINES
        .iter()
        .map(|l| l.chars().count())
        .max()
        .unwrap_or(0) as u16
        + 4)
    .min(area.width);
    let h = (HELP_LINES.len() as u16 + 2).min(area.height);
    if w < 8 || h < 3 {
        return;
    }
    let rect = Rect {
        x: area.x + (area.width - w) / 2,
        y: area.y + (area.height - h) / 2,
        width: w,
        height: h,
    };
    frame.render_widget(Clear, rect);
    let lines: Vec<Line> = HELP_LINES.iter().map(|l| Line::from(*l)).collect();
    frame.render_widget(
        Paragraph::new(lines).block(Block::default().borders(Borders::ALL).title(" Keys ")),
        rect,
    );
}

/// The log window is never squeezed below this many rows by the session pane.
pub const MIN_LOG_ROWS: u16 = 6;

/// How the parallel session pane is shown.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PaneLayout {
    Hidden,
    /// One line: counts, elapsed and the progress gauge.
    Summary,
    /// Summary line, column titles and `rows` session rows.
    Table {
        rows: u16,
    },
}

/// Pure sizing rule: `(height, layout)` for the session pane given the terminal height, the rows
/// used by everything else (`fixed`: header, command line, footer) and the number of sessions.
/// The log keeps at least [`MIN_LOG_ROWS`]; with little room the pane collapses to one summary
/// line, with none it is hidden.
pub fn session_pane_layout(total_h: u16, fixed: u16, sessions: usize) -> (u16, PaneLayout) {
    let avail = total_h.saturating_sub(fixed).saturating_sub(MIN_LOG_ROWS);
    if sessions == 0 || avail == 0 {
        return (0, PaneLayout::Hidden);
    }
    let full = u16::try_from(sessions)
        .unwrap_or(u16::MAX)
        .saturating_add(2);
    if avail >= full {
        (full, PaneLayout::Table { rows: full - 2 })
    } else if avail >= 3 {
        (avail, PaneLayout::Table { rows: avail - 2 })
    } else {
        (1, PaneLayout::Summary)
    }
}

fn state_color(row: &SessionRow) -> Color {
    match row.state.as_str() {
        "FAILED" => Color::Red,
        "done" => Color::DarkGray,
        "" => Color::Gray,
        _ => Color::Green,
    }
}

/// `sessions 1/3 active | 00:12` summary text of the pane.
pub fn session_summary(snap: &SessionSnapshot) -> String {
    format!(
        " sessions {}/{} active | {}",
        snap.running(),
        snap.rows.len(),
        format_elapsed(snap.elapsed)
    )
}

fn draw_sessions(frame: &mut Frame, area: Rect, snap: &SessionSnapshot, pane: PaneLayout) {
    if area.width == 0 || area.height == 0 || pane == PaneLayout::Hidden {
        return;
    }
    let bar = Rect { height: 1, ..area };
    let (text_area, gauge_area) = match snap.progress {
        Some(_) if bar.width >= 60 => {
            let cols = Layout::horizontal([Constraint::Min(0), Constraint::Length(24)]).split(bar);
            (cols[0], Some(cols[1]))
        }
        _ => (bar, None),
    };
    let mut summary = session_summary(snap);
    if gauge_area.is_none() {
        if let Some((d, t)) = snap.progress {
            summary.push_str(&format!(" | days {d}/{t}"));
        }
    }
    frame.render_widget(
        Paragraph::new(truncate_ellipsis(&summary, text_area.width as usize))
            .style(Style::default().add_modifier(Modifier::REVERSED)),
        text_area,
    );
    if let (Some(g), Some((d, t))) = (gauge_area, snap.progress) {
        let ratio = if t == 0 {
            0.0
        } else {
            (d as f64 / t as f64).clamp(0.0, 1.0)
        };
        frame.render_widget(
            Gauge::default()
                .ratio(ratio)
                .label(format!("days {d}/{t}"))
                .gauge_style(Style::default().fg(Color::Green).bg(Color::DarkGray)),
            g,
        );
    }
    let PaneLayout::Table { rows } = pane else {
        return;
    };
    if area.height < 2 || rows == 0 {
        return;
    }
    let table_area = Rect {
        y: area.y + 1,
        height: area.height - 1,
        ..area
    };
    // narrow terminals drop columns: task first, then the latest line
    let w = area.width;
    let (widths, with_task, with_last): (Vec<Constraint>, bool, bool) = if w >= 90 {
        (
            vec![
                Constraint::Length(4),
                Constraint::Length(8),
                Constraint::Length(26),
                Constraint::Length(5),
                Constraint::Min(10),
            ],
            true,
            true,
        )
    } else if w >= 50 {
        (
            vec![
                Constraint::Length(4),
                Constraint::Length(8),
                Constraint::Length(5),
                Constraint::Min(10),
            ],
            false,
            true,
        )
    } else {
        (
            vec![
                Constraint::Length(4),
                Constraint::Length(8),
                Constraint::Min(5),
            ],
            false,
            false,
        )
    };
    let make_row = |c: [String; 5], st: Style| {
        let [tag, state, task, time, last] = c;
        let mut cells = vec![Cell::from(tag), Cell::from(state)];
        if with_task {
            cells.push(Cell::from(task));
        }
        cells.push(Cell::from(time));
        if with_last {
            cells.push(Cell::from(last));
        }
        Row::new(cells).style(st)
    };
    let header = make_row(
        ["ses", "state", "task", "time", "latest"].map(String::from),
        Style::default()
            .fg(Color::Cyan)
            .add_modifier(Modifier::BOLD),
    );
    let body: Vec<Row> = snap
        .rows
        .iter()
        .take(rows as usize)
        .map(|r| {
            make_row(
                [
                    r.tag.clone(),
                    r.state.clone(),
                    r.task.clone(),
                    r.task_elapsed.map(format_elapsed).unwrap_or_default(),
                    r.last.clone(),
                ],
                Style::default().fg(state_color(r)),
            )
        })
        .collect();
    frame.render_widget(Table::new(body, widths).header(header), table_area);
}

fn draw_log(frame: &mut Frame, area: Rect, state: &ShellState, view: &mut ViewState) {
    if area.width == 0 || area.height == 0 {
        view.page = 1;
        view.total_lines = 0;
        return;
    }
    let bordered = area.height >= 5 && area.width >= 12;
    let inner = if bordered {
        let b = Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::DarkGray))
            .title(" Log ");
        let inner = b.inner(area);
        frame.render_widget(b, area);
        inner
    } else {
        area
    };
    if inner.width == 0 || inner.height == 0 {
        return;
    }
    let show_ts = inner.width >= 60;
    let show_tag = inner.width >= 40;
    let text_width = inner.width.saturating_sub(1).max(1); // one column for the scrollbar
    let lines: Vec<Line> = state
        .log_entries()
        .map(|e| entry_line(e, show_ts, show_tag))
        .collect();
    let total: usize = lines
        .iter()
        .map(|l| wrapped_rows(l, text_width as usize))
        .sum();
    let page = inner.height as usize;
    view.page = page;
    view.total_lines = total;
    let max_off = total.saturating_sub(page);
    if view.follow {
        view.offset_from_bottom = 0;
    }
    view.offset_from_bottom = view.offset_from_bottom.min(max_off);
    let top = max_off - view.offset_from_bottom;

    let para = Paragraph::new(lines)
        .wrap(Wrap { trim: false })
        .scroll((top.min(u16::MAX as usize) as u16, 0));
    frame.render_widget(
        para,
        Rect {
            width: text_width,
            ..inner
        },
    );
    if total > page && inner.width >= 2 {
        let mut sb = ScrollbarState::new(max_off + 1).position(top);
        let sb_area = if bordered {
            area.inner(Margin {
                vertical: 1,
                horizontal: 0,
            })
        } else {
            area
        };
        frame.render_stateful_widget(
            Scrollbar::new(ScrollbarOrientation::VerticalRight),
            sb_area,
            &mut sb,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::shell::state::Mode;
    use ratatui::{backend::TestBackend, Terminal};

    fn render(w: u16, h: u16, st: &ShellState, view: &mut ViewState) -> String {
        let mut t = Terminal::new(TestBackend::new(w, h)).unwrap();
        t.draw(|f| draw(f, st, view)).unwrap();
        let buf = t.backend().buffer().clone();
        let mut s = String::new();
        for y in 0..buf.area.height {
            for x in 0..buf.area.width {
                s.push_str(buf[(x, y)].symbol());
            }
            s.push('\n');
        }
        s
    }

    fn sample() -> ShellState {
        let mut st = ShellState::new("SAP Automation", Mode::Unattended);
        st.tcode = Some("VL06O".into());
        st.command_line = "sap_automation --tcode vl06o --layout X".into();
        st.sap = SapStatus::Connected(Some("PRD/100".into()));
        for i in 0..200 {
            st.log(Level::Info, Some("s1"), format!("export line {i}"));
        }
        st.log(Level::Error, None, "something failed");
        st
    }

    #[test]
    fn normal_sizes_show_key_text() {
        for (w, h) in [(80u16, 24u16), (200, 60)] {
            let st = sample();
            let mut v = ViewState::default();
            let out = render(w, h, &st, &mut v);
            assert!(out.contains("SAP Automation"), "{w}x{h}");
            assert!(out.contains("VL06O"));
            assert!(out.contains("sap_automation --tcode vl06o"));
            assert!(out.contains("PRD/100"));
            assert!(out.contains("unattended"));
            assert!(out.contains("something failed"), "tail is followed");
        }
    }

    #[test]
    fn tiny_sizes_do_not_panic() {
        let st = sample();
        for (w, h) in [
            (40u16, 10u16),
            (20, 5),
            (1, 1),
            (0, 0),
            (3, 40),
            (200, 1),
            (5, 2),
        ] {
            let mut v = ViewState::default();
            render(w, h, &st, &mut v);
            v.page_up();
            render(w, h, &st, &mut v);
            v.home();
            render(w, h, &st, &mut v);
        }
    }

    #[test]
    fn small_hides_cmdline_and_timestamps() {
        let st = sample();
        let mut v = ViewState::default();
        let out = render(40, 10, &st, &mut v);
        assert!(!out.contains("sap_automation --tcode"));
        assert!(!out.contains(&st.log_entries().next().unwrap().timestamp));
    }

    #[test]
    fn help_overlay_renders_and_survives_every_size() {
        let st = sample();
        let mut v = ViewState {
            help: true,
            ..Default::default()
        };
        let out = render(100, 30, &st, &mut v);
        assert!(out.contains("Keys") && out.contains("follow the end"));
        for (w, h) in [(1, 1), (5, 3), (30, 8), (60, 15), (200, 60)] {
            render(w, h, &st, &mut v);
        }
    }

    #[test]
    fn scrolling_changes_view() {
        let st = sample();
        let mut v = ViewState::default();
        render(80, 24, &st, &mut v);
        v.home();
        let out = render(80, 24, &st, &mut v);
        assert!(out.contains("export line 0"));
        assert!(!out.contains("something failed"));
        assert!(!v.follow);
    }

    fn with_sessions(n: usize, progress: Option<(usize, usize)>) -> ShellState {
        use crate::shell::state::{SessionRow, SessionSnapshot};
        let mut st = sample();
        st.sessions = Some(SessionSnapshot {
            elapsed: std::time::Duration::from_secs(75),
            progress,
            rows: (0..n)
                .map(|i| SessionRow {
                    tag: format!("s{}", i + 1),
                    state: if i == 1 { "done" } else { "running" }.into(),
                    task: format!("relabel for day {i}"),
                    last: format!("latest message {i}"),
                    task_elapsed: (i != 1).then(|| std::time::Duration::from_secs(5)),
                })
                .collect(),
        });
        st
    }

    #[test]
    fn pane_layout_rules() {
        // fixed = header + cmd + footer = 3
        assert_eq!(session_pane_layout(40, 3, 0), (0, PaneLayout::Hidden));
        assert_eq!(
            session_pane_layout(40, 3, 3),
            (5, PaneLayout::Table { rows: 3 })
        );
        // log keeps MIN_LOG_ROWS: 15 - 3 - 6 = 6 rows available, 10 sessions -> truncated
        assert_eq!(
            session_pane_layout(15, 3, 10),
            (6, PaneLayout::Table { rows: 4 })
        );
        // two spare rows -> one-line summary; none -> hidden
        assert_eq!(session_pane_layout(11, 3, 3), (1, PaneLayout::Summary));
        assert_eq!(session_pane_layout(10, 3, 3), (1, PaneLayout::Summary));
        assert_eq!(session_pane_layout(9, 3, 3), (0, PaneLayout::Hidden));
        assert_eq!(session_pane_layout(0, 0, 3), (0, PaneLayout::Hidden));
        for h in 0..60u16 {
            for n in 0..9usize {
                let (ph, _) = session_pane_layout(h, 3, n);
                assert!(ph == 0 || h.saturating_sub(3 + ph) >= MIN_LOG_ROWS);
            }
        }
    }

    #[test]
    fn sessions_pane_renders_table_and_gauge() {
        let st = with_sessions(3, Some((2, 8)));
        let mut v = ViewState::default();
        let out = render(120, 30, &st, &mut v);
        assert!(out.contains("sessions 2/3 active"));
        assert!(out.contains("days 2/8"));
        assert!(out.contains("relabel for day 0"));
        assert!(out.contains("latest message 2"));
        assert!(out.contains("something failed"), "log still visible");
    }

    #[test]
    fn sessions_pane_narrow_and_collapsed() {
        let st = with_sessions(3, Some((1, 2)));
        let mut v = ViewState::default();
        // narrow: task column dropped, progress moves into the summary text
        let out = render(55, 30, &st, &mut v);
        assert!(out.contains("sessions 2/3 active"));
        assert!(out.contains("latest message 0"));
        assert!(!out.contains("relabel for day 0"));
        // short terminal: one summary line, no table rows
        let out = render(120, 10, &st, &mut v);
        assert!(out.contains("sessions 2/3 active"));
        assert!(!out.contains("latest message"));
        // very short: hidden, log gets the room
        let out = render(120, 8, &st, &mut v);
        assert!(!out.contains("sessions 2/3 active"));
    }

    #[test]
    fn sessions_pane_footer_has_days_and_tiny_sizes_do_not_panic() {
        let st = with_sessions(6, Some((3, 9)));
        let mut v = ViewState::default();
        let out = render(120, 40, &st, &mut v);
        assert!(out.lines().last().unwrap().contains("days 3/9"));
        for (w, h) in [
            (0u16, 0u16),
            (1, 1),
            (5, 3),
            (20, 8),
            (30, 12),
            (60, 14),
            (200, 3),
        ] {
            let st = with_sessions(6, Some((0, 0)));
            let mut v = ViewState::default();
            render(w, h, &st, &mut v);
        }
    }

    #[test]
    fn empty_log_and_long_cmdline() {
        let mut st = ShellState::new("T", Mode::Interactive);
        st.command_line = "x ".repeat(200);
        let mut v = ViewState::default();
        let out = render(80, 24, &st, &mut v);
        assert!(out.contains("..."));
    }

    fn menu_state(n: usize) -> ShellState {
        use crate::shell::menu::{MenuSpec, UiRequest};
        let mut st = ShellState::new("SAP Automation", Mode::Interactive);
        st.sap = SapStatus::Connected(Some("logged in".into()));
        for i in 0..30 {
            st.log(Level::Info, None, format!("old output {i}"));
        }
        st.request = Some(UiRequest::Menu(MenuSpec {
            title: "SAP Automation - Main Menu".into(),
            items: (0..n).map(|i| format!("Action number {i}")).collect(),
            default: Some(2),
            id: 1,
        }));
        st
    }

    #[test]
    fn menu_view_shows_menu_footer_and_log_tail() {
        let st = menu_state(33);
        let mut v = ViewState::default();
        let out = render(100, 40, &st, &mut v);
        assert!(out.contains("Main Menu"));
        assert!(out.contains("> "), "selection marker");
        assert!(
            out.contains("3. Action number 2"),
            "default highlighted item shown"
        );
        assert!(out.contains("interactive"));
        assert!(out.contains("logged in"));
        assert!(out.contains("Up/Down Enter"));
        assert!(
            out.contains("old output 29"),
            "log tail under the menu on tall screens"
        );
        // short screen: menu only
        let out = render(100, 14, &st, &mut v);
        assert!(out.contains("Action number 2"));
        assert!(!out.contains("old output"));
    }

    #[test]
    fn menu_and_ack_views_survive_every_size() {
        for ack in [false, true] {
            let mut st = menu_state(40);
            if ack {
                st.request = Some(crate::shell::menu::UiRequest::Acknowledge { ok: false });
            }
            let mut v = ViewState::default();
            for w in [0u16, 1, 5, 12, 30, 80, 250] {
                for h in [0u16, 1, 2, 4, 8, 16, 17, 24, 60] {
                    render(w, h, &st, &mut v);
                }
            }
        }
    }

    #[test]
    fn ack_view_shows_return_hint_and_log() {
        let mut st = menu_state(3);
        st.request = Some(crate::shell::menu::UiRequest::Acknowledge { ok: true });
        let mut v = ViewState::default();
        let out = render(120, 24, &st, &mut v);
        assert!(out.contains("back to menu"));
        assert!(out.contains("old output 29"));
        assert!(!out.contains("Action number"));
    }
}
