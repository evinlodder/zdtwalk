//! Zephyr devicetree binding (YAML) deserialization.
//!
//! Binding files describe the properties and structure that nodes with a
//! particular `compatible` string are expected to have.  This module provides
//! types and helpers for deserializing those YAML files.
//!
//! # Example
//!
//! ```
//! use zdtwalk_dts::binding::deserialize_binding;
//!
//! let yaml = r#"
//! description: My sensor
//! compatible: "vendor,my-sensor"
//! include: [base.yaml]
//! properties:
//!   reg:
//!     required: true
//! "#;
//!
//! let binding = deserialize_binding(yaml).unwrap();
//! assert_eq!(binding.compatible.as_deref(), Some("vendor,my-sensor"));
//! assert_eq!(binding.include_file_names(), vec!["base.yaml"]);
//! ```

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fmt;

// ---------------------------------------------------------------------------
// Top-level binding
// ---------------------------------------------------------------------------

/// A parsed Zephyr devicetree binding file.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct Binding {
    /// Human-readable description of what hardware or subsystem this binding
    /// covers.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,

    /// The `compatible` string this binding matches
    /// (e.g. `"st,stm32-i2c-v2"`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub compatible: Option<String>,

    /// Other binding YAML files to include.  Included bindings contribute
    /// their property definitions to this binding.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub include: Option<BindingInclude>,

    /// Property definitions keyed by property name.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub properties: BTreeMap<String, PropertySpec>,

    /// The bus protocol this device sits on (e.g. `"i2c"`, `"spi"`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub on_bus: Option<String>,

    /// The bus protocol this device provides to its children.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bus: Option<String>,

    /// Binding specification applied to child nodes.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub child_binding: Option<Box<ChildBinding>>,
}

impl Binding {
    /// Convenience method returning all included file names, or an empty
    /// vector when there is no `include` field.
    pub fn include_file_names(&self) -> Vec<&str> {
        match &self.include {
            Some(inc) => inc.file_names(),
            None => Vec::new(),
        }
    }
}

// ---------------------------------------------------------------------------
// Includes
// ---------------------------------------------------------------------------

/// The `include` field accepts a single filename, a list of filenames,
/// or a list mixing plain filenames with filtered include objects.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum BindingInclude {
    /// A single include path, e.g. `include: base.yaml`.
    Single(String),
    /// A list of includes, e.g. `include: [a.yaml, b.yaml]` or a mix of
    /// plain strings and filtered entries.
    List(Vec<IncludeEntry>),
}

impl BindingInclude {
    /// Collect every included file name as a string slice.
    pub fn file_names(&self) -> Vec<&str> {
        match self {
            BindingInclude::Single(s) => vec![s.as_str()],
            BindingInclude::List(entries) => entries.iter().map(|e| e.file_name()).collect(),
        }
    }
}

/// One entry in an `include` list.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum IncludeEntry {
    /// A plain filename string.
    Name(String),
    /// A filename with property allow/block-list filters.
    Filtered(FilteredInclude),
}

impl IncludeEntry {
    /// Return the filename regardless of the variant.
    pub fn file_name(&self) -> &str {
        match self {
            IncludeEntry::Name(n) => n,
            IncludeEntry::Filtered(f) => &f.name,
        }
    }

    /// If this is a filtered include, return the filter details.
    pub fn filter(&self) -> Option<&FilteredInclude> {
        match self {
            IncludeEntry::Name(_) => None,
            IncludeEntry::Filtered(f) => Some(f),
        }
    }
}

/// A filtered include entry specifying which properties to allow or block
/// from the included file.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct FilteredInclude {
    /// The binding file to include.
    pub name: String,
    /// If set, only these properties are imported from the included file.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub property_allowlist: Option<Vec<String>>,
    /// If set, these properties are excluded from the import.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub property_blocklist: Option<Vec<String>>,
}

// ---------------------------------------------------------------------------
// Property specification
// ---------------------------------------------------------------------------

