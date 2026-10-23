use super::*;

/// Prepare the parents and a private staging directory for a new run at `path`.
pub(super) fn create_run_directory(path: &Path) -> Result<PathBuf, RunStoreError> {
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

    match fs::symlink_metadata(path) {
        Ok(_) => {
            ensure_not_symlink(path)?;
            return Err(RunStoreError::RunAlreadyExists);
        }
        Err(error) if error.kind() == io::ErrorKind::NotFound => {}
        Err(error) => return Err(io_error("inspect run directory", path, error)),
    }
    // The run is built beside its final name and published by one rename, so
    // an interrupted create never leaves a half-made run under the run id.
    let staging = new_staging_directory(path)?;
    set_private_directory_permissions(&staging)?;
    ensure_private_directory(&staging.join(HISTORY_DIRECTORY))?;
    Ok(staging)
}

/// Create a private sibling of `path` for building a run. A name taken by an
/// earlier or a live creator (the pid can be reused and the counter restarts
/// in every process) is skipped, never reused or removed.
fn new_staging_directory(path: &Path) -> Result<PathBuf, RunStoreError> {
    for _ in 0..1024 {
        let sequence = TEMPORARY_FILE_SEQUENCE.fetch_add(1, Ordering::Relaxed);
        let staging = path.with_file_name(format!(".creating-{}-{sequence}", std::process::id()));
        match fs::create_dir(&staging) {
            Ok(()) => return Ok(staging),
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {}
            Err(error) => return Err(io_error("create run directory", &staging, error)),
        }
    }
    Err(RunStoreError::InvalidStateComponent {
        path: path.to_path_buf(),
    })
}

/// Publish a fully built run directory under its final name. An existing run
/// (or any other entry at that name) is never replaced.
pub(super) fn publish_run_directory(staging: &Path, path: &Path) -> Result<(), RunStoreError> {
    publish_after_check(staging, path, || {})
}

/// `after_check` runs between the early existence check and the rename, so a
/// test can create the destination in that window.
pub(super) fn publish_after_check(
    staging: &Path,
    path: &Path,
    after_check: impl FnOnce(),
) -> Result<(), RunStoreError> {
    match fs::symlink_metadata(path) {
        Ok(_) => return Err(RunStoreError::RunAlreadyExists),
        Err(error) if error.kind() == io::ErrorKind::NotFound => {}
        Err(error) => return Err(io_error("inspect run directory", path, error)),
    }
    after_check();
    rename_no_replace(staging, path).map_err(|error| {
        if matches!(
            error.kind(),
            io::ErrorKind::AlreadyExists | io::ErrorKind::DirectoryNotEmpty
        ) {
            RunStoreError::RunAlreadyExists
        } else {
            io_error("publish run directory", path, error)
        }
    })?;
    path.parent().map_or(Ok(()), sync_directory)
}

/// Rename that fails with `AlreadyExists` instead of replacing an existing
/// entry, atomically (renameat2 with RENAME_NOREPLACE). There is no fallback:
/// a check followed by a plain rename leaves a window in which another
/// process's entry would be replaced, so a system without the flag fails with
/// `ErrorKind::Unsupported`.
#[cfg(target_os = "linux")]
fn rename_no_replace(from: &Path, to: &Path) -> io::Result<()> {
    use rustix::fs::{CWD, RenameFlags, renameat_with};

    rename_no_replace_via(from, to, |from, to| {
        renameat_with(CWD, from, CWD, to, RenameFlags::NOREPLACE)
    })
}

/// Map the outcome of the atomic rename. `EINVAL` and `ENOSYS` mean the
/// kernel or filesystem lacks RENAME_NOREPLACE.
#[cfg(target_os = "linux")]
pub(super) fn rename_no_replace_via(
    from: &Path,
    to: &Path,
    atomic: impl FnOnce(&Path, &Path) -> Result<(), rustix::io::Errno>,
) -> io::Result<()> {
    use rustix::io::Errno;

    atomic(from, to).map_err(|errno| match errno {
        Errno::INVAL | Errno::NOSYS => io::Error::new(
            io::ErrorKind::Unsupported,
            "this filesystem does not support an atomic no-replace rename",
        ),
        other => other.into(),
    })
}

#[cfg(not(target_os = "linux"))]
fn rename_no_replace(_from: &Path, _to: &Path) -> io::Result<()> {
    Err(io::Error::new(
        io::ErrorKind::Unsupported,
        "an atomic no-replace rename is only implemented on Linux",
    ))
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

/// Atomic saves create private levels, preserving existing modes/traverse ACLs.
pub fn ensure_save_directory(path: &Path) -> Result<(), RunStoreError> {
    if let Some(parent) = path.parent()
        && parent != path
        && !parent.as_os_str().is_empty()
    {
        ensure_save_directory(parent)?;
    }
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_symlink() => {
            Err(RunStoreError::SymlinkRefused { path: path.into() })
        }
        Ok(metadata) if metadata.is_dir() => Ok(()),
        Ok(_) => Err(RunStoreError::InvalidStateComponent { path: path.into() }),
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            let mut builder = fs::DirBuilder::new();
            #[cfg(unix)]
            {
                use std::os::unix::fs::DirBuilderExt;
                builder.mode(0o700);
            }
            match builder.create(path) {
                Ok(()) => Ok(()),
                Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {
                    ensure_save_directory(path)
                }
                Err(error) => Err(io_error("create private save directory", path, error)),
            }
        }
        Err(error) => Err(io_error("inspect private save directory", path, error)),
    }
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
