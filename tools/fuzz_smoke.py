#!/usr/bin/env python3
"""Deterministic subprocess fuzz smoke test for parser crash regressions."""

from __future__ import annotations

import argparse
import random
import subprocess
import sys


SEEDS = (
    "",
    ":\n",
    "def f(:\n    pass\n",
    "if True\n    pass\n",
    "[x for x in ]\n",
    "match value:\n    case {\n",
    "f'{value'\n",
    "'unterminated\n",
    "x = 1e+\n",
    "try:\n    pass\nexcept*:\n",
)
ALPHABET = "abcdefghijklmnopqrstuvwxyz_0123456789 ()[]{}:,.'\"\\n+-*/%<>=!&|@;"


def cases(count: int, seed: int) -> list[str]:
    """Generate reproducible malformed and semi-structured UTF-8 inputs."""

    generator = random.Random(seed)
    generated = list(SEEDS)
    for _ in range(max(0, count - len(generated))):
        length = generator.randrange(0, 160)
        generated.append("".join(generator.choice(ALPHABET) for _ in range(length)))
    return generated[:count]


def run(pysyn: str, sources: list[str], timeout: float) -> int:
    """Run each input in a fresh process and reject timeouts or crashes."""

    failures: list[tuple[int, str]] = []
    for index, source in enumerate(sources):
        try:
            completed = subprocess.run(
                [pysyn, "check", "-"], input=source, text=True, capture_output=True, timeout=timeout, check=False
            )
        except subprocess.TimeoutExpired:
            failures.append((index, "timeout"))
            continue
        except OSError as error:
            print(f"cannot execute {pysyn!r}: {error}", file=sys.stderr)
            return 2
        if completed.returncode < 0 or "panicked at" in completed.stderr or "stack overflow" in completed.stderr.lower():
            failures.append((index, f"crash returncode={completed.returncode}: {completed.stderr[:160]}"))
    print(f"fuzz smoke: {len(sources)} inputs, crashes_or_timeouts={len(failures)}")
    for index, detail in failures[:20]:
        print(f"FAIL input {index}: {detail}", file=sys.stderr)
    return 1 if failures else 0


def main() -> int:
    """Parse arguments and execute the smoke test."""

    argument_parser = argparse.ArgumentParser(description=__doc__)
    argument_parser.add_argument("--pysyn", required=True)
    argument_parser.add_argument("--cases", type=int, default=500)
    argument_parser.add_argument("--seed", type=int, default=313)
    argument_parser.add_argument("--timeout", type=float, default=1.0)
    args = argument_parser.parse_args()
    return run(args.pysyn, cases(args.cases, args.seed), args.timeout)


if __name__ == "__main__":
    raise SystemExit(main())
