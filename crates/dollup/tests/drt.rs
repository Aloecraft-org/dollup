//! The runtime's lifecycle in a root: `pull drt` fills the cache and
//! touches no root, `deploy drt` copies from the cache into `.drt_root/drt`
//! (never a link), `pin drt` deploys and records, and `audit` then says
//! the pin and the binary agree — by hash, never by running it. Against a
//! fake mirror and a fake origin on disk (`--from file://`, or the two
//! bases moved by environment), so nothing here reaches the network, and
//! every platform's asset name is present so the test does not care which
//! one this box wants.
//!
//! And `get drt`, which is the same fetch over the same directories with no
//! root and no cache in it at all — what `--from` means, what the report is
//! allowed to claim about the bytes, and what the failure path says about
//! the file it did not write.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use sha2::Digest;

mod common;

fn run(cmd: &mut Command) -> String {
    let out = cmd.output().unwrap();
    assert!(
        out.status.success(),
        "dollup failed:\n{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    )
}

fn fail(cmd: &mut Command) -> String {
    let out = cmd.output().unwrap();
    assert!(!out.status.success(), "expected failure, got success");
    format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    )
}

const ASSETS: [&str; 6] = [
    "drt_linux_static_x86_64",
    "drt_darwin_arm64",
    "drt_darwin_x86_64",
    "drt_slim_linux_static_x86_64",
    "drt_slim_darwin_arm64",
    "drt_slim_darwin_x86_64",
];

/// The same, under the artifact names doc/ALIGNMENT.md §4 aligns every
/// project on: the profile last, `musl` where `static` was, `arm64` the
/// token everywhere.
const ALIGNED_ASSETS: [&str; 6] = [
    "drt_linux_x86_64_musl",
    "drt_darwin_arm64",
    "drt_darwin_x86_64",
    "drt_linux_x86_64_musl_slim",
    "drt_darwin_arm64_slim",
    "drt_darwin_x86_64_slim",
];

/// A release directory as the mirror lays one out, with a `latest/` that
/// says which version it is.
fn write_mirror(mirror: &Path, tag: &str, body: &[u8]) -> PathBuf {
    write_mirror_with(mirror, tag, body, &ASSETS)
}

fn write_mirror_with(mirror: &Path, tag: &str, body: &[u8], assets: &[&str]) -> PathBuf {
    for dir in [mirror.join(tag), mirror.join("latest")] {
        write_release(&dir, tag, body, assets);
    }
    mirror.to_path_buf()
}

/// A release directory as GitHub's releases lay one out: `download/<tag>/`
/// and, for the newest stable release, `latest/download/` -- the origin's
/// two shapes, with a mirror directory's contents in each.
fn write_origin(origin: &Path, tag: &str, body: &[u8], latest: bool) -> PathBuf {
    write_release(&origin.join("download").join(tag), tag, body, &ASSETS);
    if latest {
        write_release(&origin.join("latest").join("download"), tag, body, &ASSETS);
    }
    origin.to_path_buf()
}

/// One release directory: the assets, the sums beside them, the BUILDINFO
/// whose `tag:` line says which release this is.
fn write_release(dir: &Path, tag: &str, body: &[u8], assets: &[&str]) {
    let sums: String = assets
        .iter()
        .map(|a| format!("{}  {a}\n", hex::encode(sha2::Sha256::digest(body))))
        .collect();
    let info = format!("tag: {tag}\ncommit: 0000\ndv_abi: 1\n");
    fs::create_dir_all(dir).unwrap();
    for asset in assets {
        fs::write(dir.join(asset), body).unwrap();
    }
    fs::write(dir.join("SHA256SUMS.txt"), &sums).unwrap();
    fs::write(dir.join("BUILDINFO.txt"), &info).unwrap();
}

fn dollup(home: &Path) -> Command {
    let mut cmd = common::dollup();
    cmd.env("HOME", home);
    cmd
}

fn project(dir: &Path) -> serde_json::Value {
    serde_json::from_slice(&fs::read(dir.join(".drt_root/project.json")).unwrap()).unwrap()
}

