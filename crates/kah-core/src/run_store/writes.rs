use super::*;

impl RunStore {
    pub(super) fn write_projection(
        &self,
        projection: &ActiveProjection,
    ) -> Result<(), RunStoreError> {
        let projection_path = self.projection_path();
        ensure_private_regular_or_missing(&projection_path)?;
        let bytes = serde_json::to_vec(projection)
            .map_err(|error| RunStoreError::Serialization(error.to_string()))?;
        let temporary = self.temporary_projection_path();
        write_new_file_bytes(&temporary, &bytes, "write temporary projection")?;
        fs::rename(&temporary, &projection_path)
            .map_err(|error| io_error("replace projection", &projection_path, error))?;
        sync_directory(&self.path)
    }

    pub(super) fn write_source_cursor(
        &self,
        source_cursor: &SourceCursor,
    ) -> Result<(), RunStoreError> {
        let source_path = self.source_path();
        ensure_private_regular_or_missing(&source_path)?;
        let bytes = serde_json::to_vec(source_cursor)
            .map_err(|error| RunStoreError::Serialization(error.to_string()))?;
        let temporary = self.temporary_source_path();
        write_new_private_file_bytes(&temporary, &bytes, "write temporary source cursor")?;
        fs::rename(&temporary, &source_path)
            .map_err(|error| io_error("replace source cursor", &source_path, error))?;
        sync_directory(&self.path)
    }

    pub(super) fn write_candidate_binding(
        &self,
        binding: &CandidateBinding,
    ) -> Result<(), RunStoreError> {
        let candidate_path = self.candidate_path();
        ensure_private_regular_or_missing(&candidate_path)?;
        let bytes = serde_json::to_vec(binding)
            .map_err(|error| RunStoreError::Serialization(error.to_string()))?;
        let temporary = self.temporary_candidate_path();
        write_new_private_file_bytes(&temporary, &bytes, "write temporary candidate binding")?;
        fs::rename(&temporary, &candidate_path)
            .map_err(|error| io_error("replace candidate binding", &candidate_path, error))?;
        sync_directory(&self.path)
    }

    pub(super) fn write_completion_gate_state(
        &self,
        state: &CompletionGateState,
    ) -> Result<(), RunStoreError> {
        let path = self.completion_gates_path();
        ensure_private_regular_or_missing(&path)?;
        let bytes = serde_json::to_vec(state)
            .map_err(|error| RunStoreError::Serialization(error.to_string()))?;
        let temporary = self.temporary_completion_gates_path();
        write_new_private_file_bytes(&temporary, &bytes, "write temporary completion gates")?;
        fs::rename(&temporary, &path)
            .map_err(|error| io_error("replace completion gates", &path, error))?;
        sync_directory(&self.path)
    }
}
