#!/usr/bin/env python3
"""render.py — assemble dollup.aloecraft.org into a directory.

    python3 site/render.py --out site/_out

Called by site/build.sh, the contract entry point. Offline, hermetic and
idempotent: it reads only this checkout and writes only --out, and the same
inputs produce the same bytes (nothing here stamps a date).

Two pages can come out of one template, and which one is decided by two
files that are either both committed or not:

  site/std-repo.pub        the public half of the std-repo signing key
  std-repo/index.json.sig  the signature over the committed index

With both present the page shows the key, the four-command start, and the
live package table, and std-repo/ ships beside it. With either absent the
page says the standard repo is not published yet and std-repo/ does NOT
ship -- an unsigned repo at the canonical URL is an invitation to pin it
unsigned, and "status means today" applies to a package repo as much as to
a portal link.

The private key is never needed here. Signing is `dollup repo publish`, run
by the publisher, who then commits the index, its signature and the public
key; this build only copies what is committed. That is what keeps it
hermetic, and it is why std-repo/publish.sh no longer deploys anything.
"""

import argparse
import hashlib
import json
import os
import re
import shutil
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
REPO = os.path.dirname(HERE)
TEMPLATE = os.path.join(HERE, "template", "index.html")
PUBKEY = os.path.join(HERE, "std-repo.pub")
STD_REPO = os.path.join(REPO, "std-repo")
TOKEN = "__DOLLUP_STD_PUBKEY__"
BLOCK = re.compile(r"<!--IF:KEY-->\n(.*?)(?:<!--ELSE:KEY-->\n(.*?))?<!--END:KEY-->\n", re.S)


def signed_mode():
    return os.path.isfile(PUBKEY) and os.path.isfile(os.path.join(STD_REPO, "index.json.sig"))


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


def sha256_file(path):
    h = hashlib.sha256()
    with open(path, "rb") as fh:
        for chunk in iter(lambda: fh.read(1 << 16), b""):
            h.update(chunk)
    return "sha256:" + h.hexdigest()


def copy_std_repo(out):
    """The four things a repo is, plus the blob projection a static mirror
    serves. blobs/ is derived here rather than committed: content-addressed,
    so the same tree always projects to the same files."""
    dst = os.path.join(out, "std-repo")
    os.makedirs(dst)
    for name in ("index.json", "index.json.sig"):
        shutil.copy2(os.path.join(STD_REPO, name), os.path.join(dst, name))
    shutil.copytree(os.path.join(STD_REPO, "packages"), os.path.join(dst, "packages"))

    with open(os.path.join(STD_REPO, "index.json")) as fh:
        index = json.load(fh)
    blobs = os.path.join(dst, "blobs", "sha256")
    os.makedirs(blobs)
    n = 0

    def project(path, want):
        nonlocal n
        have = sha256_file(path)
        if have != want:
            sys.exit("%s hashes to %s, index says %s -- re-run `dollup repo publish`"
                     % (os.path.relpath(path, REPO), have, want))
        target = os.path.join(blobs, want.split(":", 1)[1])
        if not os.path.exists(target):
            shutil.copy2(path, target)
            n += 1

    for versions in index.get("packages", {}).values():
        for entry in versions.get("versions", {}).values():
            pkg = os.path.join(STD_REPO, entry["path"])
            manifest_path = os.path.join(pkg, "manifest.json")
            project(manifest_path, entry["manifest"])
            with open(manifest_path) as fh:
                manifest = json.load(fh)
            for rel, want in manifest.get("files", {}).items():
                project(os.path.join(pkg, rel), want)
    return n


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
        n = copy_std_repo(out)
        print("   signed: key stamped, std-repo/ shipped, %d blob(s) projected" % n)
    else:
        print("   unsigned: no site/std-repo.pub + std-repo/index.json.sig pair, so the page "
              "says the standard repo is not published yet and std-repo/ is not shipped")


if __name__ == "__main__":
    main()
