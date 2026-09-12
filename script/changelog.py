#!/usr/bin/env python3
"""dollup changelog tool -- the declaration-driven engine.

CHANGELOG.yaml is the source of truth for release notes. This renders it
and checks it, so that the release page, the release mirror and the copy
of CHANGELOG.md in the tree are all derived from one file rather than
maintained in parallel.

What is repository-specific is declared, not coded (doc/ALIGNMENT.md §3):
`.technoproj`'s TECHNO_CHANGELOG names the compatibility facts this
project records, the profile mappings, the tag rule, the version stamps
and whether changelog.json is emitted. Bespoke invariants live in
script/checks.py, which this calls when it exists.

The shared engine is Aloecraft-org/technoproj (`technoproj-changelog`,
argument-for-argument the same CLI, reading the same declaration and the
same YAML -- checked: it validates this tree and renders the same
changelog.json). This copy stays until technoproj carries three things
the release workflow needs: a dev tag (vX.Y.Z-dev.N) as a build of the
newest entry, a SemVer-spelled prerelease version (0.2.0-rc.1) in
`consistency`, and `buildinfo --tag`. Then `pip install` replaces it and
nothing else here changes.

Usage:
  script/changelog.py validate              schema and consistency checks
  script/changelog.py render md             whole changelog, as Markdown
  script/changelog.py render md --tag TAG   one release's section only
                                            (what a release body wants)
  script/changelog.py render json           machine-readable form
  script/changelog.py mirror-tags           tags the mirror should carry,
                                            newest first
  script/changelog.py latest                the tag `latest/` resolves to
  script/changelog.py generate              write CHANGELOG.md (and
                                            changelog.json when declared)
  script/changelog.py check                 fail unless the generated
                                            files match the YAML; for CI
  script/changelog.py consistency           fail unless the tree agrees
                                            with the newest entry; for CI
  script/changelog.py release-check --tag TAG [--publish]
                                            fail unless TAG is releasable;
                                            prints prerelease=, version=
                                            and dev= for GITHUB_OUTPUT
  script/changelog.py buildinfo --tag TAG   the entry's compatibility
                                            facts as `key: value` lines,
                                            for BUILDINFO.txt

Version spellings (doc/ALIGNMENT.md §1): the entry's `version` is the tag
body -- the tag without its `v` -- so `tag_rule: exact` is `tag == "v" +
version` for every entry, old and new alike. A new entry spells it as
SemVer (0.4.0, 0.4.0-rc.1, 0.4.0-dev.7); an entry from before the scheme
keeps the spelling its tag has (0.5.0rc9), because existing tags are never
respelled. PEP 440 (0.4.0rc1) is a derived spelling for `stamps` in a
project that publishes to PyPI, never the canonical one. A dev tag has no
entry of its own: it is a build of the newest entry's version from one
commit.

Why the generated files are committed: the release mirror runs on a host
with a stdlib-only Python and no build step, so it reads changelog.json
directly. `check` is what stops that copy going stale.

Requires PyYAML (pip install pyyaml).
"""
import argparse
import importlib.util
import json
import os
import re
import sys

try:
    import yaml
except ImportError:
    sys.exit("changelog.py: PyYAML is required (pip install pyyaml)")

# The tree being operated on: this file's parent's parent, or TECHNO_ROOT
# the way technoproj reads it, so the two are invoked alike.
ROOT = os.environ.get("TECHNO_ROOT") or os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
SOURCE = os.path.join(ROOT, "CHANGELOG.yaml")
MD = os.path.join(ROOT, "CHANGELOG.md")
JSON = os.path.join(ROOT, "changelog.json")
TECHNOPROJ = os.path.join(ROOT, ".technoproj")
CHECKS = os.path.join(ROOT, "script", "checks.py")

# keepachangelog's six, in the order it prints them, plus our two.
SECTIONS = [
    ("added", "Added"),
    ("changed", "Changed"),
    ("deprecated", "Deprecated"),
    ("removed", "Removed"),
    ("fixed", "Fixed"),
    ("security", "Security"),
    ("known_issues", "Known issues"),
]
STATUSES = {"released", "unreleased", "tagged"}
CORE = {"version", "tag", "date", "status", "stable", "latest", "mirror",
        "summary", "upgrading"}
TAG_RULES = {"exact", "prefix", "derive"}
SPELLINGS = {"base", "pep440", "semver"}


