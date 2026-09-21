//! `dollup install drt`: the runtime onto the PATH rather than into a root.
//!
//! The other scope from `tests/drt.rs`, which covers the per-root lifecycle.
//! What is asserted here is what makes installing different from `get`: a
//! destination chosen rather than named, a binary checked *before* it takes
//! the place of one that works, and the two notes that stop an install from
//! being quietly useless — the prefix that is not on PATH, and the roots
//! that pin something else and will go on running it.
//!
//! Against a `file://` release directory, so nothing here reaches the
//! network.

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

const ASSETS: [&str; 4] = [
    "drt_linux_x86_64_musl",
    "drt_linux_static_x86_64",
    "drt_darwin_arm64",
    "drt_darwin_x86_64",
];

/// A release directory laid out the way every source lays one out.
fn write_release(dir: &Path, tag: &str, body: &[u8]) {
    fs::create_dir_all(dir).unwrap();
    let sums: String = ASSETS
        .iter()
        .map(|a| format!("{}  {a}\n", hex::encode(sha2::Sha256::digest(body))))
        .collect();
    for asset in ASSETS {
        fs::write(dir.join(asset), body).unwrap();
    }
    fs::write(dir.join("SHA256SUMS.txt"), sums).unwrap();
    fs::write(
        dir.join("BUILDINFO.txt"),
        format!("tag: {tag}\ncommit: 0000\n"),
    )
    .unwrap();
}

/// A "drt" that answers `--version`, which is the check install makes.
fn working(version: &str) -> Vec<u8> {
    format!("#!/bin/sh\necho 'drt {version}'\n").into_bytes()
}

fn dollup(home: &Path) -> Command {
    let mut cmd = common::dollup();
    cmd.env("HOME", home);
    cmd
}

fn install(home: &Path, version: &str, from: &Path, prefix: &Path) -> Command {
    let mut cmd = dollup(home);
    cmd.args(["install", "drt", version, "--from"])
        .arg(format!("file://{}", from.display()))
        .arg("--prefix")
        .arg(prefix);
    cmd
}

/// The happy path, and the two things that make it an install rather than a
/// download: the file is executable where it was put, and the report names
/// where that is.
#[test]
fn install_puts_a_runnable_drt_at_the_prefix_and_names_it() {
    let tmp = tempfile::tempdir().unwrap();
    let home = tmp.path().join("home");
    let rel = tmp.path().join("rel/v9.9.9");
    let prefix = tmp.path().join("bin");
    write_release(&rel, "v9.9.9", &working("9.9.9"));

    let out = run(&mut install(&home, "9.9.9", &rel, &prefix));
    assert!(out.contains("installed drt 9.9.9 to"), "{out}");
    assert!(
        out.contains(&prefix.join("drt").display().to_string()),
        "{out}"
    );

    let dest = prefix.join("drt");
    assert!(dest.is_file(), "the binary is at the prefix");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_ne!(
            fs::metadata(&dest).unwrap().permissions().mode() & 0o111,
            0,
            "installed executable"
        );
    }
    // It really runs -- which is the check install itself made.
    let said = Command::new(&dest).arg("--version").output().unwrap();
    assert_eq!(String::from_utf8_lossy(&said.stdout).trim(), "drt 9.9.9");
    // Nothing but the binary is left in the prefix: the staging file the
    // check ran against is gone either way.
    assert_eq!(fs::read_dir(&prefix).unwrap().count(), 1, "no strays");

    // A prefix off PATH is said, because a binary the shell will not find is
    // an install that looks like it did nothing.
    assert!(out.contains("is not on your PATH"), "{out}");

    // And it fills the same cache the root verbs read, so the second install
    // asks no source anything.
    let again = run(&mut install(&home, "9.9.9", &rel, &prefix));
    assert!(
        !again.contains("fetching"),
        "second install refetched: {again}"
    );
    assert!(again.contains("installed drt 9.9.9 to"), "{again}");
}

