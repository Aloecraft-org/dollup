#!/usr/bin/env python3
"""render.py — assemble dollup.aloecraft.org into a directory.

    python3 site/render.py --out site/_out

Called by site/build.sh, the contract entry point. Offline, hermetic and
idempotent: it reads only this checkout and writes only --out, and the same
inputs produce the same bytes (nothing here stamps a date).

Two pages can come out of one template, and which one is decided by one
file:

  site/std-repo.pub        the public half of the std-repo signing key

With it present the page shows the key, the four-command start, and the
live package table, which the page fills from /std-repo/index.json at
load. The tree behind that URL is not this repository's: it lives in
drt-std-lib, which implements the same site contract and is staged by the
deployment tooling as a sibling subtree under this vhost. With the key
absent the page says the standard repo is not published yet.

The private key is never needed here: signing is `dollup repo publish`,
run by the publisher in drt-std-lib. The key file committed here is what
`dollup init` pins (crates/dollup/src/root.rs) and what the page shows,
and the two must match.
"""

import argparse
import os
import re
import shutil
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
REPO = os.path.dirname(HERE)
TEMPLATE = os.path.join(HERE, "template", "index.html")
PUBKEY = os.path.join(HERE, "std-repo.pub")
TOKEN = "__DOLLUP_STD_PUBKEY__"
BLOCK = re.compile(r"<!--IF:KEY-->\n(.*?)(?:<!--ELSE:KEY-->\n(.*?))?<!--END:KEY-->\n", re.S)


def signed_mode():
    return os.path.isfile(PUBKEY)


def read_pubkey():
    with open(PUBKEY) as fh:
        key = fh.read().strip()
    if not key.startswith("ed25519:"):
        sys.exit("site/std-repo.pub does not hold an `ed25519:` key: %r" % key[:40])
    return key


def render_page(signed, pubkey):
    with open(TEMPLATE, encoding="utf-8") as fh:
        text = fh.read()

    def pick(m):
        return m.group(1) if signed else (m.group(2) or "")

    text = BLOCK.sub(pick, text)
    if signed:
        text = text.replace(TOKEN, pubkey)
    if TOKEN in text:
        sys.exit("a %s token survived rendering; the template's IF:KEY blocks are wrong" % TOKEN)
    return text


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--out", required=True)
    args = ap.parse_args()
    out = os.path.abspath(args.out)

    signed = signed_mode()
    pubkey = read_pubkey() if signed else None

    os.makedirs(out, exist_ok=True)
    with open(os.path.join(out, "index.html"), "w", encoding="utf-8") as fh:
        fh.write(render_page(signed, pubkey))
    # The install one-liner the page shows. Same file the release carries.
    shutil.copy2(os.path.join(REPO, "install.sh"), os.path.join(out, "install.sh"))

    if signed:
        print("   signed: key stamped; the page reads /std-repo/index.json, served from drt-std-lib")
    else:
        print("   unsigned: no site/std-repo.pub, so the page says the standard repo is not "
              "published yet")


if __name__ == "__main__":
    main()
