//! Adapter from the validated History/standing projection to commit questions.
use super::*;
use kah_core::current_rules::{Input, ValidatedSet};
pub struct CurrentSet<'a> {
    set: &'a ValidatedSet,
    input: &'a Input,
    observer: &'a str,
}
impl<'a> CurrentSet<'a> {
    pub fn new(set: &'a ValidatedSet, input: &'a Input, observer: &'a str) -> Result<Self, String> {
        if set.as_set().input_digest != input.digest()?
            || observer.is_empty()
            || set.has_conflicts()
        {
            return Err(
                "COMMIT_RULE_SOURCE_CHANGED stage=rule_source: current validated binding required"
                    .into(),
            );
        }
        Ok(Self {
            set,
            input,
            observer,
        })
    }
}
impl Source for CurrentSet<'_> {
    fn load(&self, _items: &[Item]) -> Result<Resolved, String> {
        let set = self.set.as_set();
        let rules = set
            .rules
            .iter()
            .map(|r| {
                let sources = r
                    .clauses
                    .iter()
                    .map(|id| {
                        let c = set
                            .clauses
                            .iter()
                            .find(|c| &c.id == id)
                            .ok_or("COMMIT_RULE_SOURCE_MISSING stage=rule_source")?;
                        Ok(Link {
                            source_id: c.words.source.clone(),
                            quote: c.words.quote.clone(),
                        })
                    })
                    .collect::<Result<Vec<_>, String>>()?;
                Ok(Rule {
                    id: r.id.clone(),
                    predicate: r.predicate.clone(),
                    scope: r.scope.clone(),
                    diff_checkable: r.diff_checkable,
                    sources,
                    permissions: r
                        .permissions
                        .iter()
                        .map(|p| Link {
                            source_id: format!("permission:{}:{}:{}", p.source, p.start, p.end),
                            quote: p.quote.clone(),
                        })
                        .collect(),
                })
            })
            .collect::<Result<Vec<_>, String>>()?;
        Ok(Resolved {
            source_digest: self.set.digest()?,
            observer_session: self.observer.into(),
            history_digest: Some(self.input.history_digest.clone()),
            rules,
            source_words: self
                .input
                .sources
                .iter()
                .map(|s| (s.id.clone(), s.text.clone()))
                .collect(),
        })
    }
}
