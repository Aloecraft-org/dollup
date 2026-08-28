#!/usr/bin/env python3
"""Catch collapsed `\\` line-continuations in string literals.

A literal containing a run of three or more spaces between words is almost
always a continuation whose backslash was lost, and these are user-facing
refusal messages — the text people read when dollup says no. Cheap to check,
embarrassing to ship.
"""
import re
import sys
import pathlib

bad = []
for path in pathlib.Path("crates").rglob("*.rs"):
    for lineno, line in enumerate(path.read_text().splitlines(), 1):
        for lit in re.findall(r'"((?:[^"\\]|\\.)*)"', line):
            # A literal that opens with indentation is presentation — an
            # aligned help column, where a run of spaces is the point. A lost
            # continuation always begins mid-sentence, so this separates them
            # without needing a marker on every help line.
            if lit.startswith("  "):
                continue
            if re.search(r"\w {3,}\w", lit):
                bad.append(f"{path}:{lineno}: {lit[:80]}")

for entry in bad:
    print(entry, file=sys.stderr)
sys.exit(1 if bad else 0)
