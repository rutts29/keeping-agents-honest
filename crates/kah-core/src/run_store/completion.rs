use super::*;

impl RunStore {
    pub fn record_completion_gate(
        &self,
        record: CompletionGateRecord,
    ) -> Result<CompletionGateState, RunStoreError> {
        validate_completion_gate_record(&record, &self.completion_gates_path())?;
        let mut state = self.completion_gate_state()?;
        if record.gate == CompletionGate::Review
            && state
                .record(CompletionGate::Review)
                .is_some_and(|prior| prior.candidate_digest == record.candidate_digest)
        {
            return Err(RunStoreError::ReviewAlreadyRecorded);
        }
        state.records.insert(record.gate, record);
        self.write_completion_gate_state(&state)?;
        Ok(state)
    }

    pub fn completion_gate_state(&self) -> Result<CompletionGateState, RunStoreError> {
        let path = self.completion_gates_path();
        match fs::symlink_metadata(&path) {
            Ok(_) => ensure_private_regular(&path)?,
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                return Ok(CompletionGateState::empty());
            }
            Err(error) => return Err(io_error("inspect completion gates", &path, error)),
        }
        let input = fs::read_to_string(&path)
            .map_err(|error| io_error("read completion gates", &path, error))?;
        let state: CompletionGateState = serde_json::from_str(&input)
            .map_err(|error| RunStoreError::Serialization(error.to_string()))?;
        if state.schema_version != COMPLETION_GATE_STATE_SCHEMA_VERSION {
            return Err(RunStoreError::InvalidStateComponent { path });
        }
        for (gate, record) in &state.records {
            if *gate != record.gate {
                return Err(RunStoreError::InvalidStateComponent { path });
            }
            validate_completion_gate_record(record, &path)?;
        }
        Ok(state)
    }
}
