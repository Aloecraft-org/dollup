//! Publisher-side verbs: index a repo tree, sign the index. These are what
//! the static mirror's generation script and every publisher run; keeping
//! them in the same binary keeps the format honest, because the writer and
//! the reader share one implementation.

use std::fs;
use std::path::Path;

use anyhow::{bail, Context, Result};
use dollup_format::identity::{code_set_identity, hash_bytes, package_identity};
use dollup_format::index::{Face, IndexEntry, RepoIndex, INDEX_FILE, SIG_FILE};
use dollup_format::{sign, Manifest};

/// `dollup repo index <dir>`: scan `packages/<name>/<version>/manifest.json`,
/// validate each package, hash every file, write `index.json`. Signing is a
/// separate verb because the key should not need to be where the tree is.
pub fn index(repo: &Path) -> Result<RepoIndex> {
    let mut out = RepoIndex::new();
    let packages_dir = repo.join("packages");
    if !packages_dir.is_dir() {
        bail!("{} has no packages/ directory", repo.display());
    }
    for name_entry in fs::read_dir(&packages_dir)? {
        let name_dir = name_entry?.path();
        if !name_dir.is_dir() {
            continue;
        }
        for ver_entry in fs::read_dir(&name_dir)? {
            let pkg_dir = ver_entry?.path();
            let manifest_path = pkg_dir.join("manifest.json");
            if !manifest_path.is_file() {
                continue;
            }
            let manifest_bytes = fs::read(&manifest_path)?;
            let manifest: Manifest = serde_json::from_slice(&manifest_bytes)
                .with_context(|| format!("{} does not parse", manifest_path.display()))?;
            manifest
                .check()
                .with_context(|| format!("{} refused", manifest_path.display()))?;

            // The tree is canonical: every file must be present and hash to
            // what the manifest says, or the repo is not publishable.
            for (rel, want) in &manifest.files {
                let bytes = fs::read(pkg_dir.join(rel)).with_context(|| {
                    format!("{}: '{rel}' named but absent", manifest_path.display())
                })?;
                if &hash_bytes(&bytes) != want {
                    bail!(
                        "{}: '{rel}' does not hash to its manifest entry",
                        manifest_path.display()
                    );
                }
            }

            let mut faces = vec![];
            if !manifest.capability.is_empty() {
                faces.push(Face::Capability);
            }
            if manifest.guest.is_some() {
                faces.push(Face::Guest);
            }
            if manifest.host.is_some() {
                faces.push(Face::Host);
            }
            let entry = IndexEntry {
                path: pkg_dir
                    .strip_prefix(repo)?
                    .to_string_lossy()
                    .replace('\\', "/"),
                manifest: hash_bytes(&manifest_bytes),
                package_id: package_identity(&manifest_bytes, &manifest.files),
                code_set: manifest
                    .guest
                    .as_ref()
                    .map(|g| code_set_identity(&g.main, &manifest.guest_files())),
                faces,
                targets: manifest
                    .host
                    .as_ref()
                    .map(|h| h.targets.keys().cloned().collect())
                    .unwrap_or_default(),
            };
            out.packages
                .entry(manifest.name.clone())
                .or_default()
                .versions
                .insert(manifest.version.clone(), entry);
        }
    }
    let mut bytes = serde_json::to_vec_pretty(&out)?;
    bytes.push(b'\n');
    fs::write(repo.join(INDEX_FILE), &bytes)?;
    // A stale signature over a fresh index is worse than none: drop it.
    let _ = fs::remove_file(repo.join(SIG_FILE));
    Ok(out)
}

