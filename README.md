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

## Workspace

| crate | what |
|---|---|
| [`crates/dollup-format`](crates/dollup-format) | The formats: manifest (three faces), repo index, lockfile, sources/refs, identity hashing, index signing. Pure types + bytes; no IO, no network. |
| [`crates/dollup`](crates/dollup) | The binary: the app directory, store, fetch (four schemes), and the verbs. |

## A five-minute life

`dollup` is not on `PATH` in a fresh checkout. Either prefix the commands
below with `./target/release/`, or put it on `PATH` once:

```sh
cargo build --release && export PATH="$PWD/target/release:$PATH"
# or, permanently:  cargo install --path crates/dollup
```

```sh
# Publisher side: a repo is a directory of packages.
dollup repo index ./my-repo          # scan, validate, write index.json
dollup repo keygen --out repo.key    # repo.key (private, 0600) + repo.key.pub
dollup repo sign ./my-repo --key-file repo.key
dollup repo blobs ./my-repo          # optional: projection for a static mirror

# Consumer side: an app is a directory.
dollup init
$EDITOR dollup.json                  # add sources; pin the publisher's public key
dollup add telemetry@^1              # fetch, hash-check, lock, populate code/
dollup verify                        # re-hash everything against the lock

# Snapshots: migrate a sleeping agent (acceptance demo 2's transport half).
dollup push file:///mnt/xfer night-clerk.dvsnap --package agent   # machine A
dollup pull file:///mnt/xfer night-clerk                          # machine B
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

## Building

```
cargo build && cargo test
```

No C toolchain, no diluvium checkout: dollup ships as a static single
binary that needs nothing else installed.

## Not yet built (tracked, not forgotten)

`update` and `lock` verbs; dependency version unification beyond
first-wins; the `https` scheme's blob-wise fetching (it currently reads
tree paths); writable non-file remotes for snapshot push (the `--export-state`
gate is already in front of them); consuming `drt-config` types once DRT
reads manifests (SPEC.md §2's intent).

## License

Apache-2.0, same as diluvium and DRT.
