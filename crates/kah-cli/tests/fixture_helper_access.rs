//! A fixture helper is code, mounted read-only; credentials stay private.
#[path = "track2_support/helper_binary.rs"]
mod helper_binary;
use std::{fs, os::unix::fs::PermissionsExt};
#[test]
fn copied_code_is_executable_in_gvisor_without_widening_the_login_store() {
    let root = std::env::temp_dir().join(format!(
        "kah-helper-access-{}",
        kah_cli::relay_store::nonce()
    ));
    fs::create_dir_all(&root).unwrap();
    let source = root.join("source");
    let target = root.join("kah");
    let credential = root.join("credentials");
    fs::write(&source, b"ELF-code-only").unwrap();
    fs::set_permissions(&source, fs::Permissions::from_mode(0o700)).unwrap();
    fs::write(&credential, b"dummy-test-data").unwrap();
    fs::set_permissions(&credential, fs::Permissions::from_mode(0o600)).unwrap();
    helper_binary::install(&source, &target);
    assert_eq!(fs::read(target.clone()).unwrap(), b"ELF-code-only");
    assert_eq!(
        fs::metadata(target).unwrap().permissions().mode() & 0o777,
        0o755
    );
    assert_eq!(
        fs::metadata(credential).unwrap().permissions().mode() & 0o777,
        0o600
    );
    fs::remove_dir_all(root).unwrap();
}
