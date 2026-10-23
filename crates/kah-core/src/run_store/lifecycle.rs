use super::*;

impl RunStore {
    /// Return `${XDG_STATE_HOME:-$HOME/.local/state}/kah`.
    pub fn default_state_root() -> Result<PathBuf, RunStoreError> {
        let root = match env::var_os("XDG_STATE_HOME") {
            Some(path) if Path::new(&path).is_absolute() => PathBuf::from(path),
            _ => {
                let home = env::var_os("HOME").ok_or_else(|| RunStoreError::Io {
                    action: "resolve XDG state root",
                    path: PathBuf::from("$HOME"),
                    message: "HOME is not set".to_owned(),
                })?;
                PathBuf::from(home).join(".local/state")
            }
        };
        Ok(root.join(APPLICATION_DIRECTORY))
    }

    /// The state root for `home`, which every kah store uses: the
    /// [`Self::default_state_root`] of this process's own `$HOME`, otherwise
    /// (a fixture home) `home/.local/state/kah`.
    pub fn state_root_for(home: &Path) -> PathBuf {
        if env::var_os("HOME").is_some_and(|actual| Path::new(&actual) == home)
            && let Ok(root) = Self::default_state_root()
        {
            return root;
        }
        home.join(".local/state").join(APPLICATION_DIRECTORY)
    }

    /// Derive the local path without retaining a raw repository path in it.
    pub fn path_for(
        state_root: impl AsRef<Path>,
        repository: impl AsRef<Path>,
        run_id: impl AsRef<str>,
    ) -> Result<PathBuf, RunStoreError> {
        let repository_id = repository_id(repository)?;
        let run_id = RunId::parse(run_id.as_ref())?;
        Ok(state_root
            .as_ref()
            .join(REPOSITORIES_DIRECTORY)
            .join(repository_id)
            .join(RUNS_DIRECTORY)
            .join(run_id.as_str()))
    }

    /// Create a new locked run.  `state_root` is injectable for tests; the
    /// normal caller should pass [`Self::default_state_root`].
    pub fn create(
        state_root: impl AsRef<Path>,
        repository: impl AsRef<Path>,
        run_id: impl AsRef<str>,
        history: &History,
        projection: &ActiveProjection,
    ) -> Result<Self, RunStoreError> {
        validate_projection(history, projection)?;
        let path = Self::path_for(state_root, repository, run_id)?;
        create_run_directory(&path)?;
        let lock = acquire_lock(&path)?;
        let store = Self { path, _lock: lock };

        let initial_segment = store.history_path().join(segment_name(history.records())?);
        write_new_file(
            &initial_segment,
            &history.to_jsonl()?,
            "write history segment",
        )?;
        sync_directory(&store.history_path())?;
        store.write_projection(projection)?;
        write_new_file(&store.pending_path(), "", "open pending delta")?;
        sync_directory(&store.path)?;
        Ok(store)
    }

    /// Create a run and atomically persist its private source cursor after the
    /// initial projection is durable.  If a process is interrupted between
    /// those writes, [`Self::recover`] reports `source_cursor: None`; callers
    /// can safely repair that unbound state with
    /// [`Self::commit_projection_with_source_cursor`] and the same projection.
    pub fn create_with_source_cursor(
        state_root: impl AsRef<Path>,
        repository: impl AsRef<Path>,
        run_id: impl AsRef<str>,
        history: &History,
        projection: &ActiveProjection,
        source_cursor: &SourceCursor,
    ) -> Result<Self, RunStoreError> {
        source_cursor.validate()?;
        let store = Self::create(state_root, repository, run_id, history, projection)?;
        store.write_source_cursor(source_cursor)?;
        Ok(store)
    }

    /// Open an existing run and acquire its exclusive writer lock.
    pub fn open(
        state_root: impl AsRef<Path>,
        repository: impl AsRef<Path>,
        run_id: impl AsRef<str>,
    ) -> Result<Self, RunStoreError> {
        let path = Self::path_for(state_root, repository, run_id)?;
        verify_run_directory(&path)?;
        let lock = acquire_lock(&path)?;
        let store = Self { path, _lock: lock };
        store.recover()?;
        Ok(store)
    }

    /// Detached snapshot work waits for a legitimate writer, with no deadline.
    /// Synchronous callers retain the nonblocking open API.
    pub fn open_wait(
        state_root: impl AsRef<Path>,
        repository: impl AsRef<Path>,
        run_id: impl AsRef<str>,
    ) -> Result<Self, RunStoreError> {
        let path = Self::path_for(state_root, repository, run_id)?;
        verify_run_directory(&path)?;
        let lock = acquire_lock_wait(&path)?;
        let store = Self { path, _lock: lock };
        store.recover()?;
        Ok(store)
    }

    /// Open a run using the current process's XDG state root.
    pub fn open_default(
        repository: impl AsRef<Path>,
        run_id: impl AsRef<str>,
    ) -> Result<Self, RunStoreError> {
        Self::open(Self::default_state_root()?, repository, run_id)
    }

    /// Create a run using the current process's XDG state root.
    pub fn create_default(
        repository: impl AsRef<Path>,
        run_id: impl AsRef<str>,
        history: &History,
        projection: &ActiveProjection,
    ) -> Result<Self, RunStoreError> {
        Self::create(
            Self::default_state_root()?,
            repository,
            run_id,
            history,
            projection,
        )
    }

    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }
}
