//! `dollup audit`: what `drt start` would do in this root, reported and
//! never done.
//!
//! Three refusals shape it. It does not walk up from the directory it is
//! pointed at: `.drt_root/` is in that directory or there is no root, the
//! same rule drt's own discovery follows, so an unclaimed subdirectory of a
//! root is not in that root. It does not execute `.drt_root/drt` to learn
//! its version — audit is meant to be safe on a root you just cloned and do
//! not trust — so the pin is checked by hashing the binary against the
//! pinned release's own SHA256SUMS.txt: from the cache when `dollup pull drt`
//! has filled it, from the mirror otherwise, and left unverified by name
//! when neither answers. And it writes nothing, the cache included.
//!
//! What it reports is `drt_config::resolve::resolve` — the function `drt
//! start` and `stdlib:preflight` call — over the same files, plus the one
//! question resolution cannot answer without something executing. The
//! resolution logic and the runtime's are literally the same code, which is
//! what makes "what start would do" a fact rather than an estimate.

use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

use anyhow::{Context, Result};
use drt_config::consent::{ConsentCheck, ConsentJson};
use drt_config::project::{self, ProfileName, ProjectJson, PROFILE_SUFFIX, ROOT_DIR};
use drt_config::resolve::{
    self, ArgValue, Entry, Finding, Requested, Resolution, ResolveInputs, RootInputs,
};
use drt_config::RootConfig;

use crate::home;
use crate::runtime;

/// Every line audit prints, in order, and the verdict behind them.
#[derive(Debug, Default)]
pub struct Report {
    pub lines: Vec<String>,
    /// What would stop `drt start`: resolution's blocking findings, a
    /// profile that does not parse, and the binary check, which is audit's
    /// own.
    pub blockers: Vec<String>,
    /// Why an unattended start would stop to ask, when it would. A root
    /// under systemd has no TTY, so there this is a failure, by design.
    pub would_prompt: Option<String>,
}

impl Report {
    pub fn start_would_run(&self) -> bool {
        self.blockers.is_empty() && self.would_prompt.is_none()
    }

    /// The last line, in the words preflight uses.
    pub fn verdict(&self) -> String {
        match (self.blockers.len(), &self.would_prompt) {
            (n, _) if n > 0 => {
                format!("start would not run: {n} blocker(s) above. nothing started.")
            }
            (_, Some(why)) => format!("start would stop to ask: {why}. nothing started."),
            (_, None) => "start would run. nothing started.".to_string(),
        }
    }

    fn block(&mut self, line: String) {
        self.lines.push(format!("blocks: {line}"));
        self.blockers.push(line);
    }
}

/// Audit the root at `dir` — the directory holding `.drt_root/`, never a
/// parent of it — as `drt start [profile]` would see it.
pub fn audit(dir: &Path, profile: Option<&str>) -> Result<Report> {
    let requested = match profile {
        None => Requested::Default,
        Some(p) => Requested::Profile(profile_name(p)?),
    };
    let cwd = names_in(dir).with_context(|| format!("reading {}", dir.display()))?;
    let root_dir = dir.join(ROOT_DIR);
    let mut report = Report::default();

    if !root_dir.is_dir() {
        report.lines.push(format!(
            "no root: {} does not exist (discovery does not walk up; --root names one)",
            root_dir.display()
        ));
        let inputs = ResolveInputs {
            root: None,
            requested,
            cwd,
            ..ResolveInputs::default()
        };
        render(&mut report, &resolve::resolve(&inputs), None);
        return Ok(report);
    }

    let (mut root, unparsed) = read_root(&root_dir)?;
    // First pass: which profile runs, and therefore which directory its
    // entry is looked for in, so the second pass checks the entry against a
    // real listing. Resolution is pure; running it twice costs nothing.
    let mut inputs = ResolveInputs {
        root: Some(root.clone()),
        requested,
        cwd,
        ..ResolveInputs::default()
    };
    let first = resolve::resolve(&inputs);
    // Two listings, because which one an entry must exist in is the
    // profile's choice: a debug profile points at `dlua/`, and a released
    // root's profile sets no `dlua_dir` and deploys from `init/`. Resolution
    // checks the right one; audit supplies both.
    root.init = files_under(&root_dir.join(project::INIT_DIR))?;
    root.dlua_dir = match &first.dlua_dir {
        Some(d) => files_under(&dir.join(&d.value))?,
        None => vec![],
    };
    let binary = check_binary(
        &root_dir,
        root.project.as_ref().and_then(|p| p.drt.as_deref()),
    );
    root.binary_version = binary.version.clone();
    inputs.root = Some(root);
    let res = resolve::resolve(&inputs);

    for line in unparsed {
        report.block(line);
    }
    render(&mut report, &res, Some(&binary));
    // The one audit line that looks beyond this root: a root_id another
    // root on this box also holds is a `cp -r`, and `dollup duplicate` —
    // which mints a fresh one — is what should have been used. Read from
    // the list, checked against disk, and audit adds itself to no list.
    if let Some(project) = inputs.root.as_ref().and_then(|r| r.project.as_ref()) {
        for other in crate::roots::others_claiming(project.root_id, dir) {
            report.lines.push(format!(
                "note: another root on this box claims root_id {}: {} (a cp -r, not \
                 `dollup duplicate`, which mints a fresh one)",
                project.root_id,
                other.display()
            ));
        }
    }
    Ok(report)
}

