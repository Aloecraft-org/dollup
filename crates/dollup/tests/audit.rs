//! `dollup audit` over roots built by hand — `dollup init` for this layout
//! is not built yet, so the tests write what it will write. What is
//! asserted is the shared resolution's answers as audit renders them, and
//! the one check that is audit's own: the binary against the pinned
//! release's sums, with the cache under a HOME the test controls, so nothing
//! here reaches the network.

use std::fs;
use std::path::Path;
use std::process::Command;

use sha2::Digest;

const ROOT_ID: &str = "0192f0c1-8000-7000-8000-00000000abcd";

fn dollup() -> Command {
    Command::new(env!("CARGO_BIN_EXE_dollup"))
}

/// Run audit on `dir`: (start would run, everything it printed).
fn audit(dir: &Path, args: &[&str]) -> (bool, String) {
    audit_with_home(dir, args, None)
}

fn audit_with_home(dir: &Path, args: &[&str], home: Option<&Path>) -> (bool, String) {
    let mut cmd = dollup();
    cmd.arg("--root").arg(dir).arg("audit").args(args);
    if let Some(home) = home {
        cmd.env("HOME", home);
    }
    let out = cmd.output().unwrap();
    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    (out.status.success(), text)
}

fn project_json(caps: &[&str], pin: Option<&str>) -> Vec<u8> {
    let caps: Vec<serde_json::Value> = caps
        .iter()
        .map(|c| serde_json::json!({ "capability": c }))
        .collect();
    let mut project = serde_json::json!({
        "project_name": "demo",
        "project_version": "0.0.0",
        "root_id": ROOT_ID,
        "caps": caps,
        "default_profile": "debug",
        "profiles": ["debug.config.json"]
    });
    if let Some(pin) = pin {
        project["drt"] = pin.into();
    }
    serde_json::to_vec_pretty(&project).unwrap()
}

/// A root as `dollup init` will write it: descriptor, one profile, the
/// entry it names.
fn write_root(dir: &Path, caps: &[&str], pin: Option<&str>) {
    let rd = dir.join(".drt_root");
    fs::create_dir_all(rd.join("profile")).unwrap();
    fs::create_dir_all(dir.join("dlua")).unwrap();
    fs::write(rd.join("project.json"), project_json(caps, pin)).unwrap();
    fs::write(
        rd.join("profile/debug.config.json"),
        br#"{ "dlua_dir": "dlua/", "entry": "app.dlua", "caps": [{ "capability": "host:time" }], "args": { "verbose": false } }"#,
    )
    .unwrap();
    fs::write(dir.join("dlua/app.dlua"), "print('hi')\n").unwrap();
}

/// consent.json in listed mode over the ceiling project.json declares right
/// now, hashed by the shared function — what `dollup init` will write.
fn write_consent(dir: &Path, root_id: &str) {
    let text = fs::read_to_string(dir.join(".drt_root/project.json")).unwrap();
    let project: drt_config::project::ProjectJson = serde_json::from_str(&text).unwrap();
    let hash = drt_config::project::ceiling_hash(&project).unwrap();
    let consent = serde_json::json!({
        "root_id": root_id,
        "accepted": [{
            "mode": "listed",
            "realm": "operator",
            "ceiling_hash": hash.as_str(),
            "ceiling": project.caps,
            "accepted_at": "2026-09-11T20:14:00Z"
        }],
        "signers": []
    });
    fs::write(
        dir.join(".drt_root/consent.json"),
        serde_json::to_vec_pretty(&consent).unwrap(),
    )
    .unwrap();
}

