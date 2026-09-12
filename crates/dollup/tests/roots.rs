//! `dollup roots` and the list behind it, under a HOME the test controls.
//! What is asserted: a verb that opens a root records it, a copied root is
//! seen the first time it is touched and named as sharing an id, a removed
//! root stays on the list as stale, and audit reads the list without
//! joining it.

use std::fs;
use std::path::Path;
use std::process::Command;

fn dollup(home: &Path) -> Command {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_dollup"));
    cmd.env("HOME", home);
    cmd
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

fn copy_dir(from: &Path, to: &Path) {
    fs::create_dir_all(to).unwrap();
    for entry in fs::read_dir(from).unwrap() {
        let entry = entry.unwrap();
        let target = to.join(entry.file_name());
        if entry.path().is_dir() {
            copy_dir(&entry.path(), &target);
        } else {
            fs::copy(entry.path(), target).unwrap();
        }
    }
}

#[test]
fn roots_are_recorded_when_opened_and_checked_against_disk_when_listed() {
    let tmp = tempfile::tempdir().unwrap();
    let home = tmp.path().join("home");
    let a = tmp.path().join("a");
    let b = tmp.path().join("b");
    run(dollup(&home).arg("--root").arg(&a).args(["init", "alpha"]));
    run(dollup(&home).arg("--root").arg(&b).args(["init", "beta"]));
    // The list holds canonical paths; compare against those.
    let (a, b) = (a.canonicalize().unwrap(), b.canonicalize().unwrap());

    let out = run(dollup(&home).arg("roots"));
    assert!(out.contains("alpha"), "{out}");
    assert!(out.contains("beta"), "{out}");
    assert!(!out.contains("duplicate"), "{out}");
    assert!(!out.contains("stale"), "{out}");

    // A cp -r: not on the list until touched, then named as sharing an id
    // — on the list and by audit, which does not join the list itself.
    let c = tmp.path().join("c");
    copy_dir(&a, &c);
    let c = c.canonicalize().unwrap();
    let out = run(dollup(&home).arg("roots"));
    assert!(!out.contains(&c.display().to_string()), "{out}");
    run(dollup(&home).arg("--root").arg(&c).arg("ls"));
    let out = run(dollup(&home).arg("roots"));
    assert!(out.contains(&c.display().to_string()), "{out}");
    assert!(out.contains("duplicate root_id"), "{out}");
    assert!(out.contains("dollup duplicate"), "{out}");
    let out = run(dollup(&home).arg("--root").arg(&c).arg("audit"));
    assert!(
        out.contains("note: another root on this box claims root_id"),
        "{out}"
    );
    assert!(out.contains(&a.display().to_string()), "{out}");
    assert!(out.contains("start would run"), "{out}");

    // rm -rf: still listed, as stale, with the reason.
    fs::remove_dir_all(&b).unwrap();
    let out = run(dollup(&home).arg("roots"));
    assert!(out.contains(&b.display().to_string()), "{out}");
    assert!(out.contains("stale:"), "{out}");
    assert!(out.contains("is gone"), "{out}");
}

#[test]
fn audit_reads_the_list_and_never_joins_it() {
    let tmp = tempfile::tempdir().unwrap();
    let home = tmp.path().join("home");
    let other_home = tmp.path().join("other-home");
    let d = tmp.path().join("d");
    // Made under one HOME, audited under another: the second list stays
    // empty, because audit writes nothing — this list included.
    run(dollup(&other_home)
        .arg("--root")
        .arg(&d)
        .args(["init", "delta"]));
    run(dollup(&home).arg("--root").arg(&d).arg("audit"));
    let out = run(dollup(&home).arg("roots"));
    assert!(out.contains("no roots recorded on this box"), "{out}");
    assert!(!home.join(".dollup/roots.json").exists());

    // While consent, which reads the descriptor itself, does record it.
    run(dollup(&home).arg("--root").arg(&d).arg("consent"));
    let out = run(dollup(&home).arg("roots"));
    assert!(out.contains("delta"), "{out}");
}
