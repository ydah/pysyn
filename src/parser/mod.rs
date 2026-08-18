//! Hand-written recursive-descent and Pratt parser.

#![allow(missing_docs)]

use crate::ast::*;
use crate::error::{Diagnostic, ParseError, Severity};
use crate::lexer::{tokenize_with, LexMode, LexOptions};
use crate::source::TextRange;
use crate::token::{PythonVersion, Token, TokenKind};

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
    let expr = parser.expression(0)?;
    parser.skip_newlines();
    if !parser.at(TokenKind::EndMarker) {
        return Err(parser.error_here("unexpected token after expression"));
    }
    Ok(expr)
}

/// Parses source with the requested recovery and version options.
pub fn parse(src: &str, options: ParseOptions) -> Parsed {
    let mut parser = Parser::new(src, options);
    let module = match parser.options.mode {
        SourceMode::Expression => match parser.expression(0) {
            Ok(expression) => ModModule {
                body: vec![Stmt::Expr(StmtExpr {
                    range: expression.range(),
                    value: Box::new(expression),
                })],
                range: TextRange::from_usize(0, src.len()),
            },
            Err(error) => {
                parser.push_error(error.diagnostic);
                ModModule { body: Vec::new(), range: TextRange::from_usize(0, src.len()) }
            }
        },
        SourceMode::Module | SourceMode::Interactive => match parser.parse_module_inner() {
            Ok(module) => module,
            Err(error) => {
                parser.push_error(error.diagnostic);
                ModModule { body: Vec::new(), range: TextRange::from_usize(0, src.len()) }
            }
        },
    };
    let comments = if parser.options.keep_comments { collect_comments(src) } else { Vec::new() };
    let tokens = if parser.options.keep_tokens { parser.tokens.clone() } else { Vec::new() };
    Parsed { module, comments, tokens, errors: parser.errors }
}

fn collect_comments(src: &str) -> Vec<Comment> {
    tokenize_with(src, LexOptions { mode: LexMode::Full, ..LexOptions::default() })
        .filter_map(|item| item.ok())
        .filter(|token| token.kind == TokenKind::Comment)
        .map(|token| Comment { range: token.range, text: src[token.range].into() })
        .collect()
}

