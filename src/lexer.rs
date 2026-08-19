//! Python's indentation-aware lexical scanner.

use crate::error::{Diagnostic, DiagnosticCode};
use crate::source::{TextRange, TextSize};
use crate::token::{PythonVersion, StringPrefix, Token, TokenKind};
use std::fmt;

/// Selects whether trivia tokens are emitted.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum LexMode {
    /// Emit comments and non-logical newlines.
    Full,
    /// Suppress trivia useful only to a parser.
    Parse,
}

/// Lexer configuration.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct LexOptions {
    /// Trivia mode.
    pub mode: LexMode,
    /// Python grammar version.
    pub version: PythonVersion,
}

impl Default for LexOptions {
    fn default() -> Self {
        Self { mode: LexMode::Parse, version: PythonVersion::default() }
    }
}

/// A lexical error with its source range.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LexError {
    /// Diagnostic payload.
    pub diagnostic: Diagnostic,
}

impl fmt::Display for LexError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.diagnostic.fmt(f)
    }
}

impl std::error::Error for LexError {}

/// An iterator over Python tokens.
pub struct Tokenizer<'src> {
    source: &'src str,
    items: Vec<Result<Token, LexError>>,
    cursor: usize,
}

impl<'src> Iterator for Tokenizer<'src> {
    type Item = Result<Token, LexError>;
    fn next(&mut self) -> Option<Self::Item> {
        let item = self.items.get(self.cursor)?.clone();
        self.cursor += 1;
        Some(item)
    }
}

impl<'src> Tokenizer<'src> {
    /// Returns the original source text.
    pub const fn source(&self) -> &'src str {
        self.source
    }
}

/// Tokenizes source using full tokenize-compatible trivia mode.
pub fn tokenize(src: &str) -> Tokenizer<'_> {
    tokenize_with(src, LexOptions { mode: LexMode::Full, ..LexOptions::default() })
}

/// Tokenizes source using explicit lexer options.
pub fn tokenize_with(src: &str, options: LexOptions) -> Tokenizer<'_> {
    Scanner::new(src, options).finish()
}

struct Scanner<'src> {
    src: &'src str,
    options: LexOptions,
    position: usize,
    line_start: bool,
    trivia_line: bool,
    paren_depth: u32,
    indents: Vec<Indentation>,
    pending_dedents: usize,
    items: Vec<Result<Token, LexError>>,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
struct Indentation {
    tab8: u32,
    tab1: u32,
}

