//! The snapshot manifest (SPEC.md §5): state, not code. The blob is opaque
//! here — dollup never speaks the dv ABI, so everything it knows about a
//! snapshot is in this envelope, and restore belongs to DRT.
//!
//! Queues are volatile and guest-declared; they are not snapshot content
//! and have no field.

use serde::{Deserialize, Serialize};

use crate::identity::Hash;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SnapshotManifest {
    /// Envelope version. 1.
    pub dollup_snapshot: u32,
    /// The state blob's hash.
    pub state: Hash,
    /// The exact code-set identity the instance was running. Restore is
    /// valid against this and nothing else — the perfect-bytecode-match
    /// rule made portable. Recorded here, enforced by DRT (THREAT-NOTES).
    pub code_set: Hash,
    /// The host identity stamp, as in `dv_snapshot`'s host arg. A stamped
    /// snapshot restores only under the same stamp; that check is DRT's.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub identity: Option<String>,
    /// Generic capability names the guest expects to exist at restore.
    /// Interface expectations, not grants; grants are re-made by the
    /// restoring host's config, by attenuation, as ever.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub capabilities: Vec<String>,
    /// RFC 3339.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub created: Option<String>,
    /// The `DV_ABI_VERSION` the blob was captured under, verbatim.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dv_abi: Option<String>,
}

impl SnapshotManifest {
    /// One plain path component — the same rule DRT's directory store
    /// applies to snapshot names, so a pulled snapshot is a valid store
    /// entry by construction.
    pub fn valid_name(name: &str) -> bool {
        !name.is_empty()
            && !name.starts_with('.')
            && name
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
    }
}
