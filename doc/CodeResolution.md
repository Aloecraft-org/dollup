# Code resolution — what DRT needs, proposed as DRT's own

**Status:** a request addressed to `aloecraft-org/diluvium-drt`, written from
the dollup side and staged so it does not block DRT's first core milestone.
Nothing here is a dollup dependency; see §1, which is the whole argument.

## 1. The framing: this seam is DRT's, not dollup's

Dollup's founding spec says DRT resolves code from a local directory and that
**the trust boundary is the directory, not the tool**. Take that literally and
the consequence is that *dollup should not appear in DRT's design at all.*

What DRT gains is a concept it already half-has: a **code root** — a directory
of packages that DRT reads at admission time and never writes, never fetches,
and never learns the provenance of. Dollup is one way to fill it. `git clone`
is another. `rsync` from a build host is another. A `Makefile` is another.
DRT cannot tell them apart and must not try.

This matters for DRT's own doctrine as much as dollup's. SPEC.md's rule is
that every future direction is present as a *seam*, never a blocking
dependency, and §7 already refuses dynamic loading on exactly these grounds.
A code root is a seam of that shape: it is a directory contract, so the thing
on the other side of it is replaceable by anything that can write files.

If DRT specs this, dollup needs nothing further from DRT, ever — and DRT
acquires no dependency on dollup, ever. That is the trade being proposed.

## 2. What is missing today

Three factual gaps, from reading the tree:

- `drt_config::Program` is `Path(PathBuf) | Source(String)`. One file, or
  inline text. There is no module set, no asset, no directory, no name.
- `dv_register_code` — the ABI call that hands a code set to an instance at
  construction — appears once in the repository, in SPEC.md §4's list of
  `diluvium-sys` functions **not yet transcribed**.
- Nothing reads a package manifest, because no manifest type exists. So the
  "fails at admission, by name" promise, which SPEC.md makes for grants and
  scopes, has no equivalent for code.

## 3. The proposal, as types

Everything below is **additive to the serde surface**. Existing configs —
`examples/deployment.json` included — keep parsing unchanged, because the two
current variants keep their names and their meaning. That is deliberate: this
should be safe to land in the middle of a milestone.

```rust
/// Where a program's code comes from. `path` and `source` are today's
/// variants, untouched.
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Program {
    /// A single `.dlua`/`.lua` file. Today's behaviour, kept.
    Path(PathBuf),
    /// Inline source text. Today's behaviour, kept.
    Source(String),
    /// A code set carried in the config itself: modules, assets, and an
    /// entry point, each resolved to bytes before construction. This is the
    /// in-memory case — an embedder builds one in Rust and no file is ever
    /// touched — and it is also how bytecode and static assets arrive
    /// without a directory.
    Set(CodeSet),
    /// A package named in the deployment's code root (§4).
    Package(PackageRef),
}

/// A complete unit of content for one instance. `modules` are handed over
/// with `dv_register_code`; `assets` are not code and are never registered.
#[derive(Serialize, Deserialize)]
pub struct CodeSet {
    /// The entry module's name. Must be a key of `modules`.
    pub main: String,
    /// Module name → content. Host-side resolution, always complete before
    /// construction: no ambient search path, no `require`-time anything.
    pub modules: BTreeMap<String, Content>,
    /// Asset name → content. Images, JSON, fonts. Reachable only through a
    /// capability the deployment grants (§6), never through code loading.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub assets: BTreeMap<String, Content>,
}

/// One blob, however it is spelled. Every variant resolves to bytes eagerly.
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Content {
    Path(PathBuf),
    Source(String),
    /// Raw bytes — msgpack `bin`, base64 in JSON. Assets, and precompiled
    /// chunks where the deployment has opted in (§7).
    Bytes(Vec<u8>),
}

/// Naming a package in the code root. Accepts a bare string
/// (`{"package": "hello"}`) or the full form.
#[derive(Serialize, Deserialize)]
pub struct PackageRef {
    pub name: String,
    /// Defaults to the package manifest's own entry module.
    pub module: Option<String>,
    /// A semver requirement, checked against the index. Absent = any.
    pub version: Option<String>,
}
```

And one process-level field, beside `connectors` and `listeners`, because a
code root is a property of the deployment in exactly the way those are:

```rust
pub struct RootConfig {
    // ... existing fields ...
    /// The directory packages are read from. DRT reads it and nothing else:
    /// never writes it, never fetches into it, never learns where its bytes
    /// came from. Set by config, `--code-root`, or `DRT_CODE_ROOT`.
    pub code_root: Option<PathBuf>,
}
```

## 4. The code root on disk

```
<code_root>/
  index.json                 # optional; derived by scan when absent
  <package-name>/
    manifest.json            # the package manifest
    <files as the manifest names them>
```

`index.json` maps name → `{version, path, hash}`. It is an **optimisation and
a consistency check, not a requirement**: when it is absent DRT scans
subdirectories for `manifest.json` and builds the same map. That keeps "clone
a repo into the code root and it works" true, which is the property that makes
the directory — rather than any tool — the boundary.

