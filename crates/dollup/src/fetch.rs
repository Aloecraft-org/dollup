//! Fetching: turn a source URL into readable repo bytes. Four schemes, one
//! interface, and the interface is deliberately dumb — read the index, read
//! its signature, read a path — because that is all a repo is.
//!
//! v1 keeps two honest shortcuts. `git+` shells out to the `git` binary
//! rather than linking a git implementation (the machines this runs on have
//! git; a vendored libgit2 would dwarf the rest of the binary). And the
//! archive and git schemes fetch the whole tree even when one package is
//! wanted — only `https` and `file` read incrementally. Both are recorded
//! here rather than discovered later.

use std::fs;
use std::io::Read;
use std::path::PathBuf;
use std::process::Command;

use anyhow::{bail, Context, Result};
use dollup_format::index::{INDEX_FILE, SIG_FILE};
use dollup_format::source::{Scheme, Transport};

pub struct Fetched {
    kind: Kind,
    /// For git sources: the commit the mutable input resolved to.
    pub commit: Option<String>,
}

enum Kind {
    Dir(PathBuf, #[allow(dead_code)] Option<tempfile::TempDir>),
    Http(String),
}

const MAX_FETCH: u64 = 256 * 1024 * 1024;

pub fn fetch(url: &str) -> Result<Fetched> {
    match Scheme::of(url)? {
        Scheme::Plain(Transport::File) => Ok(Fetched {
            kind: Kind::Dir(existing_dir(file_path(url)?)?, None),
            commit: None,
        }),
        Scheme::Plain(Transport::Https) => Ok(Fetched {
            kind: Kind::Http(url.trim_end_matches('/').to_string()),
            commit: None,
        }),
        Scheme::Zip(transport) => {
            let inner = &url["zip+".len()..];
            let bytes = match transport {
                Transport::File => {
                    fs::read(file_path(inner)?).with_context(|| format!("reading {inner}"))?
                }
                Transport::Https => http_get(inner)?,
            };
            let dir = extract_zip(&bytes)?;
            Ok(Fetched {
                kind: Kind::Dir(repo_root(dir.path().to_path_buf()), Some(dir)),
                commit: None,
            })
        }
        Scheme::Git(_) => {
            let inner = &url["git+".len()..];
            let dir = tempfile::tempdir()?;
            let out = Command::new("git")
                .args(["clone", "--depth", "1", "--quiet", inner])
                .arg(dir.path().join("checkout"))
                .output()
                .context("running git (the git+ scheme shells out to the git binary)")?;
            if !out.status.success() {
                bail!(
                    "git clone {inner} failed: {}",
                    String::from_utf8_lossy(&out.stderr).trim()
                );
            }
            let rev = Command::new("git")
                .args(["-C"])
                .arg(dir.path().join("checkout"))
                .args(["rev-parse", "HEAD"])
                .output()?;
            let commit = String::from_utf8_lossy(&rev.stdout).trim().to_string();
            Ok(Fetched {
                kind: Kind::Dir(dir.path().join("checkout"), Some(dir)),
                commit: Some(commit),
            })
        }
    }
}

impl Fetched {
    pub fn read(&self, rel: &str) -> Result<Option<Vec<u8>>> {
        if rel.split('/').any(|c| c == ".." || c.is_empty()) {
            bail!("'{rel}' is not a repo-relative path");
        }
        match &self.kind {
            Kind::Dir(root, _) => match fs::read(root.join(rel)) {
                Ok(b) => Ok(Some(b)),
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
                Err(e) => Err(e.into()),
            },
            Kind::Http(base) => match crate::http::agent().get(&format!("{base}/{rel}")).call() {
                Ok(resp) => {
                    let mut bytes = vec![];
                    resp.into_reader().take(MAX_FETCH).read_to_end(&mut bytes)?;
                    Ok(Some(bytes))
                }
                Err(ureq::Error::Status(404, _)) => Ok(None),
                Err(e) => Err(e.into()),
            },
        }
    }

    pub fn index_bytes(&self) -> Result<Vec<u8>> {
        // Name the place. "not a dollup repo" is true of a directory that
        // is not one AND of a path that is not there at all, and those have
        // different fixes -- `existing_dir` separates the second out before
        // this can be reached, so what lands here really is a directory
        // missing its index.
        self.read(INDEX_FILE)?.with_context(|| match &self.kind {
            Kind::Dir(root, _) => format!(
                "{} has no {INDEX_FILE} — it is a directory, but not a dollup repo. \
                 A publisher makes one with `dollup repo index <dir>`.",
                root.display()
            ),
            Kind::Http(base) => format!(
                "{base}/{INDEX_FILE} is not there — that URL is reachable but is not a dollup repo"
            ),
        })
    }

    pub fn sig_bytes(&self) -> Result<Option<Vec<u8>>> {
        self.read(SIG_FILE)
    }
}

/// A `file://` source that is not there is the single most likely thing to
/// go wrong with one, and it used to answer "source has no index.json —
/// not a dollup repo": true, unhelpful, and pointing at the wrong fix. Say
/// which path, and which of the two problems it is.
fn existing_dir(path: PathBuf) -> Result<PathBuf> {
    if !path.exists() {
        bail!(
            "{} does not exist — a file:// source names a directory on this machine",
            path.display()
        );
    }
    if !path.is_dir() {
        bail!(
            "{} is a file, not a directory — a file:// source names the repo directory \
             (for an archive, the scheme is `zip+file://`)",
            path.display()
        );
    }
    Ok(path)
}

fn file_path(url: &str) -> Result<PathBuf> {
    Ok(PathBuf::from(url.strip_prefix("file://").with_context(
        || format!("'{url}' is not a file:// url"),
    )?))
}

fn http_get(url: &str) -> Result<Vec<u8>> {
    let resp = crate::http::agent()
        .get(url)
        .call()
        .with_context(|| format!("GET {url}"))?;
    let mut bytes = vec![];
    resp.into_reader().take(MAX_FETCH).read_to_end(&mut bytes)?;
    Ok(bytes)
}

fn extract_zip(bytes: &[u8]) -> Result<tempfile::TempDir> {
    let dir = tempfile::tempdir()?;
    let mut archive = zip::ZipArchive::new(std::io::Cursor::new(bytes))?;
    archive.extract(dir.path()).context("extracting archive")?;
    Ok(dir)
}

/// A forge zipball wraps the tree in one `repo-ref/` directory; when the
/// root holds no index and exactly one directory, descend into it.
fn repo_root(dir: PathBuf) -> PathBuf {
    if dir.join(INDEX_FILE).exists() {
        return dir;
    }
    let entries: Vec<_> = fs::read_dir(&dir).into_iter().flatten().flatten().collect();
    match entries.as_slice() {
        [only] if only.path().is_dir() => only.path(),
        _ => dir,
    }
}
