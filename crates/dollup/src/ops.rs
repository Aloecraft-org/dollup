//! The verbs' substance. Fetch, verify, lock, populate — and stop:
//! materializing is dollup's last act, and nothing here grants, runs, or
//! speaks the dv ABI.

use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::fs;
use std::path::Path;

use anyhow::{bail, Context, Result};
use dollup_format::identity::{code_set_identity, hash_bytes, package_identity};
use dollup_format::index::{IndexEntry, RepoIndex};
use dollup_format::lock::LockedPackage;
use dollup_format::manifest::HostAbi;
use dollup_format::source::Ref;
use dollup_format::{sign, Manifest, SourceEntry};

use crate::deployment::Deployment;
use crate::fetch::{fetch, Fetched};
use crate::store::Store;

/// The host-face materialization gates (RepoFormat.md §6).
#[derive(Debug, Clone, Copy, Default)]
pub struct HostGates {
    pub with_host: bool,
    pub with_host_native: bool,
}

impl HostGates {
    fn admits(&self, abi: HostAbi) -> bool {
        match abi {
            HostAbi::Component | HostAbi::Js => self.with_host,
            HostAbi::Native => self.with_host_native,
        }
    }
}

/// A source, fetched and admitted: index read, signature policy applied.
pub struct OpenSource {
    pub entry: SourceEntry,
    pub fetched: Fetched,
    pub index: RepoIndex,
    /// The pinned key that verified, when one did.
    pub signed_by: Option<String>,
}

/// Apply the signature policy (RepoFormat.md §8): keys present → verify or
/// die naming the source; keys absent → unsigned, fatal for network sources
/// under `require_signatures`.
pub fn open_source(entry: &SourceEntry, require_signatures: bool) -> Result<OpenSource> {
    let url = entry.url();
    // The unsigned-network refusal comes BEFORE any fetch: a source this
    // deployment will not accept is a source it does not talk to.
    if entry.keys().is_empty() && require_signatures && entry.scheme()?.network() {
        bail!(
            "{url}: unsigned network source refused — this deployment sets \
             require_signatures, and the source entry pins no keys"
        );
    }
    let fetched = fetch(url)?;
    let index_bytes = fetched.index_bytes()?;
    let signed_by = if !entry.keys().is_empty() {
        let sig = fetched.sig_bytes()?.with_context(|| {
            format!("{url}: keys are pinned but the repo carries no index.json.sig")
        })?;
        let sig = String::from_utf8(sig).context("index.json.sig is not text")?;
        let key = sign::verify(entry.keys(), &sig, &index_bytes)
            .with_context(|| format!("{url}: signature verification failed"))?;
        Some(key.to_string())
    } else {
        None
    };
    let index: RepoIndex = serde_json::from_slice(&index_bytes)
        .with_context(|| format!("{url}: index.json does not parse"))?;
    if index.dollup_repo != 1 {
        bail!(
            "{url}: repo format {} is newer than this dollup",
            index.dollup_repo
        );
    }
    Ok(OpenSource {
        entry: entry.clone(),
        fetched,
        index,
        signed_by,
    })
}

