//! Report-path checks on a new rule set: every rule names its kind, and an
//! explicit user instruction governs a conflicting standing-file instruction.
//! Stored sets are not re-checked here, so their digests and replay stay valid.
use crate::Result;
use kah_core::current_rules::{Clause, Disposition, Input, RuleSet, Source};

pub fn check(candidate: &RuleSet, input: &Input) -> Result<()> {
    if candidate.rules.iter().any(|r| r.kind.is_none()) {
        return Err("RULE_SET_KIND stage=current_rules: give every rule kind=prohibition|obligation|per_commit_obligation".into());
    }
    if let Some(c) = candidate
        .clauses
        .iter()
        .find(|c| c.disposition == Disposition::UnresolvedConflict && user_governs(c, input))
    {
        return Err(format!("RULE_SET_PRECEDENCE stage=current_rules: clause {} is a conflict between explicit user words and a standing file only. The user instruction governs: mark the standing clause superseded with the user words as witness, state the precedence in reason, and link the user words to a current rule. Do not ask a question for this conflict.", c.id).into());
    }
    Ok(())
}
/// True when the conflict words come only from explicit user instructions and
/// standing files, with at least one of each.
pub fn user_governs(clause: &Clause, input: &Input) -> bool {
    let (mut user, mut standing) = (false, false);
    let goal = input
        .sources
        .iter()
        .rev()
        .find(|s| s.id.starts_with("goal:"));
    for words in std::iter::once(&clause.words).chain(&clause.witnesses) {
        match input.sources.iter().find(|s| s.id == words.source) {
            Some(s) if s.origin["kind"] == "standing_file" => standing = true,
            Some(s) if user_instruction(s, goal) => user = true,
            _ => return false,
        }
    }
    user && standing
}
/// The current goal or an active requirement whose every source is the user.
fn user_instruction(source: &Source, goal: Option<&Source>) -> bool {
    let event = &source.origin["event"];
    let (active, refs) = match event["type"].as_str() {
        Some("run_started" | "goal_revised") => (
            goal.is_some_and(|g| g.id == source.id),
            &event["goal"]["source_refs"],
        ),
        Some("requirement_added") => (
            source.origin["current_lifecycle"] == "active",
            &event["item"]["introduced_by"],
        ),
        Some("requirement_superseded") => (
            source.origin["current_lifecycle"] == "active",
            &event["replacement"]["introduced_by"],
        ),
        _ => return false,
    };
    source.origin["kind"] == "history"
        && active
        && refs
            .as_array()
            .is_some_and(|r| !r.is_empty() && r.iter().all(|s| s["role"] == "user"))
}
