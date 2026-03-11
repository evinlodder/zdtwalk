use nom::{
    branch::alt,
    bytes::complete::{tag, take_until, take_while, take_while1},
    character::complete::{char, multispace1, none_of, satisfy},
    combinator::{cut, map, opt, peek, recognize, value},
    multi::many0,
    sequence::{pair, preceded, terminated, tuple},
    IResult,
};

use crate::error::{ParseError, ParseErrorKind};
use crate::model::*;

type PResult<'a, T> = IResult<&'a str, T>;

// ===================================================================
// Public entry point
// ===================================================================

/// Parse a complete DTS / DTSI source string into a [`DeviceTree`].
pub fn parse_dts(input: &str) -> Result<DeviceTree, ParseError> {
    match parse_file(input) {
        Ok((remaining, tree)) => {
            let trimmed = remaining.trim();
            if trimmed.is_empty() {
                Ok(tree)
            } else {
                // Find where the leftover starts in the original source.
                let (line, column, context_line) =
                    crate::error::location_in(input, trimmed);
                Err(ParseError {
                    line,
                    column,
                    kind: ParseErrorKind::TrailingInput,
                    context_line,
                })
            }
        }
        Err(e) => {
            // nom errors carry the remaining input slice; use it for location.
            let remaining = match &e {
                nom::Err::Error(inner) | nom::Err::Failure(inner) => inner.input,
                nom::Err::Incomplete(_) => input,
            };
            let (line, column, context_line) =
                crate::error::location_in(input, remaining);
            let msg = match &e {
                nom::Err::Error(inner) | nom::Err::Failure(inner) => {
                    friendly_error_message(inner.code, remaining)
                }
                nom::Err::Incomplete(n) => format!("incomplete input: {:?}", n),
            };
            Err(ParseError {
                line,
                column,
                kind: ParseErrorKind::Syntax(msg),
                context_line,
            })
        }
    }
}

// ===================================================================
// Whitespace & comments
// ===================================================================

fn line_comment(input: &str) -> PResult<'_, ()> {
    value(
        (),
        pair(
            tag("//"),
            take_while(|c: char| c != '\n' && c != '\r'),
        ),
    )(input)
}

fn block_comment(input: &str) -> PResult<'_, ()> {
    value((), tuple((tag("/*"), take_until("*/"), tag("*/"))))(input)
}

/// Consume any amount of whitespace and comments (including zero).
fn ws(input: &str) -> PResult<'_, ()> {
    value(
        (),
        many0(alt((value((), multispace1), line_comment, block_comment))),
    )(input)
}

/// Helper: skip whitespace/comments, then apply `f`.
fn ws_before<'a, F, O>(f: F) -> impl FnMut(&'a str) -> PResult<'a, O>
where
    F: FnMut(&'a str) -> PResult<'a, O>,
{
    preceded(ws, f)
}

// ===================================================================
// File-level grammar
// ===================================================================

#[derive(Debug)]
#[allow(dead_code)]
enum TopLevelItem {
    Version(DtsVersion),
    Plugin,
    Include(Include),
    MemReserve(MemoryReservation),
    RootNode(Node),
    ReferenceNode(ReferenceNode),
    DeleteNode(String),
    /// Unrecognised preprocessor line – silently skipped.
    Preprocessor,
}

fn parse_file(input: &str) -> PResult<'_, DeviceTree> {
    let (input, _) = ws(input)?;
    let (input, items) = many0(ws_before(parse_top_level_item))(input)?;
    let (input, _) = ws(input)?;

    let mut tree = DeviceTree::new();
    for item in items {
        match item {
            TopLevelItem::Version(v) => tree.version = Some(v),
            TopLevelItem::Plugin => tree.is_plugin = true,
            TopLevelItem::Include(inc) => tree.includes.push(inc),
            TopLevelItem::MemReserve(mr) => tree.memory_reservations.push(mr),
            TopLevelItem::RootNode(node) => {
                if let Some(ref mut existing) = tree.root {
                    merge_nodes(existing, node);
                } else {
                    tree.root = Some(node);
                }
            }
            TopLevelItem::ReferenceNode(rn) => tree.reference_nodes.push(rn),
            TopLevelItem::DeleteNode(_) | TopLevelItem::Preprocessor => {}
        }
    }

    Ok((input, tree))
}

