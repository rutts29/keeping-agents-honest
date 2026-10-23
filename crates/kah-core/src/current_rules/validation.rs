use super::*;
use std::collections::{BTreeMap, BTreeSet};

impl RuleSet {
    pub fn validate(
        &self,
        input: &Input,
        previous: Option<&ValidatedSet>,
    ) -> Result<ValidatedSet, String> {
        self.validate_report(input, previous.map(ValidatedSet::as_set))?;
        Ok(ValidatedSet(self.clone(), input.revision))
    }
    /// Check a report against its host-published input/predecessor snapshot.
    /// This does not construct an adopted set; adoption validates the live chain.
    pub fn validate_report(&self, input: &Input, previous: Option<&RuleSet>) -> Result<(), String> {
        input.validate()?;
        if self.schema_version != 1
            || self.input_digest != input.digest()?
            || self.previous_digest != previous.map(digest).transpose()?
        {
            return Err(error("BINDING", "version, input or predecessor changed"));
        }
        let clauses = unique(&self.clauses, |c| &c.id)?;
        let rules = unique(&self.rules, |r| &r.id)?;
        for clause in &self.clauses {
            validate_clause(clause, input, &rules)?;
        }
        for conflict in crate::intent_history::History::replay(input.history.clone())
            .map_err(|e| error("HISTORY", &e.to_string()))?
            .open_conflicts()
            .values()
        {
            if !self.clauses.iter().any(|c| {
                c.disposition == Disposition::UnresolvedConflict
                    && conflict
                        .intent_ids
                        .iter()
                        .any(|id| c.words.source == format!("intent:{id}"))
            }) {
                return Err(error("CONFLICT", "History conflict was silently resolved"));
            }
        }
        coverage(input, &self.clauses)?;
        for rule in &self.rules {
            validate_rule(rule, input, &clauses)?;
        }
        if let Some(previous) = previous {
            for old in &previous.rules {
                if self.rules.iter().any(|new| {
                    new.predicate == old.predicate
                        && new.scope == old.scope
                        && new.diff_checkable == old.diff_checkable
                        && new.id != old.id
                }) {
                    return Err(error("IDENTITY", "unchanged rule renamed"));
                }
            }
        }
        Ok(())
    }
}
fn unique<T>(items: &[T], key: impl Fn(&T) -> &String) -> Result<BTreeMap<&String, &T>, String> {
    let mut out = BTreeMap::new();
    for item in items {
        if key(item).trim().is_empty() || out.insert(key(item), item).is_some() {
            return Err(error("IDENTITY", "empty or duplicate identity"));
        }
    }
    Ok(out)
}
fn words<'a>(input: &'a Input, words: &Words) -> Result<&'a Source, String> {
    let source = input
        .sources
        .iter()
        .find(|s| s.id == words.source)
        .ok_or_else(|| error("QUOTE", "unknown source"))?;
    if words.start >= words.end
        || source.text.get(words.start..words.end) != Some(words.quote.as_str())
    {
        return Err(error("QUOTE", "quote does not match exact UTF-8 range"));
    }
    Ok(source)
}
fn validate_clause(
    clause: &Clause,
    input: &Input,
    rules: &BTreeMap<&String, &Rule>,
) -> Result<(), String> {
    let source = words(input, &clause.words)?;
    if clause.reason.trim().is_empty() {
        return Err(error("REASON", "disposition needs reasoning"));
    }
    for witness in &clause.witnesses {
        words(input, witness)?;
    }
    let contributes = matches!(
        clause.disposition,
        Disposition::Contributes | Disposition::Merged
    );
    if contributes && source.origin["non_authority_sender"] == true {
        return Err(error(
            "AUTHORITY",
            "delegate intake cannot support a user rule",
        ));
    }
    if contributes
        && source.origin["current_lifecycle"]
            .as_str()
            .is_some_and(|s| s != "active")
    {
        return Err(error(
            "LIFECYCLE",
            "inactive History item cannot support a current rule",
        ));
    }
    clause_links(clause, contributes, rules)?;
    if matches!(
        clause.disposition,
        Disposition::Superseded
            | Disposition::ExpiredUnderCondition
            | Disposition::Withdrawn
            | Disposition::UnresolvedConflict
    ) && clause.witnesses.is_empty()
    {
        return Err(error(
            "WITNESS",
            "retirement or conflict needs an exact witness, not age",
        ));
    }
    Ok(())
}
/// Messages name the clause and the fix, so an observer sees what to change
/// (a bare "only current clauses support rules" was too vague).
fn clause_links(
    clause: &Clause,
    contributes: bool,
    rules: &BTreeMap<&String, &Rule>,
) -> Result<(), String> {
    let (id, disposition) = (&clause.id, name(&clause.disposition));
    if contributes == clause.rules.is_empty() {
        let fix = if contributes {
            "links no rule; a contributes or merged clause must link at least one rule"
        } else {
            "links rules; only a contributes or merged clause may link rules"
        };
        return Err(error(
            "ACCOUNTING",
            &format!(
                "clause {id} is {disposition} but {fix}, and every other disposition must link none"
            ),
        ));
    }
    let mut linked = BTreeSet::new();
    for rule_id in &clause.rules {
        let rule = rules.get(rule_id).ok_or_else(|| {
            error(
                "ORPHAN",
                &format!("clause {id} links rule {rule_id}, which is not in rules"),
            )
        })?;
        if !linked.insert(rule_id) {
            return Err(error(
                "ORPHAN",
                &format!("clause {id} lists rule {rule_id} more than once; list each rule once"),
            ));
        }
        if !rule.clauses.contains(&clause.id) {
            return Err(error(
                "ORPHAN",
                &format!(
                    "clause {id} links rule {rule_id}, but that rule's clauses do not list {id}; list the link on both sides"
                ),
            ));
        }
    }
    Ok(())
}
fn name(disposition: &Disposition) -> String {
    serde_json::to_value(disposition)
        .ok()
        .and_then(|v| v.as_str().map(str::to_owned))
        .unwrap_or_default()
}
fn coverage(input: &Input, clauses: &[Clause]) -> Result<(), String> {
    for source in &input.sources {
        let mut ranges = clauses
            .iter()
            .filter(|c| c.words.source == source.id)
            .map(|c| (c.words.start, c.words.end))
            .collect::<Vec<_>>();
        ranges.sort_unstable();
        let mut end = 0;
        for (start, next) in ranges {
            // Whitespace is not authority, but every non-whitespace word must be accounted for.
            if start < end
                || source
                    .text
                    .get(end..start)
                    .is_none_or(|gap| !gap.trim().is_empty())
            {
                return Err(error("COVERAGE", "overlapping or unaccounted source words"));
            }
            end = next;
        }
        if source
            .text
            .get(end..)
            .is_none_or(|gap| !gap.trim().is_empty())
        {
            return Err(error("COVERAGE", "unaccounted source tail"));
        }
    }
    Ok(())
}
fn validate_rule(
    rule: &Rule,
    input: &Input,
    clauses: &BTreeMap<&String, &Clause>,
) -> Result<(), String> {
    let rule_id = &rule.id;
    if rule.predicate.trim().is_empty() || rule.scope.trim().is_empty() || rule.clauses.is_empty() {
        return Err(error(
            "ORPHAN",
            &format!("rule {rule_id} needs a predicate, a scope and at least one source clause"),
        ));
    }
    let mut linked = BTreeSet::new();
    for id in &rule.clauses {
        let clause = clauses.get(id).ok_or_else(|| {
            error(
                "ORPHAN",
                &format!("rule {rule_id} lists clause {id}, which is not in clauses"),
            )
        })?;
        if !linked.insert(id) {
            return Err(error(
                "ORPHAN",
                &format!("rule {rule_id} lists clause {id} more than once; list each clause once"),
            ));
        }
        if !clause.rules.contains(&rule.id) {
            return Err(error(
                "ORPHAN",
                &format!(
                    "rule {rule_id} lists clause {id}, but that clause's rules do not list {rule_id}; list the link on both sides"
                ),
            ));
        }
    }
    let mut permissions = BTreeSet::new();
    for permission in &rule.permissions {
        words(input, permission)?;
        if !permissions.insert((&permission.source, permission.start, permission.end))
            || !rule.clauses.iter().any(|id| {
                let c = clauses[id];
                c.words.source == permission.source
                    && c.words.start <= permission.start
                    && c.words.end >= permission.end
            })
        {
            return Err(error(
                "PERMISSION",
                "permission must be exact words within this rule's linked clauses",
            ));
        }
    }
    Ok(())
}
