//! Ratatui menus; some widgets (e.g. input dialog) are kept for future UX paths.
#![allow(dead_code)]

use crossterm::{
    cursor::{Hide, Show},
    event::{self, Event, KeyCode, KeyEventKind},
    execute,
    terminal::{
        disable_raw_mode, enable_raw_mode, Clear, ClearType, EnterAlternateScreen,
        LeaveAlternateScreen,
    },
};
use ratatui::{
    backend::CrosstermBackend,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, ListState, Paragraph, Row, Table, Wrap},
    Frame, Terminal,
};
use std::io;
use std::time::Duration;

/// Version from Cargo.toml
const VERSION: &str = env!("CARGO_PKG_VERSION");

/// TUI App state
pub struct App {
    pub state: ListState,
    pub items: Vec<String>,
    pub title: String,
    pub selected: Option<usize>,
    pub scroll_offset: usize,
}

impl App {
    pub fn new(title: String, items: Vec<String>) -> App {
        App {
            state: ListState::default(),
            items,
            title,
            selected: None,
            scroll_offset: 0,
        }
    }

    pub fn next(&mut self) {
        let i = match self.state.selected() {
            Some(i) => {
                if i >= self.items.len() - 1 {
                    0
                } else {
                    i + 1
                }
            }
            None => 0,
        };
        self.state.select(Some(i));
        self.selected = Some(i);
        self.update_scroll_offset();
    }

    pub fn previous(&mut self) {
        let i = match self.state.selected() {
            Some(i) => {
                if i == 0 {
                    self.items.len() - 1
                } else {
                    i - 1
                }
            }
            None => 0,
        };
        self.state.select(Some(i));
        self.selected = Some(i);
        self.update_scroll_offset();
    }

    pub fn page_down(&mut self) {
        let i = match self.state.selected() {
            Some(i) => {
                let page_size = 10; // Jump by 10 items
                if i + page_size >= self.items.len() {
                    self.items.len() - 1
                } else {
                    i + page_size
                }
            }
            None => 0,
        };
        self.state.select(Some(i));
        self.selected = Some(i);
        self.update_scroll_offset();
    }

    pub fn page_up(&mut self) {
        let i = match self.state.selected() {
            Some(i) => {
                let page_size = 10; // Jump by 10 items
                i.saturating_sub(page_size)
            }
            None => 0,
        };
        self.state.select(Some(i));
        self.selected = Some(i);
        self.update_scroll_offset();
    }

    pub fn go_to_end(&mut self) {
        let i = self.items.len() - 1;
        self.state.select(Some(i));
        self.selected = Some(i);
        self.update_scroll_offset();
    }

    pub fn go_to_home(&mut self) {
        self.state.select(Some(0));
        self.selected = Some(0);
        self.update_scroll_offset();
    }

    fn update_scroll_offset(&mut self) {
        if let Some(selected) = self.state.selected() {
            // Keep selected item in the middle of visible area when possible
            // We'll calculate the proper offset in the UI based on available space
            self.scroll_offset = selected;
        }
    }

    pub fn get_selected(&self) -> Option<usize> {
        self.state.selected()
    }
}

/// Display a selection menu with TUI
pub fn show_selection_menu(
    title: &str,
    items: Vec<String>,
    default_selection: Option<usize>,
) -> io::Result<Option<usize>> {
    // Terminal setup
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, Hide)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    // App setup
    let mut app = App::new(title.to_string(), items);
    if let Some(default) = default_selection {
        app.state.select(Some(default));
        app.selected = Some(default);
    } else {
        app.state.select(Some(0));
        app.selected = Some(0);
    }

    let res = run_app(&mut terminal, &mut app);

    // Restore terminal
    disable_raw_mode()?;
    execute!(terminal.backend_mut(), Show, Clear(ClearType::All))?;
    terminal.show_cursor()?;

    res
}

