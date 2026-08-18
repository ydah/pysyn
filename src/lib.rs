//! Python source tokenizer, parser, AST, and diagnostics.

#![forbid(unsafe_code)]
#![deny(missing_docs)]

pub mod ast;
pub mod error;
pub mod lexer;
pub mod parser;
pub mod printer;
pub mod source;
pub mod token;
pub mod validate;
pub mod visit;

pub use ast::ModModule;
pub use error::{Diagnostic, DiagnosticCode, ParseError, Severity};
pub use parser::{parse, parse_expression, parse_module, ParseMode, ParseOptions, Parsed};
pub use source::{LineCol, LineIndex, SourceFile, TextRange, TextSize};
