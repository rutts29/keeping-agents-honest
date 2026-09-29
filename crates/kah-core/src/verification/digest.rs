use super::*;

pub(crate) fn verifier_digest(
    candidate: &Path,
    command: &[OsString],
) -> Result<String, VerificationError> {
    let mut hasher = Sha256::new();
    hasher.update(b"driftctl.verifier.v1\0");
    for argument in command {
        update_os_hash(&mut hasher, argument);
        let path = Path::new(argument);
        if !path.is_absolute() || !path.exists() {
            continue;
        }
        let canonical = path
            .canonicalize()
            .map_err(|error| VerificationError::io("canonicalize verifier input", error))?;
        if canonical.starts_with(candidate) {
            return Err(VerificationError::invalid(
                "VERIFICATION_BOUNDARY",
                "verifier inputs must remain outside the candidate",
            ));
        }
        let metadata = fs::symlink_metadata(&canonical)
            .map_err(|error| VerificationError::io("inspect verifier input", error))?;
        if metadata.file_type().is_file() {
            update_hash(
                &mut hasher,
                &fs::read(&canonical)
                    .map_err(|error| VerificationError::io("read verifier input", error))?,
            );
        }
    }
    Ok(format!("sha256:{:x}", hasher.finalize()))
}

pub(crate) fn candidate_digest(candidate: &Path) -> Result<String, VerificationError> {
    let output = Command::new("git")
        .current_dir(candidate)
        .args([
            "ls-files",
            "-z",
            "--cached",
            "--others",
            "--exclude-standard",
        ])
        .output()
        .map_err(|error| VerificationError::io("list candidate files", error))?;
    if !output.status.success() {
        return Err(VerificationError::invalid(
            "VERIFICATION_GIT",
            "candidate is not a readable Git checkpoint",
        ));
    }
    let mut paths = output
        .stdout
        .split(|byte| *byte == 0)
        .filter(|path| !path.is_empty())
        .map(|path| {
            std::str::from_utf8(path).map(PathBuf::from).map_err(|_| {
                VerificationError::invalid(
                    "VERIFICATION_PATH_ENCODING",
                    "candidate path is not UTF-8",
                )
            })
        })
        .collect::<Result<Vec<_>, _>>()?;
    paths.sort();
    paths.dedup();
    let mut hasher = Sha256::new();
    hasher.update(b"driftctl.candidate.v1\0");
    for relative in paths {
        hash_candidate_path(&mut hasher, candidate, &relative)?;
    }
    Ok(format!("sha256:{:x}", hasher.finalize()))
}

pub(crate) fn command_digest(command: &[OsString]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(b"driftctl.command.v1\0");
    for argument in command {
        update_os_hash(&mut hasher, argument);
    }
    format!("sha256:{:x}", hasher.finalize())
}

pub(super) fn digest_bytes(bytes: &[u8]) -> String {
    format!("sha256:{:x}", Sha256::digest(bytes))
}

pub(super) fn update_hash(hasher: &mut Sha256, bytes: &[u8]) {
    hasher.update(bytes.len().to_be_bytes());
    hasher.update(bytes);
}

#[cfg(unix)]
pub(super) fn update_os_hash(hasher: &mut Sha256, value: &OsStr) {
    use std::os::unix::ffi::OsStrExt as _;
    update_hash(hasher, value.as_bytes());
}

#[cfg(not(unix))]
pub(super) fn update_os_hash(hasher: &mut Sha256, value: &OsStr) {
    update_hash(hasher, value.to_string_lossy().as_bytes());
}

fn hash_candidate_path(
    hasher: &mut Sha256,
    candidate: &Path,
    relative: &Path,
) -> Result<(), VerificationError> {
    if relative.is_absolute()
        || relative
            .components()
            .any(|component| !matches!(component, std::path::Component::Normal(_)))
    {
        return Err(VerificationError::invalid(
            "VERIFICATION_PATH",
            "unsafe candidate path",
        ));
    }
    update_os_hash(hasher, relative.as_os_str());
    let path = candidate.join(relative);
    let metadata = fs::symlink_metadata(&path)
        .map_err(|error| VerificationError::io("inspect candidate file", error))?;
    if metadata.file_type().is_symlink() {
        update_os_hash(
            hasher,
            fs::read_link(&path)
                .map_err(|error| VerificationError::io("read candidate symlink", error))?
                .as_os_str(),
        );
    } else if metadata.file_type().is_file() {
        update_hash(
            hasher,
            &fs::read(&path)
                .map_err(|error| VerificationError::io("read candidate file", error))?,
        );
    } else {
        return Err(VerificationError::invalid(
            "VERIFICATION_FILE_TYPE",
            "candidate contains an unsupported file type",
        ));
    }
    Ok(())
}