fn run_app(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    app: &mut App,
) -> io::Result<Option<usize>> {
    loop {
        terminal.draw(|f| ui(f, app))?;

        if crossterm::event::poll(Duration::from_millis(250))? {
            if let Event::Key(key) = event::read()? {
                if key.kind == KeyEventKind::Press {
                    match key.code {
                        KeyCode::Char('q') | KeyCode::Esc => {
                            return Ok(None);
                        }
                        KeyCode::Char('j') | KeyCode::Down => {
                            app.next();
                        }
                        KeyCode::Char('k') | KeyCode::Up => {
                            app.previous();
                        }
                        KeyCode::PageDown => {
                            app.page_down();
                        }
                        KeyCode::PageUp => {
                            app.page_up();
                        }
                        KeyCode::End => {
                            app.go_to_end();
                        }
                        KeyCode::Home => {
                            app.go_to_home();
                        }
                        KeyCode::Char(' ') | KeyCode::Enter => {
                            return Ok(app.get_selected());
                        }
                        _ => {}
                    }
                }
            }
        }
    }
}

fn ui(f: &mut Frame, app: &App) {
    // Create the main layout
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .margin(2)
        .constraints(
            [
                Constraint::Length(3), // Title
                Constraint::Min(5),    // Main content (minimum 5 lines)
                Constraint::Length(5), // Instructions (increased from 3 to 5)
            ]
            .as_ref(),
        )
        .split(f.area());

    // Add version display in lower right corner
    let version_text = format!("v{VERSION}");
    let version_span = Span::styled(
        &version_text,
        Style::default().fg(Color::Gray).add_modifier(Modifier::DIM),
    );

    // Calculate position for version (lower right, accounting for margins)
    let version_area = Rect::new(
        f.area().width.saturating_sub(version_text.len() as u16 + 2),
        f.area().height.saturating_sub(1),
        version_text.len() as u16,
        1,
    );

    f.render_widget(Paragraph::new(Line::from(vec![version_span])), version_area);

    // Calculate visible area and adjust scroll offset
    let visible_height = chunks[1].height.saturating_sub(2); // Account for borders
    let selected_index = app.state.selected().unwrap_or(0);

    // Calculate scroll offset to keep selected item in the middle
    let mut adjusted_scroll_offset = selected_index.saturating_sub(visible_height as usize / 2);

    // Ensure we don't scroll past the end
    let max_scroll = app.items.len().saturating_sub(visible_height as usize);
    adjusted_scroll_offset = adjusted_scroll_offset.min(max_scroll);

    // Title
    let title = Paragraph::new(app.title.clone())
        .style(
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        )
        .alignment(ratatui::layout::Alignment::Center)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .style(Style::default().fg(Color::Gray)),
        );
    f.render_widget(title, chunks[0]);

    // Main content area
    let main_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(100)].as_ref())
        .split(chunks[1]);

    // Create list items with styling (only visible items)
    let visible_items: Vec<ListItem> = app
        .items
        .iter()
        .enumerate()
        .skip(adjusted_scroll_offset)
        .take(visible_height as usize)
        .map(|(i, item)| {
            let style = if app.state.selected() == Some(i) {
                Style::default()
                    .fg(Color::Black)
                    .bg(Color::Blue)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(Color::White)
            };

            ListItem::new(vec![Line::from(vec![Span::styled(
                format!("  {item}"),
                style,
            )])])
        })
        .collect();

    // Create a temporary state for the visible items
    let mut visible_state = ListState::default();
    if let Some(selected) = app.state.selected() {
        if selected >= adjusted_scroll_offset
            && selected < adjusted_scroll_offset + visible_items.len()
        {
            visible_state.select(Some(selected - adjusted_scroll_offset));
        }
    }

    let list = List::new(visible_items)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .style(Style::default().fg(Color::Gray))
                .title("Options"),
        )
        .highlight_style(
            Style::default()
                .fg(Color::Black)
                .bg(Color::Blue)
                .add_modifier(Modifier::BOLD),
        )
        .highlight_symbol("> ");

    f.render_stateful_widget(list, main_chunks[0], &mut visible_state);

    // Create instructions with status indicator
    let status_text = format!("{} / {}", selected_index + 1, app.items.len());
    let instructions = Paragraph::new(vec![
        Line::from(vec![
            Span::styled(
                "Navigation: ",
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled("↑/↓ or j/k", Style::default().fg(Color::White)),
            Span::styled(
                "    Status: ",
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                status_text,
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            ),
        ]),
        Line::from(vec![
            Span::styled(
                "Page: ",
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled("PgUp/PgDn", Style::default().fg(Color::White)),
            Span::styled(
                "    Jump: ",
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled("Home/End", Style::default().fg(Color::White)),
        ]),
        Line::from(vec![
            Span::styled(
                "Select: ",
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled("Enter or Space", Style::default().fg(Color::White)),
            Span::styled(
                "    Exit: ",
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled("q or Esc", Style::default().fg(Color::White)),
        ]),
    ])
    .style(Style::default().fg(Color::Gray))
    .block(
        Block::default()
            .borders(Borders::ALL)
            .style(Style::default().fg(Color::Gray)),
    );
    f.render_widget(instructions, chunks[2]);
}

/// Display a grid-based selection menu (alternative to list view)
#[allow(dead_code)]
pub fn show_grid_menu(
    title: &str,
    items: Vec<String>,
    default_selection: Option<usize>,
) -> io::Result<Option<usize>> {
    // Terminal setup
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, Hide)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    // App setup
    let mut app = App::new(title.to_string(), items);
    if let Some(default) = default_selection {
        app.state.select(Some(default));
        app.selected = Some(default);
    } else {
        app.state.select(Some(0));
        app.selected = Some(0);
    }

    let res = run_grid_app(&mut terminal, &mut app);

    // Restore terminal
    disable_raw_mode()?;
    execute!(terminal.backend_mut(), Show, Clear(ClearType::All))?;
    terminal.show_cursor()?;

    res
}

#[allow(dead_code)]
fn run_grid_app(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    app: &mut App,
) -> io::Result<Option<usize>> {
    loop {
        terminal.draw(|f| grid_ui(f, app))?;

        if crossterm::event::poll(Duration::from_millis(250))? {
            if let Event::Key(key) = event::read()? {
                if key.kind == KeyEventKind::Press {
                    match key.code {
                        KeyCode::Char('q') | KeyCode::Esc => {
                            return Ok(None);
                        }
                        KeyCode::Char('j') | KeyCode::Down => {
                            app.next();
                        }
                        KeyCode::Char('k') | KeyCode::Up => {
                            app.previous();
                        }
                        KeyCode::PageDown => {
                            app.page_down();
                        }
                        KeyCode::PageUp => {
                            app.page_up();
                        }
                        KeyCode::End => {
                            app.go_to_end();
                        }
                        KeyCode::Home => {
                            app.go_to_home();
                        }
                        KeyCode::Char(' ') | KeyCode::Enter => {
                            return Ok(app.get_selected());
                        }
                        _ => {}
                    }
                }
            }
        }
    }
}

#[allow(dead_code)]
fn grid_ui(f: &mut Frame, app: &App) {
    // Create the main layout
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .margin(2)
        .constraints(
            [
                Constraint::Length(3),
                Constraint::Min(0),
                Constraint::Length(3),
            ]
            .as_ref(),
        )
        .split(f.area());

    // Title
    let title = Paragraph::new(app.title.clone())
        .style(
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        )
        .alignment(ratatui::layout::Alignment::Center)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .style(Style::default().fg(Color::Gray)),
        );
    f.render_widget(title, chunks[0]);

    // Main content area - create a grid layout
    let main_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(100)].as_ref())
        .split(chunks[1]);

    // Create table rows for grid display
    let rows: Vec<Row> = app
        .items
        .iter()
        .enumerate()
        .map(|(i, item)| {
            let style = if app.state.selected() == Some(i) {
                Style::default()
                    .fg(Color::Black)
                    .bg(Color::Blue)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(Color::White)
            };

            Row::new(vec![format!("  {}", item)]).style(style)
        })
        .collect();

    let table = Table::new(rows, [Constraint::Percentage(100)])
        .block(
            Block::default()
                .borders(Borders::ALL)
                .style(Style::default().fg(Color::Gray))
                .title("Options"),
        )
        .row_highlight_style(
            Style::default()
                .fg(Color::Black)
                .bg(Color::Blue)
                .add_modifier(Modifier::BOLD),
        )
        .highlight_symbol("> ");

    f.render_widget(table, main_chunks[0]);

    // Instructions
    let instructions = Paragraph::new(vec![
        Line::from(vec![
            Span::styled("↑/↓", Style::default().fg(Color::Yellow)),
            Span::styled(" or ", Style::default().fg(Color::Gray)),
            Span::styled("j/k", Style::default().fg(Color::Yellow)),
            Span::styled(": Navigate", Style::default().fg(Color::Gray)),
        ]),
        Line::from(vec![
            Span::styled("Enter", Style::default().fg(Color::Yellow)),
            Span::styled(" or ", Style::default().fg(Color::Gray)),
            Span::styled("Space", Style::default().fg(Color::Yellow)),
            Span::styled(": Select", Style::default().fg(Color::Gray)),
        ]),
        Line::from(vec![
            Span::styled("q", Style::default().fg(Color::Yellow)),
            Span::styled(" or ", Style::default().fg(Color::Gray)),
            Span::styled("Esc", Style::default().fg(Color::Yellow)),
            Span::styled(": Exit", Style::default().fg(Color::Gray)),
        ]),
    ])
    .style(Style::default().fg(Color::Gray))
    .block(
        Block::default()
            .borders(Borders::ALL)
            .style(Style::default().fg(Color::Gray)),
    );
    f.render_widget(instructions, chunks[2]);
}

