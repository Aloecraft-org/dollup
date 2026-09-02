# The dollup repo format

**Status:** design, replacing §8's deferral. A dollup repo is **one format
with several transports**, and the transports are interchangeable because
identity is content. This document defines the format, the schemes, the
package manifest, and the admission rules for packages that carry host-side
code.

## 1. Why this is not deferred any more

The original spec pushed the registry to "later" on the theory that a hosted
artifact repo needs a service. It does not. Two observations remove the whole
dependency:

- **A static directory over HTTP is a complete repo.** Index plus blobs behind
  plain nginx — no server logic, no accounts, no dynamic anything.
- **A forge already serves that shape.** `https://github.com/o/r/archive/refs/
  tags/v1.0.zip` returns a whole repo tree over an ordinary GET, with no git
  binary, no API token, and no cooperation from the forge beyond what it
  already does for everyone.

So the repo format is a directory tree, and *how you get the tree* is a
scheme. Because a package's identity is the hash of its content, **the same
package fetched three different ways has one identity**, and a lockfile stays
valid across all of them. That is what makes the source list a genuine
fallback list rather than a preference: a mirror is free, an air-gapped copy
is free, and self-hosting is a base-URL edit exactly as §8 promised.

## 2. Schemes

Four, none vendor-specific:

| scheme | fetches | good for |
|---|---|---|
| `https://` | a static repo: `index.json`, then blobs by hash | the hosted mirror; incremental, cacheable |
| `zip+https://` | one archive URL, extracted and verified | any forge's zipball; zero server cooperation |
| `git+https://` | a clone or fetch | development; tags, branches, history |
| `file://` | a directory | local, mounted, rsync'd, air-gapped |

`zip+https://` takes an arbitrary URL. That is deliberate: dollup must not
grow a GitHub adapter, because the moment it knows one forge it will be asked
to know four. `zip+https://github.com/o/r/archive/refs/tags/v1.0.zip` is just
a URL that happens to come from GitHub.

Mutable inputs are accepted and never recorded. A branch zipball resolves to
content hashes; the lock stores the hashes, never the branch.

**The scaffold names two sources for the same content**, which demonstrates
the interchangeability and buys resilience for nothing:

```json
"sources": [
  "https://dollup.aloecraft.org/std-repo/",
  "zip+https://github.com/Aloecraft-org/dollup-standard/archive/refs/heads/main.zip"
]
```

Both are ordinary lines in a file the operator owns. Deleting them is the
self-hosting story; nothing resurrects them.

## 3. Repo layout

```
<repo>/
  index.json
  packages/<name>/<version>/
    manifest.json
    guest/                     # .dlua/.lua modules
    host/                      # connector implementations, per target
    assets/                    # images, json, fonts — not code
  blobs/sha256/<hash>          # static mirrors only: a generated projection
```

The **tree is canonical**; `blobs/` is a generated projection a static mirror
publishes so clients can fetch incrementally by hash. A git repo or a zipball
carries only the tree, and the client hashes it on arrival. The index records
both so a client picks whichever its scheme supports.

`index.json`:

```json
{
  "dollup_repo": 1,
  "hash": "sha256",
  "blobs": "blobs/sha256",
  "packages": {
    "can": { "versions": { "0.1.0": {
      "path": "packages/can/0.1.0",
      "manifest": "sha256:…",
      "faces": ["capability", "guest", "host"],
      "targets": ["wasm32-wasip2", "x86_64-unknown-linux-gnu"]
    } } }
  }
}
```

`faces` and `targets` sit in the index so `dollup ls` and `dollup info` answer
without fetching a package, and so a fetch can refuse a host face **before**
transferring it rather than after.

## 4. A package has up to three faces

This is the change that matters, and the reason it is being made now rather
than incrementally: the *envelope* is the expensive thing to change later, and
a one-faced envelope would harden wrong.

| face | content | bounded by |
|---|---|---|
| `capability` | the contract: capability name, scope type, call names, shape version | nothing — it is pure data |
| `guest` | `.dlua`/`.lua` modules; the ergonomic wrapper over the hostcalls | the capability model, absolutely |
| `host` | the connector implementation, per target | **nothing the runtime can enforce** |