fn parse_top_level_item(input: &str) -> PResult<'_, TopLevelItem> {
    alt((
        map(parse_version_tag, TopLevelItem::Version),
        map(parse_plugin_tag, |_| TopLevelItem::Plugin),
        map(parse_c_include, TopLevelItem::Include),
        map(parse_dts_include, TopLevelItem::Include),
        map(parse_memreserve, TopLevelItem::MemReserve),
        map(parse_root_node, TopLevelItem::RootNode),
        map(parse_reference_node, TopLevelItem::ReferenceNode),
        map(parse_top_level_delete_node, TopLevelItem::DeleteNode),
        map(parse_other_preprocessor, |_| TopLevelItem::Preprocessor),
    ))(input)
}

// ===================================================================
// Directives
// ===================================================================

fn parse_version_tag(input: &str) -> PResult<'_, DtsVersion> {
    value(DtsVersion::V1, pair(tag("/dts-v1/"), ws_before(char(';'))))(input)
}

fn parse_plugin_tag(input: &str) -> PResult<'_, ()> {
    value((), pair(tag("/plugin/"), ws_before(char(';'))))(input)
}

fn parse_c_include(input: &str) -> PResult<'_, Include> {
    let (input, _) = char('#')(input)?;
    let (input, _) = take_while(|c: char| c == ' ' || c == '\t')(input)?;
    let (input, _) = tag("include")(input)?;
    let (input, _) = take_while(|c: char| c == ' ' || c == '\t')(input)?;
    let (input, path) = alt((
        delimited_string,
        map(
            tuple((char('<'), take_until(">"), char('>'))),
            |(_, p, _): (char, &str, char)| p.to_string(),
        ),
    ))(input)?;

    Ok((
        input,
        Include {
            path,
            kind: IncludeKind::CPreprocessor,
        },
    ))
}

fn parse_dts_include(input: &str) -> PResult<'_, Include> {
    let (input, _) = tag("/include/")(input)?;
    let (input, _) = ws(input)?;
    let (input, path) = delimited_string(input)?;
    Ok((
        input,
        Include {
            path,
            kind: IncludeKind::DtsInclude,
        },
    ))
}

fn parse_memreserve(input: &str) -> PResult<'_, MemoryReservation> {
    let (input, _) = tag("/memreserve/")(input)?;
    let (input, _) = ws(input)?;
    let (input, address) = parse_integer(input)?;
    let (input, _) = ws(input)?;
    let (input, size) = parse_integer(input)?;
    let (input, _) = ws(input)?;
    let (input, _) = char(';')(input)?;
    Ok((input, MemoryReservation { address, size }))
}

/// Skip a preprocessor directive we don't specifically handle
/// (`#define`, `#ifdef`, …).  Consumes through end of line.
fn parse_other_preprocessor(input: &str) -> PResult<'_, ()> {
    let (input, _) = char('#')(input)?;
    let (input, _) = take_while(|c: char| c != '\n')(input)?;
    Ok((input, ()))
}

// ===================================================================
// Nodes
// ===================================================================

fn parse_root_node(input: &str) -> PResult<'_, Node> {
    let (input, labels) = many0(terminated(parse_label_def, ws))(input)?;
    let (input, _) = char('/')(input)?;
    // Distinguish from directives like /dts-v1/, /plugin/, etc.
    let (input, _) = peek(ws_before(char('{')))(input)?;
    let (input, _) = ws(input)?;
    let (input, _) = char('{')(input)?;
    // Committed: we know this is a root node.
    let (input, mut node) = cut(parse_node_body)(input)?;
    let (input, _) = cut(ws_before(char('}')))(input)?;
    let (input, _) = cut(ws_before(char(';')))(input)?;

    node.name = String::new();
    node.labels = labels;
    Ok((input, node))
}

fn parse_reference_node(input: &str) -> PResult<'_, ReferenceNode> {
    let (input, reference) = parse_reference(input)?;
    let (input, _) = ws(input)?;
    let (input, _) = char('{')(input)?;
    // Committed: we know this is a reference node.
    let (input, node) = cut(parse_node_body)(input)?;
    let (input, _) = cut(ws_before(char('}')))(input)?;
    let (input, _) = cut(ws_before(char(';')))(input)?;
    Ok((input, ReferenceNode { reference, node }))
}