/// Simple input dialog using TUI
pub fn show_input_dialog(prompt: &str, default_value: Option<&str>) -> io::Result<Option<String>> {
    // Terminal setup
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, Hide)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    // App setup
    let mut input = default_value.unwrap_or("").to_string();
    let mut cursor_pos = input.len();

    let res = run_input_app(&mut terminal, prompt, &mut input, &mut cursor_pos);

    // Restore terminal
    disable_raw_mode()?;
    execute!(terminal.backend_mut(), Show, Clear(ClearType::All))?;
    terminal.show_cursor()?;

    res
}

fn run_input_app(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    prompt: &str,
    input: &mut String,
    cursor_pos: &mut usize,
) -> io::Result<Option<String>> {
    loop {
        terminal.draw(|f| input_ui(f, prompt, input, *cursor_pos))?;

        if crossterm::event::poll(Duration::from_millis(250))? {
            if let Event::Key(key) = event::read()? {
                if key.kind == KeyEventKind::Press {
                    match key.code {
                        KeyCode::Char('q') | KeyCode::Esc => {
                            return Ok(None);
                        }
                        KeyCode::Char(c) => {
                            input.insert(*cursor_pos, c);
                            *cursor_pos += 1;
                        }
                        KeyCode::Backspace if *cursor_pos > 0 => {
                            input.remove(*cursor_pos - 1);
                            *cursor_pos -= 1;
                        }
                        KeyCode::Delete if *cursor_pos < input.len() => {
                            input.remove(*cursor_pos);
                        }
                        KeyCode::Left if *cursor_pos > 0 => {
                            *cursor_pos -= 1;
                        }
                        KeyCode::Right if *cursor_pos < input.len() => {
                            *cursor_pos += 1;
                        }
                        KeyCode::Enter => {
                            return Ok(Some(input.clone()));
                        }
                        _ => {}
                    }
                }
            }
        }
    }
}

