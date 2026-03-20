pub mod binding;
pub mod error;
pub mod model;
pub mod parser;
pub mod resolver;
pub mod serializer;

pub use binding::{deserialize_binding, Binding};
pub use error::Error;
pub use model::*;
pub use parser::{parse_dts, parse_property_value_str};
pub use resolver::Resolver;
pub use serializer::{format_property_value, serialize, OutputFormat, SerializerConfig};
