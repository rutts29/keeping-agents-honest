use crate::{Domain, Error};
use std::path::{Component, Path, PathBuf};

#[derive(Debug)]
pub struct Project {
    pub domain: Domain,
    pub root: PathBuf,
    pub relative_cwd: PathBuf,
    pub needs_creation: bool,
}

impl Project {
    pub fn select(
        repos: &Path,
        cwd: &Path,
        explicit: Option<(Domain, &str)>,
    ) -> Result<Self, Error> {
        if let Some((domain, name)) = explicit {
            validate_name(name)?;
            return resolve(domain, &repos.join(domain.as_str()), Path::new(name), true);
        }
        for domain in Domain::ALL {
            let directory = repos.join(domain.as_str());
            if let Ok(relative) = cwd.strip_prefix(&directory) {
                return resolve(domain, &directory, relative, false);
            }
        }
        Err(Error::new(
            "DOMAIN_REQUIRED",
            "project",
            "choose a domain and project",
        ))
    }
}

fn validate_name(name: &str) -> Result<(), Error> {
    let bytes = name.as_bytes();
    let valid = bytes.first().is_some_and(u8::is_ascii_alphanumeric)
        && bytes.len() <= 64
        && bytes
            .iter()
            .all(|b| b.is_ascii_alphanumeric() || b"._-".contains(b));
    if valid {
        Ok(())
    } else {
        Err(Error::new(
            "PROJECT_NAME",
            "project",
            "use 1–64 ASCII letters, digits, dots, underscores or hyphens; start with a letter or digit",
        ))
    }
}

fn canonical(path: &Path) -> Result<PathBuf, Error> {
    path.canonicalize().map_err(|e| {
        Error::new(
            "PROJECT_PATH",
            "project",
            format!("{}: {e}", path.display()),
        )
    })
}

fn resolve(
    domain: Domain,
    directory: &Path,
    relative: &Path,
    create: bool,
) -> Result<Project, Error> {
    let mut components = relative.components();
    let Some(Component::Normal(name)) = components.next() else {
        return Err(Error::new(
            "PROJECT_REQUIRED",
            "project",
            "a trust root is not a project",
        ));
    };
    let remainder = components.as_path();
    if remainder
        .components()
        .any(|c| !matches!(c, Component::Normal(_)))
    {
        return Err(Error::new(
            "PROJECT_ESCAPE",
            "project",
            "relative traversal is not a project path",
        ));
    }
    let directory = domain_root(directory)?;
    let candidate = directory.join(name);
    let needs_creation = create
        && !candidate
            .try_exists()
            .map_err(|e| Error::new("PROJECT_PATH", "project", e))?;
    let root = if needs_creation {
        candidate
    } else {
        canonical(&candidate)?
    };
    // Aliases may name another direct child of this domain, never another domain.
    if root.parent() != Some(directory.as_path()) {
        return Err(Error::new(
            "PROJECT_ESCAPE",
            "project",
            "project resolves outside its domain",
        ));
    }
    let actual_cwd = if needs_creation {
        root.clone()
    } else {
        canonical(&root.join(remainder))?
    };
    let relative_cwd = actual_cwd
        .strip_prefix(&root)
        .map_err(|_| Error::new("PROJECT_ESCAPE", "project", "cwd resolves outside project"))?
        .to_owned();
    if !needs_creation && !actual_cwd.is_dir() {
        return Err(Error::new(
            "PROJECT_DIRECTORY",
            "project",
            "project cwd must be a directory",
        ));
    }
    Ok(Project {
        domain,
        root,
        relative_cwd,
        needs_creation,
    })
}

fn domain_root(directory: &Path) -> Result<PathBuf, Error> {
    let resolved = canonical(directory)?;
    if resolved != directory {
        return Err(Error::new(
            "DOMAIN_ALIAS",
            "project",
            "trust roots must be canonical directories, not aliases",
        ));
    }
    Ok(resolved)
}
