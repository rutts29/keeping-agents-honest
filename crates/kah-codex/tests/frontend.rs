use kah_codex::frontend::{command, prepare};
use std::{
    fs,
    os::unix::{fs::symlink, net::UnixListener},
};

#[test]
fn remote_client_has_no_credentials_and_missing_daemon_cannot_fall_back() {
    let root = std::env::temp_dir().join(format!("kah-authless-client-{}", std::process::id()));
    let login = root.join("login");
    let client = root.join("client");
    fs::create_dir_all(&login).unwrap();
    fs::write(login.join("config.toml"), concat!("model='gpt-6.1-sol'\nmodel_reasoning_effort='low'\ncli_auth_credentials_store='file'\n[mcp_servers.private]\ncommand='server'\nenv={SECRET=", "'synthetic-tool-secret'}\n")).unwrap();
    fs::write(login.join("auth.json"), "synthetic-login-not-to-copy").unwrap();
    let before = fs::read(login.join("config.toml")).unwrap();
    prepare(&login, &client).unwrap();
    assert!(!client.join("auth.json").exists());
    assert!(fs::symlink_metadata(client.join("auth.json")).is_err());
    let config = fs::read_to_string(client.join("config.toml")).unwrap();
    assert!(config.contains("ephemeral"));
    assert!(config.contains("gpt-6.1-sol"));
    assert!(!config.contains("synthetic-tool-secret"));
    let socket = root.join("daemon.sock");
    let launch = |args: &[String]| {
        command(
            std::path::Path::new("/native/codex"),
            &client,
            &socket,
            &root,
            args,
        )
    };
    assert!(
        launch(&[])
            .unwrap_err()
            .to_string()
            .starts_with("CODEX_DAEMON_REQUIRED")
    );
    let listener = UnixListener::bind(&socket).unwrap();
    let cmd = launch(&["resume".into(), "session".into()]).unwrap();
    let args = cmd
        .get_args()
        .map(|s| s.to_string_lossy().into_owned())
        .collect::<Vec<_>>();
    assert!(args.contains(&"--remote".into()));
    assert!(!args.contains(&"--no-daemon".into()));
    assert!(launch(&["--no-daemon".into()]).is_err());
    symlink(login.join("auth.json"), client.join("auth.json")).unwrap();
    assert!(prepare(&login, &client).is_err());
    assert!(launch(&[]).is_err());
    assert_eq!(fs::read(login.join("config.toml")).unwrap(), before);
    assert_eq!(
        fs::read_to_string(login.join("auth.json")).unwrap(),
        "synthetic-login-not-to-copy"
    );
    drop(listener);
    fs::remove_dir_all(root).unwrap();
}
