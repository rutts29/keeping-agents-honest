use crate::{Domain, Error, Project};
use std::{
    fs::{DirBuilder, OpenOptions},
    io::Write,
    os::unix::fs::{DirBuilderExt, OpenOptionsExt, symlink},
    path::Path,
    process::{Command, Output},
};

const GUIDANCE: &str = include_str!("../../../assets/sandbox/cx-project-guidance.md");

impl Project {
    /// Reserve the new directory exclusively. A failed initialization is left
    /// visible for repair; it never deletes or overwrites another project.
    pub fn initialize(&self) -> Result<(), Error> {
        let mode = if self.domain == Domain::Mine {
            0o700
        } else {
            0o2770
        };
        DirBuilder::new()
            .mode(mode)
            .create(&self.root)
            .map_err(|e| {
                Error::new(
                    if e.kind() == std::io::ErrorKind::AlreadyExists {
                        "PROJECT_EXISTS"
                    } else {
                        "PROJECT_CREATE"
                    },
                    "project_init",
                    e,
                )
            })?;
        self.write_guidance()?;
        self.git(&["init", "--quiet", "--template=", "--initial-branch=main"])?;
        if self.domain != Domain::Mine {
            self.git(&["config", "core.sharedRepository", "group"])?;
        }
        self.copy_missing_identity(None)?;
        Ok(())
    }

    /// Sandboxed commands cannot see ~/.gitconfig. Copy only the user's global
    /// name and email into the project's local config, and only where the
    /// local config has none. Mine runs on the host and is left alone.
    /// Returns true when anything was written.
    pub fn ensure_identity(&self) -> Result<bool, Error> {
        if self.domain == Domain::Mine || !self.repository_root()? {
            return Ok(false);
        }
        self.copy_missing_identity(None)
    }

    /// Only a project's own repository gets identity. A plain directory (for
    /// example an observer workspace) has no local config, and a directory
    /// inside another repository must not edit that repository's config.
    fn repository_root(&self) -> Result<bool, Error> {
        let output = Command::new("git")
            .arg("-C")
            .arg(&self.root)
            .args(["rev-parse", "--show-toplevel"])
            .output()
            .map_err(init_error)?;
        if !output.status.success() {
            return Ok(false);
        }
        let top = String::from_utf8_lossy(&output.stdout);
        Ok(Path::new(top.trim()).canonicalize().ok() == self.root.canonicalize().ok())
    }

    fn copy_missing_identity(&self, global: Option<&Path>) -> Result<bool, Error> {
        let mut changed = false;
        for key in ["user.name", "user.email"] {
            let local = self.git(&["config", "--local", "--get", key]);
            if local.is_ok_and(|o| !o.stdout.trim_ascii().is_empty()) {
                continue;
            }
            let mut read = Command::new("git");
            if let Some(path) = global {
                read.env("GIT_CONFIG_GLOBAL", path);
            }
            let output = read
                .args(["config", "--global", "--get", key])
                .output()
                .map_err(init_error)?;
            let value = String::from_utf8_lossy(&output.stdout);
            if output.status.success() && !value.trim().is_empty() {
                self.git(&["config", "--local", key, value.trim()])?;
                changed = true;
            }
        }
        Ok(changed)
    }

    #[doc(hidden)]
    pub fn ensure_identity_from(&self, global: &Path) -> Result<bool, Error> {
        if self.domain == Domain::Mine || !self.repository_root()? {
            return Ok(false);
        }
        self.copy_missing_identity(Some(global))
    }

    fn write_guidance(&self) -> Result<(), Error> {
        let mut file = OpenOptions::new()
            .create_new(true)
            .write(true)
            .mode(if self.domain == Domain::Mine {
                0o600
            } else {
                0o660
            })
            .open(self.root.join("AGENTS.md"))
            .map_err(init_error)?;
        file.write_all(GUIDANCE.as_bytes()).map_err(init_error)?;
        symlink("AGENTS.md", self.root.join("CLAUDE.md")).map_err(init_error)?;
        Ok(())
    }

    fn git(&self, args: &[&str]) -> Result<Output, Error> {
        let output = Command::new("git")
            .arg("-C")
            .arg(&self.root)
            .args([
                "-c",
                "core.hooksPath=/dev/null",
                "-c",
                "commit.gpgsign=false",
            ])
            .args(args)
            .output()
            .map_err(init_error)?;
        if !output.status.success() {
            return Err(Error::new(
                "PROJECT_GIT",
                "project_init",
                String::from_utf8_lossy(&output.stderr),
            ));
        }
        Ok(output)
    }
}

fn init_error(error: std::io::Error) -> Error {
    Error::new("PROJECT_INIT_IO", "project_init", error)
}