fn parse_top_level_delete_node(input: &str) -> PResult<'_, String> {
    let (input, _) = tag("/delete-node/")(input)?;
    let (input, _) = ws(input)?;
    let (input, name) = alt((
        map(parse_reference, |r| r.to_string()),
        map(parse_node_name, |s| s.to_string()),
    ))(input)?;
    let (input, _) = ws(input)?;
    let (input, _) = char(';')(input)?;
    Ok((input, name))
}

// ===================================================================
// Node body (children + properties)
// ===================================================================

#[derive(Debug)]
enum NodeItem {
    Property(Property),
    ChildNode(Node),
    DeleteProperty(String),
    DeleteNode(String),
}

fn parse_node_body(input: &str) -> PResult<'_, Node> {
    let (input, items) = many0(ws_before(parse_node_item))(input)?;

    let mut node = Node::new("");
    for item in items {
        match item {
            NodeItem::Property(p) => node.properties.push(p),
            NodeItem::ChildNode(c) => node.children.push(c),
            NodeItem::DeleteProperty(n) => node.deleted_properties.push(n),
            NodeItem::DeleteNode(n) => node.deleted_nodes.push(n),
        }
    }
    Ok((input, node))
}

fn parse_node_item(input: &str) -> PResult<'_, NodeItem> {
    alt((
        map(parse_delete_property, NodeItem::DeleteProperty),
        map(parse_delete_node_inner, NodeItem::DeleteNode),
        map(parse_child_node, NodeItem::ChildNode),
        map(parse_property, NodeItem::Property),
    ))(input)
}

fn parse_delete_property(input: &str) -> PResult<'_, String> {
    let (input, _) = tag("/delete-property/")(input)?;
    let (input, _) = ws(input)?;
    let (input, name) = parse_property_name(input)?;
    let (input, _) = ws(input)?;
    let (input, _) = char(';')(input)?;
    Ok((input, name.to_string()))
}

fn parse_delete_node_inner(input: &str) -> PResult<'_, String> {
    let (input, _) = tag("/delete-node/")(input)?;
    let (input, _) = ws(input)?;
    let (input, name) = parse_node_name(input)?;
    let (input, _) = ws(input)?;
    let (input, _) = char(';')(input)?;
    Ok((input, name.to_string()))
}

fn parse_child_node(input: &str) -> PResult<'_, Node> {
    let (input, _) = opt(terminated(tag("/omit-if-no-ref/"), ws))(input)?;
    let (input, labels) = many0(terminated(parse_label_def, ws))(input)?;
    let (input, name) = parse_node_name(input)?;
    let (input, unit_addr) = opt(preceded(char('@'), parse_unit_address))(input)?;
    let (input, _) = ws(input)?;
    let (input, _) = char('{')(input)?;
    // Committed: we know this is a child node.
    let (input, mut node) = cut(parse_node_body)(input)?;
    let (input, _) = cut(ws_before(char('}')))(input)?;
    let (input, _) = cut(ws_before(char(';')))(input)?;

    node.name = name.to_string();
    node.unit_address = unit_addr.map(|s| s.to_string());
    node.labels = labels;
    Ok((input, node))
}

fn parse_property(input: &str) -> PResult<'_, Property> {
    let (input, labels) = many0(terminated(parse_label_def, ws))(input)?;
    let (input, name) = parse_property_name(input)?;
    let (input, _) = ws(input)?;

    // If we see `=`, we're committed to a property with a value.
    let (input, value) = if input.starts_with('=') {
        let (input, _) = char('=')(input)?;
        let (input, _) = ws(input)?;
        let (input, v) = cut(parse_property_value)(input)?;
        let (input, _) = cut(ws_before(char(';')))(input)?;
        (input, Some(v))
    } else {
        // Boolean property – just `;`.
        let (input, _) = char(';')(input)?;
        (input, None)
    };

    Ok((
        input,
        Property {
            name: name.to_string(),
            value,
            labels,
        },
    ))
}

// ===================================================================
// Property values
// ===================================================================

