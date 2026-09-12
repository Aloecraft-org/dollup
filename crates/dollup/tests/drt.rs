//! The runtime's lifecycle in a root: `pull drt` fills the cache and
//! touches no root, `deploy drt` copies from the cache into `.drt_root/drt`
//! (never a link), `pin drt` deploys and records, and `audit` then says
//! the pin and the binary agree — by hash, never by running it. Against a
//! fake mirror on disk (`--from file://`), so nothing here reaches the
//! network, and every platform's asset name is present so the test does
//! not care which one this box wants.

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

/// A release directory as the mirror lays one out, with a `latest/` that
/// says which version it is.
fn write_mirror(mirror: &Path, tag: &str, body: &[u8]) -> PathBuf {
    let sums: String = ASSETS
        .iter()
        .map(|a| format!("{}  {a}\n", hex::encode(sha2::Sha256::digest(body))))
        .collect();
    let info = format!("tag: {tag}\ncommit: 0000\ndv_abi: 1\n");
    for dir in [mirror.join(tag), mirror.join("latest")] {
        fs::create_dir_all(&dir).unwrap();
        for asset in ASSETS {
            fs::write(dir.join(asset), body).unwrap();
        }
        fs::write(dir.join("SHA256SUMS.txt"), &sums).unwrap();
        fs::write(dir.join("BUILDINFO.txt"), &info).unwrap();
    }
    mirror.to_path_buf()
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
    // pin is what `drt buildinfo` reports, and that has no `v`.
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
    // Which audit refuses, by name: start would too.
    let out = fail(dollup(&home).arg("--root").arg(&a).arg("audit"));
    assert!(out.contains("is not a 9.9.9 build"), "{out}");
    let out = run(dollup(&home).arg("--root").arg(&a).args(["pin", "drt"]));
    assert!(
        out.contains("pinned drt 9.9.10"),
        "identified through the cache: {out}"
    );
}
