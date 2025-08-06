use crossterm::{
    cursor::{Hide, Show},
    event::{self, Event, KeyCode, KeyEventKind},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, Clear, ClearType},
};
use ratatui::{
    backend::{Backend, CrosstermBackend},
    layout::{Constraint, Direction, Layout},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, ListState, Paragraph, Row, Table, TableState},
    Frame, Terminal,
};
use std::io;
use std::time::Duration;

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
                if i < page_size {
                    0
                } else {
                    i - page_size
                }
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

fn run_app<B: Backend>(terminal: &mut Terminal<B>, app: &mut App) -> io::Result<Option<usize>> {
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
        .split(f.size());

    // Calculate visible area and adjust scroll offset
    let visible_height = chunks[1].height.saturating_sub(2); // Account for borders
    let selected_index = app.state.selected().unwrap_or(0);

    // Calculate scroll offset to keep selected item in the middle
    let mut adjusted_scroll_offset = if selected_index > visible_height as usize / 2 {
        selected_index - visible_height as usize / 2
    } else {
        0
    };

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
                format!("  {}", item),
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

fn run_grid_app<B: Backend>(
    terminal: &mut Terminal<B>,
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
        .split(f.size());

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

fn run_input_app<B: Backend>(
    terminal: &mut Terminal<B>,
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
                        KeyCode::Backspace => {
                            if *cursor_pos > 0 {
                                input.remove(*cursor_pos - 1);
                                *cursor_pos -= 1;
                            }
                        }
                        KeyCode::Delete => {
                            if *cursor_pos < input.len() {
                                input.remove(*cursor_pos);
                            }
                        }
                        KeyCode::Left => {
                            if *cursor_pos > 0 {
                                *cursor_pos -= 1;
                            }
                        }
                        KeyCode::Right => {
                            if *cursor_pos < input.len() {
                                *cursor_pos += 1;
                            }
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

fn input_ui(f: &mut Frame, prompt: &str, input: &str, cursor_pos: usize) {
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
        .split(f.size());

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
    let input_text = format!("{}: {}", prompt, input);
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
