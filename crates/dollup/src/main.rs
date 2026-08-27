//! `dollup` — fetcher and resolver over a content-addressed store.
//! Install is inert; config is authority; materializing files is the last
//! act. SPEC.md is the map.

mod deployment;
mod fetch;
mod ops;
mod repo;
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
    /// Publisher-side verbs: index, sign, blobs, keygen.
    #[command(subcommand)]
    Repo(RepoVerb),
}

#[derive(Subcommand)]
enum RepoVerb {
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
    /// Generate a keypair. The private key goes to stdout line 1, public
    /// line 2 — redirect accordingly, and pin the public line in source
    /// entries.
    Keygen,
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    let dir = cli.deployment.unwrap_or_else(|| PathBuf::from("."));
    match cli.verb {
        Verb::Init => {
            let d = Deployment::init(&dir)?;
            println!("deployment scaffolded at {}", d.dir.display());
            println!("sources: none — add them to dollup.json (an empty list resolves nothing)");
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
                    println!("  package: {}", e.package_id);
                    if let Some(cs) = &e.code_set {
                        println!("  code-set: {cs}");
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
        Verb::Repo(v) => match v {
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
            RepoVerb::Keygen => {
                let (private, public) = dollup_format::sign::keygen();
                println!("{private}");
                println!("{public}");
            }
        },
    }
    Ok(())
}