#[allow(dead_code)]
fn input_ui(f: &mut Frame, prompt: &str, input: &str, _cursor_pos: usize) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .margin(2)
        .constraints(
            [
                Constraint::Length(3),
                Constraint::Length(5),
                Constraint::Length(3),
            ]
            .as_ref(),
        )
        .split(f.area());

    // Title
    let title = Paragraph::new("Input Dialog")
        .style(
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        )
        .alignment(ratatui::layout::Alignment::Center)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .style(Style::default().fg(Color::Gray)),
        );
    f.render_widget(title, chunks[0]);

    // Input area
    let input_text = format!("{prompt}: {input}");
    let input_para = Paragraph::new(input_text)
        .style(Style::default().fg(Color::White))
        .block(
            Block::default()
                .borders(Borders::ALL)
                .style(Style::default().fg(Color::Gray))
                .title("Enter Value"),
        );

    f.render_widget(input_para, chunks[1]);

    // Instructions
    let instructions = Paragraph::new(vec![
        Line::from(vec![
            Span::styled("Enter", Style::default().fg(Color::Yellow)),
            Span::styled(": Confirm", Style::default().fg(Color::Gray)),
        ]),
        Line::from(vec![
            Span::styled("Esc", Style::default().fg(Color::Yellow)),
            Span::styled(": Cancel", Style::default().fg(Color::Gray)),
        ]),
    ])
    .style(Style::default().fg(Color::Gray))
    .block(
        Block::default()
            .borders(Borders::ALL)
            .style(Style::default().fg(Color::Gray)),
    );
    f.render_widget(instructions, chunks[2]);
}