# depth: the declaration

def declaration():
    """TECHNO_CHANGELOG from .technoproj, with defaults, checked for shape:
    a declaration that is wrong is a tool that is wrong, and it should say
    so before it renders anything."""
    try:
        with open(TECHNOPROJ) as f:
            proj = json.load(f)
    except (OSError, ValueError) as e:
        sys.exit(".technoproj: %s" % e)
    d = proj.get("TECHNO_CHANGELOG")
    if not isinstance(d, dict):
        sys.exit(".technoproj: no TECHNO_CHANGELOG block")
    d.setdefault("project", proj.get("name", "this project"))
    d.setdefault("intro_extra", "")
    d.setdefault("facts", [])
    d.setdefault("mappings", [])
    d.setdefault("tag_rule", "exact")
    d.setdefault("required", ["version", "tag", "status", "stable", "mirror", "summary"])
    d.setdefault("latest_requires", ["stable", "mirror"])
    d.setdefault("emit_json", False)
    d.setdefault("stamps", [])
    for key in ("candidates", "planned"):
        if d.get(key):
            sys.exit(".technoproj: TECHNO_CHANGELOG.%s is not supported by this "
                     "engine copy" % key)
    if d["tag_rule"] not in TAG_RULES:
        sys.exit(".technoproj: tag_rule %r not one of %s"
                 % (d["tag_rule"], ", ".join(sorted(TAG_RULES))))
    for i, fact in enumerate(d["facts"]):
        if not isinstance(fact, dict) or not isinstance(fact.get("keys"), list) \
                or not fact["keys"] or not fact.get("fmt"):
            sys.exit(".technoproj: facts[%d] needs keys and fmt" % i)
        # technoproj's rule: a fact's slot is its id, or its first key.
        fact.setdefault("id", fact["keys"][0])
    for i, m in enumerate(d["mappings"]):
        if not isinstance(m, dict) or not m.get("key") or not m.get("title"):
            sys.exit(".technoproj: mappings[%d] needs key and title" % i)
    for i, s in enumerate(d["stamps"]):
        if not isinstance(s, dict) or not s.get("file") or not s.get("find") \
                or s.get("spelling", "base") not in SPELLINGS:
            sys.exit(".technoproj: stamps[%d] needs file, find and a spelling "
                     "in %s" % (i, ", ".join(sorted(SPELLINGS))))
    return d


DECL = declaration()
FACT_KEYS = []
for _fact in DECL["facts"]:
    for _k in _fact["keys"]:
        if _k not in FACT_KEYS:
            FACT_KEYS.append(_k)
MAPPING_KEYS = [m["key"] for m in DECL["mappings"]]
SCALARS = CORE | set(FACT_KEYS)
KNOWN = SCALARS | set(MAPPING_KEYS) | {k for k, _ in SECTIONS}


# depth: version spellings

# The canonical grammar (ALIGNMENT §1) and the one every tag before the
# scheme used. Both parse to the same triple so a changelog can hold both
# without rewriting a single entry.
SEMVER = re.compile(r"^(\d+\.\d+\.\d+)(?:-(dev|alpha|beta|rc)\.(\d+))?$")
LEGACY = re.compile(r"^(\d+\.\d+\.\d+)(?:(a|b|rc)(\d+)|\.dev(\d+))?$")
DEV_TAG = re.compile(r"^v(\d+\.\d+\.\d+)-dev\.(\d+)$")
KINDS = {"a": "alpha", "b": "beta", "rc": "rc"}
PEP440_KINDS = {"alpha": "a", "beta": "b", "rc": "rc"}


def parse_version(v):
    """A version -> (base, kind, n), kind one of None/dev/alpha/beta/rc.
    None when it is neither the canonical spelling nor the legacy one."""
    v = str(v)
    m = SEMVER.match(v)
    if m:
        base, kind, n = m.groups()
        return base, kind, (int(n) if n is not None else None)
    m = LEGACY.match(v)
    if m:
        base, kind, n, dev = m.groups()
        if dev is not None:
            return base, "dev", int(dev)
        if kind:
            return base, KINDS[kind], int(n)
        return base, None, None
    return None


def to_semver(v):
    parsed = parse_version(v)
    if not parsed:
        return None
    base, kind, n = parsed
    return base if kind is None else "%s-%s.%d" % (base, kind, n)


