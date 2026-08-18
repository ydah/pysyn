# Differential verification

`tools/differential.py` compares the CLI with a selected CPython executable
without downloading a corpus. It supports Python 3.10, 3.11, 3.12, and 3.13.
With no paths it runs a small version-aware smoke corpus. Passing files or
directories makes the same checks run against a local corpus.

The three checks are:

- `token`: token type, source span, and token text from `tokenize.generate_tokens`.
  F-string token cases are skipped unless `--include-fstrings` is supplied.
- `ast`: a structural comparison of the CPython AST and `pysyn dump`. Empty
  optional fields are ignored so Python-version additions such as `type_params`
  do not create false positives. `--strict-ast` also includes CPython location
  attributes and is expected to fail until the printer emits them.
- `roundtrip`: parses `pysyn unparse` with CPython and compares its structural
  AST with the original. It also checks that pysyn accepts its own output.

Build the binary first, then run the smoke corpus:

```bash
cargo build --bin pysyn
python3 tools/differential.py \
  --python python3.13 \
  --pysyn target/debug/pysyn \
  --mode all
```

Compare a local corpus without copying it into the repository:

```bash
python3 tools/differential.py \
  --python python3.12 \
  --pysyn target/debug/pysyn \
  --mode ast \
  --limit 100 \
  corpus/stdlib
```

The command exits non-zero for mismatches, parser errors, malformed output,
timeouts, or process crashes. `--report report.json` writes machine-readable
results suitable for CI artifacts. It does not fetch files or require a
network connection.

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

The scheduled CI job runs 2,000 cases. No corpus is persisted.

## Coverage

The CI `coverage` job installs `cargo-llvm-cov`, runs the complete workspace
test suite, and uploads `lcov.info`. The report is intentionally measured but
not given a hard threshold here: the current parser has known untested
branches, and a threshold must be introduced together with an explicit,
reviewed baseline rather than making every pull request red by construction.

## CI matrix

The `differential` job runs the smoke corpus once for each CPython 3.10–3.13
interpreter and uploads the JSON report even when a comparison fails. The
scheduled `fuzz` job is independent of the differential matrix. Large CPython
or third-party corpora remain opt-in local inputs; they are not downloaded by
CI.