#[test]
fn pull_fills_the_cache_deploy_copies_from_it_and_pin_records_it() {
    let tmp = tempfile::tempdir().unwrap();
    let home = tmp.path().join("home");
    let body = b"#!/bin/sh\necho drt 9.9.9\n";
    let mirror = write_mirror(&tmp.path().join("mirror"), "v9.9.9", body);
    // `--from` names a release directory, laid out as the mirror lays one
    // out; `latest/` is one of them and says which version it is.
    let latest = format!("file://{}", mirror.join("latest").display());
    let from = format!("file://{}", mirror.join("v9.9.9").display());
    let root = tmp.path().join("root");
    run(dollup(&home)
        .arg("--root")
        .arg(&root)
        .args(["init", "demo"]));

    // `latest` is resolved to the version the source names, and cached
    // under that — never as "latest".
    let out = run(dollup(&home).args(["pull", "drt", "--from", &latest]));
    assert!(out.contains("cached drt 9.9.9"), "{out}");
    assert!(out.contains("checked: sha256 ok"), "{out}");
    let cache = home.join(".dollup/cache/drt/9.9.9");
    assert!(cache.join("SHA256SUMS.txt").is_file());
    assert!(cache.join("BUILDINFO.txt").is_file());
    assert!(!home.join(".dollup/cache/drt/latest").exists());
    assert!(!root.join(".drt_root/drt").exists(), "pull touches no root");
    let out = run(dollup(&home).args(["pull", "drt", "v9.9.9", "--from", &from]));
    assert!(out.contains("already cached"), "{out}");

    // Deploy needs a version or a pin; with one it copies from the cache,
    // with no source named at all, and the copy is a file, not a link.
    let msg = fail(
        dollup(&home)
            .arg("--root")
            .arg(&root)
            .args(["deploy", "drt"]),
    );
    assert!(
        msg.contains("name one") && msg.contains("or pin one"),
        "{msg}"
    );
    let out = run(dollup(&home)
        .arg("--root")
        .arg(&root)
        .args(["deploy", "drt", "9.9.9"]));
    assert!(out.contains("deployed drt 9.9.9 to"), "{out}");
    let binary = root.join(".drt_root/drt");
    assert_eq!(fs::read(&binary).unwrap(), body);
    assert!(!fs::symlink_metadata(&binary)
        .unwrap()
        .file_type()
        .is_symlink());
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_ne!(
            fs::metadata(&binary).unwrap().permissions().mode() & 0o111,
            0
        );
    }
    assert!(project(&root).get("drt").is_none(), "deploy does not pin");

    // Pin with no version: the deployed binary is identified through the
    // cache, by hash, and pinned as what it is.
    let out = run(dollup(&home).arg("--root").arg(&root).args(["pin", "drt"]));
    assert!(out.contains("pinned drt 9.9.9"), "{out}");
    assert_eq!(project(&root)["drt"], "9.9.9");
    // And audit agrees, offline, through the cached sums.
    let out = run(dollup(&home).arg("--root").arg(&root).arg("audit"));
    assert!(out.contains("drt: 9.9.9 pinned, 9.9.9 present"), "{out}");
    assert!(out.contains("per the cached sums"), "{out}");
    assert!(out.contains("start would run"), "{out}");

    // A pin that names the tag spelling records the version spelling: the
    // pin is the tag without its `v`, which is what drt compares its own
    // stamped tag against.
    let out = run(dollup(&home)
        .arg("--root")
        .arg(&root)
        .args(["pin", "drt", "v9.9.9"]));
    assert!(out.contains("pinned drt 9.9.9"), "{out}");
}

