//! Audit public configuration bytes and absence; never open credential files.
use std::{fs, io::ErrorKind, path::Path};
pub fn snapshot(path: &Path) -> Option<Vec<u8>> {
    match fs::read(path) {
        Ok(bytes) => Some(bytes),
        Err(error) if error.kind() == ErrorKind::NotFound => None,
        Err(error) => panic!("FIXTURE_CONFIG_IO stage=audit: {}: {error}", path.display()),
    }
}
