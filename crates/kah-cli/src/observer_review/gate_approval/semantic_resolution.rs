//! Non-admitted status/scope work stays with the same observer after the one round.
use super::*;
use crate::observer_review::{source_store, state::Store};
use kah_observer::{
    decision_units::ObligationKind, review_evidence::Document, verification_contract::Method,
};
use sha2::{Digest, Sha256};
use std::{collections::BTreeSet, path::PathBuf};

pub(super) fn semantic(report: &Report, id: &str) -> bool {
    if let Some(kind) = report.decision_units.as_ref().and_then(|units| {
        units
            .requirements
            .iter()
            .find(|r| r.id == id)
            .and_then(|r| r.obligation.as_ref())
            .map(|o| &o.kind)
    }) {
        return *kind == ObligationKind::NonExecutable;
    }
    report.verification_contracts.as_ref().is_some_and(|items| {
        items.iter().any(|item| {
            item.requirement_id == id && matches!(item.method, Method::SemanticRisk { .. })
        })
    })
}

fn unresolved(report: &Report, assessment: &Value) -> BTreeSet<String> {
    assessment["needs_thinker"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .filter(|id| {
            semantic(report, id)
                && assessment["contract_coverage"][*id]["semantic_risk_admissible"] != true
        })
        .map(str::to_owned)
        .collect()
}

pub(super) fn needs_resolution(report: &Report, assessment: &Value, id: &str) -> bool {
    semantic(report, id) && assessment["contract_coverage"][id]["semantic_risk_admissible"] != true
}

pub(super) fn guard(
    root: &Path,
    request: &Request,
    report: &Report,
    assessment: &mut Value,
) -> Result<bool> {
    if let Some(expected) = request.trigger["semantic_resolution"]["observer_session"].as_str() {
        let state = Store::read(&request.worker)?
            .ok_or("REVIEW_STATE_MISSING stage=semantic_resolution")?;
        if state.observer.session.as_deref() != Some(expected) {
            return Err("REVIEW_GATE_OBSERVER_MISMATCH stage=semantic_resolution".into());
        }
    }
    guard_with(root, request, report, assessment, |snapshot| {
        full_evidence(request, snapshot)
    })
}

pub(super) fn guard_with(
    root: &Path,
    request: &Request,
    report: &Report,
    assessment: &mut Value,
    materialize: impl FnOnce(&mut Request) -> Result<()>,
) -> Result<bool> {
    if request.trigger["verification_followup"]["stage"] != "checks_complete" {
        return Ok(false);
    }
    if request.trigger["semantic_resolution"].is_object() {
        return finish(root, request, report, assessment);
    }
    let ids = unresolved(report, assessment);
    if ids.is_empty()
        || assessment["violations"]
            .as_array()
            .is_none_or(|v| !v.is_empty())
    {
        return Ok(false);
    }
    let record = record(root, request)?;
    let semantic_state = json!({"report":report,"assessment":assessment,"items":ids,
        "routing":"Linked obligations own routing; suggested commands are evidence only for non-executable items."});
    let snapshot = snapshot(root, &record, request, &ids, &semantic_state, materialize)?;
    crate::observer_review::queue::enqueue_once_at(
        root,
        &format!("semantic-resolution:{}", snapshot.id),
        &crate::observer_review::queue::Wake {
            purpose: request.purpose.clone(),
            turn: request.turn.clone(),
            trigger: json!({"captured_snapshot":snapshot,"gate_host_continuation":true}),
        },
    )?;
    remove_checks(assessment, &ids);
    assessment["passed"] = Value::Null;
    assessment["state"] = "checkpoint_needs_thinker".into();
    assessment["semantic_resolution"] = json!({"record":record,"items":ids,"status":"pending"});
    Ok(true)
}

fn snapshot(
    root: &Path,
    record: &Path,
    request: &Request,
    ids: &BTreeSet<String>,
    semantic_state: &Value,
    materialize: impl FnOnce(&mut Request) -> Result<()>,
) -> Result<Request> {
    if let Some(saved) = crate::observer_review::state::read::<Request>(record)? {
        return Ok(saved);
    }
    {
        let mut snapshot = request.clone();
        snapshot.id = format!(
            "review-{}-{}",
            kah_observer::session::now_ms(),
            crate::relay_store::nonce()
        );
        snapshot.trigger.as_object_mut().unwrap().remove("option1");
        snapshot.trigger["reconsideration"] =
            json!({"request":request.id,"cause":"non_admitted_semantic_resolution"});
        snapshot.trigger["semantic_resolution"] =
            json!({"items":ids,"origin":request.id,"same_observer":true,"verification_rounds":1});
        snapshot.documents.retain(|d| {
            !matches!(
                d.id.as_str(),
                "completion-due" | "controller-evidence" | "controller-receipts"
            )
        });
        crate::observer_review::completion_due::add(&mut snapshot)?;
        crate::observer_review::capture::controller_events::add(root, &mut snapshot)?;
        crate::observer_review::capture::controller_receipts::add(&mut snapshot)?;
        materialize(&mut snapshot)?;
        snapshot.documents.push(Document::new(
            "semantic-state",
            "controller_context",
            &format!("kah:semantic-state:{}", request.id),
            &serde_json::to_string(semantic_state)?,
        ));
        snapshot.documents.push(Document::new("semantic-resolution", "controller_context", "kah:semantic-resolution",
            "Resolve the named non-admitted status/scope items from the full run history, complete delivered code and exact-SHA receipts. This is SAME-observer context work, not another verification round. Do not ask the user a status or interpretation question. Return satisfied with exact cited evidence, or a cited finding/unsupported claim. Do not silently pass an unsupported or unconfirmed claim."));
        source_store::save(
            record.parent().unwrap(),
            record.file_name().unwrap().to_str().unwrap(),
            &json!(snapshot),
        )?;
        Ok(snapshot)
    }
}

fn remove_checks(assessment: &mut Value, ids: &BTreeSet<String>) {
    if let Some(items) = assessment["needs_check"].as_array_mut() {
        items.retain(|item| !item["id"].as_str().is_some_and(|id| ids.contains(id)));
    }
}

fn record(root: &Path, request: &Request) -> Result<PathBuf> {
    let identity = serde_json::to_vec(&(
        request.worker.clone(),
        &request.head,
        &request.intent["revision"],
    ))?;
    Ok(root
        .join("semantic-resolutions")
        .join(format!("{:x}.json", Sha256::digest(identity))))
}

fn full_evidence(request: &Request, snapshot: &mut Request) -> Result<()> {
    let state =
        Store::read(&request.worker)?.ok_or("REVIEW_STATE_MISSING stage=semantic_resolution")?;
    snapshot.trigger["semantic_resolution"]["observer_session"] = json!(
        state
            .observer
            .session
            .as_ref()
            .ok_or("REVIEW_GATE_OBSERVER_MISSING stage=semantic_resolution")?
    );
    let head = request
        .head
        .as_deref()
        .ok_or("REVIEW_GATE_HEAD_MISSING stage=semantic_resolution")?;
    let tree = super::checks::planned_candidate(&state, head, request)?;
    append_code(&tree, head, snapshot)?;
    let proof = captured(request)?.ok_or("REVIEW_GATE_PROOF_MISSING stage=semantic_resolution")?;
    snapshot.documents.push(Document::new(
        "semantic-check-receipts",
        "controller_facts",
        "kah:exact-sha-checks",
        &serde_json::to_string(&proof)?,
    ));
    Ok(())
}

pub(super) fn append_code(
    tree: &kah_run::worktree::Worktree,
    head: &str,
    snapshot: &mut Request,
) -> Result<()> {
    for (path, blob) in crate::observer_review::artifact_collect::committed(&tree.path, head)? {
        let bytes = crate::observer_review::artifact_git::git(
            &tree.path,
            &["cat-file", "blob", &blob],
            None,
        )?;
        let body = match String::from_utf8(bytes) {
            Ok(text) => json!({"path":path,"blob":blob,"text":text}),
            Err(error) => {
                json!({"path":path,"blob":blob,"encoding":"bytes","bytes":error.into_bytes()})
            }
        };
        let id = format!("semantic-code-{:x}", Sha256::digest(path.as_bytes()));
        snapshot.documents.push(Document::new(
            &id,
            "committed_source",
            &format!("git:{head}:{path}"),
            &body.to_string(),
        ));
    }
    Ok(())
}

fn finish(root: &Path, request: &Request, report: &Report, assessment: &mut Value) -> Result<bool> {
    let ids = unresolved(report, assessment);
    let violations = assessment["violations"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .filter(|id| semantic(report, id))
        .map(str::to_owned)
        .collect::<BTreeSet<_>>();
    let pending = ids.union(&violations).cloned().collect::<BTreeSet<_>>();
    if pending.is_empty() {
        return Ok(false);
    }
    let text = format!(
        "kah finding on SHA {}: semantic claims {} are unsupported or remain unconfirmed after full-evidence review by the same observer. The gate is not passed. Review {} retains the cited evidence. A new commit is required before another checkpoint.",
        request.head.as_deref().unwrap_or("unknown"),
        pending.iter().cloned().collect::<Vec<_>>().join(", "),
        request.id
    );
    crate::observer_review::reminder_queue::save(
        root,
        &json!({"request":request.id,"purpose":"checkpoint","turn":request.turn,"system_notice":true,"text":text,"head":request.head,"finding":"semantic_unconfirmed","items":pending}),
    )?;
    remove_checks(assessment, &pending);
    assessment["passed"] = false.into();
    assessment["state"] = "checkpoint_semantic_finding".into();
    assessment["semantic_resolution"] = json!({"status":"finding","items":pending});
    Ok(true)
}
