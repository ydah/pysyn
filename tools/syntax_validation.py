#!/usr/bin/env python3
"""Measure pysyn's syntax-error detection against CPython's test_syntax.py."""

from __future__ import annotations

import argparse
import json
import re
import subprocess
import sys
from dataclasses import dataclass
from pathlib import Path


@dataclass(frozen=True)
class Case:
    """One doctest-style source sample that CPython rejects."""

    source: str
    line: int
    column: int


def parse_args() -> argparse.Namespace:
    """Parse command-line arguments."""

    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--python", default=sys.executable)
    parser.add_argument("--pysyn", required=True)
    parser.add_argument("--test-syntax", type=Path, required=True)
    parser.add_argument("--report", type=Path)
    parser.add_argument("--min-detection", type=float, default=1.0)
    parser.add_argument("--min-position", type=float, default=0.0)
    return parser.parse_args()


def prompt_examples(source: str) -> list[str]:
    """Extract doctest prompt blocks without requiring doctest formatting."""

    examples: list[str] = []
    current: list[str] = []
    for line in [*source.splitlines(), ""]:
        if line.startswith(">>> "):
            if current:
                examples.append("\n".join(current) + "\n")
            current = [line[4:]]
        elif line.startswith("... "):
            if current:
                current.append(line[4:])
        elif current:
            examples.append("\n".join(current) + "\n")
            current = []
    return examples


def cpython_error(command: str, source: str) -> tuple[int, int] | None:
    """Return CPython's syntax-error location, if compilation fails."""

    completed = subprocess.run(
        [command, "-c", "import sys; compile(sys.stdin.read(), '<syntax>', 'exec')"],
        input=source,
        text=True,
        capture_output=True,
        check=False,
    )
    if completed.returncode == 0:
        return None
    match = re.search(r"line (\d+)", completed.stderr)
    if not match:
        return None
    error = SyntaxError()
    try:
        compile(source, "<syntax>", "exec")
    except SyntaxError as caught:
        error = caught
    if error.lineno is None or error.offset is None:
        return None
    return error.lineno, error.offset - 1


def pysyn_location(stderr: str) -> tuple[int, int] | None:
    """Return pysyn's displayed zero-based column and one-based line."""

    line_match = re.search(r"line (\d+)", stderr)
    caret_line = next((line for line in stderr.splitlines() if "^" in line), None)
    if line_match is None or caret_line is None:
        return None
    return int(line_match.group(1)), caret_line.index("^") - 4


def run(args: argparse.Namespace) -> int:
    """Run detection and location comparisons."""

    cases: list[Case] = []
    for source in prompt_examples(args.test_syntax.read_text()):
        location = cpython_error(args.python, source)
        if location is not None:
            cases.append(Case(source, *location))

    detected = 0
    positioned = 0
    failures: list[dict[str, object]] = []
    for case in cases:
        completed = subprocess.run(
            [args.pysyn, "check", "-"],
            input=case.source,
            text=True,
            capture_output=True,
            check=False,
        )
        if completed.returncode == 0:
            failures.append({"source": case.source, "status": "missed"})
            continue
        detected += 1
        actual = pysyn_location(completed.stderr)
        if actual == (case.line, case.column):
            positioned += 1
        else:
            failures.append(
                {
                    "source": case.source,
                    "status": "location",
                    "expected": {"line": case.line, "column": case.column},
                    "actual": actual,
                }
            )

    total = len(cases)
    detection_rate = detected / total if total else 1.0
    position_rate = positioned / total if total else 1.0
    print(
        f"test_syntax.py: cases={total}, detected={detected} "
        f"({detection_rate:.1%}), positions={positioned} ({position_rate:.1%})"
    )
    if args.report:
        args.report.write_text(
            json.dumps(
                {
                    "cases": total,
                    "detected": detected,
                    "positioned": positioned,
                    "detection_rate": detection_rate,
                    "position_rate": position_rate,
                    "failures": failures,
                },
                indent=2,
            )
            + "\n"
        )
    if detection_rate < args.min_detection or position_rate < args.min_position:
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(run(parse_args()))
