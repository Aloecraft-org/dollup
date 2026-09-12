//! `dollup duplicate <path>`: this root, copied to a new path as a new root.
//!
//! `cp -r` done right. A copy made by hand carries the original's
//! `root_id`, and two roots claiming one id is what breaks shipping —
//! `root_id` is the hinge a pull turns on, restore or new — so the copy
//! gets a fresh one, minted, and records where it came from in
//! `duplicated_from`. What is runtime-owned does not come along: `state/`
//! holds an envelope and approvals bound to the old id, `live/` is what
//! runs and `drt deploy` remakes it, `log/` is the old root's. Everything
//! else does — the descriptor, the profiles, `init/`, the lock, the
//! deployed runtime, and `dlua/` with whatever else sits beside
//! `.drt_root/`, because a root is its directory.
//!
//! Consent is the one file that is neither copied nor simply dropped. It
//! never travels, and a copy on the same box by the same operator is not
//! travel — but a copy cannot invent an acceptance nobody made. So the
//! duplicate gets exactly the consent the source *effectively* has: when
//! the source's acceptance covers its current ceiling, a fresh listed entry
//! over that ceiling; when it does not, none, and the copy's first start
//! asks, as the source's would. Blanket consent on the source becomes
//! listed on the copy: the opt-out was for that root, and `consent --all`
//! is one command away on this one. Signers come along either way: they are
//! who may approve, not an approval, and a copy on this box answers to the
//! same keys.

use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};
use drt_config::consent::{self, Accepted, ConsentCheck, ConsentJson};
use drt_config::project::{self, ProjectJson, ROOT_DIR};
use drt_config::realm::Realm;
use drt_config::time::Timestamp;

use crate::deployment::write_json;
use crate::root;
use crate::roots;

