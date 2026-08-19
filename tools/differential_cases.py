"""Case discovery for the local CPython differential harness."""

from __future__ import annotations

import fnmatch
import io
import tokenize
from dataclasses import dataclass
from pathlib import Path
from typing import Iterable


@dataclass(frozen=True)
class Case:
    """A named Python source sample and the first version that supports it."""

    name: str
    source: str
    minimum_version: tuple[int, int] = (3, 10)
    path: Path | None = None


BUILTIN_CASES = (
    Case("assignment", "value = 1 + 2\n"),
    Case("expressions", "result = [x * 2 for x in values if x > 0]\n"),
    Case("control-flow", "if ready:\n    run()\nelse:\n    recover()\n"),
    Case("function", "@decorator\ndef collect(value: int = 1, *args, **kwargs) -> int:\n    return value\n"),
    Case("imports", "from package import first as one, second\nimport os, sys\n"),
    Case("with-and-exceptions", "with resource() as handle:\n    try:\n        handle.read()\n    except OSError as error:\n        raise RuntimeError() from error\n"),
    Case("match", "match value:\n    case {\"kind\": item, **rest} if item:\n        result = item\n    case _:\n        result = None\n", (3, 10)),
    Case("except-star", "try:\n    work()\nexcept* ValueError as error:\n    report(error)\n", (3, 11)),
    Case("type-parameters", "def identity[T](value: T) -> T:\n    return value\n", (3, 12)),
    Case("type-parameter-default", "type Alias[T = int] = list[T]\n", (3, 13)),
    Case("f-string", "message = f\"value={value!r:>{width}}\"\n"),
)


def version_at_least(version: tuple[int, int], minimum: tuple[int, int]) -> bool:
    """Return whether a CPython version supports a case."""

    return version >= minimum


def builtin_cases(version: tuple[int, int]) -> list[Case]:
    """Return built-in cases supported by the requested CPython version."""

    return [case for case in BUILTIN_CASES if version_at_least(version, case.minimum_version)]


def matches_exclude(path: Path, patterns: Iterable[str]) -> bool:
    """Return whether a path matches an exclusion pattern in either form."""

    candidates = (path.as_posix(), path.resolve().as_posix())
    return any(fnmatch.fnmatch(candidate, pattern) for candidate in candidates for pattern in patterns)


def source_cases(
    paths: Iterable[Path],
    limit: int | None = None,
    exclude: Iterable[str] = (),
) -> list[Case]:
    """Read Python files from paths without downloading or materializing a corpus."""

    exclude_patterns = tuple(exclude)
    files: list[Path] = []
    for path in paths:
        if path.is_file() and path.suffix == ".py":
            files.append(path)
        elif path.is_dir():
            files.extend(candidate for candidate in sorted(path.rglob("*.py")) if candidate.is_file())

    cases: list[Case] = []
    for path in sorted(set(files)):
        if matches_exclude(path, exclude_patterns):
            continue
        if limit is not None and len(cases) >= limit:
            break
        try:
            raw = path.read_bytes()
            encoding, _ = tokenize.detect_encoding(io.BytesIO(raw).readline)
            source = raw.decode(encoding)
        except (OSError, LookupError, SyntaxError, UnicodeDecodeError):
            continue
        cases.append(Case(path.as_posix(), source, path=path))
    return cases