#[test]
fn a_consented_root_reports_what_start_would_do_and_that_it_would_run() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path();
    write_root(dir, &["host:fs/*", "host:time"], None);
    write_consent(dir, ROOT_ID);

    let (ok, out) = audit(dir, &[]);
    assert!(ok, "{out}");
    // Every field carries the rule that chose it — the parenthetical is a
    // value from the shared resolution, not text audit made up.
    assert!(out.contains("profile: debug (default_profile)"), "{out}");
    assert!(out.contains("ceiling: 2 caps (project.json caps)"), "{out}");
    assert!(
        out.contains("consent: listed, matches the ceiling"),
        "{out}"
    );
    assert!(
        out.contains("entry: app.dlua (from the profile), present"),
        "{out}"
    );
    assert!(out.contains("source: dlua/ (from the profile)"), "{out}");
    assert!(out.contains("args: { verbose: false }"), "{out}");
    assert!(out.contains("note: no drt version is pinned"), "{out}");
    assert!(out.contains("drt: no binary at"), "{out}");
    assert!(out.contains("start would run. nothing started."), "{out}");

    // Both spellings of a profile name are the same request.
    let (ok, out) = audit(dir, &["debug.config.json"]);
    assert!(ok, "{out}");
    assert!(
        out.contains("profile: debug (named on the command line)"),
        "{out}"
    );
    let (ok, out) = audit(dir, &["nope"]);
    assert!(!ok);
    assert!(
        out.contains("blocks: 'nope' is not a profile this root declares"),
        "{out}"
    );
}

#[test]
fn consent_states_are_reported_as_start_would_act_on_them() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path();
    write_root(dir, &["host:fs/*", "host:time"], None);

    // No consent yet: start would print the ceiling and ask, and a root
    // under systemd cannot answer, so that is not "would run".
    let (ok, out) = audit(dir, &[]);
    assert!(!ok);
    assert!(out.contains("consent: none accepted yet"), "{out}");
    assert!(out.contains("start would stop to ask"), "{out}");

    // Accepted, then widened: the delta is printed and -y is named as not
    // enough. The mixed edit — one dropped, one added — is the widen path.
    // `host:time` stays throughout, because the profile grants it and a
    // ceiling that dropped it would trip the profile-exceeds-ceiling check
    // as well, which is a different test.
    write_consent(dir, ROOT_ID);
    fs::write(
        dir.join(".drt_root/project.json"),
        project_json(&["host:time", "host:net/*"], None),
    )
    .unwrap();
    let (ok, out) = audit(dir, &[]);
    assert!(!ok);
    assert!(
        out.contains("the ceiling widened since it was accepted"),
        "{out}"
    );
    assert!(out.contains("--accept-changes"), "{out}");
    assert!(out.contains("+ host:net/*"), "{out}");
    assert!(out.contains("- host:fs/*"), "{out}");

    // Narrowed only: silent for start, so audit says it would run.
    fs::write(
        dir.join(".drt_root/project.json"),
        project_json(&["host:time"], None),
    )
    .unwrap();
    let (ok, out) = audit(dir, &[]);
    assert!(ok, "{out}");
    assert!(out.contains("the ceiling narrowed"), "{out}");
    assert!(out.contains("- host:fs/*"), "{out}");

    // A consent file for another root never travels; one that is here is
    // not this root's, and start refuses it by name.
    write_consent(dir, "0192f0c1-8000-7000-8000-00000000ffff");
    let (ok, out) = audit(dir, &[]);
    assert!(!ok);
    assert!(out.contains("blocks: consent.json is for root"), "{out}");
}

#[test]
fn findings_are_every_problem_not_the_first_one() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path();
    write_root(dir, &["host:fs/*", "host:time"], None);
    write_consent(dir, ROOT_ID);

    // A stray profile is ignored and said so; it does not stop anything.
    fs::write(dir.join(".drt_root/profile/release.config.json"), b"{}").unwrap();
    let (ok, out) = audit(dir, &[]);
    assert!(ok, "{out}");
    assert!(
        out.contains("note: 'release.config.json' is in profile/ but not declared"),
        "{out}"
    );

    // Three blockers at once, all reported: the entry is gone, the profile
    // exceeds the ceiling, and a profile file does not parse.
    fs::remove_file(dir.join("dlua/app.dlua")).unwrap();
    fs::write(
        dir.join(".drt_root/profile/debug.config.json"),
        br#"{ "dlua_dir": "dlua/", "entry": "app.dlua", "caps": [{ "capability": "host:net/*" }] }"#,
    )
    .unwrap();
    fs::write(
        dir.join(".drt_root/profile/release.config.json"),
        b"{ not json",
    )
    .unwrap();
    let (ok, out) = audit(dir, &[]);
    assert!(!ok);
    assert!(
        out.contains(
            "blocks: profile 'debug' names entry 'app.dlua', which is not under dlua_dir 'dlua/'"
        ),
        "{out}"
    );
    assert!(out.contains("blocks: profile 'debug': "), "{out}");
    assert!(
        out.contains("blocks: profile/release.config.json does not parse"),
        "{out}"
    );
    assert!(out.contains("start would not run: 3 blocker(s)"), "{out}");
}