impl<'src> Scanner<'src> {
    fn new(src: &str, options: LexOptions) -> Scanner<'_> {
        Scanner {
            src,
            options,
            position: 0,
            line_start: true,
            trivia_line: false,
            paren_depth: 0,
            indents: vec![Indentation { tab8: 0, tab1: 0 }],
            pending_dedents: 0,
            items: Vec::new(),
        }
    }

    fn finish(mut self) -> Tokenizer<'src> {
        while self.position < self.src.len() {
            self.scan_one();
        }
        if !self
            .items
            .iter()
            .any(|item| matches!(item, Ok(token) if token.kind == TokenKind::Newline))
            && !self.src.is_empty()
        {
            let range = TextRange::empty(TextSize::new(self.src.len() as u32));
            self.items.push(Ok(Token::new(TokenKind::Newline, range)));
        }
        while self.indents.len() > 1 {
            self.indents.pop();
            let range = TextRange::empty(TextSize::new(self.src.len() as u32));
            self.items.push(Ok(Token::new(TokenKind::Dedent, range)));
        }
        let end = TextRange::empty(TextSize::new(self.src.len() as u32));
        self.items.push(Ok(Token::new(TokenKind::EndMarker, end)));
        Tokenizer { source: self.src, items: self.items, cursor: 0 }
    }

    fn scan_one(&mut self) {
        if self.pending_dedents > 0 {
            self.pending_dedents -= 1;
            let offset = TextRange::empty(TextSize::new(self.position as u32));
            self.items.push(Ok(Token::new(TokenKind::Dedent, offset)));
            return;
        }
        if self.line_start && self.paren_depth == 0 {
            self.scan_indentation();
            if self.pending_dedents > 0 {
                return;
            }
            if self.position >= self.src.len() {
                return;
            }
        } else if self.line_start {
            // Indentation is insignificant inside delimiters, but this still
            // marks the first token on a continued physical line as content.
            self.line_start = false;
        }
        let byte = self.src.as_bytes()[self.position];
        if byte == b' ' || byte == b'\t' || byte == b'\x0c' {
            self.position += 1;
            return;
        }
        if byte == b'\r' || byte == b'\n' {
            self.scan_newline();
            return;
        }
        if byte == b'#' {
            self.scan_comment();
            return;
        }
        if byte == b'\\' {
            if self.consume_line_continuation() {
                return;
            }
            self.error(
                self.position,
                self.position + 1,
                "unexpected character after line continuation character",
            );
            self.position += 1;
            return;
        }
        if let Some((kind, end)) = self.scan_prefixed_string() {
            if let TokenKind::String { prefix, triple } = kind {
                if prefix.is_format() && self.options.mode == LexMode::Full {
                    self.emit_fstring(self.position, end, prefix, triple);
                } else {
                    self.emit(kind, self.position, end);
                }
            } else {
                self.emit(kind, self.position, end);
            }
            self.position = end;
            return;
        }
        if is_identifier_start(self.src[self.position..].chars().next().unwrap_or('\0')) {
            self.scan_name();
            return;
        }
        if self.options.mode == LexMode::Full && !byte.is_ascii() {
            // CPython's tokenize module is intentionally permissive here:
            // non-identifier Unicode scalars are still surfaced as NAME and
            // left for the parser/compiler to reject.
            self.scan_name();
            return;
        }
        if byte.is_ascii_digit()
            || (byte == b'.'
                && matches!(self.src.as_bytes().get(self.position + 1), Some(byte) if byte.is_ascii_digit()))
        {
            self.scan_number();
            return;
        }
        if let Some((kind, width)) = operator_at(&self.src[self.position..]) {
            let start = self.position;
            self.position += width;
            if matches!(kind, TokenKind::LPar | TokenKind::LSqb | TokenKind::LBrace) {
                self.paren_depth += 1;
            }
            if matches!(kind, TokenKind::RPar | TokenKind::RSqb | TokenKind::RBrace) {
                self.paren_depth = self.paren_depth.saturating_sub(1);
            }
            self.emit(kind, start, self.position);
            return;
        }
        self.error(
            self.position,
            self.position + self.src[self.position..].chars().next().unwrap_or('\0').len_utf8(),
            "invalid character in source",
        );
        let end =
            self.position + self.src[self.position..].chars().next().unwrap_or('\0').len_utf8();
        self.emit(TokenKind::Unknown, self.position, end);
        self.position = end;
    }

    fn scan_indentation(&mut self) {
        let start = self.position;
        let mut tab8 = 0;
        let mut tab1 = 0;
        while let Some(byte) = self.src.as_bytes().get(self.position) {
            match byte {
                b' ' => {
                    tab8 += 1;
                    tab1 += 1;
                    self.position += 1;
                }
                b'\t' => {
                    tab8 = (tab8 / 8 + 1) * 8;
                    tab1 += 1;
                    self.position += 1;
                }
                b'\x0c' => {
                    tab8 = 0;
                    tab1 = 0;
                    self.position += 1;
                }
                _ => break,
            }
        }
        let next = self.src.as_bytes().get(self.position).copied();
        if matches!(next, None | Some(b'\n') | Some(b'\r') | Some(b'#')) {
            self.trivia_line = true;
            self.line_start = false;
            return;
        }
        self.trivia_line = false;
        let current = self.indents.last().copied().unwrap_or(Indentation { tab8: 0, tab1: 0 });
        let indentation = Indentation { tab8, tab1 };
        if (tab8.cmp(&current.tab8) == std::cmp::Ordering::Less)
            != (tab1.cmp(&current.tab1) == std::cmp::Ordering::Less)
        {
            self.error(start, self.position, "inconsistent use of tabs and spaces in indentation");
        }
        if tab8 > current.tab8 {
            self.indents.push(indentation);
            self.emit(TokenKind::Indent, start, self.position);
        } else if tab8 < current.tab8 {
            while self.indents.len() > 1
                && matches!(self.indents.last(), Some(level) if level.tab8 > tab8)
            {
                self.indents.pop();
                self.pending_dedents += 1;
            }
            if self.indents.last().map_or(true, |level| level.tab8 != tab8) {
                self.error(
                    start,
                    self.position,
                    "unindent does not match any outer indentation level",
                );
            }
        }
        self.line_start = false;
    }

    fn scan_newline(&mut self) {
        let start = self.position;
        if self.src.as_bytes()[self.position] == b'\r' {
            self.position += 1;
            if self.src.as_bytes().get(self.position) == Some(&b'\n') {
                self.position += 1;
            }
        } else {
            self.position += 1;
        }
        self.line_start = true;
        let kind = if self.paren_depth > 0 || self.trivia_line {
            TokenKind::NonLogicalNewline
        } else {
            TokenKind::Newline
        };
        self.trivia_line = false;
        if self.options.mode == LexMode::Full || kind == TokenKind::Newline {
            self.emit(kind, start, self.position);
        }
    }

    fn scan_comment(&mut self) {
        let start = self.position;
        while self.position < self.src.len()
            && !matches!(self.src.as_bytes()[self.position], b'\r' | b'\n')
        {
            self.position += 1;
        }
        if self.options.mode == LexMode::Full {
            self.emit(TokenKind::Comment, start, self.position);
        }
    }

    fn consume_line_continuation(&mut self) -> bool {
        let start = self.position;
        let next = self.src.as_bytes().get(self.position + 1).copied();
        if next != Some(b'\n') && next != Some(b'\r') {
            return false;
        }
        self.position += 2;
        if next == Some(b'\r') && self.src.as_bytes().get(self.position) == Some(&b'\n') {
            self.position += 1;
        }
        if self.position == self.src.len() {
            self.error(start, self.position, "unexpected end of file after line continuation");
        }
        // A backslash continuation keeps the logical line (and its current
        // indentation) active across the physical newline.
        self.line_start = false;
        true
    }

    fn scan_name(&mut self) {
        let start = self.position;
        self.position += char_len_at(self.src, self.position);
        while self.position < self.src.len() {
            let character = self.src[self.position..].chars().next().unwrap_or('\0');
            if !is_identifier_continue(character) {
                break;
            }
            self.position += character.len_utf8();
        }
        let text = &self.src[start..self.position];
        self.emit(TokenKind::keyword(text), start, self.position);
    }

    fn scan_number(&mut self) {
        let start = self.position;
        let bytes = self.src.as_bytes();
        let base = if bytes.get(self.position) == Some(&b'0') {
            match bytes.get(self.position + 1) {
                Some(b'x' | b'X') => {
                    self.position += 2;
                    Some(16)
                }
                Some(b'o' | b'O') => {
                    self.position += 2;
                    Some(8)
                }
                Some(b'b' | b'B') => {
                    self.position += 2;
                    Some(2)
                }
                _ => None,
            }
        } else {
            None
        };
        if let Some(base) = base {
            while self.position < bytes.len()
                && (bytes[self.position].is_ascii_hexdigit() || bytes[self.position] == b'_')
            {
                self.position += 1;
            }
            let digits = &self.src[start + 2..self.position];
            if !valid_base_digits(digits, base) {
                self.error(start, self.position, "invalid digit in numeric literal");
            }
        } else {
            while self.position < bytes.len()
                && (bytes[self.position].is_ascii_digit() || bytes[self.position] == b'_')
            {
                self.position += 1;
            }
            if bytes.get(self.position) == Some(&b'.') {
                self.position += 1;
                while self.position < bytes.len()
                    && (bytes[self.position].is_ascii_digit() || bytes[self.position] == b'_')
                {
                    self.position += 1;
                }
            }
            if matches!(bytes.get(self.position), Some(b'e' | b'E')) {
                self.position += 1;
                if matches!(bytes.get(self.position), Some(b'+' | b'-')) {
                    self.position += 1;
                }
                while self.position < bytes.len()
                    && (bytes[self.position].is_ascii_digit() || bytes[self.position] == b'_')
                {
                    self.position += 1;
                }
            }
            if matches!(bytes.get(self.position), Some(b'j' | b'J')) {
                self.position += 1;
            }
        }
        let text = &self.src[start..self.position];
        let lower = text.to_ascii_lowercase();
        let kind = if lower.ends_with('j') {
            TokenKind::Complex
        } else if base.is_none() && (lower.contains('.') || lower.contains('e')) {
            TokenKind::Float
        } else {
            TokenKind::Int
        };
        if base.is_none() && !valid_decimal_literal(text) {
            self.error(start, self.position, "invalid decimal literal");
        }
        if kind == TokenKind::Int
            && text.len() > 1
            && text.starts_with('0')
            && text.chars().any(|c| ('1'..='9').contains(&c))
            && !text.starts_with("0x")
            && !text.starts_with("0X")
            && !text.starts_with("0o")
            && !text.starts_with("0O")
            && !text.starts_with("0b")
            && !text.starts_with("0B")
        {
            self.error(
                start,
                self.position,
                "leading zeros in decimal integer literals are not permitted",
            );
        }
        if matches!(self.src.as_bytes().get(self.position), Some(byte) if byte.is_ascii_alphanumeric() || *byte == b'_')
        {
            self.error(start, self.position + 1, "invalid decimal literal");
        }
        self.emit(kind, start, self.position);
    }

    fn scan_prefixed_string(&mut self) -> Option<(TokenKind, usize)> {
        let start = self.position;
        let mut cursor = start;
        while cursor < self.src.len()
            && cursor - start < 2
            && self.src.as_bytes()[cursor].is_ascii_alphabetic()
        {
            cursor += 1;
        }
        let prefix = &self.src[start..cursor];
        let quote = self.src.as_bytes().get(cursor).copied()?;
        if quote != b'\'' && quote != b'"' {
            return None;
        }
        let flags = StringPrefix::parse(prefix)?;
        let end = if flags.is_format() {
            self.scan_fstring_body(cursor, flags.is_raw())
        } else {
            self.scan_string_body(cursor, flags.is_raw())
        };
        let triple = self.src.as_bytes().get(cursor..cursor + 3)
            == Some(if quote == b'\'' { b"'''" } else { b"\"\"\"" });
        Some((TokenKind::String { prefix: flags, triple }, end))
    }

    fn scan_string_body(&mut self, quote_start: usize, _raw: bool) -> usize {
        let quote = self.src.as_bytes()[quote_start];
        let triple = self.src.as_bytes().get(quote_start..quote_start + 3)
            == Some(if quote == b'\'' { b"'''" } else { b"\"\"\"" });
        let delimiter = if triple { 3 } else { 1 };
        let mut cursor = quote_start + delimiter;
        while cursor < self.src.len() {
            if self.src.as_bytes().get(cursor..cursor + delimiter)
                == Some(if quote == b'\'' {
                    if triple {
                        b"'''"
                    } else {
                        b"'"
                    }
                } else {
                    if triple {
                        b"\"\"\""
                    } else {
                        b"\""
                    }
                })
            {
                return cursor + delimiter;
            }
            if self.src.as_bytes()[cursor] == b'\\' {
                cursor += 1;
                if cursor < self.src.len() {
                    cursor += char_len_at(self.src, cursor);
                }
            } else {
                cursor += char_len_at(self.src, cursor);
            }
        }
        self.error(
            quote_start,
            self.src.len(),
            if triple {
                "unterminated triple-quoted string literal"
            } else {
                "unterminated string literal"
            },
        );
        self.src.len()
    }

    fn scan_fstring_body(&mut self, quote_start: usize, raw: bool) -> usize {
        let quote = self.src.as_bytes()[quote_start];
        let triple = self.src.as_bytes().get(quote_start..quote_start + 3)
            == Some(if quote == b'\'' { b"'''" } else { b"\"\"\"" });
        let delimiter = if triple { 3 } else { 1 };
        let mut cursor = quote_start + delimiter;
        let mut field_depth = 0u32;
        let mut string_quote = None;
        let mut string_triple = false;
        let mut field_comment = false;
        let mut field_format_spec = false;
        while cursor < self.src.len() {
            let byte = self.src.as_bytes()[cursor];
            if let Some(active_quote) = string_quote {
                if byte == b'\\' {
                    cursor += 1;
                    if cursor < self.src.len() {
                        cursor += char_len_at(self.src, cursor);
                    }
                } else if string_triple
                    && self.src.as_bytes().get(cursor..cursor + 3)
                        == Some(if active_quote == b'\'' { b"'''" } else { b"\"\"\"" })
                {
                    string_quote = None;
                    cursor += 3;
                } else if !string_triple && byte == active_quote {
                    string_quote = None;
                    cursor += 1;
                } else {
                    cursor += char_len_at(self.src, cursor);
                }
                continue;
            }
            if field_comment {
                if matches!(byte, b'\n' | b'\r') {
                    field_comment = false;
                }
                cursor += char_len_at(self.src, cursor);
                continue;
            }
            if field_depth == 0 {
                if byte == b'\\' {
                    let slash_start = cursor;
                    while self.src.as_bytes().get(cursor) == Some(&b'\\') {
                        cursor += 1;
                    }
                    if (cursor - slash_start) % 2 == 1
                        && self.src.as_bytes().get(cursor) == Some(&quote)
                    {
                        cursor += char_len_at(self.src, cursor);
                    }
                    continue;
                }
                if self.src.as_bytes().get(cursor..cursor + delimiter)
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
                    return cursor + delimiter;
                }
                if byte == b'{'
                    && self.src.as_bytes().get(cursor + 1) != Some(&b'{')
                    && (raw || !is_unicode_name_brace(self.src, cursor))
                {
                    field_depth = 1;
                    cursor += 1;
                } else if (byte == b'{' || byte == b'}')
                    && self.src.as_bytes().get(cursor + 1) == Some(&byte)
                {
                    cursor += 2;
                } else {
                    cursor += char_len_at(self.src, cursor);
                }
            } else {
                match byte {
                    b':' if field_depth == 1 => {
                        field_format_spec = true;
                        cursor += 1;
                    }
                    b'#' if !field_format_spec => {
                        field_comment = true;
                        cursor += 1;
                    }
                    b'\'' | b'"' => {
                        string_quote = Some(byte);
                        string_triple = self.src.as_bytes().get(cursor..cursor + 3)
                            == Some(if byte == b'\'' { b"'''" } else { b"\"\"\"" });
                        cursor += if string_triple { 3 } else { 1 };
                    }
                    b'{' => {
                        field_depth += 1;
                        cursor += 1;
                    }
                    b'}' => {
                        field_depth = field_depth.saturating_sub(1);
                        if field_depth == 0 {
                            field_format_spec = false;
                        }
                        cursor += 1;
                    }
                    _ => cursor += char_len_at(self.src, cursor),
                }
            }
        }
        self.error(
            quote_start,
            self.src.len(),
            if triple {
                "unterminated triple-quoted f-string literal"
            } else {
                "unterminated f-string literal"
            },
        );
        self.src.len()
    }

    fn emit_fstring(&mut self, start: usize, end: usize, prefix: StringPrefix, triple: bool) {
        let raw = &self.src[start..end];
        let quote_offset = raw.find(['\'', '"']).unwrap_or(0);
        let delimiter = if triple { 3 } else { 1 };
        let body_start = start + quote_offset + delimiter;
        let has_closing_delimiter = if triple {
            if raw.as_bytes().get(quote_offset) == Some(&b'\'') {
                raw.as_bytes().ends_with(b"'''")
            } else {
                raw.as_bytes().ends_with(b"\"\"\"")
            }
        } else if raw.as_bytes().get(quote_offset) == Some(&b'\'') {
            raw.as_bytes().ends_with(b"'")
        } else {
            raw.as_bytes().ends_with(b"\"")
        };
        let body_end = if has_closing_delimiter { end - delimiter } else { end };
        self.emit(TokenKind::FStringStart { prefix, triple }, start, body_start);
        let mut cursor = body_start;
        let mut literal_start = body_start;
        while cursor < body_end {
            let byte = self.src.as_bytes()[cursor];
            if byte == b'{'
                && self.src.as_bytes().get(cursor + 1) != Some(&b'{')
                && (prefix.is_raw() || !is_unicode_name_brace(self.src, cursor))
            {
                if literal_start < cursor {
                    self.emit_fstring_middle_range(literal_start, cursor);
                }
                self.emit(TokenKind::LBrace, cursor, cursor + 1);
                let expression_start = cursor + 1;
                let expression_end = matching_fstring_brace(self.src, expression_start, body_end);
                self.emit_fstring_expression(expression_start, expression_end);
                if expression_end < body_end {
                    self.emit(TokenKind::RBrace, expression_end, expression_end + 1);
                }
                cursor = expression_end.saturating_add(1);
                literal_start = cursor;
            } else if (byte == b'{' || byte == b'}')
                && self.src.as_bytes().get(cursor + 1) == Some(&byte)
            {
                cursor += 2;
            } else {
                cursor += self.src[cursor..].chars().next().map_or(1, char::len_utf8);
            }
        }
        if literal_start < body_end {
            self.emit_fstring_middle_range(literal_start, body_end);
        }
        self.emit(TokenKind::FStringEnd { prefix, triple }, body_end, end);
    }

    fn emit_fstring_expression(&mut self, start: usize, end: usize) {
        if start >= end {
            self.error(start, end, "f-string: empty expression not allowed");
            return;
        }
        if let Some(colon) = fstring_format_colon(self.src, start, end) {
            self.emit_fstring_tokens(start, colon);
            self.emit(TokenKind::Colon, colon, colon + 1);
            self.emit_fstring_format_spec(colon + 1, end);
            return;
        }
        self.emit_fstring_tokens(start, end);
    }

    fn emit_fstring_tokens(&mut self, start: usize, end: usize) {
        let expression = &self.src[start..end];
        for item in tokenize_with(
            expression,
            LexOptions { mode: LexMode::Full, version: self.options.version },
        ) {
            let Ok(token) = item else {
                let error = item.expect_err("matched error above").diagnostic;
                let shift = |range: TextRange| {
                    TextRange::from_usize(
                        start + range.start().as_usize(),
                        start + range.end().as_usize(),
                    )
                };
                self.items.push(Err(LexError {
                    diagnostic: Diagnostic {
                        code: error.code,
                        severity: error.severity,
                        message: error.message,
                        range: shift(error.range),
                        labels: error
                            .labels
                            .into_iter()
                            .map(|label| crate::error::Label {
                                range: shift(label.range),
                                message: label.message,
                            })
                            .collect(),
                        help: error.help,
                    },
                }));
                continue;
            };
            if matches!(token.kind, TokenKind::EndMarker | TokenKind::Indent | TokenKind::Dedent) {
                continue;
            }
            let kind = if token.kind == TokenKind::Newline {
                if token.range.start() == token.range.end() {
                    continue;
                }
                TokenKind::NonLogicalNewline
            } else {
                token.kind
            };
            self.items.push(Ok(Token::new(
                kind,
                TextRange::from_usize(
                    start + token.range.start().as_usize(),
                    start + token.range.end().as_usize(),
                ),
            )));
        }
    }

    fn emit_fstring_format_spec(&mut self, start: usize, end: usize) {
        let mut cursor = start;
        let mut literal_start = start;
        while cursor < end {
            if self.src.as_bytes()[cursor] == b'{'
                && self.src.as_bytes().get(cursor + 1) != Some(&b'{')
            {
                if literal_start < cursor {
                    self.emit_fstring_middle_range(literal_start, cursor);
                }
                self.emit(TokenKind::LBrace, cursor, cursor + 1);
                let nested_end = matching_fstring_brace(self.src, cursor + 1, end);
                self.emit_fstring_expression(cursor + 1, nested_end);
                if nested_end < end {
                    self.emit(TokenKind::RBrace, nested_end, nested_end + 1);
                    cursor = nested_end + 1;
                } else {
                    cursor = end;
                }
                literal_start = cursor;
            } else if (self.src.as_bytes()[cursor] == b'{' || self.src.as_bytes()[cursor] == b'}')
                && self.src.as_bytes().get(cursor + 1) == self.src.as_bytes().get(cursor)
            {
                cursor += 2;
            } else {
                cursor += char_len_at(self.src, cursor);
            }
        }
        if literal_start < end {
            self.emit_fstring_middle_range(literal_start, end);
        } else {
            self.emit(TokenKind::FStringMiddle, end, end);
        }
    }

    fn emit_fstring_middle_range(&mut self, start: usize, end: usize) {
        let mut cursor = start;
        let mut literal_start = start;
        while cursor < end {
            if cursor + 3 <= end
                && self.src.as_bytes()[cursor] == b'\\'
                && self.src.as_bytes().get(cursor + 1) == Some(&b'N')
                && self.src.as_bytes().get(cursor + 2) == Some(&b'{')
            {
                if let Some(name_end) = self.src[cursor + 3..end].find('}') {
                    let after_name = cursor + 3 + name_end + 1;
                    if after_name < end {
                        self.emit(TokenKind::FStringMiddle, literal_start, after_name);
                        cursor = after_name;
                        literal_start = cursor;
                        continue;
                    }
                    cursor = end;
                    continue;
                }
            }
            if (self.src.as_bytes()[cursor] == b'{' || self.src.as_bytes()[cursor] == b'}')
                && self.src.as_bytes().get(cursor + 1) == self.src.as_bytes().get(cursor)
            {
                if literal_start < cursor + 1 {
                    self.emit(TokenKind::FStringMiddle, literal_start, cursor + 1);
                }
                cursor += 2;
                literal_start = cursor;
            } else {
                cursor += char_len_at(self.src, cursor);
            }
        }
        if literal_start < end {
            self.emit(TokenKind::FStringMiddle, literal_start, end);
        }
    }

    fn emit(&mut self, kind: TokenKind, start: usize, end: usize) {
        self.items.push(Ok(Token::new(kind, TextRange::from_usize(start, end))));
    }

    fn error(&mut self, start: usize, end: usize, message: &str) {
        self.items.push(Err(LexError {
            diagnostic: Diagnostic::error(
                DiagnosticCode::Lexical,
                TextRange::from_usize(start, end),
                message,
            ),
        }));
    }
}

