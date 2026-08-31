//! Publisher-side verbs: index a repo tree, sign the index. These are what
//! the static mirror's generation script and every publisher run; keeping
//! them in the same binary keeps the format honest, because the writer and
//! the reader share one implementation.

use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};
use dollup_format::identity::{code_set_identity, hash_bytes, package_identity};
use dollup_format::index::{Face, IndexEntry, RepoIndex, INDEX_FILE, SIG_FILE};
use dollup_format::source::Ref;
use dollup_format::{sign, Manifest, SourceEntry};

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
                    .map(|g| code_set_identity(g.main.as_deref(), &manifest.guest_files())),
                faces,
                runnable: manifest.guest.as_ref().is_some_and(|g| g.main.is_some()),
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

/// `dollup repo publish <dir>`: seal, index, sign, project, and then prove
/// the result is addable before anyone rsyncs it.
///
/// This exists because two separate repositories had already written the
/// same ninety lines of bash — dollup's own `std-repo/publish.sh` and
/// disco-fetchpoint's `publish.sh` — differing only in their rsync target.
/// A sequence that every publisher must run in the same order, whose steps
/// depend on each other, and whose last step is the one people skip, is a
/// verb.
///
/// **The self-check is the point.** Sealing, indexing and signing all
/// "succeed" on a repo nobody can install from: a manifest that names a
/// file it does not list, an index written before the last edit, a
/// signature over stale bytes. So the last thing publish does is resolve
/// the tree it just produced — a throwaway deployment, the tree as a
/// `file://` source, every package added, then `verify` against the lock.
/// Publishing a repo that cannot be added is the failure this prevents, and
/// it is the reason the verb is worth having at all.
///
/// What it deliberately does **not** do is deploy. rsync, scp, S3 and the
/// rest are the operator's, and a fetcher that learns to write to a server
/// has become something else. It prints the directory to copy.
#[derive(Debug)]
pub struct Published {
    pub sealed: Vec<String>,
    pub packages: usize,
    pub blobs: usize,
    pub signed_by: Option<String>,
    /// What to copy: the staging directory when one was asked for, else the
    /// repo itself.
    pub tree: PathBuf,
    pub resolved: Vec<String>,
}

pub fn publish(
    repo: &Path,
    key_file: Option<&Path>,
    stage: Option<&Path>,
    with_blobs: bool,
) -> Result<Published> {
    let mut sealed = vec![];
    // Every package, every version. Idempotent, so running it here means a
    // hand-edited module can never ship under a stale hash — and `index`
    // re-hashes independently afterwards, so a seal that did not happen is
    // caught rather than believed.
    let mut pkg_dirs: Vec<PathBuf> = vec![];
    let packages_dir = repo.join("packages");
    if packages_dir.is_dir() {
        let mut names: Vec<_> = fs::read_dir(&packages_dir)?
            .filter_map(|e| e.ok())
            .map(|e| e.path())
            .filter(|p| p.is_dir())
            .collect();
        names.sort();
        for name in names {
            let mut versions: Vec<_> = fs::read_dir(&name)?
                .filter_map(|e| e.ok())
                .map(|e| e.path())
                .filter(|p| p.join("manifest.json").is_file())
                .collect();
            versions.sort();
            pkg_dirs.append(&mut versions);
        }
    }
    if pkg_dirs.is_empty() {
        bail!(
            "{} has no packages/<name>/<version>/manifest.json — nothing to publish",
            repo.display()
        );
    }
    for dir in &pkg_dirs {
        sealed.extend(seal(dir)?);
    }

    let idx = index(repo)?;
    let packages = idx.packages.len();

    let signed_by = match key_file {
        None => None,
        Some(path) => {
            sign_index(repo, path)?;
            let key =
                fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
            // Derived, never hunted for beside the private key: the two
            // cannot drift, so what signed the index is exactly what the
            // self-check below pins.
            Some(sign::public_key_of(key.trim())?)
        }
    };

    let blobs_made = if with_blobs {
        // Rebuild rather than accumulate: the tree is canonical and blobs/
        // is a projection of it, so a blob left over from a deleted package
        // would be served forever under a name nothing indexes.
        let dir = repo.join("blobs");
        if dir.exists() {
            fs::remove_dir_all(&dir)?;
        }
        blobs(repo)?
    } else {
        0
    };

    // A repo is exactly these four things. Staging copies them and nothing
    // else, so the README, the publish script and the .git directory do not
    // travel to a web root.
    let tree = match stage {
        None => repo.to_path_buf(),
        Some(stage) => {
            if stage.exists() {
                fs::remove_dir_all(stage)?;
            }
            fs::create_dir_all(stage)?;
            for item in [INDEX_FILE, SIG_FILE, "packages", "blobs"] {
                let from = repo.join(item);
                if from.exists() {
                    copy_into(&from, &stage.join(item))?;
                }
            }
            stage.to_path_buf()
        }
    };

    let resolved = self_check(&tree, signed_by.as_deref())?;

    Ok(Published {
        sealed,
        packages,
        blobs: blobs_made,
        signed_by,
        tree,
        resolved,
    })
}