/// The rule `get` learned in 0.1.2, applied where it matters more: a
/// refusal must not have already replaced something that worked.
#[cfg(unix)]
#[test]
fn a_drt_that_does_not_run_here_never_displaces_one_that_does() {
    let tmp = tempfile::tempdir().unwrap();
    let home = tmp.path().join("home");
    let prefix = tmp.path().join("bin");
    let good = tmp.path().join("rel/v9.9.9");
    let bad = tmp.path().join("rel/v8.8.8");
    write_release(&good, "v9.9.9", &working("9.9.9"));
    // Not an executable on any platform.
    write_release(&bad, "v8.8.8", &[0x00, 0xff, 0xfe, 0x7f].repeat(8));

    run(&mut install(&home, "9.9.9", &good, &prefix));
    let dest = prefix.join("drt");
    let before = fs::read(&dest).unwrap();

    let msg = fail(&mut install(&home, "8.8.8", &bad, &prefix));
    assert!(msg.contains("does not run here"), "{msg}");
    // Named as untouched, and actually untouched.
    assert!(msg.contains("is still the drt that was there"), "{msg}");
    assert_eq!(fs::read(&dest).unwrap(), before, "the good drt survived");
    assert_eq!(fs::read_dir(&prefix).unwrap().count(), 1, "no strays");

    // With nothing installed yet, the same refusal says so instead.
    let empty = tmp.path().join("bin2");
    let msg = fail(&mut install(&home, "8.8.8", &bad, &empty));
    assert!(msg.contains("nothing was written to"), "{msg}");
    assert!(!empty.join("drt").exists(), "{msg}");
}

/// A drt on the PATH runs nothing in a root -- `drt start` there uses
/// `.drt_root/drt` -- so a root pinned elsewhere is named once, rather than
/// left as a surprise for whoever wonders which drt ran.
#[test]
fn roots_pinned_to_another_version_are_named_and_a_matching_one_is_not() {
    let tmp = tempfile::tempdir().unwrap();
    let home = tmp.path().join("home");
    let rel = tmp.path().join("rel/v9.9.9");
    let prefix = tmp.path().join("bin");
    let root = tmp.path().join("root");
    write_release(&rel, "v9.9.9", &working("9.9.9"));

    run(dollup(&home)
        .arg("--root")
        .arg(&root)
        .args(["init", "demo"]));
    let pin = |version: &str| {
        let path = root.join(".drt_root/project.json");
        let mut project: serde_json::Value =
            serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
        project["drt"] = version.into();
        fs::write(&path, serde_json::to_vec_pretty(&project).unwrap()).unwrap();
    };

    pin("9.9.8");
    let out = run(&mut install(&home, "9.9.9", &rel, &prefix));
    assert!(out.contains("pin a different drt"), "{out}");
    assert!(out.contains(&root.display().to_string()), "{out}");
    assert!(out.contains("9.9.8"), "{out}");
    // It is a note, not a refusal: the install happened.
    assert!(out.contains("installed drt 9.9.9 to"), "{out}");

    // The same version, in either spelling, is nobody's confusion.
    pin("9.9.9");
    let out = run(&mut install(&home, "9.9.9", &rel, &prefix));
    assert!(!out.contains("pin a different drt"), "{out}");
}

/// `install` knows one thing today, and says which when asked for another.
#[test]
fn install_knows_only_drt() {
    let tmp = tempfile::tempdir().unwrap();
    let msg = fail(dollup(&tmp.path().join("home")).args(["install", "hello"]));
    assert!(msg.contains("knows only `drt` today"), "{msg}");
}

/// The prefix is a directory that gets created, so `--prefix` into a path
/// that does not exist yet works rather than refusing.
#[test]
fn a_prefix_that_does_not_exist_yet_is_created() {
    let tmp = tempfile::tempdir().unwrap();
    let home = tmp.path().join("home");
    let rel = tmp.path().join("rel/v9.9.9");
    let prefix: PathBuf = tmp.path().join("nested/deep/bin");
    write_release(&rel, "v9.9.9", &working("9.9.9"));

    let out = run(&mut install(&home, "9.9.9", &rel, &prefix));
    assert!(out.contains("installed drt 9.9.9 to"), "{out}");
    assert!(prefix.join("drt").is_file(), "{out}");
}
