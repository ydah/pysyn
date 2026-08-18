//! Hand-written recursive-descent and Pratt parser.

#![allow(missing_docs)]

use crate::ast::*;
use crate::error::{Diagnostic, DiagnosticCode, ParseError, Severity};
use crate::lexer::{tokenize_with, LexMode, LexOptions};
use crate::source::{LineIndex, TextRange, TextSize};
use crate::token::{PythonVersion, Token, TokenKind};
use std::collections::{HashMap, HashSet};
#[cfg(feature = "nfkc")]
use unicode_normalization::UnicodeNormalization;

/// Parsing mode for complete source text.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum SourceMode {
    Module,
    Expression,
    Interactive,
}

/// Controls strictness and parser resource limits.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ParseOptions {
    pub version: PythonVersion,
    pub mode: SourceMode,
    pub parse_mode: ParseMode,
    pub keep_comments: bool,
    pub type_comments: bool,
    pub max_depth: u32,
    /// Maximum number of expression parser invocations allowed for one input.
    pub max_nodes: usize,
    pub max_errors: usize,
    pub keep_tokens: bool,
}

impl Default for ParseOptions {
    fn default() -> Self {
        Self {
            version: PythonVersion::default(),
            mode: SourceMode::Module,
            parse_mode: ParseMode::Strict,
            keep_comments: true,
            type_comments: false,
            max_depth: 200,
            max_nodes: 100_000,
            max_errors: 100,
            keep_tokens: false,
        }
    }
}

/// Controls whether syntax errors stop parsing.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum ParseMode {
    Strict,
    Recover,
}

/// A recovered parse result.
#[derive(Clone, Debug, PartialEq)]
pub struct Parsed {
    pub module: ModModule,
    pub comments: Vec<Comment>,
    pub tokens: Vec<Token>,
    pub errors: Vec<Diagnostic>,
}

/// Parses a complete Python module in strict mode.
pub fn parse_module(src: &str) -> Result<ModModule, ParseError> {
    let options = ParseOptions { mode: SourceMode::Module, ..ParseOptions::default() };
    let mut parser = Parser::new(src, options);
    let module = parser.parse_module_inner()?;
    if let Some(error) =
        parser.errors.into_iter().find(|diagnostic| diagnostic.severity == Severity::Error)
    {
        return Err(ParseError { diagnostic: error });
    }
    Ok(module)
}

/// Parses one Python expression in strict mode.
pub fn parse_expression(src: &str) -> Result<Expr, ParseError> {
    let options = ParseOptions { mode: SourceMode::Expression, ..ParseOptions::default() };
    let mut parser = Parser::new(src, options);
    let first = parser.expression(0)?;
    let expr = if parser.at(TokenKind::Comma) { parser.comma_expression(first)? } else { first };
    parser.skip_newlines();
    if !parser.at(TokenKind::EndMarker) {
        return Err(parser.error_here("unexpected token after expression"));
    }
    Ok(expr)
}

/// Parses source with the requested recovery and version options.
pub fn parse(src: &str, options: ParseOptions) -> Parsed {
    let escape_warnings = collect_invalid_escape_warnings(src, options.version);
    let mut parser = Parser::new(src, options);
    let module = match parser.options.mode {
        SourceMode::Expression => match parser.expression(0) {
            Ok(expression) => ModModule {
                body: vec![Stmt::Expr(StmtExpr {
                    range: expression.range(),
                    value: Box::new(expression),
                })],
                type_ignores: if parser.options.type_comments {
                    collect_type_ignores(src)
                } else {
                    Vec::new()
                },
                range: TextRange::from_usize(0, src.len()),
                source: Some(src.into()),
            },
            Err(error) => {
                parser.push_error(error.diagnostic);
                ModModule {
                    body: Vec::new(),
                    type_ignores: if parser.options.type_comments {
                        collect_type_ignores(src)
                    } else {
                        Vec::new()
                    },
                    range: TextRange::from_usize(0, src.len()),
                    source: Some(src.into()),
                }
            }
        },
        SourceMode::Module | SourceMode::Interactive => match parser.parse_module_inner() {
            Ok(module) => module,
            Err(error) => {
                parser.push_error(error.diagnostic);
                ModModule {
                    body: Vec::new(),
                    type_ignores: if parser.options.type_comments {
                        collect_type_ignores(src)
                    } else {
                        Vec::new()
                    },
                    range: TextRange::from_usize(0, src.len()),
                    source: Some(src.into()),
                }
            }
        },
    };
    parser.errors.extend(escape_warnings);
    let comments = if parser.options.keep_comments { collect_comments(src) } else { Vec::new() };
    let tokens = if parser.options.keep_tokens { parser.tokens.clone() } else { Vec::new() };
    Parsed { module, comments, tokens, errors: parser.errors }
}

fn collect_invalid_escape_warnings(src: &str, version: PythonVersion) -> Vec<Diagnostic> {
    tokenize_with(src, LexOptions { mode: LexMode::Parse, version })
        .filter_map(Result::ok)
        .filter_map(|token| {
            let TokenKind::String { prefix, triple } = token.kind else { return None };
            if prefix.is_raw() {
                return None;
            }
            let raw = &src[token.range];
            let quote = raw.find(['\'', '"'])?;
            let delimiter = if triple { 3 } else { 1 };
            let body_start = quote + delimiter;
            let body_end = raw.len().checked_sub(delimiter)?;
            let body = raw.get(body_start..body_end)?;
            invalid_escape_in(body, token.range.start().as_usize() + body_start)
        })
        .collect()
}

fn invalid_escape_in(value: &str, offset: usize) -> Option<Diagnostic> {
    let bytes = value.as_bytes();
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] != b'\\' {
            index += 1;
            continue;
        }
        let escape_start = index;
        index += 1;
        let valid = match bytes.get(index).copied() {
            Some(b'\\' | b'\'' | b'"' | b'a' | b'b' | b'f' | b'n' | b'r' | b't' | b'v' | b'\n') => {
                index += 1;
                true
            }
            Some(b'\r') => {
                index += 1;
                if bytes.get(index) == Some(&b'\n') {
                    index += 1;
                }
                true
            }
            Some(byte @ b'0'..=b'7') => {
                let mut value = (byte - b'0') as u16;
                index += 1;
                for _ in 0..2 {
                    if bytes.get(index).is_some_and(|next| (b'0'..=b'7').contains(next)) {
                        value = value * 8 + (bytes[index] - b'0') as u16;
                        index += 1;
                    } else {
                        break;
                    }
                }
                value <= 0o377
            }
            Some(b'x') => valid_hex_escape(bytes, &mut index, 2),
            Some(b'u') => valid_hex_escape(bytes, &mut index, 4),
            Some(b'U') => valid_hex_escape(bytes, &mut index, 8),
            Some(b'N') => valid_named_escape(bytes, &mut index),
            Some(_) | None => false,
        };
        if !valid {
            let end = index.max(escape_start + 1).min(value.len());
            return Some(Diagnostic::warning(
                DiagnosticCode::InvalidEscape,
                TextRange::from_usize(offset + escape_start, offset + end),
                "invalid escape sequence",
            ));
        }
    }
    None
}

fn valid_hex_escape(bytes: &[u8], index: &mut usize, count: usize) -> bool {
    *index += 1;
    let start = *index;
    while *index < bytes.len() && *index - start < count && bytes[*index].is_ascii_hexdigit() {
        *index += 1;
    }
    *index - start == count
}

fn valid_named_escape(bytes: &[u8], index: &mut usize) -> bool {
    *index += 1;
    if bytes.get(*index) != Some(&b'{') {
        return false;
    }
    *index += 1;
    let start = *index;
    while *index < bytes.len() && bytes[*index] != b'}' {
        *index += 1;
    }
    *index > start && bytes.get(*index) == Some(&b'}')
}

fn collect_comments(src: &str) -> Vec<Comment> {
    tokenize_with(src, LexOptions { mode: LexMode::Full, ..LexOptions::default() })
        .filter_map(|item| item.ok())
        .filter(|token| token.kind == TokenKind::Comment)
        .map(|token| Comment { range: token.range, text: src[token.range].into() })
        .collect()
}

fn collect_type_ignores(src: &str) -> Vec<TypeIgnore> {
    let index = LineIndex::new(src);
    tokenize_with(src, LexOptions { mode: LexMode::Full, ..LexOptions::default() })
        .filter_map(Result::ok)
        .filter(|token| token.kind == TokenKind::Comment)
        .filter_map(|token| {
            let comment = src.get(token.range.start().as_usize()..token.range.end().as_usize())?;
            let suffix = comment.strip_prefix('#')?.trim_start();
            let suffix = suffix.strip_prefix("type:")?.trim_start();
            let tag = suffix.strip_prefix("ignore")?;
            if !tag.is_empty() && !tag.starts_with('[') {
                return None;
            }
            let tag = if tag.starts_with('[') { tag.trim().to_owned() } else { "\n".into() };
            Some(TypeIgnore {
                range: token.range,
                lineno: index.line_col_chars(src, token.range.start()).line,
                tag: tag.into(),
            })
        })
        .collect()
}

struct Parser<'src> {
    src: &'src str,
    tokens: Vec<Token>,
    position: usize,
    options: ParseOptions,
    errors: Vec<Diagnostic>,
    depth: u32,
    expression_nodes: usize,
    grouped_expressions: HashSet<TextRange>,
    grouped_expression_ranges: HashMap<TextRange, TextRange>,
    grouped_pattern_ranges: HashMap<TextRange, TextRange>,
    stop_in: bool,
}

#[derive(Copy, Clone)]
enum TypeParamKind {
    TypeVar,
    ParamSpec,
    TypeVarTuple,
}

impl<'src> Parser<'src> {
    fn new(src: &'src str, options: ParseOptions) -> Self {
        let mut tokens = Vec::new();
        let mut errors = Vec::new();
        for item in
            tokenize_with(src, LexOptions { mode: LexMode::Parse, version: options.version })
        {
            match item {
                Ok(token) => tokens.push(token),
                Err(error) => errors.push(error.diagnostic),
            }
        }
        if !matches!(tokens.last().map(|token| token.kind), Some(TokenKind::EndMarker)) {
            tokens.push(Token::new(
                TokenKind::EndMarker,
                TextRange::empty(TextRange::from_usize(src.len(), src.len()).start()),
            ));
        }
        Self {
            src,
            tokens,
            position: 0,
            options,
            errors,
            depth: 0,
            expression_nodes: 0,
            grouped_expressions: HashSet::new(),
            grouped_expression_ranges: HashMap::new(),
            grouped_pattern_ranges: HashMap::new(),
            stop_in: false,
        }
    }

    fn parse_module_inner(&mut self) -> Result<ModModule, ParseError> {
        let mut body = Vec::new();
        self.skip_newlines();
        while !self.at(TokenKind::EndMarker) {
            let before = self.position;
            match self.statement() {
                Ok(stmt) => body.push(stmt),
                Err(error) if self.options.parse_mode == ParseMode::Recover => {
                    self.push_error(error.diagnostic.clone());
                    let range = error.range();
                    body.push(Stmt::Invalid(StmtInvalid {
                        range,
                        message: error.diagnostic.message.into(),
                    }));
                    self.recover_statement();
                }
                Err(error) => return Err(error),
            }
            if self.position == before {
                self.bump();
            }
            self.skip_newlines();
            if self.errors.len() >= self.options.max_errors {
                break;
            }
        }
        Ok(ModModule {
            body,
            type_ignores: if self.options.type_comments {
                collect_type_ignores(self.src)
            } else {
                Vec::new()
            },
            range: TextRange::from_usize(0, self.src.len()),
            source: Some(self.src.into()),
        })
    }

    fn statement(&mut self) -> Result<Stmt, ParseError> {
        if self.enter_depth()? {
            return Err(ParseError::too_deep(self.current().range));
        }
        let result = match self.current().kind {
            TokenKind::Def => self.function(false),
            TokenKind::Class => self.class(),
            TokenKind::At => self.decorated_statement(),
            TokenKind::If => self.if_statement(),
            TokenKind::While => self.while_statement(),
            TokenKind::For => self.for_statement(false),
            TokenKind::With => self.with_statement(false),
            TokenKind::Try => self.try_statement(),
            TokenKind::Return => self.return_statement(),
            TokenKind::Raise => self.raise_statement(),
            TokenKind::Assert => self.assert_statement(),
            TokenKind::Import => self.import_statement(),
            TokenKind::From => self.import_from_statement(),
            TokenKind::Global => self.names_statement(true),
            TokenKind::Nonlocal => self.names_statement(false),
            TokenKind::Pass => self.simple_statement(),
            TokenKind::Break => self.simple_statement(),
            TokenKind::Continue => self.simple_statement(),
            TokenKind::Del => self.delete_statement(),
            TokenKind::Async => self.async_statement(),
            TokenKind::Name if self.word_is("match") => self.try_match_statement(),
            TokenKind::Name if self.word_is("type") && self.looks_like_type_alias() => {
                self.type_alias_statement()
            }
            _ => self.simple_or_assignment(),
        };
        self.leave_depth();
        result
    }

    fn async_statement(&mut self) -> Result<Stmt, ParseError> {
        if self.peek_kind(1) == TokenKind::Def {
            self.function(true)
        } else if self.peek_kind(1) == TokenKind::For {
            self.for_statement(true)
        } else if self.peek_kind(1) == TokenKind::With {
            self.with_statement(true)
        } else {
            self.simple_or_assignment()
        }
    }

