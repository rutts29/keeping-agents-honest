use kah_codex::control::Control;
use std::{fs, os::unix::fs::PermissionsExt, path::Path, time::Duration};

#[test]
fn a_closed_native_server_has_a_specific_error() {
    let script = std::env::temp_dir().join(format!("kah-closed-{}", std::process::id()));
    fs::write(&script, "#!/bin/sh\nread -r first\n").unwrap();
    fs::set_permissions(&script, fs::Permissions::from_mode(0o700)).unwrap();
    let error = Control::connect(
        &script,
        Path::new("/tmp"),
        Path::new("/tmp"),
        Duration::from_millis(100),
    )
    .err()
    .unwrap();
    fs::remove_file(script).unwrap();
    assert_eq!(error.code, "CODEX_RPC_CLOSED");
}

#[test]
fn a_silent_native_server_is_bounded_and_reaped() {
    let script = std::env::temp_dir().join(format!("kah-silent-{}", std::process::id()));
    fs::write(&script, "#!/bin/sh\nread -r first\nread -r second\n").unwrap();
    fs::set_permissions(&script, fs::Permissions::from_mode(0o700)).unwrap();
    let error = Control::connect(
        &script,
        Path::new("/tmp"),
        Path::new("/tmp"),
        Duration::from_millis(30),
    )
    .err()
    .unwrap();
    fs::remove_file(script).unwrap();
    assert_eq!(error.code, "CODEX_RPC_TIMEOUT");
}
