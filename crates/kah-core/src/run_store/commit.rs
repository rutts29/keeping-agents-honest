use super::*;

impl RunStore {
    /// Append one validated event to the current unrotated JSONL delta.
    pub fn append_pending(&self, record: EventRecord) -> Result<(), RunStoreError> {
        let recovered = self.recover_projection_state()?;
        let mut records = recovered.history.records().to_vec();
        records.push(record.clone());
        History::replay(records)?;

        let pending = self.pending_path();
        self.repair_pending_tail()?;
        let mut line = serde_json::to_vec(&record)
            .map_err(|error| RunStoreError::Serialization(error.to_string()))?;
        line.push(b'\n');
        let mut options = OpenOptions::new();
        options.create(true).append(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;

            options.mode(0o600);
        }
        let mut file = options
            .open(&pending)
            .map_err(|error| io_error("append pending delta", &pending, error))?;
        file.write_all(&line)
            .map_err(|error| io_error("append pending delta", &pending, error))?;
        file.flush()
            .map_err(|error| io_error("flush pending delta", &pending, error))?;
        file.sync_all()
            .map_err(|error| io_error("sync pending delta", &pending, error))?;
        sync_directory(&self.path)
    }

    /// Atomically install a projection, then rotate exactly the pending delta
    /// into a never-overwritten immutable history segment.
    pub fn commit_projection(&self, projection: &ActiveProjection) -> Result<(), RunStoreError> {
        self.repair_pending_tail()?;
        let pending = self.read_pending()?;
        if !pending.is_empty() {
            let segment = self.history_path().join(segment_name(&pending)?);
            // Presence is itself the immutable-history error; the existing
            // segment is never opened or trusted on this write path.
            ensure_regular_or_missing(&segment)?;
            if segment.exists() {
                return Err(RunStoreError::ImmutableHistorySegment { path: segment });
            }
        }

        let recovered = self.recover_projection_state()?;
        validate_projection(&recovered.history, projection)?;
        self.write_projection(projection)?;

        if !pending.is_empty() {
            let pending_path = self.pending_path();
            let segment = self.history_path().join(segment_name(&pending)?);
            fs::rename(&pending_path, &segment)
                .map_err(|error| io_error("rotate pending delta", &pending_path, error))?;
            sync_directory(&self.history_path())?;
            write_new_file(&pending_path, "", "open pending delta")?;
            sync_directory(&self.path)?;
        }
        Ok(())
    }

    /// Commit a projection, then atomically advance the private source cursor.
    /// The cursor is not written when projection validation or durability fails.
    pub fn commit_projection_with_source_cursor(
        &self,
        projection: &ActiveProjection,
        source_cursor: &SourceCursor,
    ) -> Result<(), RunStoreError> {
        source_cursor.validate()?;
        self.commit_projection(projection)?;
        self.write_source_cursor(source_cursor)
    }
}
