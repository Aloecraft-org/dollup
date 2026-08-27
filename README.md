# dollup

Distribution for [Diluvium](https://github.com/Aloecraft-org/diluvium) /
[DRT](https://github.com/Aloecraft-org/diluvium-drt) artifacts: a **fetcher
and resolver over a content-addressed store**, plus transport for
hibernated-instance snapshots.

It is deliberately not a package manager. Install is inert; config is
authority; the manifest is declarative, never executable; DRT never fetches
— dollup populates the directory DRT reads, and the trust boundary is the
directory, not the tool. [`SPEC.md`](SPEC.md) is the founding spec and the
map; [`THREAT-NOTES.md`](THREAT-NOTES.md) says what is and is not checked.

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
| [`crates/dollup`](crates/dollup) | The binary: deployment, store, fetch (four schemes), and the verbs. |

## A five-minute life

```sh
# Publisher side: a repo is a directory of packages.
dollup repo index ./my-repo          # scan, validate, write index.json
dollup repo keygen --out repo.key    # repo.key (private, 0600) + repo.key.pub
dollup repo sign ./my-repo --key-file repo.key
dollup repo blobs ./my-repo          # optional: projection for a static mirror

# Consumer side: a deployment is a directory.
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
