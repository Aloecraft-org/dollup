//! `dollup deploy drt` and `dollup pin drt`: the runtime into a root, and
//! the root's record of which one.
//!
//! drt is never installed, only deployed. A root is self-contained — tar
//! it and move it and it runs — so the binary is copied from the cache into
//! `.drt_root/drt`, never linked: a link from a root into `~/.dollup/` would
//! be a root depending on `~/.dollup/`, which no root may do. `pin` writes
//! the version into `project.json`, the one descriptor field a later verb
//! writes, and deploys that version first so the pin and the binary present
//! agree — start refuses the mismatch by name, so a pin that moved without
//! the binary would be an outage.
//!
//! `dollup deploy <app>` — the profile's source into `live/` — is deliberately
//! not here yet: it must do exactly what `drt deploy` does, and which
//! directories that reads is the question drt is settling.

use std::fs;
use std::path::Path;

use anyhow::{bail, Context, Result};
use drt_config::project::ProjectJson;

use crate::deployment::write_json;
use crate::root;
use crate::roots;
use crate::runtime;

/// What `deploy drt` and `pin drt` take beyond the version.
#[derive(Debug, Clone, Default)]
pub struct Opts {
    /// Where to fetch from if the cache lacks it; `file://` for the
    /// air-gapped case.
    pub from: Option<String>,
    /// The size profile rather than the full runtime.
    pub slim: bool,
}

/// `dollup deploy drt [version]`: the release into `.drt_root/drt`, from
/// the cache, pulling first if it is not there. The version is the one
/// named, else the pin; a root with neither is told to name or pin one.
pub fn deploy_drt(dir: &Path, version: Option<&str>, opts: &Opts) -> Result<Vec<String>> {
    let project = read_project(dir)?;
    let want = match version.map(str::to_string).or_else(|| project.drt.clone()) {
        Some(v) => v,
        None => bail!(
            "no version to deploy: name one (`deploy drt v0.4.1`) or pin one (`pin drt v0.4.1`)"
        ),
    };
    let cached = runtime::ensure_cached(&want, opts.from.as_deref(), opts.slim)?;
    let mut lines = cached.lines;
    let dest = root::root_dir(dir).join("drt");
    let bytes =
        fs::read(&cached.asset).with_context(|| format!("reading {}", cached.asset.display()))?;
    runtime::write_executable(&dest, &bytes)
        .with_context(|| format!("writing {}", dest.display()))?;
    lines.push(format!(
        "deployed drt {} to {} (copied from the cache; never a link)",
        cached.release.version,
        dest.display()
    ));
    // One version under two spellings is no mismatch (doc/ALIGNMENT.md
    // §10): the comparison is drt-config's, the one start makes.
    if let Some(pinned) = &project.drt {
        if !drt_config::version::same(pinned, &cached.release.version) {
            lines.push(format!(
                "note: the pin is {pinned} and the binary is now {}; start refuses that \
                 mismatch by name until `pin drt {}` moves the pin",
                cached.release.version, cached.release.version
            ));
        }
    }
    roots::register(dir, project.root_id);
    Ok(lines)
}

/// `dollup pin drt [version]`: deploy the release and record it. With no
/// version, the binary already deployed is identified through the cache —
/// by hash, never by running it — and pinned as what it is.
pub fn pin_drt(dir: &Path, version: Option<&str>, opts: &Opts) -> Result<Vec<String>> {
    let mut project = read_project(dir)?;
    let version = match version {
        Some(v) => runtime::Release::named(v).version,
        None => match identify(dir)? {
            Some(v) => v,
            None => bail!(
                "name a version: nothing is deployed at {} that the cache can identify \
                 (`pin drt v0.4.1`, or `deploy drt v0.4.1` first)",
                root::root_dir(dir).join("drt").display()
            ),
        },
    };
    let mut lines = deploy_drt(dir, Some(&version), opts)?;
    lines.retain(|l| !l.starts_with("note: the pin is"));
    project.drt = Some(version.clone());
    write_json(&root::project_path(dir), &project)?;
    lines.push(format!(
        "pinned drt {version} in {}",
        root::project_path(dir).display()
    ));
    Ok(lines)
}

/// `dollup pin drt <version> --all`: every recorded root on this box that
/// is still there. A version is required: identifying each root's binary
/// separately would pin four roots to four different things and call it
/// one act.
pub fn pin_all(version: &str, opts: &Opts) -> Result<Vec<String>> {
    let mut lines = vec![];
    for entry in roots::load()?.roots {
        match roots::on_disk(&entry) {
            roots::OnDisk::Present { .. } => {
                lines.push(format!("{}:", entry.path.display()));
                for line in pin_drt(&entry.path, Some(version), opts)? {
                    lines.push(format!("  {line}"));
                }
            }
            other => lines.push(format!(
                "{}: skipped ({})",
                entry.path.display(),
                match other {
                    roots::OnDisk::Stale(why) => why,
                    roots::OnDisk::Replaced { now } =>
                        format!("a different root is there now: {now}"),
                    roots::OnDisk::Present { .. } => unreachable!(),
                }
            )),
        }
    }
    if lines.is_empty() {
        lines.push("no roots recorded on this box; nothing to pin".into());
    }
    Ok(lines)
}

/// Which cached release the deployed binary is, by hash against every
/// cached SHA256SUMS.txt. `None` when nothing is deployed or nothing cached
/// matches — never a guess.
fn identify(dir: &Path) -> Result<Option<String>> {
    let binary = root::root_dir(dir).join("drt");
    let Ok(bytes) = fs::read(&binary) else {
        return Ok(None);
    };
    Ok(runtime::identify(&runtime::sha256_hex(&bytes)).map(|(version, _)| version))
}

fn read_project(dir: &Path) -> Result<ProjectJson> {
    if !root::exists(dir) {
        bail!(
            "no root here — {} does not exist (discovery does not walk up; --root names one)",
            root::project_path(dir).display()
        );
    }
    let path = root::project_path(dir);
    serde_json::from_slice(&fs::read(&path)?)
        .with_context(|| format!("{} does not parse", path.display()))
}
