use super::*;

impl RunStore {
    pub(super) fn history_path(&self) -> PathBuf {
        self.path.join(HISTORY_DIRECTORY)
    }

    pub(super) fn projection_path(&self) -> PathBuf {
        self.path.join(PROJECTION_FILE)
    }

    pub(super) fn source_path(&self) -> PathBuf {
        self.path.join(SOURCE_FILE)
    }

    pub(super) fn candidate_path(&self) -> PathBuf {
        self.path.join(CANDIDATE_FILE)
    }

    pub(super) fn completion_gates_path(&self) -> PathBuf {
        self.path.join(COMPLETION_GATES_FILE)
    }

    pub(super) fn pending_path(&self) -> PathBuf {
        self.path.join(PENDING_FILE)
    }

    pub(super) fn temporary_projection_path(&self) -> PathBuf {
        let sequence = TEMPORARY_FILE_SEQUENCE.fetch_add(1, Ordering::Relaxed);
        self.path.join(format!(
            "{TEMP_PROJECTION_PREFIX}{}-{sequence}",
            std::process::id()
        ))
    }

    pub(super) fn temporary_source_path(&self) -> PathBuf {
        let sequence = TEMPORARY_FILE_SEQUENCE.fetch_add(1, Ordering::Relaxed);
        self.path.join(format!(
            "{TEMP_SOURCE_PREFIX}{}-{sequence}",
            std::process::id()
        ))
    }

    pub(super) fn temporary_candidate_path(&self) -> PathBuf {
        let sequence = TEMPORARY_FILE_SEQUENCE.fetch_add(1, Ordering::Relaxed);
        self.path.join(format!(
            "{TEMP_CANDIDATE_PREFIX}{}-{sequence}",
            std::process::id()
        ))
    }

    pub(super) fn temporary_completion_gates_path(&self) -> PathBuf {
        let sequence = TEMPORARY_FILE_SEQUENCE.fetch_add(1, Ordering::Relaxed);
        self.path.join(format!(
            "{TEMP_COMPLETION_GATES_PREFIX}{}-{sequence}",
            std::process::id()
        ))
    }
}
