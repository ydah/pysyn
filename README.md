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
cargo run -- tokenize --target-version=3.11 --format=cpython example.py
cargo run -- dump example.py
cargo run -- unparse example.py
cargo run -- check example.py
```

The CLI targets Python 3.13 by default. Use `--target-version=3.8` through
`--target-version=3.13` when tokenizing or parsing code for an older grammar;
this also selects the pre-3.12 legacy f-string token and location model.
When source uses syntax newer than the selected target, CLI parse/check
commands report `UnsupportedSyntax` and exit non-zero.
