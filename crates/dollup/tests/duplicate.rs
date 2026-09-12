//! `dollup duplicate`: the copy is a new root — fresh id, recorded origin —
//! carrying everything the source has except what is the runtime's and
//! what never travels, with the consent the source effectively has.

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

fn dollup(home: &Path) -> Command {
    let mut cmd = common::dollup();
    cmd.env("HOME", home);
    cmd
}

fn project(dir: &Path) -> serde_json::Value {
    serde_json::from_slice(&fs::read(dir.join(".drt_root/project.json")).unwrap()).unwrap()
}

/// One guest package in a repo tree.
fn write_repo(repo: &Path) {
    let dir = repo.join("packages/enc/0.1.0");
    fs::create_dir_all(dir.join("src")).unwrap();
    let body = b"return { encode = function() end }";
    fs::write(dir.join("src/enc.dlua"), body).unwrap();
    let mut files = BTreeMap::new();
    files.insert(
        "src/enc.dlua".to_string(),
        format!("sha256:{}", hex::encode(sha2::Sha256::digest(body))),
    );
    let manifest = serde_json::json!({
        "name": "enc", "version": "0.1.0", "license": "Apache-2.0",
        "guest": { "modules": { "util.enc": "src/enc.dlua" } },
        "files": files
    });
    fs::write(
        dir.join("manifest.json"),
        serde_json::to_vec_pretty(&manifest).unwrap(),
    )
    .unwrap();
}

/// A root with something in every directory a duplicate must decide about.
fn source_root(home: &Path, dir: &Path, repo: &Path) {
    run(dollup(home).arg("--root").arg(dir).args(["init", "demo"]));
    let path = dir.join(".drt_root/project.json");
    let mut p: serde_json::Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    p["sources"] = serde_json::json!([format!("file://{}", repo.display())]);
    fs::write(&path, serde_json::to_vec_pretty(&p).unwrap()).unwrap();
    run(dollup(home).arg("--root").arg(dir).args(["pull", "enc"]));
    fs::write(dir.join("dlua/app.dlua"), b"print('mine')\n").unwrap();
    fs::write(dir.join("NOTES.md"), b"beside .drt_root\n").unwrap();
    // What the runtime owns, and would be wrong on a copy.
    let meta = dir.join(".drt_root");
    fs::create_dir_all(meta.join("state/gsr/pending")).unwrap();
    fs::write(meta.join("state/envelope.json"), b"{}").unwrap();
    fs::create_dir_all(meta.join("live/demo")).unwrap();
    fs::write(meta.join("live/demo/app.dlua"), b"running").unwrap();
    fs::write(meta.join("log/root.log"), b"old logs").unwrap();
    fs::write(meta.join("drt"), b"#!/bin/sh\n").unwrap();
}

#[test]
fn a_duplicate_is_a_new_root_with_everything_but_the_runtimes_and_the_consent() {
    let tmp = tempfile::tempdir().unwrap();
    let home = tmp.path().join("home");
    let repo = tmp.path().join("repo");
    write_repo(&repo);
    run(common::dollup().args(["repo", "index"]).arg(&repo));
    let src = tmp.path().join("src");
    source_root(&home, &src, &repo);
    let dst = tmp.path().join("copy");

    let out = run(dollup(&home)
        .arg("--root")
        .arg(&src)
        .arg("duplicate")
        .arg(&dst));
    assert!(out.contains("duplicated"), "{out}");
    assert!(out.contains("left behind"), "{out}");
    assert!(
        out.contains(
            "consent: listed over the current ceiling, as the source's acceptance covers it"
        ),
        "{out}"
    );

    // A new root: fresh id, recorded origin, everything else verbatim.
    let (a, b) = (project(&src), project(&dst));
    assert_ne!(a["root_id"], b["root_id"]);
    assert_eq!(b["duplicated_from"], a["root_id"]);
    assert!(a.get("duplicated_from").is_none());
    for field in [
        "project_name",
        "project_version",
        "caps",
        "sources",
        "default_profile",
        "profiles",
    ] {
        assert_eq!(a[field], b[field], "{field}");
    }
    // What came along.
    assert_eq!(
        fs::read(dst.join("dlua/app.dlua")).unwrap(),
        b"print('mine')\n"
    );
    assert_eq!(
        fs::read(dst.join("NOTES.md")).unwrap(),
        b"beside .drt_root\n"
    );
    assert!(dst.join(".drt_root/init/util/enc.dlua").is_file());
    assert!(dst.join(".drt_root/profile/debug.config.json").is_file());
    assert!(dst.join(".drt_root/dollup.lock").is_file());
    assert_eq!(fs::read(dst.join(".drt_root/drt")).unwrap(), b"#!/bin/sh\n");
    // What stayed behind: present as empty directories, nothing in them.
    for dir in ["state", "live", "log"] {
        let d = dst.join(".drt_root").join(dir);
        assert!(d.is_dir(), "{dir}/ exists");
        assert!(
            fs::read_dir(&d).unwrap().next().is_none(),
            "{dir}/ is empty"
        );
    }
    // The copy's consent is its own, over the same ceiling, and the copy
    // would run — and verify against the shared cache.
    let consent: serde_json::Value =
        serde_json::from_slice(&fs::read(dst.join(".drt_root/consent.json")).unwrap()).unwrap();
    assert_eq!(consent["root_id"], b["root_id"]);
    assert_eq!(consent["accepted"][0]["mode"], "listed");
    let out = run(dollup(&home).arg("--root").arg(&dst).arg("audit"));
    assert!(
        out.contains("consent: listed, matches the ceiling"),
        "{out}"
    );
    assert!(out.contains("envelope: none"), "{out}");
    assert!(!out.contains("another root on this box claims"), "{out}");
    assert!(out.contains("start would run"), "{out}");
    run(dollup(&home).arg("--root").arg(&dst).arg("verify"));
    // And both are on the list, as two roots.
    let out = run(dollup(&home).arg("roots"));
    assert!(out.contains("demo"), "{out}");
    assert!(!out.contains("duplicate root_id"), "{out}");
}