When the index *is* present, a package whose files do not hash to what the
index records is refused, naming the file. This proves the directory is
internally consistent. It proves nothing about authenticity, and DRT should
say so in GUARANTEES.md rather than let the hash check imply more (§9).

File names above are a bikeshed; the shape is the ask.

## 5. Admission-time checks

At `drt start`, before any instance is constructed, for every package the
config names — transitively through `dependencies`:

1. Every module the manifest names resolves to a present file; hashes match
   the index where one exists.
2. `capabilities` ⊆ the deployment ceiling. A package declaring `host:sql/*`
   against a ceiling holding only `host:fs/*` fails, quoting the manifest
   line. Note this is a **subset test against the ceiling**, not attenuation
   — a package declaring less than the ceiling grants is fine.
3. `connectors` ⊆ what this build actually registers. A package needing `sql`
   in a build compiled without the feature fails by connector name, not by
   `denied` at first call.
4. `dv_abi` range contains this build's `DV_ABI_VERSION`; failure quotes both.
5. Source-only policy (§7).
6. `provides_connectors` present → refuse, by name. Reserved, not implemented.

This is the same promise SPEC.md §5 already makes for ill-scoped grants,
extended to code. It is the reason DRT — not dollup — has to read manifests:
the ceiling, the connector registry, and `DV_ABI_VERSION` are all facts only
the running process holds. Dollup can check hashes and nothing else, which is
precisely the layer split its §6 describes.

## 6. Assets, without a new noun

Assets are read through `host:fs/read` scoped to the code root's asset tree.
No new capability, no new connector, no new hostcall.

This falls straight out of the existing rule in `ConnectorWiring`'s own doc
comment: a connector is wired to a *place*, and the program names resources
inside it. The place is the code root; the program names its own asset file.
Config still never carries an application's filenames.

Two consequences worth stating:

- The operator writes **one** grant covering the code root, not one per
  package. Packages land underneath it as they are added.
- Under `Program::Set`, assets are inline bytes and there is no fs scope at
  all. The embedding library — SPEC.md's "degenerate profile" — gets assets
  with no filesystem involved, which is what the browser target will need.

## 7. Bytecode, and the four layers applied to code

GUARANTEES.md is unambiguous: the bytecode verifier does not exist, and
`DV_FLAG_TEXT_ONLY` is the mitigation. So:

```rust
/// Accept precompiled chunks from the code root. Default false — DRT sets
/// DV_FLAG_TEXT_ONLY. Setting it true is the same class of act as granting
/// `exec`: a deliberate, loud step out from under a stated guarantee.
pub accept_bytecode: bool,
```

The package manifest records whether each module is source or a chunk, so a
source-only deployment fails at admission naming the module, rather than
discovering it at load. That gives the user's "it could be bytecode" case a
home without weakening anything: **running bytecode becomes an operator act
in exactly the way granting a capability is.** The four-layer model in
dollup's §6 turns out to describe code as well as capabilities, which is a
good sign the split is the right one.

## 8. The command line, and code from Diluvium

The merge story in SPEC.md §5 — file + flags + env into one root object —
already covers the CLI. It needs names, not mechanism:

```
drt run --package hello            # a package from the code root
drt start --code-root ./code       # override the config's
DRT_CODE_ROOT=/srv/app/code drt start
```

**Naming code from Diluvium needs no new mechanism at all,** and this is the
one-shape rule paying out. A spawn request is an `InstanceConfig`; an
`InstanceConfig` carries a `Program`; so a program spawning a child sends
`{"package": "worker"}` and the child's code comes from the code root by
name. Nothing new is designed for it.

One question that follows, with what looks like the right answer: may a child
name a package its parent did not? **Yes** — because naming code confers
nothing. Install is inert; the child's *grants* are still attenuated from its
parent's, so a child that names a package it cannot supply capabilities for
gets a package that cannot do anything. Making the code root attenuable would
add a second control that duplicates the first and can disagree with it.

## 9. Proposed edits to DRT's SPEC.md and GUARANTEES.md

Small and localised, on purpose:

- **SPEC.md §5**, a new subsection *Code resolution*. It belongs under Config
  because it is how `program` grows up, not as a new top-level section, which
  would overstate it. Dollup gets at most one mention, in the register §9
  uses for ego-transport: a named external thing DRT consumes an interface
  from without depending on it to function.
- **SPEC.md §4**, move `dv_register_code` from the "missing" list onto the
  critical path — a code set cannot be handed over without it.
- **SPEC.md §12**, v1 cut gains: code root, admission checks, `Program::Set`.
  Seams only: store-ref lookup (§10), bytecode acceptance.
- **SPEC.md §13**, open questions gains: whether the code root is
  per-deployment or a shared read-only tree across several.
- **GUARANTEES.md**, one row: *DRT does not know where code came from.*
  Hash-matching the index proves the directory is internally consistent — not
  that it is authentic, not that anyone vouched for it. Signing is nobody's
  v1, and the guarantees doc should say so before the hash check gets read as
  implying otherwise.

