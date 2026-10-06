//! One verification round per delivered SHA and user-intent revision.
use super::*;
use std::fs;

pub fn existing(root: &Path, request: &Request) -> Result<Option<VerificationRound>> {
    let expected = Binding::from_request(request)?;
    let directory = root.join("gate-followups");
    if !directory.is_dir() {
        return Ok(None);
    }
    let mut matches = vec![];
    for entry in fs::read_dir(directory)? {
        let path = entry?.path();
        if path.extension().is_none_or(|ext| ext != "json") {
            continue;
        }
        let saved: VerificationRound = crate::observer_review::source_store::read(&path)?;
        if saved.binding.worker != expected.worker
            || saved.binding.head != expected.head
            || saved.binding.intent_revision != expected.intent_revision
        {
            continue;
        }
        let round =
            read(root, &saved.binding)?.ok_or("REVIEW_FOLLOWUP_BINDING stage=followup_scope")?;
        if path.file_name() != Some(std::ffi::OsStr::new(&format!("{}.json", round.id))) {
            return Err("REVIEW_FOLLOWUP_BINDING stage=followup_scope: private round filename differs from binding".into());
        }
        matches.push(round);
    }
    // Any legacy duplicates stay archived. The earliest valid original binding
    // owns the scope, including awaiting_thinker and completed stages.
    matches.sort_by(|a, b| a.binding.review.cmp(&b.binding.review));
    Ok(matches.into_iter().next())
}