    fn decorated_statement(&mut self) -> Result<Stmt, ParseError> {
        let mut decorators = Vec::new();
        while self.eat(TokenKind::At) {
            decorators.push(self.expression(0)?);
            self.expect(TokenKind::Newline)?;
        }
        let mut statement = match self.current().kind {
            TokenKind::Def => self.function(false)?,
            TokenKind::Async if self.peek_kind(1) == TokenKind::Def => self.function(true)?,
            TokenKind::Class => self.class()?,
            _ => return Err(self.error_here("expected function or class after decorator")),
        };
        match &mut statement {
            Stmt::FunctionDef(node) | Stmt::AsyncFunctionDef(node) => {
                node.decorator_list = decorators
            }
            Stmt::ClassDef(node) => node.decorator_list = decorators,
            _ => {}
        }
        Ok(statement)
    }

    fn try_match_statement(&mut self) -> Result<Stmt, ParseError> {
        let checkpoint = self.position;
        let error_count = self.errors.len();
        match self.match_statement() {
            Ok(statement) => Ok(statement),
            Err(_) => {
                self.position = checkpoint;
                self.errors.truncate(error_count);
                self.simple_or_assignment()
            }
        }
    }

    fn match_statement(&mut self) -> Result<Stmt, ParseError> {
        let start = self.current().range.start();
        let keyword = self.expect(TokenKind::Name)?;
        if self.name_text(keyword) != "match" {
            return Err(self.error_here("expected match"));
        }
        let subject_first = self.expression(0)?;
        let subject = if self.at(TokenKind::Comma) {
            self.comma_expression(subject_first)?
        } else {
            subject_first
        };
        self.expect(TokenKind::Colon)?;
        if !self.options.version.supports(PythonVersion::Py310) {
            self.push_error(Diagnostic::unsupported(
                self.previous().range,
                "match statements require Python 3.10 or newer",
            ));
        }
        self.expect(TokenKind::Newline)?;
        self.expect(TokenKind::Indent)?;
        let mut cases = Vec::new();
        self.skip_newlines();
        while self.at_word("case") {
            let case_start = self.bump().range.start();
            let pattern = self.case_pattern()?;
            let guard = if self.at(TokenKind::If) {
                self.bump();
                Some(self.expression(0)?)
            } else {
                None
            };
            self.expect(TokenKind::Colon)?;
            let body = self.block()?;
            let end = body.last().map(Ranged::range).unwrap_or_else(|| self.previous().range).end();
            cases.push(MatchCase { range: TextRange::new(case_start, end), pattern, guard, body });
            self.skip_newlines();
        }
        self.expect(TokenKind::Dedent)?;
        let end = cases.last().map(Ranged::range).unwrap_or_else(|| self.previous().range).end();
        Ok(Stmt::Match(StmtMatch {
            range: TextRange::new(start, end),
            subject: Box::new(subject),
            cases,
        }))
    }

    fn type_alias_statement(&mut self) -> Result<Stmt, ParseError> {
        let start = self.bump().range.start();
        let name_token = self.expect(TokenKind::Name)?;
        if !self.options.version.supports(PythonVersion::Py312) {
            self.push_error(Diagnostic::unsupported(
                TextRange::new(start, name_token.range.end()),
                "type statements require Python 3.12 or newer",
            ));
        }
        let name = Expr::Name(ExprName {
            range: name_token.range,
            id: normalize_identifier(self.name_text(name_token)),
            ctx: ExprContext::Store,
        });
        let type_params = self.type_parameters()?;
        self.expect(TokenKind::Equal)?;
        let value = Box::new(self.expression(0)?);
        let end = self.statement_end(value.range().end());
        self.consume_line_end();
        Ok(Stmt::TypeAlias(StmtTypeAlias {
            range: TextRange::new(start, end),
            name: Box::new(name),
            type_params,
            value,
        }))
    }

    fn function(&mut self, is_async: bool) -> Result<Stmt, ParseError> {
        let start = self.current().range.start();
        if is_async {
            self.expect(TokenKind::Async)?;
        }
        self.expect(TokenKind::Def)?;
        let name_token = self.expect(TokenKind::Name)?;
        let name = normalize_identifier(self.name_text(name_token));
        let type_params = self.type_parameters()?;
        let args = self.parameters()?;
        let returns =
            if self.eat(TokenKind::Arrow) { Some(Box::new(self.expression(0)?)) } else { None };
        self.expect(TokenKind::Colon)?;
        let type_comment = self.trailing_type_comment(self.previous().range.end());
        let body = self.block()?;
        let end = body.last().map(Ranged::range).unwrap_or_else(|| self.previous().range).end();
        let range = TextRange::new(start, self.suite_end(end));
        let node = StmtFunctionDef {
            range,
            name,
            decorator_list: Vec::new(),
            type_params,
            args,
            returns,
            body,
            type_comment,
        };
        Ok(if is_async {
            Stmt::AsyncFunctionDef(Box::new(node))
        } else {
            Stmt::FunctionDef(Box::new(node))
        })
    }

    fn class(&mut self) -> Result<Stmt, ParseError> {
        let start = self.expect(TokenKind::Class)?.range.start();
        let name_token = self.expect(TokenKind::Name)?;
        let name = normalize_identifier(self.name_text(name_token));
        let type_params = self.type_parameters()?;
        let mut bases = Vec::new();
        let mut keywords = Vec::new();
        if self.eat(TokenKind::LPar) {
            while !self.at(TokenKind::RPar) && !self.at(TokenKind::EndMarker) {
                if self.eat(TokenKind::DoubleStar) {
                    let starstar = self.previous();
                    let value = self.expression(0)?;
                    keywords.push(Keyword {
                        range: TextRange::new(
                            starstar.range.start(),
                            self.expression_range(&value).end(),
                        ),
                        arg: None,
                        value,
                    });
                    if !self.eat(TokenKind::Comma) {
                        break;
                    }
                    continue;
                }
                let value = self.expression(0)?;
                if let Expr::Name(name_node) = &value {
                    if self.eat(TokenKind::Equal) {
                        let keyword_value = self.expression(0)?;
                        keywords.push(Keyword {
                            range: TextRange::new(
                                value.range().start(),
                                self.expression_range(&keyword_value).end(),
                            ),
                            arg: Some(name_node.id.clone()),
                            value: keyword_value,
                        });
                    } else {
                        bases.push(value);
                    }
                } else {
                    bases.push(value);
                }
                if !self.eat(TokenKind::Comma) {
                    break;
                }
            }
            self.expect(TokenKind::RPar)?;
        }
        self.expect(TokenKind::Colon)?;
        let body = self.block()?;
        let end = body.last().map(Ranged::range).unwrap_or_else(|| self.previous().range).end();
        let range = TextRange::new(start, self.suite_end(end));
        Ok(Stmt::ClassDef(Box::new(StmtClassDef {
            range,
            name,
            bases,
            keywords,
            decorator_list: Vec::new(),
            type_params,
            body,
        })))
    }

    fn if_statement(&mut self) -> Result<Stmt, ParseError> {
        let start = self.expect(TokenKind::If)?.range.start();
        let test = Box::new(self.expression(0)?);
        self.expect(TokenKind::Colon)?;
        let body = self.block()?;
        let orelse = if self.at(TokenKind::Elif) {
            vec![self.elif_statement()?]
        } else if self.eat(TokenKind::Else) {
            self.expect(TokenKind::Colon)?;
            self.block()?
        } else {
            Vec::new()
        };
        let end = orelse
            .last()
            .map(Ranged::range)
            .or_else(|| body.last().map(Ranged::range))
            .unwrap_or_else(|| self.previous().range)
            .end();
        let end = self.suite_end(end);
        Ok(Stmt::If(StmtIf { range: TextRange::new(start, end), test, body, orelse }))
    }

    fn elif_statement(&mut self) -> Result<Stmt, ParseError> {
        let start = self.expect(TokenKind::Elif)?.range.start();
        let test = Box::new(self.expression(0)?);
        self.expect(TokenKind::Colon)?;
        let body = self.block()?;
        let orelse = if self.at(TokenKind::Elif) {
            vec![self.elif_statement()?]
        } else if self.eat(TokenKind::Else) {
            self.expect(TokenKind::Colon)?;
            self.block()?
        } else {
            Vec::new()
        };
        let end = orelse
            .last()
            .map(Ranged::range)
            .or_else(|| body.last().map(Ranged::range))
            .unwrap_or_else(|| self.previous().range)
            .end();
        let end = self.suite_end(end);
        Ok(Stmt::If(StmtIf { range: TextRange::new(start, end), test, body, orelse }))
    }

    fn while_statement(&mut self) -> Result<Stmt, ParseError> {
        let start = self.expect(TokenKind::While)?.range.start();
        let test = Box::new(self.expression(0)?);
        self.expect(TokenKind::Colon)?;
        let body = self.block()?;
        let orelse = if self.eat(TokenKind::Else) {
            self.expect(TokenKind::Colon)?;
            self.block()?
        } else {
            Vec::new()
        };
        let end = orelse
            .last()
            .map(Ranged::range)
            .or_else(|| body.last().map(Ranged::range))
            .unwrap_or_else(|| self.previous().range)
            .end();
        let end = self.suite_end(end);
        Ok(Stmt::While(StmtWhile { range: TextRange::new(start, end), test, body, orelse }))
    }

    fn for_statement(&mut self, is_async: bool) -> Result<Stmt, ParseError> {
        let start = self.current().range.start();
        if is_async {
            self.expect(TokenKind::Async)?;
        }
        self.expect(TokenKind::For)?;
        self.stop_in = true;
        let target_expression = self.expression(0)?;
        let target = mark_store(self.comma_expression(target_expression)?)?;
        self.stop_in = false;
        self.expect(TokenKind::In)?;
        let iter_first = self.expression(0)?;
        let iter =
            if self.at(TokenKind::Comma) { self.comma_expression(iter_first)? } else { iter_first };
        self.expect(TokenKind::Colon)?;
        let type_comment = self.trailing_type_comment(self.previous().range.end());
        let body = self.block()?;
        let orelse = if self.eat(TokenKind::Else) {
            self.expect(TokenKind::Colon)?;
            self.block()?
        } else {
            Vec::new()
        };
        let end = orelse
            .last()
            .map(Ranged::range)
            .or_else(|| body.last().map(Ranged::range))
            .unwrap_or_else(|| self.previous().range)
            .end();
        let end = self.suite_end(end);
        let node = StmtFor {
            range: TextRange::new(start, end),
            target: Box::new(target),
            iter: Box::new(iter),
            body,
            orelse,
            type_comment,
        };
        Ok(if is_async { Stmt::AsyncFor(node) } else { Stmt::For(node) })
    }

    fn with_statement(&mut self, is_async: bool) -> Result<Stmt, ParseError> {
        let start = self.current().range.start();
        if is_async {
            self.expect(TokenKind::Async)?;
        }
        self.expect(TokenKind::With)?;
        let mut items = Vec::new();
        let parenthesized = self.at(TokenKind::LPar) && self.has_top_level_with_separator();
        if parenthesized {
            if !self.options.version.supports(PythonVersion::Py310) {
                self.push_error(Diagnostic::unsupported(
                    self.current().range,
                    "parenthesized with statements require Python 3.10 or newer",
                ));
            }
            self.bump();
        }
        loop {
            let context_expr = self.expression(0)?;
            let optional_vars =
                if self.eat(TokenKind::As) { Some(mark_store(self.expression(0)?)?) } else { None };
            let end = optional_vars
                .as_ref()
                .map(Ranged::range)
                .unwrap_or_else(|| context_expr.range())
                .end();
            items.push(WithItem {
                range: TextRange::new(context_expr.range().start(), end),
                context_expr,
                optional_vars,
            });
            if !self.eat(TokenKind::Comma) {
                break;
            }
            if parenthesized && self.at(TokenKind::RPar) {
                break;
            }
        }
        if parenthesized {
            self.expect(TokenKind::RPar)?;
        }
        self.expect(TokenKind::Colon)?;
        let type_comment = self.trailing_type_comment(self.previous().range.end());
        let body = self.block()?;
        let end = body.last().map(Ranged::range).unwrap_or_else(|| self.previous().range).end();
        let end = self.suite_end(end);
        let node = StmtWith { range: TextRange::new(start, end), items, body, type_comment };
        Ok(if is_async { Stmt::AsyncWith(node) } else { Stmt::With(node) })
    }

    fn try_statement(&mut self) -> Result<Stmt, ParseError> {
        let start = self.expect(TokenKind::Try)?.range.start();
        self.expect(TokenKind::Colon)?;
        let body = self.block()?;
        let mut handlers = Vec::new();
        let mut is_star = false;
        while self.eat(TokenKind::Except) {
            let handler_start = self.previous().range.start();
            let handler_is_star = self.eat(TokenKind::Star);
            if handler_is_star {
                is_star = true;
                if !self.options.version.supports(PythonVersion::Py311) {
                    self.push_error(Diagnostic::unsupported(
                        self.previous().range,
                        "except* requires Python 3.11 or newer",
                    ));
                }
            }
            let typ = if self.at(TokenKind::Colon) || self.at(TokenKind::As) {
                None
            } else {
                Some(self.expression(0)?)
            };
            let name = if self.eat(TokenKind::As) {
                let name_token = self.expect(TokenKind::Name)?;
                Some(self.name_text(name_token).to_owned().into())
            } else {
                None
            };
            self.expect(TokenKind::Colon)?;
            let handler_body = self.block()?;
            let end = handler_body
                .last()
                .map(Ranged::range)
                .unwrap_or_else(|| self.previous().range)
                .end();
            let end = self.suite_end(end);
            handlers.push(ExceptHandler {
                range: TextRange::new(handler_start, end),
                typ,
                name,
                body: handler_body,
            });
        }
        let orelse = if self.eat(TokenKind::Else) {
            self.expect(TokenKind::Colon)?;
            self.block()?
        } else {
            Vec::new()
        };
        let finalbody = if self.eat(TokenKind::Finally) {
            self.expect(TokenKind::Colon)?;
            self.block()?
        } else {
            Vec::new()
        };
        if handlers.is_empty() && finalbody.is_empty() {
            return Err(self.error_here("expected except or finally"));
        }
        let end = finalbody
            .last()
            .map(Ranged::range)
            .or_else(|| orelse.last().map(Ranged::range))
            .or_else(|| handlers.last().map(Ranged::range))
            .or_else(|| body.last().map(Ranged::range))
            .unwrap_or_else(|| self.previous().range)
            .end();
        let end = self.suite_end(end);
        let node = StmtTry { range: TextRange::new(start, end), body, handlers, orelse, finalbody };
        Ok(if is_star { Stmt::TryStar(Box::new(node)) } else { Stmt::Try(Box::new(node)) })
    }

