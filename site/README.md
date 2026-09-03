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
render.py           template + committed std-repo -> _out
check.py            guards the template's load-bearing details; runs in build.sh and CI
site.json           what this site is, for the portal and the manifest
template/index.html the page: one file, no build step, no outbound requests
nginx/std-repo.conf cache policy for /std-repo/, installed as a vhost drop-in
std-repo.pub        the public signing key, once minted -- see below
```

## Signed or not: one template, two pages

The page has one conditional, `<!--IF:KEY-->` … `<!--ELSE:KEY-->` …
`<!--END:KEY-->`, and `render.py` takes the first branch only when **both**
`site/std-repo.pub` and `std-repo/index.json.sig` are committed. Then the
key is stamped where `__DOLLUP_STD_PUBKEY__` appears, the four-command
start and the live package table render, and `std-repo/` ships beside the
page with its blob projection. With either file absent the page says the
standard repo is not published yet and `std-repo/` does not ship: an
unsigned repo at the canonical URL is an invitation to pin it unsigned.

The private key is never part of a build. `std-repo/publish.sh` signs in
place and writes `site/std-repo.pub`; you commit the three files; the build
copies them. That is what keeps the build hermetic.

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
