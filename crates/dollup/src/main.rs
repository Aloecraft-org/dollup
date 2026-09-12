//! `dollup` — fetcher and resolver over a content-addressed store.
//! Install is inert; config is authority; materializing files is the last
//! act. SPEC.md is the map.

mod audit;
mod consent;
pub mod deployment;
mod fetch;
mod home;
mod http;
mod ops;
mod repo;
mod root;
mod roots;
mod runtime;
mod snap;
mod store;

use std::path::PathBuf;

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use dollup_format::source::Ref;

use deployment::Deployment;

#[derive(Parser)]
#[command(name = "dollup", version, about)]
struct Cli {
    /// The app directory (default: the current directory). Nothing is ever
    /// implicitly global. `--root` is the same directory seen as a drt
    /// root: the one holding `.drt_root/`, never a parent of it.
    #[arg(long, global = true, visible_aliases = ["deployment", "root"])]
    app: Option<PathBuf>,
    /// The config file to use. Defaults to `DOLLUP_CONFIG` if set, then
    /// `<app>/dollup.json`. Those three, and nothing else: no home
    /// directory, no XDG lookup, and nothing written on first run.
    #[arg(short = 'c', long, global = true, value_name = "FILE")]
    config: Option<PathBuf>,
    #[command(subcommand)]
    verb: Verb,
}

