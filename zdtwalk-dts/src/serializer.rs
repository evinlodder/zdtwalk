use crate::model::*;

// ---------------------------------------------------------------------------
// Configuration
// ---------------------------------------------------------------------------

/// Controls how a [`DeviceTree`] is serialized to text.
#[derive(Debug, Clone)]
pub struct SerializerConfig {
    /// String used for one level of indentation (default: `"\t"`).
    pub indent: String,
    /// Optional comment block placed at the very top of the output.
    pub header_comment: Option<String>,
    /// Target output format.
    pub output_format: OutputFormat,
    /// Sort properties alphabetically within each node.
    pub sort_properties: bool,
    /// Sort child nodes alphabetically by full name.
    pub sort_nodes: bool,
    /// Whether to emit a `/dts-v1/;` version tag.
    pub include_version: bool,
}

impl Default for SerializerConfig {
    fn default() -> Self {
        Self {
            indent: "\t".to_string(),
            header_comment: None,
            output_format: OutputFormat::Dts,
            sort_properties: false,
            sort_nodes: false,
            include_version: true,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutputFormat {
    /// Standard `.dts` output.
    Dts,
    /// Header/include fragment (`.dtsi`).
    Dtsi,
    /// Overlay (`.overlay` / `.dtso`) – adds `/plugin/;` automatically.
    Overlay,
}

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// Serialize a [`DeviceTree`] into a string according to `config`.
pub fn serialize(tree: &DeviceTree, config: &SerializerConfig) -> String {
    let mut out = String::new();

    // -- header comment --
    if let Some(comment) = &config.header_comment {
        for line in comment.lines() {
            out.push_str("// ");
            out.push_str(line);
            out.push('\n');
        }
        out.push('\n');
    }

    // -- version tag --
    if config.include_version {
        if let Some(DtsVersion::V1) = tree.version {
            out.push_str("/dts-v1/;\n");
        }
    }

    // -- plugin --
    if matches!(config.output_format, OutputFormat::Overlay) || tree.is_plugin {
        out.push_str("/plugin/;\n");
    }

    // -- includes --
    for inc in &tree.includes {
        match inc.kind {
            IncludeKind::CPreprocessor => {
                out.push_str(&format!("#include \"{}\"\n", inc.path));
            }
            IncludeKind::DtsInclude => {
                out.push_str(&format!("/include/ \"{}\"\n", inc.path));
            }
        }
    }
    if !tree.includes.is_empty() {
        out.push('\n');
    }

    // -- memory reservations --
    for mr in &tree.memory_reservations {
        out.push_str(&format!(
            "/memreserve/ {:#x} {:#x};\n",
            mr.address, mr.size
        ));
    }
    if !tree.memory_reservations.is_empty() {
        out.push('\n');
    }

    // -- root node --
    if let Some(root) = &tree.root {
        write_labels(&mut out, &root.labels);
        out.push_str("/ ");
        write_node_body(&mut out, root, 0, config);
        out.push_str(";\n");
    }

    // -- reference node overrides --
    for rn in &tree.reference_nodes {
        out.push('\n');
        out.push_str(&rn.reference.to_string());
        out.push(' ');
        write_node_body(&mut out, &rn.node, 0, config);
        out.push_str(";\n");
    }

    out
}

/// Format a [`PropertyValue`] to a stand-alone string (useful for display).
pub fn format_property_value(value: &PropertyValue) -> String {
    let mut s = String::new();
    write_property_value(&mut s, value);
    s
}

// ---------------------------------------------------------------------------
// Internal helpers
// ---------------------------------------------------------------------------

fn write_labels(out: &mut String, labels: &[String]) {
    for label in labels {
        out.push_str(label);
        out.push_str(": ");
    }
}

fn write_node_body(out: &mut String, node: &Node, depth: usize, config: &SerializerConfig) {
    out.push_str("{\n");
    let indent = config.indent.repeat(depth + 1);

    // delete-property directives
    for name in &node.deleted_properties {
        out.push_str(&indent);
        out.push_str("/delete-property/ ");
        out.push_str(name);
        out.push_str(";\n");
    }

    // delete-node directives
    for name in &node.deleted_nodes {
        out.push_str(&indent);
        out.push_str("/delete-node/ ");
        out.push_str(name);
        out.push_str(";\n");
    }

    // properties
    let properties: Vec<&Property> = if config.sort_properties {
        let mut v: Vec<&Property> = node.properties.iter().collect();
        v.sort_by(|a, b| a.name.cmp(&b.name));
        v
    } else {
        node.properties.iter().collect()
    };

    for prop in &properties {
        out.push_str(&indent);
        write_labels(out, &prop.labels);
        out.push_str(&prop.name);
        if let Some(value) = &prop.value {
            out.push_str(" = ");
            write_property_value(out, value);
        }
        out.push_str(";\n");
    }

    // children
    let children: Vec<&Node> = if config.sort_nodes {
        let mut v: Vec<&Node> = node.children.iter().collect();
        v.sort_by(|a, b| a.full_name().cmp(&b.full_name()));
        v
    } else {
        node.children.iter().collect()
    };

    for (i, child) in children.iter().enumerate() {
        if i > 0 || !properties.is_empty() || !node.deleted_properties.is_empty() {
            out.push('\n');
        }
        out.push_str(&indent);
        write_labels(out, &child.labels);
        out.push_str(&child.full_name());
        out.push(' ');
        write_node_body(out, child, depth + 1, config);
        out.push_str(";\n");
    }

    let parent_indent = config.indent.repeat(depth);
    out.push_str(&parent_indent);
    out.push('}');
}

fn write_property_value(out: &mut String, value: &PropertyValue) {
    for (i, part) in value.0.iter().enumerate() {
        if i > 0 {
            out.push_str(", ");
        }
        match part {
            ValuePart::StringLiteral(s) => {
                out.push('"');
                out.push_str(&escape_string(s));
                out.push('"');
            }
            ValuePart::CellArray(cells) => {
                out.push('<');
                for (j, cell) in cells.iter().enumerate() {
                    if j > 0 {
                        out.push(' ');
                    }
                    match cell {
                        Cell::Literal(n) => {
                            if *n > 9 {
                                out.push_str(&format!("{:#x}", n));
                            } else {
                                out.push_str(&format!("{}", n));
                            }
                        }
                        Cell::Reference(r) => out.push_str(&r.to_string()),
                        Cell::Expression(e) => {
                            out.push('(');
                            out.push_str(e);
                            out.push(')');
                        }
                        Cell::Macro(name, args) => {
                            out.push_str(name);
                            out.push('(');
                            out.push_str(args);
                            out.push(')');
                        }
                    }
                }
                out.push('>');
            }
            ValuePart::ByteString(bytes) => {
                out.push('[');
                for (j, byte) in bytes.iter().enumerate() {
                    if j > 0 {
                        out.push(' ');
                    }
                    out.push_str(&format!("{:02x}", byte));
                }
                out.push(']');
            }
            ValuePart::Reference(r) => {
                out.push_str(&r.to_string());
            }
        }
    }
}

fn escape_string(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '\n' => out.push_str("\\n"),
            '\t' => out.push_str("\\t"),
            '\r' => out.push_str("\\r"),
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '\0' => out.push_str("\\0"),
            c => out.push(c),
        }
    }
    out
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::parse_dts;

    #[test]
    fn round_trip_minimal() {
        let src = "/dts-v1/;\n/ {\n};\n";
        let tree = parse_dts(src).unwrap();
        let config = SerializerConfig::default();
        let output = serialize(&tree, &config);
        // re-parse
        parse_dts(&output).unwrap();
    }

    #[test]
    fn overlay_adds_plugin() {
        let src = "/dts-v1/;\n/ {\n\tcompat = \"x\";\n};\n";
        let tree = parse_dts(src).unwrap();
        let config = SerializerConfig {
            output_format: OutputFormat::Overlay,
            header_comment: Some("Generated by zdtwalk.".to_string()),
            ..Default::default()
        };
        let output = serialize(&tree, &config);
        assert!(output.contains("/plugin/;"));
        assert!(output.contains("// Generated by zdtwalk."));
    }

    #[test]
    fn sorted_output() {
        let src = r#"/dts-v1/;
/ {
    z-prop = "z";
    a-prop = "a";

    z-node {
    };

    a-node {
    };
};
"#;
        let tree = parse_dts(src).unwrap();
        let config = SerializerConfig {
            sort_properties: true,
            sort_nodes: true,
            ..Default::default()
        };
        let output = serialize(&tree, &config);
        let a_prop_pos = output.find("a-prop").unwrap();
        let z_prop_pos = output.find("z-prop").unwrap();
        assert!(a_prop_pos < z_prop_pos, "properties should be sorted");

        let a_node_pos = output.find("a-node").unwrap();
        let z_node_pos = output.find("z-node").unwrap();
        assert!(a_node_pos < z_node_pos, "nodes should be sorted");
    }
}