#[test]
fn the_binary_is_checked_by_hash_against_the_pinned_release_and_never_run() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path().join("root");
    let home = tmp.path().join("home");
    write_root(&dir, &["host:time"], Some("0.5.0"));
    write_consent(&dir, ROOT_ID);
    // Deliberately not executable, and not a binary at all: audit must
    // never find out the hard way.
    let fake = b"#!/bin/sh\necho if this ran the test failed\nexit 3\n";
    fs::write(dir.join(".drt_root/drt"), fake).unwrap();
    let hex = hex::encode(sha2::Sha256::digest(fake));

    // The pinned release's sums, as `dollup pull drt 0.5.0` will cache them.
    let sums = home.join(".dollup/cache/drt/0.5.0");
    fs::create_dir_all(&sums).unwrap();
    fs::write(
        sums.join("SHA256SUMS.txt"),
        format!("{hex}  drt_linux_static_x86_64\naaaa  BUILDINFO.txt\n"),
    )
    .unwrap();
    let (ok, out) = audit_with_home(&dir, &[], Some(&home));
    assert!(ok, "{out}");
    assert!(
        out.contains(
            "drt: 0.5.0 pinned, 0.5.0 present (.drt_root/drt is drt_linux_static_x86_64 by sha256"
        ),
        "{out}"
    );
    assert!(out.contains("audit never executes it"), "{out}");
    assert!(!out.contains("if this ran"), "{out}");

    // The same binary under sums that do not list it: not that release,
    // and start would refuse the mismatch, so audit blocks by name.
    fs::write(
        sums.join("SHA256SUMS.txt"),
        "bbbb  drt_linux_static_x86_64\n",
    )
    .unwrap();
    let (ok, out) = audit_with_home(&dir, &[], Some(&home));
    assert!(!ok);
    assert!(
        out.contains("blocks: drt: 0.5.0 pinned, but .drt_root/drt is not a 0.5.0 build"),
        "{out}"
    );
}

#[test]
fn no_root_here_is_said_and_discovery_does_not_walk_up() {
    let tmp = tempfile::tempdir().unwrap();
    let parent = tmp.path();
    write_root(parent, &["host:time"], None);
    write_consent(parent, ROOT_ID);
    // A subdirectory of a root is not in that root.
    let inner = parent.join("dlua");
    let (ok, out) = audit(&inner, &[]);
    assert!(!ok);
    assert!(out.contains("no root:"), "{out}");
    assert!(out.contains("discovery does not walk up"), "{out}");
    assert!(out.contains("blocks: no project.json and none of"), "{out}");
    // While the root itself is fine.
    let (ok, out) = audit(parent, &[]);
    assert!(ok, "{out}");
}

#[test]
fn a_released_root_keeps_its_entry_under_init_and_audit_looks_there() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path();
    write_root(dir, &["host:time"], None);
    write_consent(dir, ROOT_ID);
    // A released profile sets no dlua_dir: its entry is delivered content,
    // under init/, and dlua/ is not where to look — checking it would
    // report every released root's entry as missing.
    fs::write(
        dir.join(".drt_root/profile/debug.config.json"),
        br#"{ "entry": "app.dlua", "caps": [{ "capability": "host:time" }] }"#,
    )
    .unwrap();
    fs::remove_file(dir.join("dlua/app.dlua")).unwrap();
    let (ok, out) = audit(dir, &[]);
    assert!(!ok);
    assert!(
        out.contains("names entry 'app.dlua', which is not under dlua_dir 'init/'"),
        "{out}"
    );

    fs::create_dir_all(dir.join(".drt_root/init")).unwrap();
    fs::write(dir.join(".drt_root/init/app.dlua"), "print('shipped')\n").unwrap();
    let (ok, out) = audit(dir, &[]);
    assert!(ok, "{out}");
    assert!(
        out.contains("entry: app.dlua (from the profile), present"),
        "{out}"
    );
    assert!(
        out.contains("source: .drt_root/init/ (delivered content; the profile sets no dlua_dir)"),
        "{out}"
    );
}
