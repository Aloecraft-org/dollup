# dollup

Install [Diluvium](https://github.com/Aloecraft-org/diluvium) programs and
the capabilities they run on, plus move hibernated instances between
machines.

dollup fetches programs into an **app** — one directory holding the code,
the sources it came from, and a lockfile pinning every version and hash, so
the same app rebuilds byte for byte anywhere. (A drt app is a config plus a
program; dollup brings the program half and never writes the config.) Packages are
named by the hash of their contents and listed in an index the publisher
signs, which is what lets a mirror, a git remote and an offline copy be the
same artifact rather than three you have to trust separately.

Three properties shape everything else, and they are worth knowing early:
**nothing executes during an install** (a manifest is data, so there is no
setup script to audit); **installing never grants** — what a program may do
lives in your config, not in the package; and **[DRT](https://github.com/Aloecraft-org/diluvium-drt)
never fetches**, so the trust boundary is the directory dollup writes rather
than dollup itself.

[`SPEC.md`](SPEC.md) is the founding spec and the map;
[`THREAT-NOTES.md`](THREAT-NOTES.md) says what is and is not checked.

| doc | what |
|---|---|
| [`SPEC.md`](SPEC.md) | The founding spec |
| [`doc/RepoFormat.md`](doc/RepoFormat.md) | The repo format: one directory shape, four transports (`https`, `zip+https`, `git+https`, `file`), three-faced packages, index signing |
| [`doc/CodeResolution.md`](doc/CodeResolution.md) | The ask to DRT: the code root, `Program` growth, admission checks — staged so nothing blocks DRT's milestone |
| [`THREAT-NOTES.md`](THREAT-NOTES.md) | What dollup checks and deliberately does not |

## Installing

```sh
curl -fsSL https://github.com/Aloecraft-org/dollup/releases/latest/download/install.sh | sh
```

Or take the binary straight from
[Releases](https://github.com/Aloecraft-org/dollup/releases) — every one
carries `SHA256SUMS.txt` and a `BUILDINFO.txt` naming the commit it was built
from. `DOLLUP_VERSION=vX.Y.Z` pins a release, `DOLLUP_PREFIX=` chooses the
directory, and `DOLLUP_SOURCE=file:///mnt/xfer` installs with no network at
all.

From a checkout, when you want the tip rather than a release:

```sh
cargo build --release && export PATH="$PWD/target/release:$PATH"
```

## Workspace

| crate | what |
|---|---|
| [`crates/dollup-format`](crates/dollup-format) | The formats: manifest (three faces), repo index, lockfile, sources/refs, identity hashing, index signing. Pure types + bytes; no IO, no network. |
| [`crates/dollup`](crates/dollup) | The binary: the app directory, store, fetch (four schemes), and the verbs. |

## A five-minute life

The commands below assume `dollup` is on your `PATH` (see *Installing*). It
is not, in a fresh checkout — prefix them with `./target/release/` instead,
which is also what every hint dollup prints will say back to you.

```sh
# Get the runtime. One file, hash-checked, dropped where you are --
# it installs nothing and needs no config.
dollup get drt                          # ./drt, from the latest release
dollup get drt --version v0.3.0 --slim  # a pin, and the size profile
dollup get drt --from file:///mnt/xfer  # air-gapped: a directory, no network
```

The standard repo lives in its own repository,
[drt-std-lib](https://github.com/Aloecraft-org/drt-std-lib), and `init`
pins it, so the consumer side below runs against something real before you
publish anything of your own:

```sh
dollup init
dollup pull hello
dollup get drt && ./drt run .drt_root/init/hello.dlua      # a module lands at its name's path
```

```sh
# Publisher side: a repo is a directory of packages, each stating its license.
dollup repo keygen --out repo.key    # once, ever
dollup repo publish ./my-repo --key-file repo.key --stage .publish
rsync -avz --delete .publish/ user@host:/var/www/my-repo/

# `publish` is seal + index + sign + blobs, and then the step people skip:
# it RESOLVES the tree it just produced — a throwaway deployment, the tree as
# a file:// source, every package added, verified against the lock — before
# calling it publishable. The four steps are also separately available
# (`repo seal|index|sign|blobs`) when you want them one at a time.

# Consumer side: a root is a directory.
dollup new my_app && cd my_app       # .drt_root/, the standard source key-pinned, a hello
dollup pull telemetry@^1             # fetch through ~/.dollup/cache, hash-check, lock, populate .drt_root/init/
dollup pull starter                  # a starting point is copied, never locked: the files are yours
dollup verify                        # re-hash everything against the lock and the cache
dollup gc                            # sweep the cache against every root on this box

# Snapshots: migrate a sleeping agent (acceptance demo 2's transport half).
dollup snapshot push file:///mnt/xfer night-clerk.dvsnap --package agent   # machine A
dollup snapshot pull file:///mnt/xfer night-clerk                          # machine B
# → snapshots/night-clerk.dvsnap, plus the pinned code-set resolved from
#   the sources by identity; restore is DRT's verb, against that directory.
```

Snapshots are **private by default**: pushing to any non-file remote takes
`--export-state`, acknowledged out loud, because a snapshot blob is the
instance's entire heap.

Host faces (connector implementations a package carries) are **not**
materialized by default: `--with-host` admits wasm targets,
`--with-host-native` additionally admits native ones — see
[`THREAT-NOTES.md`](THREAT-NOTES.md) for why the second flag is loud.

## Starting a root

```sh
mkdir my_drt_project && cd my_drt_project && dollup init my_drt_project
ls -a .           # .drt_root/  dlua/
ls .drt_root      # init/ live/ log/ profile/ state/ project.json consent.json dollup.lock
ls .drt_root/profile   # debug.config.json  preflight.config.json
```

`project.json` is the descriptor — the ceiling (`caps`), the sources, the
declared profiles — and it is declared, never computed, so editing a profile
never invalidates it. `init/` is the deployable tree: a pulled package's
modules land at the paths their names resolve to (`util.enc` at
`util/enc.dlua`), by the same rule drt's loader walks with, so
`require("util.enc")` finds them once deployed; the rest of what a package
ships sits under `init/<name>/`, and the manifest is not materialized. `consent.json` is init accepting the ceiling it just
declared, which is why a locally authored root never prompts on its first
start and does see the delta the first time its ceiling widens. `init/` is
delivered content (what `add` populates), `dlua/` is what you edit, `live/`
is what runs, and `state/` is the runtime's own. Init creates what is
missing and never rewrites what exists: `dollup init my_drt_project release`
on an existing root adds a `release` profile and touches nothing else.

## Config

A root's config is `<root>/.drt_root/project.json`, found in the directory
named (`--root`, default the current one) and nowhere else — discovery does
not walk up, so a subdirectory of a root is not in that root. A
`dollup.json` app is still read, until every verb has moved, and `-c` /
`DOLLUP_CONFIG` name one explicitly:

```sh
dollup pull telemetry                       # <root>/.drt_root/project.json, else <root>/dollup.json
dollup -c ./somewhere.json pull telemetry   # an explicit dollup.json
DOLLUP_CONFIG=./somewhere.json dollup pull telemetry
```

**Nothing about config is read from your home directory, nothing is looked
up in XDG, and nothing is written on a first run.** `~/.dollup/` exists —
keys, the list of roots on this box, and `cache/`, the content-addressed
store every root shares: `pull` fills it and materializes from it, `verify`
checks against it, `gc` sweeps it against every recorded root's lock — and
config resolution reads nothing from it; no root ever depends on it (a root
is self-contained without its cache, and `pull` refills one). `dollup get`
needs no config at all — it takes a URL or a default it prints every time.

Writes go back to the file the config was read from: `source add` on a root
edits `project.json`'s `sources` and nothing else in it; `dollup -c x.json
source add …` edits `x.json` and leaves no `dollup.json` behind.

Sources are tried in order, and the list is a fallback list: one that
cannot be read — not there, not answering, or answering with no index — is
passed over for the next and the skip is printed. One that answers and
refuses, by signature or by policy, stops the pull; a refusal is never a
skip.

## Auditing a root

`dollup audit` reports what `drt start` would do in the root at the current
directory (or `--root <path>`, never a parent of it): which profile, and by
which rule; the pin, and whether `.drt_root/drt` is that release — by hash
against the release's own sums, never by executing it; the ceiling and where
consent stands, with the delta when the ceiling moved; the entry and whether
it exists; the merged args; and every finding, blocking or not, rather than
the first. It runs the same resolution `drt start` runs, so it says what
will happen rather than what should. Safe on a root you just cloned and do
not trust: nothing executes, nothing is delegated, nothing is written — the
cache included.

```sh
dollup audit            # the root here, as `drt start` would see it
dollup audit release    # as `drt start release` would
```

## The runtime in a root

drt is never installed, only deployed: a root is self-contained, so the
binary lives at `.drt_root/drt`, copied from `~/.dollup/cache/drt/<version>/`
and never linked to it. `pull drt` fills the cache and touches no root —
`latest` is resolved to the version the mirror names, never cached as a
moving target — `deploy drt` copies the cache into the root, and `pin drt`
deploys and records the version in `project.json`, so the pin and the
binary agree; start refuses a mismatch by name. The pin is the release tag
without its leading `v`: `0.4.1` for `v0.4.1`, and `0.5.0rc9` for the
candidate `v0.5.0rc9`, which is its own release — the crate inside every
candidate says `0.5.0`, so the pin is the tag, which drt stamps into the
binary when it cuts one and compares the pin against. A root pinned to
`0.5.0` refuses an rc9 binary, and `audit` says so in start's words, naming
both versions when the cache can tell what the binary is.

```sh
dollup pull drt                 # the cache, at the version latest names
dollup pin drt                  # deploy it here and record it
dollup pin drt v0.4.1 --all     # every root on this box
dollup pull drt --from file:///mnt/xfer   # air-gapped: a directory laid out like the mirror
dollup pull drt 0.6.0rc1 --from https://github.com/Aloecraft-org/diluvium-drt/releases/download/v0.6.0rc1
```

The last line is how a candidate the mirror does not carry arrives: GitHub's
download directory for a tag has the mirror's layout, so `--from` takes it
as it takes any other, sums and all.

## Duplicating a root

`dollup duplicate <path>` is `cp -r` done right: the copy is a new root
with a fresh `root_id`, minted, and `duplicated_from` recording this one —
two roots claiming one id is what breaks shipping. Everything comes along
except what is the runtime's (`state/`, `live/`, `log/`) and what never
travels (`consent.json`); the copy gets the consent this root effectively
has, a fresh listed entry over the same ceiling when this root's acceptance
covers it, none when it does not (blanket here becomes listed there: the
opt-out was for this root). Signers come along either way — they are who
may approve, not an approval — so a copy on this box answers to the same
keys. A destination inside this root is refused, however it is spelled.

## Roots on this box

`dollup roots` lists every root dollup has seen on this machine — roots on
disk, not deployments running. Any verb that opens a root records it in
`~/.dollup/roots.json`, so a root copied with `cp -r` is on the list the
first time anyone touches it, and it is named there as sharing a `root_id`
with the original (which is what `dollup duplicate` exists to avoid). A
root removed with `rm -rf` stays on the list, reported as stale, never
silently dropped. `audit` reads the list and is the one verb that never
joins it: it writes nothing.

## Consenting to a ceiling

`dollup consent` reviews and accepts the root's declared ceiling without
starting anything — the same check `drt start` runs, with the same
answers. A first acceptance takes a yes on the terminal or `-y`; a ceiling
that narrowed since it was accepted is updated silently; one that widened
prints what changed and takes a yes or `--accept-changes`. `-y` does not
accept a widening, on purpose: otherwise every unit file and CI job would
carry permanent pre-consent to whatever the ceiling becomes. No terminal
and no applicable flag is a refusal that names the flag, never a hang.
`dollup consent --all` writes the blanket operator entry — everything,
forever, nothing prompts again — which is the explicit opt-out, and audit
reports it as such.

The deploy order on a box, once a ceiling has widened: push, then
`dollup consent` on the operator's terminal, then restart. Restart is where
consent fires, with no terminal, so the other order is an outage by design.

## Building

```
cargo build && cargo test
```

No C toolchain, no diluvium checkout: dollup ships as a static single
binary that needs nothing else installed.

## Not yet built (tracked, not forgotten)

`update` and `lock` verbs; `get` for anything but `drt`;
deploying (`repo publish` stages a tree and prints it; rsync is yours); dependency version unification beyond
first-wins; the `https` scheme's blob-wise fetching (it currently reads
tree paths); writable non-file remotes for snapshot push (the `--export-state`
gate is already in front of them); consuming `drt-config` types once DRT
reads manifests (SPEC.md §2's intent).

## License

Apache-2.0, same as diluvium and DRT.
