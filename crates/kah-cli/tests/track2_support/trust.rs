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
    let path = home.join(".claude.json");
    assert!(
        project.starts_with(home) && fs::symlink_metadata(&path).unwrap().is_file(),
        "TRACK2_CLAUDE_CONFIG_SCOPE stage=trust"
    );
    let mut config: Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    if config["projects"].is_null() {
        config["projects"] = json!({});
    }
    config["projects"][project.to_str().unwrap()]["hasTrustDialogAccepted"] = true.into();
    save(&path, &config);
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
