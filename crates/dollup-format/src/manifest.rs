//! The package manifest (RepoFormat.md §5): up to three faces, any subset
//! legal, all declarative. Requirements carry generic capability names and
//! version ranges — never scopes, never anything executable.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::identity::Hash;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Manifest {
    pub name: String,
    pub version: semver::Version,
    /// Contracts this package defines: capability name → declaration. Pure
    /// data; the face a guest and a host are both checked against.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub capability: BTreeMap<String, CapabilityDecl>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub guest: Option<Guest>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub host: Option<Host>,
    /// Asset name → path. Not code; reached through an fs scope the
    /// deployment grants, never through code loading.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub assets: BTreeMap<String, String>,
    #[serde(default, skip_serializing_if = "Requires::is_empty")]
    pub requires: Requires,
    /// Path → hash: the identity input. Every file the package ships,
    /// including host faces and assets. Written by `dollup repo seal`; an
    /// unsealed manifest has none, and `check` refuses one whose faces name
    /// files it does not list.
    #[serde(default)]
    pub files: BTreeMap<String, Hash>,
}

/// A capability contract: names and a shape number, deliberately not
/// schemas. Argument types live in the connector's own code; a manifest
/// restating them would duplicate the truth and drift from it. Names plus a
/// version are enough to check that a connector registers what it claimed,
/// that a guest calls only what exists, and to fail by name.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CapabilityDecl {
    /// The scope type an operator must supply when wiring this capability
    /// (a directory, an interface, a key…) — named, not defined, here.
    pub scope_type: String,
    pub calls: Vec<String>,
    pub shape: u32,
}

/// The guest face: `.dlua`/`.lua` modules, handed to an instance at
/// construction. `source_only` lives here and not at top level because it is
/// a claim about diluvium chunks alone — a host face is binary by
/// definition, and a top-level flag would let the faces contradict.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Guest {
    /// The entry module, when this package is meant to be *run*. Absent
    /// means a library: modules another package requires, with no entry of
    /// its own. Users will read a package with dependencies and a version as
    /// a library whatever we call it, so the format says which it is rather
    /// than making every package claim an entry point it may not have.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub main: Option<String>,
    /// Module name → path within the package.
    pub modules: BTreeMap<String, String>,
    #[serde(default = "default_true")]
    pub source_only: bool,
}

/// The host face: connector implementations per target.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Host {
    /// Capability names this implementation provides; each must be declared
    /// in this package's `capability` map or required from another.
    pub provides: Vec<String>,
    /// Rust target triple → build.
    pub targets: BTreeMap<String, HostTarget>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HostTarget {
    pub abi: HostAbi,
    /// Role → path within the package (`module`, `glue`, …).
    pub files: BTreeMap<String, String>,
}

/// The materialization gates key off this (RepoFormat.md §6): `component`
/// behind `--with-host`, `native` additionally behind `--with-host-native`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum HostAbi {
    /// A wasm component — the preferred target; sandboxable.
    Component,
    /// A browser wasm module plus JS glue.
    Js,
    /// A native shared object. Installing one is the same class of act as
    /// `apt install`; nothing the runtime holds can bound it.
    Native,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Requires {
    /// Generic capability names the host must offer. No scopes, ever.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub capabilities: Vec<String>,
    /// Connector name → call-shape version requirement.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub connectors: BTreeMap<String, semver::VersionReq>,
    /// Package name → version requirement. Hashes land in the lock.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub packages: BTreeMap<String, semver::VersionReq>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub diluvium: Option<semver::VersionReq>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dv_abi: Option<AbiReq>,
}

/// Which `DV_ABI_VERSION` a package accepts. DRT's ABI version is an
/// **integer** — `drt buildinfo` reports `dv_abi: 1` — so this is an integer
/// range and not a semver requirement: a package declaring `">=1, <2"` would
/// be describing a version scheme that does not exist.
///
/// Spelled `"dv_abi": 1` for exactly one, or `{"min": 1, "max": 2}` for a
/// range whose `max` is inclusive and may be omitted for open-ended.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum AbiReq {
    Exact(u32),
    Range {
        min: u32,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        max: Option<u32>,
    },
}

impl AbiReq {
    /// Whether a host speaking `abi` satisfies this package.
    pub fn accepts(&self, abi: u32) -> bool {
        match self {
            AbiReq::Exact(v) => *v == abi,
            AbiReq::Range { min, max } => abi >= *min && max.is_none_or(|m| abi <= m),
        }
    }
}

