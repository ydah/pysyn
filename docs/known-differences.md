# Known differences

The project is developed incrementally against CPython's tokenizer and AST.
This file records behavior that is deliberately not yet complete so that
differential tests can distinguish implementation gaps from regressions.

- Encoding detection beyond UTF-8 is feature-gated and does not decode bytes
  without an explicit integration layer yet.
- Python 3.12/3.13 type-parameter syntax, `except*`, parenthesized `with`, and
  the common structural-pattern forms are represented. Exact CPython error
  wording and recovery spans are still intentionally not byte-for-byte
  compatible.
- Python 3.14-only template strings are outside the supported 3.8–3.13
  version range.
- The public AST keeps literal categories as distinct Rust variants and folds
  them into CPython `Constant` nodes only in the dump printer.
