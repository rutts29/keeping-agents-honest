//! Retained native evidence, never recast as new native work or Jev answers.
use super::*;
use kah_core::intent_history::{Event, GoalRevision, History, IntentItem, IntentKind, SourceRef};
use kah_observer::commit_check::{CodeFile, Hunk, Rule};
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
        let mut bundle = snapshot(state)?;
        bundle.input.code_changed = true;
        bundle.matrix_source = "retained_v12_reasoning; matrix quote adaptation pending".into();
        let head = state["event"]["head"].as_str().unwrap_or("retained-native");
        bundle.input.revision = head.into();
        bundle.input.files = super::code::coverage(state, head)?;
        code_only(&mut bundle);
        bundle.calibration = Some(super::calibration()?);
        let row = super::record(
            repo,
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
        let mut bundle = snapshot(state)?;
        bundle.input.files = super::code::drift(&original, &bundle.input.revision)?;
        code_only(&mut bundle);
        bundle.calibration = Some(super::calibration()?);
        let row = super::record(
            repo,
            output,
            &format!("drift-{provider}"),
            "planted_drift",
            json!({"expected":"violated","source":path,"fault_injection":original["faultInjection"]}),
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
fn snapshot(state: &Value) -> Result<Bundle> {
    let history = retained_history(state)?;
    let intent = commit_check::CurrentIntent::resolve(&history, BTreeMap::new())?;
    let impacted = intent.items.keys().cloned().collect();
    let rules = intent
        .items
        .keys()
        .map(|id| {
            (
                id.clone(),
                Rule {
                    authority: id.clone(),
                    unconditional_prohibition: false,
                    prohibition: false,
                    literal_check: false,
                    hunks: BTreeSet::from(["retained-actions".into()]),
                },
            )
        })
        .collect();
    let revision = format!(
        "retained:{}",
        commit_check::digest(serde_json::to_vec(state)?.as_slice())
    );
    let text = serde_json::to_string_pretty(state)?;
    Ok(Bundle {
        historical: None,
        records: history.records().to_vec(),
        quality: BTreeMap::new(),
        input: Input {
            revision: revision.clone(),
            baseline: None,
            checkpoint: false,
            code_changed: true,
            intent,
            impacted,
            rules,
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
            carried: json!({"original_state_digest":commit_check::digest(text.as_bytes()),"not_a_new_commit":true,"authority":"All verbatim active archived intent items, with original native SourceRefs; no label or decomposition rule injected"}),
        },
        matrix_source: "fresh_native_required_from_retained_evidence".into(),
        native_usage: None,
        calibration: None,
    })
}

fn retained_history(state: &Value) -> Result<History> {
    let goal = state["intent"]["goal"]
        .as_str()
        .ok_or("MEASURE_RETAINED_GOAL stage=corpus")?;
    let items = state["intent"]["items"]
        .as_array()
        .ok_or("MEASURE_RETAINED_ITEMS stage=corpus")?;
    let goal_item = items
        .iter()
        .find(|i| i["text"] == goal)
        .ok_or("MEASURE_RETAINED_GOAL_SOURCE stage=corpus")?;
    let sources: Vec<SourceRef> = serde_json::from_value(goal_item["source"].clone())?;
    let mut history = History::new(GoalRevision::new(1, goal, sources))?;
    for item in items {
        let id = item["id"]
            .as_str()
            .ok_or("MEASURE_RETAINED_ITEM_ID stage=corpus")?;
        let text = item["text"]
            .as_str()
            .ok_or("MEASURE_RETAINED_ITEM_TEXT stage=corpus")?;
        let source = serde_json::from_value(item["source"].clone())?;
        // The initial goal is an Outcome, as production intake types
        // initial_task/goal_change; other stripped categories need typing.
        if item["text"] != goal && item.get("kind").is_none() {
            return Err(format!("MEASURE_RETAINED_TYPING_REQUIRED stage=corpus: {id}").into());
        }
        history.append(Event::RequirementAdded {
            item: IntentItem::new(
                id,
                if item.get("kind").is_some() {
                    serde_json::from_value(item["kind"].clone())?
                } else {
                    IntentKind::Outcome
                },
                text,
                source,
            ),
            approval: None,
        })?;
    }
    Ok(history)
}

fn code_only(bundle: &mut Bundle) {
    bundle.input.hunks = bundle
        .input
        .files
        .iter()
        .map(|(path, file)| {
            (
                commit_check::key(path),
                Hunk {
                    file: path.clone(),
                    text: file.text.clone(),
                },
            )
        })
        .collect();
    let keys: BTreeSet<_> = bundle.input.hunks.keys().cloned().collect();
    for rule in bundle.input.rules.values_mut() {
        rule.hunks = keys.clone();
    }
    // Original reports/answers remain archived for audit, outside thinker input.
}
