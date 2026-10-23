use super::*;

impl RunStore {
    /// Read the durable projection and any unrotated JSONL delta.  Interrupted
    /// temporary files are deliberately not part of this lookup.
    pub fn recover(&self) -> Result<RecoveredRun, RunStoreError> {
        let mut recovered = self.recover_projection_state()?;
        recovered.source_cursor = self.read_source_cursor()?;
        Ok(recovered)
    }

    pub(super) fn recover_projection_state(&self) -> Result<RecoveredRun, RunStoreError> {
        verify_run_directory(&self.path)?;
        let accepted_records = self.read_history_segments()?;
        let accepted_history = History::replay(accepted_records)?;
        let pending_delta = self.read_pending()?;
        let mut all_records = accepted_history.records().to_vec();
        all_records.extend(pending_delta.iter().cloned());
        let history = History::replay(all_records)?;

        let projection_path = self.projection_path();
        ensure_private_regular(&projection_path)?;
        let contents = fs::read_to_string(&projection_path)
            .map_err(|error| io_error("read projection", &projection_path, error))?;
        let projection: ActiveProjection = serde_json::from_str(&contents)
            .map_err(|error| RunStoreError::Serialization(error.to_string()))?;

        let budget = projection.overflow.budget;
        let mut accepted_projection = project(&accepted_history, budget)?;
        accepted_projection.generated_by = projection.generated_by.clone();
        let mut current_projection = project(&history, budget)?;
        current_projection.generated_by = projection.generated_by.clone();
        if projection != accepted_projection && projection != current_projection {
            return Err(RunStoreError::ProjectionMismatch);
        }
        Ok(RecoveredRun {
            projection,
            history,
            pending_delta,
            source_cursor: None,
        })
    }

    pub(super) fn read_history_segments(&self) -> Result<Vec<EventRecord>, RunStoreError> {
        let history = self.history_path();
        ensure_private_directory(&history)?;
        let mut paths = fs::read_dir(&history)
            .map_err(|error| io_error("read history directory", &history, error))?
            .map(|entry| {
                entry
                    .map(|entry| entry.path())
                    .map_err(|error| io_error("read history entry", &history, error))
            })
            .collect::<Result<Vec<_>, _>>()?;
        paths.sort();

        let mut records = Vec::new();
        for path in paths {
            if path
                .extension()
                .is_none_or(|extension| extension != "jsonl")
            {
                return Err(RunStoreError::InvalidStateComponent { path });
            }
            ensure_private_regular(&path)?;
            let input = fs::read_to_string(&path)
                .map_err(|error| io_error("read history segment", &path, error))?;
            let segment_records = parse_jsonl(&input)?;
            let expected_name = segment_name(&segment_records)?;
            if path
                .file_name()
                .is_none_or(|name| name != std::ffi::OsStr::new(&expected_name))
            {
                return Err(RunStoreError::InvalidStateComponent { path });
            }
            records.extend(segment_records);
        }
        Ok(records)
    }

    pub(super) fn read_pending(&self) -> Result<Vec<EventRecord>, RunStoreError> {
        let pending = self.pending_path();
        match fs::symlink_metadata(&pending) {
            Ok(_) => ensure_private_regular(&pending)?,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(error) => return Err(io_error("inspect pending delta", &pending, error)),
        }
        let bytes =
            fs::read(&pending).map_err(|error| io_error("read pending delta", &pending, error))?;
        let input = std::str::from_utf8(committed_prefix(&bytes))
            .map_err(|error| RunStoreError::Serialization(error.to_string()))?;
        parse_jsonl(input)
    }

    /// A record is committed by its newline. A crash can leave an unfinished
    /// last line; drop exactly that tail so the next append starts a clean line.
    /// Malformed newline-terminated records are never touched.
    pub(super) fn repair_pending_tail(&self) -> Result<(), RunStoreError> {
        let pending = self.pending_path();
        match fs::symlink_metadata(&pending) {
            Ok(_) => ensure_private_regular(&pending)?,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(()),
            Err(error) => return Err(io_error("inspect pending delta", &pending, error)),
        }
        let bytes =
            fs::read(&pending).map_err(|error| io_error("read pending delta", &pending, error))?;
        let keep = committed_prefix(&bytes).len();
        if keep == bytes.len() {
            return Ok(());
        }
        let file = OpenOptions::new()
            .write(true)
            .open(&pending)
            .map_err(|error| io_error("open pending delta", &pending, error))?;
        file.set_len(keep as u64)
            .map_err(|error| io_error("trim unfinished pending record", &pending, error))?;
        file.sync_all()
            .map_err(|error| io_error("sync pending delta", &pending, error))
    }

    pub(super) fn read_source_cursor(&self) -> Result<Option<SourceCursor>, RunStoreError> {
        let source_path = self.source_path();
        match fs::symlink_metadata(&source_path) {
            Ok(_) => ensure_private_regular(&source_path)?,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(io_error("inspect source cursor", &source_path, error)),
        }
        let input = fs::read_to_string(&source_path)
            .map_err(|error| io_error("read source cursor", &source_path, error))?;
        let source_cursor: SourceCursor = serde_json::from_str(&input)
            .map_err(|error| RunStoreError::Serialization(error.to_string()))?;
        source_cursor.validate()?;
        Ok(Some(source_cursor))
    }
}