/// `debug` or `debug.config.json`: both spellings exist in one file, so
/// both are taken here and the name is what resolution sees.
fn profile_name(arg: &str) -> Result<String> {
    if arg.ends_with(PROFILE_SUFFIX) {
        return Ok(ProfileName::from_filename(arg)?.as_str().to_string());
    }
    Ok(arg.to_string())
}

/// The root's files, read and nothing else. A `project.json` or
/// `consent.json` that does not parse ends the audit by name — there is
/// nothing to say about a root whose descriptor cannot be read — while a
/// profile that does not parse is reported and the rest still runs.
fn read_root(root_dir: &Path) -> Result<(RootInputs, Vec<String>)> {
    let mut inputs = RootInputs::default();
    let mut unparsed = vec![];

    let path = root_dir.join("project.json");
    if path.is_file() {
        let bytes = fs::read(&path)?;
        inputs.project = Some(
            serde_json::from_slice::<ProjectJson>(&bytes)
                .with_context(|| format!("{} does not parse", path.display()))?,
        );
    }
    let path = root_dir.join("consent.json");
    if path.is_file() {
        let bytes = fs::read(&path)?;
        inputs.consent = Some(
            serde_json::from_slice::<ConsentJson>(&bytes)
                .with_context(|| format!("{} does not parse", path.display()))?,
        );
    }
    let profile_dir = root_dir.join(project::PROFILE_DIR);
    if profile_dir.is_dir() {
        for name in names_in(&profile_dir)? {
            let path = profile_dir.join(&name);
            if !path.is_file() {
                continue;
            }
            inputs.profile_dir.push(name.clone());
            match serde_json::from_slice::<RootConfig>(&fs::read(&path)?) {
                Ok(config) => {
                    inputs.profiles.insert(name, config);
                }
                Err(e) => unparsed.push(format!(
                    "{}/{name} does not parse: {e}",
                    project::PROFILE_DIR
                )),
            }
        }
    }
    Ok((inputs, unparsed))
}

// depth: the one check resolution cannot make — the binary, without running it

struct BinaryCheck {
    /// The pinned version, when the binary hashed to a build of it: what
    /// resolution compares to the pin. Never a guess.
    version: Option<String>,
    line: String,
    blocks: bool,
}

fn check_binary(root_dir: &Path, pinned: Option<&str>) -> BinaryCheck {
    let path = root_dir.join("drt");
    let Ok(bytes) = fs::read(&path) else {
        return BinaryCheck {
            version: None,
            line: format!(
                "drt: no binary at {}; `dollup deploy drt` writes one",
                path.display()
            ),
            blocks: false,
        };
    };
    let Some(pinned) = pinned else {
        return BinaryCheck {
            version: None,
            line: "drt: a binary is present and nothing is pinned to compare it to".into(),
            blocks: false,
        };
    };
    let hex = dollup_format::hash_bytes(&bytes)
        .0
        .trim_start_matches("sha256:")
        .to_string();
    let (sums, from) = match sums_for(pinned) {
        Ok(found) => found,
        Err(e) => {
            return BinaryCheck {
                version: None,
                line: format!(
                    "drt: {pinned} pinned; the binary at .drt_root/drt is not verified — {e:#}"
                ),
                blocks: false,
            }
        }
    };
    match runtime::asset_with_hash(&sums, &hex) {
        Some(asset) => BinaryCheck {
            version: Some(pinned.to_string()),
            line: format!(
                "drt: {pinned} pinned, {pinned} present (.drt_root/drt is {asset} by sha256, \
                 per {from}; audit never executes it)"
            ),
            blocks: false,
        },
        None => BinaryCheck {
            version: None,
            line: format!(
                "drt: {pinned} pinned, but .drt_root/drt is not a {pinned} build: its sha256 \
                 {}… matches nothing in that release's SHA256SUMS.txt ({from})",
                &hex[..12]
            ),
            blocks: true,
        },
    }
}

/// The pinned release's sums: from the cache when `dollup pull drt` has
/// filled it, else from the mirror. Never written here — audit reports.
fn sums_for(pinned: &str) -> Result<(String, String)> {
    if let Some(cached) = home::drt_sums_path(pinned) {
        if let Ok(text) = fs::read_to_string(&cached) {
            return Ok((text, format!("the cached sums at {}", cached.display())));
        }
    }
    // The pin is what `drt buildinfo` reports; the mirror is keyed by tag,
    // and the two differ by the `v`. A wrong guess fails by name below and
    // the binary is reported unverified — it is never matched by mistake.
    let tag = if pinned.starts_with('v') {
        pinned.to_string()
    } else {
        format!("v{pinned}")
    };
    let url = format!("{}/SHA256SUMS.txt", runtime::channel_for(&tag));
    let bytes = runtime::read_url(&url).with_context(|| {
        format!(
            "no cached SHA256SUMS.txt for {pinned} (`dollup pull drt {pinned}` caches one) \
             and {url} did not answer"
        )
    })?;
    Ok((String::from_utf8_lossy(&bytes).into_owned(), url))
}

