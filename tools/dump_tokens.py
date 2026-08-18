#!/usr/bin/env python3
"""Print CPython tokenize tuples in a stable, diff-friendly format."""

import pathlib
import sys
import tokenize


def dump(path: pathlib.Path) -> None:
    with path.open("rb") as source:
        for token in tokenize.tokenize(source.readline):
            print(token.type, token.start, token.end, repr(token.string))


if __name__ == "__main__":
    for argument in sys.argv[1:]:
        dump(pathlib.Path(argument))

