//! The deployment (SPEC.md §3): a directory holding the config, the
//! lockfile, the code root the runtime is pointed at, and the store.
//! Verbs act on the current directory or an explicit `--deployment PATH`;
//! nothing is ever implicitly global.

use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};
use dollup_format::lock::LOCK_FILE;
use dollup_format::{Lockfile, SourceEntry};
use serde::{Deserialize, Serialize};

pub const CONFIG_FILE: &str = "dollup.json";

/// `dollup.json` — the file the operator owns. The scaffold's standard
/// sources are ordinary lines here; deleting or replacing them is a
/// one-line edit that nothing resurrects.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Config {
    pub sources: Vec<SourceEntry>,
    /// With this set, an unsigned *network* source is an error at resolve
    /// time; `file://` sources are exempt.
    #[serde(default)]
    pub require_signatures: bool,
    /// The code root, relative to the deployment. DRT reads it; dollup
    /// writes it; nothing else should.
    #[serde(default = "default_code_root")]
    pub code_root: PathBuf,
}

fn default_code_root() -> PathBuf {
    "code".into()
}

pub struct Deployment {
    pub dir: PathBuf,
    /// Where the config was actually read from, so `save` writes back to
    /// the same file rather than to a `dollup.json` the caller never named.
    pub config_path: PathBuf,
    pub config: Config,
    pub lock: Lockfile,
}

impl Deployment {
    /// Which config file a run uses: the one the caller resolved, or
    /// `<deployment>/dollup.json`.
    ///
    /// The caller resolves `--config` over `DOLLUP_CONFIG` ([`from_env`]),
    /// so this stays a pure function and the precedence is testable
    /// without touching process environment. Three ways and no more — no
    /// home directory, no XDG lookup, no per-user state, nothing
    /// materialized on a first run. A tool that writes a file so it can
    /// read it back has not avoided depending on the file.
    pub fn config_path_for(dir: &Path, explicit: Option<&Path>) -> PathBuf {
        match explicit {
            Some(path) => path.to_path_buf(),
            None => dir.join(CONFIG_FILE),
        }
    }

    pub fn open(dir: &Path, explicit: Option<&Path>) -> Result<Deployment> {
        let config_path = Deployment::config_path_for(dir, explicit);
        let config: Config =
            serde_json::from_slice(&fs::read(&config_path).with_context(|| {
                format!(
                    "no app here yet — {} does not exist.\n\n  start one:  dollup init",
                    config_path.display()
                )
            })?)
            .with_context(|| format!("{} does not parse", config_path.display()))?;
        let lock_path = dir.join(LOCK_FILE);
        let lock = if lock_path.exists() {
            serde_json::from_slice(&fs::read(&lock_path)?)
                .with_context(|| format!("{} does not parse", lock_path.display()))?
        } else {
            Lockfile::default()
        };
        Ok(Deployment {
            dir: dir.to_path_buf(),
            config_path,
            config,
            lock,
        })
    }

    /// Scaffold (SPEC.md §10). The standard sources go in when they carry a
    /// real key; until the standard repo is signed and live, `init` writes
    /// an empty source list rather than a route that does not resolve or a
    /// key that does not exist.
    pub fn init(dir: &Path, explicit: Option<&Path>) -> Result<Deployment> {
        let config_path = Deployment::config_path_for(dir, explicit);
        if config_path.exists() {
            bail!("{} already exists", config_path.display());
        }
        fs::create_dir_all(dir)?;
        let config = Config {
            sources: vec![],
            require_signatures: true,
            code_root: default_code_root(),
        };
        let deployment = Deployment {
            dir: dir.to_path_buf(),
            config_path,
            config,
            lock: Lockfile::default(),
        };
        deployment.save()?;
        fs::create_dir_all(deployment.code_root())?;
        Ok(deployment)
    }

    pub fn save(&self) -> Result<()> {
        write_json(&self.config_path, &self.config)?;
        write_json(&self.dir.join(LOCK_FILE), &self.lock)
    }

    pub fn code_root(&self) -> PathBuf {
        self.dir.join(&self.config.code_root)
    }

    pub fn store_dir(&self) -> PathBuf {
        self.dir.join(".dollup").join("store")
    }
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
