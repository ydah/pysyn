#!/usr/bin/env python3
"""Fail when Criterion reports a material regression against its cached baseline."""

from __future__ import annotations

import argparse
import json
import sys
from pathlib import Path


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--criterion", type=Path, required=True)
    parser.add_argument("--max-regression", type=float, default=0.10)
    parser.add_argument(
        "--allow-missing-baseline",
        action="store_true",
        help="return success with a notice when no cached baseline exists",
    )
    return parser.parse_args()


def run(args: argparse.Namespace) -> int:
    changes = sorted(args.criterion.glob("*/**/change/estimates.json"))
    if not changes:
        if args.allow_missing_baseline:
            print(
                "notice: Criterion baseline unavailable; skipping the regression "
                "gate until a subsequent run has a cached baseline",
                file=sys.stderr,
            )
            return 0
        print(
            "error: Criterion baseline is unavailable; prime target/criterion "
            "before running the regression gate",
            file=sys.stderr,
        )
        return 2

    failures: list[tuple[str, float]] = []
    for path in changes:
        try:
            estimate = json.loads(path.read_text())
            regression = float(estimate["mean"]["point_estimate"])
        except (OSError, KeyError, TypeError, ValueError, json.JSONDecodeError) as error:
            print(f"error: invalid Criterion estimate {path}: {error}", file=sys.stderr)
            return 2
        if regression > args.max_regression:
            failures.append((str(path), regression))

    if failures:
        for path, regression in failures:
            print(
                f"benchmark regression exceeds {args.max_regression:.0%}: "
                f"{path} ({regression:.2%})",
                file=sys.stderr,
            )
        return 1

    print(f"benchmark regression gate passed for {len(changes)} measurements")
    return 0


if __name__ == "__main__":
    raise SystemExit(run(parse_args()))
