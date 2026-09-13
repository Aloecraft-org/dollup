//! `dollup get` — fetch a runtime binary and drop it here.
//!
//! This is deliberately the least clever verb in the tool. It takes one
//! file, checks its hash against the sums published beside it, writes it
//! to the working directory, and says where it came from. No install
//! prefix, no PATH surgery, no `~/.config`, nothing written anywhere the
//! caller did not point at. If you want it on your PATH, move it.
//!
//! **On SPEC.md §1's "the binary ships knowing zero URLs".** That rule is
//! about *package resolution*, and it is untouched here: `add` still
//! consults only the deployment's config, and an empty source list still
//! resolves nothing. `get` is a different verb over a different artifact —
//! a runtime binary is not a package, has no manifest, and never enters
//! the store or the lockfile. It knows two default places, asked in a fixed
//! order, it **prints the URL it is about to use every single time**, and
//! `--from` replaces both. A default you can read is not a fallback you
//! cannot see.

use std::io::Read;
use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};

/// The origin: GitHub's releases for drt. Its download directory for a tag
/// (`releases/download/<tag>/`) and its `releases/latest/download/` — the
/// newest stable release, which is what the mirror's `latest/` is too — are
/// each laid out like a mirror directory: the asset, `SHA256SUMS.txt`,
/// `BUILDINFO.txt`. Asked first. Not a forge adapter: two URL shapes, one
/// reader.
pub const DRT_RELEASES: &str = "https://github.com/Aloecraft-org/diluvium-drt/releases";

/// The release mirror's directory for drt: one directory per tag and a
/// `latest/`, carrying what drt's changelog marks `mirror: true` — which
/// excludes candidates. Asked second, for as long as the mirror lags the
/// origin; a `file://` copy of it is the air-gapped source.
pub const DRT_MIRROR: &str = "https://software.aloecraft.org/releases/diluvium-drt";

/// `DOLLUP_DRT_RELEASES` and `DOLLUP_DRT_MIRROR` replace the two bases, the
/// way drt's own installer takes `DRT_MIRROR`: the host moves, the layout
/// under it does not, so a `file://` directory laid out like the mirror is
/// an air-gapped copy, and a test's stand-in. Empty means unset.
fn base_from(var: &str, default: &str) -> String {
    std::env::var(var)
        .ok()
        .map(|v| v.trim().trim_end_matches('/').to_string())
        .filter(|v| !v.is_empty())
        .unwrap_or_else(|| default.to_string())
}

pub(crate) fn mirror_base() -> String {
    base_from("DOLLUP_DRT_MIRROR", DRT_MIRROR)
}

pub(crate) fn releases_base() -> String {
    base_from("DOLLUP_DRT_RELEASES", DRT_RELEASES)
}

/// Where a release is looked for when `--from` names nothing, in the order
/// asked: the origin, then the mirror. A fallback list, the way a root's
/// package sources are one (THREAT-NOTES.md): a place that cannot be read
/// is passed over and the next is asked, said; a place that refuses — its
/// bytes are not what its own sums say — is not.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Place {
    Origin,
    Mirror,
}

impl Place {
    pub(crate) const ORDER: [Place; 2] = [Place::Origin, Place::Mirror];

    pub(crate) fn name(self) -> &'static str {
        match self {
            Place::Origin => "the origin",
            Place::Mirror => "the mirror",
        }
    }

    /// The directory for a tag, or for `latest`. The origin spells the two
    /// differently; the mirror keys both the same way.
    pub(crate) fn dir(self, version: &str) -> String {
        match (self, version) {
            (Place::Origin, "latest") => format!("{}/latest/download", releases_base()),
            (Place::Origin, tag) => format!("{}/download/{tag}", releases_base()),
            (Place::Mirror, name) => format!("{}/{name}", mirror_base()),
        }
    }
}

/// A directory to read a release from, and what to call it in a report:
/// one of the places, or the `--from` the operator named.
#[derive(Debug, Clone)]
pub(crate) struct Source {
    pub name: &'static str,
    pub base: String,
}

