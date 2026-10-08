//! Class and ancestry select records; only selected originals are decoded.
use super::*;
pub fn claim_only(wake: &Wake) -> bool {
    wake.purpose == Purpose::Checkpoint && claim_trigger(&wake.trigger)
}
pub fn claim_trigger(trigger: &Value) -> bool {
    trigger.get("verification_followup").is_none()
        && trigger["events"].as_array().is_some_and(|events| {
            !events.is_empty() && events.iter().all(|e| e["kind"] == "native_phase_claim")
        })
}
pub(super) fn compatible(incoming: &Wake, old: &Wake) -> bool {
    !protected(&incoming.trigger)
        && !protected(&old.trigger)
        && incoming.trigger.get("verification_followup").is_none()
        && old.purpose == Purpose::Checkpoint
        && old.trigger.get("verification_followup").is_none()
        && (incoming.trigger["review_class"] == "commit" && old.trigger["review_class"] == "commit"
            || claim_only(incoming) && claim_only(old))
}
pub(super) fn selected(
    project: &Path,
    incoming: &Wake,
    headers: &[(PathBuf, Wake)],
    active: Option<&str>,
) -> Result<Vec<(PathBuf, Wake)>> {
    let mut result = vec![];
    let new = head(incoming)?;
    for (path, header) in headers {
        if !compatible(incoming, header) {
            continue;
        }
        let Ok(old) = head(header) else { continue };
        if active != Some(old) && ancestor(project, old, new)? {
            result.push((path.clone(), super::super::super::source_store::read(path)?));
        }
    }
    Ok(result)
}
pub fn trigger_head(trigger: &Value) -> Option<&str> {
    if let Some(head) = trigger["head"].as_str() {
        return Some(head);
    }
    if !claim_trigger(trigger) {
        return None;
    }
    let events = trigger["events"].as_array()?;
    let head = events.first()?["head"].as_str()?;
    events.iter().all(|e| e["head"] == head).then_some(head)
}

fn protected(trigger: &Value) -> bool {
    trigger["events"].as_array().is_some_and(|events| {
        events.iter().any(|e| {
            matches!(
                e["kind"].as_str(),
                Some("native_git_tag" | "user_gate" | "run_completion")
            )
        })
    })
}