fn valid_base_digits(digits: &str, base: u32) -> bool {
    if digits.is_empty() {
        return false;
    }
    let mut saw_digit = false;
    let mut previous_was_digit = false;
    for (index, character) in digits.chars().enumerate() {
        if character == '_' {
            if (!previous_was_digit && index != 0) || digits[index + 1..].chars().next().is_none() {
                return false;
            }
            previous_was_digit = false;
            continue;
        }
        if !is_base_digit(character, base) {
            return false;
        }
        saw_digit = true;
        previous_was_digit = true;
    }
    saw_digit && previous_was_digit
}

fn is_base_digit(character: char, base: u32) -> bool {
    match base {
        2 => matches!(character, '0' | '1'),
        8 => matches!(character, '0'..='7'),
        16 => character.is_ascii_hexdigit(),
        _ => false,
    }
}

fn valid_decimal_literal(value: &str) -> bool {
    let bytes = value.as_bytes();
    let mut index = 0;
    let mut digit_count = 0;
    let mut previous_was_digit = false;
    while index < bytes.len() && (bytes[index].is_ascii_digit() || bytes[index] == b'_') {
        if bytes[index] == b'_' {
            if !previous_was_digit {
                return false;
            }
            previous_was_digit = false;
        } else {
            digit_count += 1;
            previous_was_digit = true;
        }
        index += 1;
    }
    if index > 0 && !previous_was_digit {
        return false;
    }
    if bytes.get(index) == Some(&b'.') {
        index += 1;
        previous_was_digit = false;
        while index < bytes.len() && (bytes[index].is_ascii_digit() || bytes[index] == b'_') {
            if bytes[index] == b'_' {
                if !previous_was_digit {
                    return false;
                }
                previous_was_digit = false;
            } else {
                digit_count += 1;
                previous_was_digit = true;
            }
            index += 1;
        }
        if !previous_was_digit && index > 0 && bytes[index - 1] == b'_' {
            return false;
        }
    }
    if matches!(bytes.get(index), Some(b'e' | b'E')) {
        index += 1;
        if matches!(bytes.get(index), Some(b'+' | b'-')) {
            index += 1;
        }
        let exponent_start = index;
        let mut exponent_digit = false;
        let mut exponent_previous_digit = false;
        while index < bytes.len() && (bytes[index].is_ascii_digit() || bytes[index] == b'_') {
            if bytes[index] == b'_' {
                if !exponent_previous_digit {
                    return false;
                }
                exponent_previous_digit = false;
            } else {
                exponent_digit = true;
                exponent_previous_digit = true;
            }
            index += 1;
        }
        if !exponent_digit || !exponent_previous_digit || exponent_start == index {
            return false;
        }
    }
    if matches!(bytes.get(index), Some(b'j' | b'J')) {
        index += 1;
    }
    digit_count > 0 && index == bytes.len()
}

