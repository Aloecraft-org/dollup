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

## A source that cannot be read is passed over; one that refuses is not

The source list is a fallback list, so a source that does not answer, is
not there, or answers with no index is skipped for the next one, and the
skip is printed. What that can never do is lower the bar: the source that
does answer is held to its own entry's policy — its own pinned keys, or the
`require_signatures` refusal for an unsigned network source — so blocking
one source buys an attacker nothing but a fallback the operator listed. A
source that is reached and refuses (a signature that does not verify, an
index that does not parse, a format newer than this dollup) is fatal, never
skipped: passing over a refusal would be exactly the downgrade the policy
exists to prevent.

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
Hence the publicity gate (SPEC.md §7): `dollup snapshot push` to any
non-file remote requires explicit acknowledgment, and no repo lists
snapshots.

## The cache is shared across roots, and content-addressed

Every root on a box materializes from one store, `~/.dollup/cache/store`.
A blob's name is its hash, so one root cannot hand another a different
file under the same name, and a root cannot depend on the cache at all: it
is self-contained without it, and `pull` refills one. What sharing does
change is `gc`, which keeps what every recorded root's lock references and
sweeps the rest — a root that is not on the list, or whose lock cannot be
read, is not protecting its blobs, and the sweep says so by name.

## A module path belongs to one package

A pulled package's modules land at the paths their names resolve to, by the
loader's rule (`drt_config::modules`, applied at `repo seal` and at `pull`
alike). Two locked packages naming one module is refused at pull naming
both, and a module landing on a file no locked package owns — a committed
or hand-placed one — is refused rather than overwritten. What this does not
claim: that a module's *content* is what its name suggests. Provenance is
the signature's claim, as ever, and requiring a module grants nothing the
node did not already hold.

## Reserved names are refused, not renamed

A root's layout owns six names — `drt`, `init`, `live`, `log`, `profile`,
`state` — and dollup refuses a package under any of them, in any
capitalization, at `add`, `repo seal`, and `repo index` alike. This is a
collision rule, not a security boundary: it exists so that no tool joining a
package name onto a root's directory can land on the root's own files, and
it says nothing about what a package *not* so named may do. drt refuses the
same six on its side, and both read one constant (`drt_config::project::
RESERVED`), so the two cannot drift.

## Shipping a root never carries operator or runtime state

Stated ahead of the code, as a commitment the verb is held to rather than a
property discovered afterward: the root-shipping `dollup push` is not built,
and when it is, it excludes `consent.json`, `state/`, and the directory that
holds account-populated profiles. The exclusion is **positional** — the
envelope builder refuses those paths outright — and never a field a profile
carries, because a field means push parses every profile to decide what
travels, and one forgotten field ships someone's credentials. A path list
cannot forget. The observable trace of a `consent.json` copied around by
hand is a `root_id` that does not match `project.json`, which is what
`dollup audit` will flag; dollup cannot otherwise tell a pulled root from a
local one and does not pretend to.
