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

/// A source, opened: read and admitted, or passed over.
///
/// The split is the fallback rule (RepoFormat.md §1: the source list is "a
/// genuine fallback list rather than a preference"). A source that cannot
/// be *read* — the host does not answer, the path is not there, the URL
/// answers but holds no index — is passed over for the next one, and said.
/// A source that is read and *refuses* — a signature that does not verify,
/// an index that does not parse, a format newer than this dollup, an
/// unsigned network source under `require_signatures` — is fatal, because
/// passing over a refusal is exactly the downgrade the policy exists to
/// prevent (THREAT-NOTES.md).
pub enum Open {
    Ready(OpenSource),
    Skipped { url: String, why: String },
}

impl Open {
    pub fn ready(&self) -> Option<&OpenSource> {
        match self {
            Open::Ready(source) => Some(source),
            Open::Skipped { .. } => None,
        }
    }
}

/// Apply the signature policy (RepoFormat.md §8): keys present → verify or
/// die naming the source; keys absent → unsigned, fatal for network sources
/// under `require_signatures`. A source that cannot be read at all comes
/// back [`Open::Skipped`] rather than as an error — see [`Open`].
pub fn open_source(entry: &SourceEntry, require_signatures: bool) -> Result<Open> {
    let url = entry.url();
    // The unsigned-network refusal comes BEFORE any fetch: a source this
    // deployment will not accept is a source it does not talk to.
    if entry.keys().is_empty() && require_signatures && entry.scheme()?.network() {
        bail!(
            "{url}: unsigned network source refused — this deployment sets \
             require_signatures, and the source entry pins no keys"
        );
    }
    let skipped = |e: anyhow::Error| Open::Skipped {
        url: url.to_string(),
        why: format!("{e:#}"),
    };
    let fetched = match fetch(url) {
        Ok(fetched) => fetched,
        Err(e) => return Ok(skipped(e)),
    };
    let index_bytes = match fetched.index_bytes() {
        Ok(bytes) => bytes,
        Err(e) => return Ok(skipped(e)),
    };
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
    Ok(Open::Ready(OpenSource {
        entry: entry.clone(),
        fetched,
        index,
        signed_by,
    }))
}

/// Which sources a ref resolves against: the one it pins, borrowing the
/// configured entry's keys when there is one, or the deployment's list in
/// order. Empty is a refusal that says what to add.
fn entries_for(deployment: &Deployment, r: &Ref) -> Result<Vec<SourceEntry>> {
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
        bail!(
            "nothing to install from: this root has no package sources.\n\
             \n  \
             add one:  {} source add <url> --key <key>",
            crate::me()
        );
    }
    Ok(entries)
}

/// `dollup pull <ref>`: a package is fetched, locked and materialized
/// ([`add`]); a starting point — a template — is copied and never locked
/// ([`new_from_template`]). Which it is comes from the index, so the
/// manifest is read once, by whichever path runs.
pub fn pull(deployment: &mut Deployment, r: &Ref, gates: HostGates) -> Result<Vec<String>> {
    let entries = entries_for(deployment, r)?;
    let mut opened: Vec<Open> = vec![];
    // Skips are reported by whichever path runs next, which opens the
    // sources again; noting them here too would say everything twice.
    let (_, _, entry) = find(
        &entries,
        &mut opened,
        deployment.config.require_signatures,
        &r.name,
        r.version.as_ref(),
        &mut vec![],
    )?;
    if entry.template {
        new_from_template(deployment, r)
    } else {
        add(deployment, r, gates)
    }
}

