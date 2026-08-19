#!/usr/bin/env python3
"""Regression tests for the differential AST normalizer."""

from __future__ import annotations

import ast
import sys
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).parent))

from differential_ast import dumps_equal, normalize_dump


def normalized_dump(source: str):
    return normalize_dump(ast.parse(source, mode="eval").body)


class DifferentialAstTests(unittest.TestCase):
    def test_non_constant_value_fields_are_compared(self) -> None:
        left = normalized_dump("NamedExpr(target=Name(id='x'), value=Constant(value=1))")
        right = normalized_dump("NamedExpr(target=Name(id='x'), value=Constant(value=2))")
        self.assertFalse(dumps_equal(left, right))

    def test_empty_optional_fields_are_ignored(self) -> None:
        left = normalized_dump("Call(func=Name(id='f'))")
        right = normalized_dump("Call(func=Name(id='f'), args=[], keywords=[])")
        self.assertTrue(dumps_equal(left, right))


if __name__ == "__main__":
    unittest.main()