// depth: rendering, one line per audit question

fn render(report: &mut Report, res: &Resolution, binary: Option<&BinaryCheck>) {
    if let Some(profile) = &res.profile {
        report.lines.push(format!("profile: {profile}"));
    }
    match binary {
        Some(b) if b.blocks => report.block(b.line.clone()),
        Some(b) => report.lines.push(b.line.clone()),
        None => {}
    }
    if let Some(ceiling) = &res.ceiling {
        report.lines.push(format!(
            "ceiling: {} caps ({})",
            ceiling.value.len(),
            ceiling.rule
        ));
    }
    if let Some(check) = &res.consent {
        consent_lines(report, check);
    }
    if let Some(entry) = &res.entry {
        let missing = res
            .findings
            .iter()
            .any(|f| matches!(f, Finding::EntryMissing { .. }));
        let presence = match (&entry.value, missing) {
            (Entry::Stdlib(_), _) => ", resolved by the binary",
            (Entry::File(_), false) => ", present",
            (Entry::File(_), true) => "",
        };
        report.lines.push(format!("entry: {entry}{presence}"));
        report.lines.push(match &res.dlua_dir {
            Some(dir) => format!("source: {dir}"),
            None => format!(
                "source: {}/{}/ (delivered content; the profile sets no dlua_dir)",
                ROOT_DIR,
                project::INIT_DIR
            ),
        });
    }
    if !res.args.is_empty() {
        report.lines.push(format!("args: {}", args_line(&res.args)));
    }
    for finding in &res.findings {
        if finding.blocks() {
            report.block(finding.to_string());
        } else {
            report.lines.push(format!("note: {finding}"));
        }
    }
}

fn consent_lines(report: &mut Report, check: &ConsentCheck) {
    match check {
        ConsentCheck::Blanket {
            accepted_at,
            ceiling_changed,
        } => report.lines.push(format!(
            "consent: blanket operator consent, accepted {accepted_at}{}",
            if *ceiling_changed {
                " against a ceiling that has since changed"
            } else {
                ""
            }
        )),
        ConsentCheck::First { .. } => {
            report.lines.push(
                "consent: none accepted yet; start would print the ceiling and ask \
                 (-y accepts a first acceptance)"
                    .into(),
            );
            report.would_prompt = Some("no consent has been accepted for this ceiling".into());
        }
        ConsentCheck::Unchanged => report
            .lines
            .push("consent: listed, matches the ceiling".into()),
        ConsentCheck::Narrowed { change, .. } => {
            report.lines.push(
                "consent: listed; the ceiling narrowed since it was accepted — start updates \
                 the entry silently"
                    .into(),
            );
            report
                .lines
                .extend(change.lines().into_iter().map(|l| format!("  {l}")));
        }
        ConsentCheck::Widened {
            objection, change, ..
        } => {
            report.lines.push(format!(
                "consent: listed; the ceiling widened since it was accepted — start stops to \
                 ask, and --accept-changes is what accepts it (-y does not): {}",
                objection.0
            ));
            report
                .lines
                .extend(change.lines().into_iter().map(|l| format!("  {l}")));
            report.would_prompt = Some("the ceiling widened since consent was accepted".into());
        }
    }
}

/// `{ verbose: false, stun: [] }` — the merged table the entry would see.
fn args_line(args: &BTreeMap<String, ArgValue>) -> String {
    let inner: Vec<String> = args
        .iter()
        .map(|(k, v)| {
            format!(
                "{k}: {}",
                serde_json::to_string(v).unwrap_or_else(|_| "?".into())
            )
        })
        .collect();
    format!("{{ {} }}", inner.join(", "))
}

// depth: directory listings, the only IO here

/// Names in one directory, sorted.
fn names_in(dir: &Path) -> Result<Vec<String>> {
    let mut names = fs::read_dir(dir)?
        .map(|entry| Ok(entry?.file_name().to_string_lossy().into_owned()))
        .collect::<Result<Vec<String>>>()?;
    names.sort();
    Ok(names)
}

/// Every file under a directory, as `/`-separated paths relative to it;
/// empty when the directory is not there, which is a fact resolution
/// reports rather than an error this raises.
fn files_under(dir: &Path) -> Result<Vec<String>> {
    let mut out = vec![];
    if !dir.is_dir() {
        return Ok(out);
    }
    let mut stack = vec![dir.to_path_buf()];
    while let Some(d) = stack.pop() {
        for entry in fs::read_dir(&d)? {
            let path = entry?.path();
            if path.is_dir() {
                stack.push(path);
                continue;
            }
            out.push(path.strip_prefix(dir)?.to_string_lossy().replace('\\', "/"));
        }
    }
    out.sort();
    Ok(out)
}
