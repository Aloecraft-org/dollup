//! The whole life, offline: publish a repo, sign it, scaffold a deployment,
//! add through two schemes, verify, tamper, sweep. This is acceptance demo 1
//! minus the runtime.

use std::collections::BTreeMap;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Command;

fn dollup() -> Command {
    Command::new(env!("CARGO_BIN_EXE_dollup"))
}

fn run(cmd: &mut Command) -> String {
    let out = cmd.output().unwrap();
    assert!(
        out.status.success(),
        "dollup failed:\n{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).into_owned()
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

/// Write a package tree and its manifest, hashing as we go.
fn write_package(repo: &Path, manifest: serde_json::Value, files: &[(&str, &[u8])]) {
    let name = manifest["name"].as_str().unwrap();
    let version = manifest["version"].as_str().unwrap();
    let dir = repo.join("packages").join(name).join(version);
    let mut hashes = BTreeMap::new();
    for (rel, bytes) in files {
        let path = dir.join(rel);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, bytes).unwrap();
        hashes.insert(
            rel.to_string(),
            serde_json::Value::String(format!(
                "sha256:{}",
                hex::encode(sha2::Sha256::digest(bytes))
            )),
        );
    }
    let mut manifest = manifest;
    manifest["files"] = serde_json::Value::Object(hashes.into_iter().collect());
    fs::write(
        dir.join("manifest.json"),
        serde_json::to_vec_pretty(&manifest).unwrap(),
    )
    .unwrap();
}

use sha2::Digest;

/// A repo with a pure-guest package depending on a three-faced one.
fn build_repo(repo: &Path) {
    write_package(
        repo,
        serde_json::json!({
            "name": "can",
            "version": "0.1.0",
            "capability": {
                "host:can": { "scope_type": "interface", "calls": ["can/send", "can/recv"], "shape": 1 }
            },
            "guest": { "main": "can", "modules": { "can": "guest/can.dlua" } },
            "host": {
                "provides": ["host:can"],
                "targets": {
                    "wasm32-wasip2": { "abi": "component", "files": { "module": "host/can.wasm" } },
                    "x86_64-unknown-linux-gnu": { "abi": "native", "files": { "module": "host/libcan.so" } }
                }
            },
            "assets": { "logo": "assets/logo.png" }
        }),
        &[
            ("guest/can.dlua", b"return { send = function() end }"),
            ("host/can.wasm", b"\0asm fake component"),
            ("host/libcan.so", b"\x7fELF fake native"),
            ("assets/logo.png", b"\x89PNG fake"),
        ],
    );
    write_package(
        repo,
        serde_json::json!({
            "name": "telemetry",
            "version": "1.2.0",
            "guest": { "main": "telemetry", "modules": { "telemetry": "guest/telemetry.dlua" } },
            "requires": { "packages": { "can": "^0.1" }, "capabilities": ["host:time"] }
        }),
        &[("guest/telemetry.dlua", b"local can = require_is_a_lie")],
    );
}

#[test]
fn publish_sign_add_verify_tamper_gc() {
    let tmp = tempfile::tempdir().unwrap();
    let repo = tmp.path().join("repo");
    build_repo(&repo);

    // Publish: index, keygen, sign.
    run(dollup().args(["repo", "index"]).arg(&repo));
    let keys = run(dollup().args(["repo", "keygen"]));
    let (private, public) = keys.trim().split_once('\n').unwrap();
    let key_file = tmp.path().join("key");
    fs::write(&key_file, private).unwrap();
    run(dollup()
        .args(["repo", "sign"])
        .arg(&repo)
        .arg("--key-file")
        .arg(&key_file));

    // Deployment: scaffold, then pin the source WITH its key.
    let dep = tmp.path().join("dep");
    run(dollup().arg("--deployment").arg(&dep).arg("init"));
    let cfg_path = dep.join("dollup.json");
    let mut cfg: serde_json::Value = serde_json::from_slice(&fs::read(&cfg_path).unwrap()).unwrap();
    cfg["sources"] = serde_json::json!([
        { "url": format!("file://{}", repo.display()), "keys": [public] }
    ]);
    fs::write(&cfg_path, serde_json::to_vec_pretty(&cfg).unwrap()).unwrap();

    // Add the dependent package: both arrive, signed, host face skipped.
    let out = run(dollup()
        .arg("--deployment")
        .arg(&dep)
        .args(["add", "telemetry@^1"]));
    assert!(out.contains("telemetry 1.2.0"), "{out}");
    assert!(out.contains("can 0.1.0"), "dependency resolved: {out}");
    assert!(out.contains("signed"), "{out}");
    assert!(out.contains("host face skipped"), "{out}");
    assert!(dep.join("code/can/guest/can.dlua").exists());
    assert!(dep.join("code/can/assets/logo.png").exists());
    assert!(
        !dep.join("code/can/host/can.wasm").exists(),
        "gated by default"
    );
    assert!(!dep.join("code/can/host/libcan.so").exists());

    let ls = run(dollup().arg("--deployment").arg(&dep).arg("ls"));
    assert!(ls.contains("can 0.1.0 (signed)"), "{ls}");
    run(dollup().arg("--deployment").arg(&dep).arg("verify"));

    // Tamper with the materialized code: verify names it.
    fs::write(dep.join("code/can/guest/can.dlua"), b"evil").unwrap();
    let msg = fail(dollup().arg("--deployment").arg(&dep).arg("verify"));
    assert!(
        msg.contains("can: guest/can.dlua does not match the lock"),
        "{msg}"
    );

    // Tamper with the repo index: the signature refuses it.
    let dep2 = tmp.path().join("dep2");
    run(dollup().arg("--deployment").arg(&dep2).arg("init"));
    fs::copy(&cfg_path, dep2.join("dollup.json")).unwrap();
    let index_path = repo.join("index.json");
    let mut index = fs::read(&index_path).unwrap();
    let mut f = fs::OpenOptions::new()
        .append(true)
        .open(&index_path)
        .unwrap();
    f.write_all(b"\n").unwrap();
    drop(f);
    let msg = fail(dollup().arg("--deployment").arg(&dep2).args(["add", "can"]));
    assert!(msg.contains("signature verification failed"), "{msg}");
    index.truncate(index.len());
    fs::write(&index_path, &index).unwrap();

    // gc keeps what the lock names.
    let out = run(dollup().arg("--deployment").arg(&dep).arg("gc"));
    assert!(out.contains("swept 0"), "everything is locked: {out}");
}

#[test]
fn host_gates_admit_by_flag_and_unsigned_network_is_refused() {
    let tmp = tempfile::tempdir().unwrap();
    let repo = tmp.path().join("repo");
    build_repo(&repo);
    run(dollup().args(["repo", "index"]).arg(&repo));

    let dep = tmp.path().join("dep");
    run(dollup().arg("--deployment").arg(&dep).arg("init"));
    let cfg_path = dep.join("dollup.json");
    let mut cfg: serde_json::Value = serde_json::from_slice(&fs::read(&cfg_path).unwrap()).unwrap();
    // Unsigned file:// source: fine even under require_signatures.
    cfg["sources"] = serde_json::json!([format!("file://{}", repo.display())]);
    fs::write(&cfg_path, serde_json::to_vec_pretty(&cfg).unwrap()).unwrap();

    let out = run(dollup()
        .arg("--deployment")
        .arg(&dep)
        .args(["add", "can", "--with-host"]));
    assert!(out.contains("unsigned"), "{out}");
    assert!(dep.join("code/can/host/can.wasm").exists(), "wasm admitted");
    assert!(
        !dep.join("code/can/host/libcan.so").exists(),
        "native still gated"
    );

    let dep2 = tmp.path().join("dep2");
    run(dollup().arg("--deployment").arg(&dep2).arg("init"));
    fs::copy(&cfg_path, dep2.join("dollup.json")).unwrap();
    let out =
        run(dollup()
            .arg("--deployment")
            .arg(&dep2)
            .args(["add", "can", "--with-host-native"]));
    assert!(out.contains("can 0.1.0"), "{out}");
    assert!(
        dep2.join("code/can/host/libcan.so").exists(),
        "native admitted by its own flag"
    );

    // An unsigned *network* source is refused under require_signatures —
    // named, before any package is considered.
    let dep3 = tmp.path().join("dep3");
    run(dollup().arg("--deployment").arg(&dep3).arg("init"));
    let mut cfg3: serde_json::Value =
        serde_json::from_slice(&fs::read(dep3.join("dollup.json")).unwrap()).unwrap();
    cfg3["sources"] = serde_json::json!(["https://example.invalid/repo"]);
    fs::write(
        dep3.join("dollup.json"),
        serde_json::to_vec_pretty(&cfg3).unwrap(),
    )
    .unwrap();
    let msg = fail(dollup().arg("--deployment").arg(&dep3).args(["add", "can"]));
    assert!(msg.contains("unsigned network source refused"), "{msg}");
}

#[test]
fn zipball_of_the_same_repo_yields_the_same_identities() {
    let tmp = tempfile::tempdir().unwrap();
    let repo = tmp.path().join("repo");
    build_repo(&repo);
    run(dollup().args(["repo", "index"]).arg(&repo));

    // Zip the tree the way a forge does: wrapped in one root directory.
    let zip_path = tmp.path().join("repo-main.zip");
    zip_dir(&repo, "repo-main", &zip_path);

    let dep = tmp.path().join("dep");
    run(dollup().arg("--deployment").arg(&dep).arg("init"));
    let mut cfg: serde_json::Value =
        serde_json::from_slice(&fs::read(dep.join("dollup.json")).unwrap()).unwrap();
    cfg["sources"] = serde_json::json!([format!("zip+file://{}", zip_path.display())]);
    fs::write(
        dep.join("dollup.json"),
        serde_json::to_vec_pretty(&cfg).unwrap(),
    )
    .unwrap();

    run(dollup().arg("--deployment").arg(&dep).args(["add", "can"]));

    // Same content through a different transport → the same package_id in
    // the lock. This is "identity is content" doing its job.
    let dep_dir = tmp.path().join("dep-dir");
    run(dollup().arg("--deployment").arg(&dep_dir).arg("init"));
    let mut cfg2: serde_json::Value =
        serde_json::from_slice(&fs::read(dep_dir.join("dollup.json")).unwrap()).unwrap();
    cfg2["sources"] = serde_json::json!([format!("file://{}", repo.display())]);
    fs::write(
        dep_dir.join("dollup.json"),
        serde_json::to_vec_pretty(&cfg2).unwrap(),
    )
    .unwrap();
    run(dollup()
        .arg("--deployment")
        .arg(&dep_dir)
        .args(["add", "can"]));

    let lock_a: serde_json::Value =
        serde_json::from_slice(&fs::read(dep.join("dollup.lock")).unwrap()).unwrap();
    let lock_b: serde_json::Value =
        serde_json::from_slice(&fs::read(dep_dir.join("dollup.lock")).unwrap()).unwrap();
    assert_eq!(
        lock_a["packages"]["can"]["package_id"],
        lock_b["packages"]["can"]["package_id"]
    );
    assert_eq!(
        lock_a["packages"]["can"]["code_set"],
        lock_b["packages"]["can"]["code_set"]
    );
}

fn zip_dir(dir: &Path, root: &str, out: &Path) {
    let file = fs::File::create(out).unwrap();
    let mut w = zip::ZipWriter::new(file);
    let opts = zip::write::SimpleFileOptions::default();
    let mut stack = vec![dir.to_path_buf()];
    while let Some(d) = stack.pop() {
        for entry in fs::read_dir(&d).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                stack.push(path);
            } else {
                let rel: PathBuf = path.strip_prefix(dir).unwrap().to_path_buf();
                w.start_file(format!("{root}/{}", rel.display()), opts)
                    .unwrap();
                w.write_all(&fs::read(&path).unwrap()).unwrap();
            }
        }
    }
    w.finish().unwrap();
}

