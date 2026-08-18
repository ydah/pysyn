"""Subprocess execution primitives for differential checks."""

from __future__ import annotations

import subprocess


def run_process(command: list[str], source: str, timeout: float) -> tuple[str, str, int | str]:
    """Run a parser command and preserve timeout/crash information."""

    try:
        completed = subprocess.run(
            command, input=source, text=True, capture_output=True, timeout=timeout, check=False
        )
    except subprocess.TimeoutExpired:
        return "", "", "timeout"
    except OSError as error:
        return "", str(error), "process-error"
    if completed.returncode < 0:
        return completed.stdout, completed.stderr, f"signal:{-completed.returncode}"
    return completed.stdout, completed.stderr, completed.returncode
