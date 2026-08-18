"""Comparison operations for the local CPython differential harness."""

from __future__ import annotations

import ast
import json
from dataclasses import asdict, dataclass

from differential_ast import cpython_ast, dumps_equal, normalize_dump
from differential_cases import Case
from differential_process import run_process
from differential_tokens import actual_tokens, first_difference, has_fstring, reference_tokens


@dataclass
class Finding:
    """One comparison result, suitable for human output and JSON reports."""

    case: str
    mode: str
    status: str
    detail: str = ""


def compare_tokens(case: Case, command: str, pysyn: str, timeout: float, include_fstrings: bool) -> Finding:
    """Compare exact token type, span, and source text."""

    if has_fstring(case.source) and not include_fstrings:
        return Finding(case.name, "token", "skipped", "f-string tokenization is version-dependent")
    reference_ok, expected = reference_tokens(command, case.source, timeout)
    if not reference_ok:
        return Finding(case.name, "token", "reference-error")
    status, actual = actual_tokens(pysyn, case.source, timeout, case.path)
    if status != "ok":
        return Finding(case.name, "token", "pysyn-error", status)
    if expected == actual:
        return Finding(case.name, "token", "pass")
    return Finding(case.name, "token", "mismatch", first_difference(expected, actual))


def compare_ast(case: Case, command: str, pysyn: str, timeout: float, strict: bool) -> Finding:
    """Compare a CPython AST dump with the Rust printer output."""

    reference_ok, expected = cpython_ast(command, case.source, timeout, include_attributes=strict)
    if not reference_ok:
        return Finding(case.name, "ast", "reference-syntax", json.dumps(expected, ensure_ascii=False))
    argument = str(case.path) if case.path is not None else "-"
    stdin = "" if case.path is not None else case.source
    stdout, stderr, status = run_process([pysyn, "dump", argument], stdin, timeout)
    if status != 0:
        return Finding(case.name, "ast", "pysyn-error", str(status))
    try:
        actual = normalize_dump(ast.parse(stdout.strip(), mode="eval").body)
    except (SyntaxError, ValueError) as error:
        return Finding(case.name, "ast", "malformed-output", str(error))
    if dumps_equal(expected, actual):
        return Finding(case.name, "ast", "pass")
    return Finding(case.name, "ast", "mismatch", "normalized CPython AST differs")


def compare_roundtrip(case: Case, command: str, pysyn: str, timeout: float) -> Finding:
    """Check that pysyn's unparse preserves CPython's structural AST."""

    reference_ok, expected = cpython_ast(command, case.source, timeout)
    if not reference_ok:
        return Finding(case.name, "roundtrip", "reference-syntax")
    argument = str(case.path) if case.path is not None else "-"
    stdin = "" if case.path is not None else case.source
    output, stderr, status = run_process([pysyn, "unparse", argument], stdin, timeout)
    if status != 0:
        return Finding(case.name, "roundtrip", "pysyn-error", str(status))
    reparsed_ok, reparsed = cpython_ast(command, output, timeout)
    if not reparsed_ok:
        return Finding(case.name, "roundtrip", "unparse-invalid", json.dumps(reparsed, ensure_ascii=False))
    check_output, check_error, check_status = run_process([pysyn, "check", "-"], output, timeout)
    if check_status != 0:
        return Finding(case.name, "roundtrip", "unparse-rejected-by-pysyn", str(check_status))
    if dumps_equal(expected, reparsed):
        return Finding(case.name, "roundtrip", "pass")
    return Finding(case.name, "roundtrip", "mismatch", "unparse changed the CPython AST")


def finding_json(finding: Finding) -> dict[str, str]:
    """Serialize a finding for the optional report file."""

    return asdict(finding)
