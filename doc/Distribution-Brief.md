# Distribution: the mirror, the repos, and the libraries

**A brief for the sessions that touch this from the other side** — the one
running the release mirrors and the portal, the one owning `diluvium-drt`,
and whoever turns the nine `*-lib` repositories into packages. Written
2026-09-12 from the dollup side; every fact below was checked that day
against the live hosts, the tagged sources, and a scratch run of the tools.
Where something is a proposal it says so.

## 1. What exists today

**The unified release mirror is live and dollup is already on it.**
`https://software.aloecraft.org/releases/<project>/` serves, per project:
one directory per tag holding the release assets, `SHA256SUMS.txt` and
`BUILDINFO.txt`; `latest/` and `latest-prerelease/`; and `releases.json`.
The root serves `mirrors.json`. The page says what it is: a mirror of what
each project publishes on GitHub, verified against the checksums published
with the release. dollup v0.0.1 is there.

**A project's mirror entry is only as rich as what it commits.** drt's
`releases.json` is `source: changelog` — version, stable, facts (`dv_abi`,
`diluvium`), summary, upgrading, notes — because drt commits a
`changelog.json` rendered from `CHANGELOG.yaml` by `script/changelog.py`.
dollup's is `source: github`: no version, no facts, no notes. The
changelog's `mirror: true` flag decides what the mirror carries and
`latest: true` decides what `latest/` resolves to; drt's candidates are
`mirror: false`, so rc8, rc9 and 0.6.0rc1 are not on the mirror, and the
mirror's `latest_prerelease` is still v0.4.1.

**The portal probes mirrors.** `aloecraft-software-portal` at `b1d0fe1`
fetches `<mirror>/releases.json` for every channel marked `kind: mirror`
and upgrades the entry to a link carrying `latest`. Its `projects.json`
lists dollup with three channels: the mirror (planned), the package repo
(planned) and GitHub releases (live). dollup's own `site/site.json` carries
no mirror channel yet.

**dollup.aloecraft.org is deployed from the unsigned build; the standard
repo's tree has moved and is signed.** The page is served, says "not
published yet", and `/std-repo/` answers 404. The tree lives in
`Aloecraft-org/drt-std-lib` (its own site-contract repo, to be staged as a
sibling subtree at `/std-repo/` under the dollup vhost), every package
stating its license, and since `99784b4` its `index.json.sig` and
`std-repo.pub` are committed under the pinned key. Its `site/build.sh`
stages a signed repo with its blob projection; nothing is served until
lk_web deploys it. Meanwhile dollup's scaffold names the zipball peer of
that tree under the same key, and a fresh root pulls the standard packages
through it today, signature checked, with the dead served copy skipped and
said.

**discofetch-api is a dollup repo on GitHub, and today's dollup refuses
it.** `packages/discofetch-api/0.1.0/manifest.json` carries
`"diluvium": ">=5.5.1"`; the format now demands the 40-hex revision `drt
buildinfo` reports, or `dv_abi` for "any compatible core", and both `repo
seal` and `repo index` refuse the tree by name. The repo is also unsigned
on the network (`index.json.sig` is not committed), which is why its README
tells a consumer to turn `require_signatures` off. Its
`reference/discofetch-api.host.lua` is a `.host.lua` config, a format drt
0.6.0rc1 removed.

**The nine libraries are packages in all but name.** Each of
`discofetch-model-lib`, `discofetch-fetchpoint-lib`,
`discofetch-accounts-lib`, `discofetch-db-lib`, `drt-db-lib`,
`drt-http-api-lib`, `token-bucket-lib`, `token-rate-limit-lib` and
`node-event-lib` is one `.dlua` module extracted from discofetch's
`api/supervisor.lua`, wired by injection through `M.new(deps)` with no
`require`, plus `test/cases.dlua` and a `test/run.sh` identical across all
nine. None has a manifest, a tag, a license or CI. Their READMEs say
consumption waits on `require`; it no longer does — see §4.

**Proven in scratch, with today's tools.** Two of them (`token-bucket`,
`token-rate-limit`) wrapped in the manifest below seal, index, and pull with
the dependency resolved: `init/token_bucket.dlua`, `init/token_rate_limit.dlua`,
locked by content hash, audit says start would run. Then drt 0.6.0rc1,
pulled through dollup from GitHub's download directory, deployed them beside
the entry and resolved both through `require` at start, exit 0.