fn char_len_at(source: &str, position: usize) -> usize {
    source.get(position..).and_then(|tail| tail.chars().next()).map_or(1, char::len_utf8)
}

fn fstring_format_colon(source: &str, start: usize, end: usize) -> Option<usize> {
    let bytes = source.as_bytes();
    let mut depth = 0u32;
    let mut cursor = start;
    while cursor < end {
        match bytes[cursor] {
            b'\'' | b'"' => skip_fstring_quote(source, &mut cursor, end),
            b'(' | b'[' | b'{' => {
                depth += 1;
                cursor += 1;
            }
            b')' | b']' | b'}' => {
                depth = depth.saturating_sub(1);
                cursor += 1;
            }
            b':' if depth == 0 => return Some(cursor),
            _ => cursor += char_len_at(source, cursor),
        }
    }
    None
}

fn skip_fstring_quote(source: &str, cursor: &mut usize, end: usize) {
    let quote = source.as_bytes()[*cursor];
    *cursor += 1;
    while *cursor < end {
        if source.as_bytes()[*cursor] == b'\\' {
            *cursor += 1;
            if *cursor < end {
                *cursor += char_len_at(source, *cursor);
            }
        } else if source.as_bytes()[*cursor] == quote {
            *cursor += 1;
            return;
        } else {
            *cursor += char_len_at(source, *cursor);
        }
    }
}

