//! Retain provider outages without turning them into thinker corrections.
use super::{
    reminder_queue,
    state::{State, Store},
};
use crate::Result;
use kah_observer::judge::availability;
use serde_json::{Value, json};
use std::{env, path::PathBuf};

pub fn payment_required(error: &str) -> bool {
    error.starts_with("JEV_") && error.split_whitespace().any(|v| v == "http_status=402")
}
pub fn cause(error: &str) -> String {
    if availability::outage(error) {
        format!("SERVICE OUTAGE: {} ({error})", message(error))
    } else {
        format!("VALIDATION BLOCKED: {error}")
    }
}
pub fn message(error: &str) -> &'static str {
    if payment_required(error) {
        "Jev unavailable: payment required"
    } else {
        "Jev unavailable: temporary provider outage"
    }
}
pub fn blocked(worker: &str) -> Result<bool> {
    if super::control::config(worker)?.is_none_or(|c| !c.enabled) {
        return Ok(false);
    }
    let home = PathBuf::from(env::var_os("HOME").ok_or("REVIEW_HOME stage=service")?);
    let root = Store::directory(worker)?;
    let _notice_lock = Store::delivery_at(&root.join("service"), true)?;
    let gap: Option<Value> = super::state::read(&root.join("gap.json"))?;
    // Upgrade pre-installation provider gaps; a later recorded success wins.
    let snapshot = availability::snapshot(&home)?;
    if snapshot.as_ref().is_none_or(|v| {
        gap.as_ref()
            .and_then(|g| g["at_ms"].as_u64())
            .is_some_and(|at| at > v["observed_ms"].as_u64().unwrap_or(0))
    }) && gap
        .as_ref()
        .and_then(|g| g["error"].as_str())
        .is_some_and(availability::outage)
    {
        availability::record_error(&home, gap.as_ref().unwrap()["error"].as_str().unwrap())?;
    }
    availability::recovery::check(&home)?;
    let blocked = availability::unavailable(&home)?;
    let prior: Option<Value> = super::state::read(&root.join("service-outage.json"))?;
    if blocked {
        if let Some(state) = Store::read(worker)? {
            notice(
                &state,
                availability::snapshot(&home)?.unwrap(),
                prior.as_ref(),
            )?;
        }
    } else if prior.as_ref().is_some_and(|p| p["blocked"] == true) {
        reminder_queue::save(
            &root,
            &json!({"purpose":"jev-service-recovered","system_notice":true,
            "text":"kah observer: Jev is available again; retained review resumes without a new thinker correction."}),
        )?;
        if gap
            .as_ref()
            .and_then(|g| g["error"].as_str())
            .is_some_and(availability::outage)
        {
            std::fs::remove_file(root.join("gap.json"))?;
        }
        crate::unattended_report::save(
            &root,
            "service-outage.json",
            &json!({"blocked":false,
            "recovered_ms":kah_observer::session::now_ms(),"previous":prior}),
        )?;
    }
    Ok(blocked)
}
fn notice(state: &State, epoch: Value, prior: Option<&Value>) -> Result<()> {
    let root = Store::directory(&state.worker.session)?;
    let request = state.pending.as_ref().map(|p| p.id.as_str());
    if prior
        .is_some_and(|p| p["blocked"] == true && p["epoch"]["observed_ms"] == epoch["observed_ms"])
    {
        return Ok(());
    }
    let text = format!(
        "{}. Worker {} review {} is retained; no thinker correction or new review admission. Jev recovery resumes the retained report.",
        message(epoch["error"].as_str().unwrap_or("http_status=402")),
        state.worker.session,
        request.unwrap_or("queued")
    );
    let gap: Option<Value> = super::state::read(&root.join("gap.json"))?;
    reminder_queue::save(
        &root,
        &json!({"request":request,"purpose":"jev-service-unavailable","system_notice":true,"text":text}),
    )?;
    let sent = state.worker.socket.as_deref().map(|socket| {
        crate::run_pane::invoke_at(
            &[
                "agent",
                "prompt",
                crate::reviewer_agent::reviewer_agent(),
                &format!("[kah-builder] [kah-observer-service] {text}"),
            ],
            Some(socket),
        )
    });
    crate::unattended_report::save(
        &root,
        "service-outage.json",
        &json!({"blocked":true,
        "epoch":epoch,"request":request,"text":text,"original_gap":gap,"reviewer_submitted":sent.as_ref().is_some_and(|r|r.is_ok()),
        "reviewer_error":sent.and_then(|r|r.err()).map(|e|e.to_string())}),
    )?;
    {
        let mut gap = json!({"error":format!("JEV_SERVICE_UNAVAILABLE stage=review_service: {}",epoch["error"].as_str().unwrap_or("http_status=402")),"original_error":gap.as_ref().map(|g|&g["error"]),"request":request});
        gap["status"] = "service_unavailable".into();
        gap["message"] = message(epoch["error"].as_str().unwrap_or("http_status=402")).into();
        gap["origin"] = json!(super::failure::Origin::Host);
        crate::unattended_report::save(&root, "gap.json", &gap)?;
    }
    Ok(())
}