/// `dollup repo seal <package-dir>`: the authoring tool. Walk the package,
/// hash every file, write the `files` map into its manifest, and validate
/// the result. Publishing without this means hand-computing SHA-256 per
/// file; verification never trusts it — `index` re-hashes independently, so
/// a stale seal is caught rather than believed.
///
/// Everything present is included, because `files` means "what this package
/// contains" and a file omitted from identity is a file nobody verifies.
/// Editor debris is skipped by name, and the full list is printed so the
/// author sees exactly what they are about to publish.
pub fn seal(pkg_dir: &Path) -> Result<Vec<String>> {
    let manifest_path = pkg_dir.join("manifest.json");
    let mut manifest: Manifest = serde_json::from_slice(
        &fs::read(&manifest_path)
            .with_context(|| format!("reading {}", manifest_path.display()))?,
    )
    .with_context(|| format!("{} does not parse", manifest_path.display()))?;

    let mut files = std::collections::BTreeMap::new();
    let mut report = vec![];
    let mut stack = vec![pkg_dir.to_path_buf()];
    while let Some(dir) = stack.pop() {
        for entry in fs::read_dir(&dir)? {
            let path = entry?.path();
            let name = path
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .into_owned();
            if name.starts_with('.') || name.ends_with('~') || name.ends_with(".tmp") {
                continue;
            }
            if path.is_dir() {
                stack.push(path);
                continue;
            }
            let rel = path
                .strip_prefix(pkg_dir)?
                .to_string_lossy()
                .replace('\\', "/");
            if rel == "manifest.json" {
                continue;
            }
            let bytes = fs::read(&path)?;
            report.push(format!("  {rel} ({} bytes)", bytes.len()));
            files.insert(rel, hash_bytes(&bytes));
        }
    }
    report.sort();
    manifest.files = files;
    manifest
        .check()
        .with_context(|| format!("{} refused after sealing", manifest_path.display()))?;

    let mut bytes = serde_json::to_vec_pretty(&manifest)?;
    bytes.push(b'\n');
    fs::write(&manifest_path, bytes)?;
    report.insert(0, format!("sealed {} {}", manifest.name, manifest.version));
    Ok(report)
}

/// `dollup repo sign <dir> --key-file <path>`: sign the exact index bytes.
pub fn sign_index(repo: &Path, key_file: &Path) -> Result<()> {
    let key =
        fs::read_to_string(key_file).with_context(|| format!("reading {}", key_file.display()))?;
    let index_bytes = fs::read(repo.join(INDEX_FILE)).with_context(|| {
        format!(
            "{} has no {INDEX_FILE} — run `dollup repo index` first",
            repo.display()
        )
    })?;
    let sig = sign::sign(key.trim(), &index_bytes)?;
    fs::write(repo.join(SIG_FILE), format!("{sig}\n"))?;
    Ok(())
}

/// Generate the blob projection (RepoFormat.md §3) so a static mirror can
/// serve incrementally by hash: every package file plus every manifest,
/// linked under `blobs/sha256/<hex>`.
pub fn blobs(repo: &Path) -> Result<usize> {
    let index_bytes = fs::read(repo.join(INDEX_FILE)).with_context(|| {
        format!(
            "{} has no {INDEX_FILE} — run `dollup repo index` first",
            repo.display()
        )
    })?;
    let parsed: RepoIndex = serde_json::from_slice(&index_bytes)?;
    let mut count = 0;
    for versions in parsed.packages.values() {
        for entry in versions.versions.values() {
            let pkg_dir = repo.join(&entry.path);
            let manifest_bytes = fs::read(pkg_dir.join("manifest.json"))?;
            let manifest: Manifest = serde_json::from_slice(&manifest_bytes)?;
            count += project(repo, &entry.manifest, &manifest_bytes)?;
            for (rel, hash) in &manifest.files {
                count += project(repo, hash, &fs::read(pkg_dir.join(rel))?)?;
            }
        }
    }
    Ok(count)
}

fn project(repo: &Path, hash: &dollup_format::Hash, bytes: &[u8]) -> Result<usize> {
    let (algo, hex) = hash.0.split_once(':').context("bad hash")?;
    let dir = repo.join("blobs").join(algo);
    fs::create_dir_all(&dir)?;
    let path = dir.join(hex);
    if path.exists() {
        return Ok(0);
    }
    fs::write(path, bytes)?;
    Ok(1)
}
