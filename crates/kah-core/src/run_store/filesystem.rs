use super::*;

pub(super) fn create_run_directory(path: &Path) -> Result<(), RunStoreError> {
    let repositories = path
        .parent()
        .and_then(Path::parent)
        .and_then(Path::parent)
        .ok_or_else(|| RunStoreError::InvalidStateComponent {
            path: path.to_path_buf(),
        })?;
    ensure_private_directory(repositories.parent().ok_or_else(|| {
        RunStoreError::InvalidStateComponent {
            path: path.to_path_buf(),
        }
    })?)?;
    ensure_private_directory(repositories)?;
    ensure_private_directory(path.parent().and_then(Path::parent).ok_or_else(|| {
        RunStoreError::InvalidStateComponent {
            path: path.to_path_buf(),
        }
    })?)?;
    ensure_private_directory(path.parent().ok_or_else(|| {
        RunStoreError::InvalidStateComponent {
            path: path.to_path_buf(),
        }
    })?)?;

    match fs::create_dir(path) {
        Ok(()) => set_private_directory_permissions(path)?,
        Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {
            ensure_not_symlink(path)?;
            return Err(RunStoreError::RunAlreadyExists);
        }
        Err(error) => return Err(io_error("create run directory", path, error)),
    }
    ensure_private_directory(&path.join(HISTORY_DIRECTORY))
}

pub(super) fn verify_run_directory(path: &Path) -> Result<(), RunStoreError> {
    let parent = path
        .parent()
        .ok_or_else(|| RunStoreError::InvalidStateComponent {
            path: path.to_path_buf(),
        })?;
    let repository = parent
        .parent()
        .ok_or_else(|| RunStoreError::InvalidStateComponent {
            path: path.to_path_buf(),
        })?;
    let repositories = repository
        .parent()
        .ok_or_else(|| RunStoreError::InvalidStateComponent {
            path: path.to_path_buf(),
        })?;
    let state_root = repositories
        .parent()
        .ok_or_else(|| RunStoreError::InvalidStateComponent {
            path: path.to_path_buf(),
        })?;
    ensure_private_directory(state_root)?;
    ensure_private_directory(repositories)?;
    ensure_private_directory(repository)?;
    ensure_private_directory(parent)?;
    match fs::symlink_metadata(path) {
        Ok(_) => ensure_private_directory(path)?,
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            return Err(RunStoreError::RunNotFound);
        }
        Err(error) => return Err(io_error("inspect run directory", path, error)),
    }
    ensure_private_directory(&path.join(HISTORY_DIRECTORY))
}

pub(super) fn acquire_lock(run_path: &Path) -> Result<File, RunStoreError> {
    acquire_lock_mode(run_path, false)
}
pub(super) fn acquire_lock_wait(run_path: &Path) -> Result<File, RunStoreError> {
    acquire_lock_mode(run_path, true)
}
fn acquire_lock_mode(run_path: &Path, wait: bool) -> Result<File, RunStoreError> {
    let lock = run_path.join(LOCK_FILE);
    ensure_private_regular_or_missing(&lock)?;
    let mut options = OpenOptions::new();
    options.read(true).write(true).create(true).truncate(false);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;

        options.mode(0o600);
    }
    let file = options
        .open(&lock)
        .map_err(|error| io_error("open writer lock", &lock, error))?;
    if wait {
        file.lock()
            .map_err(|error| io_error("acquire writer lock", &lock, error))?;
        return Ok(file);
    }
    match file.try_lock() {
        Ok(()) => Ok(file),
        Err(fs::TryLockError::WouldBlock) => Err(RunStoreError::Busy),
        Err(fs::TryLockError::Error(error)) => Err(io_error("acquire writer lock", &lock, error)),
    }
}

pub(super) fn ensure_directory(path: &Path) -> Result<(), RunStoreError> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_symlink() => Err(RunStoreError::SymlinkRefused {
            path: path.to_path_buf(),
        }),
        Ok(metadata) if metadata.is_dir() => Ok(()),
        Ok(_) => Err(RunStoreError::InvalidStateComponent {
            path: path.to_path_buf(),
        }),
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            if let Some(parent) = path.parent()
                && parent != path
            {
                ensure_directory(parent)?;
            }
            match fs::create_dir(path) {
                Ok(()) => Ok(()),
                // A concurrent creator won. Re-inspect rather than treating
                // EEXIST as success: symlinks and non-directories still fail.
                Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {
                    ensure_directory(path)
                }
                Err(error) => Err(io_error("create state directory", path, error)),
            }
        }
        Err(error) => Err(io_error("inspect state directory", path, error)),
    }
}

pub fn ensure_private_directory(path: &Path) -> Result<(), RunStoreError> {
    ensure_directory(path)?;
    set_private_directory_permissions(path)
}

#[cfg(unix)]
pub(super) fn set_private_directory_permissions(path: &Path) -> Result<(), RunStoreError> {
    use std::os::unix::fs::PermissionsExt;

    fs::set_permissions(path, fs::Permissions::from_mode(0o700))
        .map_err(|error| io_error("set private state directory permissions", path, error))
}

#[cfg(not(unix))]
pub(super) fn set_private_directory_permissions(_path: &Path) -> Result<(), RunStoreError> {
    Ok(())
}

pub(super) fn ensure_not_symlink(path: &Path) -> Result<(), RunStoreError> {
    let metadata = fs::symlink_metadata(path)
        .map_err(|error| io_error("inspect state component", path, error))?;
    if metadata.file_type().is_symlink() {
        Err(RunStoreError::SymlinkRefused {
            path: path.to_path_buf(),
        })
    } else {
        Ok(())
    }
}

pub(super) fn ensure_regular(path: &Path) -> Result<(), RunStoreError> {
    let metadata =
        fs::symlink_metadata(path).map_err(|error| io_error("inspect state file", path, error))?;
    if metadata.file_type().is_symlink() {
        Err(RunStoreError::SymlinkRefused {
            path: path.to_path_buf(),
        })
    } else if metadata.is_file() {
        Ok(())
    } else {
        Err(RunStoreError::InvalidStateComponent {
            path: path.to_path_buf(),
        })
    }
}

pub(super) fn ensure_regular_or_missing(path: &Path) -> Result<(), RunStoreError> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_symlink() => Err(RunStoreError::SymlinkRefused {
            path: path.to_path_buf(),
        }),
        Ok(metadata) if metadata.is_file() => Ok(()),
        Ok(_) => Err(RunStoreError::InvalidStateComponent {
            path: path.to_path_buf(),
        }),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(io_error("inspect state file", path, error)),
    }
}

pub(super) fn ensure_private_regular(path: &Path) -> Result<(), RunStoreError> {
    ensure_regular(path)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;

        let permissions = fs::metadata(path)
            .map_err(|error| io_error("inspect private state file", path, error))?
            .permissions();
        if permissions.mode() & 0o077 != 0 {
            return Err(RunStoreError::PrivateStatePermissions {
                path: path.to_path_buf(),
            });
        }
    }
    Ok(())
}

pub(super) fn ensure_private_regular_or_missing(path: &Path) -> Result<(), RunStoreError> {
    match fs::symlink_metadata(path) {
        Ok(_) => ensure_private_regular(path),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(io_error("inspect private state file", path, error)),
    }
}
