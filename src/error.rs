//! Structured diagnostics and parse errors.

use crate::source::{LineIndex, TextRange};
use std::fmt;

/// Diagnostic severity.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub enum Severity {
    /// A fatal error.
    Error,
    /// A non-fatal warning.
    Warning,
    /// Syntax unavailable in the selected Python version.
    Unsupported,
}

/// Stable diagnostic categories.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub enum DiagnosticCode {
    /// Lexical error.
    Lexical,
    /// Indentation error.
    Indentation,
    /// Generic syntax error.
    Syntax,
    /// Validation error.
    Validation,
    /// Unsupported version syntax.
    UnsupportedSyntax,
    /// Parser recursion depth exceeded.
    TooDeep,
    /// Source encoding error.
    Encoding,
    /// Invalid string escape sequence.
    InvalidEscape,
}

impl DiagnosticCode {
    /// Returns the stable code used in reports.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Lexical => "PSY0101",
            Self::Indentation => "PSY0201",
            Self::Syntax => "PSY0301",
            Self::Validation => "PSY0401",
            Self::UnsupportedSyntax => "PSY0901",
            Self::TooDeep => "PSY0302",
            Self::Encoding => "PSY0102",
            Self::InvalidEscape => "PSY0103",
        }
    }
}

/// A secondary labeled range.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Label {
    /// Labeled source range.
    pub range: TextRange,
    /// Explanation for the range.
    pub message: String,
}

/// A structured diagnostic.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Diagnostic {
    /// Diagnostic category.
    pub code: DiagnosticCode,
    /// Severity.
    pub severity: Severity,
    /// Human-readable message.
    pub message: String,
    /// Primary source range.
    pub range: TextRange,
    /// Secondary labels.
    pub labels: Vec<Label>,
    /// Optional remediation hint.
    pub help: Option<String>,
}

impl Diagnostic {
    /// Creates an error diagnostic.
    pub fn error(code: DiagnosticCode, range: TextRange, message: impl Into<String>) -> Self {
        Self {
            code,
            severity: Severity::Error,
            message: message.into(),
            range,
            labels: Vec::new(),
            help: None,
        }
    }
    /// Creates a warning diagnostic.
    pub fn warning(code: DiagnosticCode, range: TextRange, message: impl Into<String>) -> Self {
        Self {
            code,
            severity: Severity::Warning,
            message: message.into(),
            range,
            labels: Vec::new(),
            help: None,
        }
    }
    /// Creates an unsupported-syntax diagnostic.
    pub fn unsupported(range: TextRange, message: impl Into<String>) -> Self {
        Self {
            code: DiagnosticCode::UnsupportedSyntax,
            severity: Severity::Unsupported,
            message: message.into(),
            range,
            labels: Vec::new(),
            help: None,
        }
    }
    /// Adds a secondary label.
    pub fn with_label(mut self, range: TextRange, message: impl Into<String>) -> Self {
        self.labels.push(Label { range, message: message.into() });
        self
    }
    /// Adds a help message.
    pub fn with_help(mut self, message: impl Into<String>) -> Self {
        self.help = Some(message.into());
        self
    }
    /// Renders the diagnostic with a source line and caret.
    pub fn display_with_source(&self, name: &str, source: &str) -> String {
        let index = LineIndex::new(source);
        let location = index.line_col_utf8(source, self.range.start());
        let line = source.lines().nth(location.line.saturating_sub(1) as usize).unwrap_or("");
        let mut caret = " ".repeat(location.column as usize);
        caret.push('^');
        format!(
            "  File \"{name}\", line {}\n    {line}\n    {caret}\n{}: {}",
            location.line, self.severity, self.message
        )
    }
}

impl fmt::Display for Severity {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Error => "SyntaxError",
            Self::Warning => "Warning",
            Self::Unsupported => "UnsupportedSyntax",
        })
    }
}

impl fmt::Display for Diagnostic {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.code.as_str(), self.message)
    }
}

/// A strict parse failure.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ParseError {
    /// Underlying diagnostic.
    pub diagnostic: Diagnostic,
}

impl ParseError {
    /// Creates a syntax parse error.
    pub fn syntax(range: TextRange, message: impl Into<String>) -> Self {
        Self { diagnostic: Diagnostic::error(DiagnosticCode::Syntax, range, message) }
    }
    /// Creates a recursion-depth error.
    pub fn too_deep(range: TextRange) -> Self {
        Self {
            diagnostic: Diagnostic::error(
                DiagnosticCode::TooDeep,
                range,
                "maximum parser depth exceeded",
            ),
        }
    }
    /// Creates a parser resource-limit error for oversized ASTs.
    pub fn too_many_nodes(range: TextRange) -> Self {
        Self {
            diagnostic: Diagnostic::error(
                DiagnosticCode::TooDeep,
                range,
                "maximum parser node budget exceeded",
            ),
        }
    }
    /// Returns the failure range.
    pub const fn range(&self) -> TextRange {
        self.diagnostic.range
    }
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.diagnostic.fmt(f)
    }
}

impl std::error::Error for ParseError {}