```json
{ "name": "token-rate-limit", "version": "0.1.0",
  "guest": { "modules": { "token_rate_limit": "guest/token_rate_limit.dlua" },
             "source_only": true },
  "requires": { "packages": { "token-bucket": "^0.1" } } }
```

## 2. What dollup reads from the mirror — the contract to keep

dollup's runtime verbs (`pull drt`, `deploy drt`, `pin drt`, `audit`) read
exactly this, and nothing else, from a release directory:

| read | from | why |
|---|---|---|
| `<base>/drt_linux_x86_64_musl` or `<base>/drt_linux_static_x86_64` (and the darwin, slim and windows names under both spellings) | the tag's directory | the asset for this platform, under `doc/ALIGNMENT.md` §4's spelling or the one every release up to 0.6.0rc1 used — whichever the sums list; never guessed from a version |
| `<base>/SHA256SUMS.txt` | same | the asset is hash-checked before it is cached; audit identifies a deployed binary by hash against it and never executes it |
| `<base>/BUILDINFO.txt` | same | cached beside the asset; its `tag:` line is the one fact read |
| `<project>/latest/BUILDINFO.txt` | the origin's `releases/latest/download/`, then the mirror's `latest/` | `latest` resolves to the tag on that line, and is cached under the version — never as "latest" |

`<base>` is GitHub's `releases/download/<tag>/` first and
`https://software.aloecraft.org/releases/diluvium-drt/<tag>/` second by
default — the origin leads for as long as the mirror lags it (dollup
0.1.1); the order is one constant to flip back once the mirror is current
— or whatever `--from` names. The pin in `project.json` is the
tag without its leading `v` (`0.4.1`, `0.6.0rc1`), which is what drt 0.6.0
compares its stamped release tag against, so mirror directories must stay
keyed by tag and `BUILDINFO.txt` must keep its `tag:` line. Nothing reads
`releases.json` yet.

**`--from` accepts any directory with that layout.** GitHub's
`releases/download/<tag>/` is one, so a candidate the mirror does not carry
is one command away for anyone: `dollup pull drt 0.6.0rc1 --from
https://github.com/Aloecraft-org/diluvium-drt/releases/download/v0.6.0rc1`.
Verified today, hash-checked.

**What would break dollup:** an asset name under neither spelling,
dropping the sums or BUILDINFO from a tag directory, keying a directory by
version instead of tag, or `latest/` losing its `BUILDINFO.txt`. The two
places are a fallback list: one that cannot be read is passed over and the
next asked, said, and one whose bytes disagree with its own sums is a
refusal no later place papers over (dollup 0.1.1). What would improve it: if
`releases.json` is meant to be the API, say so and dollup will resolve
`latest` and `latest-prerelease` through it and stop reading `latest/`.

## 3. Proposed: what a repo is, and how many

Two units are being conflated. A git repository is where a library's source
lives, one per library; that part is done. A dollup repo is a published,
signed index that a root adds as a source. Each of those costs a key, a
source line in every consumer, and a signature to keep in step with the
index — so there should be few, sorted by who publishes and signs.
Requirements go in the manifest, not in the split.

- **`drt-std-lib`**, served at the `std-repo` URL every `init` pins, key
  `ed25519:RZNTaXSePtutwF3IWX49hppum4O8DdCiyx7BcYSmrRc=`: `token-bucket`,
  `token-rate-limit`, `drt-db`, `drt-http-api`, `node-event`, plus the
  existing `hello`, `hostcall` and `starter`. `drt-db` and `node-event`
  declare `"connectors": ["sql"]`, so a slim profile refuses them by name;
  that is what the field exists for.
- **`discofetch-api`**, which already exists with a minted key
  (`ed25519:SVJvQTvveNjYy6r9wtqPIzf5UQ3JNuOYSUmlneMjMlo=`): add
  `discofetch-model`, `discofetch-db`, `discofetch-accounts` and
  `discofetch-fetchpoint` beside the program. One source line for an
  operator, one key, one sync discipline.
