//! The lockfile (SPEC.md §3): per deployment, the reproducibility artifact.
//! Records what was resolved, from where, to which identities — for
//! packages and pinned snapshots both (snapshot rows land with the snapshot
//! verbs).

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::identity::Hash;

pub const LOCK_FILE: &str = "dollup.lock";

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Lockfile {
    #[serde(default)]
    pub packages: BTreeMap<String, LockedPackage>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub snapshots: BTreeMap<String, LockedSnapshot>,
    /// Capability name → contract identity (the hash of its declaration).
    /// Capability names are a global namespace and this is the deployment's
    /// binding: the first definer pins the name, an identical declaration
    /// from anywhere hashes the same and passes, and a *different*
    /// declaration under a pinned name is refused by name — one deployment,
    /// one meaning per capability.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub contracts: BTreeMap<String, LockedContract>,
}

/// Which declaration a capability name is bound to, and who bound it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LockedContract {
    /// Hash of the declaration's canonical JSON.
    pub id: Hash,
    /// The package that bound it (for naming in a refusal).
    pub defined_by: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LockedPackage {
    pub version: semver::Version,
    /// The source URL it resolved from — provenance for humans; identity is
    /// the hashes below.
    pub source: String,
    /// For git sources, the commit the mutable input resolved to. Never a
    /// pin; the pin is `package_id`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub commit: Option<String>,
    /// Whether the index that named this package verified against a pinned
    /// key, and which. Recorded so `dollup ls` can say it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub signed_by: Option<String>,
    pub package_id: Hash,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub code_set: Option<Hash>,
    /// Path → hash for everything materialized; what `verify` re-checks.
    pub files: BTreeMap<String, Hash>,
}

/// A pinned snapshot: everything the manifest carried, plus where it came
/// from — the lock is the record, and a manifest can be re-emitted from it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LockedSnapshot {
    pub state: Hash,
    pub code_set: Hash,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub identity: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub capabilities: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub created: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dv_abi: Option<String>,
    /// The remote it was pulled from (or pushed to, when pushing pinned it).
    pub remote: String,
}
