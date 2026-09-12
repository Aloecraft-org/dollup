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
    /// The license this package is published under, as an SPDX expression
    /// (`Apache-2.0`, `MIT OR Apache-2.0`). Required to publish — `repo
    /// seal` and `repo index` refuse a package without one — and carried
    /// into the index and the lock, so `ls` and `info` answer without a
    /// fetch. Optional to *read*: a package published before the field
    /// existed still resolves, and is reported as declaring none.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub license: Option<String>,
    /// Contracts this package defines: capability name → declaration. Pure
    /// data; the face a guest and a host are both checked against.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub capability: BTreeMap<String, CapabilityDecl>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub guest: Option<Guest>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub host: Option<Host>,
    /// A starting point rather than a dependency: `dollup new` copies it
    /// into your app and does **not** lock it, because you are meant to edit
    /// it and a locked file you edit is a `verify` failure. This is also the
    /// one shape that may carry config — copying a file into a directory you
    /// own is not installing config into a running app, so "config is
    /// authority" survives intact.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub template: bool,
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
    /// Connectors the host build must carry.
    #[serde(default, skip_serializing_if = "ConnectorReq::is_empty")]
    pub connectors: ConnectorReq,
    /// Package name → version requirement. Hashes land in the lock.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub packages: BTreeMap<String, semver::VersionReq>,
    /// The diluvium revision this package needs, as the 40-hex git revision
    /// `drt buildinfo` reports. Deliberately not a version requirement: the
    /// core exposes no version string at runtime, and the released spelling
    /// (`5.5.1_build12p1`) is not semver — the semver-shaped form puts the
    /// build in metadata, which precedence comparison *ignores*, so
    /// `>=5.5.1` cannot tell `build12` from `build12p1`. That is precisely
    /// the distinction anyone asks this field about. A package that wants
    /// "any core I can run against" wants `dv_abi` below, not this field:
    /// that one is a range, and it is the one DRT compares at start
    /// (CodeResolution.md §5). This is recorded for a human to read.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub diluvium: Option<String>,
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
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(untagged)]
pub enum AbiReq {
    Exact(u32),
    Range {
        min: u32,
        #[serde(skip_serializing_if = "Option::is_none")]
        max: Option<u32>,
    },
}

/// Deserialized by hand so the refusal reads like the others. The untagged
/// derive answers `">=1, <2"` — the spelling a publisher reaches for first,
/// and the one RepoFormat.md itself used to show — with `data did not match
/// any variant of untagged enum AbiReq`, which names a Rust type and nothing
/// anyone can act on. This cannot be a `ManifestError`: it fails while the
/// manifest is being parsed, before `check` is handed anything, so the
/// deserializer is the only place the guidance can live.
impl<'de> Deserialize<'de> for AbiReq {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        d.deserialize_any(AbiReqVisitor)
    }
}

struct AbiReqVisitor;

impl<'de> serde::de::Visitor<'de> for AbiReqVisitor {
    type Value = AbiReq;

    fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        f.write_str(
            "`requires.dv_abi` as an integer (`1`) or a range \
             (`{\"min\": 1, \"max\": 2}`, `max` optional)",
        )
    }

    fn visit_str<E: serde::de::Error>(self, v: &str) -> Result<AbiReq, E> {
        Err(E::custom(format!(
            "requires.dv_abi '{v}' is not a version requirement: \
             `DV_ABI_VERSION` is an integer — `drt buildinfo` reports \
             `dv_abi: 1` — so write `1` for exactly that one, or \
             `{{\"min\": 1, \"max\": 2}}` for a range, `max` optional for \
             open-ended. A semver range here would describe a version scheme \
             that does not exist"
        )))
    }

    fn visit_u64<E: serde::de::Error>(self, v: u64) -> Result<AbiReq, E> {
        u32::try_from(v)
            .map(AbiReq::Exact)
            .map_err(|_| out_of_range(v))
    }

    fn visit_i64<E: serde::de::Error>(self, v: i64) -> Result<AbiReq, E> {
        u32::try_from(v)
            .map(AbiReq::Exact)
            .map_err(|_| out_of_range(v))
    }

    fn visit_map<A: serde::de::MapAccess<'de>>(self, mut map: A) -> Result<AbiReq, A::Error> {
        use serde::de::Error;
        let (mut min, mut max) = (None, None);
        while let Some(key) = map.next_key::<String>()? {
            match key.as_str() {
                "min" if min.is_some() => return Err(A::Error::duplicate_field("min")),
                "max" if max.is_some() => return Err(A::Error::duplicate_field("max")),
                "min" => min = Some(map.next_value()?),
                // Absent and null both mean open-ended, as the derive took them.
                "max" => max = Some(map.next_value::<Option<u32>>()?),
                other => {
                    return Err(A::Error::custom(format!(
                        "requires.dv_abi has no field '{other}': a range is \
                         `{{\"min\": 1, \"max\": 2}}`, `max` optional for \
                         open-ended"
                    )))
                }
            }
        }
        let Some(min) = min else {
            return Err(A::Error::custom(
                "requires.dv_abi names a range with no `min`: write \
                 `{\"min\": 1}` for open-ended, or `1` for exactly one \
                 version",
            ));
        };
        Ok(AbiReq::Range {
            min,
            max: max.flatten(),
        })
    }
}

