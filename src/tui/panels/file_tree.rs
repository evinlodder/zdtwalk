use fuzzy_matcher::skim::SkimMatcherV2;
use fuzzy_matcher::FuzzyMatcher;
use ratatui::{
    prelude::*,
    widgets::{Block, Borders, List, ListItem, Paragraph},
};

use super::super::workspace::FileEntry;

// ---------------------------------------------------------------------------
// File tree modes
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileTreeMode {
    BoardFiles,
    UserOverlays,
    Bindings,
}

impl FileTreeMode {
    pub fn label(self) -> &'static str {
        match self {
            FileTreeMode::BoardFiles => "[1] Board DTS",
            FileTreeMode::UserOverlays => "[2] User Overlays",
            FileTreeMode::Bindings => "[3] Bindings",
        }
    }

    pub fn next(self) -> Self {
        match self {
            FileTreeMode::BoardFiles => FileTreeMode::UserOverlays,
            FileTreeMode::UserOverlays => FileTreeMode::Bindings,
            FileTreeMode::Bindings => FileTreeMode::BoardFiles,
        }
    }
}

// ---------------------------------------------------------------------------
// State
// ---------------------------------------------------------------------------

pub struct FileTreeState {
    pub mode: FileTreeMode,
    pub boards: Vec<String>,
    pub boards_loaded: bool,
    pub selected_board: Option<usize>,
    pub board_picker_open: bool,

    entries: Vec<FileEntry>,
    /// Indices into `entries` after fuzzy filtering.
    filtered: Option<Vec<usize>>,
    /// Indices into `boards` after fuzzy filtering (board picker mode).
    filtered_boards: Option<Vec<usize>>,
    pub selected: usize,
    pub scroll_offset: usize,
}

impl FileTreeState {
    pub fn new() -> Self {
        Self {
            mode: FileTreeMode::UserOverlays,
            boards: Vec::new(),
            boards_loaded: false,
            selected_board: None,
            board_picker_open: false,
            entries: Vec::new(),
            filtered: None,
            filtered_boards: None,
            selected: 0,
            scroll_offset: 0,
        }
    }

    pub fn set_entries(&mut self, entries: Vec<FileEntry>) {
        self.entries = entries;
        self.filtered = None;
        self.selected = 0;
        self.scroll_offset = 0;
    }

    pub fn cycle_mode(&mut self) {
        self.mode = self.mode.next();
        self.board_picker_open = false;
    }

    pub fn set_mode(&mut self, mode: FileTreeMode) {
        if self.mode != mode {
            self.mode = mode;
            self.board_picker_open = false;
            // Clear entries from previous mode so stale results don't show.
            self.entries.clear();
            self.filtered = None;
            self.selected = 0;
            self.scroll_offset = 0;
        }
    }

    pub fn toggle_board_picker(&mut self) {
        self.board_picker_open = !self.board_picker_open;
    }

    pub fn select_board(&mut self) {
        if !self.boards.is_empty() {
            // Resolve through filter if active.
            let real_idx = match &self.filtered_boards {
                Some(indices) => indices.get(self.selected).copied(),
                None => Some(self.selected.min(self.boards.len().saturating_sub(1))),
            };
            self.selected_board = real_idx;
            self.board_picker_open = false;
            self.filtered_boards = None;
            self.selected = 0;
        }
    }

    pub fn selected_board_name(&self) -> Option<&str> {
        self.selected_board
            .and_then(|i| self.boards.get(i))
            .map(|s| s.as_str())
    }

    pub fn visible_count(&self) -> usize {
        if self.board_picker_open {
            match &self.filtered_boards {
                Some(indices) => indices.len(),
                None => self.boards.len(),
            }
        } else {
            match &self.filtered {
                Some(indices) => indices.len(),
                None => self.entries.len(),
            }
        }
    }

    pub fn move_down(&mut self) {
        let count = self.visible_count();
        if count > 0 && self.selected < count - 1 {
            self.selected += 1;
        }
    }

    pub fn move_up(&mut self) {
        if self.selected > 0 {
            self.selected -= 1;
        }
    }

    pub fn selected_entry(&self) -> Option<&FileEntry> {
        if self.board_picker_open {
            return None;
        }
        let idx = match &self.filtered {
            Some(indices) => indices.get(self.selected).copied()?,
            None => self.selected,
        };
        self.entries.get(idx)
    }