/// Scrollable table with a sticky header row (top), scrolling rows (middle) and a stationary
/// status line (bottom), on the alternate screen so the terminal is left as it was.
///
/// Keys: Up/Down (j/k), PageUp/PageDown, Home/End scroll; q, Esc or Enter close. `row_style`
/// colours a row from its cells.
pub fn show_table_view(
    title: &str,
    headers: &[&str],
    rows: &[Vec<String>],
    summary: &str,
    row_style: &dyn Fn(&[String]) -> Style,
) -> io::Result<()> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, Hide)?;
    let mut terminal = Terminal::new(CrosstermBackend::new(stdout))?;
    let res = run_table_view(&mut terminal, title, headers, rows, summary, row_style);
    disable_raw_mode()?;
    execute!(terminal.backend_mut(), Show, LeaveAlternateScreen)?;
    res
}

fn run_table_view(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    title: &str,
    headers: &[&str],
    rows: &[Vec<String>],
    summary: &str,
    row_style: &dyn Fn(&[String]) -> Style,
) -> io::Result<()> {
    use ratatui::widgets::TableState;

    // column widths from the content (the last column takes the rest)
    let mut widths: Vec<usize> = headers.iter().map(|h| h.chars().count()).collect();
    for r in rows {
        for (i, c) in r.iter().enumerate() {
            if i < widths.len() {
                widths[i] = widths[i].max(c.chars().count());
            }
        }
    }
    let last = widths.len().saturating_sub(1);

    let mut state = TableState::default();
    if !rows.is_empty() {
        state.select(Some(0));
    }
    loop {
        let mut page = 10usize;
        terminal.draw(|f| {
            let area = f.area();
            let chunks = Layout::default()
                .direction(Direction::Vertical)
                .constraints([
                    Constraint::Length(title_height(title, area.width as usize)),
                    Constraint::Min(2),
                    Constraint::Length(1),
                    Constraint::Length(1),
                ])
                .split(area);
            page = (chunks[1].height as usize).saturating_sub(2).max(1);
            let col_w = fit_widths(&widths, area.width as usize, 2, 24);
            let constraints: Vec<Constraint> = col_w
                .iter()
                .enumerate()
                .map(|(i, w)| {
                    if i == last {
                        Constraint::Fill(1)
                    } else {
                        Constraint::Length(*w as u16)
                    }
                })
                .collect();
            let last_w = col_w.get(last).copied().unwrap_or(0);
            f.render_widget(
                Paragraph::new(title.to_string())
                    .wrap(Wrap { trim: false })
                    .style(Style::default().add_modifier(Modifier::BOLD)),
                chunks[0],
            );
            let header = Row::new(headers.iter().map(|h| h.to_string())).style(
                Style::default()
                    .fg(Color::Black)
                    .bg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            );
            let body: Vec<Row> = rows
                .iter()
                .map(|r| {
                    let cells: Vec<String> = r
                        .iter()
                        .enumerate()
                        .map(|(i, c)| clip(c, col_w.get(i).copied().unwrap_or(c.len())))
                        .collect();
                    Row::new(cells).style(row_style(r))
                })
                .collect();
            let table = Table::new(body, constraints)
                .header(header)
                .column_spacing(2)
                .row_highlight_style(Style::default().add_modifier(Modifier::REVERSED));
            f.render_stateful_widget(table, chunks[1], &mut state);
            // the full text of the last column of the selected row (the table clips long ones)
            let full = state
                .selected()
                .and_then(|s| rows.get(s))
                .and_then(|r| r.get(last))
                .cloned()
                .unwrap_or_default();
            let detail = if full.chars().count() > last_w {
                format!(" {full}")
            } else {
                String::new()
            };
            f.render_widget(
                Paragraph::new(detail).style(Style::default().fg(Color::Gray)),
                chunks[2],
            );
            let pos = state.selected().map(|s| s + 1).unwrap_or(0);
            let status = format!(
                " {pos}/{}   Up/Down PgUp/PgDn Home/End scroll   q close   |   {summary}",
                rows.len()
            );
            f.render_widget(
                Paragraph::new(status).style(Style::default().add_modifier(Modifier::REVERSED)),
                chunks[3],
            );
        })?;

        if crossterm::event::poll(Duration::from_millis(250))? {
            if let Event::Key(key) = event::read()? {
                if key.kind != KeyEventKind::Press {
                    continue;
                }
                let n = rows.len();
                let cur = state.selected().unwrap_or(0);
                let to = match key.code {
                    KeyCode::Char('q') | KeyCode::Esc | KeyCode::Enter => return Ok(()),
                    KeyCode::Down | KeyCode::Char('j') => Some((cur + 1).min(n.saturating_sub(1))),
                    KeyCode::Up | KeyCode::Char('k') => Some(cur.saturating_sub(1)),
                    KeyCode::PageDown => Some((cur + page).min(n.saturating_sub(1))),
                    KeyCode::PageUp => Some(cur.saturating_sub(page)),
                    KeyCode::Home => Some(0),
                    KeyCode::End => Some(n.saturating_sub(1)),
                    _ => None,
                };
                if let (Some(t), true) = (to, n > 0) {
                    state.select(Some(t));
                }
            }
        }
    }
}