fn is_unicode_name_brace(source: &str, index: usize) -> bool {
    let bytes = source.as_bytes();
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

fn is_identifier_start(character: char) -> bool {
    character == '_' || unicode_ident::is_xid_start(character)
}

fn matching_fstring_brace(source: &str, start: usize, end: usize) -> usize {
    let mut depth = 1u32;
    let mut format_spec = false;
    let mut cursor = start;
    while cursor < end {
        match source.as_bytes()[cursor] {
            b'{' => depth += 1,
            b'}' => {
                depth = depth.saturating_sub(1);
                if depth == 0 {
                    return cursor;
                }
            }
            b':' if depth == 1 => format_spec = true,
            b'#' if !format_spec => {
                while cursor < end && !matches!(source.as_bytes()[cursor], b'\n' | b'\r') {
                    cursor += 1;
                }
                continue;
            }
            b'\'' | b'"' => {
                let quote = source.as_bytes()[cursor];
                let triple = source.as_bytes().get(cursor..cursor + 3)
                    == Some(if quote == b'\'' { b"'''" } else { b"\"\"\"" });
                let delimiter = if triple { 3 } else { 1 };
                cursor += 1;
                if triple {
                    cursor += 2;
                }
                while cursor < end {
                    if source.as_bytes()[cursor] == b'\\' {
                        let slash_start = cursor;
                        while source.as_bytes().get(cursor) == Some(&b'\\') {
                            cursor += 1;
                        }
                        if (cursor - slash_start) % 2 == 1
                            && source.as_bytes().get(cursor) == Some(&quote)
                        {
                            cursor += 1;
                        }
                        continue;
                    }
                    if source.as_bytes().get(cursor..cursor + delimiter)
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
                        cursor += delimiter;
                        break;
                    }
                    cursor += source[cursor..].chars().next().map_or(1, char::len_utf8);
                }
                continue;
            }
            _ => {}
        }
        cursor += char_len_at(source, cursor);
    }
    end
}
fn is_identifier_continue(character: char) -> bool {
    character == '_' || unicode_ident::is_xid_continue(character)
}

