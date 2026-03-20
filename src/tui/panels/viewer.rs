use std::collections::HashSet;
use std::path::PathBuf;

use ratatui::{
    prelude::*,
    widgets::Paragraph,
};

use crate::dts::{self, Binding, DeviceTree, Reference};
use crate::tui::theme;

use super::super::widgets::status_dot::StatusColor;

// ---------------------------------------------------------------------------
// Viewer modes
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ViewerMode {
    Raw,
    Simplified,
}

// ---------------------------------------------------------------------------
// What's currently being viewed
// ---------------------------------------------------------------------------

enum ViewContent {
    None,
    Dts {
        tree: DeviceTree,
    },
    Binding {
        binding: Binding,
    },
}

// ---------------------------------------------------------------------------
// Simplified-view line types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
struct SimplifiedLine {
    depth: usize,
    kind: LineKind,
    path_key: String,
}

#[derive(Debug, Clone)]
enum LineKind {
    Include(String),
    NodeHeader {
        name: String,
        status: StatusColor,
        has_children: bool,
    },
    Property {
        name: String,
        value: String,
    },
    BindingHeader {
        compatible: String,
        description: String,
    },
    BindingProperty {
        name: String,
        prop_type: String,
        required: bool,
        description: String,
    },
}

// ---------------------------------------------------------------------------
// Tab – per-file state
// ---------------------------------------------------------------------------

struct Tab {
    mode: ViewerMode,
    file: PathBuf,
    raw_content: Option<String>,
    content: ViewContent,
    scroll: usize,
    selected_line: usize,
    simplified_scroll: usize,
    expanded: HashSet<String>,
    simplified_lines: Vec<SimplifiedLine>,
    /// Visual selection anchor (line where selection started). `None` when not
    /// in visual mode.
    selection_anchor: Option<usize>,
}

impl Tab {
    fn new(path: PathBuf) -> Self {
        Self {
            mode: ViewerMode::Simplified,
            file: path,
            raw_content: None,
            content: ViewContent::None,
            scroll: 0,
            selected_line: 0,
            simplified_scroll: 0,
            expanded: HashSet::new(),
            simplified_lines: Vec::new(),
            selection_anchor: None,
        }
    }

    fn file_name(&self) -> &str {
        self.file
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("?")
    }

    fn total_lines(&self) -> usize {
        match self.mode {
            ViewerMode::Raw => self
                .raw_content
                .as_ref()
                .map(|c| c.lines().count())
                .unwrap_or(0),
            ViewerMode::Simplified => self.simplified_lines.len(),
        }
    }
}

use fuzzy_matcher::FuzzyMatcher;
use fuzzy_matcher::skim::SkimMatcherV2;

// ---------------------------------------------------------------------------
// ViewerState – manages tabs
// ---------------------------------------------------------------------------

pub struct ViewerState {
    tabs: Vec<Tab>,
    active: usize,
    /// Kept in sync with the active tab's file for external readers.
    pub current_file: Option<PathBuf>,
    /// When true, the viewer is in search-input mode (typing a query).
    pub search_active: bool,
    /// The current search input buffer.
    pub search_input: String,
    /// The committed search query (after pressing Enter).
    pub search_query: Option<String>,
    /// Line indices that match the current search.
    search_matches: Vec<usize>,
    /// Index into search_matches for current match navigation.
    search_match_idx: usize,
}

impl ViewerState {
    pub fn new() -> Self {
        Self {
            tabs: Vec::new(),
            active: 0,
            current_file: None,
            search_active: false,
            search_input: String::new(),
            search_query: None,
            search_matches: Vec::new(),
            search_match_idx: 0,
        }
    }

    // ------------------------------------------------------------------
    // Tab management
    // ------------------------------------------------------------------

    /// Find or create a tab for `path`.  Returns the tab index.
    fn ensure_tab(&mut self, path: &PathBuf) -> usize {
        if let Some(idx) = self.tabs.iter().position(|t| &t.file == path) {
            idx
        } else {
            self.tabs.push(Tab::new(path.clone()));
            self.tabs.len() - 1
        }
    }

    fn sync_current_file(&mut self) {
        self.current_file = self.tabs.get(self.active).map(|t| t.file.clone());
    }

    pub fn next_tab(&mut self) {
        if !self.tabs.is_empty() {
            self.active = (self.active + 1) % self.tabs.len();
            self.sync_current_file();
        }
    }

    pub fn prev_tab(&mut self) {
        if !self.tabs.is_empty() {
            self.active = if self.active == 0 {
                self.tabs.len() - 1
            } else {
                self.active - 1
            };
            self.sync_current_file();
        }
    }

    pub fn close_tab(&mut self) {
        if self.tabs.is_empty() {
            return;
        }
        self.tabs.remove(self.active);
        if self.tabs.is_empty() {
            self.active = 0;
        } else if self.active >= self.tabs.len() {
            self.active = self.tabs.len() - 1;
        }
        self.sync_current_file();
    }