Any subset is legal. Three combinations are worth naming:

- **capability only** — an interface package. Both a connector and a guest
  library depend on it; neither depends on the other. This is the most
  valuable face and the least dangerous one, because it has no execution
  semantics whatsoever.
- **capability + guest** — a library over a capability the host already
  offers. The ordinary case today.
- **all three** — a whole capability, both sides. The SocketCAN adapter, the
  ThreeJS binding. §6 governs it.

### 4b. Templates: the one shape that may carry config

A **template** (`"template": true`) is a starting point rather than a
dependency. `dollup new` copies its files into the app and does **not** lock
them; `dollup add` refuses it by name, and each verb points at the other.

The distinction is not cosmetic. A locked file you edit is a `verify`
failure, and a template is precisely the package you are meant to edit — so
tracking one would make the tool complain about the thing it told you to do.

It also settles something the rest of the format cannot. A drt app is a
config plus a program, and dollup never writes config, so `add` can only
ever deliver half an app. A template delivers both, and the doctrine holds
exactly: **copying a file into a directory you own is not installing config
into a running app.** Nothing is applied, nothing is merged, nothing is
re-read later — you are handed a file and it is yours from that moment.
A template's *dependencies* are ordinary packages and are added and locked
as usual, because those you did not write and are not editing.

This is also why examples do not belong in a repo but templates do. An
example teaches by the contrast between two configs and the prose around
them; strip the README and the lesson is gone. A template is the artifact
you keep after the lesson.

### 4a. Two hashes, because a package is a distribution unit and not a pinning unit

Bundling three faces in one package collides with §9 unless one thing is
stated: **a package's identity and a code-set's identity are different
hashes.**

- **Package identity** = the hash of everything the package contains. It is
  what the lockfile records and what `verify` re-checks.
- **Code-set identity** = the hash of the **guest face alone**. It is what an
  instance pins at spawn and what a snapshot restores against, because the
  guest face is the only part that is ever registered into an instance via
  `dv_register_code`. A host face lives in the host process and was never
  inside the guest.

Without this split, shipping a connector fix would bump the package version,
change the pinned code-set, and strand every sleeping agent — the exact
opposite of §9's "connector implementations upgrade freely underneath sleeping
agents". With it, §9 holds unchanged: code is exact-match on the guest face,
capabilities are interface-match on the contract, and implementations move
freely beneath both.

The alternative considered was splitting the faces into three related packages
so they version independently. Two hashes gets the same property while keeping
one envelope, which is §2's whole argument, so the faces stay together.

## 5. The manifest

```json
{
  "name": "can",
  "version": "0.1.0",

  "capability": {
    "host:can": {
      "scope_type": "interface",
      "calls": ["can/send", "can/recv", "can/filter"],
      "shape": 1
    }
  },

  "guest": {
    "main": "can",
    "modules": { "can": "guest/can.dlua" },
    "source_only": true
  },

  "host": {
    "provides": ["host:can"],
    "targets": {
      "wasm32-wasip2": {
        "abi": "component",
        "files": { "module": "host/can.wasm" }
      },
      "wasm32-unknown-unknown": {
        "abi": "js",
        "files": { "module": "host/can.wasm", "glue": "host/can.js" }
      },
      "x86_64-unknown-linux-gnu": {
        "abi": "native",
        "files": { "module": "host/libcan.so" }
      }
    }
  },

  "assets": { "logo": "assets/logo.png" },

  "requires": {
    "capabilities": ["host:time"],
    "connectors": { "time": ">=1, <2" },
    "packages": { "json": "^1.2" },
    "diluvium": ">=5.5.1",
    "dv_abi": ">=1, <2"
  },

  "files": { "guest/can.dlua": "sha256:…", "host/can.wasm": "sha256:…" }
}
```

Notes where the shape was chosen against an obvious alternative:

- **`guest.main` is optional, and its absence is the meaning.** A package
  with an entry module is something to *run*; one without is a library other
  packages require. This exists because users will read anything with a
  name, a version and dependencies as a library whatever it is called — so
  rather than argue with that reading, the format states which a package is,
  the index carries it, and `ls`/`info` say it in words. A required `main`
  forced every library to claim an entry point it did not have, which is a
  lie the tooling would then repeat.
