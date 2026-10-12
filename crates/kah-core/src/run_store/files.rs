use super::*;

pub(super) fn write_new_file(
    path: &Path,
    contents: &str,
    action: &'static str,
) -> Result<(), RunStoreError> {
    write_new_file_bytes(path, contents.as_bytes(), action)
}

pub(super) fn write_new_file_bytes(
    path: &Path,
    contents: &[u8],
    action: &'static str,
) -> Result<(), RunStoreError> {
    if let Some(parent) = path.parent() {
        ensure_save_directory(parent)?;
    }
    ensure_regular_or_missing(path)?;
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;

        options.mode(0o600);
    }
    let mut file = options.open(path).map_err(|error| {
        if error.kind() == io::ErrorKind::AlreadyExists
            && path
                .extension()
                .is_some_and(|extension| extension == "jsonl")
        {
            RunStoreError::ImmutableHistorySegment {
                path: path.to_path_buf(),
            }
        } else {
            io_error(action, path, error)
        }
    })?;
    file.write_all(contents)
        .map_err(|error| io_error(action, path, error))?;
    file.flush()
        .map_err(|error| io_error("flush state file", path, error))?;
    file.sync_all()
        .map_err(|error| io_error("sync state file", path, error))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;

        fs::set_permissions(path, fs::Permissions::from_mode(0o600))
            .map_err(|error| io_error("set private state permissions", path, error))?;
        file.sync_all()
            .map_err(|error| io_error("sync private state permissions", path, error))?;
    }
    Ok(())
}

pub(super) fn write_new_private_file_bytes(
    path: &Path,
    contents: &[u8],
    action: &'static str,
) -> Result<(), RunStoreError> {
    if let Some(parent) = path.parent() {
        ensure_save_directory(parent)?;
    }
    ensure_regular_or_missing(path)?;
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;

        options.mode(0o600);
    }
    let mut file = options
        .open(path)
        .map_err(|error| io_error(action, path, error))?;
    file.write_all(contents)
        .map_err(|error| io_error(action, path, error))?;
    file.flush()
        .map_err(|error| io_error("flush private state file", path, error))?;
    file.sync_all()
        .map_err(|error| io_error("sync private state file", path, error))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;

        fs::set_permissions(path, fs::Permissions::from_mode(0o600))
            .map_err(|error| io_error("set private state permissions", path, error))?;
        file.sync_all()
            .map_err(|error| io_error("sync private state permissions", path, error))?;
    }
    Ok(())
}

pub(super) fn segment_name(records: &[EventRecord]) -> Result<String, RunStoreError> {
    let Some(first) = records.first() else {
        return Err(RunStoreError::InvalidStateComponent {
            path: PathBuf::from(HISTORY_DIRECTORY),
        });
    };
    let last = records.last().expect("checked nonempty records");
    Ok(format!(
        "{:020}-{:020}.jsonl",
        first.sequence, last.sequence
    ))
}

pub(super) fn sync_directory(path: &Path) -> Result<(), RunStoreError> {
    #[cfg(unix)]
    {
        let directory =
            File::open(path).map_err(|error| io_error("open state directory", path, error))?;
        match directory.sync_all() {
            Ok(()) => {}
            Err(error)
                if matches!(
                    error.kind(),
                    io::ErrorKind::Unsupported | io::ErrorKind::InvalidInput
                ) => {}
            Err(error) => return Err(io_error("sync state directory", path, error)),
        }
    }
    Ok(())
}

pub(super) fn io_error(action: &'static str, path: &Path, error: io::Error) -> RunStoreError {
    RunStoreError::Io {
        action,
        path: path.to_path_buf(),
        message: error.to_string(),
    }
}