#[test]
fn pin_all_walks_the_recorded_roots_and_a_bare_pin_needs_something_to_identify() {
    let tmp = tempfile::tempdir().unwrap();
    let home = tmp.path().join("home");
    let mirror = write_mirror(&tmp.path().join("mirror"), "v9.9.9", b"#!/bin/sh\n");
    let from = format!("file://{}", mirror.join("v9.9.9").display());
    let a = tmp.path().join("a");
    let b = tmp.path().join("b");
    run(dollup(&home).arg("--root").arg(&a).args(["init", "alpha"]));
    run(dollup(&home).arg("--root").arg(&b).args(["init", "beta"]));

    // Nothing deployed and nothing cached: a bare pin has nothing to
    // identify and says what to do.
    let msg = fail(dollup(&home).arg("--root").arg(&a).args(["pin", "drt"]));
    assert!(msg.contains("name a version"), "{msg}");
    assert!(project(&a).get("drt").is_none());

    // --all takes a version, and pins every root that is still there.
    let msg = fail(dollup(&home).args(["pin", "drt", "--all"]));
    assert!(msg.contains("--all takes a version"), "{msg}");
    fs::remove_dir_all(&b).unwrap();
    let out = run(dollup(&home).args(["pin", "drt", "9.9.9", "--all", "--from", &from]));
    assert!(out.contains("pinned drt 9.9.9"), "{out}");
    assert!(
        out.contains("skipped"),
        "the removed root is named, not silently dropped: {out}"
    );
    assert_eq!(project(&a)["drt"], "9.9.9");
    assert!(a.join(".drt_root/drt").is_file());

    // Deploying a different release than the pin says so; pinning it
    // moves the pin.
    let other = write_mirror(
        &tmp.path().join("mirror2"),
        "v9.9.10",
        b"#!/bin/sh\necho ten\n",
    );
    let from2 = format!("file://{}", other.join("v9.9.10").display());
    let out = run(dollup(&home)
        .arg("--root")
        .arg(&a)
        .args(["deploy", "drt", "9.9.10", "--from", &from2]));
    assert!(
        out.contains("the pin is 9.9.9 and the binary is now 9.9.10"),
        "{out}"
    );
    // Which audit refuses, by name — both names, since the cache knows
    // what the binary is — and in start's own words.
    let out = fail(dollup(&home).arg("--root").arg(&a).arg("audit"));
    assert!(
        out.contains("is not a 9.9.9 build: it is 9.9.10 by sha256"),
        "{out}"
    );
    assert!(
        out.contains("the pinned drt is 9.9.9 and this binary is 9.9.10"),
        "{out}"
    );
    let out = run(dollup(&home).arg("--root").arg(&a).args(["pin", "drt"]));
    assert!(
        out.contains("pinned drt 9.9.10"),
        "identified through the cache: {out}"
    );
}

/// A candidate is its own release. drt compares a pin against the tag the
/// binary was cut from, minus the `v` — `0.5.0rc9` — while the crate inside
/// stays at `0.5.0`, so `0.5.0rc9` and `0.5.0` are two pins. dollup spells a
/// release the same way end to end, and audit's verdict on a mismatch is
/// start's, in start's words.
#[test]
fn a_candidate_is_its_own_release_and_a_pin_to_the_release_refuses_it() {
    let tmp = tempfile::tempdir().unwrap();
    let home = tmp.path().join("home");
    let rc = write_mirror(
        &tmp.path().join("mirror"),
        "v0.5.0rc9",
        b"#!/bin/sh\necho rc9\n",
    );
    let latest = format!("file://{}", rc.join("latest").display());
    let release = write_mirror(
        &tmp.path().join("mirror-release"),
        "v0.5.0",
        b"#!/bin/sh\necho final\n",
    );
    let from_release = format!("file://{}", release.join("v0.5.0").display());
    let root = tmp.path().join("root");
    run(dollup(&home)
        .arg("--root")
        .arg(&root)
        .args(["init", "demo"]));

    // `latest` names the candidate, through the `tag:` line the release
    // workflow writes first; it is cached and pinned as the tag without its
    // `v`, however it is spelled on the way in.
    let out = run(dollup(&home).args(["pull", "drt", "--from", &latest]));
    assert!(out.contains("cached drt 0.5.0rc9"), "{out}");
    assert!(home
        .join(".dollup/cache/drt/0.5.0rc9/SHA256SUMS.txt")
        .is_file());
    let out = run(dollup(&home)
        .arg("--root")
        .arg(&root)
        .args(["pin", "drt", "v0.5.0rc9"]));
    assert!(out.contains("pinned drt 0.5.0rc9"), "{out}");
    assert_eq!(project(&root)["drt"], "0.5.0rc9");
    let out = run(dollup(&home).arg("--root").arg(&root).arg("audit"));
    assert!(
        out.contains("drt: 0.5.0rc9 pinned, 0.5.0rc9 present"),
        "{out}"
    );
    assert!(out.contains("start would run"), "{out}");

    // The pin moved to the release by hand while the candidate stays
    // deployed: start refuses that by name, and so does audit, in the same
    // words — the release's cached sums disown the binary and the
    // candidate's name it.
    run(dollup(&home).args(["pull", "drt", "0.5.0", "--from", &from_release]));
    let mut pinned = project(&root);
    pinned["drt"] = "0.5.0".into();
    fs::write(
        root.join(".drt_root/project.json"),
        serde_json::to_vec_pretty(&pinned).unwrap(),
    )
    .unwrap();
    let out = fail(dollup(&home).arg("--root").arg(&root).arg("audit"));
    assert!(
        out.contains("the pinned drt is 0.5.0 and this binary is 0.5.0rc9"),
        "{out}"
    );
    assert!(
        out.contains("is not a 0.5.0 build: it is 0.5.0rc9 by sha256"),
        "{out}"
    );
    assert!(
        out.contains("`dollup pin drt 0.5.0rc9` moves the pin"),
        "{out}"
    );
    assert!(!out.contains("not verified"), "{out}");
}