/// Resolve the produced tree the way a consumer will: a throwaway
/// deployment, the tree as a `file://` source, every package added, then
/// `verify` against the lock. Any failure here is a repo that would have
/// been published broken.
fn self_check(tree: &Path, pin: Option<&str>) -> Result<Vec<String>> {
    let scratch = tempfile::tempdir()?;
    let mut d = crate::deployment::Deployment::init(scratch.path(), None)?;
    let url = format!(
        "file://{}",
        fs::canonicalize(tree)
            .with_context(|| format!("resolving {}", tree.display()))?
            .display()
    );
    d.config.sources.push(match pin {
        Some(key) => SourceEntry::Signed {
            url,
            keys: vec![key.to_string()],
        },
        None => SourceEntry::Url(url),
    });
    d.save()?;

    let index_bytes = fs::read(tree.join(INDEX_FILE))?;
    let parsed: RepoIndex = serde_json::from_slice(&index_bytes)?;
    let mut lines = vec![];
    // Every package in the tree, not a chosen one: the claim being checked
    // is that the published repo resolves, all of it.
    for name in parsed.packages.keys() {
        let r: Ref = name.parse()?;
        lines.extend(crate::ops::add(
            &mut d,
            &r,
            crate::ops::HostGates::default(),
        )?);
    }
    // `ops::verify` returns PROBLEMS; empty is clean. Extending the output
    // with them printed a broken repo's failures as though they were
    // successes and exited 0 — the self-check passing on precisely what it
    // exists to catch.
    let problems = crate::ops::verify(&d)?;
    if !problems.is_empty() {
        for p in &problems {
            eprintln!("{p}");
        }
        bail!(
            "the published tree does not verify ({} problem(s)) — it is not publishable",
            problems.len()
        );
    }
    lines.push(format!(
        "verified: {} package(s) match the lock",
        d.lock.packages.len()
    ));
    Ok(lines)
}

fn copy_into(from: &Path, to: &Path) -> Result<()> {
    if from.is_dir() {
        fs::create_dir_all(to)?;
        for entry in fs::read_dir(from)? {
            let entry = entry?;
            copy_into(&entry.path(), &to.join(entry.file_name()))?;
        }
    } else {
        if let Some(parent) = to.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::copy(from, to)?;
    }
    Ok(())
}

#[cfg(test)]
mod publish_tests {
    use super::*;

    /// The smallest legal repo: one package, one guest module, no deps.
    fn a_repo(root: &Path) {
        let pkg = root.join("packages/demo/0.1.0");
        fs::create_dir_all(pkg.join("guest")).unwrap();
        fs::write(pkg.join("guest/demo.dlua"), "print(host.time())\n").unwrap();
        fs::write(
            pkg.join("manifest.json"),
            r#"{"name":"demo","version":"0.1.0",
                "guest":{"main":"demo","modules":{"demo":"guest/demo.dlua"},"source_only":true}}"#,
        )
        .unwrap();
    }

    #[test]
    fn publish_produces_a_tree_that_actually_resolves() {
        let dir = tempfile::tempdir().unwrap();
        a_repo(dir.path());
        let out = publish(dir.path(), None, None, false).unwrap();
        assert_eq!(out.packages, 1);
        assert!(out.tree.join(INDEX_FILE).is_file());
        // The self-check ran and said so — the claim publish exists to make.
        assert!(
            out.resolved.iter().any(|l| l.starts_with("verified: 1")),
            "{:?}",
            out.resolved
        );
    }

    #[test]
    fn signing_pins_the_key_derived_from_the_one_that_signed() {
        let dir = tempfile::tempdir().unwrap();
        a_repo(dir.path());
        let (private, public) = sign::keygen();
        let key_file = dir.path().join("k.key");
        fs::write(&key_file, &private).unwrap();

        let out = publish(dir.path(), Some(&key_file), None, false).unwrap();
        // Derived, not read from a `.pub` beside it — and the self-check
        // resolved a SIGNED source with it, so the signature was verified
        // rather than merely written.
        assert_eq!(out.signed_by.as_deref(), Some(public.as_str()));
        assert!(dir.path().join(SIG_FILE).is_file());
    }

    #[test]
    fn staging_copies_the_repo_and_nothing_else() {
        let dir = tempfile::tempdir().unwrap();
        a_repo(dir.path());
        // Things a repo directory accumulates that must not reach a web root.
        fs::write(dir.path().join("README.md"), "notes").unwrap();
        fs::write(dir.path().join("publish.sh"), "#!/bin/sh\n").unwrap();

        let stage = dir.path().join(".publish");
        let out = publish(dir.path(), None, Some(&stage), false).unwrap();
        assert_eq!(out.tree, stage);

        let mut names: Vec<String> = fs::read_dir(&stage)
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        names.sort();
        assert_eq!(names, vec!["index.json", "packages"]);
    }

    #[test]
    fn a_directory_with_no_packages_is_refused_by_name() {
        let dir = tempfile::tempdir().unwrap();
        let err = publish(dir.path(), None, None, false)
            .unwrap_err()
            .to_string();
        assert!(err.contains("nothing to publish"), "{err}");
    }
}
