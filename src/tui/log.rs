use std::sync::Mutex;

use ratatui::{
    prelude::*,
    widgets::{Block, Borders, Paragraph},
};

// ---------------------------------------------------------------------------
// Global log store
// ---------------------------------------------------------------------------

static LOG_ENTRIES: Mutex<Vec<LogEntry>> = Mutex::new(Vec::new());

#[derive(Debug, Clone)]
pub struct LogEntry {
    pub level: LogLevel,
    pub message: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LogLevel {
    Info,
    Warn,
    Error,
}

/// Append a log entry to the global store.
pub fn push_log(level: LogLevel, message: String) {
    if let Ok(mut entries) = LOG_ENTRIES.lock() {
        entries.push(LogEntry { level, message });
    }
}

/// Read a snapshot of all log entries.
pub fn read_logs() -> Vec<LogEntry> {
    LOG_ENTRIES
        .lock()
        .map(|e| e.clone())
        .unwrap_or_default()
}

/// Convenience macros for logging.
#[macro_export]
macro_rules! tui_log {
    ($($arg:tt)*) => {
        $crate::tui::log::push_log($crate::tui::log::LogLevel::Info, format!($($arg)*))
    };
}

#[macro_export]
macro_rules! tui_warn {
    ($($arg:tt)*) => {
        $crate::tui::log::push_log($crate::tui::log::LogLevel::Warn, format!($($arg)*))
    };
}

#[macro_export]
macro_rules! tui_error {
    ($($arg:tt)*) => {
        $crate::tui::log::push_log($crate::tui::log::LogLevel::Error, format!($($arg)*))
    };
}

// ---------------------------------------------------------------------------
// Debug panel state
// ---------------------------------------------------------------------------

pub struct DebugPanel {
    pub visible: bool,
    pub scroll: usize,
    /// If true the panel auto-scrolls to newest entry.
    pub follow: bool,
}

impl DebugPanel {
    pub fn new() -> Self {
        Self {
            visible: false,
            scroll: 0,
            follow: true,
        }
    }

    pub fn toggle(&mut self) {
        self.visible = !self.visible;
    }

    pub fn scroll_up(&mut self) {
        self.follow = false;
        self.scroll = self.scroll.saturating_sub(1);
    }

    pub fn scroll_down(&mut self) {
        self.scroll = self.scroll.saturating_add(1);
        // follow will be re-checked at render time.
    }

    pub fn render(&mut self, frame: &mut Frame, area: Rect, is_active: bool) {
        let entries = read_logs();
        let total = entries.len();
        let height = area.height.saturating_sub(2) as usize; // borders take 2

        // Auto-follow: pin scroll to bottom.
        if self.follow || self.scroll + height >= total {
            self.scroll = total.saturating_sub(height);
            self.follow = true;
        }

        let lines: Vec<Line> = entries
            .iter()
            .skip(self.scroll)
            .take(height)
            .map(|entry| {
                let (tag, color) = match entry.level {
                    LogLevel::Info => ("[INFO] ", Color::Cyan),
                    LogLevel::Warn => ("[WARN] ", Color::Yellow),
                    LogLevel::Error => ("[ERR]  ", Color::Red),
                };
                Line::from(vec![
                    Span::styled(tag, Style::default().fg(color)),
                    Span::raw(&entry.message),
                ])
            })
            .collect();

        let border_color = if is_active { Color::Cyan } else { Color::DarkGray };
        let block = Block::default()
            .borders(Borders::ALL)
            .title(format!(" Debug Log ({total}) "))
            .border_style(Style::default().fg(border_color));

        let paragraph = Paragraph::new(lines).block(block);
        frame.render_widget(paragraph, area);
    }
}