fn parse_property_value(input: &str) -> PResult<'_, PropertyValue> {
    let (input, first) = parse_value_part(input)?;
    let (input, mut rest) = many0(preceded(ws_before(char(',')), ws_before(parse_value_part)))(input)?;

    let mut parts = vec![first];
    parts.append(&mut rest);
    Ok((input, PropertyValue(parts)))
}

fn parse_value_part(input: &str) -> PResult<'_, ValuePart> {
    alt((
        map(parse_string_literal, ValuePart::StringLiteral),
        map(parse_cell_array, ValuePart::CellArray),
        map(parse_byte_string, ValuePart::ByteString),
        map(parse_reference, ValuePart::Reference),
    ))(input)
}

// ===================================================================
// Strings
// ===================================================================

fn parse_string_literal(input: &str) -> PResult<'_, String> {
    let (input, _) = char('"')(input)?;
    let mut result = String::new();
    let mut remaining = input;

    loop {
        match remaining.chars().next() {
            Some('"') => {
                remaining = &remaining[1..];
                break;
            }
            Some('\\') => {
                remaining = &remaining[1..];
                match remaining.chars().next() {
                    Some('n') => {
                        result.push('\n');
                        remaining = &remaining[1..];
                    }
                    Some('t') => {
                        result.push('\t');
                        remaining = &remaining[1..];
                    }
                    Some('r') => {
                        result.push('\r');
                        remaining = &remaining[1..];
                    }
                    Some('\\') => {
                        result.push('\\');
                        remaining = &remaining[1..];
                    }
                    Some('"') => {
                        result.push('"');
                        remaining = &remaining[1..];
                    }
                    Some('0') => {
                        result.push('\0');
                        remaining = &remaining[1..];
                    }
                    Some('x') => {
                        remaining = &remaining[1..];
                        let hex: String = remaining.chars().take(2).collect();
                        if let Ok(byte) = u8::from_str_radix(&hex, 16) {
                            result.push(byte as char);
                            remaining = &remaining[hex.len()..];
                        }
                    }
                    Some(c) => {
                        result.push(c);
                        remaining = &remaining[c.len_utf8()..];
                    }
                    None => {
                        return Err(nom::Err::Error(nom::error::Error::new(
                            input,
                            nom::error::ErrorKind::Eof,
                        )));
                    }
                }
            }
            Some(c) => {
                result.push(c);
                remaining = &remaining[c.len_utf8()..];
            }
            None => {
                return Err(nom::Err::Error(nom::error::Error::new(
                    input,
                    nom::error::ErrorKind::Eof,
                )));
            }
        }
    }

    Ok((remaining, result))
}

/// Parse a `"…"` string and return its contents verbatim (no escape processing).
fn delimited_string(input: &str) -> PResult<'_, String> {
    let (input, _) = char('"')(input)?;
    let (input, content) = take_until("\"")(input)?;
    let (input, _) = char('"')(input)?;
    Ok((input, content.to_string()))
}

// ===================================================================
// Cell arrays  < … >
// ===================================================================

fn parse_cell_array(input: &str) -> PResult<'_, Vec<Cell>> {
    let (input, _) = char('<')(input)?;
    let (input, cells) = many0(ws_before(parse_cell))(input)?;
    let (input, _) = ws(input)?;
    let (input, _) = char('>')(input)?;
    Ok((input, cells))
}

fn parse_cell(input: &str) -> PResult<'_, Cell> {
    alt((
        map(parse_reference, Cell::Reference),
        map(parse_cell_expression, Cell::Expression),
        parse_macro_call,
        map(parse_char_literal, Cell::Literal),
        map(parse_integer, Cell::Literal),
    ))(input)
}

/// Parse a C macro invocation like `STM32_PINMUX('A', 0, ANALOG)` inside a
/// cell array.  Captures the identifier and the raw parenthesised arguments.
fn parse_macro_call(input: &str) -> PResult<'_, Cell> {
    let (input, name) = parse_identifier(input)?;
    let (input, _) = char('(')(input)?;
    let (input, args) = parse_balanced_parens(input, 1)?;
    Ok((input, Cell::Macro(name.to_string(), args)))
}

