"""Token comparison primitives for the differential harness."""

from __future__ import annotations

import ast
import json
import re
import subprocess
from pathlib import Path

from differential_process import run_process


ACTUAL_TOKEN_NAMES = {
    0: "ENDMARKER",
    1: "NAME",
    2: "NUMBER",
    3: "STRING",
    4: "NEWLINE",
    5: "INDENT",
    6: "DEDENT",
    54: "OP",
    55: "OP",
    59: "FSTRING_START",
    60: "FSTRING_MIDDLE",
    61: "FSTRING_END",
    62: "COMMENT",
    63: "NL",
    64: "ERRORTOKEN",
    65: "ENCODING",
}

PYTHON_KEYWORDS = {
    "False", "None", "True", "and", "as", "assert", "async", "await", "break",
    "case", "class", "continue", "def", "del", "elif", "else", "except", "finally",
    "for", "from", "global", "if", "import", "in", "is", "lambda", "match",
    "nonlocal", "not", "or", "pass", "raise", "return", "try", "type", "while",
    "with", "yield",
}


def has_fstring(source: str) -> bool:
    """Detect f-string prefixes for versions whose tokenize model differs."""

    return bool(re.search(r"(?i)(?<![\w])(?:[rubf]{0,3}f[rub]{0,2}|[rubf]{0,3}rf)[\"']", source))


def reference_tokens(command: str, source: str, timeout: float) -> tuple[bool, list[tuple]]:
    """Collect version-independent token names from CPython."""

    program = (
        "import io,json,tokenize,sys; "
        "tokens=tokenize.generate_tokens(io.StringIO(sys.stdin.read()).readline); "
        "print(json.dumps([(tokenize.tok_name[t.type],t.start,t.end,t.string) for t in tokens]))"
    )
    try:
        completed = subprocess.run(
            [command, "-c", program], input=source, text=True, capture_output=True, timeout=timeout, check=False
        )
    except (OSError, subprocess.TimeoutExpired):
        return False, []
    if completed.returncode != 0:
        return False, []
    try:
        return True, [
            (token[0], tuple(token[1]), tuple(token[2]), token[3])
            for token in json.loads(completed.stdout)
        ]
    except (json.JSONDecodeError, TypeError):
        return False, []


def actual_tokens(
    command: str, source: str, timeout: float, path: Path | None = None
) -> tuple[str, list[tuple]]:
    """Parse the CLI's CPython-format token lines."""

    argument = str(path) if path is not None else "-"
    stdin = "" if path is not None else source
    stdout, stderr, status = run_process(
        [command, "tokenize", "--format=cpython", argument], stdin, timeout
    )
    if status != 0:
        return str(status), []
    tokens: list[tuple] = []
    try:
        for line in stdout.splitlines():
            match = re.match(r"^(\d+) \((\d+), (\d+)\) \((\d+), (\d+)\) ?(.*)$", line)
            if match is None:
                raise ValueError(f"unrecognized token line: {line!r}")
            token_type, start_line, start_column, end_line, end_column, text_literal = match.groups()
            text = ast.literal_eval(text_literal) if text_literal else ""
            numeric_type = int(token_type)
            if numeric_type == 65:
                continue
            token_name = ACTUAL_TOKEN_NAMES.get(numeric_type, f"TYPE_{numeric_type}")
            if token_name == "OP" and text in PYTHON_KEYWORDS:
                token_name = "NAME"
            tokens.append(
                (token_name, (int(start_line), int(start_column)), (int(end_line), int(end_column)), text)
            )
    except (SyntaxError, ValueError, TypeError):
        return "malformed-output", []
    if stderr.strip():
        return f"lexer-error:{stderr.strip()[:160]}", tokens
    return "ok", tokens


def first_difference(expected: list[tuple], actual: list[tuple]) -> str:
    """Describe the first token mismatch without dumping an entire corpus."""

    for index, (left, right) in enumerate(zip(expected, actual)):
        if left != right:
            return f"token {index}: expected={left!r} actual={right!r}"
    return f"token count: expected={len(expected)} actual={len(actual)}"