def to_pep440(v):
    parsed = parse_version(v)
    if not parsed:
        return None
    base, kind, n = parsed
    if kind is None:
        return base
    if kind == "dev":
        return "%s.dev%d" % (base, n)
    return "%s%s%d" % (base, PEP440_KINDS[kind], n)


def to_tag(v):
    """The tag is `v` plus the version as the entry spells it: canonical
    for a new entry, legacy for one that predates the scheme."""
    return "v" + str(v)


def spell(v, spelling):
    if spelling == "base":
        parsed = parse_version(v)
        return parsed[0] if parsed else None
    if spelling == "semver":
        return to_semver(v)
    if spelling == "pep440":
        return to_pep440(v)
    return None


# depth: loading and validation

class OneOfEachKey(yaml.SafeLoader):
    """`yaml.safe_load` keeps the last of two equal keys and says nothing.
    A duplicate key is a lie the file tells about itself; refuse it."""

    def construct_mapping(self, node, deep=False):
        seen = {}
        for key_node, _ in node.value:
            key = self.construct_object(key_node, deep=deep)
            if key in seen:
                raise yaml.constructor.ConstructorError(
                    "while constructing a mapping", node.start_mark,
                    "duplicate key %r (first at line %d)"
                    % (key, seen[key] + 1),
                    key_node.start_mark)
            seen[key] = key_node.start_mark.line
        return super().construct_mapping(node, deep)


def load():
    with open(SOURCE) as f:
        try:
            return yaml.load(f, Loader=OneOfEachKey)
        except yaml.YAMLError as e:
            sys.exit("CHANGELOG.yaml: %s" % e)


def tag_of(r):
    """The entry's tag under the declared rule; None when it has none and
    the rule does not derive one."""
    tag = r.get("tag")
    if tag:
        return tag
    if DECL["tag_rule"] == "derive":
        return to_tag(r.get("version", ""))
    return None


def validate(doc):
    """-> list of problems, empty when the file is sound."""
    bad = []
    if doc.get("schema") != 1:
        bad.append("schema must be 1")
    if not doc.get("repo"):
        bad.append("no repo")
    releases = doc.get("releases") or []
    if not releases:
        bad.append("no releases")

    seen_v, seen_t, latest = set(), set(), []
    for r in releases:
        v = r.get("version", "<unnamed>")
        where = "release %s" % v

        for key in r:
            if key not in KNOWN:
                bad.append("%s: unknown key %r" % (where, key))
        for key in DECL["required"]:
            if r.get(key) in (None, ""):
                bad.append("%s: missing %s" % (where, key))

        if v in seen_v:
            bad.append("%s: duplicate version" % where)
        seen_v.add(v)
        parsed = parse_version(v)
        if not parsed:
            bad.append("%s: version is not X.Y.Z, X.Y.Z-{dev,alpha,beta,rc}.N, "
                       "or a legacy X.Y.Z{a,b,rc}N" % where)
        elif parsed[1] == "dev":
            bad.append("%s: a dev build has no entry of its own -- it is a "
                       "build of the newest entry's version" % where)

        tag = r.get("tag")
        if tag:
            if tag in seen_t:
                bad.append("%s: duplicate tag %s" % (where, tag))
            seen_t.add(tag)
            if DECL["tag_rule"] == "exact" and tag != to_tag(v):
                bad.append("%s: tag %r should be %r" % (where, tag, to_tag(v)))
            elif DECL["tag_rule"] == "prefix" and not tag.startswith("v"):
                bad.append("%s: tag %r does not start with v" % (where, tag))
        elif DECL["tag_rule"] != "derive":
            bad.append("%s: missing tag" % where)

        status = r.get("status")
        if status not in STATUSES:
            bad.append("%s: status %r not one of %s"
                       % (where, status, ", ".join(sorted(STATUSES))))

        date = r.get("date")
        if status == "unreleased":
            if date:
                bad.append("%s: unreleased but carries a date" % where)
        elif not date:
            bad.append("%s: %s but has no date" % (where, status))
        elif not re.fullmatch(r"\d{4}-\d{2}-\d{2}", str(date)):
            bad.append("%s: date %r is not ISO yyyy-mm-dd" % (where, date))

        if r.get("mirror") and status != "released":
            bad.append("%s: mirror: true but status is %r -- the mirror can "
                       "only carry a published release" % (where, status))
        if parsed and parsed[1] is not None and r.get("stable"):
            bad.append("%s: a %s build is not stable" % (where, parsed[1]))

        if r.get("latest"):
            latest.append(r)

        for key in MAPPING_KEYS:
            block = r.get(key)
            if block is None:
                continue
            if not isinstance(block, dict):
                bad.append("%s: %s must be a mapping of profile -> list"
                           % (where, key))
                continue
            for prof, names in block.items():
                if not isinstance(names, list) or not all(
                        isinstance(n, str) for n in names):
                    bad.append("%s: %s.%s must be a list of strings"
                               % (where, key, prof))

        for key in sorted(SCALARS):
            val = r.get(key)
            if not isinstance(val, (list, tuple, dict, set)):
                continue
            bad.append("%s: %s must be a single value, not a %s -- a block "
                       "scalar is '%s: |', not '%s:' followed by '- |'"
                       % (where, key, type(val).__name__, key, key))

        for key, _ in SECTIONS:
            items = r.get(key)
            if items is None:
                continue
            if not isinstance(items, list):
                bad.append("%s: %s must be a list" % (where, key))
                continue
            for i, item in enumerate(items):
                if not isinstance(item, str) or not item.strip():
                    bad.append("%s: %s[%d] must be a non-empty string"
                               % (where, key, i))

    if len(latest) != 1:
        bad.append("exactly one release must carry 'latest: true' (found %d)"
                   % len(latest))
    else:
        r = latest[0]
        for key in DECL["latest_requires"]:
            if not r.get(key):
                bad.append("release %s is latest but %s is not true"
                           % (r.get("version"), key))
        if r.get("status") != "released":
            bad.append("release %s is latest but is not released"
                       % r.get("version"))
    return bad