/// A package: resolve a ref and its dependencies against the source list,
/// in order; fetch, hash-check, store, materialize, lock. Inert by
/// construction — files on disk are the entire effect. What `pull` runs
/// for anything that is not a starting point.
pub fn add(deployment: &mut Deployment, r: &Ref, gates: HostGates) -> Result<Vec<String>> {
    let mut report = vec![];
    let entries = entries_for(deployment, r)?;

    let store = Store::open(&deployment.store_dir()?)?;
    let mut opened: Vec<Open> = vec![];
    let mut skips: Vec<String> = vec![];
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
                "{name} is locked at {} but {} is required; `dollup update` moves pins, `pull` does not",
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
            &mut skips,
        )?;
        let source = opened[source_idx]
            .ready()
            .expect("find returns the index of a source it read");
        let manifest = admit(source, &name, &version, &entry)?;
        if manifest.template {
            // Reachable only through a dependency edge: `pull` sends a
            // requested template down the copy path before this runs.
            bail!(
                "'{name}' is a starting point, not a dependency — locking it \
                 would lock files you are meant to edit.\n\
                 \n  \
                 copy it instead:  dollup pull {name}"
            );
        }

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

        // Where each file lands in the code root. A module lands at the
        // path its name resolves to — `db.claims` at `db/claims.dlua` — so
        // the loader's walk finds it under the name the package gave it;
        // everything else a package ships (assets, host faces) sits under
        // `<name>/` as before. The manifest is not materialized: the lock
        // and the cache hold what `verify`, `ls` and `gc` need, and the
        // code root is the deployable tree and nothing else.
        let placement = placement(&manifest, &name)?;
        for (path, dest) in &placement {
            if let Some(owner) = deployment.lock.packages.iter().find_map(|(other, locked)| {
                (other != &name && locked.files.contains_key(dest)).then_some(other)
            }) {
                bail!(
                    "'{name}' would place '{path}' at {dest}, which '{owner}' already provides — \
                     one root, one file per module path"
                );
            }
            let on_disk = deployment.code_root().join(dest);
            if on_disk.exists() && !placement_owned_by(&deployment.lock, &name, dest) {
                bail!(
                    "'{name}' would place '{path}' at {}, which is already there and belongs \
                     to no locked package — a committed or hand-placed file; move it or remove it",
                    on_disk.display()
                );
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

        // Materialize. The package's own subdirectory is replaced whole, as
        // before; its module files land at their own paths.
        let pkg_dir = deployment.code_root().join(&name);
        if pkg_dir.exists() {
            fs::remove_dir_all(&pkg_dir)?;
        }
        let mut locked_files: BTreeMap<String, dollup_format::Hash> = BTreeMap::new();
        for (path, bytes) in &materialize {
            let dest = &placement[path];
            write_file(&deployment.code_root().join(dest), bytes)?;
            locked_files.insert(dest.clone(), hash_bytes(bytes));
        }
        let modules: Vec<String> = manifest
            .guest
            .as_ref()
            .map(|g| g.modules.keys().cloned().collect())
            .unwrap_or_default();
        let entry_hint = manifest
            .guest
            .as_ref()
            .and_then(|g| g.main.as_ref())
            .and_then(|main| g_dest(&manifest, main));

        for (dep, dep_req) in &manifest.requires.packages {
            queue.push_back((dep.clone(), Some(dep_req.clone())));
        }

        report.push(format!(
            "{name} {version} ← {}{}{}{}",
            source.entry.url(),
            source
                .signed_by
                .as_deref()
                .map(|_| ", signed")
                .unwrap_or(", unsigned"),
            if modules.is_empty() {
                String::new()
            } else {
                format!("; require: {}", modules.join(", "))
            },
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
        if let Some(entry) = entry_hint {
            report.push(format!("  runnable: a profile's entry \"{entry}\" runs it"));
        }
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
    skips.append(&mut report);
    Ok(skips)
}

/// First source (in order) whose index satisfies the requirement wins. A
/// source that cannot be read is passed over and named in `skips`, once;
/// when nothing satisfies, the refusal says which sources were read and
/// which were not, so a dead first source never masquerades as a missing
/// package.
fn find(
    entries: &[SourceEntry],
    opened: &mut Vec<Open>,
    require_signatures: bool,
    name: &str,
    req: Option<&semver::VersionReq>,
    skips: &mut Vec<String>,
) -> Result<(usize, semver::Version, IndexEntry)> {
    for (i, entry) in entries.iter().enumerate() {
        if opened.len() <= i {
            let open = open_source(entry, require_signatures)?;
            if let Open::Skipped { url, why } = &open {
                skips.push(format!("skipped {url}: {why}"));
            }
            opened.push(open);
        }
        let Some(source) = opened[i].ready() else {
            continue;
        };
        if let Some((v, e)) = source.index.select(name, req) {
            return Ok((i, v.clone(), e.clone()));
        }
    }
    let unread: Vec<String> = opened
        .iter()
        .filter_map(|o| match o {
            Open::Skipped { url, why } => Some(format!("{url}: {why}")),
            Open::Ready(_) => None,
        })
        .collect();
    let mut msg = format!(
        "'{name}'{} is in none of {} source(s)",
        req.map(|r| format!(" ({r})")).unwrap_or_default(),
        entries.len()
    );
    if !unread.is_empty() {
        msg.push_str(&format!(
            ", {} of which could not be read:\n  {}",
            unread.len(),
            unread.join("\n  ")
        ));
    }
    bail!(msg);
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
        let code_set = code_set_identity(guest.main.as_deref(), &manifest.guest_files());
        if entry.code_set.as_ref() != Some(&code_set) {
            bail!("{name} {version}: code-set identity does not recompute — refusing");
        }
    }
    Ok(manifest)
}

/// Where each file of a package lands, relative to the code root: package
/// path → destination. A module goes to the path its name resolves to, by
/// the loader's own rule (`drt_config::modules`); anything else goes under
/// `<name>/`. The manifest has already passed `check`, so a name here is one
/// the loader accepts and a module file has a module extension.
fn placement(manifest: &Manifest, name: &str) -> Result<BTreeMap<String, String>> {
    let mut out = BTreeMap::new();
    if let Some(guest) = &manifest.guest {
        for (module, path) in &guest.modules {
            let Some(dest) = module_dest(module, path) else {
                bail!("'{name}': module '{module}' is '{path}', which is not a module file");
            };
            if let Some(other) = out.insert(path.clone(), dest.clone()) {
                bail!(
                    "'{name}': '{path}' is named by two modules, and would land at both {other} and {dest}"
                );
            }
        }
    }
    for path in manifest.files.keys() {
        out.entry(path.clone())
            .or_insert_with(|| format!("{name}/{path}"));
    }
    Ok(out)
}

/// `db.claims` in `guest/claims.dlua` lands at `db/claims.dlua`: the name's
/// path, with the file's own extension — bytecode stays bytecode.
fn module_dest(module: &str, path: &str) -> Option<String> {
    let ext = drt_config::modules::module_extension(path)?;
    drt_config::modules::paths_for_name(module)
        .ok()?
        .into_iter()
        .find(|candidate| candidate.ends_with(&format!(".{ext}")))
}

/// Where a runnable package's entry module lands, for the hint.
fn g_dest(manifest: &Manifest, main: &str) -> Option<String> {
    let guest = manifest.guest.as_ref()?;
    module_dest(main, guest.modules.get(main)?)
}

/// Is a destination one this package itself locked before? A re-pull of
/// the same package may replace its own files and no one else's.
fn placement_owned_by(lock: &dollup_format::Lockfile, name: &str, dest: &str) -> bool {
    lock.packages
        .get(name)
        .is_some_and(|locked| locked.files.contains_key(dest))
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
    let store = Store::open(&deployment.store_dir()?)?;
    for (name, locked) in &deployment.lock.packages {
        for (path, want) in &locked.files {
            let on_disk = deployment.code_root().join(path);
            match fs::read(&on_disk) {
                Ok(bytes) if &hash_bytes(&bytes) == want => {}
                Ok(_) => problems.push(format!("{name}: {path} does not match the lock")),
                Err(_) => problems.push(format!("{name}: {path} is missing")),
            }
            match store.get(want) {
                Ok(Some(_)) => {}
                Ok(None) => problems.push(format!(
                    "{name}: {path} absent from the cache (`dollup pull` refills it)"
                )),
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
            problems.push(format!("snapshot {name}: state absent from the cache"));
        }
    }
    Ok(problems)
}

/// Every blob a lock references: package files and pinned snapshot state.
fn referenced(lock: &dollup_format::Lockfile) -> impl Iterator<Item = dollup_format::Hash> + '_ {
    lock.packages
        .values()
        .flat_map(|p| p.files.values().cloned())
        .chain(lock.snapshots.values().map(|s| s.state.clone()))
}

/// `dollup gc`: sweep the store against what is referenced. A root's store
/// is the cache every root on this box shares, so the sweep keeps what
/// **every recorded root's** lock references, not only this one's; a
/// recorded root whose lock cannot be read is skipped and said, because
/// the blobs only it referenced are about to go — and its next `pull`
/// refetches them, which is what a cache is for. An app sweeps its own
/// store against its own lock, as it always did.
///
/// Returns what was swept and what was said.
pub fn gc(deployment: &Deployment) -> Result<(usize, Vec<String>)> {
    let mut keep: BTreeSet<_> = referenced(&deployment.lock).collect();
    let mut notes = vec![];
    if matches!(deployment.layout, crate::deployment::Layout::Root { .. }) {
        let here = deployment.dir.canonicalize().ok();
        for entry in crate::roots::load()?.roots {
            if Some(&entry.path) == here.as_ref() {
                continue;
            }
            let lock_path = crate::root::lock_path(&entry.path);
            match fs::read(&lock_path)
                .map_err(|e| e.to_string())
                .and_then(|b| {
                    serde_json::from_slice::<dollup_format::Lockfile>(&b).map_err(|e| e.to_string())
                }) {
                Ok(lock) => keep.extend(referenced(&lock)),
                Err(e) => notes.push(format!(
                    "skipping {}: {} could not be read ({e}); blobs only it referenced are \
                     swept, and its next `dollup pull` refetches them",
                    entry.path.display(),
                    lock_path.display()
                )),
            }
        }
    }
    let swept = Store::open(&deployment.store_dir()?)?.gc(&keep)?;
    Ok((swept, notes))
}

/// `dollup pull` of a template: a starting point, so its files are copied
/// into the app and never locked — you are meant to edit them, and a locked
/// file you edit is a `verify` failure. Its dependencies are ordinary
/// packages and are locked as usual.
///
/// This is the one path by which dollup delivers config, and the doctrine
/// holds: copying a file into a directory you own is not writing config into
/// a running app. Compare `add`, which never places a config at all.
pub fn new_from_template(deployment: &mut Deployment, r: &Ref) -> Result<Vec<String>> {
    let entries = entries_for(deployment, r)?;
    let mut opened: Vec<Open> = vec![];
    let mut skips: Vec<String> = vec![];
    let (idx, version, entry) = find(
        &entries,
        &mut opened,
        deployment.config.require_signatures,
        &r.name,
        r.version.as_ref(),
        &mut skips,
    )?;
    let source = opened[idx]
        .ready()
        .expect("find returns the index of a source it read");
    let manifest = admit(source, &r.name, &version, &entry)?;
    if !manifest.template {
        // Reachable only if the index and the manifest disagree about what
        // this is; `pull` sends a package down the lock path before here.
        bail!(
            "'{}' is not a template — it is a package you depend on.\n\
             \n  \
             lock it instead:  dollup pull {}",
            r.name,
            r.name
        );
    }

    // Refuse to clobber. A starting point that overwrites the work already
    // here is not a starting point.
    let existing: Vec<&String> = manifest
        .files
        .keys()
        .filter(|rel| deployment.dir.join(rel).exists())
        .collect();
    if !existing.is_empty() {
        bail!(
            "these already exist here, so '{}' would overwrite your work: {}",
            r.name,
            existing
                .iter()
                .map(|s| s.as_str())
                .collect::<Vec<_>>()
                .join(", ")
        );
    }

    let mut report = skips;
    for (rel, want) in &manifest.files {
        let remote = format!("{}/{}", entry.path, rel);
        let bytes = source.fetched.read(&remote)?.with_context(|| {
            format!(
                "{}: {remote} is named by the manifest but absent",
                source.entry.url()
            )
        })?;
        if &hash_bytes(&bytes) != want {
            bail!(
                "{}: {remote} does not match its manifest hash — refusing",
                source.entry.url()
            );
        }
        write_file(&deployment.dir.join(rel), &bytes)?;
        report.push(format!("  {rel}"));
    }
    report.insert(0, format!("From {} {version}:", r.name));

    // Dependencies are packages, not starting points: added and locked.
    for (dep, req) in &manifest.requires.packages {
        let dep_ref = Ref {
            source: Some(source.entry.url().to_string()),
            name: dep.clone(),
            version: Some(req.clone()),
        };
        report.extend(add(deployment, &dep_ref, HostGates::default())?);
    }
    deployment.save()?;
    Ok(report)
}
