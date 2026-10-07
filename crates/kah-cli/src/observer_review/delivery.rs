//! Herdr submits a wake; the native transcript, not a CLI acknowledgement,
//! proves acceptance. A single Enter retry is allowed only in that composer.
use super::{
    runtime,
    state::{State, Store},
};
use crate::Result;
use serde_json::Value;
use std::{
    fs::File,
    io::{BufRead, BufReader, Seek, SeekFrom},
    time::{Duration, Instant},
};

pub fn warm(state: &State, prompt: &str) -> Result<()> {
    let (socket, _) = runtime::transport(state)?;
    let pane = state
        .observer
        .pane
        .as_deref()
        .ok_or("REVIEW_PANE_ID stage=delivery")?;
    let request = state
        .pending
        .as_ref()
        .ok_or("REVIEW_PENDING_MISSING stage=delivery")?;
    let marker = format!("kah review request {} ", request.id);
    if !wait_idle(state, socket, pane)? {
        return Ok(());
    }
    let view = crate::relay_transport::herdr(socket, &["pane", "get", pane])?;
    if !matches!(
        view["result"]["pane"]["agent_status"].as_str(),
        Some("idle" | "done")
    ) {
        return if accepted(state)? {
            Ok(())
        } else {
            Err("REVIEW_NOT_IDLE stage=delivery".into())
        };
    }
    safe_keyboard(&screen(state)?)?;
    let path = state
        .observer
        .transcript
        .as_ref()
        .ok_or("REVIEW_TRANSCRIPT_MISSING stage=delivery")?;
    let offset = std::fs::metadata(path)?.len();
    crate::run_pane::invoke_at(&["agent", "prompt", pane, prompt], Some(socket))?;
    if wait_receipt(state, &marker, offset)? {
        return Ok(());
    }
    let text = screen(state)?;
    safe_keyboard(&text)?;
    let view = crate::relay_transport::herdr(socket, &["pane", "get", pane])?;
    if matches!(
        view["result"]["pane"]["agent_status"].as_str(),
        Some("idle" | "done")
    ) && composer_contains(&text, &marker, &state.worker.provider)
    {
        crate::run_pane::invoke_at(&["agent", "send-keys", pane, "Enter"], Some(socket))?;
        if wait_receipt(state, &marker, offset)? {
            return Ok(());
        }
    }
    diagnostic(state, &marker, &text)?;
    Err(missing_receipt())
}
fn missing_receipt() -> Box<dyn std::error::Error> {
    "REVIEW_SUBMISSION_UNCONFIRMED stage=delivery: native receipt absent; acknowledge this request's marker in your native turn, or remain idle for host delivery recovery. Host diagnostics are retained separately.".into()
}
#[cfg(test)]
#[path = "../../tests/observer_unit/receipt_guidance.rs"]
mod guidance_tests;
fn wait_idle(state: &State, socket: &std::path::Path, pane: &str) -> Result<bool> {
    let waited = crate::run_pane::invoke_at(
        &[
            "agent",
            "wait",
            pane,
            "--until",
            "idle",
            "--until",
            "done",
            "--timeout",
            "15000",
        ],
        Some(socket),
    );
    readiness(waited, accepted(state)?)
}
/// The short readiness handshake may expire while the previous native turn
/// is still closing. It does not end the review or discard its request.
pub fn idle_wait(result: Result<std::process::Output>) -> Result<std::process::Output> {
    result.map_err(|error| {
        let text = error.to_string();
        if wait_timeout(&text) {
            format!("REVIEW_SUBMISSION_UNCONFIRMED stage=delivery: native observer is not idle yet; request retained; {text}").into()
        } else {
            error
        }
    })
}
pub fn wait_timeout(text: &str) -> bool {
    text.strip_prefix("RUN_HERDR_COMMAND stage=pane: agent wait: ")
        .and_then(|body| serde_json::from_str::<Value>(body.trim()).ok())
        .is_some_and(|body| body["error"]["code"] == "timeout")
}
/// A native user receipt is delivery proof even when Herdr's idle handshake
/// times out. Never manufacture a CLI success response or count that as failure.
pub fn readiness(result: Result<std::process::Output>, confirmed: bool) -> Result<bool> {
    if confirmed && result.as_ref().is_err_and(|e| wait_timeout(&e.to_string())) {
        return Ok(false);
    }
    idle_wait(result).map(|_| true)
}
pub fn accepted(state: &State) -> Result<bool> {
    let current = Store::read(&state.worker.session)?.unwrap_or_else(|| state.clone());
    let Some(pending) = &state.pending else {
        return Ok(false);
    };
    let Some(path) = &current.observer.transcript else {
        return Ok(false);
    };
    let decision = super::preparation::active_decision(&current.worker.session, &pending.id)?;
    let marker = format!("kah review request {decision} ");
    let Ok(file) = File::open(path) else {
        return Ok(false);
    };
    receipt(
        BufReader::new(file),
        &current.worker.provider,
        current.observer.session.as_deref().unwrap_or(""),
        &marker,
    )
}
pub fn in_progress(state: &State) -> Result<bool> {
    let Some(pending) = &state.pending else {
        return Ok(false);
    };
    let (socket, _) = runtime::transport(state)?;
    let Some(pane) = state.observer.pane.as_deref() else {
        return Ok(false);
    };
    let view = crate::relay_transport::herdr(socket, &["pane", "get", pane])?;
    if view["result"]["pane"]["agent_status"] != "working" {
        return Ok(false);
    }
    let decision = super::preparation::active_decision(&state.worker.session, &pending.id)?;
    Ok(
        super::request_context::capture(state, &serde_json::json!({"hook_event_name":"Stop"}))?
            .is_some_and(|context| context.bound_to(&pending.id, &decision)),
    )
}
/// Cold launch is not acceptance. Native SessionStart and the persisted user
/// record prove it; an idle client with no receipt gets the same original wake.
pub fn cold(state: &State, prompt: &str) -> Result<()> {
    let request = state
        .pending
        .as_ref()
        .ok_or("REVIEW_PENDING_MISSING stage=delivery")?
        .id
        .clone();
    let started = Instant::now();
    loop {
        let current =
            Store::read(&state.worker.session)?.ok_or("REVIEW_STATE_MISSING stage=delivery")?;
        if current.observer.session.is_some()
            && current
                .observer
                .transcript
                .as_ref()
                .is_some_and(|p| p.exists())
        {
            let marker = format!("kah review request {request} ");
            if wait_receipt(&current, &marker, 0)? {
                return Ok(());
            }
            diagnostic(&current, &marker, &screen(&current)?)?;
            return warm(&current, prompt);
        }
        if started.elapsed() >= Duration::from_secs(15) {
            diagnostic(
                &current,
                "cold launch: native binding absent",
                &screen(&current)?,
            )?;
            return Err("REVIEW_SUBMISSION_UNCONFIRMED stage=delivery: cold native binding absent; request retained".into());
        }
        runtime::require_active(&state.worker.session)?;
        std::thread::sleep(Duration::from_millis(100));
    }
}
fn diagnostic(state: &State, marker: &str, screen: &str) -> Result<()> {
    let rows = state
        .observer
        .transcript
        .as_ref()
        .and_then(|p| std::fs::read_to_string(p).ok())
        .unwrap_or_default();
    let samples=rows.lines().rev().filter_map(|s|serde_json::from_str::<Value>(s).ok()).map(|r|serde_json::json!({"type":r["type"],"isMeta":r["isMeta"],"sessionId":r["sessionId"],"payloadType":r["payload"]["type"],"role":r["payload"]["role"],"messageRole":r["message"]["role"],"messageContent":r["message"]["content"]})).collect::<Vec<_>>();
    let mut value = serde_json::json!({"marker":marker,"screen":screen,"recentNativeRows":samples});
    crate::privacy::scrub(&mut value);
    let archive =
        std::path::PathBuf::from(std::env::var_os("HOME").ok_or("REVIEW_HOME stage=delivery")?)
            .join(".local/share/kah-eval/delivery");
    kah_core::run_store::ensure_private_directory(&archive)?;
    crate::unattended_report::save(
        &archive,
        &format!("{}.json", state.pending.as_ref().unwrap().id),
        &value,
    )?;
    crate::unattended_report::save(
        &Store::directory(&state.worker.session)?,
        "delivery-gap.json",
        &value,
    )
}
fn wait_receipt(state: &State, marker: &str, offset: u64) -> Result<bool> {
    let deadline = Instant::now() + Duration::from_secs(15);
    loop {
        let current =
            Store::read(&state.worker.session)?.ok_or("REVIEW_STATE_MISSING stage=delivery")?;
        let path = current
            .observer
            .transcript
            .as_ref()
            .ok_or("REVIEW_TRANSCRIPT_MISSING stage=delivery")?;
        let mut file = File::open(path)?;
        let start = if Some(path) == state.observer.transcript.as_ref()
            && file.metadata()?.len() >= offset
        {
            offset
        } else {
            0
        };
        file.seek(SeekFrom::Start(start))?;
        if receipt(
            BufReader::new(file),
            &state.worker.provider,
            state.observer.session.as_deref().unwrap_or(""),
            marker,
        )? {
            return Ok(true);
        }
        if Instant::now() >= deadline {
            return Ok(false);
        }
        runtime::require_active(&state.worker.session)?;
        std::thread::sleep(Duration::from_millis(200));
    }
}
pub fn receipt(reader: impl BufRead, provider: &str, session: &str, marker: &str) -> Result<bool> {
    for line in reader.lines() {
        let Ok(row) = serde_json::from_str::<Value>(&line?) else {
            continue;
        };
        let content = if provider == "claude" {
            if row["type"] != "user"
                || row["isMeta"] == true
                || row["isSidechain"] == true
                || row["sessionId"].as_str().is_some_and(|id| id != session)
            {
                continue;
            }
            &row["message"]["content"]
        } else {
            if row["type"] != "response_item"
                || row["payload"]["type"] != "message"
                || row["payload"]["role"] != "user"
            {
                continue;
            }
            &row["payload"]["content"]
        };
        if content
            .as_str()
            .is_some_and(|s| starts_request(s, provider, marker))
            || content.as_array().is_some_and(|parts| {
                parts.iter().any(|p| {
                    matches!(p["type"].as_str(), Some("text" | "input_text"))
                        && p["text"]
                            .as_str()
                            .is_some_and(|s| starts_request(s, provider, marker))
                })
            })
        {
            return Ok(true);
        }
    }
    Ok(false)
}
fn starts_request(text: &str, provider: &str, marker: &str) -> bool {
    let text = text.trim();
    // Claude persists bracketed pastes in a native wrapper. Only unwrap a
    // leading, complete paste; quoted history elsewhere is not a receipt.
    let text = if provider == "claude" && text.starts_with("<pasted_content id=\"") {
        text.strip_prefix("<pasted_content id=\"")
            .and_then(|paste| paste.split_once("\">\n"))
            .and_then(|(id, body)| {
                body.strip_suffix(format!("</pasted_content id=\"{id}\">").as_str())
            })
            .unwrap_or("")
            .trim_start()
    } else {
        text
    };
    text.starts_with(marker)
}
pub fn safe_keyboard(screen: &str) -> Result<()> {
    let lower = screen.to_ascii_lowercase();
    if ["billing", "credit", "purchase", "upgrade", "payment"]
        .iter()
        .any(|w| lower.contains(w))
    {
        return Err("REVIEW_BILLING_ABORT stage=delivery: no keypress sent".into());
    }
    Ok(())
}
pub fn composer_contains(screen: &str, marker: &str, provider: &str) -> bool {
    let glyph = if provider == "claude" { '❯' } else { '›' };
    screen.rsplit_once(glyph).is_some_and(|(_, tail)| {
        tail.chars()
            .filter(|c| !c.is_whitespace())
            .collect::<String>()
            .contains(
                &marker
                    .chars()
                    .filter(|c| !c.is_whitespace())
                    .collect::<String>(),
            )
    })
}
fn screen(state: &State) -> Result<String> {
    let (socket, _) = runtime::transport(state)?;
    let pane = state
        .observer
        .pane
        .as_deref()
        .ok_or("REVIEW_PANE_ID stage=delivery")?;
    let output = crate::run_pane::invoke_at(
        &[
            "pane", "read", pane, "--source", "recent", "--format", "text",
        ],
        Some(socket),
    )?;
    Ok(String::from_utf8(output.stdout)?)
}
