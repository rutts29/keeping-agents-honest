//! Intake/sync boundary. Native inference belongs to the existing controller.
use crate::session::Binding;
use kah_core::{
    current_rules::{RuleSet, Source, ValidatedSet},
    run_store::{RuleSync, RunStore},
};
use serde_json::json;
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
};
pub mod intake;

pub fn sources(binding: &Binding) -> Result<Vec<Source>, String> {
    let mut sources = standing_sources(&binding.project)?;
    sources.extend(intake::sources(binding)?);
    Ok(sources)
}

pub fn stage(binding: &Binding) -> Result<Option<RuleSync>, String> {
    let base = RunStore::default_state_root().map_err(|e| e.to_string())?;
    let Some(state) = crate::session::Store::read(&base, &binding.session)? else {
        return Ok(None);
    };
    let Some(run) = state.run_id else {
        return Ok(None);
    };
    let sources = sources(binding)?;
    let store = RunStore::open_default(&binding.project, &run).map_err(|e| e.to_string())?;
    store
        .sync_current_rules(sources, &crate::session::now_ms().to_string())
        .map(Some)
}
/// Re-read History and standing files under the writer lock before adoption.
pub fn accept(binding: &Binding, candidate: &RuleSet) -> Result<ValidatedSet, String> {
    let base = RunStore::default_state_root().map_err(|e| e.to_string())?;
    let state = crate::session::Store::read(&base, &binding.session)?
        .ok_or("RULE_SET_SESSION stage=current_rules")?;
    let run = state.run_id.ok_or("RULE_SET_RUN stage=current_rules")?;
    let sources = sources(binding)?;
    let store = RunStore::open_default(&binding.project, &run).map_err(|e| e.to_string())?;
    store.accept_current_rules(candidate, sources, &crate::session::now_ms().to_string())
}
/// Current means exact live input identity. A stale or conflicting set uses LLM review.
pub fn current(binding: &Binding) -> Result<Option<ValidatedSet>, String> {
    match stage(binding)? {
        Some(RuleSync::Ready(set)) if !set.has_conflicts() => Ok(Some(set)),
        _ => Ok(None),
    }
}
pub fn standing_sources(project: &Path) -> Result<Vec<Source>, String> {
    let mut files = BTreeMap::<PathBuf, (String, Vec<PathBuf>, PathBuf)>::new();
    for scope in project.ancestors().collect::<Vec<_>>().into_iter().rev() {
        for name in ["AGENTS.md", "CLAUDE.md"] {
            snapshot(&scope.join(name), scope, &mut files)?;
        }
    }
    // This project's instruction chain explicitly designates this plan. Keep its
    // historical sections verbatim; the observer accounts for them as context.
    if files
        .values()
        .any(|(text, _, _)| text.contains("docs/PLAN.md"))
    {
        snapshot(&project.join("docs/PLAN.md"), project, &mut files)?;
    }
    Ok(files
        .into_iter()
        .map(|(resolved, (text, aliases, scope))| {
            Source::standing(
                &resolved.to_string_lossy(),
                &text,
                json!({"resolved_path":resolved,"instruction_paths":aliases,"scope":scope}),
            )
        })
        .collect())
}
fn snapshot(
    path: &Path,
    scope: &Path,
    files: &mut BTreeMap<PathBuf, (String, Vec<PathBuf>, PathBuf)>,
) -> Result<(), String> {
    let resolved = match fs::canonicalize(path) {
        Ok(path) => path,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(e) => return Err(format!("RULE_SET_STANDING stage=current_rules: {e}")),
    };
    if let Some((_, aliases, _)) = files.get_mut(&resolved) {
        aliases.push(path.to_path_buf());
    } else {
        let text = fs::read_to_string(&resolved)
            .map_err(|e| format!("RULE_SET_STANDING stage=current_rules: {e}"))?;
        files.insert(
            resolved,
            (text, vec![path.to_path_buf()], scope.to_path_buf()),
        );
    }
    Ok(())
}

#[derive(serde::Serialize)]
#[serde(tag = "route", rename_all = "snake_case")]
pub enum CommitRoute {
    JevRules {
        head: String,
        intent_revision: u64,
        set_digest: String,
        questions: serde_json::Value,
    },
    DirectObserverReview {
        head: String,
        reason: String,
    },
}
/// Binding metadata stays local. The caller renders rule-local words into Jev state.
pub fn for_commit(binding: &Binding, head: &str) -> Result<CommitRoute, String> {
    if head.trim().is_empty() {
        return Err("RULE_SET_BINDING stage=current_rules: commit SHA required".into());
    }
    let set = current(binding)?;
    if let Some(set) = set {
        let questions = serde_json::to_value(set.questions(head)?)
            .map_err(|e| format!("RULE_SET_JSON stage=current_rules: {e}"))?;
        Ok(CommitRoute::JevRules {
            head: head.into(),
            intent_revision: set.revision(),
            set_digest: set.digest()?,
            questions,
        })
    } else {
        Ok(CommitRoute::DirectObserverReview { head:head.into(),reason:"No current validated, conflict-free rule set; same observer reviews the full commit directly".into() })
    }
}