#[cfg(test)]
mod abi_tests {
    use super::AbiReq;

    #[test]
    fn abi_requirements_are_integer_ranges() {
        // The shapes a manifest may write, parsed as a manifest would.
        let exact: AbiReq = serde_json::from_str("1").unwrap();
        assert!(exact.accepts(1) && !exact.accepts(2));

        let bounded: AbiReq = serde_json::from_str(r#"{"min":1,"max":2}"#).unwrap();
        assert!(bounded.accepts(1) && bounded.accepts(2) && !bounded.accepts(3));

        let open: AbiReq = serde_json::from_str(r#"{"min":2}"#).unwrap();
        assert!(!open.accepts(1) && open.accepts(9));
    }
}

impl Requires {
    pub fn is_empty(&self) -> bool {
        self.capabilities.is_empty()
            && self.connectors.is_empty()
            && self.packages.is_empty()
            && self.diluvium.is_none()
            && self.dv_abi.is_none()
    }
}

fn default_true() -> bool {
    true
}

/// A structural check failure, quoting what a caller needs to name it.
#[derive(Debug, thiserror::Error, PartialEq)]
pub enum ManifestError {
    #[error("guest entry module '{0}' is not in `modules`")]
    MainNotAModule(String),
    #[error("{role} '{path}' (for {owner}) is not listed in `files`")]
    UnlistedFile {
        role: &'static str,
        path: String,
        owner: String,
    },
    #[error("host face provides '{0}' but no capability declaration is present or required")]
    ProvidesUndeclared(String),
    #[error("guest face is marked source_only but '{0}' does not end in .dlua or .lua")]
    NotSource(String),
}

impl Manifest {
    /// Internal consistency: every path a face names is in `files`, the
    /// entry module exists, provides are declared. Cheap, offline, and run
    /// at publish and at add — failures are admission failures, by name.
    pub fn check(&self) -> Result<(), ManifestError> {
        if let Some(guest) = &self.guest {
            if let Some(main) = &guest.main {
                if !guest.modules.contains_key(main) {
                    return Err(ManifestError::MainNotAModule(main.clone()));
                }
            }
            for (module, path) in &guest.modules {
                if !self.files.contains_key(path) {
                    return Err(ManifestError::UnlistedFile {
                        role: "guest module",
                        path: path.clone(),
                        owner: format!("module '{module}'"),
                    });
                }
                if guest.source_only && !(path.ends_with(".dlua") || path.ends_with(".lua")) {
                    return Err(ManifestError::NotSource(path.clone()));
                }
            }
        }
        if let Some(host) = &self.host {
            for cap in &host.provides {
                let declared = self.capability.contains_key(cap)
                    || self.requires.capabilities.iter().any(|c| c == cap);
                if !declared {
                    return Err(ManifestError::ProvidesUndeclared(cap.clone()));
                }
            }
            for (triple, target) in &host.targets {
                for (role, path) in &target.files {
                    if !self.files.contains_key(path) {
                        return Err(ManifestError::UnlistedFile {
                            role: "host file",
                            path: path.clone(),
                            owner: format!("{role} for {triple}"),
                        });
                    }
                }
            }
        }
        for (name, path) in &self.assets {
            if !self.files.contains_key(path) {
                return Err(ManifestError::UnlistedFile {
                    role: "asset",
                    path: path.clone(),
                    owner: format!("asset '{name}'"),
                });
            }
        }
        Ok(())
    }

    /// The guest-face file set: path → hash, for the code-set identity.
    pub fn guest_files(&self) -> BTreeMap<String, Hash> {
        let Some(guest) = &self.guest else {
            return BTreeMap::new();
        };
        guest
            .modules
            .values()
            .filter_map(|path| Some((path.clone(), self.files.get(path)?.clone())))
            .collect()
    }
}

impl CapabilityDecl {
    /// The contract's identity: the hash of its canonical JSON (field order
    /// is the struct's, fixed). Two vendored copies of one contract hash
    /// identically; any semantic difference — a call added, the shape
    /// bumped, the scope type changed — is a different contract.
    pub fn contract_id(&self) -> crate::Hash {
        crate::hash_bytes(&serde_json::to_vec(self).expect("decl serializes"))
    }
}
