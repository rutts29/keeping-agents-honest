//! Lossless measurement bookkeeping stays local, outside decision content.
use super::*;
use kah_observer::commit_check::{Prepared, Query};

pub fn literal(original: &Prepared) -> Result<(Prepared, Value)> {
    let mut hunks = serde_json::Map::new();
    let mut hunk_keys = BTreeMap::new();
    let mut hunk_map = serde_json::Map::new();
    for (index, (id, hunk)) in original.state["diff_hunks"]
        .as_object()
        .unwrap()
        .iter()
        .enumerate()
    {
        let short = format!("h{}", index + 1);
        hunks.insert(short.clone(), hunk.clone());
        hunk_map.insert(short.clone(),json!({"full_hunk_id":id,"file":hunk["file"],"digest":commit_check::digest(hunk.to_string().as_bytes())}));
        hunk_keys.insert(id.clone(), short);
    }
    let mut checks = serde_json::Map::new();
    let mut questions = BTreeMap::new();
    let mut local = serde_json::Map::new();
    for (old_key, check) in original.state["checks"].as_object().unwrap() {
        let id = check["requirement"].as_str().unwrap();
        let (source, clause) = id
            .strip_prefix("user-")
            .and_then(|s| s.split_once("-rule-"))
            .ok_or("MEASURE_RULE_KEY stage=bookkeeping")?;
        let prefix = source
            .get(..8)
            .ok_or("MEASURE_RULE_KEY stage=bookkeeping")?;
        let short = format!("r_{prefix}_{clause}");
        if checks.contains_key(&short) {
            return Err("MEASURE_KEY_COLLISION stage=bookkeeping".into());
        }
        let selected: Vec<_> = check["hunk_ids"]
            .as_array()
            .unwrap()
            .iter()
            .map(|h| hunk_keys[h.as_str().unwrap()].clone())
            .collect();
        checks.insert(short.clone(),json!({"rule":check["rule"],"hunk_ids":selected,"permission_ids":check["permission_ids"]}));
        let old_question = format!("rule.{old_key}");
        let Query::Noul { instructions } = &original.questions[&old_question] else {
            return Err("MEASURE_LITERAL_TYPE stage=bookkeeping".into());
        };
        questions.insert(
            format!("rule.{short}"),
            Query::Noul {
                instructions: instructions.replace(old_key, &short),
            },
        );
        local.insert(short,json!({"full_rule_id":id,"original_question":old_question,"source_ids":check["source_ids"],"rule_origin":check["rule_origin"],"rule_digest":commit_check::digest(check["rule"].as_str().unwrap().as_bytes())}));
    }
    let state = json!({"diff_hunks":hunks,"checks":checks,"current_permissions":original.state["current_permissions"]});
    let map = json!({"rules":local,"hunks":hunk_map,"not_sent_to_jev":true});
    Ok((Prepared { state, questions }, map))
}
pub fn count(bundle: &Bundle, output: &Path) -> Result<Value> {
    let (prepared, map) = literal(&prepare(bundle)?.literal)?;
    kah_core::run_store::ensure_private_directory(output)?;
    save_text(output, "key-map.json", &serde_json::to_string_pretty(&map)?)?;
    let mut requests = vec![];
    let packed = decisive_packing::pack(&prepared, bundle.calibration.as_ref().unwrap())?;
    for (index, part) in packed.iter().enumerate() {
        let body = part.body()?;
        let state_bytes = serde_json::to_vec(&part.state)?.len();
        let longest = part
            .questions
            .values()
            .map(|q| match q {
                Query::Noul { instructions } => instructions.len(),
                _ => 0,
            })
            .max()
            .unwrap_or(0);
        let calibration = bundle.calibration.as_ref().unwrap();
        let state_estimate = calibration.estimate((state_bytes + longest) as u64);
        let estimate = calibration.estimate(body.len() as u64);
        let predicted_preparation = state_estimate["empirical_input_tokens"].as_f64().unwrap()
            > 32000.0
            || estimate["empirical_input_tokens"].as_f64().unwrap() > 64000.0;
        save_text(output, &format!("literal-{index}.json"), &body)?;
        requests.push(json!({"index":index,"questions":part.questions.len(),"digest":commit_check::digest(body.as_bytes()),"state_plus_longest_estimate":state_estimate,"estimate":estimate,"predicted_preparation":predicted_preparation}));
    }
    let report = json!({"requests":requests,"actual_jev_calls":0,"packing":"Whole commit when estimated to fit; otherwise deterministic first-fit decreasing whole files, no split/trim. Empirical token planning, not a tokenizer or guaranteed provider acceptance.","compaction":"Only predicted over-provider-budget bodies need SAME native observer whole-item preparation; prediction is not a content cap or tokenizer"});
    save_text(
        output,
        "count.json",
        &serde_json::to_string_pretty(&report)?,
    )?;
    Ok(report)
}
