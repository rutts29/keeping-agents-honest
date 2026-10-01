use kah_codex::overlay::{route_mcp, wrap_hook_commands};
use serde_json::json;
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
};

#[test]
fn mcp_routing_is_explicit_and_preserves_native_server_configuration() {
    let root = std::env::temp_dir().join(format!("kah-mcp-routing-{}", std::process::id()));
    fs::create_dir_all(&root).unwrap();
    let source = "[mcp_servers.network]\ncommand='node'\nargs=['server.js']\n[mcp_servers.local]\ncommand='tool'\n[mcp_servers.secret]\ncommand='private-tool'\n[mcp_servers.url]\nurl='https://example.com/mcp'\n";
    fs::write(root.join("config.toml"), source).unwrap();
    let offline = BTreeSet::from(["local".into()]);
    route_mcp(
        &root,
        &root,
        "offline",
        "online",
        &offline,
        &BTreeMap::from([("secret".into(), false)]),
    )
    .unwrap();
    let config: toml::Value =
        toml::from_str(&fs::read_to_string(root.join("config.toml")).unwrap()).unwrap();
    assert_eq!(
        config["mcp_servers"]["network"]["environment_id"].as_str(),
        Some("online")
    );
    assert_eq!(
        config["mcp_servers"]["local"]["environment_id"].as_str(),
        Some("offline")
    );
    assert_eq!(
        config["mcp_servers"]["secret"]["enabled"].as_bool(),
        Some(false)
    );
    assert!(config["mcp_servers"]["url"].get("environment_id").is_none());
    fs::write(root.join("config.toml"), source).unwrap();
    route_mcp(
        &root,
        &root,
        "offline",
        "online",
        &offline,
        &BTreeMap::from([("secret".into(), true)]),
    )
    .unwrap();
    let config: toml::Value =
        toml::from_str(&fs::read_to_string(root.join("config.toml")).unwrap()).unwrap();
    assert!(config["mcp_servers"]["secret"].get("enabled").is_none());
    fs::remove_dir_all(root).unwrap();
}
#[test]
fn hook_routing_keeps_the_body_quoted_and_retains_native_options() {
    let mut hooks = json!({"hooks":{"SessionStart":[{"matcher":"startup","hooks":[{"type":"command","command":"printf '%s' \"$VALUE\"","timeout":7}]}]}});
    wrap_hook_commands(
        &mut hooks,
        &[
            "/private/kah".into(),
            "--sandbox-shell".into(),
            "/private/spec".into(),
        ],
    )
    .unwrap();
    let hook = &hooks["hooks"]["SessionStart"][0]["hooks"][0];
    assert_eq!(hook["timeout"], 7);
    assert_eq!(hooks["hooks"]["SessionStart"][0]["matcher"], "startup");
    assert!(
        hook["command"]
            .as_str()
            .unwrap()
            .starts_with("'/private/kah' '--sandbox-shell' '/private/spec' 'printf")
    );
    assert!(hook["command"].as_str().unwrap().contains("'\\''"));
}
