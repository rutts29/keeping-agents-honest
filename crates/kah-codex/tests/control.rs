use kah_codex::control::Control;
use std::{fs, os::unix::fs::PermissionsExt, path::Path, time::Duration};

#[test]
fn a_closed_native_server_has_a_specific_error() {
    let script = fixture_path("closed");
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
    assert_eq!(error.code, "CODEX_RPC_CLOSED", "{error}");
}

#[test]
fn a_silent_native_server_is_bounded_and_reaped() {
    let script = fixture_path("silent");
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
    assert_eq!(error.code, "CODEX_RPC_TIMEOUT", "{error}");
}

/// The server sends a request of its own before the real result. The client
/// declines it under the server's id and still returns the real result.
fn server_request_then_result(server_request_id: u64) {
    let script = fixture_path("server-request");
    let seen = fixture_path("server-request-reply");
    fs::write(
        &script,
        format!(
            "#!/bin/sh\nread -r init\nprintf '%s\\n' '{{\"id\":1,\"result\":{{}}}}'\nread -r initialized\nread -r request\n\
             printf '%s\\n' '{{\"id\":{server_request_id},\"method\":\"item/commandExecution/requestApproval\",\"params\":{{}}}}'\n\
             read -r reply\nprintf '%s\\n' \"$reply\" > {}\nprintf '%s\\n' '{{\"id\":2,\"result\":{{\"ok\":true}}}}'\nread -r end\n",
            seen.display()
        ),
    )
    .unwrap();
    fs::set_permissions(&script, fs::Permissions::from_mode(0o700)).unwrap();
    let mut control = Control::connect(
        &script,
        Path::new("/tmp"),
        Path::new("/tmp"),
        Duration::from_secs(5),
    )
    .unwrap();
    let result = control
        .request("thread/read", serde_json::json!({}))
        .unwrap();
    let reply: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&seen).unwrap()).unwrap();
    drop(control);
    fs::remove_file(script).unwrap();
    fs::remove_file(seen).unwrap();
    assert_eq!(result["ok"], true);
    assert_eq!(reply["id"], server_request_id);
    assert!(reply["error"]["code"].is_i64(), "{reply}");
    assert!(
        reply.get("result").is_none(),
        "a request is never approved: {reply}"
    );
}

#[test]
fn a_server_request_is_declined_and_the_real_result_still_arrives() {
    server_request_then_result(99);
}

#[test]
fn a_server_request_with_the_pending_request_id_is_not_taken_for_its_response() {
    server_request_then_result(2);
}

fn fixture_path(name: &str) -> std::path::PathBuf {
    let nonce = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    std::env::temp_dir().join(format!("kah-{name}-{}-{nonce}", std::process::id()))
}