    // ------------------------------------------------------------------
    // Content setters (find-or-create tab, then set content)
    // ------------------------------------------------------------------

    pub fn set_raw_content(&mut self, path: PathBuf, content: String) {
        let idx = self.ensure_tab(&path);
        self.active = idx;
        let tab = &mut self.tabs[idx];
        tab.raw_content = Some(content);
        tab.content = ViewContent::None;
        tab.simplified_lines.clear();
        tab.expanded.clear();
        tab.scroll = 0;
        tab.selected_line = 0;
        tab.simplified_scroll = 0;
        self.sync_current_file();
    }

    pub fn set_parsed_dts(&mut self, path: PathBuf, tree: DeviceTree) {
        let idx = self.ensure_tab(&path);
        self.active = idx;
        let tab = &mut self.tabs[idx];
        tab.content = ViewContent::Dts { tree };
        tab.expanded.clear();
        tab.expanded.insert("/".to_string());
        tab.expanded.insert("//soc".to_string());
        rebuild_simplified_lines(tab);
        tab.selected_line = 0;
        tab.simplified_scroll = 0;
        self.sync_current_file();
    }

    pub fn set_parsed_binding(&mut self, path: PathBuf, binding: Binding) {
        let idx = self.ensure_tab(&path);
        self.active = idx;
        let tab = &mut self.tabs[idx];
        tab.content = ViewContent::Binding { binding };
        tab.expanded.clear();
        rebuild_simplified_lines(tab);
        tab.selected_line = 0;
        tab.simplified_scroll = 0;
        self.sync_current_file();
    }

    // ------------------------------------------------------------------
    // Delegate to active tab
    // ------------------------------------------------------------------

    fn active_tab(&self) -> Option<&Tab> {
        self.tabs.get(self.active)
    }

    fn active_tab_mut(&mut self) -> Option<&mut Tab> {
        self.tabs.get_mut(self.active)
    }

    pub fn toggle_mode(&mut self) {
        if let Some(tab) = self.active_tab_mut() {
            tab.mode = match tab.mode {
                ViewerMode::Raw => ViewerMode::Simplified,
                ViewerMode::Simplified => ViewerMode::Raw,
            };
            tab.scroll = 0;
            tab.selected_line = 0;
        }
        // Recompute search matches for the new mode.
        self.update_search_matches();
    }

    pub fn scroll_down(&mut self) {
        if let Some(tab) = self.active_tab_mut() {
            let max = tab.total_lines().saturating_sub(1);
            if tab.selected_line < max {
                tab.selected_line += 1;
            }
        }
    }

    pub fn scroll_up(&mut self) {
        if let Some(tab) = self.active_tab_mut() {
            tab.selected_line = tab.selected_line.saturating_sub(1);
        }
    }

    pub fn toggle_expand(&mut self) {
        if let Some(tab) = self.active_tab_mut() {
            if tab.mode != ViewerMode::Simplified {
                return;
            }
            if let Some(line) = tab.simplified_lines.get(tab.selected_line) {
                let key = line.path_key.clone();
                if tab.expanded.contains(&key) {
                    tab.expanded.remove(&key);
                } else {
                    tab.expanded.insert(key);
                }
                rebuild_simplified_lines(tab);
            }
        }
    }

    pub fn expand_current(&mut self) {
        if let Some(tab) = self.active_tab_mut() {
            if let Some(line) = tab.simplified_lines.get(tab.selected_line) {
                let key = line.path_key.clone();
                if !tab.expanded.contains(&key) {
                    tab.expanded.insert(key);
                    rebuild_simplified_lines(tab);
                }
            }
        }
    }

    pub fn collapse_current(&mut self) {
        if let Some(tab) = self.active_tab_mut() {
            if let Some(line) = tab.simplified_lines.get(tab.selected_line) {
                let key = line.path_key.clone();
                if tab.expanded.contains(&key) {
                    tab.expanded.remove(&key);
                    rebuild_simplified_lines(tab);
                }
            }
        }
    }

    pub fn selected_include(&self) -> Option<&str> {
        let tab = self.active_tab()?;
        if tab.mode == ViewerMode::Simplified {
            match tab.simplified_lines.get(tab.selected_line) {
                Some(SimplifiedLine {
                    kind: LineKind::Include(path),
                    ..
                }) => return Some(path.as_str()),
                _ => return None,
            }
        }
        // Raw mode: parse #include from the line under the cursor.
        if let Some(raw) = &tab.raw_content {
            if let Some(line) = raw.lines().nth(tab.selected_line) {
                let trimmed = line.trim();
                if let Some(rest) = trimmed.strip_prefix("#include") {
                    let rest = rest.trim();
                    if (rest.starts_with('<') && rest.ends_with('>'))
                        || (rest.starts_with('"') && rest.ends_with('"'))
                    {
                        return Some(&rest[1..rest.len() - 1]);
                    }
                }
                if let Some(rest) = trimmed.strip_prefix("/include/") {
                    let rest = rest.trim().trim_end_matches(';');
                    let rest = rest.trim();
                    if rest.starts_with('"') && rest.ends_with('"') {
                        return Some(&rest[1..rest.len() - 1]);
                    }
                }
            }
        }
        None
    }

