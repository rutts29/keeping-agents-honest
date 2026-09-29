use super::*;

impl OperationState<'_> {
    pub(super) fn initial(
        &mut self,
        operation: OperationProposal,
    ) -> Result<(), ValidationFailure> {
        let operation_sources = resolve_sources(
            &operation.source_record_ids,
            &self.lookup,
            &mut self.referenced,
        )?;
        match operation.operation {
            OperationName::Add => self.initial_add(operation, operation_sources),
            OperationName::Supersede => self.initial_supersede(operation, operation_sources),
            OperationName::Withdraw => self.initial_withdraw(operation, operation_sources),
            OperationName::Conflict => self.initial_conflict(operation, operation_sources),
        }
    }
    fn initial_add(
        &mut self,
        operation: OperationProposal,
        operation_sources: Vec<SourceRef>,
    ) -> Result<(), ValidationFailure> {
        validate_new_clause(&operation)?;
        insert_operation_key(&operation.key, &mut self.keys)?;
        let id = IntentId::new(stable_id("intent", &operation.key, self.digest));
        self.history
            .append(Event::RequirementAdded {
                item: IntentItem::new(
                    id.clone(),
                    operation.kind,
                    operation.text.trim(),
                    operation_sources,
                ),
                approval: None,
            })
            .map_err(|_| ValidationFailure::Transition)?;
        self.intents.insert(operation.key, id);
        Ok(())
    }

    fn initial_supersede(
        &mut self,
        operation: OperationProposal,
        operation_sources: Vec<SourceRef>,
    ) -> Result<(), ValidationFailure> {
        validate_new_clause(&operation)?;
        insert_operation_key(&operation.key, &mut self.keys)?;
        validate_key(&operation.target_key)?;
        let previous_id = self
            .intents
            .get(&operation.target_key)
            .cloned()
            .ok_or(ValidationFailure::Transition)?;
        let id = IntentId::new(stable_id("intent", &operation.key, self.digest));
        self.history
            .append(Event::RequirementSuperseded {
                previous_id: previous_id.clone(),
                replacement: IntentItem::superseding(
                    id.clone(),
                    operation.kind,
                    operation.text.trim(),
                    operation_sources,
                    previous_id,
                ),
                approval: None,
            })
            .map_err(|_| ValidationFailure::Transition)?;
        self.intents.insert(operation.key, id);
        Ok(())
    }

    fn initial_withdraw(
        &mut self,
        operation: OperationProposal,
        operation_sources: Vec<SourceRef>,
    ) -> Result<(), ValidationFailure> {
        if !operation.key.is_empty()
            || !operation.text.is_empty()
            || !operation.intent_keys.is_empty()
            || !operation.alternatives.is_empty()
        {
            return Err(ValidationFailure::WithdrawShape);
        }
        validate_key(&operation.target_key)?;
        let intent_id = self
            .intents
            .get(&operation.target_key)
            .cloned()
            .ok_or(ValidationFailure::Transition)?;
        self.history
            .append(Event::RequirementWithdrawn {
                intent_id,
                source_refs: operation_sources,
                approval: None,
            })
            .map_err(|_| ValidationFailure::Transition)?;
        Ok(())
    }

    fn initial_conflict(
        &mut self,
        operation: OperationProposal,
        operation_sources: Vec<SourceRef>,
    ) -> Result<(), ValidationFailure> {
        if !operation.target_key.is_empty()
            || operation.text.trim().is_empty()
            || operation.intent_keys.is_empty()
            || operation.alternatives.len() < 2
        {
            return Err(ValidationFailure::ConflictShape);
        }
        insert_operation_key(&operation.key, &mut self.keys)?;
        let conflict_key = operation.key.clone();
        reject_duplicates(&operation.intent_keys, ValidationFailure::ConflictShape)?;
        let affected = operation
            .intent_keys
            .iter()
            .map(|key| {
                validate_key(key)?;
                self.intents
                    .get(key)
                    .cloned()
                    .ok_or(ValidationFailure::Transition)
            })
            .collect::<Result<Vec<_>, _>>()?;
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
                let alternative_sources = resolve_sources(
                    &alternative.source_record_ids,
                    &self.lookup,
                    &mut self.referenced,
                )?;
                Ok(ConflictAlternative::new(
                    stable_id(
                        "alternative",
                        &format!("{conflict_key}:{}", alternative.key),
                        self.digest,
                    ),
                    alternative.text.trim(),
                    alternative_sources,
                ))
            })
            .collect::<Result<Vec<_>, _>>()?;
        self.history
            .append(Event::ConflictRaised {
                conflict: Conflict::new(
                    stable_id("conflict", &conflict_key, self.digest),
                    affected,
                    alternatives,
                    operation_sources,
                ),
                approval: None,
            })
            .map_err(|_| ValidationFailure::Transition)?;
        Ok(())
    }
}
