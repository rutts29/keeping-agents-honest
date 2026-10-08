//! Metadata only: no file contents, credential digests or symlink traversal.
use serde_json::{Value, json};
use std::{fs, os::unix::fs::MetadataExt, path::Path};
pub fn snapshot(root: &Path) -> Value {
    let mut rows = vec![];
    let mut errors = vec![];
    walk(root, root, &mut rows, &mut errors);
    json!({"entries":rows,"errors":errors,"metadata_only":true})
}
fn walk(path: &Path, root: &Path, rows: &mut Vec<Value>, errors: &mut Vec<Value>) {
    let metadata = match fs::symlink_metadata(path) {
        Ok(value) => value,
        Err(error) => {
            if path != root || error.kind() != std::io::ErrorKind::NotFound {
                errors.push(json!({"path":path,"error":error.to_string()}));
            }
            return;
        }
    };
    rows.push(json!({"path":path.strip_prefix(root).unwrap(),"mode":metadata.mode(),"uid":metadata.uid(),"gid":metadata.gid(),
        "device":metadata.dev(),"inode":metadata.ino(),"bytes":metadata.len(),"mtime":[metadata.mtime(),metadata.mtime_nsec()],
        "ctime":[metadata.ctime(),metadata.ctime_nsec()],"directory":metadata.is_dir(),"symlink":metadata.file_type().is_symlink(),"regular_file":metadata.is_file()}));
    if !metadata.is_dir() {
        return;
    }
    let paths = fs::read_dir(path).and_then(|entries| {
        entries
            .map(|e| e.map(|e| e.path()))
            .collect::<std::io::Result<Vec<_>>>()
    });
    match paths {
        Ok(mut paths) => {
            paths.sort();
            for path in paths {
                walk(&path, root, rows, errors);
            }
        }
        Err(error) => errors.push(json!({"path":path,"error":error.to_string()})),
    }
}
pub fn audit(
    homes: &[std::path::PathBuf],
    operator: &Path,
    before: &Value,
    markers: &[String],
    projects: &[std::path::PathBuf],
) -> Value {
    let mut violations = vec![];
    for home in homes {
        let scan = snapshot(home);
        violations.extend(scan["errors"].as_array().unwrap().iter().cloned());
        for entry in scan["entries"].as_array().unwrap() {
            let path = Path::new(entry["path"].as_str().unwrap());
            if path
                .file_name()
                .is_some_and(|s| s == ".credentials.json" || s == "credentials.json")
                || path == Path::new(".claude") && entry["symlink"] == true
            {
                violations.push(json!({"home":home,"entry":entry,"reason":"TRACK2_FIXTURE_CREDENTIAL stage=login_audit"}));
            }
        }
    }
    let after = snapshot(operator);
    let mut references = markers.to_vec();
    for project in projects {
        references.push(project.to_string_lossy().into_owned());
        references.push(kah_claude::paths::project_key(project));
    }
    let attributed = after["entries"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|entry| {
            references.iter().any(|needle| {
                !needle.is_empty() && entry["path"].as_str().unwrap().contains(needle)
            })
        })
        .cloned()
        .collect::<Vec<_>>();
    let regular = after["entries"]
        .as_array()
        .unwrap()
        .iter()
        .any(|e| e["path"] == ".credentials.json" && e["regular_file"] == true);
    json!({"passed":violations.is_empty() && attributed.is_empty() && regular && after["errors"].as_array().unwrap().is_empty(),
        "fixture_claude_credentials_absent":violations.is_empty(),"violations":violations,"fixture_attributed_operator_entries":attributed,
        "operator_credential_regular":regular,"credential_contents_opened":false,"before":before,"after":after,
        "other_metadata_changes_informational":before!=&after,"search_scope":"names and filesystem metadata only; no file contents"})
}
