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

## 8. Still open

Signing — unchanged in status and more pointed now that host faces exist:
v1's trust statement remains "you chose the source, and the bytes matched the
index." Namespacing. Whether `blobs/` should be the only published form on a
static mirror, dropping the tree. Whether a capability face should be allowed
to declare a scope *type* that no shipped scope validator implements, or
whether that must fail at admission.