    fn return_statement(&mut self) -> Result<Stmt, ParseError> {
        let start = self.expect(TokenKind::Return)?.range.start();
        let value = if self.at_line_end() {
            None
        } else {
            let first = self.expression(0)?;
            let value =
                if self.at(TokenKind::Comma) { self.comma_expression(first)? } else { first };
            Some(Box::new(value))
        };
        let end = value
            .as_ref()
            .map(|value| value.range())
            .unwrap_or_else(|| self.previous().range)
            .end();
        let end = self.statement_end(end);
        self.consume_line_end();
        Ok(Stmt::Return(StmtReturn { range: TextRange::new(start, end), value }))
    }

    fn raise_statement(&mut self) -> Result<Stmt, ParseError> {
        let start = self.expect(TokenKind::Raise)?.range.start();
        let exc = if self.at_line_end() { None } else { Some(Box::new(self.expression(0)?)) };
        let cause =
            if self.eat(TokenKind::From) { Some(Box::new(self.expression(0)?)) } else { None };
        let end = cause
            .as_ref()
            .or(exc.as_ref())
            .map(|value| value.range())
            .unwrap_or_else(|| self.previous().range)
            .end();
        let end = self.statement_end(end);
        self.consume_line_end();
        Ok(Stmt::Raise(StmtRaise { range: TextRange::new(start, end), exc, cause }))
    }

    fn assert_statement(&mut self) -> Result<Stmt, ParseError> {
        let start = self.expect(TokenKind::Assert)?.range.start();
        let test = Box::new(self.expression(0)?);
        let msg =
            if self.eat(TokenKind::Comma) { Some(Box::new(self.expression(0)?)) } else { None };
        let end = msg.as_ref().map(|value| value.range()).unwrap_or_else(|| test.range()).end();
        let end = self.statement_end(end);
        self.consume_line_end();
        Ok(Stmt::Assert(StmtAssert { range: TextRange::new(start, end), test, msg }))
    }

    fn import_statement(&mut self) -> Result<Stmt, ParseError> {
        let start = self.expect(TokenKind::Import)?.range.start();
        let mut names = Vec::new();
        loop {
            let name_start = self.current().range.start();
            let name = self.dotted_name()?;
            let asname = if self.eat(TokenKind::As) {
                let as_token = self.expect(TokenKind::Name)?;
                Some(self.name_text(as_token).to_owned().into())
            } else {
                None
            };
            let end = self.previous().range.end();
            names.push(Alias { range: TextRange::new(name_start, end), name: name.into(), asname });
            if !self.eat(TokenKind::Comma) {
                break;
            }
        }
        let end = self.statement_end(names.last().map(Ranged::range).unwrap_or_default().end());
        self.consume_line_end();
        Ok(Stmt::Import(StmtImport { range: TextRange::new(start, end), names }))
    }

    fn import_from_statement(&mut self) -> Result<Stmt, ParseError> {
        let start = self.expect(TokenKind::From)?.range.start();
        let mut level = 0;
        loop {
            if self.eat(TokenKind::Dot) {
                level += 1;
            } else if self.eat(TokenKind::Ellipsis) {
                level += 3;
            } else {
                break;
            }
        }
        let module = if self.at(TokenKind::Name) { Some(self.dotted_name()?.into()) } else { None };
        self.expect(TokenKind::Import)?;
        let mut names = Vec::new();
        if self.eat(TokenKind::Star) {
            names.push(Alias { range: self.previous().range, name: "*".into(), asname: None });
        } else {
            let parenthesized = self.eat(TokenKind::LPar);
            loop {
                let name_start = self.current().range.start();
                let name_token = self.expect(TokenKind::Name)?;
                let name = normalize_identifier(self.name_text(name_token));
                let asname = if self.eat(TokenKind::As) {
                    let as_token = self.expect(TokenKind::Name)?;
                    Some(self.name_text(as_token).to_owned().into())
                } else {
                    None
                };
                let end = self.previous().range.end();
                names.push(Alias { range: TextRange::new(name_start, end), name, asname });
                if !self.eat(TokenKind::Comma) {
                    break;
                }
                if parenthesized && self.at(TokenKind::RPar) {
                    break;
                }
            }
            if parenthesized {
                self.expect(TokenKind::RPar)?;
            }
        }
        let end = self.statement_end(names.last().map(Ranged::range).unwrap_or_default().end());
        self.consume_line_end();
        Ok(Stmt::ImportFrom(StmtImportFrom {
            range: TextRange::new(start, end),
            module,
            names,
            level,
        }))
    }

    fn names_statement(&mut self, global: bool) -> Result<Stmt, ParseError> {
        let start = self.bump().range.start();
        let mut names = Vec::new();
        let end = loop {
            let token = self.expect(TokenKind::Name)?;
            names.push(self.name_text(token).to_owned().into());
            if !self.eat(TokenKind::Comma) {
                break token.range.end();
            }
        };
        self.consume_line_end();
        let node = StmtNames { range: TextRange::new(start, end), names };
        Ok(if global { Stmt::Global(node) } else { Stmt::Nonlocal(node) })
    }

    fn simple_statement(&mut self) -> Result<Stmt, ParseError> {
        let token = self.bump();
        self.consume_line_end();
        Ok(match token.kind {
            TokenKind::Pass => Stmt::Pass(StmtSimple { range: token.range }),
            TokenKind::Break => Stmt::Break(StmtSimple { range: token.range }),
            TokenKind::Continue => Stmt::Continue(StmtSimple { range: token.range }),
            _ => Stmt::Invalid(StmtInvalid {
                range: token.range,
                message: "unknown simple statement".into(),
            }),
        })
    }

    fn delete_statement(&mut self) -> Result<Stmt, ParseError> {
        let start = self.expect(TokenKind::Del)?.range.start();
        let first = self.expression(0)?;
        let mut targets = vec![mark_delete(first)?];
        while self.eat(TokenKind::Comma) {
            if self.at_line_end() {
                break;
            }
            targets.push(mark_delete(self.expression(0)?)?);
        }
        let end = targets.last().map(Ranged::range).unwrap_or_else(|| self.previous().range).end();
        let end = self.statement_end(end);
        self.consume_line_end();
        Ok(Stmt::Delete(StmtDelete { range: TextRange::new(start, end), targets }))
    }

    fn simple_or_assignment(&mut self) -> Result<Stmt, ParseError> {
        let mut first = self.expression(0)?;
        if self.at(TokenKind::Comma) {
            first = self.comma_expression(first)?;
        }
        let first_start = self.expression_range(&first).start();
        if self.eat(TokenKind::Colon) {
            let target = mark_store(first)?;
            let annotation = Box::new(self.expression(0)?);
            let value =
                if self.eat(TokenKind::Equal) { Some(Box::new(self.expression(0)?)) } else { None };
            let end = value
                .as_ref()
                .map(|value| value.range())
                .unwrap_or_else(|| annotation.range())
                .end();
            let end = self.statement_end(end);
            self.consume_line_end();
            let simple = matches!(target, Expr::Name(_))
                && !self.grouped_expressions.contains(&target.range());
            return Ok(Stmt::AnnAssign(StmtAnnAssign {
                range: TextRange::new(first_start, end),
                target: Box::new(target),
                annotation,
                value,
                simple,
            }));
        }
        if let Some(op) = aug_operator(self.current().kind) {
            self.bump();
            let value_first = self.expression(0)?;
            let value = if self.at(TokenKind::Comma) {
                self.comma_expression(value_first)?
            } else {
                value_first
            };
            let end = self.statement_end(value.range().end());
            self.consume_line_end();
            let target = mark_store(first)?;
            return Ok(Stmt::AugAssign(StmtAugAssign {
                range: TextRange::new(first_start, end),
                target: Box::new(target),
                op,
                value: Box::new(value),
            }));
        }
        if self.eat(TokenKind::Equal) {
            let mut targets = vec![mark_store(first)?];
            let first_value = self.expression(0)?;
            let mut value = self.comma_expression(first_value)?;
            while self.eat(TokenKind::Equal) {
                targets.push(mark_store(value)?);
                let next_value = self.expression(0)?;
                value = self.comma_expression(next_value)?;
            }
            let end = self.statement_end(value.range().end());
            self.consume_line_end();
            let type_comment = self.trailing_type_comment(end);
            return Ok(Stmt::Assign(StmtAssign {
                range: TextRange::new(first_start, end),
                targets,
                value: Box::new(value),
                type_comment,
            }));
        }
        self.consume_line_end();
        Ok(Stmt::Expr(StmtExpr { range: self.expression_range(&first), value: Box::new(first) }))
    }

    fn block(&mut self) -> Result<Vec<Stmt>, ParseError> {
        if self.eat(TokenKind::Newline) {
            self.expect(TokenKind::Indent)?;
            let mut result = Vec::new();
            self.skip_newlines();
            while !self.at(TokenKind::Dedent) && !self.at(TokenKind::EndMarker) {
                result.push(self.statement()?);
                self.skip_newlines();
            }
            self.eat(TokenKind::Dedent);
            Ok(result)
        } else {
            let mut statements = Vec::new();
            loop {
                statements.push(self.statement()?);
                if self.at(TokenKind::Newline) || self.at(TokenKind::EndMarker) {
                    break;
                }
                if !self.eat(TokenKind::Semi) {
                    break;
                }
            }
            Ok(statements)
        }
    }

    fn parameters(&mut self) -> Result<Parameters, ParseError> {
        self.expect(TokenKind::LPar)?;
        let mut parameters = Parameters::default();
        let mut seen_default = false;
        let mut keyword_only = false;
        while !self.at(TokenKind::RPar) && !self.at(TokenKind::EndMarker) {
            if self.eat(TokenKind::Slash) {
                parameters.posonlyargs.append(&mut parameters.args);
                self.eat(TokenKind::Comma);
                continue;
            }
            if self.eat(TokenKind::DoubleStar) {
                let token = self.expect(TokenKind::Name)?;
                let annotation = if self.eat(TokenKind::Colon) {
                    Some(Box::new(self.expression(0)?))
                } else {
                    None
                };
                let end = annotation
                    .as_ref()
                    .map(|value| self.expression_range(value).end())
                    .unwrap_or(token.range.end());
                parameters.kwarg = Some(Parameter {
                    range: TextRange::new(token.range.start(), end),
                    name: normalize_identifier(self.name_text(token)),
                    annotation,
                    default: None,
                    type_comment: None,
                });
                self.eat(TokenKind::Comma);
                continue;
            } else if self.eat(TokenKind::Star) {
                if self.at(TokenKind::Name) {
                    let token = self.bump();
                    let annotation = if self.eat(TokenKind::Colon) {
                        Some(Box::new(self.expression(0)?))
                    } else {
                        None
                    };
                    let end = annotation
                        .as_ref()
                        .map(|value| self.expression_range(value).end())
                        .unwrap_or(token.range.end());
                    parameters.vararg = Some(Parameter {
                        range: TextRange::new(token.range.start(), end),
                        name: normalize_identifier(self.name_text(token)),
                        annotation,
                        default: None,
                        type_comment: None,
                    });
                    keyword_only = true;
                } else {
                    keyword_only = true;
                }
            } else if self.at(TokenKind::Name) {
                let token = self.bump();
                let name = normalize_identifier(self.name_text(token));
                let annotation = if self.eat(TokenKind::Colon) {
                    Some(Box::new(self.expression(0)?))
                } else {
                    None
                };
                let default = if self.eat(TokenKind::Equal) {
                    if !keyword_only {
                        seen_default = true;
                    }
                    Some(Box::new(self.expression(0)?))
                } else {
                    if seen_default && !keyword_only {
                        return Err(
                            self.error_here("non-default argument follows default argument")
                        );
                    }
                    None
                };
                let end = annotation
                    .as_ref()
                    .map(|value| self.expression_range(value).end())
                    .unwrap_or(token.range.end());
                let parameter = Parameter {
                    range: TextRange::new(token.range.start(), end),
                    name,
                    annotation,
                    default: default.clone(),
                    type_comment: None,
                };
                if keyword_only {
                    parameters.kwonlyargs.push(parameter);
                    parameters.kw_defaults.push(default.map(|value| *value));
                } else {
                    if let Some(default) = &default {
                        parameters.defaults.push((**default).clone());
                    }
                    parameters.args.push(parameter);
                }
            } else {
                return Err(self.error_here("expected parameter"));
            }
            if !self.eat(TokenKind::Comma) {
                break;
            }
        }
        self.expect(TokenKind::RPar)?;
        Ok(parameters)
    }

