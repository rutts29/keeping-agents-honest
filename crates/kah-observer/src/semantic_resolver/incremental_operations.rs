use super::*;

impl OperationState<'_> {
    pub(super) fn incremental(
        &mut self,
        operation: IncrementalOperationProposal,
    ) -> Result<(), ValidationFailure> {
        let operation_sources = resolve_sources(
            &operation.source_record_ids,
            &self.lookup,
            &mut self.referenced,
        )?;
        match operation.operation {
            IncrementalOperationName::Add => self.incremental_add(operation, operation_sources),
            IncrementalOperationName::Supersede => {
                self.incremental_supersede(operation, operation_sources)
            }
            IncrementalOperationName::Withdraw => {
                self.incremental_withdraw(operation, operation_sources)
            }
            IncrementalOperationName::Conflict => {
                self.incremental_conflict(operation, operation_sources)
            }
            IncrementalOperationName::Reopen => {
                self.incremental_reopen(operation, operation_sources)
            }
        }
    }
    fn incremental_add(
        &mut self,
        operation: IncrementalOperationProposal,
        operation_sources: Vec<SourceRef>,
    ) -> Result<(), ValidationFailure> {
        validate_incremental_new_clause(&operation, false)?;
        insert_operation_key(&operation.key, &mut self.keys)?;
        let id = IntentId::new(stable_id("intent", &operation.key, self.digest));
        self.history
            .append(Event::RequirementAdded {
                item: IntentItem::new(id, operation.kind, operation.text.trim(), operation_sources),
                approval: None,
            })
            .map_err(|_| ValidationFailure::Transition)?;
        Ok(())
    }

    fn incremental_supersede(
        &mut self,
        operation: IncrementalOperationProposal,
        operation_sources: Vec<SourceRef>,
    ) -> Result<(), ValidationFailure> {
        validate_incremental_new_clause(&operation, true)?;
        insert_operation_key(&operation.key, &mut self.keys)?;
        let previous_id = IntentId::new(&operation.target_intent_id);
        let id = IntentId::new(stable_id("intent", &operation.key, self.digest));
        self.history
            .append(Event::RequirementSuperseded {
                previous_id: previous_id.clone(),
                replacement: IntentItem::superseding(
                    id,
                    operation.kind,
                    operation.text.trim(),
                    operation_sources,
                    previous_id,
                ),
                approval: None,
            })
            .map_err(|_| ValidationFailure::Transition)?;
        Ok(())
    }

    fn incremental_withdraw(
        &mut self,
        operation: IncrementalOperationProposal,
        operation_sources: Vec<SourceRef>,
    ) -> Result<(), ValidationFailure> {
        validate_incremental_terminal_shape(&operation, false)?;
        self.history
            .append(Event::RequirementWithdrawn {
                intent_id: IntentId::new(&operation.target_intent_id),
                source_refs: operation_sources,
                approval: None,
            })
            .map_err(|_| ValidationFailure::Transition)?;
        Ok(())
    }

    fn incremental_conflict(
        &mut self,
        operation: IncrementalOperationProposal,
        operation_sources: Vec<SourceRef>,
    ) -> Result<(), ValidationFailure> {
        validate_incremental_conflict_shape(&operation)?;
        insert_operation_key(&operation.key, &mut self.keys)?;
        let affected = operation
            .intent_ids
            .iter()
            .map(IntentId::new)
            .collect::<Vec<_>>();
        let mut alternative_keys = BTreeSet::new();
        let alternatives = operation
            .alternatives
            .into_iter()
            .map(|alternative| {
                validate_key(&alternative.key)?;
                if alternative.text.trim().is_empty()
                    || !alternative_keys.insert(alternative.key.clone())
                {
                    return Err(ValidationFailure::AlternativeShape);
                }
                let sources = resolve_sources(
                    &alternative.source_record_ids,
                    &self.lookup,
                    &mut self.referenced,
                )?;
                Ok(ConflictAlternative::new(
                    stable_id(
                        "alternative",
                        &format!("{}:{}", operation.key, alternative.key),
                        self.digest,
                    ),
                    alternative.text.trim(),
                    sources,
                ))
            })
            .collect::<Result<Vec<_>, _>>()?;
        self.history
            .append(Event::ConflictRaised {
                conflict: Conflict::new(
                    stable_id("conflict", &operation.key, self.digest),
                    affected,
                    alternatives,
                    operation_sources,
                ),
                approval: None,
            })
            .map_err(|_| ValidationFailure::Transition)?;
        Ok(())
    }

    fn incremental_reopen(
        &mut self,
        operation: IncrementalOperationProposal,
        operation_sources: Vec<SourceRef>,
    ) -> Result<(), ValidationFailure> {
        validate_incremental_terminal_shape(&operation, true)?;
        self.history
            .append(Event::EvidenceInvalidated {
                intent_id: IntentId::new(&operation.target_intent_id),
                evidence_id: EvidenceId::new(&operation.evidence_id),
                source_refs: operation_sources,
                reason: operation.reason,
            })
            .map_err(|_| ValidationFailure::Transition)?;
        Ok(())
    }
}
