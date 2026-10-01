use kah_codex::overlay::{Policy, prepare};
use std::{fs, os::unix::fs::symlink, path::PathBuf};

struct Fixture(PathBuf);
impl Fixture {
    fn new(name: &str) -> Self {
        let root = std::env::temp_dir().join(format!("kah-overlay-{}-{name}", std::process::id()));
        fs::create_dir_all(root.join("original")).unwrap();
        fs::write(root.join("original/config.toml"), "model='user-choice'").unwrap();
        Self(root)
    }
    fn prepare(&self) -> Result<(), kah_codex::overlay::OverlayError> {
        prepare(
            &self.0.join("original"),
            &self.0.join("overlay"),
            &Policy {
                restricted: true,
                online: false,
                root: &self.0,
                cwd: &self.0,
            },
        )
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).unwrap();
    }
}

#[test]
fn mcp_can_select_an_online_executor_without_changing_the_offline_default() {
    let f = Fixture::new("multiple-executors");
    fs::create_dir(f.0.join("overlay")).unwrap();
    let offline = vec!["--network".into(), "none".into()];
    let online = vec!["--network".into(), "bridge".into()];
    kah_codex::overlay::set_executors(
        &f.0.join("overlay"),
        "tools-offline",
        &[("tools-offline", &offline), ("mcp-online", &online)],
    )
    .unwrap();
    let config: toml::Value =
        toml::from_str(&fs::read_to_string(f.0.join("overlay/environments.toml")).unwrap())
            .unwrap();
    assert_eq!(config["default"].as_str(), Some("tools-offline"));
    assert_eq!(config["include_local"].as_bool(), Some(false));
    let envs = config["environments"].as_array().unwrap();
    assert_eq!(envs.len(), 2);
    assert_eq!(envs[1]["id"].as_str(), Some("mcp-online"));
    assert_eq!(envs[1]["args"][1].as_str(), Some("bridge"));
}

#[test]
fn native_config_writes_do_not_reach_original_but_login_and_history_are_shared() {
    let f = Fixture::new("independent");
    f.prepare().unwrap();
    let overlay = f.0.join("overlay");
    fs::write(overlay.join("config.toml"), "native local setting").unwrap();
    assert_eq!(
        fs::read_to_string(f.0.join("original/config.toml")).unwrap(),
        "model='user-choice'"
    );
    for name in ["auth.json", "sessions", "history.jsonl"] {
        assert_eq!(
            fs::read_link(overlay.join(name)).unwrap(),
            f.0.join("original").join(name)
        );
    }
    assert!(
        fs::symlink_metadata(overlay.join("hooks.json"))
            .unwrap()
            .is_file()
    );
    f.prepare().unwrap();
}

#[test]
fn a_previous_config_symlink_is_refused_without_touching_the_global_target() {
    let f = Fixture::new("symlink");
    fs::create_dir(f.0.join("overlay")).unwrap();
    symlink(
        f.0.join("original/config.toml"),
        f.0.join("overlay/config.toml"),
    )
    .unwrap();
    assert_eq!(f.prepare().unwrap_err().code, "OVERLAY_CONFIG_TYPE");
    assert_eq!(
        fs::read_to_string(f.0.join("original/config.toml")).unwrap(),
        "model='user-choice'"
    );
}

#[test]
fn mine_merges_hooks_into_a_regular_file_and_migrates_only_its_old_symlink() {
    let f = Fixture::new("merged-hooks");
    let original = f.0.join("original");
    let overlay = f.0.join("overlay");
    let hooks =
        r#"{"hooks":{"SessionStart":[{"hooks":[{"type":"command","command":"user-hook"}]}]}}"#;
    fs::write(original.join("hooks.json"), hooks).unwrap();
    fs::create_dir(&overlay).unwrap();
    symlink(original.join("hooks.json"), overlay.join("hooks.json")).unwrap();
    prepare(
        &original,
        &overlay,
        &Policy {
            restricted: false,
            online: false,
            root: &f.0,
            cwd: &f.0,
        },
    )
    .unwrap();
    assert!(
        fs::symlink_metadata(overlay.join("hooks.json"))
            .unwrap()
            .is_file()
    );
    assert_eq!(
        fs::read_to_string(original.join("hooks.json")).unwrap(),
        hooks
    );
    let config: toml::Value =
        toml::from_str(&fs::read_to_string(overlay.join("config.toml")).unwrap()).unwrap();
    assert!(
        config
            .get("hooks")
            .is_none_or(|h| h.as_table().unwrap().keys().all(|k| k == "state"))
    );
    f.prepare().unwrap();
    let restricted: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(overlay.join("hooks.json")).unwrap()).unwrap();
    assert!(restricted["hooks"].as_object().unwrap().is_empty());
}

#[test]
fn merged_json_and_toml_hooks_preserve_order_and_remap_trust_without_approving_changes() {
    let f = Fixture::new("trust-map");
    let original = f.0.join("original");
    let target = f.0.join("overlay/hooks.json");
    fs::write(
        original.join("hooks.json"),
        r#"{"hooks":{"SessionStart":[{"hooks":[{"type":"command","command":"json-command"}]}]}}"#,
    )
    .unwrap();
    fs::write(
        original.join("config.toml"),
        format!(
            r#"
[[hooks.SessionStart]]
hooks = [{{type='command', command='toml-command'}}]
[hooks.state.'{}:session_start:0:0']
enabled = false
trusted_hash = 'unchanged-hash'
[hooks.state.'{}:session_start:0:0']
trusted_hash = 'second-hash'
[hooks.state.'stale']
trusted_hash = 'never-copy'
"#,
            original.join("hooks.json").display(),
            original.join("config.toml").display()
        ),
    )
    .unwrap();
    prepare(
        &original,
        &f.0.join("overlay"),
        &Policy {
            restricted: false,
            online: false,
            root: &f.0,
            cwd: &f.0,
        },
    )
    .unwrap();
    let config: toml::Value =
        toml::from_str(&fs::read_to_string(f.0.join("overlay/config.toml")).unwrap()).unwrap();
    let states = config["hooks"]["state"].as_table().unwrap();
    assert_eq!(states.len(), 2);
    assert_eq!(
        states[&format!("{}:session_start:0:0", target.display())]["enabled"].as_bool(),
        Some(false)
    );
    assert_eq!(
        states[&format!("{}:session_start:1:0", target.display())]["trusted_hash"].as_str(),
        Some("second-hash")
    );
    let file: serde_json::Value = serde_json::from_slice(&fs::read(target).unwrap()).unwrap();
    assert_eq!(
        file["hooks"]["SessionStart"][0]["hooks"][0]["command"],
        "json-command"
    );
    assert_eq!(
        file["hooks"]["SessionStart"][1]["hooks"][0]["command"],
        "toml-command"
    );
}
