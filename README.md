# pysyn

`pysyn` is a standalone Rust library for tokenizing and parsing Python 3.8–3.13
source code. It exposes source ranges, a CPython-shaped AST, diagnostics,
visitors, validation, and a small command-line interface.

```rust
let module = pysyn::parse_module("x = 1 + 2\n")?;
let text = pysyn::printer::unparse(&module);
assert_eq!(text, "x = 1 + 2\n");
# Ok::<(), pysyn::ParseError>(())
```

The project follows the architecture and acceptance criteria in
`pysyn_設計書.md` and `pysyn_作業指示書.md`. The parser is intentionally
framework-independent and does not execute Python code.

## Development

```text
cargo fmt --check
cargo test --all-features
cargo clippy --all-targets --all-features -- -D warnings
```

The current release is an early implementation. Unsupported or intentionally
different behavior is tracked in `docs/known-differences.md`.

The command-line tool accepts a file path or standard input:

```text
cargo run -- tokenize --format=cpython example.py
cargo run -- dump example.py
cargo run -- unparse example.py
cargo run -- check example.py
```
