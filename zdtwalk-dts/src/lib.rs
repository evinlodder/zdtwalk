pub mod error;
pub mod model;
pub mod parser;
pub mod resolver;
pub mod serializer;

pub use error::{Error, ParseError, ParseErrorKind};
pub use model::*;
pub use parser::parse_dts;
pub use resolver::Resolver;
pub use serializer::{format_property_value, serialize, OutputFormat, SerializerConfig};