    fn pattern(&mut self) -> Result<Pattern, ParseError> {
        let start = self.current().range.start();
        let first = self.pattern_atom()?;
        let mut patterns = vec![first];
        while self.eat(TokenKind::Vbar) {
            patterns.push(self.pattern_atom()?);
        }
        let mut pattern = if patterns.len() == 1 {
            patterns
                .pop()
                .unwrap_or(Pattern::Invalid(PatternInvalid { range: TextRange::empty(start) }))
        } else {
            let end = patterns
                .last()
                .map(|pattern| self.pattern_range(pattern).end())
                .unwrap_or_default();
            Pattern::Or(PatternOr { range: TextRange::new(start, end), patterns })
        };
        if self.eat(TokenKind::As) {
            let name = self.expect(TokenKind::Name)?;
            let range = TextRange::new(self.pattern_range(&pattern).start(), name.range.end());
            pattern = Pattern::As(PatternAs {
                range,
                pattern: Some(Box::new(pattern)),
                name: Some(self.name_text(name).to_owned().into()),
            });
        }
        Ok(pattern)
    }

    fn case_pattern(&mut self) -> Result<Pattern, ParseError> {
        let first = self.pattern()?;
        if !self.eat(TokenKind::Comma) {
            return Ok(first);
        }
        let start = first.range().start();
        let mut patterns = vec![first];
        let mut trailing_comma_end = Some(self.previous().range.end());
        while !matches!(
            self.current().kind,
            TokenKind::Colon | TokenKind::If | TokenKind::EndMarker
        ) {
            patterns.push(self.pattern()?);
            if !self.eat(TokenKind::Comma) {
                break;
            }
            trailing_comma_end = Some(self.previous().range.end());
        }
        let end = patterns.last().map(Ranged::range).unwrap_or_default().end();
        let end = trailing_comma_end.map_or(end, |comma| end.max(comma));
        Ok(Pattern::Sequence(PatternSequence { range: TextRange::new(start, end), patterns }))
    }

    fn pattern_atom(&mut self) -> Result<Pattern, ParseError> {
        let start = self.current().range.start();
        match self.current().kind {
            TokenKind::Name if self.word_is("_") => {
                self.bump();
                Ok(Pattern::As(PatternAs {
                    range: self.previous().range,
                    pattern: None,
                    name: None,
                }))
            }
            TokenKind::Name
                if self.peek_kind(1) == TokenKind::LPar || self.peek_kind(1) == TokenKind::Dot =>
            {
                let cls = self.pattern_dotted_expression()?;
                if self.eat(TokenKind::LPar) {
                    self.class_pattern(start, cls)
                } else {
                    Ok(Pattern::Value(PatternValue { range: cls.range(), value: cls }))
                }
            }
            TokenKind::Name => {
                let token = self.bump();
                Ok(Pattern::As(PatternAs {
                    range: token.range,
                    pattern: None,
                    name: Some(self.name_text(token).to_owned().into()),
                }))
            }
            TokenKind::LSqb => self.sequence_pattern(),
            TokenKind::LBrace => self.mapping_pattern(),
            TokenKind::LPar => {
                self.bump();
                if self.at(TokenKind::RPar) {
                    let end = self.bump().range.end();
                    return Ok(Pattern::Sequence(PatternSequence {
                        range: TextRange::new(start, end),
                        patterns: Vec::new(),
                    }));
                }
                let first = self.pattern()?;
                if !self.eat(TokenKind::Comma) {
                    let end = self.expect(TokenKind::RPar)?.range.end();
                    self.grouped_pattern_ranges.insert(first.range(), TextRange::new(start, end));
                    return Ok(first);
                }
                let mut patterns = vec![first];
                while !self.at(TokenKind::RPar) && !self.at(TokenKind::EndMarker) {
                    patterns.push(self.pattern()?);
                    if !self.eat(TokenKind::Comma) {
                        break;
                    }
                }
                let end = self.expect(TokenKind::RPar)?.range.end();
                Ok(Pattern::Sequence(PatternSequence {
                    range: TextRange::new(start, end),
                    patterns,
                }))
            }
            TokenKind::Star => {
                let token = self.bump();
                let name = self.expect(TokenKind::Name)?;
                Ok(Pattern::Star(PatternStar {
                    range: TextRange::new(token.range.start(), name.range.end()),
                    name: if self.name_text(name) == "_" {
                        None
                    } else {
                        Some(self.name_text(name).to_owned().into())
                    },
                }))
            }
            TokenKind::True | TokenKind::False | TokenKind::None => {
                let token = self.bump();
                let value = match token.kind {
                    TokenKind::True | TokenKind::False => Expr::BooleanLiteral(ExprBoolean {
                        range: token.range,
                        value: token.kind == TokenKind::True,
                    }),
                    _ => Expr::NoneLiteral(ExprLiteral { range: token.range }),
                };
                Ok(Pattern::Singleton(PatternSingleton { range: token.range, value }))
            }
            _ => {
                let value = self.pattern_value_expression()?;
                let range = value.range();
                Ok(Pattern::Value(PatternValue { range, value }))
            }
        }
    }

    fn pattern_value_expression(&mut self) -> Result<Expr, ParseError> {
        self.expression(6)
    }

    fn pattern_dotted_expression(&mut self) -> Result<Expr, ParseError> {
        let token = self.expect(TokenKind::Name)?;
        let mut expression = Expr::Name(ExprName {
            range: token.range,
            id: normalize_identifier(self.name_text(token)),
            ctx: ExprContext::Load,
        });
        while self.eat(TokenKind::Dot) {
            let name = self.expect(TokenKind::Name)?;
            let range = TextRange::new(expression.range().start(), name.range.end());
            expression = Expr::Attribute(ExprAttribute {
                range,
                value: Box::new(expression),
                attr: normalize_identifier(self.name_text(name)),
                ctx: ExprContext::Load,
            });
        }
        Ok(expression)
    }

    fn sequence_pattern(&mut self) -> Result<Pattern, ParseError> {
        let start = self.expect(TokenKind::LSqb)?.range.start();
        let mut patterns = Vec::new();
        while !self.at(TokenKind::RSqb) && !self.at(TokenKind::EndMarker) {
            patterns.push(self.pattern()?);
            if !self.eat(TokenKind::Comma) {
                break;
            }
        }
        let end = self.expect(TokenKind::RSqb)?.range.end();
        Ok(Pattern::Sequence(PatternSequence { range: TextRange::new(start, end), patterns }))
    }

    fn mapping_pattern(&mut self) -> Result<Pattern, ParseError> {
        let start = self.expect(TokenKind::LBrace)?.range.start();
        let mut keys = Vec::new();
        let mut patterns = Vec::new();
        let mut rest = None;
        while !self.at(TokenKind::RBrace) && !self.at(TokenKind::EndMarker) {
            if self.eat(TokenKind::DoubleStar) {
                let name = self.expect(TokenKind::Name)?;
                rest = Some(self.name_text(name).to_owned().into());
            } else {
                keys.push(self.pattern_value_expression()?);
                self.expect(TokenKind::Colon)?;
                patterns.push(self.pattern()?);
            }
            if !self.eat(TokenKind::Comma) {
                break;
            }
        }
        let end = self.expect(TokenKind::RBrace)?.range.end();
        Ok(Pattern::Mapping(PatternMapping {
            range: TextRange::new(start, end),
            keys,
            patterns,
            rest,
        }))
    }

    fn class_pattern(&mut self, start: TextSize, cls: Expr) -> Result<Pattern, ParseError> {
        let mut positional = Vec::new();
        let mut kwd_attrs = Vec::new();
        let mut kwd_patterns = Vec::new();
        while !self.at(TokenKind::RPar) && !self.at(TokenKind::EndMarker) {
            if self.at(TokenKind::Name) && self.peek_kind(1) == TokenKind::Equal {
                let attr = self.bump();
                self.bump();
                kwd_attrs.push(self.name_text(attr).to_owned().into());
                kwd_patterns.push(self.pattern()?);
            } else {
                positional.push(self.pattern()?);
            }
            if !self.eat(TokenKind::Comma) {
                break;
            }
        }
        let end = self.expect(TokenKind::RPar)?.range.end();
        Ok(Pattern::Class(PatternClass {
            range: TextRange::new(start, end),
            cls,
            patterns: positional,
            kwd_attrs,
            kwd_patterns,
        }))
    }

    fn type_parameters(&mut self) -> Result<Vec<TypeParam>, ParseError> {
        if !self.eat(TokenKind::LSqb) {
            return Ok(Vec::new());
        }
        if !self.options.version.supports(PythonVersion::Py312) {
            self.push_error(Diagnostic::unsupported(
                self.previous().range,
                "type parameter lists require Python 3.12 or newer",
            ));
        }
        let mut params = Vec::new();
        while !self.at(TokenKind::RSqb) && !self.at(TokenKind::EndMarker) {
            let start = self.current().range.start();
            let kind = if self.eat(TokenKind::DoubleStar) {
                TypeParamKind::ParamSpec
            } else if self.eat(TokenKind::Star) {
                TypeParamKind::TypeVarTuple
            } else {
                TypeParamKind::TypeVar
            };
            let name_token = self.expect(TokenKind::Name)?;
            let bound = if self.eat(TokenKind::Colon) { Some(self.expression(0)?) } else { None };
            let default = if self.eat(TokenKind::Equal) { Some(self.expression(0)?) } else { None };
            let end = default
                .as_ref()
                .map(Ranged::range)
                .or_else(|| bound.as_ref().map(Ranged::range))
                .unwrap_or(name_token.range)
                .end();
            let data = TypeParamData {
                range: TextRange::new(start, end),
                name: normalize_identifier(self.name_text(name_token)),
                bound,
                default,
            };
            params.push(match kind {
                TypeParamKind::TypeVar => TypeParam::TypeVar(data),
                TypeParamKind::ParamSpec => TypeParam::ParamSpec(data),
                TypeParamKind::TypeVarTuple => TypeParam::TypeVarTuple(data),
            });
            if !self.eat(TokenKind::Comma) {
                break;
            }
        }
        self.expect(TokenKind::RSqb)?;
        Ok(params)
    }

    fn comma_expression(&mut self, first: Expr) -> Result<Expr, ParseError> {
        if !self.eat(TokenKind::Comma) {
            return Ok(first);
        }
        let start = self.expression_range(&first).start();
        let mut elts = vec![first];
        let mut end = self.previous().range.end();
        while !self.at_line_end()
            && !self.at(TokenKind::Equal)
            && !self.at(TokenKind::Colon)
            && !self.at(TokenKind::In)
        {
            let expression = self.expression(0)?;
            end = self.expression_range(&expression).end();
            elts.push(expression);
            if !self.eat(TokenKind::Comma) {
                break;
            }
            end = self.previous().range.end();
        }
        Ok(Expr::Tuple(ExprSequence {
            range: TextRange::new(start, end),
            elts,
            ctx: ExprContext::Load,
        }))
    }

    fn expression(&mut self, minimum: u8) -> Result<Expr, ParseError> {
        if self.expression_nodes >= self.options.max_nodes {
            return Err(ParseError::too_many_nodes(self.current().range));
        }
        self.expression_nodes += 1;
        if self.enter_depth()? {
            return Err(ParseError::too_deep(self.current().range));
        }
        let mut left = self.prefix_expression()?;
        let mut chain_depth = 0u32;
        loop {
            let kind = self.current().kind;
            if kind == TokenKind::If && minimum <= 1 {
                self.bump();
                let test = self.expression(0)?;
                self.expect(TokenKind::Else)?;
                let orelse = self.expression(1)?;
                let range = TextRange::new(
                    self.expression_range(&left).start(),
                    self.expression_range(&orelse).end(),
                );
                left = Expr::IfExp(ExprIfExp {
                    range,
                    body: Box::new(left),
                    test: Box::new(test),
                    orelse: Box::new(orelse),
                });
                continue;
            }
            if kind == TokenKind::ColonEqual && minimum <= 1 {
                self.bump();
                let value = self.expression(1)?;
                let range = TextRange::new(
                    self.expression_range(&left).start(),
                    self.expression_range(&value).end(),
                );
                left = Expr::NamedExpr(ExprNamedExpr {
                    range,
                    target: Box::new(mark_store(left)?),
                    value: Box::new(value),
                });
                continue;
            }
            if kind == TokenKind::Or && minimum <= 2 {
                left = self.fold_bool(left, BoolOperator::Or, 2)?;
                continue;
            }
            if kind == TokenKind::And && minimum <= 3 {
                left = self.fold_bool(left, BoolOperator::And, 3)?;
                continue;
            }
            if !self.stop_in {
                if let Some((operator, width)) = compare_operator(self, kind) {
                    if minimum > 4 {
                        break;
                    }
                    for _ in 0..width {
                        self.bump();
                    }
                    let right = self.expression(5)?;
                    let mut ops = Vec::new();
                    let mut comparators = Vec::new();
                    if let Expr::Compare(existing) = left {
                        if self.grouped_expressions.contains(&existing.range) {
                            left = Expr::Compare(existing);
                        } else {
                            ops = existing.ops;
                            comparators = existing.comparators;
                            left = *existing.left;
                        }
                    }
                    ops.push(operator);
                    comparators.push(right);
                    let range = TextRange::new(
                        self.expression_range(&left).start(),
                        comparators
                            .last()
                            .map(|value| self.expression_range(value))
                            .unwrap_or_else(|| self.expression_range(&left))
                            .end(),
                    );
                    left = Expr::Compare(Box::new(ExprCompare {
                        range,
                        left: Box::new(left),
                        ops,
                        comparators,
                    }));
                    continue;
                }
            }
            let Some((operator, precedence, right_assoc)) = binary_operator(kind) else {
                break;
            };
            if precedence < minimum {
                break;
            }
            chain_depth = chain_depth.saturating_add(1);
            if chain_depth > self.options.max_depth {
                return Err(ParseError::too_deep(self.current().range));
            }
            self.bump();
            let right = self.expression(if right_assoc { precedence } else { precedence + 1 })?;
            let range = TextRange::new(
                self.expression_range(&left).start(),
                self.expression_range(&right).end(),
            );
            left = Expr::BinOp(ExprBinOp {
                range,
                left: Box::new(left),
                op: operator,
                right: Box::new(right),
            });
        }
        self.leave_depth();
        Ok(left)
    }

