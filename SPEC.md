# Dollup — distribution for Diluvium/DRT artifacts

**Status:** founding spec for the `dollup` repository. Working name; check
crates.io (`dollup`), the domain, and search collisions before it appears in
anything public (same rule as ego-transport). Decisions here were made
deliberately; where something is open it says so. This spec is generic:
nothing in it names any particular deployment or product.

## 1. What dollup is, and what it is not

Dollup is a **fetcher and resolver over a content-addressed store**, plus
transport for hibernated-instance snapshots. It is not a package manager in
the LuaRocks/npm sense, and the difference is load-bearing:

- **Install is inert; config is authority.** Fetching an artifact into the
  store confers nothing. A program can run only when root config names it and
  the operator supplies scopes. Dollup never grants, never runs, never speaks
  the dv ABI.
- **DRT never fetches.** DRT resolves code from a local directory (the
  auto-available directory already in its design, pointed at by env var or
  flag). Dollup's whole job is to populate that directory correctly. The
  trust boundary is the directory, not the tool.
- The manifest is **declarative, never executable**. LuaRocks executes its
  rockspec to describe a package; for a capability runtime, arbitrary code at
  install time is the entire ballgame. Rejected.
- **A published package is identical for every consumer; personalization
  lives entirely in the deployment's config.** No tokens, endpoints, or
  operator-specific values inside artifacts, structurally enforced where
  possible (the manifest has no scope-bearing fields) and stated as a
  publishing rule where not. This is what makes "the public guide and the
  production deployment share one front door" literally true.
- **The binary ships knowing zero URLs; the scaffold may know some.** No
  compiled-in sources or fallbacks: at resolve time dollup consults only the
  deployment's config, and an empty source list resolves nothing rather than
  silently reaching a default. `dollup init` scaffolds the config, and that
  scaffold may name the standard public source(s); they are ordinary lines
  in a file the operator owns, so deleting or replacing them is a one-line
  edit that nothing resurrects. Self-hosting is that edit.

Dollup is a standalone binary and a library crate. DRT may link the crate for
convenience verbs (`drt run <ref>` delegating resolution), but the binary is
the canonical interface and neither side depends on the other to function.

## 2. Two artifact kinds, one envelope

Every artifact is `{kind, manifest, content}`:

| kind | manifest describes | content |
|---|---|---|
| `package` | code: modules, requirements, compatibility | `.dlua`/`.lua` source files |
| `snapshot` | state: code-set pin, identity, provenance | one instance-ABI snapshot blob |

The envelope is shared so the store, transport, verbs, and integrity checks
are written once. The kinds differ in **publicity defaults** (§7): packages
are public artifacts; snapshots are live state and are private by default.

A `package` carries up to three **faces** — a capability contract, guest
`.dlua` modules, and a host-side connector implementation per target — of
which any subset is legal. `doc/RepoFormat.md` §4 defines them; §6 governs
what carrying a host face does and does not permit.

Manifest types are **drt-config serde types** (source of truth; LuaCATS defs
generated, never authored). Dollup depends on `drt-config` and adds nothing
schema-shaped of its own.

## 3. Identity and sources

**Identity is the content hash of the resolved file set** (package) or blob
(snapshot). Hash: SHA-256. (Boring and universal beats fast here; BLAKE3 is
the noted alternative if hashing ever shows up in a profile. One algorithm
per store; the hash name is recorded in the envelope so a future migration is
a re-hash, not a format break.)

**Sources are schemes.** Same move as endpoint refs: implement the minimum
now, grow additively, never break format.

- `https://` — v1. A static repo: an index plus content-addressed blobs
  behind plain nginx. This is the hosted mirror (§8), and it is a directory
  served by a web server, not a service.
- `zip+https://` — v1. One archive URL, fetched whole, extracted, and
  verified against the index inside it. Any forge's zipball endpoint is such
  a URL; dollup never grows a forge adapter, because knowing one forge means
  being asked to know four.
- `git+https://` — v1. Tags and branches accepted as *input*; resolution
  records the commit SHA and content hashes. Nothing mutable is ever a pin.
- `file://` — v1. Local paths and file-based remotes (a directory is a valid
  remote; rsync/scp/nfs become transport for free).

All four resolve to the same tree and therefore the same content hash, so a
lockfile is portable across them and the source list is a genuine fallback
list. `doc/RepoFormat.md` is normative for the format; this section names the
schemes.

**Select-within-repo:** a git source may contain many packages. An index file
at the repo root enumerates them (name → subpath). A ref is
`<source>#<name>`. One repo, many small programs, no repo-per-package
ceremony.

**Lockfile:** per deployment. Records name → source → resolved commit →
content hashes, for packages and pinned snapshots both. The lockfile is the
reproducibility artifact; "repeatable deployment of bundled components" is
this file doing its job.

