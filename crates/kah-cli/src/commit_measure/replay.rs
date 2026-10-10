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
        let mut bundle = corpus::real_commit(repo, sha, &labels)?;
        bundle.calibration = Some(calibration.clone());
        cases.push(record(output, &id, "real_positive", row.clone(), &bundle)?);
    }
    let controls = corpus::controls(repo, &labels)?;
    for sha in &controls {
        let id = format!("control-{sha}");
        let mut bundle = corpus::real_commit(repo, sha, &labels)?;
        bundle.calibration = Some(calibration.clone());
        cases.push(record(
            output,
            &id,
            "normal_control",
            json!({"commit":sha,"expected":"no violated row; satisfied or unclear"}),
            &bundle,
        )?);
    }
    cases.extend(extras::coverage(repo, output)?);
    cases.extend(extras::drift(repo, output)?);
    let report = json!({"mode":"count_only","cases":cases,"controls":controls,
        "sampling":"SHA-sorted, evenly spaced first-parent main commits Oct2–8, exclusions from reviewer labels; at least4x real positives",
        "calibration":calibration,"actual_jev_calls":0,"actual_jev_cost_usd":0.0,"false_flags":null,"misses":null,
        "matrix_accuracy":null,"quality":"not_run; missing native matrices are explicit, no synthetic predictions",
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
                let target = case["label"]["requirement"].as_str();
                let caught = rows.iter().any(|r| {
                    r["status"] == "violated" && target.is_none_or(|id| r["requirement"] == id)
                });
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
