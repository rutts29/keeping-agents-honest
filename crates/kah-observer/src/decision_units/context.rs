//! V8 projects the thinker's checked selection, never arbitrary byte windows.
//! Each independent decision keeps its whole evidence and original authority.
use serde_json::{Value, json};
use std::collections::BTreeSet;

pub fn decision(input: &Value, key: &str) -> Result<Value, String> {
    let Some((kind, id)) = key.split_once('.') else {
        return special(input, key);
    };
    if !matches!(kind, "requirement" | "applicability" | "blocker") {
        return special(input, key);
    }
    let original = &input["decision"]["requirements"][id];
    if !original.is_object() {
        return Err("REVIEW_DECISION_REFERENCE stage=decision_context".into());
    }
    let field = if kind == "applicability" {
        "applicability"
    } else {
        "judgment"
    };
    let indices = original[field]["evidence_citations"]
        .as_array()
        .ok_or("REVIEW_DECISION_MANIFEST stage=decision_context")?;
    let mut local = original.clone();
    let manifest = select(&mut local, indices, &original[field]["reasoning"])?;
    if kind == "applicability" {
        local.as_object_mut().unwrap().remove("judgment");
        local.as_object_mut().unwrap().remove("blocker");
        local.as_object_mut().unwrap().remove("checks");
    } else {
        local.as_object_mut().unwrap().remove("applicability");
        check_references(&local)?;
    }
    let mut state = json!({"intent":authority(input, &original["authority_items"])? ,
        "event":{"head":input["event"]["head"]},
        "decision":{"requirements":{id:local}},"source_manifest":manifest});
    state["source_manifest"]["decision"] = key.into();
    state["source_manifest"]["other_requirements"] = json!(
        input["decision"]["requirements"]
            .as_object()
            .unwrap()
            .keys()
            .filter(|other| *other != id)
            .collect::<Vec<_>>()
    );
    state["source_manifest"]["other_requirements_reason"] =
        "Each named requirement is independently evaluated with its own complete evidence.".into();
    Ok(state)
}

fn select(local: &mut Value, indices: &[Value], reason: &Value) -> Result<Value, String> {
    let citations = local["citations"]
        .as_array()
        .ok_or("REVIEW_DECISION_CITATIONS stage=decision_context")?;
    let selected = indices
        .iter()
        .map(|i| {
            i.as_u64()
                .map(|n| n as usize)
                .filter(|i| citations.get(*i).is_some())
                .ok_or("REVIEW_DECISION_MANIFEST stage=decision_context")
        })
        .collect::<Result<BTreeSet<_>, _>>()?;
    if selected.is_empty() {
        return Err("REVIEW_DECISION_MANIFEST stage=decision_context".into());
    }
    let excluded = (0..citations.len())
        .filter(|i| !selected.contains(i))
        .collect::<Vec<_>>();
    let checked = selected
        .iter()
        .map(|i| (i.to_string(), citations[*i].clone()))
        .collect::<serde_json::Map<_, _>>();
    let pointers = checked
        .iter()
        .map(|(id, citation)| {
            let mut pointer = citation.clone();
            if let Some(fields) = pointer.as_object_mut() {
                fields.remove("quote");
            }
            (id.clone(), pointer)
        })
        .collect::<serde_json::Map<_, _>>();
    let manifest = json!({"selected_citations":selected,"excluded_citations":excluded,
        "selection_reason":reason,"checked_sources":pointers,
        "indexing":"Citation keys preserve the original report indices; full sources remain in CAS."});
    local["citations"] = Value::Object(checked);
    Ok(manifest)
}

fn check_references(local: &Value) -> Result<(), String> {
    for check in local["checks"].as_array().into_iter().flatten() {
        for field in ["command_citation", "result_citation", "sha_citation"] {
            let index = check[field]
                .as_u64()
                .ok_or("REVIEW_DECISION_CHECK stage=decision_context")?
                .to_string();
            if local["citations"][&index].is_null() {
                return Err("REVIEW_DECISION_CHECK_MANIFEST stage=decision_context: the thinker must select every cited check dependency for this decision".into());
            }
        }
    }
    Ok(())
}

fn authority(input: &Value, ids: &Value) -> Result<Value, String> {
    let ids = ids
        .as_array()
        .ok_or("REVIEW_DECISION_AUTHORITY stage=decision_context")?;
    let items = input["intent"]["items"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    let mut selected = Vec::new();
    for id in ids {
        if id == "goal" {
            continue;
        }
        let item = items
            .iter()
            .find(|item| item["id"] == *id)
            .ok_or("REVIEW_DECISION_AUTHORITY stage=decision_context: linked item is missing")?;
        selected.push(item.clone());
    }
    Ok(json!({"goal":input["intent"]["goal"],"items":selected,
        "revision":input["intent"]["revision"]}))
}

fn special(input: &Value, key: &str) -> Result<Value, String> {
    let mut state = match key {
        "claim_made" => Ok(json!({"event":{"head":input["event"]["head"],
            "trigger":input["event"]["trigger"]},
            "claim_evidence":input["event"]["actions"].as_array().into_iter().flatten()
                .filter(|action| action["evidence"]["topic"] == "claim")
                .map(|action| action["evidence"].clone()).collect::<Vec<_>>()})),
        "proportion" => Ok(json!({"intent":{"goal":input["intent"]["goal"]},
            "decision":{"proportion":input["decision"]["proportion"]}})),
        "steering.proposal" => {
            let mut proposal = input["decision"]["steering"].clone();
            proposal
                .as_object_mut()
                .ok_or("REVIEW_STEERING_STATE stage=decision_context")?
                .remove("newer_conflicts");
            Ok(
                json!({"decision":{"steering":proposal},"prior_intent":input["prior_intent"],
                "event":{"user_text":input["event"]["user_text"]}}),
            )
        }
        _ => conflict(input, key),
    }?;
    state["intent"]["goal"] = input["intent"]["goal"].clone();
    state["source_manifest"] = json!({"decision":key,
        "selection":"Complete citations explicitly selected by the thinker for this named decision; other decisions are assessed separately."});
    Ok(state)
}

fn conflict(input: &Value, key: &str) -> Result<Value, String> {
    let id = key
        .strip_prefix("steering.conflict.")
        .ok_or("REVIEW_DECISION_KEY stage=decision_context")?;
    let conflicts = input["decision"]["steering"]["newer_conflicts"]
        .as_array()
        .ok_or("REVIEW_STEERING_STATE stage=decision_context")?;
    let conflict = conflicts
        .iter()
        .find(|c| c["id"] == id)
        .ok_or("REVIEW_STEERING_CONFLICT stage=decision_context")?;
    Ok(
        json!({"decision":{"steering":{"newer_conflicts":[conflict]}},
        "event":{"user_text":input["event"]["user_text"]}}),
    )
}
