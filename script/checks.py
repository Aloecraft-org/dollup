"""dollup's own consistency invariants, called by script/changelog.py.

What does not generalise stays here (doc/ALIGNMENT.md §3): the engine
checks the declared version stamps; this checks the facts the tree
already carries against the newest changelog entry.

`consistency(doc, ctx) -> list[str]`; `ctx` carries `read` (a file under
the repo root, as text), `root` and `base` (the newest entry's X.Y.Z).
"""
import re


def consistency(doc, ctx):
    bad = []
    r = doc["releases"][0]
    where = "newest entry (%s)" % r.get("version")

    # The embedded drt-config revision is a compatibility fact the release
    # publishes in BUILDINFO.txt, so a changelog claiming one revision while
    # Cargo.lock pins another would put a wrong fact beside the bytes.
    want = r.get("drt_config")
    if want:
        try:
            lock = ctx["read"]("Cargo.lock")
        except OSError as e:
            return ["Cargo.lock: cannot read (%s)" % e]
        m = re.search(r'name = "drt-config"\nversion = "[^"]*"\n'
                      r'source = "git\+[^#]*#([0-9a-f]+)"', lock)
        if not m:
            bad.append("Cargo.lock: no git revision pinned for drt-config")
        elif not m.group(1).startswith(str(want)[:12]):
            bad.append("Cargo.lock pins drt-config %s but %s says %s"
                       % (m.group(1)[:12], where, str(want)[:12]))
    return bad
