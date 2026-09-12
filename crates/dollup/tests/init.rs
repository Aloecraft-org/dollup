//! `dollup init`: the root it writes, and the rule it writes by — create
//! what is missing, never rewrite what exists. The proof that the root is
//! right is `dollup audit` on it: the shared resolution says start would
//! run, silently, which is what "a locally authored project never prompts
//! on first start" means in practice.

use std::fs;
use std::path::Path;
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

fn read(path: &Path) -> serde_json::Value {
    serde_json::from_slice(&fs::read(path).unwrap()).unwrap()
}

#[test]
fn init_writes_a_root_that_audit_says_would_run() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path().join("my_drt_project");
    let out = run(dollup()
        .arg("--root")
        .arg(&dir)
        .args(["init", "my_drt_project"]));
    assert!(out.contains("wrote .drt_root/project.json"), "{out}");

    // The layout, as the design lists it.
    for sub in ["init", "live", "log", "profile", "state"] {
        assert!(dir.join(".drt_root").join(sub).is_dir(), "{sub}/");
    }
    for file in [
        ".drt_root/project.json",
        ".drt_root/consent.json",
        ".drt_root/dollup.lock",
        ".drt_root/profile/debug.config.json",
        ".drt_root/profile/preflight.config.json",
        "dlua/app.dlua",
    ] {
        assert!(dir.join(file).is_file(), "{file}");
    }
    assert!(!dir.join("dollup.json").exists(), "dollup.json is retired");

    let project = read(&dir.join(".drt_root/project.json"));
    assert_eq!(project["project_name"], "my_drt_project");
    assert_eq!(project["project_version"], "0.0.0");
    assert_eq!(project["default_profile"], "debug");
    assert_eq!(
        project["profiles"],
        serde_json::json!(["debug.config.json", "preflight.config.json"])
    );
    assert_eq!(project["require_signatures"], true);
    assert_eq!(project["caps"][0]["capability"], "host:time");
    // The standard source, key pinned, as dollup.json used to carry it.
    let url = project["sources"][0]["url"].as_str().unwrap();
    assert!(url.starts_with("https://dollup.aloecraft.org/"), "{url}");
    let key = project["sources"][0]["keys"][0].as_str().unwrap();
    assert!(key.starts_with("ed25519:"), "{key}");
    assert!(
        project.get("drt").is_none(),
        "nothing is pinned until `dollup pin drt`"
    );
    // root_id is a uuid7 minted now, not derived from anything.
    let root_id = project["root_id"].as_str().unwrap();
    let id = drt_config::id::Uuid7::parse(root_id).unwrap();
    assert!(id.unix_ms() > 1_700_000_000_000, "{root_id}");

    // consent.json accepts exactly the ceiling just declared, listed, so
    // the first start does not ask and the first widening does.
    let consent = read(&dir.join(".drt_root/consent.json"));
    assert_eq!(consent["root_id"], root_id);
    assert_eq!(consent["accepted"][0]["mode"], "listed");
    assert_eq!(consent["accepted"][0]["realm"], "operator");
    assert_eq!(consent["accepted"][0]["ceiling"], project["caps"]);

    // The hello names the project and the moment.
    let app = fs::read_to_string(dir.join("dlua/app.dlua")).unwrap();
    assert!(app.starts_with("--- my_drt_project created at 20"), "{app}");
    assert!(app.contains("print(\"Hello from app.dlua\")"), "{app}");

    // And the shared resolution agrees: start would run, silently.
    let out = run(dollup().arg("--root").arg(&dir).arg("audit"));
    assert!(out.contains("profile: debug (default_profile)"), "{out}");
    assert!(
        out.contains("consent: listed, matches the ceiling"),
        "{out}"
    );
    assert!(
        out.contains("entry: app.dlua (from the profile), present"),
        "{out}"
    );
    assert!(out.contains("start would run. nothing started."), "{out}");
    // preflight is a declared profile of the same root.
    let out = run(dollup()
        .arg("--root")
        .arg(&dir)
        .args(["audit", "preflight"]));
    assert!(
        out.contains("entry: stdlib:preflight (from the profile), resolved by the binary"),
        "{out}"
    );
    assert!(out.contains("start would run"), "{out}");
}

