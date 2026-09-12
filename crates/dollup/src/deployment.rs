//! The deployment (SPEC.md §3): the directory a verb acts on. A **root** —
//! `.drt_root/project.json` carrying the sources, the code root at
//! `.drt_root/init/`, the lock beside the descriptor — or, until every verb
//! has moved, a `dollup.json` **app**. Verbs act on the current directory or
//! an explicit `--root PATH`; nothing is ever implicitly global.

use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};
use dollup_format::lock::LOCK_FILE;
use dollup_format::{Lockfile, SourceEntry};
use drt_config::project::{ProjectJson, INIT_DIR, ROOT_DIR};
use serde::{Deserialize, Serialize};

use crate::root;

pub const CONFIG_FILE: &str = "dollup.json";

/// The fields every verb reads. In a root they are fields of
/// `project.json`, projected here; in an app this is `dollup.json` itself.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Config {
    pub sources: Vec<SourceEntry>,
    /// With this set, an unsigned *network* source is an error at resolve
    /// time; `file://` sources are exempt.
    #[serde(default)]
    pub require_signatures: bool,
    /// The code root, relative to the deployment. DRT reads it; dollup
    /// writes it; nothing else should. A root's is `.drt_root/init`.
    #[serde(default = "default_code_root")]
    pub code_root: PathBuf,
}

fn default_code_root() -> PathBuf {
    "code".into()
}

/// Where a deployment's files are.
#[derive(Debug, Clone)]
pub enum Layout {
    /// `.drt_root/`: the descriptor carries the sources and
    /// `require_signatures`; `save` writes them back into it and nothing
    /// else, because the rest of the descriptor is declared by hand.
    Root { project: ProjectJson },
    /// A `dollup.json` app, kept readable until every verb has moved. `-c`
    /// and `DOLLUP_CONFIG` name one of these explicitly.
    Legacy { config_path: PathBuf },
}

pub struct Deployment {
    pub dir: PathBuf,
    pub layout: Layout,
    pub config: Config,
    pub lock: Lockfile,
}

impl Deployment {
    /// Which `dollup.json` a run uses when it is an app: the one the caller
    /// resolved, or `<deployment>/dollup.json`.
    ///
    /// The caller resolves `--config` over `DOLLUP_CONFIG` ([`from_env`]),
    /// so this stays a pure function and the precedence is testable
    /// without touching process environment. No home directory, no XDG
    /// lookup, no per-user state, nothing materialized on a first run. A
    /// tool that writes a file so it can read it back has not avoided
    /// depending on the file.
    pub fn config_path_for(dir: &Path, explicit: Option<&Path>) -> PathBuf {
        match explicit {
            Some(path) => path.to_path_buf(),
            None => dir.join(CONFIG_FILE),
        }
    }

    /// Is there anything here to open?
    pub fn exists(dir: &Path, explicit: Option<&Path>) -> bool {
        match explicit {
            Some(path) => path.exists(),
            None => root::exists(dir) || dir.join(CONFIG_FILE).exists(),
        }
    }

    /// A root when there is one and nothing names an app explicitly; the
    /// app otherwise.
    pub fn open(dir: &Path, explicit: Option<&Path>) -> Result<Deployment> {
        if explicit.is_none() && root::exists(dir) {
            return Deployment::open_root(dir);
        }
        Deployment::open_app(dir, Deployment::config_path_for(dir, explicit))
    }

    fn open_root(dir: &Path) -> Result<Deployment> {
        let path = root::project_path(dir);
        let project: ProjectJson = serde_json::from_slice(&fs::read(&path)?)
            .with_context(|| format!("{} does not parse", path.display()))?;
        // The descriptor holds sources as the JSON `dollup.json` held them,
        // and drt never interprets one; this is where they become typed.
        let sources = project
            .sources
            .iter()
            .map(|v| serde_json::from_value::<SourceEntry>(v.clone()))
            .collect::<Result<Vec<_>, _>>()
            .with_context(|| {
                format!(
                    "{}: a source is a url or {{\"url\", \"keys\"}}",
                    path.display()
                )
            })?;
        let config = Config {
            sources,
            require_signatures: project.require_signatures,
            code_root: PathBuf::from(ROOT_DIR).join(INIT_DIR),
        };
        let lock = read_lock(&root::lock_path(dir))?;
        Ok(Deployment {
            dir: dir.to_path_buf(),
            layout: Layout::Root { project },
            config,
            lock,
        })
    }

