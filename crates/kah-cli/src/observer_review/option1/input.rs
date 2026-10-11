//! Source-linked predicates plus exact whole-file Git patches; no per-commit matrix.
use crate::Result;
use kah_observer::{
    commit_check::{self, Authority, CodeFile, Hunk, Input, Rule},
    rule_source::Resolved,
};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};

pub fn build(
    history: &kah_core::intent_history::History,
    rules: &Resolved,
    context: &Value,
    checkpoint: bool,
) -> Result<Input> {
    let head = context["head"]
        .as_str()
        .ok_or("COMMIT_HEAD_MISSING stage=input")?;
    let mut intent = commit_check::CurrentIntent::resolve(history, BTreeMap::new())?;
    let authority_digest = kah_observer::context_selection::digest(
        &serde_json::json!({"intent":intent,"rules":rules.source_digest}),
    );
    let (hunks, files) = files(context, head)?;
    let hunk_ids = hunks.keys().cloned().collect::<BTreeSet<_>>();
    let permissions = intent.items.keys().cloned().collect::<BTreeSet<_>>();
    let selected = select(&mut intent, rules, checkpoint, &hunk_ids, &permissions)?;
    let mut input = Input {
        revision: head.into(),
        baseline: context["baseline_tree"].as_str().map(str::to_owned),
        checkpoint,
        code_changed: !hunks.is_empty(),
        intent,
        impacted: selected.keys().cloned().collect(),
        rules: selected,
        hunks,
        files,
        matrix: vec![],
        carried: json!({"authority_digest":authority_digest,"source_digest":rules.source_digest,"non_diff_rules":rules.rules.iter().filter(|r|!r.diff_checkable).collect::<Vec<_>>()}),
    };
    input.carried["intent_revision"] = json!(
        kah_core::projection::project(history, kah_core::projection::ProjectionConfig::default())?
            .revision
    );
    if checkpoint || input.rules.is_empty() {
        completion_requirements(history, &mut input);
    }
    Ok(input)
}
fn completion_requirements(history: &kah_core::intent_history::History, input: &mut Input) {
    let mut ids = history
        .snapshot()
        .intents
        .values()
        .filter(|i| {
            i.is_active()
                && !matches!(
                    i.kind,
                    kah_core::intent_history::IntentKind::Constraint
                        | kah_core::intent_history::IntentKind::Invariant
                )
        })
        .map(|i| i.id.to_string())
        .collect::<Vec<_>>();
    ids.push("goal".into());
    for id in ids {
        input.rules.insert(
            id.clone(),
            Rule {
                authority: id.clone(),
                unconditional_prohibition: false,
                prohibition: false,
                literal_check: false,
                hunks: input.hunks.keys().cloned().collect(),
            },
        );
        input.impacted.insert(id);
    }
}

fn files(
    context: &Value,
    head: &str,
) -> Result<(BTreeMap<String, Hunk>, BTreeMap<String, CodeFile>)> {
    let mut hunks = BTreeMap::new();
    let mut files = BTreeMap::new();
    for item in context["touched_files"]
        .as_array()
        .ok_or("COMMIT_FILES_MISSING stage=input")?
    {
        let path = item["path"]
            .as_str()
            .ok_or("COMMIT_PATH_MISSING stage=input")?;
        let patch = item["patch"]
            .as_str()
            .ok_or("COMMIT_PATCH_MISSING stage=input")?;
        hunks.insert(
            commit_check::key(path),
            Hunk {
                file: path.into(),
                text: patch.into(),
            },
        );
        if item["deleted"] != true {
            let text = item["content"]["text"]
                .as_str()
                .ok_or("COMMIT_BINARY_SOURCE stage=input: native direct review required")?;
            files.insert(path.into(), CodeFile::new(head, text));
        }
    }

    Ok((hunks, files))
}

fn select(
    intent: &mut commit_check::CurrentIntent,
    rules: &Resolved,
    checkpoint: bool,
    hunk_ids: &BTreeSet<String>,
    permissions: &BTreeSet<String>,
) -> Result<BTreeMap<String, Rule>> {
    let mut selected = BTreeMap::new();
    for rule in &rules.rules {
        if !rule.diff_checkable && !checkpoint {
            continue;
        }
        for link in &rule.sources {
            let source = intent
                .items
                .get(&link.source_id)
                .ok_or("COMMIT_RULE_SOURCE_MISSING stage=input")?;
            if link.quote.is_empty() || !source.text.contains(&link.quote) {
                return Err("COMMIT_RULE_SOURCE_QUOTE stage=input".into());
            }
        }
        intent.items.insert(
            rule.id.clone(),
            Authority {
                text: rule.predicate.clone(),
                origin: "user".into(),
                sources: json!(rule.sources),
            },
        );
        intent
            .permissions
            .insert(rule.id.clone(), permissions.clone());
        selected.insert(
            rule.id.clone(),
            Rule {
                authority: rule.id.clone(),
                unconditional_prohibition: true,
                prohibition: true,
                literal_check: rule.diff_checkable,
                hunks: hunk_ids.clone(),
            },
        );
    }

    Ok(selected)
}
