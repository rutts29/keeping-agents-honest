use kah_sandbox::publish_binary;
use std::{fs, os::unix::fs::PermissionsExt, path::PathBuf};

struct Fixture(PathBuf);
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[test]
fn public_runtime_is_content_addressed_and_detects_cache_tampering() {
    let f = Fixture(std::env::temp_dir().join(format!("kah-runtime-test-{}", std::process::id())));
    fs::create_dir(&f.0).unwrap();
    let source = f.0.join("source");
    fs::write(&source, b"public native executable").unwrap();
    let cache = f.0.join("cache");
    let binary = publish_binary(&source, &cache).unwrap();
    assert_eq!(fs::read(&binary).unwrap(), b"public native executable");
    assert_eq!(
        fs::metadata(&binary).unwrap().permissions().mode() & 0o777,
        0o555
    );
    assert_eq!(publish_binary(&source, &cache).unwrap(), binary);
    fs::set_permissions(&binary, fs::Permissions::from_mode(0o755)).unwrap();
    fs::write(&binary, b"modified").unwrap();
    assert_eq!(
        publish_binary(&source, &cache).unwrap_err().code,
        "RUNTIME_HASH"
    );
}