/// Rows the (wrapped, possibly multi-line) title needs: at least 1, at most 6.
fn title_height(title: &str, width: usize) -> u16 {
    let width = width.max(1);
    let rows: usize = title
        .lines()
        .map(|l| l.chars().count().div_ceil(width).max(1))
        .sum();
    rows.clamp(1, 6) as u16
}

/// Cut `s` to `width` characters, ending in `~` when something was left out.
fn clip(s: &str, width: usize) -> String {
    if s.chars().count() <= width {
        s.to_string()
    } else if width <= 1 {
        s.chars().take(width).collect()
    } else {
        let mut t: String = s.chars().take(width - 1).collect();
        t.push('~');
        t
    }
}

/// Column widths that fit a terminal of `total` characters: the last column gets at least
/// `min_last`, the other columns shrink (widest first, never below 4) when they do not fit.
fn fit_widths(widths: &[usize], total: usize, spacing: usize, min_last: usize) -> Vec<usize> {
    let mut w = widths.to_vec();
    if w.is_empty() {
        return w;
    }
    let last = w.len() - 1;
    for v in w.iter_mut().take(last) {
        *v = (*v).min(40);
    }
    let gaps = spacing * last;
    let budget = total.saturating_sub(gaps + min_last);
    while w[..last].iter().sum::<usize>() > budget {
        let Some((i, _)) = w[..last]
            .iter()
            .enumerate()
            .filter(|(_, v)| **v > 4)
            .max_by_key(|(_, v)| **v)
        else {
            break;
        };
        w[i] -= 1;
    }
    let used = w[..last].iter().sum::<usize>() + gaps;
    w[last] = total.saturating_sub(used).max(1).min(widths[last].max(1));
    w
}

#[cfg(test)]
mod table_tests {
    use super::*;

    #[test]
    fn title_height_counts_lines_and_wrapping() {
        assert_eq!(title_height("one line", 80), 1);
        assert_eq!(title_height("a\nb", 80), 2);
        assert_eq!(title_height(&"x".repeat(100), 40), 3);
        assert_eq!(title_height(&"x".repeat(1000), 10), 6);
        assert_eq!(title_height("", 80), 1);
    }

    #[test]
    fn clip_marks_what_was_cut() {
        assert_eq!(clip("abcdef", 6), "abcdef");
        assert_eq!(clip("abcdef", 4), "abc~");
        assert_eq!(clip("abc", 1), "a");
    }

    #[test]
    fn widths_fit_the_terminal_and_keep_room_for_the_last_column() {
        let widths = vec![4, 8, 8, 4, 10, 3, 5, 5, 10, 120];
        for total in [80usize, 100, 160, 300] {
            let w = fit_widths(&widths, total, 2, 24);
            let sum: usize = w.iter().sum::<usize>() + 2 * (w.len() - 1);
            assert!(
                sum <= total.max(w[..9].iter().sum::<usize>() + 18 + 24),
                "{total}: {w:?}"
            );
            assert!(w[9] >= 24.min(total), "{total}: {w:?}");
        }
        // plenty of room: nothing is cut
        let w = fit_widths(&widths, 300, 2, 24);
        assert_eq!(w[..9], widths[..9]);
    }
}
