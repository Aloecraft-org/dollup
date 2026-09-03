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
fn channel_for(version: &str) -> String {
    if version == "latest" {
        DEFAULT_DRT_CHANNEL.to_string()
    } else {
        format!("https://software.aloecraft.org/releases/diluvium-drt/{version}")
    }
}

/// The asset naming the release workflow uses (doc/Release.md).
fn asset_name(slim: bool) -> Result<String> {
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

pub struct GetOpts {
    pub version: String,
    pub slim: bool,
    pub from: Option<String>,
    pub out: PathBuf,
}

pub fn get_drt(opts: &GetOpts) -> Result<()> {
    let asset = asset_name(opts.slim)?;
    let base = match &opts.from {
        Some(url) => url.trim_end_matches('/').to_string(),
        None => channel_for(&opts.version),
    };

    println!("fetching {base}/{asset}");
    let bytes =
        read_url(&format!("{base}/{asset}")).with_context(|| format!("no {asset} at {base}"))?;

    // Verify against the sums file beside it. A missing sums file warns
    // rather than refuses — a release older than the sums-publishing
    // workflow is still a release someone may want to pin. A MISMATCH
    // always refuses.
    let checked = match read_url(&format!("{base}/SHA256SUMS.txt")) {
        Err(_) => {
            eprintln!("warning: {base} has no SHA256SUMS.txt; not verified");
            "unverified (no SHA256SUMS.txt at the source)".to_string()
        }
        Ok(sums) => match want_hash(&String::from_utf8_lossy(&sums), &asset) {
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

    let dest = opts.out.join("drt");
    write_executable(&dest, &bytes).with_context(|| format!("writing {}", dest.display()))?;

    println!("wrote {} ({} bytes)", dest.display(), bytes.len());
    println!("  checked: {checked}");
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

/// `<hex>  <name>` lines, the shape `sha256sum` prints.
fn want_hash(sums: &str, asset: &str) -> Option<String> {
    sums.lines().find_map(|line| {
        let (hash, name) = line.split_once("  ")?;
        (name.trim() == asset).then(|| hash.trim().to_string())
    })
}

/// `https://` through ureq, `file://` straight off the disk — the same two
/// schemes that make an air-gapped `add` work, for the same reason.
fn read_url(url: &str) -> Result<Vec<u8>> {
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
fn write_executable(path: &Path, bytes: &[u8]) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    std::fs::write(path, bytes)?;
    let mut perms = std::fs::metadata(path)?.permissions();
    perms.set_mode(0o755);
    std::fs::set_permissions(path, perms)?;
    Ok(())
}

#[cfg(not(unix))]
fn write_executable(path: &Path, bytes: &[u8]) -> Result<()> {
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
    }

    #[test]
    fn a_pinned_version_and_latest_differ() {
        assert_eq!(channel_for("latest"), DEFAULT_DRT_CHANNEL);
        // The mirror keeps tags as directories under /drt/, which is where
        // the deployment serves DRT's mirror (not /release/drt/).
        let pinned = channel_for("v0.3.0");
        assert!(pinned.ends_with("/drt/v0.3.0"), "{pinned}");
        assert!(
            pinned.starts_with("https://diluvium.aloecraft.org/"),
            "{pinned}"
        );
        assert_ne!(pinned, DEFAULT_DRT_CHANNEL);
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