fn parse_cell_expression(input: &str) -> PResult<'_, String> {
    let (input, _) = char('(')(input)?;
    let (input, expr) = parse_balanced_parens(input, 1)?;
    Ok((input, expr))
}

fn parse_balanced_parens(input: &str, depth: usize) -> PResult<'_, String> {
    let mut result = String::new();
    let mut remaining = input;
    let mut d = depth;

    loop {
        match remaining.chars().next() {
            Some('(') => {
                result.push('(');
                remaining = &remaining[1..];
                d += 1;
            }
            Some(')') => {
                d -= 1;
                if d == 0 {
                    remaining = &remaining[1..];
                    return Ok((remaining, result));
                }
                result.push(')');
                remaining = &remaining[1..];
            }
            Some(c) => {
                result.push(c);
                remaining = &remaining[c.len_utf8()..];
            }
            None => {
                return Err(nom::Err::Error(nom::error::Error::new(
                    input,
                    nom::error::ErrorKind::Eof,
                )));
            }
        }
    }
}

fn parse_char_literal(input: &str) -> PResult<'_, u64> {
    let (input, _) = char('\'')(input)?;
    let (input, c) = none_of("'")(input)?;
    let (input, _) = char('\'')(input)?;
    Ok((input, c as u64))
}

// ===================================================================
// Byte strings  [ … ]
// ===================================================================

fn parse_byte_string(input: &str) -> PResult<'_, Vec<u8>> {
    let (input, _) = char('[')(input)?;
    let (input, bytes) = many0(ws_before(parse_hex_byte))(input)?;
    let (input, _) = ws(input)?;
    let (input, _) = char(']')(input)?;
    Ok((input, bytes))
}

fn parse_hex_byte(input: &str) -> PResult<'_, u8> {
    let (input, hex) = recognize(pair(
        satisfy(|c: char| c.is_ascii_hexdigit()),
        satisfy(|c: char| c.is_ascii_hexdigit()),
    ))(input)?;
    let byte = u8::from_str_radix(hex, 16).map_err(|_| {
        nom::Err::Error(nom::error::Error::new(input, nom::error::ErrorKind::HexDigit))
    })?;
    Ok((input, byte))
}

// ===================================================================
// References
// ===================================================================

fn parse_reference(input: &str) -> PResult<'_, Reference> {
    let (input, _) = char('&')(input)?;
    alt((
        map(
            tuple((char('{'), take_until("}"), char('}'))),
            |(_, p, _): (char, &str, char)| Reference::Path(p.to_string()),
        ),
        map(parse_identifier, |l| Reference::Label(l.to_string())),
    ))(input)
}

// ===================================================================
// Names & identifiers
// ===================================================================

fn parse_label_def(input: &str) -> PResult<'_, String> {
    let (input, name) = parse_identifier(input)?;
    let (input, _) = ws(input)?;
    let (input, _) = char(':')(input)?;
    Ok((input, name.to_string()))
}

fn parse_identifier(input: &str) -> PResult<'_, &str> {
    recognize(pair(
        satisfy(|c: char| c.is_ascii_alphabetic() || c == '_'),
        take_while(|c: char| c.is_ascii_alphanumeric() || c == '_'),
    ))(input)
}

fn parse_node_name(input: &str) -> PResult<'_, &str> {
    recognize(pair(
        satisfy(|c: char| c.is_ascii_alphanumeric() || c == '_'),
        take_while(|c: char| c.is_ascii_alphanumeric() || ",._+-#".contains(c)),
    ))(input)
}

fn parse_property_name(input: &str) -> PResult<'_, &str> {
    recognize(pair(
        satisfy(|c: char| c.is_ascii_alphanumeric() || c == '_' || c == '#'),
        take_while(|c: char| c.is_ascii_alphanumeric() || ",._+-?#".contains(c)),
    ))(input)
}

fn parse_unit_address(input: &str) -> PResult<'_, &str> {
    take_while1(|c: char| c.is_ascii_alphanumeric() || ",._".contains(c))(input)
}

// ===================================================================
// Numbers
// ===================================================================

fn parse_integer(input: &str) -> PResult<'_, u64> {
    alt((parse_hex_integer, parse_decimal_integer))(input)
}

