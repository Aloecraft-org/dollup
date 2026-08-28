# Threat notes

**Status:** maintained from day one (SPEC.md §11), same convention as DRT's
GUARANTEES.md: this list only grows deliberately, entries are worded to be
quotable verbatim in a security conversation, and nothing becomes a
commitment by default. What is stated as *not* checked is as normative as
what is.

## What dollup checks, exhaustively

Hashes, structure, and — where keys are pinned — one signature. That is the
whole list. Fetched bytes must hash to what the manifest names; the manifest
must hash to what the index names; identities must recompute; the manifest's
internal structure must cohere (every named file listed, entry module
present, provides declared); and a pinned key must verify the index. A
failure of any of these refuses the artifact, by name.

Dollup does not sandbox, does not execute, does not grant, and does not
inspect content beyond hashing it. A `.dlua` file full of malicious source
that hashes correctly installs correctly; whether it can *do* anything is
decided entirely by the deployment's config and DRT's capability model.

## Install is inert — and the claim's exact size

Materializing files is dollup's entire effect. Nothing runs at install time,
there are no hooks, and the manifest cannot express behavior. But "inert"
describes the install act, not the disk afterward: a package sits in the
code root, and whatever later *names* it (root config, a spawn request)
activates it under that config's grants. The control is the config; dollup
neither strengthens nor weakens it.

## The signature proves authenticity, not freshness

A verified index means: a holder of a key pinned in your source list signed
exactly these bytes. It does not mean the bytes are current — a mirror can
serve yesterday's correctly-signed index and dollup cannot tell (the
rollback/freeze attacks TUF exists to address). There is no revocation. Keys
are pinned per source entry; compromise of a pinned key is compromise of
that source until the operator edits the line.

## An unsigned source is exactly as trustworthy as its transport

With no keys pinned, "the bytes matched the index" is the entire integrity
statement, and the index came over the same transport as the bytes. For
`file://` that means trusting the filesystem, which is already trusted; for
network sources this deployment-level posture is refused by default
(`require_signatures`) and disabling it is an explicit operator edit.

## `source_only` is a mitigation, not a verifier

Diluvium has no bytecode verifier; treat untrusted bytecode as untrusted
native code (DRT GUARANTEES.md). `guest.source_only` refuses precompiled
chunks at publish and at add, which mitigates by keeping input in the class
the parser checks. It does not make hostile source safe — the capability
model does, at run time, to the extent config says so.

## Contract pinning is per-deployment coherence, not global truth

The lockfile binds each capability name to one contract identity, so within
a deployment a name cannot quietly mean two things. That is the whole
claim. It does not make a name mean the *right* thing — the first package
added binds it, and choosing trustworthy sources is still where that trust
comes from. Two deployments can bind the same name to different contracts
and both are internally coherent; interoperability between them is a
publishing discipline (share the interface package), not something dollup
enforces.

## A native host face is `apt install`

A wasm host face will run in a sandbox once DRT loads components. A
**native** host face is a shared object with the host's privileges: not
bounded by the capability model, not bounded by the instruction budget, not
inspected by dollup beyond its hash. The `--with-host-native` gate exists so
placing one is a conscious act, and the hash proves provenance only as far
as the signature above proves the index. Nothing here is a sandbox.

## The code-set pin is recorded here, enforced elsewhere

Dollup records a snapshot's code-set identity and refuses to *materialize* a
mismatch. Whether a restore actually happens against the pinned code is
DRT's enforcement (today: engine header/build checks, not a code-set hash).
Until DRT checks the pin itself, a hand-arranged directory can present a
sleeping agent with different code than it hibernated under.

## Snapshot blobs are secret-bearing

A snapshot is an instance's whole heap. Secure-function scrambling is not
inherited by snapshots; anything the instance held in memory is in the blob.
Hence the publicity gate (SPEC.md §7): pushing a snapshot to any non-file
remote requires explicit acknowledgment, and no repo lists snapshots.
