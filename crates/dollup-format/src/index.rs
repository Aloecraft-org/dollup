//! The repo index (RepoFormat.md §3): the tree is canonical, the index
//! enumerates it, and — where a signature exists — signing the index signs
//! the repo, because every artifact hashes into it.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::identity::Hash;

pub const INDEX_FILE: &str = "index.json";
pub const SIG_FILE: &str = "index.json.sig";

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RepoIndex {
    /// Format version. 1.
    pub dollup_repo: u32,
    /// The store's one hash algorithm.
    pub hash: String,
    /// Where the blob projection lives on a static mirror, relative to the
    /// repo root; absent when only the tree is published.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub blobs: Option<String>,
    /// RFC 3339. Advisory in v1 — a signed index is not fresh, only
    /// authentic (RepoFormat.md §8).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub created: Option<String>,
    pub packages: BTreeMap<String, PackageVersions>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct PackageVersions {
    pub versions: BTreeMap<semver::Version, IndexEntry>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct IndexEntry {
    /// Where the package sits in the tree, relative to the repo root.
    pub path: String,
    /// Hash of the manifest bytes: fetching by blob starts here, and the
    /// manifest's own `files` map pins the rest.
    pub manifest: Hash,
    /// Whole-package identity (RepoFormat.md §4a) — what a lockfile records.
    pub package_id: Hash,
    /// Guest-face identity — what an instance pins; absent when the package
    /// has no guest face.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub code_set: Option<Hash>,
    /// Which faces are present, so `ls`/`info` answer without a fetch and a
    /// host face can be refused before transfer, not after.
    pub faces: Vec<Face>,
    /// Has an entry module: something to run rather than something to
    /// require. In the index because "is this a program or a library" is the
    /// first thing anyone asks of a listing.
    #[serde(default)]
    pub runnable: bool,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub targets: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Face {
    Capability,
    Guest,
    Host,
}

impl RepoIndex {
    pub fn new() -> Self {
        RepoIndex {
            dollup_repo: 1,
            hash: "sha256".into(),
            blobs: None,
            created: None,
            packages: BTreeMap::new(),
        }
    }

    /// The best version satisfying a requirement (None = any).
    pub fn select(
        &self,
        name: &str,
        req: Option<&semver::VersionReq>,
    ) -> Option<(&semver::Version, &IndexEntry)> {
        self.packages
            .get(name)?
            .versions
            .iter()
            .rev()
            .find(|(v, _)| req.is_none_or(|r| r.matches(v)))
    }
}

impl Default for RepoIndex {
    fn default() -> Self {
        Self::new()
    }
}
