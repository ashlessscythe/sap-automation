//! Pure rendering of the shell: `draw(frame, &ShellState, &mut ViewState)`.
#![allow(dead_code)]

use super::state::{
    format_elapsed, truncate_ellipsis, Level, LogEntry, SapStatus, ShellState, ViewState,
};
use ratatui::{
    layout::{Constraint, Layout, Margin, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, Scrollbar, ScrollbarOrientation, ScrollbarState, Wrap},
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
    optional.extend(state.extras.iter().map(|(l, v)| format!("{l}: {v}")));
    optional.push(format!(
        "{} lines, {} warn, {} err",
        state.log_len(),
        state.count(Level::Warn),
        state.count(Level::Error)
    ));
    optional.push(if view.follow { "follow" } else { "paused" }.to_string());
    optional.push("q quit  PgUp/PgDn  Home/End  f follow".to_string());
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
    let rows = Layout::vertical([
        Constraint::Length(1),
        Constraint::Length(u16::from(show_cmd)),
        Constraint::Min(0),
        Constraint::Length(u16::from(area.height >= 2)),
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
    draw_log(frame, rows[2], state, view);
    if rows[3].height > 0 {
        frame.render_widget(
            Paragraph::new(status_line(state, view, area.width as usize)),
            rows[3],
        );
    }
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

    #[test]
    fn empty_log_and_long_cmdline() {
        let mut st = ShellState::new("T", Mode::Interactive);
        st.command_line = "x ".repeat(200);
        let mut v = ViewState::default();
        let out = render(80, 24, &st, &mut v);
        assert!(out.contains("..."));
    }
}