# depth: rendering

def heading(r, dev=None):
    date = r.get("date") or "unreleased"
    if dev:
        base, n = dev
        text = "## [%s-dev.%d] - dev build %d of %s" % (base, n, n, base)
        return text + " (prerelease; the commit is in BUILDINFO.txt)"
    text = "## [%s] - %s" % (r["version"], date)
    marks = []
    if not r.get("stable"):
        marks.append("prerelease")
    if r.get("status") == "tagged":
        marks.append("tagged, not published")
    if marks:
        text += " (%s)" % ", ".join(marks)
    return text


def bullets(items):
    """A bullet may be multiline; its first line is the headline, and the
    rest is indented under it so Markdown keeps it inside the item."""
    out = []
    for item in items:
        lines = item.rstrip("\n").split("\n")
        out.append("- " + lines[0])
        for line in lines[1:]:
            out.append(("  " + line).rstrip())
    return out


def facts_of(r):
    """The declared facts this entry states, rendered in declaration
    order. A fact renders when every key it names is present; the first
    variant matching a given id wins."""
    out, done = [], set()
    for fact in DECL["facts"]:
        if fact["id"] in done:
            continue
        if all(r.get(k) is not None for k in fact["keys"]):
            out.append(fact["fmt"].format(**{k: r[k] for k in fact["keys"]}))
            done.add(fact["id"])
    return out


def render_release(r, dev=None):
    out = [heading(r, dev), ""]
    meta = []
    tag = tag_of(r)
    if tag and not dev:
        meta.append("`%s`" % tag)
    meta += facts_of(r)
    if meta:
        out += [" &middot; ".join(meta), ""]
    if r.get("summary"):
        out += [r["summary"].rstrip("\n"), ""]
    for m in DECL["mappings"]:
        key = m["key"]
        if not r.get(key):
            continue
        out += ["### " + m["title"], ""]
        for prof in sorted(r[key]):
            out.append("- `%s`: %s" % (prof, ", ".join(
                "`%s`" % n for n in r[key][prof]) or "_none_"))
        out.append("")
    for key, title in SECTIONS:
        if r.get(key):
            out += ["### " + title, ""] + bullets(r[key]) + [""]
    if r.get("upgrading"):
        out += ["### Upgrading", "", r["upgrading"].rstrip("\n"), ""]
    return "\n".join(out).rstrip("\n") + "\n"


