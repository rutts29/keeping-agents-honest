//! Native Codex session identity for Herdr; no agent-state override or delegation.
use serde_json::{Value, json};
use std::{
    env,
    io::{Read, Write},
    os::unix::net::UnixStream,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

/// Mirrors Herdr's installed Codex v8 session protocol. Screen detection still
/// owns lifecycle state. Child sessions must not replace their parent's pane ID.
pub fn session_report(
    input: &Value,
    pane: &str,
    inherited: Option<&str>,
    seq: u64,
) -> Option<Value> {
    if input["hook_event_name"] != "SessionStart" {
        return None;
    }
    let id = input["session_id"].as_str().filter(|s| !s.is_empty())?;
    input["transcript_path"]
        .as_str()
        .filter(|s| !s.is_empty())?;
    if inherited.is_some_and(|parent| parent != id) {
        return None;
    }
    let mut params = json!({"pane_id":pane,"source":"herdr:codex","agent":"codex","seq":seq,"agent_session_id":id});
    if let Some(source) = input["source"].as_str() {
        params["session_start_source"] = source.into();
    }
    Some(
        json!({"id":format!("kah-session-{seq}"),"method":"pane.report_agent_session","params":params}),
    )
}

/// Best-effort and bounded by both socket timeouts and the native hook timeout.
pub fn report() {
    if env::var("HERDR_ENV").as_deref() != Ok("1") {
        return;
    }
    let (Ok(pane), Ok(socket)) = (env::var("HERDR_PANE_ID"), env::var("HERDR_SOCKET_PATH")) else {
        return;
    };
    let input: Value = match serde_json::from_reader(std::io::stdin().take(1024 * 1024)) {
        Ok(v) => v,
        Err(_) => return,
    };
    let inherited = env::var("CODEX_THREAD_ID").ok();
    let seq = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos()
        .min(u64::MAX as u128) as u64;
    let Some(report) = session_report(&input, &pane, inherited.as_deref(), seq) else {
        return;
    };
    if let Ok(mut stream) = UnixStream::connect(socket) {
        let timeout = Some(Duration::from_millis(500));
        let _ = stream.set_write_timeout(timeout);
        let _ = stream.set_read_timeout(timeout);
        let _ = writeln!(stream, "{report}");
        let mut response = [0; 4096];
        let _ = stream.read(&mut response);
    }
}
