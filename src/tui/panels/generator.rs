use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use ratatui::{
    prelude::*,
    widgets::{Paragraph, Wrap},
};

use crate::dts::{
    self, Binding, DeviceTree, DtsVersion, Node, OutputFormat, Property, Reference, ReferenceNode,
    SerializerConfig,
};
use crate::tui::theme;

const OVERLAY_EXTENSIONS: &[&str] = &[".overlay", ".dtso"];

// ---------------------------------------------------------------------------
// Enums
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GeneratorStep {
    SelectBoard,
    EditNodes,
    SaveFile,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InputMode {
    NodeReference,
    ChildName,
    PropertyName,
    PropertyValue,
    #[allow(dead_code)]
    FileName,
}

/// Describes exactly what a visible line in the EditNodes view points to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NodeLocation {
    /// A node header. `path` is e.g. "0" (ref node 0) or "0/c1/c0".
    NodeHeader { path: String },
    /// A property line within a node.
    Property { node_path: String, prop_idx: usize },
}

// ---------------------------------------------------------------------------
// PropertyEditState
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct PropertyEditState {
    /// Path to the node containing the property (e.g. "0" or "0/c1/c0").
    pub node_path: String,
    pub prop_idx: usize,
    pub name: String,
    pub value: String,
}

// ---------------------------------------------------------------------------
// GeneratorState
// ---------------------------------------------------------------------------

pub struct GeneratorState {
    pub collapsed: bool,
    pub step: GeneratorStep,
    pub selected_board: Option<String>,
    pub overlay_tree: DeviceTree,
    pub resolved_board_tree: Option<DeviceTree>,
    pub board_bindings: HashMap<String, Binding>,
    pub board_resolving: bool,
    pub selected_node: usize,
    pub node_scroll: usize,
    /// Expanded node paths. "0" is top-level ref node 0, "0/c1" is child 1
    /// of ref node 0, etc.
    pub expanded_nodes: HashSet<String>,
    pub editing_property: Option<PropertyEditState>,
    pub input_buffer: String,
    pub input_mode: Option<InputMode>,
    pub save_path: Option<PathBuf>,
    pub save_dir: PathBuf,
    pub save_entries: Vec<String>,
    pub save_selected: usize,
    pub save_scroll: usize,
    pub save_input: String,
    pub save_input_active: bool,
    pub confirm_overwrite: bool,
    /// Whether overlay was saved and user is being asked if they want to continue.
    pub save_complete: bool,
}

impl GeneratorState {
    // ------------------------------------------------------------------
    // Construction
    // ------------------------------------------------------------------

    pub fn new() -> Self {
        let mut tree = DeviceTree::new();
        tree.version = Some(DtsVersion::V1);
        tree.is_plugin = true;

        Self {
            collapsed: false,
            step: GeneratorStep::SelectBoard,
            selected_board: None,
            overlay_tree: tree,
            resolved_board_tree: None,
            board_bindings: HashMap::new(),
            board_resolving: false,
            selected_node: 0,
            node_scroll: 0,
            expanded_nodes: HashSet::new(),
            editing_property: None,
            input_buffer: String::new(),
            input_mode: None,
            save_path: None,
            save_dir: PathBuf::from("."),
            save_entries: Vec::new(),
            save_selected: 0,
            save_scroll: 0,
            save_input: String::new(),
            save_input_active: false,
            confirm_overwrite: false,
            save_complete: false,
        }
    }

    // ------------------------------------------------------------------
    // Panel toggle
    // ------------------------------------------------------------------

    pub fn toggle_collapsed(&mut self) {
        self.collapsed = !self.collapsed;
    }

    /// Reset the overlay to start fresh (keeps board selection).
    pub fn reset_overlay(&mut self) {
        let mut tree = DeviceTree::new();
        tree.version = Some(DtsVersion::V1);
        tree.is_plugin = true;
        self.overlay_tree = tree;
        self.selected_node = 0;
        self.node_scroll = 0;
        self.expanded_nodes.clear();
        self.editing_property = None;
        self.input_buffer.clear();
        self.input_mode = None;
        self.save_path = None;
        self.save_input.clear();
        self.save_input_active = false;
        self.confirm_overwrite = false;
        self.save_complete = false;
        self.step = GeneratorStep::EditNodes;
    }

    // ------------------------------------------------------------------
    // Board sync
    // ------------------------------------------------------------------

    pub fn sync_board(&mut self, board_name: Option<&str>) {
        let new_name = board_name.map(|s| s.to_string());
        if new_name != self.selected_board {
            self.selected_board = new_name;
            self.resolved_board_tree = None;
            self.board_bindings.clear();
            self.board_resolving = false;
        }
    }

    pub fn set_resolved_tree(&mut self, tree: DeviceTree) {
        self.resolved_board_tree = Some(tree);
        self.board_resolving = false;
    }

    pub fn set_bindings(&mut self, bindings: HashMap<String, Binding>) {
        self.board_bindings = bindings;
    }

    // ------------------------------------------------------------------
    // Step navigation
    // ------------------------------------------------------------------