- **`dollup-net-lib`** stays reserved for libraries over the network
  connectors. None of the nine belongs there yet.

Moving a package between repos later is cheap: identity is the content
hash, so locks stay valid and only the source line changes.

**Where std lives.** For the zipball peer in `doc/RepoFormat.md` §2 to work,
the git repository's root must be the repo tree, so `std-repo/` moves out of
`Aloecraft-org/dollup` into `drt-std-lib`, which implements the site
contract (`doc/CONTRACT.md` in the portal repo) and is deployed by lk_web as
a sibling subtree under the dollup vhost. dollup's page keeps reading the
live index; dollup's own site stops shipping the tree.

**Names.** Package names are the hyphenated repo stems without `-lib`
(`token-bucket`); module names are the Lua identifiers the files already
use (`token_bucket`), which drt's module rules accept and dollup projects to
`init/token_bucket.dlua`. Application packages keep their `discofetch-`
prefix; std packages are bare. Namespacing proper is still open
(`RepoFormat.md` §9) and this does not close it.

## 4. drt 0.6.0rc1, read from the tag

Cut from `eacfbe9b1c8322fa663187ec1fcfe0f774d4d94c` on 2026-09-12, `stable:
false`, `mirror: false`. What it means here:

- **`require` shipped.** Every `.dlua` or `.lua` beside the entry is a
  module, resolved by the host before the program runs. A library pulled
  by dollup into `init/` is consumed on a released root today; the proof in
  §1 ran on this binary.
- **A pin is the release tag without its `v`.** `0.6.0rc1` is a version a
  root can name; `0.6.0` matches the release and not its candidates. dollup
  already spelled it that way, and `audit` now names a mismatch in start's
  own words (dollup `fdc965a`).
- **`drt-config` gained `modules::is_component`**: the walk does not enter
  a directory whose name could not be a component. dollup's projection
  already puts modules at the top of `init/` by name and everything else
  under `<package>/`, so the rule and the layout agree; dollup's dependency
  moved to the tag's commit (`3b88ee2`).
- **`.host.lua` is gone.** Every config is JSON. discofetch-api's reference
  config no longer loads.
- **The deploy overlay is still the open edge**, in drt's own words
  (`doc/Modules.md`, "Sharing across repos"): deploy copies `dlua_dir` when
  the profile sets one and `init/` when it does not, never both, so a pull
  into `init/` reaches a released root and not a development one. Until it
  is settled a development root vendors libraries into `dlua_dir`. dollup
  keeps `dollup deploy <app>` held on this.
- **`commit` still clears `init/`** before capturing `live/<name>` into it
  (`crates/drt/src/deploy.rs`, the `commit` function at the tag). On a
  released root that round-trips; on a development root it replaces pulled
  packages with the `dlua_dir` copy. Same open edge, second face.
- **A `features` compatibility fact** (`regex` today) travels in
  `BUILDINFO.txt` beside `dv_abi`. The package manifest has no
  `requires.features`; dollup will add one the day drt checks it at
  admission, and not before — a field nothing evaluates is worse than none.

## 5. Asks, by session

**Mirror and portal.**
1. Add `Aloecraft-org/drt-std-lib` to `sites.json` as a sibling subtree at
   `/std-repo/` under the dollup vhost (its `site/site.json` says so), with
   `site/nginx/std-repo.conf` as the path's drop-in; its tree is signed
   now, so stage and deploy it, and redeploy dollup's own page from
   current `main`. Then flip the package-repo channel to `live` in both
   `site.json` files and the portal's `projects.json`.
1a. Regenerate dollup's mirror entry: v0.1.0 exists and carries
   `changelog.json`, but `latest/` still serves v0.0.1, so an unpinned
   `install.sh` gets the old release, and the aligned Linux name answers
   404 there.
2. `doc/ALIGNMENT.md` (in dollup, from the alignment session) renames
   every project's artifacts; dollup reads a release's asset under either
   spelling, chosen by its `SHA256SUMS.txt`, so the mirror can carry both
   generations.
3. Say whether `releases.json` is the API (§2). If yes, dollup switches.
4. The probe generalizes: a dollup repo can answer for itself through its
   signed `index.json` (`"dollup_repo": 1`, a package count), the way a
   mirror answers through `releases.json`.