fn parse_hex_integer(input: &str) -> PResult<'_, u64> {
    let (input, _) = alt((tag("0x"), tag("0X")))(input)?;
    let (input, digits) =
        take_while1(|c: char| c.is_ascii_hexdigit() || c == '_')(input)?;
    let clean: String = digits.chars().filter(|c| *c != '_').collect();
    let val = u64::from_str_radix(&clean, 16).map_err(|_| {
        nom::Err::Error(nom::error::Error::new(
            input,
            nom::error::ErrorKind::HexDigit,
        ))
    })?;
    Ok((input, val))
}

fn parse_decimal_integer(input: &str) -> PResult<'_, u64> {
    let (input, digits) = take_while1(|c: char| c.is_ascii_digit())(input)?;
    let val: u64 = digits.parse().map_err(|_| {
        nom::Err::Error(nom::error::Error::new(
            input,
            nom::error::ErrorKind::Digit,
        ))
    })?;
    Ok((input, val))
}

// ===================================================================
// Helpers
// ===================================================================

/// Produce a human-readable error message from a nom `ErrorKind` and context.
fn friendly_error_message(code: nom::error::ErrorKind, remaining: &str) -> String {
    let next_char = remaining.chars().next();
    let next_word: String = remaining
        .chars()
        .take_while(|c| !c.is_ascii_whitespace())
        .take(20)
        .collect();

    match code {
        nom::error::ErrorKind::Char => {
            // The parser expected a specific character.  Guess from context.
            if next_word.is_empty() {
                "unexpected end of input (expected `}` or `;`)".to_string()
            } else if next_char == Some('{') || next_char == Some('}') {
                format!("unexpected `{}`; check for missing `;`", next_word)
            } else {
                format!(
                    "unexpected token `{}`; expected `}}`, `;`, or a property/node name",
                    next_word
                )
            }
        }
        nom::error::ErrorKind::Tag => {
            format!("unexpected `{}`; expected a keyword or directive", next_word)
        }
        nom::error::ErrorKind::Eof => "unexpected end of file".to_string(),
        _ => format!("unexpected `{}`", next_word),
    }
}

pub(crate) fn merge_nodes(target: &mut Node, source: Node) {
    target.properties.extend(source.properties);
    target.children.extend(source.children);
    target.deleted_properties.extend(source.deleted_properties);
    target.deleted_nodes.extend(source.deleted_nodes);
    for l in source.labels {
        if !target.labels.contains(&l) {
            target.labels.push(l);
        }
    }
}

// ===================================================================
// Tests
// ===================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_minimal_dts() {
        let src = r#"/dts-v1/;
