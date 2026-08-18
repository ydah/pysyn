#!/usr/bin/env python3
"""Run token, AST, and round-trip comparisons against a selected CPython.

No corpus is downloaded. With no path arguments the command uses a small
version-aware smoke corpus; local files or directories can be supplied for
larger, explicitly managed corpora.
"""

from __future__ import annotations

import argparse
import json
import os
import shutil
import sys
from collections import Counter
from pathlib import Path

from differential_cases import builtin_cases, source_cases
from differential_compare import Finding, compare_ast, compare_roundtrip, compare_tokens, finding_json


def parser() -> argparse.ArgumentParser:
    """Build the command-line interface."""

    result = argparse.ArgumentParser(description=__doc__)
    result.add_argument("paths", nargs="*", type=Path, help="Python files/directories to compare")
    result.add_argument("--python", default=sys.executable, help="CPython executable (3.10–3.13)")
    result.add_argument("--pysyn", default=os.environ.get("PYSYN_BIN"), help="pysyn executable")
    result.add_argument("--mode", choices=("token", "ast", "roundtrip", "all"), default="all")
    result.add_argument("--limit", type=int, help="maximum number of path cases")
    result.add_argument("--timeout", type=float, default=5.0, help="per-process timeout in seconds")
    result.add_argument("--include-fstrings", action="store_true", help="include version-dependent f-string tokens")
    result.add_argument("--strict-ast", action="store_true", help="fail AST checks when location-complete output is unavailable")
    result.add_argument("--max-failures", type=int, default=20, help="number of failures printed")
    result.add_argument("--report", type=Path, help="write a JSON report")
    return result


def resolve_pysyn(explicit: str | None) -> str:
    """Resolve a built binary without invoking a network or build step."""

    candidates = [explicit] if explicit else []
    candidates.extend(("target/debug/pysyn", "target/release/pysyn", shutil.which("pysyn")))
    for candidate in candidates:
        if candidate and Path(candidate).exists():
            return candidate
    raise SystemExit("pysyn executable not found; run `cargo build --bin pysyn` and pass --pysyn")


def python_version(command: str) -> tuple[int, int]:
    """Read the interpreter version without importing implementation-specific APIs."""

    import subprocess

    completed = subprocess.run([command, "-c", "import sys; print(sys.version_info[0], sys.version_info[1])"], capture_output=True, text=True, check=False)
    if completed.returncode != 0:
        raise SystemExit(f"cannot run {command!r}: {completed.stderr.strip()}")
    major, minor = (int(part) for part in completed.stdout.split())
    return major, minor


def run(args: argparse.Namespace) -> int:
    """Execute requested comparisons and return a process status."""

    version = python_version(args.python)
    if version not in {(3, 10), (3, 11), (3, 12), (3, 13)}:
        print(f"warning: requested CPython is {version[0]}.{version[1]}; supported matrix is 3.10–3.13", file=sys.stderr)
    pysyn = resolve_pysyn(args.pysyn)
    cases = source_cases(args.paths, args.limit) if args.paths else builtin_cases(version)
    if args.limit is not None and not args.paths:
        cases = cases[: args.limit]
    if not cases:
        raise SystemExit("no Python cases selected")

    findings: list[Finding] = []
    for case in cases:
        if args.mode in ("token", "all"):
            findings.append(compare_tokens(case, args.python, pysyn, args.timeout, args.include_fstrings))
        if args.mode in ("ast", "all"):
            findings.append(compare_ast(case, args.python, pysyn, args.timeout, args.strict_ast))
        if args.mode in ("roundtrip", "all"):
            findings.append(compare_roundtrip(case, args.python, pysyn, args.timeout))

    counts = Counter(finding.status for finding in findings)
    print(f"CPython {version[0]}.{version[1]}: {len(cases)} cases, " + ", ".join(f"{key}={value}" for key, value in sorted(counts.items())))
    failures = [finding for finding in findings if finding.status not in {"pass", "skipped", "reference-syntax"}]
    for finding in failures[: args.max_failures]:
        suffix = f": {finding.detail}" if finding.detail else ""
        print(f"FAIL {finding.mode:9} {finding.case}{suffix}", file=sys.stderr)
    if args.report:
        args.report.parent.mkdir(parents=True, exist_ok=True)
        args.report.write_text(json.dumps({"python": version, "cases": len(cases), "findings": [finding_json(f) for f in findings]}, indent=2) + "\n")
    return 1 if failures else 0


if __name__ == "__main__":
    raise SystemExit(run(parser().parse_args()))