def entry_for(doc, tag):
    """The entry a tag names, and the dev coordinates when the tag is a
    dev build: (entry, (base, n) or None). A dev tag names the newest
    entry, which must be of the same base version."""
    m = DEV_TAG.match(tag)
    if m:
        base, n = m.group(1), int(m.group(2))
        newest = doc["releases"][0]
        parsed = parse_version(newest.get("version", ""))
        if not parsed or parsed[0] != base:
            sys.exit("changelog.py: %s is a dev build of %s, but the newest "
                     "entry is %s" % (tag, base, newest.get("version")))
        return newest, (base, n)
    for r in doc["releases"]:
        if tag_of(r) == tag:
            return r, None
    sys.exit("changelog.py: no release with tag %r" % tag)


def render_md(doc, tag=None):
    if tag:
        r, dev = entry_for(doc, tag)
        return render_release(r, dev)
    head = (
        "# Changelog\n\n"
        "All notable changes to %s are recorded here.\n\n"
        "Generated from `CHANGELOG.yaml`, which is the source of truth --\n"
        "edit that file, then run `script/changelog.py generate`.\n\n"
        "The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).\n"
        % DECL["project"]
    ) + DECL["intro_extra"]
    return head + "\n" + "\n\n".join(render_release(r) for r in doc["releases"])


def render_json(doc):
    """What the mirror consumes. Rendered Markdown travels with each entry
    so the mirror needs no renderer of its own."""
    out = {
        "schema": doc["schema"],
        "repo": doc["repo"],
        "latest": next((tag_of(r) for r in doc["releases"] if r.get("latest")),
                       None),
        "mirror_tags": [tag_of(r) for r in doc["releases"] if r.get("mirror")],
        "releases": [],
    }
    keys = ["version", "tag", "date", "status", "stable", "mirror"] + \
        FACT_KEYS + MAPPING_KEYS + ["summary", "upgrading"]
    for r in doc["releases"]:
        entry = {k: r.get(k) for k in keys}
        entry["tag"] = tag_of(r)
        # PyYAML gives an unquoted yyyy-mm-dd back as a datetime.date; the
        # mirror wants a plain ISO string.
        entry["date"] = str(r["date"]) if r.get("date") else None
        entry["latest"] = bool(r.get("latest"))
        entry["sections"] = {k: r[k] for k, _ in SECTIONS if r.get(k)}
        entry["notes_md"] = render_release(r)
        out["releases"].append(entry)
    return json.dumps(out, indent=2) + "\n"


# depth: the tree agrees with the newest entry

def read(path):
    with open(os.path.join(ROOT, path)) as f:
        return f.read()


def consistency(doc):
    """The newest entry describes the tree as it stands, so the tree has to
    agree with it: every declared stamp holds the entry's version in the
    declared spelling, and script/checks.py's own invariants hold. An
    unreleased entry is exempt from the stamps -- the next version is
    opened as `unreleased` when its first change lands, and the manifests
    move when it ships -- but never from the checks, which are about facts
    the tree already carries. -> list of problems."""
    bad = []
    r = doc["releases"][0]
    version = r["version"]
    where = "newest entry (%s)" % version
    unreleased = r.get("status") == "unreleased"

    for stamp in () if unreleased else DECL["stamps"]:
        want = spell(version, stamp.get("spelling", "base"))
        try:
            text = read(stamp["file"])
        except OSError as e:
            bad.append("%s: cannot read (%s)" % (stamp["file"], e))
            continue
        found = set(re.findall(stamp["find"], text, re.M))
        if not found:
            bad.append("%s: nothing matches %r" % (stamp["file"], stamp["find"]))
        wrong = sorted(v for v in found if v != want)
        if wrong:
            bad.append("%s carries version %s but %s says %r"
                       % (stamp["file"], ", ".join(repr(w) for w in wrong),
                          where, want))

    if os.path.isfile(CHECKS):
        spec = importlib.util.spec_from_file_location("checks", CHECKS)
        checks = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(checks)
        ctx = {"read": read, "root": ROOT,
               "base": parse_version(version)[0] if parse_version(version) else None}
        bad += list(checks.consistency(doc, ctx) or [])
    return bad


# depth: the release gate

