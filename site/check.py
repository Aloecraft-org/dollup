#!/usr/bin/env python3
"""check.py — verify the things a design pass, or a merge, tends to drop.

    python3 site/check.py                  # template and site.json
    python3 site/check.py candidate.html   # a returned page, before adopting it

The page carries load-bearing details a designer has no reason to know
about: a token the build substitutes, the conditional that decides which of
two pages renders, the element the live package table fills, the fetch that
fills it, a script that must run before first paint. Losing one does not
look broken -- the page renders fine and quietly stops working -- so this
checks mechanically instead of by eye. site/build.sh runs it before every
build, and CI runs it too.
"""

import json
import os
import re
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
REPO = os.path.dirname(HERE)
TEMPLATE = os.path.join(HERE, "template", "index.html")
SITE_JSON = os.path.join(HERE, "site.json")
PUBKEY = os.path.join(HERE, "std-repo.pub")

# Phrases the copy was deliberately chosen to say. A design pass may
# re-typeset them; rewriting them is a different job with a different review.
ANCHOR_PHRASES = [
    "Install",
    "app",
    "Nothing executes on install",
    "dollup add",
    "dollup source add",
]

STATUSES = ("live", "planned")


def problems_in_template(text):
    bad = []

    n = text.count("__DOLLUP_STD_PUBKEY__")
    if n != 2:
        bad.append(
            "__DOLLUP_STD_PUBKEY__ appears %dx, expected 2 (the source-add command "
            "and the key panel) -- render.py substitutes it, so a lost token ships a "
            "page nobody can verify against" % n)

    # The conditional: three IF blocks (start, key panel, packages), each
    # closed. render.py picks a branch per block; an unclosed one swallows
    # the rest of the page.
    opens, elses, ends = (text.count("<!--IF:KEY-->"), text.count("<!--ELSE:KEY-->"),
                          text.count("<!--END:KEY-->"))
    if opens != ends or opens < 1:
        bad.append("IF:KEY / END:KEY markers do not pair (%d open, %d end) -- render.py "
                   "decides signed-vs-unsigned per block, and an unbalanced one swallows "
                   "the rest of the page" % (opens, ends))
    if elses > opens:
        bad.append("more ELSE:KEY than IF:KEY markers")
    for m in re.finditer(r"<!--IF:KEY-->\n(.*?)<!--END:KEY-->\n", text, re.S):
        if "__DOLLUP_STD_PUBKEY__" in (m.group(1).split("<!--ELSE:KEY-->")[1:] or [""])[0]:
            bad.append("the pubkey token appears in an ELSE:KEY branch, which is the "
                       "unsigned page -- it would render as a literal token")
    # Every token must sit inside an IF branch, or the unsigned page ships it.
    stripped = re.sub(r"<!--IF:KEY-->\n.*?<!--END:KEY-->\n", "", text, flags=re.S)
    if "__DOLLUP_STD_PUBKEY__" in stripped:
        bad.append("a pubkey token sits outside any IF:KEY block; the unsigned page would "
                   "carry it verbatim")

    if 'id="pkgs"' not in text:
        bad.append('no element with id="pkgs" -- the live package table has nowhere to render')
    if "/std-repo/index.json" not in text:
        bad.append("nothing fetches /std-repo/index.json -- the package table would stay empty")
    if "if (!body) return;" not in text:
        bad.append("the package-table script does not guard for the unsigned page, where "
                   'id="pkgs" does not exist; it would throw on load')
    if 'rel="icon"' not in text:
        bad.append('no <link rel="icon"> -- the page will 404 for a favicon')

    has_toggle = '[data-theme="light"]' in text and "dollup-theme" in text
    if not has_toggle and "prefers-color-scheme" not in text:
        bad.append("only one theme is defined -- the page needs a light and a dark, "
                   "selected by a persisted toggle or by prefers-color-scheme")
    if 'localStorage.getItem("dollup-theme")' not in text.split("</head>")[0]:
        bad.append("the pre-paint theme read is not in <head>; a light-mode visitor gets "
                   "a flash of dark on every load")

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
            bad.append("%s -- the page must make no third-party requests" % what)

    for phrase in ANCHOR_PHRASES:
        if phrase not in text:
            bad.append("copy anchor missing: %r" % phrase)
    return bad


def problems_in_site_json():
    with open(SITE_JSON) as fh:
        d = json.load(fh)
    bad = []
    for key in ("name", "title", "tagline", "vhost", "path", "source"):
        if not d.get(key):
            bad.append("site.json: no %s" % key)
    for c in d.get("channels", []):
        if c.get("status") not in STATUSES:
            bad.append("site.json: channel %r has status %r" % (c.get("title"), c.get("status")))
        if c.get("status") == "live" and not c.get("url"):
            bad.append("site.json: channel %r is live with no url" % c.get("title"))
    # Status means today: the package repo is served from drt-std-lib, so
    # whether it is 'live' is a hand edit made when that tree is actually
    # served -- but never without the key the page would show beside it.
    for c in d.get("channels", []):
        if "std-repo" in (c.get("url") or "") and c.get("status") == "live" \
                and not os.path.isfile(PUBKEY):
            bad.append("site.json: the package repo is 'live' but site/std-repo.pub is not "
                       "committed, so the page could not show the key it is pinned by")
    return bad


def report(label, bad):
    if bad:
        print("%s:" % label, file=sys.stderr)
        for b in bad:
            print("  FAIL  %s" % b, file=sys.stderr)
        sys.exit(1)
    print("  ok   %s" % label)


def main():
    if len(sys.argv) > 1:
        path = sys.argv[1]
        with open(path, encoding="utf-8") as fh:
            report(path, problems_in_template(fh.read()))
        return
    with open(TEMPLATE, encoding="utf-8") as fh:
        report(os.path.relpath(TEMPLATE, REPO), problems_in_template(fh.read()))
    report("site/site.json", problems_in_site_json())
    print("PASS")


if __name__ == "__main__":
    main()