/// The specification for a single DT property within a binding.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct PropertySpec {
    /// The property type (e.g. `int`, `string`, `phandle-array`).
    /// May be absent when inherited from an included binding.
    #[serde(rename = "type", default, skip_serializing_if = "Option::is_none")]
    pub property_type: Option<PropertyType>,

    /// Whether this property must be present on matching nodes.
    #[serde(default)]
    pub required: bool,

    /// Human-readable description of the property.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,

    /// A constant value this property must take.
    #[serde(rename = "const", default, skip_serializing_if = "Option::is_none")]
    pub const_value: Option<Value>,

    /// The default value used when the property is absent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default: Option<Value>,

    /// An enumeration of allowed values.
    #[serde(rename = "enum", default, skip_serializing_if = "Option::is_none")]
    pub enum_values: Option<Vec<Value>>,

    /// Whether this property is deprecated.
    #[serde(default)]
    pub deprecated: bool,

    /// The specifier space for `phandle-array` properties
    /// (e.g. `"gpio"`, `"pwm"`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub specifier_space: Option<String>,
}

// ---------------------------------------------------------------------------
// Property type
// ---------------------------------------------------------------------------

/// All property types recognised by the Zephyr devicetree binding format.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum PropertyType {
    #[serde(rename = "string")]
    String,
    #[serde(rename = "int")]
    Int,
    #[serde(rename = "boolean")]
    Boolean,
    #[serde(rename = "array")]
    Array,
    #[serde(rename = "uint8-array")]
    Uint8Array,
    #[serde(rename = "string-array")]
    StringArray,
    #[serde(rename = "phandle")]
    Phandle,
    #[serde(rename = "phandles")]
    Phandles,
    #[serde(rename = "phandle-array")]
    PhandleArray,
    #[serde(rename = "path")]
    Path,
    #[serde(rename = "compound")]
    Compound,
}

impl fmt::Display for PropertyType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PropertyType::String => write!(f, "string"),
            PropertyType::Int => write!(f, "int"),
            PropertyType::Boolean => write!(f, "boolean"),
            PropertyType::Array => write!(f, "array"),
            PropertyType::Uint8Array => write!(f, "uint8-array"),
            PropertyType::StringArray => write!(f, "string-array"),
            PropertyType::Phandle => write!(f, "phandle"),
            PropertyType::Phandles => write!(f, "phandles"),
            PropertyType::PhandleArray => write!(f, "phandle-array"),
            PropertyType::Path => write!(f, "path"),
            PropertyType::Compound => write!(f, "compound"),
        }
    }
}

// ---------------------------------------------------------------------------
// Child binding
// ---------------------------------------------------------------------------

/// Binding rules applied to child nodes of matching hardware.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct ChildBinding {
    /// Description of the child node requirements.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,

    /// Properties expected on child nodes.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub properties: BTreeMap<String, PropertySpec>,

    /// Nested child binding for grandchild nodes.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub child_binding: Option<Box<ChildBinding>>,
}

// ---------------------------------------------------------------------------
// Generic YAML value
// ---------------------------------------------------------------------------

/// A generic value type for property defaults, constants, and enumerations.
///
/// Covers the value shapes that appear in Zephyr binding YAML files.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Value {
    /// A boolean value.
    Bool(bool),
    /// A signed integer value.
    Integer(i64),
    /// A floating-point value.
    Float(f64),
    /// A string value.
    String(String),
    /// A sequence of values.
    Sequence(Vec<Value>),
    /// A string-keyed mapping.
    Mapping(BTreeMap<String, Value>),
}

impl fmt::Display for Value {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Value::Bool(b) => write!(f, "{b}"),
            Value::Integer(i) => write!(f, "{i}"),
            Value::Float(v) => write!(f, "{v}"),
            Value::String(s) => write!(f, "{s}"),
            Value::Sequence(seq) => {
                write!(f, "[")?;
                for (i, v) in seq.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{v}")?;
                }
                write!(f, "]")
            }
            Value::Mapping(map) => {
                write!(f, "{{")?;
                for (i, (k, v)) in map.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{k}: {v}")?;
                }
                write!(f, "}}")
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// Deserialize a Zephyr devicetree binding from a YAML string.
pub fn deserialize_binding(yaml: &str) -> Result<Binding, super::Error> {
    Ok(serde_yaml::from_str(yaml)?)
}