/// `dollup add`: resolve a ref and its dependencies against the source
/// list, in order; fetch, hash-check, store, materialize, lock. Inert by
/// construction — files on disk are the entire effect.
pub fn add(deployment: &mut Deployment, r: &Ref, gates: HostGates) -> Result<Vec<String>> {
    let mut report = vec![];
    // The ref may pin a source; otherwise the deployment's list, in order.
    // A pinned source that matches a configured entry borrows its keys.
    let entries: Vec<SourceEntry> = match &r.source {
        Some(url) => vec![deployment
            .config
            .sources
            .iter()
            .find(|e| e.url() == url)
            .cloned()
            .unwrap_or_else(|| SourceEntry::Url(url.clone()))],
        None => deployment.config.sources.clone(),
    };
    if entries.is_empty() {
        bail!("no sources: the source list is empty and the ref names none (an empty list resolves nothing)");
    }

    let store = Store::open(&deployment.store_dir())?;
    let mut opened: Vec<OpenSource> = vec![];
    let mut queue: VecDeque<(String, Option<semver::VersionReq>)> =
        [(r.name.clone(), r.version.clone())].into();
    let mut seen: BTreeSet<String> = BTreeSet::new();

    while let Some((name, req)) = queue.pop_front() {
        if !seen.insert(name.clone()) {
            continue;
        }
        // Already locked and satisfying? Leave it: add never moves a pin it
        // was not asked to move.
        if let Some(locked) = deployment.lock.packages.get(&name) {
            if req.as_ref().is_none_or(|r| r.matches(&locked.version)) {
                report.push(format!("{name} {} already locked", locked.version));
                continue;
            }
            bail!(
                "{name} is locked at {} but {} is required; `dollup update` moves pins, `add` does not",
                locked.version,
                req.unwrap()
            );
        }

        let (source_idx, version, entry) = find(
            &entries,
            &mut opened,
            deployment.config.require_signatures,
            &name,
            req.as_ref(),
        )?;
        let source = &opened[source_idx];
        let manifest = admit(source, &name, &version, &entry)?;

        // One deployment, one meaning per capability name: the lock pins
        // name → contract identity, and a different declaration under a
        // pinned name is refused naming both definers. Never a merge.
        for (cap, decl) in &manifest.capability {
            let id = decl.contract_id();
            match deployment.lock.contracts.get(cap) {
                Some(bound) if bound.id != id => bail!(
                    "'{name}' defines capability '{cap}' with a different contract than \
                     '{}' already bound in this deployment ({} vs {}) — one deployment, \
                     one meaning per capability name",
                    bound.defined_by,
                    id,
                    bound.id
                ),
                Some(_) => {}
                None => {
                    deployment.lock.contracts.insert(
                        cap.clone(),
                        dollup_format::lock::LockedContract {
                            id,
                            defined_by: name.clone(),
                        },
                    );
                }
            }
        }

        // Fetch what the gates admit, hash-checking every blob against the
        // manifest and the manifest against the index.
        let mut materialize: BTreeMap<String, Vec<u8>> = BTreeMap::new();
        let mut skipped: Vec<String> = vec![];
        for (path, want_hash) in wanted_files(&manifest, gates, &mut skipped) {
            let rel = format!("{}/{}", entry.path, path);
            let bytes = source.fetched.read(&rel)?.with_context(|| {
                format!(
                    "{}: {rel} is named by the manifest but absent",
                    source.entry.url()
                )
            })?;
            if hash_bytes(&bytes) != want_hash {
                bail!(
                    "{}: {rel} does not match its manifest hash — refusing the package",
                    source.entry.url()
                );
            }
            store.put(&bytes)?;
            materialize.insert(path, bytes);
        }
        let manifest_bytes = serde_json::to_vec_pretty(&manifest)?;

        // Materialize: code_root/<name>/ — manifest plus admitted files.
        let pkg_dir = deployment.code_root().join(&name);
        if pkg_dir.exists() {
            fs::remove_dir_all(&pkg_dir)?;
        }
        fs::create_dir_all(&pkg_dir)?;
        write_file(&pkg_dir.join("manifest.json"), &manifest_bytes)?;
        for (path, bytes) in &materialize {
            write_file(&pkg_dir.join(path), bytes)?;
        }

        let mut locked_files: BTreeMap<_, _> = materialize
            .iter()
            .map(|(p, b)| (p.clone(), hash_bytes(b)))
            .collect();
        locked_files.insert("manifest.json".into(), store.put(&manifest_bytes)?);

        for (dep, dep_req) in &manifest.requires.packages {
            queue.push_back((dep.clone(), Some(dep_req.clone())));
        }

        report.push(format!(
            "{name} {version} ← {}{}{}",
            source.entry.url(),
            source
                .signed_by
                .as_deref()
                .map(|_| ", signed")
                .unwrap_or(", unsigned"),
            if skipped.is_empty() {
                String::new()
            } else {
                format!(
                    "; host face skipped ({}) — --with-host{} includes it",
                    skipped.join(", "),
                    if skipped.iter().any(|s| s.contains("native")) {
                        "-native"
                    } else {
                        ""
                    }
                )
            }
        ));
        deployment.lock.packages.insert(
            name,
            LockedPackage {
                version,
                source: source.entry.url().to_string(),
                commit: source.fetched.commit.clone(),
                signed_by: source.signed_by.clone(),
                package_id: entry.package_id.clone(),
                code_set: entry.code_set.clone(),
                files: locked_files,
            },
        );
    }
    deployment.save()?;
    Ok(report)
}

/// First source (in order) whose index satisfies the requirement wins.
fn find(
    entries: &[SourceEntry],
    opened: &mut Vec<OpenSource>,
    require_signatures: bool,
    name: &str,
    req: Option<&semver::VersionReq>,
) -> Result<(usize, semver::Version, IndexEntry)> {
    for (i, entry) in entries.iter().enumerate() {
        if opened.len() <= i {
            opened.push(open_source(entry, require_signatures)?);
        }
        if let Some((v, e)) = opened[i].index.select(name, req) {
            return Ok((i, v.clone(), e.clone()));
        }
    }
    bail!(
        "'{name}'{} is in none of {} source(s)",
        req.map(|r| format!(" ({r})")).unwrap_or_default(),
        entries.len()
    );
}

