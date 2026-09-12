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

**dollup.aloecraft.org is deployed from the unsigned build.** The page is
served, says "not published yet", and `/std-repo/` answers 404 — while the
signed index, its signature and `site/std-repo.pub` have been committed in
`Aloecraft-org/dollup` since 2026-09-02 (`3d4b873`). The site build takes
its signed branch and ships `std-repo/` from a current checkout; the
deployed tree is simply older than that commit. Consequence, measured: a
fresh `dollup init` scaffolds this source, and until dollup `9363449` its
first `pull` died on it whatever other sources were listed. dollup now
passes over a source it cannot read and says so; the source is still dead
until the site is redeployed.

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
| `<base>/drt_<slim_>linux_static_x86_64` (and the darwin names) | the tag's directory | the asset, named by the platform rule in `runtime::asset_name` |
| `<base>/SHA256SUMS.txt` | same | the asset is hash-checked before it is cached; audit identifies a deployed binary by hash against it and never executes it |
| `<base>/BUILDINFO.txt` | same | cached beside the asset; its `tag:` line is the one fact read |
| `<project>/latest/BUILDINFO.txt` | the mirror | `latest` resolves to the tag on that line, and is cached under the version — never as "latest" |

`<base>` is `https://software.aloecraft.org/releases/diluvium-drt/<tag>/`
by default, or whatever `--from` names. The pin in `project.json` is the
tag without its leading `v` (`0.4.1`, `0.6.0rc1`), which is what drt 0.6.0
compares its stamped release tag against, so mirror directories must stay
keyed by tag and `BUILDINFO.txt` must keep its `tag:` line. Nothing reads
`releases.json` yet.

**`--from` accepts any directory with that layout.** GitHub's
`releases/download/<tag>/` is one, so a candidate the mirror does not carry
is one command away for anyone: `dollup pull drt 0.6.0rc1 --from
https://github.com/Aloecraft-org/diluvium-drt/releases/download/v0.6.0rc1`.
Verified today, hash-checked.

**What would break dollup:** renaming assets, dropping the sums or
BUILDINFO from a tag directory, keying a directory by version instead of
tag, or `latest/` losing its `BUILDINFO.txt`. What would improve it: if
`releases.json` is meant to be the API, say so and dollup will resolve
`latest` and `latest-prerelease` through it and stop reading `latest/`.

## 3. Proposed: what a repo is, and how many

Two units are being conflated. A git repository is where a library's source
lives, one per library; that part is done. A dollup repo is a published,
signed index that a root adds as a source. Each of those costs a key, a
source line in every consumer, and a signature to keep in step with the
index — so there should be few, sorted by who publishes and signs.
Requirements go in the manifest, not in the split.

- **`dollup-std-lib`**, served at the `std-repo` URL every `init` pins, key
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
`Aloecraft-org/dollup` into `dollup-std-lib`, which implements the site
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
1. Stage and deploy `Aloecraft-org/dollup` from current `main` so
   `/std-repo/` is served with its signature, then flip the package-repo
   channel to `live` in dollup's `site/site.json` and the portal's
   `projects.json`. Nothing else unblocks a first `dollup pull` for anyone.
2. Plan a sibling subtree for `dollup-std-lib` under the dollup vhost (§3).
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
3. Whether `requires.features` will be checked at admission, so dollup
   knows whether to carry the field.

**Library owners (the nine repos and discofetch-api).**
1. In each lib repo: `manifest.json` as in §1, the module under `guest/`,
   a `v0.1.0` tag, and Apache-2.0, which discofetch-api names as the
   family license. `drt-db` and `node-event` declare the sql connector.
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
mismatch wording (`fdc965a`), `drt-config` at rc1 (`3b88ee2`). Next, in
order: move `std-repo/` to `dollup-std-lib` and scaffold the zip peer;
adopt `CHANGELOG.yaml` and drt's changelog script with a committed
`changelog.json`, wired into release preflight; make `install.sh`
mirror-first with a GitHub fallback and an air-gap override; stamp dollup's
own compatibility facts into `BUILDINFO.txt` (the repo format version, the
`drt-config` revision); add the mirror channel to `site/site.json`; cut
v0.0.2 through all of it.

## 6. Order of work

1. Deploy the committed site (mirror session). Unblocks every first pull.
2. Move std-repo into `dollup-std-lib`; scaffold the zip peer (dollup).
3. Package the nine and fix discofetch-api (library owners), import into
   std and discofetch-api, sign, publish.
4. dollup's release conformance and v0.0.2 (dollup).
5. Held on drt: `dollup deploy <app>`, until the overlay is decided.
