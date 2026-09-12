//! The `.drt_root/` layout, and `dollup init`, which writes it.
//!
//! A root is a runtime at a path: `.drt_root/` claims that exact path and
//! holds the descriptor (`project.json`), the operator's consent to its
//! ceiling (`consent.json`), the profiles, the delivered content (`init/`),
//! what runs (`live/`), the logs, the runtime's own state, and dollup's
//! lock. `dlua/` beside it is local authoring, by convention. What init
//! writes is what the rest of dollup, and drt, read.
//!
//! Init creates what is missing and never rewrites what exists. That is the
//! whole rule, applied uniformly: a fresh directory gets everything; a root
//! that already has a descriptor gets the profile it lacked and nothing
//! else touched. dollup may create a config at root-creation time; it never
//! edits one that exists. The descriptor is the one file a later verb
//! writes — `pin`, `source add`, and init declaring a profile it has just
//! created — because it is a descriptor and not a profile.

use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{bail, Context, Result};
use dollup_format::lock::LOCK_FILE;
use dollup_format::{Lockfile, SourceEntry};
use drt_config::consent::{Accepted, ConsentJson};
use drt_config::id::Uuid7;
use drt_config::project::{self, ProfileName, ProjectJson, PROFILE_SUFFIX, ROOT_DIR};
use drt_config::realm::Realm;
use drt_config::time::Timestamp;

use crate::deployment::{write_json, CONFIG_FILE};

/// The standard source `init` scaffolds. SPEC.md §1 is precise about what
/// this is and is not: the binary knows no URLs *at resolve time* — this is
/// a line written into a file the operator owns, which they can delete or
/// replace, and nothing resurrects it.
pub const STD_REPO_URL: &str = "https://dollup.aloecraft.org/std-repo/";

/// The standard repo's signing key, minted 2026-09-03. This is what turns a
/// first run into two commands with nothing to read first: `init` scaffolds
/// the standard source with this key pinned, and `add hello` just works.
/// It must match site/std-repo.pub, which `dollup repo publish` derives from
/// the private key -- the page, the scaffold and the signature cannot drift.
pub const STD_REPO_KEY: Option<&str> = Some("ed25519:RZNTaXSePtutwF3IWX49hppum4O8DdCiyx7BcYSmrRc=");

/// The standard repo's peer (RepoFormat.md §2): GitHub's zipball of the
/// same tree, under the same key, so a root has the standard packages when
/// the served copy is down and the served copy when GitHub is. Scaffolded
/// only once `drt-std-lib`'s main carries `index.json.sig`: a keyed source
/// whose tree has no signature is refused, not skipped, and would stop
/// every pull. Set this to
/// `Some("zip+https://github.com/Aloecraft-org/drt-std-lib/archive/refs/heads/main.zip")`
/// the day it is signed.
pub const STD_REPO_ZIP: Option<&str> = None;

/// The profile init writes when none is named.
pub const DEFAULT_PROFILE: &str = "debug";
/// Local authoring, by convention: the default profile's `dlua_dir`.
pub const DLUA_DIR: &str = "dlua/";
/// The default profile's entry, under `dlua/`.
pub const ENTRY: &str = "app.dlua";
/// The profile that runs `stdlib:preflight`: how a user confirms a root is
/// set up right with nothing but drt on the box.
pub const PREFLIGHT: &str = "preflight";

/// The default profile. Its caps are the whole ceiling init declares, so
/// the two attenuate trivially; a project widens both on purpose, and that
/// is the edit consent notices.
const PROFILE_JSON: &str = r#"{
  "dlua_dir": "dlua/",
  "entry": "app.dlua",
  "caps": [
    { "capability": "host:time" }
  ],
  "args": {
    "verbose": false
  }
}
"#;

const PREFLIGHT_JSON: &str = r#"{
  "entry": "stdlib:preflight"
}
"#;

pub fn root_dir(dir: &Path) -> PathBuf {
    dir.join(ROOT_DIR)
}

pub fn project_path(dir: &Path) -> PathBuf {
    root_dir(dir).join("project.json")
}