    pub fn next_step(&mut self) {
        self.step = match self.step {
            GeneratorStep::SelectBoard => GeneratorStep::EditNodes,
            GeneratorStep::EditNodes => GeneratorStep::SaveFile,
            GeneratorStep::SaveFile => GeneratorStep::SaveFile,
        };
    }

    pub fn prev_step(&mut self) {
        self.step = match self.step {
            GeneratorStep::SelectBoard => GeneratorStep::SelectBoard,
            GeneratorStep::EditNodes => GeneratorStep::SelectBoard,
            GeneratorStep::SaveFile => GeneratorStep::EditNodes,
        };
    }

    // ------------------------------------------------------------------
    // Overlay tree helpers
    // ------------------------------------------------------------------

    fn parse_reference(input: &str) -> Reference {
        if input.starts_with('/') {
            Reference::Path(input.to_string())
        } else {
            let label = input.strip_prefix('&').unwrap_or(input);
            Reference::Label(label.to_string())
        }
    }

    fn push_reference_node(&mut self, reference: Reference) {
        let mut node = Node::new("");
        node.properties
            .push(Property::new_string("status", "okay"));
        self.overlay_tree.reference_nodes.push(ReferenceNode {
            reference,
            node,
        });
    }

    fn check_and_set_save_path(&mut self, filename: String) {
        let path = self.save_dir.join(&filename);
        if path.exists() {
            self.confirm_overwrite = true;
            self.save_input = filename;
        } else {
            self.save_path = Some(path);
        }
    }

    pub fn overlay_node_count(&self) -> usize {
        self.overlay_tree.reference_nodes.len()
    }

    pub fn add_node_from_reference(&mut self, reference: Reference, labels: &[String]) {
        let chosen_ref = if !labels.is_empty() {
            Reference::Label(labels[0].clone())
        } else {
            reference
        };
        self.push_reference_node(chosen_ref);
    }

    pub fn delete_selected_node(&mut self) {
        let loc = match self.line_to_location(self.selected_node) {
            Some(l) => l,
            None => return,
        };
        match loc {
            NodeLocation::NodeHeader { path } => {
                // Check if this is a top-level ref node (path has no '/').
                if !path.contains('/') {
                    let node_idx: usize = match path.parse() {
                        Ok(i) => i,
                        Err(_) => return,
                    };
                    self.overlay_tree.reference_nodes.remove(node_idx);
                    // Remove any expanded paths starting with this index and
                    // shift paths for nodes after the removed one.
                    let prefix = format!("{node_idx}");
                    self.expanded_nodes.retain(|p| !p.starts_with(&prefix));
                    let shifted: HashSet<String> = self
                        .expanded_nodes
                        .drain()
                        .map(|p| shift_path_after_remove(&p, node_idx))
                        .collect();
                    self.expanded_nodes = shifted;
                } else {
                    // Remove a child node. Parse the parent path and child index.
                    let (parent_path, child_part) =
                        path.rsplit_once('/').expect("checked contains /");
                    let ci: usize = match child_part.strip_prefix('c').and_then(|s| s.parse().ok())
                    {
                        Some(i) => i,
                        None => return,
                    };
                    // Remove expanded paths under this child.
                    self.expanded_nodes.retain(|p| !p.starts_with(&path));
                    if let Some(parent) = self.node_at_path_mut(parent_path) {
                        if ci < parent.children.len() {
                            parent.children.remove(ci);
                        }
                    }
                }
                let new_count = self.edit_visible_line_count();
                if new_count == 0 {
                    self.selected_node = 0;
                } else if self.selected_node >= new_count {
                    self.selected_node = new_count.saturating_sub(1);
                }
            }
            NodeLocation::Property { node_path, prop_idx } => {
                if let Some(node) = self.node_at_path_mut(&node_path) {
                    if prop_idx < node.properties.len() {
                        node.properties.remove(prop_idx);
                    }
                }
                let new_count = self.edit_visible_line_count();
                if self.selected_node >= new_count && new_count > 0 {
                    self.selected_node = new_count - 1;
                }
            }
        }
    }

    #[allow(dead_code)]
    pub fn delete_selected_property(&mut self) {
        // If editing_property is active, delete that; otherwise do nothing.
        if let Some(edit) = self.editing_property.take() {
            if let Some(node) = self.node_at_path_mut(&edit.node_path) {
                if edit.prop_idx < node.properties.len() {
                    node.properties.remove(edit.prop_idx);
                }
            }
        }
    }

