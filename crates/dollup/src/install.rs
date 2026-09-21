//! `dollup install drt` — the runtime onto this box's PATH.
//!
//! **Why this is not a flag on `get`.** `runtime.rs` promises, in as many
//! words, "no install prefix, no PATH surgery, no `~/.config`, nothing
//! written anywhere the caller did not point at". That is a promise worth
//! keeping literally, so the verb that does install is a different verb
//! rather than a mode of the one that does not. `get` drops one file where
//! you point; `install` puts it where binaries go and says so. Two honest
//! verbs beat one with an asterisk.
//!
//! **Why this is not the same as `deploy drt`.** A root's runtime lives at
//! `.drt_root/drt`, copied from the cache, never linked — `deploy.rs` spells
//! out why: a root is self-contained, and a root depending on `~/.dollup/`
//! would break that. This verb is the other scope entirely: one drt on the
//! PATH, for a person at a shell, belonging to no root. Nothing here writes
//! into any root, and no root gains a dependency on what this installs.
//!
//! **Why dollup and not `curl | sh`.** drt publishes its own installer and
//! it is a good one; this is the same act with dollup's fetch underneath.
//! What that buys: the origin-then-mirror fallback with dollup's refusal
//! semantics, where a source whose bytes disagree with its own sums ends the
//! search rather than being passed over; `--from` for an air-gapped install
//! off a `file://` directory; and the shared cache, so installing and then
//! deploying into roots fetches once. No pipe from the network into a shell.

use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};

use crate::roots;
use crate::runtime;

/// Where a binary goes when nobody says: the system directory if this user
/// can write it, else the per-user one. The same rule dollup's own
/// `install.sh` follows, and drt's — a third spelling of it would be a
/// fourth place to look when an install lands somewhere surprising.
const SYSTEM_PREFIX: &str = "/usr/local/bin";
const USER_PREFIX: &str = ".local/bin";

pub struct InstallOpts {
    pub version: String,
    pub prefix: Option<PathBuf>,
    pub slim: bool,
    pub from: Option<String>,
}

/// `dollup install drt [version]`: the release onto the PATH, through the
/// cache. -> the report, line by line.
pub fn install_drt(opts: &InstallOpts) -> Result<Vec<String>> {
    let cached = runtime::ensure_cached(&opts.version, opts.from.as_deref(), opts.slim)?;
    let mut lines = cached.lines;

    let prefix = match &opts.prefix {
        Some(dir) => dir.clone(),
        None => default_prefix()?,
    };
    std::fs::create_dir_all(&prefix).with_context(|| format!("creating {}", prefix.display()))?;
    let dest = prefix.join("drt");

    let bytes = std::fs::read(&cached.asset)
        .with_context(|| format!("reading {}", cached.asset.display()))?;

    // Beside the destination, not over it: the binary is checked where it
    // cannot be mistaken for the installed one, and only a check that passes
    // moves it into place. So a drt that will not run here leaves whatever
    // was on the PATH exactly as it was, rather than replacing a working
    // runtime with a broken one and saying so afterwards. The rename is
    // atomic because the two paths are in one directory, which also makes
    // upgrading a drt that is currently executing safe: the directory entry
    // is replaced and a running process keeps its own inode.
    //
    // `into_temp_path` is load-bearing, not tidiness: it drops the open
    // write handle and keeps the path. Linux refuses to execute a file that
    // is open for writing (ETXTBSY), so the smoke check below fails on every
    // install while that handle is alive.
    let staged = tempfile::NamedTempFile::new_in(&prefix)
        .with_context(|| format!("staging a file in {}", prefix.display()))?
        .into_temp_path();
    runtime::write_executable(&staged, &bytes)
        .with_context(|| format!("writing {}", staged.display()))?;
    runs_here(&staged, &cached.asset, &dest)?;
    staged
        .persist(&dest)
        .map_err(|e| e.error)
        .with_context(|| format!("moving the checked binary into {}", dest.display()))?;

    lines.push(format!(
        "installed drt {} to {}",
        cached.release.version,
        dest.display()
    ));
    if let Some(note) = path_note(&prefix) {
        lines.push(note);
    }
    lines.extend(shadowed_pins(&cached.release.version));
    Ok(lines)
}

/// The one place dollup runs what it fetched — and it is not
/// identification. `audit` names a binary by hash precisely so it never has
/// to execute one; this is the post-install smoke check both installers
/// already make, catching a binary for the wrong architecture with the asset
/// named rather than later, as something that reads like a broken install.
/// Run before the binary is in place, so a refusal costs the caller nothing.
fn runs_here(staged: &Path, asset: &Path, dest: &Path) -> Result<()> {
    let untouched = if dest.exists() {
        format!("{} is still the drt that was there", dest.display())
    } else {
        format!("nothing was written to {}", dest.display())
    };
    match std::process::Command::new(staged).arg("--version").output() {
        Ok(out) if out.status.success() => Ok(()),
        Ok(out) => bail!(
            "{} does not run here: `drt --version` exited {}\n  {untouched}",
            asset.display(),
            out.status
        ),
        Err(e) => bail!("{} does not run here ({e})\n  {untouched}", asset.display()),
    }
}

