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
    let input = arguments.next();
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
            for item in pysyn::lexer::tokenize(&source) {
                match item {
                    Ok(token) => println!("{:?} {}", token.kind, token.range),
                    Err(error) => eprintln!("{error}"),
                }
            }
        }
        "parse" | "dump" => match pysyn::parse_module(&source) {
            Ok(module) => println!("{}", pysyn::printer::dump(&module, Default::default())),
            Err(error) => {
                eprintln!("{error}");
                std::process::exit(1);
            }
        },
        "unparse" => match pysyn::parse_module(&source) {
            Ok(module) => print!("{}", pysyn::printer::unparse(&module)),
            Err(error) => {
                eprintln!("{error}");
                std::process::exit(1);
            }
        },
        "check" => match pysyn::parse_module(&source) {
            Ok(_) => println!("ok"),
            Err(error) => {
                eprintln!("{error}");
                std::process::exit(1);
            }
        },
        _ => {
            eprintln!("usage: pysyn [parse|tokenize|dump|unparse|check] [file|-]");
            std::process::exit(2);
        }
    }
}