/// Where a release comes from when nothing is named: the origin -- GitHub's
/// releases, whose download directory for a tag and `latest/download/` have
/// a mirror directory's layout -- and then the mirror, for as long as it
/// lags. A place that cannot be read is passed over and the next asked,
/// said; a place whose bytes disagree with its own sums is a refusal that no
/// later place papers over. An explicit `--from` never falls back.
#[test]
fn the_origin_is_asked_first_the_mirror_second_and_a_mismatch_stops_the_search() {
    let tmp = tempfile::tempdir().unwrap();
    let home = tmp.path().join("home");
    // The origin: the newest stable release, which `latest` names, and a
    // candidate, which no mirror carries.
    let origin = write_origin(
        &tmp.path().join("origin"),
        "v9.9.9",
        b"#!/bin/sh\necho nine\n",
        true,
    );
    write_origin(&origin, "v9.9.10rc1", b"#!/bin/sh\necho candidate\n", false);
    // The mirror: lagging, and holding a release the origin no longer has.
    let mirror = write_mirror(
        &tmp.path().join("mirror"),
        "v9.9.8",
        b"#!/bin/sh\necho eight\n",
    );
    let env = |cmd: &mut Command| {
        cmd.env(
            "DOLLUP_DRT_RELEASES",
            format!("file://{}", origin.display()),
        )
        .env("DOLLUP_DRT_MIRROR", format!("file://{}", mirror.display()));
    };

    // At the origin: taken from it, nothing said. A candidate too, with
    // nothing named -- the mirror is never asked.
    let mut cmd = dollup(&home);
    env(&mut cmd);
    let out = run(cmd.args(["pull", "drt", "9.9.10rc1"]));
    assert!(out.contains("cached drt 9.9.10rc1"), "{out}");
    assert!(
        out.contains(&format!(
            "fetching file://{}/download/v9.9.10rc1/",
            origin.display()
        )),
        "{out}"
    );
    assert!(!out.contains("did not answer"), "{out}");
    assert!(out.contains("checked: sha256 ok"), "{out}");

    // `latest` is the origin's newest stable release, not the mirror's.
    let mut cmd = dollup(&home);
    env(&mut cmd);
    let out = run(cmd.args(["pull", "drt"]));
    assert!(out.contains("cached drt 9.9.9"), "{out}");
    assert!(!home.join(".dollup/cache/drt/9.9.8").exists());

    // Not at the origin: taken from the mirror, and said, in the note and
    // in the report.
    let mut cmd = dollup(&home);
    env(&mut cmd);
    let out = run(cmd.args(["pull", "drt", "9.9.8"]));
    assert!(
        out.contains("note: the origin did not answer for v9.9.8"),
        "{out}"
    );
    assert!(
        out.contains(&format!(
            "from: file://{}/v9.9.8 (the mirror); the origin did not answer for v9.9.8",
            mirror.display()
        )),
        "{out}"
    );
    assert!(out.contains("cached drt 9.9.8"), "{out}");
    assert!(out.contains("checked: sha256 ok"), "{out}");
    assert!(home
        .join(".dollup/cache/drt/9.9.8/SHA256SUMS.txt")
        .is_file());

    // Nowhere: both places are named, with why.
    let mut cmd = dollup(&home);
    env(&mut cmd);
    let msg = fail(cmd.args(["pull", "drt", "9.9.11"]));
    assert!(
        msg.contains("v9.9.11 is at neither the origin nor the mirror"),
        "{msg}"
    );
    assert!(
        msg.contains(&format!(
            "the origin (file://{}/download/v9.9.11)",
            origin.display()
        )) && msg.contains(&format!("the mirror (file://{}/v9.9.11)", mirror.display())),
        "{msg}"
    );

    // The origin's bytes are not what its own sums say: refused, not
    // passed over. The mirror's good copy is never asked for, because
    // "somewhere else" is not the answer to "someone changed something".
    write_origin(&origin, "v9.9.12", b"#!/bin/sh\necho twelve\n", false);
    for asset in ASSETS {
        fs::write(
            origin.join("download/v9.9.12").join(asset),
            b"#!/bin/sh\necho tampered\n",
        )
        .unwrap();
    }
    write_release(
        &mirror.join("v9.9.12"),
        "v9.9.12",
        b"#!/bin/sh\necho twelve\n",
        &ASSETS,
    );
    let mut cmd = dollup(&home);
    env(&mut cmd);
    let msg = fail(cmd.args(["pull", "drt", "9.9.12"]));
    assert!(msg.contains("checksum mismatch"), "{msg}");
    assert!(!msg.contains("asking the mirror"), "{msg}");
    assert!(!home.join(".dollup/cache/drt/9.9.12").exists());

    // `--from` is the operator's word: one directory, no fallback, and the
    // directory's own error.
    let mut cmd = dollup(&home);
    env(&mut cmd);
    let nowhere = format!("file://{}/download/v9.9.11", origin.display());
    let msg = fail(cmd.args(["pull", "drt", "9.9.11", "--from", &nowhere]));
    // Not a release directory at all, which is a different complaint from a
    // release directory with no build for this box: nothing dollup reads is
    // there, the sums included.
    assert!(msg.contains("nothing dollup reads is at"), "{msg}");
    assert!(msg.contains("not a release directory"), "{msg}");
    assert!(!msg.contains("the mirror"), "{msg}");

    // Audit on a box with no cache: the sums for a candidate come from the
    // origin, and for a release the origin no longer has from the mirror,
    // so the binary is verified either way.
    let home2 = tmp.path().join("home2");
    let root = tmp.path().join("root");
    let mut cmd = dollup(&home2);
    env(&mut cmd);
    run(cmd.arg("--root").arg(&root).args(["init", "demo"]));
    let pin = |version: &str| {
        let mut pinned = project(&root);
        pinned["drt"] = version.into();
        fs::write(
            root.join(".drt_root/project.json"),
            serde_json::to_vec_pretty(&pinned).unwrap(),
        )
        .unwrap();
    };
    fs::write(root.join(".drt_root/drt"), b"#!/bin/sh\necho candidate\n").unwrap();
    pin("9.9.10rc1");
    let mut cmd = dollup(&home2);
    env(&mut cmd);
    let out = run(cmd.arg("--root").arg(&root).arg("audit"));
    assert!(
        out.contains("drt: 9.9.10rc1 pinned, 9.9.10rc1 present"),
        "{out}"
    );
    assert!(
        out.contains(&format!(
            "per file://{}/download/v9.9.10rc1/SHA256SUMS.txt",
            origin.display()
        )),
        "{out}"
    );
    fs::write(root.join(".drt_root/drt"), b"#!/bin/sh\necho eight\n").unwrap();
    pin("9.9.8");
    let mut cmd = dollup(&home2);
    env(&mut cmd);
    let out = run(cmd.arg("--root").arg(&root).arg("audit"));
    assert!(out.contains("drt: 9.9.8 pinned, 9.9.8 present"), "{out}");
    assert!(
        out.contains(&format!(
            "per file://{}/v9.9.8/SHA256SUMS.txt",
            mirror.display()
        )),
        "{out}"
    );
}

