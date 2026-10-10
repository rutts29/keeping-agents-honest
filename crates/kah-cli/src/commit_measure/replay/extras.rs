//! Retained native evidence, never recast as new native work or Jev answers.
use super::*;
use kah_observer::commit_check::{Authority, CodeFile, Hunk, Rule};
pub(super) fn coverage(repo: &Path, output: &Path) -> Result<Vec<Value>> {
    let home = std::env::var_os("HOME").ok_or("MEASURE_HOME stage=corpus")?;
    let root = std::path::PathBuf::from(home).join(".local/share/kah-eval/coverage-decisions");
    let mut archives = fs::read_dir(root)?
        .map(|e| e.map(|e| e.path()))
        .collect::<std::io::Result<Vec<_>>>()?;
    archives.sort();
    let archive = archives
        .first()
        .ok_or("MEASURE_COVERAGE_ARCHIVE stage=corpus")?;
    let mut cases = vec![];
    for id in [
        "normalize",
        "serialize",
        "pipeline",
        "verification_assertions",
        "trivial-passing-command",
    ] {
        let original: Value =
            serde_json::from_slice(&fs::read(archive.join(format!("{id}.json")))?)?;
        let state = &original["state"];
        let requirement = state["decision"]["requirements"]
            .as_object()
            .and_then(|m| m.values().next())
            .ok_or("MEASURE_COVERAGE_REQUIREMENT stage=corpus")?;
        let goal = state["intent"]["goal"]
            .as_str()
            .ok_or("MEASURE_COVERAGE_GOAL stage=corpus")?;
        let mut bundle = snapshot(
            goal,
            id,
            "coverage-native-evidence",
            state,
            requirement["name"].as_str().unwrap_or(id),
        )?;
        bundle.input.code_changed = true;
        bundle.matrix_source = "retained_v12_reasoning; matrix quote adaptation pending".into();
        let head = state["event"]["head"].as_str().unwrap_or("retained-native");
        bundle.input.revision = head.into();
        bundle.input.files = super::code::coverage(state, head)?;
        bundle.calibration = Some(super::calibration()?);
        let row = super::record(
            output,
            &format!("coverage-{id}"),
            "frozen_coverage",
            json!({"expected":if id == "trivial-passing-command" {"not_supported"} else {"satisfied"},"source":archive.join(format!("{id}.json")),"retained_state_digest":commit_check::digest(serde_json::to_vec(state)?.as_slice())}),
            &bundle,
        )?;
        fs::write(
            output.join(format!("coverage-{id}/retained.json")),
            serde_json::to_vec(&original)?,
        )?;
        cases.push(row);
    }
    let _ = repo;
    Ok(cases)
}
pub(super) fn drift(repo: &Path, output: &Path) -> Result<Vec<Value>> {
    let mut cases = vec![];
    for provider in ["codex", "claude"] {
        let path = repo.join(format!(
            "docs/gates/evidence/phase-5o-b/verified-drift-{provider}.json"
        ));
        let original: Value = serde_json::from_slice(&fs::read(&path)?)?;
        let state = &original["fullVerifier"]["jev_state"];
        let goal = state["intent"]["goal"]
            .as_str()
            .ok_or("MEASURE_DRIFT_GOAL stage=corpus")?;
        let mut bundle = snapshot(
            goal,
            "finite_numbers",
            &format!("planted-{provider}"),
            state,
            goal,
        )?;
        bundle.input.files = super::code::drift(&original, &bundle.input.revision)?;
        bundle.calibration = Some(super::calibration()?);
        let row = super::record(
            output,
            &format!("drift-{provider}"),
            "planted_drift",
            json!({"requirement":"finite_numbers","expected":"violated","source":path,"fault_injection":original["faultInjection"]}),
            &bundle,
        )?;
        fs::write(
            output.join(format!("drift-{provider}/retained.json")),
            serde_json::to_vec(&original)?,
        )?;
        cases.push(row);
    }
    Ok(cases)
}
fn snapshot(goal: &str, id: &str, source: &str, state: &Value, name: &str) -> Result<Bundle> {
    let history = corpus::history(goal, "recorded-user-goal", source)?;
    let mut intent = commit_check::CurrentIntent::resolve(&history, BTreeMap::new())?;
    // Named decomposition remains evidence, not newly invented user authority.
    let authority = Authority {
        text: name.into(),
        origin: "quality".into(),
        sources: json!({"retained_source":source,"authority":"recorded-user-goal","name_is_decomposition":true}),
    };
    intent.items.insert(id.into(), authority.clone());
    let revision = format!(
        "retained:{}",
        commit_check::digest(serde_json::to_vec(state)?.as_slice())
    );
    let text = serde_json::to_string_pretty(state)?;
    Ok(Bundle {
        records: history.records().to_vec(),
        quality: BTreeMap::from([(id.into(), authority)]),
        input: Input {
            revision: revision.clone(),
            baseline: None,
            checkpoint: false,
            code_changed: true,
            intent,
            impacted: BTreeSet::from([id.into()]),
            rules: BTreeMap::from([(
                id.into(),
                Rule {
                    authority: id.into(),
                    unconditional_prohibition: false,
                    prohibition: false,
                    literal_check: false,
                    hunks: BTreeSet::from(["retained-actions".into()]),
                },
            )]),
            hunks: BTreeMap::from([(
                "retained-actions".into(),
                Hunk {
                    file: "retained-native-state.json".into(),
                    text: text.clone(),
                },
            )]),
            files: BTreeMap::from([(
                "retained-native-state.json".into(),
                CodeFile::new(&revision, &text),
            )]),
            matrix: vec![],
            carried: json!({"original_state":source,"not_a_new_commit":true}),
        },
        matrix_source: "fresh_native_required_from_retained_evidence".into(),
        native_usage: None,
        calibration: None,
    })
}
