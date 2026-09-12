# site/

`dollup.aloecraft.org`, built by the site contract every Aloecraft repo
implements (the reference copy is `doc/CONTRACT.md` in
[aloecraft-software-portal](https://github.com/Aloecraft-org/aloecraft-software-portal)):

```sh
./site/build.sh                 # -> site/_out
./site/build.sh --out /tmp/x
./site/build.sh --check         # verify the template, build nothing
```

Offline, hermetic, idempotent, and it never deploys. The deployment tooling
clones this repo, runs `./site/build.sh`, and ships `site/_out` to the
vhost; this directory is what it finds.

```
build.sh            the contract entry point
render.py           template -> _out
check.py            guards the template's load-bearing details; runs in build.sh and CI
site.json           what this site is, for the portal and the manifest
template/index.html the page: one file, no build step, no outbound requests
std-repo.pub        the public half of the standard repo's signing key -- see below
```

## Signed or not: one template, two pages

The page has one conditional, `<!--IF:KEY-->` … `<!--ELSE:KEY-->` …
`<!--END:KEY-->`, and `render.py` takes the first branch only when
`site/std-repo.pub` is committed. Then the key is stamped where
`__DOLLUP_STD_PUBKEY__` appears, the four-command start renders, and the
live package table fills from `/std-repo/index.json` at load. With the key
absent the page says the standard repo is not published yet.

**The standard repo's tree is not in this repository.** It lives in
[drt-std-lib](https://github.com/Aloecraft-org/drt-std-lib), which
implements the same site contract and is staged by the deployment tooling
as a sibling subtree at `/std-repo/` under this vhost, along with the nginx
cache policy for that path. The key file committed here must match the one
that signs that tree: `dollup init` pins it (`crates/dollup/src/root.rs`),
the page shows it, and the publisher derives it from the private key.

## The page

One file, no build step, no outbound requests: system fonts, plain CSS on
custom properties, and a little JavaScript that reads
`/std-repo/index.json` — the same index dollup reads — so the published
package list cannot drift from what is actually published.

**Dark is the default**, as on diluvium.aloecraft.org, with a toggle in the
hero that switches to light and remembers the choice in `localStorage` under
`dollup-theme`. The inline script that applies a stored choice sits in
`<head>` deliberately: it has to run before first paint, or a light-mode
visitor sees a flash of dark on every load. `prefers-color-scheme` is
intentionally *not* consulted — the ask was a dark default, not a
system-following one.

## Invariants, and getting a design pass done elsewhere

The page carries details a designer has no reason to know about: the
substitution token, the conditional markers, the `id="pkgs"` element the
live table fills, the fetch that fills it, the pre-paint theme script.
Losing one does not look broken — the page renders fine and quietly stops
working — so they are checked mechanically:

```sh
python3 site/check.py                  # everything: template, site.json, std-repo tree
python3 site/check.py candidate.html   # a returned file, before adopting it
```

To hand the page to another tool for an aesthetic pass, give it
[`DESIGN-BRIEF.md`](DESIGN-BRIEF.md) followed by `template/index.html`. When
the file comes back:

```sh
python3 site/check.py candidate.html && cp candidate.html site/template/index.html
git diff site/template/index.html
```