    pub fn apply_filter(&mut self, query: &str) {
        if query.is_empty() {
            self.filtered = None;
            self.filtered_boards = None;
            self.selected = 0;
            return;
        }
        let matcher = SkimMatcherV2::default();
        if self.board_picker_open {
            // Filter the boards list.
            let indices: Vec<usize> = self
                .boards
                .iter()
                .enumerate()
                .filter(|(_, name)| matcher.fuzzy_match(name, query).is_some())
                .map(|(i, _)| i)
                .collect();
            self.filtered_boards = Some(indices);
        } else {
            // Filter the file entries.
            let mut indices: Vec<usize> = self
                .entries
                .iter()
                .enumerate()
                .filter(|(_, e)| matcher.fuzzy_match(&e.name, query).is_some())
                .map(|(i, _)| i)
                .collect();
            indices.sort();
            self.filtered = Some(indices);
        }
        self.selected = 0;
    }

    pub fn clear_filter(&mut self) {
        self.filtered = None;
        self.filtered_boards = None;
    }

    // ------------------------------------------------------------------
    // Rendering
    // ------------------------------------------------------------------

    pub fn render(&self, frame: &mut Frame, area: Rect, is_active: bool) {
        let border_style = if is_active {
            Style::default().fg(Color::Cyan)
        } else {
            Style::default().fg(Color::DarkGray)
        };

        let mode_line = self.mode.label();
        let title = if self.mode == FileTreeMode::BoardFiles {
            let board = self
                .selected_board_name()
                .unwrap_or("(none — press b)");
            format!(" {mode_line} : {board} ")
        } else {
            format!(" {mode_line} ")
        };

        let block = Block::default()
            .title(title)
            .borders(Borders::ALL)
            .border_style(border_style);

        let inner = block.inner(area);
        frame.render_widget(block, area);

        if self.board_picker_open {
            self.render_board_picker(frame, inner);
        } else {
            self.render_file_list(frame, inner);
        }
    }

    fn render_board_picker(&self, frame: &mut Frame, area: Rect) {
        if self.boards.is_empty() {
            let msg = if self.boards_loaded {
                "No boards found. Check workspace path."
            } else {
                "Loading boards..."
            };
            let p = Paragraph::new(msg).style(Style::default().fg(Color::DarkGray));
            frame.render_widget(p, area);
            return;
        }

        let visible_boards: Vec<(usize, &String)> = match &self.filtered_boards {
            Some(indices) => indices.iter().filter_map(|&i| self.boards.get(i).map(|b| (i, b))).collect(),
            None => self.boards.iter().enumerate().collect(),
        };

        if visible_boards.is_empty() {
            let p = Paragraph::new("No matching boards").style(Style::default().fg(Color::DarkGray));
            frame.render_widget(p, area);
            return;
        }

        let height = area.height as usize;
        let max_width = area.width as usize;
        let offset = self.selected.saturating_sub(height.saturating_sub(1));

        let items: Vec<ListItem> = visible_boards
            .iter()
            .enumerate()
            .skip(offset)
            .take(height)
            .map(|(i, (_, name))| {
                let style = if i == self.selected {
                    Style::default().fg(Color::Black).bg(Color::Cyan)
                } else {
                    Style::default()
                };
                ListItem::new(truncate_str(name, max_width)).style(style)
            })
            .collect();

        let list = List::new(items);
        frame.render_widget(list, area);
    }

    fn render_file_list(&self, frame: &mut Frame, area: Rect) {
        let height = area.height as usize;
        let max_width = area.width as usize;
        let offset = self.selected.saturating_sub(height.saturating_sub(1));

        let visible_entries: Vec<&FileEntry> = match &self.filtered {
            Some(indices) => indices.iter().filter_map(|&i| self.entries.get(i)).collect(),
            None => self.entries.iter().collect(),
        };

        if visible_entries.is_empty() {
            let msg = match self.mode {
                FileTreeMode::BoardFiles => "Select a board with 'b'",
                FileTreeMode::UserOverlays => "No overlay files found",
                FileTreeMode::Bindings => "No binding files found",
            };
            let p = Paragraph::new(msg).style(Style::default().fg(Color::DarkGray));
            frame.render_widget(p, area);
            return;
        }

        let items: Vec<ListItem> = visible_entries
            .iter()
            .enumerate()
            .skip(offset)
            .take(height)
            .map(|(i, entry)| {
                let style = if i == self.selected {
                    Style::default().fg(Color::Black).bg(Color::Cyan)
                } else {
                    Style::default()
                };
                let name = truncate_str(&entry.name, max_width);
                ListItem::new(name).style(style)
            })
            .collect();

        let list = List::new(items);
        frame.render_widget(list, area);
    }
}

/// Truncate a string to fit within `max_width` columns, adding "..." if needed.
fn truncate_str(s: &str, max_width: usize) -> String {
    if s.len() <= max_width {
        s.to_string()
    } else if max_width <= 3 {
        s[..max_width].to_string()
    } else {
        format!("{}...", &s[..max_width - 3])
    }
}
