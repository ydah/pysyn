#!/usr/bin/env python3
"""Extract expression source snippets from Python files for parser fixtures."""

import ast
import pathlib
import sys


def extract(path: pathlib.Path) -> list[str]:
    source = path.read_text()
    tree = ast.parse(source)
    return [ast.get_source_segment(source, node) for node in ast.walk(tree) if isinstance(node, ast.expr)]


if __name__ == "__main__":
    for argument in sys.argv[1:]:
        for expression in extract(pathlib.Path(argument)):
            if expression:
                print(expression)