/// The directories a version is looked for in, in order: `--from` alone
/// when given — the operator's word, never fallen back from — else each
/// place's directory for the tag, or for `latest`.
pub(crate) fn sources_for(version: &str, from: Option<&str>) -> Vec<Source> {
    match from {
        Some(url) => vec![Source {
            name: "--from",
            base: url.trim_end_matches('/').to_string(),
        }],
        None => Place::ORDER
            .iter()
            .map(|place| Source {
                name: place.name(),
                base: place.dir(version),
            })
            .collect(),
    }
}

/// The asset names a drt release may carry for this platform, newest
/// spelling first: `doc/ALIGNMENT.md` §4 (`drt_linux_x86_64_musl`, the
/// profile last, `arm64` the token on every OS) and the spelling every
/// release up to 0.6.0rc1 used (`drt_linux_static_x86_64`, the profile
/// first). On darwin the two coincide for the full profile, so one name
/// comes back. Which spelling a release carries is read off its
/// `SHA256SUMS.txt`, never inferred from a version: the name is a handle
/// and the sums are the fact, which is the alignment rule itself.
pub(crate) fn asset_names(slim: bool) -> Result<Vec<String>> {
    let (os_new, os_old, libc, ext) = match std::env::consts::OS {
        "linux" => ("linux", "linux_static", "_musl", ""),
        "macos" => ("darwin", "darwin", "", ""),
        "windows" => ("windows", "windows", "", ".exe"),
        other => bail!("{other} has no prebuilt DRT yet; build it from source"),
    };
    let arch = match std::env::consts::ARCH {
        "x86_64" => "x86_64",
        "aarch64" => "arm64",
        other => bail!("{other} has no prebuilt DRT yet"),
    };
    let profile = if slim { "_slim" } else { "" };
    let mut names = vec![
        format!("drt_{os_new}_{arch}{libc}{profile}{ext}"),
        format!("drt{profile}_{os_old}_{arch}{ext}"),
    ];
    names.dedup();
    Ok(names)
}

/// The asset for this platform that a directory already holds, under
/// either spelling.
fn cached_asset(dir: &Path, slim: bool) -> Result<Option<PathBuf>> {
    Ok(asset_names(slim)?
        .into_iter()
        .map(|name| dir.join(name))
        .find(|path| path.is_file()))
}

/// A release as the mirror names it: the tag it is served under, and the
/// version, which is the tag without its `v`. The pin in `project.json` is
/// the *version*, because that is what drt compares its own stamped release
/// tag against (an untagged local build answers with its crate version
/// instead), and the cache is keyed by it for the same reason. `v0.4.1` and
/// `0.4.1` name one release; `0.5.0rc9` and `0.5.0` name two, because a
/// candidate is tagged as its own release while its crates stay at `0.5.0`
/// — which is why the pin is the tag and never the crate version.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Release {
    pub tag: String,
    pub version: String,
}

impl Release {
    pub fn named(spelled: &str) -> Release {
        let version = spelled.strip_prefix('v').unwrap_or(spelled).to_string();
        Release {
            tag: format!("v{version}"),
            version,
        }
    }
}