pub fn consent_path(dir: &Path) -> PathBuf {
    root_dir(dir).join("consent.json")
}

pub fn lock_path(dir: &Path) -> PathBuf {
    root_dir(dir).join(LOCK_FILE)
}

pub fn profile_dir(dir: &Path) -> PathBuf {
    root_dir(dir).join(project::PROFILE_DIR)
}

/// Is there a root here? The descriptor is what makes one: `.drt_root/`
/// with no `project.json` is a hand-made root on the fallback order, which
/// init fills in rather than refuses.
pub fn exists(dir: &Path) -> bool {
    project_path(dir).is_file()
}

/// `dollup init [name] [profile]`: what was written and what was kept, one
/// line per file, for printing.
pub fn init(dir: &Path, name: Option<&str>, profile: Option<&str>) -> Result<Vec<String>> {
    // Everything that can refuse, before anything is written.
    if dir.join(CONFIG_FILE).exists() {
        bail!(
            "{} holds a dollup.json app, and a root and an app do not share a directory; \
             start the root elsewhere",
            dir.display()
        );
    }
    if let Some(name) = name {
        if let Some(reserved) = dollup_format::reserved::reserved(name) {
            bail!("{}", dollup_format::reserved::refusal(name, reserved));
        }
    }
    let profile = match profile {
        None => ProfileName::new(DEFAULT_PROFILE)?,
        Some(p) if p.ends_with(PROFILE_SUFFIX) => ProfileName::from_filename(p)?,
        Some(p) => ProfileName::new(p)?,
    };
    let preflight = ProfileName::new(PREFLIGHT)?;
    let (unix_ms, unix_secs) = now()?;

    let mut done = vec![];
    fs::create_dir_all(dir)?;
    for sub in [
        project::INIT_DIR,
        project::LIVE_DIR,
        project::LOG_DIR,
        project::PROFILE_DIR,
        project::STATE_DIR,
    ] {
        fs::create_dir_all(root_dir(dir).join(sub))?;
    }
    fs::create_dir_all(dir.join(DLUA_DIR))?;

    // The descriptor: minted once, or read and left as it is.
    let (mut project, fresh) = match read_project(dir)? {
        Some(existing) => (existing, false),
        None => {
            let mut project = ProjectJson::new(mint_root_id(unix_ms)?);
            project.project_version = Some("0.0.0".into());
            project.caps =
                serde_json::from_value(serde_json::json!([{ "capability": "host:time" }]))?;
            project.sources = std_sources()?;
            project.require_signatures = true;
            (project, true)
        }
    };
    let mut changed = fresh;
    match (&project.project_name, name) {
        (None, Some(name)) => {
            project.project_name = Some(name.to_string());
            changed = true;
        }
        (Some(have), Some(want)) if have != want => done.push(format!(
            "kept  project_name {have:?} (init never renames; edit {} to)",
            rel(dir, &project_path(dir))
        )),
        _ => {}
    }

    // The profile asked for and preflight: written if absent, declared if
    // not declared. The named one is the default when nothing is yet.
    let mut wrote_profile = false;
    for (pname, content, default) in [
        (&profile, PROFILE_JSON, true),
        (&preflight, PREFLIGHT_JSON, false),
    ] {
        let filename = pname.filename();
        let path = profile_dir(dir).join(&filename);
        if path.exists() {
            done.push(format!("kept  {}", rel(dir, &path)));
        } else {
            fs::write(&path, content).with_context(|| format!("writing {}", path.display()))?;
            done.push(format!("wrote {}", rel(dir, &path)));
            wrote_profile |= default;
        }
        if !project.profiles.iter().any(|f| f == &filename) {
            project.profiles.push(filename);
            changed = true;
        }
        if default && project.default_profile.is_none() {
            project.default_profile = Some(pname.as_str().to_string());
            changed = true;
        }
    }

    // The entry the profile just written names. A profile that was already
    // there names whatever it names, and its entry is not touched.
    let entry = dir.join(DLUA_DIR).join(ENTRY);
    if wrote_profile {
        if entry.exists() {
            done.push(format!("kept  {}", rel(dir, &entry)));
        } else {
            let label = match &project.project_name {
                Some(name) => name.clone(),
                None => dir
                    .canonicalize()
                    .ok()
                    .and_then(|p| p.file_name().map(|n| n.to_string_lossy().into_owned()))
                    .unwrap_or_else(|| "a drt project".into()),
            };
            fs::write(&entry, hello(&label, unix_secs))?;
            done.push(format!("wrote {}", rel(dir, &entry)));
        }
    }

    // Consent, to the ceiling this init just declared and no other: a
    // locally authored root never prompts on its first start, and its own
    // ceiling widening is what exercises the delta path. A root that
    // already had a descriptor keeps whatever consent it has; accepting a
    // ceiling someone else declared is `dollup consent`'s act, not init's.
    let consent = consent_path(dir);
    if fresh {
        if consent.exists() {
            done.push(format!(
                "kept  {} (not this root's — its root_id was just minted; audit will say so)",
                rel(dir, &consent)
            ));
        } else {
            let ceiling_hash = project::ceiling_hash(&project)?;
            let consent_json = ConsentJson {
                root_id: project.root_id,
                accepted: vec![Accepted::Listed {
                    realm: Realm::root(),
                    ceiling_hash,
                    ceiling: project.caps.clone(),
                    accepted_at: Timestamp::from_unix_secs(unix_secs),
                }],
                signers: vec![],
            };
            write_json(&consent, &consent_json)?;
            done.push(format!("wrote {}", rel(dir, &consent)));
        }
    }

    let lock = lock_path(dir);
    if !lock.exists() {
        write_json(&lock, &Lockfile::default())?;
        done.push(format!("wrote {}", rel(dir, &lock)));
    }

    let descriptor = project_path(dir);
    if changed {
        write_json(&descriptor, &project)?;
        done.insert(
            0,
            format!(
                "{} {}",
                if fresh { "wrote" } else { "edited" },
                rel(dir, &descriptor)
            ),
        );
    } else {
        done.insert(0, format!("kept  {}", rel(dir, &descriptor)));
    }
    crate::roots::register(dir, project.root_id);
    Ok(done)
}

