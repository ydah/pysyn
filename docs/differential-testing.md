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
cargo build --all-features --bin pysyn
python3 tools/differential.py \
  --python python3.13 \
  --pysyn target/debug/pysyn \
  --mode all \
  --include-fstrings
```

The CI differential job builds the binary with `cargo build --all-features`
and always passes `--include-fstrings` for each CPython 3.10–3.13 matrix entry.
This keeps the CI binary aligned with the feature-complete test build and
prevents f-string token coverage from being silently skipped.

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

The scheduled CI job runs 2,000 cases with a fixed seed and a one-second
per-process timeout. No corpus is persisted. This is explicitly a Python
subprocess smoke test: it checks reproducible malformed inputs for crashes,
panics, stack overflows, and hangs.

The repository does not currently add `cargo-fuzz` targets or Criterion
benchmarks. The existing Cargo configuration has no fuzz or benchmark harness,
and introducing either would require additional Rust/Cargo changes outside
this CI/documentation-only update. Therefore the smoke job does not satisfy
the stronger design-document requirements for arbitrary-byte/structured
`cargo-fuzz` campaigns, long-running fuzzing, or benchmark-based 10% regression
detection. Those remain explicit follow-up work rather than being represented
as completed by CI.

## Coverage

The CI `coverage` job installs `cargo-llvm-cov`, runs the complete workspace
test suite, uploads `lcov.info`, and enforces a 40% line-coverage floor with
`--fail-under-lines 40`. The current test suite measured 43.4% line coverage
locally, so this is a conservative non-regression floor rather than a claim of
the design-document's 85% target. The 85% target remains unmet and should be
raised only after the missing parser, printer, validator, and CLI paths receive
tests.

## CI matrix

The `differential` job runs the smoke corpus once for each CPython 3.10–3.13
interpreter using an all-features binary and f-string token coverage, then
uploads the JSON report even when a comparison fails. The scheduled `fuzz` job
is independent of the differential matrix and is the deterministic Python
smoke test described above. Large CPython or third-party corpora remain opt-in
local inputs; they are not downloaded by CI.