#[test]
fn one_deployment_one_meaning_per_capability_name() {
    let tmp = tempfile::tempdir().unwrap();
    let repo = tmp.path().join("repo");
    build_repo(&repo);
    // A second package redefining host:can with a different shape.
    write_package(
        &repo,
        serde_json::json!({
            "name": "fastcan",
            "version": "2.0.0",
            "capability": {
                "host:can": { "scope_type": "interface", "calls": ["can/send"], "shape": 2 }
            },
            "guest": { "main": "fastcan", "modules": { "fastcan": "guest/fastcan.dlua" } }
        }),
        &[("guest/fastcan.dlua", b"return {}")],
    );
    // And a vendored copy of the ORIGINAL contract, byte-identical decl.
    write_package(
        &repo,
        serde_json::json!({
            "name": "can-vendored",
            "version": "0.1.0",
            "capability": {
                "host:can": { "scope_type": "interface", "calls": ["can/send", "can/recv"], "shape": 1 }
            },
            "guest": { "main": "v", "modules": { "v": "guest/v.dlua" } }
        }),
        &[("guest/v.dlua", b"return {}")],
    );
    run(dollup().args(["repo", "index"]).arg(&repo));

    let dep = tmp.path().join("dep");
    run(dollup().arg("--deployment").arg(&dep).arg("init"));
    let cfg_path = dep.join("dollup.json");
    let mut cfg: serde_json::Value = serde_json::from_slice(&fs::read(&cfg_path).unwrap()).unwrap();
    cfg["sources"] = serde_json::json!([format!("file://{}", repo.display())]);
    fs::write(&cfg_path, serde_json::to_vec_pretty(&cfg).unwrap()).unwrap();

    // can binds host:can; the identical vendored declaration passes; the
    // different one is refused naming both definers.
    run(dollup().arg("--deployment").arg(&dep).args(["add", "can"]));
    run(dollup()
        .arg("--deployment")
        .arg(&dep)
        .args(["add", "can-vendored"]));
    let msg = fail(
        dollup()
            .arg("--deployment")
            .arg(&dep)
            .args(["add", "fastcan"]),
    );
    assert!(
        msg.contains("'fastcan' defines capability 'host:can'"),
        "{msg}"
    );
    assert!(msg.contains("'can' already bound"), "{msg}");

    let lock: serde_json::Value =
        serde_json::from_slice(&fs::read(dep.join("dollup.lock")).unwrap()).unwrap();
    assert_eq!(lock["contracts"]["host:can"]["defined_by"], "can");
    // info shows the contract before anything is fetched or admitted.
    let out = run(dollup()
        .arg("--deployment")
        .arg(&dep)
        .args(["info", "fastcan"]));
    assert!(out.contains("defines host:can"), "{out}");
    assert!(out.contains("shape 2"), "{out}");
}