fn out_of_range<E: serde::de::Error, V: std::fmt::Display>(v: V) -> E {
    E::custom(format!(
        "requires.dv_abi '{v}' is out of range: `DV_ABI_VERSION` is a small \
         non-negative integer, and `drt buildinfo` reports today's as \
         `dv_abi: 1`"
    ))
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

        // An omitted `max` and an explicit null are the same open-ended range,
        // as the derive took them before this was written by hand.
        let nulled: AbiReq = serde_json::from_str(r#"{"min":2,"max":null}"#).unwrap();
        assert_eq!(nulled, open);
    }

    #[test]
    fn the_written_shapes_survive_a_round_trip() {
        // Identity is the hash of a manifest's canonical JSON, so what these
        // deserialize from has to be what they serialize back to.
        for src in ["1", r#"{"min":1}"#, r#"{"min":1,"max":2}"#] {
            let req: AbiReq = serde_json::from_str(src).unwrap();
            assert_eq!(serde_json::to_string(&req).unwrap(), src);
        }
    }

    #[test]
    fn a_misspelled_requirement_is_told_what_to_write() {
        // `">=1, <2"` is the spelling a publisher reaches for, and the one the
        // repo's own example documented; the untagged derive answered it with
        // the name of a Rust type.
        let err = serde_json::from_str::<AbiReq>(r#"">=1, <2""#)
            .unwrap_err()
            .to_string();
        assert!(err.contains("requires.dv_abi '>=1, <2'"), "{err}");
        assert!(err.contains(r#"{"min": 1, "max": 2}"#), "{err}");
        assert!(!err.contains("untagged"), "{err}");

        // Every other way of missing names the field and the shape too.
        for (src, want) in [
            (r#"{"max":2}"#, "no `min`"),
            (r#"{"min":1,"maximum":2}"#, "has no field 'maximum'"),
            ("-1", "out of range"),
            ("true", "`requires.dv_abi` as an integer"),
        ] {
            let err = serde_json::from_str::<AbiReq>(src).unwrap_err().to_string();
            assert!(err.contains(want), "{src}: {err}");
        }
    }
}

/// What a package may say about connectors. Names are checkable today —
/// `drt buildinfo` reports them. Call-shape ranges are not, so the shape is
/// reserved and refused rather than accepted and ignored: admitting a
/// package on a constraint nobody evaluates is the same lie as a version
/// field nothing can check. When hosts report shapes the refusal lifts and
/// no format changes.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ConnectorReq {
    Names(Vec<String>),
    Versioned(BTreeMap<String, semver::VersionReq>),
}

impl Default for ConnectorReq {
    fn default() -> Self {
        ConnectorReq::Names(Vec::new())
    }
}

impl ConnectorReq {
    pub fn is_empty(&self) -> bool {
        match self {
            ConnectorReq::Names(n) => n.is_empty(),
            ConnectorReq::Versioned(m) => m.is_empty(),
        }
    }

    /// The names, whichever form was written.
    pub fn names(&self) -> Vec<&str> {
        match self {
            ConnectorReq::Names(n) => n.iter().map(String::as_str).collect(),
            ConnectorReq::Versioned(m) => m.keys().map(String::as_str).collect(),
        }
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
    #[error("{}", crate::reserved::refusal(.name, .reserved))]
    ReservedName {
        name: String,
        reserved: &'static str,
    },
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
    #[error("module '{name}' is '{path}', which is not a module file (.dlua, .lua, or .dluac)")]
    NotAModuleFile { name: String, path: String },
    /// The loader's own rule, applied where the package is made: a module
    /// no `require` could name is refused at seal, not discovered at load.
    #[error("module '{name}' cannot be required: {why}")]
    ModuleName { name: String, why: String },
    #[error(
        "requires.connectors states call-shape versions ({0}), and connector \
         versions are not reported by any host yet — name the connectors \
         instead, as a list"
    )]
    ConnectorVersionsUncheckable(String),
    #[error(
        "requires.diluvium '{0}' is not a revision: expected the 40-hex git \
         revision `drt buildinfo` reports (e.g. 850e00d73220…). For \"any \
         compatible core\" rather than one exact build, use `dv_abi` instead \
         — `drt buildinfo` reports that too"
    )]
    DiluviumNotARevision(String),
    #[error(
        "no license: a published package states the license it is under, as an SPDX \
         expression (\"license\": \"Apache-2.0\")"
    )]
    NoLicense,
    #[error(
        "license {0:?} is not an SPDX expression: one line of printable ASCII with no \
         surrounding whitespace"
    )]
    BadLicense(String),
}

