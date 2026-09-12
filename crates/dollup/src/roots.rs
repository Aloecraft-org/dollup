//! The list of roots on this box: `~/.dollup/roots.json`.
//!
//! `dollup roots` is the `docker ps` moment — four roots on one machine and
//! one command that lists them — except that it lists roots on disk, not
//! deployments running, which is the line SPEC.md §13a draws. Any dollup
//! verb that opens a root registers it, not only the one that created it,
//! so a root copied with `cp -r` is seen the first time anyone touches it.
//! Stale entries — a root removed with `rm -rf` — are reported, never
//! silently dropped: a list that forgets is a list nobody can trust to be
//! complete. `audit` is the one verb that does not register, because it
//! writes nothing; it reads the list, to name a `root_id` two roots share.
//!
//! Registration is best-effort and never fails the verb that triggered it.
//! This is a convenience index over the filesystem, and the filesystem is
//! the truth: every row is re-checked against disk when it is shown.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result};
use drt_config::id::Uuid7;
use drt_config::project::ProjectJson;
use drt_config::time::Timestamp;
use serde::{Deserialize, Serialize};

use crate::home;
use crate::root;

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct Registry {
    #[serde(default)]
    pub roots: Vec<Entry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Entry {
    /// Absolute, canonical: the path is the key.
    pub path: PathBuf,
    pub root_id: Uuid7,
    /// When a verb last touched it.
    pub seen: Timestamp,
}

/// Record that a verb opened the root at `dir`. Best-effort: no home means
/// no list to keep, and a list that cannot be written is said once on
/// stderr and never stops the verb.
pub fn register(dir: &Path, root_id: Uuid7) {
    let Some(path) = home::roots_path() else {
        return;
    };
    if let Err(e) = register_at(&path, dir, root_id) {
        eprintln!(
            "note: could not record this root in {}: {e:#}",
            path.display()
        );
    }
}

fn register_at(registry_path: &Path, dir: &Path, root_id: Uuid7) -> Result<()> {
    let canonical = dir
        .canonicalize()
        .with_context(|| format!("resolving {}", dir.display()))?;
    let mut registry = load_at(registry_path)?;
    let seen =
        Timestamp::from_unix_secs(SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs() as i64);
    match registry.roots.iter_mut().find(|e| e.path == canonical) {
        Some(entry) => {
            entry.root_id = root_id;
            entry.seen = seen;
        }
        None => registry.roots.push(Entry {
            path: canonical,
            root_id,
            seen,
        }),
    }
    registry.roots.sort_by(|a, b| a.path.cmp(&b.path));
    if let Some(parent) = registry_path.parent() {
        fs::create_dir_all(parent)?;
    }
    // Written whole and renamed into place, so a reader never sees half a
    // list and two verbs racing leave one complete list, not a torn one.
    let tmp = registry_path.with_extension("json.tmp");
    let mut bytes = serde_json::to_vec_pretty(&registry)?;
    bytes.push(b'\n');
    fs::write(&tmp, bytes)?;
    fs::rename(&tmp, registry_path)?;
    Ok(())
}

/// The list as recorded. Absent is empty; unreadable is a named failure,
/// because a list that is there and cannot be read is not the same as none.
pub fn load() -> Result<Registry> {
    match home::roots_path() {
        Some(path) => load_at(&path),
        None => Ok(Registry::default()),
    }
}

fn load_at(path: &Path) -> Result<Registry> {
    if !path.is_file() {
        return Ok(Registry::default());
    }
    serde_json::from_slice(&fs::read(path)?)
        .with_context(|| format!("{} does not parse", path.display()))
}

/// What is on disk at a recorded path right now.
#[derive(Debug, Clone, PartialEq)]
pub enum OnDisk {
    /// The root is there and claims this id.
    Present { name: Option<String> },
    /// The directory or its descriptor is gone: `rm -rf`, an unmounted
    /// volume. Reported, not dropped.
    Stale(String),
    /// A root is there but it is a different one now.
    Replaced { now: Uuid7 },
}

pub fn on_disk(entry: &Entry) -> OnDisk {
    let descriptor = root::project_path(&entry.path);
    let bytes = match fs::read(&descriptor) {
        Ok(bytes) => bytes,
        Err(_) => return OnDisk::Stale(format!("{} is gone", descriptor.display())),
    };
    match serde_json::from_slice::<ProjectJson>(&bytes) {
        Ok(project) if project.root_id == entry.root_id => OnDisk::Present {
            name: project.project_name,
        },
        Ok(project) => OnDisk::Replaced {
            now: project.root_id,
        },
        Err(e) => OnDisk::Stale(format!("{} does not parse: {e}", descriptor.display())),
    }
}

/// Other roots on this box that hold `root_id` on disk right now — the
/// `cp -r` case, which `dollup duplicate` exists to avoid. Checked against
/// disk, so a stale entry never accuses anyone.
pub fn others_claiming(root_id: Uuid7, except: &Path) -> Vec<PathBuf> {
    let Ok(registry) = load() else {
        return vec![];
    };
    let except = except.canonicalize().ok();
    registry
        .roots
        .iter()
        .filter(|e| e.root_id == root_id && Some(&e.path) != except.as_ref())
        .filter(|e| matches!(on_disk(e), OnDisk::Present { .. }))
        .map(|e| e.path.clone())
        .collect()
}

/// `dollup roots`: one line per recorded root, checked against disk.
pub fn report() -> Result<Vec<String>> {
    let registry = load()?;
    if registry.roots.is_empty() {
        return Ok(vec![format!(
            "no roots recorded on this box{}",
            match home::roots_path() {
                Some(p) => format!(" ({} is empty or absent)", p.display()),
                None => " (no HOME, so no list is kept)".to_string(),
            }
        )]);
    }
    let states: Vec<OnDisk> = registry.roots.iter().map(on_disk).collect();
    // Which ids two present roots share.
    let mut holders: BTreeMap<String, Vec<&Path>> = BTreeMap::new();
    for (entry, state) in registry.roots.iter().zip(&states) {
        if matches!(state, OnDisk::Present { .. }) {
            holders
                .entry(entry.root_id.to_string())
                .or_default()
                .push(&entry.path);
        }
    }
    let width = registry
        .roots
        .iter()
        .map(|e| e.path.display().to_string().len())
        .max()
        .unwrap_or(0);
    let mut lines = vec![];
    for (entry, state) in registry.roots.iter().zip(&states) {
        let path = format!("{:width$}", entry.path.display().to_string());
        let line = match state {
            OnDisk::Present { name } => {
                let shared: Vec<String> = holders[&entry.root_id.to_string()]
                    .iter()
                    .filter(|p| **p != entry.path)
                    .map(|p| p.display().to_string())
                    .collect();
                let mut line = format!(
                    "{path}  {}  {}",
                    entry.root_id,
                    name.as_deref().unwrap_or("(unnamed)")
                );
                if !shared.is_empty() {
                    line.push_str(&format!(
                        "  duplicate root_id, also at {} (a cp -r; `dollup duplicate` mints a fresh one)",
                        shared.join(", ")
                    ));
                }
                line
            }
            OnDisk::Stale(why) => format!("{path}  {}  stale: {why}", entry.root_id),
            OnDisk::Replaced { now } => format!(
                "{path}  {}  replaced: the root there now is {now} (recorded on its next use)",
                entry.root_id
            ),
        };
        lines.push(line);
    }
    Ok(lines)
}
