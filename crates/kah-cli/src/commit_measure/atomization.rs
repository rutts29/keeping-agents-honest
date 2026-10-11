//! Decisive Step1: code resolves authority; native thinker atomizes, no judge.
use super::*;
use kah_core::intent_history::IntentKind;
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Item {
    pub id: String,
    pub kind: IntentKind,
    pub text: String,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AtomicRule {
    pub rule_id: String,
    pub source_intent_id: String,
    pub source_quote: String,
    pub rule_sentence: String,
    pub kind: IntentKind,
    pub diff_checkable: bool,
    pub not_diff_checkable_reason: Option<String>,
}
pub fn input(repo: &Path, run: &str, output: &Path) -> Result<Value> {
    let store = kah_core::run_store::RunStore::open_default(repo, run)?;
    let recovered = store.recover()?;
    let snapshot = recovered.history.snapshot();
    let items: Vec<_> = snapshot
        .intents
        .values()
        .filter(|item| {
            item.is_active() && matches!(item.kind, IntentKind::Constraint | IntentKind::Invariant)
        })
        .map(|item| Item {
            id: item.id.to_string(),
            kind: item.kind.clone(),
            text: item.text.clone(),
        })
        .collect();
    kah_core::run_store::ensure_private_directory(output)?;
    let body = serde_json::to_string(&items)?;
    save_text(output, "intent-only.json", &body)?;
    let meta = json!({"run":run,"revision":recovered.projection.revision,"source_store":store.path(),
        "history_digest":commit_check::digest(&serde_json::to_vec(recovered.history.records())?),
        "input_digest":commit_check::digest(body.as_bytes()),"items":items.len(),
        "included":"active canonical Constraint/Invariant only; no projection text compaction",
        "excluded":"goals, tasks, superseded/withdrawn items, evidence/verdicts, labels, plans, result files",
        "pending_events":recovered.pending_delta.len(),"actual_jev_calls":0});
    save_text(
        output,
        "resolution.json",
        &serde_json::to_string_pretty(&meta)?,
    )?;
    Ok(meta)
}
pub fn verify(items: &[Item], rules: &[AtomicRule]) -> Result<Value> {
    let mut ids = std::collections::BTreeSet::new();
    let mut covered: BTreeMap<_, _> = items
        .iter()
        .map(|i| (i.id.clone(), vec![false; i.text.len()]))
        .collect();
    for rule in rules {
        let item = items
            .iter()
            .find(|i| i.id == rule.source_intent_id)
            .ok_or("ATOM_SOURCE_UNKNOWN stage=atomization")?;
        if rule.rule_id.is_empty()
            || !ids.insert(&rule.rule_id)
            || rule.kind != item.kind
            || rule.rule_sentence.trim().is_empty()
            || rule.source_quote.is_empty()
            || (!rule.diff_checkable
                && rule
                    .not_diff_checkable_reason
                    .as_ref()
                    .is_none_or(|r| r.trim().is_empty()))
        {
            return Err(format!("ATOM_RULE_INVALID stage=atomization: {}", rule.rule_id).into());
        }
        let matches: Vec<_> = item.text.match_indices(&rule.source_quote).collect();
        if matches.is_empty() {
            return Err(format!("ATOM_QUOTE_IDENTITY stage=atomization: {}", rule.rule_id).into());
        }
        for (start, quote) in matches {
            covered.get_mut(&item.id).unwrap()[start..start + quote.len()].fill(true);
        }
    }
    for item in items {
        for (start, ch) in item.text.char_indices() {
            if !ch.is_whitespace()
                && covered[&item.id][start..start + ch.len_utf8()].contains(&false)
            {
                return Err(format!(
                    "ATOM_SOURCE_UNCOVERED stage=atomization: {} at {}",
                    item.id, start
                )
                .into());
            }
        }
    }
    Ok(
        json!({"source_items":items.len(),"rules":rules.len(),"diff_checkable":rules.iter().filter(|r|r.diff_checkable).count(),
        "all_nonwhitespace_source_text_accounted":true,"exact_quotes":true,"actual_jev_calls":0,
        "semantic_atomicity":"native output and human review; code does not infer crispness"}),
    )
}