#[derive(Subcommand)]
enum Verb {
    /// Start a root here: `.drt_root/` with its descriptor, consent to the
    /// ceiling it declares, a default profile, a preflight profile, and
    /// `dlua/app.dlua`. Creates what is missing and never rewrites what
    /// exists, so it is also how a root gains a profile it lacked.
    Init {
        /// The project name. Absent is legal; audit says so.
        name: Option<String>,
        /// The default profile's name (default: `debug`). `release` and
        /// `release.config.json` are the same request.
        profile: Option<String>,
    },
    /// Make a directory and start a root in it: `dollup new my_app` is
    /// `mkdir my_app && cd my_app && dollup init my_app`. Never takes a
    /// ref — a starting point is a package you pull.
    New {
        /// The directory, which names the project.
        name: String,
    },
    /// Fetch a package (and its dependencies) through the cache into
    /// init/, and lock it. A starting point — a template — is copied
    /// instead and never locked: those files are yours to edit. Inert
    /// either way; nothing runs.
    Pull {
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
    /// Not a verb any more — `add` became `pull`. Caught so the old
    /// spelling answers with the new one, arguments carried over.
    #[command(hide = true)]
    Add {
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        rest: Vec<String>,
    },
    /// What the lock holds.
    Ls,
    /// Describe a package as the sources see it, without adding it.
    Info { r#ref: String },
    /// Re-hash the code root and the store against the lock.
    Verify,
    /// Sweep the store against the lock.
    Gc,
    /// Review and accept this root's declared ceiling, without starting
    /// anything: the same check `drt start` runs, with the same answers.
    /// No terminal and no applicable flag is a refusal, never a hang.
    Consent {
        /// Accept a FIRST acceptance without asking. Nothing else: a
        /// ceiling that widened since it was accepted takes
        /// --accept-changes, so no unit file carries consent to all
        /// future widening.
        #[arg(short = 'y', long)]
        yes: bool,
        /// Accept a ceiling that widened since it was accepted, after the
        /// delta is printed.
        #[arg(long)]
        accept_changes: bool,
        /// Write the blanket operator entry: everything, forever, nothing
        /// prompts again. The explicit opt-out, said out loud.
        #[arg(long, conflicts_with_all = ["yes", "accept_changes"])]
        all: bool,
    },
    /// Every root on this box: roots on disk, not deployments running.
    /// Recorded whenever a verb opens one, checked against disk when shown;
    /// a root removed with rm -rf is reported stale, never dropped.
    Roots,
    /// Check a root for likely issues: what `drt start` would do here,
    /// reported and never done. Safe on a root you do not trust — nothing
    /// executes, nothing is delegated, nothing is written.
    Audit {
        /// A profile to audit as `drt start <profile>` would, by name
        /// (`debug`) or filename (`debug.config.json`). Default: the root's
        /// default_profile.
        profile: Option<String>,
    },
    /// Not a verb — `dollup drt get` reads naturally enough that it is
    /// worth catching rather than answering "unrecognized subcommand".
    #[command(hide = true)]
    Drt {
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        rest: Vec<String>,
    },
    /// Fetch a runtime binary into the working directory. One file,
    /// hash-checked, dropped where you are. It does not install anything.
    Get {
        /// What to fetch. `drt` is the only one today.
        what: String,
        /// A release tag, or `latest`.
        #[arg(long, default_value = "latest")]
        version: String,
        /// The size profile rather than the full runtime.
        #[arg(long)]
        slim: bool,
        /// Where to fetch from, replacing the default channel. Takes
        /// `file://` too, which is the air-gapped case.
        #[arg(long, value_name = "URL")]
        from: Option<String>,
        /// Where to write it (default: the working directory).
        #[arg(long, value_name = "DIR")]
        out: Option<PathBuf>,
    },
    /// Not a verb yet — `push` is reserved for shipping a whole root, which
    /// is not built. Caught because it used to be the snapshot transport:
    /// the old spelling answers with the new one rather than "unrecognized
    /// subcommand". (`pull` is a verb again, for packages; a bare URL
    /// handed to it gets the same courtesy inside.)
    #[command(hide = true)]
    Push {
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        rest: Vec<String>,
    },
    /// Snapshot transport: move a hibernated instance between machines.
    /// Restore stays DRT's verb.
    #[command(subcommand)]
    Snapshot(SnapshotVerb),
    /// Publisher-side verbs: seal, index, sign, blobs, publish, keygen.
    #[command(subcommand)]
    Repo(RepoVerb),
    /// Where this app installs from.
    #[command(subcommand)]
    Source(SourceVerb),
}

#[derive(Subcommand)]
enum SnapshotVerb {
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
    /// Check that index.json.sig verifies over index.json under a public
    /// key. What CI asks of a committed repo.
    Verify {
        dir: PathBuf,
        /// The public key, `ed25519:<base64>`.
        #[arg(long, conflicts_with = "key_file")]
        key: Option<String>,
        /// Read the public key from a file (a `.pub`, or site/std-repo.pub).
        #[arg(long)]
        key_file: Option<PathBuf>,
    },
    /// Print the public key belonging to a private one. Derived, so it
    /// cannot drift from what actually signs a repo — which a `.pub` file
    /// sitting beside the key can.
    Pubkey {
        #[arg(long, value_name = "PATH")]
        key_file: PathBuf,
    },
    /// Seal every package, index, sign, project blobs — then prove the
    /// result actually resolves before anything is copied anywhere.
    Publish {
        dir: PathBuf,
        /// Sign the index with this key. The matching public key is derived
        /// from it, never looked for beside it, and is what the self-check
        /// pins — so what signed the repo and what verifies it cannot drift.
        #[arg(long, value_name = "PATH")]
        key_file: Option<PathBuf>,
        /// Copy the four things that constitute a repo into this directory
        /// and check that instead. What to rsync, without the README, the
        /// scripts or the .git directory riding along.
        #[arg(long, value_name = "DIR")]
        stage: Option<PathBuf>,
        /// Skip the blobs/ projection. Only a static HTTP mirror serves
        /// blobs; a file://, git+ or zip+ repo never reads them.
        #[arg(long)]
        no_blobs: bool,
    },
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

/// How this process was invoked, for printing back in hints.
///
/// `dollup get drt` drops a binary in the working directory rather than on
/// a PATH, which is the right default — but it means the tool is usually
/// reached as `./dollup`, and every hint that says "run `dollup add`"
/// answers `command not found`. Echoing argv[0] is always right: run it as
/// `dollup`, `./dollup` or `../target/release/dollup` and the hints match.
pub(crate) fn me() -> String {
    match std::env::args().next() {
        Some(a) if !a.is_empty() => a,
        _ => "dollup".to_string(),
    }
}

/// `dollup push` and `dollup pull` moved snapshots until the verbs were
/// reserved for shipping a whole root. That is not built, so the only thing
/// the old spelling can usefully do is name the new one, with the rest of
/// the line carried over so it can be run as printed.
fn snapshot_moved(verb: &str, rest: &[String]) -> Result<()> {
    let me = me();
    anyhow::bail!(
        "`{me} {verb}` is reserved for shipping a root, which is not built yet. \
         Snapshots moved under `snapshot`:\n  \
         {me} snapshot {verb}{}{}",
        if rest.is_empty() { "" } else { " " },
        rest.join(" ")
    )
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    let dir = cli.app.unwrap_or_else(|| PathBuf::from("."));
    // `--config` beats `DOLLUP_CONFIG` beats `<app>/dollup.json`. Resolved
    // once, here, so nothing below reads the environment.
    let cfg_env = deployment::from_env();
    let cfg = cli.config.as_deref().or(cfg_env.as_deref());
    match cli.verb {
        Verb::Init { name, profile } => {
            // `-c` names a dollup.json app, which init no longer writes;
            // say so rather than make a root while ignoring the flag.
            if cfg.is_some() {
                anyhow::bail!(
                    "init makes a root at {}; -c and DOLLUP_CONFIG name a dollup.json app, \
                     which init no longer writes",
                    dir.display()
                );
            }
            println!("Root at {}", dir.display());
            for line in root::init(&dir, name.as_deref(), profile.as_deref())? {
                println!("  {line}");
            }
            // Someone who just typed `dollup init` wants to run something,
            // not to learn what a profile is: the commands that work next.
            println!();
            println!("  drt start            deploy dlua/ to live/ and run it");
            println!("  dollup audit         what start would do, without doing it");
            println!("  dollup pull hello    install a program from the standard source");
        }
        Verb::New { name } => {
            if name.contains('@') || name.contains('#') {
                anyhow::bail!(
                    "`new` takes a directory name, never a ref; a starting point is a package \
                     you pull:\n  {} pull {name}",
                    me()
                );
            }
            let path = dir.join(&name);
            // The project is named for the directory, whatever path led
            // to it; the reserved names are refused inside init.
            let project = std::path::Path::new(&name)
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_else(|| name.clone());
            println!("Root at {}", path.display());
            for line in root::init(&path, Some(project.as_str()), None)? {
                println!("  {line}");
            }
            println!();
            println!("  cd {name}");
            println!("  drt start            deploy dlua/ to live/ and run it");
        }
        Verb::Pull {
            r#ref,
            with_host,
            with_host_native,
        } => {
            // A bare URL is a remote, not a package — and it is what the
            // snapshot transport used to take here. Say which spelling
            // does what rather than "in none of the sources".
            if dollup_format::source::Scheme::of(&r#ref).is_ok() {
                let me = me();
                anyhow::bail!(
                    "'{ref}' is a remote, not a package: `{me} pull <url>#<name>` pulls a \
                     package from it, and `{me} snapshot pull <url> <name>` pulls a snapshot",
                    ref = r#ref
                );
            }
            let mut d = Deployment::open(&dir, cfg)?;
            let r: Ref = r#ref.parse()?;
            let gates = ops::HostGates {
                with_host: with_host || with_host_native,
                with_host_native,
            };
            let lines = ops::pull(&mut d, &r, gates)?;
            let copied = lines.first().is_some_and(|l| l.starts_with("From "));
            for line in lines {
                println!("{line}");
            }
            if copied {
                println!();
                println!("These files are yours now — edit them.");
            }
        }
        Verb::Add { rest } => {
            let me = me();
            anyhow::bail!(
                "`{me} add` is `{me} pull` now — a package is locked, a starting point is \
                 copied, and the verb is the same:\n  \
                 {me} pull{}{}",
                if rest.is_empty() { "" } else { " " },
                rest.join(" ")
            );
        }
        Verb::Ls => {
            let d = Deployment::open(&dir, cfg)?;
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
            let d = Deployment::open(&dir, cfg)?;
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
                        if e.template {
                            "a starter template: dollup new copies it, then it is yours"
                        } else {
                            match &e.code_set {
                                Some(_) if e.runnable => "a program: dollup add it, then run it",
                                Some(_) => "a library: other packages require it",
                                None => "no guest code",
                            }
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
                                println!(
                                    "  requires connectors: {}",
                                    m.requires.connectors.names().join(", ")
                                );
                            }
                        }
                    }
                    return Ok(());
                }
            }
            anyhow::bail!("'{}' is in none of the sources", r.name);
        }
        Verb::Verify => {
            let d = Deployment::open(&dir, cfg)?;
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
            let d = Deployment::open(&dir, cfg)?;
            let (swept, notes) = ops::gc(&d)?;
            for note in notes {
                eprintln!("note: {note}");
            }
            println!("swept {swept} blob(s)");
        }
        Verb::Consent {
            yes,
            accept_changes,
            all,
        } => {
            let lines = consent::consent(
                &dir,
                consent::Flags {
                    yes,
                    accept_changes,
                    all,
                },
            )?;
            for line in lines {
                println!("{line}");
            }
        }
        Verb::Roots => {
            for line in roots::report()? {
                println!("{line}");
            }
        }
        // Deliberately does NOT open a deployment: a root is `.drt_root/`
        // and its files, and audit reads those and nothing else — the list
        // of roots included, which it reads and never adds itself to.
        Verb::Audit { profile } => {
            let report = audit::audit(&dir, profile.as_deref())?;
            for line in &report.lines {
                println!("{line}");
            }
            if !report.start_would_run() {
                anyhow::bail!("{}", report.verdict());
            }
            println!("{}", report.verdict());
        }
        Verb::Drt { rest } => {
            let rest = rest.join(" ");
            anyhow::bail!(
                "there is no `drt` subcommand — the thing comes after the verb:\n  \
                 dollup get drt{}{}",
                if rest.is_empty() { "" } else { " " },
                rest.trim_start_matches("get").trim()
            );
        }
        // Deliberately does NOT open a deployment: fetching a runtime
        // binary is not a deployment act, needs no config, and has to work
        // in an empty directory.
        Verb::Get {
            what,
            version,
            slim,
            from,
            out,
        } => {
            if what != "drt" {
                anyhow::bail!("`dollup get` knows only `drt` today; got '{what}'");
            }
            runtime::get_drt(&runtime::GetOpts {
                version,
                slim,
                from,
                out: out.unwrap_or_else(|| PathBuf::from(".")),
            })?;
        }
        Verb::Push { rest } => snapshot_moved("push", &rest)?,
        Verb::Snapshot(v) => match v {
            SnapshotVerb::Push {
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
                let mut d = Deployment::open(&dir, cfg)?;
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
            SnapshotVerb::Pull { remote, name } => {
                let mut d = Deployment::open(&dir, cfg)?;
                for line in snap::pull(&mut d, &remote, &name)? {
                    println!("{line}");
                }
            }
        },
        Verb::Source(v) => {
            let mut d = Deployment::open(&dir, cfg)?;
            match v {
                SourceVerb::Add { url, key, key_file } => {
                    dollup_format::source::Scheme::of(&url)?;
                    // `file://$PWD/../std-repo` is how a shell hands over a
                    // sibling directory, and storing it with the `..` still
                    // in it puts a path in the config whose meaning depends
                    // on where it was typed. Resolve it when it exists;
                    // leave it verbatim when it does not, because the
                    // air-gapped case adds the source before the mount.
                    let url = match url.strip_prefix("file://") {
                        Some(path) => match std::fs::canonicalize(path) {
                            Ok(real) => format!("file://{}", real.display()),
                            Err(_) => url,
                        },
                        None => url,
                    };
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
                    // Warn about the refusal that will actually happen, and
                    // only that one. `require_signatures` exempts file
                    // transports (`Scheme::network()` is HTTPS alone), so
                    // the old unconditional note fired on every `file://`
                    // source and told the reader their source would be
                    // "refused at resolve time" when it would not be — and
                    // `init` writes require_signatures: true, so it was the
                    // first thing anyone saw and it was false.
                    let network = dollup_format::source::Scheme::of(&url)?.network();
                    if !signed && network && d.config.require_signatures {
                        eprintln!(
                            "warning: {url} is unsigned and this deployment sets \
                             require_signatures — resolving from it WILL be refused. \
                             Pin the publisher's key with --key, or clear \
                             require_signatures in the config."
                        );
                    } else if !signed && network {
                        eprintln!(
                            "note: {url} is unsigned, and require_signatures is off, \
                             so nothing checks who published what it serves"
                        );
                    }
                    // A `file://` source that is not there yet is legal —
                    // adding the source before mounting the media is the
                    // air-gapped order of operations. Say it now anyway,
                    // because the other reason a path is not there is a
                    // typo, and that one costs a confusing `add` later.
                    if let Some(path) = url.strip_prefix("file://") {
                        if !std::path::Path::new(path).exists() {
                            eprintln!(
                                "note: {path} is not there yet — fine if you mount it later, \
                                 a typo otherwise"
                            );
                        }
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
            RepoVerb::Verify { dir, key, key_file } => {
                let key = match (key, key_file) {
                    (Some(k), _) => k,
                    (None, Some(path)) => std::fs::read_to_string(&path)
                        .with_context(|| format!("reading {}", path.display()))?,
                    (None, None) => anyhow::bail!("verify needs --key or --key-file"),
                };
                let by = repo::verify_index(&dir, &key)?;
                println!("verified: {} is signed by {by}", dir.display());
            }
            RepoVerb::Pubkey { key_file } => {
                let key = std::fs::read_to_string(&key_file)
                    .with_context(|| format!("reading {}", key_file.display()))?;
                println!("{}", dollup_format::sign::public_key_of(key.trim())?);
            }
            RepoVerb::Publish {
                dir,
                key_file,
                stage,
                no_blobs,
            } => {
                let out = repo::publish(&dir, key_file.as_deref(), stage.as_deref(), !no_blobs)?;
                for line in &out.sealed {
                    println!("{line}");
                }
                println!("indexed {} package(s)", out.packages);
                match &out.signed_by {
                    Some(key) => println!("signed, pin this key: {key}"),
                    None => println!("unsigned — a network source needs `--key-file`"),
                }
                if out.blobs > 0 {
                    println!("projected {} blob(s)", out.blobs);
                }
                println!("resolved the published tree:");
                for line in &out.resolved {
                    println!("  {line}");
                }
                println!(
                    "publish {}/ — the tree is ready to copy",
                    out.tree.display()
                );
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
