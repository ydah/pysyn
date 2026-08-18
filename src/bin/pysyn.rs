//! Command-line interface for pysyn.

use std::env;
use std::fs;
use std::io::{self, Read};

struct InputSource {
    text: String,
    encoding: Box<str>,
}

fn main() {
    let mut arguments = env::args().skip(1);
    let command = arguments.next().unwrap_or_else(|| "parse".into());
    if command == "--version" || command == "-V" {
        println!("pysyn {}", env!("CARGO_PKG_VERSION"));
        return;
    }
    let mut cpython_format = false;
    let mut input = None;
    for argument in arguments {
        if argument == "--format=cpython" {
            cpython_format = true;
        } else if input.is_none() {
            input = Some(argument);
        }
    }
    let input_source = match read_source(input.as_deref()) {
        Ok(source) => source,
        Err(error) => {
            eprintln!("pysyn: {error}");
            std::process::exit(2);
        }
    };
    let source = input_source.text;
    match command.as_str() {
        "tokenize" => {
            let index = pysyn::LineIndex::new(&source);
            if cpython_format {
                print_cpython_tokens(&source, &input_source.encoding, &index);
            } else {
                for item in pysyn::lexer::tokenize(&source) {
                    match item {
                        Ok(token) => println!("{:?} {}", token.kind, token.range),
                        Err(error) => eprintln!("{error}"),
                    }
                }
            }
        }
        "parse" | "dump" => match pysyn::parse_module(&source) {
            Ok(module) => println!("{}", pysyn::printer::dump(&module, Default::default())),
            Err(error) => {
                eprintln!("{}", error.diagnostic.display_with_source("<stdin>", &source));
                std::process::exit(1);
            }
        },
        "unparse" => match pysyn::parse_module(&source) {
            Ok(module) => print!("{}", pysyn::printer::unparse(&module)),
            Err(error) => {
                eprintln!("{}", error.diagnostic.display_with_source("<stdin>", &source));
                std::process::exit(1);
            }
        },
        "check" => match pysyn::parse_module(&source) {
            Ok(_) => println!("ok"),
            Err(error) => {
                eprintln!("{}", error.diagnostic.display_with_source("<stdin>", &source));
                std::process::exit(1);
            }
        },
        _ => {
            eprintln!("usage: pysyn [parse|tokenize|dump|unparse|check] [file|-]");
            std::process::exit(2);
        }
    }
}

fn cpython_token_type(kind: pysyn::token::TokenKind) -> u16 {
    match kind {
        pysyn::token::TokenKind::EndMarker => 0,
        pysyn::token::TokenKind::Name
        | pysyn::token::TokenKind::False
        | pysyn::token::TokenKind::None
        | pysyn::token::TokenKind::True
        | pysyn::token::TokenKind::And
        | pysyn::token::TokenKind::As
        | pysyn::token::TokenKind::Assert
        | pysyn::token::TokenKind::Async
        | pysyn::token::TokenKind::Await
        | pysyn::token::TokenKind::Break
        | pysyn::token::TokenKind::Class
        | pysyn::token::TokenKind::Continue
        | pysyn::token::TokenKind::Def
        | pysyn::token::TokenKind::Del
        | pysyn::token::TokenKind::Elif
        | pysyn::token::TokenKind::Else
        | pysyn::token::TokenKind::Except
        | pysyn::token::TokenKind::Finally
        | pysyn::token::TokenKind::For
        | pysyn::token::TokenKind::From
        | pysyn::token::TokenKind::Global
        | pysyn::token::TokenKind::If
        | pysyn::token::TokenKind::Import
        | pysyn::token::TokenKind::In
        | pysyn::token::TokenKind::Is
        | pysyn::token::TokenKind::Lambda
        | pysyn::token::TokenKind::Nonlocal
        | pysyn::token::TokenKind::Not
        | pysyn::token::TokenKind::Or
        | pysyn::token::TokenKind::Pass
        | pysyn::token::TokenKind::Raise
        | pysyn::token::TokenKind::Return
        | pysyn::token::TokenKind::Try
        | pysyn::token::TokenKind::While
        | pysyn::token::TokenKind::With
        | pysyn::token::TokenKind::Yield => 1,
        pysyn::token::TokenKind::Int
        | pysyn::token::TokenKind::Float
        | pysyn::token::TokenKind::Complex => 2,
        pysyn::token::TokenKind::String { .. } => 3,
        pysyn::token::TokenKind::FStringStart { .. } => 59,
        pysyn::token::TokenKind::FStringMiddle => 60,
        pysyn::token::TokenKind::FStringEnd { .. } => 61,
        pysyn::token::TokenKind::Newline => 4,
        pysyn::token::TokenKind::Indent => 5,
        pysyn::token::TokenKind::Dedent => 6,
        pysyn::token::TokenKind::Comment => 62,
        pysyn::token::TokenKind::NonLogicalNewline => 63,
        pysyn::token::TokenKind::Unknown => 64,
        _ => 55,
    }
}

