#!/usr/bin/env python3
"""Check a landing page against the things that must survive a redesign.

The page carries load-bearing details a designer has no reason to know
about: a token the publish script substitutes, the element the live package
table fills in, the fetch that fills it. Losing one of those does not look
broken — the page renders fine and quietly stops working — so this checks
mechanically instead of by eye.

    python3 web/check.py                  # check web/index.html
    python3 web/check.py candidate.html   # check something before adopting it
"""
import html.parser
import pathlib
import re
import sys

# Phrases the copy was deliberately chosen to say. A design pass may
# re-typeset them; rewriting them is a different job with a different review.
ANCHOR_PHRASES = [
    "Install",
    "deployment",
    "Nothing executes during an install",
    "dollup add",
    "dollup source add",
]


def check(path: pathlib.Path) -> list[str]:
    text = path.read_text(encoding="utf-8")
    bad = []

    n = text.count("__DOLLUP_STD_PUBKEY__")
    if n != 2:
        bad.append(
            f"__DOLLUP_STD_PUBKEY__ appears {n}x, expected 2 "
            "(the source-add command and the key panel) — publish.sh "
            "substitutes it, so a lost token ships a page nobody can verify against"
        )

    if 'id="pkgs"' not in text:
        bad.append('no element with id="pkgs" — the live package table has nowhere to render')
    if "/std-repo/index.json" not in text:
        bad.append("nothing fetches /std-repo/index.json — the package table would stay empty")
    if 'rel="icon"' not in text:
        bad.append('no <link rel="icon"> — the page will 404 for a favicon')
    if "prefers-color-scheme" not in text:
        bad.append("no prefers-color-scheme block — the page must work in dark and light")

    # Every byte is served from our own origin, on the page that hands out a
    # signing key. A webfont CDN is the usual way this gets lost.
    for pattern, what in [
        (r"<script[^>]+src=[\"']https?://", "external <script src>"),
        (r"<link[^>]+rel=[\"']stylesheet[\"'][^>]*href=[\"']https?://", "external stylesheet"),
        (r"@import\s+url\(\s*[\"']?https?://", "@import of a remote stylesheet"),
        (r"url\(\s*[\"']?https?://", "remote url() (webfont or image)"),
        (r"<img[^>]+src=[\"']https?://", "remote <img>"),
    ]:
        if re.search(pattern, text, re.I):
            bad.append(f"{what} — the page must make no third-party requests")

    for phrase in ANCHOR_PHRASES:
        if phrase not in text:
            bad.append(f"copy anchor missing: {phrase!r}")

    class P(html.parser.HTMLParser):
        def error(self, message):
            raise ValueError(message)

    try:
        P().feed(text)
    except Exception as exc:  # noqa: BLE001 - report whatever the parser says
        bad.append(f"does not parse as HTML: {exc}")

    return bad


def main() -> int:
    path = pathlib.Path(sys.argv[1] if len(sys.argv) > 1 else "web/index.html")
    if not path.is_file():
        print(f"no such file: {path}", file=sys.stderr)
        return 2
    problems = check(path)
    for problem in problems:
        print(f"  ✗ {problem}", file=sys.stderr)
    if problems:
        print(f"\n{path}: {len(problems)} problem(s)", file=sys.stderr)
        return 1
    print(f"{path}: ok")
    return 0


if __name__ == "__main__":
    sys.exit(main())