fn operator_at(source: &str) -> Option<(TokenKind, usize)> {
    const OPERATORS: &[(&str, TokenKind)] = &[
        ("**=", TokenKind::DoubleStarEqual),
        ("//=", TokenKind::DoubleSlashEqual),
        (">>=", TokenKind::RightShiftEqual),
        ("<<=", TokenKind::LeftShiftEqual),
        ("+=", TokenKind::PlusEqual),
        ("-=", TokenKind::MinusEqual),
        ("*=", TokenKind::StarEqual),
        ("/=", TokenKind::SlashEqual),
        ("%=", TokenKind::PercentEqual),
        ("@=", TokenKind::AtEqual),
        ("&=", TokenKind::AmperEqual),
        ("|=", TokenKind::VbarEqual),
        ("^=", TokenKind::CircumflexEqual),
        ("**", TokenKind::DoubleStar),
        ("//", TokenKind::DoubleSlash),
        ("<<", TokenKind::LeftShift),
        (">>", TokenKind::RightShift),
        ("->", TokenKind::Arrow),
        (":=", TokenKind::ColonEqual),
        ("==", TokenKind::EqEqual),
        ("!=", TokenKind::NotEqual),
        ("<=", TokenKind::LessEqual),
        (">=", TokenKind::GreaterEqual),
        ("...", TokenKind::Ellipsis),
        ("+", TokenKind::Plus),
        ("-", TokenKind::Minus),
        ("*", TokenKind::Star),
        ("/", TokenKind::Slash),
        ("%", TokenKind::Percent),
        ("@", TokenKind::At),
        ("&", TokenKind::Ampersand),
        ("|", TokenKind::Vbar),
        ("^", TokenKind::CircumFlex),
        ("~", TokenKind::Tilde),
        ("<", TokenKind::Less),
        (">", TokenKind::Greater),
        ("=", TokenKind::Equal),
        ("!", TokenKind::Exclamation),
        ("(", TokenKind::LPar),
        (")", TokenKind::RPar),
        ("[", TokenKind::LSqb),
        ("]", TokenKind::RSqb),
        ("{", TokenKind::LBrace),
        ("}", TokenKind::RBrace),
        (",", TokenKind::Comma),
        (":", TokenKind::Colon),
        (".", TokenKind::Dot),
        (";", TokenKind::Semi),
    ];
    OPERATORS
        .iter()
        .find_map(|(text, kind)| source.starts_with(text).then_some((*kind, text.len())))
}