    /// Return the DTS reference and labels for the node under the cursor.
    ///
    /// Works only in Simplified mode when the cursor is on a NodeHeader line.
    /// Returns `(Reference, Vec<String>)` where the Reference is a path and
    /// labels come from the underlying tree node.
    pub fn node_at_cursor(&self) -> Option<(Reference, Vec<String>)> {
        let tab = self.active_tab()?;
        if tab.mode != ViewerMode::Simplified {
            return None;
        }
        let sline = tab.simplified_lines.get(tab.selected_line)?;

        // Only NodeHeader lines map to actual nodes.
        let path_key = match &sline.kind {
            LineKind::NodeHeader { .. } => &sline.path_key,
            LineKind::Property { .. } => {
                // Walk up to the parent node header.
                // The path_key for a property is `<node_path>/<prop_name>`.
                let parent_key = sline.path_key.rsplit_once('/')?.0;
                // Find the node header with this path key.
                return self.find_node_in_tree(tab, parent_key);
            }
            _ => return None,
        };

        self.find_node_in_tree(tab, path_key)
    }

    fn find_node_in_tree(&self, tab: &Tab, path_key: &str) -> Option<(Reference, Vec<String>)> {
        let tree = match &tab.content {
            ViewContent::Dts { tree } => tree,
            _ => return None,
        };

        // The path_key is either "/" for root, "/soc/i2c@..." for nested nodes,
        // or "&label" for reference node overrides.
        if path_key.starts_with('&') {
            // It's a reference node override.
            let label = path_key.strip_prefix('&')?;
            return Some((Reference::Label(label.to_string()), vec![label.to_string()]));
        }

        // Walk the tree to find the node at this path.
        if let Some(root) = &tree.root {
            if path_key == "/" {
                let labels = root.labels.clone();
                return Some((Reference::Path("/".to_string()), labels));
            }

            if let Some(node) = find_node_by_path(root, path_key, "/") {
                let labels = node.labels.clone();
                if !labels.is_empty() {
                    return Some((Reference::Label(labels[0].clone()), labels));
                }
                return Some((Reference::Path(path_key.to_string()), labels));
            }
        }

        // Check reference nodes.
        for rn in &tree.reference_nodes {
            let ref_str = format!("{}", rn.reference);
            if ref_str == path_key {
                let labels = rn.node.labels.clone();
                return Some((rn.reference.clone(), labels));
            }
        }

        None
    }
    pub fn toggle_visual(&mut self) {
        if let Some(tab) = self.active_tab_mut() {
            if tab.selection_anchor.is_some() {
                tab.selection_anchor = None; // exit visual mode
            } else {
                tab.selection_anchor = Some(tab.selected_line);
            }
        }
    }

    /// Return true if visual selection mode is active.
    pub fn in_visual_mode(&self) -> bool {
        self.active_tab()
            .map(|t| t.selection_anchor.is_some())
            .unwrap_or(false)
    }

    /// Copy the selected lines (visual selection) to a string.
    /// Also exits visual mode.
    pub fn yank_selection(&mut self) -> Option<String> {
        let tab = self.active_tab_mut()?;
        let anchor = tab.selection_anchor.take()?;
        let start = anchor.min(tab.selected_line);
        let end = anchor.max(tab.selected_line);

        match tab.mode {
            ViewerMode::Raw => {
                let raw = tab.raw_content.as_ref()?;
                let lines: Vec<&str> = raw
                    .lines()
                    .enumerate()
                    .filter(|(i, _)| *i >= start && *i <= end)
                    .map(|(_, l)| l)
                    .collect();
                Some(lines.join("\n"))
            }
            ViewerMode::Simplified => {
                let lines: Vec<String> = tab
                    .simplified_lines
                    .iter()
                    .enumerate()
                    .filter(|(i, _)| *i >= start && *i <= end)
                    .map(|(_, sl)| simplified_line_text(sl))
                    .collect();
                Some(lines.join("\n"))
            }
        }
    }

    // ------------------------------------------------------------------
    // Search
    // ------------------------------------------------------------------

    /// Start search input mode.
    pub fn start_search(&mut self) {
        self.search_active = true;
        self.search_input.clear();
    }

    /// Handle a character input while in search mode.
    pub fn search_push(&mut self, ch: char) {
        self.search_input.push(ch);
        self.update_search_matches();
    }

    /// Handle backspace while in search mode.
    pub fn search_pop(&mut self) {
        self.search_input.pop();
        self.update_search_matches();
    }

