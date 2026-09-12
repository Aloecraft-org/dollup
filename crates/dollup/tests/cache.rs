//! The cache every root on this box shares, and `gc` across every root:
//! blobs live under `~/.dollup/cache/store`, a sweep from one root keeps
//! what every recorded root's lock references, a root that is gone is
//! skipped and said, and a root with no HOME to keep a cache in is refused
//! by name. `dollup new` is the mkdir-plus-init it says it is.

use std::collections::BTreeMap;
use std::fs;
use std::path::Path;
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

/// A guest-only package in a repo tree, hashed as `repo seal` would.
fn write_package(repo: &Path, name: &str, body: &[u8]) {
    let dir = repo.join("packages").join(name).join("0.1.0");
    fs::create_dir_all(dir.join("guest")).unwrap();
    let rel = format!("guest/{name}.dlua");
    fs::write(dir.join(&rel), body).unwrap();
    let mut files = BTreeMap::new();
    files.insert(
        rel.clone(),
        format!("sha256:{}", hex::encode(sha2::Sha256::digest(body))),
    );
    let manifest = serde_json::json!({
        "name": name,
        "version": "0.1.0",
        "guest": { "main": name, "modules": { name: rel } },
        "files": files
    });
    fs::write(
        dir.join("manifest.json"),
        serde_json::to_vec_pretty(&manifest).unwrap(),
    )
    .unwrap();
}

/// A root with the repo as its only source.
fn root_with_source(home: &Path, dir: &Path, repo: &Path) {
    run(common::dollup()
        .env("HOME", home)
        .arg("--root")
        .arg(dir)
        .args(["init", dir.file_name().unwrap().to_str().unwrap()]));
    let path = dir.join(".drt_root/project.json");
    let mut project: serde_json::Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    project["sources"] = serde_json::json!([format!("file://{}", repo.display())]);
    fs::write(&path, serde_json::to_vec_pretty(&project).unwrap()).unwrap();
}

fn blobs(home: &Path) -> usize {
    let store = home.join(".dollup/cache/store");
    if !store.is_dir() {
        return 0;
    }
    let mut n = 0;
    let mut stack = vec![store];
    while let Some(d) = stack.pop() {
        for entry in fs::read_dir(&d).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                stack.push(path);
            } else {
                n += 1;
            }
        }
    }
    n
}

#[test]
fn the_cache_is_shared_and_gc_keeps_what_any_recorded_root_references() {
    let tmp = tempfile::tempdir().unwrap();
    let home = tmp.path().join("home");
    let repo = tmp.path().join("repo");
    write_package(&repo, "can", b"return { open = function() end }");
    write_package(&repo, "hello", b"print('hello')");
    run(common::dollup().args(["repo", "index"]).arg(&repo));

    let a = tmp.path().join("a");
    let b = tmp.path().join("b");
    root_with_source(&home, &a, &repo);
    root_with_source(&home, &b, &repo);
    run(common::dollup()
        .env("HOME", &home)
        .arg("--root")
        .arg(&a)
        .args(["pull", "can"]));
    run(common::dollup()
        .env("HOME", &home)
        .arg("--root")
        .arg(&b)
        .args(["pull", "hello"]));

    // Both roots' blobs are in the one cache, and nothing is beside the roots.
    let before = blobs(&home);
    assert!(before >= 4, "manifests and modules of both: {before}");
    assert!(!a.join(".dollup").exists(), "no per-root store");
    assert!(a.join(".drt_root/init/can/guest/can.dlua").exists());

    // A sweep from a keeps b's blobs, because b is a recorded root.
    let out = run(common::dollup()
        .env("HOME", &home)
        .arg("--root")
        .arg(&a)
        .arg("gc"));
    assert!(out.contains("swept 0 blob(s)"), "{out}");
    assert_eq!(blobs(&home), before);
    run(common::dollup()
        .env("HOME", &home)
        .arg("--root")
        .arg(&b)
        .arg("verify"));

    // b is gone: the sweep says so, and what only b referenced goes.
    fs::remove_dir_all(&b).unwrap();
    let out = run(common::dollup()
        .env("HOME", &home)
        .arg("--root")
        .arg(&a)
        .arg("gc"));
    assert!(out.contains("note: skipping"), "{out}");
    assert!(
        out.contains(&b.canonicalize().unwrap_or(b.clone()).display().to_string())
            || out.contains("/b"),
        "{out}"
    );
    assert!(
        out.contains("swept 2 blob(s)"),
        "hello's manifest and module: {out}"
    );
    assert!(blobs(&home) < before);
    run(common::dollup()
        .env("HOME", &home)
        .arg("--root")
        .arg(&a)
        .arg("verify"));
}

#[test]
fn a_root_with_no_home_for_a_cache_is_refused_by_name() {
    let tmp = tempfile::tempdir().unwrap();
    let home = tmp.path().join("home");
    let repo = tmp.path().join("repo");
    write_package(&repo, "can", b"return {}");
    run(common::dollup().args(["repo", "index"]).arg(&repo));
    let a = tmp.path().join("a");
    root_with_source(&home, &a, &repo);
    let msg = fail(
        common::dollup()
            .env_remove("HOME")
            .arg("--root")
            .arg(&a)
            .args(["pull", "can"]),
    );
    assert!(msg.contains("HOME is not set"), "{msg}");
}

#[test]
fn new_makes_the_directory_and_the_root_in_it() {
    let tmp = tempfile::tempdir().unwrap();
    let out = run(common::dollup()
        .arg("--root")
        .arg(tmp.path())
        .args(["new", "my_app"]));
    assert!(out.contains("wrote .drt_root/project.json"), "{out}");
    assert!(out.contains("cd my_app"), "{out}");
    let project: serde_json::Value = serde_json::from_slice(
        &fs::read(tmp.path().join("my_app/.drt_root/project.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(project["project_name"], "my_app");
    // Never a ref: a starting point is a package you pull.
    let msg = fail(
        common::dollup()
            .arg("--root")
            .arg(tmp.path())
            .args(["new", "starter@^0.1"]),
    );
    assert!(msg.contains("never a ref"), "{msg}");
    assert!(msg.contains("pull starter@^0.1"), "{msg}");
}