fn read_project(dir: &Path) -> Result<Option<ProjectJson>> {
    let path = project_path(dir);
    if !path.is_file() {
        return Ok(None);
    }
    let bytes = fs::read(&path)?;
    Ok(Some(serde_json::from_slice(&bytes).with_context(|| {
        format!("{} does not parse", path.display())
    })?))
}

/// The scaffold's standard source, as `project.json` carries sources: the
/// same JSON `dollup.json` held, which drt never interprets.
fn std_sources() -> Result<Vec<serde_json::Value>> {
    let Some(key) = STD_REPO_KEY else {
        return Ok(vec![]);
    };
    let mut sources = vec![];
    for url in [Some(STD_REPO_URL), STD_REPO_ZIP].into_iter().flatten() {
        sources.push(serde_json::to_value(SourceEntry::Signed {
            url: url.into(),
            keys: vec![key.into()],
        })?);
    }
    Ok(sources)
}

/// `root_id`: minted, random, never derived from content. It is the hinge
/// shipping turns on, so two roots must not be able to collide by having
/// the same files.
pub(crate) fn mint_root_id(unix_ms: u64) -> Result<Uuid7> {
    let mut random = [0u8; 10];
    getrandom::getrandom(&mut random).context("no entropy for a root_id")?;
    Ok(Uuid7::mint(unix_ms, random))
}

pub(crate) fn now() -> Result<(u64, i64)> {
    let since = SystemTime::now().duration_since(UNIX_EPOCH)?;
    Ok((since.as_millis() as u64, since.as_secs() as i64))
}

fn hello(label: &str, unix_secs: i64) -> String {
    format!(
        "--- {label} created at {}\n\nprint(\"Hello from app.dlua\")\n",
        Timestamp::from_unix_secs(unix_secs)
    )
}

/// A path as the report shows it: relative to the root's directory.
fn rel(dir: &Path, path: &Path) -> String {
    path.strip_prefix(dir)
        .unwrap_or(path)
        .to_string_lossy()
        .replace('\\', "/")
}
