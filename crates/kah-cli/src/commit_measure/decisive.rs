//! Decisive measurement: frozen source-derived rules, neutral SHA-only cases.
//! The plant manifest is not an argument and is never opened by this pipeline.
use super::*;
use atomization::{AtomicRule, Item};
use kah_core::intent_history::{
    Event, GoalRevision, History, IntentItem, IntentKind, SourceProvider, SourceRef, SourceRole,
};
use kah_observer::commit_check::Rule;
use std::collections::BTreeSet;

pub fn checkpoint(
    repo: &Path,
    base: &str,
    head: &str,
    items: &Path,
    rules: &Path,
    output: &Path,
) -> Result<()> {
    let items: Vec<Item> = serde_json::from_slice(&fs::read(items)?)?;
    let rules: Vec<AtomicRule> = serde_json::from_slice(&fs::read(rules)?)?;
    atomization::verify(&items, &rules)?;
    let mut bundle = bundle(
        repo,
        head,
        &rules,
        &history(&items, &rules)?,
        &replay::calibration()?,
    )?;
    let base = source::git(repo, &["rev-parse", "--verify", base])?
        .trim()
        .to_owned();
    let (hunks, files) = collect(repo, &base, &bundle.input.revision)?;
    bundle.input.rules = selected(&rules, &hunks.keys().cloned().collect());
    bundle.input.hunks = hunks;
    bundle.input.files = files;
    bundle.input.baseline = Some(base);
    bundle.input.checkpoint = true;
    kah_core::run_store::ensure_private_directory(output)?;
    save_text(
        output,
        "checkpoint-input.json",
        &serde_json::to_string(&resolve(&bundle)?)?,
    )
}

pub fn construct(
    repo: &Path,
    items_path: &Path,
    rules_path: &Path,
    cases_path: &Path,
    output: &Path,
) -> Result<()> {
    let items: Vec<Item> = serde_json::from_slice(&fs::read(items_path)?)?;
    let rules: Vec<AtomicRule> = serde_json::from_slice(&fs::read(rules_path)?)?;
    atomization::verify(&items, &rules)?;
    let cases: Vec<String> = serde_json::from_slice(&fs::read(cases_path)?)?;
    let history = history(&items, &rules)?;
    let calibration = replay::calibration()?;
    kah_core::run_store::ensure_private_directory(output)?;
    let mut measured = vec![];
    for sha in cases {
        let bundle = bundle(repo, &sha, &rules, &history, &calibration)?;
        let head = bundle.input.revision.clone();
        let id = format!("case-{}", &head[..12]);
        let dir = output.join(&id).join("all_changed_files");
        let count = super::count(&bundle, &dir)?;
        let whole = decisive_bookkeeping::count(&bundle, &dir.join("whole-hunks"))?;
        measured.push(json!({"id":id,"sha":head,"directory":output.join(&id),"measurement":count,"whole_hunks":whole}));
    }
    save_text(
        output,
        "report.json",
        &serde_json::to_string_pretty(
            &json!({"cases":measured,"calibration":calibration,"actual_jev_calls":0,"labels":"not read","matrix":"pending same persistent native session"}),
        )?,
    )?;
    Ok(())
}
fn bundle(
    repo: &Path,
    sha: &str,
    rules: &[AtomicRule],
    history: &History,
    calibration: &estimate::Calibration,
) -> Result<Bundle> {
    let head = source::git(repo, &["rev-parse", "--verify", sha])?
        .trim()
        .to_owned();
    let parent = source::git(repo, &["rev-parse", "--verify", &format!("{head}^1")])?
        .trim()
        .to_owned();
    let (hunks, files) = collect(repo, &parent, &head)?;
    let permissions = rules
        .iter()
        .filter(|r| r.diff_checkable)
        .map(|r| {
            (
                r.rule_id.clone(),
                BTreeSet::from(["permissions-context".to_owned()]),
            )
        })
        .collect();
    let intent = commit_check::CurrentIntent::resolve(history, permissions)?;
    let hunk_ids: BTreeSet<_> = hunks.keys().cloned().collect();
    let selected = selected(rules, &hunk_ids);
    Ok(Bundle {
        records: history.records().to_vec(),
        historical: None,
        quality: BTreeMap::new(),
        input: Input {
            revision: head.clone(),
            baseline: Some(parent),
            checkpoint: false,
            code_changed: !hunks.is_empty(),
            intent,
            impacted: selected.keys().cloned().collect(),
            rules: selected,
            hunks,
            files,
            matrix: vec![],
            carried: json!({"source":"source-only rule extraction","not_diff_checkable":rules.iter().filter(|r|!r.diff_checkable).map(|r| &r.rule_id).collect::<Vec<_>>(),"selection":"All frozen diff-checkable rules; no label-derived component mapping"}),
        },
        matrix_source: "fresh_native_required_same_session".into(),
        native_usage: None,
        calibration: Some(calibration.clone()),
    })
}
fn selected(rules: &[AtomicRule], hunk_ids: &BTreeSet<String>) -> BTreeMap<String, Rule> {
    rules
        .iter()
        .filter(|r| r.diff_checkable)
        .map(|r| {
            (
                r.rule_id.clone(),
                Rule {
                    authority: r.rule_id.clone(),
                    unconditional_prohibition: true,
                    prohibition: true,
                    literal_check: true,
                    hunks: hunk_ids.clone(),
                },
            )
        })
        .collect()
}
fn history(items: &[Item], rules: &[AtomicRule]) -> Result<History> {
    let source = SourceRef::new(
        SourceProvider::Bundle,
        "decisive-source-only",
        "revision28",
        SourceRole::User,
        commit_check::digest(&serde_json::to_vec(items)?),
    );
    let mut history = History::new(GoalRevision::new(
        1,
        "Compare the changes against the frozen source-derived rules.",
        vec![source.clone()],
    ))?;
    // One lossless permission context retains the exact original items and IDs.
    // It is context, not a newly granted permission or a merged atomic rule.
    history.append(Event::RequirementAdded {
        item: IntentItem::new(
            "permissions-context",
            IntentKind::Scope,
            serde_json::to_string(items)?,
            vec![source.clone()],
        ),
        approval: None,
    })?;
    for rule in rules.iter().filter(|r| r.diff_checkable) {
        let item = items
            .iter()
            .find(|i| i.id == rule.source_intent_id)
            .unwrap();
        let link = SourceRef::new(
            SourceProvider::Bundle,
            "decisive-source-only",
            &item.id,
            SourceRole::User,
            commit_check::digest(item.text.as_bytes()),
        );
        history.append(Event::RequirementAdded {
            item: IntentItem::new(
                rule.rule_id.as_str(),
                rule.kind.clone(),
                &rule.rule_sentence,
                vec![link],
            ),
            approval: None,
        })?;
    }
    Ok(history)
}
