//! Durable questions scope a wait to their decision, never the whole worker.
use super::state::Request;
use crate::Result;
use kah_observer::{decision_units::Thinking, review_evidence::Report};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{fs, path::Path};
mod answers;
pub mod choice;
mod lineage;
mod scope;
pub(super) use scope::matches as question_scope;
mod native;
mod resolve;
pub use answers::queue as queue_answers;
pub use lineage::validate;
pub use resolve::new_user_items;
pub use resolve::resolve;

pub fn store(
    root: &Path,
    request: &Request,
    report: &Report,
    assessment: &Value,
) -> Result<Vec<Value>> {
    let Some(units) = &report.decision_units else {
        return Ok(vec![]);
    };
    let verified = native::verified(request, report)?;
    kah_core::run_store::ensure_private_directory(&root.join("sources"))?;
    let directory = root.join("questions");
    kah_core::run_store::ensure_private_directory(&directory)?;
    let _delivery = super::state::Store::delivery_at(&directory, true)?;
    let mut created = vec![];
    for r in &units.requirements {
        if !assessment["needs_user"]
            .as_array()
            .is_some_and(|a| a.iter().any(|id| id == &r.id))
        {
            continue;
        }
        let target = lineage::target(root, request, r)?;
        if target
            .as_ref()
            .is_some_and(|q| q["kind"] == "checkpoint_approval")
        {
            continue;
        }
        let source = native::source(&verified, r)?;
        let Some(value) = draft(request, r, source)? else {
            continue;
        };
        if let Some(previous) = target {
            if let Some(refined) = lineage::refine(&previous, &value)? {
                super::source_store::save(
                    &directory,
                    &format!("{}.json", previous["id"].as_str().unwrap()),
                    &refined,
                )?;
                created.push(refined);
            }
            continue;
        }
        let path = directory.join(format!("{}.json", value["id"].as_str().unwrap()));
        if super::state::read::<Value>(&path)?.is_some() {
            continue;
        }
        lineage::require_new(root, request, r)?;
        super::source_store::save(
            &directory,
            path.file_name().unwrap().to_str().unwrap(),
            &value,
        )?;
        created.push(value);
    }
    Ok(created)
}
fn draft(
    request: &Request,
    r: &kah_observer::decision_units::Requirement,
    source: Option<Value>,
) -> Result<Option<Value>> {
    let Thinking::NeedsUser {
        reason,
        summary,
        question,
        options,
        ledger_review,
        ..
    } = &r.judgment
    else {
        return Ok(None);
    };
    let options = &distinct_options(options);
    let identity = serde_json::to_vec(&native::identity(
        &request.worker,
        r,
        question,
        options,
        &source,
    ))?;
    let id = kah_core::intent_history::question_id(&Sha256::digest(identity));
    let mut value = json!({"id":id,"worker":request.worker,"requirement":r.id,"name":r.name,"request":request.id,"head":request.head,"intent_revision":request.intent["revision"],"reason":reason,"summary":summary,"question":question,"options":options,"ledger_review":ledger_review,"status":"pending","delivery":"queued","at_ms":kah_observer::session::now_ms(),"scope":"Only this decision waits; unrelated authorized work continues"});
    value["question_origin"] = if source.is_some() {
        "worker"
    } else {
        "observer"
    }
    .into();
    value["worker_question_source"] = json!(source);
    crate::privacy::scrub(&mut value);
    value["authority_at_question"] = json!(super::decisions::active_items(&request.intent));
    value["goal_at_question"] = request.intent["goal"].clone();
    Ok(Some(value))
}
pub fn pending(root: &Path) -> Result<Vec<Value>> {
    let directory = root.join("questions");
    if !directory.exists() {
        return Ok(vec![]);
    };
    let mut paths = fs::read_dir(directory)?
        .map(|e| e.map(|e| e.path()))
        .collect::<std::io::Result<Vec<_>>>()?;
    paths.sort();
    let mut questions = vec![];
    for path in paths
        .into_iter()
        .filter(|p| p.extension().is_some_and(|x| x == "json"))
    {
        if let Some(value) = super::state::read::<Value>(&path)?
            && value["status"] == "pending"
        {
            questions.push(value);
        }
    }
    Ok(questions)
}
/// One option text appears once; whitespace-only differences are the same text.
pub fn distinct_options(options: &[String]) -> Vec<String> {
    let mut seen = std::collections::BTreeSet::new();
    options
        .iter()
        .filter(|o| seen.insert(normalized(o)))
        .cloned()
        .collect()
}
pub fn normalized(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}
pub fn message(question: &Value) -> Result<String> {
    if question["kind"] == "checkpoint_claim" {
        return super::checkpoint_claim::question_message(question);
    }
    if question["kind"] == "checkpoint_approval" {
        return super::gate_approval::message(question);
    }
    let id = question["id"]
        .as_str()
        .ok_or("REVIEW_QUESTION_ID stage=question")?;
    let covered = choice::covered(question);
    Ok(format!(
        "[kah-builder] [kah-question] {} worker={} requirement={}{}\n{}{}{}\nThinker's summary: {}\nOptions:{}\nExisting ledger checked: {}\nOnly this decision waits. Unrelated authorized work can continue. This is a question, not new user authority.\n{}",
        id,
        question["worker"],
        question["requirement"],
        if covered.len() > 1 {
            format!(" covers={}", serde_json::to_string(&covered)?)
        } else {
            String::new()
        },
        if question["lineage"]
            .as_array()
            .is_some_and(|a| !a.is_empty())
        {
            "Refinement of the same active question; prior wording is retained as superseded evidence.\n"
        } else {
            ""
        },
        native::label(question),
        question["question"].as_str().unwrap_or(""),
        question["summary"].as_str().unwrap_or(""),
        numbered(question),
        question["ledger_review"]["reasoning"]
            .as_str()
            .unwrap_or(""),
        relay_command(question, id)
    ))
}
fn numbered(question: &Value) -> String {
    let options = question["options"].as_array().cloned().unwrap_or_default();
    if options.is_empty() {
        return " none".into();
    }
    options
        .iter()
        .enumerate()
        .map(|(i, o)| format!("\n  {}. {}", i + 1, o))
        .collect()
}
/// The exact relay command for this question, with its printed id.
fn relay_command(question: &Value, id: &str) -> String {
    let worker = question["worker"].as_str().unwrap_or("SESSION");
    if question["options"]
        .as_array()
        .is_some_and(|o| !o.is_empty())
    {
        format!(
            "Relay the user's answer with exactly: kah steer {worker} \"<the user's words>\" --question {id} --choose <option number or exact option text>"
        )
    } else {
        format!(
            "Relay the user's answer with exactly: kah steer {worker} \"<the user's words; name {id}>\""
        )
    }
}
pub fn deliver(worker: &str) -> Result<()> {
    let root = super::state::Store::directory(worker)?;
    // Serialize detached deliveries without holding the review or hook lock.
    let _delivery = super::state::Store::delivery_at(&root.join("questions"), true)?;
    let state = super::state::Store::read(worker)?.ok_or("REVIEW_STATE_MISSING stage=question")?;
    let (socket, _) = super::runtime::transport(&state)?;
    for mut question in pending(&root)?
        .into_iter()
        .filter(|q| q["delivery"] != "submitted")
    {
        crate::run_pane::invoke_at(
            &[
                "agent",
                "prompt",
                crate::reviewer_agent::reviewer_agent(),
                &message(&question)?,
            ],
            Some(socket),
        )?;
        question["delivery"] = "submitted".into();
        super::source_store::save(
            &root.join("questions"),
            &format!("{}.json", question["id"].as_str().unwrap()),
            &question,
        )?;
    }
    Ok(())
}
