//! Acceptance demo 2 minus the runtime: hibernate on machine A (a blob),
//! push to a file remote, pull on machine B, and B ends up holding the
//! blob AND the identical code-set — no hand-copied files. Plus the
//! publicity gate, checked before writability.

use std::fs;
use std::path::Path;
use std::process::Command;

use sha2::Digest;

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

/// One guest package, published and indexed.
fn build_repo(repo: &Path) {
    let dir = repo.join("packages/agent/0.3.0");
    fs::create_dir_all(dir.join("guest")).unwrap();
    let module = b"return { tick = function() end }";
    fs::write(dir.join("guest/agent.dlua"), module).unwrap();
    let manifest = serde_json::json!({
        "name": "agent",
        "version": "0.3.0",
        "guest": { "main": "agent", "modules": { "agent": "guest/agent.dlua" } },
        "files": { "guest/agent.dlua":
            format!("sha256:{}", hex::encode(sha2::Sha256::digest(module))) }
    });
    fs::write(
        dir.join("manifest.json"),
        serde_json::to_vec_pretty(&manifest).unwrap(),
    )
    .unwrap();
}

fn init_deployment(dep: &Path, repo: &Path) {
    run(dollup().arg("--deployment").arg(dep).arg("init"));
    let cfg_path = dep.join("dollup.json");
    let mut cfg: serde_json::Value = serde_json::from_slice(&fs::read(&cfg_path).unwrap()).unwrap();
    cfg["sources"] = serde_json::json!([format!("file://{}", repo.display())]);
    fs::write(&cfg_path, serde_json::to_vec_pretty(&cfg).unwrap()).unwrap();
}

#[test]
fn migrate_a_sleeping_agent_between_machines() {
    let tmp = tempfile::tempdir().unwrap();
    let repo = tmp.path().join("repo");
    build_repo(&repo);
    run(dollup().args(["repo", "index"]).arg(&repo));
    let remote = tmp.path().join("remote");
    fs::create_dir_all(&remote).unwrap();

    // Machine A: deployment with the package, and a hibernated blob.
    let a = tmp.path().join("machine-a");
    init_deployment(&a, &repo);
    run(dollup().arg("--deployment").arg(&a).args(["add", "agent"]));
    let blob_path = a.join("night-clerk.dvsnap");
    fs::write(&blob_path, b"opaque heap bytes: dollup never parses these").unwrap();

    let out = run(dollup()
        .arg("--deployment")
        .arg(&a)
        .arg("push")
        .arg(format!("file://{}", remote.display()))
        .arg(&blob_path)
        .args([
            "--package",
            "agent",
            "--identity",
            "host-a",
            "--capability",
            "host:time",
        ]));
    assert!(out.contains("night-clerk →"), "{out}");
    assert!(remote.join("snapshots/night-clerk/manifest.json").exists());
    assert!(remote.join("snapshots/night-clerk/state").exists());

    // Machine B: fresh deployment, same sources, nothing locked yet. Pull
    // brings the blob AND resolves the pinned code-set from the sources by
    // identity.
    let b = tmp.path().join("machine-b");
    init_deployment(&b, &repo);
    let out = run(dollup()
        .arg("--deployment")
        .arg(&b)
        .arg("pull")
        .arg(format!("file://{}", remote.display()))
        .arg("night-clerk"));
    assert!(
        out.contains("agent 0.3.0"),
        "code-set resolved by identity: {out}"
    );
    assert!(out.contains("restore is DRT's verb"), "{out}");

    let pulled = b.join("snapshots/night-clerk.dvsnap");
    assert_eq!(fs::read(&pulled).unwrap(), fs::read(&blob_path).unwrap());
    assert!(
        b.join("code/agent/guest/agent.dlua").exists(),
        "same code-set on B"
    );

    // The lock rows on A and B agree on state and code_set.
    let lock_a: serde_json::Value =
        serde_json::from_slice(&fs::read(a.join("dollup.lock")).unwrap()).unwrap();
    let lock_b: serde_json::Value =
        serde_json::from_slice(&fs::read(b.join("dollup.lock")).unwrap()).unwrap();
    assert_eq!(
        lock_a["snapshots"]["night-clerk"]["state"],
        lock_b["snapshots"]["night-clerk"]["state"]
    );
    assert_eq!(
        lock_b["snapshots"]["night-clerk"]["code_set"],
        lock_b["packages"]["agent"]["code_set"]
    );

    // verify and gc cover the snapshot; tampering the blob is named.
    run(dollup().arg("--deployment").arg(&b).arg("verify"));
    let out = run(dollup().arg("--deployment").arg(&b).arg("gc"));
    assert!(out.contains("swept 0"), "{out}");
    fs::write(&pulled, b"corrupted").unwrap();
    let msg = fail(dollup().arg("--deployment").arg(&b).arg("verify"));
    assert!(
        msg.contains("snapshot night-clerk: does not match the lock"),
        "{msg}"
    );
}