    fn fold_bool(
        &mut self,
        left: Expr,
        op: BoolOperator,
        precedence: u8,
    ) -> Result<Expr, ParseError> {
        self.bump();
        let right = self.expression(precedence + 1);
        let right = right?;
        let range = TextRange::new(
            self.expression_range(&left).start(),
            self.expression_range(&right).end(),
        );
        let mut values = match left {
            Expr::BoolOp(node)
                if node.op == op && !self.grouped_expressions.contains(&node.range) =>
            {
                node.values
            }
            other => vec![other],
        };
        if let Expr::BoolOp(node) = right {
            if node.op == op && !self.grouped_expressions.contains(&node.range) {
                values.extend(node.values);
            } else {
                values.push(Expr::BoolOp(node));
            }
        } else {
            values.push(right);
        }
        Ok(Expr::BoolOp(ExprBoolOp { range, op, values }))
    }

    fn prefix_expression(&mut self) -> Result<Expr, ParseError> {
        let token = self.current();
        let result = match token.kind {
            TokenKind::Star => {
                self.bump();
                let value = self.expression(6)?;
                Expr::Starred(ExprStarred {
                    range: TextRange::new(token.range.start(), self.expression_range(&value).end()),
                    value: Box::new(value),
                    ctx: ExprContext::Load,
                })
            }
            TokenKind::Plus | TokenKind::Minus | TokenKind::Tilde | TokenKind::Not => {
                self.bump();
                let op = match token.kind {
                    TokenKind::Plus => UnaryOperator::UAdd,
                    TokenKind::Minus => UnaryOperator::USub,
                    TokenKind::Tilde => UnaryOperator::Invert,
                    _ => UnaryOperator::Not,
                };
                let operand = self.expression(if op == UnaryOperator::Not { 4 } else { 11 })?;
                Expr::UnaryOp(ExprUnaryOp {
                    range: TextRange::new(
                        token.range.start(),
                        self.expression_range(&operand).end(),
                    ),
                    op,
                    operand: Box::new(operand),
                })
            }
            TokenKind::Await => {
                self.bump();
                let value = self.expression(6)?;
                Expr::Await(ExprUnaryValue {
                    range: TextRange::new(token.range.start(), self.expression_range(&value).end()),
                    value: Some(Box::new(value)),
                })
            }
            TokenKind::Yield => {
                self.bump();
                let value = if self.at_line_end()
                    || matches!(
                        self.current().kind,
                        TokenKind::RPar | TokenKind::RSqb | TokenKind::RBrace | TokenKind::Comma
                    ) {
                    None
                } else if self.eat(TokenKind::From) {
                    let first = self.expression(0)?;
                    let value = if self.at(TokenKind::Comma) {
                        self.comma_expression(first)?
                    } else {
                        first
                    };
                    Some((true, value))
                } else {
                    let first = self.expression(0)?;
                    let value = if self.at(TokenKind::Comma) {
                        self.comma_expression(first)?
                    } else {
                        first
                    };
                    Some((false, value))
                };
                match value {
                    Some((from, value)) => {
                        if from {
                            Expr::YieldFrom(ExprUnaryValue {
                                range: TextRange::new(
                                    token.range.start(),
                                    self.expression_range(&value).end(),
                                ),
                                value: Some(Box::new(value)),
                            })
                        } else {
                            Expr::Yield(ExprUnaryValue {
                                range: TextRange::new(
                                    token.range.start(),
                                    self.expression_range(&value).end(),
                                ),
                                value: Some(Box::new(value)),
                            })
                        }
                    }
                    None => Expr::Yield(ExprUnaryValue { range: token.range, value: None }),
                }
            }
            TokenKind::Lambda => {
                self.bump();
                let args = self.parameters_without_parentheses()?;
                self.expect(TokenKind::Colon)?;
                let body = self.expression(0)?;
                Expr::Lambda(Box::new(ExprLambda {
                    range: TextRange::new(token.range.start(), self.expression_range(&body).end()),
                    args,
                    body: Box::new(body),
                }))
            }
            _ => self.atom()?,
        };
        self.postfix(result)
    }

    fn atom(&mut self) -> Result<Expr, ParseError> {
        let token = self.bump();
        match token.kind {
            TokenKind::Name => Ok(Expr::Name(ExprName {
                range: token.range,
                id: normalize_identifier(self.name_text(token)),
                ctx: ExprContext::Load,
            })),
            TokenKind::True | TokenKind::False => Ok(Expr::BooleanLiteral(ExprBoolean {
                range: token.range,
                value: token.kind == TokenKind::True,
            })),
            TokenKind::None => Ok(Expr::NoneLiteral(ExprLiteral { range: token.range })),
            TokenKind::Ellipsis => Ok(Expr::EllipsisLiteral(ExprLiteral { range: token.range })),
            TokenKind::Int | TokenKind::Float | TokenKind::Complex => Ok(self.number_expr(token)),
            TokenKind::String { prefix, triple } => self.string_expr(token, prefix, triple),
            TokenKind::LPar => {
                let first = if self.at(TokenKind::RPar) { None } else { Some(self.expression(0)?) };
                if let Some(first) = first.as_ref() {
                    if self.at_comprehension_for() {
                        let generators = self.generators()?;
                        let end = self.expect(TokenKind::RPar)?.range.end();
                        return Ok(Expr::GeneratorExp(ExprComprehension {
                            range: TextRange::new(token.range.start(), end),
                            elt: Box::new(first.clone()),
                            generators,
                            key: None,
                            value: None,
                        }));
                    }
                }
                let mut items = first.into_iter().collect::<Vec<_>>();
                let mut tuple = false;
                loop {
                    if !self.eat(TokenKind::Comma) {
                        break;
                    }
                    tuple = true;
                    if self.at(TokenKind::RPar) {
                        break;
                    }
                    items.push(self.expression(0)?);
                }
                let end = self.expect(TokenKind::RPar)?.range.end();
                if tuple || items.len() != 1 {
                    Ok(Expr::Tuple(ExprSequence {
                        range: TextRange::new(token.range.start(), end),
                        elts: items,
                        ctx: ExprContext::Load,
                    }))
                } else {
                    let expression = items.pop().unwrap_or(Expr::Invalid(ExprInvalid {
                        range: token.range,
                        message: "empty expression".into(),
                    }));
                    let expression_range = expression.range();
                    self.grouped_expressions.insert(expression_range);
                    self.grouped_expression_ranges
                        .insert(expression_range, TextRange::new(token.range.start(), end));
                    Ok(expression)
                }
            }
            TokenKind::LSqb => {
                let mut items = Vec::new();
                if !self.at(TokenKind::RSqb) && !self.at(TokenKind::EndMarker) {
                    let first = self.expression(0)?;
                    if self.at_comprehension_for() {
                        let generators = self.generators()?;
                        let end = self.expect(TokenKind::RSqb)?.range.end();
                        return Ok(Expr::ListComp(ExprComprehension {
                            range: TextRange::new(token.range.start(), end),
                            elt: Box::new(first),
                            generators,
                            key: None,
                            value: None,
                        }));
                    }
                    items.push(first);
                    while self.eat(TokenKind::Comma) {
                        if self.at(TokenKind::RSqb) {
                            break;
                        }
                        items.push(self.expression(0)?);
                    }
                }
                let end = self.expect(TokenKind::RSqb)?.range.end();
                Ok(Expr::List(ExprSequence {
                    range: TextRange::new(token.range.start(), end),
                    elts: items,
                    ctx: ExprContext::Load,
                }))
            }
            TokenKind::LBrace => self.brace_expr(token.range.start()),
            _ => Err(ParseError::syntax(token.range, "expected expression")),
        }
    }

    fn brace_expr(&mut self, start: crate::source::TextSize) -> Result<Expr, ParseError> {
        if self.eat(TokenKind::RBrace) {
            return Ok(Expr::Dict(ExprDict {
                range: TextRange::new(start, self.previous().range.end()),
                keys: Vec::new(),
                values: Vec::new(),
            }));
        }
        let unpacked_first =
            if self.eat(TokenKind::DoubleStar) { Some(self.expression(0)?) } else { None };
        let first = if unpacked_first.is_some() { None } else { Some(self.expression(0)?) };
        let has_unpack = unpacked_first.is_some();
        let is_dict = unpacked_first.is_some() || self.eat(TokenKind::Colon);
        let mut keys = Vec::new();
        let mut values = Vec::new();
        let mut elts = Vec::new();
        if is_dict {
            if let Some(value) = unpacked_first {
                keys.push(None);
                values.push(value);
            } else {
                let Some(first) = first else {
                    return Err(self.error_here("expected dictionary key"));
                };
                keys.push(Some(first));
                values.push(self.expression(0)?);
            }
            if !has_unpack && self.at_comprehension_for() {
                let generators = self.generators()?;
                let end = self.expect(TokenKind::RBrace)?.range.end();
                let key = keys.pop().flatten().unwrap_or(Expr::Invalid(ExprInvalid {
                    range: TextRange::empty(start),
                    message: "missing dictionary key".into(),
                }));
                let value = values.pop().unwrap_or(Expr::Invalid(ExprInvalid {
                    range: TextRange::empty(start),
                    message: "missing dictionary value".into(),
                }));
                return Ok(Expr::DictComp(ExprComprehension {
                    range: TextRange::new(start, end),
                    elt: Box::new(value.clone()),
                    generators,
                    key: Some(Box::new(key)),
                    value: Some(Box::new(value)),
                }));
            }
            while self.eat(TokenKind::Comma) {
                if self.at(TokenKind::RBrace) {
                    break;
                }
                if self.eat(TokenKind::DoubleStar) {
                    keys.push(None);
                    values.push(self.expression(0)?);
                } else {
                    let key = self.expression(0)?;
                    self.expect(TokenKind::Colon)?;
                    keys.push(Some(key));
                    values.push(self.expression(0)?);
                }
            }
        } else {
            let Some(first) = first else {
                return Err(self.error_here("expected set element"));
            };
            elts.push(first);
            if self.at_comprehension_for() {
                let generators = self.generators()?;
                let end = self.expect(TokenKind::RBrace)?.range.end();
                return Ok(Expr::SetComp(ExprComprehension {
                    range: TextRange::new(start, end),
                    elt: Box::new(elts.remove(0)),
                    generators,
                    key: None,
                    value: None,
                }));
            }
            while self.eat(TokenKind::Comma) {
                if self.at(TokenKind::RBrace) {
                    break;
                }
                elts.push(self.expression(0)?);
            }
        }
        let end = self.expect(TokenKind::RBrace)?.range.end();
        if is_dict {
            Ok(Expr::Dict(ExprDict { range: TextRange::new(start, end), keys, values }))
        } else {
            Ok(Expr::Set(ExprSet { range: TextRange::new(start, end), elts }))
        }
    }

    fn generators(&mut self) -> Result<Vec<Comprehension>, ParseError> {
        let mut generators = Vec::new();
        while self.at_comprehension_for() {
            let start = self.current().range.start();
            let is_async = self.eat(TokenKind::Async);
            self.expect(TokenKind::For)?;
            self.stop_in = true;
            let target_first = self.expression(0)?;
            let target = if self.at(TokenKind::Comma) {
                self.comma_expression(target_first)?
            } else {
                target_first
            };
            self.stop_in = false;
            let target = mark_store(target)?;
            self.expect(TokenKind::In)?;
            let iter_first = self.expression(2)?;
            let iter = if self.at(TokenKind::Comma) {
                self.comma_expression(iter_first)?
            } else {
                iter_first
            };
            let mut ifs = Vec::new();
            while self.eat(TokenKind::If) {
                ifs.push(self.expression(2)?);
            }
            let end = ifs.last().map(Ranged::range).unwrap_or_else(|| iter.range()).end();
            generators.push(Comprehension {
                range: TextRange::new(start, end),
                target,
                iter,
                ifs,
                is_async,
            });
        }
        if generators.is_empty() {
            return Err(self.error_here("expected comprehension for clause"));
        }
        Ok(generators)
    }

