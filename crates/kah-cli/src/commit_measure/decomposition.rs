//! Count the existing lossless whole-hunk construction; never send a request.
use super::*;
pub fn count(bundle: &Bundle, output: &Path) -> Result<Value> {
    let plan = prepare(bundle)?;
    kah_core::run_store::ensure_private_directory(output)?;
    let mut requests = vec![];
    for (index, part) in plan.literal.whole_hunks().iter().enumerate() {
        if part.questions.is_empty() {
            continue;
        }
        let body = part.body()?;
        save_text(output, &format!("literal-{index}.json"), &body)?;
        requests.push(json!({"stage":"literal","questions":part.questions.len(),
            "request_utf8_bytes":body.len(),"state_utf8_bytes":serde_json::to_vec(&part.state)?.len(),
            "digest":commit_check::digest(body.as_bytes()),
            "estimate":bundle.calibration.as_ref().map(|c|c.estimate(body.len() as u64))}));
    }
    let report = json!({"mode":"count_only_whole_hunks","revision":plan.revision,
        "requests":requests,"actual_jev_calls":0,"actual_jev_cost_usd":0,
        "note":"Existing Prepared::whole_hunks retains every applicable rule and permission; this measures an alternative, not an estimate-triggered cap or a send-ready claim."});
    save_text(
        output,
        "count.json",
        &serde_json::to_string_pretty(&report)?,
    )?;
    Ok(report)
}