#[test]
fn the_publicity_gate_comes_before_writability() {
    let tmp = tempfile::tempdir().unwrap();
    let dep = tmp.path().join("dep");
    run(dollup().arg("--deployment").arg(&dep).arg("init"));
    // `init` scaffolds the public standard source now. This test is about a
    // code-set NOTHING carries, so it must say so: with the public source in
    // place, `pull` would (correctly) go looking there, and the failure it
    // reports would be the network's rather than the hash's.
    let cfg_path = dep.join("dollup.json");
    let mut cfg: serde_json::Value = serde_json::from_slice(&fs::read(&cfg_path).unwrap()).unwrap();
    cfg["sources"] = serde_json::json!([]);
    fs::write(&cfg_path, serde_json::to_vec_pretty(&cfg).unwrap()).unwrap();
    let blob = tmp.path().join("s.dvsnap");
    fs::write(&blob, b"state").unwrap();

    // Non-file remote without the flag: the gate speaks first, naming the
    // flag and the reason — not DNS, not writability.
    let msg = fail(
        dollup()
            .arg("--deployment")
            .arg(&dep)
            .arg("push")
            .arg("https://example.invalid/repo")
            .arg(&blob)
            .args(["--code-set", "sha256:00"]),
    );
    assert!(msg.contains("--export-state"), "{msg}");
    assert!(msg.contains("entire heap"), "{msg}");

    // With the flag acknowledged, the true v1 limit speaks.
    let msg = fail(
        dollup()
            .arg("--deployment")
            .arg(&dep)
            .arg("push")
            .arg("https://example.invalid/repo")
            .arg(&blob)
            .args(["--code-set", "sha256:00", "--export-state"]),
    );
    assert!(
        msg.contains("only file:// remotes are writable in v1"),
        "{msg}"
    );

    // Pulling a snapshot whose code-set nothing carries fails by hash.
    let remote = tmp.path().join("remote");
    fs::create_dir_all(remote.join("snapshots/ghost")).unwrap();
    fs::write(remote.join("snapshots/ghost/state"), b"state").unwrap();
    let manifest = serde_json::json!({
        "dollup_snapshot": 1,
        "state": format!("sha256:{}", hex::encode(sha2::Sha256::digest(b"state"))),
        "code_set": "sha256:feedbeef"
    });
    fs::write(
        remote.join("snapshots/ghost/manifest.json"),
        serde_json::to_vec_pretty(&manifest).unwrap(),
    )
    .unwrap();
    let msg = fail(
        dollup()
            .arg("--deployment")
            .arg(&dep)
            .arg("pull")
            .arg(format!("file://{}", remote.display()))
            .arg("ghost"),
    );
    assert!(
        msg.contains("sha256:feedbeef"),
        "fails naming the hash: {msg}"
    );
}

#[test]
fn keygen_out_writes_files_and_prints_no_private_key() {
    let tmp = tempfile::tempdir().unwrap();
    let prefix = tmp.path().join("std-repo.key");
    let out = dollup()
        .args(["repo", "keygen", "--out"])
        .arg(&prefix)
        .output()
        .unwrap();
    assert!(out.status.success());
    let stdout = String::from_utf8_lossy(&out.stdout);
    let private = fs::read_to_string(&prefix).unwrap();
    assert!(private.starts_with("ed25519:"));
    assert!(
        !stdout.contains(private.trim()),
        "the private key never touches the terminal"
    );
    let public = fs::read_to_string(prefix.with_extension("pub")).unwrap();
    assert_eq!(stdout.trim(), public.trim(), "the public key is echoed");

    use std::os::unix::fs::PermissionsExt;
    let mode = fs::metadata(&prefix).unwrap().permissions().mode() & 0o777;
    assert_eq!(mode, 0o600, "private key file is 0600");

    // A second run refuses to overwrite the private key.
    let out = dollup()
        .args(["repo", "keygen", "--out"])
        .arg(&prefix)
        .output()
        .unwrap();
    assert!(!out.status.success(), "never clobbers an existing key");
}