/// A pin written under the old candidate spelling names the same release as
/// a binary cut under the new one (doc/ALIGNMENT.md §10): `deploy` raises
/// no mismatch, `audit` says one version under two spellings, and start
/// would run -- the comparison is drt-config's, the one start makes.
/// Existing tags are never respelled, so the pin is fetched under the
/// spelling it was written in and a wrong spelling still fails by name.
#[test]
fn a_pin_in_the_old_spelling_matches_a_binary_in_the_new_one() {
    let tmp = tempfile::tempdir().unwrap();
    let home = tmp.path().join("home");
    let origin = write_origin(
        &tmp.path().join("origin"),
        "v9.9.13-rc.1",
        b"#!/bin/sh\necho rc\n",
        false,
    );
    let mirror = tmp.path().join("mirror");
    fs::create_dir_all(&mirror).unwrap();
    let env = |cmd: &mut Command| {
        cmd.env(
            "DOLLUP_DRT_RELEASES",
            format!("file://{}", origin.display()),
        )
        .env("DOLLUP_DRT_MIRROR", format!("file://{}", mirror.display()));
    };
    let root = tmp.path().join("root");
    let mut cmd = dollup(&home);
    env(&mut cmd);
    run(cmd.arg("--root").arg(&root).args(["init", "demo"]));
    let mut cmd = dollup(&home);
    env(&mut cmd);
    let out = run(cmd
        .arg("--root")
        .arg(&root)
        .args(["pin", "drt", "9.9.13-rc.1"]));
    assert!(out.contains("pinned drt 9.9.13-rc.1"), "{out}");

    // The pin rewritten in the old spelling, as a root from before the
    // cutover carries it.
    let mut pinned = project(&root);
    pinned["drt"] = "9.9.13rc1".into();
    fs::write(
        root.join(".drt_root/project.json"),
        serde_json::to_vec_pretty(&pinned).unwrap(),
    )
    .unwrap();
    let mut cmd = dollup(&home);
    env(&mut cmd);
    let out = run(cmd.arg("--root").arg(&root).arg("audit"));
    assert!(
        out.contains("drt: 9.9.13rc1 pinned, 9.9.13-rc.1 present"),
        "{out}"
    );
    assert!(out.contains("one version under two spellings"), "{out}");
    assert!(out.contains("start would run"), "{out}");
    let mut cmd = dollup(&home);
    env(&mut cmd);
    let out = run(cmd
        .arg("--root")
        .arg(&root)
        .args(["deploy", "drt", "9.9.13-rc.1"]));
    assert!(!out.contains("the pin is"), "no mismatch note: {out}");

    // A spelling nothing was ever tagged under is looked for as written,
    // and fails by name rather than being rewritten into a guess.
    let mut cmd = dollup(&home);
    env(&mut cmd);
    let msg = fail(cmd.args(["pull", "drt", "9.9.13rc1"]));
    assert!(
        msg.contains("v9.9.13rc1 is at neither the origin nor the mirror"),
        "{msg}"
    );
}

