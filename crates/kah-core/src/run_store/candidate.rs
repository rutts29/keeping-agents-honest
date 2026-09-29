use super::*;

impl RunStore {
    /// Atomically bind this run to the exact isolated child used by `continue`.
    pub fn bind_candidate(
        &self,
        child_thread_id: impl Into<String>,
        candidate: impl AsRef<Path>,
        approved_goal: &str,
    ) -> Result<CandidateBinding, RunStoreError> {
        let child_thread_id = child_thread_id.into();
        if child_thread_id.trim().is_empty()
            || child_thread_id.len() > MAX_SOURCE_CURSOR_IDENTIFIER_BYTES
            || child_thread_id.chars().any(char::is_control)
        {
            return Err(RunStoreError::InvalidStateComponent {
                path: PathBuf::from("candidate child thread id"),
            });
        }
        let workspace_root = fs::canonicalize(self.path.join("workspaces")).map_err(|error| {
            io_error(
                "canonicalize run workspace root",
                &self.path.join("workspaces"),
                error,
            )
        })?;
        let candidate = fs::canonicalize(candidate.as_ref())
            .map_err(|error| io_error("canonicalize bound candidate", candidate.as_ref(), error))?;
        if candidate == workspace_root || !candidate.starts_with(&workspace_root) {
            return Err(RunStoreError::InvalidStateComponent { path: candidate });
        }
        if approved_goal.trim().is_empty() {
            return Err(RunStoreError::InvalidStateComponent {
                path: PathBuf::from("approved child goal"),
            });
        }
        let mut goal_hasher = Sha256::new();
        goal_hasher.update(b"driftctl.approved-child-goal.v1\0");
        goal_hasher.update(approved_goal.as_bytes());
        let binding = CandidateBinding {
            schema_version: CANDIDATE_BINDING_SCHEMA_VERSION,
            child_thread_id,
            candidate_path: candidate,
            approved_goal_digest: format!("sha256:{:x}", goal_hasher.finalize()),
        };
        self.write_candidate_binding(&binding)?;
        Ok(binding)
    }

    /// Load and revalidate the exact isolated candidate bound by `continue`.
    pub fn candidate_binding(&self) -> Result<Option<CandidateBinding>, RunStoreError> {
        let candidate_path = self.candidate_path();
        match fs::symlink_metadata(&candidate_path) {
            Ok(_) => ensure_private_regular(&candidate_path)?,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
            Err(error) => {
                return Err(io_error(
                    "inspect candidate binding",
                    &candidate_path,
                    error,
                ));
            }
        }
        let input = fs::read_to_string(&candidate_path)
            .map_err(|error| io_error("read candidate binding", &candidate_path, error))?;
        let binding: CandidateBinding = serde_json::from_str(&input)
            .map_err(|error| RunStoreError::Serialization(error.to_string()))?;
        if binding.schema_version != CANDIDATE_BINDING_SCHEMA_VERSION
            || binding.child_thread_id.trim().is_empty()
            || binding.child_thread_id.len() > MAX_SOURCE_CURSOR_IDENTIFIER_BYTES
            || binding.child_thread_id.chars().any(char::is_control)
            || !binding.approved_goal_digest.starts_with("sha256:")
        {
            return Err(RunStoreError::InvalidStateComponent {
                path: candidate_path,
            });
        }
        let workspace_root = fs::canonicalize(self.path.join("workspaces")).map_err(|error| {
            io_error(
                "canonicalize run workspace root",
                &self.path.join("workspaces"),
                error,
            )
        })?;
        let canonical = fs::canonicalize(&binding.candidate_path).map_err(|error| {
            io_error(
                "canonicalize bound candidate",
                &binding.candidate_path,
                error,
            )
        })?;
        if canonical != binding.candidate_path
            || canonical == workspace_root
            || !canonical.starts_with(&workspace_root)
        {
            return Err(RunStoreError::InvalidStateComponent {
                path: binding.candidate_path,
            });
        }
        Ok(Some(binding))
    }
}