/// Which release `version` names, and where its files are looked for:
/// `--from` verbatim, else the origin's directory for the tag and then the
/// mirror's.
///
/// `latest` is resolved to a concrete tag first, through the `tag:` line
/// of the BUILDINFO.txt served beside it — the origin's newest stable
/// release, or the mirror's when the origin does not answer — because a
/// cache entry or a pin called "latest" would be a moving target: nothing
/// mutable is ever a pin. When no place can say which version `latest` is,
/// the refusal names each and asks for a version, rather than caching under
/// a name that will mean something else tomorrow.
pub fn resolve(version: &str, from: Option<&str>) -> Result<(Release, Vec<Source>)> {
    if version != "latest" {
        let release = Release::named(version);
        let sources = sources_for(&release.tag, from);
        return Ok((release, sources));
    }
    let mut refused = vec![];
    for source in sources_for("latest", from) {
        let url = format!("{}/BUILDINFO.txt", source.base);
        let info = match read_url(&url) {
            Ok(bytes) => bytes,
            Err(e) => {
                refused.push(format!(
                    "{} has no BUILDINFO.txt at {url} ({e:#})",
                    source.name
                ));
                continue;
            }
        };
        let Some(tag) = buildinfo_tag(&String::from_utf8_lossy(&info)) else {
            refused.push(format!("{url} names no tag"));
            continue;
        };
        let release = Release::named(&tag);
        // With --from, the directory given is the release. Without it, the
        // tag's own directory at each place — stable where `latest` moves
        // under it.
        let sources = match from {
            Some(_) => vec![source],
            None => sources_for(&release.tag, None),
        };
        return Ok((release, sources));
    }
    bail!(
        "nothing says which version `latest` is; name one:\n  {}",
        refused.join("\n  ")
    )
}

/// The `tag: v0.4.1` line of a BUILDINFO.txt.
fn buildinfo_tag(text: &str) -> Option<String> {
    text.lines()
        .find_map(|line| line.strip_prefix("tag:"))
        .map(|tag| tag.trim().to_string())
        .filter(|tag| !tag.is_empty())
}

/// The one refusal a later source must not paper over: the bytes at a
/// source are not what its own sums say. A place that cannot be read is
/// passed over; one whose contents disagree with themselves is reported,
/// because "try somewhere else" is the wrong answer to "someone changed
/// something".
#[derive(Debug)]
pub(crate) struct Mismatch(String);

