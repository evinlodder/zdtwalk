pub mod binding;
pub mod error;
pub mod model;
pub mod parser;
pub mod resolver;
pub mod serializer;

pub use binding::{
    deserialize_binding, deserialize_binding_from_reader, Binding, BindingInclude, ChildBinding,
    FilteredInclude, IncludeEntry, PropertySpec, PropertyType, Value,
};
pub use error::{Error, ParseError, ParseErrorKind};
pub use model::*;
pub use parser::parse_dts;
pub use resolver::Resolver;
pub use serializer::{format_property_value, serialize, OutputFormat, SerializerConfig};
