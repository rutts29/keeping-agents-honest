//! Reviewer-labelled real commits; labels never enter the thinker input.
use super::*;
use kah_core::intent_history::{
    Event, GoalRevision, History, IntentItem, IntentKind, SourceProvider, SourceRef, SourceRole,
};
use kah_observer::commit_check::{Authority, CurrentIntent, Rule};
use std::collections::BTreeSet;
pub fn history(text: &str, id: &str, origin: &str) -> Result<History> {
    let source = SourceRef::new(
        SourceProvider::Bundle,
        "phase1-retained-corpus",
        origin,
        SourceRole::User,
        commit_check::digest(text.as_bytes()),
    );
    let mut history = History::new(GoalRevision::new(1, text, vec![source.clone()]))?;
    history.append(Event::RequirementAdded {
        item: IntentItem::new(id, IntentKind::Constraint, text, vec![source]),
        approval: None,
    })?;
    Ok(history)
}
pub fn real_commit(repo: &Path, sha: &str, labels: &Value) -> Result<Bundle> {
    let head = source::git(repo, &["rev-parse", "--verify", sha])?
        .trim()
        .to_owned();
    let parent = source::git(repo, &["rev-parse", "--verify", &format!("{head}^1")])?
        .trim()
        .to_owned();
    let (hunks, files) = collect(repo, &parent, &head)?;
    let text = labels["requirements"]["no_caps"]["text"]
        .as_str()
        .ok_or("MEASURE_LABEL_RULE stage=corpus")?;
    let history = history(text, "no_caps", "reviewer-labels:no_caps:user-standing")?;
    let mut intent = CurrentIntent::resolve(&history, BTreeMap::new())?;
    let quality = Authority {
        text: labels["requirements"]["jev_literal_questions"]["text"]
            .as_str()
            .ok_or("MEASURE_LABEL_RULE stage=corpus")?
            .into(),
        origin: "quality".into(),
        sources: json!({"source":"reviewer finding F19; not user authority"}),
    };
    intent
        .items
        .insert("jev_literal_questions".into(), quality.clone());
    let code_changed = hunks
        .values()
        .any(|h| h.file.starts_with("crates/") || h.file.starts_with("demos/"));
    let impacted = impact(repo, &head, &parent)?;
    let rules = rules(&hunks);
    Ok(Bundle {
        records: history.records().to_vec(),
        quality: BTreeMap::from([("jev_literal_questions".into(), quality)]),
        input: Input {
            revision: head,
            baseline: Some(parent),
            checkpoint: false,
            code_changed,
            intent,
            impacted,
            rules,
            hunks,
            files,
            matrix: vec![],
            carried: json!({"source":"step C component impact","records":[],"reason":"Independent labelled case has no eligible prior verdict; no carry invented"}),
        },
        matrix_source: "fresh_native_required".into(),
        native_usage: None,
        calibration: None,
    })
}
fn quality_path(path: &str) -> bool {
    matches!(
        path,
        "crates/kah-cli/src/observer_review/checkpoint.rs"
            | "crates/kah-observer/src/checkpoint.rs"
    ) || path.starts_with("crates/kah-observer/src/checkpoint/")
}
fn impact(repo: &Path, head: &str, parent: &str) -> Result<BTreeSet<String>> {
    use crate::observer_review::component_map::{Component, Map};
    let map = Map {
        version: 1,
        components: vec![Component {
            name: "jev-questions".into(),
            path_globs: vec![
                "crates/kah-cli/src/observer_review/checkpoint.rs".into(),
                "crates/kah-observer/src/checkpoint.rs".into(),
                "crates/kah-observer/src/checkpoint/**".into(),
            ],
            requirements: vec!["jev_literal_questions".into()],
            tests: vec![],
            e2e_flows: vec![],
        }],
    };
    let current = map.collect(repo, head, Some(parent))?;
    Ok(current["components"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|c| c["changed_paths"].as_array().is_some_and(|p| !p.is_empty()))
        .flat_map(|c| c["requirements"].as_array().into_iter().flatten())
        .filter_map(|id| id.as_str().map(str::to_owned))
        .collect())
}
pub fn controls(repo: &Path, labels: &Value) -> Result<Vec<String>> {
    let mut excluded = BTreeSet::new();
    let positives = labels["positives"]
        .as_array()
        .ok_or("MEASURE_LABEL_POSITIVES stage=corpus")?;
    for row in positives {
        for field in ["commit", "fixed_by"] {
            let reference = row[field]
                .as_str()
                .ok_or("MEASURE_LABEL_SHA stage=corpus")?;
            excluded.insert(
                source::git(repo, &["rev-parse", "--verify", reference])?
                    .trim()
                    .to_owned(),
            );
        }
    }
    let commits = source::git(
        repo,
        &[
            "log",
            "--first-parent",
            "--since=2026-10-02T00:00:00Z",
            "--until=2026-10-08T23:59:59Z",
            "--format=%H",
            "HEAD",
        ],
    )?;
    let mut normal: Vec<_> = commits
        .lines()
        .filter(|sha| !excluded.contains(*sha))
        .map(str::to_owned)
        .collect();
    normal.sort();
    let required = positives.len() * 4;
    if normal.len() < required {
        return Err("MEASURE_CONTROLS_MISSING stage=corpus".into());
    }
    // Dataset sampling requested by reviewer: no runtime content/cost bound.
    Ok((0..required)
        .map(|i| normal[i * normal.len() / required].clone())
        .collect())
}

fn rules(hunks: &BTreeMap<String, commit_check::Hunk>) -> BTreeMap<String, Rule> {
    let all_hunks = hunks
        .iter()
        .filter(|(_, h)| h.file.starts_with("crates/") || h.file.starts_with("demos/"))
        .map(|(id, _)| id.clone())
        .collect();
    let quality_hunks = hunks
        .iter()
        .filter(|(_, h)| quality_path(&h.file))
        .map(|(id, _)| id.clone())
        .collect();
    BTreeMap::from([
        (
            "no_caps".into(),
            Rule {
                authority: "no_caps".into(),
                unconditional_prohibition: true,
                prohibition: true,
                literal_check: true,
                hunks: all_hunks,
            },
        ),
        (
            "jev_literal_questions".into(),
            Rule {
                authority: "jev_literal_questions".into(),
                unconditional_prohibition: false,
                prohibition: false,
                literal_check: true,
                hunks: quality_hunks,
            },
        ),
    ])
}