    /// Commit the search (Enter). Locks the matches and exits input mode.
    pub fn search_commit(&mut self) {
        self.search_active = false;
        if self.search_input.is_empty() {
            self.search_query = None;
            self.search_matches.clear();
        } else {
            self.search_query = Some(self.search_input.clone());
            self.update_search_matches();
            // Jump to first match.
            if let Some(&line) = self.search_matches.first() {
                self.search_match_idx = 0;
                if let Some(tab) = self.active_tab_mut() {
                    tab.selected_line = line;
                }
            }
        }
    }

    /// Cancel search (Esc while in search input mode).
    pub fn search_cancel(&mut self) {
        self.search_active = false;
        self.search_input.clear();
        self.search_query = None;
        self.search_matches.clear();
    }

    /// Jump to next search match.
    pub fn search_next(&mut self) {
        if self.search_matches.is_empty() {
            return;
        }
        self.search_match_idx = (self.search_match_idx + 1) % self.search_matches.len();
        let line = self.search_matches[self.search_match_idx];
        if let Some(tab) = self.active_tab_mut() {
            tab.selected_line = line;
        }
    }

    /// Jump to previous search match.
    pub fn search_prev(&mut self) {
        if self.search_matches.is_empty() {
            return;
        }
        self.search_match_idx = if self.search_match_idx == 0 {
            self.search_matches.len() - 1
        } else {
            self.search_match_idx - 1
        };
        let line = self.search_matches[self.search_match_idx];
        if let Some(tab) = self.active_tab_mut() {
            tab.selected_line = line;
        }
    }

    /// Recompute which lines match the current search input.
    fn update_search_matches(&mut self) {
        self.search_matches.clear();
        let query = if self.search_active {
            &self.search_input
        } else if let Some(q) = &self.search_query {
            q
        } else {
            return;
        };
        if query.is_empty() {
            return;
        }

        let matcher = SkimMatcherV2::default();
        let tab = match self.tabs.get(self.active) {
            Some(t) => t,
            None => return,
        };

        match tab.mode {
            ViewerMode::Raw => {
                if let Some(content) = &tab.raw_content {
                    for (i, line) in content.lines().enumerate() {
                        if matcher.fuzzy_match(line, query).is_some() {
                            self.search_matches.push(i);
                        }
                    }
                }
            }
            ViewerMode::Simplified => {
                for (i, sline) in tab.simplified_lines.iter().enumerate() {
                    let text = simplified_line_text(sline);
                    if matcher.fuzzy_match(&text, query).is_some() {
                        self.search_matches.push(i);
                    }
                }
            }
        }
    }

    /// Check whether a given line index is a search match (for rendering).
    #[allow(dead_code)]
    fn is_search_match(&self, line_idx: usize) -> bool {
        self.search_matches.contains(&line_idx)
    }

    // ------------------------------------------------------------------
    // Rendering
    // ------------------------------------------------------------------

    pub fn render(&mut self, frame: &mut Frame, area: Rect, is_active: bool) {
        let block = theme::panel_block("", is_active);

        let inner = block.inner(area);
        frame.render_widget(block, area);

        if self.tabs.is_empty() {
            let msg = Paragraph::new("Open a file from the left panel")
                .style(theme::muted());
            frame.render_widget(msg, inner);
            return;
        }

        // Split inner area: tab bar (1 line) + content [+ search bar (1 line)].
        let show_search_bar = self.search_active || self.search_query.is_some();
        let constraints = if show_search_bar {
            vec![
                Constraint::Length(1),
                Constraint::Min(0),
                Constraint::Length(1),
            ]
        } else {
            vec![Constraint::Length(1), Constraint::Min(0)]
        };
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints(constraints)
            .split(inner);

        self.render_tab_bar(frame, chunks[0]);
        self.render_content(frame, chunks[1]);

        if show_search_bar {
            self.render_search_bar(frame, chunks[2]);
        }
    }

    fn render_tab_bar(&self, frame: &mut Frame, area: Rect) {
        let mut spans: Vec<Span> = Vec::new();
        for (i, tab) in self.tabs.iter().enumerate() {
            let name = tab.file_name();
            if i == self.active {
                spans.push(Span::styled(
                    format!(" {name} "),
                    Style::default()
                        .fg(theme::CURSOR_FG)
                        .bg(theme::GOLD)
                        .add_modifier(Modifier::BOLD),
                ));
            } else {
                spans.push(Span::styled(
                    format!(" {name} "),
                    Style::default().fg(theme::TEXT_MUTED),
                ));
            }
            if i < self.tabs.len() - 1 {
                spans.push(Span::styled(" ", Style::default().fg(theme::BORDER_INACTIVE)));
            }
        }
        let line = Line::from(spans);
        frame.render_widget(Paragraph::new(vec![line]), area);
    }

