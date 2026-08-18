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
    let module = match parser.parse_module_inner() {
        Ok(module) => module,
        Err(error) => {
            parser.push_error(error.diagnostic);
            ModModule { body: Vec::new(), range: TextRange::from_usize(0, src.len()) }
        }
    };
    let comments = if parser.options.keep_comments { collect_comments(src) } else { Vec::new() };
    Parsed { module, comments, errors: parser.errors }
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
        Self { src, tokens, position: 0, options, errors, depth: 0, stop_in: false }
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
            TokenKind::Async => self.async_statement(),
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

    fn function(&mut self, is_async: bool) -> Result<Stmt, ParseError> {
        let start = self.current().range.start();
        if is_async {
            self.expect(TokenKind::Async)?;
        }
        self.expect(TokenKind::Def)?;
        let name_token = self.expect(TokenKind::Name)?;
        let name = self.name_text(name_token).to_owned().into();
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
        let name = self.name_text(name_token).to_owned().into();
        let mut bases = Vec::new();
        let mut keywords = Vec::new();
        if self.eat(TokenKind::LPar) {
            while !self.at(TokenKind::RPar) && !self.at(TokenKind::EndMarker) {
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
        let orelse = if self.eat(TokenKind::Elif) {
            self.position -= 1;
            vec![self.if_statement()?]
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
        let iter = self.expression(0)?;
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
        while self.eat(TokenKind::Except) {
            let handler_start = self.previous().range.start();
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
        Ok(Stmt::Try(StmtTry {
            range: TextRange::new(start, end),
            body,
            handlers,
            orelse,
            finalbody,
        }))
    }

    fn return_statement(&mut self) -> Result<Stmt, ParseError> {
        let start = self.expect(TokenKind::Return)?.range.start();
        let value = if self.at_line_end() { None } else { Some(Box::new(self.expression(0)?)) };
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
            loop {
                let name_start = self.current().range.start();
                let name_token = self.expect(TokenKind::Name)?;
                let name = self.name_text(name_token).to_owned();
                let asname = if self.eat(TokenKind::As) {
                    let as_token = self.expect(TokenKind::Name)?;
                    Some(self.name_text(as_token).to_owned().into())
                } else {
                    None
                };
                let end = self.previous().range.end();
                names.push(Alias {
                    range: TextRange::new(name_start, end),
                    name: name.into(),
                    asname,
                });
                if !self.eat(TokenKind::Comma) {
                    break;
                }
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
            let value = Box::new(self.expression(0)?);
            self.consume_line_end();
            let target = mark_store(first)?;
            return Ok(Stmt::AugAssign(StmtAugAssign {
                range: TextRange::new(first_start, value.range().end()),
                target: Box::new(target),
                op,
                value,
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
            let statement = self.statement()?;
            Ok(vec![statement])
        }
    }

    fn parameters(&mut self) -> Result<Parameters, ParseError> {
        self.expect(TokenKind::LPar)?;
        let mut parameters = Parameters::default();
        let mut seen_default = false;
        while !self.at(TokenKind::RPar) && !self.at(TokenKind::EndMarker) {
            if self.eat(TokenKind::DoubleStar) {
                let start = self.previous().range.start();
                let token = self.expect(TokenKind::Name)?;
                parameters.kwarg = Some(Parameter {
                    range: TextRange::new(start, token.range.end()),
                    name: self.name_text(token).into(),
                    annotation: None,
                    default: None,
                    type_comment: None,
                });
            } else if self.eat(TokenKind::Star) {
                let start = self.previous().range.start();
                let token = self.expect(TokenKind::Name)?;
                parameters.vararg = Some(Parameter {
                    range: TextRange::new(start, token.range.end()),
                    name: self.name_text(token).into(),
                    annotation: None,
                    default: None,
                    type_comment: None,
                });
            } else {
                let token = self.expect(TokenKind::Name)?;
                let name = self.name_text(token).to_owned();
                let default = if self.eat(TokenKind::Equal) {
                    seen_default = true;
                    Some(Box::new(self.expression(0)?))
                } else {
                    if seen_default {
                        return Err(
                            self.error_here("non-default argument follows default argument")
                        );
                    }
                    None
                };
                let parameter = Parameter {
                    range: TextRange::new(
                        token.range.start(),
                        default.as_ref().map(|value| value.range()).unwrap_or(token.range).end(),
                    ),
                    name: name.into(),
                    annotation: None,
                    default,
                    type_comment: None,
                };
                parameters.args.push(parameter);
            }
            if !self.eat(TokenKind::Comma) {
                break;
            }
        }
        self.expect(TokenKind::RPar)?;
        Ok(parameters)
    }

    fn comma_expression(&mut self, first: Expr) -> Result<Expr, ParseError> {
        if !self.eat(TokenKind::Comma) {
            return Ok(first);
        }
        let start = first.range().start();
        let mut elts = vec![first];
        while !self.at_line_end() && !self.at(TokenKind::Equal) && !self.at(TokenKind::Colon) {
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
            if kind == TokenKind::If && minimum <= 1 {
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
                if let Some(operator) = compare_operator(self, kind) {
                    if minimum > 4 {
                        break;
                    }
                    self.bump();
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
            TokenKind::Plus | TokenKind::Minus | TokenKind::Tilde | TokenKind::Not => {
                self.bump();
                let op = match token.kind {
                    TokenKind::Plus => UnaryOperator::UAdd,
                    TokenKind::Minus => UnaryOperator::USub,
                    TokenKind::Tilde => UnaryOperator::Invert,
                    _ => UnaryOperator::Not,
                };
                let operand = self.expression(6)?;
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
                let value = if self.at_line_end() {
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
                id: self.name_text(token).into(),
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
                let mut items = first.into_iter().collect::<Vec<_>>();
                let tuple = self.eat(TokenKind::Comma);
                while self.eat(TokenKind::Comma) {
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
                while !self.at(TokenKind::RSqb) && !self.at(TokenKind::EndMarker) {
                    items.push(self.expression(0)?);
                    if !self.eat(TokenKind::Comma) {
                        break;
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
        let first = self.expression(0)?;
        let is_dict = self.eat(TokenKind::Colon);
        let mut keys = Vec::new();
        let mut values = Vec::new();
        let mut elts = Vec::new();
        if is_dict {
            keys.push(Some(first));
            values.push(self.expression(0)?);
            while self.eat(TokenKind::Comma) {
                if self.at(TokenKind::RBrace) {
                    break;
                }
                let key = self.expression(0)?;
                self.expect(TokenKind::Colon)?;
                keys.push(Some(key));
                values.push(self.expression(0)?);
            }
        } else {
            elts.push(first);
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

    fn postfix(&mut self, mut expression: Expr) -> Result<Expr, ParseError> {
        loop {
            match self.current().kind {
                TokenKind::Dot => {
                    self.bump();
                    let name = self.expect(TokenKind::Name)?;
                    let attr = self.name_text(name).to_owned().into();
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
                    let lower = if self.at(TokenKind::Colon) || self.at(TokenKind::RSqb) {
                        None
                    } else {
                        Some(Box::new(self.expression(0)?))
                    };
                    let slice = if self.eat(TokenKind::Colon) {
                        let upper = if self.at(TokenKind::Colon) || self.at(TokenKind::RSqb) {
                            None
                        } else {
                            Some(Box::new(self.expression(0)?))
                        };
                        let step = if self.eat(TokenKind::Colon) {
                            if self.at(TokenKind::RSqb) {
                                None
                            } else {
                                Some(Box::new(self.expression(0)?))
                            }
                        } else {
                            None
                        };
                        let slice_start =
                            lower.as_ref().map(|value| value.range().start()).unwrap_or(start);
                        let slice_end = step
                            .as_ref()
                            .or(upper.as_ref())
                            .map(|value| value.range().end())
                            .unwrap_or_else(|| self.previous().range.end());
                        Expr::Slice(ExprSlice {
                            range: TextRange::new(slice_start, slice_end),
                            lower,
                            upper,
                            step,
                        })
                    } else {
                        lower.map(|v| *v).unwrap_or(Expr::Invalid(ExprInvalid {
                            range: self.previous().range,
                            message: "empty subscript".into(),
                        }))
                    };
                    let end = self.expect(TokenKind::RSqb)?.range.end();
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

    fn parameters_without_parentheses(&mut self) -> Result<Parameters, ParseError> {
        let mut parameters = Parameters::default();
        while self.at(TokenKind::Name) {
            let token = self.bump();
            let name = self.name_text(token).to_owned();
            let default =
                if self.eat(TokenKind::Equal) { Some(Box::new(self.expression(0)?)) } else { None };
            parameters.args.push(Parameter {
                range: TextRange::new(
                    token.range.start(),
                    default.as_ref().map(|v| v.range()).unwrap_or(token.range).end(),
                ),
                name: name.into(),
                annotation: None,
                default,
                type_comment: None,
            });
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
            return Ok(parse_fstring(
                self.src,
                TextRange::new(start, end),
                &parts[0].value,
                self.options.version,
            ));
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
        if self.at(TokenKind::Semi) {
            self.bump();
        } else {
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
fn compare_operator(parser: &Parser<'_>, kind: TokenKind) -> Option<CompareOperator> {
    match kind {
        TokenKind::EqEqual => Some(CompareOperator::Eq),
        TokenKind::NotEqual => Some(CompareOperator::NotEq),
        TokenKind::Less => Some(CompareOperator::Lt),
        TokenKind::LessEqual => Some(CompareOperator::LtE),
        TokenKind::Greater => Some(CompareOperator::Gt),
        TokenKind::GreaterEqual => Some(CompareOperator::GtE),
        TokenKind::In => Some(CompareOperator::In),
        TokenKind::Is if parser.peek_kind(1) != TokenKind::Not => Some(CompareOperator::Is),
        TokenKind::Not if parser.peek_kind(1) == TokenKind::In => Some(CompareOperator::NotIn),
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
            if let Ok(expr) = parse_expression(inner) {
                values.push(Expr::FormattedValue(ExprFormattedValue {
                    range,
                    value: Box::new(expr),
                    conversion: None,
                    format_spec: None,
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
