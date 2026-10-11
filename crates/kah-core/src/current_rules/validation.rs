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
    if contributes == clause.rules.is_empty() {
        return Err(error("ACCOUNTING", "only current clauses support rules"));
    }
    let mut linked = BTreeSet::new();
    for id in &clause.rules {
        let rule = rules
            .get(id)
            .ok_or_else(|| error("ORPHAN", "missing output rule"))?;
        if !linked.insert(id) || !rule.clauses.contains(&clause.id) {
            return Err(error(
                "ORPHAN",
                "source/rule links must be reciprocal and unique",
            ));
        }
    }
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
    if rule.predicate.trim().is_empty() || rule.scope.trim().is_empty() || rule.clauses.is_empty() {
        return Err(error("ORPHAN", "rule needs predicate, scope and sources"));
    }
    let mut linked = BTreeSet::new();
    for id in &rule.clauses {
        let clause = clauses
            .get(id)
            .ok_or_else(|| error("ORPHAN", "unknown source clause"))?;
        if !linked.insert(id) || !clause.rules.contains(&rule.id) {
            return Err(error(
                "ORPHAN",
                "rule/source links must be reciprocal and unique",
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