#[test]
fn the_copy_gets_the_consent_the_source_effectively_has_and_no_more() {
    let tmp = tempfile::tempdir().unwrap();
    let home = tmp.path().join("home");
    let src = tmp.path().join("src");
    run(dollup(&home).arg("--root").arg(&src).args(["init", "demo"]));

    // No acceptance on the source: none on the copy, and its start asks.
    fs::remove_file(src.join(".drt_root/consent.json")).unwrap();
    let dst = tmp.path().join("unaccepted");
    let out = run(dollup(&home)
        .arg("--root")
        .arg(&src)
        .arg("duplicate")
        .arg(&dst));
    assert!(
        out.contains("consent: none — the source's does not cover its ceiling"),
        "{out}"
    );
    assert!(!dst.join(".drt_root/consent.json").exists());
    let (ok, _) = {
        let out = dollup(&home)
            .arg("--root")
            .arg(&dst)
            .arg("audit")
            .output()
            .unwrap();
        (out.status.success(), out)
    };
    assert!(!ok, "the copy's start would stop to ask");

    // Signers are who may approve, not an approval: they come along even
    // when no acceptance does, and the copy's start still asks.
    let signer = serde_json::json!({
        "key_id": "portal-1",
        "alg": "ed25519",
        "public_key": "bnoc3Smwt4/ROvTFWY/v9O8qlxZuPKby5Pv8zYBQW/E=",
        "realms": ["operator.rest"]
    });
    fs::write(
        src.join(".drt_root/consent.json"),
        serde_json::to_vec_pretty(&serde_json::json!({
            "root_id": project(&src)["root_id"],
            "signers": [signer]
        }))
        .unwrap(),
    )
    .unwrap();
    let dst_signed = tmp.path().join("signers-only");
    let out = run(dollup(&home)
        .arg("--root")
        .arg(&src)
        .arg("duplicate")
        .arg(&dst_signed));
    assert!(out.contains("consent: none"), "{out}");
    assert!(out.contains("signers: 1 kept"), "{out}");
    let consent: serde_json::Value =
        serde_json::from_slice(&fs::read(dst_signed.join(".drt_root/consent.json")).unwrap())
            .unwrap();
    assert_eq!(consent["root_id"], project(&dst_signed)["root_id"]);
    assert_eq!(consent["signers"][0]["key_id"], "portal-1");
    assert!(consent.get("accepted").is_none(), "{consent}");
    assert!(!dollup(&home)
        .arg("--root")
        .arg(&dst_signed)
        .arg("audit")
        .status()
        .unwrap()
        .success());

    // Blanket on the source: listed on the copy, and said.
    run(dollup(&home)
        .arg("--root")
        .arg(&src)
        .args(["consent", "--all"]));
    let dst2 = tmp.path().join("from-blanket");
    let out = run(dollup(&home)
        .arg("--root")
        .arg(&src)
        .arg("duplicate")
        .arg(&dst2));
    assert!(
        out.contains("the source's is blanket; the copy's is listed"),
        "{out}"
    );
    let consent: serde_json::Value =
        serde_json::from_slice(&fs::read(dst2.join(".drt_root/consent.json")).unwrap()).unwrap();
    assert_eq!(consent["accepted"][0]["mode"], "listed");
}

#[test]
fn a_duplicate_goes_to_a_new_path_and_never_inside_itself() {
    let tmp = tempfile::tempdir().unwrap();
    let home = tmp.path().join("home");
    let src = tmp.path().join("src");
    run(dollup(&home).arg("--root").arg(&src).args(["init", "demo"]));

    let msg = fail(
        dollup(&home)
            .arg("--root")
            .arg(&src)
            .arg("duplicate")
            .arg(&src),
    );
    assert!(msg.contains("is already a root"), "{msg}");
    let busy = tmp.path().join("busy");
    fs::create_dir_all(&busy).unwrap();
    fs::write(busy.join("file"), b"x").unwrap();
    let msg = fail(
        dollup(&home)
            .arg("--root")
            .arg(&src)
            .arg("duplicate")
            .arg(&busy),
    );
    assert!(msg.contains("is not empty"), "{msg}");
    let inside = src.join("dlua/copy");
    let msg = fail(
        dollup(&home)
            .arg("--root")
            .arg(&src)
            .arg("duplicate")
            .arg(&inside),
    );
    assert!(msg.contains("inside the root being duplicated"), "{msg}");
    assert!(!inside.exists());
    // However much of the path is still to be made ...
    let deeper = src.join("not/yet/there");
    let msg = fail(
        dollup(&home)
            .arg("--root")
            .arg(&src)
            .arg("duplicate")
            .arg(&deeper),
    );
    assert!(msg.contains("inside the root being duplicated"), "{msg}");
    assert!(!src.join("not").exists());
    // ... and however it is spelled.
    let msg = fail(
        dollup(&home)
            .current_dir(&src)
            .args(["--root", ".", "duplicate", "copy"]),
    );
    assert!(msg.contains("inside the root being duplicated"), "{msg}");
    assert!(!src.join("copy").exists());
    // Not a root at all.
    let empty = tmp.path().join("empty");
    fs::create_dir_all(&empty).unwrap();
    let msg = fail(
        dollup(&home)
            .arg("--root")
            .arg(&empty)
            .arg("duplicate")
            .arg(tmp.path().join("x")),
    );
    assert!(msg.contains("no root here"), "{msg}");
}