    fn open_app(dir: &Path, config_path: PathBuf) -> Result<Deployment> {
        let config: Config =
            serde_json::from_slice(&fs::read(&config_path).with_context(|| {
                format!(
                    "no root here — {} does not exist, and neither does {}.\n\n  \
                 start one:  dollup init",
                    root::project_path(dir).display(),
                    config_path.display()
                )
            })?)
            .with_context(|| format!("{} does not parse", config_path.display()))?;
        let lock = read_lock(&dir.join(LOCK_FILE))?;
        Ok(Deployment {
            dir: dir.to_path_buf(),
            layout: Layout::Legacy { config_path },
            config,
            lock,
        })
    }

    /// Scaffold. With nothing named explicitly this makes a root — the
    /// layout everything is moving to — through [`root::init`]; an explicit
    /// config path still makes the `dollup.json` app it names, for the
    /// callers that pass one.
    pub fn init(dir: &Path, explicit: Option<&Path>) -> Result<Deployment> {
        let Some(config_path) = explicit else {
            root::init(dir, None, None)?;
            return Deployment::open_root(dir);
        };
        if config_path.exists() {
            bail!("{} already exists", config_path.display());
        }
        fs::create_dir_all(dir)?;
        let deployment = Deployment {
            dir: dir.to_path_buf(),
            layout: Layout::Legacy {
                config_path: config_path.to_path_buf(),
            },
            config: Config {
                sources: vec![],
                require_signatures: true,
                code_root: default_code_root(),
            },
            lock: Lockfile::default(),
        };
        deployment.save()?;
        fs::create_dir_all(deployment.code_root())?;
        Ok(deployment)
    }

    pub fn save(&self) -> Result<()> {
        match &self.layout {
            Layout::Root { project } => {
                let mut project = project.clone();
                project.sources = self
                    .config
                    .sources
                    .iter()
                    .map(serde_json::to_value)
                    .collect::<Result<_, _>>()?;
                project.require_signatures = self.config.require_signatures;
                write_json(&root::project_path(&self.dir), &project)?;
                write_json(&root::lock_path(&self.dir), &self.lock)
            }
            Layout::Legacy { config_path } => {
                write_json(config_path, &self.config)?;
                write_json(&self.dir.join(LOCK_FILE), &self.lock)
            }
        }
    }

    pub fn code_root(&self) -> PathBuf {
        self.dir.join(&self.config.code_root)
    }

    /// dollup's blob store for this deployment. Beside `.drt_root/`, not
    /// in it: a root is self-contained without it — `verify` and `gc` are
    /// what read it — and it moves to `~/.dollup/cache/` with `pull`.
    pub fn store_dir(&self) -> PathBuf {
        self.dir.join(".dollup").join("store")
    }
}

fn read_lock(path: &Path) -> Result<Lockfile> {
    if !path.exists() {
        return Ok(Lockfile::default());
    }
    serde_json::from_slice(&fs::read(path)?)
        .with_context(|| format!("{} does not parse", path.display()))
}

/// `DOLLUP_CONFIG`, if it is set to something. An empty value is not a
/// path, and treating it as one would point every verb at a directory.
pub fn from_env() -> Option<PathBuf> {
    match std::env::var_os("DOLLUP_CONFIG") {
        Some(v) if !v.is_empty() => Some(PathBuf::from(v)),
        _ => None,
    }
}

pub fn write_json<T: Serialize>(path: &Path, value: &T) -> Result<()> {
    let mut bytes = serde_json::to_vec_pretty(value)?;
    bytes.push(b'\n');
    fs::write(path, bytes).with_context(|| format!("writing {}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_default_config_is_beside_the_deployment() {
        assert_eq!(
            Deployment::config_path_for(Path::new("/srv/app"), None),
            PathBuf::from("/srv/app/dollup.json")
        );
    }

    #[test]
    fn an_explicit_path_wins_and_is_taken_verbatim() {
        // Not joined onto the deployment dir: `-c` names a file, and a
        // relative one is relative to the caller's cwd like every other
        // path a shell hands over.
        assert_eq!(
            Deployment::config_path_for(Path::new("/srv/app"), Some(Path::new("/etc/d.json"))),
            PathBuf::from("/etc/d.json")
        );
        assert_eq!(
            Deployment::config_path_for(Path::new("/srv/app"), Some(Path::new("other.json"))),
            PathBuf::from("other.json")
        );
    }
}