/// A release named the aligned way is found by its sums, not by a guess
/// from its version: the same pull reads a release under either spelling,
/// caches it under the name it carries, and deploys it.
#[test]
fn a_release_under_the_aligned_asset_names_is_read_by_its_sums() {
    let tmp = tempfile::tempdir().unwrap();
    let home = tmp.path().join("home");
    let body = b"#!/bin/sh\necho aligned\n";
    let mirror = write_mirror_with(&tmp.path().join("mirror"), "v9.9.12", body, &ALIGNED_ASSETS);
    let from = format!("file://{}", mirror.join("v9.9.12").display());
    let out = run(dollup(&home).args(["pull", "drt", "9.9.12", "--from", &from]));
    assert!(out.contains("cached drt 9.9.12"), "{out}");
    assert!(out.contains("checked: sha256 ok"), "{out}");
    #[cfg(target_os = "linux")]
    assert!(
        home.join(".dollup/cache/drt/9.9.12/drt_linux_x86_64_musl")
            .is_file(),
        "cached under the name the release carries"
    );
    let out = run(dollup(&home).args(["pull", "drt", "9.9.12", "--from", &from]));
    assert!(out.contains("already cached"), "{out}");
    let root = tmp.path().join("root");
    run(dollup(&home)
        .arg("--root")
        .arg(&root)
        .args(["init", "demo"]));
    let out = run(dollup(&home)
        .arg("--root")
        .arg(&root)
        .args(["pin", "drt", "9.9.12"]));
    assert!(out.contains("pinned drt 9.9.12"), "{out}");
    assert_eq!(fs::read(root.join(".drt_root/drt")).unwrap(), body);
    let out = run(dollup(&home).arg("--root").arg(&root).arg("audit"));
    assert!(out.contains("drt: 9.9.12 pinned, 9.9.12 present"), "{out}");
}

