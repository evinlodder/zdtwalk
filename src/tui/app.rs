use std::io::Write;
use std::path::{Path, PathBuf};

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use tokio::sync::mpsc;

use super::log::DebugPanel;
use super::panels::file_tree::{FileTreeMode, FileTreeState};
use super::panels::generator::GeneratorState;
use super::panels::viewer::ViewerState;
use super::workspace::WorkspaceState;
use crate::dts::{self, Binding, DeviceTree};
use crate::west::fetch::HalDtsEntry;
use crate::{tui_log, tui_warn, tui_error};

// ---------------------------------------------------------------------------
// Panel focus
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Panel {
    Left,
    Center,
    Right,
    Debug,
}

impl Panel {
    pub fn next(self, debug_visible: bool) -> Self {
        match self {
            Panel::Left => Panel::Center,
            Panel::Center => Panel::Right,
            Panel::Right => {
                if debug_visible { Panel::Debug } else { Panel::Left }
            }
            Panel::Debug => Panel::Left,
        }
    }

    pub fn prev(self, debug_visible: bool) -> Self {
        match self {
            Panel::Left => {
                if debug_visible { Panel::Debug } else { Panel::Right }
            }
            Panel::Center => Panel::Left,
            Panel::Right => Panel::Center,
            Panel::Debug => Panel::Right,
        }
    }
}

// ---------------------------------------------------------------------------
// Search state
// ---------------------------------------------------------------------------

#[derive(Debug, Default)]
pub struct SearchState {
    pub active: bool,
    pub query: String,
}

// ---------------------------------------------------------------------------
// Messages
// ---------------------------------------------------------------------------

#[derive(Debug)]
pub enum Message {
    Key(KeyEvent),
    Tick,
    Resize(u16, u16),
    WorkspaceReady(WorkspaceState),
    BoardsLoaded(Vec<String>),
    HalFetched(Vec<HalDtsEntry>),
    FileTreeLoaded(FileTreeMode, Vec<super::workspace::FileEntry>),
    FileParsed(PathBuf, DeviceTree),
    BindingParsed(PathBuf, Binding),
    FileContent(PathBuf, String),
    StatusUpdate(String),
    Error(String),
}

// ---------------------------------------------------------------------------
// App
// ---------------------------------------------------------------------------

pub struct App {
    pub should_quit: bool,
    pub active_panel: Panel,
    pub workspace: Option<WorkspaceState>,
    pub search: SearchState,
    pub left: FileTreeState,
    pub center: ViewerState,
    pub right: GeneratorState,
    pub status_message: Option<String>,
    /// Left panel width as a percentage (10..=50).
    pub left_width_pct: u16,
    /// Debug log panel.
    pub debug: DebugPanel,
    /// Show the keybind help overlay.
    pub show_help: bool,

    /// Channel for internal async messages (workspace discovery, file parsing, etc.)
    internal_tx: mpsc::Sender<Message>,
    internal_rx: mpsc::Receiver<Message>,
}

impl App {
    pub fn new() -> Self {
        let (internal_tx, internal_rx) = mpsc::channel(64);
        Self {
            should_quit: false,
            active_panel: Panel::Left,
            workspace: None,
            search: SearchState::default(),
            left: FileTreeState::new(),
            center: ViewerState::new(),
            right: GeneratorState::new(),
            status_message: None,
            left_width_pct: 25,
            debug: DebugPanel::new(),
            show_help: false,
            internal_tx,
            internal_rx,
        }
    }

    /// Get a sender handle for posting internal messages from background tasks.
    pub fn message_tx(&self) -> mpsc::Sender<Message> {
        self.internal_tx.clone()
    }

    /// Receive the next internal message (non-blocking, for use in select!).
    pub async fn recv_message(&mut self) -> Option<Message> {
        self.internal_rx.recv().await
    }