impl std::fmt::Display for Mismatch {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for Mismatch {}

/// One fetched asset, checked against the sums beside it where they exist.
pub struct Fetched {
    pub asset: String,
    pub bytes: Vec<u8>,
    pub sums: Option<String>,
    pub buildinfo: Option<String>,
    /// What the check concluded, for printing.
    pub checked: String,
}

/// Fetch the runtime for this platform from `base`. A missing sums file
/// warns rather than refuses — a release older than the sums-publishing
/// workflow is still a release someone may want to pin. A MISMATCH always
/// refuses.
pub fn fetch(base: &str, slim: bool) -> Result<Fetched> {
    let candidates = asset_names(slim)?;
    let sums = read_url(&format!("{base}/SHA256SUMS.txt"))
        .ok()
        .map(|b| String::from_utf8_lossy(&b).into_owned());
    // The sums say which spelling this release carries. Without sums, ask
    // for each in turn; the first that answers is the one.
    let listed = sums.as_deref().and_then(|sums| {
        candidates
            .iter()
            .find(|name| want_hash(sums, name).is_some())
    });
    let (asset, bytes) = match listed {
        Some(name) => {
            println!("fetching {base}/{name}");
            let bytes = read_url(&format!("{base}/{name}"))
                .with_context(|| format!("no {name} at {base}"))?;
            (name.clone(), bytes)
        }
        None => {
            let mut found = None;
            for name in &candidates {
                println!("fetching {base}/{name}");
                if let Ok(bytes) = read_url(&format!("{base}/{name}")) {
                    found = Some((name.clone(), bytes));
                    break;
                }
            }
            found.with_context(|| {
                format!(
                    "no drt for this platform at {base}: none of {} is there",
                    candidates.join(", ")
                )
            })?
        }
    };
    let checked = match &sums {
        None => {
            eprintln!("warning: {base} has no SHA256SUMS.txt; not verified");
            "unverified (no SHA256SUMS.txt at the source)".to_string()
        }
        Some(sums) => match want_hash(sums, &asset) {
            None => {
                eprintln!("warning: SHA256SUMS.txt does not list {asset}; not verified");
                "unverified (asset not listed)".to_string()
            }
            Some(want) => {
                let have = dollup_format::identity::hash_bytes(&bytes);
                let have = have
                    .0
                    .strip_prefix("sha256:")
                    .unwrap_or(&have.0)
                    .to_string();
                if want != have {
                    return Err(Mismatch(format!(
                        "checksum mismatch for {asset}\n  expected {want}\n  got      {have}\n  from     {base}"
                    ))
                    .into());
                }
                "sha256 ok".to_string()
            }
        },
    };
    let buildinfo = read_url(&format!("{base}/BUILDINFO.txt"))
        .ok()
        .map(|b| String::from_utf8_lossy(&b).into_owned());
    Ok(Fetched {
        asset,
        bytes,
        sums,
        buildinfo,
        checked,
    })
}

pub struct GetOpts {
    pub version: String,
    pub slim: bool,
    pub from: Option<String>,
    pub out: PathBuf,
}

/// `dollup get drt`: one file, dropped where you are.
pub fn get_drt(opts: &GetOpts) -> Result<()> {
    let (release, sources) = resolve(&opts.version, opts.from.as_deref())?;
    let (fetched, _) = fetch_release(&release, &sources, opts.slim)?;
    let dest = opts.out.join("drt");
    write_executable(&dest, &fetched.bytes)
        .with_context(|| format!("writing {}", dest.display()))?;

    println!(
        "wrote {} ({}, drt {})",
        dest.display(),
        human_size(fetched.bytes.len()),
        release.version
    );
    println!("  checked: {}", fetched.checked);
    // Name the invocation that works. `get` deliberately installs nothing,
    // so the binary is not on a PATH, and "it is not on your PATH" told
    // people a true thing without telling them what to type.
    let run_as = if dest.is_absolute() {
        dest.display().to_string()
    } else {
        format!("./{}", dest.display().to_string().trim_start_matches("./"))
    };
    println!("  run it: {run_as} --version");
    Ok(())
}

/// A release in the cache: `~/.dollup/cache/drt/<version>/`, holding the
/// asset, the sums beside it, and the BUILDINFO — the mirror's own layout,
/// so `audit` can check a pinned root offline once its runtime has been
/// pulled once.
pub struct Cached {
    pub release: Release,
    pub asset: PathBuf,
    pub lines: Vec<String>,
}

/// `dollup pull drt [version]`: fill the cache and touch no root.
pub fn pull_drt(version: &str, from: Option<&str>, slim: bool) -> Result<Cached> {
    let (release, sources) = resolve(version, from)?;
    let dir = crate::home::drt_cache_dir(&release.version).ok_or_else(|| {
        anyhow::anyhow!("dollup keeps its cache in ~/.dollup/cache, and HOME is not set")
    })?;
    if let Some(asset) = cached_asset(&dir, slim)? {
        if dir.join("SHA256SUMS.txt").is_file() {
            return Ok(Cached {
                release: release.clone(),
                asset,
                lines: vec![format!(
                    "drt {} is already cached at {}",
                    release.version,
                    dir.display()
                )],
            });
        }
    }
    let (fetched, used) = fetch_release(&release, &sources, slim)?;
    std::fs::create_dir_all(&dir).with_context(|| format!("creating {}", dir.display()))?;
    let asset = dir.join(&fetched.asset);
    write_executable(&asset, &fetched.bytes)
        .with_context(|| format!("writing {}", asset.display()))?;
    if let Some(sums) = &fetched.sums {
        std::fs::write(dir.join("SHA256SUMS.txt"), sums)?;
    }
    if let Some(info) = &fetched.buildinfo {
        std::fs::write(dir.join("BUILDINFO.txt"), info)?;
    }
    let mut lines = vec![
        format!(
            "cached drt {} at {} ({}, {})",
            release.version,
            dir.display(),
            fetched.asset,
            human_size(fetched.bytes.len())
        ),
        format!("  checked: {}", fetched.checked),
    ];
    if used > 0 {
        let passed: Vec<&str> = sources[..used].iter().map(|s| s.name).collect();
        lines.push(format!(
            "  from: {} ({}); {} did not answer for {}",
            sources[used].base,
            sources[used].name,
            passed.join(" and "),
            release.tag
        ));
    }
    Ok(Cached {
        release: release.clone(),
        asset,
        lines,
    })
}

/// Fetch a release from the first source that carries it, and say when that
/// was not the first asked; which source it was is returned, so the report
/// can name where the bytes came from. Every source is checked the same
/// way — the sums beside the asset — and a mismatch at any of them ends the
/// search rather than moving it along. A single source, which is what
/// `--from` names, fails with its own error and nothing is asked after it.
fn fetch_release(release: &Release, sources: &[Source], slim: bool) -> Result<(Fetched, usize)> {
    let mut refused = vec![];
    for (i, source) in sources.iter().enumerate() {
        match fetch(&source.base, slim) {
            Ok(fetched) => return Ok((fetched, i)),
            Err(e) if e.downcast_ref::<Mismatch>().is_some() => return Err(e),
            Err(e) if sources.len() == 1 => return Err(e),
            Err(e) => {
                if let Some(next) = sources.get(i + 1) {
                    eprintln!(
                        "note: {} did not answer for {} ({e:#}); asking {}",
                        source.name, release.tag, next.name
                    );
                }
                refused.push(format!("{} ({}): {e:#}", source.name, source.base));
            }
        }
    }
    let names: Vec<&str> = sources.iter().map(|s| s.name).collect();
    bail!(
        "{} is at neither {}:\n  {}",
        release.tag,
        names.join(" nor "),
        refused.join("\n  ")
    )
}

/// The cached asset for a release, pulling it if it is not there. A named
/// version already in the cache costs no network; `latest` always asks the
/// source which version it is.
pub fn ensure_cached(version: &str, from: Option<&str>, slim: bool) -> Result<Cached> {
    if version != "latest" {
        let release = Release::named(version);
        if let Some(dir) = crate::home::drt_cache_dir(&release.version) {
            if let Some(asset) = cached_asset(&dir, slim)? {
                return Ok(Cached {
                    release,
                    asset,
                    lines: vec![],
                });
            }
        }
    }
    pull_drt(version, from, slim)
}

/// `5.5 MiB`: a size the way a person reads one. Binary units, one decimal
/// above bytes. The exact count is what `SHA256SUMS.txt` and `ls -l` are
/// for; this line is for someone deciding whether the download looks right.
fn human_size(n: usize) -> String {
    const UNITS: [&str; 5] = ["B", "KiB", "MiB", "GiB", "TiB"];
    let mut size = n as f64;
    let mut unit = 0;
    while size >= 1024.0 && unit < UNITS.len() - 1 {
        size /= 1024.0;
        unit += 1;
    }
    if unit == 0 {
        format!("{n} B")
    } else {
        format!("{size:.1} {}", UNITS[unit])
    }
}

/// The sha256 of some bytes, as a SHA256SUMS.txt line spells it.
pub(crate) fn sha256_hex(bytes: &[u8]) -> String {
    dollup_format::hash_bytes(bytes)
        .0
        .trim_start_matches("sha256:")
        .to_string()
}

/// Which cached release a binary is, by its sha256 against every cached
/// release's SHA256SUMS.txt: the version, and the sums that vouch for it.
/// `None` when nothing cached matches — never a guess, and never by running
/// it. Releases are asked in name order, so two that ship identical bytes
/// answer the same way every time.
pub(crate) fn identify(hex: &str) -> Option<(String, PathBuf)> {
    let cache = crate::home::drt_cache_root()?;
    let mut releases: Vec<PathBuf> = std::fs::read_dir(&cache)
        .ok()?
        .flatten()
        .map(|e| e.path())
        .collect();
    releases.sort();
    releases.into_iter().find_map(|dir| {
        let sums_path = dir.join("SHA256SUMS.txt");
        let sums = std::fs::read_to_string(&sums_path).ok()?;
        asset_with_hash(&sums, hex)?;
        let version = dir.file_name()?.to_string_lossy().into_owned();
        Some((version, sums_path))
    })
}

/// The asset in a sums file whose hash is `hex`, if any: the question
/// `audit` asks of a binary it will not execute. Any asset counts — a match
/// says "this is a build of that release", which is what a pin is about;
/// which platform it is for is a different question. A release that ships
/// one binary under two names (`doc/ALIGNMENT.md` §4, for one release)
/// lists the hash twice, and the name said is this platform's own, newest
/// spelling first, so the answer reads as what was fetched.
pub(crate) fn asset_with_hash(sums: &str, hex: &str) -> Option<String> {
    let named: Vec<String> = sums
        .lines()
        .filter_map(|line| {
            let (hash, name) = line.split_once("  ")?;
            (hash.trim() == hex).then(|| name.trim().to_string())
        })
        .collect();
    let ours: Vec<String> = [false, true]
        .into_iter()
        .filter_map(|slim| asset_names(slim).ok())
        .flatten()
        .collect();
    ours.into_iter()
        .find(|name| named.contains(name))
        .or_else(|| named.into_iter().next())
}

/// `<hex>  <name>` lines, the shape `sha256sum` prints.
fn want_hash(sums: &str, asset: &str) -> Option<String> {
    sums.lines().find_map(|line| {
        let (hash, name) = line.split_once("  ")?;
        (name.trim() == asset).then(|| hash.trim().to_string())
    })
}

/// `https://` through ureq, `file://` straight off the disk — the same two
/// schemes that make an air-gapped `add` work, for the same reason.
pub(crate) fn read_url(url: &str) -> Result<Vec<u8>> {
    if let Some(path) = url.strip_prefix("file://") {
        return Ok(std::fs::read(path)?);
    }
    let resp = crate::http::agent().get(url).call()?;
    let mut bytes = vec![];
    // 64 MiB: a DRT binary is ~4.5 MB and a tenfold surprise is a bug.
    resp.into_reader().take(64 << 20).read_to_end(&mut bytes)?;
    Ok(bytes)
}

#[cfg(unix)]
pub(crate) fn write_executable(path: &Path, bytes: &[u8]) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    std::fs::write(path, bytes)?;
    let mut perms = std::fs::metadata(path)?.permissions();
    perms.set_mode(0o755);
    std::fs::set_permissions(path, perms)?;
    Ok(())
}

