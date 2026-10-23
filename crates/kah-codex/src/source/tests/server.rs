//! A server-initiated request is not the response to ours.
use super::directory;
use crate::source::server::AppServer;
use serde_json::json;
use std::{
    fs,
    io::BufReader,
    os::unix::fs::PermissionsExt,
    process::{Command, Stdio},
};

fn server(script: &std::path::Path) -> AppServer {
    let mut child = Command::new(script)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("start the fake server");
    let stdin = child.stdin.take().unwrap();
    let stdout = BufReader::new(child.stdout.take().unwrap());
    AppServer {
        child,
        stdin,
        stdout,
        next_request_id: 1,
    }
}

fn run(server_request_id: u64) {
    let parent = directory("server-request");
    let script = parent.join("server.sh");
    let seen = parent.join("reply");
    fs::write(
        &script,
        format!(
            "#!/bin/sh\nread -r request\n\
             printf '%s\\n' '{{\"id\":{server_request_id},\"method\":\"item/commandExecution/requestApproval\",\"params\":{{}}}}'\n\
             read -r reply\nprintf '%s\\n' \"$reply\" > {}\nprintf '%s\\n' '{{\"id\":1,\"result\":{{\"ok\":true}}}}'\nread -r end\n",
            seen.display()
        ),
    )
    .unwrap();
    fs::set_permissions(&script, fs::Permissions::from_mode(0o700)).unwrap();
    let mut server = server(&script);
    let result = server.request("thread/read", json!({})).unwrap();
    let reply: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&seen).unwrap()).unwrap();
    drop(server);
    fs::remove_dir_all(parent).unwrap();
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
    run(99);
}

#[test]
fn a_server_request_with_the_pending_request_id_is_not_taken_for_its_response() {
    run(1);
}
