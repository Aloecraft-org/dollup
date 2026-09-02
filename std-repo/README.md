# std-repo — the standard dollup repo

What gets served at `https://dollup.aloecraft.org/std-repo/`. It is a
directory: an index, a signature, the package tree, and a content-addressed
blob projection. No service, no database, nothing dynamic — see
[`../doc/RepoFormat.md`](../doc/RepoFormat.md) for the format, and
[`deploy/nginx.conf`](deploy/nginx.conf) for the whole server side.

## The signing key

The repo is signed; clients pin the public half in their source list. The
private key **is not in this repository and must never be** — a key in a git
repo is not a key.

```sh
cargo build --release
./target/release/dollup repo keygen --out ~/.dollup/std-repo.key
```

(0600, creates the directory, and refuses to overwrite an existing key.
`dollup` is not on `PATH` in a fresh checkout — either call it by path as
above, or `export PATH="$PWD/target/release:$PATH"` once per shell, or
`cargo install --path crates/dollup` to put it in `~/.cargo/bin`.)

That writes `~/.dollup/std-repo.key` (private) and `…key.pub` (public), and
prints only the public line. Back the private half up somewhere that is not
a repository and not a terminal transcript; rotation means publishing a new
public key and having every consumer edit their source list, so this is a
key worth handling carefully once rather than casually often.

The public half belongs in three places, all of which `publish.sh` keeps in
step: this repo's signature, the landing page, and `dollup init`'s scaffold
once it is minted.

## Publishing

```sh
cargo build --release
./std-repo/publish.sh --key-file ~/.dollup/std-repo.key            # build + self-check
./std-repo/publish.sh --key-file ~/.dollup/std-repo.key --deploy   # and rsync
```

`publish.sh` finds the binary itself (`target/release/dollup`, or
`DOLLUP_BIN`), so it needs no `PATH` setup.

The script seals every package, indexes, signs, projects blobs, assembles
the site into `.publish/`, stamps the real public key into the page, and
then **resolves the staged repo in a throwaway deployment** before it will
deploy anything. A repo that cannot be added is not published.

## Adding a package

```
packages/<name>/<version>/
  manifest.json
  guest/…          # .dlua modules
  assets/…         # optional: images, json, fonts
  host/…           # optional: connector implementations, per target
```

Write the manifest without a `files` map, then:

```sh
./target/release/dollup repo seal packages/<name>/<version>
```

Seal walks the directory, hashes everything, writes the `files` map, and
validates the result — publishing never depends on a hand-computed hash.
Verification does not trust the seal: `index` re-hashes independently, so a
stale seal is caught rather than believed.

Versions are directories, so several may coexist and a consumer's
requirement picks one.

## What is here

| package | what |
|---|---|
| `starter` | A template: a program and the config that grants it, to copy and edit. `dollup new starter` |

## Consuming it

```sh
dollup init
dollup source add https://dollup.aloecraft.org/std-repo/ --key ed25519:…
dollup add hello
```

The key is the trust anchor, and it is an ordinary line in a file you own:
delete it and nothing resurrects it, replace the URL and you are self-hosted.

## What is committed, and why

`index.json` **is** committed: a git or zipball source reads the tree
directly, and without an index in it there is no repo to read. It is
deterministic — hashes and paths only — so it diffs cleanly.

`blobs/` is **not**: it is a projection of the tree that `publish.sh`
regenerates (and rebuilds from empty, so a blob from a deleted package is
never left being served under a name nothing indexes). Committing it would
store every byte twice.

`index.json.sig` is **not committed yet**, because it would have to be signed
with a key that does not exist. Once the real key is minted, commit the
signature alongside the index so that the git and zipball sources verify the
same way the mirror does — that is the whole point of the signature living in
the tree rather than in a transport.