    fn render_content(&mut self, frame: &mut Frame, area: Rect) {
        let matches = &self.search_matches;
        let Some(tab) = self.tabs.get_mut(self.active) else {
            return;
        };

        match tab.mode {
            ViewerMode::Raw => render_raw(tab, frame, area, matches),
            ViewerMode::Simplified => {
                let h = area.height as usize;
                if tab.selected_line < tab.simplified_scroll {
                    tab.simplified_scroll = tab.selected_line;
                } else if tab.selected_line >= tab.simplified_scroll + h {
                    tab.simplified_scroll =
                        tab.selected_line.saturating_sub(h.saturating_sub(1));
                }
                render_simplified(tab, frame, area, matches);
            }
        }
    }

    fn render_search_bar(&self, frame: &mut Frame, area: Rect) {
        let match_count = self.search_matches.len();
        let query = if self.search_active {
            &self.search_input
        } else {
            self.search_query.as_deref().unwrap_or("")
        };
        let match_info = if match_count > 0 {
            format!(" [{}/{match_count}]", self.search_match_idx + 1)
        } else if !query.is_empty() {
            " [no match]".to_string()
        } else {
            String::new()
        };
        let line = Line::from(vec![
            Span::styled("/", Style::default().fg(theme::AMBER)),
            Span::styled(query.to_string(), Style::default().fg(theme::TEXT)),
            Span::styled(match_info, Style::default().fg(theme::TEXT_DIM)),
        ]);
        frame.render_widget(Paragraph::new(vec![line]), area);
    }
}

// ---------------------------------------------------------------------------
// Free functions operating on a Tab (avoids &self/&mut self borrow issues)
// ---------------------------------------------------------------------------

fn rebuild_simplified_lines(tab: &mut Tab) {
    tab.simplified_lines.clear();
    match &tab.content {
        ViewContent::None => {}
        ViewContent::Dts { tree } => {
            let tree = tree.clone();
            build_dts_lines(tab, &tree);
        }
        ViewContent::Binding { binding } => {
            let binding = binding.clone();
            build_binding_lines(tab, &binding);
        }
    }
}

fn build_dts_lines(tab: &mut Tab, tree: &DeviceTree) {
    for inc in &tree.includes {
        tab.simplified_lines.push(SimplifiedLine {
            depth: 0,
            kind: LineKind::Include(inc.path.clone()),
            path_key: format!("include:{}", inc.path),
        });
    }
    if let Some(root) = &tree.root {
        build_node_lines(tab, root, "/", 0);
    }
    for rn in &tree.reference_nodes {
        let ref_str = format!("{}", rn.reference);
        build_node_lines(tab, &rn.node, &ref_str, 0);
    }
}

fn build_node_lines(tab: &mut Tab, node: &dts::Node, path: &str, depth: usize) {
    let status = node_status(node);
    let node_key = path.to_string();
    let is_expanded = tab.expanded.contains(&node_key);

    tab.simplified_lines.push(SimplifiedLine {
        depth,
        kind: LineKind::NodeHeader {
            name: if node.name.is_empty() {
                path.to_string()
            } else {
                node.full_name()
            },
            status,
            has_children: !node.children.is_empty() || !node.properties.is_empty(),
        },
        path_key: node_key.clone(),
    });

    if is_expanded {
        for prop in &node.properties {
            let value = match &prop.value {
                Some(v) => dts::format_property_value(v),
                None => String::new(),
            };
            tab.simplified_lines.push(SimplifiedLine {
                depth: depth + 1,
                kind: LineKind::Property {
                    name: prop.name.clone(),
                    value,
                },
                path_key: format!("{}/{}", node_key, prop.name),
            });
        }
        for child in &node.children {
            let child_path = format!("{}/{}", path, child.full_name());
            build_node_lines(tab, child, &child_path, depth + 1);
        }
    }
}

fn build_binding_lines(tab: &mut Tab, binding: &Binding) {
    let compatible = binding
        .compatible
        .as_deref()
        .unwrap_or("(no compatible)")
        .to_string();
    let description = binding
        .description
        .as_deref()
        .unwrap_or("")
        .to_string();

    tab.simplified_lines.push(SimplifiedLine {
        depth: 0,
        kind: LineKind::BindingHeader {
            compatible,
            description,
        },
        path_key: "binding:header".to_string(),
    });

    for name in binding.include_file_names() {
        tab.simplified_lines.push(SimplifiedLine {
            depth: 0,
            kind: LineKind::Include(name.to_string()),
            path_key: format!("binding:include:{name}"),
        });
    }

    for (name, spec) in &binding.properties {
        let prop_type = spec
            .property_type
            .as_ref()
            .map(|t| format!("{t:?}"))
            .unwrap_or_default();
        let desc = spec.description.as_deref().unwrap_or("").to_string();

        let key = format!("binding:prop:{name}");
        let is_expanded = tab.expanded.contains(&key);

        tab.simplified_lines.push(SimplifiedLine {
            depth: 0,
            kind: LineKind::BindingProperty {
                name: name.clone(),
                prop_type: prop_type.clone(),
                required: spec.required,
                description: if is_expanded { desc } else { String::new() },
            },
            path_key: key,
        });
    }
}

