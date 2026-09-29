use kah_codex::overlay::{Policy, render_config};
use std::path::Path;

fn policy(online: bool) -> Policy<'static> {
    Policy {
        restricted: true,
        online,
        root: Path::new("/repos/research/demo"),
        cwd: Path::new("/repos/research/demo/subdir"),
    }
}

#[test]
fn restricted_config_preserves_model_choice_but_removes_host_extensions() {
    let input = r#"
model = "user-selected-model"
model_reasoning_effort = "high"
notify = ["touch", "/host/canary"]
web_search = "live"
[mcp_servers.host]
command = "touch"
args = ["/host/canary"]
[hooks]
[[hooks.SessionStart]]
matcher = "*"
[features]
apps = true
plugins = true
hooks = true
[profiles.host]
notify = ["touch", "/host/canary"]
[projects."/repos/research/demo"]
trust_level = "trusted"
"#;
    let config = render_config(input, &policy(false)).unwrap();
    let value: toml::Value = toml::from_str(&config).unwrap();
    assert_eq!(value["model"].as_str(), Some("user-selected-model"));
    assert_eq!(value["model_reasoning_effort"].as_str(), Some("high"));
    for key in ["notify", "mcp_servers", "hooks", "profiles"] {
        assert!(value.get(key).is_none(), "{key}");
    }
    for key in ["apps", "plugins", "remote_plugin", "hooks"] {
        assert_eq!(value["features"][key].as_bool(), Some(false));
    }
    assert_eq!(value["web_search"].as_str(), Some("disabled"));
    for path in ["/repos/research/demo", "/repos/research/demo/subdir"] {
        assert_eq!(
            value["projects"][path]["trust_level"].as_str(),
            Some("untrusted")
        );
    }
}

#[test]
fn search_requires_explicit_online_and_no_model_is_invented() {
    let config = render_config("", &policy(true)).unwrap();
    let value: toml::Value = toml::from_str(&config).unwrap();
    assert_eq!(value["web_search"].as_str(), Some("live"));
    assert!(value.get("model").is_none());
    assert!(value.get("model_reasoning_effort").is_none());
}

#[test]
fn mine_keeps_user_preferences_in_an_independent_rendering() {
    let mut policy = policy(false);
    policy.restricted = false;
    let config = render_config(
        "model='chosen'\n[mcp_servers.local]\ncommand='user-program'",
        &policy,
    )
    .unwrap();
    let value: toml::Value = toml::from_str(&config).unwrap();
    assert_eq!(
        value["mcp_servers"]["local"]["command"].as_str(),
        Some("user-program")
    );
}
