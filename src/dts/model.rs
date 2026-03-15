use std::fmt;

/// A complete device tree source file representation.
#[derive(Debug, Clone, PartialEq)]
pub struct DeviceTree {
    /// DTS version (typically V1).
    pub version: Option<DtsVersion>,
    /// Whether this is a plugin/overlay (`/plugin/;`).
    pub is_plugin: bool,
    /// Memory reservation entries (`/memreserve/`).
    pub memory_reservations: Vec<MemoryReservation>,
    /// Include directives found in source.
    pub includes: Vec<Include>,
    /// The root node (`/ { ... };`), if present.
    pub root: Option<Node>,
    /// Reference-based node overrides (`&label { ... };`).
    pub reference_nodes: Vec<ReferenceNode>,
}

impl DeviceTree {
    pub fn new() -> Self {
        Self {
            version: None,
            is_plugin: false,
            memory_reservations: Vec::new(),
            includes: Vec::new(),
            root: None,
            reference_nodes: Vec::new(),
        }
    }
}

impl Default for DeviceTree {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// Version
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DtsVersion {
    V1,
}

impl fmt::Display for DtsVersion {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DtsVersion::V1 => write!(f, "v1"),
        }
    }
}

// ---------------------------------------------------------------------------
// Memory reservation
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MemoryReservation {
    pub address: u64,
    pub size: u64,
}

// ---------------------------------------------------------------------------
// Includes
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Include {
    pub path: String,
    pub kind: IncludeKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IncludeKind {
    /// `/include/ "file"`
    DtsInclude,
    /// `#include "file"` or `#include <file>`
    CPreprocessor,
}

// ---------------------------------------------------------------------------
// References
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Reference {
    /// `&label`
    Label(String),
    /// `&{/path/to/node}`
    Path(String),
}

impl fmt::Display for Reference {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Reference::Label(l) => write!(f, "&{}", l),
            Reference::Path(p) => write!(f, "&{{{}}}", p),
        }
    }
}

// ---------------------------------------------------------------------------
// Reference node override
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq)]
pub struct ReferenceNode {
    pub reference: Reference,
    pub node: Node,
}

// ---------------------------------------------------------------------------
// Node
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq)]
pub struct Node {
    pub name: String,
    pub unit_address: Option<String>,
    pub labels: Vec<String>,
    pub properties: Vec<Property>,
    pub children: Vec<Node>,
    pub deleted_properties: Vec<String>,
    pub deleted_nodes: Vec<String>,
}

impl Node {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            unit_address: None,
            labels: Vec::new(),
            properties: Vec::new(),
            children: Vec::new(),
            deleted_properties: Vec::new(),
            deleted_nodes: Vec::new(),
        }
    }

    /// Return `name@unit_address` (or just `name` when there is no unit address).
    pub fn full_name(&self) -> String {
        match &self.unit_address {
            Some(addr) => format!("{}@{}", self.name, addr),
            None => self.name.clone(),
        }
    }

    /// Find a property by name.
    pub fn property(&self, name: &str) -> Option<&Property> {
        self.properties.iter().find(|p| p.name == name)
    }

    /// Find a direct child node by node name (ignoring unit address).
    pub fn child(&self, name: &str) -> Option<&Node> {
        self.children.iter().find(|c| c.name == name)
    }

    /// Find a direct child by full name (`name@unit_address`).
    pub fn child_by_full_name(&self, full_name: &str) -> Option<&Node> {
        self.children.iter().find(|c| c.full_name() == full_name)
    }

    /// Collect all nodes (recursively) that carry the given label.
    pub fn find_by_label(&self, label: &str) -> Vec<&Node> {
        let mut results = Vec::new();
        if self.labels.iter().any(|l| l == label) {
            results.push(self);
        }
        for child in &self.children {
            results.extend(child.find_by_label(label));
        }
        results
    }

    /// Walk every node depth-first, calling `f(node, depth)`.
    pub fn walk<F: FnMut(&Node, usize)>(&self, f: &mut F, depth: usize) {
        f(self, depth);
        for child in &self.children {
            child.walk(f, depth + 1);
        }
    }
}

impl Default for Node {
    fn default() -> Self {
        Self::new("")
    }
}

// ---------------------------------------------------------------------------
// Property
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq)]
pub struct Property {
    pub name: String,
    pub value: Option<PropertyValue>,
    pub labels: Vec<String>,
}

impl Property {
    pub fn new_boolean(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            value: None,
            labels: Vec::new(),
        }
    }

    pub fn new_string(name: impl Into<String>, val: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            value: Some(PropertyValue(vec![ValuePart::StringLiteral(val.into())])),
            labels: Vec::new(),
        }
    }

    pub fn new_cells(name: impl Into<String>, cells: Vec<Cell>) -> Self {
        Self {
            name: name.into(),
            value: Some(PropertyValue(vec![ValuePart::CellArray(cells)])),
            labels: Vec::new(),
        }
    }

    pub fn is_boolean(&self) -> bool {
        self.value.is_none()
    }

    /// Try to get the value as a single string.
    pub fn as_string(&self) -> Option<&str> {
        let val = self.value.as_ref()?;
        if val.0.len() == 1 {
            if let ValuePart::StringLiteral(s) = &val.0[0] {
                return Some(s);
            }
        }
        None
    }

    /// Try to get the value as a string list (comma-separated strings).
    pub fn as_string_list(&self) -> Option<Vec<&str>> {
        let val = self.value.as_ref()?;
        let mut strings = Vec::new();
        for part in &val.0 {
            if let ValuePart::StringLiteral(s) = part {
                strings.push(s.as_str());
            } else {
                return None;
            }
        }
        Some(strings)
    }

    /// Try to get the value as a flat list of u64 cell values (no references / expressions).
    pub fn as_u64_cells(&self) -> Option<Vec<u64>> {
        let val = self.value.as_ref()?;
        if val.0.len() != 1 {
            return None;
        }
        if let ValuePart::CellArray(cells) = &val.0[0] {
            let mut nums = Vec::new();
            for cell in cells {
                if let Cell::Literal(n) = cell {
                    nums.push(*n);
                } else {
                    return None;
                }
            }
            return Some(nums);
        }
        None
    }
}

// ---------------------------------------------------------------------------
// Property values
// ---------------------------------------------------------------------------

/// A property value consisting of one or more comma-separated parts.
///
/// Example: `"hello", <0x1 0x2>, [FF 00]`
#[derive(Debug, Clone, PartialEq)]
pub struct PropertyValue(pub Vec<ValuePart>);

#[derive(Debug, Clone, PartialEq)]
pub enum ValuePart {
    /// `"string"`
    StringLiteral(String),
    /// `<cell1 cell2 ...>`
    CellArray(Vec<Cell>),
    /// `[byte1 byte2 ...]`
    ByteString(Vec<u8>),
    /// `&label` or `&{/path}`
    Reference(Reference),
}

#[derive(Debug, Clone, PartialEq)]
pub enum Cell {
    /// A numeric literal (decimal or hex).
    Literal(u64),
    /// A phandle reference (`&label` or `&{/path}`).
    Reference(Reference),
    /// A C-style expression in parentheses, stored verbatim.
    Expression(String),
    /// A C macro invocation, e.g. `STM32_PINMUX('A', 0, ANALOG)`.
    /// Stored as `(name, args_verbatim)`.
    Macro(String, String),
}
