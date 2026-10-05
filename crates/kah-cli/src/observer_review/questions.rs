//! Durable questions scope a wait to their decision, never the whole worker.
use super::state::Request;
use crate::Result;
use kah_observer::{decision_units::Thinking, review_evidence::Report};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{fs, path::Path};
mod answers;
mod lineage;
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
    let identity = serde_json::to_vec(&native::identity(
        &request.worker,
        r,
        question,
        options,
        &source,
    ))?;
    let id = format!("question-{:x}", Sha256::digest(identity));
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
pub fn message(question: &Value) -> Result<String> {
    let options = serde_json::to_string(&question["options"])?;
    Ok(format!(
        "[kah-builder] [kah-question] {} worker={} requirement={}\n{}{}{}\nThinker's summary: {}\nOptions: {}\nExisting ledger checked: {}\nOnly this decision waits. Unrelated authorized work can continue. Relay the user's answer with this question id via kah steer; this is a question, not new user authority.",
        question["id"]
            .as_str()
            .ok_or("REVIEW_QUESTION_ID stage=question")?,
        question["worker"],
        question["requirement"],
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
        options,
        question["ledger_review"]["reasoning"]
            .as_str()
            .unwrap_or("")
    ))
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
            &["agent", "prompt", "kah-reviewer", &message(&question)?],
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
