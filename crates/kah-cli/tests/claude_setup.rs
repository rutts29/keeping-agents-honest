use kah_cli::claude_setup::Setup;
use serde_json::json;
use std::{ffi::OsStr, process::Command};

#[test]
fn restricted_launch_uses_the_single_native_credential_store() {
    let setup = Setup {
        settings: json!({}),
        mcp: None,
        prefix: "/fixture/prefix".into(),
        temporary: "/fixture/tmp".into(),
    };
    let mut command = Command::new("claude");
    command.env("CLAUDE_CONFIG_DIR", "/obsolete/domain-profile");
    setup.apply(&mut command);
    assert!(
        command
            .get_envs()
            .any(|(name, value)| { name == OsStr::new("CLAUDE_CONFIG_DIR") && value.is_none() })
    );
    assert_eq!(
        command.get_args().collect::<Vec<_>>(),
        ["--setting-sources", "user"]
    );
}

#[test]
fn mcp_configuration_is_passed_by_private_path_not_secret_bearing_json() {
    let setup = Setup {
        settings: json!({}),
        mcp: Some("/private/control/mcp.json".into()),
        prefix: "/fixture/prefix".into(),
        temporary: "/fixture/tmp".into(),
    };
    let mut command = Command::new("claude");
    setup.apply(&mut command);
    assert_eq!(
        command.get_args().collect::<Vec<_>>(),
        [
            "--setting-sources",
            "user",
            "--mcp-config",
            "/private/control/mcp.json"
        ]
    );
}

#[test]
fn native_mcp_overlay_keeps_commands_and_routes_explicit_policy() {
    let home = std::env::temp_dir().join(format!("kah-claude-mcp-{}", std::process::id()));
    std::fs::create_dir_all(&home).unwrap();
    let original = json!({"mcpServers":{
        "normal":{"command":"node","args":["server.js"],"env":{"TOOL_TOKEN":"fixture-only"}},
        "offline":{"command":"local-index"},
        "disabled":{"command":"secret-service"},
        "remote":{"type":"http","url":"https://example.com/mcp"}
    }});
    let path = home.join(".claude.json");
    std::fs::write(&path, original.to_string()).unwrap();
    let policy = kah_cli::setup_policy::SetupPolicy {
        offline_mcp_servers: ["offline".into()].into(),
        secret_mcp_servers: [("disabled".into(), false), ("normal".into(), true)].into(),
    };
    let (servers, names) = kah_cli::claude_setup::mcp_configuration(&home, &policy).unwrap();
    assert_eq!(servers["normal"]["command"], "node");
    assert_eq!(
        servers["normal"]["env"]["KAH_SANDBOX_PROFILE"],
        "mcp-online"
    );
    assert_eq!(
        servers["offline"]["env"]["KAH_SANDBOX_PROFILE"],
        "mcp-offline"
    );
    assert!(servers.get("disabled").is_none());
    assert_eq!(servers["remote"], original["mcpServers"]["remote"]);
    assert_eq!(names, ["TOOL_TOKEN"]);
    assert_eq!(std::fs::read_to_string(path).unwrap(), original.to_string());
    std::fs::remove_dir_all(home).unwrap();
}