## 10. Staging, so nothing blocks the current milestone

**Phase 0 — now, nearly free.** Finish the `dv_register_code` transcription.
Keep `Program` an enum and let `Path` keep its current meaning. That is the
entire phase; it is mostly a request *not* to collapse a shape.

**Phase 1 — before dollup can ship anything real.** `code_root`,
`Program::Package`, the package manifest type, the admission checks in §5.
This is the phase with the actual work in it.

**Phase 2 — additive, whenever.** `Program::Set` for the in-memory and
browser cases; `accept_bytecode`; a store-ref form
(`{"package": {"ref": "…"}}`) resolved by **lookup in a local store, never a
fetch**, failing by name with "not in the store" when absent.

**Phase 3 — the connector side (§11).** The loadable scope-type registry,
then the external `ConnectorWiring.backing`, in that order and on DRT's own
schedule — the second is gated on the wasm-component seam SPEC.md §7 already
owns.

Phase 1 is the only one dollup is waiting on, and it can land after DRT's
first core implementation is done rather than inside it.

## 10a. Dollup never writes DRT's config, in either dialect

Raised by the DRT side and worth settling here, because the answer is a
boundary rather than a mechanism.

DRT has two config dialects and they treat an unknown key oppositely. The
JSON root config is serde with no `deny_unknown_fields`, so an unknown key
is **silently ignored**. `.host.lua` **errors and names the key**
(`crates/drt/src/config.rs:214`, pinned by `tests/host_lua.rs:125`) — the C
loader's promise kept, on purpose, because a typo about to become a silent
default is the failure that loader exists to catch.

That raises an obvious worry: if dollup wrote `code_root` into a `.host.lua`
that an older DRT then read, the deployment would fail at config parse —
a crash-loop under a supervisor. **It cannot happen, because dollup does not
write DRT's config at all.** SPEC.md §1 is "install is inert; config is
authority", and §3 already says dollup never writes or manages the unit file
that runs the runtime; the root config is the same kind of object and the
same rule covers it. Dollup writes exactly three things: its own
`dollup.json`, `dollup.lock`, and the contents of the code root. The line
naming that code root is the operator's, written once; `dollup init` prints
it to paste and stops there.

So: **do not loosen `.host.lua`.** No reserved namespace, no tolerated
prefix. The strict dialect is correct, and C-host parity is a better reason
to keep it than anything here is to weaken it.

What is left is only the ordinary case of using a new key on an old binary,
and the strict dialect handles it *better* than the permissive one: an
operator who hand-writes `code_root` on an older DRT is told
`unknown key 'code_root'` and knows immediately what to upgrade. The JSON
path silently drops it, starts, serves nothing, and leaves someone debugging
why their program is not found. If either dialect deserves attention later
it is that one — and widening a permissive parser is safe whenever, so it is
not release-timed.

This is also why §8's flag and env var are load-bearing rather than
convenience: `--code-root` and `DRT_CODE_ROOT` let a deployment name a code
root **without editing a `.host.lua` at all**, which keeps the strict
dialect out of the common path entirely.

## 11. Two connector-side additions, ordered by risk

Dollup's spec has since settled that a package may carry a **capability
contract** (name, scope type, call names, shape version — pure data) and a
**host face** (a connector implementation per target, wasm component
preferred). That adds two asks here. Both are additive; neither touches the
current milestone; and they are deliberately ordered:

**First, the scope-type registry becomes loadable.** SPEC.md §5 already says
scope-types are declared per capability in a registry; a capability contract
is that registry's contents arriving from the code root instead of being
compiled in. This has no execution semantics at all, and it sharpens the
admission checks in §5 for *every* package — including the majority that
will never ship a host face. It also lets a build refuse, by name, a config
that wires a connector to a capability whose contract it has never seen.
Highest value, lowest risk; worth doing well before the second.

**Second, `ConnectorWiring.backing` accepts an external reference.** Today
it resolves a name to a compiled-in backing; it needs a form that names a
package's host face, so that writing it in root config is the operator act
that admits the implementation — layer 2 of dollup's four layers, exactly
where SPEC.md §7 already puts dynamic loading, and gated on the same
wasm-component seam §7 defers to. Until this exists, host faces placed in a
code root simply sit unreferenced, which is the correct inert state rather
than a broken one. Nothing about this ask asks §7 to move faster.

One boundary worth writing into GUARANTEES.md when this lands: **DRT never
verifies provenance — not signatures, not sources, nothing.** Dollup signs
and verifies on its side of the directory; DRT hash-checks the index for
internal consistency and stops. Splitting the duty keeps the boundary the
directory, and keeps DRT from growing a second, weaker copy of a trust
decision that belongs to whoever fills the code root.

## 12. Open, deliberately

Where the package manifest type lives (`drt-config`, so LuaCATS generation
covers artifacts too — versus a dollup crate donated upstream once DRT
actually reads it); index and manifest file names; whether the code root is
per-deployment or shared read-only across several; whether `version` in a
`PackageRef` is worth having at all when the index already pins one build.

