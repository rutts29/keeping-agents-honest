use super::*;

pub(super) fn create_private_directory(path: &Path) -> Result<(), EnrollmentError> {
    if let Ok(metadata) = fs::symlink_metadata(path) {
        if metadata.file_type().is_symlink() || !metadata.is_dir() {
            return Err(EnrollmentError::Invalid(
                "Driftctl state path must be a private directory, not a symlink",
            ));
        }
        set_private_directory_mode(path)?;
        return Ok(());
    }
    fs::create_dir_all(path)
        .map_err(|error| io_error("create private state directory", path, error))?;
    set_private_directory_mode(path)
}

pub(super) fn write_private_new(path: &Path, bytes: &[u8]) -> Result<(), EnrollmentError> {
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt as _;
        options.mode(0o600);
    }
    let mut file = options
        .open(path)
        .map_err(|error| io_error("create private enrollment", path, error))?;
    file.write_all(bytes)
        .map_err(|error| io_error("write private enrollment", path, error))?;
    file.sync_all()
        .map_err(|error| io_error("sync private enrollment", path, error))
}

pub(super) fn ensure_private_regular(
    path: &Path,
    metadata: &fs::Metadata,
) -> Result<(), EnrollmentError> {
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return Err(EnrollmentError::Invalid(
            "enrollment state must be a private regular file",
        ));
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        if metadata.permissions().mode() & 0o077 != 0 {
            return Err(EnrollmentError::Invalid(
                "enrollment state is readable or writable by another user",
            ));
        }
    }
    let _ = path;
    Ok(())
}

pub(super) fn set_private_directory_mode(path: &Path) -> Result<(), EnrollmentError> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        fs::set_permissions(path, fs::Permissions::from_mode(0o700))
            .map_err(|error| io_error("set private directory permissions", path, error))?;
    }
    Ok(())
}

pub(super) fn sync_directory(path: &Path) -> Result<(), EnrollmentError> {
    File::open(path)
        .and_then(|directory| directory.sync_all())
        .map_err(|error| io_error("sync state directory", path, error))
}

pub(super) fn io_error(
    action: &'static str,
    path: impl AsRef<Path>,
    error: io::Error,
) -> EnrollmentError {
    EnrollmentError::Io {
        action,
        path: path.as_ref().to_owned(),
        message: error.to_string(),
    }
}
