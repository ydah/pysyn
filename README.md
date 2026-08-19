# pysyn

`pysyn` is a pure Rust tokenizer and parser for Python 3.8–3.13 source code.
It parses Python syntax into a CPython-shaped AST while preserving source
ranges, diagnostics, and token-level information for inspection.

API documentation is available on [docs.rs](https://docs.rs/pysyn).

The parser is framework-independent and does not execute Python code.

## Features

- Pure Rust tokenizer and parser for Python 3.8–3.13 syntax.
- CPython-shaped AST with source locations, visitors, and validation helpers.
- Token retention, comments, type comments, and structured diagnostics.
- Recovery mode for inspecting invalid or incomplete source.
- AST dump, unparse, syntax checking, and token output through a small CLI.
- Optional `encoding` support for declared non-UTF-8 source encodings.
- Optional `nfkc` support for Unicode identifier normalization.

## Installation

When using the repository directly, add a path dependency to your
`Cargo.toml`:

```toml
[dependencies]
pysyn = { path = "../pysyn" }
```

## Usage

Parse and unparse a source string:

```rust
fn main() -> Result<(), pysyn::ParseError> {
    let module = pysyn::parse_module("answer = 40 + 2\n")?;
    let source = pysyn::printer::unparse(&module);
    assert_eq!(source, "answer = 40 + 2\n");
    Ok(())
}
```

Recover diagnostics from incomplete source:

```rust
use pysyn::parser::{parse, ParseMode, ParseOptions};

fn main() {
    let parsed = parse(
        "def broken(:\n    pass\n",
        ParseOptions { parse_mode: ParseMode::Recover, ..ParseOptions::default() },
    );
    for error in parsed.errors {
        eprintln!("{error}");
    }
}
```

The command-line tool accepts a file path or standard input:

```sh
cargo run -- tokenize --format=cpython example.py
cargo run -- tokenize --target-version=3.11 --format=cpython example.py
cargo run -- dump example.py
cargo run -- unparse example.py
cargo run -- check example.py
```

The CLI targets Python 3.13 by default. Use `--target-version=3.8` through
`--target-version=3.13` to select an older grammar and its version-specific
token and location behavior.

## API Overview

- `pysyn::parse_module(source)` parses a module into a CPython-shaped AST.
- `pysyn::parse_expression(source)` parses a standalone expression.
- `pysyn::lexer::tokenize(source)` exposes tokenization directly.
- `pysyn::SourceFile` decodes source bytes and tracks line/column ranges.
- `pysyn::validate` checks syntax and semantic constraints on an AST.
- `pysyn::visit` provides traversal helpers for AST consumers.

## Project Layout

- [`src/ast/`](src/ast/): AST definitions and node utilities.
- [`src/lexer.rs`](src/lexer.rs): Python tokenizer.
- [`src/parser/`](src/parser/): parser and context validation.
- [`src/printer/`](src/printer/): AST dump and source unparser.
- [`src/source.rs`](src/source.rs): source decoding, ranges, and positions.
- [`src/validate.rs`](src/validate.rs): syntax and semantic validation.
- [`tests/`](tests/): integration, compatibility, and coverage tests.
- [`benches/`](benches/): Criterion parser benchmarks.
- [`fuzz/`](fuzz/): cargo-fuzz targets.
- [`docs/`](docs/): differential testing and known behavior differences.

## Compatibility

The implementation targets Python 3.8–3.13. Version-specific syntax is gated
by the selected target version. Python 3.14-only template strings are outside
the supported range.

Known intentional differences and incomplete compatibility details are
tracked in [`docs/known-differences.md`](docs/known-differences.md).

## Development

Run the standard checks:

```sh
cargo fmt --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-features
```

Run benchmarks:

```sh
cargo bench
```

Run the fuzz targets with `cargo-fuzz`:

```sh
cargo fuzz run parse_bytes
cargo fuzz run parse_structured
```

Run the deterministic parser smoke test:

```sh
python tools/fuzz_smoke.py --pysyn target/debug/pysyn
```

## License

Licensed under the MIT License or Apache License 2.0.