    fn postfix(&mut self, mut expression: Expr) -> Result<Expr, ParseError> {
        let mut chain_depth = 0u32;
        loop {
            match self.current().kind {
                TokenKind::Dot => {
                    chain_depth = chain_depth.saturating_add(1);
                    if chain_depth > self.options.max_depth {
                        return Err(ParseError::too_deep(self.current().range));
                    }
                    self.bump();
                    let name = self.expect(TokenKind::Name)?;
                    let attr = normalize_identifier(self.name_text(name));
                    let range = TextRange::new(
                        self.expression_range(&expression).start(),
                        name.range.end(),
                    );
                    expression = Expr::Attribute(ExprAttribute {
                        range,
                        value: Box::new(expression),
                        attr,
                        ctx: ExprContext::Load,
                    });
                }
                TokenKind::LPar => {
                    chain_depth = chain_depth.saturating_add(1);
                    if chain_depth > self.options.max_depth {
                        return Err(ParseError::too_deep(self.current().range));
                    }
                    let start = self.expression_range(&expression).start();
                    let call_open = self.bump();
                    let mut args = Vec::new();
                    let mut keywords = Vec::new();
                    let mut generator_arg: Option<(Expr, Vec<Comprehension>)> = None;
                    while !self.at(TokenKind::RPar) && !self.at(TokenKind::EndMarker) {
                        if self.eat(TokenKind::DoubleStar) {
                            let starstar = self.previous();
                            let value = self.expression(0)?;
                            keywords.push(Keyword {
                                range: TextRange::new(
                                    starstar.range.start(),
                                    self.expression_range(&value).end(),
                                ),
                                arg: None,
                                value,
                            });
                        } else if self.at(TokenKind::Star) {
                            let star = self.bump();
                            let value = self.expression(0)?;
                            args.push(Expr::Starred(ExprStarred {
                                range: TextRange::new(star.range.start(), value.range().end()),
                                value: Box::new(value),
                                ctx: ExprContext::Load,
                            }));
                        } else {
                            let value = self.expression(0)?;
                            if self.at_comprehension_for() {
                                let generators = self.generators()?;
                                generator_arg = Some((value, generators));
                                break;
                            }
                            if let Expr::Name(node) = &value {
                                if self.eat(TokenKind::Equal) {
                                    let keyword_value = self.expression(0)?;
                                    keywords.push(Keyword {
                                        range: TextRange::new(
                                            value.range().start(),
                                            self.expression_range(&keyword_value).end(),
                                        ),
                                        arg: Some(node.id.clone()),
                                        value: keyword_value,
                                    });
                                } else {
                                    args.push(value);
                                }
                            } else {
                                args.push(value);
                            }
                        }
                        if !self.eat(TokenKind::Comma) {
                            break;
                        }
                    }
                    let end = self.expect(TokenKind::RPar)?.range.end();
                    if let Some((elt, generators)) = generator_arg {
                        args.push(Expr::GeneratorExp(ExprComprehension {
                            range: TextRange::new(call_open.range.start(), end),
                            elt: Box::new(elt),
                            generators,
                            key: None,
                            value: None,
                        }));
                    }
                    expression = Expr::Call(Box::new(ExprCall {
                        range: TextRange::new(start, end),
                        func: Box::new(expression),
                        args,
                        keywords,
                    }));
                }
                TokenKind::LSqb => {
                    chain_depth = chain_depth.saturating_add(1);
                    if chain_depth > self.options.max_depth {
                        return Err(ParseError::too_deep(self.current().range));
                    }
                    let start = self.expression_range(&expression).start();
                    self.bump();
                    let mut slices = Vec::new();
                    let mut tuple = false;
                    let mut trailing_comma_end = None;
                    if !self.at(TokenKind::RSqb) {
                        loop {
                            let item_start = self.current().range.start();
                            slices.push(self.subscript_item(item_start)?);
                            if !self.eat(TokenKind::Comma) {
                                break;
                            }
                            tuple = true;
                            trailing_comma_end = Some(self.previous().range.end());
                            if self.at(TokenKind::RSqb) {
                                break;
                            }
                        }
                    }
                    let end = self.expect(TokenKind::RSqb)?.range.end();
                    let slice =
                        if slices.len() == 1 && !tuple && !matches!(slices[0], Expr::Starred(_)) {
                            slices.pop().unwrap_or(Expr::Invalid(ExprInvalid {
                                range: TextRange::new(start, end),
                                message: "empty subscript".into(),
                            }))
                        } else if slices.is_empty() {
                            Expr::Invalid(ExprInvalid {
                                range: TextRange::new(start, end),
                                message: "empty subscript".into(),
                            })
                        } else {
                            let end = slices.last().map(Ranged::range).unwrap_or_default().end();
                            let end = trailing_comma_end.map_or(end, |comma| end.max(comma));
                            Expr::Tuple(ExprSequence {
                                range: TextRange::new(
                                    slices.first().map(Ranged::range).unwrap_or_default().start(),
                                    end,
                                ),
                                elts: slices,
                                ctx: ExprContext::Load,
                            })
                        };
                    expression = Expr::Subscript(ExprSubscript {
                        range: TextRange::new(start, end),
                        value: Box::new(expression),
                        slice: Box::new(slice),
                        ctx: ExprContext::Load,
                    });
                }
                _ => break,
            }
        }
        Ok(expression)
    }

    fn subscript_item(&mut self, start: crate::source::TextSize) -> Result<Expr, ParseError> {
        let lower =
            if self.at(TokenKind::Colon) || self.at(TokenKind::Comma) || self.at(TokenKind::RSqb) {
                None
            } else {
                Some(Box::new(self.expression(0)?))
            };
        if !self.eat(TokenKind::Colon) {
            return Ok(lower.map(|value| *value).unwrap_or(Expr::Invalid(ExprInvalid {
                range: TextRange::empty(start),
                message: "empty subscript".into(),
            })));
        }
        let first_colon_end = self.previous().range.end();
        let upper =
            if self.at(TokenKind::Colon) || self.at(TokenKind::Comma) || self.at(TokenKind::RSqb) {
                None
            } else {
                Some(Box::new(self.expression(0)?))
            };
        let (step, step_colon_end) = if self.eat(TokenKind::Colon) {
            let colon_end = self.previous().range.end();
            if self.at(TokenKind::Comma) || self.at(TokenKind::RSqb) {
                (None, Some(colon_end))
            } else {
                (Some(Box::new(self.expression(0)?)), Some(colon_end))
            }
        } else {
            (None, None)
        };
        let slice_start =
            lower.as_ref().map(|value| self.expression_range(value).start()).unwrap_or(start);
        let slice_end = step
            .as_ref()
            .map(|value| self.expression_range(value).end())
            .or(step_colon_end)
            .or(upper.as_ref().map(|value| self.expression_range(value).end()))
            .or(Some(first_colon_end))
            .unwrap_or_else(|| self.previous().range.end());
        Ok(Expr::Slice(ExprSlice {
            range: TextRange::new(slice_start, slice_end),
            lower,
            upper,
            step,
        }))
    }

    fn parameters_without_parentheses(&mut self) -> Result<Parameters, ParseError> {
        let mut parameters = Parameters::default();
        let mut keyword_only = false;
        while !self.at(TokenKind::Colon) && !self.at(TokenKind::EndMarker) {
            if self.eat(TokenKind::Slash) {
                parameters.posonlyargs.append(&mut parameters.args);
                self.eat(TokenKind::Comma);
                continue;
            } else if self.eat(TokenKind::DoubleStar) {
                let token = self.expect(TokenKind::Name)?;
                parameters.kwarg = Some(Parameter {
                    range: token.range,
                    name: normalize_identifier(self.name_text(token)),
                    annotation: None,
                    default: None,
                    type_comment: None,
                });
            } else if self.eat(TokenKind::Star) {
                if self.at(TokenKind::Name) {
                    let token = self.bump();
                    parameters.vararg = Some(Parameter {
                        range: token.range,
                        name: normalize_identifier(self.name_text(token)),
                        annotation: None,
                        default: None,
                        type_comment: None,
                    });
                }
                keyword_only = true;
            } else {
                let token = self.expect(TokenKind::Name)?;
                let name = normalize_identifier(self.name_text(token));
                let default = if self.eat(TokenKind::Equal) {
                    Some(Box::new(self.expression(0)?))
                } else {
                    None
                };
                let parameter = Parameter {
                    range: token.range,
                    name,
                    annotation: None,
                    default: default.clone(),
                    type_comment: None,
                };
                if keyword_only {
                    parameters.kwonlyargs.push(parameter);
                    parameters.kw_defaults.push(default.map(|value| *value));
                } else {
                    if let Some(default) = &default {
                        parameters.defaults.push((**default).clone());
                    }
                    parameters.args.push(parameter);
                }
            }
            if !self.eat(TokenKind::Comma) {
                break;
            }
        }
        Ok(parameters)
    }

    fn string_expr(
        &mut self,
        token: Token,
        prefix: crate::token::StringPrefix,
        triple: bool,
    ) -> Result<Expr, ParseError> {
        let mut parts = vec![self.string_part(token, prefix, triple)];
        let start = token.range.start();
        let mut end = token.range.end();
        while let TokenKind::String { prefix, triple } = self.current().kind {
            let next = self.bump();
            parts.push(self.string_part(next, prefix, triple));
            end = next.range.end();
        }
        if parts.iter().any(|part| part.flags.prefix.is_format()) {
            let mut values = Vec::new();
            for part in parts {
                if part.flags.prefix.is_format() {
                    self.check_legacy_fstring_constraints(
                        part.range,
                        &part.value,
                        part.flags.quote,
                        part.flags.triple,
                    );
                    if let Expr::FString(node) = parse_fstring(
                        self.src,
                        part.range,
                        &part.value,
                        part.flags.prefix.is_raw(),
                        part.flags.triple,
                        self.options.version,
                    )? {
                        for value in node.values {
                            append_fstring_value(&mut values, value);
                        }
                    }
                } else if !part.value.is_empty() {
                    append_fstring_value(
                        &mut values,
                        Expr::StringLiteral(ExprString {
                            range: part.range,
                            value: StringLiteralValue::new(vec![part]),
                        }),
                    );
                }
            }
            return Ok(Expr::FString(ExprFString { range: TextRange::new(start, end), values }));
        }
        if prefix.is_bytes() {
            return Ok(Expr::BytesLiteral(ExprString {
                range: TextRange::new(start, end),
                value: StringLiteralValue::new(parts),
            }));
        }
        Ok(Expr::StringLiteral(ExprString {
            range: TextRange::new(start, end),
            value: StringLiteralValue::new(parts),
        }))
    }

    fn check_legacy_fstring_constraints(
        &mut self,
        range: TextRange,
        value: &str,
        quote: char,
        triple: bool,
    ) {
        if self.options.version.supports(PythonVersion::Py312) {
            return;
        }
        let body_start = fstring_body_start(self.src, range, triple);
        let mut cursor = 0;
        while cursor < value.len() {
            let Some(open) = value[cursor..].find('{') else { break };
            let open = cursor + open;
            if value.as_bytes().get(open + 1) == Some(&b'{')
                || (!value.is_empty() && is_unicode_name_brace(value, open))
            {
                cursor = open + 1;
                continue;
            }
            let field_start = open + 1;
            let Some(field_end) = fstring_field_end(value, field_start) else { break };
            let inner = value.get(field_start..field_end).unwrap_or_default();
            let (expression, _, _, _) = split_fstring_field(inner);
            let field_range =
                TextRange::from_usize(body_start + open, body_start + field_end.saturating_add(1));
            if expression.contains(quote) {
                self.push_error(Diagnostic::unsupported(
                    field_range,
                    "f-string expressions cannot reuse the outer quote before Python 3.12",
                ));
            }
            if expression.contains('\\') {
                self.push_error(Diagnostic::unsupported(
                    field_range,
                    "f-string expressions cannot contain a backslash before Python 3.12",
                ));
            }
            if has_unquoted_marker(expression, b'#') {
                self.push_error(Diagnostic::unsupported(
                    field_range,
                    "f-string expressions cannot contain a comment before Python 3.12",
                ));
            }
            cursor = field_end.saturating_add(1);
        }
    }

    fn string_part(
        &self,
        token: Token,
        prefix: crate::token::StringPrefix,
        triple: bool,
    ) -> StringLiteralPart {
        let raw = &self.src[token.range];
        let quote_index = raw.find(['\'', '"']).unwrap_or(0);
        let delimiter = if triple { 3 } else { 1 };
        let value_end = raw.len().saturating_sub(delimiter);
        let value = raw.get(quote_index + delimiter..value_end).unwrap_or("");
        let value =
            if prefix.is_raw() || prefix.is_format() { value.to_owned() } else { unescape(value) };
        StringLiteralPart {
            range: token.range,
            flags: StringFlags {
                prefix,
                triple,
                quote: raw.as_bytes().get(quote_index).copied().unwrap_or(b'\'') as char,
            },
            value: value.into(),
        }
    }

    fn number_expr(&self, token: Token) -> Expr {
        let raw: Box<str> = self.src[token.range].into();
        let clean = raw.replace('_', "");
        let value = match token.kind {
            TokenKind::Float => Number::Float(clean.parse().unwrap_or(0.0)),
            TokenKind::Complex => Number::Complex {
                real: 0.0,
                imag: clean.trim_end_matches(['j', 'J']).parse().unwrap_or(0.0),
            },
            _ => Number::Int(Int::new(clean)),
        };
        Expr::NumberLiteral(ExprNumber { range: token.range, value, raw })
    }