/// The mistake the help used to invite: `--from` is a release *directory*,
/// and the URL a releases page offers to copy is the asset's own. Pasting
/// that made dollup ask for `<asset>/<asset>` and then report the doubled
/// directory as though it were what had been asked for. Caught by name now,
/// before a request goes out, with the URL that would have worked.
#[test]
fn a_from_that_names_the_asset_rather_than_its_directory_is_caught_by_name() {
    let tmp = tempfile::tempdir().unwrap();
    let home = tmp.path().join("home");
    let body = b"#!/bin/sh\necho drt 9.9.9\n";
    let mirror = write_mirror(&tmp.path().join("mirror"), "v9.9.9", body);
    let dir = mirror.join("v9.9.9");
    let out = tmp.path().join("out");
    fs::create_dir_all(&out).unwrap();

    // Any platform's asset name, not just this box's: the URL someone pastes
    // is as likely to name another platform's binary, and the mistake is the
    // same one.
    for asset in ["drt_linux_x86_64_musl", "drt_darwin_arm64", "BUILDINFO.txt"] {
        let from = format!("file://{}/{asset}", dir.display());
        let msg = fail(dollup(&home).args([
            "get",
            "drt",
            "--version",
            "9.9.9",
            "--from",
            &from,
            "--out",
            out.to_str().unwrap(),
        ]));
        assert!(msg.contains("--from takes a release directory"), "{msg}");
        assert!(msg.contains(asset), "the segment by name: {msg}");
        // The URL that works, ready to run.
        assert!(
            msg.contains(&format!("--from file://{}", dir.display())),
            "{msg}"
        );
        // The doubled path is named as the explanation — that is the fact
        // the old message buried — but it is never asked for: the refusal
        // lands before the first request, so no URL is read.
        assert!(msg.contains(&format!("{asset}/{asset}")), "{msg}");
        assert!(!msg.contains("reading file://"), "{msg}");
        assert!(!msg.contains("fetching file://"), "{msg}");
        // And the failure path says the destination was left alone.
        assert!(msg.contains("nothing was written"), "{msg}");
        assert!(!out.join("drt").exists(), "{msg}");
    }

    // The same directory without the filename is the whole point: it works.
    let from = format!("file://{}", dir.display());
    let got = run(dollup(&home).args([
        "get",
        "drt",
        "--version",
        "9.9.9",
        "--from",
        &from,
        "--out",
        out.to_str().unwrap(),
    ]));
    assert!(got.contains("drt 9.9.9"), "{got}");
    assert_eq!(fs::read(out.join("drt")).unwrap(), body);
}

/// A directory that answers with its sums but carries no build for this box
/// is a different complaint from a URL that is not a release directory at
/// all, and the second is what a file masquerading as a directory looks
/// like. One message each, and the file case said outright.
#[test]
fn a_release_directory_with_no_build_for_this_box_is_not_a_missing_directory() {
    let tmp = tempfile::tempdir().unwrap();
    let home = tmp.path().join("home");
    let body = b"#!/bin/sh\necho elsewhere\n";
    let out = tmp.path().join("out");
    fs::create_dir_all(&out).unwrap();
    let get = |from: &str| -> String {
        fail(dollup(&home).args([
            "get",
            "drt",
            "--version",
            "9.9.9",
            "--from",
            from,
            "--out",
            out.to_str().unwrap(),
        ]))
    };

    // Laid out right, sums and all, but every asset is for a platform no
    // box in this test is.
    let elsewhere = tmp.path().join("elsewhere/v9.9.9");
    write_release(&elsewhere, "v9.9.9", body, &["drt_plan9_vax"]);
    let msg = get(&format!("file://{}", elsewhere.display()));
    assert!(msg.contains("is a release directory, but carries"), "{msg}");
    assert!(msg.contains("no drt for this platform"), "{msg}");
    assert!(!msg.contains("drop the last segment"), "{msg}");

    // A file. dollup can see that this one is, so it says so rather than
    // listing what it failed to find inside it.
    let file = tmp.path().join("v9.9.9.tar.gz");
    fs::write(&file, body).unwrap();
    let msg = get(&format!("file://{}", file.display()));
    assert!(msg.contains("is a file, not a release directory"), "{msg}");
    assert!(msg.contains("drop the last segment"), "{msg}");
}

