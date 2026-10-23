use std::{
    fs,
    os::unix::fs::{OpenOptionsExt, PermissionsExt},
    path::Path,
};

pub fn install(actual: &Path, home: &Path) {
    let dir = home.join(".config/kah");
    kah_core::run_store::ensure_private_directory(&dir).unwrap();
    let mut source = fs::File::open(actual.join(".config/kah/typesafe.key")).unwrap();
    let mut target = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(dir.join("typesafe.key"))
        .unwrap();
    // Copy into the mode-0600 fixture file; never include key bytes in evidence.
    std::io::copy(&mut source, &mut target).unwrap();
}

pub fn cleanup(home: &Path) -> serde_json::Value {
    let path = home.join(".config/kah/typesafe.key");
    let metadata = match fs::symlink_metadata(&path) {
        Ok(metadata) => Some(metadata),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
        Err(error) => panic!("FIXTURE_JEV_KEY_METADATA stage=cleanup: {error}"),
    };
    let private_regular = metadata
        .as_ref()
        .is_some_and(|m| m.file_type().is_file() && m.permissions().mode() & 0o777 == 0o600);
    let existed = metadata.is_some();
    if existed {
        fs::remove_file(&path).unwrap();
    }
    let absent = fs::symlink_metadata(&path)
        .is_err_and(|error| error.kind() == std::io::ErrorKind::NotFound);
    serde_json::json!({"path":path,"existed":existed,"private_regular":private_regular,
        "removed":existed && absent,"absent":absent,"passed":absent && (!existed || private_regular)})
}