fn read_source(input: Option<&str>) -> Result<InputSource, String> {
    let (name, bytes) = match input {
        Some(path) if path != "-" => {
            (path, fs::read(path).map_err(|error| format!("{path}: {error}"))?)
        }
        _ => {
            let mut bytes = Vec::new();
            io::stdin().read_to_end(&mut bytes).map_err(|error| error.to_string())?;
            ("<stdin>", bytes)
        }
    };
    let encoding = pysyn::detected_encoding_name(&bytes).map_err(|error| error.to_string())?;
    let source = pysyn::SourceFile::from_bytes(name, &bytes).map_err(|error| error.to_string())?;
    Ok(InputSource { text: source.text().to_owned(), encoding })
}

fn print_cpython_tokens(source: &str, encoding: &str, index: &pysyn::LineIndex) {
    println!("65 (0, 0) (0, 0) {}", pysyn::printer::pyrepr(encoding));
    let tokens = pysyn::lexer::tokenize(source).collect::<Vec<_>>();
    let eof = source.len();
    let needs_final_newline = needs_final_newline(source, &tokens);
    let insertion_position = final_newline_position(&tokens);
    for (position, item) in tokens.iter().enumerate() {
        if needs_final_newline && position == insertion_position {
            let kind = final_newline_kind(&tokens, source, index);
            print_cpython_token(
                pysyn::token::Token::new(
                    kind,
                    pysyn::source::TextRange::empty(pysyn::source::TextSize::new(eof as u32)),
                ),
                source,
                index,
                Some(kind),
            );
        }
        match item {
            Ok(token) => {
                if token.kind == pysyn::token::TokenKind::Newline
                    && token.range.is_empty()
                    && token.range.start().as_usize() == source.len()
                    && source.ends_with(['\n', '\r'])
                {
                    continue;
                }
                let virtual_kind = if is_virtual_eof_token(&tokens, position, source) {
                    Some(final_newline_kind(&tokens, source, index))
                } else {
                    None
                };
                print_cpython_token(*token, source, index, virtual_kind);
            }
            Err(error) => print_cpython_token(
                pysyn::token::Token::new(pysyn::token::TokenKind::Unknown, error.diagnostic.range),
                source,
                index,
                None,
            ),
        }
    }
}

