use super::fixture::save;
use serde_json::{Value, json};
use std::{fs, path::Path};
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
