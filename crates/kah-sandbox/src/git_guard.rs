//! Keep Git's executable configuration host-owned while ordinary Git state stays writable.
use crate::{Error, Executor};
use std::{
    collections::BTreeSet,
    fs,
    io::{Read, Write},
    os::unix::fs::{MetadataExt, OpenOptionsExt},
    path::{Path, PathBuf},
};

pub fn protect(
    mut executor: Executor,
    project: &Path,
    extra: Option<&Path>,
) -> Result<Executor, Error> {
    let dot = project.join(".git");
    let metadata = match fs::symlink_metadata(&dot) {
        Ok(value) => value,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(executor),
        Err(e) => return Err(failure(e)),
    };
    let mut readonly = BTreeSet::new();
    let admin = if metadata.is_dir() {
        dot.clone()
    } else if metadata.is_file() {
        ensure_file(&dot, "")?;
        let value = read(&dot)?;
        let value = value
            .strip_prefix("gitdir:")
            .ok_or_else(|| failure("invalid .git pointer"))?
            .trim();
        readonly.insert(dot.clone());
        resolve(project, value)?
    } else {
        return Err(failure(".git must be a regular directory or pointer file"));
    };
    allowed(&admin, project, extra)?;
    let common = if admin.join("commondir").exists() {
        resolve(&admin, read(&admin.join("commondir"))?.trim())?
    } else {
        admin.clone()
    };
    allowed(&common, project, extra)?;
    for directory in BTreeSet::from([admin, common]) {
        mount_directory(&mut executor, &directory, &mut readonly)?;
    }
    executor.with_read_only_mounts(
        &readonly
            .into_iter()
            .map(|p| (p.clone(), p))
            .collect::<Vec<_>>(),
    )
}
fn mount_directory(
    executor: &mut Executor,
    directory: &Path,
    readonly: &mut BTreeSet<PathBuf>,
) -> Result<(), Error> {
    // Pin metadata too: replacing/renaming its parent cannot substitute a
    // writable config underneath the protected file mounts.
    executor.add_writable_mount(directory, directory)?;
    for (name, content) in [
        ("config", ""),
        ("config.worktree", ""),
        ("commondir", ".\n"),
    ] {
        let path = directory.join(name);
        ensure_file(&path, content)?;
        readonly.insert(path);
    }
    let hooks = directory.join("hooks");
    if !hooks.exists() {
        fs::create_dir(&hooks).map_err(failure)?;
    }
    if !fs::symlink_metadata(&hooks).map_err(failure)?.is_dir() {
        return Err(failure(
            "hooks must be a real directory; manage hook configuration on the host",
        ));
    }
    readonly.insert(hooks);
    if directory.join("gitdir").exists() {
        ensure_file(&directory.join("gitdir"), "")?;
        readonly.insert(directory.join("gitdir"));
    }
    Ok(())
}
fn ensure_file(path: &Path, initial: &str) -> Result<(), Error> {
    match fs::OpenOptions::new()
        .create_new(true)
        .write(true)
        .mode(0o644)
        .open(path)
    {
        Ok(mut file) => file.write_all(initial.as_bytes()).map_err(failure)?,
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => (),
        Err(e) => return Err(failure(e)),
    }
    let metadata = fs::symlink_metadata(path).map_err(failure)?;
    if !metadata.is_file() || metadata.nlink() != 1 {
        return Err(failure(
            "Git control files must be regular files without aliases",
        ));
    }
    Ok(())
}
fn read(path: &Path) -> Result<String, Error> {
    let mut value = String::new();
    fs::File::open(path)
        .map_err(failure)?
        .take(4097)
        .read_to_string(&mut value)
        .map_err(failure)?;
    if value.len() > 4096 || value.trim().is_empty() {
        return Err(failure("invalid Git directory pointer"));
    }
    Ok(value)
}
fn resolve(base: &Path, path: &str) -> Result<PathBuf, Error> {
    base.join(path).canonicalize().map_err(failure)
}
fn allowed(path: &Path, project: &Path, extra: Option<&Path>) -> Result<(), Error> {
    if !path.starts_with(project) && !extra.is_some_and(|root| path.starts_with(root)) {
        return Err(failure(
            "Git metadata leaves the project; use an ordinary clone or a kah-managed worktree",
        ));
    }
    Ok(())
}
fn failure(error: impl ToString) -> Error {
    Error::new("GIT_CONTROL_BOUNDARY", "git_boundary", error)
}
