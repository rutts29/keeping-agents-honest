//! Apply the validated thinker operation with original words and provenance.
use crate::{
    judge::Judgment,
    steering_proposal::{self, Kind, Proposal},
};
use kah_core::intent_history::*;

pub fn apply(
    history: &mut History,
    text: &str,
    source: SourceRef,
    proposal: &Proposal,
    judgment: &Judgment,
) -> Result<(), String> {
    if source.role != SourceRole::User
        || proposal.source_record != source.record
        || !steering_proposal::validated(proposal, judgment)
    {
        return Err("OBSERVER_AUTHORITY stage=validated_steering".into());
    }
    if !proposal.directs {
        return Ok(());
    }
    if matches!(proposal.kind, Kind::None | Kind::QuestionOnly) {
        return Err("OBSERVER_TYPING stage=validated_steering".into());
    }
    let id = super::item_id(&source);
    if let Some(item) = history.intent(&id) {
        return if item.text == text && item.introduced_by == [source] {
            Ok(())
        } else {
            Err("OBSERVER_SOURCE_REWRITE stage=validated_steering".into())
        };
    }
    let target = proposal.supersedes.as_deref().unwrap_or("none");
    let selected = history.intent(target).filter(|i| {
        i.is_active() || i.lifecycle == IntentLifecycle::Withdrawn && i.changed_by.contains(&source)
    });
    if target != "none" && selected.is_none() {
        return Err("OBSERVER_TARGET stage=validated_steering".into());
    }
    let protected_goal = proposal.kind != Kind::GoalChange
        && selected.is_some_and(|i| {
            i.introduced_by
                .iter()
                .any(|r| history.goal().source_refs.contains(r))
        });
    let kind = proposal.kind.label();
    let item = IntentItem::new(id, super::intent_kind(kind)?, text, vec![source.clone()]);
    let replacing = matches!(
        proposal.kind,
        Kind::Correction | Kind::ScopeReduction | Kind::GoalChange
    );
    let mut next = history.clone();
    super::apply_operation(
        &mut next,
        item,
        source,
        kind,
        target,
        replacing && !protected_goal,
    )?;
    *history = next;
    Ok(())
}
