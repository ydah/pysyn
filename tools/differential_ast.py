"""AST normalization used by the differential harness.

The Rust printer currently emits a CPython-shaped expression rather than a
location-complete ``ast.dump``.  We parse both dumps as data and remove only
empty/default fields.  Non-empty fields remain subject to comparison.
"""

from __future__ import annotations

import ast
import json
import subprocess
from typing import Any


CPYTHON_AST_PROGRAM = r'''
import ast
import json
import sys

INCLUDE_ATTRIBUTES = bool(int(sys.argv[1]))

try:
    tree = ast.parse(sys.stdin.read(), filename="<differential>")
except (SyntaxError, ValueError) as error:
    print(json.dumps({
        "ok": False,
        "message": str(error),
        "lineno": getattr(error, "lineno", None),
        "offset": getattr(error, "offset", None),
    }))
else:
    print(json.dumps({"ok": True, "dump": ast.dump(tree, include_attributes=INCLUDE_ATTRIBUTES)}))
'''


def cpython_ast(
    command: str, source: str, timeout: float, include_attributes: bool = False
) -> tuple[bool, Any]:
    """Parse source with a selected CPython executable."""

    try:
        completed = subprocess.run(
            [command, "-c", CPYTHON_AST_PROGRAM, "1" if include_attributes else "0"],
            input=source,
            text=True,
            capture_output=True,
            timeout=timeout,
            check=False,
        )
    except (OSError, subprocess.TimeoutExpired) as error:
        return False, {"kind": "reference-process", "message": str(error)}
    if completed.returncode != 0:
        return False, {"kind": "reference-process", "message": completed.stderr.strip()}
    try:
        result = json.loads(completed.stdout)
    except json.JSONDecodeError as error:
        return False, {"kind": "reference-output", "message": str(error)}
    if not result.get("ok"):
        return False, result
    try:
        return True, normalize_dump(ast.parse(result["dump"], mode="eval").body)
    except (SyntaxError, KeyError, TypeError) as error:
        return False, {"kind": "reference-dump", "message": str(error)}


def normalize_dump(node: ast.AST | ast.expr | Any) -> Any:
    """Convert a CPython-shaped dump expression into comparable data."""

    if isinstance(node, ast.Call):
        name = node.func.id if isinstance(node.func, ast.Name) else ast.dump(node.func)
        fields = {keyword.arg: normalize_dump(keyword.value) for keyword in node.keywords}
        return {"_type": name, **normalize_fields(fields, name)}
    if isinstance(node, ast.List):
        return [normalize_dump(element) for element in node.elts]
    if isinstance(node, ast.Tuple):
        return [normalize_dump(element) for element in node.elts]
    if isinstance(node, ast.Constant):
        return normalize_scalar(node.value)
    if isinstance(node, ast.Name):
        return {"_name": node.id}
    if isinstance(node, ast.UnaryOp) and isinstance(node.op, ast.USub):
        return -normalize_dump(node.operand)
    if isinstance(node, ast.Dict):
        return {str(normalize_dump(key)): normalize_dump(value) for key, value in zip(node.keys, node.values)}
    return ast.dump(node, include_attributes=False)


def normalize_scalar(value: Any) -> Any:
    """Map lone surrogate code points to the escaped form Rust can retain."""

    if not isinstance(value, str) or not any(0xD800 <= ord(character) <= 0xDFFF for character in value):
        return value
    return "".join(
        f"\\u{ord(character):04x}" if 0xD800 <= ord(character) <= 0xDFFF else character
        for character in value
    )


def normalize_fields(fields: dict[str, Any], node_type: str = "") -> dict[str, Any]:
    """Remove fields that are empty in one CPython version or printer output."""

    return {
        name: value
        for name, value in fields.items()
        if value not in (None, [], "")
        and not (name == "value" and node_type != "Constant")
        or (name in {"id", "name", "arg", "module", "attr"} and value is not None)
        or (name == "value" and node_type == "Constant")
    }


def dumps_equal(expected: Any, actual: Any) -> bool:
    """Compare normalized AST data recursively."""

    if isinstance(expected, dict) and isinstance(actual, dict):
        if expected.get("_type") != actual.get("_type"):
            return False
        keys = set(expected) | set(actual)
        keys.discard("_type")
        return all(dumps_equal(expected.get(key), actual.get(key)) for key in keys)
    if isinstance(expected, list) and isinstance(actual, list):
        return len(expected) == len(actual) and all(
            dumps_equal(left, right) for left, right in zip(expected, actual)
        )
    return expected == actual