    /// Central update dispatch.
    pub async fn update(&mut self, msg: Message) {
        match msg {
            Message::Key(key) => self.handle_key(key),
            Message::Tick => {}
            Message::Resize(_, _) => {}
            Message::WorkspaceReady(ws) => {
                self.status_message = Some(format!(
                    "Workspace: {} | Zephyr: {}",
                    ws.info.workspace_root.display(),
                    ws.info.zephyr_dir.display(),
                ));
                self.workspace = Some(ws.clone());
                // Trigger initial file tree scan.
                self.trigger_file_scan(ws.clone());
                // Kick off HAL module fetching in the background.
                self.trigger_hal_fetch(ws);
            }
            Message::BoardsLoaded(boards) => {
                self.left.boards_loaded = true;
                self.left.boards = boards;
            }
            Message::HalFetched(entries) => {
                let count = entries.iter().filter(|e| e.dts_path.is_some()).count();
                if let Some(ws) = &mut self.workspace {
                    ws.hal_entries = entries;
                }
                self.status_message = Some(format!("HAL modules ready ({count} with DTS)"));
            }
            Message::FileTreeLoaded(mode, entries) => {
                // Only apply if still in the same mode that requested this scan.
                if self.left.mode == mode {
                    self.left.set_entries(entries);
                    if self.search.active {
                        self.left.apply_filter(&self.search.query);
                    }
                }
            }
            Message::FileParsed(path, tree) => {
                self.center.set_parsed_dts(path, tree);
            }
            Message::BindingParsed(path, binding) => {
                self.center.set_parsed_binding(path, binding);
            }
            Message::FileContent(path, content) => {
                self.center.set_raw_content(path, content);
            }
            Message::StatusUpdate(msg) => {
                self.status_message = Some(msg);
            }
            Message::Error(e) => {
                tui_error!("{e}");
                self.status_message = Some(format!("Error: {e}"));
            }
        }
    }

    // ------------------------------------------------------------------
    // Key handling
    // ------------------------------------------------------------------

    fn handle_key(&mut self, key: KeyEvent) {
        // Ctrl-c always quits.
        if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c') {
            self.should_quit = true;
            return;
        }

        // Help overlay: any key dismisses it.
        if self.show_help {
            self.show_help = false;
            return;
        }

        // Search mode captures all printable keys.
        if self.search.active {
            match key.code {
                KeyCode::Esc => {
                    self.search.active = false;
                    self.search.query.clear();
                    self.left.clear_filter();
                }
                KeyCode::Enter => {
                    self.search.active = false;
                }
                KeyCode::Backspace => {
                    self.search.query.pop();
                    self.left.apply_filter(&self.search.query);
                }
                KeyCode::Char(c) => {
                    self.search.query.push(c);
                    self.left.apply_filter(&self.search.query);
                }
                _ => {}
            }
            return;
        }

