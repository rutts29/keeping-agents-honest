//! A later validated interpretation must cite new user authority to resolve a question.
use crate::Result;
use crate::observer_review::state::Request;
use kah_observer::{
    decision_units::{Thinking, confirmed},
    judge::Judgment,
    review_evidence::Report,
};
use serde_json::{Value, json};
use std::path::Path;

pub fn resolve(
    root: &Path,
    request: &Request,
    report: &Report,
    judgment: &Judgment,
) -> Result<usize> {
    let Some(units) = &report.decision_units else {
        return Ok(0);
    };
    let _delivery = super::super::state::Store::delivery_at(&root.join("questions"), true)?;
    let mut resolved = 0;
    for mut question in super::pending(root)? {
        if question["kind"] == "checkpoint_approval" {
            continue;
        }
        let Some(unit) = units
            .requirements
            .iter()
            .find(|r| question["requirement"] == r.id)
        else {
            continue;
        };
        let choice = match unit.judgment {
            Thinking::Fact { .. } | Thinking::AtomicFact { .. } => "yes",
            Thinking::Semantic { .. } => "supported",
            Thinking::NeedsUser { .. } => continue,
            Thinking::ArtifactPreservation { .. } => continue,
            Thinking::ControllerFact { .. } => continue,
        };
        if !confirmed(judgment, &format!("requirement.{}", unit.id), choice) {
            continue;
        }
        let authority = new_authority(&question, request, unit);
        let Some(authority) = authority else {
            continue;
        };
        question["status"] = "answered".into();
        question["resolution"] = json!({"request":request.id,"authority":authority,"requirement":unit,"judgment":judgment,"at_ms":kah_observer::session::now_ms()});
        super::super::source_store::save(
            &root.join("questions"),
            &format!("{}.json", question["id"].as_str().unwrap()),
            &question,
        )?;
        resolved += 1;
    }
    Ok(resolved)
}
fn new_authority(
    question: &Value,
    request: &Request,
    unit: &kah_observer::decision_units::Requirement,
) -> Option<Value> {
    new_user_items(question, &request.intent)
        .into_iter()
        .find(|item| {
            let Some(id) = item["id"].as_str() else {
                return false;
            };
            unit.authority_items.iter().any(|x| x == id)
                && item["text"].as_str().is_some_and(|text| {
                    unit.citations.iter().any(|c| {
                        c.source == "ledger" && !c.quote.is_empty() && text.contains(&c.quote)
                    })
                })
        })
}
pub fn new_user_items(question: &Value, intent: &Value) -> Vec<Value> {
    let mut items = intent["items"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|item| {
            let Some(id) = item["id"].as_str() else {
                return false;
            };
            !question["authority_at_question"]
                .as_array()
                .into_iter()
                .flatten()
                .any(|x| x == id)
                && user_origin(item)
        })
        .cloned()
        .collect::<Vec<_>>();
    let goal = json!({"id":"goal","text":intent["goal"],"source":intent["goal_source"]});
    if intent["goal"] != question["goal_at_question"] && user_origin(&goal) {
        items.push(goal);
    }
    items
}
fn user_origin(item: &Value) -> bool {
    item["source"].as_array().is_some_and(|sources| {
        sources
            .iter()
            .any(|s| s["role"] == "user" && (s["origin"].is_null() || s["origin"] == "relayed"))
    })
}
