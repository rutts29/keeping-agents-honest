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
    let selected = select(&mut intent, rules, checkpoint, &hunk_ids)?;
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
/// Native startup can precede intake's first accepted History. Preserve standing
/// words; absence of a ledger is a controller fact, never a fabricated goal.
pub fn without_history(rules: &Resolved, context: &Value) -> Result<Input> {
    let head = context["head"]
        .as_str()
        .ok_or("COMMIT_HEAD_MISSING stage=input")?;
    let (hunks, files) = files(context, head)?;
    let mut intent = commit_check::CurrentIntent {
        goal: String::new(),
        items: BTreeMap::new(),
        retired: BTreeSet::new(),
        permissions: BTreeMap::new(),
        conflicts: json!([]),
    };
    let ids = hunks.keys().cloned().collect();
    let mut selected = select(&mut intent, rules, true, &ids)?;
    let pending = "controller_intent_pending".to_owned();
    intent.items.insert(pending.clone(),Authority {text:"No user intent History is accepted yet; alignment is unconfirmed until intake settles.".into(),origin:"controller_fact".into(),sources:json!([])});
    selected.insert(
        pending.clone(),
        Rule {
            authority: pending,
            unconditional_prohibition: false,
            prohibition: false,
            literal_check: false,
            hunks: ids,
        },
    );
    Ok(Input {
        revision: head.into(),
        baseline: None,
        checkpoint: false,
        code_changed: !hunks.is_empty(),
        intent,
        rules: selected.clone(),
        impacted: selected.keys().cloned().collect(),
        hunks,
        files,
        matrix: vec![],
        carried: json!({"source_digest":rules.source_digest,"intent_revision":null,"accepted_intent":false}),
    })
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
) -> Result<BTreeMap<String, Rule>> {
    let mut selected = BTreeMap::new();
    for rule in &rules.rules {
        if !rule.diff_checkable && !checkpoint {
            continue;
        }
        for link in &rule.sources {
            let source = rules
                .source_words
                .get(&link.source_id)
                .map(String::as_str)
                .or_else(|| intent.items.get(&link.source_id).map(|s| s.text.as_str()))
                .ok_or("COMMIT_RULE_SOURCE_MISSING stage=input")?;
            if link.quote.is_empty() || !source.contains(&link.quote) {
                return Err("COMMIT_RULE_SOURCE_QUOTE stage=input".into());
            }
        }
        let authority = install(intent, rule);
        selected.insert(
            rule.id.clone(),
            Rule {
                authority,
                unconditional_prohibition: true,
                prohibition: true,
                literal_check: rule.diff_checkable,
                hunks: hunk_ids.clone(),
            },
        );
    }

    Ok(selected)
}
fn install(
    intent: &mut commit_check::CurrentIntent,
    rule: &kah_observer::rule_source::Rule,
) -> String {
    let id = format!("derived_rule:{}", rule.id);
    let permissions = rule
        .permissions
        .iter()
        .map(|p| {
            intent.items.insert(
                p.source_id.clone(),
                Authority {
                    text: p.quote.clone(),
                    origin: if p.source_id.starts_with("permission:standing:") {
                        "standing_file"
                    } else {
                        "user"
                    }
                    .into(),
                    sources: json!({"kind":"exact_linked_words","link":p}),
                },
            );
            p.source_id.clone()
        })
        .collect();
    intent.items.insert(
        id.clone(),
        Authority {
            text: if rule.scope.is_empty() {
                rule.predicate.clone()
            } else {
                format!("Scope: {}. {}", rule.scope, rule.predicate)
            },
            origin: "derived_rule".into(),
            sources: json!(rule.sources),
        },
    );
    intent.permissions.insert(id.clone(), permissions);
    id
}
