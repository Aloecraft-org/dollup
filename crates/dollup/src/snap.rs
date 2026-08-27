//! Snapshot push/pull (SPEC.md §§5, 7, 10). Push and pull move `{manifest,
//! blob}` between a deployment and a remote; restore is DRT's verb, and
//! dollup's last act is files on disk.
//!
//! The publicity gate (§7) is checked before anything else, writability
//! included: packages are public artifacts, snapshots are live state, and
//! pushing state to any non-file remote takes an explicit acknowledgment.
//! In v1 only `file://` remotes are writable, so the gate's refusal is
//! currently followed by an unwritability refusal — the order is
//! deliberate, so the acknowledgment interface is stable before more
//! transports exist.

use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};
use dollup_format::identity::hash_bytes;
use dollup_format::lock::LockedSnapshot;
use dollup_format::source::{Ref, Scheme, Transport};
use dollup_format::SnapshotManifest;

use crate::deployment::Deployment;
use crate::fetch::fetch;
use crate::ops::{self, HostGates};
use crate::store::Store;

/// Where snapshots sit on a remote: `snapshots/<name>/manifest.json` plus
/// `snapshots/<name>/state`, hash-linked by the manifest.
fn remote_paths(name: &str) -> (String, String) {
    (
        format!("snapshots/{name}/manifest.json"),
        format!("snapshots/{name}/state"),
    )
}

/// What `push` needs beyond the blob: the code-set pin, from the lock (by
/// package name) or given outright when the snapshot came from elsewhere.
pub struct PushSpec {
    pub blob_path: PathBuf,
    pub name: Option<String>,
    pub package: Option<String>,
    pub code_set: Option<String>,
    pub identity: Option<String>,
    pub capabilities: Vec<String>,
    pub dv_abi: Option<String>,
    pub export_state: bool,
}

pub fn push(deployment: &mut Deployment, remote: &str, spec: PushSpec) -> Result<String> {
    // The publicity gate, before everything — writability included.
    let scheme = Scheme::of(remote)?;
    if scheme.network() && !spec.export_state {
        bail!(
            "{remote}: refusing to push a snapshot to a non-file remote without \
             --export-state — a snapshot blob is the instance's entire heap, and \
             secure-function scrambling is not inherited by snapshots (THREAT-NOTES.md)"
        );
    }
    let Scheme::Plain(Transport::File) = scheme else {
        bail!("{remote}: only file:// remotes are writable in v1");
    };

    let name = match &spec.name {
        Some(n) => n.clone(),
        None => spec
            .blob_path
            .file_stem()
            .context("the blob path has no file name")?
            .to_string_lossy()
            .into_owned(),
    };
    if !SnapshotManifest::valid_name(&name) {
        bail!("'{name}' is not a snapshot name: one path component, no leading dot");
    }

    let code_set = match (&spec.package, &spec.code_set) {
        (Some(pkg), None) => {
            let locked = deployment.lock.packages.get(pkg).with_context(|| {
                format!(
                    "'{pkg}' is not in this deployment's lock — the code-set pin comes from there"
                )
            })?;
            locked.code_set.clone().with_context(|| {
                format!("'{pkg}' has no guest face, so it cannot be a snapshot's code-set")
            })?
        }
        (None, Some(hash)) => dollup_format::Hash(hash.clone()),
        _ => bail!("exactly one of --package or --code-set pins the snapshot's code-set"),
    };

    let blob = fs::read(&spec.blob_path)
        .with_context(|| format!("reading {}", spec.blob_path.display()))?;
    let manifest = SnapshotManifest {
        dollup_snapshot: 1,
        state: hash_bytes(&blob),
        code_set: code_set.clone(),
        identity: spec.identity.clone(),
        capabilities: spec.capabilities.clone(),
        created: None,
        dv_abi: spec.dv_abi.clone(),
    };

    let root = PathBuf::from(remote.strip_prefix("file://").unwrap());
    let (manifest_rel, state_rel) = remote_paths(&name);
    write_atomic(&root.join(&state_rel), &blob)?;
    write_atomic(
        &root.join(&manifest_rel),
        &serde_json::to_vec_pretty(&manifest)?,
    )?;

    deployment.lock.snapshots.insert(
        name.clone(),
        LockedSnapshot {
            state: manifest.state.clone(),
            code_set,
            identity: manifest.identity.clone(),
            capabilities: manifest.capabilities.clone(),
            created: manifest.created.clone(),
            dv_abi: manifest.dv_abi.clone(),
            remote: remote.to_string(),
        },
    );
    Store::open(&deployment.store_dir())?.put(&blob)?;
    deployment.save()?;
    Ok(format!("{name} → {remote} ({})", manifest.state))
}

