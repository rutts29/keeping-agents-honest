use kah_cli::setup_policy::SetupPolicy;
use std::fs;

#[test]
fn server_permissions_are_persistent_operator_declarations() {
    let home = std::env::temp_dir().join(format!("kah-setup-policy-{}", std::process::id()));
    fs::create_dir_all(home.join(".config/kah")).unwrap();
    let path = home.join(".config/kah/setup.json");
    fs::write(
        &path,
        r#"{"offline_mcp_servers":["local-files"],"secret_mcp_servers":{"private":false}}"#,
    )
    .unwrap();
    let first = SetupPolicy::load(&home).unwrap();
    assert!(first.offline_mcp_servers.contains("local-files"));
    assert_eq!(first.secret_mcp_servers.get("private"), Some(&false));
    fs::write(
        &path,
        r#"{"offline_mcp_servers":["local-files"],"secret_mcp_servers":{"private":true}}"#,
    )
    .unwrap();
    assert_eq!(
        SetupPolicy::load(&home)
            .unwrap()
            .secret_mcp_servers
            .get("private"),
        Some(&true)
    );
    assert_eq!(
        SetupPolicy::load(&home)
            .unwrap()
            .secret_mcp_servers
            .get("private"),
        Some(&true)
    );
    fs::remove_dir_all(home).unwrap();
}