/// Deserialize a Zephyr devicetree binding from a byte reader.
pub fn deserialize_binding_from_reader<R: std::io::Read>(
    reader: R,
) -> Result<Binding, super::Error> {
    Ok(serde_yaml::from_reader(reader)?)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn full_stm32_binding() {
        let yaml = include_str!("../../test_binding.yaml");
        let binding = deserialize_binding(yaml).unwrap();

        assert_eq!(
            binding.description.as_deref(),
            Some("STM32 I2C V2 controller")
        );
        assert_eq!(
            binding.compatible.as_deref(),
            Some("st,stm32-i2c-v2")
        );

        // Includes
        let includes = binding.include_file_names();
        assert_eq!(includes, vec!["i2c-controller.yaml", "pinctrl-device.yaml"]);

        // Total property count
        assert_eq!(binding.properties.len(), 9);

        // Required properties (type inherited from includes)
        assert!(binding.properties["reg"].required);
        assert!(binding.properties["interrupts"].required);
        assert!(binding.properties["pinctrl-0"].required);
        assert!(binding.properties["pinctrl-names"].required);

        // Typed properties
        assert_eq!(
            binding.properties["timings"].property_type,
            Some(PropertyType::Array)
        );
        assert!(!binding.properties["timings"].required);
        assert!(binding.properties["timings"].description.is_some());

        assert_eq!(
            binding.properties["scl-gpios"].property_type,
            Some(PropertyType::PhandleArray)
        );
        assert_eq!(
            binding.properties["sda-gpios"].property_type,
            Some(PropertyType::PhandleArray)
        );
        assert_eq!(
            binding.properties["dmas"].property_type,
            Some(PropertyType::PhandleArray)
        );
        assert_eq!(
            binding.properties["dma-names"].property_type,
            Some(PropertyType::StringArray)
        );
    }

    #[test]
    fn single_include() {
        let yaml = "compatible: \"vendor,device\"\ninclude: base.yaml\n";
        let binding = deserialize_binding(yaml).unwrap();
        assert_eq!(binding.include_file_names(), vec!["base.yaml"]);
        assert!(matches!(
            binding.include,
            Some(BindingInclude::Single(_))
        ));
    }

    #[test]
    fn list_include() {
        let yaml = "include: [a.yaml, b.yaml, c.yaml]\n";
        let binding = deserialize_binding(yaml).unwrap();
        assert_eq!(
            binding.include_file_names(),
            vec!["a.yaml", "b.yaml", "c.yaml"]
        );
    }

    #[test]
    fn filtered_include() {
        let yaml = r#"
include:
  - name: base.yaml
    property-allowlist:
      - reg
      - status
  - name: optional.yaml
    property-blocklist:
      - deprecated-prop
  - simple.yaml
"#;
        let binding = deserialize_binding(yaml).unwrap();
        let names = binding.include_file_names();
        assert_eq!(names, vec!["base.yaml", "optional.yaml", "simple.yaml"]);

        if let Some(BindingInclude::List(entries)) = &binding.include {
            assert_eq!(entries.len(), 3);

            // First: filtered with allowlist
            let f = entries[0].filter().unwrap();
            assert_eq!(f.name, "base.yaml");
            assert_eq!(
                f.property_allowlist,
                Some(vec!["reg".into(), "status".into()])
            );
            assert_eq!(f.property_blocklist, None);

            // Second: filtered with blocklist
            let f = entries[1].filter().unwrap();
            assert_eq!(f.name, "optional.yaml");
            assert_eq!(
                f.property_blocklist,
                Some(vec!["deprecated-prop".into()])
            );

            // Third: plain string
            assert!(entries[2].filter().is_none());
            assert_eq!(entries[2].file_name(), "simple.yaml");
        } else {
            panic!("expected BindingInclude::List");
        }
    }

    #[test]
    fn all_property_types() {
        let yaml = r#"
properties:
  p-string:
    type: string
  p-int:
    type: int
  p-boolean:
    type: boolean
  p-array:
    type: array
  p-uint8-array:
    type: uint8-array
  p-string-array:
    type: string-array
  p-phandle:
    type: phandle
  p-phandles:
    type: phandles
  p-phandle-array:
    type: phandle-array
  p-path:
    type: path
  p-compound:
    type: compound
"#;
        let binding = deserialize_binding(yaml).unwrap();
        assert_eq!(binding.properties.len(), 11);
        assert_eq!(binding.properties["p-string"].property_type, Some(PropertyType::String));
        assert_eq!(binding.properties["p-int"].property_type, Some(PropertyType::Int));
        assert_eq!(binding.properties["p-boolean"].property_type, Some(PropertyType::Boolean));
        assert_eq!(binding.properties["p-array"].property_type, Some(PropertyType::Array));
        assert_eq!(binding.properties["p-uint8-array"].property_type, Some(PropertyType::Uint8Array));
        assert_eq!(binding.properties["p-string-array"].property_type, Some(PropertyType::StringArray));
        assert_eq!(binding.properties["p-phandle"].property_type, Some(PropertyType::Phandle));
        assert_eq!(binding.properties["p-phandles"].property_type, Some(PropertyType::Phandles));
        assert_eq!(binding.properties["p-phandle-array"].property_type, Some(PropertyType::PhandleArray));
        assert_eq!(binding.properties["p-path"].property_type, Some(PropertyType::Path));
        assert_eq!(binding.properties["p-compound"].property_type, Some(PropertyType::Compound));
    }

    #[test]
    fn property_type_display() {
        assert_eq!(PropertyType::String.to_string(), "string");
        assert_eq!(PropertyType::Int.to_string(), "int");
        assert_eq!(PropertyType::Boolean.to_string(), "boolean");
        assert_eq!(PropertyType::Array.to_string(), "array");
        assert_eq!(PropertyType::Uint8Array.to_string(), "uint8-array");
        assert_eq!(PropertyType::StringArray.to_string(), "string-array");
        assert_eq!(PropertyType::Phandle.to_string(), "phandle");
        assert_eq!(PropertyType::Phandles.to_string(), "phandles");
        assert_eq!(PropertyType::PhandleArray.to_string(), "phandle-array");
        assert_eq!(PropertyType::Path.to_string(), "path");
        assert_eq!(PropertyType::Compound.to_string(), "compound");
    }

    #[test]
    fn const_default_enum() {
        let yaml = r#"
properties:
  fixed-value:
    type: int
    const: 42
  with-default:
    type: string
    default: "hello"
  choice:
    type: int
    enum:
      - 100
      - 200
      - 300
"#;
        let binding = deserialize_binding(yaml).unwrap();
        assert_eq!(
            binding.properties["fixed-value"].const_value,
            Some(Value::Integer(42))
        );
        assert_eq!(
            binding.properties["with-default"].default,
            Some(Value::String("hello".into()))
        );
        assert_eq!(
            binding.properties["choice"].enum_values,
            Some(vec![
                Value::Integer(100),
                Value::Integer(200),
                Value::Integer(300),
            ])
        );
    }

    #[test]
    fn child_binding() {
        let yaml = r#"
description: Parent device
child-binding:
  description: Child endpoint
  properties:
    reg:
      type: int
      required: true
    label:
      type: string
"#;
        let binding = deserialize_binding(yaml).unwrap();
        let child = binding.child_binding.as_ref().unwrap();
        assert_eq!(child.description.as_deref(), Some("Child endpoint"));
        assert_eq!(child.properties.len(), 2);
        assert!(child.properties["reg"].required);
        assert_eq!(child.properties["reg"].property_type, Some(PropertyType::Int));
        assert_eq!(child.properties["label"].property_type, Some(PropertyType::String));
    }

    #[test]
    fn nested_child_binding() {
        let yaml = r#"
child-binding:
  description: Level 1
  child-binding:
    description: Level 2
    properties:
      addr:
        type: int
"#;
        let binding = deserialize_binding(yaml).unwrap();
        let child1 = binding.child_binding.as_ref().unwrap();
        assert_eq!(child1.description.as_deref(), Some("Level 1"));
        let child2 = child1.child_binding.as_ref().unwrap();
        assert_eq!(child2.description.as_deref(), Some("Level 2"));
        assert_eq!(
            child2.properties["addr"].property_type,
            Some(PropertyType::Int)
        );
    }

    #[test]
    fn bus_field() {
        let yaml = "compatible: \"vendor,i2c-ctrl\"\nbus: i2c\n";
        let binding = deserialize_binding(yaml).unwrap();
        assert_eq!(binding.bus.as_deref(), Some("i2c"));
        assert_eq!(binding.on_bus, None);
    }

    #[test]
    fn on_bus_field() {
        let yaml = "compatible: \"vendor,sensor\"\non-bus: i2c\n";
        let binding = deserialize_binding(yaml).unwrap();
        assert_eq!(binding.on_bus.as_deref(), Some("i2c"));
        assert_eq!(binding.bus, None);
    }

    #[test]
    fn minimal_binding() {
        let yaml = "description: A minimal binding\n";
        let binding = deserialize_binding(yaml).unwrap();
        assert_eq!(
            binding.description.as_deref(),
            Some("A minimal binding")
        );
        assert_eq!(binding.compatible, None);
        assert_eq!(binding.include, None);
        assert!(binding.properties.is_empty());
        assert_eq!(binding.bus, None);
        assert_eq!(binding.on_bus, None);
        assert!(binding.child_binding.is_none());
    }

    #[test]
    fn deprecated_property() {
        let yaml = "properties:\n  old-prop:\n    type: int\n    deprecated: true\n";
        let binding = deserialize_binding(yaml).unwrap();
        assert!(binding.properties["old-prop"].deprecated);
    }

    #[test]
    fn specifier_space() {
        let yaml = r#"
properties:
  ios:
    type: phandle-array
    specifier-space: gpio
"#;
        let binding = deserialize_binding(yaml).unwrap();
        assert_eq!(
            binding.properties["ios"].specifier_space.as_deref(),
            Some("gpio")
        );
    }

    #[test]
    fn from_reader() {
        let yaml = b"description: From reader\ncompatible: \"test,device\"\n";
        let binding = deserialize_binding_from_reader(yaml.as_slice()).unwrap();
        assert_eq!(binding.description.as_deref(), Some("From reader"));
        assert_eq!(binding.compatible.as_deref(), Some("test,device"));
    }

    #[test]
    fn property_without_type() {
        let yaml = r#"
properties:
  reg:
    required: true
"#;
        let binding = deserialize_binding(yaml).unwrap();
        assert!(binding.properties["reg"].required);
        assert_eq!(binding.properties["reg"].property_type, None);
    }

    #[test]
    fn value_display() {
        assert_eq!(Value::Integer(42).to_string(), "42");
        assert_eq!(Value::String("hello".into()).to_string(), "hello");
        assert_eq!(Value::Bool(true).to_string(), "true");
        assert_eq!(Value::Float(3.14).to_string(), "3.14");
        assert_eq!(
            Value::Sequence(vec![Value::Integer(1), Value::Integer(2)]).to_string(),
            "[1, 2]"
        );
    }

    #[test]
    fn default_binding_is_empty() {
        let binding = Binding::default();
        assert_eq!(binding.description, None);
        assert_eq!(binding.compatible, None);
        assert_eq!(binding.include, None);
        assert!(binding.properties.is_empty());
    }

    #[test]
    fn default_property_spec() {
        let spec = PropertySpec::default();
        assert_eq!(spec.property_type, None);
        assert!(!spec.required);
        assert_eq!(spec.description, None);
        assert_eq!(spec.const_value, None);
        assert_eq!(spec.default, None);
        assert_eq!(spec.enum_values, None);
        assert!(!spec.deprecated);
        assert_eq!(spec.specifier_space, None);
    }

    #[test]
    fn include_entry_accessors() {
        let simple = IncludeEntry::Name("foo.yaml".into());
        assert_eq!(simple.file_name(), "foo.yaml");
        assert!(simple.filter().is_none());

        let filtered = IncludeEntry::Filtered(FilteredInclude {
            name: "bar.yaml".into(),
            property_allowlist: Some(vec!["reg".into()]),
            property_blocklist: None,
        });
        assert_eq!(filtered.file_name(), "bar.yaml");
        assert!(filtered.filter().is_some());
    }

    #[test]
    fn no_include_gives_empty_file_names() {
        let binding = Binding::default();
        assert!(binding.include_file_names().is_empty());
    }

    #[test]
    fn boolean_default_value() {
        let yaml = r#"
properties:
  enabled:
    type: boolean
    default: true
"#;
        let binding = deserialize_binding(yaml).unwrap();
        assert_eq!(binding.properties["enabled"].default, Some(Value::Bool(true)));
    }

    #[test]
    fn string_enum() {
        let yaml = r#"
properties:
  mode:
    type: string
    enum:
      - "low-power"
      - "normal"
      - "high-performance"
"#;
        let binding = deserialize_binding(yaml).unwrap();
        assert_eq!(
            binding.properties["mode"].enum_values,
            Some(vec![
                Value::String("low-power".into()),
                Value::String("normal".into()),
                Value::String("high-performance".into()),
            ])
        );
    }
}
