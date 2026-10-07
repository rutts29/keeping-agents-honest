use super::*;
pub fn job(request: &Request, input: &Value, questions: &Questions, reason: &str) -> Job {
    // Original packet authority stays protected even when today's ledger differs.
    let ids = [&request.intent, &input["intent"], &input["prior_intent"]]
        .into_iter()
        .flat_map(|intent| intent["items"].as_array().into_iter().flatten())
        .filter_map(|item| {
            item["id"]
                .as_str()
                .or_else(|| item.get(0).and_then(Value::as_str))
        })
        .map(str::to_owned)
        .collect::<BTreeSet<_>>();
    Job {
        decision: request.id.clone(),
        worker: request.worker.clone(),
        state: input.clone(),
        questions: questions.clone(),
        items: context_selection::items(input, &ids),
        reason: reason.into(),
    }
}
