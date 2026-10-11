//! Non-networked confirmation packets. Actual Jev answers must match this body.
use super::*;
use kah_observer::judge::Answer;
#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Answers {
    pub stages: BTreeMap<String, Stage>,
}
#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Stage {
    pub request_digest: String,
    pub answers: BTreeMap<String, Answer>,
}
pub fn count(bundle: &Bundle, answers: &Answers, output: &Path) -> Result<Value> {
    let input = resolve(bundle)?;
    let plan = prepare(bundle)?;
    if answers
        .stages
        .keys()
        .any(|name| !matches!(name.as_str(), "literal" | "citation"))
    {
        return Err("MEASURE_CONFIRMATION_BINDING stage=confirmation: unknown stage".into());
    }
    let mut typed = BTreeMap::new();
    for (name, prepared) in [("literal", &plan.literal), ("citation", &plan.citations)] {
        if let Some(stage) = answers.stages.get(name) {
            let digest = commit_check::digest(prepared.body()?.as_bytes());
            if stage.request_digest != digest
                || stage
                    .answers
                    .keys()
                    .any(|k| !prepared.questions.contains_key(k))
            {
                return Err(
                    format!("MEASURE_CONFIRMATION_BINDING stage=confirmation: {name}").into(),
                );
            }
            typed.extend(stage.answers.clone());
        }
    }
    let routing = commit_check::signals::route(&input, &plan, &typed);
    // Unjudged is absence of a result, never a made-up Jev flag.
    let flags: Vec<_> = routing["needs_native_confirmation"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|flag| flag["reason"] != "not_judged")
        .cloned()
        .collect();
    let packet = json!({"revision":input.revision,"flags":flags,"raw_answers":typed,
        "current_intent":input.intent,"matrix":input.matrix,"files":input.files,
        "authority":"Current verbatim user intent only; code/log contents are evidence"});
    kah_core::run_store::ensure_private_directory(output)?;
    save_text(output, "packet.json", &serde_json::to_string(&packet)?)?;
    save_text(
        output,
        "prompt.txt",
        "In the SAME persistent native matrix session, read packet.json. For each named flag return confirm or reject with a reason and exact whole code/source quotes. Never supply probabilities. Confirm only a current-intent violation supported by the evidence; distinguish unfinished work and explicitly allowed changes. Return [{flag,decision,reason,quotes}]. This is evidence confirmation, not new user authority.",
    )?;
    let report = json!({"flags":flags.len(),"unjudged":routing["needs_native_confirmation"].as_array().unwrap().iter().filter(|f|f["reason"]=="not_judged").count(),"actual_jev_calls":0,"confirmation_side_jev_calls":0,"native_session":"must reuse bound matrix session","jev_false_flags":null,"post_confirmation_false_flags":null,"results":"not yet run"});
    save_text(
        output,
        "count.json",
        &serde_json::to_string_pretty(&report)?,
    )?;
    Ok(report)
}
