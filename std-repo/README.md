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
./std-repo/publish.sh --key-file ~/.dollup/std-repo.key
```

`dollup repo publish` seals every package, indexes, signs, projects blobs,
and **resolves the result in a throwaway app** before calling it
publishable — all in place. The script then writes `site/std-repo.pub`,
derived from the private key so the key the page shows is the key that
signed, verifies the pair, and builds the site into `.publish/` as a
preview. Then you commit:

```
std-repo/index.json  std-repo/index.json.sig  site/std-repo.pub
```

**Nothing is deployed from here.** The site contract (`site/build.sh`)
forbids a repo from pushing to a host; the deployment tooling stages the
committed tree by the same build. The private key is the one thing
`publish.sh` needs that the build does not, and keeping it out of the
build is what makes the build hermetic.

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

`index.json` **and `index.json.sig`** are committed, along with
`site/std-repo.pub`. A git or zipball source reads the tree directly, so the
signature has to be in it — that is the whole point of the signature living
in the tree rather than in a transport — and the site build is hermetic, so
it can only ship what is committed. `dollup repo verify std-repo --key-file
site/std-repo.pub` is what CI runs to keep the three in step.

`blobs/` is **not** committed: it is a projection of the tree that
`render.py` (and `dollup repo blobs`) regenerate, content-addressed, so the
same tree always projects to the same files. Committing it would store
every byte twice.

Until the real key is minted none of the three exist, the site build takes
its unsigned branch, and `site/site.json` lists the package repo as
`planned`. That is deliberate: status means today.
