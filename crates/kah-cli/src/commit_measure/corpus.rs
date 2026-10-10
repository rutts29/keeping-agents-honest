//! Reviewer-labelled real commits; labels never enter the thinker input.
use super::*;
use kah_core::intent_history::{
    Event, GoalRevision, History, IntentItem, IntentKind, SourceProvider, SourceRef, SourceRole,
};
use kah_observer::commit_check::Rule;
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
pub fn real_commit(repo: &Path, sha: &str) -> Result<Bundle> {
    let head = source::git(repo, &["rev-parse", "--verify", sha])?
        .trim()
        .to_owned();
    let snapshot = historical::load(repo, &head)?;
    real_commit_with_snapshot(repo, &head, snapshot)
}
pub fn real_commit_with_snapshot(
    repo: &Path,
    sha: &str,
    snapshot: historical::Snapshot,
) -> Result<Bundle> {
    let head = source::git(repo, &["rev-parse", "--verify", sha])?
        .trim()
        .to_owned();
    let at = source::git(repo, &["show", "-s", "--format=%ct", &head])?
        .trim()
        .parse::<u64>()?
        * 1000;
    if snapshot.audit["commit_at_ms"].as_u64() != Some(at) {
        return Err(
            "MEASURE_HISTORICAL_COMMIT stage=corpus: snapshot belongs to another commit time"
                .into(),
        );
    }
    let parent = source::git(repo, &["rev-parse", "--verify", &format!("{head}^1")])?
        .trim()
        .to_owned();
    let intent = snapshot.intent()?;
    let (hunks, files) = collect(repo, &parent, &head)?;
    // No retrospectively invented component map. All authority is selected
    // when a historical map cannot attribute it; the fallback is explicit.
    let impacted = intent.items.keys().cloned().collect();
    let all_hunks: BTreeSet<String> = hunks.keys().cloned().collect();
    let rules = intent
        .items
        .keys()
        .map(|id| {
            (
                id.clone(),
                Rule {
                    authority: id.clone(),
                    unconditional_prohibition: snapshot.always_checked(id),
                    prohibition: snapshot.always_checked(id),
                    literal_check: snapshot.always_checked(id),
                    hunks: all_hunks.clone(),
                },
            )
        })
        .collect();
    Ok(Bundle {
        records: vec![],
        historical: Some(snapshot),
        quality: BTreeMap::new(),
        input: Input {
            revision: head,
            baseline: Some(parent),
            checkpoint: false,
            code_changed: !hunks.is_empty(),
            intent,
            impacted,
            rules,
            hunks,
            files,
            matrix: vec![],
            carried: json!({"source":"historical ledger","records":[],"reason":"Historical requirement map unavailable; all active authority selected, including all prohibitions. No carry invented.","permission_context":"All active authority supplied; permissions require explicit authorization in original text."}),
        },
        matrix_source: "fresh_native_required".into(),
        native_usage: None,
        calibration: None,
    })
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
