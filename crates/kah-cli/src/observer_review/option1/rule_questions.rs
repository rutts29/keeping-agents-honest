//! Rule conflicts use the existing durable user-decision/reviewer channel.
use super::super::questions::normalized;
use super::super::{source_store, state, state::Request};
use crate::Result;
use kah_core::current_rules::{Disposition, RuleSet};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;
/// One question per conflict: the same normalized set of source texts gives
/// one question, whichever clause or requirement id reports it.
pub fn store(root: &Path, request: &Request, set: &RuleSet) -> Result<()> {
    let directory = root.join("questions");
    kah_core::run_store::ensure_private_directory(&directory)?;
    let _lock = state::Store::delivery_at(&directory, true)?;
    let mut stored = fold(&directory, &request.worker, None)?;
    for c in set
        .clauses
        .iter()
        .filter(|c| c.disposition == Disposition::UnresolvedConflict)
    {
        let texts = conflict_texts(c);
        let key = texts.keys().cloned().collect::<Vec<_>>();
        let requirement = format!("rule_conflict:{}", c.id);
        if let Some(q) = stored.iter_mut().find(|q| option_key(q) == key) {
            if q["status"] == "pending" && cover(q, &requirement, c) {
                source_store::save(
                    &directory,
                    &format!("{}.json", q["id"].as_str().unwrap()),
                    q,
                )?;
            }
            continue;
        }
        let identity = json!({"worker":request.worker,"source_texts":key});
        let id =
            kah_core::intent_history::question_id(&Sha256::digest(serde_json::to_vec(&identity)?));
        let words = texts.into_values().collect::<Vec<_>>();
        let value = json!({"id":id,"kind":"current_rule_conflict","worker":request.worker,"request":request.id,"head":null,"requirement":requirement,"requirements":[requirement],"intent_revision":request.intent["revision"],"name":format!("Current instruction conflict {}",c.id),"reason":"conflicting_instructions","summary":c.reason,"question":format!("Which instruction should govern this scope? {}",words.join(" / ")),"options":words,"status":"pending","delivery":"queued","question_origin":"observer","at_ms":kah_observer::session::now_ms(),"authority_at_question":super::super::decisions::active_items(&request.intent),"goal_at_question":request.intent["goal"],"source_witnesses":[witnesses(&requirement, c)],"scope":"Only this intent decision waits; unrelated authorized work continues"});
        source_store::save(&directory, &format!("{id}.json"), &value)?;
        stored.push(value);
    }
    Ok(())
}
/// Normalized text -> first original wording, over the clause and its witnesses.
fn conflict_texts(c: &kah_core::current_rules::Clause) -> BTreeMap<String, String> {
    let mut texts = BTreeMap::new();
    for w in std::iter::once(&c.words).chain(&c.witnesses) {
        texts
            .entry(normalized(&w.quote))
            .or_insert_with(|| w.quote.clone());
    }
    texts
}
fn option_key(question: &Value) -> Vec<String> {
    let set = question["options"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .map(normalized)
        .collect::<BTreeSet<_>>();
    set.into_iter().collect()
}
fn witnesses(requirement: &str, c: &kah_core::current_rules::Clause) -> Value {
    json!({"requirement":requirement,"words":c.words,"witnesses":c.witnesses})
}
/// Adds one more requirement id to the question that already asks it.
fn cover(question: &mut Value, requirement: &str, c: &kah_core::current_rules::Clause) -> bool {
    let mut covered = super::super::questions::choice::covered(question);
    if covered.iter().any(|r| r == requirement) {
        return false;
    }
    covered.push(requirement.into());
    question["requirements"] = covered.into();
    if let Some(list) = question["source_witnesses"].as_array_mut() {
        list.push(witnesses(requirement, c));
    }
    true
}
/// Earlier runs stored one question per clause. Pending questions that ask the
/// same conflict fold into one (`prefer` names it, else the first id); the
/// others close as `merged` and their requirement ids move to it. The caller
/// holds the questions delivery lock. Returns the stored questions after folding.
pub fn fold(directory: &Path, worker: &str, prefer: Option<&str>) -> Result<Vec<Value>> {
    let mut stored = conflict_questions(directory, worker)?;
    if let Some(id) = prefer
        && let Some(i) = stored.iter().position(|q| q["id"] == id)
    {
        let preferred = stored.remove(i);
        stored.insert(0, preferred);
    }
    for i in 0..stored.len() {
        if stored[i]["status"] != "pending" {
            continue;
        }
        let key = option_key(&stored[i]);
        let mut changed = false;
        for j in i + 1..stored.len() {
            if stored[j]["status"] != "pending" || option_key(&stored[j]) != key {
                continue;
            }
            let mut covered = super::super::questions::choice::covered(&stored[i]);
            for r in super::super::questions::choice::covered(&stored[j]) {
                if !covered.contains(&r) {
                    covered.push(r);
                }
            }
            stored[i]["requirements"] = covered.into();
            stored[j]["status"] = "merged".into();
            stored[j]["merged_into"] = stored[i]["id"].clone();
            save(directory, &stored[j])?;
            changed = true;
        }
        if changed {
            save(directory, &stored[i])?;
        }
    }
    Ok(stored)
}
fn save(directory: &Path, question: &Value) -> Result<()> {
    let id = question["id"]
        .as_str()
        .ok_or("REVIEW_QUESTION_ID stage=question")?;
    source_store::save(directory, &format!("{id}.json"), question)
}
/// All stored rule-conflict questions of this worker, open or closed.
fn conflict_questions(directory: &Path, worker: &str) -> Result<Vec<Value>> {
    let mut found = vec![];
    for entry in std::fs::read_dir(directory)? {
        let path = entry?.path();
        if path.extension().is_none_or(|x| x != "json") {
            continue;
        }
        if let Some(q) = state::read::<Value>(&path)?
            && q["kind"] == "current_rule_conflict"
            && q["worker"] == worker
        {
            found.push(q);
        }
    }
    found.sort_by(|a, b| a["id"].as_str().cmp(&b["id"].as_str()));
    Ok(found)
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
