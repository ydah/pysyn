#!/usr/bin/env python3
"""Generate CPython AST dump fixtures for a source tree."""

import ast
import pathlib
import sys


def generate(source_root: pathlib.Path, output_root: pathlib.Path) -> None:
    for path in sorted(source_root.rglob("*.py")):
        output = (output_root / path.relative_to(source_root)).with_suffix(".expected")
        output.parent.mkdir(parents=True, exist_ok=True)
        try:
            tree = ast.parse(path.read_bytes(), filename=str(path))
        except SyntaxError as error:
            output.write_text(f"SyntaxError: {error.msg}\nlineno={error.lineno} offset={error.offset}\n")
        else:
            output.write_text(ast.dump(tree, include_attributes=True, indent=2) + "\n")


if __name__ == "__main__":
    generate(pathlib.Path(sys.argv[1]), pathlib.Path(sys.argv[2]))