/// Read and admit a manifest: bytes match the index, structure checks pass,
/// identities recompute. Failures name the package and the reason.
fn admit(
    source: &OpenSource,
    name: &str,
    version: &semver::Version,
    entry: &IndexEntry,
) -> Result<Manifest> {
    let rel = format!("{}/manifest.json", entry.path);
    let bytes = source
        .fetched
        .read(&rel)?
        .with_context(|| format!("{}: index names {rel} but it is absent", source.entry.url()))?;
    if hash_bytes(&bytes) != entry.manifest {
        bail!("{name} {version}: manifest does not match the index — refusing");
    }
    let manifest: Manifest = serde_json::from_slice(&bytes)
        .with_context(|| format!("{name} {version}: manifest does not parse"))?;
    if manifest.name != name || &manifest.version != version {
        bail!(
            "{name} {version}: manifest says it is {} {} — refusing",
            manifest.name,
            manifest.version
        );
    }
    manifest
        .check()
        .with_context(|| format!("{name} {version}: manifest refused"))?;
    if package_identity(&bytes, &manifest.files) != entry.package_id {
        bail!("{name} {version}: package identity does not recompute — refusing");
    }
    if let Some(guest) = &manifest.guest {
        let code_set = code_set_identity(&guest.main, &manifest.guest_files());
        if entry.code_set.as_ref() != Some(&code_set) {
            bail!("{name} {version}: code-set identity does not recompute — refusing");
        }
    }
    Ok(manifest)
}

/// Which files the gates admit: guest and assets always; host per gate,
/// recording what was skipped so `add` prints it.
fn wanted_files(
    manifest: &Manifest,
    gates: HostGates,
    skipped: &mut Vec<String>,
) -> BTreeMap<String, dollup_format::Hash> {
    let mut host_paths: BTreeMap<&str, HostAbi> = BTreeMap::new();
    if let Some(host) = &manifest.host {
        for (triple, target) in &host.targets {
            for path in target.files.values() {
                host_paths.insert(path, target.abi);
                if !gates.admits(target.abi) {
                    let label = format!("{triple} [{}]", abi_name(target.abi));
                    if !skipped.contains(&label) {
                        skipped.push(label);
                    }
                }
            }
        }
    }
    manifest
        .files
        .iter()
        .filter(|(path, _)| match host_paths.get(path.as_str()) {
            Some(abi) => gates.admits(*abi),
            None => true,
        })
        .map(|(p, h)| (p.clone(), h.clone()))
        .collect()
}

fn abi_name(abi: HostAbi) -> &'static str {
    match abi {
        HostAbi::Component => "component",
        HostAbi::Js => "js",
        HostAbi::Native => "native",
    }
}

fn write_file(path: &Path, bytes: &[u8]) -> Result<()> {
    fs::create_dir_all(path.parent().unwrap())?;
    fs::write(path, bytes).with_context(|| format!("writing {}", path.display()))
}

/// `dollup verify`: re-hash the code root and the store against the lock.
/// Returns problems; empty is clean.
pub fn verify(deployment: &Deployment) -> Result<Vec<String>> {
    let mut problems = vec![];
    let store = Store::open(&deployment.store_dir())?;
    for (name, locked) in &deployment.lock.packages {
        for (path, want) in &locked.files {
            let on_disk = deployment.code_root().join(name).join(path);
            match fs::read(&on_disk) {
                Ok(bytes) if &hash_bytes(&bytes) == want => {}
                Ok(_) => problems.push(format!("{name}: {path} does not match the lock")),
                Err(_) => problems.push(format!("{name}: {path} is missing")),
            }
            match store.get(want) {
                Ok(Some(_)) => {}
                Ok(None) => problems.push(format!("{name}: {path} absent from the store")),
                Err(e) => problems.push(format!("{name}: {path}: {e}")),
            }
        }
    }
    for (name, locked) in &deployment.lock.snapshots {
        let on_disk = deployment
            .dir
            .join("snapshots")
            .join(format!("{name}.dvsnap"));
        match fs::read(&on_disk) {
            Ok(bytes) if hash_bytes(&bytes) == locked.state => {}
            Ok(_) => problems.push(format!("snapshot {name}: does not match the lock")),
            // A pushed-but-never-pulled snapshot has no blob file on disk;
            // the store check below still covers it.
            Err(_) => {}
        }
        if store.get(&locked.state)?.is_none() {
            problems.push(format!("snapshot {name}: state absent from the store"));
        }
    }
    Ok(problems)
}

/// `dollup gc`: sweep the store against the lock — package files and pinned
/// snapshot state both.
pub fn gc(deployment: &Deployment) -> Result<usize> {
    let keep: BTreeSet<_> = deployment
        .lock
        .packages
        .values()
        .flat_map(|p| p.files.values().cloned())
        .chain(deployment.lock.snapshots.values().map(|s| s.state.clone()))
        .collect();
    Store::open(&deployment.store_dir())?.gc(&keep)
}