/// `--version` never reaches the URL when `--from` is given — the directory
/// named *is* the release — so the report reads the version back off the
/// BUILDINFO.txt it fetched rather than repeating what was asked for. A
/// label that can disagree with the file is worse than no label.
#[test]
fn get_reports_the_version_the_source_served_not_the_one_asked_for() {
    let tmp = tempfile::tempdir().unwrap();
    let home = tmp.path().join("home");
    let body = b"#!/bin/sh\necho drt 9.9.9\n";
    let mirror = write_mirror(&tmp.path().join("mirror"), "v9.9.9", body);
    let from = format!("file://{}", mirror.join("v9.9.9").display());
    let out = tmp.path().join("out");
    fs::create_dir_all(&out).unwrap();

    let got = run(dollup(&home).args([
        "get",
        "drt",
        "--version",
        "9.9.8",
        "--from",
        &from,
        "--out",
        out.to_str().unwrap(),
    ]));
    assert!(got.contains("drt 9.9.9"), "what was written: {got}");
    assert!(!got.contains("drt 9.9.8)"), "not what was asked for: {got}");
    assert!(
        got.contains("asked for drt 9.9.8 and the source served 9.9.9"),
        "{got}"
    );
    assert_eq!(fs::read(out.join("drt")).unwrap(), body);

    // `pull` keys the cache by version and `audit` reads that key back, so
    // there the same disagreement is not a label to correct but a directory
    // not to write.
    let msg = fail(dollup(&home).args(["pull", "drt", "9.9.8", "--from", &from]));
    assert!(msg.contains("serves 9.9.9"), "{msg}");
    assert!(msg.contains("tag: v9.9.9"), "{msg}");
    assert!(!home.join(".dollup/cache/drt/9.9.8").exists(), "{msg}");
    // Asked for what it is, it caches.
    let out = run(dollup(&home).args(["pull", "drt", "9.9.9", "--from", &from]));
    assert!(out.contains("cached drt 9.9.9"), "{out}");
}

/// `get` writes into the working directory, where a `drt` from an earlier
/// run is usually already sitting, so a failure has to say that the file
/// there is not the one just asked for.
#[test]
fn a_failed_get_says_the_destination_was_left_alone() {
    let tmp = tempfile::tempdir().unwrap();
    let home = tmp.path().join("home");
    let out = tmp.path().join("out");
    fs::create_dir_all(&out).unwrap();
    let nowhere = format!("file://{}/nothing/v9.9.9", tmp.path().display());
    let get = |out: &Path| -> String {
        fail(dollup(&home).args([
            "get",
            "drt",
            "--version",
            "9.9.9",
            "--from",
            &nowhere,
            "--out",
            out.to_str().unwrap(),
        ]))
    };

    // Nothing there to confuse anyone with, and it says so.
    let msg = get(&out);
    assert!(msg.contains("nothing was written"), "{msg}");
    assert!(msg.contains("was not created"), "{msg}");
    assert!(!out.join("drt").exists(), "{msg}");

    // An older binary sitting at the destination: untouched, and named as
    // untouched, which is the question the output could not answer.
    let older = b"#!/bin/sh\necho an older drt\n";
    fs::write(out.join("drt"), older).unwrap();
    let msg = get(&out);
    assert!(msg.contains("nothing was written"), "{msg}");
    assert!(msg.contains("is whatever it was before this ran"), "{msg}");
    assert_eq!(fs::read(out.join("drt")).unwrap(), older);
}

/// `DRT_VERSION` is drt's own installer knob. dollup is right to ignore it,
/// but ignoring it silently leaves the operator with two confusing facts at
/// once: the variable did nothing, and the version that arrived is whatever
/// `latest` means — the newest *stable* release, which can be far behind a
/// dev build. The note is only a note; it changes nothing.
#[test]
fn an_ignored_drt_installer_variable_is_named() {
    let tmp = tempfile::tempdir().unwrap();
    let home = tmp.path().join("home");
    let body = b"#!/bin/sh\necho drt 9.9.9\n";
    let mirror = write_mirror(&tmp.path().join("mirror"), "v9.9.9", body);
    let from = format!("file://{}", mirror.join("v9.9.9").display());

    let out = run(dollup(&home)
        .env("DRT_VERSION", "v9.9.99-dev.3")
        .args(["pull", "drt", "9.9.9", "--from", &from]));
    assert!(out.contains("DRT_VERSION=v9.9.99-dev.3"), "{out}");
    assert!(out.contains("drt's installer knob, not dollup's"), "{out}");
    assert!(out.contains("this run: 9.9.9"), "{out}");
    // Ignored, not obeyed: the release asked for is the one cached.
    assert!(out.contains("cached drt 9.9.9"), "{out}");

    // A variable that agrees with the version in hand is nobody's
    // confusion, whichever spelling it is written in.
    let out = run(dollup(&home)
        .env("DRT_VERSION", "v9.9.9")
        .args(["pull", "drt", "9.9.9", "--from", &from]));
    assert!(!out.contains("DRT_VERSION"), "{out}");
}