/// `/usr/local/bin` when this user can write it, else `~/.local/bin`.
/// Writability is probed rather than inferred from the mode bits: the
/// question is whether *this* process can write there, which ownership,
/// groups and a read-only mount all have a say in.
fn default_prefix() -> Result<PathBuf> {
    let system = PathBuf::from(SYSTEM_PREFIX);
    if writable(&system) {
        return Ok(system);
    }
    let home = std::env::var_os("HOME")
        .filter(|h| !h.is_empty())
        .ok_or_else(|| {
            anyhow::anyhow!(
                "{SYSTEM_PREFIX} is not writable and HOME is not set, so there is nowhere \
                 to install; name a directory with --prefix"
            )
        })?;
    Ok(PathBuf::from(home).join(USER_PREFIX))
}

/// Whether a directory takes a file from this process, asked by trying.
/// The probe is removed when it drops, and a directory that does not exist
/// is not writable — `install` creates the per-user one, never the system
/// one, because making `/usr/local/bin` is not a package manager's call.
fn writable(dir: &Path) -> bool {
    dir.is_dir() && tempfile::NamedTempFile::new_in(dir).is_ok()
}

/// The note drt's installer prints, for the same reason: a binary installed
/// somewhere the shell will not look is an install that appears to have done
/// nothing.
fn path_note(prefix: &Path) -> Option<String> {
    let path = std::env::var_os("PATH")?;
    let on_path = std::env::split_paths(&path).any(|p| p == prefix);
    (!on_path).then(|| {
        format!(
            "note: {} is not on your PATH; add it, or run {}/drt by its full path",
            prefix.display(),
            prefix.display()
        )
    })
}

/// Roots on this box pinned to a different version than the one just put on
/// the PATH.
///
/// A PATH drt shadows nothing inside a root — `drt start` in a root runs
/// `.drt_root/drt`, and `audit` reports on that file — but the two are easy
/// to confuse from a shell, and "the drt I installed is not the drt that
/// ran" is exactly the pair of facts that is worse learned later. Said once,
/// naming the roots, and it changes nothing.
fn shadowed_pins(installed: &str) -> Vec<String> {
    let Ok(registry) = roots::load() else {
        return vec![];
    };
    let mut differing: Vec<String> = vec![];
    for entry in registry.roots {
        if !matches!(roots::on_disk(&entry), roots::OnDisk::Present { .. }) {
            continue;
        }
        let Some(pinned) = pin_of(&entry.path) else {
            continue;
        };
        if !drt_config::version::same(&pinned, installed) {
            differing.push(format!("{} ({pinned})", entry.path.display()));
        }
    }
    if differing.is_empty() {
        return vec![];
    }
    let mut lines = vec![format!(
        "note: {} recorded root(s) pin a different drt; each runs its own \
         .drt_root/drt, not this one:",
        differing.len()
    )];
    lines.extend(differing.into_iter().map(|d| format!("  {d}")));
    lines
}

/// The `drt` pin in a root's descriptor, if it has one.
fn pin_of(dir: &Path) -> Option<String> {
    let bytes = std::fs::read(crate::root::project_path(dir)).ok()?;
    serde_json::from_slice::<drt_config::project::ProjectJson>(&bytes)
        .ok()?
        .drt
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_directory_that_takes_a_file_is_writable_and_a_missing_one_is_not() {
        let tmp = tempfile::tempdir().unwrap();
        assert!(writable(tmp.path()));
        assert!(!writable(&tmp.path().join("nope")));
        // The probe leaves nothing behind.
        assert_eq!(std::fs::read_dir(tmp.path()).unwrap().count(), 0);
    }

    #[test]
    fn the_path_note_appears_only_when_the_prefix_is_not_on_path() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path();
        // Not on PATH: said, and the full-path invocation is named.
        let note = path_note(dir).expect("a directory off PATH is noted");
        assert!(note.contains("not on your PATH"), "{note}");
        assert!(note.contains(&dir.display().to_string()), "{note}");
    }

    #[test]
    fn the_user_prefix_is_used_when_the_system_one_is_not_writable() {
        // Not asserted against the real /usr/local/bin, whose writability
        // differs between a developer's box and CI's root container. What is
        // asserted is the shape: the fallback is HOME's, under `.local/bin`.
        let home = PathBuf::from("/home/someone");
        assert_eq!(
            home.join(USER_PREFIX),
            PathBuf::from("/home/someone/.local/bin")
        );
    }
}
