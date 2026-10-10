//! Frozen labels stay outside native matrix inputs; every case is retained.
use super::*;
use std::collections::BTreeSet;
mod code;
mod extras;
pub fn run(repo: &Path, labels_path: &Path, output: &Path) -> Result<()> {
    let labels: Value = serde_json::from_slice(&fs::read(labels_path)?)?;
    kah_core::run_store::ensure_private_directory(output)?;
    save_text(
        output,
        "labels.json",
        &serde_json::to_string_pretty(&labels)?,
    )?;
    let calibration = calibration()?;
    let mut cases = vec![];
    for row in labels["positives"]
        .as_array()
        .ok_or("MEASURE_LABELS stage=replay")?
    {
        let sha = row["commit"]
            .as_str()
            .ok_or("MEASURE_LABEL_SHA stage=replay")?;
        let id = format!("real-{sha}");
        cases.push(real_case(
            repo,
            output,
            &id,
            "real_positive",
            row.clone(),
            sha,
            &calibration,
        )?);
    }
    let controls = corpus::controls(repo, &labels)?;
    for sha in &controls {
        let id = format!("control-{sha}");
        cases.push(real_case(
            repo,
            output,
            &id,
            "normal_control",
            json!({"commit":sha,"expected":"no violated row; satisfied or unclear"}),
            sha,
            &calibration,
        )?);
    }
    cases.extend(extras::coverage(repo, output)?);
    cases.extend(extras::drift(repo, output)?);
    let report = json!({"mode":"count_only","cases":cases,"controls":controls,
        "sampling":"SHA-sorted, evenly spaced first-parent main commits Oct2–8, exclusions from reviewer labels; at least4x real positives",
        "calibration":calibration,"actual_jev_calls":0,"actual_jev_cost_usd":0.0,"false_flags":null,"misses":null,
        "matrix_accuracy":null,"jev_false_flags":null,"post_confirmation_false_flags":null,"confirmation":"same persistent native session; actual typed flags required; not run","quality":"not_run; missing native matrices are explicit, no synthetic predictions",
        "cost_is_pass_fail":false,"caps":false});
    save_text(
        output,
        "report.json",
        &serde_json::to_string_pretty(&report)?,
    )?;
    println!(
        "{}",
        json!({"cases":report["cases"].as_array().unwrap().len(),"controls":controls.len(),"actual_jev_calls":0,"output":output})
    );
    Ok(())
}
pub(super) fn record(
    output: &Path,
    id: &str,
    kind: &str,
    label: Value,
    bundle: &Bundle,
) -> Result<Value> {
    let directory = output.join(id);
    let measured = count(bundle, &directory)?;
    Ok(
        json!({"id":id,"kind":kind,"label":label,"measurement":measured,"matrix_source":bundle.matrix_source,"directory":directory}),
    )
}
pub fn summarize(cases: &[Value]) -> Value {
    let mut false_flags = 0;
    let mut misses = 0;
    let mut scored = 0;
    let mut pending = BTreeSet::new();
    for case in cases {
        if case["label_derivability"]["status"] == "not derivable from intent" {
            pending.insert(format!("{}:not_derivable_from_intent", case["id"]));
            continue;
        }
        let Some(rows) = case["matrix_rows"].as_array() else {
            pending.insert(case["id"].clone().to_string());
            continue;
        };
        let violated = rows.iter().any(|r| r["status"] == "violated");
        match case["kind"].as_str() {
            Some("normal_control") => {
                false_flags += usize::from(violated);
                scored += 1;
            }
            Some("real_positive" | "planted_drift") => {
                let caught = rows.iter().any(|row| label_caught(row, &case["label"]));
                misses += usize::from(!caught);
                scored += 1;
            }
            _ => (),
        }
    }
    json!({"matrix_scored_cases":scored,"matrix_false_flags":false_flags,"matrix_misses":misses,
        "pending":pending,"pipeline_false_flags":null,"pipeline_misses":null,"citation_accuracy":null,
        "note":"Matrix-only quality is not the unrun Jev pipeline quality."})
}

pub(super) fn calibration() -> Result<estimate::Calibration> {
    let home = std::env::var_os("HOME").ok_or("MEASURE_HOME stage=estimate")?;
    let root = Path::new(&home).join(".local/share/kah-eval/coverage-decisions");
    let mut dirs = fs::read_dir(root)?
        .map(|e| e.map(|e| e.path()))
        .collect::<std::io::Result<Vec<_>>>()?;
    dirs.sort();
    estimate::Calibration::from_archive(
        dirs.first()
            .ok_or("MEASURE_CALIBRATION_MISSING stage=estimate")?,
    )
}

fn real_case(
    repo: &Path,
    output: &Path,
    id: &str,
    kind: &str,
    label: Value,
    sha: &str,
    calibration: &estimate::Calibration,
) -> Result<Value> {
    let mut bundle = match corpus::real_commit(repo, sha) {
        Ok(bundle) => bundle,
        Err(error)
            if error
                .to_string()
                .starts_with("MEASURE_HISTORICAL_INTENT_MISSING ") =>
        {
            let case = json!({"id":id,"kind":kind,"label":label,"status":"historical_intent_unavailable","reason":error.to_string(),"rule_count":null,"actual_jev_calls":0,"quality":"not derivable from recovered intent; never inject label authority"});
            save_text(
                output,
                &format!("{id}-unavailable.json"),
                &serde_json::to_string_pretty(&case)?,
            )?;
            return Ok(case);
        }
        Err(error) => return Err(error),
    };
    bundle.calibration = Some(calibration.clone());
    let mut case = record(output, id, kind, label.clone(), &bundle)?;
    // Scoring eligibility is recorded AFTER independent request construction.
    // F19 was a reviewer quality finding, not a contemporaneous user rule.
    if label["category"] == "quality" {
        case["label_derivability"] = json!({"status":"not derivable from intent","reason":"Reviewer quality finding; no equivalent literal-question user rule in the retained active ledger. No rule injected."});
    }
    Ok(case)
}

fn label_caught(row: &Value, label: &Value) -> bool {
    if row["status"] != "violated" {
        return false;
    }
    let Some(file) = label["evidence"]["file"].as_str() else {
        return true;
    };
    row["quotes"].as_array().into_iter().flatten().any(|quote| {
        if quote["file"] != file {
            return false;
        }
        let text = quote["text"].as_str().unwrap_or("");
        if let Some(line) = label["evidence"]["added_line"]
            .as_str()
            .or_else(|| label["evidence"]["added_line_contains"].as_str())
        {
            return text.contains(line);
        }
        label["evidence"]["added_lines"]
            .as_array()
            .is_none_or(|lines| {
                lines
                    .iter()
                    .any(|line| line.as_str().is_some_and(|line| text.contains(line)))
            })
    })
}