fn render_raw(tab: &mut Tab, frame: &mut Frame, area: Rect, search_matches: &[usize]) {
    let height = area.height as usize;

    // Keep the cursor in view.
    if tab.selected_line < tab.scroll {
        tab.scroll = tab.selected_line;
    } else if tab.selected_line >= tab.scroll + height {
        tab.scroll = tab.selected_line.saturating_sub(height.saturating_sub(1));
    }

    let Some(content) = &tab.raw_content else {
        return;
    };

    let width = area.width as usize;
    let all_lines: Vec<&str> = content.lines().collect();
    let total = all_lines.len();
    let scroll = tab.scroll.min(total.saturating_sub(height));

    // Reserve 1 col on the right for the scrollbar.
    let scrollbar_width: usize = 1;
    let gutter_width = 5;
    let text_width = width.saturating_sub(gutter_width + scrollbar_width);

    // Visual selection range.
    let sel_range = tab.selection_anchor.map(|a| {
        let lo = a.min(tab.selected_line);
        let hi = a.max(tab.selected_line);
        (lo, hi)
    });

    let lines: Vec<Line> = all_lines
        .iter()
        .enumerate()
        .skip(scroll)
        .take(height)
        .map(|(i, line_str)| {
            let is_cursor = i == tab.selected_line;
            let in_sel = sel_range.map_or(false, |(lo, hi)| i >= lo && i <= hi);
            let is_match = search_matches.contains(&i);
            let lineno_style = if is_cursor {
                theme::lineno_active()
            } else {
                theme::lineno_inactive()
            };
            let lineno = Span::styled(format!("{:>4} ", i + 1), lineno_style);

            let expanded = line_str.replace('\t', "    ");
            let truncated = if expanded.len() > text_width {
                format!("{}...", &expanded[..text_width.saturating_sub(3)])
            } else {
                expanded
            };

            // Determine overlay highlight style.
            let overlay = if in_sel {
                Some(theme::selection_style())
            } else if is_match && is_cursor {
                Some(theme::search_match_cursor_style())
            } else if is_match {
                Some(theme::search_match_style())
            } else if is_cursor {
                Some(Style::default().bg(theme::SELECTION_BG))
            } else {
                None
            };

            // Tokenize the line for syntax coloring.
            let token_spans = tokenize_dts_line(&truncated);

            // Apply overlay on top of token styles if needed.
            let styled_spans = if let Some(ov) = overlay {
                token_spans.into_iter().map(|s| {
                    Span::styled(s.content, s.style.patch(ov))
                }).collect::<Vec<_>>()
            } else {
                token_spans
            };

            let mut spans = vec![lineno];
            spans.extend(styled_spans);
            Line::from(spans)
        })
        .collect();

    // Content area (without scrollbar column).
    let content_area = Rect {
        x: area.x,
        y: area.y,
        width: area.width.saturating_sub(scrollbar_width as u16),
        height: area.height,
    };
    let paragraph = Paragraph::new(lines);
    frame.render_widget(paragraph, content_area);

    // Scrollbar on the right edge.
    render_scrollbar(frame, area, scroll, total, height);
}

fn render_simplified(tab: &Tab, frame: &mut Frame, area: Rect, search_matches: &[usize]) {
    if tab.simplified_lines.is_empty() {
        let msg = Paragraph::new("Nothing to display")
            .style(theme::muted());
        frame.render_widget(msg, area);
        return;
    }

    let height = area.height as usize;
    let offset = tab.simplified_scroll;
    let total = tab.simplified_lines.len();

    let sel_range = tab.selection_anchor.map(|a| {
        let lo = a.min(tab.selected_line);
        let hi = a.max(tab.selected_line);
        (lo, hi)
    });

    let scrollbar_width: u16 = 1;

    let lines: Vec<Line> = tab
        .simplified_lines
        .iter()
        .enumerate()
        .skip(offset)
        .take(height)
        .map(|(i, sline)| {
            let is_cursor = i == tab.selected_line;
            let in_sel = sel_range.map_or(false, |(lo, hi)| i >= lo && i <= hi);
            let is_match = search_matches.contains(&i);
            render_simplified_line(tab, sline, is_cursor, in_sel, is_match)
        })
        .collect();

    let content_area = Rect {
        x: area.x,
        y: area.y,
        width: area.width.saturating_sub(scrollbar_width),
        height: area.height,
    };
    let paragraph = Paragraph::new(lines);
    frame.render_widget(paragraph, content_area);

    render_scrollbar(frame, area, offset, total, height);
}

