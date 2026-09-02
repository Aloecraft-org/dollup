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
    pub config: Config,
    pub lock: Lockfile,
}

impl Deployment {
    pub fn open(dir: &Path) -> Result<Deployment> {
        let config_path = dir.join(CONFIG_FILE);
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
            config,
            lock,
        })
    }

    /// Scaffold (SPEC.md §10). The standard sources go in when they carry a
    /// real key; until the standard repo is signed and live, `init` writes
    /// an empty source list rather than a route that does not resolve or a
    /// key that does not exist.
    pub fn init(dir: &Path) -> Result<Deployment> {
        let config_path = dir.join(CONFIG_FILE);
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
            config,
            lock: Lockfile::default(),
        };
        deployment.save()?;
        fs::create_dir_all(deployment.code_root())?;
        Ok(deployment)
    }

    pub fn save(&self) -> Result<()> {
        write_json(&self.dir.join(CONFIG_FILE), &self.config)?;
        write_json(&self.dir.join(LOCK_FILE), &self.lock)
    }

    pub fn code_root(&self) -> PathBuf {
        self.dir.join(&self.config.code_root)
    }

    pub fn store_dir(&self) -> PathBuf {
        self.dir.join(".dollup").join("store")
    }
}

pub fn write_json<T: Serialize>(path: &Path, value: &T) -> Result<()> {
    let mut bytes = serde_json::to_vec_pretty(value)?;
    bytes.push(b'\n');
    fs::write(path, bytes).with_context(|| format!("writing {}", path.display()))
}
