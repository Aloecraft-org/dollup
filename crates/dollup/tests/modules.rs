//! Where a pulled package's modules land: at the paths their names resolve
//! to under `init/`, by the loader's own rule, so `require("util.enc")`
//! finds `util/enc.dlua` under the name the package gave it. Everything
//! else a package ships sits under `<name>/`, and the manifest is not
//! materialized at all. Two packages naming one module, or a module landing
//! on a file no package owns, are refused by name at pull.

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

/// A package: modules as (name, path, body), plus files that are not
/// modules, hashed as `repo seal` would.
fn write_package(
    repo: &Path,
    name: &str,
    main: Option<&str>,
    modules: &[(&str, &str, &[u8])],
    extra: &[(&str, &[u8])],
) {
    let dir = repo.join("packages").join(name).join("0.1.0");
    let mut files = BTreeMap::new();
    let mut module_map = serde_json::Map::new();
    for (module, path, body) in modules {
        let full = dir.join(path);
        fs::create_dir_all(full.parent().unwrap()).unwrap();
        fs::write(&full, body).unwrap();
        files.insert(
            path.to_string(),
            format!("sha256:{}", hex::encode(sha2::Sha256::digest(body))),
        );
        module_map.insert(
            module.to_string(),
            serde_json::Value::String(path.to_string()),
        );
    }
    for (path, body) in extra {
        let full = dir.join(path);
        fs::create_dir_all(full.parent().unwrap()).unwrap();
        fs::write(&full, body).unwrap();
        files.insert(
            path.to_string(),
            format!("sha256:{}", hex::encode(sha2::Sha256::digest(body))),
        );
    }
    let mut guest = serde_json::json!({ "modules": module_map });
    if let Some(main) = main {
        guest["main"] = main.into();
    }
    let manifest = serde_json::json!({
        "name": name,
        "version": "0.1.0",
        "guest": guest,
        "files": files
    });
    fs::write(
        dir.join("manifest.json"),
        serde_json::to_vec_pretty(&manifest).unwrap(),
    )
    .unwrap();
}

fn root_with_source(dir: &Path, repo: &Path) {
    run(common::dollup()
        .arg("--root")
        .arg(dir)
        .args(["init", "demo"]));
    let path = dir.join(".drt_root/project.json");
    let mut project: serde_json::Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    project["sources"] = serde_json::json!([format!("file://{}", repo.display())]);
    fs::write(&path, serde_json::to_vec_pretty(&project).unwrap()).unwrap();
}

#[test]
fn modules_land_where_their_names_resolve_and_nothing_else_moves() {
    let tmp = tempfile::tempdir().unwrap();
    let repo = tmp.path().join("repo");
    write_package(
        &repo,
        "enc",
        None,
        &[
            (
                "util.enc",
                "src/enc.dlua",
                b"return { encode = function() end }",
            ),
            ("util.b64", "src/b64.lua", b"return {}"),
        ],
        &[("README.md", b"# enc\n")],
    );
    write_package(
        &repo,
        "app",
        Some("app"),
        &[("app", "app.dlua", b"print('app')")],
        &[],
    );
    run(common::dollup().args(["repo", "index"]).arg(&repo));
    let root = tmp.path().join("root");
    root_with_source(&root, &repo);

    let out = run(common::dollup()
        .arg("--root")
        .arg(&root)
        .args(["pull", "enc"]));
    assert!(out.contains("require: util.b64, util.enc"), "{out}");
    let init = root.join(".drt_root/init");
    // `db.claims` at `db/claims.dlua`: the name's path, the file's extension.
    assert!(init.join("util/enc.dlua").is_file());
    assert!(init.join("util/b64.lua").is_file(), ".lua is a module too");
    // Not a module: under the package's own directory, as before.
    assert!(init.join("enc/README.md").is_file());
    // And no manifest in the deployable tree.
    assert!(!init.join("enc/manifest.json").exists());
    assert!(!init.join("enc/src").exists());
    // The lock knows the same paths, and verify checks them there.
    let lock: serde_json::Value =
        serde_json::from_slice(&fs::read(root.join(".drt_root/dollup.lock")).unwrap()).unwrap();
    let files = lock["packages"]["enc"]["files"].as_object().unwrap();
    let mut paths: Vec<&String> = files.keys().collect();
    paths.sort();
    assert_eq!(paths, ["enc/README.md", "util/b64.lua", "util/enc.dlua"]);
    run(common::dollup().arg("--root").arg(&root).arg("verify"));
    fs::write(init.join("util/enc.dlua"), b"tampered").unwrap();
    let msg = fail(common::dollup().arg("--root").arg(&root).arg("verify"));
    assert!(
        msg.contains("enc: util/enc.dlua does not match the lock"),
        "{msg}"
    );

    // A runnable package says where its entry landed.
    let out = run(common::dollup()
        .arg("--root")
        .arg(&root)
        .args(["pull", "app"]));
    assert!(
        out.contains("runnable: a profile's entry \"app.dlua\" runs it"),
        "{out}"
    );
    assert!(init.join("app.dlua").is_file());
}

#[test]
fn one_module_path_belongs_to_one_package_and_never_to_a_file_nobody_owns() {
    let tmp = tempfile::tempdir().unwrap();
    let repo = tmp.path().join("repo");
    write_package(
        &repo,
        "lib-a",
        None,
        &[("util.enc", "a.dlua", b"return 'a'")],
        &[],
    );
    write_package(
        &repo,
        "lib-b",
        None,
        &[("util.enc", "b.dlua", b"return 'b'")],
        &[],
    );
    run(common::dollup().args(["repo", "index"]).arg(&repo));

    // Two packages, one module name: the second is refused naming the first.
    let root = tmp.path().join("root");
    root_with_source(&root, &repo);
    run(common::dollup()
        .arg("--root")
        .arg(&root)
        .args(["pull", "lib-a"]));
    let msg = fail(
        common::dollup()
            .arg("--root")
            .arg(&root)
            .args(["pull", "lib-b"]),
    );
    assert!(
        msg.contains(
            "'lib-b' would place 'b.dlua' at util/enc.dlua, which 'lib-a' already provides"
        ),
        "{msg}"
    );
    assert_eq!(
        fs::read(root.join(".drt_root/init/util/enc.dlua")).unwrap(),
        b"return 'a'",
        "lib-a's file is untouched"
    );

    // A file already at the path that no locked package owns — a committed
    // or hand-placed one — is never overwritten.
    let other = tmp.path().join("other");
    root_with_source(&other, &repo);
    let committed = other.join(".drt_root/init/util/enc.dlua");
    fs::create_dir_all(committed.parent().unwrap()).unwrap();
    fs::write(&committed, b"mine, committed").unwrap();
    let msg = fail(
        common::dollup()
            .arg("--root")
            .arg(&other)
            .args(["pull", "lib-a"]),
    );
    assert!(msg.contains("belongs to no locked package"), "{msg}");
    assert_eq!(fs::read(&committed).unwrap(), b"mine, committed");
}

#[test]
fn a_module_the_loader_could_not_reach_is_refused_at_index() {
    let tmp = tempfile::tempdir().unwrap();
    let repo = tmp.path().join("repo");
    write_package(
        &repo,
        "shadow",
        None,
        &[("stdlib.x", "x.dlua", b"return {}")],
        &[],
    );
    let msg = fail(common::dollup().args(["repo", "index"]).arg(&repo));
    assert!(
        msg.contains("module 'stdlib.x' cannot be required"),
        "{msg}"
    );
    assert!(msg.contains("`stdlib` is reserved"), "{msg}");
}
