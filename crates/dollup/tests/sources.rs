//! The source list is a fallback list (RepoFormat.md §1): a source that
//! cannot be read is passed over and said; one that refuses is fatal
//! (THREAT-NOTES.md). A dead first source never masquerades as a missing
//! package, and never lowers the bar for the source that answers.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

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

fn dollup(home: &Path) -> Command {
    let mut cmd = common::dollup();
    cmd.env("HOME", home);
    cmd
}

/// A repo holding one guest-only library, sealed and indexed by the tool.
fn write_repo(home: &Path, dir: &Path, name: &str) -> String {
    let pkg = dir.join("packages").join(name).join("0.1.0");
    fs::create_dir_all(pkg.join("guest")).unwrap();
    fs::write(
        pkg.join("guest").join(format!("{name}.dlua")),
        format!("local M = {{}}\nM.name = '{name}'\nreturn M\n"),
    )
    .unwrap();
    fs::write(
        pkg.join("manifest.json"),
        format!(
            r#"{{ "name": "{name}", "version": "0.1.0",
  "guest": {{ "modules": {{ "{name}": "guest/{name}.dlua" }}, "source_only": true }} }}"#
        ),
    )
    .unwrap();
    run(dollup(home).arg("repo").arg("seal").arg(&pkg));
    run(dollup(home).arg("repo").arg("index").arg(dir));
    format!("file://{}", dir.display())
}

/// A root whose only sources are the ones the test adds: the scaffolded
/// standard source is a network source, and a test never talks to one.
fn root(home: &Path, dir: &Path) -> PathBuf {
    run(dollup(home).arg("--root").arg(dir).args(["init", "demo"]));
    run(dollup(home).arg("--root").arg(dir).args([
        "source",
        "rm",
        "https://dollup.aloecraft.org/std-repo/",
    ]));
    dir.to_path_buf()
}

#[test]
fn a_source_that_cannot_be_read_is_passed_over_and_said() {
    let tmp = tempfile::tempdir().unwrap();
    let home = tmp.path().join("home");
    let good = write_repo(&home, &tmp.path().join("good"), "lib");
    let missing = format!("file://{}", tmp.path().join("nope").display());
    let empty_dir = tmp.path().join("empty");
    fs::create_dir_all(&empty_dir).unwrap();
    let no_index = format!("file://{}", empty_dir.display());

    // Not there at all, then a directory that is not a repo, then the one
    // that has it: both skips are said, and the pull lands from the third.
    let dir = root(&home, &tmp.path().join("root"));
    for url in [&missing, &no_index, &good] {
        run(dollup(&home)
            .arg("--root")
            .arg(&dir)
            .args(["source", "add", url]));
    }
    let out = run(dollup(&home).arg("--root").arg(&dir).args(["pull", "lib"]));
    assert!(
        out.contains(&format!("skipped {missing}: ")) && out.contains("does not exist"),
        "{out}"
    );
    assert!(
        out.contains(&format!("skipped {no_index}: ")) && out.contains("has no index.json"),
        "{out}"
    );
    assert!(out.contains(&format!("lib 0.1.0 ← {good}")), "{out}");
    assert!(dir.join(".drt_root/init/lib.dlua").is_file());
    let ls = run(dollup(&home).arg("--root").arg(&dir).arg("ls"));
    assert!(
        ls.contains(&format!("lib 0.1.0 (unsigned) ← {good}")),
        "{ls}"
    );

    // `info` reads the same list the same way.
    let out = run(dollup(&home).arg("--root").arg(&dir).args(["info", "lib"]));
    assert!(out.contains(&format!("skipped {missing}: ")), "{out}");
    assert!(out.contains(&format!("lib 0.1.0 ← {good}")), "{out}");
}

#[test]
fn when_nothing_has_it_the_refusal_says_which_sources_were_not_read() {
    let tmp = tempfile::tempdir().unwrap();
    let home = tmp.path().join("home");
    let good = write_repo(&home, &tmp.path().join("good"), "lib");
    let missing = format!("file://{}", tmp.path().join("nope").display());

    let dir = root(&home, &tmp.path().join("root"));
    for url in [&missing, &good] {
        run(dollup(&home)
            .arg("--root")
            .arg(&dir)
            .args(["source", "add", url]));
    }
    let msg = fail(
        dollup(&home)
            .arg("--root")
            .arg(&dir)
            .args(["pull", "other"]),
    );
    assert!(
        msg.contains("'other' is in none of 2 source(s), 1 of which could not be read:"),
        "{msg}"
    );
    assert!(msg.contains(&format!("{missing}: ")), "{msg}");
    assert!(
        !msg.contains(&format!("{good}: ")),
        "the readable one is not listed: {msg}"
    );
}

#[test]
fn a_source_that_refuses_is_never_skipped() {
    let tmp = tempfile::tempdir().unwrap();
    let home = tmp.path().join("home");
    fs::create_dir_all(&home).unwrap();
    let signed_dir = tmp.path().join("signed");
    let signed = write_repo(&home, &signed_dir, "lib");
    let good = write_repo(&home, &tmp.path().join("good"), "lib");

    // Sign the first repo, pin its key, then tamper with its index: the
    // signature no longer verifies. The second source has the package too.
    let key = home.join("repo.key");
    run(dollup(&home)
        .arg("repo")
        .arg("keygen")
        .arg("--out")
        .arg(&key));
    run(dollup(&home)
        .arg("repo")
        .arg("sign")
        .arg(&signed_dir)
        .arg("--key-file")
        .arg(&key));
    let pubkey = run(dollup(&home)
        .arg("repo")
        .arg("pubkey")
        .arg("--key-file")
        .arg(&key))
    .trim()
    .to_string();
    let index = signed_dir.join("index.json");
    let mut bytes = fs::read(&index).unwrap();
    bytes.push(b'\n');
    fs::write(&index, bytes).unwrap();

    let dir = root(&home, &tmp.path().join("root"));
    run(dollup(&home)
        .arg("--root")
        .arg(&dir)
        .args(["source", "add", &signed, "--key", &pubkey]));
    run(dollup(&home)
        .arg("--root")
        .arg(&dir)
        .args(["source", "add", &good]));
    let msg = fail(dollup(&home).arg("--root").arg(&dir).args(["pull", "lib"]));
    assert!(msg.contains("signature verification failed"), "{msg}");
    assert!(!msg.contains("skipped"), "a refusal is not a skip: {msg}");
    assert!(!dir.join(".drt_root/init/lib.dlua").exists());
}
