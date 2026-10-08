//! Only the copied kah code is shared; native executor binds it read-only.
//! Linux executable permissions must allow its mapped sandbox UID, as the
//! operator's installed 0755 binary does. Never widen login/settings files.
use std::{fs, os::unix::fs::PermissionsExt, path::Path};
pub fn install(source: &Path, target: &Path) {
    fs::copy(source, target).unwrap();
    fs::set_permissions(target, fs::Permissions::from_mode(0o755)).unwrap();
}