**Deployment:** the unit dollup verbs operate on is a directory containing
the lockfile, the resolved code directory the runtime is pointed at, and the
source list. It is deliberately the thing a service manager references: a
unit file that runs the runtime against this directory's config is the whole
"install as a service" story, and dollup never writes or manages unit files
itself. Verbs act on the deployment in the current directory or an explicit
`--deployment PATH`; nothing is ever implicitly global.

## 4. Package manifest

`doc/RepoFormat.md` §5 is normative for the full shape, including the three
faces and host-face targets. The fields below are the ones this spec's
reasoning depends on. All declarative:

- `name`, `version` (semver)
- `modules`: module name → file (host-side resolution; code is handed to the
  instance at construction via `dv_register_code`; no ambient search path, no
  `require`-time fetching, ever)
- `files`: path → hash (the identity input)
- `capabilities`: required grants as effect × capability, **generic names
  only** (`host:kv`, not an implementation name), **no scopes** (scopes stay
  host-side; the operator supplies them; config never carries the package's
  filenames)
- `connectors`: required connector names + call-shape version range (what
  must exist in the host; see §6)
- `dependencies`: package name + version requirement (hashes land in the
  lock, not the manifest)
- `diluvium`: minimum language version; `dv_abi`: accepted
  `DV_ABI_VERSION` range
- `guest.source_only`: true unless the deployment opts out loudly. Compiled
  diluvium chunks are rejected at publish and install. There is no bytecode
  verifier; source-only is a mitigation, not a solution, and the format does
  not pretend otherwise. It sits under `guest` because it is a claim about
  the guest face alone — a host face is binary by definition, and a top-level
  flag would have made the two contradict each other.
- `capability`: the contract a package defines — capability name, scope type,
  call names, shape version. Pure data, no execution semantics; it is the
  thing a guest face and a host face are both checked against.
- `host.targets`: connector implementations per Rust target triple, each
  tagged with its ABI (wasm component, browser module-with-glue, native
  shared object). Materialization is gated per §6.

**Failure is at admission time, by name.** A package declaring a capability
the deployment ceiling cannot satisfy, a connector the build does not carry,
or an ABI range the host is outside of, fails when config names it, with the
manifest line quoted. Never a mystifying `denied` at first call.

## 5. Snapshot manifest

- `state`: blob hash
- `code_set`: the exact package identity (content hash) the instance was
  running. Restore is valid against this and nothing else; this is the
  perfect-bytecode-match rule made portable.