5. dollup's entry will turn `source: changelog` once dollup adopts drt's
   changelog schema (§6); nothing to do on the mirror side for that.

**drt.**
1. `install.sh` and `doc/Release.md` still name
   `diluvium.aloecraft.org/…/drt`; the live mirror is
   `software.aloecraft.org/releases/diluvium-drt/`, which dollup already
   uses.
2. The deploy overlay decision (§4), which also decides the `commit`
   behaviour on development roots.
2a. The pin normaliser (`doc/ALIGNMENT.md` §10): a pure function in
   drt-config that compares `0.5.0rc9` and `0.5.0-rc.9` equal for one
   cycle. dollup's audit and deploy compare pins as strings today and will
   call it the day it exists; until then a root pinned under the old
   spelling mismatches a binary tagged under the new one, on both sides.
3. Whether `requires.features` will be checked at admission, so dollup
   knows whether to carry the field.

**technoproj (the shared release tooling).** dollup's `.technoproj`
declaration and `CHANGELOG.yaml` already validate under
`technoproj-changelog` and render the same `changelog.json`; its
`version.mk` is in dollup byte for byte. dollup keeps a vendored copy of
the engine until the installed one carries what the release workflow
needs, then switches by `pip install` with nothing else changing:
1. A dev tag (`vX.Y.Z-dev.N`) as a build of the newest entry -- no entry
   of its own, `prerelease=true`, `version=X.Y.Z-dev.N` -- in
   `release-check` and `render md --tag` (ALIGNMENT §7).
2. A SemVer-spelled prerelease version (`0.2.0-rc.1`) accepted by
   `consistency`, with a `semver` stamp meaning the tag body, not the
   base (ALIGNMENT §1, revision 3); today the grammar there is the
   legacy `X.Y.ZrcN` only.
3. `buildinfo --tag TAG`: the entry's declared facts as `key: value`
   lines, so a workflow writes BUILDINFO.txt from the same tool that
   renders the notes.
4. A tag to pin: the README says `@v0.1.0` and no tag exists yet.

**Library owners (the nine repos and discofetch-api).**
1. In each lib repo: `manifest.json` as in §1 plus `"license":
   "Apache-2.0"` (the family license; `repo seal` and `repo index` refuse
   a package without one), the module under `guest/`, a `v0.1.0` tag, and
   a LICENSE file. `drt-db` and `node-event` declare the sql connector.
2. One shared reusable workflow: install dollup and drt from the mirrors,
   `dollup repo seal` and `dollup repo index` into a temp tree, run the
   existing `test/run.sh`. Nine repos, one workflow.
3. In each published repo, an import script that copies a tagged lib into
   `packages/<name>/<version>/` and seals it — `sync-from-discofetch.sh`
   already has the shape.
4. discofetch-api: replace `">=5.5.1"` with the 40-hex revision or a
   `dv_abi`, commit `index.json.sig`, and rewrite the reference config as
   JSON.

**dollup (this session).** Done: source fallback (`9363449`), pin
mismatch wording (`fdc965a`), `drt-config` at rc1 (`3b88ee2`), license as
package metadata, `std-repo/` moved to `drt-std-lib`, releases the mirror
does not carry taken from the origin, both artifact spellings read, and
the alignment shape: `.technoproj`, `CHANGELOG.yaml`, the declaration-
driven changelog engine, a release workflow that derives its prerelease
flag, its body and its BUILDINFO facts from the changelog, aligned artifact
names, a mirror-first installer, and a nightly dev build. Next: scaffold
the zip peer once drt-std-lib is signed (one constant, `root::STD_REPO_ZIP`),
and cut v0.1.0 through the workflow (revision 3 of the alignment makes
the first conforming release a minor), which turns dollup's mirror entry
`source: changelog`.

## 6. Order of work

1. Deploy the committed site (mirror session). Unblocks every first pull.
2. Move std-repo into `drt-std-lib`; scaffold the zip peer (dollup).
3. Package the nine and fix discofetch-api (library owners), import into
   std and discofetch-api, sign, publish.
4. dollup's release conformance and v0.0.2 (dollup).
5. Held on drt: `dollup deploy <app>`, until the overlay is decided.
