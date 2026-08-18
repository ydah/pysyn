# Known differences

The project is developed incrementally against CPython's tokenizer and AST.
This file records behavior that is deliberately not yet complete so that
differential tests can distinguish implementation gaps from regressions.

- Encoding detection beyond UTF-8 is feature-gated and does not decode bytes
  without an explicit integration layer yet.
- The first parser pass focuses on Python's core expression and statement
  grammar; less common 3.12/3.13 syntax is added as its AST representation is
  completed.
- The public AST keeps literal categories as distinct Rust variants and folds
  them into CPython `Constant` nodes only in the dump printer.
