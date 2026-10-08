//! Exact private paths only; receipts contain trust booleans, never auth data.
use serde_json::{Value, json};
use std::{fs, path::Path};
pub fn add(config: &Path, home: &Path, project: &Path, receipts: &Path) -> Result<(), String> {
    let canonical = fs::canonicalize(project).map_err(|e| e.to_string())?;
    let private = fs::canonicalize(home).map_err(|e| e.to_string())?;
    if canonical != project || canonical == private || !canonical.starts_with(&private) {
        return Err("TRACK2_CLAUDE_TRUST_SCOPE stage=trust".into());
    }
    let mut config_value = read(config)?;
    let key = project
        .to_str()
        .ok_or("TRACK2_CLAUDE_TRUST_PATH stage=trust")?;
    let mut recorded = read(receipts)?;
    if recorded.get(key).is_none() {
        recorded[key] = json!({"new_project":config_value["projects"].get(key).is_none(),
            "previous_trust":config_value["projects"][key]["hasTrustDialogAccepted"]});
        write(receipts, &recorded)?;
    }
    if config_value["projects"].is_null() {
        config_value["projects"] = json!({});
    }
    config_value["projects"][key]["hasTrustDialogAccepted"] = true.into();
    write(config, &config_value)
}
pub fn cleanup(config: &Path, receipts: &Path) -> Result<Value, String> {
    let recorded = read(receipts)?;
    if recorded.as_object().is_none_or(|r| r.is_empty()) {
        return Ok(json!({"passed":true,"entries":[]}));
    }
    let mut config_value = read(config)?;
    for (key, previous) in recorded
        .as_object()
        .ok_or("TRACK2_CLAUDE_TRUST_RECEIPT stage=cleanup")?
    {
        if let Some(projects) = config_value["projects"].as_object_mut() {
            if previous["new_project"] == true {
                projects.remove(key);
            } else if let Some(project) = projects.get_mut(key).and_then(Value::as_object_mut) {
                if previous["previous_trust"].is_null() {
                    project.remove("hasTrustDialogAccepted");
                } else {
                    project.insert(
                        "hasTrustDialogAccepted".into(),
                        previous["previous_trust"].clone(),
                    );
                }
            }
        }
    }
    write(config, &config_value)?;
    Ok(
        json!({"passed":true,"entries":recorded.as_object().unwrap().keys().collect::<Vec<_>>(),"added_trust_removed":true}),
    )
}
fn read(path: &Path) -> Result<Value, String> {
    match fs::symlink_metadata(path) {
        Ok(meta) if meta.is_file() => {
            serde_json::from_slice(&fs::read(path).map_err(|e| e.to_string())?)
                .map_err(|e| e.to_string())
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(json!({})),
        _ => Err("TRACK2_CLAUDE_TRUST_FILE stage=trust".into()),
    }
}
fn write(path: &Path, value: &Value) -> Result<(), String> {
    kah_cli::unattended_report::save(
        path.parent().unwrap(),
        path.file_name().unwrap().to_str().unwrap(),
        value,
    )
    .map_err(|e| e.to_string())
}
