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
//! the store or the lockfile. It knows a default channel, it **prints the
//! URL it is about to use every single time**, and `--from` replaces it.
//! A default you can read is not a fallback you cannot see.

use std::io::Read;
use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};

/// Where DRT releases live: the Aloecraft mirror. Every tag the DRT
/// changelog marks for mirroring sits under its own directory, `latest/`
/// tracks the changelog's `latest`, and each carries the release's own
/// SHA256SUMS.txt beside the assets — the same names and sums as
/// github.com/Aloecraft-org/diluvium-drt/releases, verified against them
/// before the mirror publishes a tag at all. A tag the changelog no longer
/// carries is gone from here; `--from` reaches GitHub directly for those.
pub const DEFAULT_DRT_CHANNEL: &str = "https://software.aloecraft.org/releases/diluvium-drt/latest";

/// Same, for a pinned version: the mirror keeps tags as directories.
pub(crate) fn channel_for(version: &str) -> String {
    if version == "latest" {
        DEFAULT_DRT_CHANNEL.to_string()
    } else {
        format!("https://software.aloecraft.org/releases/diluvium-drt/{version}")
    }
}

/// The asset naming the release workflow uses (doc/Release.md).
pub(crate) fn asset_name(slim: bool) -> Result<String> {
    let os = match std::env::consts::OS {
        "linux" => "linux_static",
        "macos" => "darwin",
        other => bail!("{other} has no prebuilt DRT yet; build it from source"),
    };
    let arch = match std::env::consts::ARCH {
        "x86_64" => "x86_64",
        "aarch64" => "arm64",
        other => bail!("{other} has no prebuilt DRT yet"),
    };
    // Linux ships x86_64 only today. Refuse by name rather than handing
    // over a binary that cannot exec.
    if os == "linux_static" && arch != "x86_64" {
        bail!("linux {arch} has no prebuilt DRT yet — only x86_64");
    }
    Ok(format!(
        "drt{}_{os}_{arch}",
        if slim { "_slim" } else { "" }
    ))
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

/// Which release `version` names, and where its files are: `--from`
/// verbatim, else the mirror's directory for the tag.
///
/// `latest` is resolved to a concrete tag first, through the `tag:` line
/// of the BUILDINFO.txt served beside it, because a cache entry or a pin
/// called "latest" would be a moving target — nothing mutable is ever a
/// pin. A source that cannot say which version `latest` is refuses by name
/// rather than caching under a name that will mean something else tomorrow.
pub fn resolve(version: &str, from: Option<&str>) -> Result<(Release, String)> {
    let base_for = |tag: &str| match from {
        Some(url) => url.trim_end_matches('/').to_string(),
        None => channel_for(tag),
    };
    if version != "latest" {
        let release = Release::named(version);
        let base = base_for(&release.tag);
        return Ok((release, base));
    }
    let base = base_for("latest");
    let info = read_url(&format!("{base}/BUILDINFO.txt")).with_context(|| {
        format!("{base} has no BUILDINFO.txt to say which version `latest` is; name one")
    })?;
    let tag = buildinfo_tag(&String::from_utf8_lossy(&info))
        .with_context(|| format!("{base}/BUILDINFO.txt names no tag; name a version"))?;
    let release = Release::named(&tag);
    // With --from, the directory given is the release. Without it, the
    // tag's own directory — stable where `latest/` moves under it.
    let base = match from {
        Some(_) => base,
        None => channel_for(&release.tag),
    };
    Ok((release, base))
}

/// The `tag: v0.4.1` line of a BUILDINFO.txt.
fn buildinfo_tag(text: &str) -> Option<String> {
    text.lines()
        .find_map(|line| line.strip_prefix("tag:"))
        .map(|tag| tag.trim().to_string())
        .filter(|tag| !tag.is_empty())
}

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
    let asset = asset_name(slim)?;
    println!("fetching {base}/{asset}");
    let bytes =
        read_url(&format!("{base}/{asset}")).with_context(|| format!("no {asset} at {base}"))?;
    let sums = read_url(&format!("{base}/SHA256SUMS.txt"))
        .ok()
        .map(|b| String::from_utf8_lossy(&b).into_owned());
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
                    bail!(
                        "checksum mismatch for {asset}\n  expected {want}\n  got      {have}\n  from     {base}"
                    );
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
    let (release, base) = resolve(&opts.version, opts.from.as_deref())?;
    let fetched = fetch(&base, opts.slim)?;
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
    let (release, base) = resolve(version, from)?;
    let dir = crate::home::drt_cache_dir(&release.version).ok_or_else(|| {
        anyhow::anyhow!("dollup keeps its cache in ~/.dollup/cache, and HOME is not set")
    })?;
    let asset_name = asset_name(slim)?;
    let asset = dir.join(&asset_name);
    if asset.is_file() && dir.join("SHA256SUMS.txt").is_file() {
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
    let fetched = fetch(&base, slim)?;
    std::fs::create_dir_all(&dir).with_context(|| format!("creating {}", dir.display()))?;
    write_executable(&asset, &fetched.bytes)
        .with_context(|| format!("writing {}", asset.display()))?;
    if let Some(sums) = &fetched.sums {
        std::fs::write(dir.join("SHA256SUMS.txt"), sums)?;
    }
    if let Some(info) = &fetched.buildinfo {
        std::fs::write(dir.join("BUILDINFO.txt"), info)?;
    }
    Ok(Cached {
        release: release.clone(),
        asset,
        lines: vec![
            format!(
                "cached drt {} at {} ({}, {})",
                release.version,
                dir.display(),
                fetched.asset,
                human_size(fetched.bytes.len())
            ),
            format!("  checked: {}", fetched.checked),
        ],
    })
}

/// The cached asset for a release, pulling it if it is not there. A named
/// version already in the cache costs no network; `latest` always asks the
/// source which version it is.
pub fn ensure_cached(version: &str, from: Option<&str>, slim: bool) -> Result<Cached> {
    if version != "latest" {
        let release = Release::named(version);
        if let Some(dir) = crate::home::drt_cache_dir(&release.version) {
            let asset = dir.join(asset_name(slim)?);
            if asset.is_file() {
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
/// which platform it is for is a different question.
pub(crate) fn asset_with_hash(sums: &str, hex: &str) -> Option<String> {
    sums.lines().find_map(|line| {
        let (hash, name) = line.split_once("  ")?;
        (hash.trim() == hex).then(|| name.trim().to_string())
    })
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
        assert_eq!(channel_for("latest"), DEFAULT_DRT_CHANNEL);
        // The mirror keeps tags as directories beside `latest/`, so a pin is
        // that same base with the tag where `latest` was. Derived from the
        // constant rather than spelled again: a mirror move that touched only
        // the constant is what left this test asserting an address nothing
        // served, and deriving it makes the two disagree loudly instead.
        let base = DEFAULT_DRT_CHANNEL
            .strip_suffix("latest")
            .expect("the channel is the `latest` directory on the mirror");
        let pinned = channel_for("v0.3.0");
        assert_eq!(pinned, format!("{base}v0.3.0"));
        assert_ne!(pinned, DEFAULT_DRT_CHANNEL);
        // Wherever it moves to, it is the Aloecraft mirror over TLS: the
        // default download of a runtime binary is not a host to drift on.
        assert!(
            base.starts_with("https://") && base.contains("aloecraft.org/"),
            "{base}"
        );
    }

    #[test]
    fn the_asset_name_is_the_release_workflows() {
        // Only assert the shape on the platform the test runs on.
        let full = asset_name(false).unwrap();
        let slim = asset_name(true).unwrap();
        assert!(full.starts_with("drt_"), "{full}");
        assert!(slim.starts_with("drt_slim_"), "{slim}");
        assert_eq!(slim, full.replacen("drt_", "drt_slim_", 1));
    }
}