#[test]
fn init_creates_what_is_missing_and_never_rewrites_what_exists() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path().join("p");
    run(dollup().arg("--root").arg(&dir).args(["init", "p"]));

    // Edit everything a person would edit.
    let app = dir.join("dlua/app.dlua");
    fs::write(&app, "print('mine')\n").unwrap();
    let profile = dir.join(".drt_root/profile/debug.config.json");
    fs::write(
        &profile,
        br#"{ "dlua_dir": "dlua/", "entry": "app.dlua", "caps": [], "args": { "verbose": true } }"#,
    )
    .unwrap();
    let descriptor = dir.join(".drt_root/project.json");
    let descriptor_before = fs::read(&descriptor).unwrap();
    let consent = dir.join(".drt_root/consent.json");
    let consent_before = fs::read(&consent).unwrap();

    // A second init, even with another name, touches none of it.
    let out = run(dollup().arg("--root").arg(&dir).args(["init", "renamed"]));
    assert!(out.contains("kept  .drt_root/project.json"), "{out}");
    assert!(out.contains("init never renames"), "{out}");
    assert_eq!(fs::read_to_string(&app).unwrap(), "print('mine')\n");
    assert!(fs::read_to_string(&profile)
        .unwrap()
        .contains("\"verbose\": true"));
    assert_eq!(fs::read(&descriptor).unwrap(), descriptor_before);
    assert_eq!(fs::read(&consent).unwrap(), consent_before);

    // A profile the root lacked: created and declared, and that is the
    // whole edit — the default does not move, consent does not move, the
    // program is not touched.
    let out = run(dollup()
        .arg("--root")
        .arg(&dir)
        .args(["init", "p", "release"]));
    assert!(
        out.contains("wrote .drt_root/profile/release.config.json"),
        "{out}"
    );
    assert!(out.contains("edited .drt_root/project.json"), "{out}");
    let project = read(&descriptor);
    assert_eq!(
        project["profiles"],
        serde_json::json!([
            "debug.config.json",
            "preflight.config.json",
            "release.config.json"
        ])
    );
    assert_eq!(project["default_profile"], "debug");
    assert_eq!(fs::read_to_string(&app).unwrap(), "print('mine')\n");
    assert_eq!(fs::read(&consent).unwrap(), consent_before);
    let out = run(dollup().arg("--root").arg(&dir).args(["audit", "release"]));
    assert!(
        out.contains("profile: release (named on the command line)"),
        "{out}"
    );
    assert!(out.contains("start would run"), "{out}");
}

#[test]
fn a_named_profile_is_the_default_and_refusals_write_nothing() {
    let tmp = tempfile::tempdir().unwrap();

    // The named profile is the default one, under either spelling.
    let dir = tmp.path().join("q");
    run(dollup()
        .arg("--root")
        .arg(&dir)
        .args(["init", "q", "release"]));
    let project = read(&dir.join(".drt_root/project.json"));
    assert_eq!(project["default_profile"], "release");
    assert_eq!(
        project["profiles"],
        serde_json::json!(["release.config.json", "preflight.config.json"])
    );
    assert!(!dir.join(".drt_root/profile/debug.config.json").exists());
    let dir2 = tmp.path().join("q2");
    run(dollup()
        .arg("--root")
        .arg(&dir2)
        .args(["init", "q2", "prod.config.json"]));
    assert_eq!(
        read(&dir2.join(".drt_root/project.json"))["default_profile"],
        "prod"
    );

    // No name is legal; audit is what says so, and the hello still runs.
    let dir3 = tmp.path().join("unnamed");
    run(dollup().arg("--root").arg(&dir3).arg("init"));
    assert!(read(&dir3.join(".drt_root/project.json"))
        .get("project_name")
        .is_none());
    let out = run(dollup().arg("--root").arg(&dir3).arg("audit"));
    assert!(out.contains("note: no project_name is set"), "{out}");
    assert!(out.contains("start would run"), "{out}");
    let app = fs::read_to_string(dir3.join("dlua/app.dlua")).unwrap();
    assert!(app.starts_with("--- unnamed created at"), "{app}");

    // Reserved names, for the project and for the profile, are refused
    // before anything is written.
    let dir4 = tmp.path().join("r");
    let msg = fail(dollup().arg("--root").arg(&dir4).args(["init", "live"]));
    assert!(msg.contains("'live' is a reserved name"), "{msg}");
    assert!(!dir4.exists(), "nothing written");
    let msg = fail(
        dollup()
            .arg("--root")
            .arg(&dir4)
            .args(["init", "r", "state"]),
    );
    assert!(msg.contains("'state' is a reserved name"), "{msg}");
    assert!(!dir4.exists(), "nothing written");

    // A dollup.json app is not a root, and init does not make it one.
    let dir5 = tmp.path().join("app");
    fs::create_dir_all(&dir5).unwrap();
    fs::write(dir5.join("dollup.json"), b"{ \"sources\": [] }").unwrap();
    let msg = fail(dollup().arg("--root").arg(&dir5).arg("init"));
    assert!(msg.contains("dollup.json app"), "{msg}");
    assert!(!dir5.join(".drt_root").exists());
}