fn print_cpython_token(
    token: pysyn::token::Token,
    source: &str,
    index: &pysyn::LineIndex,
    virtual_kind: Option<pysyn::token::TokenKind>,
) {
    let start = index.line_col_chars(source, token.range.start());
    let mut end = index.line_col_chars(source, token.range.end());
    let mut start = start;
    if virtual_kind.is_some() {
        end = pysyn::source::LineCol { line: start.line, column: start.column + 1 };
    } else if !token.range.is_empty()
        && matches!(
            token.kind,
            pysyn::token::TokenKind::Newline | pysyn::token::TokenKind::NonLogicalNewline
        )
    {
        let token_text = &source[token.range];
        let width = token_text.chars().count() as u32;
        end = pysyn::source::LineCol { line: start.line, column: start.column + width };
    } else if token.range.is_empty()
        && token.range.start().as_usize() == source.len()
        && matches!(
            token.kind,
            pysyn::token::TokenKind::Dedent | pysyn::token::TokenKind::EndMarker
        )
        && !source.is_empty()
        && !source.ends_with(['\n', '\r'])
    {
        start = pysyn::source::LineCol { line: start.line + 1, column: 0 };
        end = start;
    } else if matches!(
        virtual_kind.unwrap_or(token.kind),
        pysyn::token::TokenKind::Newline | pysyn::token::TokenKind::NonLogicalNewline
    ) {
        end = pysyn::source::LineCol {
            line: start.line,
            column: start.column + source[token.range].chars().count() as u32,
        };
    }
    let token_type = if let Some(kind) = virtual_kind {
        cpython_token_type(kind)
    } else {
        cpython_token_type(token.kind)
    };
    let text = if token.range.is_empty() {
        pysyn::printer::pyrepr("")
    } else {
        pysyn::printer::pyrepr(&source[token.range])
    };
    println!(
        "{} ({}, {}) ({}, {}) {}",
        token_type, start.line, start.column, end.line, end.column, text
    );
}

fn needs_final_newline(
    source: &str,
    tokens: &[Result<pysyn::token::Token, pysyn::lexer::LexError>],
) -> bool {
    if source.is_empty() || source.ends_with(['\n', '\r']) {
        return false;
    }
    tokens
        .iter()
        .rev()
        .filter_map(|item| item.as_ref().ok())
        .find(|token| token.kind != pysyn::token::TokenKind::EndMarker)
        .is_some_and(|token| {
            !matches!(
                token.kind,
                pysyn::token::TokenKind::Newline
                    | pysyn::token::TokenKind::NonLogicalNewline
                    | pysyn::token::TokenKind::EndMarker
            )
        })
}

fn final_newline_kind(
    tokens: &[Result<pysyn::token::Token, pysyn::lexer::LexError>],
    source: &str,
    index: &pysyn::LineIndex,
) -> pysyn::token::TokenKind {
    let eof_line =
        index.line_col_chars(source, pysyn::source::TextSize::new(source.len() as u32)).line;
    let has_code = tokens.iter().filter_map(|item| item.as_ref().ok()).any(|token| {
        let line = index.line_col_chars(source, token.range.start()).line;
        line == eof_line
            && !matches!(
                token.kind,
                pysyn::token::TokenKind::Comment
                    | pysyn::token::TokenKind::EndMarker
                    | pysyn::token::TokenKind::Newline
                    | pysyn::token::TokenKind::NonLogicalNewline
                    | pysyn::token::TokenKind::Dedent
                    | pysyn::token::TokenKind::Indent
            )
    });
    if has_code {
        pysyn::token::TokenKind::Newline
    } else {
        pysyn::token::TokenKind::NonLogicalNewline
    }
}

fn final_newline_position(tokens: &[Result<pysyn::token::Token, pysyn::lexer::LexError>]) -> usize {
    let end = tokens
        .iter()
        .position(
            |item| matches!(item, Ok(token) if token.kind == pysyn::token::TokenKind::EndMarker),
        )
        .unwrap_or(tokens.len());
    let mut position = end;
    while position > 0
        && matches!(
            &tokens[position - 1],
            Ok(token) if token.kind == pysyn::token::TokenKind::Dedent
        )
    {
        position -= 1;
    }
    position
}

fn is_virtual_eof_token(
    tokens: &[Result<pysyn::token::Token, pysyn::lexer::LexError>],
    position: usize,
    source: &str,
) -> bool {
    let Ok(token) = &tokens[position] else { return false };
    if !token.range.is_empty() || token.range.start().as_usize() != source.len() {
        return false;
    }
    token.kind == pysyn::token::TokenKind::Newline
        && !source.is_empty()
        && !source.ends_with(['\n', '\r'])
}