struct Parser<'src> {
    src: &'src str,
    tokens: Vec<Token>,
    position: usize,
    options: ParseOptions,
    errors: Vec<Diagnostic>,
    depth: u32,
    stop_in: bool,
    stop_pattern_guard: bool,
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
            stop_in: false,
            stop_pattern_guard: false,
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
        Ok(ModModule { body, range: TextRange::from_usize(0, self.src.len()) })
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
            let pattern = self.pattern()?;
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
        let name = Expr::Name(ExprName {
            range: name_token.range,
            id: normalize_identifier(self.name_text(name_token)),
            ctx: ExprContext::Store,
        });
        if self.eat(TokenKind::LSqb) {
            let mut depth = 1u32;
            while depth > 0 && !self.at(TokenKind::EndMarker) {
                if self.eat(TokenKind::LSqb) {
                    depth += 1;
                } else if self.eat(TokenKind::RSqb) {
                    depth = depth.saturating_sub(1);
                } else {
                    self.bump();
                }
            }
        }
        self.expect(TokenKind::Equal)?;
        let value = Box::new(self.expression(0)?);
        self.consume_line_end();
        Ok(Stmt::TypeAlias(StmtTypeAlias {
            range: TextRange::new(start, value.range().end()),
            name: Box::new(name),
            type_params: Vec::new(),
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
        self.skip_type_parameters();
        let args = self.parameters()?;
        let returns =
            if self.eat(TokenKind::Arrow) { Some(Box::new(self.expression(0)?)) } else { None };
        self.expect(TokenKind::Colon)?;
        let body = self.block()?;
        let range = TextRange::new(
            start,
            body.last().map(Ranged::range).unwrap_or_else(|| self.previous().range).end(),
        );
        let node = StmtFunctionDef {
            range,
            name,
            decorator_list: Vec::new(),
            type_params: Vec::new(),
            args,
            returns,
            body,
            type_comment: None,
        };
        Ok(if is_async { Stmt::AsyncFunctionDef(node) } else { Stmt::FunctionDef(node) })
    }

    fn class(&mut self) -> Result<Stmt, ParseError> {
        let start = self.expect(TokenKind::Class)?.range.start();
        let name_token = self.expect(TokenKind::Name)?;
        let name = normalize_identifier(self.name_text(name_token));
        self.skip_type_parameters();
        let mut bases = Vec::new();
        let mut keywords = Vec::new();
        if self.eat(TokenKind::LPar) {
            while !self.at(TokenKind::RPar) && !self.at(TokenKind::EndMarker) {
                if self.eat(TokenKind::DoubleStar) {
                    let value = self.expression(0)?;
                    keywords.push(Keyword { range: value.range(), arg: None, value });
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
                                keyword_value.range().end(),
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
        let range = TextRange::new(
            start,
            body.last().map(Ranged::range).unwrap_or_else(|| self.previous().range).end(),
        );
        Ok(Stmt::ClassDef(StmtClassDef {
            range,
            name,
            bases,
            keywords,
            decorator_list: Vec::new(),
            type_params: Vec::new(),
            body,
        }))
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
        let node = StmtFor {
            range: TextRange::new(start, end),
            target: Box::new(target),
            iter: Box::new(iter),
            body,
            orelse,
            type_comment: None,
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
        let parenthesized = self.at(TokenKind::LPar) && self.has_top_level_comma();
        if parenthesized {
            self.bump();
        }
        loop {
            let context_expr = self.expression(0)?;
            let optional_vars =
                if self.eat(TokenKind::As) { Some(self.expression(0)?) } else { None };
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
        let body = self.block()?;
        let end = body.last().map(Ranged::range).unwrap_or_else(|| self.previous().range).end();
        let node = StmtWith { range: TextRange::new(start, end), items, body, type_comment: None };
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
        let node = StmtTry { range: TextRange::new(start, end), body, handlers, orelse, finalbody };
        Ok(if is_star { Stmt::TryStar(node) } else { Stmt::Try(node) })
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
        self.consume_line_end();
        Ok(Stmt::Raise(StmtRaise { range: TextRange::new(start, end), exc, cause }))
    }

    fn assert_statement(&mut self) -> Result<Stmt, ParseError> {
        let start = self.expect(TokenKind::Assert)?.range.start();
        let test = Box::new(self.expression(0)?);
        let msg =
            if self.eat(TokenKind::Comma) { Some(Box::new(self.expression(0)?)) } else { None };
        let end = msg.as_ref().map(|value| value.range()).unwrap_or_else(|| test.range()).end();
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
        self.consume_line_end();
        let end = self.previous().range.end();
        Ok(Stmt::Import(StmtImport { range: TextRange::new(start, end), names }))
    }

    fn import_from_statement(&mut self) -> Result<Stmt, ParseError> {
        let start = self.expect(TokenKind::From)?.range.start();
        let mut level = 0;
        while self.eat(TokenKind::Dot) {
            level += 1;
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
        self.consume_line_end();
        let end = self.previous().range.end();
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
        loop {
            let token = self.expect(TokenKind::Name)?;
            names.push(self.name_text(token).to_owned().into());
            if !self.eat(TokenKind::Comma) {
                break;
            }
        }
        self.consume_line_end();
        let node = StmtNames { range: TextRange::new(start, self.previous().range.end()), names };
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
        let targets = if self.at(TokenKind::Comma) { self.comma_expression(first)? } else { first };
        let targets = match targets {
            Expr::Tuple(node) => {
                node.elts.into_iter().map(mark_delete).collect::<Result<Vec<_>, _>>()?
            }
            other => vec![mark_delete(other)?],
        };
        let end = targets.last().map(Ranged::range).unwrap_or_else(|| self.previous().range).end();
        self.consume_line_end();
        Ok(Stmt::Delete(StmtDelete { range: TextRange::new(start, end), targets }))
    }

    fn simple_or_assignment(&mut self) -> Result<Stmt, ParseError> {
        let mut first = self.expression(0)?;
        if self.at(TokenKind::Comma) {
            first = self.comma_expression(first)?;
        }
        let first_start = first.range().start();
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
            self.consume_line_end();
            return Ok(Stmt::AnnAssign(StmtAnnAssign {
                range: TextRange::new(first_start, end),
                target: Box::new(target),
                annotation,
                value,
                simple: true,
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
            self.consume_line_end();
            let target = mark_store(first)?;
            return Ok(Stmt::AugAssign(StmtAugAssign {
                range: TextRange::new(first_start, value.range().end()),
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
            self.consume_line_end();
            return Ok(Stmt::Assign(StmtAssign {
                range: TextRange::new(
                    targets.first().map(Ranged::range).unwrap_or_default().start(),
                    value.range().end(),
                ),
                targets,
                value: Box::new(value),
                type_comment: None,
            }));
        }
        self.consume_line_end();
        Ok(Stmt::Expr(StmtExpr { range: first.range(), value: Box::new(first) }))
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
                let start = self.previous().range.start();
                let token = self.expect(TokenKind::Name)?;
                let annotation = if self.eat(TokenKind::Colon) {
                    Some(Box::new(self.expression(0)?))
                } else {
                    None
                };
                let end = annotation
                    .as_ref()
                    .map(|value| value.range().end())
                    .unwrap_or(token.range.end());
                parameters.kwarg = Some(Parameter {
                    range: TextRange::new(start, end),
                    name: normalize_identifier(self.name_text(token)),
                    annotation,
                    default: None,
                    type_comment: None,
                });
                self.eat(TokenKind::Comma);
                continue;
            } else if self.eat(TokenKind::Star) {
                let start = self.previous().range.start();
                if self.at(TokenKind::Name) {
                    let token = self.bump();
                    let annotation = if self.eat(TokenKind::Colon) {
                        Some(Box::new(self.expression(0)?))
                    } else {
                        None
                    };
                    let end = annotation
                        .as_ref()
                        .map(|value| value.range().end())
                        .unwrap_or(token.range.end());
                    parameters.vararg = Some(Parameter {
                        range: TextRange::new(start, end),
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
                let end = default
                    .as_ref()
                    .map(|value| value.range().end())
                    .or_else(|| annotation.as_ref().map(|value| value.range().end()))
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
        let previous = self.stop_pattern_guard;
        self.stop_pattern_guard = true;
        let first = self.expression(0)?;
        let expression =
            if self.at(TokenKind::Comma) { self.comma_expression(first)? } else { first };
        self.stop_pattern_guard = previous;
        let pattern = pattern_from_expr(expression);
        if self.eat(TokenKind::As) {
            let name = self.expect(TokenKind::Name)?;
            let range = TextRange::new(pattern.range().start(), name.range.end());
            if self.eat(TokenKind::Comma) {
                while !matches!(self.current().kind, TokenKind::Colon | TokenKind::EndMarker) {
                    self.bump();
                }
            }
            Ok(Pattern::As(PatternAs {
                range,
                pattern: Some(Box::new(pattern)),
                name: Some(self.name_text(name).to_owned().into()),
            }))
        } else {
            Ok(pattern)
        }
    }

    fn skip_type_parameters(&mut self) {
        if !self.eat(TokenKind::LSqb) {
            return;
        }
        let mut depth = 1u32;
        while depth > 0 && !self.at(TokenKind::EndMarker) {
            if self.eat(TokenKind::LSqb) {
                depth += 1;
            } else if self.eat(TokenKind::RSqb) {
                depth = depth.saturating_sub(1);
            } else {
                self.bump();
            }
        }
    }

    fn comma_expression(&mut self, first: Expr) -> Result<Expr, ParseError> {
        if !self.eat(TokenKind::Comma) {
            return Ok(first);
        }
        let start = first.range().start();
        let mut elts = vec![first];
        while !self.at_line_end()
            && !self.at(TokenKind::Equal)
            && !self.at(TokenKind::Colon)
            && !self.at(TokenKind::In)
        {
            elts.push(self.expression(0)?);
            if !self.eat(TokenKind::Comma) {
                break;
            }
        }
        let end = elts.last().map(Ranged::range).unwrap_or_default().end();
        Ok(Expr::Tuple(ExprSequence {
            range: TextRange::new(start, end),
            elts,
            ctx: ExprContext::Load,
        }))
    }

    fn expression(&mut self, minimum: u8) -> Result<Expr, ParseError> {
        if self.enter_depth()? {
            return Err(ParseError::too_deep(self.current().range));
        }
        let mut left = self.prefix_expression()?;
        loop {
            let kind = self.current().kind;
            if kind == TokenKind::If && minimum <= 1 && !self.stop_pattern_guard {
                self.bump();
                let test = self.expression(0)?;
                self.expect(TokenKind::Else)?;
                let orelse = self.expression(1)?;
                let range = TextRange::new(left.range().start(), orelse.range().end());
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
                let range = TextRange::new(left.range().start(), value.range().end());
                left = Expr::NamedExpr(ExprNamedExpr {
                    range,
                    target: Box::new(left),
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
                        ops = existing.ops;
                        comparators = existing.comparators;
                        left = *existing.left;
                    }
                    ops.push(operator);
                    comparators.push(right);
                    let range = TextRange::new(
                        left.range().start(),
                        comparators.last().map(Ranged::range).unwrap_or(left.range()).end(),
                    );
                    left = Expr::Compare(ExprCompare {
                        range,
                        left: Box::new(left),
                        ops,
                        comparators,
                    });
                    continue;
                }
            }
            let Some((operator, precedence, right_assoc)) = binary_operator(kind) else {
                break;
            };
            if precedence < minimum {
                break;
            }
            self.bump();
            let right = self.expression(if right_assoc { precedence } else { precedence + 1 })?;
            let range = TextRange::new(left.range().start(), right.range().end());
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
        let range = TextRange::new(left.range().start(), right.range().end());
        let mut values = match left {
            Expr::BoolOp(node) if node.op == op => node.values,
            other => vec![other],
        };
        if let Expr::BoolOp(node) = right {
            if node.op == op {
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
                    range: TextRange::new(token.range.start(), value.range().end()),
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
                let operand = self.expression(if op == UnaryOperator::Not { 4 } else { 6 })?;
                Expr::UnaryOp(ExprUnaryOp {
                    range: TextRange::new(token.range.start(), operand.range().end()),
                    op,
                    operand: Box::new(operand),
                })
            }
            TokenKind::Await => {
                self.bump();
                let value = self.expression(6)?;
                Expr::Await(ExprUnaryValue {
                    range: TextRange::new(token.range.start(), value.range().end()),
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
                    Some((true, self.expression(0)?))
                } else {
                    Some((false, self.expression(0)?))
                };
                match value {
                    Some((from, value)) => {
                        if from {
                            Expr::YieldFrom(ExprUnaryValue {
                                range: TextRange::new(token.range.start(), value.range().end()),
                                value: Some(Box::new(value)),
                            })
                        } else {
                            Expr::Yield(ExprUnaryValue {
                                range: TextRange::new(token.range.start(), value.range().end()),
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
                Expr::Lambda(ExprLambda {
                    range: TextRange::new(token.range.start(), body.range().end()),
                    args,
                    body: Box::new(body),
                })
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
                    if self.stop_pattern_guard && self.eat(TokenKind::As) {
                        self.expect(TokenKind::Name)?;
                    }
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
                    Ok(items.pop().unwrap_or(Expr::Invalid(ExprInvalid {
                        range: token.range,
                        message: "empty expression".into(),
                    })))
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
                keys.push(Some(first.expect("first dictionary key")));
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
            elts.push(first.expect("first set element"));
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
        loop {
            match self.current().kind {
                TokenKind::Dot => {
                    self.bump();
                    let name = self.expect(TokenKind::Name)?;
                    let attr = normalize_identifier(self.name_text(name));
                    let range = TextRange::new(expression.range().start(), name.range.end());
                    expression = Expr::Attribute(ExprAttribute {
                        range,
                        value: Box::new(expression),
                        attr,
                        ctx: ExprContext::Load,
                    });
                }
                TokenKind::LPar => {
                    let start = expression.range().start();
                    self.bump();
                    let mut args = Vec::new();
                    let mut keywords = Vec::new();
                    let mut generator_arg: Option<(Expr, Vec<Comprehension>)> = None;
                    while !self.at(TokenKind::RPar) && !self.at(TokenKind::EndMarker) {
                        if self.eat(TokenKind::DoubleStar) {
                            let value = self.expression(0)?;
                            keywords.push(Keyword { range: value.range(), arg: None, value });
                        } else if self.eat(TokenKind::Star) {
                            let value = self.expression(0)?;
                            args.push(Expr::Starred(ExprStarred {
                                range: TextRange::new(
                                    self.previous().range.start(),
                                    value.range().end(),
                                ),
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
                                            keyword_value.range().end(),
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
                            range: TextRange::new(elt.range().start(), end),
                            elt: Box::new(elt),
                            generators,
                            key: None,
                            value: None,
                        }));
                    }
                    expression = Expr::Call(ExprCall {
                        range: TextRange::new(start, end),
                        func: Box::new(expression),
                        args,
                        keywords,
                    });
                }
                TokenKind::LSqb => {
                    let start = expression.range().start();
                    self.bump();
                    let mut slices = Vec::new();
                    if !self.at(TokenKind::RSqb) {
                        loop {
                            slices.push(self.subscript_item(start)?);
                            if !self.eat(TokenKind::Comma) || self.at(TokenKind::RSqb) {
                                break;
                            }
                        }
                    }
                    let end = self.expect(TokenKind::RSqb)?.range.end();
                    let slice = if slices.len() == 1 {
                        slices.pop().expect("one subscript item")
                    } else if slices.is_empty() {
                        Expr::Invalid(ExprInvalid {
                            range: TextRange::new(start, end),
                            message: "empty subscript".into(),
                        })
                    } else {
                        Expr::Tuple(ExprSequence {
                            range: TextRange::new(
                                slices.first().map(Ranged::range).unwrap_or_default().start(),
                                slices.last().map(Ranged::range).unwrap_or_default().end(),
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
        let upper =
            if self.at(TokenKind::Colon) || self.at(TokenKind::Comma) || self.at(TokenKind::RSqb) {
                None
            } else {
                Some(Box::new(self.expression(0)?))
            };
        let step = if self.eat(TokenKind::Colon) {
            if self.at(TokenKind::Comma) || self.at(TokenKind::RSqb) {
                None
            } else {
                Some(Box::new(self.expression(0)?))
            }
        } else {
            None
        };
        let slice_start = lower.as_ref().map(|value| value.range().start()).unwrap_or(start);
        let slice_end = step
            .as_ref()
            .or(upper.as_ref())
            .map(|value| value.range().end())
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
                let start = self.previous().range.start();
                let token = self.expect(TokenKind::Name)?;
                parameters.kwarg = Some(Parameter {
                    range: TextRange::new(start, token.range.end()),
                    name: normalize_identifier(self.name_text(token)),
                    annotation: None,
                    default: None,
                    type_comment: None,
                });
            } else if self.eat(TokenKind::Star) {
                let start = self.previous().range.start();
                if self.at(TokenKind::Name) {
                    let token = self.bump();
                    parameters.vararg = Some(Parameter {
                        range: TextRange::new(start, token.range.end()),
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
                    range: TextRange::new(
                        token.range.start(),
                        default.as_ref().map(|v| v.range()).unwrap_or(token.range).end(),
                    ),
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
        if prefix.is_format() {
            let mut values = Vec::new();
            for part in parts {
                if let Expr::FString(node) =
                    parse_fstring(self.src, part.range, &part.value, self.options.version)
                {
                    values.extend(node.values);
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
        let value = if prefix.is_raw() { value.to_owned() } else { unescape(value) };
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
    fn has_top_level_comma(&self) -> bool {
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
                TokenKind::Comma if depth == 1 => return true,
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
    let mut chars = value.chars();
    while let Some(character) = chars.next() {
        if character != '\\' {
            output.push(character);
            continue;
        }
        match chars.next() {
            Some('n') => output.push('\n'),
            Some('r') => output.push('\r'),
            Some('t') => output.push('\t'),
            Some('0') => output.push('\0'),
            Some('\\') => output.push('\\'),
            Some('\'') => output.push('\''),
            Some('"') => output.push('"'),
            Some(other) => {
                output.push('\\');
                output.push(other);
            }
            None => output.push('\\'),
        }
    }
    output
}

fn normalize_identifier(value: &str) -> Box<str> {
    if !cfg!(feature = "nfkc") || value.is_ascii() {
        return value.into();
    }
    let mut normalized = String::with_capacity(value.len());
    for character in value.chars() {
        let replacement = match character {
            'ａ'..='ｚ' | 'Ａ'..='Ｚ' | '０'..='９' => {
                char::from_u32(character as u32 - 0xfee0).unwrap_or(character)
            }
            '　' => ' ',
            'ｱ' => 'ア',
            'ｲ' => 'イ',
            'ｳ' => 'ウ',
            'ｴ' => 'エ',
            'ｵ' => 'オ',
            'ﬀ' => 'f',
            'ﬁ' => 'f',
            'ﬂ' => 'f',
            'ﬃ' => 'f',
            'ﬄ' => 'f',
            'ﬅ' => 's',
            'ﬆ' => 's',
            _ => character,
        };
        normalized.push(replacement);
        match character {
            'ﬀ' => normalized.push('f'),
            'ﬁ' => normalized.push('i'),
            'ﬂ' => normalized.push('l'),
            'ﬃ' => {
                normalized.push('f');
                normalized.push('i');
            }
            'ﬄ' => {
                normalized.push('f');
                normalized.push('l');
            }
            'ﬅ' | 'ﬆ' => normalized.push('t'),
            _ => {}
        }
    }
    normalized.into_boxed_str()
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

fn pattern_from_expr(expression: Expr) -> Pattern {
    let range = expression.range();
    match expression {
        Expr::Name(node) if node.id.as_ref() == "_" => {
            Pattern::As(PatternAs { range, pattern: None, name: None })
        }
        Expr::Name(node) => Pattern::As(PatternAs { range, pattern: None, name: Some(node.id) }),
        Expr::BooleanLiteral(_) | Expr::NoneLiteral(_) => {
            Pattern::Singleton(PatternSingleton { range, value: expression })
        }
        Expr::List(node) | Expr::Tuple(node) => Pattern::Sequence(PatternSequence {
            range,
            patterns: node.elts.into_iter().map(pattern_from_expr).collect(),
        }),
        Expr::BinOp(node) if node.op == BinaryOperator::BitOr => {
            let patterns = vec![pattern_from_expr(*node.left), pattern_from_expr(*node.right)];
            Pattern::Or(PatternOr { range, patterns })
        }
        other => Pattern::Value(PatternValue { range, value: other }),
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

fn parse_fstring(_src: &str, range: TextRange, value: &str, version: PythonVersion) -> Expr {
    let mut values = Vec::new();
    let mut literal = String::new();
    let mut index = 0;
    let bytes = value.as_bytes();
    while index < bytes.len() {
        if bytes[index] == b'{' && bytes.get(index + 1) != Some(&b'{') {
            if !literal.is_empty() {
                values.push(Expr::StringLiteral(ExprString {
                    range,
                    value: StringLiteralValue::new(vec![StringLiteralPart {
                        range,
                        flags: StringFlags {
                            prefix: Default::default(),
                            triple: false,
                            quote: '\'',
                        },
                        value: std::mem::take(&mut literal).into(),
                    }]),
                }));
            }
            let start = index + 1;
            let mut end = start;
            let mut depth = 1;
            while end < bytes.len() && depth > 0 {
                if bytes[end] == b'{' {
                    depth += 1;
                } else if bytes[end] == b'}' {
                    depth -= 1;
                }
                end += 1;
            }
            let inner = value.get(start..end.saturating_sub(1)).unwrap_or("");
            let (expression_text, conversion, format_spec) = split_fstring_field(inner);
            if let Ok(expr) = parse_expression(expression_text) {
                let format_spec =
                    format_spec.map(|spec| Box::new(parse_fstring("", range, spec, version)));
                values.push(Expr::FormattedValue(ExprFormattedValue {
                    range,
                    value: Box::new(expr),
                    conversion,
                    format_spec,
                }));
            }
            index = end;
        } else {
            if bytes[index] == b'{' && bytes.get(index + 1) == Some(&b'{') {
                literal.push('{');
                index += 2;
            } else if bytes[index] == b'}' && bytes.get(index + 1) == Some(&b'}') {
                literal.push('}');
                index += 2;
            } else {
                literal.push(bytes[index] as char);
                index += 1;
            }
        }
    }
    if !literal.is_empty() {
        values.push(Expr::StringLiteral(ExprString {
            range,
            value: StringLiteralValue::new(vec![StringLiteralPart {
                range,
                flags: StringFlags { prefix: Default::default(), triple: false, quote: '\'' },
                value: literal.into(),
            }]),
        }));
    }
    let _ = version;
    Expr::FString(ExprFString { range, values })
}

fn split_fstring_field(field: &str) -> (&str, Option<char>, Option<&str>) {
    let mut nesting = 0u32;
    let mut conversion_at = None;
    let mut format_at = None;
    for (index, character) in field.char_indices() {
        match character {
            '[' | '(' | '{' => nesting += 1,
            ']' | ')' | '}' => nesting = nesting.saturating_sub(1),
            '!' if nesting == 0 && conversion_at.is_none() => conversion_at = Some(index),
            ':' if nesting == 0 => {
                format_at = Some(index);
                break;
            }
            _ => {}
        }
    }
    let expression_end = conversion_at.or(format_at).unwrap_or(field.len());
    let expression = field[..expression_end].trim();
    let conversion = conversion_at.and_then(|index| field[index + 1..].chars().next());
    let format_spec = format_at.map(|index| field[index + 1..].trim());
    (expression, conversion, format_spec)
}