    fn dotted_name(&mut self) -> Result<String, ParseError> {
        let first = self.expect(TokenKind::Name)?;
        let mut result = self.name_text(first).to_owned();
        while self.eat(TokenKind::Dot) {
            let next = self.expect(TokenKind::Name)?;
            result.push('.');
            result.push_str(self.name_text(next));
        }
        Ok(result)
    }
    fn name_text(&self, token: Token) -> &str {
        &self.src[token.range]
    }
    fn current(&self) -> Token {
        self.tokens.get(self.position).copied().unwrap_or_else(|| {
            Token::new(
                TokenKind::EndMarker,
                TextRange::empty(TextRange::from_usize(self.src.len(), self.src.len()).start()),
            )
        })
    }
    fn word_is(&self, word: &str) -> bool {
        self.at(TokenKind::Name) && &self.src[self.current().range] == word
    }
    fn at_word(&self, word: &str) -> bool {
        self.word_is(word)
    }
    fn at_comprehension_for(&self) -> bool {
        self.at(TokenKind::For)
            || (self.at(TokenKind::Async) && self.peek_kind(1) == TokenKind::For)
    }
    fn has_top_level_with_separator(&self) -> bool {
        if !self.at(TokenKind::LPar) {
            return false;
        }
        let mut depth = 1u32;
        for token in self.tokens.iter().skip(self.position + 1) {
            match token.kind {
                TokenKind::LPar | TokenKind::LSqb | TokenKind::LBrace => depth += 1,
                TokenKind::RPar => {
                    depth = depth.saturating_sub(1);
                    if depth == 0 {
                        return false;
                    }
                }
                TokenKind::RSqb | TokenKind::RBrace => depth = depth.saturating_sub(1),
                TokenKind::Comma | TokenKind::As if depth == 1 => return true,
                _ => {}
            }
        }
        false
    }
    fn looks_like_type_alias(&self) -> bool {
        self.peek_kind(1) == TokenKind::Name
            && matches!(self.peek_kind(2), TokenKind::Equal | TokenKind::LSqb)
    }
    fn previous(&self) -> Token {
        self.tokens.get(self.position.saturating_sub(1)).copied().unwrap_or_else(|| self.current())
    }
    fn peek_kind(&self, distance: usize) -> TokenKind {
        self.tokens
            .get(self.position + distance)
            .map(|token| token.kind)
            .unwrap_or(TokenKind::EndMarker)
    }
    fn at(&self, kind: TokenKind) -> bool {
        self.current().kind == kind
    }
    fn at_line_end(&self) -> bool {
        matches!(
            self.current().kind,
            TokenKind::Newline | TokenKind::Semi | TokenKind::Dedent | TokenKind::EndMarker
        )
    }
    fn bump(&mut self) -> Token {
        let token = self.current();
        if !self.at(TokenKind::EndMarker) {
            self.position += 1;
        }
        token
    }
    fn eat(&mut self, kind: TokenKind) -> bool {
        if self.at(kind) {
            self.bump();
            true
        } else {
            false
        }
    }
    fn expect(&mut self, kind: TokenKind) -> Result<Token, ParseError> {
        if self.at(kind) {
            Ok(self.bump())
        } else {
            Err(self.error_here(&format!("expected {:?}, found {:?}", kind, self.current().kind)))
        }
    }
    fn skip_newlines(&mut self) {
        while matches!(
            self.current().kind,
            TokenKind::Newline | TokenKind::NonLogicalNewline | TokenKind::Semi
        ) {
            self.bump();
        }
    }
    fn consume_line_end(&mut self) {
        if !self.at(TokenKind::Semi) {
            self.eat(TokenKind::Newline);
        }
    }

    fn statement_end(&self, fallback: TextSize) -> TextSize {
        self.previous().range.end().max(fallback)
    }

    fn suite_end(&self, fallback: TextSize) -> TextSize {
        let mut position = self.position;
        while position > 0
            && matches!(
                self.tokens[position - 1].kind,
                TokenKind::Dedent | TokenKind::Newline | TokenKind::NonLogicalNewline
            )
        {
            position -= 1;
        }
        self.tokens
            .get(position.saturating_sub(1))
            .filter(|token| token.kind == TokenKind::Semi)
            .map(|token| token.range.end())
            .unwrap_or(fallback)
    }

    fn expression_range(&self, expression: &Expr) -> TextRange {
        self.grouped_expression_ranges
            .get(&expression.range())
            .copied()
            .unwrap_or_else(|| expression.range())
    }

    fn pattern_range(&self, pattern: &Pattern) -> TextRange {
        self.grouped_pattern_ranges
            .get(&pattern.range())
            .copied()
            .unwrap_or_else(|| pattern.range())
    }

    fn trailing_type_comment(&self, offset: TextSize) -> Option<Box<str>> {
        if !self.options.type_comments {
            return None;
        }
        let offset = offset.as_usize().min(self.src.len());
        let line_end = self.src[offset..]
            .find(['\r', '\n'])
            .map_or(self.src.len(), |position| offset + position);
        let suffix = &self.src[offset..line_end];
        let comment = suffix.split_once('#')?.1.trim();
        let value = comment.strip_prefix("type:")?.trim();
        if value.is_empty() || value == "ignore" {
            return None;
        }
        Some(value.into())
    }
    fn error_here(&self, message: &str) -> ParseError {
        ParseError::syntax(self.current().range, message)
    }
    fn push_error(&mut self, diagnostic: Diagnostic) {
        if self.errors.len() < self.options.max_errors {
            self.errors.push(diagnostic);
        }
    }
    fn recover_statement(&mut self) {
        while !matches!(
            self.current().kind,
            TokenKind::Newline | TokenKind::Dedent | TokenKind::EndMarker
        ) {
            self.bump();
        }
        self.skip_newlines();
    }
    fn enter_depth(&mut self) -> Result<bool, ParseError> {
        self.depth += 1;
        if self.depth > self.options.max_depth {
            Err(ParseError::too_deep(self.current().range))
        } else {
            Ok(false)
        }
    }
    fn leave_depth(&mut self) {
        self.depth = self.depth.saturating_sub(1);
    }
}

fn unescape(value: &str) -> String {
    let mut output = String::with_capacity(value.len());
    let mut chars = value.chars().peekable();
    while let Some(character) = chars.next() {
        if character != '\\' {
            output.push(character);
            continue;
        }
        match chars.next() {
            Some('a') => output.push('\x07'),
            Some('b') => output.push('\x08'),
            Some('f') => output.push('\x0c'),
            Some('n') => output.push('\n'),
            Some('r') => output.push('\r'),
            Some('t') => output.push('\t'),
            Some('v') => output.push('\x0b'),
            Some('\n') => {}
            Some('\r') => {
                if chars.peek() == Some(&'\n') {
                    chars.next();
                }
            }
            Some(character @ '0'..='7') => {
                let mut digits = String::with_capacity(3);
                digits.push(character);
                for _ in 0..2 {
                    let Some(&next) = chars.peek() else { break };
                    if !matches!(next, '0'..='7') {
                        break;
                    }
                    digits.push(next);
                    chars.next();
                }
                if let Ok(value) =
                    u32::from_str_radix(&digits, 8).ok().and_then(char::from_u32).ok_or(())
                {
                    output.push(value);
                } else {
                    output.push('\\');
                    output.push_str(&digits);
                }
            }
            Some('\\') => output.push('\\'),
            Some('\'') => output.push('\''),
            Some('"') => output.push('"'),
            Some('x') => {
                let digits = take_escape_digits(&mut chars, 2);
                if digits.len() == 2 {
                    if let Ok(byte) = u8::from_str_radix(&digits, 16) {
                        output.push(char::from(byte));
                    } else {
                        output.push_str("\\x");
                        output.push_str(&digits);
                    }
                } else {
                    output.push_str("\\x");
                    output.push_str(&digits);
                }
            }
            Some('u') => {
                let digits = take_escape_digits(&mut chars, 4);
                if digits.len() == 4 {
                    if let Ok(value) =
                        u32::from_str_radix(&digits, 16).ok().and_then(char::from_u32).ok_or(())
                    {
                        output.push(value);
                    } else {
                        output.push_str("\\u");
                        output.push_str(&digits.to_ascii_lowercase());
                    }
                } else {
                    output.push_str("\\u");
                    output.push_str(&digits.to_ascii_lowercase());
                }
            }
            Some('U') => {
                let digits = take_escape_digits(&mut chars, 8);
                if digits.len() == 8 {
                    if let Ok(value) =
                        u32::from_str_radix(&digits, 16).ok().and_then(char::from_u32).ok_or(())
                    {
                        output.push(value);
                    } else {
                        output.push_str("\\U");
                        output.push_str(&digits.to_ascii_lowercase());
                    }
                } else {
                    output.push_str("\\U");
                    output.push_str(&digits.to_ascii_lowercase());
                }
            }
            Some('N') => {
                let mut name = String::new();
                if chars.next() == Some('{') {
                    for next in chars.by_ref() {
                        if next == '}' {
                            break;
                        }
                        name.push(next);
                    }
                    if let Some(character) = unicode_names2::character(&name) {
                        output.push(character);
                    } else {
                        output.push('\\');
                        output.push('N');
                        output.push('{');
                        output.push_str(&name);
                        output.push('}');
                    }
                } else {
                    output.push('\\');
                    output.push('N');
                }
            }
            Some(other) => {
                output.push('\\');
                output.push(other);
            }
            None => output.push('\\'),
        }
    }
    output
}

fn append_fstring_value(values: &mut Vec<Expr>, value: Expr) {
    if let Expr::StringLiteral(current) = value {
        if let Some(Expr::StringLiteral(previous)) = values.last_mut() {
            previous.range = TextRange::new(previous.range.start(), current.range.end());
            previous.value.parts.extend(current.value.parts);
            return;
        }
        values.push(Expr::StringLiteral(current));
    } else {
        values.push(value);
    }
}

fn take_escape_digits(
    chars: &mut std::iter::Peekable<std::str::Chars<'_>>,
    count: usize,
) -> String {
    let mut digits = String::new();
    for _ in 0..count {
        let Some(&character) = chars.peek() else { break };
        if !character.is_ascii_hexdigit() {
            break;
        }
        digits.push(character);
        chars.next();
    }
    digits
}

fn is_unicode_name_brace(value: &str, index: usize) -> bool {
    let bytes = value.as_bytes();
    if index < 2 || bytes[index - 1] != b'N' {
        return false;
    }
    let mut slash_count = 0;
    let mut cursor = index - 2;
    loop {
        if bytes[cursor] != b'\\' {
            break;
        }
        slash_count += 1;
        if cursor == 0 {
            break;
        }
        cursor -= 1;
    }
    slash_count % 2 == 1
}

fn normalize_identifier(value: &str) -> Box<str> {
    if !cfg!(feature = "nfkc") || value.is_ascii() {
        return value.into();
    }
    #[cfg(feature = "nfkc")]
    {
        value.nfkc().collect::<String>().into_boxed_str()
    }
    #[cfg(not(feature = "nfkc"))]
    value.into()
}

fn mark_store(expression: Expr) -> Result<Expr, ParseError> {
    match expression {
        Expr::Name(mut node) => {
            node.ctx = ExprContext::Store;
            Ok(Expr::Name(node))
        }
        Expr::Attribute(mut node) => {
            node.ctx = ExprContext::Store;
            Ok(Expr::Attribute(node))
        }
        Expr::Subscript(mut node) => {
            node.ctx = ExprContext::Store;
            Ok(Expr::Subscript(node))
        }
        Expr::Starred(mut node) => {
            node.ctx = ExprContext::Store;
            node.value = Box::new(mark_store(*node.value)?);
            Ok(Expr::Starred(node))
        }
        Expr::Tuple(mut node) => {
            node.ctx = ExprContext::Store;
            node.elts = node.elts.into_iter().map(mark_store).collect::<Result<_, _>>()?;
            Ok(Expr::Tuple(node))
        }
        Expr::List(mut node) => {
            node.ctx = ExprContext::Store;
            node.elts = node.elts.into_iter().map(mark_store).collect::<Result<_, _>>()?;
            Ok(Expr::List(node))
        }
        other => Err(ParseError::syntax(other.range(), "cannot assign to expression")),
    }
}

fn mark_delete(expression: Expr) -> Result<Expr, ParseError> {
    match expression {
        Expr::Name(mut node) => {
            node.ctx = ExprContext::Del;
            Ok(Expr::Name(node))
        }
        Expr::Attribute(mut node) => {
            node.ctx = ExprContext::Del;
            Ok(Expr::Attribute(node))
        }
        Expr::Subscript(mut node) => {
            node.ctx = ExprContext::Del;
            Ok(Expr::Subscript(node))
        }
        Expr::Tuple(mut node) => {
            node.ctx = ExprContext::Del;
            node.elts = node.elts.into_iter().map(mark_delete).collect::<Result<_, _>>()?;
            Ok(Expr::Tuple(node))
        }
        Expr::List(mut node) => {
            node.ctx = ExprContext::Del;
            node.elts = node.elts.into_iter().map(mark_delete).collect::<Result<_, _>>()?;
            Ok(Expr::List(node))
        }
        other => Err(ParseError::syntax(other.range(), "cannot delete expression")),
    }
}

