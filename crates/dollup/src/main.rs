//! `dollup` — fetcher and resolver over a content-addressed store.
//! Install is inert; config is authority; materializing files is the last
//! act. SPEC.md is the map.

mod deployment;
mod fetch;
mod ops;
mod repo;
mod snap;
mod store;

use std::path::PathBuf;

use anyhow::Result;
use clap::{Parser, Subcommand};
use dollup_format::source::Ref;

use deployment::Deployment;

#[derive(Parser)]
#[command(name = "dollup", version, about)]
struct Cli {
    /// The deployment directory (default: the current directory). Nothing
    /// is ever implicitly global.
    #[arg(long, global = true)]
    deployment: Option<PathBuf>,
    #[command(subcommand)]
    verb: Verb,
}

#[derive(Subcommand)]
enum Verb {
    /// Scaffold a deployment: config, empty lockfile, code root.
    Init,
    /// Fetch a package (and its dependencies), lock, populate. Inert.
    Add {
        /// `name`, `name@^1.2`, or `<source-url>#name@^1.2`.
        r#ref: String,
        /// Also materialize wasm host faces (component, js).
        #[arg(long)]
        with_host: bool,
        /// Also materialize native host faces. Installing one is the same
        /// class of act as `apt install`: nothing the runtime holds bounds
        /// what it does.
        #[arg(long)]
        with_host_native: bool,
    },
    /// What the lock holds.
    Ls,
    /// Describe a package as the sources see it, without adding it.
    Info { r#ref: String },
    /// Re-hash the code root and the store against the lock.
    Verify,
    /// Sweep the store against the lock.
    Gc,
    /// Push a snapshot blob to a remote. Snapshots are private by default:
    /// a non-file remote takes --export-state, said out loud.
    Push {
        remote: String,
        /// The snapshot blob (e.g. a .dvsnap from DRT's snapshot store).
        blob: PathBuf,
        /// Snapshot name at the remote (default: the blob's file stem).
        #[arg(long)]
        name: Option<String>,
        /// Pin the code-set from this locked package's guest face.
        #[arg(long, conflicts_with = "code_set")]
        package: Option<String>,
        /// Pin the code-set outright (sha256:…), when the snapshot came
        /// from elsewhere.
        #[arg(long)]
        code_set: Option<String>,
        /// The host identity stamp, verbatim from `dv_snapshot`.
        #[arg(long)]
        identity: Option<String>,
        /// Generic capability names the guest expects at restore. Repeat.
        #[arg(long = "capability")]
        capabilities: Vec<String>,
        /// The DV_ABI_VERSION the blob was captured under.
        #[arg(long)]
        dv_abi: Option<String>,
        /// Acknowledge pushing live state off this machine: a snapshot blob
        /// is the instance's entire heap (THREAT-NOTES.md).
        #[arg(long)]
        export_state: bool,
    },
    /// Pull a snapshot: manifest, blob, and the pinned code-set — fetched
    /// by identity from the sources if absent. Restore stays DRT's verb.
    Pull { remote: String, name: String },
    /// Publisher-side verbs: seal, index, sign, blobs, keygen.
    #[command(subcommand)]
    Repo(RepoVerb),
    /// The deployment's source list.
    #[command(subcommand)]
    Source(SourceVerb),
}

#[derive(Subcommand)]
enum SourceVerb {
    /// Add a source, optionally pinning the key that must sign its index.
    Add {
        url: String,
        /// The publisher's public key, `ed25519:<base64>`.
        #[arg(long, conflicts_with = "key_file")]
        key: Option<String>,
        /// Read the public key from a file (e.g. a `.pub` from keygen).
        #[arg(long)]
        key_file: Option<PathBuf>,
    },
    /// List the sources, and whether each is signed.
    Ls,
    /// Remove a source by url.
    Rm { url: String },
}

#[derive(Subcommand)]
enum RepoVerb {
    /// Hash a package's files into its manifest and validate it. Run this
    /// after editing a package, before `index`.
    Seal { dir: PathBuf },
    /// Scan packages/, validate, write index.json (dropping any stale
    /// signature).
    Index { dir: PathBuf },
    /// Sign index.json with a private key file; writes index.json.sig.
    Sign {
        dir: PathBuf,
        #[arg(long)]
        key_file: PathBuf,
    },
    /// Generate the blobs/ projection for a static mirror.
    Blobs { dir: PathBuf },
    /// Generate a keypair. With --out, nothing sensitive touches the
    /// terminal; without it BOTH KEYS PRINT — redirect line 1 (private)
    /// somewhere safe. The public line is what source entries pin.
    Keygen {
        /// Write `<PREFIX>` (private, mode 0600) and `<PREFIX>.pub`
        /// instead of printing; only the public key is echoed.
        #[arg(long)]
        out: Option<PathBuf>,
    },
}

/// The standard source `dollup init` scaffolds. SPEC.md §1 is precise about
/// what this is and is not: the binary knows no URLs *at resolve time* — this
/// is a line written into a file the operator owns, which they can delete or
/// replace, and nothing resurrects it.
const STD_REPO_URL: &str = "https://dollup.aloecraft.org/std-repo/";

/// `None` until the standard repo's key is minted (std-repo/README.md).
/// Filling this in is what turns a first run into two commands with nothing
/// to read first, so it is worth doing the day the key exists.
const STD_REPO_KEY: Option<&str> = None;

fn main() -> Result<()> {
    let cli = Cli::parse();
    let dir = cli.deployment.unwrap_or_else(|| PathBuf::from("."));
    match cli.verb {
        Verb::Init => {
            let mut d = Deployment::init(&dir)?;
            // Someone who just typed `dollup init` wants to install
            // something, not to learn what a source is. Where the standard
            // key exists, scaffold it in and hand them a command that works;
            // where it does not, still hand them commands rather than a file
            // to go edit.
            println!("Created a deployment in {}", d.dir.display());
            println!();
            match STD_REPO_KEY {
                Some(key) => {
                    d.config.sources.push(dollup_format::SourceEntry::Signed {
                        url: STD_REPO_URL.into(),
                        keys: vec![key.into()],
                    });
                    d.save()?;
                    println!("  dollup add hello     install a program");
                    println!("  dollup ls            see what is installed");
                }
                None => {
                    println!("Add somewhere to install from, then install:");
                    println!();
                    println!("  dollup source add <url> --key <key>");
                    println!("  dollup add <name>");
                }
            }
        }
        Verb::Add {
            r#ref,
            with_host,
            with_host_native,
        } => {
            let mut d = Deployment::open(&dir)?;
            let r: Ref = r#ref.parse()?;
            let gates = ops::HostGates {
                with_host: with_host || with_host_native,
                with_host_native,
            };
            for line in ops::add(&mut d, &r, gates)? {
                println!("{line}");
            }
        }
        Verb::Ls => {
            let d = Deployment::open(&dir)?;
            for (name, p) in &d.lock.packages {
                println!(
                    "{name} {} ({}) ← {}",
                    p.version,
                    p.signed_by
                        .as_deref()
                        .map(|_| "signed")
                        .unwrap_or("unsigned"),
                    p.source
                );
            }
        }
        Verb::Info { r#ref } => {
            let d = Deployment::open(&dir)?;
            let r: Ref = r#ref.parse()?;
            let entries = match &r.source {
                Some(url) => vec![dollup_format::SourceEntry::Url(url.clone())],
                None => d.config.sources.clone(),
            };
            for entry in &entries {
                let opened = ops::open_source(entry, d.config.require_signatures)?;
                if let Some((v, e)) = opened.index.select(&r.name, r.version.as_ref()) {
                    println!("{} {v} ← {}", r.name, entry.url());
                    println!("  faces: {:?}  targets: {:?}", e.faces, e.targets);
                    println!(
                        "  {}",
                        match &e.code_set {
                            Some(_) if e.runnable => "a program: dollup add it, then run it",
                            Some(_) => "a library: other packages require it",
                            None => "no guest code",
                        }
                    );
                    println!("  package: {}", e.package_id);
                    if let Some(cs) = &e.code_set {
                        println!("  code-set: {cs}");
                    }
                    // The contract is the unit of trust review: show it in
                    // full before any host face is fetched or admitted.
                    let rel = format!("{}/manifest.json", e.path);
                    if let Some(bytes) = opened.fetched.read(&rel)? {
                        if let Ok(m) = serde_json::from_slice::<dollup_format::Manifest>(&bytes) {
                            for (cap, decl) in &m.capability {
                                println!(
                                    "  defines {cap} (scope: {}, shape {}, contract {})",
                                    decl.scope_type,
                                    decl.shape,
                                    decl.contract_id()
                                );
                                println!("    calls: {}", decl.calls.join(", "));
                            }
                            if !m.requires.capabilities.is_empty() {
                                println!(
                                    "  requires capabilities: {}",
                                    m.requires.capabilities.join(", ")
                                );
                            }
                            if !m.requires.connectors.is_empty() {
                                let list: Vec<String> = m
                                    .requires
                                    .connectors
                                    .iter()
                                    .map(|(n, v)| format!("{n} {v}"))
                                    .collect();
                                println!("  requires connectors: {}", list.join(", "));
                            }
                        }
                    }
                    return Ok(());
                }
            }
            anyhow::bail!("'{}' is in none of the sources", r.name);
        }
        Verb::Verify => {
            let d = Deployment::open(&dir)?;
            let problems = ops::verify(&d)?;
            if problems.is_empty() {
                println!("clean: {} package(s) match the lock", d.lock.packages.len());
            } else {
                for p in &problems {
                    eprintln!("{p}");
                }
                anyhow::bail!("{} problem(s)", problems.len());
            }
        }
        Verb::Gc => {
            let d = Deployment::open(&dir)?;
            println!("swept {} blob(s)", ops::gc(&d)?);
        }
        Verb::Push {
            remote,
            blob,
            name,
            package,
            code_set,
            identity,
            capabilities,
            dv_abi,
            export_state,
        } => {
            let mut d = Deployment::open(&dir)?;
            let line = snap::push(
                &mut d,
                &remote,
                snap::PushSpec {
                    blob_path: blob,
                    name,
                    package,
                    code_set,
                    identity,
                    capabilities,
                    dv_abi,
                    export_state,
                },
            )?;
            println!("{line}");
        }
        Verb::Pull { remote, name } => {
            let mut d = Deployment::open(&dir)?;
            for line in snap::pull(&mut d, &remote, &name)? {
                println!("{line}");
            }
        }
        Verb::Source(v) => {
            let mut d = Deployment::open(&dir)?;
            match v {
                SourceVerb::Add { url, key, key_file } => {
                    dollup_format::source::Scheme::of(&url)?;
                    if d.config.sources.iter().any(|e| e.url() == url) {
                        anyhow::bail!("{url} is already a source");
                    }
                    let key = match (key, key_file) {
                        (Some(k), _) => Some(k),
                        (None, Some(path)) => {
                            Some(std::fs::read_to_string(&path)?.trim().to_string())
                        }
                        (None, None) => None,
                    };
                    let entry = match key {
                        Some(k) => dollup_format::SourceEntry::Signed {
                            url: url.clone(),
                            keys: vec![k],
                        },
                        None => dollup_format::SourceEntry::Url(url.clone()),
                    };
                    let signed = !entry.keys().is_empty();
                    d.config.sources.push(entry);
                    d.save()?;
                    println!(
                        "added {url} ({})",
                        if signed { "signed" } else { "unsigned" }
                    );
                    if !signed {
                        eprintln!(
                            "note: unsigned — with require_signatures set, a network source \
                             without a pinned key is refused at resolve time"
                        );
                    }
                }
                SourceVerb::Ls => {
                    for e in &d.config.sources {
                        match e.keys() {
                            [] => println!("{}  (unsigned)", e.url()),
                            keys => println!("{}  {}", e.url(), keys.join(" ")),
                        }
                    }
                }
                SourceVerb::Rm { url } => {
                    let before = d.config.sources.len();
                    d.config.sources.retain(|e| e.url() != url);
                    if d.config.sources.len() == before {
                        anyhow::bail!("{url} is not a source");
                    }
                    d.save()?;
                    println!("removed {url}");
                }
            }
        }
        Verb::Repo(v) => match v {
            RepoVerb::Seal { dir } => {
                for line in repo::seal(&dir)? {
                    println!("{line}");
                }
            }
            RepoVerb::Index { dir } => {
                let idx = repo::index(&dir)?;
                println!("indexed {} package(s)", idx.packages.len());
            }
            RepoVerb::Sign { dir, key_file } => {
                repo::sign_index(&dir, &key_file)?;
                println!("signed");
            }
            RepoVerb::Blobs { dir } => {
                println!("projected {} blob(s)", repo::blobs(&dir)?);
            }
            RepoVerb::Keygen { out } => {
                let (private, public) = dollup_format::sign::keygen();
                match out {
                    Some(prefix) => {
                        use anyhow::Context;
                        use std::io::Write;
                        use std::os::unix::fs::OpenOptionsExt;
                        if let Some(parent) = prefix.parent().filter(|p| !p.as_os_str().is_empty())
                        {
                            std::fs::create_dir_all(parent)
                                .with_context(|| format!("creating {}", parent.display()))?;
                        }
                        let mut f = std::fs::OpenOptions::new()
                            .write(true)
                            .create_new(true)
                            .mode(0o600)
                            .open(&prefix)
                            .with_context(|| {
                                format!(
                                    "creating {} — a key already there is never overwritten",
                                    prefix.display()
                                )
                            })?;
                        writeln!(f, "{private}")?;
                        let pub_path = prefix.with_extension("pub");
                        std::fs::write(&pub_path, format!("{public}\n"))?;
                        println!("{public}");
                        eprintln!(
                            "private key: {} (0600) — public: {}",
                            prefix.display(),
                            pub_path.display()
                        );
                    }
                    None => {
                        println!("{private}");
                        println!("{public}");
                        eprintln!("both keys printed (line 1 is PRIVATE) — prefer --out <prefix>");
                    }
                }
            }
        },
    }
    Ok(())
}
