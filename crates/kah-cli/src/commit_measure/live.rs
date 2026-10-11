//! Explicit reviewer-approved replay. Existing questions/bodies remain untouched.
use super::*;
use kah_observer::judge::{self, Answer, jev::HttpClient};
#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Job {
    pub case_id: String,
    pub stage: String,
    pub path: std::path::PathBuf,
    pub digest: String,
}
#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Manifest {
    pub jobs: Vec<Job>,
}
pub fn run(manifest: &Manifest, output: &Path) -> Result<Value> {
    if std::env::var("KAH_PHASE1_JEV_GO").as_deref() != Ok("explicit-replay-approval") {
        return Err("MEASURE_JEV_GO_REQUIRED stage=live".into());
    }
    kah_core::run_store::ensure_private_directory(output)?;
    save_text(output, "manifest.json", &serde_json::to_string(manifest)?)?;
    let home = std::env::var_os("HOME").ok_or("MEASURE_HOME stage=live")?;
    let client = HttpClient::from_home(Path::new(&home))?;
    let mut spend = 0.0;
    let mut records = vec![];
    let mut stop = None;
    for (index, job) in manifest.jobs.iter().enumerate() {
        let record = execute(&client, job, index)?;
        spend += record["cost_from_actual_usage_usd"].as_f64().unwrap_or(0.0);
        save_text(
            output,
            &format!("response-{index}.json"),
            &serde_json::to_string(&record)?,
        )?;
        records.push(record.clone());
        if spend > 0.46 {
            stop = Some("APPROVED_SPEND_STOP");
        } else if record["host_stop"] == true {
            stop = Some("PROVIDER_OR_RESPONSE_FAILURE");
        }
        write_progress(output, &records, spend, stop)?;
        if stop.is_some() {
            break;
        }
    }
    Ok(
        json!({"records":records,"cost_from_actual_usage_usd":spend,"stop":stop,
        "planned_calls":manifest.jobs.len(),"actual_calls":records.len(),"no_retries":true,
        "authority":"Explicit approval for one replay with an actual-spend safeguard; not a content limit"}),
    )
}
fn execute(client: &HttpClient, job: &Job, index: usize) -> Result<Value> {
    let body = fs::read_to_string(&job.path)?;
    if commit_check::digest(body.as_bytes()) != job.digest {
        return Err("MEASURE_BODY_CHANGED stage=live".into());
    }
    let value: Value = serde_json::from_str(&body)?;
    if value["model"] != "jev-1.13.0" {
        return Err("MEASURE_MODEL_CHANGED stage=live".into());
    }
    let started = std::time::Instant::now();
    let response = client.post_raw(&body);
    let mut record = json!({"job":job,"index":index,"elapsed_ms":started.elapsed().as_millis(),
        "request_unchanged":true,"request_bytes":body.len(),"at_ms":kah_observer::session::now_ms()});
    match response {
        Ok(response) => {
            let tokens = response["usage"]["input_tokens"].as_u64();
            record["cost_from_actual_usage_usd"] =
                json!(tokens.map(|n| n as f64 * 0.042 / 1_000_000.0));
            record["host_stop"] = json!(tokens.is_none() || response["model"] != "jev-1.13.0");
            let questions: judge::Questions = serde_json::from_value(value["questions"].clone())?;
            let typed = serde_json::from_value(response["answers"].clone());
            match typed.and_then(|a| {
                judge::validate_response(&questions, &a)
                    .map(|_| a)
                    .map_err(serde::de::Error::custom)
            }) {
                Ok(typed) => record["flags"] = json!(flags(&typed)),
                Err(error) => {
                    record["answer_validation_error"] = json!(error.to_string());
                    record["host_stop"] = json!(true);
                }
            }
            record["response"] = response;
        }
        Err(error) => {
            record["error"] = json!(error.to_string());
            record["provider_rejected"] =
                json!(error.code == "JEV_CONTEXT_LIMIT" || matches!(error.status, Some(400 | 422)));
            record["host_stop"] = json!(record["provider_rejected"] != true);
            record["error_response"] = json!(client.last_error_response());
            record["cost_from_actual_usage_usd"] = Value::Null;
        }
    }
    Ok(record)
}
fn flags(answers: &BTreeMap<String, Answer>) -> Vec<Value> {
    answers.iter().filter_map(|(id,answer)| {
        let flagged = match answer {
            Answer::Noul { noul } => *noul > 0.10,
            Answer::Choice {choice,probabilities,confidence} => choice != "supports" || probabilities.get(choice).is_none_or(|p|*p<0.90) || *confidence<0.85,
            _ => true,
        };
        flagged.then(||json!({"question":id,"answer":answer,"reason":"flag_or_unsure; native confirmation required"}))
    }).collect()
}
fn write_progress(output: &Path, records: &[Value], spend: f64, stop: Option<&str>) -> Result<()> {
    use std::io::Write;
    let mut file = fs::File::create(output.join("progress.json"))?;
    file.write_all(
        serde_json::to_string_pretty(&json!({"records":records,
        "cost_from_actual_usage_usd":spend,"stop":stop}))?
        .as_bytes(),
    )?;
    file.sync_all()?;
    Ok(())
}
