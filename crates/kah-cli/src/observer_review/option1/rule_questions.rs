//! Rule conflicts use the existing durable user-decision/reviewer channel.
use super::super::{source_store, state, state::Request};
use crate::Result;
use kah_core::current_rules::{Disposition, RuleSet};
use serde_json::json;
use std::path::Path;
pub fn store(root: &Path, request: &Request, set: &RuleSet) -> Result<()> {
    let directory = root.join("questions");
    kah_core::run_store::ensure_private_directory(&directory)?;
    let _lock = state::Store::delivery_at(&directory, true)?;
    for c in set
        .clauses
        .iter()
        .filter(|c| c.disposition == Disposition::UnresolvedConflict)
    {
        let identity = json!({"worker":request.worker,"words":c.words,"witnesses":c.witnesses});
        let id = format!(
            "question-{}",
            kah_observer::context_selection::digest(&identity)
        );
        if state::read::<serde_json::Value>(&directory.join(format!("{id}.json")))?.is_some() {
            continue;
        }
        let words = std::iter::once(&c.words)
            .chain(c.witnesses.iter())
            .map(|w| w.quote.clone())
            .collect::<Vec<_>>();
        let value = json!({"id":id,"kind":"current_rule_conflict","worker":request.worker,"request":request.id,"head":null,"requirement":format!("rule_conflict:{}",c.id),"intent_revision":request.intent["revision"],"name":format!("Current instruction conflict {}",c.id),"reason":"conflicting_instructions","summary":c.reason,"question":format!("Which instruction should govern this scope? {}",words.join(" / ")),"options":words,"status":"pending","delivery":"queued","question_origin":"observer","at_ms":kah_observer::session::now_ms(),"authority_at_question":super::super::decisions::active_items(&request.intent),"goal_at_question":request.intent["goal"],"source_witnesses":identity,"scope":"Only this intent decision waits; unrelated authorized work continues"});
        source_store::save(&directory, &format!("{id}.json"), &value)?;
    }
    Ok(())
}
pub fn resolve(root: &Path, request: &Request, set: &RuleSet) -> Result<()> {
    if set
        .clauses
        .iter()
        .any(|c| c.disposition == Disposition::UnresolvedConflict)
    {
        return Ok(());
    }
    let input: kah_core::run_store::RuleRebuild =
        serde_json::from_value(request.trigger["option1"]["rebuild"].clone())?;
    let current_goal = input
        .input
        .history
        .iter()
        .rev()
        .find(|r| {
            matches!(
                r.event,
                kah_core::intent_history::Event::RunStarted { .. }
                    | kah_core::intent_history::Event::GoalRevised { .. }
            )
        })
        .map(|r| format!("goal:{}", r.sequence));
    let directory = root.join("questions");
    let _lock = state::Store::delivery_at(&directory, true)?;
    for mut q in super::super::questions::pending(root)?
        .into_iter()
        .filter(|q| q["kind"] == "current_rule_conflict")
    {
        let id = q["id"]
            .as_str()
            .ok_or("RULE_SET_QUESTION stage=current_rules")?;
        let answer = input.input.sources.iter().find(|s| {
            s.origin["kind"] == "history"
                && is_user_answer(s)
                && (!s.id.starts_with("goal:") || current_goal.as_ref() == Some(&s.id))
                && s.origin["sequence"]
                    .as_u64()
                    .is_some_and(|n| n > q["intent_revision"].as_u64().unwrap_or(u64::MAX))
                && s.text.contains(id)
        });
        if let Some(answer) = answer {
            q["status"] = "answered".into();
            q["resolution"] = json!({"request":request.id,"authority":answer,"validated_set":set,"reason":"Explicit newer named user answer; same observer's validated projection resolves the conflict"});
            source_store::save(
                &directory,
                &format!("{}.json", q["id"].as_str().unwrap()),
                &q,
            )?;
        }
    }
    Ok(())
}
/// Input was reconstructed/validated from History before this boundary.
/// Evidence summaries and inactive intents are never user answer authority.
pub fn is_user_answer(source: &kah_core::current_rules::Source) -> bool {
    let event = &source.origin["event"];
    let current = match event["type"].as_str() {
        Some("requirement_added" | "requirement_superseded") => {
            source.origin["current_lifecycle"] == "active"
        }
        Some("goal_revised") => true,
        _ => false,
    };
    let refs = event["item"]["introduced_by"]
        .as_array()
        .or_else(|| event["replacement"]["introduced_by"].as_array())
        .or_else(|| event["goal"]["source_refs"].as_array());
    current && refs.is_some_and(|r| !r.is_empty() && r.iter().all(|s| s["role"] == "user"))
}