/// What a duplicate did, one line each, for printing.
pub fn duplicate(src: &Path, dst: &Path) -> Result<Vec<String>> {
    if !root::exists(src) {
        bail!(
            "no root here — {} does not exist (discovery does not walk up; --root names one)",
            root::project_path(src).display()
        );
    }
    let src_canon = src
        .canonicalize()
        .with_context(|| format!("resolving {}", src.display()))?;
    if dst.exists() {
        if root::root_dir(dst).exists() {
            bail!(
                "{} is already a root; a duplicate goes to a new path",
                dst.display()
            );
        }
        if fs::read_dir(dst)?.next().is_some() {
            bail!(
                "{} exists and is not empty; a duplicate goes to a new or empty path",
                dst.display()
            );
        }
    }
    // Nowhere under the source, however deep and however much of the path
    // is still to be made: the nearest ancestor that exists decides.
    let dst_abs = if dst.is_absolute() {
        dst.to_path_buf()
    } else {
        std::env::current_dir()?.join(dst)
    };
    let anchor = dst_abs
        .ancestors()
        .find(|p| p.exists())
        .map(Path::canonicalize)
        .transpose()?;
    if anchor.is_some_and(|a| a.starts_with(&src_canon)) {
        bail!(
            "{} is inside the root being duplicated; a copy inside itself never finishes",
            dst.display()
        );
    }

    let project_path = root::project_path(src);
    let project: ProjectJson = serde_json::from_slice(&fs::read(&project_path)?)
        .with_context(|| format!("{} does not parse", project_path.display()))?;
    let consent_path = root::consent_path(src);
    let source_consent = if consent_path.is_file() {
        Some(
            serde_json::from_slice::<ConsentJson>(&fs::read(&consent_path)?)
                .with_context(|| format!("{} does not parse", consent_path.display()))?,
        )
    } else {
        None
    };

    // The copy: everything but what the header says stays behind.
    let meta = root::root_dir(&src_canon);
    let stays: Vec<PathBuf> = [project::STATE_DIR, project::LIVE_DIR, project::LOG_DIR]
        .iter()
        .map(|d| meta.join(d))
        .chain([meta.join("consent.json"), meta.join("project.json")])
        .collect();
    let made = !dst.exists();
    let copied = match copy_tree(&src_canon, dst, &|path| stays.iter().any(|s| path == s)) {
        Ok(n) => n,
        Err(e) => {
            // Only what this call created: a half-copy at a path that was
            // ours to make is not a root and would only block the retry.
            if made {
                let _ = fs::remove_dir_all(dst);
            }
            return Err(e);
        }
    };
    for dir in [project::STATE_DIR, project::LIVE_DIR, project::LOG_DIR] {
        fs::create_dir_all(root::root_dir(dst).join(dir))?;
    }

    // A new root: minted, never derived, and it remembers where it came from.
    let (unix_ms, unix_secs) = root::now()?;
    let mut copy = project.clone();
    copy.root_id = root::mint_root_id(unix_ms)?;
    copy.duplicated_from = Some(project.root_id);
    write_json(&root::project_path(dst), &copy)?;

    let mut lines = vec![
        format!(
            "duplicated {} → {} ({} file(s))",
            src_canon.display(),
            dst.display(),
            copied
        ),
        format!(
            "root_id {} (duplicated_from {})",
            copy.root_id, project.root_id
        ),
        format!(
            "left behind: {}/{{{}, {}, {}}} (runtime-owned) and consent.json (never travels)",
            ROOT_DIR,
            project::STATE_DIR,
            project::LIVE_DIR,
            project::LOG_DIR
        ),
    ];

    // The consent the source effectively has, and no more.
    let covering = match consent::check(&project, source_consent.as_ref()) {
        Ok(ConsentCheck::Unchanged) | Ok(ConsentCheck::Narrowed { .. }) => true,
        Ok(ConsentCheck::Blanket { .. }) => {
            lines.push(
                "consent: the source's is blanket; the copy's is listed over the current \
                 ceiling — `dollup consent --all` opts this root out too"
                    .into(),
            );
            true
        }
        Ok(ConsentCheck::First { .. }) | Ok(ConsentCheck::Widened { .. }) => false,
        Err(e) => {
            lines.push(format!(
                "consent: none written — the source's cannot be evaluated: {e}"
            ));
            false
        }
    };
    let mut fresh = ConsentJson {
        root_id: copy.root_id,
        accepted: Vec::new(),
        signers: source_consent.map(|c| c.signers).unwrap_or_default(),
    };
    if covering {
        fresh.accepted.push(Accepted::Listed {
            realm: Realm::root(),
            ceiling_hash: project::ceiling_hash(&copy)?,
            ceiling: copy.caps.clone(),
            accepted_at: Timestamp::from_unix_secs(unix_secs),
        });
        lines.push(
            "consent: listed over the current ceiling, as the source's acceptance covers it".into(),
        );
    } else if !lines.iter().any(|l| l.starts_with("consent:")) {
        lines.push(
            "consent: none — the source's does not cover its ceiling, so the copy's first \
             start asks, as the source's would"
                .into(),
        );
    }
    if !fresh.signers.is_empty() {
        lines.push(format!("signers: {} kept", fresh.signers.len()));
    }
    // No acceptance and no signers is no file: a consent.json that says
    // nothing would only look like one that does.
    if !fresh.accepted.is_empty() || !fresh.signers.is_empty() {
        write_json(&root::consent_path(dst), &fresh)?;
    }

    roots::register(dst, copy.root_id);
    Ok(lines)
}

/// Copy every file under `from` into `to`, skipping what `skip` names (and
/// everything beneath it). Depth-first over an explicit stack; symlinks are
/// recreated as symlinks where the platform has them, never followed, so a
/// link back up the tree cannot turn the walk into a loop. Returns the
/// number of files copied.
fn copy_tree(from: &Path, to: &Path, skip: &dyn Fn(&Path) -> bool) -> Result<usize> {
    let mut copied = 0;
    let mut stack = vec![from.to_path_buf()];
    fs::create_dir_all(to)?;
    while let Some(dir) = stack.pop() {
        for entry in fs::read_dir(&dir)? {
            let entry = entry?;
            let path = entry.path();
            if skip(&path) {
                continue;
            }
            let rel = path.strip_prefix(from)?;
            let target = to.join(rel);
            let kind = entry.file_type()?;
            if kind.is_dir() {
                fs::create_dir_all(&target)?;
                stack.push(path);
                continue;
            }
            if kind.is_symlink() {
                copy_link(&path, &target)?;
            } else {
                fs::copy(&path, &target).with_context(|| {
                    format!("copying {} to {}", path.display(), target.display())
                })?;
            }
            copied += 1;
        }
    }
    Ok(copied)
}

#[cfg(unix)]
fn copy_link(path: &Path, target: &Path) -> Result<()> {
    let to = fs::read_link(path)?;
    std::os::unix::fs::symlink(&to, target)
        .with_context(|| format!("linking {} -> {}", target.display(), to.display()))
}

#[cfg(not(unix))]
fn copy_link(path: &Path, target: &Path) -> Result<()> {
    fs::copy(path, target)
        .with_context(|| format!("copying {} to {}", path.display(), target.display()))?;
    Ok(())
}