fn render_simplified_line(
    tab: &Tab,
    sline: &SimplifiedLine,
    is_cursor: bool,
    in_selection: bool,
    is_match: bool,
) -> Line<'static> {
    let indent = "  ".repeat(sline.depth);
    let sel_style = if in_selection {
        theme::selection_style()
    } else if is_match && is_cursor {
        theme::search_match_cursor_style()
    } else if is_match {
        theme::search_match_style()
    } else if is_cursor {
        theme::cursor_style()
    } else {
        Style::default()
    };

    match &sline.kind {
        LineKind::Include(path) => {
            Line::from(vec![
                Span::raw(indent),
                Span::styled("#include ", Style::default().fg(theme::COPPER)),
                Span::styled(
                    format!("\"{path}\""),
                    Style::default()
                        .fg(theme::AMBER)
                        .add_modifier(Modifier::UNDERLINED),
                ),
            ])
            .style(sel_style)
        }
        LineKind::NodeHeader {
            name,
            status,
            has_children,
        } => {
            let is_expanded = tab.expanded.contains(&sline.path_key);
            let arrow = if *has_children {
                if is_expanded { "▼ " } else { "▶ " }
            } else {
                "  "
            };

            let dot = match status {
                StatusColor::Okay => Span::styled("● ", Style::default().fg(theme::SUCCESS)),
                StatusColor::Disabled => Span::styled("● ", Style::default().fg(theme::ERROR)),
                StatusColor::Unknown => {
                    Span::styled("● ", Style::default().fg(theme::TEXT_DIM))
                }
                StatusColor::None => Span::raw("  "),
            };

            Line::from(vec![
                Span::raw(indent),
                Span::styled(arrow, Style::default().fg(theme::TEXT_MUTED)),
                dot,
                Span::styled(name.clone(), Style::default().fg(theme::GOLD)),
            ])
            .style(sel_style)
        }
        LineKind::Property { name, value } => {
            let val_display = if value.is_empty() {
                String::new()
            } else {
                format!(" = {value}")
            };
            Line::from(vec![
                Span::raw(indent),
                Span::styled(name.clone(), Style::default().fg(theme::TEXT)),
                Span::styled(val_display, Style::default().fg(theme::TEXT_SECONDARY)),
            ])
            .style(sel_style)
        }
        LineKind::BindingHeader {
            compatible,
            description,
        } => {
            Line::from(vec![
                Span::styled(
                    compatible.clone(),
                    Style::default()
                        .fg(theme::AMBER)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::raw("  "),
                Span::styled(description.clone(), Style::default().fg(theme::TEXT_SECONDARY)),
            ])
            .style(sel_style)
        },
        LineKind::BindingProperty {
            name,
            prop_type,
            required,
            description,
        } => {
            let req_marker = if *required { "* " } else { "  " };
            let is_expanded = tab.expanded.contains(&sline.path_key);
            let arrow = if is_expanded { "▼ " } else { "▶ " };

            let mut spans = vec![
                Span::raw(indent),
                Span::styled(arrow, Style::default().fg(theme::TEXT_MUTED)),
                Span::styled(req_marker, Style::default().fg(theme::ERROR)),
                Span::styled(name.clone(), Style::default().fg(theme::TEXT)),
                Span::styled(
                    format!(" ({prop_type})"),
                    Style::default().fg(theme::TEXT_MUTED),
                ),
            ];

            if is_expanded && !description.is_empty() {
                spans.push(Span::styled(
                    format!("  {description}"),
                    Style::default().fg(theme::TEXT_SECONDARY),
                ));
            }

            Line::from(spans).style(sel_style)
        }
    }
}

// ---------------------------------------------------------------------------
// Scrollbar
// ---------------------------------------------------------------------------

/// Render a 1-column scrollbar on the right edge of `area`.
fn render_scrollbar(frame: &mut Frame, area: Rect, scroll: usize, total: usize, height: usize) {
    if total <= height || area.width == 0 || area.height == 0 {
        return;
    }

    let track_height = area.height as usize;
    let thumb_size = ((height as f64 / total as f64) * track_height as f64)
        .ceil()
        .max(1.0) as usize;
    let thumb_offset =
        ((scroll as f64 / total as f64) * track_height as f64).floor() as usize;
    let thumb_offset = thumb_offset.min(track_height.saturating_sub(thumb_size));

    let x = area.x + area.width - 1;
    for row in 0..track_height {
        let y = area.y + row as u16;
        let (ch, color) = if row >= thumb_offset && row < thumb_offset + thumb_size {
            (theme::SCROLLBAR_THUMB, theme::SCROLLBAR_THUMB_COLOR)
        } else {
            (theme::SCROLLBAR_TRACK, theme::SCROLLBAR_TRACK_COLOR)
        };
        frame.render_widget(
            Paragraph::new(ch).style(Style::default().fg(color)),
            Rect { x, y, width: 1, height: 1 },
        );
    }
}

// ---------------------------------------------------------------------------
// DTS syntax tokenizer (moderate: names + values + keywords)
// ---------------------------------------------------------------------------