    #[allow(dead_code)]
    pub fn get_binding_for_node<'a>(&'a self, node: &Node) -> Option<&'a Binding> {
        let compat = node.property("compatible")?.as_string()?;
        self.board_bindings.get(compat)
    }

    // ------------------------------------------------------------------
    // Cursor / scroll navigation
    // ------------------------------------------------------------------

    /// Count the total number of visible lines in the EditNodes view.
    fn edit_visible_line_count(&self) -> usize {
        let mut count = 0;
        for (idx, rn) in self.overlay_tree.reference_nodes.iter().enumerate() {
            let path = idx.to_string();
            count += 1; // node header
            count += self.count_node_children_lines(&rn.node, &path);
        }
        count
    }

    /// Recursively count visible lines for a node's contents (props + children).
    fn count_node_children_lines(&self, node: &Node, path: &str) -> usize {
        if !self.expanded_nodes.contains(path) {
            return 0;
        }
        let mut count = node.properties.len();
        for (ci, child) in node.children.iter().enumerate() {
            let child_path = format!("{path}/c{ci}");
            count += 1; // child header
            count += self.count_node_children_lines(child, &child_path);
        }
        count
    }

    /// Map a flat visible-line index to a `NodeLocation`.
    pub fn line_to_location(&self, line: usize) -> Option<NodeLocation> {
        let mut cursor = 0;
        for (idx, rn) in self.overlay_tree.reference_nodes.iter().enumerate() {
            let path = idx.to_string();
            if cursor == line {
                return Some(NodeLocation::NodeHeader { path });
            }
            cursor += 1;
            if let Some(loc) = self.line_to_location_in_node(&rn.node, &path, line, &mut cursor) {
                return Some(loc);
            }
        }
        None
    }

    /// Recursively search within a node's contents for the given line index.
    fn line_to_location_in_node(
        &self,
        node: &Node,
        path: &str,
        line: usize,
        cursor: &mut usize,
    ) -> Option<NodeLocation> {
        if !self.expanded_nodes.contains(path) {
            return None;
        }
        // Properties
        for pi in 0..node.properties.len() {
            if *cursor == line {
                return Some(NodeLocation::Property {
                    node_path: path.to_string(),
                    prop_idx: pi,
                });
            }
            *cursor += 1;
        }
        // Children
        for (ci, child) in node.children.iter().enumerate() {
            let child_path = format!("{path}/c{ci}");
            if *cursor == line {
                return Some(NodeLocation::NodeHeader {
                    path: child_path,
                });
            }
            *cursor += 1;
            if let Some(loc) =
                self.line_to_location_in_node(child, &child_path, line, cursor)
            {
                return Some(loc);
            }
        }
        None
    }

    /// Resolve a path string to a mutable reference to the `Node`.
    /// Path format: "0" → ref_nodes[0].node, "0/c1" → ref_nodes[0].node.children[1], etc.
    fn node_at_path_mut(&mut self, path: &str) -> Option<&mut Node> {
        let mut parts = path.split('/');
        let root_idx: usize = parts.next()?.parse().ok()?;
        let rn = self.overlay_tree.reference_nodes.get_mut(root_idx)?;
        let mut node = &mut rn.node;
        for part in parts {
            let ci: usize = part.strip_prefix('c')?.parse().ok()?;
            node = node.children.get_mut(ci)?;
        }
        Some(node)
    }

    /// Resolve a path string to an immutable reference to the `Node`.
    fn node_at_path(&self, path: &str) -> Option<&Node> {
        let mut parts = path.split('/');
        let root_idx: usize = parts.next()?.parse().ok()?;
        let rn = self.overlay_tree.reference_nodes.get(root_idx)?;
        let mut node = &rn.node;
        for part in parts {
            let ci: usize = part.strip_prefix('c')?.parse().ok()?;
            node = node.children.get(ci)?;
        }
        Some(node)
    }

    /// Get the node path for the currently selected line. Returns NodeHeader
    /// path if on a node, or the parent node_path if on a property.
    fn selected_node_path(&self) -> Option<String> {
        match self.line_to_location(self.selected_node)? {
            NodeLocation::NodeHeader { path } => Some(path),
            NodeLocation::Property { node_path, .. } => Some(node_path),
        }
    }

    pub fn move_up(&mut self) {
        match self.step {
            GeneratorStep::SelectBoard => {}
            GeneratorStep::EditNodes => {
                self.selected_node = self.selected_node.saturating_sub(1);
            }
            GeneratorStep::SaveFile => {
                self.save_move_up();
            }
        }
    }

    pub fn move_down(&mut self) {
        match self.step {
            GeneratorStep::SelectBoard => {}
            GeneratorStep::EditNodes => {
                let count = self.edit_visible_line_count();
                if count > 0 && self.selected_node < count - 1 {
                    self.selected_node += 1;
                }
            }
            GeneratorStep::SaveFile => {
                self.save_move_down();
            }
        }
    }

    pub fn toggle_expand(&mut self) {
        if self.step != GeneratorStep::EditNodes {
            return;
        }
        if let Some(NodeLocation::NodeHeader { path }) =
            self.line_to_location(self.selected_node)
        {
            if self.expanded_nodes.contains(&path) {
                self.expanded_nodes.remove(&path);
            } else {
                self.expanded_nodes.insert(path);
            }
        }
    }

    // ------------------------------------------------------------------
    // Input helpers
    // ------------------------------------------------------------------

    pub fn start_new_node(&mut self) {
        self.input_mode = Some(InputMode::NodeReference);
        self.input_buffer.clear();
    }

    pub fn start_add_property(&mut self) {
        let node_path = match self.selected_node_path() {
            Some(p) => p,
            None => return,
        };
        let prop_count = match self.node_at_path(&node_path) {
            Some(n) => n.properties.len(),
            None => return,
        };
        self.input_mode = Some(InputMode::PropertyName);
        self.input_buffer.clear();
        self.editing_property = Some(PropertyEditState {
            node_path,
            prop_idx: prop_count,
            name: String::new(),
            value: String::new(),
        });
    }

    pub fn start_edit_property(&mut self) {
        let loc = match self.line_to_location(self.selected_node) {
            Some(l) => l,
            None => return,
        };
        let (node_path, prop_idx) = match loc {
            NodeLocation::Property { node_path, prop_idx } => (node_path, prop_idx),
            NodeLocation::NodeHeader { path } => {
                // On a node header — try first property.
                let node = match self.node_at_path(&path) {
                    Some(n) => n,
                    None => return,
                };
                if node.properties.is_empty() {
                    return;
                }
                (path, 0)
            }
        };
        let node = match self.node_at_path(&node_path) {
            Some(n) => n,
            None => return,
        };
        if prop_idx >= node.properties.len() {
            return;
        }
        let prop = &node.properties[prop_idx];
        let value = match &prop.value {
            Some(v) => dts::format_property_value(v),
            None => String::new(),
        };
        self.editing_property = Some(PropertyEditState {
            node_path,
            prop_idx,
            name: prop.name.clone(),
            value: value.clone(),
        });
        self.input_mode = Some(InputMode::PropertyValue);
        self.input_buffer = value;
    }

    pub fn start_child_node(&mut self) {
        // Add child to the node currently under cursor.
        if self.selected_node_path().is_some() {
            self.input_mode = Some(InputMode::ChildName);
            self.input_buffer.clear();
        }
    }

    pub fn confirm_input(&mut self) {
        let mode = match self.input_mode.take() {
            Some(m) => m,
            None => return,
        };
        let buf = std::mem::take(&mut self.input_buffer);

        match mode {
            InputMode::NodeReference => {
                if !buf.is_empty() {
                    let reference = Self::parse_reference(&buf);
                    self.push_reference_node(reference);
                }
            }
            InputMode::ChildName => {
                if !buf.is_empty() {
                    if let Some(node_path) = self.selected_node_path() {
                        let child = Node::new(buf);
                        if let Some(node) = self.node_at_path_mut(&node_path) {
                            node.children.push(child);
                        }
                    }
                }
            }
            InputMode::PropertyName => {
                if !buf.is_empty() {
                    // Store the name temporarily and switch to value input
                    self.input_mode = Some(InputMode::PropertyValue);
                    if let Some(edit) = &mut self.editing_property {
                        edit.name = buf;
                    }
                    self.input_buffer.clear();
                    return; // stay in input mode
                }
            }
            InputMode::PropertyValue => {
                if let Some(edit) = self.editing_property.take() {
                    if let Some(node) = self.node_at_path_mut(&edit.node_path) {
                        let prop = if buf.is_empty() {
                            Property::new_boolean(&edit.name)
                        } else if let Some(parsed) = dts::parse_property_value_str(&buf) {
                            // User typed a valid DTS value (cell array, byte
                            // string, phandle ref, quoted string, etc.).
                            Property {
                                name: edit.name,
                                value: Some(parsed),
                                labels: Vec::new(),
                            }
                        } else {
                            // Fall back to treating the raw text as a string.
                            Property::new_string(&edit.name, &buf)
                        };
                        if edit.prop_idx < node.properties.len() {
                            node.properties[edit.prop_idx] = prop;
                        } else {
                            node.properties.push(prop);
                        }
                    }
                }
            }
            InputMode::FileName => {
                if !buf.is_empty() {
                    self.check_and_set_save_path(buf);
                }
            }
        }
    }

    pub fn cancel_input(&mut self) {
        self.input_mode = None;
        self.input_buffer.clear();
        self.editing_property = None;
    }

    pub fn push_char(&mut self, c: char) {
        self.input_buffer.push(c);
    }

    pub fn pop_char(&mut self) {
        self.input_buffer.pop();
    }

    // ------------------------------------------------------------------
    // Serialization
    // ------------------------------------------------------------------

    pub fn build_overlay_string(&self) -> String {
        let config = SerializerConfig {
            output_format: OutputFormat::Dts,
            header_comment: Some("Generated by zdtwalk".to_string()),
            include_version: false,
            ..Default::default()
        };
        // Build a tree copy without plugin/version flags so the serializer
        // emits only the header comment and reference nodes.
        let mut tree = self.overlay_tree.clone();
        tree.is_plugin = false;
        tree.version = None;
        dts::serialize(&tree, &config)
    }

    // ------------------------------------------------------------------
    // File browser
    // ------------------------------------------------------------------

    pub fn init_save_browser(&mut self, workspace_root: &Path) {
        self.save_dir = workspace_root.to_path_buf();
        self.refresh_save_entries();
        self.save_selected = 0;
        self.save_scroll = 0;
        self.save_input.clear();
        self.save_input_active = false;
        self.confirm_overwrite = false;
    }

    fn refresh_save_entries(&mut self) {
        self.save_entries.clear();
        if let Ok(entries) = std::fs::read_dir(&self.save_dir) {
            let mut names: Vec<String> = entries
                .filter_map(|e| e.ok())
                .filter(|e| {
                    if let Ok(ft) = e.file_type() {
                        if ft.is_dir() {
                            return true;
                        }
                        if let Some(name) = e.file_name().to_str() {
                            return OVERLAY_EXTENSIONS.iter().any(|ext| name.ends_with(ext));
                        }
                    }
                    false
                })
                .filter_map(|e| e.file_name().into_string().ok())
                .collect();
            names.sort();
            self.save_entries = names;
        }
    }

    pub fn save_move_up(&mut self) {
        if self.save_input_active {
            return;
        }
        self.save_selected = self.save_selected.saturating_sub(1);
    }

    pub fn save_move_down(&mut self) {
        if self.save_input_active {
            return;
        }
        let count = self.save_entries.len();
        if count > 0 && self.save_selected < count - 1 {
            self.save_selected += 1;
        }
    }

    pub fn save_enter(&mut self) {
        if self.save_input_active {
            let name = std::mem::take(&mut self.save_input);
            if !name.is_empty() {
                self.check_and_set_save_path(name);
                if !self.confirm_overwrite {
                    self.save_input_active = false;
                }
            }
            return;
        }
        if self.save_selected >= self.save_entries.len() {
            return;
        }
        let entry = self.save_entries[self.save_selected].clone();
        let path = self.save_dir.join(&entry);
        if path.is_dir() {
            self.save_dir = path;
            self.refresh_save_entries();
            self.save_selected = 0;
            self.save_scroll = 0;
        } else {
            self.save_path = Some(path);
        }
    }

    pub fn save_back(&mut self) {
        if let Some(parent) = self.save_dir.parent() {
            self.save_dir = parent.to_path_buf();
            self.refresh_save_entries();
            self.save_selected = 0;
            self.save_scroll = 0;
        }
    }

    pub fn save_start_new_file(&mut self) {
        self.save_input_active = true;
        self.save_input.clear();
    }

    // ------------------------------------------------------------------
    // Render
    // ------------------------------------------------------------------

    pub fn render(&self, frame: &mut Frame, area: Rect, is_active: bool) {
        let block = theme::panel_block(" Generator ", is_active);

        let inner = block.inner(area);
        frame.render_widget(block, area);

        // Step progress bar at top.
        let progress_area = Rect { x: inner.x, y: inner.y, width: inner.width, height: 1 };
        let (current_step, completed) = match self.step {
            GeneratorStep::SelectBoard => (0, 0),
            GeneratorStep::EditNodes => (1, 1),
            GeneratorStep::SaveFile => (2, 2),
        };
        frame.render_widget(
            Paragraph::new(vec![theme::step_progress_line(current_step, completed)]),
            progress_area,
        );

        let content_area = Rect {
            x: inner.x,
            y: inner.y + 2,
            width: inner.width,
            height: inner.height.saturating_sub(2),
        };

        match self.step {
            GeneratorStep::SelectBoard => self.render_select_board(frame, content_area),
            GeneratorStep::EditNodes => self.render_edit_nodes(frame, content_area),
            GeneratorStep::SaveFile => self.render_save_file(frame, content_area),
        }
    }

    // ---- SelectBoard -------------------------------------------------

    fn render_select_board(&self, frame: &mut Frame, area: Rect) {
        let mut lines: Vec<Line> = Vec::new();

        lines.push(Line::from(""));

        match &self.selected_board {
            Some(name) => {
                lines.push(Line::from(vec![
                    Span::styled("Board: ", theme::label()),
                    Span::styled(name.as_str(), Style::default().fg(theme::GOLD)),
                ]));

                let status = if self.board_resolving {
                    Span::styled("Resolving...", Style::default().fg(theme::AMBER))
                } else if self.resolved_board_tree.is_some() {
                    Span::styled("Resolved", theme::success())
                } else {
                    Span::styled("Not resolved", theme::muted())
                };
                lines.push(Line::from(vec![
                    Span::styled("Status: ", theme::label()),
                    status,
                ]));
            }
            None => {
                lines.push(Line::from(Span::styled(
                    "No board selected",
                    theme::muted(),
                )));
                lines.push(Line::from(Span::styled(
                    "Select a board in the left panel",
                    theme::muted(),
                )));
            }
        }

        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled(
            "-- Keybinds --",
            theme::label(),
        )));
        lines.push(Line::from(vec![
            Span::styled("  →/Enter ", theme::keybind_key()),
            Span::styled("continue to edit nodes", theme::keybind_desc()),
        ]));

        let paragraph = Paragraph::new(lines).wrap(Wrap { trim: true });
        frame.render_widget(paragraph, area);
    }

    // ---- EditNodes ---------------------------------------------------

    fn render_edit_nodes(&self, frame: &mut Frame, area: Rect) {
        let height = area.height as usize;
        if height == 0 {
            return;
        }

        let mut lines: Vec<Line> = Vec::new();

        // Board info
        if let Some(board) = &self.selected_board {
            lines.push(Line::from(vec![
                Span::styled("Board: ", theme::label()),
                Span::styled(board.as_str(), Style::default().fg(theme::GOLD)),
            ]));
        }
        lines.push(Line::from(""));

        if self.overlay_tree.reference_nodes.is_empty() {
            lines.push(Line::from(Span::styled(
                "No nodes added yet.",
                theme::muted(),
            )));
            lines.push(Line::from(Span::styled(
                "Press 'a' in center panel",
                theme::muted(),
            )));
            lines.push(Line::from(Span::styled(
                "or 'n' here for new node.",
                theme::muted(),
            )));
        } else {
            let mut flat_line_idx: usize = 0;
            for (idx, rn) in self.overlay_tree.reference_nodes.iter().enumerate() {
                let path = idx.to_string();
                let is_cursor = flat_line_idx == self.selected_node;
                let is_expanded = self.expanded_nodes.contains(&path);

                let ref_str = match &rn.reference {
                    Reference::Label(l) => format!("&{l}"),
                    Reference::Path(p) => format!("&{{{p}}}"),
                };

                let marker = if is_expanded { "▼" } else { "▶" };
                let header = format!("{marker} {ref_str} {{ ... }}");

                let style = if is_cursor {
                    theme::cursor_style()
                } else {
                    Style::default().fg(theme::TEXT)
                };
                lines.push(Line::from(Span::styled(header, style)));
                flat_line_idx += 1;

                if is_expanded {
                    self.render_node_contents(
                        &rn.node,
                        &path,
                        1,
                        &mut lines,
                        &mut flat_line_idx,
                    );
                }
            }
        }

        // Input prompt
        if let Some(mode) = &self.input_mode {
            lines.push(Line::from(""));
            let label = match mode {
                InputMode::NodeReference => "Ref (e.g. &i2c1): ",
                InputMode::ChildName => "Child name: ",
                InputMode::PropertyName => "Property name: ",
                InputMode::PropertyValue => "Property value: ",
                InputMode::FileName => "Filename: ",
            };
            lines.push(Line::from(vec![
                Span::styled(label, theme::input_label()),
                Span::styled(
                    format!("{}|", &self.input_buffer),
                    theme::input_field(),
                ),
            ]));
        }

        // Hints
        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled(
            "-- Keybinds --",
            theme::label(),
        )));
        let hints = [
            ("Enter", "expand/collapse node"),
            ("n", "new reference node"),
            ("p", "add property"),
            ("e", "edit property"),
            ("c", "add child node"),
            ("d", "delete node"),
            ("→", "next step (save)"),
            ("←", "previous step"),
        ];
        for (key, desc) in &hints {
            lines.push(Line::from(vec![
                Span::styled(format!("  {key:<6}"), theme::keybind_key()),
                Span::styled(*desc, theme::keybind_desc()),
            ]));
        }

        // Scroll support
        let scroll = self.node_scroll.min(lines.len().saturating_sub(height));
        let visible: Vec<Line> = lines.into_iter().skip(scroll).take(height).collect();

        let paragraph = Paragraph::new(visible);
        frame.render_widget(paragraph, area);
    }

    // ---- SaveFile ----------------------------------------------------

    fn render_save_file(&self, frame: &mut Frame, area: Rect) {
        let height = area.height as usize;
        if height == 0 {
            return;
        }

        let mut lines: Vec<Line> = Vec::new();

        // Post-save prompt
        if self.save_complete {
            lines.push(Line::from(Span::styled(
                "Overlay saved!",
                Style::default()
                    .fg(theme::SUCCESS)
                    .add_modifier(Modifier::BOLD),
            )));
            lines.push(Line::from(""));
            if let Some(path) = &self.save_path {
                lines.push(Line::from(vec![
                    Span::styled("Saved to: ", theme::label()),
                    Span::styled(
                        path.display().to_string(),
                        theme::success(),
                    ),
                ]));
            }
            lines.push(Line::from(""));
            lines.push(Line::from(Span::styled(
                "Continue with this overlay?",
                Style::default().fg(theme::AMBER),
            )));
            lines.push(Line::from(""));
            lines.push(Line::from(vec![
                Span::styled("  y     ", theme::keybind_key()),
                Span::styled("continue editing", theme::keybind_desc()),
            ]));
            lines.push(Line::from(vec![
                Span::styled("  n     ", theme::keybind_key()),
                Span::styled("start fresh overlay", theme::keybind_desc()),
            ]));
            lines.push(Line::from(vec![
                Span::styled("  g     ", theme::keybind_key()),
                Span::styled("close generator", theme::keybind_desc()),
            ]));

            let visible: Vec<Line> = lines.into_iter().take(height).collect();
            let paragraph = Paragraph::new(visible);
            frame.render_widget(paragraph, area);
            return;
        }

        lines.push(Line::from(""));

        // Current directory
        let dir_display = self.save_dir.display().to_string();
        lines.push(Line::from(vec![
            Span::styled("Dir: ", theme::label()),
            Span::styled(dir_display, Style::default().fg(theme::COPPER)),
        ]));
        lines.push(Line::from(""));

        if self.save_entries.is_empty() {
            lines.push(Line::from(Span::styled(
                "(empty directory)",
                theme::muted(),
            )));
        } else {
            let scroll = self.save_scroll;
            let total = self.save_entries.len();
            for (i, entry) in self.save_entries.iter().enumerate().skip(scroll) {
                let is_selected = i == self.save_selected;
                let is_dir = self.save_dir.join(entry).is_dir();
                let is_last = i == total - 1;
                let branch = if is_last { "└── " } else { "├── " };
                let text = format!("{branch}{entry}");
                let style = if is_selected {
                    theme::cursor_style()
                } else if is_dir {
                    Style::default().fg(theme::COPPER)
                } else {
                    Style::default().fg(theme::TEXT)
                };
                lines.push(Line::from(Span::styled(text, style)));
            }
        }

        lines.push(Line::from(""));

        // Filename input area
        if self.save_input_active {
            lines.push(Line::from(vec![
                Span::styled("Filename: ", theme::input_label()),
                Span::styled(
                    format!("{}|", &self.save_input),
                    theme::input_field(),
                ),
            ]));
        }

        if self.confirm_overwrite {
            lines.push(Line::from(Span::styled(
                "File exists! Press Enter to overwrite, Esc to cancel.",
                Style::default().fg(theme::ERROR).add_modifier(Modifier::BOLD),
            )));
        }

        if let Some(path) = &self.save_path {
            lines.push(Line::from(vec![
                Span::styled("Save to: ", theme::label()),
                Span::styled(
                    path.display().to_string(),
                    theme::success(),
                ),
            ]));
        }

        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled(
            "-- Keybinds --",
            theme::label(),
        )));
        let hints = [
            ("Enter", "select file / enter dir"),
            ("Back", "go up one directory"),
            ("n", "create new file"),
            ("←", "back to edit nodes"),
        ];
        for (key, desc) in &hints {
            lines.push(Line::from(vec![
                Span::styled(format!("  {key:<6}"), theme::keybind_key()),
                Span::styled(*desc, theme::keybind_desc()),
            ]));
        }

        let visible: Vec<Line> = lines.into_iter().take(height).collect();
        let paragraph = Paragraph::new(visible);
        frame.render_widget(paragraph, area);
    }

    // ---- Recursive node rendering helper ----------------------------

    fn render_node_contents<'a>(
        &self,
        node: &Node,
        path: &str,
        depth: usize,
        lines: &mut Vec<Line<'a>>,
        flat_line_idx: &mut usize,
    ) {
        let indent = "    ".repeat(depth);

        // Properties
        for prop in &node.properties {
            let is_cursor = *flat_line_idx == self.selected_node;
            let val_str = match &prop.value {
                Some(v) => format!(" = {}", dts::format_property_value(v)),
                None => String::new(),
            };
            let prop_text = format!("{indent}{}{};", prop.name, val_str);
            let pstyle = if is_cursor {
                theme::cursor_style()
            } else {
                Style::default().fg(theme::TEXT_SECONDARY)
            };
            lines.push(Line::from(Span::styled(prop_text, pstyle)));
            *flat_line_idx += 1;
        }

        // Children
        for (ci, child) in node.children.iter().enumerate() {
            let child_path = format!("{path}/c{ci}");
            let is_cursor = *flat_line_idx == self.selected_node;
            let is_expanded = self.expanded_nodes.contains(&child_path);
            let marker = if is_expanded { "▼" } else { "▶" };
            let child_text = format!("{indent}{marker} {} {{ ... }}", child.full_name());
            let cstyle = if is_cursor {
                theme::cursor_style()
            } else {
                Style::default().fg(theme::COPPER)
            };
            lines.push(Line::from(Span::styled(child_text, cstyle)));
            *flat_line_idx += 1;

            if is_expanded {
                self.render_node_contents(child, &child_path, depth + 1, lines, flat_line_idx);
            }
        }
    }
}

