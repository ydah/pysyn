//! Command-line interface for pysyn.

use std::env;
use std::fs;
use std::io::{self, Read};

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
    let source = match input.as_deref() {
        Some("-") | None => {
            let mut source = String::new();
            if let Err(error) = io::stdin().read_to_string(&mut source) {
                eprintln!("pysyn: {error}");
                std::process::exit(2);
            }
            source
        }
        Some(path) => match fs::read_to_string(path) {
            Ok(source) => source,
            Err(error) => {
                eprintln!("pysyn: {path}: {error}");
                std::process::exit(2);
            }
        },
    };
    match command.as_str() {
        "tokenize" => {
            let index = pysyn::LineIndex::new(&source);
            for item in pysyn::lexer::tokenize(&source) {
                match item {
                    Ok(token) if cpython_format => {
                        let start = index.line_col_utf8(&source, token.range.start());
                        let end = index.line_col_utf8(&source, token.range.end());
                        println!(
                            "({}, ({}, {}), ({}, {}), {})",
                            cpython_token_type(token.kind),
                            start.line,
                            start.column,
                            end.line,
                            end.column,
                            pysyn::printer::pyrepr(&source.as_str()[token.range])
                        );
                    }
                    Ok(token) => println!("{:?} {}", token.kind, token.range),
                    Err(error) => eprintln!("{error}"),
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
        pysyn::token::TokenKind::Name => 1,
        pysyn::token::TokenKind::Int
        | pysyn::token::TokenKind::Float
        | pysyn::token::TokenKind::Complex => 2,
        pysyn::token::TokenKind::String { .. }
        | pysyn::token::TokenKind::FStringStart { .. }
        | pysyn::token::TokenKind::FStringMiddle
        | pysyn::token::TokenKind::FStringEnd { .. } => 3,
        pysyn::token::TokenKind::Newline => 4,
        pysyn::token::TokenKind::Indent => 5,
        pysyn::token::TokenKind::Dedent => 6,
        pysyn::token::TokenKind::NonLogicalNewline => 62,
        pysyn::token::TokenKind::Comment => 61,
        _ => 54,
    }
}