impl Manifest {
    /// What a publisher's tool asks on top of [`Manifest::check`]: the
    /// metadata a package must state to be published at all. A license is
    /// one. Admission at `pull` asks only `check`, so a package published
    /// before a field existed still resolves.
    pub fn check_for_publish(&self) -> Result<(), ManifestError> {
        self.check()?;
        if self.license.is_none() {
            return Err(ManifestError::NoLicense);
        }
        Ok(())
    }

    /// Internal consistency: every path a face names is in `files`, the
    /// entry module exists, provides are declared. Cheap, offline, and run
    /// at publish and at add — failures are admission failures, by name.
    pub fn check(&self) -> Result<(), ManifestError> {
        // First, because it is the one failure no edit to the rest of the
        // manifest can fix: the name itself is the problem.
        if let Some(reserved) = crate::reserved::reserved(&self.name) {
            return Err(ManifestError::ReservedName {
                name: self.name.clone(),
                reserved,
            });
        }
        // Shape only: one line of printable ASCII, nothing around it. The
        // SPDX grammar is not checked, because a wrong guess at it would
        // refuse a valid expression, and a wrong license is a human's call.
        if let Some(license) = &self.license {
            let one_line = !license.is_empty()
                && license.trim() == license
                && license.chars().all(|c| c.is_ascii_graphic() || c == ' ');
            if !one_line {
                return Err(ManifestError::BadLicense(license.clone()));
            }
        }
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
                // The name is what `require` will say, and the file is what
                // answers: both checked by the loader's rule (drt-config's
                // `modules`), so a package dollup admits is one the loader
                // can reach every module of.
                if let Some(why) = drt_config::modules::refuse_name(module) {
                    return Err(ManifestError::ModuleName {
                        name: module.clone(),
                        why,
                    });
                }
                match drt_config::modules::module_extension(path) {
                    None => {
                        return Err(ManifestError::NotAModuleFile {
                            name: module.clone(),
                            path: path.clone(),
                        })
                    }
                    Some(ext)
                        if guest.source_only && ext == drt_config::modules::BYTECODE_EXTENSION =>
                    {
                        return Err(ManifestError::NotSource(path.clone()));
                    }
                    Some(_) => {}
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
        if let ConnectorReq::Versioned(m) = &self.requires.connectors {
            if !m.is_empty() {
                return Err(ManifestError::ConnectorVersionsUncheckable(
                    m.keys().cloned().collect::<Vec<_>>().join(", "),
                ));
            }
        }
        if let Some(rev) = &self.requires.diluvium {
            if rev.len() != 40 || !rev.chars().all(|c| c.is_ascii_hexdigit()) {
                return Err(ManifestError::DiluviumNotARevision(rev.clone()));
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

#[cfg(test)]
mod check_tests {
    use super::*;

    #[test]
    fn a_reserved_name_is_refused_before_anything_else_is_looked_at() {
        // Nothing else about this manifest is wrong, so the name is the
        // only thing the refusal can be about.
        let m: Manifest = serde_json::from_str(r#"{"name": "Live", "version": "0.1.0"}"#).unwrap();
        let err = m.check().unwrap_err();
        assert_eq!(
            err,
            ManifestError::ReservedName {
                name: "Live".into(),
                reserved: "live",
            }
        );
        let msg = err.to_string();
        assert!(msg.contains("'Live' is a reserved name"), "{msg}");
        assert!(
            msg.contains("drt, init, live, log, profile, state"),
            "{msg}"
        );

        let m: Manifest =
            serde_json::from_str(r#"{"name": "lively", "version": "0.1.0"}"#).unwrap();
        assert_eq!(m.check(), Ok(()));
    }
}

#[cfg(test)]
mod module_tests {
    use super::*;

    fn manifest(name: &str, path: &str) -> Manifest {
        serde_json::from_str(&format!(
            r#"{{"name": "lib", "version": "0.1.0",
                 "guest": {{ "modules": {{ "{name}": "{path}" }} }},
                 "files": {{ "{path}": "sha256:00" }} }}"#
        ))
        .unwrap()
    }

    #[test]
    fn a_module_the_loader_could_not_name_is_refused_at_seal() {
        // The loader's rule, applied here: the reserved component, a bad
        // character, and a file `require` could never answer with.
        let err = manifest("stdlib.x", "x.dlua").check().unwrap_err();
        assert!(matches!(err, ManifestError::ModuleName { .. }), "{err}");
        assert!(err.to_string().contains("`stdlib` is reserved"), "{err}");
        let err = manifest("has space", "x.dlua").check().unwrap_err();
        assert!(matches!(err, ManifestError::ModuleName { .. }), "{err}");
        let err = manifest("cfg", "config.json").check().unwrap_err();
        assert!(matches!(err, ManifestError::NotAModuleFile { .. }), "{err}");
        // `.lua` is a module file, and `.dluac` is refused only while the
        // package says source_only, which is the default.
        assert_eq!(manifest("util.enc", "guest/enc.lua").check(), Ok(()));
        let err = manifest("util.enc", "guest/enc.dluac").check().unwrap_err();
        assert!(matches!(err, ManifestError::NotSource(_)), "{err}");
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
