//! Standalone packets keep their individual original turn/state/questions.
use super::*;
use crate::observer_review::{preparation, state::Request};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
pub fn group(
    entries: &[(std::path::PathBuf, crate::observer_review::queue::Wake)],
) -> Option<(crate::observer_review::queue::Wake, Vec<std::path::PathBuf>)> {
    let first = entries.first()?;
    if first.1.purpose != crate::observer_review::state::Purpose::Preparation
        || first.1.trigger.get("captured_snapshot").is_some()
    {
        return None;
    }
    let selected = entries
        .iter()
        .filter(|(_, wake)| {
            wake.purpose == crate::observer_review::state::Purpose::Preparation
                && wake.trigger["preparation"]["purpose"]
                    == first.1.trigger["preparation"]["purpose"]
                && wake.trigger.get("captured_snapshot").is_none()
        })
        .collect::<Vec<_>>();
    if selected.is_empty() {
        return None;
    }
    let packets=selected.iter().map(|(path,wake)|json!({"source":path.file_name().unwrap(),"turn":wake.turn,"trigger":wake.trigger})).collect::<Vec<_>>();
    let mut wake = first.1.clone();
    wake.trigger["preparation_packets"] = json!(packets);
    Some((
        wake,
        selected.iter().map(|(path, _)| path.clone()).collect(),
    ))
}
pub fn stage_packets(request: &Request) -> Result<bool> {
    let Some(packets) = request.trigger["preparation_packets"].as_array() else {
        return Ok(false);
    };
    let mut jobs = vec![];
    for (index, packet) in packets.iter().enumerate() {
        jobs.push(job_for_packet(request, index, packet)?);
    }
    stage(&request.worker, &request.id, &jobs)?;
    Ok(true)
}
pub fn packets(request: &Request) -> Option<&Vec<Value>> {
    request.trigger["preparation_packets"].as_array()
}

pub fn job_for_packet(request: &Request, index: usize, packet: &Value) -> Result<Job> {
    let input = &packet["trigger"]["preparation"];
    let questions = serde_json::from_value(input["questions"].clone())?;
    let mut local = request.clone();
    local.id = format!(
        "{}-packet-{:x}",
        request.id,
        Sha256::digest(serde_json::to_vec(&(index, packet))?)
    );
    local.turn = packet["turn"]
        .as_str()
        .ok_or("PREPARATION_PACKET_TURN stage=context_preparation")?
        .into();
    local.intent = input["state"]["intent"].clone();
    Ok(preparation::job(
        &local,
        &input["state"],
        &questions,
        input["reason"]
            .as_str()
            .unwrap_or("original context preparation"),
    ))
}
