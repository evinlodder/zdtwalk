use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use ratatui::{
    prelude::*,
    widgets::{Block, Borders, Paragraph, Wrap},
};

use crate::dts::{
    self, Binding, DeviceTree, DtsVersion, Node, OutputFormat, Property, Reference, ReferenceNode,
    SerializerConfig,
};

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
    FileName,
}

// ---------------------------------------------------------------------------
// PropertyEditState
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct PropertyEditState {
    pub node_idx: usize,
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
    pub expanded_nodes: HashSet<usize>,
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
        }
    }

    // ------------------------------------------------------------------
    // Panel toggle
    // ------------------------------------------------------------------

    pub fn toggle_collapsed(&mut self) {
        self.collapsed = !self.collapsed;
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
        let count = self.overlay_node_count();
        if count == 0 {
            return;
        }
        let idx = self.selected_node.min(count - 1);
        self.overlay_tree.reference_nodes.remove(idx);
        self.expanded_nodes.remove(&idx);
        // Shift any expanded indices above the removed one
        let shifted: HashSet<usize> = self
            .expanded_nodes
            .iter()
            .map(|&i| if i > idx { i - 1 } else { i })
            .collect();
        self.expanded_nodes = shifted;
        let new_count = self.overlay_node_count();
        if new_count == 0 {
            self.selected_node = 0;
        } else if self.selected_node >= new_count {
            self.selected_node = new_count - 1;
        }
    }

    pub fn delete_selected_property(&mut self) {
        let count = self.overlay_node_count();
        if count == 0 {
            return;
        }
        let node_idx = self.selected_node.min(count - 1);
        if !self.expanded_nodes.contains(&node_idx) {
            return;
        }
        // Find which property is "selected" — we use a simple heuristic:
        // if editing_property is active, delete that; otherwise do nothing.
        if let Some(edit) = self.editing_property.take() {
            if edit.node_idx < self.overlay_tree.reference_nodes.len() {
                let node = &mut self.overlay_tree.reference_nodes[edit.node_idx].node;
                if edit.prop_idx < node.properties.len() {
                    node.properties.remove(edit.prop_idx);
                }
            }
        }
    }

    pub fn get_binding_for_node<'a>(&'a self, node: &Node) -> Option<&'a Binding> {
        let compat = node.property("compatible")?.as_string()?;
        self.board_bindings.get(compat)
    }

    // ------------------------------------------------------------------
    // Cursor / scroll navigation
    // ------------------------------------------------------------------

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
                let count = self.overlay_node_count();
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
        if self.selected_node < self.overlay_node_count() {
            if self.expanded_nodes.contains(&self.selected_node) {
                self.expanded_nodes.remove(&self.selected_node);
            } else {
                self.expanded_nodes.insert(self.selected_node);
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
        self.input_mode = Some(InputMode::PropertyName);
        self.input_buffer.clear();
    }

    pub fn start_edit_property(&mut self) {
        let count = self.overlay_node_count();
        if count == 0 {
            return;
        }
        let node_idx = self.selected_node.min(count - 1);
        let node = &self.overlay_tree.reference_nodes[node_idx].node;
        if node.properties.is_empty() {
            return;
        }
        let prop_idx = 0; // default to first property
        let prop = &node.properties[prop_idx];
        let value = match &prop.value {
            Some(v) => dts::format_property_value(v),
            None => String::new(),
        };
        self.editing_property = Some(PropertyEditState {
            node_idx,
            prop_idx,
            name: prop.name.clone(),
            value: value.clone(),
        });
        self.input_mode = Some(InputMode::PropertyValue);
        self.input_buffer = value;
    }

    pub fn start_child_node(&mut self) {
        self.input_mode = Some(InputMode::ChildName);
        self.input_buffer.clear();
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
                    let count = self.overlay_node_count();
                    if count > 0 {
                        let idx = self.selected_node.min(count - 1);
                        let child = Node::new(buf);
                        self.overlay_tree.reference_nodes[idx]
                            .node
                            .children
                            .push(child);
                    }
                }
            }
            InputMode::PropertyName => {
                if !buf.is_empty() {
                    // Store the name temporarily and switch to value input
                    self.input_mode = Some(InputMode::PropertyValue);
                    let count = self.overlay_node_count();
                    if count > 0 {
                        let node_idx = self.selected_node.min(count - 1);
                        let node = &self.overlay_tree.reference_nodes[node_idx].node;
                        self.editing_property = Some(PropertyEditState {
                            node_idx,
                            prop_idx: node.properties.len(),
                            name: buf,
                            value: String::new(),
                        });
                    }
                    self.input_buffer.clear();
                    return; // stay in input mode
                }
            }
            InputMode::PropertyValue => {
                if let Some(edit) = self.editing_property.take() {
                    if edit.node_idx < self.overlay_tree.reference_nodes.len() {
                        let node = &mut self.overlay_tree.reference_nodes[edit.node_idx].node;
                        let prop = if buf.is_empty() {
                            Property::new_boolean(&edit.name)
                        } else {
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
            output_format: OutputFormat::Overlay,
            header_comment: Some("Generated by zdtwalk".to_string()),
            ..Default::default()
        };
        dts::serialize(&self.overlay_tree, &config)
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
        let border_style = if is_active {
            Style::default().fg(Color::Cyan)
        } else {
            Style::default().fg(Color::DarkGray)
        };

        let block = Block::default()
            .title(" DTS Generator (g to toggle) ")
            .borders(Borders::ALL)
            .border_style(border_style);

        let inner = block.inner(area);
        frame.render_widget(block, area);

        match self.step {
            GeneratorStep::SelectBoard => self.render_select_board(frame, inner),
            GeneratorStep::EditNodes => self.render_edit_nodes(frame, inner),
            GeneratorStep::SaveFile => self.render_save_file(frame, inner),
        }
    }

    // ---- SelectBoard -------------------------------------------------

    fn render_select_board(&self, frame: &mut Frame, area: Rect) {
        let mut lines: Vec<Line> = Vec::new();

        lines.push(Line::from(Span::styled(
            "Step 1: Select Board",
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        )));
        lines.push(Line::from(""));

        match &self.selected_board {
            Some(name) => {
                lines.push(Line::from(vec![
                    Span::styled("Board: ", Style::default().fg(Color::Gray)),
                    Span::styled(name.as_str(), Style::default().fg(Color::Green)),
                ]));

                let status = if self.board_resolving {
                    Span::styled("Resolving...", Style::default().fg(Color::Yellow))
                } else if self.resolved_board_tree.is_some() {
                    Span::styled("Resolved ✓", Style::default().fg(Color::Green))
                } else {
                    Span::styled("Not resolved", Style::default().fg(Color::DarkGray))
                };
                lines.push(Line::from(vec![
                    Span::styled("Status: ", Style::default().fg(Color::Gray)),
                    status,
                ]));
            }
            None => {
                lines.push(Line::from(Span::styled(
                    "No board selected",
                    Style::default().fg(Color::DarkGray),
                )));
                lines.push(Line::from(Span::styled(
                    "Select a board in the left panel",
                    Style::default().fg(Color::DarkGray),
                )));
            }
        }

        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled(
            "Press → or Enter to continue",
            Style::default().fg(Color::DarkGray),
        )));

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

        lines.push(Line::from(Span::styled(
            "Step 2: Edit Overlay Nodes",
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        )));
        lines.push(Line::from(""));

        if self.overlay_tree.reference_nodes.is_empty() {
            lines.push(Line::from(Span::styled(
                "No nodes yet. Press 'a' to add from viewer or 'n' for new node.",
                Style::default().fg(Color::DarkGray),
            )));
        } else {
            for (idx, rn) in self.overlay_tree.reference_nodes.iter().enumerate() {
                let is_selected = idx == self.selected_node;
                let is_expanded = self.expanded_nodes.contains(&idx);

                let ref_str = match &rn.reference {
                    Reference::Label(l) => format!("&{l}"),
                    Reference::Path(p) => format!("&{{{p}}}"),
                };

                let marker = if is_expanded { "▼" } else { "▶" };
                let header = format!("{marker} {ref_str} {{ ... }}");

                let style = if is_selected {
                    Style::default().fg(Color::Cyan).bg(Color::DarkGray)
                } else {
                    Style::default().fg(Color::White)
                };
                lines.push(Line::from(Span::styled(header, style)));

                if is_expanded {
                    for prop in &rn.node.properties {
                        let val_str = match &prop.value {
                            Some(v) => format!(" = {}", dts::format_property_value(v)),
                            None => String::new(),
                        };
                        let prop_line = format!("    {}{};", prop.name, val_str);
                        lines.push(Line::from(Span::styled(
                            prop_line,
                            Style::default().fg(Color::Gray),
                        )));
                    }
                    for child in &rn.node.children {
                        let child_line = format!("    {} {{ ... }}", child.full_name());
                        lines.push(Line::from(Span::styled(
                            child_line,
                            Style::default().fg(Color::Blue),
                        )));
                    }
                }
            }
        }

        // Input prompt
        if let Some(mode) = &self.input_mode {
            lines.push(Line::from(""));
            let label = match mode {
                InputMode::NodeReference => "Reference (e.g. &i2c1): ",
                InputMode::ChildName => "Child name: ",
                InputMode::PropertyName => "Property name: ",
                InputMode::PropertyValue => "Property value: ",
                InputMode::FileName => "Filename: ",
            };
            lines.push(Line::from(vec![
                Span::styled(label, Style::default().fg(Color::Yellow)),
                Span::styled(
                    &self.input_buffer,
                    Style::default()
                        .fg(Color::White)
                        .add_modifier(Modifier::UNDERLINED),
                ),
            ]));
        }

        // Hints
        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled(
            "a:add from viewer  n:new node  d:delete  Enter:edit/expand  ←/→:step",
            Style::default().fg(Color::DarkGray),
        )));

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

        lines.push(Line::from(Span::styled(
            "Step 3: Save Overlay",
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        )));
        lines.push(Line::from(""));

        // Current directory
        let dir_display = self.save_dir.display().to_string();
        lines.push(Line::from(vec![
            Span::styled("Dir: ", Style::default().fg(Color::Gray)),
            Span::styled(dir_display, Style::default().fg(Color::Blue)),
        ]));
        lines.push(Line::from(""));

        if self.save_entries.is_empty() {
            lines.push(Line::from(Span::styled(
                "(empty directory)",
                Style::default().fg(Color::DarkGray),
            )));
        } else {
            let scroll = self.save_scroll;
            for (i, entry) in self.save_entries.iter().enumerate().skip(scroll) {
                let is_selected = i == self.save_selected;
                let is_dir = self.save_dir.join(entry).is_dir();
                let prefix = if is_dir { "📁 " } else { "📄 " };
                let text = format!("{prefix}{entry}");
                let style = if is_selected {
                    Style::default().fg(Color::Cyan).bg(Color::DarkGray)
                } else if is_dir {
                    Style::default().fg(Color::Blue)
                } else {
                    Style::default().fg(Color::White)
                };
                lines.push(Line::from(Span::styled(text, style)));
            }
        }

        lines.push(Line::from(""));

        // Filename input area
        if self.save_input_active {
            lines.push(Line::from(vec![
                Span::styled("Filename: ", Style::default().fg(Color::Yellow)),
                Span::styled(
                    &self.save_input,
                    Style::default()
                        .fg(Color::White)
                        .add_modifier(Modifier::UNDERLINED),
                ),
            ]));
        }

        if self.confirm_overwrite {
            lines.push(Line::from(Span::styled(
                "File exists! Press Enter to overwrite, Esc to cancel.",
                Style::default().fg(Color::Red),
            )));
        }

        if let Some(path) = &self.save_path {
            lines.push(Line::from(vec![
                Span::styled("Save to: ", Style::default().fg(Color::Gray)),
                Span::styled(
                    path.display().to_string(),
                    Style::default().fg(Color::Green),
                ),
            ]));
        }

        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled(
            "Enter:select  Backspace:up dir  n:new file  ←/→:step",
            Style::default().fg(Color::DarkGray),
        )));

        let visible: Vec<Line> = lines.into_iter().take(height).collect();
        let paragraph = Paragraph::new(visible);
        frame.render_widget(paragraph, area);
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dts::parser::parse_dts;

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
        assert!(output.contains("/dts-v1/;"));
        assert!(output.contains("/plugin/;"));
        assert!(output.contains("// Generated by zdtwalk"));
        assert!(output.contains("&i2c1"));
        assert!(output.contains("status = \"okay\""));

        // Verify round-trip parsing.
        let reparsed = parse_dts(&output).unwrap();
        assert!(reparsed.is_plugin);
        assert_eq!(reparsed.reference_nodes.len(), 1);
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

        assert!(!gen.expanded_nodes.contains(&0));
        gen.toggle_expand();
        assert!(gen.expanded_nodes.contains(&0));
        gen.toggle_expand();
        assert!(!gen.expanded_nodes.contains(&0));
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