/// Tokenize a single DTS source line into styled spans.
fn tokenize_dts_line(line: &str) -> Vec<Span<'static>> {
    let trimmed = line.trim_start();

    // Blank / whitespace-only lines.
    if trimmed.is_empty() {
        return vec![Span::raw(line.to_string())];
    }

    // Full-line comment: // ...
    if trimmed.starts_with("//") {
        return vec![Span::styled(line.to_string(), theme::dts_comment())];
    }

    // Block comment line (heuristic: starts with /* or *)
    if trimmed.starts_with("/*") || trimmed.starts_with('*') {
        return vec![Span::styled(line.to_string(), theme::dts_comment())];
    }

    // #include line
    if trimmed.starts_with("#include") {
        let leading = &line[..line.len() - trimmed.len()];
        let mut spans = vec![Span::raw(leading.to_string())];
        if let Some(rest) = trimmed.strip_prefix("#include") {
            spans.push(Span::styled("#include", theme::dts_include_keyword()));
            spans.push(Span::styled(rest.to_string(), Style::default().fg(theme::AMBER)));
        }
        return spans;
    }

    // /dts-v1/; or /plugin/;
    if trimmed.starts_with("/dts-v1/") || trimmed.starts_with("/plugin/") {
        return vec![Span::styled(line.to_string(), theme::dts_keyword())];
    }

    // &reference lines (e.g. &i2c1 { or &{/soc/...} {)
    if trimmed.starts_with('&') {
        let leading = &line[..line.len() - trimmed.len()];
        let mut spans = vec![Span::raw(leading.to_string())];
        // Find end of reference token
        let ref_end = trimmed.find(|c: char| c == ' ' || c == '{' || c == ';').unwrap_or(trimmed.len());
        spans.push(Span::styled(trimmed[..ref_end].to_string(), theme::dts_reference()));
        if ref_end < trimmed.len() {
            spans.push(Span::styled(trimmed[ref_end..].to_string(), Style::default().fg(theme::TEXT_DIM)));
        }
        return spans;
    }

    // Lines with = (property assignment)
    if let Some(eq_pos) = trimmed.find('=') {
        let leading = &line[..line.len() - trimmed.len()];
        let prop_name = &trimmed[..eq_pos].trim_end();
        let rest = &trimmed[eq_pos..];
        return vec![
            Span::raw(leading.to_string()),
            Span::styled(prop_name.to_string(), theme::dts_property_name()),
            Span::styled(" ".to_string(), Style::default()),
            Span::styled(rest.to_string(), Style::default().fg(theme::TEXT_SECONDARY)),
        ];
    }

    // Lines ending with { — node header
    if trimmed.ends_with('{') {
        let leading = &line[..line.len() - trimmed.len()];
        let name_part = trimmed.trim_end_matches('{').trim_end();
        return vec![
            Span::raw(leading.to_string()),
            Span::styled(name_part.to_string(), theme::dts_node_name()),
            Span::styled(" {", Style::default().fg(theme::TEXT_DIM)),
        ];
    }

    // Closing brace
    if trimmed.starts_with('}') {
        return vec![Span::styled(line.to_string(), Style::default().fg(theme::TEXT_DIM))];
    }

    // Default: plain text
    vec![Span::raw(line.to_string())]
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn node_status(node: &dts::Node) -> StatusColor {
    match node.property("status") {
        Some(prop) => match prop.as_string() {
            Some("okay") => StatusColor::Okay,
            Some("disabled") => StatusColor::Disabled,
            _ => StatusColor::Unknown,
        },
        None => StatusColor::None,
    }
}

/// Extract plain text representation of a simplified line (for yanking).
fn simplified_line_text(sl: &SimplifiedLine) -> String {
    let indent = "  ".repeat(sl.depth);
    match &sl.kind {
        LineKind::Include(path) => format!("{indent}#include \"{path}\""),
        LineKind::NodeHeader { name, .. } => format!("{indent}{name}"),
        LineKind::Property { name, value } => {
            if value.is_empty() {
                format!("{indent}{name}")
            } else {
                format!("{indent}{name} = {value}")
            }
        }
        LineKind::BindingHeader {
            compatible,
            description,
        } => format!("{compatible}  {description}"),
        LineKind::BindingProperty {
            name,
            prop_type,
            required,
            description,
        } => {
            let req = if *required { "* " } else { "" };
            if description.is_empty() {
                format!("{indent}{req}{name} ({prop_type})")
            } else {
                format!("{indent}{req}{name} ({prop_type})  {description}")
            }
        }
    }
}

/// Walk a node tree to find the node at a given path key.
///
/// Path keys look like `/soc/i2c@40003000` (matching the simplified line keys).
fn find_node_by_path<'a>(node: &'a dts::Node, target: &str, current: &str) -> Option<&'a dts::Node> {
    if current == target {
        return Some(node);
    }
    for child in &node.children {
        let child_path = format!("{}/{}", current, child.full_name());
        if let Some(found) = find_node_by_path(child, target, &child_path) {
            return Some(found);
        }
    }
    None
}