- **`source_only` moved under `guest`.** It means "no precompiled diluvium
  chunks", which is a statement about the guest face only — a host face is
  binary by definition. Leaving it at top level would have made the two faces
  contradict each other the first time both were present.
- **A capability declares call *names* and a shape number, not schemas.** The
  argument types live in the connector's Rust, deserialized by serde; a
  manifest restating them would duplicate the truth and drift from it. Names
  plus a version are enough to check that a loaded connector registers exactly
  what it claimed, that a guest calls only what exists, and to fail by name
  when it does not. This is §9's "name plus call shape" made concrete at the
  cheapest point that still checks something.
- **Targets are Rust target triples**, because that is what everything here is
  built with, plus an `abi` tag distinguishing a wasm component from a
  browser module-with-glue from a native shared object.
- **`requires.capabilities` still carries no scopes.** Unchanged and
  load-bearing: scopes stay host-side, the operator supplies them.

## 6. Host faces: install stays inert, and here is why that still holds

A host face is native or wasm code that runs with the host's privileges. That
looks like it breaks "install is inert." It does not, and the reason is worth
stating precisely, because the whole doctrine rests on it:

**Placing a host face on disk enables nothing.** DRT loads a connector because
root config names it in the connector registry — layer 2, "a process-level
trust act by the operator", which the founding spec already required. A host
face sitting unreferenced in the code root is bytes, exactly as a guest
package with no grants is bytes. The four layers are unchanged; what changed
is only that a package may now carry material addressed to layer 2 as well as
layer 1.

But "unchanged doctrine" is not the same as "no new risk", so the tooling
takes the same asymmetry §7 takes with snapshots — one mechanism, second use:

- **A host face is not materialized by default.** `dollup add` places the
  capability and guest faces, skips the host face, and prints what it skipped
  and the flag that would include it.
- **`--with-host` includes wasm targets.** A wasm component is sandboxable and
  is where DRT's own §7 says dynamic loading is headed.
- **A native target additionally requires `--with-host-native`**, and the
  message says the true thing: installing one is the same class of act as
  `apt install`. It is not bounded by the capability model, the instruction
  budget cannot bound it, and dollup's hash check proves only that the bytes
  match the index — never that anyone vouched for them.

**No host face ever shares Rust internals with DRT — the linking reality.**
Rust has no stable ABI: a compiled artifact can only soundly expose a C-ABI
or bytes-level surface, and passing any Rust type (a runtime handle, a
`Waker`, a socket) across a dynamic boundary is defined only when both sides
came from one build. So a distributed connector never borrows the host's
tokio, and each `abi` means exactly this much:

- `component` — sharing dissolves rather than being solved: the component
  *cannot* link host internals, and imports what it needs (a byte stream, a
  clock) from host-provided interfaces the host implements once, on its own
  runtime, behind capability gating. The cost stated plainly: async-native
  crates (tokio-postgres and kin) do not compile to this target; a component
  connector is written sans-io against the host's interfaces, not ported.
- `native` — bytes dollup places, gated loudly; **never a `dlopen` into
  DRT.** What runs one is outside DRT's loader: a separate OS process
  speaking framed msgpack under config admission, carrying its own
  statically-linked runtime. Two tokios on one machine is sound when the
  boundary is bytes; one address space pretending to share one is not.

**The compiled-in set is closed, and distributed faces are the primary
growth path — not the escape hatch.** DRT's identity is a tiny installed
package; a runtime whose answer to every integration is "recompile with
another feature" converges on either one fat build or a 2^N feature matrix,
and both betray it. A connector is compiled in only when its absence would
make the runtime not-DRT (time, fs, listen, the control plane — plus
whatever the project explicitly rules core, a decision made per connector
and out loud, never by drift). Everything else is a distributed face:
component where it can be, an out-of-process runner where it must be
native, with the host-side dispatcher gating calls identically for all
three origins. A package requiring a connector this build does not carry
still fails at admission by name, whichever lane the connector was meant
to arrive by.