def release_check(doc, tag, publishing):
    """Gate a release on its changelog entry. -> (problems, outputs). A dev
    tag needs no entry of its own and is always a prerelease."""
    if DEV_TAG.match(tag):
        r, (base, n) = entry_for(doc, tag)
        return [], {"prerelease": "true", "version": "%s-dev.%d" % (base, n),
                    "dev": "true"}
    entry = next((r for r in doc["releases"] if tag_of(r) == tag), None)
    if entry is None:
        return (["no entry in CHANGELOG.yaml for tag %r -- add one before "
                 "releasing it" % tag], {})
    bad = []
    if publishing:
        if entry["status"] != "released":
            bad.append(
                "%s is still status: %s. Before publishing, edit "
                "CHANGELOG.yaml: set status: released and a date, move "
                "latest: true onto it, set mirror: true, then re-run "
                "script/changelog.py generate and commit."
                % (tag, entry["status"]))
        if not entry.get("date"):
            bad.append("%s has no date" % tag)
    return bad, {"prerelease": "false" if entry.get("stable") else "true",
                 "version": str(entry["version"]), "dev": "false"}


def buildinfo(doc, tag):
    """The entry's compatibility facts as `key: value` lines: what
    BUILDINFO.txt carries beside tag, version, commit, branch and built.
    The same facts the changelog states, so the two cannot disagree."""
    r, _ = entry_for(doc, tag)
    return "".join("%s: %s\n" % (k, r[k]) for k in FACT_KEYS if r.get(k) is not None)


def main():
    ap = argparse.ArgumentParser(add_help=False)
    ap.add_argument("command", choices=["validate", "render", "mirror-tags",
                                        "latest", "generate", "check",
                                        "consistency", "release-check",
                                        "buildinfo"])
    ap.add_argument("format", nargs="?", choices=["md", "json"])
    ap.add_argument("--tag")
    ap.add_argument("--publish", action="store_true")
    ap.add_argument("-h", "--help", action="store_true")
    args = ap.parse_args()
    if args.help:
        print(__doc__)
        return 0

    doc = load()
    problems = validate(doc)
    if problems:
        for p in problems:
            print("CHANGELOG.yaml: " + p, file=sys.stderr)
        return 1

    if args.command == "validate":
        print("OK: %d releases, latest=%s, %d mirrored"
              % (len(doc["releases"]),
                 next(tag_of(r) for r in doc["releases"] if r.get("latest")),
                 sum(1 for r in doc["releases"] if r.get("mirror"))))
    elif args.command == "render":
        if args.format == "json":
            sys.stdout.write(render_json(doc))
        else:
            sys.stdout.write(render_md(doc, args.tag))
    elif args.command == "mirror-tags":
        for r in doc["releases"]:
            if r.get("mirror"):
                print(tag_of(r))
    elif args.command == "latest":
        print(next(tag_of(r) for r in doc["releases"] if r.get("latest")))
    elif args.command == "consistency":
        problems = consistency(doc)
        if problems:
            for p in problems:
                print("inconsistent: " + p, file=sys.stderr)
            return 1
        print("OK: the tree agrees with %s" % doc["releases"][0]["version"])
    elif args.command == "release-check":
        if not args.tag:
            sys.exit("changelog.py: release-check needs --tag")
        problems, out = release_check(doc, args.tag, args.publish)
        if problems:
            for p in problems:
                print("release-check: " + p, file=sys.stderr)
            return 1
        for k, v in out.items():
            print("%s=%s" % (k, v))
    elif args.command == "buildinfo":
        if not args.tag:
            sys.exit("changelog.py: buildinfo needs --tag")
        sys.stdout.write(buildinfo(doc, args.tag))
    elif args.command in ("generate", "check"):
        want = {MD: render_md(doc)}
        if DECL["emit_json"]:
            want[JSON] = render_json(doc)
        stale = []
        for path, text in want.items():
            name = os.path.relpath(path, ROOT)
            if args.command == "generate":
                with open(path, "w") as f:
                    f.write(text)
                print("wrote %s" % name)
            else:
                try:
                    with open(path) as f:
                        current = f.read()
                except OSError:
                    current = None
                if current != text:
                    stale.append(name)
        if stale:
            print("stale, re-run 'script/changelog.py generate': %s"
                  % ", ".join(sorted(stale)), file=sys.stderr)
            return 1
        if args.command == "check":
            print("OK: %s match CHANGELOG.yaml"
                  % " and ".join(os.path.relpath(p, ROOT) for p in want))
    return 0


if __name__ == "__main__":
    sys.exit(main())