/ {
};
"#;
        let tree = parse_dts(src).unwrap();
        assert_eq!(tree.version, Some(DtsVersion::V1));
        assert!(tree.root.is_some());
    }

    #[test]
    fn parse_properties() {
        let src = r#"/dts-v1/;
/ {
    compatible = "vendor,board";
    model = "My Board";
    #address-cells = <2>;
    #size-cells = <2>;
    empty-prop;
    multi = "a", "b", "c";
    data = [DE AD BE EF];
};
"#;
        let tree = parse_dts(src).unwrap();
        let root = tree.root.as_ref().unwrap();

        assert_eq!(root.property("compatible").unwrap().as_string(), Some("vendor,board"));
        assert_eq!(root.property("model").unwrap().as_string(), Some("My Board"));
        assert_eq!(root.property("#address-cells").unwrap().as_u64_cells(), Some(vec![2]));
        assert!(root.property("empty-prop").unwrap().is_boolean());

        let multi = root.property("multi").unwrap().as_string_list().unwrap();
        assert_eq!(multi, vec!["a", "b", "c"]);

        let data = &root.property("data").unwrap().value.as_ref().unwrap().0;
        assert_eq!(data.len(), 1);
        if let ValuePart::ByteString(b) = &data[0] {
            assert_eq!(b, &[0xDE, 0xAD, 0xBE, 0xEF]);
        } else {
            panic!("expected ByteString");
        }
    }

    #[test]
    fn parse_child_nodes_and_labels() {
        let src = r#"/dts-v1/;
/ {
    soc {
        uart0: serial@12340000 {
            compatible = "ns16550a";
            reg = <0x12340000 0x100>;
        };
    };
};
"#;
        let tree = parse_dts(src).unwrap();
        let root = tree.root.as_ref().unwrap();
        let soc = root.child("soc").unwrap();
        let serial = soc.child_by_full_name("serial@12340000").unwrap();
        assert_eq!(serial.labels, vec!["uart0".to_string()]);
        assert_eq!(serial.property("compatible").unwrap().as_string(), Some("ns16550a"));
    }

    #[test]
    fn parse_includes() {
        let src = r#"/dts-v1/;
#include "board.dtsi"
#include <dt-bindings/gpio/gpio.h>
/include/ "common.dtsi"
/ {
};
"#;
        let tree = parse_dts(src).unwrap();
        assert_eq!(tree.includes.len(), 3);
        assert_eq!(tree.includes[0].path, "board.dtsi");
        assert_eq!(tree.includes[0].kind, IncludeKind::CPreprocessor);
        assert_eq!(tree.includes[1].path, "dt-bindings/gpio/gpio.h");
        assert_eq!(tree.includes[2].kind, IncludeKind::DtsInclude);
    }

    #[test]
    fn parse_reference_override() {
        let src = r#"/dts-v1/;
/ {
    leds {
        led0: led-0 {
            gpios = <&gpio0 10 0>;
        };
    };
};

&led0 {
    status = "okay";
};
"#;
        let tree = parse_dts(src).unwrap();
        assert_eq!(tree.reference_nodes.len(), 1);
        assert_eq!(
            tree.reference_nodes[0].reference,
            Reference::Label("led0".to_string())
        );
    }

    #[test]
    fn parse_overlay() {
        let src = r#"/dts-v1/;
/plugin/;

&{/soc/i2c@40000000} {
    sensor@48 {
        compatible = "ti,tmp102";
        reg = <0x48>;
    };
};
"#;
        let tree = parse_dts(src).unwrap();
        assert!(tree.is_plugin);
        assert_eq!(tree.reference_nodes.len(), 1);
        assert_eq!(
            tree.reference_nodes[0].reference,
            Reference::Path("/soc/i2c@40000000".to_string())
        );
    }

    #[test]
    fn parse_comments() {
        let src = r#"/dts-v1/;
/* block comment */
// line comment
/ {
    /* another comment */
    prop = <1>; // inline
};
"#;
        let tree = parse_dts(src).unwrap();
        assert!(tree.root.is_some());
    }

    #[test]
    fn parse_memreserve() {
        let src = r#"/dts-v1/;
/memreserve/ 0x10000000 0x4000;
/ {
};
"#;
        let tree = parse_dts(src).unwrap();
        assert_eq!(tree.memory_reservations.len(), 1);
        assert_eq!(tree.memory_reservations[0].address, 0x10000000);
        assert_eq!(tree.memory_reservations[0].size, 0x4000);
    }

    #[test]
    fn parse_cell_expression() {
        let src = r#"/dts-v1/;
/ {
    val = <(1 + 2)>;
};
"#;
        let tree = parse_dts(src).unwrap();
        let root = tree.root.unwrap();
        let val = root.property("val").unwrap().value.as_ref().unwrap();
        if let ValuePart::CellArray(cells) = &val.0[0] {
            assert_eq!(cells[0], Cell::Expression("1 + 2".to_string()));
        } else {
            panic!("expected CellArray");
        }
    }

    #[test]
    fn parse_delete_directives() {
        let src = r#"/dts-v1/;
/ {
    /delete-property/ old-prop;
    /delete-node/ old-node;
};
"#;
        let tree = parse_dts(src).unwrap();
        let root = tree.root.unwrap();
        assert_eq!(root.deleted_properties, vec!["old-prop"]);
        assert_eq!(root.deleted_nodes, vec!["old-node"]);
    }

    #[test]
    fn parse_preprocessor_passthrough() {
        let src = r#"#define MY_CONST 42
/dts-v1/;
#ifdef SOME_FLAG
/ {
};
"#;
        // Should not fail – unknown preprocessor lines are silently skipped.
        let tree = parse_dts(src).unwrap();
        assert!(tree.root.is_some());
    }
}
