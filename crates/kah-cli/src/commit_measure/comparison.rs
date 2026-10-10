//! Comparison-only evidence scope. No production filtering policy is selected.
//! git/git v2.53.0 Documentation/diff-options.adoc: exact -M/-C and copies-harder.
use super::*;
use std::collections::BTreeSet;
pub fn omitted(repo: &Path, input: &Input) -> Result<BTreeMap<String, Value>> {
    let mut omitted = BTreeMap::new();
    for hunk in input.hunks.values() {
        if hunk.file.starts_with("docs/gates/evidence/") {
            omitted.insert(hunk.file.clone(), json!({"reason":"generated_evidence_data","production_policy":"undecided; comparison only"}));
        }
    }
    let Some(base) = &input.baseline else {
        return Ok(omitted);
    };
    let changes = source::git(
        repo,
        &[
            "diff",
            "--name-status",
            "-z",
            "--find-renames=100%",
            "--find-copies=100%",
            "--find-copies-harder",
            base,
            &input.revision,
            "--",
        ],
    )?;
    exact_copies(&changes, &mut omitted)?;
    Ok(omitted)
}
fn exact_copies(changes: &str, omitted: &mut BTreeMap<String, Value>) -> Result<()> {
    let mut fields = changes.split('\0').filter(|field| !field.is_empty());
    while let Some(status) = fields.next() {
        let first = fields
            .next()
            .ok_or("MEASURE_DIFF_STATUS stage=comparison")?;
        if status.starts_with('C') || status.starts_with('R') {
            let target = fields
                .next()
                .ok_or("MEASURE_DIFF_STATUS stage=comparison")?;
            if matches!(status, "C100" | "R100") {
                let evidence = json!({"reason":"byte_identical_copy_or_move","native_status":status,"source":first,
                    "identity_proof":"Native Git 100% blob identity to parent baseline",
                    "review_provenance":"Previously present; an independent prior-review receipt is not inferred",
                    "production_policy":"undecided; comparison only"});
                omitted.entry(target.into()).or_insert(evidence);
                if status == "R100" {
                    omitted
                        .entry(first.into())
                        .or_insert(json!({"reason":"byte_identical_move_source","target":target}));
                }
            }
        }
    }
    Ok(())
}
pub fn input(bundle: &Bundle, omitted: &BTreeMap<String, Value>) -> Result<Bundle> {
    let mut compared: Bundle = serde_json::from_value(serde_json::to_value(bundle)?)?;
    compared
        .input
        .hunks
        .retain(|_, hunk| !omitted.contains_key(&hunk.file));
    compared
        .input
        .files
        .retain(|file, _| !omitted.contains_key(file));
    let hunks: BTreeSet<_> = compared.input.hunks.keys().cloned().collect();
    for rule in compared.input.rules.values_mut() {
        rule.hunks.retain(|id| hunks.contains(id));
    }
    compared.input.carried["comparison_omissions"] = json!(omitted);
    compared.input.carried["full_input_retained"] = json!(true);
    Ok(compared)
}
pub fn count_both(repo: &Path, bundle: &Bundle, output: &Path) -> Result<Value> {
    kah_core::run_store::ensure_private_directory(output)?;
    let all = count(bundle, &output.join("all_changed_files"))?;
    let omissions = omitted(repo, &resolve(bundle)?)?;
    let comparison = input(bundle, &omissions)?;
    let without = count(
        &comparison,
        &output.join("without_evidence_or_exact_copies"),
    )?;
    let report = json!({"all_changed_files":all,"without_evidence_or_exact_copies":without,
        "comparison_omissions":omissions,"production_policy":"User decision pending; full changed-file input retained"});
    save_text(
        output,
        "comparison.json",
        &serde_json::to_string_pretty(&report)?,
    )?;
    Ok(report)
}
