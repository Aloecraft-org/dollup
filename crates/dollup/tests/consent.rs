//! `dollup consent`, on roots `dollup init` wrote, with no terminal
//! attached — which is the case that has to fail by name rather than hang.
//! The interactive yes is the one path not covered here; everything it
//! decides on is.

use std::fs;
use std::path::Path;
use std::process::{Command, Stdio};

fn dollup() -> Command {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_dollup"));
    // No terminal, on purpose: the flags and the refusals are the test.
    cmd.stdin(Stdio::null());
    cmd
}

fn output(cmd: &mut Command) -> (bool, String) {
    let out = cmd.output().unwrap();
    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    (out.status.success(), text)
}

fn consent(dir: &Path, args: &[&str]) -> (bool, String) {
    output(dollup().arg("--root").arg(dir).arg("consent").args(args))
}

fn audit(dir: &Path) -> (bool, String) {
    output(dollup().arg("--root").arg(dir).arg("audit"))
}

fn init(dir: &Path) {
    let (ok, out) = output(dollup().arg("--root").arg(dir).args(["init", "demo"]));
    assert!(ok, "{out}");
}

/// Rewrite the ceiling in project.json, leaving everything else declared.
fn set_ceiling(dir: &Path, caps: &[&str]) {
    let path = dir.join(".drt_root/project.json");
    let mut project: serde_json::Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    project["caps"] = caps
        .iter()
        .map(|c| serde_json::json!({ "capability": c }))
        .collect();
    fs::write(&path, serde_json::to_vec_pretty(&project).unwrap()).unwrap();
}

fn mode(dir: &Path) -> String {
    let consent: serde_json::Value =
        serde_json::from_slice(&fs::read(dir.join(".drt_root/consent.json")).unwrap()).unwrap();
    consent["accepted"][0]["mode"].as_str().unwrap().to_string()
}

#[test]
fn a_fresh_root_has_nothing_to_accept_and_a_first_acceptance_takes_dash_y() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path();
    init(dir);
    let (ok, out) = consent(dir, &[]);
    assert!(ok, "{out}");
    assert!(out.contains("nothing to accept"), "{out}");

    // Consent gone: the ceiling is printed, and with no terminal the
    // refusal names the flag rather than waiting for an answer.
    fs::remove_file(dir.join(".drt_root/consent.json")).unwrap();
    let (ok, out) = consent(dir, &[]);
    assert!(!ok);
    assert!(out.contains("not yet accepted"), "{out}");
    assert!(out.contains("grant host:time"), "{out}");
    assert!(out.contains("no terminal to ask on, and no -y"), "{out}");
    assert!(
        !dir.join(".drt_root/consent.json").exists(),
        "nothing written"
    );

    let (ok, out) = consent(dir, &["-y"]);
    assert!(ok, "{out}");
    assert!(out.contains("accepted (-y)"), "{out}");
    assert_eq!(mode(dir), "listed");
    let (ok, out) = audit(dir);
    assert!(ok, "{out}");
    assert!(
        out.contains("consent: listed, matches the ceiling"),
        "{out}"
    );
}

#[test]
fn a_widened_ceiling_takes_accept_changes_and_dash_y_is_not_enough() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path();
    init(dir);
    set_ceiling(dir, &["host:time", "host:net/*"]);

    let (ok, out) = consent(dir, &[]);
    assert!(!ok);
    assert!(out.contains("the ceiling WIDENED"), "{out}");
    assert!(out.contains("+ host:net/*"), "{out}");
    assert!(
        out.contains("no terminal to ask on, and no --accept-changes"),
        "{out}"
    );

    // -y is a first acceptance only, and says so.
    let (ok, out) = consent(dir, &["-y"]);
    assert!(!ok);
    assert!(out.contains("-y accepts a first acceptance only"), "{out}");
    let (ok, out) = audit(dir);
    assert!(!ok, "still widened: {out}");

    let (ok, out) = consent(dir, &["--accept-changes"]);
    assert!(ok, "{out}");
    assert!(out.contains("accepted (--accept-changes)"), "{out}");
    let (ok, out) = audit(dir);
    assert!(ok, "{out}");
    assert!(
        out.contains("consent: listed, matches the ceiling"),
        "{out}"
    );
    assert!(out.contains("ceiling: 2 caps"), "{out}");
}

#[test]
fn a_narrowed_ceiling_is_accepted_silently_as_start_would() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path();
    init(dir);
    set_ceiling(dir, &["host:time", "host:net/*"]);
    let (ok, _) = consent(dir, &["--accept-changes"]);
    assert!(ok);
    set_ceiling(dir, &["host:time"]);

    let (ok, out) = consent(dir, &[]);
    assert!(ok, "{out}");
    assert!(out.contains("narrowed"), "{out}");
    assert!(out.contains("- host:net/*"), "{out}");
    let (ok, out) = audit(dir);
    assert!(ok, "{out}");
    assert!(
        out.contains("consent: listed, matches the ceiling"),
        "{out}"
    );
}

#[test]
fn all_is_the_blanket_entry_and_nothing_prompts_after_it() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path();
    init(dir);
    let (ok, out) = consent(dir, &["--all"]);
    assert!(ok, "{out}");
    assert!(out.contains("blanket operator consent"), "{out}");
    assert_eq!(mode(dir), "all");

    // Widen it: audit says so, out loud, and start would still run.
    set_ceiling(dir, &["host:time", "host:net/*", "host:exec/*"]);
    let (ok, out) = audit(dir);
    assert!(ok, "{out}");
    assert!(
        out.contains("consent: blanket operator consent, accepted 20"),
        "{out}"
    );
    assert!(
        out.contains("against a ceiling that has since changed"),
        "{out}"
    );
    assert!(out.contains("start would run"), "{out}");
    // And consent itself has nothing to do but say so.
    let (ok, out) = consent(dir, &[]);
    assert!(ok, "{out}");
    assert!(
        out.contains("blanket operator consent is in force"),
        "{out}"
    );
}

#[test]
fn a_consent_file_for_another_root_is_refused_by_name() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path();
    init(dir);
    let path = dir.join(".drt_root/consent.json");
    let mut consent_json: serde_json::Value =
        serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    consent_json["root_id"] = "0192f0c1-8000-7000-8000-00000000ffff".into();
    fs::write(&path, serde_json::to_vec_pretty(&consent_json).unwrap()).unwrap();
    let (ok, out) = consent(dir, &["-y"]);
    assert!(!ok);
    assert!(out.contains("consent.json is for root"), "{out}");

    // And no root at all is said, not guessed at.
    let empty = tmp.path().join("empty");
    fs::create_dir_all(&empty).unwrap();
    let (ok, out) = consent(&empty, &[]);
    assert!(!ok);
    assert!(out.contains("no root here"), "{out}");
}
