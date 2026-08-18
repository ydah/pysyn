# Known differences

The project is developed incrementally against CPython's tokenizer and AST.
This file records behavior that is deliberately not yet complete so that
differential tests can distinguish implementation gaps from regressions.

- Encoding detection beyond UTF-8 is feature-gated. `SourceFile::from_bytes`
  and the CLI decode the supported declared encodings when the `encoding`
  feature is enabled; the default build rejects non-UTF-8 input.
- Python 3.12/3.13 type-parameter syntax, `except*`, parenthesized `with`, and
  the common structural-pattern forms are represented. Exact CPython error
  wording and recovery spans are still intentionally not byte-for-byte
  compatible.
- Python 3.14-only template strings are outside the supported 3.8–3.13
  version range.
- The public AST keeps literal categories as distinct Rust variants and folds
  them into CPython `Constant` nodes only in the dump printer.
- AST nodes retain exact UTF-8 byte ranges, while the dump API does not yet
  emit CPython's line/column location attributes. Differential AST checks
  therefore compare structural fields and intentionally omit locations.