/// `dollup pull`: fetch the manifest, hash-check the blob, ensure the
/// pinned code-set is present — from the lock, or found in the sources by
/// its code-set identity and added — then materialize
/// `snapshots/<name>.dvsnap`, the directory-store naming DRT restores from.
pub fn pull(deployment: &mut Deployment, remote: &str, name: &str) -> Result<Vec<String>> {
    if !SnapshotManifest::valid_name(name) {
        bail!("'{name}' is not a snapshot name: one path component, no leading dot");
    }
    let mut report = vec![];
    let fetched = fetch(remote)?;
    let (manifest_rel, state_rel) = remote_paths(name);
    let manifest_bytes = fetched
        .read(&manifest_rel)?
        .with_context(|| format!("{remote}: no snapshot '{name}'"))?;
    let manifest: SnapshotManifest = serde_json::from_slice(&manifest_bytes)
        .with_context(|| format!("{remote}: snapshot '{name}' manifest does not parse"))?;
    if manifest.dollup_snapshot != 1 {
        bail!(
            "{remote}: snapshot envelope {} is newer than this dollup",
            manifest.dollup_snapshot
        );
    }
    let blob = fetched
        .read(&state_rel)?
        .with_context(|| format!("{remote}: snapshot '{name}' has a manifest but no state"))?;
    if hash_bytes(&blob) != manifest.state {
        bail!("{remote}: snapshot '{name}' state does not match its manifest — refusing");
    }

    report.extend(ensure_code_set(deployment, &manifest.code_set)?);

    Store::open(&deployment.store_dir())?.put(&blob)?;
    let dir = deployment.dir.join("snapshots");
    write_atomic(&dir.join(format!("{name}.dvsnap")), &blob)?;
    deployment.lock.snapshots.insert(
        name.to_string(),
        LockedSnapshot {
            state: manifest.state.clone(),
            code_set: manifest.code_set.clone(),
            identity: manifest.identity.clone(),
            capabilities: manifest.capabilities.clone(),
            created: manifest.created.clone(),
            dv_abi: manifest.dv_abi.clone(),
            remote: remote.to_string(),
        },
    );
    deployment.save()?;
    report.push(format!(
        "{name} ← {remote}; restore is DRT's verb — point its snapshot store at {}",
        dir.display()
    ));
    Ok(report)
}

/// The code-set pin, satisfied: already locked, or found in the sources by
/// identity (the index carries `code_set` for exactly this) and added.
/// Failing names the hash, so the operator knows what to publish.
fn ensure_code_set(
    deployment: &mut Deployment,
    code_set: &dollup_format::Hash,
) -> Result<Vec<String>> {
    if deployment
        .lock
        .packages
        .values()
        .any(|p| p.code_set.as_ref() == Some(code_set))
    {
        return Ok(vec![]);
    }
    let sources = deployment.config.sources.clone();
    for entry in &sources {
        let opened = ops::open_source(entry, deployment.config.require_signatures)?;
        for (pkg_name, versions) in &opened.index.packages {
            for (version, e) in &versions.versions {
                if e.code_set.as_ref() == Some(code_set) {
                    let r = Ref {
                        source: Some(entry.url().to_string()),
                        name: pkg_name.clone(),
                        version: Some(format!("={version}").parse()?),
                    };
                    return ops::add(deployment, &r, HostGates::default());
                }
            }
        }
    }
    bail!(
        "no locked package and no source carries code-set {code_set} — the snapshot's \
         exact code must be published somewhere this deployment can see before it can restore"
    );
}

fn write_atomic(path: &Path, bytes: &[u8]) -> Result<()> {
    let parent = path.parent().context("no parent")?;
    fs::create_dir_all(parent)?;
    let tmp = tempfile::NamedTempFile::new_in(parent)?;
    fs::write(tmp.path(), bytes)?;
    tmp.persist(path)
        .with_context(|| format!("persisting {}", path.display()))?;
    Ok(())
}
