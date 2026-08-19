# Differential verification

`tools/differential.py` compares the CLI with a selected CPython executable
without downloading a corpus. Structural comparisons support Python 3.8–3.13;
strict AST location comparisons support Python 3.10–3.13.
With no paths it runs a small version-aware smoke corpus. Passing files or
directories makes the same checks run against a local corpus.

The three checks are:

- `token`: token type, source span, and token text from `tokenize.generate_tokens`.
  F-string token cases are skipped unless `--include-fstrings` is supplied.
- `ast`: a structural comparison of the CPython AST and `pysyn dump`. Empty
  optional fields are ignored so Python-version additions such as `type_params`
  do not create false positives. The legacy Python 3.8 `ast.Index` wrapper is
  normalized to the modern `Subscript.slice` form. `--strict-ast` also compares
  CPython location attributes; the CLI dump includes them by default, while
  `--no-attributes` selects the structural form.
- `roundtrip`: parses `pysyn unparse` with CPython and compares its structural
  AST with the original. It also checks that pysyn accepts its own output.

Build the binary first, then run the smoke corpus:

```bash
cargo build --all-features --bin pysyn
python3 tools/differential.py \
  --python python3.13 \
  --pysyn target/debug/pysyn \
  --mode all \
  --include-fstrings \
  --strict-ast
```

The CI differential job builds the binary with `cargo build --all-features`
and always passes `--include-fstrings` for each CPython 3.10–3.13 matrix entry.
It also passes `--strict-ast`, so location attributes are checked in CI.
This keeps the CI binary aligned with the feature-complete test build and
prevents f-string token or AST-location coverage from being silently skipped.
The harness passes the matching `--target-version` to every CLI invocation;
without that option a CLI invocation intentionally defaults to Python 3.13.
Syntax accepted by the host CPython but newer than the selected target is
reported as `UnsupportedSyntax` by CLI parse/check commands.

Compare a local corpus without copying it into the repository:

```bash
PYTHON=python3.12
PYTHON_STDLIB="$($PYTHON -c 'import sysconfig; print(sysconfig.get_path("stdlib"))')"
python3 tools/differential.py \
  --python "$PYTHON" \
  --pysyn target/debug/pysyn \
  --mode all \
  --include-fstrings \
  --strict-ast \
  "$PYTHON_STDLIB"
```

The CI matrix uses the standard-library directory belonging to each matrix
interpreter, so the Python 3.10–3.13 jobs exercise the local `Lib/` tree rather
than only the built-in smoke cases. The CI corpus excludes `*/test/*` and
`*/lib2to3/tests/*`: these directories contain CPython's test fixtures,
including intentionally invalid syntax and legacy Python 2 examples, rather
than production standard-library modules. The dedicated syntax diagnostics
job still exercises `test/test_syntax.py`. Remove the `--exclude` options when
you explicitly want to inspect those fixtures.

The command exits non-zero for mismatches, parser errors, malformed output,
timeouts, or process crashes. `--report report.json` writes machine-readable
results suitable for CI artifacts. It does not fetch files or require a
network connection.

## Syntax diagnostics

`tools/syntax_validation.py` extracts the rejected doctest examples from
CPython's `test/test_syntax.py` and compares both rejection and the displayed
line/column. The CI gate requires 100% detection and at least 95% location
agreement:

```bash
python3 tools/syntax_validation.py \
  --python python3.13 \
  --pysyn target/debug/pysyn \
  --test-syntax "$(python3.13 -c 'import sysconfig; print(sysconfig.get_path("stdlib"))')/test/test_syntax.py" \
  --min-detection 1.0 \
  --min-position 0.95 \
  --report syntax-validation.json
```

The report retains each mismatch so diagnostic regressions can be reviewed
without relying on the aggregate percentage alone.

## Fuzz smoke test

`tools/fuzz_smoke.py` feeds deterministic malformed UTF-8 text-safe inputs to
fresh parser processes and rejects signals, panic output, stack overflow, and
timeouts. It is a bounded crash-regression check, not a replacement for a
long-running `cargo-fuzz` campaign.

```bash
python3 tools/fuzz_smoke.py \
  --pysyn target/debug/pysyn \
  --cases 500 \
  --seed 313
```

The scheduled CI job runs 2,000 cases with a fixed seed and a one-second
per-process timeout. No corpus is persisted. This is explicitly a Python
subprocess smoke test: it checks reproducible malformed inputs for crashes,
panics, stack overflows, and hangs.

The repository also contains `cargo-fuzz` targets for arbitrary bytes and
structured parser input. The scheduled job runs each for ten minutes before
running the deterministic Python smoke test. The release gate requires a
separate 24-hour run for each target:

```bash
cargo +nightly fuzz run parse_bytes -- -max_total_time=86400
cargo +nightly fuzz run parse_structured -- -max_total_time=86400
```

Criterion benchmarks for tokenization, parsing, and the complete
parse/dump/unparse path are available through `cargo bench --bench parser`;
the main-branch job caches Criterion's baseline directory so repeated runs can
report regressions. The main-branch job passes
`--allow-missing-baseline`, so a first run after a cache miss emits a notice and
seeds the cache; later runs enforce the 10% regression budget. Direct use of
`tools/benchmark_gate.py` remains strict unless that option is supplied.

## Coverage

The CI `coverage` job installs Python 3.13, points
`PYSYN_COVERAGE_STDLIB` at that interpreter's standard library, and runs the
complete workspace test suite with `cargo-llvm-cov`. The corpus-driven test
exercises parser, printer, validator, visitor, source, and CLI paths, and CI
enforces the design-document's 85% line-coverage target with
`--fail-under-lines 85` while uploading `lcov.info`.

Run the same coverage gate locally with:

```bash
PYSYN_COVERAGE_STDLIB="$(python3.13 -c 'import sysconfig; print(sysconfig.get_path("stdlib"))')" \
cargo llvm-cov --all-features --workspace --fail-under-lines 85
```

## CI matrix

The `differential` job runs the complete standard-library corpus once for each
CPython 3.10–3.13 interpreter using an all-features binary, f-string token
coverage, and strict AST location checking, then uploads the JSON report even
when a comparison fails. The scheduled `fuzz` job is independent of the
differential matrix and is the deterministic Python smoke test described above.
Third-party corpora remain opt-in local inputs; CI does not download them.