- `identity`: the host identity stamp (as in `dv_snapshot`'s host arg)
- `capabilities`: the **generic** capability names the guest expects to
  exist at restore. Interface expectations, not grants; grants are re-made by
  the restoring host's config, by attenuation, as ever.
- `created`, `dv_abi`

Restore flow: dollup materializes (fetches snapshot manifest, ensures the
pinned code-set is present in the store, fetching it by ref if absent, and
populates the resolved directory); **DRT performs the restore.** Dollup's
last act is files on disk.

Queues are volatile and guest-declared; they are not snapshot content and do
not appear here.

## 6. Capability handling: the four layers

Installing something that *enables* a capability is not one act. The layers,
each a separate deliberate step, each in existing vocabulary (no new nouns):

1. **Artifact integrity** — hash verification. Dollup's layer.
2. **Registry admission** — what this host build *offers*. Changing the
   connector registry is a process-level trust act by the operator.
3. **Scope wiring** — root config supplies scopes for offered capabilities.
4. **Grants** — attenuation from the ceiling, checked identically at every
   depth.

Install stops after layer 1. Layers 2–4 belong to DRT and its operator.

**Packages may carry host-side code, and install is still inert.** A package
may declare a capability contract and ship a connector implementation for it
(`doc/RepoFormat.md` §§4–6). Placing that implementation on disk enables
nothing: DRT loads a connector because root config *names* it in the connector
registry — layer 2, which was always "a process-level trust act by the
operator". The four layers are unchanged; a package may now carry material
addressed to layer 2 as well as layer 1.

Unchanged doctrine is not the same as unchanged risk, so the tooling takes the
asymmetry §7 takes with snapshots, one mechanism serving a second use: a host
face is **not materialized by default**, wasm targets need `--with-host`, and
a native target needs `--with-host-native` and says the true thing — that
installing one is the same class of act as `apt install`, bounded by neither
the capability model nor the instruction budget.

The fd-channel plugin protocol stays rejected as-is: it execs an absolute path
and moves opaque frames, so there is no admission layer between layer 1 and a
running native process. The wasm component is the preferred host-face target
for exactly the reason DRT's own §7 prefers it — it is sandboxable, and a
shared object is not. Dollup may distribute host faces before DRT can load
them; placement is inert, so the two timelines do not block each other.

## 7. Snapshot publicity asymmetry

Packages and snapshots have opposite defaults and the tooling enforces the
difference:

- Packages: pushing to any remote is unceremonious.
- Snapshots: **private by default**. Pushing a snapshot to any non-`file://`
  remote requires an explicit acknowledgment flag. Snapshot blobs are treated
  as secret-bearing (known: secure-function scrambling is not inherited by
  snapshots). A registry never lists snapshots publicly; snapshot transport
  over a registry, if it ever exists, is authenticated point-to-point.

## 8. The hosted repo (defined, not deferred)

The hosted artifact repo is a **static mirror**: content-addressed blobs plus
a generated JSON index behind plain nginx. No server logic, no accounts, no
dynamic anything. This shape is already proven in-house (the release mirror:
nginx + a generation script + stable paths; Artifactory previously rejected
as overkill), and `doc/RepoFormat.md` defines it.

The standard source is `https://dollup.aloecraft.org/std-repo/`, with
`zip+https://` against the public `dollup-standard` repo as its peer. The
scaffold names both, pointing at the same content: identity is the content
hash, so they are interchangeable rather than ranked, and naming two costs
nothing while buying resilience. Consequences:

- Self-hosting a repo = serve a directory. A base-URL change in the source
  list is the entire migration.
- Multiple sources are first-class in the resolver (an ordered source list),
  so private mirrors, air-gapped copies, and vendor repos are the same code
  path as the public one.
- Namespacing, publisher identity, and signing remain **open**, and host
  faces sharpen rather than change that. v1's trust statement stays honest
  and small: you chose the source, and the bytes matched the index. A public
  repo needs artifact signatures (TUF-shaped or simpler) before it can claim
  more; that design is not attempted here.

## 9. Upgrade policy (candidate, not settled)

Working hypothesis to test this spec against, explicitly not a decision:

- **Code is exact-match.** An instance pins its code-set at spawn; snapshots
  restore only against that pin. Program upgrades are respawns, never
  in-place mutation of a running or sleeping instance.
- **Capabilities are interface-match.** Manifests declare generic capability
  names; whatever satisfies the name at restore time is acceptable. Connector
  implementations upgrade freely underneath sleeping agents; in-flight and
  cross-boundary guarantees are the application designer's, as already
  established.
- The compatibility surface of a capability is its name plus call shape,
  versioned in the connector requirement range.

If a real use case breaks this split, the manifest fields above are the
place it will show, and the fields are versioned with the envelope.

## 10. Verbs

`dollup init` (scaffold a deployment: config with the standard sources,
empty lockfile), `dollup add <ref>` (fetch, lock, populate — inert), `update`, `lock`,
`ls`, `info <ref>`, `verify` (re-hash store against lock), `push <remote>
<artifact>` (snapshot push gated per §7), `pull <remote> <ref>`, `gc`
(collect the store against lockfiles and pinned snapshots). Restore is a DRT
verb; dollup only materializes.

## 11. Documentation stance

A threat-notes document exists in the repo from day one and grows with the
design (what dollup does and does not check, what source-only does and does
not buy, what a snapshot blob contains). **No guarantees are pinned until
the shape is known**; the notes become commitments deliberately, item by
item, not by default.

## 12. v1 cut and acceptance

**In:** content-addressed store; all four schemes (`https`, `zip+https`,
`git+https`, `file`); the repo format and index; select-within-repo;
lockfile; resolved-directory population; the three-faced package manifest
including capability contracts and host-face targets, with admission-time
named failures; the host-face materialization gates; snapshot envelope with
local and file-remote push/pull and the publicity gate; `verify` and `gc`.

**Seams only:** signing; the static mirror's generation script; whatever DRT
needs to actually *load* a host face, which is DRT's timeline, not this one.

**Out, captured:** registry service and index generator; connector loading;
publisher identity; any UI; dollup's own delivery (it ships as a static
single binary through whatever release channel exists; how it reaches a
machine is not this spec's concern, only that once present it needs nothing
else installed).

**Acceptance, two demos:**

1. Clean machine: create a deployment directory, `dollup add git+<url>#<prog>`
   into it, run the runtime against it (interactively and via a trivial unit
   file pointing at the deployment); both work. The same package against a
   deployment with an insufficient ceiling fails at admission, by name.
2. Durable-agent migration: hibernate an instance on machine A; `dollup
   push` (flag acknowledged) to a file remote; `dollup pull` on machine B;
   DRT restores against the identical code-set; the agent resumes. Same
   agent, different machine, no hand-copied files.

## 13. Open questions (deliberately)

Upgrade policy confirmation (§9); repo namespacing and signing (§8 — more
pointed now that host faces exist); whether snapshot transport remains in
dollup long-term or migrates toward DRT once the snapshot store trait grows a
remote impl; store location and sharing (per-user XDG default vs
deployment-shared read-only store); whether a capability face may declare a
scope type no shipped validator implements.

**Settled since drafting.** The name: `dollup` is unclaimed on crates.io with
zero registry hits. Repo placement: own repo consuming `drt-config`, which is
viable because `drt-config` → `drt-caps` → serde/rmpv/thiserror pulls in no C
core, so dollup builds with no toolchain beyond cargo. Connector distribution
(§6): decided — packages carry host faces, gated, with the wasm component
preferred. The hosted repo (§8): defined, not deferred.