/// Shift a path string's root index when a ref node at `removed_idx` is removed.
/// E.g., "3/c1" with removed_idx=1 → "2/c1".
fn shift_path_after_remove(path: &str, removed_idx: usize) -> String {
    let (root_part, rest) = match path.split_once('/') {
        Some((r, rest)) => (r, Some(rest)),
        None => (path, None),
    };
    let root: usize = match root_part.parse() {
        Ok(i) => i,
        Err(_) => return path.to_string(),
    };
    let new_root = if root > removed_idx {
        root - 1
    } else {
        root
    };
    match rest {
        Some(r) => format!("{new_root}/{r}"),
        None => new_root.to_string(),
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generator_initial_state() {
        let gen = GeneratorState::new();
        assert_eq!(gen.step, GeneratorStep::SelectBoard);
        assert!(gen.selected_board.is_none());
        assert!(gen.overlay_tree.is_plugin);
        assert_eq!(gen.overlay_tree.version, Some(DtsVersion::V1));
        assert!(gen.overlay_tree.reference_nodes.is_empty());
    }

    #[test]
    fn add_node_from_reference_with_labels() {
        let mut gen = GeneratorState::new();
        let reference = Reference::Path("/soc/i2c@40003000".to_string());
        let labels = vec!["i2c1".to_string()];

        gen.add_node_from_reference(reference, &labels);

        assert_eq!(gen.overlay_node_count(), 1);
        let rn = &gen.overlay_tree.reference_nodes[0];
        // Should prefer label over path.
        assert_eq!(rn.reference, Reference::Label("i2c1".to_string()));
        // Should have status = "okay" pre-added.
        assert_eq!(rn.node.properties.len(), 1);
        assert_eq!(rn.node.properties[0].name, "status");
        assert_eq!(rn.node.properties[0].as_string(), Some("okay"));
    }

    #[test]
    fn add_node_from_reference_no_labels() {
        let mut gen = GeneratorState::new();
        let reference = Reference::Path("/soc/i2c@40003000".to_string());

        gen.add_node_from_reference(reference, &[]);

        assert_eq!(gen.overlay_node_count(), 1);
        let rn = &gen.overlay_tree.reference_nodes[0];
        // Should fall back to path reference.
        assert_eq!(
            rn.reference,
            Reference::Path("/soc/i2c@40003000".to_string())
        );
    }

    #[test]
    fn delete_selected_node() {
        let mut gen = GeneratorState::new();
        gen.add_node_from_reference(Reference::Label("i2c1".to_string()), &[]);
        gen.add_node_from_reference(Reference::Label("spi1".to_string()), &[]);

        assert_eq!(gen.overlay_node_count(), 2);
        gen.selected_node = 0;
        gen.delete_selected_node();
        assert_eq!(gen.overlay_node_count(), 1);
        assert_eq!(
            gen.overlay_tree.reference_nodes[0].reference,
            Reference::Label("spi1".to_string())
        );
    }

    #[test]
    fn step_navigation() {
        let mut gen = GeneratorState::new();
        assert_eq!(gen.step, GeneratorStep::SelectBoard);

        gen.next_step();
        assert_eq!(gen.step, GeneratorStep::EditNodes);

        gen.next_step();
        assert_eq!(gen.step, GeneratorStep::SaveFile);

        gen.next_step();
        assert_eq!(gen.step, GeneratorStep::SaveFile); // doesn't go past last

        gen.prev_step();
        assert_eq!(gen.step, GeneratorStep::EditNodes);

        gen.prev_step();
        assert_eq!(gen.step, GeneratorStep::SelectBoard);

        gen.prev_step();
        assert_eq!(gen.step, GeneratorStep::SelectBoard); // doesn't go before first
    }

    #[test]
    fn build_overlay_string() {
        let mut gen = GeneratorState::new();
        gen.add_node_from_reference(Reference::Label("i2c1".to_string()), &[]);

        let output = gen.build_overlay_string();
        // /dts-v1/ and /plugin/ should NOT be included per user request.
        assert!(!output.contains("/dts-v1/;"));
        assert!(!output.contains("/plugin/;"));
        assert!(output.contains("// Generated by zdtwalk"));
        assert!(output.contains("&i2c1"));
        assert!(output.contains("status = \"okay\""));
    }

    #[test]
    fn new_node_via_input() {
        let mut gen = GeneratorState::new();
        gen.step = GeneratorStep::EditNodes;

        gen.start_new_node();
        assert_eq!(gen.input_mode, Some(InputMode::NodeReference));

        gen.push_char('i');
        gen.push_char('2');
        gen.push_char('c');
        gen.push_char('1');
        gen.confirm_input();

        assert_eq!(gen.overlay_node_count(), 1);
        assert_eq!(
            gen.overlay_tree.reference_nodes[0].reference,
            Reference::Label("i2c1".to_string())
        );
        assert!(gen.input_mode.is_none());
    }

    #[test]
    fn new_node_with_path_reference() {
        let mut gen = GeneratorState::new();
        gen.step = GeneratorStep::EditNodes;

        gen.start_new_node();
        for c in "/soc/i2c@40003000".chars() {
            gen.push_char(c);
        }
        gen.confirm_input();

        assert_eq!(gen.overlay_node_count(), 1);
        assert_eq!(
            gen.overlay_tree.reference_nodes[0].reference,
            Reference::Path("/soc/i2c@40003000".to_string())
        );
    }

    #[test]
    fn board_sync() {
        let mut gen = GeneratorState::new();
        assert!(gen.selected_board.is_none());

        gen.sync_board(Some("nrf52840dk"));
        assert_eq!(gen.selected_board.as_deref(), Some("nrf52840dk"));

        // Syncing with same name should not reset state.
        gen.board_resolving = true;
        gen.sync_board(Some("nrf52840dk"));
        assert!(gen.board_resolving);

        // Syncing with different name should reset state.
        gen.sync_board(Some("stm32f4_disco"));
        assert!(!gen.board_resolving);
        assert!(gen.resolved_board_tree.is_none());
    }

    #[test]
    fn toggle_expand_nodes() {
        let mut gen = GeneratorState::new();
        gen.step = GeneratorStep::EditNodes;
        gen.add_node_from_reference(Reference::Label("i2c1".to_string()), &[]);

        assert!(!gen.expanded_nodes.contains("0"));
        gen.toggle_expand();
        assert!(gen.expanded_nodes.contains("0"));
        gen.toggle_expand();
        assert!(!gen.expanded_nodes.contains("0"));
    }

    #[test]
    fn cancel_input() {
        let mut gen = GeneratorState::new();
        gen.start_new_node();
        gen.push_char('x');
        gen.cancel_input();

        assert!(gen.input_mode.is_none());
        assert!(gen.input_buffer.is_empty());
        assert_eq!(gen.overlay_node_count(), 0);
    }
}