#[cfg(not(unix))]
pub(crate) fn write_executable(path: &Path, bytes: &[u8]) -> Result<()> {
    std::fs::write(path, bytes)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sums_lines_parse_and_do_not_prefix_match() {
        let sums = "\
aaa  drt_linux_static_x86_64
bbb  drt_slim_linux_static_x86_64
ccc  BUILDINFO.txt
";
        assert_eq!(
            want_hash(sums, "drt_linux_static_x86_64").as_deref(),
            Some("aaa")
        );
        // The slim name ENDS with the full name's suffix; a sloppy match
        // would hand the full binary the slim hash or vice versa.
        assert_eq!(
            want_hash(sums, "drt_slim_linux_static_x86_64").as_deref(),
            Some("bbb")
        );
        assert_eq!(want_hash(sums, "drt_darwin_arm64"), None);
        // And the other direction, which is how audit reads the same file.
        assert_eq!(
            asset_with_hash(sums, "bbb").as_deref(),
            Some("drt_slim_linux_static_x86_64")
        );
        assert_eq!(asset_with_hash(sums, "bb"), None, "no prefix match");
        // One binary under both spellings, as a release carries for one
        // cycle: the name said is this platform's newest.
        let both = "\
aaa  drt_linux_static_x86_64
aaa  drt_linux_x86_64_musl
bbb  drt_darwin_arm64
";
        if cfg!(all(target_os = "linux", target_arch = "x86_64")) {
            assert_eq!(
                asset_with_hash(both, "aaa").as_deref(),
                Some("drt_linux_x86_64_musl")
            );
        }
        // A hash this platform has no name for is still a build of the
        // release, under whatever name the sums give it.
        assert_eq!(
            asset_with_hash(both, "bbb").as_deref(),
            Some("drt_darwin_arm64")
        );
    }

    #[test]
    fn a_tag_and_a_version_name_one_release() {
        assert_eq!(Release::named("v0.4.1"), Release::named("0.4.1"));
        let r = Release::named("v0.5.0rc9");
        assert_eq!(
            (r.tag.as_str(), r.version.as_str()),
            ("v0.5.0rc9", "0.5.0rc9")
        );
        assert_eq!(
            buildinfo_tag("commit: abc\ntag: v0.4.1\nbuilt: today\n").as_deref(),
            Some("v0.4.1")
        );
        assert_eq!(buildinfo_tag("commit: abc\n"), None);
    }

    #[test]
    fn sizes_read_like_a_person_would() {
        assert_eq!(human_size(0), "0 B");
        assert_eq!(human_size(1023), "1023 B");
        assert_eq!(human_size(1024), "1.0 KiB");
        // The number the design doc's `dollup get drt` transcript shows.
        assert_eq!(human_size(5_789_312), "5.5 MiB");
        assert_eq!(human_size(3 << 30), "3.0 GiB");
    }

    #[test]
    fn a_pinned_version_and_latest_differ() {
        assert_eq!(Place::Mirror.dir("latest"), format!("{DRT_MIRROR}/latest"));
        let pinned = Place::Mirror.dir("v0.3.0");
        assert_eq!(pinned, format!("{DRT_MIRROR}/v0.3.0"));
        assert_ne!(pinned, Place::Mirror.dir("latest"));
        // The origin's two shapes: GitHub's download directory for a tag,
        // and its `latest/download/`, which is the newest stable release.
        assert_eq!(
            Place::Origin.dir("v0.6.1-rc.2"),
            format!("{DRT_RELEASES}/download/v0.6.1-rc.2")
        );
        assert_eq!(
            Place::Origin.dir("latest"),
            format!("{DRT_RELEASES}/latest/download")
        );
        // Wherever they move to, both are over TLS: the default download of
        // a runtime binary is not a host to drift on.
        assert!(DRT_MIRROR.starts_with("https://") && DRT_MIRROR.contains("aloecraft.org/"));
        assert!(DRT_RELEASES.starts_with("https://github.com/Aloecraft-org/"));
        // The origin is asked first, and `--from` replaces both.
        let places: Vec<&str> = sources_for("v0.3.0", None).iter().map(|s| s.name).collect();
        assert_eq!(places, ["the origin", "the mirror"]);
        let from = sources_for("v0.3.0", Some("file:///mnt/xfer/"));
        assert_eq!(from.len(), 1);
        assert_eq!(from[0].base, "file:///mnt/xfer");
    }

    #[test]
    fn the_asset_names_are_both_spellings_newest_first() {
        // Only assert the shape on the platform the test runs on.
        let full = asset_names(false).unwrap();
        let slim = asset_names(true).unwrap();
        assert!(!full.is_empty() && full.len() <= 2, "{full:?}");
        assert!(full.iter().all(|n| n.starts_with("drt_")), "{full:?}");
        assert!(slim.iter().all(|n| n.contains("slim")), "{slim:?}");
        // The alignment spelling puts the profile last; the older one put
        // it first. Both are asked for, so a release under either answers.
        assert!(
            slim[0].ends_with("_slim") || slim[0].ends_with("_slim.exe"),
            "{slim:?}"
        );
        assert!(slim.last().unwrap().starts_with("drt_slim_"), "{slim:?}");
        if cfg!(target_os = "linux") {
            assert_eq!(full, ["drt_linux_x86_64_musl", "drt_linux_static_x86_64"]);
        }
        // `arm64` is the token on every OS, so a darwin full binary has one
        // name under both spellings and is asked for once.
        if cfg!(all(target_os = "macos", target_arch = "aarch64")) {
            assert_eq!(full, ["drt_darwin_arm64"]);
        }
    }
}
