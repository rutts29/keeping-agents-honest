//! Recorded shell bodies are indivisible evidence. Never extract a test suffix.
use super::*;
use std::collections::BTreeSet;

pub(super) fn dispositions(request: &Request, report: &Report) -> Result<Value> {
    let verified = super::super::super::decisions::verify_evidence(request, report)?
        .ok_or("REVIEW_GATE_CHECK_SOURCE stage=verification")?;
    let mut records = vec![];
    for requirement in report.decision_units.iter().flat_map(|u| &u.requirements) {
        let value = verified["requirements"]
            .as_array()
            .unwrap()
            .iter()
            .find(|v| v["id"] == requirement.id)
            .ok_or("REVIEW_GATE_CHECK_SOURCE stage=verification")?;
        for check in &requirement.checks {
            let native = &value["citations"][check.command_citation]["native_records"];
            let commands = native
                .as_array()
                .into_iter()
                .flatten()
                .flat_map(|r| r["executions"].as_array().into_iter().flatten())
                .filter_map(|e| e["command"].as_str())
                .collect::<BTreeSet<_>>();
            // Multiple cited bodies remain whole evidence too. There is no
            // execution choice to resolve and no suffix to guess.
            let command = if commands.len() == 1 {
                commands.iter().next().copied()
            } else {
                None
            };
            let plan = report
                .verification_contracts
                .iter()
                .flatten()
                .find(|p| p.requirement_id == requirement.id);
            records.push(json!({"requirement_id":requirement.id,"command":command,"native_bodies":commands,
                "status":"not_replayable",
                "reason":"REVIEW_RECORDED_CHECK_NOT_REPLAYABLE stage=verification: native shell body is evidence-only; use the whole v12 plan in the existing round",
                "verification_plan":plan,
                "without_plan":"REVIEW_CHECK_UNVERIFIED stage=verification: no executable plan; route remaining uncertainty to named risk after the single round"}));
        }
    }
    Ok(json!(records))
}
