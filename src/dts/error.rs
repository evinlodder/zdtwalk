use std::fmt;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum Error {
    #[error("{0}")]
    Parse(#[from] ParseError),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Include resolution error: {0}")]
    Include(String),

    #[error("Binding YAML error: {0}")]
    Binding(#[from] serde_yaml::Error),

    #[error("West error: {0}")]
    West(#[from] crate::west::WestError),
}

#[derive(Debug)]
pub struct ParseError {
    pub line: usize,
    pub column: usize,
    pub kind: ParseErrorKind,
    pub context_line: String,
}

#[derive(Debug)]
pub enum ParseErrorKind {
    Syntax(String),
    TrailingInput,
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.kind {
            ParseErrorKind::Syntax(msg) => {
                writeln!(f, "parse error at line {}, column {}:", self.line, self.column)?;
                writeln!(f, "  --> {}:{}", self.line, self.column)?;
                writeln!(f, "   |")?;
                writeln!(f, "{:>3} | {}", self.line, self.context_line)?;
                writeln!(f, "   | {}^", " ".repeat(self.column.saturating_sub(1)))?;
                write!(f, "   = {}", msg)
            }
            ParseErrorKind::TrailingInput => {
                writeln!(f, "unexpected input at line {}, column {}:", self.line, self.column)?;
                writeln!(f, "  --> {}:{}", self.line, self.column)?;
                writeln!(f, "   |")?;
                writeln!(f, "{:>3} | {}", self.line, self.context_line)?;
                writeln!(f, "   | {}^", " ".repeat(self.column.saturating_sub(1)))?;
                write!(f, "   = parser finished but input remains; possible missing `;` or `}}`")
            }
        }
    }
}

impl std::error::Error for ParseError {}

/// Compute (line, column, context_line) from the original source and a remaining slice.
pub fn location_in(source: &str, remaining: &str) -> (usize, usize, String) {
    let offset = source.len() - remaining.len();
    let consumed = &source[..offset];
    let line = consumed.matches('\n').count() + 1;
    let last_nl = consumed.rfind('\n').map(|i| i + 1).unwrap_or(0);
    let column = offset - last_nl + 1;

    // Extract the source line for context.
    let line_start = last_nl;
    let line_end = source[offset..].find('\n').map(|i| offset + i).unwrap_or(source.len());
    let context_line = source[line_start..line_end].to_string();

    (line, column, context_line)
}