        // Global keys.
        match key.code {
            KeyCode::Char('q') => {
                if self.active_panel == Panel::Debug {
                    self.debug.toggle();
                    self.active_panel = Panel::Center;
                    return;
                }
                self.should_quit = true;
                return;
            }
            KeyCode::Char('d') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.debug.toggle();
                if self.debug.visible {
                    self.active_panel = Panel::Debug;
                } else if self.active_panel == Panel::Debug {
                    self.active_panel = Panel::Center;
                }
                return;
            }
            KeyCode::Char('?') => {
                self.show_help = true;
                return;
            }
            KeyCode::Char('/') if self.active_panel == Panel::Left => {
                self.search.active = true;
                self.search.query.clear();
                return;
            }
            KeyCode::Tab => {
                self.active_panel = self.active_panel.next(self.debug.visible);
                return;
            }
            KeyCode::BackTab => {
                self.active_panel = self.active_panel.prev(self.debug.visible);
                return;
            }
            KeyCode::Char('g') => {
                self.right.toggle_collapsed();
                return;
            }
            KeyCode::Char('[') => {
                self.left_width_pct = self.left_width_pct.saturating_sub(3).max(10);
                return;
            }
            KeyCode::Char(']') => {
                self.left_width_pct = (self.left_width_pct + 3).min(50);
                return;
            }
            _ => {}
        }

        // Dispatch to active panel.
        match self.active_panel {
            Panel::Left => self.handle_left_key(key),
            Panel::Center => self.handle_center_key(key),
            Panel::Right => {} // stub — no keys yet
            Panel::Debug => self.handle_debug_key(key),
        }
    }

    fn handle_left_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Char('j') | KeyCode::Down => self.left.move_down(),
            KeyCode::Char('k') | KeyCode::Up => self.left.move_up(),
            KeyCode::Char('m') => {
                self.left.cycle_mode();
                // Auto-load boards list when cycling into board mode.
                if self.left.mode == FileTreeMode::BoardFiles && self.left.boards.is_empty() {
                    if let Some(ws) = &self.workspace {
                        self.trigger_board_scan(ws.clone());
                    }
                }
                if let Some(ws) = &self.workspace {
                    self.trigger_file_scan(ws.clone());
                }
            }
            KeyCode::Char('1') => {
                self.left.set_mode(FileTreeMode::BoardFiles);
                // Auto-load boards list when entering board mode.
                if self.left.boards.is_empty() {
                    if let Some(ws) = &self.workspace {
                        self.trigger_board_scan(ws.clone());
                    }
                }
                if let Some(ws) = &self.workspace {
                    self.trigger_file_scan(ws.clone());
                }
            }
            KeyCode::Char('2') => {
                self.left.set_mode(FileTreeMode::UserOverlays);
                if let Some(ws) = &self.workspace {
                    self.trigger_file_scan(ws.clone());
                }
            }
            KeyCode::Char('3') => {
                self.left.set_mode(FileTreeMode::Bindings);
                if let Some(ws) = &self.workspace {
                    self.trigger_file_scan(ws.clone());
                }
            }
            KeyCode::Char('b') => {
                if self.left.mode == FileTreeMode::BoardFiles {
                    self.left.toggle_board_picker();
                    if self.left.board_picker_open {
                        // Load boards if not yet done.
                        if self.left.boards.is_empty() {
                            if let Some(ws) = &self.workspace {
                                self.trigger_board_scan(ws.clone());
                            }
                        }
                    }
                }
            }
            KeyCode::Enter => {
                if self.left.board_picker_open {
                    self.left.select_board();
                    if let Some(ws) = &self.workspace {
                        self.trigger_file_scan(ws.clone());
                    }
                } else if let Some(entry) = self.left.selected_entry() {
                    let path = entry.path.clone();
                    let tx = self.internal_tx.clone();
                    tokio::spawn(async move {
                        Self::load_file(path, tx).await;
                    });
                }
            }
            _ => {}
        }
    }

    fn handle_center_key(&mut self, key: KeyEvent) {
        // If search input mode is active, handle that first.
        if self.center.search_active {
            match key.code {
                KeyCode::Esc => self.center.search_cancel(),
                KeyCode::Enter => self.center.search_commit(),
                KeyCode::Backspace => self.center.search_pop(),
                KeyCode::Char(c) => self.center.search_push(c),
                _ => {}
            }
            return;
        }

        match key.code {
            KeyCode::Char('j') | KeyCode::Down => self.center.scroll_down(),
            KeyCode::Char('k') | KeyCode::Up => self.center.scroll_up(),
            KeyCode::Char('v') => self.center.toggle_mode(),
            KeyCode::Char('V') => self.center.toggle_visual(),
            KeyCode::Char('y') => {
                if let Some(text) = self.center.yank_selection() {
                    let line_count = text.lines().count();
                    // Copy to system clipboard via OSC 52 escape sequence.
                    Self::copy_to_clipboard(&text);
                    self.status_message = Some(format!("Yanked {line_count} lines to clipboard"));
                }
            }
            KeyCode::Char('/') => {
                self.center.start_search();
            }
            KeyCode::Char('n') => self.center.search_next(),
            KeyCode::Char('N') => self.center.search_prev(),
            KeyCode::Esc => {
                // Exit visual mode if active, clear search, otherwise do nothing.
                if self.center.in_visual_mode() {
                    self.center.toggle_visual();
                } else if self.center.search_query.is_some() {
                    self.center.search_query = None;
                    self.center.search_cancel();
                }
            }
            KeyCode::Enter | KeyCode::Char(' ') => {
                // If the selected line is an include, navigate to that file.
                if let Some(inc) = self.center.selected_include().map(|s| s.to_string()) {
                    self.open_include(&inc);
                } else {
                    self.center.toggle_expand();
                }
            }
            KeyCode::Char('h') | KeyCode::Left => self.center.collapse_current(),
            KeyCode::Char('l') | KeyCode::Right => self.center.expand_current(),
            KeyCode::Char('}') => self.center.next_tab(),
            KeyCode::Char('{') => self.center.prev_tab(),
            KeyCode::Char('w') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.center.close_tab();
            }
            _ => {}
        }
    }

    fn handle_debug_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Char('j') | KeyCode::Down => self.debug.scroll_down(),
            KeyCode::Char('k') | KeyCode::Up => self.debug.scroll_up(),
            KeyCode::Char('G') => {
                self.debug.follow = true;
            }
            KeyCode::Char('g') => {
                self.debug.scroll = 0;
                self.debug.follow = false;
            }
            _ => {}
        }
    }

    fn open_include(&mut self, include_name: &str) {
        let ws = match &self.workspace {
            Some(ws) => ws,
            None => return,
        };

        // Determine the origin directory (directory of current file).
        let origin = self
            .center
            .current_file
            .as_ref()
            .and_then(|p| p.parent())
            .unwrap_or_else(|| Path::new("."))
            .to_path_buf();

        if let Some(resolved) = super::workspace::resolve_include(ws, &origin, include_name) {
            let tx = self.internal_tx.clone();
            tokio::spawn(async move {
                Self::load_file(resolved, tx).await;
            });
        } else {
            self.status_message = Some(format!("Include not found: {include_name}"));
        }
    }

    // ------------------------------------------------------------------
    // Async triggers
    // ------------------------------------------------------------------

    fn trigger_file_scan(&self, ws: WorkspaceState) {
        let tx = self.internal_tx.clone();
        let mode = self.left.mode;
        let board = self.left.selected_board_name().map(|s| s.to_string());

        tokio::spawn(async move {
            let entries = match mode {
                FileTreeMode::BoardFiles => {
                    if let Some(board) = board {
                        super::workspace::scan_board_files(&ws, &board).await
                    } else {
                        vec![]
                    }
                }
                FileTreeMode::UserOverlays => {
                    super::workspace::scan_user_overlays(&ws.info.workspace_root).await
                }
                FileTreeMode::Bindings => {
                    super::workspace::scan_bindings(&ws.info.zephyr_dir).await
                }
            };
            let _ = tx.send(Message::FileTreeLoaded(mode, entries)).await;
        });
    }

    fn trigger_board_scan(&self, ws: WorkspaceState) {
        let tx = self.internal_tx.clone();
        tokio::spawn(async move {
            let boards = super::workspace::list_boards(&ws.info.zephyr_dir).await;
            let _ = tx.send(Message::BoardsLoaded(boards)).await;
        });
    }

    fn trigger_hal_fetch(&self, ws: WorkspaceState) {
        let tx = self.internal_tx.clone();
        tokio::spawn(async move {
            // Create a channel for progress updates.
            let (ptx, mut prx) = tokio::sync::mpsc::channel::<String>(16);

            // Spawn a task to forward progress messages as status updates.
            let stx = tx.clone();
            tokio::spawn(async move {
                while let Some(msg) = prx.recv().await {
                    let _ = stx.send(Message::StatusUpdate(msg)).await;
                }
            });

            let entries = super::workspace::fetch_hal_modules(ws, ptx).await;
            let _ = tx.send(Message::HalFetched(entries)).await;
        });
    }

    fn copy_to_clipboard(text: &str) {
        const B64: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
        let bytes = text.as_bytes();
        let mut encoded = Vec::with_capacity((bytes.len() + 2) / 3 * 4);
        for chunk in bytes.chunks(3) {
            let b0 = chunk[0] as u32;
            let b1 = if chunk.len() > 1 { chunk[1] as u32 } else { 0 };
            let b2 = if chunk.len() > 2 { chunk[2] as u32 } else { 0 };
            let n = (b0 << 16) | (b1 << 8) | b2;
            encoded.push(B64[((n >> 18) & 0x3F) as usize]);
            encoded.push(B64[((n >> 12) & 0x3F) as usize]);
            if chunk.len() > 1 {
                encoded.push(B64[((n >> 6) & 0x3F) as usize]);
            } else {
                encoded.push(b'=');
            }
            if chunk.len() > 2 {
                encoded.push(B64[(n & 0x3F) as usize]);
            } else {
                encoded.push(b'=');
            }
        }
        let b64 = String::from_utf8(encoded).unwrap_or_default();
        // OSC 52: \x1b]52;c;<base64>\x07
        let _ = std::io::stdout().write_all(format!("\x1b]52;c;{b64}\x07").as_bytes());
        let _ = std::io::stdout().flush();
    }

    async fn load_file(path: PathBuf, tx: mpsc::Sender<Message>) {
        tui_log!("Loading file: {}", path.display());
        let p = path.clone();
        let result = tokio::task::spawn_blocking(move || std::fs::read_to_string(&p)).await;

        let content = match result {
            Ok(Ok(c)) => c,
            Ok(Err(e)) => {
                let _ = tx.send(Message::Error(format!("{}: {e}", path.display()))).await;
                return;
            }
            Err(e) => {
                let _ = tx.send(Message::Error(format!("task join: {e}"))).await;
                return;
            }
        };

        // Send raw content first.
        let _ = tx
            .send(Message::FileContent(path.clone(), content.clone()))
            .await;

        // Try to parse based on extension.
        let ext = path
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("")
            .to_lowercase();

        match ext.as_str() {
            "yaml" | "yml" => {
                match dts::deserialize_binding(&content) {
                    Ok(binding) => {
                        let _ = tx.send(Message::BindingParsed(path, binding)).await;
                    }
                    Err(e) => {
                        let _ = tx
                            .send(Message::Error(format!("binding parse: {e}")))
                            .await;
                    }
                }
            }
            "dts" | "dtsi" | "overlay" | "dtso" => {
                match dts::parse_dts(&content) {
                    Ok(tree) => {
                        let _ = tx.send(Message::FileParsed(path, tree)).await;
                    }
                    Err(e) => {
                        let _ = tx.send(Message::Error(format!(
                            "parse {}: {e}",
                            path.file_name().and_then(|n| n.to_str()).unwrap_or("?")
                        ))).await;
                    }
                }
            }
            _ => {}
        }
    }
}
