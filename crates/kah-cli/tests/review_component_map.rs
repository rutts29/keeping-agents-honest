//! Native-first: git/git v2.53.0 Documentation/glossary-content.adoc
//! pathspec glob and git-diff.adoc. ls-tree does not support glob magic.
use kah_cli::observer_review::component_map::Map;
use serde_json::json;
use std::{fs, path::Path};
fn map() -> Map {
    serde_json::from_value(json!({"version":1,"components":[
        {"name":"parser","path_globs":["src/**"],"requirements":["parser-behavior"],
         "tests":[{"name":"parser test","command":"cargo test parser"}],"e2e_flows":[]}
    ]}))
    .unwrap()
}
fn git(root: &Path, args: &[&str]) -> String {
    let output = std::process::Command::new("/usr/bin/git")
        .env_clear()
        .env("PATH", "/usr/bin:/bin")
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .args([
            "-c",
            "core.hooksPath=/dev/null",
            "-c",
            "core.fsmonitor=false",
        ])
        .args(args)
        .current_dir(root)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).unwrap().trim().into()
}
fn commit(root: &Path) -> String {
    git(root, &["add", "--all"]);
    git(
        root,
        &[
            "-c",
            "user.name=fixture",
            "-c",
            "user.email=fixture@example.invalid",
            "commit",
            "-m",
            "fixture",
        ],
    );
    git(root, &["rev-parse", "HEAD"])
}
#[test]
fn exact_sha_globs_keep_deleted_files_and_force_decisions_for_unmapped_changes() {
    let root =
        std::env::temp_dir().join(format!("component-map-{}", kah_cli::relay_store::nonce()));
    fs::create_dir_all(root.join("src/deep")).unwrap();
    git(&root, &["init"]);
    fs::write(root.join("src/deep/parser.rs"), "old whole content").unwrap();
    fs::write(root.join("src/deleted.rs"), "deleted content").unwrap();
    let base = commit(&root);
    fs::write(root.join("src/deep/parser.rs"), "new whole content").unwrap();
    fs::remove_file(root.join("src/deleted.rs")).unwrap();
    fs::write(root.join("new-language.ts"), "export const x = 1;").unwrap();
    let head = commit(&root);
    let impact = map().collect(&root, &head, Some(&base)).unwrap();
    assert_eq!(impact["unmapped_paths"], json!(["new-language.ts"]));
    assert_eq!(
        impact["components"][0]["changed_paths"],
        json!(["src/deep/parser.rs", "src/deleted.rs"])
    );
    assert_eq!(
        impact["components"][0]["tests"][0]["command"],
        "cargo test parser"
    );
    assert_eq!(
        impact["components"][0]["requirements"],
        json!(["parser-behavior"])
    );
    assert!(impact["components"][0]["files"]["src/deleted.rs"].is_null());
    assert!(impact["cochange_history"].as_str().unwrap().contains(&head));
    collector(&root, &base, &impact);
    let fingerprint = impact["components"][0]["digest"].clone();
    assert_eq!(
        map().collect(&root, &head, Some(&head)).unwrap()["components"][0]["digest"],
        fingerprint,
        "component evidence depends on current blobs, not which older baseline named a deleted file"
    );
    fs::write(
        root.join("src/deep/parser.rs"),
        "uncommitted must not alter impact",
    )
    .unwrap();
    assert_eq!(
        map().collect(&root, &head, Some(&base)).unwrap()["components"][0]["digest"],
        fingerprint
    );
    fs::remove_dir_all(root).unwrap();
}
fn collector(root: &Path, base: &str, expected: &serde_json::Value) {
    let path = root.join("map.json");
    fs::write(&path, serde_json::to_vec(&map()).unwrap()).unwrap();
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_kah"))
        .args([
            "__review-project-context",
            "--commit-baseline",
            base,
            "--component-map",
        ])
        .arg(&path)
        .current_dir(root)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let context: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(&context["component_impact"], expected);
    assert!(
        context["committed_snapshot"]
            .as_str()
            .unwrap()
            .contains("new whole content")
    );
    let mut changed = map();
    changed.components[0].tests[0].command = "cargo test parser_regression".into();
    assert_ne!(
        changed
            .collect(root, context["head"].as_str().unwrap(), Some(base))
            .unwrap()["components"][0]["digest"],
        expected["components"][0]["digest"]
    );
}
#[test]
fn malformed_patterns_duplicate_components_and_unknown_requirements_are_named_errors() {
    let mut value = serde_json::to_value(map()).unwrap();
    for invalid in ["/etc/**", "../**", "src/../**", "src/\u{0}*"] {
        value["components"][0]["path_globs"] = json!([invalid]);
        let invalid: Map = serde_json::from_value(value.clone()).unwrap();
        assert!(
            invalid
                .validate(&["parser-behavior".into()])
                .unwrap_err()
                .to_string()
                .contains("REVIEW_MAP_PATH")
        );
    }
    let map = map();
    assert!(
        map.validate(&[])
            .unwrap_err()
            .to_string()
            .contains("parser-behavior")
    );
    map.validate(&["parser-behavior".into()]).unwrap();
    let mut duplicate = serde_json::to_value(map).unwrap();
    let first = duplicate["components"][0].clone();
    duplicate["components"].as_array_mut().unwrap().push(first);
    let duplicate: Map = serde_json::from_value(duplicate).unwrap();
    assert!(
        duplicate
            .validate(&["parser-behavior".into()])
            .unwrap_err()
            .to_string()
            .contains("REVIEW_MAP_COMPONENT")
    );
}