**One deployment, one meaning per capability name.** Capability names are a
global namespace with no registrar, so the deployment's lockfile is the
binding: a contract's identity is the hash of its declaration, and the
first definer pins `name → contract id`. An identical declaration from
anywhere else passes (vendored copies hash the same); a different
declaration under a pinned name is refused naming both definers, never
merged. `dollup info` prints a package's contracts in full — scope type,
calls, shape, contract id — before any face is fetched, because the
contract is the unit of trust review.

**Dollup may ship the distribution side before DRT ships the loading side.**
Placement is inert, so a wasm component can sit correctly in a code root that
no runtime can yet load. This is the doctrine paying for the parallel
timeline rather than costing anything.

## 7. What DRT needs for this, beyond `doc/CodeResolution.md`

Two additions, both small, both additive:

- **`ConnectorWiring.backing` accepts an external reference**, not only a name
  the build registered. Today it resolves a name to a compiled-in backing;
  it needs a form that names a package's host face, so that naming it in
  config is the operator act that admits it.
- **The scope-type registry becomes loadable.** DRT SPEC §5 already says
  "scope-types are declared per capability in a registry"; capability faces
  are that registry's contents arriving from outside instead of being
  compiled in. This is the highest-value and lowest-risk half of the whole
  design: pure data, no execution, and it makes the admission checks in
  `CodeResolution.md` §5 sharper for packages that have no host face at all.

## 8. Signing

Host faces made "you chose the source" too small a trust statement to stop
at, so signing is in v1, and it is deliberately the most boring construction
available.

**The trust anchor is the source entry, never the repo.** A source may carry
public keys:

```json
"sources": [
  { "url": "https://dollup.aloecraft.org/std-repo/",
    "keys": ["ed25519:BASE64…"] },
  { "url": "zip+https://github.com/Aloecraft-org/dollup-standard/archive/refs/heads/main.zip",
    "keys": ["ed25519:BASE64…"] }
]
```

A bare string remains a valid source and means *unsigned*. The scaffold pins
the standard key beside the standard URLs — the key is distributed exactly
the way the URLs are, as lines in a file the operator owns, and removing it
is the same one-line edit as removing them. This is also precisely the shape
a third party uses: a vendor shipping a DRT swarm that calls home
distributes `{url, keys}` for their own repo with their application, and
dollup treats them identically to the standard source. There is no key
hierarchy and no root of trust beyond the source list, on purpose.

**What is signed is the index, in-tree.** `index.json.sig` sits beside
`index.json` and contains an ed25519 signature over the exact bytes of
`index.json`, spelled `ed25519:<base64>` — the same encoding as keys, one
line, no framing to parse. Because every artifact hashes into the index,
signing the index signs the repo. Because the signature is a file in the
tree, every transport carries it for free: the zipball contains it, the git
clone contains it, the mirror serves it, and the identical signature
verifies the identical bytes over all four schemes.

**Verification policy, crisp:** keys present on the source → verification is
mandatory and failure is fatal, naming the source and the key. Keys absent →
the source is unsigned and every listing says so. A deployment-level
`require_signatures` (scaffolded **true**) makes an unsigned network source
an error at resolve time; `file://` sources are exempt, because a local
directory's trust story is the filesystem's.

**What this buys, and does not.** It buys: these bytes were indexed by a
holder of a key you pinned. It does not buy freshness — a mirror can serve
yesterday's correctly-signed index and dollup cannot tell (the rollback and
freeze attacks TUF exists to address; the index's `created` field plus a
client-side maximum age is the obvious future mitigation, not built).
It does not buy revocation, and multiple keys per source exist for rotation,
not for ceremony. The threat notes carry these limits verbatim.

Keys never appear in-tree. A repo carrying its own public key proves only
that it agrees with itself; verification reads keys from the source list and
nowhere else.

## 9. Still open

Namespacing. Whether `blobs/` should be the only published form on a static
mirror, dropping the tree. Whether a capability face may declare a scope
*type* no shipped scope validator implements, or must fail at admission.
Freshness (§8). Snapshot push signing — snapshots are private point-to-point
transfers, so it is not obviously worth having, but it is not decided.
