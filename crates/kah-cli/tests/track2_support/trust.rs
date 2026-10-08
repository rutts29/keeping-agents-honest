use super::fixture::save;
use kah_cli::run_pane::Pane;
use serde_json::{Value, json};
use std::{
    fs,
    path::{Path, PathBuf},
};
#[path = "dialog.rs"]
mod dialog;
pub use dialog::folder_dialog;

pub fn preseed_claude(home: &Path, project: &Path) {
    let path = kah_claude::paths::global_config(home);
    assert!(
        std::env::var_os("CLAUDE_CONFIG_DIR").as_deref()
            == Some(std::ffi::OsStr::new(super::claude_login::CONFIG_DIR))
            && path == Path::new(super::claude_login::CONFIG_DIR).join(".claude.json"),
        "TRACK2_CLAUDE_CONFIG_SCOPE stage=trust"
    );
    let root = home.parent().unwrap();
    if project.starts_with(home.join(".cache/kah-observers")) {
        let before: Value =
            serde_json::from_slice(&fs::read(root.join("observer-before.json")).unwrap()).unwrap();
        assert_eq!(
            before["existed"], false,
            "TRACK2_EXISTING_OBSERVER stage=trust"
        );
        assert!(project.is_dir(), "TRACK2_OBSERVER_CREATED stage=trust");
    }
    super::claude_trust::add(
        &path,
        home,
        project,
        &root.join("claude-trust-entries.json"),
    )
    .unwrap();
}
pub fn cleanup_claude(home: &Path) -> Value {
    let receipts = home.parent().unwrap().join("claude-trust-entries.json");
    if !receipts.exists() {
        return json!({"passed":true,"entries":[]});
    }
    let path = kah_claude::paths::global_config(home);
    assert_eq!(
        path,
        Path::new(super::claude_login::CONFIG_DIR).join(".claude.json"),
        "TRACK2_CLAUDE_CONFIG_SCOPE stage=cleanup"
    );
    match super::claude_trust::cleanup(&path, &receipts) {
        Ok(value) => value,
        Err(error) => json!({"passed":false,"error":error}),
    }
}

pub fn consent(pane: &Pane, provider: &str, project: &Path) {
    let view = pane.inspect().unwrap();
    let output = std::process::Command::new("herdr")
        .args([
            "pane", "read", &pane.id, "--source", "visible", "--format", "text",
        ])
        .output()
        .unwrap();
    let screen = String::from_utf8(output.stdout).unwrap();
    if view["agent_status"] != "blocked"
        && !screen.contains("Quick safety check:")
        && !screen.contains("Folder access")
        && !screen.contains("Open restricted")
    {
        return;
    }
    let answer = folder_dialog(provider, &screen, project);
    if let Err(reason) = &answer {
        let root = PathBuf::from(std::env::var_os("KAH_TRACK2_ROOT").unwrap());
        save(
            &root.join(format!("dialog-{}.json", pane.id.replace(':', "-"))),
            &json!({
            "reason":reason,"provider":provider,"pane":pane.id,"expected":project,"view":view,"screen":screen,"keys_sent":false}),
        );
    }
    assert_eq!(
        answer,
        Ok(true),
        "TRACK2_OTHER_DIALOG stage=startup: no keys sent"
    );
    assert!(
        std::process::Command::new("herdr")
            .args(["pane", "send-keys", &pane.id, "Enter"])
            .status()
            .unwrap()
            .success()
    );
}
