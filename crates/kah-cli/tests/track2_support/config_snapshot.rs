//! Strict recursive config inventory; secret values never enter receipts.
use super::digest;
use serde_json::{Value, json};
use std::{fs, os::unix::fs::PermissionsExt, path::Path};

pub fn snapshot(root: &Path) -> Value {
    assert!(
        fs::symlink_metadata(root).unwrap().is_dir(),
        "TRACK2_CONFIG_TYPE stage=audit: root must be a real directory"
    );
    let mut files = vec![];
    collect(root, root, &mut files);
    json!({"files":files})
}
fn collect(root: &Path, path: &Path, files: &mut Vec<Value>) {
    let metadata = fs::symlink_metadata(path).unwrap();
    let mut entry =
        json!({"name":path.strip_prefix(root).unwrap(),"mode":metadata.permissions().mode()});
    if metadata.is_dir() {
        entry["kind"] = "directory".into();
        files.push(entry);
        let mut children = fs::read_dir(path)
            .unwrap()
            .map(|e| e.unwrap().path())
            .collect::<Vec<_>>();
        children.sort();
        for child in children {
            collect(root, &child, files);
        }
    } else if metadata.is_file() {
        entry["kind"] = "file".into();
        entry["sha256"] = digest(path).into();
        files.push(entry);
    } else if metadata.file_type().is_symlink() {
        entry["kind"] = "symlink".into();
        entry["target"] = json!(fs::read_link(path).unwrap());
        files.push(entry);
    } else {
        panic!("TRACK2_CONFIG_TYPE stage=audit: unsupported file type");
    }
}