fn binary_operator(kind: TokenKind) -> Option<(BinaryOperator, u8, bool)> {
    Some(match kind {
        TokenKind::Vbar => (BinaryOperator::BitOr, 5, false),
        TokenKind::CircumFlex => (BinaryOperator::BitXor, 6, false),
        TokenKind::Ampersand => (BinaryOperator::BitAnd, 7, false),
        TokenKind::LeftShift => (BinaryOperator::LShift, 8, false),
        TokenKind::RightShift => (BinaryOperator::RShift, 8, false),
        TokenKind::Plus => (BinaryOperator::Add, 9, false),
        TokenKind::Minus => (BinaryOperator::Sub, 9, false),
        TokenKind::Star => (BinaryOperator::Mult, 10, false),
        TokenKind::At => (BinaryOperator::MatMult, 10, false),
        TokenKind::Slash => (BinaryOperator::Div, 10, false),
        TokenKind::DoubleSlash => (BinaryOperator::FloorDiv, 10, false),
        TokenKind::Percent => (BinaryOperator::Mod, 10, false),
        TokenKind::DoubleStar => (BinaryOperator::Pow, 12, true),
        _ => return None,
    })
}
fn aug_operator(kind: TokenKind) -> Option<BinaryOperator> {
    binary_operator(kind).map(|(operator, _, _)| operator).or(match kind {
        TokenKind::PlusEqual => Some(BinaryOperator::Add),
        TokenKind::MinusEqual => Some(BinaryOperator::Sub),
        TokenKind::StarEqual => Some(BinaryOperator::Mult),
        TokenKind::SlashEqual => Some(BinaryOperator::Div),
        TokenKind::DoubleSlashEqual => Some(BinaryOperator::FloorDiv),
        TokenKind::PercentEqual => Some(BinaryOperator::Mod),
        TokenKind::AtEqual => Some(BinaryOperator::MatMult),
        TokenKind::AmperEqual => Some(BinaryOperator::BitAnd),
        TokenKind::VbarEqual => Some(BinaryOperator::BitOr),
        TokenKind::CircumflexEqual => Some(BinaryOperator::BitXor),
        TokenKind::LeftShiftEqual => Some(BinaryOperator::LShift),
        TokenKind::RightShiftEqual => Some(BinaryOperator::RShift),
        TokenKind::DoubleStarEqual => Some(BinaryOperator::Pow),
        _ => None,
    })
}
fn compare_operator(parser: &Parser<'_>, kind: TokenKind) -> Option<(CompareOperator, usize)> {
    match kind {
        TokenKind::EqEqual => Some((CompareOperator::Eq, 1)),
        TokenKind::NotEqual => Some((CompareOperator::NotEq, 1)),
        TokenKind::Less => Some((CompareOperator::Lt, 1)),
        TokenKind::LessEqual => Some((CompareOperator::LtE, 1)),
        TokenKind::Greater => Some((CompareOperator::Gt, 1)),
        TokenKind::GreaterEqual => Some((CompareOperator::GtE, 1)),
        TokenKind::In => Some((CompareOperator::In, 1)),
        TokenKind::Is if parser.peek_kind(1) == TokenKind::Not => Some((CompareOperator::IsNot, 2)),
        TokenKind::Is => Some((CompareOperator::Is, 1)),
        TokenKind::Not if parser.peek_kind(1) == TokenKind::In => Some((CompareOperator::NotIn, 2)),
        _ => None,
    }
}

fn parse_fstring(
    src: &str,
    range: TextRange,
    value: &str,
    raw: bool,
    triple: bool,
    version: PythonVersion,
) -> Result<Expr, ParseError> {
    let value_start = fstring_body_start(src, range, triple);
    parse_fstring_value(src, range, value, raw, version, value_start)
}

fn parse_fstring_value(
    src: &str,
    range: TextRange,
    value: &str,
    raw: bool,
    version: PythonVersion,
    value_start: usize,
) -> Result<Expr, ParseError> {
    let value_start = value_start.min(src.len());
    let mut values = Vec::new();
    let mut literal = String::new();
    let mut literal_start = 0;
    let mut index = 0;
    let bytes = value.as_bytes();
    while index < bytes.len() {
        if bytes[index] == b'{'
            && bytes.get(index + 1) != Some(&b'{')
            && (raw || !is_unicode_name_brace(value, index))
        {
            if !literal.is_empty() {
                let literal_range =
                    TextRange::from_usize(value_start + literal_start, value_start + index);
                values.push(fstring_literal(literal_range, std::mem::take(&mut literal), raw));
            }
            let start = index + 1;
            let Some(field_end) = fstring_field_end(value, start) else {
                return Err(ParseError::syntax(range, "unterminated f-string expression"));
            };
            let end = field_end + 1;
            let inner = value.get(start..field_end).unwrap_or("");
            let (expression_text, conversion, format_spec, debug_prefix) =
                split_fstring_field(inner);
            if !version.supports(PythonVersion::Py312) && has_unquoted_newline(expression_text) {
                return Err(ParseError::syntax(
                    range,
                    "f-string expression cannot include a newline",
                ));
            }
            if let Some(prefix) = debug_prefix {
                let prefix = strip_fstring_comments(prefix);
                let prefix_range =
                    TextRange::from_usize(value_start + start, value_start + start + prefix.len());
                values.push(Expr::StringLiteral(ExprString {
                    range: prefix_range,
                    value: StringLiteralValue::new(vec![StringLiteralPart {
                        range: prefix_range,
                        flags: StringFlags {
                            prefix: Default::default(),
                            triple: false,
                            quote: '\'',
                        },
                        value: prefix.into(),
                    }]),
                }));
            }
            let expression_start = inner.find(expression_text).unwrap_or(0);
            let expression_text = normalize_fstring_expression(expression_text);
            let expression_offset = value_start + start + expression_start;
            let expression_source =
                format!("({}{})", " ".repeat(expression_offset.saturating_sub(1)), expression_text);
            let expr = parse_expression(&expression_source)
                .map_err(|error| ParseError::syntax(range, error.diagnostic.message))?;
            let conversion =
                if debug_prefix.is_some() && conversion.is_none() { Some('r') } else { conversion };
            let format_spec = match format_spec {
                Some(spec) => {
                    let spec_start = inner.find(spec).unwrap_or(inner.len());
                    let spec_start = value_start + start + spec_start;
                    let spec_range = TextRange::from_usize(
                        spec_start.saturating_sub(1),
                        value_start + field_end,
                    );
                    Some(Box::new(parse_fstring_value(
                        src, spec_range, spec, raw, version, spec_start,
                    )?))
                }
                None => None,
            };
            let field_range = TextRange::from_usize(value_start + index, value_start + end);
            values.push(Expr::FormattedValue(ExprFormattedValue {
                range: field_range,
                value: Box::new(expr),
                conversion,
                format_spec,
            }));
            literal_start = end;
            index = end;
        } else {
            if bytes[index] == b'{' && bytes.get(index + 1) == Some(&b'{') {
                literal.push('{');
                index += 2;
            } else if bytes[index] == b'}' && bytes.get(index + 1) == Some(&b'}') {
                literal.push('}');
                index += 2;
            } else {
                let character = value[index..].chars().next().unwrap_or_default();
                literal.push(character);
                index += character.len_utf8();
            }
        }
    }
    if !literal.is_empty() {
        let literal_range =
            TextRange::from_usize(value_start + literal_start, value_start + value.len());
        values.push(fstring_literal(literal_range, literal, raw));
    }
    Ok(Expr::FString(ExprFString { range, values }))
}

fn fstring_body_start(src: &str, range: TextRange, triple: bool) -> usize {
    let start = range.start().as_usize();
    let Some(raw) = src.get(start..range.end().as_usize()) else { return start };
    let quote_offset = raw.find(['\'', '"']).unwrap_or(0);
    start + quote_offset + if triple { 3 } else { 1 }
}

fn fstring_literal(range: TextRange, value: String, raw: bool) -> Expr {
    Expr::StringLiteral(ExprString {
        range,
        value: StringLiteralValue::new(vec![StringLiteralPart {
            range,
            flags: StringFlags { prefix: Default::default(), triple: false, quote: '\'' },
            value: if raw { value.into() } else { unescape(&value).into() },
        }]),
    })
}

fn strip_fstring_comments(value: &str) -> String {
    let bytes = value.as_bytes();
    let mut output = String::with_capacity(value.len());
    let mut cursor = 0;
    while cursor < bytes.len() {
        match bytes[cursor] {
            b'\'' | b'"' => {
                let start = cursor;
                skip_fstring_string(value, &mut cursor);
                output.push_str(&value[start..cursor.min(value.len())]);
            }
            b'#' => {
                while cursor < bytes.len() && !matches!(bytes[cursor], b'\n' | b'\r') {
                    cursor += 1;
                }
            }
            _ => {
                let character = value[cursor..].chars().next().unwrap_or_default();
                output.push(character);
                cursor += character.len_utf8();
            }
        }
    }
    output
}

fn fstring_field_end(value: &str, start: usize) -> Option<usize> {
    let bytes = value.as_bytes();
    let mut depth = 1u32;
    let mut format_spec = false;
    let mut cursor = start;
    while cursor < bytes.len() {
        match bytes[cursor] {
            b'{' => depth += 1,
            b'}' => {
                depth = depth.saturating_sub(1);
                if depth == 0 {
                    return Some(cursor);
                }
            }
            b':' if depth == 1 => format_spec = true,
            b'#' if !format_spec => {
                while cursor < bytes.len() && !matches!(bytes[cursor], b'\n' | b'\r') {
                    cursor += 1;
                }
                continue;
            }
            b'\'' | b'"' => {
                skip_fstring_string(value, &mut cursor);
                continue;
            }
            _ => {}
        }
        cursor += fstring_char_len(value, cursor);
    }
    None
}

fn skip_fstring_string(value: &str, cursor: &mut usize) {
    let bytes = value.as_bytes();
    let quote = bytes[*cursor];
    let triple =
        bytes.get(*cursor..*cursor + 3) == Some(if quote == b'\'' { b"'''" } else { b"\"\"\"" });
    let delimiter = if triple { 3 } else { 1 };
    *cursor += delimiter;
    while *cursor < bytes.len() {
        if bytes[*cursor] == b'\\' {
            *cursor += 1;
            if *cursor < bytes.len() {
                *cursor += fstring_char_len(value, *cursor);
            }
            continue;
        }
        if bytes.get(*cursor..*cursor + delimiter)
            == Some(if quote == b'\'' {
                if triple {
                    b"'''"
                } else {
                    b"'"
                }
            } else if triple {
                b"\"\"\""
            } else {
                b"\""
            })
        {
            *cursor += delimiter;
            return;
        }
        *cursor += value[*cursor..].chars().next().map_or(1, char::len_utf8);
    }
    *cursor = bytes.len();
}

fn has_unquoted_newline(value: &str) -> bool {
    has_unquoted_marker(value, b'\n') || has_unquoted_marker(value, b'\r')
}

fn has_unquoted_marker(value: &str, marker: u8) -> bool {
    let bytes = value.as_bytes();
    let mut cursor = 0;
    while cursor < bytes.len() {
        if bytes[cursor] == marker {
            return true;
        }
        if matches!(bytes[cursor], b'\'' | b'"') {
            skip_fstring_string(value, &mut cursor);
            continue;
        }
        if bytes[cursor] == b'\\' {
            cursor += 1;
        }
        cursor += 1;
    }
    false
}

fn normalize_fstring_expression(value: &str) -> String {
    let bytes = value.as_bytes();
    let mut output = String::with_capacity(value.len());
    let mut cursor = 0;
    while cursor < bytes.len() {
        match bytes[cursor] {
            b'\'' | b'"' => {
                let start = cursor;
                skip_fstring_string(value, &mut cursor);
                output.push_str(&value[start..cursor.min(value.len())]);
            }
            b'#' => {
                while cursor < bytes.len() && !matches!(bytes[cursor], b'\n' | b'\r') {
                    cursor += 1;
                }
                output.push(' ');
            }
            b'\n' | b'\r' => {
                output.push(' ');
                cursor += 1;
                if bytes.get(cursor.wrapping_sub(1)) == Some(&b'\r')
                    && bytes.get(cursor) == Some(&b'\n')
                {
                    cursor += 1;
                }
            }
            _ => {
                let character = value[cursor..].chars().next().unwrap_or_default();
                output.push(character);
                cursor += character.len_utf8();
            }
        }
    }
    output.trim().to_owned()
}

fn split_fstring_field(field: &str) -> (&str, Option<char>, Option<&str>, Option<&str>) {
    let mut nesting = 0u32;
    let mut debug_at = None;
    let mut conversion_at = None;
    let mut format_at = None;
    let mut cursor = 0;
    while cursor < field.len() {
        let index = cursor;
        let character = field[index..].chars().next().unwrap_or_default();
        match character {
            '\'' | '"' => {
                skip_fstring_string(field, &mut cursor);
                continue;
            }
            '[' | '(' | '{' => nesting += 1,
            ']' | ')' | '}' => nesting = nesting.saturating_sub(1),
            '=' if nesting == 0
                && debug_at.is_none()
                && field.as_bytes().get(index + 1) != Some(&b'=')
                && field.as_bytes().get(index.wrapping_sub(1)) != Some(&b':')
                && field[index + 1..].trim_start().is_empty_or_debug_suffix() =>
            {
                debug_at = Some(index);
            }
            '!' if nesting == 0
                && conversion_at.is_none()
                && field.as_bytes().get(index + 1) != Some(&b'=') =>
            {
                conversion_at = Some(index)
            }
            ':' if nesting == 0 => {
                format_at = Some(index);
                break;
            }
            _ => {}
        }
        cursor += character.len_utf8();
    }
    let expression_end = debug_at.or(conversion_at).or(format_at).unwrap_or(field.len());
    let expression = field[..expression_end].trim();
    let conversion = conversion_at.and_then(|index| field[index + 1..].chars().next());
    let format_spec = format_at.map(|index| field[index + 1..].trim());
    let debug_prefix = debug_at.map(|_| {
        let end = conversion_at.or(format_at).unwrap_or(field.len());
        &field[..end]
    });
    (expression, conversion, format_spec, debug_prefix)
}

trait FStringSuffix {
    fn is_empty_or_debug_suffix(&self) -> bool;
}

impl FStringSuffix for str {
    fn is_empty_or_debug_suffix(&self) -> bool {
        self.is_empty() || self.starts_with(['!', ':', '#'])
    }
}

fn fstring_char_len(value: &str, cursor: usize) -> usize {
    value[cursor..].chars().next().map_or(1, char::len_utf8)
}
