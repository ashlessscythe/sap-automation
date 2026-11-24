//! Plain data behind the full-screen shell: header/status info and the bounded log buffer.
#![allow(dead_code)]

use std::collections::VecDeque;
use std::time::{Duration, Instant};

/// Maximum number of log entries kept (oldest are dropped first).
pub const LOG_CAP: usize = 5000;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Level {
    Debug,
    Info,
    Warn,
    Error,
}

impl Level {
    pub fn label(self) -> &'static str {
        match self {
            Level::Debug => "DBG",
            Level::Info => "INF",
            Level::Warn => "WRN",
            Level::Error => "ERR",
        }
    }
}

#[derive(Clone, Debug)]
pub struct LogEntry {
    pub level: Level,
    /// Session tag such as `s3` in parallel runs.
    pub tag: Option<String>,
    /// Pre-formatted local time, `HH:MM:SS`.
    pub timestamp: String,
    pub text: String,
}

impl LogEntry {
    /// New entry stamped with the current local time.
    pub fn now(level: Level, tag: Option<&str>, text: impl Into<String>) -> Self {
        LogEntry {
            level,
            tag: tag.map(str::to_string),
            timestamp: chrono::Local::now().format("%H:%M:%S").to_string(),
            text: text.into(),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    Interactive,
    Unattended,
}

impl Mode {
    pub fn label(self) -> &'static str {
        match self {
            Mode::Interactive => "interactive",
            Mode::Unattended => "unattended",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SapStatus {
    Unknown,
    /// Optional system / client text such as `PRD/100`.
    Connected(Option<String>),
    Disconnected,
}

/// Everything the shell shows. Share it as `Arc<Mutex<ShellState>>` (see [`super::SharedShell`]).
#[derive(Clone, Debug)]
pub struct ShellState {
    pub title: String,
    pub tcode: Option<String>,
    pub command_line: String,
    pub mode: Mode,
    pub sap: SapStatus,
    pub started: Instant,
    /// Frozen elapsed time (set when the run is finished), else `started.elapsed()`.
    pub finished_after: Option<Duration>,
    /// Free-form `(label, value)` items shown in the status bar.
    pub extras: Vec<(String, String)>,
    log: VecDeque<LogEntry>,
    /// Total entries ever pushed, including dropped ones.
    pub total_logged: u64,
}

impl ShellState {
    pub fn new(title: impl Into<String>, mode: Mode) -> Self {
        ShellState {
            title: title.into(),
            tcode: None,
            command_line: String::new(),
            mode,
            sap: SapStatus::Unknown,
            started: Instant::now(),
            finished_after: None,
            extras: Vec::new(),
            log: VecDeque::new(),
            total_logged: 0,
        }
    }

    pub fn with_current_command_line(mut self) -> Self {
        self.command_line = command_line_from(std::env::args());
        self
    }

    pub fn push_log(&mut self, entry: LogEntry) {
        if self.log.len() >= LOG_CAP {
            self.log.pop_front();
        }
        self.log.push_back(entry);
        self.total_logged += 1;
    }

    pub fn log(&mut self, level: Level, tag: Option<&str>, text: impl Into<String>) {
        self.push_log(LogEntry::now(level, tag, text));
    }

    pub fn log_len(&self) -> usize {
        self.log.len()
    }

    pub fn log_entries(&self) -> impl DoubleEndedIterator<Item = &LogEntry> + ExactSizeIterator {
        self.log.iter()
    }

    pub fn count(&self, level: Level) -> usize {
        self.log.iter().filter(|e| e.level == level).count()
    }

    pub fn clear_log(&mut self) {
        self.log.clear();
    }

    pub fn set_extra(&mut self, label: &str, value: impl Into<String>) {
        let value = value.into();
        match self.extras.iter_mut().find(|(l, _)| l == label) {
            Some(item) => item.1 = value,
            None => self.extras.push((label.to_string(), value)),
        }
    }

    pub fn remove_extra(&mut self, label: &str) {
        self.extras.retain(|(l, _)| l != label);
    }

    pub fn finish(&mut self) {
        self.finished_after = Some(self.started.elapsed());
    }

    pub fn elapsed(&self) -> Duration {
        self.finished_after
            .unwrap_or_else(|| self.started.elapsed())
    }
}

/// Scroll / follow state of the log window (owned by the UI thread, not shared).
#[derive(Clone, Debug)]
pub struct ViewState {
    /// Lines hidden below the visible window (0 = at the tail).
    pub offset_from_bottom: usize,
    pub follow: bool,
    /// Set by `draw`: visible rows of the log window, used for page scrolling.
    pub page: usize,
    /// Set by `draw`: wrapped line count of the log, used for clamping.
    pub total_lines: usize,
    pub quit: bool,
}

impl Default for ViewState {
    fn default() -> Self {
        ViewState {
            offset_from_bottom: 0,
            follow: true,
            page: 10,
            total_lines: 0,
            quit: false,
        }
    }
}

impl ViewState {
    fn max_offset(&self) -> usize {
        self.total_lines.saturating_sub(self.page)
    }

    pub fn scroll_up(&mut self, n: usize) {
        self.follow = false;
        self.offset_from_bottom = self
            .offset_from_bottom
            .saturating_add(n)
            .min(self.max_offset());
    }

    pub fn scroll_down(&mut self, n: usize) {
        self.offset_from_bottom = self.offset_from_bottom.saturating_sub(n);
        if self.offset_from_bottom == 0 {
            self.follow = true;
        }
    }

    pub fn page_up(&mut self) {
        self.scroll_up(self.page.max(1));
    }

    pub fn page_down(&mut self) {
        self.scroll_down(self.page.max(1));
    }

    pub fn home(&mut self) {
        self.follow = false;
        self.offset_from_bottom = self.max_offset();
    }

    pub fn end(&mut self) {
        self.offset_from_bottom = 0;
        self.follow = true;
    }

    pub fn toggle_follow(&mut self) {
        if self.follow {
            self.follow = false;
        } else {
            self.end();
        }
    }
}

/// Quote one argument for display: wrapped in double quotes when it is empty or has whitespace/quotes.
pub fn quote_arg(arg: &str) -> String {
    if !arg.is_empty() && !arg.chars().any(|c| c.is_whitespace() || c == '"') {
        return arg.to_string();
    }
    format!("\"{}\"", arg.replace('"', "\\\""))
}

/// Join arguments (program path reduced to its file stem) into one display string.
pub fn command_line_from<I, S>(args: I) -> String
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    args.into_iter()
        .enumerate()
        .map(|(i, a)| {
            let a = a.as_ref();
            if i == 0 {
                let file = a.rsplit(['\\', '/']).next().unwrap_or(a);
                let stem = file.strip_suffix(".exe").unwrap_or(file);
                quote_arg(stem)
            } else {
                quote_arg(a)
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// Truncate to `max` chars (not bytes) with a trailing `...`; `max` of 0 gives an empty string.
pub fn truncate_ellipsis(s: &str, max: usize) -> String {
    let count = s.chars().count();
    if count <= max {
        return s.to_string();
    }
    if max <= 3 {
        return s.chars().take(max).collect();
    }
    let mut out: String = s.chars().take(max - 3).collect();
    out.push_str("...");
    out
}

/// `H:MM:SS` or `MM:SS`.
pub fn format_elapsed(d: Duration) -> String {
    let s = d.as_secs();
    if s >= 3600 {
        format!("{}:{:02}:{:02}", s / 3600, (s / 60) % 60, s % 60)
    } else {
        format!("{:02}:{:02}", s / 60, s % 60)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ring_buffer_is_capped() {
        let mut st = ShellState::new("t", Mode::Interactive);
        for i in 0..(LOG_CAP + 25) {
            st.log(Level::Info, None, format!("line {i}"));
        }
        assert_eq!(st.log_len(), LOG_CAP);
        assert_eq!(st.total_logged, (LOG_CAP + 25) as u64);
        assert_eq!(st.log_entries().next().unwrap().text, "line 25");
    }

    #[test]
    fn quoting() {
        assert_eq!(quote_arg("abc"), "abc");
        assert_eq!(quote_arg("a b"), "\"a b\"");
        assert_eq!(quote_arg(""), "\"\"");
        assert_eq!(quote_arg("a\"b"), "\"a\\\"b\"");
        let cl = command_line_from(["C:\\x\\sap_automation.exe", "--rname", "a b"]);
        assert_eq!(cl, "sap_automation --rname \"a b\"");
    }

    #[test]
    fn truncation() {
        assert_eq!(truncate_ellipsis("hello", 10), "hello");
        assert_eq!(truncate_ellipsis("hello world", 8), "hello...");
        assert_eq!(truncate_ellipsis("hello", 0), "");
        assert_eq!(truncate_ellipsis("hello", 2), "he");
        assert_eq!(truncate_ellipsis("äöüäöüäöü", 5), "äö...");
    }

    #[test]
    fn extras_and_elapsed() {
        let mut st = ShellState::new("t", Mode::Unattended);
        st.set_extra("days", "1/3");
        st.set_extra("days", "2/3");
        assert_eq!(st.extras.len(), 1);
        assert_eq!(st.extras[0].1, "2/3");
        assert_eq!(format_elapsed(Duration::from_secs(75)), "01:15");
        assert_eq!(format_elapsed(Duration::from_secs(3725)), "1:02:05");
    }

    #[test]
    fn view_scrolling() {
        let mut v = ViewState {
            page: 10,
            total_lines: 100,
            ..Default::default()
        };
        v.page_up();
        assert!(!v.follow);
        assert_eq!(v.offset_from_bottom, 10);
        v.home();
        assert_eq!(v.offset_from_bottom, 90);
        v.scroll_up(1000);
        assert_eq!(v.offset_from_bottom, 90);
        v.end();
        assert!(v.follow);
        v.scroll_down(5);
        assert_eq!(v.offset_from_bottom, 0);
    }
}
